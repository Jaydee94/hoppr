//! Key handling for the modal settings editor.
//!
//! Each editor view has its own handler working on the split borrow
//! `(&mut EditorState, &mut Config)`. Anything that needs the whole
//! [`App`] — saving, leaving edit mode, running a sync — is returned as an
//! [`Action`] and performed by [`handle_editor_key`] once the editor borrow
//! has ended.

use anyhow::Result;
use crossterm::event::KeyCode;

use crate::{
    app::App,
    config::{Category, Config, Inventory},
    editor::{
        CategoryForm, EditorState, EditorView, HostForm, MENU_ITEMS, PendingDelete, SYNC_AUTO_PULL,
        SYNC_AUTO_PUSH, SYNC_BTN_SAVE, SYNC_BTN_SYNC, SYNC_BTN_TEST, SYNC_ELEMENT_COUNT, SYNC_REPO,
        sync_field_is_bool, sync_field_is_button,
    },
    sync::{self, SyncContext, SyncStatus},
};

/// What the event loop should do after a key press was handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Quit,
}

/// App-level follow-up requested by an editor view handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    None,
    Save,
    Exit,
    SaveAndExit,
    DiscardAndExit,
    PullNow,
    CloneIfMissing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Back,
    Forward,
}

pub fn handle_editor_key(app: &mut App, code: KeyCode, ctrl: bool) -> Result<Flow> {
    if ctrl && matches!(code, KeyCode::Char('c')) {
        return Ok(Flow::Quit);
    }
    let Some(editor) = app.editor.as_mut() else {
        return Ok(Flow::Continue);
    };
    let config = &mut app.config;
    editor.clamp(config);

    // The unsaved-changes prompt owns the whole event loop until the user
    // resolves it — otherwise navigation keys could nudge the editor while
    // the modal is on screen. Resolving a pending delete short-circuits for
    // the same reason: "j" must not both cancel the prompt and move the
    // selection in the same tick.
    let action = if editor.pending_exit {
        handle_pending_exit(editor, code)
    } else if editor.pending_delete.is_some() {
        handle_pending_delete(editor, config, code);
        Action::None
    } else {
        match editor.view {
            EditorView::Menu => handle_menu(editor, code, ctrl),
            EditorView::Categories => handle_categories(editor, config, code, ctrl),
            EditorView::Hosts => handle_hosts(editor, config, code, ctrl),
            EditorView::CategoryForm => {
                if let Some(msg) = handle_category_form(editor, config, code) {
                    editor.flash(msg);
                }
                Action::None
            }
            EditorView::HostForm => {
                if let Some(msg) = handle_host_form(editor, config, code) {
                    editor.flash(msg);
                }
                Action::None
            }
            EditorView::Defaults => handle_defaults(editor, config, code, ctrl),
            EditorView::Sync => handle_sync(editor, config, code, ctrl),
        }
    };

    perform(app, action)?;
    Ok(Flow::Continue)
}

fn perform(app: &mut App, action: Action) -> Result<()> {
    match action {
        Action::None => {}
        Action::Save => save_config(app)?,
        Action::Exit => app.exit_edit_mode(),
        Action::SaveAndExit => {
            save_config(app)?;
            app.exit_edit_mode();
        }
        Action::DiscardAndExit => {
            // If the config has never been saved, reload fails and we
            // simply drop the in-memory edits via exit_edit_mode below.
            if let Ok(cfg) = Config::load_from_path(&app.config_path) {
                app.config = cfg;
            }
            app.exit_edit_mode();
        }
        Action::PullNow => sync_pull_now(app),
        Action::CloneIfMissing => sync_clone_if_missing(app),
    }
    Ok(())
}

fn handle_pending_exit(editor: &mut EditorState, code: KeyCode) -> Action {
    match code {
        KeyCode::Char('s') => Action::SaveAndExit,
        KeyCode::Char('d') => Action::DiscardAndExit,
        KeyCode::Char('c') | KeyCode::Esc => {
            editor.pending_exit = false;
            Action::None
        }
        _ => Action::None,
    }
}

