use super::*;

/// A weathered stone slab: softly bevelled, its tone drifting across the
/// face, with moss settled on the upper surfaces.
pub(super) fn stone(b: &mut Builder, center: Vec3, half: Vec3, color: Vec3, key: u64) {
    let moss = Vec3::new(0.38, 0.56, 0.17);
    let seed = key as f32 * 1.7;
    b.slab(center, half, Quat::IDENTITY, key, 0.06, &|dir, at| {
        let n = (at.x * 23.0 + seed).sin() * (at.z * 19.0 - seed).cos()
            + (at.y * 29.0 + at.x * 7.0).sin() * 0.6;
        let tone = color * (0.9 + 0.1 * (at.y * 9.0 + at.x * 5.0 + seed).sin());
        if (dir.y > 0.3 && n > 0.55) || (dir.y > -0.3 && n > 1.05) {
            moss
        } else {
            tone
        }
    });
}

/// A fist: a palm, four fingers curled under at the knuckles, and a thumb
/// wrapped across. `side` is +1 for the left hand, -1 for the right; `b`
/// scales it with the body.
pub(super) fn fist(p: &mut Parts, at: Vec3, side: f32, b: f32, color: Vec3) {
    // The arm hangs with the palm toward the thigh (toward -side on x):
    // the knuckles run front to back, the fingers curl in toward the body,
    // and the thumb sits at the front, wrapping over them.
    let inward = -side;
    p.matte
        .blob(at, Vec3::new(0.03, 0.05, 0.044) * b, 2, 0.0, color);
    for k in 0..4 {
        let z = (1.5 - k as f32) * 0.019 * b;
        let knuckle = at + Vec3::new(side * 0.004, -0.036 * b, z);
        let r = (0.0115 - k as f32 * 0.0008) * b;
        tube(
            &mut p.matte,
            &[
                knuckle,
                knuckle + Vec3::new(inward * 0.018, -0.022, 0.0) * b,
                knuckle + Vec3::new(inward * 0.036, -0.012, 0.0) * b,
                knuckle + Vec3::new(inward * 0.036, 0.008, 0.0) * b,
            ],
            &[(r, r), (r, r), (r * 0.95, r * 0.95), (r * 0.8, r * 0.8)],
            color * (0.97 - k as f32 * 0.01),
        );
    }
    let base = at + Vec3::new(inward * 0.012, -0.004, 0.03) * b;
    tube(
        &mut p.matte,
        &[
            base,
            base + Vec3::new(inward * 0.014, -0.02, 0.016) * b,
            base + Vec3::new(inward * 0.03, -0.032, 0.004) * b,
        ],
        &[
            (0.014 * b, 0.014 * b),
            (0.012 * b, 0.012 * b),
            (0.01 * b, 0.01 * b),
        ],
        color,
    );
}

/// A free hand with separated long fingers: a relaxed casting palm or an
/// undead reaching claw. Held props retain the existing closed grip.
pub(super) fn open_hand(p: &mut Parts, at: Vec3, side: f32, build: f32, color: Vec3, claw: bool) {
    let inward = -side;
    p.matte
        .blob(at, Vec3::new(0.027, 0.045, 0.044) * build, 2, 0.0, color);
    for finger in 0..4 {
        let spread = (1.5 - finger as f32) * 0.021;
        let short = [0.012, 0.0, 0.006, 0.023][finger];
        let root = at + Vec3::new(side * 0.003, -0.034, spread) * build;
        let radius = (0.011 - finger as f32 * 0.00065) * build;
        tube(
            &mut p.matte,
            &[
                root,
                at + Vec3::new(0.0, -0.071 + short * 0.4, spread * 1.15) * build,
                at + Vec3::new(inward * 0.012, -0.103 + short, spread * 1.35) * build,
                at + Vec3::new(
                    inward * if claw { 0.035 } else { 0.018 },
                    -0.112 + short + if claw { 0.012 } else { 0.0 },
                    spread * 1.4,
                ) * build,
            ],
            &[
                (radius, radius),
                (radius, radius),
                (radius * 0.83, radius * 0.83),
                (radius * 0.55, radius * 0.55),
            ],
            color,
        );
    }
    tube(
        &mut p.matte,
        &[
            at + Vec3::new(inward * 0.014, 0.0, 0.029) * build,
            at + Vec3::new(inward * 0.027, -0.015, 0.057) * build,
            at + Vec3::new(inward * 0.029, -0.042, 0.069) * build,
        ],
        &[
            (0.014 * build, 0.014 * build),
            (0.012 * build, 0.012 * build),
            (0.008 * build, 0.008 * build),
        ],
        color,
    );
}

