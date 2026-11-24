import React, { useEffect, useState } from "react";
import { useApp, useChatSlice } from "../state/appStore";

export function WorkActivity({ submit }: { submit: (command: string) => void }) {
  const { controlWorkSubagent, refreshWorkGoal, showToast } = useApp();
  const goalStatus = useChatSlice(chat => chat.goalStatus);
  const subagents = useChatSlice(chat => chat.subagents);
  const busy = useChatSlice(chat => chat.isStreaming);
  const followUp = useChatSlice(chat => chat.workFollowUpQueued);
  const [steering, setSteering] = useState<string | null>(null);
  const [instruction, setInstruction] = useState("");
  const [pending, setPending] = useState(false);

  useEffect(() => {
    void refreshWorkGoal().catch(() => undefined);
  }, [refreshWorkGoal]);

  const control = async (id: string, text?: string) => {
    if (pending) return;
    setPending(true);
    try {
      await controlWorkSubagent(id, text);
      showToast(text ? "Instruction queued for the subagent" : "Subagent stop requested");
      setSteering(null);
      setInstruction("");
    } catch (error) {
      showToast(error instanceof Error ? error.message : "Could not contact the subagent");
    } finally {
      setPending(false);
    }
  };

  if (!goalStatus && subagents.length === 0 && !followUp) return null;

  return (
    <section className="gb-work-activity" aria-label="Work activity">
      {goalStatus && <div className="gb-work-activity__goal">
        <span role="status">{goalStatus}</span>
        <button className="gb-chip-btn" type="button" onClick={() => submit("/goal pause")}>Pause goal</button>
        <button className="gb-chip-btn" type="button" disabled={busy} onClick={() => submit("/goal resume")}>Resume goal</button>
        <button className="gb-chip-btn" type="button" onClick={() => submit("/goal clear")}>Clear goal</button>
      </div>}
      {followUp && <div role="status">Follow-up queued</div>}
      {subagents.length > 0 && <details>
        <summary>Subagents · {subagents.filter(agent => agent.phase === "running").length} running</summary>
        <ul>
          {subagents.map(agent => (
            <li key={agent.id}>
              <div>
                <strong>{agent.goal || "Subagent"}</strong>
                <span>{[agent.outcome ?? agent.phase, agent.model, agent.activity].filter(Boolean).join(" · ")}</span>
                {agent.phase === "running" && steering === agent.id && (
                  <form onSubmit={event => { event.preventDefault(); void control(agent.id, instruction.trim()); }}>
                    <input className="gb-input" aria-label="Subagent instruction" value={instruction} onChange={event => setInstruction(event.target.value)} required />
                    <button className="gb-chip-btn" type="submit" disabled={pending || !instruction.trim()}>Send instruction</button>
                  </form>
                )}
              </div>
              {agent.phase === "running" && (
                <div className="gb-work-activity__actions">
                  <button className="gb-chip-btn" type="button" disabled={pending} onClick={() => {
                    setSteering(steering === agent.id ? null : agent.id);
                    setInstruction("");
                  }}>Steer</button>
                  <button className="gb-chip-btn" type="button" disabled={pending} onClick={() => void control(agent.id)}>Stop subagent</button>
                </div>
              )}
            </li>
          ))}
        </ul>
      </details>}
    </section>
  );
}
