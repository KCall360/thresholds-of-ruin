use super::*;

/// Seat sockets on the actual skull surface and return a local frame for
/// each eye. Animals look out to the sides as well as forward; a forward-only
/// eye can disappear behind a lateral brow or show through the opposite cheek.
pub(super) fn animal_eye_sockets(
    skull: &mut crate::sdf::Sculpt,
    center: Vec3,
    spread: f32,
    size: f32,
    outward: Vec3,
) -> [(Vec3, Vec3); 2] {
    use crate::sdf::Shape::Ellipsoid;
    [-1.0f32, 1.0].map(|side| {
        let normal = Vec3::new(side * outward.x, outward.y, outward.z).normalize();
        let hint = center + Vec3::X * side * spread;
        let at = skull.onto(hint + normal * 0.18, -normal);
        skull.carve(
            Ellipsoid(at + normal * size * 0.18, Vec3::splat(size * 1.02)),
            size * 0.16,
        );
        (at - normal * size * 0.12, normal)
    })
}

/// An ellipsoid expressed in an eye's tangent frame. The builder still owns
/// the part transform, so larger storybook skulls keep correctly oriented eyes.
pub(super) fn animal_eye_lobe(
    b: &mut Builder,
    center: Vec3,
    normal: Vec3,
    radii: Vec3,
    color: Vec3,
) {
    let right = Vec3::Y.cross(normal).normalize_or(Vec3::X);
    let up = normal.cross(right).normalize();
    let point = |row: usize, column: usize| {
        let latitude = row as f32 * PI / 8.0;
        let longitude = column as f32 * TAU / 12.0;
        let local = Vec3::new(
            latitude.sin() * longitude.cos(),
            latitude.cos(),
            latitude.sin() * longitude.sin(),
        );
        let at = center
            + right * local.x * radii.x
            + up * local.y * radii.y
            + normal * local.z * radii.z;
        let n = (right * local.x / radii.x + up * local.y / radii.y + normal * local.z / radii.z)
            .normalize();
        (at, n)
    };
    for row in 0..8 {
        for column in 0..12 {
            let a = point(row, column);
            let b0 = point(row + 1, column);
            let c = point(row + 1, column + 1);
            let d = point(row, column + 1);
            for (enabled, mut face) in [(row > 0, [a, b0, d]), (row < 7, [b0, c, d])] {
                if enabled {
                    if (face[1].0 - face[0].0)
                        .cross(face[2].0 - face[0].0)
                        .dot(face[0].1 + face[1].1 + face[2].1)
                        < 0.0
                    {
                        face.swap(1, 2);
                    }
                    b.tri_smooth(face.map(|v| v.0), face.map(|v| v.1), [color; 3]);
                }
            }
        }
    }
}

/// Bold, hooded animal eyes inside broad carved sockets. Their coloured iris
/// fills most of the opening, giving expression without exposed white spheres.
pub(super) fn animal_eyes(
    p: &mut Parts,
    frames: [(Vec3, Vec3); 2],
    size: f32,
    iris: Vec3,
    skin: Vec3,
    accent: Option<Accent>,
) {
    if let Some(accent) = accent {
        p.accent = Some(accent);
    }
    for (at, normal) in frames {
        let right = Vec3::Y.cross(normal).normalize_or(Vec3::X);
        let up = normal.cross(right).normalize();
        animal_eye_lobe(
            &mut p.matte,
            at,
            normal,
            Vec3::new(size * 1.08, size * 0.86, size * 0.65),
            DARK * 1.4,
        );
        let colored = if accent.is_some() {
            &mut p.glow
        } else {
            &mut p.satin
        };
        animal_eye_lobe(
            colored,
            at + normal * size * 0.54,
            normal,
            Vec3::new(size * 0.8, size * 0.73, size * 0.26),
            if accent.is_some() { Vec3::ONE } else { iris },
        );
        if accent.is_none() {
            animal_eye_lobe(
                &mut p.satin,
                at + normal * size * 0.78,
                normal,
                Vec3::new(size * 0.32, size * 0.54, size * 0.12),
                DARK,
            );
            animal_eye_lobe(
                &mut p.satin,
                at + normal * size * 0.87 + (right + up) * size * 0.23,
                normal,
                Vec3::splat(size * 0.13),
                Vec3::splat(1.5),
            );
        }
        tube(
            &mut p.matte,
            &[
                at - right * size * 1.1 + up * size * 0.12,
                at + up * size * 0.8 + normal * size * 0.36,
                at + right * size * 1.1 + up * size * 0.12,
            ],
            &[
                (size * 0.2, size * 0.18),
                (size * 0.26, size * 0.2),
                (size * 0.2, size * 0.18),
            ],
            skin * 0.95,
        );
    }
}

