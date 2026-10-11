use std::sync::Arc;
use tor_client_ascii::{glyph_at, App, Effect, Input, Key};
use tor_client_common::ClientState;
use tor_protocol::*;

fn creature_snapshot() -> Snapshot {
    let mut snapshot = state().snapshot();
    Arc::make_mut(&mut snapshot.state).observation.combat = Some(serde_json::from_value(serde_json::json!({
        "hp": 7, "max_hp": 12, "preparation_remaining": null, "preparation_active": false,
        "recovery_remaining": "0", "actors": [], "events": [], "objective": null,
        "victory": false, "dead": false, "terminal": false,
        "own_stats": {
            "kind": "humanoid", "subtypes": [], "hit_dice": ["warrior"],
            "attributes": {"strength": 2, "speed": 1, "intellect": 3, "willpower": 2, "awareness": 1, "presence": 0},
            "skills": (["athletics", "heavy_weaponry", "agility", "light_weaponry", "stealth", "thievery", "crafting", "deduction", "lore", "medicine", "discipline", "intimidation", "insight", "perception", "survival", "deception", "leadership", "persuasion", "spellcasting"].into_iter().map(|skill| serde_json::json!({"skill": skill, "rank": 0})).collect::<Vec<_>>()),
            "defenses": {"physical": 13, "cognitive": 15, "spiritual": 11},
            "binding": "intellect", "resources": (["stamina", "focus", "mana"].into_iter().map(|resource| serde_json::json!({"resource": resource, "balance": 2, "maximum": 3, "available": 1, "reserved": 1})).collect::<Vec<_>>()),
            "active_talents": [], "dormant_talents": [], "abilities": ["basic_melee", "magic_bolt"]
        }
    })).unwrap());
    snapshot
}

#[test]
fn stats_screen_is_readonly_scrollable_and_available_to_spectators() {
    let snapshot = creature_snapshot();
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.role = AccessRole::Spectator;
    app.ready();
    let before = app.state.as_ref().unwrap().snapshot();
    assert_eq!(app.input(Input::Key { key: Key::Stats }), Effect::None);
    assert!(app.stats_open);
    let rows = app.stats_rows().into_owned();
    assert!(rows
        .iter()
        .any(|row| row.contains("Mana: 2/3 (1 available, 1 reserved)")));
    assert!(rows.len() > tor_client_ascii::STATS_ROWS);
    for _ in 0..100 {
        assert_eq!(app.input(Input::Key { key: Key::Down }), Effect::None);
    }
    assert_eq!(app.stats_scroll, rows.len() - tor_client_ascii::STATS_ROWS);
    assert_eq!(app.input(Input::Key { key: Key::Attack }), Effect::None);
    assert!(app.stats_open);
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    assert_eq!(app.input(Input::Key { key: Key::Stats }), Effect::None);
    assert!(!app.stats_open);
}

#[test]
fn ability_menu_selects_a_granted_technique_then_an_opaque_target_without_paying() {
    for (index, ability) in [Ability::PowerStrike, Ability::MagicBolt, Ability::Fear]
        .into_iter()
        .enumerate()
    {
        let mut snapshot = creature_snapshot();
        let observation = &mut Arc::make_mut(&mut snapshot.state).observation;
        let stats = observation
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap();
        stats.abilities = vec![
            Technique::BasicMelee,
            Technique::PowerStrike,
            Technique::MagicBolt,
            Technique::Fear,
        ];
        for pool in &mut stats.resources {
            pool.balance = 0;
            pool.available = 0;
            pool.reserved = 0;
        }
        let target = super::actor_target(42);
        let actor = ActorView {
            asset: None,
            id: target,
            name: "goblin".into(),
            description: String::new(),
            position: Position { x: 2, y: 1, z: 0 },
        };
        observation.visible_actors = vec![
            actor.clone(),
            ActorView {
                position: Position { x: 3, y: 1, z: 0 },
                ..actor
            },
        ];
        let mut app = App::default();
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        app.role = AccessRole::Player;
        app.ready();
        app.messages.skip();
        let before = app.state.as_ref().unwrap().snapshot();
        assert_eq!(
            app.input(Input::Key {
                key: Key::Abilities
            }),
            Effect::None
        );
        assert_eq!(app.ability_choices.len(), 3);
        for _ in 0..index {
            app.input(Input::Key { key: Key::Down });
        }
        assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
        assert_eq!(app.selected_ability, Some(ability));
        assert_eq!(
            app.attack_targets.len(),
            1,
            "portal occurrences must not duplicate targets"
        );
        assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
        assert!(
            matches!(app.input(Input::Key { key: Key::Enter }), Effect::Request(Request::Command {
            command: Command::Act { expected_revision: 3, action: Action::UseAbility { ability: selected, target: selected_target } }, ..
        }) if selected == ability && selected_target == target)
        );
        assert_eq!(
            app.state.as_ref().unwrap().snapshot(),
            before,
            "admission sends a request without spending locally"
        );
    }
}

