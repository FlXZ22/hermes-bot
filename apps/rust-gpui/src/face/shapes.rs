use super::{contours, expressions::Eye, math::*, morphs::L, outline_data, pumpkin};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::{
    cell::RefCell,
    collections::HashMap,
    f64::consts::{FRAC_PI_2, PI},
    rc::Rc,
    sync::LazyLock,
};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(super) enum Body {
    Sphere,
    Pebble,
    Bean,
    Egg,
    Squircle,
    Tablet,
    Capsule,
    Cylinder,
    Hex,
    Gem,
    Crystal,
    Wedge,
    Shield,
    Dome,
    Arch,
    Cloud,
    Teardrop,
    Leaf,
    Square,
    Sparkle,
    Clover,
    Flower,
    Heart,
    House,
    Star,
    Pumpkin,
}
impl Body {
    pub fn named(s: &str) -> Self {
        match s {
            "pebble" => Self::Pebble,
            "bean" => Self::Bean,
            "egg" => Self::Egg,
            "squircle" => Self::Squircle,
            "tablet" => Self::Tablet,
            "capsule" => Self::Capsule,
            "cylinder" => Self::Cylinder,
            "hex" | "hexagon" => Self::Hex,
            "gem" => Self::Gem,
            "crystal" => Self::Crystal,
            "wedge" => Self::Wedge,
            "shield" => Self::Shield,
            "dome" => Self::Dome,
            "arch" => Self::Arch,
            "cloud" => Self::Cloud,
            "teardrop" => Self::Teardrop,
            "leaf" => Self::Leaf,
            "square" => Self::Square,
            "sparkle" | "spark" => Self::Sparkle,
            "clover" => Self::Clover,
            "flower" | "starFlower" => Self::Flower,
            "heart" | "heartChubby" => Self::Heart,
            "house" => Self::House,
            "star" | "star6" => Self::Star,
            "pumpkin" => Self::Pumpkin,
            _ => Self::Sphere,
        }
    }
    fn geometry(self) -> &'static Shape {
        static SHAPES: LazyLock<[std::sync::OnceLock<Shape>; 26]> =
            LazyLock::new(|| std::array::from_fn(|_| std::sync::OnceLock::new()));
        SHAPES[self as usize].get_or_init(|| Shape::new(self))
    }
    pub fn scale(self) -> f64 {
        if self == Self::Pumpkin {
            1.17
        } else {
            self.geometry().size
        }
    }
    pub fn project(
        self,
        orientation: Quat,
        sprite: f64,
        zoom: f64,
        extent: f64,
        eyes: Option<&[Eye; 2]>,
    ) -> Projected {
        let mut matrix = orientation.matrix();
        if self != Self::Pumpkin && matrix[2].0[2] < 0. {
            for row in &mut matrix {
                row.0[0] *= -1.;
                row.0[2] *= -1.;
            }
        }
        // The stock renderer snaps solid orientations to its size-dependent cache grid.
        let step = 2_f64.powf((0.6 / (extent * zoom / 2.).max(1.) / DEG).log2().floor());
        let angles = euler_from_matrix(matrix) / DEG;
        let avoid_edge = |v: f64| {
            let a = v.rem_euclid(180.);
            if (a - 90.).abs() >= 0.5 {
                v
            } else {
                v + (if a <= 90. { 89.5 } else { 90.5 }) - a
            }
        };
        let mut angles = angles.0.map(|v| (v / step + 0.5).floor() * step);
        angles[0] = avoid_edge(angles[0]);
        angles[1] = avoid_edge(angles[1]);
        let matrix = solid_matrix(VecN(angles) * DEG);
        if self == Self::Pumpkin {
            return pumpkin::project(matrix, sprite, eyes.map(|e| eye_loops(self, e)));
        }
        let geo = self.geometry();
        if matches!(geo.outline, Silhouette::Revolve(_)) {
            angles[1] = angles[1].rem_euclid(360. / 64.);
        }
        // Orthographic roll just rotates a convex projection in the view plane.
        // Cache the convex envelope before roll; lofts keep the stock screen-space ray grid.
        let roll = if geo.outline.is_convex() {
            std::mem::take(&mut angles[2]) * DEG
        } else {
            0.
        };
        let outline = OUTLINES.with_borrow_mut(|cache| {
            let grid = if matches!(
                geo.outline,
                Silhouette::Rings {
                    mode: RingMode::Extrusion,
                    ..
                }
            ) {
                sprite
            } else {
                0.
            };
            let key = (self, angles.map(f64::to_bits), grid as u32);
            if let Some(cached) = cache.get(&key) {
                return cached.clone();
            }
            if cache.len() >= 768 {
                cache.clear();
            }
            let outline = Rc::new(geo.outline.project(
                solid_matrix(VecN(angles) * DEG),
                sprite * self.scale() * 0.36 / 1.18 * 1024.,
            ));
            cache.insert(key, outline.clone());
            outline
        });
        let eyes = eyes.map_or_else(Vec::new, |eyes| {
            geo.seat
                .project(&geo.outline, eye_loops(self, eyes), matrix)
        });
        Projected {
            outline: if roll == 0. {
                outline.as_ref().clone()
            } else {
                outline.iter().map(|&p| p.rotate(roll)).collect()
            },
            eyes,
            painted: vec![],
        }
    }
}

type OutlineCache = HashMap<(Body, [u64; 3], u32), Rc<Vec<V2>>>;
thread_local! { static OUTLINES: RefCell<OutlineCache> = RefCell::default(); }

// Solid bodies are rotated Y, X, Z; their front/back symmetry is handled separately above.
pub(super) fn solid_matrix(rot: V3) -> [V3; 3] {
    let [pitch, yaw, roll] = rot.0;
    let q = Quat::axis(v3(0., 0., 1.), roll)
        .mul(Quat::axis(v3(1., 0., 0.), pitch))
        .mul(Quat::axis(v3(0., 1., 0.), yaw));
    q.matrix()
}
pub(super) struct Projected {
    pub outline: Vec<V2>,
    pub eyes: Vec<Vec<V2>>,
    pub painted: Vec<(u32, Vec<V2>)>,
}

