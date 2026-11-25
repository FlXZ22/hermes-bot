import React from "react";
import ReactDOM from "react-dom/client";
import "@grok-bot/theme/styles";
import { App } from "./App";
import { tauriBridge } from "./lib/tauriBridge";
import { mergeLegacyRendererValue } from "./lib/legacyRendererState";

if ("hermesDesktop" in window) document.documentElement.dataset.desktopHost = "chromium";
else if ("__TAURI__" in window) {
  document.documentElement.dataset.desktopHost = "tauri";
  document.addEventListener("click", event => {
    if (!event.isTrusted || event.defaultPrevented || event.button !== 0 || !(event.target instanceof Element)) return;
    const anchor = event.target.closest<HTMLAnchorElement>("a[href]");
    if (!anchor || anchor.hasAttribute("download") ||
        (anchor.protocol !== "http:" && anchor.protocol !== "https:")) return;
    event.preventDefault();
    void tauriBridge.openExternalLink(anchor.href).catch(console.warn);
  });
}

async function restoreDesktopPreferences() {
  const marker = "hermes-grok-bot:migrated-electron-v1";
  try {
    if (localStorage.getItem(marker)) return;
    const saved = await tauriBridge.loadLegacyRendererState();
    for (const [key, value] of Object.entries(saved)) {
      const current = localStorage.getItem(key);
      const merged = mergeLegacyRendererValue(key, value, current);
      if (merged !== current) localStorage.setItem(key, merged);
    }
    localStorage.setItem(marker, "true");
  } catch (error) {
    // Retry on the next launch; never prevent access to the desktop app.
    console.warn("Could not restore previous desktop preferences:", error);
  }
}

function mount() {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>
  );
}

if (document.documentElement.dataset.desktopHost === "tauri" && "__TAURI__" in window) {
  void restoreDesktopPreferences().finally(mount);
} else {
  mount();
}
