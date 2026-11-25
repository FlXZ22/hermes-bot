use gpui::{hsla, rgb, rgba, Hsla, Rgba};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeMode {
    Light,
    Dark,
}

impl Default for ThemeMode {
    fn default() -> Self {
        Self::Dark
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BotColorPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub hex: u32,
    pub light_accent: u32,
    pub dark_accent: u32,
    pub light_bubble: u32,
    pub dark_bubble: u32,
    pub light_tint: u32,
    pub dark_tint: u32,
}

pub const BOT_COLOR_PRESETS: &[BotColorPreset] = &[
    BotColorPreset {
        id: "blue",
        label: "Blue",
        hex: 0x1084FE,
        light_accent: 0x0b6dd3,
        dark_accent: 0x4d9bff,
        light_bubble: 0x126fd4,
        dark_bubble: 0x116dd2,
        light_tint: 0x1084FE,
        dark_tint: 0x1084FE,
    },
    BotColorPreset {
        id: "violet",
        label: "Violet",
        hex: 0x9159FE,
        light_accent: 0x7547d0,
        dark_accent: 0xa582ff,
        light_bubble: 0x7442d1,
        dark_bubble: 0x7845d7,
        light_tint: 0x9159FE,
        dark_tint: 0x9159FE,
    },
    BotColorPreset {
        id: "cyan",
        label: "Cyan",
        hex: 0x00BCA6,
        light_accent: 0x008273,
        dark_accent: 0x00b7a2,
        light_bubble: 0x0d9381,
        dark_bubble: 0x0e9886,
        light_tint: 0x00BCA6,
        dark_tint: 0x00BCA6,
    },
    BotColorPreset {
        id: "green",
        label: "Green",
        hex: 0x00C972,
        light_accent: 0x00854a,
        dark_accent: 0x00ba69,
        light_bubble: 0x0d9655,
        dark_bubble: 0x0e9f5a,
        light_tint: 0x00C972,
        dark_tint: 0x00C972,
    },
    BotColorPreset {
        id: "orange",
        label: "Orange",
        hex: 0xFF6700,
        light_accent: 0xc14c00,
        dark_accent: 0xfe6600,
        light_bubble: 0xd2550a,
        dark_bubble: 0xcd5309,
        light_tint: 0xFF6700,
        dark_tint: 0xFF6700,
    },
    BotColorPreset {
        id: "red",
        label: "Red",
        hex: 0xFF263C,
        light_accent: 0xcd2834,
        dark_accent: 0xff6463,
        light_bubble: 0xc82532,
        dark_bubble: 0xcf2b35,
        light_tint: 0xFF263C,
        dark_tint: 0xFF263C,
    },
    BotColorPreset {
        id: "magenta",
        label: "Magenta",
        hex: 0xFF309B,
        light_accent: 0xc62378,
        dark_accent: 0xff55a4,
        light_bubble: 0xc82779,
        dark_bubble: 0xcf297e,
        light_tint: 0xFF309B,
        dark_tint: 0xFF309B,
    },
    BotColorPreset {
        id: "yellow",
        label: "Yellow",
        hex: 0xFF9800,
        light_accent: 0xaa6300,
        dark_accent: 0xfb9500,
        light_bubble: 0xc2730b,
        dark_bubble: 0xc8770c,
        light_tint: 0xFF9800,
        dark_tint: 0xFF9800,
    },
    BotColorPreset {
        id: "brown",
        label: "Brown",
        hex: 0x936439,
        light_accent: 0x996b42,
        dark_accent: 0xc59e7d,
        light_bubble: 0x86592e,
        dark_bubble: 0x8c5d31,
        light_tint: 0x936439,
        dark_tint: 0x936439,
    },
    BotColorPreset {
        id: "gray",
        label: "Gray",
        hex: 0x777777,
        light_accent: 0x525252,
        dark_accent: 0xc1c1c1,
        light_bubble: 0x363636,
        dark_bubble: 0x525252,
        light_tint: 0x777777,
        dark_tint: 0x777777,
    },
    BotColorPreset {
        id: "black",
        label: "Black",
        hex: 0x1C1C1C,
        light_accent: 0x1c1c1c,
        dark_accent: 0xf5f5f5,
        light_bubble: 0x1c1c1c,
        dark_bubble: 0xf5f5f5,
        light_tint: 0x1c1c1c,
        dark_tint: 0x777777,
    },
];

pub const DATAVIZ_LIGHT: &[u32] = &[
    0x1084FE, 0xFF6700, 0x9159FE, 0x00C972, 0xFF309B, 0xFF9800, 0x00BCA6, 0x936439,
];

pub const DATAVIZ_DARK: &[u32] = &[
    0x1084FE, 0xFF6700, 0x9159FE, 0x00C972, 0xFF309B, 0xFF9800, 0x00BCA6, 0x97683D,
];

fn fnv1a(input: &str) -> u32 {
    let mut h: u32 = 2166136261;
    for b in input.encode_utf16() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h
}

fn splitmix32(seed: u32) -> f64 {
    let t = seed.wrapping_add(1831565813);
    let mut r = (t ^ (t >> 15)).wrapping_mul(1 | t);
    r = r.wrapping_add((r ^ (r >> 7)).wrapping_mul(61 | r)) ^ r;
    ((r ^ (r >> 14)) as f64) / 4294967296.0
}

pub fn resolve_bot_color(seed: &str, preferred: Option<&str>) -> &'static BotColorPreset {
    if let Some(pref) = preferred {
        if let Some(found) = BOT_COLOR_PRESETS.iter().find(|c| c.id == pref) {
            return found;
        }
    }
    // Exclude "black" for automatic assignment (matches AUTO_COLORS in tokens.ts)
    let auto_len = BOT_COLOR_PRESETS.len() - 1;
    let hashed = fnv1a(seed) ^ 2654435769u32;
    let idx = (splitmix32(hashed ^ 2654435769u32) * (auto_len as f64)).floor() as usize;
    &BOT_COLOR_PRESETS[idx.min(auto_len - 1)]
}

