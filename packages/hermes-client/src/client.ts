import type {
  HermesBoardSummary,
  HermesConnection,
  HermesCronJob,
  HermesEnvEntry,
  HermesFileEntry,
  HermesKanbanStatus,
  HermesKanbanTask,
  HermesLearningGraph,
  HermesLogEntry,
  HermesMcpServer,
  HermesMessage,
  HermesMessagingPlatform,
  HermesModelOption,
  HermesOAuthProvider,
  HermesProfile,
  HermesSession,
  HermesSkill,
  HermesStatus,
  HermesSystemStats,
  HermesToolset,
  HermesUsagePoint,
  HermesWebhook,
} from "./types";

export interface NativeHttpResponse {
  status: number;
  body: string;
  contentType?: string;
}

export type NativeHttpAdapter = (req: {
  url: string;
  method: string;
  headers: Record<string, string>;
  body?: string;
}) => Promise<NativeHttpResponse>;

let globalNativeHttpAdapter: NativeHttpAdapter | null = null;

export function setNativeHttpAdapter(adapter: NativeHttpAdapter | null): void {
  globalNativeHttpAdapter = adapter;
}

export class HermesApiError extends Error {
  status: number;
  body: unknown;

  constructor(message: string, status: number, body?: unknown) {
    super(message);
    this.name = "HermesApiError";
    this.status = status;
    this.body = body;
  }
}

function normalizeUrl(raw: string): string {
  return raw.trim().replace(/\/+$/, "");
}

function toArray<T>(data: unknown, ...keys: string[]): T[] {
  if (Array.isArray(data)) return data as T[];
  if (data && typeof data === "object") {
    const obj = data as Record<string, unknown>;
    for (const key of keys) {
      if (Array.isArray(obj[key])) return obj[key] as T[];
    }
  }
  return [];
}

export class HermesClient {
  private conn: HermesConnection;
  private profile?: string;

  constructor(connection: HermesConnection, profile?: string) {
    this.conn = { ...connection, url: normalizeUrl(connection.url) };
    this.profile = profile ?? connection.defaultProfile;
  }

  getConnection(): HermesConnection {
    return this.conn;
  }

  setCredentials(credentials: Pick<HermesConnection, "token" | "refreshToken" | "expiresAt" | "authProvider">): void {
    // Keep the shared connection object so an existing WebSocket can obtain a
    // fresh ticket on reconnect without interrupting a running response.
    Object.assign(this.conn, credentials);
  }

  setProfile(profile?: string): void {
    this.profile = profile;
  }

  getProfile(): string | undefined {
    return this.profile;
  }

  private buildUrl(apiPath: string, query?: Record<string, string | number | boolean | undefined>): string {
    const base = normalizeUrl(this.conn.url);
    const cleanPath = apiPath.startsWith("/") ? apiPath : `/${apiPath}`;
    const params = new URLSearchParams();
    if (query) {
      for (const [k, v] of Object.entries(query)) {
        if (v !== undefined && v !== "") params.set(k, String(v));
      }
    }
    if (this.profile && !params.has("profile") && cleanPath.startsWith("/api/")) {
      params.set("profile", this.profile);
    }
    const qs = params.toString();
    const targetUrl = `${base}${cleanPath}${qs ? `?${qs}` : ""}`;

    if (this.conn.useProxy) {
      const proxyOrigin = this.conn.proxyBaseUrl
        ? normalizeUrl(this.conn.proxyBaseUrl)
        : typeof window !== "undefined"
          ? window.location.origin
          : "http://127.0.0.1:4173";
      return `${proxyOrigin}/proxy/http?target=${encodeURIComponent(targetUrl)}`;
    }
    return targetUrl;
  }

  private buildHeaders(extra?: Record<string, string>): Record<string, string> {
    const headers: Record<string, string> = {
      Accept: "application/json",
      ...extra,
    };
    if (this.conn.token) {
      if (this.conn.authType === "session-token") {
        headers["X-Hermes-Session-Token"] = this.conn.token;
      } else if (this.conn.authType === "bearer" || this.conn.mode === "openai") {
        headers["Authorization"] = `Bearer ${this.conn.token}`;
      }
    }
    return headers;
  }

