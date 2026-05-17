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

use crate::{
    app::{App, Focus, Mode},
    connect,
    editor::{CategoryForm, EditorState, EditorView, HostForm, MENU_ITEMS},
    theme::{Theme, ACTIVE_GLYPH, INACTIVE_GLYPH},
};

const LOGO: &str = "▄▄▄▄▄ ▄▄▄▄▄ ▄▄▄▄▄ ▄▄▄▄  ▄▄▄▄  ";

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

    let mut categories_state = app.categories_state.clone();
    let mut hosts_state = app.hosts_state.clone();

    let categories = build_categories(app, theme);
    frame.render_stateful_widget(categories, split[0], &mut categories_state);

    let hosts = build_hosts(app, theme);
    frame.render_stateful_widget(hosts, split[1], &mut hosts_state);

    app.categories_state = categories_state;
    app.hosts_state = hosts_state;
}

fn build_categories<'a>(app: &'a App, theme: &Theme) -> List<'a> {
    let items: Vec<ListItem> = app
        .config
        .categories
        .iter()
        .enumerate()
        .map(|(idx, category)| {
            let selected = app.categories_state.selected() == Some(idx);
            let prefix = if selected {
                ACTIVE_GLYPH
            } else {
                INACTIVE_GLYPH
            };
            let icon = category.icon.as_deref().unwrap_or("");
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.primary)),
                Span::raw(format!("{icon} ")),
                Span::styled(
                    category.name.clone(),
                    Style::default().fg(if selected { theme.text } else { theme.text_dim }),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    List::new(items)
        .block(block("Categories", app.focus == Focus::Categories, theme))
        .style(theme.base())
        .highlight_style(theme.highlight_primary())
}

fn build_hosts<'a>(app: &'a App, theme: &Theme) -> List<'a> {
    let hosts = app.filtered_hosts();
    let items: Vec<ListItem> = hosts
        .iter()
        .enumerate()
        .map(|(idx, host)| {
            let selected = app.hosts_state.selected() == Some(idx);
            let prefix = if selected {
                ACTIVE_GLYPH
            } else {
                INACTIVE_GLYPH
            };
            let descriptor = connect::describe(&app.config, host);
            let line = Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.accent)),
                Span::styled(
                    host.name.clone(),
                    Style::default()
                        .fg(if selected { theme.text } else { theme.text_dim })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::raw("   "),
                Span::styled(descriptor, Style::default().fg(theme.text_muted)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let title = match app.current_category() {
        Some(cat) => format!("Hosts · {}", cat.name),
        None => "Hosts".to_string(),
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
            Constraint::Length(20),
            Constraint::Min(10),
            Constraint::Length(30),
        ])
        .split(area);

    let sync = Paragraph::new(Line::from(vec![
        Span::styled(" ● ", Style::default().fg(sync_color(app, theme))),
        Span::styled(app.sync_status.label(), Style::default().fg(theme.text_dim)),
    ]))
    .style(theme.base());
    frame.render_widget(sync, split[0]);

    let msg = app.status_message.clone().unwrap_or_default();
    let message = Paragraph::new(Span::styled(msg, Style::default().fg(theme.text_muted)))
        .style(theme.base());
    frame.render_widget(message, split[1]);

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

fn sync_color(app: &App, theme: &Theme) -> ratatui::style::Color {
    use crate::sync::SyncStatus::*;
    match app.sync_status {
        Disabled | Skipped => theme.text_muted,
        UpToDate | Pulled => theme.success,
        PulledWithChanges => theme.accent,
        Failed => theme.error,
    }
}

fn draw_hints(frame: &mut Frame<'_>, app: &App, theme: &Theme, area: Rect) {
    let hints = match app.mode {
        Mode::Browse => vec![
            ("Tab", "Focus"),
            ("/", "Search"),
            ("e", "Settings"),
            ("↩", "Connect"),
            ("q", "Quit"),
        ],
        Mode::Edit => vec![
            ("Esc", "Back"),
            ("↑↓", "Move"),
            ("↩", "Open"),
            ("a", "Add"),
            ("d", "Delete"),
            ("s", "Save"),
        ],
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
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("● ", Style::default().fg(footer_color)),
        Span::styled(flash, Style::default().fg(theme.text_dim)),
    ]))
    .style(Style::default().bg(theme.surface));
    frame.render_widget(footer, split[1]);
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
    let items: Vec<ListItem> = app
        .config
        .categories
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let selected = i == editor.categories_index;
            let prefix = if selected {
                ACTIVE_GLYPH
            } else {
                INACTIVE_GLYPH
            };
            let icon = c.icon.as_deref().unwrap_or("");
            ListItem::new(Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.primary)),
                Span::raw(format!("{icon} {}", c.name)),
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
    frame.render_widget(list, area);
}

fn draw_hosts_editor(
    frame: &mut Frame<'_>,
    app: &App,
    editor: &EditorState,
    theme: &Theme,
    area: Rect,
) {
    let cat = app.config.categories.get(editor.categories_index);
    let items: Vec<ListItem> = match cat {
        Some(c) => c
            .hosts
            .iter()
            .enumerate()
            .map(|(i, host)| {
                let selected = i == editor.hosts_index;
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
    frame.render_widget(list, area);
}

fn draw_host_form(frame: &mut Frame<'_>, form: &HostForm, theme: &Theme, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            HostForm::LABELS
                .iter()
                .map(|_| Constraint::Length(3))
                .collect::<Vec<_>>(),
        )
        .split(area);
    for (i, label) in HostForm::LABELS.iter().enumerate() {
        let active = form.focused == i;
        let para = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {label:<10} "),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(form.fields[i].clone(), Style::default().fg(theme.text)),
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
        let para = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {label:<10} "),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(form.fields[i].clone(), Style::default().fg(theme.text)),
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

fn draw_defaults_editor(frame: &mut Frame<'_>, editor: &EditorState, theme: &Theme, area: Rect) {
    let labels = ["Command", "Default port", "Default user"];
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
        let para = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {label:<14} "),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(
                editor.defaults_inputs[i].clone(),
                Style::default().fg(theme.text),
            ),
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
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .split(area);
    for (i, label) in labels.iter().enumerate() {
        let active = editor.sync_field == i;
        let para = Paragraph::new(Line::from(vec![
            Span::styled(
                format!(" {label:<14} "),
                Style::default().fg(theme.text_dim),
            ),
            Span::styled(
                editor.sync_inputs[i].clone(),
                Style::default().fg(theme.text),
            ),
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
        .style(Style::default().bg(theme.surface))
        .wrap(Wrap { trim: true });
        frame.render_widget(para, chunks[i]);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::{backend::TestBackend, Terminal};

    use crate::{
        app::App,
        config::{Category, Config, Host},
        sync::SyncStatus,
    };

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
        let mut app = App::new(config, PathBuf::from("/tmp/x.yaml"), SyncStatus::Disabled);

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
}
