use super::*;

/// Find the first actual front/back hit at a fixed lateral position. Whether
/// z=0 lies inside is not a reliable silhouette test on a curved or cut garment.
/// A miss remains explicit so a sewn edge can end at the silhouette.
pub(in crate::creatures) fn torso_surface_hit(
    form: &crate::sdf::Sculpt,
    x: f32,
    y: f32,
    front: bool,
) -> Option<Vec3> {
    torso_surface_hit_offset(form, x, y, front, 0.0)
}

/// Trace a clearance isosurface, reserving volume for attachments rather than
/// flattening every vertex against the unexpanded support.
pub(in crate::creatures) fn torso_surface_hit_offset(
    form: &crate::sdf::Sculpt,
    x: f32,
    y: f32,
    front: bool,
    clearance: f32,
) -> Option<Vec3> {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    surface_ray_hit(form, Vec3::new(x, y, 0.0), outward, clearance, 0.45)
}

/// Exterior-to-interior ray against an offset support surface.
pub(in crate::creatures) fn surface_ray_hit(
    form: &crate::sdf::Sculpt,
    origin: Vec3,
    outward: Vec3,
    clearance: f32,
    reach: f32,
) -> Option<Vec3> {
    let at = |depth: f32| origin + outward * depth;
    let mut depth = reach;
    let mut previous = depth;
    for _ in 0..128 {
        let distance = form.distance(at(depth)) - clearance;
        if distance <= 0.0 {
            let (mut inside, mut outside) = (depth, previous);
            for _ in 0..10 {
                let middle = (inside + outside) * 0.5;
                if form.distance(at(middle)) <= clearance {
                    inside = middle;
                } else {
                    outside = middle;
                }
            }
            return Some(at((inside + outside) * 0.5));
        }
        if distance < 0.0002 {
            return Some(at(depth));
        }
        previous = depth;
        depth -= distance.clamp(0.002, 0.025);
        if depth < -0.35 {
            break;
        }
    }
    None
}

/// Point attachments may slide to a lateral edge when their authored ray misses.
/// Full-width ribbons instead retain the exact pattern plane and end as a row.
pub(super) fn torso_surface(form: &crate::sdf::Sculpt, x: f32, y: f32, front: bool) -> Vec3 {
    let hit = |lateral| torso_surface_hit(form, lateral, y, front);
    if let Some(surface) = hit(x) {
        return surface;
    }
    // All fitted profiles stay within the garment's vertical extent. Near
    // an armhole or waist edge, preserve height and find the last supported x.
    let Some(mut surface) = hit(0.0) else {
        return Vec3::new(x, y, 0.0);
    };
    let (mut inside, mut outside) = (0.0, x);
    for _ in 0..10 {
        let middle = (inside + outside) * 0.5;
        if let Some(at) = hit(middle) {
            inside = middle;
            surface = at;
        } else {
            outside = middle;
        }
    }
    surface
}

/// Outward sculpt normal shared by torso, shoulder and waist attachments.
pub(super) fn sculpt_surface_normal(
    form: &crate::sdf::Sculpt,
    surface: Vec3,
    outward: Vec3,
) -> Vec3 {
    let e = 0.0015;
    Vec3::new(
        form.distance(surface + Vec3::X * e) - form.distance(surface - Vec3::X * e),
        form.distance(surface + Vec3::Y * e) - form.distance(surface - Vec3::Y * e),
        form.distance(surface + Vec3::Z * e) - form.distance(surface - Vec3::Z * e),
    )
    .normalize_or(outward)
}

/// A front/back hit paired with its outward surface normal.
pub(super) fn torso_surface_frame(
    form: &crate::sdf::Sculpt,
    at: Vec2,
    front: bool,
) -> (Vec3, Vec3) {
    let surface = torso_surface(form, at.x, at.y, front);
    (
        surface,
        sculpt_surface_normal(form, surface, if front { Vec3::Z } else { Vec3::NEG_Z }),
    )
}

