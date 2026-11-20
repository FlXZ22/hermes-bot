import { UsageBar } from "./UsageBar";
import { AnimatedBotFace } from "./AnimatedBotFace";
import { ChatTranscript } from "./ChatTranscript";
import React, { useEffect, useMemo, useRef, useState } from "react";
import { BotMark, CursorIcon, resolveBotColor } from "@grok-bot/theme";
import { useApp, useChatSlice, type ReasoningEffort } from "../state/appStore";
import { shortModelName } from "../lib/format";
import { tauriBridge } from "../lib/tauriBridge";

const SLASH_COMMANDS = [
  { cmd: "/model", desc: "Open model & provider picker" },
  { cmd: "/boards", desc: "Switch to Session Boards (Kanban)" },
  { cmd: "/compress", desc: "Compress conversation context" },
  { cmd: "/branch", desc: "Branch this conversation" },
  { cmd: "/skills", desc: "Manage Hermes skills & MCP" },
  { cmd: "/cron", desc: "Manage scheduled automations" },
  { cmd: "/memory", desc: "Inspect SOUL.md & memories" },
  { cmd: "/clear", desc: "Start a fresh chat session" },
];

const EFFORT_CYCLE: ReasoningEffort[] = ["none", "low", "medium", "high", "xhigh"];

const STARTER_PROMPTS = [
  {
    icon: "code" as const,
    title: "Inspect workspace repo",
    desc: "Summarize architecture, git status, and recent commits",
    prompt: "Inspect the current workspace directory, list key files, and summarize the architecture.",
  },
  {
    icon: "board-kanban" as const,
    title: "Plan a Session Board",
    desc: "Break down a feature into actionable Kanban tasks",
    prompt: "Help me break down my next project milestone into structured tasks for our Session Board.",
  },
  {
    icon: "sparkles" as const,
    title: "Create a reusable Skill",
    desc: "Author a new SKILL.md for Hermes Agent",
    prompt: "Help me design and write a new Hermes Agent skill (SKILL.md) for my daily Arch Linux workflow.",
  },
  {
    icon: "terminal" as const,
    title: "Run diagnostics & check health",
    desc: "Check system resources, tools, and active MCP servers",
    prompt: "Check the local Linux environment, active tools, and verify everything is running smoothly.",
  },
];

