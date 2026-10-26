//! Authored surface provenance, independent of mesh ownership and motion solver.
//! Parameters identify locations on a recipe's controls, never renderer entities.
use bevy::prelude::*;

/// Intended support side belongs to the recipe, not to nearest-point contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SupportSide {
    Front,
    Back,
}

#[derive(Clone, Debug)]
pub(super) struct ControlCurve {
    pub points: Vec<(Vec3, f32)>,
    pub attached: Vec<bool>,
    pub support_side: Option<SupportSide>,
}

impl ControlCurve {
    pub fn mobility(&self, segment: usize, t: f32) -> f32 {
        assert_eq!(self.points.len(), self.attached.len());
        let a = if self.attached[segment] { 0.0 } else { 1.0 };
        let b = if self.attached[segment + 1] { 0.0 } else { 1.0 };
        a + (b - a) * t.clamp(0.0, 1.0)
    }
}

/// Adapt a procedural rest pattern before material-length constraints are built.
/// The caller supplies the composed body's surface query in the same frame.
pub(super) fn fit_surface_controls(
    points: &[Vec3],
    radii: &[f32],
    pinned: &[bool],
    side: Option<SupportSide>,
    mut surface: impl FnMut(Vec3, SupportSide, f32) -> Option<Vec3>,
) -> Vec<Vec3> {
    assert_eq!(points.len(), radii.len());
    assert_eq!(points.len(), pinned.len());
    assert!(radii.iter().all(|r| r.is_finite() && *r >= 0.0));
    let Some(side) = side else {
        return points.to_vec();
    };
    points
        .iter()
        .zip(radii)
        .zip(pinned)
        .map(|((&point, &radius), &fixed)| {
            if fixed {
                return point;
            }
            let Some(hit) = surface(point, side, radius) else {
                return point;
            };
            Vec3::new(
                point.x,
                point.y,
                match side {
                    SupportSide::Front => point.z.max(hit.z),
                    SupportSide::Back => point.z.min(hit.z),
                },
            )
        })
        .collect()
}

#[derive(Clone, Debug)]
pub(super) struct SurfaceBinding<T> {
    contributions: Vec<(T, f32)>,
}
impl<T> SurfaceBinding<T> {
    pub fn single(parameter: T) -> Self {
        Self {
            contributions: vec![(parameter, 1.0)],
        }
    }
    pub fn evaluate(&self, mut sample: impl FnMut(&T) -> Vec3) -> Vec3 {
        self.contributions
            .iter()
            .fold(Vec3::ZERO, |p, (parameter, weight)| {
                p + sample(parameter) * *weight
            })
    }
    pub fn evaluate_scalar(&self, mut sample: impl FnMut(&T) -> f32) -> f32 {
        self.contributions
            .iter()
            .map(|(parameter, weight)| sample(parameter) * weight)
            .sum()
    }
    /// The ordered contributors and blend widths must come from the same recipe
    /// as the surface. Bind once in rest space; reuse through every animation.
    #[cfg(test)]
    pub fn smooth_union(sources: impl IntoIterator<Item = (T, f32, f32)>) -> Self {
        let mut sources = sources.into_iter();
        let (first, mut distance, k) = sources.next().expect("surface source");
        assert!(distance.is_finite() && k.is_finite() && k >= 0.0);
        let mut contributions = vec![(first, 1.0)];
        for (parameter, next, k) in sources {
            assert!(next.is_finite() && k.is_finite() && k >= 0.0);
            let (combined, old_weight) = crate::sdf::smooth_min_with_weight(distance, next, k);
            distance = combined;
            if old_weight == 1.0 {
                continue;
            }
            for (_, weight) in &mut contributions {
                *weight *= old_weight;
            }
            contributions.retain(|(_, weight)| *weight > 0.0);
            contributions.push((parameter, 1.0 - old_weight));
        }
        let sum: f32 = contributions.iter().map(|(_, w)| w).sum();
        for (_, weight) in &mut contributions {
            *weight /= sum;
        }
        Self { contributions }
    }
}

