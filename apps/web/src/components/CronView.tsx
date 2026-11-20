import React, { useState } from "react";
import { CursorIcon, Modal } from "@grok-bot/theme";
import { useApp } from "../state/appStore";

export const CronView: React.FC = () => {
  const { cronJobs, client, refreshCapabilities, showToast, activeProfile } = useApp();
  const [modalOpen, setModalOpen] = useState(false);
  const [name, setName] = useState("");
  const [schedule, setSchedule] = useState("0 9 * * *");
  const [prompt, setPrompt] = useState("");

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!client || !name.trim() || !prompt.trim()) return;
    try {
      await client.createCronJob({
        name: name.trim(),
        schedule: schedule.trim(),
        prompt: prompt.trim(),
        profile: activeProfile,
        enabled: true,
      });
      setModalOpen(false);
      setName("");
      setPrompt("");
      await refreshCapabilities();
      showToast("Created cron automation");
    } catch (err) {
      showToast(err instanceof Error ? err.message : "Failed creating cron job");
    }
  };

  return (
    <div className="gb-panel">
      <div className="gb-panel__header">
        <div>
          <h2 className="gb-panel__title">Cron & Automations</h2>
          <div className="gb-panel__sub">Scheduled autonomous Hermes agent runs</div>
        </div>
        <button type="button" className="gb-btn gb-btn--primary" onClick={() => setModalOpen(true)}>
          <CursorIcon name="plus" size={13} />
          New Schedule
        </button>
      </div>

      <div className="gb-panel__body">
        <div className="gb-panel__container">
          <div className="gb-section-card">
            {cronJobs.length === 0 ? (
              <div className="gb-empty">
                <CursorIcon name="clock" size={28} />
                <div className="gb-empty__title">No scheduled jobs yet</div>
                <div className="gb-empty__sub">
                  Automate daily briefings, repository audits, or notifications with Hermes Cron.
                </div>
                <button type="button" className="gb-btn gb-btn--primary" onClick={() => setModalOpen(true)}>
                  Create Cron Job
                </button>
              </div>
            ) : (
              cronJobs.map((job) => (
                <div key={job.id} className="gb-row">
                  <div className="gb-row__info">
                    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                      <span className="gb-row__label">{job.name}</span>
                      <span className="gb-badge gb-badge--muted" style={{ fontFamily: "var(--cursor-font-family-mono)" }}>
                        {job.schedule}
                      </span>
                      <span className={`gb-badge ${job.paused ? "gb-badge--warning" : "gb-badge--success"}`}>
                        {job.paused ? "Paused" : "Active"}
                      </span>
                    </div>
                    <div className="gb-row__desc">{job.prompt}</div>
                  </div>
                  <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                    <button
                      type="button"
                      className="gb-btn gb-btn--secondary gb-btn--sm"
                      onClick={async () => {
                        if (!client) return;
                        await client.triggerCronJob(job.id);
                        showToast(`Triggered "${job.name}"`);
                      }}
                    >
                      <CursorIcon name="play" size={12} />
                      Run
                    </button>
                    <button
                      type="button"
                      className="gb-btn gb-btn--secondary gb-btn--sm"
                      onClick={async () => {
                        if (!client) return;
                        if (job.paused) await client.resumeCronJob(job.id);
                        else await client.pauseCronJob(job.id);
                        await refreshCapabilities();
                      }}
                    >
                      <CursorIcon name={job.paused ? "play" : "pause"} size={12} />
                      {job.paused ? "Resume" : "Pause"}
                    </button>
                    <button
                      type="button"
                      className="gb-btn gb-btn--danger gb-btn--sm"
                      onClick={async () => {
                        if (!client) return;
                        await client.deleteCronJob(job.id);
                        await refreshCapabilities();
                        showToast("Deleted cron job");
                      }}
                    >
                      <CursorIcon name="trash" size={12} />
                    </button>
                  </div>
                </div>
              ))
            )}
          </div>
        </div>
      </div>

      <Modal open={modalOpen} title="New Hermes Cron Job" onClose={() => setModalOpen(false)}>
        <form onSubmit={handleCreate} style={{ display: "flex", flexDirection: "column", gap: 12 }}>
          <div className="gb-field">
            <label className="gb-field__label">Job Name</label>
            <input className="gb-input" placeholder="e.g. Morning Git Digest" value={name} onChange={(e) => setName(e.target.value)} required />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Cron Schedule (5-field)</label>
            <input className="gb-input gb-input--mono" placeholder="0 9 * * *" value={schedule} onChange={(e) => setSchedule(e.target.value)} required />
          </div>
          <div className="gb-field">
            <label className="gb-field__label">Prompt</label>
            <textarea className="gb-textarea" placeholder="What should Hermes do when triggered?" value={prompt} onChange={(e) => setPrompt(e.target.value)} required />
          </div>
          <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
            <button type="button" className="gb-btn gb-btn--ghost" onClick={() => setModalOpen(false)}>Cancel</button>
            <button type="submit" className="gb-btn gb-btn--primary">Save Schedule</button>
          </div>
        </form>
      </Modal>
    </div>
  );
};
