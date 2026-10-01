use tor_client_ascii::{column_glyph, App, Effect, Input, Key};
use tor_client_common::ClientState;
use tor_protocol::*;

#[test]
fn quantity_picker_submits_partial_pickup_and_drop() {
    let mut snapshot = state().snapshot();
    snapshot.state.observation.ground_items[0].item.quantity = 10;
    snapshot
        .state
        .observation
        .inventory
        .push(snapshot.state.observation.ground_items[0].item.clone());
    for key in [Key::Pickup, Key::Drop] {
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
        app.ready();
        assert_eq!(app.input(Input::Key { key }), Effect::None);
        app.input(Input::Text { text: "3".into() });
        let effect = app.input(Input::Key { key: Key::Enter });
        let expected = if key == Key::Pickup {
            Action::Take {
                item: 3,
                quantity: Some(3),
            }
        } else {
            Action::Drop {
                item: 3,
                quantity: Some(3),
            }
        };
        assert!(
            matches!(effect, Effect::Request(Request::Command { command: Command::Act { action, .. }, .. }) if action == expected)
        );
    }
}

#[test]
fn places_modal_displays_memory_and_renames_without_travel_or_time() {
    let mut snapshot = state().snapshot();
    snapshot.state.observation.places.push(PlaceView {
        key: "forgotten-cell".into(),
        name: "Quiet Reverie".into(),
    });
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Places }), Effect::None);
    assert!(app.places_open);
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    app.input(Input::Text {
        text: "Home of Echoes".into(),
    });
    assert!(
        matches!(app.input(Input::Key { key: Key::Enter }), Effect::Request(Request::Command {
        command: Command::RenamePlace { key, name, expected_revision: 3 }, ..
    }) if key == "forgotten-cell" && name == "Home of Echoes")
    );
}

#[test]
fn travel_selection_is_free_and_submits_an_opaque_cell_key() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Travel }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
    assert_eq!(
        app.input(Input::Key { key: Key::Enter }),
        Effect::Request(Request::Command {
            branch: BranchId("test".into()),
            command: Command::Travel {
                expected_revision: 3,
                destination: "2:1".into()
            }
        })
    );
}

#[test]
fn diagonal_cursor_and_actions_use_current_disclosed_state() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    app.input(Input::Key { key: Key::Travel });
    app.input(Input::Key {
        key: Key::NorthEast,
    });
    assert_eq!(app.travel_cursor, Some(Position { x: 2, y: 0, z: 0 }));
    assert!(
        matches!(app.input(Input::Key {key:Key::Enter}),Effect::Request(Request::Command {command:Command::Travel {destination,..},..}) if destination=="2:0")
    );
    app.ready();
    for (key, direction) in [
        (Key::NorthEast, Direction::NorthEast),
        (Key::SouthEast, Direction::SouthEast),
        (Key::SouthWest, Direction::SouthWest),
        (Key::NorthWest, Direction::NorthWest),
    ] {
        assert!(
            matches!(app.input(Input::Key {key}),Effect::Request(Request::Command {command:Command::Act { action:Action::Move {direction:d}, expected_revision:3 },..}) if d==direction)
        );
        app.ready();
    }
}

#[test]
fn travel_clicks_ignore_unknown_cells_and_spectators() {
    let mut app = App::new();
    app.set_state(state());
    app.ready();
    assert_eq!(app.input(Input::Click { x: 400, y: 220 }), Effect::None);
    app.role = AccessRole::Player;
    assert_eq!(app.input(Input::Click { x: 800, y: 500 }), Effect::None);
    let (x, y) =
        tor_client_ascii::render::cell_center(&app, Position { x: 3, y: 1, z: 0 }).unwrap();
    assert!(
        matches!(app.input(Input::Click { x, y }), Effect::Request(Request::Command { command: Command::Travel { destination, .. }, .. }) if destination == "3:1")
    );
}