/// Cloth laid onto a chest or back, rather than a second closed torso that
/// intersects the first. Profile entries are height and half width.
pub(super) fn fitted_panel(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    profile: &[(f32, f32)],
    front: bool,
    offset: f32,
    color: Vec3,
) {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let at = |y: f32, x: f32| torso_surface(form, x, y, front) + outward * offset;
    let columns = (profile
        .iter()
        .map(|(_, width)| *width)
        .fold(0.0f32, f32::max)
        * 2.0
        / 0.012)
        .ceil()
        .max(4.0) as usize;
    for pair in profile.windows(2) {
        // Long chords cut through a curved bust even when their endpoints
        // touch the surface. Keep both directions close to the torso mesh's
        // resolution, including the narrow vertical trim on fitted robes.
        let rows = ((pair[1].0 - pair[0].0).abs() / 0.012).ceil().max(1.0) as usize;
        let points: Vec<Vec<Vec3>> = (0..=rows)
            .map(|row| {
                let t = row as f32 / rows as f32;
                let (height, width) = if row == rows {
                    pair[1]
                } else {
                    (
                        pair[0].0 + (pair[1].0 - pair[0].0) * t,
                        pair[0].1 + (pair[1].1 - pair[0].1) * t,
                    )
                };
                (0..=columns)
                    .map(|column| at(height, (column as f32 / columns as f32 * 2.0 - 1.0) * width))
                    .collect()
            })
            .collect();
        for row in 0..rows {
            for column in 0..columns {
                b.quad_out(
                    [
                        points[row][column],
                        points[row][column + 1],
                        points[row + 1][column + 1],
                        points[row + 1][column],
                    ],
                    Vec3::Y * (points[row][column].y + points[row + 1][column].y) * 0.5,
                    color,
                );
            }
        }
    }
}

// Waist-band landmarks in the hips' frame, shared by belt construction and
// torso accessory anchoring.
pub(in crate::creatures) const BELT_BOTTOM_Y: f32 = 0.045;
pub(in crate::creatures) const BELT_TOP_Y: f32 = 0.095;
pub(in crate::creatures) const BELT_CENTER_Y: f32 = (BELT_BOTTOM_Y + BELT_TOP_Y) * 0.5;

/// An open buckle frame and tongue in a caller-supplied attachment frame.
/// The strap remains visible through its windows; the tongue joins the rim.
pub(super) fn buckle_frame(
    b: &mut Builder,
    at: Vec3,
    rotation: Quat,
    half_size: Vec2,
    color: Vec3,
) {
    let bar = half_size.min_element() * 0.17;
    let depth = bar * 0.8;
    for side in [-1.0, 1.0] {
        b.cuboid(
            at + rotation * Vec3::new(side * (half_size.x - bar), 0.0, 0.0),
            Vec3::new(bar, half_size.y, depth),
            rotation,
            color,
        );
        b.cuboid(
            at + rotation * Vec3::new(0.0, side * (half_size.y - bar), 0.0),
            Vec3::new(half_size.x - bar * 2.0, bar, depth),
            rotation,
            color,
        );
    }
    b.cuboid(
        at + rotation * Vec3::new(0.0, half_size.y * 0.18, depth * 0.3),
        Vec3::new(bar * 0.6, half_size.y * 0.65, depth * 0.7),
        rotation,
        color,
    );
}

