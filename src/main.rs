use std::{
    cmp::Reverse,
    io::{self, Stdout, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{anyhow, Context, Result};
use clap::CommandFactory;
use clap_complete::generate;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};
use ratatui::{backend::CrosstermBackend, Terminal};

mod app;
mod cli;
mod config;
mod connect;
mod editor;
mod favorites;
mod history;
mod sync;
mod terminal;
mod theme;
mod ui;

use app::{App, Focus, Mode};
use cli::{
    Cli, Command as CliCommand, ConfigCmd, ConnectArgs, HistoryArgs, ListArgs, ListFormat, SyncCmd,
};
use config::{default_config_path, Category, Config, Host, Inventory};
use editor::{CategoryForm, EditorState, EditorView, HostForm, MENU_ITEMS};
use favorites::FavoritesStore;
use history::{default_history_path, HistoryStore};
use sync::{SyncContext, SyncStatus};
use terminal::TerminalLauncher;

fn main() -> Result<()> {
    let mut cli = Cli::parse_cli();
    let command = cli.command.take();
    let config_path = resolve_config_path(&cli)?;

    match command {
        Some(CliCommand::Completions { shell }) => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "hoppr", &mut io::stdout());
            Ok(())
        }
        Some(CliCommand::Config(sub)) => run_config_cmd(sub, &config_path),
        Some(CliCommand::Sync(sub)) => run_sync_cmd(sub, &cli, &config_path),
        Some(CliCommand::List(args)) => {
            let (cfg, _) = load_with_sync(&cli, &config_path)?;
            run_list(args, &cfg)
        }
        Some(CliCommand::Connect(args)) => {
            let (cfg, _) = load_with_sync(&cli, &config_path)?;
            run_connect(args, &cfg)
        }
        Some(CliCommand::History(args)) => {
            let history = HistoryStore::load(&default_history_path());
            run_history(args, &history)
        }
        Some(CliCommand::Tui) | None => {
            let (cfg, status) = load_with_sync(&cli, &config_path)?;
            run_tui(cfg, config_path, status)
        }
    }
}

fn resolve_config_path(cli: &Cli) -> Result<PathBuf> {
    let path = cli.config.clone().unwrap_or_else(default_config_path);
    Ok(path)
}

