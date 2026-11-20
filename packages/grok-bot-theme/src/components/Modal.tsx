import React, { useEffect, useRef } from "react";
import { CursorIcon } from "./CursorIcon";

export interface ModalProps {
  open: boolean;
  title: string;
  onClose: () => void;
  footer?: React.ReactNode;
  maxWidth?: number;
  children: React.ReactNode;
}

export const Modal: React.FC<ModalProps> = ({
  open,
  title,
  onClose,
  footer,
  maxWidth = 480,
  children,
}) => {
  const panelRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    if (!open) return;
    const previous = document.activeElement as HTMLElement | null;
    const panel = panelRef.current;
    const focusable = () => Array.from(panel?.querySelectorAll<HTMLElement>('button:not(:disabled), input, textarea, select, a[href], [tabindex="0"]') ?? []).filter(el => el.getClientRects().length);
    (panel?.querySelector<HTMLElement>('input, textarea') ?? focusable()[0] ?? panel)?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeRef.current(); }
      if (e.key === "Tab") {
        const elements = focusable();
        const first = elements[0];
        const last = elements[elements.length - 1];
        if (!first) { e.preventDefault(); panel?.focus(); }
        else if (e.shiftKey && (document.activeElement === first || !panel?.contains(document.activeElement))) { e.preventDefault(); last.focus(); }
        else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => { window.removeEventListener("keydown", onKey, true); previous?.focus(); };
  }, [open]);

  if (!open) return null;

  return (
    <div
      className="gb-modal-backdrop"
      role="dialog"
      aria-modal="true"
      aria-label={title}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div ref={panelRef} tabIndex={-1} className="gb-modal" style={{ maxWidth }}>
        <div className="gb-modal__header">
          <h3 className="gb-modal__title">{title}</h3>
          <button
            type="button"
            className="gb-footer-action"
            aria-label="Close"
            onClick={onClose}
          >
            <CursorIcon name="close" size={15} />
          </button>
        </div>
        <div className="gb-modal__body">{children}</div>
        {footer ? <div className="gb-modal__footer">{footer}</div> : null}
      </div>
    </div>
  );
};
