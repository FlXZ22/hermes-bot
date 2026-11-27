use crate::hermes::types::{HermesUsagePoint, HermesUsageReport};
use crate::state::app_state::{AppState, AppViewId};
use crate::theme::components::{
    cursor_icon, format_tokens, gb_btn, gb_section_card, gb_section_card_head, BtnVariant,
    FONT_MONO, FONT_SANS,
};
use crate::theme::tokens::SandTheme;
use crate::ui::smooth_scroll::{smooth_scroll_layer, SmoothScroller};
use chrono::{Days, NaiveDate};
use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::{scroll::ScrollableElement, tooltip::Tooltip};
use std::collections::{BTreeMap, HashMap};

const PERIODS: [usize; 3] = [7, 14, 30];
const DEFAULT_PERIOD: usize = 14;

pub struct AnalyticsView {
    app: Entity<AppState>,
    scroll_handle: ScrollHandle,
    smooth_scroller: SmoothScroller,
    days: usize,
    source: Option<(u64, String)>,
    visible: bool,
    report: Option<HermesUsageReport>,
    request: Option<Task<()>>,
    failed: bool,
}

impl AnalyticsView {
    pub fn new(app: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |this, _, cx| this.sync_source(cx))
            .detach();

        let scroll_handle = ScrollHandle::new();
        let smooth_scroller = SmoothScroller::for_handle(scroll_handle.clone());
        let mut view = Self {
            app,
            scroll_handle,
            smooth_scroller,
            days: DEFAULT_PERIOD,
            source: None,
            visible: false,
            report: None,
            request: None,
            failed: false,
        };
        view.sync_source(cx);
        view
    }

    fn source(app: &AppState) -> Option<(u64, String)> {
        let client = app.client.as_ref()?;
        let profile = client
            .profile()
            .unwrap_or_else(|| app.active_profile.clone());
        Some((app.ws_generation, profile))
    }

    fn sync_source(&mut self, cx: &mut Context<Self>) {
        let app = self.app.read(cx);
        let visible = !app.setup_open && !app.initializing && app.active_view == AppViewId::Analytics;
        let entered = visible && !self.visible;
        self.visible = visible;

        let source = Self::source(app);
        let changed = self.source != source;
        if changed {
            self.request = None;
            self.report = None;
            self.failed = false;
            self.source = source;
        }
        if visible && (entered || changed) {
            self.refresh(cx);
            cx.notify();
        }
    }

    fn set_period(&mut self, days: usize, cx: &mut Context<Self>) {
        if days == self.days {
            return;
        }

        self.days = days;
        self.smooth_scroller.cancel();
        self.scroll_handle.set_offset(Point::default());
        self.report = None;
        self.request = None;
        self.failed = false;
        self.refresh(cx);
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.request.is_some() {
            return;
        }

        let app = self.app.read(cx);
        let (Some(client), Some(source)) = (app.client.clone(), Self::source(app)) else {
            return;
        };
        let rt = app.tokio_rt.clone();
        let days = self.days;
        let profile = source.1.clone();
        self.failed = false;
        self.request = Some(cx.spawn(async move |this, cx| {
            let result = rt
                .spawn(async move { client.get_usage_analytics(days, &profile).await })
                .await
                .unwrap_or_else(|err| Err(err.to_string()));

            let _ = this.update(cx, |this, cx| {
                if this.days != days || Self::source(this.app.read(cx)).as_ref() != Some(&source) {
                    return;
                }
                this.request = None;
                match result {
                    Ok(report) => this.report = Some(report),
                    Err(_) => this.failed = true,
                }
                if this.visible {
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }
}

impl Render for AnalyticsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.app.read(cx).sand_theme();
        let loading = self.request.is_some();
        let connected = self.source.is_some();
        let status = match (connected, loading, self.failed, self.report.is_some()) {
            (false, _, _, _) => Some("Connect to Hermes to view usage."),
            (_, true, _, false) => Some("Loading usage…"),
            (_, _, true, true) => Some("Refresh failed. Showing the last report for this period."),
            (_, _, true, false) => Some("Couldn’t load usage from Hermes. Try Refresh."),
            _ => None,
        };

        div()
            .size_full()
            .relative()
            .bg(theme.bg_pane)
            .font_family(FONT_SANS)
            .child(
                div()
                    .id("analytics-scroll")
                    .track_scroll(&self.scroll_handle)
                    .size_full()
                    .overflow_y_scroll()
                    .restrict_scroll_to_axis()
                    .p(px(28.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(940.0))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .gap(px(18.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .text_size(px(22.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme.text_primary)
                                            .child("Usage & Analytics"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap(px(6.0))
                                            .children(PERIODS.map(|days| {
                                                let variant = if days == self.days {
                                                    BtnVariant::Primary
                                                } else {
                                                    BtnVariant::Ghost
                                                };
                                                gb_btn(
                                                    ("usage-period", days),
                                                    &theme,
                                                    variant,
                                                    true,
                                                    false,
                                                )
                                                .child(format!("{days} days"))
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.set_period(days, cx);
                                                }))
                                            }))
                                            .child(
                                                gb_btn(
                                                    "analytics-refresh",
                                                    &theme,
                                                    BtnVariant::Secondary,
                                                    true,
                                                    !connected || loading,
                                                )
                                                .child(cursor_icon(
                                                    "refresh",
                                                    13.0,
                                                    theme.text_secondary,
                                                ))
                                                .child(if loading {
                                                    "Refreshing…"
                                                } else {
                                                    "Refresh"
                                                })
                                                .on_click(
                                                    cx.listener(|this, _, _, cx| this.refresh(cx)),
                                                ),
                                            ),
                                    ),
                            )
                            .when_some(status, |d, text| {
                                d.child(gb_section_card(&theme).child(empty_state(&theme, text)))
                            })
                            .when_some(self.report.as_ref(), |d, report| {
                                let summary = UsageSummary::from_report(report);
                                let days = report
                                    .period_days
                                    .filter(|days| (1..=365).contains(days))
                                    .unwrap_or(self.days);
                                d.child(summary_cards(&theme, &summary))
                                    .child(daily_usage(&theme, report, days))
                                    .child(model_breakdown(&theme, report, summary.sessions))
                            }),
                    ),
            )
            .vertical_scrollbar(&self.scroll_handle)
            .child(smooth_scroll_layer(&self.smooth_scroller).filter_events(|event, _| !event.modifiers.shift))
    }
}

#[derive(Debug, PartialEq)]
struct UsageSummary {
    tokens: Option<u64>,
    input: Option<u64>,
    output: Option<u64>,
    cost: Option<f64>,
    requests: Option<u64>,
    sessions: Option<u64>,
}

impl UsageSummary {
    fn from_report(report: &HermesUsageReport) -> Self {
        let totals = report.totals.as_ref();
        let daily = &report.daily;
        let input = totals
            .and_then(|t| t.total_input)
            .or_else(|| sum_counts(daily.iter().map(|p| p.input_tokens)));
        let output = totals
            .and_then(|t| t.total_output)
            .or_else(|| sum_counts(daily.iter().map(|p| p.output_tokens)));
        let reported_tokens =
            totals.and_then(|t| Some(t.total_input?.saturating_add(t.total_output?)));
        let total = reported_tokens
            .or_else(|| sum_counts(daily.iter().map(HermesUsagePoint::total_tokens)));
        let cost = totals.and_then(|t| t.total_estimated_cost).or_else(|| {
            daily
                .iter()
                .map(|p| p.cost_usd)
                .reduce(|a, b| Some(a? + b?))
                .flatten()
        });
        let requests = totals
            .and_then(|t| t.total_api_calls)
            .or_else(|| sum_counts(daily.iter().map(|p| p.requests)));
        let sessions = totals
            .and_then(|t| t.total_sessions)
            .or_else(|| sum_counts(daily.iter().map(|p| p.sessions)));
        Self {
            tokens: total.or_else(|| (sessions == Some(0)).then_some(0)),
            input,
            output,
            cost,
            requests,
            sessions,
        }
    }
}

fn sum_counts(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    values.reduce(|a, b| Some(a?.saturating_add(b?))).flatten()
}

fn summary_cards(theme: &SandTheme, summary: &UsageSummary) -> Div {
    div()
        .w_full()
        .flex()
        .flex_wrap()
        .gap(px(12.0))
        .child(metric_card(
            theme,
            "Tokens",
            token_label(summary.tokens),
            format!(
                "{} input · {} output",
                token_label(summary.input),
                token_label(summary.output)
            ),
        ))
        .child(metric_card(
            theme,
            "API Calls",
            count_label(summary.requests),
            "Provider requests".into(),
        ))
        .child(metric_card(
            theme,
            "Estimated Spend",
            cost_label(summary.cost),
            "Hermes estimate in USD".into(),
        ))
        .child(metric_card(
            theme,
            "Sessions",
            count_label(summary.sessions),
            "Started in this period".into(),
        ))
}

fn daily_usage(theme: &SandTheme, report: &HermesUsageReport, days: usize) -> Div {
    let mut timeline: BTreeMap<_, _> = report
        .daily
        .iter()
        .map(|point| (point.date, Some(point)))
        .collect();
    // The server reports UTC dates and may include a partial day at each end.
    if let (Some((&first, _)), Some((&last, _))) =
        (timeline.first_key_value(), timeline.last_key_value())
    {
        let span = (last - first).num_days() as u64;
        if span <= days as u64 {
            for offset in 1..span {
                timeline.entry(first + Days::new(offset)).or_insert(None);
            }
        }
    }

    let peak = report
        .daily
        .iter()
        .filter_map(HermesUsagePoint::total_tokens)
        .max();
    let section = gb_section_card(theme).child(section_header(
        theme,
        "three-bars",
        "Daily Token Usage",
        format!("Past {days} days · Peak {}", token_label(peak)),
    ));
    let Some(peak) = peak else {
        return section.child(empty_state(
            theme,
            "No daily token data reported for this period.",
        ));
    };

    let stride = timeline.len().div_ceil(6).max(1);
    let chart_min_width = px(timeline.len() as f32 * 15.0 + 36.0);
    section.child(
        div()
            .id("usage-chart-scroll")
            .w_full()
            .child(
                div()
                    .w_full()
                    .min_w(chart_min_width)
                    .px(px(18.0))
            .py(px(16.0))
            .flex()
            .gap(px(4.0))
            .children(
                timeline
                    .into_iter()
                    .enumerate()
                    .map(|(index, (date, usage))| {
                        let tokens = usage.and_then(HermesUsagePoint::total_tokens);
                        let ratio = tokens.unwrap_or(0) as f32 / peak.max(1) as f32;
                        let description = day_description(date, usage);
                        div()
                            .id(("usage-day", index))
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .items_center()
                            .tooltip(move |window, cx| {
                                Tooltip::new(description.clone()).build(window, cx)
                            })
                            .child(
                                div()
                                    .h(px(136.0))
                                    .w_full()
                                    .max_w(px(34.0))
                                    .flex()
                                    .flex_col()
                                    .justify_end()
                                    .border_b_1()
                                    .border_color(theme.border_default)
                                    .child(
                                        div()
                                            .w_full()
                                            .h(px(ratio * 132.0))
                                            .rounded_t(px(4.0))
                                            .bg(theme.dataviz_color(0)),
                                    ),
                            )
                            .child(
                                div()
                                    .pt(px(7.0))
                                    .h(px(24.0))
                                    .text_size(px(10.0))
                                    .font_family(FONT_MONO)
                                    .text_color(theme.text_secondary)
                                    .whitespace_nowrap()
                                    .when(index % stride == 0, |d| {
                                        d.child(date.format("%m-%d").to_string())
                                    }),
                            )
                    }),
            ),
            )
            .overflow_x_scrollbar()
    )
}

fn day_description(date: NaiveDate, usage: Option<&HermesUsagePoint>) -> String {
    let Some(usage) = usage else {
        return format!("{date} · No usage reported");
    };
    format!(
        "{date} · {} tokens · {} API calls · {} estimated",
        count_label(usage.total_tokens()),
        count_label(usage.requests),
        cost_label(usage.cost_usd),
    )
}

fn session_rows(report: &HermesUsageReport, sessions: Option<u64>) -> Option<Vec<(String, u64)>> {
    let models = report.by_model.as_ref()?;
    if !models.is_empty() && !models.iter().any(|model| model.sessions.is_some()) {
        return None;
    }

    let mut counts = HashMap::<&str, u64>::new();
    for model in models {
        if let Some(n) = model.sessions.filter(|&n| n > 0) {
            let count = counts.entry(model.model.trim()).or_default();
            *count = count.saturating_add(n);
        }
    }
    let attributed = counts.values().copied().fold(0, u64::saturating_add);
    let remainder = sessions.unwrap_or(0).saturating_sub(attributed);
    if remainder > 0 {
        let unknown = counts.entry("").or_default();
        *unknown = unknown.saturating_add(remainder);
    }
    let mut rows: Vec<_> = counts
        .into_iter()
        .map(|(model, count)| {
            let name = if model.is_empty() {
                "Model not reported"
            } else {
                model
            };
            (name.to_string(), count)
        })
        .collect();
    rows.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Some(rows)
}

fn model_breakdown(theme: &SandTheme, report: &HermesUsageReport, sessions: Option<u64>) -> Div {
    let section = gb_section_card(theme).child(section_header(
        theme,
        "chip",
        "Sessions by Model",
        "Share of counted sessions".into(),
    ));
    let Some(rows) = session_rows(report, sessions) else {
        return section.child(empty_state(
            theme,
            "Hermes didn’t report session counts by model.",
        ));
    };
    if rows.is_empty() {
        return section.child(empty_state(theme, "No sessions recorded for this period."));
    }

    let total = rows
        .iter()
        .map(|(_, count)| *count)
        .fold(0, u64::saturating_add);
    section.child(
        div()
            .w_full()
            .p(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .children(rows.into_iter().enumerate().map(|(index, (name, count))| {
                let share = count as f64 / total.max(1) as f64;
                let unit = if count == 1 { "session" } else { "sessions" };
                let color = theme.dataviz_color(index);
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .id(("usage-model", index))
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(px(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_primary)
                                    .tooltip({
                                        let name = name.clone();
                                        move |window, cx| {
                                            Tooltip::new(name.clone()).build(window, cx)
                                        }
                                    })
                                    .child(name),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family(FONT_MONO)
                                    .text_size(px(11.5))
                                    .text_color(theme.text_secondary)
                                    .child(format!("{count} {unit} · {:.1}%", share * 100.0)),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(8.0))
                            .rounded_full()
                            .bg(theme.bg_tertiary)
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(share as f32))
                                    .rounded_full()
                                    .bg(color),
                            ),
                    )
            })),
    )
}

