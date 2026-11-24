const { spawnSync } = require('node:child_process');
const { accessSync, constants, readFileSync, rmSync, statSync } = require('node:fs');
const { join } = require('node:path');

const electron = join(__dirname, '..', 'node_modules', 'electron');
const dist = join(electron, 'dist');
const pathFile = join(electron, 'path.txt');
const installer = join(electron, 'install.js');
const assets = [
  ['electron', constants.X_OK],
  ['icudtl.dat', constants.R_OK],
  ['resources.pak', constants.R_OK],
  ['resources/default_app.asar', constants.R_OK],
];

function bundleReady(version) {
  try {
    if (readFileSync(pathFile, 'utf8') !== 'electron' ||
        readFileSync(join(dist, 'version'), 'utf8').trim().replace(/^v/, '') !== version) {
      return false;
    }
    return assets.every(([name, mode]) => {
      const asset = join(dist, name);
      accessSync(asset, mode);
      const stat = statSync(asset);
      return stat.isFile() && stat.size > 0;
    });
  } catch {
    return false;
  }
}

function prepare() {
  if (process.platform !== 'linux') throw new Error('The Chromium desktop release is built on Linux.');
  let version;
  try {
    version = JSON.parse(readFileSync(join(electron, 'package.json'), 'utf8')).version;
    accessSync(installer, constants.R_OK);
    if (typeof version !== 'string' || !version) throw new Error('Invalid Electron version');
  } catch {
    throw new Error('The local Electron npm package is missing or incomplete. Run `npm ci` and retry.');
  }
  if (bundleReady(version)) return;

  console.log(`Preparing Electron ${version} (linux-${process.arch}) for the Chromium release…`);
  // `npm ci` need not download Electron. Invoke the pinned package's own
  // downloader, with its checksum and cache handling, without enabling npm hooks.
  // A stale marker can otherwise make upstream skip a partial extraction.
  rmSync(pathFile, { force: true });
  const env = { ...process.env, ELECTRON_INSTALL_PLATFORM: 'linux', ELECTRON_INSTALL_ARCH: process.arch };
  delete env.ELECTRON_OVERRIDE_DIST_PATH; // Development override, not the bundle we package.
  const result = spawnSync(process.execPath, [installer], { env, stdio: ['ignore', 'inherit', 'inherit'] });
  if (result.error) throw new Error(`Could not start the Electron downloader: ${result.error.message}`);
  if (result.status !== 0) {
    throw new Error(`Electron ${version} download/extraction failed (${result.signal ?? `exit ${result.status}`}). Check the output above and retry.`);
  }
  if (!bundleReady(version)) {
    throw new Error(`Electron ${version} did not produce a complete Chromium bundle at ${dist}.`);
  }
}

try {
  prepare();
} catch (error) {
  console.error(`Chromium runtime setup failed: ${error.message}`);
  process.exitCode = 1;
}
