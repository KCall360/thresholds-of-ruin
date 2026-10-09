use std::sync::Arc;
use tor_client_ascii::{render, App, Effect, Input};
use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(hidden: bool) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":if hidden {"1"} else {"0"}},"actor":"1","branch":"map","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":if hidden {"1"} else {"0"},"observation":{
            "actor":"1","self_target":super::actor_target(1),"tick":"0","position":{"x":0,"y":0,"z":0},
            "places":[],"visible_cells":(0..if hidden {1} else {4}).map(|x|serde_json::json!({
                "key":x.to_string(),"position":{"x":x,"y":0,"z":0},"wall":x==3,
                "stairs_up":x==2,"stairs_down":false,"place_hint":false
            })).collect::<Vec<_>>(),
            "ground_items":if hidden {vec![]} else {vec![serde_json::json!({"item":{"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(1),"name":"token"},"position":{"x":1,"y":0,"z":0},"reachable":false})]},
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
    assert_eq!(tor_client_ascii::glyph_at(observation, 1, 0), ')');
    let mut state = ClientState::from_snapshot(snapshot).unwrap();
    assert_eq!(
        render::map_tiles(&state)
            .iter()
            .find(|t| t.position.x == 1)
            .unwrap()
            .glyph,
        ')'
    );
    state.replace_snapshot(self::snapshot(true)).unwrap();
    let tiles = render::map_tiles(&state);
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
        assert_eq!(tor_client_ascii::glyph_at(observation, 1, 0), glyph);
        let state = ClientState::from_snapshot(snapshot).unwrap();
        let tiles = render::map_tiles(&state);
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
    let tiles = render::map_tiles(&state);
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
        app.input(Input::Click {
            x: tile.center.0,
            y: tile.center.1
        }),
        Effect::Request(tor_protocol::Request::Command {
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
fn remembered_elevations_and_large_maps_fit_inside_the_map_panel() {
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
    let tiles = render::map_tiles(&state);
    assert!(tiles.iter().any(|t| t.remembered && t.position.z != 0));
    assert!(tiles
        .iter()
        .all(|t| (44..748).contains(&t.center.0) && (166..458).contains(&t.center.1)));
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
fn remembered_mouse_travel_uses_the_displayed_height_slice() {
    let mut initial = snapshot(false);
    Arc::make_mut(&mut initial.state).observation.visible_cells[1]
        .position
        .z = 1;
    let mut state = ClientState::from_snapshot(initial).unwrap();
    state.replace_snapshot(snapshot(true)).unwrap();
    let tile = render::map_tiles_at_level(&state, 1)
        .into_iter()
        .find(|t| t.position.x == 1)
        .unwrap();
    assert!(tile.remembered);
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.map_level = 1;
    app.set_state(state);
    app.ready();
    assert!(
        matches!(app.input(Input::Click {x:tile.center.0,y:tile.center.1}),
        Effect::Request(Request::Command {command:Command::Travel {destination,..},..}) if destination=="1")
    );
}
