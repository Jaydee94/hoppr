use std::{env, process::Command};

use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ratatui::widgets::ListState;

use crate::config::{Category, Config, Host};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Categories,
    Hosts,
    Search,
}

pub struct App {
    pub config: Config,
    pub focus: Focus,
    pub search_query: String,
    pub categories_state: ListState,
    pub hosts_state: ListState,
    matcher: SkimMatcherV2,
}

impl App {
    pub fn new(config: Config) -> Self {
        let mut categories_state = ListState::default();
        categories_state.select((!config.categories.is_empty()).then_some(0));

        let mut app = Self {
            config,
            focus: Focus::Hosts,
            search_query: String::new(),
            categories_state,
            hosts_state: ListState::default(),
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

        scored.sort_by(|left, right| right.0.cmp(&left.0));
        scored.into_iter().map(|(_, host)| host).collect()
    }

    pub fn selected_host(&self) -> Option<&Host> {
        let hosts = self.filtered_hosts();
        self.hosts_state
            .selected()
            .and_then(|idx| hosts.get(idx).copied())
    }

    pub fn selected_host_command(&self) -> Option<Command> {
        let host = self.selected_host()?;

        if let Some(cmd) = host.cmd.as_deref() {
            let mut command = Command::new("sh");
            command.arg("-c").arg(cmd);
            return Some(command);
        }

        let mut command = Command::new("ssh");
        let user = host
            .user
            .clone()
            .or_else(|| env::var("USER").ok())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| String::from("root"));
        let port = host.port.unwrap_or(22);

        command.arg("-p").arg(port.to_string());
        command.arg(format!("{}@{}", user, host.ip));

        Some(command)
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
}

#[cfg(test)]
mod tests {
    use crate::config::{Category, Config, Host};

    use super::App;

    #[test]
    fn fuzzy_filter_matches_name_and_ip() {
        let config = Config {
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
                    },
                    Host {
                        name: "db".into(),
                        ip: "192.168.1.10".into(),
                        user: None,
                        port: None,
                        cmd: None,
                    },
                ],
            }],
        };

        let mut app = App::new(config);
        app.search_query = "gate".into();
        assert_eq!(app.filtered_hosts().len(), 1);

        app.search_query = "192".into();
        assert_eq!(app.filtered_hosts()[0].name, "db");
    }

    #[test]
    fn command_prefers_custom_cmd_over_ssh() {
        let config = Config {
            categories: vec![Category {
                name: "ops".into(),
                icon: None,
                hosts: vec![Host {
                    name: "custom".into(),
                    ip: "example.com".into(),
                    user: Some("alice".into()),
                    port: Some(2200),
                    cmd: Some("echo hello".into()),
                }],
            }],
        };

        let app = App::new(config);
        let command = app.selected_host_command().expect("command should exist");
        assert_eq!(command.get_program(), "sh");
    }
}
