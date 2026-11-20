import { spawnSync } from "node:child_process";
import { once } from "node:events";
import { createServer } from "node:http";
import { createReadStream } from "node:fs";
import { mkdtemp, rm, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, extname, join, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptDir, "../apps/web");
const scratch = await mkdtemp(join(tmpdir(), "hermes-webkit-perf-"));
const outDir = join(scratch, "dist");
const executable = join(scratch, "webkit-perf");
const mime = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".woff2": "font/woff2" };
let server;
try {
  const flags = spawnSync("pkg-config", ["--cflags", "--libs", "webkit2gtk-4.1"], { encoding: "utf8" });
  if (flags.status !== 0) throw new Error(flags.stderr || "WebKitGTK development files are required.");
  const compile = spawnSync("cc", [join(scriptDir, "webkit-perf.c"), "-o", executable, ...flags.stdout.trim().split(/\s+/)], { stdio: "inherit" });
  if (compile.status !== 0) throw new Error("Could not compile the WebKitGTK fixture host.");
  await build({ root, configFile: join(root, "vite.config.ts"), build: { outDir, rollupOptions: {
    input: join(root, "qa/performance.html"),
  } } });
  server = createServer(async (request, response) => {
    const path = resolve(outDir, "." + new URL(request.url, "http://127.0.0.1").pathname);
    if (!path.startsWith(outDir + sep)) { response.writeHead(404).end(); return; }
    try {
      const file = await stat(path);
      if (!file.isFile()) { response.writeHead(404).end(); return; }
      response.writeHead(200, { "Content-Length": file.size, "Content-Type": mime[extname(path)] ?? "application/octet-stream" });
      createReadStream(path).pipe(response);
    } catch {
      response.writeHead(404).end();
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const { spawn } = await import("node:child_process");
  const run = spawn(executable, [`http://127.0.0.1:${server.address().port}/qa/performance.html`], { stdio: "inherit" });
  const [status] = await once(run, "exit");
  if (status !== 0) process.exitCode = status || 1;
} finally {
  server?.close();
  await rm(scratch, { recursive: true, force: true });
}
