//! Top-level application state for the TUI.

use std::{
    cmp::Reverse,
    path::PathBuf,
    time::{Duration, Instant},
};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ratatui::widgets::ListState;

use crate::{
    config::{Category, Config, Host},
    editor::EditorState,
    favorites::FavoritesStore,
    history::HistoryStore,
    sync::SyncStatus,
    terminal::TerminalLauncher,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Categories,
    Hosts,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Browse,
    Edit,
}

/// Severity tier of a transient status-bar message. Drives both the
/// leading glyph and the color, plus the auto-fade TTL: friendly messages
/// (info / success) fade fast, problems linger so the user has time to
/// read them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Success,
    Info,
    Warn,
    Error,
}

impl MessageKind {
    pub fn glyph(self) -> &'static str {
        match self {
            Self::Success => "✓",
            Self::Info => "·",
            Self::Warn => "⚠",
            Self::Error => "✕",
        }
    }

    pub fn ttl(self) -> Duration {
        match self {
            Self::Success | Self::Info => Duration::from_secs(3),
            Self::Warn => Duration::from_secs(6),
            Self::Error => Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone)]
pub struct StatusMessage {
    pub kind: MessageKind,
    pub text: String,
    pub set_at: Instant,
}

impl StatusMessage {
    pub fn is_fresh(&self) -> bool {
        self.set_at.elapsed() < self.kind.ttl()
    }
}

/// Render a `Duration` as a short "x ago" string for the sync chip.
/// Keeps the output narrow so the left status column doesn't push the
/// middle slot around as time passes.
pub fn relative_time(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 5 {
        "just now".into()
    } else if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

/// A host together with the name of its originating category.
/// Returned by [`App::filtered_hosts`] so callers always know which
/// category a host belongs to (needed for global search and virtual views).
pub struct FilteredHost<'a> {
    pub host: &'a Host,
    pub category_name: &'a str,
}

/// Identifies which virtual category slot is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtualCategoryKind {
    Recent,
    Starred,
}

/// A flattened category entry suitable for the category list widget.
pub struct CategoryDisplay<'a> {
    pub name: &'a str,
    pub icon: Option<&'a str>,
    pub is_virtual: bool,
}

pub struct App {
    pub config: Config,
    pub config_path: PathBuf,
    pub focus: Focus,
    pub mode: Mode,
    pub search_query: String,
    /// When `true` and the search query is non-empty, search all categories.
    pub search_all: bool,
    pub categories_state: ListState,
    pub hosts_state: ListState,
    pub sync_status: SyncStatus,
    pub status_message: Option<StatusMessage>,
    /// Wall-clock anchor for the last successful sync, used to render
    /// the "synced 2m ago" chip on the left of the status bar.
    pub last_sync_at: Option<Instant>,
    /// Cached result of `sync::has_uncommitted_changes` — `Some(true)`
    /// surfaces the "unpushed" chip. `None` means we haven't checked yet
    /// (no sync configured, or the clone is missing).
    pub sync_dirty: Option<bool>,
    pub editor: Option<EditorState>,
    pub history: HistoryStore,
    pub favorites: FavoritesStore,
    pub terminal: TerminalLauncher,
    matcher: SkimMatcherV2,
}

impl App {
    pub fn new(
        config: Config,
        config_path: PathBuf,
        sync_status: SyncStatus,
        history: HistoryStore,
        favorites: FavoritesStore,
        terminal: TerminalLauncher,
    ) -> Self {
        let mut categories_state = ListState::default();
        let virt_count: usize =
            usize::from(!history.is_empty()) + usize::from(!favorites.is_empty());
        let total = virt_count + config.categories.len();
        categories_state.select(if total > 0 { Some(0) } else { None });

        let mut app = Self {
            config,
            config_path,
            focus: Focus::Hosts,
            mode: Mode::Browse,
            search_query: String::new(),
            search_all: false,
            categories_state,
            hosts_state: ListState::default(),
            sync_status,
            status_message: None,
            last_sync_at: None,
            sync_dirty: None,
            editor: None,
            history,
            favorites,
            terminal,
            matcher: SkimMatcherV2::default(),
        };
        app.ensure_valid_selection();
        app
    }

