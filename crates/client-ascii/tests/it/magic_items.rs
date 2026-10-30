use std::sync::Arc;
use tor_client_ascii::{inventory_entries, App, Effect, Input, Key};
use tor_client_common::{test_support, ClientState};
use tor_protocol::*;

fn snapshot(identify: bool) -> Snapshot {
    let mut view = test_support::observation();
    view.inventory = [
        (1, ItemClass::Scroll),
        (2, ItemClass::Wand),
        (3, ItemClass::Wand),
    ]
    .into_iter()
    .map(|(id, class)| ItemView {
        id: super::item_target(id),
        name: if class == ItemClass::Scroll {
            "rune scroll"
        } else {
            "oak wand"
        }
        .into(),
        class,
        appearance: "unknown".into(),
        identified: false,
        quantity: 1,
        description: String::new(),
        corpse: None,
        asset: None,
        known_currency: None,
        known_supply: None,
    })
    .collect();
    view.interactions = Some(InteractionView {
        refused: vec![],
        curse_blocked: vec![],
        identification: identify.then(|| IdentificationChoiceView {
            choice: IntentionId("paid-choice".into()),
            classes: vec![ItemClass::Wand],
            items: vec![super::item_target(2), super::item_target(3)],
        }),
        slots: vec![],
        preparation: None,
        completed: vec![],
        inventory: view
            .inventory
            .iter()
            .map(|item| ItemInteractionView {
                item: item.id,
                slot: None,
                equipped_slot: None,
                known_cursed: None,
                known_protected: None,
                known_projectile_bonus: None,
                known_equipment: None,
                drinkable: false,
                edible: false,
                readable: item.class == ItemClass::Scroll,
                zappable: item.class == ItemClass::Wand,
                portable_light: None,
                device_activation: (item.class == ItemClass::Wand)
                    .then_some(tor_protocol::DeviceActivation::Direction),
                charges: None,
            })
            .collect(),
    });
    serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},
        "context":{"stream":"magic-attachment","epoch":"0"},"actor":"1","branch":"magic",
        "cursor":{"sequence":"0","tick":"0"},"has_control":true,"intentions":[],
        "history":{"entries":[],"older_before":null},
        "state":{"snapshot":"fixture-snapshot","wizard_game":false,"revision":"1","observation":view}
    }))
    .unwrap()
}

fn app(identify: bool) -> App {
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(ClientState::from_snapshot(snapshot(identify)).unwrap());
    app.ready();
    app.messages.skip();
    app
}

fn key(app: &mut App, key: Key) -> Effect {
    app.input(Input::Key { key })
}

fn action(effect: Effect) -> Action {
    let Some(Request::Command {
        command: Command::Act { action, .. },
        ..
    }) = super::request(effect)
    else {
        panic!("expected an ordinary action request");
    };
    action
}

#[test]
fn magic_keys_parse_and_read_selects_one_exact_item_without_local_effects() {
    for (text, expected) in [("read", Key::Read), ("zap", Key::Zap)] {
        let input: Input =
            serde_json::from_value(serde_json::json!({"type":"key","key":text})).unwrap();
        assert!(matches!(input, Input::Key { key } if key == expected));
    }
    let mut app = self::app(false);
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(key(&mut app, Key::Read), Effect::None);
    assert_eq!(
        app.pickup.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![super::item_target(1)]
    );
    assert_eq!(app.input(Input::Text { text: "99".into() }), Effect::None);
    assert!(app.quantity.is_empty());
    assert_eq!(
        action(app.input(Input::Text { text: "a".into() })),
        Action::Read {
            item: super::item_target(1)
        }
    );
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
    assert!(app.busy);
    assert_eq!(key(&mut app, Key::Read), Effect::None);
}