#[test]
fn ability_selection_cancels_cleanly_and_clears_when_grants_or_control_change() {
    let mut snapshot = creature_snapshot();
    Arc::make_mut(&mut snapshot.state)
        .observation
        .visible_actors
        .push(ActorView {
            asset: None,
            id: super::actor_target(42),
            name: "goblin".into(),
            description: String::new(),
            position: Position { x: 2, y: 1, z: 0 },
        });
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    app.role = AccessRole::Player;
    app.ready();
    app.messages.skip();
    assert_eq!(
        app.input(Input::Key {
            key: Key::Abilities
        }),
        Effect::None
    );
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(app.ability_choices.is_empty());
    app.input(Input::Key {
        key: Key::Abilities,
    });
    app.input(Input::Key { key: Key::Enter });
    assert_eq!(app.selected_ability, Some(Ability::MagicBolt));
    app.input(Input::Key { key: Key::Escape });
    assert!(app.selected_ability.is_none());
    assert!(app.attack_targets.is_empty());
    app.input(Input::Key {
        key: Key::Abilities,
    });
    let view = Arc::make_mut(&mut snapshot.state);
    view.revision += 1;
    view.observation
        .combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![Technique::BasicMelee];
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    assert!(app.ability_choices.is_empty());
    assert!(app.selected_ability.is_none());
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    app.role = AccessRole::Spectator;
    assert_eq!(
        app.input(Input::Key {
            key: Key::Abilities
        }),
        Effect::None
    );
    assert!(app.status.contains("read-only"));
    assert!(app.ability_choices.is_empty());
    app.role = AccessRole::Player;
    snapshot.has_control = false;
    snapshot.readiness.admission = false;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert_eq!(
        app.input(Input::Key {
            key: Key::Abilities
        }),
        Effect::None
    );
    assert!(app.status.contains("observing"));
}

#[test]
fn ability_preparation_history_names_the_technique() {
    let target = ActorTarget::from_digest([2; 32]);
    for (ability, text) in [
        (Ability::PowerStrike, "Prepared power strike."),
        (Ability::MagicBolt, "Prepared magic bolt."),
        (Ability::Fear, "Prepared fear."),
    ] {
        let entry = HistoryEntry {
            id: EntryId("ability".into()),
            branch: BranchId("test".into()),
            actor: ActorId(1),
            tick: 0,
            author: Author::User {
                user: "test".into(),
            },
            audience: Audience::Actor,
            content: HistoryContent::Action {
                action: Action::UseAbility { ability, target },
                event: Event::AbilityStarted { ability, target },
            },
        };
        assert_eq!(tor_client_ascii::history_text(&entry), text);
    }
}

fn carried_rings() -> Snapshot {
    let mut snapshot = state().snapshot();
    let view = &mut Arc::make_mut(&mut snapshot.state).observation;
    let mut first = view.ground_items[0].item.clone();
    first.id = super::item_target(250);
    first.name = "silver ring".into();
    first.class = ItemClass::Ring;
    first.identified = false;
    let mut second = first.clone();
    second.id = super::item_target(1);
    view.inventory = vec![first, second];
    view.interactions = Some(InteractionView {
        completed: Vec::new(),
        slots: vec![EquipmentSlot::Ring, EquipmentSlot::Ring],
        preparation: None,
        inventory: view
            .inventory
            .iter()
            .map(|item| ItemInteractionView {
                item: item.id,
                slot: Some(EquipmentSlot::Ring),
                equipped_slot: None,
                known_equipment: None,
                drinkable: false,
            })
            .collect(),
    });
    snapshot
}

#[test]
fn item_picker_preserves_disclosed_order_and_uses_free_socket_without_counts() {
    let snapshot = carried_rings();
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Equip }), Effect::None);
    assert_eq!(
        app.pickup.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![super::item_target(250), super::item_target(1)]
    );
    app.input(Input::Text { text: "99".into() });
    assert!(app.quantity.is_empty(), "item work consumes one item");
    app.input(Input::Key { key: Key::Down });
    assert!(matches!(app.input(Input::Key { key: Key::Enter }),
        Effect::Request(Request::Command { command: Command::Act {
            action: Action::Equip { item, slot: 0 }, .. }, .. }) if item == super::item_target(1)));
}

