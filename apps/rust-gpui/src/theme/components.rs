use crate::theme::icon_map::cursor_icon_char;
use crate::theme::tokens::SandTheme;
use gpui::{div, prelude::*, px, rgb, Div, FontWeight, Hsla, IntoElement, SharedString, Stateful};

pub const FONT_SANS: &str = "X VF";
pub const FONT_MONO: &str = "X Mono VF";
pub const FONT_ICONS: &str = "Cursor Icons 16";

/// Authentic CursorIcons16 glyph rendered directly with the extracted cursor-icons-16.ttf font.
pub fn cursor_icon(name: &str, size_px: f32, color: Hsla) -> impl IntoElement {
    let ch = cursor_icon_char(name, false);
    div()
        .font_family(FONT_ICONS)
        .font_weight(FontWeight::NORMAL)
        .w(px(size_px))
        .h(px(size_px))
        .text_size(px(size_px))
        .line_height(px(size_px))
        .text_color(color)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(ch.to_string())
}

pub fn cursor_icon_filled(name: &str, size_px: f32, color: Hsla) -> impl IntoElement {
    let ch = cursor_icon_char(name, true);
    div()
        .font_family(FONT_ICONS)
        .font_weight(FontWeight::NORMAL)
        .w(px(size_px))
        .h(px(size_px))
        .text_size(px(size_px))
        .line_height(px(size_px))
        .text_color(color)
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .child(ch.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BtnVariant {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeTone {
    Muted,
    Info,
    Success,
    Warning,
    Danger,
    Accent,
}

pub fn gb_btn(
    id: impl Into<gpui::ElementId>,
    theme: &SandTheme,
    variant: BtnVariant,
    small: bool,
    disabled: bool,
) -> Stateful<Div> {
    let (bg, hover_bg, fg, border) = match variant {
        BtnVariant::Primary => (
            theme.bg_primary,
            theme.bg_primary_hover,
            theme.text_on_primary,
            None,
        ),
        BtnVariant::Secondary => (
            theme.bg_secondary,
            theme.bg_secondary_hover,
            theme.text_primary,
            Some(theme.border_subtle),
        ),
        BtnVariant::Ghost => (
            SandTheme::transparent(),
            theme.bg_hover,
            theme.text_secondary,
            None,
        ),
        BtnVariant::Danger => (
            theme.danger_soft,
            theme.danger_soft,
            theme.danger_text,
            None,
        ),
    };

    let mut d = div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .h(px(if small { 27.0 } else { 32.0 }))
        .px(px(if small { 10.0 } else { 13.0 }))
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_size(px(if small { 12.0 } else { 13.0 }))
        .font_weight(FontWeight::MEDIUM)
        .flex_shrink_0();

    if let Some(b) = border {
        d = d.border_1().border_color(b);
    }
    if disabled {
        d = d.opacity(0.45);
    } else {
        d = d.cursor_pointer().hover(move |s| {
            let s = s.bg(hover_bg);
            if variant == BtnVariant::Ghost {
                s.text_color(fg)
            } else {
                s
            }
        });
    }
    d
}

pub fn gb_badge(theme: &SandTheme, tone: BadgeTone, label: impl Into<SharedString>) -> Div {
    let (bg, fg) = match tone {
        BadgeTone::Muted => (theme.bg_secondary, theme.text_secondary),
        BadgeTone::Info => (theme.accent_soft, theme.accent_text),
        BadgeTone::Success => (theme.success_soft, theme.success_text),
        BadgeTone::Warning => (theme.warn_soft, theme.warn_text),
        BadgeTone::Danger => (theme.danger_soft, theme.danger_text),
        BadgeTone::Accent => (theme.agent_accent_soft, theme.agent_accent),
    };
    div()
        .flex()
        .items_center()
        .gap(px(5.0))
        .h(px(21.0))
        .px(px(8.0))
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .flex_shrink_0()
        .child(label.into())
}

pub fn gb_chip_btn(
    id: impl Into<gpui::ElementId>,
    theme: &SandTheme,
    active: bool,
) -> Stateful<Div> {
    let (bg, fg, border) = if active {
        (
            theme.agent_accent_soft,
            theme.agent_accent,
            theme.agent_accent_border,
        )
    } else {
        (theme.bg_elevated, theme.text_secondary, theme.border_default)
    };
    let hover_bg = theme.bg_elevated_hover;
    let hover_fg = theme.text_primary;
    let hover_border = theme.border_primary;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(5.0))
        .h(px(24.0))
        .px(px(9.0))
        .rounded_full()
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_color(fg)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .cursor_pointer()
        .flex_shrink_0()
        .hover(move |s| {
            s.bg(hover_bg)
                .text_color(hover_fg)
                .border_color(hover_border)
        })
}

pub fn gb_circle_btn(
    id: impl Into<gpui::ElementId>,
    theme: &SandTheme,
    active: bool,
) -> Stateful<Div> {
    let (bg, fg) = if active {
        (theme.bg_primary, theme.text_on_primary)
    } else {
        (theme.bg_elevated, theme.sidebar_text_primary)
    };
    let hover_bg = theme.bg_elevated_hover;
    div()
        .id(id)
        .w(px(34.0))
        .h(px(34.0))
        .rounded_full()
        .bg(bg)
        .border_1()
        .border_color(theme.border_default)
        .text_color(fg)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
}

pub fn gb_window_action(
    id: impl Into<gpui::ElementId>,
    theme: &SandTheme,
    active: bool,
) -> Stateful<Div> {
    let (bg, fg, border) = if active {
        (
            theme.agent_accent_soft,
            theme.agent_accent,
            theme.agent_accent_border,
        )
    } else {
        (theme.bg_elevated, theme.text_secondary, theme.border_default)
    };
    let hover_bg = theme.bg_hover;
    let hover_fg = theme.text_primary;
    div()
        .id(id)
        .w(px(30.0))
        .h(px(30.0))
        .rounded(px(8.0))
        .bg(bg)
        .border_1()
        .border_color(border)
        .text_color(fg)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
}

pub fn gb_mini_icon_btn(
    id: impl Into<gpui::ElementId>,
    theme: &SandTheme,
    danger: bool,
) -> Stateful<Div> {
    let hover_bg = if danger {
        theme.danger_soft
    } else {
        theme.bg_active
    };
    let hover_fg = if danger {
        theme.danger_text
    } else {
        theme.text_primary
    };
    div()
        .id(id)
        .w(px(22.0))
        .h(px(22.0))
        .rounded(px(6.0))
        .text_color(theme.sidebar_text_tertiary)
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
}

pub fn gb_toggle_switch(
    id: impl Into<gpui::ElementId>,
    theme: &SandTheme,
    checked: bool,
) -> Stateful<Div> {
    let bg = if checked {
        theme.accent_primary
    } else {
        theme.bg_active
    };
    div()
        .id(id)
        .w(px(34.0))
        .h(px(20.0))
        .rounded_full()
        .bg(bg)
        .p(px(2.0))
        .flex()
        .items_center()
        .flex_shrink_0()
        .cursor_pointer()
        .when(checked, |d| d.justify_end())
        .when(!checked, |d| d.justify_start())
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .rounded_full()
                .bg(rgb(0xffffff)),
        )
}

pub fn gb_section_card(theme: &SandTheme) -> Div {
    div()
        .w_full()
        .rounded(px(16.0))
        .bg(theme.bg_elevated)
        .border_1()
        .border_color(theme.border_default)
        .overflow_hidden()
        .flex()
        .flex_col()
}

pub fn gb_section_card_head(theme: &SandTheme) -> Div {
    div()
        .w_full()
        .px(px(16.0))
        .py(px(12.0))
        .border_b_1()
        .border_color(theme.border_hairline)
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
}

pub fn gb_row(theme: &SandTheme) -> Div {
    div()
        .w_full()
        .px(px(16.0))
        .py(px(11.0))
        .border_b_1()
        .border_color(theme.border_hairline)
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.0))
}

