use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::utils;

/// Lima インスタンス名を生成
pub fn instance_name(worktree_name: &str) -> String {
    let sanitized = utils::sanitize_name(worktree_name);
    format!("fracta-{}", sanitized)
}

/// Lima インスタンスの状態
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceStatus {
    Running,
    Stopped,
    NotFound,
}

impl std::fmt::Display for InstanceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceStatus::Running => write!(f, "Running"),
            InstanceStatus::Stopped => write!(f, "Stopped"),
            InstanceStatus::NotFound => write!(f, "NotFound"),
        }
    }
}

/// Lima インスタンスを作成
pub fn create(template_path: &Path, instance_name: &str) -> Result<()> {
    let output = Command::new("limactl")
        .args([
            "create",
            "--name",
            instance_name,
            template_path.to_string_lossy().as_ref(),
        ])
        .output()
        .context("Failed to execute limactl create")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("limactl create failed: {}", stderr.trim());
    }

    Ok(())
}

/// Lima インスタンスを起動
pub fn start(instance_name: &str) -> Result<()> {
    let output = Command::new("limactl")
        .args(["start", instance_name])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .output()
        .context("Failed to execute limactl start")?;

    if !output.status.success() {
        anyhow::bail!("limactl start failed for {}", instance_name);
    }

    Ok(())
}

/// Lima インスタンスを停止
pub fn stop(instance_name: &str) -> Result<()> {
    let output = Command::new("limactl")
        .args(["stop", instance_name])
        .output()
        .context("Failed to execute limactl stop")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("limactl stop failed: {}", stderr.trim());
    }

    Ok(())
}

/// Lima インスタンスを削除
pub fn delete(instance_name: &str) -> Result<()> {
    let output = Command::new("limactl")
        .args(["delete", "--force", instance_name])
        .output()
        .context("Failed to execute limactl delete")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("limactl delete failed: {}", stderr.trim());
    }

    Ok(())
}

/// Lima VM 内でコマンドを実行
pub fn shell(instance_name: &str, command: &[&str]) -> Result<std::process::Output> {
    let mut args = vec!["shell", "--workdir", "/", instance_name, "--"];
    args.extend(command);

    let output = Command::new("limactl")
        .args(&args)
        .output()
        .context("Failed to execute limactl shell")?;

    Ok(output)
}

/// Lima VM 内でコマンドを実行（出力を継承）
pub fn shell_interactive(instance_name: &str, command: &[&str]) -> Result<std::process::ExitStatus> {
    let mut args = vec!["shell", "--workdir", "/", instance_name, "--"];
    args.extend(command);

    let status = Command::new("limactl")
        .args(&args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .context("Failed to execute limactl shell")?;

    Ok(status)
}

/// Lima インスタンスの状態を取得
pub fn info(instance_name: &str) -> Result<InstanceStatus> {
    let output = Command::new("limactl")
        .args(["list", "--json", instance_name])
        .output()
        .context("Failed to execute limactl list")?;

    if !output.status.success() {
        return Ok(InstanceStatus::NotFound);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let status = parse_status_from_json(&stdout, instance_name);

    Ok(status)
}

/// Lima がインストールされているか確認
pub fn is_available() -> bool {
    Command::new("limactl")
        .args(["--version"])
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// SSH 設定ファイルのパスを取得
pub fn ssh_config_path(instance_name: &str) -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".lima")
        .join(instance_name)
        .join("ssh.config")
}

fn parse_status_from_json(json: &str, instance_name: &str) -> InstanceStatus {
    // limactl list --json の出力をパース
    // 出力は NDJSON 形式（改行区切りの JSON オブジェクト）
    for line in json.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
                if name == instance_name {
                    if let Some(status) = value.get("status").and_then(|v| v.as_str()) {
                        return match status {
                            "Running" => InstanceStatus::Running,
                            "Stopped" => InstanceStatus::Stopped,
                            _ => InstanceStatus::Stopped,
                        };
                    }
                }
            }
        }
    }
    InstanceStatus::NotFound
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instance_name() {
        assert_eq!(instance_name("develop"), "fracta-develop");
        assert_eq!(instance_name("feature/new"), "fracta-feature-new");
    }

    #[test]
    fn test_parse_status_from_json() {
        // NDJSON 形式（改行区切り）
        let json = r#"{"name": "fracta-test", "status": "Running"}"#;
        assert_eq!(
            parse_status_from_json(json, "fracta-test"),
            InstanceStatus::Running
        );

        let json = r#"{"name": "fracta-test", "status": "Stopped"}"#;
        assert_eq!(
            parse_status_from_json(json, "fracta-test"),
            InstanceStatus::Stopped
        );

        let json = r#""#;
        assert_eq!(
            parse_status_from_json(json, "fracta-test"),
            InstanceStatus::NotFound
        );
    }
}
