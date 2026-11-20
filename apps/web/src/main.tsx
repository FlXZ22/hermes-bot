import React from "react";
import ReactDOM from "react-dom/client";
import "@grok-bot/theme/styles";
import { App } from "./App";

if ("hermesDesktop" in window) document.documentElement.dataset.desktopHost = "chromium";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
