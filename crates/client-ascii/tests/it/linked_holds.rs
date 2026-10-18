use std::sync::Arc;
use tor_client_ascii::{App, Effect, Input, Key};
use tor_client_common::{test_support, ClientState};
use tor_protocol::*;

fn target(index: u8) -> HoldTarget {
    HoldTarget::from_digest([index; 32])
}

fn snapshot(count: u8) -> Snapshot {
    let mut view = test_support::observation();
    view.linked_holds = (1..=count)
        .map(|index| LinkedHold {
            target: target(index),
            holder: (index == 1).then_some(view.visible_actors[0].id),
        })
        .collect();
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},
        "context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"holds",
        "cursor":{"sequence":"0","tick":"0"},"has_control":true,"intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":"1","observation":view}
    }))
    .unwrap()
}

fn app(count: u8) -> App {
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot(count)).unwrap());
    app.role = AccessRole::Player;
    app.ready();
    app.messages.skip();
    app
}

fn key(app: &mut App, key: Key) -> Effect {
    app.input(Input::Key { key })
}

fn assert_escape(effect: Effect, hold: HoldTarget, revision: u64) {
    assert!(matches!(
        super::request(effect),
        Some(Request::Command {
            command: Command::Act {
                action: Action::Escape { hold: actual },
                expected_revision,
            },
            ..
        }) if actual == hold && expected_revision == revision
    ));
}

#[test]
fn hold_escape_requires_explicit_choice_and_dismissal_never_submits() {
    for dismiss in [Key::Escape, Key::EscapeHold] {
        let mut app = app(2);
        let before = app.state.as_ref().unwrap().snapshot();
        assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
        assert_eq!(app.hold_choices.len(), 2);
        assert_eq!(
            app.hold_choice_offers(),
            before.state.observation.linked_holds
        );
        assert_eq!(key(&mut app, Key::Wait), Effect::None);
        assert_eq!(app.input(Input::Click { x: 100, y: 200 }), Effect::None);
        assert_eq!(key(&mut app, Key::Down), Effect::None);
        assert_eq!(app.selected, 1);
        assert_eq!(key(&mut app, dismiss), Effect::None);
        assert!(app.hold_choices.is_empty());
        assert!(app.hold_choice_offers().is_empty());
        assert_eq!(key(&mut app, Key::Enter), Effect::None);
        assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
        assert!(!app.busy);
    }
    let mut app = app(2);
    let before = app.state.as_ref().unwrap().snapshot();
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
    assert_eq!(key(&mut app, Key::Up), Effect::None);
    assert_eq!(app.selected, 0);
    for _ in 0..3 {
        assert_eq!(key(&mut app, Key::Down), Effect::None);
    }
    assert_eq!(app.selected, 1);
    assert_escape(key(&mut app, Key::Enter), target(2), 1);
    assert!(app.hold_choices.is_empty());
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    assert!(app.busy);
}

#[test]
fn sole_hold_escape_is_exact_and_spectators_can_only_read_the_hold_rows() {
    let mut player = app(1);
    assert_escape(key(&mut player, Key::EscapeHold), target(1), 1);
    assert!(player.hold_choices.is_empty());

    let mut spectator = app(2);
    spectator.role = AccessRole::Spectator;
    let before = spectator.state.as_ref().unwrap().snapshot();
    let rows = spectator.hold_rows().join("\n");
    assert!(rows.contains("sentinel"));
    assert!(rows.contains("Unseen holder"));
    assert_eq!(key(&mut spectator, Key::EscapeHold), Effect::None);
    assert!(spectator.hold_choices.is_empty());
    assert_eq!(key(&mut spectator, Key::Stats), Effect::None);
    assert!(spectator.stats_rows().join("\n").contains("Unseen holder"));
    assert!(
        tor_client_ascii::status_lines(&spectator, spectator.state.as_ref().unwrap())[1]
            .contains("Held:2")
    );
    assert_eq!(spectator.state.as_ref().unwrap().snapshot(), before);
    assert!(!spectator.busy);
    assert!(tor_client_ascii::HELP.iter().any(|row| row.contains("F11")));

    let mut free = app(0);
    assert_eq!(key(&mut free, Key::EscapeHold), Effect::None);
    assert!(free.hold_choices.is_empty());
    assert!(!free.busy);
}

