import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { build } from 'vite';
import electron from 'electron';

const scratch = await mkdtemp(join(tmpdir(), 'hermes-chromium-perf-'));
const root = resolve('apps/web');
const outDir = join(scratch, 'dist');
try {
  await build({ root, configFile: join(root, 'vite.config.ts'), build: {
    outDir, rollupOptions: { input: join(root, 'qa/performance.html') },
  } });
  const child = spawn(electron, ['apps/chromium/main.cjs'], {
    env: { ...process.env, HERMES_CHROMIUM_USER_DATA: join(scratch, 'profile'), HERMES_DESKTOP_PERF_DIST: outDir },
    stdio: 'inherit',
  });
  const [status] = await once(child, 'exit');
  if (status !== 0) process.exitCode = status || 1;
} finally { await rm(scratch, { recursive: true, force: true }); }
