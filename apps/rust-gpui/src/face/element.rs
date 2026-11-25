use super::{
    clips::{fill_scale, STILL_FILL},
    engine::FaceMotionHost,
    math::*,
    morphs::Primitive,
    render::{render_tree, RenderTree, LOGICAL_SIZE},
    shapes::Body,
};
use gpui::*;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BotFaceOptions {
    pub slot_key: &'static str,
    pub is_static: bool,
    pub follow_pointer: bool,
    pub clickable_spin: bool,
    /// `None` follows the native compositor rate.
    pub max_fps: Option<f32>,
}
pub(crate) struct BotFaceElement {
    key: u64,
    props: FaceProps,
}

#[derive(Clone, Copy, PartialEq)]
struct FaceProps {
    seed: u32,
    body: Body,
    state: &'static str,
    size: f32,
    ink: Hsla,
    options: BotFaceOptions,
}

type Paths = Vec<(Path, Hsla)>;

#[derive(Clone, Copy, PartialEq, Eq)]
struct StaticPathKey {
    body: Body,
    size: u32,
    sleeping: bool,
    bounds: [u32; 4],
    ink: [u32; 4],
}

#[derive(Default)]
struct StaticPaths(VecDeque<(StaticPathKey, Rc<Paths>)>);

impl StaticPaths {
    const CAPACITY: usize = 128;

    fn get(&mut self, props: FaceProps, bounds: Bounds<Pixels>) -> Rc<Paths> {
        let key = StaticPathKey {
            body: props.body,
            size: props.size.to_bits(),
            sleeping: props.state == "sleeping",
            bounds: [bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height]
                .map(|p| f32::from(p).to_bits()),
            ink: [props.ink.h, props.ink.s, props.ink.l, props.ink.a].map(f32::to_bits),
        };
        if let Some(index) = self.0.iter().position(|(cached, _)| *cached == key) {
            let entry = self.0.remove(index).unwrap();
            let paths = entry.1.clone();
            self.0.push_front(entry);
            return paths;
        }
        let tree = render_tree(&FaceMotionHost::still(key.body, key.sleeping), props.size as f64);
        let paths = Rc::new(build_paths(&tree, &canvas_at(bounds, props.size), props.ink));
        if self.0.len() == Self::CAPACITY {
            self.0.pop_back();
        }
        self.0.push_front((key, paths.clone()));
        paths
    }
}

thread_local! {
    static STATIC_PATHS: RefCell<StaticPaths> = RefCell::default();
    static FACE_LAYER: RefCell<Option<FaceLayer>> = const { RefCell::new(None) };
}

impl BotFaceElement {
    pub fn new(
        seed: &str,
        shape: &str,
        state: &'static str,
        size: f32,
        ink: Hsla,
        options: BotFaceOptions,
        key: u64,
    ) -> Self {
        let seed = seed.encode_utf16().fold(2_166_136_261_u32, |h, c| {
            (h ^ u32::from(c)).wrapping_mul(16_777_619)
        });
        let size = if size.is_finite() { size.max(0.) } else { 0. };
        Self {
            key,
            props: FaceProps { seed, body: Body::named(shape), state, size, ink, options },
        }
    }
}

/// Cached pane content and its live faces must be dispatch-tree siblings: GPUI
/// propagates a view's invalidation through all its ancestors. Anchors still live
/// in the pane, so scroll clipping, layout, and click hitboxes stay in the pane.
#[derive(Clone, Default)]
pub(crate) struct FaceLayer(Rc<RefCell<FaceMounts>>);

#[derive(Default)]
struct FaceMounts {
    suspended: bool,
    faces: Vec<FaceMount>,
}

struct FaceMount {
    view: Entity<BotFaceView>,
    bounds: Bounds<Pixels>,
    mask: ContentMask<Pixels>,
    revision: usize,
    visible: Rc<Cell<bool>>,
}

impl FaceLayer {
    pub(crate) fn wrap(&self, content: impl IntoElement) -> impl IntoElement {
        LayerElement {
            layer: self.clone(),
            content: content.into_any_element(),
        }
    }

    fn scope<R>(&self, draw: impl FnOnce() -> R) -> R {
        let previous = FACE_LAYER.replace(Some(self.clone()));
        let result = draw();
        FACE_LAYER.set(previous);
        result
    }

