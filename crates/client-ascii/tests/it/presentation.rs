use std::sync::Arc;
use tor_client_ascii::{glyph_at, App, Effect, Input, Key};
use tor_client_common::test_support::equipment_slot;
use tor_client_common::ClientState;
use tor_protocol::*;

fn creature_snapshot() -> Snapshot {
    let mut snapshot = state().reset_snapshot().unwrap();
    Arc::make_mut(&mut snapshot.state).observation.combat = Some(serde_json::from_value(serde_json::json!({
        "public_plan":null,"hp": "7", "max_hp": "12", "sequence":null,"preparation_remaining": null, "preparation_active": false,
        "recovery_remaining": "0", "actors": [], "events": [], "objective": null,
        "victory": false, "dead": false, "terminal": false,
        "own_stats": {
                "nutrition":{"kind":"exempt"}, "conditions":[],
                "restorable_losses":{"hit_dice":"0","attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},"maximum_health":0},
                "health_recovery": {"period_ticks":"1200", "elapsed_ticks":"0", "stopped":false, "reset_percent":100},
            "kind": {"id":"humanoid","name":"Humanoid"}, "subtypes": [], "hit_dice": {"total":"1","sources":[{"source_kind":"class","source":{"id":"warrior","name":"Warrior"},"count":"1"}]},
            "attributes": {"strength": 2, "speed": 1, "intellect": 3, "willpower": 2, "awareness": 1, "presence": 0},
            "skills": (["athletics", "heavy_weaponry", "agility", "light_weaponry", "stealth", "thievery", "crafting", "deduction", "lore", "medicine", "discipline", "intimidation", "insight", "perception", "survival", "deception", "leadership", "persuasion", "spellcasting", "breath"].into_iter().map(|skill| serde_json::json!({"skill": tor_client_common::test_support::content(skill), "rank": 0})).collect::<Vec<_>>()),
            "defenses": {"physical": 13, "cognitive": 15, "spiritual": 11},
            "binding": {"id":"intellect","name":"Intellect"}, "resources": (["stamina", "focus", "mana"].into_iter().map(|resource| serde_json::json!({"resource": tor_client_common::test_support::content(resource), "balance": 2, "maximum": 3, "available": 1, "reserved": 1})).collect::<Vec<_>>()),
            "expertises": [], "active_talents": [], "dormant_talents": [], "abilities": [{"cooldown_remaining":"0","supply_costs":[],"id":"basic_melee","name":"Basic Melee","cost":null,"reach":{"kind":"melee"},"target_kind":"actor","basic_attack":true,"weapon_selection":true}, {"cooldown_remaining":"0","supply_costs":[],"id":"magic_bolt","name":"Magic Bolt","cost":null,"reach":{"kind":"sight","range":6},"target_kind":"actor","basic_attack":false,"weapon_selection":false}]
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
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(app.input(Input::Key { key: Key::Stats }), Effect::None);
    assert!(app.stats_open);
    let rows = app.stats_rows().into_owned();
    assert!(rows
        .iter()
        .any(|row| row.contains("Mana: 2/3 (1 available, 1 reserved)")));
    assert!(rows
        .iter()
        .any(|row| row.contains("Health recovery: 1 per 1200 active ticks")));
    assert!(rows.iter().any(|row| row.contains("Breath: 0")));
    assert!(rows.len() > tor_client_ascii::STATS_ROWS);
    for _ in 0..100 {
        assert_eq!(app.input(Input::Key { key: Key::Down }), Effect::None);
    }
    assert_eq!(app.stats_scroll, rows.len() - tor_client_ascii::STATS_ROWS);
    assert_eq!(app.input(Input::Key { key: Key::Attack }), Effect::None);
    assert!(app.stats_open);
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
    assert_eq!(app.input(Input::Key { key: Key::Stats }), Effect::None);
    assert!(!app.stats_open);
}

#[test]
fn ability_menu_selects_a_granted_technique_then_an_opaque_target_without_paying() {
    for (index, ability) in [
        tor_client_common::test_support::ability("power_strike"),
        tor_client_common::test_support::ability("magic_bolt"),
        tor_client_common::test_support::ability("fear"),
        tor_client_common::test_support::ability("shimmer_wave"),
    ]
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
            tor_client_common::test_support::ability("basic_melee"),
            tor_client_common::test_support::ability("power_strike"),
            tor_client_common::test_support::ability("magic_bolt"),
            tor_client_common::test_support::ability("fear"),
            tor_client_common::test_support::ability("shimmer_wave"),
        ];
        for pool in &mut stats.resources {
            pool.balance = 0;
            pool.available = 0;
            pool.reserved = 0;
        }
        let target = super::actor_target(42);
        let actor = ActorView {
            appearance_seed: None,
            worn: Vec::new(),
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
        let before = app.state.as_ref().unwrap().session().clone();
        assert_eq!(
            app.input(Input::Key {
                key: Key::Abilities
            }),
            Effect::None
        );
        assert_eq!(app.ability_choices.len(), 4);
        for _ in 0..index {
            app.input(Input::Key { key: Key::Down });
        }
        assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
        assert_eq!(app.selected_ability, Some(ability.clone()));
        assert_eq!(
            app.attack_targets.len(),
            1,
            "portal occurrences must not duplicate targets"
        );
        assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
        assert!(
            matches!(super::request(app.input(Input::Key { key: Key::Enter })), Some(Request::Command {
            command: Command::Act {  action: Action::UseAbility { id: selected, invocation: tor_protocol::AbilityInvocation { aim: tor_protocol::AbilityAim::ActorTarget { target: selected_target }, weapon: tor_protocol::WeaponSelection::Default }, ..} }, ..
        }) if selected == ability.id && selected_target == target)
        );
        assert_eq!(
            app.state.as_ref().unwrap().session().clone(),
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
            appearance_seed: None,
            worn: Vec::new(),
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
    assert_eq!(
        app.selected_ability,
        Some(tor_client_common::test_support::ability("magic_bolt"))
    );
    app.input(Input::Key { key: Key::Escape });
    assert!(app.selected_ability.is_none());
    assert!(app.attack_targets.is_empty());
    app.input(Input::Key {
        key: Key::Abilities,
    });
    let view = Arc::make_mut(&mut snapshot.state);
    view.revision += 1;
    view.snapshot = tor_protocol::SnapshotId(format!("fixture-display-{}", view.revision));
    view.observation
        .combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![tor_client_common::test_support::ability("basic_melee")];
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
        (
            tor_client_common::test_support::ability("power_strike"),
            "Prepared Power Strike.",
        ),
        (
            tor_client_common::test_support::ability("magic_bolt"),
            "Prepared Magic Bolt.",
        ),
        (
            tor_client_common::test_support::ability("fear"),
            "Prepared Fear.",
        ),
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
                action: Action::UseAbility {
                    id: ability.id.clone(),
                    invocation: tor_protocol::AbilityInvocation {
                        aim: tor_protocol::AbilityAim::ActorTarget { target },
                        weapon: tor_protocol::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
                event: Box::new(Event::AbilityStarted {
                    ability: ability.content(),
                    target,
                }),
            },
        };
        assert_eq!(tor_client_ascii::history_text(&entry), text);
    }
}

fn carried_rings() -> Snapshot {
    let mut snapshot = state().reset_snapshot().unwrap();
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
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        completed: Vec::new(),
        slots: vec![
            equipment_slot("ring", "on a finger", false),
            equipment_slot("ring", "on a finger", false),
        ],
        preparation: None,
        inventory: view
            .inventory
            .iter()
            .map(|item| ItemInteractionView {
                item: item.id,
                slot: Some(equipment_slot("ring", "on a finger", false)),
                equipped_slot: None,
                known_cursed: None,
                known_protected: None,
                known_projectile_bonus: None,
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: false,
                portable_light: None,
                device_activation: None,
                charges: None,
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
    assert!(
        matches!(super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command { command: Command::Act {
            action: Action::Equip { item, slot: 0 }, .. }, .. }) if item == super::item_target(1))
    );
}

#[test]
fn inventory_uses_a_custom_slots_disclosed_equipped_phrase() {
    let mut snapshot = carried_rings();
    let view = &mut Arc::make_mut(&mut snapshot.state).observation;
    let slot = equipment_slot("fixture_wing", "fastened to a wing", false);
    let interactions = view.interactions.as_mut().unwrap();
    interactions.slots = vec![slot.clone(); 2];
    for item in &mut interactions.inventory {
        item.slot = Some(slot.clone());
    }
    interactions.inventory[0].equipped_slot = Some(1);
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    let entries =
        tor_client_ascii::inventory_entries(&app, &app.state.as_ref().unwrap().state().observation);
    assert_eq!(entries[0].equipped.as_deref(), Some("fastened to a wing"));
    assert!(entries[0].long().contains("(fastened to a wing)"));
    assert_eq!(entries[1].equipped, None);
}

#[test]
fn item_picker_equips_a_second_disclosed_custom_attack_socket() {
    let mut snapshot = carried_rings();
    let view = &mut Arc::make_mut(&mut snapshot.state).observation;
    let slot = equipment_slot("fixture_wing", "held by a wing", true);
    let interactions = view.interactions.as_mut().unwrap();
    interactions.slots = vec![slot.clone(); 2];
    for item in &mut interactions.inventory {
        item.slot = Some(slot.clone());
    }
    interactions.inventory[0].equipped_slot = Some(0);
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Equip }), Effect::None);
    assert_eq!(app.pickup.len(), 1);
    assert!(
        matches!(super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command { command: Command::Act {
            action: Action::Equip { item, slot: 1 }, .. }, .. }) if item == super::item_target(1))
    );
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
                Arc::make_mut(&mut snapshot.state)
                    .snapshot
                    .0
                    .push_str("-next");
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
        let mut snapshot = state().reset_snapshot().unwrap();
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
        assert!(matches!(super::request(effect), Some(Request::Command {
            command: Command::Act { action, .. }, ..
        }) if action == expected));
    }
}

#[test]
fn opaque_bytes_do_not_reorder_attack_choices_or_duplicate_body_cells() {
    let mut snapshot = state().reset_snapshot().unwrap();
    let first = ActorView {
        appearance_seed: None,
        worn: Vec::new(),
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
    let mut snapshot = state().reset_snapshot().unwrap();
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
        let mut snapshot = state().reset_snapshot().unwrap();
        Arc::make_mut(&mut snapshot.state).observation.ready = due;
        snapshot.readiness.admission = admission;
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        app.ready();
        let effect = app.input(Input::Key { key: Key::Right });
        assert_eq!(
            matches!(
                super::request(effect),
                Some(Request::Command {
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
        let initial = state().reset_snapshot().unwrap();
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
                let mut snapshot = state().reset_snapshot().unwrap();
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
                    let Some(Request::Command {
                        branch, command, ..
                    }) = super::request(effect)
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
            let mut snapshot = state().reset_snapshot().unwrap();
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
                let Some(Request::Command {
                    branch,
                    context,
                    command,
                }) = super::request(effect)
                else {
                    panic!("intention request");
                };
                assert_eq!(branch, snapshot.branch);
                let intention = match command {
                    Command::ResumeIntention { intention }
                    | Command::CancelIntention {
                        intention,
                        step: None,
                    } => intention,
                    _ => panic!("wrong command"),
                };
                assert_eq!(
                    context,
                    InputContext {
                        stream: snapshot.context.clone(),
                        readiness_revision: snapshot.readiness.revision
                    }
                );
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
    let mut snapshot = state().reset_snapshot().unwrap();
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
    let mut snapshot = state().reset_snapshot().unwrap();
    Arc::make_mut(&mut snapshot.state).observation.ground_items[0]
        .item
        .quantity = 10;
    // Multiple floor stacks keep the explicit partial-quantity picker.
    let mut other = snapshot.state.observation.ground_items[0].clone();
    other.item.id = super::item_target(5);
    Arc::make_mut(&mut snapshot.state)
        .observation
        .ground_items
        .push(other);
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
            matches!(super::request(effect), Some(Request::Command { command: Command::Act { action, .. }, .. }) if action == expected)
        );
    }
}

#[test]
fn places_modal_displays_memory_and_renames_without_travel_or_time() {
    let mut snapshot = state().reset_snapshot().unwrap();
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
        matches!(super::request(app.input(Input::Key { key: Key::Enter })), Some(Request::Command {
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
        super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command {
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
        matches!(super::request(app.input(Input::Key {key:Key::Enter})),Some(Request::Command {command:Command::Travel {destination,..},..}) if destination=="2:0")
    );
    app.ready();
    for (key, direction) in [
        (Key::NorthEast, Direction::NorthEast),
        (Key::SouthEast, Direction::SouthEast),
        (Key::SouthWest, Direction::SouthWest),
        (Key::NorthWest, Direction::NorthWest),
    ] {
        assert!(
            matches!(super::request(app.input(Input::Key {key})),Some(Request::Command {command:Command::Act { action:Action::Move {direction:d}, },..}) if d==direction)
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
        matches!(super::request(app.input(Input::Click { x, y })), Some(Request::Command { command: Command::Travel { destination, .. }, .. }) if destination == "3:1")
    );
}

#[test]
fn travel_accepts_remembered_cells_with_keyboard_and_mouse() {
    for mouse in [false, true] {
        let mut client = state();
        let mut snapshot = client.reset_snapshot().unwrap();
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
            matches!(super::request(effect), Some(Request::Command {
            command: Command::Travel { destination, .. }, ..
        }) if destination == "2:1"),
            "mouse={mouse}"
        );
    }
}

#[test]
fn rewind_clears_old_drafts() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    app.input(Input::Key { key: Key::Note });
    assert!(app.note.is_some());
    app.attack_targets.push(ActorView {
        appearance_seed: None,
        worn: Vec::new(),
        asset: None,
        id: super::actor_target(2),
        name: "guard".into(),
        description: String::new(),
        position: Position { x: 1, y: 0, z: 0 },
    });
    let mut snapshot = serde_json::to_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"new-branch","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],
        "history":{"entries":[],"older_before":null},"state":state().state()
    }))
    .unwrap();
    snapshot["state"]["wizard_game"] = true.into();
    app.set_state(ClientState::from_snapshot(serde_json::from_value(snapshot).unwrap()).unwrap());
    assert!(app.note.is_none());
    assert!(app.attack_targets.is_empty());
}

#[test]
fn wizard_marker_changes_pixels_with_no_other_presentation_change() {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state());
    app.ready();
    let mut canvas = tor_client_ascii::render::Canvas::default();
    canvas.draw(&app);
    let normal = canvas.pixels.clone();
    let mut snapshot = serde_json::to_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},
        "context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test",
        "cursor":{"sequence":"0","tick":"0"},"has_control":true,"intentions":[],
        "history":{"entries":[],"older_before":null},"state":state().state()
    }))
    .unwrap();
    snapshot["state"]["wizard_game"] = true.into();
    app.set_state(ClientState::from_snapshot(serde_json::from_value(snapshot).unwrap()).unwrap());
    app.ready();
    canvas.draw(&app);
    assert_ne!(
        normal, canvas.pixels,
        "the wizard marker itself must be visible"
    );
}

