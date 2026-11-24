import type { HermesClient } from "./client";
import type { HermesConnection, HermesMessage, HermesStreamEvent, HermesTodoItem } from "./types";

type EventHandler = (event: HermesStreamEvent) => void;

function toWsOrigin(httpUrl: string): string {
  const clean = httpUrl.trim().replace(/\/+$/, "");
  if (clean.startsWith("https://")) return `wss://${clean.slice(8)}`;
  if (clean.startsWith("http://")) return `ws://${clean.slice(7)}`;
  return clean;
}

export class HermesRealtimeSession {
  private conn: HermesConnection;
  private client: HermesClient;
  private ws: WebSocket | null = null;
  private rpcId = 1;
  private handlers = new Set<EventHandler>();
  private pendingRpc = new Map<
    string | number,
    { resolve: (v: unknown) => void; reject: (e: Error) => void; timer?: ReturnType<typeof setTimeout> }
  >();
  private heartbeatTimer: ReturnType<typeof setInterval> | null = null;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private reconnectAttempt = 0;

  constructor(client: HermesClient) {
    this.client = client;
    this.conn = client.getConnection();
  }

  subscribe(handler: EventHandler): () => void {
    this.handlers.add(handler);
    return () => this.handlers.delete(handler);
  }

  private emit(event: HermesStreamEvent): void {
    for (const h of this.handlers) h(event);
  }

  isConnected(): boolean {
    return this.ws !== null && this.ws.readyState === WebSocket.OPEN;
  }

