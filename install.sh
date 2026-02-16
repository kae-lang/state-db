#!/bin/sh
# SMQL installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/kae-lang/state-db/main/install.sh | sh
set -e

REPO="kae-lang/state-db"
BINARY_NAME="smql"
INSTALL_DIR="${SMQL_INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)  OS="unknown-linux-musl" ;;
        Darwin) OS="apple-darwin" ;;
        *)      echo "Error: unsupported OS: $OS"; exit 1 ;;
    esac

    case "$ARCH" in
        x86_64|amd64)   ARCH="x86_64" ;;
        arm64|aarch64)  ARCH="aarch64" ;;
        *)              echo "Error: unsupported architecture: $ARCH"; exit 1 ;;
    esac

    TARGET="${ARCH}-${OS}"
}

# Get latest release version from GitHub
get_latest_version() {
    if [ -n "$SMQL_VERSION" ]; then
        VERSION="$SMQL_VERSION"
        return
    fi

    VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | sed -E 's/.*"v([^"]+)".*/\1/')"

    if [ -z "$VERSION" ]; then
        echo "Error: could not determine latest version"
        exit 1
    fi
}

# Download, verify, and install
install() {
    ARCHIVE="smql-${VERSION}-${TARGET}.tar.gz"
    URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ARCHIVE}"
    CHECKSUM_URL="${URL}.sha256"

    TMPDIR="$(mktemp -d)"
    trap 'rm -rf "$TMPDIR"' EXIT

    echo "Downloading smql v${VERSION} for ${TARGET}..."
    curl -fsSL "$URL" -o "${TMPDIR}/${ARCHIVE}"
    curl -fsSL "$CHECKSUM_URL" -o "${TMPDIR}/${ARCHIVE}.sha256"

    # Verify checksum
    echo "Verifying checksum..."
    cd "$TMPDIR"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "${ARCHIVE}.sha256"
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -c "${ARCHIVE}.sha256"
    else
        echo "Warning: no sha256 tool found, skipping checksum verification"
    fi

    # Extract
    tar xzf "${ARCHIVE}"

    # Install
    if [ -w "$INSTALL_DIR" ]; then
        mv "${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    else
        echo "Installing to ${INSTALL_DIR} (requires sudo)..."
        sudo mv "${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"
    fi

    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    echo ""
    echo "smql v${VERSION} installed to ${INSTALL_DIR}/${BINARY_NAME}"
    echo "Run 'smql --help' to get started."
}

detect_platform
get_latest_version
install
