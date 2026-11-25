//! Boundary extraction for projected faces and eye cutouts. Like the reference
//! union, this works on a fixed-point edge arrangement, not a coverage raster.
use super::{math::*, shapes::edges};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
struct Point(i64, i64);
type Winding = [i32; 2];

#[derive(Clone, Copy)]
struct Segment {
    a: Point,
    b: Point,
    winding: Winding,
}

pub(super) fn area(p: &[V2]) -> f64 {
    edges(p).map(|(a, b)| a.cross(b)).sum::<f64>() * 0.5
}

pub(super) fn union<'a>(polygons: impl Iterator<Item = &'a [V2]>, grid: f64) -> Vec<Vec<V2>> {
    boundary(polygons.map(|p| (p, 0)), grid, |w| w[0] != 0)
}

pub(super) fn intersection(a: &[V2], b: &[V2]) -> Vec<Vec<V2>> {
    boundary([(a, 0), (b, 1)].into_iter(), 1024., |w| {
        w[0] != 0 && w[1] != 0
    })
}

fn boundary<'a>(
    polygons: impl Iterator<Item = (&'a [V2], usize)>,
    grid: f64,
    inside: impl Fn(Winding) -> bool,
) -> Vec<Vec<V2>> {
    let quantize = |p: V2| {
        Point(
            (p.0[0] * grid + 0.5).floor() as i64,
            (p.0[1] * grid + 0.5).floor() as i64,
        )
    };
    let mut edges: HashMap<(Point, Point), Winding> = HashMap::new();
    let mut add = |p: &[Point], channel| {
        let signed: i64 = p
            .iter()
            .zip(p.iter().cycle().skip(1))
            .map(|(a, b)| a.0 * b.1 - a.1 * b.0)
            .sum();
        if signed == 0 {
            return;
        }
        let sign = signed.signum() as i32;
        for (&a, &b) in p.iter().zip(p.iter().cycle().skip(1)) {
            if a < b {
                edges.entry((a, b)).or_default()[channel] += sign;
            } else if b < a {
                edges.entry((b, a)).or_default()[channel] -= sign;
            }
        }
    };
    for (p, channel) in polygons {
        let points: Vec<_> = p.iter().copied().map(quantize).collect();
        if let [a, b, c, d] = points.as_slice() {
            if let Some(p) = crossing(*a, *b, *c, *d) {
                add(&[*a, p, *d], channel);
                add(&[p, *b, *c], channel);
                continue;
            }
            if let Some(p) = crossing(*b, *c, *d, *a) {
                add(&[*b, p, *a], channel);
                add(&[p, *c, *d], channel);
                continue;
            }
        }
        add(&points, channel);
    }
    let mut segments: Vec<_> = edges
        .into_iter()
        .filter_map(|((a, b), winding)| (winding != [0; 2]).then_some(Segment { a, b, winding }))
        .collect();
    segments.sort_unstable_by_key(|s| (s.a, s.b));

    let mut vertices: Vec<_> = segments.iter().flat_map(|s| [s.a, s.b]).collect();
    for (i, s) in segments.iter().enumerate() {
        for p in &segments[i + 1..] {
            if p.a.0 > s.b.0 {
                break;
            }
            if s.a.1.max(s.b.1) < p.a.1.min(p.b.1) || s.a.1.min(s.b.1) > p.a.1.max(p.b.1) {
                continue;
            }
            if let Some(p) = crossing(s.a, s.b, p.a, p.b) {
                vertices.push(p);
            }
        }
    }
    vertices.sort_unstable();
    vertices.dedup();

    let mut split: BTreeMap<(Point, Point), Winding> = BTreeMap::new();
    let mut stations = Vec::<(i64, Point)>::new();
    for s in &segments {
        let dx = s.b.0 - s.a.0;
        let dy = s.b.1 - s.a.1;
        let fuzz = dx.abs() + dy.abs();
        let i = vertices.partition_point(|p| p.0 < s.a.0);
        stations.clear();
        for &p in vertices[i..].iter().take_while(|p| p.0 <= s.b.0) {
            if p.1 >= s.a.1.min(s.b.1)
                && p.1 <= s.a.1.max(s.b.1)
                && cross(s.a, s.b, p).abs() * 2 <= fuzz
            {
                stations.push(((p.0 - s.a.0) * dx + (p.1 - s.a.1) * dy, p));
            }
        }
        stations.sort_unstable();
        for ab in stations.windows(2) {
            let [a, b] = [ab[0].1, ab[1].1];
            let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            let w = split.entry(key).or_default();
            for (c, d) in w.iter_mut().zip(s.winding) {
                *c += d * sign;
            }
        }
    }
    let split: Vec<_> = split.into_iter().filter(|(_, w)| *w != [0; 2]).collect();
    let mut graph: BTreeMap<Point, Vec<Point>> = BTreeMap::new();
    for (i, &((a, b), w)) in split.iter().enumerate() {
        let p = Point(a.0 + b.0, a.1 + b.1);
        let mut left = [0; 2];
        for (j, &((u, v), w)) in split.iter().enumerate() {
            if i == j {
                continue;
            }
            let u = Point(2 * u.0, 2 * u.1);
            let v = Point(2 * v.0, 2 * v.1);
            let sign = if u.1 <= p.1 && v.1 > p.1 && cross(u, v, p) > 0 {
                1
            } else if u.1 > p.1 && v.1 <= p.1 && cross(u, v, p) < 0 {
                -1
            } else {
                continue;
            };
            for (c, d) in left.iter_mut().zip(w) {
                *c += sign * d;
            }
        }
        if a.1 < b.1 || (a.1 == b.1 && a.0 > b.0) {
            left = [left[0] + w[0], left[1] + w[1]];
        }
        let right = [left[0] - w[0], left[1] - w[1]];
        if inside(left) != inside(right) {
            let (a, b) = if inside(left) { (a, b) } else { (b, a) };
            graph.entry(a).or_default().push(b);
        }
    }

    let mut outlines = Vec::new();
    while let Some((&start, _)) = graph.first_key_value() {
        let mut p = start;
        let mut points = Vec::new();
        while let Some(next) = graph.get_mut(&p).and_then(Vec::pop) {
            if graph.get(&p).is_some_and(Vec::is_empty) {
                graph.remove(&p);
            }
            points.push(v2(p.0 as f64, p.1 as f64) / grid);
            p = next;
            if p == start {
                break;
            }
        }
        if p == start && points.len() >= 3 {
            outlines.push(points);
        }
    }
    outlines
}

fn cross(a: Point, b: Point, p: Point) -> i64 {
    (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)
}

fn crossing(a: Point, b: Point, c: Point, d: Point) -> Option<Point> {
    let (u, v) = (cross(a, b, c), cross(a, b, d));
    let (x, y) = (cross(c, d, a), cross(c, d, b));
    if u.signum() * v.signum() >= 0 || x.signum() * y.signum() >= 0 {
        return None;
    }
    let t = x as f64 / (x - y) as f64;
    Some(Point(
        (mix(a.0 as f64, b.0 as f64, t) + 0.5).floor() as i64,
        (mix(a.1 as f64, b.1 as f64, t) + 0.5).floor() as i64,
    ))
}
