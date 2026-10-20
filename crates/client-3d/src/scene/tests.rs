use super::*;
use tor_client_3d_model::Actor;

#[derive(Resource)]
struct Fixture(View);

#[test]
fn identifying_a_corpse_refreshes_its_cached_recipe_without_rebuilding_unchanged_views() {
    use tor_protocol::{AppearanceSeed, ItemTarget};
    let mut app = app(CreatureModel::Placeholder);
    let id = ItemTarget::from_digest([81; 32]);
    app.world_mut()
        .resource_mut::<Fixture>()
        .0
        .items
        .push(tor_client_3d_model::GroundItem {
            corpse_seed: None,
            model: ItemModel::Corpse,
            corpse_model: CreatureModel::Placeholder,
            id,
            name: "unknown remains".into(),
            class: ItemClass::Corpse,
            quantity: 1,
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
            reachable: false,
        });
    app.update();
    let before = app.world().resource::<Game>().corpses[&id].entity;
    app.update();
    assert_eq!(
        app.world().resource::<Game>().corpses[&id].entity,
        before,
        "unchanged view retains its rig"
    );
    {
        let mut view = app.world_mut().resource_mut::<Fixture>();
        view.0.items[0].corpse_seed = Some(AppearanceSeed::from_digest([19; 32]));
        view.0.items[0].corpse_model = CreatureModel::Zombie;
        view.0.items[0].name = "zombie remains".into();
        view.0.revision += 1;
        view.0.shown.revision = view.0.revision;
    }
    app.update();
    let after = app.world().resource::<Game>().corpses[&id].entity;
    assert_ne!(
        before, after,
        "newly disclosed source must replace the generic corpse"
    );
    assert!(
        !app.world().entities().contains(before),
        "the old marker is removed"
    );
    assert!(
        app.world()
            .get::<creatures::Rig>(after)
            .unwrap()
            .joint(J::Head)
            .is_some(),
        "disclosed zombie uses its articulated recipe"
    );
    app.update();
    assert_eq!(app.world().resource::<Game>().corpses[&id].entity, after);
}

#[test]
fn a_new_stream_or_branch_drops_abandoned_ui_intentions_and_state_cues() {
    let mut app = app(CreatureModel::Placeholder);
    app.update();
    app.world_mut()
        .resource_mut::<interact::Pursuit>()
        .fetch(tor_protocol::ItemTarget::from_digest([9; 32]));
    app.world_mut().resource_mut::<interact::Panel>().open = true;
    {
        let mut game = app.world_mut().resource_mut::<Game>();
        game.hurt = 0.3;
        game.log.push_back("abandoned future".into());
    }
    {
        let mut view = app.world_mut().resource_mut::<Fixture>();
        view.0.context.epoch += 1;
        view.0.shown.input.stream = view.0.context.clone();
        view.0.branch.0 = "rewound".into();
        view.0.shown.branch = view.0.branch.clone();
    }
    app.update();
    let mut retained = Vec::new();
    if app.world().resource::<interact::Pursuit>().active() {
        retained.push("pursuit");
    }
    if app.world().resource::<interact::Panel>().open {
        retained.push("inventory selection");
    }
    let game = app.world().resource::<Game>();
    if game.hurt > 0.0 {
        retained.push("wound cue");
    }
    if !game.log.is_empty() {
        retained.push("abandoned narration");
    }
    assert!(
        retained.is_empty(),
        "old scope state survived: {retained:?}"
    );
}

fn project(
    mut commands: Commands,
    mut game: ResMut<Game>,
    palette: Res<Palette>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    view: Res<Fixture>,
    mut bodies: Query<&mut ActorBody>,
) {
    align(&mut commands, &mut game, &view.0);
    build_actors(
        &mut commands,
        &mut game,
        &palette,
        &mut library,
        &mut meshes,
        &view.0,
        &mut bodies,
        &HashSet::new(),
    );
    build_items(&mut commands, &mut game, &mut library, &mut meshes, &view.0);
    game.view = Some(view.0.clone());
}

