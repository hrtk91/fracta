use anyhow::Result;
use std::process::{Command, Stdio};

use crate::lima::client as lima;
use crate::state::State;
use crate::utils;

pub fn execute(name: &str) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    let instance = state
        .find_instance(name)
        .ok_or_else(|| anyhow::anyhow!("Instance '{}' not found", name))?;

    // Lima VM の状態を確認
    let info = lima::info(&instance.lima_instance)?;
    match info {
        lima::InstanceStatus::NotFound => {
            anyhow::bail!(
                "Lima VM '{}' not found. Run 'fracta add {}' first.",
                instance.lima_instance,
                name
            );
        }
        lima::InstanceStatus::Stopped => {
            anyhow::bail!(
                "Lima VM '{}' is not running. Start it with 'fracta up {}'.",
                instance.lima_instance,
                name
            );
        }
        lima::InstanceStatus::Running => {}
    }

    // Lima VM にシェル接続
    println!("Connecting to Lima VM: {}...", instance.lima_instance);
    println!("Worktree path: {}", instance.path);
    println!("---");

    let status = Command::new("limactl")
        .args(["shell", &instance.lima_instance])
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        anyhow::bail!("Shell session ended with error");
    }

    Ok(())
}
