import { spawnSync } from "node:child_process";

const repeats = Number(process.argv[2] ?? 3);
if (!Number.isInteger(repeats) || repeats < 3 || repeats > 9 || repeats % 2 === 0) {
  throw new Error("Choose an odd number of runs from 3 to 9.");
}
const median = values => [...values].sort((a, b) => a - b)[Math.floor(values.length / 2)];
const round = value => Math.round(value * 10) / 10;

function benchmark(script) {
  const run = spawnSync(process.execPath, [script, String(repeats)], { encoding: "utf8", timeout: 330_000 });
  const rows = [...(run.stdout ?? "").matchAll(/^RESULT (.+)$/gm)].map(match => JSON.parse(match[1]));
  if (run.status !== 0 || rows.length !== repeats) {
    throw new Error(`${script} did not finish benchmarking:\n${run.error ?? run.stderr}\n${run.stdout ?? ""}`);
  }
  return {
    runs: rows,
    frameP95Ms: median(rows.map(result => result.frameP95Ms)),
    inputToFrameP95Ms: median(rows.map(result => result.inputToFrameP95Ms)),
    slowFramePercentage: round(100 * median(rows.map(result => result.framesOver34Ms / result.frames))),
  };
}

// Run serially so that windows don't compete for compositor or CPU time.
const chromium = benchmark("scripts/chromium-perf.mjs");
const webkit = benchmark("scripts/webkit-perf.mjs");
const thresholds = {
  frameP95Ms: round(chromium.frameP95Ms + 1000 / 60),
  inputToFrameP95Ms: round(chromium.inputToFrameP95Ms + 1000 / 60),
  slowFramePercentage: round(chromium.slowFramePercentage + 10),
};
const failed = Object.keys(thresholds).filter(metric => webkit[metric] > thresholds[metric]);
console.log(JSON.stringify({ chromium, webkit, thresholds, failed }, null, 2));
if (failed.length) {
  console.error(`Tauri responsiveness gate failed: ${failed.join(", ")}.`);
  process.exitCode = 1;
}
