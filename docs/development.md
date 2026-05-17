# Development

## Toolchain

`hoppr` pins its toolchain in [`rust-toolchain.toml`](../rust-toolchain.toml). Anything ≥ 1.75 should work locally, but CI uses the pinned channel.

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

Conventional Commits **only**. See [`CONTRIBUTING.md`](../CONTRIBUTING.md). The `commit-msg` hook (from `commitlint`) enforces this when you have Node installed locally:

```bash
npm install                    # installs husky + commitlint hooks
```

(Optional — CI will reject non-conforming PR titles either way.)

## Release process

Releases are **manual**. From the GitHub UI:

1. Go to **Actions → Release**.
2. Click **Run workflow**, choose the `main` branch.
3. semantic-release inspects commits since the last tag, computes the next version (`MAJOR.MINOR.PATCH`), updates `CHANGELOG.md` and `Cargo.toml`, tags `vX.Y.Z`, and publishes a GitHub Release.
4. The release job then builds binaries for:
   - `linux-x86_64` (musl)
   - `linux-aarch64` (musl, cross-compiled)
   - `macos-x86_64`
   - `macos-aarch64`
   - `windows-x86_64`
5. Tarballs / zips are uploaded as release assets.

No human edits the version number directly. To force a major release, write a commit with `BREAKING CHANGE:` in the footer or use the `!` shortcut (`feat!: ...`).

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
