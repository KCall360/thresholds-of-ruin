use tor_client_3d_model::{view, CreatureModel, ItemModel};
use tor_client_common::{test_support::observation, ClientState, Palette};
use tor_protocol::{ItemClass, ItemTarget, ItemView, Snapshot, StreamContext, StreamId};

#[test]
fn upper_body_and_portal_images_do_not_replace_the_players_known_foot_position() {
    use tor_protocol::{AppearanceSeed, CreatureAppearance, Position};
    let mut observed = observation();
    let seed = AppearanceSeed::from_digest([27; 32]);
    observed.self_appearance = Some(CreatureAppearance { seed, asset: None });
    let mut upper = observed.visible_actors[0].clone();
    upper.id = observed.self_target;
    upper.appearance_seed = Some(seed);
    upper.asset = None;
    upper.position = Position {
        z: observed.position.z + 1,
        ..observed.position
    };
    let mut echo = upper.clone();
    echo.position = Position {
        x: observed.position.x + 5,
        ..observed.position
    };
    observed.visible_actors = vec![upper, echo];
    let feet = observed.position;
    let projected = view(&from_observation(observed), &Palette::default());
    let own = projected
        .actors
        .iter()
        .find(|actor| actor.is_self && actor.position == feet)
        .expect("own location remains known when an upper cell or portal image is visible");
    assert_eq!(own.appearance_seed, Some(seed));
    assert_eq!(
        projected.actors[0].position, feet,
        "the primary representation must precede its aliases"
    );
}

pub(super) fn state() -> ClientState {
    let mut observed = observation();
    observed.visible_actors[0].name = "unfamiliar creature".into();
    observed.visible_actors[0].description = "A shape you cannot identify.".into();
    observed.visible_actors[0].asset = None;
    observed.inventory.push(ItemView {
        corpse: None,
        id: ItemTarget::from_digest([11; 32]),
        name: "unfamiliar object".into(),
        description: "Wrapped in plain paper.".into(),
        class: ItemClass::Misc,
        quantity: 2,
        appearance: "wrapped object".into(),
        identified: false,
        asset: None,
        known_currency: None,
        known_supply: None,
    });
    from_observation(observed)
}

fn from_observation(observed: tor_protocol::Observation) -> ClientState {
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness": {"revision": "0", "admission": false, "resume": [], "cancel": []},
        "context": StreamContext { stream: StreamId("projection".into()), epoch: 0 },
        "actor": observed.actor, "branch": "play", "cursor": {"sequence": "0", "tick": observed.tick.to_string()},
        "has_control": true, "intentions": [], "history": {"entries": [], "older_before": null},
        "state": {"wizard_game": false, "revision": "0", "observation": observed},
    })).unwrap();
    ClientState::from_snapshot(snapshot).unwrap()
}

#[test]
fn own_body_and_corpses_use_disclosed_appearance_without_parsing_names() {
    use tor_protocol::{
        AppearanceSeed, CreatureAppearance, GroundItemView, PaletteBody, PaletteUpdate,
    };
    let appearance = CreatureAppearance {
        seed: AppearanceSeed::from_digest([23; 32]),
        asset: Some("creature.equine.authored".into()),
    };
    let mut observed = observation();
    observed.visible_actors.clear();
    observed.self_appearance = Some(appearance.clone());
    observed.ground_items.clear();
    observed.ground_items.push(GroundItemView {
        item: ItemView {
            corpse: Some(appearance.clone()),
            class: ItemClass::Corpse,
            quantity: 1,
            id: ItemTarget::from_digest([13; 32]),
            name: "misleading human corpse".into(),
            description: String::new(),
            appearance: "remains".into(),
            identified: true,
            asset: Some("item.corpse".into()),
            known_currency: None,
            known_supply: None,
        },
        reachable: true,
        supply_reachable: false,
        position: observed.position,
    });
    let mut palette = Palette::default();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full {
            assets: ["creature.equine.authored".into(), "item.corpse".into()]
                .into_iter()
                .collect(),
        },
    });
    let projected = view(&from_observation(observed), &palette);
    let own = projected.actors.iter().find(|actor| actor.is_self).unwrap();
    assert_eq!(own.model, CreatureModel::Equine);
    assert_eq!(own.appearance_seed, Some(appearance.seed));
    assert_eq!(projected.items[0].corpse_model, own.model);
    assert_eq!(projected.items[0].corpse_seed, own.appearance_seed);
}

