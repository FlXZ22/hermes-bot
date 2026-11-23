import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, copyFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
const { NativeClient } = createRequire(import.meta.url)('../apps/chromium/native-client.cjs');
const binary = new URL('../apps/desktop/src-tauri/target/release/hermes-native', import.meta.url).pathname;

test('updater isolates build input, survives build failure, and installs successfully on retry', { timeout: 15000 }, async () => {
  const dir = await mkdtemp(join(tmpdir(), 'hermes-updater-'));
  let client;
  try {
    // No user checkout or installation is changed. This disposable project's
    // actual npm build reads stdin, then fails in a controlled way.
    const git = (...args) => execFileSync('git', args, { cwd: dir, stdio: 'pipe' }).toString().trim();
    git('init', '-b', 'main');
    await writeFile(join(dir, 'package.json'), JSON.stringify({ scripts: { build: 'node build.cjs' } }));
    await writeFile(join(dir, 'build.cjs'), `const fs=require('node:fs');if(fs.readFileSync(0,'utf8')!=='')throw Error('IPC leaked into build');console.log('stdin-isolated');setTimeout(()=>{console.error('fixture-build-failure');process.exit(23)},1200);`);
    await writeFile(join(dir, '.gitignore'), '**/target/\n');
    git('add', '.');
    git('-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.test', 'commit', '-m', 'Fixture');
    await mkdir(join(dir, 'node_modules'));
    const config = join(dir, 'config');
    await mkdir(join(config, 'hermes-grok-bot'), { recursive: true });
    await writeFile(join(config, 'hermes-grok-bot/updater.json'), JSON.stringify({ repoPath: dir, targetBranch: 'main', autoCheck: false, githubRepo: 'FlXZ22/hermes-bot' }));
    const installed = join(dir, 'resources');
    await mkdir(installed);
    await copyFile(binary, join(installed, 'hermes-native'));
    const revision = git('rev-parse', 'HEAD');
    await writeFile(join(installed, 'release-meta.json'), JSON.stringify({host:'chromium',commit:revision,nativeCommit:revision}));
    await mkdir(join(dir, 'apps/desktop/src-tauri/target/release'), { recursive: true });
    await writeFile(join(dir, 'apps/desktop/src-tauri/target/release/hermes-native'), 'fixture');
    await mkdir(join(dir, 'scripts'));
    await writeFile(join(dir, 'scripts/install-chromium.sh'), 'set -eu\nprintf installed > installed-marker\n');
    const wrapper = join(dir, 'native');
    await writeFile(wrapper, `#!/bin/sh\nexport XDG_CONFIG_HOME='${config}'\nexec '${join(installed, 'hermes-native')}'\n`, { mode: 0o755 });
    client = new NativeClient(wrapper);
    assert.equal((await client.invoke('apply_app_update', { mode: 'local' })).phase, 'updating');
    // A check during an update must return its status, without locking itself
    // again or starting a network request. This used to deadlock the service.
    const status = await client.invoke('check_for_updates', {});
    assert.equal(status.phase, 'updating');
    let result;
    const deadline = Date.now() + 10000;
    do {
      result = await client.invoke('get_update_status', {});
      if (result.phase !== 'updating') break;
      await new Promise(resolve => setTimeout(resolve, 50));
    } while (Date.now() < deadline);
    assert.equal(result.phase, 'error');
    assert.match(result.error, /23/);
    assert.ok(result.logs.some(line => line.includes('stdin-isolated')));
    assert.ok(result.logs.some(line => line.includes('fixture-build-failure')));
    assert.equal(typeof (await client.invoke('get_backend_info', {})).status, 'string');
    await writeFile(join(dir, 'build.cjs'), `const fs=require('node:fs');if(fs.readFileSync(0,'utf8')!=='')throw Error('IPC leaked');console.log('build-success');`);
    await client.invoke('apply_app_update', { mode: 'local' });
    do {
      result = await client.invoke('get_update_status', {});
      if (result.phase !== 'updating') break;
      await new Promise(resolve => setTimeout(resolve, 50));
    } while (Date.now() < deadline);
    assert.equal(result.phase, 'restart_required', JSON.stringify(result));
    assert.equal(await readFile(join(dir, 'installed-marker'), 'utf8'), 'installed');
  } finally {
    await client?.stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('native service exit reports the exit code instead of a generic closed error', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'hermes-native-exit-'));
  let client;
  try {
    const wrapper = join(dir, 'native');
    await writeFile(wrapper, '#!/bin/sh\nread request\nexit 23\n', { mode: 0o755 });
    client = new NativeClient(wrapper);
    await assert.rejects(client.invoke('get_update_status', {}), /exit code 23.*Restart the app/);
  } finally {
    await client?.stop();
    await rm(dir, { recursive: true, force: true });
  }
});
