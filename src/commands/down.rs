use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;

use crate::config;
use crate::hooks::{self, HookContext};
use crate::state::State;
use crate::utils;

pub fn execute(name: &str) -> Result<()> {
    println!("=== Stopping worktree: {} ===", name);

    let main_repo = utils::resolve_main_repo()?;
    let config = config::load_config(&main_repo)?;
    let state = State::load(&main_repo)?;

    let worktree = state
        .find_worktree(name)
        .ok_or_else(|| anyhow::anyhow!("Worktree '{}' not found", name))?;

    let worktree_path = PathBuf::from(&worktree.path);
    let compose_base = utils::compose_base_path(&config, &worktree_path);
    let compose_file = utils::compose_generated_path(&worktree_path);

    if !compose_base.exists() {
        anyhow::bail!("Compose base not found: {}", compose_base.display());
    }
    if !compose_file.exists() {
        anyhow::bail!("Compose file not found: {}", compose_file.display());
    }

    let hook_ctx = HookContext {
        name: name.to_string(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset: worktree.port_offset,
        compose_base: compose_base.clone(),
        compose_file: compose_file.clone(),
    };

    hooks::run_hook("pre_down", &worktree_path, &hook_ctx)?;

    let output = Command::new("docker")
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

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose down failed: {}", stderr.trim());
    }

    println!("{}", String::from_utf8_lossy(&output.stdout));

    hooks::run_hook("post_down", &worktree_path, &hook_ctx)?;

    Ok(())
}
