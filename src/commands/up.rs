use anyhow::Result;
use std::path::PathBuf;

use crate::config;
use crate::hooks::{self, HookContext};
use crate::lima::client as lima;
use crate::lima::template;
use crate::state::State;
use crate::utils;

pub fn execute(name: Option<&str>) -> Result<()> {
    let main_repo = utils::resolve_main_repo()?;
    let config = config::load_config(&main_repo)?;
    let state = State::load(&main_repo)?;

    let instance = state.resolve_instance(name)?;
    let instance_name = instance.name.as_str();

    println!("=== Starting worktree: {} ===", instance_name);

    let worktree_path = PathBuf::from(&instance.path);
    let compose_base = utils::compose_base_path(&config, &worktree_path);

    if !compose_base.exists() {
        anyhow::bail!("Compose base not found: {}", compose_base.display());
    }

    // Lima VM が起動しているか確認
    let info = lima::info(&instance.lima_instance)?;
    match info {
        lima::InstanceStatus::NotFound => {
            println!(
                "Lima VM '{}' not found. Creating a new VM...",
                instance.lima_instance
            );

            let mut tmpl_cfg = template::TemplateConfig::new(&worktree_path.to_string_lossy());
            tmpl_cfg.registry_mirror = config.registry_mirror.clone();
            let temp_template = template::create_temp_template(&tmpl_cfg)?;

            lima::create(temp_template.path(), &instance.lima_instance)?;
            println!("Starting Lima VM: {}...", instance.lima_instance);
            lima::start(&instance.lima_instance)?;
        }
        lima::InstanceStatus::Stopped => {
            println!("Starting Lima VM: {}...", instance.lima_instance);
            lima::start(&instance.lima_instance)?;
        }
        lima::InstanceStatus::Running => {
            println!("Lima VM '{}' is already running.", instance.lima_instance);
        }
    }

    // compose ファイルの相対パスを取得
    let compose_rel = compose_base
        .strip_prefix(&worktree_path)
        .unwrap_or(&compose_base);

    let hook_ctx = HookContext {
        name: instance_name.to_string(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset: 0,
        compose_base: compose_base.clone(),
        compose_file: compose_base.clone(), // v2 では生成ファイル不使用
    };

    hooks::run_hook("pre_up", &worktree_path, &hook_ctx)?;

    // Lima VM 内で docker compose up を実行
    println!("Running docker compose up in VM...");
    let compose_path = compose_rel.to_string_lossy();

    // VM 内の worktree パス
    let vm_worktree_path = worktree_path.to_string_lossy();

    let status = lima::shell_interactive(
        &instance.lima_instance,
        &[
            "bash",
            "-c",
            &format!(
                "cd '{}' && COMPOSE_PARALLEL_BUILD=0 sudo docker compose -f '{}' up -d",
                vm_worktree_path, compose_path
            ),
        ],
    )?;

    if !status.success() {
        anyhow::bail!("docker compose up failed in VM");
    }

    hooks::run_hook("post_up", &worktree_path, &hook_ctx)?;

    // コンテナの状態を表示
    println!("\nContainer status:");
    let _ = lima::shell_interactive(
        &instance.lima_instance,
        &[
            "bash",
            "-c",
            &format!(
                "cd '{}' && sudo docker compose -f '{}' ps",
                vm_worktree_path, compose_path
            ),
        ],
    );

    println!("\n=== docker compose up completed ===");
    println!(
        "Use 'fracta forward {} <local_port> <remote_port>' to access services",
        instance_name
    );

    Ok(())
}
