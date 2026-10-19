//! The Rogue scenario's own monsters, drawn from the maintainer's concept
//! sheets: people-like kinds dressed on the shared humanoid recipe, winged
//! fliers, four-legged beasts and aberrations, and a rooted flytrap.
//!
//! Every recipe composes the same sculpting helpers, joints and Storybook
//! proportions as the rest of the cast. Nothing here decides what the server
//! has not disclosed: a bat is drawn on the wing because it is a bat.
use super::*;
use crate::sdf::{Sculpt, Shape::*};

// ---------------------------------------------------------------------------
// Shared pieces.

/// A flat, pointed leaf from `root` along `dir`, broad a third of the way out.
fn leaf(b: &mut Builder, root: Vec3, dir: Vec3, len: f32, width: f32, color: Vec3) {
    let dir = dir.normalize_or(Vec3::NEG_Y);
    let across = dir.cross(Vec3::Y).normalize_or(Vec3::X);
    let bend = across.cross(dir).normalize_or(Vec3::Z) * len * 0.08;
    let path = [
        root,
        root + dir * len * 0.3 + bend * 0.5,
        root + dir * len * 0.7 + bend,
        root + dir * len + bend * 0.6,
    ];
    let radii = [
        Vec2::new(width * 0.25, 0.004),
        Vec2::new(width, 0.005),
        Vec2::new(width * 0.65, 0.004),
        Vec2::splat(0.0015),
    ];
    b.tube_framed(
        &path,
        &radii,
        6,
        &|_, _, t| color * (0.88 + 0.24 * (t * PI).sin()),
        Some(across),
    );
}

/// A flat ribbon of cloth hanging from `top`, its end torn to a ragged point.
#[allow(clippy::too_many_arguments)]
fn strip(
    b: &mut Builder,
    top: Vec3,
    out: Vec3,
    across: Vec3,
    length: f32,
    width: f32,
    color: Vec3,
    seed: u32,
) {
    let flare = out.with_y(0.0).normalize_or(Vec3::Z);
    let sway = across * (rand(91, seed) - 0.5) * 0.04;
    let path = [
        top,
        top + Vec3::NEG_Y * length * 0.35 + flare * length * 0.08,
        top + Vec3::NEG_Y * length * 0.72 + flare * length * 0.14 + sway * 0.5,
        top + Vec3::NEG_Y * length + flare * length * 0.17 + sway,
    ];
    let ragged = 0.35 + 0.4 * rand(92, seed);
    let radii = [
        Vec2::new(width, 0.006),
        Vec2::new(width * 0.95, 0.006),
        Vec2::new(width * ragged, 0.005),
        Vec2::splat(0.002),
    ];
    b.tube_framed(
        &path,
        &radii,
        6,
        &|_, _, t| color * (1.0 - 0.22 * t),
        Some(across),
    );
}

/// A ring of torn hanging strips around an elliptical hem.
#[allow(clippy::too_many_arguments)]
fn tatters(
    b: &mut Builder,
    center: Vec3,
    radius: Vec2,
    count: u32,
    length: f32,
    width: f32,
    color: Vec3,
    seed: u32,
) {
    for k in 0..count {
        let a = (k as f32 + rand(93, seed + k) * 0.4) / count as f32 * TAU;
        let out = Vec3::new(a.sin(), 0.0, a.cos());
        let top = center + Vec3::new(a.sin() * radius.x, 0.0, a.cos() * radius.y);
        let across = Vec3::new(a.cos(), 0.0, -a.sin());
        let long = length * (0.55 + 0.6 * rand(94, seed + k));
        let shade = 0.82 + 0.3 * rand(95, seed + k);
        strip(b, top, out, across, long, width, color * shade, seed + k);
    }
}

/// A pointed ear from the side of the head, flat and swept back.
fn pointed_ear(p: &mut Parts, side: f32, tip: Vec3, width: f32, color: Vec3) {
    let root = Vec3::new(side * 0.093, 0.15, -0.005);
    let tip = Vec3::new(side * tip.x, tip.y, tip.z);
    let mid = root.lerp(tip, 0.45) + Vec3::Y * 0.01;
    let across = (tip - root).cross(Vec3::Z).normalize_or(Vec3::Y);
    let radii = [
        Vec2::new(width, 0.012),
        Vec2::new(width * 0.75, 0.01),
        Vec2::splat(0.002),
    ];
    p.matte
        .tube_framed(&[root, mid, tip], &radii, 8, &|_, _, _| color, Some(across));
    // A darker hollow inside the ear.
    let inner = [
        root + Vec3::X * side * 0.006,
        mid + Vec3::X * side * 0.006,
        tip.lerp(root, 0.2) + Vec3::X * side * 0.006,
    ];
    let radii = [
        Vec2::new(width * 0.55, 0.004),
        Vec2::new(width * 0.4, 0.004),
        Vec2::splat(0.0015),
    ];
    p.matte
        .tube_framed(&inner, &radii, 6, &|_, _, _| color * 0.72, Some(across));
}

/// A tusk curving up from the lower jaw.
fn tusk(p: &mut Parts, side: f32, height: f32, radius: f32) {
    let root = Vec3::new(side * 0.03, 0.074, 0.088);
    let path = [
        root,
        root + Vec3::new(side * 0.006, height * 0.55, 0.012),
        root + Vec3::new(side * 0.012, height, 0.006),
    ];
    let radii: [(f32, f32); 3] = [
        (radius, radius),
        (radius * 0.75, radius * 0.75),
        (0.002, 0.002),
    ];
    tube(&mut p.matte, &path, &radii, BONE);
}

/// A shaved scalp with a tied topknot and tail, as the orcs and trolls wear.
fn topknot(p: &mut Parts, l: &Look, wild: bool) {
    let c = hair_color(l);
    // Close-cropped stubble over the crown and sides.
    p.matte.blob(
        Vec3::new(0.0, 0.205, -0.02),
        Vec3::new(0.104, 0.093, 0.109),
        2,
        0.0,
        c.lerp(l.skin, 0.55),
    );
    // A crest from the brow back to the knot.
    for k in 0..5 {
        let z = 0.06 - k as f32 * 0.04;
        let root = Vec3::new(0.0, 0.29 - (k as f32 - 1.5).powi(2) * 0.004, z);
        tube(
            &mut p.matte,
            &[root, root + Vec3::new(0.0, 0.035, -0.02)],
            &[(0.03, 0.028), (0.012, 0.012)],
            c,
        );
    }
    let knot = Vec3::new(0.0, 0.31, -0.08);
    p.matte.blob(knot, Vec3::new(0.04, 0.035, 0.04), 1, 0.0, c);
    p.matte.torus(
        knot + Vec3::new(0.0, -0.01, -0.025),
        Vec3::new(0.0, 0.4, -1.0).normalize(),
        0.026,
        0.007,
        10,
        4,
        LEATHER * 0.7,
    );
    let strands = if wild { 4 } else { 1 };
    for k in 0..strands {
        let spread = (k as f32 - (strands - 1) as f32 * 0.5) * 0.035;
        lock(
            &mut p.matte,
            &[
                knot + Vec3::new(spread * 0.3, -0.01, -0.035),
                knot + Vec3::new(spread, -0.04, -0.08),
                knot + Vec3::new(spread * 1.5, -0.13, -0.1),
                knot + Vec3::new(spread * 1.8, -0.24, -0.08),
            ],
            if wild { 0.03 } else { 0.034 },
            0.022,
            c * (0.92 + 0.1 * k as f32),
        );
    }
}

/// A wreath of leaves around the crown of the head.
fn leaf_crown(p: &mut Parts, color: Vec3, radius: f32, y: f32) {
    for k in 0..14 {
        let a = k as f32 / 14.0 * TAU;
        let out = Vec3::new(a.sin(), 0.0, a.cos());
        let root = Vec3::new(
            a.sin() * radius,
            y + 0.012 * (a * 3.0).sin(),
            a.cos() * radius - 0.02,
        );
        let dir = (out + Vec3::Y * 0.7 + Vec3::new(a.cos(), 0.0, -a.sin()) * 0.4).normalize();
        let shade = 0.85 + 0.3 * rand(97, k);
        leaf(&mut p.matte, root, dir, 0.06, 0.022, color * shade);
    }
}

// ---------------------------------------------------------------------------
// People-like monsters, dressed on the shared humanoid recipe.

/// Chest details for the Rogue people, fitted to the outer garment.
pub(super) fn torso_details(
    p: &mut Parts,
    l: &Look,
    body: HumanoidBody,
    garment: &Sculpt,
    skin: &Sculpt,
) {
    let b = body.build;
    let (sh, w) = (body.shoulders * b, body.waist * b);
    match l.style {
        Style::Orc | Style::Troll => {
            // A harness strap across the chest, buckled to a ring at the sternum.
            for front in [true, false] {
                let strap = [
                    (Vec2::new(0.15 * sh, 0.45), 0.026 * b),
                    (Vec2::new(0.05 * b, 0.33), 0.026 * b),
                    (Vec2::new(-0.06 * b, 0.21), 0.026 * b),
                    (Vec2::new(-0.13 * w, 0.05), 0.022 * b),
                ];
                fitted_ribbon(&mut p.satin, garment, &strap, front, 0.016, LEATHER * 0.62);
            }
            let (ring, normal) = torso_surface_frame(garment, Vec2::new(0.03 * b, 0.31), true);
            p.metal.torus(
                ring + normal * 0.026,
                normal,
                0.03,
                0.008,
                14,
                4,
                Vec3::new(0.82, 0.58, 0.26),
            );
            if l.style == Style::Troll {
                // Shoulder straps over bare skin, riveted.
                for side in [-1.0f32, 1.0] {
                    let front = torso_surface(skin, side * 0.1 * b, 0.42, true);
                    let back = torso_surface(skin, side * 0.1 * b, 0.42, false);
                    tube(
                        &mut p.satin,
                        &[
                            front,
                            Vec3::new(side * 0.12 * b, 0.54, 0.05),
                            Vec3::new(side * 0.12 * b, 0.54, -0.06),
                            back,
                        ],
                        &[(0.032, 0.011); 4],
                        LEATHER * 0.7,
                    );
                }
            }
        }
        Style::Leprechaun => {
            // A cream stock at the throat and gilt buttons down the waistcoat.
            fitted_ribbon(
                &mut p.matte,
                skin,
                &[
                    (Vec2::new(0.0, 0.5), 0.03 * b),
                    (Vec2::new(0.0, 0.43), 0.024 * b),
                    (Vec2::new(0.0, 0.37), 0.012 * b),
                ],
                true,
                0.012,
                CREAM,
            );
            for k in 0..4 {
                let (at, normal) =
                    torso_surface_frame(skin, Vec2::new(0.0, 0.12 + k as f32 * 0.065), true);
                p.metal
                    .blob(at + normal * 0.012, Vec3::splat(0.012), 0, 0.0, GILT);
            }
            fitted_band(&mut p.satin, skin, 0.035, 0.07, 0.008, LEATHER * 0.6);
        }
        Style::Nymph => {
            // A bodice of overlapping leaves, pinned with a gilt diamond.
            for k in 0..9 {
                let x = ((k % 3) as f32 - 1.0) * 0.07 * b;
                let y = 0.2 + (k / 3) as f32 * 0.07;
                let (at, normal) = torso_surface_frame(skin, Vec2::new(x, y), true);
                let dir = (Vec3::NEG_Y + Vec3::X * x * 3.0).normalize();
                leaf(
                    &mut p.matte,
                    at + normal * 0.012 + Vec3::Y * 0.03,
                    dir + normal * 0.25,
                    0.08,
                    0.032,
                    l.cloth * (0.85 + 0.3 * rand(98, k)),
                );
            }
            let (at, normal) = torso_surface_frame(skin, Vec2::new(0.0, 0.1), true);
            p.metal.blob(
                at + normal * 0.02,
                Vec3::new(0.022, 0.03, 0.01),
                0,
                0.0,
                l.trim,
            );
        }
        Style::Medusa => {
            // A gilt neckline and a bronze disc at the waist.
            fitted_band(&mut p.metal, skin, 0.42, 0.44, 0.01, l.trim);
            let (at, normal) = torso_surface_frame(skin, Vec2::new(0.0, 0.07), true);
            p.metal
                .torus(at + normal * 0.02, normal, 0.03, 0.01, 14, 4, l.trim);
            p.metal
                .blob(at + normal * 0.022, Vec3::splat(0.02), 0, 0.0, l.trim * 0.8);
        }
        Style::Phantom | Style::Wraith => {
            // A knotted sash binds the torn cloth at the waist.
            let sash = if l.style == Style::Wraith {
                l.trim
            } else {
                LEATHER * 0.7
            };
            fitted_band(&mut p.matte, garment, 0.02, 0.07, 0.01, sash);
            let (at, normal) = torso_surface_frame(garment, Vec2::new(0.05 * b, 0.045), true);
            p.matte.blob(
                at + normal * 0.02,
                Vec3::new(0.03, 0.025, 0.018),
                1,
                0.0,
                sash,
            );
            if l.style == Style::Wraith {
                // The cowl of the cloak falls in a long scarf down the front.
                fitted_ribbon(
                    &mut p.matte,
                    garment,
                    &[
                        (Vec2::new(-0.07 * b, 0.47), 0.04 * b),
                        (Vec2::new(-0.06 * b, 0.32), 0.042 * b),
                        (Vec2::new(-0.05 * b, 0.16), 0.036 * b),
                    ],
                    true,
                    0.016,
                    l.trim * 0.9,
                );
            }
        }
        _ => {}
    }
}

