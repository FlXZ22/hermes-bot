use crate::hermes::types::HermesSessionUsage;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// Measurement window used for sliding-window output throughput (tokens/sec).
pub const TPS_WINDOW_SECS: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricProvenance {
    /// Reported directly by the model provider / Hermes `session.usage`.
    Measured,
    /// Estimated from runtime rate/delta samples (explicitly labeled as estimate).
    #[allow(dead_code)]
    Estimated,
    /// Not reported by the runtime/provider.
    Unavailable,
}

impl MetricProvenance {
    pub fn label(self) -> &'static str {
        match self {
            Self::Measured => "measured",
            Self::Estimated => "est.",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TokenMetricValue {
    pub value: Option<u64>,
    pub provenance: MetricProvenance,
}

impl TokenMetricValue {
    pub fn unavailable() -> Self {
        Self {
            value: None,
            provenance: MetricProvenance::Unavailable,
        }
    }

    pub fn measured(v: u64) -> Self {
        Self {
            value: Some(v),
            provenance: MetricProvenance::Measured,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThroughputMetric {
    pub tokens_per_sec: Option<f64>,
    pub provenance: MetricProvenance,
    pub window_secs: f64,
}

/// Separate agent-recorded file edit counts (distinct from Git worktree vs HEAD counts).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentEditMetrics {
    pub files_touched: usize,
    pub lines_added: u64,
    pub lines_removed: u64,
}

/// Live runtime metrics tracker for a session.
/// Never presents character counts as exact token counts.
#[derive(Debug, Clone, Default)]
pub struct SessionLiveMetrics {
    pub run_started_at: Option<Instant>,
    pub last_run_elapsed_ms: Option<u64>,
    pub first_token_at: Option<Instant>,
    pub ttft_ms: Option<u64>,
    /// Provider-reported cumulative output token samples `(timestamp, output_tokens)`
    /// for computing sliding-window measured tok/s.
    provider_output_samples: VecDeque<(Instant, u64)>,
    pub provider_input_tokens: Option<u64>,
    pub provider_output_tokens: Option<u64>,
    pub provider_reasoning_tokens: Option<u64>,
    pub provider_total_tokens: Option<u64>,
    pub provider_avg_tps: Option<f64>,
    pub cache_hit_pct: Option<f64>,
    pub context_used: Option<u64>,
    pub context_max: Option<u64>,
    pub agent_edits: AgentEditMetrics,
}

impl SessionLiveMetrics {
    pub fn on_turn_start(&mut self) {
        self.on_turn_start_at(Instant::now());
    }

    pub fn on_turn_start_at(&mut self, now: Instant) {
        self.run_started_at = Some(now);
        self.first_token_at = None;
        self.ttft_ms = None;
        self.provider_output_samples.clear();
        if let Some(out) = self.provider_output_tokens {
            self.provider_output_samples.push_back((now, out));
        }
    }

    pub fn on_first_token(&mut self) {
        if self.first_token_at.is_none() {
            let now = Instant::now();
            self.first_token_at = Some(now);
            if let Some(start) = self.run_started_at {
                self.ttft_ms = Some(now.saturating_duration_since(start).as_millis() as u64);
            }
        }
    }

    pub fn on_turn_end(&mut self) {
        if let Some(start) = self.run_started_at.take() {
            self.last_run_elapsed_ms = Some(start.elapsed().as_millis() as u64);
        }
    }

    pub fn on_provider_usage(&mut self, usage: &HermesSessionUsage) {
        self.on_provider_usage_at(usage, Instant::now());
    }

    pub fn on_provider_usage_at(&mut self, usage: &HermesSessionUsage, now: Instant) {
        if let Some(v) = usage.input_tokens.or(usage.prompt_tokens) {
            self.provider_input_tokens = Some(v);
        }
        if let Some(v) = usage.output_tokens.or(usage.completion_tokens) {
            self.provider_output_tokens = Some(v);
            self.provider_output_samples.push_back((now, v));
            let cutoff = Duration::from_secs_f64(TPS_WINDOW_SECS);
            while self.provider_output_samples.len() > 2 {
                if let Some((front_t, _)) = self.provider_output_samples.front() {
                    if now.saturating_duration_since(*front_t) > cutoff {
                        self.provider_output_samples.pop_front();
                        continue;
                    }
                }
                break;
            }
        }
        if let Some(v) = usage.reasoning_tokens {
            self.provider_reasoning_tokens = Some(v);
        }
        if let Some(v) = usage.total_tokens {
            self.provider_total_tokens = Some(v);
        }
        if let Some(tps) = usage.avg_tps.filter(|v| v.is_finite() && *v > 0.0) {
            self.provider_avg_tps = Some(tps);
        }
        if let Some(pct) = usage.cache_hit_pct.filter(|v| v.is_finite()) {
            self.cache_hit_pct = Some(pct);
        }
        if let Some(u) = usage.context_used {
            self.context_used = Some(u);
        }
        if let Some(m) = usage.context_limit {
            self.context_max = Some(m);
        }
    }

    pub fn record_agent_diff(&mut self, diff_text: &str) {
        let mut added = 0u64;
        let mut removed = 0u64;
        for line in diff_text.lines() {
            if line.starts_with("+++") || line.starts_with("---") {
                continue;
            }
            if line.starts_with('+') {
                added += 1;
            } else if line.starts_with('-') {
                removed += 1;
            }
        }
        if added > 0 || removed > 0 {
            self.agent_edits.files_touched += 1;
            self.agent_edits.lines_added += added;
            self.agent_edits.lines_removed += removed;
        }
    }

    pub fn elapsed_ms(&self) -> Option<u64> {
        self.run_started_at
            .map(|s| s.elapsed().as_millis() as u64)
            .or(self.last_run_elapsed_ms)
    }

    pub fn input_tokens(&self) -> TokenMetricValue {
        self.provider_input_tokens
            .map(TokenMetricValue::measured)
            .unwrap_or_else(TokenMetricValue::unavailable)
    }

    pub fn output_tokens(&self) -> TokenMetricValue {
        self.provider_output_tokens
            .map(TokenMetricValue::measured)
            .unwrap_or_else(TokenMetricValue::unavailable)
    }

    pub fn total_tokens(&self) -> TokenMetricValue {
        if let Some(total) = self.provider_total_tokens {
            return TokenMetricValue::measured(total);
        }
        if let (Some(i), Some(o)) = (self.provider_input_tokens, self.provider_output_tokens) {
            return TokenMetricValue::measured(i.saturating_add(o));
        }
        TokenMetricValue::unavailable()
    }

    /// Output tokens/sec over the defined `TPS_WINDOW_SECS` (5s) sliding window.
    /// Prefers measured sliding-window delta from provider counts, falls back to provider
    /// `avg_tps`, and reports `Unavailable` when neither exists (never fabricates from chars).
    pub fn output_tps(&self) -> ThroughputMetric {
        if let (Some((t0, c0)), Some((t1, c1))) = (
            self.provider_output_samples.front(),
            self.provider_output_samples.back(),
        ) {
            let dt = t1.saturating_duration_since(*t0).as_secs_f64();
            let delta = c1.saturating_sub(*c0);
            if dt >= 0.1 && delta > 0 {
                return ThroughputMetric {
                    tokens_per_sec: Some(delta as f64 / dt),
                    provenance: MetricProvenance::Measured,
                    window_secs: TPS_WINDOW_SECS,
                };
            }
        }
        if let Some(avg) = self.provider_avg_tps {
            return ThroughputMetric {
                tokens_per_sec: Some(avg),
                provenance: MetricProvenance::Measured,
                window_secs: TPS_WINDOW_SECS,
            };
        }
        ThroughputMetric {
            tokens_per_sec: None,
            provenance: MetricProvenance::Unavailable,
            window_secs: TPS_WINDOW_SECS,
        }
    }
}

pub fn format_elapsed_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let secs = ms / 1000;
        format!("{}m {:02}s", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_distinguish_measured_vs_unavailable_and_compute_window_tps() {
        let mut m = SessionLiveMetrics::default();
        assert_eq!(m.input_tokens().provenance, MetricProvenance::Unavailable);
        assert_eq!(m.output_tokens().provenance, MetricProvenance::Unavailable);
        assert_eq!(m.output_tps().provenance, MetricProvenance::Unavailable);
        assert!(m.output_tps().tokens_per_sec.is_none());

        let t0 = Instant::now();
        m.on_turn_start_at(t0);
        m.on_provider_usage_at(
            &HermesSessionUsage {
                input_tokens: Some(1200),
                output_tokens: Some(100),
                ..Default::default()
            },
            t0,
        );
        m.on_provider_usage_at(
            &HermesSessionUsage {
                input_tokens: Some(1200),
                output_tokens: Some(300),
                ..Default::default()
            },
            t0 + Duration::from_secs(2),
        );

        assert_eq!(m.input_tokens(), TokenMetricValue::measured(1200));
        assert_eq!(m.output_tokens(), TokenMetricValue::measured(300));
        assert_eq!(m.total_tokens(), TokenMetricValue::measured(1500));
        let tps = m.output_tps();
        assert_eq!(tps.provenance, MetricProvenance::Measured);
        let rate = tps.tokens_per_sec.expect("measured tps");
        assert!((rate - 100.0).abs() < 1.0, "expected ~100 tok/s, got {rate}");
    }
}