    // ── virtual category helpers ────────────────────────────────────────────

    fn virtual_category_count(&self) -> usize {
        usize::from(!self.history.is_empty()) + usize::from(!self.favorites.is_empty())
    }

    pub fn total_category_count(&self) -> usize {
        self.virtual_category_count() + self.config.categories.len()
    }

    fn virtual_category_at(&self, idx: usize) -> Option<VirtualCategoryKind> {
        let mut slot = 0usize;
        if !self.history.is_empty() {
            if idx == slot {
                return Some(VirtualCategoryKind::Recent);
            }
            slot += 1;
        }
        if !self.favorites.is_empty() && idx == slot {
            return Some(VirtualCategoryKind::Starred);
        }
        None
    }

    pub fn current_virtual_category(&self) -> Option<VirtualCategoryKind> {
        let idx = self.categories_state.selected()?;
        let vcount = self.virtual_category_count();
        if idx < vcount {
            self.virtual_category_at(idx)
        } else {
            None
        }
    }

    /// Returns the real `Category` for the current selection, or `None` if a
    /// virtual category is selected.
    pub fn current_category(&self) -> Option<&Category> {
        let idx = self.categories_state.selected()?;
        let vcount = self.virtual_category_count();
        self.config.categories.get(idx.checked_sub(vcount)?)
    }