/// Convert authored parent-mesh coordinates into a child joint's mesh frame.
/// The pivot is in unscaled parent-joint coordinates; baked mesh proportions
/// are applied independently of joint offsets. In neutral pose, pivot +
/// child_scale * this_transform * vertex equals parent_scale * vertex.
/// This is a rest-frame conversion, not a surface fit or animation transform.
pub(super) fn reparent_mesh_transform(plan: Plan, parent: J, terminal: J, pivot: Vec3) -> Affine3A {
    let parent_scale = storybook_part(plan, parent);
    let terminal_scale = storybook_part(plan, terminal);
    Affine3A::from_translation(-pivot / terminal_scale)
        * Affine3A::from_scale(parent_scale / terminal_scale)
}

/// Bone endpoint shared by the bird mesh and its support solver.
pub(in crate::creatures) const BIRD_ANKLE: Vec3 = super::anatomy::BirdBody::LOWER;

pub(super) fn bird(kit: &mut Kit, lift: Entity, look: &Look) {
    use super::anatomy::BirdBody;
    let l = look.clone();
    kit.rig.ankle_floor = BirdBody::ANKLE_HEIGHT + 0.002;
    let body = kit.joint(
        J::Body,
        lift,
        Vec3::Y * BirdBody::BODY_HEIGHT,
        "body",
        |p| {
            p.matte.blob(
                Vec3::ZERO,
                Vec3::new(0.245, 0.26, 0.355),
                11,
                0.0,
                l.skin * 0.7,
            );
            // Each shingle travels down the ovoid, turning inward below its
            // widest latitude. Lower rows shorten toward the belly rather than
            // all flaring out into one horizontal skirt.
            for row in 0..7 {
                let latitude = 0.12 + row as f32 * 0.31;
                let count = 14 + row * 4;
                for k in 0..count {
                    let n = (row * 50 + k) as u32;
                    let longitude = (k as f32 + 0.5 * (row % 2) as f32 + rand(84, n) * 0.3)
                        / count as f32
                        * TAU;
                    let sweep = 0.5 + 0.08 * rand(81, n);
                    let path: [Vec3; 4] = std::array::from_fn(|i| {
                        let t = i as f32 / 3.0;
                        let a = latitude + sweep * t;
                        let normal = Vec3::new(
                            a.sin() * longitude.cos(),
                            a.cos(),
                            a.sin() * longitude.sin(),
                        );
                        let surface = normal * Vec3::new(0.245, 0.26, 0.355);
                        // Fine tips leave the surface only slightly. This retains
                        // shaggy texture around a connected body silhouette.
                        surface
                            + normal * (0.012 + 0.012 * (PI * t).sin())
                            + Vec3::NEG_Y * (0.024 * t * t)
                    });
                    let width = (0.052 + 0.004 * row as f32) * 0.5;
                    let radii = [
                        Vec2::new(width * 0.4, 0.006),
                        Vec2::new(width, 0.009),
                        Vec2::new(width * 0.62, 0.006),
                        Vec2::splat(0.0015),
                    ];
                    let across = Vec3::new(-longitude.sin(), 0.0, longitude.cos());
                    let shade = 0.82 + 0.05 * row as f32 / 6.0 + 0.26 * rand(82, n);
                    p.matte.tube_framed(
                        &path,
                        &radii,
                        6,
                        &|_, _, t| l.skin * shade * (1.0 - 0.13 * t),
                        Some(across),
                    );
                }
            }
            for k in 0..7 {
                let a = (k as f32 - 3.0) * 0.28;
                feather(
                    p,
                    Vec3::new(a * 0.08, 0.08, -0.3),
                    Vec3::new(a * 0.4, 0.0, -1.0).normalize(),
                    0.24,
                    0.048,
                    l.skin * (0.85 + 0.05 * k as f32),
                );
            }
        },
    );
    let neck = kit.joint(J::Neck, body, Vec3::new(0.0, 0.12, 0.3), "neck", |p| {
        use crate::sdf::{Sculpt, Shape::*};
        // A continuous throat descends into the feathered breast. The
        // tapered S-neck emerges from that mass instead of ending as a pipe.
        let mut throat = Sculpt::default();
        throat.add(
            Ellipsoid(Vec3::new(0.0, -0.025, 0.035), Vec3::new(0.095, 0.09, 0.09)),
            0.0,
        );
        let path = [
            Vec3::ZERO,
            Vec3::new(0.0, 0.16, 0.08),
            Vec3::new(0.0, 0.32, 0.03),
            Vec3::new(0.0, 0.47, 0.08),
            Vec3::new(0.0, 0.5, 0.1),
        ];
        let radii = [0.078, 0.06, 0.048, 0.043, 0.04];
        for i in 0..path.len() - 1 {
            throat.add(Capsule(path[i], path[i + 1], radii[i], radii[i + 1]), 0.02);
        }
        throat.mesh(
            &mut p.matte,
            Vec3::new(-0.13, -0.14, -0.1),
            Vec3::new(0.13, 0.56, 0.19),
            0.007,
            &|_, _| l.cloth,
        );
        // A ruff of feathers where the neck meets the body.
        for k in 0..12 {
            let a = k as f32 * TAU / 12.0;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            feather(
                p,
                out * 0.06 + Vec3::Y * 0.06,
                out,
                0.11,
                0.045,
                l.skin * (0.9 + 0.2 * rand(83, k)),
            );
        }
    });
    kit.joint(J::Head, neck, Vec3::new(0.0, 0.47, 0.08), "head", |p| {
        // Scale the complete authored skull/eye/beak assembly coherently;
        // the neck pivot stays fixed at the occipital attachment.
        p.set_transform(Affine3A::from_scale(Vec3::splat(1.5)));
        use crate::sdf::{Sculpt, Shape::*};
        let mut skull = Sculpt::default();
        skull
            .add(
                Ellipsoid(Vec3::new(0.0, 0.03, 0.02), Vec3::new(0.06, 0.058, 0.075)),
                0.0,
            )
            .add(
                Ellipsoid(Vec3::new(0.0, 0.005, -0.007), Vec3::new(0.047, 0.05, 0.055)),
                0.02,
            );
        let eye_frames = animal_eye_sockets(
            &mut skull,
            Vec3::new(0.0, 0.047, 0.043),
            0.048,
            0.014,
            Vec3::new(0.88, 0.08, 0.48),
        );
        skull.mesh(
            &mut p.matte,
            Vec3::new(-0.085, -0.06, -0.08),
            Vec3::new(0.085, 0.11, 0.115),
            0.0045,
            &|_, _| l.cloth,
        );
        p.matte.prism(
            Vec3::new(0.0, 0.02, 0.07),
            Vec3::new(0.0, 0.0, 0.17),
            0.028,
            0.006,
            6,
            DARK * 2.5,
            0.0,
        );
        p.matte.prism(
            Vec3::new(0.0, 0.005, 0.07),
            Vec3::new(0.0, -0.005, 0.15),
            0.018,
            0.004,
            5,
            DARK * 2.0,
            0.0,
        );
        animal_eyes(
            p,
            eye_frames,
            0.014,
            Vec3::new(0.72, 0.4, 0.08),
            l.cloth,
            None,
        );
        // A shaggy dark crest.
        hair_tufts(
            p,
            Vec3::new(0.0, 0.08, -0.01),
            0.04,
            7,
            Vec3::new(0.0, 0.6, -1.0),
            l.skin * 0.55,
            19,
        );
    });
    for (side, leg, shin, foot) in [
        (1.0f32, J::LegL, J::ShinL, J::FootL),
        (-1.0, J::LegR, J::ShinR, J::FootR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let thigh = kit.joint(leg, body, BirdBody::hip(side), &format!("leg{tag}"), |p| {
            p.matte.blob(
                Vec3::new(0.0, -0.065, 0.02),
                Vec3::new(0.105, 0.16, 0.12),
                3,
                0.15,
                l.skin * 0.85,
            );
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, -0.09, 0.02),
                    BirdBody::UPPER * 0.65,
                    BirdBody::UPPER,
                ],
                &[(0.066, 0.075), (0.058, 0.062), (0.046, 0.05)],
                l.trim,
            );
        });
        let lower = kit.joint(shin, thigh, BirdBody::UPPER, &format!("shin{tag}"), |p| {
            p.matte
                .blob(Vec3::ZERO, Vec3::new(0.056, 0.06, 0.06), 2, 0.0, l.trim);
            tube(
                &mut p.matte,
                &[
                    Vec3::Y * 0.025,
                    Vec3::ZERO,
                    BIRD_ANKLE * 0.35,
                    BIRD_ANKLE * 0.75,
                    BIRD_ANKLE,
                ],
                &[
                    (0.044, 0.048),
                    (0.05, 0.052),
                    (0.043, 0.045),
                    (0.034, 0.036),
                    (0.03, 0.033),
                ],
                l.trim * 0.95,
            );
            for k in 0..6 {
                let t = 0.15 + k as f32 * 0.13;
                p.matte.torus(
                    BIRD_ANKLE * t,
                    BIRD_ANKLE.normalize(),
                    0.05 - 0.02 * t,
                    0.0028,
                    8,
                    3,
                    l.trim * 0.8,
                );
            }
        });
        let foot_transform = reparent_mesh_transform(Plan::Bird, shin, foot, BIRD_ANKLE);
        kit.joint(foot, lower, BIRD_ANKLE, &format!("foot{tag}"), |p| {
            // A broad palm and three long curved toes share the articulated
            // ankle frame; the native sole stays independent of shin pitch.
            p.set_transform(foot_transform);
            p.matte.blob(
                BIRD_ANKLE + Vec3::new(0.0, -0.015, 0.025),
                Vec3::new(0.07, 0.025, 0.075),
                2,
                0.0,
                l.trim,
            );
            for k in -1..=1 {
                let a = k as f32 * 0.68;
                let toe = BIRD_ANKLE + Vec3::new(a.sin() * 0.15, -0.032, a.cos() * 0.17);
                let root = BIRD_ANKLE + Vec3::new(k as f32 * 0.035, -0.012, 0.025);
                tube(
                    &mut p.matte,
                    &[root, root.lerp(toe, 0.5) + Vec3::Y * 0.009, toe],
                    &[(0.028, 0.023), (0.024, 0.018), (0.017, 0.014)],
                    l.trim * 0.8,
                );
                p.matte.prism(
                    toe,
                    toe + Vec3::new(a.sin(), -0.2, a.cos()) * 0.04,
                    0.014,
                    0.0,
                    4,
                    DARK * 3.0,
                    0.0,
                );
            }
        });
    }
}
