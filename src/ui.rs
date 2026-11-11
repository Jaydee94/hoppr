//! Renders the hoppr TUI.
//!
//! Layout:
//!
//! ┌────────────────────────────────────────────────────┐
//! │  HOPPR   ▍▍▍   <search>                            │  header
//! ├──────────┬─────────────────────────────────────────┤
//! │ Categories│ Hosts                                  │  body
//! │           │                                        │
//! ├──────────┴─────────────────────────────────────────┤
//! │  status │ keys                                     │  footer
//! └────────────────────────────────────────────────────┘

use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::Color,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{App, FilteredHost, Focus, MessageKind, Mode, VirtualCategoryKind, relative_time},
    connect,
    editor::{
        CategoryForm, EditorState, EditorView, HostForm, MENU_ITEMS, PendingDelete, SYNC_BTN_SAVE,
        SYNC_BTN_SYNC, SYNC_BTN_TEST, SYNC_INPUT_COUNT, sync_field_is_bool,
    },
    sync::SyncStatus,
    theme::{Theme, selection_glyph},
};

/// Number of columns reserved for a category icon, separator excluded.
const ICON_SLOT_WIDTH: usize = 2;

/// Render a category icon into a fixed-width slot followed by a single
/// separating space.
///
/// Emoji width is a perennial source of TUI misalignment: some glyphs (for
/// example 🖥️, a desktop-computer codepoint plus a variation selector) are
/// measured as a single column by the Unicode width tables while many
/// terminals draw them two columns wide. Padding every icon to the same
/// measured width keeps category names aligned and stops a narrow icon from
/// gluing the following name to it (issue #52). A terminal that still draws
/// an icon wider than its declared width can bleed by at most one column —
/// that residual mismatch lives between the icon table and the terminal and
/// cannot be fully papered over here.
///
/// Returns an empty string when the category has no icon so icon-less rows
/// stay flush with the selection glyph.
fn icon_slot(icon: Option<&str>) -> String {
    match icon.map(str::trim).filter(|s| !s.is_empty()) {
        Some(icon) => {
            let pad = ICON_SLOT_WIDTH.saturating_sub(UnicodeWidthStr::width(icon));
            format!("{icon}{} ", " ".repeat(pad))
        }
        None => String::new(),
    }
}

/// A key (or key combination) and the action it triggers.
type KeyHint = (&'static str, &'static str);

/// Condition under which a browse-mode footer hint is shown.
#[derive(Debug, Clone, Copy)]
enum HintWhen {
    Always,
    Searching,
    CanOpenWindow,
}

const BROWSE_HINTS: &[(HintWhen, KeyHint)] = &[
    (HintWhen::Always, ("Tab", "Focus")),
    (HintWhen::Always, ("/", "Search")),
    (HintWhen::Searching, ("⌃A", "Search all")),
    (HintWhen::Always, ("e", "Settings")),
    (HintWhen::Always, ("f", "Star")),
    (HintWhen::Always, ("↩", "Connect")),
    (HintWhen::CanOpenWindow, ("t", "New tab")),
    (HintWhen::Always, ("?", "Help")),
    (HintWhen::Always, ("q", "Quit")),
];

const HINTS_NO_EDITOR: &[KeyHint] = &[("Esc", "Back")];
const HINTS_CONFIRM_DELETE: &[KeyHint] = &[("y", "Yes"), ("n", "No"), ("Esc", "Cancel")];
const HINTS_MENU: &[KeyHint] = &[("↑↓", "Move"), ("↩", "Select"), ("Esc", "Exit")];
const HINTS_FILTER: &[KeyHint] = &[("Esc", "Close"), ("↩", "Confirm")];
const HINTS_CATEGORIES: &[KeyHint] = &[
    ("↑↓", "Move"),
    ("↩", "Edit"),
    ("a", "Add"),
    ("d", "Delete"),
    ("/", "Filter"),
    ("⌃s", "Save"),
    ("Esc", "Back"),
];
const HINTS_HOSTS: &[KeyHint] = &[
    ("↑↓", "Move"),
    ("Tab/⇧Tab", "± category"),
    ("↩", "Edit"),
    ("a", "Add"),
    ("d", "Delete"),
    ("/", "Filter"),
    ("⌃s", "Save"),
    ("Esc", "Back"),
];
const HINTS_FORM: &[KeyHint] = &[("↑↓", "Move"), ("↩", "Confirm"), ("Esc", "Cancel")];
const HINTS_DEFAULTS: &[KeyHint] = &[
    ("↑↓", "Move"),
    ("↩", "Apply"),
    ("⌃s", "Save"),
    ("Esc", "Back"),
];
const HINTS_SYNC: &[KeyHint] = &[
    ("↑↓", "Move"),
    ("Space", "Toggle"),
    ("↩", "Apply / activate"),
    ("⌃s", "Save"),
    ("Esc", "Back"),
];

/// A titled group of bindings in the help overlay.
type HelpSection = (&'static str, &'static [KeyHint]);

