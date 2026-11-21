/**
 * Clean-room TypeScript recreation of the Grok Bot 1.2.3 procedural face,
 * 26-shape silhouette, 11-color palette, 51-state choreographer, and 13-morph
 * animation engine (including `thinking` dots and `writing`/`editing` pencil),
 * built from our REA static analysis of `grok-bot_1.2.3_amd64.deb`.
 */

export interface BotColorDef {
  id: BotColorId;
  label: string;
  hex: string;
}

export const GROK_BOT_COLORS: readonly BotColorDef[] = [
  { id: "black", label: "Black", hex: "#000000" },
  { id: "brown", label: "Brown", hex: "#936439" },
  { id: "red", label: "Red", hex: "#FF263C" },
  { id: "orange", label: "Orange", hex: "#FF6700" },
  { id: "yellow", label: "Yellow", hex: "#FF9800" },
  { id: "green", label: "Green", hex: "#00C972" },
  { id: "cyan", label: "Cyan", hex: "#00BCA6" },
  { id: "blue", label: "Blue", hex: "#1084FE" },
  { id: "violet", label: "Violet", hex: "#9159FE" },
  { id: "magenta", label: "Magenta", hex: "#FF309B" },
  { id: "gray", label: "Gray", hex: "#777777" },
] as const;

export type BotColorId =
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

export const GROK_BOT_SHAPES = [
  "blob",
  "pebble",
  "bean",
  "egg",
  "squircle",
  "tablet",
  "capsule",
  "cylinder",
  "hex",
  "gem",
  "crystal",
  "wedge",
  "shield",
  "dome",
  "arch",
  "cloud",
  "teardrop",
  "leaf",
  "square",
  "sparkle",
  "clover",
  "flower",
  "heart",
  "house",
  "star",
  "pumpkin",
] as const;

export type GrokBotShapeId = (typeof GROK_BOT_SHAPES)[number];

export type GrokMorphKind =
  | "dots"
  | "orbit"
  | "radar"
  | "progress"
  | "gather"
  | "wave"
  | "send"
  | "receive"
  | "dock"
  | "ball"
  | "pencil"
  | "bang"
  | "standby";

export type GrokEyeExpression =
  | "neutral"
  | "happy"
  | "surprised"
  | "inspecting"
  | "sad"
  | "sleepy"
  | "squinted"
  | "calm"
  | "confused";

export interface StateCategory {
  label: string;
  states: readonly GrokBotStateId[];
}

export const GROK_STATE_CATEGORIES: readonly StateCategory[] = [
  {
    label: "Lifecycle",
    states: [
      "idle",
      "listening",
      "thinking",
      "streaming",
      "searching",
      "reading",
      "working",
      "coding",
      "editing",
      "writing",
      "waiting",
      "blocked",
      "finished",
      "failed",
      "sleeping",
      "waking",
    ],
  },
  {
    label: "Morphs",
    states: [
      "thinking",
      "writing",
      "editing",
      "orbit",
      "radar",
      "progress",
      "spawning",
      "dictating",
      "sending",
      "receiving",
      "uploading",
      "alerting",
      "bouncing",
      "powering-down",
    ],
  },
  {
    label: "Reactions",
    states: [
      "happy",
      "excited",
      "laughing",
      "proud",
      "curious",
      "playful",
      "shy",
      "surprised",
      "suspicious",
      "angry",
      "confused",
      "sad",
      "drowsy",
      "bored",
      "scared",
      "celebrate",
      "approving",
    ],
  },
  {
    label: "Brand & Product",
    states: ["humming", "notifying", "dragging", "logo", "showcase", "turntable"],
  },
] as const;

export type GrokBotStateId =
  | "idle"
  | "listening"
  | "thinking"
  | "streaming"
  | "searching"
  | "reading"
  | "working"
  | "coding"
  | "editing"
  | "writing"
  | "waiting"
  | "blocked"
  | "finished"
  | "failed"
  | "sleeping"
  | "waking"
  | "orbit"
  | "radar"
  | "progress"
  | "spawning"
  | "dictating"
  | "sending"
  | "receiving"
  | "uploading"
  | "alerting"
  | "bouncing"
  | "powering-down"
  | "happy"
  | "excited"
  | "laughing"
  | "proud"
  | "curious"
  | "playful"
  | "shy"
  | "surprised"
  | "suspicious"
  | "angry"
  | "confused"
  | "sad"
  | "drowsy"
  | "bored"
  | "scared"
  | "celebrate"
  | "approving"
  | "humming"
  | "notifying"
  | "dragging"
  | "logo"
  | "showcase"
  | "turntable";

interface ShapeEyeMetrics {
  width: number;
  height: number;
  gap: number;
  shiftY: number;
}

// Per-shape eye proportions derived from REA analysis of Grok Bot 1.2.3
const SHAPE_EYE_METRICS: Record<GrokBotShapeId, ShapeEyeMetrics> = {
  blob: { width: 0.24, height: 0.52, gap: 0.54, shiftY: 0.06 },
  pebble: { width: 0.239, height: 0.524, gap: 0.546, shiftY: 0.12 },
  bean: { width: 0.203, height: 0.445, gap: 0.461, shiftY: 0.12 },
  egg: { width: 0.239, height: 0.524, gap: 0.434, shiftY: 0.113 },
  squircle: { width: 0.239, height: 0.524, gap: 0.563, shiftY: 0.12 },
  tablet: { width: 0.213, height: 0.467, gap: 0.588, shiftY: 0.02 },
  capsule: { width: 0.211, height: 0.462, gap: 0.36, shiftY: 0.12 },
  cylinder: { width: 0.239, height: 0.524, gap: 0.479, shiftY: 0.12 },
  hex: { width: 0.26, height: 0.57, gap: 0.64, shiftY: 0.12 },
  gem: { width: 0.239, height: 0.524, gap: 0.501, shiftY: 0.12 },
  crystal: { width: 0.161, height: 0.353, gap: 0.371, shiftY: 0.12 },
  wedge: { width: 0.213, height: 0.467, gap: 0.468, shiftY: 0.18 },
  shield: { width: 0.268, height: 0.587, gap: 0.445, shiftY: 0.12 },
  dome: { width: 0.213, height: 0.467, gap: 0.479, shiftY: 0.106 },
  arch: { width: 0.192, height: 0.422, gap: 0.448, shiftY: 0.16 },
  cloud: { width: 0.213, height: 0.467, gap: 0.47, shiftY: 0.07 },
  teardrop: { width: 0.208, height: 0.456, gap: 0.453, shiftY: 0.18 },
  leaf: { width: 0.187, height: 0.41, gap: 0.448, shiftY: 0.04 },
  square: { width: 0.255, height: 0.56, gap: 0.529, shiftY: 0.12 },
  sparkle: { width: 0.184, height: 0.403, gap: 0.412, shiftY: 0.0 },
  clover: { width: 0.233, height: 0.511, gap: 0.559, shiftY: -0.02 },
  flower: { width: 0.193, height: 0.423, gap: 0.383, shiftY: 0.12 },
  heart: { width: 0.187, height: 0.409, gap: 0.446, shiftY: 0.052 },
  house: { width: 0.23, height: 0.503, gap: 0.513, shiftY: 0.185 },
  star: { width: 0.185, height: 0.45, gap: 0.419, shiftY: 0.12 },
  pumpkin: { width: 0.215, height: 0.46, gap: 0.49, shiftY: 0.1 },
};

