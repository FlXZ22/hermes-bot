import type { HermesClient, HermesRealtimeSession } from "@grok-bot/hermes-client";
import type { WorkCheckpointResults } from "../state/workSessions";
import { readWorkGitDiff } from "./workGit";

export async function dispatchWorkCommand(options: {
  input: string;
  gateway: HermesRealtimeSession;
  client: HermesClient | null;
  sessionId: string;
  cwd?: string;
  explore: (question: string) => Promise<string>;
}) {
  const { gateway, client, sessionId, cwd } = options;
  const seen = new Set<string>();
  let command = options.input;
  let result: Record<string, unknown>;
  for (;;) {
    if (seen.has(command) || seen.size === 8) throw new Error("Circular Hermes command alias.");
    seen.add(command);
    if (/^\/explore(?:\s|$)/i.test(command)) {
      const question = command.slice("/explore".length).trim();
      result = { type: "exec", output: question ? await options.explore(question)
        : "Use `/explore <question>` for a read-only answer. Add `@file:path` to include a workspace file." };
    } else if (command === "/oc" || command.startsWith("/oc/")) {
      if (!client || !cwd) throw new Error("Open a workspace before running an OpenCode project command.");
      const { dispatchProjectCommand } = await import("./workProjectCommands");
      result = await dispatchProjectCommand(client, cwd, command);
    } else if (/^\/diff\s+session(?:\s|$)/.test(command)) {
      const { enabled, checkpoints } = await gateway.rpc<WorkCheckpointResults["list"]>("rollback.list", { session_id: sessionId });
      const first = checkpoints.at(-1);
      if (enabled && first) {
        const { diff, stat } = await gateway.rpc<WorkCheckpointResults["diff"]>("rollback.diff", { session_id: sessionId, hash: first.hash });
        result = { type: "exec", output: diff || stat || "No changes since the first checkpoint." };
      } else {
        result = { type: "exec", output: "The session diff requires a workspace checkpoint." };
      }
    } else if (/^\/diff(?:\s|$)/.test(command)) {
      if (!client || !cwd) throw new Error("Open a Work repository before requesting a diff.");
      result = { type: "exec", output: await readWorkGitDiff(client, cwd, command) };
    } else if (/^\/(?:review|agents|status|usage|context|tools|help)(?:\s|$)/.test(command)) {
      result = await gateway.rpc("slash.exec", { session_id: sessionId, command: command.slice(1) });
    } else {
      result = await gateway.dispatchCommand(sessionId, command);
    }
    if (result.type !== "alias") return { command, result };
    if (typeof result.target !== "string" || !result.target.trim()) throw new Error("Empty Hermes command alias.");
    const args = command.slice(command.search(/\s|$/)).trim();
    command = `/${result.target.trim().replace(/^\/+/, "")}${args ? ` ${args}` : ""}`;
  }
}
