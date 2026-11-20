import React, { useState } from "react";
import { CursorIcon, Modal } from "@grok-bot/theme";
import { useApp } from "../state/appStore";

export const GatewayView: React.FC = () => {
  const { hermesStatus, messagingPlatforms, webhooks, client, refreshCapabilities, showToast } = useApp();
  const [hookModal, setHookModal] = useState(false);
  const [hookName, setHookName] = useState("");
  const [hookUrl, setHookUrl] = useState("");

  const handleGateway = async (action: "start" | "stop" | "restart") => {
    if (!client) return;
    try {
      await client.gatewayAction(action);
      await refreshCapabilities();
      showToast(`Gateway ${action}ed`);
    } catch (err) {
      showToast(err instanceof Error ? err.message : `Gateway ${action} failed`);
    }
  };

  const handleAddWebhook = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!client || !hookUrl.trim()) return;
    try {
      await client.createWebhook({ name: hookName.trim() || "Webhook", url: hookUrl.trim(), enabled: true });
      setHookModal(false);
      setHookName("");
      setHookUrl("");
      await refreshCapabilities();
      showToast("Webhook created");
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Failed creating webhook");
    }
  };

  return (
    <div className="gb-panel">
      <div className="gb-panel__header">
        <div>
          <h2 className="gb-panel__title">Gateway, Messaging & Webhooks</h2>
          <div className="gb-panel__sub">
            Connect Hermes across Telegram, Discord, Slack, WhatsApp, Signal, and custom Webhooks
          </div>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <button type="button" className="gb-btn gb-btn--secondary" onClick={() => void handleGateway("restart")}>
            <CursorIcon name="refresh" size={13} />
            Restart
          </button>
          <button
            type="button"
            className="gb-btn gb-btn--primary"
            onClick={() => void handleGateway(hermesStatus?.gateway_running ? "stop" : "start")}
          >
            <CursorIcon name={hermesStatus?.gateway_running ? "stop" : "play"} size={13} />
            {hermesStatus?.gateway_running ? "Stop Gateway" : "Start Gateway"}
          </button>
        </div>
      </div>

      <div className="gb-panel__body">
        <div className="gb-panel__container">
          <div className="gb-stats-grid">
            <div className="gb-stat-card">
              <div className="gb-stat-card__label">Gateway State</div>
              <div className="gb-stat-card__value">
                {hermesStatus?.gateway_running ? "Running" : hermesStatus?.gateway_state || "Stopped"}
              </div>
            </div>
            <div className="gb-stat-card">
              <div className="gb-stat-card__label">Platforms</div>
              <div className="gb-stat-card__value">{messagingPlatforms.length}</div>
            </div>
            <div className="gb-stat-card">
              <div className="gb-stat-card__label">Webhooks</div>
              <div className="gb-stat-card__value">{webhooks.length}</div>
            </div>
          </div>

          <div className="gb-section-card">
            <div className="gb-section-card__head">
              <span className="gb-section-card__title">Messaging Platforms</span>
            </div>
            {messagingPlatforms.length === 0 ? (
              <div className="gb-empty">
                <CursorIcon name="globe" size={26} />
                <div className="gb-empty__title">No messaging platforms configured</div>
                <div className="gb-empty__sub">Configure Telegram, Discord, Slack, or WhatsApp tokens in Settings → Keys.</div>
              </div>
            ) : (
              messagingPlatforms.map((plat) => (
                <div key={plat.id} className="gb-row">
                  <div className="gb-row__info">
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <span className="gb-row__label">{plat.name || plat.id}</span>
                      <span className={`gb-badge ${plat.connected ? "gb-badge--success" : plat.configured ? "gb-badge--info" : "gb-badge--muted"}`}>
                        {plat.connected ? "Connected" : plat.configured ? "Configured" : "Not configured"}
                      </span>
                    </div>
                    {plat.status && <div className="gb-row__desc">{plat.status}</div>}
                  </div>
                  <button
                    type="button"
                    className="gb-btn gb-btn--secondary gb-btn--sm"
                    onClick={async () => {
                      if (!client) return;
                      try {
                        await client.testMessagingPlatform(plat.id);
                        showToast(`${plat.name || plat.id} test passed`);
                      } catch (err) {
                        showToast(err instanceof Error ? err.message : "Test failed");
                      }
                    }}
                  >
                    Test
                  </button>
                </div>
              ))
            )}
          </div>

          <div className="gb-section-card">
            <div className="gb-section-card__head">
              <span className="gb-section-card__title">Dynamic Webhooks</span>
              <button type="button" className="gb-btn gb-btn--primary gb-btn--sm" onClick={() => setHookModal(true)}>
                <CursorIcon name="plus" size={12} />
                Add Webhook
              </button>
            </div>
            {webhooks.length === 0 ? (
              <div className="gb-row">
                <span className="gb-row__desc">No webhooks registered yet.</span>
              </div>
            ) : (
              webhooks.map((wh) => (
                <div key={wh.id} className="gb-row">
                  <div className="gb-row__info">
                    <div className="gb-row__label">{wh.name || wh.id}</div>
                    <div className="gb-row__desc" style={{ fontFamily: "var(--cursor-font-family-mono)" }}>{wh.url}</div>
                  </div>
                  <button
                    type="button"
                    className="gb-btn gb-btn--danger gb-btn--sm"
                    onClick={async () => {
                      if (!client) return;
                      await client.deleteWebhook(wh.id);
                      await refreshCapabilities();
                    }}
                  >
                    <CursorIcon name="trash" size={12} />
                  </button>
                </div>
              ))
            )}
          </div>
        </div>
      </div>

      <Modal open={hookModal} title="Add Webhook" onClose={() => setHookModal(false)}>
        <form onSubmit={handleAddWebhook} style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div className="gb-field">
            <label className="gb-field__label">Name</label>
            <input className="gb-input" placeholder="e.g. Deploy Notify" value={hookName} onChange={(e) => setHookName(e.target.value)} />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Target URL</label>
            <input className="gb-input gb-input--mono" placeholder="https://..." value={hookUrl} onChange={(e) => setHookUrl(e.target.value)} required />
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button type="button" className="gb-btn gb-btn--ghost" onClick={() => setHookModal(false)}>Cancel</button>
            <button type="submit" className="gb-btn gb-btn--primary">Save Webhook</button>
          </div>
        </form>
      </Modal>
    </div>
  );
};
