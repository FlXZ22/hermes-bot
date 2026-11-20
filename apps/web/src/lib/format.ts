export function formatRelativeTime(ts?: string | number | null): string {
  if (ts === undefined || ts === null || ts === "") return "";
  let ms: number;
  if (typeof ts === "number") {
    ms = ts > 1e12 ? ts : ts * 1000;
  } else {
    const parsed = Number(ts);
    if (!Number.isNaN(parsed)) {
      ms = parsed > 1e12 ? parsed : parsed * 1000;
    } else {
      ms = Date.parse(ts);
    }
  }
  if (Number.isNaN(ms) || ms <= 0) return "";
  const diffSec = Math.max(0, Math.floor((Date.now() - ms) / 1000));
  if (diffSec < 45) return "now";
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m`;
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h`;
  if (diffSec < 86400 * 7) return `${Math.floor(diffSec / 86400)}d`;
  const d = new Date(ms);
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export function formatTokens(n?: number | null): string {
  if (!n || n <= 0) return "0";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

export function shortModelName(model?: string | null): string {
  if (!model) return "Select model";
  const parts = model.split("/");
  return parts[parts.length - 1] || model;
}

export function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 42) || `board-${Date.now().toString(36)}`;
}
