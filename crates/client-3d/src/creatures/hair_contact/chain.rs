//! Small control chains for hanging geometry. No mesh/material ownership lives here.
use bevy::prelude::*;

/// Fit a chain against a distance field while preserving its authored segment
/// lengths. Pinned controls represent the scalp/socket; radii reserve thickness.
/// The caller supplies posed targets and a distance field that stays fixed during
/// this call. Equal query positions must return equal distances.
pub(super) fn fit(
    targets: &[Vec3],
    radii: &[f32],
    pinned: &[bool],
    distance: impl Fn(Vec3) -> f32,
) -> Vec<Vec3> {
    solve(targets, radii, pinned, targets, targets, false, &distance)
}

/// Continue an existing fit; retain its contact side while restoring the target.
pub(super) fn advance(
    previous: &[Vec3],
    targets: &[Vec3],
    rest: &[Vec3],
    dt: f32,
    radii: &[f32],
    pinned: &[bool],
    distance: impl Fn(Vec3) -> f32,
) -> Vec<Vec3> {
    assert!(dt.is_finite() && dt >= 0.0);
    assert_eq!(previous.len(), targets.len());
    assert_eq!(previous.len(), rest.len());
    assert_eq!(previous.len(), radii.len());
    assert_eq!(previous.len(), pinned.len());
    if dt == 0.0 {
        return previous.to_vec();
    }
    // Recovery is a time-dependent prediction, not a side effect of the
    // constraint iteration budget. Contact sweeps still bound the prediction.
    const RECOVERY_RATE: f32 = 60.0;
    let blend = -(-RECOVERY_RATE * dt).exp_m1();
    let destinations: Vec<_> = previous
        .iter()
        .zip(targets)
        .zip(pinned)
        .map(|((&from, &to), &fixed)| if fixed { to } else { from.lerp(to, blend) })
        .collect();
    let predicted = transport(previous, &destinations, radii, pinned, &distance);
    solve(targets, radii, pinned, &predicted, rest, true, &distance)
}

/// Carry free controls with an attachment while retaining their contact side.
/// Pinned controls follow their authored socket exactly.
pub(super) fn transport(
    previous: &[Vec3],
    destinations: &[Vec3],
    radii: &[f32],
    pinned: &[bool],
    distance: impl Fn(Vec3) -> f32,
) -> Vec<Vec3> {
    assert_eq!(previous.len(), destinations.len());
    assert_eq!(previous.len(), radii.len());
    assert_eq!(previous.len(), pinned.len());
    previous
        .iter()
        .zip(destinations)
        .zip(radii)
        .zip(pinned)
        .map(|(((&from, &to), &radius), &pinned)| {
            if pinned {
                to
            } else {
                move_with_contact(from, to, radius, &distance)
            }
        })
        .collect()
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    #[test]
    fn attachment_transport_cannot_cross_an_obstacle_with_clear_endpoints() {
        let previous = [Vec3::new(-1.0, 2.0, 0.0), Vec3::NEG_X];
        let destinations = [Vec3::new(1.0, 2.0, 0.0), Vec3::X];
        let field = |p: Vec3| p.length() - 0.25;
        assert!(field(previous[1]) > 0.1 && field(destinations[1]) > 0.1);
        let carried = transport(&previous, &destinations, &[0.1; 2], &[true, false], field);
        assert_eq!(carried[0], destinations[0], "socket remains exact");
        assert!(
            carried[1].x < -0.34,
            "free control tunneled: {:?}",
            carried[1]
        );
        assert!(field(carried[1]) >= 0.099);
    }
    #[test]
    fn settled_chain_does_not_repeat_distance_queries() {
        use std::cell::Cell;
        let rest = [Vec3::ZERO, Vec3::Y, Vec3::Y * 2.0];
        for animated in [false, true] {
            let queries = Cell::new(0usize);
            let fitted = solve(
                &rest,
                &[0.02; 3],
                &[true, false, false],
                &rest,
                &rest,
                animated,
                &|_| {
                    queries.set(queries.get() + 1);
                    10.0
                },
            );
            assert_eq!(fitted, rest);
            assert!(
                queries.get() < 100,
                "settled chain repeated queries: animated={animated}, queries={}",
                queries.get()
            );
        }
    }

    #[test]
    fn stationary_overlapping_control_is_still_depenetrated() {
        for animated in [false, true] {
            let rest = [Vec3::ZERO];
            let fitted = solve(&rest, &[0.02], &[false], &rest, &rest, animated, &|p| {
                p.x - 1.0
            });
            assert!(fitted[0].x >= 1.02, "overlap remained: {fitted:?}");
            assert_eq!(fitted[0].y, 0.0);
            assert_eq!(fitted[0].z, 0.0);
        }
    }

    #[test]
    fn unobstructed_contact_move_queries_its_start_once() {
        use std::cell::Cell;
        let queries = Cell::new(0usize);
        let moved = move_with_contact(Vec3::ZERO, Vec3::new(0.3, 0.2, 0.1), 0.02, &|_| {
            queries.set(queries.get() + 1);
            10.0
        });
        assert_eq!(moved, Vec3::new(0.3, 0.2, 0.1));
        assert_eq!(queries.get(), 1, "the clear start is queried once");
    }

    #[test]
    fn unobstructed_attachment_transport_reaches_its_destination() {
        let result = transport(
            &[Vec3::ZERO],
            &[Vec3::new(0.3, 0.2, 0.1)],
            &[0.02],
            &[false],
            |_| 10.0,
        );
        assert_eq!(result, [Vec3::new(0.3, 0.2, 0.1)]);
    }
}

