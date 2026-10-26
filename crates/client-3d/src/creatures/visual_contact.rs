//! Visual contact profiles derived from composed geometry, in joint-local space.
use super::*;
use bevy::asset::RenderAssetUsages;
use bevy::render::mesh::VertexAttributeValues;

/// The nearer of two signed distances, keeping `best` on a tie. Unlike
/// `f32::min`, whose choice between -0 and +0 is unspecified, the result is
/// the same bits on every build: the hair solver stops on exact fixed points.
fn nearer(best: f32, candidate: f32) -> f32 {
    if candidate < best {
        candidate
    } else {
        best
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Cylinder {
    center: Vec3,
    direction: Vec3,
    half_length: f32,
    radius: f32,
    half_extent: Vec3,
    to_local: Affine3A,
    distance_scale: f32,
}
impl Cylinder {
    // Geometry is immutable under posing. Prepare its invariant values once;
    // only the query-space transform and distance scale change with the joints.
    fn new(a: Vec3, b: Vec3, radius: f32) -> Self {
        let axis = b - a;
        let direction = axis.normalize_or(Vec3::Y);
        let half_length = axis.length() * 0.5;
        Self {
            center: (a + b) * 0.5,
            direction,
            half_length,
            radius,
            half_extent: direction.abs() * half_length + Vec3::splat(radius),
            to_local: Affine3A::IDENTITY,
            distance_scale: 1.0,
        }
    }
    #[cfg(test)]
    pub fn distance(&self, p: Vec3) -> f32 {
        let p = self.to_local.transform_point3(p);
        self.local_distance(p - self.center)
    }
    fn local_distance(&self, offset: Vec3) -> f32 {
        let along = offset.dot(self.direction);
        let radial = (offset - self.direction * along).length();
        let q = Vec2::new(radial - self.radius, along.abs() - self.half_length);
        (q.max(Vec2::ZERO).length() + q.max_element().min(0.0)) * self.distance_scale
    }
    // The enclosing box's largest signed slab distance is a lower bound,
    // including inside the box. Rounding margin keeps near ties on the exact path.
    fn may_improve(&self, offset: Vec3, best: f32) -> bool {
        let lower = (offset.abs() - self.half_extent).max_element();
        let margin = (offset.abs().max_element() + self.half_extent.max_element() + 1.0)
            * (32.0 * f32::EPSILON);
        let bound = (lower - margin) * self.distance_scale;
        bound.partial_cmp(&best) != Some(std::cmp::Ordering::Greater)
    }
    #[cfg(test)]
    fn nearest(&self, p: Vec3, best: f32) -> f32 {
        self.nearest_local(self.to_local.transform_point3(p), best)
    }
    fn nearest_local(&self, p: Vec3, best: f32) -> f32 {
        let offset = p - self.center;
        if self.may_improve(offset, best) {
            nearer(best, self.local_distance(offset))
        } else {
            best
        }
    }
}

pub(super) struct Part {
    /// Local transforms from the torso to the mesh's joint.
    path: Vec<J>,
    cylinders: Vec<Cylinder>,
}
impl Part {
    pub fn from_model(path: Vec<J>, model: &Model, meshes: &mut Assets<Mesh>, scale: Vec3) -> Self {
        let mut triangles = Vec::new();
        for (handle, _) in &model.0 {
            let mesh = meshes.get_mut(handle).expect("composed contact mesh");
            // Redressing may reuse this cached mesh after renderer extraction.
            mesh.asset_usage |= RenderAssetUsages::MAIN_WORLD;
            let Some(VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            let indices: Vec<_> = mesh
                .indices()
                .map(|i| i.iter().collect())
                .unwrap_or_else(|| (0..points.len()).collect());
            for tri in indices.as_chunks::<3>().0 {
                triangles.push(tri.map(|i| Vec3::from_array(points[i]) * scale));
            }
        }
        Self {
            path,
            cylinders: profile(&triangles),
        }
    }
    pub fn posed(
        &self,
        frame: impl Fn(J) -> Option<Affine3A>,
    ) -> impl Iterator<Item = Cylinder> + '_ {
        // Resolve the complete chain before yielding anything. An absent joint
        // suppresses this contributor, not the other parts in the scene.
        let transform = self
            .path
            .iter()
            .try_fold(Affine3A::IDENTITY, |pose, &joint| {
                frame(joint).map(|local| pose * local)
            });
        let to_local =
            transform.map(|pose| pose.inverse() * Affine3A::from_scale(anatomy::HIP_SCALE));
        let distance_scale = 1.0 / anatomy::HIP_SCALE.max_element();
        self.cylinders.iter().filter_map(move |c| {
            Some(Cylinder {
                to_local: to_local?,
                distance_scale,
                ..*c
            })
        })
    }
}

fn profile(triangles: &[[Vec3; 3]]) -> Vec<Cylinder> {
    if triangles.is_empty() {
        return Vec::new();
    }
    let (low, high) = triangles
        .iter()
        .flatten()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.y), hi.max(p.y))
        });
    (0..6)
        .filter_map(|i| {
            let a = low + (high - low) * i as f32 / 6.0;
            let b = low + (high - low) * (i + 1) as f32 / 6.0;
            // Clip crossing faces to the slab before measuring its radial envelope.
            // Counting a distant triangle vertex here inflates shoulder contact.
            let mut points = Vec::new();
            for tri in triangles {
                for &p in tri {
                    if p.y >= a && p.y <= b {
                        points.push(p);
                    }
                }
                for edge in [[0, 1], [1, 2], [2, 0]] {
                    let p = tri[edge[0]];
                    let q = tri[edge[1]];
                    for y in [a, b] {
                        if (q.y - p.y).abs() > 1e-8 {
                            let t = (y - p.y) / (q.y - p.y);
                            if (0.0..=1.0).contains(&t) {
                                points.push(p.lerp(q, t));
                            }
                        }
                    }
                }
            }
            if points.is_empty() {
                return None;
            }
            let lo = points.iter().copied().reduce(Vec3::min)?;
            let hi = points.iter().copied().reduce(Vec3::max)?;
            let center = (lo + hi) * 0.5;
            let radius = points
                .iter()
                .map(|p| Vec2::new(p.x - center.x, p.z - center.z).length())
                .reduce(f32::max)?;
            Some(Cylinder::new(
                Vec3::new(center.x, a - 0.003, center.z),
                Vec3::new(center.x, b + 0.003, center.z),
                radius + 0.003,
            ))
        })
        .collect()
}