  async request<T = unknown>(
    apiPath: string,
    options: {
      method?: string;
      query?: Record<string, string | number | boolean | undefined>;
      body?: unknown;
      headers?: Record<string, string>;
      signal?: AbortSignal;
      rawText?: boolean;
    } = {}
  ): Promise<T> {
    const url = this.buildUrl(apiPath, options.query);
    const headers = this.buildHeaders(options.headers);
    let body: BodyInit | undefined;
    if (options.body !== undefined) {
      if (options.body instanceof FormData || typeof options.body === "string") {
        body = options.body;
      } else {
        headers["Content-Type"] = "application/json";
        body = JSON.stringify(options.body);
      }
    }

    let status = 0;
    let ok = false;
    let contentType = "";
    let text = "";

    if (globalNativeHttpAdapter && !(body instanceof FormData)) {
      const nativeRes = await globalNativeHttpAdapter({
        url,
        method: options.method ?? "GET",
        headers,
        body: typeof body === "string" ? body : undefined,
      });
      status = nativeRes.status;
      ok = status >= 200 && status < 300;
      contentType = nativeRes.contentType ?? "";
      text = nativeRes.body ?? "";
    } else {
      const res = await fetch(url, {
        method: options.method ?? "GET",
        headers,
        body,
        credentials: this.conn.authType === "cookie" ? "include" : "same-origin",
        signal: options.signal ?? AbortSignal.timeout(30_000),
      });
      status = res.status;
      ok = res.ok;
      contentType = res.headers.get("content-type") ?? "";
      text = await res.text();
    }

    let parsed: unknown = text;
    if (!options.rawText && (contentType.includes("application/json") || text.trim().startsWith("{") || text.trim().startsWith("["))) {
      try {
        parsed = text ? JSON.parse(text) : {};
      } catch {
        parsed = text;
      }
    }

    if (!ok) {
      const parsedObj = parsed && typeof parsed === "object" ? (parsed as Record<string, unknown>) : null;
      const detail = parsedObj?.detail;
      const msg: string =
        (Array.isArray(detail) ? detail.map(item => {
          const issue = item as Record<string, unknown>;
          return `${Array.isArray(issue.loc) ? issue.loc.join(".") + ": " : ""}${issue.msg ?? JSON.stringify(issue)}`;
        }).join("; ") : detail ? String(detail) : "") ||
        (parsedObj && "error" in parsedObj && parsedObj.error ? String(parsedObj.error) : "") ||
        `Request failed (${status})`;
      throw new HermesApiError(msg, status, parsed);
    }
    return parsed as T;
  }

  // Health, Status & Gateway Lifecycle
  async testConnection(): Promise<{ ok: boolean; mode: "dashboard" | "openai"; status?: HermesStatus; models?: string[]; error?: string; errorStatus?: number }> {
    if (this.conn.mode === "openai") {
      try {
        const res = await this.request<{ data?: Array<{ id: string }> }>("/v1/models");
        const models = (res.data ?? []).map((m) => m.id);
        return { ok: true, mode: "openai", models };
      } catch (err) {
        return { ok: false, mode: "openai", error: err instanceof Error ? err.message : String(err), errorStatus: err instanceof HermesApiError ? err.status : undefined };
      }
    }
    try {
      // /api/status can be public. Verify a protected API as well so an
      // expired or incorrect credential never looks like a working connection.
      const [status, profiles] = await Promise.all([
        this.getStatus(), this.request<unknown>("/api/profiles"),
      ]);
      if (!status || typeof status !== "object" || !profiles || typeof profiles !== "object") {
        throw new Error("This address did not return the Hermes dashboard API. Check the server URL.");
      }
      return { ok: true, mode: "dashboard", status };
    } catch (err) {
      return { ok: false, mode: "dashboard", error: err instanceof Error ? err.message : String(err), errorStatus: err instanceof HermesApiError ? err.status : undefined };
    }
  }

  getStatus(): Promise<HermesStatus> {
    return this.request<HermesStatus>("/api/status");
  }

  getSystemStats(): Promise<HermesSystemStats> {
    return this.request<HermesSystemStats>("/api/system/stats");
  }

  getAuthMe(): Promise<Record<string, unknown>> {
    return this.request<Record<string, unknown>>("/api/auth/me");
  }

  async mintWsTicket(): Promise<string | null> {
    try {
      const res = await this.request<{ ticket?: string }>("/api/auth/ws-ticket", { method: "POST" });
      return res.ticket ?? null;
    } catch {
      return null;
    }
  }

  gatewayAction(action: "start" | "stop" | "restart"): Promise<Record<string, unknown>> {
    return this.request(`/api/gateway/${action}`, { method: "POST" });
  }

  checkUpdate(): Promise<Record<string, unknown>> {
    return this.request("/api/hermes/update/check");
  }

  triggerUpdate(): Promise<Record<string, unknown>> {
    return this.request("/api/hermes/update", { method: "POST" });
  }

  // Profiles / Bots
  async listProfiles(): Promise<HermesProfile[]> {
    const data = await this.request<unknown>("/api/profiles");
    return toArray<HermesProfile>(data, "profiles", "items");
  }

  setActiveProfile(name: string): Promise<Record<string, unknown>> {
    return this.request("/api/profiles/active", { method: "POST", body: { name } });
  }

  createProfile(payload: { name: string; clone_from?: string; description?: string; model?: string; provider?: string }): Promise<Record<string, unknown>> {
    return this.request("/api/profiles", { method: "POST", body: payload });
  }

