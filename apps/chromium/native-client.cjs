const { spawn } = require('node:child_process');
const { EventEmitter } = require('node:events');

const maxFrame = 48 * 1024 * 1024;
const maxPending = 32;

class NativeClient extends EventEmitter {
  #child;
  #pending = new Map();
  #terminals = new Map();
  #nextId = 0;
  #stopping = false;
  #failure;
  #pieces = [];
  #size = 0;

  constructor(binary) {
    super();
    this.#child = spawn(binary, [], { stdio: ['pipe', 'pipe', 'inherit'], windowsHide: true });
    this.#child.stdout.on('data', bytes => this.#read(bytes));
    this.#child.stdin.on('error', error => this.#fail(error));
    this.#child.on('error', error => this.#fail(error));
    this.#child.on('close', (code, signal) => {
      const reason = signal ? `signal ${signal}` : `exit code ${code}`;
      this.#fail(new Error(`Hermes native service exited (${reason}). Restart the app before retrying. If this happened during a build, check the terminal/system logs for the cause.`));
    });
  }

  #fail(error) {
    this.#failure ??= error;
    for (const job of this.#pending.values()) {
      clearTimeout(job.timer);
      job.reject(error);
    }
    this.#pending.clear();
    this.#terminals.clear();
  }

  #read(bytes) {
    // Parse complete frames only. A response can contain up to 32 MB of JSON
    // before envelope encoding, so avoid copying the prefix on every chunk.
    for (let begin = 0; begin < bytes.length;) {
      const end = bytes.indexOf(10, begin);
      const piece = bytes.subarray(begin, end < 0 ? bytes.length : end);
      this.#size += piece.length;
      if (this.#size > maxFrame) {
        this.#fail(new Error('Oversized native response'));
        this.#child.kill();
        return;
      }
      this.#pieces.push(piece);
      if (end < 0) break;
      try {
        const message = JSON.parse(Buffer.concat(this.#pieces, this.#size).toString('utf8'));
        this.#pieces = [];
        this.#size = 0;
        this.#message(message);
      } catch {
        this.#fail(new Error('Invalid native response'));
        this.#child.kill();
        return;
      }
      begin = end + 1;
    }
  }

  #message(message) {
    if (message.event) {
      if (this.#terminals.get(message.terminalId) !== message.subscription) return;
      this.emit('terminal', message.terminalId, message.event);
      if (message.event.type === 'exit' || message.event.type === 'error') {
        this.#terminals.delete(message.terminalId);
      }
      return;
    }
    const operation = this.#pending.get(message.id);
    if (!operation) return;
    this.#pending.delete(message.id);
    clearTimeout(operation.timer);
    if (typeof message.error === 'string') operation.reject(new Error(message.error));
    else operation.resolve(message.result);
  }

  invoke(command, args) {
    if (this.#stopping || this.#failure) return Promise.reject(this.#failure ?? new Error('Hermes native service is closing'));
    const pendingLimit = command === 'terminal_acknowledge' ? maxPending + 16 : maxPending;
    if (this.#pending.size >= pendingLimit) {
      return Promise.reject(new Error('Hermes native service is busy'));
    }
    const id = ++this.#nextId;
    const frame = JSON.stringify({ id, command, args }) + '\n';
    const frameSize = Buffer.byteLength(frame);
    if (frameSize > maxFrame) return Promise.reject(new Error('Native request is too large'));
    if (this.#child.stdin.writableLength + frameSize > maxFrame) return Promise.reject(new Error('Native service is busy'));
    const terminal = command === 'terminal_start' ? args.id : undefined;
    if (terminal) this.#terminals.set(terminal, id);
    if (command === 'terminal_close') this.#terminals.delete(args.id);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error('Hermes native request timed out'));
      }, 45_000);
      this.#pending.set(id, { resolve, reject, timer });
      this.#child.stdin.write(frame);
    }).catch(error => {
      if (terminal && this.#terminals.get(terminal) === id) this.#terminals.delete(terminal);
      throw error;
    });
  }

  detachRenderer() {
    this.#terminals.clear();
  }

  async stop() {
    if (this.#stopping || this.#child.exitCode !== null || this.#child.signalCode !== null) return;
    this.#stopping = true;
    await new Promise(resolve => {
      const timeout = setTimeout(() => this.#child.kill(), 3500);
      this.#child.once('close', () => { clearTimeout(timeout); resolve(); });
      this.#child.stdin.end();
    });
  }
}

module.exports = { NativeClient };