fn handle_pending_delete(editor: &mut EditorState, config: &mut Config, code: KeyCode) {
    if code != KeyCode::Char('y') {
        editor.cancel_delete();
        editor.flash("Delete cancelled");
        return;
    }
    if editor.confirm_delete(config) {
        let msg = match editor.view {
            EditorView::Categories => "Category removed",
            EditorView::Hosts => "Host removed",
            _ => "Removed",
        };
        editor.flash(msg);
    } else {
        editor.flash("Delete cancelled");
    }
}

fn handle_menu(editor: &mut EditorState, code: KeyCode, ctrl: bool) -> Action {
    match code {
        KeyCode::Esc => {
            if editor.dirty {
                editor.pending_exit = true;
            } else {
                return Action::Exit;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            editor.menu_index = (editor.menu_index + MENU_ITEMS.len() - 1) % MENU_ITEMS.len();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            editor.menu_index = (editor.menu_index + 1) % MENU_ITEMS.len();
        }
        KeyCode::Enter => editor.view = MENU_ITEMS[editor.menu_index].1,
        KeyCode::Char('s') if ctrl && editor.dirty => return Action::Save,
        _ => {}
    }
    Action::None
}

fn handle_categories(
    editor: &mut EditorState,
    config: &Config,
    code: KeyCode,
    ctrl: bool,
) -> Action {
    if editor.filter_focus {
        handle_category_filter_input(editor, config, code);
        return Action::None;
    }
    match code {
        KeyCode::Esc => {
            // Second Esc on a non-focused, populated filter clears it,
            // freeing the user from a hidden narrowing they can't see.
            if editor.category_filter.is_some() {
                editor.category_filter = None;
            } else {
                editor.view = EditorView::Menu;
            }
        }
        KeyCode::Char('/') => {
            editor.filter_focus = true;
            if editor.category_filter.is_none() {
                editor.category_filter = Some(String::new());
            }
        }
        KeyCode::Up | KeyCode::Char('k') => move_visible_category(editor, config, Step::Back),
        KeyCode::Down | KeyCode::Char('j') => move_visible_category(editor, config, Step::Forward),
        KeyCode::Char('a') => {
            editor.category_form = Some(CategoryForm::new_create());
            editor.view = EditorView::CategoryForm;
        }
        KeyCode::Char('r') | KeyCode::Enter => {
            if let Some(cat) = config.categories.get(editor.categories_index) {
                editor.category_form = Some(CategoryForm::new_edit(editor.categories_index, cat));
                editor.view = EditorView::CategoryForm;
            }
        }
        KeyCode::Char('d') if !config.categories.is_empty() => {
            if let Some(cat) = config.categories.get(editor.categories_index) {
                editor.request_delete(PendingDelete::Category {
                    name: cat.name.clone(),
                });
            }
        }
        KeyCode::Char('s') if ctrl && editor.dirty => return Action::Save,
        _ => {}
    }
    Action::None
}

fn handle_hosts(editor: &mut EditorState, config: &Config, code: KeyCode, ctrl: bool) -> Action {
    if editor.filter_focus {
        handle_host_filter_input(editor, config, code);
        return Action::None;
    }
    match code {
        KeyCode::Esc => {
            if editor.host_filter.is_some() {
                editor.host_filter = None;
            } else {
                editor.view = EditorView::Menu;
            }
        }
        KeyCode::Char('/') => {
            editor.filter_focus = true;
            if editor.host_filter.is_none() {
                editor.host_filter = Some(String::new());
            }
        }
        KeyCode::Tab => {
            let len = config.categories.len();
            if len > 0 {
                editor.categories_index = (editor.categories_index + 1) % len;
                editor.hosts_index = 0;
                // Filter is scoped to the visible category; switching
                // would otherwise leave a stale narrowing in place.
                editor.host_filter = None;
            }
        }
        KeyCode::BackTab => {
            let len = config.categories.len();
            if len > 0 {
                editor.categories_index = (editor.categories_index + len - 1) % len;
                editor.hosts_index = 0;
                editor.host_filter = None;
            }
        }
        KeyCode::Up | KeyCode::Char('k') => move_visible_host(editor, config, Step::Back),
        KeyCode::Down | KeyCode::Char('j') => move_visible_host(editor, config, Step::Forward),
        KeyCode::Char('a') if !config.categories.is_empty() => {
            editor.host_form = Some(HostForm::new_create(editor.categories_index));
            editor.view = EditorView::HostForm;
        }
        KeyCode::Char('r') | KeyCode::Enter => {
            if let Some(host) = config
                .categories
                .get(editor.categories_index)
                .and_then(|c| c.hosts.get(editor.hosts_index))
            {
                editor.host_form = Some(HostForm::new_edit(
                    editor.categories_index,
                    editor.hosts_index,
                    host,
                ));
                editor.view = EditorView::HostForm;
            }
        }
        KeyCode::Char('d') => {
            if let Some(cat) = config.categories.get(editor.categories_index)
                && let Some(host) = cat.hosts.get(editor.hosts_index)
            {
                editor.request_delete(PendingDelete::Host {
                    category_name: cat.name.clone(),
                    host_name: host.name.clone(),
                });
            }
        }
        KeyCode::Char('s') if ctrl && editor.dirty => return Action::Save,
        _ => {}
    }
    Action::None
}

fn handle_defaults(
    editor: &mut EditorState,
    config: &mut Config,
    code: KeyCode,
    ctrl: bool,
) -> Action {
    let field_count = editor.defaults_inputs.len();
    match code {
        KeyCode::Esc => editor.view = EditorView::Menu,
        KeyCode::Tab | KeyCode::Down => {
            editor.defaults_field = (editor.defaults_field + 1) % field_count;
        }
        KeyCode::BackTab | KeyCode::Up => {
            editor.defaults_field = (editor.defaults_field + field_count - 1) % field_count;
        }
        KeyCode::Backspace => {
            editor.defaults_inputs[editor.defaults_field].pop();
        }
        KeyCode::Char('s') if ctrl => return Action::Save,
        KeyCode::Enter => match editor.apply_defaults(config) {
            Ok(()) => editor.flash("Defaults applied · Ctrl+S to save"),
            Err(err) => editor.flash(err),
        },
        KeyCode::Char(c) => editor.defaults_inputs[editor.defaults_field].push(c),
        _ => {}
    }
    Action::None
}

fn handle_sync(editor: &mut EditorState, config: &mut Config, code: KeyCode, ctrl: bool) -> Action {
    let is_text_field =
        !sync_field_is_bool(editor.sync_field) && !sync_field_is_button(editor.sync_field);
    match code {
        KeyCode::Esc => editor.view = EditorView::Menu,
        KeyCode::Tab | KeyCode::Down => {
            editor.sync_field = (editor.sync_field + 1) % SYNC_ELEMENT_COUNT;
        }
        KeyCode::BackTab | KeyCode::Up => {
            editor.sync_field = (editor.sync_field + SYNC_ELEMENT_COUNT - 1) % SYNC_ELEMENT_COUNT;
        }
        KeyCode::Backspace if is_text_field => {
            editor.sync_inputs[editor.sync_field].pop();
        }
        // Universal save shortcut — kept alongside the [Save & push] button.
        KeyCode::Char('s') if ctrl => return Action::Save,
        KeyCode::Char(' ') if sync_field_is_bool(editor.sync_field) => toggle_sync_field(editor),
        KeyCode::Enter => return activate_sync_field(editor, config),
        KeyCode::Char(c) if is_text_field => editor.sync_inputs[editor.sync_field].push(c),
        _ => {}
    }
    Action::None
}

fn activate_sync_field(editor: &mut EditorState, config: &mut Config) -> Action {
    match editor.sync_field {
        SYNC_BTN_TEST => sync_test_connection(editor),
        SYNC_BTN_SYNC => {
            if sync_apply_form(editor, config) {
                return Action::PullNow;
            }
        }
        SYNC_BTN_SAVE => {
            if sync_apply_form(editor, config) {
                return Action::Save;
            }
        }
        SYNC_AUTO_PULL | SYNC_AUTO_PUSH => toggle_sync_field(editor),
        // Text fields: Enter applies the form so users who don't
        // notice the buttons still have a workable shortcut.
        _ => match editor.apply_sync(config) {
            Ok(()) => {
                editor.flash("Sync applied");
                return Action::CloneIfMissing;
            }
            Err(err) => editor.flash(err),
        },
    }
    Action::None
}

fn toggle_sync_field(editor: &mut EditorState) {
    let field = editor.sync_field;
    let new = editor.toggle_sync_bool(field).to_string();
    editor.dirty = true;
    let name = if field == SYNC_AUTO_PULL {
        "auto_pull"
    } else {
        "auto_push"
    };
    editor.flash(format!("{name} = {new}"));
}

fn handle_category_form(
    editor: &mut EditorState,
    config: &mut Config,
    code: KeyCode,
) -> Option<String> {
    let Some(form) = editor.category_form.as_mut() else {
        editor.view = EditorView::Categories;
        return None;
    };
    match code {
        KeyCode::Esc => {
            editor.category_form = None;
            editor.view = EditorView::Categories;
            None
        }
        KeyCode::Tab | KeyCode::Down => {
            form.next_field();
            None
        }
        KeyCode::BackTab | KeyCode::Up => {
            form.prev_field();
            None
        }
        KeyCode::Backspace => {
            form.pop_char();
            None
        }
        KeyCode::Enter => {
            form.submitted_once = true;
            let mut cat = if form.mode_create {
                Category::default()
            } else {
                config
                    .categories
                    .get(form.category_index.unwrap_or(0))
                    .cloned()
                    .unwrap_or_default()
            };
            if let Err(err) = form.apply(&mut cat) {
                return Some(err);
            }
            if form.mode_create {
                config.categories.push(cat);
            } else if let Some(i) = form.category_index
                && let Some(slot) = config.categories.get_mut(i)
            {
                *slot = cat;
            }
            let created = form.mode_create;
            editor.dirty = true;
            editor.category_form = None;
            editor.view = EditorView::Categories;
            Some(if created {
                "Category added · Ctrl+S to save".into()
            } else {
                "Category updated · Ctrl+S to save".into()
            })
        }
        KeyCode::Char(c) => {
            form.push_char(c);
            None
        }
        _ => None,
    }
}

fn handle_host_form(
    editor: &mut EditorState,
    config: &mut Config,
    code: KeyCode,
) -> Option<String> {
    let Some(form) = editor.host_form.as_mut() else {
        editor.view = EditorView::Hosts;
        return None;
    };
    match code {
        KeyCode::Esc => {
            editor.host_form = None;
            editor.view = EditorView::Hosts;
            None
        }
        KeyCode::Tab | KeyCode::Down => {
            form.next_field();
            None
        }
        KeyCode::BackTab | KeyCode::Up => {
            form.prev_field();
            None
        }
        KeyCode::Backspace => {
            form.pop_char();
            None
        }
        KeyCode::Enter => {
            form.submitted_once = true;
            let host = match form.to_host() {
                Ok(h) => h,
                Err(err) => return Some(err),
            };
            if form.mode_create {
                if let Some(cat) = config.categories.get_mut(form.category_index) {
                    cat.hosts.push(host);
                }
            } else if let (Some(cat), Some(idx)) = (
                config.categories.get_mut(form.category_index),
                form.host_index,
            ) && let Some(slot) = cat.hosts.get_mut(idx)
            {
                *slot = host;
            }
            let created = form.mode_create;
            editor.dirty = true;
            editor.host_form = None;
            editor.view = EditorView::Hosts;
            Some(if created {
                "Host added · Ctrl+S to save".into()
            } else {
                "Host updated · Ctrl+S to save".into()
            })
        }
        KeyCode::Char(c) => {
            form.push_char(c);
            None
        }
        _ => None,
    }
}

/// Move the categories selection one step over the *visible*
/// (filtered) subset, then remap the resulting position back to the
/// original `categories_index`. Without the remap a delete/edit would
/// target the wrong row whenever a filter is active.
fn move_visible_category(editor: &mut EditorState, config: &Config, step: Step) {
    let visible = editor.visible_categories(config);
    if let Some(index) = step_visible(&visible, editor.categories_index, step) {
        editor.categories_index = index;
    }
}

/// Same as `move_visible_category` but for hosts inside the currently
/// selected category.
fn move_visible_host(editor: &mut EditorState, config: &Config, step: Step) {
    let visible = editor.visible_hosts(config);
    if let Some(index) = step_visible(&visible, editor.hosts_index, step) {
        editor.hosts_index = index;
    }
}

/// Original index of the entry one `step` away from `current` in a
/// filtered `(original index, item)` list, wrapping at both ends. `None`
/// when nothing is visible.
fn step_visible<T>(visible: &[(usize, T)], current: usize, step: Step) -> Option<usize> {
    let len = visible.len();
    if len == 0 {
        return None;
    }
    let pos = visible
        .iter()
        .position(|(index, _)| *index == current)
        .unwrap_or(0);
    let next = match step {
        Step::Back => (pos + len - 1) % len,
        Step::Forward => (pos + 1) % len,
    };
    Some(visible[next].0)
}

fn handle_category_filter_input(editor: &mut EditorState, config: &Config, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            editor.filter_focus = false;
        }
        KeyCode::Enter => {
            editor.filter_focus = false;
            if let Some((first_index, _)) = editor.visible_categories(config).first() {
                editor.categories_index = *first_index;
            }
        }
        KeyCode::Backspace => {
            if let Some(buf) = editor.category_filter.as_mut() {
                buf.pop();
            }
        }
        KeyCode::Char(c) => {
            editor
                .category_filter
                .get_or_insert_with(String::new)
                .push(c);
            // Keep the highlight on something visible while the query narrows.
            if let Some((first_index, _)) = editor.visible_categories(config).first() {
                editor.categories_index = *first_index;
            }
        }
        _ => {}
    }
}

