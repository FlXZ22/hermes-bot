# Build validation — 19 November 2026

Installed and launched `/home/metis/.local/share/hermes-grok-bot/hermes-grok-bot`, with a command symlink at `/home/metis/.local/bin/hermes-grok-bot` and an application launcher entry. The installed file was compared with the release build and matched. The launched desktop process created its WebKit processes and its own Hermes child.

Executed successfully:

- `npm run typecheck`
- `npm test`: 9 client regression tests
- `npm run test:rust`: 3 Rust tests
- `npm run test:integration`: installed Hermes read APIs, authenticated WebSocket, session creation, inference controls, heartbeat
- `npm run test:e2e`: real Hermes gateway with isolated home and deterministic local model; streaming, transcript persistence, durable session IDs, rename, branch, resume, attachment staging, pin, archive/restore, board/task creation and updates, cron creation/pause/deletion
- `npm run desktop:build`
- Release binary `--diagnose-backend`: authenticated Rust HTTP succeeded; observed 966 ms backend startup and 2 ms local metadata loading on this machine
- Installer and PKGBUILD shell syntax checks
- Linked shared-library inspection: no missing libraries reported

Browser checks verified loaded session history, installed skills/toolsets, model/provider choices, both theme tokens in use, and draft isolation while switching existing sessions. The draft-check text was cleared afterward; no test prompt was sent to a paid model. `verification-chat.jpg` records the browser interface. Native window rendering has not been independently inspected through a native UI automation surface.

One existing saved session reported that its inference provider is unconfigured when resumed. A configured provider/model must be selected for actual inference. The isolated test configuration completed real gateway inference successfully.

These checks establish working covered workflows. They do not establish complete Hermes feature parity, complete Grok Bot screen coverage, or pixel-for-pixel visual equivalence. The current Rust host reuses the installed Python Hermes agent runtime. See README.md for the outstanding scope.

## Welcome and remote setup continuation

Added first-run welcome, local/server selection, native browser password/SSO sign-in using PKCE, token connection options, SSH tunnel guidance, authenticated REST plus live WebSocket checks before saving, saved remote restoration, and credential refresh. Native credentials use an atomic private config file (0600; directory 0700), not an encrypted keychain. Remote connections no longer start or replace themselves with a local Hermes process, read local transcripts/memory/workspace files, or write remote board preferences into the local Hermes home. Drafts and UI board metadata are scoped by target.

Executed successfully for this continuation:

- TypeScript checks; 14 client regression tests; 6 Rust tests, including the real loopback callback listener, PKCE exchange, cancellation, private storage permissions, and atomic persistence.
- `npm run test:auth`: real Hermes 0.21.0 password provider, native authorization/code exchange, rejection of an invalid bearer token, authenticated REST, ticket-authenticated WebSocket heartbeat, refresh, saved credential restoration, and reconnect after a real gateway restart. Disposable credentials and an isolated home; no user's configuration changed.
- `npm run test:e2e`: real streaming inference and the existing covered session/attachment/board/schedule workflows still pass against the isolated deterministic model.
- Browser UI with an isolated real gateway: first-run welcome; incorrect token rejection; valid remote setup reaches “You’re connected”; reload restores the named remote instead of the development local gateway; a failed server switch preserves that saved target and allows return to the working app. Light/dark welcome and server-form screenshots saved alongside this report. The browser preview uses token setup; native browser opening was covered by protocol/listener tests rather than a visual native-app automation test.

No actual remote server URL or user sign-in was supplied. The app is prepared for that setup, but these tests do not assert access to the user's server or verify its inference-provider configuration. Broad feature/pixel parity limits above remain applicable.

Final native release: rebuilt and installed; source timestamps precede the release binary; installed file matches that binary; launcher present; shared-library inspection reports no missing dependencies. The release diagnostic again passed authenticated Rust HTTP (observed 971 ms gateway boot, 3 ms local metadata read). Launched the installed desktop build for first-run setup; it stays running with WebKit subprocesses and no local Hermes child before a local connection is selected. A GTK titlebar warning is emitted on startup; no crash was observed.

## Interactive Rust terminal and SSH workspace continuation

Replaced the bounded command form with real Rust-owned PTYs and an xterm.js renderer. Added multiple renameable terminal tabs, keyboard input/Ctrl+C, resize, clipboard shortcuts, local cwd tracking, live-shell reattachment after renderer reload, and scoped terminal metadata/scrollback restoration after app relaunch. Panels stay mounted while hidden or while navigating to other app views. Terminal code loads on first opening the panel. Switching connection/SSH scope closes that scope's terminals; app exit stops owned PTY children.

Server connections can optionally store SSH host/user/port/key/config settings. Both interactive SSH terminals and remote Git status use that target; there is no local-shell fallback for a remote connection without SSH. SSH config defaults are preserved when user/port are blank, and host-key verification is retained. Git parsing now handles NUL-delimited paths/rename pairs and recognizes a repository before its first commit.

Validation:

- TypeScript checks and 15 client regression tests passed, including SSH defaults, stored targets and option-injection rejection.
- 11 Rust tests passed. Actual local PTYs verified terminal detection, shell environment/directory persistence, resize, Ctrl+C, reattachment to the same PID/output and process reaping.
- A disposable loopback OpenSSH server with generated keys, isolated config and known-hosts files verified a real interactive SSH shell, remote resizing and Git status. The workspace name included spaces, Unicode, an apostrophe and literal `$(...)` text. No user SSH files, server accounts or credentials were changed.
- A 1.5 MB output stress test verified the 256 KB acknowledgement window, rejection of stale-renderer acknowledgements, resumed delivery and the 1 MB native replay-buffer bound.
- Browser preview verified lazy terminal loading and panel/navigation persistence. The preview deliberately has no native shell transport; rendered native xterm keyboard interaction has not been verified through a native UI automation surface.

Source references: installed Hermes `electron/terminal-ipc.ts` and `electron/ssh-connection.ts`; [Tauri Channel API](https://docs.rs/tauri/latest/tauri/ipc/struct.Channel.html); [portable-pty API](https://docs.rs/portable-pty/0.9.0/portable_pty/); [xterm addons](https://xtermjs.org/docs/guides/using-addons/).

The full-scope comparison is now tracked in `PARITY.md`. This continuation does not establish all Hermes feature or Grok screen parity.

The terminal continuation release was rebuilt and installed. The installed binary matches the release file, all relevant source files precede its timestamp, and no missing linked libraries were reported. The already-running desktop was not interrupted for the upgrade. The browser panel was checked at the configured 1360×860 desktop window size (`verification-terminal-panel.png`); its screenshot shows the intentional browser-only fallback, not a live native PTY. Relaunch the Linux binary for the new terminal transport.

## Responsive chat release

The retained REA design summary and Grok Bot theme report were used as visual reference for the existing Sand palette, typography, bot marks, composer and transcript. The local REA clone and original renderer are absent from this checkout, so the retained static evidence is the basis for this pass.

Run `npm run test:desktop-perf` from a Linux desktop with WebKitGTK development headers and a C compiler. It builds the production renderer, opens a temporary WebKitGTK window with an ephemeral storage context, and uses fixture data and a simulated gateway. The workload combines a large transcript, 600 formatted deltas, synthetic text input, and repeated programmatic scrolling. The resulting input-to-frame number is a rendering latency proxy, and the session switch number measures two frames after a cached selection. Model latency and compositor-visible input are outside this benchmark.

The frontend regression check covers virtual scrolling, load-older pagination, worker Markdown, image opt-in, profile switching, and stream ordering. The local Hermes end-to-end check uses an isolated home and deterministic model to exercise the real gateway and session database.

The repeated production fixture on this machine recorded these results, in milliseconds:

| Engine and build | 95th-percentile frame interval | 95th-percentile input to next frame | Cached switch to second frame |
| --- | ---: | ---: | ---: |
| WebKitGTK, previous build | 114 | 76 | 45 |
| WebKitGTK, updated build | 71 | 18 | 43 |
| Headed Chromium on Wayland, updated build | 11.2 | 18 | 22 |

The updated GTK fixture still misses the scroll frame budget under joint streaming and scrolling. A separate run that allowed DMA-BUF reduced the number of slow frames but did not improve the 95th percentile, so it does not establish a safe performance fix.

## Chromium desktop host and private Rust bridge (`hermes-native`)

Added a sandboxed Chromium desktop host (`apps/chromium`) paired with a standalone Rust subprocess binary (`hermes-native`, implemented in `apps/desktop/src-tauri/src/native_ipc.rs` and `src/bin/hermes-native.rs`). Both the Chromium host and the Tauri WebKitGTK host share the same Rust implementation for `HermesSupervisor`, authenticated HTTP requests, PKCE remote login, atomic private connection storage, SQLite session/bootstrap reads, workspace/Git commands, and bounded-window interactive PTY/SSH terminals (`EventSink`).

Security and isolation checks (`npm run test:chromium:smoke`):

- Renderer runs with `sandbox: true`, `contextIsolation: true`, `nodeIntegration: false`, custom `hermes-desktop://app` scheme, strict Content-Security-Policy, and main-frame sender validation.
- Unknown commands and `window.open` child windows are rejected; external links open only through the system browser on explicit navigation dispositions.
- End-to-end smoke test exercises first-run welcome, local Rust gateway startup, deterministic local model streaming, worker Markdown rendering inside the visible transcript viewport, private native credential persistence (token omitted from persisted local connection), and interactive PTY start/write/output/acknowledgement/close through the private bridge.
- `npm test` (20 JS tests) and `npm run test:rust` (13 Rust tests) pass, including `native_ipc` frame boundary/validation tests and `NativeClient` subprocess protocol tests.
- Rebuilt and installed both the WebKitGTK release (`npm run desktop:build && npm run desktop:install`) and the Chromium release (`npm run desktop:chromium:build && npm run desktop:chromium:install`), providing `~/.local/bin/hermes-grok-bot` (Chromium) and `~/.local/bin/hermes-grok-bot-webkit` (WebKitGTK).