const HELP_BROWSE: &[KeyHint] = &[
    ("Tab", "Toggle focus (Categories ↔ Hosts)"),
    ("/", "Search"),
    ("Ctrl+A", "Toggle global / current-category search"),
    ("Ctrl+U", "Clear search query"),
    ("Backspace", "Delete char in search"),
    ("e", "Open settings editor"),
    ("f", "Star / unstar host"),
    ("t", "Open connection in new tab"),
    ("Enter", "Connect to selected host"),
    ("↑ ↓ / j k", "Move selection"),
    ("? / F1", "Toggle this help"),
    ("q / Esc / Ctrl+C", "Quit"),
];
const HELP_SEARCH: &[KeyHint] = &[
    ("Backspace", "Delete char"),
    ("Ctrl+U", "Clear query"),
    ("Ctrl+A", "Toggle global search"),
    ("Enter", "Confirm and return"),
];
const HELP_EDITOR_LISTS: &[KeyHint] = &[
    ("↑ ↓ / j k", "Move within list"),
    ("a", "Add"),
    ("r / Enter", "Edit selected"),
    ("d", "Delete"),
    ("Ctrl+S", "Save config to disk"),
    ("Tab / Shift+Tab", "Next / previous category (Hosts view)"),
    ("Esc", "Back"),
];
const HELP_FORMS: &[KeyHint] = &[
    ("Tab / Shift+Tab", "Next / previous field"),
    ("↑ ↓", "Next / previous field"),
    ("Backspace", "Delete char"),
    ("Enter", "Confirm"),
    ("Esc", "Cancel"),
];

/// Help overlay content, one slice of sections per column (left, right).
const HELP_COLUMNS: [&[HelpSection]; 2] = [
    &[("Browse", HELP_BROWSE), ("Search input", HELP_SEARCH)],
    &[("Editor lists", HELP_EDITOR_LISTS), ("Forms", HELP_FORMS)],
];

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let theme = Theme::midnight();

    let area = frame.area();
    frame.render_widget(Block::default().style(theme.base()), area);

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(frame, app, &theme, outer[0]);
    draw_body(frame, app, &theme, outer[1]);
    draw_status(frame, app, &theme, outer[2]);
    draw_hints(frame, app, &theme, outer[3]);

    if app.mode == Mode::Edit {
        let overlay = centered_rect(80, 80, area);
        frame.render_widget(Clear, overlay);
        draw_editor(frame, app, &theme, overlay);
    }

    if app.show_help {
        let overlay = centered_rect(80, 80, area);
        frame.render_widget(Clear, overlay);
        draw_help(frame, &theme, overlay);
    }
}

fn draw_header(frame: &mut Frame<'_>, app: &App, theme: &Theme, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(20)])
        .split(area);

    let logo = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("hoppr", theme.title()),
            Span::raw(" "),
            Span::styled("▍", theme.accent),
            Span::styled("▍", theme.primary),
            Span::styled("▍", theme.primary_glow),
        ]),
        Line::from(Span::styled("remote shell launcher", theme.text_muted)),
    ])
    .style(theme.base())
    .block(Block::default().style(theme.base()));
    frame.render_widget(logo, split[0]);

    let search_active = app.focus == Focus::Search;
    let placeholder = if app.search_query.is_empty() && !search_active {
        Span::styled("Type / to search", theme.text_muted)
    } else {
        Span::styled(app.search_query.as_str(), theme.text)
    };
    let mut spans = vec![
        Span::styled(
            " 󰍉 ",
            if search_active {
                theme.accent
            } else {
                theme.text_dim
            },
        ),
        placeholder,
    ];
    if search_active {
        spans.push(Span::styled("▌", theme.cursor()));
    }

    let search = Paragraph::new(Line::from(spans))
        .style(theme.base())
        .block(block("Search", search_active, theme));
    frame.render_widget(search, split[1]);
}

fn draw_body(frame: &mut Frame<'_>, app: &mut App, theme: &Theme, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    let mut categories_state = app.categories_state;
    let mut hosts_state = app.hosts_state;

    if app.total_category_count() == 0 {
        let block = block("Categories", app.focus == Focus::Categories, theme);
        let hint = empty_state_paragraph("No categories yet", "Press e → Manage categories", theme)
            .block(block);
        frame.render_widget(hint, split[0]);
    } else {
        let categories = build_categories(app, theme);
        frame.render_stateful_widget(categories, split[0], &mut categories_state);
    }

    let hosts = app.filtered_hosts();
    let hosts_block = block(&hosts_title(app), app.focus == Focus::Hosts, theme);
    if hosts.is_empty() {
        let (headline, sub) = hosts_empty_state(app);
        let hint = empty_state_paragraph(&headline, sub, theme).block(hosts_block);
        frame.render_widget(hint, split[1]);
    } else {
        let list = build_hosts(app, &hosts, theme).block(hosts_block);
        frame.render_stateful_widget(list, split[1], &mut hosts_state);
    }

    app.categories_state = categories_state;
    app.hosts_state = hosts_state;
}

fn is_global_search(app: &App) -> bool {
    app.search_all && !app.search_query.trim().is_empty()
}

fn hosts_title(app: &App) -> String {
    if is_global_search(app) {
        return "Hosts · All Categories".to_string();
    }
    match app.current_virtual_category() {
        Some(VirtualCategoryKind::Recent) => "Hosts · Recent".to_string(),
        Some(VirtualCategoryKind::Starred) => "Hosts · Starred".to_string(),
        None => match app.current_category() {
            Some(cat) => format!("Hosts · {}", cat.name),
            None => "Hosts".to_string(),
        },
    }
}

