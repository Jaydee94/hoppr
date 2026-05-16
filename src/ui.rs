use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::app::{App, Focus};

const BG: Color = Color::Rgb(0x1e, 0x1e, 0x2e);
const BORDER_INACTIVE: Color = Color::Rgb(0x6c, 0x70, 0x86);
const ACTIVE: Color = Color::Rgb(0x89, 0xdc, 0xeb);
const SECONDARY: Color = Color::Rgb(0xcb, 0xa6, 0xf7);
const MUTED: Color = Color::Rgb(0xa6, 0xad, 0xc8);

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(20), Constraint::Min(10)])
        .split(root[0]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(root[1]);

    frame.render_widget(
        Paragraph::new("▗▖ ▗▖ ▗▄▖ ▗▄▄▖ ▗▄▄▖ ▗▄▄▖\n▐▛▚▖▐▌▐▌ ▐▌▐▌ ▐▌▐▌ ▐▌▐▌ ▐▌")
            .style(Style::default().fg(SECONDARY).bg(BG))
            .block(Block::default().style(Style::default().bg(BG))),
        header[0],
    );

    let search_border = if app.focus == Focus::Search {
        ACTIVE
    } else {
        BORDER_INACTIVE
    };

    frame.render_widget(
        Paragraph::new(app.search_query.as_str())
            .style(Style::default().fg(MUTED).bg(BG))
            .block(
                Block::default()
                    .title(" Search (/)")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(search_border).bg(BG)),
            ),
        header[1],
    );

    let category_items = app
        .config
        .categories
        .iter()
        .map(|category| {
            let label = if let Some(icon) = &category.icon {
                format!("{} {}", icon, category.name)
            } else {
                category.name.clone()
            };
            ListItem::new(label)
        })
        .collect::<Vec<_>>();

    let category_border = if app.focus == Focus::Categories {
        ACTIVE
    } else {
        BORDER_INACTIVE
    };

    let categories = List::new(category_items)
        .block(
            Block::default()
                .title(" Categories ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(category_border).bg(BG)),
        )
        .style(Style::default().fg(MUTED).bg(BG))
        .highlight_style(Style::default().fg(ACTIVE).add_modifier(Modifier::BOLD))
        .highlight_symbol("› ");
    frame.render_stateful_widget(categories, main[0], &mut app.categories_state);

    let hosts = app.filtered_hosts();
    let host_items = hosts
        .iter()
        .map(|host| {
            let user = host.user.clone().unwrap_or_else(|| "$USER".to_string());
            let port = host.port.unwrap_or(22);
            let meta = host
                .cmd
                .clone()
                .unwrap_or_else(|| format!("ssh {}@{}:{}", user, host.ip, port));
            ListItem::new(Line::from(format!("{}  [{}]", host.name, meta)))
        })
        .collect::<Vec<_>>();

    let host_border = if app.focus == Focus::Hosts {
        ACTIVE
    } else {
        BORDER_INACTIVE
    };

    let hosts_widget = List::new(host_items)
        .block(
            Block::default()
                .title(" Hosts ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(host_border).bg(BG)),
        )
        .style(Style::default().fg(MUTED).bg(BG))
        .highlight_style(Style::default().fg(SECONDARY).add_modifier(Modifier::BOLD))
        .highlight_symbol("› ");
    frame.render_stateful_widget(hosts_widget, main[1], &mut app.hosts_state);

    frame.render_widget(
        Paragraph::new("[Tab] Focus  [/] Search  [↑/k][↓/j] Move  [Enter] Connect  [q/Esc] Quit")
            .style(Style::default().fg(MUTED).bg(BG)),
        root[2],
    );
}

#[cfg(test)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use crate::{
        app::App,
        config::{Category, Config, Host},
    };

    #[test]
    fn draw_renders_categories_and_hosts() {
        let config = Config {
            categories: vec![Category {
                name: "Infrastructure".into(),
                icon: Some("🚀".into()),
                hosts: vec![Host {
                    name: "gateway".into(),
                    ip: "10.0.0.1".into(),
                    user: None,
                    port: None,
                    cmd: None,
                }],
            }],
        };
        let mut app = App::new(config);

        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("terminal should initialize");

        terminal
            .draw(|frame| super::draw(frame, &mut app))
            .expect("ui should draw");

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
