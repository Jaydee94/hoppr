use std::{
    cmp::Reverse,
    io::{self, Stdout, Write},
    path::{Path, PathBuf},
    process::{self, Command, ExitStatus, Stdio},
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
use serde::Serialize;

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
use history::{HistoryEntry, HistoryStore, default_history_path};
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
            let code = run_connect(args, &cfg)?;
            if code != 0 {
                process::exit(code);
            }
            Ok(())
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
        if app.show_help {
            match key.code {
                KeyCode::Char('?' | 'q') | KeyCode::F(1) | KeyCode::Esc => {
                    app.show_help = false;
                }
                _ => {}
            }
            continue;
        }

        if is_help_key(key.code, key.modifiers) && help_toggle_allowed(app) {
            app.toggle_help();
            continue;
        }

        let flow = match app.mode {
            Mode::Browse => handle_browse_key(terminal, app, key.code, key.modifiers)?,
            Mode::Edit => {
                let mut redraw = |app: &mut App| -> Result<()> {
                    terminal.draw(|frame| ui::draw(frame, app))?;
                    Ok(())
                };
                input::handle_editor_key(app, key.code, key.modifiers, &mut redraw)?
            }
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
    modifiers: KeyModifiers,
) -> Result<Flow> {
    match browse_key(app, code, modifiers) {
        BrowseStep::Quit => return Ok(Flow::Quit),
        BrowseStep::Connect => ssh_handoff(terminal, app)?,
        BrowseStep::Handled => {}
    }
    Ok(Flow::Continue)
}

/// What a browse-mode key press leaves for the caller to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowseStep {
    Handled,
    Quit,
    Connect,
}

/// The letter bindings are plain keys: `plain` is false for a letter held
/// with Ctrl or Alt, so Ctrl+E does not open the editor and Alt+Q does not
/// quit. The Ctrl chords in turn need Ctrl without Alt, because Ctrl+Alt is
/// how AltGr text arrives (see [`input::typed_char`]).
fn browse_key(app: &mut App, code: KeyCode, modifiers: KeyModifiers) -> BrowseStep {
    let ctrl = modifiers.contains(KeyModifiers::CONTROL);
    let ctrl_only = ctrl && !modifiers.contains(KeyModifiers::ALT);
    let plain = input::typed_char(code, modifiers).is_some();
    let searching = app.focus == Focus::Search;
    match code {
        KeyCode::Char('c') if ctrl_only => return BrowseStep::Quit,
        KeyCode::Esc if !searching => return BrowseStep::Quit,
        KeyCode::Char('q') if plain && !searching => return BrowseStep::Quit,
        KeyCode::Esc | KeyCode::Enter if searching => {
            app.clear_search_focus_hosts();
        }
        KeyCode::Tab => app.toggle_focus(),
        KeyCode::Char('/') if plain && !searching => {
            // `/` always starts a fresh search: a leftover query from an
            // earlier search is wiped so keystrokes don't append to stale
            // text. Inside the search input it is an ordinary character,
            // because category and host names may contain it.
            if !app.search_query.is_empty() {
                app.clear_search_query();
            }
            app.focus_search();
        }
        KeyCode::Char('u') if ctrl_only && searching => {
            app.clear_search_query();
        }
        KeyCode::Char('e') if plain && !searching => app.enter_edit_mode(),
        KeyCode::Up if !searching => app.previous(),
        KeyCode::Char('k') if plain && !searching => app.previous(),
        KeyCode::Down if !searching => app.next(),
        KeyCode::Char('j') if plain && !searching => app.next(),
        KeyCode::Backspace if searching => app.pop_search_char(),
        KeyCode::Char('a') if ctrl_only => toggle_search_scope(app),
        KeyCode::Char(_) if searching => type_into_search(app, code, modifiers),
        KeyCode::Char('f') if plain && app.focus == Focus::Hosts => toggle_favorite(app),
        KeyCode::Char('t') if plain && app.focus == Focus::Hosts && app.terminal.is_available() => {
            ssh_spawn(app);
        }
        KeyCode::Enter if app.focus == Focus::Hosts => return BrowseStep::Connect,
        _ => {}
    }
    BrowseStep::Handled
}

