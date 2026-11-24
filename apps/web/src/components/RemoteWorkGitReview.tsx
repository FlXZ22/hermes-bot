import React, { useEffect, useRef, useState } from "react";
import type { HermesClient, HermesGitStatus } from "@grok-bot/hermes-client";
import { resolveWorkGitRoot } from "../lib/workGit";
import { useApp, useChatSlice } from "../state/appStore";
import { WorkGitActions } from "./WorkGitActions";
import { WorkPullRequest } from "./WorkPullRequest";

export function RemoteWorkGitReview({ client, cwd }: { client: HermesClient; cwd: string }) {
  const { createNewSession, showToast } = useApp();
  const turn = useChatSlice(chat => chat.lastTurnSettlement);
  const [repo, setRepo] = useState<{ root: string; status: HermesGitStatus | null }>();
  const [revision, setRevision] = useState(0);
  const [file, setFile] = useState("");
  const [mode, setMode] = useState<"unstaged" | "staged" | "uncommitted">("uncommitted");
  const [diff, setDiff] = useState<string>();
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const root = repo?.root;

  useEffect(() => {
    let mounted = true;
    setError("");
    const load = async () => {
      const root = await resolveWorkGitRoot(client, cwd);
      const status = await client.getGitStatus(root);
      if (mounted) setRepo({ root, status });
    };
    void load().catch(reason => {
      if (mounted) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { mounted = false; };
  }, [client, cwd, revision, turn]);

  useEffect(() => {
    if (!root || !file) return;
    let mounted = true;
    setDiff(undefined);
    void client.getGitFileDiff(root, file, mode).then(data => {
      if (mounted) setDiff(data.diff);
    }, reason => {
      if (mounted) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { mounted = false; };
  }, [client, root, file, mode, revision, turn]);

  const mutate = async <T,>(action: (path: string) => Promise<T>): Promise<T | null> => {
    if (pending.current || !root) return null;
    pending.current = true;
    setBusy(true);
    setError("");
    try {
      const result = await action(root);
      setRevision(value => value + 1);
      return result;
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      return null;
    } finally {
      pending.current = false;
      setBusy(false);
    }
  };

  const selected = repo?.status?.files.find(entry => entry.path === file);
  return (
    <section className="gb-work-remote-git" aria-label="Remote Git review">
      <div className="gb-work-remote-git__row">
        <span className="gb-badge gb-badge--accent">{repo?.status?.branch || (repo?.status?.detached ? "Detached HEAD" : "Git review")}</span>
        <button className="gb-btn gb-btn--secondary gb-btn--sm" type="button" disabled={busy}
          onClick={() => setRevision(value => value + 1)}>Refresh</button>
      </div>
      {error && <p role="alert">{error}</p>}
      {!repo && !error && <p role="status">Reading Git status…</p>}
      {repo && !repo.status && <p>The Work folder is outside a Git repository.</p>}
      {repo?.status && !repo.status.changed && <p>Working tree clean.</p>}
      {repo?.status && repo.status.changed > 0 && (
        <label>
          Changed file
          <select className="gb-input" value={file} disabled={busy} onChange={event => setFile(event.target.value)}>
            <option value="">Select a file</option>
            {repo.status.files.map(entry => <option value={entry.path} key={entry.path}>{entry.path}</option>)}
          </select>
        </label>
      )}
      {selected && (
        <>
          <label>
            Diff scope
            <select className="gb-input" value={mode} onChange={event => setMode(event.target.value as typeof mode)}>
              <option value="uncommitted">Uncommitted</option>
              <option value="unstaged">Unstaged</option>
              <option value="staged">Staged</option>
            </select>
          </label>
          <div className="gb-work-remote-git__row">
            {selected.unstaged && <button className="gb-btn gb-btn--secondary gb-btn--sm" type="button" disabled={busy}
              onClick={() => void mutate(path => client.changeGitStaging(path, file, true))}>Stage</button>}
            {selected.staged && <button className="gb-btn gb-btn--secondary gb-btn--sm" type="button" disabled={busy}
              onClick={() => void mutate(path => client.changeGitStaging(path, file, false))}>Unstage</button>}
          </div>
          {diff !== undefined && <pre className="gb-term-output">{diff.slice(0, 64_000) || "No changes in this scope."}{diff.length > 64_000 ? "\n… Diff truncated" : ""}</pre>}
        </>
      )}
      {repo?.status && <>
        <WorkGitActions busy={busy} staged={repo.status.files.some(entry => entry.staged)} remoteChanges={repo.status.changed > 0}
          detached={repo.status.detached} run={action => mutate(async path => {
            if (action.type === "commit") {
              await client.commitGitChanges(path, action.message);
              showToast("Changes committed");
            } else if (action.type === "push") {
              await client.pushGitBranch(path);
              showToast("Git push finished");
            } else {
              const result = await client.switchGitBranch(path, action.branch);
              showToast(`Switched to ${result.branch}`);
            }
            return true;
          }).then(Boolean)}
          createWorktree={async branch => (await mutate(path => client.createGitWorktree(path, branch)))?.path ?? null}
          openWorktree={async path => {
            try { await createNewSession({ section: "work", cwd: path }); }
            catch (reason) { setError(reason instanceof Error ? reason.message : String(reason)); }
          }} />
        <WorkPullRequest client={client} cwd={cwd} disabled={busy} />
      </>}
    </section>
  );
}
