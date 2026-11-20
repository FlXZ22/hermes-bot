import type { HermesConnection, HermesSshTarget } from "./types";

export function normalizeServerUrl(raw: string): string {
  let value = raw.trim();
  if (!value) throw new Error("Enter your Hermes server address.");
  if (/^[a-z][a-z\d+.-]*:/i.test(value) && !/^[a-z][a-z\d+.-]*:\/\//i.test(value) && !/^[^/]+:\d+(?:\/|$)/.test(value)) throw new Error("Use an http:// or https:// server address.");
  if (!/^[a-z][a-z\d+.-]*:\/\//i.test(value)) {
    value = `${/^(localhost|127\.\d+\.\d+\.\d+|\[::1\])(?::|\/|$)/i.test(value) ? "http" : "https"}://${value}`;
  }
  let url: URL;
  try { url = new URL(value); } catch { throw new Error("Enter a valid server URL, such as https://hermes.example.com."); }
  if (!["http:", "https:"].includes(url.protocol) || !url.hostname || url.username || url.password || url.search || url.hash) {
    throw new Error("Use an http:// or https:// address without a password, query, or fragment.");
  }
  return url.toString().replace(/\/+$/, "");
}

export function normalizeSshTarget(value: HermesSshTarget): HermesSshTarget {
  const host = value.host.trim(); const user = value.user?.trim();
  if (!host || host.startsWith("-") || !/^[a-z\d._:[\]-]+$/i.test(host)) throw new Error("Enter an SSH hostname, IP address, or SSH config alias.");
  if (user && (user.startsWith("-") || !/^[a-z\d._-]+$/i.test(user))) throw new Error("Enter a valid SSH username.");
  const port = value.port;
  if (port !== undefined && (!Number.isInteger(port) || port < 1 || port > 65535)) throw new Error("SSH port must be between 1 and 65535.");
  return { host, user: user || undefined, port, identityFile: value.identityFile?.trim() || undefined, configFile: value.configFile?.trim() || undefined };
}

export function parseSavedConnection(value: unknown): HermesConnection | null {
  if (!value || typeof value !== "object") return null;
  const conn = value as HermesConnection;
  if (typeof conn.id !== "string" || !conn.id || typeof conn.name !== "string" || typeof conn.url !== "string"
    || conn.mode !== "dashboard" || !["none", "session-token", "bearer", "cookie"].includes(conn.authType)) return null;
  try { return { ...conn, url: normalizeServerUrl(conn.url), ssh: conn.ssh ? normalizeSshTarget(conn.ssh) : undefined }; } catch { return null; }
}

export function connectionError(error: unknown): string {
  const status = (error as { status?: number })?.status;
  if (status === 401) return "The server needs you to sign in. Check your token or choose Sign in with browser.";
  if (status === 403) return "The server refused access. Check your account permissions and the server’s allowed origins.";
  if (status === 404) return "Hermes was not found at this address. Use the dashboard URL, including any path prefix.";
  const message = error instanceof Error ? error.message : String(error);
  if (/websocket/i.test(message)) return "The server’s live chat connection failed. Check that your reverse proxy forwards WebSocket upgrades for /api/ws.";
  if (/fetch|network|sending request|timed out|timeout/i.test(message)) return "Could not reach Hermes. Check the address, VPN or SSH tunnel, and TLS certificate. In browser preview, the server must also allow this origin.";
  return message;
}