/// Choose the right empty-state copy for the hosts panel based on which
/// virtual / real category is active and whether a search query is in play.
/// Returns `(headline, subtext)`.
fn hosts_empty_state(app: &App) -> (String, &'static str) {
    let query = app.search_query.trim();
    if !query.is_empty() {
        return (
            format!("No hosts match \"{query}\""),
            "Try Ctrl+A for global search.",
        );
    }

    match app.current_virtual_category() {
        Some(VirtualCategoryKind::Recent) => (
            "Recent is empty".to_string(),
            "Connect to a host to populate Recent.",
        ),
        Some(VirtualCategoryKind::Starred) => (
            "No starred hosts yet".to_string(),
            "Press f on a host to star it.",
        ),
        None => (
            "No hosts in this category".to_string(),
            "Press e → Manage hosts.",
        ),
    }
}

/// Two-line centered hint used inside the categories and hosts panels when
/// there is nothing to list. Kept in `ui.rs` so the surrounding `Block` styling
/// remains visually identical to the populated state.
fn empty_state_paragraph<'a>(headline: &'a str, sub: &'a str, theme: &Theme) -> Paragraph<'a> {
    Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(headline, Theme::bold(theme.text_dim))),
        Line::from(Span::styled(sub, theme.text_muted)),
    ])
    .alignment(Alignment::Center)
    .style(theme.base())
    .wrap(Wrap { trim: true })
}

fn build_categories<'a>(app: &'a App, theme: &Theme) -> List<'a> {
    let items: Vec<ListItem> = app
        .categories_for_display()
        .into_iter()
        .enumerate()
        .map(|(idx, cat)| {
            let selected = app.categories_state.selected() == Some(idx);
            let name_color = match (selected, cat.is_virtual) {
                (true, true) => theme.accent,
                (true, false) => theme.text,
                (false, _) => theme.text_dim,
            };

            let mut spans = vec![Span::styled(selection_glyph(selected), theme.primary)];
            let slot = icon_slot(cat.icon);
            if !slot.is_empty() {
                spans.push(Span::styled(slot, theme.text));
            }
            spans.push(Span::styled(cat.name, name_color));
            ListItem::new(Line::from(spans))
        })
        .collect();

    List::new(items)
        .block(block("Categories", app.focus == Focus::Categories, theme))
        .style(theme.base())
        .highlight_style(theme.highlight_primary())
}

fn build_hosts(app: &App, hosts: &[FilteredHost<'_>], theme: &Theme) -> List<'static> {
    let show_category = is_global_search(app) || app.current_virtual_category().is_some();

    let items: Vec<ListItem> = hosts
        .iter()
        .enumerate()
        .map(|(idx, fh)| {
            let host = fh.host;
            let selected = app.hosts_state.selected() == Some(idx);
            let star = if app.favorites.is_favorite(fh.category_name, &host.name) {
                "★ "
            } else {
                ""
            };
            let name_text = if show_category {
                format!("[{}] {star}{}", fh.category_name, host.name)
            } else {
                format!("{star}{}", host.name)
            };

            ListItem::new(Line::from(vec![
                Span::styled(selection_glyph(selected), theme.accent),
                Span::styled(name_text, theme.selectable(selected)),
                Span::raw("   "),
                Span::styled(connect::describe(&app.config, host), theme.text_muted),
            ]))
        })
        .collect();

    List::new(items)
        .style(theme.base())
        .highlight_style(theme.highlight_accent())
}

fn draw_status(frame: &mut Frame<'_>, app: &App, theme: &Theme, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(32),
            Constraint::Min(10),
            Constraint::Length(20),
        ])
        .split(area);

    let sync = Paragraph::new(sync_chip(app, theme)).style(theme.base());
    frame.render_widget(sync, split[0]);

    let middle = Paragraph::new(middle_line(app, theme)).style(theme.base());
    frame.render_widget(middle, split[1]);

    let host_count = app.filtered_hosts().len();
    let total_hosts: usize = app.config.categories.iter().map(|c| c.hosts.len()).sum();
    let stats = Paragraph::new(Line::from(vec![
        Span::styled(format!("{host_count}/{total_hosts}"), theme.text),
        Span::styled(" hosts ", theme.text_muted),
    ]))
    .alignment(Alignment::Right)
    .style(theme.base());
    frame.render_widget(stats, split[2]);
}

/// Compose the left-hand sync chip:
/// `<glyph> <state> [ · ! unpushed]` where `<state>` is either a fixed
/// label (off / failed / …) or a relative timestamp once we've synced
/// at least once in this session. The glyph is redundant with the colour
/// so colourblind users can still distinguish the states.
fn sync_chip(app: &App, theme: &Theme) -> Line<'static> {
    let status = app.sync_status;
    let (glyph, color) = match status {
        SyncStatus::Disabled | SyncStatus::Skipped => ("⊘", theme.text_muted),
        SyncStatus::Failed => ("✕", theme.error),
        SyncStatus::UpToDate | SyncStatus::Pulled => ("✓", theme.success),
        SyncStatus::PulledWithChanges => ("↻", theme.accent),
    };
    let label: Cow<'static, str> = match status {
        SyncStatus::Disabled | SyncStatus::Skipped | SyncStatus::Failed => status.label().into(),
        SyncStatus::UpToDate | SyncStatus::Pulled | SyncStatus::PulledWithChanges => {
            match app.last_sync_at {
                Some(t) => format!("synced {}", relative_time(t.elapsed())).into(),
                None => "synced".into(),
            }
        }
    };

    let mut spans = vec![
        Span::styled(format!(" {glyph} "), color),
        Span::styled(label, theme.text_dim),
    ];
    if app.sync_dirty == Some(true) {
        spans.push(Span::styled(" · ", theme.text_muted));
        spans.push(Span::styled("! unpushed", Theme::bold(theme.warning)));
    }
    Line::from(spans)
}