#[test]
fn zap_selects_device_before_direction_and_never_infers_charge_eligibility() {
    let mut app = self::app(false);
    let mut snapshot = app.state.as_ref().unwrap().reset_snapshot().unwrap();
    Arc::make_mut(&mut snapshot.state).observation.inventory[2].identified = true;
    Arc::make_mut(&mut snapshot.state)
        .observation
        .interactions
        .as_mut()
        .unwrap()
        .inventory[2]
        .charges = Some(0);
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert_eq!(key(&mut app, Key::Zap), Effect::None);
    assert_eq!(
        app.pickup.len(),
        2,
        "both unknown and empty devices remain choices"
    );
    assert_eq!(app.input(Input::Text { text: "c".into() }), Effect::None);
    assert_eq!(app.zap_item(), Some(super::item_target(3)));
    assert!(app.pickup.is_empty());
    assert!(!app.busy, "selecting a device costs no local turn");
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert_eq!(app.input(Input::Click { x: 500, y: 400 }), Effect::None);
    assert_eq!(
        action(key(&mut app, Key::NorthEast)),
        Action::Zap {
            item: super::item_target(3),
            aim: DeviceAim::Direction {
                direction: Direction::NorthEast
            }
        }
    );
    assert!(app.zap_item().is_none());
}

#[test]
fn zap_cancels_freely_and_stale_views_control_loss_or_disconnect_clear_selection() {
    for reason in ["observation", "snapshot", "control", "disconnect"] {
        let mut app = self::app(false);
        key(&mut app, Key::Zap);
        key(&mut app, Key::Enter);
        assert!(app.zap_item().is_some());
        match reason {
            "disconnect" => app.disconnect("Disconnected".into()),
            "snapshot" => {
                let mut fresh = snapshot(false);
                fresh.context.epoch += 1;
                app.replace_snapshot(fresh).unwrap();
            }
            "control" => {
                let mut snapshot = snapshot(false);
                snapshot.has_control = false;
                snapshot.readiness.admission = false;
                app.set_state(ClientState::from_snapshot(snapshot).unwrap());
            }
            _ => {
                let mut snapshot = snapshot(false);
                Arc::make_mut(&mut snapshot.state).revision += 1;
                Arc::make_mut(&mut snapshot.state)
                    .snapshot
                    .0
                    .push_str("-next");
                app.set_state(ClientState::from_snapshot(snapshot).unwrap());
            }
        }
        assert!(app.zap_item().is_none(), "{reason}");
        assert_eq!(key(&mut app, Key::Enter), Effect::None);
    }
    let mut app = self::app(false);
    key(&mut app, Key::Zap);
    key(&mut app, Key::Enter);
    let before = app.state.as_ref().unwrap().session().clone();
    assert_eq!(key(&mut app, Key::Escape), Effect::None);
    assert!(app.zap_item().is_none());
    assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
    assert!(!app.busy);
}

#[test]
fn identification_is_category_bound_and_cancel_submits_the_paid_choice() {
    let mut app = self::app(true);
    assert_eq!(
        app.identification_choice().unwrap().classes,
        vec![ItemClass::Wand]
    );
    let rows = app.identification_rows();
    assert_eq!(rows.len(), 2);
    assert!(rows[0].contains("b -"));
    assert!(rows[0].contains("oak wand"));
    assert!(!rows.join(" ").contains("rune scroll"));
    assert_eq!(app.input(Input::Text { text: "a".into() }), Effect::None);
    assert_eq!(key(&mut app, Key::Read), Effect::None);
    assert!(!app.busy);
    assert_eq!(
        action(app.input(Input::Text { text: "c".into() })),
        Action::Identify {
            choice: IntentionId("paid-choice".into()),
            item: Some(super::item_target(3))
        }
    );
    assert_eq!(
        key(&mut app, Key::Escape),
        Effect::None,
        "busy input cannot submit a second choice"
    );

    let mut app = self::app(true);
    assert_eq!(
        action(key(&mut app, Key::Escape)),
        Action::Identify {
            choice: IntentionId("paid-choice".into()),
            item: None
        }
    );
}

#[test]
fn identification_empty_category_still_allows_paid_abort() {
    let mut app = self::app(true);
    let mut snapshot = snapshot(true);
    Arc::make_mut(&mut snapshot.state)
        .observation
        .interactions
        .as_mut()
        .unwrap()
        .identification
        .as_mut()
        .unwrap()
        .items
        .clear();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert!(app.identification_rows().is_empty());
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert_eq!(
        action(key(&mut app, Key::Escape)),
        Action::Identify {
            choice: IntentionId("paid-choice".into()),
            item: None
        }
    );
}

