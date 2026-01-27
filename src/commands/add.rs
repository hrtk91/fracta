use anyhow::{Context, Result};
use std::process::Command;

use crate::config;
use crate::hooks::{self, HookContext};
use crate::lima::{client as lima, template};
use crate::state::{Instance, State};
use crate::utils;

pub fn execute(name: &str, base_branch: Option<Option<String>>) -> Result<()> {
    println!("=== Adding worktree: {} ===", name);

    // Lima が利用可能か確認
    if !lima::is_available() {
        anyhow::bail!("Lima is not installed. Please install lima first: brew install lima");
    }

    let main_repo = utils::resolve_main_repo()?;
    let config = config::load_config(&main_repo)?;

    let mut state = State::load(&main_repo)?;
    if state.find_instance(name).is_some() {
        anyhow::bail!("Instance '{}' already exists", name);
    }

    let repo_name = main_repo
        .file_name()
        .and_then(|n| n.to_str())
        .context("Failed to get repository name")?;

    // ディレクトリ名として使用するため、name をサニタイズ
    let sanitized_name = utils::sanitize_name(name);
    let worktree_path = main_repo
        .parent()
        .context("Failed to get parent directory")?
        .join(format!("{}-{}", repo_name, sanitized_name));

    let lima_instance = lima::instance_name(name);

    // Lima インスタンスが既に存在するか確認
    let info = lima::info(&lima_instance)?;
    if info != lima::InstanceStatus::NotFound {
        anyhow::bail!(
            "Lima instance '{}' already exists. Remove it first with: limactl delete {}",
            lima_instance,
            lima_instance
        );
    }

    let compose_base = utils::compose_base_path(&config, &worktree_path);
    let compose_file = utils::compose_generated_path(&worktree_path);

    let hook_ctx = HookContext {
        name: name.to_string(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset: 0, // v2 では不使用
        compose_base: compose_base.clone(),
        compose_file: compose_file.clone(),
    };

    hooks::run_hook("pre_add", &main_repo, &hook_ctx)?;

    // Git worktree を作成
    let mut git_args = vec!["worktree".to_string(), "add".to_string()];
    if let Some(base) = &base_branch {
        git_args.push("-b".to_string());
        git_args.push(name.to_string());
        git_args.push(worktree_path.to_string_lossy().to_string());
        if let Some(base_name) = base {
            git_args.push(base_name.to_string());
        } else {
            git_args.push("HEAD".to_string());
        }
    } else {
        git_args.push(worktree_path.to_string_lossy().to_string());
        git_args.push(name.to_string());
    }

    println!("Creating git worktree...");
    let output = Command::new("git")
        .args(&git_args)
        .current_dir(&main_repo)
        .output()
        .context("Failed to execute git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr.trim());
    }

    // Lima テンプレートを生成
    println!("Creating Lima VM template...");
    let mut template_config = template::TemplateConfig::new(&worktree_path.to_string_lossy());
    if let Some(mirror) = config.registry_mirror() {
        template_config.registry_mirror = Some(mirror.to_string());
    }
    let temp_template = template::create_temp_template(&template_config)?;

    // Lima VM を作成
    println!("Creating Lima VM: {}...", lima_instance);
    if let Err(e) = lima::create(temp_template.path(), &lima_instance) {
        // 失敗した場合は worktree を削除
        eprintln!("Failed to create Lima VM, cleaning up worktree...");
        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", worktree_path.to_string_lossy().as_ref()])
            .current_dir(&main_repo)
            .output();
        return Err(e);
    }

    // 状態を保存
    let instance = Instance {
        name: name.to_string(),
        path: worktree_path.to_string_lossy().to_string(),
        branch: name.to_string(),
        lima_instance: lima_instance.clone(),
        active_forwards: Vec::new(),
        active_proxy: None,
        active_browser: None,
    };

    state.add_instance(instance);
    state.save(&main_repo)?;

    hooks::run_hook("post_add", &worktree_path, &hook_ctx)?;

    println!("\n=== Worktree added successfully ===");
    println!("  Worktree: {}", worktree_path.display());
    println!("  Lima VM:  {}", lima_instance);
    println!("\nNext steps:");
    println!("  fracta up {}     - Start VM and docker compose", name);
    println!("  fracta shell {}  - Connect to VM shell", name);

    Ok(())
}
