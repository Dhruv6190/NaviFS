#!/usr/bin/env bash
# NaviFS Universal 1-Click Plug & Play Installer for macOS and Linux.
# Installs navifs to $HOME/.local/bin, updates PATH, and auto-configures AI agents.

set -euo pipefail

REPO="Dhruv6190/NaviFS"
INSTALL_DIR="${NAVIFS_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$INSTALL_DIR"
DEST_EXE="$INSTALL_DIR/navifs"

echo -e "\033[1;36m========================================================\033[0m"
echo -e "\033[1;36m       NaviFS Universal Plug & Play Installer           \033[0m"
echo -e "\033[1;36m========================================================\033[0m"

# 1. Check local build candidate first
INSTALLED=0
if [ -f "./target/release/navifs" ]; then
    echo -e "\033[1;32m[OK] Using locally built binary from ./target/release/navifs\033[0m"
    cp "./target/release/navifs" "$DEST_EXE"
    INSTALLED=1
elif [ -n "${CARGO_TARGET_DIR:-}" ] && [ -f "$CARGO_TARGET_DIR/release/navifs" ]; then
    echo -e "\033[1;32m[OK] Using locally built binary from $CARGO_TARGET_DIR/release/navifs\033[0m"
    cp "$CARGO_TARGET_DIR/release/navifs" "$DEST_EXE"
    INSTALLED=1
fi

if [ "$INSTALLED" -eq 0 ]; then
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    TARGET=""
    case "$OS" in
        Linux)
            case "$ARCH" in
                x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
                aarch64) TARGET="aarch64-unknown-linux-gnu" ;;
                *) echo "Unsupported Linux architecture: $ARCH" && exit 1 ;;
            esac
            ;;
        Darwin)
            case "$ARCH" in
                x86_64) TARGET="x86_64-apple-darwin" ;;
                arm64)  TARGET="aarch64-apple-darwin" ;;
                *) echo "Unsupported macOS architecture: $ARCH" && exit 1 ;;
            esac
            ;;
        *)
            echo "Unsupported operating system: $OS" && exit 1
            ;;
    esac

    VERSION="${NAVIFS_VERSION:-latest}"
    if [ "$VERSION" = "latest" ]; then
        BASE_URL="https://github.com/$REPO/releases/latest/download"
    else
        BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
    fi

    ARCHIVE_NAME="navifs-$TARGET.tar.gz"
    DOWNLOAD_URL="$BASE_URL/$ARCHIVE_NAME"
    CHECKSUM_URL="$BASE_URL/checksums.txt"

    TEMP_DIR="$(mktemp -d)"
    trap 'rm -rf "$TEMP_DIR"' EXIT

    echo "Downloading $DOWNLOAD_URL..."
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_DIR/$ARCHIVE_NAME"
        curl -fsSL "$CHECKSUM_URL" -o "$TEMP_DIR/checksums.txt" 2>/dev/null || true
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$DOWNLOAD_URL" -O "$TEMP_DIR/$ARCHIVE_NAME"
        wget -q "$CHECKSUM_URL" -O "$TEMP_DIR/checksums.txt" 2>/dev/null || true
    else
        echo "Neither curl nor wget found. Please install one to proceed." && exit 1
    fi

    # Verify checksum if checksums.txt exists
    if [ -f "$TEMP_DIR/checksums.txt" ]; then
        EXPECTED_HASH="$(grep "$ARCHIVE_NAME" "$TEMP_DIR/checksums.txt" | awk '{print $1}')"
        if [ -n "$EXPECTED_HASH" ]; then
            if command -v sha256sum >/dev/null 2>&1; then
                ACTUAL_HASH="$(sha256sum "$TEMP_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
            elif command -v shasum >/dev/null 2>&1; then
                ACTUAL_HASH="$(shasum -a 256 "$TEMP_DIR/$ARCHIVE_NAME" | awk '{print $1}')"
            else
                ACTUAL_HASH=""
            fi

            if [ -n "$ACTUAL_HASH" ] && [ "$ACTUAL_HASH" != "$EXPECTED_HASH" ]; then
                echo "Checksum verification failed for $ARCHIVE_NAME!" && exit 1
            elif [ -n "$ACTUAL_HASH" ]; then
                echo -e "\033[1;32m[OK] SHA-256 verified: $ACTUAL_HASH\033[0m"
            fi
        fi
    fi

    tar -xzf "$TEMP_DIR/$ARCHIVE_NAME" -C "$TEMP_DIR"
    cp "$TEMP_DIR/navifs" "$DEST_EXE"
fi

chmod +x "$DEST_EXE"
echo -e "\033[1;32m[OK] Installed binary to: $DEST_EXE\033[0m"

# 2. Add to PATH in shell profile if needed
SHELL_CONFIG=""
if [ -f "$HOME/.zshrc" ]; then
    SHELL_CONFIG="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
    SHELL_CONFIG="$HOME/.bashrc"
fi

if [ -n "$SHELL_CONFIG" ]; then
    if ! grep -q "$INSTALL_DIR" "$SHELL_CONFIG" 2>/dev/null; then
        echo "export PATH=\"$INSTALL_DIR:\$PATH\"" >> "$SHELL_CONFIG"
        echo -e "\033[1;32m[OK] Added $INSTALL_DIR to $SHELL_CONFIG\033[0m"
    fi
fi

export PATH="$INSTALL_DIR:$PATH"

# 3. Launch auto-configurator
echo -e "\n\033[1;36mLaunching NaviFS Auto-Configurator...\033[0m"
"$DEST_EXE" setup "$@"
