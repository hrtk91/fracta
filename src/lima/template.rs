use anyhow::{Context, Result};
use std::path::Path;

/// Lima テンプレート設定
#[derive(Debug, Clone)]
pub struct TemplateConfig {
    pub worktree_path: String,
    pub cpus: u32,
    pub memory: String,
    pub disk: String,
    pub mount_type: String,
    pub user: String,
    pub provision_scripts: Vec<String>,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            worktree_path: String::new(),
            cpus: 4,
            memory: "8GiB".to_string(),
            disk: "50GiB".to_string(),
            mount_type: "virtiofs".to_string(),
            user: "lima".to_string(),
            provision_scripts: Vec::new(),
        }
    }
}

impl TemplateConfig {
    pub fn new(worktree_path: &str, mount_type: Option<&str>, user: Option<&str>) -> Self {
        let mut config = Self {
            worktree_path: worktree_path.to_string(),
            ..Default::default()
        };
        if let Some(mount_type) = mount_type {
            if !mount_type.trim().is_empty() {
                config.mount_type = mount_type.trim().to_string();
            }
        }
        if let Some(user) = user {
            if !user.trim().is_empty() {
                config.user = user.trim().to_string();
            }
        }
        config
    }

    /// fracta.toml の vm_provision_scripts からスクリプト内容を読み込む
    pub fn load_provision_scripts(
        &mut self,
        script_paths: &[String],
        base_dir: &Path,
    ) -> Result<()> {
        for path_str in script_paths {
            let script_path = if Path::new(path_str).is_absolute() {
                std::path::PathBuf::from(path_str)
            } else {
                base_dir.join(path_str)
            };
            let content = std::fs::read_to_string(&script_path).context(format!(
                "Failed to read provision script: {}",
                script_path.display()
            ))?;
            self.provision_scripts.push(content);
        }
        Ok(())
    }
}

/// provision セクションを生成
fn generate_provision(config: &TemplateConfig) -> String {
    let mut provisions = String::new();

    // 基本: sudo 設定
    provisions.push_str(
        r#"  - mode: system
    script: |
      #!/bin/bash
      set -eux -o pipefail
      echo "{{.User}} ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/fracta-user
      chmod 0440 /etc/sudoers.d/fracta-user
"#,
    );

    // ユーザー指定のプロビジョニングスクリプト（冪等性マーカー付き）
    for (i, script) in config.provision_scripts.iter().enumerate() {
        // スクリプト内容のハッシュでマーカーを作る
        let hash = simple_hash(script);
        let marker = format!("/var/lib/fracta/provisioned/{}", hash);

        let indented_body = indent_script(script);
        provisions.push_str(&format!(
            r#"  - mode: system
    script: |
      #!/bin/bash
      set -eux -o pipefail
      # Provision script {} (hash: {})
      if [ -f '{}' ]; then
        echo "Already provisioned ({}), skipping"
        exit 0
      fi
{}
      mkdir -p /var/lib/fracta/provisioned
      touch '{}'
"#,
            i + 1,
            hash,
            marker,
            hash,
            indented_body,
            marker,
        ));
    }

    provisions
}

/// probe セクションを生成（provision スクリプトがある場合のみ）
fn generate_probes(config: &TemplateConfig) -> String {
    if config.provision_scripts.is_empty() {
        return String::new();
    }

    // 最後のプロビジョニングスクリプトのマーカーを待つ
    let last_hash = simple_hash(config.provision_scripts.last().unwrap());
    let marker = format!("/var/lib/fracta/provisioned/{}", last_hash);

    format!(
        r#"
# Wait for provisioning to complete
probes:
  - script: |
      #!/bin/bash
      if ! timeout 600s bash -c "until [ -f '{}' ]; do sleep 5; done"; then
        echo "Provisioning did not complete in time"
        exit 1
      fi
    hint: "Waiting for provisioning to complete..."
"#,
        marker,
    )
}

