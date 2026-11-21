import React, { useEffect, useRef, useState } from "react";
import { CursorIcon } from "@grok-bot/theme";
import {
  GROK_BOT_COLORS,
  GROK_BOT_SHAPES,
  GROK_STATE_CATEGORIES,
  mountGrokBotFace,
  type BotColorId,
  type GrokBotController,
  type GrokBotShapeId,
  type GrokBotStateId,
} from "./grokBotEngine";

interface LiveFaceProps {
  shape: GrokBotShapeId;
  color: string;
  eyeColor?: string | null;
  state: GrokBotStateId;
  size: number;
  isFollowingPointer?: boolean;
  isReducedMotion?: boolean;
  isPaused?: boolean;
  seed?: number;
  onFps?: (fps: number) => void;
  onClick?: () => void;
}

const LiveGrokFace: React.FC<LiveFaceProps> = ({
  shape,
  color,
  eyeColor = null,
  state,
  size,
  isFollowingPointer = false,
  isReducedMotion = false,
  isPaused = false,
  seed = 1,
  onFps,
  onClick,
}) => {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const ctrlRef = useRef<GrokBotController | null>(null);

  useEffect(() => {
    const el = hostRef.current;
    if (!el) return;
    const ctrl = mountGrokBotFace(el, {
      shape,
      color,
      eyeColor,
      state,
      size,
      isFollowingPointer,
      isReducedMotion,
      isPaused,
      seed,
    });
    ctrlRef.current = ctrl;
    let fpsTimer: number | undefined;
    if (onFps) {
      fpsTimer = window.setInterval(() => onFps(ctrl.getFps()), 500);
    }
    return () => {
      if (fpsTimer) window.clearInterval(fpsTimer);
      ctrl.destroy();
      ctrlRef.current = null;
    };
    // Mount once per container/seed; updates flow through ctrl.update()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seed]);

  useEffect(() => {
    ctrlRef.current?.update({
      shape,
      color,
      eyeColor,
      state,
      size,
      isFollowingPointer,
      isReducedMotion,
      isPaused,
    });
  }, [shape, color, eyeColor, state, size, isFollowingPointer, isReducedMotion, isPaused]);

  return (
    <div
      ref={hostRef}
      onClick={() => {
        ctrlRef.current?.poke("spin");
        onClick?.();
      }}
      style={{
        width: size,
        height: size,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        cursor: "pointer",
      }}
      title="Click to spin"
    />
  );
};

const KEY_ANIMATION_STATES: Array<{ id: GrokBotStateId; label: string; desc: string }> = [
  { id: "idle", label: "Idle", desc: "Breathing, gentle gaze & natural blink" },
  { id: "thinking", label: "Thinking", desc: "Morphs into 3 staggered Gaussian wave dots" },
  { id: "writing", label: "Writing", desc: "Pencil tip + angled shaft + live Catmull-Rom ink" },
  { id: "editing", label: "Editing (Pen)", desc: "Pen animation with scribble wiggle & erase cycle" },
  { id: "streaming", label: "Streaming", desc: "Expressive talking bob & tilt" },
  { id: "searching", label: "Searching", desc: "Concentric pulsing radar sweep rings" },
  { id: "waiting", label: "Waiting", desc: "Orbiting satellite around core" },
  { id: "dictating", label: "Dictating", desc: "5-bar voice waveform equalizer" },
  { id: "progress", label: "Progress", desc: "Rotating circular progress arc" },
  { id: "alerting", label: "Alerting", desc: "Damped wobbling exclamation bar" },
  { id: "bouncing", label: "Bouncing", desc: "Gravity parabola bounce" },
  { id: "celebrate", label: "Celebrate", desc: "Joyful bounce with happy crescent eyes" },
];

export const BotPreviewApp: React.FC = () => {
  const [theme, setTheme] = useState<"light" | "dark">("dark");
  const [shape, setShape] = useState<GrokBotShapeId>("blob");
  const [colorId, setColorId] = useState<BotColorId>("blue");
  const [state, setState] = useState<GrokBotStateId>("editing");
  const [categoryIdx, setCategoryIdx] = useState(0);
  const [followPointer, setFollowPointer] = useState(true);
  const [reducedMotion, setReducedMotion] = useState(false);
  const [cutoutEyes, setCutoutEyes] = useState(true);
  const [fps, setFps] = useState(60);
  const [iconStyle, setIconStyle] = useState<"tinted" | "monochrome" | "dark-plate">("tinted");

  useEffect(() => {
    document.documentElement.dataset.theme = theme === "dark" ? "sand-dark" : "sand-light";
  }, [theme]);

  const activeColor = GROK_BOT_COLORS.find((c) => c.id === colorId) ?? GROK_BOT_COLORS[0];
  // Ensure black bot remains crisp on dark backgrounds (matches Grok Bot dark ink handling)
  const resolvedHex =
    theme === "dark" && activeColor.id === "black" ? "#F5F5F5" : activeColor.hex;
  const eyeColor = cutoutEyes ? null : theme === "dark" ? "#161616" : "#FFFFFF";

  return (
    <div
      style={{
        height: "100%",
        overflowY: "auto",
        background: "var(--grok-bot-shared-substrate)",
        color: "var(--grok-bot-text-primary)",
        padding: "24px 32px 64px",
      }}
    >
      <div style={{ maxWidth: 1200, margin: "0 auto", display: "flex", flexDirection: "column", gap: 24 }}>
        {/* Header */}
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            flexWrap: "wrap",
            gap: 16,
          }}
        >
          <div>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <h1 style={{ margin: 0, fontSize: 22, fontWeight: 600 }}>
                Grok Bot 1.2.3 — Faces, Animations &amp; App Icons Preview
              </h1>
              <span className="gb-badge gb-badge--info">REA Clean-Room Engine</span>
              <span className="gb-badge gb-badge--success">{fps} FPS</span>
            </div>
            <p style={{ margin: "6px 0 0", fontSize: 13, color: "var(--grok-bot-text-secondary)" }}>
              Standalone visual preview recreated from REA analysis of <code>grok-bot_1.2.3_amd64.deb</code> — 26 shapes,
              11 colors, 50 states, 13 morphs (including thinking dots &amp; editing pen), and app icon variants.
            </p>
          </div>

          <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
            <div className="gb-segmented">
              <button
                type="button"
                className="gb-segmented__item"
                data-active={theme === "light" ? "true" : undefined}
                onClick={() => setTheme("light")}
              >
                Sand Light
              </button>
              <button
                type="button"
                className="gb-segmented__item"
                data-active={theme === "dark" ? "true" : undefined}
                onClick={() => setTheme("dark")}
              >
                Sand Dark
              </button>
            </div>
          </div>
        </div>

        {/* Hero Interactive Stage */}
        <div className="gb-section-card">
          <div className="gb-section-card__head">
            <span className="gb-section-card__title">Interactive Stage</span>
            <div style={{ display: "flex", alignItems: "center", gap: 8, flexWrap: "wrap" }}>
              <button
                type="button"
                className={`gb-btn gb-btn--sm ${followPointer ? "gb-btn--primary" : "gb-btn--secondary"}`}
                onClick={() => setFollowPointer((v) => !v)}
              >
                <CursorIcon name="sparkle" size={12} />
                Cursor Gaze: {followPointer ? "On" : "Off"}
              </button>
              <button
                type="button"
                className={`gb-btn gb-btn--sm ${cutoutEyes ? "gb-btn--primary" : "gb-btn--secondary"}`}
                onClick={() => setCutoutEyes((v) => !v)}
              >
                Eyes: {cutoutEyes ? "Cutout (Authentic)" : "Solid"}
              </button>
              <button
                type="button"
                className={`gb-btn gb-btn--sm ${reducedMotion ? "gb-btn--danger" : "gb-btn--secondary"}`}
                onClick={() => setReducedMotion((v) => !v)}
              >
                Reduced Motion: {reducedMotion ? "On" : "Off"}
              </button>
            </div>
          </div>

          <div
            className="gb-section-card__body"
            style={{
              display: "grid",
              gridTemplateColumns: "minmax(260px, 320px) minmax(0, 1fr)",
              gap: 24,
              alignItems: "center",
            }}
          >
            {/* Stage Canvas */}
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                alignItems: "center",
                justifyContent: "center",
                gap: 14,
                padding: 28,
                borderRadius: 16,
                background: "var(--grok-bot-sidebar-tint)",
                border: "1px solid var(--grok-bot-sidebar-separator)",
              }}
            >
              <LiveGrokFace
                shape={shape}
                color={resolvedHex}
                eyeColor={eyeColor}
                state={state}
                size={176}
                isFollowingPointer={followPointer}
                isReducedMotion={reducedMotion}
                seed={101}
                onFps={setFps}
              />
              <div style={{ textAlign: "center" }}>
                <div style={{ fontWeight: 600, fontSize: 14 }}>
                  {shape} · {activeColor.label} ·{" "}
                  <code style={{ fontFamily: "var(--cursor-font-family-mono)" }}>{state}</code>
                </div>
                <div style={{ fontSize: 12, color: "var(--grok-bot-text-secondary)", marginTop: 2 }}>
                  Move cursor to test gaze · Click bot to trigger spin poke
                </div>
              </div>
            </div>

            {/* Controls: 11 Colors + 50 States */}
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              <div>
                <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 8, color: "var(--grok-bot-text-secondary)" }}>
                  11 Official Grok Bot Colors
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
                  {GROK_BOT_COLORS.map((c) => {
                    const active = c.id === colorId;
                    return (
                      <button
                        key={c.id}
                        type="button"
                        onClick={() => setColorId(c.id)}
                        className="gb-btn gb-btn--secondary gb-btn--sm"
                        style={{
                          borderColor: active ? c.hex : undefined,
                          boxShadow: active ? `0 0 0 1.5px ${c.hex}` : undefined,
                        }}
                      >
                        <span
                          style={{
                            width: 12,
                            height: 12,
                            borderRadius: 999,
                            background: c.hex,
                            border: "1px solid rgba(128,128,128,0.35)",
                            display: "inline-block",
                          }}
                        />
                        {c.label}
                      </button>
                    );
                  })}
                </div>
              </div>

              <div>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    gap: 8,
                    marginBottom: 10,
                    flexWrap: "wrap",
                  }}
                >
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--grok-bot-text-secondary)" }}>
                    States &amp; Morphs
                  </span>
                  <div className="gb-segmented">
                    {GROK_STATE_CATEGORIES.map((cat, idx) => (
                      <button
                        key={cat.label}
                        type="button"
                        className="gb-segmented__item"
                        data-active={categoryIdx === idx ? "true" : undefined}
                        onClick={() => setCategoryIdx(idx)}
                      >
                        {cat.label}
                      </button>
                    ))}
                  </div>
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                  {GROK_STATE_CATEGORIES[categoryIdx].states.map((st) => (
                    <button
                      key={st}
                      type="button"
                      className={`gb-btn gb-btn--sm ${state === st ? "gb-btn--primary" : "gb-btn--secondary"}`}
                      onClick={() => setState(st)}
                    >
                      {st}
                    </button>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Key State & Morph Animations Side-by-Side */}
        <div className="gb-section-card">
          <div className="gb-section-card__head">
            <span className="gb-section-card__title">Core State &amp; Morph Animations (Live)</span>
            <span className="gb-badge gb-badge--muted">Thinking · Writing · Editing Pen · Morphs</span>
          </div>
          <div
            className="gb-section-card__body"
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(175px, 1fr))",
              gap: 12,
            }}
          >
            {KEY_ANIMATION_STATES.map((item, idx) => {
              const selected = state === item.id;
              return (
                <div
                  key={item.id}
                  onClick={() => setState(item.id)}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "center",
                    gap: 10,
                    padding: "16px 12px",
                    borderRadius: 12,
                    background: "var(--grok-bot-sidebar-tint)",
                    border: selected
                      ? `1.5px solid ${resolvedHex}`
                      : "1px solid var(--grok-bot-sidebar-separator)",
                    cursor: "pointer",
                    textAlign: "center",
                  }}
                >
                  <LiveGrokFace
                    shape={shape}
                    color={resolvedHex}
                    eyeColor={eyeColor}
                    state={item.id}
                    size={64}
                    isReducedMotion={reducedMotion}
                    seed={200 + idx}
                  />
                  <div>
                    <div style={{ fontWeight: 600, fontSize: 13 }}>{item.label}</div>
                    <div style={{ fontSize: 11, color: "var(--grok-bot-text-secondary)", marginTop: 2 }}>
                      {item.desc}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        </div>

        {/* All 26 Official Shapes */}
        <div className="gb-section-card">
          <div className="gb-section-card__head">
            <span className="gb-section-card__title">All 26 Grok Bot Shapes</span>
            <span className="gb-badge gb-badge--muted">Click any shape to preview on stage</span>
          </div>
          <div
            className="gb-section-card__body"
            style={{
              display: "grid",
              gridTemplateColumns: "repeat(auto-fill, minmax(104px, 1fr))",
              gap: 10,
            }}
          >
            {GROK_BOT_SHAPES.map((s, idx) => {
              const selected = shape === s;
              return (
                <div
                  key={s}
                  onClick={() => setShape(s)}
                  style={{
                    display: "flex",
                    flexDirection: "column",
                    alignItems: "center",
                    gap: 8,
                    padding: "12px 8px",
                    borderRadius: 12,
                    background: "var(--grok-bot-sidebar-tint)",
                    border: selected
                      ? `1.5px solid ${resolvedHex}`
                      : "1px solid var(--grok-bot-sidebar-separator)",
                    cursor: "pointer",
                  }}
                >
                  <LiveGrokFace
                    shape={s}
                    color={resolvedHex}
                    eyeColor={eyeColor}
                    state="idle"
                    size={52}
                    isFollowingPointer={followPointer}
                    isReducedMotion={reducedMotion}
                    seed={300 + idx}
                  />
                  <span style={{ fontSize: 11.5, fontWeight: selected ? 600 : 500 }}>{s}</span>
                </div>
              );
            })}
          </div>
        </div>

        {/* App Icons & Color / Animation Variants */}
        <div className="gb-section-card">
          <div className="gb-section-card__head">
            <span className="gb-section-card__title">App Icons — Color, Size &amp; Animation Variants</span>
            <div className="gb-segmented">
              {(
                [
                  ["tinted", "Tinted"],
                  ["dark-plate", "Dark Plate"],
                  ["monochrome", "Light Plate"],
                ] as const
              ).map(([id, label]) => (
                <button
                  key={id}
                  type="button"
                  className="gb-segmented__item"
                  data-active={iconStyle === id ? "true" : undefined}
                  onClick={() => setIconStyle(id)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          <div className="gb-section-card__body" style={{ display: "flex", flexDirection: "column", gap: 20 }}>
            {/* 11 Color App Icon Tiles */}
            <div>
              <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 10, color: "var(--grok-bot-text-secondary)" }}>
                11 Color App Icon Variants (Squircle App Launcher Frame)
              </div>
              <div style={{ display: "flex", flexWrap: "wrap", gap: 14 }}>
                {GROK_BOT_COLORS.map((c, idx) => {
                  const plateBg =
                    iconStyle === "tinted"
                      ? c.hex
                      : iconStyle === "dark-plate"
                        ? "#161616"
                        : "#FFFFFF";
                  const botHex =
                    iconStyle === "tinted"
                      ? "#FFFFFF"
                      : iconStyle === "dark-plate" && c.id === "black"
                        ? "#F5F5F5"
                        : c.hex;
                  return (
                    <div
                      key={c.id}
                      onClick={() => setColorId(c.id)}
                      style={{
                        display: "flex",
                        flexDirection: "column",
                        alignItems: "center",
                        gap: 6,
                        cursor: "pointer",
                      }}
                    >
                      <div
                        style={{
                          width: 68,
                          height: 68,
                          borderRadius: 16,
                          background: plateBg,
                          border: "1px solid rgba(128,128,128,0.22)",
                          boxShadow: "0 6px 18px rgba(0,0,0,0.22)",
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                        }}
                      >
                        <LiveGrokFace
                          shape={shape}
                          color={botHex}
                          eyeColor={null}
                          state="idle"
                          size={46}
                          isReducedMotion={reducedMotion}
                          seed={500 + idx}
                        />
                      </div>
                      <span style={{ fontSize: 11, color: "var(--grok-bot-text-secondary)" }}>{c.label}</span>
                    </div>
                  );
                })}
              </div>
            </div>

            {/* Animated App Icon Variants + Size Ladder */}
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(280px, 1fr))", gap: 16 }}>
              <div
                style={{
                  padding: 16,
                  borderRadius: 12,
                  background: "var(--grok-bot-sidebar-tint)",
                  border: "1px solid var(--grok-bot-sidebar-separator)",
                }}
              >
                <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 12, color: "var(--grok-bot-text-secondary)" }}>
                  Animated App Icon States
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: 14 }}>
                  {(["idle", "thinking", "editing", "searching", "alerting"] as const).map((st, idx) => (
                    <div
                      key={st}
                      style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6 }}
                    >
                      <div
                        style={{
                          width: 60,
                          height: 60,
                          borderRadius: 14,
                          background: iconStyle === "dark-plate" ? "#161616" : resolvedHex,
                          border: "1px solid rgba(128,128,128,0.22)",
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                        }}
                      >
                        <LiveGrokFace
                          shape={shape}
                          color={iconStyle === "dark-plate" ? resolvedHex : "#FFFFFF"}
                          eyeColor={null}
                          state={st}
                          size={42}
                          isReducedMotion={reducedMotion}
                          seed={600 + idx}
                        />
                      </div>
                      <span style={{ fontSize: 11, color: "var(--grok-bot-text-secondary)" }}>{st}</span>
                    </div>
                  ))}
                </div>
              </div>

              <div
                style={{
                  padding: 16,
                  borderRadius: 12,
                  background: "var(--grok-bot-sidebar-tint)",
                  border: "1px solid var(--grok-bot-sidebar-separator)",
                }}
              >
                <div style={{ fontSize: 12, fontWeight: 600, marginBottom: 12, color: "var(--grok-bot-text-secondary)" }}>
                  App Icon Size Ladder (16px – 96px)
                </div>
                <div style={{ display: "flex", alignItems: "flex-end", gap: 14, flexWrap: "wrap" }}>
                  {[16, 24, 32, 48, 64, 96].map((sz, idx) => (
                    <div
                      key={sz}
                      style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6 }}
                    >
                      <div
                        style={{
                          width: sz,
                          height: sz,
                          borderRadius: Math.max(4, Math.round(sz * 0.23)),
                          background: resolvedHex,
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          boxShadow: "0 4px 12px rgba(0,0,0,0.2)",
                        }}
                      >
                        <LiveGrokFace
                          shape={shape}
                          color="#FFFFFF"
                          eyeColor={null}
                          state="idle"
                          size={Math.round(sz * 0.7)}
                          isReducedMotion={reducedMotion}
                          seed={700 + idx}
                        />
                      </div>
                      <span style={{ fontSize: 10.5, color: "var(--grok-bot-text-secondary)" }}>{sz}px</span>
                    </div>
                  ))}
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
