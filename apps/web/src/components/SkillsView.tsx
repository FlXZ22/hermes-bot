import React, { useState } from "react";
import { CursorIcon, Modal, ToggleSwitch } from "@grok-bot/theme";
import { useApp } from "../state/appStore";

export const SkillsView: React.FC = () => {
  const { skills, toolsets, mcpServers, client, refreshCapabilities, showToast } = useApp();
  const [tab, setTab] = useState<"skills" | "hub" | "toolsets" | "mcp">("skills");
  const [search, setSearch] = useState("");
  const [hubQuery, setHubQuery] = useState("");
  const [hubResults, setHubResults] = useState<Array<Record<string, unknown>>>([]);
  const [hubLoading, setHubLoading] = useState(false);

  // Skill editor modal
  const [editingSkill, setEditingSkill] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState("");

  // Add MCP modal
  const [mcpModalOpen, setMcpModalOpen] = useState(false);
  const [mcpName, setMcpName] = useState("");
  const [mcpTransport, setMcpTransport] = useState("stdio");
  const [mcpCommand, setMcpCommand] = useState("");
  const [mcpArgs, setMcpArgs] = useState("");
  const [mcpUrl, setMcpUrl] = useState("");

  const filteredSkills = skills.filter((s) =>
    `${s.name} ${s.category ?? ""} ${s.description ?? ""}`.toLowerCase().includes(search.trim().toLowerCase())
  );

  const openSkillEditor = async (name: string) => {
    setEditingSkill(name);
    if (client) {
      const content = await client.getSkillContent(name).catch(() => "");
      setSkillContent(content);
    }
  };

  const handleSaveSkill = async () => {
    if (!editingSkill || !client) return;
    await client.saveSkillContent(editingSkill, skillContent);
    showToast(`Saved skill "${editingSkill}"`);
    setEditingSkill(null);
  };

  const handleHubSearch = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!client || !hubQuery.trim()) return;
    setHubLoading(true);
    try {
      const res = await client.searchSkillsHub(hubQuery.trim());
      setHubResults(res);
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Hub search failed");
    } finally {
      setHubLoading(false);
    }
  };

  const handleAddMcp = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!client || !mcpName.trim()) return;
    try {
      await client.addMcpServer({
        name: mcpName.trim(),
        transport: mcpTransport,
        command: mcpTransport === "stdio" ? mcpCommand.trim() : undefined,
        args: mcpTransport === "stdio" && mcpArgs.trim() ? mcpArgs.trim().split(/\s+/) : undefined,
        url: mcpTransport !== "stdio" ? mcpUrl.trim() : undefined,
        enabled: true,
      });
      setMcpModalOpen(false);
      setMcpName("");
      setMcpCommand("");
      setMcpArgs("");
      setMcpUrl("");
      await refreshCapabilities();
      showToast("Added MCP server");
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Failed to add MCP server");
    }
  };

  return (
    <div className="gb-panel">
      <div className="gb-panel__header">
        <div>
          <h2 className="gb-panel__title">Capabilities & MCP</h2>
          <div className="gb-panel__sub">
            {skills.length} skills · {toolsets.length} toolsets · {mcpServers.length} MCP servers
          </div>
        </div>

        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <div className="gb-segmented">
            <button type="button" className="gb-segmented__item" data-active={tab === "skills" ? "true" : undefined} onClick={() => setTab("skills")}>
              Skills
            </button>
            <button type="button" className="gb-segmented__item" data-active={tab === "hub" ? "true" : undefined} onClick={() => setTab("hub")}>
              Hub
            </button>
            <button type="button" className="gb-segmented__item" data-active={tab === "toolsets" ? "true" : undefined} onClick={() => setTab("toolsets")}>
              Toolsets
            </button>
            <button type="button" className="gb-segmented__item" data-active={tab === "mcp" ? "true" : undefined} onClick={() => setTab("mcp")}>
              MCP
            </button>
          </div>
          {tab === "mcp" && (
            <button type="button" className="gb-btn gb-btn--primary" onClick={() => setMcpModalOpen(true)}>
              <CursorIcon name="plus" size={13} />
              Add MCP
            </button>
          )}
        </div>
      </div>

      <div className="gb-panel__body">
        <div className="gb-panel__container">
          {tab === "skills" && (
            <>
              <input
                className="gb-input"
                placeholder="Filter installed Hermes skills..."
                value={search}
                onChange={(e) => setSearch(e.target.value)}
              />
              <div className="gb-section-card">
                {filteredSkills.length === 0 ? (
                  <div className="gb-empty">
                    <CursorIcon name="sparkles" size={26} />
                    <div className="gb-empty__title">No skills found</div>
                    <div className="gb-empty__sub">Install skills from the Hub or ask Hermes to write one for you.</div>
                  </div>
                ) : (
                  filteredSkills.map((sk) => (
                    <div key={sk.name} className="gb-row">
                      <div className="gb-row__info">
                        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                          <span className="gb-row__label">{sk.name}</span>
                          {sk.category && <span className="gb-badge gb-badge--muted">{sk.category}</span>}
                        </div>
                        <div className="gb-row__desc">{sk.description || sk.path}</div>
                      </div>
                      <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                        <button
                          type="button"
                          className="gb-btn gb-btn--secondary gb-btn--sm"
                          onClick={() => void openSkillEditor(sk.name)}
                        >
                          Edit
                        </button>
                        <ToggleSwitch
                          checked={sk.enabled}
                          onChange={async (enabled) => {
                            if (!client) return;
                            await client.toggleSkill(sk.name, enabled).catch(() => undefined);
                            await refreshCapabilities();
                          }}
                        />
                      </div>
                    </div>
                  ))
                )}
              </div>
            </>
          )}

          {tab === "hub" && (
            <>
              <form onSubmit={handleHubSearch} style={{ display: "flex", gap: 8 }}>
                <input
                  className="gb-input"
                  placeholder="Search Hermes Skills Hub (e.g. git, docker, arch, research)..."
                  value={hubQuery}
                  onChange={(e) => setHubQuery(e.target.value)}
                />
                <button type="submit" className="gb-btn gb-btn--primary" disabled={hubLoading || !hubQuery.trim()}>
                  {hubLoading ? "Searching..." : "Search Hub"}
                </button>
              </form>
              <div className="gb-section-card">
                {hubResults.length === 0 ? (
                  <div className="gb-empty">
                    <CursorIcon name="search" size={26} />
                    <div className="gb-empty__title">Discover Community Skills</div>
                    <div className="gb-empty__sub">Search Hermes Skills Hub to preview and install skills in one click.</div>
                  </div>
                ) : (
                  hubResults.map((item, idx) => {
                    const id = String(item.identifier ?? item.name ?? idx);
                    return (
                      <div key={id} className="gb-row">
                        <div className="gb-row__info">
                          <div className="gb-row__label">{String(item.name ?? id)}</div>
                          <div className="gb-row__desc">{String(item.description ?? "")}</div>
                        </div>
                        <button
                          type="button"
                          className="gb-btn gb-btn--primary gb-btn--sm"
                          onClick={async () => {
                            if (!client) return;
                            await client.installHubSkill(id);
                            await refreshCapabilities();
                            showToast(`Installed ${id}`);
                          }}
                        >
                          Install
                        </button>
                      </div>
                    );
                  })
                )}
              </div>
            </>
          )}

          {tab === "toolsets" && (
            <div className="gb-section-card">
              {toolsets.length === 0 ? (
                <div className="gb-empty">
                  <CursorIcon name="wrench" size={26} />
                  <div className="gb-empty__title">No toolsets loaded yet</div>
                  <div className="gb-empty__sub">Toolsets load automatically once local Hermes serve is connected.</div>
                </div>
              ) : (
                toolsets.map((ts) => (
                  <div key={ts.name} className="gb-row">
                    <div className="gb-row__info">
                      <div className="gb-row__label">{ts.name}</div>
                      <div className="gb-row__desc">
                        {ts.description || (ts.tools ? `${ts.tools.length} tools: ${ts.tools.slice(0, 6).join(", ")}` : "")}
                      </div>
                    </div>
                    <ToggleSwitch
                      checked={ts.enabled}
                      onChange={async (enabled) => {
                        if (!client) return;
                        await client.toggleToolset(ts.name, enabled).catch(() => undefined);
                        await refreshCapabilities();
                      }}
                    />
                  </div>
                ))
              )}
            </div>
          )}

          {tab === "mcp" && (
            <div className="gb-section-card">
              {mcpServers.length === 0 ? (
                <div className="gb-empty">
                  <CursorIcon name="plug" size={26} />
                  <div className="gb-empty__title">No MCP servers configured</div>
                  <div className="gb-empty__sub">Connect stdio or HTTP/SSE Model Context Protocol servers to extend Hermes.</div>
                  <button type="button" className="gb-btn gb-btn--primary" onClick={() => setMcpModalOpen(true)}>
                    Add MCP Server
                  </button>
                </div>
              ) : (
                mcpServers.map((srv) => (
                  <div key={srv.name} className="gb-row">
                    <div className="gb-row__info">
                      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                        <span className="gb-row__label">{srv.name}</span>
                        <span className="gb-badge gb-badge--muted">{srv.transport || (srv.url ? "http" : "stdio")}</span>
                        {srv.status && <span className="gb-badge gb-badge--info">{srv.status}</span>}
                      </div>
                      <div className="gb-row__desc" style={{ fontFamily: "var(--cursor-font-family-mono)" }}>
                        {srv.url || `${srv.command ?? ""} ${(srv.args ?? []).join(" ")}`}
                      </div>
                    </div>
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <button
                        type="button"
                        className="gb-btn gb-btn--secondary gb-btn--sm"
                        onClick={async () => {
                          if (!client) return;
                          try {
                            await client.testMcpServer(srv.name);
                            showToast(`MCP "${srv.name}" healthy`);
                          } catch (err) {
                            showToast(err instanceof Error ? err.message : "Test failed");
                          }
                        }}
                      >
                        Test
                      </button>
                      <ToggleSwitch
                        checked={srv.enabled !== false}
                        onChange={async (enabled) => {
                          if (!client) return;
                          await client.toggleMcpServer(srv.name, enabled).catch(() => undefined);
                          await refreshCapabilities();
                        }}
                      />
                      <button
                        type="button"
                        className="gb-btn gb-btn--danger gb-btn--sm"
                        onClick={async () => {
                          if (!client) return;
                          await client.deleteMcpServer(srv.name);
                          await refreshCapabilities();
                          showToast(`Removed MCP "${srv.name}"`);
                        }}
                      >
                        <CursorIcon name="trash" size={12} />
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          )}
        </div>
      </div>

      {/* Skill Content Editor Modal */}
      <Modal open={Boolean(editingSkill)} title={`SKILL.md · ${editingSkill ?? ""}`} onClose={() => setEditingSkill(null)}>
        <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <textarea
            className="gb-textarea gb-textarea--mono"
            style={{ minHeight: 300 }}
            value={skillContent}
            onChange={(e) => setSkillContent(e.target.value)}
          />
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button type="button" className="gb-btn gb-btn--ghost" onClick={() => setEditingSkill(null)}>
              Cancel
            </button>
            <button type="button" className="gb-btn gb-btn--primary" onClick={() => void handleSaveSkill()}>
              Save SKILL.md
            </button>
          </div>
        </div>
      </Modal>

      {/* Add MCP Server Modal */}
      <Modal open={mcpModalOpen} title="Add MCP Server" onClose={() => setMcpModalOpen(false)}>
        <form onSubmit={handleAddMcp} style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div className="gb-field">
            <label className="gb-field__label">Server Name</label>
            <input className="gb-input" placeholder="e.g. filesystem, github, linear" value={mcpName} onChange={(e) => setMcpName(e.target.value)} required />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Transport</label>
            <select className="gb-select" value={mcpTransport} onChange={(e) => setMcpTransport(e.target.value)}>
              <option value="stdio">stdio (local command)</option>
              <option value="sse">SSE (HTTP URL)</option>
              <option value="streamable_http">Streamable HTTP</option>
            </select>
          </div>
          {mcpTransport === "stdio" ? (
            <>
              <div className="gb-field">
                <label className="gb-field__label">Command</label>
                <input className="gb-input gb-input--mono" placeholder="npx" value={mcpCommand} onChange={(e) => setMcpCommand(e.target.value)} required />
              </div>
              <div className="gb-field">
                <label className="gb-field__label">Arguments (space-separated)</label>
                <input className="gb-input gb-input--mono" placeholder="-y @modelcontextprotocol/server-filesystem /home" value={mcpArgs} onChange={(e) => setMcpArgs(e.target.value)} />
              </div>
            </>
          ) : (
            <div className="gb-field">
              <label className="gb-field__label">Endpoint URL</label>
              <input className="gb-input gb-input--mono" placeholder="https://..." value={mcpUrl} onChange={(e) => setMcpUrl(e.target.value)} required />
            </div>
          )}
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button type="button" className="gb-btn gb-btn--ghost" onClick={() => setMcpModalOpen(false)}>Cancel</button>
            <button type="submit" className="gb-btn gb-btn--primary">Add Server</button>
          </div>
        </form>
      </Modal>
    </div>
  );
};
