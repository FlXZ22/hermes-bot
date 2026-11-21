import React, { useCallback, useEffect, useRef, useState } from "react";
import { CursorIcon } from "@grok-bot/theme";
import { AnimatedBotFace } from "./AnimatedBotFace";
import { tauriBridge, type AppUpdateStatus } from "../lib/tauriBridge";

interface UpdatesSettingsTabProps {
  isDesktop: boolean;
  showToast: (msg: string) => void;
}

function formatDate(iso: string | null | undefined): string {
  if (!iso || iso === "unknown") return "Unknown";
  const parsed = new Date(iso);
  if (Number.isNaN(parsed.getTime())) return iso;
  return parsed.toLocaleString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function phaseBadge(status: AppUpdateStatus): { label: string; tone: string } {
  switch (status.phase) {
    case "checking":
      return { label: "Checking…", tone: "gb-badge--info" };
    case "available":
      return {
        label:
          status.commitsBehind > 0
            ? `Update available (${status.commitsBehind} commit${status.commitsBehind === 1 ? "" : "s"})`
            : "Update available",
        tone: "gb-badge--accent",
      };
    case "updating":
      return { label: "Updating…", tone: "gb-badge--warning" };
    case "restart_required":
      return { label: "Restart required", tone: "gb-badge--success" };
    case "error":
      return { label: "Update error", tone: "gb-badge--danger" };
    case "up_to_date":
      return { label: "Up to date", tone: "gb-badge--success" };
    default:
      return { label: "Ready", tone: "gb-badge--muted" };
  }
}

export const UpdatesSettingsTab: React.FC<UpdatesSettingsTabProps> = ({ isDesktop, showToast }) => {
  const [status, setStatus] = useState<AppUpdateStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [checking, setChecking] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [copied, setCopied] = useState(false);
  const [forceNativeRebuild, setForceNativeRebuild] = useState(false);
  const [repoPathInput, setRepoPathInput] = useState("");
  const [savingRepo, setSavingRepo] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);
  const logRef = useRef<HTMLPreElement | null>(null);

  const refreshStatus = useCallback(async () => {
    if (!isDesktop) return;
    try {
      const next = await tauriBridge.getUpdateStatus();
      setStatus(next);
      setRepoPathInput((prev) => (prev ? prev : next.repoPath ?? ""));
      setLoadError(null);
    } catch (error) {
      setLoadError(error instanceof Error ? error.message : String(error));
    }
  }, [isDesktop]);

  const autoCheckedRef = useRef(false);

  useEffect(() => {
    if (!isDesktop) return;
    setLoading(true);
    void tauriBridge
      .getUpdateStatus()
      .then((next) => {
        setStatus(next);
        setRepoPathInput((prev) => (prev ? prev : next.repoPath ?? ""));
        setLoadError(null);
        if (next.autoCheck && next.phase === "idle" && !next.lastCheckedAt && !autoCheckedRef.current) {
          autoCheckedRef.current = true;
          setChecking(true);
          void tauriBridge
            .checkForUpdates()
            .then(setStatus)
            .catch(() => undefined)
            .finally(() => setChecking(false));
        }
      })
      .catch((error) => {
        setLoadError(error instanceof Error ? error.message : String(error));
      })
      .finally(() => setLoading(false));
  }, [isDesktop]);

  // Poll live progress & logs while an update is running.
  useEffect(() => {
    if (!isDesktop || status?.phase !== "updating") return;
    const timer = window.setInterval(() => {
      void refreshStatus();
    }, 600);
    return () => window.clearInterval(timer);
  }, [isDesktop, status?.phase, refreshStatus]);

  // Auto-scroll build logs to the bottom as new lines arrive.
  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [status?.logs.length]);

  const handleCheck = async () => {
    if (!isDesktop) return;
    setChecking(true);
    setLoadError(null);
    try {
      const next = await tauriBridge.checkForUpdates();
      setStatus(next);
      if (next.phase === "available") {
        showToast(`Update available on ${next.targetBranch} (${next.latestCommit ?? "new commits"})`);
      } else if (next.phase === "up_to_date") {
        showToast("Hermes Grok Bot is up to date");
      } else if (next.error) {
        showToast(next.error);
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLoadError(message);
      showToast(message);
    } finally {
      setChecking(false);
    }
  };

  const handleApply = async (mode: "remote" | "local") => {
    if (!isDesktop) return;
    setLoadError(null);
    try {
      const next = await tauriBridge.applyAppUpdate(mode, forceNativeRebuild);
      setStatus(next);
      showToast(
        mode === "local"
          ? "Building & installing current local workspace…"
          : `Updating from origin/${next.targetBranch}…`,
      );
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setLoadError(message);
      showToast(message);
    }
  };

  const handleRestart = async () => {
    if (!isDesktop) return;
    setRestarting(true);
    try {
      await tauriBridge.restartApp();
    } catch (error) {
      setRestarting(false);
      showToast(error instanceof Error ? error.message : String(error));
    }
  };

  const handleBranchChange = async (branch: string) => {
    if (!isDesktop) return;
    try {
      const next = await tauriBridge.setUpdateSettings({ targetBranch: branch });
      setStatus(next);
      showToast(`Update branch set to ${branch}`);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  };

  const handleAutoCheckToggle = async (enabled: boolean) => {
    if (!isDesktop) return;
    try {
      const next = await tauriBridge.setUpdateSettings({ autoCheck: enabled });
      setStatus(next);
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    }
  };

  const handleSaveRepoPath = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!isDesktop) return;
    setSavingRepo(true);
    try {
      const next = await tauriBridge.setUpdateSettings({ repoPath: repoPathInput.trim() });
      setStatus(next);
      showToast(next.repoValid ? "Repository path saved" : "Path saved, but no valid git checkout found there");
    } catch (error) {
      showToast(error instanceof Error ? error.message : String(error));
    } finally {
      setSavingRepo(false);
    }
  };

  const handleCopyVersion = async () => {
    if (!status) return;
    const summary = [
      `Version: ${status.currentVersion}`,
      `Commit: ${status.currentCommit}`,
      `Native Commit: ${status.nativeCommit}`,
      `Built: ${status.builtAt}`,
      `Branch: ${status.targetBranch} (checked out: ${status.currentBranch})`,
      `Host: ${status.host}`,
    ].join("\n");
    try {
      await navigator.clipboard.writeText(summary);
      setCopied(true);
      showToast("Copied version info");
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      showToast("Unable to copy version info");
    }
  };

  if (!isDesktop) {
    return (
      <div className="gb-section-card">
        <div className="gb-section-card__head">
          <span className="gb-section-card__title">App Updates</span>
          <span className="gb-badge gb-badge--muted">Web Mode</span>
        </div>
        <div className="gb-row">
          <div className="gb-row__info">
            <div className="gb-row__label">Desktop Updater Unavailable in Browser</div>
            <div className="gb-row__desc">
              Open the installed Linux desktop app (<code>hermes-grok-bot</code>) to check for updates, switch branches,
              and install fast UI updates without rebuilding Tauri.
            </div>
          </div>
          <a
            href="https://github.com/FlXZ22/hermes-bot"
            target="_blank"
            rel="noreferrer"
            className="gb-btn gb-btn--secondary gb-btn--sm"
            style={{ textDecoration: "none" }}
          >
            <CursorIcon name="link" size={13} />
            GitHub Repo
          </a>
        </div>
      </div>
    );
  }

  const isBusy = checking || status?.phase === "checking" || status?.phase === "updating";
  const badge = status ? phaseBadge(status) : { label: loading ? "Loading…" : "Ready", tone: "gb-badge--muted" };

  return (
    <>
      <div className="gb-section-card">
        <div className="gb-section-card__head">
          <span className="gb-section-card__title">App Updates</span>
          <span className={`gb-badge ${badge.tone}`}>{badge.label}</span>
        </div>

        <div className="gb-row">
          <div style={{ display: "flex", alignItems: "center", gap: 12, flex: 1, minWidth: 0 }}>
            <AnimatedBotFace
              seed="hermes-updater"
              size={36}
              state={
                status?.phase === "updating"
                  ? "progress"
                  : status?.phase === "checking"
                    ? "searching"
                    : status?.phase === "restart_required"
                      ? "celebrate"
                      : status?.phase === "error"
                        ? "failed"
                        : "idle"
              }
              maxFrameRate={30}
            />
            <div className="gb-row__info">
              <div className="gb-row__label">
                Hermes Grok Bot{" "}
                <span style={{ fontFamily: "var(--cursor-font-family-mono)", fontWeight: 500, fontSize: 12.5 }}>
                  v{status?.currentVersion ?? "1.0.0"}
                </span>
              </div>
              <div className="gb-row__desc">
                Commit{" "}
                <code style={{ fontFamily: "var(--cursor-font-family-mono)" }}>
                  {status?.currentCommit ?? "…"}
                </code>
                {" · "}
                Host: {status?.host === "webkit" ? "WebKitGTK (Tauri)" : "Chromium + Rust Native"}
                {" · "}
                Built {formatDate(status?.builtAt)}
              </div>
            </div>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 8, flexShrink: 0 }}>
            <button
              type="button"
              className="gb-btn gb-btn--ghost gb-btn--sm"
              disabled={!status}
              onClick={() => void handleCopyVersion()}
              title="Copy version information"
            >
              <CursorIcon name={copied ? "check" : "copy"} size={13} />
              {copied ? "Copied" : "Copy info"}
            </button>
            <button
              type="button"
              className="gb-btn gb-btn--secondary gb-btn--sm"
              disabled={isBusy}
              onClick={() => void handleCheck()}
            >
              <CursorIcon name="refresh" size={13} spin={checking || status?.phase === "checking"} />
              {checking || status?.phase === "checking" ? "Checking…" : "Check for Updates"}
            </button>
          </div>
        </div>

        {status?.phase === "restart_required" && (
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label">Update Ready to Launch</div>
              <div className="gb-row__desc">
                Installed commit{" "}
                <code style={{ fontFamily: "var(--cursor-font-family-mono)" }}>{status.latestCommit ?? "latest"}</code>.
                Restart Hermes Grok Bot to switch to the updated build.
              </div>
            </div>
            <button
              type="button"
              className="gb-btn gb-btn--primary"
              disabled={restarting}
              onClick={() => void handleRestart()}
            >
              <CursorIcon name="refresh" size={14} spin={restarting} />
              {restarting ? "Restarting…" : "Restart App Now"}
            </button>
          </div>
        )}

        {status?.phase === "available" && (
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label">
                New build available on{" "}
                <code style={{ fontFamily: "var(--cursor-font-family-mono)" }}>{status.targetBranch}</code>
              </div>
              <div className="gb-row__desc">
                {status.latestCommit && (
                  <>
                    <code style={{ fontFamily: "var(--cursor-font-family-mono)" }}>{status.latestCommit}</code>
                    {" — "}
                  </>
                )}
                {status.latestCommitMessage ?? "New commits ready to install"}
                {" · "}
                {status.rustRebuildNeeded
                  ? "Includes Rust native rebuild"
                  : "Fast UI update (skips Rust/Tauri rebuild)"}
              </div>
            </div>
            <button
              type="button"
              className="gb-btn gb-btn--primary"
              disabled={isBusy || !status.repoValid}
              onClick={() => void handleApply("remote")}
            >
              <CursorIcon name="cloud-download" size={14} />
              Update Now
            </button>
          </div>
        )}

        {status?.phase === "updating" && (
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label" style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <CursorIcon name="refresh" size={13} spin />
                {status.step ?? "Installing update…"}
              </div>
              <div className="gb-row__desc">
                Your current session stays active until you choose to restart.
              </div>
            </div>
          </div>
        )}

        {(loadError || status?.error) && (
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label" style={{ color: "var(--grok-bot-text-danger)", display: "flex", alignItems: "center", gap: 6 }}>
                <CursorIcon name="warning" size={14} />
                Update Failed
              </div>
              <div className="gb-row__desc" style={{ color: "var(--grok-bot-text-danger)" }}>
                {loadError ?? status?.error}
              </div>
            </div>
          </div>
        )}

        {status?.latestReleaseTag && status.latestReleaseUrl && (
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label">Latest GitHub Release: {status.latestReleaseTag}</div>
              <div className="gb-row__desc">Published on github.com/{status.githubRepo}</div>
            </div>
            <a
              href={status.latestReleaseUrl}
              target="_blank"
              rel="noreferrer"
              className="gb-btn gb-btn--ghost gb-btn--sm"
              style={{ textDecoration: "none" }}
            >
              <CursorIcon name="link" size={13} />
              View Release
            </a>
          </div>
        )}

        {status?.lastCheckedAt && (
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__desc">Last checked: {formatDate(status.lastCheckedAt)}</div>
            </div>
            <span className="gb-badge gb-badge--muted">{status.githubRepo}</span>
          </div>
        )}
      </div>

      <div className="gb-section-card">
        <div className="gb-section-card__head">
          <span className="gb-section-card__title">Update Channel & Local Preview</span>
          {status?.repoDirty && <span className="gb-badge gb-badge--warning">Uncommitted local changes</span>}
        </div>

        <div className="gb-row">
          <div className="gb-row__info">
            <div className="gb-row__label">Update Branch</div>
            <div className="gb-row__desc">
              Pull updates from this GitHub branch (currently checked out:{" "}
              <code style={{ fontFamily: "var(--cursor-font-family-mono)" }}>
                {status?.currentBranch ?? "main"}
              </code>
              )
            </div>
          </div>
          <select
            className="gb-select"
            style={{ width: 220 }}
            disabled={isBusy || !status?.repoValid}
            value={status?.targetBranch ?? "main"}
            onChange={(e) => void handleBranchChange(e.target.value)}
          >
            {(status?.availableBranches ?? ["main"]).map((branch) => (
              <option key={branch} value={branch}>
                {branch}
              </option>
            ))}
          </select>
        </div>

        <div className="gb-row">
          <div className="gb-row__info">
            <div className="gb-row__label">Check for Updates Automatically</div>
            <div className="gb-row__desc">Check GitHub for newer commits when opening Updates</div>
          </div>
          <div className="gb-segmented">
            <button
              type="button"
              className="gb-segmented__item"
              data-active={status?.autoCheck ? "true" : undefined}
              disabled={isBusy}
              onClick={() => void handleAutoCheckToggle(true)}
            >
              On
            </button>
            <button
              type="button"
              className="gb-segmented__item"
              data-active={status && !status.autoCheck ? "true" : undefined}
              disabled={isBusy}
              onClick={() => void handleAutoCheckToggle(false)}
            >
              Off
            </button>
          </div>
        </div>

        <div className="gb-row">
          <div className="gb-row__info">
            <div className="gb-row__label">Install Current Local Branch</div>
            <div className="gb-row__desc">
              Fast-install your current local checkout (
              <code style={{ fontFamily: "var(--cursor-font-family-mono)" }}>
                {status?.currentBranch ?? "main"}
              </code>
              ) without pulling or touching git state. Skips Rust/Tauri rebuild unless native files changed.
            </div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 10, flexShrink: 0 }}>
            <label
              style={{
                display: "inline-flex",
                alignItems: "center",
                gap: 6,
                fontSize: 12,
                color: "var(--grok-bot-text-secondary)",
                cursor: "pointer",
              }}
            >
              <input
                type="checkbox"
                checked={forceNativeRebuild}
                disabled={isBusy}
                onChange={(e) => setForceNativeRebuild(e.target.checked)}
              />
              Force Rust rebuild
            </label>
            <button
              type="button"
              className="gb-btn gb-btn--secondary gb-btn--sm"
              disabled={isBusy || !status?.repoValid}
              onClick={() => void handleApply("local")}
            >
              <CursorIcon name="git-branch" size={13} />
              Install Local Build
            </button>
          </div>
        </div>

        <form className="gb-row" onSubmit={handleSaveRepoPath} style={{ gap: 10 }}>
          <div className="gb-row__info" style={{ flex: "0 0 180px" }}>
            <div className="gb-row__label">Local Repository</div>
            <div className="gb-row__desc">
              {status?.repoValid ? "Valid git checkout" : "Not configured"}
            </div>
          </div>
          <input
            className="gb-input gb-input--mono"
            style={{ flex: 1 }}
            placeholder="/path/to/hermes-bot"
            value={repoPathInput}
            onChange={(e) => setRepoPathInput(e.target.value)}
            disabled={isBusy || savingRepo}
          />
          <button
            type="submit"
            className="gb-btn gb-btn--secondary gb-btn--sm"
            disabled={isBusy || savingRepo}
          >
            {savingRepo ? "Saving…" : "Save"}
          </button>
        </form>
      </div>

      {status && status.logs.length > 0 && (
        <div className="gb-section-card">
          <div className="gb-section-card__head">
            <span className="gb-section-card__title">Update Activity Log</span>
            <span className="gb-badge gb-badge--muted">{status.logs.length} lines</span>
          </div>
          <div className="gb-section-card__body">
            <pre ref={logRef} className="gb-term-output" style={{ maxHeight: 260, margin: 0 }}>
              {status.logs.join("\n")}
            </pre>
          </div>
        </div>
      )}
    </>
  );
};
