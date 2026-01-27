use anyhow::Result;

use crate::lima::client as lima;
use crate::lima::ssh;
use crate::state::{ProxyForward, State};
use crate::utils;

const PROXY_PORT_START: u16 = 1080;
const PROXY_PORT_END: u16 = 1099;

fn find_available_port(state: &State) -> Result<u16> {
    for port in PROXY_PORT_START..=PROXY_PORT_END {
        if !state.port_allocations.contains_key(&port) {
            return Ok(port);
        }
    }
    anyhow::bail!(
        "No available proxy ports in range {}-{}",
        PROXY_PORT_START,
        PROXY_PORT_END
    );
}

pub fn execute(name: Option<&str>, port: Option<u16>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?.clone();
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

    if let Some(active) = instance.active_proxy {
        if ssh::is_process_alive(active.pid) {
            anyhow::bail!(
                "SOCKS5 proxy is already running for '{}' on localhost:{} (PID {})",
                name,
                active.local_port,
                active.pid
            );
        }
        state.remove_proxy(name)?;
    }

    let local_port = match port {
        Some(p) => p,
        None => find_available_port(&state)?,
    };

    if let Some(owner) = state.port_allocations.get(&local_port) {
        anyhow::bail!(
            "Local port {} is already in use by instance '{}'",
            local_port,
            owner
        );
    }

    println!(
        "Starting SOCKS5 proxy: localhost:{} -> VM:{}",
        local_port, instance.lima_instance
    );

    let child = ssh::start_proxy(&instance.lima_instance, local_port)?;
    let pid = child.id();

    let proxy = ProxyForward {
        local_port,
        pid,
    };

    state.add_proxy(name, proxy)?;
    state.save(&main_repo)?;

    println!("SOCKS5 proxy started successfully.");
    println!("  Local:  localhost:{}", local_port);
    println!("  PID:    {}", pid);
    println!("\nSet your browser/Playwright proxy to socks5://localhost:{}", local_port);

    Ok(())
}
