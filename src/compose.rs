use anyhow::{Context, Result};
use serde_yaml::{Mapping, Value};
use std::collections::HashMap;
use std::path::Path;

pub struct ComposeResult {
    pub yaml: String,
    pub warnings: Vec<String>,
}

pub struct PortEntry {
    pub service: String,
    pub host: String,
    pub target: String,
    pub link: Option<String>,
}

pub fn extract_ports(compose_file: &Path) -> Result<Vec<PortEntry>> {
    let content = std::fs::read_to_string(compose_file)
        .context(format!("Failed to read {}", compose_file.display()))?;

    let base: Value = serde_yaml::from_str(&content)
        .context(format!("Failed to parse {}", compose_file.display()))?;

    let services = match base.get("services").and_then(Value::as_mapping) {
        Some(services) => services,
        None => return Ok(Vec::new()),
    };

    let mut entries = Vec::new();

    for (service_name, service_value) in services {
        let ports_value = match service_value.get("ports") {
            Some(value) => value,
            None => continue,
        };
        let ports = match ports_value.as_sequence() {
            Some(seq) => seq,
            None => continue,
        };

        let service = service_name.as_str().unwrap_or("unknown").to_string();

        for entry in ports {
            let parsed = match entry {
                Value::String(value) => parse_short_port_entry(value),
                Value::Mapping(map) => parse_long_port_entry(map),
                _ => None,
            };

            if let Some((host, target)) = parsed {
                let link = host_to_link(&host);
                entries.push(PortEntry {
                    service: service.clone(),
                    host,
                    target,
                    link,
                });
            }
        }
    }

    Ok(entries)
}

pub fn format_ports_table(entries: &[PortEntry]) -> String {
    let mut output = String::new();
    output.push_str(&format!("{:<22} {:<18} {:<18} {}\n", "SERVICE", "HOST", "TARGET", "LINK"));
    output.push_str(&format!("{}\n", "-".repeat(80)));

    for entry in entries {
        let link = entry.link.as_deref().unwrap_or("");
        output.push_str(&format!(
            "{:<22} {:<18} {:<18} {}\n",
            entry.service, entry.host, entry.target, link
        ));
    }

    output.trim_end().to_string()
}

pub fn generate_compose(
    compose_base: &Path,
    port_offset: u16,
    worktree_name: &str,
    env: &HashMap<String, String>,
) -> Result<ComposeResult> {
    let content = std::fs::read_to_string(compose_base)
        .context(format!("Failed to read {}", compose_base.display()))?;

    let mut base: Value = serde_yaml::from_str(&content)
        .context(format!("Failed to parse {}", compose_base.display()))?;

    let mut warnings = Vec::new();

    let services = match base.get_mut("services").and_then(Value::as_mapping_mut) {
        Some(services) => services,
        None => {
            warnings.push("No services found in compose base".to_string());
            let yaml = serde_yaml::to_string(&base)
                .context("Failed to serialize compose")?;
            return Ok(ComposeResult { yaml, warnings });
        }
    };

    for (_service_name, service_value) in services.iter_mut() {
        let service_map = match service_value.as_mapping_mut() {
            Some(map) => map,
            None => continue,
        };

        if let Some(ports_value) = service_map.get("ports") {
            if let Some((new_ports, ports_changed)) =
                rewrite_ports(ports_value, port_offset, env, &mut warnings)
            {
                if ports_changed {
                    service_map.insert(
                        Value::String("ports".to_string()),
                        Value::Sequence(new_ports),
                    );
                }
            }
        }

        if let Some(container_name) = service_map.get("container_name") {
            if let Some(new_name) =
                rewrite_container_name(container_name, worktree_name, &mut warnings)
            {
                service_map.insert(Value::String("container_name".to_string()), new_name);
            }
        }
    }

    let yaml = serde_yaml::to_string(&base)
        .context("Failed to serialize compose")?;

    Ok(ComposeResult { yaml, warnings })
}