/// Head details for the Rogue people, in the face's own coordinates.
pub(super) fn head_details(p: &mut Parts, l: &Look) {
    let fem = l.v.woman;
    match l.style {
        Style::Orc => {
            for side in [-1.0f32, 1.0] {
                pointed_ear(p, side, Vec3::new(0.17, 0.215, -0.06), 0.026, l.skin);
                tusk(p, side, if fem { 0.035 } else { 0.05 }, 0.011);
            }
            topknot(p, l, fem);
        }
        Style::Troll => {
            for side in [-1.0f32, 1.0] {
                pointed_ear(p, side, Vec3::new(0.24, 0.2, -0.08), 0.03, l.skin);
                tusk(p, side, 0.04, 0.012);
            }
            // A big ruddy nose.
            p.matte.blob(
                Vec3::new(0.0, 0.13, 0.118),
                Vec3::new(0.038, 0.034, 0.044),
                2,
                0.0,
                Vec3::new(0.82, 0.36, 0.28),
            );
            topknot(p, l, true);
        }
        Style::Leprechaun => {
            for side in [-1.0f32, 1.0] {
                pointed_ear(p, side, Vec3::new(0.16, 0.235, -0.04), 0.022, l.skin);
                // Rosy cheeks.
                p.matte.blob(
                    Vec3::new(side * 0.06, 0.125, 0.085),
                    Vec3::new(0.018, 0.012, 0.008),
                    0,
                    0.0,
                    l.skin.lerp(Vec3::new(0.85, 0.42, 0.38), 0.45),
                );
            }
        }
        Style::Nymph => {
            for side in [-1.0f32, 1.0] {
                pointed_ear(p, side, Vec3::new(0.15, 0.24, -0.05), 0.02, l.skin);
            }
            leaf_crown(p, l.cloth, 0.1, 0.27);
            if fem {
                for side in [-1.0f32, 1.0] {
                    p.metal.blob(
                        Vec3::new(side * 0.1, 0.11, 0.0),
                        Vec3::new(0.006, 0.014, 0.004),
                        0,
                        0.0,
                        l.trim,
                    );
                }
            }
        }
        Style::Medusa => {
            for side in [-1.0f32, 1.0] {
                pointed_ear(p, side, Vec3::new(0.14, 0.21, -0.04), 0.02, l.skin);
                // Scales over the temples and down the cheeks.
                for k in 0..6 {
                    let y = 0.2 - k as f32 * 0.018;
                    let x = side * (0.075 + 0.006 * (k % 2) as f32);
                    p.matte.blob(
                        Vec3::new(x, y, 0.06 - k as f32 * 0.004),
                        Vec3::new(0.008, 0.006, 0.004),
                        0,
                        0.0,
                        l.skin * 0.72,
                    );
                }
                if fem {
                    p.metal.torus(
                        Vec3::new(side * 0.102, 0.11, 0.0),
                        Vec3::X,
                        0.012,
                        0.003,
                        10,
                        3,
                        l.trim,
                    );
                }
            }
            // A dark scaled scalp from which the serpents grow.
            p.matte.blob(
                Vec3::new(0.0, 0.205, -0.02),
                Vec3::new(0.106, 0.095, 0.11),
                2,
                0.0,
                Vec3::new(0.2, 0.32, 0.22),
            );
        }
        Style::Phantom => {
            // Hollow shadows under bright eyes.
            for side in [-1.0f32, 1.0] {
                p.matte.blob(
                    Vec3::new(side * 0.038, 0.142, 0.096),
                    Vec3::new(0.02, 0.008, 0.006),
                    0,
                    0.0,
                    l.skin * 0.6,
                );
            }
        }
        _ => {}
    }
}

/// Where a point on the face mesh lands in the head joint's frame: the
/// head part's in-mesh enlargement, then its Storybook proportion.
fn head_space(at: Vec3, woman: bool) -> Vec3 {
    let size = if woman { 1.1 } else { 1.14 };
    let enlarged = Vec3::Y * 0.08 + (at - Vec3::Y * 0.08) * size;
    enlarged * super::super::anatomy::storybook_part(Plan::Biped, J::Head)
}

/// Garments and growths hung on the finished humanoid rig.
pub(super) fn biped_extras(
    kit: &mut Kit,
    l: &Look,
    body: HumanoidBody,
    torso: Entity,
    hips: Entity,
) {
    let b = body.build;
    let pelvis = body.pelvis_radii();
    match l.style {
        Style::Orc => {
            // A red loincloth front and back, hanging from the belt.
            let red = l.trim;
            kit.attach(hips, Transform::IDENTITY, "loincloth", |p| {
                for front in [1.0f32, -1.0] {
                    let down = Vec3::new(0.0, 1.0, -0.12 * front).normalize();
                    p.matte.extrude(
                        &[
                            Vec2::new(-0.075 * b, 0.0),
                            Vec2::new(0.075 * b, 0.0),
                            Vec2::new(0.09 * b, -0.4),
                            Vec2::new(0.0, -0.44),
                            Vec2::new(-0.09 * b, -0.4),
                        ],
                        Vec3::new(0.0, 0.04, front * (pelvis.z + 0.03)),
                        Vec3::X,
                        down,
                        0.012,
                        red,
                    );
                    // A worked border near the hem.
                    p.matte.extrude(
                        &[
                            Vec2::new(-0.088 * b, -0.33),
                            Vec2::new(0.088 * b, -0.33),
                            Vec2::new(0.09 * b, -0.36),
                            Vec2::new(-0.09 * b, -0.36),
                        ],
                        Vec3::new(0.0, 0.04, front * (pelvis.z + 0.037)),
                        Vec3::X,
                        down,
                        0.004,
                        GILT * 0.8,
                    );
                }
                p.metal.torus(
                    Vec3::new(0.0, 0.07, pelvis.z + 0.045),
                    Vec3::Z,
                    0.035,
                    0.01,
                    14,
                    4,
                    Vec3::new(0.82, 0.58, 0.26),
                );
            });
        }
        Style::Troll => {
            // Ragged strips of the kilt and a hanging leather apron.
            let kilt = l.cloth;
            kit.attach(hips, Transform::IDENTITY, "rags", |p| {
                tatters(
                    &mut p.matte,
                    Vec3::Y * -0.16,
                    Vec2::new(pelvis.x * 1.15, pelvis.z * 1.15),
                    16,
                    0.14,
                    0.03,
                    kilt,
                    11,
                );
                p.satin.extrude(
                    &[
                        Vec2::new(-0.08 * b, 0.0),
                        Vec2::new(0.08 * b, 0.0),
                        Vec2::new(0.07 * b, -0.32),
                        Vec2::new(-0.07 * b, -0.32),
                    ],
                    Vec3::new(0.0, 0.03, pelvis.z + 0.035),
                    Vec3::X,
                    Vec3::new(0.0, 1.0, -0.1).normalize(),
                    0.012,
                    LEATHER * 0.75,
                );
            });
        }
        Style::Leprechaun => {
            // A broad belt with a big gilt buckle over the coat.
            kit.attach(hips, Transform::IDENTITY, "buckle", |p| {
                let front = pelvis.z + 0.022;
                p.metal.torus(
                    Vec3::new(0.0, 0.07, front),
                    Vec3::Z,
                    0.04,
                    0.009,
                    4,
                    4,
                    GILT,
                );
                p.metal.cuboid(
                    Vec3::new(0.0, 0.07, front),
                    Vec3::new(0.006, 0.03, 0.006),
                    Quat::IDENTITY,
                    GILT,
                );
                p.satin.cuboid(
                    Vec3::new(0.11 * b, 0.0, front - 0.02),
                    Vec3::new(0.035, 0.045, 0.025),
                    Quat::from_rotation_y(0.4),
                    LEATHER * 0.8,
                );
            });
        }
        Style::Nymph => {
            // Two layers of long leaves over the short skirt.
            let green = l.cloth;
            kit.attach(hips, Transform::IDENTITY, "leaves", |p| {
                for layer in 0..2 {
                    let count = 13 + layer * 3;
                    for k in 0..count {
                        let a = (k as f32 + layer as f32 * 0.5) / count as f32 * TAU;
                        let out = Vec3::new(a.sin(), 0.0, a.cos());
                        let root = Vec3::new(
                            a.sin() * pelvis.x * 1.05,
                            0.04 - layer as f32 * 0.09,
                            a.cos() * pelvis.z * 1.05,
                        );
                        let dir = (Vec3::NEG_Y + out * 0.35).normalize();
                        let shade = 0.8 + 0.35 * rand(99, k + layer * 40);
                        leaf(
                            &mut p.matte,
                            root,
                            dir,
                            0.16 + 0.05 * rand(100, k),
                            0.045,
                            green.lerp(l.trim, 0.25 * layer as f32) * shade,
                        );
                    }
                }
            });
        }
        Style::Medusa => {
            // Bronze discs on the shoulders, and serpents for hair.
            for (arm, side) in [(J::ArmL, 1.0f32), (J::ArmR, -1.0)] {
                if let Some(joint) = kit.rig.joint(arm) {
                    let trim = l.trim;
                    kit.attach(joint, Transform::IDENTITY, &format!("disc{side}"), |p| {
                        p.metal.blob(
                            Vec3::new(side * 0.01, 0.03, 0.0),
                            Vec3::new(0.06, 0.03, 0.065) * b,
                            1,
                            0.0,
                            trim * 0.85,
                        );
                        p.metal.torus(
                            Vec3::new(side * 0.03, 0.035, 0.0),
                            Vec3::new(side, 1.0, 0.0).normalize(),
                            0.04 * b,
                            0.008,
                            14,
                            3,
                            trim,
                        );
                    });
                }
            }
            if let Some(head) = kit.rig.joint(J::Head) {
                medusa_serpents(kit, l, head);
            }
        }
        Style::Phantom | Style::Wraith => {
            // Torn hems below the robe-skirt, and rags from the sleeves.
            let (length, cloth) = if l.style == Style::Wraith {
                (0.66, l.cloth)
            } else {
                (0.5, l.cloth * 0.9)
            };
            let hem = Vec2::new(
                (0.208 + length * 0.045) * b * body.hips_width,
                (0.208 + length * 0.045) * 0.74 * b,
            );
            kit.attach(hips, Transform::IDENTITY, "tatters", |p| {
                tatters(
                    &mut p.matte,
                    Vec3::Y * (0.06 - length),
                    hem,
                    22,
                    0.24,
                    0.035,
                    cloth,
                    21,
                );
            });
            for (fore, side) in [(J::ForeL, 1.0f32), (J::ForeR, -1.0)] {
                if let Some(joint) = kit.rig.joint(fore) {
                    let sleeve = cloth * 0.95;
                    kit.attach(joint, Transform::IDENTITY, &format!("rags{side}"), |p| {
                        tatters(
                            &mut p.matte,
                            Vec3::Y * -0.06,
                            Vec2::new(0.055 * b, 0.055 * b),
                            6,
                            0.16,
                            0.024,
                            sleeve,
                            31 + side as u32,
                        );
                    });
                }
            }
            if l.style == Style::Wraith {
                wraith_cloak(kit, l, body, torso);
            }
        }
        _ => {}
    }
}

