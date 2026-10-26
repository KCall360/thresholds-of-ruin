//! Hats, wreaths, bands and veils, built in the head's space over the hair.
//! Each is its own small recipe any wearer's head can carry.
use super::*;

/// Where a hat's brim sits on the head, over the hair.
const BRIM: f32 = 0.255;

/// A three-cornered hat: a rounded crown, a brim turned up on three sides
/// into walls dipping between their corners, and a gilt edge along them.
pub(super) fn tricorn(p: &mut Parts, color: Vec3, trim: Vec3) {
    p.matte.blob(
        Vec3::new(0.0, BRIM + 0.04, -0.012),
        Vec3::new(0.14, 0.075, 0.148),
        3,
        0.0,
        color,
    );
    p.matte.blob(
        Vec3::new(0.0, BRIM, -0.012),
        Vec3::new(0.24, 0.016, 0.24),
        4,
        0.0,
        color,
    );
    let corner = |k: usize| {
        let a = k as f32 * TAU / 3.0;
        Vec3::new(a.sin() * 0.255, BRIM, a.cos() * 0.255 - 0.012)
    };
    for k in 0..3 {
        let (a, b) = (corner(k), corner(k + 1));
        let mid = (a + b) * 0.5;
        let along = (b - a).normalize();
        let out = Vec3::new(mid.x, 0.0, mid.z + 0.012).normalize();
        let up = (Vec3::Y + out * 0.35).normalize();
        let half = a.distance(b) * 0.5;
        let h = 0.085;
        p.matte.extrude(
            &[
                Vec2::new(-half, 0.0),
                Vec2::new(half, 0.0),
                Vec2::new(half * 1.02, h),
                Vec2::new(0.0, h * 0.5),
                Vec2::new(-half * 1.02, h),
            ],
            mid,
            along,
            up,
            0.012,
            color,
        );
        tube(
            &mut p.metal,
            &[a + up * h, mid + up * h * 0.5 + out * 0.008, b + up * h],
            &[(0.006, 0.006); 3],
            trim,
        );
    }
}

/// A witch's hat: a wide brim, a tall cone bending back to its point, a
/// band round its foot and a bow with sheer tails falling behind.
pub(super) fn witch_hat(p: &mut Parts, color: Vec3, band: Vec3) {
    p.matte.blob(
        Vec3::new(0.0, BRIM, -0.012),
        Vec3::new(0.3, 0.014, 0.3),
        5,
        0.03,
        color,
    );
    tube(
        &mut p.matte,
        &[
            Vec3::new(0.0, BRIM, -0.012),
            Vec3::new(0.0, BRIM + 0.11, -0.025),
            Vec3::new(0.0, BRIM + 0.22, -0.05),
            Vec3::new(0.0, BRIM + 0.31, -0.1),
            Vec3::new(0.0, BRIM + 0.36, -0.17),
        ],
        &[
            (0.135, 0.14),
            (0.1, 0.104),
            (0.064, 0.066),
            (0.03, 0.03),
            (0.004, 0.004),
        ],
        color,
    );
    p.matte.torus(
        Vec3::new(0.0, BRIM + 0.022, -0.015),
        Vec3::Y,
        0.128,
        0.016,
        16,
        4,
        band,
    );
    // The bow at her left, and its sheer tails falling down her back.
    for lobe in [-1.0f32, 1.0] {
        p.matte.blob(
            Vec3::new(0.125, BRIM + 0.03, -0.04 + lobe * 0.035),
            Vec3::new(0.02, 0.026, 0.032),
            2,
            0.0,
            band,
        );
    }
    p.sheer.extrude(
        &[
            Vec2::new(-0.03, 0.0),
            Vec2::new(0.03, 0.0),
            Vec2::new(0.07, 0.42),
            Vec2::new(-0.05, 0.4),
        ],
        Vec3::new(0.12, BRIM + 0.02, -0.08),
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::NEG_Y,
        0.004,
        band,
    );
}

/// Fit to the already generated face and hair, before adding ornament geometry.
fn brow_point(p: &Parts, angle: f32) -> Vec3 {
    let out = Vec3::new(angle.sin(), 0.0, angle.cos());
    let origin = Vec3::new(0.0, 0.241 - 0.006 * angle.cos(), -0.012);
    p.matte
        .outer_surface(origin, out)
        .unwrap_or(origin + out * 0.10)
        + out * 0.006
}

fn brow_path(p: &Parts) -> Vec<Vec3> {
    (0..=64)
        .map(|i| brow_point(p, i as f32 * TAU / 64.0))
        .collect()
}

/// A wreath following the wearer's hair, with leaves above the brows.
pub(super) fn wreath(p: &mut Parts, color: Vec3) {
    let path = brow_path(p);
    for k in -6i32..=6 {
        if k == 0 {
            continue;
        }
        let a = k as f32 * 0.24;
        let root = brow_point(p, a);
        let back =
            (brow_point(p, a + 0.02) - brow_point(p, a - 0.02)).normalize_or(Vec3::X) * a.signum();
        let dir = (back + Vec3::Y * 0.45 + Vec3::new(a.sin(), 0.0, a.cos()) * 0.15).normalize();
        ornament_parts::leaf(&mut p.metal, root, dir, 0.05, 0.015, color);
    }
    tube(
        &mut p.metal,
        &path,
        &vec![(0.003, 0.003); path.len()],
        color * 0.8,
    );
}

