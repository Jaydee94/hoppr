//! Configuration loading, saving and defaults for hoppr.
//!
//! The on-disk representation is YAML so users can hand-edit and version-control
//! the file in a central git repository (see [`crate::sync`]). The same struct is
//! also used by the in-app settings editor.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

const QUALIFIER: &str = "dev";
const ORG: &str = "hoppr";
const APP: &str = "hoppr";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub defaults: Defaults,
    pub sync: Option<SyncConfig>,
    pub categories: Vec<Category>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Defaults {
    /// Default connection command when a host does not provide its own
    /// `cmd` or `command_template`. Supports `{user}`, `{host}`, `{port}`
    /// placeholders. Defaults to plain `ssh`.
    pub command: ConnectCommand,
    /// Default SSH port — only used when the chosen command template
    /// includes the `{port}` placeholder.
    pub port: u16,
    /// Default username when a host omits `user` and `$USER` is empty.
    pub user: Option<String>,
    /// Terminal emulator used by "open in new window" (Shift+Enter).
    /// When unset, hoppr auto-detects from the environment.
    /// Examples: `"wt"`, `"gnome-terminal"`, `"xterm"`, `"alacritty -e"`.
    pub terminal_command: Option<String>,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            command: ConnectCommand::default(),
            port: 22,
            user: None,
            terminal_command: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum ConnectCommand {
    /// Shorthand: just a program name, hoppr expands the default arguments.
    Program(String),
    /// Full template form. `args` may contain placeholders.
    Template {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl Default for ConnectCommand {
    fn default() -> Self {
        Self::Program("ssh".into())
    }
}

impl ConnectCommand {
    pub fn program(&self) -> &str {
        match self {
            Self::Program(p) => p,
            Self::Template { program, .. } => program,
        }
    }

    pub fn args(&self) -> Option<&[String]> {
        match self {
            Self::Program(_) => None,
            Self::Template { args, .. } => Some(args),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SyncConfig {
    /// Remote git URL hosting the hoppr config (HTTPS or SSH).
    pub repo: Option<String>,
    /// Branch to track. Defaults to `main`.
    pub branch: Option<String>,
    /// File path inside the repo. Defaults to `config.yaml`.
    pub path: Option<String>,
    /// Local clone directory. Defaults to the user data dir.
    pub local: Option<String>,
    /// Automatically pull on startup. Defaults to `true` when sync is enabled.
    pub auto_pull: Option<bool>,
    /// Automatically commit & push when the TUI saves changes.
    /// Defaults to `false` — opt-in.
    pub auto_push: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Category {
    pub name: String,
    pub icon: Option<String>,
    pub hosts: Vec<Host>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct Host {
    pub name: String,
    pub ip: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    /// Raw shell command — when set, takes precedence over the template.
    pub cmd: Option<String>,
    /// Per-host override of the default connect command.
    pub command: Option<ConnectCommand>,
}

impl Config {
    #[allow(dead_code)]
    pub fn load() -> Result<Self> {
        let path = default_config_path();
        Self::load_or_default(&path)
    }

    pub fn load_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load_from_path(path)
    }

    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read config file: {}", path.display()))?;

        if content.trim().is_empty() {
            return Ok(Self::default());
        }

        serde_yaml::from_str::<Self>(&content)
            .with_context(|| format!("invalid YAML in config file: {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir: {}", parent.display()))?;
        }
        let yaml = serde_yaml::to_string(self).context("failed to serialize config")?;

        // Atomic write: stage in a sibling tempfile, then rename. This avoids
        // truncating the user's config if hoppr crashes mid-write.
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_else(|| std::ffi::OsString::from("config.yaml"));
        let mut tmp = parent.join(&file_name);
        tmp.as_mut_os_string().push(".tmp");

        fs::write(&tmp, yaml)
            .with_context(|| format!("failed to stage config file: {}", tmp.display()))?;
        restrict_perms(&tmp).ok();
        fs::rename(&tmp, path)
            .with_context(|| format!("failed to write config file: {}", path.display()))?;
        Ok(())
    }

    pub fn sync_enabled(&self) -> bool {
        self.sync.as_ref().and_then(|s| s.repo.as_deref()).is_some()
    }

    /// Replace the in-memory host inventory (categories + hosts) with the one
    /// pulled from the central repo. Local `defaults` and `sync` settings are
    /// left untouched — those are per-machine.
    pub fn apply_inventory(&mut self, inv: Inventory) {
        self.categories = inv.categories;
    }

    /// Extract the shared subset that gets pushed to the central repo. Sync
    /// configuration and connection defaults stay on the local machine.
    pub fn to_inventory(&self) -> Inventory {
        Inventory {
            categories: self.categories.clone(),
        }
    }
}

/// The shared host inventory — the slice of the configuration that lives in
/// the central git repo so a team can publish a common set of VMs and
/// categories. Unknown fields are tolerated so a repo that still holds a full
/// `Config` snapshot keeps loading (we just pick out `categories`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Inventory {
    pub categories: Vec<Category>,
}

impl Inventory {
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read inventory file: {}", path.display()))?;

        if content.trim().is_empty() {
            return Ok(Self::default());
        }

        serde_yaml::from_str::<Self>(&content)
            .with_context(|| format!("invalid YAML in inventory file: {}", path.display()))
    }

    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create inventory dir: {}", parent.display()))?;
        }
        let yaml = serde_yaml::to_string(self).context("failed to serialize inventory")?;

        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let file_name = path
            .file_name()
            .map(|n| n.to_owned())
            .unwrap_or_else(|| std::ffi::OsString::from("inventory.yaml"));
        let mut tmp = parent.join(&file_name);
        tmp.as_mut_os_string().push(".tmp");

        fs::write(&tmp, yaml)
            .with_context(|| format!("failed to stage inventory file: {}", tmp.display()))?;
        fs::rename(&tmp, path)
            .with_context(|| format!("failed to write inventory file: {}", path.display()))?;
        Ok(())
    }
}

pub fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from(QUALIFIER, ORG, APP)
}