/// A long hooded cloak worn to rags, hanging from the shoulders.
fn wraith_cloak(kit: &mut Kit, l: &Look, body: HumanoidBody, torso: Entity) {
    let cloak = l.cloth * 0.8;
    let lining = l.trim * 0.7;
    kit.attach(torso, Transform::from_xyz(0.0, 0.53, -0.15), "cloak", |p| {
        let folds = 7;
        for k in 0..folds {
            let t = k as f32 / (folds - 1) as f32 * 2.0 - 1.0;
            let top = Vec3::new(
                t * 0.18 * body.profile.shoulders,
                0.0,
                -0.01 - 0.02 * (1.0 - t * t),
            );
            let drop = 1.1 + 0.12 * rand(96, k);
            let bottom = Vec3::new(
                t * 0.34 * body.profile.hips,
                -drop,
                -0.15 - 0.05 * (1.0 - t * t),
            );
            let mid = top.lerp(bottom, 0.5) + Vec3::new(0.0, 0.0, -0.03);
            let c = if t.abs() > 0.9 {
                lining
            } else {
                cloak * (0.92 + 0.08 * (k as f32 * 2.3).sin())
            };
            tube(
                &mut p.matte,
                &[top, mid, bottom],
                &[
                    (0.045 * body.profile.shoulders, 0.025),
                    (0.055, 0.03),
                    (0.05 * body.profile.hips, 0.024),
                ],
                c,
            );
            // Each fold ends in torn rags.
            strip(
                &mut p.matte,
                bottom,
                Vec3::new(t, 0.0, -1.0),
                Vec3::X,
                0.18 + 0.1 * rand(97, k),
                0.04,
                c * 0.9,
                41 + k,
            );
        }
        p.matte.blob(
            Vec3::ZERO,
            Vec3::new(0.22 * body.profile.shoulders, 0.04, 0.06),
            2,
            0.0,
            cloak,
        );
    });
}

/// Serpents in place of hair: each its own posable piece rooted in the scalp.
fn medusa_serpents(kit: &mut Kit, l: &Look, head: Entity) {
    let fem = l.v.woman;
    let count = 11;
    for k in 0..count {
        let a = (k as f32 + 0.5) / count as f32 * TAU;
        // Rooted over the crown and back of the head, none over the face.
        let lean = if a.cos() > 0.55 { 0.35 } else { 1.0 };
        let root_mesh = Vec3::new(a.sin() * 0.075, 0.27, a.cos() * 0.07 - 0.03);
        let root = head_space(root_mesh, fem);
        let out = Vec3::new(a.sin(), 0.0, a.cos() * lean - 0.4).normalize();
        let length = 0.18 + 0.07 * rand(101, k);
        let skin = Vec3::new(0.22, 0.42, 0.24) * (0.85 + 0.3 * rand(102, k));
        let belly = Vec3::new(0.78, 0.74, 0.5);
        let gold = l.trim;
        let model = kit.part(&format!("serpent{k}"), |p| {
            // A rising, outward-curling body ending in a raised head.
            let path = [
                Vec3::ZERO,
                Vec3::Y * length * 0.35 + out * length * 0.1,
                Vec3::Y * length * 0.55 + out * length * 0.45,
                Vec3::Y * length * 0.5 + out * length * 0.8,
            ];
            let radii = [
                Vec2::splat(0.018),
                Vec2::splat(0.016),
                Vec2::splat(0.014),
                Vec2::splat(0.012),
            ];
            p.matte.tube(&path, &radii, 8, &|dir, _, _| {
                if dir.y < -0.4 {
                    belly
                } else {
                    skin
                }
            });
            // A gilt band near the root.
            p.metal.torus(
                path[1] * 0.6,
                (path[1] - path[0]).normalize(),
                0.019,
                0.004,
                10,
                3,
                gold,
            );
            let face = path[3];
            let ahead = (out + Vec3::Y * 0.25).normalize();
            p.matte.blob(
                face + ahead * 0.02,
                Vec3::new(0.022, 0.016, 0.03),
                1,
                0.0,
                skin,
            );
            // An open mouth with a pink lining, and amber eyes.
            p.matte.blob(
                face + ahead * 0.045 - Vec3::Y * 0.006,
                Vec3::new(0.014, 0.004, 0.012),
                0,
                0.0,
                Vec3::new(0.8, 0.36, 0.42),
            );
            let across = ahead.cross(Vec3::Y).normalize_or(Vec3::X);
            for side in [-1.0f32, 1.0] {
                p.glow.blob(
                    face + ahead * 0.03 + across * side * 0.014 + Vec3::Y * 0.008,
                    Vec3::splat(0.0055),
                    0,
                    0.0,
                    Vec3::ONE,
                );
            }
            p.accent = Some(Accent::Amber);
        });
        let e = kit
            .commands
            .spawn((Transform::from_translation(root), Visibility::default()))
            .id();
        kit.commands.entity(e).with_children(|p| model.attach(p));
        kit.commands.entity(head).add_child(e);
        kit.rig.segments.push(e);
    }
}

// ---------------------------------------------------------------------------
// Fliers: always on the wing.

/// Height of a flier's body above the floor, before its overall scale.
pub(in crate::creatures) const FLIGHT: f32 = 1.6;

/// A fan of triangles from `center` through the `rim`, its free edges
/// scalloped between the points like stretched skin. Seen from both sides.
fn membrane(b: &mut Builder, center: Vec3, rim: &[Vec3], color: Vec3) {
    for pair in rim.windows(2) {
        let (a, c) = (pair[0], pair[1]);
        let scallop = a.lerp(c, 0.5).lerp(center, 0.16);
        for tri in [[center, a, scallop], [center, scallop, c]] {
            b.tri(tri, color);
            b.tri([tri[0], tri[2], tri[1]], color * 0.82);
        }
    }
}

/// A flat feather from `root` along `dir`, lying across `across`, barred.
#[allow(clippy::too_many_arguments)]
fn vane(
    b: &mut Builder,
    root: Vec3,
    dir: Vec3,
    len: f32,
    width: f32,
    across: Vec3,
    color: Vec3,
    bars: Option<Vec3>,
) {
    let dir = dir.normalize_or(Vec3::NEG_Z);
    let path = [
        root,
        root + dir * len * 0.3,
        root + dir * len * 0.75,
        root + dir * len,
    ];
    let radii = [
        Vec2::new(width * 0.35, 0.005),
        Vec2::new(width, 0.006),
        Vec2::new(width * 0.9, 0.005),
        Vec2::new(width * 0.3, 0.002),
    ];
    b.tube_framed(
        &path,
        &radii,
        6,
        &|_, _, t| match bars {
            Some(bar) if (t * 9.0).fract() < 0.32 && t > 0.15 => bar,
            _ => color * (1.0 - 0.12 * t),
        },
        Some(across),
    );
}

/// A bat or a kestrel: a small body held aloft, wings spread to beat.
pub(super) fn flier(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let bat = l.style == Style::Bat;
    kit.rig.girth = 0.12;
    let body = kit.joint(J::Body, lift, Vec3::Y * FLIGHT, "body", |p| {
        if bat {
            let chest = l.trim;
            p.matte.blob_painted(
                Vec3::ZERO,
                Vec3::new(0.12, 0.125, 0.17),
                3,
                0.04,
                &|dir, _| {
                    if dir.z > 0.25 && dir.y < 0.55 {
                        chest
                    } else {
                        l.skin * (0.92 + 0.08 * dir.y)
                    }
                },
            );
            // A ruff of fur at the shoulders.
            hair_tufts(
                p,
                Vec3::new(0.0, 0.06, 0.08),
                0.1,
                12,
                Vec3::new(0.0, -0.3, -1.0),
                l.skin * 1.08,
                53,
            );
        } else {
            let (back, breast) = (l.skin, l.cloth);
            p.matte.blob_painted(
                Vec3::ZERO,
                Vec3::new(0.1, 0.11, 0.19),
                3,
                0.0,
                &|dir, at| {
                    let spot = ((at.x * 70.0).sin() * (at.z * 60.0 + at.y * 20.0).cos()) > 0.72;
                    if dir.y < 0.35 && dir.z > -0.6 {
                        if spot {
                            DARK * 3.0
                        } else {
                            breast
                        }
                    } else if ((at.z * 40.0).sin()) > 0.82 {
                        back * 0.45
                    } else {
                        back
                    }
                },
            );
        }
    });
    let neck = kit.joint(J::Neck, body, Vec3::new(0.0, 0.06, 0.14), "neck", |p| {
        let c = if bat { l.skin } else { l.trim };
        p.matte.blob(
            Vec3::new(0.0, 0.02, 0.0),
            Vec3::new(0.07, 0.065, 0.07),
            1,
            0.0,
            c,
        );
    });
    kit.joint(J::Head, neck, Vec3::new(0.0, 0.07, 0.05), "head", |p| {
        if bat {
            bat_head(p, &l);
        } else {
            kestrel_head(p, &l);
        }
    });
    for (side, wing, tip, leg, shin) in [
        (1.0f32, J::WingL, J::WingTipL, J::LegL, J::ShinL),
        (-1.0, J::WingR, J::WingTipR, J::LegR, J::ShinR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let wrist = Vec3::new(side * 0.3, 0.04, -0.03);
        let arm = kit.joint(
            wing,
            body,
            Vec3::new(side * 0.08, 0.05, 0.05),
            &format!("wing{tag}"),
            |p| {
                if bat {
                    bat_arm(p, &l, side, wrist);
                } else {
                    kestrel_arm(p, &l, side, wrist);
                }
            },
        );
        kit.joint(tip, arm, wrist, &format!("tip{tag}"), |p| {
            if bat {
                bat_hand(p, &l, side, wrist);
            } else {
                kestrel_hand(p, &l, side);
            }
        });
        // Small legs trail beneath, clawed or taloned.
        let thigh = kit.joint(
            leg,
            body,
            Vec3::new(side * 0.05, -0.08, -0.08),
            &format!("leg{tag}"),
            |p| {
                let c = if bat { l.skin * 0.8 } else { l.cloth * 0.9 };
                p.matte.blob(
                    Vec3::new(0.0, -0.02, 0.0),
                    Vec3::new(0.035, 0.05, 0.04),
                    1,
                    0.0,
                    c,
                );
            },
        );
        kit.joint(
            shin,
            thigh,
            Vec3::new(0.0, -0.05, -0.02),
            &format!("shin{tag}"),
            |p| {
                let c = if bat {
                    l.skin * 0.55
                } else {
                    Vec3::new(0.95, 0.72, 0.2)
                };
                tube(
                    &mut p.matte,
                    &[Vec3::ZERO, Vec3::new(0.0, -0.06, -0.05)],
                    &[(0.011, 0.011), (0.009, 0.009)],
                    c,
                );
                for k in -1i32..=1 {
                    let toe = Vec3::new(
                        k as f32 * 0.015,
                        -0.085,
                        -0.04 + 0.02 * (1 - k.abs()) as f32,
                    );
                    p.matte.prism(
                        Vec3::new(0.0, -0.06, -0.05),
                        toe,
                        0.006,
                        0.0015,
                        4,
                        DARK * 2.0,
                        0.0,
                    );
                }
            },
        );
    }
    if !bat {
        // A banded fan of tail feathers.
        let tail = l.trim;
        kit.joint(J::Tail, body, Vec3::new(0.0, 0.0, -0.17), "tail", |p| {
            for k in 0..9 {
                let a = (k as f32 - 4.0) * 0.07;
                let dir = Vec3::new(a, -0.05, -1.0).normalize();
                vane(
                    &mut p.matte,
                    Vec3::new(a * 0.1, 0.0, 0.0),
                    dir,
                    0.26,
                    0.03,
                    Vec3::X,
                    tail * (0.92 + 0.08 * rand(103, k)),
                    Some(DARK * 2.5),
                );
            }
        });
    }
}

fn bat_head(p: &mut Parts, l: &Look) {
    let mut skull = Sculpt::default();
    skull
        .add(
            Ellipsoid(Vec3::new(0.0, 0.04, 0.0), Vec3::new(0.085, 0.08, 0.085)),
            0.0,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.01, 0.08), Vec3::new(0.046, 0.04, 0.07)),
            0.03,
        );
    let frames = animal_eye_sockets(
        &mut skull,
        Vec3::new(0.0, 0.06, 0.06),
        0.045,
        0.017,
        Vec3::new(0.6, 0.12, 0.79),
    );
    let fur = l.skin;
    skull.mesh(
        &mut p.matte,
        Vec3::new(-0.11, -0.06, -0.1),
        Vec3::new(0.11, 0.14, 0.17),
        0.005,
        &|at, _| if at.z > 0.11 { fur * 0.7 } else { fur },
    );
    p.matte.blob(
        Vec3::new(0.0, 0.025, 0.15),
        Vec3::new(0.016, 0.012, 0.012),
        0,
        0.0,
        DARK * 1.5,
    );
    animal_eyes(p, frames, 0.017, Vec3::new(0.62, 0.36, 0.12), fur, None);
    // Tall ears, broad at the root, pink within.
    for side in [-1.0f32, 1.0] {
        let root = Vec3::new(side * 0.05, 0.09, -0.01);
        let tip = Vec3::new(side * 0.12, 0.33, -0.04);
        let across = (tip - root).cross(Vec3::Z).normalize_or(Vec3::X);
        let mid = root.lerp(tip, 0.45) + Vec3::X * side * 0.012;
        p.matte.tube_framed(
            &[root, mid, tip],
            &[
                Vec2::new(0.055, 0.012),
                Vec2::new(0.05, 0.01),
                Vec2::splat(0.003),
            ],
            8,
            &|_, _, _| fur,
            Some(across),
        );
        p.matte.tube_framed(
            &[
                root + Vec3::Z * 0.008,
                mid + Vec3::Z * 0.008,
                tip.lerp(root, 0.15) + Vec3::Z * 0.008,
            ],
            &[
                Vec2::new(0.035, 0.005),
                Vec2::new(0.032, 0.005),
                Vec2::splat(0.002),
            ],
            6,
            &|_, _, _| Vec3::new(0.86, 0.5, 0.46),
            Some(across),
        );
        // A small fang.
        p.matte.prism(
            Vec3::new(side * 0.016, -0.005, 0.135),
            Vec3::new(side * 0.017, -0.03, 0.138),
            0.005,
            0.0,
            3,
            BONE,
            0.0,
        );
    }
}

