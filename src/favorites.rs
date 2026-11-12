//! Persistent host bookmarks — stores a set of "category/host" keys on disk.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    config::project_dirs,
    storage::{self, Access},
};

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
            .and_then(|s| yaml_serde::from_str::<FavoritesFile>(&s).ok())
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

    /// Carry a star over to a host's new name. Returns `true` if a star moved.
    pub fn rename_host(&mut self, category: &str, old_host: &str, new_host: &str) -> bool {
        if !self.favorites.remove(&Self::key(category, old_host)) {
            return false;
        }
        self.favorites.insert(Self::key(category, new_host));
        true
    }

    /// Carry every star in a category over to the category's new name.
    /// Returns `true` if any star moved.
    pub fn rename_category(&mut self, old_category: &str, new_category: &str) -> bool {
        let old_prefix = Self::key(old_category, "");
        let moved: Vec<String> = self
            .favorites
            .iter()
            .filter(|key| key.starts_with(&old_prefix))
            .cloned()
            .collect();
        for key in &moved {
            self.favorites.remove(key);
            self.favorites
                .insert(Self::key(new_category, &key[old_prefix.len()..]));
        }
        !moved.is_empty()
    }

    pub fn save(&self) -> Result<()> {
        let file = FavoritesFile {
            favorites: self.favorites.clone(),
        };
        storage::save_yaml(&self.path, &file, "favorites", Access::Inherit)
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
    fn rename_host_moves_the_star() {
        let tmp = TempDir::new().unwrap();
        let mut store = FavoritesStore::load(&tmp.path().join("favorites.yaml"));
        store.toggle("prod", "web-01");
        store.toggle("prod", "db-01");

        assert!(store.rename_host("prod", "web-01", "web-a"));
        assert!(store.is_favorite("prod", "web-a"));
        assert!(!store.is_favorite("prod", "web-01"));
        assert!(store.is_favorite("prod", "db-01"));
    }

    #[test]
    fn rename_host_ignores_unstarred_hosts_and_other_categories() {
        let tmp = TempDir::new().unwrap();
        let mut store = FavoritesStore::load(&tmp.path().join("favorites.yaml"));
        store.toggle("prod", "web-01");

        assert!(!store.rename_host("prod", "db-01", "db-a"));
        assert!(!store.rename_host("lab", "web-01", "web-a"));
        assert!(!store.is_favorite("prod", "db-a"));
        assert!(!store.is_favorite("lab", "web-a"));
        assert!(store.is_favorite("prod", "web-01"));
    }

    #[test]
    fn rename_category_moves_only_that_categorys_stars() {
        let tmp = TempDir::new().unwrap();
        let mut store = FavoritesStore::load(&tmp.path().join("favorites.yaml"));
        store.toggle("prod", "web-01");
        store.toggle("prod", "db-01");
        store.toggle("prod-eu", "web-01");

        assert!(store.rename_category("prod", "live"));
        assert!(store.is_favorite("live", "web-01"));
        assert!(store.is_favorite("live", "db-01"));
        assert!(!store.is_favorite("prod", "web-01"));
        assert!(store.is_favorite("prod-eu", "web-01"));
        assert!(!store.rename_category("prod", "other"));
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
