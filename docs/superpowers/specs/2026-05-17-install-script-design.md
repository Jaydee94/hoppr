# Install Script Design

**Date:** 2026-05-17
**Status:** Approved

## Goal

Provide a single command that installs the latest hoppr release binary and ensures it is on the user's PATH. Re-running the command acts as an update; it skips the download if the installed version already matches the latest release.

## Files

| File | Purpose |
|---|---|
| `install.sh` | POSIX shell installer for Linux and macOS |
| `install.ps1` | PowerShell installer for Windows x86_64 |

README is updated with one-liner invocation for each platform, replacing the existing manual steps.

## Unix script (`install.sh`)

### Platform detection

`uname -s` maps to `linux` or `darwin`. `uname -m` maps to `x86_64` or `aarch64`. These combine to select the release asset, e.g. `hoppr-linux-x86_64.tar.gz`. The script exits with a clear error message on any unsupported platform.

### Version check

The latest release tag is fetched from the GitHub API using `curl` and extracted with `grep`/`sed` — no `jq` dependency. If `hoppr` is already on `$PATH`, `hoppr --version` is compared against the fetched tag. The GitHub tag carries a `v` prefix (e.g. `v0.1.1`) while `hoppr --version` prints the bare version (`0.1.1`); the comparison strips the leading `v` from the tag. If versions match the script prints "Already up to date (vX.Y.Z)" and exits 0.

### Download and extract

The `.tar.gz` archive is downloaded to a `mktemp` temp directory. Extraction uses `tar -xz --strip-components=1` to discard the `hoppr-{version}-{target}/` subdirectory that wraps the binary inside the archive. Only the `hoppr` binary is moved to `~/.local/bin/`.

### PATH setup

If `~/.local/bin` is not in `$PATH`, the script appends `export PATH="$HOME/.local/bin:$PATH"` to every rc file that already exists among `~/.bashrc`, `~/.zshrc`, and `~/.profile`. After modification it prints a one-line reminder to source the rc file or open a new shell.

### Output style

One status line per major step. Final line: `hoppr vX.Y.Z installed — run 'hoppr --help' to get started`. PATH reminder printed only when modification was made.

## Windows script (`install.ps1`)

### Platform

Only `x86_64` is released for Windows, so no arch detection is needed. Asset: `hoppr-windows-x86_64.zip`.

### Version check

`Invoke-RestMethod` fetches the latest release from the GitHub API. `.tag_name` gives the version string. If `hoppr.exe` exists in the install directory, `hoppr --version` is compared against the tag (stripping the leading `v`) and the script exits early if already up to date.

### Download and extract

The `.zip` is downloaded to `$env:TEMP` and expanded with `Expand-Archive`. The binary is moved from the extracted `hoppr-{version}-{target}\` subdirectory to `$env:LOCALAPPDATA\hoppr\bin\hoppr.exe`. The directory is created if it does not exist.

### PATH setup

The user-level PATH is read from the registry via `[Environment]::GetEnvironmentVariable('PATH', 'User')`. If the install directory is absent it is appended and written back with `[Environment]::SetEnvironmentVariable`. This persists across new terminals without requiring administrator rights. The current shell session requires either a new terminal or a manual `$env:PATH` refresh; the script prints a reminder.

### Output style

Matches `install.sh`: one line per step, final confirmation, PATH reminder only when needed.

## README changes

- Replace the existing manual install steps with a one-liner for Unix and a one-liner for Windows.
- Add a brief note that re-running the command updates hoppr.
- Link to `install.sh` and `install.ps1` in the repo for users who want to inspect before running.

## Error handling

- Missing `curl` or `tar` on Unix: print dependency name and exit 1.
- GitHub API rate-limited or unreachable: surface the HTTP error and exit 1.
- Unsupported OS/arch: name the detected values and exit 1.
- Failed binary move (e.g. permissions): print the path and suggest `sudo` only if `/usr/local/bin` was the target; for `~/.local/bin` a permissions failure is unexpected and the raw error is sufficient.

## Out of scope

- Rollback / version pinning (can re-run script if needed).
- Package manager integrations (Homebrew formula, AUR, etc.).
- Windows ARM64 (no release target exists yet).
- Silent/non-interactive mode flags.