pub fn mix_rgba(a: Rgba, b: Rgba, ratio_a: f32) -> Hsla {
    let rb = 1.0 - ratio_a;
    Rgba {
        r: a.r * ratio_a + b.r * rb,
        g: a.g * ratio_a + b.g * rb,
        b: a.b * ratio_a + b.b * rb,
        a: a.a * ratio_a + b.a * rb,
    }
    .into()
}

pub fn with_alpha(hex: u32, alpha: f32) -> Hsla {
    let mut c: Rgba = rgb(hex);
    c.a = alpha;
    c.into()
}

/// Complete Grok Bot 1.2.3 Sand Light / Sand Dark semantic color tokens
#[derive(Debug, Clone, Copy)]
pub struct SandTheme {
    pub mode: ThemeMode,
    // Surfaces
    pub bg_pane: Hsla,
    pub bg_chrome: Hsla,
    pub bg_sidebar: Hsla,
    pub bg_elevated: Hsla,
    pub bg_elevated_hover: Hsla,
    pub bg_scrim: Hsla,
    pub bg_hover: Hsla,
    pub bg_active: Hsla,
    pub bg_primary: Hsla,
    pub bg_primary_hover: Hsla,
    pub bg_secondary: Hsla,
    pub bg_secondary_hover: Hsla,
    pub bg_tertiary: Hsla,
    pub bg_card: Hsla,
    // Text & Icons
    pub text_primary: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    pub text_quaternary: Hsla,
    pub text_on_primary: Hsla,
    // Borders
    pub border_primary: Hsla,
    pub border_default: Hsla,
    pub border_subtle: Hsla,
    pub border_hairline: Hsla,
    // Sidebar specifics
    pub sidebar_text_primary: Hsla,
    pub sidebar_text_secondary: Hsla,
    pub sidebar_text_tertiary: Hsla,
    pub sidebar_separator: Hsla,
    pub sidebar_hover: Hsla,
    pub sidebar_selected: Hsla,
    // Chat bubbles
    pub bubble_user: Hsla,
    pub bubble_user_ink: Hsla,
    pub bubble_agent: Hsla,
    // Semantic roles
    pub accent_primary: Hsla,
    pub accent_soft: Hsla,
    pub accent_text: Hsla,
    pub danger_primary: Hsla,
    pub danger_soft: Hsla,
    pub danger_text: Hsla,
    pub success_primary: Hsla,
    pub success_soft: Hsla,
    pub success_text: Hsla,
    pub warn_primary: Hsla,
    pub warn_soft: Hsla,
    pub warn_text: Hsla,
    // Active bot accent & side-pane tint
    pub agent_accent: Hsla,
    pub agent_accent_soft: Hsla,
    pub agent_accent_border: Hsla,
    pub side_pane_bg: Hsla,
}