// Coordinate space: 200x200 centered at L = 100
const VIEW = 200;
const L = 100;
const RING_SAMPLES = 96;
const TAU = Math.PI * 2;

type Pt = [number, number];

function clamp(v: number, lo: number, hi: number): number {
  return Math.max(lo, Math.min(hi, v));
}

function frac(v: number): number {
  return ((v % 1) + 1) % 1;
}

function smoothstep(t: number): number {
  const x = clamp(t, 0, 1);
  return x * x * (3 - 2 * x);
}

function easeInOut(t: number): number {
  const x = clamp(t, 0, 1);
  return x < 0.5 ? 2 * x * x : 1 - Math.pow(-2 * x + 2, 2) / 2;
}

// Critically damped spring step (omega = 14, zeta = 1 matching Grok Bot's `br`)
interface Spring {
  x: number;
  v: number;
  t: number;
}

function makeSpring(initial: number): Spring {
  return { x: initial, v: 0, t: initial };
}

function stepSpring(s: Spring, omega: number, zeta: number, dt: number): void {
  const f = 1 + 2 * dt * zeta * omega;
  const oo = omega * omega;
  const hoo = dt * oo;
  const hhoo = dt * hoo;
  const detInv = 1 / (f + hhoo);
  const detX = f * s.x + dt * s.v + hhoo * s.t;
  const detV = s.v + hoo * (s.t - s.x);
  s.x = detX * detInv;
  s.v = detV * detInv;
}

function samplePolar(radiusFn: (theta: number) => number, radius = 78): Pt[] {
  const pts: Pt[] = [];
  for (let i = 0; i < RING_SAMPLES; i++) {
    const th = (i / RING_SAMPLES) * TAU;
    const r = radiusFn(th) * radius;
    pts.push([L + Math.cos(th) * r, L + Math.sin(th) * r]);
  }
  return pts;
}

function raycastCircles(circles: Array<{ x: number; y: number; r: number }>, scale = 78): Pt[] {
  return samplePolar((th) => {
    const cx = Math.cos(th);
    const cy = Math.sin(th);
    let lo = 0;
    let hi = 1.25;
    for (let k = 0; k < 20; k++) {
      const mid = (lo + hi) * 0.5;
      const px = mid * cx;
      const py = mid * cy;
      if (circles.some((c) => (px - c.x) ** 2 + (py - c.y) ** 2 <= c.r * c.r)) {
        lo = mid;
      } else {
        hi = mid;
      }
    }
    return lo;
  }, scale);
}

function superellipse(rx: number, ry: number, n: number): Pt[] {
  return samplePolar((th) => {
    const c = Math.abs(Math.cos(th));
    const s = Math.abs(Math.sin(th));
    const denom = Math.pow(Math.pow(c / rx, n) + Math.pow(s / ry, n), 1 / n);
    return denom > 1e-6 ? 1 / denom : 1;
  }, 78);
}

function roundedPolygon(sides: number, rotOffset: number, roundness: number, rx = 1, ry = 1): Pt[] {
  const apothem = Math.cos(Math.PI / sides);
  return samplePolar((th) => {
    const a = th - rotOffset;
    const sector = TAU / sides;
    const local = ((a % sector) + sector) % sector - sector / 2;
    const polyR = apothem / Math.cos(local);
    return polyR * (1 - roundness) + roundness;
  }, 78).map(([x, y]) => [L + (x - L) * rx, L + (y - L) * ry]);
}

function resampleClosed(pts: Pt[], count: number): Pt[] {
  const segs: number[] = [0];
  let total = 0;
  for (let i = 0; i < pts.length; i++) {
    const a = pts[i];
    const b = pts[(i + 1) % pts.length];
    total += Math.hypot(b[0] - a[0], b[1] - a[1]);
    segs.push(total);
  }
  const out: Pt[] = [];
  let cursor = 0;
  for (let i = 0; i < count; i++) {
    const target = (i / count) * total;
    while (cursor < pts.length - 1 && segs[cursor + 1] < target) cursor++;
    const a = pts[cursor];
    const b = pts[(cursor + 1) % pts.length];
    const len = segs[cursor + 1] - segs[cursor];
    const t = len > 1e-6 ? (target - segs[cursor]) / len : 0;
    out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
  }
  return out;
}

// Build normalized 96-point rings for all 26 Grok Bot shapes
const SHAPE_RING_CACHE = new Map<GrokBotShapeId, Pt[]>();

