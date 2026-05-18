# CLAUDE.md

Guidance for Claude Code when working on this repository.

## Project

`hoppr` is a Rust TUI launcher for SSH and other remote shells.
Stack: **Rust 2021**, **ratatui 0.29**, **crossterm**, **clap 4**, **git2** (vendored libgit2 + libssh2 + OpenSSL — both HTTPS and SSH repo URLs work without a system `git`/`ssh`), **serde_yaml**.

The binary is a single crate. There is no library target.

## Source layout

```
src/
  main.rs        entry point + CLI dispatch + TUI event loop
  cli.rs         clap argument and subcommand definitions
  config.rs      YAML schema (Config, Defaults, SyncConfig, Category, Host)
  connect.rs     builds the std::process::Command that launches the remote shell
  sync.rs        git2-backed central-repo clone / fast-forward / commit-and-push
  app.rs         TUI application state (focus, search, selection, editor handle)
  editor.rs      modal in-TUI settings editor (forms, validation)
  ui.rs          ratatui render functions, layout, popups
  theme.rs       design tokens (color palette + style helpers)
assets/          logo.svg, banner.svg, icon.svg, demo.svg, demo.tape
docs/            long-form documentation (English only)
```

## How to run things

```bash
cargo build              # debug build
cargo test               # run all tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all
cargo run -- --help      # check CLI surface
```

`cargo build` takes ~90 s cold because of vendored libgit2 + openssl. Subsequent rebuilds are < 5 s.

## Conventions

### Conventional Commits — required

Every commit message must follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>[optional scope]: <description>

[body]

[footer]
```

Allowed types: `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `chore`, `ci`, `build`, `style`, `revert`.
Breaking changes use `!` after the type (e.g. `feat!: drop ssh fallback`) **and** a `BREAKING CHANGE:` footer.

The manual release workflow (see [`docs/development.md`](docs/development.md)) auto-generates `CHANGELOG.md` and the GitHub release notes from these commits.

### English only

All commits, code comments, docs, error messages and PR descriptions are written in English. If you encounter non-English material that pre-dates this convention, translate it as part of the next reasonable commit.

### Docs sync — non-negotiable

**Before every commit + push**, review whether the changes affect public behavior, configuration, CLI surface, keybindings, or design tokens. If yes, update the relevant file under `docs/` and `README.md` in the same commit. README stays short — long-form goes in `docs/`.

### Code style

- No comments that describe *what* the code does — leave that to identifiers.
- Comments may explain *why* when the reason is non-obvious (invariant, workaround, subtle interaction).
- No `// TODO` left in `main` — open an issue instead.
- Public APIs get doc comments (`///`) only when the contract is non-trivial.
- One module = one responsibility. Don't make `app.rs` know how to build a git2 signature.

### Errors

- `anyhow::Result` at boundaries (CLI entry points, sync operations).
- `Context::context` is mandatory when surfacing a filesystem or git operation — the user needs to see *which path* / *which remote* failed.
- Inside library-style modules (`connect.rs`, `config.rs`), prefer specific `thiserror` enums only when callers need to match — otherwise `anyhow` is fine since this is a binary crate.

### TUI

- All visual styling lives in `src/theme.rs`. Don't hard-code colors elsewhere.
- The render path must stay allocation-light per frame — avoid building large `Vec`s in tight loops if state can be cached on `App`.
- The event loop polls at 200 ms; expensive work (sync, IO) belongs off the render thread or behind a one-shot action.

## CI / release

- **CI** (`.github/workflows/ci.yml`) runs fmt, clippy with `-D warnings`, tests, and a release build on every push and PR.
- **Release** (`.github/workflows/release.yml`) is **manual only** — `workflow_dispatch`. Every triggered run always builds and publishes. Inputs:
  - `bump` (`patch`/`minor`/`major`, default `patch`) — used when `version` is empty.
  - `version` (optional explicit semver) — overrides `bump`.
  - `dry_run` (boolean) — print the plan and exit.

  On a non-dry run it:
  1. computes the next version from the latest `v*` tag (or `Cargo.toml` if none) and the chosen bump,
  2. builds binaries for Linux x86_64 + aarch64 (musl), macOS x86_64 + aarch64, Windows x86_64,
  3. updates `Cargo.toml`, `Cargo.lock`, and prepends a section to `CHANGELOG.md`,
  4. commits `chore(release): vX.Y.Z [skip ci]`, tags `vX.Y.Z`, and pushes both,
  5. creates a GitHub Release via `gh release create --generate-notes` and attaches every archive + `checksums.txt`.

Never push to a release branch directly — open a PR, get it merged into `main`, then trigger the release workflow.

## Test guidance

- Each module owns its own `#[cfg(test)] mod tests`.
- Integration-ish tests that need a filesystem use `tempfile::TempDir`.
- `ui.rs` has a smoke test that renders into a `TestBackend` — keep it cheap, no flaky assertions on whitespace.
- Never write a test that requires network or an SSH agent.

## Common pitfalls

- `Config::save` is not atomic — fine for a hand-edited launcher, but don't rely on it inside `sync.rs` mid-pull.
- `git2` credential callbacks try, in order: SSH agent, `~/.ssh/id_*`, `$HOPPR_GIT_USER`/`$HOPPR_GIT_TOKEN`, then the OS git credential helper. Tests that touch sync must not depend on real credentials.
- `serde_yaml` does not preserve comments. The central-repo workflow assumes the source-of-truth is the YAML graph, not its formatting.

## Files Claude should treat as guardrails

- `commitlint.config.cjs` — commit message rules
- `rust-toolchain.toml` — pinned toolchain
- `.github/workflows/*.yml` — CI + release surface

Modifying these files requires the matching update to `docs/development.md`.
