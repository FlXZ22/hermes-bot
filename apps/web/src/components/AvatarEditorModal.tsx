import React, { useEffect, useState } from "react";
import {
  BOT_COLOR_PRESETS,
  Modal,
  resolveBotColor,
  type GrokBotColorId,
} from "@grok-bot/theme";
import { AnimatedBotFace } from "./AnimatedBotFace";
import { ALL_GROK_BOT_SHAPES, resolveGrokBotShape } from "../lib/botState";
import type { GrokEngineState } from "../vendor/grok-face-engine";
import { useApp } from "../state/appStore";

const PREVIEW_STATES: readonly { id: GrokEngineState; label: string }[] = [
  { id: "idle", label: "Idle" },
  { id: "thinking", label: "Thinking" },
  { id: "writing", label: "Writing" },
  { id: "searching", label: "Searching" },
  { id: "streaming", label: "Streaming" },
  { id: "happy", label: "Happy" },
  { id: "celebrate", label: "Celebrate" },
  { id: "sleeping", label: "Sleeping" },
];

export const AvatarEditorModal: React.FC = () => {
  const { avatarEditorTarget, setAvatarEditorTarget, botAvatars, updateBotAvatar } = useApp();
  const seed = avatarEditorTarget?.seed ?? "default";
  const existing = botAvatars[seed];

  const [name, setName] = useState("");
  const [shapeId, setShapeId] = useState<string>("blob");
  const [colorId, setColorId] = useState<GrokBotColorId>("blue");
  const [previewState, setPreviewState] = useState<GrokEngineState>("idle");

  useEffect(() => {
    if (!avatarEditorTarget) return;
    setName(existing?.displayName ?? avatarEditorTarget.title);
    setShapeId(resolveGrokBotShape(existing?.shapeId, seed));
    setColorId(existing?.colorId ?? resolveBotColor(seed).id);
    setPreviewState("idle");
  }, [avatarEditorTarget, existing, seed]);

  if (!avatarEditorTarget) return null;

  const handleSave = () => {
    updateBotAvatar(seed, {
      displayName: name.trim() || avatarEditorTarget.title,
      shapeId,
      colorId,
    });
    setAvatarEditorTarget(null);
  };

  return (
    <Modal open={Boolean(avatarEditorTarget)} title="Customize bot" onClose={() => setAvatarEditorTarget(null)}>
      <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 16, padding: "4px 0" }}>
          <AnimatedBotFace
            seed={seed}
            shapeId={shapeId}
            colorId={colorId}
            size={72}
            state={previewState}
            isFollowingPointer
            clickableSpin
            maxFrameRate={60}
          />
          <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 8 }}>
            <div className="gb-field">
              <label className="gb-field__label">Display Name</label>
              <input className="gb-input" value={name} onChange={(e) => setName(e.target.value)} />
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
              {PREVIEW_STATES.map((st) => (
                <button
                  key={st.id}
                  type="button"
                  className={`gb-btn gb-btn--sm ${previewState === st.id ? "gb-btn--primary" : "gb-btn--ghost"}`}
                  style={{ padding: "2px 8px", fontSize: 11 }}
                  onClick={() => setPreviewState(st.id)}
                >
                  {st.label}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="gb-field">
          <label className="gb-field__label">Shape ({ALL_GROK_BOT_SHAPES.length})</label>
          <div
            className="gb-shape-grid"
            style={{
              gridTemplateColumns: "repeat(5, minmax(0, 1fr))",
              maxHeight: 220,
              overflowY: "auto",
              paddingRight: 2,
            }}
          >
            {ALL_GROK_BOT_SHAPES.map((shape) => (
              <button
                key={shape.id}
                type="button"
                className="gb-shape-btn"
                data-active={shapeId === shape.id ? "true" : undefined}
                onClick={() => setShapeId(shape.id)}
                title={shape.label}
              >
                <AnimatedBotFace
                  seed={seed}
                  shapeId={shape.id}
                  colorId={colorId}
                  size={36}
                  state="idle"
                  isStatic
                />
              </button>
            ))}
          </div>
        </div>

        <div className="gb-field">
          <label className="gb-field__label">Color ({BOT_COLOR_PRESETS.length})</label>
          <div className="gb-color-grid">
            {BOT_COLOR_PRESETS.map((preset) => (
              <button
                key={preset.id}
                type="button"
                className="gb-color-dot"
                data-active={colorId === preset.id ? "true" : undefined}
                style={{ background: preset.hex }}
                onClick={() => setColorId(preset.id)}
                title={preset.label}
              />
            ))}
          </div>
        </div>

        <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
          <button type="button" className="gb-btn gb-btn--ghost" onClick={() => setAvatarEditorTarget(null)}>
            Cancel
          </button>
          <button type="button" className="gb-btn gb-btn--primary" onClick={handleSave}>
            Save Appearance
          </button>
        </div>
      </div>
    </Modal>
  );
};
