//! The trap and leaf masses carry the plant's silhouette; its base stays rooted.
use super::*;
use crate::creatures::rogue_tests::{animate_look, spawn_one};
use bevy::render::mesh::VertexAttributeValues;

fn direct_points(
    world: &World,
    meshes: &Assets<Mesh>,
    joint: Entity,
    green_only: bool,
) -> Vec<Vec3> {
    let mut out = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(vertices)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors");
        };
        let local = world.get::<Transform>(child).unwrap().compute_affine();
        for (p, c) in vertices.iter().zip(colors) {
            if !green_only || (c[1] > c[0] * 1.3 && c[1] > c[2] * 1.3) {
                out.push(local.transform_point3((*p).into()));
            }
        }
    }
    assert!(!out.is_empty() && out.iter().all(|p| p.is_finite()));
    out
}

#[test]
fn flytrap_has_a_large_deep_two_lobed_trap() {
    for build in 0..3 {
        let look = Look::with_choice(
            CreatureModel::Flytrap,
            42,
            Some(Variant { build, ..default() }),
        );
        let (world, rig, _, meshes) = spawn_one(&look);
        for joint in [J::Head, J::Jaw] {
            let p = direct_points(&world, &meshes, rig.joint(joint).unwrap(), false);
            let (low, high) = p.iter().fold(
                (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
                |(a, b), p| (a.min(*p), b.max(*p)),
            );
            let size = high - low;
            assert!(
                size.x > 0.55 && size.y > 0.16 && size.z > 0.50,
                "build={build} {joint:?}: trap lobe too small/shallow for the stalk: {size:?}"
            );
        }
    }
}

#[test]
fn flytrap_has_a_broad_leaf_rosette_and_substantial_supporting_stalk() {
    let look = Look::of(CreatureModel::Flytrap, 42);
    let (world, rig, _, meshes) = spawn_one(&look);
    let base = direct_points(&world, &meshes, rig.joint(J::Body).unwrap(), true);
    let extent = base
        .iter()
        .map(|p| Vec2::new(p.x, p.z).length())
        .fold(0.0, f32::max);
    assert!(
        extent > 0.44,
        "leaf rosette is too small: radial extent={extent:.4}"
    );
    for (joint, next) in [(J::Neck, J::Torso), (J::Torso, J::Head)] {
        let points = direct_points(&world, &meshes, rig.joint(joint).unwrap(), true);
        let axis = rig.rest[next as usize].normalize();
        let length = rig.rest[next as usize].length();
        let core = points
            .iter()
            .filter(|p| {
                let t = p.dot(axis) / length;
                (0.0..0.25).contains(&t)
            })
            .map(|p| (*p - axis * p.dot(axis)).length())
            .fold(0.0, f32::max);
        assert!(
            core > 0.07,
            "{joint:?}: thin supporting stalk radius={core:.4}"
        );
    }
}

#[test]
fn flytrap_opens_its_lobes_at_rest_and_snaps_without_uprooting() {
    let (mut app, root) = animate_look(Look::of(CreatureModel::Flytrap, 42), 12, |_| Vec3::ZERO);
    let rig = app.world().get::<Rig>(root).unwrap();
    let jaw = rig.joint(J::Jaw).unwrap();
    let base = rig.joint(J::Body).unwrap();
    let gape = app
        .world()
        .get::<Transform>(jaw)
        .unwrap()
        .rotation
        .angle_between(Quat::IDENTITY);
    assert!(
        gape > 0.65,
        "resting trap should display its red interior, relative gape={gape:.4}"
    );
    let m = Move::Strike;
    app.world_mut().get_mut::<Anim>(root).unwrap().action =
        Some((m, m.impact() - 0.016 / m.duration()));
    app.update();
    let shut = app
        .world()
        .get::<Transform>(jaw)
        .unwrap()
        .rotation
        .angle_between(Quat::IDENTITY);
    assert!(shut < 0.16, "trap must close at impact: {shut:.4}");
    assert!(
        app.world()
            .get::<Transform>(base)
            .unwrap()
            .translation
            .length()
            < 1e-6
    );
    assert!(
        app.world()
            .get::<Transform>(root)
            .unwrap()
            .translation
            .length()
            < 1e-6
    );
}
