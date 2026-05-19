//! In-TUI settings editor.
//!
//! The editor is a modal overlay that lets the user mutate the loaded
//! [`Config`] — adding/removing categories and hosts, editing fields,
//! and tweaking global defaults. Saving writes the YAML and, when
//! `sync.auto_push` is enabled, triggers an upstream commit.

use crate::config::{Category, Config, ConnectCommand, Host};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorView {
    Menu,
    Categories,
    Hosts,
    HostForm,
    CategoryForm,
    Defaults,
    Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormMode {
    Create,
    Update,
}

#[derive(Debug, Default, Clone)]
pub struct HostForm {
    pub mode_create: bool,
    pub category_index: usize,
    pub host_index: Option<usize>,
    pub fields: [String; 7], // name, ip, user, port, cmd, command_program, command_args
    pub focused: usize,
    pub submitted_once: bool,
}

impl HostForm {
    pub const LABELS: [&'static str; 7] = [
        "Name",
        "IP / Host",
        "User",
        "Port",
        "Raw cmd",
        "Program",
        "Args",
    ];

    /// Hint shown beneath the Args input — surfaces the splitting rule and the
    /// available placeholders without forcing the user into the YAML docs.
    pub const ARGS_HINT: &'static str =
        " space-separated · supports {user}/{host}/{ip}/{port}/{name}";

    pub fn new_create(category_index: usize) -> Self {
        Self {
            mode_create: true,
            category_index,
            host_index: None,
            fields: Default::default(),
            focused: 0,
            submitted_once: false,
        }
    }

    pub fn new_edit(category_index: usize, host_index: usize, host: &Host) -> Self {
        let (program, args) = match host.command.as_ref() {
            Some(ConnectCommand::Program(p)) => (p.clone(), String::new()),
            Some(ConnectCommand::Template { program, args }) => (program.clone(), args.join(" ")),
            None => (String::new(), String::new()),
        };
        let fields = [
            host.name.clone(),
            host.ip.clone(),
            host.user.clone().unwrap_or_default(),
            host.port.map(|p| p.to_string()).unwrap_or_default(),
            host.cmd.clone().unwrap_or_default(),
            program,
            args,
        ];
        Self {
            mode_create: false,
            category_index,
            host_index: Some(host_index),
            fields,
            focused: 0,
            submitted_once: false,
        }
    }

    /// Validation message for the field at `idx`, or `None` if no error
    /// should be surfaced. Returns `None` until the user has attempted to
    /// submit at least once so editing stays quiet on the first pass.
    pub fn field_error(&self, idx: usize) -> Option<&'static str> {
        if !self.submitted_once {
            return None;
        }
        match idx {
            0 if self.fields[0].trim().is_empty() => Some("required"),
            1 if self.fields[1].trim().is_empty() => Some("required"),
            3 => {
                let port = self.fields[3].trim();
                if !port.is_empty() && port.parse::<u16>().is_err() {
                    Some("0-65535")
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    pub fn next_field(&mut self) {
        self.focused = (self.focused + 1) % self.fields.len();
    }

    pub fn prev_field(&mut self) {
        self.focused = (self.focused + self.fields.len() - 1) % self.fields.len();
    }

    pub fn push_char(&mut self, ch: char) {
        self.fields[self.focused].push(ch);
    }

    pub fn pop_char(&mut self) {
        self.fields[self.focused].pop();
    }

    pub fn to_host(&self) -> Result<Host, String> {
        let name = self.fields[0].trim();
        let ip = self.fields[1].trim();
        if name.is_empty() {
            return Err("Name is required".into());
        }
        if ip.is_empty() {
            return Err("IP / host is required".into());
        }
        let port = if self.fields[3].trim().is_empty() {
            None
        } else {
            Some(
                self.fields[3]
                    .trim()
                    .parse::<u16>()
                    .map_err(|_| "Port must be a number 0-65535".to_string())?,
            )
        };
        let user = empty_to_none(&self.fields[2]);
        let cmd = empty_to_none(&self.fields[4]);
        let program = empty_to_none(&self.fields[5]);
        let args: Vec<String> = self.fields[6]
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let command = match (program, args.is_empty()) {
            (Some(program), true) => Some(ConnectCommand::Program(program)),
            (Some(program), false) => Some(ConnectCommand::Template { program, args }),
            (None, true) => None,
            (None, false) => return Err("Args require a program".into()),
        };

        Ok(Host {
            name: name.into(),
            ip: ip.into(),
            user,
            port,
            cmd,
            command,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct CategoryForm {
    pub mode_create: bool,
    pub category_index: Option<usize>,
    pub fields: [String; 2], // name, icon
    pub focused: usize,
    pub submitted_once: bool,
}

impl CategoryForm {
    pub const LABELS: [&'static str; 2] = ["Name", "Icon"];

    pub fn new_create() -> Self {
        Self {
            mode_create: true,
            category_index: None,
            fields: Default::default(),
            focused: 0,
            submitted_once: false,
        }
    }

    pub fn new_edit(index: usize, category: &Category) -> Self {
        Self {
            mode_create: false,
            category_index: Some(index),
            fields: [
                category.name.clone(),
                category.icon.clone().unwrap_or_default(),
            ],
            focused: 0,
            submitted_once: false,
        }
    }

    /// Validation message for the field at `idx`, or `None` if no error
    /// should be surfaced. Returns `None` until the user has attempted to
    /// submit at least once.
    pub fn field_error(&self, idx: usize) -> Option<&'static str> {
        if !self.submitted_once {
            return None;
        }
        match idx {
            0 if self.fields[0].trim().is_empty() => Some("required"),
            _ => None,
        }
    }

    pub fn next_field(&mut self) {
        self.focused = (self.focused + 1) % self.fields.len();
    }

    pub fn prev_field(&mut self) {
        self.focused = (self.focused + self.fields.len() - 1) % self.fields.len();
    }

    pub fn push_char(&mut self, ch: char) {
        self.fields[self.focused].push(ch);
    }

    pub fn pop_char(&mut self) {
        self.fields[self.focused].pop();
    }

    pub fn apply(&self, target: &mut Category) -> Result<(), String> {
        let name = self.fields[0].trim();
        if name.is_empty() {
            return Err("Name is required".into());
        }
        target.name = name.into();
        target.icon = empty_to_none(&self.fields[1]);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingDelete {
    Category {
        name: String,
    },
    Host {
        category_name: String,
        host_name: String,
    },
}

#[derive(Debug, Clone)]
pub struct EditorState {
    pub view: EditorView,
    pub menu_index: usize,
    pub categories_index: usize,
    pub hosts_index: usize,
    pub host_form: Option<HostForm>,
    pub category_form: Option<CategoryForm>,
    pub defaults_field: usize,
    pub defaults_inputs: [String; 4], // command program, port, user, terminal_command
    pub sync_field: usize,
    pub sync_inputs: [String; 6], // repo, branch, path, local, auto_pull, auto_push
    pub dirty: bool,
    pub flash: Option<String>,
    /// Set when the user pressed Esc on the menu while `dirty` was true.
    /// While true, the event handler short-circuits to a save/discard/cancel
    /// prompt instead of silently writing the config to disk.
    pub pending_exit: bool,
    pub pending_delete: Option<PendingDelete>,
    pub category_filter: Option<String>,
    pub host_filter: Option<String>,
    pub filter_focus: bool,
}

/// Layout of the sync editor.
///
/// The first six positions back the form inputs in `sync_inputs`. The
/// last three are virtual focus slots for the action buttons rendered
/// underneath the form — they don't read from `sync_inputs`, they fire
/// their action on Enter.
pub const SYNC_REPO: usize = 0;
#[allow(dead_code)]
pub const SYNC_BRANCH: usize = 1;
#[allow(dead_code)]
pub const SYNC_PATH: usize = 2;
#[allow(dead_code)]
pub const SYNC_LOCAL: usize = 3;
pub const SYNC_AUTO_PULL: usize = 4;
pub const SYNC_AUTO_PUSH: usize = 5;
pub const SYNC_BTN_TEST: usize = 6;
pub const SYNC_BTN_SYNC: usize = 7;
pub const SYNC_BTN_SAVE: usize = 8;
/// Total navigable elements in the sync editor (fields + buttons).
pub const SYNC_ELEMENT_COUNT: usize = 9;

pub fn sync_field_is_bool(field: usize) -> bool {
    field == SYNC_AUTO_PULL || field == SYNC_AUTO_PUSH
}

pub fn sync_field_is_button(field: usize) -> bool {
    matches!(field, SYNC_BTN_TEST | SYNC_BTN_SYNC | SYNC_BTN_SAVE)
}

impl EditorState {
    pub fn from_config(config: &Config) -> Self {
        let defaults_inputs = [
            config.defaults.command.program().to_string(),
            config.defaults.port.to_string(),
            config.defaults.user.clone().unwrap_or_default(),
            config.defaults.terminal_command.clone().unwrap_or_default(),
        ];
        let sync = config.sync.clone().unwrap_or_default();
        let sync_inputs = [
            sync.repo.unwrap_or_default(),
            sync.branch.unwrap_or_default(),
            sync.path.unwrap_or_default(),
            sync.local.unwrap_or_default(),
            sync.auto_pull.map(bool_str).unwrap_or_default(),
            sync.auto_push.map(bool_str).unwrap_or_default(),
        ];
        Self {
            view: EditorView::Menu,
            menu_index: 0,
            categories_index: 0,
            hosts_index: 0,
            host_form: None,
            category_form: None,
            defaults_field: 0,
            defaults_inputs,
            sync_field: 0,
            sync_inputs,
            dirty: false,
            flash: None,
            pending_exit: false,
            pending_delete: None,
            category_filter: None,
            host_filter: None,
            filter_focus: false,
        }
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some(msg.into());
    }

    /// Stage a destructive delete so the user confirms before the entry is
    /// removed. Use [`Self::confirm_delete`] / [`Self::cancel_delete`] to
    /// resolve the prompt.
    pub fn request_delete(&mut self, pending: PendingDelete) {
        self.pending_delete = Some(pending);
    }

    /// Apply the queued delete against `config`. Returns `true` when a delete
    /// actually happened — callers should flash a "removed" message and mark
    /// the editor dirty. Returns `false` if no delete was pending or if the
    /// referenced entry no longer exists (e.g. the config changed between the
    /// keypress and the confirmation).
    pub fn confirm_delete(&mut self, config: &mut Config) -> bool {
        let Some(pending) = self.pending_delete.take() else {
            return false;
        };
        match pending {
            PendingDelete::Category { name } => {
                if let Some(idx) = config.categories.iter().position(|c| c.name == name) {
                    config.categories.remove(idx);
                    self.dirty = true;
                    return true;
                }
            }
            PendingDelete::Host {
                category_name,
                host_name,
            } => {
                if let Some(cat) = config
                    .categories
                    .iter_mut()
                    .find(|c| c.name == category_name)
                {
                    if let Some(idx) = cat.hosts.iter().position(|h| h.name == host_name) {
                        cat.hosts.remove(idx);
                        self.dirty = true;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Drop the queued delete without mutating the config. Returns `true`
    /// when there was a pending delete to cancel, so callers can decide
    /// whether to surface a "cancelled" flash.
    pub fn cancel_delete(&mut self) -> bool {
        self.pending_delete.take().is_some()
    }

    pub fn clamp(&mut self, config: &Config) {
        if config.categories.is_empty() {
            self.categories_index = 0;
            self.hosts_index = 0;
        } else {
            self.categories_index = self.categories_index.min(config.categories.len() - 1);
            let hosts = &config.categories[self.categories_index].hosts;
            self.hosts_index = if hosts.is_empty() {
                0
            } else {
                self.hosts_index.min(hosts.len() - 1)
            };
        }
    }

    /// Categories paired with their original index, filtered by the
    /// current `category_filter`. The original index is what callers
    /// must use when mutating `config.categories` so that deletes and
    /// edits land on the right entry — never the visible offset.
    pub fn visible_categories<'a>(&self, config: &'a Config) -> Vec<(usize, &'a Category)> {
        let needle = self
            .category_filter
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_lowercase);
        config
            .categories
            .iter()
            .enumerate()
            .filter(|(_, cat)| match needle.as_deref() {
                Some(q) => cat.name.to_lowercase().contains(q),
                None => true,
            })
            .collect()
    }

    /// Hosts of the currently selected category paired with their
    /// original index, filtered by the current `host_filter`.
    pub fn visible_hosts<'a>(&self, config: &'a Config) -> Vec<(usize, &'a Host)> {
        let Some(cat) = config.categories.get(self.categories_index) else {
            return Vec::new();
        };
        let needle = self
            .host_filter
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty())
            .map(str::to_lowercase);
        cat.hosts
            .iter()
            .enumerate()
            .filter(|(_, host)| match needle.as_deref() {
                Some(q) => host.name.to_lowercase().contains(q),
                None => true,
            })
            .collect()
    }

    pub fn apply_defaults(&mut self, config: &mut Config) -> Result<(), String> {
        let program = self.defaults_inputs[0].trim();
        if program.is_empty() {
            return Err("Command program is required".into());
        }
        let port: u16 = self.defaults_inputs[1]
            .trim()
            .parse()
            .map_err(|_| "Port must be a number 0-65535".to_string())?;

        config.defaults.command = ConnectCommand::Program(program.into());
        config.defaults.port = port;
        config.defaults.user = empty_to_none(&self.defaults_inputs[2]);
        config.defaults.terminal_command = empty_to_none(&self.defaults_inputs[3]);
        self.dirty = true;
        Ok(())
    }

    pub fn apply_sync(&mut self, config: &mut Config) -> Result<(), String> {
        let repo = empty_to_none(&self.sync_inputs[0]);
        if repo.is_none() {
            config.sync = None;
            self.dirty = true;
            return Ok(());
        }
        let mut sync = config.sync.clone().unwrap_or_default();
        sync.repo = repo;
        sync.branch = empty_to_none(&self.sync_inputs[1]);
        sync.path = empty_to_none(&self.sync_inputs[2]);
        sync.local = empty_to_none(&self.sync_inputs[3]);
        sync.auto_pull = parse_bool(&self.sync_inputs[4])?;
        sync.auto_push = parse_bool(&self.sync_inputs[5])?;
        config.sync = Some(sync);
        self.dirty = true;
        Ok(())
    }

    /// Flip the boolean field at `field` (indexed by `SYNC_AUTO_*`).
    /// Empty values become `true`, `true` becomes `false`, `false` becomes
    /// `true`. Returns the new textual representation.
    pub fn toggle_sync_bool(&mut self, field: usize) -> &str {
        if !sync_field_is_bool(field) {
            return "";
        }
        let next = match parse_bool(&self.sync_inputs[field]).ok().flatten() {
            Some(true) => "false",
            Some(false) | None => "true",
        };
        self.sync_inputs[field] = next.into();
        &self.sync_inputs[field]
    }
}

fn empty_to_none(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_bool(input: &str) -> Result<Option<bool>, String> {
    let s = input.trim().to_lowercase();
    if s.is_empty() {
        return Ok(None);
    }
    match s.as_str() {
        "true" | "yes" | "y" | "1" | "on" => Ok(Some(true)),
        "false" | "no" | "n" | "0" | "off" => Ok(Some(false)),
        other => Err(format!("invalid boolean: {other}")),
    }
}

fn bool_str(b: bool) -> String {
    if b {
        "true".into()
    } else {
        "false".into()
    }
}

pub const MENU_ITEMS: &[(&str, EditorView)] = &[
    ("Manage categories", EditorView::Categories),
    ("Manage hosts", EditorView::Hosts),
    ("Connection defaults", EditorView::Defaults),
    ("Central repo sync", EditorView::Sync),
];

#[allow(dead_code)]
pub const FORM_MODE_CREATE: FormMode = FormMode::Create;
#[allow(dead_code)]
pub const FORM_MODE_UPDATE: FormMode = FormMode::Update;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn toggle_sync_bool_cycles_through_states() {
        let mut state = EditorState::from_config(&Config::default());
        // Unset -> true
        assert_eq!(state.toggle_sync_bool(SYNC_AUTO_PULL), "true");
        // true -> false
        assert_eq!(state.toggle_sync_bool(SYNC_AUTO_PULL), "false");
        // false -> true
        assert_eq!(state.toggle_sync_bool(SYNC_AUTO_PULL), "true");
    }

    #[test]
    fn toggle_sync_bool_ignores_non_bool_fields() {
        let mut state = EditorState::from_config(&Config::default());
        state.sync_inputs[SYNC_REPO] = "git@example.com:x/y.git".into();
        assert_eq!(state.toggle_sync_bool(SYNC_REPO), "");
        assert_eq!(state.sync_inputs[SYNC_REPO], "git@example.com:x/y.git");
    }

    #[test]
    fn apply_defaults_writes_terminal_command_when_set() {
        let mut config = Config::default();
        let mut state = EditorState::from_config(&config);
        state.defaults_inputs[3] = "  wt  ".into();
        state.apply_defaults(&mut config).unwrap();
        assert_eq!(config.defaults.terminal_command.as_deref(), Some("wt"));
    }

    #[test]
    fn host_form_round_trips_template_command() {
        let host = Host {
            name: "edge".into(),
            ip: "10.0.0.5".into(),
            user: Some("ops".into()),
            port: Some(2222),
            cmd: None,
            command: Some(ConnectCommand::Template {
                program: "kitty".into(),
                args: vec![
                    "+kitten".into(),
                    "ssh".into(),
                    "-p".into(),
                    "{port}".into(),
                    "{user}@{host}".into(),
                ],
            }),
        };
        let form = HostForm::new_edit(0, 0, &host);
        assert_eq!(form.fields[5], "kitty");
        assert_eq!(form.fields[6], "+kitten ssh -p {port} {user}@{host}");
        let round_tripped = form.to_host().expect("template round-trip");
        assert_eq!(round_tripped, host);
    }

    #[test]
    fn host_form_round_trips_program_only_command() {
        let host = Host {
            name: "edge".into(),
            ip: "10.0.0.5".into(),
            user: Some("ops".into()),
            port: Some(22),
            cmd: None,
            command: Some(ConnectCommand::Program("mosh".into())),
        };
        let form = HostForm::new_edit(0, 0, &host);
        assert_eq!(form.fields[5], "mosh");
        assert!(form.fields[6].is_empty());
        let round_tripped = form.to_host().expect("program round-trip");
        assert_eq!(round_tripped, host);
    }

    #[test]
    fn host_form_rejects_args_without_program() {
        let mut form = HostForm::new_create(0);
        form.fields[0] = "edge".into();
        form.fields[1] = "10.0.0.5".into();
        form.fields[6] = "--foo".into();
        let err = form
            .to_host()
            .expect_err("args without program should fail");
        assert!(err.contains("program"), "unexpected error: {err}");
    }

    #[test]
    fn host_form_field_error_reports_required_name_after_submit() {
        let mut form = HostForm::new_create(0);
        assert!(!form.submitted_once);
        assert_eq!(form.field_error(0), None);
        form.submitted_once = true;
        assert_eq!(form.field_error(0), Some("required"));
    }

    #[test]
    fn host_form_field_error_flags_unparseable_port() {
        let mut form = HostForm::new_create(0);
        form.fields[0] = "web".into();
        form.fields[1] = "10.0.0.1".into();
        form.fields[3] = "abc".into();
        form.submitted_once = true;
        assert_eq!(form.field_error(3), Some("0-65535"));
    }

    #[test]
    fn host_form_field_error_none_when_valid() {
        let mut form = HostForm::new_create(0);
        form.fields[0] = "web".into();
        form.fields[1] = "10.0.0.1".into();
        form.fields[3] = "22".into();
        form.submitted_once = true;
        for idx in 0..form.fields.len() {
            assert_eq!(form.field_error(idx), None, "idx {idx}");
        }
    }

    #[test]
    fn category_form_field_error_reports_required_name() {
        let mut form = CategoryForm::new_create();
        assert_eq!(form.field_error(0), None);
        form.submitted_once = true;
        assert_eq!(form.field_error(0), Some("required"));
        form.fields[0] = "ops".into();
        assert_eq!(form.field_error(0), None);
        assert_eq!(form.field_error(1), None);
    }

    fn config_with(categories: Vec<(&str, Vec<&str>)>) -> Config {
        Config {
            defaults: Default::default(),
            sync: None,
            categories: categories
                .into_iter()
                .map(|(name, hosts)| Category {
                    name: name.into(),
                    icon: None,
                    hosts: hosts
                        .into_iter()
                        .map(|h| Host {
                            name: h.into(),
                            ip: "10.0.0.1".into(),
                            user: None,
                            port: None,
                            cmd: None,
                            command: None,
                        })
                        .collect(),
                })
                .collect(),
        }
    }

    #[test]
    fn visible_categories_unfiltered_returns_everything_with_indices() {
        let config = config_with(vec![("Ops", vec![]), ("Home", vec![]), ("Cloud", vec![])]);
        let state = EditorState::from_config(&config);
        let visible = state.visible_categories(&config);
        assert_eq!(visible.len(), 3);
        assert_eq!(visible[0].0, 0);
        assert_eq!(visible[1].0, 1);
        assert_eq!(visible[2].0, 2);
        assert_eq!(visible[0].1.name, "Ops");
    }

    #[test]
    fn visible_categories_filters_case_insensitive() {
        let config = config_with(vec![
            ("Ops", vec![]),
            ("Home Lab", vec![]),
            ("Cloud", vec![]),
        ]);
        let mut state = EditorState::from_config(&config);
        state.category_filter = Some("home".into());
        let visible = state.visible_categories(&config);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].0, 1);
        assert_eq!(visible[0].1.name, "Home Lab");
    }

    #[test]
    fn visible_categories_treats_empty_filter_as_no_op() {
        let config = config_with(vec![("Ops", vec![]), ("Home", vec![])]);
        let mut state = EditorState::from_config(&config);
        state.category_filter = Some(String::new());
        assert_eq!(state.visible_categories(&config).len(), 2);
        state.category_filter = Some("   ".into());
        assert_eq!(state.visible_categories(&config).len(), 2);
    }

    #[test]
    fn visible_hosts_filters_current_category_only() {
        let config = config_with(vec![
            ("Ops", vec!["web", "db", "cache"]),
            ("Home", vec!["web-home"]),
        ]);
        let mut state = EditorState::from_config(&config);
        state.categories_index = 0;
        state.host_filter = Some("DB".into());
        let visible = state.visible_hosts(&config);
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].0, 1);
        assert_eq!(visible[0].1.name, "db");
    }

    #[test]
    fn visible_hosts_unfiltered_preserves_original_indices() {
        let config = config_with(vec![("Ops", vec!["a", "b", "c"])]);
        let state = EditorState::from_config(&config);
        let visible = state.visible_hosts(&config);
        assert_eq!(
            visible.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }

    #[test]
    fn visible_hosts_returns_empty_for_missing_category() {
        let config = config_with(vec![]);
        let state = EditorState::from_config(&config);
        assert!(state.visible_hosts(&config).is_empty());
    }

    #[test]
    fn pending_exit_toggle_preserves_dirty() {
        let mut state = EditorState::from_config(&Config::default());
        state.dirty = true;
        assert!(!state.pending_exit);
        state.pending_exit = true;
        assert!(state.dirty, "raising pending_exit must not clear dirty");
        state.pending_exit = false;
        assert!(state.dirty, "lowering pending_exit must not clear dirty");
    }

    #[test]
    fn apply_defaults_clears_terminal_command_when_empty() {
        let mut config = Config::default();
        config.defaults.terminal_command = Some("alacritty -e".into());
        let mut state = EditorState::from_config(&config);
        assert_eq!(state.defaults_inputs[3], "alacritty -e");
        state.defaults_inputs[3].clear();
        state.apply_defaults(&mut config).unwrap();
        assert!(config.defaults.terminal_command.is_none());
    }

    fn config_with_one_host() -> Config {
        let mut config = Config::default();
        config.categories.push(Category {
            name: "ops".into(),
            icon: None,
            hosts: vec![Host {
                name: "web-01".into(),
                ip: "10.0.0.1".into(),
                user: None,
                port: None,
                cmd: None,
                command: None,
            }],
        });
        config
    }

    #[test]
    fn request_delete_stores_pending_state() {
        let mut state = EditorState::from_config(&Config::default());
        assert!(state.pending_delete.is_none());
        state.request_delete(PendingDelete::Category { name: "ops".into() });
        assert_eq!(
            state.pending_delete,
            Some(PendingDelete::Category { name: "ops".into() })
        );
    }

    #[test]
    fn confirm_delete_removes_category_and_marks_dirty() {
        let mut config = config_with_one_host();
        let mut state = EditorState::from_config(&config);
        state.request_delete(PendingDelete::Category { name: "ops".into() });
        assert!(state.confirm_delete(&mut config));
        assert!(config.categories.is_empty());
        assert!(state.dirty);
        assert!(state.pending_delete.is_none());
    }

    #[test]
    fn confirm_delete_removes_host_and_marks_dirty() {
        let mut config = config_with_one_host();
        let mut state = EditorState::from_config(&config);
        state.request_delete(PendingDelete::Host {
            category_name: "ops".into(),
            host_name: "web-01".into(),
        });
        assert!(state.confirm_delete(&mut config));
        assert!(config.categories[0].hosts.is_empty());
        assert!(state.dirty);
        assert!(state.pending_delete.is_none());
    }

    #[test]
    fn confirm_delete_without_pending_is_a_noop() {
        let mut config = config_with_one_host();
        let mut state = EditorState::from_config(&config);
        assert!(!state.confirm_delete(&mut config));
        assert!(!state.dirty);
        assert_eq!(config.categories.len(), 1);
    }

    #[test]
    fn confirm_delete_with_missing_target_leaves_state_untouched() {
        let mut config = config_with_one_host();
        let mut state = EditorState::from_config(&config);
        state.request_delete(PendingDelete::Category {
            name: "missing".into(),
        });
        assert!(!state.confirm_delete(&mut config));
        assert!(!state.dirty);
        assert!(state.pending_delete.is_none());
        assert_eq!(config.categories.len(), 1);
    }

    #[test]
    fn cancel_delete_clears_pending_without_mutation() {
        let config = config_with_one_host();
        let mut state = EditorState::from_config(&config);
        state.request_delete(PendingDelete::Host {
            category_name: "ops".into(),
            host_name: "web-01".into(),
        });
        assert!(state.cancel_delete());
        assert!(state.pending_delete.is_none());
        assert!(!state.dirty);
        assert_eq!(config.categories[0].hosts.len(), 1);
    }

    #[test]
    fn cancel_delete_when_idle_returns_false() {
        let mut state = EditorState::from_config(&Config::default());
        assert!(!state.cancel_delete());
    }
}
