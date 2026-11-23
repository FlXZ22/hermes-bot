import { useEffect, useState } from "react";
import type { GrokEngineShape, GrokEngineState } from "../vendor/grok-face-engine";

/**
 * Exact 26 Grok Bot 1.2.3 body shapes (25 standard + pumpkin) from REA analysis.
 */
export const ALL_GROK_BOT_SHAPES: readonly { id: GrokEngineShape; label: string }[] = [
  { id: "blob", label: "Blob" },
  { id: "pebble", label: "Pebble" },
  { id: "bean", label: "Bean" },
  { id: "egg", label: "Egg" },
  { id: "squircle", label: "Squircle" },
  { id: "tablet", label: "Tablet" },
  { id: "capsule", label: "Capsule" },
  { id: "cylinder", label: "Cylinder" },
  { id: "hex", label: "Hex" },
  { id: "gem", label: "Gem" },
  { id: "crystal", label: "Crystal" },
  { id: "wedge", label: "Wedge" },
  { id: "shield", label: "Shield" },
  { id: "dome", label: "Dome" },
  { id: "arch", label: "Arch" },
  { id: "cloud", label: "Cloud" },
  { id: "teardrop", label: "Teardrop" },
  { id: "leaf", label: "Leaf" },
  { id: "square", label: "Square" },
  { id: "sparkle", label: "Sparkle" },
  { id: "clover", label: "Clover" },
  { id: "flower", label: "Flower" },
  { id: "heart", label: "Heart" },
  { id: "house", label: "House" },
  { id: "star", label: "Star" },
  { id: "pumpkin", label: "Pumpkin" },
] as const;

const VALID_SHAPE_SET = new Set<string>(ALL_GROK_BOT_SHAPES.map((s) => s.id));

/**
 * Deterministic Grok Bot shape resolver (FNV-1a + splitmix32 matching Grok Bot's On/An/Pn).
 */
