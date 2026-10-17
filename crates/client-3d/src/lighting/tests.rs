use super::*;

#[test]
fn death_and_resurrection_while_dark_preserve_the_creatures_glow_state() {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    let source = library.materials.glow(props::Accent::Amber);
    let ember = library.materials.dim(&source).unwrap();
    app.insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, (shade, lamps).chain());
    let root = app.world_mut().spawn(Lit(true)).id();
    let mesh = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), ChildOf(root)))
        .id();
    let lamp = app
        .world_mut()
        .spawn((
            PointLight {
                intensity: 123.0,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    app.update();
    app.world_mut().entity_mut(root).insert(Lit(false));
    app.update();
    app.world_mut()
        .entity_mut(root)
        .insert(creatures::Snuffed::default());
    app.update();
    app.world_mut().entity_mut(root).insert(Lit(true));
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0,
        ember
    );
    assert_eq!(app.world().get::<PointLight>(lamp).unwrap().intensity, 0.0);
    app.world_mut()
        .entity_mut(root)
        .remove::<creatures::Snuffed>();
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0,
        source
    );
    assert_eq!(
        app.world().get::<PointLight>(lamp).unwrap().intensity,
        123.0
    );
}

#[test]
fn dark_roots_suppress_and_restore_materials_and_lights_including_new_children() {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let source = materials.add(StandardMaterial {
        emissive: LinearRgba::WHITE,
        ..default()
    });
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    let initial_materials = materials.len();
    app.init_resource::<Game>()
        .insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, (shade, lamps).chain());
    let root = app.world_mut().spawn(Lit(true)).id();
    let mesh = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), ChildOf(root)))
        .id();
    let lamp = app
        .world_mut()
        .spawn((
            PointLight {
                intensity: 123.0,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0,
        source
    );
    for lit in [false, true, false, true] {
        app.world_mut().entity_mut(root).insert(Lit(lit));
        app.world_mut().resource_mut::<Game>().view_serial += 1;
        app.update();
        let handle = &app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0;
        assert_eq!(*handle == source, lit);
        let material = app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .get(handle)
            .unwrap();
        assert_eq!(material.unlit, !lit);
        if !lit {
            assert_eq!(material.emissive, LinearRgba::BLACK);
        }
        assert_eq!(
            app.world().get::<PointLight>(lamp).unwrap().intensity,
            if lit { 123.0 } else { 0.0 }
        );
    }
    app.world_mut().entity_mut(root).insert(Lit(false));
    app.world_mut().resource_mut::<Game>().view_serial += 1;
    app.update();
    let held = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), ChildOf(root)))
        .id();
    app.update();
    assert_ne!(
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(held)
            .unwrap()
            .0,
        source
    );
    assert_eq!(
        app.world().resource::<Assets<StandardMaterial>>().len(),
        initial_materials + 1,
        "all dark descendants share one material rather than allocating per frame"
    );
}
