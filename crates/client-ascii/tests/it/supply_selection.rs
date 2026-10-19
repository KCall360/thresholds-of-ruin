use tor_client_ascii::{App, Effect, Input, Key};
use tor_client_common::{test_support, ClientState};
use tor_protocol::*;

fn app() -> App {
    let mut view = test_support::observation();
    let mut ability = test_support::ability("mend");
    ability.reach = AbilityReach::SelfOnly;
    ability.target_kind = AbilityTargetKind::SelfOnly;
    ability.cooldown_remaining = 25;
    ability.supply_costs = vec![AbilitySupplyCost {
        supply: "bandage".into(),
        name: "Bandage".into(),
        quantity: 1,
        access: SupplyAccess::CarriedOrGroundTouch,
    }];
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![ability];
    view.ground_items.push(GroundItemView {
        reachable: false,
        supply_reachable: true,
        position: Position { x: 1, y: 0, z: 0 },
        item: ItemView {
            corpse: None,
            id: super::item_target(4),
            class: ItemClass::Misc,
            quantity: 1,
            appearance: "bandage".into(),
            identified: true,
            description: String::new(),
            name: "clean bandage".into(),
            asset: None,
            known_supply: Some(SupplyView {
                id: "bandage".into(),
                name: "Bandage".into(),
            }),
        },
    });
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},
        "context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test",
        "cursor":{"sequence":"0","tick":"0"},"has_control":true,"intentions":[],
        "history":{"entries":[],"older_before":null},"state":{"wizard_game":false,"revision":"1","observation":view}
    })).unwrap();
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.role = AccessRole::Player;
    app.ready();
    app.messages.skip();
    app
}
fn open(app: &mut App) {
    assert_eq!(
        app.input(Input::Key {
            key: Key::Abilities
        }),
        Effect::None
    );
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.supply_selection.is_some());
}
#[test]
fn phase5_supply_menu_defaults_to_carried_and_never_pays_locally() {
    let mut app = app();
    let before = app.state.as_ref().unwrap().snapshot();
    open(&mut app);
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    assert!(app.supply_rows().join(" ").contains("25 active ticks"));
    assert!(app.supply_rows().join(" ").contains("1 Bandage"));
    assert!(matches!(
        super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command {
            command: Command::Act {
                action: Action::UseAbility {
                    supplies: SupplySelection::Carried,
                    ..
                },
                ..
            },
            ..
        })
    ));
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
}
#[test]
fn phase5_supply_menu_selects_exact_adjacent_ground_and_cancels_cleanly() {
    let mut app = app();
    open(&mut app);
    app.input(Input::Key { key: Key::Down });
    app.input(Input::Key { key: Key::Wait });
    assert_eq!(app.supply_selection.as_ref().unwrap().checked.len(), 1);
    assert!(
        matches!(super::request(app.input(Input::Key { key: Key::Enter })), Some(Request::Command { command: Command::Act { action: Action::UseAbility { supplies: SupplySelection::Explicit { items }, .. }, .. }, .. }) if items == vec![SelectedSupply { cost_ordinal: 0, item: super::item_target(4) }])
    );
    let mut app = self::app();
    open(&mut app);
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(app.supply_selection.is_none());
    open(&mut app);
    app.disconnect("Disconnected".into());
    assert!(app.supply_selection.is_none());
}

#[test]
fn phase5_supply_picker_bounds_exact_checked_rows_and_clears_on_revision_change() {
    let mut app = app();
    let mut snapshot = app.state.as_ref().unwrap().snapshot();
    let view = &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
    let source = view.ground_items[0].clone();
    view.ground_items = (1..=33)
        .map(|index| {
            let mut row = source.clone();
            row.item.id = super::item_target(index);
            row
        })
        .collect();
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities[0]
        .supply_costs[0]
        .quantity = 33;
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    open(&mut app);
    for index in 1..=33 {
        app.selected = index;
        assert_eq!(app.input(Input::Key { key: Key::Wait }), Effect::None);
    }
    assert_eq!(
        app.supply_selection.as_ref().unwrap().checked.len(),
        MAX_SUPPLY_FRAGMENTS
    );
    assert!(app.status.contains("32"));
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.supply_selection.is_some());
    std::sync::Arc::make_mut(&mut snapshot.state).revision += 1;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert!(app.supply_selection.is_none());
}

