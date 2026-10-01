use tor_client_hack::{column_glyph, resolve, Glyph, Structure};
use tor_protocol::*;

fn structural(structure: Structure) -> Glyph {
    resolve(None, structure)
}

#[test]
fn cave_floor_then_each_shorter_prefix_then_the_structural_glyph() {
    let cave = Glyph {
        ch: '.',
        color: 0x6E7A55,
        word: "cave floor",
    };
    assert_eq!(resolve(Some("terrain.floor.cave"), Structure::Floor), cave);
    assert_eq!(
        resolve(Some("terrain.floor.cave.damp"), Structure::Floor),
        cave
    );

    let floor = structural(Structure::Floor);
    assert_eq!(floor.ch, '.');
    assert_eq!(floor.word, "floor");
    assert_eq!(
        resolve(Some("terrain.floor.other"), Structure::Floor),
        floor
    );
    assert_eq!(resolve(Some("terrain.other"), Structure::Floor), floor);
    assert_eq!(resolve(Some("terrain.floor.cav"), Structure::Floor), floor);
}

#[test]
fn a_one_segment_id_is_looked_up_as_itself() {
    let item = structural(Structure::Item);
    assert_eq!(resolve(Some("coin"), Structure::Item), item);
    assert_eq!(item.ch, '!');
    assert_ne!(item.color, 0xE6C34A);
    assert_eq!(resolve(Some("item"), Structure::Item), item);
}

#[test]
fn a_rejected_asset_string_is_not_retained() {
    let wall = structural(Structure::Wall);
    let long = "a".repeat(81);
    let rejected = [
        "",
        "Terrain.floor.cave",
        "terrain.floor.cave.",
        "a..b",
        "foo_bar",
        "a/b",
        "has space",
        "caf\u{e9}",
        "{fmt}",
        "../etc",
        "bad\u{1}id",
        long.as_str(),
    ];
    for bad in rejected {
        let glyph = resolve(Some(bad), Structure::Wall);
        assert_eq!(glyph, wall, "{bad}");
        if !bad.is_empty() {
            assert!(!format!("{glyph:?}").contains(bad), "{bad}");
        }
    }
}

#[test]
fn an_unknown_well_formed_id_uses_the_structural_glyph() {
    let creature = structural(Structure::Actor);
    assert_eq!(creature.ch, '&');
    assert_eq!(creature.word, "creature");
    assert_eq!(resolve(Some("creature.goblin"), Structure::Actor), creature);
    assert_eq!(resolve(Some("creature.delver"), Structure::Actor), creature);

    let mut view = observation(vec![cell("floor", 1, 0, 0, false)], vec![], vec![]);
    view.visible_actors.push(ActorView {
        asset: Some("creature.delver".into()),
        name: String::new(),
        description: String::new(),
        id: ActorId(2),
        position: Position { x: 1, y: 0, z: 0 },
    });
    let drawn = column_glyph(&view, &[], 1, 0).unwrap();
    assert_eq!(drawn.ch, '&');
    assert_eq!(drawn.word, "creature");
    assert_eq!(column_glyph(&view, &[], 0, 0).unwrap().ch, '@');
}

#[test]
fn a_door_asset_cannot_change_the_forced_character() {
    let stone_wall = resolve(Some("terrain.wall.stone"), Structure::DoorClosed);
    assert_eq!(stone_wall.ch, '#');
    assert_eq!(stone_wall.word, "stone wall");
    let stone_floor = resolve(Some("terrain.floor.stone"), Structure::DoorOpen);
    assert_eq!(stone_floor.ch, '.');
    assert_eq!(stone_floor.word, "stone floor");

    for (open, asset, ch, structure) in [
        (false, "terrain.wall.stone", '+', Structure::DoorClosed),
        (true, "terrain.wall.stone", '/', Structure::DoorOpen),
        (false, "terrain.floor.stone", '+', Structure::DoorClosed),
        (true, "terrain.floor.stone", '/', Structure::DoorOpen),
    ] {
        let mut door_cell = cell("door", 1, 0, 0, false);
        door_cell.door = Some(DoorView {
            id: 1,
            name: "door".into(),
            description: String::new(),
            open,
            reachable: true,
            approaches: Vec::new(),
            asset: Some(asset.into()),
        });
        let view = observation(vec![door_cell], vec![], vec![]);
        let drawn = column_glyph(&view, &[], 1, 0).unwrap();
        let fallback = structural(structure);
        assert_eq!(drawn.ch, ch, "{asset} open={open}");
        assert_eq!(drawn.color, fallback.color, "{asset}");
        assert_eq!(drawn.word, fallback.word, "{asset}");
    }
}

