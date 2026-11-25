import { WORK_SESSIONS_KEY } from "../state/workSessions";

const draftKeys = new Set(["hermes-grok-bot:drafts", "hermes-grok-bot:work-drafts"]);

function object(value: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed as Record<string, unknown> : null;
  } catch {
    return null;
  }
}

/** Import data from the previous desktop origin without replacing Tauri edits. */
export function mergeLegacyRendererValue(key: string, saved: string, current: string | null): string {
  if (current === null) return saved;
  if (!draftKeys.has(key) && key !== WORK_SESSIONS_KEY) return current;
  const fromElectron = object(saved);
  const fromTauri = object(current);
  if (!fromElectron || !fromTauri) return current;
  if (draftKeys.has(key)) {
    const legacy = Object.fromEntries(Object.entries(fromElectron).filter(([, value]) => typeof value === "string"));
    return JSON.stringify({ ...legacy, ...fromTauri });
  }
  const ids = (value: unknown): string[] => Array.isArray(value) ? value.filter(item => typeof item === "string") : [];
  return JSON.stringify(Object.fromEntries([...new Set([...Object.keys(fromElectron), ...Object.keys(fromTauri)])].map(scope => [
    scope, [...new Set([...ids(fromTauri[scope]), ...ids(fromElectron[scope])])],
  ])));
}
