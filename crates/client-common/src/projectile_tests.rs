use super::*;
use crate::weapon_preview::WeaponPreview;

fn item(index: u8) -> ItemTarget {
    ItemTarget::from_digest([index; 32])
}
fn state() -> ClientState {
    let mut view = crate::test_support::observation();
    let mut ability = crate::test_support::ability("fixture_projectile");
    ability.name = "Fixture Projectile".into();
    ability.target_kind = AbilityTargetKind::DirectionalProjectile;
    ability.reach = AbilityReach::Projectile;
    ability.weapon_selection = false;
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![ability];
    view.inventory = (1..=200)
        .map(|n| ItemView {
            id: item(n),
            name: format!("known item {n}"),
            class: ItemClass::Weapon,
            quantity: 1,
            appearance: "weapon".into(),
            identified: true,
            known_currency: None,
            known_supply: None,
            description: String::new(),
            asset: None,
            corpse: None,
        })
        .collect();
    let slot = crate::test_support::equipment_slot("weapon", "in hand", true);
    view.interactions = Some(InteractionView {
        identification: None,
        completed: vec![],
        refused: vec![],
        curse_blocked: vec![],
        slots: vec![slot.clone(); 2],
        preparation: None,
        inventory: view
            .inventory
            .iter()
            .map(|item| ItemInteractionView {
                item: item.id,
                slot: Some(slot.clone()),
                equipped_slot: (item.id == self::item(1)).then_some(1),
                known_projectile_bonus: None,
                known_equipment: None,
                known_cursed: None,
                known_protected: None,
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
    ClientState::from_snapshot(serde_json::from_value(serde_json::json!({"readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},"context":{"stream":"fixture","epoch":"0"},"actor":"1","branch":"main","cursor":{"sequence":"0","tick":"0"},"has_control":true,"intentions":[],"history":{"entries":[],"older_before":null},"state":{"wizard_game":false,"revision":"1","observation":view}})).unwrap()).unwrap()
}
fn menu(state: &ClientState) -> ProjectileMenu {
    ProjectileMenu::new(state, "fixture_projectile").unwrap()
}
fn source(n: u8) -> ProjectileChoice {
    ProjectileChoice::Throw {
        source: ThrownSourceSelection::Carried {
            item: item(n),
            socket: 0,
        },
    }
}
fn page(
    menu: &ProjectileMenu,
    choices: Vec<ProjectileChoice>,
    next_offset: Option<u32>,
) -> ProjectileChoicePage {
    ProjectileChoicePage {
        query: menu.query().unwrap().clone(),
        choices,
        next_offset,
    }
}
fn accept(
    menu: &mut ProjectileMenu,
    state: &ClientState,
    id: &str,
    choices: Vec<ProjectileChoice>,
    next: Option<u32>,
) {
    let page = page(menu, choices, next);
    menu.bind_request(id).unwrap();
    menu.accept(state, &state.snapshot().reply_context(), id, &page)
        .unwrap();
}
fn changed(state: &ClientState, change: impl FnOnce(&mut Snapshot)) -> ClientState {
    let mut snapshot = state.snapshot();
    change(&mut snapshot);
    ClientState::from_snapshot(snapshot).unwrap()
}

#[test]
fn replies_bind_request_full_context_and_complete_query_without_mutating_state() {
    let state = state();
    let before = state.snapshot();
    let mut menu = menu(&state);
    let page = page(&menu, vec![source(2)], None);
    let context = state.snapshot().reply_context();
    assert!(menu.accept(&state, &context, "page", &page).is_err());
    menu.bind_request("page").unwrap();
    assert!(menu.bind_request("another").is_err());
    assert!(menu.accept(&state, &context, "another", &page).is_err());
    for field in 0..8 {
        let mut wrong = context.clone();
        match field {
            0 => wrong.input.stream.stream.0.push('x'),
            1 => wrong.input.stream.epoch += 1,
            2 => wrong.input.readiness_revision += 1,
            3 => wrong.actor.0 += 1,
            4 => wrong.branch.0.push('x'),
            5 => wrong.cursor.sequence += 1,
            6 => wrong.cursor.tick += 1,
            _ => wrong.revision += 1,
        }
        assert!(
            menu.accept(&state, &wrong, "page", &page).is_err(),
            "context {field}"
        );
    }
    for field in 0..3 {
        let mut wrong = page.clone();
        match field {
            0 => wrong.query.ability = "other_ability".into(),
            1 => wrong.query.offset += 1,
            _ => {
                wrong.query.phase = ProjectileChoicePhase::Ammunition {
                    launcher: EquippedProjectileWeapon {
                        item: item(1),
                        socket: 1,
                    },
                }
            }
        }
        assert!(
            menu.accept(&state, &context, "page", &wrong).is_err(),
            "query {field}"
        );
    }
    assert!(menu.choose(&state, 0).is_err());
    menu.accept(&state, &context, "page", &page).unwrap();
    assert_eq!(menu.rows(), vec!["known item 2 (carried, socket 0)"]);
    assert!(
        menu.accept(&state, &context, "page", &page).is_err(),
        "duplicate reply is not a new page"
    );
    assert_eq!(state.snapshot(), before);
}
#[test]
fn empty_filtered_page_and_full_page_remain_navigable_in_both_directions() {
    let state = state();
    let mut menu = menu(&state);
    accept(&mut menu, &state, "empty", vec![], Some(128));
    assert!(menu.has_next());
    assert!(menu.choose(&state, 0).is_err());
    let Command::ProjectileChoices { query, .. } = menu.next(&state).unwrap() else {
        panic!("query")
    };
    assert_eq!(query.offset, 128);
    assert_eq!(query.phase, ProjectileChoicePhase::Sources {});
    let choices: Vec<_> = (129..=144).map(source).collect();
    accept(&mut menu, &state, "full", choices.clone(), Some(144));
    assert_eq!(menu.choices(), choices);
    assert!(menu.has_next());
    assert!(menu.has_previous());
    let Command::ProjectileChoices { query, .. } = menu.previous(&state).unwrap() else {
        panic!("query")
    };
    assert_eq!(query.offset, 0);
    assert!(menu.choices().is_empty());
    assert!(!menu.has_next());
    accept(&mut menu, &state, "empty-again", vec![], Some(128));
    assert!(menu.has_next());
    assert!(!menu.has_previous());
}
#[test]
fn launcher_selection_binds_ammunition_query_and_final_closed_delivery() {
    let state = state();
    let before = state.snapshot();
    let mut menu = menu(&state);
    let launcher = EquippedProjectileWeapon {
        item: item(1),
        socket: 1,
    };
    accept(
        &mut menu,
        &state,
        "launcher",
        vec![ProjectileChoice::Launcher { launcher }],
        None,
    );
    let Command::ProjectileChoices { query, .. } = menu.choose(&state, 0).unwrap().unwrap() else {
        panic!("ammunition query")
    };
    assert_eq!(query.phase, ProjectileChoicePhase::Ammunition { launcher });
    assert_eq!(query.offset, 0);
    let exact = page(
        &menu,
        vec![ProjectileChoice::Ammunition { item: item(2) }],
        None,
    );
    menu.bind_request("ammunition").unwrap();
    let mut wrong = exact.clone();
    wrong.query.phase = ProjectileChoicePhase::Ammunition {
        launcher: EquippedProjectileWeapon {
            socket: 0,
            ..launcher
        },
    };
    assert!(menu
        .accept(
            &state,
            &state.snapshot().reply_context(),
            "ammunition",
            &wrong
        )
        .is_err());
    menu.accept(
        &state,
        &state.snapshot().reply_context(),
        "ammunition",
        &exact,
    )
    .unwrap();
    assert_eq!(menu.choose(&state, 0).unwrap(), None);
    assert_eq!(menu.step(), Step::Direction);
    assert_eq!(
        menu.action(&state, Direction::NorthEast).unwrap(),
        Action::UseAbility {
            id: "fixture_projectile".into(),
            invocation: AbilityInvocation::directional_projectile(
                Direction::NorthEast,
                ProjectileDeliverySelection::Launch {
                    launcher,
                    ammunition: item(2)
                }
            ),
            supplies: SupplySelection::Carried
        }
    );
    assert_eq!(state.snapshot(), before);
}
#[test]
fn source_socket_inventory_ability_and_authority_changes_invalidate_every_stage() {
    let state = state();
    let mut waiting = menu(&state);
    waiting.bind_request("page").unwrap();
    let page = page(&waiting, vec![source(2)], None);
    let mut selected = menu(&state);
    accept(&mut selected, &state, "chosen", vec![source(2)], None);
    selected.choose(&state, 0).unwrap();
    for field in 0..10 {
        let changed = changed(&state, |s| match field {
            0 => {
                s.has_control = false;
                s.readiness.admission = false;
            }
            1 => s.context.epoch += 1,
            2 => s.branch.0.push('x'),
            3 => s.readiness.revision += 1,
            4 => std::sync::Arc::make_mut(&mut s.state).revision += 1,
            5 => {
                let view = &mut std::sync::Arc::make_mut(&mut s.state).observation;
                view.inventory.remove(1);
                view.interactions
                    .as_mut()
                    .unwrap()
                    .inventory
                    .retain(|row| row.item != item(2));
            }
            6 => {
                std::sync::Arc::make_mut(&mut s.state)
                    .observation
                    .interactions
                    .as_mut()
                    .unwrap()
                    .inventory[1]
                    .equipped_slot = Some(0)
            }
            7 => {
                let equipment = std::sync::Arc::make_mut(&mut s.state)
                    .observation
                    .interactions
                    .as_mut()
                    .unwrap();
                equipment.inventory[0].equipped_slot = None;
                equipment.slots.pop();
            }
            8 => std::sync::Arc::make_mut(&mut s.state)
                .observation
                .combat
                .as_mut()
                .unwrap()
                .own_stats
                .as_mut()
                .unwrap()
                .abilities
                .clear(),
            _ => std::sync::Arc::make_mut(&mut s.state).observation.inventory[1].quantity += 1,
        });
        assert!(!waiting.current(&changed), "change {field}");
        assert!(waiting
            .clone()
            .accept(&changed, &changed.snapshot().reply_context(), "page", &page)
            .is_err());
        assert!(!selected.current(&changed));
        assert!(selected.action(&changed, Direction::North).is_err());
    }
}
#[test]
fn exact_preview_owns_projectile_terms_and_rejects_changed_source_direction_or_socket() {
    let state = state();
    let mut menu = menu(&state);
    accept(&mut menu, &state, "source", vec![source(2)], None);
    menu.choose(&state, 0).unwrap();
    let action = menu.action(&state, Direction::North).unwrap();
    assert!(crate::weapon_preview::explicit_action(&action));
    let mut gate = WeaponPreview::new(&state, action.clone()).unwrap();
    gate.bind_request("preview").unwrap();
    let Action::UseAbility {
        id,
        invocation,
        supplies,
    } = &action
    else {
        panic!("ability")
    };
    let response = AbilityWeaponPreview {
        ability: id.clone(),
        invocation: *invocation,
        supplies: supplies.clone(),
        phase: AbilityWeaponPreviewPhase::Fresh,
        cost: Some(DiagnosticCostBundle {
            stamina: None,
            focus: Some(DiagnosticCost {
                resource: crate::test_support::content("focus"),
                start: 1,
                resolution: 2,
            }),
            mana: None,
        }),
        preparation: 731,
        recovery: 229,
    };
    assert!(gate.clone().confirm(&state).is_err());
    for field in 0..4 {
        let mut wrong = response.clone();
        wrong.invocation = AbilityInvocation::directional_projectile(
            if field == 0 {
                Direction::South
            } else {
                Direction::North
            },
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Carried {
                    item: item(if field == 1 { 3 } else { 2 }),
                    socket: u16::from(field == 2),
                },
            },
        );
        if field == 3 {
            wrong.ability = "other".into();
        }
        assert!(gate
            .accept(&state, &state.snapshot().reply_context(), "preview", &wrong)
            .is_err());
        assert!(gate.clone().confirm(&state).is_err());
    }
    gate.accept(
        &state,
        &state.snapshot().reply_context(),
        "preview",
        &response,
    )
    .unwrap();
    let rows = gate.lines().join(" ");
    assert!(rows.contains("731 ticks"));
    assert!(rows.contains("229 ticks"));
    assert!(rows.contains("Focus"));
    let changed = changed(&state, |s| {
        std::sync::Arc::make_mut(&mut s.state).observation.inventory[1].quantity += 1
    });
    assert!(gate.clone().confirm(&changed).is_err());
    assert_eq!(gate.confirm(&state).unwrap(), action);
}
