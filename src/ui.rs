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

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use unicode_width::UnicodeWidthStr;

use crate::{
    app::{relative_time, App, Focus, MessageKind, Mode, VirtualCategoryKind},
    connect,
    editor::{
        sync_field_is_bool, CategoryForm, EditorState, EditorView, HostForm, PendingDelete,
        MENU_ITEMS, SYNC_BTN_SAVE, SYNC_BTN_SYNC, SYNC_BTN_TEST,
    },
    sync::SyncStatus,
    theme::{Theme, ACTIVE_GLYPH, INACTIVE_GLYPH},
};

const LOGO: &str = "▄▄▄▄▄ ▄▄▄▄▄ ▄▄▄▄▄ ▄▄▄▄  ▄▄▄▄  ";

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
            Span::styled(
                "hoppr",
                Style::default()
                    .fg(theme.primary_glow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled("▍", Style::default().fg(theme.accent)),
            Span::styled("▍", Style::default().fg(theme.primary)),
            Span::styled("▍", Style::default().fg(theme.primary_glow)),
        ]),
        Line::from(Span::styled(
            "remote shell launcher",
            Style::default().fg(theme.text_muted),
        )),
    ])
    .style(theme.base())
    .block(Block::default().style(theme.base()));
    frame.render_widget(logo, split[0]);

    let search_active = app.focus == Focus::Search;
    let placeholder = if app.search_query.is_empty() && !search_active {
        Span::styled("Type / to search", Style::default().fg(theme.text_muted))
    } else {
        Span::styled(app.search_query.as_str(), Style::default().fg(theme.text))
    };
    let mut spans = vec![
        Span::styled(
            " 󰍉 ",
            Style::default().fg(if search_active {
                theme.accent
            } else {
                theme.text_dim
            }),
        ),
        placeholder,
    ];
    if search_active {
        spans.push(Span::styled(
            "▌",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::SLOW_BLINK),
        ));
    }

    let search = Paragraph::new(Line::from(spans))
        .style(theme.base())
        .block(block("Search", search_active, theme));
    frame.render_widget(search, split[1]);
    let _ = LOGO; // retained as a fallback for unicode-poor terminals
}

fn draw_body(frame: &mut Frame<'_>, app: &mut App, theme: &Theme, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    let mut categories_state = app.categories_state;
    let mut hosts_state = app.hosts_state;

    let categories_display = app.categories_for_display();
    if categories_display.is_empty() {
        let block = block("Categories", app.focus == Focus::Categories, theme);
        let hint = empty_state_paragraph("No categories yet", "Press e → Manage categories", theme)
            .block(block);
        frame.render_widget(hint, split[0]);
    } else {
        let categories = build_categories(app, theme);
        frame.render_stateful_widget(categories, split[0], &mut categories_state);
    }

    let hosts = app.filtered_hosts();
    if hosts.is_empty() {
        let (title, headline, sub) = hosts_empty_state(app);
        let hint = empty_state_paragraph(&headline, &sub, theme).block(block_owned(
            title,
            app.focus == Focus::Hosts,
            theme,
        ));
        frame.render_widget(hint, split[1]);
    } else {
        let list = build_hosts(app, theme);
        frame.render_stateful_widget(list, split[1], &mut hosts_state);
    }

    app.categories_state = categories_state;
    app.hosts_state = hosts_state;
}

