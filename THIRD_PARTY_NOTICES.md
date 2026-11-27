# Third-party components and reference assets

- Grok Bot visual reference assets (XVF / XVF Mono fonts, CursorIcons16 font, application icons, and bot silhouettes) were extracted from the user-supplied Grok Bot 1.2.3 Linux package and the public Grok Bot website. Their original rights and any applicable restrictions remain with their respective owners. This repository does not assert a new license for those assets.
- Hermes Agent is an external installed runtime from Nous Research (`https://github.com/NousResearch/hermes-agent`, MIT License, Copyright (c) 2025 Nous Research). Its source repository and installed license are available in the user's Hermes installation. It is not bundled into this binary.
- DeepSeek Harness (`https://github.com/deepseek-ai/deepseek-harness`, pinned reference commit `4b399c39a6d1479cd84ca591b4d7c88e24d38a96`, MIT License, Copyright (c) 2026 DeepSeek) was studied as an architectural and interaction reference for trajectory inspection, goal lifecycles, subagent views, and programmatic tool calling (`run_code`) inspection patterns. This application is not affiliated with or endorsed by DeepSeek.
- REA from https://github.com/morluto/rea was used for static reference analysis. Its own license is retained in tools/rea/LICENSE.
- xterm.js and its fit/serialize addons provide terminal rendering under the MIT license. portable-pty (part of WezTerm) provides native pseudoterminals under its upstream license. Their licenses remain in the installed package/crate metadata.
- Tauri, React, and the other build dependencies retain their respective upstream licenses, as recorded in Cargo.lock, package-lock.json, and their installed package metadata.

This application is a personal reconstruction and is not an official Grok Bot or Nous Research release.