#[test]
fn refreshed_hold_offers_clear_selection_instead_of_rebinding_the_selected_index() {
    for refresh in 0..3 {
        let mut app = app(2);
        assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
        assert_eq!(key(&mut app, Key::Down), Effect::None);
        assert_eq!(app.hold_choices[app.selected].target, target(2));
        let before = app.state.as_ref().unwrap().snapshot();
        let mut changed = before.clone();
        let state = Arc::make_mut(&mut changed.state);
        state.revision += 1;
        state.observation.linked_holds.reverse();
        if refresh != 0 {
            let body = if refresh == 1 {
                UpdateBody::Observation {
                    state: changed.state,
                    event: None,
                }
            } else {
                UpdateBody::ObservationDelta {
                    base: ObservationBase {
                        cursor: before.cursor,
                        revision: before.state.revision,
                    },
                    state: Box::new(StateDelta::between(&before.state, &changed.state).unwrap()),
                    event: None,
                }
            };
            app.update(StreamUpdate {
                context: changed.context,
                actor: changed.actor,
                branch: changed.branch,
                cursor: StreamCursor {
                    sequence: 1,
                    tick: 0,
                },
                body,
            })
            .unwrap();
        } else {
            app.set_state(ClientState::from_snapshot(changed).unwrap());
        }
        assert!(app.hold_choices.is_empty());
        assert_eq!(key(&mut app, Key::Enter), Effect::None);
        app.messages.skip();
        assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
        assert_eq!(app.selected, 0);
        assert_escape(key(&mut app, Key::Enter), target(2), 2);
    }
}

#[test]
fn snapshot_reset_clears_hold_selection_even_when_observation_revision_is_unchanged() {
    let mut app = app(2);
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
    assert_eq!(key(&mut app, Key::Down), Effect::None);
    let mut reset = app.state.as_ref().unwrap().snapshot();
    reset.context.epoch += 1;
    let mut invalid = reset.clone();
    invalid.actor = ActorId(99);
    assert!(app.replace_snapshot(invalid).is_err());
    assert_eq!(app.hold_choices[app.selected].target, target(2));
    app.replace_snapshot(reset).unwrap();
    assert!(app.hold_choices.is_empty());
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert!(!app.busy);
}

#[test]
fn hold_escape_checks_connection_busy_control_and_current_admission() {
    let mut app = app(2);
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
    app.busy = true;
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert_eq!(app.hold_choices.len(), 2);
    app.ready();
    app.connected = false;
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert_eq!(app.hold_choices.len(), 2);
    app.connected = true;
    app.disconnect("Disconnected".into());
    assert!(app.hold_choices.is_empty());
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);

    let mut app = self::app(2);
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
    let current = app.state.as_ref().unwrap().snapshot();
    app.update(StreamUpdate {
        context: current.context,
        actor: current.actor,
        branch: current.branch,
        cursor: StreamCursor {
            sequence: 1,
            tick: 0,
        },
        body: UpdateBody::Control { has_control: false },
    })
    .unwrap();
    assert!(app.hold_choices.is_empty());
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
    assert!(app.hold_choices.is_empty());

    let mut denied = snapshot(1);
    denied.readiness.admission = false;
    app.set_state(ClientState::from_snapshot(denied).unwrap());
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
    assert!(!app.busy);
    let mut app = self::app(2);
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
    let current = app.state.as_ref().unwrap().snapshot();
    app.update(StreamUpdate {
        context: current.context,
        actor: current.actor,
        branch: current.branch,
        cursor: StreamCursor {
            sequence: 1,
            tick: 0,
        },
        body: UpdateBody::Readiness {
            readiness: Readiness {
                revision: 1,
                admission: false,
                resume: vec![],
                cancel: vec![],
            },
        },
    })
    .unwrap();
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert!(app.hold_choices.is_empty());
    assert!(!app.busy);
}

#[test]
fn hold_escape_modal_draws_the_selected_row_and_scrolls_with_long_offers() {
    let mut app = app(32);
    let mut canvas = tor_client_ascii::render::Canvas::default();
    assert_eq!(key(&mut app, Key::EscapeHold), Effect::None);
    canvas.draw(&app);
    let first = canvas.pixels.clone();
    for _ in 0..31 {
        assert_eq!(key(&mut app, Key::Down), Effect::None);
    }
    canvas.draw(&app);
    assert_ne!(canvas.pixels, first);
    assert_eq!(app.selected, 31);
    assert_escape(key(&mut app, Key::Enter), target(32), 1);
}

#[test]
fn hold_escape_history_keeps_the_start_generic() {
    let entry = HistoryEntry {
        id: EntryId("escape-entry".into()),
        branch: BranchId("holds".into()),
        actor: ActorId(1),
        tick: 0,
        author: Author::User {
            user: "test".into(),
        },
        audience: Audience::Actor,
        content: HistoryContent::Action {
            action: Action::Escape { hold: target(1) },
            event: Event::EscapeStarted { hold: target(1) },
        },
    };
    assert_eq!(
        tor_client_ascii::history_text(&entry),
        "Began trying to escape a hold."
    );
}