fn solve(
    targets: &[Vec3],
    radii: &[f32],
    pinned: &[bool],
    initial: &[Vec3],
    rest: &[Vec3],
    animated: bool,
    distance: &impl Fn(Vec3) -> f32,
) -> Vec<Vec3> {
    assert_eq!(targets.len(), radii.len());
    assert_eq!(targets.len(), pinned.len());
    assert_eq!(targets.len(), initial.len());
    let mut points = initial.to_vec();
    let lengths: Vec<_> = rest.windows(2).map(|p| p[0].distance(p[1])).collect();
    let mut before = points.clone();
    for _ in 0..48 {
        before.copy_from_slice(&points);
        // A few inexpensive length sweeps distribute contact along the lock.
        for _ in 0..4 {
            for (i, &length) in lengths.iter().enumerate() {
                let delta = points[i + 1] - points[i];
                let current = delta.length();
                let mobility = usize::from(!pinned[i]) + usize::from(!pinned[i + 1]);
                if current < 1e-7 || mobility == 0 {
                    continue;
                }
                let correction = delta * ((current - length) / current / mobility as f32);
                if !pinned[i] {
                    points[i] = apply_movement(
                        points[i],
                        points[i] + correction,
                        radii[i],
                        distance,
                        animated,
                    );
                }
                if !pinned[i + 1] {
                    points[i + 1] = apply_movement(
                        points[i + 1],
                        points[i + 1] - correction,
                        radii[i + 1],
                        distance,
                        animated,
                    );
                }
            }
        }
        for i in 0..points.len() {
            if !pinned[i] {
                let correction = contact(points[i], radii[i], distance);
                points[i] += correction;
            }
        }
        // Endpoints alone miss a segment passing through an obstacle.
        for i in 0..lengths.len() {
            let mobility = usize::from(!pinned[i]) + usize::from(!pinned[i + 1]);
            if mobility == 0 {
                continue;
            }
            for t in [0.25, 0.5, 0.75] {
                let p = points[i].lerp(points[i + 1], t);
                let radius = radii[i] + (radii[i + 1] - radii[i]) * t;
                let weights = [1.0 - t, t];
                let mass: f32 = (0..2)
                    .filter(|&k| !pinned[i + k])
                    .map(|k| weights[k] * weights[k])
                    .sum();
                let correction = contact(p, radius, distance);
                for k in 0..2 {
                    if !pinned[i + k] {
                        points[i + k] = apply_movement(
                            points[i + k],
                            points[i + k] + correction * weights[k] / mass,
                            radii[i + k],
                            distance,
                            animated,
                        );
                    }
                }
            }
        }
        // A complete pass is a deterministic function of these controls and the
        // static field. An exact fixed point cannot improve with more passes.
        // Compare bits (including signed zero); no clearance/strain tolerance is
        // introduced, and overlapping stationary controls still get a full pass.
        if points.iter().zip(&before).all(|(after, before)| {
            after.to_array().map(f32::to_bits) == before.to_array().map(f32::to_bits)
        }) {
            break;
        }
    }
    points
}

