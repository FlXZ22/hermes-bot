/** Hermes uses a transient gateway ID for RPCs and a durable ID for history. */
export function resolveStreamSessionId(
  wireId: string | undefined,
  activeId: string | null,
  liveIds: Record<string, string>
): string {
  if (!wireId) return activeId ?? "draft";
  return Object.keys(liveIds).find(id => liveIds[id] === wireId) ?? wireId;
}
