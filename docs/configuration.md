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

| field              | type           | default | meaning                                                            |
| ------------------ | -------------- | ------- | ------------------------------------------------------------------ |
| `command`          | string \| obj  | `ssh`   | Default connect command for hosts that don't set one.              |
| `port`             | `u16`          | `22`    | Default port substituted into `{port}` and `ssh -p`.               |
| `user`             | string         | none    | Fallback user for the `ssh` shorthand when a host has no `user` field. When unset (or when the connect program isn't `ssh`), hoppr omits the `user@` prefix entirely so `ssh_config` (`User` directive) or the program's own default can take over. |
| `terminal_command` | string         | auto    | Terminal emulator used by `t` (open in new tab). When unset, hoppr auto-detects from the environment (Windows Terminal, iTerm2, GNOME Terminal, Konsole, xterm). Examples: `"wt"`, `"gnome-terminal"`, `"alacritty -e"`. Editable from the TUI: Settings → Connection defaults → Terminal command. |

### Alternative connect commands

The `command` field accepts two shapes:

**Shorthand** — just a program name. `hoppr` provides sensible default arguments:

```yaml
defaults:
  command: mosh        # → `mosh host`
```

Only `ssh` embeds the resolved `user@` prefix in the rendered command — every other program is assumed to read its own user from its own configuration (mosh from `--user`, autossh from `~/.ssh/config`, kitty/wezterm from their kittens, …). If you need a different user for a non-ssh program, set it through that program's config, or write a `Template` form (see below) that splices `{user}` where you want it.

| program       | resolved invocation                              |
| ------------- | ------------------------------------------------ |
| `ssh`         | `ssh -p <port> [<user>@]<host>`                  |
| `autossh`     | `autossh -p <port> <host>`                       |
| `mosh`        | `mosh [--ssh "ssh -p <port>"] <host>`            |
| `telnet`      | `telnet <host> <port>`                           |
| _(other)_     | `<program> <host>`                               |

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
| `user`     | string  |          | Only consumed by the `ssh` shorthand and by templates that reference `{user}`. For `ssh`, falls back to `defaults.user`; when neither is set the `user@` prefix is dropped so `~/.ssh/config` decides. For `mosh`/`autossh`/`telnet`/custom programs the field is ignored — configure the user inside that program's own config or use a `Template` form. Set `user: ""` to override an inherited `defaults.user` and force "no user" for a single host. |
| `port`     | `u16`   |          | Falls back to `defaults.port`.         |
| `cmd`      | string  |          | Raw shell command. Wins over everything else. |
| `command`  | string \| obj |    | Per-host structured override.          |

### Letting external tools set the user

Sometimes the user is already defined in `~/.ssh/config`:

```ssh-config
Host bastion.example.com
    User deploy
    IdentityFile ~/.ssh/deploy_ed25519
```

Leave `user` out of the hoppr config for those hosts and the rendered `ssh` command becomes `ssh -p 22 bastion.example.com`. SSH then picks up `User deploy` from the config.

For `mosh`, `autossh`, `telnet`, and any other shorthand program, hoppr never writes a `user@` prefix at all — those programs are expected to discover the user from their own configuration (or to be invoked through a `Template` form when you need a specific arg shape). Only templates that explicitly use `{user}` still fall back to `$USER` (then `root`) because the placeholder asks for a value.

## Editing

- **Hand-edit** the YAML in any editor. `hoppr config edit` opens `$EDITOR`.
- **In-TUI** via the settings menu (`e` from the main screen). Categories, hosts and global defaults all editable; save with `s`. The host form exposes both the `Program` and `Args` fields, so structured `Template` commands can be authored without dropping to YAML.
- **Central repo** via `hoppr sync push` — only `categories` are pushed to the shared inventory; `defaults` and the `sync` stanza stay on the local machine. See [`docs/sync.md`](sync.md).

`serde_yaml` does not preserve comments — keep narrative documentation in a separate `README.md` inside your config repo if you want it.
