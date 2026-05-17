# Central repo sync

`hoppr` can keep its config file in a private git repository so the same host list works across every machine you log into.

## How it works

When `sync.repo` is set, every launch of `hoppr` does:

1. If `~/.local/share/hoppr/config-repo` (or your `sync.local`) doesn't exist, **clone** it.
2. Otherwise, **fast-forward pull** the configured branch (skipped when `auto_pull: false`).
3. Copy the tracked file out of the clone into the active config location, if the local file is missing.
4. Load the YAML and continue normally.

Writes go the other direction — the active config is the source of truth. Use `hoppr sync push` (or set `auto_push: true`) to copy it back into the clone, commit, and push to the remote.

git operations use **libgit2** (vendored). There is no runtime dependency on the system `git` binary.

## Configuration

```yaml
sync:
  repo: git@github.com:you/hoppr-config.git    # required to enable sync
  branch: main                                  # default: main
  path: config.yaml                             # default: config.yaml
  local: ~/.local/share/hoppr/config-repo       # default: platform data dir
  auto_pull: true                               # default: true
  auto_push: false                              # default: false
```

| field       | default                              | notes                                                                 |
| ----------- | ------------------------------------ | --------------------------------------------------------------------- |
| `repo`      | _none_                               | When unset, sync is disabled. HTTPS or SSH URL.                       |
| `branch`    | `main`                               | Must already exist on the remote.                                     |
| `path`      | `config.yaml`                        | File path inside the repo. Multiple machines can share one repo with different file paths for different roles. |
| `local`     | platform data dir                    | Where hoppr clones the repo. `~` expansion is supported.              |
| `auto_pull` | `true` when `repo` is set            | Set to `false` to bypass the pull on every launch.                    |
| `auto_push` | `false`                              | When `true`, in-TUI saves are committed and pushed automatically.     |

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

## Safety

- Pulls are **fast-forward only**. If the remote and local have diverged, hoppr refuses to merge — resolve it by hand in the clone directory and try again.
- A push that would race against a remote update fails. Re-pull and try again.
- The pull happens **before** the YAML is parsed; a corrupt upstream file will surface as a startup error, not silently override your local state.
- Writes go via a `*.tmp` sibling + rename, so an interrupted save can't truncate your config.
- The saved config file is `chmod 0600` on Unix.
- HTTPS URLs containing `user:token@` are redacted in error messages and `hoppr sync status` output.
- `sync.path` values containing `..` segments are rejected — hoppr falls back to `config.yaml`.

### Trust boundary

Anyone with write access to the sync repo can land a host entry with a raw
`cmd:` field, which is executed via `sh -c` when you connect to that host.
**Treat the sync repo as a code-execution surface** — restrict write access to
the same trust circle that has shell access to your machines, and review
diffs as carefully as you'd review a `.bashrc` change.

## Recommended layout

Many users keep a single private repo with per-machine files:

```
my-hoppr-config/
  laptop.yaml
  workstation.yaml
  jump-host.yaml
```

…then point each machine's `sync.path` at the right file.
