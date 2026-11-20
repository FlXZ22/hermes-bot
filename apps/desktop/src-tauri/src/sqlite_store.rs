use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSessionRow {
    pub id: String,
    pub title: Option<String>,
    pub model: Option<String>,
    pub source: Option<String>,
    pub started_at: Option<f64>,
    pub ended_at: Option<f64>,
    pub message_count: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub preview: Option<String>,
    pub cwd: Option<String>,
    pub pinned: bool,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalMessageRow {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: Option<f64>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalProjectRow {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub board_slug: Option<String>,
    pub primary_path: Option<String>,
    pub created_at: i64,
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModelItem {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub configured: bool,
    pub context_length: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSkillItem {
    pub name: String,
    pub category: String,
    pub path: String,
    pub description: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalBootstrapPayload {
    pub active_model: Option<String>,
    pub active_provider: Option<String>,
    pub base_url: Option<String>,
    pub reasoning_effort: Option<String>,
    pub sessions: Vec<LocalSessionRow>,
    pub projects: Vec<LocalProjectRow>,
    pub models: Vec<LocalModelItem>,
    pub skills: Vec<LocalSkillItem>,
    pub soul_md: String,
    pub memory_md: String,
    pub user_md: String,
    pub board_state: Value,
    pub elapsed_ms: u64,
}

fn open_ro(path: &Path) -> Option<Connection> {
    if !path.is_file() {
        return None;
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags).ok()?;
    let _ = conn.busy_timeout(std::time::Duration::from_millis(800));
    Some(conn)
}

fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut cols = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&format!("PRAGMA table_info({table})")) {
        if let Ok(rows) = stmt.query_map([], |r| r.get::<_, String>(1)) {
            for col in rows.flatten() {
                cols.push(col);
            }
        }
    }
    cols
}

pub fn load_local_sessions(hermes_home: &Path, limit: usize) -> Vec<LocalSessionRow> {
    let db_path = hermes_home.join("state.db");
    let Some(conn) = open_ro(&db_path) else {
        return vec![];
    };

    let cols = table_columns(&conn, "sessions");
    if cols.is_empty() {
        return vec![];
    }

    let has_col = |name: &str| cols.iter().any(|c| c == name);
    let sel_title = if has_col("title") { "s.title" } else { "NULL" };
    let sel_model = if has_col("model") { "s.model" } else { "NULL" };
    let sel_source = if has_col("source") {
        "s.source"
    } else {
        "NULL"
    };
    let sel_started = if has_col("started_at") {
        "s.started_at"
    } else {
        "NULL"
    };
    let sel_ended = if has_col("ended_at") {
        "s.ended_at"
    } else {
        "NULL"
    };
    let sel_mc = if has_col("message_count") {
        "COALESCE(s.message_count, 0)"
    } else {
        "0"
    };
    let sel_in = if has_col("input_tokens") {
        "COALESCE(s.input_tokens, 0)"
    } else {
        "0"
    };
    let sel_out = if has_col("output_tokens") {
        "COALESCE(s.output_tokens, 0)"
    } else {
        "0"
    };
    let sel_cwd = if has_col("cwd") {
        "s.cwd"
    } else if has_col("workspace") {
        "s.workspace"
    } else {
        "NULL"
    };
    let sel_pinned = if has_col("pinned") {
        "COALESCE(s.pinned, 0)"
    } else {
        "0"
    };
    let sel_archived = if has_col("archived") {
        "COALESCE(s.archived, 0)"
    } else {
        "0"
    };
    let order_col = if has_col("last_activity_at") {
        "COALESCE(s.last_activity_at, s.started_at, 0)"
    } else if has_col("started_at") {
        "COALESCE(s.started_at, 0)"
    } else {
        "s.rowid"
    };

    let sql = format!(
        "SELECT s.id, {sel_title}, {sel_model}, {sel_source}, {sel_started}, {sel_ended}, \
         {sel_mc}, {sel_in}, {sel_out}, {sel_cwd}, {sel_pinned}, {sel_archived}, \
         (SELECT SUBSTR(m.content, 1, 140) FROM messages m WHERE m.session_id = s.id AND m.role IN ('user','assistant') AND m.content IS NOT NULL AND m.content != '' ORDER BY m.id DESC LIMIT 1) \
         FROM sessions s ORDER BY {sel_pinned} DESC, {order_col} DESC LIMIT ?1"
    );

    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map([limit as i64], |row| {
            Ok(LocalSessionRow {
                id: row.get::<_, String>(0)?,
                title: row.get::<_, Option<String>>(1).ok().flatten(),
                model: row.get::<_, Option<String>>(2).ok().flatten(),
                source: row.get::<_, Option<String>>(3).ok().flatten(),
                started_at: row.get::<_, Option<f64>>(4).ok().flatten(),
                ended_at: row.get::<_, Option<f64>>(5).ok().flatten(),
                message_count: row.get::<_, i64>(6).unwrap_or(0),
                input_tokens: row.get::<_, i64>(7).unwrap_or(0),
                output_tokens: row.get::<_, i64>(8).unwrap_or(0),
                cwd: row.get::<_, Option<String>>(9).ok().flatten(),
                pinned: row.get::<_, i64>(10).unwrap_or(0) != 0,
                archived: row.get::<_, i64>(11).unwrap_or(0) != 0,
                preview: row.get::<_, Option<String>>(12).ok().flatten(),
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
    }
    out
}

pub fn load_local_session_messages(
    hermes_home: &Path,
    session_id: &str,
    limit: usize,
) -> Vec<LocalMessageRow> {
    let db_path = hermes_home.join("state.db");
    let Some(conn) = open_ro(&db_path) else {
        return vec![];
    };

    let cols = table_columns(&conn, "messages");
    if cols.is_empty() {
        return vec![];
    }

    let has_col = |name: &str| cols.iter().any(|c| c == name);
    let sel_ts = if has_col("timestamp") {
        "timestamp"
    } else if has_col("created_at") {
        "created_at"
    } else {
        "NULL"
    };
    let sel_model = if has_col("model") { "model" } else { "NULL" };

    let sql = format!(
        "SELECT CAST(id AS TEXT), role, COALESCE(content, ''), {sel_ts}, {sel_model} \
         FROM (SELECT * FROM messages WHERE session_id = ?1 ORDER BY id DESC LIMIT ?2) ORDER BY id ASC"
    );

    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&sql) {
        if let Ok(rows) = stmt.query_map(rusqlite::params![session_id, limit as i64], |row| {
            Ok(LocalMessageRow {
                id: row.get::<_, String>(0)?,
                role: row
                    .get::<_, String>(1)
                    .unwrap_or_else(|_| "assistant".into()),
                content: row.get::<_, String>(2).unwrap_or_default(),
                timestamp: row.get::<_, Option<f64>>(3).ok().flatten(),
                model: row.get::<_, Option<String>>(4).ok().flatten(),
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
    }
    out
}

pub fn load_local_projects(hermes_home: &Path) -> Vec<LocalProjectRow> {
    let db_path = hermes_home.join("projects.db");
    let Some(conn) = open_ro(&db_path) else {
        return vec![];
    };

    let mut out = Vec::new();
    if let Ok(mut stmt) = conn.prepare(
        "SELECT id, slug, name, description, icon, color, board_slug, primary_path, created_at, archived \
         FROM projects WHERE archived = 0 ORDER BY created_at DESC",
    ) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok(LocalProjectRow {
                id: row.get(0)?,
                slug: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3).ok(),
                icon: row.get(4).ok(),
                color: row.get(5).ok(),
                board_slug: row.get(6).ok(),
                primary_path: row.get(7).ok(),
                created_at: row.get(8).unwrap_or(0),
                archived: row.get::<_, i64>(9).unwrap_or(0) != 0,
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
    }
    out
}

fn load_cached_models(
    hermes_home: &Path,
    active_model: &Option<String>,
    active_provider: &Option<String>,
) -> Vec<LocalModelItem> {
    let mut models = Vec::new();
    let mut seen = HashMap::<String, bool>::new();

    if let Some(m) = active_model {
        let prov = active_provider.clone().unwrap_or_else(|| "auto".into());
        seen.insert(m.clone(), true);
        models.push(LocalModelItem {
            id: m.clone(),
            name: m.clone(),
            provider: prov,
            configured: true,
            context_length: None,
        });
    }

    // Read provider_models_cache.json if present
    let prov_cache = hermes_home.join("provider_models_cache.json");
    if let Ok(raw) = fs::read_to_string(&prov_cache) {
        if let Ok(json) = serde_json::from_str::<Value>(&raw) {
            if let Some(obj) = json.as_object() {
                for (provider, entry) in obj {
                    let list = entry
                        .get("models")
                        .and_then(|v| v.as_array())
                        .or_else(|| entry.as_array());
                    if let Some(arr) = list {
                        for item in arr {
                            let id = item.as_str().map(|s| s.to_string()).or_else(|| {
                                item.get("id")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                            });
                            if let Some(id) = id {
                                if seen.insert(id.clone(), true).is_none() {
                                    let name = item
                                        .get("name")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or(&id)
                                        .to_string();
                                    let ctx = item.get("context_length").and_then(|v| v.as_u64());
                                    models.push(LocalModelItem {
                                        id,
                                        name,
                                        provider: provider.clone(),
                                        configured: true,
                                        context_length: ctx,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Curated popular Hermes models so the picker is always rich even before API keys are configured
    let defaults = [
        (
            "anthropic/claude-opus-4.6",
            "Claude Opus 4.6",
            "anthropic",
            200_000,
        ),
        (
            "anthropic/claude-sonnet-4.6",
            "Claude Sonnet 4.6",
            "anthropic",
            200_000,
        ),
        ("x-ai/grok-4", "Grok 4", "xai", 256_000),
        ("x-ai/grok-4-fast", "Grok 4 Fast", "xai", 256_000),
        ("x-ai/grok-3", "Grok 3", "xai", 131_072),
        ("openai/gpt-5.4", "GPT-5.4", "openai", 256_000),
        ("openai/o3", "o3", "openai", 200_000),
        (
            "google/gemini-2.5-pro",
            "Gemini 2.5 Pro",
            "google",
            1_000_000,
        ),
        (
            "google/gemini-2.5-flash",
            "Gemini 2.5 Flash",
            "google",
            1_000_000,
        ),
        (
            "nousresearch/hermes-4-405b",
            "Hermes 4 405B",
            "nous",
            131_072,
        ),
        ("nousresearch/hermes-4-70b", "Hermes 4 70B", "nous", 131_072),
        ("deepseek/deepseek-r1", "DeepSeek R1", "deepseek", 128_000),
        (
            "deepseek/deepseek-chat-v3",
            "DeepSeek V3",
            "deepseek",
            128_000,
        ),
    ];

    for (id, name, provider, ctx) in defaults {
        if seen.insert(id.to_string(), true).is_none() {
            models.push(LocalModelItem {
                id: id.into(),
                name: name.into(),
                provider: provider.into(),
                configured: false,
                context_length: Some(ctx),
            });
        }
    }

    models
}

fn load_local_skills(hermes_home: &Path) -> Vec<LocalSkillItem> {
    let root = hermes_home.join("skills");
    let mut skills = Vec::new();
    let mut pending = vec![root.clone()];
    let mut visited = std::collections::HashSet::new();
    while let Some(directory) = pending.pop() {
        let Ok(identity) = fs::canonicalize(&directory) else {
            continue;
        };
        if !visited.insert(identity) {
            continue;
        }
        let skill_md = directory.join("SKILL.md");
        if skill_md.is_file() {
            let raw = fs::read_to_string(&skill_md).unwrap_or_default();
            let metadata = raw
                .strip_prefix("---\n")
                .and_then(|rest| rest.split_once("\n---"))
                .and_then(|(header, _)| serde_yaml::from_str::<serde_yaml::Value>(header).ok());
            let name = directory
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let category = directory
                .parent()
                .and_then(|parent| parent.strip_prefix(&root).ok())
                .map(|parent| parent.to_string_lossy().to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "local".into());
            let description = metadata
                .as_ref()
                .and_then(|value| value.get("description"))
                .and_then(|value| value.as_str())
                .unwrap_or("Installed Hermes skill")
                .to_string();
            skills.push(LocalSkillItem {
                name,
                category,
                path: skill_md.to_string_lossy().to_string(),
                description,
                enabled: true,
            });
            continue;
        }
        if let Ok(entries) = fs::read_dir(directory) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    pending.push(entry.path());
                }
            }
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

fn board_state_path(hermes_home: &Path) -> PathBuf {
    hermes_home.join("grok_bot_boards.json")
}

pub fn load_board_state(hermes_home: &Path) -> Value {
    let path = board_state_path(hermes_home);
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            return v;
        }
    }
    serde_json::json!({
        "boards": [
            {
                "slug": "default",
                "name": "General",
                "description": "Default Hermes workspace board",
                "color": "blue",
                "workspace_path": ""
            }
        ],
        "sessionBoards": {},
        "tasks": [],
        "botAvatars": {}
    })
}

pub fn save_board_state(hermes_home: &Path, state: &Value) -> Result<(), String> {
    let path = board_state_path(hermes_home);
    let tmp = hermes_home.join("grok_bot_boards.json.tmp");
    let serialized = serde_json::to_string_pretty(state)
        .map_err(|e| format!("Failed serializing board state: {e}"))?;
    fs::write(&tmp, serialized)
        .map_err(|e| format!("Failed writing board state temp file: {e}"))?;
    fs::rename(&tmp, &path).map_err(|e| format!("Failed committing board state: {e}"))?;
    Ok(())
}

pub fn load_local_bootstrap(hermes_home: &Path) -> LocalBootstrapPayload {
    let started = std::time::Instant::now();
    let cfg_path = hermes_home.join("config.yaml");
    let mut active_model = None;
    let mut active_provider = None;
    let mut base_url = None;
    let mut reasoning_effort = None;

    if let Ok(raw) = fs::read_to_string(&cfg_path) {
        if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&raw) {
            if let Some(m) = yaml.get("model") {
                active_model = m
                    .get("default")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                active_provider = m
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                base_url = m
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
            if let Some(a) = yaml.get("agent") {
                reasoning_effort = a
                    .get("reasoning_effort")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
            }
        }
    }

    let sessions = load_local_sessions(hermes_home, 120);
    let projects = load_local_projects(hermes_home);
    let models = load_cached_models(hermes_home, &active_model, &active_provider);
    let skills = load_local_skills(hermes_home);
    let soul_md = fs::read_to_string(hermes_home.join("SOUL.md")).unwrap_or_default();
    let memory_md = fs::read_to_string(hermes_home.join("memories/MEMORY.md")).unwrap_or_default();
    let user_md = fs::read_to_string(hermes_home.join("memories/USER.md")).unwrap_or_default();
    let board_state = load_board_state(hermes_home);

    LocalBootstrapPayload {
        active_model,
        active_provider,
        base_url,
        reasoning_effort,
        sessions,
        projects,
        models,
        skills,
        soul_md,
        memory_md,
        user_md,
        board_state,
        elapsed_ms: started.elapsed().as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("hermes-ui-store-{}", rand::random::<u64>()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn transcript_hydration_reads_the_latest_page_in_conversation_order() {
        let fixture = Fixture::new();
        let db = Connection::open(fixture.0.join("state.db")).unwrap();
        db.execute_batch("CREATE TABLE messages (id INTEGER PRIMARY KEY, session_id TEXT, role TEXT, content TEXT, timestamp REAL);").unwrap();
        for id in 1..=8 {
            db.execute(
                "INSERT INTO messages VALUES (?1, 'chat', 'assistant', ?2, ?1)",
                rusqlite::params![id, format!("message {id}")],
            )
            .unwrap();
        }
        db.execute(
            "INSERT INTO messages VALUES (9, 'other-chat', 'user', 'other', 9)",
            [],
        )
        .unwrap();
        let rows = load_local_session_messages(&fixture.0, "chat", 3);
        assert_eq!(
            rows.iter()
                .map(|row| row.content.as_str())
                .collect::<Vec<_>>(),
            ["message 6", "message 7", "message 8"]
        );
        assert!(load_local_session_messages(&fixture.0, "missing", 3).is_empty());
        assert_eq!(
            db.query_row("SELECT count(*) FROM messages", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            9
        );
    }

    #[test]
    fn board_state_survives_atomic_round_trip() {
        let fixture = Fixture::new();
        let value = serde_json::json!({"boards": [{"slug": "work", "name": "Work"}], "sessionBoards": {"chat": "work"}, "tasks": [], "botAvatars": {}});
        save_board_state(&fixture.0, &value).unwrap();
        assert_eq!(load_board_state(&fixture.0), value);
        assert!(!fixture.0.join("grok_bot_boards.json.tmp").exists());
    }
}
