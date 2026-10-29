use super::*;

/// A neck widens into the collar and narrows beneath the jaw. The lower
/// station is buried below the garment opening even when the head turns.
/// The neck from the shoulders up into the head, in the head's space:
/// points up its middle and its radii (across, front to back) at each.
pub(in crate::creatures) fn neck_profile(build: f32, slim: f32) -> [(Vec3, Vec2); 4] {
    let scale = build * slim;
    [
        (
            Vec3::new(0.0, -0.065, -0.008),
            Vec2::new(0.056, 0.052) * scale,
        ),
        (
            Vec3::new(0.0, -0.022, -0.007),
            Vec2::new(0.05, 0.047) * scale,
        ),
        (
            Vec3::new(0.0, 0.033, -0.008),
            Vec2::new(0.045, 0.043) * scale,
        ),
        (
            Vec3::new(0.0, 0.085, -0.01),
            Vec2::new(0.045, 0.045) * scale,
        ),
    ]
}

/// How slender a neck is: hers finer than his, though never a stalk under
/// the storybook head.
pub(in crate::creatures) fn neck_slim(woman: bool) -> f32 {
    if woman {
        0.9
    } else {
        1.0
    }
}

pub(super) fn biped_neck(b: &mut Builder, build: f32, slim: f32, color: Vec3) {
    // Straight profiles need no spline subdivision. Finish still smooths
    // the resulting mesh normally; restoring this flag leaves later forms
    // unchanged and keeps these small transition pieces inexpensive.
    let smooth = b.smooth;
    b.smooth = false;
    let profile = neck_profile(build, slim);
    tube(
        b,
        &profile.map(|(at, _)| at),
        &profile.map(|(_, r)| (r.x, r.y)),
        color,
    );
    b.smooth = smooth;
}

/// What a styled sleeve adds over the shoulder and upper arm: a puff, or a
/// gathered off-shoulder band, sheer or not.
pub(super) fn styled_sleeve_top(
    p: &mut Parts,
    sleeve: super::outfit::Sleeve,
    side: f32,
    b: f32,
    arm_len: f32,
) {
    let builder = if sleeve.sheer {
        &mut p.sheer
    } else {
        &mut p.matte
    };
    let color = sleeve.color;
    if let Some(lining) = sleeve.puff_lining {
        sleeve_parts::slashed_puff(builder, side, b, arm_len, color, lining);
    } else if sleeve.puff || sleeve.cut == SleeveCut::Puff {
        // A gathered puff over the shoulder, banded beneath.
        builder.blob(
            Vec3::new(side * 0.008, -0.045 * arm_len, 0.0),
            Vec3::new(0.083, 0.078, 0.083) * b,
            7,
            0.08,
            color,
        );
        builder.torus(
            Vec3::Y * -0.105 * arm_len,
            Vec3::Y,
            0.052 * b,
            0.011 * b,
            14,
            4,
            sleeve.flare.unwrap_or(color * 0.85),
        );
    }
    if sleeve.cut == SleeveCut::OffShoulder {
        royal_parts::sleeve_swag(builder, b, arm_len, color, sleeve.flare);
    }
    if sleeve.cut == SleeveCut::Short {
        // A fitted cap over the shoulder, ending a third of the way down the
        // upper arm in a turned-up hem.
        builder.blob(
            Vec3::new(side * 0.006, -0.035 * arm_len, 0.0),
            Vec3::new(0.074, 0.066, 0.074) * b,
            11,
            0.0,
            color,
        );
        builder.tube(
            &[Vec3::Y * -0.03 * arm_len, Vec3::Y * -0.115 * arm_len],
            &[Vec2::splat(0.071 * b), Vec2::splat(0.067 * b)],
            16,
            &|_, _, _| color,
        );
        builder.torus(
            Vec3::Y * -0.112 * arm_len,
            Vec3::Y,
            0.066 * b,
            0.009 * b,
            16,
            4,
            sleeve.flare.unwrap_or(color * 0.82),
        );
    }
}