/// Choose the right empty-state copy for the hosts panel based on which
/// virtual / real category is active and whether a search query is in play.
/// Returns `(title, headline, subtext)`.
fn hosts_empty_state(app: &App) -> (String, String, String) {
    let query = app.search_query.trim();
    if !query.is_empty() {
        let title = if app.search_all {
            "Hosts · All Categories".to_string()
        } else {
            match app.current_virtual_category() {
                Some(VirtualCategoryKind::Recent) => "Hosts · Recent".to_string(),
                Some(VirtualCategoryKind::Starred) => "Hosts · Starred".to_string(),
                None => match app.current_category() {
                    Some(cat) => format!("Hosts · {}", cat.name),
                    None => "Hosts".to_string(),
                },
            }
        };
        return (
            title,
            format!("No hosts match \"{query}\""),
            "Try Ctrl+A for global search.".to_string(),
        );
    }

    match app.current_virtual_category() {
        Some(VirtualCategoryKind::Recent) => (
            "Hosts · Recent".to_string(),
            "Recent is empty".to_string(),
            "Connect to a host to populate Recent.".to_string(),
        ),
        Some(VirtualCategoryKind::Starred) => (
            "Hosts · Starred".to_string(),
            "No starred hosts yet".to_string(),
            "Press f on a host to star it.".to_string(),
        ),
        None => {
            let title = match app.current_category() {
                Some(cat) => format!("Hosts · {}", cat.name),
                None => "Hosts".to_string(),
            };
            (
                title,
                "No hosts in this category".to_string(),
                "Press e → Manage hosts.".to_string(),
            )
        }
    }
}

/// Two-line centered hint used inside the categories and hosts panels when
/// there is nothing to list. Kept in `ui.rs` so the surrounding `Block` styling
/// remains visually identical to the populated state.
fn empty_state_paragraph<'a>(headline: &'a str, sub: &'a str, theme: &Theme) -> Paragraph<'a> {
    Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            headline,
            Style::default()
                .fg(theme.text_dim)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(sub, Style::default().fg(theme.text_muted))),
    ])
    .alignment(Alignment::Center)
    .style(theme.base())
    .wrap(Wrap { trim: true })
}

fn build_categories<'a>(app: &'a App, theme: &Theme) -> List<'a> {
    let cats = app.categories_for_display();
    let items: Vec<ListItem> = cats
        .into_iter()
        .enumerate()
        .map(|(idx, cat)| {
            let selected = app.categories_state.selected() == Some(idx);
            let prefix = if selected {
                ACTIVE_GLYPH
            } else {
                INACTIVE_GLYPH
            };
            let name_color = if cat.is_virtual {
                if selected {
                    theme.accent
                } else {
                    theme.text_dim
                }
            } else if selected {
                theme.text
            } else {
                theme.text_dim
            };

            let mut spans = vec![Span::styled(prefix, Style::default().fg(theme.primary))];
            let slot = icon_slot(cat.icon);
            if !slot.is_empty() {
                spans.push(Span::styled(slot, Style::default().fg(theme.text)));
            }
            spans.push(Span::styled(
                cat.name.to_owned(),
                Style::default().fg(name_color),
            ));
            ListItem::new(Line::from(spans))
        })
        .collect();

    List::new(items)
        .block(block("Categories", app.focus == Focus::Categories, theme))
        .style(theme.base())
        .highlight_style(theme.highlight_primary())
}