#[test]
fn item_selection_cancels_and_is_invalidated_by_state_or_control_changes() {
    for change in ["escape", "revision", "control", "disconnect"] {
        let mut snapshot = carried_rings();
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
        app.ready();
        app.input(Input::Key { key: Key::Equip });
        assert_eq!(app.pickup.len(), 2);
        match change {
            "escape" => {
                assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
            }
            "revision" => {
                Arc::make_mut(&mut snapshot.state).revision += 1;
                snapshot.readiness.revision = snapshot.state.revision;
                app.set_state(ClientState::from_snapshot(snapshot).unwrap());
            }
            "control" => {
                snapshot.has_control = false;
                snapshot.readiness.admission = false;
                app.set_state(ClientState::from_snapshot(snapshot).unwrap());
            }
            _ => app.disconnect("disconnected".into()),
        }
        assert!(app.pickup.is_empty(), "{change}");
        assert!(app.item_operation.is_none(), "{change}");
        assert_eq!(
            app.input(Input::Key { key: Key::Enter }),
            Effect::None,
            "{change}"
        );
    }
}

#[test]
fn equipment_picker_keeps_blocked_choices_and_explains_removal() {
    let mut snapshot = carried_rings();
    Arc::make_mut(&mut snapshot.state)
        .observation
        .interactions
        .as_mut()
        .unwrap()
        .slots
        .clear();
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Equip }), Effect::None);
    assert_eq!(app.pickup.len(), 2);
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.status.contains("Remove"));
    assert!(!app.busy);
}

#[test]
fn opaque_bytes_do_not_reorder_pickup_or_drop_choices() {
    for key in [Key::Pickup, Key::Drop] {
        let mut snapshot = state().snapshot();
        let observation = &mut Arc::make_mut(&mut snapshot.state).observation;
        let mut first = observation.ground_items[0].clone();
        first.item.id = super::item_target(250);
        first.item.quantity = 10;
        let mut second = first.clone();
        second.item.id = super::item_target(1);
        second.item.quantity = 1;
        if key == Key::Drop {
            observation.inventory = vec![first.item.clone(), second.item.clone()];
        } else {
            let mut repeated = first.clone();
            repeated.position.x = 2;
            observation.ground_items = vec![first, second, repeated];
        }
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        app.ready();
        app.input(Input::Key { key });
        assert_eq!(
            app.pickup.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![super::item_target(250), super::item_target(1)]
        );
        app.input(Input::Text { text: "3".into() });
        let effect = app.input(Input::Key { key: Key::Enter });
        let expected = if key == Key::Drop {
            Action::Drop {
                item: super::item_target(250),
                quantity: Some(3),
            }
        } else {
            Action::Take {
                item: super::item_target(250),
                quantity: Some(3),
            }
        };
        assert!(matches!(effect, Effect::Request(Request::Command {
            command: Command::Act { action, .. }, ..
        }) if action == expected));
    }
}

#[test]
fn opaque_bytes_do_not_reorder_attack_choices_or_duplicate_body_cells() {
    let mut snapshot = state().snapshot();
    let first = ActorView {
        id: super::actor_target(250),
        name: "first figure".into(),
        description: String::new(),
        asset: None,
        position: Position { x: 1, y: 1, z: 0 },
    };
    let mut second = first.clone();
    second.id = super::actor_target(2);
    let mut repeated = first.clone();
    repeated.position.x = 2;
    Arc::make_mut(&mut snapshot.state)
        .observation
        .visible_actors = vec![first, second, repeated];
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    app.input(Input::Key { key: Key::Attack });
    assert_eq!(
        app.attack_targets
            .iter()
            .map(|actor| actor.id)
            .collect::<Vec<_>>(),
        vec![super::actor_target(250), super::actor_target(2)]
    );
}

#[test]
fn suspended_work_does_not_advertise_or_send_undisclosed_controls() {
    let mut snapshot = state().snapshot();
    snapshot.readiness.admission = false;
    snapshot.intentions = vec![IntentionStatus {
        actor: snapshot.actor,
        branch: snapshot.branch.clone(),
        intention: IntentionId("suspended".into()),
        entry_id: EntryId("root".into()),
        phase: IntentionPhase::Suspended,
    }];
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    let hint = app.intention_hint().unwrap();
    assert!(hint.starts_with("Action suspended."));
    assert!(!hint.contains("F8 resume"));
    assert!(!hint.contains("F9 cancel"));
    for key in [Key::ResumeIntention, Key::CancelIntention] {
        assert_eq!(app.input(Input::Key { key }), Effect::None);
    }
}

