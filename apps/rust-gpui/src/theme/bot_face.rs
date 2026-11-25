use crate::theme::tokens::SandTheme;
use gpui::{div, prelude::*, px, svg, Hsla, IntoElement, SharedString};

pub const ALL_GROK_BOT_SHAPES: &[(&str, &str)] = &[
    ("blob", "Blob"),
    ("pebble", "Pebble"),
    ("bean", "Bean"),
    ("egg", "Egg"),
    ("squircle", "Squircle"),
    ("tablet", "Tablet"),
    ("capsule", "Capsule"),
    ("cylinder", "Cylinder"),
    ("hex", "Hex"),
    ("gem", "Gem"),
    ("crystal", "Crystal"),
    ("wedge", "Wedge"),
    ("shield", "Shield"),
    ("dome", "Dome"),
    ("arch", "Arch"),
    ("cloud", "Cloud"),
    ("teardrop", "Teardrop"),
    ("leaf", "Leaf"),
    ("square", "Square"),
    ("sparkle", "Sparkle"),
    ("clover", "Clover"),
    ("flower", "Flower"),
    ("heart", "Heart"),
    ("house", "House"),
    ("star", "Star"),
    ("pumpkin", "Pumpkin"),
];

const DEFAULT_SHAPES: &[&str] = &[
    "blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop",
];

pub fn resolve_bot_shape(shape_id: Option<&str>, seed: &str) -> &'static str {
    if let Some(id) = shape_id {
        if let Some((found, _)) = ALL_GROK_BOT_SHAPES.iter().find(|(s, _)| *s == id) {
            return found;
        }
    }
    let mut h: u32 = 2166136261;
    for b in seed.encode_utf16() {
        h ^= b as u32;
        h = h.wrapping_mul(16777619);
    }
    h = (h ^ (h >> 16)).wrapping_mul(73244475);
    h = (h ^ (h >> 13)).wrapping_mul(3266489909);
    let idx = ((h ^ (h >> 16)) as usize) % DEFAULT_SHAPES.len();
    DEFAULT_SHAPES[idx]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotFaceState {
    Idle,
    Thinking,
    Streaming,
    Searching,
    Reading,
    Writing,
    Coding,
    Running,
    Computing,
    Sending,
    Progress,
    Orbit,
    Waiting,
    Working,
    Blocked,
    Finished,
    Failed,
    Sleeping,
    Happy,
    Celebrate,
    #[allow(dead_code)]
    Excited,
}

impl BotFaceState {
    #[allow(dead_code)]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Thinking => "thinking",
            Self::Streaming => "streaming",
            Self::Searching => "searching",
            Self::Reading => "reading",
            Self::Writing => "writing",
            Self::Coding => "coding",
            Self::Running => "running",
            Self::Computing => "computing",
            Self::Sending => "sending",
            Self::Progress => "progress",
            Self::Orbit => "orbit",
            Self::Waiting => "waiting",
            Self::Working => "working",
            Self::Blocked => "blocked",
            Self::Finished => "finished",
            Self::Failed => "failed",
            Self::Sleeping => "sleeping",
            Self::Happy => "happy",
            Self::Celebrate => "celebrate",
            Self::Excited => "excited",
        }
    }

    pub fn morph_prefix(self) -> Option<&'static str> {
        match self {
            Self::Thinking => Some("thinking"),
            Self::Searching => Some("searching"),
            Self::Writing => Some("writing"),
            Self::Orbit | Self::Waiting => Some("orbit"),
            Self::Progress => Some("progress"),
            Self::Sending => Some("sending"),
            _ => None,
        }
    }
}

pub fn classify_hermes_activity(tool_name: Option<&str>, status_text: Option<&str>) -> BotFaceState {
    let raw = format!(
        "{} {}",
        tool_name.unwrap_or(""),
        status_text.unwrap_or("")
    )
    .replace(['_', '-'], " ")
    .to_lowercase();
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return BotFaceState::Thinking;
    }

    let has_any = |words: &[&str]| words.iter().any(|w| trimmed.contains(w));

    if has_any(&[
        "write", "edit", "patch", "replace", "create file", "save file", "update file",
        "insert", "apply diff", "draft", "memo", "soul",
    ]) {
        return BotFaceState::Writing;
    }
    if has_any(&["search", "grep", "glob", "find", "ripgrep", "tavily", "lookup", "query"]) {
        return BotFaceState::Searching;
    }
    if has_any(&["read", "view", "fetch", "browse", "open url", "list dir", "inspect", "scrape"]) {
        return BotFaceState::Reading;
    }
    if has_any(&["terminal", "shell", "bash", "zsh", "exec", "command", "process", "ssh", "cli"]) {
        return BotFaceState::Running;
    }
    if has_any(&["code", "python", "node", "eval", "compile", "test", "lint", "git", "debug"]) {
        return BotFaceState::Coding;
    }
    if has_any(&["computer", "screenshot", "browser", "click", "mouse", "vision", "desktop"]) {
        return BotFaceState::Computing;
    }
    if has_any(&["generate", "image", "video", "audio", "tts", "render", "diagram", "chart"]) {
        return BotFaceState::Progress;
    }
    if has_any(&["send", "message", "email", "slack", "telegram", "discord", "webhook", "notify"]) {
        return BotFaceState::Sending;
    }
    if has_any(&["connect", "mcp", "login", "auth", "sync", "oauth"]) {
        return BotFaceState::Orbit;
    }
    if has_any(&["wait", "await", "sleep", "delegate", "subagent", "cron", "schedule", "poll"]) {
        return BotFaceState::Waiting;
    }
    if has_any(&["think", "reason", "plan", "analyz"]) {
        return BotFaceState::Thinking;
    }
    if tool_name.is_some() {
        BotFaceState::Working
    } else {
        BotFaceState::Thinking
    }
}