fn type_into_search(app: &mut App, code: KeyCode, modifiers: KeyModifiers) {
    if let Some(ch) = input::typed_char(code, modifiers) {
        app.append_search_char(ch);
    }
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

fn is_help_key(code: KeyCode, modifiers: KeyModifiers) -> bool {
    code == KeyCode::F(1) || input::typed_char(code, modifiers) == Some('?')
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

    let mut command = connect::build_command(&app.config, &host);
    let target = connection_target_label(&app.config, &host);

    match suspend_tui(terminal, || run_session(&mut command, &target))? {
        Ok(()) => {
            app.history.record(&host.name, &host.ip, &category_name);
            let _ = app.history.save();
        }
        Err(err) => app.set_status_error(format!("{err:#}")),
    }
    Ok(())
}

/// Hand the real terminal to `session`, then take it back.
///
/// Re-entering raw mode and the alternate screen is not conditional on
/// anything `session` does or on how far leaving got, so the event loop
/// never resumes on a cooked, main-screen terminal.
fn suspend_tui<T>(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    session: impl FnOnce() -> T,
) -> Result<T> {
    let outcome = leave_tui().map(|()| session());
    let resumed = resume_tui(terminal);
    let outcome = outcome?;
    resumed?;
    Ok(outcome)
}

fn leave_tui() -> Result<()> {
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
    Ok(())
}

fn resume_tui(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen)?;
    terminal.clear()?;
    Ok(())
}

/// Run `command` on the inherited terminal until it exits. The error names
/// the program, since a missing `ssh`/`mosh` binary is the usual cause.
fn run_session(command: &mut Command, target: &str) -> Result<()> {
    let program = command.get_program().to_string_lossy().into_owned();
    let mut child = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Could not start {program}"))?;
    show_connecting_spinner(target, &mut child);
    child
        .wait()
        .with_context(|| format!("Lost track of {program}"))?;
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
            println!("{}", printable_yaml(cfg)?);
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
            println!("{}", push_summary(&ctx));
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

fn push_summary(ctx: &SyncContext) -> String {
    format!("pushed to {}@{}", ctx.safe_url(), ctx.branch)
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
            println!("{}", printable_yaml(filtered)?);
        }
        ListFormat::Json => println!("{}", list_json(&categories)?),
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

/// YAML for stdout. Takes the config by value because the dump is not the
/// config: credentials embedded in `sync.repo` are stripped, as terminal
/// output ends up in scrollback, logs and pasted bug reports.
fn printable_yaml(mut config: Config) -> Result<String> {
    if let Some(repo) = config.sync.as_mut().and_then(|sync| sync.repo.as_mut()) {
        *repo = sync::redact_url(repo);
    }
    yaml_serde::to_string(&config).context("serialize config as YAML")
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
        ListFormat::Json => println!("{}", history_json(&entries)?),
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

#[derive(Serialize)]
struct JsonCategory<'a> {
    name: &'a str,
    hosts: Vec<JsonHost<'a>>,
}

#[derive(Serialize)]
struct JsonHost<'a> {
    name: &'a str,
    ip: &'a str,
}

#[derive(Serialize)]
struct JsonHistoryEntry<'a> {
    host: &'a str,
    ip: &'a str,
    category: &'a str,
    connected_at: String,
}

fn list_json(categories: &[&Category]) -> Result<String> {
    let rows: Vec<JsonCategory> = categories
        .iter()
        .map(|category| JsonCategory {
            name: &category.name,
            hosts: category
                .hosts
                .iter()
                .map(|host| JsonHost {
                    name: &host.name,
                    ip: &host.ip,
                })
                .collect(),
        })
        .collect();
    serde_json::to_string(&rows).context("serialize host list as JSON")
}

fn history_json(entries: &[&HistoryEntry]) -> Result<String> {
    let rows: Vec<JsonHistoryEntry> = entries
        .iter()
        .map(|entry| JsonHistoryEntry {
            host: &entry.host_name,
            ip: &entry.ip,
            category: &entry.category,
            connected_at: entry.connected_at_display(),
        })
        .collect();
    serde_json::to_string(&rows).context("serialize history as JSON")
}