    /// Called only on a content cache miss. When GPUI replays the pane, its
    /// existing anchors are still valid and no new ones are collected.
    pub(crate) fn begin_pane() {
        FACE_LAYER.with_borrow(|layer| {
            if let Some(layer) = layer {
                *layer.0.borrow_mut() = FaceMounts::default();
            }
        });
    }

    pub(crate) fn suspend_capture() {
        FACE_LAYER.with_borrow(|layer| {
            if let Some(layer) = layer {
                layer.0.borrow_mut().suspended = true;
            }
        });
    }

    fn capture(mount: FaceMount) -> bool {
        FACE_LAYER.with_borrow(|layer| {
            let Some(layer) = layer else { return false };
            let mut mounts = layer.0.borrow_mut();
            if mounts.suspended {
                return false;
            }
            mounts.faces.push(mount);
            true
        })
    }
}

struct LayerElement {
    layer: FaceLayer,
    content: AnyElement,
}

struct MountedElement {
    element: AnyElement,
    mask: ContentMask<Pixels>,
    revision: usize,
    visible: Rc<Cell<bool>>,
}

impl Element for LayerElement {
    type RequestLayoutState = ();
    type PrepaintState = Vec<MountedElement>;

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
        let id = self.layer.scope(|| self.content.request_layout(window, cx));
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.layer.scope(|| self.content.prepaint(window, cx));

        self.layer.0.borrow().faces.iter().map(|mount| {
            // Reuse the GPUI paint range (including its already-binned GPU paths)
            // until this face ticks, changes appearance, or moves.
            let style = StyleRefinement {
                size: SizeRefinement {
                    width: Some(mount.bounds.size.width.into()),
                    height: Some(mount.bounds.size.height.into()),
                },
                ..Default::default()
            };
            let mut element = mount.view.clone().cached(style).into_any_element();
            window.with_content_mask(Some(mount.mask), |window| {
                window.with_element_namespace(("face-props", mount.revision), |window| {
                    element.prepaint_as_root(
                        mount.bounds.origin, mount.bounds.size.into(), window, cx,
                    );
                });
            });
            MountedElement {
                element,
                mask: mount.mask,
                revision: mount.revision,
                visible: mount.visible.clone(),
            }
        }).collect()
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        faces: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.content.paint(window, cx);
        // A virtual list can speculatively prepaint rows that it doesn't paint.
        // Only keep anchors committed by the pane's paint pass.
        self.layer.0.borrow_mut().faces.retain(|face| face.visible.get());
        for face in faces {
            if face.visible.get() {
                window.with_content_mask(Some(face.mask), |window| {
                    window.with_element_namespace(("face-props", face.revision), |window| {
                        face.element.paint(window, cx);
                    });
                });
            }
        }
    }
}

enum FacePaint {
    None,
    Static,
    Inline(AnyElement),
    Layer(Rc<Cell<bool>>),
}

pub(crate) struct FacePrepaint(FacePaint, Option<(Hitbox, WeakEntity<BotFaceView>)>);

impl Element for BotFaceElement {
    type RequestLayoutState = ();
    type PrepaintState = FacePrepaint;

    fn id(&self) -> Option<ElementId> {
        Some(SharedString::from(format!("bot-face-{:016x}", self.key)).into())
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
        (face_layout(self.props.size, window, cx), ())
    }
    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let mask = window.content_mask();
        if self.props.size <= 0. || !bounds.intersects(&mask.bounds) {
            return FacePrepaint(FacePaint::None, None);
        }
        if self.props.options.is_static {
            return FacePrepaint(FacePaint::Static, None);
        }

        // Unlike use_keyed_state, this doesn't add an observer that notifies
        // the layout owner on every animation tick.
        let view = window.with_element_state(id.unwrap(), |view: Option<Entity<BotFaceView>>, _| {
            let view = view.unwrap_or_else(|| cx.new(|_| BotFaceView::new(self.props)));
            (view.clone(), view)
        });
        let revision = if view.read(cx).props != self.props {
            view.update(cx, |face, _| face.set_props(self.props))
        } else {
            view.read(cx).revision
        };
        let hitbox = self.props.options.clickable_spin.then(|| {
            (window.insert_hitbox(bounds, HitboxBehavior::Normal), view.downgrade())
        });
        let visible = Rc::new(Cell::new(false));
        if FaceLayer::capture(FaceMount { view: view.clone(), bounds, mask, revision, visible: visible.clone() }) {
            return FacePrepaint(FacePaint::Layer(visible), hitbox);
        }

