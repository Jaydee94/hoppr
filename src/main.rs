use std::{
    cmp::Reverse,
    fmt::Write as _,
    io::{self, Stdout, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, anyhow};
use clap::CommandFactory;
use clap_complete::generate;
use crossterm::{
    cursor::{SetCursorStyle, Show},
    event::{
        self, Event, KeyCode, KeyEventKind, KeyModifiers, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use ratatui::{Terminal, backend::CrosstermBackend};

mod app;
mod cli;
mod config;
mod connect;
mod editor;
mod favorites;
mod history;
mod input;
mod storage;
mod sync;
mod terminal;
mod theme;
mod ui;

use app::{App, Focus, Mode};
use cli::{
    Cli, Command as CliCommand, ConfigCmd, ConnectArgs, HistoryArgs, ListArgs, ListFormat, SyncCmd,
};
use config::{Category, Config, Host, Inventory, default_config_path};
use editor::EditorView;
use favorites::FavoritesStore;
use history::{HistoryStore, default_history_path};
use input::Flow;
use sync::{SyncContext, SyncStatus};
use terminal::TerminalLauncher;

fn main() -> Result<()> {
    let mut cli = Cli::parse_cli();
    let command = cli.command.take();
    let config_path = cli
        .config_path()
        .map_or_else(default_config_path, Path::to_path_buf);

    match command {
        Some(CliCommand::Completions { shell }) => {
            let mut cmd = Cli::command();
            generate(shell, &mut cmd, "hoppr", &mut io::stdout());
            Ok(())
        }
        Some(CliCommand::Config(sub)) => run_config_cmd(&sub, &config_path),
        Some(CliCommand::Sync(sub)) => run_sync_cmd(sub, &config_path),
        Some(CliCommand::List(args)) => {
            let (cfg, _) = load_with_sync(&cli, &config_path)?;
            run_list(&args, &cfg)
        }
        Some(CliCommand::Connect(args)) => {
            let (cfg, _) = load_with_sync(&cli, &config_path)?;
            run_connect(args, &cfg)
        }
        Some(CliCommand::History(args)) => {
            let history = HistoryStore::load(&default_history_path());
            run_history(&args, &history)
        }
        Some(CliCommand::Tui) | None => {
            let (cfg, status) = load_with_sync(&cli, &config_path)?;
            run_tui(cfg, config_path, status)
        }
    }
}

fn load_with_sync(cli: &Cli, config_path: &Path) -> Result<(Config, SyncStatus)> {
    let mut config = Config::load_or_default(config_path)?;
    let mut status = if config.sync_enabled() {
        SyncStatus::UpToDate
    } else {
        SyncStatus::Disabled
    };

    let want_sync = !cli.no_sync && (cli.sync || config.sync_enabled());
    if want_sync
        && let Some(sync_cfg) = config.sync.as_ref()
        && let Some(ctx) = SyncContext::from(sync_cfg)
    {
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
    // Stamp the freshness chip if we actually contacted the remote, and
    // probe the local clone so the "unpushed" indicator is accurate from
    // the first frame.
    if matches!(
        app.sync_status,
        SyncStatus::UpToDate | SyncStatus::Pulled | SyncStatus::PulledWithChanges
    ) {
        app.record_sync(sync::probe_dirty(app.config.sync.as_ref()));
    }

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
    // Push unconditionally — terminals that don't support the kitty keyboard
    // protocol silently ignore this sequence, so there's no downside. The
    // supports_keyboard_enhancement() query is unreliable in WSL2.
    execute!(
        stdout,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
        )
    )?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(mut terminal: Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    disable_raw_mode()?;
    // Restore the user's configured cursor shape (DECSCUSR 0). ratatui leaves
    // the cursor as a steady block on some terminals; without this the shell
    // hoppr returns to would inherit a non-blinking cursor. See issue #51.
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        SetCursorStyle::DefaultUserShape
    )?;
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

        if app.show_help {
            match key.code {
                KeyCode::Char('?' | 'q') | KeyCode::F(1) | KeyCode::Esc => {
                    app.show_help = false;
                }
                _ => {}
            }
            continue;
        }

        if matches!(key.code, KeyCode::Char('?') | KeyCode::F(1)) && help_toggle_allowed(app) {
            app.toggle_help();
            continue;
        }

        let flow = match app.mode {
            Mode::Browse => handle_browse_key(terminal, app, key.code, ctrl)?,
            Mode::Edit => input::handle_editor_key(app, key.code, ctrl)?,
        };
        if flow == Flow::Quit {
            return Ok(());
        }
    }
}

fn handle_browse_key(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    code: KeyCode,
    ctrl: bool,
) -> Result<Flow> {
    match code {
        KeyCode::Char('c') if ctrl => return Ok(Flow::Quit),
        KeyCode::Esc | KeyCode::Char('q') if app.focus != Focus::Search => return Ok(Flow::Quit),
        KeyCode::Esc | KeyCode::Enter if app.focus == Focus::Search => {
            app.clear_search_focus_hosts();
        }
        KeyCode::Tab => app.toggle_focus(),
        KeyCode::Char('/') => {
            // `/` always starts a fresh search: when re-entering from
            // another focus with a leftover query, wipe it first so
            // keystrokes don't append to stale text.
            if app.focus != Focus::Search && !app.search_query.is_empty() {
                app.clear_search_query();
            }
            app.focus_search();
        }
        KeyCode::Char('u') if ctrl && app.focus == Focus::Search => {
            app.clear_search_query();
        }
        KeyCode::Char('e') if app.focus != Focus::Search => app.enter_edit_mode(),
        KeyCode::Up | KeyCode::Char('k') if app.focus != Focus::Search => app.previous(),
        KeyCode::Down | KeyCode::Char('j') if app.focus != Focus::Search => app.next(),
        KeyCode::Backspace if app.focus == Focus::Search => app.pop_search_char(),
        KeyCode::Char('a') if ctrl => toggle_search_scope(app),
        KeyCode::Char(c) if app.focus == Focus::Search => app.append_search_char(c),
        KeyCode::Char('f') if app.focus == Focus::Hosts => toggle_favorite(app),
        KeyCode::Char('t') if app.focus == Focus::Hosts && app.terminal.is_available() => {
            ssh_spawn(app);
        }
        KeyCode::Enter if app.focus == Focus::Hosts => ssh_handoff(terminal, app)?,
        _ => {}
    }
    Ok(Flow::Continue)
}

fn toggle_search_scope(app: &mut App) {
    app.search_all = !app.search_all;
    app.hosts_state.select(Some(0));
    let label = if app.search_all {
        "Global search: all categories"
    } else {
        "Search: current category"
    };
    app.set_status(label);
}

fn toggle_favorite(app: &mut App) {
    let host_cat = app
        .selected_host_with_category()
        .map(|(h, c)| (h.name.clone(), c.to_owned()));
    let Some((host_name, cat_name)) = host_cat else {
        return;
    };
    let starred = app.favorites.toggle(&cat_name, &host_name);
    let _ = app.favorites.save();
    if starred {
        app.set_status_success(format!("★  {host_name} starred"));
    } else {
        app.set_status(format!("{host_name} removed from favorites"));
    }
    app.ensure_valid_selection();
}

/// `?` and F1 must NOT toggle the help overlay when the focused widget
/// would otherwise consume them as text input: the browse-mode search
/// box, and the editor's form/text views. We only allow it in list-style
/// contexts where these keys are otherwise unused.
fn help_toggle_allowed(app: &App) -> bool {
    match app.mode {
        Mode::Browse => app.focus != Focus::Search,
        Mode::Edit => matches!(
            app.editor.as_ref().map(|e| e.view),
            Some(EditorView::Menu | EditorView::Categories | EditorView::Hosts)
        ),
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
    let target = connection_target_label(&app.config, &host);

    disable_raw_mode()?;
    // ratatui hides the cursor while rendering; re-show it so the inherited
    // ssh session starts with a visible cursor instead of inheriting DECTCEM
    // off (issue #46). Reset the cursor shape to the user's default (DECSCUSR
    // 0) so the remote shell gets a blinking cursor rather than the steady
    // block ratatui can leave behind (issue #51).
    execute!(
        io::stdout(),
        LeaveAlternateScreen,
        Show,
        SetCursorStyle::DefaultUserShape
    )?;

    let mut child = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to launch ssh to {target}"))?;
    show_connecting_spinner(&target, &mut child);
    let _ = child.wait()?;

    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(())
}

/// Render a brief "Connecting to …" spinner on stderr while the ssh child is
/// establishing its session. Caps the animation so it can never linger over
/// the remote shell's output: cleared as soon as the child exits, the
/// 1.5 s budget is up, or we detect the connection has likely completed.
fn show_connecting_spinner(target: &str, child: &mut std::process::Child) {
    const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    const MAX_DURATION: Duration = Duration::from_millis(1500);
    const FRAME_INTERVAL: Duration = Duration::from_millis(80);

    let start = Instant::now();
    let mut idx = 0usize;
    loop {
        if matches!(child.try_wait(), Ok(Some(_))) {
            break;
        }
        if start.elapsed() >= MAX_DURATION {
            break;
        }
        let mut err = io::stderr().lock();
        let _ = write!(err, "\r\x1b[2K{} Connecting to {} …", FRAMES[idx], target);
        let _ = err.flush();
        drop(err);
        thread::sleep(FRAME_INTERVAL);
        idx = (idx + 1) % FRAMES.len();
    }
    let mut err = io::stderr().lock();
    let _ = write!(err, "\r\x1b[2K");
    let _ = err.flush();
}

fn connection_target_label(config: &Config, host: &Host) -> String {
    let program = host
        .command
        .as_ref()
        .unwrap_or(&config.defaults.command)
        .program();
    // Mirror connect::build_command: only ssh embeds a user@ prefix; for
    // other programs the user is resolved by their own configuration.
    let user = if program == "ssh" {
        connect::effective_user(config, host)
    } else {
        None
    };
    match user {
        Some(user) => format!("{user}@{}", host.ip),
        None => host.ip.clone(),
    }
}

/// Open the connection in a new terminal window without interrupting the TUI.
fn ssh_spawn(app: &mut App) {
    let host_cat = app
        .selected_host_with_category()
        .map(|(h, c)| (h.clone(), c.to_owned()));
    let Some((host, category_name)) = host_cat else {
        return;
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
            app.set_status_success(format!("Opened {} in new window", host.name));
        }
        Err(err) => {
            app.set_status_error(format!("Terminal launch failed: {err:#}"));
        }
    }
}

// ─── headless subcommands ───────────────────────────────────────────────────

fn run_config_cmd(cmd: &ConfigCmd, config_path: &Path) -> Result<()> {
    match cmd {
        ConfigCmd::Path => {
            println!("{}", config_path.display());
        }
        ConfigCmd::Show => {
            let cfg = Config::load_or_default(config_path)?;
            println!("{}", yaml_serde::to_string(&cfg)?);
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
            if config_path.exists() && !*force {
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

fn run_sync_cmd(cmd: SyncCmd, config_path: &Path) -> Result<()> {
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
            println!("branch: {branch}");
            println!("local:  {}", ctx.local_clone.display());
            println!("dirty:  {dirty}");
        }
    }
    Ok(())
}

fn run_list(args: &ListArgs, config: &Config) -> Result<()> {
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
            println!("{}", yaml_serde::to_string(&filtered)?);
        }
        ListFormat::Json => {
            let filtered: Vec<_> = categories.into_iter().cloned().collect();
            let mut out = String::from("[");
            for (i, c) in filtered.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write!(out, "{{\"name\":{:?},\"hosts\":[", c.name)?;
                for (j, h) in c.hosts.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    write!(out, "{{\"name\":{:?},\"ip\":{:?}}}", h.name, h.ip)?;
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

fn run_history(args: &HistoryArgs, history: &HistoryStore) -> Result<()> {
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
                write!(
                    out,
                    "{{\"host\":{:?},\"ip\":{:?},\"category\":{:?},\"connected_at\":{:?}}}",
                    e.host_name,
                    e.ip,
                    e.category,
                    e.connected_at_display()
                )?;
            }
            out.push(']');
            println!("{out}");
        }
        ListFormat::Yaml => {
            let data: Vec<_> = entries
                .iter()
                .map(|e| {
                    let mut m = yaml_serde::Mapping::new();
                    m.insert("host".into(), e.host_name.clone().into());
                    m.insert("ip".into(), e.ip.clone().into());
                    m.insert("category".into(), e.category.clone().into());
                    m.insert("connected_at".into(), e.connected_at_display().into());
                    yaml_serde::Value::Mapping(m)
                })
                .collect();
            println!("{}", yaml_serde::to_string(&data)?);
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
    let mut effective = host;
    if let Some(user) = args.user {
        effective.user = Some(user);
    }
    if let Some(port) = args.port {
        effective.port = Some(port);
    }
    if let Some(cmd) = args.command {
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
