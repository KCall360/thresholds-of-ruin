//! Closed hooked mandibles shared by raptor recipes. Dimensions are anatomical.
use super::*;

pub(super) struct HookedBeak {
    pub root: Vec3,
    pub length: f32,
    pub half_width: f32,
    pub depth: f32,
    pub hook: f32,
}

impl HookedBeak {
    fn mandibles(&self) -> [Vec<[Vec3; 3]>; 2] {
        let upper = [
            (0.00, 1.00, 0.82, 0.00),
            (0.30, 0.96, 1.00, 0.00),
            (0.60, 0.70, 0.77, 0.08),
            (0.82, 0.38, 0.43, 0.38),
            (1.00, 0.025, 0.035, 1.00),
        ]
        .map(|(z, w, d, h)| {
            let w = self.half_width * w;
            let d = self.depth * d;
            let center = self.root + Vec3::new(0.0, -self.hook * h, self.length * z);
            [
                Vec3::new(-w, d * 0.25, 0.0),
                Vec3::ZERO,
                Vec3::new(w, d * 0.25, 0.0),
                Vec3::new(w * 0.65, d * 0.8, 0.0),
                Vec3::new(0.0, d, 0.0),
                Vec3::new(-w * 0.65, d * 0.8, 0.0),
            ]
            .map(|p| center + p)
        });
        let lower = [
            (0.00, 0.85, 0.40, 0.00),
            (0.32, 0.80, 0.42, 0.03),
            (0.58, 0.49, 0.27, 0.16),
            (0.75, 0.035, 0.045, 0.32),
        ]
        .map(|(z, w, d, h)| {
            let w = self.half_width * w;
            let d = self.depth * d;
            let center =
                self.root + Vec3::new(0.0, -self.depth * 0.13 - self.hook * h, self.length * z);
            [
                Vec3::new(-w, -d * 0.12, 0.0),
                Vec3::new(-w * 0.65, -d * 0.7, 0.0),
                Vec3::new(0.0, -d, 0.0),
                Vec3::new(w * 0.65, -d * 0.7, 0.0),
                Vec3::new(w, -d * 0.12, 0.0),
                Vec3::ZERO,
            ]
            .map(|p| center + p)
        });
        [closed_sections(&upper), closed_sections(&lower)]
    }

    pub(super) fn emit(&self, b: &mut Builder, upper: Vec3, lower: Vec3) {
        for (faces, color) in self.mandibles().into_iter().zip([upper, lower]) {
            for face in faces {
                b.tri(face, color);
            }
        }
    }
}

/// Counterclockwise sections in XY progress toward +Z. Each mandible closes independently.
fn closed_sections(rings: &[[Vec3; 6]]) -> Vec<[Vec3; 3]> {
    let mut faces = Vec::new();
    for pair in rings.windows(2) {
        for j in 0..6 {
            let k = (j + 1) % 6;
            faces.push([pair[0][j], pair[1][k], pair[1][j]]);
            faces.push([pair[0][j], pair[0][k], pair[1][k]]);
        }
    }
    for (ring, forward) in [(&rings[0], false), (&rings[rings.len() - 1], true)] {
        let center = ring.iter().copied().sum::<Vec3>() / 6.0;
        for j in 0..6 {
            let k = (j + 1) % 6;
            faces.push(if forward {
                [center, ring[j], ring[k]]
            } else {
                [center, ring[k], ring[j]]
            });
        }
    }
    faces
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn recipes() -> [HookedBeak; 2] {
        [
            HookedBeak {
                root: Vec3::ZERO,
                length: 0.175,
                half_width: 0.068,
                depth: 0.085,
                hook: 0.065,
            },
            HookedBeak {
                root: Vec3::ZERO,
                length: 0.085,
                half_width: 0.032,
                depth: 0.046,
                hook: 0.038,
            },
        ]
    }

    #[test]
    fn raptor_beak_mandibles_are_closed_outward_volumes() {
        for beak in recipes() {
            for faces in beak.mandibles() {
                let mut edges = BTreeMap::new();
                let mut volume = 0.0;
                for p in faces {
                    assert!(p.iter().all(|v| v.is_finite()));
                    assert!((p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-18);
                    volume += p[0].dot(p[1].cross(p[2])) / 6.0;
                    let keys = p.map(|v| v.to_array().map(|x| (x * 1_000_000.0).round() as i32));
                    for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                        let (key, sign) = if keys[a] < keys[b] {
                            ((keys[a], keys[b]), 1)
                        } else {
                            ((keys[b], keys[a]), -1)
                        };
                        let entry = edges.entry(key).or_insert((0, 0));
                        entry.0 += 1;
                        entry.1 += sign;
                    }
                }
                assert!(
                    edges.values().all(|v| *v == (2, 0)),
                    "closed consistently wound shell"
                );
                assert!(volume > 0.000001, "positive solid volume {volume}");
            }
        }
    }

    #[test]
    fn raptor_beak_has_a_broad_ridge_descending_hook_and_short_lower_jaw() {
        for beak in recipes() {
            let [upper, lower] = beak.mandibles();
            let tip = upper
                .iter()
                .flatten()
                .filter(|p| p.z > beak.length * 0.99)
                .map(|p| p.y)
                .fold(f32::NEG_INFINITY, f32::max);
            assert!(tip < -beak.hook * 0.9, "hook descends below attachment");
            assert!(
                lower.iter().flatten().all(|p| p.z < beak.length * 0.8),
                "lower mandible ends before hook"
            );
            let breadth = upper
                .iter()
                .flatten()
                .filter(|p| p.z < beak.length * 0.4)
                .map(|p| p.x.abs())
                .fold(0.0, f32::max);
            assert!(breadth > beak.length * 0.3, "broad load-bearing beak base");
        }
    }
}