#[test]
fn ray_menu_selects_all_ten_directions_and_preserves_supply_selection_without_targets() {
    for (key, direction) in [
        (Key::Up, Direction::North),
        (Key::Right, Direction::East),
        (Key::Down, Direction::South),
        (Key::Left, Direction::West),
        (Key::NorthEast, Direction::NorthEast),
        (Key::SouthEast, Direction::SouthEast),
        (Key::SouthWest, Direction::SouthWest),
        (Key::NorthWest, Direction::NorthWest),
        (Key::Ascend, Direction::Up),
        (Key::Descend, Direction::Down),
    ] {
        let mut app = app();
        let mut snapshot = app.state.as_ref().unwrap().snapshot();
        let mut state = snapshot.state.as_ref().clone();
        let ability = &mut state
            .observation
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities[0];
        ability.target_kind = AbilityTargetKind::DirectionalRay;
        ability.reach = AbilityReach::Sight { range: 6 };
        state.observation.visible_actors.clear();
        snapshot.state = state.into();
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        app.ready();
        app.messages.skip();
        let before = app.state.as_ref().unwrap().snapshot();
        assert_eq!(
            app.input(Input::Key {
                key: Key::Abilities
            }),
            Effect::None
        );
        assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
        assert!(app.status.contains("Choose a direction"));
        assert!(app.attack_targets.is_empty());
        assert!(app.supply_selection.is_none());
        assert_eq!(app.input(Input::Key { key }), Effect::None);
        assert!(app.supply_selection.is_some());
        assert!(
            matches!(super::request(app.input(Input::Key { key: Key::Enter })), Some(Request::Command {
            command: Command::Act { action: Action::UseAbility {
                invocation: tor_protocol::AbilityInvocation { aim: tor_protocol::AbilityAim::DirectionalRay { direction: selected }, weapon: tor_protocol::WeaponSelection::Default }, supplies: SupplySelection::Carried, ..
            }, .. }, ..
        }) if selected == direction)
        );
        assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    }
}