  renameProfile(name: string, newName: string): Promise<Record<string, unknown>> {
    return this.request(`/api/profiles/${encodeURIComponent(name)}`, {
      method: "PATCH",
      body: { new_name: newName, name: newName },
    });
  }

  deleteProfile(name: string): Promise<Record<string, unknown>> {
    return this.request(`/api/profiles/${encodeURIComponent(name)}`, { method: "DELETE" });
  }

  setProfileDescription(name: string, description: string): Promise<Record<string, unknown>> {
    return this.request(`/api/profiles/${encodeURIComponent(name)}/description`, {
      method: "PUT",
      body: { description },
    });
  }

  setProfileModel(name: string, model: string, provider?: string): Promise<Record<string, unknown>> {
    return this.request(`/api/profiles/${encodeURIComponent(name)}/model`, {
      method: "PUT",
      body: { model, provider },
    });
  }

  async getProfileSoul(name: string): Promise<string> {
    const res = await this.request<{ content?: string; soul?: string } | string>(
      `/api/profiles/${encodeURIComponent(name)}/soul`
    );
    if (typeof res === "string") return res;
    return res.content ?? res.soul ?? "";
  }

  saveProfileSoul(name: string, content: string): Promise<Record<string, unknown>> {
    return this.request(`/api/profiles/${encodeURIComponent(name)}/soul`, {
      method: "PUT",
      body: { content },
    });
  }

  // Sessions
  async listSessions(limit = 100, archived: "include" | "exclude" | "only" = "exclude"): Promise<HermesSession[]> {
    const sessions: HermesSession[] = [];
    const requested = Math.max(0, Math.floor(limit));
    while (sessions.length < requested) {
      const pageSize = Math.min(100, requested - sessions.length);
      const data = await this.request<unknown>("/api/sessions", { query: { limit: pageSize, offset: sessions.length, order: "recent", archived } });
      const page = toArray<HermesSession>(data, "sessions", "items");
      sessions.push(...page);
      if (page.length < pageSize) break;
    }
    return sessions;
  }

  async listAllProfileSessions(): Promise<HermesSession[]> {
    const data = await this.request<unknown>("/api/profiles/sessions");
    return toArray<HermesSession>(data, "sessions", "items");
  }

  async searchSessions(q: string): Promise<HermesSession[]> {
    const data = await this.request<unknown>("/api/sessions/search", { query: { q } });
    return toArray<HermesSession>(data, "results", "sessions", "items");
  }

  async getSessionMessages(sessionId: string, limit = 500): Promise<HermesMessage[]> {
    const data = await this.request<unknown>(`/api/sessions/${encodeURIComponent(sessionId)}/messages`, {
      query: { limit, order: "latest" },
    });
    const raw = toArray<Record<string, unknown>>(data, "messages", "items");
    return raw.map((m, idx) => ({
      id: String(m.id ?? `${sessionId}-${idx}`),
      role: (m.role === "user" || m.role === "assistant" || m.role === "system" || m.role === "tool"
        ? m.role
        : "assistant") as HermesMessage["role"],
      content: typeof m.content === "string" ? m.content : JSON.stringify(m.content ?? ""),
      timestamp: (m.timestamp ?? m.created_at) as string | number | undefined,
      model: m.model ? String(m.model) : undefined,
    }));
  }

  renameSession(sessionId: string, title: string, profile?: string): Promise<Record<string, unknown>> {
    return this.request(`/api/sessions/${encodeURIComponent(sessionId)}`, {
      method: "PATCH",
      body: { title, profile: profile ?? this.profile },
    });
  }

  deleteSession(sessionId: string, profile?: string): Promise<Record<string, unknown>> {
    return this.request(`/api/sessions/${encodeURIComponent(sessionId)}`, {
      method: "DELETE",
      body: { profile: profile ?? this.profile },
    });
  }

  bulkDeleteSessions(ids: string[]): Promise<Record<string, unknown>> {
    return this.request("/api/sessions/bulk-delete", { method: "POST", body: { ids, session_ids: ids } });
  }

  deleteEmptySessions(): Promise<Record<string, unknown>> {
    return this.request("/api/sessions/empty", { method: "DELETE" });
  }

  getSessionStats(): Promise<Record<string, unknown>> {
    return this.request("/api/sessions/stats");
  }

  exportSession(sessionId: string): Promise<unknown> {
    return this.request(`/api/sessions/${encodeURIComponent(sessionId)}/export`);
  }

  async listWorkspaces(): Promise<Array<{ name: string; path: string }>> {
    const data = await this.request<unknown>("/api/chat/workspaces");
    return toArray<{ name: string; path: string }>(data, "workspaces", "items");
  }

  // Models & Providers
  getModelInfo(): Promise<Record<string, unknown>> {
    return this.request("/api/model/info");
  }

