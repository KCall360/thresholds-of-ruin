use std::sync::Arc;
use tor_client_ascii::{render, App, Effect, Input};
use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(hidden: bool) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":if hidden {"1"} else {"0"}},"actor":"1","branch":"map","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":if hidden {"1"} else {"0"},"observation":{"linked_holds":[],
            "actor":"1","self_target":super::actor_target(1),"tick":"0","position":{"x":0,"y":0,"z":0},
            "places":[],"visible_cells":(0..if hidden {1} else {4}).map(|x|serde_json::json!({
                "key":x.to_string(),"position":{"x":x,"y":0,"z":0},"wall":x==3,
                "trap":null,"object_presentation":null,"stairs_up":x==2,"stairs_down":false,"lit":true,"place_hint":false
            })).collect::<Vec<_>>(),
            "ground_items":if hidden {vec![]} else {vec![serde_json::json!({"presentation":null,"supply_reachable":false,"item":{"known_currency":null,"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(1),"name":"token"},"position":{"x":1,"y":0,"z":0},"reachable":false})]},
            "visible_actors":if hidden {vec![]} else {vec![serde_json::json!({"id":super::actor_target(2),"position":{"x":2,"y":0,"z":0}})]},
            "inventory":[],"ready":true
        }}
    })).unwrap()
}

#[test]
fn item_classes_use_disclosed_pile_order_and_keep_their_remembered_glyph() {
    let mut snapshot = snapshot(false);
    let observation = &mut Arc::make_mut(&mut snapshot.state).observation;
    let mut second = observation.ground_items[0].clone();
    observation.ground_items[0].item.class = ItemClass::Weapon;
    second.item.id = super::item_target(2);
    second.item.class = ItemClass::Potion;
    observation.ground_items.push(second);
    assert_eq!(
        tor_client_ascii::glyph_at(observation, &Default::default(), 1, 0),
        ')'
    );
    let mut state = ClientState::from_snapshot(snapshot).unwrap();
    assert_eq!(
        render::map_tiles(&state, &Default::default())
            .iter()
            .find(|t| t.position.x == 1)
            .unwrap()
            .glyph,
        ')'
    );
    state.replace_snapshot(self::snapshot(true)).unwrap();
    let tiles = render::map_tiles(&state, &Default::default());
    let tile = tiles.iter().find(|t| t.position.x == 1).unwrap();
    assert_eq!(tile.glyph, ')');
    assert!(tile.remembered);
    assert_eq!(tile.color, render::MEMORY_COLOR);
}

#[test]
fn every_physical_class_draws_its_symbol_through_the_map_renderer() {
    for (class, glyph) in [
        (ItemClass::Misc, '('),
        (ItemClass::Tool, '('),
        (ItemClass::Weapon, ')'),
        (ItemClass::Armor, '['),
        (ItemClass::Potion, '!'),
        (ItemClass::Food, '%'),
        (ItemClass::Corpse, '%'),
        (ItemClass::Amulet, '"'),
        (ItemClass::Ring, '='),
        (ItemClass::Scroll, '?'),
        (ItemClass::Spellbook, '+'),
        (ItemClass::Wand, '/'),
        (ItemClass::Coin, '$'),
        (ItemClass::Gem, '*'),
    ] {
        let mut snapshot = snapshot(false);
        let observation = &mut Arc::make_mut(&mut snapshot.state).observation;
        observation.ground_items[0].item.class = class;
        assert_eq!(
            tor_client_ascii::glyph_at(observation, &Default::default(), 1, 0),
            glyph
        );
        let state = ClientState::from_snapshot(snapshot).unwrap();
        let tiles = render::map_tiles(&state, &Default::default());
        assert_eq!(
            tiles.iter().find(|t| t.position.x == 1).unwrap().glyph,
            glyph
        );
    }
}

#[test]
fn hidden_terrain_and_items_are_grey_actors_disappear_and_clicks_use_known_cells() {
    let mut state = ClientState::from_snapshot(snapshot(false)).unwrap();
    state.replace_snapshot(snapshot(true)).unwrap();
    let tiles = render::map_tiles(&state, &Default::default());
    for (x, glyph) in [(0, '@'), (1, '('), (2, '<'), (3, '#')] {
        let tile = tiles.iter().find(|t| t.position.x == x).unwrap();
        assert_eq!(tile.glyph, glyph);
        assert_eq!(tile.remembered, x != 0);
        if x != 0 {
            assert_eq!(tile.color, render::MEMORY_COLOR);
        }
    }
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state);
    app.ready();
    let tile = &tiles[1];
    assert_eq!(
        super::request(app.input(Input::Click {
            x: tile.center.0,
            y: tile.center.1
        })),
        Some(tor_protocol::Request::Command {
            context: app.state.as_ref().unwrap().input_context(),
            branch: app.state.as_ref().unwrap().branch().clone(),
            command: tor_protocol::Command::Travel {
                expected_revision: 1,
                destination: "1".into()
            }
        })
    );
    app.ready();
    let mut canvas = render::Canvas::default();
    canvas.draw(&app);
    assert!(canvas.pixels.contains(&render::MEMORY_COLOR));
    let here = &tiles[0];
    assert!(matches!(
        app.input(Input::Click {
            x: here.center.0,
            y: here.center.1
        }),
        Effect::Request(_)
    ));
}

