use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::config;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let config = config::load_config(&main_repo)?;
    let state = State::load(&main_repo)?;

    let (worktree_name, worktree_path, port_offset) = match name {
        Some(name) => {
            let worktree = state
                .find_worktree(name)
                .ok_or_else(|| anyhow::anyhow!("Worktree '{}' not found", name))?;
            (worktree.name.as_str(), PathBuf::from(&worktree.path), worktree.port_offset)
        }
        None => {
            let cwd = std::env::current_dir()
                .context("Failed to get current directory")?;
            let worktree = state.worktrees.iter().find(|wt| {
                utils::is_path_within(std::path::Path::new(&wt.path), &cwd)
            });

            let worktree = match worktree {
                Some(wt) => wt,
                None => anyhow::bail!("No worktree found for current directory"),
            };

            (worktree.name.as_str(), PathBuf::from(&worktree.path), worktree.port_offset)
        }
    };

    let compose_base = utils::compose_base_path(&config, &worktree_path);
    let compose_file = utils::compose_generated_path(&worktree_path);

    if !compose_base.exists() {
        anyhow::bail!("Compose base not found: {}", compose_base.display());
    }
    if !compose_file.exists() {
        anyhow::bail!("Compose file not found: {}", compose_file.display());
    }

    println!("=== docker compose ps: {} (offset {}) ===", worktree_name, port_offset);

    let output = Command::new("docker")
        .args([
            "compose",
            "--project-directory",
            worktree_path.to_string_lossy().as_ref(),
            "-f",
            compose_file.to_string_lossy().as_ref(),
            "ps",
        ])
        .current_dir(&worktree_path)
        .output()
        .context("Failed to execute docker compose ps")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose ps failed: {}", stderr.trim());
    }

    println!("{}", String::from_utf8_lossy(&output.stdout));

    Ok(())
}
