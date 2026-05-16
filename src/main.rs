use std::{
    io::{self, Stdout},
    process::Stdio,
    time::Duration,
};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

mod app;
mod config;
mod ui;

use app::{App, Focus};
use config::Config;

fn main() -> Result<()> {
    let config = Config::load()?;
    let mut app = App::new(config);
    let mut terminal = setup_terminal()?;

    let run_result = run_app(&mut terminal, &mut app);
    let restore_result = restore_terminal(terminal);

    run_result?;
    restore_result?;
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        if !event::poll(Duration::from_millis(200))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };

        if key.kind != KeyEventKind::Press {
            continue;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
            KeyCode::Tab => app.toggle_focus(),
            KeyCode::Char('/') => app.focus_search(),
            KeyCode::Up | KeyCode::Char('k') => app.previous(),
            KeyCode::Down | KeyCode::Char('j') => app.next(),
            KeyCode::Backspace if app.focus == Focus::Search => app.pop_search_char(),
            KeyCode::Enter if app.focus == Focus::Search => app.clear_search_focus_hosts(),
            KeyCode::Char(c) if app.focus == Focus::Search => app.append_search_char(c),
            KeyCode::Enter if app.focus == Focus::Hosts => {
                ssh_handoff(terminal, app)?;
            }
            _ => {}
        }
    }
}

fn ssh_handoff(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let Some(mut command) = app.selected_host_command() else {
        return Ok(());
    };

    // Required TUI suspension ordering for safe process handoff.
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    // Restore terminal state after child process exits.
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;

    Ok(())
}
