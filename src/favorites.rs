//! Persistent host bookmarks — the set of starred (category, host) pairs.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::{
    config::project_dirs,
    storage::{self, Access},
};

const SEPARATOR: char = '/';

/// One star as it is written to disk.
///
/// The original format is a single `category/host` string, which cannot say
/// where the category ends once a name contains a `/` itself. Such a star is
/// written as a `{category, host}` mapping; every other star keeps the
/// string form, so files without a `/` in any name look the way they always
/// did.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
enum Entry {
    Key(String),
    Pair { category: String, host: String },
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct FavoritesFile {
    #[serde(default)]
    favorites: Vec<Entry>,
}

pub struct FavoritesStore {
    /// Starred host names per category. A category without stars has no
    /// entry, so an empty map means no stars at all.
    favorites: HashMap<String, HashSet<String>>,
    /// String keys from an older file that do not have exactly one `/` and
    /// so do not name one (category, host) pair. They are kept as they are
    /// until [`Self::adopt_legacy_keys`] can match them against the config.
    legacy_keys: BTreeSet<String>,
    path: PathBuf,
}

impl FavoritesStore {
    pub fn load(path: &Path) -> Self {
        let entries = fs::read_to_string(path)
            .ok()
            .and_then(|s| yaml_serde::from_str::<FavoritesFile>(&s).ok())
            .map(|f| f.favorites)
            .unwrap_or_default();
        let mut store = Self {
            favorites: HashMap::new(),
            legacy_keys: BTreeSet::new(),
            path: path.to_owned(),
        };
        for entry in entries {
            match entry {
                Entry::Pair { category, host } => store.star(&category, &host),
                Entry::Key(key) => match key.split_once(SEPARATOR) {
                    Some((category, host)) if !host.contains(SEPARATOR) => {
                        store.star(category, host);
                    }
                    _ => {
                        store.legacy_keys.insert(key);
                    }
                },
            }
        }
        store
    }

    /// Resolve the ambiguous keys of an older file. A key becomes a star on
    /// every way of splitting it that `exists(category, host)` confirms —
    /// which is what the string comparison used to light up — and is then
    /// dropped. A key that matches nothing is kept, so stars survive a start
    /// with an inventory that is temporarily missing their host.
    pub fn adopt_legacy_keys(&mut self, exists: impl Fn(&str, &str) -> bool) {
        for key in std::mem::take(&mut self.legacy_keys) {
            let mut adopted = false;
            for (at, _) in key.match_indices(SEPARATOR) {
                let (category, host) = (&key[..at], &key[at + SEPARATOR.len_utf8()..]);
                if exists(category, host) {
                    self.star(category, host);
                    adopted = true;
                }
            }
            if !adopted {
                self.legacy_keys.insert(key);
            }
        }
    }

    /// Toggle a host's favorite status. Returns `true` if it is now starred.
    pub fn toggle(&mut self, category: &str, host: &str) -> bool {
        if self.unstar(category, host) {
            false
        } else {
            self.star(category, host);
            true
        }
    }

    pub fn is_favorite(&self, category: &str, host: &str) -> bool {
        self.favorites
            .get(category)
            .is_some_and(|hosts| hosts.contains(host))
    }

    pub fn is_empty(&self) -> bool {
        self.favorites.is_empty()
    }

    /// Carry a star over to a host's new name. Returns `true` if a star moved.
    pub fn rename_host(&mut self, category: &str, old_host: &str, new_host: &str) -> bool {
        if !self.unstar(category, old_host) {
            return false;
        }
        self.star(category, new_host);
        true
    }

    /// Carry every star in a category over to the category's new name.
    /// Returns `true` if any star moved.
    pub fn rename_category(&mut self, old_category: &str, new_category: &str) -> bool {
        let Some(hosts) = self.favorites.remove(old_category) else {
            return false;
        };
        self.favorites
            .entry(new_category.to_owned())
            .or_default()
            .extend(hosts);
        true
    }

    fn star(&mut self, category: &str, host: &str) {
        self.favorites
            .entry(category.to_owned())
            .or_default()
            .insert(host.to_owned());
    }

    fn unstar(&mut self, category: &str, host: &str) -> bool {
        let Some(hosts) = self.favorites.get_mut(category) else {
            return false;
        };
        let removed = hosts.remove(host);
        if hosts.is_empty() {
            self.favorites.remove(category);
        }
        removed
    }

    pub fn save(&self) -> Result<()> {
        let entries: BTreeSet<Entry> = self
            .favorites
            .iter()
            .flat_map(|(category, hosts)| hosts.iter().map(move |host| (category, host)))
            .map(|(category, host)| {
                if category.contains(SEPARATOR) || host.contains(SEPARATOR) {
                    Entry::Pair {
                        category: category.clone(),
                        host: host.clone(),
                    }
                } else {
                    Entry::Key(format!("{category}{SEPARATOR}{host}"))
                }
            })
            .chain(self.legacy_keys.iter().cloned().map(Entry::Key))
            .collect();
        let file = FavoritesFile {
            favorites: entries.into_iter().collect(),
        };
        storage::save_yaml(&self.path, &file, "favorites", Access::Inherit)
    }
}

impl Default for FavoritesStore {
    fn default() -> Self {
        Self {
            favorites: HashMap::new(),
            legacy_keys: BTreeSet::new(),
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
        assert!(store.is_empty());
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
    fn names_containing_a_slash_stay_distinct() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("favorites.yaml");
        let mut store = FavoritesStore::load(&path);
        store.toggle("a/b", "c");
        store.toggle("x", "y");

        assert!(store.is_favorite("a/b", "c"));
        assert!(!store.is_favorite("a", "b/c"));

        assert!(!store.rename_category("a", "z"));
        assert!(store.is_favorite("a/b", "c"));
        assert!(store.rename_category("a/b", "d/e"));

        store.save().unwrap();
        let loaded = FavoritesStore::load(&path);
        assert!(loaded.is_favorite("d/e", "c"));
        assert!(!loaded.is_favorite("d", "e/c"));
        assert!(loaded.is_favorite("x", "y"));
    }

    #[test]
    fn slash_free_stars_keep_the_original_file_format() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("favorites.yaml");
        let mut store = FavoritesStore::load(&path);
        store.toggle("prod", "web-01");
        store.toggle("infra", "gateway");
        store.save().unwrap();

        let written = fs::read_to_string(&path).unwrap();
        assert_eq!(written, "favorites:\n- infra/gateway\n- prod/web-01\n");
    }

    #[test]
    fn legacy_keys_are_matched_against_the_known_hosts() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("favorites.yaml");
        fs::write(
            &path,
            "favorites:\n- prod/web-01\n- eu/west/db\n- gone/for/now\n",
        )
        .unwrap();
        let mut store = FavoritesStore::load(&path);
        assert!(store.is_favorite("prod", "web-01"));
        assert!(!store.is_favorite("eu/west", "db"));

        store.adopt_legacy_keys(|category, host| (category, host) == ("eu/west", "db"));
        assert!(store.is_favorite("eu/west", "db"));
        assert!(!store.is_favorite("eu", "west/db"));
        assert!(!store.is_favorite("gone", "for/now"));

        store.save().unwrap();
        let mut reloaded = FavoritesStore::load(&path);
        assert!(reloaded.is_favorite("eu/west", "db"));
        reloaded.adopt_legacy_keys(|category, host| (category, host) == ("gone/for", "now"));
        assert!(reloaded.is_favorite("gone/for", "now"));
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
