use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeState {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub port_offset: u16,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct State {
    pub worktrees: Vec<WorktreeState>,
}

impl State {
    fn state_file_path(main_repo: &Path) -> PathBuf {
        main_repo.join(".fracta").join("state.json")
    }

    pub fn load(main_repo: &Path) -> Result<Self> {
        let path = Self::state_file_path(main_repo);
        if !path.exists() {
            return Ok(State::default());
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read state file")?;

        let state: State = serde_json::from_str(&content)
            .context("Failed to parse state file")?;

        Ok(state)
    }

    pub fn save(&self, main_repo: &Path) -> Result<()> {
        let path = Self::state_file_path(main_repo);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .context("Failed to create .fracta directory")?;
        }

        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize state")?;

        fs::write(&path, content)
            .context("Failed to write state file")?;

        Ok(())
    }

    pub fn add_worktree(&mut self, worktree: WorktreeState) {
        self.worktrees.push(worktree);
    }

    pub fn remove_worktree(&mut self, name: &str) {
        self.worktrees.retain(|w| w.name != name);
    }

    pub fn find_worktree(&self, name: &str) -> Option<&WorktreeState> {
        self.worktrees.iter().find(|w| w.name == name)
    }

}