#[test]
fn only_the_same_cosmetic_identity_in_the_same_3d_cell_reuses_a_fallen_body() {
    use tor_protocol::{AppearanceSeed, ItemTarget};
    for (height, seed, expected) in [(3, 19, false), (0, 37, false), (0, 19, true)] {
        let mut app = app(CreatureModel::Placeholder);
        app.update();
        let entity = *app
            .world()
            .resource::<Game>()
            .actors
            .values()
            .next()
            .unwrap();
        app.world_mut().resource_mut::<Game>().dying.push(DeadBody {
            id: ActorTarget::from_digest([4; 32]),
            entity,
            cell: IVec3::new(1, 0, 0),
            appearance_seed: Some(AppearanceSeed::from_digest([19; 32])),
            age: 0.0,
            holds: false,
        });
        app.world_mut()
            .resource_mut::<Fixture>()
            .0
            .items
            .push(tor_client_3d_model::GroundItem {
                model: ItemModel::Corpse,
                corpse_model: CreatureModel::Placeholder,
                corpse_seed: Some(AppearanceSeed::from_digest([seed; 32])),
                id: ItemTarget::from_digest([8; 32]),
                name: "first name corpse".into(),
                class: ItemClass::Corpse,
                quantity: 1,
                description: String::new(),
                position: Position {
                    x: 1,
                    y: 0,
                    z: height,
                },
                reachable: false,
            });
        app.update();
        assert_eq!(
            app.world().resource::<Game>().dying[0].holds,
            expected,
            "height={height}, seed={seed}"
        );
    }
}

fn app(model: CreatureModel) -> App {
    let actor = Actor {
        appearance_seed: None,
        id: ActorTarget::from_digest([4; 32]),
        model,
        name: "first name".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
        hostile: false,
        injury: None,
        is_self: false,
    };
    let cell = CellView {
        lit: true,
        key: "occupied".into(),
        position: actor.position,
        wall: false,
        material: String::new(),
        trap: None,
        door: None,
        asset: None,
        stairs_up: false,
        stairs_down: false,
        place_hint: false,
    };
    let mut view = crate::test_support::view(0);
    view.visible = vec![cell];
    view.actors = vec![actor];
    let mut materials = Assets::<StandardMaterial>::default();
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    let mut app = App::new();
    app.init_resource::<Game>()
        .init_resource::<interact::Pursuit>()
        .init_resource::<interact::Panel>()
        .init_resource::<interact::Hover>()
        .insert_resource(Fixture(view))
        .insert_resource(library)
        .insert_resource(materials)
        .init_resource::<Assets<Mesh>>()
        .insert_resource(Palette {
            ring: default(),
            cave_mat: default(),
            crystal_mat: default(),
            hostile_ring: default(),
        })
        .add_systems(Update, project);
    app
}

#[test]
fn observer_role_and_interaction_target_do_not_change_the_rendered_appearance() {
    let rendered = |own, target, variation| {
        let mut app = app(CreatureModel::Equine);
        {
            let mut fixture = app.world_mut().resource_mut::<Fixture>();
            let actor = &mut fixture.0.actors[0];
            actor.id = ActorTarget::from_digest([target; 32]);
            actor.is_self = own;
            actor.appearance_seed =
                Some(tor_protocol::AppearanceSeed::from_digest([variation; 32]));
        }
        app.update();
        let entity = *app
            .world()
            .resource::<Game>()
            .actors
            .values()
            .next()
            .unwrap();
        app.world()
            .get::<ActorBody>(entity)
            .unwrap()
            .appearance_key
            .clone()
    };
    let own = rendered(true, 1, 19);
    assert_eq!(own, rendered(false, 2, 19));
    assert_ne!(
        own,
        rendered(false, 2, 37),
        "positive control: distinct individuals have procedural variation"
    );
}

#[test]
fn repeated_body_cells_create_one_rig_and_one_label() {
    let mut app = app(CreatureModel::Placeholder);
    let mut upper = app.world().resource::<Fixture>().0.actors[0].clone();
    upper.position.z += 1;
    app.world_mut()
        .resource_mut::<Fixture>()
        .0
        .actors
        .push(upper);
    app.update();
    let game = app.world().resource::<Game>();
    assert_eq!(game.actors.len(), 1);
    assert_eq!(
        game.labels.len(),
        1,
        "deferred body creation must not leak a second label"
    );
}

#[test]
fn a_cached_rig_does_not_jump_to_a_later_alias_in_the_same_view() {
    let mut app = app(CreatureModel::Placeholder);
    app.update();
    let entity = *app
        .world()
        .resource::<Game>()
        .actors
        .values()
        .next()
        .unwrap();
    let expected = app.world().get::<ActorBody>(entity).unwrap().goal;
    let mut echo = app.world().resource::<Fixture>().0.actors[0].clone();
    echo.position.x += 5;
    app.world_mut()
        .resource_mut::<Fixture>()
        .0
        .actors
        .push(echo);
    app.update();
    assert_eq!(
        app.world().get::<ActorBody>(entity).unwrap().goal,
        expected,
        "a repeated target must not overwrite its primary rig later in the same reconciliation"
    );
}