#[test]
fn rewind_clears_old_drafts_and_wizard_marker_changes_the_visible_frame() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    app.input(Input::Key { key: Key::Note });
    assert!(app.note.is_some());
    app.attack_targets.push(ActorView {
        asset: None,
        id: ActorId(2),
        name: "guard".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
    });
    let mut canvas = tor_client_ascii::render::Canvas::default();
    canvas.draw(&app);
    let normal = canvas.pixels.clone();
    let mut snapshot = serde_json::to_value(serde_json::json!({
        "actor":1,"branch":"new-branch","cursor":{"sequence":0,"tick":0},"has_control":true,
        "history":{"entries":[],"older_before":null},"state":state().state()
    }))
    .unwrap();
    snapshot["state"]["wizard_game"] = true.into();
    app.set_state(ClientState::from_snapshot(serde_json::from_value(snapshot).unwrap()).unwrap());
    assert!(app.note.is_none());
    assert!(app.attack_targets.is_empty());
    canvas.draw(&app);
    assert_ne!(normal, canvas.pixels);
}

fn state() -> ClientState {
    ClientState::from_snapshot(serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"test","cursor":{"sequence":0,"tick":0},"has_control":true,
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":3,"observation":{
            "actor":1,"tick":0,"position":{"x":1,"y":1,"z":0},

            "places":[],"visible_cells": (0..5).flat_map(|x| (0..3).map(move |y| serde_json::json!({"key":format!("{x}:{y}"),"stairs_up":false,"stairs_down":false,"position":{"x":x,"y":y,"z":0},"wall":false,"place_hint":false}))).collect::<Vec<_>>(),
            "ground_items":[{"reachable":true,"item":{"quantity":1,"appearance":"item","identified":true,"id":3,"name":"token"},"position":{"x":1,"y":1,"z":0}}],
            "inventory":[],"visible_actors":[],
            "ready":true
        }}
    })).unwrap()).unwrap()
}

#[test]
fn input_uses_current_revision_and_does_not_queue_actions_while_busy() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    assert_eq!(
        app.input(Input::Key { key: Key::Right }),
        Effect::Request(Request::Command {
            branch: BranchId("test".into()),
            command: Command::Act {
                expected_revision: 3,
                action: Action::Move {
                    direction: Direction::East
                }
            }
        })
    );
    assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
    app.ready();
    assert!(matches!(
        app.input(Input::Key { key: Key::Pickup }),
        Effect::Request(Request::Command {
            command: Command::Act {
                action: Action::Take {
                    item: 3,
                    quantity: None
                },
                ..
            },
            ..
        })
    ));
}

#[test]
fn an_item_above_an_open_cell_is_drawn_and_a_standing_wall_stays_a_wall() {
    let state = state();
    let o = &state.state().observation;
    assert_eq!(column_glyph(o, &[], 1, 1).unwrap().ch, '@');
    assert_eq!(column_glyph(o, &[], 4, 1).unwrap().ch, '.');
    assert!(column_glyph(o, &[], 99, 1).is_none());
    let mut other_level = o.clone();
    other_level.ground_items[0].position = Position { x: 2, y: 1, z: 1 };
    assert_eq!(column_glyph(&other_level, &[], 2, 1).unwrap().ch, '!');
    other_level
        .visible_cells
        .retain(|cell| cell.position.x != 2);
    other_level.ground_items.clear();
    assert!(column_glyph(&other_level, &[], 2, 1).is_none());
    other_level.visible_cells.push(CellView {
        asset: None,
        door: None,
        material: "stone".into(),
        key: "wall".into(),
        stairs_up: false,
        stairs_down: false,
        position: Position { x: 2, y: 1, z: 0 },
        wall: true,
        place_hint: false,
    });
    assert_eq!(column_glyph(&other_level, &[], 2, 1).unwrap().ch, '#');
}

#[test]
fn notes_are_modal_and_do_not_turn_typed_movement_letters_into_actions() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Note }), Effect::None);
    assert_eq!(
        app.input(Input::Text {
            text: "east".into()
        }),
        Effect::None
    );
    assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
    assert!(
        matches!(app.input(Input::Key {key:Key::Enter}), Effect::Request(Request::Command {command:Command::Annotate {text, audience:Audience::Private,anchor:Anchor::State {revision:3},..},..}) if text == "east")
    );
}

