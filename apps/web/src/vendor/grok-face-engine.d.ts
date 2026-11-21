export type GrokEngineState =
  | "idle"
  | "listening"
  | "thinking"
  | "streaming"
  | "searching"
  | "reading"
  | "working"
  | "coding"
  | "running"
  | "computing"
  | "controlling"
  | "waiting"
  | "blocked"
  | "finished"
  | "failed"
  | "sleeping"
  | "waking"
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
  | "orbit"
  | "radar"
  | "progress"
  | "spawning"
  | "loading"
  | "dictating"
  | "sending"
  | "receiving"
  | "uploading"
  | "writing"
  | "alerting"
  | "bouncing"
  | "powering-down"
  | "humming"
  | "notifying"
  | "dragging"
  | "logo"
  | "showcase"
  | "turntable";

export type GrokEngineShape =
  | "blob"
  | "pebble"
  | "bean"
  | "egg"
  | "squircle"
  | "tablet"
  | "capsule"
  | "cylinder"
  | "hex"
  | "gem"
  | "crystal"
  | "wedge"
  | "shield"
  | "dome"
  | "arch"
  | "cloud"
  | "teardrop"
  | "leaf"
  | "square"
  | "sparkle"
  | "clover"
  | "flower"
  | "heart"
  | "house"
  | "star"
  | "pumpkin";

export type GrokEngineExpression =
  | "neutral"
  | "happy"
  | "surprised"
  | "inspecting"
  | "sad"
  | "sleepy"
  | "squinted"
  | "calm"
  | "confused";

export interface GrokAppearance {
  ink: [number, number, number];
  spot: [number, number, number];
  cutout: boolean;
  inkPaint: string;
  spotPaint?: string;
}

export interface GrokHostConfig {
  id: string;
  seed?: string;
  body: string;
  appearance: GrokAppearance;
  state?: GrokEngineState;
  clip?: unknown;
  morph?: string | null;
  size: number | string;
  fill?: number;
  resolution?: number;
  maxFrameRate?: number;
  isPlaying?: boolean;
  isEmphasized?: boolean;
  isFollowingPointer?: boolean;
  isStatic?: boolean;
  isProof?: boolean;
}

export interface GrokHostController {
  attach(target: { host: HTMLElement; canvas: HTMLElement }): () => void;
  update(patch: Partial<GrokHostConfig>): void;
  setState(state: GrokEngineState): void;
  setBody(body: string): void;
  setPlaying(playing: boolean): void;
  setExpression(expr: GrokEngineExpression | null, opts?: unknown): void;
  blink(): void;
  look(direction: string): void;
  restart(): void;
  poke(kind: string, opts?: unknown): void;
  gaze(point: [number, number] | null): void;
}

export declare function createHost(config: GrokHostConfig): GrokHostController;
export declare function bodyForShape(shape: string): string;
export declare function appearance(inkPaint: string, spotPaint?: string): GrokAppearance;
export declare function renderSvg(
  frame: unknown,
  appearance: GrokAppearance,
  resolution: number,
  id: string,
  opts?: Record<string, unknown>,
): string;
export declare function createMotion(opts: { body: string }): {
  frame(): unknown;
  advance(dt: number): void;
  setBody(body: string, opts?: { isInstant?: boolean }): void;
  setMorph(morph: string | null, opts?: { isInstant?: boolean }): void;
  pause(): void;
  play(): void;
};
export declare const shapes: readonly string[];
export declare const bodies: readonly string[];
export declare const stateNames: readonly GrokEngineState[];
export declare const expressions: readonly GrokEngineExpression[];
export declare const morphKinds: Record<string, string>;
export declare const colors: readonly Array<{ id: string; label: string; value: string }>;
export declare const themePrimitives: {
  light: Record<string, string>;
  dark: Record<string, string>;
};
