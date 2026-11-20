import type { RenderRequest } from "./markdown.worker";

type Result = string[] | string | null;
const pending = new Map<number, (result: Result) => void>();
let worker: Worker | undefined;
let nextId = 0;

function stop() {
  worker?.terminate();
  worker = undefined;
  const jobs = [...pending.values()];
  pending.clear();
  for (const resolve of jobs) resolve(null);
}

export function renderInWorker(request: RenderRequest, receive: (result: Result) => void) {
  const id = ++nextId;
  try {
    if (!worker) {
      worker = new Worker(new URL("./markdown.worker.ts", import.meta.url), { type: "module" });
      worker.onmessage = (event: MessageEvent<{ id: number; result: Result }>) => {
        const job = pending.get(event.data.id);
        pending.delete(event.data.id);
        job?.(event.data.result);
      };
      worker.onerror = event => { console.error("Markdown worker:", event.message); stop(); };
      worker.onmessageerror = stop;
    }
    pending.set(id, receive);
    worker.postMessage({ id, request });
  } catch {
    pending.delete(id);
    receive(null);
  }
  return () => { pending.delete(id); };
}
