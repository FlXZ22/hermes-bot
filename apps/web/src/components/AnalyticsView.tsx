import React, { useMemo } from "react";
import { CursorIcon, DATAVIZ_PALETTE } from "@grok-bot/theme";
import { useApp } from "../state/appStore";
import { formatTokens, shortModelName } from "../lib/format";

export const AnalyticsView: React.FC = () => {
  const { usagePoints, sessions, tasks, boards, theme } = useApp();
  const palette = theme === "dark" ? DATAVIZ_PALETTE.dark : DATAVIZ_PALETTE.light;

  const totals = useMemo(() => {
    const tokens = usagePoints.reduce((acc, p) => acc + (p.tokens ?? (p.input_tokens ?? 0) + (p.output_tokens ?? 0)), 0)
      || sessions.reduce((acc, s) => acc + (s.tokens_used ?? 0), 0);
    const cost = usagePoints.reduce((acc, p) => acc + (p.cost_usd ?? 0), 0);
    const requests = usagePoints.reduce((acc, p) => acc + (p.requests ?? 0), 0) || sessions.reduce((acc, s) => acc + (s.message_count ?? 0), 0);
    return { tokens, cost, requests };
  }, [sessions, usagePoints]);

  const modelBreakdown = useMemo(() => {
    const map = new Map<string, number>();
    for (const s of sessions) {
      const m = shortModelName(s.model || "unknown");
      map.set(m, (map.get(m) ?? 0) + Math.max(1, s.message_count ?? 1));
    }
    return Array.from(map.entries()).sort((a, b) => b[1] - a[1]).slice(0, 8);
  }, [sessions]);

  const maxModelVal = Math.max(1, ...modelBreakdown.map(([, v]) => v));
  const maxDayTokens = Math.max(
    1,
    ...usagePoints.map((p) => p.tokens ?? (p.input_tokens ?? 0) + (p.output_tokens ?? 0))
  );

  return (
    <div className="gb-panel">
      <div className="gb-panel__header">
        <div>
          <h2 className="gb-panel__title">Usage & Analytics</h2>
          <div className="gb-panel__sub">Token consumption, model distribution, and workspace activity</div>
        </div>
      </div>

      <div className="gb-panel__body">
        <div className="gb-panel__container">
          <div className="gb-stats-grid">
            <div className="gb-stat-card">
              <div className="gb-stat-card__label">Total Tokens</div>
              <div className="gb-stat-card__value">{formatTokens(totals.tokens)}</div>
            </div>
            <div className="gb-stat-card">
              <div className="gb-stat-card__label">Estimated Spend</div>
              <div className="gb-stat-card__value">${totals.cost.toFixed(2)}</div>
            </div>
            <div className="gb-stat-card">
              <div className="gb-stat-card__label">Sessions</div>
              <div className="gb-stat-card__value">{sessions.length}</div>
            </div>
            <div className="gb-stat-card">
              <div className="gb-stat-card__label">Board Tasks</div>
              <div className="gb-stat-card__value">
                {tasks.length} <span style={{ fontSize: 13, fontWeight: 400, color: "var(--grok-bot-text-tertiary)" }}>/ {boards.length} boards</span>
              </div>
            </div>
          </div>

          <div className="gb-section-card">
            <div className="gb-section-card__head">
              <span className="gb-section-card__title">Daily Token Activity (14 Days)</span>
            </div>
            <div className="gb-section-card__body">
              {usagePoints.length === 0 ? (
                <div className="gb-empty" style={{ padding: 24 }}>
                  <CursorIcon name="chart-bars" size={24} />
                  <div className="gb-empty__sub">Token timeline populates automatically from your Hermes sessions.</div>
                </div>
              ) : (
                <div style={{ display: "flex", alignItems: "flex-end", gap: 6, height: 150, paddingTop: 12 }}>
                  {usagePoints.map((pt, idx) => {
                    const val = pt.tokens ?? (pt.input_tokens ?? 0) + (pt.output_tokens ?? 0);
                    const pct = Math.max(6, Math.round((val / maxDayTokens) * 100));
                    return (
                      <div
                        key={pt.date}
                        style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", gap: 6, height: "100%", justifyContent: "flex-end" }}
                        title={`${pt.date}: ${formatTokens(val)} tokens ($${(pt.cost_usd ?? 0).toFixed(3)})`}
                      >
                        <div
                          style={{
                            width: "100%",
                            height: `${pct}%`,
                            borderRadius: "6px 6px 3px 3px",
                            background: palette[idx % palette.length],
                            opacity: 0.9,
                          }}
                        />
                        <span style={{ fontSize: 10, color: "var(--grok-bot-text-tertiary)" }}>
                          {pt.date.slice(5)}
                        </span>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          </div>

          <div className="gb-section-card">
            <div className="gb-section-card__head">
              <span className="gb-section-card__title">Sessions by model</span>
            </div>
            <div className="gb-section-card__body" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
              {modelBreakdown.length === 0 ? (
                <span style={{ fontSize: 13, color: "var(--grok-bot-text-tertiary)" }}>No model usage recorded yet.</span>
              ) : (
                modelBreakdown.map(([modelName, count], idx) => (
                  <div key={modelName} style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12.5 }}>
                      <span style={{ fontWeight: 500 }}>{modelName}</span>
                      <span style={{ color: "var(--grok-bot-text-tertiary)" }}>{count} msgs</span>
                    </div>
                    <div style={{ height: 7, borderRadius: 9999, background: "var(--grok-bot-bg-tertiary)", overflow: "hidden" }}>
                      <div
                        style={{
                          width: `${Math.max(6, Math.round((count / maxModelVal) * 100))}%`,
                          height: "100%",
                          borderRadius: 9999,
                          background: palette[idx % palette.length],
                        }}
                      />
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
