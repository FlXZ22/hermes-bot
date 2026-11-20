import React, { memo, useEffect, useRef } from "react";
import { BotMark, getBotMarkShape, resolveBotColor } from "@grok-bot/theme";
import type { FaceRenderer } from "../vendor/grok-face-runtime";

export const AnimatedBotFace = memo(function AnimatedBotFace({ seed, shapeId, colorId, size = 32, working = false }: {
  seed: string; shapeId?: string | null; colorId?: string | null; size?: number; working?: boolean;
}) {
  const host = useRef<HTMLSpanElement>(null);
  const renderer = useRef<FaceRenderer | null>(null);
  const selectedShape = getBotMarkShape(shapeId, seed).id;
  const shape = selectedShape === "clover" ? "cloud" : selectedShape;
  const color = resolveBotColor(seed, colorId).hex;
  const state = working ? "thinking" : "idle";
  const latest = useRef({ shape, state });
  latest.current = { shape, state };
  useEffect(() => {
    const mount = host.current;
    if (!mount) return;
    let disposed = false, visible = true, frame = 0, previous = 0;
    let resume: ReturnType<typeof setTimeout> | undefined;
    const motion = matchMedia("(prefers-reduced-motion: reduce)");
    const stop = () => { cancelAnimationFrame(frame); frame = 0; previous = 0; };
    const tick = (time: number) => {
      if (disposed || !visible || document.hidden || motion.matches) { stop(); return; }
      if (!previous) previous = time;
      if (time - previous >= 1000 / 30) {
        renderer.current?.advance(Math.min(50, time - previous)); previous = time;
      }
      frame = requestAnimationFrame(tick);
    };
    const sync = () => {
      stop();
      if (renderer.current && visible && !document.hidden && !motion.matches) frame = requestAnimationFrame(tick);
    };
    const onScroll = () => { stop(); clearTimeout(resume); resume = setTimeout(sync, 160); };
    const observer = new IntersectionObserver(([entry]) => { visible = entry.isIntersecting; sync(); });
    observer.observe(mount);
    document.addEventListener("visibilitychange", sync);
    document.addEventListener("scroll", onScroll, true);
    motion.addEventListener("change", sync);
    void import("../vendor/grok-face-runtime.js").then(({ createGrokRenderer }) => {
      if (disposed) return;
      const face = createGrokRenderer(mount, { ...latest.current, seed: Array.from(seed).reduce((n,c) => n + c.charCodeAt(0), 724) });
      renderer.current = face;
      face.svg.setAttribute("aria-hidden", "true");
      face.advance(1);
      sync();
    }).catch(() => { /* Keep the static avatar when the animation chunk is unavailable. */ });
    return () => {
      disposed = true; stop(); clearTimeout(resume); observer.disconnect();
      document.removeEventListener("visibilitychange", sync);
      document.removeEventListener("scroll", onScroll, true);
      motion.removeEventListener("change", sync);
      renderer.current?.destroy(); renderer.current = null;
    };
  }, [seed]);
  useEffect(() => { renderer.current?.update({ shape, state }); renderer.current?.advance(1); }, [shape, state]);
  return <span className="gb-animated-face" role="img" aria-label={`${seed}, ${working ? "thinking" : "ready"}`} style={{ width: size, height: size, "--fg": color, "--bg": "#ffffff" } as React.CSSProperties}>
    <span className="gb-animated-face__fallback"><BotMark seed={seed} shapeId={shapeId} colorId={colorId} size={size} /></span>
    <span className="gb-animated-face__renderer" ref={host} />
  </span>;
});