#[test]
fn action_admission_uses_disclosed_capacity_independently_of_simulation_turn() {
    for (due, admission) in [(true, false), (false, true)] {
        let mut snapshot = state().snapshot();
        Arc::make_mut(&mut snapshot.state).observation.ready = due;
        snapshot.readiness.admission = admission;
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        app.ready();
        let effect = app.input(Input::Key { key: Key::Right });
        assert_eq!(
            matches!(
                effect,
                Effect::Request(Request::Command {
                    command: Command::Act { .. },
                    ..
                })
            ),
            admission,
            "turn readiness must not substitute for admission permission"
        );
    }
}

#[test]
fn validated_lifecycle_updates_refresh_action_status_without_changing_spectator_status() {
    for role in [AccessRole::Player, AccessRole::Spectator] {
        let initial = state().snapshot();
        let mut app = App::new();
        app.role = role;
        app.set_state(ClientState::from_snapshot(initial.clone()).unwrap());
        let banner = "Spectator access is read-only.";
        app.status = if role == AccessRole::Player {
            "Action queued.".into()
        } else {
            banner.into()
        };
        let update = |sequence, phase, actor| StreamUpdate {
            context: fixture_context(),
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: initial.cursor.sequence + sequence,
                tick: initial.cursor.tick,
            },
            body: UpdateBody::Intention {
                status: IntentionStatus {
                    actor,
                    branch: initial.branch.clone(),
                    intention: IntentionId("original".into()),
                    entry_id: EntryId("admission".into()),
                    phase,
                },
            },
        };
        app.update(update(1, IntentionPhase::Queued, initial.actor))
            .unwrap();
        let before = app.status.clone();
        assert!(app
            .update(update(2, IntentionPhase::Resolved, ActorId(999)))
            .is_err());
        assert_eq!(app.status, before);
        assert!(app.state.as_ref().unwrap().has_pending_intention());
        app.update(update(2, IntentionPhase::Resolved, initial.actor))
            .unwrap();
        assert!(!app.state.as_ref().unwrap().has_pending_intention());
        assert!(app.intention_hint().is_none());
        assert_eq!(
            app.status,
            if role == AccessRole::Player {
                ""
            } else {
                banner
            }
        );
    }
}

#[test]
fn mixed_intention_hints_and_keys_select_queue_before_paused_progress() {
    for phase in [IntentionPhase::Queued, IntentionPhase::Suspended] {
        for queue_first in [false, true] {
            for key in [Key::ResumeIntention, Key::CancelIntention] {
                let mut snapshot = state().snapshot();
                snapshot.has_control = true;
                let progress = IntentionStatus {
                    actor: snapshot.actor,
                    branch: snapshot.branch.clone(),
                    intention: IntentionId("paused-progress".into()),
                    entry_id: EntryId("progress-root".into()),
                    phase: IntentionPhase::Paused,
                };
                let queued = IntentionStatus {
                    actor: snapshot.actor,
                    branch: snapshot.branch.clone(),
                    intention: IntentionId("queued-work".into()),
                    entry_id: EntryId("queue-root".into()),
                    phase,
                };
                snapshot.readiness.admission = false;
                snapshot.readiness.cancel = vec![
                    IntentionId("queued-work".into()),
                    IntentionId("paused-progress".into()),
                ];
                snapshot.readiness.resume = if phase == IntentionPhase::Suspended {
                    vec![IntentionId("queued-work".into())]
                } else {
                    vec![]
                };
                snapshot.intentions = if queue_first {
                    vec![queued, progress]
                } else {
                    vec![progress, queued]
                };
                let mut app = App::new();
                app.role = AccessRole::Player;
                app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
                app.ready();
                let hint = app.intention_hint().unwrap();
                assert!(
                    hint.starts_with(if phase == IntentionPhase::Queued {
                        "Action queued."
                    } else {
                        "Action suspended."
                    }),
                    "hint must describe the work selected by the controls: {hint}"
                );
                assert_eq!(
                    hint.contains("F8 resume"),
                    phase == IntentionPhase::Suspended
                );
                let effect = app.input(Input::Key { key });
                if key == Key::ResumeIntention && phase == IntentionPhase::Queued {
                    assert_eq!(effect, Effect::None);
                } else {
                    let Effect::Request(Request::Command {
                        branch, command, ..
                    }) = effect
                    else {
                        panic!("intention request");
                    };
                    let intention = match command {
                        Command::ResumeIntention { intention, .. }
                        | Command::CancelIntention { intention, .. } => intention,
                        _ => panic!("wrong command"),
                    };
                    assert_eq!(branch, snapshot.branch);
                    assert_eq!(intention.0, "queued-work");
                }
                assert_eq!(app.state.as_ref().unwrap().intentions().len(), 2);
            }
        }
    }
}