/// Renders an authentic Grok Bot 1.2.3 cutout SVG face (all 26 shapes + live morph states),
/// tinted in the exact bot color on the GPU.
pub fn bot_face(
    theme: &SandTheme,
    seed: &str,
    shape_id: Option<&str>,
    color_id: Option<&str>,
    size_px: f32,
    state: BotFaceState,
    anim_tick: u64,
) -> impl IntoElement {
    let shape = resolve_bot_shape(shape_id, seed);
    let ink: Hsla = if state == BotFaceState::Sleeping {
        theme.bot_ink(seed, Some("gray"))
    } else {
        theme.bot_ink(seed, color_id)
    };

    // Use authentic morph SVGs for active morph states, otherwise the bot's shape SVG.
    let asset_path: SharedString = if let Some(prefix) = state.morph_prefix() {
        let frame = anim_tick % 4;
        SharedString::from(format!("states/{prefix}_{frame}.svg"))
    } else {
        SharedString::from(format!("shapes/{shape}.svg"))
    };

    // Match Grok Bot's .gb-animated-face__renderer 153.33% canvas scale so the inner body
    // occupies the exact visual diameter as in the Electron/Web renderer!
    let render_px = size_px * 1.5333;

    // Gentle vertical bob for live active / celebrate states
    let bob_y = match state {
        BotFaceState::Idle | BotFaceState::Sleeping => 0.0,
        BotFaceState::Celebrate | BotFaceState::Finished | BotFaceState::Excited => {
            ((anim_tick as f32) * 0.9).sin() * 2.5
        }
        _ => ((anim_tick as f32) * 0.6).sin() * 1.5,
    };

    div()
        .w(px(size_px))
        .h(px(size_px))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .relative()
        .child(
            svg()
                .path(asset_path)
                .w(px(render_px))
                .h(px(render_px))
                .flex_shrink_0()
                .mt(px(bob_y))
                .text_color(ink),
        )
}

/// Describe a tool call identically to `apps/web/src/lib/toolActivity.ts`.
pub fn describe_tool(name: &str, arguments: Option<&serde_json::Value>, preview: Option<&str>, running: bool) -> (&'static str, &'static str, String, BotFaceState) {
    let state = classify_hermes_activity(Some(name), None);
    let (icon, active, past) = match state {
        BotFaceState::Writing => ("pencil", "Writing", "Wrote"),
        BotFaceState::Reading => ("book-open", "Reading", "Read"),
        BotFaceState::Searching => ("search", "Searching", "Searched"),
        BotFaceState::Running => ("terminal", "Running", "Ran"),
        BotFaceState::Coding => ("code", "Working on", "Worked on"),
        BotFaceState::Computing => ("globe", "Using", "Used"),
        BotFaceState::Sending => ("send", "Sending", "Sent"),
        _ => ("wrench", "Using", "Used"),
    };

    let mut detail_opt: Option<String> = None;
    let extract_obj = |obj: &serde_json::Map<String, serde_json::Value>| -> Option<String> {
        for key in ["command", "cmd", "path", "file_path", "filename", "query", "url"] {
            if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
                let t = s.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
        None
    };

    if let Some(args) = arguments {
        if let Some(obj) = args.as_object() {
            detail_opt = extract_obj(obj);
        } else if let Some(s) = args.as_str() {
            if s.len() < 8192 {
                if let Ok(serde_json::Value::Object(obj)) = serde_json::from_str(s) {
                    detail_opt = extract_obj(&obj);
                }
            }
        }
    }
    if detail_opt.is_none() {
        detail_opt = preview
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from);
    }

    let detail = detail_opt
        .unwrap_or_else(|| name.to_string())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(240)
        .collect::<String>();

    let label = if running { active } else { past };
    (icon, label, detail, state)
}
