use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::compose;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>, short: bool) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
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

    let compose_file = utils::compose_generated_path(&worktree_path);
    if !compose_file.exists() {
        anyhow::bail!("Compose file not found: {}", compose_file.display());
    }

    let ports = compose::extract_ports(&compose_file)?;

    if short {
        for entry in ports {
            println!("{}\t{}\t{}", entry.service, entry.host, entry.target);
        }
        return Ok(());
    }

    println!("=== Ports: {} (offset {}) ===", worktree_name, port_offset);

    if ports.is_empty() {
        println!("No published ports found.");
        return Ok(());
    }

    println!("{}", compose::format_ports_table(&ports));

    Ok(())
}
