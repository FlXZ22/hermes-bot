import { Channel } from "@tauri-apps/api/core";
import type { HermesSshTarget, HermesConnection, NativeHttpAdapter, NativeHttpResponse } from "@grok-bot/hermes-client";

interface TauriGlobal {
  core?: {
    invoke: <T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
  };
}

function getTauriInvoke(): (<T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>) | null {
  if (typeof window === "undefined") return null;
  const t = (window as unknown as { __TAURI__?: TauriGlobal }).__TAURI__;
  return t?.core?.invoke ?? null;
}

interface ChromiumDesktop {
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  terminalStart: (args: Record<string, unknown>, onEvent: (event: TerminalEvent) => void) => Promise<TerminalInfo>;
}

function getChromiumDesktop() {
  return typeof window === "undefined" ? undefined :
    (window as unknown as { hermesDesktop?: ChromiumDesktop }).hermesDesktop;
}

export function isDesktopHost(): boolean {
  return Boolean(getChromiumDesktop() || getTauriInvoke());
}

export function isTauriHost(): boolean {
  return Boolean(getTauriInvoke()) && !getChromiumDesktop();
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const fn = getChromiumDesktop()?.invoke ?? getTauriInvoke();
  if (!fn) {
    throw new Error(`Desktop command "${cmd}" unavailable in web mode`);
  }
  return fn<T>(cmd, args);
}

export type TerminalEvent =
  | { type: "started"; reused: boolean; generation: string }
  | { type: "output"; data: string }
  | { type: "exit"; code: number }
  | { type: "error"; message: string };
export interface TerminalInfo {
  id: string;
  pid: number | null;
  cwd: string;
  shell: string;
  remote: boolean;
  exit_code: number | null;
}

export interface RustBackendInfo {
  status: "starting" | "ready" | "error" | "stopped";
  url: string | null;
  token: string | null;
  pid: number | null;
  hermes_home: string;
  binary_path: string | null;
  boot_ms: number | null;
  error: string | null;
  logs: string[];
}

export interface LocalBootstrapPayload {
  active_model: string | null;
  active_provider: string | null;
  base_url: string | null;
  reasoning_effort: string | null;
  sessions: Array<{
    id: string;
    title?: string | null;
    model?: string | null;
    source?: string | null;
    started_at?: number | null;
    ended_at?: number | null;
    message_count: number;
    input_tokens: number;
    output_tokens: number;
    preview?: string | null;
    cwd?: string | null;
    pinned: boolean;
    archived: boolean;
  }>;
  projects: Array<{
    id: string;
    slug: string;
    name: string;
    description?: string | null;
    icon?: string | null;
    color?: string | null;
    board_slug?: string | null;
    primary_path?: string | null;
    created_at: number;
    archived: boolean;
  }>;
  models: Array<{
    id: string;
    name: string;
    provider: string;
    configured: boolean;
    context_length?: number | null;
  }>;
  skills: Array<{
    name: string;
    category: string;
    path: string;
    description: string;
    enabled: boolean;
  }>;
  soul_md: string;
  memory_md: string;
  user_md: string;
  board_state: Record<string, unknown>;
  elapsed_ms: number;
}

export interface WorkspaceFileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size: number;
  modified: number;
}

export interface GitWorkspaceStatus {
  is_repo: boolean;
  branch: string;
  changed_files: Array<{ status: string; path: string; index_status?: string; worktree_status?: string; paths?: string[] | null }>;
  worktrees: string[];
  diff: string;
}

export type GitWorkspaceAction =
  | { type: "stage" | "unstage"; paths: string[] }
  | { type: "commit"; message: string }
  | { type: "push" }
  | { type: "switchBranch"; branch: string }
  | { type: "worktree"; path: string; branch: string };

export type GitDiffScope = "unstaged" | "staged" | "uncommitted" | "branch" | "lastTurn";

export interface LinuxSystemStats {
  hostname: string;
  os_name: string;
  memory_used_mb: number;
  memory_total_mb: number;
  memory_percent: number;
  load_avg: string;
  home_dir: string;
}

export interface NativeTokenSet {
  access_token: string;
  refresh_token?: string;
  expires_at?: number;
  provider?: string;
}

export type AppUpdatePhase =
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "updating"
  | "restart_required"
  | "error";

export interface AppUpdateStatus {
  currentVersion: string;
  currentCommit: string;
  currentFullCommit: string;
  nativeCommit: string;
  builtAt: string;
  host: string;
  repoPath: string | null;
  repoValid: boolean;
  repoDirty: boolean;
  currentBranch: string;
  targetBranch: string;
  availableBranches: string[];
  githubRepo: string;
  autoCheck: boolean;
  phase: AppUpdatePhase;
  updateAvailable: boolean;
  commitsBehind: number;
  latestCommit: string | null;
  latestCommitMessage: string | null;
  latestCommitDate: string | null;
  rustRebuildNeeded: boolean;
  latestReleaseTag: string | null;
  latestReleaseUrl: string | null;
  step: string | null;
  logs: string[];
  lastCheckedAt: string | null;
  error: string | null;
}

export interface AppUpdateSettingsPatch {
  targetBranch?: string;
  repoPath?: string;
  autoCheck?: boolean;
}