#[test]
fn intention_controls_preserve_identity_context_and_read_only_access() {
    for (phase, key, allowed) in [
        (IntentionPhase::Suspended, Key::ResumeIntention, true),
        (IntentionPhase::Suspended, Key::CancelIntention, true),
        (IntentionPhase::Queued, Key::ResumeIntention, false),
        (IntentionPhase::Queued, Key::CancelIntention, true),
        (IntentionPhase::Started, Key::ResumeIntention, false),
        (IntentionPhase::Started, Key::CancelIntention, true),
        (IntentionPhase::Paused, Key::ResumeIntention, true),
        (IntentionPhase::Paused, Key::CancelIntention, true),
    ] {
        for role in [AccessRole::Player, AccessRole::Spectator] {
            let mut snapshot = state().snapshot();
            snapshot.readiness.admission = false;
            snapshot.readiness.cancel = vec![IntentionId("original".into())];
            snapshot.readiness.resume =
                if matches!(phase, IntentionPhase::Suspended | IntentionPhase::Paused) {
                    vec![IntentionId("original".into())]
                } else {
                    vec![]
                };
            snapshot.intentions = vec![IntentionStatus {
                actor: snapshot.actor,
                branch: snapshot.branch.clone(),
                intention: IntentionId("original".into()),
                entry_id: EntryId("admission".into()),
                phase,
            }];
            let mut app = App::new();
            app.role = role;
            app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
            app.ready();
            let effect = app.input(Input::Key { key });
            if allowed && role == AccessRole::Player {
                let Effect::Request(Request::Command {
                    branch, command, ..
                }) = effect
                else {
                    panic!("intention request");
                };
                assert_eq!(branch, snapshot.branch);
                let (expected_revision, intention) = match command {
                    Command::ResumeIntention {
                        expected_revision,
                        intention,
                    }
                    | Command::CancelIntention {
                        expected_revision,
                        intention,
                    } => (expected_revision, intention),
                    _ => panic!("wrong command"),
                };
                assert_eq!(expected_revision, snapshot.state.revision);
                assert_eq!(intention.0, "original");
            } else {
                assert_eq!(effect, Effect::None);
            }
            if role == AccessRole::Spectator {
                assert!(app.status.contains("read-only"));
                assert!(app.intention_hint().is_none());
            } else if matches!(phase, IntentionPhase::Suspended | IntentionPhase::Paused) {
                assert!(app.intention_hint().unwrap().contains("F8 resume"));
            } else if phase == IntentionPhase::Started {
                assert!(app.intention_hint().unwrap().contains("F9 cancel"));
            }
        }
    }
}

#[test]
fn queued_intention_prevents_another_gameplay_request_even_when_observation_is_ready() {
    let mut snapshot = state().snapshot();
    snapshot.readiness.admission = false;
    snapshot.intentions = vec![IntentionStatus {
        actor: snapshot.actor,
        branch: snapshot.branch.clone(),
        intention: IntentionId("queued".into()),
        entry_id: EntryId("admission".into()),
        phase: IntentionPhase::Queued,
    }];
    assert!(snapshot.state.observation.ready);
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
    assert_eq!(app.status, "You can't act right now.");
}