fn kestrel_head(p: &mut Parts, l: &Look) {
    let mut skull = Sculpt::default();
    skull.add(
        Ellipsoid(Vec3::new(0.0, 0.04, 0.0), Vec3::new(0.075, 0.072, 0.08)),
        0.0,
    );
    let frames = animal_eye_sockets(
        &mut skull,
        Vec3::new(0.0, 0.055, 0.04),
        0.05,
        0.019,
        Vec3::new(0.82, 0.1, 0.56),
    );
    let (cap, cheek) = (l.trim, Vec3::new(0.95, 0.92, 0.86));
    skull.mesh(
        &mut p.matte,
        Vec3::new(-0.1, -0.05, -0.1),
        Vec3::new(0.1, 0.13, 0.11),
        0.0045,
        &|at, normal| {
            // A grey cap, pale cheeks and a dark moustache stripe below the eye.
            let stripe = at.x.abs() > 0.035 && at.y < 0.035 && at.y > -0.015 && at.z > 0.0;
            if stripe {
                DARK * 2.5
            } else if normal.y > 0.25 || at.z < -0.03 {
                cap
            } else {
                cheek
            }
        },
    );
    // A yellow cere and a dark hooked beak.
    p.matte.blob(
        Vec3::new(0.0, 0.04, 0.075),
        Vec3::new(0.028, 0.022, 0.02),
        0,
        0.0,
        Vec3::new(0.96, 0.76, 0.22),
    );
    tube(
        &mut p.matte,
        &[
            Vec3::new(0.0, 0.035, 0.088),
            Vec3::new(0.0, 0.03, 0.12),
            Vec3::new(0.0, 0.005, 0.135),
            Vec3::new(0.0, -0.012, 0.125),
        ],
        &[
            (0.018, 0.016),
            (0.013, 0.012),
            (0.007, 0.006),
            (0.002, 0.002),
        ],
        Vec3::new(0.3, 0.32, 0.36),
    );
    animal_eyes(
        p,
        frames,
        0.019,
        DARK * 2.0,
        Vec3::new(0.96, 0.76, 0.22),
        None,
    );
}

/// The bat's arm: a long bony forearm and the skin between it and the flank.
fn bat_arm(p: &mut Parts, l: &Look, side: f32, wrist: Vec3) {
    let bone = l.skin * 0.7;
    tube(
        &mut p.matte,
        &[Vec3::ZERO, wrist * 0.5 + Vec3::Y * 0.02, wrist],
        &[(0.022, 0.022), (0.016, 0.016), (0.013, 0.013)],
        bone,
    );
    let flank = Vec3::new(side * 0.01, -0.05, -0.2);
    let trail = Vec3::new(side * 0.06, -0.04, -0.26);
    membrane(
        &mut p.matte,
        Vec3::ZERO,
        &[wrist, trail + (wrist - trail) * 0.1, trail, flank],
        l.cloth,
    );
}

/// The bat's hand: long fingers fanning from the wrist with skin between.
fn bat_hand(p: &mut Parts, l: &Look, side: f32, wrist: Vec3) {
    let bone = l.skin * 0.7;
    let tips = [
        Vec3::new(side * 0.36, 0.06, 0.03),
        Vec3::new(side * 0.4, -0.02, -0.15),
        Vec3::new(side * 0.28, -0.03, -0.3),
        Vec3::new(side * 0.12, -0.03, -0.34),
    ];
    for tip in tips {
        tube(
            &mut p.matte,
            &[Vec3::ZERO, tip * 0.55 + Vec3::Y * 0.012, tip],
            &[(0.011, 0.011), (0.008, 0.008), (0.004, 0.004)],
            bone,
        );
    }
    // The thumb's hooked claw.
    p.matte.prism(
        Vec3::new(side * 0.01, 0.01, 0.01),
        Vec3::new(side * 0.02, 0.0, 0.05),
        0.008,
        0.0,
        4,
        DARK * 2.0,
        0.0,
    );
    let trail = Vec3::new(side * 0.06, -0.04, -0.26) - wrist;
    let mut rim = tips.to_vec();
    rim.push(trail);
    membrane(&mut p.matte, Vec3::ZERO, &rim, l.cloth);
}

/// The kestrel's arm: rufous spotted coverts over a row of secondaries.
fn kestrel_arm(p: &mut Parts, l: &Look, side: f32, wrist: Vec3) {
    let back = l.skin;
    p.matte.tube_framed(
        &[Vec3::ZERO, wrist * 0.5 + Vec3::new(0.0, 0.015, 0.0), wrist],
        &[
            Vec2::new(0.06, 0.02),
            Vec2::new(0.07, 0.018),
            Vec2::new(0.035, 0.014),
        ],
        8,
        &|_, at, _| {
            if ((at.x * 55.0).sin() * (at.z * 70.0).cos()) > 0.7 {
                DARK * 3.0
            } else {
                back
            }
        },
        Some(Vec3::Z),
    );
    for k in 0..6 {
        let t = (k as f32 + 0.5) / 6.0;
        let root = wrist * t + Vec3::new(0.0, 0.0, -0.03);
        vane(
            &mut p.matte,
            root,
            Vec3::new(side * 0.15, -0.04, -1.0),
            0.14,
            0.035,
            Vec3::X,
            back * 0.85,
            Some(DARK * 3.0),
        );
    }
}

/// The kestrel's hand: long dark primaries fanning out from the wrist.
fn kestrel_hand(p: &mut Parts, l: &Look, side: f32) {
    for k in 0..7 {
        let a = k as f32 / 6.0;
        let dir = Vec3::new(side * (1.0 - a * 0.55), -0.03, 0.15 - a * 1.1).normalize();
        let across = dir.cross(Vec3::Y).normalize_or(Vec3::X);
        vane(
            &mut p.matte,
            Vec3::new(0.0, 0.0, -0.01 * k as f32),
            dir,
            0.22 + 0.04 * (1.0 - (a - 0.3).abs()),
            0.032,
            across,
            Vec3::new(0.3, 0.24, 0.2).lerp(l.skin, 0.25 * a),
            Some(l.cloth * 0.8),
        );
    }
}

// ---------------------------------------------------------------------------
// Four-legged beasts and aberrations. They keep the equine joint layout so
// the shared support solver walks them, with their own leg lengths and feet.

#[derive(Clone, Copy, PartialEq)]
enum Foot {
    /// An eagle's foot: three toes forward, one back, hooked claws.
    Talon,
    /// A lion's soft paw.
    Paw,
    /// A reptile's splayed toes and long dark claws.
    Claw,
    /// Webbed toes tipped with rust.
    Webbed,
    /// A broad stump with blunt nails.
    Stump,
}

#[derive(Clone, Copy)]
struct Leg {
    hip: Vec3,
    upper: f32,
    lower: f32,
    /// Radii at the hip, the knee and the ankle.
    radii: [f32; 3],
    foot: Foot,
}

/// Height of the body joint, and the depth of each foot below its ankle.
struct Stance {
    body: f32,
    depth: f32,
    front: Leg,
    back: Leg,
}

const ANKLE_Z: f32 = 0.02;

fn stance(style: Style) -> Stance {
    let leg = |x: f32, z: f32, upper: f32, lower: f32, radii: [f32; 3], foot: Foot| Leg {
        hip: Vec3::new(x, 0.0, z),
        upper,
        lower,
        radii,
        foot,
    };
    let mut s = match style {
        // Storybook beasts stand on short, heavy legs under big bodies.
        Style::Griffin => Stance {
            body: 0.68,
            depth: 0.07,
            front: leg(0.15, 0.34, 0.26, 0.25, [0.12, 0.07, 0.05], Foot::Talon),
            back: leg(0.16, -0.36, 0.28, 0.26, [0.15, 0.09, 0.07], Foot::Paw),
        },
        Style::Dragon => Stance {
            body: 0.66,
            depth: 0.06,
            front: leg(0.19, 0.3, 0.26, 0.24, [0.13, 0.085, 0.07], Foot::Claw),
            back: leg(0.2, -0.34, 0.28, 0.26, [0.16, 0.095, 0.075], Foot::Claw),
        },
        Style::Jabberwock => Stance {
            body: 0.8,
            depth: 0.06,
            front: leg(0.22, 0.3, 0.46, 0.42, [0.11, 0.075, 0.06], Foot::Claw),
            back: leg(0.18, -0.32, 0.34, 0.32, [0.14, 0.09, 0.07], Foot::Claw),
        },
        Style::Aquator => Stance {
            body: 0.42,
            depth: 0.05,
            front: leg(0.2, 0.22, 0.17, 0.16, [0.075, 0.052, 0.046], Foot::Webbed),
            back: leg(0.21, -0.22, 0.18, 0.16, [0.085, 0.055, 0.048], Foot::Webbed),
        },
        _ => Stance {
            body: 0.38,
            depth: 0.06,
            front: leg(0.2, 0.2, 0.14, 0.14, [0.08, 0.072, 0.07], Foot::Stump),
            back: leg(0.2, -0.2, 0.14, 0.14, [0.085, 0.074, 0.07], Foot::Stump),
        },
    };
    // Each hip sits where its sole meets the floor beneath the body joint.
    for l in [&mut s.front, &mut s.back] {
        l.hip.y = l.upper + l.lower + s.depth - s.body;
    }
    s
}

