# Contributing

Thanks for your interest in `hoppr`. This is a small project — keep PRs focused and the diff readable.

## Ground rules

- **English only** — code, comments, commits, docs, PR descriptions. If you find non-English text in the repo, translating it counts as a contribution.
- **Conventional Commits** — `commitlint` rejects everything else. Examples:
  - `feat(tui): add multi-select to host list`
  - `fix(sync): handle non fast-forward pulls`
  - `docs: document the kitty connect template`
  - `feat!: rename --config-file to --config`  ← breaking change
- **Docs sync** — if you change behavior, update `README.md` + the relevant file under `docs/` in the same PR.

## Workflow

1. Open an issue first for non-trivial changes — a quick "I'm thinking about doing X" beats a 500-line PR nobody asked for.
2. Branch from `main`. No long-lived feature branches.
3. Keep the diff small. Multiple small PRs > one big one.
4. CI must be green before review.

## Local setup

```bash
git clone https://github.com/Jaydee94/hoppr.git
cd hoppr
cargo build              # ~90 s cold (vendored libgit2)
cargo test
npm install              # optional — installs the commit-msg hook
```

## Before opening a PR

- [ ] `cargo fmt`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Documentation updated (see [`docs/`](docs/))
- [ ] Commits follow Conventional Commits

## PR description

Use the template — describe **what** changed and **why**, list anything you couldn't test locally, and link the related issue.

## License

By contributing, you agree that your contributions will be licensed under the [MIT license](LICENSE).