/// One sleeve or bracer follows the forearm's full upper mass and narrow
/// wrist. A thin coloured hem replaces stacked cylindrical cuff rings.
pub(super) fn biped_cuff(
    b: &mut Builder,
    build: f32,
    length: f32,
    muscle: f32,
    bell: bool,
    color: Vec3,
    trim: Vec3,
) {
    let heights = if bell {
        [0.055, 0.11, 0.19, 0.25, 0.263]
    } else {
        [0.11, 0.145, 0.205, 0.25, 0.268]
    };
    let radii = if bell {
        [
            Vec2::new(0.052, 0.047),
            Vec2::new(0.057, 0.05),
            Vec2::new(0.072, 0.063),
            Vec2::new(0.084, 0.075),
            Vec2::new(0.083, 0.074),
        ]
    } else {
        [
            Vec2::new(0.053, 0.047),
            Vec2::new(0.057, 0.05),
            Vec2::new(0.049, 0.042),
            Vec2::new(0.042, 0.037),
            Vec2::new(0.043, 0.038),
        ]
    };
    let path = heights.map(|height| Vec3::Y * -height * length);
    // Follow proximal forearm fullness, then return to the authored wrist and
    // hem. Both recipes keep their final two stations and flare unchanged.
    // Bracers and cuffs are worn over the eased sleeve beneath them.
    let radii: [Vec2; 5] = std::array::from_fn(|i| {
        let influence = 1.0 - ease((heights[i] - 0.145) / (0.25 - 0.145));
        radii[i] * build * (1.0 + (muscle - 1.0) * influence) + Vec2::splat(SLEEVE_EASE * build)
    });
    if bell {
        let profile = std::array::from_fn(|i| (heights[i], radii[i]));
        sleeve_parts::bell(b, &profile, build, length, color, trim);
        return;
    }
    let smooth = b.smooth;
    b.smooth = false;
    b.tube(&path, &radii, 12, &|_, _at, along| {
        if along > 0.94 {
            trim
        } else {
            color
        }
    });
    b.smooth = smooth;
}

/// A broad forged crown over a thinner sliding lame. Ring profiles give the
/// metal deliberate planes; the hollow underside is fitted to the actual arm.
/// The material and colours of layered shoulder caps: steel with a gilt rim
/// for plate, or boiled leather with a stitched, studded or iron rim.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::creatures) struct PauldronFinish {
    pub leather: bool,
    pub face: Vec3,
    pub rim: Vec3,
}

impl PauldronFinish {
    pub const STEEL: Self = Self {
        leather: false,
        face: STEEL,
        rim: GILT,
    };
}

