use tor_client_ascii::{render, App, Effect, Input};
use tor_protocol::*;

fn snapshot(hidden: bool) -> Snapshot {
    serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"map","cursor":{"sequence":0,"tick":0},"has_control":true,
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":if hidden {1} else {0},"observation":{
            "actor":1,"tick":0,"position":{"x":0,"y":0,"z":0},
            "places":[],"visible_cells":(0..if hidden {1} else {4}).map(|x|serde_json::json!({
                "key":x.to_string(),"position":{"x":x,"y":0,"z":0},"wall":x==3,
                "stairs_up":x==2,"stairs_down":false,"place_hint":false
            })).collect::<Vec<_>>(),
            "ground_items":if hidden {vec![]} else {vec![serde_json::json!({"item":{"quantity":1,"appearance":"item","identified":true,"id":1,"name":"token"},"position":{"x":1,"y":0,"z":0},"reachable":false})]},
            "visible_actors":if hidden {vec![]} else {vec![serde_json::json!({"id":2,"position":{"x":2,"y":0,"z":0}})]},
            "inventory":[],"ready":true
        }}
    })).unwrap()
}

#[test]
fn hidden_terrain_and_items_are_grey_actors_disappear_and_clicks_use_visible_cells_only() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.replace_snapshot(snapshot(false)).unwrap();
    let before = render::map_tiles(&app);
    let origin = before
        .iter()
        .find(|tile| tile.position.x == 0)
        .unwrap()
        .center;
    app.replace_snapshot(snapshot(true)).unwrap();
    let tiles = render::map_tiles(&app);
    assert_eq!(
        tiles
            .iter()
            .find(|tile| tile.position.x == 0)
            .unwrap()
            .center,
        origin
    );
    for (x, glyph) in [(0, '@'), (1, '.'), (2, '<'), (3, '#')] {
        let tile = tiles.iter().find(|tile| tile.position.x == x).unwrap();
        assert_eq!(tile.glyph, glyph);
        assert_eq!(tile.position.z, 0);
        assert_eq!(tile.remembered, x != 0);
        if x != 0 {
            assert_eq!(tile.color, render::MEMORY_COLOR);
        }
    }
    app.ready();
    let grey = tiles.iter().find(|tile| tile.position.x == 1).unwrap();
    assert_eq!(
        app.input(Input::Click {
            x: grey.center.0,
            y: grey.center.1
        }),
        Effect::None
    );
    let mut canvas = render::Canvas::default();
    canvas.draw(&app);
    assert!(canvas.pixels.contains(&render::MEMORY_COLOR));
    let here = tiles.iter().find(|tile| tile.position.x == 0).unwrap();
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
    let mut app = App::new();
    app.replace_snapshot(first).unwrap();
    app.replace_snapshot(snapshot(true)).unwrap();
    let tiles = render::map_tiles(&app);
    let state = app.state.as_ref().unwrap();
    let standing: std::collections::BTreeSet<_> = state
        .map_memory()
        .filter(|cell| cell.position.z == 0)
        .map(|cell| (cell.position.x, cell.position.y))
        .chain(
            state
                .state()
                .observation
                .visible_cells
                .iter()
                .filter(|cell| cell.position.z == 0)
                .map(|cell| (cell.position.x, cell.position.y)),
        )
        .collect();
    assert!(tiles.iter().all(|tile| tile.position.z == 0));
    assert_eq!(
        tiles
            .iter()
            .map(|tile| (tile.position.x, tile.position.y))
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        tiles.len()
    );
    assert!(tiles
        .iter()
        .all(|tile| standing.contains(&(tile.position.x, tile.position.y))));
    assert!(tiles.iter().all(|tile| {
        let (x, y) = tile.center;
        x < 1200 && (48..768).contains(&y) && x % 16 == 8 && (y - 48) % 16 == 8
    }));
    render::Canvas::default().draw(&app);
}
