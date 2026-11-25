use super::{
    clips::{fill_scale, CATALOG, STILL_FILL},
    engine::FaceMotionHost,
    expressions::{rest_rotation, Expression, Eyes},
    math::*,
    morphs::Primitive,
    render::{render_tree, RenderTree},
    shapes::{self, Body},
};
use crate::theme::bot_face::ALL_GROK_BOT_SHAPES;

fn finite_contour(points: &[V2]) {
    assert!(points.len() >= 3);
    assert!(points.iter().all(|p| p.finite()));
    let area = shapes::edges(points)
        .map(|(a, b)| a.cross(b))
        .sum::<f64>()
        .abs()
        / 2.;
    assert!(area > 1e-6, "degenerate contour: {area}");
}
fn verify(tree: &RenderTree) {
    finite_contour(&tree.body);
    for p in &tree.eyes {
        finite_contour(p);
    }
    for (_, p) in &tree.painted {
        finite_contour(p);
    }
    assert!(tree.head_opacity.is_finite() && tree.paint_opacity.is_finite());
    for p in &tree.parts {
        let positive = |x: f64| assert!(x.is_finite() && x > 0.);
        let opacity = match p {
            Primitive::Dot { center, r, opacity } => {
                assert!(center.finite());
                positive(*r);
                *opacity
            }
            Primitive::Ring {
                center,
                r,
                width,
                opacity,
                arc,
            } => {
                assert!(center.finite());
                positive(*r);
                positive(*width);
                if let Some((f, a)) = arc {
                    assert!((0. ..=1.).contains(f) && a.is_finite());
                }
                *opacity
            }
            Primitive::Shaft {
                width,
                base,
                height,
                transform,
                opacity,
            } => {
                positive(*width);
                positive(*base);
                positive(*height);
                assert!(transform.origin.finite() && transform.angle.is_finite());
                positive(transform.scale);
                *opacity
            }
            Primitive::Ink {
                points,
                width,
                opacity,
            } => {
                positive(*width);
                assert!(points.len() >= 2 && points.iter().all(|p| p.finite()));
                *opacity
            }
        };
        assert!((0. ..=1.).contains(&opacity));
    }
}

#[test]
fn all_bodies_have_finite_closed_cutout_outlines() {
    assert_eq!(ALL_GROK_BOT_SHAPES.len(), 26);
    let mut seen = vec![];
    for (name, _) in ALL_GROK_BOT_SHAPES {
        let body = Body::named(name);
        assert!(!seen.contains(&body), "duplicate {name}");
        seen.push(body);
        let still = FaceMotionHost::still(body, false);
        for size in [16., 36., 96.] {
            let tree = render_tree(&still, size);
            verify(&tree);
            assert_eq!(
                tree.eyes.len(),
                2,
                "missing front-facing eyes for {name} at {size}px"
            );
            if body == Body::Pumpkin {
                assert!(!tree.painted.is_empty());
            }
        }
        let mut turned = still;
        for yaw in [-75., -20., 30., 75., 173.] {
            turned.orientation = Quat::euler(v3(13., yaw, -20.) * DEG);
            verify(&render_tree(&turned, 60.));
        }
    }
    for (picker, engine) in [
        ("blob", "sphere"),
        ("flower", "starFlower"),
        ("heart", "heartChubby"),
        ("star", "star6"),
    ] {
        assert_eq!(Body::named(picker), Body::named(engine));
    }
}

