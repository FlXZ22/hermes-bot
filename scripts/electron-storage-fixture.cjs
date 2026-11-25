const { app, BrowserWindow, protocol } = require('electron');

app.enableSandbox();
app.setPath('userData', process.env.HERMES_CHROMIUM_USER_DATA);
protocol.registerSchemesAsPrivileged([{
  scheme: 'hermes-desktop', privileges: { standard: true, secure: true },
}]);

app.whenReady().then(async () => {
  protocol.handle('hermes-desktop', () => new Response('<!doctype html><title>Storage fixture</title>', {
    headers: { 'content-type': 'text/html' },
  }));
  const window = new BrowserWindow({ show: false });
  await window.loadURL('hermes-desktop://app/index.html');
  const draft = JSON.stringify({ fixture: 'legacy café 🔧', workspace: process.env.HERMES_CHROMIUM_WORK_FIXTURE });
  await window.webContents.executeJavaScript(`
    localStorage.setItem("hermes-grok-bot:work-drafts", ${JSON.stringify(draft)});
    localStorage.setItem("hermes-grok-bot:connection", "fixture-native-credentials-are-not-migrated");
  `);
  window.webContents.session.flushStorageData();
  app.quit();
}).catch(error => { console.error(error); app.exit(1); });