#[test]
fn remembered_heights_and_large_maps_collapse_into_the_single_map() {
    let mut first = snapshot(false);
    let template = first.state.observation.visible_cells[0].clone();
    let existing: std::collections::BTreeSet<_> = first
        .state
        .observation
        .visible_cells
        .iter()
        .map(|cell| cell.position)
        .collect();
    for z in -8..=8 {
        for x in -24..=24 {
            for y in -12..=12 {
                let mut cell = template.clone();
                cell.key = format!("{x}:{y}:{z}");
                cell.position = Position { x, y, z };
                if !existing.contains(&cell.position) {
                    Arc::make_mut(&mut first.state)
                        .observation
                        .visible_cells
                        .push(cell);
                }
            }
        }
    }
    let mut state = ClientState::from_snapshot(first).unwrap();
    state.replace_snapshot(snapshot(true)).unwrap();
    let tiles = render::map_tiles(&state, &Default::default());
    // Every height collapses into one map: one tile per column, all of them
    // inside the map area.
    let columns: std::collections::BTreeSet<_> =
        tiles.iter().map(|t| (t.position.x, t.position.y)).collect();
    assert_eq!(columns.len(), tiles.len());
    assert!(tiles.iter().any(|t| t.remembered));
    let (left, top) = (tor_client_ascii::map::LEFT, tor_client_ascii::map::TOP);
    let right = left + tor_client_ascii::map::COLS as usize * tor_client_ascii::map::STEP;
    let bottom = top + tor_client_ascii::map::ROWS as usize * tor_client_ascii::map::STEP;
    assert!(tiles
        .iter()
        .all(|t| (left..right).contains(&t.center.0) && (top..bottom).contains(&t.center.1)));
    let mut app = App::new();
    app.set_state(state);
    render::Canvas::default().draw(&app);
}

#[test]
fn remembered_travel_rejects_known_obstacles_unknown_slices_and_spectators() {
    use tor_client_ascii::Key;
    for kind in ["wall", "closed", "unknown", "other-slice", "spectator"] {
        let mut initial = snapshot(false);
        let cell = &mut Arc::make_mut(&mut initial.state).observation.visible_cells[1];
        if kind == "wall" {
            cell.wall = true;
        }
        if kind == "closed" {
            cell.door = Some(
                serde_json::from_value(
                    serde_json::json!({"id":tor_protocol::DoorTarget::from_digest([1;32]),"name":"door","description":"","open":false,"reachable":false,"approaches":[]}),
                )
                .unwrap(),
            );
        }
        if kind == "other-slice" {
            cell.position.z = 1;
        }
        let mut state = ClientState::from_snapshot(initial).unwrap();
        state.replace_snapshot(snapshot(true)).unwrap();
        let mut app = App::new();
        app.role = if kind == "spectator" {
            AccessRole::Spectator
        } else {
            AccessRole::Player
        };
        app.set_state(state);
        app.ready();
        app.input(Input::Key { key: Key::Travel });
        for _ in 0..if kind == "unknown" { 8 } else { 1 } {
            app.input(Input::Key { key: Key::Right });
        }
        assert_eq!(
            app.input(Input::Key { key: Key::Enter }),
            Effect::None,
            "{kind}"
        );
    }
}

#[test]
fn a_column_known_only_at_head_height_is_drawn_but_not_travelled_to() {
    let mut initial = snapshot(false);
    Arc::make_mut(&mut initial.state).observation.visible_cells[1]
        .position
        .z = 1;
    let mut state = ClientState::from_snapshot(initial).unwrap();
    state.replace_snapshot(snapshot(true)).unwrap();
    let tile = render::map_tiles(&state, &Default::default())
        .into_iter()
        .find(|t| t.position.x == 1)
        .unwrap();
    assert!(tile.remembered);
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state);
    app.ready();
    assert_eq!(
        app.input(Input::Click {
            x: tile.center.0,
            y: tile.center.1
        }),
        Effect::None
    );
}