/// A shared accessory assembly: the strap and its hardware use the same
/// garment surface, including the fuller female chest and fitted waist.
pub(super) fn fitted_baldric(
    p: &mut Parts,
    garment: &crate::sdf::Sculpt,
    body: HumanoidBody,
    baldric: Baldric,
    overlay: f32,
) {
    let b = body.build;
    let side = if baldric.reverse { -1.0 } else { 1.0 };
    let path = [
        (
            Vec2::new(
                -side * 0.20 * body.shoulders * b,
                body.shoulder(1.0).y - 0.01,
            ),
            0.025 * b,
        ),
        (
            Vec2::new(-side * 0.16 * body.shoulders * b, 0.43),
            0.025 * b,
        ),
        (Vec2::new(-side * 0.065 * b, 0.325), 0.025 * b),
        (Vec2::new(side * 0.065 * b, 0.195), 0.025 * b),
        (
            Vec2::new(side * 0.13 * body.waist * b, BELT_CENTER_Y - body.torso.y),
            0.022 * b,
        ),
    ];
    // Trace the complete support envelope at the authored x/y coordinates.
    // Large normal offsets on a tightly curved waist can cross each other
    // and fold the strip; resolving the grown surface first keeps it a sheet.
    let skin = torso_shape(body, None, true);
    let mut support = torso_layer_support(&skin, garment);
    support.inflate(BALDRIC_SURFACE_OFFSET + overlay);
    let garment = &support;
    let front = ribbon_surface_rows(garment, &path, true, 0.0);
    let back = ribbon_surface_rows(garment, &path, false, 0.0);
    let shoulder = ribbon_surface_turn(garment, &front[0], &back[0], Vec3::Y);
    let waist = ribbon_surface_turn(
        garment,
        front.last().unwrap(),
        back.last().unwrap(),
        Vec3::X * side,
    );
    for (rows, outward) in [
        (&front, Vec3::Z),
        (&back, Vec3::NEG_Z),
        (&shoulder, Vec3::Y),
        (&waist, Vec3::X * side),
    ] {
        emit_ribbon_strip(&mut p.satin, rows, outward, baldric.color, false);
    }
    if baldric.buckle {
        // Place hardware on the strap's centreline, rather than choosing a
        // separate species-specific point that may drift off it.
        let segment = path
            .windows(2)
            .find(|pair| pair[0].0.y >= 0.3 && pair[1].0.y <= 0.3)
            .expect("baldric crosses the buckle height");
        let t = (segment[0].0.y - 0.3) / (segment[0].0.y - segment[1].0.y);
        let at = segment[0].0.lerp(segment[1].0, t);
        let (surface, normal) = torso_surface_frame(garment, at, true);
        let centre = surface + normal * 0.007;
        let direction = (segment[1].0 - segment[0].0).normalize();
        let before = torso_surface_frame(garment, at - direction * 0.01, true).0;
        let after = torso_surface_frame(garment, at + direction * 0.01, true).0;
        let across = (after - before).cross(normal).normalize_or(Vec3::X);
        let along = normal.cross(across).normalize_or(Vec3::Y);
        let frame = Quat::from_mat3(&Mat3::from_cols(across, along, normal));
        buckle_frame(
            &mut p.metal,
            centre,
            frame,
            Vec2::new(0.034 * b, 0.027 * b),
            Vec3::new(0.74, 0.5, 0.26),
        );
    }
}

/// A flat strap follows the garment across its full width. Sweeping a tube
/// through a few surface points can twist its wide axis away from the cloth
/// and interpolate back inside a fuller chest. Each row here is projected
/// after interpolation; the narrow turned edges give leather real thickness.
pub(super) fn ribbon_surface_rows(
    form: &crate::sdf::Sculpt,
    path: &[(Vec2, f32)],
    front: bool,
    offset: f32,
) -> Vec<Vec<(Vec3, Vec3)>> {
    ribbon_path_rows(path)
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|at| {
                    let (surface, normal) = torso_surface_frame(form, at, front);
                    (surface + normal * offset, normal)
                })
                .collect()
        })
        .collect()
}

