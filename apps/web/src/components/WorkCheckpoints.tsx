import React, { useEffect, useState } from "react";
import { Modal } from "@grok-bot/theme";
import { useApp, useChatSlice } from "../state/appStore";
import type { WorkCheckpointResults } from "../state/workSessions";

export function WorkCheckpoints({ onClose }: { onClose: () => void }) {
  const { activeSession, showToast, workCheckpoint } = useApp();
  const isStreaming = useChatSlice(chat => chat.isStreaming);
  const [list, setList] = useState<WorkCheckpointResults["list"]>();
  const [hash, setHash] = useState("");
  const [preview, setPreview] = useState<WorkCheckpointResults["diff"]>();
  const [error, setError] = useState("");
  const [confirming, setConfirming] = useState(false);
  const [restoring, setRestoring] = useState(false);

  useEffect(() => {
    let current = true;
    workCheckpoint("list").then(data => {
      if (current) setList(data);
    }, reason => {
      if (current) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { current = false; };
  }, [workCheckpoint]);

  useEffect(() => {
    if (!hash) return;
    let current = true;
    workCheckpoint("diff", hash).then(data => {
      if (current) setPreview(data);
    }, reason => {
      if (current) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { current = false; };
  }, [hash, workCheckpoint]);

  const restore = async () => {
    if (restoring || isStreaming || !hash || !preview) return;
    if (!confirming) { setConfirming(true); return; }
    setRestoring(true);
    setError("");
    try {
      await workCheckpoint("restore", hash);
      showToast("Checkpoint restored");
      onClose();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setRestoring(false);
    }
  };

  return (
    <Modal
      open
      title="Work checkpoints"
      onClose={() => { if (!restoring) onClose(); }}
      maxWidth={640}
      footer={
        <>
          <button className="gb-btn gb-btn--secondary" type="button" disabled={restoring} onClick={() => {
            if (confirming) setConfirming(false);
            else onClose();
          }}>{confirming ? "Back" : "Close"}</button>
          {list?.enabled && list.checkpoints.length > 0 && (
            <button className="gb-btn gb-btn--danger" type="button" disabled={!preview || isStreaming || restoring}
              onClick={() => void restore()}>
              {restoring ? "Restoring…" : confirming ? "Restore files and history" : "Restore checkpoint"}
            </button>
          )}
        </>
      }
    >
      <p className="gb-work-checkpoints__path">{activeSession?.cwd}</p>
      {error && <p role="alert">{error}</p>}
      {!list && !error && <p role="status">Loading checkpoints…</p>}
      {list && !list.enabled && <p>Checkpoints are disabled in this Hermes profile.</p>}
      {list?.enabled && list.checkpoints.length === 0 && <p>No checkpoints for this workspace yet.</p>}
      {list?.enabled && list.checkpoints.length > 0 && (
        <div className="gb-work-checkpoints">
          <label>
            Checkpoint
            <select className="gb-input" value={hash} disabled={restoring} onChange={event => {
              setHash(event.target.value);
              setError("");
              setPreview(undefined);
              setConfirming(false);
            }}>
              <option value="">Select a checkpoint</option>
              {list.checkpoints.map(item => (
                <option key={item.hash} value={item.hash}>{item.message || "Checkpoint"} · {item.hash.slice(0, 8)}</option>
              ))}
            </select>
          </label>
          {hash && !preview && !error && <p role="status">Loading checkpoint diff…</p>}
          {preview && <pre className="gb-term-output">{preview.diff || preview.stat || "No file changes."}</pre>}
          {isStreaming && <p>Stop the current response before restoring a checkpoint.</p>}
          {confirming && <p role="status">Restoring can overwrite or remove workspace files and rewind conversation history.</p>}
        </div>
      )}
    </Modal>
  );
}
