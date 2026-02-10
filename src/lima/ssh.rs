use anyhow::{Context, Result};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use super::client;

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
    let mut child = Command::new("ssh")
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
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to start SSH SOCKS5 proxy")?;

    ensure_forward_started(&mut child, "SOCKS5 proxy")?;
    Ok(child)
}

fn ensure_forward_started(child: &mut Child, label: &str) -> Result<()> {
    thread::sleep(Duration::from_millis(200));
    if let Some(status) = child.try_wait().context("Failed to check SSH status")? {
        let stderr = child
            .stderr
            .take()
            .map(|mut s| {
                let mut buf = String::new();
                let _ = std::io::Read::read_to_string(&mut s, &mut buf);
                buf
            })
            .unwrap_or_default();
        anyhow::bail!(
            "SSH {} exited early: {}{}",
            label,
            status,
            if stderr.trim().is_empty() {
                String::new()
            } else {
                format!(" ({})", stderr.trim())
            }
        );
    }
    Ok(())
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
