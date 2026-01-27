use anyhow::Result;

use crate::lima::client as lima;
use crate::lima::ssh;
use crate::state::{PortForward, State};
use crate::utils;

pub fn execute(name: Option<&str>, local_port: u16, remote_port: u16) -> Result<()> {
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

    // ポートが既に使用されているか確認
    if let Some(owner) = state.port_allocations.get(&local_port) {
        anyhow::bail!(
            "Local port {} is already in use by instance '{}'",
            local_port,
            owner
        );
    }

    // 同じインスタンスで同じリモートポートのフォワードが既にあるか確認
    if instance
        .active_forwards
        .iter()
        .any(|f| f.remote_port == remote_port)
    {
        anyhow::bail!(
            "Remote port {} is already being forwarded for instance '{}'",
            remote_port,
            name
        );
    }

    println!(
        "Starting port forward: localhost:{} -> VM:{}",
        local_port, remote_port
    );

    // SSH ポートフォワードを開始
    let child = ssh::start_forward(&instance.lima_instance, local_port, remote_port)?;
    let pid = child.id();

    // 状態を更新
    let forward = PortForward {
        local_port,
        remote_port,
        pid,
    };

    state.add_forward(name, forward)?;
    state.save(&main_repo)?;

    println!("Port forward started successfully.");
    println!("  Local:  localhost:{}", local_port);
    println!("  Remote: VM:{}", remote_port);
    println!("  PID:    {}", pid);
    println!("\nAccess the service at: http://localhost:{}", local_port);

    Ok(())
}
