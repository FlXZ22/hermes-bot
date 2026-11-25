//! Compose the renderer's body, projected eye holes and independent morph marks in 240×240 space.
use super::{
    clips::{fill_scale, DEFAULT_FILL, STILL_FILL},
    contours,
    engine::Frame,
    expressions::sphere_eyes,
    math::*,
    morphs::{Primitive, L},
    pumpkin::clip_convex,
    shapes::*,
};
use std::{f64::consts::PI, sync::LazyLock};

pub(super) const LOGICAL_SIZE: f64 = 240.;
const CENTER: V2 = v2(LOGICAL_SIZE / 2., LOGICAL_SIZE / 2.);
pub(super) struct RenderTree {
    pub body: Vec<V2>,
    pub eyes: Vec<Vec<V2>>,
    pub painted: Vec<(u32, Vec<V2>)>,
    pub parts: Vec<Primitive>,
    pub head_opacity: f64,
    pub paint_opacity: f64,
}
struct ShapeFrame {
    center: V2,
    body: Vec<V2>,
    eyes: Vec<Vec<V2>>,
    painted: Vec<(u32, Vec<V2>)>,
}

pub(super) fn render_tree(frame: &Frame, display_size: f64) -> RenderTree {
    let extent = display_size * fill_scale(STILL_FILL);
    let radius = frame.zoom * LOGICAL_SIZE / 2.;
    let mut layers: Vec<_> = frame
        .body_weights
        .iter()
        .map(|&(b, w)| (project(frame, b, extent), w))
        .collect();
    layers.sort_by(|a, b| b.1.total_cmp(&a.1));
    let center = layers.first().map_or(CENTER, |l| l.0.center);
    let marks = DEFAULT_FILL * LOGICAL_SIZE / 2. / L;
    let geom = frame.morph.map(|m| m.geometry(display_size));
    let round = geom.as_ref().map_or(0., |g| g.round);
    let mut body = if layers.len() == 1 && round == 0. {
        std::mem::take(&mut layers[0].0.body)
    } else {
        blend_outlines(
            &layers,
            center,
            radius,
            round,
            geom.as_ref().map_or(0., |g| g.pencil_round),
        )
    };
    let mut eyes = if layers.len() == 1 {
        std::mem::take(&mut layers[0].0.eyes)
    } else {
        blend_eyes(&layers, radius)
    };
    if eyes
        .iter()
        .flatten()
        .any(|p| !contains(&body, *p) && !near_boundary(&body, *p))
    {
        // Even-odd fill alone would draw any portion of an eye outside the head.
        // Sphere squash has a convex silhouette; use the edge arrangement for
        // concave bodies, whose eye holes can meet the silhouette at two points.
        let convex = hull(body.clone());
        if convex.len() == body.len() {
            eyes = eyes
                .into_iter()
                .map(|eye| clip_convex(eye, &convex))
                .filter(|p| p.len() >= 3)
                .collect();
        } else {
            eyes = eyes
                .into_iter()
                .flat_map(|eye| {
                    if eye.iter().all(|&p| contains(&body, p)) {
                        vec![eye]
                    } else {
                        contours::intersection(&body, &eye)
                    }
                })
                .collect();
        }
    }
    eyes.retain(|e| contours::area(e).abs() > 1e-6);
    if eyes.len() >= 2 {
        let boxes: Vec<_> = eyes.iter().map(|p| bounds(p)).collect();
        let overlap = boxes.iter().enumerate().any(|(i, (a, b))| {
            boxes[i + 1..]
                .iter()
                .any(|(c, d)| (0..2).all(|j| a.0[j] < d.0[j] && c.0[j] < b.0[j]))
        });
        if overlap {
            let merged = contours::union(eyes.iter().map(Vec::as_slice), 1024.);
            if !merged.is_empty() {
                eyes = merged;
            }
        }
    }
    let mut painted = if layers.len() == 1 {
        std::mem::take(&mut layers[0].0.painted)
    } else {
        vec![]
    };
    let Some(g) = geom else {
        return RenderTree {
            body,
            eyes,
            painted,
            parts: vec![],
            head_opacity: 1.,
            paint_opacity: 1.,
        };
    };
    let amount = frame.morph.map_or(0., |m| m.amount);
    let head_scale = 1. - amount + g.scale * amount * DEFAULT_FILL / frame.zoom;
    let zoom_center = CENTER + v2(114.5 - L, 114.5 - L) * marks;
    let zoom = |p: V2| zoom_center + (p - zoom_center) * g.zoom;
    let head =
        |p: V2| zoom(center + g.translate * marks + ((p - center) * head_scale).rotate(g.rotation));
    for p in body
        .iter_mut()
        .chain(eyes.iter_mut().flatten())
        .chain(painted.iter_mut().flat_map(|(_, p)| p))
    {
        *p = head(*p);
    }
    let mark = |p: V2| zoom(CENTER + (p - v2(L, L)) * marks);
    let parts = g
        .parts
        .into_iter()
        .map(|p| match p {
            Primitive::Dot { center, r, opacity } => Primitive::Dot {
                center: mark(center),
                r: r * marks * g.zoom,
                opacity,
            },
            Primitive::Ring {
                center,
                r,
                width,
                opacity,
                arc,
            } => Primitive::Ring {
                center: mark(center),
                r: r * marks * g.zoom,
                width: width * marks * g.zoom,
                opacity,
                arc,
            },
            Primitive::Ink {
                points,
                width,
                opacity,
            } => Primitive::Ink {
                points: points.into_iter().map(mark).collect(),
                width: width * marks * g.zoom,
                opacity,
            },
            Primitive::Shaft {
                width,
                base,
                height,
                mut transform,
                opacity,
            } => {
                transform.origin = mark(transform.origin);
                transform.scale *= marks * g.zoom;
                Primitive::Shaft {
                    width,
                    base,
                    height,
                    transform,
                    opacity,
                }
            }
        })
        .collect();
    RenderTree {
        body,
        eyes,
        painted,
        parts,
        head_opacity: g.opacity,
        paint_opacity: 1. - round,
    }
}
fn near_boundary(body: &[V2], p: V2) -> bool {
    edges(body).any(|(a, b)| {
        let d = b - a;
        (p - a - d * sat((p - a).dot(d) / d.dot(d).max(1e-15))).len() < 0.012
    })
}