  async getModelOptions(includeUnconfigured = true): Promise<HermesModelOption[]> {
    if (this.conn.mode === "openai") {
      const res = await this.request<{ data?: Array<{ id: string; owned_by?: string }> }>("/v1/models");
      return (res.data ?? []).map((m) => ({
        id: m.id,
        name: m.id,
        provider: m.owned_by ?? "hermes",
        configured: true,
      }));
    }
    const data = await this.request<unknown>("/api/model/options", {
      query: { include_unconfigured: includeUnconfigured ? 1 : 0 },
    });
    if (Array.isArray(data)) return data as HermesModelOption[];
    if (data && typeof data === "object") {
      const obj = data as Record<string, unknown>;
      if (Array.isArray(obj.models)) return obj.models as HermesModelOption[];
      if (Array.isArray(obj.options)) return obj.options as HermesModelOption[];
      if (Array.isArray(obj.providers)) {
        const flat: HermesModelOption[] = [];
        for (const p of obj.providers as Array<Record<string, unknown>>) {
          const provId = String(p.slug ?? p.id ?? p.provider ?? p.name ?? "provider");
          const models = Array.isArray(p.models) ? p.models : [];
          for (const m of models) {
            if (typeof m === "string") {
              flat.push({ id: m, name: m, provider: provId, configured: Boolean(p.authenticated ?? p.configured ?? false) });
            } else if (m && typeof m === "object") {
              const mo = m as Record<string, unknown>;
              flat.push({
                id: String(mo.id ?? mo.model ?? mo.name),
                name: String(mo.name ?? mo.id ?? mo.model),
                provider: provId,
                configured: Boolean(mo.configured ?? p.authenticated ?? p.configured ?? false),
                context_length: typeof mo.context_length === "number" ? mo.context_length : undefined,
              });
            }
          }
        }
        return flat;
      }
    }
    return [];
  }

  setModel(payload: { model: string; provider?: string; base_url?: string }): Promise<Record<string, unknown>> {
    return this.request("/api/model/set", { method: "POST", body: payload });
  }

  getAuxiliaryModels(): Promise<Record<string, unknown>> {
    return this.request("/api/model/auxiliary");
  }

  getMoaConfig(): Promise<Record<string, unknown>> {
    return this.request("/api/model/moa");
  }

  saveMoaConfig(payload: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request("/api/model/moa", { method: "PUT", body: payload });
  }

  async listOAuthProviders(): Promise<HermesOAuthProvider[]> {
    const data = await this.request<unknown>("/api/providers/oauth");
    return toArray<HermesOAuthProvider>(data, "providers", "items");
  }

  startOAuth(providerId: string): Promise<Record<string, unknown>> {
    return this.request(`/api/providers/oauth/${encodeURIComponent(providerId)}/start`, { method: "POST" });
  }

  pollOAuth(providerId: string, sessionId: string): Promise<Record<string, unknown>> {
    return this.request(`/api/providers/oauth/${encodeURIComponent(providerId)}/poll/${encodeURIComponent(sessionId)}`);
  }

  disconnectOAuth(providerId: string): Promise<Record<string, unknown>> {
    return this.request(`/api/providers/oauth/${encodeURIComponent(providerId)}`, { method: "DELETE" });
  }

  getCredentialPool(): Promise<Record<string, unknown>> {
    return this.request("/api/credentials/pool");
  }

  // Skills, Toolsets & MCP
  async listSkills(): Promise<HermesSkill[]> {
    const data = await this.request<unknown>("/api/skills");
    return toArray<HermesSkill>(data, "skills", "items");
  }

  toggleSkill(name: string, enabled: boolean): Promise<Record<string, unknown>> {
    return this.request("/api/skills/toggle", { method: "PUT", body: { name, enabled } });
  }

  async getSkillContent(name: string): Promise<string> {
    const res = await this.request<{ content?: string } | string>("/api/skills/content", { query: { name } });
    return typeof res === "string" ? res : (res.content ?? "");
  }

  saveSkillContent(name: string, content: string): Promise<Record<string, unknown>> {
    return this.request("/api/skills/content", { method: "PUT", body: { name, content } });
  }

  createSkill(payload: { name: string; category?: string; content: string }): Promise<Record<string, unknown>> {
    return this.request("/api/skills", { method: "POST", body: payload });
  }

  async searchSkillsHub(query: string): Promise<Array<Record<string, unknown>>> {
    const data = await this.request<unknown>("/api/skills/hub/search", { query: { q: query } });
    return toArray<Record<string, unknown>>(data, "results", "skills", "items");
  }

  installHubSkill(identifier: string): Promise<Record<string, unknown>> {
    return this.request("/api/skills/hub/install", { method: "POST", body: { identifier, name: identifier } });
  }

  uninstallHubSkill(name: string): Promise<Record<string, unknown>> {
    return this.request("/api/skills/hub/uninstall", { method: "POST", body: { name } });
  }

  async listToolsets(): Promise<HermesToolset[]> {
    const data = await this.request<unknown>("/api/tools/toolsets");
    return toArray<HermesToolset>(data, "toolsets", "items");
  }

