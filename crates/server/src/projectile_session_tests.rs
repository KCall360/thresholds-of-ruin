//! Ordinary physical release owns its unit, paid terms, and source-free replay.
use super::*;
use tor_simulation::{
    abilities::{FrozenWeaponSelection, Plan, PlannedEffect, ProjectileSource},
    content_ids::{AbilityId, ExpertiseId},
    expertise::{ExpertiseCategory, ExpertiseDef},
    projectile_profiles::{HitRecovery, ProjectileDeliveryKind},
    resources::Resource,
    rules::{Reach, StrikeSource, TimingSource},
    EquipmentSlotId, ItemId,
};

fn projectile_scenario(delivery: ProjectileDeliveryKind) -> crate::Scenario {
    let mut scenario = scenario();
    let package = scenario.package.take().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    let mut rules = package.rules().unwrap().document().clone();
    rules.expertises.push(ExpertiseDef {
        id: ExpertiseId::new("fixture.projectile").unwrap(),
        name: "Fixture projectile".into(),
        category: ExpertiseCategory::Equipment,
    });
    let basic = rules
        .abilities
        .iter()
        .find(|row| row.basic_attack)
        .unwrap()
        .clone();
    for (id, kind) in [
        ("fixture.throw", ProjectileDeliveryKind::Throw),
        ("fixture.launch", ProjectileDeliveryKind::Launch),
    ] {
        let mut ability = basic.clone();
        ability.id = AbilityId::new(id).unwrap();
        ability.name = id.into();
        ability.basic_attack = false;
        ability.invocation = tor_simulation::abilities::InvocationKind::DirectionalProjectile;
        ability.reach = Reach::Projectile;
        ability.strikes = StrikeSource::Projectile { delivery: kind };
        ability.timing.source = TimingSource::SelectedWeapon;
        rules.constants.default_abilities.push(ability.id.clone());
        rules.abilities.push(ability);
    }
    let launch = delivery == ProjectileDeliveryKind::Launch;
    let equipment = &mut manifest["archetypes"]["fixture.secondary"]["equipment"];
    equipment["attack"]["bonus"] = json!(100);
    equipment["weapon_profile"] = json!({
        "expertise":"fixture.projectile",
        "launcher":if launch {json!({"family":"fixture.bow","range":{"short_feet":20,"long_feet":60}})} else {Value::Null},
        "base_traits":if launch {json!([])} else {json!([{"kind":"thrown","range":{"short_feet":20,"long_feet":60}}])},
        "expert_traits":{"add":[],"remove":[]}
    });
    if launch {
        manifest["archetypes"]["fixture.arrow"] = json!({
            "name":"fixture arrow", "class":"weapon", "stackable":true,
            "projectile":{"family":"fixture.bow","recovery":"consumed_on_hit",
                "damage":{"components":[{"category":"keen","descriptor":null,"amount":{"type":"fixed","value":11}}],
                    "primary":{"category":"keen","descriptor":null}}}
        });
        manifest["characters"][0]["loadout"]
            .as_array_mut()
            .unwrap()
            .push(json!({"archetype":"fixture.arrow","quantity":2}));
    }
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            serde_json::from_value(manifest).unwrap(),
            package.region_defs().unwrap(),
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    scenario
}

#[path = "projectile_choice_session_tests.rs"]
mod choice_tests;

fn live_items(saved: &Value) -> &serde_json::Map<String, Value> {
    let pool = saved["game"]["items"].as_u64().unwrap() as usize;
    saved["shared"]["items"][pool].as_object().unwrap()
}

fn resources_equal(service: &Service, mana: (u32, u32), focus: (u32, u32)) {
    for (id, expected) in [("mana", mana), ("focus", focus)] {
        let row = resource(service, id);
        assert_eq!((row.balance, row.reserved), expected, "{id}");
    }
}

