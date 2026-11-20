import React, { useCallback, useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SerializeAddon } from "@xterm/addon-serialize";
import { CursorIcon } from "@grok-bot/theme";
import type { HermesSshTarget } from "@grok-bot/hermes-client";
import { tauriBridge } from "../lib/tauriBridge";
import { useApp } from "../state/appStore";
import "@xterm/xterm/css/xterm.css";
import "./terminal.css";

interface TerminalTab { id: string; title: string; cwd: string; buffer?: string }
const MAX_TABS = 16;

function readTabs(key: string): { tabs: TerminalTab[]; active: string | null } {
  try {
    const stored = JSON.parse(localStorage.getItem(key) ?? "{}");
    const tabs = Array.isArray(stored.tabs) ? stored.tabs.filter((tab: TerminalTab) => tab && typeof tab.id === "string" && typeof tab.cwd === "string" && typeof tab.title === "string").slice(0, MAX_TABS).map((tab: TerminalTab) => ({ ...tab, buffer: typeof tab.buffer === "string" ? tab.buffer.slice(-48000) : undefined })) : [];
    return { tabs, active: tabs.some((tab: TerminalTab) => tab.id === stored.active) ? stored.active : tabs[0]?.id ?? null };
  } catch { return { tabs: [], active: null }; }
}