struct Shape {
    size: f64,
    outline: Silhouette,
    seat: Seat,
}
enum Silhouette {
    Sphere,
    Revolve(Vec<(f64, f64)>),
    Solid(Vec<V3>),
    Balls(Vec<Ball>),
    Rings { rings: Vec<Vec<V3>>, mode: RingMode },
}
enum RingMode {
    Convex,
    Extrusion,
    Loft,
}
#[derive(Clone, Copy)]
struct Ball {
    center: V3,
    r: f64,
}
enum Seat {
    None,
    Front(f64),
    Loft {
        center: V2,
        depth: f64,
        radii: Vec<f64>,
    },
    Wrap {
        r: f64,
        lo: f64,
        hi: f64,
        eye_y: f64,
        profile: Option<Profile>,
        puffy: bool,
    },
    WrapX {
        r: f64,
        lo: f64,
        hi: f64,
        flat: f64,
    },
}

impl Shape {
    fn new(body: Body) -> Self {
        use Body::*;
        let size = match body {
            Sphere => 1.,
            Pebble => 1.2168,
            Squircle => 1.17,
            Tablet | Flower => 1.3923,
            Hex => 1.2519,
            Crystal => 1.3806,
            Wedge => 1.359072,
            Cloud => 1.3455,
            Teardrop => 1.318122,
            Square => 1.0413,
            Sparkle => 1.521,
            Clover => 1.0881,
            Heart => 1.3572,
            Star => 1.4976,
            _ => 1.17,
        };
        let (outline, seat) = match body {
            Sphere | Pumpkin => (Silhouette::Sphere, Seat::None),
            Tablet | Capsule => {
                let p = planar(body);
                let (lo, hi) = bounds(&radial(&p, V2::default(), 96));
                let x = (hi.0[0] - lo.0[0]) / 2.;
                let y = (hi.0[1] - lo.0[1]) / 2.;
                let horizontal = body == Tablet;
                let (r, flat) = if horizontal {
                    (y, (x - y).max(0.))
                } else {
                    (x, (y - x).max(0.))
                };
                let balls = (0..=32)
                    .map(|i| {
                        let d = mix(-flat, flat, i as f64 / 32.);
                        Ball {
                            center: if horizontal {
                                v3(d, 0., 0.)
                            } else {
                                v3(0., d, 0.)
                            },
                            r,
                        }
                    })
                    .collect();
                let seat = if horizontal {
                    Seat::WrapX {
                        r,
                        lo: -x + 0.02,
                        hi: x - 0.02,
                        flat,
                    }
                } else {
                    Seat::Wrap {
                        r: ((r.max(0.55) + r.max(0.35)) / 2.).max(0.35),
                        lo: lo.0[1],
                        hi: hi.0[1],
                        eye_y: (lo.0[1] + hi.0[1]) * 0.5,
                        profile: None,
                        puffy: false,
                    }
                };
                (Silhouette::Balls(balls), seat)
            }
            Squircle => {
                let p = radial(&planar(body), V2::default(), 96);
                let mut points = vec![];
                for (i, &a) in p.iter().enumerate() {
                    let d = p[(i + 1) % p.len()] - p[(i + p.len() - 1) % p.len()];
                    let mut n = v2(d.0[1], -d.0[0]).unit();
                    if n.dot(a) < 0. {
                        n = -n;
                    }
                    for j in 0..=4 {
                        let t = j as f64 * FRAC_PI_2 / 4.;
                        let q = a - n * (0.11 * (1. - t.cos()));
                        let z = 0.17 + 0.11 * t.sin();
                        points.extend([v3(q.0[0], q.0[1], z), v3(q.0[0], q.0[1], -z)]);
                    }
                }
                (Silhouette::Solid(points), Seat::Front(0.28))
            }
            Crystal => {
                let mut hubs = vec![v3(0., 1.04, 0.), v3(0., -1.04, 0.)];
                for y in [-0.51, 0.51] {
                    for i in 0..6 {
                        let [x, z] = (polar(TAU * i as f64 / 6.) * 0.52).0;
                        hubs.push(v3(x, y, z));
                    }
                }
                let points = hubs
                    .into_iter()
                    .flat_map(|p| {
                        (0..=8).flat_map(move |i| {
                            let (s, c) = (PI * i as f64 / 8.).sin_cos();
                            let n = if i == 0 || i == 8 { 1 } else { 14 };
                            (0..n).map(move |j| {
                                let (u, v) = (TAU * j as f64 / n as f64).sin_cos();
                                p + v3(s * u, c, s * v) * 0.12
                            })
                        })
                    })
                    .collect();
                (
                    Silhouette::Solid(points),
                    Seat::Front(3_f64.sqrt() / 2. * 0.64),
                )
            }
            Cloud | Heart => {
                let p = planar(body);
                let p = if p.len() > 160 {
                    (0..160).map(|i| p[i * p.len() / 160]).collect()
                } else {
                    p
                };
                let (lo, hi) = bounds(&p);
                let center = (lo + hi) * 0.5;
                let depth = if body == Cloud { 0.72 } else { 0.62 };
                let radii = (0..180)
                    .map(|i| reach(&p, center, polar((i as f64 + 0.5) * TAU / 180.)))
                    .collect();
                let rings = (0..=16)
                    .map(|j| {
                        let a = -FRAC_PI_2 + PI * j as f64 / 16.;
                        let k = a.cos().max(0.).powf(0.72);
                        let z = depth * a.sin();
                        p.iter()
                            .map(|&p| {
                                let [x, y] = center.lerp(p, k).0;
                                v3(x, y, z)
                            })
                            .collect()
                    })
                    .collect();
                (
                    Silhouette::Rings {
                        rings,
                        mode: RingMode::Loft,
                    },
                    Seat::Loft {
                        center,
                        depth,
                        radii,
                    },
                )
            }
            Hex | Square | Sparkle | Clover | Flower | House | Star => {
                let p = planar(body);
                let (lo, hi) = bounds(&p);
                let center = (lo + hi) * 0.5;
                let [w, h] = ((hi - lo) * 0.5).0;
                let (depth, bevel) = if body == Star {
                    (0.3, 0.18)
                } else {
                    (0.36, 0.22)
                };
                let rings = (0..10)
                    .map(|i| {
                        let k = if i <= 4 { 4 - i } else { i - 5 };
                        let a = FRAC_PI_2 * k as f64 / 4.;
                        let z = (if i <= 4 { -1. } else { 1. }) * (depth - bevel + bevel * a.sin());
                        let scale = 1. - bevel / w.min(h) * (1. - a.cos());
                        p.iter()
                            .map(|&p| {
                                let [x, y] = center.lerp(p, scale).0;
                                v3(x, y, z)
                            })
                            .collect()
                    })
                    .collect();
                let mode = if convex(&p) {
                    RingMode::Convex
                } else {
                    RingMode::Extrusion
                };
                (Silhouette::Rings { rings, mode }, Seat::Front(depth))
            }
            _ => {
                let p = planar(body);
                let profile = Profile::new(p.clone());
                let lo = profile.lo;
                let hi = profile.hi;
                let width = hi - lo;
                let eye_y = match body {
                    Cylinder | Wedge | Teardrop => (hi + lo) * 0.5,
                    Arch => mix(hi, lo, 0.42),
                    _ => 0_f64.clamp(lo + 0.02, hi - 0.02),
                };
                let r = profile.at(eye_y);
                let outline = if body == Bean {
                    Silhouette::Balls(inscribed_balls(&p))
                } else {
                    let joins = [(hi, -1.), (lo, 1.)]
                        .into_iter()
                        .filter_map(|(y, s)| {
                            let base = profile.raw(y);
                            if base >= 0.06 {
                                return None;
                            }
                            let d = (1..=25)
                                .map(|i| width * i as f64 / 512.)
                                .find(|&d| (profile.raw(y + s * d) - base).abs() > 1e-4)
                                .unwrap_or(width * 0.02);
                            Some((y, s, d, profile.raw(y + s * d)))
                        })
                        .collect::<Vec<_>>();
                    let samples = (0..=95)
                        .map(|i| {
                            let y = mix(hi, lo, (1. - (PI * i as f64 / 95.).cos()) / 2.);
                            let mut r = profile.raw(y);
                            for &(edge, s, d, j) in &joins {
                                let t = (y - edge) * s;
                                if (0. ..d).contains(&t) {
                                    r = r.min(j * (t / d).sqrt());
                                }
                            }
                            if r < if i == 0 || i == 95 { 0.06 } else { 1e-4 } {
                                r = 0.;
                            }
                            (y, r)
                        })
                        .collect();
                    Silhouette::Revolve(samples)
                };
                (
                    outline,
                    Seat::Wrap {
                        r,
                        lo,
                        hi,
                        eye_y,
                        profile: Some(profile),
                        puffy: body == Bean,
                    },
                )
            }
        };
        Self {
            size,
            outline,
            seat,
        }
    }
}