#[test]
fn palette_tints_terrain_without_replacing_geometry_state_or_memory_cues() {
    use tor_client_ascii::map;
    use tor_client_common::{presentation, Palette};
    let mut initial = snapshot(false);
    let o = &mut Arc::make_mut(&mut initial.state).observation;
    o.visible_actors.clear();
    o.ground_items.clear();
    let template = o.visible_cells[0].clone();
    o.visible_cells.clear();
    for x in 0..=8 {
        for z in -1..=1 {
            let mut cell = template.clone();
            cell.key = format!("{x}:{z}");
            cell.position = Position { x, y: 0, z };
            cell.wall = z == -1 || x == 2 || (x == 5 && z == 0);
            if x == 6 && z == -1 {
                cell.wall = false;
            }
            cell.asset = Some(
                if z == -1 {
                    "terrain.floor.cave"
                } else {
                    "terrain.wall.marble"
                }
                .into(),
            );
            cell.stairs_up = x == 7 && z == 0;
            cell.stairs_down = x == 8 && z == 0;
            if (x == 3 || x == 4) && z == 0 {
                cell.door = Some(
                    serde_json::from_value(serde_json::json!({
                        "id":tor_protocol::DoorTarget::from_digest([x as u8;32]),
                        "name":"oak door","description":"","open":x==4,
                        "reachable":false,"approaches":[],"asset":"terrain.wall.marble"
                    }))
                    .unwrap(),
                );
            }
            o.visible_cells.push(cell);
        }
    }
    let mut palette = Palette::default();
    let assets = ["terrain.floor.cave".into(), "terrain.wall.marble".into()].into();
    palette.apply(&PaletteUpdate {
        revision: 1,
        body: PaletteBody::Full { assets },
    });
    let mut state = ClientState::from_snapshot(initial).unwrap();
    let current = render::map_tiles(&state, &palette);
    let expected = [
        (1, map::Kind::Floor, '.'),
        (2, map::Kind::Wall, '#'),
        (3, map::Kind::Door, '+'),
        (4, map::Kind::Door, '/'),
        (5, map::Kind::LowWall, '#'),
        (6, map::Kind::Drop, '^'),
        (7, map::Kind::StairsUp, '<'),
        (8, map::Kind::StairsDown, '>'),
    ];
    for (x, kind, glyph) in expected {
        let tile = current.iter().find(|tile| tile.position.x == x).unwrap();
        assert_eq!(
            (tile.kind, tile.glyph, tile.remembered),
            (kind, glyph, false)
        );
        if x <= 4 {
            let asset = if x == 1 {
                "terrain.floor.cave"
            } else {
                "terrain.wall.marble"
            };
            assert_eq!(
                tile.color,
                presentation::resolve(&palette, Some(asset))
                    .unwrap()
                    .ascii
                    .color
            );
        }
    }
    palette.apply(&PaletteUpdate {
        revision: 3,
        body: PaletteBody::Delta {
            base: 2,
            added: Default::default(),
            removed: Default::default(),
        },
    });
    let stale = render::map_tiles(&state, &palette);
    let fallback = render::map_tiles(&state, &Palette::default());
    assert_eq!(stale, fallback);
    for (shown, default) in current.iter().zip(&fallback) {
        assert_eq!((shown.kind, shown.glyph), (default.kind, default.glyph));
        if shown.position.x >= 5 {
            assert_eq!(shown.color, default.color);
        }
        if (1..=4).contains(&shown.position.x) {
            assert_ne!(shown.color, default.color);
        }
    }
    palette.apply(&PaletteUpdate {
        revision: 4,
        body: PaletteBody::Full {
            assets: ["terrain.floor.cave".into(), "terrain.wall.marble".into()].into(),
        },
    });
    assert_eq!(render::map_tiles(&state, &palette), current);
    let mut hidden = snapshot(true);
    Arc::make_mut(&mut hidden.state).observation.visible_cells[0].key = "0:0".into();
    state.replace_snapshot(hidden).unwrap();
    let memory = render::map_tiles(&state, &palette);
    for (x, kind, glyph) in expected {
        let tile = memory.iter().find(|tile| tile.position.x == x).unwrap();
        assert_eq!(
            (tile.kind, tile.glyph, tile.remembered, tile.color),
            (kind, glyph, true, render::MEMORY_COLOR)
        );
    }
}

