use anyhow::{Context, Result};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;

/// worktree名からポートオフセットを計算
///
/// - main: offset=0
/// - worktree: 1000..9000 (1000刻み)
pub fn calculate_port_offset(name: &str) -> u16 {
    if name == "main" || name.is_empty() {
        return 0;
    }

    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let hash = hasher.finish();

    let offset_multiplier = ((hash % 9) + 1) as u16;
    offset_multiplier * 1000
}

pub fn choose_port_offset(name: &str, used_offsets: &HashSet<u16>) -> u16 {
    let base = calculate_port_offset(name);
    if base == 0 {
        return 0;
    }

    let start_index = ((base / 1000).saturating_sub(1)) as usize;
    let candidates: Vec<u16> = (1..=9).map(|i| i * 1000).collect();

    for i in 0..candidates.len() {
        let idx = (start_index + i) % candidates.len();
        let candidate = candidates[idx];
        if !used_offsets.contains(&candidate) {
            return candidate;
        }
    }

    base
}

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

pub fn load_compose_env(compose_base: &Path) -> Result<HashMap<String, String>> {
    let mut env_map: HashMap<String, String> = std::env::vars().collect();

    let dotenv_path = compose_base
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".env");

    if dotenv_path.exists() {
        let content = std::fs::read_to_string(&dotenv_path)
            .context(format!("Failed to read {}", dotenv_path.display()))?;
        let env_file = parse_dotenv(&content);
        for (key, value) in env_file {
            env_map.entry(key).or_insert(value);
        }
    }

    Ok(env_map)
}

fn parse_dotenv(content: &str) -> HashMap<String, String> {
    let mut env_map = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let line = line.strip_prefix("export ").unwrap_or(line);
        let (key, value) = match line.split_once('=') {
            Some((key, value)) => (key.trim(), value.trim()),
            None => continue,
        };

        if key.is_empty() {
            continue;
        }

        let value = strip_quotes(value);
        env_map.insert(key.to_string(), value.to_string());
    }

    env_map
}

fn strip_quotes(value: &str) -> &str {
    let bytes = value.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &value[1..value.len() - 1];
        }
    }
    value
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .context(format!("Failed to create directory {}", parent.display()))?;
    }
    Ok(())
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
    fn test_calculate_port_offset() {
        assert_eq!(calculate_port_offset("main"), 0);
        assert_eq!(calculate_port_offset(""), 0);

        let offset_a = calculate_port_offset("feature-A");
        assert!(offset_a >= 1000 && offset_a <= 9000);
        assert_eq!(offset_a % 1000, 0);

        assert_eq!(
            calculate_port_offset("feature-A"),
            calculate_port_offset("feature-A")
        );
    }

    #[test]
    fn test_choose_port_offset() {
        use std::collections::HashSet;

        let mut used = HashSet::new();
        used.insert(1000);
        used.insert(2000);
        let offset = choose_port_offset("feature-A", &used);
        assert!(offset >= 1000 && offset <= 9000);
        assert!(!used.contains(&offset));
    }

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
