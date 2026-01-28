use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::lima::client as lima;
use crate::utils;

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

fn sync_image(instance_name: &str, image: &str) -> Result<()> {
    let mut save = Command::new("docker")
        .args(["save", image])
        .stdout(Stdio::piped())
        .spawn()
        .context("Failed to run docker save")?;

    let save_stdout = save
        .stdout
        .take()
        .context("Failed to capture docker save output")?;

    let mut load = Command::new("limactl")
        .args(["shell", "--workdir", "/", instance_name, "--", "sudo", "docker", "load"])
        .stdin(Stdio::from(save_stdout))
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("Failed to run docker load in VM")?;

    let load_status = load.wait().context("Failed to wait for docker load")?;
    let save_status = save.wait().context("Failed to wait for docker save")?;

    if !save_status.success() {
        anyhow::bail!("docker save failed for image {}", image);
    }
    if !load_status.success() {
        anyhow::bail!("docker load failed in VM for image {}", image);
    }

    Ok(())
}

pub fn sync_images_to_vm(instance_name: &str, images: &[String]) -> Result<()> {
    for image in images {
        let host_id = match host_image_id(image)? {
            Some(id) => id,
            None => {
                println!("Skipping (not found on host): {}", image);
                continue;
            }
        };

        let vm_id = vm_image_id(instance_name, image)?;
        if let Some(vm_id) = vm_id {
            if vm_id == host_id {
                println!("Already synced: {}", image);
                continue;
            }
        }

        println!("Syncing image: {}", image);
        sync_image(instance_name, image)?;
    }

    Ok(())
}
