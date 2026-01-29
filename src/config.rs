use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub compose_base: Option<String>,
    pub hooks: Option<HookCommands>,
}

#[derive(Debug, Deserialize, Default)]
pub struct HookCommands {
    pub pre_add: Option<String>,
    pub post_add: Option<String>,
    pub pre_up: Option<String>,
    pub post_up: Option<String>,
    pub pre_down: Option<String>,
    pub post_down: Option<String>,
    pub pre_remove: Option<String>,
    pub post_remove: Option<String>,
    pub pre_restart: Option<String>,
    pub post_restart: Option<String>,
}

impl Config {
    pub fn compose_base(&self) -> &str {
        self.compose_base.as_deref().unwrap_or("docker-compose.yml")
    }

    pub fn hook_command(&self, hook: &str) -> Option<&str> {
        let hooks = self.hooks.as_ref()?;
        match hook {
            "pre_add" => hooks.pre_add.as_deref(),
            "post_add" => hooks.post_add.as_deref(),
            "pre_up" => hooks.pre_up.as_deref(),
            "post_up" => hooks.post_up.as_deref(),
            "pre_down" => hooks.pre_down.as_deref(),
            "post_down" => hooks.post_down.as_deref(),
            "pre_remove" => hooks.pre_remove.as_deref(),
            "post_remove" => hooks.post_remove.as_deref(),
            "pre_restart" => hooks.pre_restart.as_deref(),
            "post_restart" => hooks.post_restart.as_deref(),
            _ => None,
        }
    }
}

fn config_paths_in_dir(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let primary = dir.join("fracta.toml");
    if primary.exists() {
        paths.push(primary);
    }

    let mut extra = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries {
            let entry = entry.context("Failed to read config directory")?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let file_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(name) => name,
                None => continue,
            };
            if file_name == "fracta.toml" {
                continue;
            }
            if file_name.starts_with("fracta.") && file_name.ends_with(".toml") {
                extra.push(path);
            }
        }
    }
    extra.sort();
    paths.extend(extra);
    Ok(paths)
}

fn merge_hooks(target: &mut Option<HookCommands>, incoming: HookCommands) {
    let dst = target.get_or_insert_with(HookCommands::default);
    if incoming.pre_add.is_some() {
        dst.pre_add = incoming.pre_add;
    }
    if incoming.post_add.is_some() {
        dst.post_add = incoming.post_add;
    }
    if incoming.pre_up.is_some() {
        dst.pre_up = incoming.pre_up;
    }
    if incoming.post_up.is_some() {
        dst.post_up = incoming.post_up;
    }
    if incoming.pre_down.is_some() {
        dst.pre_down = incoming.pre_down;
    }
    if incoming.post_down.is_some() {
        dst.post_down = incoming.post_down;
    }
    if incoming.pre_remove.is_some() {
        dst.pre_remove = incoming.pre_remove;
    }
    if incoming.post_remove.is_some() {
        dst.post_remove = incoming.post_remove;
    }
    if incoming.pre_restart.is_some() {
        dst.pre_restart = incoming.pre_restart;
    }
    if incoming.post_restart.is_some() {
        dst.post_restart = incoming.post_restart;
    }
}

fn merge_config(target: &mut Config, incoming: Config) {
    if incoming.compose_base.is_some() {
        target.compose_base = incoming.compose_base;
    }
    if let Some(hooks) = incoming.hooks {
        merge_hooks(&mut target.hooks, hooks);
    }
}

pub fn load_config(main_repo: &Path, worktree_path: Option<&Path>) -> Result<Config> {
    let mut config = Config::default();
    let mut paths = config_paths_in_dir(main_repo)?;
    if let Some(worktree) = worktree_path {
        if worktree != main_repo {
            let mut worktree_paths = config_paths_in_dir(worktree)?;
            paths.append(&mut worktree_paths);
        }
    }

    for path in paths {
        let content = std::fs::read_to_string(&path)
            .context(format!("Failed to read {}", path.display()))?;
        let incoming: Config = toml::from_str(&content)
            .context(format!("Failed to parse {}", path.display()))?;
        merge_config(&mut config, incoming);
    }

    Ok(config)
}