function TerminalSurface({ tab, active, ssh, update, theme }: { tab: TerminalTab; active: boolean; ssh?: HermesSshTarget; update: (id: string, patch: Partial<TerminalTab>) => void; theme: string }) {
  const host = useRef<HTMLDivElement>(null);
  const terminal = useRef<Terminal | null>(null);
  const fitAddon = useRef<FitAddon | null>(null);
  const [status, setStatus] = useState("Starting…");
  const [error, setError] = useState("");
  const [restart, setRestart] = useState(0);
  const saved = useRef(tab); saved.current = tab;
  const updateRef = useRef(update); updateRef.current = update;
  useEffect(() => {
    if (!host.current) return;
    let disposed = false;
    let connected = false;
    let endedCode: number | null = null;
    let failed = false;
    let generation = "";
    const consumed = new Map<string, number>();
    let ackTimer: ReturnType<typeof setTimeout> | undefined;
    let snapshotTimer: ReturnType<typeof setTimeout> | undefined;
    let writeQueue: Promise<unknown> = Promise.resolve();
    const term = new Terminal({ fontFamily: '"XVF Mono", monospace', fontSize: 12, lineHeight: 1.25, cursorBlink: true, scrollback: 5000, screenReaderMode: true });
    const fit = new FitAddon(); const serialize = new SerializeAddon();
    term.loadAddon(fit); term.loadAddon(serialize); term.open(host.current);
    terminal.current = term; fitAddon.current = fit;
    const snapshot = () => {
      snapshotTimer = undefined;
      if (!disposed) {
        const buffer = serialize.serialize({ scrollback: 200, excludeAltBuffer: true, excludeModes: true }).slice(-48000);
        updateRef.current(tab.id, { buffer });
      }
    };
    const acknowledge = () => {
      clearTimeout(ackTimer); ackTimer = undefined;
      for (const [stream, bytes] of consumed) {
        if (bytes) void tauriBridge.terminalAcknowledge(tab.id, stream, bytes).catch(() => undefined);
      }
      consumed.clear();
    };
    const onOutput = (stream: string, bytes: number) => {
      if (disposed) return;
      consumed.set(stream, (consumed.get(stream) ?? 0) + bytes);
      if ((consumed.get(stream) ?? 0) >= 64 * 1024) acknowledge();
      else if (!ackTimer) ackTimer = setTimeout(acknowledge, 20);
      if (!snapshotTimer) snapshotTimer = setTimeout(snapshot, 750);
    };
    const onData = term.onData(data => {
      if (!connected || disposed) return;
      writeQueue = writeQueue.catch(() => undefined).then(() => disposed ? undefined : tauriBridge.terminalWrite(tab.id, data))
        .catch(err => { if (!disposed) setError(String(err)); });
    });
    const onResize = term.onResize(size => { if (connected) void tauriBridge.terminalResize(tab.id, size.cols, size.rows).catch(() => undefined); });
    // The shell owns these shortcuts. Keep app-wide handlers out of readline.
    term.attachCustomKeyEventHandler(event => {
      if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.code === "KeyC") {
        if (event.type === "keydown" && term.hasSelection()) void navigator.clipboard.writeText(term.getSelection()).catch(() => undefined);
        return false;
      }
      if ((event.ctrlKey || event.metaKey) && event.shiftKey && event.code === "KeyV") {
        if (event.type === "keydown") void navigator.clipboard.readText().then(text => { if (!disposed) term.paste(text); }).catch(() => setError("Clipboard is unavailable. Use the terminal’s paste shortcut."));
        return false;
      }
      return true;
    });
    setStatus("Starting…"); setError("");
    void tauriBridge.terminalStart(tab.id, saved.current.cwd, term.cols, term.rows, ssh, event => {
      if (disposed) return;
      if (event.type === "started") {
        connected = true; generation = event.generation;
        term.reset();
        if (!event.reused && saved.current.buffer) {
          term.write(saved.current.buffer);
          term.writeln("\r\n\x1b[2m— Previous terminal history · new shell —\x1b[0m\r\n");
        }
        setStatus(ssh ? `SSH · ${ssh.host}` : "Shell");
      } else if (event.type === "output") {
        const bytes = Uint8Array.from(atob(event.data), value => value.charCodeAt(0));
        const stream = generation;
        term.write(bytes, () => onOutput(stream, bytes.length));
      } else if (event.type === "exit") {
        connected = false; endedCode = event.code; setStatus(`Exited (${event.code})`);
      } else if (event.type === "error") { setError(event.message); connected = false; failed = true; }
    }).then(info => {
      if (disposed) return;
      const exitCode = endedCode ?? info.exit_code;
      connected = exitCode === null && !failed;
      setStatus(connected ? info.remote ? `SSH · ${ssh?.host}` : info.shell : `Exited (${exitCode ?? "error"})`);
      if (info.cwd) updateRef.current(tab.id, { cwd: info.cwd });
      if (host.current?.clientWidth && host.current?.clientHeight) fit.fit();
      if (active) term.focus();
    }).catch(err => { if (!disposed) { setStatus("Closed"); setError(String(err)); } });
    const observer = new ResizeObserver(() => { if (host.current?.clientWidth && host.current?.clientHeight) fit.fit(); });
    observer.observe(host.current);
    // Serialize while the page is still alive. Processes survive renderer reload
    // in Rust; app relaunch starts a new shell beneath the saved scrollback.
    window.addEventListener("pagehide", snapshot);
    return () => {
      snapshot(); acknowledge(); disposed = true;
      clearTimeout(snapshotTimer); clearTimeout(ackTimer); observer.disconnect(); window.removeEventListener("pagehide", snapshot);
      onData.dispose(); onResize.dispose(); term.dispose();
      terminal.current = null; fitAddon.current = null;
    };
    // A terminal's starting directory is a snapshot, not tied to chat navigation.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tab.id, restart]);

  useEffect(() => {
    const term = terminal.current; const element = host.current;
    if (!term || !element) return;
    const styles = getComputedStyle(element);
    term.options.theme = {
      background: styles.getPropertyValue("--grok-bot-bg-editor").trim() || (theme === "dark" ? "#161616" : "#fff"),
      foreground: theme === "dark" ? "#dbdbdb" : "#363636", cursor: theme === "dark" ? "#f5f5f5" : "#1c1c1c",
      selectionBackground: theme === "dark" ? "#ffffff30" : "#00000020",
    };
    if (active) { fitAddon.current?.fit(); term.focus(); }
  }, [theme, active, restart]);

  useEffect(() => {
    if (!active || ssh) return;
    const timer = setInterval(() => { void tauriBridge.terminalInfo(tab.id).then(info => { if (info.cwd) updateRef.current(tab.id, { cwd: info.cwd }); }).catch(() => undefined); }, 2000);
    return () => clearInterval(timer);
  }, [active, ssh, tab.id]);

  return <section className="gb-terminal-session" style={{ display: active ? "flex" : "none" }} aria-label={tab.title}>
    <div className="gb-terminal-status"><span title={tab.cwd}>{status} · {tab.cwd || "Home"}</span><button className="gb-mini-icon-btn" aria-label="Restart terminal" title="Restart terminal" onClick={() => { void tauriBridge.terminalClose(tab.id).then(() => setRestart(value => value + 1)); }}><CursorIcon name="refresh" size={12} /></button><button className="gb-mini-icon-btn" aria-label="Clear terminal" title="Clear terminal" onClick={() => terminal.current?.clear()}><CursorIcon name="trash" size={12} /></button></div>
    {error && <div className="gb-terminal-error" role="alert">{error}</div>}
    <div className="gb-terminal-screen" ref={host} onMouseDown={() => terminal.current?.focus()} />
  </section>;
}

export function TerminalWorkspace({ cwd, visible }: { cwd: string; visible: boolean }) {
  const { isDesktop, isLocalConnection, connection, theme, setSetupOpen, client, initializing } = useApp();
  const scope = isLocalConnection ? "local" : `${connection.url}:${JSON.stringify(connection.ssh ?? null)}`;
  const key = `hermes-grok-bot:terminals:${scope}`;
  const [initial] = useState(() => readTabs(key));
  const [tabs, setTabs] = useState<TerminalTab[]>(initial.tabs);
  const [active, setActive] = useState<string | null>(initial.active);
  const [editing, setEditing] = useState<string | null>(null);
  const latestTabs = useRef(tabs); latestTabs.current = tabs;
  const activeRef = useRef(active); activeRef.current = active;
  const closeTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const canOpen = isDesktop && (isLocalConnection || !!connection.ssh);

  useEffect(() => {
    try { localStorage.setItem(key, JSON.stringify({ tabs, active })); } catch { /* A full store must not break a live terminal. */ }
  }, [tabs, active, key]);
  useEffect(() => {
    clearTimeout(closeTimer.current);
    return () => {
      // Delay one task so React StrictMode's effect check cannot kill a shell
      // immediately reattached by its second mount.
      closeTimer.current = setTimeout(() => { for (const tab of latestTabs.current) void tauriBridge.terminalClose(tab.id).catch(() => undefined); }, 0);
    };
  }, []);
  const update = useCallback((id: string, patch: Partial<TerminalTab>) => {
    const next = latestTabs.current.map(tab => tab.id === id && Object.entries(patch).some(([key, value]) => tab[key as keyof TerminalTab] !== value) ? { ...tab, ...patch } : tab);
    latestTabs.current = next;
    try { localStorage.setItem(key, JSON.stringify({ tabs: next, active: activeRef.current })); } catch { /* Quota may be full. */ }
    setTabs(next);
  }, [key]);
  const create = () => {
    const id = crypto.randomUUID();
    setTabs(tabs => [...tabs, { id, cwd, title: `Terminal ${tabs.length + 1}` }]); setActive(id);
  };
  const close = (id: string) => {
    void tauriBridge.terminalClose(id).catch(() => undefined);
    setTabs(tabs => tabs.filter(tab => tab.id !== id));
    if (active === id) setActive(tabs.find(tab => tab.id !== id)?.id ?? null);
  };

  return <div className="gb-terminal-workspace">
    <div className="gb-terminal-tabs" role="tablist" aria-label="Shell terminals">
      {tabs.map(tab => <div className="gb-terminal-tab" data-active={active === tab.id || undefined} key={tab.id}>
        {editing === tab.id ? <input className="gb-terminal-name" aria-label="Terminal name" defaultValue={tab.title} autoFocus onBlur={event => { update(tab.id, { title: event.target.value.trim() || "Terminal" }); setEditing(null); }} onKeyDown={event => { if (event.key === "Escape") { event.currentTarget.value = tab.title; event.currentTarget.blur(); } else if (event.key === "Enter") event.currentTarget.blur(); }} /> : <button role="tab" aria-selected={active === tab.id} onClick={() => setActive(tab.id)} onDoubleClick={() => setEditing(tab.id)} title={`${tab.title} · Double-click to rename`}><CursorIcon name="terminal" size={12} />{tab.title}</button>}
        <button className="gb-mini-icon-btn" aria-label={`Close ${tab.title}`} onClick={() => close(tab.id)}><CursorIcon name="close" size={10} /></button>
      </div>)}
      <button className="gb-mini-icon-btn" aria-label="New terminal" title="New terminal" disabled={!canOpen || tabs.length >= MAX_TABS} onClick={create}><CursorIcon name="plus" size={14} /></button>
    </div>
    {!canOpen ? <div className="gb-terminal-empty"><CursorIcon name="terminal" size={24} /><strong>{isDesktop ? "Connect your server’s shell" : "Interactive terminals run in the Linux app"}</strong><p>{isDesktop ? "Add SSH access to this connection to open a shell on your server. Your dashboard connection stays separate." : "Open Hermes Grok Bot to use the Rust terminal. Files and chat remain available in this preview."}</p>{isDesktop && <button className="gb-btn gb-btn--secondary gb-btn--sm" onClick={() => setSetupOpen(true)}>Configure SSH access</button>}</div>
      : !tabs.length ? <div className="gb-terminal-empty"><CursorIcon name="terminal" size={24} /><strong>A shell for your workspace</strong><p>Run interactive programs and keep your work in separate terminal tabs.</p><button className="gb-btn gb-btn--secondary gb-btn--sm" onClick={create}>Open terminal</button></div> : null}
    {canOpen && client && !initializing && tabs.map(tab => <TerminalSurface key={tab.id} tab={tab} active={visible && active === tab.id} ssh={isLocalConnection ? undefined : connection.ssh} update={update} theme={theme} />)}
  </div>;
}
