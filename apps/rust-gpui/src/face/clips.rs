//! The checked-in REA catalog is the lossless source of keyframes and director recipes.
use super::expressions::Expression;
use super::math::*;
use super::morphs::Morph;
use super::shapes::Body;
use serde::Deserialize;
use std::{collections::BTreeMap, sync::LazyLock};

pub(super) static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    let data: RawCatalog = serde_json::from_str(include_str!("../../../../face/demo/catalog.json"))
        .expect("checked-in face catalog");
    Catalog {
        states: data.states,
        clips: data
            .clips
            .into_iter()
            .map(|raw| (raw.name.clone(), raw.compile()))
            .collect(),
    }
});

pub(super) struct Catalog {
    pub states: BTreeMap<String, State>,
    pub clips: BTreeMap<String, Clip>,
}

#[derive(Deserialize)]
struct RawCatalog {
    states: BTreeMap<String, State>,
    clips: Vec<RawClip>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct State {
    pub clips: Vec<String>,
    pub compact_clips: Option<Vec<String>>,
    pub face: Option<Expression>,
    pub morph: Option<Morph>,
    pub next: Option<String>,
    pub hold_seconds: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Pose {
    pub rot: V3,
    pub pos: V2,
    pub squash: V2,
    pub zoom: f64,
    pub lids: f64,
    pub anchor: f64,
}
pub(super) const DEFAULT_FILL: f64 = 0.6;
pub(super) const STILL_FILL: f64 = 0.92;
pub(super) fn fill_scale(fill: f64) -> f64 {
    fill / DEFAULT_FILL
}

impl Default for Pose {
    fn default() -> Self {
        Self {
            rot: v3(0., 0., 0.),
            pos: v2(0., 0.),
            squash: v2(1., 1.),
            zoom: DEFAULT_FILL,
            lids: 0.,
            anchor: 0.,
        }
    }
}
impl Pose {
    pub fn near(self, b: Self) -> bool {
        (self.rot - b.rot)
            .0
            .into_iter()
            .all(|r| ((r + 180.).rem_euclid(360.) - 180.).abs() < 4.)
            && (self.pos - b.pos).0.into_iter().all(|v| v.abs() < 0.015)
            && (self.squash - b.squash)
                .0
                .into_iter()
                .all(|v| v.abs() < 0.02)
            && (self.zoom - b.zoom).abs() < 0.02
            && (self.lids - b.lids).abs() < 0.05
    }
    pub fn bridge(self, b: Self, t: f64) -> Self {
        let t = bezier(sat(t / 0.35), [0.4, 0., 0.2, 1.]);
        let rot = VecN(std::array::from_fn(|i| {
            let a = self.rot.0[i];
            let b = b.rot.0[i];
            mix(a, b + 360. * ((a - b) / 360. + 0.5).floor(), t)
        }));
        Self {
            rot,
            pos: self.pos.lerp(b.pos, t),
            squash: self.squash.lerp(b.squash, t),
            zoom: mix(self.zoom, b.zoom, t),
            lids: sat(mix(self.lids, b.lids, t)),
            anchor: self.anchor,
        }
    }
}

#[derive(Deserialize, Default)]
pub(super) struct Inspect {
    pub bias: Option<f64>,
    pub follow: Option<bool>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawClip {
    name: String,
    duration: f64,
    body: Option<String>,
    #[serde(default)]
    squash_anchor: f64,
    work_area: Option<WorkArea>,
    #[serde(default)]
    inspect: Inspect,
    tracks: BTreeMap<String, Vec<RawKey>>,
}
#[derive(Deserialize)]
struct WorkArea {
    start: f64,
}
#[derive(Deserialize)]
struct RawKey {
    t: f64,
    v: Value,
    ease: Option<[f64; 4]>,
    cut: Option<bool>,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Value {
    Numeric(Vec<f64>),
    Text(String),
}

#[derive(Clone, Copy)]
pub(super) struct ExpressionKey {
    pub t: f64,
    pub name: Expression,
    pub through_blink: bool,
}
struct Key<const N: usize> {
    t: f64,
    v: VecN<N>,
    ease: [f64; 4],
}
pub(super) struct Clip {
    pub duration: f64,
    pub body: Body,
    anchor: f64,
    rest_time: f64,
    pub inspect: Inspect,
    pos: Vec<Key<2>>,
    rot: Vec<Key<3>>,
    squash: Vec<Key<2>>,
    zoom: Vec<Key<1>>,
    lids: Vec<Key<1>>,
    pub expressions: Vec<ExpressionKey>,
}

impl RawClip {
    fn compile(mut self) -> Clip {
        let mut lids = track(self.tracks.remove("lids"));
        let expressions = self
            .tracks
            .remove("expression")
            .or_else(|| self.tracks.remove("expr"))
            .into_iter()
            .flatten()
            .filter_map(|key| {
                let Value::Text(name) = key.v else {
                    return None;
                };
                if name == "blink" {
                    lids.extend([(0., 0.), (0.26 * 0.45, 1.), (0.26, 0.)].map(|(t, v)| Key {
                        t: key.t + t,
                        v: VecN([v]),
                        ease: EASE,
                    }));
                    None
                } else {
                    Some(ExpressionKey {
                        t: key.t,
                        name: Expression::parse(&name),
                        through_blink: key.cut == Some(false),
                    })
                }
            })
            .collect();
        lids.sort_by(|a, b| a.t.total_cmp(&b.t));
        let mut take = |names: &[&str]| names.iter().find_map(|name| self.tracks.remove(*name));
        Clip {
            duration: self.duration,
            body: Body::named(match self.body.as_deref() {
                // The authored recipe aliases are separate from the public picker aliases.
                Some("arch") => "sphere",
                body => body.unwrap_or("sphere"),
            }),
            anchor: self.squash_anchor,
            rest_time: self.work_area.map_or(0., |w| w.start),
            inspect: self.inspect,
            pos: track(take(&["position", "pos"])),
            rot: track(take(&["rotation", "rot"])),
            squash: track(take(&["squash", "scl"])),
            zoom: track(take(&["scale", "zoom"])),
            lids,
            expressions,
        }
    }
}

fn track<const N: usize>(keys: Option<Vec<RawKey>>) -> Vec<Key<N>> {
    keys.into_iter()
        .flatten()
        .filter_map(|key| match key.v {
            Value::Numeric(v) => Some(Key {
                t: key.t,
                v: VecN(v.try_into().ok()?),
                ease: key.ease.unwrap_or(EASE),
            }),
            Value::Text(_) => None,
        })
        .collect()
}
fn eval<const N: usize>(keys: &[Key<N>], t: f64) -> Option<VecN<N>> {
    let first = keys.first()?;
    if t <= first.t {
        return Some(first.v);
    }
    let i = keys.partition_point(|k| k.t <= t);
    let a = &keys[i - 1];
    let Some(b) = keys.get(i) else {
        return Some(a.v);
    };
    Some(a.v.lerp(b.v, bezier((t - a.t) / (b.t - a.t).max(1e-9), a.ease)))
}
impl Clip {
    pub fn evaluate(&self, t: f64, previous_zoom: f64) -> Pose {
        let p = Pose::default();
        Pose {
            pos: eval(&self.pos, t).unwrap_or(p.pos),
            rot: eval(&self.rot, t).unwrap_or(p.rot),
            squash: eval(&self.squash, t).unwrap_or(p.squash),
            zoom: eval(&self.zoom, t).map_or(previous_zoom, |v| v.0[0].clamp(0.25, 2.5)),
            lids: eval(&self.lids, t).map_or(0., |v| sat(v.0[0])),
            anchor: self.anchor,
        }
    }
    pub fn rest(&self) -> (Pose, Expression) {
        let t = self.rest_time;
        (
            self.evaluate(t, DEFAULT_FILL),
            self.expression_at(t)
                .or_else(|| self.expressions.first().map(|k| k.name))
                .unwrap_or_default(),
        )
    }
    pub fn expression_at(&self, t: f64) -> Option<Expression> {
        self.expressions
            .iter()
            .rev()
            .find(|k| k.t <= t + 1e-9)
            .map(|k| k.name)
    }
}