#[test]
fn cold_hydrated_item_and_door_draw_without_action_handles_and_preserve_cell_travel() {
    let mut client = ClientState::from_snapshot(snapshot(true)).unwrap();
    let query = client.next_map_query().unwrap();
    client.bind_map_query(&query, "hydrate").unwrap();
    let cell = |x, key: &str| RememberedMapCell {
        object_presentation: None,
        key: key.into(),
        position: Position { x, y: 0, z: 0 },
        wall: false,
        material: "stone".into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: None,
        ground_items: vec![],
    };
    let mut item = cell(1, "remembered-floor");
    item.ground_items.push(MapItemAppearance {
        presentation: None,
        class: ItemClass::Potion,
        name: "amber bottle".into(),
        description: "A bottle".into(),
        appearance: "amber bottle".into(),
        identified: false,
        quantity: 2,
        asset: None,
    });
    let mut door = cell(2, "remembered-door");
    door.door = Some(MapDoorAppearance {
        name: "oak door".into(),
        description: "Old oak".into(),
        open: false,
        asset: None,
    });
    let page = MapPage {
        transfer: "cold".into(),
        actor: ActorId(1),
        branch: client.branch().clone(),
        base: query.base,
        window: query.window,
        start: 0,
        page: 0,
        total_cells: 2,
        complete: true,
        ambiguous: false,
        limited: false,
        cells: vec![item, door],
        next: None,
    };
    let response = ServerMessage::MapPage {
        context: client.reply_context(),
        request_id: "hydrate".into(),
        page: Box::new(page.clone()),
    };
    let bytes = encoded_length(&response, MAX_MAP_PAGE_BYTES)
        .unwrap()
        .unwrap();
    client
        .accept_map_page(&client.reply_context(), "hydrate", &page, bytes)
        .unwrap();
    assert!(client.state().observation.ground_items.is_empty());
    assert!(client
        .state()
        .observation
        .visible_cells
        .iter()
        .all(|cell| cell.door.is_none()));
    let mut presented = ClientState::from_snapshot(snapshot(true)).unwrap();
    presented
        .apply_map_memory_update(client.map_memory_update().unwrap())
        .unwrap();
    let tiles = render::map_tiles(&presented, &Default::default());
    let bottle = tiles.iter().find(|tile| tile.position.x == 1).unwrap();
    assert_eq!(bottle.glyph, '!');
    assert!(bottle.remembered);
    assert_eq!(bottle.color, render::MEMORY_COLOR);
    let door = tiles.iter().find(|tile| tile.position.x == 2).unwrap();
    assert_eq!(door.glyph, '+');
    assert!(door.remembered);
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(presented);
    app.ready();
    let request = super::request(app.input(Input::Click {
        x: bottle.center.0,
        y: bottle.center.1,
    }))
    .unwrap();
    assert!(
        matches!(request,Request::Command {command:Command::Travel {destination,..},..} if destination == "remembered-floor")
    );
}

#[test]
fn current_pile_wins_over_a_lower_remembered_pile_in_the_same_column() {
    let mut first = snapshot(false);
    let view = &mut Arc::make_mut(&mut first.state).observation;
    view.visible_cells[1].position.z = -2;
    view.ground_items[0].position.z = -2;
    view.ground_items[0].item.class = ItemClass::Potion;
    let mut state = ClientState::from_snapshot(first).unwrap();
    let mut next = snapshot(false);
    next.context.epoch = 1;
    let view = &mut Arc::make_mut(&mut next.state).observation;
    view.ground_items[0].item.class = ItemClass::Weapon;
    // Keep a unique stationary anchor; the foot cell has another physical key.
    view.visible_cells[1].key = "upper-floor".into();
    state.replace_snapshot(next).unwrap();
    let tiles = render::map_tiles(&state, &Default::default());
    assert_eq!(tiles.iter().find(|t| t.position.x == 1).unwrap().glyph, ')');
}

#[test]
fn remembered_trap_draws_grey_but_selected_look_cannot_examine_it() {
    let mut initial = snapshot(false);
    let o = &mut Arc::make_mut(&mut initial.state).observation;
    o.ground_items.clear();
    o.visible_cells[1].trap = Some(TrapView {
        id: TrapTarget::from_digest([64; 32]),
        family: None,
        reachable: true,
    });
    let mut state = ClientState::from_snapshot(initial).unwrap();
    state.replace_snapshot(snapshot(true)).unwrap();
    let tiles = render::map_tiles(&state, &Default::default());
    let tile = tiles.iter().find(|tile| tile.position.x == 1).unwrap();
    assert_eq!(tile.kind, tor_client_ascii::map::Kind::Trap);
    assert_eq!(tile.color, render::MEMORY_COLOR);
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state);
    app.ready();
    app.look_cursor = Some(Position { x: 1, y: 0, z: 0 });
    assert_eq!(
        app.input(Input::Key {
            key: tor_client_ascii::Key::Enter
        }),
        Effect::None
    );
}
