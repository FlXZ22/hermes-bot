import React from "react";
import { getBotMarkShape } from "../marks";
import { resolveBotColor } from "../tokens";

export interface BotMarkProps {
  seed: string;
  shapeId?: string | null;
  colorId?: string | null;
  size?: number;
  working?: boolean;
  className?: string;
  title?: string;
}

export const BotMark: React.FC<BotMarkProps> = ({
  seed,
  shapeId,
  colorId,
  size = 36,
  working = false,
  className = "",
  title,
}) => {
  const shape = getBotMarkShape(shapeId, seed);
  const color = resolveBotColor(seed, colorId);

  return (
    <span
      className={className}
      title={title}
      style={{
        width: size,
        height: size,
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        flexShrink: 0,
        position: "relative",
      }}
    >
      <svg
        viewBox="-16 -12 272 262"
        width={size}
        height={size}
        role="img"
        aria-label={title ?? seed}
        style={{
          display: "block",
          overflow: "visible",
          transition: "transform 0.2s cubic-bezier(0.22, 1, 0.36, 1)",
          transform: working ? "scale(1.04)" : undefined,
        }}
      >
        <path
          d={shape.head}
          fill={color.hex}
          stroke={color.id === "black" ? "rgba(255,255,255,0.22)" : "none"}
          strokeWidth={color.id === "black" ? 6 : 0}
        />
        <path d={shape.eyes[0]} fill="#ffffff" />
        <path d={shape.eyes[1]} fill="#ffffff" />
      </svg>
    </span>
  );
};
