use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;

pub fn resolve_main_repo() -> Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .context("Failed to execute git rev-parse")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git rev-parse failed: {}", stderr.trim());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("git rev-parse returned empty path");
    }

    let mut common_dir = PathBuf::from(trimmed);
    if common_dir.is_relative() {
        let cwd = std::env::current_dir().context("Failed to get current directory")?;
        common_dir = cwd.join(common_dir);
    }

    let main_repo = common_dir
        .parent()
        .context("Failed to resolve MAIN_REPO")?
        .to_path_buf();

    Ok(main_repo)
}

pub fn fracta_worktree_dir(worktree_path: &Path) -> PathBuf {
    worktree_path.join(".fracta")
}

pub fn compose_base_path(config: &Config, worktree_path: &Path) -> PathBuf {
    let base = Path::new(config.compose_base());
    if base.is_absolute() {
        base.to_path_buf()
    } else {
        worktree_path.join(base)
    }
}

pub fn compose_generated_path(worktree_path: &Path) -> PathBuf {
    fracta_worktree_dir(worktree_path).join("compose.generated.yml")
}

pub fn is_path_within(parent: &Path, child: &Path) -> bool {
    let parent = match parent.canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };
    let child = match child.canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };

    child.starts_with(&parent)
}

/// ディレクトリ名やコンテナ名として使用できない文字をサニタイズ
///
/// - `/` を `-` に置換
/// - 連続する `-` を1つにまとめる
pub fn sanitize_name(name: &str) -> String {
    let sanitized = name.replace('/', "-");
    let parts: Vec<&str> = sanitized.split('-').filter(|s| !s.is_empty()).collect();
    parts.join("-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("develop3"), "develop3");
        assert_eq!(sanitize_name("feature/new-feature"), "feature-new-feature");
        assert_eq!(sanitize_name("bugfix/issue-123"), "bugfix-issue-123");
        assert_eq!(sanitize_name("feature//double-slash"), "feature-double-slash");
        assert_eq!(sanitize_name("/leading-slash"), "leading-slash");
        assert_eq!(sanitize_name("trailing-slash/"), "trailing-slash");
        assert_eq!(sanitize_name("normal-name"), "normal-name");
    }
}
