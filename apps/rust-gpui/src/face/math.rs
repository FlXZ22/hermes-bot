use std::ops::{Add, Div, Mul, Neg, Sub};

pub(super) const DT: f64 = 1.0 / 120.0;
pub(super) const DEG: f64 = std::f64::consts::PI / 180.0;
pub(super) const TAU: f64 = std::f64::consts::TAU;
pub(super) const EASE: [f64; 4] = [0.42, 0.0, 0.58, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct VecN<const N: usize>(pub [f64; N]);
pub(super) type V2 = VecN<2>;
pub(super) type V3 = VecN<3>;

impl<const N: usize> Default for VecN<N> {
    fn default() -> Self {
        Self([0.0; N])
    }
}

pub(super) const fn v2(x: f64, y: f64) -> V2 {
    VecN([x, y])
}
pub(super) const fn v3(x: f64, y: f64, z: f64) -> V3 {
    VecN([x, y, z])
}

impl<const N: usize> VecN<N> {
    pub fn dot(self, b: Self) -> f64 {
        (0..N).map(|i| self.0[i] * b.0[i]).sum()
    }
    pub fn len(self) -> f64 {
        self.dot(self).sqrt()
    }
    pub fn unit(self) -> Self {
        let n = self.len();
        if n == 0.0 {
            self
        } else {
            self / n
        }
    }
    pub fn lerp(self, b: Self, t: f64) -> Self {
        self + (b - self) * t
    }
    pub fn finite(self) -> bool {
        self.0.iter().all(|v| v.is_finite())
    }
}

impl V2 {
    pub fn cross(self, b: Self) -> f64 {
        self.0[0] * b.0[1] - self.0[1] * b.0[0]
    }
    pub fn angle(self) -> f64 {
        self.0[1].atan2(self.0[0])
    }
    pub fn rotate(self, angle: f64) -> Self {
        let (s, c) = angle.sin_cos();
        v2(c * self.0[0] - s * self.0[1], s * self.0[0] + c * self.0[1])
    }
}
impl V3 {
    pub fn cross(self, b: Self) -> Self {
        let [x, y, z] = self.0;
        let [u, v, w] = b.0;
        v3(y * w - z * v, z * u - x * w, x * v - y * u)
    }
    pub fn xy(self) -> V2 {
        v2(self.0[0], self.0[1])
    }
}
impl<const N: usize> Add for VecN<N> {
    type Output = Self;
    fn add(self, b: Self) -> Self {
        Self(std::array::from_fn(|i| self.0[i] + b.0[i]))
    }
}
impl<const N: usize> Sub for VecN<N> {
    type Output = Self;
    fn sub(self, b: Self) -> Self {
        self + -b
    }
}
impl<const N: usize> Mul<f64> for VecN<N> {
    type Output = Self;
    fn mul(self, s: f64) -> Self {
        Self(self.0.map(|x| x * s))
    }
}
impl<const N: usize> Div<f64> for VecN<N> {
    type Output = Self;
    fn div(self, s: f64) -> Self {
        self * (1.0 / s)
    }
}
impl<const N: usize> Neg for VecN<N> {
    type Output = Self;
    fn neg(self) -> Self {
        self * -1.0
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Quat(pub VecN<4>);
impl Quat {
    pub fn interpolate(self, b: Self, t: f64) -> Self {
        let dot = self.0.dot(b.0);
        let b = if dot < 0. { -b.0 } else { b.0 };
        let dot = dot.abs().min(1.);
        if dot > 0.9995 {
            return Self(self.0.lerp(b, t).unit());
        }
        let a = dot.acos();
        Self((self.0 * ((1. - t) * a).sin() + b * (t * a).sin()) / a.sin())
    }
    pub fn axis(axis: V3, angle: f64) -> Self {
        let (s, c) = (angle / 2.0).sin_cos();
        let [x, y, z] = (axis * s).0;
        Self(VecN([c, x, y, z]))
    }
    pub fn mul(self, b: Self) -> Self {
        let [w, x, y, z] = self.0 .0;
        let [a, i, j, k] = b.0 .0;
        Self(VecN([
            w * a - x * i - y * j - z * k,
            w * i + x * a + y * k - z * j,
            w * j - x * k + y * a + z * i,
            w * k + x * j - y * i + z * a,
        ]))
    }
    pub fn between(a: V3, b: V3) -> Self {
        let [x, y, z] = a.cross(b).0;
        Self(VecN([1.0 + a.dot(b), x, y, z]).unit())
    }
    pub fn inverse(self) -> Self {
        let [w, x, y, z] = self.0 .0;
        Self(VecN([w, -x, -y, -z]))
    }
    pub fn euler(angles: V3) -> Self {
        let [pitch, yaw, roll] = angles.0;
        Self::axis(v3(0., 0., 1.), roll)
            .mul(Self::axis(v3(0., 1., 0.), yaw).mul(Self::axis(v3(1., 0., 0.), pitch)))
    }
    pub fn matrix(self) -> [V3; 3] {
        let [w, x, y, z] = self.0 .0;
        [
            v3(
                1. - 2. * (y * y + z * z),
                2. * (x * y - w * z),
                2. * (x * z + w * y),
            ),
            v3(
                2. * (x * y + w * z),
                1. - 2. * (x * x + z * z),
                2. * (y * z - w * x),
            ),
            v3(
                2. * (x * z - w * y),
                2. * (y * z + w * x),
                1. - 2. * (x * x + y * y),
            ),
        ]
    }
}
pub(super) fn euler_from_matrix(m: [V3; 3]) -> V3 {
    let [a, b, c] = m.map(|v| v.0);
    let s = (-c[0]).clamp(-1., 1.);
    if s.abs() > 0.999999 {
        v3((-b[2]).atan2(b[1]), s.asin(), 0.)
    } else {
        v3(c[1].atan2(c[2]), s.asin(), b[0].atan2(a[0]))
    }
}
pub(super) fn rotate(m: [V3; 3], p: V3) -> V3 {
    VecN(m.map(|row| row.dot(p)))
}
pub(super) fn polar(a: f64) -> V2 {
    let (s, c) = a.sin_cos();
    v2(c, s)
}
pub(super) fn mix(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
pub(super) fn sat(t: f64) -> f64 {
    t.clamp(0., 1.)
}
pub(super) fn smooth(t: f64) -> f64 {
    let t = sat(t);
    t * t * (3. - 2. * t)
}
pub(super) fn cubic_in_out(t: f64) -> f64 {
    if t < 0.5 {
        4. * t * t * t
    } else {
        1. - (-2. * t + 2.).powi(3) / 2.
    }
}
pub(super) fn cubic_out(t: f64) -> f64 {
    1. - (1. - t).powi(3)
}
pub(super) fn back_out(t: f64) -> f64 {
    1. + 2.70158 * (t - 1.).powi(3) + 1.70158 * (t - 1.).powi(2)
}
pub(super) fn bezier(t: f64, [x1, y1, x2, y2]: [f64; 4]) -> f64 {
    let sample = |u: f64, a, b| {
        let r = 1. - u;
        3. * r * r * u * a + 3. * r * u * u * b + u * u * u
    };
    let mut u = t;
    for _ in 0..8 {
        let d = sample(u, x1, x2) - t;
        let r = 1. - u;
        let slope = 3. * r * r * x1 + 6. * r * u * (x2 - x1) + 3. * u * u * (1. - x2);
        if d.abs() < 1e-6 || slope.abs() < 1e-6 {
            break;
        }
        u = sat(u - d / slope);
    }
    sample(u, y1, y2)
}
pub(super) fn cubic(p: [V2; 4], t: f64) -> V2 {
    let s = 1. - t;
    p[0] * (s * s * s) + p[1] * (3. * s * s * t) + p[2] * (3. * s * t * t) + p[3] * (t * t * t)
}
pub(super) fn cubic_tangent(p: [V2; 4], t: f64) -> V2 {
    let s = 1. - t;
    (p[1] - p[0]) * (3. * s * s) + (p[2] - p[1]) * (6. * s * t) + (p[3] - p[2]) * (3. * t * t)
}
pub(super) fn spring_pulse(t: f64, a: f64, b: f64) -> f64 {
    if t <= a || t >= b {
        0.
    } else {
        ((t - a) / (b - a) * std::f64::consts::PI).sin().powi(2)
    }
}
pub(super) struct Spring {
    pub x: f64,
    pub v: f64,
    pub target: f64,
}
impl Spring {
    pub fn new(x: f64) -> Self {
        Self {
            x,
            v: 0.,
            target: x,
        }
    }
    pub fn reset(&mut self, x: f64) {
        *self = Self::new(x);
    }
    pub fn advance(&mut self, hz: f64) {
        self.v += (-2. * hz * self.v - hz * hz * (self.x - self.target)) * DT;
        self.x += self.v * DT;
        if !self.x.is_finite() || !self.v.is_finite() {
            self.reset(self.target);
        }
    }
}
pub(super) struct Random(pub u32);
impl Random {
    pub fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(1_831_565_813);
        let t = self.0;
        let mut e = (t ^ (t >> 15)).wrapping_mul(t | 1);
        e ^= e.wrapping_add((e ^ (e >> 7)).wrapping_mul(e | 61));
        (e ^ (e >> 14)) as f64 / 4_294_967_296.
    }
}
