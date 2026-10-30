//! Bounded sampling of authored tube paths, independent of normal smoothing.
use bevy::prelude::{Vec2, Vec3};
use std::f32::consts::PI;

// Model-space silhouette tolerance. Painting also retains short spans, or the
// old eight-step spacing when an authored segment was larger than that.
const ERROR: f32 = 0.001;
const PAINT_SPAN: f32 = 0.015;
const MAX_DEPTH: u8 = 6;

pub(super) struct Sample {
    pub center: Vec3,
    pub radius: Vec2,
    pub tangent: Vec3,
}

#[derive(Clone, Copy)]
struct Cubic([Vec3; 4]);
impl Cubic {
    fn span(path: &[Vec3], i: usize) -> Self {
        let previous = path[i.saturating_sub(1)];
        let next = path[(i + 2).min(path.len() - 1)];
        Self([
            path[i],
            path[i] + (path[i + 1] - previous) / 6.0,
            path[i + 1] - (next - path[i]) / 6.0,
            path[i + 1],
        ])
    }
    fn split(self) -> (Self, Self) {
        let [a, b, c, d] = self.0;
        let (ab, bc, cd) = ((a + b) * 0.5, (b + c) * 0.5, (c + d) * 0.5);
        let (abc, bcd) = ((ab + bc) * 0.5, (bc + cd) * 0.5);
        let middle = (abc + bcd) * 0.5;
        (Self([a, ab, abc, middle]), Self([middle, bcd, cd, d]))
    }
    fn at(self, t: f32) -> Vec3 {
        let [a, b, c, d] = self.0;
        let u = 1.0 - t;
        a * u * u * u + b * 3.0 * u * u * t + c * 3.0 * u * t * t + d * t * t * t
    }
    fn flat(self, radius: f32, paint_span: f32) -> bool {
        let [a, b, c, d] = self.0;
        let chord = d - a;
        if chord.length() > paint_span {
            return false;
        }
        if [b, c]
            .into_iter()
            .any(|p| segment_distance(p, a, d) > ERROR)
        {
            return false;
        }
        // The derivative's Bezier controls bound the tangent cone. Limit its
        // angular sag at the tube's radius as well as centerline chord error.
        let axis = chord.normalize_or((b - a).normalize_or(Vec3::Y));
        let min_dot = (1.0 - ERROR / radius.max(ERROR)).clamp(0.0, 1.0);
        [b - a, c - b, d - c]
            .into_iter()
            .all(|v| v.length_squared() < 1e-16 || v.normalize().dot(axis) >= min_dot)
    }
}

fn segment_distance(point: Vec3, a: Vec3, b: Vec3) -> f32 {
    let edge = b - a;
    let t = ((point - a).dot(edge) / edge.length_squared().max(1e-20)).clamp(0.0, 1.0);
    point.distance(a + edge * t)
}

fn subdivide(curve: Cubic, radii: [Vec2; 2], paint_span: f32, depth: u8, out: &mut Vec<Sample>) {
    if depth == MAX_DEPTH || curve.flat(radii[0].max(radii[1]).max_element(), paint_span) {
        out.push(Sample {
            center: curve.0[0],
            radius: radii[0],
            tangent: (curve.0[1] - curve.0[0])
                .normalize_or((curve.0[3] - curve.0[0]).normalize_or(Vec3::Y)),
        });
    } else {
        let (left, right) = curve.split();
        let middle = (radii[0] + radii[1]) * 0.5;
        subdivide(left, [radii[0], middle], paint_span, depth + 1, out);
        subdivide(right, [middle, radii[1]], paint_span, depth + 1, out);
    }
}

pub(super) fn sample(path: &[Vec3], radii: &[Vec2], curved: bool) -> Vec<Sample> {
    assert_eq!(path.len(), radii.len(), "each tube knot needs its radii");
    if path.len() < 2 {
        return path
            .iter()
            .zip(radii)
            .map(|(&center, &radius)| Sample {
                center,
                radius,
                tangent: Vec3::Y,
            })
            .collect();
    }
    let mut out = Vec::with_capacity(path.len());
    for i in 0..path.len() - 1 {
        if curved {
            let curve = Cubic::span(path, i);
            let legacy_span = (0..8)
                .map(|k| {
                    curve
                        .at(k as f32 / 8.0)
                        .distance(curve.at((k + 1) as f32 / 8.0))
                })
                .fold(0.0_f32, f32::max);
            subdivide(
                curve,
                [radii[i], radii[i + 1]],
                PAINT_SPAN.max(legacy_span),
                0,
                &mut out,
            );
        } else {
            out.push(Sample {
                center: path[i],
                radius: radii[i],
                tangent: (path[i + 1] - path[i.saturating_sub(1)]).normalize_or(Vec3::Y),
            });
        }
    }
    let last = path.len() - 1;
    out.push(Sample {
        center: path[last],
        radius: radii[last],
        tangent: (path[last] - path[last - 1]).normalize_or(Vec3::Y),
    });
    out
}

