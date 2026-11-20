import React, { useMemo, useState } from "react";
import { CursorIcon, Modal, ToggleSwitch } from "@grok-bot/theme";
import { useApp, type ReasoningEffort } from "../state/appStore";
import { formatTokens } from "../lib/format";

const EFFORTS: ReasoningEffort[] = ["none", "low", "medium", "high", "xhigh"];

export const ModelPickerModal: React.FC = () => {
  const {
    modelPickerOpen,
    setModelPickerOpen,
    models,
    activeModel,
    activeProvider,
    reasoningEffort,
    fastMode,
    yoloMode,
    selectModel,
    setReasoningEffort,
    setFastMode,
    setYoloMode,
  } = useApp();

  const [query, setQuery] = useState("");
  const [providerFilter, setProviderFilter] = useState<string>("all");
  const [customModel, setCustomModel] = useState("");
  const [saveDefault, setSaveDefault] = useState(false);

  const providers = useMemo(() => {
    const set = new Set<string>();
    for (const m of models) {
      if (m.provider) set.add(m.provider);
    }
    return ["all", ...Array.from(set).sort()];
  }, [models]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return models.filter((m) => {
      if (providerFilter !== "all" && m.provider !== providerFilter) return false;
      if (!q) return true;
      return `${m.id} ${m.name ?? ""} ${m.provider}`.toLowerCase().includes(q);
    });
  }, [models, providerFilter, query]);

  const handleChoose = async (modelId: string, provider?: string) => {
    try {
      await selectModel(modelId, provider, saveDefault);
      setModelPickerOpen(false);
    } catch { /* The store reports the gateway error. */ }
  };

  return (
    <Modal open={modelPickerOpen} title="Choose Hermes Model" onClose={() => setModelPickerOpen(false)}>
      <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
        {/* Search bar */}
        <div className="gb-sidebar-search" style={{ margin: 0, height: 36 }}>
          <CursorIcon name="search" size={14} />
          <input
            className="gb-sidebar-search__input"
            placeholder="Search models or providers (e.g. claude, grok, gemini, hermes)..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            autoFocus
          />
          {query && (
            <button type="button" className="gb-mini-icon-btn" onClick={() => setQuery("")}>
              <CursorIcon name="close" size={12} />
            </button>
          )}
        </div>

        {/* Provider filter chips */}
        <div style={{ display: "flex", gap: 5, flexWrap: "wrap" }}>
          {providers.slice(0, 10).map((p) => (
            <button
              key={p}
              type="button"
              className="gb-board-pill"
              data-active={providerFilter === p ? "true" : undefined}
              onClick={() => setProviderFilter(p)}
            >
              {p === "all" ? "All Providers" : p}
            </button>
          ))}
        </div>

        {/* Model catalog list */}
        <div className="gb-model-list">
          {filtered.map((m) => {
            const isSelected = m.id === activeModel;
            return (
              <button
                key={`${m.provider}:${m.id}`}
                type="button"
                className="gb-model-row"
                data-active={isSelected ? "true" : undefined}
                onClick={() => void handleChoose(m.id, m.provider)}
              >
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 7 }}>
                    <span style={{ fontSize: 13.5, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                      {m.name || m.id}
                    </span>
                    <span className="gb-badge gb-badge--muted">{m.provider}</span>
                    {m.configured ? (
                      <span className="gb-badge gb-badge--success">Ready</span>
                    ) : null}
                  </div>
                  <div
                    style={{
                      fontSize: 11.5,
                      color: "var(--grok-bot-text-tertiary)",
                      fontFamily: "var(--cursor-font-family-mono)",
                      marginTop: 2,
                    }}
                  >
                    {m.id}
                    {m.context_length ? ` · ${formatTokens(m.context_length)} ctx` : ""}
                  </div>
                </div>
                {isSelected && <CursorIcon name="check-circle" size={16} style={{ color: "var(--sand-accent-text)" }} />}
              </button>
            );
          })}
          {filtered.length === 0 && (
            <div style={{ padding: 20, textAlign: "center", fontSize: 13, color: "var(--grok-bot-text-tertiary)" }}>
              No matching models. Enter a custom model ID below.
            </div>
          )}
        </div>

        {/* Custom model ID entry */}
        <form
          style={{ display: "flex", gap: 8 }}
          onSubmit={(e) => {
            e.preventDefault();
            if (customModel.trim()) {
              void handleChoose(customModel.trim(), activeProvider);
              setCustomModel("");
            }
          }}
        >
          <input
            className="gb-input gb-input--mono"
            placeholder="Custom model ID (e.g. openrouter/anthropic/claude-opus-4.6)"
            value={customModel}
            onChange={(e) => setCustomModel(e.target.value)}
          />
          <button type="submit" className="gb-btn gb-btn--secondary" disabled={!customModel.trim()}>
            Use
          </button>
        </form>

        {/* Reasoning Effort & Runtime Controls */}
        <div className="gb-section-card">
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label">Reasoning Effort</div>
              <div className="gb-row__desc">Controls extended thinking depth per turn</div>
            </div>
            <div className="gb-segmented">
              {EFFORTS.map((ef) => (
                <button
                  key={ef}
                  type="button"
                  className="gb-segmented__item"
                  data-active={reasoningEffort === ef ? "true" : undefined}
                  onClick={() => setReasoningEffort(ef)}
                >
                  {ef}
                </button>
              ))}
            </div>
          </div>
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label">Fast / Priority Tier</div>
              <div className="gb-row__desc">Prefer low-latency priority endpoints when supported</div>
            </div>
            <ToggleSwitch checked={fastMode} onChange={setFastMode} />
          </div>
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label">YOLO Auto-Approve</div>
              <div className="gb-row__desc">Skip interactive approval prompts for shell & tool calls</div>
            </div>
            <ToggleSwitch checked={yoloMode} onChange={setYoloMode} />
          </div>
          <div className="gb-row">
            <div className="gb-row__info">
              <div className="gb-row__label">Save as Hermes Default</div>
              <div className="gb-row__desc">Persist selection to ~/.hermes/config.yaml</div>
            </div>
            <ToggleSwitch checked={saveDefault} onChange={setSaveDefault} />
          </div>
        </div>
      </div>
    </Modal>
  );
};
