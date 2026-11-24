import React, { useState } from "react";
import type { HermesClient } from "@grok-bot/hermes-client";
import { Modal } from "@grok-bot/theme";
import { resolveWorkGitRoot } from "../lib/workGit";

export function WorkPullRequest({ client, cwd, disabled }: { client: HermesClient; cwd: string; disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const [url, setUrl] = useState("");
  const [error, setError] = useState("");

  const create = async () => {
    if (busy || url) return;
    setBusy(true);
    setError("");
    try {
      const root = await resolveWorkGitRoot(client, cwd);
      const result = await client.createGitPullRequest(root);
      const link = new URL(result.url);
      if (link.protocol !== "https:") throw new Error("Hermes returned an invalid pull request link.");
      setUrl(link.href);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : "Could not create the pull request.");
    } finally {
      setBusy(false);
    }
  };

  return <>
    <button className="gb-btn gb-btn--secondary gb-btn--sm" type="button" disabled={disabled}
      onClick={() => setOpen(true)}>{url ? "Open pull request" : "Create pull request"}</button>
    <Modal open={open} title={url ? "Pull request created" : "Create pull request"}
      onClose={() => { if (!busy) setOpen(false); }}
      footer={<>
        <button className="gb-btn gb-btn--secondary" type="button" disabled={busy}
          onClick={() => setOpen(false)}>{url ? "Close" : "Cancel"}</button>
        {!url && <button className="gb-btn gb-btn--primary" type="button" disabled={busy}
          onClick={() => void create()}>{busy ? "Creating…" : "Push and create pull request"}</button>}
        {url && <button className="gb-btn gb-btn--primary" type="button" onClick={() => {
          void navigator.clipboard.writeText(url).then(() => setOpen(false), () => setError("Could not copy the link."));
        }}>Copy link</button>}
      </>}>
      {url ? <a href={url} target="_blank" rel="noopener noreferrer">{url}</a>
        : <p>Hermes will push the current branch and use GitHub CLI to create a pull request from its commits.</p>}
      {error && <p role="alert">{error}</p>}
    </Modal>
  </>;
}
