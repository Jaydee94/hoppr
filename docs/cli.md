# CLI

Every interactive action has a matching subcommand so `hoppr` is fully scriptable.

```text
hoppr [GLOBAL FLAGS] [SUBCOMMAND]
```

If no subcommand is given, `hoppr` launches the TUI.

## Global flags

| flag                    | env             | meaning                                          |
| ----------------------- | --------------- | ------------------------------------------------ |
| `-c, --config <PATH>`   | `HOPPR_CONFIG`  | Use this config file instead of the default.     |
| `--no-sync`             |                 | Skip the central-repo pull on this invocation.   |
| `--sync`                |                 | Force a sync attempt even if disabled in config. |

## Subcommands

### `hoppr` / `hoppr tui`

Launches the interactive TUI. See [`docs/keybindings.md`](keybindings.md).

### `hoppr connect <query>`

Headless connect to a single host without opening the TUI.

```bash
hoppr connect prod-gateway
hoppr connect prod/gateway     # category/host form
hoppr connect 10.4.0.10        # match by IP
hoppr connect db --user ops --port 2200
hoppr connect db --command mosh --dry-run
```

| flag             | meaning                                                  |
| ---------------- | -------------------------------------------------------- |
| `-u, --user`     | Override the user from config.                           |
| `-p, --port`     | Override the port.                                       |
| `--command`      | Override the program (e.g. `mosh`, `telnet`).            |
| `--dry-run`      | Print the resolved command instead of executing it.      |

Matching: exact name → exact IP → `category/name` form → fuzzy match across name + IP.

### `hoppr list` / `hoppr ls`

List configured hosts.

```bash
hoppr list
hoppr list --category prod
hoppr list --format json
```

| flag                  | values                              | default  |
| --------------------- | ----------------------------------- | -------- |
| `-c, --category`      | substring                           | _all_    |
| `-o, --format`        | `table` · `json` · `yaml` · `plain` | `table`  |

### `hoppr config`

| subcommand            | effect                                                    |
| --------------------- | --------------------------------------------------------- |
| `hoppr config path`   | Print the active config file path.                        |
| `hoppr config show`   | Dump the resolved YAML to stdout.                         |
| `hoppr config edit`   | Open the file in `$VISUAL` / `$EDITOR`.                   |
| `hoppr config init`   | Write a starter config. Use `--force` to overwrite.       |

### `hoppr sync`

See [`docs/sync.md`](sync.md) for the full story.

| subcommand               | effect                                                  |
| ------------------------ | ------------------------------------------------------- |
| `hoppr sync pull`        | Clone (if missing) and fast-forward pull the repo.      |
| `hoppr sync push`        | Copy local config to tracked path, commit and push.     |
| `hoppr sync status`      | Print repo URL, branch, local clone path and dirtiness. |

### `hoppr history`

Show the connection history (last 50 connections, most recent first).

```bash
hoppr history
hoppr history --limit 10
hoppr history --format json
```

| flag           | values                              | default  |
| -------------- | ----------------------------------- | -------- |
| `-n, --limit`  | integer                             | `20`     |
| `-o, --format` | `table` · `json` · `yaml` · `plain` | `table`  |

History is stored locally at:

| OS      | Path                                                                |
| ------- | ------------------------------------------------------------------- |
| Linux   | `$XDG_DATA_HOME/hoppr/history.yaml`                                 |
| macOS   | `~/Library/Application Support/dev.hoppr.hoppr/history.yaml`       |
| Windows | `%APPDATA%\hoppr\hoppr\data\history.yaml`                           |

History is never synced to the central git repo — it is per-machine.

### `hoppr completions <shell>`

Emit a shell completion script.

```bash
hoppr completions bash > /etc/bash_completion.d/hoppr
hoppr completions zsh  > ~/.zsh/completions/_hoppr
hoppr completions fish > ~/.config/fish/completions/hoppr.fish
```

Supported: `bash`, `zsh`, `fish`, `elvish`, `powershell`.

## Exit codes

| code | meaning                                            |
| ---- | -------------------------------------------------- |
| `0`  | success                                            |
| `1`  | parsing or runtime error (see stderr)              |
| _N_  | exit code of the spawned remote-shell process      |
