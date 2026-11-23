# Grok Bot face review

A standalone review website that runs the face engine recovered from the official Grok Bot **1.2.3** Linux application. Start it with `npm start` and open `http://127.0.0.1:5188`. `npm run build` copies the self-contained website into `dist/`.

The catalog exposes all 53 shipped states, 28 authored clips, 9 eye expressions, 13 morph selectors, 26 body geometries, 11 avatar colors with the app's light/dark tokens, pointer gaze, look directions, blink, spin, bounce, and nod. State and shape changes use the original transition code. The inspector supplies pause, replay, speed, and timeline controls for review.

“Editing” has no distinct state in this release. The original activity classifier maps file drafting to `writing` (the pencil transformation); coding/work uses the `Working_A`–`Working_F` clips. Browsing maps to reading, generation to progress, commands to running, computer activity to computing/controlling, messaging to sending, and connection activity to orbit. These aliases do not add new animation recipes.

## Recovery and tools

[REA](https://github.com/morluto/rea) 6.10.1 extracted the official ASAR. Its JavaScript static-analysis library inspected the isolated renderer code. Ghidra and additional MCP registrations were unnecessary. The CLI's worker returned an incomplete result on this host; both unsuccessful reports are preserved. `scripts/rea-static.mjs` calls the same REA parser and static analyzer in-process, with its original uncertainty and semantic limits recorded in `evidence/rea-static-engine.json`.

The official download is linked by [x.ai/bot](https://x.ai/bot) and redirects to:

`https://downloads.cursor.com/grokbot/stable/a5927fa2ba82336a29f9343babd28b4c903abbb2/linux/x64/grok-bot_1.2.3_amd64.deb`

Debian package SHA-256: `5917521542680b07813a429911f01bfdd191ec6b5409fe5eab623ad5fbf3b08a`.

Source renderer: `dist/renderer/assets/index.eager-common-D2eZn7JB.js`, SHA-256 `77e626ca2ab064adb8bc45453d07ee3013bacdc66095df0dfa5ff643672ec055`.

`scripts/extract-engine.cjs` selects the transitive dependencies of the motion host, geometry renderer, director, clips, palettes, and catalogs. Each recovered declaration is copied unchanged, with byte ranges and hashes in `evidence/engine-provenance.json`. The 756 declarations have no application bootstrap, telemetry, network calls, account code, or React dependency. The buildless viewer is new code around this engine.

The application wrapper `yT` uses fill `0.92`, maps blob/flower/heart/star to sphere/starFlower/heartChubby/star6, and resolves chromatic colors through bright theme primitives rather than the engine's alternate palette. The viewer follows those wrapper choices. Color and state catalogs are available at `demo/catalog.json`.

## Verification

`npm test` evaluates the recovered engine against the complete, unmodified original renderer bundle, with added test exports. The retained result in `evidence/verification.json` records **80,344 exact frame comparisons and 3,000 exact SVG comparisons across 457 scenarios**. Coverage includes every state (including blocked's 120-second transition), every clip over two loops, every body/expression combination, expression-pair blink transitions, morph transitions, shape transitions, gestures, gaze, and look directions.

`scripts/browser-qa.cjs` uses the shared Playwright installation documented in the supplied AGENTS instructions. It verifies catalog selection, controls, themes, and the mobile layout, with screenshots under `evidence/`.

The checks establish the recovered engine's equivalence within the exercised scenarios. They do not certify pixel equivalence of the entire authenticated desktop application. Seeded review playback begins at phase zero; the application may begin at a randomized phase. Pause, seeking, and playback speed are review controls. Gallery thumbnails show a fixed sample; click a tile for its full animation.

## Files

- `demo/`: standalone browser deliverable, without dependencies or a build step.
- `scripts/`: reproducible extraction, comparison, build, server, and browser checks.
- `evidence/`: original REA extraction, static findings, provenance, and test results.
- `artifacts/`: downloaded package, extracted application, and original comparison fixture; locally retained and ignored by Git.
- `tools/`: REA checkout, pinned runtime, and formatting tool; locally retained and ignored by Git.
- `site/`: private Sites source checkout and deployment package source.

Original Grok Bot code and font assets retain their respective owners' rights. This review contains recovered application material; no open-source license for that material is asserted.
