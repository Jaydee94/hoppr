//! Key handling for the modal settings editor.
//!
//! Each editor view has its own handler working on the split borrow
//! `(&mut EditorState, &mut Config)`. Anything that needs the whole
//! [`App`] — saving, leaving edit mode, running a sync — is returned as an
//! [`Action`] and performed by [`handle_editor_key`] once the editor borrow
//! has ended.

use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers};

use crate::{
    app::App,
    config::{Category, Config, Inventory},
    editor::{
        CategoryForm, EditorState, EditorView, HostForm, MENU_ITEMS, PendingDelete, Rename,
        SYNC_AUTO_PULL, SYNC_AUTO_PUSH, SYNC_BTN_SAVE, SYNC_BTN_SYNC, SYNC_BTN_TEST,
        SYNC_ELEMENT_COUNT, SYNC_REPO, sync_field_is_bool, sync_field_is_button,
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
    TestConnection,
    PullNow,
    CloneIfMissing,
}

/// Draws one frame immediately. The sync actions block the event loop for
/// the length of a git round-trip, so a progress message set just before
/// one would be overwritten by the result without ever reaching the screen
/// unless a frame is forced in between.
pub type Redraw<'a> = dyn FnMut(&mut App) -> Result<()> + 'a;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Back,
    Forward,
}

pub fn handle_editor_key(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    redraw: &mut Redraw<'_>,
) -> Result<Flow> {
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let chord = modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
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
        handle_pending_exit(editor, code, chord)
    } else if editor.pending_delete.is_some() {
        handle_pending_delete(editor, config, code, chord);
        Action::None
    } else {
        match editor.view {
            EditorView::Menu => handle_menu(editor, list_key(code, modifiers)),
            EditorView::Categories => handle_categories(editor, config, code, modifiers),
            EditorView::Hosts => handle_hosts(editor, config, code, modifiers),
            EditorView::CategoryForm => handle_category_form(editor, config, code, modifiers),
            EditorView::HostForm => handle_host_form(editor, config, code, modifiers),
            EditorView::Defaults => handle_defaults(editor, config, code, modifiers),
            EditorView::Sync => handle_sync(editor, config, code, modifiers),
        }
    };

    perform(app, action, redraw)?;
    Ok(Flow::Continue)
}

fn perform(app: &mut App, action: Action, redraw: &mut Redraw<'_>) -> Result<()> {
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
            } else {
                // Without a file to reload, the edited config stays live, so
                // the stores have to follow its renames after all.
                migrate_renamed_entries(app);
            }
            app.exit_edit_mode();
        }
        Action::TestConnection => {
            redraw(app)?;
            sync_run_connection_test(app);
        }
        Action::PullNow => sync_pull_now(app, redraw)?,
        Action::CloneIfMissing => sync_clone_if_missing(app, redraw)?,
    }
    Ok(())
}

/// A key press as the list-style views (menu, categories, hosts) see it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKey {
    Plain(KeyCode),
    Save,
    Unbound,
}

/// The list views bind bare letters (`a`, `d`, `r`, `j`, `k`, `/`), so a
/// letter held with Ctrl or Alt must not fall through to them: Ctrl+D is
/// not "delete". Ctrl+S is the only chord they know.
fn list_key(code: KeyCode, modifiers: KeyModifiers) -> ListKey {
    let chord = modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT);
    match code {
        KeyCode::Char('s') if modifiers.contains(KeyModifiers::CONTROL) => ListKey::Save,
        KeyCode::Char(_) if chord => ListKey::Unbound,
        _ => ListKey::Plain(code),
    }
}

/// The character a key press types into a text input, if it types one.
///
/// A letter held with Ctrl or Alt is a chord, not text. Both together is how
/// crossterm reports AltGr on Windows and Linux, which many keyboard layouts
/// need for `@`, `{` or `\`, so that combination still types.
pub fn typed_char(code: KeyCode, modifiers: KeyModifiers) -> Option<char> {
    let KeyCode::Char(ch) = code else {
        return None;
    };
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    (ctrl == alt).then_some(ch)
}

