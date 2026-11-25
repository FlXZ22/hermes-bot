use super::{
    clips::{Clip, ExpressionKey, Pose, CATALOG},
    expressions::{rest_rotation, Expression, Eye, Eyes},
    math::*,
    morphs::{MorphFrame, MorphMotion},
    shapes::Body,
};
use std::{
    collections::{HashMap, VecDeque},
    f64::consts::PI,
};

pub(super) struct FaceMotionHost {
    requested: &'static str,
    state: &'static str,
    elapsed: f64,
    slot: usize,
    playlists: HashMap<&'static [String], VecDeque<&'static str>>,
    random: Random,
    compact: bool,
    started: bool,
    clip: Option<&'static str>,
    clip_time: f64,
    bridge: Option<Bridge>,
    pose: Pose,
    eyes: Eyes,
    held: Option<Expression>,
    pending: Option<ExpressionKey>,
    suppress_lids: bool,
    bias: f64,
    follow_inspect: bool,
    morph: MorphMotion,
    morph_rotation: Option<V3>,
    body_override: bool,
    body: Body,
    body_from: Vec<(Body, f64)>,
    body_progress: Spring,
    gaze: Gaze,
    look: Option<Look>,
    pokes: Vec<Poke>,
    accumulator: f64,
    previous: Option<Frame>,
}
struct Bridge {
    name: &'static str,
    from: Pose,
    to: Pose,
    t: f64,
}
struct Look {
    from: Pose,
    rot: V3,
    pos: V2,
    t: f64,
}
#[derive(Default)]
struct Gaze {
    target: Option<V2>,
    smoothed: V2,
    hold: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Gesture {
    Spin,
    Bounce,
    Nod,
}
struct Poke {
    kind: Gesture,
    time: f64,
    turns: f64,
}
impl Poke {
    fn duration(&self) -> f64 {
        match self.kind {
            Gesture::Spin => 0.9 + 0.4 * (self.turns - 1.),
            Gesture::Bounce => 0.62,
            Gesture::Nod => 0.5,
        }
    }
}

#[derive(Clone)]
pub(super) struct Frame {
    pub body_weights: Vec<(Body, f64)>,
    pub orientation: Quat,
    pub zoom: f64,
    pub offset: V2,
    pub squash: V2,
    pub anchor: f64,
    pub eyes: Option<[Eye; 2]>,
    pub morph: Option<MorphFrame>,
}

impl FaceMotionHost {
    pub fn new(
        body: impl Into<Option<Body>>,
        state: &'static str,
        seed: u32,
        compact: bool,
    ) -> Self {
        let body = body.into();
        let mut host = Self {
            requested: "",
            state: "idle",
            elapsed: 0.,
            slot: 0,
            playlists: HashMap::new(),
            random: Random(seed),
            compact,
            started: false,
            clip: None,
            clip_time: 0.,
            bridge: None,
            pose: Pose {
                rot: rest_rotation(),
                ..Pose::default()
            },
            eyes: Eyes::new(),
            held: None,
            pending: None,
            suppress_lids: false,
            bias: -1.,
            follow_inspect: true,
            morph: MorphMotion::new(),
            morph_rotation: None,
            body_override: body.is_some(),
            body: body.unwrap_or(Body::Sphere),
            body_from: vec![],
            body_progress: Spring::new(1.),
            gaze: Gaze::default(),
            look: None,
            pokes: vec![],
            accumulator: 0.,
            previous: None,
        };
        host.set_state(state);
        host
    }

    pub fn still(body: Body, sleeping: bool) -> Frame {
        let expression = if sleeping {
            Expression::Sleepy
        } else {
            Expression::Neutral
        };
        Frame {
            body_weights: vec![(body, 1.)],
            orientation: Quat::euler(rest_rotation() * DEG),
            zoom: 0.6,
            offset: V2::default(),
            squash: v2(1., 1.),
            anchor: 0.,
            eyes: Some(expression.eyes()),
            morph: None,
        }
    }

