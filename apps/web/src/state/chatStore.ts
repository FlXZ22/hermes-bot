import type { HermesMessage, HermesSessionUsage, HermesTodoItem } from "@grok-bot/hermes-client";

export interface ChatSnapshot {
  messages: HermesMessage[];
  messageIds: string[];
  firstItemIndex: number;
  olderOffset: number | null;
  loadingHistory: boolean;
  todos: HermesTodoItem[];
  sessionUsage: HermesSessionUsage | null;
  isStreaming: boolean;
  liveStatusText: string;
}

const empty: ChatSnapshot = { messages: [], messageIds: [], firstItemIndex: 1_000_000, olderOffset: null, loadingHistory: false, todos: [], sessionUsage: null, isStreaming: false, liveStatusText: "" };
type MessageUpdate = (messages: HermesMessage[]) => HermesMessage[];

export class ChatStore {
  private sessions = new Map<string, ChatSnapshot>();
  private messagesById = new Map<string, Map<string, HermesMessage>>();
  private recent = new Set<string>();
  private listeners = new Map<string, Set<() => void>>();
  private deltas = new Map<string, { text: string[]; thinking: string[]; messageId?: string }>();
  private timer: ReturnType<typeof setTimeout> | undefined;
  readonly liveIds: Record<string, string> = Object.create(null);

  get = (id: string | null) => id === null ? empty : this.sessions.get(id) ?? empty;

  retain(id: string) {
    this.recent.delete(id);
    this.recent.add(id);
    if (this.recent.size <= 32) return;
    for (const candidate of this.recent) {
      const session = this.get(candidate);
      if (candidate === id || this.listeners.has(candidate) || session.isStreaming || session.loadingHistory) continue;
      this.recent.delete(candidate);
      this.sessions.delete(candidate);
      this.messagesById.delete(candidate);
      if (this.recent.size <= 32) break;
    }
  }

  getMessage = (id: string | null, messageId: string) => id === null ? undefined : this.messagesById.get(id)?.get(messageId);

  subscribe = (id: string | null, listener: () => void) => {
    if (id === null) return () => {};
    const listeners = this.listeners.get(id) ?? new Set<() => void>();
    listeners.add(listener);
    this.listeners.set(id, listeners);
    return () => {
      listeners.delete(listener);
      if (!listeners.size) this.listeners.delete(id);
    };
  };

  patch(id: string, patch: Partial<ChatSnapshot>) {
    const previous = this.get(id);
    if (!(Object.keys(patch) as (keyof ChatSnapshot)[]).some(key => previous[key] !== patch[key])) return;
    const next = { ...previous, ...patch };
    if (patch.messages && patch.messages !== previous.messages) {
      const messages = patch.messages;
      const byId = this.messagesById.get(id) ?? new Map<string, HermesMessage>();
      const sameIds = messages.length === previous.messageIds.length &&
        messages.every((message, index) => previous.messageIds[index] === message.id);
      if (!sameIds) {
        byId.clear();
        next.messageIds = messages.map(message => message.id);
        for (const message of messages) byId.set(message.id, message);
      } else {
        for (let index = 0; index < messages.length; index++) {
          const message = messages[index];
          if (message !== previous.messages[index]) byId.set(message.id, message);
        }
      }
      this.messagesById.set(id, byId);
    }
    this.sessions.set(id, next);
    if (previous === empty) this.retain(id);
    this.listeners.get(id)?.forEach(listener => listener());
  }

  prepend(id: string, incoming: HermesMessage[], olderOffset: number | null) {
    const previous = this.get(id);
    const known = new Set(previous.messageIds);
    const added = incoming.filter(message => !known.has(message.id));
    this.patch(id, {
      messages: added.length ? [...added, ...previous.messages] : previous.messages,
      firstItemIndex: previous.firstItemIndex - added.length,
      olderOffset,
    });
  }

  updateMessages(id: string, update: MessageUpdate) {
    this.flush(id, update);
  }

  buffer(id: string, content: string, field: "text" | "thinking" = "text", messageId?: string) {
    let pending = this.deltas.get(id);
    if (pending && pending.messageId !== messageId) {
      this.flush(id);
      pending = undefined;
    }
    if (!pending) {
      pending = { text: [], thinking: [], messageId };
      this.deltas.set(id, pending);
    }
    pending[field].push(content);
    this.patch(id, { isStreaming: true });
    this.timer ??= setTimeout(() => {
      this.timer = undefined;
      for (const sessionId of this.deltas.keys()) this.flush(sessionId);
    }, 24);
  }

  private flush(id: string, update?: MessageUpdate) {
    const delta = this.deltas.get(id);
    this.deltas.delete(id);
    if (!this.deltas.size && this.timer !== undefined) {
      clearTimeout(this.timer);
      this.timer = undefined;
    }
    let messages = this.get(id).messages;
    if (delta) {
      const index = delta.messageId
        ? lastMessageIndex(messages, message => message.id === delta.messageId)
        : lastMessageIndex(messages, message => message.role === "assistant" && message.streaming === true);
      const text = delta.text.join("");
      const thinking = delta.thinking.join("");
      if (index >= 0) {
        messages = messages.slice();
        const previous = messages[index];
        messages[index] = {
          ...previous,
          content: previous.content + text,
          thinking: thinking ? (previous.thinking ?? "") + thinking : previous.thinking,
        };
      } else if (!delta.messageId) {
        messages = [...messages, newAssistant({ content: text, thinking: thinking || undefined })];
      }
    }
    if (update) messages = update(messages);
    this.patch(id, { messages });
  }

  finish(id: string, content?: string, messageId?: string) {
    this.updateMessages(id, messages => {
      const index = lastMessageIndex(messages, message => message.streaming === true && (!messageId || message.id === messageId));
      if (index < 0) return content && !messageId && messages.at(-1)?.content !== content
        ? [...messages, { ...newAssistant({ content }), streaming: false }]
        : messages;
      const next = messages.slice();
      next[index] = { ...next[index], content: next[index].content || content || "", streaming: false };
      return next;
    });
    this.patch(id, { isStreaming: false, liveStatusText: "" });
  }

  disconnect() {
    for (const id of this.sessions.keys()) {
      if (this.get(id).isStreaming || this.deltas.has(id)) this.finish(id);
    }
    for (const key of Object.keys(this.liveIds)) delete this.liveIds[key];
  }
}

let nextMessageId = 0;
export const newAssistant = (patch: Partial<HermesMessage> = {}): HermesMessage => ({
  id: `asst-${Date.now()}-${++nextMessageId}`,
  role: "assistant",
  content: "",
  streaming: true,
  timestamp: Date.now(),
  ...patch,
});

export function lastMessageIndex(messages: HermesMessage[], matches: (message: HermesMessage) => boolean) {
  for (let index = messages.length - 1; index >= 0; index--) {
    if (matches(messages[index])) return index;
  }
  return -1;
}
