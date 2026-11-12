## [1.2.4](https://github.com/Jaydee94/hoppr/compare/v1.2.3...v1.2.4) (2026-11-12)

### 🐛 Bug Fixes

* **cli:** emit real JSON from list and history ([7b6ad20](https://github.com/Jaydee94/hoppr/commit/7b6ad2067efc26709949042ae5d8821220ce59b7))
* **cli:** pass the remote shell's exit code through hoppr connect ([43cbe76](https://github.com/Jaydee94/hoppr/commit/43cbe76cb1cca8d3a4d3e1d98c45179165ff9c7e))
* **cli:** stop the config and category short flags from colliding ([c13f368](https://github.com/Jaydee94/hoppr/commit/c13f3689feadb68225f4768f4d0e671d10c8b7b4))
* **cli:** strip sync.repo credentials from the printed config ([e53833e](https://github.com/Jaydee94/hoppr/commit/e53833efb0a13c20a9e0e9930bf403ff7db1e429))
* **editor:** apply the defaults and sync forms when leaving them with esc ([614305a](https://github.com/Jaydee94/hoppr/commit/614305a9fb8353b19ac3517f8f5b945766631032))
* **editor:** apply the open form on ctrl+s and keep chords out of text inputs ([b3cee62](https://github.com/Jaydee94/hoppr/commit/b3cee620c5b7979aa79b5f1af2e870e6b680313a))
* **editor:** draw a progress message before the auto-push on save ([12e9b49](https://github.com/Jaydee94/hoppr/commit/12e9b49ee7966fb85e906252587792e270669981))
* **editor:** draw sync progress messages before the blocking git call ([1538142](https://github.com/Jaydee94/hoppr/commit/1538142db3ba549efdfdda01570a30636fab4bcb))
* **editor:** ignore ctrl and alt chords on plain-letter list keys ([0105dc3](https://github.com/Jaydee94/hoppr/commit/0105dc33a4e4c680d99a6467eb45d288664ba571))
* **editor:** keep stars and history when a host or category is renamed ([b67b2a3](https://github.com/Jaydee94/hoppr/commit/b67b2a3d68579d884beeede793c8d5e78729e0e6))
* **editor:** keep template args when applying the connection defaults ([1e4c3b2](https://github.com/Jaydee94/hoppr/commit/1e4c3b2c7dc8818bd9f016e489af7bd7a8d19829))
* **editor:** name a failed auto-push in the editor footer ([112ecda](https://github.com/Jaydee94/hoppr/commit/112ecdab0ef93af8189021716d5b738993edfc7d))
* **editor:** name ctrl+s, not s, in the save reminders ([374488f](https://github.com/Jaydee94/hoppr/commit/374488fb76826f5721818e97082838dac537e44f))
* **favorites:** keep stars apart when a name contains a slash ([0285e38](https://github.com/Jaydee94/hoppr/commit/0285e385fc6c7c41a154a7e9377b13743a55a925))
* **favorites:** match legacy star keys again after an in-session pull ([7883377](https://github.com/Jaydee94/hoppr/commit/788337745b914e9cc5afe6886c9027ae38f69191))
* **history,favorites:** save atomically instead of truncating in place ([847c448](https://github.com/Jaydee94/hoppr/commit/847c4483775a9e41f7e4cd51958cf5a43f619da4))
* **history:** dedupe recorded connections per category ([d800c26](https://github.com/Jaydee94/hoppr/commit/d800c26aa6e523f08074608e2fade53f4a57d4a5))
* **storage:** create the owner-only config file with mode 0600 ([d0d3a30](https://github.com/Jaydee94/hoppr/commit/d0d3a30d257a8381465ee4576d4a0f8b4c75b66c))
* **sync:** detect public keys by file extension, ignoring case ([4bf6c17](https://github.com/Jaydee94/hoppr/commit/4bf6c17ce192ed47b9a092b12029edc88161c48a))
* **sync:** fall through to key files and tokens when a credential fails ([b076d1b](https://github.com/Jaydee94/hoppr/commit/b076d1b9476efd7d22875f1b4ce3040bd9a86c5d))
* **sync:** only treat an @ inside the url authority as userinfo ([a9c40c6](https://github.com/Jaydee94/hoppr/commit/a9c40c6e5584be783aafda1f822a9f7a47d38c66))
* **sync:** redact credentials from the push summary and test flash ([36e1aeb](https://github.com/Jaydee94/hoppr/commit/36e1aebb14e51ec2827abb636584f4b770411e6e))
* **sync:** redact passwords in ssh and other non-http repo urls ([493e3d9](https://github.com/Jaydee94/hoppr/commit/493e3d9cb328ce028c3bc0f74ccdd285807c69df))
* **tui:** do not quit on ctrl+alt+c ([128e7a5](https://github.com/Jaydee94/hoppr/commit/128e7a50d9d59e58ff86f3e30e0a12a278e2ef11))
* **tui:** keep ctrl and alt chords away from the plain-letter bindings ([85c1605](https://github.com/Jaydee94/hoppr/commit/85c1605a016d52206913ad0bff1d669d8bb8d156))
* **tui:** keep running when the connect command cannot be started ([75b737d](https://github.com/Jaydee94/hoppr/commit/75b737d4458a87bc7c7b873566c95deb0365a372))
* **tui:** let a slash be typed into the browse search ([d90a1be](https://github.com/Jaydee94/hoppr/commit/d90a1beb12f08f66b2fb19ff7c58a7a01f496800))

### 📚 Documentation

* align contributor docs with the actual commit tooling ([d68e7ab](https://github.com/Jaydee94/hoppr/commit/d68e7abfb9ee146e17c045f1f4acc11145b3b594))
* **cli:** correct the documented exit codes ([4bcca3b](https://github.com/Jaydee94/hoppr/commit/4bcca3bdc8a2e4f7c8f98a58e90eeee9936e8b50))
* drop the RUST_LOG tip that nothing implements ([b727696](https://github.com/Jaydee94/hoppr/commit/b727696f8b2fff8ac70a8b27a1a14de7f4b23d4d))
* remove the obsolete install-script plan and spec ([0058640](https://github.com/Jaydee94/hoppr/commit/005864051d7846cbb642b704a6e0e3d06f3a6a71))

### 📦 Chores

* **deps:** update rust crate serde_json to v1.0.154 ([7482df4](https://github.com/Jaydee94/hoppr/commit/7482df4595e56c583888de5ffbaeabec72a113d6))

### ♻️ Code Refactoring

* apply readability fixes from clippy pedantic ([093bcbf](https://github.com/Jaydee94/hoppr/commit/093bcbf076769b27dccf700eb13b9ed6afed846d))
* **app:** remove the never-constructed warn status severity ([946e9c4](https://github.com/Jaydee94/hoppr/commit/946e9c471e6b0e0403bb5687727bfa9cd947618f))
* **config:** share YAML load and atomic save through a storage module ([f0f953d](https://github.com/Jaydee94/hoppr/commit/f0f953dd0a773c2ae7b1f55bd82bc75040b0ce83))
* drop needless Result wrappers and an unused parameter in main ([3129a93](https://github.com/Jaydee94/hoppr/commit/3129a93136e25d122d7790e0a7617abd732de247))
* **editor:** index sync form inputs with named constants ([d75f564](https://github.com/Jaydee94/hoppr/commit/d75f5649d23fdd5f1dc0692fbbc9925e037ac337))
* move settings-editor key handling into an input module ([09458d1](https://github.com/Jaydee94/hoppr/commit/09458d10f1ba7eb01bd0d1a3ffdb0434d1df53ce))
* remove unused functions, constants and the FormMode enum ([c8fe6f6](https://github.com/Jaydee94/hoppr/commit/c8fe6f697ffeb0431f60564776052e7acdf2cb46))
* split browse-mode key handling out of the event loop ([02aa123](https://github.com/Jaydee94/hoppr/commit/02aa1232ba553a8ad4db0821705ac0f7ac54d3ca))
* **sync:** drop the never-read SyncContext::auto_push field ([f164d6b](https://github.com/Jaydee94/hoppr/commit/f164d6b5d797845a8027873ab14bd144de1c5081))
* **theme:** remove unused border_strong token and style helpers ([add1c2d](https://github.com/Jaydee94/hoppr/commit/add1c2d6ecb83d4b8e13c18dedc63e6b5032f7a3))
* **ui:** derive the sync form layout from its label table ([33d4a2a](https://github.com/Jaydee94/hoppr/commit/33d4a2a9b7baa7fc640a4ca395ecbe7a5151bc6b))
* **ui:** route styling through theme helpers and table-driven hints ([676323c](https://github.com/Jaydee94/hoppr/commit/676323c8a7223e50032a10db82acc7867c5ceb14))

### 👷 Build System

* declare the real minimum supported Rust version (1.88) ([5011651](https://github.com/Jaydee94/hoppr/commit/5011651c92c2301077ad22114bec509efc956d4e))
* **deps:** refresh Cargo.lock with cargo update ([7e65e16](https://github.com/Jaydee94/hoppr/commit/7e65e168901204b959944740b39eb9a3ab8892fe))
* **deps:** remove unused thiserror dependency ([1eb876f](https://github.com/Jaydee94/hoppr/commit/1eb876f477f5d2249dac31101dcb4d01e6efffb5))
* **deps:** replace archived serde_yaml with yaml_serde ([05bdd63](https://github.com/Jaydee94/hoppr/commit/05bdd63d80d37a09333f134cef187c266813bb40))
* migrate to the Rust 2024 edition ([56e2826](https://github.com/Jaydee94/hoppr/commit/56e2826b16cc29f0c6bea9d3dc3f8a26829dfbec))

### 🔧 CI/CD

* pass workflow inputs to shell steps through env ([de03b80](https://github.com/Jaydee94/hoppr/commit/de03b80bc85a31c6e35c2a1fd17ae95bf5ce04b5))
* **release:** build macOS binaries on macos-latest ([1ab437e](https://github.com/Jaydee94/hoppr/commit/1ab437e2061028a494e5dce86755035d5107a2e7))
* **release:** derive versions with semantic-release ([6e8791b](https://github.com/Jaydee94/hoppr/commit/6e8791b4ae09d1546b914b6e5593af6d981ce974))

## v1.2.3 — 2026-11-04

- Merge pull request #92 from Jaydee94/renovate/node-26.x (7c65a56)
- Merge pull request #91 from Jaydee94/renovate/unicode-width-0.x-lockfile (17bc94e)
- chore(deps): update dependency node to v26 (2676f19)
- chore(deps): update rust crate unicode-width to v0.2.3 (e6b6d80)
- Merge pull request #90 from Jaydee94/renovate/commitlint-monorepo (b7a33ed)
- chore(deps): update commitlint monorepo to v21.2.4 (27d1778)
- Merge pull request #89 from Jaydee94/renovate/thiserror-2.x-lockfile (15984fe)
- Merge pull request #88 from Jaydee94/renovate/commitlint-monorepo (a55db7a)
- Merge pull request #87 from Jaydee94/renovate/clap_complete-4.x-lockfile (9f61b21)
- Merge pull request #86 from Jaydee94/renovate/clap-4.x-lockfile (159033d)
- chore(deps): update rust crate thiserror to v2.0.21 (ce5a337)
- chore(deps): update commitlint monorepo to v21.2.3 (340d444)
- chore(deps): update rust crate clap_complete to v4.6.11 (1171d2d)
- chore(deps): update rust crate clap to v4.6.7 (702c265)
- Merge pull request #82 from Jaydee94/renovate/clap-4.x-lockfile (8e6be89)
- Merge pull request #83 from Jaydee94/renovate/clap_complete-4.x-lockfile (0a88727)
- Merge pull request #85 from Jaydee94/renovate/commitlint-monorepo (d2b8684)
- chore(deps): update commitlint monorepo to v21.2.2 (432ce20)
- chore(deps): update rust crate clap_complete to v4.6.9 (8e43b94)
- chore(deps): update rust crate clap to v4.6.6 (8bd7e95)
- Merge pull request #84 from Jaydee94/renovate/thiserror-2.x-lockfile (2d18a71)
- chore(deps): update rust crate thiserror to v2.0.20 (e84250d)
- Merge pull request #81 from Jaydee94/renovate/time-0.x-lockfile (813b0ee)
- Merge pull request #80 from Jaydee94/renovate/clap-4.x-lockfile (3f1fae4)
- chore(deps): update rust crate time to v0.3.55 (9c52edc)
- chore(deps): update rust crate clap to v4.6.5 (2a7ada9)
- Merge pull request #79 from Jaydee94/renovate/clap_complete-4.x-lockfile (f1c65d9)
- chore(deps): update rust crate clap_complete to v4.6.8 (65441ad)
- Merge pull request #78 from Jaydee94/renovate/clap-4.x-lockfile (2cf7064)
- chore(deps): update rust crate clap to v4.6.4 (71c02b5)
- Merge pull request #77 from Jaydee94/renovate/time-0.x-lockfile (683daf1)
- Merge pull request #76 from Jaydee94/renovate/clap-4.x-lockfile (edda5a3)
- Merge pull request #75 from Jaydee94/renovate/thiserror-2.x-lockfile (5c554bb)
- Merge pull request #74 from Jaydee94/renovate/serde-monorepo (9947970)
- Merge pull request #73 from Jaydee94/renovate/anyhow-1.x-lockfile (9ab1ee5)
- chore(deps): update rust crate time to v0.3.54 (95c42dd)
- chore(deps): update rust crate thiserror to v2.0.19 (2be7119)
- chore(deps): update rust crate serde to v1.0.229 (ba1e762)
- chore(deps): update rust crate clap to v4.6.3 (df047b7)
- chore(deps): update rust crate anyhow to v1.0.104 (1b940e7)
- Merge pull request #72 from Jaydee94/renovate/clap-4.x-lockfile (603d65f)
- chore(deps): update rust crate clap to v4.6.2 (648985b)
- Merge pull request #71 from Jaydee94/renovate/actions-setup-node-7.x (8a022e1)
- chore(deps): update actions/setup-node action to v7 (c70d1a3)
- Merge pull request #70 from Jaydee94/renovate/commitlint-monorepo (951519c)
- chore(deps): update dependency @commitlint/cli to v21.2.1 (01afd14)
- Merge pull request #69 from Jaydee94/renovate/time-0.x-lockfile (e836bb6)
- Merge pull request #68 from Jaydee94/renovate/clap_complete-4.x-lockfile (ea8ce57)
- chore(deps): update rust crate time to v0.3.53 (978c77f)
- chore(deps): update rust crate clap_complete to v4.6.7 (daf8f18)
- Merge pull request #67 from Jaydee94/renovate/clap_complete-4.x-lockfile (5ff765d)
- Merge pull request #66 from Jaydee94/renovate/commitlint-monorepo (1df4513)
- Merge pull request #65 from Jaydee94/renovate/time-0.x-lockfile (5c7c6f6)
- chore(deps): update rust crate clap_complete to v4.6.6 (89158a7)
- chore(deps): update commitlint monorepo to v21.2.0 (0fcfe85)
- chore(deps): update rust crate time to v0.3.52 (c269702)
- Merge pull request #64 from Jaydee94/renovate/anyhow-1.x-lockfile (ac512c9)
- chore(deps): update rust crate anyhow to v1.0.103 (7b75662)
- Merge pull request #63 from Jaydee94/renovate/commitlint-monorepo (eda894f)
- chore(deps): update commitlint monorepo to v21.1.0 (c538a6d)
- Merge pull request #58 from Jaydee94/renovate/time-0.x-lockfile (cfe2ef5)
- chore(deps): update rust crate time to v0.3.51 (3975644)
- Merge pull request #62 from Jaydee94/renovate/node-24.x (39b454d)
- Merge pull request #61 from Jaydee94/renovate/actions-checkout-7.x (4e64300)
- chore(deps): update dependency node to v24 (d860f47)
- chore(deps): update actions/checkout action to v7 (678bb0a)
- chore: remove Dependabot config in favor of Renovate (#60) (9e33e0e)
- chore(deps): update rust crate ratatui to v0.30.2 (#57) (76b4ba1)
- chore(deps): bump time from 0.3.47 to 0.3.49 (#56) (f1035c7)
- chore(deps-dev): bump @commitlint/config-conventional from 19.8.1 to 21.0.2 (#48) (a62b53d)
- chore: configure Renovate (#45) (86fe4d0)
- chore(deps): bump ratatui from 0.30.0 to 0.30.1 (#55) (5616a8a)
- chore(deps): bump git2 from 0.20.4 to 0.21.0 (#44) (0a0eec9)
- chore(deps-dev): bump @commitlint/cli from 21.0.1 to 21.0.2 (#47) (ff663b4)
- chore(deps): bump unicode-width from 0.2.0 to 0.2.2 (#54) (30a9088)

## v1.2.2 — 2026-06-01

- fix: blinking cursor on exit (#51) and fixed-width category icons (#52) (#53) (8810b0b)
- fix(ui): render category icons in a fixed-width slot (7848dbc)
- fix(tui): restore blinking cursor on exit and ssh handoff (ce36605)

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