function buildShapeRing(shape: GrokBotShapeId): Pt[] {
  switch (shape) {
    case "blob":
      return samplePolar(() => 1.0, 78);
    case "pebble":
      return superellipse(1.06, 0.92, 2.35);
    case "bean":
      return samplePolar((th) => {
        const c = Math.cos(th);
        const s = Math.sin(th);
        const indent = Math.exp(-Math.pow((s + 0.15) * 2.2, 2)) * (c > 0 ? 0.12 : 0.04);
        return 0.96 + 0.09 * Math.sin(th * 2 + 0.5) - indent;
      }, 78);
    case "egg":
      return samplePolar((th) => {
        const s = Math.sin(th);
        const width = 0.86 + 0.14 * s;
        return 1 / Math.hypot(Math.cos(th) / width, s / 1.04);
      }, 78);
    case "squircle":
      return superellipse(0.98, 0.98, 4.2);
    case "tablet":
      return superellipse(1.12, 0.78, 3.6);
    case "capsule":
      return superellipse(0.76, 1.06, 3.8);
    case "cylinder":
      return superellipse(0.84, 1.02, 6.2);
    case "hex":
      return roundedPolygon(6, -Math.PI / 2, 0.16, 1.0, 1.0);
    case "gem":
      return roundedPolygon(6, -Math.PI / 2, 0.08, 1.04, 0.96);
    case "crystal":
      return roundedPolygon(4, -Math.PI / 2, 0.1, 0.86, 1.08);
    case "wedge":
      return roundedPolygon(3, -Math.PI / 2, 0.24, 1.02, 0.98);
    case "shield":
      return samplePolar((th) => {
        const c = Math.cos(th);
        const s = Math.sin(th);
        if (s < 0) {
          const denom = Math.pow(Math.pow(Math.abs(c) / 0.95, 4.5) + Math.pow(Math.abs(s) / 0.92, 4.5), 1 / 4.5);
          return 1 / Math.max(denom, 0.5);
        }
        const taper = 0.96 - 0.34 * s * s;
        return 1 / Math.hypot(c / taper, s / 1.05);
      }, 78);
    case "dome":
      return samplePolar((th) => {
        const c = Math.cos(th);
        const s = Math.sin(th);
        if (s > 0.25) {
          const denom = Math.pow(Math.pow(Math.abs(c) / 1.0, 5.5) + Math.pow(Math.abs(s) / 0.82, 5.5), 1 / 5.5);
          return 1 / Math.max(denom, 0.5);
        }
        return 1 / Math.hypot(c / 1.0, s / 1.02);
      }, 78);
    case "arch":
      return samplePolar((th) => {
        const c = Math.cos(th);
        const s = Math.sin(th);
        if (s > 0) {
          const denom = Math.pow(Math.pow(Math.abs(c) / 0.9, 6.5) + Math.pow(Math.abs(s) / 0.96, 6.5), 1 / 6.5);
          return 1 / Math.max(denom, 0.5);
        }
        return 1 / Math.hypot(c / 0.9, s / 1.02);
      }, 78);
    case "cloud":
      return raycastCircles(
        [
          { x: 0, y: 0.08, r: 0.56 },
          { x: -0.46, y: 0.16, r: 0.42 },
          { x: 0.46, y: 0.16, r: 0.42 },
          { x: -0.26, y: -0.26, r: 0.44 },
          { x: 0.28, y: -0.22, r: 0.42 },
          { x: 0.0, y: -0.34, r: 0.44 },
        ],
        82,
      );
    case "teardrop":
      return samplePolar((th) => {
        const s = Math.sin(th);
        const c = Math.cos(th);
        const topPinch = s < 0 ? 1 - 0.55 * Math.pow(-s, 1.5) : 1;
        const rx = 0.92 * Math.max(0.28, topPinch);
        const ry = s < 0 ? 1.08 : 0.94;
        return 1 / Math.hypot(c / rx, s / ry);
      }, 78);
    case "leaf":
      return samplePolar((th) => {
        const rot = th + 0.45;
        const c = Math.cos(rot);
        const s = Math.sin(rot);
        const tip = 1 - 0.38 * Math.pow(Math.abs(c), 1.7);
        return 1 / Math.hypot(c / 1.08, s / (0.88 * Math.max(0.32, tip)));
      }, 78);
    case "square":
      return superellipse(0.95, 0.95, 7.0);
    case "sparkle":
      return samplePolar((th) => 0.72 + 0.34 * Math.pow(Math.abs(Math.cos(2 * th)), 1.5), 80);
    case "clover":
      return raycastCircles(
        [
          { x: 0, y: 0, r: 0.46 },
          { x: -0.4, y: -0.36, r: 0.46 },
          { x: 0.4, y: -0.36, r: 0.46 },
          { x: -0.4, y: 0.36, r: 0.46 },
          { x: 0.4, y: 0.36, r: 0.46 },
        ],
        80,
      );
    case "flower": {
      // Exact 6-petal flower circle union from Grok Bot's Rc()
      const circles: Array<{ x: number; y: number; r: number }> = [{ x: 0, y: 0, r: 0.44 }];
      for (let i = 0; i < 6; i++) {
        const a = -Math.PI / 2 + (i * TAU) / 6;
        circles.push({ x: 0.58 * Math.cos(a), y: 0.58 * Math.sin(a), r: 0.38 });
      }
      return raycastCircles(circles, 84);
    }
    case "heart": {
      const raw: Pt[] = [];
      for (let i = 0; i < 120; i++) {
        const t = (i / 120) * TAU;
        const x = 16 * Math.pow(Math.sin(t), 3);
        const y = -(13 * Math.cos(t) - 5 * Math.cos(2 * t) - 2 * Math.cos(3 * t) - Math.cos(4 * t));
        raw.push([L + x * 4.7, L + y * 4.7 - 4]);
      }
      return resampleClosed(raw, RING_SAMPLES);
    }
    case "house": {
      const verts: Pt[] = [
        [L, L - 78],
        [L + 74, L - 16],
        [L + 66, L + 72],
        [L - 66, L + 72],
        [L - 74, L - 16],
      ];
      // Round corners cleanly
      const smooth: Pt[] = [];
      for (let i = 0; i < verts.length; i++) {
        const prev = verts[(i + verts.length - 1) % verts.length];
        const curr = verts[i];
        const next = verts[(i + 1) % verts.length];
        const p1: Pt = [curr[0] + (prev[0] - curr[0]) * 0.22, curr[1] + (prev[1] - curr[1]) * 0.22];
        const p2: Pt = [curr[0] + (next[0] - curr[0]) * 0.22, curr[1] + (next[1] - curr[1]) * 0.22];
        for (let k = 0; k <= 6; k++) {
          const t = k / 6;
          const mt = 1 - t;
          smooth.push([
            mt * mt * p1[0] + 2 * mt * t * curr[0] + t * t * p2[0],
            mt * mt * p1[1] + 2 * mt * t * curr[1] + t * t * p2[1],
          ]);
        }
      }
      return resampleClosed(smooth, RING_SAMPLES);
    }
    case "star":
      return samplePolar((th) => 0.78 + 0.26 * Math.cos(6 * (th + Math.PI / 2)), 78);
    case "pumpkin":
      return raycastCircles(
        [
          { x: 0, y: 0.04, r: 0.66 },
          { x: -0.42, y: 0.06, r: 0.56 },
          { x: 0.42, y: 0.06, r: 0.56 },
          { x: 0, y: -0.68, r: 0.16 },
        ],
        80,
      );
  }
}

function getShapeRing(shape: GrokBotShapeId): Pt[] {
  let ring = SHAPE_RING_CACHE.get(shape);
  if (!ring) {
    ring = buildShapeRing(shape);
    SHAPE_RING_CACHE.set(shape, ring);
  }
  return ring;
}

// Circle ring and downward-pointing rounded triangle ring (for pencil tip morph)
const CIRCLE_RING: Pt[] = Array.from({ length: RING_SAMPLES }, (_, i) => {
  const a = (i / RING_SAMPLES) * TAU;
  return [L + Math.cos(a) * L, L + Math.sin(a) * L];
});

const PENCIL_TIP_RING: Pt[] = (() => {
  // Rounded inverted triangle pointing down (matching Grok Bot's rotated `qs(88, 18)`)
  const verts: Pt[] = [
    [L, L + 96],
    [L - 82, L - 56],
    [L + 82, L - 56],
  ];
  const smooth: Pt[] = [];
  for (let i = 0; i < 3; i++) {
    const prev = verts[(i + 2) % 3];
    const curr = verts[i];
    const next = verts[(i + 1) % 3];
    const r = i === 0 ? 0.24 : 0.26;
    const p1: Pt = [curr[0] + (prev[0] - curr[0]) * r, curr[1] + (prev[1] - curr[1]) * r];
    const p2: Pt = [curr[0] + (next[0] - curr[0]) * r, curr[1] + (next[1] - curr[1]) * r];
    for (let k = 0; k <= 8; k++) {
      const t = k / 8;
      const mt = 1 - t;
      smooth.push([
        mt * mt * p1[0] + 2 * mt * t * curr[0] + t * t * p2[0],
        mt * mt * p1[1] + 2 * mt * t * curr[1] + t * t * p2[1],
      ]);
    }
  }
  return resampleClosed(smooth, RING_SAMPLES);
})();

function ringToPath(pts: Pt[]): string {
  if (pts.length === 0) return "";
  let d = `M${pts[0][0].toFixed(1)} ${pts[0][1].toFixed(1)}`;
  for (let i = 1; i < pts.length; i++) {
    d += `L${pts[i][0].toFixed(1)} ${pts[i][1].toFixed(1)}`;
  }
  return d + "Z";
}

