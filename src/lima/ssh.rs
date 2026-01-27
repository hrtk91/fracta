use anyhow::{Context, Result};
use std::process::{Child, Command, Stdio};

use super::client;

/// SSH ポートフォワードを開始
pub fn start_forward(
    instance_name: &str,
    local_port: u16,
    remote_port: u16,
) -> Result<Child> {
    let ssh_config = client::ssh_config_path(instance_name);

    if !ssh_config.exists() {
        anyhow::bail!(
            "SSH config not found for instance '{}'. Is the VM running?",
            instance_name
        );
    }

    let host = format!("lima-{}", instance_name);
    let child = Command::new("ssh")
        .args([
            "-F",
            ssh_config.to_string_lossy().as_ref(),
            "-N",
            "-o",
            "ExitOnForwardFailure=yes",
            "-L",
            &format!("{}:localhost:{}", local_port, remote_port),
            &host,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start SSH port forward")?;

    Ok(child)
}

/// SSH SOCKS5 プロキシを開始
pub fn start_proxy(instance_name: &str, local_port: u16) -> Result<Child> {
    let ssh_config = client::ssh_config_path(instance_name);

    if !ssh_config.exists() {
        anyhow::bail!(
            "SSH config not found for instance '{}'. Is the VM running?",
            instance_name
        );
    }

    let host = format!("lima-{}", instance_name);
    let child = Command::new("ssh")
        .args([
            "-F",
            ssh_config.to_string_lossy().as_ref(),
            "-N",
            "-o",
            "ExitOnForwardFailure=yes",
            "-D",
            &format!("127.0.0.1:{}", local_port),
            &host,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start SSH SOCKS5 proxy")?;

    Ok(child)
}

/// SSH ポートフォワードを停止
pub fn stop_forward(pid: u32) -> Result<()> {
    let output = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output()
        .context("Failed to execute kill")?;

    if !output.status.success() {
        // プロセスが既に終了している場合は成功扱い
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.contains("No such process") {
            anyhow::bail!("Failed to kill process {}: {}", pid, stderr.trim());
        }
    }

    Ok(())
}

/// PID が生きているか確認
pub fn is_process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}
