//! Conventional keys route through disclosed actions, never guessed game rules.
use std::sync::Arc;
use tor_client_ascii::{App, BumpAttacks, Effect, Input, Key};
use tor_client_common::{test_support, ClientState};
use tor_protocol::*;

fn search() -> AbilityView {
    let mut ability = test_support::ability("fixture.search");
    ability.name = "Search".into();
    ability.reach = AbilityReach::SelfOnly;
    ability.target_kind = AbilityTargetKind::SelfOnly;
    ability
}

fn snapshot() -> Snapshot {
    let mut view = test_support::observation();
    view.self_target = ActorTarget::from_digest([41; 32]);
    view.visible_actors.clear();
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![test_support::ability("basic_melee"), search()];
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},
        "context":{"stream":"controls-attachment","epoch":"0"},"actor":"1","branch":"controls",
        "cursor":{"sequence":"0","tick":"0"},"has_control":true,"intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"snapshot":"controls-snapshot","wizard_game":false,"revision":"1","observation":view}
    }))
    .unwrap()
}

fn app(snapshot: Snapshot) -> App {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    app.messages.skip();
    app
}

fn key(app: &mut App, key: Key) -> Effect {
    app.input(Input::Key { key })
}

fn text(app: &mut App, value: &str) -> Effect {
    app.input(Input::Text { text: value.into() })
}

fn action(effect: Effect) -> Action {
    match super::request(effect) {
        Some(Request::Command {
            command: Command::Act { action },
            ..
        }) => action,
        other => panic!("expected one gameplay action, got {other:?}"),
    }
}

fn expected_search() -> Action {
    Action::UseAbility {
        id: "fixture.search".into(),
        invocation: AbilityInvocation {
            aim: AbilityAim::ActorTarget {
                target: ActorTarget::from_digest([41; 32]),
            },
            weapon: WeaponSelection::Default,
        },
        supplies: SupplySelection::Carried,
    }
}

#[test]
fn search_shortcut_and_menu_submit_the_same_disclosed_self_target() {
    for through_menu in [false, true] {
        let mut app = app(snapshot());
        let before = app.state.as_ref().unwrap().session().clone();
        if through_menu {
            assert_eq!(key(&mut app, Key::Abilities), Effect::None);
            assert_eq!(app.ability_choices, vec![search()]);
            assert_eq!(app.state.as_ref().unwrap().session(), &before);
        }
        let input = if through_menu {
            Key::Enter
        } else {
            Key::Search
        };
        assert_eq!(action(key(&mut app, input)), expected_search());
        assert!(app.busy);
        assert!(app.ability_choices.is_empty());
        assert!(app.selected_ability.is_none());
        assert!(app.attack_targets.is_empty());
        assert_eq!(
            app.state.as_ref().unwrap().session(),
            &before,
            "only the server pays and changes state"
        );
        assert_eq!(
            key(&mut app, Key::Search),
            Effect::None,
            "one outstanding input cannot submit twice"
        );
    }
}

#[test]
fn named_search_requires_a_unique_disclosed_self_only_grant() {
    // Establish the positive fixture before mutating a single grant dimension.
    assert_eq!(
        action(key(&mut app(snapshot()), Key::Search)),
        expected_search()
    );
    for case in ["absent", "ambiguous", "wrong-kind", "no-stats"] {
        let mut snapshot = snapshot();
        let view = &mut Arc::make_mut(&mut snapshot.state).observation;
        let stats = view.combat.as_mut().unwrap().own_stats.as_mut().unwrap();
        match case {
            "absent" => stats.abilities.retain(|ability| ability.basic_attack),
            "ambiguous" => {
                let mut duplicate = search();
                duplicate.id = "other.search".into();
                stats.abilities.push(duplicate);
            }
            "wrong-kind" => {
                stats.abilities[1].target_kind = AbilityTargetKind::Actor;
                stats.abilities[1].reach = AbilityReach::Sight { range: 6 };
            }
            "no-stats" => view.combat.as_mut().unwrap().own_stats = None,
            _ => unreachable!(),
        }
        let mut app = app(snapshot);
        let before = app.state.as_ref().unwrap().session().clone();
        assert_eq!(key(&mut app, Key::Search), Effect::None, "{case}");
        assert!(!app.busy, "{case}");
        assert!(
            app.ability_choices.is_empty() && app.attack_targets.is_empty(),
            "{case}"
        );
        assert_eq!(app.state.as_ref().unwrap().session(), &before, "{case}");
        assert!(
            app.status.contains(if case == "ambiguous" {
                "ambiguous"
            } else {
                "disclosed"
            }),
            "{case}: {}",
            app.status
        );
    }
}

