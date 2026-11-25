//! Trace an implicit contour on a bounded grid. Edge crossings are solved against
//! the field itself; the grid is used only to discover the contour's topology.
use super::math::*;

pub(super) fn trace(
    field: impl Fn(V2) -> f64,
    lo: V2,
    hi: V2,
    step: f64,
    tolerance: f64,
) -> Vec<Vec<V2>> {
    let origin = lo - v2(step, step) * 2.;
    let [x, y] = ((hi - lo) / step).0;
    let (w, h) = (x.ceil() as usize + 5, y.ceil() as usize + 5);
    let pos = |i: usize| origin + v2((i % w) as f64, (i / w) as f64) * step;
    let values: Vec<_> = (0..w * h).map(|i| field(pos(i))).collect();
    let none = usize::MAX;
    let mut links = vec![none; 2 * w * h];
    for y in 0..h - 1 {
        for x in 0..w - 1 {
            let j = y * w + x;
            let points = [j, j + 1, j + 1 + w, j + w];
            let signs = points.map(|i| values[i] < 0.);
            if signs.iter().all(|&s| s == signs[0]) {
                continue;
            }
            let edges = [2 * j, 2 * (j + 1) + 1, 2 * (j + w), 2 * j + 1];
            let mut crossed = [0; 4];
            let mut n = 0;
            for i in 0..4 {
                if signs[i] != signs[(i + 1) % 4] {
                    crossed[n] = i;
                    n += 1;
                }
            }
            if n == 2 {
                let (a, b) = (crossed[0], crossed[1]);
                let (a, b) = if signs[a] { (a, b) } else { (b, a) };
                links[edges[a]] = edges[b];
            } else if n == 4 {
                let center = field(pos(j) + v2(step, step) / 2.) < 0.;
                for i in 0..4 {
                    if signs[i] {
                        links[edges[i]] = edges[(i + if center { 1 } else { 3 }) % 4];
                    }
                }
            }
        }
    }
    let at = |id: usize| {
        let a = id / 2;
        let b = a + if id % 2 == 0 { 1 } else { w };
        let mut p = pos(a);
        let mut q = pos(b);
        let (mut x, mut y) = (values[a], values[b]);
        let mut last = 0;
        for _ in 0..60 {
            let m = p.lerp(q, x / (x - y));
            let z = field(m);
            if z.abs() <= tolerance || (p - q).len() < 1e-15 {
                return m;
            }
            if (z < 0.) == (y < 0.) {
                q = m;
                y = z;
                if last == -1 {
                    x *= 0.5;
                }
                last = -1;
            } else {
                p = m;
                x = z;
                if last == 1 {
                    y *= 0.5;
                }
                last = 1;
            }
        }
        (p + q) * 0.5
    };
    let mut contours = Vec::new();
    for start in 0..links.len() {
        if links[start] == none {
            continue;
        }
        let mut p = start;
        let mut path = Vec::new();
        while links[p] != none {
            path.push(at(p));
            p = std::mem::replace(&mut links[p], none);
            if p == start {
                break;
            }
        }
        if p == start && path.len() >= 3 {
            contours.push(path);
        }
    }
    contours
}