export function resolveGrokBotShape(shapeId?: string | null, seed?: string): GrokEngineShape {
  if (shapeId && VALID_SHAPE_SET.has(shapeId)) {
    return shapeId as GrokEngineShape;
  }
  if (seed) {
    let h = 2166136261;
    for (let i = 0; i < seed.length; i++) {
      h ^= seed.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    h = Math.imul(h ^ (h >>> 16), 73244475);
    h = Math.imul(h ^ (h >>> 13), 3266489909);
    // Use the 8 core shapes for deterministic default seeding (matching Grok Bot's `yn`).
    const defaults: readonly GrokEngineShape[] = [
      "blob",
      "pebble",
      "squircle",
      "tablet",
      "wedge",
      "hex",
      "cloud",
      "teardrop",
    ];
    return defaults[((h ^ (h >>> 16)) >>> 0) % defaults.length];
  }
  return "blob";
}

export type TurnSettlementOutcome = "success" | "error" | "cancelled";

export interface TurnSettlement {
  outcome: TurnSettlementOutcome;
  settledAtMs: number;
}

export interface BotActivityFacts {
  isStreaming: boolean;
  hasStreamedText: boolean;
  activeToolName: string | null;
  liveStatusText: string;
  awaitingUserResponse: boolean;
  lastTurnSettlement: TurnSettlement | null;
}

/**
 * Grok Bot's exact settlement hold duration (`Zct = 8000ms` in index.eager-app).
 */
export const TURN_SETTLEMENT_HOLD_MS = 8_000;

/**
 * Exact Grok Bot verb-to-state map (`Xct` in index.eager-app).
 */
const ACTIVITY_VERB_TO_STATE: Record<string, GrokEngineState> = {
  thinking: "thinking",
  searching: "searching",
  browsing: "reading",
  reading: "reading",
  connecting: "orbit",
  writing: "writing",
  coding: "coding",
  generating: "progress",
  "running-commands": "running",
  "on-its-computer": "computing",
  "on-your-computer": "controlling",
  working: "working",
  messaging: "sending",
  sending: "sending",
  waiting: "waiting",
};

/**
 * Classify a Hermes tool call name or live status string into a Grok Bot
 * activity verb, matching Grok Bot's tool classifier (`tO(...)` in index.eager-app).
 */
export function classifyHermesActivity(
  toolName?: string | null,
  statusText?: string | null,
): GrokEngineState {
  const raw = `${toolName ?? ""} ${statusText ?? ""}`
    .replace(/[_-]+/g, " ")
    .trim()
    .toLowerCase();
  if (!raw) return "thinking";

  // File writing / editing / drafting -> "writing" (authentic pencil animation!)
  if (
    /\b(write|edit|patch|replace|create_file|save_file|update_file|insert|str_replace|apply_diff|draft|memo|note_write|soul|memory_write)\b/.test(
      raw,
    ) ||
    raw.includes("write_file") ||
    raw.includes("edit_file") ||
    raw.includes("replace_file") ||
    raw.includes("drafting")
  ) {
    return ACTIVITY_VERB_TO_STATE.writing;
  }

  // Web/code search -> "searching" (radar rings morph)
  if (
    /\b(search|web_search|grep|glob|find|ripgrep|duckduckgo|tavily|perplexity|lookup|query)\b/.test(
      raw,
    )
  ) {
    return ACTIVITY_VERB_TO_STATE.searching;
  }

  // Reading / viewing / browsing -> "reading"
  if (
    /\b(read|view|fetch|browse|open_url|web_fetch|list_dir|ls|cat|stat|inspect|scrape|crawl)\b/.test(
      raw,
    )
  ) {
    return ACTIVITY_VERB_TO_STATE.reading;
  }

  // Shell / terminal commands -> "running"
  if (
    /\b(terminal|shell|bash|zsh|sh|exec|command|run_command|process|ssh|cli)\b/.test(
      raw,
    )
  ) {
    return ACTIVITY_VERB_TO_STATE["running-commands"];
  }

  // Coding / code execution / git / tests -> "coding"
  if (
    /\b(code|python|node|execute_code|eval|repl|compile|test|lint|git|refactor|debug)\b/.test(
      raw,
    )
  ) {
    return ACTIVITY_VERB_TO_STATE.coding;
  }

  // Computer use / browser automation / vision -> "computing"
  if (
    /\b(computer|screenshot|browser|click|mouse|keyboard|vision|desktop|sandbox)\b/.test(
      raw,
    )
  ) {
    return ACTIVITY_VERB_TO_STATE["on-its-computer"];
  }

  // Image / audio / video / artifact generation -> "progress"
  if (
    /\b(generate|image|video|audio|tts|speech|render|diagram|chart|export)\b/.test(
      raw,
    )
  ) {
    return ACTIVITY_VERB_TO_STATE.generating;
  }

  // Messaging / notifications / webhooks -> "sending"
  if (
    /\b(send|message|email|mail|slack|telegram|discord|whatsapp|webhook|notify|post)\b/.test(
      raw,
    )
  ) {
    return ACTIVITY_VERB_TO_STATE.messaging;
  }

  // Connecting / MCP / subagents / cron / waiting -> "orbit" / "waiting"
  if (/\b(connect|mcp|login|auth|sync|oauth)\b/.test(raw)) {
    return ACTIVITY_VERB_TO_STATE.connecting;
  }
  if (/\b(wait|await|sleep|delegate|subagent|cron|schedule|poll)\b/.test(raw)) {
    return ACTIVITY_VERB_TO_STATE.waiting;
  }

  // Explicit thinking / reasoning status
  if (/\b(think|reason|plan|analyz|ponder)\b/.test(raw)) {
    return ACTIVITY_VERB_TO_STATE.thinking;
  }

  return toolName ? ACTIVITY_VERB_TO_STATE.working : ACTIVITY_VERB_TO_STATE.thinking;
}

/**
 * Exact recreation of Grok Bot's `uO(facts, nowMs)` state machine:
 * 1. If awaiting user approval/clarification -> `blocked` (transitions to `drowsy` after 120s in engine)
 * 2. If turn is active (`isStreaming`):
 *    - If a tool is currently running -> classified activity state (`writing`, `searching`, `reading`, `running`, `coding`, `working`, etc.)
 *    - Else if assistant text has started streaming -> `streaming`
 *    - Else -> classified status or `thinking` (`dots` morph)
 * 3. If turn settled within the last 8,000ms (`Zct`):
 *    - `success` -> `finished` (happy celebration clip)
 *    - `error` -> `failed` (sad/blocked clip)
 *    - `cancelled` -> `idle`
 * 4. Otherwise -> `idle`
 */
export function deriveGrokBotState(
  facts: BotActivityFacts | null | undefined,
  nowMs: number = Date.now(),
): GrokEngineState {
  if (!facts) return "idle";
  if (facts.awaitingUserResponse) return "blocked";
  if (facts.isStreaming) {
    if (facts.activeToolName) {
      return classifyHermesActivity(facts.activeToolName, facts.liveStatusText);
    }
    if (facts.hasStreamedText) {
      return "streaming";
    }
    return classifyHermesActivity(null, facts.liveStatusText);
  }
  const settlement = facts.lastTurnSettlement;
  if (settlement && nowMs < settlement.settledAtMs + TURN_SETTLEMENT_HOLD_MS) {
    if (settlement.outcome === "success") return "finished";
    if (settlement.outcome === "error") return "failed";
  }
  return "idle";
}

/**
 * React hook mirroring Grok Bot's `nlt` / `oO` timer hook: automatically
 * transitions `finished` / `failed` back to `idle` after `TURN_SETTLEMENT_HOLD_MS` (8s).
 */
export function useDerivedBotState(facts: BotActivityFacts | null | undefined): GrokEngineState {
  const [, setTick] = useState(0);
  const settlement = facts?.lastTurnSettlement ?? null;
  const settlesAt = settlement ? settlement.settledAtMs + TURN_SETTLEMENT_HOLD_MS : null;

  useEffect(() => {
    if (settlesAt === null) return;
    const remaining = settlesAt - Date.now();
    if (remaining <= 0) return;
    const timer = window.setTimeout(() => setTick((t) => t + 1), remaining + 20);
    return () => window.clearTimeout(timer);
  }, [settlesAt]);

  return deriveGrokBotState(facts, Date.now());
}
