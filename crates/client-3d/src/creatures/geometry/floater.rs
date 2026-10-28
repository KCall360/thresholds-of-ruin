use super::*;

/// Things that float: a burning wisp, a drifting knot of ice.
pub(super) fn floater(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let aura = l.aura.unwrap_or(Accent::Ember);
    let core = kit.joint(J::Core, lift, Vec3::Y * 1.1, "core", |p| {
        p.accent = Some(aura);
        if l.style == Style::Ice {
            // Dark blue recesses and pale cyan planes keep the cluster
            // readable in daylight as well as against the dark dungeon.
            p.satin.smooth = false;
            p.satin.sides_mul = 1;
            p.satin.res = (5, 8);
            p.glow.smooth = false;
            p.glow.sides_mul = 1;
            p.satin.blob(
                Vec3::ZERO,
                Vec3::new(0.17, 0.205, 0.15),
                4,
                0.13,
                Vec3::new(0.12, 0.27, 0.4),
            );
            for k in 0..9 {
                let a = k as f32 * TAU / 9.0;
                let dir = Vec3::new(a.cos(), (k as f32 * 1.3).sin() * 0.8, a.sin()).normalize();
                let color = Vec3::new(0.16, 0.43, 0.61).lerp(l.skin, 0.15 + 0.5 * rand(17, k));
                ice_shard(
                    p,
                    dir * 0.06,
                    dir * (0.3 + rand(k as u64, 3) * 0.15),
                    0.055 + 0.025 * rand(19, k),
                    color,
                );
            }
            eyes(
                p,
                Some(Accent::White),
                Vec3::new(0.0, 0.04, 0.15),
                0.06,
                0.02,
            );
            p.accent = Some(aura);
        } else {
            // Shaded outer petals frame the luminous heart and curling tongues.
            for k in 0..6 {
                let a = k as f32 * TAU / 6.0 + 0.3;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                tube(
                    &mut p.matte,
                    &[
                        out * 0.05 - Vec3::Y * 0.09,
                        out * 0.19 + Vec3::Y * 0.02,
                        out * 0.12 + Vec3::Y * 0.17,
                    ],
                    &[(0.05, 0.02), (0.06, 0.025), (0.004, 0.004)],
                    Vec3::new(0.85, 0.22, 0.035),
                );
            }
            for (k, h) in [(0, 0.52f32), (1, 0.36), (2, 0.4)] {
                let a = k as f32 * 2.2;
                let out = Vec3::new(a.cos(), 0.0, a.sin()) * 0.04;
                let curl = Vec3::new(-a.sin(), 0.0, a.cos()) * 0.06;
                tube(
                    &mut p.glow,
                    &[
                        out,
                        out * 1.4 + Vec3::Y * h * 0.4 + curl * 0.5,
                        out + Vec3::Y * h * 0.75 - curl * 0.3,
                        Vec3::Y * h + curl,
                    ],
                    &[(0.075, 0.05), (0.06, 0.04), (0.03, 0.02), (0.003, 0.003)],
                    Vec3::ONE,
                );
            }
        }
    });
    if l.style == Style::Wisp {
        // A brighter golden heart inside the flames.
        kit.attach(core, Transform::IDENTITY, "heart", |p| {
            p.accent = Some(Accent::Amber);
            p.glow
                .blob(Vec3::Y * 0.02, Vec3::splat(0.085), 1, 0.0, Vec3::ONE);
        });
    }
    if l.style == Style::Ice {
        for (side, arm) in [(1.0f32, J::ArmL), (-1.0, J::ArmR)] {
            kit.joint(
                arm,
                core,
                Vec3::new(side * 0.42, -0.1, 0.0),
                &format!("shard{side}"),
                |p| {
                    p.accent = Some(aura);
                    p.satin.smooth = false;
                    p.satin.sides_mul = 1;
                    p.glow.smooth = false;
                    p.glow.sides_mul = 1;
                    for k in 0..4 {
                        let dir = Vec3::new(side * 0.3, -1.0, 0.2 * (k as f32 - 1.5)).normalize();
                        ice_shard(
                            p,
                            Vec3::new(0.0, 0.0, (k as f32 - 1.5) * 0.022),
                            dir * (0.32 + k as f32 * 0.04),
                            0.045 + k as f32 * 0.004,
                            Vec3::new(0.22, 0.5, 0.69).lerp(l.skin, k as f32 * 0.18),
                        );
                    }
                },
            );
        }
    }
    let mote = kit.part("mote", |p| {
        p.accent = Some(aura);
        p.glow
            .blob(Vec3::ZERO, Vec3::splat(0.035), 0, 0.0, Vec3::ONE);
    });
    for _ in 0..6 {
        let e = kit
            .commands
            .spawn((Transform::default(), Visibility::default()))
            .id();
        kit.commands.entity(e).with_children(|p| mote.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
}
