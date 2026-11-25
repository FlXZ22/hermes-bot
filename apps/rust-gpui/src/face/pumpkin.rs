//! The pumpkin is a radially parameterized solid; its stem has its own painted contours.
use super::{
    math::*,
    shapes::{edges, hull, Projected},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    f64::consts::{FRAC_PI_2, PI},
    rc::Rc,
    sync::LazyLock,
};

const RAYS: usize = 256;
const RADIUS: f64 = 1.08;
const STEM_ROWS: usize = 72;
const STEM_SIDES: usize = 48;
fn radius(p: V3) -> f64 {
    let [x, y, z] = p.0;
    let r = 1.
        / ((x.hypot(z) / RADIUS).powf(2.2) + (y.abs() / (RADIUS * 0.84)).powf(2.2)).powf(1. / 2.2);
    let i = (4. * (z.atan2(x) - FRAC_PI_2 - 19. * DEG)).cos().abs();
    let a = if y < 0. {
        (1. - y * y).max(0.).powf(0.2)
    } else {
        (1. - y * y).max(0.).sqrt() * (1. - 0.45 * y)
    };
    let o = 1. - 0.16 * (1. - i.powf(0.6)) * a;
    let s = (3. * y.clamp(-1., 1.).asin()).cos().abs().powf(0.6);
    let c = if y < 0. { 1. } else { (1. - 2.2 * y).max(0.) };
    let l = 1. - 0.16 * 0.55 * 0.9 * (1. - s) * c;
    let u = if y > 0. {
        0.11 * (-(1. - y) / 0.02).exp()
    } else {
        0.03 * (-(1. + y) / 0.03).exp()
    };
    r * o * l * (1. - u)
}
static RADII: LazyLock<Vec<f32>> = LazyLock::new(|| {
    (0..=160)
        .flat_map(|i| {
            let (y, r) = (-FRAC_PI_2 + PI * i as f64 / 160.).sin_cos();
            (0..=320).map(move |j| {
                let [x, z] = (polar(-PI + TAU * j as f64 / 320.) * r).0;
                radius(v3(x, y, z)) as f32
            })
        })
        .collect()
});
fn surface(p: V3) -> f64 {
    let [x, y, z] = p.0;
    let n = p.len().max(1e-12);
    let a = ((y / n).clamp(-1., 1.).asin() + FRAC_PI_2) / PI * 160.;
    let b = (z.atan2(x) + PI) / TAU * 320.;
    let i = (a.floor() as usize).min(159);
    let j = (b.floor() as usize).min(319);
    let d = i * 321 + j;
    let t = &*RADII;
    mix(
        mix(t[d] as f64, t[d + 1] as f64, b - j as f64),
        mix(t[d + 321] as f64, t[d + 322] as f64, b - j as f64),
        a - i as f64,
    )
}
static REACH: LazyLock<[(f64, f64); 49]> = LazyLock::new(|| {
    let mut a = std::array::from_fn(|i| {
        let (s, c) = ((i as f64 - 24.) / 24. * 1.25).sin_cos();
        (c, s)
    });
    a.sort_by(|a, b| b.0.total_cmp(&a.0));
    a
});
struct PumpkinProjection {
    outline: Vec<V2>,
    clip: Vec<V2>,
    painted: Vec<(u32, Vec<V2>)>,
}
type PumpkinCache = HashMap<([[u64; 3]; 3], u32), Rc<PumpkinProjection>>;
thread_local! {static PUMPKINS:RefCell<PumpkinCache>=RefCell::default();}