fn paid_projectile_reopens(delivery: ProjectileDeliveryKind) {
    let launch = delivery == ProjectileDeliveryKind::Launch;
    let ability = if launch {
        "fixture.launch"
    } else {
        "fixture.throw"
    };
    let damage = if launch { 11 } else { 7 };
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("projectile.db");
        let policy = crate::SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let mut wire = Wire::connect(
            Engine::open_with_policy(&path, projectile_scenario(delivery), policy.clone()).unwrap(),
        );
        let observation = wire.service.engine.observation(SOURCE).unwrap();
        let weapon = observation
            .inventory
            .iter()
            .find(|item| item.name == "secondary sword")
            .unwrap()
            .id;
        let socket = observation
            .interactions
            .as_ref()
            .unwrap()
            .inventory
            .iter()
            .find(|offer| offer.item == weapon)
            .unwrap()
            .equipped_slot
            .unwrap();
        assert_eq!(socket, 10);
        let selected = if launch {
            let ammunition = observation
                .inventory
                .iter()
                .find(|item| item.name == "fixture arrow")
                .unwrap()
                .id;
            ProjectileDeliverySelection::Launch {
                launcher: EquippedProjectileWeapon {
                    item: weapon,
                    socket,
                },
                ammunition,
            }
        } else {
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Equipped {
                    item: weapon,
                    socket,
                },
            }
        };
        let action = Action::UseAbility {
            id: ability.into(),
            invocation: AbilityInvocation {
                aim: AbilityAim::DirectionalProjectile {
                    direction: Direction::East,
                    delivery: selected,
                },
                weapon: WeaponSelection::Default,
            },
            supplies: SupplySelection::Carried,
        };
        let encoded = serde_json::to_value(&action).unwrap();
        assert!(
            encoded["invocation"]["aim"].get("target").is_none(),
            "only direction and exact delivery are requested"
        );
        let backend = wire.service.engine.decode_action(SOURCE, &action).unwrap();
        assert_eq!(wire.service.engine.encode_action(SOURCE, &backend), action);
        let crate::actions::Action::UseAbility { invocation, .. } = backend else {
            panic!("ability")
        };
        assert_eq!(invocation.weapon, crate::actions::WeaponSelection::Default);
        let crate::actions::AbilityAim::DirectionalProjectile {
            delivery: decoded, ..
        } = invocation.aim
        else {
            panic!("directional delivery")
        };
        let (weapon_id, paid_id) = match decoded {
            crate::actions::ProjectileDeliverySelection::Throw {
                source: crate::actions::ThrownSourceSelection::Equipped { item, socket: 10 },
            } => (item, item),
            crate::actions::ProjectileDeliverySelection::Launch {
                launcher,
                ammunition,
            } => {
                assert_eq!(launcher.socket, socket);
                (launcher.item, ammunition)
            }
            _ => panic!("exact public equipment selection"),
        };
        let hp_before = hp(&wire.service, TARGET);
        let initial = saved(&wire.service);
        let target_location = live_actor(&initial, TARGET)["location"].clone();
        let request = wire.request(SOURCE, action.clone());
        let queued_receipt = ack(wire.send(SOURCE, "release", request.clone()));
        let identity = exact_receipt(&queued_receipt);
        let queued = saved(&wire.service);
        assert_eq!(
            ack(wire.send(SOURCE, "release", request.clone())),
            queued_receipt
        );
        assert_eq!(
            saved(&wire.service),
            queued,
            "queued retry cannot claim or release"
        );
        wire.service.run_until_blocked();
        for actor in ACTORS {
            wire.drain(actor);
        }
        let started = ack(wire.send(SOURCE, "release", request.clone()));
        assert_eq!(exact_receipt(&started), identity);
        assert!(matches!(
            started,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ));
        resources_equal(&wire.service, (19, 2), (20, 2));
        assert_eq!(
            hp(&wire.service, TARGET),
            hp_before,
            "preparation has no collision or damage"
        );
        let pending = saved(&wire.service);
        let previous = live_actor(&pending, SOURCE);
        let frozen: Plan = serde_json::from_value(previous["pending"]["plan"].clone()).unwrap();
        let Plan::Projectile(plan) = &frozen else {
            panic!("owned projectile plan")
        };
        assert_eq!(plan.actor, tor_simulation::ActorId(SOURCE.0));
        assert_eq!(plan.ability.as_str(), ability);
        let expected_selection = if launch {
            tor_simulation::abilities::ProjectileDeliverySelection::Launch {
                launcher: tor_simulation::abilities::EquippedProjectileWeapon {
                    item: ItemId(weapon_id),
                    socket: EquipmentSlotId(socket),
                },
                ammunition: ItemId(paid_id),
            }
        } else {
            tor_simulation::abilities::ProjectileDeliverySelection::Throw {
                source: tor_simulation::abilities::ThrownSourceSelection::Equipped {
                    item: ItemId(weapon_id),
                    socket: EquipmentSlotId(socket),
                },
            }
        };
        assert_eq!(plan.selection, expected_selection);
        assert_eq!(
            plan.direction,
            tor_simulation::abilities::StepDirection::East
        );
        assert_eq!((plan.preparation, plan.recovery), (300, 100));
        assert_eq!((plan.range.short_feet, plan.range.long_feet), (20, 60));
        assert!(plan.bonus >= 100);
        let captured_weapon = match &plan.source {
            ProjectileSource::ThrowEquipped { weapon } if !launch => weapon,
            ProjectileSource::Launch {
                weapon,
                ammunition,
                payload,
            } if launch => {
                assert_eq!(*ammunition, ItemId(paid_id));
                assert_eq!(payload.recovery, HitRecovery::ConsumedOnHit);
                assert_eq!(payload.family.as_str(), "fixture.bow");
                weapon
            }
            _ => panic!("frozen exact delivery source"),
        };
        let FrozenWeaponSelection::Equipped {
            item,
            socket: captured_socket,
            secondary,
            ..
        } = captured_weapon
        else {
            panic!("equipped source")
        };
        assert_eq!(
            (*item, *captured_socket, *secondary),
            (ItemId(weapon_id), EquipmentSlotId(socket), true)
        );
        let [PlannedEffect::Damage { damage: payload }] = plan.effects.as_slice() else {
            panic!("one frozen damage payload")
        };
        assert_eq!(
            payload.components()[0].amount(),
            tor_simulation::damage::DamageAmount::Fixed(damage)
        );
        let cost = plan.cost.unwrap();
        for (resource, phases) in [(Resource::Mana, (1, 2)), (Resource::Focus, (0, 2))] {
            let row = cost.entries().find(|row| row.resource == resource).unwrap();
            assert_eq!((row.start, row.resolution), phases);
        }
        assert_eq!(previous["pending"]["active"], true);
        assert_eq!(previous["pending"]["remaining"], 300);
        assert_eq!(
            previous["pending"]["item_payment"],
            json!({"kind":"held","refs":[{"cost_ordinal":0,"item":paid_id}]})
        );
        let paid = &live_items(&pending)[&paid_id.to_string()];
        assert_eq!(paid["quantity"], if launch { 2 } else { 1 });
        assert_eq!(paid["location"], json!({"Carried":SOURCE.0}));
        assert_eq!(
            paid["holds"],
            json!([{"key":{"lineage":{"actor":SOURCE.0,"origin":previous["pending"]["origin"]},"cost_ordinal":0},"quantity":1}])
        );
        assert_eq!(previous["equipment"][socket.to_string()], weapon_id);
        assert_eq!(ack(wire.send(SOURCE, "release", request.clone())), started);
        assert_eq!(
            saved(&wire.service),
            pending,
            "started retry retains exact claim and terms"
        );
        wire.service.engine.flush().unwrap();
        drop(wire);

        let engine =
            Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy.clone())
                .unwrap();
        if interval == 4096 {
            assert!(engine.recovery_profile().records_replayed > 0);
        }
        assert_eq!(
            saved_engine(&engine),
            pending,
            "source-free Engine restore precedes Service startup pause"
        );
        let service = Service::new(engine);
        let paused_state = saved(&service);
        let paused_actor = live_actor(&paused_state, SOURCE);
        assert_eq!(paused_actor["pending"]["active"], false);
        for field in [
            "plan",
            "hold",
            "origin",
            "intention",
            "progress",
            "item_payment",
            "threats",
            "remaining",
        ] {
            assert_eq!(
                paused_actor["pending"][field], previous["pending"][field],
                "pause preserves {field}"
            );
        }
        for field in ["combat", "equipment", "location"] {
            assert_eq!(
                paused_actor[field], previous[field],
                "pause preserves {field}"
            );
        }
        for field in ["rules_rng", "tick"] {
            assert_eq!(paused_state["game"][field], pending["game"][field]);
        }
        assert_eq!(
            live_items(&paused_state),
            live_items(&pending),
            "startup retains reciprocal exact item claims"
        );
        let mut wire = Wire::connect_service(service);
        assert_eq!(
            saved(&wire.service),
            paused_state,
            "owner attach does not implicitly resume"
        );
        let paused = ack(wire.send(SOURCE, "release", request.clone()));
        assert_eq!(exact_receipt(&paused), identity);
        assert!(matches!(
            paused,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Paused,
                ..
            }
        ));
        assert_eq!(saved(&wire.service), paused_state);
        let snapshot = wire
            .send(SOURCE, "resume-snapshot", Request::Snapshot)
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::Snapshot { snapshot, .. } => Some(snapshot),
                _ => None,
            })
            .unwrap();
        assert!(snapshot.readiness.resume.contains(&identity.0));
        let resume = Request::Command {
            context: snapshot.reply_context().input,
            branch: snapshot.branch.clone(),
            command: Command::ResumeIntention {
                intention: identity.0.clone(),
            },
        };
        ack(wire.send(SOURCE, "resume-release", resume));
        let mut completed = None;
        for ordinal in 0..12 {
            wire.service.run_until_blocked();
            for actor in ACTORS {
                wire.drain(actor);
            }
            let receipt = ack(wire.send(SOURCE, "release", request.clone()));
            assert_eq!(exact_receipt(&receipt), identity);
            if matches!(
                receipt,
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Resolved,
                    ..
                }
            ) {
                completed = Some(receipt);
                break;
            }
            let actor = wire.service.engine.next_actor().unwrap();
            let wait = wire.request(actor, Action::Wait);
            ack(wire.send(actor, &format!("clock-{ordinal}"), wait));
        }
        let completed = completed.expect("ordinary scheduler resolves one physical release");
        assert_eq!(
            hp(&wire.service, TARGET),
            hp_before - u64::from(damage),
            "one collision with the first real creature, using the selected payload"
        );
        resources_equal(&wire.service, (17, 0), (18, 0));
        let settled = saved(&wire.service);
        assert!(live_actor(&settled, SOURCE)["pending"].is_null());
        let primary = live_actor(&initial, SOURCE)["equipment"]["0"]
            .as_u64()
            .unwrap()
            .to_string();
        assert_eq!(
            live_actor(&settled, SOURCE)["equipment"]["0"],
            live_actor(&initial, SOURCE)["equipment"]["0"]
        );
        assert_eq!(
            live_items(&settled)[&primary],
            live_items(&initial)[&primary],
            "release cannot replace or mutate the unselected primary"
        );
        let items = live_items(&settled);
        assert_eq!(
            items.len(),
            live_items(&initial).len(),
            "no extra physical unit is created"
        );
        assert!(items
            .values()
            .all(|item| item["holds"].as_array().unwrap().is_empty()));
        let paid = &items[&paid_id.to_string()];
        assert_eq!(paid["quantity"], 1);
        if launch {
            assert_eq!(paid["location"], json!({"Carried":SOURCE.0}));
            assert_eq!(
                live_actor(&settled, SOURCE)["equipment"][socket.to_string()],
                weapon_id
            );
            assert_eq!(
                items
                    .values()
                    .filter(|item| item["spec"]["projectile"].is_object())
                    .map(|item| item["quantity"].as_u64().unwrap())
                    .sum::<u64>(),
                1,
                "one consumed-on-hit arrow, no ground recovery"
            );
        } else {
            assert_eq!(
                paid["location"],
                json!({"Ground":target_location}),
                "durable thrown unit recovers at the creature actually hit"
            );
            assert!(live_actor(&settled, SOURCE)["equipment"]
                .get(socket.to_string())
                .is_none());
        }
        assert_eq!(
            ack(wire.send(SOURCE, "release", request.clone())),
            completed
        );
        assert_eq!(
            saved(&wire.service),
            settled,
            "completed retry never releases or pays again"
        );
        wire.service.engine.flush().unwrap();
        drop(wire);

        let engine = Engine::open_with_policy(&path, crate::Scenario::two_room(0), policy).unwrap();
        if interval == 4096 {
            assert!(engine.recovery_profile().records_replayed > 0);
        }
        assert_eq!(
            saved_engine(&engine),
            settled,
            "completed source-free replay preserves settlement, landing and RNG"
        );
        let mut wire = Wire::connect(engine);
        assert_eq!(saved(&wire.service), settled);
        assert_eq!(
            ack(wire.send(SOURCE, "release", request.clone())),
            completed
        );
        assert_eq!(saved(&wire.service), settled);
        let mut altered = request;
        let Request::Command {
            command:
                Command::Act {
                    action: Action::UseAbility { invocation, .. },
                    ..
                },
            ..
        } = &mut altered
        else {
            panic!("ability request")
        };
        let AbilityAim::DirectionalProjectile { direction, .. } = &mut invocation.aim else {
            panic!("directional aim")
        };
        *direction = Direction::West;
        assert!(
            wire.send(SOURCE, "release", altered)
                .iter()
                .any(|message| matches!(message, ServerMessage::Error { .. })),
            "a receipt ID cannot change its frozen direction"
        );
        assert_eq!(saved(&wire.service), settled);
    }
}

