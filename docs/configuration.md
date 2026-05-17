# Configuration

`hoppr` reads a single YAML file. By default it lives at:

| OS      | Path                                                    |
| ------- | ------------------------------------------------------- |
| Linux   | `$XDG_CONFIG_HOME/hoppr/config.yaml` (usually `~/.config/hoppr/config.yaml`) |
| macOS   | `~/Library/Application Support/dev.hoppr.hoppr/config.yaml` |
| Windows | `%APPDATA%\hoppr\hoppr\config\config.yaml`               |

Override with `--config /path/to/config.yaml` or the `HOPPR_CONFIG` environment variable.

Bootstrap a starter file with:

```bash
hoppr config init
```

## Schema

```yaml
defaults:
  command: ssh           # or a structured template — see below
  port: 22
  user: admin            # optional fallback when host omits user

sync:                    # optional — see docs/sync.md
  repo: git@github.com:you/hoppr-config.git
  branch: main
  path: config.yaml
  local: ~/.local/share/hoppr/config-repo
  auto_pull: true
  auto_push: false

categories:
  - name: Production
    icon: "🚀"
    hosts:
      - name: prod-gateway
        ip: 10.4.0.1
        user: deploy
        port: 22
      - name: prod-db
        ip: 10.4.0.10
        user: ops
        # per-host command override
        command:
          program: mosh
```

### `defaults`

| field     | type           | default | meaning                                                            |
| --------- | -------------- | ------- | ------------------------------------------------------------------ |
| `command` | string \| obj  | `ssh`   | Default connect command for hosts that don't set one.              |
| `port`    | `u16`          | `22`    | Default port substituted into `{port}` and `ssh -p`.               |
| `user`    | string         | none    | Used when a host has no `user` field and `$USER` is empty.         |

### Alternative connect commands

The `command` field accepts two shapes:

**Shorthand** — just a program name. `hoppr` provides sensible default arguments:

```yaml
defaults:
  command: mosh        # → `mosh user@host`
```

| program       | resolved invocation                              |
| ------------- | ------------------------------------------------ |
| `ssh`         | `ssh -p <port> <user>@<host>`                    |
| `autossh`     | `autossh -p <port> <user>@<host>`                |
| `mosh`        | `mosh [--ssh "ssh -p <port>"] <user>@<host>`     |
| `telnet`      | `telnet <host> <port>`                           |
| _(other)_     | `<program> <user>@<host>`                        |

**Template** — full control. Supports `{user}`, `{host}`, `{ip}`, `{port}`, `{name}` placeholders:

```yaml
defaults:
  command:
    program: kitty
    args:
      - "+kitten"
      - "ssh"
      - "-p"
      - "{port}"
      - "{user}@{host}"
```

Per-host overrides have the same shape (under `hosts[].command`).

### Raw shell commands

For one-off connections that don't fit a template — e.g. jumping through a bastion — set `hosts[].cmd`:

```yaml
- name: db-via-bastion
  ip: db.internal
  user: ops
  cmd: ssh -J bastion.example.com ops@db.internal
```

`cmd` is run through `sh -c` and wins over `command` / `defaults.command`.

### Categories & hosts

| field   | type   | required | notes                              |
| ------- | ------ | -------- | ---------------------------------- |
| `name`  | string | ✓        | Displayed verbatim.                |
| `icon`  | string |          | Any character — emoji works fine.  |
| `hosts` | list   | ✓        | At least one to be useful.         |

| host field | type    | required | notes                                  |
| ---------- | ------- | -------- | -------------------------------------- |
| `name`     | string  | ✓        | Display name and fuzzy-search target.  |
| `ip`       | string  | ✓        | IP, hostname or any string SSH accepts.|
| `user`     | string  |          | Falls back to `defaults.user` → `$USER` → `root`. |
| `port`     | `u16`   |          | Falls back to `defaults.port`.         |
| `cmd`      | string  |          | Raw shell command. Wins over everything else. |
| `command`  | string \| obj |    | Per-host structured override.          |

## Editing

- **Hand-edit** the YAML in any editor. `hoppr config edit` opens `$EDITOR`.
- **In-TUI** via the settings menu (`e` from the main screen). Categories, hosts and global defaults all editable; save with `s`.
- **Central repo** via `hoppr sync push` — see [`docs/sync.md`](sync.md).

`serde_yaml` does not preserve comments — keep narrative documentation in a separate `README.md` inside your config repo if you want it.
