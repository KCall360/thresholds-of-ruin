//! Encoded Service owner-disclosure and revision/authority contracts.
use super::*;

fn menu_scenario(
    ammunition: usize,
    concealed_launcher: bool,
    concealed_ammunition: bool,
) -> crate::Scenario {
    let mut scenario = projectile_scenario(ProjectileDeliveryKind::Launch);
    let package = scenario.package.take().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    if concealed_launcher {
        manifest["archetypes"]["fixture.secondary"]["appearance_pool"] =
            json!("fixture.hidden_weapon");
        manifest["appearance_pools"]["fixture.hidden_weapon"] =
            json!({"appearances":["unknown weapon"]});
    }
    manifest["characters"][0]["loadout"] = json!([
        {"archetype":"fixture.primary","quantity":1,"equipped_slot":0},
        {"archetype":"fixture.secondary","quantity":1,"equipped_slot":10}
    ]);
    for index in 0..ammunition {
        let id = format!("fixture.arrow_{index}");
        let mut archetype = manifest["archetypes"]["fixture.arrow"].clone();
        archetype["name"] = json!(format!("arrow {index}"));
        if concealed_ammunition && index == 0 {
            archetype["appearance_pool"] = json!("fixture.hidden_ammunition");
            manifest["appearance_pools"]["fixture.hidden_ammunition"] =
                json!({"appearances":["unknown ammunition"]});
        }
        manifest["archetypes"][&id] = archetype;
        manifest["characters"][0]["loadout"]
            .as_array_mut()
            .unwrap()
            .push(json!({"archetype":id,"quantity":2}));
    }
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            serde_json::from_value(manifest).unwrap(),
            package.region_defs().unwrap(),
            &toml::to_string(package.rules().unwrap().document()).unwrap(),
        )
        .unwrap(),
    ));
    scenario
}
fn query(phase: ProjectileChoicePhase) -> ProjectileChoiceQuery {
    ProjectileChoiceQuery {
        ability: "fixture.launch".into(),
        phase,
        offset: 0,
    }
}
fn request(wire: &Wire, actor: ActorId, query: ProjectileChoiceQuery) -> Request {
    Request::Command {
        context: wire.service.input_context(wire.clients[&actor].id).unwrap(),
        branch: wire.service.engine.branch().clone(),
        command: Command::ProjectileChoices {
            expected_revision: wire
                .service
                .reply_context(wire.clients[&actor].id)
                .unwrap()
                .revision,
            query,
        },
    }
}
fn fetch(wire: &mut Wire, query: ProjectileChoiceQuery) -> ProjectileChoicePage {
    let request = request(wire, SOURCE, query.clone());
    let before = saved(&wire.service);
    let expected = wire
        .service
        .reply_context(wire.clients[&SOURCE].id)
        .unwrap();
    let messages = wire.send(SOURCE, "choices", request);
    assert_eq!(
        saved(&wire.service),
        before,
        "menu query changes no journal, receipt, RNG, clock, holds, knowledge or resources"
    );
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, ServerMessage::Error { .. })),
        "{messages:?}"
    );
    let page = messages
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::ProjectileChoices {
                context,
                request_id,
                page,
            } => {
                assert_eq!(context, expected);
                assert_eq!(request_id, "choices");
                Some(*page)
            }
            _ => None,
        })
        .expect("encoded private projectile page");
    assert!(page.valid_for(&query));
    page
}
fn launcher(wire: &mut Wire) -> EquippedProjectileWeapon {
    let page = fetch(wire, query(ProjectileChoicePhase::Sources {}));
    let [ProjectileChoice::Launcher { launcher }] = page.choices.as_slice() else {
        panic!("one launcher: {page:?}")
    };
    *launcher
}
fn denied(wire: &mut Wire, request: Request, expected: ErrorCode) {
    let before = saved(&wire.service);
    let messages = wire.send(SOURCE, "denied-choices", request);
    assert!(
        messages.iter().any(
            |message| matches!(message, ServerMessage::Error { code, .. } if *code == expected)
        ),
        "{messages:?}"
    );
    assert!(!messages
        .iter()
        .any(|message| matches!(message, ServerMessage::ProjectileChoices { .. })));
    assert_eq!(saved(&wire.service), before);
}
#[test]
fn encoded_projectile_choices_page_inventory_order_without_cross_product_or_mutation() {
    let mut wire = Wire::connect(Engine::memory(menu_scenario(35, false, false)).unwrap());
    let launcher = launcher(&mut wire);
    let expected: Vec<_> = wire
        .service
        .engine
        .observation(SOURCE)
        .unwrap()
        .inventory
        .iter()
        .filter(|item| item.name.starts_with("arrow "))
        .map(|item| item.id)
        .collect();
    assert_eq!(expected.len(), 35);
    let mut query = query(ProjectileChoicePhase::Ammunition { launcher });
    let mut actual = Vec::new();
    let mut sizes = Vec::new();
    loop {
        let page = fetch(&mut wire, query.clone());
        sizes.push(page.choices.len());
        actual.extend(page.choices.iter().map(|choice| choice.item()));
        match page.next_offset {
            Some(next) => {
                assert!(next > query.offset);
                query.offset = next;
            }
            None => break,
        }
    }
    assert_eq!(sizes, [16, 16, 3]);
    assert_eq!(
        actual, expected,
        "canonical disclosed inventory order, exactly once"
    );
    query.offset = wire
        .service
        .engine
        .observation(SOURCE)
        .unwrap()
        .inventory
        .len() as u32;
    let terminal = fetch(&mut wire, query.clone());
    assert!(terminal.choices.is_empty() && terminal.next_offset.is_none());
    query.offset += 1;
    let bad = request(&wire, SOURCE, query);
    denied(&mut wire, bad, ErrorCode::InvalidRequest);
}
#[test]
fn encoded_projectile_choices_require_current_owner_launcher_and_controller_context() {
    let mut wire = Wire::connect(Engine::memory(menu_scenario(1, false, false)).unwrap());
    let launcher = launcher(&mut wire);
    for case in 0..5 {
        let mut bad = request(
            &wire,
            SOURCE,
            query(ProjectileChoicePhase::Ammunition { launcher }),
        );
        let Request::Command {
            context,
            branch,
            command:
                Command::ProjectileChoices {
                    expected_revision,
                    query,
                },
        } = &mut bad
        else {
            unreachable!()
        };
        let code = match case {
            0 => {
                *expected_revision += 1;
                // This generation was never issued on the current attachment.
                ErrorCode::StaleContext
            }
            1 => {
                *branch = BranchId("foreign-branch".into());
                ErrorCode::WrongBranch
            }
            2 => {
                context.stream.epoch += 1;
                ErrorCode::StaleContext
            }
            3 => {
                let mut foreign = launcher;
                let state = saved(&wire.service);
                let internal = ItemId(
                    live_items(&state)
                        .iter()
                        .find(|(_, item)| item["spec"]["name"] == "secondary sword")
                        .unwrap()
                        .0
                        .parse()
                        .unwrap(),
                );
                foreign.item = wire.service.engine.target_scope(TARGET).item(internal);
                query.phase = ProjectileChoicePhase::Ammunition { launcher: foreign };
                ErrorCode::InvalidAction
            }
            4 => {
                query.phase = ProjectileChoicePhase::Ammunition {
                    launcher: EquippedProjectileWeapon {
                        socket: 0,
                        ..launcher
                    },
                };
                ErrorCode::InvalidAction
            }
            _ => unreachable!(),
        };
        denied(&mut wire, bad, code);
    }
    let stale = request(&wire, SOURCE, query(ProjectileChoicePhase::Sources {}));
    wire.send(SOURCE, "release-choices", Request::ReleaseControl);
    denied(&mut wire, stale, ErrorCode::StaleContext);
    let bad = request(&wire, SOURCE, query(ProjectileChoicePhase::Sources {}));
    denied(&mut wire, bad, ErrorCode::NotController);
}
#[test]
fn encoded_projectile_choices_withhold_unidentified_type_capabilities() {
    let mut wire = Wire::connect(Engine::memory(menu_scenario(2, false, true)).unwrap());
    let launcher = launcher(&mut wire);
    let page = fetch(
        &mut wire,
        query(ProjectileChoicePhase::Ammunition { launcher }),
    );
    let inventory = &wire.service.engine.observation(SOURCE).unwrap().inventory;
    let known = inventory
        .iter()
        .find(|item| item.name == "arrow 1")
        .unwrap()
        .id;
    assert_eq!(page.choices, [ProjectileChoice::Ammunition { item: known }]);
    let mut wire = Wire::connect(Engine::memory(menu_scenario(1, true, false)).unwrap());
    assert!(fetch(&mut wire, query(ProjectileChoicePhase::Sources {}))
        .choices
        .is_empty());
    let inventory = wire.service.engine.observation(SOURCE).unwrap();
    let hidden = inventory
        .inventory
        .iter()
        .find(|item| item.name == "unknown weapon")
        .unwrap();
    assert!(!hidden.identified);
    let bad = request(
        &wire,
        SOURCE,
        query(ProjectileChoicePhase::Ammunition {
            launcher: EquippedProjectileWeapon {
                item: hidden.id,
                socket: 10,
            },
        }),
    );
    denied(&mut wire, bad, ErrorCode::InvalidAction);
}
#[test]
fn encoded_throw_choices_preserve_equipped_source_mode_and_explicit_socket() {
    let mut wire =
        Wire::connect(Engine::memory(projectile_scenario(ProjectileDeliveryKind::Throw)).unwrap());
    let query = ProjectileChoiceQuery {
        ability: "fixture.throw".into(),
        phase: ProjectileChoicePhase::Sources {},
        offset: 0,
    };
    let page = fetch(&mut wire, query);
    assert!(matches!(
        page.choices.as_slice(),
        [ProjectileChoice::Throw {
            source: ThrownSourceSelection::Equipped { socket: 10, .. }
        }]
    ));
    let invalid = request(
        &wire,
        SOURCE,
        ProjectileChoiceQuery {
            ability: "basic_melee".into(),
            phase: ProjectileChoicePhase::Sources {},
            offset: 0,
        },
    );
    denied(&mut wire, invalid, ErrorCode::InvalidAction);
}
#[test]
fn encoded_projectile_choices_are_never_disclosed_to_spectators() {
    let mut wire = Wire::connect(Engine::memory(menu_scenario(1, false, false)).unwrap());
    let spectator = wire
        .service
        .connect(
            &Account {
                user: "projectile-spectator".into(),
                token: "test-only".into(),
                role: AccessRole::Spectator,
                actors: [SOURCE].into(),
            },
            "spectator".into(),
        )
        .unwrap();
    let _owner = wire.clients.insert(SOURCE, spectator).unwrap();
    wire.send(
        SOURCE,
        "spectator-attach",
        Request::Attach { actor: SOURCE },
    );
    let bad = request(&wire, SOURCE, query(ProjectileChoicePhase::Sources {}));
    denied(&mut wire, bad, ErrorCode::Unauthorized);
}
