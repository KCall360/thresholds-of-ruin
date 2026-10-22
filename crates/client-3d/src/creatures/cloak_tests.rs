//! The emitted cloak is one piece of cloth, rather than disconnected flutes.
use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::{BTreeMap, BTreeSet};

fn cloak_meshes(world: &World, rig: &Rig) -> Vec<Handle<Mesh>> {
    let mut pending: Vec<_> = rig
        .drapes
        .iter()
        .filter_map(|(e, kind)| (*kind == Drape::Cloak).then_some(*e))
        .collect();
    let mut meshes = Vec::new();
    while let Some(entity) = pending.pop() {
        if let Some(mesh) = world.get::<Mesh3d>(entity) {
            meshes.push(mesh.0.clone());
        }
        if let Some(children) = world.get::<Children>(entity) {
            pending.extend(children.iter());
        }
    }
    meshes
}

#[test]
fn native_cloaks_emit_one_connected_closed_sheet() {
    for model in [CreatureModel::Vampire, CreatureModel::Wraith] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    42,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let (world, rig, _, meshes) = rogue_tests::spawn_one(&look);
                let mut welded = BTreeMap::new();
                let mut edges = BTreeMap::<(usize, usize), usize>::new();
                let mut neighbors = BTreeMap::<usize, BTreeSet<usize>>::new();
                for handle in cloak_meshes(&world, &rig) {
                    let mesh = meshes.get(&handle).unwrap();
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("cloak vertices")
                    };
                    let ids: Vec<_> = points
                        .iter()
                        .map(|p| {
                            let key = p.map(|v| (v * 1e6).round() as i32);
                            let next = welded.len();
                            *welded.entry(key).or_insert(next)
                        })
                        .collect();
                    let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                    for triangle in indices.as_chunks::<3>().0 {
                        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                            let (a, b) = (ids[triangle[a]], ids[triangle[b]]);
                            if a == b {
                                continue;
                            }
                            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
                            neighbors.entry(a).or_default().insert(b);
                            neighbors.entry(b).or_default().insert(a);
                        }
                    }
                }
                assert!(!neighbors.is_empty(), "{model:?} needs a cloak");
                let mut remaining: BTreeSet<_> = neighbors.keys().copied().collect();
                let mut components = 0;
                while let Some(start) = remaining.pop_first() {
                    components += 1;
                    let mut pending = vec![start];
                    while let Some(vertex) = pending.pop() {
                        for adjacent in &neighbors[&vertex] {
                            if remaining.remove(adjacent) {
                                pending.push(*adjacent);
                            }
                        }
                    }
                }
                assert_eq!(
                    components, 1,
                    "{model:?} woman={woman} build={build}: cloth must connect across the back"
                );
                assert!(
                    edges.values().all(|uses| *uses == 2),
                    "the lining and exterior must close at the perimeter"
                );
            }
        }
    }
}

#[test]
fn replacement_cloaks_keep_the_wearers_ghost_material() {
    for model in [
        CreatureModel::Vampire,
        CreatureModel::Phantom,
        CreatureModel::Wraith,
        CreatureModel::Zombie,
    ] {
        for wardrobe in [Wardrobe::Ranger, Wardrobe::Courtier] {
            let look = Look::with_choice(
                model,
                42,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            )
            .dressed(wardrobe);
            let (world, rig, library, _) = rogue_tests::spawn_one(&look);
            let cloaks: Vec<_> = rig
                .drapes
                .iter()
                .filter(|(_, kind)| *kind == Drape::Cloak)
                .collect();
            assert_eq!(cloaks.len(), 1, "{model:?} {wardrobe:?}");
            let material = world
                .get::<MeshMaterial3d<StandardMaterial>>(cloaks[0].0)
                .unwrap();
            assert_eq!(
                material.0 == library.materials.ghost(),
                model == CreatureModel::Phantom,
                "{model:?} {wardrobe:?}"
            );
        }
    }
}
