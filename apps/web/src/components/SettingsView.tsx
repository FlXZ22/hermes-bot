import React, { useEffect, useState } from "react";
import { BotMark, CursorIcon } from "@grok-bot/theme";
import type { HermesEnvEntry, HermesSshTarget } from "@grok-bot/hermes-client";
import { normalizeSshTarget } from "@grok-bot/hermes-client";
import { SshFields } from "./SshFields";
import { useApp } from "../state/appStore";

export const SettingsView: React.FC = () => {
  const {
    theme,
    setTheme,
    isDesktop,
    rustBackend,
    connection,
    connectionStatus,
    connectionDetail,
    bootstrapMs,
    systemStats,
    setSetupOpen,
    updateConnection,
    isLocalConnection,
    restartLocalBackend,
    client,
    profiles,
    activeProfile,
    setActiveProfile,
    botAvatars,
    setAvatarEditorTarget,
    setModelPickerOpen,
    activeModel,
    activeProvider,
    refreshCapabilities,
    showToast,
  } = useApp();

  const [tab, setTab] = useState<"general" | "connections" | "keys" | "profiles" | "config" | "logs">("general");

  const [ssh, setSsh] = useState<HermesSshTarget>(connection.ssh ?? { host: "" });
  const [savingSsh, setSavingSsh] = useState(false);
  const [sshError, setSshError] = useState("");
  useEffect(() => { setSsh(connection.ssh ?? { host: "" }); }, [connection.ssh]);
  const saveSsh = async (remove = false) => {
    setSavingSsh(true); setSshError("");
    try {
      await updateConnection({ ...connection, ssh: remove ? undefined : normalizeSshTarget(ssh) });
      showToast(remove ? "SSH access removed" : "SSH access saved. Open Terminal to connect.");
    } catch (error) { setSshError(error instanceof Error ? error.message : String(error)); }
    finally { setSavingSsh(false); }
  };

  // Env keys
  const [envEntries, setEnvEntries] = useState<HermesEnvEntry[]>([]);
  const [newKey, setNewKey] = useState("");
  const [newVal, setNewVal] = useState("");

  // Raw config.yaml
  const [rawConfig, setRawConfig] = useState("");
  const [newProfileName, setNewProfileName] = useState("");

  // Logs & diagnostics
  const [serverLogs, setServerLogs] = useState<string[]>([]);
  const [diagOutput, setDiagOutput] = useState("");

  useEffect(() => {
    if (!client) return;
    if (tab === "keys") {
      client.listEnv().then(setEnvEntries).catch(() => undefined);
    } else if (tab === "config") {
      client.getRawConfig().then(setRawConfig).catch(() => undefined);
    } else if (tab === "logs") {
      client
        .getLogs(150)
        .then((rows) => setServerLogs(rows.map((r) => `${r.timestamp ? `[${r.timestamp}] ` : ""}${r.message}`)))
        .catch(() => undefined);
    }
  }, [client, tab]);

  const handleSaveEnv = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!client || !newKey.trim()) return;
    await client.setEnv(newKey.trim(), newVal);
    setNewKey("");
    setNewVal("");
    const refreshed = await client.listEnv().catch(() => []);
    setEnvEntries(refreshed);
    showToast("Saved API key / env variable");
  };

  const handleCreateProfile = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!client || !newProfileName.trim()) return;
    await client.createProfile({ name: newProfileName.trim() });
    setNewProfileName("");
    await refreshCapabilities();
    showToast("Created Hermes profile");
  };

  return (
    <div className="gb-panel">
      <div className="gb-panel__header">
        <div>
          <h2 className="gb-panel__title">Settings</h2>
          <div className="gb-panel__sub">Appearance, connections, profiles, and preferences</div>
        </div>
        <div className="gb-segmented">
          {(
            [
              ["general", "Appearance"],
              ["connections", "Connections"],
              ["keys", "API Keys"],
              ["profiles", "Profiles"],
              ["config", "Config"],
              ["logs", "Logs"],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              className="gb-segmented__item"
              data-active={tab === id ? "true" : undefined}
              onClick={() => setTab(id)}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      <div className="gb-panel__body">
        <div className="gb-panel__container">
          {tab === "general" && (
            <>
              <div className="gb-section-card">
                <div className="gb-section-card__head">
                  <span className="gb-section-card__title">Appearance</span>
                  
                </div>
                <div className="gb-row">
                  <div className="gb-row__info">
                    <div className="gb-row__label">Color Theme</div>
                    <div className="gb-row__desc">Choose a light or dark appearance</div>
                  </div>
                  <div className="gb-segmented">
                    <button
                      type="button"
                      className="gb-segmented__item"
                      data-active={theme === "light" ? "true" : undefined}
                      onClick={() => setTheme("light")}
                    >
                      Sand Light
                    </button>
                    <button
                      type="button"
                      className="gb-segmented__item"
                      data-active={theme === "dark" ? "true" : undefined}
                      onClick={() => setTheme("dark")}
                    >
                      Sand Dark
                    </button>
                  </div>
                </div>
                <div className="gb-row">
                  <div className="gb-row__info">
                    <div className="gb-row__label">Active Model & Provider</div>
                    <div className="gb-row__desc">
                      {activeModel} ({activeProvider})
                    </div>
                  </div>
                  <button type="button" className="gb-btn gb-btn--secondary" onClick={() => setModelPickerOpen(true)}>
                    Change Model
                  </button>
                </div>
              </div>

              <div className="gb-section-card">
                <div className="gb-section-card__head">
                  <span className="gb-section-card__title">Desktop runtime</span>
                  <span className="gb-badge gb-badge--success">{isDesktop ? "Rust + Tauri v2" : "Web Mode"}</span>
                </div>
                <div className="gb-row">
                  <div className="gb-row__info">
                    <div className="gb-row__label">Local history</div>
                    <div className="gb-row__desc">Time to load your saved conversations</div>
                  </div>
                  <span className="gb-badge gb-badge--info">{bootstrapMs !== null ? `${bootstrapMs}ms` : "N/A"}</span>
                </div>
                {systemStats && (
                  <div className="gb-row">
                    <div className="gb-row__info">
                      <div className="gb-row__label">{systemStats.os_name}</div>
                      <div className="gb-row__desc">
                        Host: {systemStats.hostname} · Load: {systemStats.load_avg}
                      </div>
                    </div>
                    <span className="gb-badge gb-badge--muted">
                      {systemStats.memory_used_mb} / {systemStats.memory_total_mb} MB
                    </span>
                  </div>
                )}
              </div>
            </>
          )}

          {tab === "connections" && <div className="gb-section-card">
            <div className="gb-section-card__head"><span className="gb-section-card__title">Your Hermes connection</span></div>
            <div className="gb-section-card__body" style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              <div className="gb-row"><CursorIcon name={isLocalConnection ? "laptop" : "server"} size={24} /><div className="gb-row__info"><div className="gb-row__label">{connection.name}</div><div className="gb-row__desc">{isLocalConnection ? "Hermes runs on this computer" : connection.url}</div></div><span className="gb-badge">{connectionStatus}</span></div>
              <p style={{ color: "var(--grok-bot-text-secondary)", fontSize: 13, margin: 0 }}>{connectionDetail}</p>
              <button className="gb-btn gb-btn--primary" onClick={() => setSetupOpen(true)}>Manage connection <CursorIcon name="arrow-right" size={14} /></button>
              {isDesktop && !isLocalConnection && <form onSubmit={event => { event.preventDefault(); void saveSsh(); }}>
                <h3 style={{ fontSize: 14, fontWeight: 500, margin: "8px 0" }}>SSH workspace access</h3>
                <fieldset disabled={savingSsh} style={{ border: 0, padding: 0, margin: 0 }}><SshFields value={ssh} onChange={setSsh} />
                  <div style={{ display: "flex", gap: 10, marginTop: 14 }}><button type="submit" className="gb-btn gb-btn--secondary gb-btn--sm">{savingSsh ? "Saving…" : "Save SSH access"}</button>{connection.ssh && <button type="button" className="gb-btn gb-btn--ghost gb-btn--sm" onClick={() => void saveSsh(true)}>Remove SSH access</button>}</div>
                </fieldset>
                {sshError && <p style={{ color: "var(--grok-bot-text-danger)", fontSize: 12 }} role="alert">{sshError}</p>}
              </form>}
              {isDesktop && isLocalConnection && <button className="gb-btn gb-btn--secondary" onClick={() => void restartLocalBackend()}><CursorIcon name="refresh" size={14} /> Restart local Hermes</button>}
            </div>
          </div>}

          {tab === "keys" && (
            <>
              <div className="gb-section-card">
                <div className="gb-section-card__head">
                  <span className="gb-section-card__title">Add or Update Provider Key (~/.hermes/.env)</span>
                </div>
                <form className="gb-section-card__body" onSubmit={handleSaveEnv} style={{ display: "flex", gap: 8 }}>
                  <input
                    className="gb-input gb-input--mono"
                    placeholder="e.g. OPENROUTER_API_KEY, ANTHROPIC_API_KEY, XAI_API_KEY"
                    value={newKey}
                    onChange={(e) => setNewKey(e.target.value)}
                    required
                  />
                  <input
                    className="gb-input gb-input--mono"
                    type="password"
                    placeholder="Secret value..."
                    value={newVal}
                    onChange={(e) => setNewVal(e.target.value)}
                    required
                  />
                  <button type="submit" className="gb-btn gb-btn--primary">Save</button>
                </form>
              </div>

              <div className="gb-section-card">
                <div className="gb-section-card__head">
                  <span className="gb-section-card__title">Configured Environment Variables ({envEntries.length})</span>
                </div>
                {envEntries.length === 0 ? (
                  <div className="gb-row">
                    <span className="gb-row__desc">Connect to local Hermes backend to manage ~/.hermes/.env keys.</span>
                  </div>
                ) : (
                  envEntries.map((entry) => (
                    <div key={entry.key} className="gb-row">
                      <div className="gb-row__info">
                        <div className="gb-row__label" style={{ fontFamily: "var(--cursor-font-family-mono)", fontSize: 12.5 }}>
                          {entry.key}
                        </div>
                        <div className="gb-row__desc">{entry.masked || (entry.is_set ? "••••••••" : "Not set")}</div>
                      </div>
                      <button
                        type="button"
                        className="gb-btn gb-btn--danger gb-btn--sm"
                        onClick={async () => {
                          if (!client) return;
                          await client.deleteEnv(entry.key);
                          setEnvEntries((prev) => prev.filter((e) => e.key !== entry.key));
                          showToast(`Removed ${entry.key}`);
                        }}
                      >
                        Remove
                      </button>
                    </div>
                  ))
                )}
              </div>
            </>
          )}

          {tab === "profiles" && (
            <>
              <form onSubmit={handleCreateProfile} style={{ display: "flex", gap: 8 }}>
                <input
                  className="gb-input"
                  placeholder="New Hermes profile name (e.g. coder, researcher, ops)..."
                  value={newProfileName}
                  onChange={(e) => setNewProfileName(e.target.value)}
                />
                <button type="submit" className="gb-btn gb-btn--primary" disabled={!newProfileName.trim()}>
                  Create Profile
                </button>
              </form>

              <div className="gb-section-card">
                {profiles.map((p) => {
                  const av = botAvatars[p.name];
                  return (
                    <div key={p.name} className="gb-row">
                      <div style={{ display: "flex", alignItems: "center", gap: 12, flex: 1 }}>
                        <BotMark seed={p.name} shapeId={av?.shapeId} colorId={av?.colorId} size={34} />
                        <div className="gb-row__info">
                          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                            <span className="gb-row__label">{av?.displayName || p.name}</span>
                            {activeProfile === p.name && <span className="gb-badge gb-badge--success">Active</span>}
                          </div>
                          <div className="gb-row__desc">{p.description || p.model || "Hermes profile"}</div>
                        </div>
                      </div>
                      <div style={{ display: "flex", gap: 6 }}>
                        <button
                          type="button"
                          className="gb-btn gb-btn--secondary gb-btn--sm"
                          onClick={() => setAvatarEditorTarget({ seed: p.name, title: av?.displayName || p.name })}
                        >
                          Avatar
                        </button>
                        {activeProfile !== p.name && (
                          <button
                            type="button"
                            className="gb-btn gb-btn--primary gb-btn--sm"
                            onClick={() => void setActiveProfile(p.name)}
                          >
                            Switch
                          </button>
                        )}
                      </div>
                    </div>
                  );
                })}
              </div>
            </>
          )}

          {tab === "config" && (
            <div className="gb-section-card">
              <div className="gb-section-card__head">
                <span className="gb-section-card__title">~/.hermes/config.yaml</span>
                <button
                  type="button"
                  className="gb-btn gb-btn--primary gb-btn--sm"
                  onClick={async () => {
                    if (!client) return;
                    await client.saveRawConfig(rawConfig);
                    showToast("Saved config.yaml");
                  }}
                >
                  Save YAML
                </button>
              </div>
              <div className="gb-section-card__body">
                <textarea
                  className="gb-textarea gb-textarea--mono"
                  style={{ minHeight: 380 }}
                  value={rawConfig}
                  onChange={(e) => setRawConfig(e.target.value)}
                />
              </div>
            </div>
          )}

          {tab === "logs" && (
            <>
              <div style={{ display: "flex", gap: 8 }}>
                <button
                  type="button"
                  className="gb-btn gb-btn--secondary"
                  onClick={async () => {
                    if (!client) return;
                    const res = await client.runDoctor().catch((e) => ({ error: String(e) }));
                    setDiagOutput(JSON.stringify(res, null, 2));
                  }}
                >
                  Run Hermes Doctor
                </button>
                <button
                  type="button"
                  className="gb-btn gb-btn--secondary"
                  onClick={async () => {
                    if (!client) return;
                    const res = await client.runSecurityAudit().catch((e) => ({ error: String(e) }));
                    setDiagOutput(JSON.stringify(res, null, 2));
                  }}
                >
                  Security Audit
                </button>
              </div>

              {diagOutput && (
                <pre className="gb-term-output" style={{ maxHeight: 220, margin: 0 }}>
                  {diagOutput}
                </pre>
              )}

              <div className="gb-section-card">
                <div className="gb-section-card__head">
                  <span className="gb-section-card__title">Rust Supervisor & Hermes Backend Logs</span>
                </div>
                <div className="gb-section-card__body">
                  <pre className="gb-term-output" style={{ maxHeight: 360, margin: 0 }}>
                    {[...(rustBackend?.logs ?? []), ...serverLogs].join("\n") || "No logs captured yet."}
                  </pre>
                </div>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
};
