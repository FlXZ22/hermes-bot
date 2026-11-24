import { createServer } from 'node:http';
import { execFileSync, spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { homedir, tmpdir } from 'node:os';
import { mkdir, mkdtemp, readFile, writeFile, rm } from 'node:fs/promises';
import { once } from 'node:events';
import assert from 'node:assert/strict';
import { HermesClient } from '../packages/hermes-client/dist/client.js';
import { HermesRealtimeSession } from '../packages/hermes-client/dist/ws.js';

// Real Hermes gateway + real session database, with an isolated home and local
// deterministic model. This exercises persistence without model spend or edits
// to the user's transcripts/configuration.
const home = await mkdtemp(`${tmpdir()}/hermes-ui-e2e-`);
const token = randomBytes(32).toString('hex');
const reply = 'Hermes UI integration response.';
let modelRequests = 0;
let exploreRequest;
const model = createServer(async (req, res) => {
  if (req.method === 'GET') {
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify({ data: [{ id: 'hermes-ui-test', object: 'model' }] }));
    return;
  }
  let body = '';
  for await (const chunk of req) body += chunk;
  const data = JSON.parse(body);
  modelRequests++;
  const user = data.messages?.findLast(message => message.role === 'user');
  if (user?.content?.includes?.('Explore the Work fixture without editing.')) {
    exploreRequest = data;
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify({
      id: 'fixture-explore', object: 'chat.completion', model: 'hermes-ui-test',
      choices: [{ index: 0, message: {
        role: 'assistant', content: 'The existing Work fixture contains one line.',
        tool_calls: [{ id: 'call_explore_write', type: 'function', function: {
          name: 'write_file', arguments: JSON.stringify({ path: 'work-fixture.txt', content: 'Unexpected Explore write\n' }),
        } }],
      }, finish_reason: 'stop' }],
      usage: { prompt_tokens: 10, completion_tokens: 10, total_tokens: 20 },
    }));
    return;
  }
  if (!data.stream) {
    res.setHeader('Content-Type', 'application/json');
    res.end(JSON.stringify({ id: 'fixture', object: 'chat.completion', model: 'hermes-ui-test', choices: [{ index: 0, message: { role: 'assistant', content: reply }, finish_reason: 'stop' }], usage: { prompt_tokens: 1, completion_tokens: 5, total_tokens: 6 } }));
    return;
  }
  res.setHeader('Content-Type', 'text/event-stream');
  const toolTurn = user?.content === 'Create the Work fixture file.' && !data.messages.some(message => message.role === 'tool');
  const deltas = toolTurn ? [
    { role: 'assistant', tool_calls: [{
      index: 0,
      id: 'call_work_file',
      type: 'function',
      function: { name: 'write_file', arguments: JSON.stringify({ path: 'work-fixture.txt', content: 'Created by Hermes Work\n' }) },
    }] },
  ] : [{ role: 'assistant' }, { content: 'Hermes UI ' }, { content: 'integration response.' }];
  for (const delta of deltas) {
    res.write(`data: ${JSON.stringify({ id: 'fixture', object: 'chat.completion.chunk', model: 'hermes-ui-test', choices: [{ index: 0, delta, finish_reason: null }] })}\n\n`);
  }
  res.end(`data: ${JSON.stringify({ id: 'fixture', object: 'chat.completion.chunk', model: 'hermes-ui-test', choices: [{ index: 0, delta: {}, finish_reason: toolTurn ? 'tool_calls' : 'stop' }] })}\n\ndata: [DONE]\n\n`);
});
model.listen(0, '127.0.0.1');
await once(model, 'listening');
const modelPort = model.address().port;
await writeFile(`${home}/config.yaml`, `model:\n  default: hermes-ui-test\n  provider: custom\n  base_url: http://127.0.0.1:${modelPort}/v1\n  api_key: local-fixture\nagent:\n  max_turns: 2\nmemory:\n  enabled: false\ncompression:\n  enabled: false\n`);
const python = process.env.HERMES_PYTHON || `${homedir()}/.hermes/hermes-agent/venv/bin/python`;
const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !/(API_KEY|TOKEN|SECRET|HERMES_PROFILE)/.test(key)));
const child = spawn(python, ['-m', 'hermes_cli.main', 'serve', '--host', '127.0.0.1', '--port', '0'], {
  env: { ...env, HERMES_HOME: home, HERMES_DASHBOARD_SESSION_TOKEN: token, PYTHONUNBUFFERED: '1' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let rt;
let diagnostics = '';
child.stderr.on('data', chunk => { diagnostics = (diagnostics + chunk).slice(-8000); });
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
  });
  const client = new HermesClient({ id: 'e2e', name: 'E2E', url: `http://127.0.0.1:${port}`, mode: 'dashboard', authType: 'session-token', token });
  rt = new HermesRealtimeSession(client);
  await rt.connect();
  const session = await rt.createSession({ source: 'desktop', model: 'hermes-ui-test', provider: 'custom', title: 'E2E conversation', cwd: home });
  const deltas = [];
  const completion = new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('No completed response from gateway')), 40_000);
    rt.subscribe(event => {
      if (event.sessionId !== session.session_id) return;
      if (event.type === 'text.delta') deltas.push(event.delta);
      if (event.type === 'error') { clearTimeout(timer); reject(new Error(event.message)); }
      if (event.type === 'message.complete') { clearTimeout(timer); resolve(); }
    });
  });
  const attachment = await rt.rpc('file.attach', {
    session_id: session.session_id, name: 'fixture.txt',
    data_url: 'data:text/plain;base64,' + Buffer.from('Fixture attachment').toString('base64'),
  });
  assert.ok(attachment.ref_text?.startsWith('@file:'));
  assert.ok(attachment.path.startsWith(home));
  await rt.submitPrompt(session.session_id, `Respond with the fixture response. Do not use tools.\n${attachment.ref_text}`);
  await completion;
  assert.equal(deltas.join(''), reply);
  assert.ok(modelRequests > 0);
  console.log('PASS real prompt.submit → model → streamed response → message.complete');
  const messages = await client.getSessionMessages(session.stored_session_id);
  assert.ok(messages.some(m => m.role === 'user'));
  assert.ok(messages.some(m => m.role === 'assistant' && m.content === reply));
  await rt.setSessionTitle(session.session_id, 'Renamed fixture');
  const sessions = await client.listSessions(120);
  assert.equal(sessions.find(s => s.id === session.stored_session_id)?.title, 'Renamed fixture');
  console.log('PASS transcript persistence, durable IDs, rename, paginated session listing');
  const branch = await rt.branchSession(session.session_id);
  assert.equal(typeof branch.stored_session_id, 'string');
  const resumed = await rt.resumeSession(session.stored_session_id, { omit_messages: true });
  assert.equal(typeof resumed.session_id, 'string');
  console.log('PASS conversation branch, resume, and file attachment staging');
  const workspace = `${home}/project with spaces`;
  await mkdir(workspace);
  execFileSync("git", ["init", "--quiet"], { cwd: workspace });
  const work = await rt.createSession({ source: 'desktop', title: 'Fixture Work', cwd: workspace, model: 'hermes-ui-test', provider: 'custom' });
  assert.equal(work.info.cwd, workspace);
  const workPlan = await rt.dispatchCommand(work.session_id, '/plan Add a file');
  assert.equal(workPlan.type, 'send');
  assert.match(workPlan.message, /Add a file/);
  const workGoal = await rt.dispatchCommand(work.session_id, '/goal Add a file');
  assert.equal(workGoal.type, 'send');
  assert.equal(workGoal.message, 'Add a file');
  const paused = await rt.dispatchCommand(work.session_id, '/goal pause');
  assert.equal(paused.type, 'exec');
  assert.match(paused.output, /Goal paused/);
  const resumedGoal = await rt.dispatchCommand(work.session_id, '/goal resume');
  assert.equal(resumedGoal.type, 'send');
  assert.equal(resumedGoal.display, '/goal resume');
  await rt.dispatchCommand(work.session_id, '/goal clear');
  console.log('PASS Work workspace binding, plan dispatch, and native goal lifecycle');
  const workComplete = new Promise((resolve, reject) => {
    const timer = setTimeout(() => { unsubscribe(); reject(new Error('No completed Work response')); }, 40_000);
    const unsubscribe = rt.subscribe(event => {
      if (event.sessionId !== work.session_id || (event.type !== 'error' && event.type !== 'message.complete')) return;
      unsubscribe();
      clearTimeout(timer);
      if (event.type === 'error') reject(new Error(event.message));
      else resolve();
    });
  });
  await rt.submitPrompt(work.session_id, 'Create the Work fixture file.');
  await workComplete;
  assert.equal(await readFile(`${workspace}/work-fixture.txt`, 'utf8'), 'Created by Hermes Work\n');
  const workGitStatus = await client.getGitStatus(workspace);
  assert.ok(workGitStatus?.files.some(file => file.path === 'work-fixture.txt' && file.untracked));
  const workDiff = await client.getGitFileDiff(workspace, 'work-fixture.txt', 'unstaged');
  assert.match(workDiff.diff, /\+Created by Hermes Work/);
  console.log('PASS Hermes Work writes a file inside the requested workspace and reports its diff');
  assert.ok((await client.listFiles(workspace)).some(entry => entry.name === 'work-fixture.txt'));
  const fixturePath = `${workspace}/work-fixture.txt`;
  const source = await client.readFile(fixturePath, { expectedPath: fixturePath, maxBytes: 64 * 1024 });
  await assert.rejects(client.readFile(fixturePath, { expectedPath: `${workspace}/wrong.txt`, maxBytes: 64 * 1024 }), /expected workspace file/);
  const beforeExplore = await client.getSessionMessages(work.stored_session_id);
  const explored = await rt.rpc('llm.oneshot', {
    session_id: work.session_id, task: 'work_explore', instructions: 'Answer in read-only mode.',
    input: `Explore the Work fixture without editing.\n${source}`, max_tokens: 1400,
  });
  assert.equal(explored.text, 'The existing Work fixture contains one line.');
  assert.ok(exploreRequest);
  assert.ok(!exploreRequest.tools?.length);
  assert.equal(await readFile(fixturePath, 'utf8'), source);
  assert.deepEqual(await client.getSessionMessages(work.stored_session_id), beforeExplore);
  console.log('PASS real Hermes Explore request has no tools, does not write files, and preserves history');
  const importTime = Date.now() / 1000;
  const imported = {
    id: 'work-import-fixture', title: 'Imported fixture', cwd: workspace, source: 'desktop',
    started_at: importTime, ended_at: importTime, model: 'hermes-ui-test',
    messages: [
      { role: 'user', content: 'Check a historical fixture.', timestamp: importTime },
      { role: 'assistant', content: 'The fixture is ready.', timestamp: importTime },
    ],
  };
  assert.deepEqual((await client.importSession(imported)).imported_ids, [imported.id]);
  assert.deepEqual((await client.importSession(imported)).skipped_ids, [imported.id]);
  assert.equal((await client.listSessions()).find(row => row.id === imported.id)?.cwd, workspace);
  assert.deepEqual((await client.getSessionMessages(imported.id)).map(({ role, content }) => [role, content]),
    imported.messages.map(({ role, content }) => [role, content]));
  const importedLive = await rt.resumeSession(imported.id, { omit_messages: true });
  assert.equal(importedLive.info.cwd, workspace);
  console.log('PASS native Work transcript import, duplicate retry, and workspace resume');
  execFileSync('git', ['config', 'user.name', 'Hermes fixture'], { cwd: workspace });
  execFileSync('git', ['config', 'user.email', 'hermes-fixture@example.invalid'], { cwd: workspace });
  await client.changeGitStaging(workspace, 'work-fixture.txt', true);
  assert.equal((await client.commitGitChanges(workspace, 'Create fixture')).ok, true);
  const fixtureRemote = `${home}/fixture-remote.git`;
  execFileSync('git', ['init', '--bare', '--quiet', fixtureRemote], { cwd: workspace });
  execFileSync('git', ['remote', 'add', 'origin', fixtureRemote], { cwd: workspace });
  assert.equal((await client.pushGitBranch(workspace)).ok, true);
  assert.match(execFileSync('git', ['ls-remote', '--heads', fixtureRemote], { cwd: workspace, encoding: 'utf8' }), /refs\/heads\//);
  execFileSync('git', ['branch', 'review-fixture'], { cwd: workspace });
  assert.equal((await client.switchGitBranch(workspace, 'review-fixture')).branch, 'review-fixture');
  const worktree = await client.createGitWorktree(workspace, 'worktree-fixture');
  assert.equal(worktree.branch, 'worktree-fixture');
  assert.ok(worktree.path.startsWith(`${workspace}/.worktrees/`));
  assert.equal(await readFile(`${worktree.path}/work-fixture.txt`, 'utf8'), 'Created by Hermes Work\n');
  console.log('PASS Hermes Git staging, commit, push to a local remote, branch switch, and worktree creation');
  await client.pinSession(session.stored_session_id, true);
  assert.equal((await client.listSessions(120)).find(s => s.id === session.stored_session_id)?.pinned, true);
  await client.archiveSession(session.stored_session_id, true);
  assert.equal((await client.listSessions(120, 'include')).find(s => s.id === session.stored_session_id)?.archived, true);
  await client.archiveSession(session.stored_session_id, false);
  console.log('PASS pin, archive, and restore');
  await client.createKanbanBoard({ name: 'Fixture board', slug: 'fixture' });
  const created = await client.createKanbanTask({ title: 'Fixture task', status: 'todo', priority: 'medium' }, 'fixture');
  const taskId = created.task?.id ?? created.id;
  assert.ok(taskId);
  await client.updateKanbanTask(taskId, { status: 'ready' }, 'fixture');
  const board = await client.getKanbanBoard('fixture');
  assert.ok(board.tasks.some(t => t.id === taskId && t.status === 'ready'));
  console.log('PASS board creation, task creation, status update, board hydration');
  const job = await client.createCronJob({ name: 'Fixture schedule', prompt: 'Fixture', schedule: '0 0 1 1 *' });
  const jobId = job.id ?? job.job?.id;
  assert.ok(jobId);
  await client.pauseCronJob(jobId);
  const jobs = await client.listCronJobs();
  assert.equal(typeof jobs[0].schedule, 'string');
  assert.equal(jobs.find(j => j.id === jobId)?.paused, true);
  await client.deleteCronJob(jobId);
  console.log('PASS cron creation, schedule normalization, pause, and deletion');
} catch (error) {
  console.error(error.stack);
  // Diagnostics contain fixture-only data; never print real home logs here.
  console.error(diagnostics);
  process.exitCode = 1;
} finally {
  rt?.disconnect();
  child.kill('SIGTERM');
  await Promise.race([once(child, 'exit'), new Promise(resolve => setTimeout(() => { child.kill('SIGKILL'); resolve(); }, 3000).unref())]);
  model.closeAllConnections();
  model.close();
  await rm(home, { recursive: true, force: true });
}
