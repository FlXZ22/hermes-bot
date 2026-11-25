use super::{clips::ExpressionKey, field, math::*, shapes::bounds};
use serde::Deserialize;
use std::{
    cell::RefCell,
    collections::HashMap,
    f64::consts::{FRAC_PI_2, PI},
    rc::Rc,
    sync::LazyLock,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum Expression {
    #[default]
    Neutral,
    Happy,
    Surprised,
    Sad,
    Sleepy,
    Squinted,
    Calm,
    Inspecting,
    Confused,
}
impl Expression {
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "happy" | "smile" => Self::Happy,
            "surprised" => Self::Surprised,
            "sad" | "thinking" => Self::Sad,
            "sleepy" | "sleep" => Self::Sleepy,
            "squinted" | "squint" => Self::Squinted,
            "calm" => Self::Calm,
            "inspecting" | "inspect" | "strange" => Self::Inspecting,
            "confused" => Self::Confused,
            _ => Self::Neutral,
        }
    }
    fn direct(self) -> bool {
        matches!(
            self,
            Self::Happy | Self::Sad | Self::Sleepy | Self::Squinted | Self::Confused
        )
    }
    pub fn allows_lids(self) -> bool {
        !self.direct() && self != Self::Calm
    }
    fn inspect(self) -> f64 {
        f64::from(self == Self::Inspecting)
    }
    pub fn eyes(self) -> [Eye; 2] {
        let mut left = match self {
            Self::Neutral => Eye::stadium(0.26, 0.57, 0., v2(0., 0.), 0.),
            Self::Happy => Eye::spine(
                v2(0.03, 0.03),
                0.1305,
                v2(0.16, -0.04),
                v2(0., 0.12),
                v2(-0.16, -0.04),
                true,
            ),
            Self::Surprised => Eye::stadium(0.42, 0.86, 0., v2(0.03, 0.), 0.),
            Self::Sad => Eye::stadium(0.26, 0.5, -28. * DEG, v2(-0.05, 0.), 0.),
            Self::Sleepy => Eye::spine(
                v2(0.01, 0.),
                0.13,
                v2(-0.11, 0.025),
                v2(0., -0.05),
                v2(0.11, 0.025),
                false,
            ),
            Self::Squinted => Eye::stadium(0.26, 0.5, FRAC_PI_2, v2(0.01, 0.), 0.),
            Self::Calm => Eye::spine(
                v2(0.05, 0.),
                0.247,
                v2(-0.296, 0.),
                v2(0., 0.),
                v2(0.296, 0.),
                false,
            ),
            Self::Inspecting => Eye::stadium(0.54, 0.54, 0., v2(0.04, 0.), 0.),
            Self::Confused => Eye::spine(
                v2(-0.1, 0.),
                0.1305,
                v2(0.01, 0.16),
                v2(-0.17, 0.),
                v2(0.01, -0.16),
                false,
            ),
        };
        if self == Self::Calm {
            left.oval = v2(0.296, 0.247);
        }
        let right = if self == Self::Inspecting {
            Eye::stadium(0.36, 0.36, 0., v2(0.04, 0.), 0.)
        } else {
            left.clone()
        };
        [left, right]
    }
}

