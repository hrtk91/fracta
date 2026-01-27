use anyhow::Result;
use std::process::{Command, Stdio};

use crate::lima::client as lima;
use crate::state::State;
use crate::utils;

pub fn execute(
    name: Option<&str>,
    shell: Option<&str>,
    workdir: Option<&str>,
    tty: Option<bool>,
    command: &[String],
) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?;
    let name = instance.name.as_str();

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

    let mut args: Vec<String> = vec!["shell".to_string()];
    if let Some(shell) = shell {
        args.push("--shell".to_string());
        args.push(shell.to_string());
    }
    if let Some(workdir) = workdir {
        args.push("--workdir".to_string());
        args.push(workdir.to_string());
    }
    if let Some(tty) = tty {
        args.push("--tty".to_string());
        args.push(tty.to_string());
    }
    args.push(instance.lima_instance.clone());
    if !command.is_empty() {
        args.extend(command.iter().cloned());
    }

    let status = Command::new("limactl")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    if !status.success() {
        anyhow::bail!("Shell session ended with error");
    }

    Ok(())
}