/// Tufts of hair or fur: `count` tapered strands rising from around a
/// crown at `center`, swept along `sweep` and drooping at the tips. The
/// same strands make a fringe, a mane or a beard.
pub(super) fn hair_tufts(
    p: &mut Parts,
    center: Vec3,
    radius: f32,
    count: u32,
    sweep: Vec3,
    color: Vec3,
    seed: u64,
) {
    let sweep = sweep.normalize_or(Vec3::Z);
    for k in 0..count {
        let a = k as f32 / count as f32 * TAU + rand(seed, k) * 0.5;
        let root = center
            + Vec3::new(
                a.cos() * radius * 0.7,
                rand(seed, k + 50) * 0.02,
                a.sin() * radius * 0.7,
            );
        let out = (Vec3::new(a.cos(), 0.1, a.sin()) * 0.6 + sweep).normalize();
        let length = radius * (0.7 + rand(seed, k + 100) * 0.4);
        let w = radius * (0.36 + rand(seed, k + 150) * 0.1);
        // Each lock lies along the head, lifting a little before it falls.
        tube(
            &mut p.matte,
            &[
                root,
                root + out * length * 0.5 + Vec3::Y * length * 0.05,
                root + out * length - Vec3::Y * length * 0.45,
            ],
            &[(w, w * 0.6), (w * 0.8, w * 0.5), (0.003, 0.003)],
            color * (0.92 + rand(seed, k + 200) * 0.16),
        );
    }
}

/// A smooth tube of one colour through `path` with elliptical `radii`.
pub(super) fn tube(b: &mut Builder, path: &[Vec3], radii: &[(f32, f32)], color: Vec3) {
    let radii: Vec<Vec2> = radii.iter().map(|(x, z)| Vec2::new(*x, *z)).collect();
    b.tube(path, &radii, 10, &|_, _, _| color);
}

/// A soft leaf-shaped feather from `root`, curving outward along `out`
/// and drooping as it goes; flat, broad in the middle, pointed at the tip.
pub(super) fn feather(p: &mut Parts, root: Vec3, out: Vec3, length: f32, width: f32, color: Vec3) {
    let down = Vec3::NEG_Y;
    let path = [
        root,
        root + out * length * 0.22 + down * length * 0.25,
        root + out * length * 0.34 + down * length * 0.6,
        root + out * length * 0.38 + down * length,
    ];
    let w = width * 0.5;
    let radii = [
        Vec2::new(w * 0.4, 0.006),
        Vec2::new(w, 0.01),
        Vec2::new(w * 0.62, 0.007),
        Vec2::new(0.002, 0.002),
    ];
    p.matte
        .tube(&path, &radii, 6, &|_, _, t| color * (1.0 - 0.18 * t));
}

/// Eyes with whites, a coloured iris, a pupil and a catchlight, set a
/// little forward of `center` on either side.
pub(super) fn cartoon_eyes(p: &mut Parts, center: Vec3, spread: f32, size: f32, iris: Vec3) {
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        p.matte.blob(
            at,
            Vec3::new(size * 1.55, size * 1.75, size * 1.0),
            0,
            0.0,
            Vec3::new(0.95, 0.94, 0.9),
        );
        p.matte.blob(
            at + Vec3::new(0.0, 0.0, size * 0.62),
            Vec3::new(size * 1.0, size * 1.1, size * 0.5),
            0,
            0.0,
            iris,
        );
        p.matte.blob(
            at + Vec3::new(0.0, 0.0, size * 0.92),
            Vec3::new(size * 0.55, size * 0.62, size * 0.25),
            0,
            0.0,
            DARK,
        );
        p.matte.blob(
            at + Vec3::new(size * 0.35, size * 0.42, size * 1.05),
            Vec3::splat(size * 0.22),
            0,
            0.0,
            Vec3::splat(1.6),
        );
    }
}