/// A four-legged Rogue beast on its own body, head, tail and feet.
pub(super) fn beast(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let style = l.style;
    let s = stance(style);
    kit.rig.ankle_floor = s.depth + 0.008;
    kit.rig.legs = (s.front.upper + s.front.lower) / 0.84;
    kit.rig.girth = match style {
        Style::Aquator => 0.33,
        Style::Xeroc => 0.36,
        Style::Dragon => 0.28,
        _ => 0.24,
    };
    let body = kit.joint(J::Body, lift, Vec3::Y * s.body, "body", |p| match style {
        Style::Griffin => griffin_body(p, &l),
        Style::Dragon => dragon_body(p, &l),
        Style::Jabberwock => jabberwock_body(p, &l),
        Style::Aquator => aquator_body(p, &l),
        _ => xeroc_body(p, &l),
    });
    let (neck_at, head_at) = match style {
        Style::Griffin => (Vec3::new(0.0, 0.16, 0.42), Vec3::new(0.0, 0.24, 0.16)),
        Style::Dragon => (Vec3::new(0.0, 0.14, 0.5), Vec3::new(0.0, 0.42, 0.26)),
        Style::Jabberwock => (Vec3::new(0.0, 0.44, 0.34), Vec3::new(0.0, 0.3, 0.3)),
        Style::Aquator => (Vec3::new(0.0, 0.06, 0.3), Vec3::new(0.0, 0.02, 0.06)),
        _ => (Vec3::new(0.0, 0.0, 0.34), Vec3::new(0.0, 0.02, 0.06)),
    };
    let neck = kit.joint(J::Neck, body, neck_at, "neck", |p| match style {
        Style::Griffin => griffin_neck(p, &l, head_at),
        Style::Dragon | Style::Jabberwock => scaled_neck(p, &l, head_at),
        _ => {}
    });
    let head = kit.joint(J::Head, neck, head_at, "head", |p| match style {
        Style::Griffin => griffin_head(p, &l),
        Style::Dragon => dragon_head(p, &l),
        Style::Jabberwock => jabberwock_head(p, &l),
        Style::Aquator => aquator_head(p, &l),
        _ => xeroc_head(p, &l),
    });
    if matches!(
        style,
        Style::Dragon | Style::Jabberwock | Style::Xeroc | Style::Aquator
    ) {
        let jaw_at = match style {
            Style::Xeroc => Vec3::new(0.0, -0.02, 0.0),
            Style::Aquator => Vec3::new(0.0, -0.02, 0.02),
            _ => Vec3::new(0.0, -0.02, 0.02),
        };
        kit.joint(J::Jaw, head, jaw_at, "jaw", |p| beast_jaw(p, &l));
    }
    if style == Style::Aquator {
        aquator_tendrils(kit, &l, head);
    }
    if style != Style::Xeroc {
        kit.joint(J::Tail, body, tail_root(style), "tail", |p| {
            beast_tail(p, &l)
        });
    }
    if matches!(style, Style::Griffin | Style::Dragon) {
        for (side, wing, tip) in [
            (1.0f32, J::WingL, J::WingTipL),
            (-1.0, J::WingR, J::WingTipR),
        ] {
            folded_wing(kit, &l, body, side, wing, tip);
        }
    }
    let legs = [
        (J::FL, J::FLs, J::HoofFL, 1.0f32, true, "fl"),
        (J::FR, J::FRs, J::HoofFR, -1.0, true, "fr"),
        (J::BL, J::BLs, J::HoofBL, 1.0, false, "bl"),
        (J::BR, J::BRs, J::HoofBR, -1.0, false, "br"),
    ];
    for (thigh, shin, foot, side, front, tag) in legs {
        let leg = if front { s.front } else { s.back };
        let hip = Vec3::new(side * leg.hip.x, leg.hip.y, leg.hip.z);
        let (upper_color, lower_color) = leg_colors(&l, front);
        let upper = kit.joint(thigh, body, hip, &format!("leg{tag}"), |p| {
            let [r0, r1, _] = leg.radii;
            p.matte.tube(
                &[
                    Vec3::Y * leg.upper * 0.35,
                    Vec3::new(0.0, -leg.upper * 0.3, 0.012),
                    Vec3::Y * -leg.upper,
                ],
                &[
                    Vec2::splat(r0),
                    Vec2::new(r0 * 0.82, r0 * 0.9),
                    Vec2::splat(r1),
                ],
                12,
                &|_, _, _| upper_color,
            );
            p.matte.blob(
                Vec3::Y * -leg.upper,
                Vec3::splat(r1 * 1.12),
                1,
                0.0,
                upper_color,
            );
        });
        let lower = kit.joint(
            shin,
            upper,
            Vec3::Y * -leg.upper,
            &format!("shin{tag}"),
            |p| {
                let [_, r1, r2] = leg.radii;
                p.matte.tube(
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -leg.lower * 0.5, 0.008),
                        Vec3::new(0.0, -leg.lower, ANKLE_Z),
                    ],
                    &[
                        Vec2::splat(r1),
                        Vec2::splat((r1 + r2) * 0.5),
                        Vec2::splat(r2),
                    ],
                    12,
                    &|_, _, t| upper_color.lerp(lower_color, ease(t * 2.0)),
                );
            },
        );
        kit.joint(
            foot,
            lower,
            Vec3::new(0.0, -leg.lower, ANKLE_Z),
            &format!("foot{tag}"),
            |p| beast_foot(p, &l, leg.foot, s.depth, leg.radii[2], lower_color),
        );
    }
}

fn tail_root(style: Style) -> Vec3 {
    match style {
        Style::Griffin => Vec3::new(0.0, 0.1, -0.52),
        Style::Dragon => Vec3::new(0.0, 0.04, -0.56),
        Style::Jabberwock => Vec3::new(0.0, -0.02, -0.46),
        _ => Vec3::new(0.0, 0.02, -0.38),
    }
}

fn leg_colors(l: &Look, front: bool) -> (Vec3, Vec3) {
    match l.style {
        // Eagle forelegs: feathered above, yellow-brown scaly shanks below.
        Style::Griffin if front => (l.cloth, Vec3::new(0.62, 0.46, 0.2)),
        Style::Xeroc => (l.skin, l.cloth * 0.9),
        _ => (l.skin, l.skin * 0.92),
    }
}

/// Feet below the ankle pivot, toes forward, the sole at `-depth`.
fn beast_foot(p: &mut Parts, l: &Look, foot: Foot, depth: f32, ankle: f32, color: Vec3) {
    let claw = DARK * 2.2;
    let sole = -depth;
    match foot {
        Foot::Talon => {
            p.matte.blob(
                Vec3::Y * sole * 0.6,
                Vec3::splat(ankle * 1.2),
                1,
                0.0,
                color,
            );
            for (x, z) in [
                (-0.045f32, 0.09f32),
                (0.0, 0.11),
                (0.045, 0.09),
                (0.0, -0.06),
            ] {
                let tip = Vec3::new(x, sole + 0.012, z);
                tube(
                    &mut p.matte,
                    &[Vec3::Y * sole * 0.6, tip * 0.6 + Vec3::Y * sole * 0.4, tip],
                    &[(0.022, 0.02), (0.017, 0.015), (0.013, 0.012)],
                    color,
                );
                let out = Vec3::new(x, 0.0, z).normalize_or(Vec3::Z);
                tube(
                    &mut p.matte,
                    &[
                        tip,
                        tip + out * 0.03 + Vec3::Y * 0.004,
                        tip + out * 0.045 - Vec3::Y * 0.014,
                    ],
                    &[(0.01, 0.01), (0.007, 0.007), (0.001, 0.001)],
                    claw,
                );
            }
        }
        Foot::Paw => {
            p.matte.blob(
                Vec3::new(0.0, sole * 0.55, 0.03),
                Vec3::new(0.068, depth * 0.55, 0.085),
                2,
                0.0,
                color,
            );
            for k in 0..4 {
                let x = (k as f32 - 1.5) * 0.03;
                let toe = Vec3::new(x, sole + 0.02, 0.1 - (x.abs() * 0.5));
                p.matte
                    .blob(toe, Vec3::new(0.02, 0.02, 0.024), 0, 0.0, color * 0.95);
                p.matte.prism(
                    toe + Vec3::Z * 0.015,
                    toe + Vec3::new(0.0, -0.012, 0.035),
                    0.006,
                    0.0,
                    4,
                    claw,
                    0.0,
                );
            }
        }
        Foot::Claw | Foot::Webbed => {
            let webbed = foot == Foot::Webbed;
            p.matte.blob(
                Vec3::new(0.0, sole * 0.6, 0.02),
                Vec3::new(ankle * 1.25, depth * 0.5, ankle * 1.5),
                1,
                0.0,
                color,
            );
            let toes: Vec<Vec3> = (0..4)
                .map(|k| {
                    let a = (k as f32 - 1.5) * if webbed { 0.42 } else { 0.32 };
                    Vec3::new(a.sin() * 0.11, sole + 0.012, a.cos() * 0.11 + 0.02)
                })
                .collect();
            for (k, tip) in toes.iter().enumerate() {
                let root = Vec3::new(0.0, sole * 0.6, 0.02);
                tube(
                    &mut p.matte,
                    &[root, root.lerp(*tip, 0.6) + Vec3::Y * 0.01, *tip],
                    &[(0.022, 0.018), (0.017, 0.014), (0.012, 0.01)],
                    color * (0.95 + 0.03 * k as f32),
                );
                let out = (*tip - root).with_y(0.0).normalize_or(Vec3::Z);
                let tip_color = if webbed { l.trim } else { claw };
                tube(
                    &mut p.matte,
                    &[*tip, *tip + out * 0.03, *tip + out * 0.05 - Vec3::Y * 0.012],
                    &[(0.01, 0.009), (0.007, 0.006), (0.001, 0.001)],
                    tip_color,
                );
            }
            if webbed {
                let center = Vec3::new(0.0, sole + 0.008, 0.02);
                membrane(&mut p.matte, center, &toes, l.skin * 0.6);
            }
        }
        Foot::Stump => {
            p.matte.tube(
                &[Vec3::Y * 0.01, Vec3::Y * sole * 0.5, Vec3::Y * sole],
                &[
                    Vec2::splat(ankle),
                    Vec2::splat(ankle * 1.12),
                    Vec2::splat(ankle * 1.2),
                ],
                12,
                &|_, _, _| color,
            );
            for k in 0..3 {
                let a = (k as f32 - 1.0) * 0.5;
                p.matte.blob(
                    Vec3::new(a.sin() * ankle * 1.1, sole + 0.012, a.cos() * ankle * 1.1),
                    Vec3::new(0.018, 0.012, 0.012),
                    0,
                    0.0,
                    BONE * 0.85,
                );
            }
        }
    }
}

/// A row of dorsal spines or frills from `from` to `to`, shortening at both ends.
fn spines(b: &mut Builder, from: Vec3, to: Vec3, count: u32, height: f32, color: Vec3) {
    for k in 0..count {
        let t = (k as f32 + 0.5) / count as f32;
        let root = from.lerp(to, t);
        let h = height * (0.4 + 0.6 * (t * PI).sin());
        let back = (from - to).normalize_or(Vec3::NEG_Z);
        b.tube_framed(
            &[
                root,
                root + Vec3::Y * h * 0.6 - back * h * 0.15,
                root + Vec3::Y * h - back * h * 0.45,
            ],
            &[
                Vec2::new(h * 0.42, 0.012),
                Vec2::new(h * 0.25, 0.008),
                Vec2::splat(0.002),
            ],
            6,
            &|_, _, t| color * (1.0 - 0.2 * t),
            Some(back),
        );
    }
}