#[test]
fn losing_control_or_disconnect_prevents_actions() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    let mut state = state();
    state
        .apply(StreamUpdate {
            actor: ActorId(1),
            branch: BranchId("test".into()),
            cursor: StreamCursor {
                sequence: 1,
                tick: 0,
            },
            body: UpdateBody::Control { has_control: false },
        })
        .unwrap();
    app.set_state(state);
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Wait }), Effect::None);
    assert!(app.status.contains("observing"));
    app.disconnect("Lost connection".into());
    assert_eq!(app.input(Input::Key { key: Key::Control }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::Quit);
}

#[test]
fn ambiguous_pickup_is_modal_free_and_invalidated_by_an_observation_change() {
    let mut snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"test","cursor":{"sequence":0,"tick":0},"has_control":true,
        "history":{"entries":[],"older_before":null},"state":state().state()
    }))
    .unwrap();
    snapshot
        .state
        .observation
        .ground_items
        .push(GroundItemView {
            reachable: true,
            item: ItemView {
                asset: None,
                quantity: 1,
                appearance: String::new(),
                identified: true,
                description: String::new(),
                id: 4,
                name: "another token".into(),
            },
            position: snapshot.state.observation.position,
        });
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Pickup }), Effect::None);
    assert_eq!(app.pickup.len(), 2);
    assert!(!app.busy);
    assert_eq!(app.input(Input::Key { key: Key::Down }), Effect::None);
    assert!(matches!(
        app.input(Input::Key { key: Key::Enter }),
        Effect::Request(Request::Command {
            command: Command::Act {
                action: Action::Take {
                    item: 4,
                    quantity: None
                },
                ..
            },
            ..
        })
    ));
    app.ready();
    app.input(Input::Key { key: Key::Pickup });
    snapshot.state.revision += 1;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert!(app.pickup.is_empty());
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
}

#[test]
fn note_audience_unicode_limits_and_cancel_do_not_change_state() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    app.input(Input::Key { key: Key::Note });
    app.input(Input::Text {
        text: "é".repeat(3000),
    });
    assert_eq!(app.note.as_ref().unwrap().text.len(), MAX_NOTE_BYTES);
    app.input(Input::Key {
        key: Key::Backspace,
    });
    assert_eq!(app.note.as_ref().unwrap().text.len(), MAX_NOTE_BYTES - 2);
    app.input(Input::Key { key: Key::Tab });
    assert!(matches!(
        app.input(Input::Key { key: Key::Enter }),
        Effect::Request(Request::Command {
            command: Command::Annotate {
                audience: Audience::Actor,
                ..
            },
            ..
        })
    ));
    app.ready();
    app.input(Input::Key { key: Key::Note });
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(app.note.is_none());
    assert_eq!(app.state.as_ref().unwrap().state().revision, 3);
}

#[test]
fn history_scroll_is_local_and_does_not_move_the_actor() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    app.history_page = Some(HistoryPage {
        entries: vec![],
        older_before: Some(EntryId("older".into())),
    });
    assert_eq!(app.input(Input::Key { key: Key::Down }), Effect::None);
    assert_eq!(
        app.input(Input::Key {
            key: Key::OlderHistory
        }),
        Effect::Request(Request::History {
            before: Some(EntryId("older".into())),
            limit: 50
        })
    );
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(app.history_page.is_none());
}

#[test]
fn renderer_handles_large_rooms_and_long_untrusted_labels_without_mutating_state() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    let original = state();
    let mut view = original.state().clone();
    view.observation.position.x = i32::MAX - 1;
    view.observation.position.y = i32::MAX - 1;
    app.set_state(
        ClientState::from_snapshot(Snapshot {
            travel: None,
            actor: ActorId(1),
            branch: original.branch().clone(),
            cursor: original.cursor(),
            state: view,
            has_control: true,
            history: HistoryPage {
                entries: vec![],
                older_before: None,
            },
        })
        .unwrap(),
    );
    app.ready();
    let before = app.state.clone();
    let mut canvas = tor_client_ascii::render::Canvas::default();
    canvas.draw(&app);
    assert_eq!(
        canvas.pixels.len(),
        tor_client_ascii::render::WIDTH * tor_client_ascii::render::HEIGHT
    );
    assert!(canvas.pixels.windows(2).any(|p| p[0] != p[1]));
    assert_eq!(app.state, before);
    app.input(Input::Key { key: Key::Note });
    app.input(Input::Text {
        text: "long text".repeat(500),
    });
    canvas.draw(&app);
}

