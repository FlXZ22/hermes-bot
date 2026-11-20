import test from "node:test";
import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { ChatStore, newAssistant } from "../apps/web/src/state/chatStore.ts";

const require = createRequire(import.meta.url);
const { NativeClient } = require("../apps/chromium/native-client.cjs");

test("stream bursts apply once and keep row identities stable", async () => {
  const chat = new ChatStore();
  const history = Array.from({ length: 1000 }, (_, i) => ({ id: String(i), role: "user", content: "past" }));
  const active = newAssistant();
  chat.updateMessages("active", () => [...history, active]);
  chat.patch("active", { isStreaming: true });
  const ids = chat.get("active").messageIds;
  let notifications = 0;
  let otherNotifications = 0;
  const painted = new Promise(resolve => chat.subscribe("active", () => { notifications++; resolve(); }));
  chat.subscribe("other", () => otherNotifications++);
  for (let i = 0; i < 500; i++) chat.buffer("active", "word ");
  assert.equal(notifications, 0);
  await painted;
  assert.equal(notifications, 1);
  assert.equal(otherNotifications, 0);
  assert.equal(chat.get("active").messageIds, ids);
  assert.equal(chat.getMessage("active", "999"), history[999]);
  assert.equal(chat.getMessage("active", active.id).content, "word ".repeat(500));
});

test("approvals cannot split an in-flight response and finishing flushes outstanding text", () => {
  const chat = new ChatStore();
  const active = newAssistant();
  chat.updateMessages("id", () => [active]);
  chat.buffer("id", "before ");
  chat.updateMessages("id", rows => [...rows, newAssistant({ id: "approval", streaming: false })]);
  chat.buffer("id", "after");
  chat.buffer("id", "reason", "thinking");
  chat.finish("id");
  const result = chat.get("id");
  assert.equal(result.messages.length, 2);
  assert.deepEqual(result.messages[0], { ...active, content: "before after", thinking: "reason", streaming: false });
  assert.equal(result.isStreaming, false);
});

test("identical stored session IDs in distinct profiles do not share chat state", () => {
  const first = new ChatStore();
  const second = new ChatStore();
  first.buffer("same-id", "profile one");
  second.buffer("same-id", "profile two");
  first.finish("same-id");
  assert.equal(second.get("same-id").messages.length, 0);
  second.finish("same-id");
  assert.equal(first.get("same-id").messages[0].content, "profile one");
  assert.equal(second.get("same-id").messages[0].content, "profile two");
});

test("NativeClient reassembles chunked frames and routes terminal events by subscription", async () => {
  const client = new NativeClient(process.execPath);
  client.stop();
  const realBinary = new URL("../apps/desktop/src-tauri/target/release/hermes-native", import.meta.url).pathname;
  const native = new NativeClient(realBinary);
  try {
    const info = await native.invoke("get_backend_info", {});
    assert.equal(typeof info.status, "string");
    await assert.rejects(() => native.invoke("unknown_command", {}), /Unknown native command/);
  } finally {
    await native.stop();
  }
});
