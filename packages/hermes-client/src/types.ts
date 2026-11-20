export type HermesServerMode = "dashboard" | "openai";
export type HermesAuthType = "none" | "session-token" | "bearer" | "cookie";

export interface HermesSshTarget {
  host: string;
  user?: string;
  port?: number;
  identityFile?: string;
  configFile?: string;
}

export interface HermesConnection {
  id: string;
  name: string;
  url: string;
  mode: HermesServerMode;
  authType: HermesAuthType;
  token?: string;
  refreshToken?: string;
  expiresAt?: number;
  authProvider?: string;
  useProxy?: boolean;
  proxyBaseUrl?: string;
  defaultProfile?: string;
  ssh?: HermesSshTarget;
}

export interface HermesStatus {
  ok?: boolean;
  version?: string;
  gateway_running?: boolean;
  gateway_state?: string;
  gateway_pid?: number | null;
  active_profile?: string;
  active_sessions?: number;
  model?: string;
  provider?: string;
  uptime_seconds?: number;
  platforms?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface HermesSystemStats {
  cpu_percent?: number;
  memory_percent?: number;
  memory_used_mb?: number;
  memory_total_mb?: number;
  disk_percent?: number;
  disk_used_gb?: number;
  disk_total_gb?: number;
  hostname?: string;
  platform?: string;
  [key: string]: unknown;
}

export interface HermesProfile {
  name: string;
  active?: boolean;
  default?: boolean;
  description?: string;
  model?: string;
  provider?: string;
  path?: string;
  session_count?: number;
  skill_count?: number;
  created_at?: string;
  [key: string]: unknown;
}

export interface HermesSession {
  id: string;
  title?: string;
  name?: string;
  profile?: string;
  model?: string;
  provider?: string;
  source?: string;
  message_count?: number;
  created_at?: string | number;
  updated_at?: string | number;
  started_at?: string | number;
  last_active?: string | number;
  last_message?: string;
  preview?: string;
  tokens_used?: number;
  context_limit?: number;
  workspace?: string;
  cwd?: string;
  branch?: string;
  pinned?: boolean;
  archived?: boolean;
  board_slug?: string;
  project_id?: string;
  [key: string]: unknown;
}

export interface HermesToolCall {
  id: string;
  name: string;
  arguments?: string | Record<string, unknown>;
  output?: string;
  status?: "running" | "done" | "error";
  durationMs?: number;
  preview?: string;
}

export interface HermesApprovalRequest {
  id: string;
  tool?: string;
  title: string;
  command?: string;
  detail?: string;
  settled?: "allowed" | "always" | "denied" | null;
}

export interface HermesClarifyRequest {
  id: string;
  question: string;
  choices?: string[];
  answered?: string | null;
}

export interface HermesMessage {
  id: string;
  role: "user" | "assistant" | "system" | "tool";
  content: string;
  thinking?: string;
  timestamp?: string | number;
  model?: string;
  toolCalls?: HermesToolCall[];
  approval?: HermesApprovalRequest;
  clarify?: HermesClarifyRequest;
  streaming?: boolean;
}

export interface HermesModelOption {
  id: string;
  name?: string;
  provider: string;
  configured?: boolean;
  context_length?: number;
  supports_vision?: boolean;
  supports_tools?: boolean;
  supports_reasoning?: boolean;
  pricing?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface HermesOAuthProvider {
  id: string;
  name: string;
  connected: boolean;
  account?: string;
  expires_at?: string;
  [key: string]: unknown;
}

export interface HermesSkill {
  name: string;
  path?: string;
  category?: string;
  description?: string;
  enabled: boolean;
  source?: string;
  [key: string]: unknown;
}

export interface HermesToolset {
  name: string;
  description?: string;
  enabled: boolean;
  tools?: string[];
  provider?: string;
  [key: string]: unknown;
}

export interface HermesMcpServer {
  name: string;
  command?: string;
  args?: string[];
  url?: string;
  transport?: string;
  enabled?: boolean;
  status?: string;
  env?: Record<string, string>;
  [key: string]: unknown;
}

export interface HermesCronJob {
  id: string;
  name: string;
  schedule: string;
  prompt: string;
  enabled?: boolean;
  paused?: boolean;
  profile?: string;
  delivery_target?: string;
  last_run?: string;
  next_run?: string;
  status?: string;
  [key: string]: unknown;
}

export interface HermesMessagingPlatform {
  id: string;
  name: string;
  enabled?: boolean;
  configured?: boolean;
  connected?: boolean;
  status?: string;
  config?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface HermesWebhook {
  id: string;
  name?: string;
  url: string;
  events?: string[];
  enabled?: boolean;
  secret?: string;
  [key: string]: unknown;
}

export interface HermesFileEntry {
  name: string;
  path: string;
  is_dir: boolean;
  size?: number;
  modified?: string | number;
}

export interface HermesEnvEntry {
  key: string;
  value?: string;
  masked?: string;
  is_set?: boolean;
  secret?: boolean;
}

export interface HermesLogEntry {
  timestamp?: string;
  level?: string;
  source?: string;
  message: string;
}

export interface HermesUsagePoint {
  date: string;
  tokens?: number;
  input_tokens?: number;
  output_tokens?: number;
  cost_usd?: number;
  requests?: number;
}

// Session Boards & Kanban (Hermes Kanban plugin + Projects)
export type HermesKanbanStatus =
  | "triage"
  | "todo"
  | "scheduled"
  | "ready"
  | "running"
  | "blocked"
  | "review"
  | "done"
  | "archived";

export interface HermesBoardSummary {
  slug: string;
  name: string;
  description?: string;
  color?: string;
  icon?: string;
  workspace_path?: string;
  active?: boolean;
  task_count?: number;
  session_count?: number;
  created_at?: string | number;
  [key: string]: unknown;
}

export interface HermesKanbanTask {
  id: string;
  board?: string;
  title: string;
  body?: string;
  description?: string;
  status: HermesKanbanStatus;
  priority?: "low" | "medium" | "high" | "urgent" | number | string;
  assignee?: string;
  profile?: string;
  model?: string;
  provider?: string;
  cwd?: string;
  session_id?: string;
  tags?: string[];
  comments_count?: number;
  created_at?: string | number;
  updated_at?: string | number;
  [key: string]: unknown;
}

export interface HermesProject {
  id: string;
  slug: string;
  name: string;
  description?: string;
  icon?: string;
  color?: string;
  board_slug?: string;
  primary_path?: string;
  folders?: Array<{ path: string; label?: string; is_primary?: boolean }>;
  created_at?: number;
  archived?: boolean;
  [key: string]: unknown;
}

export interface HermesLearningNode {
  id: string;
  label: string;
  kind: "memory" | "skill" | "session" | "topic" | string;
  summary?: string;
  content?: string;
  created_at?: string | number;
  [key: string]: unknown;
}

export interface HermesLearningGraph {
  nodes: HermesLearningNode[];
  edges: Array<{ source: string; target: string; label?: string }>;
}

export interface HermesSessionUsage {
  input_tokens?: number;
  output_tokens?: number;
  total_tokens?: number;
  context_tokens?: number;
  context_limit?: number;
  cost_usd?: number;
  [key: string]: unknown;
}

export interface HermesTodoItem {
  id?: string;
  content: string;
  status: "pending" | "in_progress" | "completed" | "cancelled" | string;
}

export type HermesStreamEvent =
  | { type: "status"; status: "connecting" | "connected" | "disconnected" | "error"; detail?: string }
  | { type: "gateway.ready"; payload?: Record<string, unknown> }
  | { type: "session.ready"; sessionId: string; info?: Record<string, unknown> }
  | { type: "session.info"; sessionId?: string; info: Record<string, unknown> }
  | { type: "session.usage"; sessionId?: string; usage: HermesSessionUsage }
  | { type: "status.update"; sessionId?: string; text: string }
  | { type: "message.start"; sessionId?: string; streamId?: string }
  | { type: "text.delta"; sessionId?: string; delta: string }
  | { type: "thinking.delta"; sessionId?: string; delta: string }
  | { type: "message.interim"; sessionId?: string; content: string }
  | { type: "tool.start"; sessionId?: string; toolCall: HermesToolCall }
  | { type: "tool.progress"; sessionId?: string; toolCallId: string; preview?: string }
  | { type: "tool.end"; sessionId?: string; toolCallId: string; output?: string; error?: string; durationMs?: number }
  | { type: "todo.updated"; sessionId?: string; todos: HermesTodoItem[] }
  | { type: "approval.request"; sessionId?: string; approval: HermesApprovalRequest }
  | { type: "approval.settled"; sessionId?: string; id: string; decision: "allowed" | "always" | "denied" }
  | { type: "clarify.request"; sessionId?: string; clarify: HermesClarifyRequest }
  | { type: "clarify.settled"; sessionId?: string; id: string; answer: string }
  | { type: "message.complete"; sessionId?: string; content?: string }
  | { type: "error"; sessionId?: string; message: string };