fn with_weapons(mut app: App, permitted: bool) -> App {
    let mut snapshot = app.state.as_ref().unwrap().snapshot();
    let view = &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities[0]
        .weapon_selection = permitted;
    let slot = test_support::equipment_slot("fixture_wing", "fastened to a wing", true);
    let mut rows = Vec::new();
    for (id, socket, name) in [(80, 0, "primary sword"), (81, 10, "secondary sword")] {
        let mut item = view.ground_items[0].item.clone();
        item.id = super::item_target(id);
        item.class = ItemClass::Weapon;
        item.name = name.into();
        item.identified = false;
        item.known_supply = None;
        view.inventory.push(item.clone());
        rows.push(ItemInteractionView {
            item: item.id,
            slot: Some(slot.clone()),
            equipped_slot: Some(socket),
            known_cursed: None,
            known_protected: None,
            known_equipment: None,
            drinkable: false,
            edible: false,
            readable: false,
            zappable: false,
            charges: None,
        });
    }
    view.interactions = Some(InteractionView {
        curse_blocked: vec![],
        identification: None,
        completed: vec![],
        slots: vec![slot; 11],
        preparation: None,
        inventory: rows,
    });
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.ready();
    app.messages.skip();
    app
}
fn select_secondary(app: &mut App) {
    assert_eq!(app.input(Input::Key { key: Key::Tab }), Effect::None);
    assert!(app.weapon_selection.is_some());
    assert!(app.weapon_rows().join(" ").contains("Default primary"));
    assert!(app.weapon_rows().join(" ").contains("socket 10"));
    assert_eq!(app.input(Input::Key { key: Key::Down }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Down }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.weapon_selection.is_none());
    assert_eq!(
        app.selected_weapon,
        WeaponSelection::Equipped {
            item: super::item_target(81),
            socket: 10
        }
    );
}
#[test]
fn weapon_picker_preserves_one_exact_action_through_supplies_without_local_payment() {
    let mut app = with_weapons(app(), true);
    let before = app.state.as_ref().unwrap().snapshot();
    assert_eq!(
        app.input(Input::Key {
            key: Key::Abilities
        }),
        Effect::None
    );
    select_secondary(&mut app);
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.supply_selection.is_some());
    assert_eq!(app.input(Input::Key { key: Key::Down }), Effect::None);
    assert_eq!(app.input(Input::Key { key: Key::Wait }), Effect::None);
    let query = app.input(Input::Key { key: Key::Enter });
    let committed = confirm_preview(&mut app, query);
    assert!(matches!(super::request(committed), Some(Request::Command {
        command: Command::Act { action: Action::UseAbility {
            invocation: AbilityInvocation { weapon: WeaponSelection::Equipped { item, socket: 10 }, .. },
            supplies: SupplySelection::Explicit { items }, ..
        }, .. }, ..
    }) if item == super::item_target(81) && items == vec![SelectedSupply { cost_ordinal: 0, item: super::item_target(4) }]));
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    assert_eq!(app.selected_weapon, WeaponSelection::Default);
    assert!(app.supply_selection.is_none());
    assert!(app.weapon_selection.is_none());
}
#[test]
fn weapon_picker_requires_disclosed_permission_and_cancel_retains_default() {
    let mut app = with_weapons(app(), false);
    app.input(Input::Key {
        key: Key::Abilities,
    });
    assert_eq!(app.input(Input::Key { key: Key::Tab }), Effect::None);
    assert!(app.weapon_selection.is_none());
    assert!(app.status.contains("does not select a weapon"));
    app.input(Input::Key { key: Key::Escape });
    let mut app = with_weapons(self::app(), true);
    app.input(Input::Key {
        key: Key::Abilities,
    });
    app.input(Input::Key { key: Key::Tab });
    app.input(Input::Key { key: Key::Down });
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert_eq!(app.selected_weapon, WeaponSelection::Default);
    assert!(app.weapon_selection.is_none());
    app.input(Input::Key { key: Key::Enter });
    assert!(matches!(
        super::request(app.input(Input::Key { key: Key::Enter })),
        Some(Request::Command {
            command: Command::Act {
                action: Action::UseAbility {
                    invocation: AbilityInvocation {
                        weapon: WeaponSelection::Default,
                        ..
                    },
                    ..
                },
                ..
            },
            ..
        })
    ));
}
#[test]
fn weapon_selection_rejects_disappeared_or_moved_items_and_clears_on_new_observation() {
    let mut app = with_weapons(app(), true);
    app.input(Input::Key {
        key: Key::Abilities,
    });
    select_secondary(&mut app);
    app.input(Input::Key { key: Key::Enter });
    assert!(app.supply_selection.is_some());
    let mut snapshot = app.state.as_ref().unwrap().snapshot();
    std::sync::Arc::make_mut(&mut snapshot.state)
        .observation
        .interactions
        .as_mut()
        .unwrap()
        .inventory[1]
        .equipped_slot = Some(1);
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.status.contains("no longer equipped"));
    assert!(
        app.supply_selection.is_some(),
        "failed confirmation stays local"
    );
    let mut app = with_weapons(self::app(), true);
    app.input(Input::Key {
        key: Key::Abilities,
    });
    app.input(Input::Key { key: Key::Tab });
    app.input(Input::Key { key: Key::Down });
    app.input(Input::Key { key: Key::Down });
    let mut snapshot = app.state.as_ref().unwrap().snapshot();
    let observation = &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
    let removed = observation.inventory.pop().unwrap().id;
    observation
        .interactions
        .as_mut()
        .unwrap()
        .inventory
        .retain(|row| row.item != removed);
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.weapon_selection.is_some());
    assert_eq!(app.selected_weapon, WeaponSelection::Default);
    std::sync::Arc::make_mut(&mut snapshot.state).revision += 1;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert!(app.weapon_selection.is_none());
    assert!(app.ability_choices.is_empty());
    assert!(app.status.contains("choose your weapon again"));
}