fn build_hosts<'a>(app: &'a App, theme: &Theme) -> List<'a> {
    let hosts = app.filtered_hosts();
    let show_category = (app.search_all && !app.search_query.trim().is_empty())
        || app.current_virtual_category().is_some();

    let items: Vec<ListItem> = hosts
        .iter()
        .enumerate()
        .map(|(idx, fh)| {
            let host = fh.host;
            let selected = app.hosts_state.selected() == Some(idx);
            let prefix = if selected {
                ACTIVE_GLYPH
            } else {
                INACTIVE_GLYPH
            };
            let descriptor = connect::describe(&app.config, host);
            let is_fav = app.favorites.is_favorite(fh.category_name, &host.name);

            let name_text = if show_category {
                format!(
                    "[{}] {}{}",
                    fh.category_name,
                    if is_fav { "★ " } else { "" },
                    host.name
                )
            } else {
                format!("{}{}", if is_fav { "★ " } else { "" }, host.name)
            };

            let name_style = Style::default()
                .fg(if selected { theme.text } else { theme.text_dim })
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                });

            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.accent)),
                Span::styled(name_text, name_style),
                Span::raw("   "),
                Span::styled(descriptor, Style::default().fg(theme.text_muted)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = if app.search_all && !app.search_query.trim().is_empty() {
        "Hosts · All Categories".to_string()
    } else {
        match app.current_virtual_category() {
            Some(VirtualCategoryKind::Recent) => "Hosts · Recent".to_string(),
            Some(VirtualCategoryKind::Starred) => "Hosts · Starred".to_string(),
            None => match app.current_category() {
                Some(cat) => format!("Hosts · {}", cat.name),
                None => "Hosts".to_string(),
            },
        }
    };

    List::new(items)
        .block(block_owned(title, app.focus == Focus::Hosts, theme))
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
        Span::styled(
            format!("{host_count}/{total_hosts}"),
            Style::default().fg(theme.text),
        ),
        Span::styled(" hosts ", Style::default().fg(theme.text_muted)),
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
    let (glyph, color, label) = match app.sync_status {
        SyncStatus::Disabled => ("⊘", theme.text_muted, "sync off".to_string()),
        SyncStatus::Skipped => ("⊘", theme.text_muted, "sync skipped".to_string()),
        SyncStatus::Failed => ("✕", theme.error, "sync error".to_string()),
        SyncStatus::UpToDate | SyncStatus::Pulled => {
            let label = match app.last_sync_at {
                Some(t) => format!("synced {}", relative_time(t.elapsed())),
                None => "synced".to_string(),
            };
            ("✓", theme.success, label)
        }
        SyncStatus::PulledWithChanges => {
            let label = match app.last_sync_at {
                Some(t) => format!("synced {}", relative_time(t.elapsed())),
                None => "synced".to_string(),
            };
            ("↻", theme.accent, label)
        }
    };

    let mut spans = vec![
        Span::styled(format!(" {glyph} "), Style::default().fg(color)),
        Span::styled(label, Style::default().fg(theme.text_dim)),
    ];
    if app.sync_dirty == Some(true) {
        spans.push(Span::styled(" · ", Style::default().fg(theme.text_muted)));
        spans.push(Span::styled(
            "! unpushed",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ));
    }
    Line::from(spans)
}