#[test]
fn quantity_picker_submits_partial_pickup_and_drop() {
    let mut snapshot = state().snapshot();
    Arc::make_mut(&mut snapshot.state).observation.ground_items[0]
        .item
        .quantity = 10;
    let mut carried = snapshot.state.observation.ground_items[0].item.clone();
    carried.id = super::item_target(4);
    Arc::make_mut(&mut snapshot.state)
        .observation
        .inventory
        .push(carried);
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
                item: super::item_target(3),
                quantity: Some(3),
            }
        } else {
            Action::Drop {
                item: super::item_target(4),
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
    Arc::make_mut(&mut snapshot.state)
        .observation
        .places
        .push(PlaceView {
            key: "forgotten-cell".into(),
            name: "Quiet Reverie".into(),
            origin: PlaceNameOrigin::Authored,
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
            context: state().input_context(),
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
fn travel_accepts_remembered_cells_with_keyboard_and_mouse() {
    for mouse in [false, true] {
        let mut client = state();
        let mut snapshot = client.snapshot();
        snapshot.context.epoch += 1;
        Arc::make_mut(&mut snapshot.state)
            .observation
            .visible_cells
            .retain(|c| c.key != "2:1");
        client.replace_snapshot(snapshot).unwrap();
        assert!(client.map_memory().any(|c| c.key == "2:1"));
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(client);
        app.ready();
        let effect = if mouse {
            let (x, y) = tor_client_ascii::render::cell_center(
                &state().state().observation,
                Position { x: 2, y: 1, z: 0 },
            )
            .unwrap();
            app.input(Input::Click { x, y })
        } else {
            app.input(Input::Key { key: Key::Travel });
            app.input(Input::Key { key: Key::Right });
            app.input(Input::Key { key: Key::Enter })
        };
        assert!(
            matches!(effect, Effect::Request(Request::Command {
            command: Command::Travel { destination, .. }, ..
        }) if destination == "2:1"),
            "mouse={mouse}"
        );
    }
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
        id: super::actor_target(2),
        name: "guard".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
    });
    let mut canvas = tor_client_ascii::render::Canvas::default();
    canvas.draw(&app);
    let normal = canvas.pixels.clone();
    let mut snapshot = serde_json::to_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"new-branch","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],
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
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"wizard_game":false,"revision":"3","observation":{
            "actor":"1","self_target":super::actor_target(1),"tick":"0","position":{"x":1,"y":1,"z":0},

            "places":[],"visible_cells": (0..5).flat_map(|x| (0..3).map(move |y| serde_json::json!({"key":format!("{x}:{y}"),"stairs_up":false,"stairs_down":false,"position":{"x":x,"y":y,"z":0},"wall":false,"place_hint":false}))).collect::<Vec<_>>(),
            "ground_items":[{"reachable":true,"item":{"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(3),"name":"token"},"position":{"x":1,"y":1,"z":0}}],
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
            context: state().input_context(),
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
                    item,
                    quantity: None
                },
                ..
            },
            ..
        }) if item == super::item_target(3)
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
    assert_eq!(glyph_at(&other_level, 2, 1), '#');
    other_level.ground_items[0].position = Position { x: 3, y: 1, z: 0 };
    assert_eq!(glyph_at(&other_level, 3, 1), '(');
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
            context: fixture_context(),
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
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],
        "history":{"entries":[],"older_before":null},"state":state().state()
    }))
    .unwrap();
    let position = snapshot.state.observation.position;
    Arc::make_mut(&mut snapshot.state)
        .observation
        .ground_items
        .push(GroundItemView {
            reachable: true,
            item: ItemView {
                class: Default::default(),
                asset: None,
                quantity: 1,
                appearance: String::new(),
                identified: true,
                description: String::new(),
                id: super::item_target(4),
                name: "another token".into(),
            },
            position,
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
                    item,
                    quantity: None
                },
                ..
            },
            ..
        }) if item == super::item_target(4)
    ));
    app.ready();
    app.input(Input::Key { key: Key::Pickup });
    Arc::make_mut(&mut snapshot.state).revision += 1;
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
            readiness: tor_protocol::Readiness {
                revision: 0,
                admission: false,
                resume: vec![],
                cancel: vec![],
            },
            context: fixture_context(),
            intentions: Vec::new(),
            travel: None,
            actor: ActorId(1),
            branch: original.branch().clone(),
            cursor: original.cursor(),
            state: view.into(),
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
fn resized_clicks_use_the_rendered_cell_layout_and_ignore_far_heights() {
    use tor_client_ascii::render::{cell_at, cell_center, logical_mouse};
    let mut view = state().state().observation.clone();
    // A stair's far landing shares the column but is drawn nowhere.
    let mut landing = view.visible_cells[0].clone();
    landing.key = "landing".into();
    landing.position = Position { x: 2, y: 1, z: -17 };
    view.visible_cells.push(landing.clone());
    let mut snapshot = state().snapshot();
    Arc::make_mut(&mut snapshot.state).observation = view.clone();
    assert!(
        !tor_client_ascii::render::map_tiles(&ClientState::from_snapshot(snapshot).unwrap())
            .iter()
            .any(|t| t.position.z == -17)
    );
    for position in [view.position, Position { x: 3, y: 1, z: 0 }] {
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
    let (x, y) = cell_center(&view, Position { x: 2, y: 1, z: 0 }).unwrap();
    assert_eq!(cell_at(&view, x, y), Some(Position { x: 2, y: 1, z: 0 }));
    assert_eq!(logical_mouse(10.0, 100.0, 1600, 800), None);
}

#[test]
fn keys_skip_an_active_journey_and_changed_observations_clear_selection() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    app.input(Input::Key { key: Key::Travel });
    assert!(app.travel_cursor.is_some());
    let mut current = state();
    current
        .apply(StreamUpdate {
            context: fixture_context(),
            actor: ActorId(1),
            branch: BranchId("test".into()),
            cursor: StreamCursor {
                sequence: 1,
                tick: 100,
            },
            body: UpdateBody::Observation {
                state: (StateView {
                    revision: 4,
                    observation: Observation {
                        tick: 100,
                        ..current.state().observation.clone()
                    },
                    ..current.state().clone()
                })
                .into(),
                event: None,
            },
        })
        .unwrap();
    app.set_state(current.clone());
    assert!(app.travel_cursor.is_none());
    current
        .apply(StreamUpdate {
            context: fixture_context(),
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
    // Only the server ends a journey: any key shows the rest of it at once.
    for key in [Key::Escape, Key::Up, Key::Wait] {
        assert_eq!(app.input(Input::Key { key }), Effect::Skip);
    }
    assert_eq!(
        app.input(Input::Key { key: Key::Slower }),
        Effect::Pace(100)
    );
    assert_eq!(app.input(Input::Key { key: Key::Faster }), Effect::Pace(75));
    assert_eq!(app.input(Input::Key { key: Key::Faster }), Effect::Pace(50));
}

#[test]
fn door_glyphs_and_explicit_selection_submit_actions_without_movement() {
    let mut snapshot = serde_json::to_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],
        "history":{"entries":[],"older_before":null},"state":state().state()
    }))
    .unwrap();
    let cells = snapshot["state"]["observation"]["visible_cells"]
        .as_array_mut()
        .unwrap();
    for (index, id) in [(7, 11), (5, 12)] {
        cells[index]["door"] = serde_json::json!({"id":super::door_target(id),"name":"wooden door","description":"wood", "open":false,"reachable":true,"approaches":[]});
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
                    door,
                    open: true
                },
                ..
            },
            ..
        }) if door == super::door_target(12)
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
        let mut view = Arc::unwrap_or_clone(initial.state.clone());
        view.revision += sequence;
        view.observation.tick += sequence;
        view.observation.visible_cells[0].key = format!("intermediate-{sequence}");
        app.update(StreamUpdate {
            context: fixture_context(),
            actor: initial.actor,
            branch: initial.branch.clone(),
            cursor: StreamCursor {
                sequence: initial.cursor.sequence + sequence,
                tick: view.observation.tick,
            },
            body: UpdateBody::Observation {
                state: view.into(),
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
    rewind.context.epoch += 1;
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
            id: super::actor_target(2),
            name: "guard".into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        });
        view.observation.combat = Some(CombatView {
            own_stats: None,
            hp: 30,
            max_hp: 30,
            preparation_remaining: None,
            preparation_active: false,
            recovery_remaining: 0,
            actors: vec![CombatActorView {
                actor: super::actor_target(2),
                hostile,
                injury: Injury::Healthy,
            }],
            events: vec![],
            objective: None,
            exit: None,
            victory: false,
            dead: false,
            terminal: false,
        });
        let snapshot: Snapshot=serde_json::from_value(serde_json::json!({"readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],"history":{"entries":[],"older_before":null},"state":view})).unwrap();
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.bump_attacks = mode;
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
                Action::Attack {
                    target: super::actor_target(2),
                }
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
    let mut app = App::new();
    let mut canvas = tor_client_ascii::render::Canvas::default();
    app.status = "HP 20/20 — Victory!".into();
    canvas.draw(&app);
    let prose = canvas.pixels.clone();
    app.status = "HP 20/20 - Victory!".into();
    canvas.draw(&app);
    assert_eq!(canvas.pixels, prose);
}

/// Context for one synthetic attachment used by this fixture/workload.
fn fixture_context() -> tor_protocol::StreamContext {
    tor_protocol::StreamContext {
        stream: tor_protocol::StreamId("fixture-attachment".into()),
        epoch: 0,
    }
}

#[test]
fn privileged_inspection_is_authorized_scrollable_and_cleared_on_snapshot() {
    let snapshot = creature_snapshot();
    let combat = snapshot.state.observation.combat.as_ref().unwrap();
    let stats = combat.own_stats.clone().unwrap();
    let report = CreatureInspectionView {
        actor: ActorId(2),
        tick: 0,
        name: "blue sentinel".into(),
        faction: "blue".into(),
        species: InspectionSpecies {
            id: "human".into(),
            kind: CreatureType::Humanoid,
            subtypes: vec![],
            attributes: stats.attributes,
            melee: serde_json::from_value(serde_json::json!({"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"damage":{"primary":{"category":"impact","descriptor":null,"sides":6},"components":[{"category":"impact","descriptor":null,"amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}]}})).unwrap(),
        },
        initial_attributes: stats.attributes,
        templates: vec![],
        max_hp: 12,
        hp: 7,
        injury: 5,
        dead: false,
        stats,
        hit_dice: vec![InspectionHitDie {
            ordinal: 1,
            source: HitDieSource::Warrior,
            health_seed: u64::MAX,
            health_die: 8,
            base_health: 7,
            training: vec![],
            attribute: None,
            talent: None,
        }],
        grants: vec![],
        fear: vec![],
    };
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    for role in [AccessRole::Player, AccessRole::Spectator] {
        app.role = role;
        assert!(app.show_inspection(&report).is_err());
        assert!(!app.stats_open);
        assert!(!app.has_inspection());
    }
    app.role = AccessRole::Wizard;
    let before = app.state.as_ref().unwrap().snapshot();
    let mut invalid = report.clone();
    invalid.hp += 1;
    app.show_inspection(&report).unwrap();
    let displayed = app.stats_rows().to_vec();
    assert!(app.show_inspection(&invalid).is_err());
    assert_eq!(app.stats_rows().as_ref(), displayed);
    assert!(app.stats_open);
    assert!(app.stats_rows().join("\n").contains("blue sentinel"));
    for _ in 0..100 {
        app.input(Input::Key { key: Key::Down });
    }
    assert_eq!(
        app.stats_scroll,
        app.stats_rows().len() - tor_client_ascii::STATS_ROWS
    );
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    let mut reset = snapshot;
    reset.context.epoch += 1;
    app.replace_snapshot(reset).unwrap();
    assert!(!app.stats_open);
    assert!(!app.has_inspection());
}