#[test]
fn disclosed_renaming_updates_the_existing_label() {
    let mut app = app(CreatureModel::Placeholder);
    app.update();
    app.world_mut().resource_mut::<Fixture>().0.actors[0].name = "new name".into();
    app.update();
    let actor = *app
        .world()
        .resource::<Game>()
        .actors
        .values()
        .next()
        .unwrap();
    let body = app.world().get::<ActorBody>(actor).unwrap();
    assert_eq!(body.name, "new name");
    assert_eq!(app.world().get::<Text>(body.label).unwrap().0, "new name");
}

#[test]
fn hostility_changes_refresh_the_label_cue_as_well_as_the_ring() {
    let mut app = app(CreatureModel::Placeholder);
    app.update();
    app.world_mut().resource_mut::<Fixture>().0.actors[0].hostile = true;
    app.update();
    let actor = *app
        .world()
        .resource::<Game>()
        .actors
        .values()
        .next()
        .unwrap();
    let body = app.world().get::<ActorBody>(actor).unwrap();
    assert_eq!(
        app.world().get::<TextColor>(body.label).unwrap().0,
        Color::srgb(1.0, 0.55, 0.5)
    );
}

#[test]
fn a_removed_body_does_not_leave_a_stale_hand_entry() {
    let mut app = app(CreatureModel::Humanoid);
    app.update();
    assert_eq!(app.world().resource::<Game>().hands.len(), 1);
    app.world_mut().resource_mut::<Fixture>().0.actors.clear();
    app.update();
    assert!(
        app.world().resource::<Game>().hands.is_empty(),
        "removed rigs cannot retain hand references"
    );
}

#[test]
fn a_body_is_not_lowered_into_unseen_space_beneath_its_disclosed_cell() {
    let mut app = app(CreatureModel::Placeholder);
    let position = Position { x: 1, y: 0, z: 5 };
    {
        let mut view = app.world_mut().resource_mut::<Fixture>();
        view.0.actors[0].position = position;
        view.0.visible[0].position = position;
    }
    app.update();
    let actor = *app
        .world()
        .resource::<Game>()
        .actors
        .values()
        .next()
        .unwrap();
    assert_eq!(
        app.world().get::<ActorBody>(actor).unwrap().goal,
        stand(ivec(position))
    );
}

fn project_terrain(
    mut commands: Commands,
    mut game: ResMut<Game>,
    palette: Res<Palette>,
    mut library: ResMut<PropLibrary>,
    mut meshes: ResMut<Assets<Mesh>>,
    view: Res<Fixture>,
) {
    build_cells(
        &mut commands,
        &mut game,
        &palette,
        &mut library,
        &mut meshes,
        &view.0,
    );
}

#[test]
fn remembered_terrain_builds_dim_geometry_without_decorative_lighting() {
    use tor_protocol::RememberedMapCell;
    let mut app = app(CreatureModel::Placeholder);
    app.add_systems(Update, project_terrain.after(project));
    app.update();
    assert!(!app
        .world()
        .resource::<Game>()
        .cave
        .solids
        .contains_key(&IVec3::new(2, 0, 2)));
    let wall = RememberedMapCell {
        key: "remembered-wall".into(),
        position: Position { x: 2, y: 2, z: 0 },
        wall: true,
        material: "stone".into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: vec![],
    };
    let floor = RememberedMapCell {
        key: "remembered-floor".into(),
        wall: false,
        position: Position { x: 3, y: 2, z: 0 },
        ..wall.clone()
    };
    app.world_mut().resource_mut::<Fixture>().0.memory = vec![wall, floor];
    app.update();
    let game = app.world().resource::<Game>();
    let solid = &game.cave.solids[&IVec3::new(2, 0, 2)];
    assert!(!solid.seen && !solid.lit);
    assert!(game.cave.open.contains(&IVec3::new(3, 0, 2)));
    assert!(!game.cave.lit_open.contains(&IVec3::new(3, 0, 2)));
    assert!(
        !game.chunks.is_empty(),
        "remembered terrain must create renderable chunks"
    );
}
