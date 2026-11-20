import type { ChatSnapshot } from "../state/chatStore";
import React, { memo, useEffect, useCallback, useDeferredValue, useId, useMemo, useRef, useState } from "react";
import { Virtuoso } from "react-virtuoso";
import { ApprovalCard, BotMark, ClarifyCard, CursorIcon } from "@grok-bot/theme";
import type { HermesMessage, HermesToolCall } from "@grok-bot/hermes-client";
import { useApp, useChatMessage, useChatSlice } from "../state/appStore";
import { CodeBlock, MarkdownBody } from "../lib/markdown";
import { shortModelName } from "../lib/format";

const ToolPayload = memo(function ToolPayload({ value, label }: { value: unknown; label: string }) {
  const payload = useMemo(() => {
    if (typeof value !== "string") return { code: JSON.stringify(value, null, 2) ?? "", lang: "json" };
    try { return { code: JSON.stringify(JSON.parse(value), null, 2), lang: "json" }; }
    catch { return { code: value, lang: "text" }; }
  }, [value]);
  return <section><h4 className="gb-tool-payload-label">{label}</h4><CodeBlock {...payload} /></section>;
});

const ToolActivityCard = memo(function ToolActivityCard({ tool }: { tool: HermesToolCall }) {
  const [open, setOpen] = useState(false);
  const bodyId = useId();
  return <div className="gb-tool-card">
    <button type="button" className="gb-tool-card__header" aria-expanded={open} aria-controls={bodyId} onClick={() => setOpen(v => !v)}>
      <CursorIcon name={tool.status === "running" ? "spinner" : tool.status === "error" ? "x-circle" : "check-circle"} size={14} spin={tool.status === "running"} />
      <span className="gb-tool-card__name">{tool.status === "running" ? "Using" : "Used"} {tool.name}</span>
      <span className="gb-tool-card__detail">{tool.status === "error" ? "Failed" : tool.preview || (tool.status === "running" ? "Running…" : "Completed")}</span>
      {tool.durationMs != null && <span className="gb-tool-duration">{(tool.durationMs / 1000).toFixed(1)}s</span>}
      <CursorIcon name={open ? "chevron-down" : "chevron-right"} size={12} />
    </button>
    {open && <div id={bodyId} className="gb-tool-card__body">
      {tool.arguments != null && <ToolPayload value={tool.arguments} label="Input" />}
      {tool.output != null && <ToolPayload value={tool.output} label="Output" />}
      {tool.arguments == null && tool.output == null && <span>No payload available yet.</span>}
    </div>}
  </div>;
});

const ThinkingDisclosure = memo(function ThinkingDisclosure({ text, streaming }: { text: string; streaming?: boolean }) {
  const [open, setOpen] = useState(false);
  const bodyId = useId();
  if (!text) return null;
  return <div className="gb-thinking-card">
    <button type="button" className="gb-thinking-card__head" aria-expanded={open} aria-controls={bodyId} onClick={() => setOpen(v => !v)}>
      <CursorIcon name={streaming ? "spinner" : "brain"} size={13} spin={Boolean(streaming)} />
      <span>{streaming ? "Thinking…" : "Thought process"}</span>
      <CursorIcon name={open ? "chevron-down" : "chevron-right"} size={12} style={{ marginLeft: "auto" }} />
    </button>
    {open && <div id={bodyId} className="gb-thinking-card__body"><MarkdownBody content={text} streaming={streaming} /></div>}
  </div>;
});

