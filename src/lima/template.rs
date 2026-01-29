use anyhow::{Context, Result};

/// Lima テンプレート設定
#[derive(Debug, Clone)]
pub struct TemplateConfig {
    pub worktree_path: String,
    pub cpus: u32,
    pub memory: String,
    pub disk: String,
}

impl Default for TemplateConfig {
    fn default() -> Self {
        Self {
            worktree_path: String::new(),
            cpus: 4,
            memory: "8GiB".to_string(),
            disk: "50GiB".to_string(),
        }
    }
}

impl TemplateConfig {
    pub fn new(worktree_path: &str) -> Self {
        Self {
            worktree_path: worktree_path.to_string(),
            ..Default::default()
        }
    }
}

/// Lima VM テンプレートを生成
pub fn generate(config: &TemplateConfig) -> String {
    format!(
        r#"# fracta Lima VM template
# Auto-generated for worktree development

# VM configuration
cpus: {cpus}
memory: "{memory}"
disk: "{disk}"

# Use Ubuntu 24.04 LTS
images:
  - location: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-arm64.img"
    arch: "aarch64"
  - location: "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg-amd64.img"
    arch: "x86_64"

# Mount worktree directory
mounts:
  - location: "{worktree_path}"
    writable: true
    sshfs:
      cache: true
      followSymlinks: true

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

# Provisioning script for Docker
provision:
  - mode: system
    script: |
      #!/bin/bash
      set -eux -o pipefail

      # Ensure curl and certificates are available
      if ! command -v curl &> /dev/null; then
        apt-get update
        apt-get install -y --no-install-recommends curl ca-certificates
      fi

      # Install Docker if not present
      if ! command -v docker &> /dev/null; then
        curl -fsSL https://get.docker.com | sh
        usermod -aG docker "{{{{.User}}}}"
      fi

      # Allow passwordless sudo for the Lima user (development convenience)
      echo "{{{{.User}}}} ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/fracta-user
      chmod 0440 /etc/sudoers.d/fracta-user

      # Install docker-compose plugin if not present
      if ! docker compose version &> /dev/null; then
        DOCKER_CONFIG=${{DOCKER_CONFIG:-/usr/local/lib/docker}}
        mkdir -p $DOCKER_CONFIG/cli-plugins
        COMPOSE_VERSION=$(curl -s https://api.github.com/repos/docker/compose/releases/latest | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
        ARCH=$(uname -m)
        curl -SL "https://github.com/docker/compose/releases/download/${{COMPOSE_VERSION}}/docker-compose-linux-${{ARCH}}" -o $DOCKER_CONFIG/cli-plugins/docker-compose
        chmod +x $DOCKER_CONFIG/cli-plugins/docker-compose
      fi

      # Install Node.js and npm if not present (agent-browser dependency)
      if ! command -v node &> /dev/null; then
        apt-get update
        apt-get install -y --no-install-recommends nodejs npm
      fi

      # Install Japanese fonts (Noto)
      if ! command -v fc-list &> /dev/null; then
        apt-get update
        apt-get install -y --no-install-recommends fontconfig
      fi
      if ! fc-list | grep -qi "Noto"; then
        apt-get update
        apt-get install -y --no-install-recommends \
          fonts-noto \
          fonts-noto-cjk \
          fonts-noto-color-emoji
        fc-cache -f -v
      fi

      # Install uv (official installer)
      if ! command -v uv &> /dev/null; then
        curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="/usr/local/bin" UV_NO_MODIFY_PATH=1 sh
      fi
"#,
        cpus = config.cpus,
        memory = config.memory,
        disk = config.disk,
        worktree_path = config.worktree_path,
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
        let config = TemplateConfig::new("/home/user/project");
        let template = generate(&config);

        assert!(template.contains("cpus: 4"));
        assert!(template.contains("memory: \"8GiB\""));
        assert!(template.contains("disk: \"50GiB\""));
        assert!(template.contains("/home/user/project"));
        assert!(template.contains("vmType: \"vz\""));
        assert!(template.contains("curl -fsSL https://get.docker.com"));
    }

}