#[test]
fn spectator_can_browse_but_cannot_create_any_mutation_or_note_draft() {
    let mut app = App::new();
    app.set_state(state());
    app.ready();
    for key in [
        Key::Up,
        Key::Down,
        Key::Left,
        Key::Right,
        Key::Ascend,
        Key::Descend,
        Key::Wait,
        Key::Pickup,
        Key::Control,
        Key::Release,
        Key::Note,
    ] {
        assert_eq!(app.input(Input::Key { key }), Effect::None);
        assert!(app.status.contains("read-only"));
        assert!(app.note.is_none());
        assert!(!app.busy);
    }
    assert!(matches!(
        app.input(Input::Key { key: Key::History }),
        Effect::Request(Request::History { .. })
    ));
    app.history_page = Some(HistoryPage {
        entries: vec![],
        older_before: None,
    });
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Down }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::Quit);
}

#[test]
fn resized_clicks_use_the_one_drawn_column() {
    use tor_client_ascii::render::{cell_at, cell_center, logical_mouse};
    let mut snapshot = state().snapshot();
    let mut landing = snapshot.state.observation.visible_cells[0].clone();
    landing.key = "landing".into();
    landing.position.z = 1;
    snapshot
        .state
        .observation
        .visible_cells
        .push(landing.clone());
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    let player = app.state.as_ref().unwrap().state().observation.position;
    assert!(cell_center(&app, landing.position).is_none());
    let (x, y) = cell_center(&app, player).unwrap();
    assert_eq!(cell_at(&app, x, y), Some(player));
    // 1600x800 has 200 pixels of letterboxing on either side.
    assert_eq!(
        logical_mouse((x + 200) as f32, y as f32, 1600, 800),
        Some((x, y))
    );
    assert_eq!(
        logical_mouse(x as f32 / 2.0, y as f32 / 2.0, 600, 400),
        Some((x, y))
    );
    assert_eq!(logical_mouse(10.0, 100.0, 1600, 800), None);
}

#[test]
fn escape_cancels_active_travel_and_changed_observations_clear_selection() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    app.input(Input::Key { key: Key::Travel });
    assert!(app.travel_cursor.is_some());
    let mut current = state();
    current
        .apply(StreamUpdate {
            actor: ActorId(1),
            branch: BranchId("test".into()),
            cursor: StreamCursor {
                sequence: 1,
                tick: 100,
            },
            body: UpdateBody::Observation {
                state: Box::new(StateView {
                    revision: 4,
                    observation: Observation {
                        tick: 100,
                        ..current.state().observation.clone()
                    },
                    ..current.state().clone()
                }),
                event: None,
            },
        })
        .unwrap();
    app.set_state(current.clone());
    assert!(app.travel_cursor.is_none());
    current
        .apply(StreamUpdate {
            actor: ActorId(1),
            branch: BranchId("test".into()),
            cursor: StreamCursor {
                sequence: 2,
                tick: 100,
            },
            body: UpdateBody::Travel {
                status: TravelStatus {
                    id: EntryId("trip".into()),
                    destination: "2:1".into(),
                    completed_steps: 0,
                    phase: TravelPhase::Active,
                },
                entry: Some(Box::new(HistoryEntry {
                    id: EntryId("trip".into()),
                    branch: BranchId("test".into()),
                    actor: ActorId(1),
                    tick: 100,
                    author: Author::User {
                        user: "test".into(),
                    },
                    audience: Audience::Actor,
                    content: HistoryContent::Travel {
                        destination: "2:1".into(),
                    },
                })),
            },
        })
        .unwrap();
    app.set_state(current);
    app.busy = true;
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(app.busy);
    app.ready();
    assert_eq!(
        app.input(Input::Key { key: Key::Escape }),
        Effect::Request(Request::CancelTravel {
            branch: BranchId("test".into()),
            travel_id: EntryId("trip".into())
        })
    );
}

