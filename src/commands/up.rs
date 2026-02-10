use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::config;
use crate::hooks::{self, HookContext};
use crate::images;
use crate::lima::client as lima;
use crate::lima::template;
use crate::state::State;
use crate::utils;

pub fn execute(
    name: Option<&str>,
    no_sync_images: bool,
    no_parallel_build: bool,
    vm_build_copy: bool,
    vm_build_dir: Option<&str>,
) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?;
    let instance_name = instance.name.as_str();
    let project_name = utils::sanitize_name(instance_name);

    println!("=== Starting worktree: {} ===", instance_name);

    let worktree_path = PathBuf::from(&instance.path);
    let config = config::load_config(&main_repo, Some(&worktree_path))?;
    let compose_base = utils::compose_base_path(&config, &worktree_path);

    if !compose_base.exists() {
        anyhow::bail!("Compose base not found: {}", compose_base.display());
    }

    // Lima VM が起動しているか確認
    let info = lima::info(&instance.lima_instance)?;
    match info {
        lima::InstanceStatus::NotFound => {
            println!(
                "Lima VM '{}' not found. Creating a new VM...",
                instance.lima_instance
            );

            let tmpl_cfg = template::TemplateConfig::new(
                &worktree_path.to_string_lossy(),
                config.vm_mount_type.as_deref(),
                config.vm_user.as_deref(),
            );
            let temp_template = template::create_temp_template(&tmpl_cfg)?;

            lima::create(temp_template.path(), &instance.lima_instance)?;
            println!("Starting Lima VM: {}...", instance.lima_instance);
            lima::start(&instance.lima_instance)?;
        }
        lima::InstanceStatus::Stopped => {
            println!("Starting Lima VM: {}...", instance.lima_instance);
            lima::start(&instance.lima_instance)?;
        }
        lima::InstanceStatus::Running => {
            println!("Lima VM '{}' is already running.", instance.lima_instance);
        }
    }

    // compose ファイルの相対パスを取得
    let compose_rel = compose_base
        .strip_prefix(&worktree_path)
        .unwrap_or(&compose_base);

    let hook_ctx = HookContext {
        name: instance_name.to_string(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset: 0,
        compose_base: compose_base.clone(),
        compose_file: compose_base.clone(), // v2 では生成ファイル不使用
    };

    hooks::run_hook("pre_up", &worktree_path, &hook_ctx, &config)?;

    let parallel_build = if no_parallel_build {
        false
    } else {
        config.compose_parallel_build.unwrap_or(true)
    };

    let use_vm_build_copy = vm_build_copy || config.vm_build_copy.unwrap_or(false);
    let vm_build_root = vm_build_dir
        .map(|s| s.to_string())
        .or_else(|| config.vm_build_dir.clone())
        .unwrap_or_else(|| "/tmp/fracta-build".to_string());

    if use_vm_build_copy && !utils::is_path_within(&worktree_path, &compose_base) {
        anyhow::bail!(
            "vm_build_copy is enabled but compose_base is outside the worktree: {}",
            compose_base.display()
        );
    }

    let vm_worktree_path = if use_vm_build_copy {
        let dest = format!("{}/{}", vm_build_root.trim_end_matches('/'), project_name);
        println!("Syncing worktree to VM (local build copy)...");
        sync_worktree_to_vm(&instance.lima_instance, &worktree_path, &dest)?;
        dest
    } else {
        worktree_path.to_string_lossy().to_string()
    };

    if !no_sync_images {
        println!("Syncing images to VM...");
        let images = images::collect_compose_images(&compose_base, &worktree_path)?;
        if images.is_empty() {
            println!("No images found to sync.");
        } else {
            images::sync_images_to_vm(&instance.lima_instance, &images)?;
        }
    } else {
        println!("Image sync skipped (--no-sync-images).");
    }

    // Lima VM 内で docker compose up を実行
    println!("Running docker compose up in VM...");
    let compose_path = compose_rel.to_string_lossy();
    let env_prefix = compose_env_prefix(&project_name, parallel_build);
    let status = lima::shell_interactive(
        &instance.lima_instance,
        &[
            "bash",
            "-c",
            &format!(
                "cd '{}' && {}sudo docker compose -f '{}' up -d",
                vm_worktree_path, env_prefix, compose_path
            ),
        ],
    )?;

    if !status.success() {
        anyhow::bail!("docker compose up failed in VM");
    }

    hooks::run_hook("post_up", &worktree_path, &hook_ctx, &config)?;

    // コンテナの状態を表示
    println!("\nContainer status:");
    let _ = lima::shell_interactive(
        &instance.lima_instance,
        &[
            "bash",
            "-c",
            &format!(
                "cd '{}' && {}sudo docker compose -f '{}' ps",
                vm_worktree_path, env_prefix, compose_path
            ),
        ],
    );

    println!("\n=== docker compose up completed ===");
    println!(
        "Use 'fracta forward {} <local_port> <remote_port>' to access services",
        instance_name
    );

    Ok(())
}

fn compose_env_prefix(project_name: &str, parallel_build: bool) -> String {
    if parallel_build {
        format!("COMPOSE_PROJECT_NAME={} ", project_name)
    } else {
        format!("COMPOSE_PROJECT_NAME={} COMPOSE_PARALLEL_BUILD=0 ", project_name)
    }
}

fn sync_worktree_to_vm(instance_name: &str, src: &Path, dest: &str) -> Result<()> {
    let mut tar = Command::new("tar")
        .args(["-C", src.to_string_lossy().as_ref(), "--exclude", ".git", "-cf", "-", "."])
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to run tar")?;

    let tar_stdout = tar.stdout.take().context("Failed to capture tar output")?;

    let mut limactl = Command::new("limactl")
        .args([
            "shell",
            "--workdir",
            "/",
            instance_name,
            "--",
            "bash",
            "-lc",
            &format!("mkdir -p '{dest}' && tar -xf - -C '{dest}'"),
        ])
        .stdin(tar_stdout)
        .spawn()
        .context("Failed to run limactl shell for sync")?;

    let limactl_status = limactl
        .wait()
        .context("Failed to wait for limactl shell")?;
    let tar_status = tar.wait().context("Failed to wait for tar")?;

    if !tar_status.success() {
        anyhow::bail!("tar failed while syncing worktree to VM");
    }
    if !limactl_status.success() {
        anyhow::bail!("limactl shell failed while syncing worktree to VM");
    }

    Ok(())
}
