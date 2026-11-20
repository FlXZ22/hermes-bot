#!/usr/bin/env node
/**
 * Extracts CursorIcons16 glyph mappings, bot coat/tint palettes, bot shapes,
 * and exact baby-grok-bot component structures from the fetched x.ai/bot JS chunks.
 */
import { readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const JS_DIR = join(__dirname, "grok-bot-web", "js");
const CHUNKS = [
  "30bechs5m37tr.js",
  "3a0fz1o9-rxp3.js",
  "2ruviwhb973r3.js",
  "1qjn-osn7_gob.js",
  "32sdt7g0j3wei.js",
  "0cf59cht7b050.js",
];

async function main() {
  const report = {
    cursorIconSnippets: [],
    agentThemes: [],
    classUsages: new Set(),
    uiStrings: [],
    componentExcerpts: {},
  };

  for (const file of CHUNKS) {
    const src = await readFile(join(JS_DIR, file), "utf8");

    // 1. Find CursorIcons16 usage and nearby icon glyph maps
    let idx = 0;
    while ((idx = src.indexOf("CursorIcons16", idx)) !== -1) {
      report.cursorIconSnippets.push({
        file,
        offset: idx,
        snippet: src.slice(Math.max(0, idx - 400), Math.min(src.length, idx + 800)),
      });
      idx += 13;
    }

    // 2. Find all private-use unicode glyph maps like { ...: "\u..." } or { ...: "..." }
    for (const m of src.matchAll(/([a-zA-Z0-9_-]{2,40}):\s*["']([\uE000-\uF8FF]|\\u[eEfF][0-9a-fA-F]{3})["']/g)) {
      const name = m[1];
      const raw = m[2];
      const code = raw.startsWith("\\u")
        ? parseInt(raw.slice(2), 16)
        : raw.codePointAt(0);
      report.uiStrings.push({ kind: "icon_glyph", file, name, hex: `0x${code.toString(16).toUpperCase()}` });
    }

    // 3. Find baby-grok-bot-* class references
    for (const m of src.matchAll(/baby-grok-bot[a-zA-Z0-9_-]*/g)) {
      report.classUsages.add(m[0]);
    }

    // 4. Find agent coat/tint definitions
    for (const m of src.matchAll(/--grok-bot-current-agent-[a-z-]+/g)) {
      const start = Math.max(0, m.index - 200);
      const end = Math.min(src.length, m.index + 500);
      report.agentThemes.push({ file, offset: m.index, snippet: src.slice(start, end) });
    }

    // 5. Capture key layout snippets from 32sdt7g0j3wei.js and 0cf59cht7b050.js
    for (const marker of [
      "baby-grok-bot-shell",
      "baby-grok-bot-sidebar-header",
      "baby-grok-bot-pinned-grid",
      "baby-grok-bot-agent-title-pill",
      "baby-grok-bot-composer",
      "baby-grok-bot-side-pane",
      "baby-grok-bot-approval",
      "baby-grok-bot-rich",
    ]) {
      const pos = src.indexOf(marker);
      if (pos !== -1 && !report.componentExcerpts[marker]) {
        report.componentExcerpts[marker] = {
          file,
          offset: pos,
          snippet: src.slice(Math.max(0, pos - 300), Math.min(src.length, pos + 1500)),
        };
      }
    }
  }

  const output = {
    ...report,
    classUsages: [...report.classUsages].sort(),
  };

  await writeFile(join(__dirname, "grok-bot-extracted.json"), JSON.stringify(output, null, 2), "utf8");
  console.log(
    `Extracted ${output.uiStrings.length} icon glyphs, ${output.classUsages.length} classes, ${output.cursorIconSnippets.length} CursorIcons16 hits, ${Object.keys(output.componentExcerpts).length} component excerpts`
  );
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