#[test]
fn door_glyphs_and_explicit_selection_submit_actions_without_movement() {
    let mut snapshot = serde_json::to_value(serde_json::json!({
        "actor":1,"branch":"test","cursor":{"sequence":0,"tick":0},"has_control":true,
        "history":{"entries":[],"older_before":null},"state":state().state()
    }))
    .unwrap();
    let cells = snapshot["state"]["observation"]["visible_cells"]
        .as_array_mut()
        .unwrap();
    for (index, id) in [(7, 11), (5, 12)] {
        cells[index]["door"] = serde_json::json!({"id":id,"name":"wooden door","description":"wood", "open":false,"reachable":true,"approaches":[]});
    }
    let state = ClientState::from_snapshot(serde_json::from_value(snapshot).unwrap()).unwrap();
    assert_eq!(
        column_glyph(&state.state().observation, &[], 2, 1)
            .unwrap()
            .ch,
        '+'
    );
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state);
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::OpenDoor }), Effect::None);
    assert!(matches!(
        app.input(Input::Key { key: Key::Down }),
        Effect::Request(Request::Command {
            command: Command::Act {
                action: Action::SetDoor {
                    door: 12,
                    open: true
                },
                ..
            },
            ..
        })
    ));
}

#[test]
fn missing_door_direction_is_rejected_locally_and_escape_cancels() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    let before = app.state.as_ref().unwrap().state().clone();
    for action in [Key::OpenDoor, Key::CloseDoor] {
        assert_eq!(app.input(Input::Key { key: action }), Effect::None);
        assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
        assert_eq!(app.status, "There is no door in that direction.");
        assert!(!app.busy);
        assert_eq!(app.state.as_ref().unwrap().state(), &before);
        assert_eq!(app.input(Input::Key { key: action }), Effect::None);
        assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    }
}

#[test]
fn streamed_updates_retain_intermediate_memory_and_snapshot_resets_selections() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    let initial = state().snapshot();
    app.replace_snapshot(initial.clone()).unwrap();
    app.ready();
    for sequence in 1..=64 {
        let mut view = initial.state.clone();
        view.revision += sequence;
        view.observation.tick += sequence;
        view.observation.visible_cells[0].key = format!("intermediate-{sequence}");
        app.update(StreamUpdate {
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: initial.cursor.sequence + sequence,
                tick: view.observation.tick,
            },
            body: UpdateBody::Observation {
                state: Box::new(view),
                event: None,
            },
        })
        .unwrap();
    }
    let current = app.state.as_ref().unwrap();
    assert!(current.memory().any(|c| c.key == "intermediate-1"));
    let before = current.clone();
    let mut wrong = initial.clone();
    wrong.actor = ActorId(99);
    assert!(app.replace_snapshot(wrong).is_err());
    assert_eq!(app.state.as_ref().unwrap(), &before);
    app.input(Input::Key { key: Key::Note });
    assert!(app.note.is_some());
    let mut rewind = initial;
    rewind.branch = BranchId("rewound".into());
    app.replace_snapshot(rewind).unwrap();
    assert!(app.note.is_none());
    assert!(!app
        .state
        .as_ref()
        .unwrap()
        .memory()
        .any(|c| c.key.starts_with("intermediate-")));
}

