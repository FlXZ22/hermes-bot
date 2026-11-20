export type GrokBotColorId =
  | "black"
  | "brown"
  | "red"
  | "orange"
  | "yellow"
  | "green"
  | "cyan"
  | "blue"
  | "violet"
  | "magenta"
  | "gray";

export interface BotColorPreset {
  id: GrokBotColorId;
  label: string;
  hex: string;
  lightAccent: string;
  darkAccent: string;
  lightBubble: string;
  darkBubble: string;
  lightTint: string;
  darkTint: string;
}

export const BOT_COLOR_PRESETS: readonly BotColorPreset[] = [
  {
    id: "blue",
    label: "Blue",
    hex: "#1084FE",
    lightAccent: "#0b6dd3",
    darkAccent: "#4d9bff",
    lightBubble: "#126fd4",
    darkBubble: "#116dd2",
    lightTint: "#1084FE",
    darkTint: "#1084FE",
  },
  {
    id: "violet",
    label: "Violet",
    hex: "#9159FE",
    lightAccent: "#7547d0",
    darkAccent: "#a582ff",
    lightBubble: "#7442d1",
    darkBubble: "#7845d7",
    lightTint: "#9159FE",
    darkTint: "#9159FE",
  },
  {
    id: "cyan",
    label: "Cyan",
    hex: "#00BCA6",
    lightAccent: "#008273",
    darkAccent: "#00b7a2",
    lightBubble: "#0d9381",
    darkBubble: "#0e9886",
    lightTint: "#00BCA6",
    darkTint: "#00BCA6",
  },
  {
    id: "green",
    label: "Green",
    hex: "#00C972",
    lightAccent: "#00854a",
    darkAccent: "#00ba69",
    lightBubble: "#0d9655",
    darkBubble: "#0e9f5a",
    lightTint: "#00C972",
    darkTint: "#00C972",
  },
  {
    id: "orange",
    label: "Orange",
    hex: "#FF6700",
    lightAccent: "#c14c00",
    darkAccent: "#fe6600",
    lightBubble: "#d2550a",
    darkBubble: "#cd5309",
    lightTint: "#FF6700",
    darkTint: "#FF6700",
  },
  {
    id: "red",
    label: "Red",
    hex: "#FF263C",
    lightAccent: "#cd2834",
    darkAccent: "#ff6463",
    lightBubble: "#c82532",
    darkBubble: "#cf2b35",
    lightTint: "#FF263C",
    darkTint: "#FF263C",
  },
  {
    id: "magenta",
    label: "Magenta",
    hex: "#FF309B",
    lightAccent: "#c62378",
    darkAccent: "#ff55a4",
    lightBubble: "#c82779",
    darkBubble: "#cf297e",
    lightTint: "#FF309B",
    darkTint: "#FF309B",
  },
  {
    id: "yellow",
    label: "Yellow",
    hex: "#FF9800",
    lightAccent: "#aa6300",
    darkAccent: "#fb9500",
    lightBubble: "#c2730b",
    darkBubble: "#c8770c",
    lightTint: "#FF9800",
    darkTint: "#FF9800",
  },
  {
    id: "brown",
    label: "Brown",
    hex: "#936439",
    lightAccent: "#996b42",
    darkAccent: "#c59e7d",
    lightBubble: "#86592e",
    darkBubble: "#8c5d31",
    lightTint: "#936439",
    darkTint: "#936439",
  },
  {
    id: "gray",
    label: "Gray",
    hex: "#777777",
    lightAccent: "#525252",
    darkAccent: "#c1c1c1",
    lightBubble: "#363636",
    darkBubble: "#525252",
    lightTint: "#777777",
    darkTint: "#777777",
  },
  {
    id: "black",
    label: "Black",
    hex: "#1C1C1C",
    lightAccent: "#1c1c1c",
    darkAccent: "#f5f5f5",
    lightBubble: "#1c1c1c",
    darkBubble: "#f5f5f5",
    lightTint: "#1c1c1c",
    darkTint: "#777777",
  },
] as const;

const AUTO_COLORS = BOT_COLOR_PRESETS.filter((c) => c.id !== "black");

export const DATAVIZ_PALETTE = {
  light: [
    "#1084FE",
    "#FF6700",
    "#9159FE",
    "#00C972",
    "#FF309B",
    "#FF9800",
    "#00BCA6",
    "#936439",
  ],
  dark: [
    "#1084FE",
    "#FF6700",
    "#9159FE",
    "#00C972",
    "#FF309B",
    "#FF9800",
    "#00BCA6",
    "#97683D",
  ],
} as const;

function fnv1a(input: string): number {
  let h = 2166136261;
  for (let i = 0; i < input.length; i++) {
    h ^= input.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

function splitmix32(seed: number): number {
  let t = (seed + 1831565813) | 0;
  let r = Math.imul(t ^ (t >>> 15), 1 | t);
  r = (r + Math.imul(r ^ (r >>> 7), 61 | r)) ^ r;
  return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
}

export function resolveBotColor(seed: string, preferredColor?: string | null): BotColorPreset {
  if (preferredColor) {
    const exact = BOT_COLOR_PRESETS.find((c) => c.id === preferredColor);
    if (exact) return exact;
  }
  const hashed = (fnv1a(seed) ^ Math.imul(1, 2654435769)) >>> 0;
  const idx = Math.floor(splitmix32((hashed ^ Math.imul(1, 2654435769)) >>> 0) * AUTO_COLORS.length);
  return AUTO_COLORS[idx] ?? BOT_COLOR_PRESETS[0];
}

export function getAgentThemeVars(
  seed: string,
  appearance: "light" | "dark",
  preferredColor?: string | null
): Record<string, string> {
  const preset = resolveBotColor(seed, preferredColor);
  const isDark = appearance === "dark";
  return {
    "--grok-bot-current-agent-tint": isDark ? preset.darkTint : preset.lightTint,
    "--grok-bot-current-agent-bubble": isDark ? preset.darkBubble : preset.lightBubble,
    "--grok-bot-current-agent-bubble-ink":
      preset.id === "black" && isDark ? "#121212" : "#ffffff",
    "--grok-bot-current-agent-accent": isDark ? preset.darkAccent : preset.lightAccent,
    "--grok-bot-current-agent-coat": preset.hex,
  };
}