        let mut element = view.into_any_element();
        element.prepaint_as_root(bounds.origin, bounds.size.into(), window, cx);
        FacePrepaint(FacePaint::Inline(element), hitbox)
    }
    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        FacePrepaint(face, hitbox): &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        match face {
            FacePaint::None => (),
            FacePaint::Static => {
                let paths = STATIC_PATHS.with_borrow_mut(|cache| cache.get(self.props, bounds));
                paint_paths(&paths, window);
            }
            FacePaint::Inline(element) => element.paint(window, cx),
            FacePaint::Layer(visible) => visible.set(true),
        }
        if let Some((hitbox, view)) = hitbox.clone() {
            window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && event.button == MouseButton::Left && hitbox.is_hovered(window) {
                    let _ = view.update(cx, |face, cx| {
                        let mut live = face.live.borrow_mut();
                        live.host.poke("spin");
                        live.tree = None;
                        cx.notify();
                    });
                }
            });
        }
    }
}

struct BotFaceView {
    props: FaceProps,
    revision: usize,
    live: Rc<RefCell<LiveFace>>,
}

impl BotFaceView {
    fn new(props: FaceProps) -> Self {
        Self { props, revision: 0, live: Rc::new(RefCell::new(LiveFace::new(props))) }
    }

    fn set_props(&mut self, props: FaceProps) -> usize {
        let mut live = self.live.borrow_mut();
        if self.props.seed != props.seed || (self.props.size < 48.) != (props.size < 48.) {
            *live = LiveFace::new(props);
        } else {
            live.host.set_body(props.body);
            live.host.set_state(props.state);
            live.tree = None;
            live.timer = None;
        }
        self.props = props;
        self.revision = self.revision.wrapping_add(1);
        self.revision
    }
}

impl Render for BotFaceView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let props = self.props;
        let live = self.live.clone();
        let view = cx.weak_entity();
        canvas(
            |_, _, _| (),
            move |bounds, (), window, cx| {
                if !bounds.intersects(&window.content_mask().bounds) {
                    return;
                }
                let now = Instant::now();
                let interval = props.options.max_fps
                    .filter(|fps| fps.is_finite() && *fps > 0.)
                    .and_then(|fps| Duration::try_from_secs_f64(1. / f64::from(fps)).ok())
                    .filter(|interval| !interval.is_zero());
                let mut live = live.borrow_mut();
                paint_paths(live.paths_at(props, bounds, window.mouse_position(), now, interval), window);
                if let Some(interval) = interval {
                    if live.timer.is_none() {
                        let last = live.last.unwrap();
                        live.timer = Some(cx.spawn(async move |cx| {
                            cx.background_executor().timer(interval.saturating_sub(last.elapsed())).await;
                            let _ = view.update(cx, |face, cx| {
                                face.live.borrow_mut().timer = None;
                                cx.notify();
                            });
                        }));
                    }
                } else {
                    window.request_animation_frame();
                }
            },
        ).w(px(props.size)).h(px(props.size)).flex_shrink_0()
    }
}

struct LiveFace {
    host: FaceMotionHost,
    last: Option<Instant>,
    tree: Option<RenderTree>,
    paths: Paths,
    bounds: Option<Bounds<Pixels>>,
    timer: Option<Task<()>>,
}

impl LiveFace {
    fn new(props: FaceProps) -> Self {
        Self {
            host: FaceMotionHost::new(props.body, props.state, props.seed, props.size < 48.),
            last: None,
            tree: None,
            paths: Vec::new(),
            bounds: None,
            timer: None,
        }
    }

