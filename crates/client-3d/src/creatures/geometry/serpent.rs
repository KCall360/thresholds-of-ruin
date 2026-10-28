use super::*;

/// A banded snake: a smooth tapering body laid along a travelling wave, a
/// wedge-shaped head with a jaw, and a tongue that flickers.
pub(super) fn serpent(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let rattler = l.style == Style::Rattlesnake;
    for i in 0..SNAKE {
        let r = snake_segment_radius(i);
        let model = kit.part(&format!("seg{i}"), |p| {
            if i == 0 {
                serpent_skull(p, &l);
            } else if rattler && i >= SNAKE - 3 {
                // The rattle: stacked hollow segments, each a ridged ring.
                let k = (i - (SNAKE - 3)) as f32;
                let tone = Vec3::new(0.86, 0.74, 0.5) * (1.0 - 0.06 * k);
                for ring in 0..3 {
                    let z = (ring as f32 - 1.0) * 0.022;
                    p.satin.blob(
                        Vec3::new(0.0, 0.0, z),
                        Vec3::new(0.034 - k * 0.004, 0.026 - k * 0.003, 0.016),
                        ring,
                        0.0,
                        tone * (0.9 + 0.05 * ring as f32),
                    );
                }
            } else if rattler {
                // Dark diamonds outlined in cream down the olive back.
                let half_length = (r * 1.9).max(0.052);
                p.satin.blob_painted(
                    Vec3::ZERO,
                    Vec3::new(r, r * 0.75, half_length),
                    i as u64,
                    0.0,
                    &|normal, at| {
                        let phase = (i % 2) as f32 * half_length;
                        let diamond = at.x.abs() / (r * 0.85)
                            + ((at.z + phase).rem_euclid(half_length * 2.0) - half_length).abs()
                                / (half_length * 0.9);
                        if normal.y < -0.25 {
                            l.trim
                        } else if normal.y > -0.1 && diamond < 0.75 {
                            Vec3::new(0.24, 0.17, 0.1)
                        } else if normal.y > -0.1 && diamond < 0.95 {
                            l.cloth
                        } else {
                            l.skin
                        }
                    },
                );
            } else {
                // A painted belly and flush dorsal diamonds share the same
                // surface as the banded hide. No floating blocks on the spine.
                let band = (i / 2) % 2 == 1;
                let c = if band { l.cloth } else { l.skin };
                let half_length = (r * 1.9).max(0.052);
                p.satin.blob_painted(
                    Vec3::ZERO,
                    Vec3::new(r, r * 0.75, half_length),
                    i as u64,
                    0.0,
                    &|normal, at| {
                        let diamond = at.x.abs() / (r * 0.7) + at.z.abs() / (half_length * 0.6);
                        if normal.y < -0.25 {
                            l.trim
                        } else if i % 2 == 0 && normal.y > 0.35 && diamond < 1.0 {
                            l.skin * 0.68
                        } else {
                            c
                        }
                    },
                );
            }
        });
        let e = kit
            .commands
            .spawn((Transform::default(), Visibility::default()))
            .id();
        kit.commands.entity(e).with_children(|p| model.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
    // The tongue rides on the head and flicks out now and then.
    let head = kit.rig.segments[0];
    let dimensions = super::anatomy::SerpentHead::resolve(l.style);
    let jaw = kit.joint(J::Jaw, head, dimensions.hinge(), "jaw", |p| {
        serpent_jaw(p, &l);
    });
    kit.joint(J::Fangs, head, dimensions.fangs(), "fangs", |p| {
        p.set_transform(Affine3A::from_scale(dimensions.scale));
        beast_parts::FangArc {
            frame: Transform::from_xyz(0.0, 0.0, -1.3f32.cos() * 0.11)
                .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            half_width: 0.068,
            rise: 0.11,
            depth: 0.0,
            growth: Vec3::Z,
            length: if l.style == Style::Rattlesnake {
                0.08
            } else {
                0.058
            },
            radius: 0.008,
            count: 2,
        }
        .emit(&mut p.matte, BONE);
    });
    let tongue = kit.part("tongue", |p| {
        let red = Vec3::new(0.7, 0.12, 0.12);
        p.matte.prism(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 0.08),
            0.006,
            0.005,
            4,
            red,
            0.0,
        );
        for side in [-1.0f32, 1.0] {
            p.matte.prism(
                Vec3::new(0.0, 0.0, 0.08),
                Vec3::new(side * 0.015, 0.0, 0.11),
                0.004,
                0.0,
                3,
                red,
                0.0,
            );
        }
    });
    let e = kit
        .commands
        .spawn((
            Transform::from_translation(dimensions.tongue()),
            Visibility::default(),
        ))
        .id();
    kit.commands.entity(e).with_children(|p| tongue.attach(p));
    kit.commands.entity(jaw).add_child(e);
    kit.rig.joints[J::Snout as usize] = Some(e);
    kit.rig.rest[J::Snout as usize] = dimensions.tongue();
}

/// Both snakes share a broad sculpted skull, seated eyes and a separate jaw.
/// The pit viper adds broad rear cheek masses; ornamental size cannot stand in
/// for the head itself. Teeth reuse the same mouth-frame rows as other beasts.
pub(super) fn serpent_skull(p: &mut Parts, l: &Look) {
    use crate::sdf::{Sculpt, Shape::*};
    let dimensions = super::anatomy::SerpentHead::resolve(l.style);
    p.set_transform(Affine3A::from_scale(dimensions.scale));
    let mut skull = Sculpt::default();
    skull
        .add(
            Ellipsoid(Vec3::new(0.0, 0.02, 0.005), Vec3::new(0.095, 0.07, 0.115)),
            0.0,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.005, 0.13), Vec3::new(0.074, 0.043, 0.09)),
            0.025,
        );
    for side in [-1.0f32, 1.0] {
        if l.style == Style::Rattlesnake {
            skull.add(
                Ellipsoid(
                    Vec3::new(side * 0.055, 0.002, -0.025),
                    Vec3::new(0.05, 0.052, 0.075),
                ),
                0.015,
            );
        }
        skull.add(
            Ellipsoid(
                Vec3::new(side * 0.06, 0.052, 0.075),
                Vec3::new(0.03, 0.025, 0.045),
            ),
            0.018,
        );
    }
    skull.carve(Beyond(Vec3::NEG_Y, 0.018), 0.0);
    let eyes = animal_eye_sockets(
        &mut skull,
        Vec3::new(0.0, 0.036, 0.09),
        0.078,
        0.022,
        Vec3::new(0.84, 0.10, 0.54),
    );
    skull.mesh(
        &mut p.satin,
        Vec3::new(-0.13, -0.022, -0.13),
        Vec3::new(0.13, 0.125, 0.23),
        0.005,
        &|at, normal| {
            if normal.y < -0.35 && at.z > 0.025 {
                Vec3::new(0.42, 0.085, 0.11)
            } else {
                l.skin
            }
        },
    );
    animal_eyes(p, eyes, 0.022, Vec3::new(0.86, 0.57, 0.1), l.skin, None);
    for side in [-1.0f32, 1.0] {
        p.matte.blob(
            Vec3::new(side * 0.032, 0.014, 0.208),
            Vec3::new(0.005, 0.0035, 0.003),
            0,
            0.0,
            DARK,
        );
    }
    let frame =
        Transform::from_xyz(0.0, -0.016, 0.055).with_rotation(Quat::from_rotation_x(FRAC_PI_2));
    beast_parts::FangArc {
        frame,
        half_width: 0.068,
        rise: 0.11,
        depth: 0.0,
        growth: Vec3::Z,
        length: 0.027,
        radius: 0.004,
        count: 7,
    }
    .emit(&mut p.matte, BONE);
}