fn rewrite_ports(
    ports_value: &Value,
    port_offset: u16,
    env: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Option<(Vec<Value>, bool)> {
    let ports = match ports_value.as_sequence() {
        Some(seq) => seq,
        None => {
            warnings.push("ports is not a list; skipping".to_string());
            return None;
        }
    };

    let mut changed = false;
    let mut new_ports = Vec::with_capacity(ports.len());

    for entry in ports {
        match entry {
            Value::String(value) => {
                let (new_value, did_change) =
                    rewrite_short_port(value, port_offset, env, warnings);
                new_ports.push(Value::String(new_value));
                if did_change {
                    changed = true;
                }
            }
            Value::Mapping(map) => {
                let (new_map, did_change) = rewrite_long_port(map, port_offset, env, warnings);
                new_ports.push(Value::Mapping(new_map));
                if did_change {
                    changed = true;
                }
            }
            _ => {
                warnings.push("ports entry has unsupported type; skipping".to_string());
                new_ports.push(entry.clone());
            }
        }
    }

    Some((new_ports, changed))
}

fn rewrite_short_port(
    value: &str,
    port_offset: u16,
    env: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> (String, bool) {
    let (main, proto) = match value.split_once('/') {
        Some((main, proto)) => (main, Some(proto)),
        None => (value, None),
    };

    let parts: Vec<&str> = main.split(':').collect();
    let (ip, host, container) = match parts.len() {
        1 => return (value.to_string(), false),
        2 => (None, parts[0], parts[1]),
        3 => (Some(parts[0]), parts[1], parts[2]),
        _ => {
            warnings.push(format!("Unsupported port format: {}", value));
            return (value.to_string(), false);
        }
    };

    let host_num = match resolve_host_port(host, env, warnings) {
        Some(num) => num,
        None => return (value.to_string(), false),
    };

    let new_host = host_num + u32::from(port_offset);
    if new_host > 65535 {
        warnings.push(format!("Port overflow: {}", value));
        return (value.to_string(), false);
    }

    let mut rebuilt = String::new();
    if let Some(ip) = ip {
        rebuilt.push_str(ip);
        rebuilt.push(':');
    }
    rebuilt.push_str(&new_host.to_string());
    rebuilt.push(':');
    rebuilt.push_str(container);

    let result = match proto {
        Some(proto) => format!("{}/{}", rebuilt, proto),
        None => rebuilt,
    };

    (result, true)
}

fn rewrite_long_port(
    map: &Mapping,
    port_offset: u16,
    env: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> (Mapping, bool) {
    let mut new_map = map.clone();
    let key = Value::String("published".to_string());

    let value = match new_map.get(&key) {
        Some(value) => value.clone(),
        None => return (new_map, false),
    };

    let (published, is_string) = match value {
        Value::Number(num) => match num.as_u64() {
            Some(num) => (num, false),
            None => {
                warnings.push("Non-integer published port; skipping".to_string());
                return (new_map, false);
            }
        },
        Value::String(text) => match resolve_env_number(&text, env, warnings) {
            Some(num) => (num, true),
            None => return (new_map, false),
        },
        _ => {
            warnings.push("Unsupported published port type".to_string());
            return (new_map, false);
        }
    };

    let new_published = published + u64::from(port_offset);
    if new_published > 65535 {
        warnings.push("Published port overflow".to_string());
        return (new_map, false);
    }

    let new_value = if is_string {
        Value::String(new_published.to_string())
    } else {
        Value::Number(new_published.into())
    };

    new_map.insert(key, new_value);

    (new_map, true)
}

fn sanitize_container_name(name: &str) -> String {
    // utils::sanitize_nameを再利用
    crate::utils::sanitize_name(name)
}

fn rewrite_container_name(
    value: &Value,
    worktree_name: &str,
    warnings: &mut Vec<String>,
) -> Option<Value> {
    let base = match value.as_str() {
        Some(name) => name,
        None => {
            warnings.push("container_name is not a string; skipping".to_string());
            return None;
        }
    };

    if base.is_empty() || worktree_name.is_empty() {
        return None;
    }

    let sanitized_name = sanitize_container_name(worktree_name);
    let suffix = format!("-{}", sanitized_name);
    if base.ends_with(&suffix) {
        return None;
    }

    Some(Value::String(format!("{}{}", base, suffix)))
}

fn parse_short_port_entry(value: &str) -> Option<(String, String)> {
    let (main, proto) = match value.split_once('/') {
        Some((main, proto)) => (main, Some(proto)),
        None => (value, None),
    };

    let parts: Vec<&str> = main.split(':').collect();
    let (host, target) = match parts.len() {
        1 => return None,
        2 => (parts[0].to_string(), parts[1].to_string()),
        3 => (format!("{}:{}", parts[0], parts[1]), parts[2].to_string()),
        _ => return None,
    };

    let target = match proto {
        Some(proto) => format!("{}/{}", target, proto),
        None => target,
    };

    Some((host, target))
}

fn parse_long_port_entry(map: &Mapping) -> Option<(String, String)> {
    let published = map.get("published")?;
    let target = map.get("target")?;

    let host = match published {
        Value::Number(num) => num.as_u64()?.to_string(),
        Value::String(text) => text.to_string(),
        _ => return None,
    };

    let mut target = match target {
        Value::Number(num) => num.as_u64()?.to_string(),
        Value::String(text) => text.to_string(),
        _ => return None,
    };

    if let Some(Value::String(proto)) = map.get("protocol") {
        if !proto.is_empty() {
            target = format!("{}/{}", target, proto);
        }
    }

    Some((host, target))
}

fn host_to_link(host: &str) -> Option<String> {
    if host.chars().all(|c| c.is_ascii_digit()) {
        return Some(format!("http://localhost:{}", host));
    }

    if let Some((ip, port)) = host.rsplit_once(':') {
        if !port.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let resolved_ip = if ip == "0.0.0.0" { "localhost" } else { ip };
        return Some(format!("http://{}:{}", resolved_ip, port));
    }

    None
}

fn resolve_host_port(
    host: &str,
    env: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Option<u32> {
    if let Some(num) = resolve_env_number(host, env, warnings) {
        if num > u64::from(u16::MAX) {
            warnings.push(format!("Host port overflow: {}", host));
            return None;
        }
        return Some(num as u32);
    }

    if is_port_range(host) {
        warnings.push(format!("Port range not supported: {}", host));
        return None;
    }

    warnings.push(format!("Non-numeric host port: {}", host));
    None
}

fn resolve_env_number(
    value: &str,
    env: &HashMap<String, String>,
    warnings: &mut Vec<String>,
) -> Option<u64> {
    if value.chars().all(|c| c.is_ascii_digit()) {
        return value.parse::<u64>().ok();
    }

    let expr = match parse_env_expr(value) {
        Some(expr) => expr,
        None => return None,
    };

    let resolved = match resolve_env_expr(&expr, env) {
        Some(val) => val,
        None => {
            warnings.push(format!("Unresolved env port: {}", value));
            return None;
        }
    };

    if !resolved.chars().all(|c| c.is_ascii_digit()) {
        warnings.push(format!("Non-numeric env port: {}", value));
        return None;
    }

    resolved.parse::<u64>().ok()
}

fn is_port_range(value: &str) -> bool {
    let mut parts = value.splitn(3, '-');
    let first = match parts.next() {
        Some(part) => part,
        None => return false,
    };
    let second = match parts.next() {
        Some(part) => part,
        None => return false,
    };
    if parts.next().is_some() {
        return false;
    }
    !first.is_empty()
        && !second.is_empty()
        && first.chars().all(|c| c.is_ascii_digit())
        && second.chars().all(|c| c.is_ascii_digit())
}

#[derive(Debug)]
enum EnvExpr {
    Simple { name: String },
    Default { name: String, default: String, treat_empty: bool },
}

fn parse_env_expr(input: &str) -> Option<EnvExpr> {
    if !input.starts_with("${") || !input.ends_with('}') {
        return None;
    }

    let inner = &input[2..input.len() - 1];
    if inner.is_empty() {
        return None;
    }

    if let Some((name, default)) = inner.split_once(":-") {
        if name.is_empty() {
            return None;
        }
        return Some(EnvExpr::Default {
            name: name.to_string(),
            default: default.to_string(),
            treat_empty: true,
        });
    }

    if let Some((name, default)) = inner.split_once('-') {
        if name.is_empty() {
            return None;
        }
        return Some(EnvExpr::Default {
            name: name.to_string(),
            default: default.to_string(),
            treat_empty: false,
        });
    }

    Some(EnvExpr::Simple {
        name: inner.to_string(),
    })
}

fn resolve_env_expr(expr: &EnvExpr, env: &HashMap<String, String>) -> Option<String> {
    match expr {
        EnvExpr::Simple { name } => env.get(name).cloned(),
        EnvExpr::Default {
            name,
            default,
            treat_empty,
        } => match env.get(name) {
            Some(value) => {
                if *treat_empty && value.is_empty() {
                    Some(default.clone())
                } else {
                    Some(value.clone())
                }
            }
            None => Some(default.clone()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_rewrite_short_port() {
        let mut warnings = Vec::new();
        let env = HashMap::new();
        let (value, changed) = rewrite_short_port("8080:80", 1000, &env, &mut warnings);
        assert_eq!(value, "9080:80");
        assert!(changed);

        let (value, changed) =
            rewrite_short_port("127.0.0.1:8080:80", 1000, &env, &mut warnings);
        assert_eq!(value, "127.0.0.1:9080:80");
        assert!(changed);

        let (value, changed) = rewrite_short_port("8080:80/tcp", 1000, &env, &mut warnings);
        assert_eq!(value, "9080:80/tcp");
        assert!(changed);

        let (value, changed) = rewrite_short_port("3000", 1000, &env, &mut warnings);
        assert_eq!(value, "3000");
        assert!(!changed);
    }

    #[test]
    fn test_rewrite_long_port() {
        let mut warnings = Vec::new();
        let env = HashMap::new();
        let mut map = Mapping::new();
        map.insert(Value::String("target".to_string()), Value::Number(80.into()));
        map.insert(Value::String("published".to_string()), Value::Number(8080.into()));

        let (new_map, changed) = rewrite_long_port(&map, 1000, &env, &mut warnings);
        assert!(changed);
        assert_eq!(
            new_map.get(&Value::String("published".to_string())),
            Some(&Value::Number(9080.into()))
        );
    }

    #[test]
    fn test_rewrite_env_default() {
        let mut warnings = Vec::new();
        let env = HashMap::new();
        let (value, changed) =
            rewrite_short_port("${BACKEND_PORT-12910}:12910", 1000, &env, &mut warnings);
        assert_eq!(value, "13910:12910");
        assert!(changed);
    }

    #[test]
    fn test_rewrite_env_value() {
        let mut warnings = Vec::new();
        let mut env = HashMap::new();
        env.insert("BACKEND_PORT".to_string(), "12000".to_string());
        let (value, changed) =
            rewrite_short_port("${BACKEND_PORT-12910}:12910", 1000, &env, &mut warnings);
        assert_eq!(value, "13000:12910");
        assert!(changed);
    }

    #[test]
    fn test_sanitize_container_name() {
        assert_eq!(sanitize_container_name("develop3"), "develop3");
        assert_eq!(sanitize_container_name("feature/new-feature"), "feature-new-feature");
        assert_eq!(sanitize_container_name("bugfix/issue-123"), "bugfix-issue-123");
        assert_eq!(sanitize_container_name("feature//double-slash"), "feature-double-slash");
        assert_eq!(sanitize_container_name("/leading-slash"), "leading-slash");
        assert_eq!(sanitize_container_name("trailing-slash/"), "trailing-slash");
    }

    #[test]
    fn test_rewrite_container_name() {
        let mut warnings = Vec::new();
        let value = Value::String("localstack-main".to_string());
        let rewritten = rewrite_container_name(&value, "develop3", &mut warnings);
        assert_eq!(
            rewritten,
            Some(Value::String("localstack-main-develop3".to_string()))
        );
    }

    #[test]
    fn test_rewrite_container_name_with_slash() {
        let mut warnings = Vec::new();
        let value = Value::String("localstack-main".to_string());
        let rewritten = rewrite_container_name(&value, "feature/new-feature", &mut warnings);
        assert_eq!(
            rewritten,
            Some(Value::String("localstack-main-feature-new-feature".to_string()))
        );
    }

    #[test]
    fn test_extract_ports_short() {
        let yaml = r#"
services:
  app:
    ports:
      - 8080:80
      - 127.0.0.1:9000:9000/tcp
"#;
        let mut path = std::env::temp_dir();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        path.push(format!("fracta-test-{}.yml", nonce));
        std::fs::write(&path, yaml).unwrap();
        let ports = extract_ports(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].service, "app");
        assert_eq!(ports[0].host, "8080");
        assert_eq!(ports[0].target, "80");
        assert_eq!(ports[1].host, "127.0.0.1:9000");
        assert_eq!(ports[1].target, "9000/tcp");
    }
}