/// Scales: a soft repeating variation over the hide, lighter on the belly.
fn scaled(base: Vec3, belly: Vec3, at: Vec3, normal: Vec3) -> Vec3 {
    if normal.y < -0.35 {
        let band = ((at.z * 38.0).sin() * 0.5 + 0.5).powi(4);
        return belly * (0.92 + 0.08 * band);
    }
    let cell = (at.x * 61.0).sin() * (at.y * 57.0 + at.z * 43.0).cos();
    base * (0.93 + 0.08 * cell)
}

fn griffin_body(p: &mut Parts, l: &Look) {
    let mut torso = Sculpt::default();
    torso
        .add(
            Ellipsoid(Vec3::new(0.0, -0.01, -0.05), Vec3::new(0.22, 0.24, 0.46)),
            0.0,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.03, 0.3), Vec3::new(0.24, 0.27, 0.24)),
            0.08,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.02, -0.38), Vec3::new(0.23, 0.24, 0.22)),
            0.07,
        );
    let (lion, feathers) = (l.skin, l.cloth);
    torso.mesh(
        &mut p.matte,
        Vec3::new(-0.3, -0.3, -0.66),
        Vec3::new(0.3, 0.32, 0.56),
        0.012,
        &|at, normal| {
            // Feathers cover the eagle forequarters; the lion begins behind.
            let front = ease((at.z - 0.05) / 0.25);
            let scallop = 0.9 + 0.1 * ((at.z * 46.0).sin() * (at.x * 40.0).cos()).abs();
            let belly = if normal.y < -0.4 { 1.06 } else { 1.0 };
            lion.lerp(feathers * scallop, front) * belly
        },
    );
    // Long breast feathers where the eagle meets the lion.
    for k in 0..14 {
        let a = (k as f32 / 13.0 - 0.5) * 2.4;
        let root = Vec3::new(a.sin() * 0.2, -0.02 + a.cos() * 0.1, 0.24 + a.cos() * 0.08);
        let out = Vec3::new(a.sin() * 0.4, 0.0, -0.4).normalize();
        feather(
            p,
            root,
            out,
            0.14,
            0.05,
            l.cloth * (0.86 + 0.2 * rand(104, k)),
        );
    }
}

fn dragon_body(p: &mut Parts, l: &Look) {
    let mut torso = Sculpt::default();
    torso
        .add(
            Ellipsoid(Vec3::new(0.0, -0.01, -0.06), Vec3::new(0.27, 0.27, 0.52)),
            0.0,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.02, 0.3), Vec3::new(0.25, 0.27, 0.24)),
            0.08,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.0, -0.4), Vec3::new(0.27, 0.27, 0.24)),
            0.08,
        );
    let (red, belly) = (l.skin, l.cloth);
    torso.mesh(
        &mut p.matte,
        Vec3::new(-0.34, -0.32, -0.7),
        Vec3::new(0.34, 0.32, 0.58),
        0.012,
        &|at, normal| scaled(red, belly, at, normal),
    );
    spines(
        &mut p.matte,
        Vec3::new(0.0, 0.25, 0.42),
        Vec3::new(0.0, 0.22, -0.56),
        9,
        0.1,
        l.cloth * 0.9,
    );
}

fn jabberwock_body(p: &mut Parts, l: &Look) {
    // Hips low behind, the chest rearing high in front.
    let mut torso = Sculpt::default();
    torso
        .add(
            Ellipsoid(Vec3::new(0.0, 0.0, -0.28), Vec3::new(0.22, 0.23, 0.27)),
            0.0,
        )
        .add(
            Capsule(
                Vec3::new(0.0, 0.02, -0.2),
                Vec3::new(0.0, 0.3, 0.22),
                0.21,
                0.22,
            ),
            0.08,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.32, 0.26), Vec3::new(0.26, 0.27, 0.22)),
            0.08,
        );
    let (green, belly) = (l.skin, l.cloth);
    torso.mesh(
        &mut p.matte,
        Vec3::new(-0.32, -0.28, -0.6),
        Vec3::new(0.32, 0.62, 0.52),
        0.012,
        &|at, normal| {
            if normal.z > 0.35 && at.z > 0.0 && at.x.abs() < 0.16 {
                belly
            } else {
                scaled(green, belly, at, normal)
            }
        },
    );
    // Orange frills along the spine, and scale-feathers at the shoulders.
    spines(
        &mut p.matte,
        Vec3::new(0.0, 0.54, 0.24),
        Vec3::new(0.0, 0.2, -0.5),
        10,
        0.12,
        l.trim,
    );
    for side in [-1.0f32, 1.0] {
        for k in 0..5 {
            let root = Vec3::new(side * 0.22, 0.4 - k as f32 * 0.05, 0.26 - k as f32 * 0.04);
            leaf(
                &mut p.matte,
                root,
                Vec3::new(side * 0.6, -0.3, -0.7),
                0.1,
                0.04,
                l.trim * (0.85 + 0.25 * rand(105, k)),
            );
        }
    }
}

fn aquator_body(p: &mut Parts, l: &Look) {
    let mut torso = Sculpt::default();
    torso
        .add(
            Ellipsoid(Vec3::new(0.0, 0.04, -0.02), Vec3::new(0.3, 0.24, 0.36)),
            0.0,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.0, 0.22), Vec3::new(0.27, 0.2, 0.18)),
            0.08,
        );
    let (teal, cream, rust) = (l.skin, l.cloth, l.trim);
    torso.mesh(
        &mut p.matte,
        Vec3::new(-0.38, -0.26, -0.44),
        Vec3::new(0.38, 0.32, 0.44),
        0.012,
        &|at, normal| {
            if normal.y < -0.3 || (normal.z > 0.5 && at.y < 0.05) {
                return cream;
            }
            // Rust mottling on the wet teal skin.
            let spot = (at.x * 23.0 + 1.3).sin() * (at.z * 19.0).cos() * (at.y * 17.0 + 0.7).sin();
            if spot > 0.42 {
                rust * 0.95
            } else {
                teal * (0.92 + 0.1 * normal.y)
            }
        },
    );
    // A crest of flexible dorsal gills.
    for k in 0..7 {
        let z = 0.12 - k as f32 * 0.07;
        let root = Vec3::new(0.0, 0.27 - (k as f32 - 2.0).powi(2) * 0.004, z);
        let h = 0.14 - (k as f32 - 2.0).abs() * 0.012;
        p.matte.tube_framed(
            &[
                root,
                root + Vec3::new(0.0, h * 0.6, -0.03),
                root + Vec3::new(0.0, h, -0.09),
            ],
            &[
                Vec2::new(0.035, 0.008),
                Vec2::new(0.03, 0.006),
                Vec2::splat(0.002),
            ],
            6,
            &|_, _, t| rust.lerp(rust * 0.55, t),
            Some(Vec3::Z),
        );
    }
}

fn xeroc_body(p: &mut Parts, l: &Look) {
    // A domed shell of plates over a soft tan underbody.
    let mut shell = Sculpt::default();
    shell
        .add(
            Ellipsoid(Vec3::new(0.0, 0.08, -0.02), Vec3::new(0.34, 0.2, 0.4)),
            0.0,
        )
        .carve(Beyond(Vec3::NEG_Y, 0.02), 0.02);
    let plate = l.skin;
    shell.mesh(
        &mut p.matte,
        Vec3::new(-0.4, -0.05, -0.46),
        Vec3::new(0.4, 0.32, 0.46),
        0.011,
        &|at, _| {
            // Seams between broad plates.
            let u = (at.x * 9.0)
                .sin()
                .abs()
                .min((at.z * 8.0 + at.x * 3.0).sin().abs());
            if u < 0.12 {
                plate * 0.55
            } else {
                plate * (0.95 + 0.08 * (at.z * 13.0).sin())
            }
        },
    );
    p.matte.blob(
        Vec3::new(0.0, -0.03, 0.0),
        Vec3::new(0.27, 0.1, 0.32),
        2,
        0.0,
        l.cloth,
    );
    // The rim of the shell overhangs the body.
    p.matte.torus(
        Vec3::new(0.0, 0.0, -0.02),
        Vec3::Y,
        0.3,
        0.035,
        28,
        6,
        plate * 0.8,
    );
}

fn griffin_neck(p: &mut Parts, l: &Look, head: Vec3) {
    p.matte.tube(
        &[
            Vec3::new(0.0, -0.06, -0.06),
            head * 0.5 + Vec3::Z * 0.03,
            head,
        ],
        &[
            Vec2::new(0.15, 0.17),
            Vec2::new(0.1, 0.11),
            Vec2::new(0.085, 0.09),
        ],
        12,
        &|_, _, _| l.cloth,
    );
    for k in 0..10 {
        let a = k as f32 / 10.0 * TAU;
        let root = head * 0.45 + Vec3::new(a.sin() * 0.1, 0.0, a.cos() * 0.1);
        feather(
            p,
            root,
            Vec3::new(a.sin(), 0.0, a.cos()),
            0.12,
            0.05,
            l.cloth * (0.9 + 0.15 * rand(106, k)),
        );
    }
}

fn scaled_neck(p: &mut Parts, l: &Look, head: Vec3) {
    let (skin, belly) = (l.skin, l.cloth);
    p.matte.tube(
        &[
            Vec3::new(0.0, -0.08, -0.08),
            Vec3::new(0.0, head.y * 0.45, head.z * 0.25),
            Vec3::new(0.0, head.y * 0.85, head.z * 0.7),
            head,
        ],
        &[
            Vec2::new(0.16, 0.17),
            Vec2::new(0.12, 0.12),
            Vec2::new(0.095, 0.1),
            Vec2::new(0.085, 0.09),
        ],
        12,
        &|dir, at, _| {
            if dir.z > 0.45 || dir.y < -0.5 {
                belly
            } else {
                scaled(skin, belly, at, dir)
            }
        },
    );
    let crest = if l.style == Style::Jabberwock {
        l.trim
    } else {
        l.cloth * 0.9
    };
    spines(
        &mut p.matte,
        Vec3::new(0.0, head.y * 0.3, -0.08),
        head + Vec3::new(0.0, 0.08, -0.08),
        6,
        0.08,
        crest,
    );
}

fn griffin_head(p: &mut Parts, l: &Look) {
    let mut skull = Sculpt::default();
    skull
        .add(
            Ellipsoid(Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.1, 0.1, 0.11)),
            0.0,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.1, 0.06), Vec3::new(0.08, 0.025, 0.05)),
            0.03,
        );
    let frames = animal_eye_sockets(
        &mut skull,
        Vec3::new(0.0, 0.08, 0.06),
        0.06,
        0.02,
        Vec3::new(0.75, 0.15, 0.64),
    );
    let (cream, crown) = (l.cloth, l.skin);
    skull.mesh(
        &mut p.matte,
        Vec3::new(-0.13, -0.07, -0.13),
        Vec3::new(0.13, 0.17, 0.14),
        0.005,
        &|_, normal| if normal.y > 0.55 { crown } else { cream },
    );
    // A yellow cere and a great hooked beak.
    p.matte.blob(
        Vec3::new(0.0, 0.06, 0.1),
        Vec3::new(0.04, 0.032, 0.028),
        0,
        0.0,
        Vec3::new(0.95, 0.75, 0.22),
    );
    tube(
        &mut p.matte,
        &[
            Vec3::new(0.0, 0.06, 0.11),
            Vec3::new(0.0, 0.055, 0.17),
            Vec3::new(0.0, 0.02, 0.21),
            Vec3::new(0.0, -0.025, 0.19),
        ],
        &[(0.034, 0.03), (0.026, 0.022), (0.012, 0.01), (0.002, 0.002)],
        Vec3::new(0.95, 0.75, 0.22),
    );
    p.matte.prism(
        Vec3::new(0.0, 0.01, 0.2),
        Vec3::new(0.0, -0.025, 0.19),
        0.01,
        0.0,
        4,
        DARK * 2.0,
        0.0,
    );
    animal_eyes(p, frames, 0.02, Vec3::new(0.85, 0.55, 0.12), cream, None);
    // Swept-back crest feathers.
    for k in 0..7 {
        let x = (k as f32 - 3.0) * 0.02;
        feather(
            p,
            Vec3::new(x, 0.13, -0.04),
            Vec3::new(x * 2.0, 0.6, -1.0).normalize(),
            0.12,
            0.04,
            crown * (0.9 + 0.12 * rand(107, k)),
        );
    }
}