#[test]
fn reference_still_geometry_matches_rea() {
    // Extents sampled from engine.js Hg(oo({body}).frame(), body, 240).
    let reference = [
        ("sphere", [48., 48., 192., 192.]),
        ("cloud", [50.44, 55.44, 202.59, 186.53]),
        ("square", [41.82, 48.44, 198.18, 191.56]),
        ("sparkle", [32.97, 32.51, 207.05, 207.42]),
        ("clover", [46.04, 49.88, 193.95, 190.12]),
        ("heartChubby", [37.51, 48.55, 187.59, 184.29]),
        ("starFlower", [42.97, 45.05, 197.03, 194.95]),
        ("teardrop", [57.75, 47.60, 192.85, 198.70]),
        ("tablet", [40.60, 58.35, 199.40, 181.65]),
        ("wedge", [62.72, 54.78, 218.29, 214.53]),
        ("house", [53.06, 51.50, 204.50, 199.35]),
        ("star6", [34.34, 37.34, 205.66, 202.66]),
        ("pebble", [48.14, 51.79, 194.07, 189.52]),
        ("bean", [55.57, 54.63, 175.45, 190.]),
        ("egg", [62.16, 53.43, 182.84, 189.12]),
        ("squircle", [41.96, 46.14, 198.04, 193.86]),
        ("capsule", [63.82, 52.07, 176.18, 187.93]),
        ("cylinder", [46.42, 44.52, 193.58, 195.48]),
        ("hex", [44.47, 46.78, 195.53, 193.22]),
        ("gem", [53.67, 54.52, 186.33, 185.48]),
        ("crystal", [56.41, 33.96, 183.59, 206.04]),
        ("shield", [43.06, 40.29, 178.77, 188.40]),
        ("dome", [60.51, 67.59, 203.54, 197.19]),
        ("arch", [62.37, 51.74, 189.26, 201.47]),
        ("leaf", [63.82, 56.26, 176.18, 183.74]),
        ("pumpkin", [44.26, 33., 194.57, 189.56]),
    ];
    let close = |p: &[V2], b: [f64; 4], tolerance: f64, label: &str| {
        let (a, z) = shapes::bounds(p);
        let actual = [a.0[0], a.0[1], z.0[0], z.0[1]];
        assert!(
            actual
                .into_iter()
                .zip(b)
                .all(|(a, b)| (a - b).abs() < tolerance),
            "{label}: {actual:?} vs {b:?}"
        );
    };
    let size = 240. / fill_scale(STILL_FILL);
    let mut mismatches = Vec::new();
    for (name, rect) in reference {
        let body = Body::named(name);
        let tree = render_tree(&FaceMotionHost::still(body, false), size);
        let mut p = tree.body.clone();
        for (_, outline) in &tree.painted {
            p.extend(outline);
        }
        let (a, b) = shapes::bounds(&p);
        let actual = [a.0[0], a.0[1], b.0[0], b.0[1]];
        if !actual
            .into_iter()
            .zip(rect)
            .all(|(a, b)| (a - b).abs() < 2.)
        {
            mismatches.push((name, actual, rect));
        }
        if body == Body::Sphere {
            close(
                &tree.eyes[0],
                [111.893, 79.0854, 142.4525, 114.2092],
                0.5,
                "left sphere eye",
            );
            close(
                &tree.eyes[1],
                [146.7212, 64.6249, 173.9311, 97.6289],
                0.5,
                "right sphere eye",
            );
        }
    }
    assert!(mismatches.is_empty(), "reference extents: {mismatches:#?}");
}

#[test]
fn all_expressions_and_transition_pairs_are_finite() {
    use Expression::*;
    let expressions = [
        Neutral, Happy, Surprised, Sad, Sleepy, Squinted, Calm, Inspecting, Confused,
    ];
    for from in expressions {
        for body in [Body::Sphere, Body::Tablet, Body::Heart, Body::Crystal] {
            let mut f = FaceMotionHost::still(body, false);
            f.eyes = Some(from.eyes());
            let tree = render_tree(&f, 72.);
            verify(&tree);
            // The authored Crystal/Confused eye mounts overlap. The stock mask
            // unions them, so the native even-odd path needs one merged contour.
            let count = if from == Confused && body == Body::Crystal {
                1
            } else {
                2
            };
            assert_eq!(tree.eyes.len(), count, "{from:?} {body:?}");
        }
        for to in expressions {
            for blink in [false, true] {
                let mut eyes = Eyes::new();
                eyes.set(from, false);
                eyes.advance(1.);
                eyes.set(to, blink);
                for i in 0..44 {
                    if i % 5 == 0 || i == 43 {
                        let p = eyes.frame(12., -1., true, 0.);
                        let mut frame = FaceMotionHost::still(Body::Sphere, false);
                        frame.eyes = Some(p);
                        let tree = render_tree(&frame, 72.);
                        verify(&tree);
                        if i == 43 {
                            assert_eq!(tree.eyes.len(), 2, "{from:?} -> {to:?} (blink: {blink})");
                        }
                    }
                    eyes.advance(DT);
                }
            }
        }
    }
}

