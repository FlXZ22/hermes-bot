use crate::state::app_state::AppState;
use crate::theme::components::{
    cursor_icon, format_tokens, gb_btn, gb_section_card, gb_section_card_head, short_model_name,
    BtnVariant, FONT_MONO, FONT_SANS,
};
use gpui::*;
use std::collections::HashMap;

pub struct AnalyticsView {
    app: Entity<AppState>,
}

impl AnalyticsView {
    pub fn new(app: Entity<AppState>, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self { app }
    }
}

impl Render for AnalyticsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (theme, usage_points, sessions, tasks, boards, has_client) = {
            let s = self.app.read(cx);
            (
                s.sand_theme(),
                s.usage_points.clone(),
                s.sessions.clone(),
                s.tasks.clone(),
                s.boards.clone(),
                s.client.is_some(),
            )
        };

        let mut total_tokens: u64 = 0;
        let mut total_cost: f64 = 0.0;
        let mut total_requests: u64 = 0;
        for pt in &usage_points {
            let t = pt
                .tokens
                .unwrap_or_else(|| pt.input_tokens.unwrap_or(0) + pt.output_tokens.unwrap_or(0));
            total_tokens += t;
            total_cost += pt.cost_usd.unwrap_or(0.0);
            total_requests += pt.requests.unwrap_or(0);
        }

        // Last 14 days for bar chart
        let chart_points: Vec<(String, u64, f64)> = usage_points
            .iter()
            .rev()
            .take(14)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .map(|pt| {
                let t = pt
                    .tokens
                    .unwrap_or_else(|| pt.input_tokens.unwrap_or(0) + pt.output_tokens.unwrap_or(0));
                (pt.date.clone(), t, pt.cost_usd.unwrap_or(0.0))
            })
            .collect();
        let max_bar = chart_points
            .iter()
            .map(|(_, t, _)| *t)
            .max()
            .unwrap_or(1)
            .max(1);

        // Sessions by model breakdown
        let mut by_model: HashMap<String, usize> = HashMap::new();
        for s in &sessions {
            let m = s
                .model
                .as_deref()
                .filter(|m| !m.trim().is_empty())
                .map(short_model_name)
                .unwrap_or_else(|| "default".into());
            *by_model.entry(m).or_insert(0) += 1;
        }
        let mut model_rows: Vec<(String, usize)> = by_model.into_iter().collect();
        model_rows.sort_by(|a, b| b.1.cmp(&a.1));
        let max_model_count = model_rows.first().map(|(_, c)| *c).unwrap_or(1).max(1);

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.bg_pane)
            .font_family(FONT_SANS)
            .child(
                div()
                    .id("analytics-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .p(px(28.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .w_full()
                            .max_w(px(940.0))
                            .flex()
                            .flex_col()
                            .gap(px(18.0))
                            // Header
                            .child(
                                div()
                                    .flex()
                                    .items_start()
                                    .justify_between()
                                    .gap(px(12.0))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .text_size(px(22.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(theme.text_primary)
                                                    .child("Usage & Analytics"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .text_color(theme.text_secondary)
                                                    .child("Token volume, spend estimates, and session distribution across models."),
                                            ),
                                    )
                                    .child(
                                        gb_btn("analytics-refresh", &theme, BtnVariant::Secondary, false, !has_client)
                                            .child(cursor_icon("refresh", 13.0, theme.text_primary))
                                            .child("Refresh")
                                            .on_click(cx.listener(|v, _, _, cx| {
                                                v.app.update(cx, |s, cx| s.refresh_capabilities(false, cx));
                                            })),
                                    ),
                            )
                            // 4 Stat cards
                            .child(
                                div()
                                    .w_full()
                                    .grid()
                                    .grid_cols(4)
                                    .gap(px(12.0))
                                    .child(metric_card(
                                        &theme,
                                        "Total Tokens (30d)",
                                        format_tokens(total_tokens),
                                        format!("{total_requests} requests"),
                                    ))
                                    .child(metric_card(
                                        &theme,
                                        "Estimated Spend",
                                        format!("${total_cost:.2}"),
                                        "Across active providers".into(),
                                    ))
                                    .child(metric_card(
                                        &theme,
                                        "Sessions",
                                        sessions.len().to_string(),
                                        format!(
                                            "{} pinned",
                                            sessions.iter().filter(|s| s.pinned).count()
                                        ),
                                    ))
                                    .child(metric_card(
                                        &theme,
                                        "Board Tasks",
                                        tasks.len().to_string(),
                                        format!("Across {} boards", boards.len()),
                                    )),
                            )
                            // 14-day Token Bar Chart
                            .child(
                                gb_section_card(&theme)
                                    .child(
                                        gb_section_card_head(&theme)
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .child(cursor_icon("three-bars", 15.0, theme.text_secondary))
                                                    .child(
                                                        div()
                                                            .text_size(px(13.5))
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(theme.text_primary)
                                                            .child("Daily Token Usage (Last 14 Days)"),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .font_family(FONT_MONO)
                                                    .text_size(px(11.5))
                                                    .text_color(theme.text_tertiary)
                                                    .child(format!("Peak: {}", format_tokens(max_bar))),
                                            ),
                                    )
                                    .child(if chart_points.is_empty() {
                                        div()
                                            .p(px(28.0))
                                            .text_size(px(13.0))
                                            .text_color(theme.text_secondary)
                                            .child("No usage telemetry points recorded yet.")
                                    } else {
                                        div()
                                            .w_full()
                                            .px(px(18.0))
                                            .py(px(18.0))
                                            .flex()
                                            .items_end()
                                            .gap(px(8.0))
                                            .h(px(200.0))
                                            .children(chart_points.iter().enumerate().map(|(idx, (date, tokens, _cost))| {
                                                let ratio = (*tokens as f32 / max_bar as f32).clamp(0.04, 1.0);
                                                let bar_h = (ratio * 132.0).max(6.0);
                                                let short_date = if date.len() >= 5 {
                                                    &date[date.len() - 5..]
                                                } else {
                                                    date.as_str()
                                                };
                                                let color = theme.dataviz_color(idx);
                                                div()
                                                    .flex_1()
                                                    .h_full()
                                                    .flex()
                                                    .flex_col()
                                                    .items_center()
                                                    .justify_end()
                                                    .gap(px(6.0))
                                                    .child(
                                                        div()
                                                            .font_family(FONT_MONO)
                                                            .text_size(px(10.0))
                                                            .text_color(theme.text_tertiary)
                                                            .child(format_tokens(*tokens)),
                                                    )
                                                    .child(
                                                        div()
                                                            .w_full()
                                                            .max_w(px(34.0))
                                                            .h(px(bar_h))
                                                            .rounded_t(px(6.0))
                                                            .bg(color),
                                                    )
                                                    .child(
                                                        div()
                                                            .font_family(FONT_MONO)
                                                            .text_size(px(10.0))
                                                            .text_color(theme.text_secondary)
                                                            .child(short_date.to_string()),
                                                    )
                                            }))
                                    }),
                            )
                            // Sessions by Model horizontal bars
                            .child(
                                gb_section_card(&theme)
                                    .child(
                                        gb_section_card_head(&theme).child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .child(cursor_icon("chip", 15.0, theme.text_secondary))
                                                .child(
                                                    div()
                                                        .text_size(px(13.5))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(theme.text_primary)
                                                        .child("Sessions by Model"),
                                                ),
                                        ),
                                    )
                                    .child(if model_rows.is_empty() {
                                        div()
                                            .p(px(24.0))
                                            .text_size(px(13.0))
                                            .text_color(theme.text_secondary)
                                            .child("Start chatting to see model usage breakdown.")
                                    } else {
                                        div()
                                            .w_full()
                                            .p(px(16.0))
                                            .flex()
                                            .flex_col()
                                            .gap(px(11.0))
                                            .children(model_rows.iter().take(10).enumerate().map(
                                                |(idx, (model, count))| {
                                                    let pct = (*count as f32 / max_model_count as f32)
                                                        .clamp(0.05, 1.0);
                                                    let color = theme.dataviz_color(idx);
                                                    div()
                                                        .w_full()
                                                        .flex()
                                                        .flex_col()
                                                        .gap(px(4.0))
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .items_center()
                                                                .justify_between()
                                                                .child(
                                                                    div()
                                                                        .text_size(px(12.5))
                                                                        .font_weight(FontWeight::MEDIUM)
                                                                        .text_color(theme.text_primary)
                                                                        .child(model.clone()),
                                                                )
                                                                .child(
                                                                    div()
                                                                        .font_family(FONT_MONO)
                                                                        .text_size(px(12.0))
                                                                        .text_color(theme.text_secondary)
                                                                        .child(format!("{count} sessions")),
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
                                                                        .w(relative(pct))
                                                                        .rounded_full()
                                                                        .bg(color),
                                                                ),
                                                        )
                                                },
                                            ))
                                    }),
                            ),
                    ),
            )
    }
}

fn metric_card(
    theme: &crate::theme::tokens::SandTheme,
    label: &str,
    value: String,
    subtitle: String,
) -> Div {
    div()
        .rounded(px(16.0))
        .bg(theme.bg_elevated)
        .border_1()
        .border_color(theme.border_default)
        .p(px(16.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_secondary)
                .child(label.to_string()),
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