/// Compose the middle slot:
///   1. when a search query narrows the list, prefix with a
///      `filter: "q"` chip (or `global: "q"` when `search_all` is on);
///   2. then either the active (un-faded) status message with its
///      severity glyph + color, or the resolved connection command of
///      the currently selected host as a default.
fn middle_line(app: &App, theme: &Theme) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    let query = app.search_query.trim();
    if !query.is_empty() {
        let label = if app.search_all {
            format!("global: \"{query}\"")
        } else {
            format!("filter: \"{query}\"")
        };
        spans.push(Span::styled(label, theme.title()));
        spans.push(Span::styled("  ·  ", theme.text_muted));
    }

    if let Some(msg) = app.active_status() {
        let color = match msg.kind {
            MessageKind::Success => theme.success,
            MessageKind::Info => theme.text_dim,
            MessageKind::Error => theme.error,
        };
        spans.push(Span::styled(
            format!("{} ", msg.kind.glyph()),
            Theme::bold(color),
        ));
        spans.push(Span::styled(msg.text.clone(), color));
    } else if let Some((host, _cat)) = app.selected_host_with_category() {
        let preview = connect::describe(&app.config, host);
        spans.push(Span::styled(format!("[{}] ", host.name), theme.text_dim));
        spans.push(Span::styled(preview, theme.text_muted));
    }

    Line::from(spans)
}

fn draw_hints(frame: &mut Frame<'_>, app: &App, theme: &Theme, area: Rect) {
    let line = match app.mode {
        Mode::Browse => {
            let searching = app.focus == Focus::Search || !app.search_query.is_empty();
            let can_open_window = app.terminal.is_available();
            let visible = BROWSE_HINTS
                .iter()
                .filter(|(when, _)| match when {
                    HintWhen::Always => true,
                    HintWhen::Searching => searching,
                    HintWhen::CanOpenWindow => can_open_window,
                })
                .map(|(_, hint)| *hint);
            hint_line(visible, theme)
        }
        Mode::Edit => hint_line(editor_hints(app.editor.as_ref()).iter().copied(), theme),
    };
    let hints = Paragraph::new(line)
        .style(theme.base())
        .alignment(Alignment::Center);
    frame.render_widget(hints, area);
}

fn editor_hints(editor: Option<&EditorState>) -> &'static [KeyHint] {
    let Some(editor) = editor else {
        return HINTS_NO_EDITOR;
    };
    if editor.pending_delete.is_some() {
        return HINTS_CONFIRM_DELETE;
    }
    match editor.view {
        EditorView::Menu => HINTS_MENU,
        EditorView::Categories | EditorView::Hosts if editor.filter_focus => HINTS_FILTER,
        EditorView::Categories => HINTS_CATEGORIES,
        EditorView::Hosts => HINTS_HOSTS,
        EditorView::CategoryForm | EditorView::HostForm => HINTS_FORM,
        EditorView::Defaults => HINTS_DEFAULTS,
        EditorView::Sync => HINTS_SYNC,
    }
}

/// Footer line of `[key] label` pairs separated by dots.
fn hint_line(hints: impl Iterator<Item = KeyHint>, theme: &Theme) -> Line<'static> {
    let mut spans = Vec::new();
    for (key, label) in hints {
        if !spans.is_empty() {
            spans.push(Span::styled("  ·  ", theme.text_muted));
        }
        spans.extend([
            Span::styled("[", theme.title()),
            Span::styled(key, theme.title()),
            Span::styled("]", theme.title()),
            Span::raw(" "),
            Span::styled(label, theme.text_dim),
        ]);
    }
    Line::from(spans)
}

fn block(title: &str, active: bool, theme: &Theme) -> Block<'static> {
    let title_style = if active {
        theme.title()
    } else {
        theme.text_dim.into()
    };
    Block::default()
        .title(Span::styled(format!(" {title} "), title_style))
        .borders(Borders::ALL)
        .border_type(if active {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(theme.border_style(active))
        .style(theme.base())
}

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Thick-bordered surface shared by the settings editor, the help overlay
/// and the confirmation prompts.
fn modal_block(border: Color, theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(theme.panel_border(border))
        .style(theme.panel())
}

fn draw_help(frame: &mut Frame<'_>, theme: &Theme, area: Rect) {
    let block =
        modal_block(theme.primary, theme).title(Span::styled(" Keybindings ", theme.title()));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(split[0]);
    for (sections, column) in HELP_COLUMNS.iter().zip(columns.iter()) {
        let column_text = Paragraph::new(help_lines(sections, theme)).style(theme.panel());
        frame.render_widget(column_text, *column);
    }

    let footer = Paragraph::new(Line::from(Span::styled(
        "Press ? or Esc to close",
        theme.text_muted,
    )))
    .alignment(Alignment::Center)
    .style(theme.panel());
    frame.render_widget(footer, split[1]);
}

fn help_lines(sections: &[HelpSection], theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for (title, entries) in sections {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(*title, theme.title())));
        for (key, label) in *entries {
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{key:<18}"), theme.key()),
                Span::styled(*label, theme.text_dim),
            ]));
        }
    }
    lines
}