#[test]
fn all_authored_clips_cover_every_track() {
    assert_eq!(CATALOG.clips.len(), 28);
    for (name, clip) in &CATALOG.clips {
        assert!(clip.duration.is_finite() && clip.duration > 0., "{name}");
        for i in 0..=240 {
            let p = clip.evaluate(clip.duration * i as f64 / 240., 0.6);
            assert!(
                p.rot.finite()
                    && p.pos.finite()
                    && p.squash.finite()
                    && p.zoom.is_finite()
                    && p.anchor.is_finite(),
                "{name}"
            );
            assert!((0. ..=1.).contains(&p.lids));
        }
        assert!(clip.expressions.windows(2).all(|w| w[0].t <= w[1].t));
    }
}

#[test]
fn all_director_states_and_morphs_step_and_render() {
    assert_eq!(CATALOG.states.len(), 53);
    assert_eq!(
        CATALOG
            .states
            .values()
            .filter_map(|s| s.morph)
            .map(|m| m as u8)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        13
    );
    for (state, spec) in &CATALOG.states {
        assert!(!spec.clips.is_empty() && spec.clips.iter().all(|c| CATALOG.clips.contains_key(c)));
        assert!(spec
            .next
            .as_ref()
            .is_none_or(|s| CATALOG.states.contains_key(s)));
        let mut host = FaceMotionHost::new(Body::Sphere, state, 42, false);
        let mut max_parts = 0;
        for i in 0..240 {
            host.advance(DT);
            if i % 11 == 0 || i == 239 {
                let frame = host.frame();
                assert!((frame.orientation.0.len() - 1.).abs() < 1e-12, "{state}");
                let tree = render_tree(&frame, 60.);
                verify(&tree);
                max_parts = max_parts.max(tree.parts.len());
            }
        }
        if spec.morph.is_some() && state != "bouncing" {
            assert!(max_parts > 0, "missing morph primitives: {state}");
        }
        if state == "thinking" {
            assert_eq!(max_parts, 2, "the animated head is the third dot");
        }
        if state == "orbit" {
            assert_eq!(max_parts, 5);
        }
        if state == "dictating" {
            assert_eq!(max_parts, 4, "the animated head is the fifth dot");
        }
    }
    let mut host = FaceMotionHost::new(Body::Sphere, "blocked", 1, false);
    for _ in 0..14_399 {
        host.advance(DT);
    }
    assert_eq!(host.state(), "blocked");
    host.advance(DT);
    assert_eq!(host.state(), "drowsy");
    host.set_state("blocked");
    assert_eq!(
        host.state(),
        "drowsy",
        "unchanged requested state must not restart the director"
    );
}

#[test]
fn pointer_gaze_pokes_and_body_spring_are_bounded_and_settle() {
    let make = || FaceMotionHost::new(Body::Sphere, "idle", 1, true);
    let mut a = make();
    let mut b = make();
    let distance = |a: Quat, b: Quat| 1. - a.0.dot(b.0).abs();
    a.set_gaze(Some(v2(1., -1.)));
    for _ in 0..90 {
        a.advance(DT);
        b.advance(DT);
    }
    assert!(distance(a.frame().orientation, b.frame().orientation) > 0.001);
    a.set_gaze(None);
    a.poke("spin");
    a.poke("nod");
    a.poke("bounce");
    a.blink();
    let mut spin = false;
    let mut old = a.frame().orientation;
    for _ in 0..240 {
        a.advance(DT);
        b.advance(DT);
        let frame = a.frame();
        let q = frame.orientation;
        assert!(q.0.finite() && (q.0.len() - 1.).abs() < 1e-12);
        assert!(
            distance(old, q) < 0.12,
            "orientation jumped between fixed steps"
        );
        old = q;
        spin |= distance(q, b.frame().orientation) > 0.9;
    }
    assert!(spin);
    assert!(distance(a.frame().orientation, b.frame().orientation) < 1e-12);
    a.look("left");
    a.advance(0.04);
    assert!(distance(a.frame().orientation, b.frame().orientation) > 0.);
    a.set_body(Body::Cloud);
    for _ in 0..18 {
        a.advance(DT);
    }
    let transition = a.frame();
    assert_eq!(transition.body_weights.len(), 2);
    verify(&render_tree(&transition, 56.));
    a.set_body(Body::Tablet);
    for _ in 0..240 {
        a.advance(DT);
    }
    assert_eq!(a.frame().body_weights, vec![(Body::Tablet, 1.)]);
    assert!((rest_rotation() - v3(-15.8962710433, 33.2931563504, 25.3848012746)).len() < 1e-8);
}

