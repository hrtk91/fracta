use anyhow::Result;
use std::process::{Command, Stdio};

use crate::lima::client as lima;
use crate::lima::ssh;
use crate::state::State;
use crate::utils;

pub fn start(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;
    let instance = state.resolve_instance(name)?;

    let info = lima::info(&instance.lima_instance)?;
    match info {
        lima::InstanceStatus::NotFound => {
            anyhow::bail!(
                "Lima VM '{}' not found. Run 'fracta add {}' first.",
                instance.lima_instance,
                instance.name
            );
        }
        lima::InstanceStatus::Running => {
            println!("Lima VM '{}' is already running.", instance.lima_instance);
        }
        lima::InstanceStatus::Stopped => {
            println!("Starting Lima VM: {}...", instance.lima_instance);
            lima::start(&instance.lima_instance)?;
            println!("Lima VM started.");
        }
    }

    Ok(())
}

pub fn stop(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;
    let instance = state.resolve_instance(name)?.clone();
    let instance_name = instance.name.clone();

    // Stop local helper processes first so state/ports stay consistent.
    for forward in &instance.active_forwards {
        if ssh::is_process_alive(forward.pid) {
            if let Err(e) = ssh::stop_forward(forward.pid) {
                eprintln!(
                    "Warning: Failed to stop forward PID {} (localhost:{}): {}",
                    forward.pid, forward.local_port, e
                );
            }
        }
    }
    state.clear_forwards(&instance_name)?;

    if let Some(proxy) = &instance.active_proxy {
        if ssh::is_process_alive(proxy.pid) {
            if let Err(e) = ssh::stop_forward(proxy.pid) {
                eprintln!(
                    "Warning: Failed to stop SOCKS5 proxy PID {} (localhost:{}): {}",
                    proxy.pid, proxy.local_port, e
                );
            }
        }
    }
    state.remove_proxy(&instance_name)?;

    if let Some(browser) = &instance.active_browser {
        if ssh::is_process_alive(browser.pid) {
            if let Err(e) = ssh::stop_forward(browser.pid) {
                eprintln!(
                    "Warning: Failed to stop Playwright PID {}: {}",
                    browser.pid, e
                );
            }
        }
    }
    state.remove_browser(&instance_name)?;
    state.save(&main_repo)?;

    let info = lima::info(&instance.lima_instance)?;
    if info == lima::InstanceStatus::Running {
        println!("Stopping Lima VM: {}...", instance.lima_instance);
        lima::stop(&instance.lima_instance)?;
        println!("Lima VM stopped.");
    } else {
        println!("Lima VM '{}' is not running.", instance.lima_instance);
    }

    Ok(())
}

pub fn shell(
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
                "Lima VM '{}' is not running. Start it with 'fracta vm start {}'.",
                instance.lima_instance,
                name
            );
        }
        lima::InstanceStatus::Running => {}
    }

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

pub fn list() -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    if state.instances.is_empty() {
        println!("No instances found.");
        return Ok(());
    }

    println!("=== VM Instances ===");
    println!(
        "{:<20} {:<25} {:<10} {}",
        "NAME", "LIMA VM", "VM STATUS", "PATH"
    );
    println!("{}", "-".repeat(90));

    for inst in &state.instances {
        let vm_status = match lima::info(&inst.lima_instance) {
            Ok(status) => status.to_string(),
            Err(_) => "Unknown".to_string(),
        };

        println!(
            "{:<20} {:<25} {:<10} {}",
            inst.name, inst.lima_instance, vm_status, inst.path
        );
    }

    Ok(())
}
