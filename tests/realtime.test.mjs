import test from 'node:test';
import assert from 'node:assert/strict';
import { HermesClient } from '../packages/hermes-client/dist/client.js';
import { HermesRealtimeSession } from '../packages/hermes-client/dist/ws.js';
import { resolveStreamSessionId } from '../packages/hermes-client/dist/sessionIdentity.js';

const connection = { id: 'test', name: 'test', url: 'http://127.0.0.1:9119', mode: 'dashboard', authType: 'session-token', token: 'test-token' };

test('coalesced Hermes frames retain each originating session', () => {
  const rt = new HermesRealtimeSession(new HermesClient(connection));
  const events = [];
  rt.subscribe(event => events.push(event));
  for (const [sid, delta] of [['chat-a', 'first'], ['chat-b', 'other'], ['chat-a', 'last']]) {
    rt.handleRawFrame(JSON.stringify({ jsonrpc: '2.0', method: 'event', params: { type: 'text.delta', session_id: sid, payload: { delta } } }));
  }
  assert.deepEqual(events, [
    { type: 'text.delta', sessionId: 'chat-a', delta: 'first' },
    { type: 'text.delta', sessionId: 'chat-b', delta: 'other' },
    { type: 'text.delta', sessionId: 'chat-a', delta: 'last' },
  ]);
});

test('inference settings use the gateway key/value protocol and session scope', async () => {
  const rt = new HermesRealtimeSession(new HermesClient(connection));
  const calls = [];
  rt.rpc = async (method, params) => { calls.push({ method, params }); return {}; };
  await rt.setSessionConfig('live-a', { model: 'vendor/model', provider: 'openrouter', reasoning_effort: 'high', fast: false, yolo: true });
  assert.deepEqual(calls.map(c => c.params), [
    { session_id: 'live-a', key: 'model', value: 'vendor/model --provider openrouter', scope: 'session' },
    { session_id: 'live-a', key: 'reasoning', value: 'high', scope: 'session' },
    { session_id: 'live-a', key: 'fast', value: 'off', scope: 'session' },
    { session_id: 'live-a', key: 'yolo', value: 'on', scope: 'session' },
  ]);
  assert.ok(calls.every(c => c.method === 'config.set'));
});

test('gateway selection guards propagate to the caller', async () => {
  const rt = new HermesRealtimeSession(new HermesClient(connection));
  rt.rpc = async () => ({ confirm_required: true, confirm_message: 'Confirm this model in Hermes' });
  await assert.rejects(rt.setSessionConfig('live-a', { model: 'guarded-model' }), /Confirm this model/);
});

test('slash dispatch separates the command name from arguments', async () => {
  const rt = new HermesRealtimeSession(new HermesClient(connection));
  let call;
  rt.rpc = async (method, params) => { call = { method, params }; return {}; };
  await rt.dispatchCommand('live-a', '/status with multiple words');
  assert.deepEqual(call, { method: 'command.dispatch', params: { session_id: 'live-a', name: 'status', arg: 'with multiple words' } });
});

test('disconnect rejects pending RPCs immediately and clears their timers', async () => {
  const rt = new HermesRealtimeSession(new HermesClient(connection));
  const original = globalThis.WebSocket;
  globalThis.WebSocket = { OPEN: 1 };
  try {
    rt.ws = { readyState: 1, send() {}, close() {} };
    const pending = rt.rpc('session.resume', { session_id: 'old' });
    rt.disconnect();
    await assert.rejects(pending, /disconnected/);
    assert.equal(rt.pendingRpc.size, 0);
    assert.equal(rt.reconnectTimer, null);
  } finally { globalThis.WebSocket = original; }
});

test('session lists paginate at the real Hermes maximum of 100', async () => {
  const c = new HermesClient(connection);
  const queries = [];
  c.request = async (_path, { query }) => {
    queries.push(query);
    return { sessions: Array.from({ length: query.limit }, (_, i) => ({ id: `session-${query.offset + i}` })) };
  };
  const sessions = await c.listSessions(120);
  assert.equal(sessions.length, 120);
  assert.equal(sessions[119].id, 'session-119');
  assert.deepEqual(queries, [
    { limit: 100, offset: 0, order: 'recent', archived: 'exclude' },
    { limit: 20, offset: 100, order: 'recent', archived: 'exclude' },
  ]);
});

test('model catalogs preserve provider IDs and authentication state', async () => {
  const client = new HermesClient(connection);
  client.request = async () => ({ providers: [
    { slug: 'custom-colab', name: 'Colab GPU', authenticated: true, models: ['fixture-model'] },
    { slug: 'anthropic', name: 'Anthropic', authenticated: false, models: ['unconfigured-model'] },
  ] });
  const models = await client.getModelOptions();
  assert.equal(models[0].provider, 'custom-colab');
  assert.equal(models[0].configured, true);
  assert.equal(models[1].configured, false);
});

test('cron schedules normalize structured wire values for rendering', async () => {
  const client = new HermesClient(connection);
  client.request = async () => [{ id: 'job', name: 'Briefing', prompt: 'Hello', schedule: { kind: 'cron', expr: '0 9 * * *' }, enabled: false }];
  const [job] = await client.listCronJobs();
  assert.equal(job.schedule, '0 9 * * *');
  assert.equal(job.paused, true);
});


test('switching the visible chat cannot redirect a background response', () => {
  const ids = { 'stored-a': 'live-a', 'stored-b': 'live-b' };
  assert.equal(resolveStreamSessionId('live-a', 'stored-b', ids), 'stored-a');
  assert.equal(resolveStreamSessionId('stored-a', 'stored-b', ids), 'stored-a');
  assert.equal(resolveStreamSessionId(undefined, 'stored-b', ids), 'stored-b');
});

test('stored transcripts preserve reasoning and tool activity for collapsed rendering', async () => {
  const client = new HermesClient(connection);
  client.request = async () => ({ messages: [
    { id: 'a', role: 'assistant', content: 'Done', reasoning_content: '**Checked** the result', tool_calls: [
      { id: 'tc', function: { name: 'terminal', arguments: '{"command":"pwd"}' } },
    ] },
    { id: 't', role: 'tool', name: 'terminal', content: '/workspace' },
  ] });
  const messages = await client.getSessionMessages('session');
  assert.equal(messages[0].thinking, '**Checked** the result');
  assert.equal(messages[0].toolCalls[0].name, 'terminal');
  assert.equal(messages[0].toolCalls[0].arguments, '{"command":"pwd"}');
  assert.equal(messages[1].toolName, 'terminal');
  assert.equal(messages[1].role, 'tool');
});