/// スクリプトを provision ブロック内のインデントに合わせる
fn indent_script(script: &str) -> String {
    let trimmed = script.trim();
    // shebang を除去（provision の script: | ブロック内では不要、既に #!/bin/bash がある）
    let body = if trimmed.starts_with("#!") {
        trimmed
            .lines()
            .skip(1)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string()
    } else {
        trimmed.to_string()
    };

    body.lines()
        .map(|line| format!("      {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// 簡易ハッシュ（マーカーファイル名用）
fn simple_hash(s: &str) -> String {
    // FNV-1a 64bit
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// Lima VM テンプレートを生成
pub fn generate(config: &TemplateConfig) -> String {
    let mount_block = if config.mount_type == "sshfs" {
        format!(
            r#"  - location: "{worktree_path}"
    writable: true
    sshfs:
      cache: true
      followSymlinks: true
"#,
            worktree_path = config.worktree_path
        )
    } else {
        format!(
            r#"  - location: "{worktree_path}"
    writable: true
"#,
            worktree_path = config.worktree_path
        )
    };

    let provision_block = generate_provision(config);
    let probes_block = generate_probes(config);

    format!(
        r#"# fracta Lima VM template
# Auto-generated for worktree development

# VM configuration
cpus: {cpus}
memory: "{memory}"
disk: "{disk}"

# Default user (dev convenience)
user:
  name: "{user}"

# Use Ubuntu 24.04 LTS
images:
  - location: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-arm64.img"
    arch: "aarch64"
  - location: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-amd64.img"
    arch: "x86_64"

# Mount worktree directory
mounts:
{mount_block}

# VM type: vz for Apple Silicon
vmType: "vz"
rosetta:
  enabled: true
  binfmt: true

# Network configuration
networks:
  - vzNAT: true

# Disable automatic port forwarding (use fracta forward instead)
portForwards:
  - ignore: true
    proto: any
    guestIP: 0.0.0.0

# Docker installation via containerd
containerd:
  system: false
  user: false

# Provisioning
provision:
{provision_block}{probes_block}
"#,
        cpus = config.cpus,
        memory = config.memory,
        disk = config.disk,
        user = config.user,
        mount_block = mount_block.trim_end(),
        provision_block = provision_block.trim_end(),
        probes_block = probes_block,
    )
}

/// 一時テンプレートファイルを作成
pub fn create_temp_template(config: &TemplateConfig) -> Result<tempfile::NamedTempFile> {
    let content = generate(config);

    let mut temp = tempfile::Builder::new()
        .prefix("fracta-lima-")
        .suffix(".yaml")
        .tempfile()
        .context("Failed to create temp file")?;

    std::io::Write::write_all(&mut temp, content.as_bytes())
        .context("Failed to write temp template")?;

    Ok(temp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_config_default() {
        let config = TemplateConfig::default();
        assert_eq!(config.cpus, 4);
        assert_eq!(config.memory, "8GiB");
        assert_eq!(config.disk, "50GiB");
    }

    #[test]
    fn test_generate_template() {
        let config = TemplateConfig::new("/home/user/project", None, None);
        let template = generate(&config);

        assert!(template.contains("cpus: 4"));
        assert!(template.contains("memory: \"8GiB\""));
        assert!(template.contains("disk: \"50GiB\""));
        assert!(template.contains("user:\n  name: \"lima\""));
        assert!(template.contains("/home/user/project"));
        assert!(template.contains("vmType: \"vz\""));
        assert!(!template.contains("mountType"));
    }

    #[test]
    fn test_generate_with_provision() {
        let mut config = TemplateConfig::new("/home/user/project", None, None);
        config.provision_scripts = vec![
            "#!/bin/bash\napt-get update\napt-get install -y curl".to_string(),
        ];
        let template = generate(&config);

        assert!(template.contains("Already provisioned"));
        assert!(template.contains("apt-get update"));
        assert!(template.contains("apt-get install -y curl"));
        assert!(template.contains("probes:"));
        assert!(template.contains("timeout 600s"));
    }

    #[test]
    fn test_simple_hash_deterministic() {
        let h1 = simple_hash("hello");
        let h2 = simple_hash("hello");
        let h3 = simple_hash("world");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 16);
    }

    #[test]
    fn test_indent_script_strips_shebang() {
        let script = "#!/bin/bash\necho hello\necho world";
        let indented = indent_script(script);
        assert!(!indented.contains("#!/bin/bash"));
        assert!(indented.contains("      echo hello"));
    }
}