#[test]
fn identification_refreshed_selection_uses_current_choice_and_exact_target() {
    let mut app = self::app(true);
    key(&mut app, Key::Down);
    let mut snapshot = snapshot(true);
    Arc::make_mut(&mut snapshot.state).revision += 1;
    Arc::make_mut(&mut snapshot.state)
        .snapshot
        .0
        .push_str("-next");
    let choice = Arc::make_mut(&mut snapshot.state)
        .observation
        .interactions
        .as_mut()
        .unwrap()
        .identification
        .as_mut()
        .unwrap();
    choice.choice = IntentionId("replacement-choice".into());
    choice.items = vec![super::item_target(2)];
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert_eq!(
        action(key(&mut app, Key::Enter)),
        Action::Identify {
            choice: IntentionId("replacement-choice".into()),
            item: Some(super::item_target(2))
        }
    );
}

#[test]
fn magic_controls_and_identification_are_owner_only_and_reset_drops_choice() {
    for role in [AccessRole::Spectator, AccessRole::Player] {
        let mut app = self::app(true);
        app.role = role;
        let mut snapshot = snapshot(true);
        snapshot.has_control = false;
        snapshot.readiness.admission = false;
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        assert!(app.identification_choice().is_none());
        assert!(app.identification_rows().is_empty());
        for input in [Key::Read, Key::Zap] {
            assert_eq!(key(&mut app, input), Effect::None);
            assert!(app.pickup.is_empty());
            assert!(!app.busy);
        }
    }
    let mut app = self::app(true);
    let mut fresh = snapshot(false);
    fresh.context.epoch += 1;
    app.replace_snapshot(fresh).unwrap();
    assert!(app.identification_choice().is_none());
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    app.disconnect("Disconnected".into());
    assert!(app.identification_choice().is_none());
}

#[test]
fn inventory_displays_only_disclosed_exact_instance_charges_and_modal_renders() {
    let mut app = self::app(false);
    let mut known = snapshot(false);
    Arc::make_mut(&mut known.state).observation.inventory[2].identified = true;
    Arc::make_mut(&mut known.state)
        .observation
        .interactions
        .as_mut()
        .unwrap()
        .inventory[2]
        .charges = Some(0);
    app.set_state(ClientState::from_snapshot(known).unwrap());
    let entries = inventory_entries(&app, &app.state.as_ref().unwrap().state().observation);
    assert!(!entries[1].long().contains("charge"));
    assert!(entries[2].long().contains("charges: 0"));
    assert!(!entries[1].short().contains("charge"));
    let mut canvas = tor_client_ascii::render::Canvas::default();
    canvas.draw(&app);
    let normal = canvas.pixels.clone();
    app.set_state(ClientState::from_snapshot(snapshot(true)).unwrap());
    canvas.draw(&app);
    assert_ne!(
        canvas.pixels, normal,
        "paid identification is visibly modal"
    );
    assert!(tor_client_ascii::HELP
        .iter()
        .any(|row| row.contains("F6") && row.contains("F12")));
}

