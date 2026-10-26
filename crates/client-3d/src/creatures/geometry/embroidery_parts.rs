//! Fitted stitched ornament, independent of the cloth mesh's coarse shading grid.
use super::super::outfit::Motif;
use super::*;

pub(super) fn embroider(b: &mut Builder, surface: &ClothSurface, motif: Motif, color: Vec3) {
    let radius = surface.point(0.65, PI * 0.5, false).xz().length().max(0.1);
    let repeats = ((TAU * radius / 0.14).round() as usize).clamp(8, 24);
    let angle_width = TAU / repeats as f32 * 0.72;
    let height = (0.10 / (surface.length + 0.04)).min(0.32);
    let rows: &[f32] = if surface.length > 0.4 {
        &[0.32, 0.76]
    } else {
        &[0.62]
    };
    let paths = paths(motif);
    for &row in rows {
        for repeat in 0..repeats {
            let center = (repeat as f32 + 0.5) * TAU / repeats as f32;
            let at = |p: Vec2| {
                let q = surface.point(row + p.y * height, center + p.x * angle_width, false);
                q + Vec3::new(q.x, 0.0, q.z).normalize_or(Vec3::Z) * 0.003
            };
            for path in &paths {
                fitted_stitches(b, path, &|p| Some(at(p)), color);
            }
        }
    }
}

/// Sample each motif before projection and break its strokes wherever cloth
/// is absent. This works for a draped sheet or a sculpted torso alike.
pub(super) fn fitted_stitches(
    b: &mut Builder,
    path: &[Vec2],
    project: &impl Fn(Vec2) -> Option<Vec3>,
    color: Vec3,
) {
    let Some(last) = path.last() else { return };
    let mut samples = Vec::new();
    for segment in path.windows(2) {
        let steps = (segment[0].distance(segment[1]) / 0.025).ceil().max(1.0) as usize;
        for step in 0..steps {
            samples.push(segment[0].lerp(segment[1], step as f32 / steps as f32));
        }
    }
    samples.push(*last);
    let mut fitted = Vec::new();
    let flush = |b: &mut Builder, points: &mut Vec<Vec3>| {
        if points.len() >= 2 {
            tube(b, points, &vec![(0.0012, 0.0012); points.len()], color);
        }
        points.clear();
    };
    for sample in samples {
        match project(sample) {
            Some(point) => fitted.push(point),
            None => flush(b, &mut fitted),
        }
    }
    flush(b, &mut fitted);
}

/// Fine stitches on the actual garment surface, clipped at its free edges.
pub(super) fn bodice(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    build: f32,
    motif: Motif,
    color: Vec3,
    clothed: &impl Fn(Vec3, Vec3) -> bool,
) {
    let paths = paths(motif);
    for front in [true, false] {
        let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
        for row in 0..4 {
            for column in -3..=3 {
                let project = |uv: Vec2| {
                    let x = (column as f32 + uv.x) * 0.105 * build;
                    let y = 0.08 + (row as f32 + uv.y) * 0.125;
                    let surface = torso_surface_hit(form, x, y, front)?;
                    let normal = sculpt_surface_normal(form, surface, outward);
                    // End before turning round the silhouette. Give thread a
                    // small cloth margin at necklines, armholes and crop edges.
                    if normal.dot(outward) < 0.35
                        || [
                            Vec3::ZERO,
                            Vec3::X * 0.003,
                            Vec3::NEG_X * 0.003,
                            Vec3::Y * 0.003,
                            Vec3::NEG_Y * 0.003,
                        ]
                        .iter()
                        .any(|offset| !clothed(surface + *offset, normal))
                    {
                        return None;
                    }
                    Some(surface + normal * 0.003)
                };
                for path in &paths {
                    fitted_stitches(b, path, &project, color);
                }
            }
        }
    }
}

fn paths(motif: Motif) -> Vec<Vec<Vec2>> {
    let circle = |center: Vec2, radius: f32| {
        (0..=24)
            .map(|i| {
                let angle = i as f32 / 24.0 * TAU;
                center + Vec2::new(angle.cos(), angle.sin()) * radius
            })
            .collect::<Vec<_>>()
    };
    let line = |a: (f32, f32), b: (f32, f32)| vec![Vec2::new(a.0, a.1), Vec2::new(b.0, b.1)];
    match motif {
        Motif::Leaf => {
            let mut result = vec![line((0.0, -0.4), (0.0, 0.4))];
            for side in [-1.0, 1.0] {
                result.push(
                    (0..=16)
                        .map(|i| {
                            let t = i as f32 / 16.0;
                            Vec2::new(side * (PI * t).sin() * 0.27, -0.38 + 0.76 * t)
                        })
                        .collect(),
                );
                for y in [-0.2, 0.0, 0.2] {
                    result.push(line((0.0, y + 0.11), (side * 0.20, y - 0.05)));
                }
            }
            result
        }
        Motif::Sun => {
            let mut result = vec![circle(Vec2::ZERO, 0.17)];
            for i in 0..8 {
                let angle = i as f32 / 8.0 * TAU;
                let direction = Vec2::new(angle.cos(), angle.sin());
                result.push(vec![direction * 0.24, direction * 0.40]);
            }
            result
        }
        Motif::Lily => vec![
            line((0.0, -0.4), (0.0, 0.4)),
            line((-0.25, 0.12), (0.25, 0.12)),
            circle(Vec2::new(-0.14, -0.10), 0.13),
            circle(Vec2::new(0.14, -0.10), 0.13),
        ],
        Motif::Knot => vec![vec![
            Vec2::new(0.0, -0.36),
            Vec2::new(0.32, 0.0),
            Vec2::new(0.0, 0.36),
            Vec2::new(-0.32, 0.0),
            Vec2::new(0.0, -0.36),
        ]],
        Motif::Bone => {
            let mut result = vec![line((0.0, -0.27), (0.0, 0.27))];
            for x in [-0.065, 0.065] {
                for y in [-0.27, 0.27] {
                    result.push(circle(Vec2::new(x, y), 0.065));
                }
            }
            result
        }
        Motif::Scroll => vec![
            (0..=32)
                .map(|i| {
                    let v = i as f32 / 32.0 * 0.8 - 0.4;
                    Vec2::new((v * 10.0).sin() * 0.16, v)
                })
                .collect(),
            circle(Vec2::new(0.15, 0.18), 0.10),
        ],
    }
}