pub(super) fn project(m: [V3; 3], sprite: f64, eyes: Option<(Vec<Vec<V2>>, f64)>) -> Projected {
    let inv = std::array::from_fn(|i| VecN(std::array::from_fn(|j| m[j].0[i])));
    let projection = PUMPKINS.with_borrow_mut(|cache| {
        let key = (m.map(|v| v.0.map(f64::to_bits)), sprite as u32);
        if let Some(projected) = cache.get(&key) {
            return projected.clone();
        }
        if cache.len() >= 128 {
            cache.clear();
        }
        let reaches: [f64; RAYS] = std::array::from_fn(|i| {
            let d = polar(TAU * i as f64 / RAYS as f64);
            let mut max = 0_f64;
            for &(x, z) in REACH.iter() {
                if x * RADIUS < max {
                    break;
                }
                max = max.max(x * surface(rotate(inv, v3(d.0[0] * x, d.0[1] * x, z))));
            }
            max
        });
        let outline = reaches
            .iter()
            .enumerate()
            .map(|(i, &r)| polar(TAU * i as f64 / RAYS as f64) * r)
            .collect();
        let clip = hull(
            reaches
                .iter()
                .enumerate()
                .map(|(i, &r)| polar(TAU * i as f64 / RAYS as f64) * (r - 0.015).max(0.05))
                .collect(),
        );
        let painted = stem(m, inv, sprite, &reaches);
        let projected = Rc::new(PumpkinProjection {
            outline,
            clip,
            painted,
        });
        cache.insert(key, projected.clone());
        projected
    });
    let mut result = Projected {
        outline: projection.outline.clone(),
        eyes: vec![],
        painted: projection.painted.clone(),
    };
    if let Some((eyes, _)) = eyes {
        let mount = |p: V2| {
            let l = p.len();
            let q = p * (if l < 1e-8 { 1. } else { l.sin() / l });
            rotate(m, v3(q.0[0], q.0[1], l.cos()))
        };
        let point = |p: V3| {
            let p = p.unit();
            p * surface(rotate(inv, p)) * 1.002
        };
        let visible = |p: V3| {
            if p.0[2] / p.len().max(1e-12) < -0.35 {
                return false;
            }
            let p = point(p);
            !(1..=48).any(|i| {
                let q = p + v3(0., 0., 0.01 + 2.2 * i as f64 / 48.);
                let n = q.len();
                n <= RADIUS && n < surface(rotate(inv, q))
            })
        };
        for eye in eyes {
            let p: Vec<_> = eye.iter().map(|&p| mount(p)).collect();
            let vis: Vec<_> = p.iter().map(|&p| visible(p)).collect();
            if !vis.contains(&true) {
                continue;
            }
            let n = p.len();
            let mut loop_points = vec![];
            for i in 0..n {
                let j = (i + 1) % n;
                if vis[i] {
                    loop_points.push(point(p[i]).xy());
                }
                if vis[i] != vis[j] {
                    let (mut a, mut b) = if vis[i] { (p[i], p[j]) } else { (p[j], p[i]) };
                    for _ in 0..14 {
                        let c = a.lerp(b, 0.5);
                        if visible(c) {
                            a = c
                        } else {
                            b = c
                        }
                    }
                    loop_points.push(point(a).xy());
                }
            }
            let clipped = clip_convex(loop_points, &projection.clip);
            if clipped.len() >= 3 {
                result.eyes.push(clipped);
            }
        }
    }
    result
}

pub(super) fn clip_convex(mut p: Vec<V2>, clip: &[V2]) -> Vec<V2> {
    for (a, b) in edges(clip) {
        if p.len() < 3 {
            return vec![];
        }
        let d = b - a;
        if p.iter().all(|&p| d.cross(p - a) >= 0.) {
            continue;
        }
        let mut next = vec![];
        for (x, y) in edges(&p) {
            let f = d.cross(x - a);
            let g = d.cross(y - a);
            if f >= 0. {
                next.push(x);
            }
            if (f >= 0.) != (g >= 0.) {
                next.push(x.lerp(y, f / (f - g)));
            }
        }
        p = next;
    }
    p
}

