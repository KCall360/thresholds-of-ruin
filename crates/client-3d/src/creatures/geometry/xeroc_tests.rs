//! The true form needs a substantial maw and sculpted shell, rather than a
//! small turtle head in front of a smooth, painted dome.
use super::*;
use crate::creatures::rogue_tests::{points_under, spawn_one};
use bevy::render::mesh::VertexAttributeValues;
use std::collections::{BTreeMap, BTreeSet};

fn bounds(points: &[Vec3]) -> (Vec3, Vec3) {
    assert!(!points.is_empty());
    points.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(low, high), p| (low.min(*p), high.max(*p)),
    )
}

// Only the joint's directly attached mesh children: a head's lower jaw remains
// an independent articulated part, and the body query excludes its limbs.
fn painted_faces(
    world: &World,
    meshes: &Assets<Mesh>,
    joint: Entity,
    pigment: Vec3,
) -> Vec<[Vec3; 3]> {
    let c = Color::srgb(pigment.x, pigment.y, pigment.z).to_linear();
    let target = Vec3::new(c.red, c.green, c.blue);
    let mut faces = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("pigments");
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for tri in indices.as_chunks::<3>().0 {
            if tri.iter().all(|i| {
                Vec3::new(colors[*i][0], colors[*i][1], colors[*i][2]).distance(target) < 1e-5
            }) {
                faces.push(tri.map(|i| transform.transform_point3(Vec3::from_array(points[i]))));
            }
        }
    }
    faces
}

#[test]
fn xeroc_true_form_has_a_large_fanged_maw_and_substantial_lower_jaw() {
    let mut failures = Vec::new();
    for build in 0..3 {
        let look = Look::with_choice(
            CreatureModel::Xeroc,
            42,
            Some(Variant { build, ..default() }),
        );
        let (world, rig, _, meshes) = spawn_one(&look);
        let head = rig.joint(J::Head).unwrap();
        let jaw = rig.joint(J::Jaw).unwrap();
        let (low, high) = bounds(&points_under(&world, &meshes, head));
        let (jaw_low, jaw_high) = bounds(&points_under(&world, &meshes, jaw));
        let fangs: Vec<_> = painted_faces(&world, &meshes, head, BONE)
            .into_iter()
            .flatten()
            .collect();
        let (fang_low, fang_high) = bounds(&fangs);
        let size = high - low;
        let jaw_size = jaw_high - jaw_low;
        let fang_size = fang_high - fang_low;
        if size.x < 0.48
            || size.y < 0.32
            || size.y / size.x < 0.62
            || jaw_size.x < size.x * 0.72
            || jaw_size.y < 0.09
            || fang_size.x < 0.35
            || fang_size.y < 0.09
        {
            failures.push(format!(
                "build={build}: head={size:?}, jaw={jaw_size:?}, upper fang arch={fang_size:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn xeroc_lower_fangs_emerge_above_the_mouth_pad_across_the_lip() {
    let look = Look::of(CreatureModel::Xeroc, 42);
    let (world, rig, _, meshes) = spawn_one(&look);
    let points: Vec<_> = painted_faces(&world, &meshes, rig.joint(J::Jaw).unwrap(), BONE)
        .into_iter()
        .flatten()
        .filter(|p| p.y > 0.025)
        .collect();
    assert!(
        !points.is_empty(),
        "lower fangs are buried in the mouth pad"
    );
    let (low, high) = bounds(&points);
    assert!(
        high.x - low.x > 0.32,
        "exposed fangs must span the lip, not one central tooth"
    );
}

#[test]
fn xeroc_shell_has_broad_raised_overlapping_armor_masses() {
    let look = Look::of(CreatureModel::Xeroc, 42);
    let (world, rig, _, meshes) = spawn_one(&look);
    let faces = painted_faces(
        &world,
        &meshes,
        rig.joint(J::Body).unwrap(),
        look.skin * 1.08,
    );
    assert!(
        !faces.is_empty(),
        "Xeroc needs physical shell plates, not painted seams"
    );
    let key = |p: Vec3| p.to_array().map(|v| (v * 100_000.0).round() as i32);
    let mut at_vertex: BTreeMap<[i32; 3], Vec<usize>> = BTreeMap::new();
    for (i, tri) in faces.iter().enumerate() {
        for p in tri {
            at_vertex.entry(key(*p)).or_default().push(i);
        }
    }
    let mut seen = BTreeSet::new();
    let mut broad = 0;
    let mut left = 0;
    let mut right = 0;
    for start in 0..faces.len() {
        if !seen.insert(start) {
            continue;
        }
        let mut pending = vec![start];
        let mut points = Vec::new();
        while let Some(i) = pending.pop() {
            points.extend(faces[i]);
            for p in faces[i] {
                for next in &at_vertex[&key(p)] {
                    if seen.insert(*next) {
                        pending.push(*next);
                    }
                }
            }
        }
        let (low, high) = bounds(&points);
        let size = high - low;
        if size.x.max(size.z) >= 0.12 && size.y >= 0.012 {
            broad += 1;
            left += usize::from((low.x + high.x) * 0.5 < -0.03);
            right += usize::from((low.x + high.x) * 0.5 > 0.03);
        }
    }
    assert!(broad >= 5 && left >= 2 && right >= 2,
        "shell plates must cover both shoulders and flanks: broad={broad}, left/right={left}/{right}");
}
