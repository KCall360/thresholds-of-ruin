//! Shared scalar and analytic field evaluation, with optional derivatives.
use super::{smooth_min_with_weight, Sculpt, Shape, Step};
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub(super) struct FieldSample {
    pub distance: f32,
    /// Unnormalized field derivative, including taper, stretch and blending.
    pub gradient: Vec3,
}

impl FieldSample {
    fn negated(self) -> Self {
        Self {
            distance: -self.distance,
            gradient: -self.gradient,
        }
    }

    #[inline]
    fn minimum<const WITH_GRADIENT: bool>(self, other: Self, blend: f32) -> Self {
        let (distance, weight) = smooth_min_with_weight(self.distance, other.distance, blend);
        let gradient = if !WITH_GRADIENT {
            Vec3::ZERO
        } else if weight == 1.0 {
            self.gradient
        } else if weight == 0.0 {
            other.gradient
        } else {
            self.gradient * weight + other.gradient * (1.0 - weight)
        };
        Self { distance, gradient }
    }
}

impl Shape {
    #[cfg(test)]
    pub(super) fn sample(self, point: Vec3) -> FieldSample {
        self.evaluate::<true>(point)
    }

    // One primitive formula serves both paths. Constant specialization removes
    // derivative work from ordinary scalar sampling.
    #[inline]
    pub(super) fn evaluate<const WITH_GRADIENT: bool>(self, point: Vec3) -> FieldSample {
        match self {
            Shape::Ellipsoid(center, radii) => {
                let offset = point - center;
                let squared = radii * radii;
                let k0 = (offset / radii).length();
                let k1 = (offset / squared).length();
                if k1 < 1e-6 {
                    return FieldSample {
                        distance: -radii.min_element(),
                        gradient: Vec3::ZERO,
                    };
                }
                let distance = k0 * (k0 - 1.0) / k1;
                let gradient = if WITH_GRADIENT {
                    let g0 = offset / (squared * k0);
                    let g1 = offset / (squared * squared * k1);
                    g0 * ((2.0 * k0 - 1.0) / k1) - g1 * (distance / k1)
                } else {
                    Vec3::ZERO
                };
                FieldSample { distance, gradient }
            }
            Shape::Capsule(a, b, ra, rb) => {
                let ab = b - a;
                let length_squared = ab.length_squared();
                let raw_t = (point - a).dot(ab) / length_squared;
                let t = raw_t.clamp(0.0, 1.0);
                let offset = point - (a + ab * t);
                let distance = offset.length() - (ra + (rb - ra) * t);
                let gradient = if WITH_GRADIENT {
                    let radial = offset.normalize_or_zero();
                    // At a clamp boundary either one-sided derivative is valid;
                    // use their mean, matching a symmetric slope at the joint.
                    let along = if raw_t > 0.0 && raw_t < 1.0 {
                        ab / length_squared
                    } else if raw_t == 0.0 || raw_t == 1.0 {
                        ab / (2.0 * length_squared)
                    } else {
                        Vec3::ZERO
                    };
                    radial - along * (radial.dot(ab) + rb - ra)
                } else {
                    Vec3::ZERO
                };
                FieldSample { distance, gradient }
            }
            Shape::Torus(center, radii, depth) => {
                let offset = point - center;
                let radial_length = Vec2::new(offset.x, offset.z / depth).length();
                let radial = radial_length - radii.x;
                let ring_length = Vec2::new(radial, offset.y).length();
                let distance = ring_length - radii.y;
                let gradient = if WITH_GRADIENT && ring_length > 0.0 {
                    let horizontal = if radial_length > 0.0 {
                        radial / (ring_length * radial_length)
                    } else {
                        0.0
                    };
                    Vec3::new(
                        offset.x * horizontal,
                        offset.y / ring_length,
                        offset.z * horizontal / (depth * depth),
                    )
                } else {
                    Vec3::ZERO
                };
                FieldSample { distance, gradient }
            }
            Shape::Beyond(normal, offset) => FieldSample {
                distance: offset - point.dot(normal),
                gradient: if WITH_GRADIENT { -normal } else { Vec3::ZERO },
            },
        }
    }
}