#[test]
fn search_and_extended_dispatch_preserve_authority_and_readiness() {
    for through_command in [false, true] {
        for case in [
            "no-control",
            "spectator",
            "busy",
            "not-admitted",
            "disconnected",
        ] {
            let mut snapshot = snapshot();
            if case == "no-control" {
                snapshot.has_control = false;
                snapshot.readiness.admission = false;
            }
            if case == "not-admitted" {
                snapshot.readiness.admission = false;
            }
            let mut app = app(snapshot);
            // The editor may already be open when readiness/control changes.
            if through_command {
                assert_eq!(key(&mut app, Key::Commands), Effect::None);
                assert_eq!(text(&mut app, "search"), Effect::None);
            }
            if case == "spectator" {
                app.role = AccessRole::Spectator;
            }
            if case == "busy" {
                app.busy = true;
            }
            if case == "disconnected" {
                app.connected = false;
            }
            let before = app.state.as_ref().unwrap().session().clone();
            let input = if through_command {
                Key::Enter
            } else {
                Key::Search
            };
            assert_eq!(
                key(&mut app, input),
                Effect::None,
                "{case}, extended={through_command}"
            );
            assert!(app.attack_targets.is_empty());
            assert_eq!(
                app.state.as_ref().unwrap().session(),
                &before,
                "{case}, extended={through_command}"
            );
        }
    }
}

#[test]
fn command_editor_is_free_modal_cancellable_and_reopens_cleanly() {
    let mut app = app(snapshot());
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(key(&mut app, Key::Commands), Effect::None);
    assert!(app.command_rows().iter().any(|row| row.contains("#search")));
    assert!(app.command_rows().iter().any(|row| row.contains("#charts")));
    assert_eq!(text(&mut app, "not-a-command"), Effect::None);
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert!(app.status.contains("No supported command"));
    assert_eq!(app.command_line.as_deref(), Some("not-a-command"));
    assert_eq!(
        key(&mut app, Key::Search),
        Effect::None,
        "letters inside the editor are not paid shortcuts"
    );
    assert_eq!(
        key(&mut app, Key::SaveQuit),
        Effect::None,
        "S inside the editor does not close the application"
    );
    assert_eq!(key(&mut app, Key::Escape), Effect::None);
    assert!(app.command_line.is_none());
    assert_eq!(app.state.as_ref().unwrap().session(), &before);
    assert_eq!(key(&mut app, Key::Commands), Effect::None);
    assert_eq!(app.command_line.as_deref(), Some(""));
    assert_eq!(text(&mut app, "search"), Effect::None);
    assert_eq!(action(key(&mut app, Key::Enter)), expected_search());
    assert!(app.command_line.is_none());
}

#[test]
fn spectator_can_open_commands_but_paid_dispatch_stays_read_only() {
    let mut app = app(snapshot());
    app.role = AccessRole::Spectator;
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(key(&mut app, Key::Commands), Effect::None);
    assert_eq!(text(&mut app, "search"), Effect::None);
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert!(app.status.contains("read-only"));
    assert_eq!(app.state.as_ref().unwrap().session(), &before);
    assert_eq!(key(&mut app, Key::Commands), Effect::None);
    assert_eq!(text(&mut app, "stats"), Effect::None);
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert!(
        app.stats_open,
        "read-only commands remain useful to spectators"
    );
    assert_eq!(app.state.as_ref().unwrap().session(), &before);
}

