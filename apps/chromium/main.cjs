const { app, BrowserWindow, dialog, ipcMain, protocol, shell } = require('electron');
const { readFile } = require('node:fs/promises');
const { extname, join, resolve } = require('node:path');
const { NativeClient } = require('./native-client.cjs');

const scheme = 'hermes-desktop';
const root = `${scheme}://app`;
const benchmark = process.env.HERMES_DESKTOP_PERF_DIST;
const packaged = __dirname === join(process.resourcesPath, 'app');
const dist = benchmark ? resolve(benchmark) : packaged
  ? join(process.resourcesPath, 'renderer') : resolve(__dirname, '../web/dist');
const pagePath = benchmark ? '/qa/performance.html' : '/index.html';
const pageUrl = root + pagePath;
const nativeBinary = packaged ? join(process.resourcesPath, 'hermes-native') :
  resolve(__dirname, '../desktop/src-tauri/target/release/hermes-native');
const icon = packaged ? join(process.resourcesPath, 'icon.png') :
  resolve(__dirname, '../../packages/grok-bot-theme/assets/icons/grok-bot-256.png');
const assets = new Map([
  ['.html', 'text/html; charset=utf-8'], ['.js', 'text/javascript; charset=utf-8'],
  ['.css', 'text/css; charset=utf-8'], ['.woff2', 'font/woff2'],
  ['.png', 'image/png'], ['.svg', 'image/svg+xml'], ['.ico', 'image/x-icon'],
]);
const commands = new Set([
  'start_local_backend', 'stop_local_backend', 'get_backend_info', 'restart_hermes_backend',
  'load_connection', 'save_connection', 'begin_remote_login', 'poll_remote_login', 'cancel_remote_login',
  'refresh_remote_login', 'hermes_http_request', 'load_local_bootstrap', 'load_local_session_messages',
  'load_board_state', 'save_board_state', 'save_local_memory_file', 'list_workspace_files',
  'read_workspace_file', 'git_workspace_status', 'get_linux_system_stats', 'terminal_start',
  'terminal_write', 'terminal_acknowledge', 'terminal_resize', 'terminal_info', 'terminal_close',
]);
const csp = [
  "default-src 'none'", "script-src 'self'", "worker-src 'self'",
  "style-src 'self' 'unsafe-inline'", "font-src 'self'", "img-src 'self' data: https: http:",
  "connect-src 'self' ws: wss:", "object-src 'none'", "frame-src 'none'",
  "frame-ancestors 'none'", "base-uri 'none'", "form-action 'none'",
].join('; ');

protocol.registerSchemesAsPrivileged([{
  scheme, privileges: { standard: true, secure: true, supportFetchAPI: true, corsEnabled: true, codeCache: true },
}]);
app.enableSandbox();
app.setName('Hermes Grok Bot');
app.setDesktopName('hermes-grok-bot.desktop');
app.setPath('userData', join(app.getPath('appData'), 'hermes-grok-bot-chromium'));

if (process.env.HERMES_CHROMIUM_USER_DATA) app.setPath('userData', resolve(process.env.HERMES_CHROMIUM_USER_DATA));
if (process.env.WAYLAND_DISPLAY) app.commandLine.appendSwitch('ozone-platform', 'wayland');

let native;
let win;
let shuttingDown = false;

function allowedPage(url) {
  try {
    const destination = new URL(url);
    return destination.protocol === `${scheme}:` && destination.host === 'app' &&
      destination.pathname === pagePath && !destination.search && !destination.username && !destination.password;
  } catch { return false; }
}

function externalUrl(address) {
  try {
    const url = new URL(address);
    return ['https:', 'http:'].includes(url.protocol) && !url.username && !url.password ? url.href : null;
  } catch { return null; }
}

async function assetResponse(request) {
  const url = new URL(request.url);
  if (request.method !== 'GET' || url.host !== 'app' || url.username || url.password ||
      !(url.pathname === pagePath || /^\/assets\/[a-zA-Z0-9_.-]+$/.test(url.pathname))) {
    return new Response(null, { status: 404 });
  }
  const mime = assets.get(extname(url.pathname));
  if (!mime) return new Response(null, { status: 404 });
  try {
    const body = await readFile(join(dist, url.pathname.slice(1)));
    return new Response(body, { headers: {
      'Content-Type': mime,
      'Content-Security-Policy': csp,
      'X-Content-Type-Options': 'nosniff',
      'Referrer-Policy': 'no-referrer',
    } });
  } catch { return new Response(null, { status: 404 }); }
}

