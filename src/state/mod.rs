use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::utils;

/// ポートフォワード情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortForward {
    pub local_port: u16,
    pub remote_port: u16,
    pub pid: u32,
}

/// v2: Lima 統合版の Worktree 状態
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub lima_instance: String,
    #[serde(default)]
    pub active_forwards: Vec<PortForward>,
}

/// v2: Lima 統合版の状態
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct StateV2 {
    pub version: u32,
    pub instances: Vec<Instance>,
    #[serde(default)]
    pub port_allocations: HashMap<u16, String>,
}

/// v1: 旧形式の Worktree 状態（マイグレーション用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorktreeState {
    pub name: String,
    pub path: String,
    pub branch: String,
    pub port_offset: u16,
}

/// v1: 旧形式の状態（マイグレーション用）
#[derive(Debug, Default, Serialize, Deserialize)]
struct StateV1 {
    worktrees: Vec<WorktreeState>,
}

impl StateV2 {
    fn state_file_path(main_repo: &Path) -> PathBuf {
        main_repo.join(".fracta").join("state.json")
    }

    pub fn load(main_repo: &Path) -> Result<Self> {
        let path = Self::state_file_path(main_repo);
        if !path.exists() {
            return Ok(StateV2 {
                version: 2,
                instances: Vec::new(),
                port_allocations: HashMap::new(),
            });
        }

        let content = fs::read_to_string(&path)
            .context("Failed to read state file")?;

        // v2 形式でパース
        if let Ok(state) = serde_json::from_str::<StateV2>(&content) {
            if state.version == 2 {
                return Ok(state);
            }
        }

        // v1 形式からマイグレーション
        if let Ok(v1) = serde_json::from_str::<StateV1>(&content) {
            let state = migrate_v1_to_v2(v1);
            return Ok(state);
        }

        // パース失敗
        anyhow::bail!("Failed to parse state file: unsupported format")
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

    pub fn add_instance(&mut self, instance: Instance) {
        self.instances.push(instance);
    }

    pub fn remove_instance(&mut self, name: &str) {
        // 関連するポートフォワードを解放
        let ports_to_remove: Vec<u16> = self
            .instances
            .iter()
            .filter(|i| i.name == name)
            .flat_map(|i| i.active_forwards.iter().map(|f| f.local_port))
            .collect();

        for port in ports_to_remove {
            self.port_allocations.remove(&port);
        }
        self.instances.retain(|i| i.name != name);
    }

    pub fn find_instance(&self, name: &str) -> Option<&Instance> {
        self.instances.iter().find(|i| i.name == name)
    }

    pub fn find_instance_mut(&mut self, name: &str) -> Option<&mut Instance> {
        self.instances.iter_mut().find(|i| i.name == name)
    }

    pub fn resolve_instance(&self, name: Option<&str>) -> Result<&Instance> {
        match name {
            Some(name) => self
                .find_instance(name)
                .ok_or_else(|| anyhow::anyhow!("Instance '{}' not found", name)),
            None => {
                let cwd = std::env::current_dir()
                    .context("Failed to get current directory")?;
                let instance = self.instances.iter().find(|inst| {
                    utils::is_path_within(std::path::Path::new(&inst.path), &cwd)
                });

                match instance {
                    Some(inst) => Ok(inst),
                    None => anyhow::bail!("No instance found for current directory"),
                }
            }
        }
    }

    /// ポートフォワードを追加
    pub fn add_forward(&mut self, instance_name: &str, forward: PortForward) -> Result<()> {
        // まずポート割り当てを更新
        self.port_allocations.insert(forward.local_port, instance_name.to_string());

        // 次にインスタンスを更新
        let instance = self.find_instance_mut(instance_name)
            .ok_or_else(|| anyhow::anyhow!("Instance '{}' not found", instance_name))?;

        instance.active_forwards.push(forward);
        Ok(())
    }

    /// ポートフォワードを削除
    pub fn remove_forward(&mut self, instance_name: &str, local_port: u16) -> Result<Option<PortForward>> {
        // まずポート割り当てを削除
        self.port_allocations.remove(&local_port);

        // 次にインスタンスから削除
        let instance = self.find_instance_mut(instance_name)
            .ok_or_else(|| anyhow::anyhow!("Instance '{}' not found", instance_name))?;

        let idx = instance.active_forwards.iter()
            .position(|f| f.local_port == local_port);

        if let Some(idx) = idx {
            return Ok(Some(instance.active_forwards.remove(idx)));
        }

        Ok(None)
    }

    /// 全てのポートフォワードをクリア
    pub fn clear_forwards(&mut self, instance_name: &str) -> Result<Vec<PortForward>> {
        // まず削除するポートを収集
        let ports_to_remove: Vec<u16> = self
            .find_instance(instance_name)
            .map(|i| i.active_forwards.iter().map(|f| f.local_port).collect())
            .unwrap_or_default();

        // ポート割り当てを削除
        for port in ports_to_remove {
            self.port_allocations.remove(&port);
        }

        // インスタンスのフォワードをクリア
        let instance = self.find_instance_mut(instance_name)
            .ok_or_else(|| anyhow::anyhow!("Instance '{}' not found", instance_name))?;

        Ok(std::mem::take(&mut instance.active_forwards))
    }
}

/// v1 から v2 へのマイグレーション
fn migrate_v1_to_v2(v1: StateV1) -> StateV2 {
    let instances = v1.worktrees
        .into_iter()
        .map(|wt| {
            let sanitized = crate::utils::sanitize_name(&wt.name);
            Instance {
                name: wt.name,
                path: wt.path,
                branch: wt.branch,
                lima_instance: format!("fracta-{}", sanitized),
                active_forwards: Vec::new(),
            }
        })
        .collect();

    StateV2 {
        version: 2,
        instances,
        port_allocations: HashMap::new(),
    }
}

// v1 互換エイリアス（段階的移行用）
pub type State = StateV2;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_v2_default() {
        let state = StateV2::default();
        assert_eq!(state.version, 0);
        assert!(state.instances.is_empty());
        assert!(state.port_allocations.is_empty());
    }

