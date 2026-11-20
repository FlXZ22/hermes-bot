import test from 'node:test';
import assert from 'node:assert/strict';
import { normalizeServerUrl, normalizeSshTarget, parseSavedConnection, connectionError } from '../packages/hermes-client/dist/connection.js';
import { HermesClient, HermesApiError } from '../packages/hermes-client/dist/client.js';

test('server addresses preserve reverse-proxy paths and normalize safe defaults', () => {
  assert.equal(normalizeServerUrl(' hermes.example.com/agent/ '), 'https://hermes.example.com/agent');
  assert.equal(normalizeServerUrl('localhost:9119/'), 'http://localhost:9119');
  assert.equal(normalizeServerUrl('127.0.0.1:9119'), 'http://127.0.0.1:9119');
  assert.equal(normalizeServerUrl('https://example.com/hermes'), 'https://example.com/hermes');
  for (const value of ['', 'file:///etc/passwd', 'javascript:alert(1)', 'https://user:password@example.com', 'https://example.com?token=secret', 'https://example.com/#fragment']) {
    assert.throws(() => normalizeServerUrl(value));
  }
});

test('corrupt or unsupported saved connections return to setup', () => {
  for (const value of [null, {}, 'oops', { id: 'remote', name: 'Remote', url: 'file:///bad', mode: 'dashboard', authType: 'none' }]) assert.equal(parseSavedConnection(value), null);
  const conn = { id: 'remote', name: 'Server', url: 'https://example.com/', mode: 'dashboard', authType: 'bearer', token: 'fixture' };
  assert.equal(parseSavedConnection(conn).url, 'https://example.com');
  assert.equal(parseSavedConnection(conn).token, 'fixture');
});

test('public status never masks a rejected authenticated API', async () => {
  const client = new HermesClient({ id: 'remote', name: 'Server', url: 'https://example.com', mode: 'dashboard', authType: 'bearer', token: 'bad' });
  client.request = async path => {
    if (path === '/api/status') return { ok: true };
    throw new HermesApiError('Unauthenticated', 401);
  };
  const result = await client.testConnection();
  assert.equal(result.ok, false);
  assert.equal(result.errorStatus, 401);
  assert.match(connectionError({ status: result.errorStatus }), /sign in/i);
});

test('a login page returned as HTTP 200 is not a healthy Hermes API', async () => {
  const client = new HermesClient({ id: 'remote', name: 'Server', url: 'https://example.com', mode: 'dashboard', authType: 'none' });
  client.request = async path => path === '/api/status' ? { ok: true } : '<html>Sign in</html>';
  const result = await client.testConnection();
  assert.equal(result.ok, false);
  assert.match(result.error, /dashboard API/);
});

test('refresh updates shared credentials without replacing the live transport', () => {
  const client = new HermesClient({ id: 'remote', name: 'Server', url: 'https://example.com', mode: 'dashboard', authType: 'bearer', token: 'old' });
  const shared = client.getConnection();
  client.setCredentials({ token: 'new', refreshToken: 'rotated', expiresAt: 2000000000, authProvider: 'basic' });
  assert.equal(client.getConnection(), shared);
  assert.equal(shared.token, 'new');
  assert.equal(shared.refreshToken, 'rotated');
});

test('SSH aliases retain their config defaults and reject option injection', () => {
  const target = normalizeSshTarget({ host: ' my-alias ', configFile: ' ~/custom config ' });
  assert.equal(target.host, 'my-alias');
  assert.equal(target.port, undefined);
  assert.equal(target.configFile, '~/custom config');
  for (const value of [{host:'-oProxyCommand=bad'}, {host:'host;command'}, {host:'good',user:'user@another'}, {host:'good',port:0}, {host:'good',port:65536}]) assert.throws(() => normalizeSshTarget(value));
  const saved = parseSavedConnection({ id:'remote', name:'Server', url:'https://example.com', mode:'dashboard', authType:'bearer', ssh: target });
  assert.equal(saved.ssh.host, 'my-alias');
});
