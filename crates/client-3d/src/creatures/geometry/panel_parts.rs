//! Paired open bodice panels: one resolved cut for coverage and binding.
use super::*;

/// Absolute lateral center and half-width for the selected side. The panel passes
/// over the bust apex, then converges at the waist; side skin remains exposed.
pub(super) fn span(body: HumanoidBody, side: f32, y: f32, width: f32) -> Option<(f32, f32)> {
    let bust = body.bust?;
    if !(0.055..=0.515).contains(&y) {
        return None;
    }
    let apex = bust.apex(side);
    let center_x = apex.x.abs();
    let (_, radii) = bust.mass(side, 0.0);
    let half = (radii.x * width).max(0.014 * body.build);
    let (center, half) = if y <= apex.y {
        let t = ((y - 0.055) / (apex.y - 0.055)).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        (
            0.022 * body.build + (center_x - 0.022 * body.build) * t,
            0.025 * body.build + (half - 0.025 * body.build) * t,
        )
    } else {
        let t = ((y - apex.y) / (0.515 - apex.y)).clamp(0.0, 1.0);
        let t = t * t * (3.0 - 2.0 * t);
        (
            center_x + (0.14 * body.shoulders * body.build - center_x) * t,
            half + (0.027 * body.build - half) * t,
        )
    };
    Some((center, half))
}

pub(super) fn covers(body: HumanoidBody, at: Vec3, width: f32) -> bool {
    let Some((center, half)) = span(body, if at.x < 0.0 { -1.0 } else { 1.0 }, at.y, width) else {
        return false;
    };
    // Back shoulder returns stop high; the remaining back is an open drape.
    if at.z <= 0.0 && at.y < 0.43 {
        return false;
    }
    (at.x.abs() - center).abs() <= half
}

pub(super) fn bind(
    p: &mut Parts,
    surface: &crate::sdf::Sculpt,
    body: HumanoidBody,
    width: f32,
    color: Vec3,
) {
    for side in [-1.0, 1.0] {
        for edge in [-1.0, 1.0] {
            let path: Vec<_> = (0..=48)
                .filter_map(|row| {
                    let y = 0.06 + row as f32 / 48.0 * 0.45;
                    span(body, side, y, width).map(|(center, half)| {
                        (Vec2::new(side * (center + edge * (half - 0.002)), y), 0.004)
                    })
                })
                .collect();
            fitted_ribbon(&mut p.matte, surface, &path, true, 0.003, color);
        }
    }
}