#[derive(Clone, Debug)]
pub(super) struct Eye {
    pub off: V2,
    pub tilt: f64,
    pub width: f64,
    pub lid: f64,
    pub oval: V2,
    oval_mix: f64,
    pub points: [V2; 21],
    path: [[V2; 4]; 2],
    pub ring: Option<Vec<V2>>,
}
impl Eye {
    fn spine(off: V2, width: f64, a: V2, b: V2, c: V2, arc: bool) -> Self {
        let path = if arc {
            let u = b - a;
            let v = c - a;
            let d = 2. * u.cross(v);
            if d.abs() > 1e-9 {
                let center = a + v2(
                    u.dot(u) * v.0[1] - v.dot(v) * u.0[1],
                    v.dot(v) * u.0[0] - u.dot(u) * v.0[0],
                ) / d;
                let radius = (a - center).len();
                let start = (a - center).angle();
                let end = ((c - center).angle() - start).rem_euclid(TAU);
                let span = if ((b - center).angle() - start).rem_euclid(TAU) <= end {
                    end
                } else {
                    end - TAU
                };
                std::array::from_fn(|i| {
                    let p = start + i as f64 * span * 0.5;
                    let q = p + span * 0.5;
                    let k = 4. / 3. * (span / 8.).tan() * radius;
                    let x = center + polar(p) * radius;
                    let y = center + polar(q) * radius;
                    [
                        x,
                        x + polar(p + FRAC_PI_2) * k,
                        y - polar(q + FRAC_PI_2) * k,
                        y,
                    ]
                })
            } else {
                line_spine(a, c)
            }
        } else {
            let u = (a + b) * 0.5;
            let v = (b + c) * 0.5;
            let m = (u + v) * 0.5;
            [
                [a, a.lerp(u, 2. / 3.), m.lerp(u, 2. / 3.), m],
                [m, m.lerp(v, 2. / 3.), c.lerp(v, 2. / 3.), c],
            ]
        };
        let mut eye = Self {
            off,
            width,
            tilt: 0.,
            lid: 1.,
            oval: V2::default(),
            oval_mix: 0.,
            points: [V2::default(); 21],
            path,
            ring: None,
        };
        eye.resample();
        eye
    }
    fn resample(&mut self) {
        self.points = std::array::from_fn(|i| {
            let j = usize::from(i > 10);
            cubic(self.path[j], (i - 10 * j) as f64 / 10.)
        });
    }
    fn stadium(width: f64, len: f64, angle: f64, off: V2, tilt: f64) -> Self {
        let d = (len - width).max(0.) / 2.;
        let p = v2(angle.sin(), angle.cos()) * d;
        let mut e = Self::spine(off, width / 2., -p, V2::default(), p, false);
        e.tilt = tilt;
        e
    }
    pub fn is_oval(&self) -> bool {
        self.oval.0.into_iter().all(|r| r > 1e-4)
    }
    fn curved(&self) -> bool {
        let a = self.points[0];
        let d = self.points[20] - a;
        !self.is_oval()
            && d.dot(d) >= 1e-8
            && self
                .points
                .iter()
                .any(|p| ((*p - a).cross(d) / d.len()).abs() > 0.012_f64.max(self.width * 0.2))
    }
    pub fn dimensions(&self) -> (f64, f64, f64, V2) {
        if self.is_oval() {
            return (
                2. * self.oval.0[0],
                2. * self.oval.0[1] * self.lid,
                self.tilt,
                self.off,
            );
        }
        let [a, b] = [self.points[0], self.points[20]];
        let [x, y] = (b - a).0;
        (
            2. * self.width,
            (x.hypot(y) + 2. * self.width) * self.lid,
            x.atan2(y) + self.tilt,
            self.off + v2((a.0[0] + b.0[0]) / 2., (a.0[1] + b.0[1]) * self.lid / 2.),
        )
    }
    fn axes(&self) -> (V2, f64, f64, f64) {
        if self.is_oval() {
            let [a, b] = self.oval.0;
            (
                self.off,
                if a >= b { FRAC_PI_2 } else { 0. },
                a.max(b),
                a.min(b),
            )
        } else {
            let (a, b) = (self.points[0], self.points[20]);
            let [x, y] = (b - a).0;
            (
                self.off + (a + b) * 0.5,
                self.tilt + x.atan2(y),
                x.hypot(y) * 0.5 + self.width,
                self.width,
            )
        }
    }
    fn close_lid(mut self, t: f64) -> Self {
        if t == 0. {
            return self;
        }
        let [x, y] = (self.points[20] - self.points[0]).0;
        let ang = x.atan2(y);
        if y.abs() >= x.abs() && ang.abs() >= 1e-5 {
            self.tilt += ang;
            self.points = self.points.map(|p| p.rotate(ang));
        }
        let r = (1. - t).max(0.03);
        let (lo, hi) = if self.is_oval() {
            (-self.oval.0[1], self.oval.0[1])
        } else {
            self.points
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    (lo.min(p.0[1] - self.width), hi.max(p.0[1] + self.width))
                })
        };
        self.off = self.off + v2(0., mix(lo, hi, 0.4) * (1. - r)).rotate(-self.tilt);
        self.lid *= r;
        self
    }
    pub fn blink(&self, t: f64) -> Self {
        if t <= 0. {
            return self.clone();
        }
        let (width, height, a, c) = self.dimensions();
        let w = (width * a.cos().abs() + height * a.sin().abs()).max(1e-4);
        let h = (width * a.sin().abs() + height * a.cos().abs()).max(w);
        let h = mix(h, w * 0.42, sat(t));
        if h >= w {
            Self::stadium(w, h, 0., c, 0.)
        } else {
            Self::stadium(h, w, 0., c, FRAC_PI_2)
        }
    }
    pub fn spine_contour(&self, transform: impl Fn(V2) -> V2, width: f64) -> Vec<V2> {
        let samples: Vec<_> = (0..21)
            .map(|i| {
                let k = usize::from(i > 10);
                let t = (i - 10 * k) as f64 / 10.;
                let p = cubic(self.path[k], t);
                (
                    transform(p),
                    transform(p + cubic_tangent(self.path[k], t)) - transform(p),
                )
            })
            .collect();
        capsule(&samples, width)
    }
    fn sphere_contours(&self, radius_px: f64) -> Rc<Vec<Vec<V2>>> {
        let band = radius_px.max(1.).log2().ceil().clamp(0., 16.) as u32;
        let key = ContourKey {
            band,
            profile: [self.width, self.oval.0[0], self.oval.0[1], self.oval_mix].map(f64::to_bits),
            spine: self.points.map(|p| p.0.map(f64::to_bits)),
        };
        EYE_CONTOURS.with_borrow_mut(|cache| {
            if let Some(shape) = cache.get(&key) {
                return shape.clone();
            }
            if cache.len() >= 512 {
                cache.clear();
            }
            let shape = Rc::new(self.trace_sphere_contours(0.02 / 2_f64.powi(band as i32)));
            cache.insert(key, shape.clone());
            shape
        })
    }
    fn trace_sphere_contours(&self, deviation: f64) -> Vec<Vec<V2>> {
        let [rx, ry] = self.oval.0;
        let oval = rx.min(ry);
        let ellipse_mix = if oval > 1e-5 {
            if self.oval_mix > 1e-4 {
                sat(self.oval_mix)
            } else {
                smooth(oval / 0.08)
            }
        } else {
            0.
        };
        let width = if self.width > 0. { self.width } else { -1. };
        let (mut lo, mut hi, mut feature) = (
            v2(f64::INFINITY, f64::INFINITY),
            v2(f64::NEG_INFINITY, f64::NEG_INFINITY),
            f64::INFINITY,
        );
        if width > 0. && ellipse_mix < 1. {
            let (a, b) = bounds(&self.points);
            lo = a - v2(width, width);
            hi = b + v2(width, width);
            feature = width;
        }
        if ellipse_mix > 0. {
            lo = v2(lo.0[0].min(-rx), lo.0[1].min(-ry));
            hi = v2(hi.0[0].max(rx), hi.0[1].max(ry));
            feature = feature.min(oval);
        }
        let [w, h] = (hi - lo).0;
        if w <= 0. || h <= 0. {
            return vec![];
        }
        let segment = |p: V2, q: V2| {
            let d = q - p;
            let l = d.dot(d);
            (p.0, d.0, if l > 0. { l.recip() } else { 0. })
        };
        let start = self.points[0];
        let end = self.points[20];
        let axis = end - start;
        let line = [segment(start, end)];
        let bends =
            std::array::from_fn::<_, 20, _>(|i| segment(self.points[i], self.points[i + 1]));
        let segments = if self.points.iter().all(|&p| {
            let d = p - start;
            d.cross(axis).abs() < 1e-12
                && d.dot(axis) >= -1e-12
                && d.dot(axis) <= axis.dot(axis) + 1e-12
                && (axis.len() > 1e-12 || d.len() < 1e-12)
        }) {
            line.as_slice()
        } else {
            bends.as_slice()
        };
        let field = |VecN([x, y]): V2| {
            let ellipse = || (((x / rx).powi(2) + (y / ry).powi(2)).sqrt() - 1.) * oval;
            if ellipse_mix == 1. {
                return ellipse();
            }
            let distance2 = segments
                .iter()
                .fold(1e6_f64, |best, &([a, b], [u, v], recip)| {
                    let (x, y) = (x - a, y - b);
                    let t = sat((x * u + y * v) * recip);
                    best.min((x - u * t).powi(2) + (y - v * t).powi(2))
                });
            let distance = distance2.sqrt() - width;
            if ellipse_mix == 0. {
                distance
            } else {
                mix(distance, ellipse(), ellipse_mix)
            }
        };
        let step = 0.015_f64.min(feature / 3.).max(w.max(h) / 400.);
        field::trace(field, lo, hi, step, deviation * 0.001)
    }
}

