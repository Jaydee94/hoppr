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

`--config` is accepted anywhere on the command line. The short form `-c <PATH>` is only valid **before** the subcommand, because subcommands may give `-c` their own meaning (`hoppr list -c` is `--category`); `--help` therefore lists only `--config`. Passing both `-c` and `--config` is an error. Either flag overrides `HOPPR_CONFIG`.

```bash
hoppr -c ./demo.yaml list -c prod        # config file, then category filter
hoppr list --config ./demo.yaml -c prod  # same thing
```

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

`--format yaml` prints the matching categories together with the `defaults` and `sync` stanzas. Like `hoppr config show`, it strips credentials from `sync.repo` (see below).

### `hoppr config`

| subcommand            | effect                                                    |
| --------------------- | --------------------------------------------------------- |
| `hoppr config path`   | Print the active config file path.                        |
| `hoppr config show`   | Dump the resolved YAML to stdout.                         |
| `hoppr config edit`   | Open the file in `$VISUAL` / `$EDITOR`.                   |
| `hoppr config init`   | Write a starter config. Use `--force` to overwrite.       |

`hoppr config show` (and `hoppr list --format yaml`) is meant for reading, not for restoring a config: credentials embedded in an HTTP(S) `sync.repo` URL are stripped, so `https://user:token@host/team/hosts.git` is printed as `https://host/team/hosts.git`. The file named by `hoppr config path` is the source of truth — copy that file, not the dump, when you move a config to another machine.

### `hoppr sync`

See [`docs/sync.md`](sync.md) for the full story.

| subcommand               | effect                                                  |
| ------------------------ | ------------------------------------------------------- |
| `hoppr sync pull`        | Clone (if missing) and fast-forward pull the repo.      |
| `hoppr sync push`        | Copy local config to tracked path, commit and push.     |
| `hoppr sync status`      | Print repo URL, branch, local clone path and dirtiness. |

### `hoppr history`

Show the connection history (last 50 connections, most recent first). A host has one entry per category + name + IP, moved to the top each time you connect; the same machine listed under two categories keeps an entry for each.

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

| code | meaning                                                                                  |
| ---- | ---------------------------------------------------------------------------------------- |
| `0`  | success — including `--help`, `--version` and a `hoppr connect` whose remote shell ran   |
| `1`  | runtime error (see stderr): unreadable config, no host matched, sync failure, a program that could not be started |
| `2`  | usage error (see stderr): unknown flag or subcommand, missing or invalid argument        |

The exit status of the spawned remote-shell process is **not** passed through: `hoppr connect` exits `0` once the program has run, whatever it returned.
