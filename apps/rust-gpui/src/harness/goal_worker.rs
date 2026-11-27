use crate::harness::events::now_epoch_ms;
use crate::harness::store::HarnessStore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const DEFAULT_MAX_GOAL_ROUNDS: u32 = 256;

/// Explicit durable goal phases with strict CAS state transition rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GoalPhase {
    Active,
    Paused,
    Blocked,
    Complete,
    Cancelled,
    Failed,
}

impl GoalPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::Complete => "complete",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "active" => Self::Active,
            "paused" => Self::Paused,
            "blocked" => Self::Blocked,
            "complete" | "completed" => Self::Complete,
            "cancelled" | "canceled" => Self::Cancelled,
            "failed" | "error" => Self::Failed,
            _ => Self::Paused,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Complete | Self::Cancelled | Self::Failed)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::Paused => "Paused",
            Self::Blocked => "Blocked",
            Self::Complete => "Complete",
            Self::Cancelled => "Cancelled",
            Self::Failed => "Failed",
        }
    }
}

/// Process-local continuation arming state (NEVER persisted — always starts `Disarmed`
/// on cold restart/reconnect so the app never launches duplicate unattended runs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum GoalActivation {
    Armed,
    #[default]
    Disarmed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalRecord {
    pub goal_id: String,
    pub session_id: String,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub cwd: Option<String>,
    pub objective: String,
    pub phase: GoalPhase,
    pub revision: u64,
    pub rounds_started: u32,
    pub max_rounds: u32,
    pub max_duration_secs: Option<u64>,
    pub max_tokens: Option<u64>,
    pub tokens_used: u64,
    pub stop_reason: Option<String>,
    pub blocked_code: Option<String>,
    pub blocked_message: Option<String>,
    pub last_summary: Option<String>,
    pub worker_owner: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalEventRecord {
    pub goal_id: String,
    pub revision: u64,
    pub action: String,
    pub phase: GoalPhase,
    pub detail: String,
    pub created_at_ms: i64,
}

/// Live worker status snapshot exposed to the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalRuntimeSnapshot {
    pub goal: GoalRecord,
    pub activation: GoalActivation,
    pub worker_connected: bool,
    pub recovered_from_restart: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalCommand {
    Status,
    Create {
        objective: String,
        max_rounds: Option<u32>,
        max_duration_secs: Option<u64>,
        max_tokens: Option<u64>,
    },
    Pause,
    Resume,
    Cancel,
    Complete(Option<String>),
}

/// Parse `/goal ...` command syntax, supporting optional `--max-rounds N`, `--max-minutes M`, `--max-tokens T`.
pub fn parse_goal_slash_command(raw_input: &str) -> Option<GoalCommand> {
    let trimmed = raw_input.trim();
    let rest = if trimmed == "/goal" {
        ""
    } else if let Some(after) = trimmed.strip_prefix("/goal ") {
        after.trim()
    } else {
        return None;
    };

    if rest.is_empty() || rest.eq_ignore_ascii_case("status") || rest.eq_ignore_ascii_case("show") {
        return Some(GoalCommand::Status);
    }
    if rest.eq_ignore_ascii_case("pause") {
        return Some(GoalCommand::Pause);
    }
    if rest.eq_ignore_ascii_case("resume") {
        return Some(GoalCommand::Resume);
    }
    if rest.eq_ignore_ascii_case("cancel")
        || rest.eq_ignore_ascii_case("stop")
        || rest.eq_ignore_ascii_case("clear")
    {
        return Some(GoalCommand::Cancel);
    }
    if rest.eq_ignore_ascii_case("complete") || rest.eq_ignore_ascii_case("done") {
        return Some(GoalCommand::Complete(None));
    }
    if let Some(summary) = rest.strip_prefix("complete ").or_else(|| rest.strip_prefix("done ")) {
        return Some(GoalCommand::Complete(Some(summary.trim().to_string())));
    }

    let mut max_rounds = None;
    let mut max_duration_secs = None;
    let mut max_tokens = None;
    let mut words = Vec::new();
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    let mut i = 0;
    while i < tokens.len() {
        match tokens[i] {
            "--max-rounds" | "--rounds" if i + 1 < tokens.len() => {
                max_rounds = tokens[i + 1].parse::<u32>().ok().filter(|v| *v > 0);
                i += 2;
            }
            "--max-minutes" | "--minutes" if i + 1 < tokens.len() => {
                max_duration_secs = tokens[i + 1]
                    .parse::<u64>()
                    .ok()
                    .filter(|v| *v > 0)
                    .map(|m| m * 60);
                i += 2;
            }
            "--max-tokens" | "--tokens" if i + 1 < tokens.len() => {
                max_tokens = tokens[i + 1].parse::<u64>().ok().filter(|v| *v > 0);
                i += 2;
            }
            other => {
                words.push(other);
                i += 1;
            }
        }
    }
    let objective = words.join(" ").trim().to_string();
    if objective.is_empty() {
        Some(GoalCommand::Status)
    } else {
        Some(GoalCommand::Create {
            objective,
            max_rounds,
            max_duration_secs,
            max_tokens,
        })
    }
}

struct WorkerState {
    worker_id: String,
    goals_by_session: HashMap<String, GoalRecord>,
    activation: HashMap<String, GoalActivation>,
    recovered_sessions: HashMap<String, bool>,
    in_flight_rounds: HashMap<String, u32>,
    worker_connected: bool,
}

/// Persistent background goal coordinator independent of UI views.
///
/// Lifecycle & recovery contract:
/// - Goals continue running when switching conversations or minimizing the app.
/// - Every state transition enforces compare-and-set (`expected_revision`) and writes
///   durably to `HarnessStore` (`harness.db`).
/// - On cold restart or reconnect, persisted goals are restored in `GoalActivation::Disarmed`
///   state so the app NEVER launches duplicate unattended runs; the user explicitly resumes
///   or continues them.
#[derive(Clone)]
pub struct GoalWorker {
    store: HarnessStore,
    inner: Arc<Mutex<WorkerState>>,
}

impl GoalWorker {
    pub fn new(store: HarnessStore) -> Self {
        let worker_id = format!("rust-goal-worker-{}", std::process::id());
        let mut goals_by_session: HashMap<String, GoalRecord> = HashMap::new();
        let mut activation = HashMap::new();
        let mut recovered_sessions = HashMap::new();

        for goal in store.load_all_goals() {
            // Keep the newest goal per session; all restored goals start Disarmed to prevent duplicate runs.
            let sid = goal.session_id.clone();
            if !goals_by_session.contains_key(&sid) {
                let was_active = goal.phase == GoalPhase::Active;
                goals_by_session.insert(sid.clone(), goal);
                activation.insert(sid.clone(), GoalActivation::Disarmed);
                if was_active {
                    recovered_sessions.insert(sid, true);
                }
            }
        }

        Self {
            store,
            inner: Arc::new(Mutex::new(WorkerState {
                worker_id,
                goals_by_session,
                activation,
                recovered_sessions,
                in_flight_rounds: HashMap::new(),
                worker_connected: false,
            })),
        }
    }

    pub fn set_worker_connected(&self, connected: bool) {
        if let Ok(mut st) = self.inner.lock() {
            st.worker_connected = connected;
            if !connected {
                // Disarm all goals when backend connection drops so reconnect never duplicates runs.
                for act in st.activation.values_mut() {
                    *act = GoalActivation::Disarmed;
                }
                st.in_flight_rounds.clear();
            }
        }
    }

    #[allow(dead_code)]
    pub fn is_worker_connected(&self) -> bool {
        self.inner.lock().map(|s| s.worker_connected).unwrap_or(false)
    }

    pub fn snapshot_for_session(&self, session_id: &str) -> Option<GoalRuntimeSnapshot> {
        let st = self.inner.lock().ok()?;
        let goal = st.goals_by_session.get(session_id)?.clone();
        let activation = st
            .activation
            .get(session_id)
            .copied()
            .unwrap_or(GoalActivation::Disarmed);
        let recovered = st.recovered_sessions.get(session_id).copied().unwrap_or(false);
        Some(GoalRuntimeSnapshot {
            goal,
            activation,
            worker_connected: st.worker_connected,
            recovered_from_restart: recovered,
        })
    }

    #[allow(dead_code)]
    pub fn all_snapshots(&self) -> Vec<GoalRuntimeSnapshot> {
        let Ok(st) = self.inner.lock() else {
            return Vec::new();
        };
        let mut out: Vec<GoalRuntimeSnapshot> = st
            .goals_by_session
            .iter()
            .map(|(sid, goal)| GoalRuntimeSnapshot {
                goal: goal.clone(),
                activation: st
                    .activation
                    .get(sid)
                    .copied()
                    .unwrap_or(GoalActivation::Disarmed),
                worker_connected: st.worker_connected,
                recovered_from_restart: st.recovered_sessions.get(sid).copied().unwrap_or(false),
            })
            .collect();
        out.sort_by(|a, b| b.goal.updated_at_ms.cmp(&a.goal.updated_at_ms));
        out
    }

    pub fn rekey_session(&self, old_id: &str, new_id: &str) {
        if old_id == new_id {
            return;
        }
        if let Ok(mut st) = self.inner.lock() {
            if let Some(mut g) = st.goals_by_session.remove(old_id) {
                g.session_id = new_id.to_string();
                st.goals_by_session.insert(new_id.to_string(), g);
            }
            if let Some(a) = st.activation.remove(old_id) {
                st.activation.insert(new_id.to_string(), a);
            }
            if let Some(r) = st.recovered_sessions.remove(old_id) {
                st.recovered_sessions.insert(new_id.to_string(), r);
            }
            if let Some(f) = st.in_flight_rounds.remove(old_id) {
                st.in_flight_rounds.insert(new_id.to_string(), f);
            }
        }
    }

    pub fn create_goal(
        &self,
        session_id: &str,
        objective: String,
        project_id: Option<String>,
        project_name: Option<String>,
        cwd: Option<String>,
        max_rounds: Option<u32>,
        max_duration_secs: Option<u64>,
        max_tokens: Option<u64>,
    ) -> Result<GoalRuntimeSnapshot, String> {
        let obj = objective.trim().to_string();
        if obj.is_empty() {
            return Err("Goal objective cannot be empty.".into());
        }
        let mut st = self.inner.lock().map_err(|_| "goal worker lock poisoned".to_string())?;
        if let Some(existing) = st.goals_by_session.get(session_id) {
            if !existing.phase.is_terminal() {
                return Err(format!(
                    "Session already has an active/open goal ({}). Pause, complete, or cancel it first.",
                    existing.phase.label()
                ));
            }
        }
        let now = now_epoch_ms();
        let goal = GoalRecord {
            goal_id: format!("goal-{}", uuid::Uuid::new_v4().simple()),
            session_id: session_id.to_string(),
            project_id,
            project_name,
            cwd,
            objective: obj,
            phase: GoalPhase::Active,
            revision: 1,
            rounds_started: 0,
            max_rounds: max_rounds.unwrap_or(DEFAULT_MAX_GOAL_ROUNDS).max(1),
            max_duration_secs,
            max_tokens,
            tokens_used: 0,
            stop_reason: None,
            blocked_code: None,
            blocked_message: None,
            last_summary: None,
            worker_owner: Some(st.worker_id.clone()),
            created_at_ms: now,
            updated_at_ms: now,
        };
        let ev = GoalEventRecord {
            goal_id: goal.goal_id.clone(),
            revision: 1,
            action: "create".into(),
            phase: GoalPhase::Active,
            detail: goal.objective.clone(),
            created_at_ms: now,
        };
        self.store.save_goal(&goal, Some(&ev))?;
        st.goals_by_session.insert(session_id.to_string(), goal.clone());
        st.activation.insert(session_id.to_string(), GoalActivation::Armed);
        st.recovered_sessions.remove(session_id);
        Ok(GoalRuntimeSnapshot {
            goal,
            activation: GoalActivation::Armed,
            worker_connected: st.worker_connected,
            recovered_from_restart: false,
        })
    }

    pub fn pause_goal(&self, session_id: &str, expected_rev: Option<u64>, reason: &str) -> Result<GoalRuntimeSnapshot, String> {
        let mut st = self.inner.lock().map_err(|_| "goal worker lock poisoned".to_string())?;
        let goal = st
            .goals_by_session
            .get_mut(session_id)
            .ok_or_else(|| "No goal exists for this conversation.".to_string())?;
        if let Some(exp) = expected_rev {
            if goal.revision != exp {
                return Err(format!("GOAL_STALE_REVISION: expected {exp}, got {}", goal.revision));
            }
        }
        if goal.phase != GoalPhase::Active {
            return Err(format!("Cannot pause a goal in '{}' state.", goal.phase.label()));
        }
        let now = now_epoch_ms();
        goal.phase = GoalPhase::Paused;
        goal.revision += 1;
        goal.stop_reason = Some(reason.to_string());
        goal.updated_at_ms = now;
        let updated = goal.clone();
        let ev = GoalEventRecord {
            goal_id: updated.goal_id.clone(),
            revision: updated.revision,
            action: "pause".into(),
            phase: GoalPhase::Paused,
            detail: reason.to_string(),
            created_at_ms: now,
        };
        self.store.save_goal(&updated, Some(&ev))?;
        st.activation.insert(session_id.to_string(), GoalActivation::Disarmed);
        st.in_flight_rounds.remove(session_id);
        Ok(GoalRuntimeSnapshot {
            goal: updated,
            activation: GoalActivation::Disarmed,
            worker_connected: st.worker_connected,
            recovered_from_restart: false,
        })
    }

    pub fn resume_goal(&self, session_id: &str, expected_rev: Option<u64>) -> Result<GoalRuntimeSnapshot, String> {
        let mut st = self.inner.lock().map_err(|_| "goal worker lock poisoned".to_string())?;
        let worker_id = st.worker_id.clone();
        let goal = st
            .goals_by_session
            .get_mut(session_id)
            .ok_or_else(|| "No goal exists for this conversation.".to_string())?;
        if let Some(exp) = expected_rev {
            if goal.revision != exp {
                return Err(format!("GOAL_STALE_REVISION: expected {exp}, got {}", goal.revision));
            }
        }
        if !matches!(goal.phase, GoalPhase::Active | GoalPhase::Paused | GoalPhase::Blocked) {
            return Err(format!("Cannot resume a goal in '{}' state.", goal.phase.label()));
        }
        if goal.rounds_started >= goal.max_rounds {
            return Err(format!(
                "Goal reached its round budget ({}/{}).",
                goal.rounds_started, goal.max_rounds
            ));
        }
        let now = now_epoch_ms();
        goal.phase = GoalPhase::Active;
        goal.revision += 1;
        goal.blocked_code = None;
        goal.blocked_message = None;
        goal.stop_reason = None;
        goal.worker_owner = Some(worker_id);
        goal.updated_at_ms = now;
        let updated = goal.clone();
        let ev = GoalEventRecord {
            goal_id: updated.goal_id.clone(),
            revision: updated.revision,
            action: "resume".into(),
            phase: GoalPhase::Active,
            detail: "Resumed by user".into(),
            created_at_ms: now,
        };
        self.store.save_goal(&updated, Some(&ev))?;
        st.activation.insert(session_id.to_string(), GoalActivation::Armed);
        st.recovered_sessions.remove(session_id);
        Ok(GoalRuntimeSnapshot {
            goal: updated,
            activation: GoalActivation::Armed,
            worker_connected: st.worker_connected,
            recovered_from_restart: false,
        })
    }

    pub fn cancel_goal(&self, session_id: &str, reason: &str) -> Result<GoalRuntimeSnapshot, String> {
        let mut st = self.inner.lock().map_err(|_| "goal worker lock poisoned".to_string())?;
        let goal = st
            .goals_by_session
            .get_mut(session_id)
            .ok_or_else(|| "No goal exists for this conversation.".to_string())?;
        if goal.phase.is_terminal() {
            return Err(format!("Goal is already '{}'.", goal.phase.label()));
        }
        let now = now_epoch_ms();
        goal.phase = GoalPhase::Cancelled;
        goal.revision += 1;
        goal.stop_reason = Some(reason.to_string());
        goal.updated_at_ms = now;
        let updated = goal.clone();
        let ev = GoalEventRecord {
            goal_id: updated.goal_id.clone(),
            revision: updated.revision,
            action: "cancel".into(),
            phase: GoalPhase::Cancelled,
            detail: reason.to_string(),
            created_at_ms: now,
        };
        self.store.save_goal(&updated, Some(&ev))?;
        st.activation.insert(session_id.to_string(), GoalActivation::Disarmed);
        st.in_flight_rounds.remove(session_id);
        Ok(GoalRuntimeSnapshot {
            goal: updated,
            activation: GoalActivation::Disarmed,
            worker_connected: st.worker_connected,
            recovered_from_restart: false,
        })
    }

    pub fn complete_goal(&self, session_id: &str, summary: Option<String>) -> Result<GoalRuntimeSnapshot, String> {
        let mut st = self.inner.lock().map_err(|_| "goal worker lock poisoned".to_string())?;
        let goal = st
            .goals_by_session
            .get_mut(session_id)
            .ok_or_else(|| "No goal exists for this conversation.".to_string())?;
        if goal.phase == GoalPhase::Complete {
            return Err("Goal is already complete.".into());
        }
        let now = now_epoch_ms();
        goal.phase = GoalPhase::Complete;
        goal.revision += 1;
        goal.stop_reason = Some("completed".into());
        if let Some(s) = summary.filter(|s| !s.trim().is_empty()) {
            goal.last_summary = Some(s);
        }
        goal.updated_at_ms = now;
        let updated = goal.clone();
        let ev = GoalEventRecord {
            goal_id: updated.goal_id.clone(),
            revision: updated.revision,
            action: "complete".into(),
            phase: GoalPhase::Complete,
            detail: updated
                .last_summary
                .clone()
                .unwrap_or_else(|| "Marked complete".into()),
            created_at_ms: now,
        };
        self.store.save_goal(&updated, Some(&ev))?;
        st.activation.insert(session_id.to_string(), GoalActivation::Disarmed);
        st.in_flight_rounds.remove(session_id);
        Ok(GoalRuntimeSnapshot {
            goal: updated,
            activation: GoalActivation::Disarmed,
            worker_connected: st.worker_connected,
            recovered_from_restart: false,
        })
    }

    pub fn fail_goal(&self, session_id: &str, error_message: &str) -> Result<GoalRuntimeSnapshot, String> {
        let mut st = self.inner.lock().map_err(|_| "goal worker lock poisoned".to_string())?;
        let goal = st
            .goals_by_session
            .get_mut(session_id)
            .ok_or_else(|| "No goal exists for this conversation.".to_string())?;
        let now = now_epoch_ms();
        goal.phase = GoalPhase::Failed;
        goal.revision += 1;
        goal.stop_reason = Some(format!("failed: {error_message}"));
        goal.blocked_code = Some("runtime-error".into());
        goal.blocked_message = Some(error_message.to_string());
        goal.updated_at_ms = now;
        let updated = goal.clone();
        let ev = GoalEventRecord {
            goal_id: updated.goal_id.clone(),
            revision: updated.revision,
            action: "fail".into(),
            phase: GoalPhase::Failed,
            detail: error_message.to_string(),
            created_at_ms: now,
        };
        self.store.save_goal(&updated, Some(&ev))?;
        st.activation.insert(session_id.to_string(), GoalActivation::Disarmed);
        st.in_flight_rounds.remove(session_id);
        Ok(GoalRuntimeSnapshot {
            goal: updated,
            activation: GoalActivation::Disarmed,
            worker_connected: st.worker_connected,
            recovered_from_restart: false,
        })
    }

    /// Reserve the next sequential round if the goal is `Active` & `Armed`, enforcing
    /// round, duration, and token budgets. Returns `Ok(Some(round))` when a round may
    /// proceed, or `Ok(None)` when disarmed/already in flight/budget exhausted.
    pub fn try_begin_round(
        &self,
        session_id: &str,
        tokens_used_now: Option<u64>,
    ) -> Result<Option<u32>, String> {
        let mut st = self.inner.lock().map_err(|_| "goal worker lock poisoned".to_string())?;
        if st.in_flight_rounds.contains_key(session_id) {
            return Ok(None);
        }
        if st.activation.get(session_id).copied().unwrap_or(GoalActivation::Disarmed)
            != GoalActivation::Armed
        {
            return Ok(None);
        }
        let Some(goal) = st.goals_by_session.get_mut(session_id) else {
            return Ok(None);
        };
        if goal.phase != GoalPhase::Active {
            return Ok(None);
        }
        let now = now_epoch_ms();
        if let Some(t) = tokens_used_now {
            goal.tokens_used = goal.tokens_used.max(t);
        }

        // Enforce budgets before admitting a new round
        let mut limit_breach: Option<(&'static str, String)> = None;
        if goal.rounds_started >= goal.max_rounds {
            limit_breach = Some((
                "round-limit",
                format!("Reached configured max rounds ({}/{})", goal.rounds_started, goal.max_rounds),
            ));
        } else if let Some(max_secs) = goal.max_duration_secs {
            let elapsed_secs = ((now - goal.created_at_ms).max(0) as u64) / 1000;
            if elapsed_secs >= max_secs {
                limit_breach = Some((
                    "time-limit",
                    format!("Reached configured time limit ({elapsed_secs}s / {max_secs}s)"),
                ));
            }
        } else if let Some(max_tok) = goal.max_tokens {
            if goal.tokens_used >= max_tok {
                limit_breach = Some((
                    "token-limit",
                    format!("Reached configured token limit ({} / {max_tok})", goal.tokens_used),
                ));
            }
        }

        if let Some((code, msg)) = limit_breach {
            goal.phase = GoalPhase::Blocked;
            goal.revision += 1;
            goal.blocked_code = Some(code.into());
            goal.blocked_message = Some(msg.clone());
            goal.stop_reason = Some(msg.clone());
            goal.updated_at_ms = now;
            let updated = goal.clone();
            let ev = GoalEventRecord {
                goal_id: updated.goal_id.clone(),
                revision: updated.revision,
                action: "block".into(),
                phase: GoalPhase::Blocked,
                detail: msg,
                created_at_ms: now,
            };
            self.store.save_goal(&updated, Some(&ev))?;
            st.activation.insert(session_id.to_string(), GoalActivation::Disarmed);
            return Ok(None);
        }

        goal.rounds_started += 1;
        goal.revision += 1;
        goal.updated_at_ms = now;
        let round = goal.rounds_started;
        let updated = goal.clone();
        let ev = GoalEventRecord {
            goal_id: updated.goal_id.clone(),
            revision: updated.revision,
            action: "round_start".into(),
            phase: GoalPhase::Active,
            detail: format!("Round {round}/{}", updated.max_rounds),
            created_at_ms: now,
        };
        self.store.save_goal(&updated, Some(&ev))?;
        st.in_flight_rounds.insert(session_id.to_string(), round);
        Ok(Some(round))
    }

    /// Record turn settlement for a goal session. Returns `true` if the goal remains
    /// `Active` & `Armed` for a follow-up round.
    pub fn on_turn_settled(
        &self,
        session_id: &str,
        succeeded: bool,
        summary: Option<String>,
        tokens_used_now: Option<u64>,
        completed_marker: bool,
    ) -> bool {
        let Ok(mut st) = self.inner.lock() else {
            return false;
        };
        st.in_flight_rounds.remove(session_id);
        let Some(goal) = st.goals_by_session.get_mut(session_id) else {
            return false;
        };
        if goal.phase != GoalPhase::Active {
            return false;
        }
        let now = now_epoch_ms();
        if let Some(t) = tokens_used_now {
            goal.tokens_used = goal.tokens_used.max(t);
        }
        if let Some(s) = summary.filter(|s| !s.trim().is_empty()) {
            goal.last_summary = Some(s.chars().take(240).collect());
        }
        goal.updated_at_ms = now;

        if !succeeded {
            goal.phase = GoalPhase::Failed;
            goal.revision += 1;
            goal.stop_reason = Some("turn_failed".into());
            let updated = goal.clone();
            let ev = GoalEventRecord {
                goal_id: updated.goal_id.clone(),
                revision: updated.revision,
                action: "fail".into(),
                phase: GoalPhase::Failed,
                detail: updated.last_summary.clone().unwrap_or_else(|| "Turn failed".into()),
                created_at_ms: now,
            };
            let _ = self.store.save_goal(&updated, Some(&ev));
            st.activation.insert(session_id.to_string(), GoalActivation::Disarmed);
            return false;
        }

        if completed_marker {
            goal.phase = GoalPhase::Complete;
            goal.revision += 1;
            goal.stop_reason = Some("goal_achieved".into());
            let updated = goal.clone();
            let ev = GoalEventRecord {
                goal_id: updated.goal_id.clone(),
                revision: updated.revision,
                action: "complete".into(),
                phase: GoalPhase::Complete,
                detail: "Goal completed".into(),
                created_at_ms: now,
            };
            let _ = self.store.save_goal(&updated, Some(&ev));
            st.activation.insert(session_id.to_string(), GoalActivation::Disarmed);
            return false;
        }

        let updated = goal.clone();
        let _ = self.store.save_goal(&updated, None);
        st.activation.get(session_id).copied() == Some(GoalActivation::Armed)
    }

    pub fn goal_events(&self, goal_id: &str) -> Vec<GoalEventRecord> {
        self.store.load_goal_events(goal_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_lifecycle_enforces_cas_budgets_and_cold_restart_recovery_without_duplicates() {
        let dir = std::env::temp_dir().join(format!("hermes-goal-test-{}", uuid::Uuid::new_v4()));
        let store = HarnessStore::open_at(&dir.join("harness.db")).expect("open store");
        let worker = GoalWorker::new(store.clone());
        worker.set_worker_connected(true);

        let created = worker
            .create_goal(
                "sess-g1",
                "Ship full coding agent".into(),
                Some("proj-1".into()),
                Some("Grok Bot".into()),
                Some("/tmp/repo".into()),
                Some(2),
                None,
                None,
            )
            .expect("create goal");
        assert_eq!(created.goal.phase, GoalPhase::Active);
        assert_eq!(created.goal.revision, 1);
        assert_eq!(created.activation, GoalActivation::Armed);

        // Stale CAS revision must fail
        assert!(worker.pause_goal("sess-g1", Some(99), "test").is_err());

        // Round 1
        assert_eq!(worker.try_begin_round("sess-g1", None).expect("r1"), Some(1));
        // Duplicate round while in flight must be prevented
        assert_eq!(worker.try_begin_round("sess-g1", None).expect("dup"), None);
        assert!(worker.on_turn_settled("sess-g1", true, Some("Step 1 done".into()), Some(500), false));

        // Pause & Resume
        let paused = worker.pause_goal("sess-g1", None, "user paused").expect("pause");
        assert_eq!(paused.goal.phase, GoalPhase::Paused);
        assert_eq!(paused.activation, GoalActivation::Disarmed);
        assert_eq!(worker.try_begin_round("sess-g1", None).expect("paused"), None);

        let resumed = worker.resume_goal("sess-g1", None).expect("resume");
        assert_eq!(resumed.goal.phase, GoalPhase::Active);
        assert_eq!(resumed.activation, GoalActivation::Armed);

        // Cold restart simulation: reopened worker restores persisted goal as Disarmed (no duplicate run!)
        let reopened_worker = GoalWorker::new(store.clone());
        let recovered = reopened_worker.snapshot_for_session("sess-g1").expect("restored");
        assert_eq!(recovered.goal.phase, GoalPhase::Active);
        assert_eq!(recovered.activation, GoalActivation::Disarmed);
        assert!(recovered.recovered_from_restart);
        assert_eq!(reopened_worker.try_begin_round("sess-g1", None).expect("no dup"), None);

        // User explicitly resumes -> Round 2 -> Round 3 hits budget (max_rounds=2) -> Blocked with stop_reason
        reopened_worker.resume_goal("sess-g1", None).expect("resume after recovery");
        assert_eq!(reopened_worker.try_begin_round("sess-g1", None).expect("r2"), Some(2));
        reopened_worker.on_turn_settled("sess-g1", true, Some("Step 2 done".into()), Some(900), false);
        assert_eq!(reopened_worker.try_begin_round("sess-g1", None).expect("limit"), None);
        let blocked = reopened_worker.snapshot_for_session("sess-g1").expect("blocked");
        assert_eq!(blocked.goal.phase, GoalPhase::Blocked);
        assert_eq!(blocked.goal.blocked_code.as_deref(), Some("round-limit"));

        // History recorded
        assert!(reopened_worker.goal_events(&blocked.goal.goal_id).len() >= 5);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn goal_command_parser_extracts_subcommands_and_limits() {
        assert_eq!(parse_goal_slash_command("/goal"), Some(GoalCommand::Status));
        assert_eq!(parse_goal_slash_command("/goal pause"), Some(GoalCommand::Pause));
        assert_eq!(parse_goal_slash_command("/goal resume"), Some(GoalCommand::Resume));
        assert_eq!(parse_goal_slash_command("/goal cancel"), Some(GoalCommand::Cancel));
        assert_eq!(
            parse_goal_slash_command("/goal Fix tests --max-rounds 12 --max-minutes 15 --max-tokens 50000"),
            Some(GoalCommand::Create {
                objective: "Fix tests".into(),
                max_rounds: Some(12),
                max_duration_secs: Some(900),
                max_tokens: Some(50000),
            })
        );
    }
}
