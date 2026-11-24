const { chromium, expect } = require(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
const assert = require('node:assert/strict');
const { readFile } = require('node:fs/promises');

async function checkWork() {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage({ viewport: { width: 1200, height: 850 }, reducedMotion: 'reduce' });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    let profile = 'default';
    let gateway;
    let goalStatus = 'No active goal';
    let staged = false;
    let pullRequests = 0;
    let projectWorkTip = false;
    let exploreUnavailable = false;
    const workSessions = [];
    const imports = [];
    const gitMutations = [];
    const calls = [];
    const fileReads = [];
    await page.addInitScript(disableWebCrypto => {
      if (disableWebCrypto) Object.defineProperty(crypto, 'subtle', { value: undefined });
      localStorage.setItem('hermes-grok-bot:connection', JSON.stringify({
        id: 'fixture', name: 'Fixture', url: 'http://hermes.fixture', mode: 'dashboard', authType: 'none',
      }));
    }, process.env.WORK_TEST_NO_WEB_CRYPTO === '1');
    await page.route('http://hermes.fixture/**', async route => {
      const request = route.request();
      const { pathname, searchParams } = new URL(request.url());
      let body = {};
      let status = 200;
      if (pathname === '/api/status') body = { status: 'running', model: 'fixture-model' };
      if (pathname === '/api/profiles') body = { profiles: ['default', 'writer'].map(name => ({ name, active: profile === name })) };
      if (pathname === '/api/profiles/active' && request.method() === 'POST') {
        profile = request.postDataJSON().name;
      }
      if (pathname === '/api/sessions') body = {
        sessions: [
          { id: `chat-${profile}`, title: `${profile} conversation`, profile },
          ...workSessions.filter(session => session.profile === profile).map(session =>
            projectWorkTip ? { ...session, id: `${session.id}-tip`, _lineage_root_id: session.id } : session
          ),
        ],
      };
      if (pathname.includes('/chat-') && pathname.endsWith('/messages')) {
        body = { messages: [{ id: 'reply-1', role: 'assistant', content: 'Chat history' }] };
      }
      if (pathname.endsWith('/work-writer/messages')) {
        body = { messages: [{ id: 'work-stored-1', role: 'assistant', content: 'Saved Work result' }] };
      }
      if (pathname === '/api/sessions/import') {
        const payload = request.postDataJSON();
        const [session] = payload.sessions;
        const alreadyImported = workSessions.some(row => row.id === session.id && row.profile === payload.profile);
        imports.push(payload);
        if (!alreadyImported) workSessions.push({ ...session, profile: payload.profile });
        body = { ok: true, imported_ids: alreadyImported ? [] : [session.id], skipped_ids: alreadyImported ? [session.id] : [] };
      }
      if (pathname.includes('/work-import-') && pathname.endsWith('/messages')) {
        const session = workSessions.find(row => pathname === `/api/sessions/${row.id}/messages` && row.profile === profile);
        body = { messages: session?.messages ?? [] };
      }
      if (pathname === '/api/files') {
        const path = searchParams.get('path');
        const folders = {
          '/projects/example': [
            { name: 'file name.txt', path: '/projects/example/file name.txt', size: 17 },
            { name: 'outside.txt', path: '/outside.txt', size: 10 },
          ],
          '/projects/example/.opencode/commands': [{ name: 'team', is_directory: true }],
          '/projects/example/.opencode/commands/team': [{ name: 'review.md' }],
        };
        status = Object.hasOwn(folders, path) ? 200 : 404;
        body = { entries: folders[path] ?? [] };
      }
      if (pathname === '/api/files/read') {
        const files = {
          '/projects/example/opencode.jsonc': `{
            // A project command in OpenCode v2 format.
            "commands": { "check": { "template": "Check $1 against $2. Raw arguments: $ARGUMENTS", "description": "Compare files", }, },
          }`,
          '/projects/example/.opencode/commands/team/review.md': '---\ndescription: Review this branch\n---\nReview the summary for $ARGUMENTS.\n\n!`git diff --stat`',
        };
        const path = searchParams.get('path');
        fileReads.push(path);
        if (path === '/projects/example/file name.txt') {
          body = { path, size: 17, content: 'Explore fixture.\n' };
        } else {
          status = Object.hasOwn(files, path) ? 200 : 404;
          body = { content: files[path] ?? '' };
        }
      }
      if (pathname === '/api/git/worktrees') body = { worktrees: [{ path: '/projects/example' }] };
      if (pathname === '/api/git/status') body = {
        branch: 'main', changed: 1, files: [{ path: 'fixture.txt', untracked: !staged, unstaged: !staged, staged }],
      };
      if (pathname === '/api/git/review/diff') body = { diff: '+ Fixture change' };
      if (pathname === '/api/git/file-diff') body = { diff: '+ Fixture change' };
      if (pathname === '/api/git/review/stage' || pathname === '/api/git/review/unstage') {
        assert.equal(request.postDataJSON().file, ':(top,literal)fixture.txt');
        staged = pathname.endsWith('/stage');
        body = { ok: true };
      }
      if (['/api/git/review/commit', '/api/git/review/push', '/api/git/branch/switch', '/api/git/worktree/add'].includes(pathname)) {
        const payload = request.postDataJSON();
        gitMutations.push([pathname, payload]);
        body = pathname.endsWith('/add') ? { path: '/projects/example/.worktrees/feature', branch: 'feature' }
          : pathname.endsWith('/switch') ? { branch: payload.branch } : { ok: true };
      }
      if (pathname === '/api/git/review/create-pr') {
        pullRequests++;
        body = { url: 'https://github.example/example/project/pull/12' };
      }
      await route.fulfill({ status, json: body, headers: { 'Access-Control-Allow-Origin': '*' } });
    });
    await page.routeWebSocket('ws://hermes.fixture/**', socket => {
      gateway = socket;
      socket.onMessage(raw => {
        const call = JSON.parse(raw.toString());
        const { id, method, params = {} } = call;
        calls.push(call);
        if (method === 'llm.oneshot') {
          if (exploreUnavailable) socket.send(JSON.stringify({ jsonrpc: '2.0', id, error: { code: -32601, message: 'Unknown method llm.oneshot' } }));
          else socket.send(JSON.stringify({ jsonrpc: '2.0', id, result: { text: 'Explore answer from the fixture.' } }));
          return;
        }
        let result = {};
        if (method === 'session.resume') result = { session_id: `live-${params.session_id}` };
        if (method === 'session.create') {
          if (params.cwd === '/invalid') {
            result = { session_id: 'invalid', stored_session_id: 'invalid', info: { cwd: '/fallback' } };
          } else {
            const session = { id: `work-${params.profile}`, title: params.title ?? 'New Work', profile: params.profile, cwd: params.cwd };
            workSessions.push(session);
            result = { session_id: `live-${session.id}`, stored_session_id: session.id, info: { cwd: session.cwd } };
          }
        }
        if (method === 'command.dispatch' && params.name === 'goal') {
          if (params.arg === 'status') result = { type: 'exec', output: goalStatus };
          else if (params.arg === 'pause') {
            goalStatus = 'Goal paused';
            result = { type: 'exec', output: goalStatus };
          } else {
            goalStatus = 'Goal set';
            result = { type: 'send', notice: goalStatus, message: 'Canonical goal prompt' };
          }
        }
        if (method === 'subagent.steer') result = { queued: true };
        if (method === 'subagent.interrupt') result = { found: true };
        if (method === 'command.dispatch' && params.name === 'queue') result = { type: 'send', message: params.arg };
        if (method === 'prompt.submit' && params.queued) result = { status: 'queued' };
        if (method === 'complete.path') result = { items: [{ text: '@file:file name.txt', display: 'file name.txt' }] };
        if (method === 'rollback.list') result = { enabled: true, checkpoints: [{ hash: 'abcdef01', timestamp: '', message: 'Before goal' }] };
        if (method === 'rollback.diff') result = { stat: '', diff: '+ change' };
        if (method === 'rollback.restore') result = { success: true };
        socket.send(JSON.stringify({ jsonrpc: '2.0', id, result }));
        if (method === 'session.interrupt') socket.send(JSON.stringify({
          method: 'event', params: { type: 'message.complete', session_id: params.session_id, payload: {} },
        }));
      });
    });
    await page.goto('http://127.0.0.1:5173/');
    const writer = page.locator('.gb-pinned-tile').filter({ hasText: 'writer' });
    await writer.click();
    await expect(writer).toHaveAttribute('aria-pressed', 'true');
    const chatRow = page.locator('.gb-agent-row').filter({ hasText: 'writer conversation' });
    await chatRow.click();
    await page.getByText('Chat history', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Work', exact: true }).click();
    const workspace = page.getByRole('textbox', { name: 'Workspace path', exact: true });
    await workspace.fill('/invalid');
    await page.getByRole('button', { name: 'Open workspace', exact: true }).click();
    await page.getByRole('alert').filter({ hasText: 'Hermes could not open that workspace' }).waitFor();
    assert.ok(calls.some(call => call.method === 'session.close' && call.params.session_id === 'invalid'));
    await workspace.fill('/projects/example/');
    await page.getByRole('button', { name: 'Open workspace', exact: true }).click();
    await page.getByRole('heading', { name: 'Work in example', exact: true }).waitFor();
    assert.ok(calls.some(call => call.method === 'session.create' && call.params.profile === 'writer' && call.params.cwd === '/projects/example'));
    await expect(chatRow).toHaveCount(0);
    const composer = page.getByPlaceholder('Describe a code change or type / for commands');
    await composer.fill('Review @file');
    await page.getByRole('button', { name: 'file name.txt', exact: true }).click();
    await expect(composer).toHaveValue('Review @file:`file name.txt` ');
    assert.ok(calls.some(call => call.method === 'complete.path' && call.params.cwd === '/projects/example'));
    await composer.fill('/goal Add fixtures');
    await composer.press('Enter');
    await page.locator('.gb-transcript').getByText('Goal set', { exact: true }).waitFor();
    await expect.poll(() => calls.filter(call => call.method === 'prompt.submit').map(call => [call.params.session_id, call.params.text])).toEqual([
      ['live-work-writer', 'Canonical goal prompt'],
    ]);
    await page.getByRole('button', { name: 'Pause goal', exact: true }).click();
    await page.locator('.gb-transcript').getByText('Goal paused', { exact: true }).waitFor();
    assert.equal(calls.filter(call => call.method === 'prompt.submit').length, 1);
    await composer.fill('Run the unit tests afterwards');
    await page.getByRole('button', { name: 'Queue', exact: true }).click();
    await expect.poll(() => calls.find(call => call.method === 'prompt.submit' && call.params.queued)?.params.text)
      .toBe('Run the unit tests afterwards');
    gateway.send(JSON.stringify({ method: 'event', params: {
      type: 'subagent.start', session_id: 'live-work-writer',
      payload: { subagent_id: 'child-1', goal: 'Review fixture' },
    } }));
    await page.getByText('Subagents · 1 running', { exact: true }).click();
    await page.getByRole('button', { name: 'Steer', exact: true }).click();
    await page.getByRole('textbox', { name: 'Subagent instruction' }).fill('Check expected output');
    await page.getByRole('button', { name: 'Send instruction', exact: true }).click();
    await expect.poll(() => calls.find(call => call.method === 'subagent.steer')?.params).toEqual({
      session_id: 'live-work-writer', subagent_id: 'child-1', text: 'Check expected output',
    });
    await page.getByRole('button', { name: 'Stop subagent', exact: true }).click();
    await expect.poll(() => calls.find(call => call.method === 'subagent.interrupt')?.params.subagent_id).toBe('child-1');
    await page.getByRole('button', { name: 'Chat', exact: true }).click();
    await page.getByText('Chat history', { exact: true }).waitFor();
    await expect(page.locator('.gb-agent-row').filter({ hasText: '/goal Add fixtures' })).toHaveCount(0);
    await page.getByRole('button', { name: 'Work', exact: true }).click();
    await page.locator('.gb-transcript').getByText('Goal paused', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Stop generating', exact: true }).click();
    await composer.fill('/diff');
    await composer.press('Enter');
    await page.locator('.gb-transcript pre').getByText('+ Fixture change', { exact: true }).waitFor();
    await page.getByRole('button', { name: 'Git', exact: true }).filter({ visible: true }).first().click();
    const review = page.getByRole('region', { name: 'Remote Git review' });
    await review.getByRole('combobox', { name: 'Changed file' }).selectOption('fixture.txt');
    await review.getByText('+ Fixture change', { exact: true }).waitFor();
    await review.getByRole('button', { name: 'Stage', exact: true }).click();
    await review.getByRole('button', { name: 'Unstage', exact: true }).click();
    await review.getByRole('button', { name: 'Stage', exact: true }).waitFor();
    await review.getByLabel('Commit message').fill('Checkpoint fixture');
    await review.getByRole('button', { name: 'Commit changes', exact: true }).click();
    await page.getByText('Changes committed', { exact: true }).waitFor();
    await review.getByRole('button', { name: 'Push', exact: true }).click();
    await page.getByText('Git push finished', { exact: true }).waitFor();
    await review.getByText('Switch branch', { exact: true }).first().click();
    await review.getByLabel('Existing branch name').fill('review');
    await review.getByRole('button', { name: 'Switch branch', exact: true }).click();
    await page.getByText('Switched to review', { exact: true }).waitFor();
    await review.getByText('New worktree', { exact: true }).click();
    await review.getByLabel('Branch name', { exact: true }).fill('feature');
    await review.getByRole('button', { name: 'Create worktree', exact: true }).click();
    await review.getByRole('button', { name: 'Open new worktree in Work', exact: true }).waitFor();
    assert.deepEqual(gitMutations, [
      ['/api/git/review/commit', { path: '/projects/example', message: 'Checkpoint fixture' }],
      ['/api/git/review/push', { path: '/projects/example' }],
      ['/api/git/branch/switch', { path: '/projects/example', branch: 'review' }],
      ['/api/git/worktree/add', { path: '/projects/example', branch: 'feature', name: 'feature' }],
    ]);
    await review.getByRole('button', { name: 'Create pull request', exact: true }).click();
    assert.equal(pullRequests, 0);
    await page.getByRole('button', { name: 'Push and create pull request', exact: true }).click();
    await page.getByRole('link', { name: 'https://github.example/example/project/pull/12' }).waitFor();
    assert.equal(pullRequests, 1);
    await page.getByRole('dialog').getByRole('button', { name: 'Close', exact: true }).last().click();
    await composer.fill('/export');
    const downloadStarted = page.waitForEvent('download');
    await composer.press('Enter');
    const download = await downloadStarted;
    const exported = JSON.parse(await readFile(await download.path(), 'utf8'));
    assert.equal(exported.session.cwd, '/projects/example');
    assert.equal(exported.messages[0].content, 'Saved Work result');
    await page.getByRole('button', { name: 'Checkpoints', exact: true }).click();
    const checkpoint = page.getByRole('dialog', { name: 'Work checkpoints' });
    await checkpoint.getByRole('combobox', { name: 'Checkpoint' }).selectOption('abcdef01');
    await checkpoint.getByText('+ change', { exact: true }).waitFor();
    await checkpoint.getByRole('button', { name: 'Restore checkpoint', exact: true }).click();
    assert.equal(calls.filter(call => call.method === 'rollback.restore').length, 0);
    await checkpoint.getByRole('button', { name: 'Restore files and history', exact: true }).click();
    await page.getByText('Checkpoint restored', { exact: true }).waitFor();
    assert.equal(calls.find(call => call.method === 'rollback.restore')?.params.hash, 'abcdef01');
    const newWork = page.getByRole('button', { name: 'New Work', exact: true }).first();
    const chooseExport = page.getByLabel('Session export', { exact: true });
    const importButton = page.getByRole('button', { name: 'Import session', exact: true });
    const fillExport = (value, name = 'session.json') => chooseExport.setInputFiles({
      name, mimeType: 'application/json', buffer: Buffer.from(JSON.stringify(value)),
    });
    await newWork.click();
    await fillExport({ invalid: true });
    await page.getByRole('alert').filter({ hasText: 'Choose an OpenCode session export' }).waitFor();
    const fixture = {
      info: { id: 'ses_fixture', title: 'OpenCode v2 fixture' },
      messages: [
        { type: 'system', text: 'Private source instructions' },
        { type: 'user', text: 'Imported question', time: { created: 1790000000000 } },
        { type: 'assistant', content: [{ type: 'reasoning', text: 'Private reasoning' }, { type: 'text', text: 'Imported reply' }] },
      ],
    };
    const promptCount = calls.filter(call => call.method === 'prompt.submit').length;
    await workspace.fill('/projects/example/');
    await fillExport(fixture);
    await page.getByText('OpenCode v2 fixture · OpenCode · 2 messages', { exact: true }).waitFor();
    assert.equal(imports.length, 0);
    await importButton.click();
    await page.getByText('Imported reply', { exact: true }).waitFor();
    assert.equal(imports[0].profile, 'writer');
    const saved = imports[0].sessions[0];
    assert.equal(saved.cwd, '/projects/example');
    assert.deepEqual(saved.messages, [
      { role: 'user', content: 'Imported question', timestamp: 1790000000 },
      { role: 'assistant', content: 'Imported reply', timestamp: 1790000000 },
    ]);
    await newWork.click();
    await workspace.fill('/projects/example');
    await fillExport(fixture);
    await importButton.click();
    await page.getByText('Imported reply', { exact: true }).waitFor();
    assert.equal(imports[1].sessions[0].id, saved.id);
    await expect(page.locator('.gb-agent-row').filter({ hasText: 'OpenCode v2 fixture' })).toHaveCount(1);
    await newWork.click();
    await workspace.fill('/projects/example');
    await fillExport({
      info: { id: 'ses_legacy', title: 'OpenCode v1 fixture' },
      messages: [{ info: { role: 'assistant' }, parts: [{ type: 'text', text: 'Legacy reply' }] }],
    });
    await importButton.click();
    await page.getByText('Legacy reply', { exact: true }).waitFor();
    await newWork.click();
    await workspace.fill('/projects/example');
    await fillExport(exported);
    await importButton.click();
    await page.getByText('Saved Work result', { exact: true }).waitFor();
    assert.equal(imports[3].sessions[0].messages[0].content, 'Saved Work result');
    assert.equal(calls.filter(call => call.method === 'prompt.submit').length, promptCount);
    await composer.fill('/oc ');
    await composer.press('Enter');
    await expect(page.locator('.gb-transcript')).toContainText('/oc/check');
    await expect(page.locator('.gb-transcript')).toContainText('/oc/team/review');
    assert.equal(calls.filter(call => call.method === 'prompt.submit').length, promptCount);
    await composer.fill('/oc/check "src/file one.ts" stable branch');
    await composer.press('Enter');
    await expect.poll(() => calls.filter(call => call.method === 'prompt.submit').at(-1)?.params.text)
      .toBe('Check src/file one.ts against stable branch. Raw arguments: "src/file one.ts" stable branch');
    await page.getByRole('button', { name: 'Stop generating', exact: true }).click();
    await composer.fill('/oc/team/review this workspace');
    await composer.press('Enter');
    await expect.poll(() => calls.filter(call => call.method === 'prompt.submit').at(-1)?.params.text)
      .toContain('Review the summary for this workspace.\n\n!`git diff --stat`');
    assert.match(calls.filter(call => call.method === 'prompt.submit').at(-1).params.text, /use your terminal tool/);
    await page.getByRole('button', { name: 'Stop generating', exact: true }).click();
    const beforeExplore = calls.filter(call => call.method === 'prompt.submit').length;
    await page.getByRole('button', { name: 'Explore', exact: true }).click();
    await expect(composer).toHaveValue('/explore ');
    await composer.fill('/explore What is in @file:`file name.txt`');
    await composer.press('Enter');
    await expect(page.locator('.gb-transcript')).toContainText('Explore answer from the fixture.');
    const [explore] = calls.filter(call => call.method === 'llm.oneshot');
    assert.ok(explore.params.session_id.startsWith('live-work-import-'));
    assert.equal(explore.params.task, 'work_explore');
    assert.deepEqual(JSON.parse(explore.params.input).context.files, [
      { path: 'file name.txt', excerpt: 'Explore fixture.\n', truncated: false },
    ]);
    await composer.fill('/explore Read @file:outside.txt');
    await composer.press('Enter');
    await page.getByText('Explore could not access outside.txt as a workspace file.', { exact: true }).waitFor();
    assert.ok(!fileReads.some(path => path.endsWith('/outside.txt')));
    await composer.fill('/explore Read @file:../outside.txt');
    await composer.press('Enter');
    await page.getByText('Explore requires file paths relative to this workspace.', { exact: true }).waitFor();
    assert.equal(calls.filter(call => call.method === 'llm.oneshot').length, 1);
    exploreUnavailable = true;
    await composer.fill('/explore What is in this workspace?');
    await composer.press('Enter');
    await page.getByText('Unknown method llm.oneshot', { exact: true }).waitFor();
    assert.deepEqual(JSON.parse(calls.filter(call => call.method === 'llm.oneshot').at(-1).params.input).context, {
      topLevelEntries: ['file name.txt'],
    });
    assert.equal(calls.filter(call => call.method === 'prompt.submit').length, beforeExplore);
    await page.getByRole('button', { name: 'Chat', exact: true }).click();
    await expect(page.locator('.gb-agent-row').filter({ hasText: 'OpenCode v2 fixture' })).toHaveCount(0);
    projectWorkTip = true;
    await page.reload();
    await writer.waitFor();
    if (await writer.getAttribute('aria-pressed') !== 'true') await writer.click();
    await expect(writer).toHaveAttribute('aria-pressed', 'true');
    await page.getByRole('button', { name: 'Work', exact: true }).click();
    await page.locator('.gb-agent-row').filter({ hasText: 'OpenCode v2 fixture' }).click();
    await page.getByText('Imported reply', { exact: true }).waitFor();
    await page.locator('.gb-agent-row').filter({ hasText: 'New Work' }).first().click();
    await page.getByText('Saved Work result', { exact: true }).waitFor();
    const defaultProfile = page.locator('.gb-pinned-tile').filter({ hasText: 'Hermes' });
    await defaultProfile.click();
    await expect(defaultProfile).toHaveAttribute('aria-pressed', 'true');
    await page.getByRole('button', { name: 'Work', exact: true }).click();
    await expect(page.locator('.gb-agent-row')).toHaveCount(0);
    assert.deepEqual(errors, []);
    console.log('PASS Work workspace validation, profile isolation, session import, OpenCode commands, read-only Explore, remote Git, goals, and checkpoints');
  } finally {
    await browser.close();
  }
}

checkWork().catch(error => { console.error(error); process.exitCode = 1; });