    fn paths_at(
        &mut self, props: FaceProps, bounds: Bounds<Pixels>, mouse: Point<Pixels>,
        now: Instant, interval: Option<Duration>,
    ) -> &Paths {
        let elapsed = self.last.map_or(Duration::ZERO, |last| now.duration_since(last));
        let tick = self.tree.is_none() || interval.is_none_or(|interval| elapsed >= interval);
        let map = canvas_at(bounds, props.size);
        if tick {
            self.host.set_gaze(props.options.follow_pointer.then(|| {
                let [x, y] = map.center.0;
                v2(f32::from(mouse.x) as f64 - x, y - f32::from(mouse.y) as f64)
                    / (2. * (f32::from(bounds.size.width) as f64).max(1.))
            }));
            self.host.advance(elapsed.as_secs_f64());
            self.last = Some(now);
            self.tree = Some(render_tree(&self.host.display_frame(), props.size as f64));
        }
        if tick || self.bounds != Some(bounds) {
            self.paths = build_paths(self.tree.as_ref().unwrap(), &map, props.ink);
            self.bounds = Some(bounds);
        }
        &self.paths
    }
}

fn face_layout(size: f32, window: &mut Window, cx: &mut App) -> LayoutId {
    window.request_layout(Style {
        size: Size { width: px(size).into(), height: px(size).into() },
        flex_shrink: 0.,
        ..Default::default()
    }, [], cx)
}

fn canvas_at(bounds: Bounds<Pixels>, size: f32) -> Canvas {
    let center = bounds.center();
    Canvas {
        center: v2(f32::from(center.x) as f64, f32::from(center.y) as f64),
        scale: size as f64 * fill_scale(STILL_FILL) / LOGICAL_SIZE,
    }
}

