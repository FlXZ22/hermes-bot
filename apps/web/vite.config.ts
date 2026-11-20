import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

export default defineConfig({
  plugins: [react()],
  base: "./",
  resolve: {
    alias: {
      // The browser export uses document.createElement, which is unavailable in a worker.
      "decode-named-character-reference": require.resolve("decode-named-character-reference"),
      "@grok-bot/theme/styles": path.resolve(__dirname, "../../packages/grok-bot-theme/src/styles/index.css"),
      "@grok-bot/theme": path.resolve(__dirname, "../../packages/grok-bot-theme/src/index.ts"),
      "@grok-bot/hermes-client": path.resolve(__dirname, "../../packages/hermes-client/src/index.ts"),
    },
  },
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    fs: {
      allow: [path.resolve(__dirname, "../..")],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    assetsInlineLimit: 0,
  },
});
