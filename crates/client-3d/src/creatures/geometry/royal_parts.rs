//! Free central panels and gathered sleeve swags; reusable on any garment.
use super::*;

pub(super) fn panel_point(
    surface: &ClothSurface,
    t: f32,
    across: f32,
    width: f32,
    inner: bool,
) -> Vec3 {
    // A separate, continuous central panel leaves two real openings. It shares
    // the parent's cut and sway, but never inherits the side panels' separation.
    let unsplit = ClothSurface {
        split: 0.0,
        ..surface.clone()
    };
    unsplit.point(t, (across * width).rem_euclid(TAU), inner) + Vec3::Z * 0.008
}

pub(super) fn center_panel(
    b: &mut Builder,
    surface: &ClothSurface,
    width: f32,
    color: Vec3,
    trim: Option<Vec3>,
) {
    let rows = surface.rows();
    let point = |t, x, inner| panel_point(surface, t, x, width, inner);
    for row in 0..rows {
        let t0 = row as f32 / rows as f32;
        let t1 = (row + 1) as f32 / rows as f32;
        for column in 0..8 {
            let x0 = column as f32 / 4.0 - 1.0;
            let x1 = (column + 1) as f32 / 4.0 - 1.0;
            for inner in [false, true] {
                b.quad_out(
                    [
                        point(t0, x0, inner),
                        point(t0, x1, inner),
                        point(t1, x1, inner),
                        point(t1, x0, inner),
                    ],
                    Vec3::new(0.0, -surface.length * t0, if inner { 2.0 } else { 0.0 }),
                    color * if inner { 0.7 } else { 1.0 },
                );
            }
        }
        for x in [-1.0, 1.0] {
            b.quad_out(
                [
                    point(t0, x, false),
                    point(t1, x, false),
                    point(t1, x, true),
                    point(t0, x, true),
                ],
                Vec3::new(-x, -surface.length * t0, 0.0),
                color,
            );
        }
    }
    for t in [0.0, 1.0] {
        for column in 0..8 {
            let x0 = column as f32 / 4.0 - 1.0;
            let x1 = (column + 1) as f32 / 4.0 - 1.0;
            b.quad_out(
                [
                    point(t, x0, false),
                    point(t, x1, false),
                    point(t, x1, true),
                    point(t, x0, true),
                ],
                Vec3::Y * -surface.length * 0.5,
                color,
            );
        }
    }
    if let Some(trim) = trim {
        for x in [-1.0, 1.0] {
            let path: Vec<_> = (0..=rows)
                .map(|r| point(r as f32 / rows as f32, x, false) + Vec3::Z * 0.002)
                .collect();
            tube(b, &path, &vec![(0.003, 0.003); path.len()], trim);
        }
    }
}

pub(super) fn sleeve_swag(
    b: &mut Builder,
    build: f32,
    arm_len: f32,
    color: Vec3,
    trim: Option<Vec3>,
) {
    let point = |t: f32, a: f32| {
        let fullness = (PI * t).sin() * 0.021;
        let radius = (0.057 + fullness + 0.003 * (a * 5.0).cos() * t) * build;
        Vec3::new(
            a.sin() * radius,
            -(0.055 + t * (0.065 + 0.025 * a.cos().abs())) * arm_len,
            a.cos() * radius,
        )
    };
    for row in 0..6 {
        for k in 0..32 {
            let t0 = row as f32 / 6.0;
            let t1 = (row + 1) as f32 / 6.0;
            let a0 = k as f32 * TAU / 32.0;
            let a1 = (k + 1) as f32 * TAU / 32.0;
            let q = [point(t0, a0), point(t0, a1), point(t1, a1), point(t1, a0)];
            b.quad_out(q, Vec3::Y * q[0].y, color);
            b.quad_out(
                q,
                Vec3::new(a0.sin() * 2.0, q[0].y, a0.cos() * 2.0),
                color * 0.75,
            );
        }
    }
    if let Some(trim) = trim {
        for t in [0.0, 1.0] {
            let path: Vec<_> = (0..=32).map(|k| point(t, k as f32 * TAU / 32.0)).collect();
            tube(b, &path, &vec![(0.003, 0.003); path.len()], trim);
        }
    }
}

/// Thin boning and scrolling seam ornament fitted to the bodice surface.
pub(super) fn corset(
    p: &mut Parts,
    body: HumanoidBody,
    form: &crate::sdf::Sculpt,
    metal: Vec3,
    gem: Vec3,
) {
    for side in [-1.0, 1.0] {
        for share in [0.42, 0.80] {
            let path = boning_path(body, form, side, share);
            fitted_ribbon(&mut p.metal, form, &path, true, 0.004, metal);
        }
        let top = body.bust.map_or(0.30, |b| b.fold);
        let path: Vec<_> = (0..=40)
            .map(|k| {
                let t = k as f32 / 40.0;
                (
                    Vec2::new(
                        side * (0.008 + 0.008 * (t * PI * 8.0).sin()),
                        0.045 + (top - 0.045) * t,
                    ),
                    0.0025,
                )
            })
            .collect();
        fitted_ribbon(&mut p.metal, form, &path, true, 0.004, metal);
    }
    let y = sweetheart_top(body, 0.0, 1.0, 0.55) - 0.02;
    let at = torso_surface(form, 0.0, y, true) + Vec3::Z * 0.007;
    p.metal
        .blob(at, Vec3::new(0.014, 0.024, 0.007), 1, 0.0, metal);
    p.satin.blob(
        at + Vec3::Z * 0.005,
        Vec3::new(0.009, 0.018, 0.006),
        1,
        0.0,
        gem,
    );
}

pub(super) fn boning_path(
    body: HumanoidBody,
    form: &crate::sdf::Sculpt,
    side: f32,
    share: f32,
) -> Vec<(Vec2, f32)> {
    (0..=24)
        .map(|k| {
            let t = k as f32 / 24.0;
            let y = 0.045 + (body.bust.map_or(0.32, |b| b.fold) - 0.045) * t;
            let half_width = pelvis_surface(form, y, Vec3::X).x;
            let x = side * half_width * share;
            (Vec2::new(x, y), 0.003)
        })
        .collect()
}