  private startHeartbeat(): void {
    this.stopHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      if (this.ws && this.ws.readyState === WebSocket.OPEN) {
        this.rpc("gateway.ping", {}, 10_000).catch(() => {
          // ignore transient ping failure
        });
      }
    }, 20_000);
  }

  private stopHeartbeat(): void {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }

  async connect(profile?: string): Promise<void> {
    if (this.conn.mode === "openai") {
      this.emit({ type: "status", status: "connected", detail: "OpenAI streaming ready" });
      return;
    }

    this.disconnect();
    this.emit({ type: "status", status: "connecting" });

    const params = new URLSearchParams();
    if (profile) params.set("profile", profile);

    if (this.conn.authType === "session-token" && this.conn.token) {
      params.set("token", this.conn.token);
    } else if (this.conn.authType === "cookie" || this.conn.authType === "bearer") {
      const ticket = await this.client.mintWsTicket();
      if (ticket) params.set("ticket", ticket);
      else if (this.conn.token) params.set("token", this.conn.token);
    }

    const qs = params.toString();
    const targetWsUrl = `${toWsOrigin(this.conn.url)}/api/ws${qs ? `?${qs}` : ""}`;
    let wsUrl = targetWsUrl;
    if (this.conn.useProxy) {
      const proxyOrigin = this.conn.proxyBaseUrl
        ? toWsOrigin(this.conn.proxyBaseUrl)
        : typeof window !== "undefined"
          ? toWsOrigin(window.location.origin)
          : "ws://127.0.0.1:4173";
      wsUrl = `${proxyOrigin}/proxy/ws?target=${encodeURIComponent(targetWsUrl)}`;
    }

    await new Promise<void>((resolve, reject) => {
      const ws = new WebSocket(wsUrl);
      this.ws = ws;

      const timer = setTimeout(() => {
        if (ws.readyState !== WebSocket.OPEN) {
          ws.close();
          reject(new Error("WebSocket connection timed out"));
        }
      }, 10_000);

      ws.onopen = () => {
        if (this.ws !== ws) return;
        clearTimeout(timer);
        this.reconnectAttempt = 0;
        this.emit({ type: "status", status: "connected" });
        this.startHeartbeat();
        resolve();
      };

      ws.onerror = () => {
        if (this.ws !== ws) return;
        clearTimeout(timer);
        this.emit({ type: "status", status: "error", detail: "Could not reach Hermes WebSocket" });
        reject(new Error("WebSocket connection failed"));
      };

      ws.onclose = () => {
        clearTimeout(timer);
        if (this.ws !== ws) return;
        this.ws = null;
        this.stopHeartbeat();
        for (const [, p] of this.pendingRpc) {
          if (p.timer) clearTimeout(p.timer);
          p.reject(new Error("WebSocket closed"));
        }
        this.pendingRpc.clear();
        this.emit({ type: "status", status: "disconnected" });
        reject(new Error("WebSocket closed before connecting"));
        const delay = Math.min(15_000, 500 * 2 ** this.reconnectAttempt++);
        this.reconnectTimer = setTimeout(() => {
          this.reconnectTimer = null;
          void this.connect(profile).catch(() => undefined);
        }, delay);
      };

      ws.onmessage = (evt) => {
        const rawText = typeof evt.data === "string" ? evt.data : String(evt.data);
        // Hermes ws.py coalesces streaming token frames as newline-delimited JSON lines
        const lines = rawText.includes("\n") ? rawText.split("\n") : [rawText];
        for (const line of lines) {
          const trimmed = line.trim();
          if (trimmed) this.handleRawFrame(trimmed);
        }
      };
    });
  }

  disconnect(): void {
    this.stopHeartbeat();
    if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
    this.reconnectTimer = null;
    const ws = this.ws;
    this.ws = null;
    for (const [, pending] of this.pendingRpc) {
      if (pending.timer) clearTimeout(pending.timer);
      pending.reject(new Error("WebSocket disconnected"));
    }
    this.pendingRpc.clear();
    if (ws) {
      try {
        ws.close();
      } catch {
        // ignore
      }
    }
  }

  private handleRawFrame(raw: string): void {
    let msg: Record<string, unknown>;
    try {
      msg = JSON.parse(raw) as Record<string, unknown>;
    } catch {
      return;
    }

    // JSON-RPC response to our request
    if ((typeof msg.id === "number" || typeof msg.id === "string") && this.pendingRpc.has(msg.id)) {
      const pending = this.pendingRpc.get(msg.id)!;
      this.pendingRpc.delete(msg.id);
      if (pending.timer) clearTimeout(pending.timer);
      if (msg.error) {
        const errObj = msg.error as Record<string, unknown>;
        pending.reject(new Error(String(errObj.message ?? "RPC error")));
      } else {
        pending.resolve(msg.result);
      }
      return;
    }

    // Official Hermes tui_gateway envelope: { jsonrpc: "2.0", method: "event", params: { type, session_id, payload } }
    let eventType = "";
    let sessionId: string | undefined;
    let payload: Record<string, unknown> = {};

    if (msg.method === "event" && msg.params && typeof msg.params === "object") {
      const env = msg.params as Record<string, unknown>;
      eventType = String(env.type ?? "");
      sessionId = env.session_id ? String(env.session_id) : env.sessionId ? String(env.sessionId) : undefined;
      payload =
        env.payload && typeof env.payload === "object"
          ? (env.payload as Record<string, unknown>)
          : env;
    } else {
      eventType = String(msg.method ?? msg.type ?? msg.event ?? "");
      payload = ((msg.params ?? msg.data ?? msg) as Record<string, unknown>) || {};
      sessionId = payload.session_id
        ? String(payload.session_id)
        : payload.sessionId
          ? String(payload.sessionId)
          : undefined;
    }

    switch (eventType) {
      case "gateway.ready":
        this.emit({ type: "gateway.ready", payload });
        return;

      case "session.info":
        this.emit({ type: "session.info", sessionId, info: payload });
        return;

      case "session.usage":
        this.emit({ type: "session.usage", sessionId, usage: payload });
        return;

      case "status.update":
        this.emit({
          type: "status.update",
          sessionId,
          text: String(payload.text ?? payload.status ?? payload.message ?? ""),
          kind: typeof payload.kind === "string" ? payload.kind : undefined,
        });
        return;

      case "message.start":
        this.emit({
          type: "message.start",
          sessionId,
          streamId: payload.stream_id ? String(payload.stream_id) : undefined,
        });
        return;

      case "message.delta":
      case "text.delta":
      case "delta":
      case "assistant.delta":
      case "stream.delta": {
        const delta = String(payload.delta ?? payload.text ?? payload.content ?? "");
        if (delta) this.emit({ type: "text.delta", sessionId, delta });
        return;
      }

      case "thinking.delta":
      case "reasoning.delta": {
        const delta = String(payload.delta ?? payload.text ?? payload.reasoning ?? "");
        if (delta) this.emit({ type: "thinking.delta", sessionId, delta });
        return;
      }

      case "message.interim": {
        const content = String(payload.content ?? payload.text ?? "");
        if (content) this.emit({ type: "message.interim", sessionId, content });
        return;
      }

      case "tool.start":
      case "tool_call.start":
        this.emit({
          type: "tool.start",
          sessionId,
          toolCall: {
            id: String(payload.call_id ?? payload.id ?? payload.tool_call_id ?? `tool-${Date.now()}`),
            name: String(payload.name ?? payload.tool ?? payload.tool_name ?? "tool"),
            arguments: (payload.arguments ?? payload.args ?? payload.input) as
              | string
              | Record<string, unknown>
              | undefined,
            preview: payload.preview ? String(payload.preview) : undefined,
            status: "running",
          },
        });
        return;

      case "tool.progress":
        this.emit({
          type: "tool.progress",
          sessionId,
          toolCallId: String(payload.call_id ?? payload.id ?? payload.tool_call_id ?? ""),
          preview: String(payload.preview ?? payload.text ?? payload.output ?? ""),
        });
        return;

      case "tool.complete":
      case "tool.end":
      case "tool_call.end":
      case "tool.result":
        this.emit({
          type: "tool.end",
          sessionId,
          toolCallId: String(payload.call_id ?? payload.id ?? payload.tool_call_id ?? ""),
          output:
            payload.output !== undefined
              ? String(payload.output)
              : payload.result !== undefined
                ? String(payload.result)
                : payload.summary !== undefined
                  ? String(payload.summary)
                  : undefined,
          error: payload.error ? String(payload.error) : undefined,
          durationMs:
            typeof payload.duration_ms === "number"
              ? payload.duration_ms
              : typeof payload.elapsed_ms === "number"
                ? payload.elapsed_ms
                : undefined,
        });
        return;

      case "todo.updated": {
        const rawTodos = Array.isArray(payload.todos)
          ? payload.todos
          : Array.isArray(payload.items)
            ? payload.items
            : [];
        const todos: HermesTodoItem[] = rawTodos.map((t: Record<string, unknown>, idx: number) => ({
          id: String(t.id ?? idx),
          content: String(t.content ?? t.title ?? t.text ?? ""),
          status: String(t.status ?? "pending"),
        }));
        this.emit({ type: "todo.updated", sessionId, todos });
        return;
      }

      case "subagent.spawn_requested":
      case "subagent.start":
      case "subagent.tool":
      case "subagent.progress":
      case "subagent.complete": {
        const id = typeof payload.subagent_id === "string" ? payload.subagent_id : "";
        if (!id || !sessionId) return;
        const activity = payload.summary ?? payload.tool_preview ?? payload.text ?? payload.tool_name;
        this.emit({
          type: "subagent.update",
          sessionId,
          subagent: {
            id,
            phase: eventType === "subagent.complete" ? "finished" : eventType === "subagent.spawn_requested" ? "queued" : "running",
            goal: typeof payload.goal === "string" && payload.goal ? payload.goal : undefined,
            model: typeof payload.model === "string" && payload.model ? payload.model : undefined,
            activity: typeof activity === "string" && activity ? activity.slice(0, 400) : undefined,
            outcome: typeof payload.status === "string" ? payload.status : undefined,
          },
        });
        return;
      }

      case "approval.request":
        this.emit({
          type: "approval.request",
          sessionId,
          approval: {
            id: String(payload.request_id ?? payload.id ?? payload.approval_id ?? `appr-${Date.now()}`),
            tool: payload.tool ? String(payload.tool) : undefined,
            title: String(payload.title ?? payload.description ?? payload.tool ?? "Run command"),
            command: payload.command ? String(payload.command) : undefined,
            detail: String(payload.command ?? payload.detail ?? payload.reason ?? ""),
            settled: null,
          },
        });
        return;

      case "approval.settled": {
        const rawDec = String(payload.decision ?? payload.choice ?? "allowed");
        const decision =
          rawDec === "deny" || rawDec === "denied"
            ? "denied"
            : rawDec === "always"
              ? "always"
              : "allowed";
        this.emit({
          type: "approval.settled",
          sessionId,
          id: String(payload.request_id ?? payload.id ?? payload.approval_id ?? ""),
          decision,
        });
        return;
      }

      case "clarify.request":
        this.emit({
          type: "clarify.request",
          sessionId,
          clarify: {
            id: String(payload.request_id ?? payload.id ?? `clarify-${Date.now()}`),
            question: String(payload.question ?? payload.prompt ?? "Please clarify"),
            choices: Array.isArray(payload.choices) ? payload.choices.map(String) : undefined,
            answered: null,
          },
        });
        return;

      case "clarify.settled":
        this.emit({
          type: "clarify.settled",
          sessionId,
          id: String(payload.request_id ?? payload.id ?? ""),
          answer: String(payload.answer ?? ""),
        });
        return;

      case "message.complete":
      case "turn.complete":
      case "done":
      case "session.idle":
        this.emit({
          type: "message.complete",
          sessionId,
          content: payload.content ? String(payload.content) : payload.text ? String(payload.text) : undefined,
        });
        return;

      case "error":
        this.emit({
          type: "error",
          sessionId,
          message: String(payload.message ?? payload.error ?? "Hermes error"),
        });
        return;
    }
  }

  rpc<T = unknown>(method: string, params: Record<string, unknown> = {}, timeoutMs = 120_000): Promise<T> {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error("WebSocket is not connected"));
    }
    const id = `r${this.rpcId++}`;
    return new Promise<T>((resolve, reject) => {
      const timer =
        timeoutMs > 0
          ? setTimeout(() => {
              if (this.pendingRpc.delete(id)) {
                reject(new Error(`RPC ${method} timed out`));
              }
            }, timeoutMs)
          : undefined;
      this.pendingRpc.set(id, {
        resolve: (v) => resolve(v as T),
        reject,
        timer,
      });
      this.ws!.send(JSON.stringify({ jsonrpc: "2.0", id, method, params }));
    });
  }

  createSession(params: {
    profile?: string;
    model?: string;
    provider?: string;
    cwd?: string;
    title?: string;
    reasoning_effort?: string;
    fast?: boolean;
    source?: string;
  } = {}): Promise<Record<string, unknown>> {
    return this.rpc("session.create", { source: "desktop", ...params });
  }

  resumeSession(sessionId: string, opts: { profile?: string; omit_messages?: boolean } = {}): Promise<Record<string, unknown>> {
    return this.rpc("session.resume", { session_id: sessionId, ...opts });
  }

  submitPrompt(sessionId: string, text: string, extra: Record<string, unknown> = {}): Promise<Record<string, unknown>> {
    return this.rpc("prompt.submit", { session_id: sessionId, text, ...extra });
  }

  interruptSession(sessionId: string): Promise<Record<string, unknown>> {
    return this.rpc("session.interrupt", { session_id: sessionId });
  }

  steerSession(sessionId: string, text: string): Promise<Record<string, unknown>> {
    return this.rpc("session.steer", { session_id: sessionId, text });
  }

  compressSession(sessionId: string): Promise<Record<string, unknown>> {
    return this.rpc("session.compress", { session_id: sessionId });
  }

  branchSession(sessionId: string, title?: string): Promise<Record<string, unknown>> {
    return this.rpc("session.branch", { session_id: sessionId, title });
  }

  undoSession(sessionId: string): Promise<Record<string, unknown>> {
    return this.rpc("session.undo", { session_id: sessionId });
  }

  setSessionTitle(sessionId: string, title: string): Promise<Record<string, unknown>> {
    return this.rpc("session.title", { session_id: sessionId, title });
  }

  setSessionCwd(sessionId: string, cwd: string): Promise<Record<string, unknown>> {
    return this.rpc("session.cwd.set", { session_id: sessionId, cwd });
  }

  respondApproval(
    sessionId: string,
    requestId: string,
    choice: "once" | "always" | "deny"
  ): Promise<Record<string, unknown>> {
    return this.rpc("approval.respond", {
      session_id: sessionId,
      request_id: requestId,
      choice,
    });
  }

  respondClarify(sessionId: string, requestId: string, answer: string): Promise<Record<string, unknown>> {
    return this.rpc("clarify.respond", {
      session_id: sessionId,
      request_id: requestId,
      answer,
    });
  }

  async setSessionConfig(sessionId: string, patch: Record<string, unknown>): Promise<Record<string, unknown>> {
    // Hermes config.set accepts one key/value pair, not a configuration object.
    let result: Record<string, unknown> = {};
    for (const [field, raw] of Object.entries(patch)) {
      if (raw === undefined || field === "provider") continue;
      const key = field === "reasoning_effort" ? "reasoning" : field;
      let value = typeof raw === "boolean" ? (raw ? "on" : "off") : String(raw);
      if (key === "model" && patch.provider && patch.provider !== "auto") {
        value += ` --provider ${patch.provider}`;
      }
      result = await this.rpc("config.set", { session_id: sessionId, key, value, scope: "session" });
      if (result.confirm_required) throw new Error(String(result.confirm_message ?? result.warning ?? "Model change requires confirmation in Hermes"));
    }
    return result;
  }

  dispatchCommand(sessionId: string, input: string): Promise<Record<string, unknown>> {
    const [, name = "", arg = ""] = /^\/?(\S+)\s*([\s\S]*)$/.exec(input.trim()) ?? [];
    return this.rpc("command.dispatch", { session_id: sessionId, name, arg });
  }

  // Projects RPCs
  listProjects(): Promise<Record<string, unknown>> {
    return this.rpc("projects.list", {});
  }

  createProject(payload: {
    name: string;
    description?: string;
    color?: string;
    icon?: string;
    primary_path?: string;
    board_slug?: string;
  }): Promise<Record<string, unknown>> {
    return this.rpc("projects.create", payload);
  }

  updateProject(projectId: string, patch: Record<string, unknown>): Promise<Record<string, unknown>> {
    return this.rpc("projects.update", { project_id: projectId, ...patch });
  }

  deleteProject(projectId: string): Promise<Record<string, unknown>> {
    return this.rpc("projects.delete", { project_id: projectId });
  }

  setActiveProject(projectId: string | null): Promise<Record<string, unknown>> {
    return this.rpc("projects.set_active", { project_id: projectId });
  }

  // OpenAI-compatible SSE streaming fallback (/v1/chat/completions)
  async streamOpenAIChat(params: {
    model: string;
    messages: Pick<HermesMessage, "role" | "content">[];
    sessionKey?: string;
    signal?: AbortSignal;
    onDelta: (delta: string) => void;
  }): Promise<string> {
    const base = this.conn.url.replace(/\/+$/, "");
    const rawTarget = `${base}/v1/chat/completions`;
    const url = this.conn.useProxy
      ? `${(this.conn.proxyBaseUrl ?? (typeof window !== "undefined" ? window.location.origin : "http://127.0.0.1:4173")).replace(/\/+$/, "")}/proxy/http?target=${encodeURIComponent(rawTarget)}`
      : rawTarget;

    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      Accept: "text/event-stream",
    };
    if (this.conn.token) {
      headers["Authorization"] = `Bearer ${this.conn.token}`;
    }
    if (params.sessionKey) {
      headers["X-Hermes-Session-Key"] = params.sessionKey;
    }

    const res = await fetch(url, {
      method: "POST",
      headers,
      signal: params.signal,
      body: JSON.stringify({
        model: params.model,
        stream: true,
        messages: params.messages.map((m) => ({ role: m.role, content: m.content })),
      }),
    });

    if (!res.ok) {
      const errText = await res.text();
      throw new Error(`Chat request failed (${res.status}): ${errText.slice(0, 200)}`);
    }

    const reader = res.body?.getReader();
    if (!reader) return "";
    const decoder = new TextDecoder();
    let buffer = "";
    let full = "";

    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";
      for (const line of lines) {
        const trimmed = line.trim();
        if (!trimmed.startsWith("data:")) continue;
        const payload = trimmed.slice(5).trim();
        if (payload === "[DONE]") break;
        try {
          const json = JSON.parse(payload) as {
            choices?: Array<{ delta?: { content?: string }; message?: { content?: string } }>;
          };
          const piece = json.choices?.[0]?.delta?.content ?? json.choices?.[0]?.message?.content ?? "";
          if (piece) {
            full += piece;
            params.onDelta(piece);
          }
        } catch {
          // ignore non-json chunks
        }
      }
    }
    return full;
  }
}
