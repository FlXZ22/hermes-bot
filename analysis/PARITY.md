# Scope audit — Grok Bot UI and Hermes desktop

The objective remains the full requested Linux application, with Grok Bot styling and Hermes desktop functionality. This is an implementation/evidence inventory, not a completion certificate. A green unit test for one protocol does not prove parity of a whole feature family.

Authoritative references inspected:

- Supplied Grok Bot 1.2.3 package under `analysis/grok-bot-asar`, retained REA analyses, and `THEME_REVERSE_ENGINEERING.md`.
- Installed Hermes 0.21.0 source under `~/.hermes/hermes-agent`, especially `apps/desktop/src/app/routes.ts`, `app/settings/`, `app/right-sidebar/terminal/`, `electron/terminal-ipc.ts`, `electron/ssh-connection.ts`, and `hermes_cli/web_server.py`.
- Current app views and state in `apps/web/src/App.tsx`, `state/appStore.tsx`, SDK source under `packages/hermes-client/src`, and native commands under `apps/desktop/src-tauri/src`.

| Requirement / feature family | Current authoritative evidence | Status / remaining work |
| --- | --- | --- |
| Linux app with Rust application backend | Chromium desktop host (`apps/chromium`) + `hermes-native` Rust subprocess and Tauri WebKitGTK binary; Rust supervisor/HTTP/files/Git/PTY/credential commands; native build, smoke test, and diagnostic | Working covered paths on both Chromium and WebKitGTK hosts. Agent inference/tools still use Hermes's external Python runtime. |
| Exact Grok typography, glyphs, marks, semantic colors | Extracted XVF/XVF Mono/CursorIcons16 assets, SVG paths and tokens; REA evidence | Assets used. All authenticated screens and responsive states have not been compared visually at runtime. Exact overall parity is unproven. |
| Welcome and server connection | Native PKCE listener tests; real gateway auth/refresh/restart tests; browser setup/reload/error checks | Working covered flow. Full multi-connection registry, managed SSH dashboard tunnels and account provisioning are not reproduced. |
| Chat, sessions, boards, model selection | Real isolated gateway/model E2E covers streaming, durable sessions, rename, resume/branch, attachments, archive/pin, board/task status, cron | Working covered core. Audit advanced message editing, retries, search/export/bulk operations, session comparison/context attachment and all tool-card types against Hermes source. |
| Native and SSH terminals | Real PTY and disposable OpenSSH tests; xterm renderer; terminal tabs and native commands | Implemented local/SSH interactive shells, resize, Ctrl+C, reattach, output flow control and saved history. Native rendered interaction still needs direct UI verification. Agent background-process mirrors and full context-selection/link workflows remain. |
| Workspace files and Git | Local Rust reads; remote file API; local/SSH Git status/diff/worktrees | Covered reads. Full editor, diff selection/staging, worktree management and artifact integration remain. |
| Skills / toolsets / MCP | SkillsView and Hermes client APIs; installed runtime read checks | Core controls present. Full provider/tool configuration and every install/update/error workflow need parity verification. |
| Cron / automations | CronView; E2E create/pause/delete/schedule normalization | Covered core. Advanced scheduler fields, history, concurrency and delivery controls require comparison. |
| Memory / knowledge | SOUL profile API, local memory files, learning graph | Remote direct memory-file editing and provider configuration panels incomplete. |
| Messaging / webhooks | GatewayView and SDK methods | Core controls present. Full provider setup, sessions, delivery/testing and routing flows need verification. |
| Usage | AnalyticsView and usage endpoints | Basic charts present. Full command-center analytics and billing/token breakdowns not equivalent. |
| Profiles / agents | Sidebar profiles, Settings profile creation/switch, SOUL/avatar | Full profile lifecycle and Hermes `/agents` page not implemented. |
| Artifacts | Hermes defines `/artifacts`; no equivalent AppView | Missing dedicated artifact catalog/viewer workflows. |
| Starmap | Hermes defines `/starmap`; no equivalent AppView | Missing dedicated Starmap page. |
| Command center | Hermes defines `/command-center`; no equivalent AppView | Missing dedicated command-center page. |
| Voice / audio / video / calls | Hermes source has corresponding providers/endpoints; current app lacks complete flows | Incomplete. |
| Settings and extensibility | Existing general/connections/keys/profiles/config/logs tabs | Inference-provider OAuth, custom endpoints/fallback models, tool/browser/computer-use/memory configuration, notification/keybinding/session administration, plugin lifecycle/contributed routes and update management remain incomplete. |

Work should continue against these gaps; do not mark the full goal complete from the existing test count or successful installation. `VALIDATION.md` records the narrower checks that actually ran.
