#!/usr/bin/env bash
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
app_binary="$project_dir/apps/desktop/src-tauri/target/release/hermes-grok-bot"
if [[ ! -x "$app_binary" ]]; then
  printf '%s\n' 'Build the desktop app first: npm run desktop:build' >&2
  exit 1
fi
install_root="${XDG_DATA_HOME:-$HOME/.local/share}/hermes-grok-bot"
launcher_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
bin_dir="$HOME/.local/bin"
mkdir -p "$install_root" "$launcher_dir" "$bin_dir"
# Atomic replacement allows an already running version to finish cleanly.
install -m755 "$app_binary" "$install_root/hermes-grok-bot.new"
mv -f "$install_root/hermes-grok-bot.new" "$install_root/hermes-grok-bot"
install -m644 "$project_dir/packages/grok-bot-theme/assets/icons/grok-bot-256.png" "$install_root/icon.png"
ln -sfn "$install_root/hermes-grok-bot" "$bin_dir/hermes-grok-bot"
cat > "$launcher_dir/hermes-grok-bot.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Hermes Grok Bot
Comment=Hermes Agent with the Grok Bot interface
Exec="$install_root/hermes-grok-bot"
Icon=$install_root/icon.png
Terminal=false
Categories=Development;Utility;
StartupWMClass=ai.nousresearch.hermes.grokbot
DESKTOP
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$launcher_dir"
fi
printf 'Installed %s\nLaunch with: hermes-grok-bot\n' "$install_root/hermes-grok-bot"
