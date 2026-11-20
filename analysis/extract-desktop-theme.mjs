import fs from 'node:fs';
import path from 'node:path';

const assetsDir = '/home/metis/Projects/grok-bot-ui/analysis/grok-bot-asar/dist/renderer/assets';
const cssPath = path.join(assetsDir, 'index-CqCBWzOp.css');
const css = fs.readFileSync(cssPath, 'utf8');

console.log(`index-CqCBWzOp.css bytes: ${css.length}`);

// Extract @font-face blocks
const fontFaces = css.match(/@font-face\{[^}]+\}/g) || [];
console.log(`\n@font-face count: ${fontFaces.length}`);
for (const ff of fontFaces) console.log(ff);

// Extract :root / [data-theme] / .dark / theme variable blocks
const varBlocks = [];
const blockRe = /([^{}@]*?(?::root|data-theme|grok-bot|theme-dark|theme-light|\.dark)[^{}@]*)\{([^}]*--[^}]+)\}/g;
let m;
while ((m = blockRe.exec(css))) {
  const sel = m[1].trim();
  const body = m[2].trim();
  if (body.includes('--')) {
    varBlocks.push({ sel, body });
  }
}
console.log(`\nTheme variable blocks: ${varBlocks.length}`);
for (const b of varBlocks.slice(0, 30)) {
  console.log(`\n--- ${b.sel} ---\n${b.body.slice(0, 1200)}`);
}

// Find all icon glyph definitions across renderer JS files
const jsFiles = fs.readdirSync(assetsDir).filter((f) => f.endsWith('.js'));
const iconCandidates = [];
for (const f of jsFiles) {
  const txt = fs.readFileSync(path.join(assetsDir, f), 'utf8');
  if (txt.includes('CursorIcons16') || txt.includes('ea6d') || txt.includes('EA6D')) {
    iconCandidates.push({ f, len: txt.length });
  }
}
console.log('\nIcon candidate JS files:', iconCandidates);

// Find color palette / avatar shape definitions
const avatarCandidates = [];
for (const f of jsFiles) {
  const txt = fs.readFileSync(path.join(assetsDir, f), 'utf8');
  if (txt.includes('97683D') || txt.includes('885CF5') || txt.includes('885cf5') || txt.includes('agentColors') || txt.includes('M228.54 114.27')) {
    avatarCandidates.push({ f, len: txt.length });
  }
}
console.log('Avatar/color candidate JS files:', avatarCandidates);

// Find main window / sidebar / chat layout class names from index CSS
const classes = new Set();
for (const cm of css.matchAll(/\.([a-zA-Z_-][a-zA-Z0-9_-]{3,})/g)) {
  if (cm[1].includes('sidebar') || cm[1].includes('composer') || cm[1].includes('chat') || cm[1].includes('grok') || cm[1].includes('agent') || cm[1].includes('pane') || cm[1].includes('shell') || cm[1].includes('bubble') || cm[1].includes('settings')) {
    classes.add(cm[1]);
  }
}
console.log('\nRelevant CSS classes count:', classes.size);
console.log([...classes].sort().slice(0, 120).join(', '));
