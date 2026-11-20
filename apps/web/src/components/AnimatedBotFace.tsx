import React, { memo, useEffect, useRef } from "react";
import { BotMark, getBotMarkShape, resolveBotColor } from "@grok-bot/theme";
import type { FaceRenderer } from "../vendor/grok-face-runtime";

export const AnimatedBotFace = memo(function AnimatedBotFace({ seed, shapeId, colorId, size = 32, working = false }: {
  seed: string; shapeId?: string | null; colorId?: string | null; size?: number; working?: boolean;
}) {
  const host = useRef<HTMLSpanElement>(null);
  const renderer = useRef<FaceRenderer | null>(null);
  const wake = useRef<() => void>(() => {});
  const selectedShape = getBotMarkShape(shapeId, seed).id;
  const shape = selectedShape === "clover" ? "cloud" : selectedShape;
  const color = resolveBotColor(seed, colorId).hex;
  const state = working ? "thinking" : "idle";
  const latest = useRef({ shape, state });
  latest.current = { shape, state };
  useEffect(() => {
    const mount = host.current;
    if (!mount) return;
    let disposed = false, visible = true, frame = 0, previous = 0, playUntil = 0;
    let resume: ReturnType<typeof setTimeout> | undefined;
    let blink: ReturnType<typeof setTimeout> | undefined;
    let scrolling = false;
    const motion = matchMedia("(prefers-reduced-motion: reduce)");
    const canAnimate = () => !disposed && visible && !document.hidden && !motion.matches && !scrolling;
    const stop = () => {
      cancelAnimationFrame(frame);
      clearTimeout(blink);
      frame = 0;
      previous = 0;
    };
    const scheduleBlink = () => {
      if (size < 40 || !canAnimate() || latest.current.state !== "idle") return;
      blink = setTimeout(() => {
        if (!canAnimate() || !renderer.current) return;
        renderer.current.action("blink");
        play(300);
      }, 9000);
    };
    const tick = (time: number) => {
      frame = 0;
      if (!canAnimate()) return;
      if (!previous) previous = time;
      if (time - previous >= 1000 / 30) {
        renderer.current?.advance(Math.min(50, time - previous));
        previous = time;
      }
      if (time < playUntil) frame = requestAnimationFrame(tick);
      else { previous = 0; scheduleBlink(); }
    };
    const play = (duration: number) => {
      if (!canAnimate() || !renderer.current) return;
      clearTimeout(blink);
      playUntil = performance.now() + duration;
      if (!frame) frame = requestAnimationFrame(tick);
    };
    const sync = () => { stop(); scheduleBlink(); };
    wake.current = () => play(750);
    const onScroll = () => {
      if (!scrolling) { scrolling = true; stop(); }
      clearTimeout(resume);
      resume = setTimeout(() => { scrolling = false; sync(); }, 160);
    };
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
      disposed = true; wake.current = () => {}; stop(); clearTimeout(resume); observer.disconnect();
      document.removeEventListener("visibilitychange", sync);
      document.removeEventListener("scroll", onScroll, true);
      motion.removeEventListener("change", sync);
      renderer.current?.destroy(); renderer.current = null;
    };
  }, [seed, size]);
  useEffect(() => {
    renderer.current?.update({ shape, state });
    renderer.current?.advance(1);
    wake.current();
  }, [shape, state]);
  return <span className="gb-animated-face" role="img" aria-label={`${seed}, ${working ? "thinking" : "ready"}`} style={{ width: size, height: size, "--fg": color, "--bg": "#ffffff" } as React.CSSProperties}>
    <span className="gb-animated-face__fallback"><BotMark seed={seed} shapeId={shapeId} colorId={colorId} size={size} /></span>
    <span className="gb-animated-face__renderer" ref={host} />
  </span>;
});