/// Horned reptile skulls share a snout, brow and teeth; the jaw is its own joint.
fn reptile_skull(p: &mut Parts, l: &Look, length: f32) -> Sculpt {
    let mut skull = Sculpt::default();
    skull
        .add(
            Ellipsoid(Vec3::new(0.0, 0.05, 0.0), Vec3::new(0.1, 0.09, 0.11)),
            0.0,
        )
        .add(
            Ellipsoid(
                Vec3::new(0.0, 0.03, 0.15 * length),
                Vec3::new(0.07, 0.055, 0.13 * length),
            ),
            0.05,
        );
    for side in [-1.0f32, 1.0] {
        skull.add(
            Ellipsoid(
                Vec3::new(side * 0.055, 0.11, 0.06),
                Vec3::new(0.035, 0.02, 0.05),
            ),
            0.02,
        );
    }
    let frames = animal_eye_sockets(
        &mut skull,
        Vec3::new(0.0, 0.09, 0.07),
        0.06,
        0.019,
        Vec3::new(0.85, 0.2, 0.48),
    );
    let (skin, belly) = (l.skin, l.cloth);
    skull.mesh(
        &mut p.matte,
        Vec3::new(-0.14, -0.06, -0.14),
        Vec3::new(0.14, 0.17, 0.3 * length + 0.04),
        0.005,
        &|at, normal| scaled(skin, belly, at, normal),
    );
    for side in [-1.0f32, 1.0] {
        p.matte.blob(
            Vec3::new(side * 0.025, 0.06, 0.27 * length),
            Vec3::new(0.008, 0.006, 0.006),
            0,
            0.0,
            DARK,
        );
        // Teeth along the upper jaw.
        for k in 0..4 {
            let z = 0.08 + k as f32 * 0.045 * length;
            p.matte.prism(
                Vec3::new(side * 0.05, -0.0, z),
                Vec3::new(side * 0.048, -0.035, z + 0.005),
                0.009,
                0.0,
                3,
                BONE,
                0.0,
            );
        }
    }
    animal_eyes(p, frames, 0.019, Vec3::new(0.95, 0.7, 0.12), skin, None);
    skull
}

fn dragon_head(p: &mut Parts, l: &Look) {
    reptile_skull(p, l, 1.0);
    let horn = Vec3::new(0.36, 0.24, 0.16);
    for side in [-1.0f32, 1.0] {
        tube(
            &mut p.matte,
            &[
                Vec3::new(side * 0.06, 0.11, -0.03),
                Vec3::new(side * 0.1, 0.17, -0.12),
                Vec3::new(side * 0.12, 0.2, -0.24),
                Vec3::new(side * 0.11, 0.25, -0.31),
            ],
            &[(0.03, 0.03), (0.024, 0.024), (0.014, 0.014), (0.002, 0.002)],
            horn,
        );
        // Cheek spikes fanning back from the jaw.
        for k in 0..3 {
            let root = Vec3::new(side * 0.085, 0.04 - k as f32 * 0.025, -0.02);
            p.matte.prism(
                root,
                root + Vec3::new(side * 0.06, 0.01, -0.07),
                0.016,
                0.0,
                4,
                l.cloth * 0.9,
                0.0,
            );
        }
    }
}

fn jabberwock_head(p: &mut Parts, l: &Look) {
    reptile_skull(p, l, 1.25);
    let antler = Vec3::new(0.4, 0.26, 0.16);
    for side in [-1.0f32, 1.0] {
        // Branching antler-horns.
        let base = Vec3::new(side * 0.06, 0.12, -0.03);
        let fork = base + Vec3::new(side * 0.08, 0.12, -0.1);
        tube(
            &mut p.matte,
            &[base, fork, fork + Vec3::new(side * 0.05, 0.1, -0.12)],
            &[(0.024, 0.024), (0.018, 0.018), (0.003, 0.003)],
            antler,
        );
        tube(
            &mut p.matte,
            &[
                fork,
                fork + Vec3::new(side * 0.1, 0.03, 0.02),
                fork + Vec3::new(side * 0.15, 0.08, 0.06),
            ],
            &[(0.016, 0.016), (0.012, 0.012), (0.002, 0.002)],
            antler,
        );
        // An orange frill around the face.
        for k in 0..6 {
            let a = k as f32 / 5.0;
            let root = Vec3::new(side * 0.09, 0.12 - a * 0.14, 0.02 - a * 0.04);
            leaf(
                &mut p.matte,
                root,
                Vec3::new(side * 0.8, 0.2 - a * 0.6, -0.5),
                0.1,
                0.04,
                l.trim * (0.85 + 0.3 * rand(108, k)),
            );
        }
    }
    // Two buck teeth jutting from the snout.
    for side in [-1.0f32, 1.0] {
        p.matte.prism(
            Vec3::new(side * 0.02, 0.0, 0.3),
            Vec3::new(side * 0.022, -0.06, 0.31),
            0.014,
            0.0,
            4,
            BONE,
            0.0,
        );
    }
}

fn aquator_head(p: &mut Parts, l: &Look) {
    // A broad flat amphibian head with small black eyes set on top.
    let mut skull = Sculpt::default();
    skull.add(
        Ellipsoid(Vec3::new(0.0, 0.04, 0.04), Vec3::new(0.22, 0.13, 0.15)),
        0.0,
    );
    let (teal, cream) = (l.skin, l.cloth);
    skull.mesh(
        &mut p.matte,
        Vec3::new(-0.26, -0.11, -0.13),
        Vec3::new(0.26, 0.19, 0.22),
        0.008,
        &|at, normal| {
            if normal.y < -0.1 && at.z > 0.0 {
                cream
            } else {
                teal
            }
        },
    );
    for side in [-1.0f32, 1.0] {
        let at = Vec3::new(side * 0.1, 0.15, 0.08);
        p.matte.blob(at, Vec3::splat(0.04), 1, 0.0, teal);
        p.satin.blob(
            at + Vec3::new(side * 0.012, 0.012, 0.02),
            Vec3::splat(0.027),
            0,
            0.0,
            DARK * 1.2,
        );
        p.satin.blob(
            at + Vec3::new(side * 0.02, 0.03, 0.035),
            Vec3::splat(0.007),
            0,
            0.0,
            Vec3::splat(1.5),
        );
    }
}

fn xeroc_head(p: &mut Parts, l: &Look) {
    // A wide skull that is mostly mouth, with yellow slit eyes on top.
    let mut skull = Sculpt::default();
    skull
        .add(
            Ellipsoid(Vec3::new(0.0, 0.06, 0.04), Vec3::new(0.17, 0.1, 0.15)),
            0.0,
        )
        .carve(
            Ellipsoid(Vec3::new(0.0, 0.0, 0.14), Vec3::new(0.13, 0.05, 0.1)),
            0.02,
        );
    let frames = animal_eye_sockets(
        &mut skull,
        Vec3::new(0.0, 0.13, 0.06),
        0.09,
        0.024,
        Vec3::new(0.5, 0.7, 0.5),
    );
    let (plate, mouth) = (l.skin, l.trim);
    skull.mesh(
        &mut p.matte,
        Vec3::new(-0.2, -0.04, -0.12),
        Vec3::new(0.2, 0.18, 0.21),
        0.006,
        &|at, normal| {
            if normal.y < -0.3 && at.z > 0.05 {
                mouth
            } else {
                plate
            }
        },
    );
    // A ring of fangs along the upper lip.
    for k in 0..9 {
        let a = (k as f32 / 8.0 - 0.5) * 2.4;
        let root = Vec3::new(a.sin() * 0.13, 0.03, 0.06 + a.cos() * 0.11);
        p.matte.prism(
            root,
            root + Vec3::new(0.0, -0.045, 0.006),
            0.01,
            0.0,
            4,
            BONE,
            0.0,
        );
    }
    animal_eyes(p, frames, 0.024, Vec3::new(0.96, 0.78, 0.12), plate, None);
}

/// A lower jaw on its own joint, opened by bites, breath and gaping idles.
fn beast_jaw(p: &mut Parts, l: &Look) {
    let (skin, belly, mouth) = (l.skin, l.cloth, l.trim);
    match l.style {
        Style::Xeroc => {
            p.matte.blob(
                Vec3::new(0.0, -0.02, 0.1),
                Vec3::new(0.15, 0.04, 0.13),
                2,
                0.0,
                skin * 0.9,
            );
            p.matte.blob(
                Vec3::new(0.0, 0.0, 0.1),
                Vec3::new(0.12, 0.02, 0.1),
                1,
                0.0,
                mouth,
            );
            for k in 0..8 {
                let a = (k as f32 / 7.0 - 0.5) * 2.2;
                let root = Vec3::new(a.sin() * 0.12, 0.0, 0.08 + a.cos() * 0.1);
                p.matte.prism(
                    root,
                    root + Vec3::new(0.0, 0.04, 0.0),
                    0.009,
                    0.0,
                    4,
                    BONE,
                    0.0,
                );
            }
        }
        Style::Aquator => {
            p.matte.blob(
                Vec3::new(0.0, -0.03, 0.06),
                Vec3::new(0.2, 0.05, 0.13),
                2,
                0.0,
                belly,
            );
            p.matte.blob(
                Vec3::new(0.0, -0.01, 0.07),
                Vec3::new(0.16, 0.02, 0.1),
                1,
                0.0,
                Vec3::new(0.72, 0.38, 0.38),
            );
        }
        _ => {
            let length = if l.style == Style::Jabberwock {
                1.25
            } else {
                1.0
            };
            p.matte.tube(
                &[
                    Vec3::new(0.0, -0.01, -0.02),
                    Vec3::new(0.0, -0.02, 0.14 * length),
                    Vec3::new(0.0, -0.01, 0.26 * length),
                ],
                &[
                    Vec2::new(0.07, 0.035),
                    Vec2::new(0.06, 0.03),
                    Vec2::new(0.035, 0.02),
                ],
                10,
                &|dir, _, _| if dir.y < 0.0 { belly } else { skin },
            );
            p.matte.blob(
                Vec3::new(0.0, 0.005, 0.12 * length),
                Vec3::new(0.045, 0.012, 0.1 * length),
                1,
                0.0,
                Vec3::new(0.74, 0.3, 0.32),
            );
            for side in [-1.0f32, 1.0] {
                for k in 0..4 {
                    let z = 0.07 + k as f32 * 0.045 * length;
                    p.matte.prism(
                        Vec3::new(side * 0.045, 0.0, z),
                        Vec3::new(side * 0.043, 0.03, z),
                        0.008,
                        0.0,
                        3,
                        BONE,
                        0.0,
                    );
                }
            }
        }
    }
}