const SAMPLE_AROUND: usize = 480;
static RAYS: LazyLock<[V2; SAMPLE_AROUND]> =
    LazyLock::new(|| std::array::from_fn(|i| polar(i as f64 * TAU / SAMPLE_AROUND as f64)));
static REVOLVE: LazyLock<[V2; 64]> =
    LazyLock::new(|| std::array::from_fn(|i| polar(i as f64 * TAU / 64.)));
impl Silhouette {
    fn is_convex(&self) -> bool {
        matches!(
            self,
            Self::Revolve(_)
                | Self::Solid(_)
                | Self::Rings {
                    mode: RingMode::Convex,
                    ..
                }
        )
    }
    fn project(&self, m: [V3; 3], grid: f64) -> Vec<V2> {
        let to_view = |p| rotate(m, p).xy();
        match self {
            Self::Sphere => radial_circle(192),
            Self::Solid(p) => hull(p.iter().map(|&p| to_view(p)).collect()),
            Self::Revolve(rows) => {
                // The projected convex hull is the support envelope of the 64-sided lathe rows.
                let x = v2(m[0].0[0], m[1].0[0]);
                let y = v2(m[0].0[1], m[1].0[1]);
                let z = v2(m[0].0[2], m[1].0[2]);
                let rings = REVOLVE.map(|p| x * p.0[1] + z * p.0[0]);
                let mut candidates = Vec::with_capacity(rows.len() * 64);
                for &(h, r) in rows {
                    let center = y * h;
                    if r == 0. {
                        candidates.push(center);
                    } else {
                        candidates.extend(rings.iter().map(|&p| center + p * r));
                    }
                }
                hull(candidates)
            }
            Self::Balls(p) => {
                let balls: Vec<_> = p.iter().map(|p| (to_view(p.center), p.r)).collect();
                let center = mean(balls.iter().map(|&(p, _)| p), balls.len());
                (0..160)
                    .map(|i| {
                        let dir = polar(i as f64 * TAU / 160.);
                        center + dir * ball_reach(balls.iter().map(|&(p, r)| (p - center, r)), dir)
                    })
                    .collect()
            }
            Self::Rings { rings, mode } => {
                let projected: Vec<Vec<_>> = rings
                    .iter()
                    .map(|ring| ring.iter().map(|&p| to_view(p)).collect())
                    .collect();
                if matches!(mode, RingMode::Extrusion) {
                    let faces: Vec<_> = projected
                        .windows(2)
                        .flat_map(|p| {
                            edges(&p[0])
                                .zip(edges(&p[1]))
                                .map(|((a, b), (c, d))| [a, b, d, c])
                        })
                        .collect();
                    let n = projected.len();
                    let loops = contours::union(
                        [projected[0].as_slice(), projected[n - 1].as_slice()]
                            .into_iter()
                            .chain(faces.iter().map(|p| p.as_slice())),
                        grid,
                    );
                    if let Some(loop_points) = loops
                        .into_iter()
                        .max_by(|a, b| contours::area(a).abs().total_cmp(&contours::area(b).abs()))
                    {
                        return loop_points;
                    }
                }
                match mode {
                    RingMode::Convex => hull(projected.into_iter().flatten().collect()),
                    RingMode::Loft
                        if (projected[0][0] - projected[projected.len() - 1][0]).len() < 1e-6 =>
                    {
                        projected[projected.len() / 2].clone()
                    }
                    _ => {
                        let mut rays = [0_f64; SAMPLE_AROUND];
                        for r in &projected {
                            for (a, b) in edges(r) {
                                trace(&mut rays, a, b);
                            }
                        }
                        for pair in projected.windows(2) {
                            for (&a, &b) in pair[0].iter().zip(&pair[1]) {
                                trace(&mut rays, a, b);
                            }
                        }
                        RAYS.iter()
                            .zip(rays)
                            .filter_map(|(&p, r)| (r > 0.).then_some(p * r))
                            .collect()
                    }
                }
            }
        }
    }
}
fn trace(radii: &mut [f64; SAMPLE_AROUND], a: V2, b: V2) {
    let k = SAMPLE_AROUND as f64 / TAU;
    let x = a.angle().rem_euclid(TAU) * k;
    let mut d = b.angle().rem_euclid(TAU) * k - x;
    let n = SAMPLE_AROUND as f64;
    if d > n / 2. {
        d -= n;
    }
    if d < -n / 2. {
        d += n;
    }
    let start = x.min(x + d).floor() as i32 - 1;
    let end = x.max(x + d).ceil() as i32 + 1;
    for i in start..=end {
        let i = i.rem_euclid(SAMPLE_AROUND as i32) as usize;
        if let Some(r) = intersection(a, b, RAYS[i]) {
            radii[i] = radii[i].max(r);
        }
    }
}
fn ball_reach(p: impl Iterator<Item = (V2, f64)>, d: V2) -> f64 {
    p.fold(0., |max, (p, r)| {
        let b = d.dot(p);
        let disc = b * b - p.dot(p) + r * r;
        if disc <= 0. {
            max
        } else {
            max.max(b + disc.sqrt())
        }
    })
}
fn inscribed_balls(p: &[V2]) -> Vec<Ball> {
    let (lo, hi) = bounds(p);
    let [w, h] = (hi - lo).0;
    let step = w.max(h) / 40.;
    let mut pool = vec![];
    let mut y = hi.0[1] - step / 2.;
    while y > lo.0[1] {
        let mut x = lo.0[0] + step / 2.;
        while x < hi.0[0] {
            let q = v2(x, y);
            if contains(p, q) {
                let r = p
                    .iter()
                    .map(|&p| (p - q).len())
                    .fold(f64::INFINITY, f64::min);
                if r > step {
                    pool.push(Ball {
                        center: v3(x, y, 0.),
                        r,
                    });
                }
            }
            x += step;
        }
        y -= step;
    }
    pool.sort_by(|a, b| b.r.total_cmp(&a.r));
    let mut filtered: Vec<Ball> = vec![];
    for b in pool {
        if !filtered
            .iter()
            .any(|a| (b.center - a.center).len() + b.r <= a.r + step * 0.25)
        {
            filtered.push(b);
        }
    }
    filtered
}

