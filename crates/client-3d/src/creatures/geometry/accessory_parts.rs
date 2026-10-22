//! Jewellery and small pieces: chokers, collars, armlets, waist pieces,
//! pointed shoulders and garters. Each is built in the space of the joint it
//! rides on and fitted to the resolved wearer, so any outfit can wear it.
use super::super::outfit::{Neck, Waist};
use super::*;

/// What is worn on the neck itself, in the head joint's space where the
/// neck is built: chokers and standing collars turn with the head.
pub(super) fn neck_band(p: &mut Parts, neck: Neck, build: f32, slim: f32) {
    let r = |y: f32| {
        let profile = neck_profile(build, slim);
        let (a, b) = (profile[0], profile[1]);
        let t = ((y - a.0.y) / (b.0.y - a.0.y)).clamp(0.0, 1.0);
        a.1.lerp(b.1, t).max_element()
    };
    match neck {
        Neck::Choker(band) => {
            p.satin.torus(
                Vec3::new(0.0, -0.03, -0.008),
                Vec3::Y,
                r(-0.03) + 0.004,
                0.009,
                16,
                4,
                band,
            );
        }
        Neck::Jewel { band, gem } => {
            p.satin.torus(
                Vec3::new(0.0, -0.03, -0.008),
                Vec3::Y,
                r(-0.03) + 0.004,
                0.01,
                16,
                4,
                band,
            );
            p.metal.blob(
                Vec3::new(0.0, -0.03, r(-0.03) + 0.006),
                Vec3::new(0.013, 0.016, 0.008),
                1,
                0.0,
                gem,
            );
        }
        Neck::Stand(color) => {
            // A stiff collar rising round the neck, open in a V in front.
            for side in [-1.0f32, 1.0] {
                let path: Vec<Vec3> = (0..=6)
                    .map(|k| {
                        let a = side * (0.35 + k as f32 / 6.0 * (PI - 0.35));
                        Vec3::new(
                            a.sin() * (r(-0.05) + 0.012),
                            -0.04,
                            a.cos() * (r(-0.05) + 0.012) - 0.008,
                        )
                    })
                    .collect();
                for (i, w) in path.windows(2).enumerate() {
                    let rise = 0.05 + 0.015 * (i as f32 / 5.0);
                    p.satin.extrude(
                        &[
                            Vec2::new(0.0, 0.0),
                            Vec2::new(w[0].distance(w[1]), 0.0),
                            Vec2::new(w[0].distance(w[1]), rise),
                            Vec2::new(0.0, rise),
                        ],
                        w[0] - Vec3::Y * 0.025,
                        (w[1] - w[0]).normalize(),
                        Vec3::Y,
                        0.008,
                        color,
                    );
                }
            }
        }
        Neck::Collar { .. } | Neck::Teeth { .. } => {}
    }
}

/// What lies over the collarbones, in the torso's space: a broad beaded
/// collar, or teeth strung on a cord.
pub(super) fn neck_collar(p: &mut Parts, neck: Neck, build: f32) {
    match neck {
        Neck::Collar { metal, beads } => {
            // Bands rising in rings round the base of the neck.
            for (k, (radius, y)) in [(0.075, 0.548), (0.098, 0.53), (0.12, 0.508)]
                .into_iter()
                .enumerate()
            {
                p.metal.torus(
                    Vec3::new(0.0, y, -0.01),
                    Vec3::new(0.0, 1.0, -0.25).normalize(),
                    radius * build,
                    0.011,
                    20,
                    4,
                    if k % 2 == 0 { metal } else { beads },
                );
            }
            // A fringe of drops round the front.
            for k in -6i32..=6 {
                let a = k as f32 * 0.2;
                let at = Vec3::new(
                    a.sin() * 0.125 * build,
                    0.488,
                    a.cos() * 0.11 * build - 0.005,
                );
                p.satin
                    .blob(at, Vec3::new(0.008, 0.012, 0.008), 1, 0.0, beads);
            }
        }
        Neck::Teeth { cord } => {
            p.satin.torus(
                Vec3::new(0.0, 0.535, -0.01),
                Vec3::new(0.0, 1.0, -0.35).normalize(),
                0.078 * build,
                0.005,
                18,
                3,
                cord,
            );
            for k in -3i32..=3 {
                let a = k as f32 * 0.32;
                let root = Vec3::new(a.sin() * 0.082 * build, 0.525, a.cos() * 0.07 * build);
                let long = if k == 0 { 0.05 } else { 0.036 };
                p.matte.prism(
                    root,
                    root + Vec3::new(a.sin() * 0.008, -long, 0.01),
                    0.008,
                    0.0,
                    5,
                    BONE,
                    0.0,
                );
                p.satin.blob(root, Vec3::splat(0.007), 1, 0.0, cord * 0.7);
            }
        }
        _ => {}
    }
}

/// A band round the upper arm, in the arm joint's space.
pub(super) fn armlet(p: &mut Parts, color: Vec3, build: f32, arm_len: f32) {
    p.metal.torus(
        Vec3::Y * -0.13 * arm_len,
        Vec3::Y,
        0.05 * build,
        0.011,
        14,
        4,
        color,
    );
}

