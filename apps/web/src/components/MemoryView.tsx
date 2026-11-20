import React, { useEffect, useState } from "react";
import { CursorIcon } from "@grok-bot/theme";
import type { HermesLearningGraph } from "@grok-bot/hermes-client";
import { useApp } from "../state/appStore";

export const MemoryView: React.FC = () => {
  const { soulMd, memoryMd, userMd, saveMemoryFile, client, showToast, isDesktop, isLocalConnection } = useApp();
  const [tab, setTab] = useState<"memory" | "user" | "soul" | "graph">("memory");
  const [memDraft, setMemDraft] = useState(memoryMd);
  const [userDraft, setUserDraft] = useState(userMd);
  const [soulDraft, setSoulDraft] = useState(soulMd);
  const [graph, setGraph] = useState<HermesLearningGraph | null>(null);

  useEffect(() => setMemDraft(memoryMd), [memoryMd]);
  useEffect(() => setUserDraft(userMd), [userMd]);
  useEffect(() => setSoulDraft(soulMd), [soulMd]);

  useEffect(() => {
    if (tab === "graph" && client) {
      client.getLearningGraph().then(setGraph).catch(() => undefined);
    }
  }, [client, tab]);

  return (
    <div className="gb-panel">
      <div className="gb-panel__header">
        <div>
          <h2 className="gb-panel__title">Memory & Knowledge</h2>
          <div className="gb-panel__sub">Persistent Hermes memory, user profile, SOUL.md persona, and learning graph</div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <div className="gb-segmented">
            <button type="button" className="gb-segmented__item" data-active={tab === "memory" ? "true" : undefined} onClick={() => setTab("memory")}>MEMORY.md</button>
            <button type="button" className="gb-segmented__item" data-active={tab === "user" ? "true" : undefined} onClick={() => setTab("user")}>USER.md</button>
            <button type="button" className="gb-segmented__item" data-active={tab === "soul" ? "true" : undefined} onClick={() => setTab("soul")}>SOUL.md</button>
            <button type="button" className="gb-segmented__item" data-active={tab === "graph" ? "true" : undefined} onClick={() => setTab("graph")}>Learning Graph</button>
          </div>
          {client && (
            <button
              type="button"
              className="gb-btn gb-btn--secondary"
              onClick={async () => {
                await client.runCurator().catch(() => undefined);
                showToast("Triggered Hermes memory curator");
              }}
            >
              <CursorIcon name="sparkles" size={13} />
              Run Curator
            </button>
          )}
        </div>
      </div>

      <div className="gb-panel__body">
        <div className="gb-panel__container">
          {tab === "memory" && (
            <div className="gb-section-card">
              <div className="gb-section-card__head">
                <span className="gb-section-card__title">~/.hermes/memories/MEMORY.md</span>
                <button type="button" className="gb-btn gb-btn--primary gb-btn--sm" disabled={!isDesktop || !isLocalConnection} title={!isLocalConnection ? "Edit this file on your Hermes server" : undefined} onClick={() => void saveMemoryFile("memory", memDraft)}>
                  Save MEMORY.md
                </button>
              </div>
              <div className="gb-section-card__body">
                <textarea
                  className="gb-textarea gb-textarea--mono"
                  style={{ minHeight: 360 }}
                  readOnly={!isDesktop || !isLocalConnection}
                  value={memDraft}
                  onChange={(e) => setMemDraft(e.target.value)}
                  placeholder={!isDesktop || !isLocalConnection ? "MEMORY.md is managed on your Hermes server. You can edit the agent’s SOUL.md in the SOUL tab." : "Long-term curated agent memories..."}
                />
              </div>
            </div>
          )}

          {tab === "user" && (
            <div className="gb-section-card">
              <div className="gb-section-card__head">
                <span className="gb-section-card__title">~/.hermes/memories/USER.md</span>
                <button type="button" className="gb-btn gb-btn--primary gb-btn--sm" disabled={!isDesktop || !isLocalConnection} title={!isLocalConnection ? "Edit this file on your Hermes server" : undefined} onClick={() => void saveMemoryFile("user", userDraft)}>
                  Save USER.md
                </button>
              </div>
              <div className="gb-section-card__body">
                <textarea
                  className="gb-textarea gb-textarea--mono"
                  style={{ minHeight: 360 }}
                  readOnly={!isDesktop || !isLocalConnection}
                  value={userDraft}
                  onChange={(e) => setUserDraft(e.target.value)}
                  placeholder={!isDesktop || !isLocalConnection ? "USER.md is managed on your Hermes server. You can edit the agent’s SOUL.md in the SOUL tab." : "Your preferences, tools, OS (Arch Linux / Omarchy), coding style..."}
                />
              </div>
            </div>
          )}

          {tab === "soul" && (
            <div className="gb-section-card">
              <div className="gb-section-card__head">
                <span className="gb-section-card__title">~/.hermes/SOUL.md</span>
                <button type="button" className="gb-btn gb-btn--primary gb-btn--sm" onClick={() => void saveMemoryFile("soul", soulDraft)}>
                  Save SOUL.md
                </button>
              </div>
              <div className="gb-section-card__body">
                <textarea
                  className="gb-textarea gb-textarea--mono"
                  style={{ minHeight: 360 }}
                  value={soulDraft}
                  onChange={(e) => setSoulDraft(e.target.value)}
                />
              </div>
            </div>
          )}

          {tab === "graph" && (
            <div className="gb-section-card">
              <div className="gb-section-card__head">
                <span className="gb-section-card__title">
                  Learned Skills & Memory Nodes ({graph?.nodes.length ?? 0})
                </span>
              </div>
              {!graph || graph.nodes.length === 0 ? (
                <div className="gb-empty">
                  <CursorIcon name="brain" size={28} />
                  <div className="gb-empty__title">Learning graph is building</div>
                  <div className="gb-empty__sub">As you work with Hermes, learned skills and memories appear here.</div>
                </div>
              ) : (
                graph.nodes.map((node) => (
                  <div key={node.id} className="gb-row">
                    <div className="gb-row__info">
                      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                        <span className="gb-row__label">{node.label}</span>
                        <span className="gb-badge gb-badge--accent">{node.kind}</span>
                      </div>
                      {node.summary && <div className="gb-row__desc">{node.summary}</div>}
                    </div>
                  </div>
                ))
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
};