pub(super) fn biped_pauldron(p: &mut Parts, side: f32, body: HumanoidBody, finish: PauldronFinish) {
    let layer = if finish.leather {
        &mut p.matte
    } else {
        &mut p.metal
    };
    let build = body.build;
    let mut support = upper_arm_form(side, build, body.arm_length, body.profile.muscle, 0.94);
    // Facet chords need room as well as their vertices. Padding is part of the
    // support, so it follows both the clavicle insertion and the muscle belly.
    support.inflate(0.012 * build);
    for (profile, tint, crown) in [
        (
            &[
                (0.042, 0.025),
                (0.030, 0.055),
                (0.014, 0.078),
                (-0.030, 0.098),
                (-0.072, 0.100),
                (-0.078, 0.100),
            ][..],
            1.0,
            true,
        ),
        (
            &[
                (-0.065, 0.088),
                (-0.098, 0.094),
                (-0.134, 0.087),
                (-0.140, 0.086),
            ][..],
            0.92,
            false,
        ),
    ] {
        const SIDES: usize = 20;
        let inner: Vec<[Vec3; SIDES]> = profile
            .iter()
            .enumerate()
            .map(|(row, &(height, radius))| {
                std::array::from_fn(|k| {
                    let angle = k as f32 / SIDES as f32 * TAU;
                    let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
                    let center = Vec3::Y * height * body.arm_length;
                    // Both ends of a panel bound its complete support span.
                    // A clavicle bulge between ring rows must not pierce a flat
                    // face whose endpoints happen to clear it.
                    let top = profile[row.saturating_sub(1)].0;
                    let bottom = profile[(row + 1).min(profile.len() - 1)].0;
                    let fitted = (0..=8)
                        .map(|step| {
                            let y = (top + (bottom - top) * step as f32 / 8.0) * body.arm_length;
                            let origin = Vec3::Y * y;
                            let hit = support.onto(origin + outward * 0.4, -outward);
                            if support.distance(hit).abs() < 0.002 {
                                (hit - origin).dot(outward)
                            } else {
                                0.0
                            }
                        })
                        .fold(0.0, f32::max);
                    center + outward * (radius * build).max(fitted)
                })
            })
            .collect();
        let outer: Vec<[Vec3; SIDES]> = inner
            .iter()
            .enumerate()
            .map(|(row, ring)| {
                std::array::from_fn(|k| {
                    let angle = k as f32 / SIDES as f32 * TAU;
                    let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
                    // A turned lower edge catches a single readable gold line.
                    let rim = if row + 1 == inner.len() { 0.003 } else { 0.0 };
                    ring[k]
                        + outward * (0.008 + rim) * build
                        + if row == 0 && crown {
                            Vec3::Y * 0.008 * build
                        } else {
                            Vec3::ZERO
                        }
                })
            })
            .collect();
        for row in 0..inner.len() - 1 {
            for k in 0..SIDES {
                let next = (k + 1) % SIDES;
                let inside =
                    Vec3::Y * (profile[row].0 + profile[row + 1].0) * 0.5 * body.arm_length;
                layer.quad_out(
                    [
                        outer[row][k],
                        outer[row + 1][k],
                        outer[row + 1][next],
                        outer[row][next],
                    ],
                    inside,
                    if row + 2 == inner.len() {
                        finish.rim
                    } else {
                        finish.face * tint
                    },
                );
                // Reverse winding on the inside: the surface faces the arm.
                let q = [
                    inner[row][k],
                    inner[row][next],
                    inner[row + 1][next],
                    inner[row + 1][k],
                ];
                let normal = (q[1] - q[0]).cross(q[2] - q[0]);
                let mut q = q;
                if normal.dot((q[0] + q[2]) * 0.5 - inside) > 0.0 {
                    q.reverse();
                }
                layer.tri([q[0], q[1], q[2]], finish.face * 0.68);
                layer.tri([q[0], q[2], q[3]], finish.face * 0.68);
            }
        }
        for row in [0, inner.len() - 1] {
            let upward = if row == 0 { Vec3::Y } else { Vec3::NEG_Y };
            let inside = Vec3::Y * profile[row].0 * body.arm_length - upward;
            for k in 0..SIDES {
                let next = (k + 1) % SIDES;
                layer.quad_out(
                    [
                        inner[row][k],
                        outer[row][k],
                        outer[row][next],
                        inner[row][next],
                    ],
                    inside,
                    finish.rim,
                );
            }
        }
        if crown {
            let inner_center = Vec3::Y * profile[0].0 * body.arm_length;
            let outer_center = inner_center + Vec3::Y * 0.008 * build;
            for k in 0..SIDES {
                let next = (k + 1) % SIDES;
                layer.tri_out(
                    [outer_center, outer[0][k], outer[0][next]],
                    outer_center - Vec3::Y,
                    finish.face,
                );
                layer.tri_out(
                    [inner_center, inner[0][k], inner[0][next]],
                    inner_center + Vec3::Y,
                    finish.face * 0.68,
                );
            }
        }
    }
}

/// Construction of an existing outer garment, independent of its pigment.
#[derive(Clone, Copy)]
pub(in crate::creatures) enum TorsoMaterial {
    Cloth,
    Leather,
    Plate,
}

pub(super) fn torso_garment(
    body: HumanoidBody,
    material: TorsoMaterial,
    trim_neck: bool,
) -> crate::sdf::Sculpt {
    torso_shape(body, Some(material), trim_neck)
}

/// Where a shell ends at the top: its own height, or for an under-chest
/// corset just beneath the wearer's chest.
pub(in crate::creatures) fn shell_top(body: HumanoidBody, top: f32, underbust: bool) -> f32 {
    if underbust {
        body.bust.map_or(top, |bust| bust.fold + 0.03)
    } else {
        top
    }
}