export const ChatView: React.FC = () => {
  const {
    isDesktop,
    activeProfile,
    botAvatars,
    activeSession,
    activeSessionId,
    showToast,
    boards,
    activeBoardSlug,
    sessionBoards,
    assignSessionToBoard,
    activeModel,
    activeProvider,
    reasoningEffort,
    fastMode,
    yoloMode,
    setReasoningEffort,
    setFastMode,
    setYoloMode,
    setModelPickerOpen,
    setAvatarEditorTarget,
    setActiveView,
    rightPaneOpen,
    setRightPaneOpen,
    createNewSession,
    sendMessage,
    interruptStream,
    renameSession,
    compressActiveSession,
    branchActiveSession,
  } = useApp();

  const { connection, setupOpen } = useApp();
  const loadingHistory = useChatSlice(chat => chat.loadingHistory);
  const hasMessages = useChatSlice(chat => chat.messages.length > 0);
  const isStreaming = useChatSlice(chat => chat.isStreaming);
  const sessionUsage = useChatSlice(chat => chat.sessionUsage);
  const draftKey = `${connection.id}:${connection.id.startsWith("local-") ? "local" : connection.url}:${activeProfile}:${activeSessionId ?? "new"}`;
  const [drafts, setDrafts] = useState<Record<string, string>>(() => {
    try { return JSON.parse(localStorage.getItem("hermes-grok-bot:drafts") ?? "{}"); } catch { return {}; }
  });
  const draft = drafts[draftKey] ?? "";
  const setDraft = (value: React.SetStateAction<string>) => setDrafts(prev => ({
    ...prev, [draftKey]: typeof value === "function" ? value(prev[draftKey] ?? "") : value,
  }));
  const latestDrafts = useRef(drafts);
  latestDrafts.current = drafts;
  const saveDrafts = () => {
    try { localStorage.setItem("hermes-grok-bot:drafts", JSON.stringify(latestDrafts.current)); } catch { /* Storage may be full. */ }
  };
  useEffect(() => {
    const timer = setTimeout(saveDrafts, 400);
    return () => clearTimeout(timer);
  }, [drafts]);
  useEffect(() => {
    window.addEventListener("pagehide", saveDrafts);
    return () => { window.removeEventListener("pagehide", saveDrafts); saveDrafts(); };
  }, []);
  const [filesBySession, setFilesBySession] = useState<Record<string, File[]>>({});
  const attachments = filesBySession[draftKey] ?? [];
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [attachMenuOpen, setAttachMenuOpen] = useState(false);
  const [editingTitle, setEditingTitle] = useState(false);
  const [titleDraft, setTitleDraft] = useState("");
  const [boardMenuOpen, setBoardMenuOpen] = useState(false);
  const [slashIndex, setSlashIndex] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  // Auto-resize composer textarea
  useEffect(() => {
    const ta = textareaRef.current;
    if (!ta || CSS.supports("field-sizing", "content")) return;
    ta.style.height = "auto";
    ta.style.height = `${Math.min(140, Math.max(24, ta.scrollHeight))}px`;
  }, [draft, setupOpen]);

  const currentBoardSlug = useMemo(() => {
    if (activeSessionId && sessionBoards[activeSessionId]) return sessionBoards[activeSessionId];
    if (activeSession?.board_slug) return activeSession.board_slug;
    if (activeBoardSlug !== "all") return activeBoardSlug;
    return "default";
  }, [activeBoardSlug, activeSession?.board_slug, activeSessionId, sessionBoards]);

  const currentBoard = useMemo(
    () => boards.find((b) => b.slug === currentBoardSlug) ?? boards[0],
    [boards, currentBoardSlug]
  );

  const botSeed = activeSession?.profile || activeProfile || "default";
  const botAv = botAvatars[botSeed];

  const matchingSlash = useMemo(() => {
    if (!draft.startsWith("/") || draft.includes(" ")) return [];
    const q = draft.toLowerCase();
    return SLASH_COMMANDS.filter((c) => c.cmd.startsWith(q));
  }, [draft, setupOpen]);

  const handleSubmit = async (customText?: string) => {
    const text = (customText ?? draft).trim();
    if (!text && !attachments.length) return;

    // Client-side UI shortcut slash commands
    if (text === "/model") {
      setDraft("");
      setModelPickerOpen(true);
      return;
    }
    if (text === "/boards") {
      setDraft("");
      setActiveView("boards");
      return;
    }
    if (text === "/skills") {
      setDraft("");
      setActiveView("skills");
      return;
    }
    if (text === "/cron") {
      setDraft("");
      setActiveView("cron");
      return;
    }
    if (text === "/memory") {
      setDraft("");
      setActiveView("memory");
      return;
    }
    if (text === "/clear") {
      setDraft("");
      await createNewSession();
      return;
    }
    if (text === "/compress") {
      setDraft("");
      await compressActiveSession();
      return;
    }
    if (text === "/branch") {
      setDraft("");
      await branchActiveSession();
      return;
    }

    try {
      await sendMessage(text, attachments);
      setDraft(prev => prev.trim() === text ? "" : prev);
      setFilesBySession(prev => ({ ...prev, [draftKey]: (prev[draftKey] ?? []).filter(file => !attachments.includes(file)) }));
    } catch (error) {
      showToast(error instanceof Error ? error.message : "Message could not be sent");
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (matchingSlash.length > 0) {
      if (e.key === "ArrowDown") {
        e.preventDefault();
        setSlashIndex((i) => (i + 1) % matchingSlash.length);
        return;
      }
      if (e.key === "ArrowUp") {
        e.preventDefault();
        setSlashIndex((i) => (i - 1 + matchingSlash.length) % matchingSlash.length);
        return;
      }
      if (e.key === "Tab" || (e.key === "Enter" && !e.shiftKey)) {
        e.preventDefault();
        const chosen = matchingSlash[slashIndex] ?? matchingSlash[0];
        if (chosen) {
          void handleSubmit(chosen.cmd);
        }
        return;
      }
    }

    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      if (!isStreaming) void handleSubmit();
    }
  };

  const cycleEffort = () => {
    const idx = EFFORT_CYCLE.indexOf(reasoningEffort);
    setReasoningEffort(EFFORT_CYCLE[(idx + 1) % EFFORT_CYCLE.length]);
  };

  const handleHeaderMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isDesktop) return;
    if ((e.target as HTMLElement).closest("button, input, a")) return;
    tauriBridge.windowStartDrag();
  };

  return (
    <main className="gb-chat">
      {/* Floating Frosted Grok Bot Chat Header */}
      <header className="gb-chat-header" data-tauri-drag-region="" onMouseDown={handleHeaderMouseDown}>
        <div className="gb-chat-header__left">
          {editingTitle && activeSessionId ? (
            <div className="gb-title-pill">
              <BotMark seed={botSeed} shapeId={botAv?.shapeId} colorId={botAv?.colorId} size={20} />
              <input
                className="gb-input"
                style={{ width: 220, padding: "2px 7px", fontSize: 13 }}
                value={titleDraft}
                autoFocus
                onChange={(e) => setTitleDraft(e.target.value)}
                onBlur={() => {
                  void renameSession(activeSessionId, titleDraft);
                  setEditingTitle(false);
                }}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    void renameSession(activeSessionId, titleDraft);
                    setEditingTitle(false);
                  } else if (e.key === "Escape") {
                    setEditingTitle(false);
                  }
                }}
              />
            </div>
          ) : (
            <button
              type="button"
              className="gb-title-pill"
              onClick={() => {
                if (activeSessionId) {
                  setTitleDraft(activeSession?.title ?? "New Chat");
                  setEditingTitle(true);
                } else {
                  setAvatarEditorTarget({ seed: botSeed, title: botAv?.displayName || botSeed });
                }
              }}
              title="Click to rename session"
            >
              <AnimatedBotFace
                seed={botSeed}
                shapeId={botAv?.shapeId}
                colorId={botAv?.colorId}
                size={20}
                working={isStreaming}
              />
              <span className="gb-title-pill__name">
                {activeSession?.title || botAv?.displayName || (activeProfile === "default" ? "Hermes" : activeProfile)}
              </span>
              <span className="gb-title-pill__meta">· {shortModelName(activeModel)}</span>
            </button>
          )}
        </div>

        <div className="gb-chat-header__right">
          {activeSessionId && (
            <>
              <button
                type="button"
                className="gb-window-action"
                title="Branch session"
                onClick={() => void branchActiveSession()}
              >
                <CursorIcon name="git-branch" size={14} />
              </button>
              <button
                type="button"
                className="gb-window-action"
                title="Compress context"
                onClick={() => void compressActiveSession()}
              >
                <CursorIcon name="arrows-contract-simple" size={14} />
              </button>
            </>
          )}
          <button
            type="button"
            className="gb-window-action"
            title="New Chat"
            onClick={() => void createNewSession()}
          >
            <CursorIcon name="chat-bubble-plus" size={14} />
          </button>
          <button
            type="button"
            className="gb-window-action"
            data-active={rightPaneOpen ? "true" : undefined}
            title="Toggle Inspector Pane (Ctrl+\)"
            onClick={() => setRightPaneOpen(!rightPaneOpen)}
          >
            <CursorIcon name={rightPaneOpen ? "layout-sidebar-right-on" : "layout-sidebar-right"} size={14} />
          </button>
        </div>
      </header>

      {/* Transcript or Authentic Grok Bot Welcome Hero */}
      {!hasMessages && loadingHistory ? (
        <div className="gb-welcome" role="status"><CursorIcon name="spinner" spin size={18} /><span>Loading conversation…</span></div>
      ) : !hasMessages ? (
        <div className="gb-welcome">
          <button
            type="button"
            onClick={() => setAvatarEditorTarget({ seed: botSeed, title: botAv?.displayName || botSeed })}
            title="Customize Bot Avatar"
            style={{ borderRadius: 28, padding: 6 }}
          >
            <AnimatedBotFace seed={botSeed} shapeId={botAv?.shapeId} colorId={botAv?.colorId} size={76} />
          </button>
          <h1 className="gb-welcome__title">
            {botAv?.displayName || (activeProfile === "default" ? "Hermes" : activeProfile)}
          </h1>
          <p className="gb-welcome__sub">
            Your agent, ready to work. Start a conversation or pick up where you left off.
          </p>

          <div className="gb-welcome__prompts">
            {STARTER_PROMPTS.map((item) => (
              <button
                key={item.title}
                type="button"
                className="gb-prompt-card"
                onClick={() => void handleSubmit(item.prompt)}
              >
                <span className="gb-prompt-card__title">
                  <CursorIcon name={item.icon} size={14} />
                  {item.title}
                </span>
                <span className="gb-prompt-card__desc">{item.desc}</span>
              </button>
            ))}
          </div>
        </div>
      ) : (
        <ChatTranscript key={activeSessionId ?? "new"} />
      )}

      {/* Floating Frosted Grok Bot Pill Composer + Hermes Controls Strip */}
      <div className="gb-composer-wrap">
        <div className="gb-composer-inner">
          {/* Slash Command Autocomplete Popover */}
          {matchingSlash.length > 0 && (
            <div className="gb-popover" role="listbox">
              {matchingSlash.map((item, idx) => (
                <button
                  key={item.cmd}
                  type="button"
                  className="gb-popover-item"
                  data-highlighted={idx === slashIndex ? "true" : undefined}
                  onMouseEnter={() => setSlashIndex(idx)}
                  onClick={() => void handleSubmit(item.cmd)}
                >
                  <span style={{ fontFamily: "var(--cursor-font-family-mono)", fontWeight: 600 }}>{item.cmd}</span>
                  <span style={{ fontSize: 12, color: "var(--grok-bot-text-tertiary)", marginLeft: "auto" }}>
                    {item.desc}
                  </span>
                </button>
              ))}
            </div>
          )}

          {/* Board Assignment Quick Popover */}
          {boardMenuOpen && (
            <div className="gb-popover" style={{ left: "auto", right: 0 }}>
              <div style={{ padding: "4px 9px 6px", fontSize: 11, color: "var(--grok-bot-text-tertiary)", fontWeight: 600 }}>
                Assign Session Board
              </div>
              {boards.map((b) => {
                const preset = resolveBotColor(b.slug, b.color);
                return (
                  <button
                    key={b.slug}
                    type="button"
                    className="gb-popover-item"
                    data-highlighted={currentBoard?.slug === b.slug ? "true" : undefined}
                    onClick={() => {
                      if (activeSessionId) assignSessionToBoard(activeSessionId, b.slug);
                      setBoardMenuOpen(false);
                    }}
                  >
                    <span className="gb-board-pill__dot" style={{ background: preset.hex }} />
                    <span>{b.name}</span>
                    {currentBoard?.slug === b.slug && (
                      <CursorIcon name="check" size={13} style={{ marginLeft: "auto" }} />
                    )}
                  </button>
                );
              })}
            </div>
          )}

          {/* Hermes Status & Model Strip right above the floating pill */}
          <div className="gb-composer-strip">
            <div className="gb-composer-strip__group">
              <button
                type="button"
                className="gb-chip-btn"
                onClick={() => setModelPickerOpen(true)}
                title={`Active Model: ${activeModel} (${activeProvider}) — Ctrl+Shift+M`}
              >
                <CursorIcon name="sparkles" size={12} />
                <span>{shortModelName(activeModel)}</span>
                <CursorIcon name="chevron-down-small" size={11} />
              </button>

              <button
                type="button"
                className="gb-chip-btn"
                data-active={reasoningEffort !== "none" ? "true" : undefined}
                onClick={cycleEffort}
                title="Cycle reasoning effort (none / low / medium / high / xhigh)"
              >
                <CursorIcon name="brain" size={12} />
                <span>{reasoningEffort}</span>
              </button>

              <button
                type="button"
                className="gb-chip-btn"
                data-active={fastMode ? "true" : undefined}
                onClick={() => setFastMode(!fastMode)}
                title="Toggle priority / fast inference"
              >
                <CursorIcon name="zap" size={12} />
                <span>Fast</span>
              </button>

              <button
                type="button"
                className="gb-chip-btn"
                data-active={yoloMode ? "true" : undefined}
                onClick={() => setYoloMode(!yoloMode)}
                title={yoloMode ? "YOLO mode: auto-approve tools" : "Ask before dangerous commands"}
              >
                <CursorIcon name={yoloMode ? "shield-check" : "shield"} size={12} />
                <span>{yoloMode ? "YOLO" : "Ask"}</span>
              </button>
            </div>

            <div className="gb-composer-strip__group">
              {currentBoard && (
                <button
                  type="button"
                  className="gb-chip-btn"
                  onClick={() => setBoardMenuOpen((v) => !v)}
                  title="Change session board"
                >
                  <span
                    className="gb-board-pill__dot"
                    style={{ background: resolveBotColor(currentBoard.slug, currentBoard.color).hex }}
                  />
                  <span>{currentBoard.name}</span>
                </button>
              )}


            </div>
          </div>

          <UsageBar usage={sessionUsage} contextLimit={activeSession?.context_limit} />
          <input ref={fileInputRef} type="file" multiple hidden onChange={event => {
            const selected = Array.from(event.target.files ?? []);
            if (selected.some(file => file.size > 50 * 1024 * 1024)) {
              showToast("Choose files under 50 MB each.");
            } else {
              setFilesBySession(prev => ({ ...prev, [draftKey]: [...(prev[draftKey] ?? []), ...selected] }));
            }
            event.target.value = "";
          }} />
          {attachments.length > 0 && <div className="gb-attachment-list">
            {attachments.map((file, index) => <span className="gb-attachment" key={`${file.name}-${index}`}>
              <CursorIcon name="file" size={13} /> {file.name}
              <button type="button" aria-label={`Remove ${file.name}`} onClick={() => setFilesBySession(prev => ({ ...prev, [draftKey]: attachments.filter((_, i) => i !== index) }))}><CursorIcon name="close" size={12} /></button>
            </span>)}
          </div>}
          {attachMenuOpen && <div className="gb-attach-menu">
            <button type="button" className="gb-btn gb-btn--ghost" onClick={() => { fileInputRef.current?.click(); setAttachMenuOpen(false); }}><CursorIcon name="file" size={14} /> Attach files</button>
            <button type="button" className="gb-btn gb-btn--ghost" onClick={() => { setDraft(prev => prev.startsWith("/") ? prev : `/${prev}`); setAttachMenuOpen(false); textareaRef.current?.focus(); }}>Slash commands</button>
          </div>}
          {/* Floating Frosted Glass Pill Composer */}
          <div className="gb-composer">
            <button
              type="button"
              className="gb-composer__plus"
              title="Attachments and commands"
              aria-expanded={attachMenuOpen}
              onClick={() => setAttachMenuOpen(value => !value)}
            >
              <CursorIcon name="plus" size={15} />
            </button>

            <textarea
              ref={textareaRef}
              rows={1}
              className="gb-composer__textarea"
              placeholder={`Message ${botAv?.displayName || (activeProfile === "default" ? "Hermes" : activeProfile)}... (type / for commands)`}
              value={draft}
              onChange={(e) => {
                setDraft(e.target.value);
                setSlashIndex(0);
              }}
              onKeyDown={handleKeyDown}
            />

            {isStreaming ? (
              <button
                type="button"
                className="gb-composer__send gb-composer__send--stop"
                title="Stop generating"
                onClick={() => void interruptStream()}
              >
                <CursorIcon name="stop" size={13} />
              </button>
            ) : (
              <button
                type="button"
                className="gb-composer__send"
                disabled={!draft.trim() && attachments.length === 0}
                title="Send message (Enter)"
                onClick={() => void handleSubmit()}
              >
                <CursorIcon name="arrow-up" size={15} />
              </button>
            )}
          </div>
        </div>
      </div>
    </main>
  );
};