#[derive(PartialEq, Eq, Hash)]
struct ContourKey {
    band: u32,
    profile: [u64; 4],
    spine: [[u64; 2]; 21],
}
type EyeContourCache = HashMap<ContourKey, Rc<Vec<Vec<V2>>>>;
thread_local! { static EYE_CONTOURS: RefCell<EyeContourCache> = RefCell::default(); }

fn line_spine(a: V2, b: V2) -> [[V2; 4]; 2] {
    std::array::from_fn(|i| std::array::from_fn(|j| a.lerp(b, (i as f64 + j as f64 / 3.) * 0.5)))
}
fn capsule(points: &[(V2, V2)], width: f64) -> Vec<V2> {
    if width <= 0. || points.len() < 2 {
        return vec![];
    }
    let mean = points.iter().fold(V2::default(), |sum, p| sum + p.0) / points.len() as f64;
    if width > 1.05 * points.iter().map(|p| (p.0 - mean).len()).fold(0., f64::max) {
        return (0..48)
            .map(|i| {
                let dir = polar(TAU * i as f64 / 48.);
                let p = points
                    .iter()
                    .max_by(|a, b| a.0.dot(dir).total_cmp(&b.0.dot(dir)))
                    .map_or(mean, |p| p.0);
                p + dir * width
            })
            .collect();
    }
    let mut normals = Vec::<V2>::with_capacity(points.len());
    for &(_, d) in points {
        let mut n = v2(-d.0[1], d.0[0]).unit();
        if normals.last().is_some_and(|p| p.dot(n) < 0.) {
            n = -n;
        }
        normals.push(n);
    }
    let cap = |p: V2, n: V2, d: V2| {
        let a = n.angle();
        let sign = if (d.angle() - a + PI).rem_euclid(TAU) - PI >= 0. {
            1.
        } else {
            -1.
        };
        (1..10).map(move |i| p + polar(a + sign * PI * i as f64 / 10.) * width)
    };
    let last = points.len() - 1;
    points
        .iter()
        .zip(&normals)
        .map(|(p, &n)| p.0 + n * width)
        .chain(cap(points[last].0, normals[last], points[last].1))
        .chain(
            points
                .iter()
                .zip(&normals)
                .rev()
                .map(|(p, &n)| p.0 - n * width),
        )
        .chain(cap(points[0].0, -normals[0], -points[0].1))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Method {
    Blink,
    Lid,
    Spine,
    Stadium,
    Oval,
}
struct Transition {
    method: Method,
    from: [Eye; 2],
    to: [Eye; 2],
    name: Expression,
    inspect: f64,
    t: f64,
    duration: f64,
    held: bool,
}
pub(super) struct Eyes {
    pub name: Expression,
    base: [Eye; 2],
    inspect: f64,
    transition: Option<Transition>,
}
impl Eyes {
    pub fn new() -> Self {
        Self {
            name: Expression::Neutral,
            base: Expression::Neutral.eyes(),
            inspect: 0.,
            transition: None,
        }
    }
    pub fn morphing(&self) -> bool {
        self.transition
            .as_ref()
            .is_some_and(|tr| tr.method != Method::Blink && !tr.held)
    }
    pub fn amount(&self) -> Option<f64> {
        let tr = self.transition.as_ref()?;
        if tr.method != Method::Blink || tr.held {
            return None;
        }
        let (a, b) = blink_window(true, tr.duration);
        let t = sat(tr.t / tr.duration);
        Some(if t < a {
            smooth(t / a)
        } else if t <= b {
            1.
        } else {
            1. - smooth((t - b) / (1. - b).max(1e-6))
        })
    }
    fn current(&self) -> ([Eye; 2], f64) {
        let Some(tr) = &self.transition else {
            return (self.base.clone(), self.inspect);
        };
        if tr.method == Method::Blink {
            return (tr.from.clone(), tr.inspect);
        }
        let t = sat(tr.t / tr.duration);
        if tr.method == Method::Lid {
            let (a, b) = blink_window(false, tr.duration);
            if t < a {
                return (
                    tr.from.clone().map(|e| e.close_lid(smooth(t / a))),
                    tr.inspect,
                );
            }
            if t <= b {
                return (
                    tr.to.clone().map(|mut e| {
                        e.width = 0.;
                        e.oval = V2::default();
                        e
                    }),
                    if t < b { 0. } else { tr.name.inspect() },
                );
            }
            let r = sat((t - b) / (1. - b).max(1e-6));
            let open = if r < 0.7 {
                (1. - (1. - r / 0.7).powi(2)) * 1.06
            } else {
                1.06 - 0.06 * smooth((r - 0.7) / 0.3)
            };
            return (
                tr.to.clone().map(|e| e.close_lid(1. - open)),
                tr.name.inspect(),
            );
        }
        let t = bezier(t, [0.4, 0.06, 0.18, 1.]);
        (
            std::array::from_fn(|i| blend(&tr.from[i], &tr.to[i], t, tr.method)),
            mix(tr.inspect, tr.name.inspect(), t),
        )
    }
    pub fn play(&mut self, key: ExpressionKey) {
        if self.name != key.name {
            self.set(key.name, key.through_blink)
        }
    }
    pub fn set(&mut self, name: Expression, through_blink: bool) {
        let (mut from, inspect) = self.current();
        for e in &mut from {
            e.oval_mix = 0.;
        }
        let to = name.eyes();
        let curved = from.iter().chain(&to).any(Eye::curved);
        let method = if through_blink && !name.direct() && !self.name.direct() {
            Method::Lid
        } else if !curved && from.iter().chain(&to).any(Eye::is_oval) {
            Method::Oval
        } else if curved || self.name == Expression::Sleepy || name == Expression::Sleepy {
            Method::Spine
        } else {
            Method::Stadium
        };
        self.name = name;
        self.transition = Some(Transition {
            method,
            from,
            to,
            name,
            inspect,
            t: 0.,
            duration: if method == Method::Lid { 0.35 } else { 0.28 },
            held: false,
        });
    }
    pub fn blink(&mut self) {
        if self.transition.as_ref().is_none_or(|t| t.held) {
            self.transition = Some(Transition {
                method: Method::Blink,
                from: self.base.clone(),
                to: self.name.eyes(),
                name: self.name,
                inspect: self.inspect,
                t: 0.,
                duration: 0.26,
                held: false,
            });
        }
    }
    pub fn advance(&mut self, dt: f64) {
        if let Some(tr) = &mut self.transition {
            if tr.held {
                return;
            }
            tr.t += dt;
            if tr.t >= tr.duration {
                if let Some(tr) = self.transition.take() {
                    self.base = tr.to;
                    self.inspect = tr.name.inspect();
                }
            }
        }
    }
    pub fn seek(&mut self, keys: &[ExpressionKey], t: f64) {
        *self = Self::new();
        let mut old = 0.;
        for &key in keys.iter().take_while(|k| k.t <= t + 1e-9) {
            self.advance(key.t - old);
            self.play(key);
            old = key.t;
        }
        self.advance(t - old);
        if let Some(tr) = &mut self.transition {
            tr.held = true;
        }
    }
    pub fn release(&mut self) {
        if let Some(tr) = &mut self.transition {
            tr.held = false;
        }
    }
    pub fn frame(&self, yaw: f64, bias: f64, follow: bool, lids: f64) -> [Eye; 2] {
        let (mut eyes, t) = self.current();
        if t >= 0.001 {
            let b = if follow {
                (yaw / 20.).clamp(-1., 1.)
            } else {
                bias
            };
            for (i, e) in eyes.iter_mut().enumerate() {
                let r = mix(
                    1.,
                    (if i == 0 { 2_f64 / 3. } else { 1.5_f64 }).powf((1. + b) / 2.),
                    t,
                );
                e.width *= r;
                e.oval_mix = 0.;
                e.points = e.points.map(|p| p * r);
            }
        }
        let t = self.amount().unwrap_or(lids);
        if t > 1e-4 {
            eyes = eyes.map(|e| e.blink(t));
        }
        eyes
    }
}
fn blink_window(blink: bool, duration: f64) -> (f64, f64) {
    let n = 0.28_f64.min((if blink { 2. } else { 4.25 }) / 60. / duration.max(0.05));
    let a = (1. - n) * 0.42;
    (a, a + n)
}
fn blend(a: &Eye, b: &Eye, t: f64, method: Method) -> Eye {
    if method == Method::Spine {
        let mut e = a.clone();
        e.off = a.off.lerp(b.off, t);
        e.tilt = mix(a.tilt, b.tilt, t);
        e.width = mix(a.width, b.width, t);
        e.lid = mix(a.lid, b.lid, t);
        e.oval = a.oval.lerp(b.oval, t);
        e.oval_mix = 0.;
        e.ring = None;
        e.path =
            std::array::from_fn(|i| std::array::from_fn(|j| a.path[i][j].lerp(b.path[i][j], t)));
        e.resample();
        return e;
    }
    if method == Method::Oval && t <= 1e-4 {
        return a.clone();
    }
    if t >= if method == Method::Oval {
        0.995
    } else {
        0.9999
    } {
        return b.clone();
    }
    let (c0, a0, l0, w0) = a.axes();
    let (c1, a1, l1, w1) = b.axes();
    let mut delta = a1 - a0;
    while delta > FRAC_PI_2 {
        delta -= PI
    }
    while delta < -FRAC_PI_2 {
        delta += PI
    }
    let angle = a0 + delta * t;
    let w = mix(w0, w1, t).max(1e-4);
    let off = c0.lerp(c1, t);
    if method == Method::Stadium {
        return Eye::stadium(
            2. * w,
            2. * (w + mix((l0 - w0).max(0.), (l1 - w1).max(0.), t)),
            angle,
            off,
            0.,
        );
    }
    let l = mix(l0, l1, t).max(1e-4);
    let f = mix(
        f64::from(a.is_oval()),
        f64::from(b.is_oval()),
        if b.is_oval() { cubic_out(t) } else { t.powi(3) },
    );
    let mut eye = Eye::stadium(2. * w, 2. * l.max(w), 0., off, angle);
    if f > 1e-4 {
        eye.oval = v2(w, l);
        eye.oval_mix = f;
    }
    eye.ring = Some(
        (0..48)
            .map(|i| {
                let [c, s] = polar(TAU * i as f64 / 48.).0;
                let r = (l - w).max(0.);
                let d = w / s.abs().max(1e-6);
                let reach = if d * c.abs() <= r + 1e-6 {
                    d
                } else {
                    r * c.abs() + (l * l - r * r + w * w).max(0.).sqrt()
                };
                v2(mix(reach * s, w * s, f), mix(reach * c, l * c, f))
            })
            .collect(),
    );
    eye
}

struct Mount {
    middle: V3,
    across: V3,
    along: V3,
}
static SPHERE_MOUNTS: LazyLock<([Mount; 2], Quat)> = LazyLock::new(|| {
    let a = [v3(0.01722, 0.45638, 0.88962), v3(0.48863, 0.67924, 0.54762)];
    let b = [v3(0.186, 0.20949, 0.95996), v3(0.65674, 0.43195, 0.61815)];
    let middle = ((a[0] + b[0]).unit() + (a[1] + b[1]).unit()).unit();
    let q = Quat::between(middle, v3(0., 0., 1.));
    let sum = rotate(q.matrix(), b[0] - a[0]) + rotate(q.matrix(), b[1] - a[1]);
    let angle = (PI - sum.xy().angle()).rem_euclid(PI) - FRAC_PI_2;
    let q = Quat::axis(v3(0., 0., 1.), angle).mul(q);
    (
        std::array::from_fn(|i| {
            let a = rotate(q.matrix(), a[i]);
            let b = rotate(q.matrix(), b[i]);
            let middle = (a + b).unit();
            let mut along = (b - middle * b.dot(middle)).unit();
            if along.0[1] < 0. {
                along = -along;
            }
            let mut across = middle.cross(along);
            if across.0[0] * middle.0[0] < 0. {
                across = -across;
            }
            Mount {
                middle,
                across,
                along,
            }
        }),
        q.inverse(),
    )
});
pub(super) fn rest_rotation() -> V3 {
    euler_from_matrix(SPHERE_MOUNTS.1.matrix()) / DEG
}
pub(super) fn sphere_eyes(eyes: &[Eye; 2], m: [V3; 3], radius_px: f64) -> Vec<Vec<V2>> {
    let mut projected = vec![];
    for (e, mount) in eyes.iter().zip(&SPHERE_MOUNTS.0) {
        let o = rotate(m, mount.middle);
        let s = rotate(m, mount.across);
        let c = rotate(m, mount.along);
        for contour in e.sphere_contours(radius_px * e.lid.max(1.)).iter() {
            let points: Vec<_> = contour
                .iter()
                .map(|p| {
                    let [x, y] = (e.off + v2(p.0[0], p.0[1] * e.lid).rotate(-e.tilt)).0;
                    let l = x.hypot(y);
                    o * l.cos() + (s * x + c * y) * (if l > 1e-12 { l.sin() / l } else { 1. })
                })
                .collect();
            if let Some(loop_points) = horizon(&points, o.xy().angle()) {
                projected.push(loop_points);
            }
        }
    }
    projected
}
fn horizon(points: &[V3], angle: f64) -> Option<Vec<V2>> {
    if points.len() < 3 {
        return None;
    }
    let n = points.len();
    let Some(start) = (0..n).find(|&i| points[i].0[2] <= 0. && points[(i + 1) % n].0[2] > 0.)
    else {
        return (points[0].0[2] > 0.).then(|| points.iter().map(|p| p.xy()).collect());
    };
    let mut out = Vec::new();
    let mut exit = None;
    let mut first = None;
    let append_arc = |out: &mut Vec<V2>, a: V2, b: V2| {
        let wrap = |v: f64| (v + PI).rem_euclid(TAU) - PI;
        let d = wrap(b.angle() - angle) - wrap(a.angle() - angle);
        let n = (d.abs() / (PI / 48.)).ceil() as usize;
        for i in 1..n {
            out.push(polar(a.angle() + d * i as f64 / n as f64));
        }
        out.push(b);
    };
    for i in 0..n {
        let a = points[(start + i) % n];
        let b = points[(start + i + 1) % n];
        if (a.0[2] > 0.) != (b.0[2] > 0.) {
            let p = a.lerp(b, a.0[2] / (a.0[2] - b.0[2])).xy().unit();
            if b.0[2] > 0. {
                if let Some(x) = exit.take() {
                    append_arc(&mut out, x, p)
                } else {
                    first = Some(p);
                    out.push(p);
                }
            } else {
                out.push(p);
                exit = Some(p);
            }
        }
        if b.0[2] > 0. {
            out.push(b.xy());
        }
    }
    if let (Some(a), Some(b)) = (exit, first) {
        append_arc(&mut out, a, b)
    }
    (out.len() >= 3).then_some(out)
}
