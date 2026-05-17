# Install Script Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `install.sh` and `install.ps1` scripts that download and install the latest hoppr binary, skip the download if already up to date, and put the binary on the user's PATH automatically.

**Architecture:** Two standalone scripts — one POSIX shell (Linux/macOS), one PowerShell (Windows). Each detects the platform, fetches the latest GitHub release tag, compares it against the installed version, downloads and extracts the binary only when needed, places it in a user-local directory, and patches PATH if that directory is not already present. No external tool dependencies beyond `curl`/`tar` (Unix) and built-in PowerShell cmdlets (Windows).

**Tech Stack:** POSIX sh, PowerShell 5.1+, GitHub Releases API (`api.github.com/repos/.../releases/latest`)

---

## File Map

| Action | Path | Responsibility |
|---|---|---|
| Create | `install.sh` | POSIX installer: detect platform, version check, download, extract, PATH setup |
| Create | `install.ps1` | PowerShell installer: same flow for Windows x86_64 |
| Modify | `README.md` | Replace buggy manual Install steps with one-liner commands |

---

### Task 1: Create `install.sh`

**Files:**
- Create: `install.sh`

- [ ] **Step 1: Install shellcheck if not present**

```bash
# Ubuntu / Debian / WSL
sudo apt-get install -y shellcheck
# macOS
brew install shellcheck
# verify
shellcheck --version
```

- [ ] **Step 2: Write `install.sh`**

Create `install.sh` at the repo root with this exact content:

```bash
#!/usr/bin/env bash
set -euo pipefail

REPO="Jaydee94/hoppr"
INSTALL_DIR="${HOPPR_INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="hoppr"

detect_platform() {
    local os arch
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    arch=$(uname -m)

    case "$os" in
        linux)  os="linux" ;;
        darwin) os="macos" ;;
        *)      printf 'Unsupported OS: %s\n' "$os" >&2; exit 1 ;;
    esac

    case "$arch" in
        x86_64)        arch="x86_64" ;;
        aarch64|arm64) arch="aarch64" ;;
        *)             printf 'Unsupported architecture: %s\n' "$arch" >&2; exit 1 ;;
    esac

    printf '%s-%s' "$os" "$arch"
}

fetch_latest_tag() {
    local tag
    tag=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')

    if [ -z "$tag" ]; then
        printf 'Failed to fetch latest release tag from GitHub API\n' >&2
        exit 1
    fi

    printf '%s' "$tag"
}

get_installed_version() {
    if command -v "$BINARY_NAME" > /dev/null 2>&1; then
        "$BINARY_NAME" --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1
    fi
}

setup_path() {
    local dir="$1"
    if [[ ":$PATH:" != *":${dir}:"* ]]; then
        local export_line="export PATH=\"${dir}:\$PATH\""
        local rc_files=("$HOME/.bashrc" "$HOME/.zshrc" "$HOME/.profile")
        local modified=0
        for rc in "${rc_files[@]}"; do
            if [ -f "$rc" ] && ! grep -qF "$export_line" "$rc"; then
                printf '\n# Added by hoppr installer\n%s\n' "$export_line" >> "$rc"
                modified=1
            fi
        done
        if [ "$modified" -eq 1 ]; then
            printf '\nAdded %s to PATH in your shell rc file(s).\n' "$dir"
            printf 'Run: source ~/.bashrc  (or open a new terminal)\n'
        fi
    fi
}

main() {
    printf 'Fetching latest hoppr release...\n'

    local platform latest_tag latest_version installed_version
    platform=$(detect_platform)
    latest_tag=$(fetch_latest_tag)
    latest_version="${latest_tag#v}"
    installed_version=$(get_installed_version)

    if [ "${installed_version}" = "${latest_version}" ]; then
        printf 'Already up to date (v%s)\n' "$latest_version"
        exit 0
    fi

    local asset_name="hoppr-${platform}.tar.gz"
    local download_url="https://github.com/${REPO}/releases/download/${latest_tag}/${asset_name}"
    local tmp_dir
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    printf 'Downloading %s...\n' "$asset_name"
    curl -fsSL "$download_url" -o "${tmp_dir}/${asset_name}"

    printf 'Extracting...\n'
    tar -xz --strip-components=1 -C "$tmp_dir" -f "${tmp_dir}/${asset_name}"

    mkdir -p "$INSTALL_DIR"
    mv "${tmp_dir}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    setup_path "$INSTALL_DIR"

    printf '\nhoppr v%s installed — run '\''hoppr --help'\'' to get started\n' "$latest_version"
}

main
```

- [ ] **Step 3: Make it executable**

```bash
chmod +x install.sh
```

- [ ] **Step 4: Run shellcheck**

```bash
shellcheck install.sh
```

Expected: no output (zero warnings, zero errors). Fix any issues shellcheck reports before proceeding.

