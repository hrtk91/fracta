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
    /// カスタムテンプレートファイルのパス（None ならデフォルト）
    pub custom_template: Option<String>,
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
            custom_template: None,
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

    /// カスタムテンプレートを解決する
    /// 優先順: vm_template 設定 > .fracta/lima-template.yaml > 内蔵デフォルト
    pub fn resolve_template(
        &mut self,
        vm_template: Option<&str>,
        main_repo: &Path,
        worktree_path: &Path,
    ) {
        if let Some(path) = vm_template {
            let resolved = if Path::new(path).is_absolute() {
                std::path::PathBuf::from(path)
            } else {
                main_repo.join(path)
            };
            if resolved.exists() {
                self.custom_template = Some(resolved.to_string_lossy().to_string());
                return;
            }
        }

        // .fracta/lima-template.yaml を探す（worktree → main_repo の順）
        for dir in &[worktree_path, main_repo] {
            let candidate = dir.join(".fracta").join("lima-template.yaml");
            if candidate.exists() {
                self.custom_template = Some(candidate.to_string_lossy().to_string());
                return;
            }
        }
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

    provisions.push_str(&generate_user_provisions(&config.provision_scripts));

    provisions
}

/// ユーザー指定のプロビジョニングスクリプト（冪等性マーカー付き）を生成
fn generate_user_provisions(scripts: &[String]) -> String {
    let mut provisions = String::new();

    for (i, script) in scripts.iter().enumerate() {
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
      # Wait for DNS to be available
      echo "Waiting for network..."
      for i in $(seq 1 60); do
        if nslookup archive.ubuntu.com > /dev/null 2>&1; then
          echo "Network ready"
          break
        fi
        sleep 5
      done
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
fn generate_probes(scripts: &[String]) -> String {
    if scripts.is_empty() {
        return String::new();
    }

    let last_hash = simple_hash(scripts.last().unwrap());
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
pub fn simple_hash(s: &str) -> String {
    // FNV-1a 64bit
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{:016x}", hash)
}

/// デフォルトの Lima VM テンプレートを生成
pub fn generate_default(config: &TemplateConfig) -> String {
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
    let probes_block = generate_probes(&config.provision_scripts);

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

/// カスタムテンプレートに provision スクリプトを注入
fn inject_provisions_into_template(
    template: &str,
    scripts: &[String],
) -> String {
    if scripts.is_empty() {
        return template.to_string();
    }

    let user_provisions = generate_user_provisions(scripts);
    let probes = generate_probes(scripts);

    let mut result = template.to_string();

    // provision: セクションがあればその末尾に追加
    if let Some(pos) = find_yaml_section_end(&result, "provision:") {
        result.insert_str(pos, &user_provisions);
    } else {
        // provision セクションがなければ末尾に追加
        result.push_str("\nprovision:\n");
        result.push_str(&user_provisions);
    }

    // probes セクションを追加（既存があれば末尾に）
    if let Some(pos) = find_yaml_section_end(&result, "probes:") {
        let probe_entries = probes
            .trim_start_matches('\n')
            .trim_start_matches("# Wait for provisioning to complete\n")
            .trim_start_matches("probes:\n");
        result.insert_str(pos, probe_entries);
    } else {
        result.push_str(&probes);
    }

    result
}

/// YAML のトップレベルセクションの末尾位置を見つける
fn find_yaml_section_end(yaml: &str, section: &str) -> Option<usize> {
    let section_start = yaml.find(section)?;
    let after_section = section_start + section.len();

    // セクション開始以降の行を見て、次のトップレベルキーを探す
    let remaining = &yaml[after_section..];
    for (i, line) in remaining.lines().enumerate() {
        if i == 0 {
            continue; // セクション行自体はスキップ
        }
        // 空行でもインデントされた行でもない = 次のトップレベルキー
        if !line.is_empty()
            && !line.starts_with(' ')
            && !line.starts_with('#')
            && !line.starts_with('\t')
        {
            // この行の開始位置を返す
            let line_start = remaining[..remaining.find(line).unwrap_or(0)].len();
            return Some(after_section + line_start);
        }
    }

    // セクションが末尾まで続いている
    Some(yaml.len())
}

/// テンプレートを生成（カスタム or デフォルト）
pub fn generate(config: &TemplateConfig) -> String {
    if let Some(template_path) = &config.custom_template {
        if let Ok(custom) = std::fs::read_to_string(template_path) {
            // カスタムテンプレートに worktree_path を置換
            let processed = custom
                .replace("{{WORKTREE_PATH}}", &config.worktree_path)
                .replace("{{USER}}", &config.user)
                .replace("{{CPUS}}", &config.cpus.to_string())
                .replace("{{MEMORY}}", &config.memory)
                .replace("{{DISK}}", &config.disk);
            return inject_provisions_into_template(&processed, &config.provision_scripts);
        }
    }
    generate_default(config)
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

    #[test]
    fn test_inject_provisions_into_custom_template() {
        let template = r#"vmType: "vz"
provision:
  - mode: system
    script: |
      echo "custom setup"
networks:
  - vzNAT: true
"#;
        let scripts = vec!["echo 'hello'".to_string()];
        let result = inject_provisions_into_template(template, &scripts);

        assert!(result.contains("custom setup"));
        assert!(result.contains("echo 'hello'"));
        assert!(result.contains("probes:"));
    }

    #[test]
    fn test_custom_template_placeholders() {
        let mut config = TemplateConfig::new("/my/worktree", None, None);
        config.custom_template = None; // will use default
        let template = generate(&config);
        assert!(template.contains("/my/worktree"));
    }
}
