//! Binding traced around a fitted upper bodice edge in three dimensions.
use super::*;

fn radial_surface(surface: &crate::sdf::Sculpt, y: f32, angle: f32) -> Vec3 {
    let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
    // A coverage-height solve needs a continuous radial hit. The general
    // sphere marcher stops at a coarse distance tolerance, which can make
    // its returned hit jump while the outer height solve is converging.
    let (mut lo, mut hi) = (0.0, 0.5);
    for _ in 0..24 {
        let radius = (lo + hi) * 0.5;
        if surface.distance(outward * radius + Vec3::Y * y) < 0.0 {
            lo = radius;
        } else {
            hi = radius;
        }
    }
    outward * ((lo + hi) * 0.5) + Vec3::Y * y
}

/// Resolve an implicit upper edge on the actual surface, not on independently
/// projected front/back outlines. Bounds bracket the intended torso edge.
pub(super) fn edge_point(
    surface: &crate::sdf::Sculpt,
    angle: f32,
    bounds: Vec2,
    height: &dyn Fn(Vec3) -> f32,
    clearance: f32,
) -> Vec3 {
    let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
    let fitted = |y| {
        let at = radial_surface(surface, y, angle);
        at + sculpt_surface_normal(surface, at, outward) * clearance
    };
    let (mut lo, mut hi) = (bounds.x, bounds.y);
    for _ in 0..20 {
        let y = (lo + hi) * 0.5;
        let point = fitted(y);
        if point.y < height(point) {
            lo = y;
        } else {
            hi = y;
        }
    }
    fitted((lo + hi) * 0.5)
}

pub(super) fn bind(
    p: &mut Parts,
    body: HumanoidBody,
    surface: &crate::sdf::Sculpt,
    dip: f32,
    color: Vec3,
) {
    const SIDES: usize = 128;
    let height = |at: Vec3| sweetheart_top(body, at.x, bodice_front(at), dip);
    let rings: Vec<[Vec3; 2]> = (0..SIDES)
        .map(|i| {
            let angle = i as f32 * TAU / SIDES as f32;
            // Bind across the cut, including the normal lift in the solve.
            // Lifting an already fitted strip can expose a rim on slopes.
            [-0.003, 0.003].map(|offset| {
                edge_point(
                    surface,
                    angle,
                    Vec2::new(0.18, 0.49),
                    &|at| height(at) + offset,
                    0.004,
                )
            })
        })
        .collect();
    for i in 0..SIDES {
        let next = (i + 1) % SIDES;
        let points = [rings[i][0], rings[next][0], rings[next][1], rings[i][1]];
        let center = points.iter().copied().sum::<Vec3>() * 0.25;
        let inward = center - Vec3::new(center.x, 0.0, center.z).normalize_or(Vec3::Z) * 0.02;
        p.matte.quad_out(points, inward, color);
    }
}