- [ ] **Step 5: Smoke-test platform detection**

```bash
bash -c 'source ./install.sh; detect_platform'
```

Expected output (example on Linux x86_64): `linux-x86_64`

- [ ] **Step 6: Commit**

```bash
git add install.sh
git commit -m "feat: add install.sh for Linux and macOS

Downloads the latest hoppr release, skips if already up to date,
installs to ~/.local/bin, and adds it to PATH if needed.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 2: Create `install.ps1`

**Files:**
- Create: `install.ps1`

- [ ] **Step 1: Write `install.ps1`**

Create `install.ps1` at the repo root with this exact content:

```powershell
$ErrorActionPreference = 'Stop'

$Repo       = 'Jaydee94/hoppr'
$InstallDir = Join-Path $env:LOCALAPPDATA 'hoppr\bin'
$BinaryName = 'hoppr.exe'
$BinaryPath = Join-Path $InstallDir $BinaryName

function Get-LatestTag {
    $release = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
    return $release.tag_name
}

function Get-InstalledVersion {
    if (Test-Path $BinaryPath) {
        try {
            $output = & $BinaryPath --version 2>$null
            if ($output -match '(\d+\.\d+\.\d+)') { return $Matches[1] }
        } catch {}
    }
    return $null
}

Write-Host 'Fetching latest hoppr release...'

$LatestTag        = Get-LatestTag
$LatestVersion    = $LatestTag.TrimStart('v')
$InstalledVersion = Get-InstalledVersion

if ($InstalledVersion -eq $LatestVersion) {
    Write-Host "Already up to date (v$LatestVersion)"
    exit 0
}

$AssetName   = 'hoppr-windows-x86_64.zip'
$DownloadUrl = "https://github.com/$Repo/releases/download/$LatestTag/$AssetName"
$TempDir     = Join-Path $env:TEMP "hoppr-install-$(Get-Random)"
New-Item -ItemType Directory -Path $TempDir -Force | Out-Null

try {
    Write-Host "Downloading $AssetName..."
    $ZipPath = Join-Path $TempDir $AssetName
    Invoke-WebRequest -Uri $DownloadUrl -OutFile $ZipPath

    Write-Host 'Extracting...'
    Expand-Archive -Path $ZipPath -DestinationPath $TempDir -Force

    $ExtractedBinary = Get-ChildItem -Path $TempDir -Filter $BinaryName -Recurse |
                       Select-Object -First 1
    if (-not $ExtractedBinary) {
        Write-Error "Could not find $BinaryName in downloaded archive"
        exit 1
    }

    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -Path $ExtractedBinary.FullName -Destination $BinaryPath -Force

    $UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable('PATH', "$UserPath;$InstallDir", 'User')
        Write-Host ''
        Write-Host "Added $InstallDir to your user PATH."
        Write-Host 'Open a new terminal for PATH changes to take effect.'
    }

    Write-Host ''
    Write-Host "hoppr v$LatestVersion installed — run 'hoppr --help' to get started"
} finally {
    Remove-Item -Path $TempDir -Recurse -Force -ErrorAction SilentlyContinue
}
```

- [ ] **Step 2: Commit**

```bash
git add install.ps1
git commit -m "feat: add install.ps1 for Windows

Downloads latest hoppr release, skips if already up to date,
installs to %LOCALAPPDATA%\hoppr\bin, and adds it to user PATH.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```

---

### Task 3: Update `README.md`

**Files:**
- Modify: `README.md:43-57`

- [ ] **Step 1: Replace the Install section**

In `README.md`, replace the entire `## Install` section (lines 43–57) with:

```markdown
## Install

### Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/Jaydee94/hoppr/main/install.sh | bash
```

### Windows (PowerShell)

```powershell
irm https://raw.githubusercontent.com/Jaydee94/hoppr/main/install.ps1 | iex
```

Re-running either command updates hoppr to the latest release. The binary is installed to `~/.local/bin` (Unix) or `%LOCALAPPDATA%\hoppr\bin` (Windows); both locations are added to your PATH automatically if not already present. Inspect the scripts before running: [`install.sh`](install.sh) · [`install.ps1`](install.ps1).

Or build from source:

```bash
cargo install --git https://github.com/Jaydee94/hoppr.git
```
```

- [ ] **Step 2: Verify the README renders correctly**

```bash
# Check there are no broken markdown fences (odd number of ``` fences is a common mistake)
grep -c '^\`\`\`' README.md
```

Expected: an even number (each opening fence has a closing fence).

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs(readme): update Install section with install script one-liners

Replaces the manual (and incorrect) tar extraction command with
curl-pipe-bash and irm-pipe-iex one-liners pointing to install.sh
and install.ps1.

Co-Authored-By: Claude Sonnet 4.6 <noreply@anthropic.com>"
```
