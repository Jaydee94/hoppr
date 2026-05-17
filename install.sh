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
