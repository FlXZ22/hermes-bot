//! Compositor-driven UI motion. Finite transitions stop at rest; live
//! indicators request frames only while visible and active.

use gpui::*;
use std::f32::consts::TAU;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub fn ease_out_cubic(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

pub fn ease_out_quint(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(5)
}

pub fn ease_out_back_soft(t: f32) -> f32 {
    let x = t - 1.0;
    1.0 + x * x * (1.65 * x + 0.65)
}

/// A non-overshooting, retargetable scalar with an analytic exponential step.
pub struct SpringValue {
    current: f32,
    target: f32,
    last_tick: Option<Instant>,
    epsilon: f32,
}

impl SpringValue {
    const RATE: f32 = 28.0;

    pub fn new(initial: f32) -> Self {
        Self {
            current: initial,
            target: initial,
            last_tick: None,
            epsilon: 0.001,
        }
    }

    pub fn with_precision(mut self, epsilon: f32) -> Self {
        self.epsilon = epsilon;
        self
    }

    pub fn set_target(&mut self, target: f32) {
        if target == self.target {
            return;
        }
        self.target = target;
        self.last_tick.get_or_insert_with(Instant::now);
    }

    pub fn snap(&mut self, value: f32) {
        self.current = value;
        self.target = value;
        self.last_tick = None;
    }

    pub fn is_animating(&self) -> bool {
        self.last_tick.is_some()
    }

    pub fn value(&self) -> f32 {
        self.current
    }

    pub fn advance(&mut self, window: &mut Window) -> f32 {
        if self.is_animating() {
            self.tick(Instant::now());
            if self.is_animating() {
                window.request_animation_frame();
            }
        }
        self.current
    }

    fn tick(&mut self, now: Instant) {
        let Some(last) = self.last_tick else { return };
        let dt = now
            .saturating_duration_since(last)
            .as_secs_f32()
            .clamp(0.001, 0.05);
        let remaining = (self.target - self.current) * (-Self::RATE * dt).exp();
        // For exponential decay, speed is RATE * |remaining|. At this
        // tolerance both the positional error and the residual speed settle.
        if remaining.abs() <= self.epsilon {
            self.snap(self.target);
        } else {
            self.current = self.target - remaining;
            self.last_tick = Some(now);
        }
    }
}

/// Keep the ID unchanged on repaint, even when returning to a previously seen view.
pub struct EntranceKey<K> {
    value: Option<K>,
    generation: usize,
}

impl<K: PartialEq> EntranceKey<K> {
    pub fn new() -> Self {
        Self {
            value: None,
            generation: 0,
        }
    }

    pub fn id(&mut self, slot: &'static str, value: K) -> ElementId {
        if self.value.as_ref() != Some(&value) {
            self.value = Some(value);
            self.generation = self.generation.wrapping_add(1);
        }
        (slot, self.generation).into()
    }
}

#[derive(Default)]
pub struct ModalMotion {
    open: bool,
    generation: usize,
}

impl ModalMotion {
    pub fn set_open(&mut self, open: bool) {
        if open && !self.open {
            self.generation = self.generation.wrapping_add(1);
        }
        self.open = open;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn backdrop(&self, id: &'static str, color: Hsla) -> impl IntoElement {
        // A separate scrim layer avoids multiplying the card's fade by the backdrop's.
        modal_backdrop_enter((id, self.generation), div().absolute().inset_0().bg(color))
    }

    pub fn card<E: IntoElement + Styled + 'static>(
        &self,
        id: &'static str,
        el: E,
    ) -> impl IntoElement {
        modal_card_enter((id, self.generation), el)
    }
}

pub fn modal_backdrop_enter<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    el: E,
) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(160)).with_easing(ease_out_cubic),
        |el, t| el.opacity(t),
    )
}

pub fn modal_card_enter<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    el: E,
) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(200)).with_easing(ease_out_back_soft),
        |el, t| {
            el.opacity(0.25 + 0.75 * t.min(1.0))
                .mt(px(10.0 * (1.0 - t)))
        },
    )
}

