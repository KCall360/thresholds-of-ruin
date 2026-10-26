//! Portable attached garment details, fitted to the resolved wearer.
use super::ornament_parts::leaf;
use super::*;
use crate::creatures::outfit::{GarmentDetails, HipDecoration};

pub(super) fn spawn(kit: &mut Kit, details: GarmentDetails, body: HumanoidBody, hips: Entity) {
    let b = body.build;
    let pelvis = body.pelvis_radii();
    match details.hip {
        Some(HipDecoration::Loincloth {
            cloth: red,
            edge,
            ring,
        }) => {
            // A red loincloth front and back, hanging from the belt.
            kit.drape(
                hips,
                Transform::IDENTITY,
                Drape::Skirt(0.4),
                "loincloth",
                |p| {
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
                            edge,
                        );
                    }
                    p.metal.torus(
                        Vec3::new(0.0, 0.07, pelvis.z + 0.045),
                        Vec3::Z,
                        0.035,
                        0.01,
                        14,
                        4,
                        ring,
                    );
                },
            );
        }
        Some(HipDecoration::RaggedApron { cloth: kilt, apron }) => {
            // Ragged strips of the kilt and a hanging leather apron.
            kit.drape(hips, Transform::IDENTITY, Drape::Skirt(0.3), "rags", |p| {
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
                    apron,
                );
            });
        }
        Some(HipDecoration::BucklePouch { metal, pouch }) => {
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
                    metal,
                );
                p.metal.cuboid(
                    Vec3::new(0.0, 0.07, front),
                    Vec3::new(0.006, 0.03, 0.006),
                    Quat::IDENTITY,
                    metal,
                );
                p.satin.cuboid(
                    Vec3::new(0.11 * b, 0.0, front - 0.02),
                    Vec3::new(0.035, 0.045, 0.025),
                    Quat::from_rotation_y(0.4),
                    pouch,
                );
            });
        }
        Some(HipDecoration::Leaves { cloth: green, trim }) => {
            // Two layers of long leaves over the short skirt.
            kit.drape(
                hips,
                Transform::IDENTITY,
                Drape::Skirt(0.25),
                "leaves",
                |p| {
                    // Each leaf is pinned to the sculpted hips it hangs from.
                    let form = super::pelvis_shape(body);
                    for layer in 0..2 {
                        let count = 13 + layer * 3;
                        for k in 0..count {
                            let a = (k as f32 + layer as f32 * 0.5) / count as f32 * TAU;
                            let out = Vec3::new(a.sin(), 0.0, a.cos());
                            let root =
                                super::pelvis_surface(&form, 0.04 - layer as f32 * 0.09, out)
                                    + out * 0.006;
                            let dir = (Vec3::NEG_Y + out * 0.35).normalize();
                            let shade = 0.8 + 0.35 * rand(99, k + layer * 40);
                            leaf(
                                &mut p.matte,
                                root,
                                dir,
                                0.16 + 0.05 * rand(100, k),
                                0.045,
                                green.lerp(trim, 0.25 * layer as f32) * shade,
                            );
                        }
                    }
                },
            );
        }
        Some(HipDecoration::TornHem { length, cloth }) => {
            // Torn hems below the robe-skirt, and rags from the sleeves.
            let hem = Vec2::new(
                (0.208 + length * 0.045) * b * body.hips_width,
                (0.208 + length * 0.045) * 0.74 * b,
            );
            kit.drape(
                hips,
                Transform::IDENTITY,
                Drape::Skirt(length + 0.24),
                "tatters",
                |p| {
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
                },
            );
        }
        None => {}
    }
    if let Some(trim) = details.shoulder_discs {
        // Bronze discs on the shoulders.
        for (arm, side) in [(J::ArmL, 1.0f32), (J::ArmR, -1.0)] {
            if let Some(joint) = kit.rig.joint(arm) {
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
    }

    if let Some(sleeve) = details.sleeve_tatters {
        for (fore, side) in [(J::ForeL, 1.0f32), (J::ForeR, -1.0)] {
            if let Some(joint) = kit.rig.joint(fore) {
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
    }
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