#[test]
fn configurable_bump_attacks_use_disclosed_hostility_only() {
    use tor_client_ascii::BumpAttacks;
    for (mode, hostile, attacks) in [
        (BumpAttacks::Hostile, true, true),
        (BumpAttacks::Hostile, false, false),
        (BumpAttacks::Any, false, true),
        (BumpAttacks::Off, true, false),
    ] {
        let mut view = state().state().clone();
        view.observation.visible_actors.push(ActorView {
            asset: None,
            id: ActorId(2),
            name: "guard".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        });
        view.observation.combat = Some(CombatView {
            hp: 30,
            max_hp: 30,
            preparation_remaining: None,
            preparation_active: false,
            recovery_remaining: 0,
            actors: vec![CombatActorView {
                actor: ActorId(2),
                hostile,
                injury: "healthy".into(),
            }],
            messages: vec![],
            objective: None,
            victory: false,
            dead: false,
            terminal: false,
        });
        let snapshot: Snapshot=serde_json::from_value(serde_json::json!({"actor":1,"branch":"test","cursor":{"sequence":0,"tick":0},"has_control":true,"history":{"entries":[],"older_before":null},"state":view})).unwrap();
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.config.bump_attacks = mode;
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        app.ready();
        let Effect::Request(Request::Command {
            command: Command::Act { action, .. },
            ..
        }) = app.input(Input::Key { key: Key::Right })
        else {
            panic!("action")
        };
        assert_eq!(
            action,
            if attacks {
                Action::Attack { target: ActorId(2) }
            } else {
                Action::Move {
                    direction: Direction::East,
                }
            }
        );
        app.ready();
        app.input(Input::Key { key: Key::Attack });
        assert!(!app.attack_targets.is_empty());
        app.disconnect("lost connection".into());
        assert!(app.attack_targets.is_empty());
    }
}

#[test]
fn prose_dashes_render_as_supported_bitmap_glyphs() {
    let mut prose_app = App::new();
    prose_app.accept_status("HP 20/20 — Victory!".into());
    let mut plain = App::new();
    plain.accept_status("HP 20/20 - Victory!".into());
    let mut canvas = tor_client_ascii::render::Canvas::default();
    canvas.draw(&prose_app);
    let prose = canvas.pixels.clone();
    canvas.draw(&plain);
    assert_eq!(canvas.pixels, prose);
}

#[test]
fn annotation_control_character_and_invalid_action_append_once() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    let current = state();
    app.set_state(current.clone());
    app.ready();
    let busy_before = app.busy;
    app.update(StreamUpdate {
        actor: ActorId(1),
        branch: BranchId("test".into()),
        cursor: StreamCursor {
            sequence: 1,
            tick: 100,
        },
        body: UpdateBody::Observation {
            state: Box::new(StateView {
                revision: 4,
                observation: Observation {
                    tick: 100,
                    ..current.state().observation.clone()
                },
                ..current.state().clone()
            }),
            event: Some(Box::new(HistoryEntry {
                id: EntryId("move".into()),
                branch: BranchId("test".into()),
                actor: ActorId(1),
                tick: 100,
                author: Author::User {
                    user: "tester".into(),
                },
                audience: Audience::Actor,
                content: HistoryContent::Action {
                    action: Action::Move {
                        direction: Direction::East,
                    },
                    event: Event::Moved {
                        direction: Direction::East,
                    },
                },
            })),
        },
    })
    .unwrap();
    app.update(StreamUpdate {
        actor: ActorId(1),
        branch: BranchId("test".into()),
        cursor: StreamCursor {
            sequence: 2,
            tick: 100,
        },
        body: UpdateBody::Annotation {
            entry: Box::new(HistoryEntry {
                id: EntryId("note".into()),
                branch: BranchId("test".into()),
                actor: ActorId(1),
                tick: 100,
                author: Author::User {
                    user: "tester".into(),
                },
                audience: Audience::Private,
                content: HistoryContent::Annotation {
                    anchor: Anchor::State { revision: 4 },
                    category: AnnotationCategory::Note,
                    text: "A\u{1}note".into(),
                },
            }),
        },
    })
    .unwrap();
    app.accept_status("InvalidAction: Action is unavailable".into());
    assert_eq!(app.busy, busy_before);
    let narration = app.state.as_ref().unwrap().narration();
    assert_eq!(narration, &["You move east.".to_owned()]);
    let messages = app.message_lines();
    let joined = messages.join("\n");
    assert_eq!(joined.matches("Note: A note").count(), 1);
    assert!(!joined.contains('\u{1}'));
    assert_eq!(
        joined
            .matches("InvalidAction: Action is unavailable")
            .count(),
        1
    );
    assert_eq!(joined.matches("Action is unavailable").count(), 1);
    assert_eq!(narration, &["You move east.".to_owned()]);
    let before = app.message_lines();
    let mut canvas = tor_client_ascii::render::Canvas::default();
    canvas.draw(&app);
    canvas.draw(&app);
    assert_eq!(app.message_lines(), before);

    app.replace_snapshot(state().snapshot()).unwrap();
    assert!(app.message_lines().is_empty());
    assert!(app.state.as_ref().unwrap().narration().is_empty());
    app.accept_status("still here".into());
    let mut other = state().snapshot();
    other.branch = BranchId("other".into());
    app.replace_snapshot(other).unwrap();
    assert!(app.message_lines().is_empty());
    assert_eq!(app.status, "Timeline changed; pending selections cleared.");
}

