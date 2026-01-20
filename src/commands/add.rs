use anyhow::{Context, Result};
use std::fs;
use std::process::Command;

use crate::compose;
use crate::config;
use crate::hooks::{self, HookContext};
use crate::state::{State, WorktreeState};
use crate::utils;

pub fn execute(name: &str, base_branch: Option<Option<String>>) -> Result<()> {
    println!("=== Adding worktree: {} ===", name);

    let main_repo = utils::resolve_main_repo()?;
    let config = config::load_config(&main_repo)?;

    let mut state = State::load(&main_repo)?;
    if state.find_worktree(name).is_some() {
        anyhow::bail!("Worktree '{}' already exists", name);
    }

    let repo_name = main_repo
        .file_name()
        .and_then(|n| n.to_str())
        .context("Failed to get repository name")?;

    // ディレクトリ名として使用するため、nameをサニタイズ
    let sanitized_name = utils::sanitize_name(name);
    let worktree_path = main_repo
        .parent()
        .context("Failed to get parent directory")?
        .join(format!("{}-{}", repo_name, sanitized_name));

    let used_offsets: std::collections::HashSet<u16> =
        state.worktrees.iter().map(|wt| wt.port_offset).collect();
    let port_offset = utils::choose_port_offset(name, &used_offsets);
    let compose_base = utils::compose_base_path(&config, &worktree_path);
    let compose_file = utils::compose_generated_path(&worktree_path);

    let hook_ctx = HookContext {
        name: name.to_string(),
        worktree_path: worktree_path.clone(),
        main_repo: main_repo.clone(),
        port_offset,
        compose_base: compose_base.clone(),
        compose_file: compose_file.clone(),
    };

    hooks::run_hook("pre_add", &main_repo, &hook_ctx)?;

    let mut git_args = vec!["worktree".to_string(), "add".to_string()];
    if let Some(base) = &base_branch {
        git_args.push("-b".to_string());
        git_args.push(name.to_string());
        git_args.push(worktree_path.to_string_lossy().to_string());
        if let Some(base_name) = base {
            git_args.push(base_name.to_string());
        } else {
            git_args.push("HEAD".to_string());
        }
    } else {
        git_args.push(worktree_path.to_string_lossy().to_string());
        git_args.push(name.to_string());
    }

    let output = Command::new("git")
        .args(&git_args)
        .current_dir(&main_repo)
        .output()
        .context("Failed to execute git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr.trim());
    }

    if !compose_base.exists() {
        anyhow::bail!(
            "Compose base not found: {}",
            compose_base.display()
        );
    }

    let env = utils::load_compose_env(&compose_base)?;
    let compose_result = compose::generate_compose(&compose_base, port_offset, name, &env)?;
    utils::ensure_parent_dir(&compose_file)?;
    fs::write(&compose_file, compose_result.yaml)
        .context("Failed to write compose file")?;

    for warning in compose_result.warnings {
        eprintln!("Warning: {}", warning);
    }

    let worktree_state = WorktreeState {
        name: name.to_string(),
        path: worktree_path.to_string_lossy().to_string(),
        branch: name.to_string(),
        port_offset,
    };

    state.add_worktree(worktree_state);
    state.save(&main_repo)?;

    hooks::run_hook("post_add", &worktree_path, &hook_ctx)?;

    println!("Worktree added: {}", worktree_path.display());
    println!("Port offset: {}", port_offset);

    Ok(())
}
