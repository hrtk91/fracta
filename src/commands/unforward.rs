use anyhow::Result;

use crate::lima::ssh;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>, local_port: u16) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?.clone();
    let name = instance.name.as_str();

    // フォワードを探す
    let forward = instance
        .active_forwards
        .iter()
        .find(|f| f.local_port == local_port)
        .cloned();

    let forward = match forward {
        Some(f) => f,
        None => {
            anyhow::bail!(
                "No port forward found on local port {} for instance '{}'",
                local_port,
                name
            );
        }
    };

    println!(
        "Stopping port forward: localhost:{} -> VM:{}",
        forward.local_port, forward.remote_port
    );

    // プロセスを停止
    if ssh::is_process_alive(forward.pid) {
        ssh::stop_forward(forward.pid)?;
    } else {
        println!("Forward process (PID {}) was already stopped.", forward.pid);
    }

    // 状態を更新
    state.remove_forward(name, local_port)?;
    state.save(&main_repo)?;

    println!("Port forward stopped successfully.");

    Ok(())
}

/// 全てのポートフォワードを停止
pub fn execute_all(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let mut state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?.clone();
    let name = instance.name.as_str();

    let forwards = instance.active_forwards.clone();

    if forwards.is_empty() {
        println!("No active port forwards for instance '{}'", name);
        return Ok(());
    }

    println!("Stopping {} port forwards for instance '{}'...", forwards.len(), name);

    for forward in &forwards {
        println!(
            "  Stopping localhost:{} -> VM:{}",
            forward.local_port, forward.remote_port
        );
        if ssh::is_process_alive(forward.pid) {
            if let Err(e) = ssh::stop_forward(forward.pid) {
                eprintln!("    Warning: Failed to stop PID {}: {}", forward.pid, e);
            }
        }
    }

    // 状態をクリア
    state.clear_forwards(name)?;
    state.save(&main_repo)?;

    println!("All port forwards stopped.");

    Ok(())
}
