use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::Config;
use crate::lima::client as lima;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

pub struct HookContext {
    pub name: String,
    pub worktree_path: PathBuf,
    pub main_repo: PathBuf,
    pub port_offset: u16,
    pub compose_base: PathBuf,
    pub compose_file: PathBuf,
}

pub fn run_hook(hook: &str, working_dir: &Path, ctx: &HookContext, config: &Config) -> Result<()> {
    if let Some(cmd) = config.hook_command(hook) {
        run_config_hook(cmd, working_dir, ctx)?;
    }

    let hook_path = hook_path(&ctx.main_repo, hook);

    if !hook_path.exists() {
        return Ok(());
    }

    let metadata = std::fs::metadata(&hook_path)
        .context(format!("Failed to read {}", hook_path.display()))?;

    if !is_executable(&metadata) {
        return Ok(());
    }

    let status = Command::new(&hook_path)
        .current_dir(working_dir)
        .env("FRACTA_NAME", &ctx.name)
        .env("FRACTA_PATH", ctx.worktree_path.display().to_string())
        .env("MAIN_REPO", ctx.main_repo.display().to_string())
        .env("PORT_OFFSET", ctx.port_offset.to_string())
        .env("COMPOSE_BASE", ctx.compose_base.display().to_string())
        .env("COMPOSE_OVERRIDE", ctx.compose_file.display().to_string())
        .status()
        .context(format!("Failed to run hook {}", hook))?;

    if !status.success() {
        anyhow::bail!("Hook {} failed", hook);
    }

    Ok(())
}

fn run_config_hook(cmd: &str, working_dir: &Path, ctx: &HookContext) -> Result<()> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return Ok(());
    }

    if let Some(inner) = cmd.strip_prefix("vm:") {
        return run_vm_command(inner.trim(), ctx);
    }
    if let Some(inner) = cmd.strip_prefix("limactl:") {
        return run_vm_command(inner.trim(), ctx);
    }

    let status = Command::new("bash")
        .arg("-lc")
        .arg(cmd)
        .current_dir(working_dir)
        .env("FRACTA_NAME", &ctx.name)
        .env("FRACTA_PATH", ctx.worktree_path.display().to_string())
        .env("MAIN_REPO", ctx.main_repo.display().to_string())
        .env("PORT_OFFSET", ctx.port_offset.to_string())
        .env("COMPOSE_BASE", ctx.compose_base.display().to_string())
        .env("COMPOSE_OVERRIDE", ctx.compose_file.display().to_string())
        .status()
        .context("Failed to run config hook")?;

    if !status.success() {
        anyhow::bail!("Config hook failed");
    }

    Ok(())
}

fn run_vm_command(cmd: &str, ctx: &HookContext) -> Result<()> {
    let instance = lima::instance_name(&ctx.name);
    let env_exports = format!(
        "export FRACTA_NAME=\"{name}\" FRACTA_PATH=\"{path}\" MAIN_REPO=\"{repo}\" PORT_OFFSET=\"{offset}\" COMPOSE_BASE=\"{base}\" COMPOSE_OVERRIDE=\"{override}\";",
        name = ctx.name,
        path = ctx.worktree_path.display(),
        repo = ctx.main_repo.display(),
        offset = ctx.port_offset,
        base = ctx.compose_base.display(),
        override = ctx.compose_file.display()
    );
    let full_cmd = format!("{} {}", env_exports, cmd);
    let status = lima::shell_interactive(&instance, &["bash", "-lc", &full_cmd])?;
    if !status.success() {
        anyhow::bail!("VM hook failed");
    }
    Ok(())
}

fn hook_path(main_repo: &Path, hook: &str) -> PathBuf {
    main_repo.join(".fracta").join("hooks").join(hook)
}

fn is_executable(metadata: &std::fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        let _ = metadata;
        true
    }
}
