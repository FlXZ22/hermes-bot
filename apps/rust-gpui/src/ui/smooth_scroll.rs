use gpui::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

/// Exponential damping constant for smooth wheel glide.
/// Higher = snappier, lower = floatier. 18.0 gives Zed-like crisp 120fps glide (~160ms settle).
const SMOOTH_DAMPING: f32 = 18.0;
/// Pixels scrolled per discrete mouse-wheel line notch.
const LINE_STEP_PX: f32 = 48.0;
/// Stop animating and snap when remaining distance falls below this threshold (0% idle CPU).
const SETTLE_EPSILON_PX: f32 = 0.35;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollAxes {
    Vertical,
    Horizontal,
    Both,
}

impl ScrollAxes {
    fn allows_x(self) -> bool {
        matches!(self, Self::Horizontal | Self::Both)
    }

    fn allows_y(self) -> bool {
        matches!(self, Self::Vertical | Self::Both)
    }
}

#[derive(Clone)]
pub enum SmoothScrollTarget {
    Div(ScrollHandle),
    UniformList(UniformListScrollHandle),
    List(ListState),
}

struct SmoothScrollInner {
    target: SmoothScrollTarget,
    pending_x: f32,
    pending_y: f32,
    animating: bool,
    generation: u64,
    last_frame: Option<Instant>,
}

/// Shared registry of nested scroll region hitboxes so an outer `SmoothScrollLayer`
/// never steals mouse-wheel events when the pointer is over an inner textarea, log box,
/// code block, or column scroller.
#[derive(Clone, Default)]
pub struct NestedScrollRegions(Rc<RefCell<Vec<Hitbox>>>);

impl NestedScrollRegions {
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    pub fn push(&self, hitbox: Hitbox) {
        self.0.borrow_mut().push(hitbox);
    }

    pub fn any_hovered(&self, window: &Window) -> bool {
        self.0
            .borrow()
            .iter()
            .any(|hitbox| hitbox.should_handle_scroll(window))
    }

