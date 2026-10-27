//! Overlapping trim follows an authored elliptical rim in any joint's space.
use super::super::outfit::{Fringe, FringeKind};
use super::*;

pub(super) fn rim(p: &mut Parts, fringe: Fringe, center: Vec3, radii: Vec2, scale: f32) {
    if fringe.kind == FringeKind::Fur {
        fur_parts::rim(p, fringe, center, radii, scale);
        return;
    }
    let count = 12;
    for row in 0..2 {
        for k in 0..count {
            let angle = (k as f32 + row as f32 * 0.5) * TAU / count as f32;
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            let root = center
                + Vec3::new(
                    out.x * radii.x,
                    -row as f32 * 0.025 * scale,
                    out.z * radii.y,
                );
            let direction = (out * 0.45 - Vec3::Y).normalize();
            let length = fringe.length * scale * (0.9 + 0.1 * (k as f32 * 2.4).sin());
            let width = radii.max_element() * TAU / count as f32 * 0.72;
            let color = fringe.color * (0.88 + 0.12 * (k % 3) as f32 / 2.0);
            match fringe.kind {
                FringeKind::Feathers => feather_parts::vane(
                    &mut p.matte,
                    root,
                    direction,
                    length,
                    width,
                    Vec3::new(out.z, 0.0, -out.x),
                    color,
                    Some(fringe.accent),
                ),
                FringeKind::Leaves => beast_parts::LeafBlade {
                    root,
                    direction,
                    length,
                    half_width: width,
                    arch: 0.1,
                    droop: 0.1,
                    color,
                    edge: fringe.accent,
                }
                .emit(&mut p.matte),
                FringeKind::Fur => unreachable!("fur uses its own pile construction"),
            }
        }
    }
}

pub(super) fn shoulder(p: &mut Parts, fringe: Fringe, center: Vec3, radii: Vec2, scale: f32) {
    if fringe.kind == FringeKind::Fur {
        fur_parts::cap(p, fringe, center, radii, scale);
    }
    rim(p, fringe, center, radii, scale);
}
