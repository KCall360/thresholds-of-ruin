//! Read one joint's rendered geometry, including its owned soft-skin frames.
use crate::creatures::Rig;
use bevy::{math::Affine3A, prelude::*};
use std::collections::HashSet;

pub(super) struct JointMesh<'a> {
    pub mesh: &'a Mesh,
    pub transform: Affine3A,
}

pub(super) fn joint_meshes<'a>(
    world: &World,
    rig: &Rig,
    meshes: &'a Assets<Mesh>,
    joint: Entity,
) -> Vec<JointMesh<'a>> {
    // Attachment frames belong to this joint; other animated joints and drapes
    // own separate surfaces and must not fill a missing patch in this one.
    let boundaries: HashSet<_> = rig
        .joints
        .iter()
        .flatten()
        .copied()
        .chain(rig.drapes.iter().map(|(entity, _)| *entity))
        .collect();
    let mut pending = vec![(joint, Affine3A::IDENTITY)];
    let mut output = Vec::new();
    while let Some((parent, frame)) = pending.pop() {
        let Some(children) = world.get::<Children>(parent) else {
            continue;
        };
        for child in children.iter() {
            if boundaries.contains(&child) {
                continue;
            }
            let transform = frame
                * world
                    .get::<Transform>(child)
                    .map_or(Affine3A::IDENTITY, Transform::compute_affine);
            if let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) {
                output.push(JointMesh { mesh, transform });
            }
            pending.push((child, transform));
        }
    }
    output
}