fn truncate(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        return input.to_string();
    }
    let mut out: String = input.chars().take(max - 1).collect();
    out.push('…');
    out
}

/// Runs the connection and returns the exit code `hoppr connect` should end
/// with, so a script can tell a failed remote shell from a clean one.
fn run_connect(args: ConnectArgs, config: &Config) -> Result<i32> {
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
        return Ok(0);
    }
    let mut command = command;
    let status = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    Ok(exit_code_for(status))
}

/// A child killed by a signal has no exit code; shells report it as
/// 128 + the signal number, and scripts wrapping `hoppr connect` expect that.
fn exit_code_for(status: ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_session_reports_the_program_that_could_not_be_started() {
        let mut command = Command::new("hoppr-test-no-such-program");
        let err = run_session(&mut command, "ops@10.0.0.1").expect_err("spawn fails");
        let message = format!("{err:#}");
        assert!(
            message.starts_with("Could not start hoppr-test-no-such-program: "),
            "unexpected message: {message}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn exit_code_for_maps_codes_and_signals() {
        use std::os::unix::process::ExitStatusExt;

        assert_eq!(exit_code_for(ExitStatus::from_raw(0)), 0);
        assert_eq!(exit_code_for(ExitStatus::from_raw(3 << 8)), 3);
        assert_eq!(exit_code_for(ExitStatus::from_raw(255 << 8)), 255);
        assert_eq!(exit_code_for(ExitStatus::from_raw(15)), 128 + 15);
        assert_eq!(exit_code_for(ExitStatus::from_raw(9)), 128 + 9);
    }

    #[cfg(unix)]
    fn connect_args(query: &str, dry_run: bool) -> ConnectArgs {
        ConnectArgs {
            query: query.into(),
            user: None,
            port: None,
            command: None,
            dry_run,
        }
    }

    #[cfg(unix)]
    fn config_with_cmd_hosts() -> Config {
        let host = |name: &str, cmd: &str| Host {
            name: name.into(),
            ip: "127.0.0.1".into(),
            cmd: Some(cmd.into()),
            ..Host::default()
        };
        Config {
            categories: vec![Category {
                name: "local".into(),
                icon: None,
                hosts: vec![
                    host("clean", "exit 0"),
                    host("failing", "exit 7"),
                    host("killed", "kill -TERM $$"),
                ],
            }],
            ..Config::default()
        }
    }

    #[cfg(unix)]
    #[test]
    fn run_connect_returns_the_exit_code_of_the_child() {
        let config = config_with_cmd_hosts();
        let code = |query: &str, dry_run: bool| {
            run_connect(connect_args(query, dry_run), &config).expect("child runs")
        };
        assert_eq!(code("local/clean", false), 0);
        assert_eq!(code("local/failing", false), 7);
        assert_eq!(code("local/killed", false), 128 + 15);
        assert_eq!(code("local/failing", true), 0);
    }

    fn browsing_app(dir: &tempfile::TempDir) -> App {
        let config = Config {
            categories: vec![Category {
                name: "prod".into(),
                icon: None,
                hosts: ["web-01", "web-02"]
                    .into_iter()
                    .map(|name| Host {
                        name: name.into(),
                        ip: "10.0.0.1".into(),
                        ..Host::default()
                    })
                    .collect(),
            }],
            ..Config::default()
        };
        App::new(
            config,
            dir.path().join("config.yaml"),
            SyncStatus::Disabled,
            HistoryStore::load(&dir.path().join("history.yaml")),
            FavoritesStore::load(&dir.path().join("favorites.yaml")),
            TerminalLauncher::detect(None),
        )
    }

    const CTRL_ALT: KeyModifiers = KeyModifiers::CONTROL.union(KeyModifiers::ALT);

    #[test]
    fn browse_letters_ignore_ctrl_and_alt_chords() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        for focus in [Focus::Hosts, Focus::Categories] {
            for modifiers in [
                KeyModifiers::CONTROL,
                KeyModifiers::ALT,
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            ] {
                let mut app = browsing_app(&dir);
                app.focus = focus;
                let selected = (app.categories_state.selected(), app.hosts_state.selected());
                for letter in ['e', 'q', 'j', 'k', 'f', 't', '/'] {
                    let step = browse_key(&mut app, KeyCode::Char(letter), modifiers);
                    let context = format!("{focus:?} {modifiers:?}+{letter}");
                    assert_eq!(step, BrowseStep::Handled, "{context}");
                    assert_eq!(app.mode, Mode::Browse, "{context}");
                    assert_eq!(app.focus, focus, "{context}");
                    assert_eq!(
                        (app.categories_state.selected(), app.hosts_state.selected()),
                        selected,
                        "{context}"
                    );
                    assert!(app.active_status().is_none(), "{context}");
                }
                assert!(!dir.path().join("favorites.yaml").exists());
            }
        }
    }

    #[test]
    fn plain_browse_letters_and_documented_chords_still_act() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let none = KeyModifiers::NONE;

        let mut app = browsing_app(&dir);
        app.focus = Focus::Hosts;
        let first = app.hosts_state.selected();
        browse_key(&mut app, KeyCode::Char('j'), none);
        assert_ne!(app.hosts_state.selected(), first);
        browse_key(&mut app, KeyCode::Char('k'), none);
        assert_eq!(app.hosts_state.selected(), first);
        assert_eq!(
            browse_key(&mut app, KeyCode::Enter, none),
            BrowseStep::Connect
        );
        assert_eq!(
            browse_key(&mut app, KeyCode::Char('q'), none),
            BrowseStep::Quit
        );
        assert_eq!(
            browse_key(&mut app, KeyCode::Char('c'), CTRL_ALT),
            BrowseStep::Handled
        );
        assert_eq!(
            browse_key(&mut app, KeyCode::Char('c'), KeyModifiers::CONTROL),
            BrowseStep::Quit
        );

        browse_key(&mut app, KeyCode::Char('a'), KeyModifiers::CONTROL);
        assert!(app.search_all);
        browse_key(&mut app, KeyCode::Char('a'), CTRL_ALT);
        assert!(app.search_all);

        browse_key(&mut app, KeyCode::Char('/'), none);
        assert_eq!(app.focus, Focus::Search);
        browse_key(&mut app, KeyCode::Char('/'), none);
        assert_eq!(app.search_query, "/");
        browse_key(&mut app, KeyCode::Backspace, none);
        for (letter, modifiers) in [('q', none), ('e', none), ('a', CTRL_ALT), ('q', CTRL_ALT)] {
            let step = browse_key(&mut app, KeyCode::Char(letter), modifiers);
            assert_eq!(step, BrowseStep::Handled);
        }
        assert_eq!(app.search_query, "qeaq");
        browse_key(&mut app, KeyCode::Char('u'), CTRL_ALT);
        assert_eq!(app.search_query, "qeaqu");
        browse_key(&mut app, KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(app.search_query, "");

        browse_key(&mut app, KeyCode::Esc, none);
        browse_key(&mut app, KeyCode::Char('e'), none);
        assert_eq!(app.mode, Mode::Edit);
    }

    #[test]
    fn help_key_is_a_plain_question_mark_or_f1() {
        assert!(is_help_key(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(is_help_key(KeyCode::Char('?'), KeyModifiers::SHIFT));
        assert!(is_help_key(KeyCode::F(1), KeyModifiers::NONE));
        assert!(!is_help_key(KeyCode::Char('?'), KeyModifiers::CONTROL));
        assert!(!is_help_key(KeyCode::Char('?'), KeyModifiers::ALT));
    }

    #[test]
    fn search_input_ignores_chords_but_keeps_altgr_text() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let mut app = browsing_app(&dir);
        app.focus_search();

        type_into_search(&mut app, KeyCode::Char('w'), KeyModifiers::NONE);
        type_into_search(&mut app, KeyCode::Char('x'), KeyModifiers::CONTROL);
        type_into_search(&mut app, KeyCode::Char('y'), KeyModifiers::ALT);
        type_into_search(&mut app, KeyCode::Char('W'), KeyModifiers::SHIFT);
        type_into_search(
            &mut app,
            KeyCode::Char('@'),
            KeyModifiers::CONTROL | KeyModifiers::ALT,
        );
        assert_eq!(app.search_query, "wW@");
    }

    const AWKWARD: &str = "q\"b\\t\tn\nc\u{7}é\u{1F680}";

    #[test]
    fn list_json_keeps_layout_and_escapes_awkward_names() {
        let plain = Category {
            name: "prod".into(),
            icon: None,
            hosts: vec![Host {
                name: "web-01".into(),
                ip: "10.0.0.1".into(),
                ..Host::default()
            }],
        };
        assert_eq!(
            list_json(&[&plain]).expect("json"),
            r#"[{"name":"prod","hosts":[{"name":"web-01","ip":"10.0.0.1"}]}]"#
        );

        let awkward = Category {
            name: AWKWARD.into(),
            icon: None,
            hosts: vec![Host {
                name: AWKWARD.into(),
                ip: "10.0.0.2".into(),
                ..Host::default()
            }],
        };
        let parsed: serde_json::Value =
            serde_json::from_str(&list_json(&[&awkward]).expect("json")).expect("valid JSON");
        assert_eq!(parsed[0]["name"], AWKWARD);
        assert_eq!(parsed[0]["hosts"][0]["name"], AWKWARD);
    }

    #[test]
    fn history_json_keeps_layout_and_escapes_awkward_names() {
        let entry = HistoryEntry {
            host_name: "web-01".into(),
            ip: "10.0.0.1".into(),
            category: "prod".into(),
            connected_at: 0,
        };
        assert_eq!(
            history_json(&[&entry]).expect("json"),
            r#"[{"host":"web-01","ip":"10.0.0.1","category":"prod","connected_at":"1970-01-01 00:00"}]"#
        );

        let awkward = HistoryEntry {
            host_name: AWKWARD.into(),
            category: AWKWARD.into(),
            ..entry
        };
        let parsed: serde_json::Value =
            serde_json::from_str(&history_json(&[&awkward]).expect("json")).expect("valid JSON");
        assert_eq!(parsed[0]["host"], AWKWARD);
        assert_eq!(parsed[0]["category"], AWKWARD);
    }

    #[test]
    fn printable_yaml_strips_repo_credentials_and_nothing_else() {
        let config = Config {
            sync: Some(config::SyncConfig {
                repo: Some("https://alice:s3cret@git.example.com/team/hosts.git".into()),
                branch: Some("trunk".into()),
                ..Default::default()
            }),
            categories: vec![Category {
                name: "prod".into(),
                icon: None,
                hosts: vec![Host {
                    name: "web-01".into(),
                    ip: "10.0.0.1".into(),
                    user: Some("alice".into()),
                    ..Host::default()
                }],
            }],
            ..Config::default()
        };

        let printed = printable_yaml(config.clone()).expect("yaml");
        assert!(!printed.contains("s3cret"), "{printed}");
        let parsed: Config = yaml_serde::from_str(&printed).expect("still a config");
        let sync_cfg = parsed.sync.expect("sync stanza kept");
        assert_eq!(
            sync_cfg.repo.as_deref(),
            Some("https://git.example.com/team/hosts.git")
        );
        assert_eq!(sync_cfg.branch.as_deref(), Some("trunk"));
        assert_eq!(parsed.categories[0].hosts, config.categories[0].hosts);
    }

    #[test]
    fn printable_yaml_leaves_ssh_urls_and_unsynced_configs_alone() {
        let mut config = Config {
            sync: Some(config::SyncConfig {
                repo: Some("git@github.com:team/hosts.git".into()),
                ..Default::default()
            }),
            ..Config::default()
        };
        let printed = printable_yaml(config.clone()).expect("yaml");
        assert!(
            printed.contains("git@github.com:team/hosts.git"),
            "{printed}"
        );

        config.sync = None;
        let printed = printable_yaml(config).expect("yaml");
        assert!(printed.contains("sync: null"), "{printed}");
    }

    #[test]
    fn push_summary_does_not_leak_url_credentials() {
        let sync_cfg = config::SyncConfig {
            repo: Some("https://alice:s3cret@git.example.com/team/hosts.git".into()),
            branch: Some("trunk".into()),
            ..Default::default()
        };
        let ctx = SyncContext::from(&sync_cfg).expect("repo is set");
        assert_eq!(
            push_summary(&ctx),
            "pushed to https://git.example.com/team/hosts.git@trunk"
        );
    }
}
