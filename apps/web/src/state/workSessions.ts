export const WORK_SESSIONS_KEY = "hermes-grok-bot:work-sessions";

export type SessionSection = "chat" | "work";
export type WorkSessionCatalog = Record<string, string[]>;

export interface WorkCheckpoint {
  hash: string;
  timestamp: string | number;
  message: string;
}

export interface WorkCheckpointResults {
  list: { enabled: boolean; checkpoints: WorkCheckpoint[] };
  diff: { stat: string; diff: string };
  restore: { success: boolean; error?: string; history_removed?: number };
}

export interface WorkReference {
  text: string;
  display: string;
  meta?: string;
}

export function formatWorkReference(text: string): string {
  const match = /^(@(?:file|folder):)(.+)$/.exec(text);
  if (!match || !/[\s()[\]{}<>"'\x60]/.test(match[2])) return text;
  const quote = ["`", '"', "'"].find(candidate => !match[2].includes(candidate));
  return quote ? `${match[1]}${quote}${match[2]}${quote}` : text;
}

export function loadWorkSessions(): WorkSessionCatalog {
  try {
    const raw: unknown = JSON.parse(localStorage.getItem(WORK_SESSIONS_KEY) ?? "{}");
    if (!raw || typeof raw !== "object" || Array.isArray(raw)) return {};
    return Object.fromEntries(Object.entries(raw).filter(
      ([, ids]) => Array.isArray(ids) && ids.every(id => typeof id === "string")
    ));
  } catch {
    return {};
  }
}

export function normalizeWorkspacePath(raw: string): string {
  const path = raw.trim();
  if (!path.startsWith("/") || /[\u0000-\u001f]/.test(path)) {
    throw new Error("Enter the absolute path of a workspace folder on the Hermes host.");
  }
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (part === "..") parts.pop();
    else if (part && part !== ".") parts.push(part);
  }
  return `/${parts.join("/")}`;
}

export function isWorkControlCommand(text: string): boolean {
  return /^\/(?:queue|q|steer)(?:\s|$)/i.test(text)
    || /^\/(?:goal(?:\s+(?:status|pause|clear))?|status|agents|diff)$/i.test(text);
}

export function workGoalStatus(text: string): string | null {
  const firstLine = text.trim().split("\n", 1)[0];
  return /^(?:No (?:active )?goal|✓ Goal cleared)/i.test(firstLine) ? null : firstLine || null;
}