/// Resolve the shell once for emission and attachments. Openings belong to
/// this same surface so fitting cannot silently trace a different garment.
pub(super) fn outfit_shell(
    body: HumanoidBody,
    outfit: &Outfit,
    trim_neck: bool,
) -> Option<crate::sdf::Sculpt> {
    use crate::sdf::Shape::*;
    let Some(Outer::Shell {
        top,
        sleeveless,
        leather,
        underbust,
        opening,
        ..
    }) = outfit.outer
    else {
        return None;
    };
    let top = shell_top(body, top, underbust);
    let b = body.build;
    let sh = body.shoulders * b;
    let plunge = NecklineCut::resolve(outfit.bodice.cut, body);
    let material = if leather {
        TorsoMaterial::Leather
    } else {
        TorsoMaterial::Cloth
    };
    let mut shell = torso_garment(body, material, trim_neck);
    shell
        .carve(Beyond(Vec3::Y, top), 0.01)
        .carve(Beyond(-Vec3::Y, 0.0), 0.0);
    if sleeveless {
        for side in [-1.0f32, 1.0] {
            shell.carve(Beyond(Vec3::X * side, 0.185 * sh), 0.008);
        }
    }
    match opening {
        ShellOpening::Closed => {}
        ShellOpening::ShapedNeck => {
            if let Some(cut) = plunge {
                shell.carve(cut.opening(body, top, 0.014), 0.006);
            } else {
                shell.carve(
                    Ellipsoid(
                        Vec3::new(0.0, top + 0.055, 0.135 * b),
                        Vec3::new(0.11 * sh, 0.105, 0.15 * b),
                    ),
                    0.006,
                );
            }
        }
        ShellOpening::PlungingCoat => {
            match plunge {
                Some(cut) => shell.carve(cut.opening(body, top, 0.01), 0.006),
                None => shell.carve(
                    Ellipsoid(
                        Vec3::new(0.0, top + 0.01, 0.15 * b),
                        Vec3::new(0.085 * sh, 0.15, 0.13 * b),
                    ),
                    0.006,
                ),
            };
        }
        ShellOpening::OpenCoat => {
            shell.carve(
                Ellipsoid(
                    Vec3::new(0.0, 0.2, 0.2 * b),
                    Vec3::new(0.075 * sh, 0.3, 0.12 * b),
                ),
                0.008,
            );
        }
    }
    Some(shell)
}

/// A closed waist section of the selected shell, expressed in hips space.
/// Current peplum recipes all attach below the neckline and arm openings.
/// Match both depth and centre: a fitted torso is not centred on z=0.
pub(super) fn shell_waist(
    form: &crate::sdf::Sculpt,
    body: HumanoidBody,
    height: f32,
) -> (Vec2, Vec3) {
    let scale = storybook_part(Plan::Biped, J::Torso);
    let y = (height - body.torso.y) / scale.y;
    let front = torso_surface(form, 0.0, y, true).z;
    let back = torso_surface(form, 0.0, y, false).z;
    let centre = (front + back) * 0.5;
    let (mut inside, mut outside) = (0.0, 0.5 * body.build);
    for _ in 0..20 {
        let x = (inside + outside) * 0.5;
        if form.distance(Vec3::new(x, y, centre)) <= 0.0 {
            inside = x;
        } else {
            outside = x;
        }
    }
    // The 7mm turned cloth overlaps its support slightly at the sewn edge.
    let radii =
        Vec2::new((inside + outside) * 0.5, (front - back) * 0.5) * scale.xz() + Vec2::splat(0.003);
    let offset = body.torso + Vec3::new(0.0, y, centre) * scale - Vec3::Y * 0.04;
    (radii, offset)
}

/// A woman's chest, each side one form attached across the rib cage: a broad
/// base thickening the chest wall from sternum edge to the side, an upper
/// slope leaving the wall high on the chest, and the lower fullness it runs
/// into. `grow` lays a garment over the same supports.
pub(in crate::creatures) fn bust_supports(body: &mut crate::sdf::Sculpt, bust: &Bust, grow: f32) {
    use crate::sdf::Shape::*;
    for side in [-1.0f32, 1.0] {
        let (center, radii) = bust.mass(side, grow);
        let mid = (bust.top + bust.fold) * 0.5;
        // The base lies on the wall, not in front of it: it widens the
        // attachment so the fullness grows out of the chest.
        let depth = 0.022 + 0.3 * bust.radii.z;
        body.add(
            Ellipsoid(
                Vec3::new(side * bust.root, mid, center.z - radii.z * 0.7),
                Vec3::new(bust.footprint, (bust.top - bust.fold) * 0.5, depth) + Vec3::splat(grow),
            ),
            0.05,
        );
        // The upper slope: a tapering ramp from high on the chest wall down
        // into the fullness, its curve set by the contour.
        let start = bust.slope_start * Vec3::new(side, 1.0, 1.0);
        body.add(
            Capsule(
                start,
                center + Vec3::Y * radii.y * 0.25,
                bust.slope + grow,
                radii.x.min(radii.z) * 0.82,
            ),
            0.045,
        )
        .add(Ellipsoid(center, radii), 0.03);
    }
}

/// The two halves of the seat, left then right: centre and radii.
pub(in crate::creatures) fn seat_masses(body: HumanoidBody) -> [(Vec3, Vec3); 2] {
    let r = body.pelvis_radii();
    let depth = if body.woman { 0.58 } else { 0.48 };
    [-1.0f32, 1.0].map(|side| {
        (
            Vec3::new(side * r.x * 0.36, -0.035, -r.z * 0.4),
            r * Vec3::new(0.44, 0.68, depth),
        )
    })
}

