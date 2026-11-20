import { ChatStore, lastMessageIndex, newAssistant } from "./chatStore";
import React, { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import {
  HermesClient,
  HermesRealtimeSession,
  setNativeHttpAdapter,
  parseSavedConnection,
  connectionError,
  normalizeServerUrl,
  type HermesBoardSummary,
  type HermesConnection,
  type HermesCronJob,
  type HermesKanbanStatus,
  type HermesKanbanTask,
  type HermesMcpServer,
  type HermesMessage,
  type HermesMessagingPlatform,
  type HermesModelOption,
  type HermesProfile,
  type HermesSession,
  type HermesSkill,
  type HermesStatus,
  type HermesStreamEvent,
  type HermesToolset,
  type HermesUsagePoint,
  type HermesWebhook,
} from "@grok-bot/hermes-client";
import { resolveBotColor, type GrokBotColorId } from "@grok-bot/theme";
import { tauriBridge, type LinuxSystemStats, type RustBackendInfo } from "../lib/tauriBridge";
import { slugify } from "../lib/format";

export type AppViewId =
  | "chat"
  | "boards"
  | "skills"
  | "cron"
  | "memory"
  | "gateway"
  | "analytics"
  | "settings";

export type RightPaneTabId = "files" | "terminal" | "git" | "bot";
export type ThemeMode = "light" | "dark";
export type ReasoningEffort = "none" | "low" | "medium" | "high" | "xhigh";

export interface BotAvatarOverride {
  shapeId?: string;
  colorId?: GrokBotColorId;
  pinned?: boolean;
  displayName?: string;
}

export interface PersistedBoardState {
  boards: HermesBoardSummary[];
  sessionBoards: Record<string, string>; // sessionId -> boardSlug
  tasks: HermesKanbanTask[];
  botAvatars: Record<string, BotAvatarOverride>;
}

const DEFAULT_BOARD: HermesBoardSummary = {
  slug: "default",
  name: "General",
  description: "Default Hermes workspace session board",
  color: "blue",
  workspace_path: "",
};

const HISTORY_PAGE_SIZE = 48;

const STORAGE_THEME_KEY = "hermes-grok-bot:theme";
const STORAGE_CONN_KEY = "hermes-grok-bot:connection";
const STORAGE_BOARDS_KEY = "hermes-grok-bot:boards";
const STORAGE_SIDE_WIDTH_KEY = "hermes-grok-bot:side-width";

export interface AppContextValue {
  // UI Shell State
  theme: ThemeMode;
  setTheme: (t: ThemeMode) => void;
  toggleTheme: () => void;
  activeView: AppViewId;
  setActiveView: (v: AppViewId) => void;
  sidebarCollapsed: boolean;
  setSidebarCollapsed: (c: boolean) => void;
  rightPaneOpen: boolean;
  setRightPaneOpen: (open: boolean) => void;
  rightPaneTab: RightPaneTabId;
  setRightPaneTab: (tab: RightPaneTabId) => void;
  rightPaneWidth: number;
  setRightPaneWidth: (w: number) => void;
  commandPaletteOpen: boolean;
  setCommandPaletteOpen: (open: boolean) => void;
  modelPickerOpen: boolean;
  setModelPickerOpen: (open: boolean) => void;
  avatarEditorTarget: { seed: string; title: string } | null;
  setAvatarEditorTarget: (target: { seed: string; title: string } | null) => void;

  // Backend & Connection State
  isDesktop: boolean;
  isLocalConnection: boolean;
  setupOpen: boolean;
  setSetupOpen: (open: boolean) => void;
  initializing: boolean;
  startLocalConnection: () => Promise<void>;
  rustBackend: RustBackendInfo | null;
  connection: HermesConnection;
  connectionStatus: "connecting" | "connected" | "local" | "disconnected" | "error";
  connectionDetail: string;
  hermesStatus: HermesStatus | null;
  systemStats: LinuxSystemStats | null;
  bootstrapMs: number | null;
  client: HermesClient | null;
  updateConnection: (conn: HermesConnection) => Promise<void>;
  restartingBackend: boolean;
  refreshingModels: boolean;
  restartLocalBackend: () => Promise<void>;
  refreshModels: () => Promise<void>;
  refreshAll: () => Promise<void>;

  // Profiles / Bots & Avatars
  profiles: HermesProfile[];
  activeProfile: string;
  profileSwitching: boolean;
  setActiveProfile: (name: string) => Promise<void>;
  botAvatars: Record<string, BotAvatarOverride>;
  updateBotAvatar: (seed: string, patch: BotAvatarOverride) => void;
  soulMd: string;
  memoryMd: string;
  userMd: string;
  saveMemoryFile: (kind: "soul" | "memory" | "user", content: string) => Promise<void>;

  // Session Boards & Kanban Tasks
  boards: HermesBoardSummary[];
  activeBoardSlug: string; // "all" or specific board slug
  setActiveBoardSlug: (slug: string) => void;
  sessionBoards: Record<string, string>;
  assignSessionToBoard: (sessionId: string, boardSlug: string) => void;
  createBoard: (payload: { name: string; description?: string; color?: string; workspace_path?: string }) => Promise<HermesBoardSummary>;
  deleteBoard: (slug: string) => Promise<void>;
  tasks: HermesKanbanTask[];
  createTask: (payload: {
    title: string;
    body?: string;
    status?: HermesKanbanStatus;
    priority?: string;
    assignee?: string;
    model?: string;
    provider?: string;
    board?: string;
    cwd?: string;
  }) => Promise<HermesKanbanTask>;
  updateTask: (taskId: string, patch: Partial<HermesKanbanTask>) => Promise<void>;
  deleteTask: (taskId: string) => Promise<void>;
  dispatchBoardWorkers: (boardSlug?: string) => Promise<void>;
  launchSessionForTask: (task: HermesKanbanTask) => Promise<void>;

  // Sessions & Chat
  sessions: HermesSession[];
  activeSessionId: string | null;
  activeSession: HermesSession | null;
  chat: ChatStore;
  loadOlderMessages: () => Promise<void>;
  selectSession: (sessionId: string | null) => Promise<void>;
  createNewSession: (opts?: { title?: string; boardSlug?: string; cwd?: string; model?: string; provider?: string }) => Promise<string | null>;
  sendMessage: (text: string, attachments?: File[]) => Promise<void>;
  interruptStream: () => Promise<void>;
  renameSession: (sessionId: string, title: string) => Promise<void>;
  deleteSession: (sessionId: string) => Promise<void>;
  archiveSession: (sessionId: string, archived: boolean) => Promise<void>;
  togglePinSession: (sessionId: string) => Promise<void>;
  compressActiveSession: () => Promise<void>;
  branchActiveSession: () => Promise<void>;
  respondApproval: (approvalId: string, choice: "once" | "always" | "deny") => Promise<void>;
  respondClarify: (clarifyId: string, answer: string) => Promise<void>;

  // Models & Inference Controls
  models: HermesModelOption[];
  activeModel: string;
  activeProvider: string;
  reasoningEffort: ReasoningEffort;
  fastMode: boolean;
  yoloMode: boolean;
  selectModel: (model: string, provider?: string, saveAsDefault?: boolean) => Promise<void>;
  setReasoningEffort: (effort: ReasoningEffort) => void;
  setFastMode: (fast: boolean) => void;
  setYoloMode: (yolo: boolean) => void;

  // Hermes Capabilities Data
  skills: HermesSkill[];
  toolsets: HermesToolset[];
  mcpServers: HermesMcpServer[];
  cronJobs: HermesCronJob[];
  messagingPlatforms: HermesMessagingPlatform[];
  webhooks: HermesWebhook[];
  usagePoints: HermesUsagePoint[];
  refreshCapabilities: () => Promise<void>;
  toast: string | null;
  showToast: (msg: string) => void;
}

const AppContext = createContext<AppContextValue | null>(null);

function loadStoredTheme(): ThemeMode {
  try {
    const saved = localStorage.getItem(STORAGE_THEME_KEY);
    if (saved === "dark" || saved === "light") return saved;
  } catch {
    // ignore
  }
  return "dark";
}

function loadStoredBoardState(): PersistedBoardState {
  try {
    const raw = localStorage.getItem(STORAGE_BOARDS_KEY);
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<PersistedBoardState>;
      return {
        boards: parsed.boards && parsed.boards.length ? parsed.boards : [DEFAULT_BOARD],
        sessionBoards: parsed.sessionBoards ?? {},
        tasks: parsed.tasks ?? [],
        botAvatars: parsed.botAvatars ?? {},
      };
    }
  } catch {
    // ignore
  }
  return {
    boards: [DEFAULT_BOARD],
    sessionBoards: {},
    tasks: [],
    botAvatars: {},
  };
}

function loadStoredConnection(): HermesConnection {
  try {
    const raw = localStorage.getItem(STORAGE_CONN_KEY);
    if (raw) { const conn = parseSavedConnection(JSON.parse(raw)); if (conn) return conn; }
  } catch {
    // ignore
  }
  return {
    id: "local-rust",
    name: "This computer",
    url: "http://127.0.0.1:9119",
    mode: "dashboard",
    authType: "session-token",
  };
}