pub fn default_config_path() -> PathBuf {
    if let Some(dirs) = project_dirs() {
        return dirs.config_dir().join("config.yaml");
    }
    PathBuf::from("config.yaml")
}

#[cfg(unix)]
fn restrict_perms(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(path, perms)
}

#[cfg(not(unix))]
fn restrict_perms(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub fn default_sync_local() -> PathBuf {
    if let Some(dirs) = project_dirs() {
        return dirs.data_dir().join("config-repo");
    }
    PathBuf::from(".hoppr/config-repo")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn load_returns_default_when_file_missing() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("missing.yaml");
        let cfg = Config::load_or_default(&path).expect("missing file should be ok");
        assert!(cfg.categories.is_empty());
    }

    #[test]
    fn load_parses_valid_yaml() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("config.yaml");
        fs::write(
            &path,
            r#"
defaults:
  command: ssh
  port: 2022
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
        .expect("write yaml");

        let cfg = Config::load_from_path(&path).expect("parse yaml");
        assert_eq!(cfg.categories.len(), 1);
        assert_eq!(cfg.categories[0].hosts[0].name, "gateway");
        assert_eq!(cfg.categories[0].hosts[0].port, Some(2222));
        assert_eq!(cfg.defaults.port, 2022);
        assert_eq!(cfg.defaults.command.program(), "ssh");
    }

    #[test]
    fn template_command_round_trips() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("config.yaml");
        fs::write(
            &path,
            r#"
defaults:
  command:
    program: mosh
    args: ["--", "{user}@{host}"]
categories: []
"#,
        )
        .expect("write yaml");

        let cfg = Config::load_from_path(&path).expect("parse yaml");
        assert_eq!(cfg.defaults.command.program(), "mosh");
        assert_eq!(cfg.defaults.command.args().unwrap().len(), 2);
    }

    #[test]
    fn inventory_picks_only_categories_from_full_config() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("hoppr.yaml");
        fs::write(
            &path,
            r#"
defaults:
  command: ssh
  port: 4242
sync:
  repo: git@example.com:team/cfg.git
categories:
  - name: prod
    icon: "🛰"
    hosts:
      - name: app-1
        ip: 10.0.0.10
        user: ops
"#,
        )
        .expect("write yaml");

        let inv = Inventory::load_from_path(&path).expect("load inventory");
        assert_eq!(inv.categories.len(), 1);
        assert_eq!(inv.categories[0].hosts[0].name, "app-1");
    }

    #[test]
    fn apply_inventory_keeps_local_defaults_and_sync() {
        let mut cfg = Config {
            defaults: Defaults {
                command: ConnectCommand::Program("mosh".into()),
                port: 2222,
                user: Some("me".into()),
                terminal_command: None,
            },
            sync: Some(SyncConfig {
                repo: Some("git@example.com:team/cfg.git".into()),
                ..Default::default()
            }),
            categories: vec![Category {
                name: "stale".into(),
                ..Default::default()
            }],
        };

        let inv = Inventory {
            categories: vec![Category {
                name: "fresh".into(),
                icon: None,
                hosts: vec![Host {
                    name: "edge".into(),
                    ip: "10.0.0.5".into(),
                    ..Default::default()
                }],
            }],
        };
        cfg.apply_inventory(inv);

        assert_eq!(cfg.categories.len(), 1);
        assert_eq!(cfg.categories[0].name, "fresh");
        assert_eq!(cfg.defaults.port, 2222);
        assert_eq!(cfg.defaults.command.program(), "mosh");
        assert!(cfg.sync.as_ref().and_then(|s| s.repo.as_deref()).is_some());
    }

    #[test]
    fn inventory_round_trips_via_disk() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("nested").join("hoppr.yaml");
        let inv = Inventory {
            categories: vec![Category {
                name: "lab".into(),
                icon: Some("🧪".into()),
                hosts: vec![Host {
                    name: "bench".into(),
                    ip: "192.168.0.42".into(),
                    user: Some("alice".into()),
                    port: Some(2222),
                    cmd: None,
                    command: None,
                }],
            }],
        };
        inv.save_to_path(&path).expect("save inventory");
        let reloaded = Inventory::load_from_path(&path).expect("reload inventory");
        assert_eq!(reloaded.categories.len(), 1);
        assert_eq!(reloaded.categories[0].hosts[0].port, Some(2222));
    }

    #[test]
    fn save_then_load_round_trips() {
        let tmp = TempDir::new().expect("tempdir");
        let path = tmp.path().join("nested").join("config.yaml");
        let mut cfg = Config::default();
        cfg.categories.push(Category {
            name: "ops".into(),
            icon: Some("⚙".into()),
            hosts: vec![Host {
                name: "edge".into(),
                ip: "10.0.0.5".into(),
                user: Some("root".into()),
                port: Some(22),
                cmd: None,
                command: None,
            }],
        });

        cfg.save(&path).expect("save");
        let loaded = Config::load_from_path(&path).expect("reload");
        assert_eq!(loaded.categories[0].hosts[0].name, "edge");
    }
}