struct Canvas {
    center: V2,
    scale: f64,
}
impl Canvas {
    fn point(&self, p: V2) -> Option<Point<Pixels>> {
        if !p.finite() {
            return None;
        }
        let [x, y] = (self.center + (p - v2(LOGICAL_SIZE / 2., LOGICAL_SIZE / 2.)) * self.scale)
            .0
            .map(|v| v as f32);
        (x.is_finite() && y.is_finite()).then(|| point(px(x), px(y)))
    }
    fn contour(&self, b: &mut PathBuilder, p: &[V2]) {
        if p.len() < 3 {
            return;
        }
        let Some(points) = p.iter().map(|&p| self.point(p)).collect::<Option<Vec<_>>>() else {
            return;
        };
        b.move_to(points[0]);
        for p in &points[1..] {
            b.line_to(*p);
        }
        b.close();
    }
    fn stroke(&self, width: f64) -> Option<PathBuilder> {
        let w = (width * self.scale) as f32;
        (w.is_finite() && w > 0.).then(|| {
            PathBuilder::stroke(px(w)).with_style(PathStyle::Stroke(StrokeOptions {
                line_width: w,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                miter_limit: 4.,
            }))
        })
    }
    fn line(&self, b: &mut PathBuilder, p: V2) -> Option<()> {
        b.line_to(self.point(p)?);
        Some(())
    }
    fn cubic(&self, b: &mut PathBuilder, p: V2, a: V2, c: V2) -> Option<()> {
        b.cubic_bezier_to(self.point(p)?, self.point(a)?, self.point(c)?);
        Some(())
    }
    fn arc(
        &self,
        b: &mut PathBuilder,
        center: V2,
        r: f64,
        a: f64,
        span: f64,
        transform: impl Fn(V2) -> V2,
    ) -> Option<()> {
        if !r.is_finite() || r <= 0. || !a.is_finite() || !span.is_finite() {
            return None;
        }
        let n = (span.abs() / (TAU / 4.)).ceil().clamp(1., 4.) as usize;
        let d = span / n as f64;
        let k = if (d.abs() - TAU / 4.).abs() < 1e-10 {
            0.55228475 * d.signum()
        } else {
            4. / 3. * (d / 4.).tan()
        };
        for i in 0..n {
            let a = a + d * i as f64;
            let z = a + d;
            let p = polar(a);
            let q = polar(z);
            self.cubic(
                b,
                transform(center + q * r),
                transform(center + (p + v2(-p.0[1], p.0[0]) * k) * r),
                transform(center + (q - v2(-q.0[1], q.0[0]) * k) * r),
            )?;
        }
        Some(())
    }
    fn primitive(&self, p: &Primitive) -> Option<(PathBuilder, f64)> {
        match p {
            Primitive::Dot { center, r, opacity } => {
                let mut b = PathBuilder::fill();
                b.move_to(self.point(*center + v2(*r, 0.))?);
                self.arc(&mut b, *center, *r, 0., TAU, |p| p)?;
                b.close();
                Some((b, *opacity))
            }
            Primitive::Ring {
                center,
                r,
                width,
                opacity,
                arc,
            } => {
                let mut b = self.stroke(*width)?;
                let (fraction, a) = arc.unwrap_or((1., 0.));
                b.move_to(self.point(*center + polar(a) * *r)?);
                self.arc(&mut b, *center, *r, a, TAU * fraction, |p| p)?;
                if fraction >= 1. {
                    b.close();
                }
                Some((b, *opacity))
            }
            Primitive::Shaft {
                width,
                base,
                height,
                transform,
                opacity,
            } => {
                // Source SVG: move, top semicircle, line, bottom semicircle, close.
                let (r, s) = (*width / 2., *base / 2.);
                let y = -height / 2. + r;
                let z = height / 2. - s;
                let t = |p| transform.apply(p);
                let mut b = PathBuilder::fill();
                b.move_to(self.point(t(v2(-r, y)))?);
                self.arc(&mut b, v2(0., y), r, TAU / 2., TAU / 2., t)?;
                self.line(&mut b, t(v2(s, z)))?;
                self.arc(&mut b, v2(0., z), s, 0., TAU / 2., t)?;
                b.close();
                Some((b, *opacity))
            }
            Primitive::Ink {
                points,
                width,
                opacity,
            } => {
                let mut b = self.stroke(*width)?;
                let n = points.len();
                b.move_to(self.point(*points.first()?)?);
                if n == 2 {
                    self.line(&mut b, points[1])?;
                } else {
                    for i in 0..n.saturating_sub(1) {
                        let (p, q) = (points[i], points[i + 1]);
                        let a = points[i.saturating_sub(1)];
                        let z = points[(i + 2).min(n - 1)];
                        self.cubic(&mut b, q, p + (q - a) / 6., q - (z - p) / 6.)?;
                    }
                }
                Some((b, *opacity))
            }
        }
    }
}
fn build_paths(tree: &RenderTree, map: &Canvas, ink: Hsla) -> Paths {
    let mut paths = Vec::with_capacity(tree.parts.len() + 1 + tree.painted.len());
    let mut draw = |b: PathBuilder, mut ink: Hsla, opacity: f64| {
        if opacity.is_finite() && opacity > 0. {
            ink.a *= sat(opacity) as f32;
            if let Ok(path) = b.build() {
                paths.push((path, ink));
            }
        }
    };
    for p in &tree.parts {
        if let Some((b, a)) = map.primitive(p) {
            draw(b, ink, a);
        }
    }
    let mut silhouette = PathBuilder::fill();
    map.contour(&mut silhouette, &tree.body);
    // All eye contours participate in the same even-odd fill as the outer body.
    for eye in &tree.eyes {
        map.contour(&mut silhouette, eye);
    }
    draw(silhouette, ink, tree.head_opacity);
    for (color, outline) in &tree.painted {
        let mut b = PathBuilder::fill();
        map.contour(&mut b, outline);
        let mut c: Hsla = rgb(*color).into();
        c.a = ink.a;
        draw(b, c, tree.paint_opacity * tree.head_opacity);
    }
    paths
}

