#!/usr/bin/env bash
# NaviFS Universal 1-Click Plug & Play Installer for macOS and Linux.
# Installs navifs to $HOME/.navifs/bin, updates PATH, and auto-configures AI agents.

set -e

echo -e "\033[1;36m========================================================\033[0m"
echo -e "\033[1;36m       NaviFS Universal Plug & Play Installer           \033[0m"
echo -e "\033[1;36m========================================================\033[0m"

NAVI_DIR="$HOME/.navifs/bin"
mkdir -p "$NAVI_DIR"
DEST_EXE="$NAVI_DIR/navifs"

# Copy local build or build if missing
if [ -f "./target/release/navifs" ]; then
    cp "./target/release/navifs" "$DEST_EXE"
elif command -v cargo &> /dev/null; then
    echo "Building release binary..."
    cargo build --release -p navifs-daemon
    cp "./target/release/navifs" "$DEST_EXE"
fi

chmod +x "$DEST_EXE"
echo -e "\033[1;32m[OK] Installed binary to: $DEST_EXE\033[0m"

# Add to PATH in bashrc / zshrc
SHELL_RC=""
if [ -f "$HOME/.zshrc" ]; then
    SHELL_RC="$HOME/.zshrc"
elif [ -f "$HOME/.bashrc" ]; then
    SHELL_RC="$HOME/.bashrc"
fi

if [ -n "$SHELL_RC" ]; then
    if ! grep -q "$NAVI_DIR" "$SHELL_RC"; then
        echo "export PATH=\"$NAVI_DIR:\$PATH\"" >> "$SHELL_RC"
        echo -e "\033[1;32m[OK] Added $NAVI_DIR to $SHELL_RC\033[0m"
    fi
fi

export PATH="$NAVI_DIR:$PATH"

# Run setup
echo -e "\n\033[1;36mLaunching NaviFS Auto-Configurator...\033[0m"
"$DEST_EXE" setup "$@"