pub fn popover_enter<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    el: E,
) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(140)).with_easing(ease_out_cubic),
        |el, t| {
            // A relative offset keeps virtual list row heights constant during the reveal.
            el.relative().top(px(6.0 * (1.0 - t))).opacity(t)
        },
    )
}

pub fn toast_enter<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    el: E,
) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(200)).with_easing(ease_out_quint),
        |el, t| el.bottom(px(20.0 - 12.0 * (1.0 - t))).opacity(t),
    )
}

pub fn view_enter<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    el: E,
) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(160)).with_easing(ease_out_cubic),
        |el, t| {
            if t < 1.0 {
                // Faces inside a fading pane subtree need its inherited opacity.
                crate::face::FaceLayer::suspend_capture();
            }
            el.relative()
                .top(px(4.0 * (1.0 - t)))
                .opacity(0.7 + 0.3 * t)
        },
    )
}

pub fn pill_enter<E: IntoElement + Styled + 'static>(
    id: impl Into<ElementId>,
    el: E,
) -> impl IntoElement {
    el.with_animation(
        id,
        Animation::new(Duration::from_millis(160)).with_easing(ease_out_cubic),
        |el, t| el.relative().top(px(6.0 * (1.0 - t))).opacity(t),
    )
}

pub fn gpu_spinner(size_px: f32, color: Hsla) -> impl IntoElement {
    Indicator {
        width: size_px,
        height: size_px,
        color,
        kind: IndicatorKind::Spinner,
    }
}

pub fn typing_dots(color: Hsla) -> impl IntoElement {
    Indicator {
        width: 20.0,
        height: 12.0,
        color,
        kind: IndicatorKind::Typing,
    }
}

pub fn live_pulse_dot(color: Hsla, size_px: f32, active: bool) -> impl IntoElement {
    Indicator {
        width: size_px + 4.0,
        height: size_px + 4.0,
        color,
        kind: IndicatorKind::Pulse { size_px, active },
    }
}

/// Stateful only while mounted, underneath the switch's own stable ID.
pub fn toggle_thumb(checked: bool) -> impl IntoElement {
    let target = if checked { 14.0 } else { 0.0 };
    canvas(
        move |bounds, window, cx| {
            let state = window.use_keyed_state("switch-thumb", cx, |_, _| {
                SpringValue::new(target).with_precision(0.1)
            });
            state.update(cx, |value, _| {
                if bounds.intersects(&window.content_mask().bounds) {
                    value.set_target(target);
                    value.advance(window)
                } else {
                    value.snap(target);
                    target
                }
            })
        },
        |bounds, offset, window, _| {
            let center = bounds.origin + point(px(8.0 + offset), px(8.0));
            paint_dot(window, center, 16.0, rgb(0xffffff).into());
        },
    )
    .w(px(30.0))
    .h(px(16.0))
    .flex_shrink_0()
}

enum IndicatorKind {
    Spinner,
    Typing,
    Pulse { size_px: f32, active: bool },
}

struct Indicator {
    width: f32,
    height: f32,
    color: Hsla,
    kind: IndicatorKind,
}

impl Element for Indicator {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size = size(px(self.width).into(), px(self.height).into());
        style.flex_shrink = 0.0;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        if self.width <= 0.0 || !bounds.intersects(&window.content_mask().bounds) {
            return;
        }

        let center = bounds.origin + point(bounds.size.width / 2.0, bounds.size.height / 2.0);
        match self.kind {
            IndicatorKind::Spinner => {
                if let Ok(path) = spinner_arc(center, self.width, phase(0.86)).build() {
                    window.paint_path(path, self.color);
                }
            }
            IndicatorKind::Typing => {
                let phase = phase(1.05);
                for i in 0..3 {
                    let wave = ((phase - i as f32 * 0.95).cos() + 1.0) * 0.5;
                    let dot = center + point(px((i as f32 - 1.0) * 6.0), px(1.0 - 2.5 * wave));
                    paint_dot(window, dot, 3.5, self.color.opacity(0.4 + 0.6 * wave));
                }
            }
            IndicatorKind::Pulse { size_px, active } => {
                if active {
                    let wave = (1.0 - phase(1.6).cos()) * 0.5;
                    paint_dot(
                        window,
                        center,
                        size_px + 4.0 * wave,
                        self.color.opacity(0.24 * (1.0 - wave)),
                    );
                }
                paint_dot(window, center, size_px, self.color);
                if !active {
                    return;
                }
            }
        }
        window.request_animation_frame();
    }
}

