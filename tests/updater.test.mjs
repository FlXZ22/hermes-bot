import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, writeFile, rm, copyFile, readFile, readlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
const { NativeClient } = createRequire(import.meta.url)('../apps/chromium/native-client.cjs');
const binary = new URL('../apps/desktop/src-tauri/target/release/hermes-native', import.meta.url).pathname;

test('updater prepares Electron first, isolates build input, and installs successfully after failures', { timeout: 20000 }, async () => {
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
    await mkdir(join(dir, 'node_modules/electron'), { recursive: true });
    await writeFile(join(dir, 'node_modules/electron/package.json'), '{}');
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
    await writeFile(join(dir, 'scripts/prepare-electron.cjs'), `
      if (require('node:fs').existsSync('fail-electron')) process.exit(24);
      console.log('electron-prepared');
    `);
    await writeFile(join(dir, 'scripts/install-chromium.sh'), 'set -eu\nprintf installed > installed-marker\n');
    const wrapper = join(dir, 'native');
    await writeFile(wrapper, `#!/bin/sh\nexport XDG_CONFIG_HOME='${config}'\nexec '${join(installed, 'hermes-native')}'\n`, { mode: 0o755 });
    client = new NativeClient(wrapper);
    const settled = async () => {
      const deadline = Date.now() + 5000;
      do {
        const result = await client.invoke('get_update_status', {});
        if (result.phase !== 'updating') return result;
        await new Promise(resolve => setTimeout(resolve, 50));
      } while (Date.now() < deadline);
      assert.fail('Update did not settle');
    };

    await writeFile(join(dir, 'fail-electron'), '');
    await client.invoke('apply_app_update', { mode: 'local' });
    let result = await settled();
    assert.equal(result.phase, 'error');
    assert.match(result.error, /prepare-electron\.cjs.*24/);
    assert.ok(!result.logs.some(line => line.includes('Building UI')));
    await rm(join(dir, 'fail-electron'));

    assert.equal((await client.invoke('apply_app_update', { mode: 'local' })).phase, 'updating');
    // A check during an update must return its status, without locking itself
    // again or starting a network request. This used to deadlock the service.
    const status = await client.invoke('check_for_updates', {});
    assert.equal(status.phase, 'updating');
    result = await settled();
    assert.equal(result.phase, 'error');
    assert.match(result.error, /23/);
    assert.ok(result.logs.some(line => line.includes('electron-prepared')));
    assert.ok(result.logs.some(line => line.includes('stdin-isolated')));
    assert.ok(result.logs.some(line => line.includes('fixture-build-failure')));
    assert.equal(typeof (await client.invoke('get_backend_info', {})).status, 'string');
    await writeFile(join(dir, 'build.cjs'), `const fs=require('node:fs');if(fs.readFileSync(0,'utf8')!=='')throw Error('IPC leaked');console.log('build-success');`);
    await client.invoke('apply_app_update', { mode: 'local' });
    result = await settled();
    assert.equal(result.phase, 'restart_required', JSON.stringify(result));
    assert.equal(await readFile(join(dir, 'installed-marker'), 'utf8'), 'installed');

    // A checkout from before the preflight existed is still installable.
    await rm(join(dir, 'scripts/prepare-electron.cjs'));
    await client.invoke('apply_app_update', { mode: 'local' });
    result = await settled();
    assert.equal(result.phase, 'restart_required', JSON.stringify(result));
    assert.ok(!result.logs.some(line => line.includes('Preparing Chromium runtime')));
  } finally {
    await client?.stop();
    await rm(dir, { recursive: true, force: true });
  }
});

