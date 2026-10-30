use super::*;

/// Blue-violet charcoal keeps a black coat readable under ordinary light.
/// The dorsal planes carry a little more pigment value; this is not emission.
pub(super) fn equine_hide(base: Vec3, style: Style, normal: Vec3) -> Vec3 {
    if style == Style::Unicorn {
        let dorsal = (normal.y * 0.5 + 0.5).clamp(0.0, 1.0);
        base.lerp(Vec3::new(0.23, 0.25, 0.34), dorsal * 0.3)
    } else {
        base
    }
}

/// Horses and their kin; with `centaur`, a rider's torso rises from the withers.
pub(super) fn quadruped(kit: &mut Kit, lift: Entity, look: &Look, centaur: bool) {
    let l = look.clone();
    let style = l.style;
    let quagga = style == Style::Quagga;
    // All four unposed soles meet the floor; movement crouches this linked
    // rest height rather than inheriting unrelated horse-length dimensions.
    let equine = super::anatomy::EquineBody::resolve(&l);
    let body_height = super::anatomy::EquineBody::BODY_HEIGHT;
    kit.rig.ankle_floor = super::anatomy::EquineBody::ANKLE_HEIGHT;
    kit.rig.legs = (super::anatomy::EquineBody::upper(true).length()
        + super::anatomy::EquineBody::lower(true).length())
        / 0.84;
    // Shorter supports take shorter distance steps at the same travel speed.
    kit.rig.stride = kit.rig.stride.max(0.85 / kit.rig.legs);
    kit.rig.girth = 0.32 * equine.breadth;
    // A quagga's coat: chestnut behind, cream and brown stripes over the
    // forequarters, neck and face, a pale belly. Painted, not banded.
    let coat = move |dir: Vec3, at: Vec3| -> Vec3 {
        if !quagga {
            return equine_hide(l.skin, style, dir);
        }
        let dark = Vec3::new(0.36, 0.2, 0.1);
        let cream = l.cloth;
        // A cream chest and belly; stripes band the sides.
        if dir.y < -0.5 || (dir.z > 0.45 && at.z > 0.2) {
            return cream;
        }
        let front = ease((at.z + 0.1) / 0.5);
        // Each quagga's stripes are its own: spacing, slant and phase.
        let pv = f32::from(l.v.pattern);
        let wave = (at.z * (30.0 + 3.0 * pv)
            + at.y * (4.0 + 2.5 * (pv - 1.5))
            + (at.y * 5.0).sin() * 1.2
            + pv * 1.7)
            .sin();
        let striped = cream.lerp(dark, ((wave - 0.15) * 6.0).clamp(0.0, 1.0));
        l.skin.lerp(striped, front)
    };
    let body = kit.joint(
        J::Body,
        lift,
        Vec3::new(0.0, body_height, 0.0),
        "body",
        |p| {
            // One continuous hide over the rib cage, scapulae and haunches. The
            // limbs emerge inside these masses; separate round shoulder patches
            // make every horse, unicorn and centaur look assembled from toys.
            use crate::sdf::{Sculpt, Shape::*};
            let wide = Vec3::new(equine.breadth, 1.0, 1.0);
            let mut barrel = Sculpt::default();
            barrel
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, -0.015, -0.015),
                        Vec3::new(0.215, 0.265, 0.43) * wide,
                    ),
                    0.0,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, -0.055, 0.30),
                        Vec3::new(0.18, 0.245, 0.20) * wide,
                    ),
                    0.075,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, 0.025, -0.36),
                        Vec3::new(0.225, 0.245, 0.215) * wide,
                    ),
                    0.075,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, 0.18, 0.25),
                        Vec3::new(0.105, 0.09, 0.19) * wide,
                    ),
                    0.06,
                );
            for side in [-1.0f32, 1.0] {
                barrel
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.09 * equine.breadth, -0.10, 0.34),
                            Vec3::new(0.11, 0.18, 0.14) * wide,
                        ),
                        0.05,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.11 * equine.breadth, 0.015, 0.29),
                            Vec3::new(0.125, 0.22, 0.17) * wide,
                        ),
                        0.075,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.125 * equine.breadth, 0.015, -0.35),
                            Vec3::new(0.14, 0.24, 0.205) * wide,
                        ),
                        0.075,
                    );
            }
            barrel.mesh(
                &mut p.matte,
                Vec3::new(-0.37, -0.38, -0.65),
                Vec3::new(0.37, 0.34, 0.62),
                0.012,
                &|at, normal| coat(normal, at),
            );
        },
    );
    let neck_at = Vec3::new(0.0, 0.12, 0.45);
    if centaur {
        let torso = kit.joint(J::Torso, body, Vec3::new(0.0, 0.15, 0.42), "torso", |p| {
            // One torso rising out of the withers: waist, ribs, chest.
            let fem = l.v.woman;
            let w = if fem { 0.88 } else { 1.0 };
            // Her chest is fitted to the rider's own rib cage, as a biped's is.
            let bust = super::anatomy::centaur_bust(&l);
            // Her leather wrap: from under the chest to a sweetheart edge
            // over the upper slope, dipping between the two sides.
            let wrap = bust.map(|bust| {
                let apex = bust.apex(1.0);
                let low = bust.fold - 0.05;
                let high = apex.y + bust.radii.y * 0.62;
                let dip = apex.y - 0.005;
                move |x: f32| {
                    let t = (x.abs() / apex.x).min(1.6);
                    let edge = if t < 1.0 {
                        dip + (high - dip) * (1.0 - (1.0 - t).powi(2))
                    } else {
                        high - (t - 1.0) * 0.06
                    };
                    (low, edge)
                }
            });
            if let Some(bust) = bust {
                for side in [-1.0f32, 1.0] {
                    let (center, radii) = bust.mass(side, 0.0);
                    p.soft.push(crate::props::SoftRegion {
                        center,
                        radii,
                        facing: 1.0,
                        give: 0.45 * (0.5 + 0.6 * bust.volume),
                    });
                }
            }
            let body_shape = {
                // Sculpted, as the people's torsos are: the waist rising out
                // of the withers into ribs, chest and rounded shoulders.
                use crate::sdf::{Sculpt, Shape::*};
                let mut body = Sculpt::default();
                body.add(
                    // The lower human root sits inside the withers instead
                    // of hanging out over the horse's breast below the belt.
                    Ellipsoid(Vec3::new(0.0, -0.1, -0.055), Vec3::new(0.15, 0.15, 0.12)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.15 * w, 0.14, 0.12)),
                    0.06,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.3, 0.005), Vec3::new(0.2 * w, 0.17, 0.13)),
                    0.06,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.44, -0.01), Vec3::new(0.2 * w, 0.1, 0.11)),
                    0.14,
                )
                .add(
                    Capsule(
                        Vec3::new(0.0, 0.5, -0.005),
                        Vec3::new(0.0, 0.6, -0.005),
                        0.065,
                        0.055,
                    ),
                    0.04,
                );
                // Deltoids belong to the animated arms; the rib/clavicle
                // support stays on the torso without retaining lateral caps.
                if let Some(bust) = bust {
                    bust_supports(&mut body, &bust, 0.0);
                } else {
                    // The male rider has the same broad continuous chest
                    // plane as the bipeds, with no paired forward cups.
                    body.add(
                        Ellipsoid(Vec3::new(0.0, 0.38, 0.015), Vec3::new(0.185, 0.1, 0.1)),
                        0.04,
                    )
                    .add(
                        Ellipsoid(Vec3::new(0.0, 0.17, 0.075), Vec3::new(0.07, 0.09, 0.04)),
                        0.04,
                    );
                }
                body.mesh(
                    &mut p.matte,
                    Vec3::new(-0.34, -0.3, -0.22),
                    Vec3::new(0.34, 0.64, 0.27),
                    0.0085,
                    &|at, _| {
                        // The wrapped leather follows the sculpted chest and
                        // waist, preserving the figure instead of a rigid tube.
                        let wrapped = wrap.is_some_and(|edges| {
                            let (low, high) = edges(at.x);
                            at.y > low && at.y < if at.z > 0.0 { high } else { 0.41 }
                        });
                        if at.y < 0.035 {
                            // The coat transition is concealed by the belt;
                            // any overlap below it belongs to the equine body.
                            l.skin
                        } else if wrapped {
                            LEATHER
                        } else {
                            l.cloth
                        }
                    },
                );
                body
            };
            if let Some(edges) = wrap {
                // Its edges are turned and stitched, top and bottom.
                for top in [true, false] {
                    let at = |x: f32| {
                        let (low, high) = edges(x);
                        (Vec2::new(x, if top { high } else { low }), 0.012)
                    };
                    fitted_ribbon(
                        &mut p.matte,
                        &body_shape,
                        &[-0.16, -0.12, -0.08, -0.04, 0.0, 0.04, 0.08, 0.12, 0.16].map(at),
                        true,
                        0.004,
                        LEATHER * 0.7,
                    );
                }
            }
            // A close-fitting diagonal baldric follows the chest and back;
            // it never projects as a rectangular plank past the shoulders.
            for front in [false, true] {
                fitted_ribbon(
                    &mut p.satin,
                    &body_shape,
                    &[
                        (Vec2::new(-0.105, 0.1), 0.026),
                        (Vec2::new(-0.055, 0.24), 0.026),
                        (Vec2::new(0.04, 0.37), 0.026),
                        (Vec2::new(0.15, 0.48), 0.025),
                    ],
                    front,
                    0.01,
                    LEATHER * 0.72,
                );
            }
            let (buckle, normal) = torso_surface_frame(&body_shape, Vec2::new(0.04, 0.37), true);
            p.metal.blob(
                buckle + normal * 0.018,
                Vec3::splat(0.018),
                0,
                0.0,
                Vec3::new(0.74, 0.5, 0.26),
            );
            tube(
                &mut p.satin,
                &[Vec3::Y * 0.015, Vec3::Y * 0.035, Vec3::Y * 0.055],
                &[(0.164, 0.15), (0.16, 0.146), (0.157, 0.143)],
                LEATHER * 0.72,
            );
        });
        kit.joint(J::Head, torso, Vec3::Y * 0.6, "head", |p| {
            // The rider's face and hair are a person's; men wear full beards.
            let rider = Look {
                skin: l.cloth,
                ..l.clone()
            };
            biped_neck(&mut p.matte, 1.0, neck_slim(rider.v.woman), l.cloth);
            p.set_transform(
                Affine3A::from_translation(Vec3::Y * 0.08)
                    * Affine3A::from_scale(Vec3::splat(1.1))
                    * Affine3A::from_translation(Vec3::Y * -0.08),
            );
            human_face(p, &rider, true);
            hair(p, &rider, true);
            if !rider.v.woman {
                beard(p, hair_color(&rider));
            }
        });
        let arms = super::anatomy::RiderArms::resolve(&l);
        for (side, arm, fore, hand) in [
            (1.0f32, J::ArmL, J::ForeL, J::HandL),
            (-1.0, J::ArmR, J::ForeR, J::HandR),
        ] {
            let tag = if side > 0.0 { "l" } else { "r" };
            let reach = arms.reach;
            let upper = kit.joint(
                arm,
                torso,
                Vec3::new(side * reach, 0.48, 0.0),
                &format!("arm{tag}"),
                |p| {
                    organic_upper_arm(
                        &mut p.matte,
                        side,
                        arms.width,
                        1.0,
                        arms.muscle,
                        true,
                        l.cloth,
                    );
                },
            );
            let lower = kit.joint(fore, upper, Vec3::Y * -0.3, &format!("fore{tag}"), |p| {
                organic_forearm(&mut p.matte, arms.width, 1.0, arms.muscle, true, l.cloth);
                p.satin.prism(
                    Vec3::Y * -0.13,
                    Vec3::Y * -0.25,
                    0.058 * arms.width,
                    0.052 * arms.width,
                    10,
                    LEATHER,
                    0.0,
                );
                fist(p, Vec3::Y * -0.3, side, arms.width, l.cloth);
            });
            let grip = kit
                .commands
                .spawn((
                    Transform::from_translation(Vec3::Y * -0.31),
                    Visibility::default(),
                ))
                .id();
            kit.commands.entity(lower).add_child(grip);
            kit.rig.joints[hand as usize] = Some(grip);
            // Without its rest offset the grip would be posed at the elbow.
            kit.rig.rest[hand as usize] = Vec3::Y * -0.31;
        }
        let hand = kit.rig.joint(J::HandR).unwrap();
        kit.attach(
            hand,
            Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            "spear",
            |p| {
                p.matte
                    .prism(Vec3::Y * -0.7, Vec3::Y * 0.9, 0.02, 0.02, 6, WOOD, 0.0);
                p.satin
                    .prism(Vec3::Y * 0.78, Vec3::Y * 0.9, 0.024, 0.024, 6, LEATHER, 0.0);
                p.metal
                    .prism(Vec3::Y * 0.9, Vec3::Y * 1.14, 0.045, 0.0, 4, STEEL, 0.0);
                p.matte.prism(
                    Vec3::Y * 0.86,
                    Vec3::new(0.05, 0.7, 0.0),
                    0.012,
                    0.0,
                    3,
                    Vec3::new(0.6, 0.15, 0.1),
                    0.0,
                );
            },
        );
    } else {
        let neck = kit.joint(J::Neck, body, neck_at, "neck", |p| {
            let top = Vec3::new(0.0, 0.36, 0.3);
            let neck_coat = |dir: Vec3, at: Vec3| -> Vec3 {
                if quagga {
                    let wave = (at.y * 52.0 + at.z * 12.0).sin();
                    l.cloth.lerp(
                        Vec3::new(0.38, 0.21, 0.11),
                        ((wave - 0.05) * 3.0).clamp(0.0, 1.0),
                    )
                } else {
                    equine_hide(l.skin, style, dir)
                }
            };
            // The crest, throat and root form one angled neck. Its broad
            // root continues into the chest, covering the articulated join.
            use crate::sdf::{Sculpt, Shape::*};
            let mut neck_shape = Sculpt::default();
            neck_shape
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.045, -0.06), Vec3::new(0.125, 0.18, 0.165)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.1, 0.065), Vec3::new(0.112, 0.19, 0.15)),
                    0.055,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.245, 0.19), Vec3::new(0.085, 0.15, 0.12)),
                    0.045,
                )
                .add(Ellipsoid(top, Vec3::new(0.074, 0.095, 0.09)), 0.035);
            neck_shape.mesh(
                &mut p.matte,
                Vec3::new(-0.17, -0.25, -0.25),
                Vec3::new(0.17, 0.49, 0.42),
                0.009,
                &|at, normal| {
                    if quagga && normal.z > 0.7 && at.y < 0.32 {
                        l.cloth
                    } else {
                        neck_coat(normal, at)
                    }
                },
            );
            let mane = if style == Style::Unicorn {
                l.cloth
            } else {
                l.trim
            };
            // The mane: locks along the crest. A unicorn's falls long and
            // sweeps to one side; a quagga's stands up in a short brush.
            let locks = if style == Style::Unicorn { 20 } else { 18 };
            for k in 0..locks {
                let t = k as f32 / (locks - 1) as f32;
                // Along the crest, the top of the arched neck.
                let crest = Vec3::new(0.0, 0.06 + t * 0.36, -0.16 + t * 0.4);
                let root = neck_shape.onto(crest - Vec3::Z * 0.18, Vec3::Z);
                let shade = mane * (0.8 + 0.35 * rand(5, k));
                if style == Style::Unicorn {
                    let side = if k % 3 == 0 { -1.0 } else { 1.0 };
                    let len = 0.26 + 0.12 * rand(6, k);
                    let curl = 0.03 * (k as f32 * 1.3).sin();
                    // Each lock lifts off the crest, then falls close along the neck.
                    let path = [
                        root,
                        root + Vec3::new(side * 0.03, 0.05, -0.05),
                        root + Vec3::new(side * 0.08, 0.0, -0.09),
                        root + Vec3::new(side * 0.1 + curl, -len * 0.55, -0.1 + 0.05 * rand(7, k)),
                        root + Vec3::new(side * 0.09 - curl + 0.02 * rand(8, k), -len, -0.06),
                    ];
                    let r = 0.03 + 0.012 * rand(9, k);
                    tube(
                        &mut p.matte,
                        &path,
                        &[
                            (r, r * 0.7),
                            (r * 1.1, r * 0.8),
                            (r, r * 0.7),
                            (r * 0.6, r * 0.5),
                            (0.003, 0.003),
                        ],
                        shade,
                    );
                } else {
                    let path = [
                        root,
                        root + Vec3::new(0.0, 0.06, -0.035),
                        root + Vec3::new(0.0, 0.1, -0.06),
                    ];
                    tube(
                        &mut p.matte,
                        &path,
                        &[(0.016, 0.03), (0.014, 0.026), (0.002, 0.004)],
                        shade,
                    );
                }
            }
        });
        kit.joint(J::Snout, neck, Vec3::new(0.0, 0.4, 0.31), "head", |p| {
            let face = |normal: Vec3, at: Vec3| -> Vec3 {
                if quagga && (at.z * 30.0 - at.y * 20.0).sin() > 0.35 && at.z < 0.2 {
                    Vec3::new(0.38, 0.21, 0.11)
                } else if quagga {
                    l.cloth
                } else {
                    equine_hide(l.skin, style, normal)
                }
            };
            // A touch larger than life, as the renders draw it.
            p.set_transform(Affine3A::from_scale(Vec3::splat(1.28)));
            use crate::sdf::{Sculpt, Shape::*};
            let mut skull = Sculpt::default();
            skull
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.01, 0.03), Vec3::new(0.093, 0.105, 0.13)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.018, 0.16), Vec3::new(0.079, 0.072, 0.16)),
                    0.03,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.075, 0.16), Vec3::new(0.070, 0.047, 0.15)),
                    0.025,
                );
            for side in [-1.0f32, 1.0] {
                skull
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.052, -0.035, 0.03),
                            Vec3::new(0.039, 0.057, 0.077),
                        ),
                        0.025,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.061, 0.042, 0.057),
                            Vec3::new(0.027, 0.024, 0.043),
                        ),
                        0.016,
                    );
            }
            skull
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.055, 0.29), Vec3::new(0.083, 0.066, 0.095)),
                    0.04,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.105, 0.235), Vec3::new(0.067, 0.038, 0.12)),
                    0.025,
                );
            let eye_frames = animal_eye_sockets(
                &mut skull,
                Vec3::new(0.0, 0.034, 0.076),
                0.077,
                0.018,
                Vec3::new(0.92, 0.08, 0.38),
            );
            skull.mesh(
                &mut p.matte,
                Vec3::new(-0.14, -0.17, -0.12),
                Vec3::new(0.14, 0.15, 0.405),
                0.006,
                &|at, normal| {
                    if at.z > 0.275 && at.y < -0.025 {
                        if quagga {
                            DARK * 3.0
                        } else {
                            equine_hide(l.skin, style, normal) * 0.88
                        }
                    } else {
                        face(normal, at)
                    }
                },
            );
            for side in [-1.0f32, 1.0] {
                p.matte.blob(
                    skull.onto(Vec3::new(side * 0.043, -0.045, 0.46), -Vec3::Z) + Vec3::Z * 0.003,
                    Vec3::new(0.012, 0.016, 0.01),
                    0,
                    0.0,
                    DARK,
                );
                // Leaf-shaped ears, cupped forward, darker inside.
                let base = Vec3::new(side * 0.045, 0.08, -0.01);
                let ear = [
                    base,
                    base + Vec3::new(side * 0.01, 0.07, 0.0),
                    base + Vec3::new(side * 0.02, 0.14, -0.03),
                ];
                tube(
                    &mut p.matte,
                    &ear,
                    &[(0.034, 0.016), (0.03, 0.014), (0.004, 0.004)],
                    l.skin,
                );
                tube(
                    &mut p.matte,
                    &ear.map(|v| v + Vec3::Z * 0.008),
                    &[(0.022, 0.007), (0.019, 0.007), (0.002, 0.002)],
                    l.skin * 0.55,
                );
            }
            // The forelock spills over the brow.
            let lock = if style == Style::Unicorn {
                l.cloth
            } else {
                l.trim
            };
            hair_tufts(
                p,
                Vec3::new(0.0, 0.1, 0.0),
                0.05,
                5,
                Vec3::new(0.0, -0.3, 1.0),
                lock,
                11,
            );
            animal_eyes(
                p,
                eye_frames,
                0.018,
                Vec3::new(0.44, 0.24, 0.09),
                l.skin,
                l.eyes,
            );
            if style == Style::Unicorn {
                p.accent = Some(Accent::Violet);
                let (a, b) = (Vec3::new(0.0, 0.09, 0.07), Vec3::new(0.0, 0.4, 0.28));
                p.metal
                    .prism(a, b, 0.034, 0.0, 10, Vec3::new(0.12, 0.1, 0.16), 0.0);
                for k in 0..4 {
                    let t = 0.12 + k as f32 * 0.2;
                    p.glow.torus(
                        a.lerp(b, t),
                        (b - a).normalize(),
                        0.034 * (1.0 - t) + 0.005,
                        0.005,
                        12,
                        3,
                        Vec3::ONE,
                    );
                }
                hair_tufts(
                    p,
                    Vec3::new(0.0, -0.12, 0.27),
                    0.025,
                    4,
                    Vec3::new(0.0, -1.0, -0.2),
                    l.cloth,
                    61,
                );
            }
        });
    }
    kit.joint(J::Tail, body, Vec3::new(0.0, 0.1, -0.50), "tail", |p| {
        let c = if style == Style::Unicorn {
            l.cloth
        } else {
            l.trim
        };
        tube(
            &mut p.matte,
            &[
                Vec3::ZERO,
                Vec3::new(0.0, -0.06, -0.06),
                Vec3::new(0.0, -0.14, -0.09),
            ],
            &[(0.05, 0.05), (0.04, 0.042), (0.03, 0.03)],
            l.skin,
        );
        // A flowing switch of hair: strands that fan and curl.
        let strands = if style == Style::Unicorn { 12 } else { 9 };
        for k in 0..strands {
            let a = (k as f32 - (strands - 1) as f32 * 0.5) * 0.11;
            let sway = (k as f32 * 1.7).sin() * 0.03;
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, -0.1, -0.08),
                    Vec3::new(a * 0.12, -0.26, -0.15),
                    Vec3::new(a * 0.25 + sway, -0.42, -0.16),
                    Vec3::new(a * 0.32 + sway, -0.56, -0.12),
                ],
                &[(0.034, 0.034), (0.034, 0.03), (0.024, 0.02), (0.002, 0.002)],
                c * (0.8 + 0.4 * rand(13, k)),
            );
        }
    });
    let legs = [
        (J::FL, J::FLs, J::HoofFL, equine.hip(1.0, true), "fl", true),
        (J::FR, J::FRs, J::HoofFR, equine.hip(-1.0, true), "fr", true),
        (
            J::BL,
            J::BLs,
            J::HoofBL,
            equine.hip(1.0, false),
            "bl",
            false,
        ),
        (
            J::BR,
            J::BRs,
            J::HoofBR,
            equine.hip(-1.0, false),
            "br",
            false,
        ),
    ];
    for (thigh, shin, hoof, at, tag, front) in legs {
        let leg_color = if quagga { l.cloth } else { l.skin };
        let upper_at = super::anatomy::EquineBody::upper(front);
        let ankle_at = super::anatomy::EquineBody::lower(front);
        let upper_end = Vec2::new(0.05, 0.058);
        let lower_root = Vec2::new(0.05, 0.056);
        let upper_scale = storybook_part(kit.rig.plan, thigh);
        let lower_scale = storybook_part(kit.rig.plan, shin);
        // A pivot-centered cover contains both scaled end rings as the lower
        // leg turns. Size it from those rings, not the larger muscle above.
        let joint_radius = (upper_end.max_element() * upper_scale.max_element())
            .max(lower_root.max_element() * lower_scale.max_element())
            + 0.0015;
        let joint_radii = Vec3::splat(joint_radius) / upper_scale;
        let root_overlap = lower_root.min_element() * 0.25;
        let upper = kit.joint(thigh, body, at, &format!("leg{tag}"), |p| {
            // A muscled upper leg tapering to a bony knee.
            let (top, mid) = if front { (0.125, 0.092) } else { (0.15, 0.11) };
            let path = [
                Vec3::Y * 0.13,
                upper_at * 0.28 + Vec3::new(0.0, 0.0, if front { 0.005 } else { -0.025 }),
                upper_at * 0.76,
                upper_at,
            ]
            .map(|p| p / upper_scale);
            tube(
                &mut p.matte,
                &path,
                &[
                    (top * 0.6, top * 0.8),
                    (mid * 0.82, mid),
                    (0.056, 0.064),
                    (upper_end.x, upper_end.y),
                ],
                l.skin,
            );
            // Enough rings keep the small cover inside its silhouette margin.
            // The original offset knee/hock landmark remains visible below.
            let resolution = p.matte.res;
            p.matte.res = (18, 32);
            p.matte
                .blob(upper_at / upper_scale, joint_radii, 1, 0.0, l.skin);
            p.matte.res = resolution;
            p.matte.blob(
                upper_at / upper_scale,
                Vec3::new(0.048, 0.054, 0.05),
                1,
                0.0,
                l.skin,
            );
        });
        let lower = kit.joint(shin, upper, upper_at, &format!("shin{tag}"), |p| {
            // Keep the hidden overlap separate: tube paint uses arc length,
            // so inserting it into the cannon would shift the stocking fade.
            p.matte.tube(
                &[Vec3::Y * root_overlap, Vec3::ZERO],
                &[lower_root * 0.85, lower_root],
                12,
                &|_, _, _| l.skin,
            );
            // The stocking fades into the cannon bone instead of ending in
            // a hard sleeve. The fetlock and forward pastern carry the hoof.
            let radii = [
                lower_root,
                Vec2::new(0.035, 0.041),
                Vec2::new(0.043, 0.051),
                Vec2::new(0.038, 0.045),
                Vec2::new(0.045, 0.05),
            ];
            p.matte.tube(
                &[
                    Vec3::ZERO,
                    ankle_at * 0.48 / lower_scale,
                    ankle_at * 0.81 / lower_scale,
                    ankle_at * 0.94 / lower_scale,
                    ankle_at / lower_scale,
                ],
                &radii,
                12,
                &|_, _, t| l.skin.lerp(leg_color, ease((t - 0.08) / 0.28)),
            );
        });
        kit.joint(hoof, lower, ankle_at, &format!("hoof{tag}"), |p| {
            equine_parts::hoof(&mut p.matte, DARK * 2.4);
        });
    }
}
