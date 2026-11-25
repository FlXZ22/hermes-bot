#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_SRC="$ROOT_DIR/apps/rust-gpui/target/release/hermes-grok-bot-rust"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/256x256/apps"

echo "==> Building pure-Rust GPUI Hermes Grok Bot (release)..."
cargo build --release --manifest-path "$ROOT_DIR/apps/rust-gpui/Cargo.toml"

mkdir -p "$BIN_DIR" "$APP_DIR" "$ICON_DIR"
install -m 755 "$BIN_SRC" "$BIN_DIR/hermes-grok-bot-rust"
install -m 644 "$ROOT_DIR/apps/rust-gpui/assets/icons/grok-bot-256.png" "$ICON_DIR/hermes-grok-bot-rust.png"

cat > "$APP_DIR/hermes-grok-bot-rust.desktop" <<EOF
[Desktop Entry]
Name=Hermes Grok Bot (Rust GPUI)
Comment=Native Rust GPU-accelerated Grok Bot desktop client for Hermes Agent
Exec=${BIN_DIR}/hermes-grok-bot-rust
Icon=hermes-grok-bot-rust
Terminal=false
Type=Application
Categories=Development;Utility;Network;
StartupWMClass=hermes-grok-bot
EOF

echo "==> Installed ${BIN_DIR}/hermes-grok-bot-rust"
echo "==> Desktop launcher written to ${APP_DIR}/hermes-grok-bot-rust.desktop"