  toggleToolset(name: string, enabled: boolean): Promise<Record<string, unknown>> {
    return this.request(`/api/tools/toolsets/${encodeURIComponent(name)}`, {
      method: "PUT",
      body: { enabled },
    });
  }

  async listMcpServers(): Promise<HermesMcpServer[]> {
    const data = await this.request<unknown>("/api/mcp/servers");
    if (Array.isArray(data)) return data as HermesMcpServer[];
    if (data && typeof data === "object") {
      const obj = data as Record<string, unknown>;
      if (Array.isArray(obj.servers)) return obj.servers as HermesMcpServer[];
      if (obj.servers && typeof obj.servers === "object") {
        return Object.entries(obj.servers as Record<string, Record<string, unknown>>).map(([name, cfg]) => ({
          name,
          ...cfg,
        }));
      }
    }
    return [];
  }

  addMcpServer(server: HermesMcpServer): Promise<Record<string, unknown>> {
    return this.request("/api/mcp/servers", { method: "POST", body: server });
  }

  deleteMcpServer(name: string): Promise<Record<string, unknown>> {
    return this.request(`/api/mcp/servers/${encodeURIComponent(name)}`, { method: "DELETE" });
  }

  testMcpServer(name: string): Promise<Record<string, unknown>> {
    return this.request(`/api/mcp/servers/${encodeURIComponent(name)}/test`, { method: "POST" });
  }

  toggleMcpServer(name: string, enabled: boolean): Promise<Record<string, unknown>> {
    return this.request(`/api/mcp/servers/${encodeURIComponent(name)}/enabled`, {
      method: "PUT",
      body: { enabled },
    });
  }

  async listMcpCatalog(): Promise<Array<Record<string, unknown>>> {
    const data = await this.request<unknown>("/api/mcp/catalog");
    return toArray<Record<string, unknown>>(data, "servers", "catalog", "items");
  }

  // Routines / Cron
  async listCronJobs(): Promise<HermesCronJob[]> {
    const data = await this.request<unknown>("/api/cron/jobs");
    return toArray<Record<string, unknown>>(data, "jobs", "items").map(job => {
      const schedule = job.schedule;
      const shape = schedule && typeof schedule === "object" ? schedule as Record<string, unknown> : {};
      return { ...job,
        id: String(job.id), name: String(job.name ?? "Scheduled job"), prompt: String(job.prompt ?? ""),
        schedule: String(job.schedule_display ?? shape.display ?? shape.expr ?? shape.value ?? shape.run_at ?? schedule ?? ""),
        paused: job.paused === true || job.enabled === false,
      } as HermesCronJob;
    });
  }

  createCronJob(job: Partial<HermesCronJob>): Promise<Record<string, unknown>> {
    return this.request("/api/cron/jobs", { method: "POST", body: job });
  }

  updateCronJob(id: string, job: Partial<HermesCronJob>): Promise<Record<string, unknown>> {
    return this.request(`/api/cron/jobs/${encodeURIComponent(id)}`, { method: "PUT", body: { updates: job } });
  }

  deleteCronJob(id: string): Promise<Record<string, unknown>> {
    return this.request(`/api/cron/jobs/${encodeURIComponent(id)}`, { method: "DELETE" });
  }

  pauseCronJob(id: string): Promise<Record<string, unknown>> {
    return this.request(`/api/cron/jobs/${encodeURIComponent(id)}/pause`, { method: "POST" });
  }

  resumeCronJob(id: string): Promise<Record<string, unknown>> {
    return this.request(`/api/cron/jobs/${encodeURIComponent(id)}/resume`, { method: "POST" });
  }

  triggerCronJob(id: string): Promise<Record<string, unknown>> {
    return this.request(`/api/cron/jobs/${encodeURIComponent(id)}/trigger`, { method: "POST" });
  }

  async listCronBlueprints(): Promise<Array<Record<string, unknown>>> {
    const data = await this.request<unknown>("/api/cron/blueprints");
    return toArray<Record<string, unknown>>(data, "blueprints", "items");
  }

  instantiateCronBlueprint(id: string): Promise<Record<string, unknown>> {
    return this.request("/api/cron/blueprints/instantiate", { method: "POST", body: { id, blueprint_id: id } });
  }

  // Memory, Curator & Ops
  getMemory(): Promise<Record<string, unknown>> {
    return this.request("/api/memory");
  }

  setMemoryProvider(provider: string): Promise<Record<string, unknown>> {
    return this.request("/api/memory/provider", { method: "PUT", body: { provider } });
  }

  resetMemory(): Promise<Record<string, unknown>> {
    return this.request("/api/memory/reset", { method: "POST" });
  }

  getCurator(): Promise<Record<string, unknown>> {
    return this.request("/api/curator");
  }

  setCuratorPaused(paused: boolean): Promise<Record<string, unknown>> {
    return this.request("/api/curator/paused", { method: "PUT", body: { paused } });
  }