    /// Flat list of all visible categories (virtual first, then real) for the
    /// category panel widget.
    pub fn categories_for_display(&self) -> Vec<CategoryDisplay<'_>> {
        let mut out = Vec::new();
        if !self.history.is_empty() {
            out.push(CategoryDisplay {
                name: "🕒 Recent",
                icon: None,
                is_virtual: true,
            });
        }
        if !self.favorites.is_empty() {
            out.push(CategoryDisplay {
                name: "★ Starred",
                icon: None,
                is_virtual: true,
            });
        }
        for cat in &self.config.categories {
            out.push(CategoryDisplay {
                name: &cat.name,
                icon: cat.icon.as_deref(),
                is_virtual: false,
            });
        }
        out
    }

    // ── host filtering ──────────────────────────────────────────────────────

    /// Returns the hosts to display in the hosts panel. Always includes the
    /// originating category name so callers can render it in global/virtual
    /// views without a second lookup.
    pub fn filtered_hosts(&self) -> Vec<FilteredHost<'_>> {
        if self.search_all && !self.search_query.trim().is_empty() {
            return self.search_all_categories();
        }
        match self.current_virtual_category() {
            Some(VirtualCategoryKind::Recent) => return self.recent_hosts(),
            Some(VirtualCategoryKind::Starred) => return self.starred_hosts(),
            None => {}
        }
        let Some(category) = self.current_category() else {
            return Vec::new();
        };
        let cat_name = category.name.as_str();
        if self.search_query.trim().is_empty() {
            return category
                .hosts
                .iter()
                .map(|h| FilteredHost {
                    host: h,
                    category_name: cat_name,
                })
                .collect();
        }
        let mut scored: Vec<(i64, &Host)> = category
            .hosts
            .iter()
            .filter_map(|host| {
                let haystack = format!("{} {}", host.name, host.ip);
                self.matcher
                    .fuzzy_match(&haystack, &self.search_query)
                    .map(|s| (s, host))
            })
            .collect();
        scored.sort_by_key(|(s, _)| Reverse(*s));
        scored
            .into_iter()
            .map(|(_, host)| FilteredHost {
                host,
                category_name: cat_name,
            })
            .collect()
    }

    fn search_all_categories(&self) -> Vec<FilteredHost<'_>> {
        let categories = &self.config.categories;
        let q = &self.search_query;
        let mut scored: Vec<(i64, &Host, &str)> = categories
            .iter()
            .flat_map(|cat| cat.hosts.iter().map(move |host| (cat.name.as_str(), host)))
            .filter_map(|(cat_name, host)| {
                let haystack = format!("{} {} {}", host.name, host.ip, cat_name);
                self.matcher
                    .fuzzy_match(&haystack, q)
                    .map(|s| (s, host, cat_name))
            })
            .collect();
        scored.sort_by_key(|(s, _, _)| Reverse(*s));
        scored
            .into_iter()
            .map(|(_, host, category_name)| FilteredHost {
                host,
                category_name,
            })
            .collect()
    }

    fn recent_hosts(&self) -> Vec<FilteredHost<'_>> {
        let categories = &self.config.categories;
        let q = self.search_query.trim();
        self.history
            .recent_default()
            .filter_map(|entry| {
                for cat in categories {
                    if cat.name == entry.category {
                        for host in &cat.hosts {
                            if host.name == entry.host_name
                                && host.ip == entry.ip
                                && (q.is_empty()
                                    || self
                                        .matcher
                                        .fuzzy_match(&format!("{} {}", host.name, host.ip), q)
                                        .is_some())
                            {
                                return Some(FilteredHost {
                                    host,
                                    category_name: &cat.name,
                                });
                            }
                        }
                    }
                }
                None
            })
            .collect()
    }

    fn starred_hosts(&self) -> Vec<FilteredHost<'_>> {
        let categories = &self.config.categories;
        let q = self.search_query.trim();
        let mut result = Vec::new();
        for cat in categories {
            for host in &cat.hosts {
                if self.favorites.is_favorite(&cat.name, &host.name)
                    && (q.is_empty()
                        || self
                            .matcher
                            .fuzzy_match(&format!("{} {}", host.name, host.ip), q)
                            .is_some())
                {
                    result.push(FilteredHost {
                        host,
                        category_name: &cat.name,
                    });
                }
            }
        }
        result
    }

    #[allow(dead_code)]
    pub fn selected_host(&self) -> Option<&Host> {
        let hosts = self.filtered_hosts();
        self.hosts_state
            .selected()
            .and_then(|idx| hosts.get(idx).map(|fh| fh.host))
    }

    /// Returns the selected host together with its category name. Needed to
    /// record history and toggle favorites without a second scan.
    pub fn selected_host_with_category(&self) -> Option<(&Host, &str)> {
        let hosts = self.filtered_hosts();
        self.hosts_state
            .selected()
            .and_then(|idx| hosts.get(idx).map(|fh| (fh.host, fh.category_name)))
    }

    // ── selection management ────────────────────────────────────────────────

    pub fn ensure_valid_selection(&mut self) {
        let total = self.total_category_count();
        if total == 0 {
            self.categories_state.select(None);
            self.hosts_state.select(None);
            return;
        }

        let sel = self.categories_state.selected().unwrap_or(0).min(total - 1);
        self.categories_state.select(Some(sel));

        let host_len = self.filtered_hosts().len();
        if host_len == 0 {
            self.hosts_state.select(None);
        } else {
            let h = self.hosts_state.selected().unwrap_or(0).min(host_len - 1);
            self.hosts_state.select(Some(h));
        }
    }

    pub fn next(&mut self) {
        match self.focus {
            Focus::Categories => {
                let len = self.total_category_count();
                if len == 0 {
                    return;
                }
                let next = match self.categories_state.selected() {
                    Some(i) => (i + 1) % len,
                    None => 0,
                };
                self.categories_state.select(Some(next));
                self.hosts_state.select(Some(0));
            }
            Focus::Hosts => {
                let len = self.filtered_hosts().len();
                if len == 0 {
                    return;
                }
                let next = match self.hosts_state.selected() {
                    Some(i) => (i + 1) % len,
                    None => 0,
                };
                self.hosts_state.select(Some(next));
            }
            Focus::Search => {}
        }
        self.ensure_valid_selection();
    }

    pub fn previous(&mut self) {
        match self.focus {
            Focus::Categories => {
                let len = self.total_category_count();
                if len == 0 {
                    return;
                }
                let prev = match self.categories_state.selected() {
                    Some(i) => (i + len - 1) % len,
                    None => 0,
                };
                self.categories_state.select(Some(prev));
                self.hosts_state.select(Some(0));
            }
            Focus::Hosts => {
                let len = self.filtered_hosts().len();
                if len == 0 {
                    return;
                }
                let prev = match self.hosts_state.selected() {
                    Some(i) => (i + len - 1) % len,
                    None => 0,
                };
                self.hosts_state.select(Some(prev));
            }
            Focus::Search => {}
        }
        self.ensure_valid_selection();
    }

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Categories => Focus::Hosts,
            Focus::Hosts | Focus::Search => Focus::Categories,
        };
    }

    pub fn focus_search(&mut self) {
        self.focus = Focus::Search;
    }

    pub fn append_search_char(&mut self, ch: char) {
        self.search_query.push(ch);
        self.hosts_state.select(Some(0));
        self.ensure_valid_selection();
    }

    pub fn pop_search_char(&mut self) {
        self.search_query.pop();
        self.hosts_state.select(Some(0));
        self.ensure_valid_selection();
    }

    pub fn clear_search_focus_hosts(&mut self) {
        self.focus = Focus::Hosts;
        self.ensure_valid_selection();
    }

    pub fn enter_edit_mode(&mut self) {
        self.editor = Some(EditorState::from_config(&self.config));
        self.mode = Mode::Edit;
    }

    pub fn exit_edit_mode(&mut self) {
        self.editor = None;
        self.mode = Mode::Browse;
        self.ensure_valid_selection();
    }

    pub fn set_status<S: Into<String>>(&mut self, msg: S) {
        self.set_status_kind(MessageKind::Info, msg);
    }

    pub fn set_status_success<S: Into<String>>(&mut self, msg: S) {
        self.set_status_kind(MessageKind::Success, msg);
    }

    #[allow(dead_code)]
    pub fn set_status_warn<S: Into<String>>(&mut self, msg: S) {
        self.set_status_kind(MessageKind::Warn, msg);
    }

    pub fn set_status_error<S: Into<String>>(&mut self, msg: S) {
        self.set_status_kind(MessageKind::Error, msg);
    }

    pub fn set_status_kind<S: Into<String>>(&mut self, kind: MessageKind, msg: S) {
        self.status_message = Some(StatusMessage {
            kind,
            text: msg.into(),
            set_at: Instant::now(),
        });
    }

    /// Returns the current message only while it's still inside its TTL.
    /// Expired messages are reported as `None` so the renderer can fall
    /// back to the default content (selected-host preview).
    pub fn active_status(&self) -> Option<&StatusMessage> {
        self.status_message.as_ref().filter(|m| m.is_fresh())
    }

    /// Stamp `last_sync_at` and remember whether the local clone has
    /// uncommitted changes. Caller passes `None` for `dirty` when sync
    /// is disabled or the clone is missing.
    pub fn record_sync(&mut self, dirty: Option<bool>) {
        self.last_sync_at = Some(Instant::now());
        self.sync_dirty = dirty;
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{
        config::{Category, Config, Host},
        favorites::FavoritesStore,
        history::HistoryStore,
        sync::SyncStatus,
        terminal::TerminalLauncher,
    };

    use super::App;

    fn fixture() -> App {
        let config = Config {
            defaults: Default::default(),
            sync: None,
            categories: vec![Category {
                name: "infra".into(),
                icon: None,
                hosts: vec![
                    Host {
                        name: "gateway".into(),
                        ip: "10.0.0.1".into(),
                        user: None,
                        port: None,
                        cmd: None,
                        command: None,
                    },
                    Host {
                        name: "db".into(),
                        ip: "192.168.1.10".into(),
                        user: None,
                        port: None,
                        cmd: None,
                        command: None,
                    },
                ],
            }],
        };
        App::new(
            config,
            PathBuf::from("/tmp/x.yaml"),
            SyncStatus::Disabled,
            HistoryStore::default(),
            FavoritesStore::default(),
            TerminalLauncher::detect(None),
        )
    }

    #[test]
    fn fuzzy_filter_matches_name_and_ip() {
        let mut app = fixture();
        app.search_query = "gate".into();
        assert_eq!(app.filtered_hosts().len(), 1);

        app.search_query = "192".into();
        assert_eq!(app.filtered_hosts()[0].host.name, "db");
    }

    #[test]
    fn edit_mode_toggles() {
        let mut app = fixture();
        app.enter_edit_mode();
        assert!(matches!(app.mode, super::Mode::Edit));
        app.exit_edit_mode();
        assert!(matches!(app.mode, super::Mode::Browse));
    }

    #[test]
    fn global_search_finds_across_categories() {
        let config = Config {
            defaults: Default::default(),
            sync: None,
            categories: vec![
                Category {
                    name: "prod".into(),
                    icon: None,
                    hosts: vec![Host {
                        name: "web-01".into(),
                        ip: "10.1.0.1".into(),
                        ..Default::default()
                    }],
                },
                Category {
                    name: "staging".into(),
                    icon: None,
                    hosts: vec![Host {
                        name: "web-02".into(),
                        ip: "10.2.0.1".into(),
                        ..Default::default()
                    }],
                },
            ],
        };
        let mut app = App::new(
            config,
            PathBuf::from("/tmp/x.yaml"),
            SyncStatus::Disabled,
            HistoryStore::default(),
            FavoritesStore::default(),
            TerminalLauncher::detect(None),
        );
        app.search_all = true;
        app.search_query = "web".into();
        assert_eq!(app.filtered_hosts().len(), 2);
    }

    #[test]
    fn relative_time_buckets_correctly() {
        use super::relative_time;
        use std::time::Duration;
        assert_eq!(relative_time(Duration::from_secs(2)), "just now");
        assert_eq!(relative_time(Duration::from_secs(12)), "12s ago");
        assert_eq!(relative_time(Duration::from_secs(150)), "2m ago");
        assert_eq!(relative_time(Duration::from_secs(4_000)), "1h ago");
        assert_eq!(relative_time(Duration::from_secs(200_000)), "2d ago");
    }

    #[test]
    fn status_message_expires_after_ttl() {
        use super::{MessageKind, StatusMessage};
        use std::time::{Duration, Instant};
        // Construct a message stamped far in the past.
        let past = Instant::now() - Duration::from_secs(60);
        let stale = StatusMessage {
            kind: MessageKind::Info,
            text: "old".into(),
            set_at: past,
        };
        assert!(!stale.is_fresh());

        let fresh = StatusMessage {
            kind: MessageKind::Error,
            text: "new".into(),
            set_at: Instant::now(),
        };
        assert!(fresh.is_fresh());
    }

    #[test]
    fn starred_virtual_category_shows_favorites() {
        use tempfile::TempDir;
        let tmp = TempDir::new().unwrap();
        let fav_path = tmp.path().join("fav.yaml");
        let mut favs = FavoritesStore::load(&fav_path);
        favs.toggle("infra", "gateway");

        let config = Config {
            defaults: Default::default(),
            sync: None,
            categories: vec![Category {
                name: "infra".into(),
                icon: None,
                hosts: vec![
                    Host {
                        name: "gateway".into(),
                        ip: "10.0.0.1".into(),
                        ..Default::default()
                    },
                    Host {
                        name: "db".into(),
                        ip: "10.0.0.2".into(),
                        ..Default::default()
                    },
                ],
            }],
        };

        let mut app = App::new(
            config,
            PathBuf::from("/tmp/x.yaml"),
            SyncStatus::Disabled,
            HistoryStore::default(),
            favs,
            TerminalLauncher::detect(None),
        );

        // First category should be "★ Starred"
        app.categories_state.select(Some(0));
        let hosts = app.filtered_hosts();
        assert_eq!(hosts.len(), 1);
        assert_eq!(hosts[0].host.name, "gateway");
    }
}
