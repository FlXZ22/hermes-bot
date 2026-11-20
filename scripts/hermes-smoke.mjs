import { spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { homedir } from 'node:os';
import { once } from 'node:events';
import assert from 'node:assert/strict';
import { HermesClient } from '../packages/hermes-client/dist/client.js';
import { HermesRealtimeSession } from '../packages/hermes-client/dist/ws.js';

// Real gateway, read-only API checks. Session creation is lazy in Hermes and
// does not persist a transcript until a prompt is submitted. No model calls.
const token = randomBytes(32).toString('hex');
const hermesHome = process.env.HERMES_HOME || `${homedir()}/.hermes`;
const python = process.env.HERMES_PYTHON || `${hermesHome}/hermes-agent/venv/bin/python`;
const child = spawn(python, ['-m', 'hermes_cli.main', 'serve', '--host', '127.0.0.1', '--port', '0'], {
  env: { ...process.env, HERMES_HOME: hermesHome, HERMES_DASHBOARD_SESSION_TOKEN: token, PYTHONUNBUFFERED: '1' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let rt;
try {
  const port = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Hermes startup timed out')), 30_000);
    let output = '';
    child.on('error', reject);
    child.on('exit', code => { clearTimeout(timer); reject(new Error(`Hermes exited: ${code}`)); });
    child.stdout.on('data', chunk => {
      output += chunk;
      const match = /HERMES_BACKEND_READY port=(\d+)/.exec(output);
      if (match) { clearTimeout(timer); resolve(Number(match[1])); }
    });
    // Drain diagnostics, without printing user configuration or credentials.
    child.stderr.resume();
  });
  const client = new HermesClient({ id: 'smoke', name: 'Smoke', url: `http://127.0.0.1:${port}`, mode: 'dashboard', authType: 'session-token', token });
  const checks = ['getStatus', 'listProfiles', 'listSessions', 'getModelInfo', 'getModelOptions', 'listSkills', 'listToolsets', 'listMcpServers', 'listCronJobs', 'listMessagingPlatforms', 'listWebhooks', 'getUsageAnalytics', 'listKanbanBoards', 'getKanbanBoard'];
  let failures = 0;
  for (const check of checks) {
    try {
      const result = await client[check]();
      console.log(`PASS ${check}${Array.isArray(result) ? ` (${result.length} records)` : ''}`);
    } catch (err) {
      console.log(`FAIL ${check}: ${err.status ?? err.name}`);
      failures++;
    }
  }
  rt = new HermesRealtimeSession(client);
  await rt.connect();
  const session = await rt.createSession({ source: 'desktop', title: 'UI compatibility check' });
  assert.equal(typeof session.session_id, 'string');
  assert.equal(typeof session.stored_session_id, 'string');
  await rt.setSessionConfig(session.session_id, { reasoning_effort: 'medium', fast: false, yolo: false });
  const ping = await rt.rpc('gateway.ping');
  assert.ok(ping);
  console.log('PASS WebSocket: authenticated connection, session creation, inference controls, heartbeat');
  if (failures) process.exitCode = 1;
} finally {
  rt?.disconnect();
  child.kill('SIGTERM');
  await Promise.race([once(child, 'exit'), new Promise(resolve => setTimeout(() => { child.kill('SIGKILL'); resolve(); }, 3000).unref())]);
}
