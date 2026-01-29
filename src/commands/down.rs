use anyhow::Result;
use std::path::PathBuf;

use crate::config;
use crate::hooks::{self, HookContext};
use crate::lima::client as lima;
use crate::lima::ssh;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>, stop_vm: bool) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?.clone();
    let instance_name = instance.name.clone();

    println!("=== Stopping worktree: {} ===", instance_name);

    let worktree_path = PathBuf::from(&instance.path);
    let config = config::load_config(&main_repo, Some(&worktree_path))?;
    let compose_base = utils::compose_base_path(&config, &worktree_path);

    if !compose_base.exists() {
        anyhow::bail!("Compose base not found: {}", compose_base.display());
    }

    let hook_ctx = HookContext {
        name: instance_name.clone(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset: 0,
        compose_base: compose_base.clone(),
        compose_file: compose_base.clone(),
    };

    hooks::run_hook("pre_down", &worktree_path, &hook_ctx, &config)?;

    // Lima VM が起動しているか確認
    let info = lima::info(&instance.lima_instance)?;
    if info == lima::InstanceStatus::Running {
        // compose ファイルの相対パスを取得
        let compose_rel = compose_base
            .strip_prefix(&worktree_path)
            .unwrap_or(&compose_base);
        let compose_path = compose_rel.to_string_lossy();
        let vm_worktree_path = worktree_path.to_string_lossy();

        // VM 内で docker compose down を実行
        println!("Running docker compose down in VM...");
        let output = lima::shell(
            &instance.lima_instance,
            &[
                "bash",
                "-c",
                &format!(
                    "cd '{}' && sudo docker compose -f '{}' down",
                    vm_worktree_path, compose_path
                ),
            ],
        )?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("docker compose down failed: {}", stderr.trim());
        }

        println!("{}", String::from_utf8_lossy(&output.stdout));

        // アクティブなポートフォワードを停止
        if !instance.active_forwards.is_empty() {
            println!("Stopping port forwards...");
            for fwd in &instance.active_forwards {
                if ssh::is_process_alive(fwd.pid) {
                    if let Err(e) = ssh::stop_forward(fwd.pid) {
                        eprintln!("Warning: Failed to stop forward on port {}: {}", fwd.local_port, e);
                    }
                }
            }
            // 状態をクリア
            state.clear_forwards(&instance_name)?;
            state.save(&main_repo)?;
        }

        // --vm オプションが指定された場合は Lima VM も停止
        if stop_vm {
            println!("Stopping Lima VM: {}...", instance.lima_instance);
            lima::stop(&instance.lima_instance)?;
            println!("Lima VM stopped.");
        }
    } else {
        println!("Lima VM '{}' is not running.", instance.lima_instance);
    }

    hooks::run_hook("post_down", &worktree_path, &hook_ctx, &config)?;

    println!("=== docker compose down completed ===");

    Ok(())
}
