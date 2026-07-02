#!/usr/bin/env bash
# Linux Installation Script for weeb-tui
# Usage: curl -fsSL https://raw.githubusercontent.com/silent9669/weeb-tui/main/packaging/linux/install.sh | bash

set -euo pipefail

REPO="silent9669/weeb-tui"
INSTALL_DIR="/usr/local/bin"
TEMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TEMP_DIR"' EXIT

show_banner() {
    cat <<'BANNER'
  /$$$$$$  /$$   /$$ /$$$$$$    /$$$$$$$$ /$$   /$$ /$$$$$$
 /$$__  $$| $$$ | $$|_  $$_/   |__  $$__/| $$  | $$|_  $$_/
| $$  \ $$| $$$$| $$  | $$        | $$   | $$  | $$  | $$
| $$$$$$$$| $$ $$ $$  | $$ /$$$$$$| $$   | $$  | $$  | $$
| $$__  $$| $$  $$$$  | $$|______/| $$   | $$  | $$  | $$
| $$  | $$| $$\  $$$  | $$        | $$   | $$   | $$  | $$
| $$  | $$| $$ \  $$ /$$$$$$      | $$   |  $$$$$$/ /$$$$$$
|__/  |__/|__/  \__/|______/      |__/    \______/ |______/

v3.8.4
BANNER
}

status() {
    printf '[weeb-tui] %s\n' "$1"
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        printf 'Missing required command: %s\n' "$1" >&2
        exit 1
    fi
}

download() {
    local url="$1"
    local output="$2"
    status "Downloading $(basename "$output")"
    curl --fail --location --progress-bar --output "$output" "$url"
}

show_banner

require_command curl
require_command tar

if ! command -v mpv >/dev/null 2>&1; then
    status "mpv is required for playback. Install it with your package manager before watching videos."
fi

ARCH="$(uname -m)"
if [ "$ARCH" != "x86_64" ]; then
    printf 'Unsupported Linux architecture for prebuilt binary: %s\n' "$ARCH" >&2
    exit 1
fi

LATEST_RELEASE="$(curl --fail --silent --location "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')"
if [ -z "$LATEST_RELEASE" ]; then
    printf 'Could not resolve latest weeb-tui release.\n' >&2
    exit 1
fi

ASSET="weeb-tui-linux-x86_64.tar.gz"
ARCHIVE_PATH="${TEMP_DIR}/${ASSET}"
download "https://github.com/${REPO}/releases/download/${LATEST_RELEASE}/${ASSET}" "$ARCHIVE_PATH"

status "Extracting weeb-tui"
tar -xzf "$ARCHIVE_PATH" -C "$TEMP_DIR"
chmod +x "${TEMP_DIR}/weeb-tui"

status "Installing to ${INSTALL_DIR}"
if [ ! -d "$INSTALL_DIR" ]; then
    sudo mkdir -p "$INSTALL_DIR"
fi
sudo cp "${TEMP_DIR}/weeb-tui" "${INSTALL_DIR}/weeb-tui"

status "Verifying installation"
"${INSTALL_DIR}/weeb-tui" --version
status "Installation complete"
