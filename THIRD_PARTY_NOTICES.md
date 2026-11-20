# Third-party components and reference assets

- Grok Bot visual reference assets (XVF / XVF Mono fonts, CursorIcons16 font, application icons, and bot silhouettes) were extracted from the user-supplied Grok Bot 1.2.3 Linux package and the public Grok Bot website. Their original rights and any applicable restrictions remain with their respective owners. This repository does not assert a new license for those assets.
- Hermes Agent is an external installed runtime from Nous Research. Its source repository and installed license are available in the user's Hermes installation. It is not bundled into this binary.
- REA from https://github.com/morluto/rea was used for static reference analysis. Its own license is retained in tools/rea/LICENSE.
- xterm.js and its fit/serialize addons provide terminal rendering under the MIT license. portable-pty (part of WezTerm) provides native pseudoterminals under its upstream license. Their licenses remain in the installed package/crate metadata.
- Tauri, React, and the other build dependencies retain their respective upstream licenses, as recorded in Cargo.lock, package-lock.json, and their installed package metadata.

This application is a personal reconstruction and is not an official Grok Bot or Nous Research release.