  runCurator(): Promise<Record<string, unknown>> {
    return this.request("/api/curator/run", { method: "POST" });
  }

  async listCheckpoints(): Promise<Array<Record<string, unknown>>> {
    const data = await this.request<unknown>("/api/ops/checkpoints");
    return toArray<Record<string, unknown>>(data, "checkpoints", "items");
  }

  pruneCheckpoints(): Promise<Record<string, unknown>> {
    return this.request("/api/ops/checkpoints/prune", { method: "POST" });
  }

  async listHooks(): Promise<Array<Record<string, unknown>>> {
    const data = await this.request<unknown>("/api/ops/hooks");
    return toArray<Record<string, unknown>>(data, "hooks", "items");
  }

  runDoctor(): Promise<Record<string, unknown>> {
    return this.request("/api/ops/doctor", { method: "POST" });
  }

  runSecurityAudit(): Promise<Record<string, unknown>> {
    return this.request("/api/ops/security-audit", { method: "POST" });
  }

  runBackup(): Promise<Record<string, unknown>> {
    return this.request("/api/ops/backup", { method: "POST" });
  }

  // Messaging Platforms, Pairing & Webhooks
  async listMessagingPlatforms(): Promise<HermesMessagingPlatform[]> {
    const data = await this.request<unknown>("/api/messaging/platforms");
    return toArray<HermesMessagingPlatform>(data, "platforms", "items");
  }

  updateMessagingPlatform(id: string, config: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request(`/api/messaging/platforms/${encodeURIComponent(id)}`, { method: "PUT", body: config });
  }

  testMessagingPlatform(id: string): Promise<Record<string, unknown>> {
    return this.request(`/api/messaging/platforms/${encodeURIComponent(id)}/test`, { method: "POST" });
  }

  getPairing(): Promise<Record<string, unknown>> {
    return this.request("/api/pairing");
  }

  approvePairing(payload: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request("/api/pairing/approve", { method: "POST", body: { ...payload, profile: this.profile } });
  }

  revokePairing(payload: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request("/api/pairing/revoke", { method: "POST", body: { ...payload, profile: this.profile } });
  }

  async listWebhooks(): Promise<HermesWebhook[]> {
    const data = await this.request<unknown>("/api/webhooks");
    return toArray<HermesWebhook>(data, "webhooks", "items");
  }

  createWebhook(payload: Partial<HermesWebhook>): Promise<Record<string, unknown>> {
    return this.request("/api/webhooks", { method: "POST", body: payload });
  }

  deleteWebhook(id: string): Promise<Record<string, unknown>> {
    return this.request(`/api/webhooks/${encodeURIComponent(id)}`, { method: "DELETE" });
  }

  // Files
  async listFiles(dirPath = ""): Promise<HermesFileEntry[]> {
    const data = await this.request<unknown>("/api/files", { query: { path: dirPath } });
    return toArray<Record<string, unknown>>(data, "entries", "files", "items").map(entry => ({
      ...entry, name: String(entry.name), path: String(entry.path),
      is_dir: Boolean(entry.is_directory ?? entry.is_dir),
      size: typeof entry.size === "number" ? entry.size : undefined,
    }));
  }

  async readFile(filePath: string): Promise<string> {
    const res = await this.request<{ content?: string; data_url?: string } | string>("/api/files/read", {
      query: { path: filePath },
    });
    if (typeof res === "string") return res;
    if (res.content !== undefined) return res.content;
    if (res.data_url) {
      const encoded = res.data_url.slice(res.data_url.indexOf(",") + 1);
      return new TextDecoder().decode(Uint8Array.from(atob(encoded), char => char.charCodeAt(0)));
    }
    return "";
  }

  mkdir(dirPath: string): Promise<Record<string, unknown>> {
    return this.request("/api/files/mkdir", { method: "POST", body: { path: dirPath } });
  }

  deleteFile(targetPath: string): Promise<Record<string, unknown>> {
    return this.request("/api/files", { method: "DELETE", query: { path: targetPath } });
  }

  // Config & Env Secrets
  getConfig(): Promise<Record<string, unknown>> {
    return this.request("/api/config");
  }

  saveConfig(config: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.request("/api/config", { method: "PUT", body: config });
  }

  async getRawConfig(): Promise<string> {
    const res = await this.request<{ content?: string; yaml?: string } | string>("/api/config/raw");
    if (typeof res === "string") return res;
    return res.content ?? res.yaml ?? "";
  }

  saveRawConfig(content: string): Promise<Record<string, unknown>> {
    return this.request("/api/config/raw", { method: "PUT", body: { content } });
  }