pub fn gb_segmented_shell(theme: &SandTheme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .p(px(3.0))
        .rounded(px(10.0))
        .bg(theme.bg_tertiary)
        .border_1()
        .border_color(theme.border_default)
}

pub fn gb_segmented_item(
    id: impl Into<gpui::ElementId>,
    theme: &SandTheme,
    active: bool,
    label: impl Into<SharedString>,
) -> Stateful<Div> {
    let hover_fg = theme.text_primary;
    let mut d = div()
        .id(id)
        .h(px(26.0))
        .px(px(11.0))
        .rounded(px(7.0))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .child(label.into());
    if active {
        d = d.bg(theme.bg_elevated).text_color(theme.text_primary);
    } else {
        d = d
            .text_color(theme.text_secondary)
            .hover(move |s| s.text_color(hover_fg));
    }
    d
}

pub fn gb_board_pill(
    id: impl Into<gpui::ElementId>,
    theme: &SandTheme,
    active: bool,
) -> Stateful<Div> {
    let hover_bg = theme.sidebar_hover;
    let hover_fg = theme.sidebar_text_primary;
    let mut d = div()
        .id(id)
        .h(px(26.0))
        .px(px(9.0))
        .rounded_full()
        .text_size(px(12.0))
        .font_weight(FontWeight::MEDIUM)
        .flex()
        .items_center()
        .gap(px(6.0))
        .flex_shrink_0()
        .cursor_pointer();
    if active {
        d = d.bg(theme.bg_primary).text_color(theme.text_on_primary);
    } else {
        d = d
            .bg(theme.bg_tertiary)
            .border_1()
            .border_color(theme.sidebar_separator)
            .text_color(theme.sidebar_text_secondary)
            .hover(move |s| s.bg(hover_bg).text_color(hover_fg));
    }
    d
}