/// Sample a strip in the garment's pattern plane before fitting its depth.
/// Keeping the complete width in each row prevents independent edge clamps
/// from pinching a binding into overlapping triangles at the body silhouette.
pub(super) fn ribbon_path_rows(path: &[(Vec2, f32)]) -> Vec<Vec<Vec2>> {
    if path.len() < 2 {
        return Vec::new();
    }
    let across = |i: usize| {
        let tangent = (path[(i + 1).min(path.len() - 1)].0 - path[i.saturating_sub(1)].0)
            .normalize_or(Vec2::Y);
        Vec2::new(tangent.y, -tangent.x)
    };
    let columns = (path.iter().map(|(_, width)| *width).fold(0.0f32, f32::max) * 2.0 / 0.006)
        .ceil()
        .max(4.0) as usize;
    let mut points = Vec::new();
    for (i, pair) in path.windows(2).enumerate() {
        let rows = ((pair[1].0 - pair[0].0).length() / 0.006).ceil().max(1.0) as usize;
        for row in 0..rows {
            let t = row as f32 / rows as f32;
            let center = pair[0].0.lerp(pair[1].0, t);
            let side = across(i).lerp(across(i + 1), t).normalize_or(Vec2::X);
            let width = pair[0].1 + (pair[1].1 - pair[0].1) * t;
            points.push(
                (0..=columns)
                    .map(|column| {
                        center + side * ((column as f32 / columns as f32 * 2.0 - 1.0) * width)
                    })
                    .collect(),
            );
        }
    }
    let (center, width) = path[path.len() - 1];
    points.push(
        (0..=columns)
            .map(|column| {
                center
                    + across(path.len() - 1)
                        * ((column as f32 / columns as f32 * 2.0 - 1.0) * width)
            })
            .collect(),
    );
    points
}

/// Join two already sampled edge rows around the top or flank of the support.
/// The cloned end rows keep adjoining patches watertight without hidden caps.
pub(super) fn ribbon_surface_turn(
    form: &crate::sdf::Sculpt,
    start: &[(Vec3, Vec3)],
    end: &[(Vec3, Vec3)],
    outward: Vec3,
) -> Vec<Vec<(Vec3, Vec3)>> {
    assert_eq!(start.len(), end.len());
    let span = start
        .iter()
        .zip(end)
        .map(|(a, b)| a.0.distance(b.0))
        .fold(0.0f32, f32::max);
    let steps = (span / 0.012).ceil().max(4.0) as usize;
    let mut rows = vec![start.to_vec()];
    for row in 1..steps {
        let t = row as f32 / steps as f32;
        rows.push(
            start
                .iter()
                .zip(end)
                .map(|(a, b)| {
                    let from = a.0.lerp(b.0, t) + outward * 0.65;
                    // A ray grazing the flank takes short steps; keep
                    // marching from where one bounded march stopped.
                    let mut surface = form.onto(from, -outward);
                    for _ in 0..4 {
                        if form.distance(surface).abs() < 0.0005 {
                            break;
                        }
                        surface = form.onto(surface, -outward);
                    }
                    debug_assert!(
                        surface.is_finite() && form.distance(surface).abs() < 0.002,
                        "ribbon turn must stay on its support: {surface}"
                    );
                    (surface, sculpt_surface_normal(form, surface, outward))
                })
                .collect(),
        );
    }
    rows.push(end.to_vec());
    rows
}

pub(super) fn emit_ribbon_strip(
    b: &mut Builder,
    points: &[Vec<(Vec3, Vec3)>],
    outward: Vec3,
    color: Vec3,
    cap_ends: bool,
) {
    if points.len() < 2 || points[0].len() < 2 {
        return;
    }
    let columns = points[0].len() - 1;
    let thickness = 0.007;
    let inside_between = |a: (Vec3, Vec3), z: (Vec3, Vec3)| {
        (a.0 + z.0) * 0.5 - (a.1 + z.1).normalize_or(outward) * (thickness * 0.5)
    };
    for pair in points.windows(2) {
        for column in 0..columns {
            let q = [
                pair[0][column],
                pair[0][column + 1],
                pair[1][column + 1],
                pair[1][column],
            ];
            let center = q.iter().map(|point| point.0).sum::<Vec3>() * 0.25;
            let normal = q
                .iter()
                .map(|point| point.1)
                .sum::<Vec3>()
                .normalize_or(outward);
            let top = q.map(|point| point.0);
            // Each fitted face is a graph along this patch's projection ray.
            // A mean sculpt normal can point across a sharp shell opening,
            // reversing one half of a non-planar quad even without a fold.
            for face in [[top[0], top[1], top[2]], [top[0], top[2], top[3]]] {
                let center = face.iter().sum::<Vec3>() / 3.0;
                b.tri_out(face, center - outward * 0.1, color);
            }
            b.quad_out(
                q.map(|(point, normal)| point - normal * thickness),
                center + normal * 0.1,
                color * 0.8,
            );
        }
        let inside = inside_between(pair[0][columns / 2], pair[1][columns / 2]);
        for column in [0, columns] {
            let (a, z) = (pair[0][column], pair[1][column]);
            b.quad_out(
                [a.0, z.0, z.0 - z.1 * thickness, a.0 - a.1 * thickness],
                inside,
                color * 0.8,
            );
        }
    }
    if cap_ends {
        for (end, next) in [(0, 1), (points.len() - 1, points.len() - 2)] {
            let inside = inside_between(points[end][columns / 2], points[next][columns / 2]);
            for column in 0..columns {
                let (a, z) = (points[end][column], points[end][column + 1]);
                b.quad_out(
                    [a.0, z.0, z.0 - z.1 * thickness, a.0 - a.1 * thickness],
                    inside,
                    color * 0.8,
                );
            }
        }
    }
}

