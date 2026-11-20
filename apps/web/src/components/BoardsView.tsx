import React, { useMemo, useState } from "react";
import { BotMark, CursorIcon, Modal, resolveBotColor } from "@grok-bot/theme";
import type { HermesKanbanStatus, HermesKanbanTask } from "@grok-bot/hermes-client";
import { useApp } from "../state/appStore";
import { formatRelativeTime, shortModelName } from "../lib/format";

const COLUMNS: Array<{ id: HermesKanbanStatus; label: string; dot: string }> = [
  { id: "triage", label: "Triage", dot: "#777777" },
  { id: "todo", label: "To Do", dot: "#1084FE" },
  { id: "scheduled", label: "Scheduled", dot: "#97683D" },
  { id: "blocked", label: "Blocked", dot: "#FF6700" },
  { id: "ready", label: "Ready", dot: "#9159FE" },
  { id: "running", label: "Running", dot: "#FF9800" },
  { id: "review", label: "Review", dot: "#00BCA6" },
  { id: "done", label: "Done", dot: "#00C972" },
];

const priorityNumber = (value: unknown) => {
  const legacy: Record<string, number> = { low: -1, medium: 0, high: 1, urgent: 2 };
  return typeof value === "string" && value in legacy ? legacy[value] : Number(value ?? 0);
};

