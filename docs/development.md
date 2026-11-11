# Development

## Toolchain

[`rust-toolchain.toml`](../rust-toolchain.toml) selects the `stable` channel (plus `rustfmt` and `clippy`) for both local builds and CI — it tracks the latest stable release rather than pinning a version. The minimum supported Rust version is **1.88**, the highest `rust-version` declared by a dependency (`ratatui` 0.30 and `time` 0.3); it is recorded as `rust-version` in `Cargo.toml`.

```bash
rustup show                   # confirm the channel is installed
cargo --version
```

The first build takes ~90 s because `git2` vendors libgit2 + openssl. Incremental builds are fast.

## Building & testing

```bash
cargo build                    # debug
cargo build --release          # release (LTO thin, stripped)
cargo test                     # unit + integration tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Before pushing, run all four. CI fails on any of them.

## Running locally

```bash
cargo run -- --help            # show the CLI
cargo run                      # launches the TUI against your real config
cargo run -- --config ./tmp/demo.yaml   # isolated config
```

To test the sync flow without a real remote, point `sync.repo` at a local bare repo:

```bash
git init --bare /tmp/hoppr-remote.git
cargo run -- config init
yq -i '.sync.repo = "/tmp/hoppr-remote.git"' ~/.config/hoppr/config.yaml
cargo run -- sync push -m "init"
```

## Commits

Conventional Commits **only**. See [`CONTRIBUTING.md`](../CONTRIBUTING.md). The rules live in [`commitlint.config.cjs`](../commitlint.config.cjs).

There is no local `commit-msg` hook. The `commitlint` job in CI lints every commit of a pull request (base..head), so a single non-conforming commit fails the PR — the PR title itself is not checked. To run the same check locally before pushing (optional, needs Node):

```bash
npm install                               # installs commitlint
npx commitlint --from=origin/main         # lint the commits on your branch
```

## Release process

Releases are **manual**. Every triggered run always builds, tags, and publishes — there is no "no qualifying commits, skipping" branch.

From the GitHub UI:

1. Go to **Actions → release** (the workflow lives in [`.github/workflows/release.yml`](../.github/workflows/release.yml)).
2. Click **Run workflow** on the `main` branch and pick the inputs:
   - **`bump`** (default `patch`) — `patch` / `minor` / `major`. Used when `version` is empty.
   - **`version`** (optional) — explicit semver like `1.2.3`. When set, overrides `bump`.
   - **`dry_run`** (default `false`) — when on, the workflow only prints the planned version and exits.
3. The `plan` job picks the next version: explicit input if given, otherwise the highest existing `v*` tag bumped by `bump`. If there is no tag yet, the current `Cargo.toml` version is the baseline. The job refuses to overwrite an existing tag.
4. The `build` matrix compiles release binaries for:
   - `x86_64-unknown-linux-musl` → `hoppr-linux-x86_64.tar.gz`
   - `aarch64-unknown-linux-musl` → `hoppr-linux-aarch64.tar.gz` (cross-compiled)
   - `x86_64-apple-darwin` → `hoppr-macos-x86_64.tar.gz`
   - `aarch64-apple-darwin` → `hoppr-macos-aarch64.tar.gz`
   - `x86_64-pc-windows-msvc` → `hoppr-windows-x86_64.zip`

   Each build pins `Cargo.toml` to the planned version locally so the binary's `--version` matches the tag.
5. The `publish` job:
   - downloads every artifact and computes `checksums.txt` (SHA-256),
   - bumps `Cargo.toml` + `Cargo.lock` on `main`,
   - prepends a new section to `CHANGELOG.md` with the commits since the previous tag,
   - commits `chore(release): vX.Y.Z [skip ci]`,
   - creates an annotated `vX.Y.Z` tag and pushes both,
   - calls `gh release create vX.Y.Z --generate-notes` and attaches all five archives plus `checksums.txt`.

No human edits `Cargo.toml`'s version directly — the workflow owns it. To prepare a major release, write commits with `feat!:` or a `BREAKING CHANGE:` footer so reviewers know what's coming, then trigger the workflow with `bump: major` (or pass the exact `version`).

## Project layout

See [`CLAUDE.md`](../CLAUDE.md) for the per-file responsibilities. Public surface lives at module boundaries — no cross-module reach-through into private fields.

## Adding a dependency

Justify it in the PR description. We keep `cargo build` under 2 minutes cold. Heavy crates (anything bringing its own C compiler) should be considered carefully.

## Pre-PR checklist

- [ ] `cargo fmt`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Docs under `docs/` reflect the change
- [ ] `README.md` is still accurate (especially keybindings + CLI examples)
- [ ] At least one commit follows Conventional Commits and explains *why*

## Local development tips

- The TUI runs in a real terminal — `cargo test` covers a snapshot via `TestBackend`, but visual tweaks need a human eye.
- For the in-TUI editor, the `tempfile` crate is gold for round-tripping the YAML in tests.
- When debugging sync, set `RUST_LOG=trace` (not yet wired but planned) or shell into the local clone (`hoppr sync status` prints the path).