struct Profile {
    p: Vec<V2>,
    lo: f64,
    hi: f64,
    bins: [Vec<usize>; 64],
}
impl Profile {
    fn new(p: Vec<V2>) -> Self {
        let (a, b) = bounds(&p);
        let mut s = Self {
            p,
            lo: a.0[1],
            hi: b.0[1],
            bins: std::array::from_fn(|_| vec![]),
        };
        for (i, (a, b)) in edges(&s.p).enumerate() {
            let start = s.bin(a.0[1].min(b.0[1]) - 1e-5);
            let end = s.bin(a.0[1].max(b.0[1]) + 1e-5);
            for j in start..=end {
                s.bins[j].push(i);
            }
        }
        s
    }
    fn bin(&self, y: f64) -> usize {
        ((y - self.lo + 1e-5) / (self.hi - self.lo + 2e-5) * 64.).clamp(0., 63.) as usize
    }
    fn raw(&self, y: f64) -> f64 {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &i in &self.bins[self.bin(y)] {
            let [ax, ay] = self.p[i].0;
            let [bx, by] = self.p[(i + 1) % self.p.len()].0;
            if (by - ay).abs() < 1e-9 {
                if (ay - y).abs() < 1e-6 {
                    lo = lo.min(ax.min(bx));
                    hi = hi.max(ax.max(bx));
                }
            } else if y >= ay.min(by) && y <= ay.max(by) {
                let x = mix(ax, bx, (y - ay) / (by - ay));
                lo = lo.min(x);
                hi = hi.max(x);
            }
        }
        ((hi - lo) * 0.5).max(0.)
    }
    fn at(&self, y: f64) -> f64 {
        self.raw(y.clamp(self.lo, self.hi)).max(0.02)
    }
}

