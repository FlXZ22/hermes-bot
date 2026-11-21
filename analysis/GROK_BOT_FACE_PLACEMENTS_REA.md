# Grok Bot 1.2.3 Face Placement, Size & State Specification (REA)

Reverse-engineered directly from the official Linux package `grok-bot_1.2.3_amd64.deb` (`analysis/grok-bot-asar/dist/renderer/assets/index.eager-app-B6gpFfGB.js` and `index.eager-common-D2eZn7JB.js`) using REA static analysis (`tools/rea`).

## 1. Official Avatar Size Scale (`TO`)

Source: `index.eager-app-B6gpFfGB.js` @ offset `1366865`:

```js
var TO = {
  xs: 16,
  sm: 22,
  md: 28,
  lg: 36,
  xl: 72,
};
```

## 2. Exact UI Placements & Sizes (`yT` / `EO` / `GO` / `CO`)

| UI Location | Grok Bot Symbol / Offset | Size (`px`) | Playback / Flags | State Behavior |
|---|---|---|---|---|
| **Pinned Bots Grid (Sidebar)** | `GO` @ `1560615`, `1612824` | **`60px`** (`fillPx: 60, size: "xl"`) | `isStatic: false`, `maxFrameRate: 30` | Active bot tracks live state; others `idle` |
| **Sidebar Session / Bot Row (Expanded)** | `GO` @ `1560671`, `EO` @ `4153556` | **`36px`** (`TO.lg`, `Lur = 36`) | Active: live `30fps`; Inactive: `isStatic: true` | Active session live state; inactive `idle` |
| **Sidebar Session / Bot Row (Collapsed)** | `EO` @ `4153556` | **`22px`** (`TO.sm`, `Rur = 22`) | Active: live `30fps`; Inactive: `isStatic: true` | Active session live state |
| **Archived / Sleeping Bot Row** | `yT` @ `1906988` | **`24px`** (`pkt = 24`) | `isPaused: true`, `color: "gray"` | `state: "sleeping"` |
| **Chat Floating Title Pill** | `EO` @ `2781725`, `GO` @ `1480020` | **`24px`** (`xpn = 24`) | Live `60fps` | Live session state (`uO`) |
| **Chat Pending / Activity Indicator** | `CO` / `yO` @ `3076161` | **`28px`** (`$Y = 28`, `TO.md`) | Live `60fps` | Live turn activity (`thinking` dots, `writing` pencil, `searching` radar, `streaming`, `blocked`) |
| **Chat Empty State Hero** | `yT` @ `4358045` | **`72px`** (`rvr = 72`, `TO.xl`) | Live `60fps`, `isFollowingPointer: true` | Live session state / `idle`, click-to-spin |
| **Inspector / Details Panel** | `DO` @ `2323596` | **`56px`** (`BB = 56`) | Live `60fps` | Live session state |
| **Welcome / Onboarding Hero** | `yT` @ `3432727` | **`96px`** (`szn = 96`) | Live `60fps`, `isFollowingPointer: true` | `orbit` connecting, `celebrate` ready, `idle` default |
| **Avatar Editor Preview** | `EO` @ `1753206` | **`72px`** (`TO.xl`, `size: "xl"`) | Live `60fps`, `isFollowingPointer: true`, `clickableSpin` | Live preview of selected shape/color/state |
| **Avatar Editor Shape Grid** | `yT` @ `1378982` | **`36px`** (`Blt = 36`), 5 columns (`xn = 5`) | `isPaused: true`, `isStatic: true` | All 26 official shapes (`hn` + `pumpkin`) |
| **Compact Badges / Board Cards** | `GO` @ `1541583`, `3960194` | **`16px–22px`** (`TO.xs` / `TO.sm`) | `isStatic: true` | `idle` |

## 3. Exact Wrapper Rendering Parameters (`yT` @ `1158703`)

- **Fill ratio**: `fill: 0.92` (`Mi`)
- **Shape normalization (`bodyForShape`)**: `blob → sphere`, `flower → starFlower`, `heart → heartChubby`, `star → star6`; all other 22 shapes (`pebble`, `bean`, `egg`, `squircle`, `tablet`, `capsule`, `cylinder`, `hex`, `gem`, `crystal`, `wedge`, `shield`, `dome`, `arch`, `cloud`, `teardrop`, `leaf`, `square`, `sparkle`, `clover`, `house`, `pumpkin`) pass directly.
- **Cutout eyes**: `appearance(inkPaint)` (`cutout: true`) by default; `black` adapts to theme primary ink (`#1c1c1c` light / `#f5f5f5` dark).
- **Frame rate throttling**: `60fps` for hero/active surfaces, `30fps` for compact sidebar items (`flt` / `o = n ? 60 : 30`).

## 4. Exact Live State Machine (`uO` / `Xct` / `cO` / `Zct` @ `1357433–1358105`)

1. **`awaitingUserResponse != null`** → `blocked` (`Blocked_A`, transitions to `drowsy` after 120s in engine)
2. **Active turn (`isStreaming`)**:
   - Active tool call → mapped via `Xct`:
     - File write/edit/patch/draft → `writing` (**pencil morph** with angled shaft & live Catmull-Rom ink)
     - Search (`web_search`, `grep`, `glob`, `find`) → `searching` (**radar rings morph**)
     - Read/view/browse/fetch → `reading` (`Working_*`)
     - Shell/terminal/command → `running` (`Working_*`)
     - Code/eval/test/git → `coding` (`Working_*`)
     - Computer/screenshot/browser → `computing` (`Working_*`)
     - Image/audio/video generation → `progress` (**progress arc morph**)
     - Messaging/email/webhook → `sending` (**send morph**)
     - MCP/auth/connect → `orbit` (**orbit morph**)
     - Wait/schedule/subagent → `waiting` (**orbit morph**)
     - Other tool → `working` (`Working_*`)
   - Assistant text actively streaming → `streaming` (`Streaming_A/B/C`)
   - Reasoning / waiting for first token → `thinking` (**3 bouncing dots morph**)
3. **Turn settlement hold (`Zct = 8000ms`)**:
   - `success` → `finished` (`Done_B` / `Done_A` happy clip for 8s, then `idle`)
   - `error` → `failed` (`Blocked_B` for 8s, then `idle`)
   - `cancelled` → `idle`
4. **Default** → `idle` (`Idle_B`, `Idle_D`, `Idle_A`)