static STEM: LazyLock<Vec<V3>> = LazyLock::new(|| {
    let base = radius(v3(0., 1., 0.));
    let angle = |x: f64| -65. * DEG * x.powf(2.2);
    let mut top = base;
    for i in 1..=24 {
        let (s, c) = (i as f64 / 24. * 0.6).sin_cos();
        for j in 0..32 {
            let [x, z] = (polar(j as f64 * TAU / 32.) * s).0;
            top = top.max(radius(v3(x, c, z)) * c);
        }
    }
    let mut center = v2(0., base - 0.08);
    let mut vertices = Vec::new();
    for i in 0..=STEM_ROWS {
        let t = i as f64 / STEM_ROWS as f64;
        let a = angle(t);
        let c = sat(1. - (center.0[1] - (top + 0.01)) / 0.09);
        let r = 0.115 + 0.055 * (1. - t).powi(2) + 0.06 * c * c;
        for j in 0..STEM_SIDES {
            let (s, c) = (TAU * j as f64 / STEM_SIDES as f64).sin_cos();
            vertices.push(v3(
                center.0[0] + r * c * a.cos(),
                center.0[1] - r * c * a.sin(),
                r * s,
            ));
        }
        if i < STEM_ROWS {
            for j in 0..8 {
                let a = angle(t + (j as f64 + 0.5) / 8. / STEM_ROWS as f64);
                let (s, c) = a.sin_cos();
                center = center + v2(s, c) * 0.68 / (8 * STEM_ROWS) as f64;
            }
        }
    }
    vertices.push(v3(center.0[0], center.0[1], 0.));
    vertices
});
fn stem(m: [V3; 3], inv: [V3; 3], sprite: f64, reaches: &[f64; RAYS]) -> Vec<(u32, Vec<V2>)> {
    let size = sprite * 1.17 * 0.36 / 1.18;
    let vertices: Vec<V3> = STEM.iter().map(|&p| rotate(m, p)).collect();
    let folded = vertices.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |[x, y, u, v], p| {
            let [a, b] = (p.xy() * size).0;
            [x.min(a), y.min(b), u.max(a), v.max(b)]
        },
    );
    let [x0, y0, x1, y1] = folded.map(|v| v.floor() as i32);
    let (x0, y0) = (x0 - 1, y0 - 1);
    let w = (x1 - x0 + 3) as usize;
    let h = (y1 - y0 + 3) as usize;
    let mut grid = vec![(f64::NEG_INFINITY, 0_u8); w * h];
    let occluded = |x: f64, y: f64, z: f64| {
        let r = x.hypot(y);
        let a = (y.atan2(x) / TAU * RAYS as f64)
            .floor()
            .rem_euclid(RAYS as f64) as usize;
        if r > RADIUS || r > 1.01 * reaches[a].max(reaches[(a + 1) % RAYS]) {
            return false;
        }
        let p = rotate(inv, v3(x, y, 0.));
        let d = rotate(inv, v3(0., 0., 1.));
        let mut z = z + 0.012;
        while z <= RADIUS {
            let q = p + d * z;
            if q.len() < surface(q) {
                return true;
            }
            z += 0.02;
        }
        false
    };
    let mut triangle = |a: usize, b: usize, c: usize, paint: u8| {
        let [a, b, c] = [vertices[a], vertices[b], vertices[c]];
        let u = (b - a).xy() * size;
        let v = (c - a).xy() * size;
        let det = u.cross(v);
        if det.abs() < 1e-8 {
            return;
        }
        let o = a.xy() * size;
        let low = (o + v2(0_f64.min(u.0[0]).min(v.0[0]), 0_f64.min(u.0[1]).min(v.0[1]))).0;
        let high = (o + v2(0_f64.max(u.0[0]).max(v.0[0]), 0_f64.max(u.0[1]).max(v.0[1]))).0;
        for y in (low[1].floor() as i32).max(y0)..(high[1].ceil() as i32).min(y0 + h as i32) {
            for x in (low[0].floor() as i32).max(x0)..(high[0].ceil() as i32).min(x0 + w as i32) {
                let q = v2(x as f64 + 0.5, y as f64 + 0.5) - o;
                let i = q.cross(v) / det;
                let j = u.cross(q) / det;
                if i < 0. || j < 0. || i + j > 1. {
                    continue;
                }
                let p = a + (b - a) * i + (c - a) * j;
                let cell = &mut grid[(y - y0) as usize * w + (x - x0) as usize];
                if p.0[2] > cell.0 && !occluded(p.0[0], p.0[1], p.0[2]) {
                    *cell = (p.0[2], paint);
                }
            }
        }
    };
    for i in 0..STEM_ROWS {
        for j in 0..STEM_SIDES {
            let a = i * STEM_SIDES + j;
            let b = i * STEM_SIDES + (j + 1) % STEM_SIDES;
            triangle(a, b, b + STEM_SIDES, 1);
            triangle(a, b + STEM_SIDES, a + STEM_SIDES, 1);
        }
    }
    let start = STEM_ROWS * STEM_SIDES;
    for i in 0..STEM_SIDES {
        triangle(
            vertices.len() - 1,
            start + i,
            start + (i + 1) % STEM_SIDES,
            2,
        );
    }
    let mut contours = vec![];
    for (paint, color) in [(1, 0x3d3d3d), (2, 0x525252)] {
        let on = |x: usize, y: usize| {
            let p = grid[y * w + x].1;
            p >= paint
        };
        // March the half-pixel boundaries of the two visibility masks, then paint the resulting vectors.
        let mut links: HashMap<(usize, usize), Vec<(usize, usize)>> = HashMap::new();
        for y in 0..h - 1 {
            for x in 0..w - 1 {
                let v = [on(x, y), on(x + 1, y), on(x + 1, y + 1), on(x, y + 1)];
                let pts = [
                    (2 * x + 1, 2 * y),
                    (2 * x + 2, 2 * y + 1),
                    (2 * x + 1, 2 * y + 2),
                    (2 * x, 2 * y + 1),
                ];
                let change: Vec<_> = (0..4).filter(|&i| v[i] != v[(i + 1) % 4]).collect();
                let mut connect = |i: usize, j: usize| {
                    links.entry(pts[i]).or_default().push(pts[j]);
                    links.entry(pts[j]).or_default().push(pts[i]);
                };
                if change.len() == 2 {
                    connect(change[0], change[1]);
                } else if change.len() == 4 {
                    for (i, &inside) in v.iter().enumerate() {
                        if inside {
                            connect((i + 3) % 4, i)
                        }
                    }
                }
            }
        }
        while let Some((&first, _)) = links.iter().next() {
            let mut p = first;
            let mut points = Vec::new();
            loop {
                let Some(q) = links.get_mut(&p).and_then(Vec::pop) else {
                    break;
                };
                if links.get(&p).is_some_and(Vec::is_empty) {
                    links.remove(&p);
                }
                if let Some(list) = links.get_mut(&q) {
                    list.retain(|&a| a != p);
                    if list.is_empty() {
                        links.remove(&q);
                    }
                }
                points.push(
                    v2(
                        x0 as f64 + 0.5 + p.0 as f64 / 2.,
                        y0 as f64 + 0.5 + p.1 as f64 / 2.,
                    ) / size,
                );
                p = q;
                if p == first {
                    break;
                }
            }
            links.remove(&first);
            if p == first && points.len() >= 3 {
                contours.push((color, points));
            }
        }
    }
    contours
}