fn load_with_sync(cli: &Cli, config_path: &Path) -> Result<(Config, SyncStatus)> {
    let mut config = Config::load_or_default(config_path)?;
    let mut status = if config.sync_enabled() {
        SyncStatus::UpToDate
    } else {
        SyncStatus::Disabled
    };

    let want_sync = !cli.no_sync && (cli.sync || config.sync_enabled());
    if want_sync {
        if let Some(sync_cfg) = config.sync.as_ref() {
            if let Some(ctx) = SyncContext::from(sync_cfg) {
                match sync::ensure_repo(&ctx) {
                    Ok(s) => {
                        status = s;
                        let tracked = ctx.tracked_path();
                        if tracked.exists() {
                            match Inventory::load_from_path(&tracked) {
                                Ok(inv) => config.apply_inventory(inv),
                                Err(err) => {
                                    eprintln!("hoppr: shared inventory unreadable — {err:#}");
                                    status = SyncStatus::Failed;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        eprintln!("hoppr: sync failed — {err:#}");
                        status = SyncStatus::Failed;
                    }
                }
            }
        }
    }
    Ok((config, status))
}

fn run_tui(config: Config, config_path: PathBuf, status: SyncStatus) -> Result<()> {
    let history = HistoryStore::load(&default_history_path());
    let favorites = FavoritesStore::load(&favorites::default_favorites_path());
    let terminal_launcher = TerminalLauncher::detect(config.defaults.terminal_command.as_deref());
    let mut app = App::new(
        config,
        config_path,
        status,
        history,
        favorites,
        terminal_launcher,
    );

    let mut terminal = setup_terminal()?;
    let run_result = event_loop(&mut terminal, &mut app);
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

fn event_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
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
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match app.mode {
            Mode::Browse => match key.code {
                KeyCode::Char('c') if ctrl => return Ok(()),
                KeyCode::Esc | KeyCode::Char('q') if app.focus != Focus::Search => return Ok(()),
                KeyCode::Esc if app.focus == Focus::Search => app.clear_search_focus_hosts(),
                KeyCode::Tab => app.toggle_focus(),
                KeyCode::Char('/') => app.focus_search(),
                KeyCode::Char('e') if app.focus != Focus::Search => app.enter_edit_mode(),
                KeyCode::Up | KeyCode::Char('k') if app.focus != Focus::Search => app.previous(),
                KeyCode::Down | KeyCode::Char('j') if app.focus != Focus::Search => app.next(),
                KeyCode::Backspace if app.focus == Focus::Search => app.pop_search_char(),
                KeyCode::Enter if app.focus == Focus::Search => app.clear_search_focus_hosts(),
                // Global search toggle (Ctrl+A) — only meaningful in search mode
                KeyCode::Char('a') if ctrl => {
                    app.search_all = !app.search_all;
                    app.hosts_state.select(Some(0));
                    let label = if app.search_all {
                        "Global search: all categories"
                    } else {
                        "Search: current category"
                    };
                    app.set_status(label);
                }
                KeyCode::Char(c) if app.focus == Focus::Search => app.append_search_char(c),
                // Favorite toggle
                KeyCode::Char('f') if app.focus == Focus::Hosts => {
                    let host_cat = app
                        .selected_host_with_category()
                        .map(|(h, c)| (h.name.clone(), c.to_owned()));
                    if let Some((host_name, cat_name)) = host_cat {
                        let starred = app.favorites.toggle(&cat_name, &host_name);
                        let _ = app.favorites.save();
                        if starred {
                            app.set_status(format!("★  {host_name} starred"));
                        } else {
                            app.set_status(format!("{host_name} removed from favorites"));
                        }
                        app.ensure_valid_selection();
                    }
                }
                KeyCode::Enter if app.focus == Focus::Hosts => {
                    if shift && app.terminal.is_available() {
                        ssh_spawn(app)?;
                    } else {
                        ssh_handoff(terminal, app)?;
                    }
                }
                _ => {}
            },
            Mode::Edit => {
                if handle_editor_event(app, key.code, ctrl)? {
                    return Ok(());
                }
            }
        }
    }
}

fn ssh_handoff(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> Result<()> {
    let host_cat = app
        .selected_host_with_category()
        .map(|(h, c)| (h.clone(), c.to_owned()));
    let Some((host, category_name)) = host_cat else {
        return Ok(());
    };

    app.history.record(&host.name, &host.ip, &category_name);
    let _ = app.history.save();

    let mut command = connect::build_command(&app.config, &host);

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;

    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(())
}

/// Open the connection in a new terminal window without interrupting the TUI.
fn ssh_spawn(app: &mut App) -> Result<()> {
    let host_cat = app
        .selected_host_with_category()
        .map(|(h, c)| (h.clone(), c.to_owned()));
    let Some((host, category_name)) = host_cat else {
        return Ok(());
    };

    let command = connect::build_command(&app.config, &host);
    let program = command.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let mut argv = vec![program];
    argv.extend(args);

    match app.terminal.spawn(&argv) {
        Ok(()) => {
            app.history.record(&host.name, &host.ip, &category_name);
            let _ = app.history.save();
            app.set_status(format!("Opened {} in new window", host.name));
        }
        Err(err) => {
            app.set_status(format!("Terminal launch failed: {err:#}"));
        }
    }
    Ok(())
}

// Returns Ok(true) when the caller should quit.
fn handle_editor_event(app: &mut App, code: KeyCode, ctrl: bool) -> Result<bool> {
    if ctrl && matches!(code, KeyCode::Char('c')) {
        return Ok(true);
    }

    let editor = app.editor.as_mut().expect("editor state in edit mode");
    editor.clamp(&app.config);

    match editor.view {
        EditorView::Menu => match code {
            KeyCode::Esc => {
                if app.editor.as_ref().map(|e| e.dirty).unwrap_or(false) {
                    save_config(app)?;
                }
                app.exit_edit_mode();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                editor.menu_index = (editor.menu_index + MENU_ITEMS.len() - 1) % MENU_ITEMS.len();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                editor.menu_index = (editor.menu_index + 1) % MENU_ITEMS.len();
            }
            KeyCode::Enter => editor.view = MENU_ITEMS[editor.menu_index].1,
            KeyCode::Char('s') if editor.dirty => save_config(app)?,
            _ => {}
        },
        EditorView::Categories => match code {
            KeyCode::Esc => editor.view = EditorView::Menu,
            KeyCode::Up | KeyCode::Char('k') => {
                let len = app.config.categories.len().max(1);
                editor.categories_index = (editor.categories_index + len - 1) % len;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let len = app.config.categories.len().max(1);
                editor.categories_index = (editor.categories_index + 1) % len;
            }
            KeyCode::Char('a') => {
                editor.category_form = Some(CategoryForm::new_create());
                editor.view = EditorView::CategoryForm;
            }
            KeyCode::Char('r') | KeyCode::Enter => {
                if let Some(cat) = app.config.categories.get(editor.categories_index) {
                    editor.category_form =
                        Some(CategoryForm::new_edit(editor.categories_index, cat));
                    editor.view = EditorView::CategoryForm;
                }
            }
            KeyCode::Char('d') if !app.config.categories.is_empty() => {
                app.config.categories.remove(editor.categories_index);
                editor.dirty = true;
                editor.flash("Category removed");
            }
            KeyCode::Char('s') if editor.dirty => save_config(app)?,
            _ => {}
        },
        EditorView::Hosts => match code {
            KeyCode::Esc => editor.view = EditorView::Menu,
            KeyCode::Tab => {
                let len = app.config.categories.len();
                if len > 0 {
                    editor.categories_index = (editor.categories_index + 1) % len;
                    editor.hosts_index = 0;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                let len = app
                    .config
                    .categories
                    .get(editor.categories_index)
                    .map(|c| c.hosts.len())
                    .unwrap_or(0)
                    .max(1);
                editor.hosts_index = (editor.hosts_index + len - 1) % len;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                let len = app
                    .config
                    .categories
                    .get(editor.categories_index)
                    .map(|c| c.hosts.len())
                    .unwrap_or(0)
                    .max(1);
                editor.hosts_index = (editor.hosts_index + 1) % len;
            }
            KeyCode::Char('a') if !app.config.categories.is_empty() => {
                editor.host_form = Some(HostForm::new_create(editor.categories_index));
                editor.view = EditorView::HostForm;
            }
            KeyCode::Char('r') | KeyCode::Enter => {
                if let Some(host) = app
                    .config
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
                if let Some(cat) = app.config.categories.get_mut(editor.categories_index) {
                    if !cat.hosts.is_empty() {
                        cat.hosts.remove(editor.hosts_index);
                        editor.dirty = true;
                        editor.flash("Host removed");
                    }
                }
            }
            KeyCode::Char('s') if editor.dirty => save_config(app)?,
            _ => {}
        },
        EditorView::CategoryForm => {
            let form_event_result = handle_category_form(editor, &mut app.config, code);
            if let Some(msg) = form_event_result {
                editor.flash(msg);
            }
        }
        EditorView::HostForm => {
            let form_event_result = handle_host_form(editor, &mut app.config, code);
            if let Some(msg) = form_event_result {
                editor.flash(msg);
            }
        }
        EditorView::Defaults => match code {
            KeyCode::Esc => editor.view = EditorView::Menu,
            KeyCode::Tab | KeyCode::Down => {
                editor.defaults_field = (editor.defaults_field + 1) % 3;
            }
            KeyCode::BackTab | KeyCode::Up => {
                editor.defaults_field = (editor.defaults_field + 3 - 1) % 3;
            }
            KeyCode::Backspace => {
                editor.defaults_inputs[editor.defaults_field].pop();
            }
            KeyCode::Char('s') if ctrl => save_config(app)?,
            KeyCode::Enter => match editor.apply_defaults(&mut app.config) {
                Ok(()) => editor.flash("Defaults applied · Ctrl+s to save"),
                Err(err) => editor.flash(err),
            },
            KeyCode::Char(c) => editor.defaults_inputs[editor.defaults_field].push(c),
            _ => {}
        },
        EditorView::Sync => match code {
            KeyCode::Esc => editor.view = EditorView::Menu,
            KeyCode::Tab | KeyCode::Down => {
                editor.sync_field = (editor.sync_field + 1) % 6;
            }
            KeyCode::BackTab | KeyCode::Up => {
                editor.sync_field = (editor.sync_field + 6 - 1) % 6;
            }
            KeyCode::Backspace => {
                editor.sync_inputs[editor.sync_field].pop();
            }
            KeyCode::Char('s') if ctrl => save_config(app)?,
            KeyCode::Enter => match editor.apply_sync(&mut app.config) {
                Ok(()) => editor.flash("Sync applied · Ctrl+s to save"),
                Err(err) => editor.flash(err),
            },
            KeyCode::Char(c) => editor.sync_inputs[editor.sync_field].push(c),
            _ => {}
        },
    }
    Ok(false)
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
            } else if let Some(i) = form.category_index {
                if let Some(slot) = config.categories.get_mut(i) {
                    *slot = cat;
                }
            }
            let created = form.mode_create;
            editor.dirty = true;
            editor.category_form = None;
            editor.view = EditorView::Categories;
            Some(if created {
                "Category added · s to save".into()
            } else {
                "Category updated · s to save".into()
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
            ) {
                if let Some(slot) = cat.hosts.get_mut(idx) {
                    *slot = host;
                }
            }
            let created = form.mode_create;
            editor.dirty = true;
            editor.host_form = None;
            editor.view = EditorView::Hosts;
            Some(if created {
                "Host added · s to save".into()
            } else {
                "Host updated · s to save".into()
            })
        }
        KeyCode::Char(c) => {
            form.push_char(c);
            None
        }
        _ => None,
    }
}

fn save_config(app: &mut App) -> Result<()> {
    app.config.save(&app.config_path)?;
    if let Some(editor) = app.editor.as_mut() {
        editor.dirty = false;
        editor.flash("Saved");
    }
    app.set_status(format!("Saved to {}", app.config_path.display()));

    if let Some(sync_cfg) = app.config.sync.as_ref() {
        if sync_cfg.auto_push.unwrap_or(false) {
            if let Some(ctx) = SyncContext::from(sync_cfg) {
                let inventory = app.config.to_inventory();
                let outcome = inventory
                    .save_to_path(ctx.tracked_path())
                    .and_then(|()| sync::commit_and_push(&ctx, "chore: update hoppr inventory"));
                match outcome {
                    Ok(()) => app.set_status("Auto-pushed inventory upstream"),
                    Err(err) => app.set_status(format!("Auto-push failed: {err:#}")),
                }
            }
        }
    }
    Ok(())
}

// ─── headless subcommands ───────────────────────────────────────────────────

fn run_config_cmd(cmd: ConfigCmd, config_path: &Path) -> Result<()> {
    match cmd {
        ConfigCmd::Path => {
            println!("{}", config_path.display());
        }
        ConfigCmd::Show => {
            let cfg = Config::load_or_default(config_path)?;
            println!("{}", serde_yaml::to_string(&cfg)?);
        }
        ConfigCmd::Edit => {
            if !config_path.exists() {
                Config::default().save(config_path)?;
            }
            let editor = std::env::var("VISUAL")
                .or_else(|_| std::env::var("EDITOR"))
                .unwrap_or_else(|_| {
                    if cfg!(target_os = "windows") {
                        "notepad".into()
                    } else {
                        "vi".into()
                    }
                });
            let status = Command::new(editor).arg(config_path).status()?;
            if !status.success() {
                return Err(anyhow!("editor exited with {status}"));
            }
        }
        ConfigCmd::Init { force } => {
            if config_path.exists() && !force {
                return Err(anyhow!(
                    "config already exists at {} — pass --force to overwrite",
                    config_path.display()
                ));
            }
            let cfg = starter_config();
            cfg.save(config_path)?;
            println!("wrote starter config to {}", config_path.display());
        }
    }
    Ok(())
}

fn starter_config() -> Config {
    Config {
        defaults: config::Defaults::default(),
        sync: None,
        categories: vec![Category {
            name: "Home".into(),
            icon: Some("🏠".into()),
            hosts: vec![Host {
                name: "router".into(),
                ip: "192.168.1.1".into(),
                user: Some("admin".into()),
                port: None,
                cmd: None,
                command: None,
            }],
        }],
    }
}

fn run_sync_cmd(cmd: SyncCmd, _cli: &Cli, config_path: &Path) -> Result<()> {
    let cfg = Config::load_or_default(config_path)?;
    let sync_cfg = cfg.sync.as_ref().ok_or_else(|| {
        anyhow!(
            "sync is not configured (set sync.repo in {})",
            config_path.display()
        )
    })?;
    let ctx = SyncContext::from(sync_cfg).ok_or_else(|| anyhow!("sync.repo is missing"))?;

    match cmd {
        SyncCmd::Pull => {
            let status = sync::ensure_repo(&ctx)?;
            println!("pull: {}", status.label());
        }
        SyncCmd::Push { message } => {
            cfg.to_inventory()
                .save_to_path(ctx.tracked_path())
                .context("write inventory into tracked path")?;
            sync::commit_and_push(&ctx, &message)?;
            println!("pushed to {}@{}", ctx.repo_url, ctx.branch);
        }
        SyncCmd::Status => {
            let branch = sync::current_branch(&ctx).unwrap_or_else(|_| "?".into());
            let dirty = sync::has_uncommitted_changes(&ctx).unwrap_or(false);
            println!("repo:   {}", ctx.safe_url());
            println!("branch: {}", branch);
            println!("local:  {}", ctx.local_clone.display());
            println!("dirty:  {dirty}");
        }
    }
    Ok(())
}

fn run_list(args: ListArgs, config: &Config) -> Result<()> {
    let categories: Vec<&Category> = config
        .categories
        .iter()
        .filter(|c| match args.category.as_deref() {
            Some(filter) => c.name.to_lowercase().contains(&filter.to_lowercase()),
            None => true,
        })
        .collect();

    match args.format {
        ListFormat::Plain => {
            for cat in categories {
                for host in &cat.hosts {
                    println!("{}/{}\t{}", cat.name, host.name, host.ip);
                }
            }
        }
        ListFormat::Yaml => {
            let filtered = Config {
                defaults: config.defaults.clone(),
                sync: config.sync.clone(),
                categories: categories.into_iter().cloned().collect(),
            };
            println!("{}", serde_yaml::to_string(&filtered)?);
        }
        ListFormat::Json => {
            let filtered: Vec<_> = categories.into_iter().cloned().collect();
            let mut out = String::from("[");
            for (i, c) in filtered.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("{{\"name\":{:?},\"hosts\":[", c.name));
                for (j, h) in c.hosts.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    out.push_str(&format!("{{\"name\":{:?},\"ip\":{:?}}}", h.name, h.ip));
                }
                out.push_str("]}");
            }
            out.push(']');
            println!("{out}");
        }
        ListFormat::Table => {
            let mut writer = io::stdout().lock();
            writeln!(writer, "CATEGORY            HOST                IP")?;
            for cat in categories {
                for host in &cat.hosts {
                    writeln!(
                        writer,
                        "{:<20}{:<20}{}",
                        truncate(&cat.name, 18),
                        truncate(&host.name, 18),
                        host.ip
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn run_history(args: HistoryArgs, history: &HistoryStore) -> Result<()> {
    let entries: Vec<_> = history.recent(args.limit).collect();

    match args.format {
        ListFormat::Table => {
            let mut writer = io::stdout().lock();
            writeln!(
                writer,
                "{:<25} {:<20} {:<20} CONNECTED",
                "HOST", "IP", "CATEGORY"
            )?;
            for e in &entries {
                writeln!(
                    writer,
                    "{:<25} {:<20} {:<20} {}",
                    truncate(&e.host_name, 23),
                    truncate(&e.ip, 18),
                    truncate(&e.category, 18),
                    e.connected_at_display()
                )?;
            }
        }
        ListFormat::Plain => {
            for e in &entries {
                println!("{}/{}\t{}", e.category, e.host_name, e.ip);
            }
        }
        ListFormat::Json => {
            let mut out = String::from("[");
            for (i, e) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!(
                    "{{\"host\":{:?},\"ip\":{:?},\"category\":{:?},\"connected_at\":{:?}}}",
                    e.host_name,
                    e.ip,
                    e.category,
                    e.connected_at_display()
                ));
            }
            out.push(']');
            println!("{out}");
        }
        ListFormat::Yaml => {
            let data: Vec<_> = entries
                .iter()
                .map(|e| {
                    let mut m = serde_yaml::Mapping::new();
                    m.insert("host".into(), e.host_name.clone().into());
                    m.insert("ip".into(), e.ip.clone().into());
                    m.insert("category".into(), e.category.clone().into());
                    m.insert("connected_at".into(), e.connected_at_display().into());
                    serde_yaml::Value::Mapping(m)
                })
                .collect();
            println!("{}", serde_yaml::to_string(&data)?);
        }
    }
    Ok(())
}

fn truncate(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let mut out: String = input.chars().take(max - 1).collect();
    out.push('…');
    out
}

fn run_connect(args: ConnectArgs, config: &Config) -> Result<()> {
    let host = find_host(config, &args.query)
        .ok_or_else(|| anyhow!("no host matched query: {}", args.query))?;
    let mut effective = host.clone();
    if let Some(user) = args.user.clone() {
        effective.user = Some(user);
    }
    if let Some(port) = args.port {
        effective.port = Some(port);
    }
    if let Some(cmd) = args.command.clone() {
        effective.command = Some(config::ConnectCommand::Program(cmd));
    }

    let command = connect::build_command(config, &effective);
    if args.dry_run {
        let program = command.get_program().to_string_lossy().into_owned();
        let args: Vec<String> = command
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        println!("{program} {}", args.join(" "));
        return Ok(());
    }
    let mut command = command;
    command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(())
}

fn find_host(config: &Config, query: &str) -> Option<Host> {
    let matcher = SkimMatcherV2::default();

    if let Some((cat_q, host_q)) = query.split_once('/') {
        return config
            .categories
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(cat_q))
            .and_then(|c| {
                c.hosts
                    .iter()
                    .find(|h| h.name.eq_ignore_ascii_case(host_q))
                    .cloned()
            });
    }

    let exact = config
        .categories
        .iter()
        .flat_map(|c| &c.hosts)
        .find(|h| h.name.eq_ignore_ascii_case(query) || h.ip.eq_ignore_ascii_case(query));
    if let Some(h) = exact {
        return Some(h.clone());
    }

    let mut scored: Vec<(i64, &Host)> = config
        .categories
        .iter()
        .flat_map(|c| &c.hosts)
        .filter_map(|h| {
            let hay = format!("{} {}", h.name, h.ip);
            matcher.fuzzy_match(&hay, query).map(|s| (s, h))
        })
        .collect();
    scored.sort_by_key(|(score, _)| Reverse(*score));
    scored.first().map(|(_, h)| (*h).clone())
}