#[test]
fn unknown_models_keep_disclosed_targets_text_and_inventory_actions_available() {
    let state = state();
    let source = &state.state().observation;
    let projected = view(&state, &Palette::default());
    assert_eq!(projected.shown, state.reply_context());
    let source_actor = &source.visible_actors[0];
    let actor = projected
        .actors
        .iter()
        .find(|actor| actor.id == source_actor.id)
        .unwrap();
    assert_eq!(actor.model, CreatureModel::Placeholder);
    assert_eq!(actor.position, source_actor.position);
    assert_eq!(actor.name, source_actor.name);
    assert_eq!(actor.description, source_actor.description);
    let source_item = source.inventory.last().unwrap();
    let item = projected
        .inventory
        .iter()
        .find(|item| item.id == source_item.id)
        .unwrap();
    assert_eq!(item.model, ItemModel::Placeholder);
    assert_eq!(item.name, source_item.name);
    assert_eq!(item.description, source_item.description);
    assert_eq!(item.quantity, 2);
    assert!(
        projected.actors.iter().any(|actor| actor.is_self),
        "a missing self model cannot remove the player"
    );
}

#[test]
fn hydrated_terrain_is_dim_and_never_grants_live_occupants_or_door_controls() {
    use tor_protocol::*;
    let mut client = state();
    let query = client.next_map_query().unwrap();
    client.bind_map_query(&query, "map").unwrap();
    let remembered = RememberedMapCell {
        key: "remembered-only".into(),
        position: Position { x: 12, y: 4, z: 0 },
        wall: false,
        material: "granite".into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: true,
        trap: None,
        door: Some(MapDoorAppearance {
            name: "old door".into(),
            description: "stale".into(),
            open: false,
            asset: None,
        }),
        ground_items: vec![],
    };
    let page = MapPage {
        transfer: "transfer".into(),
        actor: client.snapshot().actor,
        branch: client.branch().clone(),
        base: client.observation_base(),
        window: query.window,
        start: 0,
        page: 0,
        total_cells: 1,
        complete: true,
        ambiguous: false,
        limited: false,
        cells: vec![remembered.clone()],
        next: None,
    };
    client
        .accept_map_page(&client.reply_context(), "map", &page, 1000)
        .unwrap();
    let projected = view(&client, &Palette::default());
    assert_eq!(projected.memory, vec![remembered]);
    let terrain = projected
        .terrain()
        .find(|c| c.key == "remembered-only")
        .unwrap();
    assert!(!terrain.seen);
    assert!(!terrain.lit);
    assert_eq!(terrain.door, Some(false));
    assert!(terrain.stairs_down);
    assert!(projected
        .actors
        .iter()
        .all(|a| a.position != terrain.position));
    assert!(projected
        .items
        .iter()
        .all(|i| i.position != terrain.position));
    assert!(!matches!(
        tor_client_3d_model::describe(&projected, terrain.position).2,
        Some(tor_client_3d_model::Intent::Door(..))
    ));
}

#[test]
fn visible_terrain_uses_explicit_illumination_instead_of_visibility() {
    let mut observed = observation();
    let dark = tor_protocol::CellView {
        lit: false,
        key: "dark".into(),
        position: observed.position,
        wall: false,
        stairs_up: false,
        stairs_down: false,
        material: "stone".into(),
        place_hint: false,
        asset: None,
        trap: None,
        door: None,
    };
    let bright = tor_protocol::CellView {
        lit: true,
        key: "bright".into(),
        position: tor_protocol::Position {
            x: observed.position.x + 1,
            ..observed.position
        },
        ..dark.clone()
    };
    observed.visible_cells = vec![dark, bright];
    let projected = view(&from_observation(observed), &Palette::default());
    let cells: Vec<_> = projected.terrain().collect();
    assert!(cells[0].seen && !cells[0].lit);
    assert!(cells[1].seen && cells[1].lit);
}
