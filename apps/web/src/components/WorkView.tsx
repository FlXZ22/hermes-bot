import React, { useId, useMemo, useRef, useState } from "react";
import { CursorIcon } from "@grok-bot/theme";
import { readWorkImport, type WorkImport } from "../lib/workImport";
import { useApp } from "../state/appStore";
import { ChatView } from "./ChatView";
import "./work.css";

export function WorkView() {
  const { activeProfile, activeSession, boards, connection, connectionStatus, createNewSession, importWorkSession, sessions, workSessionIds } = useApp();
  const [path, setPath] = useState("");
  const [opening, setOpening] = useState(false);
  const pending = useRef(false);
  const [error, setError] = useState("");
  const [preview, setPreview] = useState<WorkImport | null>(null);
  const [importState, setImportState] = useState<"reading" | "saving" | null>(null);
  const [importError, setImportError] = useState("");
  const pathLabel = useId();
  const fileLabel = useId();
  const projectPaths = useMemo(() => [...new Set([
    ...sessions.filter(session => workSessionIds.has(session.id)).map(session => session.cwd),
    ...boards.map(board => board.workspace_path),
  ].filter((dir): dir is string => Boolean(dir?.startsWith("/"))))].slice(0, 6), [boards, sessions, workSessionIds]);
  const connected = connectionStatus === "connected" && connection.mode !== "openai";

  const openWorkspace = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!connected || pending.current) return;
    pending.current = true;
    setOpening(true);
    setError("");
    try {
      await createNewSession({ section: "work", cwd: path });
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not open workspace.");
    } finally {
      pending.current = false;
      setOpening(false);
    }
  };

  const previewImport = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.currentTarget.files?.[0];
    event.currentTarget.value = "";
    if (!file || pending.current) return;
    pending.current = true;
    setPreview(null);
    setImportError("");
    setImportState("reading");
    try {
      setPreview(await readWorkImport(file));
    } catch (err) {
      setImportError(err instanceof Error ? err.message : "Could not read the session export.");
    } finally {
      pending.current = false;
      setImportState(null);
    }
  };

  const saveImport = async () => {
    if (!preview || !connected || !path.trim() || pending.current) return;
    pending.current = true;
    setImportState("saving");
    setImportError("");
    try {
      await importWorkSession(preview, path);
      setPreview(null);
    } catch (err) {
      setImportError(err instanceof Error ? err.message : "Could not import the session.");
    } finally {
      pending.current = false;
      setImportState(null);
    }
  };

  const busy = opening || importState !== null;
  if (activeSession && workSessionIds.has(activeSession.id)) return <ChatView work />;

  return (
    <main className="gb-chat gb-work">
      <div className="gb-work-setup">
        <div className="gb-work-setup__icon"><CursorIcon name="code" size={26} /></div>
        <h1>Work</h1>
        <p>Choose the project folder where {activeProfile === "default" ? "Hermes" : activeProfile} will work.</p>
        <form onSubmit={event => void openWorkspace(event)}>
          <label htmlFor={pathLabel}>Workspace path</label>
          <div className="gb-work-setup__input">
            <input
              id={pathLabel}
              className="gb-input"
              placeholder="/path/to/project"
              value={path}
              autoComplete="off"
              spellCheck={false}
              onChange={event => { setPath(event.target.value); setError(""); }}
              disabled={busy}
              required
            />
            <button className="gb-btn gb-btn--primary" disabled={!connected || !path.trim() || busy} type="submit">
              {opening ? "Opening…" : "Open workspace"}
            </button>
          </div>
          {!connected && <p role="status">Connect to a Hermes gateway to open a workspace.</p>}
          {error && <p className="gb-work-setup__error" role="alert">{error}</p>}
        </form>
        {projectPaths.length > 0 && (
          <div className="gb-work-recents">
            <h2>Project folders</h2>
            {projectPaths.map(dir => (
              <button className="gb-work-recents__row" key={dir} type="button" disabled={busy} onClick={() => { setPath(dir); setError(""); }}>
                <CursorIcon name="folder" size={16} />
                <span>{dir}</span>
              </button>
            ))}
          </div>
        )}
        <div className="gb-work-import">
          <label htmlFor={fileLabel}>Session export</label>
          <input id={fileLabel} className="gb-input" type="file" accept=".json,application/json" disabled={busy}
            onChange={event => void previewImport(event)} />
          {importState === "reading" && <p role="status">Reading session…</p>}
          {importError && <p className="gb-work-setup__error" role="alert">{importError}</p>}
          {preview && <>
            <p>{preview.title} · {preview.format} · {preview.messages.length} messages</p>
            <p>Imports conversation text into the workspace path above.</p>
            <button className="gb-btn gb-btn--primary" type="button" disabled={!connected || !path.trim() || busy}
              onClick={() => void saveImport()}>{importState === "saving" ? "Importing…" : "Import session"}</button>
          </>}
        </div>
      </div>
    </main>
  );
}