/// Compose the middle slot:
///   1. when a search query narrows the list, prefix with a
///      `filter: "q"` chip (or `global: "q"` when search_all is on);
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
        spans.push(Span::styled(
            label,
            Style::default()
                .fg(theme.primary_glow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled("  ·  ", Style::default().fg(theme.text_muted)));
    }

    if let Some(msg) = app.active_status() {
        let color = match msg.kind {
            MessageKind::Success => theme.success,
            MessageKind::Info => theme.text_dim,
            MessageKind::Warn => theme.warning,
            MessageKind::Error => theme.error,
        };
        spans.push(Span::styled(
            format!("{} ", msg.kind.glyph()),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(msg.text.clone(), Style::default().fg(color)));
    } else if let Some((host, _cat)) = app.selected_host_with_category() {
        let preview = connect::describe(&app.config, host);
        spans.push(Span::styled(
            format!("[{}] ", host.name),
            Style::default().fg(theme.text_dim),
        ));
        spans.push(Span::styled(preview, Style::default().fg(theme.text_muted)));
    }

    Line::from(spans)
}

fn draw_hints(frame: &mut Frame<'_>, app: &App, theme: &Theme, area: Rect) {
    let hints: Vec<(&str, &str)> = match app.mode {
        Mode::Browse => {
            let mut h = vec![("Tab", "Focus"), ("/", "Search")];
            if app.focus == Focus::Search || !app.search_query.is_empty() {
                h.push(("⌃A", "Search all"));
            }
            h.push(("e", "Settings"));
            h.push(("f", "Star"));
            h.push(("↩", "Connect"));
            if app.terminal.is_available() {
                h.push(("t", "New tab"));
            }
            h.push(("?", "Help"));
            h.push(("q", "Quit"));
            h
        }
        Mode::Edit => {
            if app
                .editor
                .as_ref()
                .map(|e| e.pending_delete.is_some())
                .unwrap_or(false)
            {
                vec![("y", "Yes"), ("n", "No"), ("Esc", "Cancel")]
            } else {
                match app.editor.as_ref().map(|e| e.view) {
                    Some(EditorView::Menu) => {
                        vec![("↑↓", "Move"), ("↩", "Select"), ("Esc", "Exit")]
                    }
                    Some(EditorView::Categories) => {
                        if app.editor.as_ref().map(|e| e.filter_focus).unwrap_or(false) {
                            vec![("Esc", "Close"), ("↩", "Confirm")]
                        } else {
                            vec![
                                ("↑↓", "Move"),
                                ("↩", "Edit"),
                                ("a", "Add"),
                                ("d", "Delete"),
                                ("/", "Filter"),
                                ("⌃s", "Save"),
                                ("Esc", "Back"),
                            ]
                        }
                    }
                    Some(EditorView::Hosts) => {
                        if app.editor.as_ref().map(|e| e.filter_focus).unwrap_or(false) {
                            vec![("Esc", "Close"), ("↩", "Confirm")]
                        } else {
                            vec![
                                ("↑↓", "Move"),
                                ("Tab/⇧Tab", "± category"),
                                ("↩", "Edit"),
                                ("a", "Add"),
                                ("d", "Delete"),
                                ("/", "Filter"),
                                ("⌃s", "Save"),
                                ("Esc", "Back"),
                            ]
                        }
                    }
                    Some(EditorView::CategoryForm) | Some(EditorView::HostForm) => {
                        vec![("↑↓", "Move"), ("↩", "Confirm"), ("Esc", "Cancel")]
                    }
                    Some(EditorView::Defaults) => vec![
                        ("↑↓", "Move"),
                        ("↩", "Apply"),
                        ("⌃s", "Save"),
                        ("Esc", "Back"),
                    ],
                    Some(EditorView::Sync) => vec![
                        ("↑↓", "Move"),
                        ("Space", "Toggle"),
                        ("↩", "Apply / activate"),
                        ("⌃s", "Save"),
                        ("Esc", "Back"),
                    ],
                    None => vec![("Esc", "Back")],
                }
            }
        }
    };
    let mut spans = Vec::with_capacity(hints.len() * 4);
    for (i, (key, label)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ·  ", Style::default().fg(theme.text_muted)));
        }
        spans.push(Span::styled(
            format!("[{key}]"),
            Style::default()
                .fg(theme.primary_glow)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*label, Style::default().fg(theme.text_dim)));
    }
    let line = Paragraph::new(Line::from(spans))
        .style(theme.base())
        .alignment(Alignment::Center);
    frame.render_widget(line, area);
}

fn block(title: &str, active: bool, theme: &Theme) -> Block<'static> {
    block_owned(title.to_string(), active, theme)
}