    /// Place as the first child of a `.relative()` scroll wrapper to reset hitboxes each frame.
    pub fn reset_marker(&self) -> impl IntoElement {
        let regions = self.clone();
        canvas(
            move |_, _, _| {
                regions.clear();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .w_full()
        .h(px(0.0))
    }

    /// Place as a zero-layout overlay child inside a `.relative()` nested scrollable container
    /// (such as a Textarea wrapper, log box, or nested column) so the outer page scroller yields.
    pub fn mark_nested(&self) -> impl IntoElement {
        let regions = self.clone();
        canvas(
            move |bounds, window, _| {
                let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
                regions.push(hitbox);
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }
}

/// Native VSync-paced (90Hz / 120Hz / 144Hz / 240Hz) smooth scroll controller.
///
/// - Discrete mouse wheel (`ScrollDelta::Lines`): smoothly interpolated across compositor
///   frames via `window.on_next_frame(...)` using critically-damped exponential decay.
/// - Trackpad / high-precision scroll (`ScrollDelta::Pixels`): cancels any pending wheel
///   glide and passes straight through 1:1 to GPUI with zero added latency.
#[derive(Clone)]
pub struct SmoothScroller {
    inner: Rc<RefCell<SmoothScrollInner>>,
}

impl SmoothScroller {
    pub fn for_handle(handle: ScrollHandle) -> Self {
        Self::new(SmoothScrollTarget::Div(handle))
    }

    pub fn for_uniform_list(handle: UniformListScrollHandle) -> Self {
        Self::new(SmoothScrollTarget::UniformList(handle))
    }

    pub fn for_list(state: ListState) -> Self {
        Self::new(SmoothScrollTarget::List(state))
    }

    fn new(target: SmoothScrollTarget) -> Self {
        Self {
            inner: Rc::new(RefCell::new(SmoothScrollInner {
                target,
                pending_x: 0.0,
                pending_y: 0.0,
                animating: false,
                generation: 0,
                last_frame: None,
            })),
        }
    }

    /// Immediately cancel any in-flight wheel glide (e.g. when user touches trackpad, switches
    /// tabs/filters, or jumps to end). Bumps generation so stale frame callbacks exit cleanly.
    pub fn cancel(&self) {
        let mut inner = self.inner.borrow_mut();
        inner.pending_x = 0.0;
        inner.pending_y = 0.0;
        inner.animating = false;
        inner.generation = inner.generation.wrapping_add(1);
        inner.last_frame = None;
    }

    /// Check whether the underlying scroll target can move in the requested `(dx, dy)` direction.
    /// Prevents claiming/stopping wheel events when the target cannot scroll or is at a boundary.
    pub fn can_scroll(&self, dx: f32, dy: f32) -> bool {
        let target = self.inner.borrow().target.clone();
        match target {
            SmoothScrollTarget::Div(handle) => handle_can_scroll(&handle, dx, dy),
            SmoothScrollTarget::UniformList(handle) => {
                let base = handle.0.borrow().base_handle.clone();
                handle_can_scroll(&base, dx, dy)
            }
            SmoothScrollTarget::List(state) => {
                if dy.abs() <= 0.001 {
                    return false;
                }
                let max = state.max_offset_for_scrollbar();
                if max.y <= px(0.5) {
                    return false;
                }
                let cur = state.scroll_px_offset_for_scrollbar();
                // cur.y is in [-max.y, 0.0]; dy < 0 scrolls down (cur.y decreases toward -max.y)
                if dy < 0.0 {
                    cur.y > -max.y + px(0.5)
                } else {
                    cur.y < px(-0.5)
                }
            }
        }
    }

    /// Queue a smooth wheel delta (in logical pixels, matching GPUI's ScrollEvent delta sign
    /// where negative Y scrolls down). Returns `true` if the target accepted and moved.
    pub fn add_wheel_delta(&self, dx: f32, dy: f32, window: &mut Window, cx: &mut App) -> bool {
        if !self.can_scroll(dx, dy) {
            return false;
        }
        let (should_start, gen) = {
            let mut inner = self.inner.borrow_mut();
            // If user reversed scroll direction, immediately cancel opposite momentum for instant response
            if (inner.pending_y > 0.0 && dy < 0.0) || (inner.pending_y < 0.0 && dy > 0.0) {
                inner.pending_y = 0.0;
            }
            if (inner.pending_x > 0.0 && dx < 0.0) || (inner.pending_x < 0.0 && dx > 0.0) {
                inner.pending_x = 0.0;
            }
            inner.pending_x = (inner.pending_x + dx).clamp(-2400.0, 2400.0);
            inner.pending_y = (inner.pending_y + dy).clamp(-2400.0, 2400.0);
            if !inner.animating {
                inner.animating = true;
                inner.generation = inner.generation.wrapping_add(1);
                inner.last_frame = Some(Instant::now());
                (true, inner.generation)
            } else {
                (false, inner.generation)
            }
        };

        if should_start {
            // Apply an immediate first micro-step on the current frame for 0ms perceived latency
            let keep_going = self.step(0.008, window, cx);
            if keep_going {
                Self::schedule_frame(self.inner.clone(), gen, window);
            }
        }
        true
    }

    fn schedule_frame(inner: Rc<RefCell<SmoothScrollInner>>, gen: u64, window: &mut Window) {
        window.on_next_frame(move |window, cx| {
            let now = Instant::now();
            let dt = {
                let mut state = inner.borrow_mut();
                if !state.animating || state.generation != gen {
                    return;
                }
                let dt = state
                    .last_frame
                    .map(|prev| now.saturating_duration_since(prev).as_secs_f32())
                    .unwrap_or(1.0 / 120.0)
                    .clamp(0.001, 0.05);
                state.last_frame = Some(now);
                dt
            };

            let scroller = SmoothScroller {
                inner: inner.clone(),
            };
            let still_running = scroller.step(dt, window, cx);
            if still_running && inner.borrow().generation == gen {
                Self::schedule_frame(inner, gen, window);
            }
        });
    }

    fn step(&self, dt: f32, window: &mut Window, _cx: &mut App) -> bool {
        let factor = 1.0 - (-dt * SMOOTH_DAMPING).exp();
        let (target, step_x, step_y, keep_going) = {
            let mut inner = self.inner.borrow_mut();
            let sx = if inner.pending_x.abs() < SETTLE_EPSILON_PX {
                let rem = inner.pending_x;
                inner.pending_x = 0.0;
                rem
            } else {
                let s = inner.pending_x * factor;
                inner.pending_x -= s;
                s
            };
            let sy = if inner.pending_y.abs() < SETTLE_EPSILON_PX {
                let rem = inner.pending_y;
                inner.pending_y = 0.0;
                rem
            } else {
                let s = inner.pending_y * factor;
                inner.pending_y -= s;
                s
            };
            let keep = inner.pending_x.abs() >= SETTLE_EPSILON_PX
                || inner.pending_y.abs() >= SETTLE_EPSILON_PX;
            if !keep {
                inner.animating = false;
                inner.last_frame = None;
            }
            (inner.target.clone(), sx, sy, keep)
        };

        let mut moved = false;
        match target {
            SmoothScrollTarget::Div(handle) => {
                moved = apply_scroll_handle_delta(&handle, step_x, step_y);
            }
            SmoothScrollTarget::UniformList(handle) => {
                let base = handle.0.borrow().base_handle.clone();
                moved = apply_scroll_handle_delta(&base, step_x, step_y);
            }
            SmoothScrollTarget::List(state) => {
                if step_y.abs() > 0.001 {
                    let before = state.scroll_px_offset_for_scrollbar();
                    state.scroll_by(px(-step_y));
                    let after = state.scroll_px_offset_for_scrollbar();
                    moved = (after.y - before.y).abs() > px(0.01);
                }
            }
        }

        if moved {
            window.refresh();
            keep_going
        } else {
            // Hit scroll boundary cleanly — stop animating immediately (0% wasted frames)
            self.cancel();
            false
        }
    }
}

fn handle_can_scroll(handle: &ScrollHandle, dx: f32, dy: f32) -> bool {
    let old = handle.offset();
    let max = handle.max_offset();
    let can_x = dx.abs() > 0.001
        && max.x > px(0.5)
        && if dx < 0.0 {
            old.x > -max.x + px(0.5)
        } else {
            old.x < px(-0.5)
        };
    let can_y = dy.abs() > 0.001
        && max.y > px(0.5)
        && if dy < 0.0 {
            old.y > -max.y + px(0.5)
        } else {
            old.y < px(-0.5)
        };
    can_x || can_y
}

fn apply_scroll_handle_delta(handle: &ScrollHandle, dx: f32, dy: f32) -> bool {
    let old = handle.offset();
    let max = handle.max_offset();
    let next_x = if dx.abs() > 0.001 {
        (old.x + px(dx)).clamp(-max.x, px(0.0))
    } else {
        old.x
    };
    let next_y = if dy.abs() > 0.001 {
        (old.y + px(dy)).clamp(-max.y, px(0.0))
    } else {
        old.y
    };
    if (next_x - old.x).abs() < px(0.01) && (next_y - old.y).abs() < px(0.01) {
        return false;
    }
    handle.set_offset(point(next_x, next_y));
    true
}

/// Transparent zero-layout overlay element that intercepts discrete mouse-wheel `ScrollDelta::Lines`
/// events along the vertical axis and drives `SmoothScroller` at native monitor VSync.
pub fn smooth_scroll_layer(scroller: &SmoothScroller) -> SmoothScrollLayer {
    SmoothScrollLayer {
        scroller: scroller.clone(),
        axes: ScrollAxes::Vertical,
        handles_event: None,
    }
}

/// Horizontal-only smooth wheel overlay (responds to horizontal wheel or Shift+vertical wheel,
/// leaving ordinary vertical wheel events untouched for nested vertical columns).
pub fn smooth_scroll_layer_x(scroller: &SmoothScroller) -> SmoothScrollLayer {
    SmoothScrollLayer {
        scroller: scroller.clone(),
        axes: ScrollAxes::Horizontal,
        handles_event: None,
    }
}

#[allow(dead_code)]
pub fn smooth_scroll_layer_xy(scroller: &SmoothScroller) -> SmoothScrollLayer {
    SmoothScrollLayer {
        scroller: scroller.clone(),
        axes: ScrollAxes::Both,
        handles_event: None,
    }
}

pub struct SmoothScrollLayer {
    scroller: SmoothScroller,
    axes: ScrollAxes,
    handles_event: Option<Rc<dyn Fn(&ScrollEvent, &Window) -> bool>>,
}

impl SmoothScrollLayer {
    pub fn filter_events(mut self, filter: impl Fn(&ScrollEvent, &Window) -> bool + 'static) -> Self {
        self.handles_event = Some(Rc::new(filter));
        self
    }

    pub fn yield_to(self, nested: &NestedScrollRegions) -> Self {
        let nested = nested.clone();
        self.filter_events(move |_, window| !nested.any_hovered(window))
    }
}

impl Element for SmoothScrollLayer {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.position = Position::Absolute;
        style.inset = Edges::all(Length::Definite(px(0.0).into()));
        let layout_id = window.request_layout(style, [], cx);
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        _cx: &mut App,
    ) -> Hitbox {
        // Normal (non-blocking) hitbox so clicks, drags, and hovers pass straight through
        window.insert_hitbox(bounds, HitboxBehavior::Normal)
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        hitbox: &mut Hitbox,
        window: &mut Window,
        _cx: &mut App,
    ) {
        let scroller = self.scroller.clone();
        let axes = self.axes;
        let handles_event = self.handles_event.clone();
        let hitbox = hitbox.clone();
        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || !hitbox.should_handle_scroll(window) {
                return;
            }
            if handles_event.as_ref().is_some_and(|filter| !filter(event, window)) {
                scroller.cancel();
                return;
            }
            match event.delta {
                ScrollDelta::Lines(lines) => {
                    let (dx, dy) = if axes.allows_x() && event.modifiers.shift && lines.x == 0.0 {
                        (lines.y * LINE_STEP_PX, 0.0)
                    } else {
                        (
                            if axes.allows_x() {
                                lines.x * LINE_STEP_PX
                            } else {
                                0.0
                            },
                            if axes.allows_y() {
                                lines.y * LINE_STEP_PX
                            } else {
                                0.0
                            },
                        )
                    };
                    if (dx.abs() > 0.001 || dy.abs() > 0.001)
                        && scroller.add_wheel_delta(dx, dy, window, cx)
                    {
                        cx.stop_propagation();
                    }
                }
                ScrollDelta::Pixels(_) => {
                    // Trackpad / precision wheel: cancel wheel inertia and let GPUI scroll 1:1
                    scroller.cancel();
                }
            }
        });
    }
}
