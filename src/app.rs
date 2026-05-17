//! Top-level application state for the TUI.

use std::{cmp::Reverse, path::PathBuf};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ratatui::widgets::ListState;

use crate::{
    config::{Category, Config, Host},
    editor::EditorState,
    sync::SyncStatus,
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

pub struct App {
    pub config: Config,
    pub config_path: PathBuf,
    pub focus: Focus,
    pub mode: Mode,
    pub search_query: String,
    pub categories_state: ListState,
    pub hosts_state: ListState,
    pub sync_status: SyncStatus,
    pub status_message: Option<String>,
    pub editor: Option<EditorState>,
    matcher: SkimMatcherV2,
}

impl App {
    pub fn new(config: Config, config_path: PathBuf, sync_status: SyncStatus) -> Self {
        let mut categories_state = ListState::default();
        categories_state.select((!config.categories.is_empty()).then_some(0));

        let mut app = Self {
            config,
            config_path,
            focus: Focus::Hosts,
            mode: Mode::Browse,
            search_query: String::new(),
            categories_state,
            hosts_state: ListState::default(),
            sync_status,
            status_message: None,
            editor: None,
            matcher: SkimMatcherV2::default(),
        };
        app.ensure_valid_selection();
        app
    }

    pub fn current_category(&self) -> Option<&Category> {
        self.categories_state
            .selected()
            .and_then(|index| self.config.categories.get(index))
    }

    pub fn filtered_hosts(&self) -> Vec<&Host> {
        let Some(category) = self.current_category() else {
            return Vec::new();
        };
        if self.search_query.trim().is_empty() {
            return category.hosts.iter().collect();
        }
        let mut scored = category
            .hosts
            .iter()
            .filter_map(|host| {
                let haystack = format!("{} {}", host.name, host.ip);
                self.matcher
                    .fuzzy_match(&haystack, &self.search_query)
                    .map(|score| (score, host))
            })
            .collect::<Vec<_>>();
        scored.sort_by_key(|(score, _)| Reverse(*score));
        scored.into_iter().map(|(_, host)| host).collect()
    }

    pub fn selected_host(&self) -> Option<&Host> {
        let hosts = self.filtered_hosts();
        self.hosts_state
            .selected()
            .and_then(|idx| hosts.get(idx).copied())
    }

    pub fn ensure_valid_selection(&mut self) {
        let category_len = self.config.categories.len();
        if category_len == 0 {
            self.categories_state.select(None);
            self.hosts_state.select(None);
            return;
        }

        let selected_category = self
            .categories_state
            .selected()
            .unwrap_or(0)
            .min(category_len - 1);
        self.categories_state.select(Some(selected_category));

        let host_len = self.filtered_hosts().len();
        if host_len == 0 {
            self.hosts_state.select(None);
        } else {
            let selected_host = self.hosts_state.selected().unwrap_or(0).min(host_len - 1);
            self.hosts_state.select(Some(selected_host));
        }
    }

    pub fn next(&mut self) {
        match self.focus {
            Focus::Categories => {
                let len = self.config.categories.len();
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
                let len = self.config.categories.len();
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
        self.status_message = Some(msg.into());
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::{
        config::{Category, Config, Host},
        sync::SyncStatus,
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
        App::new(config, PathBuf::from("/tmp/x.yaml"), SyncStatus::Disabled)
    }

    #[test]
    fn fuzzy_filter_matches_name_and_ip() {
        let mut app = fixture();
        app.search_query = "gate".into();
        assert_eq!(app.filtered_hosts().len(), 1);

        app.search_query = "192".into();
        assert_eq!(app.filtered_hosts()[0].name, "db");
    }

    #[test]
    fn edit_mode_toggles() {
        let mut app = fixture();
        app.enter_edit_mode();
        assert!(matches!(app.mode, super::Mode::Edit));
        app.exit_edit_mode();
        assert!(matches!(app.mode, super::Mode::Browse));
    }
}