#[test]
fn wizard_console_is_private_and_submits_the_current_input_context() {
    let mut app = App::new();
    app.set_state(state());
    app.ready();
    for role in [AccessRole::Player, AccessRole::Spectator] {
        app.role = role;
        assert_eq!(app.input(Input::Key { key: Key::Wizard }), Effect::None);
        assert!(app.wizard_command.is_none());
    }
    app.role = AccessRole::Wizard;
    let before = app.state.as_ref().unwrap().snapshot();
    assert_eq!(app.input(Input::Key { key: Key::Wizard }), Effect::None);
    assert_eq!(
        app.input(Input::Text {
            text: "creature inspect 2".into()
        }),
        Effect::None
    );
    let Effect::Request(Request::Command {
        context,
        branch,
        command,
    }) = app.input(Input::Key { key: Key::Enter })
    else {
        panic!("query request");
    };
    assert_eq!(context, before.reply_context().input);
    assert_eq!(branch, before.branch);
    assert_eq!(
        command,
        Command::Wizard {
            expected_revision: before.state.revision,
            operation: "creature inspect 2".into()
        }
    );
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    assert!(app.wizard_command.is_none());
    app.ready();
    app.input(Input::Key { key: Key::Wizard });
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(app.wizard_command.is_none());
}

#[test]
fn combat_diagnostics_presentation_is_private_and_atomic() {
    let report = tor_protocol::CombatDiagnosticsView {
        enabled: true,
        tick: 0,
        captured: 0,
        dropped: 0,
        retained: 0,
        through: 0,
        records: vec![],
    };
    let mut app = App::new();
    for role in [
        tor_protocol::AccessRole::Player,
        tor_protocol::AccessRole::Spectator,
    ] {
        app.role = role;
        assert!(app.show_combat_diagnostics(&report).is_err());
        assert!(!app.stats_open);
    }
    app.role = tor_protocol::AccessRole::Wizard;
    app.show_combat_diagnostics(&report).unwrap();
    let rows = app.stats_rows().to_vec();
    assert!(rows.join(" ").contains("Combat diagnostics"));
    assert!(app.stats_open);
    let mut invalid = report.clone();
    invalid.retained = 1;
    assert!(app.show_combat_diagnostics(&invalid).is_err());
    assert_eq!(app.stats_rows().as_ref(), rows);
}
