use crate::harness::events::{
    now_epoch_ms, ConversationMode, SessionModeBinding, SubagentNode, SubagentStatus,
    TrajectoryEvent, TrajectoryEventKind, WorkProject,
};
use crate::harness::goal_worker::{GoalEventRecord, GoalPhase, GoalRecord};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const HARNESS_SCHEMA_VERSION: i64 = 1;

/// Durable, versioned SQLite store for harness state (`~/.config/hermes-grok-bot/harness.db`).
/// Separate from Hermes's `state.db` so existing Hermes sessions and settings are preserved untouched.
#[derive(Clone)]
pub struct HarnessStore {
    path: PathBuf,
    conn: Arc<Mutex<Connection>>,
}

impl HarnessStore {
    pub fn open_default() -> Result<Self, String> {
        let base = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("hermes-grok-bot");
        Self::open_at(&base.join("harness.db"))
    }

    pub fn open_at(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed creating harness db directory: {e}"))?;
        }
        let conn = Connection::open(path)
            .map_err(|e| format!("Failed opening harness db {}: {e}", path.display()))?;
        let _ = conn.busy_timeout(std::time::Duration::from_millis(2000));
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(|e| format!("Failed configuring SQLite pragmas: {e}"))?;

        Self::migrate(&conn)?;