/// Stiff pointed shoulders standing up off the arm, in its joint's space.
pub(super) fn peak(p: &mut Parts, color: Vec3, side: f32, build: f32) {
    p.satin.extrude(
        &[
            Vec2::new(-0.03, -0.03),
            Vec2::new(0.105, -0.03),
            Vec2::new(0.12, 0.07),
        ],
        Vec3::new(0.0, 0.0, -0.05 * build),
        Vec3::X * side,
        Vec3::Y,
        0.1 * build,
        color,
    );
}

/// What is worn round the waist over the garments, in the hips' space.
pub(super) fn waist(p: &mut Parts, waist: Waist, body: HumanoidBody) {
    let b = body.build;
    let belt = super::super::robe::belt_radii(b, body.waist);
    let at =
        |a: f32, y: f32, out: f32| Vec3::new(a.sin() * (belt.x + out), y, a.cos() * (belt.y + out));
    match waist {
        Waist::Sash(color) => {
            // A broad sash round the waist, knotted at her left hip with
            // two tails hanging from the knot.
            tube(
                &mut p.matte,
                &[Vec3::Y * 0.04, Vec3::Y * 0.11],
                &[(belt.x + 0.008, belt.y + 0.008); 2],
                color,
            );
            let knot = at(0.9, 0.06, 0.012);
            p.matte
                .blob(knot, Vec3::new(0.028, 0.03, 0.022), 3, 0.1, color);
            for (k, swing) in [(0.0f32, -0.02f32), (1.0, 0.03)] {
                p.matte.extrude(
                    &[
                        Vec2::new(-0.022, 0.0),
                        Vec2::new(0.022, 0.0),
                        Vec2::new(0.03 + swing, 0.3 - 0.06 * k),
                        Vec2::new(-0.01 + swing, 0.32 - 0.06 * k),
                    ],
                    knot + Vec3::new(0.0, -0.01, 0.006),
                    Vec3::new(0.9f32.cos(), 0.0, -0.9f32.sin()),
                    Vec3::NEG_Y,
                    0.006,
                    color * (1.0 - 0.08 * k),
                );
            }
        }
        Waist::Coins(metal) => {
            // A belt low on the hips, dipping to a point in front, hung
            // with swags of coins.
            let path: Vec<Vec3> = (0..=16)
                .map(|k| {
                    let a = (k as f32 / 16.0 - 0.5) * TAU;
                    at(a, -0.005 - 0.05 * a.cos().max(0.0).powi(4), 0.012)
                })
                .collect();
            tube(
                &mut p.metal,
                &path,
                &vec![(0.012, 0.012); path.len()],
                metal,
            );
            for row in 0..2 {
                for k in -5i32..=5 {
                    let a = k as f32 * 0.24;
                    let sag = 0.035 * (1.0 - (k as f32 / 5.0).powi(2));
                    let coin = at(
                        a,
                        -0.03 - 0.05 * a.cos().powi(4) - sag - 0.035 * row as f32,
                        0.02,
                    );
                    p.metal
                        .blob(coin, Vec3::new(0.011, 0.011, 0.003), 1, 0.0, metal);
                }
            }
        }
        Waist::Crossed(color) => {
            // Cords crossing round the waist above the hips, tied in front
            // with a bow and two hanging ends.
            for k in 0..3 {
                let y = 0.1 + 0.035 * k as f32;
                let path: Vec<Vec3> = (0..=16)
                    .map(|j| {
                        let a = (j as f32 / 16.0) * TAU;
                        at(a, y + 0.03 * (a * 2.0).sin(), 0.006)
                    })
                    .collect();
                tube(
                    &mut p.metal,
                    &path,
                    &vec![(0.005, 0.005); path.len()],
                    color,
                );
            }
            let knot = at(0.0, 0.08, 0.012);
            for lobe in [-1.0f32, 1.0] {
                p.metal.blob(
                    knot + Vec3::X * lobe * 0.018,
                    Vec3::new(0.016, 0.01, 0.006),
                    1,
                    0.0,
                    color,
                );
                tube(
                    &mut p.metal,
                    &[knot, knot + Vec3::new(lobe * 0.012, -0.08, 0.004)],
                    &[(0.004, 0.004); 2],
                    color,
                );
            }
        }
    }
}

/// A cloth tie knotted round her left thigh, in the leg joint's space.
pub(super) fn garter(p: &mut Parts, color: Vec3, build: f32) {
    let y = -0.2;
    p.matte
        .torus(Vec3::Y * y, Vec3::Y, 0.082 * build, 0.012, 14, 4, color);
    p.matte.blob(
        Vec3::new(0.07 * build, y, 0.03),
        Vec3::new(0.018, 0.02, 0.016),
        2,
        0.1,
        color,
    );
    p.matte.extrude(
        &[
            Vec2::new(-0.012, 0.0),
            Vec2::new(0.012, 0.0),
            Vec2::new(0.02, 0.14),
            Vec2::new(-0.004, 0.13),
        ],
        Vec3::new(0.075 * build, y - 0.01, 0.035),
        Vec3::Z,
        Vec3::NEG_Y,
        0.004,
        color,
    );
}