fn paint_paths(paths: &Paths, window: &mut Window) {
    for (path, color) in paths {
        window.paint_path(path.clone(), *color);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        render_tree, v2, Body, BotFaceElement, BotFaceOptions, Canvas, FaceMotionHost, FaceProps,
        LiveFace, Primitive, StaticPaths, TAU,
    };
    use crate::face::morphs::Transform2;
    use gpui::{point, px, rgb, size, Bounds, PathBuilder};
    use std::{rc::Rc, time::{Duration, Instant}};

    fn test_face() -> FaceProps {
        BotFaceElement::new("test", "sphere", "idle", 28., rgb(0xffffff).into(),
            BotFaceOptions::default(), 0).props
    }

    #[test]
    fn static_paths_are_shared_until_the_placement_or_appearance_changes() {
        let mut cache = StaticPaths::default();
        let props = test_face();
        let bounds = Bounds { origin: point(px(12.), px(35.)), size: size(px(28.), px(28.)) };
        let original = cache.get(props, bounds);
        assert!(!original.is_empty());
        assert!(Rc::ptr_eq(&original, &cache.get(props, bounds)));
        let moved = Bounds { origin: point(px(12.), px(36.)), ..bounds };
        assert!(!Rc::ptr_eq(&original, &cache.get(props, moved)));
        assert!(!Rc::ptr_eq(&original, &cache.get(FaceProps { state: "sleeping", ..props }, bounds)));
        let tinted = FaceProps { ink: rgb(0x555555).into(), ..props };
        assert!(!Rc::ptr_eq(&original, &cache.get(tinted, bounds)));

        for y in 0..StaticPaths::CAPACITY {
            cache.get(props, Bounds { origin: point(px(0.), px(y as f32)), ..bounds });
        }
        assert_eq!(cache.0.len(), StaticPaths::CAPACITY);
        assert!(!Rc::ptr_eq(&original, &cache.get(props, bounds)));
    }

    #[test]
    fn throttled_paths_reuse_geometry_and_rebuild_at_a_new_origin_without_ticking() {
        let props = test_face();
        let mut face = LiveFace::new(props);
        let bounds = Bounds { origin: point(px(12.), px(35.)), size: size(px(28.), px(28.)) };
        let mouse = point(px(20.), px(42.));
        let interval = Duration::from_secs_f64(1. / 30.);
        let start = Instant::now();
        let first = face.paths_at(props, bounds, mouse, start, Some(interval)).as_ptr();
        let early = start + Duration::from_millis(12);
        assert_eq!(first, face.paths_at(props, bounds, mouse, early, Some(interval)).as_ptr());
        let moved = Bounds { origin: point(px(12.), px(38.)), ..bounds };
        assert_ne!(first, face.paths_at(props, moved, mouse, early, Some(interval)).as_ptr());
        assert_eq!(face.last, Some(start));
        face.paths_at(props, moved, mouse, start + interval, Some(interval));
        assert_eq!(face.last, Some(start + interval));
    }

    #[test]
    fn gpu_paths_build_for_cutouts_and_every_primitive_type() {
        let canvas = Canvas {
            center: v2(50., 50.),
            scale: 0.5,
        };
        let tree = render_tree(&FaceMotionHost::still(Body::Sphere, false), 72.);
        let mut fill = PathBuilder::fill();
        canvas.contour(&mut fill, &tree.body);
        for eye in &tree.eyes {
            canvas.contour(&mut fill, eye);
        }
        assert!(fill.build().is_ok());
        let p = v2(120., 120.);
        let valid = [
            Primitive::Dot {
                center: p,
                r: 3.,
                opacity: 1.,
            },
            Primitive::Ring {
                center: p,
                r: 10.,
                width: 2.,
                opacity: 1.,
                arc: None,
            },
            Primitive::Ring {
                center: p,
                r: 10.,
                width: 2.,
                opacity: 1.,
                arc: Some((0.7, -TAU / 4.)),
            },
            Primitive::Shaft {
                width: 30.,
                base: 17.,
                height: 96.,
                transform: Transform2 {
                    origin: p,
                    angle: 0.3,
                    scale: 0.6,
                },
                opacity: 1.,
            },
            Primitive::Ink {
                points: vec![p, p + v2(2., 4.), p + v2(3., 0.)],
                width: 3.,
                opacity: 1.,
            },
        ];
        for p in &valid {
            assert!(canvas.primitive(p).is_some_and(|(b, _)| b.build().is_ok()));
        }
        assert!(canvas.point(v2(f64::INFINITY, 0.)).is_none());
        assert!(canvas
            .primitive(&Primitive::Ring {
                center: p,
                r: 5.,
                width: f64::NAN,
                opacity: 1.,
                arc: None
            })
            .is_none());
        assert!(canvas
            .primitive(&Primitive::Ink {
                points: vec![p, v2(f64::NAN, 0.)],
                width: 3.,
                opacity: 1.
            })
            .is_none());
    }
}
