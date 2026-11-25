// Only embedded by the desktop-qa feature. Exercises the packaged origin and
// actual Tauri command/channel instead of substituting a mocked JS bridge.
const wire = [];
const NativeWebSocket = window.WebSocket;
window.WebSocket = class extends NativeWebSocket {
  constructor(...args) {
    super(...args);
    this.addEventListener("message", event => {
      if (typeof event.data !== "string") return;
      for (const line of event.data.trim().split("\n")) {
        try {
          const reply = JSON.parse(line);
          const type = reply.params?.type;
          const suffix = reply.params?.session_id ?? reply.result?.session_id;
          const detail = type === "message.delta"
            ? ` len=${String(reply.params.payload?.delta ?? reply.params.payload?.text ?? "").length}` : "";
          wire.push(`${type ?? `${reply.error ? "err" : "ok"}:${reply.id}`}${suffix ? `:${suffix}` : ""}${detail}`);
          if (wire.length > 28) wire.shift();
        } catch { /* Unrelated frame. */ }
      }
    });
  }
  send(data) {
    try {
      const call = JSON.parse(data);
      wire.push(`${call.method}:${call.id}${call.params?.session_id ? `:${call.params.session_id}` : ""}`);
    } catch { /* Unrelated frame. */ }
    return super.send(data);
  }
};
addEventListener("DOMContentLoaded", () => {
  const delay = ms => new Promise(resolve => setTimeout(resolve, ms));
  const assert = (condition, message) => { if (!condition) throw new Error(message); };
  const wait = async (get, description) => {
    const deadline = Date.now() + 40_000;
    do {
      const result = get();
      if (result) return result;
      await delay(40);
    } while (Date.now() < deadline);
    const transcript = document.querySelector(".gb-transcript");
    const scroll = transcript && `${transcript.scrollTop}/${transcript.clientHeight}/${transcript.scrollHeight}`;
    throw new Error(`${description}: ${document.body?.innerText.slice(0, 450)} (transcript: ${transcript?.textContent.slice(0, 200)}; scroll: ${scroll})`);
  };
  const labelOf = node => node.textContent.replace(/\p{Co}/gu, "").trim();
  const button = label => {
    const visible = Array.from(document.querySelectorAll("button"))
      .filter(node => !node.disabled && node.getClientRects().length);
    return visible.find(node => labelOf(node) === label)
      ?? visible.find(node => labelOf(node).startsWith(label));
  };
  const click = async label => (await wait(() => button(label), `Missing button ${label}`)).click();
  const fill = (target, value) => {
    const prototype = target instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, "value").set.call(target, value);
    target.dispatchEvent(new Event("input", { bubbles: true }));
  };
  const text = selector => document.querySelector(selector)?.textContent ?? "";
  const run = async () => {
    const core = window.__TAURI__?.core;
    assert(core?.invoke && core.Channel, "No Tauri core bridge");
    assert(!window.hermesDesktop && !window.require, "Unexpected Electron/Node bridge");
    assert(location.protocol === "tauri:", `Unexpected app origin ${location.protocol}`);
    assert(window.open("about:blank") === null, "Popup was not blocked");
    assert(await core.invoke("run_shell_command", { cwd: "/", command: "" }).then(() => false, () => true),
      "Unused raw shell command unexpectedly exposed");
    assert(await core.invoke("open_external_link", { address: "file:///dev/null" }).then(() => false, () => true),
      "Non-web link unexpectedly allowed");
    assert(localStorage.getItem("hermes-grok-bot:migrated-electron-v1") === "true" ||
      await wait(() => localStorage.getItem("hermes-grok-bot:migrated-electron-v1"), "Renderer migration did not settle"),
      "Missing migration marker");
    assert(localStorage.getItem("hermes-grok-bot:connection") === null, "Legacy credentials should not be imported into renderer");
    const draft = JSON.parse(localStorage.getItem("hermes-grok-bot:work-drafts") ?? "{}");
    assert(draft.fixture === "legacy café 🔧", "Legacy draft did not migrate");
    await click("Get started");
    await click("This computer");
    try {
      await click("Open Hermes");
    } catch (error) {
      const backend = await core.invoke("get_backend_info");
      throw new Error(`${error.message} [native backend: ${backend.status}; ${backend.error ?? ""}; ${backend.logs?.slice(-4).join(" ")}]`);
    }

    let input = await wait(() => document.querySelector(".gb-composer__textarea"), "Missing Chat composer");
    fill(input, "Say hello.");
    (await wait(() => document.querySelector('button[title^="Send message"]:not([disabled])'), "Missing Send button")).click();
    await wait(() => text(".gb-transcript strong").includes("Chromium gateway fixture"), "No formatted Hermes model response");
    const saved = await core.invoke("load_connection");
    assert(saved.id === "local-rust" && saved.token === undefined, "Native local connection storage mismatch");
    fill(input, "Render a long formatted fixture.");
    (await wait(() => document.querySelector('button[title^="Send message"]:not([disabled])'), "Missing Send button")).click();
    await wait(() => text('.gb-transcript .gb-markdown[data-chunked="true"]').includes("Block 63"), "Embedded WebView Markdown worker did not finish");

    const workspace = draft.workspace;
    assert(workspace, "Fixture workspace not staged");
    (await wait(() => document.querySelector('.gb-sidebar-nav button[title="Work"]'), "Missing Work navigation")).click();
    input = await wait(() => document.querySelector(".gb-work-setup input"), "Missing workspace chooser");
    fill(input, workspace);
    await click("Open workspace");
    await wait(() => text(".gb-work-toolbar").includes(workspace), "Work did not open the chosen folder");
    input = await wait(() => document.querySelector(".gb-composer__textarea"), "Missing Work composer");
    fill(input, "/explore What does this Work fixture contain? @file:work-fixture.txt");
    (await wait(() => document.querySelector('button[title^="Send message"]:not([disabled])'), "Missing Work Send button")).click();
    await wait(() => text(".gb-transcript").includes("The Chromium fixture contains a single line."), "Explore did not return a read-only answer");
    await click("Git");
    (await wait(() => document.querySelector('button[aria-label="Stage work-fixture.txt"]'), "Missing file staging control")).click();
    await wait(() => document.querySelector('button[aria-label="Unstage work-fixture.txt"]'), "Git did not refresh");
    const diff = await core.invoke("git_workspace_status", { dirPath: workspace, scope: "staged" });
    assert(diff.diff.includes("Chromium Git fixture"), "Native Git index did not include the fixture");
    const exported = await core.invoke("save_work_export", {
      fileName: "fixture-export.json",
      contents: JSON.stringify({ format: "hermes-work-session", version: 1 }),
    });
    assert(exported.endsWith("/fixture-export.json"), "Native export did not save the fixture");

    const id = "tauri-smoke";
    const onEvent = new core.Channel();
    let output = "";
    let generation;
    onEvent.onmessage = event => {
      if (event.type === "started") generation = event.generation;
      if (event.type === "output") {
        const bytes = atob(event.data);
        output += bytes;
        if (generation) void core.invoke("terminal_acknowledge", { id, generation, bytes: bytes.length });
      }
    };
    await core.invoke("terminal_start", { id, cwd: workspace, cols: 80, rows: 24, onEvent });
    try {
      await core.invoke("terminal_write", { id, data: "printf '%s%s\\n' '__TAURI_' 'PTY__'\n" });
      await wait(() => output.includes("__TAURI_PTY__"), "Tauri terminal did not stream the PTY output");
    } finally {
      await core.invoke("terminal_close", { id });
    }
    return "PASS Tauri secure origin, Electron draft migration, local Hermes streaming, Markdown worker, Work Explore/Git/export, and native PTY channel";
  };
  void run().then(
    result => { document.title = `HERMES QA: ${result}`; },
    error => { document.title = `HERMES QA: FAIL ${error.message} (gateway: ${wire.join(", ")})`; },
  );
});