function catmullRomPath(pts: Pt[]): string {
  if (pts.length < 2) return "";
  let d = `M${pts[0][0].toFixed(1)} ${pts[0][1].toFixed(1)}`;
  if (pts.length === 2) {
    return `${d}L${pts[1][0].toFixed(1)} ${pts[1][1].toFixed(1)}`;
  }
  for (let i = 0; i < pts.length - 1; i++) {
    const p0 = pts[Math.max(i - 1, 0)];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[Math.min(i + 2, pts.length - 1)];
    const c1x = p1[0] + (p2[0] - p0[0]) / 6;
    const c1y = p1[1] + (p2[1] - p0[1]) / 6;
    const c2x = p2[0] - (p3[0] - p1[0]) / 6;
    const c2y = p2[1] - (p3[1] - p1[1]) / 6;
    d += `C${c1x.toFixed(1)} ${c1y.toFixed(1)} ${c2x.toFixed(1)} ${c2y.toFixed(1)} ${p2[0].toFixed(1)} ${p2[1].toFixed(1)}`;
  }
  return d;
}

// Capsule path centered at (L, L) (matching Grok Bot's `og(30, 88)`)
function capsulePath(width: number, height: number): string {
  const r = width / 2;
  const top = L - height / 2 + r;
  const bot = L + height / 2 - r;
  return `M${L - r} ${top}A${r} ${r} 0 0 1 ${L + r} ${top}L${L + r} ${bot}A${r} ${r} 0 0 1 ${L - r} ${bot}Z`;
}

// Tapered exclamation bar centered at (L, L) (matching Grok Bot's `sg(30, 17, 96)`)
function taperedBarPath(topW: number, botW: number, height: number): string {
  const r1 = topW / 2;
  const r2 = botW / 2;
  const top = L - height / 2;
  const bot = L + height / 2;
  return `M${L - r1} ${top + r1}A${r1} ${r1} 0 0 1 ${L + r1} ${top + r1}L${L + r2} ${bot - r2}A${r2} ${r2} 0 0 1 ${L - r2} ${bot - r2}Z`;
}

const PENCIL_SHAFT_PATH = capsulePath(30, 88);
const BANG_BAR_PATH = taperedBarPath(30, 17, 96);

// Exact Grok Bot morph constants from REA analysis
const MORPH_CORE_RADIUS: Record<GrokMorphKind, number> = {
  dots: 22,
  orbit: 19,
  radar: 19,
  progress: 19,
  gather: 19,
  wave: 16,
  send: 20,
  receive: 20,
  dock: 20,
  ball: 18,
  pencil: 17,
  bang: 13,
  standby: 13,
};

const DOTS_PERIOD_MS = 1400;
const DOTS_LIFT = 9;
const DOTS_SPACING = 62;
const PENCIL_PERIOD_MS = 2500;
const PENCIL_WRITE_FRAC = 0.68;
const PENCIL_WRITE_MS = PENCIL_PERIOD_MS * PENCIL_WRITE_FRAC;

function pencilPose(ms: number): { x: number; y: number; wig: number; rot: number } {
  const t = frac(ms / PENCIL_PERIOD_MS);
  if (t < PENCIL_WRITE_FRAC) {
    const n = t / PENCIL_WRITE_FRAC;
    const ease = smoothstep(n);
    const feather = clamp(n / 0.08, 0, 1) * clamp((1 - n) / 0.08, 0, 1);
    return {
      x: -54 + 118 * ease,
      y: 26,
      wig: Math.sin(n * 24) * 3.2 * feather,
      rot: 17 + Math.sin(ms * 0.0006) * 1,
    };
  }
  const n = easeInOut((t - PENCIL_WRITE_FRAC) / (1 - PENCIL_WRITE_FRAC));
  return {
    x: 64 - 118 * n,
    y: 26 - 20 * Math.sin(n * Math.PI),
    wig: 0,
    rot: 17 - 2 * Math.sin(n * Math.PI) + Math.sin(ms * 0.0006) * 1,
  };
}

function pencilTipPoint(ms: number): Pt {
  const p = pencilPose(ms);
  return [L + p.x, L + p.y + p.wig + 19];
}

// Precomputed ink samples for the writing stroke (matching Grok Bot's `Sg` & `Cg`)
const PENCIL_INK_SAMPLES: Array<{ at: number; point: Pt }> = (() => {
  const samples: Array<{ at: number; point: Pt }> = [];
  for (let t = 0; t < PENCIL_WRITE_MS; t += 12) {
    const pt = pencilTipPoint(t);
    const prev = samples[samples.length - 1];
    if (!prev || Math.hypot(pt[0] - prev.point[0], pt[1] - prev.point[1]) > 2.4) {
      samples.push({ at: t, point: pt });
    }
  }
  return samples;
})();

function pencilInkPoints(ms: number): Pt[] {
  const cycleMs = frac(ms / PENCIL_PERIOD_MS) * PENCIL_PERIOD_MS;
  const n = Math.min(cycleMs, PENCIL_WRITE_MS);
  const kept = PENCIL_INK_SAMPLES.filter((s) => s.at <= n);
  const pts = kept.map((s) => s.point);
  if (kept.length === 0 || n > kept[kept.length - 1].at) {
    pts.push(pencilTipPoint(n));
  }
  const trimmed = pts.slice(-64);
  if (cycleMs < PENCIL_WRITE_MS) return trimmed;
  const eraseCount = Math.floor(((cycleMs - PENCIL_WRITE_MS) / 1000) * 120);
  return trimmed.slice(eraseCount);
}

function dotsWave(stateTimeSec: number, idx: number, amount: number, reduced: boolean) {
  const phase = frac((stateTimeSec * 1000) / DOTS_PERIOD_MS + 0.119);
  const d = Math.abs(phase - idx / 3);
  const wrap = Math.min(d, 1 - d);
  const s = reduced ? 1 : Math.exp(-(wrap * wrap) / 0.045);
  const c = reduced ? 0 : 1;
  return {
    lift: s * DOTS_LIFT * amount * c,
    pop: 1 + c * (0.84 + 0.22 * s - 1),
    tone: 1 - c * 0.5 * (1 - s),
  };
}

function stateToMorph(state: GrokBotStateId): GrokMorphKind | null {
  switch (state) {
    case "thinking":
      return "dots";
    case "writing":
    case "editing":
      return "pencil";
    case "orbit":
    case "waiting":
      return "orbit";
    case "radar":
    case "searching":
      return "radar";
    case "progress":
      return "progress";
    case "spawning":
      return "gather";
    case "dictating":
      return "wave";
    case "sending":
      return "send";
    case "receiving":
      return "receive";
    case "uploading":
      return "dock";
    case "bouncing":
      return "ball";
    case "alerting":
      return "bang";
    case "powering-down":
      return "standby";
    default:
      return null;
  }
}

function stateToExpression(state: GrokBotStateId): GrokEyeExpression {
  switch (state) {
    case "listening":
    case "shy":
    case "humming":
      return "calm";
    case "finished":
    case "happy":
    case "excited":
    case "laughing":
    case "celebrate":
    case "proud":
    case "playful":
    case "approving":
      return "happy";
    case "curious":
      return "inspecting";
    case "surprised":
    case "waking":
    case "notifying":
    case "dragging":
    case "scared":
      return "surprised";
    case "sleeping":
    case "drowsy":
    case "bored":
      return "sleepy";
    case "suspicious":
    case "angry":
      return "squinted";
    case "sad":
    case "failed":
      return "sad";
    case "confused":
    case "blocked":
      return "confused";
    default:
      return "neutral";
  }
}