fn handle_host_filter_input(editor: &mut EditorState, config: &Config, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            editor.filter_focus = false;
        }
        KeyCode::Enter => {
            editor.filter_focus = false;
            if let Some((first_index, _)) = editor.visible_hosts(config).first() {
                editor.hosts_index = *first_index;
            }
        }
        KeyCode::Backspace => {
            if let Some(buf) = editor.host_filter.as_mut() {
                buf.pop();
            }
        }
        KeyCode::Char(c) => {
            editor.host_filter.get_or_insert_with(String::new).push(c);
            if let Some((first_index, _)) = editor.visible_hosts(config).first() {
                editor.hosts_index = *first_index;
            }
        }
        _ => {}
    }
}

/// Apply the live sync form onto `config`. A validation error is surfaced
/// through the editor's flash line and reported as `false` so the button
/// handler does not move on to the network step.
fn sync_apply_form(editor: &mut EditorState, config: &mut Config) -> bool {
    match editor.apply_sync(config) {
        Ok(()) => true,
        Err(err) => {
            editor.flash(err);
            false
        }
    }
}

/// Probe the repo URL currently entered in the sync form.
///
/// Uses the URL from the form rather than the persisted config so the user
/// can verify it before committing the change. Blocks the UI thread — fine
/// because the libgit2 handshake is short.
fn sync_test_connection(editor: &mut EditorState) {
    let url = editor.sync_inputs[SYNC_REPO].trim().to_string();
    if url.is_empty() {
        editor.flash("Set a repo URL first");
        return;
    }
    editor.flash(format!("Reaching {} …", sync::redact_url(&url)));
    match sync::test_connection(&url) {
        Ok(()) => editor.flash("Connection OK"),
        Err(err) => editor.flash(format!("Connection failed: {err:#}")),
    }
}