/// The pelvis sculpted as one form inside its fitted envelope: a waist
/// tapering up into the torso's own, shelves over the hip joints, a lower
/// belly and a rounded seat, fuller on a woman. Garments that fit the
/// envelope (skirts, belts, trousers) still contain it.
pub(super) fn pelvic_supports(body: HumanoidBody, offset: Vec3) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let b = body.build;
    let r = body.pelvis_radii();
    let seat = seat_masses(body)[1];
    let mut pelvis = Sculpt::default();
    pelvis
        .add(
            Ellipsoid(
                offset + Vec3::new(0.0, -0.005, 0.0),
                r * Vec3::new(0.86, 0.82, 0.95),
            ),
            0.0,
        )
        // Up into the waist: just inside the torso's lowest support, so the
        // two parts meet without a ridge and the torso's own garment, not
        // the seat's, shows above the belt.
        .add(
            Ellipsoid(
                offset + body.torso + Vec3::new(0.0, 0.075, -0.004),
                Vec3::new(0.15 * body.waist * b, 0.115, 0.108 * b) - Vec3::splat(0.005),
            ),
            0.045,
        )
        // The lower belly, soft and forward of the hip line.
        .add(
            Ellipsoid(
                offset + Vec3::new(0.0, -0.015, r.z * 0.16),
                r * Vec3::new(0.7, 0.56, 0.6),
            ),
            0.045,
        );
    for side in [-1.0f32, 1.0] {
        pelvis
            // The hip shelf over the joint the thigh hangs from, as wide as
            // the thigh's head beneath it up to the waistband.
            .add(
                Ellipsoid(
                    offset + Vec3::new(side * r.x * 0.6, -0.005, 0.0),
                    r * Vec3::new(0.4, 0.8, 0.6),
                ),
                0.05,
            )
            // The seat: rounder and fuller on a woman.
            .add(
                Ellipsoid(offset + seat.0 * Vec3::new(side, 1.0, 1.0), seat.1),
                0.04,
            );
    }
    pelvis
}

/// The lower skin/seat ends beneath the upper garment; both use the same supports.
pub(in crate::creatures) fn pelvis_shape(body: HumanoidBody) -> crate::sdf::Sculpt {
    let mut pelvis = pelvic_supports(body, Vec3::ZERO);
    pelvis.carve(
        crate::sdf::Shape::Beyond(Vec3::Y, BELT_BOTTOM_Y - 0.005),
        0.005,
    );
    pelvis
}

/// Where the sculpted pelvis's surface lies at a height, out along a
/// horizontal direction: the anchor for anything hung round the hips.
pub(in crate::creatures) fn pelvis_surface(pelvis: &crate::sdf::Sculpt, y: f32, out: Vec3) -> Vec3 {
    pelvis.onto(Vec3::new(0.0, y, 0.0) + out * 0.5, -out)
}

/// A supporting garment does not follow the fold beneath a chest: it falls
/// from the fullness to the rib cage, by `share` of the chest's depth.
pub(in crate::creatures) fn bust_underbridge(
    body: &mut crate::sdf::Sculpt,
    bust: &Bust,
    share: f32,
    grow: f32,
    allowance: f32,
) {
    use crate::sdf::Shape::*;
    let low = bust.fold - 0.075;
    for side in [-1.0f32, 1.0] {
        let (center, radii) = bust.mass(side, 0.0);
        // It falls straight down as a broad sheet whose lower edge lies
        // flush on the rib cage, never a ridge or a lump on it.
        let x = center.x * 0.9;
        let spread = radii.x * 0.7;
        let end = Vec3::new(x, low, bust.wall.front(x, low) - spread + 0.002);
        body.add(
            Capsule(
                center,
                end,
                radii.z * share + grow + allowance,
                spread + grow,
            ),
            0.04,
        );
    }
}