const MessageItem = memo(function MessageItem({ msg, respondApproval, respondClarify }: {
  msg: HermesMessage;
  respondApproval: ReturnType<typeof useApp>["respondApproval"];
  respondClarify: ReturnType<typeof useApp>["respondClarify"];
}) {
  const deferredContent = useDeferredValue(msg.content);
  const renderedContent = msg.streaming ? deferredContent : msg.content;
  if (msg.role === "tool") {
    return <div className="gb-entry gb-entry--assistant"><ToolActivityCard tool={{ id: msg.id, name: msg.toolName ?? "tool", output: msg.content, status: "done" }} /></div>;
  }
  if (msg.role === "user") {
    return (
      <div key={msg.id} className="gb-entry gb-entry--user">
        <div className="gb-bubble-user">{msg.content}</div>
      </div>
    );
  }

  return (
    <div key={msg.id} className="gb-entry gb-entry--assistant" style={{ gap: 8 }}>
      {msg.thinking ? <ThinkingDisclosure text={msg.thinking} streaming={msg.streaming} /> : null}

      {msg.toolCalls && msg.toolCalls.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 6, width: "100%" }}>
          {msg.toolCalls.map((tc) => (
            <ToolActivityCard key={tc.id} tool={tc} />
          ))}
        </div>
      )}

      {msg.approval && (
        <ApprovalCard
          title={msg.approval.title}
          detail={msg.approval.detail}
          settled={msg.approval.settled}
          onAllowOnce={() => void respondApproval(msg.approval!.id, "once")}
          onAllowAlways={() => void respondApproval(msg.approval!.id, "always")}
          onDeny={() => void respondApproval(msg.approval!.id, "deny")}
        />
      )}

      {msg.clarify && (
        <ClarifyCard
          question={msg.clarify.question}
          choices={msg.clarify.choices}
          answered={msg.clarify.answered}
          onAnswer={(ans) => void respondClarify(msg.clarify!.id, ans)}
        />
      )}

      {msg.content ? (
        <div className="gb-bubble-agent">
          <MarkdownBody content={renderedContent} streaming={msg.streaming} />
        </div>
      ) : null}

      {msg.content && !msg.streaming && (
        <div className="gb-msg-meta">
          {msg.model && <span>{shortModelName(msg.model)}</span>}
          <button
            type="button"
            className="gb-mini-icon-btn"
            title="Copy message"
            onClick={() => navigator.clipboard?.writeText(msg.content).catch(() => undefined)}
          >
            <CursorIcon name="copy" size={11} />
          </button>
        </div>
      )}
    </div>
  );

});

const SubscribedMessage = memo(function SubscribedMessage({ id, ...actions }: Pick<React.ComponentProps<typeof MessageItem>, "respondApproval" | "respondClarify"> & { id: string }) {
  const msg = useChatMessage(id);
  return msg ? <MessageItem msg={msg} {...actions} /> : null;
});

const TranscriptHeader = ({ context }: { context?: { plan: React.ReactNode; pending: React.ReactNode } }) => <div className="gb-transcript-top">{context?.plan}</div>;
const TranscriptFooter = ({ context }: { context?: { plan: React.ReactNode; pending: React.ReactNode } }) => <div className="gb-transcript-bottom">{context?.pending}</div>;
const components = { Header: TranscriptHeader, Footer: TranscriptFooter };

