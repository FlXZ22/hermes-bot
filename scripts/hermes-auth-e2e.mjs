import { spawn } from 'node:child_process';
import { randomBytes, createHash } from 'node:crypto';
import { homedir, tmpdir } from 'node:os';
import { mkdtemp, writeFile, rm, readFile } from 'node:fs/promises';
import { once } from 'node:events';
import assert from 'node:assert/strict';
import { HermesClient } from '../packages/hermes-client/dist/client.js';
import { HermesRealtimeSession } from '../packages/hermes-client/dist/ws.js';
import { parseSavedConnection } from '../packages/hermes-client/dist/connection.js';

// Real Hermes password provider, native PKCE exchange, refresh, bearer REST and
// ticket-authenticated WebSocket. Isolated home and disposable credentials only.
const home = await mkdtemp(`${tmpdir()}/hermes-auth-e2e-`);
const password = randomBytes(24).toString('hex');
const secret = randomBytes(32).toString('hex');
await writeFile(`${home}/config.yaml`, `dashboard:\n  public_url: https://hermes-auth-fixture.invalid\n  basic_auth:\n    username: fixture\n    password: ${password}\n    secret: ${secret}\n`);
const python = process.env.HERMES_PYTHON || `${homedir()}/.hermes/hermes-agent/venv/bin/python`;
const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/(API_KEY|TOKEN|SECRET|HERMES_PROFILE|HERMES_DASHBOARD|HERMES_DESKTOP)/.test(key)));
let child, rt;
let stderr = '';
async function start() {
  child = spawn(python, ['-m', 'hermes_cli.main', 'serve', '--host', '127.0.0.1', '--port', '0'], {
    env: { ...env, HERMES_HOME: home, PYTHONUNBUFFERED: '1' }, stdio: ['ignore', 'pipe', 'pipe'],
  });
  child.stderr.on('data', chunk => { stderr = (stderr + chunk).slice(-5000); });
  const port = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('Hermes auth startup timed out')), 30_000);
    let output = '';
    child.on('error', reject);
    child.on('exit', code => { clearTimeout(timer); reject(new Error(`Hermes exited: ${code}`)); });
    child.stdout.on('data', chunk => {
      output += chunk;
      const match = /HERMES_BACKEND_READY port=(\d+)/.exec(output);
      if (match) { clearTimeout(timer); resolve(Number(match[1])); }
    });
  });
  return `http://127.0.0.1:${port}`;
}
async function stop() {
  if (!child || child.exitCode !== null) return;
  const exited = once(child, 'exit'); child.kill('SIGTERM');
  const force = setTimeout(() => child.kill('SIGKILL'), 3000);
  await exited; clearTimeout(force);
}
async function json(url, body, headers = {}) {
  const response = await fetch(url, { method: body ? 'POST' : 'GET', headers: { 'Content-Type': 'application/json', ...headers }, body: body ? JSON.stringify(body) : undefined, redirect: 'manual' });
  assert.equal(response.status, 200, `${new URL(url).pathname} returned ${response.status}`);
  return response.json();
}
try {
  let url = await start();
  const providers = await json(`${url}/api/auth/providers`);
  assert.ok(providers.providers.some(p => p.name === 'basic' && p.supports_password));
  const verifier = randomBytes(32).toString('base64url');
  const state = randomBytes(32).toString('base64url');
  const params = new URLSearchParams({ provider: 'basic', code_challenge: createHash('sha256').update(verifier).digest('base64url'), code_challenge_method: 'S256', redirect_uri: 'http://127.0.0.1:49123/callback', state });
  const authorize = await fetch(`${url}/auth/native/authorize?${params}`, { redirect: 'manual' });
  assert.equal(authorize.status, 302);
  const cookie = authorize.headers.getSetCookie().map(item => item.split(';')[0]).join('; ');
  assert.ok(cookie.includes('pkce'));
  const login = await json(`${url}/auth/password-login`, { provider: 'basic', username: 'fixture', password }, { Cookie: cookie });
  const callback = new URL(login.next);
  assert.equal(callback.searchParams.get('state'), state);
  const code = callback.searchParams.get('code'); assert.ok(code);
  const tokens = await json(`${url}/auth/native/token`, { code, code_verifier: verifier });
  assert.ok(tokens.access_token && tokens.refresh_token && tokens.expires_at);
  let connection = { id: 'remote:fixture', name: 'Fixture server', url, mode: 'dashboard', authType: 'bearer', token: tokens.access_token, refreshToken: tokens.refresh_token, expiresAt: tokens.expires_at, authProvider: tokens.provider };
  const invalid = await new HermesClient({ ...connection, token: 'incorrect-fixture' }).testConnection();
  assert.equal(invalid.ok, false); assert.equal(invalid.errorStatus, 401);
  let client = new HermesClient(connection);
  assert.equal((await client.testConnection()).ok, true);
  rt = new HermesRealtimeSession(client); await rt.connect(); await rt.rpc('gateway.ping', {}); rt.disconnect();
  console.log('PASS real password sign-in → PKCE code exchange → authenticated REST + WebSocket ticket');
  const rotated = await json(`${url}/auth/native/refresh`, { refresh_token: connection.refreshToken, provider: connection.authProvider });
  connection = { ...connection, token: rotated.access_token, refreshToken: rotated.refresh_token, expiresAt: rotated.expires_at };
  await writeFile(`${home}/saved-connection.json`, JSON.stringify(connection));
  const restored = parseSavedConnection(JSON.parse(await readFile(`${home}/saved-connection.json`, 'utf8')));
  assert.equal(restored.id, 'remote:fixture');
  assert.equal((await new HermesClient(restored).testConnection()).ok, true);
  await stop(); url = await start();
  client = new HermesClient({ ...restored, url });
  assert.equal((await client.testConnection()).ok, true);
  rt = new HermesRealtimeSession(client); await rt.connect(); await rt.rpc('gateway.ping', {}); rt.disconnect();
  console.log('PASS refresh, saved remote restoration, and reconnect after a real server restart');
} catch (error) {
  // Never print server logs, tokens, passwords or the connection fixture.
  console.error(error.message);
  if (stderr.includes('No module named')) console.error('The installed Hermes runtime is missing a required module.');
  process.exitCode = 1;
} finally {
  rt?.disconnect(); await stop(); await rm(home, { recursive: true, force: true });
}