export const tauriBridge = {
  terminalStart: (id: string, cwd: string, cols: number, rows: number, ssh: HermesSshTarget | undefined, onEvent: (event: TerminalEvent) => void): Promise<TerminalInfo> => {
    const chromium = getChromiumDesktop();
    if (chromium) return chromium.terminalStart({ id, cwd, cols, rows, ssh }, onEvent);
    const channel = new Channel<TerminalEvent>();
    channel.onmessage = onEvent;
    return invoke("terminal_start", { id, cwd, cols, rows, ssh, onEvent: channel });
  },
  terminalWrite: (id: string, data: string): Promise<void> => invoke("terminal_write", { id, data }),
  terminalAcknowledge: (id: string, generation: string, bytes: number): Promise<void> => invoke("terminal_acknowledge", { id, generation, bytes }),
  terminalResize: (id: string, cols: number, rows: number): Promise<void> => invoke("terminal_resize", { id, cols, rows }),
  terminalInfo: (id: string): Promise<TerminalInfo> => invoke("terminal_info", { id }),
  terminalClose: (id: string): Promise<void> => invoke("terminal_close", { id }),
  loadConnection: (): Promise<HermesConnection | null> => invoke("load_connection"),
  saveConnection: (connection: HermesConnection): Promise<void> => invoke("save_connection", { connection }),
  startLocalBackend: (): Promise<void> => invoke("start_local_backend"),
  stopLocalBackend: (): Promise<void> => invoke("stop_local_backend"),
  beginRemoteLogin: (url: string, provider: string): Promise<string> => invoke("begin_remote_login", { url, provider }),
  pollRemoteLogin: (): Promise<NativeTokenSet | null> => invoke("poll_remote_login"),
  cancelRemoteLogin: (): Promise<void> => invoke("cancel_remote_login"),
  refreshRemoteLogin: (connection: HermesConnection): Promise<NativeTokenSet> => invoke("refresh_remote_login", {
    url: connection.url, refreshToken: connection.refreshToken, provider: connection.authProvider ?? "",
  }),
  isDesktop: isDesktopHost,

  getBackendInfo: (): Promise<RustBackendInfo> => invoke<RustBackendInfo>("get_backend_info"),

  restartBackend: (): Promise<RustBackendInfo> => invoke<RustBackendInfo>("restart_hermes_backend"),

  createHttpAdapter: (): NativeHttpAdapter | null => {
    if (!isDesktopHost()) return null;
    return async (req) =>
      invoke<NativeHttpResponse>("hermes_http_request", {
        req: {
          url: req.url,
          method: req.method,
          headers: req.headers,
          body: req.body,
        },
      });
  },

  loadLocalBootstrap: (profile?: string): Promise<LocalBootstrapPayload> =>
    invoke<LocalBootstrapPayload>("load_local_bootstrap", { profile }),

  loadLocalSessionMessages: (sessionId: string, limit = 500, profile?: string) =>
    invoke<
      Array<{
        id: string;
        role: string;
        content: string;
        timestamp?: number | null;
        model?: string | null;
      }>
    >("load_local_session_messages", { sessionId, limit, profile }),

  loadBoardState: (): Promise<Record<string, unknown>> =>
    invoke<Record<string, unknown>>("load_board_state"),

  saveBoardState: (state: Record<string, unknown>): Promise<boolean> =>
    invoke<boolean>("save_board_state", { state }),

  saveLocalMemoryFile: (kind: "soul" | "memory" | "user", content: string, profile?: string): Promise<boolean> =>
    invoke<boolean>("save_local_memory_file", { kind, content, profile }),

  listWorkspaceFiles: (dirPath: string): Promise<WorkspaceFileEntry[]> =>
    invoke<WorkspaceFileEntry[]>("list_workspace_files", { dirPath }),

  readWorkspaceFile: (filePath: string): Promise<string> =>
    invoke<string>("read_workspace_file", { filePath }),

  gitWorkspaceStatus: (dirPath: string, ssh?: HermesSshTarget, scope?: GitDiffScope, baseline?: string): Promise<GitWorkspaceStatus> =>
    invoke<GitWorkspaceStatus>("git_workspace_status", { dirPath, ssh, scope, baseline }),

  gitWorkspaceAction: (dirPath: string, action: GitWorkspaceAction, ssh?: HermesSshTarget): Promise<string> =>
    invoke<string>("git_workspace_action", { dirPath, action, ssh }),

  gitWorkTurnBaseline: (dirPath: string, ssh?: HermesSshTarget): Promise<string> =>
    invoke<string>("git_work_turn_baseline", { dirPath, ssh }),

  openExternalLink: (address: string): Promise<void> => invoke<void>("open_external_link", { address }),
  loadLegacyRendererState: (): Promise<Record<string, string>> => invoke<Record<string, string>>("load_legacy_renderer_state"),
  saveWorkExport: (fileName: string, contents: string): Promise<string> => invoke<string>("save_work_export", { fileName, contents }),

  getLinuxSystemStats: (): Promise<LinuxSystemStats> =>
    invoke<LinuxSystemStats>("get_linux_system_stats"),

  getUpdateStatus: (): Promise<AppUpdateStatus> =>
    invoke<AppUpdateStatus>("get_update_status"),

  checkForUpdates: (): Promise<AppUpdateStatus> =>
    invoke<AppUpdateStatus>("check_for_updates"),

  setUpdateSettings: (patch: AppUpdateSettingsPatch): Promise<AppUpdateStatus> =>
    invoke<AppUpdateStatus>("set_update_settings", { patch }),

  applyAppUpdate: (mode: "remote" | "local", forceNativeRebuild = false): Promise<AppUpdateStatus> =>
    invoke<AppUpdateStatus>("apply_app_update", { mode, forceNativeRebuild }),

  restartApp: (): Promise<void> =>
    invoke<void>("restart_app"),

  windowMinimize: (): Promise<void> => invoke<void>("window_minimize").catch(() => undefined),
  windowToggleMaximize: (): Promise<void> => invoke<void>("window_toggle_maximize").catch(() => undefined),
  windowClose: (): Promise<void> => invoke<void>("window_close").catch(() => undefined),
  windowStartDrag: (): Promise<void> => invoke<void>("window_start_drag").catch(() => undefined),
};