fn flash(app: &mut App, msg: impl Into<String>) {
    if let Some(editor) = app.editor.as_mut() {
        editor.flash(msg);
    }
}

/// Run a sync (clone or fast-forward pull) using the persisted config.
///
/// Requires the form to have been applied first, so the new settings are
/// visible to [`SyncContext::from`]. Blocks the UI thread.
fn sync_pull_now(app: &mut App) {
    let Some(sync_cfg) = app.config.sync.as_ref() else {
        flash(app, "Apply (↩) the form before syncing");
        return;
    };
    let Some(ctx) = SyncContext::from(sync_cfg) else {
        flash(app, "Set a repo URL first");
        return;
    };
    flash(app, "Syncing …");
    run_sync(app, &ctx, "Sync", "Sync failed");
}

/// First-time bootstrap: clone the configured repo if the local copy is
/// missing (or stuck in a half-cloned state). Called right after the
/// sync form is applied so a fresh setup gets the repo on disk without
/// the user having to leave the editor.
fn sync_clone_if_missing(app: &mut App) {
    let Some(ctx) = app.config.sync.as_ref().and_then(SyncContext::from) else {
        return;
    };
    if sync::local_repo_ready(&ctx) {
        return;
    }
    flash(app, format!("Cloning into {} …", ctx.local_clone.display()));
    run_sync(app, &ctx, "Cloned", "Clone failed");
}

