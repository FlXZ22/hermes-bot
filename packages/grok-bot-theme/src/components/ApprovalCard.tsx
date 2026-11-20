import React from "react";
import { CursorIcon } from "./CursorIcon";

export interface ApprovalCardProps {
  title: string;
  detail?: string;
  label?: string;
  settled?: "allowed" | "always" | "denied" | null;
  onAllowOnce?: () => void;
  onAllowAlways?: () => void;
  onDeny?: () => void;
}

export const ApprovalCard: React.FC<ApprovalCardProps> = ({
  title,
  detail,
  label = "Needs your OK",
  settled = null,
  onAllowOnce,
  onAllowAlways,
  onDeny,
}) => {
  return (
    <div className="gb-approval">
      <div className="gb-approval__label">{label}</div>
      <div className="gb-approval__title">{title}</div>
      {detail ? <div className="gb-approval__detail">{detail}</div> : null}
      <div className="gb-approval__actions">
        {settled ? (
          <span
            className={`gb-badge ${
              settled === "denied" ? "gb-badge--danger" : "gb-badge--success"
            }`}
          >
            <CursorIcon name={settled === "denied" ? "x-circle" : "check-circle"} size={13} />
            {settled === "denied"
              ? "Denied"
              : settled === "always"
                ? "Always allowed"
                : "Allowed"}
          </span>
        ) : (
          <>
            <button type="button" className="gb-btn gb-btn--primary" onClick={onAllowOnce}>
              Allow once
            </button>
            {onAllowAlways ? (
              <button type="button" className="gb-btn gb-btn--secondary" onClick={onAllowAlways}>
                Always allow
              </button>
            ) : null}
            <button type="button" className="gb-btn gb-btn--ghost" onClick={onDeny}>
              Deny
            </button>
          </>
        )}
      </div>
    </div>
  );
};
