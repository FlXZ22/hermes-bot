import fs from 'node:fs';

const assetsDir = '/home/metis/Projects/grok-bot-ui/analysis/grok-bot-asar/dist/renderer/assets';
const plat = fs.readFileSync(`${assetsDir}/index.eager-platform-D6aSBzVM.js`, 'utf8');
const app = fs.readFileSync(`${assetsDir}/index.eager-app-B6gpFfGB.js`, 'utf8');

// 1. Decode FULL CursorIcons16 table (pa, ma, ha, ga, _a)
const iVa = plat.indexOf('function va(e){');
const iPa = plat.lastIndexOf('var pa=', iVa);
const iconSlice = plat.slice(iPa, plat.indexOf('function Oa(', iVa));

const paMatch = iconSlice.match(/pa=`([^`]*)`/);
const maMatch = iconSlice.match(/ma=`([^`]*)`/);
const haMatch = iconSlice.match(/ha=`([^`]*)`/);
const gaMatch = iconSlice.match(/ga=`([^`]*)`/);
const stepMatch = iconSlice.match(/_a=(\d+)/);

function va(e) {
  const t = [];
  let n = '';
  for (const r of e.split(' ')) {
    n = n.slice(0, Number.parseInt(r.charAt(0), 36)) + r.slice(1);
    t.push(n);
  }
  return t;
}
function ya(e) {
  const t = [];
  let n = 0;
  for (const r of e.split(' ')) {
    n += Number.parseInt(r, 36);
    t.push(n);
  }
  return t;
}

const names = va(paMatch[1]);
const outlineCodes = ya(maMatch[1]);
const filledCodes = ya(gaMatch[1]);
const ha = haMatch[1];
const step = Number(stepMatch[1]);

const outlineMap = {};
for (let i = 0; i < names.length; i++) {
  if (outlineCodes[i] !== undefined) {
    outlineMap[names[i]] = outlineCodes[i].toString(16).toUpperCase();
  }
}
const filledMap = {};
for (let i = 0; i < filledCodes.length; i++) {
  const name = names[Number.parseInt(ha.slice(i * step, (i + 1) * step), 36)];
  if (name !== undefined && filledCodes[i] !== undefined) {
    filledMap[name] = filledCodes[i].toString(16).toUpperCase();
  }
}

fs.writeFileSync(
  '/home/metis/Projects/grok-bot-ui/analysis/desktop-icons.json',
  JSON.stringify({ totalOutline: Object.keys(outlineMap).length, totalFilled: Object.keys(filledMap).length, outline: outlineMap, filled: filledMap }, null, 2)
);
console.log(`Decoded desktop icons: ${Object.keys(outlineMap).length} outline, ${Object.keys(filledMap).length} filled`);

// 2. Locate Xa (light + dark palette) and gAr in index.eager-app
const iGAr = app.indexOf('function gAr(');
const iBAr = app.indexOf('function bAr(');
console.log('gAr at', iGAr, 'bAr at', iBAr);
if (iGAr !== -1) console.log('\n--- gAr / bAr ---\n', app.slice(Math.max(0, iGAr - 200), iGAr + 1600));

const iXa = app.indexOf('var Xa=');
const iXa2 = app.indexOf(',Xa=');
const xaPos = iXa !== -1 ? iXa : iXa2;
console.log('Xa at', xaPos);
if (xaPos !== -1) {
  console.log('\n--- Xa ---\n', app.slice(xaPos, xaPos + 2400));
}

// 3. Find dark sand palette (search for #141414 or #181818 or #1c1c1c in app near light palette #f5f5f5)
let f5Pos = 0;
let found = 0;
while ((f5Pos = app.indexOf('#f5f5f5', f5Pos)) !== -1 && found < 5) {
  const ctx = app.slice(Math.max(0, f5Pos - 250), f5Pos + 500);
  if (ctx.includes('sidebar') || ctx.includes('pane') || ctx.includes('chrome')) {
    console.log('\n--- Palette around #f5f5f5 at', f5Pos, '---\n', ctx);
    found++;
  }
  f5Pos += 7;
}
