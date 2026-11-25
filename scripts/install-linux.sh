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
if [[ "${1:-}" == --preview || "${1:-}" == --activate ]]; then
  if [[ "$1" == --preview ]]; then
    host=tauri-preview
    launcher_name=hermes-grok-bot-tauri
    launcher_title='Hermes Grok Bot (Tauri Preview)'
  else
    host=tauri
    launcher_name=hermes-grok-bot
    launcher_title='Hermes Grok Bot'
  fi
  channel="$install_root/$host"
  mkdir -p "$channel/releases"
  stage="$(mktemp -d "$channel/releases/.stage.XXXXXXXX")"
  trap '[[ -z "$stage" ]] || rm -rf -- "$stage"' EXIT
  release="$channel/releases/${stage##*.stage.}"
  install -m755 "$app_binary" "$stage/hermes-grok-bot"
  install -m644 "$project_dir/packages/grok-bot-theme/assets/icons/grok-bot-256.png" "$stage/icon.png"
  commit="$(git -C "$project_dir" rev-parse HEAD 2>/dev/null || printf unknown)"
  branch="$(git -C "$project_dir" rev-parse --abbrev-ref HEAD 2>/dev/null || printf main)"
  version="$(node -p 'require(process.argv[1]).version' "$project_dir/package.json")"
  node -e '
    const [version, commit, branch, host] = process.argv.slice(1);
    console.log(JSON.stringify({
      version, commit, branch, shortCommit: commit.slice(0, 12),
      nativeCommit: commit, host, builtAt: new Date().toISOString(),
    }));
  ' "$version" "$commit" "$branch" "$host" > "$stage/release-meta.json"
  # Activation is explicit. Preserve, but don't copy, the last Chromium release.
  if [[ "$host" == tauri && -x "$install_root/chromium/current/hermes-chromium" ]]; then
    ln -sfn "$install_root/chromium/current/hermes-chromium" "$bin_dir/hermes-grok-bot-electron"
  fi
  cat > "$launcher_dir/$launcher_name.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=$launcher_title
Comment=Hermes Agent desktop using the system webview
Exec="$bin_dir/$launcher_name"
Icon=$channel/current/icon.png
Terminal=false
Categories=Development;Utility;
StartupWMClass=ai.nousresearch.hermes.grokbot
DESKTOP
  mv -T "$stage" "$release"
  stage=""
  ln -s "$release" "$channel/.next.${release##*/}"
  mv -Tf "$channel/.next.${release##*/}" "$channel/current"
  ln -s "$channel/current/hermes-grok-bot" "$bin_dir/.$launcher_name.${release##*/}.next"
  mv -Tf "$bin_dir/.$launcher_name.${release##*/}.next" "$bin_dir/$launcher_name"
  if command -v update-desktop-database >/dev/null 2>&1 && ! update-desktop-database "$launcher_dir"; then
    printf '%s\n' 'Warning: desktop menu cache refresh failed; the release is installed.' >&2
  fi
  printf 'Installed Tauri release %s\nLaunch with: %s\n' "$release" "$launcher_name"
  exit 0
elif [[ $# -gt 0 ]]; then
  printf '%s\n' 'Usage: scripts/install-linux.sh [--preview | --activate]' >&2
  exit 2
fi
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