export const AppProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const isDesktop = tauriBridge.isDesktop();

  // Shell UI state
  const [theme, setThemeState] = useState<ThemeMode>(loadStoredTheme);
  const [activeView, setActiveView] = useState<AppViewId>("chat");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [rightPaneOpen, setRightPaneOpen] = useState(false);
  const [rightPaneTab, setRightPaneTab] = useState<RightPaneTabId>("files");
  const [rightPaneWidth, setRightPaneWidthState] = useState<number>(() => {
    try {
      const w = Number(localStorage.getItem(STORAGE_SIDE_WIDTH_KEY));
      if (w >= 260 && w <= 620) return w;
    } catch {
      // ignore
    }
    return 336;
  });
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [modelPickerOpen, setModelPickerOpen] = useState(false);
  const [avatarEditorTarget, setAvatarEditorTarget] = useState<{ seed: string; title: string } | null>(null);
  const [toast, setToast] = useState<string | null>(null);

  const [setupOpen, setSetupOpen] = useState(true);
  const [initializing, setInitializing] = useState(true);
  const connectionAttemptRef = useRef(0);
  // Backend & Connection
  const [rustBackend, setRustBackend] = useState<RustBackendInfo | null>(null);
  const [connection, setConnection] = useState<HermesConnection>(loadStoredConnection);
  const [connectionStatus, setConnectionStatus] = useState<
    "connecting" | "connected" | "local" | "disconnected" | "error"
  >("connecting");
  const [connectionDetail, setConnectionDetail] = useState("Choose where Hermes runs");
  const isLocalConnection = connection.id === "local-rust" || connection.id === "local-dev";
  const [hermesStatus, setHermesStatus] = useState<HermesStatus | null>(null);
  const [systemStats, setSystemStats] = useState<LinuxSystemStats | null>(null);
  const [bootstrapMs, setBootstrapMs] = useState<number | null>(null);
  const [restartingBackend, setRestartingBackend] = useState(false);
  const [refreshingModels, setRefreshingModels] = useState(false);
  const wasDisconnectedRef = useRef(false);

  // Profiles & Memory
  const [profiles, setProfiles] = useState<HermesProfile[]>([
    { name: "default", active: true, default: true, description: "Primary Hermes Agent" },
  ]);
  const profileSwitchRef = useRef(false);
  const [profileSwitching, setProfileSwitching] = useState(false);
  const [activeProfile, setActiveProfileState] = useState<string>("default");
  const [soulMd, setSoulMd] = useState("");
  const [memoryMd, setMemoryMd] = useState("");
  const [userMd, setUserMd] = useState("");

  // Session Boards & Kanban
  const initialBoardState = useMemo(loadStoredBoardState, []);
  const [boards, setBoards] = useState<HermesBoardSummary[]>(initialBoardState.boards);
  const [boardsHydrated, setBoardsHydrated] = useState(!isDesktop);
  const boardSaveQueueRef = useRef<Promise<unknown>>(Promise.resolve());
  const [activeBoardSlug, setActiveBoardSlug] = useState<string>("all");
  const [sessionBoards, setSessionBoards] = useState<Record<string, string>>(initialBoardState.sessionBoards);
  const [tasks, setTasks] = useState<HermesKanbanTask[]>(initialBoardState.tasks);
  const [botAvatars, setBotAvatars] = useState<Record<string, BotAvatarOverride>>(initialBoardState.botAvatars);

  // Sessions & Chat
  const [sessions, setSessions] = useState<HermesSession[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [chat, setChat] = useState(() => new ChatStore());
  const currentChat = useRef(chat);
  const profileChats = useRef(new Map<string, ChatStore>());
  const sseControllers = useRef(new Map<ChatStore, Map<string, AbortController>>());
  const liveRoutes = useRef(new Map<string, { chat: ChatStore; id: string }>());
  const activateChat = useCallback((profile: string, reset = false) => {
    if (reset) {
      for (const controllers of sseControllers.current.values()) {
        for (const controller of controllers.values()) controller.abort();
      }
      sseControllers.current.clear();
      for (const store of new Set([currentChat.current, ...profileChats.current.values()])) store.disconnect();
      profileChats.current.clear();
      liveRoutes.current.clear();
    }
    const store = profileChats.current.get(profile) ?? new ChatStore();
    profileChats.current.set(profile, store);
    currentChat.current = store;
    setChat(store);
  }, []);
  const registerLiveSession = useCallback((store: ChatStore, id: string, wireId: string) => {
    store.liveIds[id] = wireId;
    liveRoutes.current.set(wireId, { chat: store, id });
  }, []);

  // Models & Inference settings
  const [models, setModels] = useState<HermesModelOption[]>([]);
  const [activeModel, setActiveModel] = useState<string>("anthropic/claude-opus-4.6");
  const [activeProvider, setActiveProvider] = useState<string>("auto");
  const [reasoningEffort, setReasoningEffort] = useState<ReasoningEffort>("medium");
  const [fastMode, setFastMode] = useState<boolean>(false);
  const [yoloMode, setYoloMode] = useState<boolean>(false);

  // Capabilities
  const [skills, setSkills] = useState<HermesSkill[]>([]);
  const [toolsets, setToolsets] = useState<HermesToolset[]>([]);
  const [mcpServers, setMcpServers] = useState<HermesMcpServer[]>([]);
  const [cronJobs, setCronJobs] = useState<HermesCronJob[]>([]);
  const [messagingPlatforms, setMessagingPlatforms] = useState<HermesMessagingPlatform[]>([]);
  const [webhooks, setWebhooks] = useState<HermesWebhook[]>([]);
  const [usagePoints, setUsagePoints] = useState<HermesUsagePoint[]>([]);

  // Mutable refs for live WS client and live session mapping
  const clientRef = useRef<HermesClient | null>(null);
  const rtRef = useRef<HermesRealtimeSession | null>(null);
  const draftSessionIdsRef = useRef(new Set<string>());
  const activeSessionIdRef = useRef<string | null>(null);
  activeSessionIdRef.current = activeSessionId;

  const toastTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const showToast = useCallback((msg: string) => {
    setToast(msg);
    if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
    toastTimerRef.current = setTimeout(() => setToast(null), 3200);
  }, []);

  // Persist committed state once, outside React updater functions. Native
  // saves run in order so quick successive edits cannot race the temp file.
  useEffect(() => {
    if (!boardsHydrated || setupOpen) return;
    const state = { boards, sessionBoards, tasks, botAvatars };
    try { localStorage.setItem(isLocalConnection ? STORAGE_BOARDS_KEY : `${STORAGE_BOARDS_KEY}:${connection.url}`, JSON.stringify(state)); } catch { /* Storage unavailable. */ }
    if (isDesktop && isLocalConnection) {
      boardSaveQueueRef.current = boardSaveQueueRef.current
        .catch(() => undefined)
        .then(() => tauriBridge.saveBoardState(state as unknown as Record<string, unknown>))
        .catch(error => showToast(`Could not save board state: ${String(error)}`));
    }
  }, [boards, sessionBoards, tasks, botAvatars, boardsHydrated, isDesktop, isLocalConnection, connection.url, setupOpen, showToast]);

  // Sync theme to document root (authentic Grok Bot data-theme="cursor-dark")
  useEffect(() => {
    const root = document.documentElement;
    if (theme === "dark") {
      root.setAttribute("data-theme", "cursor-dark");
    } else {
      root.removeAttribute("data-theme");
    }
    try {
      localStorage.setItem(STORAGE_THEME_KEY, theme);
    } catch {
      // ignore
    }
  }, [theme]);

  // Apply per-bot dynamic tint to CSS variables (exact Grok Bot side-pane & accent tinting)
  const activeSession = useMemo(
    () => sessions.find((s) => s.id === activeSessionId) ?? null,
    [sessions, activeSessionId]
  );

  useEffect(() => {
    const seed = activeSession?.profile || activeProfile || "default";
    const override = botAvatars[seed];
    const preset = resolveBotColor(seed, override?.colorId);
    const root = document.documentElement;
    root.style.setProperty("--grok-bot-current-agent-tint", theme === "dark" ? preset.darkTint : preset.lightTint);
    root.style.setProperty("--grok-bot-current-agent-accent", theme === "dark" ? preset.darkAccent : preset.lightAccent);
    root.style.setProperty("--grok-bot-current-agent-bubble", theme === "dark" ? preset.darkBubble : preset.lightBubble);
    root.style.setProperty("--grok-bot-current-agent-coat", preset.hex);
  }, [activeProfile, activeSession?.profile, botAvatars, theme]);

  // Global keyboard shortcuts (Ctrl+K Command Palette, Ctrl+N New Chat, Ctrl+B Sidebar, Ctrl+\ Right Pane)
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (setupOpen) return;
      if ((e.target as HTMLElement)?.closest?.(".xterm")) return;
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;
      const key = e.key.toLowerCase();
      if (key === "k") {
        e.preventDefault();
        setCommandPaletteOpen((v) => !v);
      } else if (key === "b" && !e.shiftKey) {
        e.preventDefault();
        setSidebarCollapsed((v) => !v);
      } else if (key === "\\") {
        e.preventDefault();
        setRightPaneOpen((v) => !v);
      } else if (key === "m" && e.shiftKey) {
        e.preventDefault();
        setModelPickerOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [setupOpen]);

  const setTheme = useCallback((t: ThemeMode) => setThemeState(t), []);
  const toggleTheme = useCallback(() => setThemeState((prev) => (prev === "dark" ? "light" : "dark")), []);
  const setRightPaneWidth = useCallback((w: number) => {
    const clamped = Math.max(260, Math.min(620, w));
    setRightPaneWidthState(clamped);
    try {
      localStorage.setItem(STORAGE_SIDE_WIDTH_KEY, String(clamped));
    } catch {
      // ignore
    }
  }, []);

  // Handle real-time Hermes WebSocket events
  const handleStreamEvent = useCallback((event: HermesStreamEvent) => {
    const wireSid = "sessionId" in event ? event.sessionId : undefined;
    const route = wireSid ? liveRoutes.current.get(wireSid) : undefined;
    const targetChat = route?.chat ?? currentChat.current;
    const targetSid = route?.id ?? wireSid ?? activeSessionIdRef.current ?? "draft";

    if (event.type === "status") {
      if (event.status === "connected") {
        setConnectionStatus("connected");
        setConnectionDetail("Live Hermes Gateway");
        if (wasDisconnectedRef.current) {
          wasDisconnectedRef.current = false;
          void refreshCapabilities({ refreshModels: true });
        }
      } else if (event.status === "connecting") {
        setConnectionStatus("connecting");
        setConnectionDetail("Connecting WebSocket...");
      } else if (event.status === "disconnected") {
        wasDisconnectedRef.current = true;
        setConnectionStatus("disconnected");
        setConnectionDetail("Connection lost. Reconnecting…");
        for (const store of new Set([currentChat.current, ...profileChats.current.values()])) store.disconnect();
        liveRoutes.current.clear();
      } else if (event.status === "error") {
        setConnectionStatus("error");
        setConnectionDetail(event.detail ?? "WebSocket error");
      }
      return;
    }

    // A runtime ID is registered by create/resume before the client submits.
    // Unknown IDs must not write to a different profile with a matching stored ID.
    if (wireSid && !route) return;

    if (event.type === "session.info" || event.type === "session.ready") {
      if (targetChat !== currentChat.current) return;
      const { model, provider } = event.info ?? {};
      setSessions(previous => previous.map(session => {
        if (session.id !== targetSid) return session;
        const nextModel = typeof model === "string" ? model : session.model;
        const nextProvider = typeof provider === "string" ? provider : session.provider;
        return nextModel === session.model && nextProvider === session.provider
          ? session : { ...session, model: nextModel, provider: nextProvider };
      }));
      return;
    }

    if (event.type === "text.delta" || event.type === "thinking.delta") {
      targetChat.buffer(targetSid, event.delta, event.type === "text.delta" ? "text" : "thinking");
      return;
    }

    if (event.type === "status.update") targetChat.patch(targetSid, { liveStatusText: event.text });
    if (event.type === "session.usage") targetChat.patch(targetSid, { sessionUsage: event.usage });
    if (event.type === "todo.updated") targetChat.patch(targetSid, { todos: event.todos });

    if (event.type === "message.start") {
      targetChat.patch(targetSid, { isStreaming: true });
      targetChat.updateMessages(targetSid, messages => messages.some(message => message.streaming)
        ? messages : [...messages, newAssistant()]);
    }

    if (event.type === "message.interim") {
      targetChat.updateMessages(targetSid, messages => messages.map(message =>
        message.streaming ? { ...message, content: event.content, streaming: false } : message));
    }

    if (event.type === "message.complete" || event.type === "error") {
      targetChat.finish(targetSid, event.type === "message.complete" ? event.content : undefined);
      if (event.type === "error") {
        if (targetChat === currentChat.current) showToast(event.message);
        targetChat.updateMessages(targetSid, messages => [
          ...messages, newAssistant({ content: event.message, streaming: false }),
        ]);
      }
    }

    if (event.type === "tool.start") {
      targetChat.patch(targetSid, { isStreaming: true, liveStatusText: `Running ${event.toolCall.name}…` });
      targetChat.updateMessages(targetSid, messages => {
        const index = lastMessageIndex(messages, message => Boolean(message.streaming));
        if (index < 0) return [...messages, newAssistant({ toolCalls: [event.toolCall] })];
        const next = messages.slice();
        const message = messages[index];
        next[index] = { ...message, toolCalls: [...(message.toolCalls ?? []), event.toolCall] };
        return next;
      });
    }

    if (event.type === "tool.progress" || event.type === "tool.end") {
      if (event.type === "tool.end") targetChat.patch(targetSid, { liveStatusText: "" });
      const matches = (tool: NonNullable<HermesMessage["toolCalls"]>[number]) =>
        event.toolCallId ? tool.id === event.toolCallId : tool.status === "running";
      targetChat.updateMessages(targetSid, messages => {
        const index = lastMessageIndex(messages, message => Boolean(message.toolCalls?.some(matches)));
        if (index < 0) return messages;
        const message = messages[index];
        const tools = message.toolCalls!.map(tool => {
          if (!matches(tool)) return tool;
          if (event.type === "tool.progress") return { ...tool, preview: event.preview };
          return { ...tool, status: event.error ? "error" as const : "done" as const,
            output: event.error ?? event.output ?? tool.output, durationMs: event.durationMs };
        });
        const next = messages.slice();
        next[index] = { ...message, toolCalls: tools };
        return next;
      });
    }

    if (event.type === "approval.request" || event.type === "clarify.request") {
      const approval = event.type === "approval.request" ? event.approval : undefined;
      const clarify = event.type === "clarify.request" ? event.clarify : undefined;
      const id = approval ? `appr-${approval.id}` : `clarify-${clarify!.id}`;
      targetChat.updateMessages(targetSid, messages => messages.some(message => message.id === id)
        ? messages : [...messages, newAssistant({ id, approval, clarify, streaming: false })]);
    }

    if (event.type === "approval.settled" || event.type === "clarify.settled") {
      targetChat.updateMessages(targetSid, messages => messages.map(message => {
        if (event.type === "approval.settled" && message.approval?.id === event.id) {
          return { ...message, approval: { ...message.approval, settled: event.decision } };
        }
        if (event.type === "clarify.settled" && message.clarify?.id === event.id) {
          return { ...message, clarify: { ...message.clarify, answered: event.answer } };
        }
        return message;
      }));
    }
  }, [showToast]);

  // Hydrate all Hermes capabilities from live HTTP API
  const refreshCapabilities = useCallback(async (opts?: { refreshModels?: boolean }) => {
    const c = clientRef.current;
    if (!c) return;
    const requestedProfile = c.getProfile();

    // Apply each capability as it arrives: slow model discovery must not
    // delay sessions, boards, or skills. One optional API cannot block startup.
    const load = async <T,>(name: string, request: Promise<T>, apply: (data: T) => void | Promise<void>) => {
      try {
        const data = await request;
        if (clientRef.current === c && c.getProfile() === requestedProfile) await apply(data);
      } catch (error) {
        console.warn(`Hermes ${name}:`, error instanceof Error ? error.message : String(error));
      }
    };
    await Promise.allSettled([
      load("status", c.getStatus(), setHermesStatus),
      load("profiles", c.listProfiles(), data => {
        if (data.length) setProfiles(data);
      }),
      load("sessions", c.listSessions(120, "include"), data => setSessions(prev => [
        ...data.map(s => ({ ...prev.find(p => p.id === s.id), ...s })),
        ...prev.filter(s => (s.id.startsWith("session_") || draftSessionIdsRef.current.has(s.id)) && !data.some(d => d.id === s.id)),
      ])),
      load("models", c.getModelOptions(true, Boolean(opts?.refreshModels)), data => { if (data.length) setModels(data); }),
      load("model info", c.getModelInfo(), info => {
        if (!activeSessionIdRef.current) {
          if (typeof info.model === "string" && info.model) setActiveModel(info.model);
          if (typeof info.provider === "string" && info.provider) setActiveProvider(info.provider);
        }
      }),
      load("skills", c.listSkills(), setSkills),
      load("toolsets", c.listToolsets(), setToolsets),
      load("MCP servers", c.listMcpServers(), setMcpServers),
      load("cron jobs", c.listCronJobs(), setCronJobs),
      load("platforms", c.listMessagingPlatforms(), setMessagingPlatforms),
      load("webhooks", c.listWebhooks(), setWebhooks),
      load("usage", c.getUsageAnalytics(14), setUsagePoints),
      load("boards", c.listKanbanBoards(), async data => {
        setBoards(prev => {
          const merged = new Map(prev.map(b => [b.slug, b]));
          for (const board of data) merged.set(board.slug, { ...merged.get(board.slug), ...board });
          return [...merged.values()];
        });
        const taskSets = await Promise.allSettled(data.map(board => c.getKanbanBoard(board.slug)));
        if (clientRef.current !== c) return;
        const remoteTasks = taskSets.flatMap(result => result.status === "fulfilled" ? result.value.tasks : []);
        const hydrated = new Set(data.filter((_, index) => taskSets[index].status === "fulfilled").map(board => board.slug));
        setTasks(prev => [...prev.filter(task => task.id.startsWith("task_") || !hydrated.has(task.board ?? "default")), ...remoteTasks]);
      }),
    ]);
  }, []);

  const persistConnection = useCallback(async (conn: HermesConnection) => {
    // Local runtime tokens and ports are ephemeral. Always reacquire on launch.
    const saved = conn.id === "local-rust" || conn.id === "local-dev"
      ? { ...conn, url: "http://127.0.0.1:9119", token: undefined } : conn;
    if (isDesktop) {
      await tauriBridge.saveConnection(saved);
      localStorage.removeItem(STORAGE_CONN_KEY); // migrate legacy secrets out of WebKit storage
    } else {
      localStorage.setItem(STORAGE_CONN_KEY, JSON.stringify(saved));
    }
  }, [isDesktop]);

  const hydrateLocal = useCallback(async () => {
        try {
          const [boot, sys] = await Promise.all([
            tauriBridge.loadLocalBootstrap(),
            tauriBridge.getLinuxSystemStats().catch(() => null),
          ]);
          setBootstrapMs(boot.elapsed_ms);
          if (sys) setSystemStats(sys);

          if (boot.active_model) setActiveModel(boot.active_model);
          if (boot.active_provider) setActiveProvider(boot.active_provider);
          if (boot.reasoning_effort && ["none", "low", "medium", "high", "xhigh"].includes(boot.reasoning_effort)) {
            setReasoningEffort(boot.reasoning_effort as ReasoningEffort);
          }

          if (boot.sessions.length > 0) {
            setSessions(
              boot.sessions.map((s) => ({
                id: s.id,
                title: s.title ?? undefined,
                model: s.model ?? undefined,
                source: s.source ?? undefined,
                started_at: s.started_at ?? undefined,
                updated_at: s.ended_at ?? s.started_at ?? undefined,
                message_count: s.message_count,
                tokens_used: s.input_tokens + s.output_tokens,
                preview: s.preview ?? undefined,
                cwd: s.cwd ?? undefined,
                pinned: s.pinned,
                archived: s.archived,
              }))
            );
          }

          if (boot.models.length > 0) {
            setModels(
              boot.models.map((m) => ({
                id: m.id,
                name: m.name,
                provider: m.provider,
                configured: m.configured,
                context_length: m.context_length ?? undefined,
              }))
            );
          }

          if (boot.skills.length > 0) {
            setSkills(boot.skills);
          }
          setSoulMd(boot.soul_md);
          setMemoryMd(boot.memory_md);
          setUserMd(boot.user_md);

          // Hydrate persisted board state + Hermes projects.db projects
          const rawBs = boot.board_state as Partial<PersistedBoardState>;
          const mergedBoards: HermesBoardSummary[] = [
            ...(rawBs.boards && rawBs.boards.length ? rawBs.boards : [DEFAULT_BOARD]),
          ];
          for (const proj of boot.projects) {
            if (!mergedBoards.some((b) => b.slug === proj.slug)) {
              mergedBoards.push({
                slug: proj.slug,
                name: proj.name,
                description: proj.description ?? undefined,
                color: proj.color ?? "violet",
                workspace_path: proj.primary_path ?? "",
              });
            }
          }
          setBoards(mergedBoards);
          if (rawBs.sessionBoards) setSessionBoards(rawBs.sessionBoards);
          if (rawBs.tasks) setTasks(rawBs.tasks);
          if (rawBs.botAvatars) setBotAvatars(rawBs.botAvatars);
          setBoardsHydrated(true);

        } catch { /* Local data is optional; the gateway remains authoritative. */ }
  }, []);

  // A connection is ready only when both authenticated REST and live chat work.
  // Failed candidates leave the current connection and its saved credentials intact.
  const connectToHermes = useCallback(async (conn: HermesConnection, profile = "default") => {
    const attempt = ++connectionAttemptRef.current;
    setNativeHttpAdapter(tauriBridge.createHttpAdapter());
    const c = new HermesClient(conn, profile);
    setConnectionStatus("connecting");
    setConnectionDetail(`Checking ${conn.name}…`);
    const rt = new HermesRealtimeSession(c);
    try {
      const test = await c.testConnection();
      if (!test.ok) {
        const error = new Error(test.error ?? "Could not connect") as Error & { status?: number };
        error.status = test.errorStatus;
        throw error;
      }
      await rt.connect(profile);
      await rt.rpc("gateway.ping", {}, 10_000);
      if (attempt !== connectionAttemptRef.current) { rt.disconnect(); throw new Error("Connection attempt was replaced by a newer one."); }
      const previous = clientRef.current?.getConnection();
      const switched = !previous || previous.id !== conn.id || previous.url !== conn.url;
      rtRef.current?.disconnect();
      for (const store of profileChats.current.values()) store.disconnect();
      liveRoutes.current.clear();
      clientRef.current = c;
      rtRef.current = rt;
      rt.subscribe(event => { if (rtRef.current === rt) handleStreamEvent(event); });
      if (switched) {
        draftSessionIdsRef.current.clear(); activeSessionIdRef.current = null;
        activateChat(profile, true);
        setActiveSessionId(null); setActiveProfileState(profile); setSessions([]);
        setModels([]); setActiveModel(""); setActiveProvider("auto"); setSkills([]); setToolsets([]);
        setMcpServers([]); setCronJobs([]); setMessagingPlatforms([]); setWebhooks([]); setUsagePoints([]);
        setProfiles([{ name: profile, active: true }]); setSoulMd(""); setMemoryMd(""); setUserMd("");
        setSystemStats(null); setBootstrapMs(null); setActiveBoardSlug("all");
        setBoardsHydrated(false);
        let saved: Partial<PersistedBoardState> = {};
        try { saved = JSON.parse(localStorage.getItem(conn.id.startsWith("local-") ? STORAGE_BOARDS_KEY : `${STORAGE_BOARDS_KEY}:${conn.url}`) ?? "{}"); } catch { /* Empty cache. */ }
        setBoards(saved.boards ?? [DEFAULT_BOARD]); setTasks(saved.tasks ?? []);
        setSessionBoards(saved.sessionBoards ?? {}); setBotAvatars(saved.botAvatars ?? {});
      }
      setConnection(conn); setHermesStatus(test.status ?? null);
      if (conn.id === "local-rust") await hydrateLocal();
      setBoardsHydrated(true);
      setConnectionStatus("connected"); setConnectionDetail(conn.name);
      void refreshCapabilities();
      void c.getProfileSoul(profile).then(soul => { if (clientRef.current === c) setSoulMd(soul); }).catch(() => undefined);
    } catch (error) {
      rt.disconnect();
      if (attempt === connectionAttemptRef.current) {
        const previousLive = rtRef.current?.isConnected() && clientRef.current;
        setConnectionStatus(previousLive ? "connected" : "error");
        setConnectionDetail(previousLive ? previousLive.getConnection().name : connectionError(error));
      }
      throw new Error(connectionError(error));
    }
  }, [activateChat, handleStreamEvent, hydrateLocal, refreshCapabilities]);

  const updateConnection = useCallback(async (input: HermesConnection) => {
    const conn = { ...input, url: normalizeServerUrl(input.url), mode: "dashboard" as const };
    await connectToHermes(conn, conn.defaultProfile ?? "default");
    await persistConnection(conn);
    if (isDesktop && conn.id !== "local-rust") {
      await tauriBridge.stopLocalBackend();
      setRustBackend(await tauriBridge.getBackendInfo());
    }
  }, [connectToHermes, persistConnection, isDesktop]);

  const startLocalConnection = useCallback(async () => {
    setConnectionStatus("connecting"); setConnectionDetail("Starting Hermes on this computer…");
    if (isDesktop) {
      await tauriBridge.startLocalBackend();
      for (let i = 0; i < 80; i++) {
        const info = await tauriBridge.getBackendInfo(); setRustBackend(info);
        if (info.status === "ready" && info.url && info.token) {
          await updateConnection({ id: "local-rust", name: "This computer", url: info.url, mode: "dashboard", authType: "session-token", token: info.token });
          return;
        }
        if (info.status === "error") throw new Error(info.error ?? "Hermes could not start. Install Hermes Agent, or connect to a server.");
        await new Promise(resolve => setTimeout(resolve, 400));
      }
      throw new Error("Hermes took too long to start. Check the runtime in Settings, or connect to a server.");
    }
    for (let i = 0; i < 30; i++) {
      const response = await fetch("/__local_hermes").catch(() => null);
      if (response?.ok && response.headers.get("content-type")?.includes("application/json")) {
        const conn = parseSavedConnection(await response.json());
        if (!conn) throw new Error("The local gateway returned an invalid connection.");
        await updateConnection(conn); return;
      }
      if (response?.status !== 503) break;
      await new Promise(resolve => setTimeout(resolve, 400));
    }
    throw new Error("Local Hermes is available in the Linux app. In this preview, start npm run dev:connected or connect to your server.");
  }, [isDesktop, updateConnection]);

  const refreshModels = useCallback(async () => {
    const c = clientRef.current;
    if (!c || refreshingModels) return;
    setRefreshingModels(true);
    try {
      const [freshModels, info] = await Promise.all([
        c.getModelOptions(true, true),
        c.getModelInfo().catch(() => null),
      ]);
      if (clientRef.current === c) {
        if (freshModels.length) setModels(freshModels);
        if (info && !activeSessionIdRef.current) {
          if (typeof info.model === "string" && info.model) setActiveModel(info.model);
          if (typeof info.provider === "string" && info.provider) setActiveProvider(info.provider);
        }
      }
      showToast(`Refreshed ${freshModels.length} models across providers`);
    } catch (error) {
      showToast(connectionError(error));
    } finally {
      setRefreshingModels(false);
    }
  }, [refreshingModels, showToast]);

  const restartLocalBackend = useCallback(async () => {
    if (restartingBackend) return;
    setRestartingBackend(true);
    showToast("Restarting Hermes backend…");
    try {
      if (isLocalConnection) {
        if (isDesktop) {
          await tauriBridge.restartBackend();
        } else if (clientRef.current) {
          await clientRef.current.gatewayAction("restart").catch(() => undefined);
        }
        await startLocalConnection();
      } else {
        const c = clientRef.current;
        if (c) {
          await Promise.allSettled([
            c.gatewayAction("restart"),
            c.rescanDashboardPlugins(),
          ]);
        }
        await new Promise(resolve => setTimeout(resolve, 1200));
        await connectToHermes(connection, activeProfile);
      }
      if (clientRef.current) {
        const freshModels = await clientRef.current.getModelOptions(true, true).catch(() => []);
        if (freshModels.length) setModels(freshModels);
      }
      showToast("Hermes backend restarted & providers reloaded");
    } catch (error) {
      showToast(connectionError(error));
    } finally {
      setRestartingBackend(false);
    }
  }, [activeProfile, connectToHermes, connection, isDesktop, isLocalConnection, restartingBackend, showToast, startLocalConnection]);

  useEffect(() => {
    let cancelled = false;
    async function init() {
      try {
        let saved: HermesConnection | null = null;
        if (isDesktop) saved = parseSavedConnection(await tauriBridge.loadConnection());
        if (!saved) {
          try { saved = parseSavedConnection(JSON.parse(localStorage.getItem(STORAGE_CONN_KEY) ?? "null")); } catch { /* First launch. */ }
        }
        if (cancelled || !saved) return;
        setConnection(saved);
        if (saved.id === "local-rust" || saved.id === "local-dev") {
          await startLocalConnection();
        } else {
          if (isDesktop && saved.refreshToken && (saved.expiresAt ?? 0) <= Date.now() / 1000 + 60) {
            const tokens = await tauriBridge.refreshRemoteLogin(saved);
            saved = { ...saved, token: tokens.access_token, refreshToken: tokens.refresh_token ?? saved.refreshToken, expiresAt: tokens.expires_at, authProvider: tokens.provider };
          }
          if (cancelled) return;
          await updateConnection(saved);
        }
        if (!cancelled) setSetupOpen(false);
      } catch (error) {
        if (!cancelled) { setConnectionStatus("error"); setConnectionDetail(connectionError(error)); setSetupOpen(true); }
      } finally { if (!cancelled) setInitializing(false); }
    }
    void init();
    return () => { cancelled = true; ++connectionAttemptRef.current; rtRef.current?.disconnect(); };
    // Initialization is deliberately once per mount; target changes happen explicitly above.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Rotate HTTP credentials in place; keep live responses and session IDs intact.
  useEffect(() => {
    if (!isDesktop || !connection.refreshToken || !connection.expiresAt) return;
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;
    const currentClient = clientRef.current;
    const rotate = async () => {
      try {
        const tokens = await tauriBridge.refreshRemoteLogin(connection);
        if (cancelled || clientRef.current !== currentClient) return;
        const next = { ...connection, token: tokens.access_token, refreshToken: tokens.refresh_token ?? connection.refreshToken, expiresAt: tokens.expires_at, authProvider: tokens.provider };
        await persistConnection(next);
        if (cancelled || clientRef.current !== currentClient) return;
        currentClient?.setCredentials(next);
        setConnection(next);
      } catch (error) {
        if (cancelled) return;
        if (/expired|rejected sign-in/i.test(String(error))) {
          setConnectionStatus("error"); setConnectionDetail("Server sign-in expired. Open Settings → Connections to sign in again.");
          showToast("Server sign-in expired. Open Connections to sign in again.");
        } else {
          setConnectionDetail("Could not refresh server sign-in. Retrying…");
          timer = setTimeout(() => void rotate(), 30_000);
        }
      }
    };
    timer = setTimeout(() => void rotate(), Math.min(2_147_000_000, Math.max(10_000, connection.expiresAt * 1000 - Date.now() - 60_000)));
    return () => { cancelled = true; clearTimeout(timer); };
  }, [connection, isDesktop, persistConnection, showToast]);

  const refreshAll = useCallback(async () => {
    if (isDesktop && isLocalConnection) {
      const boot = await tauriBridge.loadLocalBootstrap().catch(() => null);
      if (boot && boot.sessions.length > 0 && !clientRef.current) {
        setSessions(
          boot.sessions.map((s) => ({
            id: s.id,
            title: s.title ?? undefined,
            model: s.model ?? undefined,
            source: s.source ?? undefined,
            started_at: s.started_at ?? undefined,
            updated_at: s.ended_at ?? s.started_at ?? undefined,
            message_count: s.message_count,
            preview: s.preview ?? undefined,
            cwd: s.cwd ?? undefined,
            pinned: s.pinned,
          }))
        );
      }
    }
    await refreshCapabilities();
  }, [isDesktop, isLocalConnection, refreshCapabilities]);

  // Commit the selected bot only after the gateway accepts it. Serialize switches
  // so a slower response cannot overwrite a newer profile selection.
  const setActiveProfile = useCallback(async (name: string) => {
    if (profileSwitchRef.current || name === activeProfile) return;
    const c = clientRef.current;
    if (!c) { showToast("Connect to Hermes to switch bots"); return; }
    profileSwitchRef.current = true; setProfileSwitching(true);
    try {
      await c.setActiveProfile(name);
      if (clientRef.current !== c) return;
      c.setProfile(name);
      activateChat(name);
      setActiveProfileState(name); activeSessionIdRef.current = null;
      setActiveSessionId(null); setSessions([]); setActiveView("chat");
      setSoulMd(""); setMemoryMd(""); setUserMd("");
      setProfiles(prev => prev.map(profile => ({ ...profile, active: profile.name === name })));
      if (isDesktop && isLocalConnection) {
        const local = await tauriBridge.loadLocalBootstrap(name).catch(() => null);
        if (local && clientRef.current === c && c.getProfile() === name) {
          setSoulMd(local.soul_md); setMemoryMd(local.memory_md); setUserMd(local.user_md);
          if (local.active_model) setActiveModel(local.active_model);
          if (local.active_provider) setActiveProvider(local.active_provider);
        }
      }
      const soul = await c.getProfileSoul(name).catch(() => "");
      if (clientRef.current !== c || c.getProfile() !== name) return;
      setSoulMd(soul);
      await refreshCapabilities();
      showToast(`Switched to ${name === "default" ? "Hermes" : name}`);
    } catch (error) { showToast(error instanceof Error ? error.message : "Could not switch bot"); }
    finally { profileSwitchRef.current = false; setProfileSwitching(false); }
  }, [activateChat, activeProfile, refreshCapabilities, showToast, isDesktop, isLocalConnection]);

  const updateBotAvatar = useCallback(
    (seed: string, patch: BotAvatarOverride) => {
      setBotAvatars((prev) => {
        const next = { ...prev, [seed]: { ...prev[seed], ...patch } };

        return next;
      });
    },
    [boards, sessionBoards, tasks]
  );

  // Save SOUL.md / MEMORY.md / USER.md
  const saveMemoryFile = useCallback(
    async (kind: "soul" | "memory" | "user", content: string) => {
      try {
        if (kind === "soul" && clientRef.current) {
          await clientRef.current.saveProfileSoul(activeProfile, content);
        } else if (isDesktop && isLocalConnection) {
          await tauriBridge.saveLocalMemoryFile(kind, content, activeProfile);
        } else {
          throw new Error("Editing MEMORY.md and USER.md is available for local connections. Use your server to edit these files.");
        }
        if (kind === "soul") setSoulMd(content);
        if (kind === "memory") setMemoryMd(content);
        if (kind === "user") setUserMd(content);
        showToast(`Saved ${kind.toUpperCase()}.md`);
      } catch (error) {
        showToast(error instanceof Error ? error.message : "Could not save memory file");
      }
    },
    [activeProfile, isDesktop, isLocalConnection, showToast]
  );

  // Session Boards management
  const assignSessionToBoard = useCallback(
    (sessionId: string, boardSlug: string) => {
      setSessionBoards((prev) => {
        const next = { ...prev, [sessionId]: boardSlug };

        return next;
      });
    },
    [boards, botAvatars, tasks]
  );

  const createBoard = useCallback(
    async (payload: { name: string; description?: string; color?: string; workspace_path?: string }) => {
      const slug = slugify(payload.name);
      const board: HermesBoardSummary = {
        slug,
        name: payload.name.trim(),
        description: payload.description?.trim() ?? "",
        color: payload.color ?? "violet",
        workspace_path: payload.workspace_path?.trim() ?? "",
        created_at: Date.now(),
      };
      if (clientRef.current) {
        await clientRef.current
          .createKanbanBoard({ name: board.name, slug: board.slug, description: board.description, color: board.color, workspace_path: board.workspace_path })
          .catch(() => undefined);
      }
      if (rtRef.current?.isConnected() && board.workspace_path) {
        await rtRef.current
          .createProject({
            name: board.name,
            description: board.description,
            color: board.color,
            primary_path: board.workspace_path,
            board_slug: board.slug,
          })
          .catch(() => undefined);
      }
      setBoards((prev) => {
        const next = [...prev.filter((b) => b.slug !== board.slug), board];

        return next;
      });
      setActiveBoardSlug(board.slug);
      showToast(`Created board "${board.name}"`);
      return board;
    },
    [botAvatars, sessionBoards, showToast, tasks]
  );

  const deleteBoard = useCallback(
    async (slug: string) => {
      if (slug === "default") return;
      if (clientRef.current) {
        await clientRef.current.deleteKanbanBoard(slug).catch(() => undefined);
      }
      setBoards((prev) => {
        const next = prev.filter((b) => b.slug !== slug);

        return next;
      });
      if (activeBoardSlug === slug) setActiveBoardSlug("all");
      showToast(`Deleted board "${slug}"`);
    },
    [activeBoardSlug, botAvatars, sessionBoards, showToast, tasks]
  );

  // Kanban Task CRUD
  const createTask = useCallback(
    async (payload: {
      title: string;
      body?: string;
      status?: HermesKanbanStatus;
      priority?: string;
      assignee?: string;
      model?: string;
      provider?: string;
      board?: string;
      cwd?: string;
    }) => {
      const boardSlug = payload.board && payload.board !== "all" ? payload.board : activeBoardSlug !== "all" ? activeBoardSlug : "default";
      let serverId: string | undefined;
      if (clientRef.current) {
        const res = await clientRef.current
          .createKanbanTask(
            {
              title: payload.title,
              body: payload.body,
              status: payload.status ?? "todo",
              priority: payload.priority ?? "0",
              assignee: payload.assignee ?? activeProfile,
              model: payload.model ?? activeModel,
              provider: payload.provider ?? activeProvider,
              cwd: payload.cwd,
            },
            boardSlug
          )
          .catch(() => null);
        const taskObj = res && typeof res === "object" ? (res.task as Record<string, unknown> | undefined) : undefined;
        if (taskObj?.id) serverId = String(taskObj.id);
      }

      const task: HermesKanbanTask = {
        id: serverId ?? `task_${Date.now().toString(36)}`,
        board: boardSlug,
        title: payload.title.trim(),
        body: payload.body?.trim() ?? "",
        status: payload.status ?? "todo",
        priority: payload.priority ?? "0",
        assignee: payload.assignee ?? activeProfile,
        model: payload.model ?? activeModel,
        provider: payload.provider ?? activeProvider,
        cwd: payload.cwd,
        created_at: Date.now(),
        updated_at: Date.now(),
      };

      setTasks((prev) => {
        const next = [task, ...prev];

        return next;
      });
      showToast(`Added task "${task.title}"`);
      return task;
    },
    [activeBoardSlug, activeModel, activeProvider, activeProfile, boards, botAvatars, sessionBoards, showToast]
  );

  const updateTask = useCallback(
    async (taskId: string, patch: Partial<HermesKanbanTask>) => {
      const target = tasks.find((t) => t.id === taskId);
      if (clientRef.current) {
        await clientRef.current.updateKanbanTask(taskId, patch, target?.board).catch(() => undefined);
      }
      setTasks((prev) => {
        const next = prev.map((t) => (t.id === taskId ? { ...t, ...patch, updated_at: Date.now() } : t));

        return next;
      });
    },
    [boards, botAvatars, sessionBoards, tasks]
  );

  const deleteTask = useCallback(
    async (taskId: string) => {
      const target = tasks.find((t) => t.id === taskId);
      if (clientRef.current) {
        await clientRef.current.deleteKanbanTask(taskId, target?.board).catch(() => undefined);
      }
      setTasks((prev) => {
        const next = prev.filter((t) => t.id !== taskId);

        return next;
      });
      showToast("Task deleted");
    },
    [boards, botAvatars, sessionBoards, showToast, tasks]
  );

  const dispatchBoardWorkers = useCallback(
    async (boardSlug?: string) => {
      if (!clientRef.current) {
        showToast("Connect to Hermes backend to dispatch workers");
        return;
      }
      try {
        await clientRef.current.dispatchKanban(boardSlug && boardSlug !== "all" ? boardSlug : undefined);
        showToast("Dispatched Hermes board workers");
        await refreshCapabilities();
      } catch (err) {
        showToast(err instanceof Error ? err.message : "Dispatch failed");
      }
    },
    [refreshCapabilities, showToast]
  );

  // Paint the cached transcript immediately and reconcile with the gateway.
  const selectSession = useCallback(
    async (sessionId: string | null) => {
      activeSessionIdRef.current = sessionId;
      setActiveSessionId(sessionId);
      const selected = sessions.find(session => session.id === sessionId);
      if (selected?.model) setActiveModel(selected.model);
      if (selected?.provider) setActiveProvider(selected.provider);
      setActiveView("chat");
      if (!sessionId) return;
      chat.retain(sessionId);
      const cached = chat.get(sessionId);
      if (cached.isStreaming || cached.loadingHistory) return;

      const client = clientRef.current;
      const realtime = rtRef.current;
      const original = chat.get(sessionId).messages;
      let baseline = original;
      if (!original.length) chat.patch(sessionId, { loadingHistory: true });
      const mayHydrate = () => currentChat.current === chat && clientRef.current === client
        && (!client || client.getProfile() === activeProfile) && !chat.get(sessionId).isStreaming;

      const local = async () => {
        if (!isDesktop || !isLocalConnection || original.length) return;
        try {
          const rows = await tauriBridge.loadLocalSessionMessages(sessionId, client ? HISTORY_PAGE_SIZE : 500, activeProfile);
          if (mayHydrate() && rows.length && chat.get(sessionId).messages === original) {
            baseline = rows.map(row => ({
              id: row.id,
              role: row.role === "user" || row.role === "system" || row.role === "tool" ? row.role : "assistant",
              content: row.content,
              timestamp: row.timestamp ?? undefined,
              model: row.model ?? undefined,
            }));
            chat.updateMessages(sessionId, () => baseline);
          }
        } catch { /* Gateway history remains available. */ }
      };
      const remote = async () => {
        if (!client) return;
        try {
          const incoming = await client.getSessionMessages(sessionId, HISTORY_PAGE_SIZE);
          if (mayHydrate() && incoming.length && chat.get(sessionId).messages === baseline) {
            const previous = new Map(baseline.map(message => [message.id, message]));
            chat.updateMessages(sessionId, displayed => {
              const first = incoming[0].id;
              const overlap = displayed.findIndex(message => message.id === first);
              const older = overlap >= 0 ? displayed.slice(0, overlap) : [];
              return [...older, ...incoming.map(message => {
              const cached = previous.get(message.id);
              return cached && cached.role === message.role && cached.content === message.content &&
                cached.timestamp === message.timestamp && cached.model === message.model &&
                cached.thinking === message.thinking && cached.toolName === message.toolName &&
                !cached.toolCalls?.length && !message.toolCalls?.length && !cached.approval && !cached.clarify
                ? cached : message;
              })];
            });
            const knownOffset = cached.olderOffset;
            if (incoming.length >= HISTORY_PAGE_SIZE && original.length <= HISTORY_PAGE_SIZE && knownOffset === null) {
              chat.patch(sessionId, { olderOffset: HISTORY_PAGE_SIZE });
            }
          }
        } catch { /* Keep the cached or local transcript. */ }
      };
      const resume = async () => {
        if (!realtime?.isConnected() || chat.liveIds[sessionId]) return;
        try {
          const result = await realtime.resumeSession(sessionId, { profile: activeProfile, omit_messages: true });
          if (rtRef.current === realtime && typeof result.session_id === "string") {
            registerLiveSession(chat, sessionId, result.session_id);
          }
        } catch { /* Drafts may not be in the gateway yet. */ }
      };
      void resume();
      try { await Promise.all([local(), remote()]); }
      finally { chat.patch(sessionId, { loadingHistory: false }); }
    },
    [activeProfile, chat, isDesktop, isLocalConnection, registerLiveSession, sessions]
  );

  const loadOlderMessages = useCallback(async () => {
    const id = activeSessionId;
    const client = clientRef.current;
    if (!id || !client || currentChat.current !== chat) return;
    const { olderOffset, loadingHistory } = chat.get(id);
    if (olderOffset === null || loadingHistory) return;
    chat.patch(id, { loadingHistory: true });
    try {
      const messages = await client.getSessionMessages(id, HISTORY_PAGE_SIZE, olderOffset);
      if (clientRef.current === client && client.getProfile() === activeProfile) {
        chat.prepend(id, messages, messages.length >= HISTORY_PAGE_SIZE ? olderOffset + messages.length : null);
      }
    } catch (error) {
      if (currentChat.current === chat) showToast(error instanceof Error ? error.message : "Could not load older messages");
    } finally {
      chat.patch(id, { loadingHistory: false });
    }
  }, [activeSessionId, activeProfile, chat, showToast]);

  const createNewSession = useCallback(
    async (opts?: { title?: string; boardSlug?: string; cwd?: string; model?: string; provider?: string }) => {
      const boardSlug = opts?.boardSlug ?? (activeBoardSlug !== "all" ? activeBoardSlug : "default");
      const board = boards.find((b) => b.slug === boardSlug);
      const cwd = opts?.cwd ?? board?.workspace_path ?? undefined;
      const model = opts?.model ?? activeModel;
      const provider = opts?.provider ?? activeProvider;

      let newId = `session_${Date.now().toString(36)}`;
      const realtime = rtRef.current;
      if (realtime?.isConnected()) {
        try {
          const res = await realtime.createSession({
            profile: activeProfile,
            model,
            provider: provider !== "auto" ? provider : undefined,
            reasoning_effort: reasoningEffort,
            fast: fastMode,
            cwd: cwd || undefined,
            title: opts?.title,
            source: "desktop",
          });
          const sid = String(res.stored_session_id ?? res.session_key ?? res.session_id ?? newId);
          newId = sid;
          if (rtRef.current === realtime && typeof res.session_id === "string") {
            registerLiveSession(chat, newId, res.session_id);
          }
        } catch {
          // fall back to local draft id
        }
      }

      if (currentChat.current !== chat || rtRef.current !== realtime) return null;
      const newSession: HermesSession = {
        id: newId,
        title: opts?.title ?? "New Chat",
        profile: activeProfile,
        model,
        provider,
        source: "desktop",
        cwd,
        board_slug: boardSlug,
        message_count: 0,
        created_at: Date.now(),
        updated_at: Date.now(),
      };

      draftSessionIdsRef.current.add(newId);
      setSessions((prev) => [newSession, ...prev.filter((s) => s.id !== newId)]);
      assignSessionToBoard(newId, boardSlug);
      setActiveModel(model);
      setActiveProvider(provider);
      activeSessionIdRef.current = newId;
      setActiveSessionId(newId);
      setActiveView("chat");
      return newId;
    },
    [activeBoardSlug, activeModel, activeProfile, activeProvider, assignSessionToBoard, boards, chat, fastMode, reasoningEffort, registerLiveSession]
  );

  useEffect(() => {
    const onNew = (event: KeyboardEvent) => {
      if (setupOpen) return;
      if ((event.target as HTMLElement)?.closest?.(".xterm")) return;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "n" && !event.shiftKey) {
        event.preventDefault(); void createNewSession();
      }
    };
    window.addEventListener("keydown", onNew);
    return () => window.removeEventListener("keydown", onNew);
  }, [createNewSession, setupOpen]);

  const launchSessionForTask = useCallback(
    async (task: HermesKanbanTask) => {
      if (task.session_id && sessions.some((s) => s.id === task.session_id)) {
        await selectSession(task.session_id);
        return;
      }
      const sid = await createNewSession({
        title: task.title,
        boardSlug: task.board ?? "default",
        cwd: task.cwd,
        model: task.model,
        provider: task.provider,
      });
      if (sid) {
        await updateTask(task.id, { session_id: sid });
        const intro = task.body ? `Work on task: **${task.title}**\n\n${task.body}` : `Work on task: **${task.title}**`;
        // Seed prompt automatically if connected
        if (rtRef.current?.isConnected()) {
          const liveSid = chat.liveIds[sid] ?? sid;
          chat.updateMessages(sid, () => [{ id: `u-${Date.now()}`, role: "user", content: intro, timestamp: Date.now() }]);
          chat.patch(sid, { isStreaming: true });
          try {
            await rtRef.current.submitPrompt(liveSid, intro);
            await updateTask(task.id, { status: "running" });
          } catch (error) {
            chat.patch(sid, { isStreaming: false });
            showToast(error instanceof Error ? error.message : "Task could not start");
          }
        }
      }
    },
    [chat, createNewSession, selectSession, sessions, updateTask, showToast]
  );

  const sendMessage = useCallback(
    async (rawText: string, attachments: File[] = []) => {
      const realtime = rtRef.current;
      const text = rawText.trim() || (attachments.length ? "Please review the attached files." : "");
      if (!text) return;
      if (!realtime?.isConnected() && connection.mode !== "openai") {
        throw new Error("Hermes is disconnected. Your draft is kept; reconnect in Settings to send it.");
      }
      if (attachments.length && connection.mode === "openai") throw new Error("File attachments require a Hermes gateway connection.");

      let sid = activeSessionId;
      if (!sid) {
        sid = await createNewSession({ title: text.slice(0, 48) });
      }
      if (!sid || currentChat.current !== chat || rtRef.current !== realtime) {
        throw new Error("The conversation changed before the message could be sent.");
      }
      if (chat.get(sid).isStreaming) throw new Error("Wait for the current response before sending another message.");

      const userMsg: HermesMessage = {
        id: `user-${Date.now()}`,
        role: "user",
        content: text,
        timestamp: Date.now(),
      };

      chat.updateMessages(sid, messages => [...messages, userMsg]);

      setSessions((prev) =>
        prev.map((s) =>
          s.id === sid
            ? {
                ...s,
                title: !s.title || s.title === "New Chat" ? text.slice(0, 52) : s.title,
                preview: text.slice(0, 100),
                updated_at: Date.now(),
                message_count: (s.message_count ?? 0) + 1,
              }
            : s
        )
      );

      // Handle slash commands if connected
      if (text.startsWith("/") && realtime?.isConnected()) {
        const liveSid = chat.liveIds[sid] ?? sid;
        try {
          const res = await realtime.dispatchCommand(liveSid, text);
          if (res && typeof res.output === "string" && res.output) {
            chat.updateMessages(sid, messages => [
                ...messages,
                { id: `cmd-${Date.now()}`, role: "assistant", content: res.output as string, timestamp: Date.now() },
              ]);
            return;
          }
        } catch {
          // fall through to normal prompt.submit
        }
      }

      // Live WebSocket JSON-RPC submit
      if (realtime?.isConnected()) {
        chat.patch(sid, { isStreaming: true });
        chat.patch(sid, { liveStatusText: "Thinking..." });
        try {
          let liveSid = chat.liveIds[sid];
          if (!liveSid && !sid.startsWith("session_")) {
            const resumed = await realtime.resumeSession(sid, { profile: activeProfile, omit_messages: true });
            liveSid = String(resumed.session_id ?? sid);
            if (rtRef.current !== realtime) throw new Error("Connection changed while sending");
            registerLiveSession(chat, sid, liveSid);
          }
          if (!liveSid) {
            const created = await realtime.createSession({
              profile: activeProfile,
              model: activeModel,
              provider: activeProvider !== "auto" ? activeProvider : undefined,
              reasoning_effort: reasoningEffort,
              fast: fastMode,
              source: "desktop",
              cwd: activeSession?.cwd,
              title: activeSession?.title,
            });
            liveSid = String(created.session_id ?? sid);
            if (rtRef.current !== realtime) throw new Error("Connection changed while sending");
            registerLiveSession(chat, sid, liveSid);
          }
          await realtime.setSessionConfig(liveSid, {
            reasoning_effort: reasoningEffort, fast: fastMode, yolo: yoloMode,
          });
          let prompt = text;
          for (const file of attachments) {
            const dataUrl = await new Promise<string>((resolve, reject) => {
              const reader = new FileReader();
              reader.onload = () => resolve(String(reader.result));
              reader.onerror = () => reject(new Error(`Could not read ${file.name}`));
              reader.readAsDataURL(file);
            });
            if (file.type.startsWith("image/")) {
              await realtime.rpc("image.attach_bytes", { session_id: liveSid, content_base64: dataUrl, filename: file.name });
            } else if (file.type === "application/pdf") {
              await realtime.rpc("pdf.attach", { session_id: liveSid, content_base64: dataUrl, filename: file.name });
            } else {
              const attached = await realtime.rpc<Record<string, unknown>>("file.attach", { session_id: liveSid, data_url: dataUrl, name: file.name });
              prompt += `\n${attached.ref_text ?? ""}`;
            }
          }
          await realtime.submitPrompt(liveSid, prompt);
          if (currentChat.current === chat) draftSessionIdsRef.current.delete(sid);
        } catch (err) {
          chat.finish(sid);
          const msg = err instanceof Error ? err.message : String(err);
          chat.updateMessages(sid, messages => [
              ...messages,
              {
                id: `err-${Date.now()}`,
                role: "assistant",
                content: `Could not deliver message to Hermes backend: ${msg}`,
                timestamp: Date.now(),
              },
            ]);
          throw err;
        }
        return;
      }

      // OpenAI SSE fallback if configured in openai mode
      if (connection.mode === "openai" && realtime) {
        chat.patch(sid, { isStreaming: true });
        const assistant = newAssistant();
        const history = chat.get(sid).messages;
        chat.updateMessages(sid, messages => [...messages, assistant]);
        const controllers = sseControllers.current.get(chat) ?? new Map<string, AbortController>();
        const controller = new AbortController();
        controllers.set(sid, controller);
        sseControllers.current.set(chat, controllers);
        try {
          await realtime.streamOpenAIChat({
            model: activeModel,
            messages: history,
            sessionKey: sid,
            signal: controller.signal,
            onDelta: delta => chat.buffer(sid, delta, "text", assistant.id),
          });
        } catch (err) {
          if (!controller.signal.aborted && currentChat.current === chat) {
            showToast(err instanceof Error ? err.message : "Stream error");
          }
        } finally {
          controllers.delete(sid);
          if (!controllers.size) sseControllers.current.delete(chat);
          chat.finish(sid, undefined, assistant.id);
        }
        return;
      }

      // Informative offline fallback when backend isn't ready yet
      chat.updateMessages(sid, messages => [
          ...messages,
          {
            id: `info-${Date.now()}`,
            role: "assistant",
            content:
              "Hermes backend is not connected yet. Check **Settings → Connections** or wait for the local Rust-managed `hermes serve` process to finish starting.",
            timestamp: Date.now(),
          },
        ]);
    },
    [
      activeModel,
      activeProfile,
      activeProvider,
      activeSessionId,
      activeSession?.cwd,
      activeSession?.title,
      connection.mode,
      createNewSession,
      fastMode,
      chat,
      registerLiveSession,
      reasoningEffort,
      yoloMode,
      showToast,
    ]
  );

  const interruptStream = useCallback(async () => {
    if (!activeSessionId) return;
    const controller = sseControllers.current.get(chat)?.get(activeSessionId);
    if (controller) {
      controller.abort();
      return;
    }
    const liveSid = chat.liveIds[activeSessionId] ?? activeSessionId;
    try {
      await rtRef.current?.interruptSession(liveSid);
      chat.finish(activeSessionId);
    } catch (error) {
      showToast(error instanceof Error ? error.message : "Could not interrupt response");
    }
  }, [activeSessionId, chat, showToast]);

  const renameSession = useCallback(
    async (sessionId: string, title: string) => {
      const clean = title.trim();
      if (!clean) return;
      setSessions((prev) => prev.map((s) => (s.id === sessionId ? { ...s, title: clean } : s)));
      if (clientRef.current) {
        await clientRef.current.renameSession(sessionId, clean, activeProfile).catch(() => undefined);
      }
      if (rtRef.current?.isConnected()) {
        const liveSid = chat.liveIds[sessionId] ?? sessionId;
        await rtRef.current.setSessionTitle(liveSid, clean).catch(() => undefined);
      }
    },
    [activeProfile, chat]
  );

  const deleteSession = useCallback(
    async (sessionId: string) => {
      setSessions((prev) => prev.filter((s) => s.id !== sessionId));
      if (activeSessionId === sessionId) setActiveSessionId(null);
      if (clientRef.current) {
        await clientRef.current.deleteSession(sessionId, activeProfile).catch(() => undefined);
      }
      showToast("Session deleted");
    },
    [activeProfile, activeSessionId, showToast]
  );

  const archiveSession = useCallback(async (sessionId: string, archived: boolean) => {
    try {
      if (clientRef.current) await clientRef.current.archiveSession(sessionId, archived, activeProfile);
      setSessions(prev => prev.map(session => session.id === sessionId ? { ...session, archived } : session));
      if (archived && activeSessionId === sessionId) setActiveSessionId(null);
      showToast(archived ? "Session archived" : "Session restored");
    } catch (error) { showToast(error instanceof Error ? error.message : "Could not archive session"); }
  }, [activeProfile, activeSessionId, showToast]);

  const togglePinSession = useCallback(
    async (sessionId: string) => {
      const target = sessions.find((s) => s.id === sessionId);
      const nextPinned = !target?.pinned;
      setSessions((prev) => prev.map((s) => (s.id === sessionId ? { ...s, pinned: nextPinned } : s)));
      if (clientRef.current) {
        await clientRef.current.pinSession(sessionId, nextPinned, activeProfile).catch(() => undefined);
      }
    },
    [activeProfile, sessions]
  );

  const compressActiveSession = useCallback(async () => {
    if (!activeSessionId || !rtRef.current?.isConnected()) {
      showToast("Connect to Hermes to compress session context");
      return;
    }
    const liveSid = chat.liveIds[activeSessionId] ?? activeSessionId;
    try {
      await rtRef.current.compressSession(liveSid);
      showToast("Context compressed");
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Compress failed");
    }
  }, [activeSessionId, chat, showToast]);

  const branchActiveSession = useCallback(async () => {
    if (!activeSessionId) return;
    if (rtRef.current?.isConnected()) {
      const liveSid = chat.liveIds[activeSessionId] ?? activeSessionId;
      try {
        const res = await rtRef.current.branchSession(liveSid);
        const newSid = String(res.stored_session_id ?? res.session_key ?? res.session_id ?? "");
        if (newSid) {
          await refreshCapabilities();
          await selectSession(newSid);
          showToast("Branched session");
          return;
        }
      } catch {
        // fallback below
      }
    }
    await createNewSession({ title: `${activeSession?.title ?? "Chat"} (branch)` });
    showToast("Branched session");
  }, [activeSession?.title, activeSessionId, chat, createNewSession, refreshCapabilities, selectSession, showToast]);

  const respondApproval = useCallback(
    async (approvalId: string, choice: "once" | "always" | "deny") => {
      const sid = activeSessionId ?? "draft";
      const settled: "allowed" | "always" | "denied" =
        choice === "deny" ? "denied" : choice === "always" ? "always" : "allowed";
      chat.updateMessages(sid, messages => messages.map((m) =>
          m.approval?.id === approvalId ? { ...m, approval: { ...m.approval, settled } } : m
        ));
      if (rtRef.current?.isConnected()) {
        const liveSid = chat.liveIds[sid] ?? sid;
        await rtRef.current.respondApproval(liveSid, approvalId, choice).catch(() => undefined);
      }
    },
    [activeSessionId, chat]
  );

  const respondClarify = useCallback(
    async (clarifyId: string, answer: string) => {
      const sid = activeSessionId ?? "draft";
      chat.updateMessages(sid, messages => messages.map((m) =>
          m.clarify?.id === clarifyId ? { ...m, clarify: { ...m.clarify, answered: answer } } : m
        ));
      if (rtRef.current?.isConnected()) {
        const liveSid = chat.liveIds[sid] ?? sid;
        await rtRef.current.respondClarify(liveSid, clarifyId, answer).catch(() => undefined);
      }
    },
    [activeSessionId, chat]
  );

  // Per-session selection, with an explicit option to change the Hermes default.
  const selectModel = useCallback(
    async (model: string, provider?: string, saveAsDefault = false) => {
      try {
        if (activeSessionId && rtRef.current?.isConnected()) {
          const liveSid = chat.liveIds[activeSessionId] ?? activeSessionId;
          await rtRef.current.setSessionConfig(liveSid, { model, provider });
        }
        if (saveAsDefault && clientRef.current) {
          await clientRef.current.setModel({ model, provider: provider !== "auto" ? provider : undefined });
        }
        setActiveModel(model);
        if (provider) setActiveProvider(provider);
        setSessions(prev => prev.map(s => s.id === activeSessionId ? { ...s, model, provider: provider ?? activeProvider } : s));
        showToast(`Model set to ${model}`);
      } catch (err) {
        showToast(err instanceof Error ? err.message : "Model change failed");
        throw err;
      }
    },
    [activeSessionId, activeProvider, chat, showToast]
  );

  const value: AppContextValue = {
    theme,
    setTheme,
    toggleTheme,
    activeView,
    setActiveView,
    sidebarCollapsed,
    setSidebarCollapsed,
    rightPaneOpen,
    setRightPaneOpen,
    rightPaneTab,
    setRightPaneTab,
    rightPaneWidth,
    setRightPaneWidth,
    commandPaletteOpen,
    setCommandPaletteOpen,
    modelPickerOpen,
    setModelPickerOpen,
    avatarEditorTarget,
    setAvatarEditorTarget,
    isDesktop,
    isLocalConnection, setupOpen, setSetupOpen, initializing, startLocalConnection,
    rustBackend,
    connection,
    connectionStatus,
    connectionDetail,
    hermesStatus,
    systemStats,
    bootstrapMs,
    client: clientRef.current,
    updateConnection,
    restartingBackend,
    refreshingModels,
    restartLocalBackend,
    refreshModels,
    refreshAll,
    profiles,
    profileSwitching,
    activeProfile,
    setActiveProfile,
    botAvatars,
    updateBotAvatar,
    soulMd,
    memoryMd,
    userMd,
    saveMemoryFile,
    boards,
    activeBoardSlug,
    setActiveBoardSlug,
    sessionBoards,
    assignSessionToBoard,
    createBoard,
    deleteBoard,
    tasks,
    createTask,
    updateTask,
    deleteTask,
    dispatchBoardWorkers,
    launchSessionForTask,
    sessions,
    activeSessionId,
    activeSession,
    chat,
    loadOlderMessages,
    selectSession,
    createNewSession,
    sendMessage,
    interruptStream,
    renameSession,
    deleteSession,
    archiveSession,
    togglePinSession,
    compressActiveSession,
    branchActiveSession,
    respondApproval,
    respondClarify,
    models,
    activeModel,
    activeProvider,
    reasoningEffort,
    fastMode,
    yoloMode,
    selectModel,
    setReasoningEffort,
    setFastMode,
    setYoloMode,
    skills,
    toolsets,
    mcpServers,
    cronJobs,
    messagingPlatforms,
    webhooks,
    usagePoints,
    refreshCapabilities,
    toast,
    showToast,
  };

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
};

export function useApp(): AppContextValue {
  const ctx = useContext(AppContext);
  if (!ctx) throw new Error("useApp must be used inside AppProvider");
  return ctx;
}

export function useChatSlice<T>(select: (snapshot: ReturnType<ChatStore["get"]>) => T): T {
  const { activeSessionId, chat } = useApp();
  const subscribe = useCallback((listener: () => void) => chat.subscribe(activeSessionId, listener), [activeSessionId, chat]);
  return useSyncExternalStore(subscribe, () => select(chat.get(activeSessionId)));
}

export function useChatMessage(messageId: string) {
  const { activeSessionId, chat } = useApp();
  const subscribe = useCallback((listener: () => void) => chat.subscribe(activeSessionId, listener), [activeSessionId, chat]);
  return useSyncExternalStore(subscribe, () => chat.getMessage(activeSessionId, messageId));
}
