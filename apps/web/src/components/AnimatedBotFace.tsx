import React, { memo, useEffect, useId, useRef } from "react";
import { resolveBotColor } from "@grok-bot/theme";
import { resolveGrokBotShape } from "../lib/botState";
import type {
  GrokEngineState,
  GrokHostController,
} from "../vendor/grok-face-engine";

export interface AnimatedBotFaceProps {
  seed: string;
  shapeId?: string | null;
  colorId?: string | null;
  size?: number;
  /** Full Grok Bot 1.2.3 state (53 states). Takes precedence over `working`. */
  state?: GrokEngineState;
  /** Backward-compatible boolean fallback (`true` -> `"thinking"`, `false` -> `"idle"`). */
  working?: boolean;
  /** Follow cursor across the viewport (used on welcome / hero faces). */
  isFollowingPointer?: boolean;
  /** Emphasize face expression (e.g. hover / highlight). */
  isEmphasized?: boolean;
  /** Pause animation playback (e.g. archived / sleeping / static thumbnail). */
  isPaused?: boolean;
  /** Render static first frame without animation loop (ideal for shape pickers). */
  isStatic?: boolean;
  /** Optional custom eye color. Omit for authentic Grok Bot cutout eyes. */
  eyeColor?: string;
  /** Cap frame rate (60 for hero/active, 30 for compact sidebar items). */
  maxFrameRate?: number;
  /** Allow clicking the bot to trigger Grok Bot's spin poke. */
  clickableSpin?: boolean;
  className?: string;
}

type EngineModule = typeof import("../vendor/grok-face-engine.js");
let enginePromise: Promise<EngineModule> | null = null;

function loadEngine(): Promise<EngineModule> {
  if (!enginePromise) {
    enginePromise = import("../vendor/grok-face-engine.js");
  }
  return enginePromise;
}

function resolveInkPaint(seed: string, colorId?: string | null): string {
  const preset = resolveBotColor(seed, colorId);
  // Match Grok Bot's KS(color): "black" adapts to light/dark neutral ink via CSS var.
  if (preset.id === "black") {
    return "var(--grok-bot-text-primary, #1c1c1c)";
  }
  return preset.hex;
}

export const AnimatedBotFace = memo(function AnimatedBotFace({
  seed,
  shapeId,
  colorId,
  size = 32,
  state,
  working = false,
  isFollowingPointer = false,
  isEmphasized = false,
  isPaused = false,
  isStatic = false,
  eyeColor,
  maxFrameRate,
  clickableSpin = false,
  className,
}: AnimatedBotFaceProps) {
  const uid = useId().replace(/[^a-zA-Z0-9_-]/g, "");
  const hostRef = useRef<HTMLSpanElement | null>(null);
  const canvasRef = useRef<HTMLSpanElement | null>(null);
  const controllerRef = useRef<GrokHostController | null>(null);
  const engineRef = useRef<EngineModule | null>(null);

  const resolvedShape = resolveGrokBotShape(shapeId, seed);
  const resolvedState: GrokEngineState = state ?? (working ? "thinking" : "idle");
  const inkPaint = resolveInkPaint(seed, colorId);
  const effectiveFps = maxFrameRate ?? (size < 40 ? 30 : 60);

  const latestRef = useRef({
    resolvedShape,
    resolvedState,
    inkPaint,
    eyeColor,
    size,
    isFollowingPointer,
    isEmphasized,
    isPaused,
    isStatic,
    effectiveFps,
  });
  latestRef.current = {
    resolvedShape,
    resolvedState,
    inkPaint,
    eyeColor,
    size,
    isFollowingPointer,
    isEmphasized,
    isPaused,
    isStatic,
    effectiveFps,
  };

  useEffect(() => {
    const hostEl = hostRef.current;
    const canvasEl = canvasRef.current;
    if (!hostEl || !canvasEl) return;

    let disposed = false;
    let detach: (() => void) | null = null;
    let scrollTimer: ReturnType<typeof setTimeout> | undefined;
    let scrolling = false;

    const onScroll = () => {
      if (latestRef.current.isStatic || latestRef.current.isPaused) return;
      if (!scrolling) {
        scrolling = true;
        controllerRef.current?.setPlaying(false);
      }
      if (scrollTimer) clearTimeout(scrollTimer);
      scrollTimer = setTimeout(() => {
        scrolling = false;
        if (!disposed && !latestRef.current.isPaused && !latestRef.current.isStatic) {
          controllerRef.current?.setPlaying(true);
        }
      }, 150);
    };

    document.addEventListener("scroll", onScroll, true);

    void loadEngine()
      .then((eng) => {
        if (disposed) return;
        engineRef.current = eng;
        const cur = latestRef.current;
        const app = cur.eyeColor
          ? eng.appearance(cur.inkPaint, cur.eyeColor)
          : eng.appearance(cur.inkPaint);

        const ctrl = eng.createHost({
          id: `gb-${uid}`,
          seed,
          body: eng.bodyForShape(cur.resolvedShape),
          appearance: app,
          state: cur.resolvedState,
          size: cur.size,
          fill: 0.92,
          maxFrameRate: cur.effectiveFps,
          isPlaying: !cur.isPaused && !cur.isStatic && !scrolling,
          isEmphasized: cur.isEmphasized,
          isFollowingPointer: cur.isFollowingPointer,
          isStatic: cur.isStatic,
        });
        controllerRef.current = ctrl;
        detach = ctrl.attach({ host: hostEl, canvas: canvasEl });
        hostEl.dataset.engineReady = "true";
      })
      .catch(() => {
        /* Keep static BotMark fallback if the engine chunk fails to load. */
      });

    return () => {
      disposed = true;
      if (scrollTimer) clearTimeout(scrollTimer);
      document.removeEventListener("scroll", onScroll, true);
      detach?.();
      controllerRef.current = null;
      delete hostEl.dataset.engineReady;
    };
  }, [seed, uid]);

  useEffect(() => {
    const ctrl = controllerRef.current;
    const eng = engineRef.current;
    if (!ctrl || !eng) return;
    ctrl.update({
      body: eng.bodyForShape(resolvedShape),
      appearance: eyeColor ? eng.appearance(inkPaint, eyeColor) : eng.appearance(inkPaint),
      state: resolvedState,
      size,
      fill: 0.92,
      maxFrameRate: effectiveFps,
      isPlaying: !isPaused && !isStatic,
      isEmphasized,
      isFollowingPointer,
      isStatic,
    });
  }, [
    resolvedShape,
    resolvedState,
    inkPaint,
    eyeColor,
    size,
    effectiveFps,
    isPaused,
    isStatic,
    isEmphasized,
    isFollowingPointer,
  ]);

  const handleClick = clickableSpin
    ? () => {
        controllerRef.current?.poke("spin");
      }
    : undefined;

  return (
    <span
      ref={hostRef}
      className={className ? `gb-animated-face ${className}` : "gb-animated-face"}
      role="img"
      aria-label={`${seed}, ${resolvedState}`}
      data-grok-state={resolvedState}
      onClick={handleClick}
      style={
        {
          width: size,
          height: size,
          "--fg": inkPaint,
          cursor: clickableSpin ? "pointer" : undefined,
        } as React.CSSProperties
      }
    >
      <span className="gb-animated-face__renderer" ref={canvasRef} />
    </span>
  );
});
