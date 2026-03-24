use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use crate::lima::client as lima;
use crate::utils;

const CACHE_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60); // 7 days

fn cache_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME not set")?;
    Ok(PathBuf::from(home).join(".fracta").join("cache"))
}

/// image ID から短縮ハッシュを取得（ファイル名用）
fn cache_key(image_id: &str) -> String {
    // image_id is like "sha256:abcdef1234..."
    let hash = image_id.strip_prefix("sha256:").unwrap_or(image_id);
    hash[..12.min(hash.len())].to_string()
}

fn cache_path(image_id: &str) -> Result<PathBuf> {
    Ok(cache_dir()?.join(format!("{}.tar.gz", cache_key(image_id))))
}

fn ensure_cache_dir() -> Result<PathBuf> {
    let dir = cache_dir()?;
    fs::create_dir_all(&dir).context("Failed to create cache directory")?;
    Ok(dir)
}

fn docker_compose_config(compose_base: &Path, worktree_path: &Path) -> Result<Value> {
    let compose_path = compose_base
        .to_str()
        .context("Compose base path is not valid UTF-8")?;
    let output = Command::new("docker")
        .args(["compose", "-f", compose_path, "config", "--format", "json"])
        .current_dir(worktree_path)
        .output()
        .context("Failed to execute docker compose config")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("docker compose config failed: {}", stderr.trim());
    }

    let value: Value =
        serde_json::from_slice(&output.stdout).context("Failed to parse compose config JSON")?;
    Ok(value)
}

fn default_project_name(worktree_path: &Path) -> Result<String> {
    let name = worktree_path
        .file_name()
        .context("Failed to determine worktree directory name")?
        .to_string_lossy()
        .to_string();
    Ok(utils::sanitize_name(&name))
}

pub fn collect_compose_images(compose_base: &Path, worktree_path: &Path) -> Result<Vec<String>> {
    let config = docker_compose_config(compose_base, worktree_path)?;
    let project = config
        .get("name")
        .and_then(|v| v.as_str())
        .map(|v| utils::sanitize_name(v))
        .unwrap_or(default_project_name(worktree_path)?);

    let mut images = BTreeSet::new();
    let services = config
        .get("services")
        .and_then(|v| v.as_object())
        .context("Compose config does not contain services")?;

    for (service_name, service_cfg) in services {
        if let Some(image) = service_cfg.get("image").and_then(|v| v.as_str()) {
            images.insert(image.to_string());
        } else {
            images.insert(format!("{}-{}", project, service_name));
        }
    }

    Ok(images.into_iter().collect())
}

fn host_image_id(image: &str) -> Result<Option<String>> {
    let output = Command::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", image])
        .output()
        .context("Failed to execute docker image inspect")?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        Ok(None)
    } else {
        Ok(Some(stdout))
    }
}

fn vm_image_id(instance_name: &str, image: &str) -> Result<Option<String>> {
    let output = lima::shell(
        instance_name,
        &[
            "bash",
            "-c",
            &format!("sudo docker image inspect --format '{{{{.Id}}}}' {}", image),
        ],
    )?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        Ok(None)
    } else {
        Ok(Some(stdout))
    }
}

/// docker save | gzip してキャッシュに保存
fn save_to_cache(image: &str, image_id: &str) -> Result<PathBuf> {
    ensure_cache_dir()?;
    let path = cache_path(image_id)?;

    if path.exists() {
        return Ok(path);
    }

    let mut save = Command::new("docker")
        .args(["save", image])
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to run docker save")?;

    let save_stdout = save
        .stdout
        .take()
        .context("Failed to capture docker save output")?;

    let mut gzip = Command::new("gzip")
        .args(["-1"]) // fast compression
        .stdin(Stdio::from(save_stdout))
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to run gzip")?;

    let gzip_stdout = gzip
        .stdout
        .take()
        .context("Failed to capture gzip output")?;

    // Write to temp file first, then rename for atomicity
    let tmp_path = path.with_extension("tar.gz.tmp");
    let mut tmp_file =
        fs::File::create(&tmp_path).context("Failed to create temp cache file")?;
    std::io::copy(&mut std::io::BufReader::new(gzip_stdout), &mut tmp_file)
        .context("Failed to write cache file")?;

    let gzip_status = gzip.wait().context("Failed to wait for gzip")?;
    let save_status = save.wait().context("Failed to wait for docker save")?;

    if !save_status.success() || !gzip_status.success() {
        let _ = fs::remove_file(&tmp_path);
        anyhow::bail!("docker save | gzip failed for image {}", image);
    }

    fs::rename(&tmp_path, &path).context("Failed to rename cache file")?;
    Ok(path)
}

