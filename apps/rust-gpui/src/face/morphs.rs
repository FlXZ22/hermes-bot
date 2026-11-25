use super::math::*;
use serde::Deserialize;
use std::{f64::consts::PI, sync::LazyLock};

pub(super) const L: f64 = 114.2705;
const CENTER: V2 = v2(L, L);
const PENCIL_PERIOD: f64 = 2500.;
const PENCIL_DRAW: f64 = 0.68;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Morph {
    Thinking,
    Orbit,
    Radar,
    Progress,
    Spawning,
    Dictating,
    Sending,
    Receiving,
    Uploading,
    Bouncing,
    Writing,
    Alerting,
    PoweringDown,
}
impl Morph {
    fn duration(self) -> Option<f64> {
        match self {
            Self::Progress => Some(2.5),
            Self::Spawning => Some(2.),
            _ => None,
        }
    }
    fn radius(self) -> f64 {
        match self {
            Self::Thinking => 22.,
            Self::Orbit | Self::Radar | Self::Progress | Self::Spawning => 19.,
            Self::Dictating => 16.,
            Self::Sending | Self::Receiving | Self::Uploading => 20.,
            Self::Bouncing => 18.,
            Self::Writing => 17.,
            Self::Alerting | Self::PoweringDown => 13.,
        }
    }
    fn zoom(self) -> f64 {
        match self {
            Self::Thinking => 1.5,
            Self::Orbit | Self::Radar => 1.14,
            Self::Progress => 1.32,
            Self::Spawning => 1.15,
            Self::Dictating => 1.42,
            Self::Sending | Self::Receiving => 1.12,
            Self::Uploading => 1.3,
            Self::Bouncing => 1.22,
            Self::Writing => 1.18,
            Self::Alerting => 1.28,
            Self::PoweringDown => 1.75,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Clock {
    state: f64,
    shot: f64,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct MorphFrame {
    pub amount: f64,
    pub spin: f64,
    kind: Option<Morph>,
    outgoing: Option<Morph>,
    swap: f64,
    clock: Clock,
    outgoing_clock: Clock,
}
pub(super) struct MorphMotion {
    pub target: Option<Morph>,
    previous: Option<Morph>,
    kind: Option<Morph>,
    outgoing: Option<Morph>,
    time: f64,
    state: f64,
    shot: f64,
    out_state: f64,
    out_shot: f64,
    rest: Option<f64>,
    active: bool,
    sign: f64,
    rng: Random,
    amount: Spring,
    swap: Spring,
    spin: Spring,
}
impl MorphMotion {
    pub fn new() -> Self {
        Self {
            target: None,
            previous: None,
            kind: None,
            outgoing: None,
            time: 0.,
            state: 0.,
            shot: 0.,
            out_state: 0.,
            out_shot: 0.,
            rest: None,
            active: false,
            sign: 1.,
            rng: Random(2_654_435_769),
            amount: Spring::new(0.),
            swap: Spring::new(1.),
            spin: Spring::new(0.),
        }
    }
    pub fn advance(&mut self) {
        self.time += DT;
        let old_shot = self.shot;
        if self.target != self.previous {
            self.previous = self.target;
            if self.target.is_some() {
                self.shot = self.time;
                self.rest = None;
            }
        }
        let mut active = self.target.is_some();
        if let Some(d) = self.target.and_then(Morph::duration) {
            match self.rest {
                None if self.time - self.shot > d => self.rest = Some(self.time),
                Some(t) if self.time - t > 1.5 => {
                    self.rest = None;
                    self.shot = self.time;
                }
                _ => {}
            }
            active = self.rest.is_none();
        }
        self.amount.target = f64::from(active);
        if self.target.is_some() && self.target != self.kind {
            if self.kind.is_some() && self.amount.x > 0.02 {
                self.outgoing = self.kind;
                self.out_state = self.state;
                self.out_shot = old_shot;
                self.swap.reset(0.);
                self.swap.target = 1.;
            } else {
                self.outgoing = None;
                self.swap.reset(1.);
            }
            self.kind = self.target;
            self.state = self.time;
        }
        if self.target.is_none() && self.amount.x < 0.004 {
            self.kind = None;
            self.outgoing = None;
        }
        if self.swap.x > 0.996 {
            self.outgoing = None;
        }
        if active != self.active {
            if active {
                self.sign = if self.rng.next() < 0.5 { 1. } else { -1. };
            }
            self.spin.target += PI * self.sign;
            self.active = active;
        }
        self.amount.advance(14.);
        self.swap.advance(11.);
        self.spin.advance(14.);
    }
    pub fn amount(&self) -> f64 {
        sat(self.amount.x)
    }
    pub fn settled(&self) -> bool {
        (self.amount.x - self.amount.target).abs() < 0.001
            && (self.spin.target - self.spin.x).abs() < 0.01
            && self.swap.x > 0.996
    }
    pub fn frame(&self) -> Option<MorphFrame> {
        if self.kind.is_none() && self.settled() {
            return None;
        }
        Some(MorphFrame {
            amount: self.amount(),
            spin: self.spin.x,
            kind: self.kind,
            outgoing: self.outgoing,
            swap: sat(self.swap.x),
            clock: Clock {
                state: self.time - self.state,
                shot: self.time - self.shot,
            },
            outgoing_clock: Clock {
                state: self.time - self.out_state,
                shot: self.time - self.out_shot,
            },
        })
    }
}

#[derive(Clone, Debug)]
pub(super) enum Primitive {
    Dot {
        center: V2,
        r: f64,
        opacity: f64,
    },
    Ring {
        center: V2,
        r: f64,
        width: f64,
        opacity: f64,
        arc: Option<(f64, f64)>,
    },
    Shaft {
        width: f64,
        base: f64,
        height: f64,
        transform: Transform2,
        opacity: f64,
    },
    Ink {
        points: Vec<V2>,
        width: f64,
        opacity: f64,
    },
}
/// Affine map for the pencil and alert bar's SVG arc contours, composed in source order.
#[derive(Clone, Copy, Debug)]
pub(super) struct Transform2 {
    pub origin: V2,
    pub angle: f64,
    pub scale: f64,
}
impl Transform2 {
    pub fn apply(self, p: V2) -> V2 {
        self.origin + (p * self.scale).rotate(self.angle)
    }
}

pub(super) struct MorphGeometry {
    pub scale: f64,
    pub translate: V2,
    pub rotation: f64,
    pub opacity: f64,
    pub round: f64,
    pub zoom: f64,
    pub pencil_round: f64,
    pub parts: Vec<Primitive>,
}
impl MorphFrame {
    pub fn interpolate(&mut self, a: Self, t: f64) {
        self.amount = mix(a.amount, self.amount, t);
        if self.kind == a.kind {
            self.clock.state = mix(a.clock.state, self.clock.state, t);
            if self.clock.shot >= a.clock.shot {
                self.clock.shot = mix(a.clock.shot, self.clock.shot, t);
            }
            if self.outgoing == a.outgoing {
                self.swap = mix(a.swap, self.swap, t);
                self.outgoing_clock.state =
                    mix(a.outgoing_clock.state, self.outgoing_clock.state, t);
                self.outgoing_clock.shot = mix(a.outgoing_clock.shot, self.outgoing_clock.shot, t);
            }
        }
    }
    pub fn geometry(self, display_size: f64) -> MorphGeometry {
        let a = self.amount;
        let swap = self.swap;
        let out = if swap < 0.999 { self.outgoing } else { None };
        let weight = |m| {
            if Some(m) == self.kind {
                a * swap
            } else if Some(m) == out {
                a * (1. - swap)
            } else {
                0.
            }
        };
        let clock = |m| {
            if Some(m) == self.kind {
                self.clock
            } else {
                self.outgoing_clock
            }
        };
        let radius = self
            .kind
            .map_or(19., |m| mix(out.unwrap_or(m).radius(), m.radius(), swap));
        let f = weight(Morph::Thinking);
        let (lift, pop, tone) =
            if f > 0. || self.kind == Some(Morph::Thinking) || out == Some(Morph::Thinking) {
                dots(clock(Morph::Thinking), 1, a)
            } else {
                (0., 1., 1.)
            };
        let mut head = 1. + (pop - 1.) * f / a.max(0.001);
        let mut pos = v2(0., -lift * f);
        let mut angle = 0.;
        let receive = weight(Morph::Receiving);
        if receive > 0.004 {
            head *= 1.
                + 0.11
                    * (PI
                        * sat(
                            ((clock(Morph::Receiving).state / 1.7).rem_euclid(1.) - 0.58) / 0.34,
                        ))
                    .sin()
                    * receive;
        }
        let send = weight(Morph::Sending);
        if send > 0.004 {
            let t = (clock(Morph::Sending).state / 1.5).rem_euclid(1.);
            let x = if t < 0.18 {
                -0.06 * (t * PI / 0.18).sin()
            } else if t < 0.42 {
                0.05 * ((t - 0.18) * PI / 0.24).sin()
            } else {
                0.
            };
            head *= 1. + x * send;
        }
        let alert = weight(Morph::Alerting);
        if alert > 0.004 {
            head *= 1. + 0.04 * alert_decay(clock(Morph::Alerting).state) * alert;
            pos = pos + v2(0., 58. * alert);
        }
        let pencil = weight(Morph::Writing);
        if pencil > 0.004 {
            let p = pencil_pose(clock(Morph::Writing).state * 1000.);
            pos = pos + (p.pos + v2(0., p.wig * 0.5)) * pencil;
            angle += p.rot * pencil;
        }
        let ball = weight(Morph::Bouncing);
        if ball > 0.004 {
            let t = clock(Morph::Bouncing).state;
            let k = 416. / 0.62_f64.powi(2);
            let t0 = (80. / k).sqrt();
            let phase = ((t - t0) / 0.62).rem_euclid(1.);
            pos = pos
                + v2(
                    0.,
                    ball * if t < t0 {
                        0.5 * k * t * t
                    } else {
                        40. - 208. * phase * (1. - phase)
                    },
                );
        }
        let standby = weight(Morph::PoweringDown);
        let dim = (0.28 + 0.2 * (clock(Morph::PoweringDown).state * 1.6).sin()) * standby;
        let zoom = self
            .kind
            .map_or(1., |m| mix(out.unwrap_or(m).zoom(), m.zoom(), swap));
        let mut parts = Vec::new();
        for m in [out, self.kind].into_iter().flatten() {
            let w = weight(m);
            if w > 0.004 {
                primitives(m, w, clock(m), radius, &mut parts);
            }
        }
        MorphGeometry {
            scale: radius / L * head,
            translate: pos * a,
            rotation: angle * a * DEG,
            opacity: (1. - (1. - tone) * f) * (1. - dim),
            round: cubic_in_out(sat(a / 0.62)),
            zoom: 1. + (zoom - 1.) * a * (1. - smooth((display_size - 44.) / 90.)),
            pencil_round: if swap >= 0.999 {
                f64::from(self.kind == Some(Morph::Writing))
            } else {
                mix(
                    f64::from(out == Some(Morph::Writing)),
                    f64::from(self.kind == Some(Morph::Writing)),
                    cubic_in_out(swap),
                )
            },
            parts,
        }
    }
}

fn dots(t: Clock, i: usize, w: f64) -> (f64, f64, f64) {
    let p = (t.state / 1.4 + 0.119).rem_euclid(1.);
    let d = (p - i as f64 / 3.).abs();
    let s = (-d.min(1. - d).powi(2) / 0.045).exp();
    (s * 9. * w, 0.84 + 0.22 * s, 0.5 + 0.5 * s)
}
fn alert_decay(t: f64) -> f64 {
    (-t.rem_euclid(2.2) * 5.5).exp()
}
struct Pencil {
    pos: V2,
    wig: f64,
    rot: f64,
}
fn pencil_pose(ms: f64) -> Pencil {
    let t = (ms / PENCIL_PERIOD).rem_euclid(1.);
    if t < PENCIL_DRAW {
        let n = t / PENCIL_DRAW;
        let i = sat(n / 0.08) * sat((1. - n) / 0.08);
        Pencil {
            pos: v2(-54. + 118. * smooth(n), 26.),
            wig: (n * 24.).sin() * 3.2 * i,
            rot: 17. + (ms * 0.0006).sin(),
        }
    } else {
        let n = cubic_in_out((t - PENCIL_DRAW) / (1. - PENCIL_DRAW));
        let s = (n * PI).sin();
        Pencil {
            pos: v2(64. - 118. * n, 26. - 20. * s),
            wig: 0.,
            rot: 17. - 2. * s + (ms * 0.0006).sin(),
        }
    }
}
fn ink_tip(ms: f64) -> V2 {
    let p = pencil_pose(ms);
    CENTER + p.pos + v2(0., p.wig + 19.)
}
static INK_SAMPLES: LazyLock<Vec<(f64, V2)>> = LazyLock::new(|| {
    let mut out: Vec<(f64, V2)> = Vec::new();
    for i in 0..(PENCIL_PERIOD * PENCIL_DRAW) as u32 {
        let t = i as f64;
        let p = ink_tip(t);
        if out.last().is_none_or(|(_, q)| (p - *q).len() > 2.4) {
            out.push((t, p));
        }
    }
    out
});
fn ink(ms: f64) -> Vec<V2> {
    let t = ms.rem_euclid(PENCIL_PERIOD);
    let end = PENCIL_PERIOD * PENCIL_DRAW;
    let n = t.min(end);
    let mut points: Vec<_> = INK_SAMPLES
        .iter()
        .take_while(|(t, _)| *t <= n)
        .map(|(_, p)| *p)
        .collect();
    if points.last().is_some()
        && INK_SAMPLES
            .get(points.len() - 1)
            .is_some_and(|(t, _)| *t < n)
    {
        points.push(ink_tip(n));
    }
    let skip =
        points.len().saturating_sub(64) + (((t - end).max(0.) / 1000. * 120.).floor() as usize);
    points.into_iter().skip(skip).collect()
}
fn primitives(m: Morph, w: f64, t: Clock, radius: f64, out: &mut Vec<Primitive>) {
    use Morph::*;
    let mut dot = |p, r, opacity| {
        if opacity > 0. {
            out.push(Primitive::Dot {
                center: CENTER + p,
                r,
                opacity,
            });
        }
    };
    let ring = |r, width, opacity, arc| Primitive::Ring {
        center: CENTER,
        r,
        width,
        opacity,
        arc,
    };
    let n = cubic_out(w);
    let s = t.state;
    match m {
        Thinking => {
            for i in 0..2 {
                let a = sat((w - i as f64 * 0.12) / (1. - i as f64 * 0.12));
                if a <= 0.004 {
                    continue;
                }
                let (lift, pop, tone) = dots(t, 2 * i, w);
                let p = cubic_out(a);
                dot(
                    v2((i as f64 * 2. - 1.) * 62. * back_out(a), -lift),
                    22. * p * pop * 1.02,
                    p * tone,
                );
            }
        }
        Orbit => {
            for i in 0..5 {
                let a = s * 1.7 + i as f64 * TAU / 5.;
                let c = a.cos();
                let r = 52. * back_out(w);
                dot(
                    v2(r * a.sin(), -r * 0.42 * c),
                    (12. * (0.5 + 0.5 * sat(c)) * n).max(0.3),
                    ((c + 0.4) / 0.6).clamp(0.18, 1.) * n,
                );
            }
        }
        Radar => {
            for i in 0..3 {
                let a = (s / 1.3 + i as f64 / 3.).rem_euclid(1.);
                out.push(ring(
                    mix(radius, 104., a),
                    3.4 * (1. - 0.55 * a),
                    n * (1. - a) * 0.9,
                    None,
                ));
            }
        }
        Progress => {
            let r = 62. * back_out(w);
            let arc = sat(sat(t.shot / 2.5) / 0.85);
            out.push(ring(r, 5., n * 0.16, None));
            if arc > 0. {
                out.push(ring(r, 5., n, Some((arc, -90. * DEG))));
            }
        }
        Spawning => {
            for i in 0..5 {
                let i = i as f64;
                let a = sat((t.shot / 2. - i * 0.09) / 0.62);
                if a >= 1. {
                    continue;
                }
                let b = cubic_out(a);
                let p = polar(i * 2.4 + a * 2.2) * 96. * (1. - b);
                dot(
                    v2(p.0[0], p.0[1] * 0.8),
                    9. * (0.5 + 0.5 * b) * n,
                    n * sat(a * 5.) * (1. - b * 0.25),
                );
            }
        }
        Dictating => {
            for i in [-2_f64, -1., 1., 2.] {
                let a = sat((w - i.abs() * 0.1) / (1. - i.abs() * 0.1));
                if a <= 0.004 {
                    continue;
                }
                let amp = (0.42
                    + 0.29 * (s * 2.1).sin() * (s * 3.4).sin()
                    + 0.29 * (s * 1.3 + 1.7).sin())
                    * (0.55 + 0.45 * (s * 12. - i.abs() * 1.05).sin());
                dot(
                    v2(i * 44. * back_out(a), -6. * sat(amp) * a),
                    (7. + 9. * amp.clamp(0.08, 1.)) * cubic_out(a),
                    a,
                );
            }
        }
        Sending => {
            let t = (s / 1.5).rem_euclid(1.);
            let i = sat((t - 0.18) / 0.55);
            let a = i * i * (0.4 + 0.6 * i);
            let dir = v2(0.74, -0.62) * 108.;
            if i > 0. && i < 1. {
                dot(dir * a, 10. * (1. - a * 0.55) * n, n * (1. - a * a));
            }
            let l = sat((t - 0.26) / 0.55);
            let u = l * l * (0.4 + 0.6 * l);
            if i > 0. && l > 0. && l < 1. {
                dot(dir * u, 5. * (1. - u * 0.6) * n, n * 0.3 * (1. - u));
            }
            let d = sat((t - 0.18) / 0.3);
            if d > 0. && d < 1. {
                out.push(ring(
                    20. + 34. * cubic_out(d),
                    2.8 * (1. - d),
                    n * (1. - d) * 0.8,
                    None,
                ));
            }
        }
        Receiving => {
            let t = s / 1.7;
            let a = t.rem_euclid(1.);
            let o = sat(a / 0.6);
            let b = cubic_out(o);
            let dir = polar(-PI * 1.25 + Random(t.floor() as u32).next() * PI * 1.5);
            if o < 1. {
                dot(
                    dir * 108. * (1. - b)
                        + v2(-dir.0[1], dir.0[0]) * 18. * (o * PI).sin() * (1. - b * 0.7),
                    3.5 + 6.5 * b,
                    n * sat(o * 3.5) * (0.3 + 0.7 * b),
                );
            }
            let f = sat((a - 0.58) / 0.32);
            if f > 0. && f < 1. {
                out.push(ring(
                    20. + 26. * cubic_out(f),
                    2.8 * (1. - f),
                    n * (1. - f) * 0.8,
                    None,
                ));
            }
        }
        Uploading => {
            for i in [0., 1.] {
                let a = sat((s - (0.2 + i * 1.3)) / 0.9);
                if a <= 0. {
                    continue;
                }
                let o = cubic_out(a);
                let phase = s * 1.1 + i * PI;
                dot(
                    v2(-120. + i * 30., 95.).lerp(
                        v2(
                            42. * phase.sin(),
                            21. * phase.cos() + (s * 3. + i).sin() * 2.,
                        ),
                        o,
                    ),
                    (7. + 3. * o) * n,
                    n * sat(a * 4.),
                );
            }
        }
        Bouncing => {} // The head itself is the ball.
        Writing => {
            let ms = s * 1000.;
            let p = pencil_pose(ms);
            let arm = polar((p.rot - 90.) * DEG) * 68.;
            out.push(Primitive::Shaft {
                width: 30.,
                base: 30.,
                height: 88.,
                transform: Transform2 {
                    origin: CENTER + (p.pos + v2(0., p.wig * 0.15) + arm) * w,
                    angle: p.rot * w * DEG,
                    scale: n,
                },
                opacity: sat(w * 1.6 - 0.3),
            });
            let points = ink(ms);
            if points.len() >= 2 {
                out.push(Primitive::Ink {
                    points,
                    width: 6.,
                    opacity: sat(w * 1.2),
                });
            }
        }
        Alerting => {
            let r = cubic_out(sat(w * 1.1));
            let a = (s * 42.).sin() * 2.2 * alert_decay(s) * DEG;
            out.push(Primitive::Shaft {
                width: 30.,
                base: 17.,
                height: 96.,
                transform: Transform2 {
                    origin: CENTER + v2(0., -26. - (1. - r) * 70. - 74.) + v2(0., 74.).rotate(a),
                    angle: a,
                    scale: sat(w * 1.2),
                },
                opacity: sat(w * 1.5 - 0.2),
            });
        }
        PoweringDown => {
            let a = 0.5 + 0.5 * (s * 1.6).sin();
            dot(V2::default(), 26. + 7. * a, n * (0.06 + 0.1 * a));
        }
    }
}