/// Body supports remain exact; garment ease is local to ribs and back so
/// the waist stays fitted and the shoulder/neck openings keep their anchors.
pub(in crate::creatures) fn torso_shape(
    fit: HumanoidBody,
    material: Option<TorsoMaterial>,
    trim_neck: bool,
) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let b = fit.build;
    let (sh, w) = (fit.shoulders * b, fit.waist * b);
    let fem = fit.woman;
    // Structured fronts span the valley between the chest's fullness: by a
    // share of its depth and an allowance past the material's own growth.
    let (grow, ease, bridge) = match material {
        None => (0.0, Vec2::ZERO, None),
        Some(TorsoMaterial::Cloth) => (0.010, Vec2::new(0.006, 0.004), Some((0.55, 0.0))),
        Some(TorsoMaterial::Leather) => (0.014, Vec2::new(0.010, 0.006), Some((0.8, 0.004))),
        Some(TorsoMaterial::Plate) => (0.018, Vec2::new(0.012, 0.008), Some((1.0, 0.008))),
    };
    let g = Vec3::splat(grow);
    let rib_g = if material.is_some() {
        g + Vec3::new(ease.x, 0.0, ease.y)
    } else {
        g
    };
    let mut body = Sculpt::default();
    body.add(
        Ellipsoid(
            Vec3::new(0.0, 0.075, -0.004),
            Vec3::new(0.15 * w, 0.115, 0.108 * b) + g,
        ),
        0.0,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.29, -0.014),
            Vec3::new(0.183 * sh.max(w), 0.175, 0.119 * b) + rib_g,
        ),
        0.055,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.385, -0.047 * b),
            Vec3::new(0.18 * sh, 0.125, 0.09 * b) + rib_g,
        ),
        0.045,
    )
    .add(
        // The neck's root stays just inside the neck itself, as slender as
        // it is, so the neck rises out of the shoulders' slope: never out of
        // a wider stub whose flat top would show as a ledge round it.
        Capsule(
            Vec3::new(0.0, 0.505, -0.008),
            Vec3::new(0.0, super::anatomy::NECK_SEAT, -0.008),
            0.059 * b * neck_slim(fem) - 0.002 + grow,
            0.05 * b * neck_slim(fem) - 0.004 + grow,
        ),
        0.035,
    );
    for side in [-1.0f32, 1.0] {
        // The clavicle slopes into the arm's moving deltoid instead
        // of leaving a separate spherical cap on the torso.
        body.add(
            Capsule(
                Vec3::new(side * 0.035 * b, 0.485, -0.005),
                Vec3::new(side * 0.195 * sh, 0.465, -0.005),
                0.038 * b + grow,
                0.034 * b + grow,
            ),
            0.035,
        );
    }
    if let Some(bust) = fit.bust {
        bust_supports(&mut body, &bust, grow);
    }
    if !fem {
        // One broad, shallow pectoral plane joins both shoulders to
        // the rib cage. Paired forward lobes read as breast cups,
        // especially when the fitted leather follows their shape.
        body.add(
            Ellipsoid(
                Vec3::new(0.0, 0.39, -0.006 * b),
                Vec3::new(0.185 * sh, 0.11, 0.095 * b) + g,
            ),
            0.04,
        );
    }
    // Structured fronts span the chest valley while the chest's own supports
    // retain the upper and lateral form. A single replacement ellipsoid
    // would not enclose those offset supports without flattening the shape.
    if let (Some(bust), Some((share, allowance))) = (fit.bust, bridge) {
        let (left, l) = bust.mass(-1.0, 0.0);
        let (right, r) = bust.mass(1.0, 0.0);
        body.add(
            Capsule(
                left,
                right,
                l.z * share + grow + allowance,
                r.z * share + grow + allowance,
            ),
            0.02,
        );
        // Plate is forged as one protective volume across the chest, never
        // a pair of cups: a single shell spans both sides and the valley.
        if matches!(material, Some(TorsoMaterial::Plate)) {
            let span = right.x + r.x * 0.75;
            body.add(
                Ellipsoid(
                    Vec3::new(0.0, (left.y + right.y) * 0.5 + 0.01, right.z),
                    Vec3::new(span, r.y * 1.35, r.z) + Vec3::splat(grow + allowance),
                ),
                0.04,
            );
        }
        bust_underbridge(&mut body, &bust, share, grow, allowance);
    }
    // Carry the same upper-pelvis curve into the lower torso. Independent
    // waist ellipsoids left a visible step, especially on fuller hips.
    let mut lower = pelvic_supports(fit, -fit.torso);
    lower.inflate(grow);
    lower.carve(
        Beyond(Vec3::NEG_Y, -(BELT_BOTTOM_Y - 0.01 - fit.torso.y)),
        0.005,
    );
    body.union(lower, 0.0);
    if trim_neck && material.is_some() {
        // The tunic ends at its fitted opening rather than climbing
        // inside the moving skin neck as a second thick cylinder. Bare
        // skin is not trimmed: the neck rises from it unbroken.
        body.carve(Beyond(Vec3::Y, 0.552), 0.004);
    }
    body
}
