import React, { useState } from "react";
import type { GitWorkspaceAction } from "../lib/tauriBridge";

type WorkGitAction = Extract<GitWorkspaceAction, { type: "commit" | "push" | "switchBranch" }>;

export function WorkGitActions({ busy, staged, remoteChanges, detached, run, createWorktree, openWorktree }: {
  busy: boolean;
  staged: boolean;
  remoteChanges?: boolean;
  detached?: boolean;
  run: (action: WorkGitAction) => Promise<boolean>;
  createWorktree: (branch: string, path: string) => Promise<string | null>;
  openWorktree: (path: string) => Promise<void>;
}) {
  const [message, setMessage] = useState("");
  const [branch, setBranch] = useState("");
  const [switchTo, setSwitchTo] = useState("");
  const [path, setPath] = useState("");
  const [created, setCreated] = useState("");
  const hosted = remoteChanges !== undefined;

  return (
    <div className="gb-work-git-actions">
      <form onSubmit={async event => {
        event.preventDefault();
        if (await run({ type: "commit", message: message.trim() })) setMessage("");
      }}>
        <label>
          Commit message
          <input className="gb-input" value={message} onChange={event => setMessage(event.target.value)} required />
        </label>
        <div className="gb-work-git-actions__buttons">
          <button className="gb-btn gb-btn--secondary gb-btn--sm" type="submit" disabled={!(staged || remoteChanges) || !message.trim() || busy}
            title={hosted ? "Hermes commits staged files or stages all changes when the index is empty." : undefined}>
            {hosted ? "Commit changes" : "Commit staged"}
          </button>
          <button className="gb-btn gb-btn--secondary gb-btn--sm" type="button" disabled={busy || detached} onClick={() => void run({ type: "push" })}>Push</button>
        </div>
      </form>
      <details>
        <summary>Switch branch</summary>
        <form onSubmit={async event => {
          event.preventDefault();
          if (await run({ type: "switchBranch", branch: switchTo.trim() })) setSwitchTo("");
        }}>
          <label>
            Existing branch name
            <input className="gb-input" value={switchTo} onChange={event => setSwitchTo(event.target.value)} required />
          </label>
          <button className="gb-btn gb-btn--secondary gb-btn--sm" type="submit" disabled={busy || !switchTo.trim()}>Switch branch</button>
        </form>
      </details>
      <details>
        <summary>New worktree</summary>
        <form onSubmit={async event => {
          event.preventDefault();
          const worktree = await createWorktree(branch.trim(), path.trim());
          if (worktree) {
            setCreated(worktree);
            setPath("");
            setBranch("");
          }
        }}>
          <label>
            Branch name
            <input className="gb-input" value={branch} onChange={event => setBranch(event.target.value)} required />
          </label>
          {hosted ? <p>Hermes creates the folder under the repository’s .worktrees directory.</p> : <label>
            Worktree path
            <input className="gb-input" value={path} onChange={event => setPath(event.target.value)} placeholder="/path/to/worktree" required />
          </label>}
          <button className="gb-btn gb-btn--secondary gb-btn--sm" type="submit" disabled={busy || !branch.trim() || (!hosted && !path.trim())}>
            Create worktree
          </button>
        </form>
        {created && <button className="gb-btn gb-btn--secondary gb-btn--sm" type="button" disabled={busy}
          onClick={() => void openWorktree(created)}>Open new worktree in Work</button>}
      </details>
    </div>
  );
}