#[test]
fn encoded_paid_throw_pending_completed_source_free_reopen_recovers_exact_unit() {
    paid_projectile_reopens(ProjectileDeliveryKind::Throw);
}

#[test]
fn encoded_paid_launch_pending_completed_source_free_reopen_consumes_one_arrow() {
    paid_projectile_reopens(ProjectileDeliveryKind::Launch);
}

fn standalone_stack_scenario() -> crate::Scenario {
    let mut scenario = projectile_scenario(ProjectileDeliveryKind::Throw);
    let package = scenario.package.take().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    manifest["archetypes"]["fixture.secondary"]["stackable"] = json!(true);
    manifest["archetypes"]["fixture.secondary"]["equipment"]["weapon_profile"]["thrown_payload"] = json!({
        "recovery":"consumed_on_hit", "damage":{"components":[{"category":"keen","amount":{"type":"fixed","value":11}}],
        "primary":{"category":"keen"}}});
    manifest["characters"][0]["loadout"][1] = json!({"archetype":"fixture.secondary","quantity":3});
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

#[test]
fn encoded_standalone_stack_storage_rejection_preserves_admission_and_actual_release() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("standalone-stack.db");
    let initial = Engine::open(&path, standalone_stack_scenario()).unwrap();
    initial.flush().unwrap();
    drop(initial);
    let rejecting = crate::SavePolicy {
        max_pending_bytes: 1,
        ..Default::default()
    };
    let mut wire = Wire::connect(
        Engine::open_with_policy(&path, crate::Scenario::two_room(999), rejecting.clone()).unwrap(),
    );
    let weapon = wire
        .service
        .engine
        .observation(SOURCE)
        .unwrap()
        .inventory
        .into_iter()
        .find(|item| item.name == "secondary sword")
        .unwrap()
        .id;
    let action = Action::UseAbility {
        id: "fixture.throw".into(),
        invocation: AbilityInvocation {
            aim: AbilityAim::DirectionalProjectile {
                direction: Direction::East,
                delivery: ProjectileDeliverySelection::Throw {
                    source: ThrownSourceSelection::Carried {
                        item: weapon,
                        socket: 10,
                    },
                },
            },
            weapon: WeaponSelection::Default,
        },
        supplies: SupplySelection::Carried,
    };
    let request = wire.request(SOURCE, action.clone());
    let before = saved(&wire.service);
    let paid = live_items(&before)
        .iter()
        .find(|(_, row)| row["spec"]["name"] == "secondary sword")
        .unwrap()
        .0
        .clone();
    assert_eq!(live_items(&before)[&paid]["quantity"], 3);
    assert!(live_actor(&before, SOURCE)["equipment"].get("10").is_none());
    let accepted = wire.service.engine.save_status().accepted_sequence;
    let history = wire
        .service
        .engine
        .history(SOURCE, "weapon-1", None, 100)
        .unwrap();
    for _ in 0..2 {
        let replies = wire.send(SOURCE, "stack-release", request.clone());
        assert!(
            replies.iter().any(|reply| matches!(
                reply,
                ServerMessage::Error {
                    code: ErrorCode::StorageFailure,
                    ..
                }
            )),
            "{replies:?}"
        );
        assert!(!replies
            .iter()
            .any(|reply| matches!(reply, ServerMessage::Ack { .. })));
        assert_eq!(
            saved(&wire.service),
            before,
            "rejected admission claims, pays and allocates nothing"
        );
        assert_eq!(
            wire.service
                .engine
                .history(SOURCE, "weapon-1", None, 100)
                .unwrap(),
            history
        );
        assert_eq!(
            wire.service.engine.save_status().accepted_sequence,
            accepted
        );
    }
    drop(wire);
    let mut wire = Wire::connect(Engine::open(&path, crate::Scenario::two_room(0)).unwrap());
    let request = wire.request(SOURCE, action);
    let admitted = ack(wire.send(SOURCE, "stack-release", request.clone()));
    let queued = saved(&wire.service);
    assert_ne!(
        queued, before,
        "the same valid action publishes once storage permits it"
    );
    assert_eq!(ack(wire.send(SOURCE, "stack-release", request)), admitted);
    assert_eq!(saved(&wire.service), queued);
    let health = hp(&wire.service, TARGET);
    wire.service.engine.flush().unwrap();
    drop(wire);

    // Refuse each real candidate before publishing it, then retry that exact
    // operation with healthy storage. The terminal candidate must release the
    // held unit and damage the target, not merely reject a setup transaction.
    let mut completed = false;
    for ordinal in 0..12 {
        let mut engine =
            Engine::open_with_policy(&path, crate::Scenario::two_room(0), rejecting.clone())
                .unwrap();
        let execute = engine.next_intention_actor().is_some();
        let actor = engine.next_actor().unwrap_or(SOURCE);
        let branch = engine.branch().clone();
        let command = crate::journal::Command::Act {
            expected_boundary: engine.causal_boundary(actor).unwrap(),
            action: crate::actions::Action::Wait,
        };
        let before = saved_engine(&engine);
        let sequence = engine.save_status().accepted_sequence;
        let history = engine.history(SOURCE, "weapon-1", None, 100).unwrap();
        let durable = crate::inspect_save(&path).unwrap();
        for _ in 0..2 {
            let error = if execute {
                engine.execute_next_intention().unwrap_err()
            } else {
                engine
                    .command(
                        "weapon-2",
                        "stack-fault",
                        actor,
                        &format!("clock-{ordinal}"),
                        &branch,
                        command.clone(),
                    )
                    .unwrap_err()
            };
            assert_eq!(error.code, ErrorCode::StorageFailure);
            assert_eq!(saved_engine(&engine), before, "candidate {ordinal}: exact inventory, claim, payment, RNG, actor and revision rollback");
            assert_eq!(
                engine.history(SOURCE, "weapon-1", None, 100).unwrap(),
                history
            );
            assert_eq!(engine.save_status().accepted_sequence, sequence);
            assert_eq!(crate::inspect_save(&path).unwrap(), durable);
        }
        drop(engine);
        let mut healthy = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
        if execute {
            healthy
                .execute_next_intention()
                .unwrap()
                .expect("queued candidate executes");
        } else {
            let accepted = healthy
                .command(
                    "weapon-2",
                    "stack-fault",
                    actor,
                    &format!("clock-{ordinal}"),
                    &branch,
                    command.clone(),
                )
                .unwrap();
            let after = saved_engine(&healthy);
            let retry = healthy
                .command(
                    "weapon-2",
                    "stack-fault",
                    actor,
                    &format!("clock-{ordinal}"),
                    &branch,
                    command,
                )
                .unwrap();
            assert!(retry.duplicate);
            assert_eq!(retry.entry, accepted.entry);
            assert_eq!(saved_engine(&healthy), after);
        }
        let after = saved_engine(&healthy);
        assert_ne!(
            after, before,
            "successful candidate must make the withheld transition"
        );
        if live_items(&after)[&paid]["quantity"] == 2 {
            assert_eq!(live_items(&before)[&paid]["quantity"], 3);
            assert_eq!(live_items(&before)[&paid]["holds"][0]["quantity"], 1);
            assert!(live_items(&after)[&paid]["holds"]
                .as_array()
                .unwrap()
                .is_empty());
            assert_eq!(
                healthy.observation(TARGET).unwrap().combat.unwrap().hp,
                health - 11
            );
            assert!(live_actor(&after, SOURCE)["equipment"].get("10").is_none());
            assert!(live_actor(&after, SOURCE)["pending"].is_null());
            healthy.flush().unwrap();
            drop(healthy);
            let reopened = Engine::open(&path, crate::Scenario::two_room(42)).unwrap();
            assert_eq!(saved_engine(&reopened), after);
            completed = true;
            break;
        }
        healthy.flush().unwrap();
    }
    assert!(
        completed,
        "bounded ordinary waits must reach the actual refused then retried release"
    );
}
