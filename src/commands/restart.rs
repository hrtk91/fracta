use anyhow::Result;
use std::path::PathBuf;

use crate::config;
use crate::hooks::{self, HookContext};
use crate::lima::client as lima;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?;
    let name = instance.name.as_str();

    println!("=== Restarting worktree: {} ===", name);

    let worktree_path = PathBuf::from(&instance.path);
    let config = config::load_config(&main_repo, Some(&worktree_path))?;
    let compose_base = utils::compose_base_path(&config, &worktree_path);

    if !compose_base.exists() {
        anyhow::bail!("Compose base not found: {}", compose_base.display());
    }

    // Lima VM の状態を確認
    let info = lima::info(&instance.lima_instance)?;
    if info != lima::InstanceStatus::Running {
        anyhow::bail!(
            "Lima VM '{}' is not running. Start it with 'fracta up {}'.",
            instance.lima_instance,
            name
        );
    }

    let hook_ctx = HookContext {
        name: name.to_string(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset: 0,
        compose_base: compose_base.clone(),
        compose_file: compose_base.clone(),
    };

    hooks::run_hook("pre_restart", &worktree_path, &hook_ctx, &config)?;

    // compose ファイルの相対パスを取得
    let compose_rel = compose_base
        .strip_prefix(&worktree_path)
        .unwrap_or(&compose_base);
    let compose_path = compose_rel.to_string_lossy();
    let vm_worktree_path = worktree_path.to_string_lossy();

    // VM 内で docker compose restart を実行
    println!("Running docker compose restart in VM...");
    let status = lima::shell_interactive(
        &instance.lima_instance,
        &[
            "bash",
            "-c",
            &format!(
                "cd '{}' && sudo docker compose -f '{}' restart",
                vm_worktree_path, compose_path
            ),
        ],
    )?;

    if !status.success() {
        anyhow::bail!("docker compose restart failed in VM");
    }

    hooks::run_hook("post_restart", &worktree_path, &hook_ctx, &config)?;

    println!("=== docker compose restart completed ===");

    Ok(())
}
