import React, { useState } from "react";
import { CursorIcon } from "./CursorIcon";

export interface ClarifyCardProps {
  question: string;
  choices?: string[];
  answered?: string | null;
  onAnswer: (answer: string) => void;
}

const KEYS = ["A", "B", "C", "D", "E", "F"];

export const ClarifyCard: React.FC<ClarifyCardProps> = ({
  question,
  choices = [],
  answered = null,
  onAnswer,
}) => {
  const [custom, setCustom] = useState("");

  return (
    <div className="gb-rich">
      <div className="gb-rich__header">
        <div>
          <div className="gb-approval__label">Quick question</div>
          <h4 className="gb-rich__title">{question}</h4>
        </div>
      </div>
      <div className="gb-rich__body" style={{ display: "flex", flexDirection: "column", gap: 6 }}>
        {answered ? (
          <span className="gb-badge gb-badge--success">
            <CursorIcon name="check-circle" size={13} />
            {answered}
          </span>
        ) : (
          <>
            {choices.map((choice, idx) => (
              <button
                key={choice}
                type="button"
                className="gb-rich-choice"
                onClick={() => onAnswer(choice)}
              >
                <span className="gb-keycap">{KEYS[idx] ?? idx + 1}</span>
                <span>{choice}</span>
              </button>
            ))}
            <form
              style={{ display: "flex", gap: 8, marginTop: 6 }}
              onSubmit={(e) => {
                e.preventDefault();
                if (custom.trim()) {
                  onAnswer(custom.trim());
                  setCustom("");
                }
              }}
            >
              <input
                type="text"
                className="gb-input"
                placeholder="Type an answer"
                value={custom}
                onChange={(e) => setCustom(e.target.value)}
              />
              <button
                type="submit"
                className="gb-btn gb-btn--primary"
                disabled={!custom.trim()}
              >
                Send
              </button>
            </form>
          </>
        )}
      </div>
    </div>
  );
};