  async listEnv(): Promise<HermesEnvEntry[]> {
    const data = await this.request<unknown>("/api/env");
    if (Array.isArray(data)) return data as HermesEnvEntry[];
    if (data && typeof data === "object") {
      const obj = data as Record<string, unknown>;
      if (Array.isArray(obj.vars)) return obj.vars as HermesEnvEntry[];
      if (Array.isArray(obj.entries)) return obj.entries as HermesEnvEntry[];
      if (obj.env && typeof obj.env === "object") {
        return Object.entries(obj.env as Record<string, unknown>).map(([key, val]) => ({
          key,
          masked: typeof val === "string" ? val : String(val ?? ""),
          is_set: Boolean(val),
        }));
      }
    }
    return [];
  }

  setEnv(key: string, value: string): Promise<Record<string, unknown>> {
    return this.request("/api/env", { method: "PUT", body: { key, value } });
  }

  deleteEnv(key: string): Promise<Record<string, unknown>> {
    return this.request("/api/env", { method: "DELETE", query: { key }, body: { key } });
  }

  async revealEnv(key: string): Promise<string> {
    const res = await this.request<{ value?: string }>("/api/env/reveal", { method: "POST", body: { key } });
    return res.value ?? "";
  }

  // Logs & Analytics
  async getLogs(lines = 200): Promise<HermesLogEntry[]> {
    const data = await this.request<unknown>("/api/logs", { query: { lines } });
    if (typeof data === "string") {
      return data
        .split("\n")
        .filter(Boolean)
        .map((message) => ({ message }));
    }
    const arr = toArray<HermesLogEntry | string>(data, "logs", "lines", "entries");
    return arr.map((item) => (typeof item === "string" ? { message: item } : item));
  }

  async getUsageAnalytics(days = 14): Promise<HermesUsagePoint[]> {
    const data = await this.request<unknown>("/api/analytics/usage", { query: { days } });
    return toArray<HermesUsagePoint>(data, "daily", "points", "usage", "items");
  }

  async getModelAnalytics(days = 14): Promise<Array<Record<string, unknown>>> {
    const data = await this.request<unknown>("/api/analytics/models", { query: { days } });
    return toArray<Record<string, unknown>>(data, "models", "breakdown", "items");
  }

  // Session Pin / Archive
  pinSession(sessionId: string, pinned: boolean, profile?: string): Promise<Record<string, unknown>> {
    return this.request(`/api/sessions/${encodeURIComponent(sessionId)}`, {
      method: "PATCH",
      body: { pinned, profile: profile ?? this.profile },
    });
  }

  archiveSession(sessionId: string, archived: boolean, profile?: string): Promise<Record<string, unknown>> {
    return this.request(`/api/sessions/${encodeURIComponent(sessionId)}`, {
      method: "PATCH",
      body: { archived, profile: profile ?? this.profile },
    });
  }

  // Projects Tree
  async getProjectsTree(): Promise<Record<string, unknown>> {
    return this.request<Record<string, unknown>>("/api/profiles/projects/tree");
  }

  // Kanban Session Boards (/api/plugins/kanban/*)
  async listKanbanBoards(): Promise<HermesBoardSummary[]> {
    const data = await this.request<unknown>("/api/plugins/kanban/boards");
    return toArray<HermesBoardSummary>(data, "boards", "items").map(board => ({
      ...board, workspace_path: String(board.default_workdir ?? board.workspace_path ?? ""),
      task_count: typeof board.total === "number" ? board.total : board.task_count,
    }));
  }

  createKanbanBoard(payload: { name: string; slug?: string; description?: string; color?: string; workspace_path?: string }): Promise<Record<string, unknown>> {
    const { workspace_path, ...fields } = payload;
    return this.request("/api/plugins/kanban/boards", { method: "POST", body: { ...fields, default_workdir: workspace_path || undefined } });
  }

  updateKanbanBoard(slug: string, patch: { name?: string; description?: string }): Promise<Record<string, unknown>> {
    return this.request(`/api/plugins/kanban/boards/${encodeURIComponent(slug)}`, { method: "PATCH", body: patch });
  }

  deleteKanbanBoard(slug: string): Promise<Record<string, unknown>> {
    return this.request(`/api/plugins/kanban/boards/${encodeURIComponent(slug)}`, { method: "DELETE" });
  }

  switchKanbanBoard(slug: string): Promise<Record<string, unknown>> {
    return this.request(`/api/plugins/kanban/boards/${encodeURIComponent(slug)}/switch`, { method: "POST" });
  }

