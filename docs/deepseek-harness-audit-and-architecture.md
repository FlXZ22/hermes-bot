# DeepSeek Harness & Full Coding Agent Upgrade — Audit, Feature Matrix & Architecture Plan

**Date:** November 26, 2026  
**Target Repository:** `https://github.com/FlXZ22/hermes-bot.git` (local checkout `/home/metis/Projects/grok-bot-ui`, branch `rust-gpui-rewrite`, base commit `4ebbd10c9e23a30628fb791e83e38e3df0a17bd6`)  
**Target Crate:** `apps/rust-gpui` (`hermes-grok-bot-gpui`, binary `hermes-grok-bot-rust`)  
**Installed Hermes Agent Runtime:** `v0.21.0 (2026.8.31)` · Git commit `29112bef099274229cadff79cdff7bf7b99c4b77` (`https://github.com/NousResearch/hermes-agent.git`, local `/home/metis/.hermes/hermes-agent`, Python 3.11.17)  
**Pinned DeepSeek Harness Reference:** `https://github.com/deepseek-ai/deepseek-harness` · Git commit **`4b399c39a6d1479cd84ca591b4d7c88e24d38a96`** (Developer Preview)

---

## 1. Licenses & Attribution

| Source | License | Attribution & Constraints |
|---|---|---|
| **Hermes Agent** (`NousResearch/hermes-agent`) | **MIT License** (`Copyright (c) 2025 Nous Research`) | Permissive; retain copyright notice in `THIRD_PARTY_NOTICES.md`. |
| **DeepSeek Harness** (`deepseek-ai/deepseek-harness`) | **MIT License** (`Copyright (c) 2026 DeepSeek`) | Permissive; retain copyright notice in `THIRD_PARTY_NOTICES.md`. Per `BRAND_GUIDELINES.md`, `"DeepSeek Harness"` is a trademark — use only truthfully descriptively (never in our app's product name). Avoid non-permissive optional packages (`libreoffice-kit` MPL-2.0, `claude-agent-sdk` proprietary). |
| **Grok Bot Face & Design System** (`face/`, `analysis/`) | Local authoritative assets & REA specifications | Reused verbatim via `apps/rust-gpui/src/face/` and `apps/rust-gpui/src/theme/`. Never replaced or rebuilt. |

---

## 2. Rust vs. External Runtime Boundary

Hermes Agent runs as an external local or remote process (`hermes-agent` Python 3.11 FastAPI/Starlette + JSON-RPC 2.0 WebSocket server on `/api/ws` and REST API on `/api/*`), managed locally by `HermesBackendSupervisor` (`apps/rust-gpui/src/backend/hermes_backend.rs`) or reached remotely over HTTP(S)/WS + optional SSH (`apps/rust-gpui/src/backend/ssh.rs`).

| Layer | Rust Application Responsibilities (`apps/rust-gpui`) | External Hermes Runtime Responsibilities (`hermes-agent v0.21.0`) |
|---|---|---|
| **UI & Rendering** | Native GPU rendering via GPUI (`gpui-pre 0.3.14`), Grok Bot dark/light theme tokens, `face/` animation engine (120 FPS target), virtualized transcript/trajectory, streaming syntax-highlighted Markdown, copy controls, modals, inspector panes. | None (pure headless backend). |
| **Durable Harness Store (`harness.db`)** | Versioned SQLite store (`~/.config/hermes-grok-bot/harness.db`, schema v1) persisting: `conversation_modes` (`Chat \| Work` lock + project binding), local `projects` cache, deduplicated append-only `trajectory_events`, `goals` + `goal_events`, `subagent_runs`, `ptc_records`, and runtime metrics snapshots. | Persists core chat `sessions`/`messages` in `$HERMES_HOME/state.db` (schema v26) and server projects in `$HERMES_HOME/projects.db`. |
| **Runtime Adapter** | Typed event ingestion (`hermes/ws.rs`, `hermes/client.rs`), framing/deduplication, `tool_id` & `duration_s` normalization, `subagent.*` / `session.usage` / `approval.*` / `clarify.*` bridging, `commands.catalog` / `command.dispatch` / `slash.exec` routing. | Executes LLM requests, streaming deltas, built-in tools (`read_file`, `write_file`, `patch`, `search_files`, `terminal`, `execute_code`, `delegate_task`, `todo`, `clarify`, MCP), approvals, and `/api/fs/*` server browsing. |
| **Project & Server FS Service** | Server filesystem browser modal with path canonicalization, home-directory scope indicator, symlink/permission error display, local/SSH/Hermes `/api/fs/list` unification, and project CRUD (`projects.*` WS RPC + `harness.db`). | Provides `/api/fs/list`, `/api/fs/read-text`, `/api/fs/default-cwd`, `/api/fs/git-root` and `projects.*` RPCs on the agent host. |
| **Persistent Goal Worker** | Independent Tokio background actor (`harness/goal_worker.rs`) holding CAS state machine, round/time/token caps, pause/resume/cancel/block/complete transitions, and reconnect recovery (`disarmed` on cold restart so no duplicate unattended runs). Coordinates Hermes turns via WS `prompt.submit` / `/goal`. | Runs individual agent turns and native `/goal` Ralph-loop continuation while connected. Closing the desktop app stops the local managed `HermesBackendSupervisor` child process (remote servers stay alive; reconnecting restores persisted goal state in `disarmed` mode until resumed). |
| **PTC Orchestration & Inspection** | Rust PTC inspector & parity parser (`harness/ptc.rs`) supporting both Hermes native `execute_code` (Python persistent kernel) and DeepSeek-format `run_code`/`cua_run_code` (TypeScript/Python) plus local Rust-coordinated inspection, verbatim copy, original JSON toggle, schema-metadata language detection, and nested subcall tree. | Executes Python PTC scripts inside its persistent per-session kernel (`tools/code_execution_tool.py`) with 7 sandbox-allowed tools (`read_file`, `write_file`, `patch`, `search_files`, `terminal`, `web_search`, `web_extract`). |
| **Git & Live Metrics Services** | Off-UI-thread Git status/diff/numstat (`backend/workspace.rs`, detached HEAD, non-Git dirs, explicit `worktree vs HEAD` vs agent edit counts) and live metrics engine (`harness/metrics.rs`: elapsed time, provider-reported vs estimated vs unavailable tokens, sliding-window output tok/s). | Supplies provider-reported `session.usage` (`input`, `output`, `reasoning`, `cache_hit_pct`, `avg_tps`) over WS. |

---

## 3. Authoritative Execution State Machine & `face/` Asset Mapping

All main-stream chat/work animations are driven from one authoritative `ExecutionState` enum derived from live runtime events. **Sidebar Bots Section** animations remain strictly unchanged (idle by default and while working; pop up + gaze only on hover or job completion).

| Runtime Execution State | Authoritative Trigger | Mapped `face/` `BotFaceState` | Existing `face/` Asset / Clip / Morph |
|---|---|---|---|
| `Idle` | No active turn, settlement window expired | `BotFaceState::Idle` | `Idle_B` / `Idle_D` / `Idle_A` |
| `Processing / Thinking` | Active turn, reasoning delta, or waiting for first token | `BotFaceState::Thinking` | **3 bouncing dots morph** (`thinking`) |
| `Executing Tools` | Running non-edit tool (`search_files` → `Searching` radar; `read_file` → `Reading`; `terminal` → `Running`; `execute_code` → `Coding`; other → `Working`) | `Searching` / `Reading` / `Running` / `Coding` / `Working` | Radar rings (`searching`) & `Working_*` clips |
| `Editing` | Running `write_file`, `patch`, `edit`, `apply_patch` | `BotFaceState::Writing` | **Angled pencil & live Catmull-Rom ink morph** (`writing`) |
| `Writing` (text streaming) | Assistant visible text streaming (`has_streamed_text && no active tool`) | `BotFaceState::Streaming` | `Streaming_A` / `Streaming_B` / `Streaming_C` |
| `Waiting` | Pending approval, clarify question, or waiting on subagent/delegation | `BotFaceState::Blocked` (approval/clarify) / `BotFaceState::Waiting` (subagent wait) | `Blocked_A` (approval) / **Orbit morph** (`orbit`) |
| `Completed` | Turn settled with `Success` (8s hold) | `BotFaceState::Finished` | `Done_B` / `Done_A` happy clip |
| `Failed` | Turn settled with `Error` / failed tool / interrupted error (8s hold) | `BotFaceState::Failed` | `Blocked_B` error clip |

*Audit finding:* Every required runtime state already has an authoritative animation/clip/morph in `apps/rust-gpui/src/face/` — **no new animation assets need to be invented**. Reduced-motion preference (`UiPreferences.reduced_motion` + `REDUCED_MOTION` env) switches inline faces to static/low-motion frames while preserving state expressions.

---

## 4. Feature Matrix

| Requirement | DeepSeek Harness Reference (`4b399c39`) | Hermes v0.21.0 Capability | Existing Rust Component | Required Rust Changes | Verification Scenario |
|---|---|---|---|---|---|
| **§3 Chat \| Work Modes** | Mode-gated session capabilities (`packages/core/session`) | `session.create(cwd, source="desktop")`, `sessions.cwd` | `ui/chat.rs`, `state/app_state.rs`, `state/persistence.rs` | Centered `Chat \| Work` pill toggle at top of new conversations; lock on first message; persist in `harness.db`; Work mode landing composer bar & workspace chrome. | Toggle works on draft; locks after 1st message; persists across restart; new conversation unlocks. |
| **§4 Inline Stream Animations & Minimal Activity** | `packages/client/ui-chat` (two-layer folding, auto-collapse on completion, kept open on error) | `message.start`, `reasoning.delta`, `tool.start`, `tool.complete`, `message.complete` | `ui/chat.rs`, `face/element.rs`, `theme/bot_face.rs` | Bare inline `face/` on page background (no card/border); remove assistant response bubble card; compact disclosure beside activity indicator (collapsed by default, auto-collapses on completion, stays accessible on failure); reduced-motion toggle + frame-time telemetry. | Sidebar bots stay idle while working; main stream face pops bare on background and transitions across all runtime states; activity auto-collapses on completion & reopens cleanly. |
| **§5 Streaming Markdown & Copying** | `packages/client/ui-chat`, `CodeBlock` verbatim copy | Streaming `message.delta` chunks | `ui/markdown.rs`, `ui/chat.rs` | Language-aware syntax highlighting (`Rust`, `TS/JS`, `Python`, `JSON`, `Bash`, `YAML`, `TOML`, `SQL`, `Diff`, `HTML/CSS`); copy full response & code blocks both during streaming and after completion with verbatim indentation; no flicker on incomplete fences; tail-follow + scroll-up lock. | Unit tests for incomplete Markdown chunks, syntax spans, exact whitespace copy; streaming scroll-lock check. |
| **§6 Project Setup & Server Directory Picker** | `working-directory/change`, `workspace-root/change` | `GET /api/fs/list`, `GET /api/fs/default-cwd`, `projects.*` WS RPCs | `backend/workspace.rs`, `hermes/client.rs` | `harness/projects.rs` + `ui/modals/project_picker.rs`: create/select/reuse projects (name + cwd); browse agent server filesystem (Hermes `/api/fs/list` + local/SSH fallback), server identity, parent/child nav, direct path input, home directory option with explicit scope badge, canonicalization/symlink policy validation, non-Git support. | Browse server dirs, handle empty & permission-error dirs, select home dir with scope notice, bind Work session & tools to saved cwd without running `git init`. |
| **§7 Hermes Commands & Tools** | `command/run`, `tool/call`, `tool/result`, `approval/*` | `commands.catalog`, `command.dispatch`, `slash.exec`, 81 commands, `tool.start`/`tool.complete` (`tool_id`, `duration_s`, `inline_diff`) | `hermes/ws.rs`, `ui/chat.rs`, `ui/modals/command_palette.rs` | Fix `ws.rs` `tool_id`/`duration_s`/`inline_diff`/`result_text` parsing; add searchable slash-command popover in composer + catalog sync marking terminal/config/unavailable commands; distinguish `waiting_for_approval`, `running`, `failed`, `completed`; preserve cancel & approvals. | Slash-command autocomplete & capability badges; real tool execution shows args/output/duration/errors; approval & cancel flow verified. |
| **§8 Persistent `/goal` & Background Worker** | `packages/goal/goal` (`fold.ts`, CAS transitions, `maxGoalRounds`), `goal-round-driver`, `ui-goal` | Native `/goal` WS `command.dispatch` (`status`/`pause`/`resume`/`clear`/`<text>`) + `status.update(kind="goal")` | None (new) | `harness/goal_worker.rs` + `harness/store.rs` + composer `GoalBar`: `/goal` entry point, CAS state machine (`active`, `paused`, `blocked`, `complete`, `cancelled`, `failed`), round/time/token caps + stop reasons, background Tokio worker surviving view/conversation switches & minimize, cold-restart recovery in `disarmed` state (zero duplicate runs). | Create/pause/resume/cancel/complete/fail unit + integration tests; switch conversations while running; restart restores persisted goal without launching duplicate runs. |
| **§9 Agent View & Persistent Trajectory** | `packages/client/ui-trajectory`, `packages/subagent/subagent`, `packages/client/ui-subagent` | `subagent.start`/`tool`/`thinking`/`complete`, `delegation.status`, `subagent.interrupt`/`steer`, `async_delegations` | `ui/inspector/right_pane.rs`, `hermes/ws.rs` | `harness/events.rs` + `harness/store.rs` + Inspector **Agents** & **Trajectory** tabs: active agent + parent-child subagent tree with individual status/interrupt; chronological virtualized trajectory with stable `session_id`/`run_id`/`turn`/`step`/`agent_id`/`tool_call_id`; persisted & deduplicated by `event_id`; read-only inspection never reruns tools. | Inspect restored trajectory across restarts without rerunning tools; verify subagent parent-child hierarchy & deduplication tests. |
| **§10 PTC (`run_code` / `execute_code`)** | `packages/core/tool-executor-ptc`, `packages/client/ui-trajectory/src/client/code-program.ts` | Native `execute_code` (Python persistent kernel, 7 sandbox tools) | None (new) | `harness/ptc.rs` + PTC Inspector panel (Trajectory & Chat): parity checklist below; supports Hermes `execute_code` & DeepSeek `run_code`/`cua_run_code`; readable line-numbered source, verbatim copy, `{}` original JSON view, schema-metadata-only language highlighting, verbatim output/error preservation, nested subcalls. | Fixture + live tests verifying TS/Python schema metadata detection, verbatim copy, `{}` JSON toggle, unsplit historical output preservation, error rendering. |
| **§11 Git View & Live Metrics** | `packages/deliverables/workspace-changes`, `packages/session/session-stats` | `GET /api/git/*`, WS `session.usage` (`input`, `output`, `reasoning`, `avg_tps`, `cache_hit_pct`) | `backend/workspace.rs`, `ui/inspector/right_pane.rs` | Upgrade `backend/workspace.rs` with detached-HEAD handling, `git diff --numstat` (`+added/-removed`, explicit `worktree vs HEAD` basis), per-file expandable diffs, separate agent-edit line counts; `harness/metrics.rs` for elapsed time, provider-reported vs estimated vs `unavailable` tokens, sliding-window output tok/s, off-UI-thread refresh. | Tests on normal repo, detached HEAD, pre-existing changes, non-Git folder; metrics tests verifying `Measured` vs `Estimated` vs `Unavailable` labeling. |

---

## 5. PTC (`run_code` / `execute_code`) Parity Checklist (Agreed Before Implementation)

Based directly on DeepSeek Harness (`packages/core/tool-executor-ptc/src/ptc.ts`, `packages/client/ui-trajectory/src/client/code-program.ts`, `TrajectoryTable.tsx:1929-2033`) and Hermes v0.21.0 (`tools/code_execution_tool.py`):

1. **Supported Languages & Interpreters:**
   - **Hermes live runtime**: Python 3.11 persistent per-session kernel via native `execute_code` tool (`from hermes_tools import ...`).
   - **DeepSeek reference / imported traces**: TypeScript (`run_code`, `cua_run_code`) and historical Python recordings.
   - **Inspector language resolution**: Strictly from recorded tool schema / recorded `language_meta` (`"typescript"` or `"python"`). Missing, conflicting, or unknown explicit declarations render as plain unhighlighted text — never guessed from source heuristics.
2. **Tool Access:**
   - Exposes Hermes's 7 sandbox-allowed tools inside `execute_code` (`read_file`, `write_file`, `patch`, `search_files`, `terminal`, `web_search`, `web_extract`), filtered to tools enabled in the session; tracks nested subcall IDs (`<parent>:ptc:<n>`).
3. **Execution Lifecycle & Cancellation:**
   - Distinct phases: `preparing` → `running` → `completed | failed | cancelled | timed_out`.
   - Cancellation forwards `session.interrupt` to Hermes (or aborts local Rust PTC runner if invoked locally) and records terminal status cleanly.
4. **Persistence:**
   - Records `PtcRecord` durably in `harness.db` (`tool_call_id`, `tool_name`, `description`, `source`, `raw_args_json`, `recorded_language`, `output_text`, `is_error`, `error_code`, `duration_ms`, `subcalls`). Retained kernel memory is explicitly non-persistent.
5. **Errors & Historical Output Preservation:**
   - Non-empty error output is rendered verbatim (even if valid JSON) without inventing synthetic `stdout` vs `return` splits when the recording has flat output text.
   - Non-error output renders structured JSON preview when valid JSON object/array, or verbatim `<pre>` otherwise.
6. **Inspector UI Controls:**
   - Line-numbered readable source view with syntax highlighting from recorded language metadata.
   - Verbatim `Copy source` (preserving exact newlines/indentation) and `Copy output`.
   - `{}` Original JSON toggle showing raw recorded `arguments` JSON verbatim.
   - Expandable nested subcall list (`<parent>:ptc:<n>`) and `Inspect in Trajectory` jump.

---

## 6. Setup & Run Instructions

### Prerequisites
- **Rust toolchain** (stable Cargo + `rustc`, edition 2021)
- **Hermes Agent runtime** (`hermes` CLI / dashboard, verified on `v0.21.0 (2026.8.31)`, commit `29112bef099274229cadff79cdff7bf7b99c4b77`, Python 3.11.17) for local execution, OR a remote Hermes server URL + credentials.
- Linux desktop session (Wayland or X11) with Vulkan/OpenGL GPU support for GPUI.

### Build & Run
```bash
# Development check (zero warnings)
cd apps/rust-gpui
cargo check --all-targets

# Run unit & fixture test suite
cargo test --no-fail-fast -- --skip real_ssh_pty_and_git_use_only_the_configured_target

# Run live Hermes v0.21.0 loopback verification (requires installed `hermes`)
cargo test live_hermes_runtime_smoke_verification -- --ignored --nocapture

# Build optimized release binary (50 MB standalone executable)
cargo build --release
./target/release/hermes-grok-bot-rust

# Or run from repository root via npm script
npm run desktop:rust:run
```

### Persistent State Locations
- **Harness durable SQLite store:** `~/.config/hermes-grok-bot/harness.db` (schema v1: `projects`, `conversation_modes`, `trajectory_events`, `goals`, `goal_events`, `subagent_runs`)
- **UI preferences (theme, reduced motion):** `~/.config/hermes-grok-bot/ui-prefs.json`
- **Hermes runtime store:** `$HERMES_HOME/state.db` & `$HERMES_HOME/projects.db`

---

## 7. Verification Results (Fixture Tests vs. Live Runtime Checks)

All checks below were executed on November 26, 2026 against the final tree (`cargo check --all-targets`: **0 errors, 0 warnings**; `cargo build --release`: **0 errors, 0 warnings**).

### A. Fixture & Automated Unit/Integration Tests (`75 passed, 0 failed`)
- **Modes, Projects & Durable Store (`harness::store`, `harness::projects`):**
  - Persists `Chat | Work` mode lock + project binding across SQLite reopen; rekeys draft IDs to live server IDs; deduplicates `trajectory_events` by `UNIQUE(event_id)`.
  - Lexical path normalization, symlink canonicalization, empty directory handling, permission/not-found errors, and non-Git folder support without running `git init`.
- **Execution State, Commands & Activity Disclosure (`harness::events`, `ui::chat`):**
  - All runtime states (`Idle`, `Thinking`, `ExecutingTools`, `Editing`, `Writing`, `Waiting`, `Completed`, `Failed`) map cleanly to authoritative `face/` assets.
  - Hermes v0.21.0 `commands.catalog` parser handles `categories`, `pairs`, `canon` aliases, `usage:` argument hints, and `desktop` capability markers (`RequiresTerminal`, `RequiresConfig`, `Unavailable`).
  - Activity disclosure starts collapsed by default, auto-collapses on turn completion, stays reopenable, and preserves failure diagnostics.
  - Sidebar Bot animations (`ui/sidebar.rs`) remain untouched: idle by default and while working, animating only on hover or job completion.
- **Streaming Markdown & Verbatim Copy (`ui::markdown`):**
  - UTF-8-safe syntax highlighting (`Rust`, `TS/JS/TSX`, `Python`, `JSON`, `Bash`, `YAML`, `TOML`, `SQL`, `Diff`, `HTML/CSS`), unknown/missing languages remain unhighlighted, streaming incomplete fences/strings/comments produce valid spans with no flicker, verbatim indentation/whitespace/newline preservation on copy.
- **Persistent `/goal` & Background Worker (`harness::goal_worker`):**
  - Parses `/goal` subcommands (`pause`, `resume`, `cancel`, `complete`, `status`, `--max-rounds`, `--max-minutes`, `--max-tokens`).
  - Enforces CAS revision transitions (`Active`, `Paused`, `Blocked`, `Complete`, `Cancelled`, `Failed`), round/duration/token budgets + stop reasons, and cold-restart recovery in `Disarmed` state (zero duplicate unattended runs).
- **PTC Inspection Parity (`harness::ptc`):**
  - Recognizes Hermes `execute_code` and DeepSeek `run_code`/`cua_run_code`; resolves highlighting strictly from recorded schema/language metadata (never guessed from source); preserves verbatim source, `{}` original JSON, and unsplit historical output/error text.
- **Git Status, Diffs & Live Metrics (`backend::workspace`, `harness::metrics`):**
  - Normal repo, detached HEAD, renamed/binary/unicode/spaced files, explicit `worktree vs HEAD` `git diff --numstat` counts, per-file diff section extraction, non-Git folder handling without `git init`.
  - Distinguishes `Measured` (provider-reported) vs `Unavailable` tokens and computes 5s sliding-window output tok/s.
- **Face Engine & Reduced Motion (`face::tests`, `face::element::tests`):**
  - All 26 bodies, authored clips, expressions, morphs, cutout GPU paths, reduced-motion static pose preservation, and 120-sample frame-interval ring buffer telemetry.

### B. Live Hermes v0.21.0 Runtime Verification (`live_hermes_runtime_smoke_verification`: `1 passed, 0 failed`)
- **Boot & Health:** Supervised local Hermes v0.21.0 dashboard started cleanly on loopback (`http://127.0.0.1:39895`) in `1,646 ms` and shut down cleanly on `stop()`.
- **HTTP APIs:** Verified `GET /api/status`, `GET /api/profiles` (1 profile), `GET /api/fs/default-cwd`, `GET /api/fs/list`, and `GET /api/fs/git-root` against both default workspace and `/home/metis/Projects/grok-bot-ui`.
- **WebSocket JSON-RPC (`/api/ws`):** Verified live WS handshake, `gateway.ping` (`{"ok":true}`), `commands.catalog` (**153 commands parsed, 151 live from Hermes v0.21.0 catalog** — live testing caught and fixed a wire-format mismatch where `commands.catalog` returns categorized `pairs` + `commands` metadata object rather than an array), and `projects.list`.
- **Note on Live LLM Turns:** Full multi-turn live LLM generation / interactive GUI clicking was not driven with paid external provider API calls in this automated headless session; live verification covered real local Hermes process startup, HTTP FS/status endpoints, and WebSocket JSON-RPC contracts, while streaming delta/tool/subagent/goal/PTC flows were verified through deterministic fixture tests.

---

## 8. Performance & Frame-Time Measurements

### Test Environment
- **OS / Kernel:** `Linux metis 7.2.11-arch1-1 #1 SMP PREEMPT_DYNAMIC x86_64`
- **CPU:** `Intel(R) Core(TM) i5-14500` (14 cores / 20 threads)
- **Build:** `cargo test --release` (`opt-level = 3`)

### Measured Results (`face_frame_pipeline_meets_120fps_budget`)
- **Workload:** 1,000 consecutive frames at 120 Hz (`dt = 8.333 ms`), cycling `thinking`, `writing`, `searching`, `streaming`, `waiting`, `finished`, and `failed` states (`FaceMotionHost::advance` + `frame()` + `render_tree()` at 36 px).
- **Average frame compute time:** **`84.2 µs` (`0.084 ms`)**
- **Median (p50):** **`70.1 µs` (`0.070 ms`)**
- **95th percentile (p95):** **`166.2 µs` (`0.166 ms`)** — **~50× faster than the `8,333 µs` 120 FPS frame budget**
- **Maximum (p100):** **`292.1 µs` (`0.292 ms`)**
- **Runtime Frame Telemetry:** `face::face_frame_stats()` continuously tracks the last 120 presented frame intervals in the live GPUI app and displays avg/p95 ms + effective FPS in the Inspector **Bot** tab.

---

## 9. Documented Differences & Runtime Limitations

1. **Desktop App Close vs. Background Goals:** Switching views, switching conversations, or minimizing the window keeps the Tokio `GoalWorker` and Hermes connection alive and auto-advancing rounds. **Closing the desktop application** stops the locally managed `HermesBackendSupervisor` child process (remote Hermes servers continue running independently). On cold restart or reconnect, persisted goals are restored from `harness.db` in **`Disarmed`** mode so duplicate unattended runs never fire until the user explicitly clicks **Resume**.
2. **PTC Runtime Language vs. DeepSeek Reference:** DeepSeek Harness executes TypeScript via `run_code`/`cua_run_code`, whereas Hermes v0.21.0 executes Python 3.11 via its native persistent kernel tool `execute_code`. Our Rust PTC inspector supports both formats with identical UI parity, resolving syntax highlighting strictly from recorded tool schema/language metadata (never guessing from source text).
3. **GPUI Text Selection & Verbatim Copy:** GPUI's `StyledText` renders rich syntax-highlighted and streaming Markdown blocks natively on the GPU, paired with one-click verbatim copy actions on every assistant response (both during streaming and after completion) and on every code block / PTC source / PTC output that copy exact source characters and indentation directly to the system clipboard.
4. **SSH Live Integration Test:** `real_ssh_pty_and_git_use_only_the_configured_target` requires an external SSH target configured via environment variables and was skipped (`--skip`) in local offline runs.
5. **Product Naming & Licenses:** Per DeepSeek Harness `BRAND_GUIDELINES.md`, `"DeepSeek Harness"` is not used in the application's product name. MIT attributions for both Hermes Agent and DeepSeek Harness are recorded in [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).