impl Sculpt {
    pub(super) fn sample(&self, point: Vec3) -> FieldSample {
        self.sample_among(point, self.steps.iter())
    }

    pub(super) fn sample_among<'a>(
        &self,
        point: Vec3,
        steps: impl Iterator<Item = &'a Step>,
    ) -> FieldSample {
        self.evaluate_among::<true>(point, steps)
    }

    /// Symmetric slope for projection at hard seams; preserve the selected
    /// ordered field in every stencil query.
    pub(super) fn sample_symmetric_among<'a>(
        &self,
        point: Vec3,
        epsilon: f32,
        steps: impl Iterator<Item = &'a Step> + Clone,
    ) -> FieldSample {
        let distance = |p| self.distance_among(p, steps.clone());
        FieldSample {
            distance: distance(point),
            gradient: Vec3::new(
                distance(point + Vec3::X * epsilon) - distance(point - Vec3::X * epsilon),
                distance(point + Vec3::Y * epsilon) - distance(point - Vec3::Y * epsilon),
                distance(point + Vec3::Z * epsilon) - distance(point - Vec3::Z * epsilon),
            ) / (2.0 * epsilon),
        }
    }

    #[inline]
    pub(super) fn evaluate_among<'a, const WITH_GRADIENT: bool>(
        &self,
        point: Vec3,
        steps: impl Iterator<Item = &'a Step>,
    ) -> FieldSample {
        let mut result = FieldSample {
            distance: f32::MAX,
            gradient: Vec3::ZERO,
        };
        for step in steps {
            match *step {
                Step::Add(shape, blend) => {
                    let next = shape.evaluate::<WITH_GRADIENT>(point);
                    result = if result.distance == f32::MAX {
                        next
                    } else {
                        result.minimum::<WITH_GRADIENT>(next, blend)
                    };
                }
                Step::Carve(shape, blend) => {
                    result = result
                        .negated()
                        .minimum::<WITH_GRADIENT>(shape.evaluate::<WITH_GRADIENT>(point), blend)
                        .negated();
                }
                Step::Inflate(amount) => result.distance -= amount,
                Step::Union(ref child, blend) => {
                    let next = child.evaluate_among::<WITH_GRADIENT>(point, child.steps.iter());
                    result = if result.distance == f32::MAX {
                        next
                    } else {
                        result.minimum::<WITH_GRADIENT>(next, blend)
                    };
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numerical_gradient(distance: impl Fn(Vec3) -> f32, point: Vec3) -> Vec3 {
        let epsilon = 0.00002;
        Vec3::new(
            distance(point + Vec3::X * epsilon) - distance(point - Vec3::X * epsilon),
            distance(point + Vec3::Y * epsilon) - distance(point - Vec3::Y * epsilon),
            distance(point + Vec3::Z * epsilon) - distance(point - Vec3::Z * epsilon),
        ) / (2.0 * epsilon)
    }

    fn assert_gradient(actual: Vec3, expected: Vec3, label: &str) {
        assert!(actual.is_finite() && expected.is_finite(), "{label}");
        assert!(
            actual.distance(expected) <= 0.005 * (1.0 + expected.length()),
            "{label}: gradient {actual:?}, independent slope {expected:?}"
        );
    }

    #[test]
    fn numerical_fallback_preserves_values_and_averages_hard_seams() {
        let mut field = Sculpt::default();
        field
            .add(Shape::Ellipsoid(Vec3::X * -0.1, Vec3::splat(0.2)), 0.0)
            .add(Shape::Ellipsoid(Vec3::X * 0.1, Vec3::splat(0.2)), 0.0);
        let center = field.sample_symmetric_among(Vec3::ZERO, 0.001, field.steps.iter());
        assert!((center.distance + 0.1).abs() < 1e-6);
        assert!(center.gradient.length() < 1e-5);
        assert!(field.sample(Vec3::ZERO).gradient.distance(Vec3::X) < 1e-5);
        let one = field.sample_symmetric_among(Vec3::ZERO, 0.001, field.steps[1..].iter());
        assert!((one.distance + 0.1).abs() < 1e-6);
        assert_gradient(one.gradient, Vec3::NEG_X, "selected right sphere");
        let point = Vec3::new(0.03, 0.04, -0.02);
        let outside_seam = field.sample_symmetric_among(point, 0.0001, field.steps.iter());
        assert_eq!(
            outside_seam.distance.to_bits(),
            field.distance(point).to_bits()
        );
        assert_gradient(
            outside_seam.gradient,
            (point - Vec3::X * 0.1).normalize(),
            "smooth sphere patch",
        );
    }

    #[test]
    fn primitive_samples_match_independent_values_and_axis_derivatives() {
        let ellipsoid = Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.12, 0.05, 0.08));
        let capsule = Shape::Capsule(Vec3::X * -0.1, Vec3::X * 0.1, 0.04, 0.07);
        let ring = Shape::Torus(Vec3::ZERO, Vec2::new(0.1, 0.02), 1.25);
        let normal = Vec3::new(1.0, 2.0, 2.0) / 3.0;
        let cases = [
            (ellipsoid, Vec3::X * 0.15, 0.03, Vec3::X),
            (ellipsoid, Vec3::Y * 0.08, 0.03, Vec3::Y),
            (ellipsoid, Vec3::Z * -0.11, 0.03, Vec3::NEG_Z),
            (capsule, Vec3::Y * 0.08, 0.025, Vec3::new(-0.15, 1.0, 0.0)),
            (capsule, Vec3::X * 0.2, 0.03, Vec3::X),
            (capsule, Vec3::X * -0.18, 0.04, Vec3::NEG_X),
            (
                capsule,
                Vec3::new(-0.1, 0.08, 0.0),
                0.04,
                Vec3::new(-0.075, 1.0, 0.0),
            ),
            (
                capsule,
                Vec3::new(0.1, 0.08, 0.0),
                0.01,
                Vec3::new(-0.075, 1.0, 0.0),
            ),
            (ring, Vec3::X * 0.12, 0.0, Vec3::X),
            (ring, Vec3::new(0.1, 0.02, 0.0), 0.0, Vec3::Y),
            (ring, Vec3::Z * 0.15, 0.0, Vec3::Z / 1.25),
            (Shape::Beyond(normal, 0.03), normal * 0.08, -0.05, -normal),
        ];
        for &(shape, point, expected, _) in &cases {
            assert!((shape.sample(point).distance - expected).abs() < 1e-6);
        }
        for (shape, point, _, expected) in cases {
            assert!(shape.sample(point).gradient.distance(expected) < 1e-5);
        }
    }

    #[test]
    fn primitive_gradients_follow_numerical_slopes_away_from_singularities() {
        let shapes = [
            Shape::Ellipsoid(
                Vec3::new(0.011, -0.017, 0.023),
                Vec3::new(0.12, 0.045, 0.09),
            ),
            Shape::Capsule(
                Vec3::new(-0.09, 0.013, -0.01),
                Vec3::new(0.12, -0.04, 0.033),
                0.04,
                0.07,
            ),
            Shape::Torus(Vec3::new(0.01, 0.02, -0.03), Vec2::new(0.11, 0.02), 0.75),
            Shape::Torus(Vec3::new(0.01, 0.02, -0.03), Vec2::new(0.11, 0.02), 1.25),
            Shape::Beyond(Vec3::new(0.3, 0.7, 0.2).normalize(), 0.03),
        ];
        for (index, shape) in shapes.into_iter().enumerate() {
            for x in [-0.19, -0.083, 0.017, 0.091, 0.22] {
                for y in [-0.19, -0.083, 0.017, 0.091, 0.22] {
                    for z in [-0.19, -0.083, 0.017, 0.091, 0.22] {
                        let point = Vec3::new(x, y, z);
                        assert_eq!(
                            shape.sample(point).distance.to_bits(),
                            shape.distance(point).to_bits()
                        );
                        assert_gradient(
                            shape.sample(point).gradient,
                            numerical_gradient(|p| shape.distance(p), point),
                            &format!("primitive {index} at {point:?}"),
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn composed_samples_preserve_order_selected_steps_and_nested_layers() {
        let mut child = Sculpt::default();
        child
            .add(
                Shape::Ellipsoid(Vec3::new(0.08, -0.04, 0.0), Vec3::new(0.1, 0.08, 0.09)),
                0.0,
            )
            .carve(Shape::Beyond(Vec3::Y, 0.025), 0.008);
        let mut field = Sculpt::default();
        field
            .add(
                Shape::Ellipsoid(Vec3::new(-0.05, 0.0, 0.0), Vec3::new(0.12, 0.09, 0.08)),
                0.0,
            )
            .add(
                Shape::Capsule(
                    Vec3::new(-0.03, -0.09, 0.01),
                    Vec3::new(0.05, 0.11, 0.02),
                    0.04,
                    0.055,
                ),
                0.04,
            )
            .carve(
                Shape::Ellipsoid(Vec3::new(0.0, 0.02, 0.07), Vec3::new(0.05, 0.035, 0.07)),
                0.015,
            )
            .inflate(0.006)
            .union(child, 0.02);
        for indices in [vec![0, 1, 2, 3, 4], vec![0, 2, 3, 4], vec![4, 3, 1]] {
            for x in [-0.173, -0.071, 0.023, 0.109] {
                for y in [-0.133, -0.041, 0.019, 0.097] {
                    for z in [-0.113, -0.037, 0.031, 0.083] {
                        let point = Vec3::new(x, y, z);
                        let distance =
                            |p| field.distance_among(p, indices.iter().map(|&i| &field.steps[i]));
                        let sample =
                            field.sample_among(point, indices.iter().map(|&i| &field.steps[i]));
                        assert_eq!(sample.distance.to_bits(), distance(point).to_bits());
                        assert_gradient(
                            sample.gradient,
                            numerical_gradient(distance, point),
                            &format!("steps {indices:?} at {point:?}"),
                        );
                    }
                }
            }
        }
        assert_eq!(
            field.sample(Vec3::ZERO).distance.to_bits(),
            field.distance(Vec3::ZERO).to_bits()
        );
    }

    #[test]
    fn singular_points_and_empty_fields_keep_finite_subgradients() {
        for (shape, point) in [
            (
                Shape::Ellipsoid(Vec3::ZERO, Vec3::new(0.12, 0.05, 0.08)),
                Vec3::ZERO,
            ),
            (
                Shape::Capsule(Vec3::ZERO, Vec3::Y * 0.2, 0.04, 0.06),
                Vec3::ZERO,
            ),
            (
                Shape::Capsule(Vec3::ZERO, Vec3::Y * 0.2, 0.04, 0.06),
                Vec3::Y * 0.1,
            ),
            (
                Shape::Torus(Vec3::ZERO, Vec2::new(0.1, 0.02), 1.25),
                Vec3::ZERO,
            ),
            (
                Shape::Torus(Vec3::ZERO, Vec2::new(0.1, 0.02), 1.25),
                Vec3::X * 0.1,
            ),
        ] {
            let sample = shape.sample(point);
            assert_eq!(sample.distance.to_bits(), shape.distance(point).to_bits());
            assert!(sample.gradient.is_finite());
        }
        let mut empty = Sculpt::default();
        empty.inflate(0.01);
        assert_eq!(empty.sample(Vec3::ZERO).distance, f32::MAX);
        assert_eq!(empty.sample(Vec3::ZERO).gradient, Vec3::ZERO);
        empty.carve(Shape::Beyond(Vec3::Y, 0.03), 0.01);
        assert_eq!(empty.sample(Vec3::ZERO).distance, f32::MAX);
        assert_eq!(empty.sample(Vec3::ZERO).gradient, Vec3::ZERO);
        empty.add(Shape::Ellipsoid(Vec3::ZERO, Vec3::splat(0.05)), 0.02);
        let first = empty.sample(Vec3::X * 0.07);
        assert!((first.distance - 0.02).abs() < 1e-6);
        assert!(first.gradient.distance(Vec3::X) < 1e-5);
        let mut tie = Sculpt::default();
        tie.add(Shape::Ellipsoid(Vec3::X * -0.1, Vec3::splat(0.2)), 0.0)
            .add(Shape::Ellipsoid(Vec3::X * 0.1, Vec3::splat(0.2)), 0.0);
        assert!(tie.sample(Vec3::ZERO).gradient.is_finite());
        assert_eq!(
            tie.sample(Vec3::ZERO).distance.to_bits(),
            tie.distance(Vec3::ZERO).to_bits()
        );
    }
}
