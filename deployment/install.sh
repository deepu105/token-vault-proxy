#!/bin/bash

# Install script for tv-proxy (Auth0 Token Vault Proxy)
# Licensed under the MIT license — see LICENSE for details.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/deepu105/token-vault-proxy/main/deployment/install.sh | bash
#
# Options (via environment variables):
#   BIN_DIR   — installation directory (default: /usr/local/bin)
#   VERSION   — specific version to install (default: latest)

set -e

GITHUB_USER="deepu105"
GITHUB_REPO="token-vault-proxy"
EXE_FILENAME="tv-proxy"
EXE_DEST_DIR="${BIN_DIR:-/usr/local/bin}"

bye() {
    result=$?
    if [ "$result" != "0" ]; then
        echo "Failed to install ${EXE_FILENAME}"
    fi
    exit $result
}

fail() {
    echo "Error: $1" >&2
    exit 1
}

find_arch() {
    local ARCH
    ARCH=$(uname -m)

    case "$ARCH" in
    x86_64 | amd64) ARCH="x64" ;;
    arm64 | aarch64) ARCH="arm64" ;;
    *) fail "Unsupported architecture: $ARCH" ;;
    esac

    echo "$ARCH"
}

find_os() {
    local OS
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')

    case "$OS" in
    linux) OS="linux" ;;
    darwin) OS="macos" ;;
    mingw* | msys* | cygwin*) fail "Windows is not supported. Use WSL instead." ;;
    *) fail "Unsupported operating system: $OS" ;;
    esac

    echo "$OS"
}

find_download_url() {
    local OS=$1
    local ARCH=$2
    local ARTIFACT="${EXE_FILENAME}-${OS}-${ARCH}"

    if [ -n "$VERSION" ]; then
        echo "https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases/download/${VERSION}/${ARTIFACT}"
    else
        local LATEST_URL="https://api.github.com/repos/${GITHUB_USER}/${GITHUB_REPO}/releases/latest"
        local URL
        URL=$(curl -fsSL "$LATEST_URL" | grep "browser_download_url.*${ARTIFACT}" | cut -d '"' -f 4 | head -n 1)
        if [ -z "$URL" ]; then
            return 1
        fi
        echo "$URL"
    fi
}

find_exec_dest_path() {
    if [ ! -d "$EXE_DEST_DIR" ]; then
        mkdir -p "$EXE_DEST_DIR" 2>/dev/null || true
    fi

    if [ ! -w "$EXE_DEST_DIR" ]; then
        echo "Cannot write to ${EXE_DEST_DIR}."
        echo "Run with 'sudo' to install to ${EXE_DEST_DIR}, or set BIN_DIR to a writable directory."
        echo "Installing to current directory instead..."
        EXE_DEST_DIR=$(pwd)
    fi
}

download_file() {
    local FILE_URL=$1
    local FILE_PATH=$2
    echo "Downloading ${EXE_FILENAME}..."
    echo "  ${FILE_URL}"
    local HTTP_CODE
    HTTP_CODE=$(curl -fsSL -w '%{http_code}' -o "$FILE_PATH" "$FILE_URL")
    if [ "$HTTP_CODE" != "200" ]; then
        fail "Download failed with HTTP status $HTTP_CODE"
    fi
}

main() {
    echo "Installing ${EXE_FILENAME}..."
    echo ""

    local ARCH
    ARCH=$(find_arch)
    local OS
    OS=$(find_os)

    echo "  Platform: ${OS}/${ARCH}"

    local FILE_URL
    FILE_URL=$(find_download_url "$OS" "$ARCH") || fail "No release found for ${OS}/${ARCH}. Check https://github.com/${GITHUB_USER}/${GITHUB_REPO}/releases"

    find_exec_dest_path

    local EXE_DEST_FILE="${EXE_DEST_DIR}/${EXE_FILENAME}"
    local TMP_FILE
    TMP_FILE=$(mktemp)

    download_file "$FILE_URL" "$TMP_FILE"
    mv "$TMP_FILE" "$EXE_DEST_FILE"
    chmod +x "$EXE_DEST_FILE"

    echo ""
    echo "✓ ${EXE_FILENAME} installed to ${EXE_DEST_FILE}"
    echo ""
    echo "Get started:"
    echo "  ${EXE_FILENAME} --help"
    echo "  ${EXE_FILENAME} init"
}

trap "bye" EXIT
main
