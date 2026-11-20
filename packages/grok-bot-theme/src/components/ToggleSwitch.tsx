import React from "react";

export interface ToggleSwitchProps {
  checked: boolean;
  onChange: (next: boolean) => void;
  disabled?: boolean;
  label?: string;
}

export const ToggleSwitch: React.FC<ToggleSwitchProps> = ({
  checked,
  onChange,
  disabled = false,
  label,
}) => (
  <button
    type="button"
    role="switch"
    aria-checked={checked}
    aria-label={label}
    disabled={disabled}
    data-checked={checked ? "true" : "false"}
    className="gb-switch"
    onClick={() => onChange(!checked)}
  >
    <span className="gb-switch__thumb" />
  </button>
);