#[test]
fn more_pages_without_a_command_and_scrollback_does_not_quit() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    for line in ["one", "two", "three", "four"] {
        app.accept_status(line.into());
    }
    assert!(app.more());
    assert!(!app.busy);
    assert_eq!(
        app.message_lines(),
        vec!["one".to_owned(), "two".into(), "--More--".into()]
    );
    assert_eq!(app.input(Input::Key { key: Key::Space }), Effect::None);
    assert!(!app.busy);
    assert_eq!(app.message_lines(), vec!["three".to_owned(), "four".into()]);
    assert_eq!(
        app.input(Input::Key { key: Key::Space }),
        Effect::Request(Request::Command {
            branch: BranchId("test".into()),
            command: Command::Act {
                expected_revision: 3,
                action: Action::Wait,
            },
        })
    );

    let mut paging = App::new();
    paging.role = AccessRole::Player;
    paging.set_state(state());
    paging.ready();
    for line in ["one", "two", "three", "four"] {
        paging.accept_status(line.into());
    }
    paging.busy = true;
    assert_eq!(paging.input(Input::Key { key: Key::Right }), Effect::None);
    assert!(paging.busy);
    assert_eq!(
        paging.message_lines(),
        vec!["three".to_owned(), "four".into()]
    );

    let mut reading = App::new();
    reading.role = AccessRole::Player;
    reading.set_state(state());
    reading.ready();
    reading.accept_status("kept".into());
    for _ in 0..4 {
        reading.accept_status("page".into());
    }
    assert!(reading.more());
    assert_eq!(reading.input(Input::Key { key: Key::Space }), Effect::None);
    assert_eq!(
        reading.input(Input::Key {
            key: Key::Scrollback
        }),
        Effect::None
    );
    assert!(reading.scrollback_open());
    assert!(reading.scrollback_lines().iter().any(|line| line == "kept"));
    assert_eq!(reading.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(!reading.scrollback_open());
    assert_eq!(reading.input(Input::Key { key: Key::Escape }), Effect::Quit);
}

fn room(width: i32, height: i32) -> ClientState {
    let mut snapshot = state().snapshot();
    snapshot.state.observation.position = Position { x: 0, y: 0, z: 0 };
    snapshot.state.observation.visible_cells = (0..width)
        .flat_map(|x| {
            (0..height).map(move |y| {
                serde_json::json!({
                    "key": format!("{x}:{y}"),
                    "stairs_up": false,
                    "stairs_down": false,
                    "position": {"x": x, "y": y, "z": 0},
                    "wall": false,
                    "place_hint": false
                })
            })
        })
        .map(|cell| serde_json::from_value(cell).unwrap())
        .collect();
    snapshot.state.observation.ground_items.clear();
    let mut above = snapshot.state.observation.visible_cells[0].clone();
    above.key = "above".into();
    above.position.z = 1;
    snapshot.state.observation.visible_cells.push(above);
    ClientState::from_snapshot(snapshot).unwrap()
}

#[test]
fn fitting_rooms_use_one_sixteen_pixel_step_and_one_plane() {
    use tor_client_ascii::render::map_tiles;
    for (width, height) in [(3, 3), (40, 40)] {
        let mut app = App::new();
        app.set_state(room(width, height));
        let tiles = map_tiles(&app);
        assert!(tiles.iter().all(|tile| tile.position.z == 0));
        assert_eq!(
            tiles
                .iter()
                .filter(|tile| tile.position.x == 0 && tile.position.y == 0)
                .count(),
            1
        );
        let here = tiles.iter().find(|tile| tile.glyph == '@').unwrap();
        let east = tiles
            .iter()
            .find(|tile| {
                tile.position.x == here.position.x + 1 && tile.position.y == here.position.y
            })
            .unwrap();
        let south = tiles
            .iter()
            .find(|tile| {
                tile.position.y == here.position.y + 1 && tile.position.x == here.position.x
            })
            .unwrap();
        assert_eq!(east.center.0 - here.center.0, 16);
        assert_eq!(south.center.1 - here.center.1, 16);
    }
}

