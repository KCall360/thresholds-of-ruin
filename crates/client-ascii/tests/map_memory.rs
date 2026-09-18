use tor_client_ascii::{render, App, Effect, Input};
use tor_client_common::ClientState;
use tor_protocol::*;

fn snapshot(hidden: bool) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"map","cursor":{"sequence":0,"tick":0},"has_control":true,
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":if hidden {1} else {0},"observation":{
            "actor":1,"tick":0,"position":{"x":0,"y":0,"z":0},
            "visible_cells":(0..if hidden {1} else {4}).map(|x|serde_json::json!({
                "key":x.to_string(),"position":{"x":x,"y":0,"z":0},"wall":x==3,
                "stairs_up":x==2,"stairs_down":false,"place_hint":false
            })).collect::<Vec<_>>(),
            "ground_items":if hidden {vec![]} else {vec![serde_json::json!({"item":{"id":1,"name":"token"},"position":{"x":1,"y":0,"z":0},"reachable":false})]},
            "visible_actors":if hidden {vec![]} else {vec![serde_json::json!({"id":2,"position":{"x":2,"y":0,"z":0}})]},
            "inventory":[],"ready":true
        }}
    })).unwrap()
}

#[test]
fn hidden_terrain_and_items_are_grey_actors_disappear_and_clicks_use_visible_cells_only() {
    let mut state = ClientState::from_snapshot(snapshot(false)).unwrap();
    state.replace_snapshot(snapshot(true)).unwrap();
    let tiles = render::map_tiles(&state);
    for (x, glyph) in [(0, '@'), (1, '!'), (2, '<'), (3, '#')] {
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
        Effect::None
    );
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
    for z in -8..=8 {
        for x in -24..=24 {
            for y in -12..=12 {
                let mut cell = template.clone();
                cell.key = format!("{x}:{y}:{z}");
                cell.position = Position { x, y, z };
                if cell.position != (Position { x: 0, y: 0, z: 0 }) {
                    first.state.observation.visible_cells.push(cell);
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
