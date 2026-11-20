#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
bundle="$project_dir/node_modules/electron/dist"
renderer="$project_dir/apps/web/dist"
native="$project_dir/apps/desktop/src-tauri/target/release/hermes-native"

if [[ ! -x "$bundle/electron" || ! -x "$native" || ! -r "$renderer/index.html" ]]; then
  printf '%s\n' 'Build the Chromium app before installing it: npm run desktop:chromium:build' >&2
  exit 1
fi

install_root="${XDG_DATA_HOME:-$HOME/.local/share}/hermes-grok-bot"
launcher_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
bin_dir="$HOME/.local/bin"
mkdir -p "$install_root/chromium/releases" "$launcher_dir" "$bin_dir"
stage="$(mktemp -d "$install_root/chromium/releases/.stage.XXXXXXXX")"
trap '[[ -z "$stage" ]] || rm -rf -- "$stage"' EXIT
release="$install_root/chromium/releases/${stage##*.stage.}"
cp -a "$bundle/." "$stage/"
mv "$stage/electron" "$stage/hermes-chromium"
mkdir -p "$stage/resources/app"
install -m644 "$project_dir/apps/chromium/"{package.json,main.cjs,preload.cjs,native-client.cjs} "$stage/resources/app/"
cp -a "$renderer" "$stage/resources/renderer"
install -m755 "$native" "$stage/resources/hermes-native"
icon="$project_dir/packages/grok-bot-theme/assets/icons/grok-bot-256.png"
install -m644 "$icon" "$stage/resources/icon.png"
mv -T "$stage" "$release"
stage=""

# The previous binary stays available for the WebKit compatibility launcher.
ln -sfn "$release" "$install_root/chromium/.next"
mv -Tf "$install_root/chromium/.next" "$install_root/chromium/current"
install -m644 "$icon" "$install_root/icon.png"
ln -sfn "$install_root/chromium/current/hermes-chromium" "$bin_dir/hermes-grok-bot"
if [[ -x "$install_root/hermes-grok-bot" ]]; then
  ln -sfn "$install_root/hermes-grok-bot" "$bin_dir/hermes-grok-bot-webkit"
fi
cat > "$launcher_dir/hermes-grok-bot.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Hermes Grok Bot
Comment=Hermes Agent with the Grok Bot interface
Exec="$bin_dir/hermes-grok-bot"
Icon=$install_root/icon.png
Terminal=false
Categories=Development;Utility;
StartupWMClass=hermes-grok-bot
DESKTOP
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$launcher_dir"
fi
printf 'Installed Chromium release %s\n' "$release"
