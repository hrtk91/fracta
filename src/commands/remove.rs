use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::config;
use crate::hooks::{self, HookContext};
use crate::state::State;
use crate::utils;

pub fn execute(name: &str, force: bool) -> Result<()> {
    println!("=== Removing worktree: {} ===", name);

    let main_repo = utils::resolve_main_repo()?;
    let config = config::load_config(&main_repo)?;
    let mut state = State::load(&main_repo)?;

    let worktree = state
        .find_worktree(name)
        .ok_or_else(|| anyhow::anyhow!("Worktree '{}' not found", name))?;

    let worktree_path = PathBuf::from(&worktree.path);
    let compose_base = utils::compose_base_path(&config, &worktree_path);
    let compose_file = utils::compose_generated_path(&worktree_path);

    let hook_ctx = HookContext {
        name: name.to_string(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset: worktree.port_offset,
        compose_base: compose_base.clone(),
        compose_file: compose_file.clone(),
    };

    hooks::run_hook("pre_remove", &worktree_path, &hook_ctx)?;

    let compose_missing = !compose_base.exists() || !compose_file.exists();
    if compose_missing {
        if force {
            if !compose_base.exists() {
                eprintln!("Warning: Compose base not found: {}", compose_base.display());
            }
            if !compose_file.exists() {
                eprintln!("Warning: Compose file not found: {}", compose_file.display());
            }
            eprintln!("Warning: Skipping docker compose down (force enabled)");
        } else {
            if !compose_base.exists() {
                anyhow::bail!("Compose base not found: {}", compose_base.display());
            }
            anyhow::bail!("Compose file not found: {}", compose_file.display());
        }
    } else {
        let down_output = Command::new("docker")
            .args([
                "compose",
                "--project-directory",
                worktree_path.to_string_lossy().as_ref(),
                "-f",
                compose_file.to_string_lossy().as_ref(),
                "down",
            ])
            .current_dir(&worktree_path)
            .output()
            .context("Failed to execute docker compose down")?;

        if !down_output.status.success() {
            let stderr = String::from_utf8_lossy(&down_output.stderr);
            if force {
                eprintln!("Warning: docker compose down failed: {}", stderr.trim());
            } else {
                anyhow::bail!("docker compose down failed: {}", stderr.trim());
            }
        } else {
            println!("{}", String::from_utf8_lossy(&down_output.stdout));
        }
    }

    hooks::run_hook("post_remove", &worktree_path, &hook_ctx)?;

    if compose_file.exists() {
        std::fs::remove_file(&compose_file)
            .context("Failed to remove compose file")?;
    }

    let worktree_fracta_dir = utils::fracta_worktree_dir(&worktree_path);
    if worktree_fracta_dir.exists() {
        let _ = std::fs::remove_dir(&worktree_fracta_dir);
    }

    let output = Command::new("git")
        .args(["worktree", "remove", worktree_path.to_string_lossy().as_ref()])
        .current_dir(&main_repo)
        .output()
        .context("Failed to execute git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree remove failed: {}", stderr.trim());
    }

    state.remove_worktree(name);
    state.save(&main_repo)?;

    Ok(())
}
