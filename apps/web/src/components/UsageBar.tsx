import React, { memo } from "react";
import type { HermesSessionUsage } from "@grok-bot/hermes-client";
import { formatTokens } from "../lib/format";

export const UsageBar = memo(function UsageBar({ usage, contextLimit }: { usage: HermesSessionUsage | null; contextLimit?: number }) {
  const used = usage?.context_tokens;
  const limit = usage?.context_limit ?? contextLimit;
  const known = typeof used === "number" && Number.isFinite(used) && used >= 0 && typeof limit === "number" && Number.isFinite(limit) && limit > 0;
  const percent = known ? Math.min(100, used / limit * 100) : undefined;
  const label = known ? `Context ${formatTokens(used)} / ${formatTokens(limit)}`
    : usage?.total_tokens != null ? `${formatTokens(usage.total_tokens)} tokens · context unavailable` : "Usage unavailable";
  return <div className="gb-usage" data-warning={percent != null && percent >= 85 ? "true" : undefined}>
    <span className="gb-usage__track" role="progressbar" aria-label="Context usage" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent} aria-valuetext={label}>
      <span className="gb-usage__fill" style={{ width: `${percent ?? 0}%` }} />
    </span>
    <span>{label}</span>
    {known && <span className="gb-usage__percent">{Math.round(percent!)}%</span>}
  </div>;
});