/// Clone or pull, load the shared inventory and report the result on the
/// editor's flash line, prefixed with `done` or `failed`.
fn run_sync(app: &mut App, ctx: &SyncContext, done: &str, failed: &str) {
    match sync::ensure_repo(ctx) {
        Ok(status) => {
            app.sync_status = status;
            let tracked = ctx.tracked_path();
            if tracked.exists()
                && let Ok(inv) = Inventory::load_from_path(&tracked)
            {
                app.config.apply_inventory(inv);
            }
            app.record_sync(sync::has_uncommitted_changes(ctx).ok());
            flash(app, format!("{done} · {}", status.label()));
        }
        Err(err) => {
            app.sync_status = SyncStatus::Failed;
            flash(app, format!("{failed}: {err:#}"));
        }
    }
}

fn save_config(app: &mut App) -> Result<()> {
    app.config.save(&app.config_path)?;
    if let Some(editor) = app.editor.as_mut() {
        editor.dirty = false;
        editor.flash("Saved");
    }
    app.set_status_success(format!("Saved to {}", app.config_path.display()));

    if let Some(sync_cfg) = app.config.sync.as_ref() {
        if sync_cfg.auto_push.unwrap_or(false) {
            if let Some(ctx) = SyncContext::from(sync_cfg) {
                let inventory = app.config.to_inventory();
                let outcome = inventory
                    .save_to_path(ctx.tracked_path())
                    .and_then(|()| sync::commit_and_push(&ctx, "chore: update hoppr inventory"));
                match outcome {
                    Ok(()) => {
                        app.set_status_success("Auto-pushed inventory upstream");
                        app.record_sync(Some(false));
                    }
                    Err(err) => app.set_status_error(format!("Auto-push failed: {err:#}")),
                }
            }
        } else {
            // No auto-push — refresh the dirty flag so the chip reflects
            // that the local clone now has uncommitted edits waiting.
            app.sync_dirty = sync::probe_dirty(app.config.sync.as_ref());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::{
        app::Mode, config::Host, favorites::FavoritesStore, history::HistoryStore,
        terminal::TerminalLauncher,
    };

    fn editing_app(dir: &TempDir) -> App {
        let config = Config {
            categories: vec![Category {
                name: "infra".into(),
                icon: None,
                hosts: vec![Host {
                    name: "gateway".into(),
                    ip: "10.0.0.1".into(),
                    ..Host::default()
                }],
            }],
            ..Config::default()
        };
        let mut app = App::new(
            config,
            dir.path().join("config.yaml"),
            SyncStatus::Disabled,
            HistoryStore::load(&dir.path().join("history.yaml")),
            FavoritesStore::load(&dir.path().join("favorites.yaml")),
            TerminalLauncher::detect(None),
        );
        app.enter_edit_mode();
        app
    }

    fn press(app: &mut App, code: KeyCode) -> Flow {
        handle_editor_key(app, code, false).expect("key handled")
    }

    fn editor(app: &App) -> &EditorState {
        app.editor.as_ref().expect("editor open")
    }

    #[test]
    fn ctrl_c_quits_from_the_editor() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        let flow = handle_editor_key(&mut app, KeyCode::Char('c'), true).expect("key handled");
        assert_eq!(flow, Flow::Quit);
    }

    #[test]
    fn esc_on_clean_menu_leaves_edit_mode() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        assert_eq!(press(&mut app, KeyCode::Esc), Flow::Continue);
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.editor.is_none());
    }

    #[test]
    fn esc_on_dirty_menu_prompts_then_saves_on_s() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        app.editor.as_mut().expect("editor open").dirty = true;

        press(&mut app, KeyCode::Esc);
        assert!(editor(&app).pending_exit);
        assert_eq!(app.mode, Mode::Edit);

        press(&mut app, KeyCode::Char('s'));
        assert_eq!(app.mode, Mode::Browse);
        assert!(app.config_path.exists());
    }

    #[test]
    fn discard_reloads_the_config_from_disk() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        app.config.save(&app.config_path).expect("save");
        app.config.categories.clear();
        let state = app.editor.as_mut().expect("editor open");
        state.dirty = true;
        state.pending_exit = true;

        press(&mut app, KeyCode::Char('d'));
        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.config.categories.len(), 1);
    }

    #[test]
    fn deleting_a_host_requires_confirmation() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(editor(&app).view, EditorView::Hosts);

        press(&mut app, KeyCode::Char('d'));
        assert!(editor(&app).pending_delete.is_some());
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(app.config.categories[0].hosts.len(), 1);

        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('y'));
        assert!(app.config.categories[0].hosts.is_empty());
        assert!(editor(&app).dirty);
        assert_eq!(editor(&app).flash.as_deref(), Some("Host removed"));
    }

    #[test]
    fn sync_toggle_flips_auto_pull_with_space() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        let state = app.editor.as_mut().expect("editor open");
        state.view = EditorView::Sync;
        state.sync_field = SYNC_AUTO_PULL;

        press(&mut app, KeyCode::Char(' '));
        assert_eq!(editor(&app).sync_inputs[SYNC_AUTO_PULL], "true");
        assert_eq!(editor(&app).flash.as_deref(), Some("auto_pull = true"));
    }

    #[test]
    fn category_form_adds_a_category() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('a'));
        for ch in "lab".chars() {
            press(&mut app, KeyCode::Char(ch));
        }
        press(&mut app, KeyCode::Enter);

        assert_eq!(app.config.categories.len(), 2);
        assert_eq!(app.config.categories[1].name, "lab");
        assert_eq!(editor(&app).view, EditorView::Categories);
    }

    #[test]
    fn save_reminder_names_the_key_that_actually_saves() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('a'));
        press(&mut app, KeyCode::Char('x'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(
            editor(&app).flash.as_deref(),
            Some("Category added · Ctrl+S to save")
        );

        press(&mut app, KeyCode::Char('s'));
        assert!(editor(&app).dirty);
        assert!(!app.config_path.exists());

        handle_editor_key(&mut app, KeyCode::Char('s'), true).expect("key handled");
        assert!(!editor(&app).dirty);
        assert!(app.config_path.exists());
    }
}
