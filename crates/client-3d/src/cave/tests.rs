use super::*;
use bevy::render::mesh::VertexAttributeValues;

fn cave(key: u64, seen: bool, lit: bool, above_lit: bool) -> CaveWorld {
    let mut world = CaveWorld::default();
    world.update(
        [IVec3::ZERO, IVec3::new(1, 1, 0)]
            .into_iter()
            .map(|at| (at, Solid { key, seen, lit }))
            .collect(),
        HashSet::from([IVec3::Y]),
        if above_lit {
            HashSet::from([IVec3::Y])
        } else {
            HashSet::new()
        },
        HashSet::new(),
    );
    world
}

#[test]
fn dark_awareness_and_memory_have_no_glow_and_an_explicit_dark_mesh_mask() {
    let key = (0..1000)
        .find(|key| {
            !cave(*key, true, true, true)
                .build(IVec3::ZERO)
                .lights
                .is_empty()
        })
        .expect("positive crystal control");
    let lit = cave(key, true, true, true).build(IVec3::ZERO);
    assert!(lit.crystals.iter().any(Option::is_some));
    for (seen, lit, above_lit) in [
        (true, false, false),
        (false, true, false),
        (true, false, true),
    ] {
        let built = cave(key, seen, lit, above_lit).build(IVec3::ZERO);
        assert!(built.lights.is_empty());
        assert!(built.crystals.iter().all(Option::is_none));
        let rock = built.rock.unwrap();
        let Some(VertexAttributeValues::Float32x4(colors)) = rock.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("vertex mask")
        };
        assert!(!colors.is_empty());
        assert!(colors.iter().all(|color| color[3] == 0.0));
    }
    assert!(
        cave(key, true, true, false)
            .build(IVec3::ZERO)
            .lights
            .is_empty(),
        "a lit floor must not grow a light into a dark open cell"
    );
}

#[test]
fn changing_only_illumination_rebuilds_the_affected_chunks() {
    let mut world = cave(12, true, true, true);
    let solids = world
        .solids
        .iter()
        .map(|(at, solid)| {
            (
                *at,
                Solid {
                    lit: false,
                    ..*solid
                },
            )
        })
        .collect();
    let dirty = world.update(solids, world.open.clone(), HashSet::new(), HashSet::new());
    assert!(dirty.contains(&IVec3::ZERO));
}