/// キャッシュから VM に load
fn load_from_cache(instance_name: &str, cache_file: &Path) -> Result<()> {
    let mut gunzip = Command::new("gunzip")
        .args(["-c"])
        .arg(cache_file)
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to run gunzip")?;

    let gunzip_stdout = gunzip
        .stdout
        .take()
        .context("Failed to capture gunzip output")?;

    let mut load = Command::new("limactl")
        .args([
            "shell", "--workdir", "/", instance_name, "--", "sudo", "docker", "load",
        ])
        .stdin(Stdio::from(gunzip_stdout))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to run docker load in VM")?;

    let load_status = load.wait().context("Failed to wait for docker load")?;
    let gunzip_status = gunzip.wait().context("Failed to wait for gunzip")?;

    if !gunzip_status.success() {
        anyhow::bail!("gunzip failed for {}", cache_file.display());
    }
    if !load_status.success() {
        anyhow::bail!("docker load failed in VM");
    }

    Ok(())
}

fn sync_image(instance_name: &str, image: &str, image_id: &str) -> Result<()> {
    let cached = cache_path(image_id)?;

    if cached.exists() {
        println!("  Loading from cache...");
    } else {
        println!("  Saving to cache...");
        save_to_cache(image, image_id)?;
    }

    println!("  Loading into VM...");
    load_from_cache(instance_name, &cache_path(image_id)?)?;

    Ok(())
}

pub fn sync_images_to_vm(instance_name: &str, images: &[String]) -> Result<()> {
    let mut used_keys = HashSet::new();

    for image in images {
        let host_id = match host_image_id(image)? {
            Some(id) => id,
            None => {
                println!("Skipping (not found on host): {}", image);
                continue;
            }
        };

        used_keys.insert(cache_key(&host_id));

        let vm_id = vm_image_id(instance_name, image)?;
        if let Some(vm_id) = vm_id {
            if vm_id == host_id {
                println!("Already synced: {}", image);
                continue;
            }
        }

        println!("Syncing image: {}", image);
        sync_image(instance_name, image, &host_id)?;
    }

    // Cleanup stale cache entries
    if let Err(e) = cleanup_cache(&used_keys) {
        eprintln!("Warning: cache cleanup failed: {}", e);
    }

    Ok(())
}

/// 7 日超かつ今回使われなかったキャッシュを削除
fn cleanup_cache(used_keys: &HashSet<String>) -> Result<()> {
    let dir = cache_dir()?;
    if !dir.exists() {
        return Ok(());
    }

    let now = SystemTime::now();
    let mut cleaned = 0u64;

    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();

        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.strip_suffix(".tar").unwrap_or(n).to_string(),
            None => continue,
        };

        // Skip if used in this sync
        if used_keys.contains(&name) {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified = metadata.modified().unwrap_or(now);
        if let Ok(age) = now.duration_since(modified) {
            if age > CACHE_MAX_AGE {
                let size = metadata.len();
                if let Err(e) = fs::remove_file(&path) {
                    eprintln!("Warning: failed to remove {}: {}", path.display(), e);
                } else {
                    cleaned += size;
                }
            }
        }
    }

    if cleaned > 0 {
        println!(
            "Cache cleanup: removed {:.1} MB of stale images",
            cleaned as f64 / 1_048_576.0
        );
    }

    Ok(())
}