fn section_header(theme: &SandTheme, icon: &str, label: &str, detail: String) -> Div {
    gb_section_card_head(theme)
        .flex_wrap()
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(cursor_icon(icon, 15.0, theme.text_secondary))
                .child(
                    div()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text_primary)
                        .child(label.to_owned()),
                ),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(theme.text_tertiary)
                .child(detail),
        )
}

fn empty_state(theme: &SandTheme, text: &str) -> Div {
    div()
        .p(px(20.0))
        .text_size(px(13.0))
        .text_color(theme.text_secondary)
        .child(text.to_owned())
}

fn metric_card(theme: &SandTheme, label: &str, value: String, subtitle: String) -> Div {
    gb_section_card(theme)
        .flex_1()
        .min_w(px(128.0)).max_w_full()
        .p(px(16.0))
        .gap(px(5.0))
        .child(
            div()
                .text_size(px(12.0))
                .text_color(theme.text_secondary)
                .child(label.to_owned()),
        )
        .child(
            div()
                .text_size(px(24.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text_primary)
                .child(value),
        )
        .child(
            div()
                .text_size(px(11.5))
                .text_color(theme.text_tertiary)
                .child(subtitle),
        )
}

fn token_label(value: Option<u64>) -> String {
    value.map(format_tokens).unwrap_or_else(|| "—".into())
}

fn count_label(value: Option<u64>) -> String {
    value.map(|n| n.to_string()).unwrap_or_else(|| "—".into())
}

fn cost_label(value: Option<f64>) -> String {
    match value {
        Some(cost) if (0.0..0.01).contains(&cost) && cost > 0.0 => "<$0.01".into(),
        Some(cost) => format!("${cost:.2}"),
        None => "—".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::{cost_label, session_rows, HermesUsageReport, UsageSummary};
    use serde_json::json;

    #[test]
    fn summary_uses_report_totals_and_keeps_unreported_metrics_unknown() {
        let report: HermesUsageReport = serde_json::from_value(json!({
            "daily": [
                {"day": "2026-11-24", "input_tokens": 50, "output_tokens": 20, "api_calls": 0},
                {"day": "2026-11-25", "input_tokens": 75, "output_tokens": 25}
            ],
            "totals": {"total_input": 150, "total_output": 50, "total_sessions": 2}
        }))
        .unwrap();

        assert_eq!(
            UsageSummary::from_report(&report),
            UsageSummary {
                tokens: Some(200),
                input: Some(150),
                output: Some(50),
                sessions: Some(2),
                cost: None,
                requests: None,
            }
        );
        assert_eq!(cost_label(Some(0.003)), "<$0.01");
        assert_eq!(cost_label(Some(0.0)), "$0.00");
    }

    #[test]
    fn model_counts_keep_full_names_and_account_for_unreported_models() {
        let report: HermesUsageReport = serde_json::from_value(json!({
            "daily": [],
            "by_model": [
                {"model": "provider-one/model", "sessions": 1},
                {"model": "provider-two/model", "sessions": 1},
                {"model": "provider-one/model", "sessions": 2},
                {"model": "auxiliary-only", "sessions": 0}
            ]
        }))
        .unwrap();

        assert_eq!(
            session_rows(&report, Some(6)).unwrap(),
            vec![
                ("provider-one/model".into(), 3),
                ("Model not reported".into(), 2),
                ("provider-two/model".into(), 1),
            ]
        );
        assert!(session_rows(&HermesUsageReport::default(), Some(6)).is_none());
    }
}
