import test from "node:test";
import assert from "node:assert/strict";
import {
  ALL_GROK_BOT_SHAPES,
  TURN_SETTLEMENT_HOLD_MS,
  classifyHermesActivity,
  deriveGrokBotState,
  resolveGrokBotShape,
} from "../apps/web/src/lib/botState.ts";

test("Hermes tools map to authentic Grok Bot 1.2.3 activity states", () => {
  assert.equal(classifyHermesActivity("write_file"), "writing");
  assert.equal(classifyHermesActivity("edit_file"), "writing");
  assert.equal(classifyHermesActivity("replace_file_content"), "writing");
  assert.equal(classifyHermesActivity("web_search"), "searching");
  assert.equal(classifyHermesActivity("grep_search"), "searching");
  assert.equal(classifyHermesActivity("read_file"), "reading");
  assert.equal(classifyHermesActivity("web_fetch"), "reading");
  assert.equal(classifyHermesActivity("run_command"), "running");
  assert.equal(classifyHermesActivity("terminal"), "running");
  assert.equal(classifyHermesActivity("execute_code"), "coding");
  assert.equal(classifyHermesActivity("screenshot"), "computing");
  assert.equal(classifyHermesActivity("generate_image"), "progress");
  assert.equal(classifyHermesActivity("send_message"), "sending");
  assert.equal(classifyHermesActivity("mcp_connect"), "orbit");
  assert.equal(classifyHermesActivity("schedule_cron"), "waiting");
  assert.equal(classifyHermesActivity("custom_unknown_tool"), "working");
  assert.equal(classifyHermesActivity(null, "Thinking..."), "thinking");
});

test("deriveGrokBotState matches Grok Bot uO priority and 8s settlement window", () => {
  const base = {
    isStreaming: false,
    hasStreamedText: false,
    activeToolName: null,
    liveStatusText: "",
    awaitingUserResponse: false,
    lastTurnSettlement: null,
  };

  assert.equal(deriveGrokBotState(base, 10_000), "idle");
  assert.equal(deriveGrokBotState({ ...base, isStreaming: true }, 10_000), "thinking");
  assert.equal(deriveGrokBotState({ ...base, isStreaming: true, activeToolName: "write_file" }, 10_000), "writing");
  assert.equal(deriveGrokBotState({ ...base, isStreaming: true, hasStreamedText: true }, 10_000), "streaming");
  assert.equal(deriveGrokBotState({ ...base, isStreaming: true, awaitingUserResponse: true }, 10_000), "blocked");

  const settled = { ...base, lastTurnSettlement: { outcome: "success", settledAtMs: 10_000 } };
  assert.equal(deriveGrokBotState(settled, 10_000 + TURN_SETTLEMENT_HOLD_MS - 1), "finished");
  assert.equal(deriveGrokBotState(settled, 10_000 + TURN_SETTLEMENT_HOLD_MS), "idle");

  const errored = { ...base, lastTurnSettlement: { outcome: "error", settledAtMs: 10_000 } };
  assert.equal(deriveGrokBotState(errored, 12_000), "failed");
  assert.equal(deriveGrokBotState(errored, 18_500), "idle");
});

test("all 26 Grok Bot shapes resolve validly and deterministically", () => {
  assert.equal(ALL_GROK_BOT_SHAPES.length, 26);
  for (const s of ALL_GROK_BOT_SHAPES) {
    assert.equal(resolveGrokBotShape(s.id, "seed"), s.id);
  }
  const a = resolveGrokBotShape(undefined, "hermes-bot-alpha");
  const b = resolveGrokBotShape(undefined, "hermes-bot-alpha");
  assert.equal(a, b);
});
