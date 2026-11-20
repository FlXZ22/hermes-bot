const gateway = "http://hermes-perf.invalid";
const ids = ["perf-a", "perf-b"];
const originalFetch = window.fetch.bind(window);
const histories = Object.fromEntries(ids.map(id => [id, Array.from({ length: 1000 }, (_, i) => ({
  id: `${id}-${i}`,
  role: i % 2 ? "assistant" : "user",
  content: i % 2 ? `### Response ${i}\n\nA **rich** paragraph.\n\n| A | B |\n| - | - |\n| 1 | 2 |` : `Prompt ${i}`,
}))]));

window.fetch = async (input, options) => {
  const url = new URL(input instanceof Request ? input.url : input, location.href);
  if (url.origin !== gateway) return originalFetch(input, options);
  if (url.pathname === "/api/profiles") return Response.json({ profiles: [{ name: "default", active: true }] });
  if (url.pathname === "/api/status") return Response.json({ status: "running" });
  if (url.pathname === "/api/sessions") {
    if (Number(url.searchParams.get("offset"))) return Response.json({ sessions: [] });
    return Response.json({ sessions: ids.map(id => ({ id, title: id, model: "fixture" })) });
  }
  const history = /^\/api\/sessions\/([^/]+)\/messages$/.exec(url.pathname);
  return Response.json(history ? { messages: histories[history[1]] } : {});
};

let activeSocket: FixtureSocket;
class FixtureSocket {
  static OPEN = 1;
  readyState = 0;
  onopen?: () => void;
  onmessage?: (event: { data: string }) => void;
  constructor() {
    activeSocket = this;
    queueMicrotask(() => { this.readyState = FixtureSocket.OPEN; this.onopen?.(); });
  }
  send(data: string) {
    const { id, method, params } = JSON.parse(data);
    const result = method === "session.resume" ? { session_id: `live-${params.session_id}` } : {};
    queueMicrotask(() => this.onmessage?.({ data: JSON.stringify({ jsonrpc: "2.0", id, result }) }));
  }
  event(type: string, payload: object = {}) {
    this.onmessage?.({ data: JSON.stringify({ method: "event",
      params: { type, session_id: "live-perf-a", payload },
    }) });
  }
  close() { this.readyState = 3; }
}

Object.defineProperty(window, "WebSocket", { value: FixtureSocket });
localStorage.setItem("hermes-grok-bot:connection", JSON.stringify({
  id: "performance-fixture", name: "Fixture", url: gateway, mode: "dashboard", authType: "none",
}));

const wait = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
const frame = () => new Promise<number>(resolve => requestAnimationFrame(resolve));
const percentile = (values: number[], ratio: number) => {
  const ordered = values.slice().sort((a, b) => a - b);
  return Math.round((ordered[Math.ceil(ordered.length * ratio) - 1] ?? 0) * 10) / 10;
};
async function until<T>(inspect: () => T | undefined): Promise<T> {
  const start = performance.now();
  while (performance.now() - start < 10_000) {
    const result = inspect();
    if (result) return result;
    await wait(15);
  }
  throw new Error("Fixture did not reach the expected UI state");
}

async function run() {
  await import("../src/main");
  const row = (id: string) => [...document.querySelectorAll<HTMLElement>(".gb-agent-row")].find(node => node.textContent?.includes(id));
  (await until(() => row("perf-a"))).click();
  await until(() => document.querySelector(".gb-bubble-agent h3"));
  await wait(400);
  const area = document.querySelector<HTMLTextAreaElement>(".gb-composer__textarea")!;
  const scroll = document.querySelector<HTMLElement>(".gb-transcript")!;
  const frames: number[] = [];
  const typing: number[] = [];
  const nativeSetValue = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!;
  let measuring = true;
  let previous = await frame();
  const capture = (time: number) => {
    frames.push(time - previous);
    previous = time;
    if (measuring) requestAnimationFrame(capture);
  };
  requestAnimationFrame(capture);

  activeSocket.event("message.start");
  for (let batch = 0; batch < 50; batch++) {
    for (let token = 0; token < 12; token++) {
      activeSocket.event("text.delta", { delta: token === 0
        ? `\n\n### Part ${batch}\n\n` : "A longer **formatted** response with `code` and links. " });
    }
    scroll.scrollTop = batch % 10 < 8 ? scroll.scrollHeight : scroll.scrollTop - 270;
    const typedAt = performance.now();
    nativeSetValue.call(area, "Draft " + "x".repeat(batch));
    area.dispatchEvent(new InputEvent("input", { bubbles: true, data: "x" }));
    void frame().then(() => { typing.push(performance.now() - typedAt); });
    await wait(25);
  }
  activeSocket.event("message.complete");
  await until(() => document.body.textContent?.includes("Part 49"));
  measuring = false;
  await frame();
  const mounted = document.querySelectorAll(".gb-entry").length;
  row("perf-b")?.click();
  await until(() => document.querySelector(".gb-bubble-agent h3"));
  await wait(100);
  const switchStarted = performance.now();
  row("perf-a")?.click();
  await frame();
  await frame();
  return {
    frameP95Ms: percentile(frames, 0.95),
    framesOver34Ms: frames.filter(value => value > 34).length,
    frames: frames.length,
    inputToFrameP95Ms: percentile(typing, 0.95),
    cachedSwitchToSecondFrameMs: Math.round(performance.now() - switchStarted),
    mountedRows: mounted,
  };
}

run().then(result => { document.title = "RESULT " + JSON.stringify(result); })
  .catch(error => { document.title = "ERROR " + String(error); });
