//! Conforming local refinement: neighbouring faces share every split edge.
use bevy::prelude::Vec3;
use std::collections::{BTreeMap, HashMap};

const MAX_ROUNDS: usize = 4;

/// Projection can make a grid quad concave. Keep its boundary, but replace an
/// inward diagonal when the other diagonal gives two outward faces.
pub(super) fn repair_quad_diagonals(
    points: &[Vec3],
    faces: &mut [[usize; 3]],
    normal: impl Fn(Vec3) -> Vec3,
) {
    let outward = |face: [usize; 3]| {
        let [a, b, c] = face.map(|i| points[i]);
        (b - a).cross(c - a).dot(normal((a + b + c) / 3.0)) > 0.0
    };
    for pair in faces.as_chunks_mut::<2>().0 {
        if pair.iter().copied().all(&outward) {
            continue;
        }
        let [a, b, c] = pair[0];
        let d = pair[1][2];
        let alternative = [[a, b, d], [b, c, d]];
        if alternative.into_iter().all(&outward) {
            pair.copy_from_slice(&alternative);
        }
    }
}

/// Returns false when the bounded refinement budget cannot meet the tolerance.
pub(super) fn refine(
    points: &mut Vec<Vec3>,
    faces: &mut Vec<[usize; 3]>,
    tolerance: f32,
    error: impl Fn(Vec3) -> f32,
    project: impl Fn(Vec3) -> Vec3,
) -> bool {
    let bad = |p| {
        let value = error(p);
        !value.is_finite() || value > tolerance
    };
    if let Some(point) = points.iter().copied().find(|&p| bad(p)) {
        if crate::props::profile_builds() {
            eprintln!(
                "3d_sdf_refinement_failed reason=initial_vertex at={point:?} error={}",
                error(point)
            );
        }
        return false;
    }
    // Unchanged edges have unchanged positions and field error across rounds.
    let mut checked = HashMap::new();
    // Inspect the result of the final permitted subdivision as well. The last
    // inspection cannot add points or faces beyond the subdivision budget.
    for round in 0..=MAX_ROUNDS {
        let mut splits = BTreeMap::new();
        for &[a, b, c] in faces.iter() {
            let center = (points[a] + points[b] + points[c]) / 3.0;
            let center_error = bad(center);
            for (x, y) in [(a, b), (b, c), (c, a)] {
                let key = (x.min(y), x.max(y));
                if center_error
                    || *checked.entry(key).or_insert_with(|| {
                        [0.25, 0.5, 0.75]
                            .into_iter()
                            .any(|t| bad(points[x].lerp(points[y], t)))
                    })
                {
                    splits.entry(key).or_insert(0);
                }
            }
        }
        if splits.is_empty() {
            return true;
        }
        if round == MAX_ROUNDS {
            if crate::props::profile_builds() {
                let worst = faces
                    .iter()
                    .map(|face| {
                        let [a, b, c] = face.map(|i| points[i]);
                        let center = (a + b + c) / 3.0;
                        (error(center), center, [a, b, c])
                    })
                    .max_by(|a, b| a.0.total_cmp(&b.0));
                let edge = splits.keys().next().map(|&(a, b)| {
                    (
                        points[a],
                        points[b],
                        [0.25, 0.5, 0.75].map(|t| error(points[a].lerp(points[b], t))),
                    )
                });
                eprintln!("3d_sdf_refinement_unresolved centroid={worst:?} edge={edge:?}");
            }
            break;
        }
        for (&(a, b), index) in &mut splits {
            *index = points.len();
            let point = project((points[a] + points[b]) * 0.5);
            if !point.is_finite() || bad(point) {
                if crate::props::profile_builds() {
                    eprintln!(
                        "3d_sdf_refinement_failed reason=edge_projection at={point:?} error={}",
                        error(point)
                    );
                }
                return false;
            }
            points.push(point);
        }
        let mut next = Vec::with_capacity(faces.len() * 2);
        for &face in faces.iter() {
            let mids = std::array::from_fn::<_, 3, _>(|i| {
                let (a, b) = (face[i], face[(i + 1) % 3]);
                splits.get(&(a.min(b), a.max(b))).copied()
            });
            match mids.iter().flatten().count() {
                0 => next.push(face),
                1 => {
                    let i = mids.iter().position(Option::is_some).unwrap();
                    let [a, b, c] = std::array::from_fn(|j| face[(i + j) % 3]);
                    let m = mids[i].unwrap();
                    next.extend([[a, m, c], [m, b, c]]);
                }
                2 => {
                    let i = (0..3)
                        .find(|&i| mids[i].is_some() && mids[(i + 1) % 3].is_some())
                        .unwrap();
                    let [a, b, c] = std::array::from_fn(|j| face[(i + j) % 3]);
                    let (ab, bc) = (mids[i].unwrap(), mids[(i + 1) % 3].unwrap());
                    let current = [[a, ab, c], [ab, bc, c]];
                    let alternate = [[a, ab, bc], [a, bc, c]];
                    let parent_normal = (points[b] - points[a]).cross(points[c] - points[a]);
                    let outward = |faces: [[usize; 3]; 2]| {
                        faces.into_iter().all(|face| {
                            let [x, y, z] = face.map(|index| points[index]);
                            (y - x).cross(z - x).dot(parent_normal) > 0.0
                        })
                    };
                    let shorter = points[a].distance_squared(points[bc])
                        < points[ab].distance_squared(points[c]);
                    next.push([ab, b, bc]);
                    next.extend(if outward(alternate) && (!outward(current) || shorter) {
                        alternate
                    } else {
                        current
                    });
                }
                3 => {
                    let [a, b, c] = face;
                    let [ab, bc, ca] = mids.map(Option::unwrap);
                    next.extend([[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]]);
                }
                _ => unreachable!(),
            }
        }
        *faces = next;
    }
    if crate::props::profile_builds() {
        eprintln!(
            "3d_sdf_refinement_failed reason=round_limit vertices={} faces={}",
            points.len(),
            faces.len()
        );
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_allowed_refinement_round_can_complete_the_surface() {
        // An octant of a unit sphere needs four rounds for this tolerance.
        // The oracle is analytic and independent of the sculpt implementation.
        let mut points = vec![Vec3::X, Vec3::Y, Vec3::Z];
        let mut faces = vec![[0, 1, 2]];
        let tolerance = 0.006;
        assert!(refine(
            &mut points,
            &mut faces,
            tolerance,
            |p| (p.length() - 1.0).abs(),
            |p| p.normalize(),
        ));
        for &[a, b, c] in &faces {
            for p in [
                (points[a] + points[b] + points[c]) / 3.0,
                (points[a] + points[b]) * 0.5,
                (points[b] + points[c]) * 0.5,
                (points[c] + points[a]) * 0.5,
            ] {
                assert!((p.length() - 1.0).abs() <= tolerance);
            }
        }
        assert!(faces.len() <= 256, "at most four subdivision rounds");
    }

    #[test]
    fn two_split_edges_use_the_shorter_outward_diagonal() {
        let mut points = vec![Vec3::ZERO, Vec3::X * 2.0, Vec3::Y];
        let mut faces = vec![[0, 1, 2]];
        let edge_centers = [Vec3::X, Vec3::new(1.0, 0.5, 0.0)];
        assert!(refine(
            &mut points,
            &mut faces,
            0.01,
            |p| if edge_centers
                .iter()
                .any(|&center| p.distance(center) < 0.001)
            {
                1.0
            } else {
                0.0
            },
            |p| p + Vec3::Z * 0.1,
        ));
        assert_eq!(faces.len(), 3);
        let bc = points
            .iter()
            .position(|&p| p == Vec3::new(1.0, 0.5, 0.1))
            .unwrap();
        let ab = points
            .iter()
            .position(|&p| p == Vec3::new(1.0, 0.0, 0.1))
            .unwrap();
        assert!(faces
            .iter()
            .any(|face| face.contains(&0) && face.contains(&bc)));
        assert!(!faces
            .iter()
            .any(|face| face.contains(&ab) && face.contains(&2)));
        for &face in &faces {
            let [a, b, c] = face.map(|i| points[i]);
            assert!((b - a).cross(c - a).z > 0.0);
        }
    }

    #[test]
    fn projected_concave_quads_keep_the_boundary_and_use_an_outward_diagonal() {
        let points = [Vec3::ZERO, Vec3::new(0.2, 0.2, 0.0), Vec3::X, Vec3::Y];
        let mut faces = [[0, 1, 2], [0, 2, 3]];
        let boundary = |faces: &[[usize; 3]]| {
            let mut edges = BTreeMap::new();
            for &[a, b, c] in faces {
                for (x, y) in [(a, b), (b, c), (c, a)] {
                    *edges.entry((x.min(y), x.max(y))).or_insert(0) += 1;
                }
            }
            edges
                .into_iter()
                .filter(|(_, count)| *count == 1)
                .map(|(edge, _)| edge)
                .collect::<Vec<_>>()
        };
        let before = boundary(&faces);
        repair_quad_diagonals(&points, &mut faces, |_| Vec3::Z);
        assert_eq!(faces, [[0, 1, 3], [1, 2, 3]]);
        assert_eq!(boundary(&faces), before);
        let mut square = [[0, 1, 2], [0, 2, 3]];
        repair_quad_diagonals(
            &[Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Y, Vec3::Y],
            &mut square,
            |_| Vec3::Z,
        );
        assert_eq!(
            square,
            [[0, 1, 2], [0, 2, 3]],
            "ordinary outward quads stay unchanged"
        );
        let mut collapsed = [[0, 1, 2], [0, 2, 3]];
        repair_quad_diagonals(
            &[Vec3::ZERO, Vec3::ZERO, Vec3::X, Vec3::Y],
            &mut collapsed,
            |_| Vec3::Z,
        );
        assert_eq!(
            collapsed,
            [[0, 1, 2], [0, 2, 3]],
            "invalid alternatives cannot bypass the fallback"
        );
    }

    #[test]
    fn local_face_refinement_splits_its_neighbours_shared_edge() {
        let mut points = vec![Vec3::ZERO, Vec3::X, Vec3::Y, Vec3::X + Vec3::Y];
        let mut faces = vec![[0, 1, 2], [1, 3, 2]];
        let center = (Vec3::X + Vec3::Y) / 3.0;
        assert!(refine(
            &mut points,
            &mut faces,
            0.1,
            |p| if p.distance(center) < 0.001 { 1.0 } else { 0.0 },
            |p| p + Vec3::Z
        ));
        assert_eq!(faces.len(), 6, "four refined faces and two neighbour faces");
        let shared = points
            .iter()
            .position(|&p| p == (Vec3::X + Vec3::Y) * 0.5 + Vec3::Z)
            .unwrap();
        assert_eq!(points.iter().filter(|&&p| p == points[shared]).count(), 1);
        let mut edges = BTreeMap::new();
        for &[a, b, c] in &faces {
            assert!((points[b] - points[a]).cross(points[c] - points[a]).z > 0.0);
            for (x, y) in [(a, b), (b, c), (c, a)] {
                *edges.entry((x.min(y), x.max(y))).or_insert(0) += 1;
            }
        }
        assert_eq!(edges.get(&(1.min(shared), 1.max(shared))), Some(&2));
        assert_eq!(edges.get(&(2.min(shared), 2.max(shared))), Some(&2));
        assert!(
            !edges.contains_key(&(1, 2)),
            "no unsplit diagonal may remain"
        );
    }

    #[test]
    fn flat_faces_are_unchanged_and_pathological_work_is_bounded() {
        let mut points = vec![Vec3::ZERO, Vec3::X, Vec3::Y];
        let mut faces = vec![[0, 1, 2]];
        assert!(refine(&mut points, &mut faces, 0.001, |_| 0.0, |p| p));
        assert_eq!((points.len(), faces.len()), (3, 1));
        // Every inserted edge point is dyadic and valid, but the unresolved
        // face centroids keep requesting refinement. Exercise the actual cap.
        let unresolved_center = |p: Vec3| {
            let x = (p.x * 4096.0).fract();
            let y = (p.y * 4096.0).fract();
            if x.min(1.0 - x) > 0.01 || y.min(1.0 - y) > 0.01 {
                1.0
            } else {
                0.0
            }
        };
        assert!(!refine(
            &mut points,
            &mut faces,
            0.001,
            unresolved_center,
            |p| p
        ));
        assert_eq!(faces.len(), 4usize.pow(MAX_ROUNDS as u32));
    }

    #[test]
    fn invalid_starting_or_projected_points_reject_instead_of_claiming_convergence() {
        let mut points = vec![Vec3::ZERO, Vec3::X, Vec3::Y];
        let mut faces = vec![[0, 1, 2]];
        assert!(!refine(&mut points, &mut faces, 0.001, |_| f32::NAN, |p| p));
        let center = (Vec3::X + Vec3::Y) / 3.0;
        assert!(!refine(
            &mut points,
            &mut faces,
            0.1,
            |p| if p.distance(center) < 0.001 { 1.0 } else { 0.0 },
            |_| Vec3::splat(f32::NAN)
        ));
    }
}
