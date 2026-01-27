use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Default)]
pub struct Config {
    pub compose_base: Option<String>,
    pub registry_mirror: Option<String>,
}

impl Config {
    pub fn compose_base(&self) -> &str {
        self.compose_base.as_deref().unwrap_or("docker-compose.yml")
    }

    pub fn registry_mirror(&self) -> Option<&str> {
        self.registry_mirror.as_deref()
    }
}

pub fn config_path(main_repo: &Path) -> PathBuf {
    main_repo.join("fracta.toml")
}

pub fn load_config(main_repo: &Path) -> Result<Config> {
    let path = config_path(main_repo);
    if !path.exists() {
        return Ok(Config::default());
    }

    let content = std::fs::read_to_string(&path)
        .context(format!("Failed to read {}", path.display()))?;

    let config: Config = toml::from_str(&content)
        .context(format!("Failed to parse {}", path.display()))?;

    Ok(config)
}