pub(super) fn posed(parts: &[Part], frame: impl Fn(J) -> Option<Affine3A>) -> Vec<Cylinder> {
    parts.iter().flat_map(|part| part.posed(&frame)).collect()
}

/// Static composed layers and current posed contributors share one query.
pub(super) struct Scene {
    pub body: crate::sdf::ContactField,
    posed: Vec<Cylinder>,
    // Consecutive cylinders from one posed mesh share query coordinates.
    // Preserve their original reduction order; only the transform is shared.
    groups: Vec<std::ops::Range<usize>>,
}
impl Scene {
    pub fn new(body: crate::sdf::Sculpt, posed: Vec<Cylinder>) -> Self {
        let mut groups = Vec::new();
        group_frames(&posed, &mut groups);
        Self {
            body: crate::sdf::ContactField::new(body),
            posed,
            groups,
        }
    }
    pub fn set_pose(&mut self, posed: Vec<Cylinder>) -> bool {
        if self.posed == posed {
            return false;
        }
        group_frames(&posed, &mut self.groups);
        self.posed = posed;
        true
    }
    pub fn distance(&self, p: Vec3) -> f32 {
        let mut best = self.body.distance(p);
        for group in &self.groups {
            let cylinders = &self.posed[group.clone()];
            let local = cylinders[0].to_local.transform_point3(p);
            for cylinder in cylinders {
                best = cylinder.nearest_local(local, best);
            }
        }
        best
    }
}
// Retain allocation across pose changes. Groups never outlive their posed list.
fn group_frames(cylinders: &[Cylinder], groups: &mut Vec<std::ops::Range<usize>>) {
    groups.clear();
    let mut start = 0;
    for index in 1..cylinders.len() {
        if cylinders[index].to_local != cylinders[start].to_local {
            groups.push(start..index);
            start = index;
        }
    }
    if start < cylinders.len() {
        groups.push(start..cylinders.len());
    }
}