export interface GrokBotRenderOptions {
  shape: GrokBotShapeId;
  color: string;
  eyeColor?: string | null; // null/undefined = authentic cutout eyes
  state: GrokBotStateId;
  size: number;
  isFollowingPointer?: boolean;
  isReducedMotion?: boolean;
  isPaused?: boolean;
  seed?: number;
}

export interface GrokBotController {
  update(patch: Partial<GrokBotRenderOptions>): void;
  poke(kind?: "spin"): void;
  setGaze(gaze: [number, number] | null): void;
  destroy(): void;
  getFps(): number;
}

const SVG_NS = "http://www.w3.org/2000/svg";

function createSvgEl<K extends keyof SVGElementTagNameMap>(tag: K): SVGElementTagNameMap[K] {
  return document.createElementNS(SVG_NS, tag);
}

export function mountGrokBotFace(container: HTMLElement, initial: GrokBotRenderOptions): GrokBotController {
  let opts: GrokBotRenderOptions = { ...initial };
  let disposed = false;
  let rafId = 0;
  let lastTime = 0;
  let simTime = (initial.seed ?? 1) * 0.37;
  let stateTime = 0;
  let nextBlinkAt = simTime + 2.4 + ((initial.seed ?? 7) % 10) * 0.35;
  let blinkProgress = 0;

  // Springs
  const morphAmount = makeSpring(stateToMorph(opts.state) ? 1 : 0);
  const spinAngle = makeSpring(0);
  const gazeX = makeSpring(0);
  const gazeY = makeSpring(0);
  let pointerGaze: [number, number] | null = null;
  let activeMorph: GrokMorphKind | null = stateToMorph(opts.state);
  let spinDir = 1;

  // FPS tracking
  let fpsFrames = 0;
  let fpsAccum = 0;
  let currentFps = 60;

  // Build reusable SVG DOM tree (zero element creation during animation frames!)
  const uid = `gb-${Math.random().toString(36).slice(2, 9)}`;
  const svg = createSvgEl("svg");
  svg.setAttribute("viewBox", `0 0 ${VIEW} ${VIEW}`);
  svg.setAttribute("width", String(opts.size));
  svg.setAttribute("height", String(opts.size));
  svg.setAttribute("aria-hidden", "true");
  svg.style.display = "block";
  svg.style.overflow = "visible";

  const defs = createSvgEl("defs");
  const mask = createSvgEl("mask");
  mask.setAttribute("id", `${uid}-mask`);
  const maskBg = createSvgEl("rect");
  maskBg.setAttribute("x", "-40");
  maskBg.setAttribute("y", "-40");
  maskBg.setAttribute("width", String(VIEW + 80));
  maskBg.setAttribute("height", String(VIEW + 80));
  maskBg.setAttribute("fill", "#ffffff");
  const maskEyeL = createSvgEl("path");
  maskEyeL.setAttribute("fill", "#000000");
  const maskEyeR = createSvgEl("path");
  maskEyeR.setAttribute("fill", "#000000");
  mask.append(maskBg, maskEyeL, maskEyeR);
  defs.append(mask);

  const rootG = createSvgEl("g");
  const bodyPath = createSvgEl("path");
  const eyeG = createSvgEl("g");
  const paintEyeL = createSvgEl("path");
  const paintEyeR = createSvgEl("path");
  eyeG.append(paintEyeL, paintEyeR);

  // Morph auxiliary layer
  const morphG = createSvgEl("g");
  // Pre-allocated morph primitives:
  const mDotL = createSvgEl("circle");
  const mDotR = createSvgEl("circle");
  const mRing1 = createSvgEl("circle");
  const mRing2 = createSvgEl("circle");
  const mArc = createSvgEl("circle");
  const mSat = createSvgEl("circle");
  const mPencilShaft = createSvgEl("path");
  const mPencilInk = createSvgEl("path");
  const mBangBar = createSvgEl("path");
  const mHalo = createSvgEl("circle");
  const mWaveBars = Array.from({ length: 5 }, () => createSvgEl("rect"));
  const mGatherDots = Array.from({ length: 4 }, () => createSvgEl("circle"));

  for (const r of [mRing1, mRing2, mArc]) {
    r.setAttribute("fill", "none");
  }
  mPencilInk.setAttribute("fill", "none");
  mPencilInk.setAttribute("stroke-linecap", "round");
  mPencilInk.setAttribute("stroke-linejoin", "round");

  morphG.append(
    mHalo,
    mRing1,
    mRing2,
    mArc,
    mSat,
    mDotL,
    mDotR,
    mPencilInk,
    mPencilShaft,
    mBangBar,
    ...mWaveBars,
    ...mGatherDots,
  );

  rootG.append(bodyPath, eyeG);
  svg.append(defs, rootG, morphG);
  container.replaceChildren(svg);

  function hideAllMorphParts() {
    for (const el of [mDotL, mDotR, mRing1, mRing2, mArc, mSat, mPencilShaft, mPencilInk, mBangBar, mHalo, ...mWaveBars, ...mGatherDots]) {
      el.setAttribute("opacity", "0");
    }
  }

  function buildEyePath(
    cx: number,
    cy: number,
    w: number,
    h: number,
    expr: GrokEyeExpression,
    side: -1 | 1,
    blink: number,
  ): string {
    const openH = Math.max(2.2, h * (1 - blink * 0.92));
    if (expr === "happy") {
      // Smiling upward crescent arc
      const rx = w * 0.72;
      const ry = openH * 0.34;
      const stroke = Math.max(4.5, w * 0.34);
      const y0 = cy + ry * 0.35;
      return `M${(cx - rx).toFixed(1)} ${y0.toFixed(1)}Q${cx.toFixed(1)} ${(y0 - ry * 2.1).toFixed(1)} ${(cx + rx).toFixed(1)} ${y0.toFixed(1)}Q${cx.toFixed(1)} ${(y0 - ry * 2.1 + stroke * 1.45).toFixed(1)} ${(cx - rx).toFixed(1)} ${y0.toFixed(1)}Z`;
    }
    if (expr === "sleepy") {
      // Gentle downward sleepy arc
      const rx = w * 0.68;
      const ry = openH * 0.24;
      const stroke = Math.max(4.2, w * 0.3);
      const y0 = cy - ry * 0.2;
      return `M${(cx - rx).toFixed(1)} ${y0.toFixed(1)}Q${cx.toFixed(1)} ${(y0 + ry * 2.1).toFixed(1)} ${(cx + rx).toFixed(1)} ${y0.toFixed(1)}Q${cx.toFixed(1)} ${(y0 + ry * 2.1 - stroke * 1.4).toFixed(1)} ${(cx - rx).toFixed(1)} ${y0.toFixed(1)}Z`;
    }

    let rx = w * 0.5;
    let ry = openH * 0.5;
    let tilt = 0;

    if (expr === "surprised") {
      rx *= 1.18;
      ry *= 1.16;
    } else if (expr === "inspecting") {
      const k = side === -1 ? 1.15 : 0.78;
      rx *= k;
      ry *= k;
    } else if (expr === "sad") {
      tilt = side * -22;
      ry *= 0.88;
    } else if (expr === "squinted") {
      ry = Math.max(3.5, ry * 0.32);
      rx *= 1.15;
    } else if (expr === "calm") {
      ry *= 0.62;
      rx *= 1.05;
    } else if (expr === "confused") {
      tilt = side === -1 ? -14 : 18;
      ry *= side === -1 ? 0.92 : 0.74;
    }

    // Rounded capsule/oval oriented with tilt
    const rad = (tilt * Math.PI) / 180;
    const cos = Math.cos(rad);
    const sin = Math.sin(rad);
    const pts: Pt[] = [];
    const N = 24;
    for (let i = 0; i < N; i++) {
      const a = (i / N) * TAU;
      const lx = Math.cos(a) * rx;
      const ly = Math.sin(a) * ry;
      pts.push([cx + lx * cos - ly * sin, cy + lx * sin + ly * cos]);
    }
    return ringToPath(pts);
  }

  function renderFrame(dt: number) {
    const reduced = Boolean(opts.isReducedMotion);
    const stepDt = reduced ? 0 : Math.min(dt, 0.05);
    simTime += stepDt;
    stateTime += stepDt;

    const targetMorph = stateToMorph(opts.state);
    if (targetMorph !== activeMorph) {
      if (targetMorph != null && !reduced) {
        spinDir = -spinDir;
        spinAngle.t += Math.PI * spinDir;
      } else if (targetMorph == null && activeMorph != null && !reduced) {
        spinAngle.t += Math.PI * spinDir;
      }
      activeMorph = targetMorph;
      stateTime = 0;
    }
    morphAmount.t = targetMorph ? 1 : 0;

    // Gaze target (pointer or organic idle look-around)
    let gx = 0;
    let gy = 0;
    if (pointerGaze) {
      gx = pointerGaze[0];
      gy = pointerGaze[1];
    } else if (!reduced && !targetMorph) {
      if (opts.state === "reading" || opts.state === "coding" || opts.state === "working") {
        gx = Math.sin(simTime * 2.6) * 0.55;
        gy = 0.28 + Math.sin(simTime * 5.2) * 0.08;
      } else if (opts.state === "streaming") {
        gx = Math.sin(simTime * 1.4) * 0.28;
        gy = Math.sin(simTime * 2.8) * 0.14;
      } else if (opts.state === "turntable" || opts.state === "showcase") {
        gx = Math.sin(simTime * 1.7) * 0.72;
        gy = Math.cos(simTime * 1.1) * 0.25;
      } else {
        gx = Math.sin(simTime * 0.65) * 0.22 + Math.sin(simTime * 1.37) * 0.12;
        gy = Math.cos(simTime * 0.53) * 0.14;
      }
    }
    gazeX.t = gx;
    gazeY.t = gy;

    if (reduced) {
      morphAmount.x = morphAmount.t;
      spinAngle.x = spinAngle.t;
      gazeX.x = gazeX.t;
      gazeY.x = gazeY.t;
    } else {
      stepSpring(morphAmount, 14, 1, stepDt);
      stepSpring(spinAngle, 14, 1, stepDt);
      stepSpring(gazeX, 11, 0.95, stepDt);
      stepSpring(gazeY, 11, 0.95, stepDt);
    }

    // Natural blinking
    if (!reduced && !targetMorph) {
      if (simTime >= nextBlinkAt) {
        const elapsed = simTime - nextBlinkAt;
        if (elapsed < 0.18) {
          blinkProgress = Math.sin((elapsed / 0.18) * Math.PI);
        } else {
          blinkProgress = 0;
          nextBlinkAt = simTime + 2.8 + Math.abs(Math.sin(simTime * 13.7)) * 3.4;
        }
      }
    } else {
      blinkProgress = 0;
    }

    const mAmt = clamp(morphAmount.x, 0, 1);
    const kind = activeMorph;

    // Subtle body motion per state
    let bobY = 0;
    let tiltDeg = 0;
    let scaleX = 1;
    let scaleY = 1;
    if (!reduced && mAmt < 0.9) {
      const k = 1 - mAmt;
      if (opts.state === "streaming" || opts.state === "excited" || opts.state === "laughing") {
        bobY = Math.sin(simTime * 6.2) * 5.5 * k;
        tiltDeg = Math.sin(simTime * 3.1) * 6.5 * k;
        scaleY = 1 + Math.sin(simTime * 6.2) * 0.035 * k;
        scaleX = 1 - Math.sin(simTime * 6.2) * 0.025 * k;
      } else if (opts.state === "celebrate") {
        bobY = -Math.abs(Math.sin(simTime * 5.2)) * 12 * k;
        tiltDeg = Math.sin(simTime * 5.2) * 12 * k;
      } else if (opts.state === "approving") {
        bobY = Math.sin(simTime * 7.0) * 6 * k;
      } else if (opts.state === "sleeping" || opts.state === "drowsy") {
        bobY = Math.sin(simTime * 1.4) * 2.5 * k;
        scaleY = 1 + Math.sin(simTime * 1.4) * 0.02 * k;
      } else if (opts.state === "failed" || opts.state === "confused" || opts.state === "angry") {
        tiltDeg = Math.sin(simTime * 9.0) * 4.5 * k;
      } else {
        bobY = Math.sin(simTime * 2.1) * 2.4 * k;
        tiltDeg = gazeX.x * 5.5 * k;
      }
    }

    // Morph core transform & morph target ring
    const targetRing = kind === "pencil" ? PENCIL_TIP_RING : CIRCLE_RING;
    const baseRing = getShapeRing(opts.shape);
    const round = smoothstep(clamp(mAmt / 0.72, 0, 1));

    let coreScale = 1;
    let coreDx = 0;
    let coreDy = 0;
    let coreRotDeg = 0;
    let coreOpacity = 1;

    hideAllMorphParts();

    if (kind && mAmt > 0.005) {
      const targetR = MORPH_CORE_RADIUS[kind];
      let pop = 1;
      const ms = stateTime * 1000;

      if (kind === "dots") {
        const center = dotsWave(stateTime, 1, mAmt, reduced);
        pop = center.pop;
        coreDy -= center.lift;
        coreOpacity = 1 - (1 - center.tone) * mAmt;

        const left = dotsWave(stateTime, 0, mAmt, reduced);
        const right = dotsWave(stateTime, 2, mAmt, reduced);
        const spread = DOTS_SPACING * smoothstep(mAmt);
        mDotL.setAttribute("cx", (L - spread).toFixed(1));
        mDotL.setAttribute("cy", (L - left.lift).toFixed(1));
        mDotL.setAttribute("r", (targetR * left.pop * mAmt).toFixed(2));
        mDotL.setAttribute("fill", opts.color);
        mDotL.setAttribute("opacity", (mAmt * left.tone).toFixed(3));

        mDotR.setAttribute("cx", (L + spread).toFixed(1));
        mDotR.setAttribute("cy", (L - right.lift).toFixed(1));
        mDotR.setAttribute("r", (targetR * right.pop * mAmt).toFixed(2));
        mDotR.setAttribute("fill", opts.color);
        mDotR.setAttribute("opacity", (mAmt * right.tone).toFixed(3));
      } else if (kind === "pencil") {
        const pose = pencilPose(reduced ? PENCIL_WRITE_MS * 0.55 : ms);
        coreDx = pose.x * mAmt;
        coreDy = (pose.y + pose.wig * 0.5) * mAmt;
        coreRotDeg = pose.rot * mAmt;

        const rad = ((pose.rot - 90) * Math.PI) / 180;
        const sx = L + (pose.x + Math.cos(rad) * 68) * mAmt;
        const sy = L + (pose.y + pose.wig * 0.15 + Math.sin(rad) * 68) * mAmt;
        mPencilShaft.setAttribute("d", PENCIL_SHAFT_PATH);
        mPencilShaft.setAttribute("fill", opts.color);
        mPencilShaft.setAttribute(
          "transform",
          `translate(${sx.toFixed(1)} ${sy.toFixed(1)}) rotate(${(pose.rot * mAmt).toFixed(1)}) scale(${smoothstep(mAmt).toFixed(3)}) translate(${-L} ${-L})`,
        );
        mPencilShaft.setAttribute("opacity", clamp(mAmt * 1.6 - 0.3, 0, 1).toFixed(3));

        const inkPts = pencilInkPoints(reduced ? PENCIL_WRITE_MS * 0.55 : ms);
        if (inkPts.length >= 2) {
          mPencilInk.setAttribute("d", catmullRomPath(inkPts));
          mPencilInk.setAttribute("stroke", opts.color);
          mPencilInk.setAttribute("stroke-width", "6");
          mPencilInk.setAttribute("opacity", clamp(mAmt * 1.2, 0, 1).toFixed(3));
        }
      } else if (kind === "orbit") {
        const angle = reduced ? 0.6 : stateTime * 2.8;
        const r = 56 * smoothstep(mAmt);
        mRing1.setAttribute("cx", String(L));
        mRing1.setAttribute("cy", String(L));
        mRing1.setAttribute("r", r.toFixed(1));
        mRing1.setAttribute("stroke", opts.color);
        mRing1.setAttribute("stroke-width", "6");
        mRing1.setAttribute("opacity", (0.22 * mAmt).toFixed(3));

        mSat.setAttribute("cx", (L + Math.cos(angle) * r).toFixed(1));
        mSat.setAttribute("cy", (L + Math.sin(angle) * r).toFixed(1));
        mSat.setAttribute("r", (11 * mAmt).toFixed(1));
        mSat.setAttribute("fill", opts.color);
        mSat.setAttribute("opacity", mAmt.toFixed(3));
      } else if (kind === "radar") {
        const p1 = frac(stateTime * 0.65);
        const p2 = frac(stateTime * 0.65 + 0.5);
        for (const [el, p] of [
          [mRing1, p1],
          [mRing2, p2],
        ] as const) {
          el.setAttribute("cx", String(L));
          el.setAttribute("cy", String(L));
          el.setAttribute("r", (24 + p * 64).toFixed(1));
          el.setAttribute("stroke", opts.color);
          el.setAttribute("stroke-width", (6 * (1 - p * 0.5)).toFixed(2));
          el.setAttribute("opacity", (Math.sin(p * Math.PI) * 0.55 * mAmt).toFixed(3));
        }
      } else if (kind === "progress") {
        const r = 54 * smoothstep(mAmt);
        const circ = TAU * r;
        const prog = reduced ? 0.65 : frac(stateTime / 2.5);
        mRing1.setAttribute("cx", String(L));
        mRing1.setAttribute("cy", String(L));
        mRing1.setAttribute("r", r.toFixed(1));
        mRing1.setAttribute("stroke", opts.color);
        mRing1.setAttribute("stroke-width", "8");
        mRing1.setAttribute("opacity", (0.18 * mAmt).toFixed(3));

        mArc.setAttribute("cx", String(L));
        mArc.setAttribute("cy", String(L));
        mArc.setAttribute("r", r.toFixed(1));
        mArc.setAttribute("stroke", opts.color);
        mArc.setAttribute("stroke-width", "8");
        mArc.setAttribute("stroke-linecap", "round");
        mArc.setAttribute("stroke-dasharray", `${(circ * Math.max(0.08, prog)).toFixed(1)} ${circ.toFixed(1)}`);
        mArc.setAttribute("transform", `rotate(${(-90 + stateTime * 90).toFixed(1)} ${L} ${L})`);
        mArc.setAttribute("opacity", mAmt.toFixed(3));
      } else if (kind === "wave") {
        const indices = [-2, -1, 0, 1, 2];
        const env = 0.42 + 0.29 * Math.sin(ms * 0.0021) * Math.sin(ms * 0.0034) + 0.29 * Math.sin(ms * 0.0013 + 1.7);
        indices.forEach((slot, idx) => {
          const el = mWaveBars[idx];
          if (slot === 0) {
            el.setAttribute("opacity", "0");
            return;
          }
          const level = reduced ? 0.6 : env * (0.55 + 0.45 * Math.sin(ms * 0.012 - Math.abs(slot) * 1.05));
          const h = 24 + level * 68;
          const w = 16;
          const x = L + slot * 30 * smoothstep(mAmt) - w / 2;
          const y = L - h / 2;
          el.setAttribute("x", x.toFixed(1));
          el.setAttribute("y", y.toFixed(1));
          el.setAttribute("width", String(w));
          el.setAttribute("height", h.toFixed(1));
          el.setAttribute("rx", String(w / 2));
          el.setAttribute("fill", opts.color);
          el.setAttribute("opacity", mAmt.toFixed(3));
        });
      } else if (kind === "gather") {
        const p = reduced ? 0.5 : frac(stateTime / 1.8);
        const dist = 70 * (1 - easeInOut(p));
        mGatherDots.forEach((dot, i) => {
          const a = (i / 4) * TAU + stateTime * 1.6;
          dot.setAttribute("cx", (L + Math.cos(a) * dist).toFixed(1));
          dot.setAttribute("cy", (L + Math.sin(a) * dist).toFixed(1));
          dot.setAttribute("r", (11 * (0.4 + 0.6 * (1 - p)) * mAmt).toFixed(1));
          dot.setAttribute("fill", opts.color);
          dot.setAttribute("opacity", (Math.sin(p * Math.PI) * mAmt).toFixed(3));
        });
      } else if (kind === "send" || kind === "receive") {
        const dir = kind === "send" ? -1 : 1;
        const p = frac(ms / (kind === "send" ? 1500 : 1700));
        coreDy = dir * Math.sin(p * Math.PI) * 18 * mAmt;
        mRing1.setAttribute("cx", String(L));
        mRing1.setAttribute("cy", (L + dir * (p * 54 - 22)).toFixed(1));
        mRing1.setAttribute("r", (22 + p * 26).toFixed(1));
        mRing1.setAttribute("stroke", opts.color);
        mRing1.setAttribute("stroke-width", "6");
        mRing1.setAttribute("opacity", (Math.sin(p * Math.PI) * 0.45 * mAmt).toFixed(3));
      } else if (kind === "dock") {
        const p = frac(ms / 1600);
        coreDy = -16 * Math.sin(p * Math.PI) * mAmt;
        const bar = mWaveBars[0];
        bar.setAttribute("x", String(L - 44));
        bar.setAttribute("y", String(L + 44));
        bar.setAttribute("width", "88");
        bar.setAttribute("height", "12");
        bar.setAttribute("rx", "6");
        bar.setAttribute("fill", opts.color);
        bar.setAttribute("opacity", mAmt.toFixed(3));
      } else if (kind === "ball") {
        const t = frac(stateTime / 0.62);
        coreDy = (40 - 208 * t * (1 - t)) * mAmt;
      } else if (kind === "bang") {
        const r = smoothstep(clamp(mAmt * 1.1, 0, 1));
        const decay = Math.exp(-(stateTime % 2.2) * 5.5);
        const wob = reduced ? 0 : Math.sin(stateTime * 42) * 2.2 * decay;
        pop = 1 + 0.04 * decay;
        coreDy = 58 * mAmt;
        mBangBar.setAttribute("d", BANG_BAR_PATH);
        mBangBar.setAttribute("fill", opts.color);
        mBangBar.setAttribute(
          "transform",
          `translate(0 ${(-26 - (1 - r) * 70).toFixed(1)}) rotate(${wob.toFixed(2)} ${L} ${L - 74}) translate(${L} ${L}) scale(${clamp(mAmt * 1.2, 0, 1).toFixed(3)}) translate(${-L} ${-L})`,
        );
        mBangBar.setAttribute("opacity", clamp(mAmt * 1.5 - 0.2, 0, 1).toFixed(3));
      } else if (kind === "standby") {
        const pulse = 0.5 + 0.5 * Math.sin(ms * 0.0016);
        coreOpacity = 1 - (0.28 + 0.2 * Math.sin(ms * 0.0016)) * mAmt;
        mHalo.setAttribute("cx", String(L));
        mHalo.setAttribute("cy", String(L));
        mHalo.setAttribute("r", (26 + 7 * pulse).toFixed(1));
        mHalo.setAttribute("fill", opts.color);
        mHalo.setAttribute("opacity", (smoothstep(mAmt) * (0.08 + 0.12 * pulse)).toFixed(3));
      }

      coreScale = 1 - mAmt + (targetR / L) * pop * mAmt;
    }

    // Interpolate body outline ring between shape & morph target
    const blendedRing: Pt[] = new Array(RING_SAMPLES);
    const bodyShiftX = gazeX.x * 4.5 * (1 - mAmt);
    const bodyShiftY = gazeY.x * 3.5 * (1 - mAmt);
    for (let i = 0; i < RING_SAMPLES; i++) {
      const a = baseRing[i];
      const b = targetRing[i];
      const x = a[0] + (b[0] - a[0]) * round;
      const y = a[1] + (b[1] - a[1]) * round;
      blendedRing[i] = [L + (x - L) * scaleX + bodyShiftX, L + (y - L) * scaleY + bodyShiftY];
    }

    const totalRot = tiltDeg + coreRotDeg + (spinAngle.x * 180) / Math.PI * (mAmt > 0.01 && mAmt < 0.99 ? 1 : 0);
    bodyPath.setAttribute("d", ringToPath(blendedRing));
    bodyPath.setAttribute("fill", opts.color);
    bodyPath.setAttribute("opacity", coreOpacity.toFixed(3));

    rootG.setAttribute(
      "transform",
      `translate(${(L + coreDx).toFixed(2)} ${(L + bobY + coreDy).toFixed(2)}) rotate(${totalRot.toFixed(2)}) scale(${coreScale.toFixed(4)}) translate(${-L} ${-L})`,
    );

    // Eyes (visible when morph amount < 0.5, matching Grok Bot's `isFaceVisible: i < .5`)
    const faceVisible = mAmt < 0.5;
    if (faceVisible) {
      const m = SHAPE_EYE_METRICS[opts.shape];
      const eyeScale = 76;
      const eyeW = m.width * eyeScale;
      const eyeH = m.height * eyeScale;
      const halfGap = (m.gap * eyeScale) / 2;
      const eyeShiftX = gazeX.x * 16;
      const eyeShiftY = m.shiftY * 32 + gazeY.x * 12;
      const expr = stateToExpression(opts.state);

      // Subtle 3D foreshortening of outer/inner eye when looking left/right
      const leftScaleX = 1 - Math.max(0, -gazeX.x) * 0.16;
      const rightScaleX = 1 - Math.max(0, gazeX.x) * 0.16;

      const dL = buildEyePath(L - halfGap + eyeShiftX, L + eyeShiftY, eyeW * leftScaleX, eyeH, expr, -1, blinkProgress);
      const dR = buildEyePath(L + halfGap + eyeShiftX, L + eyeShiftY, eyeW * rightScaleX, eyeH, expr, 1, blinkProgress);

      if (opts.eyeColor) {
        bodyPath.removeAttribute("mask");
        eyeG.setAttribute("opacity", String(1 - mAmt * 2));
        paintEyeL.setAttribute("d", dL);
        paintEyeR.setAttribute("d", dR);
        paintEyeL.setAttribute("fill", opts.eyeColor);
        paintEyeR.setAttribute("fill", opts.eyeColor);
      } else {
        bodyPath.setAttribute("mask", `url(#${uid}-mask)`);
        eyeG.setAttribute("opacity", "0");
        maskEyeL.setAttribute("d", dL);
        maskEyeR.setAttribute("d", dR);
      }
    } else {
      bodyPath.removeAttribute("mask");
      eyeG.setAttribute("opacity", "0");
    }
  }

  function loop(now: number) {
    rafId = 0;
    if (disposed) return;
    if (!lastTime) lastTime = now;
    const dt = (now - lastTime) / 1000;
    lastTime = now;

    fpsFrames++;
    fpsAccum += dt;
    if (fpsAccum >= 0.5) {
      currentFps = Math.round(fpsFrames / fpsAccum);
      fpsFrames = 0;
      fpsAccum = 0;
    }

    if (!opts.isPaused) {
      renderFrame(dt);
      rafId = requestAnimationFrame(loop);
    }
  }

  function onPointerMove(e: PointerEvent) {
    if (!opts.isFollowingPointer) return;
    const rect = svg.getBoundingClientRect();
    const radius = Math.max(rect.width, 1) * 2;
    const nx = clamp((e.clientX - (rect.left + rect.width / 2)) / radius, -1, 1);
    const ny = clamp((e.clientY - (rect.top + rect.height / 2)) / radius, -1, 1);
    pointerGaze = [nx, ny];
  }

  function onPointerLeave() {
    pointerGaze = null;
  }

  window.addEventListener("pointermove", onPointerMove, { passive: true });
  window.addEventListener("pointerleave", onPointerLeave);

  renderFrame(0);
  if (!opts.isPaused) {
    rafId = requestAnimationFrame(loop);
  }

  return {
    update(patch) {
      const wasPaused = opts.isPaused;
      opts = { ...opts, ...patch };
      svg.setAttribute("width", String(opts.size));
      svg.setAttribute("height", String(opts.size));
      if (!opts.isFollowingPointer) pointerGaze = null;
      renderFrame(0);
      if (wasPaused && !opts.isPaused && !rafId) {
        lastTime = 0;
        rafId = requestAnimationFrame(loop);
      }
    },
    poke() {
      if (!opts.isReducedMotion) {
        spinDir = -spinDir;
        spinAngle.t += TAU * spinDir;
      }
    },
    setGaze(gaze) {
      pointerGaze = gaze;
    },
    getFps() {
      return currentFps;
    },
    destroy() {
      disposed = true;
      cancelAnimationFrame(rafId);
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerleave", onPointerLeave);
      container.replaceChildren();
    },
  };
}