fn block_owned(title: String, active: bool, theme: &Theme) -> Block<'static> {
    let title_style = if active {
        Style::default()
            .fg(theme.primary_glow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.text_dim)
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

fn draw_help(frame: &mut Frame<'_>, theme: &Theme, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(theme.primary).bg(theme.surface))
        .style(Style::default().bg(theme.surface))
        .title(Span::styled(
            " Keybindings ",
            Style::default()
                .fg(theme.primary_glow)
                .add_modifier(Modifier::BOLD),
        ));
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

    let browse: &[(&str, &str)] = &[
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
    let editor_lists: &[(&str, &str)] = &[
        ("↑ ↓ / j k", "Move within list"),
        ("a", "Add"),
        ("r / Enter", "Edit selected"),
        ("d", "Delete"),
        ("Ctrl+S", "Save config to disk"),
        ("Tab / Shift+Tab", "Next / previous category (Hosts view)"),
        ("Esc", "Back"),
    ];
    let forms: &[(&str, &str)] = &[
        ("Tab / Shift+Tab", "Next / previous field"),
        ("↑ ↓", "Next / previous field"),
        ("Backspace", "Delete char"),
        ("Enter", "Confirm"),
        ("Esc", "Cancel"),
    ];
    let search: &[(&str, &str)] = &[
        ("Backspace", "Delete char"),
        ("Ctrl+U", "Clear query"),
        ("Ctrl+A", "Toggle global search"),
        ("Enter", "Confirm and return"),
    ];

    let mut left: Vec<Line<'static>> = Vec::new();
    push_help_section(&mut left, "Browse", browse, theme);
    left.push(Line::from(""));
    push_help_section(&mut left, "Search input", search, theme);

    let mut right: Vec<Line<'static>> = Vec::new();
    push_help_section(&mut right, "Editor lists", editor_lists, theme);
    right.push(Line::from(""));
    push_help_section(&mut right, "Forms", forms, theme);

    let left_para = Paragraph::new(left).style(Style::default().bg(theme.surface));
    frame.render_widget(left_para, columns[0]);
    let right_para = Paragraph::new(right).style(Style::default().bg(theme.surface));
    frame.render_widget(right_para, columns[1]);

    let footer = Paragraph::new(Line::from(Span::styled(
        "Press ? or Esc to close",
        Style::default().fg(theme.text_muted),
    )))
    .alignment(Alignment::Center)
    .style(Style::default().bg(theme.surface));
    frame.render_widget(footer, split[1]);
}

fn push_help_section(
    lines: &mut Vec<Line<'static>>,
    title: &str,
    entries: &[(&str, &str)],
    theme: &Theme,
) {
    lines.push(Line::from(Span::styled(
        title.to_string(),
        Style::default()
            .fg(theme.primary_glow)
            .add_modifier(Modifier::BOLD),
    )));
    for (key, label) in entries {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{key:<18}"),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(label.to_string(), Style::default().fg(theme.text_dim)),
        ]));
    }
}

fn draw_editor(frame: &mut Frame<'_>, app: &App, theme: &Theme, area: Rect) {
    let Some(editor) = app.editor.as_ref() else {
        return;
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(theme.primary).bg(theme.surface))
        .style(Style::default().bg(theme.surface))
        .title(Span::styled(
            " Settings ",
            Style::default()
                .fg(theme.primary_glow)
                .add_modifier(Modifier::BOLD),
        ));
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

    let footer_line = if editor.pending_exit {
        Line::from(vec![
            Span::styled("● ", Style::default().fg(theme.warning)),
            Span::styled(
                "[s] Save  ·  [d] Discard  ·  [c] Cancel",
                Style::default().fg(theme.text_dim),
            ),
        ])
    } else {
        let flash = editor.flash.clone().unwrap_or_else(|| {
            if editor.dirty {
                "Unsaved changes — press s to save".to_string()
            } else {
                "Saved".to_string()
            }
        });
        let footer_color = if editor.dirty {
            theme.warning
        } else {
            theme.success
        };
        Line::from(vec![
            Span::styled("● ", Style::default().fg(footer_color)),
            Span::styled(flash, Style::default().fg(theme.text_dim)),
        ])
    };
    let footer = Paragraph::new(footer_line).style(Style::default().bg(theme.surface));
    frame.render_widget(footer, split[1]);

    if editor.pending_exit {
        draw_pending_exit_modal(frame, theme, area);
    }
    if editor.pending_delete.is_some() {
        draw_pending_delete(frame, editor, theme, area);
    }
}