        Ok(Self {
            path: path.to_path_buf(),
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    #[allow(dead_code)]
    pub fn db_path(&self) -> &Path {
        &self.path
    }

    fn migrate(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS harness_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )
        .map_err(|e| format!("Failed creating harness_meta: {e}"))?;

        let current_version: i64 = conn
            .query_row(
                "SELECT value FROM harness_meta WHERE key = 'schema_version'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|e| format!("Failed reading schema_version: {e}"))?
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);

        if current_version < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    cwd TEXT NOT NULL,
                    server_id TEXT NOT NULL,
                    is_home INTEGER NOT NULL DEFAULT 0,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE UNIQUE INDEX IF NOT EXISTS idx_projects_server_cwd
                    ON projects(server_id, cwd);

                CREATE TABLE IF NOT EXISTS conversation_modes (
                    session_id TEXT PRIMARY KEY,
                    mode TEXT NOT NULL,
                    locked INTEGER NOT NULL DEFAULT 0,
                    project_id TEXT,
                    project_name TEXT,
                    cwd TEXT,
                    server_id TEXT,
                    updated_at_ms INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS trajectory_events (
                    seq INTEGER PRIMARY KEY AUTOINCREMENT,
                    event_id TEXT NOT NULL UNIQUE,
                    session_id TEXT NOT NULL,
                    run_id TEXT NOT NULL,
                    turn INTEGER NOT NULL,
                    step INTEGER NOT NULL,
                    agent_id TEXT NOT NULL,
                    parent_agent_id TEXT,
                    tool_call_id TEXT,
                    kind TEXT NOT NULL,
                    title TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    is_error INTEGER NOT NULL DEFAULT 0,
                    duration_ms INTEGER,
                    payload_json TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_trajectory_session_seq
                    ON trajectory_events(session_id, seq);

                CREATE TABLE IF NOT EXISTS goals (
                    goal_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    project_id TEXT,
                    project_name TEXT,
                    cwd TEXT,
                    objective TEXT NOT NULL,
                    phase TEXT NOT NULL,
                    revision INTEGER NOT NULL,
                    rounds_started INTEGER NOT NULL DEFAULT 0,
                    max_rounds INTEGER NOT NULL DEFAULT 256,
                    max_duration_secs INTEGER,
                    max_tokens INTEGER,
                    tokens_used INTEGER NOT NULL DEFAULT 0,
                    stop_reason TEXT,
                    blocked_code TEXT,
                    blocked_message TEXT,
                    last_summary TEXT,
                    worker_owner TEXT,
                    created_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_goals_session_updated
                    ON goals(session_id, updated_at_ms DESC);

                CREATE TABLE IF NOT EXISTS goal_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    goal_id TEXT NOT NULL,
                    revision INTEGER NOT NULL,
                    action TEXT NOT NULL,
                    phase TEXT NOT NULL,
                    detail TEXT NOT NULL,
                    created_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_goal_events_goal
                    ON goal_events(goal_id, id);

                CREATE TABLE IF NOT EXISTS subagent_runs (
                    subagent_id TEXT PRIMARY KEY,
                    session_id TEXT NOT NULL,
                    parent_agent_id TEXT NOT NULL,
                    child_session_id TEXT,
                    depth INTEGER NOT NULL DEFAULT 1,
                    goal TEXT NOT NULL,
                    model TEXT,
                    status TEXT NOT NULL,
                    current_operation TEXT NOT NULL DEFAULT '',
                    tool_count INTEGER NOT NULL DEFAULT 0,
                    input_tokens INTEGER,
                    output_tokens INTEGER,
                    duration_ms INTEGER,
                    summary TEXT,
                    started_at_ms INTEGER NOT NULL,
                    updated_at_ms INTEGER NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_subagent_runs_session
                    ON subagent_runs(session_id, started_at_ms DESC);",
            )
            .map_err(|e| format!("Failed migrating harness db to v1: {e}"))?;

            conn.execute(
                "INSERT INTO harness_meta(key, value) VALUES('schema_version', ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [HARNESS_SCHEMA_VERSION.to_string()],
            )
            .map_err(|e| format!("Failed recording schema_version: {e}"))?;
        }

        Ok(())
    }

    // ---- Projects ----

    pub fn upsert_project(&self, project: &WorkProject) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "harness db lock poisoned".to_string())?;
        conn.execute(
            "INSERT INTO projects(id, name, cwd, server_id, is_home, created_at_ms, updated_at_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(server_id, cwd) DO UPDATE SET
                 name = excluded.name,
                 is_home = excluded.is_home,
                 updated_at_ms = excluded.updated_at_ms",
            params![
                project.id,
                project.name,
                project.cwd,
                project.server_id,
                if project.is_home { 1i64 } else { 0i64 },
                project.created_at_ms,
                project.updated_at_ms,
            ],
        )
        .map_err(|e| format!("Failed saving project: {e}"))?;
        Ok(())
    }

    pub fn list_projects(&self, server_id: Option<&str>) -> Vec<WorkProject> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let query = if server_id.is_some() {
            "SELECT id, name, cwd, server_id, is_home, created_at_ms, updated_at_ms
             FROM projects WHERE server_id = ?1 ORDER BY updated_at_ms DESC, name ASC"
        } else {
            "SELECT id, name, cwd, server_id, is_home, created_at_ms, updated_at_ms
             FROM projects ORDER BY updated_at_ms DESC, name ASC"
        };
        let Ok(mut stmt) = conn.prepare(query) else {
            return out;
        };
        let map_row = |row: &rusqlite::Row<'_>| -> rusqlite::Result<WorkProject> {
            Ok(WorkProject {
                id: row.get(0)?,
                name: row.get(1)?,
                cwd: row.get(2)?,
                server_id: row.get(3)?,
                is_home: row.get::<_, i64>(4).unwrap_or(0) != 0,
                created_at_ms: row.get(5)?,
                updated_at_ms: row.get(6)?,
            })
        };
        if let Some(sid) = server_id {
            if let Ok(rows) = stmt.query_map([sid], map_row) {
                for r in rows.flatten() {
                    out.push(r);
                }
            }
        } else if let Ok(rows) = stmt.query_map([], map_row) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
        out
    }

    // ---- Conversation Mode Bindings ----

    pub fn save_mode_binding(&self, binding: &SessionModeBinding) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "harness db lock poisoned".to_string())?;
        conn.execute(
            "INSERT INTO conversation_modes(session_id, mode, locked, project_id, project_name, cwd, server_id, updated_at_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(session_id) DO UPDATE SET
                 mode = excluded.mode,
                 locked = excluded.locked,
                 project_id = COALESCE(excluded.project_id, conversation_modes.project_id),
                 project_name = COALESCE(excluded.project_name, conversation_modes.project_name),
                 cwd = COALESCE(excluded.cwd, conversation_modes.cwd),
                 server_id = COALESCE(excluded.server_id, conversation_modes.server_id),
                 updated_at_ms = excluded.updated_at_ms",
            params![
                binding.session_id,
                binding.mode.as_str(),
                if binding.locked { 1i64 } else { 0i64 },
                binding.project_id,
                binding.project_name,
                binding.cwd,
                binding.server_id,
                binding.updated_at_ms,
            ],
        )
        .map_err(|e| format!("Failed saving conversation mode: {e}"))?;
        Ok(())
    }

    pub fn load_all_mode_bindings(&self) -> Vec<SessionModeBinding> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT session_id, mode, locked, project_id, project_name, cwd, server_id, updated_at_ms
             FROM conversation_modes ORDER BY updated_at_ms DESC",
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Ok(rows) = stmt.query_map([], |row| {
            let mode_s: String = row.get(1)?;
            Ok(SessionModeBinding {
                session_id: row.get(0)?,
                mode: ConversationMode::from_str_lossy(&mode_s),
                locked: row.get::<_, i64>(2).unwrap_or(0) != 0,
                project_id: row.get(3)?,
                project_name: row.get(4)?,
                cwd: row.get(5)?,
                server_id: row.get(6)?,
                updated_at_ms: row.get(7)?,
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
        out
    }

    pub fn rekey_session_id(&self, old_id: &str, new_id: &str) -> Result<(), String> {
        if old_id == new_id || old_id.is_empty() || new_id.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().map_err(|_| "harness db lock poisoned".to_string())?;
        let existing_new: Option<String> = conn
            .query_row(
                "SELECT session_id FROM conversation_modes WHERE session_id = ?1",
                [new_id],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None);
        if existing_new.is_none() {
            let _ = conn.execute(
                "UPDATE conversation_modes SET session_id = ?1 WHERE session_id = ?2",
                params![new_id, old_id],
            );
        } else {
            let _ = conn.execute(
                "DELETE FROM conversation_modes WHERE session_id = ?1",
                [old_id],
            );
        }
        let _ = conn.execute(
            "UPDATE trajectory_events SET session_id = ?1 WHERE session_id = ?2",
            params![new_id, old_id],
        );
        let _ = conn.execute(
            "UPDATE goals SET session_id = ?1 WHERE session_id = ?2",
            params![new_id, old_id],
        );
        let _ = conn.execute(
            "UPDATE subagent_runs SET session_id = ?1 WHERE session_id = ?2",
            params![new_id, old_id],
        );
        Ok(())
    }

    // ---- Append-only Deduplicated Trajectory Events ----

    /// Insert a durable trajectory event. Returns `Ok(Some(seq))` if newly appended,
    /// or `Ok(None)` if an event with the same `event_id` was already persisted (idempotent).
    pub fn append_trajectory_event(&self, ev: &TrajectoryEvent) -> Result<Option<i64>, String> {
        let conn = self.conn.lock().map_err(|_| "harness db lock poisoned".to_string())?;
        let payload_json = serde_json::to_string(&ev.payload).unwrap_or_else(|_| "{}".into());
        let rows = conn
            .execute(
                "INSERT OR IGNORE INTO trajectory_events(
                    event_id, session_id, run_id, turn, step, agent_id, parent_agent_id,
                    tool_call_id, kind, title, summary, is_error, duration_ms, payload_json, created_at_ms
                 ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
                params![
                    ev.event_id,
                    ev.session_id,
                    ev.run_id,
                    ev.turn as i64,
                    ev.step as i64,
                    ev.agent_id,
                    ev.parent_agent_id,
                    ev.tool_call_id,
                    ev.kind.as_str(),
                    ev.title,
                    ev.summary,
                    if ev.is_error { 1i64 } else { 0i64 },
                    ev.duration_ms.map(|d| d as i64),
                    payload_json,
                    ev.created_at_ms,
                ],
            )
            .map_err(|e| format!("Failed appending trajectory event: {e}"))?;
        if rows == 0 {
            Ok(None)
        } else {
            Ok(Some(conn.last_insert_rowid()))
        }
    }

    pub fn load_trajectory_events(&self, session_id: &str, limit: usize) -> Vec<TrajectoryEvent> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT seq, event_id, session_id, run_id, turn, step, agent_id, parent_agent_id,
                    tool_call_id, kind, title, summary, is_error, duration_ms, payload_json, created_at_ms
             FROM (
                 SELECT * FROM trajectory_events WHERE session_id = ?1 ORDER BY seq DESC LIMIT ?2
             ) ORDER BY seq ASC",
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Ok(rows) = stmt.query_map(params![session_id, limit as i64], |row| {
            let kind_s: String = row.get(9)?;
            let payload_s: String = row.get(14)?;
            let payload: Value = serde_json::from_str(&payload_s).unwrap_or(Value::Null);
            Ok(TrajectoryEvent {
                seq: row.get(0)?,
                event_id: row.get(1)?,
                session_id: row.get(2)?,
                run_id: row.get(3)?,
                turn: row.get::<_, i64>(4).unwrap_or(0).max(0) as u32,
                step: row.get::<_, i64>(5).unwrap_or(0).max(0) as u32,
                agent_id: row.get(6)?,
                parent_agent_id: row.get(7)?,
                tool_call_id: row.get(8)?,
                kind: TrajectoryEventKind::from_str_lossy(&kind_s),
                title: row.get(10)?,
                summary: row.get(11)?,
                is_error: row.get::<_, i64>(12).unwrap_or(0) != 0,
                duration_ms: row.get::<_, Option<i64>>(13)?.map(|v| v.max(0) as u64),
                payload,
                created_at_ms: row.get(15)?,
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
        out
    }

    // ---- Goals ----

    pub fn save_goal(&self, goal: &GoalRecord, event: Option<&GoalEventRecord>) -> Result<(), String> {
        let mut conn = self.conn.lock().map_err(|_| "harness db lock poisoned".to_string())?;
        let tx = conn
            .transaction()
            .map_err(|e| format!("Failed starting goal transaction: {e}"))?;
        tx.execute(
            "INSERT INTO goals(
                goal_id, session_id, project_id, project_name, cwd, objective, phase, revision,
                rounds_started, max_rounds, max_duration_secs, max_tokens, tokens_used,
                stop_reason, blocked_code, blocked_message, last_summary, worker_owner,
                created_at_ms, updated_at_ms
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
             ON CONFLICT(goal_id) DO UPDATE SET
                session_id = excluded.session_id,
                project_id = excluded.project_id,
                project_name = excluded.project_name,
                cwd = excluded.cwd,
                objective = excluded.objective,
                phase = excluded.phase,
                revision = excluded.revision,
                rounds_started = excluded.rounds_started,
                max_rounds = excluded.max_rounds,
                max_duration_secs = excluded.max_duration_secs,
                max_tokens = excluded.max_tokens,
                tokens_used = excluded.tokens_used,
                stop_reason = excluded.stop_reason,
                blocked_code = excluded.blocked_code,
                blocked_message = excluded.blocked_message,
                last_summary = excluded.last_summary,
                worker_owner = excluded.worker_owner,
                updated_at_ms = excluded.updated_at_ms",
            params![
                goal.goal_id,
                goal.session_id,
                goal.project_id,
                goal.project_name,
                goal.cwd,
                goal.objective,
                goal.phase.as_str(),
                goal.revision as i64,
                goal.rounds_started as i64,
                goal.max_rounds as i64,
                goal.max_duration_secs.map(|v| v as i64),
                goal.max_tokens.map(|v| v as i64),
                goal.tokens_used as i64,
                goal.stop_reason,
                goal.blocked_code,
                goal.blocked_message,
                goal.last_summary,
                goal.worker_owner,
                goal.created_at_ms,
                goal.updated_at_ms,
            ],
        )
        .map_err(|e| format!("Failed persisting goal: {e}"))?;

        if let Some(ev) = event {
            tx.execute(
                "INSERT INTO goal_events(goal_id, revision, action, phase, detail, created_at_ms)
                 VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    ev.goal_id,
                    ev.revision as i64,
                    ev.action,
                    ev.phase.as_str(),
                    ev.detail,
                    ev.created_at_ms,
                ],
            )
            .map_err(|e| format!("Failed persisting goal event: {e}"))?;
        }
        tx.commit().map_err(|e| format!("Failed committing goal tx: {e}"))?;
        Ok(())
    }

    pub fn load_all_goals(&self) -> Vec<GoalRecord> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT goal_id, session_id, project_id, project_name, cwd, objective, phase, revision,
                    rounds_started, max_rounds, max_duration_secs, max_tokens, tokens_used,
                    stop_reason, blocked_code, blocked_message, last_summary, worker_owner,
                    created_at_ms, updated_at_ms
             FROM goals ORDER BY updated_at_ms DESC",
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Ok(rows) = stmt.query_map([], |row| {
            let phase_s: String = row.get(6)?;
            Ok(GoalRecord {
                goal_id: row.get(0)?,
                session_id: row.get(1)?,
                project_id: row.get(2)?,
                project_name: row.get(3)?,
                cwd: row.get(4)?,
                objective: row.get(5)?,
                phase: GoalPhase::from_str_lossy(&phase_s),
                revision: row.get::<_, i64>(7).unwrap_or(1).max(1) as u64,
                rounds_started: row.get::<_, i64>(8).unwrap_or(0).max(0) as u32,
                max_rounds: row.get::<_, i64>(9).unwrap_or(256).max(1) as u32,
                max_duration_secs: row.get::<_, Option<i64>>(10)?.map(|v| v.max(1) as u64),
                max_tokens: row.get::<_, Option<i64>>(11)?.map(|v| v.max(1) as u64),
                tokens_used: row.get::<_, i64>(12).unwrap_or(0).max(0) as u64,
                stop_reason: row.get(13)?,
                blocked_code: row.get(14)?,
                blocked_message: row.get(15)?,
                last_summary: row.get(16)?,
                worker_owner: row.get(17)?,
                created_at_ms: row.get(18)?,
                updated_at_ms: row.get(19)?,
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
        out
    }

    pub fn load_goal_events(&self, goal_id: &str) -> Vec<GoalEventRecord> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT goal_id, revision, action, phase, detail, created_at_ms
             FROM goal_events WHERE goal_id = ?1 ORDER BY id ASC",
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Ok(rows) = stmt.query_map([goal_id], |row| {
            let phase_s: String = row.get(3)?;
            Ok(GoalEventRecord {
                goal_id: row.get(0)?,
                revision: row.get::<_, i64>(1).unwrap_or(1).max(1) as u64,
                action: row.get(2)?,
                phase: GoalPhase::from_str_lossy(&phase_s),
                detail: row.get(4)?,
                created_at_ms: row.get(5)?,
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
        out
    }

    // ---- Subagent Runs ----

    pub fn upsert_subagent(&self, node: &SubagentNode) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|_| "harness db lock poisoned".to_string())?;
        conn.execute(
            "INSERT INTO subagent_runs(
                subagent_id, session_id, parent_agent_id, child_session_id, depth, goal, model,
                status, current_operation, tool_count, input_tokens, output_tokens, duration_ms,
                summary, started_at_ms, updated_at_ms
             ) VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
             ON CONFLICT(subagent_id) DO UPDATE SET
                child_session_id = COALESCE(excluded.child_session_id, subagent_runs.child_session_id),
                model = COALESCE(excluded.model, subagent_runs.model),
                status = excluded.status,
                current_operation = excluded.current_operation,
                tool_count = excluded.tool_count,
                input_tokens = COALESCE(excluded.input_tokens, subagent_runs.input_tokens),
                output_tokens = COALESCE(excluded.output_tokens, subagent_runs.output_tokens),
                duration_ms = COALESCE(excluded.duration_ms, subagent_runs.duration_ms),
                summary = COALESCE(excluded.summary, subagent_runs.summary),
                updated_at_ms = excluded.updated_at_ms",
            params![
                node.subagent_id,
                node.session_id,
                node.parent_agent_id,
                node.child_session_id,
                node.depth as i64,
                node.goal,
                node.model,
                node.status.as_str(),
                node.current_operation,
                node.tool_count as i64,
                node.input_tokens.map(|v| v as i64),
                node.output_tokens.map(|v| v as i64),
                node.duration_ms.map(|v| v as i64),
                node.summary,
                node.started_at_ms,
                node.updated_at_ms,
            ],
        )
        .map_err(|e| format!("Failed saving subagent run: {e}"))?;
        Ok(())
    }

    pub fn load_subagents(&self, session_id: &str) -> Vec<SubagentNode> {
        let Ok(conn) = self.conn.lock() else {
            return Vec::new();
        };
        let Ok(mut stmt) = conn.prepare(
            "SELECT subagent_id, session_id, parent_agent_id, child_session_id, depth, goal, model,
                    status, current_operation, tool_count, input_tokens, output_tokens, duration_ms,
                    summary, started_at_ms, updated_at_ms
             FROM subagent_runs WHERE session_id = ?1 ORDER BY started_at_ms DESC",
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Ok(rows) = stmt.query_map([session_id], |row| {
            let status_s: String = row.get(7)?;
            Ok(SubagentNode {
                subagent_id: row.get(0)?,
                session_id: row.get(1)?,
                parent_agent_id: row.get(2)?,
                child_session_id: row.get(3)?,
                depth: row.get::<_, i64>(4).unwrap_or(1).max(0) as u32,
                goal: row.get(5)?,
                model: row.get(6)?,
                status: SubagentStatus::from_str_lossy(&status_s),
                current_operation: row.get(8)?,
                tool_count: row.get::<_, i64>(9).unwrap_or(0).max(0) as u64,
                input_tokens: row.get::<_, Option<i64>>(10)?.map(|v| v.max(0) as u64),
                output_tokens: row.get::<_, Option<i64>>(11)?.map(|v| v.max(0) as u64),
                duration_ms: row.get::<_, Option<i64>>(12)?.map(|v| v.max(0) as u64),
                summary: row.get(13)?,
                started_at_ms: row.get(14)?,
                updated_at_ms: row.get(15)?,
            })
        }) {
            for r in rows.flatten() {
                out.push(r);
            }
        }
        out
    }

    pub fn mark_running_subagents_interrupted_on_recovery(&self) {
        let Ok(conn) = self.conn.lock() else {
            return;
        };
        let _ = conn.execute(
            "UPDATE subagent_runs SET status = 'interrupted', current_operation = 'Disconnected during previous session', updated_at_ms = ?1
             WHERE status = 'running'",
            [now_epoch_ms()],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn store_persists_mode_lock_projects_and_deduplicates_trajectory_events() {
        let dir = std::env::temp_dir().join(format!("hermes-harness-test-{}", uuid::Uuid::new_v4()));
        let db = dir.join("harness.db");
        let store = HarnessStore::open_at(&db).expect("open store");

        let proj = WorkProject {
            id: "proj-1".into(),
            name: "Grok Bot".into(),
            cwd: "/tmp/example".into(),
            server_id: "local-rust".into(),
            is_home: false,
            created_at_ms: 1000,
            updated_at_ms: 1000,
        };
        store.upsert_project(&proj).expect("save project");
        assert_eq!(store.list_projects(Some("local-rust")).len(), 1);

        let binding = SessionModeBinding {
            session_id: "sess-1".into(),
            mode: ConversationMode::Work,
            locked: true,
            project_id: Some("proj-1".into()),
            project_name: Some("Grok Bot".into()),
            cwd: Some("/tmp/example".into()),
            server_id: Some("local-rust".into()),
            updated_at_ms: 2000,
        };
        store.save_mode_binding(&binding).expect("save mode");

        let ev = TrajectoryEvent {
            seq: 0,
            event_id: "ev-dedup-1".into(),
            session_id: "sess-1".into(),
            run_id: "run-1".into(),
            turn: 1,
            step: 1,
            agent_id: "lead".into(),
            parent_agent_id: None,
            tool_call_id: Some("tc-1".into()),
            kind: TrajectoryEventKind::ToolCallStart,
            title: "read_file".into(),
            summary: "Read src/main.rs".into(),
            is_error: false,
            duration_ms: None,
            payload: json!({"path": "src/main.rs"}),
            created_at_ms: 2100,
        };
        assert!(store.append_trajectory_event(&ev).expect("first append").is_some());
        // Duplicate delivery with same event_id must be ignored
        assert!(store.append_trajectory_event(&ev).expect("dup append").is_none());

        // Reopen to verify durability across restarts
        drop(store);
        let reopened = HarnessStore::open_at(&db).expect("reopen store");
        let loaded_modes = reopened.load_all_mode_bindings();
        assert_eq!(loaded_modes.len(), 1);
        assert_eq!(loaded_modes[0].mode, ConversationMode::Work);
        assert!(loaded_modes[0].locked);
        assert_eq!(reopened.load_trajectory_events("sess-1", 50).len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