  async getKanbanBoard(boardSlug?: string): Promise<{ board?: string; tasks: HermesKanbanTask[] }> {
    const data = await this.request<Record<string, unknown>>("/api/plugins/kanban/board", {
      query: boardSlug ? { board: boardSlug } : undefined,
    });
    const board = typeof data.board === "string" ? data.board : boardSlug;
    const normalize = (item: Record<string, unknown>, status?: string): HermesKanbanTask => ({
      ...item as unknown as HermesKanbanTask,
      board: String(item.board ?? board ?? "default"),
      status: String(item.status ?? status ?? "todo") as HermesKanbanStatus,
      model: (item.model_override ?? item.model ?? undefined) as string | undefined,
      provider: (item.provider_override ?? item.provider ?? undefined) as string | undefined,
      cwd: (item.workspace_path ?? item.cwd ?? undefined) as string | undefined,
    });
    const tasks: HermesKanbanTask[] = [];
    if (Array.isArray(data.tasks)) {
      tasks.push(...data.tasks.map(item => normalize(item as Record<string, unknown>)));
    } else if (Array.isArray(data.columns)) {
      for (const column of data.columns as Array<{ name: string; tasks: Array<Record<string, unknown>> }>) {
        tasks.push(...(column.tasks ?? []).map(item => normalize(item, column.name)));
      }
    } else if (data.columns && typeof data.columns === "object") {
      for (const [status, list] of Object.entries(data.columns as Record<string, unknown>)) {
        if (Array.isArray(list)) tasks.push(...list.map(item => normalize(item, status)));
      }
    }
    return { board, tasks };
  }

  private taskPriority(value: unknown): number | undefined {
    if (value === undefined) return undefined;
    const legacy: Record<string, number> = { low: -1, medium: 0, high: 1, urgent: 2 };
    const parsed = typeof value === "string" && value in legacy ? legacy[value] : Number(value);
    return Number.isFinite(parsed) ? Math.trunc(parsed) : 0;
  }

  async createKanbanTask(
    payload: { title: string; body?: string; status?: HermesKanbanStatus; priority?: string | number;
      assignee?: string; model?: string; provider?: string; cwd?: string; tags?: string[] },
    boardSlug?: string
  ): Promise<Record<string, unknown>> {
    const { status, model, provider, cwd, priority, ...fields } = payload;
    let result = await this.request<Record<string, unknown>>("/api/plugins/kanban/tasks", {
      method: "POST", query: boardSlug ? { board: boardSlug } : undefined,
      body: { ...fields, priority: this.taskPriority(priority), model_override: model || undefined,
        provider_override: provider || undefined, workspace_kind: cwd ? "dir" : "scratch",
        workspace_path: cwd || undefined, triage: status === "triage" },
    });
    const task = result.task as HermesKanbanTask | undefined;
    if (task?.id && status && task.status !== status) {
      result = await this.updateKanbanTask(task.id, { status }, boardSlug);
    }
    return result;
  }

  updateKanbanTask(taskId: string, patch: Partial<HermesKanbanTask>, boardSlug?: string): Promise<Record<string, unknown>> {
    const { model, provider, priority, cwd: _cwd, ...fields } = patch;
    return this.request(`/api/plugins/kanban/tasks/${encodeURIComponent(taskId)}`, {
      method: "PATCH", query: boardSlug ? { board: boardSlug } : undefined,
      body: { ...fields, priority: this.taskPriority(priority), model_override: model, provider_override: provider },
    });
  }

  deleteKanbanTask(taskId: string, boardSlug?: string): Promise<Record<string, unknown>> {
    return this.request(`/api/plugins/kanban/tasks/${encodeURIComponent(taskId)}`, {
      method: "DELETE",
      query: boardSlug ? { board: boardSlug } : undefined,
    });
  }

  addKanbanComment(taskId: string, body: string, boardSlug?: string): Promise<Record<string, unknown>> {
    return this.request(`/api/plugins/kanban/tasks/${encodeURIComponent(taskId)}/comments`, {
      method: "POST",
      query: boardSlug ? { board: boardSlug } : undefined,
      body: { body, text: body },
    });
  }

  dispatchKanban(boardSlug?: string): Promise<Record<string, unknown>> {
    return this.request("/api/plugins/kanban/dispatch", {
      method: "POST",
      query: boardSlug ? { board: boardSlug } : undefined,
    });
  }

  decomposeKanbanTask(taskId: string, boardSlug?: string): Promise<Record<string, unknown>> {
    return this.request(`/api/plugins/kanban/tasks/${encodeURIComponent(taskId)}/decompose`, {
      method: "POST",
      query: boardSlug ? { board: boardSlug } : undefined,
    });
  }

  async getActiveKanbanWorkers(boardSlug?: string): Promise<Array<Record<string, unknown>>> {
    const data = await this.request<unknown>("/api/plugins/kanban/workers/active", {
      query: boardSlug ? { board: boardSlug } : undefined,
    });
    return toArray<Record<string, unknown>>(data, "workers", "active", "items");
  }

  // Learning / Memory Knowledge Graph
  async getLearningGraph(): Promise<HermesLearningGraph> {
    const data = await this.request<Record<string, unknown>>("/api/learning/graph");
    return {
      nodes: toArray(data, "nodes", "items"),
      edges: toArray(data, "edges", "links"),
    };
  }

  getLearningNode(id: string): Promise<Record<string, unknown>> {
    return this.request("/api/learning/node", { query: { id } });
  }
}