/// A close-fitting brow band with an optional forehead plaque and jewel.
pub(super) fn circlet(p: &mut Parts, color: Vec3, gem: Option<Vec3>, broad: bool) {
    let path = brow_path(p);
    let front = path[0];
    tube(
        &mut p.metal,
        &path,
        &vec![
            if broad {
                (0.009, 0.009)
            } else {
                (0.004, 0.004)
            };
            path.len()
        ],
        color,
    );
    if broad {
        p.metal.cuboid(
            front + Vec3::Z * 0.004,
            Vec3::new(0.025, 0.016, 0.004),
            Quat::IDENTITY,
            color,
        );
    }
    if let Some(gem) = gem {
        p.satin.blob(
            front + Vec3::Z * 0.011,
            Vec3::new(0.012, 0.014, 0.007),
            1,
            0.0,
            gem,
        );
    }
}

/// A small cap on the crown, rimmed, with a sheer veil hanging behind.
pub(super) fn veiled(p: &mut Parts, cap: Vec3, veil: Vec3) {
    p.matte.blob(
        Vec3::new(0.0, 0.265, -0.035),
        Vec3::new(0.115, 0.05, 0.11),
        3,
        0.0,
        cap,
    );
    p.metal.torus(
        Vec3::new(0.0, 0.245, -0.035),
        Vec3::new(0.0, 1.0, 0.2).normalize(),
        0.112,
        0.006,
        16,
        3,
        GILT,
    );
    p.sheer.extrude(
        &[
            Vec2::new(-0.08, 0.0),
            Vec2::new(0.08, 0.0),
            Vec2::new(0.15, 0.46),
            Vec2::new(-0.15, 0.46),
        ],
        Vec3::new(0.0, 0.27, -0.13),
        Vec3::X,
        Vec3::new(0.0, -1.0, -0.12).normalize(),
        0.004,
        veil,
    );
}

/// The braid emerges from the lower rear of this cap, not from the neck.
pub(super) const WINGED_HELM_HAIR_ROOT: Vec3 = Vec3::new(0.0, 0.215, -0.125);

/// An open cap keeps the face readable; fans use the same closed feathers as wings.
pub(super) fn winged_helm(p: &mut Parts, metal: Vec3, feathers: Vec3) {
    p.metal.blob(
        Vec3::new(0.0, 0.28, -0.025),
        Vec3::new(0.13, 0.10, 0.135),
        3,
        0.0,
        metal,
    );
    // A close forehead band follows the cap rather than the hair-sized
    // circlet. Its lower edge stays above the eyes on every scaled head.
    p.metal.torus(
        Vec3::new(0.0, 0.245, -0.025),
        Vec3::Y,
        0.132,
        0.008,
        32,
        6,
        GILT,
    );
    p.metal.blob(
        Vec3::new(0.0, 0.25, 0.112),
        Vec3::new(0.018, 0.022, 0.008),
        1,
        0.0,
        GILT,
    );
    for side in [-1.0f32, 1.0] {
        let root = Vec3::new(side * 0.115, 0.25, -0.015);
        for k in 0..6 {
            let t = k as f32 / 5.0;
            feather_parts::vane(
                &mut p.matte,
                root + Vec3::Z * (t * 0.045),
                Vec3::new(side * (0.30 + t * 0.5), 1.0 - t * 0.35, -0.25),
                0.22 - t * 0.07,
                0.022,
                Vec3::Z,
                if k < 3 { feathers } else { CREAM },
                Some(GILT),
            );
        }
        p.metal.blob(root, Vec3::splat(0.025), 1, 0.0, GILT);
    }
}

/// Open arches with gem-set points; the supporting band follows actual hair.
pub(super) fn crown(p: &mut Parts, metal: Vec3, gem: Vec3) {
    let path = brow_path(p);
    tube(
        &mut p.metal,
        &path,
        &vec![(0.004, 0.004); path.len()],
        metal,
    );
    for k in 0..8 {
        let index = k * 8;
        let a = path[index];
        let b = path[index + 8];
        let peak = path[index + 4] + Vec3::Y * (if k == 0 || k == 7 { 0.095 } else { 0.065 });
        let points = [
            a,
            a.lerp(peak, 0.55) + Vec3::Y * 0.009,
            peak,
            b.lerp(peak, 0.55) + Vec3::Y * 0.009,
            b,
        ];
        tube(&mut p.metal, &points, &[(0.003, 0.003); 5], metal);
        p.metal.blob(peak, Vec3::splat(0.005), 1, 0.0, metal);
        let jewel = path[index + 4] + Vec3::Y * 0.035;
        p.metal
            .blob(jewel, Vec3::new(0.012, 0.021, 0.009), 1, 0.0, metal);
        p.satin.blob(
            jewel + Vec3::new(jewel.x, 0.0, jewel.z).normalize_or(Vec3::Z) * 0.005,
            Vec3::new(0.008, 0.015, 0.008),
            1,
            0.0,
            gem,
        );
    }
}