/// Ctrl+S as the text views see it. With Alt held as well the key press is
/// AltGr text (see [`typed_char`]), not the save chord.
fn is_save_chord(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::Char('s')
        && modifiers.contains(KeyModifiers::CONTROL)
        && !modifiers.contains(KeyModifiers::ALT)
}

fn handle_pending_exit(editor: &mut EditorState, code: KeyCode, chord: bool) -> Action {
    if chord {
        return Action::None;
    }
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

fn handle_pending_delete(
    editor: &mut EditorState,
    config: &mut Config,
    code: KeyCode,
    chord: bool,
) {
    if chord || code != KeyCode::Char('y') {
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

fn handle_menu(editor: &mut EditorState, key: ListKey) -> Action {
    let code = match key {
        ListKey::Plain(code) => code,
        ListKey::Save if editor.dirty => return Action::Save,
        ListKey::Save | ListKey::Unbound => return Action::None,
    };
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
        _ => {}
    }
    Action::None
}

fn handle_categories(
    editor: &mut EditorState,
    config: &Config,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Action {
    if editor.filter_focus && !is_save_chord(code, modifiers) {
        handle_category_filter_input(editor, config, code, modifiers);
        return Action::None;
    }
    let code = match list_key(code, modifiers) {
        ListKey::Plain(code) => code,
        ListKey::Save if editor.dirty => return Action::Save,
        ListKey::Save | ListKey::Unbound => return Action::None,
    };
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
        _ => {}
    }
    Action::None
}

fn handle_hosts(
    editor: &mut EditorState,
    config: &Config,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Action {
    if editor.filter_focus && !is_save_chord(code, modifiers) {
        handle_host_filter_input(editor, config, code, modifiers);
        return Action::None;
    }
    let code = match list_key(code, modifiers) {
        ListKey::Plain(code) => code,
        ListKey::Save if editor.dirty => return Action::Save,
        ListKey::Save | ListKey::Unbound => return Action::None,
    };
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
        _ => {}
    }
    Action::None
}

fn handle_defaults(
    editor: &mut EditorState,
    config: &mut Config,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Action {
    if is_save_chord(code, modifiers) {
        return match editor.apply_defaults(config) {
            Ok(()) => Action::Save,
            Err(err) => {
                editor.flash(err);
                Action::None
            }
        };
    }
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
        KeyCode::Enter => match editor.apply_defaults(config) {
            Ok(()) => editor.flash("Defaults applied · Ctrl+S to save"),
            Err(err) => editor.flash(err),
        },
        _ => {
            if let Some(ch) = typed_char(code, modifiers) {
                editor.defaults_inputs[editor.defaults_field].push(ch);
            }
        }
    }
    Action::None
}

fn handle_sync(
    editor: &mut EditorState,
    config: &mut Config,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Action {
    // Same path as the [Save & push] button, so the shortcut cannot write a
    // config that is missing what the form shows.
    if is_save_chord(code, modifiers) {
        return if sync_apply_form(editor, config) {
            Action::Save
        } else {
            Action::None
        };
    }
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
        KeyCode::Enter => return activate_sync_field(editor, config),
        _ => match typed_char(code, modifiers) {
            Some(' ') if sync_field_is_bool(editor.sync_field) => toggle_sync_field(editor),
            Some(ch) if is_text_field => editor.sync_inputs[editor.sync_field].push(ch),
            _ => {}
        },
    }
    Action::None
}

fn activate_sync_field(editor: &mut EditorState, config: &mut Config) -> Action {
    match editor.sync_field {
        SYNC_BTN_TEST => return sync_test_connection(editor),
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
    modifiers: KeyModifiers,
) -> Action {
    let Some(form) = editor.category_form.as_mut() else {
        editor.view = EditorView::Categories;
        return Action::None;
    };
    let save = is_save_chord(code, modifiers);
    if save || code == KeyCode::Enter {
        let submitted = submit_category_form(editor, config);
        return finish_form(editor, submitted, save);
    }
    match code {
        KeyCode::Esc => {
            editor.category_form = None;
            editor.view = EditorView::Categories;
        }
        KeyCode::Tab | KeyCode::Down => form.next_field(),
        KeyCode::BackTab | KeyCode::Up => form.prev_field(),
        KeyCode::Backspace => form.pop_char(),
        _ => {
            if let Some(ch) = typed_char(code, modifiers) {
                form.push_char(ch);
            }
        }
    }
    Action::None
}

/// Report a form submission: a rejected form shows its validation error and
/// stays open, an accepted one is either written to disk right away (`save`)
/// or left pending with a reminder of the key that writes it.
fn finish_form(editor: &mut EditorState, submitted: Result<&str, String>, save: bool) -> Action {
    match submitted {
        Ok(_) if save => return Action::Save,
        Ok(done) => editor.flash(format!("{done} · Ctrl+S to save")),
        Err(err) => editor.flash(err),
    }
    Action::None
}

fn submit_category_form(
    editor: &mut EditorState,
    config: &mut Config,
) -> Result<&'static str, String> {
    let Some(form) = editor.category_form.as_mut() else {
        return Err("No category form is open".into());
    };
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
    form.apply(&mut cat)?;
    let created = form.mode_create;
    if created {
        config.categories.push(cat);
    } else if let Some(i) = form.category_index
        && let Some(slot) = config.categories.get_mut(i)
    {
        if slot.name != cat.name {
            editor.pending_renames.push(Rename::Category {
                from: slot.name.clone(),
                to: cat.name.clone(),
            });
        }
        *slot = cat;
    }
    editor.dirty = true;
    editor.category_form = None;
    editor.view = EditorView::Categories;
    Ok(if created {
        "Category added"
    } else {
        "Category updated"
    })
}

fn handle_host_form(
    editor: &mut EditorState,
    config: &mut Config,
    code: KeyCode,
    modifiers: KeyModifiers,
) -> Action {
    let Some(form) = editor.host_form.as_mut() else {
        editor.view = EditorView::Hosts;
        return Action::None;
    };
    let save = is_save_chord(code, modifiers);
    if save || code == KeyCode::Enter {
        let submitted = submit_host_form(editor, config);
        return finish_form(editor, submitted, save);
    }
    match code {
        KeyCode::Esc => {
            editor.host_form = None;
            editor.view = EditorView::Hosts;
        }
        KeyCode::Tab | KeyCode::Down => form.next_field(),
        KeyCode::BackTab | KeyCode::Up => form.prev_field(),
        KeyCode::Backspace => form.pop_char(),
        _ => {
            if let Some(ch) = typed_char(code, modifiers) {
                form.push_char(ch);
            }
        }
    }
    Action::None
}

fn submit_host_form(editor: &mut EditorState, config: &mut Config) -> Result<&'static str, String> {
    let Some(form) = editor.host_form.as_mut() else {
        return Err("No host form is open".into());
    };
    form.submitted_once = true;
    let host = form.to_host()?;
    let created = form.mode_create;
    if created {
        if let Some(cat) = config.categories.get_mut(form.category_index) {
            cat.hosts.push(host);
        }
    } else if let (Some(cat), Some(idx)) = (
        config.categories.get_mut(form.category_index),
        form.host_index,
    ) && let Some(slot) = cat.hosts.get_mut(idx)
    {
        if slot.name != host.name || slot.ip != host.ip {
            editor.pending_renames.push(Rename::Host {
                category: cat.name.clone(),
                from_name: slot.name.clone(),
                from_ip: slot.ip.clone(),
                to_name: host.name.clone(),
                to_ip: host.ip.clone(),
            });
        }
        *slot = host;
    }
    editor.dirty = true;
    editor.host_form = None;
    editor.view = EditorView::Hosts;
    Ok(if created {
        "Host added"
    } else {
        "Host updated"
    })
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

fn handle_category_filter_input(
    editor: &mut EditorState,
    config: &Config,
    code: KeyCode,
    modifiers: KeyModifiers,
) {
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
        _ => {
            if let Some(ch) = typed_char(code, modifiers) {
                editor
                    .category_filter
                    .get_or_insert_with(String::new)
                    .push(ch);
                // Keep the highlight on something visible while the query narrows.
                if let Some((first_index, _)) = editor.visible_categories(config).first() {
                    editor.categories_index = *first_index;
                }
            }
        }
    }
}

fn handle_host_filter_input(
    editor: &mut EditorState,
    config: &Config,
    code: KeyCode,
    modifiers: KeyModifiers,
) {
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
        _ => {
            if let Some(ch) = typed_char(code, modifiers) {
                editor.host_filter.get_or_insert_with(String::new).push(ch);
                if let Some((first_index, _)) = editor.visible_hosts(config).first() {
                    editor.hosts_index = *first_index;
                }
            }
        }
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

/// Announce a probe of the repo URL currently entered in the sync form; the
/// probe itself is [`sync_run_connection_test`].
fn sync_test_connection(editor: &mut EditorState) -> Action {
    let url = editor.sync_inputs[SYNC_REPO].trim().to_string();
    if url.is_empty() {
        editor.flash("Set a repo URL first");
        return Action::None;
    }
    editor.flash(format!("Reaching {} …", sync::redact_url(&url)));
    Action::TestConnection
}

/// Uses the URL from the form rather than the persisted config so the user
/// can verify it before committing the change. Blocks the UI thread — fine
/// because the libgit2 handshake is short.
fn sync_run_connection_test(app: &mut App) {
    let Some(editor) = app.editor.as_mut() else {
        return;
    };
    let url = editor.sync_inputs[SYNC_REPO].trim().to_string();
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
fn sync_pull_now(app: &mut App, redraw: &mut Redraw<'_>) -> Result<()> {
    let Some(sync_cfg) = app.config.sync.as_ref() else {
        flash(app, "Apply (↩) the form before syncing");
        return Ok(());
    };
    let Some(ctx) = SyncContext::from(sync_cfg) else {
        flash(app, "Set a repo URL first");
        return Ok(());
    };
    flash(app, "Syncing …");
    redraw(app)?;
    run_sync(app, &ctx, "Sync", "Sync failed");
    Ok(())
}

/// First-time bootstrap: clone the configured repo if the local copy is
/// missing (or stuck in a half-cloned state). Called right after the
/// sync form is applied so a fresh setup gets the repo on disk without
/// the user having to leave the editor.
fn sync_clone_if_missing(app: &mut App, redraw: &mut Redraw<'_>) -> Result<()> {
    let Some(ctx) = app.config.sync.as_ref().and_then(SyncContext::from) else {
        return Ok(());
    };
    if sync::local_repo_ready(&ctx) {
        return Ok(());
    }
    flash(app, format!("Cloning into {} …", ctx.local_clone.display()));
    redraw(app)?;
    run_sync(app, &ctx, "Cloned", "Clone failed");
    Ok(())
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

/// Replay the editor's pending renames onto favorites and history so a
/// renamed host keeps its star and its place under Recent.
fn migrate_renamed_entries(app: &mut App) {
    let Some(editor) = app.editor.as_mut() else {
        return;
    };
    let mut favorites_changed = false;
    let mut history_changed = false;
    for rename in std::mem::take(&mut editor.pending_renames) {
        match rename {
            Rename::Category { from, to } => {
                favorites_changed |= app.favorites.rename_category(&from, &to);
                history_changed |= app.history.rename_category(&from, &to);
            }
            Rename::Host {
                category,
                from_name,
                from_ip,
                to_name,
                to_ip,
            } => {
                if from_name != to_name {
                    favorites_changed |= app.favorites.rename_host(&category, &from_name, &to_name);
                }
                history_changed |=
                    app.history
                        .rename_host(&category, (&from_name, &from_ip), (&to_name, &to_ip));
            }
        }
    }
    if favorites_changed {
        let _ = app.favorites.save();
    }
    if history_changed {
        let _ = app.history.save();
    }
}

fn save_config(app: &mut App) -> Result<()> {
    app.config.save(&app.config_path)?;
    migrate_renamed_entries(app);
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
        app::Mode, config::Host, editor::SYNC_BRANCH, favorites::FavoritesStore,
        history::HistoryStore, terminal::TerminalLauncher,
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
        press_with(app, code, KeyModifiers::NONE)
    }

    fn press_with(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> Flow {
        handle_editor_key(app, code, modifiers, &mut |_| Ok(())).expect("key handled")
    }

    /// Presses `code` and returns the flash line as it stood at each forced
    /// redraw, i.e. what the user gets to see before the handler returns.
    fn press_recording_redraws(app: &mut App, code: KeyCode) -> Vec<String> {
        let mut frames = Vec::new();
        handle_editor_key(app, code, KeyModifiers::NONE, &mut |app| {
            frames.push(editor(app).flash.clone().unwrap_or_default());
            Ok(())
        })
        .expect("key handled");
        frames
    }

    fn open_sync_form(app: &mut App, dir: &TempDir, field: usize) {
        let unreachable_repo = dir.path().join("no-such-remote.git");
        let state = app.editor.as_mut().expect("editor open");
        state.view = EditorView::Sync;
        state.sync_inputs[SYNC_REPO] = unreachable_repo.to_string_lossy().into_owned();
        state.sync_inputs[crate::editor::SYNC_LOCAL] =
            dir.path().join("clone").to_string_lossy().into_owned();
        state.sync_field = field;
    }

    #[test]
    fn test_button_draws_its_progress_message_before_probing() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        open_sync_form(&mut app, &dir, SYNC_BTN_TEST);

        let frames = press_recording_redraws(&mut app, KeyCode::Enter);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].starts_with("Reaching "), "{frames:?}");
        let result = editor(&app).flash.clone().unwrap_or_default();
        assert!(result.starts_with("Connection failed"), "{result}");
    }

    #[test]
    fn pull_button_draws_its_progress_message_before_syncing() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        open_sync_form(&mut app, &dir, SYNC_BTN_SYNC);

        let frames = press_recording_redraws(&mut app, KeyCode::Enter);
        assert_eq!(frames, ["Syncing …"]);
        let result = editor(&app).flash.clone().unwrap_or_default();
        assert!(result.starts_with("Sync failed"), "{result}");
    }

    #[test]
    fn applying_the_sync_form_draws_the_clone_message_before_cloning() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        open_sync_form(&mut app, &dir, SYNC_REPO);

        let frames = press_recording_redraws(&mut app, KeyCode::Enter);
        assert_eq!(frames.len(), 1);
        assert!(frames[0].starts_with("Cloning into "), "{frames:?}");
        let result = editor(&app).flash.clone().unwrap_or_default();
        assert!(result.starts_with("Clone failed"), "{result}");
    }

    #[test]
    fn keys_without_blocking_work_do_not_force_a_redraw() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        assert!(press_recording_redraws(&mut app, KeyCode::Down).is_empty());
        open_sync_form(&mut app, &dir, SYNC_BTN_TEST);
        app.editor.as_mut().expect("editor open").sync_inputs[SYNC_REPO].clear();
        assert!(press_recording_redraws(&mut app, KeyCode::Enter).is_empty());
    }

    fn editor(app: &App) -> &EditorState {
        app.editor.as_ref().expect("editor open")
    }

    #[test]
    fn ctrl_c_quits_from_the_editor() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        let flow = press_with(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL);
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
    fn list_letters_ignore_ctrl_and_alt_chords() {
        let dir = TempDir::new().expect("tempdir");
        for modifiers in [
            KeyModifiers::CONTROL,
            KeyModifiers::ALT,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ] {
            for view in [EditorView::Categories, EditorView::Hosts] {
                let mut app = editing_app(&dir);
                app.editor.as_mut().expect("editor open").view = view;
                for letter in ['a', 'r', 'd', '/', 'j', 'k'] {
                    press_with(&mut app, KeyCode::Char(letter), modifiers);
                    let state = editor(&app);
                    assert_eq!(state.view, view, "{modifiers:?}+{letter}");
                    assert!(state.pending_delete.is_none(), "{modifiers:?}+{letter}");
                    assert!(!state.filter_focus, "{modifiers:?}+{letter}");
                }
            }
        }
    }

    #[test]
    fn plain_list_letters_still_act() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        app.editor.as_mut().expect("editor open").view = EditorView::Hosts;
        press(&mut app, KeyCode::Char('d'));
        assert!(editor(&app).pending_delete.is_some());
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Char('r'));
        assert_eq!(editor(&app).view, EditorView::HostForm);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('a'));
        assert_eq!(editor(&app).view, EditorView::HostForm);
    }

    #[test]
    fn prompts_ignore_ctrl_and_alt_chords() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        let state = app.editor.as_mut().expect("editor open");
        state.dirty = true;
        state.pending_exit = true;
        press_with(&mut app, KeyCode::Char('d'), KeyModifiers::CONTROL);
        press_with(&mut app, KeyCode::Char('s'), KeyModifiers::ALT);
        assert_eq!(app.mode, Mode::Edit);
        assert!(editor(&app).pending_exit);
        assert!(!app.config_path.exists());

        let state = app.editor.as_mut().expect("editor open");
        state.pending_exit = false;
        state.view = EditorView::Hosts;
        press(&mut app, KeyCode::Char('d'));
        press_with(&mut app, KeyCode::Char('y'), KeyModifiers::CONTROL);
        assert_eq!(app.config.categories[0].hosts.len(), 1);
        assert!(editor(&app).pending_delete.is_none());
    }

    fn ctrl_s(app: &mut App) {
        press_with(app, KeyCode::Char('s'), KeyModifiers::CONTROL);
    }

    fn flash_of(app: &App) -> &str {
        editor(app).flash.as_deref().unwrap_or_default()
    }

    #[test]
    fn ctrl_s_in_defaults_applies_the_typed_form_before_saving() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        let state = app.editor.as_mut().expect("editor open");
        state.view = EditorView::Defaults;
        state.defaults_field = 1;
        retype(&mut app, 2, "2200");

        ctrl_s(&mut app);
        assert_eq!(flash_of(&app), "Saved");
        assert!(!editor(&app).dirty);
        let on_disk = Config::load_from_path(&app.config_path).expect("saved config");
        assert_eq!(on_disk.defaults.port, 2200);
    }

    #[test]
    fn ctrl_s_in_defaults_reports_an_invalid_form_instead_of_saving() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        let state = app.editor.as_mut().expect("editor open");
        state.view = EditorView::Defaults;
        state.defaults_field = 1;
        retype(&mut app, 2, "ssh");

        ctrl_s(&mut app);
        assert_eq!(flash_of(&app), "Port must be a number 0-65535");
        assert!(!app.config_path.exists());
        assert_eq!(app.config.defaults.port, 22);
        assert_eq!(editor(&app).defaults_inputs[1], "ssh");
    }

    #[test]
    fn ctrl_s_in_sync_applies_the_typed_form_before_saving() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        open_sync_form(&mut app, &dir, SYNC_BRANCH);
        retype(&mut app, 0, "trunk");

        ctrl_s(&mut app);
        assert_eq!(flash_of(&app), "Saved");
        let on_disk = Config::load_from_path(&app.config_path).expect("saved config");
        let sync_cfg = on_disk.sync.expect("sync stanza saved");
        assert_eq!(sync_cfg.branch.as_deref(), Some("trunk"));
        assert_eq!(
            sync_cfg.repo.as_deref(),
            Some(editor(&app).sync_inputs[SYNC_REPO].as_str())
        );
    }

    #[test]
    fn ctrl_s_in_sync_reports_an_invalid_form_instead_of_saving() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        open_sync_form(&mut app, &dir, SYNC_BRANCH);
        app.editor.as_mut().expect("editor open").sync_inputs[SYNC_AUTO_PUSH] = "maybe".into();

        ctrl_s(&mut app);
        assert_eq!(flash_of(&app), "invalid boolean: maybe");
        assert!(!app.config_path.exists());
        assert!(app.config.sync.is_none());
    }

    #[test]
    fn ctrl_s_in_a_form_submits_it_and_saves() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('a'));
        retype(&mut app, 0, "db");
        press(&mut app, KeyCode::Tab);
        retype(&mut app, 0, "10.0.0.2");

        ctrl_s(&mut app);
        assert_eq!(editor(&app).view, EditorView::Hosts);
        assert_eq!(flash_of(&app), "Saved");
        let on_disk = Config::load_from_path(&app.config_path).expect("saved config");
        let names: Vec<_> = on_disk.categories[0]
            .hosts
            .iter()
            .map(|host| host.name.as_str())
            .collect();
        assert_eq!(names, ["gateway", "db"]);

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('a'));
        retype(&mut app, 0, "lab");
        ctrl_s(&mut app);
        assert_eq!(editor(&app).view, EditorView::Categories);
        let on_disk = Config::load_from_path(&app.config_path).expect("saved config");
        assert_eq!(on_disk.categories[1].name, "lab");
    }

    #[test]
    fn ctrl_s_in_an_invalid_form_keeps_it_open_and_saves_nothing() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('a'));
        retype(&mut app, 0, "db");

        ctrl_s(&mut app);
        assert_eq!(editor(&app).view, EditorView::HostForm);
        assert_eq!(flash_of(&app), "IP / host is required");
        assert!(!app.config_path.exists());
        assert_eq!(app.config.categories[0].hosts.len(), 1);
        let form = editor(&app).host_form.as_ref().expect("form still open");
        assert_eq!(form.fields[0], "db");
    }

    const UNBOUND_CHORDS: [(char, KeyModifiers); 4] = [
        ('s', KeyModifiers::ALT),
        ('x', KeyModifiers::CONTROL),
        ('x', KeyModifiers::ALT),
        ('X', KeyModifiers::CONTROL.union(KeyModifiers::SHIFT)),
    ];
    const ALT_GR: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);

    fn type_chords_then_altgr(app: &mut App) {
        for (letter, modifiers) in UNBOUND_CHORDS {
            press_with(app, KeyCode::Char(letter), modifiers);
        }
        press_with(app, KeyCode::Char('@'), ALT_GR);
        press_with(app, KeyCode::Char('s'), ALT_GR);
        press_with(app, KeyCode::Char('Q'), KeyModifiers::SHIFT);
    }

    #[test]
    fn text_inputs_ignore_unbound_chords_but_keep_altgr_text() {
        let dir = TempDir::new().expect("tempdir");

        let mut app = editing_app(&dir);
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('a'));
        type_chords_then_altgr(&mut app);
        let form = editor(&app).host_form.as_ref().expect("host form open");
        assert_eq!(form.fields[0], "@sQ");

        let mut app = editing_app(&dir);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('a'));
        type_chords_then_altgr(&mut app);
        let form = editor(&app)
            .category_form
            .as_ref()
            .expect("category form open");
        assert_eq!(form.fields[0], "@sQ");

        let mut app = editing_app(&dir);
        let state = app.editor.as_mut().expect("editor open");
        state.view = EditorView::Defaults;
        state.defaults_field = 2;
        type_chords_then_altgr(&mut app);
        assert_eq!(editor(&app).defaults_inputs[2], "@sQ");

        let mut app = editing_app(&dir);
        let state = app.editor.as_mut().expect("editor open");
        state.view = EditorView::Sync;
        state.sync_field = SYNC_BRANCH;
        type_chords_then_altgr(&mut app);
        assert_eq!(editor(&app).sync_inputs[SYNC_BRANCH], "@sQ");

        for view in [EditorView::Categories, EditorView::Hosts] {
            let mut app = editing_app(&dir);
            app.editor.as_mut().expect("editor open").view = view;
            press(&mut app, KeyCode::Char('/'));
            type_chords_then_altgr(&mut app);
            let state = editor(&app);
            let filter = if view == EditorView::Categories {
                &state.category_filter
            } else {
                &state.host_filter
            };
            assert_eq!(filter.as_deref(), Some("@sQ"), "{view:?}");
        }
        assert!(!dir.path().join("config.yaml").exists());
    }

    #[test]
    fn ctrl_s_saves_while_a_list_filter_is_focused() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        let state = app.editor.as_mut().expect("editor open");
        state.view = EditorView::Hosts;
        state.dirty = true;
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Char('g'));

        ctrl_s(&mut app);
        assert!(app.config_path.exists());
        assert_eq!(editor(&app).host_filter.as_deref(), Some("g"));
        assert!(editor(&app).filter_focus);
    }

    #[test]
    fn sync_toggle_ignores_chorded_space() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = editing_app(&dir);
        let state = app.editor.as_mut().expect("editor open");
        state.view = EditorView::Sync;
        state.sync_field = SYNC_AUTO_PULL;
        press_with(&mut app, KeyCode::Char(' '), KeyModifiers::CONTROL);
        assert_eq!(editor(&app).sync_inputs[SYNC_AUTO_PULL], "");
    }

    fn retype(app: &mut App, erase: usize, text: &str) {
        for _ in 0..erase {
            press(app, KeyCode::Backspace);
        }
        for ch in text.chars() {
            press(app, KeyCode::Char(ch));
        }
    }

    fn starred_app_with_history(dir: &TempDir) -> App {
        let mut app = editing_app(dir);
        app.favorites.toggle("infra", "gateway");
        app.favorites.save().expect("save favorites");
        app.history.record("gateway", "10.0.0.1", "infra");
        app.history.save().expect("save history");
        app
    }

    #[test]
    fn saved_renames_carry_star_and_history_along() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = starred_app_with_history(&dir);

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('r'));
        retype(&mut app, "gateway".len(), "edge");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.config.categories[0].hosts[0].name, "edge");
        assert!(app.favorites.is_favorite("infra", "gateway"));

        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Up);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('r'));
        retype(&mut app, "infra".len(), "core");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.config.categories[0].name, "core");

        press_with(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(editor(&app).pending_renames.is_empty());
        assert!(app.favorites.is_favorite("core", "edge"));
        assert!(!app.favorites.is_favorite("infra", "gateway"));

        let favorites = FavoritesStore::load(&dir.path().join("favorites.yaml"));
        assert!(favorites.is_favorite("core", "edge"));
        let history = HistoryStore::load(&dir.path().join("history.yaml"));
        let entry = history.recent(1).next().expect("history entry");
        assert_eq!(
            (entry.category.as_str(), entry.host_name.as_str()),
            ("core", "edge")
        );
    }

    #[test]
    fn discarded_renames_leave_star_and_history_alone() {
        let dir = TempDir::new().expect("tempdir");
        let mut app = starred_app_with_history(&dir);
        app.config.save(&app.config_path).expect("save");

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('r'));
        retype(&mut app, "gateway".len(), "edge");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('d'));

        assert_eq!(app.mode, Mode::Browse);
        assert_eq!(app.config.categories[0].hosts[0].name, "gateway");
        assert!(app.favorites.is_favorite("infra", "gateway"));
        assert_eq!(
            app.history.recent(1).next().expect("entry").host_name,
            "gateway"
        );
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

        press_with(&mut app, KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert!(!editor(&app).dirty);
        assert!(app.config_path.exists());
    }
}
