//! Persistent host bookmarks — stores a set of "category/host" keys on disk.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::project_dirs;

#[derive(Debug, Default, Serialize, Deserialize)]
struct FavoritesFile {
    #[serde(default)]
    favorites: HashSet<String>,
}

pub struct FavoritesStore {
    favorites: HashSet<String>,
    path: PathBuf,
}

impl FavoritesStore {
    pub fn load(path: &Path) -> Self {
        let favorites = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_yaml::from_str::<FavoritesFile>(&s).ok())
            .map(|f| f.favorites)
            .unwrap_or_default();
        Self {
            favorites,
            path: path.to_owned(),
        }
    }

    /// Toggle a host's favorite status. Returns `true` if it is now starred.
    pub fn toggle(&mut self, category: &str, host: &str) -> bool {
        let key = Self::key(category, host);
        if self.favorites.contains(&key) {
            self.favorites.remove(&key);
            false
        } else {
            self.favorites.insert(key);
            true
        }
    }

    pub fn is_favorite(&self, category: &str, host: &str) -> bool {
        self.favorites.contains(&Self::key(category, host))
    }

    pub fn is_empty(&self) -> bool {
        self.favorites.is_empty()
    }

    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create favorites dir: {}", parent.display()))?;
        }
        let file = FavoritesFile {
            favorites: self.favorites.clone(),
        };
        let yaml = serde_yaml::to_string(&file).context("failed to serialize favorites")?;
        fs::write(&self.path, yaml)
            .with_context(|| format!("failed to write favorites: {}", self.path.display()))
    }

    fn key(category: &str, host: &str) -> String {
        format!("{category}/{host}")
    }
}

impl Default for FavoritesStore {
    fn default() -> Self {
        Self {
            favorites: HashSet::new(),
            path: default_favorites_path(),
        }
    }
}

pub fn default_favorites_path() -> PathBuf {
    if let Some(dirs) = project_dirs() {
        return dirs.data_dir().join("favorites.yaml");
    }
    PathBuf::from(".hoppr/favorites.yaml")
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn toggle_adds_then_removes() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("favorites.yaml");
        let mut store = FavoritesStore::load(&path);

        assert!(!store.is_favorite("prod", "web-01"));
        assert!(store.toggle("prod", "web-01"));
        assert!(store.is_favorite("prod", "web-01"));
        assert!(!store.toggle("prod", "web-01"));
        assert!(!store.is_favorite("prod", "web-01"));
    }

    #[test]
    fn save_and_load_round_trips() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("favorites.yaml");
        let mut store = FavoritesStore::load(&path);
        store.toggle("infra", "gateway");
        store.save().unwrap();

        let loaded = FavoritesStore::load(&path);
        assert!(loaded.is_favorite("infra", "gateway"));
    }
}
