## v1.2.1 — 2026-06-01

- Merge pull request #50 from Jaydee94/claude/parallel-issues-IsN2a (2ac49fd)
- feat(search): match multiple whitespace-separated terms across host metadata (8c4bde2)
- fix(connect): restore terminal cursor before handing off to ssh (492cd04)

## v1.2.0 — 2026-05-19

- feat(ui): add in-app help overlay (e7506b3)
- feat(editor): add filter to categories and hosts lists (9da9568)
- feat(editor): confirm destructive deletes in editor (b416171)
- fix(editor): prompt before exiting with unsaved changes (5c127f8)
- refactor(editor): unify Ctrl+S as the save shortcut (6ae210d)
- feat(editor): inline field-level validation feedback in forms (9b95164)
- feat(editor): support template ConnectCommand in HostForm (c8689b1)
- feat(editor): clarify Sync form button semantics (b610ca0)
- feat(ui): redundant glyphs for sync chip accessibility (9c80e95)
- feat(ui): always advertise Ctrl+A global-search toggle (ed6292f)
- feat(editor): expose defaults.terminal_command in Defaults form (4025fa3)
- fix(search): preserve selection and reset query on re-entry (3318c38)
- feat(ui): render sync auto_pull/auto_push as checkbox toggles (761e582)
- feat(editor): clarify Tab semantics in Hosts editor view (6a20509)
- feat(ui): add empty-state hints to categories, hosts, and search (9bebf2b)
- chore: ignore .claude/ agent worktrees (a8960a7)

## v1.1.3 — 2026-05-18

- Merge pull request #28 from Jaydee94/claude/improve-repo-sync-config-1KQKZ (6f9f521)
- fix(sync): detect push rejections + always attempt push on auto-push (503e37b)

## v1.1.2 — 2026-05-18

- Merge pull request #27 from Jaydee94/claude/improve-repo-sync-config-1KQKZ (e0c7b12)
- fix(sync): make the [Test connection] button actually exercise SSH auth (e8095d5)
- fix(sync): enable libssh2 in git2 so SSH repo URLs work (b58ceee)

## v1.1.1 — 2026-05-18

- Merge pull request #26 from Jaydee94/claude/improve-repo-sync-config-1KQKZ (4ccc4eb)
- feat(editor): replace sync hotkeys with focusable action buttons (3b8dab3)
- fix(sync): recover from half-cloned local repo on startup (4d05a73)

## v1.1.0 — 2026-05-18

- Merge pull request #25 from Jaydee94/claude/improve-repo-sync-config-1KQKZ (5b8188f)
- feat(ui): richer status bar with severity colors, sync freshness and host preview (bc3bcd7)
- feat(connect): only ssh embeds the user@ prefix; mosh/autossh/telnet skip it (6e03b9a)
- feat(connect): omit user prefix when neither host nor defaults set one (226e58b)
- feat(tui): improve sync editor UX and add ssh connection spinner (e0b3941)

## v1.0.0 — 2026-05-17

- docs: update keybinding references from Shift+Enter to t (89d4523)
- feat(terminal): switch new-tab keybinding from Shift+Enter to t (f0b709f)
- fix(terminal): add REPORT_ALL_KEYS_AS_ESCAPE_CODES for Shift+Enter detection (a4269ee)
- fix(terminal): use cmd.exe trampoline for Windows Terminal tab from WSL2 (c8cab1f)
- fix(tui): push keyboard enhancement flags unconditionally (f7bc74d)
- fix(terminal): use wsl.exe wrapper when spawning new tab from WSL2 (6692028)

## v0.1.5 — 2026-05-17

- feat(tui): enable keyboard enhancement flags for Shift+Enter support (f9e120c)
- fix(sync): guard credential callback against libgit2 retry loop (c744560)

## v0.1.4 — 2026-05-17

- fix(editor): auto-save on exit when there are unsaved changes (c979775)

## v0.1.3 — 2026-05-17

- style(ui): apply rustfmt formatting to editor hint arms (8bbdbec)
- fix(ui): context-sensitive editor hints and clearer save feedback (4dfcf65)
- fix(install.sh): move tmp_dir to script scope to fix unbound variable on exit (4ec2baa)

## v0.1.2 — 2026-05-17

- fix(ui): show settings keybinding in Browse mode hint bar (da9a039)
- Merge pull request #24 from Jaydee94/claude/add-install-scripts (35abb3f)
- docs(readme): update Install section with install script one-liners (645ff79)
- fix(install.ps1): move PATH success messages inside try block (a265704)
- fix(install.ps1): robust PATH manipulation for null and long PATH values (a1254a9)
- feat: add install.ps1 for Windows (58b303c)
- fix: validate tag format and binary presence in install.sh (1624a06)
- feat: add install.sh for Linux and macOS (c28e558)
- docs(plans): add install script implementation plan (a9e353f)
- docs(specs): add install script design doc (340b4bf)
- Merge pull request #23 from Jaydee94/claude/readme-overhaul-clearer (d92974d)
- docs(readme): rewrite for clarity, push details into docs/ (4b6c7a7)

## v0.1.1 — 2026-05-17

- Merge pull request #22 from Jaydee94/claude/fix-release-process-pbROE (81ea84c)
- ci(release): always build and publish on manual trigger (9e7c256)
- Merge pull request #21 from Jaydee94/fix/release-persist-credentials (c616afc)
- fix(ci): persist git credentials in release plan and dry-run jobs (d55ea82)
- Merge pull request #20 from Jaydee94/fix/release-plan-grep-pipefail (9719a62)
- fix(ci): guard grep pipeline against pipefail in release plan step (90bccac)
- Merge pull request #19 from Jaydee94/claude/fix-ci-releases-sZSm9 (3092832)
- chore(deps): update package-lock.json for bumped npm packages (70c7080)
- chore(deps): bump Cargo crates (32a7a61)
- chore(deps): bump npm release toolchain (5c7e082)
- ci(deps): bump GitHub Actions in ci workflow (d1de49b)
- fix(ci): repair release workflow builds (03753f3)
- Merge pull request #18 from Jaydee94/claude/fix-ci-releases-sZSm9 (c0ae0f9)
- fix(ci): sync Cargo.lock after version pin in release builds (ba997d2)
- Merge pull request #17 from Jaydee94/claude/hoppr-feature-planning-dUphM (1fd2608)
- feat: add history, favorites, global search, and new-window launch (7220091)
- Merge pull request #16 from Jaydee94/claude/external-config-vms-categories-OH1Wt (4167cd9)
- docs(demo): turn the static demo image into a four-step walk-through (f10e8d7)
- feat(sync): share only the VM/category inventory via the central repo (159099c)
- ci: commit npm package-lock.json so commitlint can run (e53247e)
- fix(clippy): satisfy rust 1.95 lints to unblock CI on main (83ea77a)
- Merge pull request #2 from Jaydee94/claude/hoppr-modernize-LEEcX (ed2814d)
- ci: add CI, manual semantic-release pipeline and conventional commit tooling (81d2e24)
- docs: add brand assets, README, full docs suite, LICENSE and CLAUDE.md (4ec65c8)
- feat: revamp hoppr with CLI, git sync, in-TUI editor and modern theme (736764c)
- Merge pull request #1 from Jaydee94/copilot/implement-tui-for-vm-manager (f8342c9)
- feat: implement modular hoppr TUI vm manager scaffold (11af3a2)
- Initial plan (246052f)
- Initial commit (1d217ab)