impl SandTheme {
    pub fn for_mode(mode: ThemeMode, bot_seed: &str, bot_color: Option<&str>) -> Self {
        let preset = resolve_bot_color(bot_seed, bot_color);
        match mode {
            ThemeMode::Light => {
                let agent_accent_hex = preset.light_accent;
                let agent_tint_hex = preset.light_tint;
                Self {
                    mode,
                    bg_pane: rgb(0xffffff).into(),
                    bg_chrome: rgb(0xfcfcfc).into(),
                    bg_sidebar: rgb(0xf5f5f5).into(),
                    bg_elevated: rgb(0xffffff).into(),
                    bg_elevated_hover: rgb(0xf1f1f1).into(),
                    bg_scrim: rgba(0x0000006b).into(),
                    bg_hover: with_alpha(0x1c1c1c, 0.06),
                    bg_active: with_alpha(0x1c1c1c, 0.09),
                    bg_primary: rgb(0x1c1c1c).into(),
                    bg_primary_hover: rgb(0x363636).into(),
                    bg_secondary: with_alpha(0x1c1c1c, 0.06),
                    bg_secondary_hover: with_alpha(0x1c1c1c, 0.10),
                    bg_tertiary: with_alpha(0x1c1c1c, 0.05),
                    bg_card: with_alpha(0x1c1c1c, 0.05),
                    text_primary: rgb(0x1c1c1c).into(),
                    text_secondary: with_alpha(0x1c1c1c, 0.68),
                    text_tertiary: with_alpha(0x1c1c1c, 0.56),
                    text_quaternary: with_alpha(0x1c1c1c, 0.40),
                    text_on_primary: rgb(0xffffff).into(),
                    border_primary: with_alpha(0x1c1c1c, 0.24),
                    border_default: with_alpha(0x1c1c1c, 0.12),
                    border_subtle: with_alpha(0x1c1c1c, 0.08),
                    border_hairline: with_alpha(0x1c1c1c, 0.06),
                    sidebar_text_primary: rgb(0x1c1c1c).into(),
                    sidebar_text_secondary: with_alpha(0x1c1c1c, 0.64),
                    sidebar_text_tertiary: with_alpha(0x1c1c1c, 0.44),
                    sidebar_separator: with_alpha(0x1c1c1c, 0.10),
                    sidebar_hover: with_alpha(0x1c1c1c, 0.05),
                    sidebar_selected: with_alpha(0x1c1c1c, 0.085),
                    bubble_user: rgb(0x1c1c1c).into(),
                    bubble_user_ink: rgb(0xffffff).into(),
                    bubble_agent: with_alpha(0x1c1c1c, 0.055),
                    accent_primary: rgb(0x1680f3).into(),
                    accent_soft: with_alpha(0x1e86fc, 0.10),
                    accent_text: rgb(0x0b6dd3).into(),
                    danger_primary: rgb(0xe72e3b).into(),
                    danger_soft: with_alpha(0xfb2b3e, 0.10),
                    danger_text: rgb(0xcd2834).into(),
                    success_primary: rgb(0x10ab61).into(),
                    success_soft: with_alpha(0x14c16f, 0.10),
                    success_text: rgb(0x00854a).into(),
                    warn_primary: rgb(0xda830e).into(),
                    warn_soft: with_alpha(0xf59312, 0.12),
                    warn_text: rgb(0xaa6300).into(),
                    agent_accent: rgb(agent_accent_hex).into(),
                    agent_accent_soft: with_alpha(agent_accent_hex, 0.14),
                    agent_accent_border: with_alpha(agent_accent_hex, 0.34),
                    side_pane_bg: mix_rgba(rgb(agent_tint_hex), rgb(0xfcfcfc), 0.06),
                }
            }
            ThemeMode::Dark => {
                let agent_accent_hex = preset.dark_accent;
                let agent_tint_hex = preset.dark_tint;
                Self {
                    mode,
                    bg_pane: rgb(0x161616).into(),
                    bg_chrome: rgb(0x121212).into(),
                    bg_sidebar: rgb(0x0e0e0e).into(),
                    bg_elevated: rgb(0x1f1f1f).into(),
                    bg_elevated_hover: rgb(0x292929).into(),
                    bg_scrim: rgba(0x000000a6).into(),
                    bg_hover: with_alpha(0xf5f5f5, 0.06),
                    bg_active: with_alpha(0xf5f5f5, 0.11),
                    bg_primary: rgb(0xf5f5f5).into(),
                    bg_primary_hover: rgb(0xdbdbdb).into(),
                    bg_secondary: with_alpha(0xf5f5f5, 0.07),
                    bg_secondary_hover: with_alpha(0xf5f5f5, 0.12),
                    bg_tertiary: with_alpha(0xf5f5f5, 0.06),
                    bg_card: with_alpha(0xf5f5f5, 0.06),
                    text_primary: rgb(0xf5f5f5).into(),
                    text_secondary: with_alpha(0xf5f5f5, 0.68),
                    text_tertiary: with_alpha(0xf5f5f5, 0.48),
                    text_quaternary: with_alpha(0xf5f5f5, 0.34),
                    text_on_primary: rgb(0x121212).into(),
                    border_primary: with_alpha(0xf5f5f5, 0.22),
                    border_default: with_alpha(0xf5f5f5, 0.11),
                    border_subtle: with_alpha(0xf5f5f5, 0.075),
                    border_hairline: with_alpha(0xf5f5f5, 0.05),
                    sidebar_text_primary: with_alpha(0xf5f5f5, 0.94),
                    sidebar_text_secondary: with_alpha(0xf5f5f5, 0.56),
                    sidebar_text_tertiary: with_alpha(0xf5f5f5, 0.36),
                    sidebar_separator: with_alpha(0xf5f5f5, 0.09),
                    sidebar_hover: with_alpha(0xf5f5f5, 0.055),
                    sidebar_selected: with_alpha(0xf5f5f5, 0.095),
                    bubble_user: rgb(0xecece8).into(),
                    bubble_user_ink: rgb(0x121212).into(),
                    bubble_agent: with_alpha(0xf5f5f5, 0.065),
                    accent_primary: rgb(0x1884fc).into(),
                    accent_soft: with_alpha(0x1884fc, 0.18),
                    accent_text: rgb(0x4d9bff).into(),
                    danger_primary: rgb(0xcf2b35).into(),
                    danger_soft: with_alpha(0xfc2c3e, 0.16),
                    danger_text: rgb(0xff6463).into(),
                    success_primary: rgb(0x0e9f5a).into(),
                    success_soft: with_alpha(0x14c370, 0.15),
                    success_text: rgb(0x29cb78).into(),
                    warn_primary: rgb(0xc8770c).into(),
                    warn_soft: with_alpha(0xf69412, 0.16),
                    warn_text: rgb(0xfb9500).into(),
                    agent_accent: rgb(agent_accent_hex).into(),
                    agent_accent_soft: with_alpha(agent_accent_hex, 0.14),
                    agent_accent_border: with_alpha(agent_accent_hex, 0.34),
                    side_pane_bg: mix_rgba(rgb(agent_tint_hex), rgb(0x121212), 0.06),
                }
            }
        }
    }

    pub fn dataviz_color(&self, idx: usize) -> Hsla {
        let pal = match self.mode {
            ThemeMode::Light => DATAVIZ_LIGHT,
            ThemeMode::Dark => DATAVIZ_DARK,
        };
        rgb(pal[idx % pal.len()]).into()
    }

    pub fn bot_ink(&self, seed: &str, preferred: Option<&str>) -> Hsla {
        let preset = resolve_bot_color(seed, preferred);
        if preset.id == "black" {
            self.text_primary
        } else {
            rgb(preset.hex).into()
        }
    }

    pub fn transparent() -> Hsla {
        hsla(0.0, 0.0, 0.0, 0.0)
    }
}
