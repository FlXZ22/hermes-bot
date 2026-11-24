# Hermes Grok Bot for Linux

A Linux desktop application with a Rust backend and a React interface based on the Grok Bot Linux package. Two desktop hosts share the same Rust service layer (`HermesSupervisor`, authenticated HTTP proxy, PKCE remote login, local SQLite reads, workspace/Git inspection, and interactive PTY/SSH terminals):

- **Chromium desktop host** (`apps/chromium` + `hermes-native` Rust subprocess): sandboxed, context-isolated Chromium window served over `hermes-desktop://app` for low-latency streaming and scrolling on Wayland/X11.
- **Tauri 2 WebKitGTK host** (`apps/desktop`): single-binary system WebKitGTK build retained as `hermes-grok-bot-webkit`.

The installed Hermes Agent supplies inference, tools, memory, profiles, and scheduling.

## Run the Linux app

On first launch, choose **This computer** or **Connect a server**. A server connection does not require a local Hermes installation. For **This computer**, Hermes Agent must already be installed and configured. The host looks for its managed Python environment in `~/.hermes/hermes-agent/venv/bin/python`, then the Hermes executable. Set `HERMES_HOME` to use another Hermes installation/home. No model credentials are bundled.

```bash
npm ci
npm run desktop:build
npm run desktop:install
npm run desktop:chromium:build
npm run desktop:chromium:install
hermes-grok-bot
```

The installer writes only to `~/.local/share/hermes-grok-bot`, `~/.local/share/applications`, and `~/.local/bin`. It does not require root. `hermes-grok-bot` launches the Chromium release, while `hermes-grok-bot-webkit` launches the WebKitGTK binary. The app appears as **Hermes Grok Bot** in the application launcher. An optional local Arch package recipe is in `packaging/arch/PKGBUILD`.

Build prerequisites on Arch are Node.js, npm, Rust, pkgconf, a C/C++ toolchain, GTK 3, WebKitGTK 4.1, and libsoup 3.

For desktop development, run `npm run desktop:chromium` (after `npm run desktop:chromium:build`) or `npm run desktop:dev` (Tauri/WebKitGTK). For the browser preview connected to the installed Hermes backend, run `npm run dev:connected`, then open `http://127.0.0.1:5173`. That command starts an owned gateway and a same-origin development proxy, and stops them on Ctrl+C. `npm run dev` starts the frontend alone.

## App updates and recovery

In the desktop app, use **Settings → Updates** with a clean local checkout and
update branch `main`. **Update Now** fetches that branch, fast-forwards the
checkout, builds the UI, rebuilds Rust when needed, and installs the app.
**Restart App Now** launches the installed release. **Install Local Build**
previews the current checkout without switching Git branches.

If the native service exits during an update, the stopped process cannot finish
its own repair. Close the app and run these commands from the repository after
committing or stashing local changes:

```bash
git switch main
git pull --ff-only origin main
npm ci
npm run build
cargo build --locked --manifest-path apps/desktop/src-tauri/Cargo.toml --release --no-default-features --bin hermes-native --jobs 2
bash scripts/install-chromium.sh
hermes-grok-bot
```

This recovery installs the Chromium host. It needs the same Node/npm, Rust and
Linux runtime prerequisites as the normal build. If the service exits again,
launch `hermes-grok-bot` from a terminal and retain the terminal output and the
reported exit code or signal; the old generic “service closed” message does not
identify the cause. Build subprocesses have isolated stdin, and update checks
remain responsive during installation. Native update builds use two jobs to
limit peak resource use.

## Chat

The transcript virtualizes variable-height messages (`react-virtuoso`), isolates per-message subscriptions in `ChatStore`, parses large Markdown responses and syntax highlighting in a Web Worker, paginates older session history on demand, and gates external images behind explicit opt-in.
Scrolling up pauses following; **Jump to latest** resumes it. Responses support
GFM tables, task lists, nested lists, and highlighted code with copy controls.
Thinking and tool activity start collapsed, including saved tool messages.

