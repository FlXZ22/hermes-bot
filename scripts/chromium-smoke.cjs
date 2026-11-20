const assert = require('node:assert/strict');
const { app } = require('electron');
const { writeFile } = require('node:fs/promises');
const { ready } = require('../apps/chromium/main.cjs');

async function run() {
  const win = await ready;
  const contents = win.webContents;
  const js = (body, userGesture = false) => contents.executeJavaScript(body, userGesture);
  const wait = async (body, message) => {
    for (let tries = 0; tries < 300; tries++) {
      if (await js(body)) return;
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    throw new Error(message);
  };
  const click = label => js(`Array.from(document.querySelectorAll('button')).find(button =>
    button.textContent.includes(${JSON.stringify(label)}) && button.getClientRects().length && !button.disabled)?.click()`, true);
  assert.deepEqual(await js(`({
    bridge: typeof hermesDesktop.invoke, node: typeof require, process: typeof process,
    secureContext: isSecureContext,
  })`), { bridge: 'function', node: 'undefined', process: 'undefined', secureContext: true });
  assert.equal(contents.getLastWebPreferences().sandbox, true);
  assert.equal(contents.getLastWebPreferences().contextIsolation, true);
  assert.equal(contents.getLastWebPreferences().nodeIntegration, false);
  assert.equal(await js(`hermesDesktop.invoke('this_command_does_not_exist').then(() => false, () => true)`), true);
  await wait(`[...document.querySelectorAll('button')].some(b => b.textContent.includes('Get started'))`, 'Welcome did not load');
  await click('Get started');
  await click('This computer');
  await wait(`[...document.querySelectorAll('button')].some(b => b.textContent.includes('Open Hermes')) ||
    document.querySelector('.gb-welcome__error')?.textContent`, 'Local gateway did not connect');
  const gatewayError = await js(`document.querySelector('.gb-welcome__error')?.textContent`);
  assert.ok(!gatewayError, gatewayError);
  await click('Open Hermes');
  await wait(`Boolean(document.querySelector('.gb-composer__textarea'))`, 'Composer did not appear');
  await js(`(() => {
    const input = document.querySelector('.gb-composer__textarea');
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(input, 'Say hello.');
    input.dispatchEvent(new Event('input', { bubbles: true }));
  })()`, true);
  await js(`document.querySelector('button[title^="Send message"]')?.click()`, true);
  await wait(`Boolean(document.querySelector('.gb-transcript')?.textContent.includes('Chromium gateway fixture'))`, 'The local model reply did not reach the composer');
  const selected = await js(`hermesDesktop.invoke('load_connection')`);
  assert.equal(selected.id, 'local-rust');
  assert.equal(selected.token, undefined);
  assert.equal(await js(`window.open('about:blank') === null`), true);
  await wait(`Boolean(document.querySelector('.gb-transcript strong')?.textContent.includes('Chromium gateway fixture'))`, 'Markdown did not render');
  await wait(`(() => {
    const answer = document.querySelector('.gb-transcript strong')?.getBoundingClientRect();
    const viewport = document.querySelector('.gb-transcript')?.getBoundingClientRect();
    return Boolean(answer && viewport && answer.height && answer.top >= viewport.top && answer.bottom <= viewport.bottom);
  })()`, 'The reply exists but is outside the visible transcript');
  if (process.env.HERMES_CHROMIUM_SMOKE_SCREENSHOT) {
    await js('new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))');
    const page = await contents.capturePage();
    await writeFile(process.env.HERMES_CHROMIUM_SMOKE_SCREENSHOT, page.toPNG());
  }
  console.log('PASS sandboxed renderer, blocked child window, native credentials and authenticated Markdown streaming');

  const result = await js(`(async () => {
    const id = 'chromium-smoke';
    let output = '';
    let generation = '';
    const info = await hermesDesktop.terminalStart({ id, cwd: '/tmp', rows: 24, cols: 80 }, event => {
      if (event.type === 'started') generation = event.generation;
      if (event.type === 'output') {
        const bytes = atob(event.data);
        output += bytes;
        if (generation) void hermesDesktop.invoke('terminal_acknowledge', { id, generation, bytes: bytes.length });
      }
    });
    try {
      await hermesDesktop.invoke('terminal_write', { id, data: "printf '%s%s\\\\n' '__CHROMIUM_' 'PTY__'\\n" });
      for (let index = 0; index < 80 && !output.includes('__CHROMIUM_PTY__'); index++) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      return { local: !info.remote, output: output.includes('__CHROMIUM_PTY__') };
    } finally { await hermesDesktop.invoke('terminal_close', { id }); }
  })()`);
  assert.deepEqual(result, { local: true, output: true });
  console.log('PASS terminal output, input, acknowledgements, and close through the private bridge');
}

run().then(() => app.quit()).catch(error => {
  console.error(error);
  app.once('will-quit', () => app.exit(1));
  app.quit();
});