    #[test]
    fn test_add_remove_instance() {
        let mut state = StateV2 {
            version: 2,
            instances: Vec::new(),
            port_allocations: HashMap::new(),
        };

        let instance = Instance {
            name: "test".to_string(),
            path: "/path/to/test".to_string(),
            branch: "main".to_string(),
            lima_instance: "fracta-test".to_string(),
            active_forwards: Vec::new(),
        };

        state.add_instance(instance);
        assert_eq!(state.instances.len(), 1);
        assert!(state.find_instance("test").is_some());

        state.remove_instance("test");
        assert!(state.instances.is_empty());
    }

    #[test]
    fn test_port_forward_management() {
        let mut state = StateV2 {
            version: 2,
            instances: vec![Instance {
                name: "test".to_string(),
                path: "/path/to/test".to_string(),
                branch: "main".to_string(),
                lima_instance: "fracta-test".to_string(),
                active_forwards: Vec::new(),
            }],
            port_allocations: HashMap::new(),
        };

        let fwd = PortForward {
            local_port: 22901,
            remote_port: 3000,
            pid: 12345,
        };

        state.add_forward("test", fwd).unwrap();
        assert_eq!(state.port_allocations.len(), 1);
        assert!(state.port_allocations.contains_key(&22901));

        let inst = state.find_instance("test").unwrap();
        assert_eq!(inst.active_forwards.len(), 1);

        state.remove_forward("test", 22901).unwrap();
        assert!(state.port_allocations.is_empty());
    }

    #[test]
    fn test_migrate_v1_to_v2() {
        let v1 = StateV1 {
            worktrees: vec![WorktreeState {
                name: "develop".to_string(),
                path: "/path/to/develop".to_string(),
                branch: "develop".to_string(),
                port_offset: 1000,
            }],
        };

        let v2 = migrate_v1_to_v2(v1);
        assert_eq!(v2.version, 2);
        assert_eq!(v2.instances.len(), 1);
        assert_eq!(v2.instances[0].name, "develop");
        assert_eq!(v2.instances[0].lima_instance, "fracta-develop");
    }
}
