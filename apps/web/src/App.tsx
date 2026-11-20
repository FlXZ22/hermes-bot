import React from "react";
import { AppProvider, useApp } from "./state/appStore";
import { Sidebar } from "./components/Sidebar";
import { ChatView } from "./components/ChatView";
import { BoardsView } from "./components/BoardsView";
import { SkillsView } from "./components/SkillsView";
import { CronView } from "./components/CronView";
import { MemoryView } from "./components/MemoryView";
import { GatewayView } from "./components/GatewayView";
import { AnalyticsView } from "./components/AnalyticsView";
import { SettingsView } from "./components/SettingsView";
import { RightSidePane } from "./components/RightSidePane";
import { ModelPickerModal } from "./components/ModelPickerModal";
import { AvatarEditorModal } from "./components/AvatarEditorModal";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { CommandPalette } from "./components/CommandPalette";

const Shell: React.FC = () => {
  const { activeView, toast, setupOpen, initializing, connection } = useApp();


  const renderCenter = () => {
    switch (activeView) {
      case "boards":
        return <BoardsView />;
      case "skills":
        return <SkillsView />;
      case "cron":
        return <CronView />;
      case "memory":
        return <MemoryView />;
      case "gateway":
        return <GatewayView />;
      case "analytics":
        return <AnalyticsView />;
      case "settings":
        return <SettingsView />;
      case "chat":
      default:
        return <ChatView />;
    }
  };

  return (
    <>
    {(setupOpen || initializing) && <WelcomeScreen />}
    <div className="gb-shell grok-bot-root" style={setupOpen || initializing ? { display: "none" } : undefined}>
      <Sidebar />
      {renderCenter()}
      <RightSidePane key={`${connection.id}:${connection.id.startsWith("local-") ? "local" : connection.url}:${JSON.stringify(connection.ssh ?? null)}`} />

      <ModelPickerModal />
      <AvatarEditorModal />
      <CommandPalette />

      {toast && (
        <div
          style={{
            position: "fixed",
            bottom: 22,
            right: 22,
            zIndex: 90,
            padding: "8px 14px",
            borderRadius: 9999,
            background: "var(--grok-bot-fill-emphasis)",
            color: "var(--grok-bot-bubble-user-ink)",
            fontSize: 13,
            fontWeight: 500,
            boxShadow: "var(--sand-shadow-popover)",
          }}
        >
          {toast}
        </div>
      )}
    </div>
    </>
  );
};

export const App: React.FC = () => (
  <AppProvider>
    <Shell />
  </AppProvider>
);

export default App;
