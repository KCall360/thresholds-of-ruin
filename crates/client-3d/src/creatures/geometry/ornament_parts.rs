//! Sewn torso details fitted from garment recipes, independent of species.
use super::*;
use crate::sdf::Sculpt;

pub(super) fn emit(
    p: &mut Parts,
    ornament: TorsoOrnament,
    body: HumanoidBody,
    garment: &Sculpt,
    skin: &Sculpt,
    neckline: Option<NecklineCut>,
) {
    let b = body.build;
    let (sh, w) = (body.shoulders * b, body.waist * b);
    let on = |x, y, front| torso_surface(garment, x, y, front);
    match ornament {
        TorsoOrnament::LacedFront { cord } => {
            // Lacing up the front of the jerkin and a strap across it.
            for k in 0..4 {
                let height = 0.1 + k as f32 * 0.08;
                fitted_ribbon(
                    &mut p.matte,
                    garment,
                    &[
                        (Vec2::new(-0.021 * b, height - 0.014), 0.004),
                        (Vec2::new(0.021 * b, height + 0.014), 0.004),
                    ],
                    true,
                    0.012,
                    cord,
                );
            }
        }
        TorsoOrnament::Placket { color } => {
            // The placket rises to the collar, or to a point beneath
            // her plunge.
            let top = neckline.map_or(0.49, |cut| cut.bottom - 0.012);
            let tip = if neckline.is_some() { 0.008 } else { 0.025 * b };
            fitted_panel(
                &mut p.matte,
                skin,
                &[
                    (0.02, 0.025 * b),
                    ((0.02 + top) * 0.5, 0.031 * b),
                    (top, tip),
                ],
                true,
                0.01,
                color,
            );
            fitted_band(&mut p.matte, skin, 0.035, 0.07, 0.006, color * 0.8);
        }
        TorsoOrnament::RivetedVest { strap, rivets } => {
            // A riveted leather vest on shoulder straps.
            for side in [-1.0f32, 1.0] {
                let (f, k) = (on(side * 0.11, 0.41, true), on(side * 0.11, 0.41, false));
                tube(
                    &mut p.satin,
                    &[
                        f,
                        Vec3::new(side * 0.13, 0.53, 0.05),
                        Vec3::new(side * 0.13, 0.53, -0.06),
                        k,
                    ],
                    &[(0.03, 0.01); 4],
                    strap,
                );
            }
            for k in 0..3 {
                p.metal.blob(
                    on(-0.1 + k as f32 * 0.1, 0.16, true),
                    Vec3::splat(0.012),
                    0,
                    0.0,
                    rivets,
                );
            }
        }
        TorsoOrnament::StitchedFront { thread } => {
            for k in 0..3 {
                p.matte.cuboid(
                    on(0.055 * b, 0.2 + k as f32 * 0.04, true) + Vec3::Z * 0.003,
                    Vec3::new(0.04 * b, 0.005, 0.004),
                    Quat::IDENTITY,
                    thread,
                );
            }
        }
        TorsoOrnament::HighCollar {
            collar,
            pads,
            cravat,
            buttons,
        } => {
            // A stand-up collar framing the head from behind.
            // Plum shoulder pads remain distinct from the dark cape.
            for side in [-1.0f32, 1.0] {
                p.matte.blob(
                    Vec3::new(side * 0.2 * b * body.profile.shoulders, 0.47, 0.0),
                    Vec3::new(0.085, 0.045, 0.09),
                    2,
                    0.0,
                    pads,
                );
            }
            // A high collar, red-lined, rising behind the head and folding open at the front.
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, 0.5, -0.05),
                    Vec3::new(0.0, 0.58, -0.08),
                    Vec3::new(0.0, 0.67, -0.11),
                ],
                &[(0.11, 0.085), (0.13, 0.095), (0.155, 0.105)],
                collar,
            );
            // His cravat closes the shirt front; her fitted bodice keeps
            // its open neckline, as on the adult female concept sheet.
            if !body.woman {
                fitted_ribbon(
                    &mut p.matte,
                    garment,
                    &[
                        (Vec2::new(0.0, 0.46), 0.024 * b),
                        (Vec2::new(0.0, 0.36), 0.02 * b),
                        (Vec2::new(0.0, 0.26), 0.012 * b),
                    ],
                    true,
                    0.01,
                    cravat,
                );
            }
            for side in [-1.0f32, 1.0] {
                for k in 0..3 {
                    p.metal.blob(
                        on(side * 0.055, 0.13 + k as f32 * 0.08, true),
                        Vec3::splat(0.011),
                        0,
                        0.0,
                        buttons,
                    );
                }
            }
        }

        TorsoOrnament::Corset { metal, gem } => royal_parts::corset(p, body, skin, metal, gem),
        TorsoOrnament::Harness {
            strap: color,
            ring: ring_color,
            shoulder_straps,
        } => {
            // A harness strap across the chest, buckled to a ring at the sternum.
            for front in [true, false] {
                let strap = [
                    (Vec2::new(0.15 * sh, 0.45), 0.026 * b),
                    (Vec2::new(0.05 * b, 0.33), 0.026 * b),
                    (Vec2::new(-0.06 * b, 0.21), 0.026 * b),
                    (Vec2::new(-0.13 * w, 0.05), 0.022 * b),
                ];
                fitted_ribbon(&mut p.satin, garment, &strap, front, 0.016, color);
            }
            let (ring, normal) = torso_surface_frame(garment, Vec2::new(0.03 * b, 0.31), true);
            p.metal.torus(
                ring + normal * 0.026,
                normal,
                0.03,
                0.008,
                14,
                4,
                ring_color,
            );
            if let Some(color) = shoulder_straps {
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
                        color,
                    );
                }
            }
        }
        TorsoOrnament::StockAndButtons {
            stock,
            buttons,
            belt,
        } => {
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
                stock,
            );
            for k in 0..4 {
                let (at, normal) =
                    torso_surface_frame(skin, Vec2::new(0.0, 0.12 + k as f32 * 0.065), true);
                p.metal
                    .blob(at + normal * 0.012, Vec3::splat(0.012), 0, 0.0, buttons);
            }
            fitted_band(&mut p.satin, skin, 0.035, 0.07, 0.008, belt);
        }
        TorsoOrnament::LeafBodice { leaves, pin } => {
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
                    leaves * (0.85 + 0.3 * rand(98, k)),
                );
            }
            let (at, normal) = torso_surface_frame(skin, Vec2::new(0.0, 0.1), true);
            p.metal.blob(
                at + normal * 0.02,
                Vec3::new(0.022, 0.03, 0.01),
                0,
                0.0,
                pin,
            );
        }
        TorsoOrnament::NeckBandAndDisc { metal } => {
            // A gilt neckline and a bronze disc at the waist.
            fitted_band(&mut p.metal, skin, 0.42, 0.44, 0.01, metal);
            let (at, normal) = torso_surface_frame(skin, Vec2::new(0.0, 0.07), true);
            p.metal
                .torus(at + normal * 0.02, normal, 0.03, 0.01, 14, 4, metal);
            p.metal
                .blob(at + normal * 0.022, Vec3::splat(0.02), 0, 0.0, metal * 0.8);
        }
        TorsoOrnament::KnottedSash { sash, scarf } => {
            // A knotted sash binds the torn cloth at the waist.
            fitted_band(&mut p.matte, garment, 0.02, 0.07, 0.01, sash);
            let (at, normal) = torso_surface_frame(garment, Vec2::new(0.05 * b, 0.045), true);
            p.matte.blob(
                at + normal * 0.02,
                Vec3::new(0.03, 0.025, 0.018),
                1,
                0.0,
                sash,
            );
            if let Some(color) = scarf {
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
                    color,
                );
            }
        }
    }
}

/// A flat, pointed leaf from `root` along `dir`, broad a third of the way out.
/// Shared by sewn foliage, crowns and plant models.
pub(super) fn leaf(b: &mut Builder, root: Vec3, dir: Vec3, len: f32, width: f32, color: Vec3) {
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