fn apply_movement(
    from: Vec3,
    to: Vec3,
    radius: f32,
    distance: &impl Fn(Vec3) -> f32,
    animated: bool,
) -> Vec3 {
    if animated {
        move_with_contact(from, to, radius, distance)
    } else {
        to
    }
}

/// Bound a control correction through a static signed-distance field, then slide
/// the remaining motion in the contact tangent plane. A finite query budget stops
/// at the last reached point rather than jumping to an unverified destination.
/// Initial overlaps still need depenetration; moving-collider sweeps are separate.
fn move_with_contact(from: Vec3, to: Vec3, radius: f32, distance: &impl Fn(Vec3) -> f32) -> Vec3 {
    let mut point = from;
    // The field at `point` when depenetration last queried it there. The
    // sweep's first step reuses it instead of asking again for the same bits.
    let mut known = None;
    for _ in 0..8 {
        let field = distance(point);
        let gap = field - radius;
        let correction = if gap >= 0.0 {
            Vec3::ZERO
        } else {
            contact_normal(point, distance) * (-gap + 0.0001)
        };
        if correction.length_squared() < 1e-12 {
            known = Some(field);
            break;
        }
        point += correction;
    }
    let mut target = to;
    for _ in 0..4 {
        let delta = target - point;
        if delta.length_squared() < 1e-12 {
            return point;
        }
        let direction = delta.normalize();
        let mut hit = false;
        for _ in 0..512 {
            let gap = known.take().unwrap_or_else(|| distance(point)) - radius;
            if gap < 0.00001 {
                hit = true;
                break;
            }
            let remaining = point.distance(target);
            if gap >= remaining {
                return target;
            }
            point += direction * (gap * 0.9);
        }
        if !hit {
            return point;
        }
        let normal = contact_normal(point, distance);
        let remaining = target - point;
        let tangent = remaining - normal * remaining.dot(normal).min(0.0);
        if tangent.length_squared() < 1e-12 {
            return point;
        }
        point += normal * 0.001;
        target = point + tangent;
    }
    point
}

fn contact_normal(p: Vec3, distance: &impl Fn(Vec3) -> f32) -> Vec3 {
    let e = 0.001;
    Vec3::new(
        distance(p + Vec3::X * e) - distance(p - Vec3::X * e),
        distance(p + Vec3::Y * e) - distance(p - Vec3::Y * e),
        distance(p + Vec3::Z * e) - distance(p - Vec3::Z * e),
    )
    .normalize_or_zero()
}

fn contact(p: Vec3, radius: f32, distance: &impl Fn(Vec3) -> f32) -> Vec3 {
    let gap = distance(p) - radius;
    if gap >= 0.0 {
        return Vec3::ZERO;
    }
    let normal = contact_normal(p, distance);
    normal * (-gap + 0.0001)
}

