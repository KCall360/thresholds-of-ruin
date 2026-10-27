//! Closed feather vanes shared by wings, armor trim and headwear.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn vane(
    b: &mut Builder,
    root: Vec3,
    dir: Vec3,
    len: f32,
    width: f32,
    across: Vec3,
    color: Vec3,
    bars: Option<Vec3>,
) {
    let dir = dir.normalize_or(Vec3::NEG_Z);
    let across =
        (across - dir * across.dot(dir)).normalize_or(dir.cross(Vec3::Y).normalize_or(Vec3::X));
    let normal = dir.cross(across);
    // A narrow insertion, a full vane, then a long taper into one point.
    const PROFILE: [(f32, f32); 7] = [
        (0.0, 0.06),
        (0.12, 0.48),
        (0.32, 1.0),
        (0.62, 0.9),
        (0.82, 0.55),
        (0.94, 0.16),
        (1.0, 0.0),
    ];
    const STEPS: usize = 32;
    const SECTION: [(f32, f32); 6] = [
        (-1.0, 0.0),
        (-0.045, 1.0),
        (0.045, 1.0),
        (1.0, 0.0),
        (0.045, -1.0),
        (-0.045, -1.0),
    ];
    let rings: Vec<[Vec3; 6]> = (0..=STEPS)
        .map(|row| {
            let t = row as f32 / STEPS as f32;
            let span = PROFILE.windows(2).find(|s| t <= s[1].0).unwrap();
            let f = ease((t - span[0].0) / (span[1].0 - span[0].0));
            let radius = width * (span[0].1 + (span[1].1 - span[0].1) * f);
            let thickness = radius * 0.12 * (1.0 - 0.35 * t);
            SECTION
                .map(|(x, y)| root + dir * len * t + across * radius * x + normal * thickness * y)
        })
        .collect();
    for row in 0..STEPS {
        let t = (row as f32 + 0.5) / STEPS as f32;
        let inside = root + dir * len * t;
        let pigment = match bars {
            Some(bar) if (t * 9.0).fract() < 0.32 && t > 0.15 => bar,
            _ => color * (1.0 - 0.12 * t),
        };
        for column in 0..SECTION.len() {
            let next = (column + 1) % SECTION.len();
            let pigment = if column == 1 || column == 4 {
                color.lerp(BONE, 0.18) * (1.0 - 0.08 * t)
            } else {
                pigment
            };
            b.quad_out(
                [
                    rings[row][column],
                    rings[row + 1][column],
                    rings[row + 1][next],
                    rings[row][next],
                ],
                inside,
                pigment,
            );
        }
    }
    // Close the inserted shaft. The far end converges to one shared tip, with
    // no terminal cap or tiny open hole when the wing exposes its underside.
    for column in 0..SECTION.len() {
        b.tri_out(
            [
                root,
                rings[0][column],
                rings[0][(column + 1) % SECTION.len()],
            ],
            root + dir,
            color,
        );
    }
}