fn project(frame: &Frame, body: Body, extent: f64) -> ShapeFrame {
    let radius = frame.zoom * LOGICAL_SIZE / 2.;
    let [sx, sy] = frame.squash.0;
    let center = CENTER
        + v2(
            frame.offset.0[0],
            -frame.offset.0[1] - frame.anchor * (1. - sy),
        ) * radius;
    if body == Body::Sphere {
        let outline = CIRCLE
            .iter()
            .map(|p| center + v2(p.0[0] * sx.abs(), p.0[1] * sy.abs()) * radius)
            .collect();
        let eyes = frame.eyes.as_ref().map_or_else(Vec::new, |e| {
            sphere_eyes(e, frame.orientation.matrix(), frame.zoom * extent * 0.5)
                .into_iter()
                .map(|p| {
                    p.into_iter()
                        .map(|p| center + v2(p.0[0], -p.0[1]) * radius)
                        .collect()
                })
                .collect()
        });
        return ShapeFrame {
            center,
            body: outline,
            eyes,
            painted: vec![],
        };
    }
    let resolution = (extent * frame.zoom / (184. / 256.)).round().max(8.);
    let solid = body.project(
        frame.orientation,
        resolution.max(144.),
        frame.zoom,
        extent,
        frame.eyes.as_ref(),
    );
    let k = (LOGICAL_SIZE / extent) * resolution * body.scale() * 0.36 / 1.18;
    let map = |p: V2| center + v2(p.0[0] * sx, -p.0[1] * sy) * k;
    ShapeFrame {
        center,
        body: solid.outline.into_iter().map(map).collect(),
        eyes: solid
            .eyes
            .into_iter()
            .map(|p| p.into_iter().map(map).collect())
            .collect(),
        painted: solid
            .painted
            .into_iter()
            .map(|(c, p)| (c, p.into_iter().map(map).collect()))
            .collect(),
    }
}
static CIRCLE: LazyLock<Vec<V2>> = LazyLock::new(|| radial_circle(192));
fn blend_outlines(
    layers: &[(ShapeFrame, f64)],
    center: V2,
    radius: f64,
    round: f64,
    pencil: f64,
) -> Vec<V2> {
    let mut angles: Vec<_> = (0..192).map(|i| TAU * i as f64 / 192.).collect();
    let rings: Vec<_> = layers
        .iter()
        .map(|(l, weight)| {
            let p: Vec<_> = l.body.iter().map(|p| *p - l.center).collect();
            angles.extend(p.iter().map(|p| p.angle().rem_euclid(TAU)));
            (Sectors::new(&p), *weight)
        })
        .collect();
    angles.sort_unstable_by(f64::total_cmp);
    angles.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let pencil_ring: Vec<_> = pencil_ring().iter().map(|p| v2(-p.0[0], p.0[1])).collect();
    let pencil_ring = Sectors::new(&pencil_ring);
    angles
        .into_iter()
        .map(|a| {
            let d = polar(a);
            let p = layers
                .iter()
                .zip(&rings)
                .fold(V2::default(), |acc, ((l, _), (r, w))| {
                    acc + (l.center + d * r.at(a, d)) * *w
                });
            p.lerp(
                center + d * radius * mix(1., pencil_ring.at(a, d), pencil),
                round,
            )
        })
        .collect()
}
const SECTORS: usize = 720;
struct Sectors(Vec<Vec<(V2, V2)>>);
impl Sectors {
    fn new(points: &[V2]) -> Self {
        let mut bins = vec![vec![]; SECTORS];
        let k = SECTORS as f64 / TAU;
        for (a, b) in edges(points) {
            let start = a.angle().rem_euclid(TAU);
            let span = (b.angle() - start + PI).rem_euclid(TAU) - PI;
            let lo = (start.min(start + span) * k).floor() as i32 - 1;
            let hi = (start.max(start + span) * k).floor() as i32 + 1;
            for i in lo..=hi {
                bins[i.rem_euclid(SECTORS as i32) as usize].push((a, b));
            }
        }
        Self(bins)
    }
    fn at(&self, a: f64, d: V2) -> f64 {
        self.0[(a / TAU * SECTORS as f64) as usize % SECTORS]
            .iter()
            .filter_map(|&(a, b)| intersection(a, b, d))
            .fold(0., f64::max)
    }
}