/// Bind a visible surface to the fitted control chain.
pub(super) struct Deformer<'a> {
    source: &'a [Vec3],
    fitted: &'a [Vec3],
    rotations: Vec<Quat>,
}
impl<'a> Deformer<'a> {
    pub(super) fn new(source: &'a [Vec3], fitted: &'a [Vec3]) -> Self {
        assert_eq!(source.len(), fitted.len());
        assert!(source.len() >= 2);
        let segments: Vec<_> = source
            .windows(2)
            .zip(fitted.windows(2))
            .map(|(a, b)| {
                Quat::from_rotation_arc(
                    (a[1] - a[0]).normalize_or(Vec3::Y),
                    (b[1] - b[0]).normalize_or(Vec3::Y),
                )
            })
            .collect();
        // Both segments use the same orientation at their shared control.
        // Interpolating unit rotations preserves each section's radial offset.
        let rotations = (0..source.len())
            .map(|i| {
                if i == 0 {
                    segments[0]
                } else if i == source.len() - 1 {
                    segments[i - 1]
                } else {
                    segments[i - 1].slerp(segments[i], 0.5)
                }
            })
            .collect();
        Self {
            source,
            fitted,
            rotations,
        }
    }
    pub(super) fn point(&self, point: Vec3, segment: usize, t: f32) -> Vec3 {
        let source_center = self.source[segment].lerp(self.source[segment + 1], t);
        let target_center = self.fitted[segment].lerp(self.fitted[segment + 1], t);
        target_center
            + self.rotations[segment].slerp(self.rotations[segment + 1], t)
                * (point - source_center)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn recovery_targets_cannot_change_rest_segment_lengths() {
        let rest = [Vec3::ZERO, Vec3::NEG_Y];
        let targets = [Vec3::ZERO, Vec3::NEG_Y * 2.0];
        let fitted = advance(
            &rest,
            &targets,
            &rest,
            0.02,
            &[0.02; 2],
            &[true, false],
            |_| 10.0,
        );
        assert_eq!(fitted[0], rest[0]);
        assert!(
            (fitted[1].distance(fitted[0]) - 1.0).abs() < 1e-5,
            "target stretched rest geometry: {fitted:?}"
        );
    }
    #[test]
    fn free_recovery_depends_on_elapsed_time_not_update_count() {
        let targets = [Vec3::ZERO, Vec3::Y];
        let start = [Vec3::X, Vec3::X + Vec3::Y];
        let run = |hz: usize| {
            let mut points = start.to_vec();
            for _ in 0..hz / 10 {
                points = advance(
                    &points,
                    &targets,
                    &targets,
                    1.0 / hz as f32,
                    &[0.02; 2],
                    &[false; 2],
                    |_| 10.0,
                );
            }
            points
        };
        let reference = run(120);
        for hz in [20, 50, 60] {
            let actual = run(hz);
            for (a, b) in actual.iter().zip(&reference) {
                assert!(
                    a.distance(*b) < 0.00001,
                    "rate {hz}: {actual:?} versus {reference:?}"
                );
            }
            assert!((actual[0].distance(actual[1]) - 1.0).abs() < 0.00001);
        }
        assert!(
            reference[0].x > 0.0 && reference[0].x < 0.1,
            "recovery must advance, without snapping"
        );
    }

    #[test]
    fn zero_elapsed_time_preserves_existing_controls() {
        let previous = [Vec3::X];
        assert_eq!(
            advance(
                &previous,
                &[Vec3::ZERO],
                &[Vec3::ZERO],
                0.0,
                &[0.02],
                &[false],
                |_| 10.0
            ),
            previous
        );
    }
    #[test]
    fn animated_fit_keeps_contact_side_and_recovers_when_released() {
        let previous = [Vec3::new(-0.3, 0.0, 0.0)];
        let targets = [Vec3::new(0.3, 0.0, 0.0)];
        let radii = [0.02];
        let pinned = [false];
        let pose = advance(&previous, &targets, &targets, 0.016, &radii, &pinned, |p| {
            p.length() - 0.1
        });
        assert!(
            pose[0].x < -0.1199,
            "lost the prior contact side: {:?}",
            pose[0]
        );
        let mut released = pose;
        for _ in 0..12 {
            released = advance(
                &released,
                &targets,
                &targets,
                0.016,
                &radii,
                &pinned,
                |_| 1.0,
            );
        }
        assert!(
            released[0].distance(targets[0]) < 0.0001,
            "failed to recover authored pose: {:?}",
            released[0]
        );
    }

    #[test]
    fn control_motion_cannot_cross_obstacle_and_slides_along_a_wall() {
        let from = Vec3::new(-0.3, 0.0, 0.0);
        let to = Vec3::new(0.3, 0.1, 0.0);
        let sphere = |p: Vec3| p.length() - 0.1;
        let result = move_with_contact(from, to, 0.02, &sphere);
        assert!(
            result.x < 0.0,
            "crossed sphere despite clear endpoints: {result:?}"
        );
        assert!(sphere(result) >= 0.0199);
        let result = move_with_contact(
            Vec3::new(0.0, 0.0, 0.1),
            Vec3::new(0.2, 0.0, -0.1),
            0.02,
            &|p| p.z,
        );
        assert!(
            result.x > 0.19 && result.z >= 0.0199,
            "failed to slide: {result:?}"
        );
        assert_eq!(move_with_contact(from, to, 0.02, &|_| 1.0), to);
    }

    use super::*;
    #[test]
    fn skin_is_continuous_across_a_bending_control_joint() {
        let source = [Vec3::Y * 0.2, Vec3::ZERO, Vec3::NEG_Y * 0.2];
        let fitted = [Vec3::Y * 0.2, Vec3::ZERO, Vec3::new(0.12, -0.16, 0.0)];
        let deformation = Deformer::new(&source, &fitted);
        let a = Vec3::new(0.04, 0.00001, 0.0);
        let b = Vec3::new(0.04, -0.00001, 0.0);
        let left = deformation.point(a, 0, 0.99995);
        let right = deformation.point(b, 1, 0.00005);
        assert!(
            left.distance(right) < 0.0002,
            "adjacent surface points split at the control joint: {}",
            left.distance(right)
        );
        // A chain following one rigid transform must preserve the surface exactly.
        let rotation = Quat::from_rotation_z(0.4);
        let translation = Vec3::new(0.3, 0.1, -0.2);
        let rigid = source.map(|p| rotation * p + translation);
        let deformation = Deformer::new(&source, &rigid);
        for (point, segment, t) in [(a, 0, 0.99995), (b, 1, 0.00005)] {
            assert!(
                deformation
                    .point(point, segment, t)
                    .distance(rotation * point + translation)
                    < 1e-5
            );
        }
    }

    #[test]
    fn chain_slides_along_surface_with_fixed_socket_and_reserved_thickness() {
        let targets = [
            Vec3::new(0.0, 0.5, 0.12),
            Vec3::new(0.0, 0.3, -0.02),
            Vec3::new(0.0, 0.1, -0.04),
        ];
        let radii = [0.025; 3];
        let pinned = [true, false, false];
        let pose = fit(&targets, &radii, &pinned, |p| p.z);
        assert_eq!(pose[0], targets[0]);
        for p in &pose {
            assert!(p.z >= 0.0249);
        }
        for i in 0..2 {
            assert!(
                (pose[i].distance(pose[i + 1]) - targets[i].distance(targets[i + 1])).abs() < 0.002
            );
        }
        let shifted = targets.map(|p| p + Vec3::X * 0.2);
        let slide = fit(&shifted, &radii, &pinned, |p| p.z);
        for (a, b) in pose.iter().zip(slide) {
            assert!(b.distance(*a + Vec3::X * 0.2) < 1e-5);
        }
        let released = fit(&targets, &radii, &pinned, |_| 1.0);
        assert_eq!(released, targets, "removed contact restores authored pose");
    }
}
