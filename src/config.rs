use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Category {
    pub name: String,
    pub icon: Option<String>,
    #[serde(default)]
    pub hosts: Vec<Host>,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub ip: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub cmd: Option<String>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = default_config_path();
        if !path.exists() {
            return Ok(Self::default());
        }

        Self::load_from_path(path)
    }

    pub fn load_from_path(path: PathBuf) -> Result<Self> {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;

        let cfg = serde_yaml::from_str::<Self>(&content)
            .with_context(|| format!("invalid YAML in config file: {}", path.display()))?;

        Ok(cfg)
    }
}

pub fn default_config_path() -> PathBuf {
    let mut path = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));

    path.push("hoppr");
    path.push("config.yaml");
    path
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::TempDir;

    use super::Config;

    #[test]
    fn load_returns_default_when_file_missing() {
        let cfg = Config::load_from_path(PathBuf::from("/tmp/does-not-exist-hoppr.yaml"));
        assert!(cfg.is_err());

        let empty = Config::default();
        assert!(empty.categories.is_empty());
    }

    #[test]
    fn load_parses_valid_yaml() {
        let tmp = TempDir::new().expect("tempdir should be created");
        let path = tmp.path().join("config.yaml");
        fs::write(
            &path,
            r#"
categories:
  - name: Infrastructure
    icon: "🚀"
    hosts:
      - name: gateway
        ip: 10.0.0.1
        user: admin
        port: 2222
"#,
        )
        .expect("yaml should be written");

        let cfg = Config::load_from_path(path).expect("yaml should parse");
        assert_eq!(cfg.categories.len(), 1);
        assert_eq!(cfg.categories[0].hosts[0].name, "gateway");
        assert_eq!(cfg.categories[0].hosts[0].port, Some(2222));
    }
}
