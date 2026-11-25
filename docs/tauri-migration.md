# Linux desktop migration: Electron to Tauri 2

Tauri reuses the existing React screens, Rust services, PTY channels, and
Hermes gateway. It does not bundle a browser engine or ship a second native
service process. The Linux target uses the distribution's WebKitGTK 4.1.
The desktop session uses D-Bus for single-instance handling.

## Preview and rollback

```bash
npm ci
npm run desktop:build
npm run desktop:tauri:preview:install
hermes-grok-bot-tauri
```

The preview and its updates use a separate release pointer. They do not alter
`hermes-grok-bot`, its desktop entry, or the installed Chromium release. The
hosts share the native connection file and Hermes data already saved on the
host. Close the other desktop host if testing a local gateway. The first Tauri launch copies only known renderer preferences,
drafts, Work session labels, board preferences, and terminal UI state from the
old application origin; it does not import browser tokens or overwrite existing
Tauri values. The previous Electron profile remains untouched.

`apps/desktop/release-host` is still `chromium`. After passing the release
gate, changing it to `tauri` will let old Electron updaters hand off through
their original installer. Tauri will take over the standard launcher and
retain `hermes-grok-bot-electron` as a rollback launcher.

Run `npm run test:tauri:smoke` on a Linux desktop (or an isolated X11 display).
It uses throwaway directories and a local deterministic model. The test-only
`desktop-qa` feature is not part of the preview or production build.

## Release gate

Do not switch the default launcher until all of these pass:

1. A new Tauri install, a preview update and restart, and a Chromium-to-Tauri
   upgrade preserve user data and work in an isolated profile. Verify Chat
   streaming, Work/Explore and Git, terminal I/O, remote authentication and
   reconnect, and the updater. A failed update must leave its active launcher
   usable. The old installed Chromium release remains available for rollback.
2. Measure the shipped desktop processes, not just JavaScript heap. In
   `npm run test:desktop-memory`, Tauri's median idle PSS should be at least
   5% lower and its installed payload at least 50% smaller. This excludes
   system libraries, Hermes itself, and the separate rollback copy.
3. Run the same streaming/scrolling and input fixture at least three times
   per engine on the same desktop. Compare medians of per-run p95 frame
   interval and input-to-frame time. Tauri should not add more than one
   60 Hz frame (~16.7 ms) to either p95, or more than ten percentage points
   to the share of frame intervals over 34 ms. Use the renderer from the same
   revision for both engines. `npm run test:desktop-perf:gate` runs both
   fixtures serially. They measure rendering responsiveness,
   not end-to-end keyboard or model latency.

## Initial evidence (Intel Iris Xe, Wayland, WebKitGTK 2.54.1)

Three isolated launches of each desktop host, on the first-run screen with no
Hermes process, measured these medians:

| App | PSS | RSS (shared pages can be counted more than once) | App payload |
| --- | ---: | ---: | ---: |
| Installed Electron baseline | 331.7 MiB | 723.7 MiB | 283.6 MiB |
| Initial Tauri candidate | 272.2 MiB | 440.2 MiB | 13.5 MiB |

An exploratory run of the heavy streaming fixture gave 44.4 ms frame p95 for
Chromium and 87 ms for the original WebKit layout. Non-overlapping chat chrome
and worker-backed streaming Markdown improved the observed WebKit p95 to
69 ms in a follow-up run. Those single-run numbers are not a release pass.
The default launcher remains on Electron while the responsiveness and upgrade
checks are completed.