pub fn gb_dot(hex: u32, size_px: f32) -> Div {
    div()
        .w(px(size_px))
        .h(px(size_px))
        .rounded_full()
        .bg(rgb(hex))
        .flex_shrink_0()
}

pub fn gb_traffic_lights() -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(20.0))
        .child(
            div()
                .id("tl-close")
                .w(px(12.0))
                .h(px(12.0))
                .rounded_full()
                .bg(rgb(0xff5f57))
                .cursor_pointer()
                .on_click(|_, window, _| window.remove_window()),
        )
        .child(
            div()
                .id("tl-min")
                .w(px(12.0))
                .h(px(12.0))
                .rounded_full()
                .bg(rgb(0xfebc2e))
                .cursor_pointer()
                .on_click(|_, window, _| window.minimize_window()),
        )
        .child(
            div()
                .id("tl-max")
                .w(px(12.0))
                .h(px(12.0))
                .rounded_full()
                .bg(rgb(0x28c840))
                .cursor_pointer()
                .on_click(|_, window, _| window.zoom_window()),
        )
}

pub fn format_relative_time(ts_secs: Option<f64>) -> String {
    let Some(secs) = ts_secs else {
        return String::new();
    };
    let now = crate::hermes::types::now_secs();
    let diff = (now - secs).max(0.0) as u64;
    if diff < 60 {
        "now".into()
    } else if diff < 3600 {
        format!("{}m", diff / 60)
    } else if diff < 86400 {
        format!("{}h", diff / 3600)
    } else if diff < 86400 * 7 {
        format!("{}d", diff / 86400)
    } else {
        let dt = chrono::DateTime::from_timestamp(secs as i64, 0)
            .unwrap_or_else(chrono::Utc::now);
        dt.format("%b %d").to_string()
    }
}

pub fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", (n as f64) / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", (n as f64) / 1_000.0)
    } else {
        n.to_string()
    }
}

pub fn short_model_name(model: &str) -> String {
    let trimmed = model.trim();
    if trimmed.is_empty() {
        return "Model".into();
    }
    trimmed
        .rsplit('/')
        .next()
        .unwrap_or(trimmed)
        .to_string()
}

pub fn slugify(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "board".into()
    } else {
        trimmed
    }
}