Use **Hermes Bots → +** to create a bot with separate conversations and memory,
then click a bot to switch. The usage strip above the input shows context tokens
and the gateway's reported limit; unavailable figures are labeled explicitly.
The face animation is adapted from [mikeg0/grokbot-face](https://github.com/mikeg0/grokbot-face),
loads separately, pauses during scrolling, and respects reduced motion.

For the chat regression checks, start `npm run dev`, then run
`npm run test:chat-ui` in another terminal. The checks use Playwright, deterministic
HTTP/WebSocket fixtures and a 1,000-message transcript. They do not call a live model.

## Work

Select a Hermes profile, open **Work** in the sidebar, and enter the absolute
path to a project folder on the Hermes host. Work sessions appear in that
profile’s Work list. Open **Chat** to return to its conversation list.

Hermes uses the selected folder for coding tools, commands, and tests. Use
`/plan` to draft an implementation plan, `/goal <objective>` for a persistent
goal, and `/diff` or **Git** to review changes. The activity panel shows
subagents and lets you steer or stop them. **Checkpoints** previews and
restores snapshots in profiles with Hermes checkpoints enabled. Type `@` to
reference project files. While the agent runs, use **Queue** for a follow-up
or **Steer turn** for an instruction during the current turn. Use `/export`
to download the saved Work conversation as JSON. From **New Work**, choose
a workspace path and select a **Session export** to import conversation
text from OpenCode v1, OpenCode v2, or a Hermes Work JSON file into the
selected profile. The session is ready for a new prompt after import.

Use **Explore** or `/explore <question>` for a read-only answer. It sees a
bounded list of top-level entries, or excerpts from up to four workspace files
you name, for example `/explore How is startup wired? @file:src/main.ts`.
Quote paths with spaces. Explore sends this context to Hermes's one-shot model endpoint, not to
the coding agent. It cannot run shell commands, edit files, delegate work, or
search for more files. Answers appear in the current view but are not saved to
the Hermes transcript. A gateway without `llm.oneshot` shows an error rather
than starting a regular turn.

Use `/oc` to list OpenCode commands in the active workspace and
`/oc/<command> <arguments>` to run one with Hermes. Work reads `.opencode/commands`,
the legacy `.opencode/command` directory, and the workspace's `opencode.json(c)`
files. Templates support `$ARGUMENTS` and positional placeholders. Shell blocks
use Hermes's terminal tool and its permissions. Commands use the active Work
session's model and agent.

The Work Git panel supports stage, unstage, commit, push, switching branches,
and creating worktrees. Browser connections and desktop connections to a server
without SSH use the Hermes Git API. Its commit action stages all changes when
the index is empty, and it creates worktrees under the repository’s `.worktrees`
directory. The desktop Git panel also offers **Last turn**, which compares tracked
files against their state when the request was submitted. **Create pull request**
uses GitHub CLI on the Hermes host after you confirm the push. Desktop Work
sessions have their own terminal tabs.

Run `npm run test:work-ui` with the development server running to exercise
profile separation, workspace validation, Explore, goal controls, and Git
review. For both browser checks, install `@playwright/test` locally
(`npm install --no-save @playwright/test`, then `npx playwright install chromium`)
or set `PLAYWRIGHT_MODULE` to an existing installation.
See the [OpenCode compatibility audit](docs/work-opencode-compatibility.md)
for available Hermes features and remaining Work integrations.

## Connect your server

Choose **Get started → Connect a server**, enter your Hermes dashboard URL (including any reverse-proxy path prefix), and give it a name. **Sign in with browser** opens your server’s password or SSO login in the system browser. The app verifies authenticated API access and a live WebSocket before saving the connection. It restores that server on relaunch and refreshes native sign-in credentials before expiry. Access tokens and session tokens are also supported.

Native connection credentials are stored in `~/.config/hermes-grok-bot/connection.json` (or under `XDG_CONFIG_HOME`), with file permissions `0600` and directory permissions `0700`. This is a private file, not an encrypted keychain. Legacy WebKit connection credentials are migrated after a successful connection. A failed connection attempt does not replace the saved working connection. Use **Settings → Connections → Manage connection** to switch targets or sign in again.

For an SSH tunnel, keep the server bound to loopback and forward its dashboard port:

```bash
ssh -N -L 9119:127.0.0.1:9119 user@your-server
```

Leave the SSH session open; connect to `http://127.0.0.1:9119` using **Session token** and the server’s `HERMES_DASHBOARD_SESSION_TOKEN`. For a public dashboard, configure Hermes authentication and HTTPS on the server first. Browser sign-in requires a server with the native authorization endpoints (validated against Hermes 0.21.0). A plain OpenAI API endpoint does not provide Hermes sessions, boards, or tools and is not offered in the connection setup.

Remote boards, assignments, avatars, and drafts are scoped by server. Remote connections read files through the server API. Add optional SSH access under **Settings → Connections** (or during server setup) to use interactive server terminals and remote Git status. SSH config aliases, usernames, ports, identity files, and alternate SSH config files are supported; blank port/user fields honor your SSH config. Host verification stays enabled. Interactive SSH can ask for a password in the terminal; Git refresh requires a working key or SSH agent. SOUL editing uses the remote profile API; direct MEMORY.md / USER.md editing currently requires a local connection. No local Hermes process starts until **This computer** is selected.

## Implemented workflows

- First-run welcome, local/server setup, native browser sign-in, validated connections, saved target restoration, and credential refresh.
- Streaming chat, reasoning, tool activity, approvals, clarification cards, interruption, slash commands, resume, branch, compression, renaming, pinning, and archive/restore.
- Per-session drafts, image/PDF/file attachments, model/provider selection, reasoning effort, fast mode, and session approval controls.
- Session boards with workspace directories; Kanban task creation, status changes, model overrides, dispatch, and session launch.
- Profiles with configurable Grok Bot marks, shapes, and colors.
- Installed skills, Skills Hub, toolsets, and MCP server controls.
- Cron schedules, memory/personality editors, learning graph, messaging gateway, webhooks, usage, configuration, API keys, and logs.
- A native file browser, text previews, local/SSH Git changes, and interactive Rust PTY terminals with multiple tabs, renaming, resize, Ctrl+C, clipboard shortcuts, live-shell reattachment, and saved scrollback. Fast output uses acknowledgements and a bounded output window to keep the renderer responsive. The terminal renderer loads on first use.
- Sand Light and Sand Dark themes using the extracted XVF and XVF Mono fonts and CursorIcons16 glyphs.

For local connections, the native host loads existing sessions and capability metadata from the local Hermes home after the gateway handshake. API capability requests populate independently, so model discovery does not block the sidebar.

## Data and behavior

Hermes keeps its normal data in `~/.hermes` (or `HERMES_HOME`). UI board assignments and avatar preferences use `grok_bot_boards.json`; browser storage keeps appearance preferences, target-scoped drafts, and terminal tab metadata with up to 200 lines / 48 KB of scrollback per tab. Named-profile memory and transcript reads resolve within that profile's home. A model selection changes the current session; changing the Hermes global default requires the explicit picker option.

The host owns its spawned gateway and PTY children and terminates them on application exit. Terminals stay alive while switching app panels or hiding the inspector. Renderer reload reattaches to the live shell; reopening the application restores terminal metadata/history with a new shell when you open the terminal panel. Other Hermes services are not stopped. The browser development proxy is bound to loopback. The production app uses Rust HTTP requests and direct authenticated Hermes WebSockets.

## Validation

```bash
npm run typecheck
npm test
npm run test:rust
npm run test:integration
npm run test:e2e
npm run test:auth
npm run test:chromium:smoke
npm run test:chromium:perf
npm run test:desktop-perf
npm run desktop:diagnose
```

`test:integration` checks the installed gateway's read APIs and JSON-RPC controls without submitting prompts. `test:e2e` runs the real Hermes gateway with a temporary home and a local deterministic model. It checks streaming, persisted transcripts, session operations, attachments, boards, and schedules without paid inference or changes to the user's configuration.

`test:auth` exercises the real password provider, native PKCE exchange, credential refresh, authenticated REST, WebSocket tickets, and reconnect after server restart with disposable credentials and an isolated home. Rust tests exercise the actual loopback callback listener, PKCE verifier exchange, cancellation, and private connection storage. Terminal tests launch real PTYs and a disposable loopback OpenSSH server with isolated keys/config/known-hosts files, checking shell state, resize, interrupt, reattachment, cleanup, remote Git, and bounded output flow control. The SSH fixture requires the installed `sshd` and `ssh-keygen` executables; no user SSH files are changed.

`desktop:diagnose` uses the actual Rust supervisor and HTTP adapter without requiring a display. It prints startup timing and record counts without credentials or transcript contents.

## Reference evidence and current limits

The reverse-engineering record is in `analysis/THEME_REVERSE_ENGINEERING.md`, with retained REA evidence from [morluto/rea](https://github.com/morluto/rea). The design sources are the supplied Grok Bot 1.2.3 Debian package and [x.ai/bot](https://x.ai/bot). Hermes behavior was checked against the installed 0.21.0 source/runtime and the [official desktop page](https://hermes-agent.nousresearch.com/desktop).

The feature comparison is tracked in `analysis/PARITY.md`. This is a working reconstruction, not a certification of pixel-for-pixel parity. Authenticated Grok Bot screens have not all been compared at runtime. Full Hermes parity is also unfinished: arbitrary desktop plugin panels, voice/video/call features, cloud account creation and inference-provider OAuth setup flows, advanced terminal integration such as agent-process mirrors, and every advanced administration surface are not reproduced. Existing Hermes credentials work through the backend; API-key configuration is available in this app. The Rust host reuses the existing Python Hermes agent rather than reimplementing its inference/tool runtime in Rust.
