import { execFileSync, spawn } from "node:child_process";
import { once } from "node:events";
import { mkdir, mkdtemp, readFile, realpath, rm, stat } from "node:fs/promises";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

if (process.platform !== "linux" || !(process.env.DISPLAY || process.env.WAYLAND_DISPLAY)) {
  throw new Error("Run this comparison from a Linux graphical session.");
}

const chromium = await realpath(process.argv[2] ?? join(
  process.env.XDG_DATA_HOME || join(homedir(), ".local/share"),
  "hermes-grok-bot/chromium/current/hermes-chromium",
));
const tauri = await realpath(process.argv[3] ?? resolve("apps/desktop/src-tauri/target/release/hermes-grok-bot"));
const scratch = await mkdtemp(join(tmpdir(), "hermes-desktop-memory-"));
const MiB = 1024 * 1024;
const rounded = value => Math.round(value * 10) / 10;
const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];

function subtree(root) {
  const rows = execFileSync("ps", ["-e", "-o", "pid=,ppid="], { encoding: "utf8" });
  const children = new Map();
  for (const line of rows.trim().split("\n")) {
    const [pid, parent] = line.trim().split(/\s+/).map(Number);
    if (!children.has(parent)) children.set(parent, []);
    children.get(parent).push(pid);
  }
  const pids = [root];
  for (let index = 0; index < pids.length; index++) pids.push(...(children.get(pids[index]) ?? []));
  return pids;
}

async function measure(root, rendererFlag) {
  const pids = subtree(root);
  let renderer = false;
  let pss = 0;
  let rss = 0;
  let inaccessible = 0;
  for (const pid of pids) {
    const cmdline = await readFile(`/proc/${pid}/cmdline`, "utf8").catch(() => "");
    renderer ||= rendererFlag.test(cmdline);
    const status = await readFile(`/proc/${pid}/status`, "utf8").catch(() => "");
    const rssKiB = Number(/^VmRSS:\s+(\d+)/m.exec(status)?.[1] ?? 0);
    rss += rssKiB;
    if (rssKiB <= 0) continue;
    const data = await readFile(`/proc/${pid}/smaps_rollup`, "utf8").catch(() => "");
    const pssKiB = /^Pss:\s+(\d+)/m.exec(data)?.[1];
    if (pssKiB === undefined) inaccessible++;
    else pss += Number(pssKiB);
  }
  return renderer ? { pss: inaccessible ? null : rounded(pss / 1024), rss: rounded(rss / 1024), processes: pids.length } : null;
}

async function run(name, binary, rendererFlag, iteration) {
  const isolated = join(scratch, `${name}-${iteration}`);
  await mkdir(isolated);
  const child = spawn(binary, [], {
    detached: true,
    stdio: "ignore",
    cwd: isolated,
    env: {
      ...process.env,
      XDG_CONFIG_HOME: join(isolated, "config"),
      XDG_DATA_HOME: join(isolated, "data"),
      XDG_CACHE_HOME: join(isolated, "cache"),
      HERMES_HOME: join(isolated, "hermes"),
      HERMES_CHROMIUM_USER_DATA: join(isolated, "chromium"),
    },
  });
  const closed = once(child, "exit");
  try {
    // Identical first-run screen with no gateway or model subprocess.
    await once(child, "spawn");
    await delay(2900);
    if (child.exitCode !== null || child.signalCode !== null) throw new Error(`${name} closed before sampling.`);
    const sample = await measure(child.pid, rendererFlag);
    if (!sample) throw new Error(`${name} did not start a renderer.`);
    return sample;
  } finally {
    if (child.pid && child.exitCode === null && child.signalCode === null) {
      try { process.kill(-child.pid, "SIGTERM"); } catch { /* Already closed. */ }
      await Promise.race([closed, delay(2000, undefined, { ref: false })]);
      if (child.exitCode === null && child.signalCode === null) {
        try { process.kill(-child.pid, "SIGKILL"); } catch { /* Already closed. */ }
        await closed;
      }
    }
  }
}

try {
  const result = { scenario: "isolated first-run screen; no Hermes gateway", units: "MiB", chromium: [], tauri: [] };
  for (let i = 0; i < 3; i++) {
    const paths = [
      ["chromium", chromium, /--type=renderer\b/],
      ["tauri", tauri, /WebKitWebProcess\b/],
    ];
    for (const [name, binary, flag] of i % 2 ? paths.reverse() : paths) {
      result[name].push(await run(name, binary, flag, i));
    }
  }
  const size = Number(execFileSync("du", ["-sb", dirname(chromium)], { encoding: "utf8" }).split(/\s+/, 1)[0]);
  const summary = samples => ({
    pss: samples.every(sample => sample.pss !== null) ? median(samples.map(sample => sample.pss)) : null,
    rss: median(samples.map(sample => sample.rss)),
  });
  console.log(JSON.stringify({
    ...result,
    medians: { chromium: summary(result.chromium), tauri: summary(result.tauri) },
    installPayload: {
      chromiumBundle: rounded(size / MiB),
      tauriBinary: rounded((await stat(tauri)).size / MiB),
      note: "Tauri uses the system WebKitGTK library; system dependencies and the separate rollback copy are not counted.",
    },
  }, null, 2));
} finally {
  await rm(scratch, { recursive: true, force: true });
}