fn phase(period: f64) -> f32 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    let elapsed = EPOCH.get_or_init(Instant::now).elapsed().as_secs_f64();
    (elapsed % period / period) as f32 * TAU
}

fn paint_dot(window: &mut Window, center: Point<Pixels>, diameter: f32, color: Hsla) {
    let radius = px(diameter / 2.0);
    let bounds = Bounds::new(
        center - point(radius, radius),
        size(px(diameter), px(diameter)),
    );
    window.paint_quad(quad(
        bounds,
        Corners::all(radius),
        color,
        Edges::default(),
        transparent_black(),
        BorderStyle::Solid,
    ));
}

fn spinner_arc(center: Point<Pixels>, diameter: f32, angle: f32) -> PathBuilder {
    let width = (diameter * 0.115).clamp(1.0, 2.4);
    let radius = (diameter - width) * 0.5;
    let mut path = PathBuilder::stroke(px(width)).with_style(PathStyle::Stroke(StrokeOptions {
        line_width: width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        miter_limit: 4.0,
    }));
    let position = |x, y| center + point(px(radius * x), px(radius * y));
    let (mut y, mut x) = angle.sin_cos();
    path.move_to(position(x, y));
    // Exactly three cubic quarter circles, with tangent-continuous joins.
    let k = 0.552_284_8;
    for _ in 0..3 {
        let (nx, ny) = (-y, x);
        path.cubic_bezier_to(
            position(nx, ny),
            position(x - k * y, y + k * x),
            position(nx + k * ny, ny - k * nx),
        );
        (x, y) = (nx, ny);
    }
    path
}

#[cfg(test)]
mod tests {
    use super::{
        ease_out_back_soft, ease_out_cubic, ease_out_quint, spinner_arc, ModalMotion, SpringValue,
    };
    use gpui::{point, px};
    use std::{f32::consts::TAU, time::Duration};

    #[test]
    fn easings_have_exact_endpoints_and_finite_values() {
        for ease in [
            ease_out_cubic as fn(f32) -> f32,
            ease_out_quint,
            ease_out_back_soft,
        ] {
            assert_eq!(ease(0.0), 0.0);
            assert_eq!(ease(1.0), 1.0);
            for i in 0..=100 {
                let value = ease(i as f32 / 100.0);
                assert!(value.is_finite() && (0.0..=1.02).contains(&value));
            }
        }
    }

    #[test]
    fn spring_retargets_and_settles_at_native_refresh_rates() {
        for hz in [60, 120, 240] {
            let mut spring = SpringValue::new(280.0).with_precision(0.1);
            let dt = Duration::from_secs_f64(1.0 / f64::from(hz));
            spring.set_target(68.0);
            let mut now = spring.last_tick.unwrap();
            for _ in 0..hz / 10 {
                now += dt;
                let previous = spring.current;
                spring.tick(now);
                assert!((68.0..previous).contains(&spring.current));
            }
            spring.set_target(280.0);
            for _ in 0..hz / 2 {
                now += dt;
                spring.tick(now);
            }
            assert_eq!(spring.current, 280.0);
            assert!(!spring.is_animating());
            spring.set_target(280.0);
            assert!(!spring.is_animating());
        }
    }

    #[test]
    fn modal_key_stays_stable_until_the_next_open() {
        let mut motion = ModalMotion::default();
        motion.set_open(true);
        let first = motion.generation;
        motion.set_open(true);
        assert_eq!(motion.generation, first);
        motion.set_open(false);
        motion.set_open(true);
        assert_ne!(motion.generation, first);
    }

    #[test]
    fn spinner_builds_at_all_phases_and_icon_sizes() {
        for size in [11.0, 13.0, 18.0, 32.0] {
            for i in 0..16 {
                let center = point(px(25.0), px(40.0));
                assert!(spinner_arc(center, size, i as f32 * TAU / 16.0)
                    .build()
                    .is_ok());
            }
        }
    }
}
