//! Loose hair shares cached source geometry, but each wearer owns contact deformation.
//! Coordinates are explicit: hair mesh -> posed head/drape -> torso -> support.
use super::*;
mod chain;
use super::deformation::{ControlCurve, SupportSide, SurfaceBinding};
use bevy::asset::RenderAssetUsages;
use bevy::render::{mesh::VertexAttributeValues, primitives::Aabb};

pub(super) struct HairRig {
    pub entity: Entity,
    meshes: Vec<ContactMesh>,
    support: super::visual_contact::Scene,
    scale: Vec3,
    controls: Vec<ControlCurve>,
    last_frame: Option<Affine3A>,
    settled: bool,
    rest_frame: Option<Affine3A>,
    fitted: Vec<Vec<Vec3>>,
    fitted_rest: Vec<Vec<Vec3>>,
}
impl HairRig {
    pub(super) fn reset_presentation(&mut self) {
        self.last_frame = None;
        self.settled = false;
        self.rest_frame = None;
        self.fitted.clear();
        self.fitted_rest.clear();
    }

    pub(super) fn set_contacts(&mut self, contacts: Vec<super::visual_contact::Cylinder>) {
        if self.support.set_pose(contacts) {
            self.last_frame = None;
        }
    }
}

// Attachment ownership comes from the same authored controls as deformation.
// Smooth the interpolated free weight without consulting wearer position/height.
fn attachment_influence(authored: f32) -> f32 {
    let t = authored.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn skin_attachment(rest: Vec3, deformed: Vec3, influence: f32) -> Vec3 {
    rest.lerp(deformed, influence)
}

#[cfg(test)]
mod attachment_tests {
    use super::*;
    #[test]
    fn authored_attachment_ownership_does_not_change_with_body_height() {
        for height in [0.4, 0.7, 1.2] {
            let curve = ControlCurve {
                points: (0..4)
                    .map(|i| (Vec3::new(0.1, height - i as f32 * 0.1, 0.0), 0.02))
                    .collect(),
                attached: vec![true, true, false, false],
                support_side: None,
            };
            assert_eq!(
                attachment_influence(curve.mobility(0, 0.5)),
                0.0,
                "an authored attachment became free at height {height}"
            );
            assert_eq!(
                attachment_influence(curve.mobility(2, 0.5)),
                1.0,
                "authored free hair became pinned at height {height}"
            );
        }
    }
    #[test]
    fn neighboring_surface_points_stay_continuous_at_scalp_boundary() {
        let upper = Vec3::new(0.1, 0.7, 0.1);
        let end = upper - Vec3::Y * 0.1;
        let t = 0.00002;
        let lower = upper.lerp(end, t);
        let curve = ControlCurve {
            points: vec![(upper, 0.02), (end, 0.02)],
            attached: vec![true, false],
            support_side: None,
        };
        let weight = |t| SurfaceBinding::single(t).evaluate_scalar(|&t| curve.mobility(0, t));
        let displacement = Vec3::new(0.12, -0.03, 0.08);
        let a = skin_attachment(
            upper,
            upper + displacement,
            attachment_influence(weight(0.0)),
        );
        let b = skin_attachment(lower, lower + displacement, attachment_influence(weight(t)));
        assert_eq!(a, upper, "scalp must remain exact");
        assert!(
            a.distance(b) < 0.00001,
            "attachment tears by {}",
            a.distance(b)
        );
    }
    #[test]
    fn attachment_retains_exact_scalp_and_free_fall() {
        assert_eq!(attachment_influence(0.0), 0.0);
        assert_eq!(attachment_influence(1.0), 1.0);
        let rest = Vec3::ONE;
        let target = Vec3::ZERO;
        assert_eq!(skin_attachment(rest, target, 0.0), rest);
        assert_eq!(skin_attachment(rest, target, 1.0), target);
    }
}

struct ContactMesh {
    #[cfg(test)]
    source: Handle<Mesh>,
    handle: Handle<Mesh>,
    deforming: bool,
    rest: Vec<Vec3>,
    bindings: Vec<SurfaceBinding<(usize, usize, f32)>>,
    triangles: Vec<[usize; 3]>,
    edges: Vec<[usize; 2]>,
    influence: Vec<f32>,
}
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    at: Transform,
    scale: Vec3,
    models: &[Model],
    support: crate::sdf::Sculpt,
    head: Affine3A,
    curves: &[ControlCurve],
    contacts: Vec<super::visual_contact::Cylinder>,
) -> HairRig {
    let entity = commands.spawn((at, Visibility::default())).id();
    commands.entity(parent).add_child(entity);
    for curve in curves {
        assert!(curve.points.len() >= 2);
        assert_eq!(curve.points.len(), curve.attached.len());
    }
    let mut hair = HairRig {
        entity,
        meshes: Vec::new(),
        support: super::visual_contact::Scene::new(support, contacts),
        scale,
        controls: curves.to_vec(),
        last_frame: None,
        settled: false,
        rest_frame: None,
        fitted: Vec::new(),
        fitted_rest: Vec::new(),
    };
    let frame = head * at.compute_affine();
    assert_eq!(
        models.len(),
        curves.len(),
        "each rendered lock owns one control curve"
    );
    for (curve, source, material) in models.iter().enumerate().flat_map(|(curve, model)| {
        model
            .0
            .iter()
            .map(move |(source, material)| (curve, source, material))
    }) {
        // Never mutate the library mesh. Only deforming components need private
        // storage; fully attached components can stay shared under their joint.
        let mesh = {
            let source_mesh = meshes.get_mut(source).expect("cached hair mesh");
            // Retain source coordinates for later composition and fitting.
            source_mesh.asset_usage |= RenderAssetUsages::MAIN_WORLD;
            source_mesh
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            continue;
        };
        let rest: Vec<Vec3> = points.iter().map(|p| Vec3::from_array(*p)).collect();
        let bindings: Vec<_> = rest
            .iter()
            .map(|p| {
                // Ownership is authored by the component, never selected among
                // neighboring locks by proximity. Segment coordinates are bound
                // once in the immutable source frame.
                let (segment, t, _) = curves[curve]
                    .points
                    .windows(2)
                    .enumerate()
                    .map(|(segment, pair)| {
                        let a = pair[0].0;
                        let b = pair[1].0;
                        let t = ((*p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                        (segment, t, p.distance_squared(a.lerp(b, t)))
                    })
                    .min_by(|a, b| a.2.total_cmp(&b.2))
                    .expect("lock segments");
                SurfaceBinding::single((curve, segment, t))
            })
            .collect();
        // Keep scalp/crown attachment vertices fixed; contact belongs to the fall.
        let influence: Vec<f32> = bindings
            .iter()
            .map(|binding| {
                attachment_influence(
                    binding
                        .evaluate_scalar(|&(curve, segment, t)| curves[curve].mobility(segment, t)),
                )
            })
            .collect();
        let indices: Vec<_> = mesh.indices().expect("hair topology").iter().collect();
        let triangles = indices.as_chunks::<3>().0.to_vec();
        let edges = triangles
            .iter()
            .flat_map(|t| [[t[0], t[1]], [t[1], t[2]], [t[2], t[0]]])
            .map(|mut e| {
                e.sort_unstable();
                e
            })
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let deforming = influence.iter().any(|&weight| weight > 0.0);
        let handle = if deforming {
            let private = mesh.clone();
            meshes.add(private)
        } else {
            source.clone()
        };
        let child = commands
            .spawn((
                Mesh3d(handle.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_scale(scale),
                Visibility::default(),
                Aabb {
                    center: bevy::math::Vec3A::ZERO,
                    half_extents: bevy::math::Vec3A::splat(1.5),
                },
            ))
            .id();
        commands.entity(entity).add_child(child);
        hair.meshes.push(ContactMesh {
            #[cfg(test)]
            source: source.clone(),
            handle,
            deforming,
            rest,
            bindings,
            triangles,
            edges,
            influence,
        });
    }
    place(&mut hair, frame, 0.0, meshes);
    hair
}

pub(super) fn place(hair: &mut HairRig, frame: Affine3A, dt: f32, meshes: &mut Assets<Mesh>) {
    place_profiled(hair, frame, dt, meshes, None);
}

fn record_stage(
    profile: &mut Option<&mut [std::time::Duration; 4]>,
    stage: usize,
    started: Option<std::time::Instant>,
) {
    if let (Some(profile), Some(started)) = (profile.as_deref_mut(), started) {
        profile[stage] += started.elapsed();
    }
}

/// Optional latest-step CPU attribution: controls, deformation, constraints,
/// and mesh writes/normals. Ordinary placement does not read the clock.
pub(super) fn place_profiled(
    hair: &mut HairRig,
    frame: Affine3A,
    dt: f32,
    meshes: &mut Assets<Mesh>,
    mut profile: Option<&mut [std::time::Duration; 4]>,
) {
    assert!(dt.is_finite() && dt >= 0.0);
    // Construction fits once even at zero time. Existing presentation state
    // must hold while capture readback or an explicit pause stops the clock.
    if (dt == 0.0 && hair.rest_frame.is_some()) || (hair.last_frame == Some(frame) && hair.settled)
    {
        return;
    }
    let started = profile.as_ref().map(|_| std::time::Instant::now());
    let inverse = frame.inverse();
    let controls: Vec<Vec<Vec3>> = hair
        .controls
        .iter()
        .map(|curve| {
            curve
                .points
                .iter()
                .map(|(p, _)| frame.transform_point3(*p * hair.scale) / anatomy::HIP_SCALE)
                .collect()
        })
        .collect();
    let fitted: Vec<_> = hair
        .controls
        .iter()
        .zip(&controls)
        .enumerate()
        .map(|(index, (source, targets))| {
            let pinned = &source.attached;
            let radii: Vec<_> = source
                .points
                .iter()
                .map(|(_, r)| {
                    r * hair.scale.max_element() / anatomy::HIP_SCALE.min_element() + 0.020
                })
                .collect();
            if let (Some(previous), Some(points), Some(rest)) = (
                hair.rest_frame,
                hair.fitted.get(index),
                hair.fitted_rest.get(index),
            ) {
                // Rest and history live in torso support space. Only socket
                // targets follow head rotation; free history is never carried
                // through a rigid head transform before the contact solve.
                let movement = frame * previous.inverse();
                let posed: Vec<_> = rest
                    .iter()
                    .map(|p| {
                        movement.transform_point3(*p * anatomy::HIP_SCALE) / anatomy::HIP_SCALE
                    })
                    .collect();
                let target = super::deformation::hanging_targets(rest, &posed, pinned);
                chain::advance(points, &target, rest, dt, &radii, pinned, |p| {
                    hair.support.distance(p)
                })
            } else {
                let rest = super::deformation::fit_surface_controls(
                    targets,
                    &radii,
                    pinned,
                    source.support_side,
                    |p, side, clearance| {
                        geometry::torso_surface_hit_offset(
                            &hair.support,
                            p.x,
                            p.y,
                            side == SupportSide::Front,
                            clearance,
                        )
                    },
                );
                chain::fit(&rest, &radii, pinned, |p| hair.support.distance(p))
            }
        })
        .collect();
    let deformers: Vec<_> = controls
        .iter()
        .zip(&fitted)
        .map(|(source, target)| chain::Deformer::new(source, target))
        .collect();
    record_stage(&mut profile, 0, started);
    for part in &hair.meshes {
        if !part.deforming {
            // The entity's joint transform already moves this rigid geometry.
            // Leave cached positions/normals untouched, including on first fit.
            continue;
        }
        let started = profile.as_ref().map(|_| std::time::Instant::now());
        let mut points = Vec::with_capacity(part.rest.len());
        for (index, rest) in part.rest.iter().enumerate() {
            let mut at = frame.transform_point3(*rest * hair.scale);
            if part.influence[index] > 0.0 {
                let deformed = part.bindings[index].evaluate(|&(curve, segment, t)| {
                    deformers[curve].point(at / anatomy::HIP_SCALE, segment, t)
                });
                at = skin_attachment(at / anatomy::HIP_SCALE, deformed, part.influence[index])
                    * anatomy::HIP_SCALE;
            }
            points.push(at / anatomy::HIP_SCALE);
        }
        let rest_points: Vec<_> = part
            .rest
            .iter()
            .map(|p| frame.transform_point3(*p * hair.scale) / anatomy::HIP_SCALE)
            .collect();
        let edges: Vec<_> = part
            .edges
            .iter()
            .map(|&[a, b]| (a, b, rest_points[a].distance(rest_points[b])))
            .collect();
        record_stage(&mut profile, 1, started);
        let started = profile.as_ref().map(|_| std::time::Instant::now());
        constrain_faces(
            &mut points,
            &part.triangles,
            &edges,
            &part.influence,
            &hair.support,
        );
        record_stage(&mut profile, 2, started);
        let started = profile.as_ref().map(|_| std::time::Instant::now());
        let points: Vec<_> = points
            .into_iter()
            .map(|p| (inverse.transform_point3(p * anatomy::HIP_SCALE) / hair.scale).to_array())
            .collect();
        if let Some(mesh) = meshes.get_mut(&part.handle) {
            mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, points);
            mesh.compute_smooth_normals();
        }
        record_stage(&mut profile, 3, started);
    }
    // A stationary attachment is not enough to cache a still-recovering chain.
    // This tolerance is in normalized torso space, independent of wearer scale.
    hair.settled = fitted.len() == hair.fitted.len()
        && fitted.iter().zip(&hair.fitted).all(|(now, before)| {
            now.len() == before.len()
                && now
                    .iter()
                    .zip(before)
                    .all(|(a, b)| a.distance_squared(*b) <= 1e-12)
        });
    hair.last_frame = Some(frame);
    if hair.rest_frame.is_none() {
        hair.rest_frame = Some(frame);
        hair.fitted_rest = fitted.clone();
    }
    hair.fitted = fitted;
}

/// Resolve face/edge contact on shared vertices after the lock-volume fit.
/// Samples use barycentric weights; fixed attachment vertices have zero mobility.
fn constrain_faces(
    points: &mut [Vec3],
    triangles: &[[usize; 3]],
    edges: &[(usize, usize, f32)],
    influence: &[f32],
    support: &super::visual_contact::Scene,
) {
    const SAMPLES: [[f32; 3]; 4] = [
        [1.0 / 3.0; 3],
        [0.5, 0.5, 0.0],
        [0.0, 0.5, 0.5],
        [0.5, 0.0, 0.5],
    ];
    for _ in 0..24 {
        let mut corrected = false;
        // Edge relaxation is cheap relative to support queries. Diffuse the
        // displacement through the mesh before the next collision sweep.
        for _ in 0..8 {
            corrected |= limit_stretch(points, edges, influence, support);
        }
        for tri in triangles {
            for weights in SAMPLES {
                let p = points[tri[0]] * weights[0]
                    + points[tri[1]] * weights[1]
                    + points[tri[2]] * weights[2];
                if !(0.04..0.60).contains(&p.y) {
                    continue;
                }
                let distance = support.distance(p);
                if distance >= 0.006 {
                    continue;
                }
                let mobility: f32 = (0..3)
                    .map(|i| weights[i] * weights[i] * influence[tri[i]])
                    .sum();
                if mobility < 1e-6 {
                    continue;
                }
                let e = 0.001;
                let gradient = Vec3::new(
                    support.distance(p + Vec3::X * e) - support.distance(p - Vec3::X * e),
                    support.distance(p + Vec3::Y * e) - support.distance(p - Vec3::Y * e),
                    support.distance(p + Vec3::Z * e) - support.distance(p - Vec3::Z * e),
                )
                .normalize_or_zero();
                let correction = gradient * (0.006 - distance).min(0.03);
                for i in 0..3 {
                    if influence[tri[i]] > 0.0 {
                        points[tri[i]] += correction * (weights[i] * influence[tri[i]] / mobility);
                    }
                }
                corrected = true;
            }
        }
        // Contact updates shared vertices after the initial length sweep. Finish
        // each iteration by distributing that displacement through the edges;
        // otherwise the final collision sample leaves an unchecked stretch.
        for _ in 0..64 {
            let stretched = limit_stretch(points, edges, influence, support);
            corrected |= stretched;
            if !stretched {
                break;
            }
        }
        if !corrected {
            break;
        }
    }
}

/// A unilateral stretch constraint: contact may bend a lock, but should not
/// turn a few source faces into a wide sheet. Shared edges spread displacement.
fn limit_stretch(
    points: &mut [Vec3],
    edges: &[(usize, usize, f32)],
    influence: &[f32],
    support: &super::visual_contact::Scene,
) -> bool {
    let mut changed = false;
    for &(a, b, rest) in edges {
        let delta = points[b] - points[a];
        let length = delta.length();
        let excess = length - (rest * 1.5 + 0.002);
        let mobility = influence[a] + influence[b];
        if excess <= 0.0001 || mobility == 0.0 {
            continue;
        }
        let direction = delta / length;
        let slide = |p: Vec3, desired: Vec3| {
            if support.distance(p) >= 0.008 {
                return desired;
            }
            let e = 0.001;
            let normal = Vec3::new(
                support.distance(p + Vec3::X * e) - support.distance(p - Vec3::X * e),
                support.distance(p + Vec3::Y * e) - support.distance(p - Vec3::Y * e),
                support.distance(p + Vec3::Z * e) - support.distance(p - Vec3::Z * e),
            )
            .normalize_or_zero();
            desired - normal * desired.dot(normal).min(0.0)
        };
        let da = if influence[a] > 0.0 {
            slide(points[a], direction) * influence[a]
        } else {
            Vec3::ZERO
        };
        let db = if influence[b] > 0.0 {
            slide(points[b], -direction) * influence[b]
        } else {
            Vec3::ZERO
        };
        let weight = da.dot(direction) - db.dot(direction);
        if weight < 1e-5 {
            continue;
        }
        let shift = (excess / weight).min(0.01);
        points[a] += da * shift;
        points[b] += db * shift;
        changed = true;
    }
    changed
}

/// Immutable cached geometry; animated instances intentionally have other handles.
#[cfg(test)]
pub(super) fn source_meshes(hair: &HairRig) -> Vec<AssetId<Mesh>> {
    hair.meshes.iter().map(|part| part.source.id()).collect()
}

#[cfg(test)]
pub(super) fn attachment_witnesses(hair: &HairRig) -> (Vec<bool>, Vec3) {
    let crown = hair
        .controls
        .iter()
        .map(|curve| curve.points[0].0)
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("hair roots")
        * hair.scale;
    (
        hair.meshes
            .iter()
            .rev()
            .flat_map(|part| part.influence.iter().map(|w| *w == 0.0))
            .collect(),
        crown,
    )
}

// Match the depth-first scene helper: children are pushed in insertion order
// and popped in reverse, while vertices within each mesh retain their order.
#[cfg(test)]
pub(super) fn authored_points(hair: &HairRig) -> Vec<Vec3> {
    hair.meshes
        .iter()
        .rev()
        .flat_map(|part| part.rest.iter().map(|p| *p * hair.scale))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_locks_do_not_share_deforming_triangles() {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                bust_lean: 1,
                ..default()
            }),
        )
        .dressed(Wardrobe::SylvanDuelist);
        let (_, rig, _, _) = super::super::rogue_tests::spawn_one(&look);
        let hair = rig.hair.as_ref().expect("loose hair fixture");
        let mut witnessed = std::collections::BTreeSet::new();
        for part in &hair.meshes {
            for triangle in &part.triangles {
                let mut owners = std::collections::BTreeSet::new();
                for &vertex in triangle {
                    part.bindings[vertex].evaluate(|&(curve, _, _)| {
                        owners.insert(curve);
                        Vec3::ZERO
                    });
                }
                assert_eq!(
                    owners.len(),
                    1,
                    "triangle bridges independently moving locks: {owners:?}"
                );
                witnessed.extend(owners);
            }
        }
        assert_eq!(
            witnessed.len(),
            hair.controls.len(),
            "every authored lock needs rendered coverage"
        );
    }

    #[test]
    fn wearers_keep_independent_contact_meshes_and_cached_sources() {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                bust_lean: 1,
                ..default()
            }),
        )
        .dressed(Wardrobe::SylvanDuelist);
        let (mut world, mut first, mut library, mut meshes) =
            super::super::rogue_tests::spawn_one(&look);
        let positions = |meshes: &Assets<Mesh>| -> std::collections::HashMap<_, Vec<[f32; 3]>> {
            meshes
                .iter()
                .filter_map(
                    |(id, mesh)| match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
                        Some(VertexAttributeValues::Float32x3(p)) => Some((id, p.clone())),
                        _ => None,
                    },
                )
                .collect()
        };
        let before = positions(&meshes);
        let mut equipment = Equipment::default();
        equipment.select("breastplate").unwrap();
        let parent = world.spawn(Transform::default()).id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let second = super::super::spawn(
            &mut Commands::new(&mut queue, &world),
            &mut library,
            &mut meshes,
            parent,
            &look.equipped(equipment),
        );
        queue.apply(&mut world);
        let after = positions(&meshes);
        for (id, points) in &before {
            assert_eq!(
                after.get(id),
                Some(points),
                "composing another wearer mutated an existing mesh"
            );
        }
        let head = world
            .get::<Transform>(first.joint(J::Head).unwrap())
            .unwrap()
            .compute_affine();
        let first_hair = first.hair.as_mut().unwrap();
        let second_hair = second.hair.as_ref().unwrap();
        // Independently protect the authored crown, rather than defining scalp
        // solely as whichever vertices the mobility calculation happens to pin.
        let crown = first_hair
            .controls
            .iter()
            .map(|c| c.points[0].0.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut crown_vertices = 0;
        for part in &first_hair.meshes {
            for (point, &mobility) in part.rest.iter().zip(&part.influence) {
                if point.y >= crown {
                    assert_eq!(mobility, 0.0, "authored crown must stay attached");
                    crown_vertices += 1;
                }
            }
        }
        assert!(
            crown_vertices > 20,
            "crown witness must cover real source geometry"
        );

        let mut rigid = 0;
        for a in &first_hair.meshes {
            if a.influence.iter().all(|&weight| weight == 0.0) {
                rigid += 1;
                assert_eq!(a.handle, a.source, "rigid lock must reuse its source mesh");
                assert!(second_hair.meshes.iter().any(|b| a.handle == b.handle));
            } else {
                assert_ne!(a.handle, a.source, "deforming lock must own its mesh");
                assert!(second_hair.meshes.iter().all(|b| a.handle != b.handle));
            }
        }
        assert!(rigid > 0, "fixture must exercise a fully attached lock");
        let drape = world
            .get::<Transform>(first_hair.entity)
            .unwrap()
            .compute_affine();
        place(
            first_hair,
            head * drape * Affine3A::from_quat(Quat::from_rotation_x(0.15)),
            0.016,
            &mut meshes,
        );
        let fixed_frame = head * drape * Affine3A::from_quat(Quat::from_rotation_x(0.15));
        let displaced = first_hair.fitted.clone();
        place(first_hair, fixed_frame, 0.016, &mut meshes);
        assert!(
            first_hair.fitted != displaced,
            "stationary attachment must allow free controls to settle"
        );
        let paused = first_hair.fitted.clone();
        place(first_hair, fixed_frame, 0.0, &mut meshes);
        assert_eq!(
            first_hair.fitted, paused,
            "paused presentation must preserve controls"
        );
        let animated = positions(&meshes);
        let mut changed = 0;
        for (id, points) in &after {
            if first_hair
                .meshes
                .iter()
                .any(|part| part.handle.id() == *id && part.influence.iter().any(|&w| w > 0.0))
            {
                changed += usize::from(animated.get(id) != Some(points));
            } else {
                assert_eq!(
                    animated.get(id),
                    Some(points),
                    "animation mutated a cached source or other wearer"
                );
            }
        }
        assert!(changed > 0, "the isolation check must exercise deformation");
    }

    // Opt-in evidence for the preserved failure fixture. Ordinary runs keep
    // their bounded output; this report does not change fitting or assertions.
    fn report_fit_stages(hair: &HairRig, frame: Affine3A, meshes: &Assets<Mesh>) {
        if std::env::var_os("TOR_HAIR_DIAGNOSTICS").is_none() {
            return;
        }
        let arrays = |points: &[Vec3]| points.iter().map(|p| p.to_array()).collect::<Vec<_>>();
        let curves: Vec<_> = hair.controls.iter().enumerate().map(|(index, source)| {
            let pinned = &source.attached;
            let authored: Vec<_> = source.points.iter().map(|(p, _)| frame.transform_point3(*p * hair.scale) / anatomy::HIP_SCALE).collect();
            let radii: Vec<_> = source.points.iter().map(|(_, r)| r * hair.scale.max_element() / anatomy::HIP_SCALE.min_element() + 0.020).collect();
            let rest = super::deformation::fit_surface_controls(&authored, &radii, pinned, source.support_side, |p, side, clearance| {
                geometry::torso_surface_hit_offset(&hair.support, p.x, p.y, side == SupportSide::Front, clearance)
            });
            let body = chain::fit(&rest, &radii, pinned, |p| hair.support.body.distance(p));
            let full = chain::fit(&rest, &radii, pinned, |p| hair.support.distance(p));
            serde_json::json!({"index":index,"authored":arrays(&authored),"surface_fit":arrays(&rest),"support_side":format!("{:?}",source.support_side),"radii":radii,"pinned":pinned,"body_only":arrays(&body),"full_contact":arrays(&full),"current":arrays(&hair.fitted[index])})
        }).collect();
        let surfaces: Vec<_> = hair.meshes.iter().map(|part| {
            let authored: Vec<_> = part.rest.iter().map(|p| frame.transform_point3(*p * hair.scale) / anatomy::HIP_SCALE).collect();
            let mesh = meshes.get(&part.handle).unwrap();
            let Some(VertexAttributeValues::Float32x3(vertices)) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!("diagnostic mesh positions") };
            let final_points: Vec<_> = vertices.iter().map(|p| frame.transform_point3(Vec3::from_array(*p) * hair.scale) / anatomy::HIP_SCALE).collect();
            serde_json::json!({"authored":arrays(&authored),"final":arrays(&final_points),"influence":part.influence})
        }).collect();
        eprintln!(
            "HAIR_FIT_STAGES {}",
            serde_json::json!({"version":2,"frame":"normalized torso support","curves":curves,"surfaces":surfaces})
        );
    }

    #[test]
    fn contact_survives_jump_head_pose() {
        let mut equipment = Equipment::default();
        equipment.select("breastplate").unwrap();
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                bust_lean: 1,
                ..default()
            }),
        )
        .dressed(Wardrobe::SylvanDuelist)
        .equipped(equipment);
        assert_jump_contact(look);
    }

    #[test]
    fn contact_survives_nymph_gown_and_cloak_jump() {
        let mut equipment = Equipment::default();
        equipment.select("cloak").unwrap();
        let look = Look::with_choice(
            CreatureModel::Nymph,
            7,
            Some(Variant {
                woman: true,
                build: 2,
                bust: 4,
                ..default()
            }),
        )
        .dressed(Wardrobe::RoyalPrincess)
        .equipped(equipment);
        assert_jump_contact(look);
    }

    fn assert_jump_contact(look: Look) {
        let (mut app, root) = super::super::rogue_tests::animate_look(look, 1, |_| Vec3::ZERO);
        {
            let world = app.world();
            let rig = world.get::<Rig>(root).unwrap();
            let hair = rig.hair.as_ref().unwrap();
            let frame = world
                .get::<Transform>(rig.joint(J::Head).unwrap())
                .unwrap()
                .compute_affine()
                * world
                    .get::<Transform>(hair.entity)
                    .unwrap()
                    .compute_affine();
            report_fit_stages(hair, frame, world.resource::<Assets<Mesh>>());
        }
        app.world_mut()
            .get_mut::<Anim>(root)
            .unwrap()
            .act(Move::Jump);
        let mut previous_controls: Vec<Vec3> = Vec::new();
        let mut previous_vertices: Vec<Vec3> = Vec::new();
        for tick in 0..30 {
            app.update();
            let world = app.world();
            let rig = world.get::<Rig>(root).unwrap();
            let hair = rig.hair.as_ref().unwrap();
            let frame = world
                .get::<Transform>(rig.joint(J::Head).unwrap())
                .unwrap()
                .compute_affine()
                * world
                    .get::<Transform>(hair.entity)
                    .unwrap()
                    .compute_affine();
            let meshes = world.resource::<Assets<Mesh>>();
            let mut checked = 0;
            let mut vertices = Vec::new();
            for part in &hair.meshes {
                let mesh = meshes.get(&part.handle).unwrap();
                assert!(
                    mesh.asset_usage.contains(RenderAssetUsages::MAIN_WORLD),
                    "contact meshes must survive render extraction"
                );
                let Some(VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("hair positions")
                };
                vertices.extend(points.iter().map(|p| {
                    frame.transform_point3(Vec3::from_array(*p) * hair.scale) / anatomy::HIP_SCALE
                }));
                for &[a, b] in &part.edges {
                    let rest_length = (part.rest[b] - part.rest[a]).length();
                    let length = Vec3::from_array(points[b]).distance(Vec3::from_array(points[a]));
                    if length > rest_length * 2.0 + 0.004 {
                        for i in [a, b] {
                            let rest = frame.transform_point3(part.rest[i] * hair.scale)
                                / anatomy::HIP_SCALE;
                            let fitted = frame
                                .transform_point3(Vec3::from_array(points[i]) * hair.scale)
                                / anatomy::HIP_SCALE;
                            eprintln!("edge vertex={i} influence={} binding={:?} rest={rest:?} fitted={fitted:?} rest_gap={} fitted_gap={}",part.influence[i],part.bindings[i],hair.support.distance(rest),hair.support.distance(fitted));
                        }
                    }
                    assert!(
                        length <= rest_length * 2.0 + 0.004,
                        "tick={tick}: hair edge stretched from {rest_length} to {length}"
                    );
                }
                let arms = super::super::visual_contact::posed(&rig.visual_contacts, |j| {
                    rig.joint(j)
                        .and_then(|entity| world.get::<Transform>(entity))
                        .map(Transform::compute_affine)
                });
                for p in points {
                    let at = frame.transform_point3(Vec3::from_array(*p) * hair.scale)
                        / anatomy::HIP_SCALE;
                    if (0.12..0.50).contains(&at.y) {
                        assert!(
                            arms.iter().all(|c| c.distance(at) >= -0.003),
                            "tick={tick}: hair crosses posed arm at {at:?}"
                        );
                    }
                }
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                for tri in indices.as_chunks::<3>().0 {
                    let v = tri.map(|i| {
                        frame.transform_point3(Vec3::from_array(points[i]) * hair.scale)
                            / anatomy::HIP_SCALE
                    });
                    let center = (v[0] + v[1] + v[2]) / 3.0;
                    if (0.12..0.50).contains(&center.y) {
                        assert!(
                            hair.support.distance(center) > 0.001,
                            "tick={tick}: hair face crosses torso at {center:?}, gap={}",
                            hair.support.distance(center)
                        );
                    }
                }
                for (i, p) in points.iter().enumerate() {
                    if part.influence[i] == 0.0 {
                        assert!(
                            Vec3::from_array(*p).distance(part.rest[i]) < 1e-5,
                            "scalp attachment must stay fixed"
                        );
                        continue;
                    }
                    let at = frame.transform_point3(Vec3::from_array(*p) * hair.scale)
                        / anatomy::HIP_SCALE;
                    let uncached: &crate::sdf::Sculpt = &hair.support.body;
                    assert_eq!(
                        hair.support.body.distance(at).to_bits(),
                        uncached.distance(at).to_bits(),
                        "regional cache changed composed body contact at {at:?}"
                    );
                    if (0.12..0.40).contains(&at.y) && at.z > 0.0 {
                        if let Some(surface) =
                            geometry::torso_surface_hit(&hair.support, at.x, at.y, true)
                        {
                            assert!(
                                at.z > surface.z + 0.005,
                                "tick={tick} hair at {at:?}, support={surface:?}"
                            );
                            checked += 1;
                        }
                    }
                }
            }
            let controls: Vec<Vec3> = hair.fitted.iter().flatten().copied().collect();
            if !previous_controls.is_empty() {
                assert_eq!(controls.len(), previous_controls.len());
                assert_eq!(vertices.len(), previous_vertices.len());
                let max_move = |old: &[Vec3], new: &[Vec3]| {
                    old.iter()
                        .zip(new)
                        .enumerate()
                        .map(|(i, (a, b))| (i, a.distance(*b)))
                        .max_by(|a, b| a.1.total_cmp(&b.1))
                        .unwrap()
                };
                let (control, control_delta) = max_move(&previous_controls, &controls);
                let (vertex, vertex_delta) = max_move(&previous_vertices, &vertices);
                eprintln!("tick={tick} front={checked} control_delta={control_delta} control={control} {:?}->{:?} vertex_delta={vertex_delta} vertex={vertex} {:?}->{:?}",previous_controls[control],controls[control],previous_vertices[vertex],vertices[vertex]);
            }
            previous_controls = controls;
            previous_vertices = vertices;
            assert!(checked > 20, "tick={tick}: front witnesses={checked}");
        }
    }
}
