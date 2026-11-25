import { createServer } from 'node:http';
import { execFileSync, spawn } from 'node:child_process';
import { once } from 'node:events';
import { homedir, tmpdir } from 'node:os';
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import assert from 'node:assert/strict';
import electron from 'electron';

const scratch = await mkdtemp(join(tmpdir(), 'hermes-chromium-smoke-'));
const tauri = process.argv[2] === '--tauri' ? resolve(process.argv[3]) : undefined;
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
  if (!input.stream) {
    response.writeHead(200, { 'Content-Type': 'application/json' });
    response.end(JSON.stringify({
      id: 'fixture-auxiliary', object: 'chat.completion', model: 'fixture-model',
      choices: [{ index: 0, message: { role: 'assistant', content: 'Chromium gateway fixture' }, finish_reason: 'stop' }],
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
  const long = input.messages?.some(message => message.role === 'user' && message.content === 'Render a long formatted fixture.');
  if (long) {
    for (let block = 0; block < 64; block++) {
      response.write(`data: ${part({ content: `### Block ${block}\n\n**Long Markdown fixture** and a repeated paragraph used to exercise the embedded WebView worker.\n\n` })}\n\n`);
    }
  } else {
    response.write(`data: ${part({ content: '**Chromium gateway fixture**' })}\n\n`);
  }
  response.end(`data: ${part({}, 'stop')}\n\ndata: [DONE]\n\n`);
});
model.listen(0, '127.0.0.1');
await once(model, 'listening');
await writeFile(join(hermesHome, 'config.yaml'), `model:
  default: fixture-model
  provider: custom
  base_url: http://127.0.0.1:${model.address().port}/v1
  api_key: local-fixture
  context_length: 32768
memory:
  enabled: false
compression:
  enabled: false
`);
const inherited = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/(API_KEY|TOKEN|SECRET|HERMES_(PROFILE|DESKTOP_PERF_DIST))/.test(key)));
const config = join(scratch, 'config');
const env = {
  ...inherited, HERMES_HOME: hermesHome, HERMES_CHROMIUM_USER_DATA: join(config, 'hermes-grok-bot-chromium'),
  XDG_CONFIG_HOME: config, XDG_DATA_HOME: join(scratch, 'data'), XDG_CACHE_HOME: join(scratch, 'cache'),
  HERMES_CHROMIUM_WORK_FIXTURE: workspace,
  ...(tauri ? { HOME: join(scratch, 'home') } : {}),
};
let timeout;
try {
  await mkdir(env.HERMES_CHROMIUM_USER_DATA, { recursive: true });
  if (tauri) {
    await mkdir(env.HOME);
    execFileSync(electron, ['scripts/electron-storage-fixture.cjs'], { env, stdio: 'inherit', timeout: 15_000 });
  }
  const child = spawn(tauri ?? electron, tauri ? [] : ['scripts/chromium-smoke.cjs'], {
    env: { ...env, ...(tauri ? { HERMES_TAURI_QA: '1' } : {}) },
    stdio: tauri ? ['ignore', 'pipe', 'inherit'] : 'inherit',
  });
  let output = '';
  child.stdout?.on('data', chunk => { output += chunk; process.stdout.write(chunk); });
  timeout = setTimeout(() => child.kill(), tauri ? 120_000 : 75_000);
  const [status] = await once(child, 'exit');
  assert.equal(status, 0);
  if (tauri) assert.match(output, /^PASS Tauri secure origin/m);
  assert.ok(requests > 0);
  assert.ok(explored, 'Explore must send the selected workspace excerpt without tools');
  if (tauri) {
    const exported = JSON.parse(await readFile(join(env.HOME, 'Downloads', 'fixture-export.json'), 'utf8'));
    assert.equal(exported.format, 'hermes-work-session');
  }
} catch (error) {
  console.error(`Fixture model received ${requests} request(s).`);
  const diagnostics = await readFile(join(hermesHome, 'logs/errors.log'), 'utf8').catch(() => '');
  if (diagnostics) console.error(diagnostics.slice(-3000));
  throw error;
} finally {
  clearTimeout(timeout);
  model.closeAllConnections();
  model.close();
  await rm(scratch, { recursive: true, force: true });
}