/// Build hanging recovery targets in the support frame. Attachments follow the
/// posed socket; free controls must retain the rest shape's support orientation.
pub(super) fn hanging_targets(rest: &[Vec3], posed: &[Vec3], pinned: &[bool]) -> Vec<Vec3> {
    assert_eq!(rest.len(), posed.len());
    assert_eq!(rest.len(), pinned.len());
    let (shift, count) = rest
        .iter()
        .zip(posed)
        .zip(pinned)
        .filter(|(_, fixed)| **fixed)
        .fold((Vec3::ZERO, 0usize), |(sum, count), ((a, b), _)| {
            (sum + *b - *a, count + 1)
        });
    let shift = if count == 0 {
        Vec3::ZERO
    } else {
        shift / count as f32
    };
    rest.iter()
        .zip(posed)
        .zip(pinned)
        .map(|((&rest, &posed), &fixed)| if fixed { posed } else { rest + shift })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn surface_rest_fit_retains_side_clearance_and_authored_pattern() {
        for side in [SupportSide::Front, SupportSide::Back] {
            let sign = if side == SupportSide::Front {
                1.0
            } else {
                -1.0
            };
            let points = [
                Vec3::new(0.1, 0.3, 0.0),
                Vec3::new(0.1, 0.2, 0.0),
                Vec3::new(0.1, 0.1, sign * 0.5),
                Vec3::new(9.0, 0.0, 0.0),
            ];
            let fitted = fit_surface_controls(
                &points,
                &[0.02; 4],
                &[true, false, false, false],
                Some(side),
                |p, selected, radius| {
                    assert_eq!(selected, side);
                    (p.x < 1.0).then_some(Vec3::new(p.x, p.y, sign * (0.2 + radius)))
                },
            );
            assert_eq!(fitted[0], points[0], "pinned attachment moved");
            assert!(
                (fitted[1].z - sign * 0.22).abs() < 1e-6,
                "lost support side or clearance: {fitted:?}"
            );
            assert_eq!(
                fitted[1].xy(),
                points[1].xy(),
                "fitting moved the authored pattern laterally or vertically"
            );
            assert_eq!(fitted[2], points[2], "already-clear detail was flattened");
            assert_eq!(fitted[3], points[3], "a ray miss invented support");
            assert_eq!(
                fit_surface_controls(&points, &[0.02; 4], &[false; 4], None, |_, _, _| panic!(
                    "unconstrained curve queried support"
                )),
                points
            );
        }
    }
    #[test]
    fn hanging_targets_follow_socket_translation_without_rotating_free_fall() {
        let rest = [Vec3::Y, Vec3::ZERO, Vec3::NEG_Y];
        let pose = Transform::from_xyz(0.4, 0.2, 0.1).with_rotation(Quat::from_rotation_z(0.8));
        let posed: Vec<_> = rest.iter().map(|p| pose.transform_point(*p)).collect();
        let target = hanging_targets(&rest, &posed, &[true, false, false]);
        assert_eq!(target[0], posed[0]);
        let shift = posed[0] - rest[0];
        for i in 1..rest.len() {
            assert!(
                target[i].distance(rest[i] + shift) < 1e-6,
                "free fall rotated with the socket"
            );
        }
        assert!((target[2] - target[1]).distance(Vec3::NEG_Y) < 1e-6);
    }
    #[test]
    fn hanging_targets_handle_multiple_or_absent_attachments() {
        let rest = [Vec3::NEG_X, Vec3::X, Vec3::NEG_Y];
        let offset = Vec3::new(0.2, 0.1, -0.3);
        let translated: Vec<_> = rest.iter().map(|p| *p + offset).collect();
        let result = hanging_targets(&rest, &translated, &[true, true, false]);
        for (a, b) in result.iter().zip(&translated) {
            assert!(a.distance(*b) < 1e-6);
        }
        assert_eq!(hanging_targets(&rest, &translated, &[false; 3]), rest);
    }
    fn binding(x: f32) -> SurfaceBinding<usize> {
        SurfaceBinding::smooth_union([
            (0, (x + 0.1).abs() - 0.12, 0.03),
            (1, (x - 0.1).abs() - 0.12, 0.03),
        ])
    }
    #[test]
    fn blended_surface_keeps_neighboring_points_continuous_between_sources() {
        let deform = |x: f32| {
            binding(x).evaluate(|&source| Vec3::new(x, if source == 0 { -0.2 } else { 0.2 }, 0.0))
        };
        assert!(
            deform(-0.000001).distance(deform(0.000001)) < 0.0001,
            "source ownership introduced a tear"
        );
    }
    #[test]
    fn union_provenance_matches_geometry_derivative_and_ignores_distant_sources() {
        let binding =
            SurfaceBinding::smooth_union([(0, 0.02, 0.03), (1, 0.03, 0.03), (2, 10.0, 0.03)]);
        assert!(binding.contributions.iter().all(|(source, _)| *source != 2));
        let epsilon = 0.00001;
        let derivative = (crate::sdf::smooth_min_with_weight(0.02 + epsilon, 0.03, 0.03).0
            - crate::sdf::smooth_min_with_weight(0.02 - epsilon, 0.03, 0.03).0)
            / (2.0 * epsilon);
        let weight = binding
            .contributions
            .iter()
            .find(|(source, _)| *source == 0)
            .unwrap()
            .1;
        assert!((weight - derivative).abs() < 0.001);
        assert!((binding.contributions.iter().map(|(_, w)| w).sum::<f32>() - 1.0).abs() < 1e-6);
    }
    #[test]
    fn shared_rigid_motion_preserves_the_surface() {
        let motion = Transform::from_xyz(0.3, -0.2, 0.1).with_rotation(Quat::from_rotation_z(0.7));
        for x in [-0.2, -0.001, 0.0, 0.001, 0.2] {
            let expected = motion.transform_point(Vec3::new(x, 0.1, 0.2));
            assert!(binding(x).evaluate(|_| expected).distance(expected) < 1e-6);
        }
    }
    #[test]
    fn authored_cloth_parameters_keep_the_yoke_fixed_and_hem_mobile() {
        let swing = Quat::from_rotation_x(0.4);
        for (row, give) in [(0, 0.0), (1, 0.0), (20, 1.0)] {
            let binding = SurfaceBinding::single(give);
            let rest = Vec3::new(0.2, -row as f32 * 0.05, -0.1);
            let local =
                binding.evaluate(|&t| swing.inverse() * Quat::IDENTITY.slerp(swing, t) * rest);
            let expected = if give == 0.0 { rest } else { swing * rest };
            assert!((swing * local).distance(expected) < 1e-6);
        }
    }
}