fn draw_pending_exit_modal(frame: &mut Frame<'_>, theme: &Theme, area: Rect) {
    let modal = centered_rect(40, 30, area);
    frame.render_widget(Clear, modal);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(theme.warning).bg(theme.surface))
        .style(Style::default().bg(theme.surface));
    frame.render_widget(block, modal);

    let inner = modal.inner(Margin {
        horizontal: 2,
        vertical: 1,
    });
    let lines = vec![
        Line::from(Span::styled(
            "Unsaved changes",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Save / Discard / Cancel",
            Style::default().fg(theme.text),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "[s] save · [d] discard · [c] cancel",
            Style::default().fg(theme.text_muted),
        )),
    ];
    let body = Paragraph::new(lines)
        .alignment(Alignment::Center)
        .style(Style::default().bg(theme.surface));
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

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(theme.warning).bg(theme.surface))
        .style(Style::default().bg(theme.surface))
        .title(Span::styled(
            " Confirm delete ",
            Style::default()
                .fg(theme.warning)
                .add_modifier(Modifier::BOLD),
        ));

    let body = Paragraph::new(vec![
        Line::from(Span::styled(
            prompt,
            Style::default().fg(theme.text).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "[y]",
                Style::default()
                    .fg(theme.primary_glow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Yes  ·  ", Style::default().fg(theme.text_dim)),
            Span::styled(
                "[n]",
                Style::default()
                    .fg(theme.primary_glow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" No", Style::default().fg(theme.text_dim)),
        ]),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .block(block)
    .style(Style::default().bg(theme.surface));
    frame.render_widget(body, popup);
}

fn draw_menu(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    let items: Vec<ListItem> = MENU_ITEMS
        .iter()
        .enumerate()
        .map(|(i, (label, _))| {
            let selected = i == editor.menu_index;
            let prefix = if selected {
                ACTIVE_GLYPH
            } else {
                INACTIVE_GLYPH
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.primary)),
                Span::styled(
                    *label,
                    Style::default()
                        .fg(if selected { theme.text } else { theme.text_dim })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ]))
        })
        .collect();
    let list = List::new(items).style(Style::default().bg(theme.surface));
    frame.render_widget(list, area);
}

fn draw_categories_editor(
    frame: &mut Frame<'_>,
    app: &App,
    editor: &EditorState,
    theme: &Theme,
    area: Rect,
) {
    let (filter_area, list_area) = split_filter_area(editor.category_filter.as_deref(), area);

    if let Some(filter) = editor.category_filter.as_deref() {
        if let Some(filter_area) = filter_area {
            draw_filter_input(frame, filter, editor.filter_focus, theme, filter_area);
        }
    }

    let visible = editor.visible_categories(&app.config);
    let items: Vec<ListItem> = visible
        .iter()
        .map(|(i, c)| {
            let selected = *i == editor.categories_index;
            let prefix = if selected {
                ACTIVE_GLYPH
            } else {
                INACTIVE_GLYPH
            };
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.primary)),
                Span::raw(icon_slot(c.icon.as_deref())),
                Span::raw(c.name.clone()),
                Span::styled(
                    format!("  ({} hosts)", c.hosts.len()),
                    Style::default().fg(theme.text_muted),
                ),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .title(" Categories ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.surface))
                .style(Style::default().bg(theme.surface)),
        )
        .style(Style::default().bg(theme.surface));
    frame.render_widget(list, list_area);
}

fn draw_hosts_editor(
    frame: &mut Frame<'_>,
    app: &App,
    editor: &EditorState,
    theme: &Theme,
    area: Rect,
) {
    let (filter_area, list_area) = split_filter_area(editor.host_filter.as_deref(), area);

    if let Some(filter) = editor.host_filter.as_deref() {
        if let Some(filter_area) = filter_area {
            draw_filter_input(frame, filter, editor.filter_focus, theme, filter_area);
        }
    }

    let cat = app.config.categories.get(editor.categories_index);
    let items: Vec<ListItem> = match cat {
        Some(_) => editor
            .visible_hosts(&app.config)
            .iter()
            .map(|(i, host)| {
                let selected = *i == editor.hosts_index;
                let prefix = if selected {
                    ACTIVE_GLYPH
                } else {
                    INACTIVE_GLYPH
                };
                ListItem::new(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(theme.accent)),
                    Span::raw(format!("{} → {}", host.name, host.ip)),
                ]))
            })
            .collect(),
        None => vec![ListItem::new("(no category — add one first)")],
    };
    let title = cat
        .map(|c| format!(" Hosts · {} ", c.name))
        .unwrap_or_else(|| " Hosts ".into());
    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border).bg(theme.surface))
                .style(Style::default().bg(theme.surface)),
        )
        .style(Style::default().bg(theme.surface));
    frame.render_widget(list, list_area);
}

