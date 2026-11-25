#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
# Older running Electron builds always call this installer after fetching.
# Only the versioned release selector can authorize the eventual cutover.
release_host="$project_dir/apps/desktop/release-host"
if [[ -f "$release_host" ]]; then
  case "$(< "$release_host")" in
    chromium) ;;
    tauri)
      printf '%s\n' 'Building the Tauri release; the previous Electron installation will be retained.'
      (cd "$project_dir" && npm run desktop:build)
      exec bash "$project_dir/scripts/install-linux.sh" --activate
      ;;
    *)
      printf '%s\n' 'Invalid desktop release host; the current installation was not changed.' >&2
      exit 1
      ;;
  esac
fi
bundle="$project_dir/node_modules/electron/dist"
renderer="$project_dir/apps/web/dist"
native="$project_dir/apps/desktop/src-tauri/target/release/hermes-native"

if [[ ! -x "$native" ]]; then
  printf 'Missing native binary: %s\nRun npm run desktop:chromium:build before installing.\n' "$native" >&2
  exit 1
fi
if [[ ! -r "$renderer/index.html" ]]; then
  printf 'Missing renderer build: %s\nRun npm run build before installing.\n' "$renderer/index.html" >&2
  exit 1
fi
# Also run here so an already-installed updater can repair a checkout after
# fetching this version, even if that updater has no runtime preparation step.
node "$project_dir/scripts/prepare-electron.cjs"

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

commit_full="$(git -C "$project_dir" rev-parse HEAD 2>/dev/null || printf 'unknown')"
commit_short="$(git -C "$project_dir" rev-parse --short=12 HEAD 2>/dev/null || printf 'unknown')"
branch_name="$(git -C "$project_dir" rev-parse --abbrev-ref HEAD 2>/dev/null || printf 'main')"
built_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
app_version="$(node -e "console.log(require(process.argv[1]).version || '1.0.0')" "$project_dir/package.json" 2>/dev/null || printf '1.0.0')"
native_commit="${HERMES_NATIVE_COMMIT:-$commit_full}"
cat > "$stage/resources/release-meta.json" <<META
{
  "version": "$app_version",
  "commit": "$commit_full",
  "shortCommit": "$commit_short",
  "branch": "$branch_name",
  "builtAt": "$built_at",
  "nativeCommit": "$native_commit",
  "host": "chromium"
}
META

mv -T "$stage" "$release"
stage=""

# The previous binary stays available for the WebKit compatibility launcher.
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
# Publish the new release only after the required files and launchers exist.
ln -sfn "$release" "$install_root/chromium/.next"
mv -Tf "$install_root/chromium/.next" "$install_root/chromium/current"
if command -v update-desktop-database >/dev/null 2>&1 && ! update-desktop-database "$launcher_dir"; then
  printf '%s\n' 'Warning: desktop menu cache refresh failed; the release is installed.' >&2
fi
printf 'Installed Chromium release %s\n' "$release"