#[test]
fn stairs_and_pits_ignore_a_floor_asset_on_the_cell() {
    let misused = resolve(Some("terrain.floor.stone"), Structure::StairsUp);
    assert_eq!(misused.ch, '.');
    assert_eq!(misused.color, 0x9AA7A0);
    assert_eq!(misused.word, "stone floor");
    let pit_row = resolve(Some("terrain.floor.stone"), Structure::Pit);
    assert_eq!(pit_row, misused);

    let mut stairs = cell("stairs", 1, 0, 0, false);
    stairs.stairs_up = true;
    stairs.stairs_down = true;
    stairs.asset = Some("terrain.floor.stone".into());
    let drawn = column_glyph(&observation(vec![stairs], vec![], vec![]), &[], 1, 0).unwrap();
    let up = structural(Structure::StairsUp);
    assert_eq!(drawn.ch, '<');
    assert_eq!(drawn.color, up.color);
    assert_eq!(drawn.word, up.word);

    let mut hole = cell("hole", 1, 0, 0, false);
    hole.asset = Some("terrain.floor.stone".into());
    let under = cell("under", 1, 0, -1, false);
    let pit = column_glyph(&observation(vec![hole, under], vec![], vec![]), &[], 1, 0).unwrap();
    let caret = structural(Structure::Pit);
    assert_eq!(pit.ch, '^');
    assert_eq!(pit.color, caret.color);
    assert_eq!(pit.word, caret.word);
    assert_eq!(caret.color, 0xD07A4A);
    assert_eq!(caret.word, "pit");
}

#[test]
fn published_rows_resolve_ahead_of_the_structural_default() {
    let rows = [
        (
            "creature.rat",
            Glyph {
                ch: 'r',
                color: 0xC4A574,
                word: "rat",
            },
        ),
        (
            "item.coin",
            Glyph {
                ch: '$',
                color: 0xE6C34A,
                word: "coin",
            },
        ),
        (
            "terrain.floor.marble",
            Glyph {
                ch: '.',
                color: 0xD9E2EA,
                word: "marble floor",
            },
        ),
        (
            "terrain.wall.marble",
            Glyph {
                ch: '#',
                color: 0xE6E6E6,
                word: "marble wall",
            },
        ),
        (
            "terrain.wall.cave",
            Glyph {
                ch: '#',
                color: 0x7D6B52,
                word: "cave wall",
            },
        ),
        (
            "terrain.floor.stone",
            Glyph {
                ch: '.',
                color: 0x9AA7A0,
                word: "stone floor",
            },
        ),
        (
            "terrain.wall.stone",
            Glyph {
                ch: '#',
                color: 0xB7B7B7,
                word: "stone wall",
            },
        ),
    ];
    for (id, glyph) in rows {
        assert_eq!(resolve(Some(id), Structure::Wall), glyph, "{id}");
    }
}

fn observation(
    visible_cells: Vec<CellView>,
    visible_actors: Vec<ActorView>,
    ground_items: Vec<GroundItemView>,
) -> Observation {
    Observation {
        combat: None,
        motion: None,
        places: Vec::new(),
        actor: ActorId(1),
        tick: 0,
        position: Position { x: 0, y: 0, z: 0 },
        visible_cells,
        ground_items,
        inventory: Vec::new(),
        visible_actors,
        ready: true,
    }
}

fn cell(key: &str, x: i32, y: i32, z: i32, wall: bool) -> CellView {
    CellView {
        asset: None,
        door: None,
        material: String::new(),
        key: key.into(),
        stairs_up: false,
        stairs_down: false,
        position: Position { x, y, z },
        wall,
        place_hint: false,
    }
}
