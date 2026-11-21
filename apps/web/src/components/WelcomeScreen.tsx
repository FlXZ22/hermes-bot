import React, { useEffect, useRef, useState } from "react";
import { CursorIcon } from "@grok-bot/theme";
import { HermesClient, normalizeServerUrl, normalizeSshTarget, connectionError, setNativeHttpAdapter, type HermesConnection, type HermesSshTarget } from "@grok-bot/hermes-client";
import { AnimatedBotFace } from "./AnimatedBotFace";
import { useApp } from "../state/appStore";
import { tauriBridge } from "../lib/tauriBridge";
import { SshFields } from "./SshFields";
import "./welcome.css";

type Step = "welcome" | "choose" | "remote" | "ready";

export function WelcomeScreen() {
  const { connection, connectionStatus, connectionDetail, isDesktop, isLocalConnection, initializing,
    startLocalConnection, updateConnection, setSetupOpen, theme, toggleTheme, models, activeModel,
    activeProvider, setActiveView, setModelPickerOpen, profiles, sessions } = useApp();
  const [step, setStep] = useState<Step>(connectionStatus === "connected" ? "choose" : connectionStatus === "error" && !isLocalConnection ? "remote" : "welcome");
  const [name, setName] = useState(isLocalConnection ? "" : connection.name);
  const [url, setUrl] = useState(isLocalConnection ? "" : connection.url);
  const [auth, setAuth] = useState<"browser" | "bearer" | "session-token" | "none">(isDesktop ? "browser" : "session-token");
  const [token, setToken] = useState("");
  const [sshEnabled, setSshEnabled] = useState(!!connection.ssh);
  const [ssh, setSsh] = useState<HermesSshTarget>(connection.ssh ?? { host: "" });
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [loginUrl, setLoginUrl] = useState("");
  const [providers, setProviders] = useState<Array<{ name: string; display_name: string }>>([]);
  const [provider, setProvider] = useState("");
  const [copied, setCopied] = useState(false);
  const operation = useRef(0);
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => { heading.current?.focus(); }, [step]);
  useEffect(() => () => { operation.current++; if (isDesktop) void tauriBridge.cancelRemoteLogin(); }, [isDesktop]);
  // A failed saved connection becomes a recoverable setup screen on relaunch.
  useEffect(() => {
    if (!initializing && connectionStatus === "error") {
      setError(connectionDetail);
      if (!isLocalConnection) { setUrl(connection.url); setName(connection.name); setStep("remote"); }
      else setStep("choose");
    }
  }, [initializing]);

  const useLocal = async () => {
    setBusy("Starting Hermes…"); setError("");
    try { await startLocalConnection(); setStep("ready"); }
    catch (err) { setError(connectionError(err)); }
    finally { setBusy(""); }
  };

  const connectRemote = async (event: React.FormEvent) => {
    event.preventDefault(); setError(""); setCopied(false);
    const current = ++operation.current;
    setBusy(auth === "browser" ? "Finding sign-in options…" : "Checking your connection…");
    try {
      const address = normalizeServerUrl(url); setUrl(address);
      const candidate: HermesConnection = {
        id: `remote:${address}`, name: name.trim() || new URL(address).hostname,
        url: address, mode: "dashboard", authType: auth === "browser" ? "bearer" : auth,
        ssh: sshEnabled ? normalizeSshTarget(ssh) : undefined,
        token: auth === "none" || auth === "browser" ? undefined : token.trim(),
      };
      if (auth !== "none" && auth !== "browser" && !candidate.token) throw new Error("Paste the access token from your Hermes server.");
      if (auth === "browser") {
        setNativeHttpAdapter(tauriBridge.createHttpAdapter());
        let selected = provider;
        if (!selected || !providers.length) {
          const response = await new HermesClient(candidate).request<{ providers: Array<{ name: string; display_name: string }> }>("/api/auth/providers");
          if (!Array.isArray(response.providers) || !response.providers.length) throw new Error("This server has no browser sign-in configured. Use an access token or an SSH tunnel.");
          setProviders(response.providers); selected = response.providers[0].name; setProvider(selected);
          if (response.providers.length > 1) { setBusy(""); return; }
        }
        if (operation.current !== current) return;
        setBusy("Finish signing in in your browser…");
        setLoginUrl(await tauriBridge.beginRemoteLogin(address, selected));
        while (operation.current === current) {
          await new Promise(resolve => setTimeout(resolve, 600));
          if (operation.current !== current) return;
          const tokens = await tauriBridge.pollRemoteLogin();
          if (tokens) {
            candidate.token = tokens.access_token; candidate.refreshToken = tokens.refresh_token;
            candidate.expiresAt = tokens.expires_at; candidate.authProvider = tokens.provider;
            break;
          }
        }
        if (operation.current !== current) return;
      }
      setLoginUrl(""); setBusy("Checking access and live chat…");
      await updateConnection(candidate);
      if (operation.current === current) { setToken(""); setStep("ready"); }
    } catch (err) { if (operation.current === current) setError(connectionError(err)); }
    finally { if (operation.current === current) { setBusy(""); setLoginUrl(""); } }
  };

  const cancelLogin = async () => {
    operation.current++; setLoginUrl(""); setBusy("Cancelling sign-in…");
    try { await tauriBridge.cancelRemoteLogin(); }
    finally { setBusy(""); }
  };
  const modelReady = models.some(model => model.configured && (model.id === activeModel || model.id.endsWith(`/${activeModel}`))
    && (activeProvider === "auto" || model.provider === activeProvider));
  const openApp = (view: "chat" | "settings" = "chat") => { setActiveView(view); setSetupOpen(false); };
  const title = initializing ? "Opening Hermes…" : step === "welcome" ? "Meet Hermes." : step === "choose" ? "Where does Hermes live?"
    : step === "remote" ? "Your Hermes. Anywhere." : "You’re connected.";

  return <main className="gb-onboarding grok-bot-root">
    <header className="gb-welcome__bar" data-tauri-drag-region="" onMouseDown={event => {
      if (isDesktop && event.target === event.currentTarget && event.button === 0) void tauriBridge.windowStartDrag();
    }}>
      {isDesktop ? <div className="gb-traffic-lights" aria-label="Window controls">
        <button className="gb-traffic-light gb-traffic-light--close" aria-label="Close" onClick={() => void tauriBridge.windowClose()} />
        <button className="gb-traffic-light gb-traffic-light--min" aria-label="Minimize" onClick={() => void tauriBridge.windowMinimize()} />
        <button className="gb-traffic-light gb-traffic-light--max" aria-label="Maximize" onClick={() => void tauriBridge.windowToggleMaximize()} />
      </div> : <span className="gb-welcome__wordmark">Hermes</span>}
      <div className="gb-welcome__bar-actions">
        {connectionStatus === "connected" && step !== "ready" && !busy && <button className="gb-btn gb-btn--ghost gb-btn--sm" onClick={() => setSetupOpen(false)}>Back to Hermes</button>}
        <button className="gb-icon-btn" aria-label={`Use ${theme === "dark" ? "light" : "dark"} theme`} onClick={toggleTheme}><CursorIcon name={theme === "dark" ? "sun" : "moon"} size={16} /></button>
      </div>
    </header>
    <div className={`gb-welcome__stage gb-welcome__stage--${step}`}>
      {!initializing && step !== "welcome" && step !== "ready" && <button className="gb-welcome__back" disabled={!!busy} onClick={() => { setError(""); setStep(step === "remote" ? "choose" : "welcome"); }}><CursorIcon name="arrow-left" size={14} /> Back</button>}
      <div className="gb-welcome__hero" aria-hidden="true">
        <span className="gb-welcome__friend gb-welcome__friend--one">
          <AnimatedBotFace seed="welcome-red" colorId="red" shapeId="blob" size={42} state={step === "ready" ? "happy" : "idle"} isFollowingPointer maxFrameRate={60} />
        </span>
        <span className="gb-welcome__hero-bot">
          <AnimatedBotFace
            seed="welcome"
            colorId={step === "ready" ? "blue" : "black"}
            shapeId="blob"
            size={step === "welcome" ? 96 : 74}
            state={busy || initializing ? "orbit" : step === "ready" ? "celebrate" : "idle"}
            isFollowingPointer={!busy && !initializing}
            clickableSpin
            maxFrameRate={60}
          />
        </span>
        <span className="gb-welcome__friend gb-welcome__friend--two">
          <AnimatedBotFace seed="welcome-cyan" colorId="cyan" shapeId="blob" size={34} state={step === "ready" ? "excited" : "idle"} isFollowingPointer maxFrameRate={60} />
        </span>
      </div>
      <h1 ref={heading} tabIndex={-1}>{title}</h1>
      <p className="gb-welcome__subtitle">{initializing ? connectionDetail : step === "welcome" ? "A little help. A lot of possibility."
        : step === "choose" ? "Work on this computer, or pick up with your agent on a server."
        : step === "remote" ? "Bring your sessions, boards, skills, and models with you."
        : `Connected to ${connection.name}. Your workspace is ready to explore.`}</p>

      {!initializing && step === "welcome" && <div className="gb-welcome__landing">
        <p>Chat, build, and get things done with your own agent.</p>
        <button className="gb-btn gb-btn--primary gb-welcome__continue" onClick={() => setStep("choose")}>Get started <CursorIcon name="arrow-right" size={15} /></button>
        <button className="gb-welcome__link" onClick={() => setStep("remote")}>Already running Hermes? Connect your server</button>
      </div>}

      {!initializing && step === "choose" && <div className="gb-welcome__choices">
        <button className="gb-welcome__choice" onClick={() => void useLocal()} disabled={!!busy}>
          <span className="gb-welcome__choice-icon"><CursorIcon name="laptop" size={22} /></span>
          <span><strong>This computer</strong><small>Start your installed Hermes Agent.<br />Your work stays on this device.</small></span><CursorIcon name="arrow-right" size={16} />
        </button>
        <button className="gb-welcome__choice" onClick={() => { setError(""); setStep("remote"); }} disabled={!!busy}>
          <span className="gb-welcome__choice-icon"><CursorIcon name="server" size={22} /></span>
          <span><strong>Connect a server</strong><small>Your own server, VPS, or home lab.<br />Continue where your agent left off.</small></span><CursorIcon name="arrow-right" size={16} />
        </button>
      </div>}

      {!initializing && step === "remote" && <form className="gb-welcome__form" onSubmit={connectRemote}>
        <fieldset disabled={!!busy}>
          <label className="gb-field"><span className="gb-field__label">Server address</span>
            <input className="gb-input" type="text" inputMode="url" autoComplete="url" placeholder="https://hermes.example.com" value={url} onChange={e => { setUrl(e.target.value); setProviders([]); setProvider(""); }} required autoFocus aria-describedby="server-hint" />
            <small id="server-hint">Use your Hermes dashboard address. An SSH tunnel works too.</small>
          </label>
          <label className="gb-field"><span className="gb-field__label">Connection name <span className="gb-welcome__optional">optional</span></span>
            <input className="gb-input" placeholder="My server" autoComplete="off" value={name} onChange={e => setName(e.target.value)} />
          </label>
          <label className="gb-field"><span className="gb-field__label">Sign-in method</span>
            <select className="gb-select" value={auth} onChange={e => setAuth(e.target.value as typeof auth)}>
              {isDesktop && <option value="browser">Sign in with browser · Recommended</option>}
              <option value="bearer">Access token</option><option value="session-token">Session token · Local or SSH tunnel</option><option value="none">No authentication · Trusted network</option>
            </select>
          </label>
          {auth === "browser" ? <>
            <p className="gb-welcome__hint">Your browser will open for your server’s password or SSO sign-in. Return here when you’re done.</p>
            {providers.length > 1 && <label className="gb-field"><span className="gb-field__label">Sign in with</span><select className="gb-select" value={provider} onChange={e => setProvider(e.target.value)}>{providers.map(item => <option key={item.name} value={item.name}>{item.display_name}</option>)}</select></label>}
          </> : auth !== "none" && <label className="gb-field"><span className="gb-field__label">{auth === "bearer" ? "Access token" : "Hermes session token"}</span><input className="gb-input" type="password" autoComplete="off" value={token} onChange={e => setToken(e.target.value)} required placeholder="Paste your server token" /></label>}
          {isDesktop && <details className="gb-welcome__help" style={{ marginTop: 0 }}><summary>SSH access for terminals and Git <span className="gb-welcome__optional">optional</span></summary>
            <label style={{ display: "flex", gap: 8, alignItems: "center", marginTop: 12 }}><input type="checkbox" checked={sshEnabled} onChange={e => setSshEnabled(e.target.checked)} /> Enable SSH workspace tools</label>
            {sshEnabled && <SshFields value={ssh} onChange={setSsh} />}
          </details>}
          <button className="gb-btn gb-btn--primary gb-welcome__submit" type="submit">{auth === "browser" ? "Sign in with browser" : "Connect to Hermes"}<CursorIcon name="arrow-right" size={15} /></button>
          <p className="gb-welcome__storage">{isDesktop ? "Connection details are saved in a private file on this computer." : "Connection details are remembered in this browser."}</p>
        </fieldset>
        {!busy && <details className="gb-welcome__help"><summary>Need help connecting?</summary>
          <p>Start Hermes on your server with <code>hermes serve</code>. For a public dashboard, configure Hermes authentication and HTTPS first, then use browser sign-in.</p>
          <p>For a private server, keep Hermes bound to <code>127.0.0.1:9119</code> and open an SSH tunnel from this computer:</p>
          <code className="gb-welcome__command">ssh -N -L 9119:127.0.0.1:9119 user@your-server</code>
          <p>Leave that terminal open. Enter <code>http://127.0.0.1:9119</code> above, choose Session token, and paste the server’s <code>HERMES_DASHBOARD_SESSION_TOKEN</code>.</p>
          <a href="https://hermes-agent.nousresearch.com/desktop" target="_blank" rel="noreferrer">Hermes setup guide <CursorIcon name="arrow-right" size={12} /></a>
        </details>}
      </form>}

      {!initializing && step === "ready" && <div className="gb-welcome__ready">
        <div className="gb-welcome__check"><CursorIcon name="check" size={16} /><span>Server access and live chat verified</span></div>
        <div className="gb-welcome__check"><CursorIcon name="check" size={16} /><span>{sessions.length} saved sessions · {profiles.length} {profiles.length === 1 ? "profile" : "profiles"}</span></div>
        <div className="gb-welcome__model"><CursorIcon name={modelReady ? "check" : "info"} size={16} /><span>{modelReady ? `Model: ${activeModel}` : "Choose a configured model before your first message."}</span></div>
        <button className="gb-btn gb-btn--primary gb-welcome__submit" onClick={() => openApp()}>Open Hermes <CursorIcon name="arrow-right" size={15} /></button>
        {!modelReady && <button className="gb-welcome__link" onClick={() => { openApp(); setModelPickerOpen(true); }}>Choose a model</button>}
      </div>}
      {(busy || initializing) && <div className="gb-welcome__progress" role="status" aria-live="polite"><CursorIcon name="spinner" size={16} spin />{busy || connectionDetail}</div>}
      {loginUrl && <div className="gb-welcome__login-actions">
        <button className="gb-welcome__link" onClick={() => void navigator.clipboard.writeText(loginUrl).then(() => setCopied(true)).catch(() => setError("Your clipboard is unavailable. Check your default browser and try signing in again."))}>{copied ? "Sign-in link copied" : "Copy sign-in link"}</button>
        <button className="gb-welcome__link" onClick={() => void cancelLogin()}>Cancel sign-in</button>
      </div>}
      {error && !initializing && <div className="gb-welcome__error" role="alert"><CursorIcon name="info" size={17} /><span>{error}</span></div>}
    </div>
    <footer className="gb-welcome__footer"><span>Hermes for Linux</span><span>Your agent. Your infrastructure.</span></footer>
  </main>;
}