/// Split the editor list area into an optional 1-line filter row and the
/// remaining list region. When no filter is active the full area is
/// returned for the list.
fn split_filter_area(filter: Option<&str>, area: Rect) -> (Option<Rect>, Rect) {
    if filter.is_none() {
        return (None, area);
    }
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(1)])
        .split(area);
    (Some(chunks[0]), chunks[1])
}

fn draw_filter_input(frame: &mut Frame<'_>, query: &str, focus: bool, theme: &Theme, area: Rect) {
    let mut spans = vec![
        Span::styled("🔍 ", Style::default().fg(theme.accent)),
        Span::styled(query.to_string(), Style::default().fg(theme.text)),
    ];
    if focus {
        spans.push(Span::styled("▌", Style::default().fg(theme.accent)));
    }
    let para = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.surface));
    frame.render_widget(para, area);
}

fn draw_host_form(frame: &mut Frame<'_>, form: &HostForm, theme: &Theme, area: Rect) {
    let mut constraints: Vec<Constraint> = HostForm::LABELS
        .iter()
        .map(|_| Constraint::Length(3))
        .collect();
    // Args row gets a one-line hint underneath so the placeholder vocabulary
    // is discoverable without leaving the TUI.
    constraints.push(Constraint::Length(1));
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);
    for (i, label) in HostForm::LABELS.iter().enumerate() {
        let active = form.focused == i;
        let error = form.field_error(i);
        let mut spans = vec![
            Span::styled(
                format!(" {label:<10} "),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(form.fields[i].clone(), Style::default().fg(theme.text)),
        ];
        if active {
            spans.push(Span::styled("▌", Style::default().fg(theme.accent)));
        }
        if let Some(err) = error {
            spans.push(Span::styled(
                format!(" · {err}"),
                Style::default().fg(theme.text_muted),
            ));
        }
        let border_color = if error.is_some() {
            theme.error
        } else if active {
            theme.primary
        } else {
            theme.border
        };
        let para = Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(theme.surface)),
            )
            .style(Style::default().bg(theme.surface));
        frame.render_widget(para, chunks[i]);
    }
    if let Some(hint_area) = chunks.get(HostForm::LABELS.len()) {
        let hint = Paragraph::new(Line::from(Span::styled(
            HostForm::ARGS_HINT,
            Style::default().fg(theme.text_muted),
        )))
        .style(Style::default().bg(theme.surface));
        frame.render_widget(hint, *hint_area);
    }
}

fn draw_category_form(frame: &mut Frame<'_>, form: &CategoryForm, theme: &Theme, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            CategoryForm::LABELS
                .iter()
                .map(|_| Constraint::Length(3))
                .collect::<Vec<_>>(),
        )
        .split(area);
    for (i, label) in CategoryForm::LABELS.iter().enumerate() {
        let active = form.focused == i;
        let error = form.field_error(i);
        let mut spans = vec![
            Span::styled(
                format!(" {label:<10} "),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(form.fields[i].clone(), Style::default().fg(theme.text)),
        ];
        if active {
            spans.push(Span::styled("▌", Style::default().fg(theme.accent)));
        }
        if let Some(err) = error {
            spans.push(Span::styled(
                format!(" · {err}"),
                Style::default().fg(theme.text_muted),
            ));
        }
        let border_color = if error.is_some() {
            theme.error
        } else if active {
            theme.primary
        } else {
            theme.border
        };
        let para = Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .border_style(Style::default().fg(border_color))
                    .style(Style::default().bg(theme.surface)),
            )
            .style(Style::default().bg(theme.surface));
        frame.render_widget(para, chunks[i]);
    }
}

