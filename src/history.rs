//! Connection history — records and persists the last N remote connections.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use time::{macros::format_description, OffsetDateTime};

use crate::config::project_dirs;

const MAX_ENTRIES: usize = 50;
const RECENT_DEFAULT: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub host_name: String,
    pub ip: String,
    pub category: String,
    /// Unix timestamp (seconds UTC).
    pub connected_at: i64,
}

impl HistoryEntry {
    pub fn connected_at_display(&self) -> String {
        let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]");
        OffsetDateTime::from_unix_timestamp(self.connected_at)
            .ok()
            .and_then(|dt| dt.format(&fmt).ok())
            .unwrap_or_else(|| "?".into())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HistoryFile {
    #[serde(default)]
    entries: VecDeque<HistoryEntry>,
}

pub struct HistoryStore {
    entries: VecDeque<HistoryEntry>,
    path: PathBuf,
}

impl HistoryStore {
    pub fn load(path: &Path) -> Self {
        let entries = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_yaml::from_str::<HistoryFile>(&s).ok())
            .map(|f| f.entries)
            .unwrap_or_default();
        Self {
            entries,
            path: path.to_owned(),
        }
    }

    pub fn record(&mut self, host_name: &str, ip: &str, category: &str) {
        self.entries
            .retain(|e| !(e.host_name == host_name && e.ip == ip));
        self.entries.push_front(HistoryEntry {
            host_name: host_name.to_owned(),
            ip: ip.to_owned(),
            category: category.to_owned(),
            connected_at: OffsetDateTime::now_utc().unix_timestamp(),
        });
        while self.entries.len() > MAX_ENTRIES {
            self.entries.pop_back();
        }
    }

    pub fn recent(&self, n: usize) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter().take(n)
    }

    pub fn recent_default(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.recent(RECENT_DEFAULT)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create history dir: {}", parent.display()))?;
        }
        let file = HistoryFile {
            entries: self.entries.clone(),
        };
        let yaml = serde_yaml::to_string(&file).context("failed to serialize history")?;
        fs::write(&self.path, yaml)
            .with_context(|| format!("failed to write history: {}", self.path.display()))
    }
}

impl Default for HistoryStore {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
            path: default_history_path(),
        }
    }
}

pub fn default_history_path() -> PathBuf {
    if let Some(dirs) = project_dirs() {
        return dirs.data_dir().join("history.yaml");
    }
    PathBuf::from(".hoppr/history.yaml")
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn record_deduplicates_and_moves_to_front() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.yaml");
        let mut store = HistoryStore::load(&path);

        store.record("web-01", "10.0.0.1", "prod");
        store.record("db-01", "10.0.0.2", "prod");
        store.record("web-01", "10.0.0.1", "prod"); // duplicate

        let entries: Vec<_> = store.recent(10).collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].host_name, "web-01");
        assert_eq!(entries[1].host_name, "db-01");
    }

    #[test]
    fn save_and_load_round_trips() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.yaml");
        let mut store = HistoryStore::load(&path);
        store.record("gw", "192.168.1.1", "home");
        store.save().unwrap();

        let loaded = HistoryStore::load(&path);
        assert_eq!(loaded.recent(1).next().unwrap().host_name, "gw");
    }

    #[test]
    fn caps_at_max_entries() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.yaml");
        let mut store = HistoryStore::load(&path);
        for i in 0..60u32 {
            store.record(&format!("host-{i}"), &format!("10.0.0.{i}"), "ops");
        }
        assert_eq!(store.recent(100).count(), MAX_ENTRIES);
    }
}