async function createWindow() {
  protocol.handle(scheme, assetResponse);
  win = new BrowserWindow({
    width: 1360, height: 860, minWidth: 820, minHeight: 560,
    icon, frame: false, backgroundColor: '#171717', show: Boolean(benchmark), title: 'Hermes',
    webPreferences: {
      ...(benchmark ? {} : { preload: join(__dirname, 'preload.cjs') }),
      contextIsolation: true, sandbox: true, nodeIntegration: false,
      webSecurity: true, webviewTag: false, navigateOnDragDrop: false, devTools: Boolean(benchmark),
    },
  });
  const contents = win.webContents;
  const permissionAllowed = (requester, permission, details) =>
    requester === contents && details.isMainFrame && allowedPage(details.requestingUrl) &&
      (permission === 'clipboard-sanitized-write' || permission === 'loopback-network');
  contents.session.setPermissionRequestHandler((requester, permission, callback, details) =>
    callback(permissionAllowed(requester, permission, details)));
  contents.session.setPermissionCheckHandler((requester, permission, _origin, details) =>
    permissionAllowed(requester, permission, details));
  // Paste remains tied to a real keyboard event, with no clipboard-read IPC.
  contents.on('before-input-event', (event, input) => {
    if ((input.control || input.meta) && input.shift && input.type === 'keyDown' &&
        input.key.toLowerCase() === 'v' && allowedPage(contents.getURL())) {
      event.preventDefault();
      contents.paste();
    }
  });
  contents.on('will-navigate', event => event.preventDefault());
  contents.on('will-frame-navigate', event => event.preventDefault());
  contents.on('will-redirect', event => event.preventDefault());
  contents.on('will-attach-webview', event => event.preventDefault());
  contents.setWindowOpenHandler(({ url, disposition }) => {
    const address = externalUrl(url);
    if (!benchmark && address && ['foreground-tab', 'background-tab', 'new-window'].includes(disposition)) {
      void shell.openExternal(address).catch(() => {});
    }
    return { action: 'deny' };
  });
  contents.on('render-process-gone', () => native?.detachRenderer());
  win.once('ready-to-show', () => win.show());

  if (benchmark) {
    win.on('page-title-updated', (_event, title) => {
      if (!/^(RESULT|ERROR) /.test(title)) return;
      console.log(title);
      app.exit(title.startsWith('RESULT ') ? 0 : 1);
    });
    setTimeout(() => { console.error('Chromium performance fixture timed out'); app.exit(1); }, 25_000).unref();
  } else {
    native = new NativeClient(nativeBinary);
    native.on('terminal', (id, event) => {
      if (!contents.isDestroyed() && allowedPage(contents.getURL())) contents.send('hermes:terminal-event', id, event);
    });
    ipcMain.handle('hermes:command', (event, command, args) => {
      if (event.sender !== contents || event.senderFrame !== contents.mainFrame || !allowedPage(event.senderFrame?.url)) {
        throw new Error('Native requests require the app’s main frame');
      }
      if (!args || typeof args !== 'object' || Array.isArray(args)) throw new Error('Invalid native request');
      switch (command) {
        case 'window_minimize': return win.minimize();
        case 'window_toggle_maximize': return win.isMaximized() ? win.unmaximize() : win.maximize();
        case 'window_close': return win.close();
        case 'window_start_drag': return undefined; // Chromium uses CSS drag regions.
        default:
          if (!commands.has(command)) throw new Error('Unknown native command');
          return native.invoke(command, args).catch(error => {
            if (shuttingDown || contents.isDestroyed()) return null;
            throw error;
          });
      }
    });
  }
  await win.loadURL(pageUrl);
  return win;
}

if (benchmark || app.requestSingleInstanceLock()) {
  app.on('second-instance', () => { if (win?.isMinimized()) win.restore(); win?.focus(); });
  module.exports.ready = app.whenReady().then(createWindow);
  module.exports.ready.catch(error => {
    console.error(error.message);
    if (!benchmark) dialog.showErrorBox('Hermes could not start', error.message);
    app.quit();
  });
  app.on('window-all-closed', () => { if (!shuttingDown) app.quit(); });
  app.on('before-quit', event => {
    if (!native || shuttingDown) return;
    event.preventDefault();
    shuttingDown = true;
    ipcMain.removeHandler('hermes:command');
    if (win && !win.isDestroyed()) win.destroy();
    native.stop().finally(() => app.quit());
  });
} else app.quit();