pub(super) fn sides(authored: usize, multiplier: usize, radius: f32, curved: bool) -> usize {
    let cap = authored.saturating_mul(multiplier).clamp(3, 32);
    if !curved {
        return cap;
    }
    if radius <= ERROR {
        return cap.min(authored.max(3));
    }
    let angle = (1.0 - ERROR / radius).clamp(-1.0, 1.0).acos();
    let needed = (PI / angle).ceil().min(cap as f32) as usize;
    let rounded = needed.div_ceil(4) * 4;
    rounded.max(authored.min(32)).clamp(3, cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curved_samples_preserve_knots_radii_and_both_s_bend_lobes() {
        let path = [
            Vec3::new(-0.1, -0.2, 0.0),
            Vec3::ZERO,
            Vec3::new(0.1, 0.0, 0.0),
            Vec3::new(0.2, 0.2, 0.0),
        ];
        let radii = [
            Vec2::new(0.01, 0.015),
            Vec2::new(0.02, 0.01),
            Vec2::new(0.006, 0.01),
            Vec2::splat(0.004),
        ];
        let sampled = sample(&path, &radii, true);
        for (p, r) in path.into_iter().zip(radii) {
            assert!(sampled.iter().any(|s| s.center == p && s.radius == r));
        }
        assert!(sampled.iter().all(|s| s.center.is_finite()
            && s.radius.is_finite()
            && s.tangent.is_finite()
            && (s.tangent.length() - 1.0).abs() < 1e-5));
        // Independent Hermite evaluation of the authored Catmull-Rom curve.
        for i in 0..3 {
            let a = path[i];
            let b = path[i + 1];
            let da = (b - path[i.saturating_sub(1)]) * 0.5;
            let db = (path[(i + 2).min(3)] - a) * 0.5;
            for k in 0..=1000 {
                let t = k as f32 / 1000.0;
                let p = a * (2.0 * t * t * t - 3.0 * t * t + 1.0)
                    + da * (t * t * t - 2.0 * t * t + t)
                    + b * (-2.0 * t * t * t + 3.0 * t * t)
                    + db * (t * t * t - t * t);
                let distance = sampled
                    .windows(2)
                    .map(|w| segment_distance(p, w[0].center, w[1].center))
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    distance <= ERROR + 1e-6,
                    "reference curve escaped sampled chords: {distance}"
                );
            }
        }
        let middle: Vec<_> = sampled
            .iter()
            .filter(|s| (0.01..0.09).contains(&s.center.x))
            .collect();
        assert!(middle.iter().any(|s| s.center.y > 0.005));
        assert!(middle.iter().any(|s| s.center.y < -0.005));
    }

    #[test]
    fn smooth_small_spans_are_sparse_but_paint_spacing_and_work_are_bounded() {
        let path = [Vec3::ZERO, Vec3::Y * 0.03];
        let sampled = sample(&path, &[Vec2::splat(0.01); 2], true);
        assert!(
            sampled.len() <= 5,
            "straight tiny spans do not need eight subdivisions"
        );
        assert!(sampled
            .windows(2)
            .all(|w| w[0].center.distance(w[1].center) <= PAINT_SPAN + 1e-6));
        let bent = sample(
            &[Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::Z],
            &[Vec2::splat(0.1); 4],
            true,
        );
        assert!(bent.len() <= 3 * (1 << MAX_DEPTH) + 1);
    }

    #[test]
    fn angular_density_tracks_radius_without_exceeding_authored_limits() {
        let small = sides(10, 2, 0.01, true);
        let large = sides(10, 2, 0.10, true);
        assert!(small >= 10 && small < large && large <= 20);
        assert!(0.01 * (1.0 - (PI / small as f32).cos()) <= ERROR);
        assert_eq!(sides(6, 1, 0.1, false), 6);
        assert_eq!(sides(6, 2, 0.1, false), 12);
        assert_eq!(sides(32, usize::MAX, 1e9, true), 32);
    }
    #[test]
    fn authored_low_poly_knots_and_degenerate_paths_remain_finite() {
        assert!(sample(&[], &[], true).is_empty());
        assert_eq!(sample(&[Vec3::ZERO], &[Vec2::ONE], true).len(), 1);
        let coincident = sample(&[Vec3::ZERO; 2], &[Vec2::ONE; 2], true);
        assert!(coincident.iter().all(|s| s.tangent.is_finite()));
        let path = [Vec3::ZERO, Vec3::X, Vec3::Y];
        let radii = [Vec2::ONE, Vec2::splat(2.0), Vec2::splat(0.5)];
        let coarse = sample(&path, &radii, false);
        assert_eq!(coarse.len(), path.len());
        for ((sample, position), radius) in coarse.iter().zip(path).zip(radii) {
            assert_eq!(sample.center, position);
            assert_eq!(sample.radius, radius);
        }
    }

    #[test]
    #[should_panic(expected = "each tube knot needs its radii")]
    fn missing_radius_is_rejected_before_sampling() {
        sample(&[Vec3::ZERO, Vec3::Y], &[Vec2::ONE], true);
    }
}
