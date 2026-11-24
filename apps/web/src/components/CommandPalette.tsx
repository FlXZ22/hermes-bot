import React, { useMemo, useState } from "react";
import { CursorIcon, type CursorIconName } from "@grok-bot/theme";
import { useApp } from "../state/appStore";

interface PaletteItem {
  id: string;
  label: string;
  detail: string;
  icon: CursorIconName;
  run: () => void;
}

export const CommandPalette: React.FC = () => {
  const {
    commandPaletteOpen,
    setCommandPaletteOpen,
    setActiveView,
    setModelPickerOpen,
    toggleTheme,
    theme,
    createNewSession,
    startNewWork,
    sessions,
    workSessionIds,
    selectSession,
    boards,
    setActiveBoardSlug,
    restartLocalBackend,
    refreshModels,
  } = useApp();

  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);

  const items = useMemo<PaletteItem[]>(() => {
    const base: PaletteItem[] = [
      {
        id: "new-chat",
        label: "New Chat Session",
        detail: "Start a fresh Hermes conversation",
        icon: "chat-bubble-plus",
        run: () => void createNewSession(),
      },
      {
        id: "new-work",
        label: "New Work Session",
        detail: "Choose a workspace for Hermes",
        icon: "code",
        run: startNewWork,
      },
      {
        id: "pick-model",
        label: "Choose Model & Provider",
        detail: "Ctrl+Shift+M",
        icon: "sparkles",
        run: () => setModelPickerOpen(true),
      },
      {
        id: "restart-backend",
        label: "Restart Hermes Backend",
        detail: "Reload providers, plugins, and gateway",
        icon: "refresh",
        run: () => void restartLocalBackend(),
      },
      {
        id: "refresh-models",
        label: "Refresh Models & Providers",
        detail: "Re-fetch live provider catalogs",
        icon: "refresh",
        run: () => void refreshModels(),
      },
      {
        id: "open-boards",
        label: "Open Session Boards (Kanban)",
        detail: "View all boards & tasks",
        icon: "board-kanban",
        run: () => setActiveView("boards"),
      },
      {
        id: "toggle-theme",
        label: theme === "dark" ? "Switch to Sand Light Theme" : "Switch to Sand Dark Theme",
        detail: "Grok Bot appearance",
        icon: theme === "dark" ? "sun" : "moon",
        run: toggleTheme,
      },
      {
        id: "open-skills",
        label: "Skills, Toolsets & MCP Servers",
        detail: "Manage capabilities",
        icon: "plug",
        run: () => setActiveView("skills"),
      },
      {
        id: "open-cron",
        label: "Cron Jobs & Automations",
        detail: "Scheduled prompts",
        icon: "clock",
        run: () => setActiveView("cron"),
      },
      {
        id: "open-memory",
        label: "Memory & SOUL.md",
        detail: "Knowledge graph & persona",
        icon: "brain",
        run: () => setActiveView("memory"),
      },
      {
        id: "open-gateway",
        label: "Gateway, Messaging & Webhooks",
        detail: "Platforms & pairings",
        icon: "globe",
        run: () => setActiveView("gateway"),
      },
      {
        id: "open-settings",
        label: "Settings & Connections",
        detail: "API keys, config, logs",
        icon: "settings-gear",
        run: () => setActiveView("settings"),
      },
    ];

    for (const b of boards) {
      base.push({
        id: `board:${b.slug}`,
        label: `Board: ${b.name}`,
        detail: "Filter sidebar & open board",
        icon: "board-kanban",
        run: () => {
          setActiveBoardSlug(b.slug);
          setActiveView("boards");
        },
      });
    }

    for (const s of sessions.slice(0, 20)) {
      const work = workSessionIds.has(s.id);
      base.push({
        id: `sess:${s.id}`,
        label: s.title || (work ? "Untitled Work" : "Untitled Chat"),
        detail: work ? `Work · ${s.cwd ?? "Workspace"}` : s.model || "Session",
        icon: work ? "code" : "chat-bubble",
        run: () => void selectSession(s.id),
      });
    }

    const q = query.trim().toLowerCase();
    if (!q) return base.slice(0, 14);
    return base.filter((it) => `${it.label} ${it.detail}`.toLowerCase().includes(q)).slice(0, 14);
  }, [
    boards,
    createNewSession,
    query,
    selectSession,
    sessions,
    workSessionIds,
    setActiveBoardSlug,
    setActiveView,
    setModelPickerOpen,
    theme,
    toggleTheme,
    restartLocalBackend,
    refreshModels,
  ]);

  if (!commandPaletteOpen) return null;

  const close = () => {
    setCommandPaletteOpen(false);
    setQuery("");
    setIndex(0);
  };

  return (
    <div className="gb-modal-backdrop" onMouseDown={close}>
      <div className="gb-modal gb-cmd-modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="gb-cmd-input-wrap">
          <CursorIcon name="search" size={16} style={{ color: "var(--grok-bot-text-tertiary)" }} />
          <input
            className="gb-cmd-input"
            placeholder="Jump to a session, board, model, or command..."
            value={query}
            autoFocus
            onChange={(e) => {
              setQuery(e.target.value);
              setIndex(0);
            }}
            onKeyDown={(e) => {
              if (e.key === "Escape") close();
              else if (e.key === "ArrowDown") {
                e.preventDefault();
                setIndex((i) => (items.length ? (i + 1) % items.length : 0));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setIndex((i) => (items.length ? (i - 1 + items.length) % items.length : 0));
              } else if (e.key === "Enter" && items[index]) {
                e.preventDefault();
                items[index].run();
                close();
              }
            }}
          />
          <span className="gb-keycap">Esc</span>
        </div>

        <div className="gb-cmd-list">
          {items.map((item, idx) => (
            <button
              key={item.id}
              type="button"
              className="gb-popover-item"
              data-highlighted={idx === index ? "true" : undefined}
              onMouseEnter={() => setIndex(idx)}
              onClick={() => {
                item.run();
                close();
              }}
            >
              <CursorIcon name={item.icon} size={15} style={{ color: "var(--grok-bot-text-secondary)" }} />
              <span style={{ flex: 1, fontWeight: 500, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {item.label}
              </span>
              <span style={{ fontSize: 12, color: "var(--grok-bot-text-tertiary)" }}>{item.detail}</span>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
};
