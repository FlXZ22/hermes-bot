import React, { useCallback, useEffect, useRef, useState } from "react";
import { BotMark, CursorIcon } from "@grok-bot/theme";
import { useApp, type RightPaneTabId } from "../state/appStore";
import { tauriBridge, type GitWorkspaceStatus, type WorkspaceFileEntry } from "../lib/tauriBridge";
import { shortModelName } from "../lib/format";

const TerminalWorkspace = React.lazy(() => import("./TerminalWorkspace").then(module => ({ default: module.TerminalWorkspace })));

export const RightSidePane: React.FC = () => {
  const {
    isDesktop: nativeDesktop,
    isLocalConnection,
    rightPaneOpen,
    activeView,
    setupOpen,
    setRightPaneOpen,
    rightPaneTab,
    setRightPaneTab,
    rightPaneWidth,
    setRightPaneWidth,
    activeProfile,
    activeSession,
    activeModel,
    activeProvider,
    botAvatars,
    setAvatarEditorTarget,
    soulMd,
    saveMemoryFile,
    systemStats,
    bootstrapMs,
    rustBackend,
    client,
    connection,
  } = useApp();

  const [terminalMounted, setTerminalMounted] = useState(false);
  useEffect(() => { if (rightPaneOpen && rightPaneTab === "terminal" && activeView === "chat" && !setupOpen) setTerminalMounted(true); }, [rightPaneOpen, rightPaneTab, activeView, setupOpen]);

  const isDesktop = nativeDesktop && isLocalConnection;
  const defaultDir = activeSession?.cwd || systemStats?.home_dir || (isLocalConnection ? "/home" : "");
  const [cwd, setCwd] = useState(defaultDir);
  const [files, setFiles] = useState<WorkspaceFileEntry[]>([]);
  const [fileError, setFileError] = useState<string | null>(null);
  const [previewPath, setPreviewPath] = useState<string | null>(null);
  const [previewContent, setPreviewContent] = useState<string>("");

  // Git state
  const [gitStatus, setGitStatus] = useState<GitWorkspaceStatus | null>(null);
  const [gitLoading, setGitLoading] = useState(false);
  const [gitError, setGitError] = useState("");
  const canReadGit = nativeDesktop && (isLocalConnection || !!connection.ssh);

  // Bot SOUL draft
  const [soulDraft, setSoulDraft] = useState(soulMd);
  useEffect(() => {
    setSoulDraft(soulMd);
  }, [soulMd]);

  useEffect(() => {
    if (activeSession?.cwd) {
      setCwd(activeSession.cwd);
    } else if (systemStats?.home_dir && cwd === "/home") {
      setCwd(systemStats.home_dir);
    }
  }, [activeSession?.cwd, cwd, systemStats?.home_dir]);

  const loadFiles = useCallback(
    async (targetDir: string) => {
      setFileError(null);
      try {
        if (isDesktop) {
          const list = await tauriBridge.listWorkspaceFiles(targetDir);
          setFiles(list);
        } else if (client) {
          const list = await client.listFiles(targetDir);
          setFiles(
            list.map((e) => ({
              name: e.name,
              path: e.path,
              is_dir: e.is_dir,
              size: e.size ?? 0,
              modified: Number(e.modified ?? 0),
            }))
          );
        }
      } catch (err) {
        setFileError(err instanceof Error ? err.message : String(err));
      }
    },
    [client, isDesktop]
  );

  const loadGit = useCallback(async () => {
    if (!canReadGit) return;
    setGitLoading(true); setGitError("");
    try {
      const st = await tauriBridge.gitWorkspaceStatus(cwd, isLocalConnection ? undefined : connection.ssh);
      setGitStatus(st);
    } catch (error) { setGitError(String(error)); setGitStatus(null);
    } finally {
      setGitLoading(false);
    }
  }, [cwd, canReadGit, isLocalConnection, connection.ssh]);

  useEffect(() => {
    if (!rightPaneOpen || activeView !== "chat" || setupOpen) return;
    if (rightPaneTab === "files") void loadFiles(cwd);
    if (rightPaneTab === "git") void loadGit();
  }, [cwd, loadFiles, loadGit, rightPaneOpen, rightPaneTab, activeView, setupOpen]);

  // Drag resize handle
  const dragRef = useRef<{ startX: number; startWidth: number } | null>(null);
  const startResize = (e: React.MouseEvent) => {
    e.preventDefault();
    dragRef.current = { startX: e.clientX, startWidth: rightPaneWidth };
    const onMove = (ev: MouseEvent) => {
      if (!dragRef.current) return;
      const delta = dragRef.current.startX - ev.clientX;
      setRightPaneWidth(dragRef.current.startWidth + delta);
    };
    const onUp = () => {
      dragRef.current = null;
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };


  const openFile = async (entry: WorkspaceFileEntry) => {
    if (entry.is_dir) {
      setCwd(entry.path);
      setPreviewPath(null);
      return;
    }
    setPreviewPath(entry.path);
    try {
      const text = isDesktop
        ? await tauriBridge.readWorkspaceFile(entry.path)
        : client
          ? await client.readFile(entry.path)
          : "";
      setPreviewContent(text);
    } catch (err) {
      setPreviewContent(err instanceof Error ? err.message : "Unable to read file");
    }
  };

  const goParentDir = () => {
    const clean = cwd.replace(/\/+$/, "");
    const idx = clean.lastIndexOf("/");
    if (idx > 0) setCwd(clean.slice(0, idx));
    else setCwd("/");
  };

  const botSeed = activeSession?.profile || activeProfile || "default";
  const botAv = botAvatars[botSeed];

  return (
    <aside className="gb-side-pane" style={{ width: rightPaneWidth, display: rightPaneOpen && activeView === "chat" && !setupOpen ? undefined : "none" }}>
      <div className="gb-side-pane__resize" onMouseDown={startResize} />

      <div className="gb-side-pane__header">
        <div className="gb-segmented">
          {(
            [
              { id: "files", label: "Files" },
              { id: "terminal", label: "Term" },
              { id: "git", label: "Git" },
              { id: "bot", label: "Bot" },
            ] as Array<{ id: RightPaneTabId; label: string }>
          ).map((t) => (
            <button
              key={t.id}
              type="button"
              className="gb-segmented__item"
              data-active={rightPaneTab === t.id ? "true" : undefined}
              onClick={() => setRightPaneTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </div>

        <button
          type="button"
          className="gb-footer-action"
          title="Close Inspector"
          onClick={() => setRightPaneOpen(false)}
        >
          <CursorIcon name="close" size={14} />
        </button>
      </div>

      <div className="gb-side-pane__body">
        {/* FILES TAB */}
        {rightPaneTab === "files" && (
          <>
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <button type="button" className="gb-btn gb-btn--secondary gb-btn--sm" onClick={goParentDir} title="Parent directory">
                <CursorIcon name="arrow-up" size={12} />
              </button>
              <input
                className="gb-input gb-input--mono"
                style={{ padding: "4px 8px", fontSize: 12 }}
                value={cwd}
                onChange={(e) => setCwd(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void loadFiles(cwd);
                }}
              />
              <button
                type="button"
                className="gb-btn gb-btn--secondary gb-btn--sm"
                onClick={() => void loadFiles(cwd)}
                title="Refresh"
              >
                <CursorIcon name="refresh" size={12} />
              </button>
            </div>

            {fileError && <div className="gb-badge gb-badge--danger">{fileError}</div>}

            {previewPath ? (
              <div style={{ display: "flex", flexDirection: "column", gap: 8, flex: 1, minHeight: 0 }}>
                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
                  <span style={{ fontSize: 12, fontFamily: "var(--cursor-font-family-mono)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {previewPath.split("/").pop()}
                  </span>
                  <button
                    type="button"
                    className="gb-btn gb-btn--ghost gb-btn--sm"
                    onClick={() => setPreviewPath(null)}
                  >
                    Back
                  </button>
                </div>
                <pre className="gb-term-output" style={{ margin: 0 }}>
                  {previewContent}
                </pre>
              </div>
            ) : (
              <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                {files.slice(0, 120).map((f) => (
                  <button
                    key={f.path}
                    type="button"
                    className="gb-file-row"
                    onClick={() => void openFile(f)}
                  >
                    <CursorIcon
                      name={f.is_dir ? "folder" : "file-code"}
                      size={14}
                      style={{ color: f.is_dir ? "var(--grok-bot-current-agent-accent)" : "var(--grok-bot-text-tertiary)" }}
                    />
                    <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {f.name}
                    </span>
                    {!f.is_dir && (
                      <span style={{ fontSize: 11, color: "var(--grok-bot-text-tertiary)" }}>
                        {f.size < 1024 ? `${f.size}B` : `${(f.size / 1024).toFixed(1)}K`}
                      </span>
                    )}
                  </button>
                ))}
              </div>
            )}
          </>
        )}

        <div style={{ display: rightPaneTab === "terminal" ? "flex" : "none", flex: 1, minHeight: 0 }}>
          {terminalMounted && <React.Suspense fallback={<span style={{ fontSize: 12, color: "var(--grok-bot-text-secondary)" }}>Opening terminal…</span>}><TerminalWorkspace cwd={cwd} visible={rightPaneOpen && activeView === "chat" && rightPaneTab === "terminal" && !setupOpen} /></React.Suspense>}
        </div>

        {/* GIT TAB */}
        {rightPaneTab === "git" && (
          <>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
              <span className="gb-badge gb-badge--accent">
                <CursorIcon name="git-branch" size={12} />
                {!canReadGit ? "Add SSH access to read server Git status" : gitStatus?.is_repo ? gitStatus.branch : "Not a git repo"}
              </span>
              <button type="button" className="gb-btn gb-btn--secondary gb-btn--sm" disabled={!canReadGit || gitLoading} onClick={() => void loadGit()}>
                <CursorIcon name="refresh" size={12} spin={gitLoading} />
                Refresh
              </button>
            </div>

            {gitError && <div className="gb-terminal-error" role="alert">{gitError}</div>}
            {gitStatus?.is_repo && (
              <>
                <div className="gb-section-card">
                  <div className="gb-section-card__head" style={{ padding: "9px 12px" }}>
                    <span className="gb-section-card__title" style={{ fontSize: 12.5 }}>
                      Changed Files ({gitStatus.changed_files.length})
                    </span>
                  </div>
                  <div style={{ padding: "8px 10px", display: "flex", flexDirection: "column", gap: 4 }}>
                    {gitStatus.changed_files.length === 0 ? (
                      <span style={{ fontSize: 12, color: "var(--grok-bot-text-tertiary)" }}>Working tree clean</span>
                    ) : (
                      gitStatus.changed_files.map((f, idx) => (
                        <div key={idx} style={{ display: "flex", gap: 8, fontSize: 12, fontFamily: "var(--cursor-font-family-mono)" }}>
                          <span className="gb-badge gb-badge--warning">{f.status || "M"}</span>
                          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{f.path}</span>
                        </div>
                      ))
                    )}
                  </div>
                </div>

                {gitStatus.worktrees.length > 0 && (
                  <div className="gb-section-card">
                    <div className="gb-section-card__head" style={{ padding: "9px 12px" }}>
                      <span className="gb-section-card__title" style={{ fontSize: 12.5 }}>
                        Worktrees ({gitStatus.worktrees.length})
                      </span>
                    </div>
                    <div style={{ padding: "8px 10px", fontSize: 11.5, fontFamily: "var(--cursor-font-family-mono)", color: "var(--grok-bot-text-secondary)" }}>
                      {gitStatus.worktrees.map((w, i) => (
                        <div key={i} style={{ marginBottom: 4 }}>
                          {w}
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {gitStatus.diff && (
                  <pre className="gb-term-output" style={{ margin: 0, maxHeight: 300 }}>
                    {gitStatus.diff}
                  </pre>
                )}
              </>
            )}
          </>
        )}

        {/* BOT / SYSTEM INSPECTOR TAB */}
        {rightPaneTab === "bot" && (
          <>
            <div className="gb-section-card">
              <div className="gb-section-card__body" style={{ display: "flex", alignItems: "center", gap: 12 }}>
                <BotMark seed={botSeed} shapeId={botAv?.shapeId} colorId={botAv?.colorId} size={46} />
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: 14, fontWeight: 600 }}>
                    {botAv?.displayName || (activeProfile === "default" ? "Hermes" : activeProfile)}
                  </div>
                  <div style={{ fontSize: 12, color: "var(--grok-bot-text-tertiary)" }}>
                    {shortModelName(activeModel)} · {activeProvider}
                  </div>
                </div>
                <button
                  type="button"
                  className="gb-btn gb-btn--secondary gb-btn--sm"
                  onClick={() => setAvatarEditorTarget({ seed: botSeed, title: botAv?.displayName || botSeed })}
                >
                  Edit
                </button>
              </div>
            </div>

            <div className="gb-section-card">
              <div className="gb-section-card__head" style={{ padding: "9px 12px" }}>
                <span className="gb-section-card__title" style={{ fontSize: 12.5 }}>
                  SOUL.md Persona
                </span>
                <button
                  type="button"
                  className="gb-btn gb-btn--primary gb-btn--sm"
                  onClick={() => void saveMemoryFile("soul", soulDraft)}
                >
                  Save
                </button>
              </div>
              <div className="gb-section-card__body" style={{ padding: 10 }}>
                <textarea
                  className="gb-textarea gb-textarea--mono"
                  style={{ minHeight: 150 }}
                  value={soulDraft}
                  onChange={(e) => setSoulDraft(e.target.value)}
                  placeholder="Define Hermes personality & tone in SOUL.md..."
                />
              </div>
            </div>

            {systemStats && (
              <div className="gb-section-card">
                <div className="gb-section-card__head" style={{ padding: "9px 12px" }}>
                  <span className="gb-section-card__title" style={{ fontSize: 12.5 }}>
                    Rust Native Runtime
                  </span>
                  {bootstrapMs !== null && <span className="gb-badge gb-badge--success">{bootstrapMs}ms boot</span>}
                </div>
                <div className="gb-row">
                  <span className="gb-row__desc">Host</span>
                  <span style={{ fontSize: 12.5, fontWeight: 500 }}>{systemStats.hostname}</span>
                </div>
                <div className="gb-row">
                  <span className="gb-row__desc">OS</span>
                  <span style={{ fontSize: 12.5, fontWeight: 500 }}>{systemStats.os_name}</span>
                </div>
                <div className="gb-row">
                  <span className="gb-row__desc">Memory</span>
                  <span style={{ fontSize: 12.5, fontWeight: 500 }}>
                    {systemStats.memory_used_mb} / {systemStats.memory_total_mb} MB ({systemStats.memory_percent}%)
                  </span>
                </div>
                {rustBackend?.pid && (
                  <div className="gb-row">
                    <span className="gb-row__desc">Hermes PID</span>
                    <span style={{ fontSize: 12.5, fontFamily: "var(--cursor-font-family-mono)" }}>
                      {rustBackend.pid}
                    </span>
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </aside>
  );
};