fn beast_tail(p: &mut Parts, l: &Look) {
    match l.style {
        Style::Griffin => {
            tube(
                &mut p.matte,
                &[
                    Vec3::ZERO,
                    Vec3::new(0.0, -0.2, -0.2),
                    Vec3::new(0.0, -0.32, -0.45),
                    Vec3::new(0.0, -0.18, -0.68),
                ],
                &[(0.05, 0.05), (0.04, 0.04), (0.032, 0.032), (0.03, 0.03)],
                l.skin,
            );
            p.matte.blob(
                Vec3::new(0.0, -0.14, -0.74),
                Vec3::new(0.08, 0.1, 0.1),
                3,
                0.15,
                l.cloth,
            );
            hair_tufts(
                p,
                Vec3::new(0.0, -0.14, -0.74),
                0.09,
                10,
                Vec3::new(0.0, 0.4, -1.0),
                l.cloth,
                109,
            );
        }
        Style::Dragon | Style::Jabberwock => {
            let long = if l.style == Style::Dragon { 1.0 } else { 0.9 };
            let path = [
                Vec3::ZERO,
                Vec3::new(0.0, -0.1, -0.35) * long,
                Vec3::new(0.0, -0.2, -0.7) * long,
                Vec3::new(0.06, -0.14, -1.0) * long,
                Vec3::new(0.12, -0.04, -1.2) * long,
            ];
            let (skin, belly) = (l.skin, l.cloth);
            p.matte.tube(
                &path,
                &[
                    Vec2::splat(0.14),
                    Vec2::splat(0.11),
                    Vec2::splat(0.075),
                    Vec2::splat(0.04),
                    Vec2::splat(0.008),
                ],
                12,
                &|dir, at, _| scaled(skin, belly, at, dir),
            );
            let crest = if l.style == Style::Jabberwock {
                l.trim
            } else {
                l.cloth * 0.9
            };
            for pair in path.windows(2) {
                spines(
                    &mut p.matte,
                    pair[0] + Vec3::Y * 0.1,
                    pair[1] + Vec3::Y * 0.06,
                    3,
                    0.08,
                    crest,
                );
            }
            if l.style == Style::Dragon {
                // A spade at the tip.
                p.matte.extrude(
                    &[
                        Vec2::new(0.0, 0.0),
                        Vec2::new(0.07, -0.06),
                        Vec2::new(0.0, -0.16),
                        Vec2::new(-0.07, -0.06),
                    ],
                    path[4],
                    Vec3::X,
                    Vec3::Z,
                    0.02,
                    l.trim,
                );
            }
        }
        _ => {
            // A short tapering amphibian tail.
            tube(
                &mut p.matte,
                &[
                    Vec3::ZERO,
                    Vec3::new(0.0, -0.04, -0.14),
                    Vec3::new(0.0, -0.08, -0.26),
                ],
                &[(0.12, 0.09), (0.07, 0.05), (0.01, 0.01)],
                l.skin,
            );
        }
    }
}

/// A wing folded along the back; its tip rides on its own joint at the wrist.
fn folded_wing(kit: &mut Kit, l: &Look, body: Entity, side: f32, wing: J, tip: J) {
    let dragon = l.style == Style::Dragon;
    let root = Vec3::new(side * 0.15, 0.21, 0.2);
    let wrist = if dragon {
        Vec3::new(side * 0.06, 0.34, -0.2)
    } else {
        Vec3::new(side * 0.1, 0.1, -0.24)
    };
    let trail = Vec3::new(side * 0.19, -0.12, -0.05);
    let tag = if side > 0.0 { "l" } else { "r" };
    let (bone, skin, coverts, quills) = (l.skin, l.trim, l.cloth, l.trim);
    let arm = kit.joint(wing, body, root, &format!("wing{tag}"), |p| {
        tube(
            &mut p.matte,
            &[Vec3::ZERO, wrist * 0.5 + Vec3::X * side * 0.03, wrist],
            &[(0.045, 0.045), (0.035, 0.035), (0.03, 0.03)],
            if dragon { bone } else { coverts },
        );
        if dragon {
            membrane(&mut p.matte, Vec3::ZERO, &[wrist, trail], skin);
            p.matte.prism(
                wrist,
                wrist + Vec3::new(0.0, 0.08, 0.04),
                0.018,
                0.0,
                4,
                BONE,
                0.0,
            );
        } else {
            // Coverts lie over the arm; secondaries hang behind it.
            p.matte.tube_framed(
                &[Vec3::ZERO, wrist * 0.55, wrist],
                &[
                    Vec2::new(0.1, 0.03),
                    Vec2::new(0.11, 0.03),
                    Vec2::new(0.06, 0.02),
                ],
                8,
                &|_, _, t| coverts * (0.95 + 0.06 * t),
                Some(Vec3::Y),
            );
            for k in 0..6 {
                let t = (k as f32 + 0.5) / 6.0;
                let r = wrist * t + Vec3::X * side * 0.03;
                vane(
                    &mut p.matte,
                    r,
                    Vec3::new(side * 0.45, -0.55, -0.7),
                    0.28,
                    0.07,
                    Vec3::new(0.0, 0.6, -0.8).normalize(),
                    coverts.lerp(quills, 0.4),
                    None,
                );
            }
        }
    });
    let trail_from_wrist = trail - wrist;
    kit.joint(tip, arm, wrist, &format!("tip{tag}"), |p| {
        let tips = [
            Vec3::new(side * 0.02, 0.1, -0.46),
            Vec3::new(side * 0.07, -0.14, -0.6),
            Vec3::new(side * 0.13, -0.32, -0.5),
            Vec3::new(side * 0.17, -0.42, -0.26),
        ];
        if dragon {
            for t in tips {
                tube(
                    &mut p.matte,
                    &[Vec3::ZERO, t * 0.6, t],
                    &[(0.022, 0.022), (0.016, 0.016), (0.005, 0.005)],
                    bone,
                );
            }
            let mut rim = tips.to_vec();
            rim.push(trail_from_wrist);
            membrane(&mut p.matte, Vec3::ZERO, &rim, skin);
        } else {
            // Primaries lie back along the flank, overlapping like shingles.
            for k in 0..9 {
                let a = k as f32 / 8.0;
                let dir = Vec3::new(side * (0.18 + 0.2 * a), -0.12 - 0.35 * a, -1.0).normalize();
                let across = Vec3::new(0.0, 1.0, 0.25 * side).normalize();
                vane(
                    &mut p.matte,
                    Vec3::new(side * 0.01 * k as f32, -0.025 * k as f32, 0.0),
                    dir,
                    0.58 - 0.16 * a,
                    0.075,
                    across,
                    quills * (0.9 + 0.15 * rand(110, k)),
                    None,
                );
            }
        }
    });
}

/// Two long sensing tendrils hang from the aquator's cheeks, each a chain of
/// posable segments ending in a knobbly, rust-encrusted tip.
fn aquator_tendrils(kit: &mut Kit, l: &Look, head: Entity) {
    const STEP: Vec3 = Vec3::new(0.0, -0.05, 0.07);
    for side in [-1.0f32, 1.0] {
        let mut parent = head;
        let mut at = Vec3::new(side * 0.19, 0.0, 0.1);
        for k in 0..4 {
            let last = k == 3;
            let (teal, rust) = (l.skin, l.trim);
            let r = 0.026 - k as f32 * 0.003;
            let model = kit.part(&format!("tendril{k}"), |p| {
                tube(
                    &mut p.matte,
                    &[Vec3::ZERO, STEP * 0.5, STEP],
                    &[(r, r), (r * 0.95, r * 0.95), (r * 0.9, r * 0.9)],
                    teal,
                );
                p.matte.blob(Vec3::ZERO, Vec3::splat(r * 1.1), 0, 0.0, teal);
                if last {
                    for n in 0..9 {
                        let a = n as f32 * 2.4;
                        let off =
                            Vec3::new(a.cos() * 0.025, -0.01 * n as f32, 0.04 + a.sin() * 0.02);
                        p.matte.blob(
                            STEP + off,
                            Vec3::splat(0.024),
                            n as u64,
                            0.25,
                            rust * (0.85 + 0.25 * rand(111, n)),
                        );
                    }
                }
            });
            let e = kit
                .commands
                .spawn((Transform::from_translation(at), Visibility::default()))
                .id();
            kit.commands.entity(e).with_children(|p| model.attach(p));
            kit.commands.entity(parent).add_child(e);
            kit.rig.segments.push(e);
            parent = e;
            at = STEP;
        }
    }
}

// ---------------------------------------------------------------------------
// The Venus flytrap: rooted, with a two-jointed stem and a snapping trap.

pub(super) fn flytrap(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    kit.rig.girth = 0.3;
    let base = kit.joint(J::Body, lift, Vec3::ZERO, "base", |p| {
        // Roots spread over the floor beneath a rosette of broad leaves.
        let soil = Vec3::new(0.32, 0.22, 0.14);
        p.matte.blob(
            Vec3::new(0.0, 0.02, 0.0),
            Vec3::new(0.26, 0.05, 0.26),
            3,
            0.3,
            soil,
        );
        for k in 0..9 {
            let a = k as f32 / 9.0 * TAU + rand(112, k) * 0.4;
            let out = Vec3::new(a.sin(), 0.0, a.cos());
            tube(
                &mut p.matte,
                &[
                    out * 0.06 + Vec3::Y * 0.04,
                    out * 0.2 + Vec3::Y * 0.02,
                    out * 0.34 + Vec3::Y * 0.01,
                ],
                &[(0.022, 0.022), (0.014, 0.014), (0.004, 0.004)],
                Vec3::new(0.44, 0.3, 0.2),
            );
        }
        for k in 0..8 {
            let a = (k as f32 + 0.5) / 8.0 * TAU;
            let dir = Vec3::new(a.sin(), 0.55 + 0.2 * rand(113, k), a.cos());
            let green = l.skin * (0.85 + 0.25 * rand(114, k));
            leaf(&mut p.matte, Vec3::Y * 0.06, dir, 0.38, 0.11, green);
            // A red rim along each leaf.
            leaf(
                &mut p.matte,
                Vec3::Y * 0.065,
                dir + Vec3::Y * 0.05,
                0.34,
                0.03,
                l.cloth * 0.8,
            );
        }
    });
    let lower = kit.joint(J::Neck, base, Vec3::Y * 0.08, "stem", |p| {
        stem(p, &l, Vec3::new(0.0, 0.38, 0.04));
    });
    let upper = kit.joint(J::Torso, lower, Vec3::new(0.0, 0.38, 0.04), "stalk", |p| {
        stem(p, &l, Vec3::new(0.0, 0.34, 0.1));
    });
    let trap = kit.joint(J::Head, upper, Vec3::new(0.0, 0.34, 0.1), "trap", |p| {
        trap_lobe(p, &l, 1.0);
    });
    kit.joint(J::Jaw, trap, Vec3::ZERO, "lobe", |p| trap_lobe(p, &l, -1.0));
}

fn stem(p: &mut Parts, l: &Look, top: Vec3) {
    p.matte.tube(
        &[Vec3::ZERO, top * 0.5 + Vec3::Z * 0.02, top],
        &[Vec2::splat(0.05), Vec2::splat(0.044), Vec2::splat(0.04)],
        10,
        &|_, _, t| l.skin * (0.88 + 0.12 * (t * 9.0).sin().abs()),
    );
    // Small leaves sprout from the stem.
    for side in [-1.0f32, 1.0] {
        leaf(
            &mut p.matte,
            top * 0.55,
            Vec3::new(side, 0.4, 0.2),
            0.12,
            0.04,
            l.skin * 0.95,
        );
    }
}

/// One lobe of the trap: `up` 1 for the upper lobe, -1 for the lower.
fn trap_lobe(p: &mut Parts, l: &Look, up: f32) {
    // Each lobe is half a shell hinged at the back, opening forward.
    let mut lobe = Sculpt::default();
    lobe.add(
        Ellipsoid(Vec3::new(0.0, 0.0, 0.14), Vec3::new(0.15, 0.09, 0.15)),
        0.0,
    )
    .carve(Beyond(Vec3::NEG_Y * up, 0.0), 0.0)
    .carve(
        Ellipsoid(Vec3::new(0.0, 0.0, 0.15), Vec3::new(0.12, 0.06, 0.12)),
        0.01,
    );
    let (green, red) = (l.skin, l.cloth);
    lobe.mesh(
        &mut p.matte,
        Vec3::new(-0.17, -0.1, -0.02),
        Vec3::new(0.17, 0.1, 0.31),
        0.006,
        &|at, normal| {
            let inside = normal.y * up < -0.2 && (at.y * up) < 0.05;
            if inside {
                red
            } else {
                green * (0.9 + 0.1 * ((at.x * 40.0).sin() * (at.z * 35.0).cos()))
            }
        },
    );
    // Teeth along the rim interlock when the trap closes.
    for k in 0..13 {
        let a = (k as f32 / 12.0 - 0.5) * 2.9 + if up > 0.0 { 0.0 } else { 0.12 };
        let rim = Vec3::new(a.sin() * 0.15, 0.0, 0.14 + a.cos() * 0.15);
        let out = Vec3::new(a.sin(), 0.0, a.cos());
        p.matte.prism(
            rim,
            rim + out * 0.03 - Vec3::Y * up * 0.07,
            0.008,
            0.0,
            4,
            l.trim,
            0.0,
        );
    }
}