test('Chromium installer provisions a missing or partial Electron bundle and keeps the active release if it fails', async () => {
  const dir = await mkdtemp(join(tmpdir(), 'hermes-chromium-install-'));
  try {
    const home = join(dir, 'home');
    const data = join(dir, 'data');
    const tools = join(dir, 'tools');
    const electron = join(dir, 'node_modules/electron');
    const files = {
      'package.json': ['{"version":"1.0.0"}'],
      'node_modules/electron/package.json': ['{"version":"45.4.1"}'],
      'node_modules/electron/install.js': [`
        const assert = require('node:assert/strict');
        const { existsSync, mkdirSync, writeFileSync, appendFileSync } = require('node:fs');
        const { join } = require('node:path');
        assert.equal(process.env.ELECTRON_INSTALL_PLATFORM, 'linux');
        assert.equal(process.env.ELECTRON_INSTALL_ARCH, process.arch);
        assert.equal(process.env.ELECTRON_OVERRIDE_DIST_PATH, undefined);
        assert.ok(!existsSync(join(__dirname, 'path.txt')), 'stale installer marker was not removed');
        if (process.env.FIXTURE_DOWNLOAD_FAIL) process.exit(17);
        appendFileSync(join(__dirname, 'install-count'), 'installed\\n');
        mkdirSync(join(__dirname, 'dist/resources'), { recursive: true });
        for (const name of ['icudtl.dat', 'resources.pak', 'resources/default_app.asar']) {
          writeFileSync(join(__dirname, 'dist', name), 'fixture');
        }
        writeFileSync(join(__dirname, 'dist/electron'), '#!/bin/sh\\nprintf fixture\\n', { mode: 0o755 });
        writeFileSync(join(__dirname, 'dist/version'), '45.4.1\\n');
        writeFileSync(join(__dirname, 'path.txt'), 'electron');
      `],
      'apps/desktop/src-tauri/target/release/hermes-native': ['fixture', 0o755],
      'apps/web/dist/index.html': ['<!doctype html>'],
      'apps/chromium/package.json': ['{}'],
      'apps/chromium/main.cjs': [''],
      'apps/chromium/preload.cjs': [''],
      'apps/chromium/native-client.cjs': [''],
      'packages/grok-bot-theme/assets/icons/grok-bot-256.png': ['fixture icon'],
      'tools/update-desktop-database': ['#!/bin/sh\nexit 12\n', 0o755],
    };
    for (const [path, [text, mode]] of Object.entries(files)) {
      await mkdir(dirname(join(dir, path)), { recursive: true });
      await writeFile(join(dir, path), text, { mode });
    }
    await mkdir(join(dir, 'scripts'));
    for (const script of ['prepare-electron.cjs', 'install-chromium.sh']) {
      await copyFile(new URL(`../scripts/${script}`, import.meta.url), join(dir, 'scripts', script));
    }

    const current = join(data, 'hermes-grok-bot/chromium/current');
    const install = (env = {}) => spawnSync('bash', ['scripts/install-chromium.sh'], {
      cwd: dir, encoding: 'utf8',
      env: { ...process.env, HOME: home, XDG_DATA_HOME: data, PATH: `${tools}:${process.env.PATH}`,
        ELECTRON_OVERRIDE_DIST_PATH: '/not/the/packaged/runtime', ...env },
    });
    // npm ci installed the module but no dist, as in the reported failure.
    let result = install();
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /Preparing Electron 45\.4\.1/);
    assert.match(result.stderr, /desktop menu cache refresh failed/);
    assert.equal(execFileSync(join(home, '.local/bin/hermes-grok-bot'), { encoding: 'utf8' }), 'fixture');
    result = install();
    assert.equal(result.status, 0, result.stderr);
    assert.doesNotMatch(result.stdout, /Preparing Electron/);
    assert.equal(await readFile(join(electron, 'install-count'), 'utf8'), 'installed\n');
    const previous = await readlink(current);

    // A partial extraction with a valid version and marker must be repaired.
    await rm(join(electron, 'dist/resources.pak'));
    result = install({ FIXTURE_DOWNLOAD_FAIL: '1' });
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Chromium runtime setup failed: Electron 45\.4\.1 download\/extraction failed \(exit 17\)/);
    assert.equal(await readlink(current), previous);
    assert.equal(execFileSync(join(home, '.local/bin/hermes-grok-bot'), { encoding: 'utf8' }), 'fixture');

    // Retrying can repair the checkout and atomically activate the new release.
    result = install();
    assert.equal(result.status, 0, result.stderr);
    assert.notEqual(await readlink(current), previous);
    assert.equal(await readFile(join(electron, 'install-count'), 'utf8'), 'installed\ninstalled\n');
  } finally {
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
