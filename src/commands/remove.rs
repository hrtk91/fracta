use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::config;
use crate::hooks::{self, HookContext};
use crate::lima::client as lima;
use crate::lima::ssh;
use crate::state::State;
use crate::utils;

pub fn execute(
    name: Option<&str>,
    force: bool,
    vm_only: bool,
    worktree_only: bool,
) -> Result<()> {
    if vm_only && worktree_only {
        anyhow::bail!("Cannot use --vm-only and --worktree-only together");
    }

    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?.clone();
    let name = instance.name.as_str();

    println!("=== Removing worktree: {} ===", name);

    let worktree_path = PathBuf::from(&instance.path);
    let config = config::load_config(&main_repo, Some(&worktree_path))?;
    let compose_base = utils::compose_base_path(&config, &worktree_path);

    let hook_ctx = HookContext {
        name: name.to_string(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset: 0,
        compose_base: compose_base.clone(),
        compose_file: compose_base.clone(),
    };

    hooks::run_hook("pre_remove", &worktree_path, &hook_ctx, &config)?;

    // アクティブなポートフォワードを停止
    if !instance.active_forwards.is_empty() {
        println!("Stopping port forwards...");
        for fwd in &instance.active_forwards {
            if ssh::is_process_alive(fwd.pid) {
                let _ = ssh::stop_forward(fwd.pid);
            }
        }
    }

    let remove_vm = !worktree_only;
    let remove_worktree = !vm_only;

    // Lima VM が起動している場合は docker compose down を実行
    let info = lima::info(&instance.lima_instance)?;
    if remove_vm && info == lima::InstanceStatus::Running {
        if compose_base.exists() {
            let compose_rel = compose_base
                .strip_prefix(&worktree_path)
                .unwrap_or(&compose_base);
            let compose_path = compose_rel.to_string_lossy();
            let vm_worktree_path = worktree_path.to_string_lossy();

            println!("Running docker compose down in VM...");
            let output = lima::shell(
                &instance.lima_instance,
                &[
                    "bash",
                    "-c",
                    &format!(
                        "cd '{}' && sudo docker compose -f '{}' down 2>/dev/null || true",
                        vm_worktree_path, compose_path
                    ),
                ],
            );

            if let Err(e) = output {
                if force {
                    eprintln!("Warning: docker compose down failed: {}", e);
                } else {
                    return Err(e);
                }
            }
        }
    }

    // Lima VM を削除
    if remove_vm && info != lima::InstanceStatus::NotFound {
        println!("Deleting Lima VM: {}...", instance.lima_instance);
        if let Err(e) = lima::delete(&instance.lima_instance) {
            if force {
                eprintln!("Warning: Failed to delete Lima VM: {}", e);
            } else {
                return Err(e);
            }
        }
    }

    hooks::run_hook("post_remove", &worktree_path, &hook_ctx, &config)?;

    if remove_worktree {
        // .fracta ディレクトリを削除
        let worktree_fracta_dir = utils::fracta_worktree_dir(&worktree_path);
        if worktree_fracta_dir.exists() {
            let _ = std::fs::remove_dir_all(&worktree_fracta_dir);
        }

        // Git worktree を削除
        println!("Removing git worktree...");
        let output = Command::new("git")
            .args(["worktree", "remove", "--force", worktree_path.to_string_lossy().as_ref()])
            .current_dir(&main_repo)
            .output()
            .context("Failed to execute git worktree remove")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if force {
                eprintln!("Warning: git worktree remove failed: {}", stderr.trim());
            } else {
                anyhow::bail!("git worktree remove failed: {}", stderr.trim());
            }
        }
    }

    if remove_vm && remove_worktree {
        // 状態を完全に削除
        state.remove_instance(name);
    } else if remove_vm {
        if let Some(inst) = state.find_instance_mut(name) {
            inst.active_forwards.clear();
            inst.active_proxy = None;
            inst.active_browser = None;
        }
    }
    state.save(&main_repo)?;

    if remove_vm && remove_worktree {
        println!("=== Worktree '{}' removed successfully ===", name);
    } else if remove_vm {
        println!("=== VM for '{}' removed successfully ===", name);
    } else {
        println!("=== Worktree '{}' removed successfully (VM preserved) ===", name);
    }

    Ok(())
}
