import type { HermesSessionImportMessage } from "@grok-bot/hermes-client";

export interface WorkImport {
  format: "Hermes Work" | "OpenCode";
  title: string;
  messages: HermesSessionImportMessage[];
  identity: string;
}

const IMPORT_LIMIT_BYTES = 5 * 1024 * 1024;
const IMPORT_LIMIT_MESSAGES = 10_000;
const IMPORT_ID_PREFIX = "work-import-";

function object(value: unknown): Record<string, unknown> | undefined {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : undefined;
}

function seconds(value: unknown): number | undefined {
  const stamp = typeof value === "number" ? value : typeof value === "string" ? Date.parse(value) / 1000 : NaN;
  if (!Number.isFinite(stamp) || stamp < 0) return undefined;
  return stamp >= 100_000_000_000 ? stamp / 1000 : stamp;
}

function textParts(parts: unknown): string | undefined {
  if (!Array.isArray(parts)) return undefined;
  const chunks = parts.flatMap(value => {
    const part = object(value);
    return part?.type === "text" && typeof part.text === "string" && !part.synthetic && !part.ignored ? [part.text] : [];
  });
  return chunks.join("\n\n");
}

export function isImportedWorkId(id: string): boolean {
  return id.startsWith(IMPORT_ID_PREFIX);
}

async function digest(value: string): Promise<string> {
  const encoded = new TextEncoder().encode(value);
  const bytes = crypto.subtle
    ? new Uint8Array(await crypto.subtle.digest("SHA-256", encoded))
    : (await import("@noble/hashes/sha2.js")).sha256(encoded);
  return Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
}

export async function importedWorkId(identity: string, cwd: string): Promise<string> {
  return `${IMPORT_ID_PREFIX}${await digest(JSON.stringify([identity, cwd]))}`;
}

export async function readWorkImport(file: File): Promise<WorkImport> {
  if (file.size > IMPORT_LIMIT_BYTES) throw new Error("Choose a session export smaller than 5 MB.");
  const text = await file.text();
  let root: Record<string, unknown> | undefined;
  try {
    root = object(JSON.parse(text));
  } catch {
    throw new Error("The session file must contain valid JSON.");
  }
  const isWork = root?.format === "hermes-work-session" && root.version === 1;
  const envelope = isWork ? root : object(root?.data) ?? root;
  const info = object(envelope?.[isWork ? "session" : "info"]);
  if (!info || typeof info.id !== "string" || !Array.isArray(envelope?.messages)) {
    throw new Error("Choose an OpenCode session export or a Hermes Work session export.");
  }

  let timestamp = seconds(object(info.time)?.created) ?? seconds(info.started_at) ?? 0;
  const messages: HermesSessionImportMessage[] = [];
  for (const value of envelope.messages) {
    const message = object(value);
    if (!message) continue;
    const legacy = object(message.info);
    const role = isWork ? message.role : legacy?.role ?? message.type;
    if (role !== "user" && role !== "assistant") continue;
    const content = isWork ? message.content : legacy ? textParts(message.parts)
      : role === "user" ? message.text : textParts(message.content);
    if (typeof content !== "string" || !content.trim()) continue;
    timestamp = seconds(isWork ? message.timestamp : object((legacy ?? message).time)?.created) ?? timestamp;
    messages.push({ role, content, timestamp });
    if (messages.length > IMPORT_LIMIT_MESSAGES) throw new Error("A session can contain at most 10,000 imported messages.");
  }
  if (!messages.length) throw new Error("The session export has no conversation text to import.");
  return {
    format: isWork ? "Hermes Work" : "OpenCode",
    title: typeof info.title === "string" && info.title.trim() ? info.title.trim().slice(0, 200) : "Imported Work session",
    messages,
    identity: await digest(text),
  };
}
