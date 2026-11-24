import { createServer } from 'node:http';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { homedir, tmpdir } from 'node:os';
import { mkdtemp, mkdir, rm, symlink, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import electron from 'electron';

const scratch = await mkdtemp(join(tmpdir(), 'hermes-chromium-smoke-'));
const hermesHome = join(scratch, 'hermes');
const workspace = join(scratch, 'Work fixture');
await mkdir(hermesHome);
await mkdir(workspace);
execFileSync('git', ['init', '--quiet'], { cwd: workspace });
await writeFile(join(workspace, 'work-fixture.txt'), 'Chromium Git fixture\n');
await symlink(join(homedir(), '.hermes/hermes-agent'), join(hermesHome, 'hermes-agent'));
let requests = 0;
let explored = false;
const model = createServer(async (request, response) => {
  if (request.method === 'GET') {
    response.writeHead(200, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({ data: [{ id: 'fixture-model', object: 'model' }] }));
    return;
  }
  let body = '';
  for await (const chunk of request) body += chunk;
  const input = JSON.parse(body);
  requests++;
  if (input.messages?.some(message => typeof message.content === 'string' && message.content.includes('What does this Work fixture contain?'))) {
    explored = !input.tools?.length && input.messages.some(message => message.content?.includes?.('Chromium Git fixture'));
    response.writeHead(200, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({
      id: 'fixture-explore', object: 'chat.completion', model: 'fixture-model',
      choices: [{ index: 0, message: { role: 'assistant', content: 'The Chromium fixture contains a single line.' }, finish_reason: 'stop' }],
      usage: { prompt_tokens: 10, completion_tokens: 10, total_tokens: 20 },
    }));
    return;
  }
  response.writeHead(200, { 'Content-Type': 'text/event-stream' });
  const part = (delta, reason = null) => JSON.stringify({
    id: 'fixture', object: 'chat.completion.chunk', model: 'fixture-model',
    choices: [{ index: 0, delta, finish_reason: reason }],
  });
  response.write(`data: ${part({ role: 'assistant' })}\n\n`);
  response.write(`data: ${part({ content: '**Chromium gateway fixture**' })}\n\n`);
  response.end(`data: ${part({}, 'stop')}\n\ndata: [DONE]\n\n`);
});
model.listen(0, '127.0.0.1');
await once(model, 'listening');
await writeFile(join(hermesHome, 'config.yaml'), `model:
  default: fixture-model
  provider: custom
  base_url: http://127.0.0.1:${model.address().port}/v1
  api_key: local-fixture
memory:
  enabled: false
compression:
  enabled: false
`);
const inherited = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/(API_KEY|TOKEN|SECRET|HERMES_(PROFILE|DESKTOP_PERF_DIST))/.test(key)));
const child = spawn(electron, ['scripts/chromium-smoke.cjs'], {
  env: {
    ...inherited, HERMES_HOME: hermesHome, HERMES_CHROMIUM_USER_DATA: join(scratch, 'profile'),
    XDG_CONFIG_HOME: join(scratch, 'config'),
    HERMES_CHROMIUM_WORK_FIXTURE: workspace,
  },
  stdio: 'inherit',
});
const timeout = setTimeout(() => child.kill(), 75_000);
try {
  const [status] = await once(child, 'exit');
  assert.equal(status, 0);
  assert.ok(requests > 0);
  assert.ok(explored, 'Explore must send the selected workspace excerpt without tools');
} finally {
  clearTimeout(timeout);
  model.closeAllConnections();
  model.close();
  await rm(scratch, { recursive: true, force: true });
}