fn draw_editor(frame: &mut Frame<'_>, app: &App, theme: &Theme, area: Rect) {
    let Some(editor) = app.editor.as_ref() else {
        return;
    };

    let block = modal_block(theme.primary, theme).title(Span::styled(" Settings ", theme.title()));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(2)])
        .split(inner);

    match editor.view {
        EditorView::Menu => draw_menu(frame, editor, theme, split[0]),
        EditorView::Categories => draw_categories_editor(frame, app, editor, theme, split[0]),
        EditorView::Hosts => draw_hosts_editor(frame, app, editor, theme, split[0]),
        EditorView::HostForm => {
            if let Some(form) = editor.host_form.as_ref() {
                draw_host_form(frame, form, theme, split[0]);
            }
        }
        EditorView::CategoryForm => {
            if let Some(form) = editor.category_form.as_ref() {
                draw_category_form(frame, form, theme, split[0]);
            }
        }
        EditorView::Defaults => draw_defaults_editor(frame, editor, theme, split[0]),
        EditorView::Sync => draw_sync_editor(frame, editor, theme, split[0]),
    }

    let footer = Paragraph::new(editor_footer(editor, theme)).style(theme.panel());
    frame.render_widget(footer, split[1]);

    if editor.pending_exit {
        draw_pending_exit_modal(frame, theme, area);
    }
    if editor.pending_delete.is_some() {
        draw_pending_delete(frame, editor, theme, area);
    }
}

fn editor_footer<'a>(editor: &'a EditorState, theme: &Theme) -> Line<'a> {
    if editor.pending_exit {
        return Line::from(vec![
            Span::styled("● ", theme.warning),
            Span::styled("[s] Save  ·  [d] Discard  ·  [c] Cancel", theme.text_dim),
        ]);
    }
    let (marker_color, fallback) = if editor.dirty {
        (theme.warning, "Unsaved changes — press s to save")
    } else {
        (theme.success, "Saved")
    };
    Line::from(vec![
        Span::styled("● ", marker_color),
        Span::styled(editor.flash.as_deref().unwrap_or(fallback), theme.text_dim),
    ])
}

fn draw_pending_exit_modal(frame: &mut Frame<'_>, theme: &Theme, area: Rect) {
    let modal = centered_rect(40, 30, area);
    frame.render_widget(Clear, modal);
    frame.render_widget(modal_block(theme.warning, theme), modal);

    let inner = modal.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let lines = vec![
        Line::from(Span::styled("Unsaved changes", Theme::bold(theme.warning))),
        Line::from(""),
        Line::from(Span::styled("Save / Discard / Cancel", theme.text)),
        Line::from(""),
        Line::from(Span::styled(
            "[s] save · [d] discard · [c] cancel",
            theme.text_muted,
        )),
    ];
    let body = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .style(theme.panel());
    frame.render_widget(body, inner);
}

fn draw_pending_delete(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    let Some(pending) = editor.pending_delete.as_ref() else {
        return;
    };
    let prompt = match pending {
        PendingDelete::Category { name } => format!("Delete category \"{name}\"?"),
        PendingDelete::Host { host_name, .. } => format!("Delete host \"{host_name}\"?"),
    };

    let popup = centered_rect(50, 30, area);
    frame.render_widget(Clear, popup);

    let block = modal_block(theme.warning, theme)
        .title(Span::styled(" Confirm delete ", Theme::bold(theme.warning)));

    let body = Paragraph::new(vec![
        Line::from(Span::styled(prompt, Theme::bold(theme.text))),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y]", theme.title()),
            Span::styled(" Yes  ·  ", theme.text_dim),
            Span::styled("[n]", theme.title()),
            Span::styled(" No", theme.text_dim),
        ]),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .block(block)
    .style(theme.panel());
    frame.render_widget(body, popup);
}

fn draw_menu(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    let items: Vec<ListItem> = MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(i, (label, _))| {
            let selected = i == editor.menu_index;
            ListItem::new(Line::from(vec![
                Span::styled(selection_glyph(selected), theme.primary),
                Span::styled(*label, theme.selectable(selected)),
            ]))
        })
        .collect();
    let list = List::new(items).style(theme.panel());
    frame.render_widget(list, area);
}

