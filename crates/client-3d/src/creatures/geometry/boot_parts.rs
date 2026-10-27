//! Constructed leather uppers, layered welts and separate tapered heels.
//! The last and the garment contact envelope resolve from the same footwear choice.
use super::*;

fn loft<const N: usize>(rings: &[[Vec3; N]]) -> Vec<[Vec3; 3]> {
    let mut faces = Vec::new();
    for pair in rings.windows(2) {
        for j in 0..N {
            let k = (j + 1) % N;
            faces.push([pair[0][j], pair[1][k], pair[1][j]]);
            faces.push([pair[0][j], pair[0][k], pair[1][k]]);
        }
    }
    for (ring, forward) in [(&rings[0], false), (&rings[rings.len() - 1], true)] {
        let center = ring.iter().copied().sum::<Vec3>() / N as f32;
        for j in 0..N {
            let k = (j + 1) % N;
            faces.push(if forward {
                [center, ring[j], ring[k]]
            } else {
                [center, ring[k], ring[j]]
            });
        }
    }
    faces
}

fn components(last: BootLast) -> [Vec<[Vec3; 3]>; 3] {
    let sections = last.sections();
    // Counterclockwise D-shaped sections, with a flat buried base and rounded vamp.
    const UPPER: [(f32, f32); 9] = [
        (-1.0, 0.0),
        (1.0, 0.0),
        (1.0, 0.18),
        (0.78, 0.70),
        (0.35, 0.96),
        (0.0, 1.0),
        (-0.35, 0.96),
        (-0.78, 0.70),
        (-1.0, 0.18),
    ];
    let upper = sections.map(|s| {
        UPPER.map(|(x, y)| Vec3::new(x * s.width * 0.94, s.top - 0.003 + y * s.height, s.z))
    });
    // A bevel distinguishes the welt edge from the flat supporting outsole.
    let sole = sections.map(|s| {
        [
            Vec3::new(-s.width, s.top, s.z),
            Vec3::new(-s.width, s.bottom + 0.004, s.z),
            Vec3::new(-s.width * 0.82, s.bottom, s.z),
            Vec3::new(s.width * 0.82, s.bottom, s.z),
            Vec3::new(s.width, s.bottom + 0.004, s.z),
            Vec3::new(s.width, s.top, s.z),
        ]
    });
    let heel = if last.heeled {
        let b = last.build;
        const CORNERS: [(f32, f32); 8] = [
            (-0.7, -1.0),
            (0.7, -1.0),
            (1.0, -0.7),
            (1.0, 0.7),
            (0.7, 1.0),
            (-0.7, 1.0),
            (-1.0, 0.7),
            (-1.0, -0.7),
        ];
        // Loft toward +Y: use XZ rings with reversed winding for an upward cap.
        let rings = [
            (BootLast::FLOOR, 0.022 * b, 0.020 * b),
            (-0.036, 0.034 * b, 0.026 * b),
        ]
        .map(|(y, w, d)| CORNERS.map(|(x, z)| Vec3::new(x * w, y, -0.035 * b - z * d)));
        loft(&rings)
    } else {
        Vec::new()
    };
    [loft(&upper), loft(&sole), heel]
}

/// A shoe on the same last: the upper closed in its colour, or for a sandal
/// the bare foot, on a sole and heel of the shoe's.
pub(super) fn emit_shoe(p: &mut Parts, last: BootLast, upper: Vec3, sole: Vec3) {
    let [foot, base, heel] = components(last);
    for face in foot {
        p.satin.tri(face, upper);
    }
    for face in base {
        p.matte.tri(face, sole * 0.6);
    }
    for face in heel {
        p.matte.tri(face, sole * 0.8);
    }
}

pub(super) fn emit(p: &mut Parts, last: BootLast, color: Vec3) {
    let [upper, sole, heel] = components(last);
    for face in upper {
        p.satin.tri(face, color);
    }
    for face in sole {
        let n = (face[1] - face[0])
            .cross(face[2] - face[0])
            .normalize_or_zero();
        p.matte
            .tri(face, color * if n.y < -0.5 { 0.32 } else { 0.56 });
    }
    for face in heel {
        p.matte.tri(face, color * 0.40);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn constructed_boot_components_are_closed_outward_volumes() {
        for heeled in [false, true] {
            for build in [0.8, 1.0, 1.4] {
                for faces in components(BootLast { build, heeled }) {
                    if faces.is_empty() {
                        continue;
                    }
                    let mut edges = BTreeMap::new();
                    let mut volume = 0.0;
                    for p in faces {
                        assert!(p.iter().all(|v| v.is_finite()));
                        assert!((p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-18);
                        volume += p[0].dot(p[1].cross(p[2])) / 6.0;
                        let keys =
                            p.map(|v| v.to_array().map(|x| (x * 1_000_000.0).round() as i32));
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
                        "closed material layer"
                    );
                    assert!(volume > 0.00001, "positive material volume {volume}");
                }
            }
        }
    }

    #[test]
    fn constructed_boot_surfaces_fit_the_same_contact_envelope_used_by_cloth() {
        for heeled in [false, true] {
            for build in [0.8, 1.0, 1.4] {
                let last = BootLast { build, heeled };
                let shapes = last.contacts();
                for faces in components(last) {
                    for p in faces {
                        assert!(shapes.iter().any(|(center,radii)|p.iter().all(|v|((*v-*center)/ *radii).length_squared()<=1.00001)),"one convex contact ellipsoid must contain each complete triangle: heeled {heeled} build {build} triangle {p:?}");
                        assert!(
                            p.iter().all(|v| v.y >= BootLast::FLOOR - 1e-6),
                            "nothing extends below the supporting sole"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn constructed_boot_heel_and_forefoot_share_the_support_plane() {
        for build in [0.8, 1.0, 1.4] {
            let [_, sole, heel] = components(BootLast {
                build,
                heeled: true,
            });
            let area = |faces: Vec<[Vec3; 3]>| {
                faces
                    .into_iter()
                    .filter(|p| p.iter().all(|v| (v.y - BootLast::FLOOR).abs() < 1e-6))
                    .map(|p| (p[1] - p[0]).cross(p[2] - p[0]).length() * 0.5)
                    .sum::<f32>()
            };
            assert!(
                area(heel) > 0.001 * build * build,
                "heel tip supports weight"
            );
            assert!(
                area(sole) > 0.003 * build * build,
                "forefoot has a substantial contact patch"
            );
        }
    }
}