fn blend_eyes(layers: &[(ShapeFrame, f64)], radius: f64) -> Vec<Vec<V2>> {
    let mut anchors: Vec<(V2, Vec<Option<&[V2]>>)> = vec![];
    for (i, (l, _)) in layers.iter().enumerate() {
        for eye in &l.eyes {
            let p = mean(eye.iter().copied(), eye.len());
            let closest = anchors
                .iter_mut()
                .filter(|(a, l)| l[i].is_none() && (*a - p).len() < radius * 0.6)
                .min_by(|(a, _), (b, _)| (*a - p).len().total_cmp(&(*b - p).len()));
            if let Some((_, loops)) = closest {
                loops[i] = Some(eye);
            } else {
                let mut loops = vec![None; layers.len()];
                loops[i] = Some(eye.as_slice());
                anchors.push((p, loops));
            }
        }
    }
    anchors
        .into_iter()
        .map(|(anchor, loops)| {
            let maps: Vec<_> = loops.into_iter().map(|p| p.map(Perimeter::new)).collect();
            let mut samples: Vec<_> = (0..96).map(|i| i as f64 / 96.).collect();
            samples.extend(
                maps.iter()
                    .flatten()
                    .flat_map(|p| p.dist.iter().copied())
                    .filter(|&x| x < 1.),
            );
            samples.sort_unstable_by(f64::total_cmp);
            samples.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
            samples
                .into_iter()
                .map(|t| {
                    maps.iter()
                        .zip(layers)
                        .fold(V2::default(), |sum, (m, (_, w))| {
                            sum + m.as_ref().map_or(anchor, |p| p.at(t)) * *w
                        })
                })
                .collect()
        })
        .collect()
}
struct Perimeter {
    points: Vec<V2>,
    dist: Vec<f64>,
}
impl Perimeter {
    fn new(p: &[V2]) -> Self {
        let mut p = p.to_vec();
        if edges(&p).map(|(a, b)| a.cross(b)).sum::<f64>() < 0. {
            p.reverse();
        }
        let center = mean(p.iter().copied(), p.len());
        if let Some((x, i)) = edges(&p)
            .enumerate()
            .filter_map(|(i, (a, b))| {
                intersection(a - center, b - center, v2(1., 0.)).map(|x| (x, i))
            })
            .max_by(|a, b| a.0.total_cmp(&b.0))
        {
            let n = p.len();
            p.rotate_left((i + 1) % n);
            p.insert(0, center + v2(x, 0.));
        }
        let mut dist = vec![0.];
        let mut total = 0.;
        for (a, b) in edges(&p) {
            total += (a - b).len();
            dist.push(total);
        }
        if total > 0. {
            for d in &mut dist {
                *d /= total;
            }
        }
        Self { points: p, dist }
    }
    fn at(&self, t: f64) -> V2 {
        let i = self
            .dist
            .partition_point(|&d| d < t)
            .saturating_sub(1)
            .min(self.points.len() - 1);
        let d = self.dist[i + 1] - self.dist[i];
        let u = if d == 0. { 0. } else { (t - self.dist[i]) / d };
        self.points[i].lerp(self.points[(i + 1) % self.points.len()], u)
    }
}