#[test]
fn magic_pickers_and_aiming_clear_on_full_delta_and_same_revision_reset() {
    for operation in [Key::Read, Key::Zap] {
        for refresh in ["full", "delta", "reset"] {
            let mut app = self::app(false);
            key(&mut app, operation);
            assert!(!app.pickup.is_empty());
            let before = app.state.as_ref().unwrap().reset_snapshot().unwrap();
            let mut changed = before.clone();
            if refresh == "reset" {
                changed.context.epoch += 1;
                app.replace_snapshot(changed).unwrap();
            } else {
                let view = Arc::make_mut(&mut changed.state);
                view.revision += 1;
                view.snapshot =
                    tor_protocol::SnapshotId(format!("fixture-display-{}", view.revision));
                view.observation.inventory.reverse();
                let body = if refresh == "full" {
                    UpdateBody::Observation {
                        state: changed.state,
                        event: None,
                    }
                } else {
                    UpdateBody::ObservationDelta {
                        base: ObservationBase {
                            snapshot: before.state.snapshot.clone(),
                            cursor: before.cursor,
                            revision: before.state.revision,
                        },
                        state: Box::new(
                            StateDelta::between(&before.state, &changed.state).unwrap(),
                        ),
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
            }
            assert!(app.pickup.is_empty(), "{operation:?} {refresh}");
            assert!(app.item_operation.is_none());
            assert_eq!(key(&mut app, Key::Enter), Effect::None);
        }
    }
    let mut app = self::app(false);
    key(&mut app, Key::Zap);
    key(&mut app, Key::Enter);
    let before = app.state.as_ref().unwrap().reply_context();
    app.update(StreamUpdate {
        context: before.input.stream,
        actor: before.actor,
        branch: before.branch,
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
    assert!(app.zap_item().is_none());
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
}

#[test]
fn identified_self_device_selection_emits_without_direction_modal() {
    let mut snap = snapshot(false);
    let view = &mut Arc::make_mut(&mut snap.state).observation;
    view.inventory[1].identified = true;
    view.interactions.as_mut().unwrap().inventory[1].device_activation =
        Some(DeviceActivation::SelfTarget);
    let state = ClientState::from_snapshot(snap).expect("valid disclosed self device premise");
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state);
    app.ready();
    app.messages.skip();
    assert_eq!(key(&mut app, Key::Zap), Effect::None);
    assert_eq!(app.pickup.len(), 2);
    assert_eq!(
        action(app.input(Input::Text { text: "b".into() })),
        Action::Zap {
            item: super::item_target(2),
            aim: DeviceAim::SelfTarget {}
        }
    );
    assert!(app.zap_item().is_none());
}

#[test]
fn portable_light_keys_choose_explicit_actions_cancel_and_show_fuel() {
    for (lit, input, expected) in [
        (false, Key::Ignite, false),
        (true, Key::Extinguish, true),
        (false, Key::Apply, false),
        (true, Key::Apply, true),
    ] {
        let mut snap = snapshot(false);
        Arc::make_mut(&mut snap.state).observation = test_support::portable_light_observation(lit);
        let item = snap.state.observation.inventory[0].id;
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snap).unwrap());
        app.ready();
        app.messages.skip();
        let before = app.state.as_ref().unwrap().session().clone();
        let entries = inventory_entries(&app, &app.state.as_ref().unwrap().state().observation);
        assert!(entries[0].long().contains("fuel: 100000 ticks"));
        assert_eq!(key(&mut app, input), Effect::None);
        assert_eq!(app.pickup.len(), 1);
        assert_eq!(key(&mut app, Key::Escape), Effect::None);
        assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
        assert_eq!(key(&mut app, input), Effect::None);
        let emitted = action(key(&mut app, Key::Enter));
        assert_eq!(
            emitted,
            if expected {
                Action::Extinguish { item }
            } else {
                Action::Ignite { item }
            }
        );
    }
    for (text, expected) in [("ignite", Key::Ignite), ("extinguish", Key::Extinguish)] {
        let input: Input =
            serde_json::from_value(serde_json::json!({"type":"key","key":text})).unwrap();
        assert!(matches!(input, Input::Key { key } if key == expected));
    }
}

#[test]
fn dual_device_direction_prompt_offers_explicit_self_without_defaulting_to_it() {
    let mut snap = snapshot(false);
    let view = &mut Arc::make_mut(&mut snap.state).observation;
    view.inventory[1].identified = true;
    view.interactions.as_mut().unwrap().inventory[1].device_activation =
        Some(DeviceActivation::DirectionOrSelf);
    let state = ClientState::from_snapshot(snap).expect("valid dual aim premise");
    let mut app = App::new();
    app.role = AccessRole::Player;
    app.set_state(state);
    app.ready();
    app.messages.skip();
    assert_eq!(key(&mut app, Key::Zap), Effect::None);
    assert_eq!(app.input(Input::Text { text: "b".into() }), Effect::None);
    assert!(app.status.contains("targets yourself"));
    assert_eq!(app.zap_item(), Some(super::item_target(2)));
    assert_eq!(key(&mut app, Key::Enter), Effect::None);
    assert_eq!(
        action(key(&mut app, Key::Wait)),
        Action::Zap {
            item: super::item_target(2),
            aim: DeviceAim::SelfTarget {}
        }
    );
    assert!(app.zap_item().is_none());
}

#[test]
fn apply_requires_disclosed_light_and_clears_on_refresh_control_loss_and_disconnect() {
    let mut no_light = self::app(false);
    let before = no_light.state.as_ref().unwrap().session().clone();
    assert_eq!(key(&mut no_light, Key::Apply), Effect::None);
    assert!(no_light.pickup.is_empty());
    assert!(!no_light.applying);
    assert!(no_light.status.contains("portable light"));
    assert_eq!(no_light.state.as_ref().unwrap().session().clone(), before);
    for change in ["refresh", "control", "disconnect"] {
        let mut snap = snapshot(false);
        Arc::make_mut(&mut snap.state).observation =
            test_support::portable_light_observation(false);
        let mut app = App::new();
        app.role = AccessRole::Player;
        app.set_state(ClientState::from_snapshot(snap.clone()).unwrap());
        app.ready();
        app.messages.skip();
        assert_eq!(key(&mut app, Key::Apply), Effect::None);
        assert!(app.applying);
        // Counts do not turn Apply into a multi-item or pickup operation.
        assert_eq!(app.input(Input::Text { text: "99".into() }), Effect::None);
        assert!(app.quantity.is_empty());
        match change {
            "disconnect" => app.disconnect("test disconnect".into()),
            "control" => {
                snap.has_control = false;
                snap.readiness.admission = false;
                app.set_state(ClientState::from_snapshot(snap).unwrap());
            }
            _ => {
                // Even a same-revision transport reset invalidates the selected light.
                snap.context.epoch += 1;
                app.replace_snapshot(snap).unwrap();
            }
        }
        assert!(!app.applying, "{change}");
        assert!(app.pickup.is_empty(), "{change}");
        assert_eq!(key(&mut app, Key::Enter), Effect::None, "{change}");
    }
}

#[test]
fn equipment_keys_filter_public_classes_and_reuse_exact_equip_remove_actions() {
    for equipped in [false, true] {
        for (input, indices) in if equipped {
            vec![
                (Key::TakeOff, vec![1]),
                (Key::RemoveAccessory, vec![2, 3]),
                (Key::Unequip, vec![0, 1, 2, 3, 4]),
            ]
        } else {
            vec![
                (Key::Wield, vec![0]),
                (Key::Wear, vec![1]),
                (Key::PutOn, vec![2, 3]),
                (Key::Equip, vec![0, 1, 2, 3, 4]),
            ]
        } {
            let mut snap = snapshot(false);
            let view = &mut Arc::make_mut(&mut snap.state).observation;
            let prototype = view.inventory[0].clone();
            let offer = view.interactions.as_ref().unwrap().inventory[0].clone();
            view.inventory = [
                ItemClass::Weapon,
                ItemClass::Armor,
                ItemClass::Ring,
                ItemClass::Amulet,
                ItemClass::Tool,
            ]
            .into_iter()
            .enumerate()
            .map(|(index, class)| ItemView {
                id: super::item_target(index as u64 + 10),
                class,
                name: format!("fixture item {index}"),
                ..prototype.clone()
            })
            .collect();
            let slots: Vec<_> = (0..5)
                .map(|index| {
                    test_support::equipment_slot(
                        &format!("fixture_socket_{index}"),
                        "equipped",
                        false,
                    )
                })
                .collect();
            let interactions = view.interactions.as_mut().unwrap();
            interactions.slots = slots.clone();
            interactions.inventory = view
                .inventory
                .iter()
                .enumerate()
                .map(|(index, item)| ItemInteractionView {
                    item: item.id,
                    slot: Some(slots[index].clone()),
                    equipped_slot: equipped.then_some(index as u16),
                    readable: false,
                    ..offer.clone()
                })
                .collect();
            let mut app = App::new();
            app.role = AccessRole::Player;
            app.set_state(ClientState::from_snapshot(snap).expect("valid equipment premise"));
            app.ready();
            app.messages.skip();
            let before = app.state.as_ref().unwrap().session().clone();
            assert_eq!(key(&mut app, input), Effect::None);
            assert_eq!(
                app.pickup.iter().map(|item| item.id).collect::<Vec<_>>(),
                indices
                    .iter()
                    .map(|index| super::item_target(*index as u64 + 10))
                    .collect::<Vec<_>>()
            );
            assert_eq!(app.state.as_ref().unwrap().session().clone(), before);
            let item = super::item_target(indices[0] as u64 + 10);
            assert_eq!(
                action(key(&mut app, Key::Enter)),
                if equipped {
                    Action::Unequip { item }
                } else {
                    Action::Equip {
                        item,
                        slot: indices[0] as u16,
                    }
                }
            );
        }
    }
}