#[test]
fn escape_never_quits_and_save_quit_is_explicit() {
    for spectator in [false, true] {
        let mut app = app(snapshot());
        if spectator {
            app.role = AccessRole::Spectator;
        }
        assert_eq!(key(&mut app, Key::Escape), Effect::None);
        assert_eq!(key(&mut app, Key::Inventory), Effect::None);
        assert_eq!(key(&mut app, Key::Escape), Effect::None);
        assert!(!app.inventory_open);
        assert_eq!(key(&mut app, Key::SaveQuit), Effect::Quit);
    }
}

#[test]
fn movement_prefix_cancellation_never_executes_the_following_command() {
    for prefix in [Key::RunPrefix, Key::MoveOnlyPrefix] {
        let mut app = app(snapshot());
        let before = app.state.as_ref().unwrap().session().clone();
        assert_eq!(key(&mut app, prefix), Effect::None);
        assert_eq!(key(&mut app, Key::Escape), Effect::None);
        assert_eq!(app.state.as_ref().unwrap().session(), &before);
        assert_eq!(key(&mut app, prefix), Effect::None);
        assert_eq!(
            key(&mut app, Key::Search),
            Effect::None,
            "non-direction cancels, not acts"
        );
        assert_eq!(app.state.as_ref().unwrap().session(), &before);
        assert_eq!(action(key(&mut app, Key::Search)), expected_search());
    }
}

#[test]
fn move_only_prefix_preserves_step_instead_of_bump_attack_and_restores_policy() {
    let mut snapshot = snapshot();
    let view = &mut Arc::make_mut(&mut snapshot.state).observation;
    let target = ActorTarget::from_digest([42; 32]);
    view.visible_actors.push(ActorView {
        id: target,
        name: "sentinel".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
        appearance_seed: None,
        worn: vec![],
        asset: None,
    });
    view.combat.as_mut().unwrap().actors.push(CombatActorView {
        actor: target,
        hostile: true,
        allied: false,
        injury: Injury::Healthy,
    });
    let mut ordinary = app(snapshot.clone());
    assert!(
        matches!(action(key(&mut ordinary, Key::Right)), Action::UseAbility { ref id, .. } if id == "basic_melee")
    );
    let mut moving = app(snapshot);
    let before = moving.state.as_ref().unwrap().session().clone();
    assert_eq!(moving.bump_attacks, BumpAttacks::Hostile);
    assert_eq!(key(&mut moving, Key::MoveOnlyPrefix), Effect::None);
    assert_eq!(
        action(key(&mut moving, Key::Right)),
        Action::Move {
            direction: Direction::East
        }
    );
    assert_eq!(moving.bump_attacks, BumpAttacks::Hostile);
    assert_eq!(moving.state.as_ref().unwrap().session(), &before);
}

#[test]
fn search_key_pages_unread_messages_before_any_paid_action() {
    let mut app = app(snapshot());
    let before = app.state.as_ref().unwrap().session().clone();
    for index in 0..8 {
        app.messages
            .push(format!("Message {index}: {}", "unread words ".repeat(12)));
    }
    assert!(app.messages.more());
    assert_eq!(key(&mut app, Key::Search), Effect::None);
    assert!(!app.busy);
    assert_eq!(app.state.as_ref().unwrap().session(), &before);
    app.messages.skip();
    assert_eq!(action(key(&mut app, Key::Search)), expected_search());
}

