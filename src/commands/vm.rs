use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::config;
use crate::lima::client as lima;
use crate::lima::{ssh, template};
use crate::state::{Instance, State};
use crate::utils;

/// 現在の worktree に Lima VM を追加する
pub fn add_vm(name: Option<&str>) -> Result<()> {
    if !lima::is_available() {
        anyhow::bail!("Lima is not installed. Please install lima first: brew install lima");
    }

    let main_repo = utils::resolve_main_repo()?;
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let mut state = State::load(&main_repo)?;

    // fracta 管理下の instance を探す
    let existing = state.instances.iter().find(|inst| {
        utils::is_path_within(std::path::Path::new(&inst.path), &cwd)
    });

    let (instance_name, worktree_path) = if let Some(inst) = existing {
        // 既に VM が紐付いている場合はエラー
        if !inst.lima_instance.is_empty() {
            let info = lima::info(&inst.lima_instance)?;
            if info != lima::InstanceStatus::NotFound {
                anyhow::bail!(
                    "Instance '{}' already has Lima VM '{}'",
                    inst.name,
                    inst.lima_instance
                );
            }
        }
        (inst.name.clone(), PathBuf::from(&inst.path))
    } else {
        // fracta 管理外の worktree → 新規登録
        let dir_name = cwd
            .file_name()
            .and_then(|n| n.to_str())
            .context("Failed to get directory name")?
            .to_string();
        let instance_name = name.unwrap_or(&dir_name).to_string();

        if state.find_instance(&instance_name).is_some() {
            anyhow::bail!("Instance '{}' already exists in state", instance_name);
        }

        // git worktree かどうか確認
        let output = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .current_dir(&cwd)
            .output()
            .context("Failed to check git worktree")?;
        if !output.status.success() {
            anyhow::bail!("Current directory is not a git repository");
        }

        let branch = {
            let output = Command::new("git")
                .args(["rev-parse", "--abbrev-ref", "HEAD"])
                .current_dir(&cwd)
                .output()?;
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        let instance = Instance {
            name: instance_name.clone(),
            path: cwd.to_string_lossy().to_string(),
            branch,
            lima_instance: String::new(),
            active_forwards: Vec::new(),
            active_proxy: None,
            active_browser: None,
        };
        state.add_instance(instance);
        (instance_name, cwd.clone())
    };

    let lima_instance = lima::instance_name(&instance_name);

    // Lima インスタンスが既に存在するか確認
    let info = lima::info(&lima_instance)?;
    if info != lima::InstanceStatus::NotFound {
        anyhow::bail!(
            "Lima instance '{}' already exists. Remove it first with: limactl delete {}",
            lima_instance,
            lima_instance
        );
    }

    let config = config::load_config(&main_repo, Some(&worktree_path))?;

    // Lima テンプレートを生成
    println!("Creating Lima VM template...");
    let mut template_config = template::TemplateConfig::new(
        &worktree_path.to_string_lossy(),
        config.vm_mount_type.as_deref(),
        config.vm_user.as_deref(),
    );
    template_config.resolve_template(config.vm_template.as_deref(), &main_repo, &worktree_path);
    if let Some(scripts) = &config.vm_provision_scripts {
        template_config.load_provision_scripts(scripts, &main_repo)?;
    }
    let temp_template = template::create_temp_template(&template_config)?;

    // Lima VM を作成
    println!("Creating Lima VM: {}...", lima_instance);
    lima::create(temp_template.path(), &lima_instance)?;

    // state を更新
    let inst = state
        .find_instance_mut(&instance_name)
        .context("Instance not found in state")?;
    inst.lima_instance = lima_instance.clone();
    state.save(&main_repo)?;

    println!("\n=== VM added successfully ===");
    println!("  Instance: {}", instance_name);
    println!("  Lima VM:  {}", lima_instance);
    println!("  Path:     {}", worktree_path.display());
    println!("\nNext steps:");
    println!("  fracta up        - Start VM and docker compose");
    println!("  fracta vm shell  - Connect to VM shell");

    Ok(())
}

pub fn start(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;
    let instance = state.resolve_instance(name)?;

    let info = lima::info(&instance.lima_instance)?;
    match info {
        lima::InstanceStatus::NotFound => {
            anyhow::bail!(
                "Lima VM '{}' not found. Run 'fracta add {}' first.",
                instance.lima_instance,
                instance.name
            );
        }
        lima::InstanceStatus::Running => {
            println!("Lima VM '{}' is already running.", instance.lima_instance);
        }
        lima::InstanceStatus::Stopped => {
            println!("Starting Lima VM: {}...", instance.lima_instance);
            lima::start(&instance.lima_instance)?;
            println!("Lima VM started.");
        }
    }

    Ok(())
}

pub fn stop(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;
    let instance = state.resolve_instance(name)?.clone();
    let instance_name = instance.name.clone();

    // Stop local helper processes first so state/ports stay consistent.
    for forward in &instance.active_forwards {
        if ssh::is_process_alive(forward.pid) {
            if let Err(e) = ssh::stop_forward(forward.pid) {
                eprintln!(
                    "Warning: Failed to stop forward PID {} (localhost:{}): {}",
                    forward.pid, forward.local_port, e
                );
            }
        }
    }
    state.clear_forwards(&instance_name)?;

    if let Some(proxy) = &instance.active_proxy {
        if ssh::is_process_alive(proxy.pid) {
            if let Err(e) = ssh::stop_forward(proxy.pid) {
                eprintln!(
                    "Warning: Failed to stop SOCKS5 proxy PID {} (localhost:{}): {}",
                    proxy.pid, proxy.local_port, e
                );
            }
        }
    }
    state.remove_proxy(&instance_name)?;

    if let Some(browser) = &instance.active_browser {
        if ssh::is_process_alive(browser.pid) {
            if let Err(e) = ssh::stop_forward(browser.pid) {
                eprintln!(
                    "Warning: Failed to stop Playwright PID {}: {}",
                    browser.pid, e
                );
            }
        }
    }
    state.remove_browser(&instance_name)?;
    state.save(&main_repo)?;

    let info = lima::info(&instance.lima_instance)?;
    if info == lima::InstanceStatus::Running {
        println!("Stopping Lima VM: {}...", instance.lima_instance);
        lima::stop(&instance.lima_instance)?;
        println!("Lima VM stopped.");
    } else {
        println!("Lima VM '{}' is not running.", instance.lima_instance);
    }

    Ok(())
}

pub fn shell(
    name: Option<&str>,
    shell: Option<&str>,
    workdir: Option<&str>,
    tty: Option<bool>,
    command: &[String],
) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;
    let instance = state.resolve_instance(name)?;
    let name = instance.name.as_str();

    let info = lima::info(&instance.lima_instance)?;
    match info {
        lima::InstanceStatus::NotFound => {
            anyhow::bail!(
                "Lima VM '{}' not found. Run 'fracta add {}' first.",
                instance.lima_instance,
                name
            );
        }
        lima::InstanceStatus::Stopped => {
            anyhow::bail!(
                "Lima VM '{}' is not running. Start it with 'fracta vm start {}'.",
                instance.lima_instance,
                name
            );
        }
        lima::InstanceStatus::Running => {}
    }

    println!("Connecting to Lima VM: {}...", instance.lima_instance);
    println!("Worktree path: {}", instance.path);
    println!("---");

    let mut args: Vec<String> = vec!["shell".to_string()];
    if let Some(shell) = shell {
        args.push("--shell".to_string());
        args.push(shell.to_string());
    }
    if let Some(workdir) = workdir {
        args.push("--workdir".to_string());
        args.push(workdir.to_string());
    }
    if let Some(tty) = tty {
        args.push("--tty".to_string());
        args.push(tty.to_string());
    }
    args.push(instance.lima_instance.clone());
    if !command.is_empty() {
        args.extend(command.iter().cloned());
    }

    let status = Command::new("limactl")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        anyhow::bail!("Shell session ended with error");
    }

    Ok(())
}

pub fn list() -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    if state.instances.is_empty() {
        println!("No instances found.");
        return Ok(());
    }

    println!("=== VM Instances ===");
    println!(
        "{:<20} {:<25} {:<10} {}",
        "NAME", "LIMA VM", "VM STATUS", "PATH"
    );
    println!("{}", "-".repeat(90));

    for inst in &state.instances {
        let vm_status = match lima::info(&inst.lima_instance) {
            Ok(status) => status.to_string(),
            Err(_) => "Unknown".to_string(),
        };

        println!(
            "{:<20} {:<25} {:<10} {}",
            inst.name, inst.lima_instance, vm_status, inst.path
        );
    }

    Ok(())
}

/// デフォルトの Lima テンプレートを stdout に出力
pub fn template() -> Result<()> {
    let config = template::TemplateConfig::new("{{WORKTREE_PATH}}", None, None);
    let content = template::generate_default(&config);
    print!("{}", content);
    Ok(())
}