#[test]
fn ordinary_attack_picker_emits_only_selected_or_default_weapon_action() {
    for explicit in [false, true] {
        let mut app = with_weapons(app(), true);
        let mut snapshot = app.state.as_ref().unwrap().snapshot();
        std::sync::Arc::make_mut(&mut snapshot.state)
            .observation
            .combat
            .as_mut()
            .unwrap()
            .own_stats
            .as_mut()
            .unwrap()
            .abilities = vec![test_support::ability("basic_melee")];
        app.set_state(ClientState::from_snapshot(snapshot).unwrap());
        let before = app.state.as_ref().unwrap().snapshot();
        assert_eq!(app.input(Input::Key { key: Key::Attack }), Effect::None);
        if explicit {
            select_secondary(&mut app);
            app.input(Input::Key { key: Key::Tab });
            app.input(Input::Key { key: Key::Escape });
            assert_eq!(
                app.selected_weapon,
                WeaponSelection::Equipped {
                    item: super::item_target(81),
                    socket: 10
                }
            );
        }
        let expected = if explicit {
            WeaponSelection::Equipped {
                item: super::item_target(81),
                socket: 10,
            }
        } else {
            WeaponSelection::Default
        };
        let effect = app.input(Input::Key { key: Key::Enter });
        let effect = if explicit {
            confirm_preview(&mut app, effect)
        } else {
            effect
        };
        assert!(matches!(super::request(effect), Some(Request::Command {
            command: Command::Act { action: Action::UseAbility { id, invocation: AbilityInvocation { weapon, .. }, supplies: SupplySelection::Carried }, .. }, ..
        }) if id == "basic_melee" && weapon == expected));
        assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
        assert!(app.attack_targets.is_empty());
        assert_eq!(app.selected_weapon, WeaponSelection::Default);
    }
}

fn preview_reply(effect: Effect) -> AbilityWeaponPreview {
    let Some(Request::Command {
        command:
            Command::PreviewAbilityWeapon {
                ability,
                invocation,
                supplies,
                ..
            },
        ..
    }) = super::request(effect)
    else {
        panic!("query before action")
    };
    AbilityWeaponPreview {
        ability,
        invocation,
        supplies,
        phase: AbilityWeaponPreviewPhase::Fresh,
        cost: Some(DiagnosticCostBundle {
            stamina: None,
            focus: Some(DiagnosticCost {
                resource: test_support::content("focus"),
                start: 0,
                resolution: 2,
            }),
            mana: Some(DiagnosticCost {
                resource: test_support::content("mana"),
                start: 1,
                resolution: 2,
            }),
        }),
        preparation: 300,
        recovery: 100,
    }
}
fn confirm_preview(app: &mut App, effect: Effect) -> Effect {
    let preview = preview_reply(effect);
    assert!(app.weapon_confirmation.is_some());
    app.bind_weapon_preview("preview").unwrap();
    assert_eq!(
        app.input(Input::Key { key: Key::Enter }),
        Effect::None,
        "waiting cannot commit"
    );
    let context = app.state.as_ref().unwrap().reply_context();
    app.show_weapon_preview(&context, "preview", &preview)
        .unwrap();
    app.ready();
    let rows = app.weapon_preview_rows().join(" ");
    assert!(rows.contains("Time to next strike: 300 ticks"));
    assert!(rows.contains("Mana"));
    assert!(rows.contains("Focus"));
    app.input(Input::Key { key: Key::Enter })
}
#[test]
fn selected_preview_cancellation_mismatch_and_staleness_never_commit() {
    for failure in 0..4 {
        let mut app = with_weapons(app(), true);
        app.input(Input::Key {
            key: Key::Abilities,
        });
        select_secondary(&mut app);
        app.input(Input::Key { key: Key::Enter });
        let effect = app.input(Input::Key { key: Key::Enter });
        let mut response = preview_reply(effect);
        app.bind_weapon_preview("preview").unwrap();
        let context = app.state.as_ref().unwrap().reply_context();
        if failure == 0 {
            response.invocation.weapon = WeaponSelection::Default;
            assert!(app
                .show_weapon_preview(&context, "preview", &response)
                .is_err());
            app.ready();
            assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
        } else {
            app.show_weapon_preview(&context, "preview", &response)
                .unwrap();
            app.ready();
            if failure == 1 {
                assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
            } else {
                let mut snapshot = app.state.as_ref().unwrap().snapshot();
                if failure == 2 {
                    std::sync::Arc::make_mut(&mut snapshot.state).revision += 1;
                } else {
                    snapshot.has_control = false;
                    snapshot.readiness.admission = false;
                }
                app.set_state(ClientState::from_snapshot(snapshot).unwrap());
            }
            assert!(app.weapon_confirmation.is_none());
            assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
        }
    }
}
