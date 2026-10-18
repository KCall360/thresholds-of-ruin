use super::*;
use bevy::ecs::world::CommandQueue;

fn assert_drawable(model: &props::Model, meshes: &Assets<Mesh>) {
    assert!(
        !model.0.is_empty(),
        "fallback must contain drawable geometry"
    );
    for (handle, _) in &model.0 {
        let mesh = meshes.get(handle).unwrap();
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap();
        assert!(positions.len() >= 3);
        assert!(positions.iter().flatten().all(|value| value.is_finite()));
        assert!(positions
            .iter()
            .any(|point| point.iter().any(|value| value.abs() > 0.05)));
    }
}

#[test]
fn missing_creature_has_visible_geometry_and_no_invented_weapon_hand() {
    let mut world = World::default();
    let parent = world
        .spawn((Transform::default(), Visibility::default()))
        .id();
    let mut queue = CommandQueue::default();
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
    {
        let mut commands = Commands::new(&mut queue, &world);
        let look = Look::of(CreatureModel::Placeholder, 123);
        let rig = creatures::spawn(&mut commands, &mut library, &mut meshes, parent, &look);
        assert!(rig.joint(J::HandR).is_none());
    }
    queue.apply(&mut world);
    let mut query = world.query::<&Mesh3d>();
    let models: Vec<_> = query
        .iter(&world)
        .map(|mesh| (mesh.0.clone(), Handle::<StandardMaterial>::default()))
        .collect();
    assert_drawable(&props::Model(models), &meshes);
}

#[test]
fn missing_item_is_drawable_and_reuses_its_mesh() {
    let mut meshes = Assets::<Mesh>::default();
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(PropMaterials::new(&mut materials));
    let first = library.item(ItemModel::Placeholder, &mut meshes);
    assert_drawable(&first, &meshes);
    let count = meshes.len();
    for _ in 0..20 {
        assert_eq!(library.item(ItemModel::Placeholder, &mut meshes).0, first.0);
    }
    assert_eq!(meshes.len(), count);
}