fn state() -> ClientState {
    ClientState::from_snapshot(serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test","cursor":{"sequence":"0","tick":"0"},"has_control":true, "intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"snapshot":"fixture-snapshot","wizard_game":false,"revision":"3","observation":{"charts":[],"linked_holds":[],
            "actor":"1","self_target":super::actor_target(1),"tick":"0","position":{"x":1,"y":1,"z":0},

            "places":[],"visible_cells": (0..5).flat_map(|x| (0..3).map(move |y| serde_json::json!({"key":format!("{x}:{y}"),"trap":null,"object_presentation":null,"stairs_up":false,"stairs_down":false,"position":{"x":x,"y":y,"z":0},"wall":false,"lit":true,"place_hint":false}))).collect::<Vec<_>>(),
            "ground_items":[{"presentation":null,"supply_reachable":false,"reachable":true,"item":{"known_currency":null,"known_supply":null,"quantity":"1","class":"misc","appearance":"item","identified":true,"id":super::item_target(3),"name":"token"},"position":{"x":1,"y":1,"z":0}}],
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
        super::request(app.input(Input::Key { key: Key::Right })),
        Some(Request::Command {
            context: state().input_context(),
            branch: BranchId("test".into()),
            command: Command::Act {
                action: Action::Move {
                    direction: Direction::East
                }
            }
        })
    );
    assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
    app.ready();
    assert!(matches!(
        super::request(app.input(Input::Key { key: Key::Pickup })),
        Some(Request::Command {
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
    assert_eq!(glyph_at(o, &Default::default(), 1, 1), '@');
    assert_eq!(glyph_at(o, &Default::default(), 4, 1), '.');
    assert_eq!(glyph_at(o, &Default::default(), 2, 1), '.');
    assert_eq!(glyph_at(o, &Default::default(), 99, 1), ' ');
    let mut other_level = o.clone();
    other_level.ground_items[0].position = Position { x: 2, y: 1, z: 1 };
    assert_eq!(glyph_at(&other_level, &Default::default(), 2, 1), '.');
    other_level
        .visible_cells
        .retain(|cell| cell.position.x != 2);
    assert_eq!(glyph_at(&other_level, &Default::default(), 2, 1), ' ');
    other_level.visible_cells.push(CellView {
        object_presentation: None,
        lit: true,
        asset: None,
        trap: None,
        door: None,
        material: "stone".into(),
        key: "wall".into(),
        stairs_up: false,
        stairs_down: false,
        position: Position { x: 2, y: 1, z: 0 },
        wall: true,
        place_hint: false,
    });
    assert_eq!(glyph_at(&other_level, &Default::default(), 2, 1), '#');
    other_level.ground_items[0].position = Position { x: 3, y: 1, z: 0 };
    assert_eq!(glyph_at(&other_level, &Default::default(), 3, 1), '(');
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
        matches!(super::request(app.input(Input::Key {key:Key::Enter})), Some(Request::Command {command:Command::Annotate {text, audience:Audience::Private,anchor:Anchor::State {snapshot},..},..}) if text == "east" && snapshot == state().state().snapshot)
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
            presentation: None,
            reachable: true,
            item: ItemView {
                corpse: None,
                class: Default::default(),
                asset: None,
                quantity: 1,
                appearance: String::new(),
                identified: true,
                description: String::new(),
                id: super::item_target(4),
                name: "another token".into(),
                known_currency: None,
                known_supply: None,
            },
            position,
            supply_reachable: false,
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
        super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command {
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
    Arc::make_mut(&mut snapshot.state)
        .snapshot
        .0
        .push_str("-next");
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
        super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command {
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
        super::request(app.input(Input::Key {
            key: Key::OlderHistory
        })),
        Some(Request::History {
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
        super::request(app.input(Input::Key { key: Key::History })),
        Some(Request::History { .. })
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
    let mut snapshot = state().reset_snapshot().unwrap();
    Arc::make_mut(&mut snapshot.state).observation = view.clone();
    assert!(!tor_client_ascii::render::map_tiles(
        &ClientState::from_snapshot(snapshot).unwrap(),
        &Default::default()
    )
    .iter()
    .any(|t| t.position.z == -17));
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
                    snapshot: tor_protocol::SnapshotId("fixture-display-4".into()),
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
    assert_eq!(
        glyph_at(&state.state().observation, &Default::default(), 2, 1),
        '+'
    );
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state);
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::OpenDoor }), Effect::None);
    assert!(matches!(
        super::request(app.input(Input::Key { key: Key::Down })),
        Some(Request::Command {
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
    let initial = state().reset_snapshot().unwrap();
    app.replace_snapshot(initial.clone()).unwrap();
    app.ready();
    for sequence in 1..=64 {
        let mut view = Arc::unwrap_or_clone(initial.state.clone());
        view.revision += sequence;
        view.snapshot = tor_protocol::SnapshotId(format!("fixture-display-{}", view.revision));
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
            appearance_seed: None,
            worn: Vec::new(),
            asset: None,
            id: super::actor_target(2),
            name: "guard".into(),
            description: String::new(),
            position: Position { x: 2, y: 1, z: 0 },
        });
        view.observation.combat = Some(CombatView {
            public_plan: None,
            sequence: None,
            own_stats: Some(tor_client_common::test_support::stats()),
            hp: 30,
            max_hp: 30,
            preparation_remaining: None,
            preparation_active: false,
            recovery_remaining: 0,
            actors: vec![CombatActorView {
                actor: super::actor_target(2),
                hostile,
                allied: false,
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
        let Some(Request::Command {
            command: Command::Act { action, .. },
            ..
        }) = super::request(app.input(Input::Key { key: Key::Right }))
        else {
            panic!("action")
        };
        assert_eq!(
            action,
            if attacks {
                Action::UseAbility {
                    id: "basic_melee".into(),
                    invocation: tor_protocol::AbilityInvocation {
                        aim: tor_protocol::AbilityAim::ActorTarget {
                            target: super::actor_target(2),
                        },
                        weapon: tor_protocol::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
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
    app.ready();
    app.status.clear();
    let mut canvas = tor_client_ascii::render::Canvas::default();
    // Only the status glyph row: changes elsewhere cannot satisfy this oracle.
    let status_pixels = |canvas: &tor_client_ascii::render::Canvas| {
        (6..22)
            .flat_map(|y| canvas.pixels[y * 1200 + 24..y * 1200 + 400].iter().copied())
            .collect::<Vec<_>>()
    };
    canvas.draw(&app);
    let blank = status_pixels(&canvas);
    app.status = "HP 20/20 - Victory!".into();
    canvas.draw(&app);
    let hyphen = status_pixels(&canvas);
    assert_ne!(hyphen, blank, "status must actually be visible");
    for dash in ['—', '–'] {
        app.status = format!("HP 20/20 {dash} Victory!");
        canvas.draw(&app);
        assert_eq!(status_pixels(&canvas), hyphen, "normalize {dash}");
    }
    app.status = "HP 20/20 ? Victory!".into();
    canvas.draw(&app);
    assert_ne!(status_pixels(&canvas), hyphen, "negative glyph control");
}

/// Context for one synthetic attachment used by this fixture/workload.
fn fixture_context() -> tor_protocol::StreamContext {
    tor_protocol::StreamContext {
        stream: tor_protocol::StreamId("fixture-attachment".into()),
        epoch: 0,
    }
}

fn inspection_report() -> CreatureInspectionView {
    let snapshot = creature_snapshot();
    let combat = snapshot.state.observation.combat.as_ref().unwrap();
    let mut stats = combat.own_stats.clone().unwrap();
    stats.skills = ["athletics", "lore"]
        .map(|id| SkillView {
            skill: tor_client_common::test_support::content(id),
            rank: 1,
        })
        .to_vec();
    let attack: AttackView = serde_json::from_value(serde_json::json!({"skill":{"id":"heavy_weaponry","name":"Heavy Weaponry"},"bonus":0,"damage_enchantment":0,"wind_up":60,"recovery":40,"damage":{"primary":{"category":"impact","descriptor":null,"sides":6},"components":[{"category":"impact","descriptor":null,"amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}]}})).unwrap();
    let report = CreatureInspectionView {
        actor: ActorId(2),
        revision: 5,
        tick: 0,
        name: "blue sentinel".into(),
        faction: "blue".into(),
        species: tor_client_common::test_support::content("human"),
        starting_skills: [
            tor_client_common::test_support::content("athletics"),
            tor_client_common::test_support::content("lore"),
        ],
        restoration: InspectionRestoration::ChooseAgain,
        dormant_talents: vec![],
        ability_strikes: stats
            .abilities
            .iter()
            .map(|ability| AbilityStrikesView {
                ability: ability.id.clone(),
                source: StrikeSourceInspectionView::SelectedMelee {},
            })
            .collect(),
        natural_attack: NaturalAttackView {
            source: tor_client_common::test_support::content("human"),
            skill: attack.skill.clone(),
            bonus: attack.bonus,
            wind_up: attack.wind_up,
            recovery: attack.recovery,
            strike_count: 1,
        },
        selected_attack: SelectedAttackView {
            category: "equipment".into(),
            source: tor_client_common::test_support::content("blade"),
            socket: Some(0),
            attack: SelectedMeleeView::Weapon {
                attack: AttackView {
                    damage_enchantment: 0,
                    bonus: 100,
                    ..attack
                },
            },
            attribute: tor_client_common::test_support::content("strength"),
            attribute_value: 2,
            rank: 1,
            check_bonus: 103,
            abilities: vec![],
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
            source_kind: HitDieSourceKind::Class,
            source: tor_client_common::test_support::content("warrior"),
            health_seed: u64::MAX,
            health_die: 8,
            base_health: 7,
            point_pools: vec![],
            purchases: vec![],
            attribute: None,
        }],
        sources: vec![],
        conditions: vec![],
    };
    report
}

#[test]
fn privileged_inspection_is_authorized_scrollable_and_cleared_on_snapshot() {
    let snapshot = creature_snapshot();
    let report = inspection_report();
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    for role in [AccessRole::Player, AccessRole::Spectator] {
        app.role = role;
        assert!(app.show_inspection(&report).is_err());
        assert!(!app.stats_open);
        assert!(!app.has_inspection());
    }
    app.role = AccessRole::Wizard;
    let before = app.state.as_ref().unwrap().session().clone();
    let mut invalid = report.clone();
    invalid.hp += 1;
    app.show_inspection(&report).unwrap();
    let displayed = app.stats_rows().to_vec();
    assert!(app.show_inspection(&invalid).is_err());
    assert_eq!(app.stats_rows().as_ref(), displayed);
    let mut invalid_rank = report.clone();
    invalid_rank.stats.skills[0].rank = 2;
    assert!(app.show_inspection(&invalid_rank).is_err());
    assert_eq!(app.stats_rows().as_ref(), displayed);
    assert!(app.stats_open);
    assert!(app.stats_rows().join("\n").contains("blue sentinel"));
    assert!(app.stats_rows().join("\n").contains("weapon check +100"));
    assert!(app.stats_rows().join("\n").contains("Selected check: +103"));
    for _ in 0..100 {
        app.input(Input::Key { key: Key::Down });
    }
    assert_eq!(
        app.stats_scroll,
        app.stats_rows().len() - tor_client_ascii::STATS_ROWS
    );
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
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
    let before = app.state.as_ref().unwrap().session().clone();
    let expected = app.state.as_ref().unwrap().reply_context();
    assert_eq!(app.input(Input::Key { key: Key::Wizard }), Effect::None);
    assert_eq!(
        app.input(Input::Text {
            text: "creature inspect 2".into()
        }),
        Effect::None
    );
    let Some(Request::Command {
        context,
        branch,
        command,
    }) = super::request(app.input(Input::Key { key: Key::Enter }))
    else {
        panic!("query request");
    };
    assert_eq!(context, expected.input);
    assert_eq!(branch, expected.branch);
    assert_eq!(
        command,
        Command::Wizard {
            expected_revision: expected.revision,
            operation: "creature inspect 2".into()
        }
    );
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
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

fn ordered_strike_snapshot(completed: u16, active: bool, permitted: bool) -> Snapshot {
    let mut snapshot = creature_snapshot();
    let combat = &mut Arc::make_mut(&mut snapshot.state).observation.combat;
    let mut value = serde_json::to_value(combat.as_ref().unwrap()).unwrap();
    value["preparation_remaining"] = serde_json::json!("5");
    value["preparation_active"] = serde_json::json!(active);
    value["sequence"] = serde_json::json!({"completed":completed,"total":3,
        "next_strike_remaining":"5","cancel_recovery_remaining":if completed == 0 { "0" } else { "35" }});
    *combat = Some(serde_json::from_value(value).unwrap());
    snapshot.readiness.admission = false;
    snapshot.readiness.cancel = if permitted {
        vec![IntentionId("sequence".into())]
    } else {
        vec![]
    };
    snapshot.intentions = vec![IntentionStatus {
        actor: snapshot.actor,
        branch: snapshot.branch.clone(),
        intention: IntentionId("sequence".into()),
        entry_id: EntryId("admitted".into()),
        phase: if active {
            IntentionPhase::Started
        } else {
            IntentionPhase::Paused
        },
    }];
    snapshot
}

#[test]
fn ordered_strikes_ascii_status_reports_prefix_pause_and_exact_debt() {
    for active in [true, false] {
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(
            ClientState::from_snapshot(ordered_strike_snapshot(1, active, true)).unwrap(),
        );
        app.ready();
        let rows = tor_client_ascii::status_lines(&app, app.state.as_ref().unwrap());
        assert!(rows[1].contains("1/3"), "{rows:?}");
        assert!(rows[1].contains("Next:5"), "{rows:?}");
        assert!(rows[1].contains("Cancel:35"), "{rows:?}");
        assert_eq!(rows[1].contains("paused"), !active, "{rows:?}");
        assert!(app
            .intention_hint()
            .unwrap()
            .contains("move to cancel and step"));
    }
}

#[test]
fn ordered_strikes_native_direction_sends_one_atomic_escape_only_with_permission() {
    for completed in [0, 1] {
        for permitted in [false, true] {
            let mut app = App::new();
            app.role = AccessRole::Player;
            app.set_state(
                ClientState::from_snapshot(ordered_strike_snapshot(completed, false, permitted))
                    .unwrap(),
            );
            app.ready();
            app.messages.skip();
            let before = app.state.as_ref().unwrap().session().clone();
            let effect = app.input(Input::Key { key: Key::Left });
            if completed > 0 && permitted {
                let Effect::Request(request) = effect else {
                    panic!("one atomic escape request expected")
                };
                let value = serde_json::to_value(request).unwrap();
                assert_eq!(value["command"]["type"], "cancel_intention");
                assert_eq!(value["command"]["intention"], "sequence");
                assert_eq!(value["command"]["step"], "west");
            } else {
                assert_eq!(effect, Effect::None);
            }
            assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
        }
    }
}

#[test]
fn ordered_strikes_spectator_cannot_escape_or_receive_control_hints() {
    let mut app = App::new();
    app.role = AccessRole::Spectator;
    app.set_state(ClientState::from_snapshot(ordered_strike_snapshot(1, false, true)).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Left }), Effect::None);
    assert!(app.intention_hint().is_none());
}

#[test]
fn ordered_strikes_escape_selects_the_preparation_instead_of_unrelated_queued_work() {
    let mut snapshot = ordered_strike_snapshot(1, false, true);
    snapshot.intentions.insert(
        0,
        IntentionStatus {
            actor: snapshot.actor,
            branch: snapshot.branch.clone(),
            intention: IntentionId("queued-work".into()),
            entry_id: EntryId("queued-admission".into()),
            phase: IntentionPhase::Queued,
        },
    );
    snapshot
        .readiness
        .cancel
        .push(IntentionId("queued-work".into()));
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    app.messages.skip();
    let Effect::Request(request) = app.input(Input::Key { key: Key::Left }) else {
        panic!("escape request")
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["command"]["intention"], "sequence");
    assert_eq!(value["command"]["step"], "west");
}

#[test]
fn ordered_strikes_queued_resume_retains_the_same_escape_identity() {
    let mut snapshot = ordered_strike_snapshot(1, false, true);
    snapshot.intentions[0].phase = IntentionPhase::Queued;
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    app.messages.skip();
    let Effect::Request(request) = app.input(Input::Key { key: Key::Left }) else {
        panic!("queued resume escape")
    };
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["command"]["intention"], "sequence");
    assert_eq!(value["command"]["step"], "west");
}

#[test]
fn ordered_strikes_ascii_nonterminal_victory_keeps_sequence_status() {
    let mut snapshot = ordered_strike_snapshot(1, false, true);
    Arc::make_mut(&mut snapshot.state)
        .observation
        .combat
        .as_mut()
        .unwrap()
        .victory = true;
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    let rows = tor_client_ascii::status_lines(&app, app.state.as_ref().unwrap());
    for expected in ["Victorious", "Sequence paused:1/3", "Next:5", "Cancel:35"] {
        assert!(rows[1].contains(expected), "missing {expected}: {rows:?}");
    }
}

#[test]
fn ordered_strikes_ascii_private_source_menu_builds_a_current_typed_page_query() {
    let mut value = serde_json::to_value(inspection_report()).unwrap();
    value["revision"] = serde_json::json!("5");
    value["natural_attack"]
        .as_object_mut()
        .unwrap()
        .remove("strikes");
    value["natural_attack"]["strike_count"] = serde_json::json!(16);
    value["ability_strikes"]=serde_json::json!(value["stats"]["abilities"].as_array().unwrap().iter().map(|ability|
        serde_json::json!({"ability":ability["id"],"source":{"kind":"selected_melee"}})).collect::<Vec<_>>());
    let report: CreatureInspectionView =
        serde_json::from_value(value).expect("lightweight source summary");
    let mut app = App::new();
    app.role = AccessRole::Wizard;
    app.set_state(ClientState::from_snapshot(creature_snapshot()).unwrap());
    app.ready();
    app.show_inspection(&report).unwrap();
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(app.input(Input::Key { key: Key::Tab }), Effect::None);
    assert!(app.stats_rows().join("\n").contains("Strike sources"));
    let Effect::Request(request) = app.input(Input::Key { key: Key::Enter }) else {
        panic!("source selection requests one bounded detail page");
    };
    let wire = serde_json::to_value(request).unwrap();
    assert_eq!(wire["command"]["type"], "inspect_strike_source");
    assert_eq!(wire["command"]["actor"], "2");
    assert_eq!(wire["command"]["target_revision"], "5");
    assert_eq!(
        wire["command"]["source"],
        serde_json::json!({"kind":"natural"})
    );
    assert!(wire["command"]["cursor"].is_null());
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
    let mut reset = app.state.as_ref().unwrap().reset_snapshot().unwrap();
    reset.context.epoch += 1;
    app.replace_snapshot(reset).unwrap();
    assert!(!app.has_inspection());
    assert!(
        !matches!(
            app.input(Input::Key { key: Key::Faster }),
            Effect::Request(_)
        ),
        "reset cannot request a stale next page"
    );
}

#[test]
fn ordered_strikes_ascii_detail_keys_replace_current_page_and_revisit_zero_without_reopening() {
    let report = inspection_report();
    let mut app = App::new();
    app.role = AccessRole::Wizard;
    app.set_state(ClientState::from_snapshot(creature_snapshot()).unwrap());
    app.ready();
    app.show_inspection(&report).unwrap();
    app.input(Input::Key { key: Key::Tab });
    assert!(matches!(
        app.input(Input::Key { key: Key::Enter }),
        Effect::Request(_)
    ));
    let context = app.state.as_ref().unwrap().reply_context();
    let mut entries = vec![StrikePageEntry::StrikeHeader {
        strike: 0,
        offset: 0,
        unlock: None,
        check: None,
        success_count: 0,
        failure_count: 0,
        descriptor_count: 32,
        rider_count: 0,
    }];
    entries.extend((0..32).map(|index| StrikePageEntry::StrikeDescriptor {
        strike: 0,
        index,
        descriptor: tor_client_common::test_support::content(&format!("d_{index}")),
    }));
    let pages: Vec<_> = entries
        .chunks(16)
        .enumerate()
        .map(|(number, chunk)| CreatureStrikePage {
            inspection: StrikeInspectionId("native-detail".into()),
            actor: report.actor,
            target_revision: report.revision,
            source: StrikeSourceSelector::Natural {},
            resolved: StrikeSourceAttribution::Natural {
                species: report.natural_attack.source.clone(),
            },
            strike_count: 1,
            start: chunk[0].position(),
            entries: chunk.to_vec(),
            next: entries
                .get((number + 1) * 16)
                .map(|entry| StrikePageCursor {
                    inspection: StrikeInspectionId("native-detail".into()),
                    position: entry.position(),
                }),
        })
        .collect();
    for (number, page) in pages.iter().enumerate() {
        app.bind_strike_request("page");
        app.show_strike_page(&context, "page", page)
            .expect("requested private detail page installs");
        app.ready();
        let rows = app.stats_rows().join("\n");
        assert!(rows.starts_with("Strike source:"));
        if number > 0 {
            assert!(
                !rows.contains("D 0 (d_0)"),
                "previous page text must not accumulate: {rows}"
            );
        }
        if let Some(cursor) = &page.next {
            assert!(
                matches!(super::request(app.input(Input::Key { key: Key::Faster })), Some(Request::Command {
                command: Command::InspectStrikeSource { cursor: Some(ref actual), .. }, .. }) if actual == cursor)
            );
        }
    }
    assert_eq!(app.input(Input::Key { key: Key::Faster }), Effect::None);
    for number in (0..2).rev() {
        assert!(
            matches!(super::request(app.input(Input::Key { key: Key::Slower })), Some(Request::Command {
            command: Command::InspectStrikeSource { cursor: Some(ref actual), .. }, .. }) if actual.inspection == pages[0].inspection && actual.position == pages[number].start)
        );
        app.bind_strike_request("previous");
        app.show_strike_page(&context, "previous", &pages[number])
            .unwrap();
        app.ready();
    }
    app.input(Input::Key { key: Key::Escape });
    assert!(app.stats_rows().join("\n").contains("Strike sources"));
}

#[test]
fn phase5_supportive_menu_lists_self_and_allies_but_never_neutrals() {
    let mut snapshot = creature_snapshot();
    let view = &mut Arc::make_mut(&mut snapshot.state).observation;
    let own = view.self_target;
    let ally = super::actor_target(42);
    let neutral = super::actor_target(43);
    let mut ability = tor_client_common::test_support::ability("mend");
    ability.reach = AbilityReach::SelfOrAlly {
        range: AbilityAllyRange::Touch,
    };
    ability.target_kind = AbilityTargetKind::SelfOrAlly;
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![ability];
    view.visible_actors.clear();
    for (id, name) in [(ally, "companion"), (neutral, "stranger")] {
        view.visible_actors.push(ActorView {
            appearance_seed: None,
            worn: Vec::new(),
            asset: None,
            id,
            name: name.into(),
            description: String::new(),
            position: Position { x: 1, y: 0, z: 0 },
        });
        view.combat.as_mut().unwrap().actors.push(CombatActorView {
            actor: id,
            hostile: false,
            allied: id == ally,
            injury: Injury::Healthy,
        });
    }
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.role = AccessRole::Player;
    app.ready();
    app.messages.skip();
    app.input(Input::Key {
        key: Key::Abilities,
    });
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert_eq!(
        app.attack_targets
            .iter()
            .map(|actor| actor.id)
            .collect::<Vec<_>>(),
        [own, ally]
    );
    assert!(
        matches!(super::request(app.input(Input::Key { key: Key::Enter })), Some(Request::Command { command: Command::Act { action: Action::UseAbility { invocation: tor_protocol::AbilityInvocation { aim: tor_protocol::AbilityAim::ActorTarget { target }, weapon: tor_protocol::WeaponSelection::Default }, .. }, .. }, .. }) if target == own)
    );
}

#[test]
fn eat_picker_and_hunger_status_use_only_disclosed_nutrition() {
    let mut snapshot = creature_snapshot();
    let view = &mut Arc::make_mut(&mut snapshot.state).observation;
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .nutrition = NutritionView::Required {
        reserve: 6,
        maximum: 12,
        hunger: Hunger::Hungry,
        unit_ticks: 100,
        elapsed: 0,
        starve_at: -3,
    };
    // The carried meal has one authoritative location, not a second copy on the ground.
    let mut item = view.ground_items.remove(0).item;
    item.name = "fixture meal".into();
    item.class = ItemClass::Food;
    view.inventory = vec![item.clone()];
    view.interactions = Some(InteractionView {
        refused: vec![],
        curse_blocked: vec![],
        identification: None,
        slots: vec![],
        preparation: None,
        completed: vec![],
        inventory: vec![ItemInteractionView {
            item: item.id,
            slot: None,
            equipped_slot: None,
            known_cursed: None,
            known_protected: None,
            known_projectile_bonus: None,
            known_equipment: None,
            drinkable: false,
            edible: true,
            readable: false,
            zappable: false,
            portable_light: None,
            device_activation: None,
            charges: None,
        }],
    });
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    assert!(
        tor_client_ascii::status_lines(&app, app.state.as_ref().unwrap())[1].contains("hungry")
    );
    assert_eq!(app.input(Input::Key { key: Key::Eat }), Effect::None);
    assert_eq!(app.pickup.len(), 1);
    app.input(Input::Text { text: "99".into() });
    assert!(app.quantity.is_empty());
    assert!(
        matches!(super::request(app.input(Input::Key { key: Key::Enter })), Some(Request::Command { command: Command::Act { action: Action::Eat { item: target }, .. }, .. }) if target == item.id)
    );
}

#[test]
fn personal_condition_rows_are_visible_readonly_and_distinguish_protection() {
    let mut snapshot = creature_snapshot();
    Arc::make_mut(&mut snapshot.state)
        .observation
        .combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .conditions = vec![
        PersonalCondition {
            condition: ContentRef {
                id: "fixture_faint".into(),
                name: "Fixture Faint".into(),
            },
            phase: ConditionPhase::Active,
            remaining_ticks: Some(200),
        },
        PersonalCondition {
            condition: ContentRef {
                id: "fixture_ice".into(),
                name: "Fixture Ice".into(),
            },
            phase: ConditionPhase::Protection,
            remaining_ticks: Some(400),
        },
        PersonalCondition {
            condition: ContentRef {
                id: "fixture_haste".into(),
                name: "Fixture Haste".into(),
            },
            phase: ConditionPhase::Active,
            remaining_ticks: None,
        },
    ];
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(app.input(Input::Key { key: Key::Stats }), Effect::None);
    let rows = app.stats_rows().join("\n");
    assert!(rows.contains("Fixture Haste: until removed"));
    assert!(rows.contains("Fixture Faint: 200 active ticks remaining"));
    assert!(rows.contains("Protection against Fixture Ice: 400 active ticks remaining"));
    assert!(!rows.contains("from actor"));
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
}

#[test]
fn stats_screen_displays_restorable_losses_without_changing_state() {
    let mut snapshot = creature_snapshot();
    let combat = Arc::make_mut(&mut snapshot.state)
        .observation
        .combat
        .as_mut()
        .unwrap();
    let mut value = serde_json::to_value(combat.own_stats.as_ref().unwrap()).unwrap();
    value["restorable_losses"] = serde_json::json!({
        "attributes":{"strength":1,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "hit_dice":"0","maximum_health":2
    });
    combat.own_stats = Some(serde_json::from_value(value).unwrap());
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(app.input(Input::Key { key: Key::Stats }), Effect::None);
    assert!(app
        .stats_rows()
        .iter()
        .any(|line| line.contains("Restorable losses: Strength 1, maximum Health 2")));
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
}

#[test]
fn creature_catalog_is_private_scrollable_and_has_no_strike_binding() {
    let snapshot = creature_snapshot();
    let page = CreatureCatalogPage {
        depth: Some(27),
        entries: (0..32)
            .map(|i| CreatureCatalogEntry {
                selector: format!("rogue:vampire{i:02}"),
                name: format!("Vampire {i}"),
                details: "Authored creature with racial growth at depth 27".into(),
            })
            .collect(),
        next: Some("rogue:vampire31".into()),
    };
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    for role in [AccessRole::Player, AccessRole::Spectator] {
        app.role = role;
        assert!(app.show_creature_catalog(&page).is_err());
        assert!(!app.stats_open);
    }
    app.role = AccessRole::Wizard;
    app.show_inspection(&inspection_report()).unwrap();
    app.show_creature_catalog(&page).unwrap();
    assert_eq!(app.stats_title(), "CREATURE CATALOG");
    assert!(app.stats_open);
    let rows = app.stats_rows().to_vec();
    assert!(rows.join("\n").contains("Vampire 0"));
    assert!(matches!(
        app.input(Input::Key { key: Key::Tab }),
        Effect::None
    ));
    assert_eq!(app.stats_rows().as_ref(), rows);
    let mut invalid = page.clone();
    invalid.depth = Some(0);
    assert!(app.show_creature_catalog(&invalid).is_err());
    assert_eq!(app.stats_rows().as_ref(), rows);
    for _ in 0..200 {
        app.input(Input::Key { key: Key::Down });
    }
    assert_eq!(app.stats_scroll, rows.len() - tor_client_ascii::STATS_ROWS);
    let mut reset = snapshot;
    reset.context.epoch += 1;
    app.replace_snapshot(reset).unwrap();
    assert!(!app.stats_open);
    assert!(!app.has_inspection());
    assert_eq!(app.stats_title(), "CREATURE STATS");
}

#[test]
fn wizard_creature_catalog_is_discoverable_in_ascii_help() {
    assert!(tor_client_ascii::HELP
        .iter()
        .any(|line| line.contains("F7") && line.contains("creature list")));
}

#[test]
fn one_reachable_stack_is_taken_whole_without_a_quantity_menu() {
    for quantity in [1, 6, u64::MAX] {
        let mut snapshot = state().reset_snapshot().unwrap();
        Arc::make_mut(&mut snapshot.state).observation.ground_items[0]
            .item
            .quantity = quantity;
        let target = snapshot.state.observation.ground_items[0].item.id;
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        app.ready();
        let effect = app.input(Input::Key { key: Key::Pickup });
        assert!(matches!(super::request(effect), Some(Request::Command {
            command: Command::Act { action: Action::Take { item, quantity: None }, .. }, ..
        }) if item == target));
        assert!(app.pickup.is_empty());
        assert!(
            matches!(app.input(Input::Key { key: Key::Pickup }), Effect::None),
            "a pending request must not repeat pickup"
        );
    }
}

#[test]
fn status_reports_disclosed_attributes_resources_and_exact_gold_without_name_inference() {
    let mut snapshot = creature_snapshot();
    let mut coin = snapshot.state.observation.ground_items[0].item.clone();
    coin.id = super::item_target(800);
    coin.class = ItemClass::Coin;
    coin.identified = true;
    coin.known_currency = Some(CurrencyKind::Gold);
    coin.name = "unusual tender".into();
    coin.quantity = u64::MAX;
    let mut second = coin.clone();
    second.id = super::item_target(801);
    let mut fake = coin.clone();
    fake.id = super::item_target(802);
    fake.name = "gold coins".into();
    fake.known_currency = None;
    Arc::make_mut(&mut snapshot.state).observation.inventory = vec![coin, second, fake];
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    let state = app.state.as_ref().unwrap();
    let before = state.session().clone();
    let rows = tor_client_ascii::status_lines(&app, state);
    for term in ["Str:2", "Spd:1", "Int:3", "Wil:2", "Awr:1", "Pre:0", "HD:1"] {
        assert!(rows[0].contains(term), "{term}: {rows:?}");
    }
    for term in [
        "HP:7(12)",
        "Stm:2/3[-1]",
        "Foc:2/3[-1]",
        "Mana:2/3[-1]",
        "Gold:36893488147419103230",
    ] {
        assert!(rows[1].contains(term), "{term}: {rows:?}");
    }
    for width in [0, 1, 24, 72, 146] {
        let fitted = tor_client_ascii::status_lines_at_width(&app, state, width);
        assert!(fitted.iter().all(|row| row.chars().count() <= width));
        if width > 0 && rows[0].chars().count() > width {
            assert!(fitted[0].ends_with('+'));
        }
    }
    let fitted = tor_client_ascii::status_lines_at_width(&app, state, 72).join(" ");
    for term in [
        "HP:7(12)",
        "Stm:2/3[-1]",
        "Foc:2/3[-1]",
        "Mana:2/3[-1]",
        "Gold:36893488147419103230",
    ] {
        assert!(fitted.contains(term), "{term}: {fitted}");
    }
    for width in [8, 12, 24, 48, 72] {
        let fitted = tor_client_ascii::status_lines_at_width(&app, state, width);
        for token in fitted[1..].iter().flat_map(|line| line.split_whitespace()) {
            assert!(
                token == "+" || rows[1].split_whitespace().any(|original| original == token),
                "partial field {token}"
            );
        }
    }
    assert_eq!(state.session().clone(), before);
}

#[test]
fn status_does_not_invent_unavailable_personal_stats_or_hidden_gold() {
    let mut snapshot = creature_snapshot();
    let observation = &mut Arc::make_mut(&mut snapshot.state).observation;
    observation.combat.as_mut().unwrap().own_stats = None;
    let mut item = observation.ground_items[0].item.clone();
    item.id = super::item_target(803);
    item.name = "gold".into();
    item.class = ItemClass::Coin;
    item.identified = false;
    item.known_currency = None;
    observation.inventory = vec![item];
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    let rows = tor_client_ascii::status_lines(&app, app.state.as_ref().unwrap());
    assert!(!rows[0].contains("Str:"));
    assert!(!rows[1].contains("Mana:"));
    assert!(rows[1].contains("Gold:0"));
    assert!(rows[1].contains("HP:7(12)"));
}

#[test]
fn fitted_status_preserves_paused_state_with_large_counters() {
    let mut snapshot = ordered_strike_snapshot(1, false, true);
    let combat = Arc::make_mut(&mut snapshot.state)
        .observation
        .combat
        .as_mut()
        .unwrap();
    combat.preparation_remaining = Some(u64::MAX);
    let sequence = combat.sequence.as_mut().unwrap();
    sequence.next_strike_remaining = u64::MAX;
    sequence.cancel_recovery_remaining = u64::MAX;
    let mut app = App::new();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    let state = app.state.as_ref().unwrap();
    let rows = tor_client_ascii::status_lines_at_width(&app, state, 72);
    let shown = rows.join(" ");
    assert!(shown.contains("Sequence paused"), "{shown}");
    assert!(shown.contains("Gold:0"), "{shown}");
    assert!(shown.contains("HP:"), "{shown}");
    assert!(rows.iter().all(|row| row.chars().count() <= 72));
    for width in 1..72 {
        let rows = tor_client_ascii::status_lines_at_width(&app, state, width);
        assert!(
            !rows
                .iter()
                .any(|row| row.contains("Sequence +") || row.ends_with("Sequence")),
            "{rows:?}"
        );
    }
}

#[test]
fn discovered_trap_draws_and_selected_adjacent_look_examines_without_moving() {
    let target = TrapTarget::from_digest([61; 32]);
    let mut snapshot = state().reset_snapshot().unwrap();
    let o = &mut Arc::make_mut(&mut snapshot.state).observation;
    o.ground_items.clear();
    let position = Position {
        x: o.position.x + 1,
        y: o.position.y,
        z: o.position.z,
    };
    let cell = o
        .visible_cells
        .iter_mut()
        .find(|cell| cell.position == position)
        .unwrap();
    cell.trap = Some(TrapView {
        id: target,
        family: None,
        reachable: true,
    });
    assert_eq!(
        glyph_at(o, &Default::default(), position.x, position.y),
        '^'
    );
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::Look }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
    assert!(
        matches!(super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command { command: Command::Act { action: Action::ExamineTrap { trap }, .. }, .. }) if trap == target)
    );
    assert!(app.look_cursor.is_none());
}

#[test]
fn distant_or_identified_trap_look_never_creates_an_examination_action() {
    let target = TrapTarget::from_digest([62; 32]);
    for reachable in [false, true] {
        let mut snapshot = state().reset_snapshot().unwrap();
        let o = &mut Arc::make_mut(&mut snapshot.state).observation;
        o.ground_items.clear();
        let position = Position {
            x: o.position.x + 1,
            y: o.position.y,
            z: o.position.z,
        };
        o.visible_cells
            .iter_mut()
            .find(|cell| cell.position == position)
            .unwrap()
            .trap = Some(TrapView {
            id: target,
            family: reachable.then_some(TrapFamily::Rust),
            reachable,
        });
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        app.ready();
        assert_eq!(app.input(Input::Key { key: Key::Look }), Effect::None);
        assert_eq!(app.input(Input::Key { key: Key::Right }), Effect::None);
        assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    }
}

#[test]
fn running_stops_before_a_known_trap_even_when_an_item_covers_its_glyph() {
    let mut snapshot = state().reset_snapshot().unwrap();
    let o = &mut Arc::make_mut(&mut snapshot.state).observation;
    let position = Position {
        x: o.position.x + 1,
        ..o.position
    };
    o.ground_items[0].position = position;
    o.visible_cells
        .iter_mut()
        .find(|cell| cell.position == position)
        .unwrap()
        .trap = Some(TrapView {
        id: TrapTarget::from_digest([63; 32]),
        family: None,
        reachable: true,
    });
    assert_eq!(
        glyph_at(o, &Default::default(), position.x, position.y),
        '('
    );
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    assert_eq!(app.input(Input::Key { key: Key::RunRight }), Effect::None);
    assert!(app.running.is_none());
    assert!(matches!(
        super::request(app.input(Input::Key { key: Key::Right })),
        Some(Request::Command {
            command: Command::Act {
                action: Action::Move {
                    direction: Direction::East
                },
                ..
            },
            ..
        })
    ));
}