#[test]
fn fixed_steps_are_independent_of_paint_cadence_and_clamp_hitches() {
    let mut a = FaceMotionHost::new(Body::Sphere, "spawning", 15, false);
    let mut b = FaceMotionHost::new(Body::Sphere, "spawning", 15, false);
    for _ in 0..480 {
        a.advance(DT / 2.);
    }
    for _ in 0..120 {
        b.advance(2. * DT);
    }
    assert_eq!(a.frame().orientation.0, b.frame().orientation.0);
    assert_eq!(a.frame().body_weights, b.frame().body_weights);
    a.advance(60.);
    b.advance(0.05);
    a.advance(f64::NAN);
    a.advance(f64::INFINITY);
    a.advance(-1.);
    assert_eq!(a.frame().orientation.0, b.frame().orientation.0);
}

#[test]
fn seeded_motion_matches_reference_after_two_seconds() {
    // createDirector(createMotion({body:"sphere"}), state, {seed:42}),
    // followed by 240 director and engine advances of 1/120 each.
    let samples = [
        (
            "idle",
            [0.9985983, 0.0077859, 0.0502386, 0.0147273],
            [0.6308282, 0., 0., 1., 1.],
        ),
        (
            "streaming",
            [0.9999882, -0.0035729, 0.003213, 0.0007038],
            [0.6, 0.0105527, 0., 1., 1.],
        ),
        (
            "working",
            [-0.999776, 0.0002682, -0.0211168, -0.0014028],
            [0.6000391, 0.0016052, -0.0000314, 1., 1.],
        ),
        (
            "blocked",
            [0.9939929, 0.0823256, 0.0687354, 0.0218188],
            [0.4999806, -0.0347367, 0.0485297, 1., 1.],
        ),
        (
            "finished",
            [1., 0., 0., 0.],
            [0.6, 0., 0., 1.0166465, 0.9912162],
        ),
        (
            "failed",
            [0.9809245, 0.1940423, -0.0032184, -0.011154],
            [0.6, 0.0042594, -0.2232055, 1., 1.],
        ),
        ("sleeping", [1., 0., 0., 0.], [0.6034916, 0., 0., 1., 1.]),
        (
            "celebrate",
            [0.9513355, -0.1983916, -0.1346409, 0.1935803],
            [0.6, 0., 0., 1., 1.],
        ),
        ("writing", [0., 0., 1., 0.], [0.6350791, 0., 0., 1., 1.]),
    ];
    let mut mismatches = vec![];
    for (state, q, expected) in samples {
        let mut host = FaceMotionHost::new(Body::Sphere, state, 42, false);
        for _ in 0..240 {
            host.advance(DT);
        }
        let frame = host.frame();
        let [x, y] = frame.offset.0;
        let [u, v] = frame.squash.0;
        let actual = [frame.zoom, x, y, u, v];
        let q_delta = (frame.orientation.0 - VecN(q))
            .len()
            .min((frame.orientation.0 + VecN(q)).len());
        if q_delta > 1e-5
            || !actual
                .into_iter()
                .zip(expected)
                .all(|(a, b)| (a - b).abs() < 1e-5)
        {
            mismatches.push((state, frame.orientation.0 .0, q, actual, expected));
        }
    }
    assert!(
        mismatches.is_empty(),
        "motion differs from the reference: {mismatches:#?}"
    );
}

#[test]
fn edge_arrangement_clips_and_merges_overlapping_polygons() {
    use super::contours::{area, intersection, union};
    let rect = |x: f64, y: f64| [v2(x, y), v2(x + 2., y), v2(x + 2., y + 2.), v2(x, y + 2.)];
    let a = rect(0., 0.);
    let b = rect(1., 1.);
    let total = union([a.as_slice(), b.as_slice()].into_iter(), 1024.);
    let common = intersection(&a, &b);
    assert_eq!(total.len(), 1);
    assert_eq!(common.len(), 1);
    assert_eq!(area(&total[0]).abs(), 7.);
    assert_eq!(area(&common[0]).abs(), 1.);
}
