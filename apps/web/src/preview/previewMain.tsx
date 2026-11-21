import React from "react";
import ReactDOM from "react-dom/client";
import "@grok-bot/theme/styles";
import { BotPreviewApp } from "./BotPreviewApp";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <BotPreviewApp />
  </React.StrictMode>,
);
