const { contextBridge, ipcRenderer } = require('electron');

// No Node APIs or raw IPC events cross into the page.
const terminalListeners = new Map();
ipcRenderer.on('hermes:terminal-event', (_event, id, data) => {
  const listener = terminalListeners.get(id);
  if (data.type === 'exit' || data.type === 'error') terminalListeners.delete(id);
  listener?.(data);
});

contextBridge.exposeInMainWorld('hermesDesktop', {
  invoke(command, args = {}) {
    if (command === 'terminal_start') return Promise.reject(new Error('Use terminalStart to create an output channel'));
    if (command === 'terminal_close') terminalListeners.delete(args.id);
    return ipcRenderer.invoke('hermes:command', command, args);
  },
  terminalStart(args, onEvent) {
    if (typeof onEvent !== 'function') return Promise.reject(new Error('A terminal output listener is required'));
    terminalListeners.set(args.id, onEvent);
    return ipcRenderer.invoke('hermes:command', 'terminal_start', args).catch(error => {
      if (terminalListeners.get(args.id) === onEvent) terminalListeners.delete(args.id);
      throw error;
    });
  },
});
