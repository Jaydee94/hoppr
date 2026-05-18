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
    pub fields: [String; 6], // name, ip, user, port, cmd, command_program
    pub focused: usize,
}

impl HostForm {
    pub const LABELS: [&'static str; 6] =
        ["Name", "IP / Host", "User", "Port", "Raw cmd", "Program"];

    pub fn new_create(category_index: usize) -> Self {
        Self {
            mode_create: true,
            category_index,
            host_index: None,
            fields: Default::default(),
            focused: 0,
        }
    }

    pub fn new_edit(category_index: usize, host_index: usize, host: &Host) -> Self {
        let fields = [
            host.name.clone(),
            host.ip.clone(),
            host.user.clone().unwrap_or_default(),
            host.port.map(|p| p.to_string()).unwrap_or_default(),
            host.cmd.clone().unwrap_or_default(),
            host.command
                .as_ref()
                .map(|c| c.program().to_string())
                .unwrap_or_default(),
        ];
        Self {
            mode_create: false,
            category_index,
            host_index: Some(host_index),
            fields,
            focused: 0,
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
        let command = empty_to_none(&self.fields[5]).map(ConnectCommand::Program);

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
}

impl CategoryForm {
    pub const LABELS: [&'static str; 2] = ["Name", "Icon"];

    pub fn new_create() -> Self {
        Self {
            mode_create: true,
            category_index: None,
            fields: Default::default(),
            focused: 0,
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

#[derive(Debug, Clone)]
pub struct EditorState {
    pub view: EditorView,
    pub menu_index: usize,
    pub categories_index: usize,
    pub hosts_index: usize,
    pub host_form: Option<HostForm>,
    pub category_form: Option<CategoryForm>,
    pub defaults_field: usize,
    pub defaults_inputs: [String; 3], // command program, port, user
    pub sync_field: usize,
    pub sync_inputs: [String; 6], // repo, branch, path, local, auto_pull, auto_push
    pub dirty: bool,
    pub flash: Option<String>,
}

/// Layout of the sync editor — strings indexed by field position.
pub const SYNC_REPO: usize = 0;
#[allow(dead_code)]
pub const SYNC_BRANCH: usize = 1;
#[allow(dead_code)]
pub const SYNC_PATH: usize = 2;
#[allow(dead_code)]
pub const SYNC_LOCAL: usize = 3;
pub const SYNC_AUTO_PULL: usize = 4;
pub const SYNC_AUTO_PUSH: usize = 5;
pub const SYNC_FIELD_COUNT: usize = 6;

pub fn sync_field_is_bool(field: usize) -> bool {
    field == SYNC_AUTO_PULL || field == SYNC_AUTO_PUSH
}

impl EditorState {
    pub fn from_config(config: &Config) -> Self {
        let defaults_inputs = [
            config.defaults.command.program().to_string(),
            config.defaults.port.to_string(),
            config.defaults.user.clone().unwrap_or_default(),
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
        }
    }

    pub fn flash(&mut self, msg: impl Into<String>) {
        self.flash = Some(msg.into());
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
}
