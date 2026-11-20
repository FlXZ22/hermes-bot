import React from "react";
import { getCursorIconChar, type CursorIconName } from "../iconMap";

export interface CursorIconProps extends React.HTMLAttributes<HTMLSpanElement> {
  name: CursorIconName;
  size?: number;
  variant?: "outline" | "filled";
  spin?: boolean;
}

export const CursorIcon: React.FC<CursorIconProps> = ({
  name,
  size = 16,
  variant = "outline",
  spin = false,
  className = "",
  style,
  ...rest
}) => {
  const glyph = getCursorIconChar(name, variant) ?? getCursorIconChar("circle", "outline") ?? "\uEA74";
  return (
    <span
      aria-hidden="true"
      className={`gb-icon${spin ? " gb-icon--spin" : ""}${className ? ` ${className}` : ""}`}
      style={{
        fontSize: `${size}px`,
        width: `${size}px`,
        height: `${size}px`,
        ...style,
      }}
      {...rest}
    >
      {glyph}
    </span>
  );
};
