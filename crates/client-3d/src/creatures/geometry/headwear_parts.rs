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

/// A wreath of leaves round the brow, meeting over the forehead and
/// pointing back toward the crown.
pub(super) fn wreath(p: &mut Parts, color: Vec3) {
    for k in -6i32..=6 {
        if k == 0 {
            continue;
        }
        let a = k as f32 * 0.24;
        let root = Vec3::new(a.sin() * 0.168, 0.2, a.cos() * 0.168 - 0.012);
        let back = Vec3::new(a.cos(), 0.0, -a.sin()) * a.signum();
        let dir = (back + Vec3::Y * 0.35 + Vec3::new(a.sin(), 0.0, a.cos()) * 0.3).normalize();
        rogue::leaf(&mut p.metal, root, dir, 0.08, 0.032, color);
    }
    p.metal.torus(
        Vec3::new(0.0, 0.2, -0.012),
        Vec3::Y,
        0.166,
        0.004,
        16,
        3,
        color * 0.8,
    );
}

/// A band round the brow, low in front, narrow or broad, perhaps set with
/// a jewel over the forehead.
pub(super) fn circlet(p: &mut Parts, color: Vec3, gem: Option<Vec3>, broad: bool) {
    let axis = Vec3::new(0.0, 1.0, 0.15).normalize();
    let center = Vec3::new(0.0, 0.205, -0.012);
    p.metal.torus(
        center,
        axis,
        0.168,
        if broad { 0.016 } else { 0.007 },
        18,
        4,
        color,
    );
    if broad {
        // A plaque over the forehead.
        p.metal.cuboid(
            Vec3::new(0.0, 0.195, 0.162),
            Vec3::new(0.035, 0.026, 0.006),
            Quat::from_rotation_x(-0.15),
            color,
        );
    }
    if let Some(gem) = gem {
        p.satin.blob(
            Vec3::new(0.0, 0.19, 0.172),
            Vec3::new(0.015, 0.018, 0.009),
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