fn draw_categories_editor(
    frame: &mut Frame<'_>,
    app: &App,
    editor: &EditorState,
    theme: &Theme,
    area: Rect,
) {
    let filter = editor.category_filter.as_deref();
    let list_area = draw_filter_row(frame, filter, editor.filter_focus, theme, area);

    let items: Vec<ListItem> = editor
        .visible_categories(&app.config)
        .into_iter()
        .map(|(i, c)| {
            let selected = i == editor.categories_index;
            ListItem::new(Line::from(vec![
                Span::styled(selection_glyph(selected), theme.primary),
                Span::raw(icon_slot(c.icon.as_deref())),
                Span::raw(c.name.as_str()),
                Span::styled(format!("  ({} hosts)", c.hosts.len()), theme.text_muted),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(list_block(" Categories ", theme))
        .style(theme.panel());
    frame.render_widget(list, list_area);
}

fn draw_hosts_editor(
    frame: &mut Frame<'_>,
    app: &App,
    editor: &EditorState,
    theme: &Theme,
    area: Rect,
) {
    let filter = editor.host_filter.as_deref();
    let list_area = draw_filter_row(frame, filter, editor.filter_focus, theme, area);

    let cat = app.config.categories.get(editor.categories_index);
    let items: Vec<ListItem> = match cat {
        Some(_) => editor
            .visible_hosts(&app.config)
            .into_iter()
            .map(|(i, host)| {
                let selected = i == editor.hosts_index;
                ListItem::new(Line::from(vec![
                    Span::styled(selection_glyph(selected), theme.accent),
                    Span::raw(format!("{} → {}", host.name, host.ip)),
                ]))
            })
            .collect(),
        None => vec![ListItem::new("(no category — add one first)")],
    };
    let title = match cat {
        Some(c) => format!(" Hosts · {} ", c.name),
        None => " Hosts ".to_string(),
    };
    let list = List::new(items)
        .block(list_block(title, theme))
        .style(theme.panel());
    frame.render_widget(list, list_area);
}

fn list_block<'a>(title: impl Into<Line<'a>>, theme: &Theme) -> Block<'a> {
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(theme.panel_border(theme.border))
        .style(theme.panel())
}

/// Draw the one-line filter row above an editor list when a filter is
/// set, and return the area left for the list (all of `area` otherwise).
fn draw_filter_row(
    frame: &mut Frame<'_>,
    filter: Option<&str>,
    focus: bool,
    theme: &Theme,
    area: Rect,
) -> Rect {
    let Some(query) = filter else {
        return area;
    };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);

    let mut spans = vec![
        Span::styled("🔍 ", theme.accent),
        Span::styled(query, theme.text),
    ];
    if focus {
        spans.push(caret(theme));
    }
    let row = Paragraph::new(Line::from(spans)).style(theme.panel());
    frame.render_widget(row, chunks[0]);
    chunks[1]
}

fn caret(theme: &Theme) -> Span<'static> {
    Span::styled("▌", theme.accent)
}

fn input_block(border: Color, theme: &Theme) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(border)
        .style(theme.panel())
}

/// One bordered `label value▌ · error` row of a form. The border turns
/// to the error color while `error` is set.
fn input_row<'a>(
    label: String,
    value: Span<'a>,
    active: bool,
    error: Option<&str>,
    theme: &Theme,
) -> Paragraph<'a> {
    let mut spans = vec![Span::styled(label, theme.text_dim), value];
    if active {
        spans.push(caret(theme));
    }
    if let Some(err) = error {
        spans.push(Span::styled(format!(" · {err}"), theme.text_muted));
    }
    Paragraph::new(Line::from(spans))
        .block(input_block(
            theme.input_border(active, error.is_some()),
            theme,
        ))
        .style(theme.panel())
}

fn draw_host_form(frame: &mut Frame<'_>, form: &HostForm, theme: &Theme, area: Rect) {
    // Args row gets a one-line hint underneath so the placeholder vocabulary
    // is discoverable without leaving the TUI.
    let constraints = HostForm::LABELS
        .iter()
        .map(|_| Constraint::Length(3))
        .chain([Constraint::Length(1)]);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    for (i, label) in HostForm::LABELS.iter().enumerate() {
        let row = input_row(
            format!(" {label:<10} "),
            Span::styled(form.fields[i].as_str(), theme.text),
            form.focused == i,
            form.field_error(i),
            theme,
        );
        frame.render_widget(row, chunks[i]);
    }
    if let Some(hint_area) = chunks.get(HostForm::LABELS.len()) {
        let hint = Paragraph::new(Line::from(Span::styled(
            HostForm::ARGS_HINT,
            theme.text_muted,
        )))
        .style(theme.panel());
        frame.render_widget(hint, *hint_area);
    }
}

fn draw_category_form(frame: &mut Frame<'_>, form: &CategoryForm, theme: &Theme, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3); CategoryForm::LABELS.len()])
        .split(area);
    for (i, label) in CategoryForm::LABELS.iter().enumerate() {
        let row = input_row(
            format!(" {label:<10} "),
            Span::styled(form.fields[i].as_str(), theme.text),
            form.focused == i,
            form.field_error(i),
            theme,
        );
        frame.render_widget(row, chunks[i]);
    }
}

fn draw_defaults_editor(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    const LABELS: [&str; 4] = [
        "Command",
        "Default port",
        "Default user",
        "Terminal command",
    ];
    const TERMINAL_COMMAND: usize = 3;

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3); LABELS.len()])
        .split(area);
    for (i, label) in LABELS.iter().enumerate() {
        // Terminal command falls back to runtime auto-detection when blank;
        // surface that contract as a muted placeholder rather than an empty cell.
        let value = editor.defaults_inputs[i].as_str();
        let value_span = if value.is_empty() && i == TERMINAL_COMMAND {
            Span::styled("auto-detected", theme.text_muted)
        } else {
            Span::styled(value, theme.text)
        };
        let row = input_row(
            format!(" {label:<16} "),
            value_span,
            editor.defaults_field == i,
            None,
            theme,
        );
        frame.render_widget(row, chunks[i]);
    }
}