pub(super) fn serpent_jaw(p: &mut Parts, l: &Look) {
    use crate::sdf::{Sculpt, Shape::*};
    let dimensions = super::anatomy::SerpentHead::resolve(l.style);
    p.set_transform(
        Affine3A::from_translation(-dimensions.hinge()) * Affine3A::from_scale(dimensions.scale),
    );
    let mut jaw = Sculpt::default();
    jaw.add(
        Ellipsoid(
            Vec3::new(0.0, -0.038, 0.085),
            Vec3::new(0.074, 0.023, 0.132),
        ),
        0.0,
    );
    jaw.mesh(
        &mut p.satin,
        Vec3::new(-0.083, -0.066, -0.055),
        Vec3::new(0.083, -0.01, 0.225),
        0.005,
        &|_, normal| {
            if normal.y > 0.25 {
                Vec3::new(0.48, 0.10, 0.14)
            } else {
                l.trim
            }
        },
    );
}

/// Segments in a snake's body.
pub(in crate::creatures) const SNAKE: usize = 20;

/// Horizontal radius shared by the tapered mesh and its floor placement.
/// Body sections are three quarters as high as they are wide; the separately
/// sculpted head has its own low jaw profile.
pub(in crate::creatures) fn snake_segment_radius(index: usize) -> f32 {
    if index == 0 {
        0.08
    } else {
        let fraction = index as f32 / SNAKE as f32;
        0.093 * (1.0 - fraction).powf(0.72) + 0.005
    }
}

/// A crystal has broad shaded faces and a sharply terminating cap. Emission
/// belongs at the tip; a completely glowing cone loses all of its volume.
pub(super) fn ice_shard(p: &mut Parts, root: Vec3, tip: Vec3, width: f32, color: Vec3) {
    let shoulder = root.lerp(tip, 0.68);
    let cap = root.lerp(tip, 0.9);
    p.satin
        .prism(root, shoulder, width * 0.55, width, 6, color, 0.0);
    p.satin
        .prism(shoulder, cap, width, width * 0.3, 6, color * 1.15, 0.0);
    p.glow.prism(cap, tip, width * 0.3, 0.0, 6, Vec3::ONE, 0.0);
}
