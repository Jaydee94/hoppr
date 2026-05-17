<p align="center">
  <img src="assets/banner.svg" alt="hoppr" width="780">
</p>

<p align="center">
  <strong>Stop typing <code>ssh user@10.4.0.10 -p 2222</code>. Just hop.</strong><br>
  <em>A fast TUI for the servers you SSH into every day.</em>
</p>

<p align="center">
  <a href="https://github.com/Jaydee94/hoppr/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Jaydee94/hoppr/ci.yml?branch=main&label=ci&style=flat-square" alt="CI"></a>
  <a href="https://github.com/Jaydee94/hoppr/releases"><img src="https://img.shields.io/github/v/release/Jaydee94/hoppr?style=flat-square" alt="release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-7c5cff?style=flat-square" alt="license"></a>
  <img src="https://img.shields.io/badge/rust-1.75%2B-orange?style=flat-square" alt="rust">
</p>

<p align="center">
  <img src="assets/demo.svg" alt="hoppr four-step walk-through" width="960">
</p>

---

## What is hoppr?

`hoppr` is a tiny terminal launcher for SSH (and any other remote shell). You open it, type a few letters, hit `Enter` — you're connected. Hosts live in a YAML file you edit by hand, edit inside the TUI, or sync from a private git repo so your team works off the same inventory.

```bash
$ hoppr                       # browse + connect interactively
$ hoppr connect prod-db       # headless, scriptable
$ hoppr sync push             # share the inventory with your team
```

## Why hoppr?

If you SSH into more than a handful of machines, the same problems hit:

- Your shell aliases drift — half are outdated, half work, you can't tell which.
- Your `~/.ssh/config` is a wall of `Host` blocks you scroll through every morning.
- Your team has a doc somewhere with the right IPs, and nobody updates it.

hoppr replaces all three: one YAML file (optionally git-synced for the team), one keystroke to launch the TUI, fuzzy search to find the host, `Enter` to connect.

## Install

Pre-built binaries for Linux (x86_64, aarch64), macOS (Intel, Apple Silicon) and Windows are attached to every [release](https://github.com/Jaydee94/hoppr/releases/latest).

```bash
# Linux x86_64
curl -L https://github.com/Jaydee94/hoppr/releases/latest/download/hoppr-linux-x86_64.tar.gz \
  | tar -xz -C ~/.local/bin
```

Or from source:

```bash
cargo install --git https://github.com/Jaydee94/hoppr.git
```

## Quick start

```bash
hoppr config init       # writes a starter config at ~/.config/hoppr/config.yaml
hoppr                   # launch the TUI
```

In the TUI: `/` to search, `↑ ↓` to navigate, `Enter` to connect, `e` to edit hosts, `q` to quit. Full keymap → [`docs/keybindings.md`](docs/keybindings.md).

## Highlights

- **Opens in < 50 ms.** Built on [ratatui](https://github.com/ratatui-org/ratatui), single static binary.
- **Fuzzy search** across categories, with a global cross-category mode.
- **Edit in place** — add or rename hosts from inside the TUI, no shelling out to `$EDITOR`.
- **Team inventory in git** — point at a private repo; hoppr clones, pulls on launch, pushes on demand.
- **Favorites & history** — star the hosts you hop to often; the last 10 connections always sit at the top.
- **Pluggable transport** — `ssh`, `mosh`, `telnet`, `kitty +kitten ssh`, or a fully custom template.
- **CLI parity** — every TUI action is also a subcommand, so hoppr fits in scripts as cleanly as in your prompt.
- **Cross-platform** — Linux, macOS, Windows.

## Docs

The README stays short on purpose. Everything else lives in [`docs/`](./docs):

| Topic                                                  | File                                        |
| ------------------------------------------------------ | ------------------------------------------- |
| YAML schema, defaults, custom connect commands         | [`configuration.md`](docs/configuration.md) |
| Every subcommand and flag                              | [`cli.md`](docs/cli.md)                     |
| Team inventory, credentials, safety model              | [`sync.md`](docs/sync.md)                   |
| Browse-mode and edit-mode keymap                       | [`keybindings.md`](docs/keybindings.md)     |
| Color tokens & UI primitives                           | [`design-system.md`](docs/design-system.md) |
| Build, test, release process                           | [`development.md`](docs/development.md)     |

## Contributing

See [`CONTRIBUTING.md`](CONTRIBUTING.md). All commits follow [Conventional Commits](https://www.conventionalcommits.org/); the release workflow turns the log between tags into release notes.

## License

[MIT](LICENSE) · © 2026 hoppr contributors
