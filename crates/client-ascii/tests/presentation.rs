use tor_client_ascii::{glyph_at, App, Effect, Input, Key};
use tor_client_common::ClientState;
use tor_protocol::*;

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
fn travel_clicks_ignore_unknown_cells_and_spectators() {
    let mut app = App::new();
    app.set_state(state());
    app.ready();
    assert_eq!(app.input(Input::Click { x: 400, y: 220 }), Effect::None);
    app.role = AccessRole::Player;
    assert_eq!(app.input(Input::Click { x: 800, y: 500 }), Effect::None);
    let (x, y) = tor_client_ascii::render::cell_center(
        &state().state().observation,
        Position { x: 3, y: 1, z: 0 },
    )
    .unwrap();
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
    canvas.draw(&app);
    assert_ne!(normal, canvas.pixels);
}

fn state() -> ClientState {
    ClientState::from_snapshot(serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"test","cursor":{"sequence":0,"tick":0},"has_control":true,
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":3,"observation":{
            "actor":1,"tick":0,"position":{"x":1,"y":1,"z":0},

            "visible_cells": (0..5).flat_map(|x| (0..3).map(move |y| serde_json::json!({"key":format!("{x}:{y}"),"stairs_up":false,"stairs_down":false,"position":{"x":x,"y":y,"z":0},"wall":false,"place_hint":false}))).collect::<Vec<_>>(),
            "ground_items":[{"reachable":true,"item":{"id":3,"name":"token"},"position":{"x":1,"y":1,"z":0}}],
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
                action: Action::Take { item: 3 },
                ..
            },
            ..
        })
    ));
}

#[test]
fn only_disclosed_current_level_cells_are_drawn_and_actor_wins_over_item() {
    let state = state();
    let o = &state.state().observation;
    assert_eq!(glyph_at(o, 1, 1), '@');
    assert_eq!(glyph_at(o, 4, 1), '.');
    assert_eq!(glyph_at(o, 2, 1), '.');
    assert_eq!(glyph_at(o, 99, 1), ' ');
    let mut other_level = o.clone();
    other_level.ground_items[0].position = Position { x: 2, y: 1, z: 1 };
    assert_eq!(glyph_at(&other_level, 2, 1), '.');
    other_level
        .visible_cells
        .retain(|cell| cell.position.x != 2);
    assert_eq!(glyph_at(&other_level, 2, 1), ' ');
    other_level.visible_cells.push(CellView {
        door: None,
        material: "stone".into(),
        key: "wall".into(),
        stairs_up: false,
        stairs_down: false,
        position: Position { x: 2, y: 1, z: 0 },
        wall: true,
        place_hint: false,
    });
    assert_eq!(glyph_at(&other_level, 2, 1), '#');
    other_level.ground_items[0].position = Position { x: 3, y: 1, z: 0 };
    assert_eq!(glyph_at(&other_level, 3, 1), '!');
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
                action: Action::Take { item: 4 },
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
fn resized_clicks_and_stair_panels_use_the_rendered_cell_layout() {
    use tor_client_ascii::render::{cell_at, cell_center, logical_mouse};
    let mut view = state().state().observation.clone();
    let mut landing = view.visible_cells[0].clone();
    landing.key = "landing".into();
    landing.position.z = 1;
    view.visible_cells.push(landing.clone());
    for position in [view.position, landing.position] {
        let (x, y) = cell_center(&view, position).unwrap();
        assert_eq!(cell_at(&view, x, y), Some(position));
        // 1600x800 has 200 pixels of letterboxing on either side.
        assert_eq!(
            logical_mouse((x + 200) as f32, y as f32, 1600, 800),
            Some((x, y))
        );
        assert_eq!(
            logical_mouse(x as f32 / 2.0, y as f32 / 2.0, 600, 400),
            Some((x, y))
        );
    }
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
    assert_eq!(glyph_at(&state.state().observation, 2, 1), '+');
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