impl std::ops::Deref for Scene {
    type Target = crate::sdf::ContactField;
    fn deref(&self) -> &Self::Target {
        &self.body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grouped_scene_queries_match_exhaustive_contacts_across_pose_changes() {
        let mut body = crate::sdf::Sculpt::default();
        body.add(
            crate::sdf::Shape::Ellipsoid(Vec3::new(-0.7, 0.2, 0.0), Vec3::splat(0.05)),
            0.0,
        );
        let mut scene = Scene::new(body, Vec::new());
        let frames = [
            Affine3A::IDENTITY,
            Affine3A::from_scale_rotation_translation(
                Vec3::new(0.8, 1.2, 0.9),
                Quat::from_rotation_z(0.7),
                Vec3::new(0.2, -0.3, 0.1),
            ),
        ];
        let contacts: Vec<_> = [0, 0, 0, 1, 1, 0]
            .into_iter()
            .enumerate()
            .map(|(i, frame)| {
                let y = i as f32 * 0.08;
                Cylinder {
                    to_local: frames[frame],
                    distance_scale: if i % 2 == 0 { 0.75 } else { 1.25 },
                    ..Cylinder::new(Vec3::Y * y, Vec3::Y * (y + 0.15), 0.12)
                }
            })
            .collect();
        let mut reversed = contacts.clone();
        reversed.reverse();
        for (posed, group_count) in [(contacts, 3), (reversed, 3), (Vec::new(), 0)] {
            assert!(scene.set_pose(posed.clone()));
            assert!(
                !scene.set_pose(posed.clone()),
                "unchanged pose must not invalidate fitting"
            );
            assert_eq!(
                scene.groups.len(),
                group_count,
                "one transform per consecutive shared frame"
            );
            for x in -4..=4 {
                for y in -4..=4 {
                    for z in -4..=4 {
                        let point = Vec3::new(x as f32, y as f32, z as f32) * 0.11;
                        let expected = posed
                            .iter()
                            .fold(scene.body.distance(point), |d, c| nearer(d, c.distance(point)));
                        assert_eq!(
                            scene.distance(point).to_bits(),
                            expected.to_bits(),
                            "point {point:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn nearer_keeps_the_current_best_on_ties_including_signed_zero() {
        assert_eq!(nearer(-0.0, 0.0).to_bits(), (-0.0f32).to_bits());
        assert_eq!(nearer(0.0, -0.0).to_bits(), 0.0f32.to_bits());
        assert_eq!(nearer(0.5, 0.25), 0.25);
        assert_eq!(nearer(0.25, 0.5), 0.25);
        assert_eq!(nearer(f32::INFINITY, -1.0), -1.0);
    }

    #[test]
    fn cylinder_queries_preserve_endpoint_distance_under_pose() {
        // Endpoint formula is the pre-refactor reference. Check signed distances,
        // including degenerate axes, caps, interiors and nonuniform pose scales.
        for (a, b) in [
            (Vec3::ZERO, Vec3::Y),
            (Vec3::new(-0.2, 0.1, 0.3), Vec3::new(0.4, 0.7, -0.1)),
            (Vec3::splat(0.2), Vec3::splat(0.2)),
        ] {
            for to_local in [
                Affine3A::IDENTITY,
                Affine3A::from_scale_rotation_translation(
                    Vec3::new(0.8, 1.2, 0.9),
                    Quat::from_rotation_z(0.7),
                    Vec3::new(0.2, -0.3, 0.1),
                ),
            ] {
                let cylinder = Cylinder {
                    to_local,
                    distance_scale: 0.7,
                    ..Cylinder::new(a, b, 0.12)
                };
                for x in -5..=5 {
                    for y in -5..=5 {
                        for z in -5..=5 {
                            let p = Vec3::new(x as f32, y as f32, z as f32) * 0.13;
                            let local = to_local.transform_point3(p);
                            let axis = b - a;
                            let direction = axis.normalize_or(Vec3::Y);
                            let offset = local - (a + b) * 0.5;
                            let along = offset.dot(direction);
                            let radial = (offset - direction * along).length();
                            let q = Vec2::new(radial - 0.12, along.abs() - axis.length() * 0.5);
                            let expected =
                                (q.max(Vec2::ZERO).length() + q.max_element().min(0.0)) * 0.7;
                            assert_eq!(cylinder.distance(p).to_bits(), expected.to_bits(), "{p:?}");
                            for best in [
                                f32::NEG_INFINITY,
                                -0.3,
                                -0.0,
                                0.0,
                                0.01,
                                0.4,
                                f32::INFINITY,
                                expected,
                                expected - 1e-6,
                                expected + 1e-6,
                            ] {
                                assert_eq!(
                                    cylinder.nearest(p, best).to_bits(),
                                    nearer(best, expected).to_bits(),
                                    "nearest at {p:?}, best {best}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn distant_contact_is_culled_but_boundary_and_interior_are_evaluated() {
        let c = Cylinder::new(Vec3::ZERO, Vec3::Y, 0.1);
        assert!(!c.may_improve(Vec3::splat(10.0), 0.01));
        assert!(!c.may_improve(Vec3::splat(10.0), -0.1));
        assert!(c.may_improve(Vec3::ZERO, -0.1));
        assert!(c.may_improve(Vec3::X * 0.1, 0.0));
        assert!(c.may_improve(Vec3::splat(10.0), f32::INFINITY));
        assert!(c.may_improve(Vec3::splat(f32::INFINITY), 0.0));
        for magnitude in [1e3, 1e6, 1e10] {
            let p = Vec3::splat(magnitude);
            let expected = c.distance(p);
            for best in [0.0, expected, expected * 0.99, expected * 1.01] {
                assert_eq!(c.nearest(p, best).to_bits(), nearer(best, expected).to_bits());
            }
        }
    }

    #[test]
    fn missing_joint_omits_only_its_complete_contact_chain() {
        let cylinder = Cylinder::new(Vec3::ZERO, Vec3::Y, 0.1);
        let parts = [
            Part {
                path: vec![J::ArmL, J::ForeL],
                cylinders: vec![cylinder; 2],
            },
            Part {
                path: vec![J::ArmR],
                cylinders: vec![cylinder],
            },
        ];
        let frame = |j| (j != J::ForeL).then_some(Affine3A::from_translation(Vec3::X));
        let contacts = posed(&parts, frame);
        assert_eq!(
            contacts.len(),
            1,
            "incomplete chain must not emit partial contacts"
        );
        assert_eq!(contacts, parts[1].posed(frame).collect::<Vec<_>>());
        assert_eq!(posed(&parts, |_| None), Vec::<Cylinder>::new());
        let all = posed(&parts, |_| Some(Affine3A::IDENTITY));
        assert_eq!(all.len(), 3);
        assert_eq!(all, posed(&parts, |_| Some(Affine3A::IDENTITY)));
    }

    #[test]
    fn profile_covers_long_faces_between_sparse_vertices() {
        let triangles = [[
            Vec3::new(-0.1, 0.1, 0.08),
            Vec3::new(0.15, -0.5, 0.05),
            Vec3::new(-0.1, -0.5, -0.1),
        ]];
        let caps = profile(&triangles);
        for i in 0..=20 {
            for j in 0..=20 - i {
                let a = i as f32 / 20.0;
                let b = j as f32 / 20.0;
                let p = triangles[0][0] * a + triangles[0][1] * b + triangles[0][2] * (1.0 - a - b);
                assert!(
                    caps.iter().any(|c| c.distance(p) <= 0.0),
                    "uncovered face at {p:?}"
                );
            }
        }
        let part = Part {
            path: vec![J::ArmL, J::ForeL],
            cylinders: caps,
        };
        let upper = Affine3A::from_rotation_z(0.7);
        let lower = Affine3A::from_translation(Vec3::Y * -0.3);
        let posed: Vec<_> = part
            .posed(|j| Some(if j == J::ArmL { upper } else { lower }))
            .collect();
        for tri in triangles {
            for p in tri {
                let p = (upper * lower).transform_point3(p) / anatomy::HIP_SCALE;
                assert!(posed.iter().any(|c| c.distance(p) <= 0.0));
            }
        }
    }
}
