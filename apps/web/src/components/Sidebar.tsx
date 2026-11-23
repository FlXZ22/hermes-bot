import React, { useMemo, useState } from "react";
import { CursorIcon, Modal, resolveBotColor, type CursorIconName } from "@grok-bot/theme";
import { AnimatedBotFace } from "./AnimatedBotFace";
import { useDerivedBotState } from "../lib/botState";
import { useApp, useChatSlice, type AppViewId } from "../state/appStore";
import { tauriBridge } from "../lib/tauriBridge";
import { formatRelativeTime } from "../lib/format";

const NAV_ITEMS: Array<{ id: AppViewId; label: string; icon: CursorIconName }> = [
  { id: "chat", label: "Chat", icon: "chat-bubble" },
  { id: "boards", label: "Boards", icon: "board-kanban" },
  { id: "skills", label: "Skills", icon: "sparkles" },
  { id: "cron", label: "Cron", icon: "clock" },
  { id: "memory", label: "Memory", icon: "brain" },
  { id: "gateway", label: "Gateway", icon: "globe" },
  { id: "analytics", label: "Usage", icon: "chart-bars" },
];

const BOARD_COLORS = ["blue", "violet", "cyan", "green", "orange", "red", "magenta", "yellow"];

export const Sidebar: React.FC = () => {
  const {
    theme,
    toggleTheme,
    activeView,
    setActiveView,
    sidebarCollapsed,
    setSidebarCollapsed,
    setCommandPaletteOpen,
    setAvatarEditorTarget,
    isDesktop,
    connectionStatus,
    connectionDetail,
    profiles,
    profileSwitching,
    client,
    refreshCapabilities,
    activeProfile,
    setActiveProfile,
    botAvatars,
    boards,
    activeBoardSlug,
    setActiveBoardSlug,
    sessionBoards,
    createBoard,
    sessions,
    activeSessionId,
    selectSession,
    createNewSession,
    renameSession,
    deleteSession,
    archiveSession,
    togglePinSession,
  } = useApp();

  const isStreaming = useChatSlice(chat => chat.isStreaming);
  const liveStatusText = useChatSlice(chat => chat.liveStatusText);
  const activeToolName = useChatSlice(chat => chat.activeToolName);
  const hasStreamedText = useChatSlice(chat => chat.hasStreamedText);
  const awaitingUserResponse = useChatSlice(chat => chat.awaitingUserResponse);
  const lastTurnSettlement = useChatSlice(chat => chat.lastTurnSettlement);
  const activeBotState = useDerivedBotState({
    isStreaming,
    hasStreamedText,
    activeToolName,
    liveStatusText,
    awaitingUserResponse,
    lastTurnSettlement,
  });
  const [newBotOpen, setNewBotOpen] = useState(false);
  const [botName, setBotName] = useState("");
  const [botDescription, setBotDescription] = useState("");
  const [botBusy, setBotBusy] = useState(false);
  const [botError, setBotError] = useState("");
  const [search, setSearch] = useState("");
  const [showArchived, setShowArchived] = useState(false);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const [hoveredBot, setHoveredBot] = useState<string | null>(null);
  const [hoveredSession, setHoveredSession] = useState<string | null>(null);
  const [newBoardOpen, setNewBoardOpen] = useState(false);
  const [boardName, setBoardName] = useState("");
  const [boardDesc, setBoardDesc] = useState("");
  const [boardPath, setBoardPath] = useState("");
  const [boardColor, setBoardColor] = useState("violet");

  const filteredSessions = useMemo(() => {
    const q = search.trim().toLowerCase();
    return sessions.filter((s) => {
      if (Boolean(s.archived) !== showArchived) return false;
      if (activeBoardSlug !== "all") {
        const assigned = sessionBoards[s.id] ?? s.board_slug ?? "default";
        if (assigned !== activeBoardSlug) return false;
      }
      if (!q) return true;
      const hay = `${s.title ?? ""} ${s.preview ?? ""} ${s.model ?? ""} ${s.profile ?? ""}`.toLowerCase();
      return hay.includes(q);
    });
  }, [activeBoardSlug, search, sessionBoards, sessions, showArchived]);

  const pinnedSessions = useMemo(() => filteredSessions.filter((s) => s.pinned), [filteredSessions]);
  const recentSessions = useMemo(() => filteredSessions.filter((s) => !s.pinned), [filteredSessions]);

  const statusDotClass =
    connectionStatus === "connected"
      ? "gb-status-dot--ok"
      : connectionStatus === "local" || connectionStatus === "connecting"
        ? "gb-status-dot--warn"
        : connectionStatus === "error"
          ? "gb-status-dot--err"
          : "gb-status-dot--muted";

  const handleHeaderMouseDown = (e: React.MouseEvent<HTMLDivElement>) => {
    if (!isDesktop) return;
    if ((e.target as HTMLElement).closest("button, input, a")) return;
    tauriBridge.windowStartDrag();
  };

  const handleCreateBoard = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!boardName.trim()) return;
    await createBoard({
      name: boardName,
      description: boardDesc,
      color: boardColor,
      workspace_path: boardPath,
    });
    setBoardName("");
    setBoardDesc("");
    setBoardPath("");
    setNewBoardOpen(false);
  };

  const renderSessionRow = (s: typeof sessions[number]) => {
    const isActive = activeView === "chat" && activeSessionId === s.id;
    const seed = s.profile || s.id;
    const av = botAvatars[seed];
    const isWorking = isActive && isStreaming;
    const assignedBoardSlug = sessionBoards[s.id] ?? s.board_slug;
    const assignedBoard = boards.find((b) => b.slug === assignedBoardSlug);

    const isHovered = hoveredSession === s.id;
    const activeShouldAnimate = isHovered || activeBotState !== "idle";

    return (
      <div
        key={s.id}
        className="gb-agent-row"
        data-active={isActive ? "true" : undefined}
        role="button"
        tabIndex={0}
        onPointerEnter={() => setHoveredSession(s.id)}
        onPointerLeave={() => setHoveredSession((cur) => (cur === s.id ? null : cur))}
        onClick={() => {
          if (renamingId !== s.id) void selectSession(s.id);
        }}
        onKeyDown={(e) => {
          if (e.key === "Enter" && renamingId !== s.id) void selectSession(s.id);
        }}
      >
        {isActive ? (
          <AnimatedBotFace
            seed={seed}
            shapeId={av?.shapeId}
            colorId={av?.colorId}
            size={sidebarCollapsed ? 22 : 36}
            state={activeBotState}
            isPaused={!activeShouldAnimate}
            isFollowingPointer={isHovered}
            maxFrameRate={30}
          />
        ) : s.archived ? (
          <AnimatedBotFace
            seed={seed}
            shapeId={av?.shapeId}
            colorId="gray"
            size={sidebarCollapsed ? 22 : 24}
            state="sleeping"
            isPaused
            isStatic
          />
        ) : (
          <AnimatedBotFace
            seed={seed}
            shapeId={av?.shapeId}
            colorId={av?.colorId}
            size={sidebarCollapsed ? 22 : 36}
            state="idle"
            isPaused={!isHovered}
            isStatic={!isHovered}
            isFollowingPointer={isHovered}
            maxFrameRate={30}
          />
        )}

        {!sidebarCollapsed && (
          <div className="gb-agent-row__body">
            <div className="gb-agent-row__top">
              {renamingId === s.id ? (
                <input
                  className="gb-input"
                  style={{ padding: "2px 6px", fontSize: 13 }}
                  value={renameDraft}
                  autoFocus
                  onClick={(e) => e.stopPropagation()}
                  onChange={(e) => setRenameDraft(e.target.value)}
                  onBlur={() => {
                    void renameSession(s.id, renameDraft);
                    setRenamingId(null);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      void renameSession(s.id, renameDraft);
                      setRenamingId(null);
                    } else if (e.key === "Escape") {
                      setRenamingId(null);
                    }
                  }}
                />
              ) : (
                <span className="gb-agent-row__name">{s.title || "Untitled Chat"}</span>
              )}

              <span className="gb-agent-row__time">
                {formatRelativeTime(s.updated_at ?? s.started_at ?? s.created_at)}
              </span>

              <span className="gb-agent-row__actions" onClick={(e) => e.stopPropagation()}>
                <button
                  type="button"
                  className="gb-mini-icon-btn"
                  title={s.pinned ? "Unpin" : "Pin"}
                  onClick={() => void togglePinSession(s.id)}
                >
                  <CursorIcon name={s.pinned ? "pin-slash" : "pin"} size={12} />
                </button>
                <button
                  type="button"
                  className="gb-mini-icon-btn"
                  title="Rename"
                  onClick={() => {
                    setRenamingId(s.id);
                    setRenameDraft(s.title || "");
                  }}
                >
                  <CursorIcon name="pencil" size={12} />
                </button>
                <button
                  type="button"
                  className="gb-mini-icon-btn gb-mini-icon-btn--danger"
                  title={s.archived ? "Restore session" : "Archive session"}
                  onClick={() => void archiveSession(s.id, !s.archived)}
                >
                  <CursorIcon name="archive" size={12} />
                </button>
              </span>
            </div>

            {isWorking ? (
              <span className="gb-agent-row__working">
                <CursorIcon name="spinner" size={11} spin />
                Working...
              </span>
            ) : (
              <div className="gb-agent-row__preview">
                {assignedBoard && activeBoardSlug === "all" && assignedBoard.slug !== "default" ? (
                  <span style={{ opacity: 0.72, marginRight: 5 }}>[{assignedBoard.name}]</span>
                ) : null}
                {s.preview || s.model || `${s.message_count ?? 0} messages`}
              </div>
            )}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className="gb-sidebar-slot">
      <aside className="gb-sidebar" data-collapsed={sidebarCollapsed ? "true" : undefined}>
        {/* Top Bar with Authentic Grok Bot Traffic Lights + Circular Header Actions */}
        <div
          className="gb-sidebar-header gb-drag-region"
          data-tauri-drag-region=""
          onMouseDown={handleHeaderMouseDown}
        >
          <div className="gb-sidebar-header__lead gb-no-drag">
            {!sidebarCollapsed && (
              <div className="gb-traffic-lights" aria-label="Window controls">
                <button
                  type="button"
                  className="gb-traffic-light gb-traffic-light--close"
                  title="Close"
                  onClick={() => void tauriBridge.windowClose()}
                />
                <button
                  type="button"
                  className="gb-traffic-light gb-traffic-light--min"
                  title="Minimize"
                  onClick={() => void tauriBridge.windowMinimize()}
                />
                <button
                  type="button"
                  className="gb-traffic-light gb-traffic-light--max"
                  title="Maximize"
                  onClick={() => void tauriBridge.windowToggleMaximize()}
                />
              </div>
            )}
          </div>

          <div className="gb-sidebar-header__actions gb-no-drag">
            <button
              type="button"
              className="gb-circle-btn"
              title={sidebarCollapsed ? "Expand sidebar (Ctrl+B)" : "Collapse sidebar (Ctrl+B)"}
              onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
            >
              <CursorIcon name={sidebarCollapsed ? "layout-sidebar-left-on" : "layout-sidebar-left"} size={15} />
            </button>

            {!sidebarCollapsed && (
              <button
                type="button"
                className="gb-circle-btn"
                title="New Chat"
                onClick={() => void createNewSession()}
              >
                <CursorIcon name="chat-bubble-plus" size={15} />
              </button>
            )}
          </div>
        </div>

        {/* Search Input */}
        {!sidebarCollapsed && (
          <div className="gb-sidebar-search">
            <CursorIcon name="search" size={13} />
            <input
              type="text"
              className="gb-sidebar-search__input"
              placeholder="Search sessions..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
            {search && (
              <button type="button" className="gb-mini-icon-btn" onClick={() => setSearch("")}>
                <CursorIcon name="close" size={11} />
              </button>
            )}
          </div>
        )}

        {/* Session Boards Switcher Strip */}
        {!sidebarCollapsed && (
          <div className="gb-board-strip" aria-label="Session boards">
            <button
              type="button"
              className="gb-board-pill"
              data-active={activeBoardSlug === "all" ? "true" : undefined}
              onClick={() => setActiveBoardSlug("all")}
            >
              All
            </button>
            {boards.map((b) => {
              const preset = resolveBotColor(b.slug, b.color);
              return (
                <button
                  key={b.slug}
                  type="button"
                  className="gb-board-pill"
                  data-active={activeBoardSlug === b.slug ? "true" : undefined}
                  onClick={() => setActiveBoardSlug(b.slug)}
                  title={b.description || b.name}
                >
                  <span className="gb-board-pill__dot" style={{ background: preset.hex }} />
                  {b.name}
                </button>
              );
            })}
            <button
              type="button"
              className="gb-board-pill"
              title="Create Session Board"
              onClick={() => setNewBoardOpen(true)}
            >
              <CursorIcon name="plus" size={11} />
            </button>
          </div>
        )}

        {/* Scrollable Pinned Bots Grid + Sessions */}
        <div className="gb-sidebar-list">
          {/* Signature Grok Bot 3-Column Pinned Bots / Profiles Grid */}
          {!sidebarCollapsed && (
            <>
              <div className="gb-sidebar-section-label">
                <span>Hermes Bots</span>
                <button type="button" className="gb-mini-icon-btn" title="Add bot" aria-label="Add bot" onClick={() => { setBotError(""); setNewBotOpen(true); }}><CursorIcon name="plus" size={13} /></button>
                <button
                  type="button"
                  className="gb-mini-icon-btn"
                  title="Customize active bot avatar"
                  onClick={() =>
                    setAvatarEditorTarget({
                      seed: activeProfile,
                      title: botAvatars[activeProfile]?.displayName || activeProfile,
                    })
                  }
                >
                  <CursorIcon name="sliders" size={12} />
                </button>
              </div>
              <div className="gb-pinned-grid">
                {profiles.map((prof) => {
                  const av = botAvatars[prof.name];
                  const active = activeProfile === prof.name;
                  const isHovered = hoveredBot === prof.name;
                  const tileState = active ? activeBotState : "idle";
                  const shouldAnimate = isHovered || tileState !== "idle";
                  return (
                    <button
                      key={prof.name}
                      type="button"
                      className="gb-pinned-tile"
                      data-active={active ? "true" : undefined}
                      disabled={profileSwitching}
                      aria-pressed={active}
                      onPointerEnter={() => setHoveredBot(prof.name)}
                      onPointerLeave={() => setHoveredBot((cur) => (cur === prof.name ? null : cur))}
                      onClick={() => void setActiveProfile(prof.name)}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        setAvatarEditorTarget({
                          seed: prof.name,
                          title: av?.displayName || prof.name,
                        });
                      }}
                      title={`${prof.name} — right-click to customize Grok Bot avatar`}
                    >
                      <AnimatedBotFace
                        seed={prof.name}
                        shapeId={av?.shapeId}
                        colorId={av?.colorId}
                        size={50}
                        state={tileState}
                        isPaused={!shouldAnimate}
                        isFollowingPointer={isHovered}
                        maxFrameRate={30}
                      />
                      <span className="gb-pinned-tile__label">
                        {av?.displayName || (prof.name === "default" ? "Hermes" : prof.name)}
                      </span>
                    </button>
                  );
                })}
              </div>
              <div className="gb-sidebar-divider" />
            </>
          )}

          {/* Pinned Sessions */}
          {pinnedSessions.length > 0 && (
            <>
              {!sidebarCollapsed && (
                <div className="gb-sidebar-section-label">
                  <span>Pinned</span>
                  <span>{pinnedSessions.length}</span>
                </div>
              )}
              {pinnedSessions.map(renderSessionRow)}
              {!sidebarCollapsed && <div className="gb-sidebar-divider" />}
            </>
          )}

          {/* Recent Sessions */}
          {!sidebarCollapsed && (
            <div className="gb-sidebar-section-label">
              <span>{showArchived ? "Archived" : activeBoardSlug === "all" ? "Sessions" : `Board · ${boards.find((b) => b.slug === activeBoardSlug)?.name ?? activeBoardSlug}`}</span>
              <span style={{display:"flex",alignItems:"center",gap:6}}>{recentSessions.length}<button type="button" className="gb-mini-icon-btn" title={showArchived ? "Show active sessions" : "Show archived sessions"} aria-pressed={showArchived} onClick={() => setShowArchived(value => !value)}><CursorIcon name="archive" size={12}/></button></span>
            </div>
          )}

          {recentSessions.length > 0 ? (
            recentSessions.map(renderSessionRow)
          ) : !sidebarCollapsed ? (
            <div style={{ padding: "14px 10px", fontSize: 12.5, color: "var(--grok-bot-sidebar-text-tertiary)" }}>
              No sessions here yet. Click <strong>+</strong> to start a new chat.
            </div>
          ) : null}
        </div>

        {/* Navigation Dock */}
        <div className="gb-sidebar-nav">
          {NAV_ITEMS.map((item) => (
            <button
              key={item.id}
              type="button"
              className="gb-nav-pill"
              data-active={activeView === item.id ? "true" : undefined}
              title={item.label}
              onClick={() => setActiveView(item.id)}
            >
              <CursorIcon name={item.icon} size={14} />
              {!sidebarCollapsed && <span>{item.label}</span>}
            </button>
          ))}
        </div>

        {/* Footer: Connection Status + Theme + Command Palette + Settings */}
        <div className="gb-sidebar-footer">
          <button
            type="button"
            className="gb-connection-chip"
            title={connectionDetail}
            onClick={() => setActiveView("settings")}
          >
            <span className={`gb-status-dot ${statusDotClass}`} />
            {!sidebarCollapsed && (
              <span
                style={{
                  fontSize: 12,
                  color: "var(--grok-bot-sidebar-text-secondary)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {connectionStatus === "connected"
                  ? "Hermes Live"
                  : connectionStatus === "local"
                    ? "Local Rust"
                    : connectionStatus === "connecting"
                      ? "Connecting..."
                      : "Offline"}
              </span>
            )}
          </button>

          <div style={{ display: "flex", alignItems: "center", gap: 2 }}>
            <button
              type="button"
              className="gb-footer-action"
              title="Command Palette (Ctrl+K)"
              onClick={() => setCommandPaletteOpen(true)}
            >
              <CursorIcon name="command" size={14} />
            </button>
            <button
              type="button"
              className="gb-footer-action"
              title={theme === "dark" ? "Switch to Sand Light" : "Switch to Sand Dark"}
              onClick={toggleTheme}
            >
              <CursorIcon name={theme === "dark" ? "sun" : "moon"} size={14} />
            </button>
            <button
              type="button"
              className="gb-footer-action"
              data-active={activeView === "settings" ? "true" : undefined}
              title="Settings"
              onClick={() => setActiveView("settings")}
            >
              <CursorIcon name="settings-gear" size={14} />
            </button>
          </div>
        </div>
      </aside>

      <Modal open={newBotOpen} title="Add a bot" onClose={() => { if (!botBusy) setNewBotOpen(false); }}>
        <form className="gb-bot-form" onSubmit={async event => {
          event.preventDefault();
          if (!client || botBusy) { if (!client) setBotError("Connect to Hermes to add a bot."); return; }
          const name = botName.trim();
          if (profiles.some(profile => profile.name.toLowerCase() === name.toLowerCase())) { setBotError("A bot with that name already exists."); return; }
          setBotBusy(true); setBotError("");
          try {
            await client.createProfile({ name, description: botDescription.trim() || undefined });
            await refreshCapabilities();
            await setActiveProfile(name);
            setNewBotOpen(false); setBotName(""); setBotDescription("");
          } catch (error) { setBotError(error instanceof Error ? error.message : "Could not create bot"); }
          finally { setBotBusy(false); }
        }}>
          <p className="gb-bot-form__hint">A separate Hermes profile with its own conversations and memory.</p>
          <label className="gb-field"><span className="gb-field__label">Bot name</span><input autoFocus className="gb-input" value={botName} onChange={event => setBotName(event.target.value)} required pattern="[a-zA-Z0-9_-]+" maxLength={48} placeholder="research" disabled={botBusy} /><span className="gb-bot-form__hint">Letters, numbers, underscores, and hyphens.</span></label>
          <label className="gb-field"><span className="gb-field__label">Description</span><input className="gb-input" value={botDescription} onChange={event => setBotDescription(event.target.value)} placeholder="Optional purpose for this bot" disabled={botBusy} /></label>
          {botError && <p role="alert" className="gb-bot-form__error">{botError}</p>}
          <button type="submit" className="gb-btn gb-btn--primary" disabled={botBusy || !botName.trim()}>{botBusy ? "Creating…" : "Create bot"}</button>
        </form>
      </Modal>
      {/* Create New Session Board Modal */}
      <Modal open={newBoardOpen} title="New Session Board" onClose={() => setNewBoardOpen(false)}>
        <form onSubmit={handleCreateBoard} style={{ display: "flex", flexDirection: "column", gap: 14 }}>
          <div className="gb-field">
            <label className="gb-field__label">Board Name</label>
            <input
              className="gb-input"
              placeholder="e.g. Backend Rust Refactor, Research, Ops"
              value={boardName}
              onChange={(e) => setBoardName(e.target.value)}
              autoFocus
              required
            />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Description</label>
            <input
              className="gb-input"
              placeholder="Optional workspace purpose or notes"
              value={boardDesc}
              onChange={(e) => setBoardDesc(e.target.value)}
            />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Workspace Directory (Optional)</label>
            <input
              className="gb-input gb-input--mono"
              placeholder="/home/metis/Projects/..."
              value={boardPath}
              onChange={(e) => setBoardPath(e.target.value)}
            />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Accent Color</label>
            <div className="gb-color-grid">
              {BOARD_COLORS.map((c) => {
                const preset = resolveBotColor(c, c);
                return (
                  <button
                    key={c}
                    type="button"
                    className="gb-color-dot"
                    data-active={boardColor === c ? "true" : undefined}
                    style={{ background: preset.hex }}
                    onClick={() => setBoardColor(c)}
                    title={preset.label}
                  />
                );
              })}
            </div>
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 4 }}>
            <button type="button" className="gb-btn gb-btn--ghost" onClick={() => setNewBoardOpen(false)}>
              Cancel
            </button>
            <button type="submit" className="gb-btn gb-btn--primary" disabled={!boardName.trim()}>
              Create Board
            </button>
          </div>
        </form>
      </Modal>
    </div>
  );
};
