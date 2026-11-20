import fs from 'node:fs';
import path from 'node:path';

const assetsDir = '/home/metis/Projects/grok-bot-ui/analysis/grok-bot-asar/dist/renderer/assets';
const css = fs.readFileSync(path.join(assetsDir, 'index-CqCBWzOp.css'), 'utf8');

// 1. All @font-face
const fontFaces = css.match(/@font-face\{[^}]+\}/g) || [];

// 2. All --sand-* and --cursor-* variable blocks (including dark mode!)
const sandBlocks = [];
const re = /([^{}@]*)\{([^}]*(?:--sand-|--cursor-bg-|--cursor-text-|--cursor-icon-|--cursor-border-|--grok-bot-)[^}]*)\}/g;
let m;
while ((m = re.exec(css))) {
  sandBlocks.push({ selector: m[1].trim(), body: m[2].trim() });
}

// 3. Check cursor-icons-16 CSS rules (glyph codes)
const iconRules = [];
const iconRe = /\.([a-zA-Z0-9_-]+):before\{content:"\\([0-9a-fA-F]+)"\}/g;
while ((m = iconRe.exec(css))) {
  iconRules.push({ name: m[1], code: m[2].toUpperCase() });
}

// 4. Extract color/avatar definitions from chunk-agent-avatar-editor and index.eager-app
const avatarEditorJs = fs.readFileSync(path.join(assetsDir, 'chunk-agent-avatar-editor-KnneU8LA.js'), 'utf8');
const eagerAppJs = fs.readFileSync(path.join(assetsDir, 'index.eager-app-B6gpFfGB.js'), 'utf8');

// Find icon unicode map in eager JS if not in CSS
const iconMapMatches = [];
for (const fname of ['index.eager-app-B6gpFfGB.js', 'index.eager-common-D2eZn7JB.js', 'index.eager-platform-D6aSBzVM.js', 'index-Dy9GRwPf.js']) {
  const txt = fs.readFileSync(path.join(assetsDir, fname), 'utf8');
  const idx = txt.indexOf('\\uea6d');
  const idx2 = txt.indexOf('\\uEA6D');
  const idx3 = txt.indexOf('ea6d');
  const idx4 = txt.indexOf('cursor-icons-16');
  const idx5 = txt.indexOf('CursorIcons');
  if (idx !== -1 || idx2 !== -1 || idx3 !== -1 || idx4 !== -1 || idx5 !== -1) {
    iconMapMatches.push({ fname, idx, idx2, idx3, idx4, idx5 });
  }
}

// Extract avatar palette & shapes snippet around M228.54 114.27 or #97683D
const shapeIdx = eagerAppJs.indexOf('M228.54 114.27');
const colorIdx = eagerAppJs.indexOf('97683D');
const colorIdxLower = eagerAppJs.indexOf('97683d');

// Extract dark/light theme setup in CSS
const darkSelectors = sandBlocks.filter((b) =>
  b.selector.includes('dark') || b.body.includes('#1c1c1c') || b.body.includes('#141414') || b.body.includes('color-scheme')
);

const out = {
  cssBytes: css.length,
  fontFaces: fontFaces.filter((f) => !f.includes('KaTeX')),
  sandBlockCount: sandBlocks.length,
  sandBlocks: sandBlocks.slice(0, 60),
  iconRuleCount: iconRules.length,
  iconRulesSample: iconRules.slice(0, 50),
  iconMapMatches,
  shapeIdx,
  colorIdx,
  colorIdxLower,
  colorSnippet: colorIdx !== -1 ? eagerAppJs.slice(Math.max(0, colorIdx - 300), colorIdx + 1200) : (colorIdxLower !== -1 ? eagerAppJs.slice(Math.max(0, colorIdxLower - 300), colorIdxLower + 1200) : null),
  avatarEditorHead: avatarEditorJs.slice(0, 2500),
};

fs.writeFileSync('/home/metis/Projects/grok-bot-ui/analysis/desktop-theme-extract.json', JSON.stringify(out, null, 2));
console.log('Summary:', {
  cssBytes: out.cssBytes,
  nonKatexFontFaces: out.fontFaces.length,
  sandBlockCount: out.sandBlockCount,
  darkSelectorCount: darkSelectors.length,
  iconRuleCount: out.iconRuleCount,
  iconMapMatches: out.iconMapMatches,
  shapeIdx: out.shapeIdx,
  colorIdx: out.colorIdx,
  colorIdxLower: out.colorIdxLower,
});
console.log('\nNon-KaTeX @font-face:\n', out.fontFaces.join('\n'));
console.log('\nDark/Sand selectors:');
for (const b of sandBlocks) {
  if (b.body.includes('--sand-bg-') || b.body.includes('--sand-text-')) {
    console.log(`SEL: ${b.selector}\n${b.body}\n`);
  }
}
