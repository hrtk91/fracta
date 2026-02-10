use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::config;
use crate::lima::client as lima;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>, short: bool) -> Result<()> {
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

    if short {
        // 短い形式: アクティブなフォワードのみ表示
        for fwd in &active_forwards {
            println!("{}\t{}", fwd.local_port, fwd.remote_port);
        }
        return Ok(());
    }

    println!("=== Ports: {} ===", instance_name);
    println!("Lima VM: {}", lima_instance);

    if active_forwards.is_empty() {
        println!("\nNo active port forwards.");
        println!("Use 'fracta forward {} <local_port> <remote_port>' to create one.", instance_name);
    } else {
        println!("\nActive port forwards:");
        println!("{:<12} {:<12} {:<10} {}", "LOCAL", "REMOTE", "PID", "ACCESS");
        println!("{}", "-".repeat(60));
        for fwd in &active_forwards {
            println!(
                "{:<12} {:<12} {:<10} http://localhost:{}",
                fwd.local_port, fwd.remote_port, fwd.pid, fwd.local_port
            );
        }
    }

    // Lima VM が起動している場合は、VM 内で公開されているポートを表示
    let info = lima::info(&lima_instance)?;
    if info == lima::InstanceStatus::Running {
        let config = config::load_config(&main_repo, Some(&worktree_path))?;
        let compose_base = utils::compose_base_path(&config, &worktree_path);
        if compose_base.exists() {
            let compose_rel = compose_base
                .strip_prefix(&worktree_path)
                .unwrap_or(&compose_base);
            let compose_path = compose_rel.to_string_lossy();
            let vm_worktree_path = worktree_path.to_string_lossy();
            let project_name = utils::sanitize_name(instance_name);
            let env_prefix = format!("COMPOSE_PROJECT_NAME={} ", project_name);

            println!("\nPorts exposed in VM:");
            let output = lima::shell(
                &lima_instance,
                &[
                    "bash",
                    "-c",
                    &format!(
                        "cd '{}' && {}sudo docker compose -f '{}' ps --format json 2>/dev/null | jq -r '.[] | select(.Publishers != null) | .Publishers[] | \"\\(.TargetPort)\\t\\(.PublishedPort)\"' 2>/dev/null || echo 'Run \"fracta up {}\" to see container ports'",
                        vm_worktree_path, env_prefix, compose_path, instance_name
                    ),
                ],
            )?;
            println!("{}", String::from_utf8_lossy(&output.stdout));
        }
    }

    Ok(())
}
