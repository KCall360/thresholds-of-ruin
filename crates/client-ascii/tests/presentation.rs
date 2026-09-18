use tor_client_ascii::{glyph_at, App, Effect, Input, Key};
use tor_client_common::ClientState;
use tor_protocol::*;

fn state() -> ClientState {
    ClientState::from_snapshot(serde_json::from_value(serde_json::json!({
        "actor":1,"branch":"test","cursor":{"sequence":0,"tick":0},"has_control":true,
        "history":{"entries":[],"older_before":null},
        "state":{"revision":3,"observation":{
            "actor":1,"tick":0,"position":{"region":1,"x":1,"y":1,"z":0},
            "region":{"id":1,"name":"Entry","width":5,"depth":3,"height":1},
            "ground_items":[{"item":{"id":3,"name":"token"},"position":{"region":1,"x":1,"y":1,"z":0}}],
            "inventory":[],"visible_actors":[],"exits":[{"position":{"region":1,"x":4,"y":1,"z":0},"direction":"east"}],
            "known_places":[{"id":1,"name":"Entry"}],"ready":true
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
    assert_eq!(glyph_at(o, 4, 1), '+');
    assert_eq!(glyph_at(o, 2, 1), '.');
    assert_eq!(glyph_at(o, 99, 1), ' ');
    let mut other_level = o.clone();
    other_level.ground_items[0].position = Position {
        region: 1,
        x: 2,
        y: 1,
        z: 1,
    };
    assert_eq!(glyph_at(&other_level, 2, 1), '.');
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
            item: ItemView {
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
    view.observation.region.width = i32::MAX;
    view.observation.region.depth = i32::MAX;
    view.observation.position.x = i32::MAX - 1;
    view.observation.position.y = i32::MAX - 1;
    view.observation.region.name = "Room\u{1b}[2J".repeat(1000);
    app.set_state(
        ClientState::from_snapshot(Snapshot {
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
