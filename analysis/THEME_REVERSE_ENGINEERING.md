# Grok Bot Theme Reverse-Engineering Report (REA)

This document records the design-system reverse engineering of Grok Bot using [`morluto/rea`](https://github.com/morluto/rea) (`tools/rea`) across both the public web preview (`https://x.ai/bot`) and the official Linux desktop package (`grok-bot_1.2.3_amd64.deb`).

## Artifacts Analyzed

1. **Web Preview (`analysis/grok-bot-web`)**
   - 1 HTML entry, 6 CSS bundles, 55 JavaScript chunks, 4 `.woff2` fonts
   - Analyzed statically with REA: `analysis/rea-grok-bot-evidence.json`
   - Extracted 9 organic SVG `grok-bot-mark` bot head + slanted capsule eye paths (`analysis/grok-bot-marks.json`)
   - Extracted `.baby-grok-bot` light & `[data-theme="cursor-dark"]` glass tokens, shell layout, sidebar, pinned grid, chat bubbles, floating title pill, composer, approval cards, rich cards, and side pane (`analysis/grok-bot-web/css/1t-q8mt3it8fo.css` and `0tg00au57e2ry.css`)

2. **Linux Desktop App (`grok-bot_1.2.3_amd64.deb` -> `analysis/grok-bot-asar/dist/renderer`)**
   - Analyzed statically with REA: `analysis/rea-grok-bot-desktop-evidence.json`
   - Extracted exact desktop `@font-face` assets:
     - `cursor-icons-16-QUmxS7q3.woff2` (`cursor-icons` icon font)
     - `X-VF-C7NyO54W.woff2` (`XVF` variable sans typeface, weights 200-800)
     - `XMono-VF-K4lk0ar0.woff2` (`XVF Mono` variable monospace typeface, weights 200-800)
   - Decoded the complete `cursor-icons-16` icon table from `index.eager-platform-D6aSBzVM.js`: **818 outline glyphs** and **406 filled glyphs** (`analysis/desktop-icons.json`)
   - Extracted the complete `Sand Light` (`sand-light`) and `Sand Dark` (`sand-dark`) semantic token primitives (`index.eager-common-D2eZn7JB.js`):
     - Light neutrals: elevated `#ffffff`, pane `#ffffff`, chrome `#fcfcfc`, sidebar `#f5f5f5`, base `#1c1c1c`, hover `#363636`
     - Dark neutrals: elevated `#1f1f1f`, pane `#161616`, chrome `#121212`, sidebar `#0e0e0e`, base `#f5f5f5`, hover `#dbdbdb`
     - 11 official bot avatar colors (`black`, `brown`, `red`, `orange`, `yellow`, `green`, `cyan`, `blue`, `violet`, `magenta`, `gray`)
     - Deterministic FNV-1a + splitmix32 bot color and shape assignment (`On`, `An`, `Pn`)
     - Exact typography scale (`heading-1..3`, `body-1..2`, `label`, `caption`), radii (`base 8px`, `lg 10px`, `xl 14px`, `2xl 16px`, `3xl 18px`), shadow stack (`control`, `inline`, `popover`, `modal`, `window`), and dataviz categorical palette (`#1084FE`, `#FF6700`, `#9159FE`, `#00C972`, `#FF309B`, `#FF9800`, `#00BCA6`, `#97683D`)
   - Extracted official Linux hicolor PNG app icons (`16x16` through `512x512`).

## Reuse Package

All extracted visual tokens, fonts, icons, bot marks, and layout primitives are packaged cleanly in `packages/grok-bot-theme` (`@grok-bot/theme`) for reuse across both the Linux desktop app and standalone Hermes web applications.


## Continuation verification

The continued build retains a focused REA analysis of the shipped `assets/index.eager-common-D2eZn7JB.js` design-system source in `analysis/rea-design-verification`, with a compact summary in `analysis/rea-design-summary.json`. The parent evidence ID is `ev_f14d2f34003b10891d598c713df50a84def2b97982ae9f6ca3047f2deebbaf52`. Source selection and semantic retention limits are recorded in that evidence; this is static analysis and does not establish runtime pixel equivalence.

The fonts and app icons are used from the extracted package rather than replaced with generic substitutes. The reconstructed light/dark theme cascade was corrected so the root theme is inherited by the shell. Browser inspection verified that installed Hermes sessions, skills, toolsets, model providers, and board controls load through the local development proxy. Automated gateway tests use an isolated home and local test model to verify streaming, persistence, attachments, archive/restore, boards, and schedules.

## Welcome flow (19 November continuation)

Analyzed the shipped `assets/chunk-onboarding-flow-CGMElaOD.js` with REA, retaining the evidence in `analysis/rea-onboarding-verification` and a compact result in `analysis/rea-onboarding-summary.json`. Static observations include a full-screen sequence (`landing`, `meet`, `computer-demo`, `jobs`, attribution/role, connect/tools, create, hand-off), a black blob hero, red/cyan/blue companion bots, animated floating positions, shared heading/footer layouts, and back/forward navigation. The selected-file analysis has explicit node/unknown ceilings and leaves cross-chunk behavior unresolved; it is not a runtime visual comparison of the entire Grok onboarding.

The Hermes welcome reuses the extracted XVF font, icon font, marks, colors, and light/dark tokens. Its local/server choice and connection forms adapt those patterns to Hermes infrastructure rather than reproducing Grok account registration or service-connector questions. Reference the welcome and setup screenshots in `analysis/verification-welcome-dark.png`, `analysis/verification-welcome-light.png`, and `analysis/verification-remote-setup.png`.
