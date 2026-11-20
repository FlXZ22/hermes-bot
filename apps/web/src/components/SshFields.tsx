import React from "react";
import type { HermesSshTarget } from "@grok-bot/hermes-client";

export function SshFields({ value, onChange }: { value: HermesSshTarget; onChange: (value: HermesSshTarget) => void }) {
  return <div style={{ display: "flex", flexDirection: "column", gap: 12, marginTop: 12 }}>
    <label className="gb-field"><span className="gb-field__label">SSH host</span><input className="gb-input" value={value.host} autoComplete="off" placeholder="server.example.com or SSH config alias" onChange={e => onChange({ ...value, host: e.target.value })} required /></label>
    <div style={{ display: "grid", gridTemplateColumns: "1fr 100px", gap: 10 }}>
      <label className="gb-field"><span className="gb-field__label">Username <small>optional</small></span><input className="gb-input" value={value.user ?? ""} autoComplete="off" placeholder="Use SSH config" onChange={e => onChange({ ...value, user: e.target.value })} /></label>
      <label className="gb-field"><span className="gb-field__label">Port</span><input className="gb-input" type="number" min={1} max={65535} value={value.port ?? ""} placeholder="SSH config" onChange={e => onChange({ ...value, port: e.target.value ? Number(e.target.value) : undefined })} /></label>
    </div>
    <label className="gb-field"><span className="gb-field__label">Identity file <small>optional</small></span><input className="gb-input" value={value.identityFile ?? ""} autoComplete="off" placeholder="~/.ssh/id_ed25519" onChange={e => onChange({ ...value, identityFile: e.target.value })} /></label>
    <label className="gb-field"><span className="gb-field__label">SSH config file <small>optional</small></span><input className="gb-input" value={value.configFile ?? ""} autoComplete="off" placeholder="Use ~/.ssh/config" onChange={e => onChange({ ...value, configFile: e.target.value })} /></label>
    <p style={{ fontSize: 11, lineHeight: 1.6, color: "var(--grok-bot-text-secondary)", margin: 0 }}>Uses your SSH config, keys, and agent. You can answer password and host verification prompts in the terminal. Git refresh uses keys or your agent.</p>
  </div>;
}