    pub fn set_state(&mut self, state: &'static str) {
        if self.requested == state {
            return;
        }
        self.requested = state;
        self.enter(if CATALOG.states.contains_key(state) {
            state
        } else {
            "idle"
        });
    }
    fn enter(&mut self, state: &'static str) {
        self.state = state;
        self.elapsed = 0.;
        self.slot = 0;
        let spec = &CATALOG.states[state];
        self.morph.target = spec.morph;
        if self.held != spec.face {
            self.held = spec.face;
            let expression = self
                .held
                .or_else(|| {
                    if let Some(b) = &self.bridge {
                        CATALOG.clips[b.name].expression_at(0.)
                    } else {
                        self.active_clip()
                            .and_then(|c| c.expression_at(self.clip_time))
                    }
                })
                .unwrap_or_default();
            if expression != self.eyes.name {
                self.queue(ExpressionKey {
                    t: 0.,
                    name: expression,
                    through_blink: false,
                });
            }
        }
        self.choose_clip();
    }
    fn choose_clip(&mut self) {
        let spec = &CATALOG.states[self.state];
        let clips = if self.compact {
            spec.compact_clips.as_ref().unwrap_or(&spec.clips)
        } else {
            &spec.clips
        };
        let once = spec.next.is_some() && spec.hold_seconds.is_none();
        let name = if once {
            clips.get(self.slot).map(String::as_str)
        } else {
            let q = self.playlists.entry(clips.as_slice()).or_default();
            if q.is_empty() {
                let mut bag: Vec<_> = clips.iter().map(String::as_str).collect();
                for i in (1..bag.len()).rev() {
                    let j = (self.random.next() * (i + 1) as f64).floor() as usize;
                    bag.swap(i, j);
                }
                if let Some(i) = bag.iter().position(|&c| Some(c) != self.clip) {
                    bag.swap(0, i);
                }
                q.extend(bag);
            }
            q.pop_front()
        };
        let Some(name) = name else { return };
        if Some(name) == self.clip {
            return;
        }
        if self.started {
            self.start_bridge(name);
        } else {
            let t = if once {
                0.
            } else {
                self.random.next() * CATALOG.clips[name].duration
            };
            self.load(name, t);
            self.started = true;
        }
        self.clip = Some(name);
    }
    fn active_clip(&self) -> Option<&'static Clip> {
        self.clip.map(|name| &CATALOG.clips[name])
    }
    fn load(&mut self, name: &'static str, t: f64) {
        self.bridge = None;
        self.clip = Some(name);
        let clip = &CATALOG.clips[name];
        if !self.body_override {
            self.body = clip.body;
        }
        self.clip_time = t;
        self.pose = clip.evaluate(t, self.pose.zoom);
        self.look = None;
        if let Some(b) = clip.inspect.bias.filter(|x| x.is_finite()) {
            self.bias = b.clamp(-1., 1.);
        }
        if let Some(f) = clip.inspect.follow {
            self.follow_inspect = f;
        }
        if t > 0. {
            self.pending = None;
            if let Some(expr) = self.held {
                self.eyes.seek(
                    &[ExpressionKey {
                        t: 0.,
                        name: expr,
                        through_blink: false,
                    }],
                    0.,
                );
            } else {
                self.eyes.seek(&clip.expressions, t);
            }
        }
        self.eyes.release();
        if t <= 0. {
            self.play_keys(clip, -1e-9, 0.);
        }
    }
    fn start_bridge(&mut self, name: &'static str) {
        let mut from = self.pose;
        from.lids = self.lids();
        let (to, expr) = CATALOG.clips[name].rest();
        self.bridge = Some(Bridge {
            name,
            from,
            to,
            t: 0.,
        });
        if self.held.is_none() && self.eyes.name != expr {
            self.queue(ExpressionKey {
                t: 0.,
                name: expr,
                through_blink: false,
            });
        }
    }
    fn lids(&self) -> f64 {
        if self.suppress_lids {
            0.
        } else {
            self.pose.lids
        }
    }
    fn update_lids(&mut self) {
        if !self.eyes.name.allows_lids() || self.eyes.morphing() {
            self.suppress_lids = true;
        } else if self.pose.lids == 0. {
            self.suppress_lids = false;
        }
    }
    fn queue(&mut self, key: ExpressionKey) {
        if self.eyes.amount().unwrap_or(self.lids()) > 0. {
            self.pending = Some(key);
        } else {
            self.eyes.play(key);
            self.update_lids();
        }
    }
    fn play_keys(&mut self, clip: &Clip, old: f64, new: f64) {
        if self.held.is_some() {
            return;
        }
        for &key in clip.expressions.iter().filter(|k| k.t > old && k.t <= new) {
            self.queue(key);
        }
    }
    fn play_clip(&mut self, dt: f64) -> bool {
        if let Some(b) = &mut self.bridge {
            b.t += dt;
            self.pose = b.from.bridge(b.to, b.t);
            if b.t >= 0.35 {
                let name = b.name;
                self.load(name, 0.);
            }
            return false;
        }
        let Some(clip) = self.active_clip() else {
            return false;
        };
        let new = self.clip_time + dt;
        if new < clip.duration {
            self.play_keys(clip, self.clip_time, new);
            self.clip_time = new;
            self.pose = clip.evaluate(new, self.pose.zoom);
            false
        } else {
            self.play_keys(clip, self.clip_time, clip.duration);
            let (rest, _) = clip.rest();
            if !self.pose.near(rest) {
                if let Some(name) = self.clip {
                    self.start_bridge(name);
                }
            } else {
                let t = (new - clip.duration).rem_euclid(clip.duration);
                self.play_keys(clip, -1e-9, t);
                self.clip_time = t;
                self.pose = clip.evaluate(t, self.pose.zoom);
            }
            true
        }
    }