pub(super) fn fitted_ribbon(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    path: &[(Vec2, f32)],
    front: bool,
    offset: f32,
    color: Vec3,
) {
    // Grow the support before tracing it. Moving already fitted vertices
    // along changing normals also moves their x/y and can fold a narrow seam.
    let mut support = form.clone();
    support.inflate(offset);
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let fit = |row: &[Vec2]| -> Option<Vec<(Vec3, Vec3)>> {
        row.iter()
            .map(|at| {
                // A sewn edge ends where its complete width leaves the
                // garment; no unsupported vertex is pulled toward the neck
                // or waist while its neighbours remain at their authored x/y.
                let surface = torso_surface_hit(&support, at.x, at.y, front)?;
                Some((surface, sculpt_surface_normal(&support, surface, outward)))
            })
            .collect()
    };
    let rows = ribbon_path_rows(path);
    let mut points = Vec::new();
    for row in rows {
        if let Some(row) = fit(&row) {
            points.push(row);
        } else if !points.is_empty() {
            emit_ribbon_strip(b, &points, outward, color, true);
            points.clear();
        }
    }
    emit_ribbon_strip(b, &points, outward, color, true);
}

/// An attachment can cross an opening or the top of an outer garment.
/// Preserve the body underneath instead of tracing only that clipped layer.
pub(super) fn torso_layer_support(
    skin: &crate::sdf::Sculpt,
    garment: &crate::sdf::Sculpt,
) -> crate::sdf::Sculpt {
    let mut support = skin.clone();
    support.union(garment.clone(), 0.0);
    support
}

// Torso attachments reserve the same layers that the tabard renderer emits.
pub(super) const TABARD_SURFACE_OFFSET: f32 = 0.014;
pub(in crate::creatures) const BALDRIC_SURFACE_OFFSET: f32 = 0.017;
pub(super) const EMBLEM_SURFACE_OFFSET: f32 = 0.022;
pub(super) const EMBLEM_FIELD_OFFSET: f32 = 0.026;

