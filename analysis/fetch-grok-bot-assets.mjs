#!/usr/bin/env node
/**
 * Downloads public theme assets (CSS, fonts, SVG bot marks, and client JS chunks)
 * from https://x.ai/bot into analysis/grok-bot-web/ so REA can statically analyze
 * the Grok Bot UI theme, tokens, icon font mappings, and component structure.
 */
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(__dirname, "grok-bot-web");
const BASE_URL = "https://x.ai";

async function fetchText(url) {
  const res = await fetch(url, {
    headers: { "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36" },
  });
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  return res.text();
}

async function fetchBytes(url) {
  const res = await fetch(url, {
    headers: { "User-Agent": "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36" },
  });
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  return new Uint8Array(await res.arrayBuffer());
}

async function main() {
  await mkdir(join(OUT_DIR, "css"), { recursive: true });
  await mkdir(join(OUT_DIR, "js"), { recursive: true });
  await mkdir(join(OUT_DIR, "fonts"), { recursive: true });

  console.log("Fetching https://x.ai/bot ...");
  const html = await fetchText(`${BASE_URL}/bot`);
  await writeFile(join(OUT_DIR, "index.html"), html, "utf8");

  // Extract CSS and JS chunk paths
  const cssPaths = [...new Set([...html.matchAll(/href="(\/_next\/static\/chunks\/[^"]+\.css[^"]*)"/g)].map((m) => m[1]))];
  const jsPaths = [...new Set([...html.matchAll(/src="(\/_next\/static\/chunks\/[^"]+\.js[^"]*)"/g)].map((m) => m[1]))];
  const fontPaths = [
    ...new Set([
      ...[...html.matchAll(/href="(\/_next\/static\/media\/[^"]+\.woff2[^"]*)"/g)].map((m) => m[1]),
      ...[...html.matchAll(/url\('(\/grok-bot-static\/fonts\/[^']+\.woff2[^']*)'\)/g)].map((m) => m[1]),
    ]),
  ];

  console.log(`Found ${cssPaths.length} CSS, ${jsPaths.length} JS, ${fontPaths.length} inline font URLs`);

  const allCss = [];
  for (const rel of cssPaths) {
    const cleanName = rel.split("?")[0].split("/").pop();
    const css = await fetchText(`${BASE_URL}${rel}`);
    allCss.push({ name: cleanName, css });
    await writeFile(join(OUT_DIR, "css", cleanName), css, "utf8");
    // Also grab fonts referenced from CSS (like CursorGothic)
    for (const m of css.matchAll(/url\(\.\.\/media\/([^)]+\.woff2[^)]*)\)/g)) {
      fontPaths.push(`/_next/static/media/${m[1].replace(/["']/g, "")}`);
    }
  }

  const uniqueFonts = [...new Set(fontPaths)];
  const savedFonts = [];
  for (const rel of uniqueFonts) {
    const cleanName = rel.split("?")[0].split("/").pop();
    try {
      const bytes = await fetchBytes(`${BASE_URL}${rel}`);
      await writeFile(join(OUT_DIR, "fonts", cleanName), bytes);
      savedFonts.push({ name: cleanName, url: `${BASE_URL}${rel}`, bytes: bytes.length });
      console.log(`Saved font ${cleanName} (${bytes.length} bytes)`);
    } catch (err) {
      console.warn(`Failed font ${rel}: ${err.message}`);
    }
  }

  const relevantChunks = [];
  for (const rel of jsPaths) {
    const cleanName = rel.split("?")[0].split("/").pop();
    try {
      const js = await fetchText(`${BASE_URL}${rel}`);
      await writeFile(join(OUT_DIR, "js", cleanName), js, "utf8");
      if (
        js.includes("baby-grok-bot") ||
        js.includes("CursorIcons16") ||
        js.includes("grok-bot-mark") ||
        js.includes("cursor-dark")
      ) {
        relevantChunks.push({ name: cleanName, length: js.length });
      }
    } catch (err) {
      console.warn(`Failed js ${rel}: ${err.message}`);
    }
  }

  // Extract unique SVG grok-bot-mark shapes from HTML
  const marks = new Map();
  for (const m of html.matchAll(/<svg[^>]*class="grok-bot-mark[^"]*"[^>]*>(.*?)<\/svg>/g)) {
    const inner = m[1];
    const headMatch = inner.match(/class="grok-bot-mark__head" d="([^"]+)"/);
    const eyeMatches = [...inner.matchAll(/class="grok-bot-mark__eye" d="([^"]+)"/g)].map((e) => e[1]);
    if (headMatch && eyeMatches.length === 2) {
      const key = headMatch[1].slice(0, 64);
      if (!marks.has(key)) {
        marks.set(key, { head: headMatch[1], eyes: eyeMatches });
      }
    }
  }

  const manifest = {
    source: `${BASE_URL}/bot`,
    fetchedAt: new Date().toISOString(),
    cssFiles: allCss.map((c) => c.name),
    jsCount: jsPaths.length,
    relevantThemeChunks: relevantChunks,
    fonts: savedFonts,
    uniqueBotMarkShapes: marks.size,
  };

  await writeFile(join(__dirname, "grok-bot-marks.json"), JSON.stringify([...marks.values()], null, 2), "utf8");
  await writeFile(join(__dirname, "manifest.json"), JSON.stringify(manifest, null, 2), "utf8");
  console.log("Done!", JSON.stringify(manifest, null, 2));
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
