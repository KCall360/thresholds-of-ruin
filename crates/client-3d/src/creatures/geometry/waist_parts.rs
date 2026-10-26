//! Thin bands fitted to the emitted support, including asymmetric hip sections.
use super::*;

pub(super) fn point(
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    height: f32,
    angle: f32,
    gap: f32,
) -> Vec3 {
    let out = Vec3::new(angle.sin(), 0.0, angle.cos());
    let y = height - body.torso.y;
    let surface = pelvis_surface(support, y, out);
    let mut near = surface.dot(out);
    let mut far = near + 0.04;
    // Keep the authored height while offsetting by real surface clearance.
    // A radial offset alone gets thinner on a steeply flaring hip.
    for _ in 0..14 {
        let radius = (near + far) * 0.5;
        let sample = out * radius + Vec3::Y * y;
        if support.distance(sample) < gap {
            near = radius;
        } else {
            far = radius;
        }
    }
    out * ((near + far) * 0.5) + Vec3::Y * height
}

pub(super) fn band(
    b: &mut Builder,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    heights: Vec2,
    color: Vec3,
) {
    slanted_band(b, body, support, heights, Vec2::new(0.0, 0.004), color);
}

/// A fitted strap: tilt is its sinusoidal rise around the waist; clearance
/// separates crossing straps without changing the underlying support.
pub(super) fn slanted_band(
    b: &mut Builder,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    heights: Vec2,
    tilt_clearance: Vec2,
    color: Vec3,
) {
    const SIDES: usize = 48;
    const ROWS: usize = 4;
    let rings: Vec<_> = (0..=ROWS)
        .map(|row| {
            let height = heights.x + (heights.y - heights.x) * row as f32 / ROWS as f32;
            std::array::from_fn::<_, SIDES, _>(|k| {
                let angle = k as f32 * TAU / SIDES as f32;
                let height = height + tilt_clearance.x * angle.sin();
                [
                    point(body, support, height, angle, tilt_clearance.y),
                    point(body, support, height, angle, tilt_clearance.y + 0.003),
                ]
            })
        })
        .collect();
    let mut quad = |points: [Vec3; 4], facing: Vec3| {
        let inside = points.iter().copied().sum::<Vec3>() * 0.25 - facing * 0.01;
        b.tri_out([points[0], points[1], points[2]], inside, color);
        b.tri_out([points[0], points[2], points[3]], inside, color);
    };
    for k in 0..SIDES {
        let next = (k + 1) % SIDES;
        let angle = (k as f32 + 0.5) * TAU / SIDES as f32;
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        for row in 0..ROWS {
            for (side, facing) in [(0, -1.0), (1, 1.0)] {
                quad(
                    [
                        rings[row][k][side],
                        rings[row][next][side],
                        rings[row + 1][next][side],
                        rings[row + 1][k][side],
                    ],
                    out * facing,
                );
            }
        }
        for (row, facing) in [(0, Vec3::NEG_Y), (ROWS, Vec3::Y)] {
            quad(
                [
                    rings[row][k][0],
                    rings[row][next][0],
                    rings[row][next][1],
                    rings[row][k][1],
                ],
                facing,
            );
        }
    }
}