/// Heraldry is laid onto the tabard like embroidery. Project each small
/// triangle, so a badge follows both the chest and the hollow below it.
#[allow(clippy::too_many_arguments)]
pub(super) fn fitted_emblem(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    center: Vec2,
    size: f32,
    front: bool,
    color: Vec3,
    field: Vec3,
    kind: u8,
) {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let triangle = |b: &mut Builder, tri: [Vec2; 3], offset: f32, pigment: Vec3| {
        let steps = (tri[0]
            .distance(tri[1])
            .max(tri[1].distance(tri[2]))
            .max(tri[2].distance(tri[0]))
            * size
            / 0.012)
            .ceil()
            .max(1.0) as usize;
        let points: Vec<Vec<Vec3>> = (0..=steps)
            .map(|row| {
                (0..=steps - row)
                    .map(|column| {
                        let uv = tri[0]
                            + (tri[1] - tri[0]) * (row as f32 / steps as f32)
                            + (tri[2] - tri[0]) * (column as f32 / steps as f32);
                        let at = center + Vec2::new(uv.x * outward.z, uv.y) * size;
                        torso_surface(form, at.x, at.y, front) + outward * offset
                    })
                    .collect()
            })
            .collect();
        for row in 0..steps {
            for column in 0..steps - row {
                let a = [
                    points[row][column],
                    points[row + 1][column],
                    points[row][column + 1],
                ];
                b.tri_out(a, a[0] - outward * 0.1, pigment);
                if column + row + 1 < steps {
                    let z = [
                        points[row + 1][column],
                        points[row + 1][column + 1],
                        points[row][column + 1],
                    ];
                    b.tri_out(z, z[0] - outward * 0.1, pigment);
                }
            }
        }
    };
    let disc = |b: &mut Builder, at: Vec2, radius: f32, offset: f32, pigment: Vec3| {
        for k in 0..32 {
            let angle = k as f32 * TAU / 32.0;
            let next = (k + 1) as f32 * TAU / 32.0;
            triangle(
                b,
                [
                    at,
                    at + Vec2::new(angle.cos(), angle.sin()) * radius,
                    at + Vec2::new(next.cos(), next.sin()) * radius,
                ],
                offset,
                pigment,
            );
        }
    };
    match kind % 4 {
        0 => {
            disc(b, Vec2::ZERO, 0.38, EMBLEM_SURFACE_OFFSET, color);
            for k in 0..8 {
                let angle = k as f32 * TAU / 8.0;
                let dir = Vec2::new(angle.cos(), angle.sin());
                let side = Vec2::new(-dir.y, dir.x) * 0.1;
                let radius = if k % 2 == 0 { 0.8 } else { 0.66 };
                triangle(
                    b,
                    [dir * 0.42 + side, dir * radius, dir * 0.42 - side],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
        }
        1 => {
            for k in 0..16 {
                let angle = k as f32 * TAU / 16.0;
                let next = (k + 1) as f32 * TAU / 16.0;
                let radius = |i: usize| match i % 4 {
                    0 => 0.85,
                    2 => 0.55,
                    _ => 0.22,
                };
                triangle(
                    b,
                    [
                        Vec2::ZERO,
                        Vec2::new(angle.cos(), angle.sin()) * radius(k),
                        Vec2::new(next.cos(), next.sin()) * radius(k + 1),
                    ],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
            disc(b, Vec2::ZERO, 0.15, EMBLEM_FIELD_OFFSET, field);
        }
        2 => {
            disc(b, Vec2::ZERO, 0.62, EMBLEM_SURFACE_OFFSET, color);
            disc(b, Vec2::new(0.0, 0.26), 0.56, EMBLEM_FIELD_OFFSET, field);
        }
        _ => {
            for (x, height, width) in [(0.0, 0.85, 0.3), (-0.3, 0.45, 0.2), (0.3, 0.45, 0.2)] {
                triangle(
                    b,
                    [
                        Vec2::new(x - width, -0.6),
                        Vec2::new(x * 0.6, height),
                        Vec2::new(x + width, -0.6),
                    ],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
            disc(b, Vec2::new(0.0, -0.45), 0.32, EMBLEM_SURFACE_OFFSET, color);
        }
    }
}

/// A belt or edging seated on the whole circumference of an authored torso.
pub(super) fn fitted_band(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    low: f32,
    high: f32,
    offset: f32,
    color: Vec3,
) {
    let at = |y: f32, angle: f32| {
        let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
        form.onto(Vec3::Y * y + outward * 0.45, -outward) + outward * offset
    };
    let points: [[Vec3; 65]; 2] = [low, high]
        .map(|height| std::array::from_fn(|column| at(height, column as f32 * TAU / 64.0)));
    for column in 0..64 {
        b.quad_out(
            [
                points[0][column],
                points[0][column + 1],
                points[1][column + 1],
                points[1][column],
            ],
            Vec3::Y * (low + high) * 0.5,
            color,
        );
    }
}
