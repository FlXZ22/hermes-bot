use crate::hermes::types::{HermesBoardSummary, HermesKanbanTask};
use crate::theme::tokens::ThemeMode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BotAvatarOverride {
    pub shape_id: Option<String>,
    pub color_id: Option<String>,
    pub pinned: Option<bool>,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedBoardState {
    pub boards: Vec<HermesBoardSummary>,
    pub session_boards: HashMap<String, String>,
    pub tasks: Vec<HermesKanbanTask>,
    pub bot_avatars: HashMap<String, BotAvatarOverride>,
}

impl Default for PersistedBoardState {
    fn default() -> Self {
        Self {
            boards: vec![default_board()],
            session_boards: HashMap::new(),
            tasks: Vec::new(),
            bot_avatars: HashMap::new(),
        }
    }
}

pub fn default_board() -> HermesBoardSummary {
    HermesBoardSummary {
        slug: "default".into(),
        name: "General".into(),
        description: Some("Default Hermes workspace session board".into()),
        color: Some("blue".into()),
        workspace_path: Some(String::new()),
        task_count: None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiPreferences {
    pub theme: ThemeMode,
    pub right_pane_width: f32,
    pub drafts: HashMap<String, String>,
}

impl Default for UiPreferences {
    fn default() -> Self {
        Self {
            theme: ThemeMode::Dark,
            right_pane_width: 336.0,
            drafts: HashMap::new(),
        }
    }
}

fn prefs_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("hermes-grok-bot/gpui-prefs.json"))
}

pub fn load_ui_prefs() -> UiPreferences {
    let Some(path) = prefs_path() else {
        return UiPreferences::default();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_ui_prefs(prefs: &UiPreferences) {
    let Some(path) = prefs_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(prefs) {
        let _ = fs::write(path, json);
    }
}
