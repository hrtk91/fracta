use anyhow::Result;

use crate::state::State;
use crate::utils;

pub fn execute() -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    if state.worktrees.is_empty() {
        println!("No worktrees found.");
        return Ok(());
    }

    println!("=== Worktrees ===");
    println!("{:<20} {:<10} {:<12} {}", "NAME", "OFFSET", "BRANCH", "PATH");
    println!("{}", "-".repeat(72));

    for wt in state.worktrees {
        println!(
            "{:<20} {:<10} {:<12} {}",
            wt.name, wt.port_offset, wt.branch, wt.path
        );
    }

    Ok(())
}
