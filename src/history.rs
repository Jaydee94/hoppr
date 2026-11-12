//! Connection history — records and persists the last N remote connections.

use std::{
    collections::VecDeque,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use time::{OffsetDateTime, macros::format_description};

use crate::{
    config::project_dirs,
    storage::{self, Access},
};

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
            .and_then(|s| yaml_serde::from_str::<HistoryFile>(&s).ok())
            .map(|f| f.entries)
            .unwrap_or_default();
        Self {
            entries,
            path: path.to_owned(),
        }
    }

    /// Put a connection at the front, replacing the entry it already has.
    ///
    /// An entry is one category + name + IP, the same identity the Recent
    /// list resolves against the config: the same machine listed in two
    /// categories is two hosts there and keeps two entries here.
    pub fn record(&mut self, host_name: &str, ip: &str, category: &str) {
        self.entries
            .retain(|e| !(e.category == category && e.host_name == host_name && e.ip == ip));
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

    /// Point the entries recorded for a host at its new name and address.
    /// Returns `true` if any entry changed.
    ///
    /// [`Self::record`] keeps one entry per category + name + IP, so when
    /// the new identity already has an entry only the most recent one
    /// survives.
    pub fn rename_host(
        &mut self,
        category: &str,
        (old_name, old_ip): (&str, &str),
        (new_name, new_ip): (&str, &str),
    ) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.category == category && entry.host_name == old_name && entry.ip == old_ip {
                new_name.clone_into(&mut entry.host_name);
                new_ip.clone_into(&mut entry.ip);
                changed = true;
            }
        }
        if changed {
            let mut newer_exists = false;
            self.entries.retain(|entry| {
                if entry.category != category || entry.host_name != new_name || entry.ip != new_ip {
                    return true;
                }
                !std::mem::replace(&mut newer_exists, true)
            });
        }
        changed
    }

    /// Point every entry of a category at the category's new name. Returns
    /// `true` if any entry changed.
    pub fn rename_category(&mut self, old_category: &str, new_category: &str) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            if entry.category == old_category {
                new_category.clone_into(&mut entry.category);
                changed = true;
            }
        }
        changed
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
        let file = HistoryFile {
            entries: self.entries.clone(),
        };
        storage::save_yaml(&self.path, &file, "history", Access::Inherit)
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
    fn record_keeps_the_same_host_apart_per_category() {
        let tmp = TempDir::new().unwrap();
        let mut store = HistoryStore::load(&tmp.path().join("history.yaml"));
        store.record("web-01", "10.0.0.1", "prod");
        store.record("web-01", "10.0.0.1", "lab");
        store.record("web-01", "10.0.0.1", "prod");

        let seen: Vec<_> = store.recent(10).map(|e| e.category.as_str()).collect();
        assert_eq!(seen, ["prod", "lab"]);
    }

    #[test]
    fn rename_host_leaves_the_same_identity_in_other_categories() {
        let tmp = TempDir::new().unwrap();
        let mut store = HistoryStore::load(&tmp.path().join("history.yaml"));
        store.record("new", "10.0.0.1", "lab");
        store.record("old", "10.0.0.1", "prod");
        store.record("new", "10.0.0.1", "prod");

        assert!(store.rename_host("prod", ("old", "10.0.0.1"), ("new", "10.0.0.1")));
        let seen: Vec<_> = store
            .recent(10)
            .map(|e| (e.category.as_str(), e.host_name.as_str()))
            .collect();
        assert_eq!(seen, [("prod", "new"), ("lab", "new")]);
    }

    #[test]
    fn loads_a_history_file_written_before_per_category_entries() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("history.yaml");
        fs::write(
            &path,
            "entries:\n- host_name: gw\n  ip: 192.168.1.1\n  category: home\n  connected_at: 1700000000\n",
        )
        .unwrap();
        let mut store = HistoryStore::load(&path);
        store.record("gw", "192.168.1.1", "home");
        assert_eq!(store.recent(10).count(), 1);
    }

    #[test]
    fn rename_host_follows_name_and_ip_within_the_category() {
        let tmp = TempDir::new().unwrap();
        let mut store = HistoryStore::load(&tmp.path().join("history.yaml"));
        store.record("web-01", "10.0.0.1", "prod");
        store.record("web-01", "10.9.9.9", "lab");
        store.record("db-01", "10.0.0.2", "prod");

        assert!(store.rename_host("prod", ("web-01", "10.0.0.1"), ("web-a", "10.0.0.7")));
        let seen: Vec<_> = store
            .recent(10)
            .map(|e| (e.category.as_str(), e.host_name.as_str(), e.ip.as_str()))
            .collect();
        assert_eq!(
            seen,
            [
                ("prod", "db-01", "10.0.0.2"),
                ("lab", "web-01", "10.9.9.9"),
                ("prod", "web-a", "10.0.0.7"),
            ]
        );
        assert!(!store.rename_host("prod", ("gone", "10.0.0.1"), ("x", "y")));
    }

    #[test]
    fn rename_host_keeps_one_entry_per_name_and_ip() {
        let tmp = TempDir::new().unwrap();
        let mut store = HistoryStore::load(&tmp.path().join("history.yaml"));
        store.record("old", "10.0.0.1", "prod");
        store.record("other", "10.0.0.3", "prod");
        store.record("new", "10.0.0.1", "prod");

        assert!(store.rename_host("prod", ("old", "10.0.0.1"), ("new", "10.0.0.1")));
        let names: Vec<_> = store.recent(10).map(|e| e.host_name.as_str()).collect();
        assert_eq!(names, ["new", "other"]);
    }

    #[test]
    fn rename_category_updates_only_matching_entries() {
        let tmp = TempDir::new().unwrap();
        let mut store = HistoryStore::load(&tmp.path().join("history.yaml"));
        store.record("web-01", "10.0.0.1", "prod");
        store.record("gw", "192.168.1.1", "home");

        assert!(store.rename_category("prod", "live"));
        let categories: Vec<_> = store.recent(10).map(|e| e.category.as_str()).collect();
        assert_eq!(categories, ["home", "live"]);
        assert!(!store.rename_category("prod", "other"));
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
