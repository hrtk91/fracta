use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::config;
use crate::lima::client as lima;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    let (instance_name, worktree_path, lima_instance, active_forwards) = match name {
        Some(name) => {
            let instance = state
                .find_instance(name)
                .ok_or_else(|| anyhow::anyhow!("Instance '{}' not found", name))?;
            (
                instance.name.as_str(),
                PathBuf::from(&instance.path),
                instance.lima_instance.clone(),
                instance.active_forwards.clone(),
            )
        }
        None => {
            let cwd = std::env::current_dir()
                .context("Failed to get current directory")?;
            let instance = state.instances.iter().find(|inst| {
                utils::is_path_within(std::path::Path::new(&inst.path), &cwd)
            });

            let instance = match instance {
                Some(inst) => inst,
                None => anyhow::bail!("No instance found for current directory"),
            };

            (
                instance.name.as_str(),
                PathBuf::from(&instance.path),
                instance.lima_instance.clone(),
                instance.active_forwards.clone(),
            )
        }
    };

    let config = config::load_config(&main_repo, Some(&worktree_path))?;
    let compose_base = utils::compose_base_path(&config, &worktree_path);

    // Lima VM の状態を確認
    let info = lima::info(&lima_instance)?;

    println!("=== Instance: {} ===", instance_name);
    println!("Lima VM: {} ({})", lima_instance, info);
    println!("Worktree: {}", worktree_path.display());

    // アクティブなポートフォワード
    if !active_forwards.is_empty() {
        println!("\nActive port forwards:");
        println!("{:<12} {:<12} {:<10}", "LOCAL", "REMOTE", "PID");
        println!("{}", "-".repeat(34));
        for fwd in &active_forwards {
            println!(
                "{:<12} {:<12} {:<10}",
                fwd.local_port, fwd.remote_port, fwd.pid
            );
        }
    }

    // VM が起動している場合のみ docker compose ps を実行
    if info == lima::InstanceStatus::Running {
        if compose_base.exists() {
            let compose_rel = compose_base
                .strip_prefix(&worktree_path)
                .unwrap_or(&compose_base);
            let compose_path = compose_rel.to_string_lossy();
            let vm_worktree_path = worktree_path.to_string_lossy();
            let project_name = utils::sanitize_name(instance_name);
            let env_prefix = format!("COMPOSE_PROJECT_NAME={} ", project_name);

            println!("\n=== Docker Compose Status ===");

            let _ = lima::shell_interactive(
                &lima_instance,
                &[
                    "bash",
                    "-c",
                    &format!(
                    "cd '{}' && {}sudo docker compose -f '{}' ps",
                    vm_worktree_path, env_prefix, compose_path
                ),
            ],
        );
        } else {
            println!("\nCompose base not found: {}", compose_base.display());
        }
    } else {
        println!("\nLima VM is not running. Start it with: fracta up {}", instance_name);
    }

    Ok(())
}