fn eye_loops(body: Body, eyes: &[Eye; 2]) -> (Vec<Vec<V2>>, f64) {
    use Body::*;
    let (w, h, g, shift) = match body {
        Cloud => (0.21294, 0.46683, 0.470016, 0.07),
        Square => (0.2553258426966292, 0.559752808988764, 0.529408, 0.12),
        Sparkle => (0.18382, 0.40299, 0.41216, 0.),
        Clover => (0.23296, 0.51072, 0.559104, -0.02),
        Heart => (0.186576, 0.409032, 0.446208, 0.052),
        Flower => (0.19292, 0.42294, 0.382976, 0.12),
        Teardrop => (0.207792, 0.455544, 0.452608, 0.18),
        Tablet => (0.2132, 0.4674, 0.587776, 0.02),
        Wedge => (0.2132, 0.4674, 0.467712, 0.18),
        House => (0.2295124, 0.5031618, 0.512512, 0.18499534000463813),
        Star => (0.184782, 0.44965989, 0.41913344, 0.12),
        Pebble => (0.2392, 0.5244, 0.546304, 0.12),
        Bean => (0.2028, 0.4446, 0.4608, 0.12),
        Egg => (0.2392, 0.5244, 0.433664, 0.11299906800092761),
        Squircle => (0.2392, 0.5244, 0.5632, 0.12),
        Capsule => (0.2106, 0.4617, 0.360448, 0.12),
        Cylinder => (0.2392, 0.5244, 0.47872, 0.12),
        Gem => (0.2392, 0.5244, 0.501248, 0.12),
        Crystal => (0.1612, 0.3534, 0.3712, 0.12),
        Shield => (0.267904, 0.587328, 0.444928, 0.12),
        Dome => (0.2132, 0.4674, 0.47872, 0.10599813600185524),
        Arch => (0.1924, 0.4218, 0.448, 0.16),
        Leaf => (0.1872, 0.4104, 0.448, 0.04),
        _ => (0.26, 0.57, 0.64, 0.12),
    };
    let x_scale = w / 0.26;
    let y_scale = h / 0.57;
    let gap_scale = g / 0.64;
    if body != Pumpkin && eyes.iter().all(|e| e.is_oval() && e.ring.is_none()) {
        let (w, h, a, p) = eyes[0].dimensions();
        let (w1, h1, a1, q) = eyes[1].dimensions();
        let gap = ((0.64 + p.0[0] + q.0[0]) * gap_scale).clamp(0.05, 1.8);
        let ellipse = |w: f64, h: f64, a: f64, side: f64| {
            let w = w * x_scale;
            let h = h * x_scale;
            let theta = a + if w >= h { 0. } else { FRAC_PI_2 };
            (0..64)
                .map(|i| {
                    let d = polar(TAU * i as f64 / 64.);
                    let p = v2(
                        side * w.max(h).clamp(0.02, 1.8) * 0.5 * d.0[0],
                        w.min(h).clamp(0.01, 1.) * 0.5 * d.0[1],
                    );
                    p.rotate(-side * theta) + v2(side * gap / 2., 0.)
                })
                .collect()
        };
        return (
            vec![ellipse(w, h, a, -1.), ellipse(w1, h1, a1, 1.)],
            shift + (p.0[1] + q.0[1]) / 2. * y_scale,
        );
    }
    let loops = eyes
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            if e.width <= 0. && !e.is_oval() {
                return None;
            }
            let side = if i == 0 { -1. } else { 1. };
            let center = v2(
                side * (0.32 + e.off.0[0]) * gap_scale
                    + if body == Pumpkin { 0.04_f64.sin() } else { 0. },
                -shift.sin() + e.off.0[1] * y_scale,
            );
            let transform = |p: V2| {
                let p = p.rotate(e.tilt);
                center + v2(side * p.0[0], p.0[1] * e.lid) * x_scale
            };
            let r = if let Some(ring) = &e.ring {
                ring.iter()
                    .map(|&p| {
                        let p = p - e.off;
                        transform(v2(p.0[0], p.0[1] * e.lid))
                    })
                    .collect()
            } else if e.is_oval() {
                (0..48)
                    .map(|i| {
                        let p = polar(TAU * i as f64 / 48.);
                        transform(v2(p.0[0] * e.oval.0[0], p.0[1] * e.oval.0[1] * e.lid))
                    })
                    .collect()
            } else {
                e.spine_contour(transform, e.width * x_scale)
            };
            (r.len() >= 3).then_some(r)
        })
        .collect();
    (loops, 0.)
}