#[test]
fn run_prefix_uses_the_existing_run_and_any_key_still_stops_it() {
    let mut snapshot = snapshot();
    let view = &mut Arc::make_mut(&mut snapshot.state).observation;
    for x in 0..=2 {
        for z in -1..=2 {
            view.visible_cells.push(CellView {
                key: format!("run:{x}:{z}"),
                position: Position { x, y: 0, z },
                wall: z == -1 || z == 2,
                material: String::new(),
                lit: true,
                door: None,
                trap: None,
                stairs_up: false,
                stairs_down: false,
                place_hint: false,
                object_presentation: None,
                asset: None,
            });
        }
    }
    let mut direct = app(snapshot.clone());
    assert_eq!(
        action(key(&mut direct, Key::RunRight)),
        Action::Move {
            direction: Direction::East
        }
    );
    let mut prefixed = app(snapshot);
    let before = prefixed.state.as_ref().unwrap().session().clone();
    assert_eq!(key(&mut prefixed, Key::RunPrefix), Effect::None);
    assert_eq!(
        action(key(&mut prefixed, Key::Right)),
        Action::Move {
            direction: Direction::East
        }
    );
    assert_eq!(prefixed.running, Some(Direction::East));
    assert_eq!(prefixed.running, direct.running);
    assert_eq!(prefixed.state.as_ref().unwrap().session(), &before);
    assert_eq!(
        key(&mut prefixed, Key::Search),
        Effect::None,
        "a key stops running rather than taking an extra turn"
    );
    assert_eq!(prefixed.running, None);
    assert_eq!(prefixed.state.as_ref().unwrap().session(), &before);
}

#[test]
fn extended_command_abbreviation_requires_uniqueness_or_explicit_menu_choice() {
    let mut app = app(snapshot());
    assert_eq!(key(&mut app, Key::Commands), Effect::None);
    assert_eq!(text(&mut app, "s"), Effect::None);
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert!(app.status.contains("ambiguous"));
    assert_eq!(app.command_line.as_deref(), Some("s"));
    assert_eq!(app.state.as_ref().unwrap().session(), &before);
    // Explicit navigation authorizes the highlighted menu entry, even at row 0.
    assert_eq!(key(&mut app, Key::Up), Effect::None);
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert!(app.command_line.is_none());
    assert_eq!(app.ability_choices, vec![search()]);
    assert_eq!(app.state.as_ref().unwrap().session(), &before);
    assert_eq!(key(&mut app, Key::Escape), Effect::None);
    assert_eq!(key(&mut app, Key::Commands), Effect::None);
    assert_eq!(text(&mut app, "searc"), Effect::None);
    assert_eq!(action(key(&mut app, Key::Enter)), expected_search());
    let help = tor_client_ascii::HELP.join("\n");
    for hint in ["s       Search", "z Z", "r a", "#", "S       save"] {
        assert!(help.contains(hint), "missing help hint: {hint}");
    }
}

#[test]
fn move_only_prefix_survives_an_intervening_picture_without_becoming_an_attack() {
    for delta in [false, true] {
        let initial = snapshot();
        let mut app = app(initial.clone());
        assert_eq!(key(&mut app, Key::MoveOnlyPrefix), Effect::None);
        let mut after = initial.state.as_ref().clone();
        after.revision += 1;
        after.snapshot = SnapshotId("new-occupied-picture".into());
        let target = ActorTarget::from_digest([42; 32]);
        after.observation.visible_actors.push(ActorView {
            id: target,
            name: "new sentinel".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
            appearance_seed: None,
            worn: vec![],
            asset: None,
        });
        after
            .observation
            .combat
            .as_mut()
            .unwrap()
            .actors
            .push(CombatActorView {
                actor: target,
                hostile: true,
                allied: false,
                injury: Injury::Healthy,
            });
        let body = if delta {
            UpdateBody::ObservationDelta {
                base: ObservationBase {
                    snapshot: initial.state.snapshot.clone(),
                    cursor: initial.cursor,
                    revision: initial.state.revision,
                },
                state: Box::new(StateDelta::between(&initial.state, &after).unwrap()),
                event: None,
            }
        } else {
            UpdateBody::Observation {
                state: after.into(),
                event: None,
            }
        };
        app.update(StreamUpdate {
            context: initial.context,
            actor: initial.actor,
            branch: initial.branch,
            cursor: StreamCursor {
                sequence: 1,
                tick: 0,
            },
            body,
        })
        .unwrap();
        assert_eq!(
            action(key(&mut app, Key::Right)),
            Action::Move {
                direction: Direction::East
            },
            "the explicit no-attack prefix survives a {} picture",
            if delta { "delta" } else { "full" }
        );
    }
}