    pub fn set_body(&mut self, body: Body) {
        self.body_override = true;
        if body != self.body {
            if body == Body::Pumpkin || self.body == Body::Pumpkin {
                self.body_from.clear();
                self.body_progress.reset(1.);
            } else {
                self.body_from = self.body_weights();
                self.body_progress.reset(0.);
                self.body_progress.target = 1.;
            }
            self.body = body;
        }
    }
    fn body_weights(&self) -> Vec<(Body, f64)> {
        if self.body_from.is_empty() {
            return vec![(self.body, 1.)];
        }
        let t = cubic_in_out(sat(self.body_progress.x));
        let mut weights: Vec<_> = self
            .body_from
            .iter()
            .map(|&(b, w)| (b, w * (1. - t)))
            .collect();
        if let Some((_, w)) = weights.iter_mut().find(|(b, _)| *b == self.body) {
            *w += t;
        } else {
            weights.push((self.body, t));
        }
        weights.retain(|(_, w)| *w > 0.0001);
        let sum = weights.iter().map(|(_, w)| w).sum::<f64>();
        for (_, w) in &mut weights {
            *w /= sum;
        }
        weights
    }
    pub fn set_gaze(&mut self, p: Option<V2>) {
        self.gaze.target =
            p.map(|p| VecN(p.0.map(|v| if v.is_finite() { v.clamp(-1., 1.) } else { 0. })));
        if self.gaze.hold == 0. {
            if let Some(p) = self.gaze.target {
                self.gaze.smoothed = p;
            }
        }
    }
    pub fn poke(&mut self, kind: &str) {
        match kind {
            "spin" => self.push_poke(Gesture::Spin),
            "bounce" => self.bounce(),
            "nod" => self.nod(),
            _ => {}
        }
    }
    pub fn bounce(&mut self) {
        self.push_poke(Gesture::Bounce);
    }
    pub fn nod(&mut self) {
        self.push_poke(Gesture::Nod);
    }
    fn push_poke(&mut self, kind: Gesture) {
        self.pokes.push(Poke {
            kind,
            time: 0.,
            turns: 1.,
        });
    }
    #[allow(dead_code)] // Programmatic gestures, also exercised by the engine tests.
    pub fn blink(&mut self) {
        self.eyes.blink();
    }
    #[allow(dead_code)]
    pub fn look(&mut self, direction: &str) {
        let (rot, pos) = match direction {
            "left" => (v3(0., -20., 0.), v2(-0.1, 0.)),
            "right" => (v3(0., 20., 0.), v2(0.1, 0.)),
            "up" => (v3(-20., 0., 0.), v2(0., 0.1)),
            "down" => (v3(20., 0., 0.), v2(0., -0.1)),
            "center" => (V3::default(), V2::default()),
            _ => return,
        };
        self.look = Some(Look {
            from: self.pose,
            rot,
            pos,
            t: 0.,
        });
    }
    pub fn advance(&mut self, frame_dt: f64) {
        if !frame_dt.is_finite() {
            return;
        }
        self.accumulator += frame_dt.clamp(0., 0.05);
        while self.accumulator + 1e-12 >= DT {
            self.accumulator = (self.accumulator - DT).max(0.);
            if self.accumulator + 1e-12 < DT {
                self.previous = Some(self.frame());
            }
            self.step();
        }
    }
    /// Keep the simulation deterministic at 120Hz while sampling a smooth pose
    /// between ticks on monitors with a faster or non-integral refresh rate.
    pub fn display_frame(&self) -> Frame {
        let mut frame = self.frame();
        if let Some(a) = &self.previous {
            let t = sat(self.accumulator / DT);
            frame.orientation = a.orientation.interpolate(frame.orientation, t);
            frame.zoom = mix(a.zoom, frame.zoom, t);
            frame.offset = a.offset.lerp(frame.offset, t);
            frame.squash = a.squash.lerp(frame.squash, t);
            if let (Some(a), Some(b)) = (a.morph, frame.morph.as_mut()) {
                b.interpolate(a, t);
            }
        }
        frame
    }
    fn step(&mut self) {
        let spec = &CATALOG.states[self.state];
        self.elapsed += DT;
        if let (Some(next), Some(hold)) = (spec.next.as_deref(), spec.hold_seconds) {
            if self.elapsed + 1e-9 >= hold {
                self.enter(next);
            }
        }
        let blend = 1. - 0.86_f64.powf(DT * 60.);
        let track = |a, b| {
            let v = mix(a, b, blend);
            if (v - b).abs() < 0.001 {
                b
            } else {
                v
            }
        };
        if let Some(p) = self.gaze.target {
            self.gaze.smoothed = VecN(std::array::from_fn(|i| {
                track(self.gaze.smoothed.0[i], p.0[i])
            }));
        }
        self.gaze.hold = track(self.gaze.hold, f64::from(self.gaze.target.is_some()));
        let looped = self.play_clip(DT * (1. - self.morph.amount()));
        if let Some(l) = &mut self.look {
            l.t += DT;
            let t = bezier(sat(l.t / 0.5), EASE);
            self.pose.rot = l.from.rot.lerp(l.rot, t);
            self.pose.pos = l.from.pos.lerp(l.pos, t);
            if l.t >= 0.5 {
                self.look = None;
            }
        }
        self.eyes.advance(DT);
        if self.eyes.amount().unwrap_or(self.lids()) == 0. {
            if let Some(key) = self.pending.take() {
                self.eyes.play(key);
            }
        }
        self.update_lids();
        if !self.body_from.is_empty() {
            self.body_progress.advance(10.);
            if (1. - self.body_progress.x).abs() < 0.001 && self.body_progress.v.abs() < 0.01 {
                self.body_from.clear();
                self.body_progress.reset(1.);
            }
        }
        self.pokes.retain_mut(|p| {
            p.time += DT;
            p.time < p.duration()
        });
        self.morph.advance();
        if self.morph.amount() > 0. || !self.morph.settled() {
            self.morph_rotation.get_or_insert_with(|| {
                VecN(self.pose.rot.0.map(|v| 360. * (v / 360. + 0.5).floor()))
            });
        } else {
            self.morph_rotation = None;
        }
        if looped {
            let spec = &CATALOG.states[self.state];
            if let Some(next) = spec.next.as_deref().filter(|_| spec.hold_seconds.is_none()) {
                self.slot += 1;
                if self.slot >= spec.clips.len() {
                    self.enter(next);
                    return;
                }
            }
            self.choose_clip();
        }
    }
    pub fn frame(&self) -> Frame {
        let mut p = self.pose;
        let [x, y] = self.gaze.smoothed.0;
        let wide = self.body != Body::Sphere;
        let k = match self.body {
            Body::Heart => 0.6,
            Body::Wedge => 0.55,
            _ => 1.,
        } * 0.55
            * self.gaze.hold;
        p.rot = p.rot
            + v3(
                -if wide { 19. } else { 14. } * y,
                if wide { 26. } else { 18. } * x,
                0.,
            ) * k;
        p.pos = p.pos
            + v2(
                if wide { 0.07 } else { 0.03 } * x,
                if wide { 0.055 } else { 0.024 } * y,
            ) * (0.55 * self.gaze.hold);
        for poke in &self.pokes {
            let t = poke.time / poke.duration();
            match poke.kind {
                Gesture::Spin => p.rot.0[1] += 180. * poke.turns * (1. - (PI * t).cos()),
                Gesture::Nod => p.rot.0[0] += 16. * spring_pulse(t, 0., 1.),
                Gesture::Bounce => {
                    let f = spring_pulse(t, 0., 0.18) + spring_pulse(t, 0.82, 1.);
                    let g = spring_pulse(t, 0.18, 0.82);
                    p.pos.0[1] += 0.3 * (PI * sat((t - 0.14) / 0.72)).sin();
                    p.squash.0[0] *= 1. + 0.08 * f - 0.04 * g;
                    p.squash.0[1] *= 1. - 0.1 * f + 0.06 * g;
                }
            }
        }
        let morph = self.morph.frame();
        let amount = self.morph.amount();
        let remain = 1. - amount;
        let rot = self.morph_rotation.unwrap_or_default().lerp(p.rot, remain);
        let quat = Quat::euler((rot + v3(0., morph.map_or(0., |m| m.spin % TAU) / DEG, 0.)) * DEG);
        Frame {
            body_weights: self.body_weights(),
            orientation: quat,
            zoom: p.zoom,
            offset: p.pos * remain,
            squash: v2(1., 1.).lerp(p.squash, remain),
            anchor: p.anchor,
            eyes: (amount < 0.5).then(|| {
                self.eyes
                    .frame(rot.0[1], self.bias, self.follow_inspect, self.lids())
            }),
            morph,
        }
    }
    #[cfg(test)]
    pub fn state(&self) -> &str {
        self.state
    }
}