impl Seat {
    fn project(
        &self,
        outline: &Silhouette,
        (mut loops, shift): (Vec<Vec<V2>>, f64),
        m: [V3; 3],
    ) -> Vec<Vec<V2>> {
        let view = |x, y, z| rotate(m, v3(x, y, z)).xy();
        let front = m[2].0[2];
        let visible = |x, y, z| m[2].dot(v3(x, y, z));
        let fit = |loops: &mut Vec<Vec<V2>>, axis: usize, lo: f64, hi: f64, center: f64| {
            let (a, b) = loops
                .iter()
                .flatten()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), p| {
                    (a.min(p.0[axis]), b.max(p.0[axis]))
                });
            if b - a <= hi - lo {
                let d = if a + center < lo {
                    lo - a - center
                } else if b + center > hi {
                    hi - b - center
                } else {
                    0.
                };
                for p in loops.iter_mut().flatten() {
                    p.0[axis] += d;
                }
            }
        };
        match self {
            Self::None => vec![],
            Self::Front(z) => {
                if front > 0.03 {
                    loops
                        .iter()
                        .map(|p| {
                            p.iter()
                                .map(|p| view(p.0[0], p.0[1] - shift.sin(), *z))
                                .collect()
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
            Self::Loft {
                center,
                depth,
                radii,
            } => {
                if front < 0.05 {
                    return vec![];
                }
                let z = |p: V2| {
                    let p = p - *center;
                    let r = p.len();
                    if r < 1e-8 {
                        return *depth;
                    }
                    let i = p.angle().rem_euclid(TAU) / TAU * 180.;
                    let a = i.floor() as usize % 180;
                    let k = mix(radii[a], radii[(a + 1) % 180], i.fract()).max(1e-8);
                    depth * (1. - (r / k).min(1.).powf(2. / 0.72)).max(0.).sqrt()
                };
                loops
                    .iter()
                    .filter_map(|p| {
                        let c = mean(p.iter().copied(), p.len()) - v2(0., shift.sin());
                        let h = 0.004;
                        let u = (z(c + v2(h, 0.)) - z(c - v2(h, 0.))) / (2. * h);
                        let v = (z(c + v2(0., h)) - z(c - v2(0., h))) / (2. * h);
                        let n = v3(-u, -v, 1.).unit();
                        if n.dot(m[2]) < 0.12 {
                            return None;
                        }
                        let z = z(c);
                        Some(
                            p.iter()
                                .map(|&p| {
                                    let [x, y] = (p - v2(0., shift.sin())).0;
                                    view(x, y, z + (x - c.0[0]) * u + (y - c.0[1]) * v)
                                })
                                .collect(),
                        )
                    })
                    .collect()
            }
            Self::Wrap {
                r,
                lo,
                hi,
                eye_y,
                profile,
                puffy,
            } => {
                let pad = if profile.is_some() { 0.02 } else { 0.06 };
                let center = (eye_y - shift).clamp(lo + pad, hi - pad);
                fit(&mut loops, 1, *lo, *hi, center);
                loops
                    .iter()
                    .filter_map(|p| {
                        let p: Vec<_> = if let (true, Silhouette::Balls(balls)) = (*puffy, outline)
                        {
                            let c = mean(p.iter().copied(), p.len()) + v2(0., center);
                            let c = v2(c.0[0], c.0[1].clamp(*lo, *hi));
                            let score = |b: &Ball| {
                                let d = (c - b.center.xy()).len();
                                if d < b.r {
                                    b.center.0[2] + (b.r * b.r - d * d).sqrt()
                                } else {
                                    b.r - d
                                }
                            };
                            let b = balls.iter().max_by(|a, b| score(a).total_cmp(&score(b)))?;
                            p.iter()
                                .filter_map(|p| {
                                    let d = v2(p.0[0], (center + p.0[1]).clamp(*lo, *hi))
                                        - b.center.xy();
                                    let unit = if d.len() > 1e-9 { d.unit() } else { v2(1., 0.) };
                                    let mut l = (d.len() / b.r).min(PI - 0.02);
                                    let normal = |a: f64| {
                                        v3(unit.0[0] * a.sin(), unit.0[1] * a.sin(), a.cos())
                                    };
                                    if normal(l).dot(m[2]) < 0.06 && front > 0.06 {
                                        let mut a = 0.;
                                        let mut z = l;
                                        for _ in 0..20 {
                                            let h = (a + z) / 2.;
                                            if normal(h).dot(m[2]) >= 0.06 {
                                                a = h
                                            } else {
                                                z = h
                                            }
                                        }
                                        l = a;
                                    }
                                    let n = normal(l);
                                    (n.dot(m[2]) >= 0.05)
                                        .then(|| rotate(m, b.center + n * b.r).xy())
                                })
                                .collect()
                        } else {
                            p.iter()
                                .filter_map(|p| {
                                    let y = (center + p.0[1]).clamp(*lo, *hi);
                                    let radius = |y| profile.as_ref().map_or(*r, |p| p.at(y));
                                    let r = radius(y);
                                    let a = (p.0[0] / r.max(0.12)).clamp(-2.6, 2.6);
                                    let d = -(radius((y + 0.012).min(*hi))
                                        - radius((y - 0.012).max(*lo)))
                                        / 0.024;
                                    (visible(a.sin(), d, a.cos()) >= 0.05)
                                        .then(|| view(r * a.sin(), y, r * a.cos()))
                                })
                                .collect()
                        };
                        (p.len() > 2).then_some(p)
                    })
                    .collect()
            }
            Self::WrapX { r, lo, hi, flat } => {
                let center = (-shift / r).clamp(-2.4, 2.4);
                let radius = |x: f64| {
                    let d = (x.abs() - flat).max(0.);
                    (r * r - d * d).max(0.).sqrt()
                };
                fit(&mut loops, 0, *lo, *hi, 0.);
                loops
                    .iter()
                    .filter_map(|p| {
                        let p: Vec<_> = p
                            .iter()
                            .filter_map(|p| {
                                let x = p.0[0].clamp(*lo, *hi);
                                let a = center + (p.0[1] / r.max(0.12)).clamp(-2.6, 2.6);
                                let n = -(radius((x + 0.012).min(*hi))
                                    - radius((x - 0.012).max(*lo)))
                                    / 0.024;
                                (visible(n, a.sin(), a.cos()) >= 0.05)
                                    .then(|| view(x, radius(x) * a.sin(), radius(x) * a.cos()))
                            })
                            .collect();
                        (p.len() > 2).then_some(p)
                    })
                    .collect()
            }
        }
    }
}

pub(super) fn mean(p: impl Iterator<Item = V2>, n: usize) -> V2 {
    p.fold(V2::default(), |a, b| a + b) / n.max(1) as f64
}
pub(super) fn edges(p: &[V2]) -> impl Iterator<Item = (V2, V2)> + '_ {
    p.iter().copied().zip(p.iter().cycle().skip(1).copied())
}
pub(super) fn contains(p: &[V2], q: V2) -> bool {
    let [x, y] = q.0;
    edges(p).fold(false, |hit, (a, b)| {
        let [ax, ay] = a.0;
        let [bx, by] = b.0;
        hit ^ ((ay > y) != (by > y) && x < (bx - ax) * (y - ay) / (by - ay) + ax)
    })
}
pub(super) fn bounds(p: &[V2]) -> (V2, V2) {
    p.iter().fold(
        (
            v2(f64::INFINITY, f64::INFINITY),
            v2(f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(a, b), p| {
            (
                v2(a.0[0].min(p.0[0]), a.0[1].min(p.0[1])),
                v2(b.0[0].max(p.0[0]), b.0[1].max(p.0[1])),
            )
        },
    )
}
pub(super) fn intersection(a: V2, b: V2, d: V2) -> Option<f64> {
    let v = b - a;
    let k = d.cross(v);
    if k.abs() < 1e-12 {
        return None;
    }
    let t = a.cross(d) / k;
    let r = a.cross(v) / k;
    (r >= 0. && (-1e-9..=1.000000001).contains(&t)).then_some(r)
}
pub(super) fn reach(p: &[V2], center: V2, d: V2) -> f64 {
    edges(p)
        .filter_map(|(a, b)| intersection(a - center, b - center, d))
        .fold(0., f64::max)
}
fn radial(p: &[V2], center: V2, n: usize) -> Vec<V2> {
    radial_circle(n)
        .into_iter()
        .map(|d| center + d * reach(p, center, d))
        .collect()
}
pub(super) fn radial_circle(n: usize) -> Vec<V2> {
    (0..n).map(|i| polar(TAU * i as f64 / n as f64)).collect()
}
fn convex(p: &[V2]) -> bool {
    let mut sign = 0.;
    for ((a, b), c) in edges(p).zip(p.iter().cycle().skip(2)) {
        let v = (b - a).cross(*c - b);
        if v.abs() < 1e-12 {
            continue;
        }
        if sign != 0. && v.signum() != sign {
            return false;
        }
        sign = v.signum();
    }
    true
}
pub(super) fn hull(mut p: Vec<V2>) -> Vec<V2> {
    if p.len() < 3 {
        return p;
    }
    p.sort_unstable_by(|a, b| a.0[0].total_cmp(&b.0[0]).then(a.0[1].total_cmp(&b.0[1])));
    let half = |iter: &mut dyn Iterator<Item = &V2>| {
        let mut out: Vec<V2> = vec![];
        for &p in iter {
            while out.len() >= 2 {
                let a = out[out.len() - 2];
                let b = out[out.len() - 1];
                if (b - a).cross(p - b) > 0. {
                    break;
                }
                out.pop();
            }
            out.push(p);
        }
        out.pop();
        out
    };
    let mut out = half(&mut p.iter());
    out.extend(half(&mut p.iter().rev()));
    out
}

fn decode(s: &str) -> Vec<V2> {
    STANDARD
        .decode(s)
        .expect("authored planar outline")
        .chunks_exact(4)
        .map(|b| {
            v2(
                i16::from_le_bytes([b[0], b[1]]) as f64,
                i16::from_le_bytes([b[2], b[3]]) as f64,
            ) / 16000.
        })
        .collect()
}
fn planar(body: Body) -> Vec<V2> {
    use Body::*;
    let regular = |r: f64, n: usize, start: f64| {
        (0..n)
            .map(|i| polar(i as f64 * TAU / n as f64 + start) * r)
            .collect::<Vec<_>>()
    };
    let smooth_shape = |f: &dyn Fn(f64) -> V2, n| {
        let p: Vec<_> = (0..n).map(|i| f(i as f64 * TAU / n as f64)).collect();
        catmull(&p)
    };
    let superellipse = |x, y, n| {
        smooth_shape(
            &|t| {
                let [c, s] = polar(t).0;
                v2(
                    c.signum() * c.abs().powf(2. / n) * x,
                    s.signum() * s.abs().powf(2. / n) * y,
                )
            },
            128,
        )
    };
    let ball_ring = |balls: &[(V2, f64)], n| {
        smooth_shape(
            &|t| {
                let d = polar(t);
                d * ball_reach(balls.iter().copied(), d)
            },
            n,
        )
    };
    let p = match body {
        Heart => return decode(outline_data::HEART),
        Sparkle => return decode(outline_data::SPARKLE),
        Star => return decode(outline_data::STAR),
        Flower => {
            return (0..240)
                .map(|i| {
                    let d = polar(i as f64 * TAU / 240.);
                    let mut lo = 0.;
                    let mut hi = 0.91;
                    for _ in 0..22 {
                        let m = (lo + hi) * 0.5;
                        let p = d * m;
                        if p.len() <= 0.36
                            || (0..6).any(|i| {
                                (p - polar(-FRAC_PI_2 + i as f64 * TAU / 6.) * 0.58).len() <= 0.33
                            })
                        {
                            lo = m
                        } else {
                            hi = m;
                        }
                    }
                    d * lo
                })
                .collect()
        }
        Pebble => smooth_shape(
            &|t| {
                let s =
                    108. * (1. + 0.075 * ((2. * t + 1.1).sin() * 0.6 + (3. * t - 1.1).sin() * 0.4));
                v2(t.cos() * s, t.sin() * s * 0.98)
            },
            128,
        ),
        Bean => smooth_shape(
            &|t| {
                let f = |a: f64| (-((t - a + PI).rem_euclid(TAU) - PI).powi(2) / 0.4232).exp();
                let k = 1. - 0.34 * f(PI) + 0.34 * 0.34 * f(TAU);
                v2(t.cos() * 94., t.sin() * 112.) * k
            },
            128,
        ),
        Egg => smooth_shape(
            &|t| {
                let [x, y] = polar(t).0;
                v2(x * 98. * (1. - 0.22 * ((1. - y) / 2.).powi(2)), y * 113.)
            },
            128,
        ),
        Squircle => superellipse(107., 107., 3.2),
        Gem => superellipse(112., 113., 1.5),
        Leaf => smooth_shape(
            &|t| {
                let [x, y] = polar(t).0;
                v2(x * 88. * (1. - y * y).max(0.).powf(0.25), y * 113.)
            },
            128,
        ),
        Cloud => ball_ring(
            &[
                (v2(-62., 26.), 56.),
                (v2(62., 26.), 54.),
                (v2(0., 34.), 62.),
                (v2(-24., -30.), 62.),
                (v2(38., -26.), 54.),
            ],
            160,
        ),
        Clover => ball_ring(
            &[
                (v2(-38., -38.), 58.),
                (v2(38., -38.), 58.),
                (v2(38., 38.), 58.),
                (v2(-38., 38.), 58.),
            ],
            160,
        ),
        Square => rounded(
            &[
                v2(-107., -107.),
                v2(107., -107.),
                v2(107., 107.),
                v2(-107., 107.),
            ],
            &[28.],
        ),
        Hex => rounded(&regular(114., 6, PI / 6.), &[20.]),
        Wedge => rounded(&regular(130., 3, -FRAC_PI_2), &[80., 60., 60.]),
        House => rounded(
            &[
                v2(-100., 110.),
                v2(100., 110.),
                v2(100., -32.),
                v2(0., -110.),
                v2(-100., -32.),
            ],
            &[29.],
        ),
        Tablet => {
            let (r, x) = (74., 40.);
            let mut p = arc(v2(x, 0.), r, r, -FRAC_PI_2, FRAC_PI_2);
            p.extend(arc(v2(-x, 0.), r, r, FRAC_PI_2, PI * 1.5));
            close_lines(&p)
        }
        Capsule | Cylinder => {
            let (x, y, h) = if body == Capsule {
                (72., 113., 72.)
            } else {
                (94., 110., 48.)
            };
            let mut p = arc(v2(0., -y + h), x, h, PI, TAU);
            p.extend(arc(v2(0., y - h), x, h, 0., PI));
            close_lines(&p)
        }
        Dome | Arch => {
            let (x, y, n, cy, ry) = if body == Dome {
                (114., 82., 26., 56., 138.)
            } else {
                (76., 113., 42., -37., 76.)
            };
            let mut p = arc(v2(0., cy), x, ry, PI, TAU);
            p.extend(line(v2(x, cy), v2(x, y - n)));
            p.extend(curve([v2(x, y - n), v2(x, y), v2(x, y), v2(x - n, y)]));
            p.extend(line(v2(x - n, y), v2(n - x, y)));
            p.extend(curve([v2(n - x, y), v2(-x, y), v2(-x, y), v2(-x, y - n)]));
            close_lines(&p)
        }
        Shield => {
            let a = v2(-98., -78.);
            let b = v2(0., -118.);
            let c = v2(98., -78.);
            let d = v2(0., 108.);
            [
                [a, v2(-98., -110.), v2(-49., -118.), b],
                [b, v2(49., -118.), v2(98., -110.), c],
                [c, v2(98., 45.36), v2(60.76, 108.), d],
                [d, v2(-60.76, 108.), v2(-98., 45.36), a],
            ]
            .into_iter()
            .flat_map(curve)
            .collect()
        }
        Teardrop => return teardrop(48.),
        _ => regular(114., 128, 0.),
    };
    normalize(p)
}
pub(super) fn pencil_ring() -> &'static [V2] {
    static RING: LazyLock<Vec<V2>> = LazyLock::new(|| radial(&teardrop(18.), V2::default(), 96));
    &RING
}
fn teardrop(round: f64) -> Vec<V2> {
    let r = 88_f64;
    let y = 26.;
    let s = r / 140.;
    let x = r * (1. - s * s).sqrt();
    let a = v2(x, y - r * s);
    let b = v2(0., -114.);
    let c = v2(-x, y - r * s);
    let mut p = corner(a, b, c, round);
    let start = *p.last().unwrap_or(&c);
    p.extend(line(start, c));
    p.extend(arc(
        v2(0., y),
        r,
        r,
        PI - (a.0[1] - y).atan2(a.0[0]),
        (a.0[1] - y).atan2(a.0[0]),
    ));
    normalize(p)
}
fn normalize(p: Vec<V2>) -> Vec<V2> {
    let (lo, hi) = bounds(&p);
    let c = (lo + hi) * 0.5;
    let [x, y] = (hi - lo).0;
    let k = (228.44 / x.max(y)).clamp(0.9, 1.35);
    p.into_iter()
        .map(|p| {
            let q = (if (k - 1.).abs() < 0.005 && c.0.iter().all(|v| v.abs() < 0.5) {
                p
            } else {
                (p - c) * k
            }) / L;
            v2(q.0[0], -q.0[1])
        })
        .collect()
}
fn close_lines(p: &[V2]) -> Vec<V2> {
    edges(p).flat_map(|(a, b)| line(a, b)).collect()
}
fn line(a: V2, b: V2) -> Vec<V2> {
    let n = ((a - b).len() / 4.).ceil().max(2.) as usize;
    (0..n).map(|i| a.lerp(b, i as f64 / n as f64)).collect()
}
fn curve(p: [V2; 4]) -> Vec<V2> {
    let n = (p.windows(2).map(|p| (p[1] - p[0]).len()).sum::<f64>() / 4.)
        .ceil()
        .max(2.) as usize;
    (0..n).map(|i| cubic(p, i as f64 / n as f64)).collect()
}
fn catmull(p: &[V2]) -> Vec<V2> {
    let n = p.len();
    (0..n)
        .flat_map(|i| {
            let a = p[i];
            let b = p[(i + 1) % n];
            curve([
                a,
                a + (b - p[(i + n - 1) % n]) / 6.,
                b - (p[(i + 2) % n] - a) / 6.,
                b,
            ])
        })
        .collect()
}
fn arc(center: V2, rx: f64, ry: f64, a: f64, b: f64) -> Vec<V2> {
    let n = ((b - a).abs() / FRAC_PI_2).ceil().max(1.) as usize;
    let step = (b - a) / n as f64;
    let k = 4. / 3. * (step / 4.).tan();
    let p = |t: f64| v2(rx * t.cos(), ry * t.sin());
    let tangent = |t: f64| v2(-rx * t.sin(), ry * t.cos());
    let mut out: Vec<_> = (0..n)
        .flat_map(|i| {
            let t = a + i as f64 * step;
            let u = t + step;
            curve([
                center + p(t),
                center + p(t) + tangent(t) * k,
                center + p(u) - tangent(u) * k,
                center + p(u),
            ])
        })
        .collect();
    out.push(center + p(b));
    out
}
fn corner(a: V2, b: V2, c: V2, r: f64) -> Vec<V2> {
    let p = b + (a - b).unit() * r;
    let q = b + (c - b).unit() * r;
    let n = (((p - b).len() + (q - b).len()) / 4.).ceil().max(2.) as usize;
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            p.lerp(b, t).lerp(b.lerp(q, t), t)
        })
        .collect()
}
fn rounded(p: &[V2], r: &[f64]) -> Vec<V2> {
    let n = p.len();
    let corners: Vec<_> = (0..n)
        .map(|i| corner(p[(i + n - 1) % n], p[i], p[(i + 1) % n], r[i % r.len()]))
        .collect();
    let mut out = vec![];
    for i in 0..n {
        let a = &corners[i];
        out.extend(a);
        out.extend(line(a[a.len() - 1], corners[(i + 1) % n][0]));
    }
    out
}
