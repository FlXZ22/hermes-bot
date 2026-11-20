import React, { useEffect, useState } from "react";
import {
  BOT_COLOR_PRESETS,
  BOT_MARK_SHAPES,
  BotMark,
  Modal,
  resolveBotColor,
  type GrokBotColorId,
} from "@grok-bot/theme";
import { useApp } from "../state/appStore";

export const AvatarEditorModal: React.FC = () => {
  const { avatarEditorTarget, setAvatarEditorTarget, botAvatars, updateBotAvatar } = useApp();
  const seed = avatarEditorTarget?.seed ?? "default";
  const existing = botAvatars[seed];

  const [name, setName] = useState("");
  const [shapeId, setShapeId] = useState<string | undefined>(undefined);
  const [colorId, setColorId] = useState<GrokBotColorId>("blue");

  useEffect(() => {
    if (!avatarEditorTarget) return;
    setName(existing?.displayName ?? avatarEditorTarget.title);
    setShapeId(existing?.shapeId);
    setColorId(existing?.colorId ?? resolveBotColor(seed).id);
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
          <BotMark seed={seed} shapeId={shapeId} colorId={colorId} size={64} />
          <div className="gb-field" style={{ flex: 1 }}>
            <label className="gb-field__label">Display Name</label>
            <input className="gb-input" value={name} onChange={(e) => setName(e.target.value)} />
          </div>
        </div>

        <div className="gb-field">
          <label className="gb-field__label">Shape</label>
          <div className="gb-shape-grid">
            {BOT_MARK_SHAPES.map((shape) => (
              <button
                key={shape.id}
                type="button"
                className="gb-shape-btn"
                data-active={shapeId === shape.id ? "true" : undefined}
                onClick={() => setShapeId(shape.id)}
                title={shape.id}
              >
                <BotMark seed={seed} shapeId={shape.id} colorId={colorId} size={28} />
              </button>
            ))}
          </div>
        </div>

        <div className="gb-field">
          <label className="gb-field__label">Color</label>
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
