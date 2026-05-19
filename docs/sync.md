# Central repo sync

`hoppr` can keep a shared **inventory** — categories and hosts — in a private git repository so a team works off the same set of VMs while each member keeps their own local preferences.

## What is shared, what is local

| lives in the synced repo (shared) | lives in the local `config.yaml` (per-machine) |
| --------------------------------- | ---------------------------------------------- |
| `categories[]` (with their hosts) | `sync` settings (URL, branch, path, auto-pull) |
|                                   | `defaults` (default `command`, port, user)     |

The synced file is an **inventory**, not a full configuration. Connection defaults, the sync stanza itself, and any other per-user setting never leave the local machine. New team members bootstrap by setting their own `sync.repo` and getting the team's hosts populated on first launch.

## How it works

When `sync.repo` is set, every launch of `hoppr` does:

1. If `~/.local/share/hoppr/config-repo` (or your `sync.local`) doesn't exist, **clone** it.
2. Otherwise, **fast-forward pull** the configured branch (skipped when `auto_pull: false`).
3. Read the tracked file as an inventory and replace the in-memory `categories` with what the team published.
4. Continue with the merged configuration (local `defaults` + shared `categories`).

Writes go the other direction — the local categories are the source of truth. Use `hoppr sync push` (or set `auto_push: true`) to write the inventory subset back into the clone, commit, and push. Your `defaults` and `sync` stanzas stay out of the repo.

git operations use **libgit2** (vendored, with bundled libssh2 + OpenSSL). Both HTTPS (`https://…`) and SSH (`git@host:…` / `ssh://…`) repo URLs are supported, and there is no runtime dependency on the system `git` / `ssh` binaries.

## First-time setup from the TUI

Open settings with `e`, pick **Central repo sync**, fill in the Repo URL (and optionally branch / path / local clone path). The `auto-pull` and `auto-push` rows render as checkboxes (`[●] On` / `[○] Off`); press `Space` (or `Enter`) on the focused row to flip them — no need to type `true`/`false` by hand.

Three action buttons sit underneath the form. Tab/↑↓ moves focus through them; `Enter` activates the focused button.

- **[Test connection]** runs a credentialed `ls-remote` against the URL currently in the form. Nothing is written to disk, so you can confirm SSH keys / tokens before committing the config.
- **[Sync now]** applies the form and then clones (first run) or fast-forward pulls. The latest inventory is loaded into the running session as soon as the network call finishes.
- **[Save]** applies the form and writes the local config to disk. When `auto-push` is on, the inventory is committed and pushed upstream in the same step. `Ctrl+S` is kept as a parallel keyboard shortcut for muscle memory.

Pressing `Enter` while focused on a plain text field still applies the form silently — and triggers an auto-clone if `sync.local` is missing — so the form remains usable for keyboard-only flows.

## Configuration

```yaml
sync:
  repo: git@github.com:you/hoppr-inventory.git  # required to enable sync
  branch: main                                  # default: main
  path: config.yaml                             # default: config.yaml — points at the inventory file in the repo
  local: ~/.local/share/hoppr/config-repo       # default: platform data dir
  auto_pull: true                               # default: true
  auto_push: false                              # default: false
```

| field       | default                              | notes                                                                 |
| ----------- | ------------------------------------ | --------------------------------------------------------------------- |
| `repo`      | _none_                               | When unset, sync is disabled. HTTPS or SSH URL.                       |
| `branch`    | `main`                               | Must already exist on the remote.                                     |
| `path`      | `config.yaml`                        | Inventory file path inside the repo. Multiple roles can share one repo with different file paths. |
| `local`     | platform data dir                    | Where hoppr clones the repo. `~` expansion is supported.              |
| `auto_pull` | `true` when `repo` is set            | Set to `false` to bypass the pull on every launch.                    |
| `auto_push` | `false`                              | When `true`, in-TUI saves are committed and pushed automatically.     |

## Inventory file format

The synced file contains only the shared categories:

```yaml
categories:
  - name: Production
    icon: "🚀"
    hosts:
      - name: prod-gateway
        ip: 10.4.0.1
        user: deploy
      - name: prod-db
        ip: 10.4.0.10
        user: ops
  - name: Staging
    hosts:
      - name: stage-1
        ip: 10.5.0.1
```

Any additional top-level keys in the file (left over from an older full-config layout, for example) are ignored — only `categories` is read.

## Credentials

The credential callback tries, in order:

1. The running SSH agent (any key it has loaded).
2. `~/.ssh/id_ed25519`, `~/.ssh/id_rsa`, `~/.ssh/id_ecdsa` (the first one that exists).
3. `HOPPR_GIT_USER` + `HOPPR_GIT_TOKEN` environment variables (for HTTPS).
4. The OS git credential helper (`credential.helper` from your global git config).
5. Default credentials.

SSH passphrase-protected keys are only usable via the agent — load them with `ssh-add` first.

## CLI

```bash
hoppr sync pull                      # one-shot fast-forward pull
hoppr sync push -m "feat: add lab"   # commit + push
hoppr sync status                    # show repo / branch / dirty
hoppr --no-sync                      # bypass auto-pull for one launch
hoppr --sync ls                      # opt-in to a sync even with auto_pull: false
```

## Status chip glyphs

The sync chip in the footer encodes its state with a glyph **and** a colour, so it stays readable for colourblind users:

| glyph        | colour  | meaning                                                    |
| ------------ | ------- | ---------------------------------------------------------- |
| `⊘ sync off` | muted   | `sync.repo` is unset — no sync configured.                 |
| `⊘ sync skipped` | muted | `auto_pull: false` (or `--no-sync`) bypassed the pull.    |
| `✕ sync error` | red    | The pull or push attempt failed; check the status line.    |
| `✓ synced`   | green   | Pull succeeded; the local clone is at the remote tip.      |
| `↻ synced`   | accent  | Pull pulled new commits that changed the inventory.        |
| `! unpushed` | warning | Suffix added when local commits have not been pushed yet.  |

## Safety

- Pulls are **fast-forward only**. If the remote and local have diverged, hoppr refuses to merge — resolve it by hand in the clone directory and try again.
- A push that would race against a remote update fails. Re-pull and try again.
- The pull happens **before** the YAML is parsed; a corrupt upstream file will surface as a startup error, not silently override your local state.
- Writes go via a `*.tmp` sibling + rename, so an interrupted save can't truncate your config.
- The saved config file is `chmod 0600` on Unix.
- HTTPS URLs containing `user:token@` are redacted in error messages and `hoppr sync status` output.
- `sync.path` values containing `..` segments are rejected — hoppr falls back to `config.yaml`.
- If `sync.local` exists but isn't a usable git repo (most commonly the half-cloned residue of a failed first run), hoppr wipes the directory and re-clones on the next start — provided it's empty or only contains a broken `.git/`. Directories with unrelated files are left alone and a clear error is surfaced so user data is never clobbered.

### Trust boundary

Anyone with write access to the sync repo can land a host entry with a raw
`cmd:` field, which is executed via `sh -c` when you connect to that host.
**Treat the sync repo as a code-execution surface** — restrict write access to
the same trust circle that has shell access to your machines, and review
diffs as carefully as you'd review a `.bashrc` change.

## Recommended layout

Teams typically keep a single private repo with role-scoped inventories:

```
my-hoppr-inventory/
  prod.yaml         # what the on-call rotation needs
  lab.yaml          # what every engineer can hop into
  jump-hosts.yaml   # bastions only
```

…then each machine points its `sync.path` at the inventory file relevant to that role. Connection defaults and the `sync` stanza stay in the machine's local `config.yaml` and never reach the repo.