export const BoardsView: React.FC = () => {
  const {
    boards,
    activeBoardSlug,
    setActiveBoardSlug,
    createBoard,
    deleteBoard,
    tasks,
    createTask,
    updateTask,
    deleteTask,
    dispatchBoardWorkers,
    launchSessionForTask,
    sessions,
    sessionBoards,
    selectSession,
    createNewSession,
    profiles,
    models,
    activeModel,
    activeProvider,
    activeProfile,
    botAvatars,
  } = useApp();

  const [mode, setMode] = useState<"kanban" | "sessions">("kanban");
  const [dragTaskId, setDragTaskId] = useState<string | null>(null);
  const [dropCol, setDropCol] = useState<HermesKanbanStatus | null>(null);

  // Task Modal state
  const [taskModalOpen, setTaskModalOpen] = useState(false);
  const [editingTask, setEditingTask] = useState<HermesKanbanTask | null>(null);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");
  const [status, setStatus] = useState<HermesKanbanStatus>("todo");
  const [priority, setPriority] = useState("0");
  const [assignee, setAssignee] = useState(activeProfile);
  const [model, setModel] = useState(activeModel);
  const [provider, setProvider] = useState(activeProvider);
  const [cwd, setCwd] = useState("");

  // New Board modal
  const [boardModalOpen, setBoardModalOpen] = useState(false);
  const [newBoardName, setNewBoardName] = useState("");
  const [newBoardDesc, setNewBoardDesc] = useState("");
  const [newBoardPath, setNewBoardPath] = useState("");
  const [newBoardColor, setNewBoardColor] = useState("violet");

  const activeBoard = useMemo(
    () => (activeBoardSlug === "all" ? null : boards.find((b) => b.slug === activeBoardSlug) ?? null),
    [activeBoardSlug, boards]
  );

  const visibleTasks = useMemo(() => {
    if (activeBoardSlug === "all") return tasks;
    return tasks.filter((t) => (t.board || "default") === activeBoardSlug);
  }, [activeBoardSlug, tasks]);

  const visibleSessions = useMemo(() => {
    if (activeBoardSlug === "all") return sessions;
    return sessions.filter((s) => (sessionBoards[s.id] ?? s.board_slug ?? "default") === activeBoardSlug);
  }, [activeBoardSlug, sessionBoards, sessions]);

  const openNewTask = (defaultStatus: HermesKanbanStatus = "todo") => {
    setEditingTask(null);
    setTitle("");
    setBody("");
    setStatus(defaultStatus);
    setPriority("0");
    setAssignee(activeProfile);
    setModel(activeModel);
    setProvider(activeProvider);
    setCwd(activeBoard?.workspace_path ?? "");
    setTaskModalOpen(true);
  };

  const openEditTask = (task: HermesKanbanTask) => {
    setEditingTask(task);
    setTitle(task.title);
    setBody(task.body ?? task.description ?? "");
    setStatus(task.status);
    setPriority(String(priorityNumber(task.priority)));
    setAssignee(task.assignee ?? activeProfile);
    setModel(task.model ?? activeModel);
    setProvider(task.provider ?? activeProvider);
    setCwd(task.cwd ?? "");
    setTaskModalOpen(true);
  };

  const handleSaveTask = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim()) return;
    if (editingTask) {
      await updateTask(editingTask.id, {
        title: title.trim(),
        body: body.trim(),
        status,
        priority,
        assignee,
        model,
        provider,
        cwd: cwd.trim() || undefined,
      });
    } else {
      await createTask({
        title: title.trim(),
        body: body.trim(),
        status,
        priority,
        assignee,
        model,
        provider,
        board: activeBoardSlug === "all" ? "default" : activeBoardSlug,
        cwd: cwd.trim() || undefined,
      });
    }
    setTaskModalOpen(false);
  };

  const handleCreateBoard = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newBoardName.trim()) return;
    await createBoard({
      name: newBoardName,
      description: newBoardDesc,
      color: newBoardColor,
      workspace_path: newBoardPath,
    });
    setNewBoardName("");
    setNewBoardDesc("");
    setNewBoardPath("");
    setBoardModalOpen(false);
  };

  const priorityBadgeClass = (p?: string | number) => {
    const v = priorityNumber(p);
    if (v >= 2) return "gb-badge--danger";
    if (v >= 1) return "gb-badge--warning";
    return "gb-badge--muted";
  };

  return (
    <div className="gb-panel">
      {/* Header */}
      <div className="gb-panel__header">
        <div style={{ display: "flex", alignItems: "center", gap: 12, minWidth: 0 }}>
          <div>
            <h2 className="gb-panel__title">
              {activeBoard ? activeBoard.name : "All Session Boards"}
            </h2>
            <div className="gb-panel__sub">
              {activeBoard?.description ||
                `${visibleTasks.length} tasks · ${visibleSessions.length} sessions across ${boards.length} boards`}
            </div>
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <div className="gb-segmented">
            <button
              type="button"
              className="gb-segmented__item"
              data-active={mode === "kanban" ? "true" : undefined}
              onClick={() => setMode("kanban")}
            >
              Kanban
            </button>
            <button
              type="button"
              className="gb-segmented__item"
              data-active={mode === "sessions" ? "true" : undefined}
              onClick={() => setMode("sessions")}
            >
              Sessions ({visibleSessions.length})
            </button>
          </div>

          <button
            type="button"
            className="gb-btn gb-btn--secondary"
            onClick={() => void dispatchBoardWorkers(activeBoardSlug)}
            title="Dispatch Hermes autonomous workers for Ready tasks"
          >
            <CursorIcon name="play" size={13} />
            Dispatch
          </button>

          <button
            type="button"
            className="gb-btn gb-btn--primary"
            onClick={() => (mode === "kanban" ? openNewTask("todo") : void createNewSession())}
          >
            <CursorIcon name="plus" size={13} />
            {mode === "kanban" ? "New Task" : "New Session"}
          </button>
        </div>
      </div>

      {/* Board Switcher Sub-bar */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          gap: 10,
          padding: "10px 20px",
          borderBottom: "0.5px solid var(--grok-bot-border-default)",
          background: "var(--grok-bot-bg-chrome)",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 6, overflowX: "auto" }}>
          <button
            type="button"
            className="gb-board-pill"
            data-active={activeBoardSlug === "all" ? "true" : undefined}
            onClick={() => setActiveBoardSlug("all")}
          >
            All Boards
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
              >
                <span className="gb-board-pill__dot" style={{ background: preset.hex }} />
                {b.name}
              </button>
            );
          })}
          <button type="button" className="gb-board-pill" onClick={() => setBoardModalOpen(true)}>
            <CursorIcon name="plus" size={11} />
            New Board
          </button>
        </div>

        {activeBoard && activeBoard.slug !== "default" && (
          <button
            type="button"
            className="gb-btn gb-btn--danger gb-btn--sm"
            onClick={() => void deleteBoard(activeBoard.slug)}
          >
            <CursorIcon name="trash" size={12} />
            Delete Board
          </button>
        )}
      </div>

      {/* Body: Kanban Board OR Board Sessions Grid */}
      {mode === "kanban" ? (
        <div className="gb-kanban-wrap">
          {COLUMNS.map((col) => {
            const colTasks = visibleTasks.filter((t) => t.status === col.id);
            return (
              <section
                key={col.id}
                className="gb-kanban-col"
                data-drop-over={dropCol === col.id ? "true" : undefined}
                onDragOver={(e) => {
                  e.preventDefault();
                  setDropCol(col.id);
                }}
                onDragLeave={() => setDropCol((prev) => (prev === col.id ? null : prev))}
                onDrop={(e) => {
                  e.preventDefault();
                  setDropCol(null);
                  if (dragTaskId) {
                    void updateTask(dragTaskId, { status: col.id });
                    setDragTaskId(null);
                  }
                }}
              >
                <div className="gb-kanban-col__head">
                  <span className="gb-kanban-col__title">
                    <span className="gb-board-pill__dot" style={{ background: col.dot }} />
                    {col.label}
                    <span className="gb-badge gb-badge--muted">{colTasks.length}</span>
                  </span>
                  <button
                    type="button"
                    className="gb-mini-icon-btn"
                    title={`Add task to ${col.label}`}
                    onClick={() => openNewTask(col.id)}
                  >
                    <CursorIcon name="plus" size={13} />
                  </button>
                </div>

                <div className="gb-kanban-col__body">
                  {colTasks.map((task) => {
                    const seed = task.assignee || activeProfile;
                    const av = botAvatars[seed];
                    return (
                      <div
                        key={task.id}
                        className="gb-task-card"
                        draggable
                        onDragStart={() => setDragTaskId(task.id)}
                        onDragEnd={() => {
                          setDragTaskId(null);
                          setDropCol(null);
                        }}
                        onClick={() => openEditTask(task)}
                      >
                        <div className="gb-task-card__title">{task.title}</div>
                        {(task.body || task.description) && (
                          <div className="gb-task-card__desc">{task.body || task.description}</div>
                        )}
                        <div className="gb-task-card__footer">
                          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                            <BotMark seed={seed} shapeId={av?.shapeId} colorId={av?.colorId} size={18} />
                            <span className={`gb-badge ${priorityBadgeClass(task.priority)}`}>
                              {String(priorityNumber(task.priority))}
                            </span>
                            {task.model && (
                              <span className="gb-badge gb-badge--muted">{shortModelName(task.model)}</span>
                            )}
                          </div>

                          <button
                            type="button"
                            className="gb-btn gb-btn--secondary gb-btn--sm"
                            onClick={(e) => {
                              e.stopPropagation();
                              void launchSessionForTask(task);
                            }}
                            title="Open or start a Hermes chat session for this task"
                          >
                            <CursorIcon name="chat-bubble" size={11} />
                            {task.session_id ? "Open" : "Run"}
                          </button>
                        </div>
                      </div>
                    );
                  })}

                  {colTasks.length === 0 && (
                    <div
                      style={{
                        padding: "18px 10px",
                        textAlign: "center",
                        fontSize: 12,
                        color: "var(--grok-bot-text-tertiary)",
                        border: "1px dashed var(--grok-bot-border-default)",
                        borderRadius: 12,
                      }}
                    >
                      Drop a task here
                    </div>
                  )}
                </div>
              </section>
            );
          })}
        </div>
      ) : (
        <div className="gb-panel__body">
          <div className="gb-panel__container">
            {visibleSessions.length === 0 ? (
              <div className="gb-empty">
                <CursorIcon name="chat-bubble-plus" size={28} />
                <div className="gb-empty__title">No sessions in this board yet</div>
                <div className="gb-empty__sub">
                  Start a new chat session scoped to this board and workspace folder.
                </div>
                <button type="button" className="gb-btn gb-btn--primary" onClick={() => void createNewSession()}>
                  New Session
                </button>
              </div>
            ) : (
              <div
                style={{
                  display: "grid",
                  gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))",
                  gap: 12,
                }}
              >
                {visibleSessions.map((s) => {
                  const seed = s.profile || s.id;
                  const av = botAvatars[seed];
                  return (
                    <button
                      key={s.id}
                      type="button"
                      className="gb-prompt-card"
                      style={{ padding: 14, gap: 8 }}
                      onClick={() => void selectSession(s.id)}
                    >
                      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                        <BotMark seed={seed} shapeId={av?.shapeId} colorId={av?.colorId} size={30} />
                        <div style={{ minWidth: 0, flex: 1 }}>
                          <div
                            style={{
                              fontSize: 13.5,
                              fontWeight: 600,
                              overflow: "hidden",
                              textOverflow: "ellipsis",
                              whiteSpace: "nowrap",
                            }}
                          >
                            {s.title || "Untitled Chat"}
                          </div>
                          <div style={{ fontSize: 11.5, color: "var(--grok-bot-text-tertiary)" }}>
                            {formatRelativeTime(s.updated_at ?? s.started_at)} · {s.message_count ?? 0} msgs
                          </div>
                        </div>
                      </div>
                      <div className="gb-task-card__desc">{s.preview || "Open conversation"}</div>
                      <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
                        {s.model && <span className="gb-badge gb-badge--muted">{shortModelName(s.model)}</span>}
                        {s.source && <span className="gb-badge gb-badge--info">{s.source}</span>}
                      </div>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Task Editor Modal */}
      <Modal
        open={taskModalOpen}
        title={editingTask ? "Edit Board Task" : "New Board Task"}
        onClose={() => setTaskModalOpen(false)}
      >
        <form onSubmit={handleSaveTask} style={{ display: "flex", flexDirection: "column", gap: 13 }}>
          <div className="gb-field">
            <label className="gb-field__label">Task Title</label>
            <input
              className="gb-input"
              placeholder="What should Hermes build or investigate?"
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              autoFocus
              required
            />
          </div>

          <div className="gb-field">
            <label className="gb-field__label">Specification / Prompt Details</label>
            <textarea
              className="gb-textarea"
              placeholder="Detailed context, acceptance criteria, or instructions..."
              value={body}
              onChange={(e) => setBody(e.target.value)}
            />
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
            <div className="gb-field">
              <label className="gb-field__label">Column</label>
              <select
                className="gb-select"
                value={status}
                onChange={(e) => setStatus(e.target.value as HermesKanbanStatus)}
              >
                {COLUMNS.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.label}
                  </option>
                ))}
              </select>
            </div>
            <div className="gb-field">
              <label className="gb-field__label">Priority</label>
              <input type="number" step="1" className="gb-input" value={priority} onChange={event => setPriority(event.target.value)} />
            </div>
          </div>

          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 10 }}>
            <div className="gb-field">
              <label className="gb-field__label">Assigned Hermes Profile</label>
              <select className="gb-select" value={assignee} onChange={(e) => setAssignee(e.target.value)}>
                {profiles.map((p) => (
                  <option key={p.name} value={p.name}>
                    {p.name}
                  </option>
                ))}
              </select>
            </div>
            <div className="gb-field">
              <label className="gb-field__label">Model Override</label>
              <select className="gb-select" value={JSON.stringify([provider, model])} onChange={event => {
                const [nextProvider, nextModel] = JSON.parse(event.target.value) as [string, string];
                setProvider(nextProvider); setModel(nextModel);
              }}>
                <option value={JSON.stringify([provider, model])}>{shortModelName(model)} · {provider} (selected)</option>
                {models.filter(option => option.id !== model || option.provider !== provider).map(option => (
                  <option key={`${option.provider}:${option.id}`} value={JSON.stringify([option.provider, option.id])}>
                    {option.name || option.id} · {option.provider}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className="gb-field">
            <label className="gb-field__label">Workspace Directory (Optional)</label>
            <input
              className="gb-input gb-input--mono"
              placeholder="/home/metis/Projects/..."
              value={cwd}
              onChange={(e) => setCwd(e.target.value)}
            />
          </div>

          <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginTop: 4 }}>
            {editingTask ? (
              <button
                type="button"
                className="gb-btn gb-btn--danger"
                onClick={() => {
                  void deleteTask(editingTask.id);
                  setTaskModalOpen(false);
                }}
              >
                Delete
              </button>
            ) : (
              <span />
            )}

            <div style={{ display: "flex", gap: 8 }}>
              <button type="button" className="gb-btn gb-btn--ghost" onClick={() => setTaskModalOpen(false)}>
                Cancel
              </button>
              <button type="submit" className="gb-btn gb-btn--primary" disabled={!title.trim()}>
                {editingTask ? "Save Task" : "Create Task"}
              </button>
            </div>
          </div>
        </form>
      </Modal>

      {/* New Board Modal */}
      <Modal open={boardModalOpen} title="Create Session Board" onClose={() => setBoardModalOpen(false)}>
        <form onSubmit={handleCreateBoard} style={{ display: "flex", flexDirection: "column", gap: 13 }}>
          <div className="gb-field">
            <label className="gb-field__label">Board Name</label>
            <input
              className="gb-input"
              placeholder="e.g. Frontend Sprint, Research Lab"
              value={newBoardName}
              onChange={(e) => setNewBoardName(e.target.value)}
              autoFocus
              required
            />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Description</label>
            <input
              className="gb-input"
              value={newBoardDesc}
              onChange={(e) => setNewBoardDesc(e.target.value)}
            />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Workspace Folder (Optional)</label>
            <input
              className="gb-input gb-input--mono"
              placeholder="/home/metis/Projects/..."
              value={newBoardPath}
              onChange={(e) => setNewBoardPath(e.target.value)}
            />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Accent Color</label>
            <div className="gb-color-grid">
              {["blue", "violet", "cyan", "green", "orange", "red", "magenta", "yellow"].map((c) => (
                <button
                  key={c}
                  type="button"
                  className="gb-color-dot"
                  data-active={newBoardColor === c ? "true" : undefined}
                  style={{ background: resolveBotColor(c, c).hex }}
                  onClick={() => setNewBoardColor(c)}
                />
              ))}
            </div>
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button type="button" className="gb-btn gb-btn--ghost" onClick={() => setBoardModalOpen(false)}>
              Cancel
            </button>
            <button type="submit" className="gb-btn gb-btn--primary" disabled={!newBoardName.trim()}>
              Create Board
            </button>
          </div>
        </form>
      </Modal>
    </div>
  );
};