// Separate boundary: typing a draft never reconciles the message list.
type TranscriptProps = Pick<ChatSnapshot, "todos" | "isStreaming" | "liveStatusText"> & {
  messages?: HermesMessage[];
  messageIds?: string[];
  firstItemIndex?: number;
  loadOlder?: () => Promise<void>;
  loadingHistory?: boolean;
} & Pick<ReturnType<typeof useApp>, "activeProfile" | "activeSession" | "botAvatars" | "respondApproval" | "respondClarify">;
export const ChatTranscriptView = memo(function ChatTranscriptView({ messages, messageIds, firstItemIndex = 0, loadOlder, loadingHistory, todos, isStreaming, liveStatusText, activeProfile, activeSession,
    botAvatars, respondApproval, respondClarify }: TranscriptProps) {
  const botSeed = activeSession?.profile || activeProfile || "default";
  const botAv = botAvatars[botSeed];
  const scroller = useRef<HTMLElement | null>(null);
  const setScroller = useCallback((element: HTMLElement | Window | null) => {
    scroller.current = element instanceof HTMLElement ? element : null;
  }, []);
  const pinned = useRef(true);
  const followFrame = useRef<number | null>(null);
  const goToBottom = useCallback(() => {
    if (followFrame.current !== null) return;
    followFrame.current = requestAnimationFrame(() => {
      followFrame.current = null;
      const element = scroller.current;
      if (element && pinned.current) element.scrollTop = element.scrollHeight;
    });
  }, []);
  useEffect(() => () => {
    if (followFrame.current !== null) cancelAnimationFrame(followFrame.current);
  }, []);
  const [atBottom, setAtBottom] = useState(true);
  const plan = <>
    {loadOlder && <button type="button" className="gb-btn gb-history-more" disabled={loadingHistory}
      onClick={() => void loadOlder()}>{loadingHistory ? "Loading…" : "Load older messages"}</button>}
    {/* Live Agent Todo Checklist */}
    {todos.length > 0 && (
      <div className="gb-section-card" style={{ marginBottom: 4 }}>
        <div className="gb-section-card__head" style={{ padding: "9px 13px" }}>
          <span className="gb-section-card__title" style={{ fontSize: 12.5, display: "flex", alignItems: "center", gap: 6 }}>
            <CursorIcon name="list-todo" size={13} />
            Agent Plan ({todos.filter((t) => t.status === "completed").length}/{todos.length})
          </span>
        </div>
        <div style={{ padding: "8px 13px", display: "flex", flexDirection: "column", gap: 5 }}>
          {todos.map((t, idx) => (
            <div key={t.id ?? idx} style={{ display: "flex", alignItems: "center", gap: 7, fontSize: 12.5 }}>
              <CursorIcon
                name={
                  t.status === "completed"
                    ? "check-circle"
                    : t.status === "in_progress"
                      ? "spinner"
                      : "circle"
                }
                size={13}
                spin={t.status === "in_progress"}
                style={{
                  color:
                    t.status === "completed"
                      ? "var(--grok-bot-text-success)"
                      : t.status === "in_progress"
                        ? "var(--grok-bot-text-accent)"
                        : "var(--grok-bot-text-tertiary)",
                }}
              />
              <span
                style={{
                  textDecoration: t.status === "completed" ? "line-through" : undefined,
                  color: t.status === "completed" ? "var(--grok-bot-text-tertiary)" : "var(--grok-bot-text-primary)",
                }}
              >
                {t.content}
              </span>
            </div>
          ))}
        </div>
      </div>
    )}

</>;
  const pending = <>
    {isStreaming && (
      <div className="gb-pending">
        <BotMark seed={botSeed} shapeId={botAv?.shapeId} colorId={botAv?.colorId} size={22} working />
        <span className="gb-pending__label">
          <span>{liveStatusText || "Hermes is thinking"}</span>
          <span className="gb-typing-dots">
            <span className="gb-typing-dot" />
            <span className="gb-typing-dot" />
            <span className="gb-typing-dot" />
          </span>
        </span>
      </div>
    )}</>;
  const renderMessage = useCallback((_: number, entry: string | HermesMessage) => (
    <div className="gb-transcript-row">
      {typeof entry === "string"
        ? <SubscribedMessage id={entry} respondApproval={respondApproval} respondClarify={respondClarify} />
        : <MessageItem msg={entry} respondApproval={respondApproval} respondClarify={respondClarify} />}
    </div>
  ), [respondApproval, respondClarify]);
  const rows = messageIds ?? messages ?? [];
  return <div className="gb-transcript-shell">
    <Virtuoso scrollerRef={setScroller} className="gb-transcript" data={rows}
      computeItemKey={(_, entry) => typeof entry === "string" ? entry : entry.id}
      firstItemIndex={firstItemIndex}
      initialTopMostItemIndex={rows.length - 1}
      increaseViewportBy={400} defaultItemHeight={120}
      atBottomThreshold={48}
      atBottomStateChange={setAtBottom}
      onScrollCapture={event => {
        // Nested code/table scrollers must not change transcript following.
        if (event.target !== event.currentTarget) return;
        const el = event.currentTarget;
        pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight < 48;
      }}
      onWheelCapture={event => { if (event.deltaY < 0) pinned.current = false; }}
      totalListHeightChanged={() => { if (pinned.current) goToBottom(); }}
      components={components} context={{ plan, pending }} itemContent={renderMessage}
    />
    {!atBottom && <button className="gb-jump-latest gb-btn" onClick={() => { pinned.current = true; goToBottom(); }}>Jump to latest <CursorIcon name="chevron-down" size={14} /></button>}
  </div>;
});

export const ChatTranscript = memo(function ChatTranscript() {
  const { activeProfile, activeSession, botAvatars, respondApproval, respondClarify, loadOlderMessages } = useApp();
  const messageIds = useChatSlice(chat => chat.messageIds);
  const firstItemIndex = useChatSlice(chat => chat.firstItemIndex);
  const hasOlder = useChatSlice(chat => chat.olderOffset !== null);
  const loadingHistory = useChatSlice(chat => chat.loadingHistory);
  const todos = useChatSlice(chat => chat.todos);
  const isStreaming = useChatSlice(chat => chat.isStreaming);
  const liveStatusText = useChatSlice(chat => chat.liveStatusText);
  return <ChatTranscriptView messageIds={messageIds} firstItemIndex={firstItemIndex}
    loadOlder={hasOlder ? loadOlderMessages : undefined} loadingHistory={loadingHistory} todos={todos} isStreaming={isStreaming}
    liveStatusText={liveStatusText} activeProfile={activeProfile} activeSession={activeSession}
    botAvatars={botAvatars} respondApproval={respondApproval} respondClarify={respondClarify} />;
});
