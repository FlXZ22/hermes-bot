import fs from 'node:fs';
import path from 'node:path';

const asarPath = '/home/metis/Projects/grok-bot-ui/analysis/grok-bot-deb/opt/Grok Bot/resources/app.asar';
const outDir = '/home/metis/Projects/grok-bot-ui/analysis/grok-bot-asar';

const fd = fs.openSync(asarPath, 'r');
const hdr = Buffer.alloc(16);
fs.readSync(fd, hdr, 0, 16, 0);
// ASAR uses Chromium Pickle:
// 0..4: payload size of header size pickle (4)
// 4..8: header pickle size
// 8..12: payload size of header string pickle
// 12..16: header string length
const headerPickleSize = hdr.readUInt32LE(4);
const headerStrLen = hdr.readUInt32LE(12);
const headerBuf = Buffer.alloc(headerStrLen);
fs.readSync(fd, headerBuf, 0, headerStrLen, 16);
const header = JSON.parse(headerBuf.toString('utf8'));
const baseOffset = 8 + headerPickleSize;

const entries = [];
function walk(node, relPath = '') {
  if (node.files) {
    for (const [name, child] of Object.entries(node.files)) {
      walk(child, relPath ? `${relPath}/${name}` : name);
    }
  } else {
    entries.push({ relPath, size: node.size, offset: node.offset, unpacked: Boolean(node.unpacked) });
  }
}
walk(header);

console.log(`Total files in app.asar: ${entries.length}`);
const topCounts = {};
for (const e of entries) {
  const top = e.relPath.split('/').slice(0, 2).join('/');
  topCounts[top] = (topCounts[top] || 0) + 1;
}
console.log('Top paths:', topCounts);

// Extract package.json, dist/renderer or similar UI/CSS/fonts/assets/main files (skip huge node_modules unless small)
fs.mkdirSync(outDir, { recursive: true });
let extracted = 0;
for (const e of entries) {
  if (e.unpacked || e.offset === undefined) continue;
  const isNodeMod = e.relPath.includes('node_modules/');
  if (isNodeMod) continue;
  const dest = path.join(outDir, e.relPath);
  fs.mkdirSync(path.dirname(dest), { recursive: true });
  const buf = Buffer.alloc(e.size);
  fs.readSync(fd, buf, 0, e.size, baseOffset + Number(e.offset));
  fs.writeFileSync(dest, buf);
  extracted++;
}
fs.closeSync(fd);
console.log(`Extracted ${extracted} non-node_modules files to ${outDir}`);
