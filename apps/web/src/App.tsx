import React from "react";
import { AppProvider, useApp } from "./state/appStore";
import { Sidebar } from "./components/Sidebar";
import { ChatView } from "./components/ChatView";
import { RightSidePane } from "./components/RightSidePane";
import { ModelPickerModal } from "./components/ModelPickerModal";
import { AvatarEditorModal } from "./components/AvatarEditorModal";
import { WelcomeScreen } from "./components/WelcomeScreen";
import { CommandPalette } from "./components/CommandPalette";

const BoardsView = React.lazy(() => import("./components/BoardsView").then(module => ({ default: module.BoardsView })));

const SkillsView = React.lazy(() => import("./components/SkillsView").then(module => ({ default: module.SkillsView })));

const CronView = React.lazy(() => import("./components/CronView").then(module => ({ default: module.CronView })));

const MemoryView = React.lazy(() => import("./components/MemoryView").then(module => ({ default: module.MemoryView })));

const GatewayView = React.lazy(() => import("./components/GatewayView").then(module => ({ default: module.GatewayView })));

const AnalyticsView = React.lazy(() => import("./components/AnalyticsView").then(module => ({ default: module.AnalyticsView })));

const SettingsView = React.lazy(() => import("./components/SettingsView").then(module => ({ default: module.SettingsView })));

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
      <React.Suspense fallback={<main className="gb-chat" aria-busy="true" />}>{renderCenter()}</React.Suspense>
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