#[test]
fn look_clicks_do_not_travel() {
    use tor_client_ascii::render::cell_center;
    use tor_client_ascii::Click;
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.config.click = Click::Look;
    app.set_state(state());
    app.ready();
    let (x, y) = cell_center(&app, Position { x: 3, y: 1, z: 0 }).unwrap();
    assert_eq!(app.input(Input::Click { x, y }), Effect::None);
}

#[test]
fn an_eight_cell_corridor_centers_its_west_cell() {
    use tor_client_ascii::render::cell_center;
    let mut snapshot = state().snapshot();
    snapshot.state.observation.position = Position { x: 0, y: 0, z: 0 };
    snapshot.state.observation.ground_items.clear();
    snapshot.state.observation.visible_cells = (-5..=2)
        .map(|x| {
            serde_json::json!({
                "key": format!("{x}"),
                "stairs_up": false,
                "stairs_down": false,
                "position": {"x": x, "y": 0, "z": 0},
                "wall": false,
                "place_hint": false
            })
        })
        .map(|cell| serde_json::from_value(cell).unwrap())
        .collect();
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert_eq!(
        cell_center(&app, Position { x: -5, y: 0, z: 0 }),
        Some((536, 408))
    );
}

#[test]
fn a_scrolling_chart_shift_moves_the_origin_once() {
    use tor_client_ascii::render::map_tiles;
    fn band(shift: i32, omit: Option<i32>, revision: u64) -> Snapshot {
        let cells: Vec<_> = (-40..=40)
            .filter(|x| Some(*x) != omit)
            .map(|x| {
                serde_json::json!({
                    "key": format!("{x}"),
                    "stairs_up": false,
                    "stairs_down": false,
                    "position": {"x": x + shift, "y": 0, "z": 0},
                    "wall": false,
                    "place_hint": false
                })
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "actor": 1,
            "branch": "scroll",
            "cursor": {"sequence": 0, "tick": revision},
            "has_control": true,
            "history": {"entries": [], "older_before": null},
            "state": {
                "wizard_game": false,
                "revision": revision,
                "observation": {
                    "actor": 1,
                    "tick": revision,
                    "position": {"x": 0, "y": 0, "z": 0},
                    "places": [],
                    "visible_cells": cells,
                    "ground_items": [],
                    "inventory": [],
                    "visible_actors": [],
                    "ready": true
                }
            }
        }))
        .unwrap()
    }
    let mut app = App::new();
    app.replace_snapshot(band(0, None, 0)).unwrap();
    let first = map_tiles(&app);
    assert!(first.iter().all(|tile| tile.position.z == 0));
    let tracked = first.iter().find(|tile| tile.position.x == 20).unwrap();
    let player = first.iter().find(|tile| tile.glyph == '@').unwrap();
    assert_eq!(tracked.center.0 / 16, 57);
    assert_eq!(player.center.0 / 16, 37);
    app.replace_snapshot(band(-1, Some(20), 1)).unwrap();
    let shifted = map_tiles(&app);
    let tracked = shifted.iter().find(|tile| tile.position.x == 19).unwrap();
    let player = shifted.iter().find(|tile| tile.glyph == '@').unwrap();
    assert!(tracked.remembered);
    assert_eq!(tracked.center.0 / 16, 57);
    assert_eq!(player.center.0 / 16, 38);
    app.replace_snapshot(band(-1, Some(20), 1)).unwrap();
    let again = map_tiles(&app);
    let tracked = again.iter().find(|tile| tile.position.x == 19).unwrap();
    let player = again.iter().find(|tile| tile.glyph == '@').unwrap();
    assert_eq!(tracked.center.0 / 16, 57);
    assert_eq!(player.center.0 / 16, 38);
}
