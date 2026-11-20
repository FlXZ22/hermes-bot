# @grok-bot/theme

Reusable Grok Bot theme, icon font (`cursor-icons-16`), variable fonts (`XVF`, `XVF Mono`), organic SVG bot marks, and UI primitives for any Hermes Agent web or desktop application.

## Install and Use in Any Hermes Web App

1. Import the stylesheet once in your app entry (`main.tsx`):

```ts
import "@grok-bot/theme/styles";
```

2. Set the theme attribute on `<html>` or your root wrapper:

```ts
document.documentElement.dataset.theme = "cursor-dark"; // or "cursor-light"
```

3. Use the theme primitives and per-bot color helpers:

```tsx
import {
  BotMark,
  CursorIcon,
  ApprovalCard,
  ToggleSwitch,
  getAgentThemeVars,
} from "@grok-bot/theme";

const style = getAgentThemeVars("hermes", "dark", "blue");
```
