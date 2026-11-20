import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { homedir } from 'node:os';
import { createServer } from 'vite';

// Same-origin proxy for browser development. Production desktop traffic uses
// the Rust HTTP adapter and never needs a development server.
const token = process.env.HERMES_DEV_SESSION_TOKEN || randomBytes(32).toString('hex');
const webPort = Number(process.env.HERMES_DEV_PORT || 5173);
if (!Number.isInteger(webPort) || webPort < 1024 || webPort > 65535) throw new Error('HERMES_DEV_PORT must be between 1024 and 65535');
const origin = `http://127.0.0.1:${webPort}`;
const hermesHome = process.env.HERMES_HOME || `${homedir()}/.hermes`;
const python = process.env.HERMES_PYTHON || `${hermesHome}/hermes-agent/venv/bin/python`;
const child = spawn(python, ['-m', 'hermes_cli.main', 'serve', '--host', '127.0.0.1', '--port', '0'], {
  env: { ...process.env, HERMES_HOME: hermesHome, HERMES_DASHBOARD_SESSION_TOKEN: token, PYTHONUNBUFFERED: '1' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let vite;
let stopping = false;
const stop = async () => {
  if (stopping) return;
  stopping = true;
  await vite?.close();
  child.kill('SIGTERM');
  const timer = setTimeout(() => child.kill('SIGKILL'), 3000);
  timer.unref();
  child.once('exit', () => clearTimeout(timer));
};
process.on('SIGINT', () => { void stop(); });
process.on('SIGTERM', () => { void stop(); });
child.stderr.resume();
try {
  const port = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Hermes startup timed out')), 30_000);
    let output = '';
    child.once('error', reject);
    child.once('exit', code => { clearTimeout(timer); reject(new Error(`Hermes exited: ${code}`)); });
    child.stdout.on('data', chunk => {
      output += chunk;
      const match = /HERMES_BACKEND_READY port=(\d+)/.exec(output);
      if (match) { clearTimeout(timer); resolve(Number(match[1])); }
    });
  });
  const connection = { id: 'local-dev', name: 'Local Hermes', url: `${origin}/hermes`, mode: 'dashboard', authType: 'session-token', token };
  vite = await createServer({
    configFile: 'apps/web/vite.config.ts', root: 'apps/web',
    server: { port: webPort, proxy: { '/hermes': { target: `http://127.0.0.1:${port}`, ws: true, rewrite: path => path.replace(/^\/hermes/, '') } } },
    plugins: [{ name: 'local-hermes-handshake', configureServer(server) {
      server.middlewares.use('/__local_hermes', (req, res) => {
        if (req.headers.origin && req.headers.origin !== origin) { res.statusCode = 403; res.end(); return; }
        res.setHeader('Cache-Control', 'no-store');
        res.setHeader('Content-Type', 'application/json');
        res.end(JSON.stringify(connection));
      });
    } }],
  });
  child.once('exit', () => { if (!stopping) void stop(); });
  await vite.listen();
  console.log('Hermes gateway ready.');
  vite.printUrls();
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
  await stop();
}