fn draw_sync_editor(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    const LABELS: [&str; SYNC_INPUT_COUNT] = [
        "Repo URL",
        "Branch",
        "Path in repo",
        "Local clone",
        "Auto-pull",
        "Auto-push",
    ];
    const BUTTON_ROW_HEIGHT: u16 = 4;

    let constraints = LABELS
        .iter()
        .map(|_| Constraint::Length(3))
        .chain([Constraint::Length(BUTTON_ROW_HEIGHT), Constraint::Min(0)]);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    for (i, label) in LABELS.iter().enumerate() {
        let active = editor.sync_field == i;
        let is_bool = sync_field_is_bool(i);
        let mut spans = vec![Span::styled(format!(" {label:<14} "), theme.text_dim)];
        if is_bool {
            spans.extend(toggle_spans(&editor.sync_inputs[i], theme));
        } else {
            spans.push(Span::styled(editor.sync_inputs[i].as_str(), theme.text));
            if active {
                spans.push(caret(theme));
            }
        }
        // Boolean toggles get a thick border when focused so the
        // checkbox stands out from the text-input rows above.
        let border_type = if active && is_bool {
            BorderType::Thick
        } else {
            BorderType::Rounded
        };
        let para = Paragraph::new(Line::from(spans))
            .block(input_block(theme.input_border(active, false), theme).border_type(border_type))
            .style(theme.panel())
            .wrap(Wrap { trim: true });
        frame.render_widget(para, chunks[i]);
    }

    draw_sync_buttons(frame, editor, theme, chunks[LABELS.len()]);
}

/// Render the row of action buttons under the sync form. Each button is
/// a focusable element with its own `sync_field` index — Enter on the
/// focused button fires its action in the editor event handler.
fn draw_sync_buttons(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    let button_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(area);

    let buttons: [(usize, &str, &str, Color); 3] = [
        (SYNC_BTN_TEST, "Test", "read-only probe", theme.accent),
        (
            SYNC_BTN_SYNC,
            "Pull now",
            "apply + pull",
            theme.primary_glow,
        ),
        (
            SYNC_BTN_SAVE,
            "Save & push",
            "write + push if auto",
            theme.success,
        ),
    ];
    for (i, (field, label, caption, accent)) in buttons.iter().enumerate() {
        let active = editor.sync_field == *field;
        let fill = theme.button_fill(active);
        let border = if active { *accent } else { theme.border };
        let lines = vec![
            Line::from(Span::styled(format!(" {label} "), theme.selectable(active))),
            Line::from(Span::styled(
                format!(" {caption} "),
                fill.fg(theme.text_muted),
            )),
        ];
        let body = Paragraph::new(lines)
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(if active {
                        BorderType::Thick
                    } else {
                        BorderType::Rounded
                    })
                    .border_style(fill.fg(border))
                    .style(fill),
            )
            .style(fill);
        frame.render_widget(body, button_chunks[i]);
    }
}