/// A soft bag fitted over the union of the clothed waist and pelvic support.
/// The same radial fit carries its back, flap and the loops rising to the belt.
pub(super) fn pouch(
    b: &mut Builder,
    body: HumanoidBody,
    support: &crate::sdf::Sculpt,
    color: Vec3,
) {
    let pelvis = pelvis_shape(body);
    let fit = |height: f32, angle: f32, clearance: f32| {
        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
        let (mut near, mut far) = (0.0, 0.7 * body.build);
        for _ in 0..18 {
            let radius = (near + far) * 0.5;
            let at = out * radius + Vec3::Y * height;
            let distance = support.distance(at - body.torso).min(pelvis.distance(at));
            if distance < clearance {
                near = radius;
            } else {
                far = radius;
            }
        }
        out * ((near + far) * 0.5) + Vec3::Y * height
    };
    let radius = fit(0.0, 0.9, 0.012).with_y(0.0).length();
    let half_angle = 0.035 * body.build / radius;
    let bag = |u: f32, v: f32, front: bool| {
        // Slightly narrowed corners and a convex front give the leather volume.
        let width = 0.86 + 0.14 * (PI * v).sin();
        let angle = 0.9 + (2.0 * u - 1.0) * half_angle * width;
        let height = (0.035 - 0.095 * v) * body.build;
        let thickness = if front {
            (0.012 + 0.025 * (PI * u).sin() * (PI * v).sin()) * body.build
        } else {
            0.0
        };
        fit(height, angle, 0.012) + Vec3::new(angle.sin(), 0.0, angle.cos()) * thickness
    };
    const ROWS: usize = 10;
    const COLUMNS: usize = 8;
    for row in 0..ROWS {
        let v0 = row as f32 / ROWS as f32;
        let v1 = (row + 1) as f32 / ROWS as f32;
        for col in 0..COLUMNS {
            let u0 = col as f32 / COLUMNS as f32;
            let u1 = (col + 1) as f32 / COLUMNS as f32;
            for front in [false, true] {
                let q = [
                    bag(u0, v0, front),
                    bag(u1, v0, front),
                    bag(u1, v1, front),
                    bag(u0, v1, front),
                ];
                let inside = (bag((u0 + u1) * 0.5, (v0 + v1) * 0.5, false)
                    + bag((u0 + u1) * 0.5, (v0 + v1) * 0.5, true))
                    * 0.5;
                b.quad_out(q, inside, color * if front { 1.0 } else { 0.85 });
            }
        }
        for u in [0.0, 1.0] {
            b.quad_out(
                [
                    bag(u, v0, false),
                    bag(u, v0, true),
                    bag(u, v1, true),
                    bag(u, v1, false),
                ],
                bag(0.5, (v0 + v1) * 0.5, true),
                color * 0.9,
            );
        }
    }
    for v in [0.0, 1.0] {
        for col in 0..COLUMNS {
            let u0 = col as f32 / COLUMNS as f32;
            let u1 = (col + 1) as f32 / COLUMNS as f32;
            b.quad_out(
                [
                    bag(u0, v, false),
                    bag(u1, v, false),
                    bag(u1, v, true),
                    bag(u0, v, true),
                ],
                bag((u0 + u1) * 0.5, 0.5, true),
                color * 0.9,
            );
        }
    }
    // A raised flap edge and two leather loops make the attachment readable.
    let flap: Vec<_> = (0..=16)
        .map(|i| {
            let u = i as f32 / 16.0;
            let angle = 0.9 + (2.0 * u - 1.0) * half_angle;
            bag(u, 0.29 + 0.08 * (PI * u).sin(), true)
                + Vec3::new(angle.sin(), 0.0, angle.cos()) * 0.0015
        })
        .collect();
    tube(b, &flap, &vec![(0.0018, 0.0018); flap.len()], color * 0.68);
    for side in [-1.0, 1.0] {
        for row in 0..8 {
            let y0 =
                0.032 * body.build + (BELT_TOP_Y + 0.005 - 0.032 * body.build) * row as f32 / 8.0;
            let y1 = 0.032 * body.build
                + (BELT_TOP_Y + 0.005 - 0.032 * body.build) * (row + 1) as f32 / 8.0;
            let a0 = 0.9 + (side * 0.019 - 0.004) * body.build / radius;
            let a1 = 0.9 + (side * 0.019 + 0.004) * body.build / radius;
            let q = [
                fit(y0, a0, 0.014),
                fit(y0, a1, 0.014),
                fit(y1, a1, 0.014),
                fit(y1, a0, 0.014),
            ];
            b.quad_out(q, Vec3::Y * ((y0 + y1) * 0.5), color * 0.8);
        }
    }
}

/// The fitted leather band and its buckle stay in one attachment frame.
pub(super) fn belt(p: &mut Parts, body: HumanoidBody, support: &crate::sdf::Sculpt) {
    band(
        &mut p.satin,
        body,
        support,
        Vec2::new(BELT_BOTTOM_Y, BELT_TOP_Y),
        LEATHER * 0.8,
    );
    p.metal.cuboid(
        point(body, support, BELT_CENTER_Y, 0.0, 0.008),
        Vec3::new(0.035, 0.03, 0.012),
        Quat::IDENTITY,
        Vec3::new(0.74, 0.5, 0.26),
    );
}