fn draw_defaults_editor(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    let labels = [
        "Command",
        "Default port",
        "Default user",
        "Terminal command",
    ];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            labels
                .iter()
                .map(|_| Constraint::Length(3))
                .collect::<Vec<_>>(),
        )
        .split(area);
    for (i, label) in labels.iter().enumerate() {
        let active = editor.defaults_field == i;
        // Terminal command falls back to runtime auto-detection when blank;
        // surface that contract as a muted placeholder rather than an empty cell.
        let value = &editor.defaults_inputs[i];
        let value_span = if value.is_empty() && i == 3 {
            Span::styled("auto-detected", Style::default().fg(theme.text_muted))
        } else {
            Span::styled(value.clone(), Style::default().fg(theme.text))
        };
        let para = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {label:<16} "),
                Style::default().fg(theme.text_dim),
            ),
            value_span,
            if active {
                Span::styled("▌", Style::default().fg(theme.accent))
            } else {
                Span::raw("")
            },
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if active {
                    theme.primary
                } else {
                    theme.border
                }))
                .style(Style::default().bg(theme.surface)),
        )
        .style(Style::default().bg(theme.surface));
        frame.render_widget(para, chunks[i]);
    }
}

fn draw_sync_editor(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    let labels = [
        "Repo URL",
        "Branch",
        "Path in repo",
        "Local clone",
        "Auto-pull",
        "Auto-push",
    ];
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Repo URL
            Constraint::Length(3), // Branch
            Constraint::Length(3), // Path in repo
            Constraint::Length(3), // Local clone
            Constraint::Length(3), // Auto-pull toggle
            Constraint::Length(3), // Auto-push toggle
            Constraint::Length(4), // Action buttons row (label + caption)
            Constraint::Min(0),
        ])
        .split(area);
    for (i, label) in labels.iter().enumerate() {
        let active = editor.sync_field == i;
        let is_bool = sync_field_is_bool(i);
        let value_spans = if is_bool {
            toggle_spans(&editor.sync_inputs[i], theme)
        } else {
            vec![Span::styled(
                editor.sync_inputs[i].clone(),
                Style::default().fg(theme.text),
            )]
        };
        let mut spans = vec![Span::styled(
            format!(" {label:<14} "),
            Style::default().fg(theme.text_dim),
        )];
        spans.extend(value_spans);
        if active && !is_bool {
            spans.push(Span::styled("▌", Style::default().fg(theme.accent)));
        }
        // Boolean toggles get a thick border when focused so the
        // checkbox stands out from the text-input rows above.
        let border_type = if active && is_bool {
            BorderType::Thick
        } else {
            BorderType::Rounded
        };
        let para = Paragraph::new(Line::from(spans))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(border_type)
                    .border_style(Style::default().fg(if active {
                        theme.primary
                    } else {
                        theme.border
                    }))
                    .style(Style::default().bg(theme.surface)),
            )
            .style(Style::default().bg(theme.surface))
            .wrap(Wrap { trim: true });
        frame.render_widget(para, chunks[i]);
    }

    draw_sync_buttons(frame, editor, theme, chunks[6]);
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

    let buttons: [(usize, &str, &str, ratatui::style::Color); 3] = [
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
        let (bg, fg, border) = if active {
            (theme.surface_alt, theme.text, *accent)
        } else {
            (theme.surface, theme.text_dim, theme.border)
        };
        let mut text_style = Style::default().fg(fg);
        if active {
            text_style = text_style.add_modifier(Modifier::BOLD);
        }
        let caption_style = Style::default().fg(theme.text_muted).bg(bg);
        let lines = vec![
            Line::from(Span::styled(format!(" {label} "), text_style)),
            Line::from(Span::styled(format!(" {caption} "), caption_style)),
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
                    .border_style(Style::default().fg(border).bg(bg))
                    .style(Style::default().bg(bg)),
            )
            .style(Style::default().bg(bg));
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
        Span::styled(mark, Style::default().fg(color)),
        Span::raw(" "),
        Span::styled(label.to_string(), Style::default().fg(color)),
        Span::styled("   space to toggle", Style::default().fg(theme.text_muted)),
    ]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{backend::TestBackend, Terminal};

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