/// Render a boolean field as a checkbox: `[●] On` (set), `[○] Off`
/// (explicitly off) or `[○] Off (default)` (untouched).
fn toggle_spans(value: &str, theme: &Theme) -> Vec<Span<'static>> {
    let (mark, label, color) = match value.trim().to_lowercase().as_str() {
        "true" | "yes" | "y" | "1" | "on" => ("[●]", "On", theme.success),
        "false" | "no" | "n" | "0" | "off" => ("[○]", "Off", theme.text_dim),
        _ => ("[○]", "Off (default)", theme.text_muted),
    };
    vec![
        Span::styled(mark, color),
        Span::raw(" "),
        Span::styled(label.to_string(), color),
        Span::styled("   space to toggle", theme.text_muted),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{Terminal, backend::TestBackend};

    use crate::{
        app::App,
        config::{Category, Config, Host},
        editor::{EditorView, SYNC_AUTO_PULL, SYNC_AUTO_PUSH},
        favorites::FavoritesStore,
        history::HistoryStore,
        sync::SyncStatus,
        terminal::TerminalLauncher,
    };

    #[test]
    fn icon_slot_is_empty_without_icon() {
        assert_eq!(super::icon_slot(None), "");
        assert_eq!(super::icon_slot(Some("")), "");
        assert_eq!(super::icon_slot(Some("   ")), "");
    }

    #[test]
    fn icon_slot_pads_to_a_uniform_width() {
        use unicode_width::UnicodeWidthStr;
        // Regardless of the icon's own measured width (narrow ASCII, a wide
        // emoji, or a variation-selector emoji), every populated slot occupies
        // the same number of columns so category names line up. See issue #52.
        for icon in ["A", "🚀", "🖥️", "🏠"] {
            let slot = super::icon_slot(Some(icon));
            assert_eq!(
                UnicodeWidthStr::width(slot.as_str()),
                super::ICON_SLOT_WIDTH + 1,
                "icon {icon:?} produced an off-width slot {slot:?}"
            );
        }
    }

    #[test]
    fn draw_renders_categories_and_hosts() {
        let config = Config {
            defaults: Default::default(),
            sync: None,
            categories: vec![Category {
                name: "Infrastructure".into(),
                icon: Some("🚀".into()),
                hosts: vec![Host {
                    name: "gateway".into(),
                    ip: "10.0.0.1".into(),
                    user: None,
                    port: None,
                    cmd: None,
                    command: None,
                }],
            }],
        };
        let mut app = App::new(
            config,
            PathBuf::from("/tmp/x.yaml"),
            SyncStatus::Disabled,
            HistoryStore::default(),
            FavoritesStore::default(),
            TerminalLauncher::detect(None),
        );

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| super::draw(frame, &mut app))
            .expect("ui draw");

        let buf = terminal.backend().buffer().clone();
        let rendered = buf
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("Infrastructure"));
        assert!(rendered.contains("gateway"));
    }

    #[test]
    fn draw_renders_empty_state_hints() {
        let config = Config {
            defaults: Default::default(),
            sync: None,
            categories: Vec::new(),
        };
        let mut app = App::new(
            config,
            PathBuf::from("/tmp/x.yaml"),
            SyncStatus::Disabled,
            HistoryStore::default(),
            FavoritesStore::default(),
            TerminalLauncher::detect(None),
        );

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");

        terminal
            .draw(|frame| super::draw(frame, &mut app))
            .expect("ui draw");

        let buf = terminal.backend().buffer().clone();
        let rendered = buf
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(
            rendered.contains("No categories yet"),
            "expected categories empty-state hint, got: {rendered}"
        );
    }

    fn render_sync_editor(auto_pull: &str, auto_push: &str) -> String {
        let config = Config {
            defaults: Default::default(),
            sync: None,
            categories: vec![],
        };
        let mut app = App::new(
            config,
            PathBuf::from("/tmp/x.yaml"),
            SyncStatus::Disabled,
            HistoryStore::default(),
            FavoritesStore::default(),
            TerminalLauncher::detect(None),
        );
        app.enter_edit_mode();
        let editor = app.editor.as_mut().expect("editor");
        editor.view = EditorView::Sync;
        editor.sync_inputs[SYNC_AUTO_PULL] = auto_pull.into();
        editor.sync_inputs[SYNC_AUTO_PUSH] = auto_push.into();

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::draw(frame, &mut app))
            .expect("ui draw");

        let buf = terminal.backend().buffer().clone();
        buf.content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn sync_editor_renders_on_checkbox_glyph() {
        let rendered = render_sync_editor("true", "true");
        assert!(
            rendered.contains('\u{25CF}'),
            "expected filled checkbox glyph in: {rendered}"
        );
        assert!(rendered.contains("On"));
    }

    #[test]
    fn sync_editor_renders_off_checkbox_glyph() {
        let rendered = render_sync_editor("false", "false");
        assert!(
            rendered.contains('\u{25CB}'),
            "expected empty checkbox glyph in: {rendered}"
        );
        assert!(rendered.contains("Off"));
    }

    fn render_with_sync_state(status: SyncStatus, dirty: Option<bool>) -> String {
        let config = Config {
            defaults: Default::default(),
            sync: None,
            categories: vec![],
        };
        let mut app = App::new(
            config,
            PathBuf::from("/tmp/x.yaml"),
            status,
            HistoryStore::default(),
            FavoritesStore::default(),
            TerminalLauncher::detect(None),
        );
        app.sync_dirty = dirty;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::draw(frame, &mut app))
            .expect("ui draw");

        let buf = terminal.backend().buffer().clone();
        buf.content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    #[test]
    fn sync_chip_renders_disabled_glyph() {
        let rendered = render_with_sync_state(SyncStatus::Disabled, None);
        assert!(
            rendered.contains('\u{2298}'),
            "expected ⊘ glyph for Disabled in: {rendered}"
        );
        assert!(rendered.contains("sync off"));
    }

    #[test]
    fn sync_chip_renders_skipped_glyph() {
        let rendered = render_with_sync_state(SyncStatus::Skipped, None);
        assert!(
            rendered.contains('\u{2298}'),
            "expected ⊘ glyph for Skipped in: {rendered}"
        );
        assert!(rendered.contains("sync skipped"));
    }

    #[test]
    fn sync_chip_renders_failed_glyph() {
        let rendered = render_with_sync_state(SyncStatus::Failed, None);
        assert!(
            rendered.contains('\u{2715}'),
            "expected ✕ glyph for Failed in: {rendered}"
        );
        assert!(rendered.contains("sync error"));
    }

    #[test]
    fn sync_chip_renders_up_to_date_glyph() {
        let rendered = render_with_sync_state(SyncStatus::UpToDate, None);
        assert!(
            rendered.contains('\u{2713}'),
            "expected ✓ glyph for UpToDate in: {rendered}"
        );
        assert!(rendered.contains("synced"));
    }

    #[test]
    fn sync_chip_renders_pulled_glyph() {
        let rendered = render_with_sync_state(SyncStatus::Pulled, None);
        assert!(
            rendered.contains('\u{2713}'),
            "expected ✓ glyph for Pulled in: {rendered}"
        );
        assert!(rendered.contains("synced"));
    }

    #[test]
    fn sync_chip_renders_pulled_with_changes_glyph() {
        let rendered = render_with_sync_state(SyncStatus::PulledWithChanges, None);
        assert!(
            rendered.contains('\u{21BB}'),
            "expected ↻ glyph for PulledWithChanges in: {rendered}"
        );
        assert!(rendered.contains("synced"));
    }

    #[test]
    fn sync_chip_renders_unpushed_marker() {
        let rendered = render_with_sync_state(SyncStatus::UpToDate, Some(true));
        assert!(
            rendered.contains("! unpushed"),
            "expected '! unpushed' suffix in: {rendered}"
        );
    }

    #[test]
    fn help_overlay_renders_keybindings_title_and_entries() {
        let config = Config {
            defaults: Default::default(),
            sync: None,
            categories: vec![],
        };
        let mut app = App::new(
            config,
            PathBuf::from("/tmp/x.yaml"),
            SyncStatus::Disabled,
            HistoryStore::default(),
            FavoritesStore::default(),
            TerminalLauncher::detect(None),
        );
        app.show_help = true;

        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| super::draw(frame, &mut app))
            .expect("ui draw");

        let buf = terminal.backend().buffer().clone();
        let rendered = buf
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(
            rendered.contains("Keybindings"),
            "expected 'Keybindings' title in help overlay, got: {rendered}"
        );
        assert!(
            rendered.contains("Ctrl+A"),
            "expected 'Ctrl+A' binding in help overlay, got: {rendered}"
        );
    }
}
