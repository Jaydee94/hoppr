<p align="center">
  <img src="assets/banner.svg" alt="hoppr" width="780">
</p>

<p align="center">
  <em>A fast, minimal TUI launcher for SSH and other remote shells.</em>
</p>

<p align="center">
  <a href="https://github.com/Jaydee94/hoppr/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Jaydee94/hoppr/ci.yml?branch=main&label=ci&style=flat-square" alt="CI"></a>
  <a href="https://github.com/Jaydee94/hoppr/releases"><img src="https://img.shields.io/github/v/release/Jaydee94/hoppr?style=flat-square" alt="release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-7c5cff?style=flat-square" alt="license"></a>
  <img src="https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square" alt="rust">
</p>

<p align="center">
  <img src="assets/demo.svg" alt="hoppr TUI" width="900">
</p>

<sub>Animated demo: install <a href="https://github.com/charmbracelet/vhs">VHS</a> and run <code>vhs assets/demo.tape</code> to regenerate <code>assets/demo.gif</code>.</sub>

---

## What it does

`hoppr` is a tiny TUI you keep on a hotkey. Type to fuzzy-search hosts, hit `↩` to drop into an SSH session — no shell aliases to maintain, no copy-pasting from a notes file. Hosts live in a YAML file you can edit by hand, from inside the TUI, or sync from a central git repo across your machines.

```bash
$ hoppr            # interactive TUI
$ hoppr connect prod-gateway
$ hoppr list --category prod
$ hoppr sync push  # commit + push your local edits upstream
```

## Highlights

- **Fast TUI** — built on [ratatui](https://github.com/ratatui-org/ratatui), opens in < 50 ms.
- **In-TUI settings** — add hosts, edit categories, change defaults, save to YAML. No shelling out to an editor.
- **Central git config** — point at a repo URL; hoppr auto-clones, fast-forward pulls on every launch, pushes when you want.
- **Pluggable connect command** — defaults to `ssh`, supports `mosh`, `telnet`, `kitty +kitten ssh`, raw shell, or any custom template with `{user}` `{host}` `{port}` placeholders.
- **CLI parity** — every TUI action is also a subcommand (`connect`, `list`, `sync`, `config`).
- **Cross-platform** — Linux, macOS, Windows. Single static binary.

## Install

### Pre-built binaries

```bash
# pick the asset for your OS from the latest release:
#   https://github.com/Jaydee94/hoppr/releases/latest
curl -L https://github.com/Jaydee94/hoppr/releases/latest/download/hoppr-linux-x86_64.tar.gz \
  | tar -xz -C ~/.local/bin
```

### From source

```bash
git clone https://github.com/Jaydee94/hoppr.git
cd hoppr
cargo install --path .
```

## Quick start

```bash
hoppr config init        # writes ~/.config/hoppr/config.yaml
hoppr                    # launch the TUI
```

Inside the TUI:

| key       | action                          |
| --------- | ------------------------------- |
| `Tab`     | switch between Categories / Hosts |
| `/`       | search                          |
| `↑ ↓ j k` | navigate                        |
| `↩`       | connect to the selected host    |
| `e`       | open the in-TUI settings menu   |
| `q` `Esc` | quit                            |

## Docs

The full reference lives in [`docs/`](./docs):

- [`docs/configuration.md`](docs/configuration.md) — YAML schema, defaults, alternative connect commands
- [`docs/cli.md`](docs/cli.md) — every subcommand and flag
- [`docs/sync.md`](docs/sync.md) — central-repo sync, credentials, auto-push
- [`docs/keybindings.md`](docs/keybindings.md) — keymap for browse + edit modes
- [`docs/design-system.md`](docs/design-system.md) — color tokens & UI primitives
- [`docs/development.md`](docs/development.md) — build, test, release flow

## Contributing

Contributions welcome — see [`CONTRIBUTING.md`](CONTRIBUTING.md). All commits must follow [Conventional Commits](https://www.conventionalcommits.org/) so semantic-release can ship them automatically.

## License

[MIT](LICENSE) · © 2026 hoppr contributors