#[test]
fn reset_or_control_interruption_consumes_pending_direction_instead_of_reinterpreting_it() {
    for interruption in ["reset", "control", "disconnect"] {
        let mut app = app(snapshot());
        assert_eq!(key(&mut app, Key::MoveOnlyPrefix), Effect::None);
        match interruption {
            "reset" => {
                let mut fresh = snapshot();
                fresh.context.epoch += 1;
                app.replace_snapshot(fresh).unwrap();
            }
            "control" => {
                let initial = snapshot();
                for (sequence, has_control) in [(1, false), (2, true)] {
                    app.update(StreamUpdate {
                        context: initial.context.clone(),
                        actor: initial.actor,
                        branch: initial.branch.clone(),
                        cursor: StreamCursor { sequence, tick: 0 },
                        body: UpdateBody::Control { has_control },
                    })
                    .unwrap();
                    assert_eq!(app.state.as_ref().unwrap().has_control(), has_control);
                }
            }
            _ => {
                app.disconnect("interrupted".into());
                assert_eq!(key(&mut app, Key::Right), Effect::None);
                assert!(app.status.contains("not available"));
                assert!(!app.connected);
                app.set_state(ClientState::from_snapshot(snapshot()).unwrap());
                app.ready();
                app.messages.skip();
                assert_eq!(action(key(&mut app, Key::Search)), expected_search());
                continue;
            }
        }
        app.ready();
        app.messages.skip();
        assert_eq!(key(&mut app, Key::Right), Effect::None, "{interruption}");
        assert!(
            app.status.contains("cancelled"),
            "{interruption}: {}",
            app.status
        );
        assert_eq!(action(key(&mut app, Key::Search)), expected_search());
    }
}

#[test]
fn throw_and_fire_shortcuts_query_only_the_matching_disclosed_projectile_grant() {
    for (input, name, id) in [
        (Key::Throw, "Throw", "fixture.toss"),
        (Key::Fire, "Launch", "fixture.launcher"),
    ] {
        for through_menu in [false, true] {
            let mut snap = snapshot();
            let mut projectile = test_support::ability(id);
            projectile.name = name.into();
            projectile.target_kind = AbilityTargetKind::DirectionalProjectile;
            projectile.reach = AbilityReach::Projectile;
            Arc::make_mut(&mut snap.state)
                .observation
                .combat
                .as_mut()
                .unwrap()
                .own_stats
                .as_mut()
                .unwrap()
                .abilities = vec![projectile];
            let mut app = app(snap);
            let before = app.state.as_ref().unwrap().session().clone();
            let input = if through_menu {
                assert_eq!(key(&mut app, Key::Abilities), Effect::None);
                Key::Enter
            } else {
                input
            };
            assert!(
                matches!(super::request(key(&mut app, input)), Some(Request::Command {
                command: Command::ProjectileChoices { expected_revision: 1,
                    query: ProjectileChoiceQuery { ability, phase: ProjectileChoicePhase::Sources {}, offset: 0 } }, ..
            }) if ability == id)
            );
            assert!(app.projectile_selection.is_some());
            assert!(app.attack_targets.is_empty() && app.selected_ability.is_none());
            assert_eq!(
                app.state.as_ref().unwrap().session(),
                &before,
                "opening a source query cannot admit or pay for a projectile"
            );
        }
        let mut absent = app(snapshot());
        assert_eq!(key(&mut absent, input), Effect::None);
        assert!(absent.projectile_selection.is_none());
        assert!(absent.status.contains("disclosed"));
    }
}
