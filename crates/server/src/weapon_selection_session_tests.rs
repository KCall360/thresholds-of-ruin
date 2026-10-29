//! Ordinary encoded selected-weapon receipts and source-free persistent replay.
use super::*;
use serde_json::{json, Value};

const SOURCE: ActorId = ActorId(1);
const TARGET: ActorId = ActorId(2);
const ACTORS: [ActorId; 2] = [SOURCE, TARGET];

fn scenario() -> crate::Scenario {
    let mut scenario = crate::public_plan_test_fixture::scenario(false);
    let package = scenario.package.take().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    let mut rules = package.rules().unwrap().document().clone();
    for resource in &mut rules.resources {
        resource.recovery_period = 100_000;
        resource.capacity.required_flag = None;
    }
    let basic = rules
        .abilities
        .iter_mut()
        .find(|ability| ability.basic_attack)
        .unwrap();
    basic.cost = Some(tor_simulation::rules::Cost {
        resource: tor_simulation::resources::Resource::Mana,
        start: 1,
        resolution: 2,
    });
    basic.timing = tor_simulation::rules::Timing {
        source: tor_simulation::rules::TimingSource::Fixed {
            prepare: 300,
            recover: 100,
        },
        scaled_by: None,
    };
    basic.strikes = serde_json::from_value(json!({"kind":"authored", "strikes":[{
        "offset":0,"unlock":null,"check":null,"on_success":[{"kind":"damage","damage":{"kind":"selected_weapon","impact_bonus":0}}],
        "on_failure":[],"descriptors":[],"riders":[]
    }]})).unwrap();
    let modifiers = manifest["creatures"]["species"]["reader"]["modifiers"]
        .as_array_mut()
        .unwrap();
    *modifiers = vec![
        json!({"scope":"always","op":{"kind":"set","stat":{"kind":"resource_max","resource":"focus"},"value":20}}),
        json!({"scope":"always","op":{"kind":"set","stat":{"kind":"resource_max","resource":"mana"},"value":20}}),
        json!({"scope":"always","op":{"kind":"set","stat":{"kind":"health"},"value":100}}),
    ];
    for (id, name, damage) in [
        ("fixture.primary", "primary sword", 3),
        ("fixture.secondary", "secondary sword", 7),
    ] {
        manifest["archetypes"][id] = json!({"name":name, "class":"weapon", "equipment":{
            "slot":"weapon", "attack":{"skill":"heavy_weaponry","bonus":0,"wind_up":300,"recovery":100,
                "damage":{"components":[{"category":"keen","descriptor":null,"amount":{"type":"fixed","value":damage}}],"primary":{"category":"keen","descriptor":null}}}
        }});
    }
    manifest["characters"][0]["loadout"] = json!([
        {"archetype":"fixture.primary","quantity":1,"equipped_slot":0},
        {"archetype":"fixture.secondary","quantity":1,"equipped_slot":10}
    ]);
    let regions = package.region_defs().unwrap();
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            serde_json::from_value(manifest).unwrap(),
            regions,
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    scenario
}

struct Wire {
    service: Service,
    clients: BTreeMap<ActorId, Connection>,
}
impl Wire {
    fn connect(engine: Engine) -> Self {
        Self::connect_service(Service::new(engine))
    }
    fn connect_service(service: Service) -> Self {
        let mut wire = Self {
            service,
            clients: BTreeMap::new(),
        };
        for actor in ACTORS {
            let client = wire
                .service
                .connect(
                    &Account {
                        user: format!("weapon-{}", actor.0),
                        token: "test-only".into(),
                        role: AccessRole::Player,
                        actors: [actor].into(),
                    },
                    "encoded-weapon".into(),
                )
                .unwrap();
            wire.clients.insert(actor, client);
            wire.send(actor, "attach", Request::Attach { actor });
            wire.send(actor, "control", Request::AcquireControl);
        }
        wire
    }
    fn drain(&mut self, actor: ActorId) -> Vec<ServerMessage> {
        std::iter::from_fn(|| {
            self.clients
                .get_mut(&actor)
                .unwrap()
                .messages
                .try_recv_frame()
                .ok()
        })
        .map(|frame| decode_response(&frame.text).unwrap())
        .collect()
    }
    fn send(&mut self, actor: ActorId, id: &str, request: Request) -> Vec<ServerMessage> {
        let encoded = serde_json::to_string(&ClientMessage::Request {
            request_id: id.into(),
            request: Box::new(request),
        })
        .unwrap();
        let ClientMessage::Request {
            request_id,
            request,
        } = decode_request(&encoded).unwrap()
        else {
            panic!("request")
        };
        self.service
            .handle(self.clients[&actor].id, request_id, *request);
        self.drain(actor)
    }
    fn request(&mut self, actor: ActorId, action: Action) -> Request {
        let snapshot = self
            .send(actor, "snapshot", Request::Snapshot)
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::Snapshot { snapshot, .. } => Some(snapshot),
                _ => None,
            })
            .expect("encoded current snapshot");
        assert!(snapshot.has_control && snapshot.readiness.admission);
        Request::Command {
            context: snapshot.reply_context().input,
            branch: snapshot.branch.clone(),
            command: Command::Act { action },
        }
    }
}
fn ack(messages: Vec<ServerMessage>) -> RequestReceipt {
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Error { .. })),
        "{messages:?}"
    );
    messages
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::Ack { receipt, .. } => Some(receipt),
            _ => None,
        })
        .expect("encoded acknowledgement")
}
fn saved(service: &Service) -> Value {
    saved_engine(&service.engine)
}
fn saved_engine(engine: &Engine) -> Value {
    serde_json::to_value(crate::engine::Checkpoint::capture(engine).encode("weapon-service", 0))
        .unwrap()
}
fn live_actor(saved: &Value, actor: ActorId) -> &Value {
    let index = saved["game"]["actors"].as_u64().unwrap() as usize;
    &saved["shared"]["actors"][index][actor.0.to_string()]
}
fn hp(service: &Service, actor: ActorId) -> u64 {
    service
        .engine
        .state(actor)
        .unwrap()
        .observation
        .combat
        .unwrap()
        .hp
}

fn resource(service: &Service, id: &str) -> ResourceView {
    service
        .engine
        .state(SOURCE)
        .unwrap()
        .observation
        .combat
        .unwrap()
        .own_stats
        .unwrap()
        .resources
        .into_iter()
        .find(|row| row.resource.id == id)
        .unwrap()
}
fn contains_object(
    value: &Value,
    predicate: &impl Fn(&serde_json::Map<String, Value>) -> bool,
) -> bool {
    match value {
        Value::Object(object) => {
            predicate(object)
                || object
                    .values()
                    .any(|value| contains_object(value, predicate))
        }
        Value::Array(values) => values.iter().any(|value| contains_object(value, predicate)),
        _ => false,
    }
}
fn exact_receipt(receipt: &RequestReceipt) -> (IntentionId, EntryId) {
    let RequestReceipt::Admitted {
        intention,
        entry_id,
        ..
    } = receipt
    else {
        panic!("ordinary admitted action")
    };
    (intention.clone(), entry_id.clone())
}

#[test]
fn encoded_selected_weapon_mixed_cost_pending_completed_reopen_and_receipts_are_exact() {
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("weapon.db");
        let policy = crate::SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let mut wire =
            Wire::connect(Engine::open_with_policy(&path, scenario(), policy.clone()).unwrap());
        let observation = wire.service.engine.observation(SOURCE).unwrap();
        let item = observation
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
            .find(|offer| offer.item == item)
            .unwrap()
            .equipped_slot
            .unwrap();
        assert_eq!(socket, 10);
        let target = observation
            .visible_actors
            .iter()
            .find(|actor| actor.id != observation.self_target)
            .unwrap()
            .id;
        let action = Action::UseAbility {
            id: "basic_melee".into(),
            invocation: AbilityInvocation {
                aim: AbilityAim::ActorTarget { target },
                weapon: WeaponSelection::Equipped { item, socket },
            },
            supplies: SupplySelection::Carried,
        };
        let backend = wire.service.engine.decode_action(SOURCE, &action).unwrap();
        let crate::actions::Action::UseAbility { invocation, .. } = &backend else {
            unreachable!()
        };
        let crate::actions::WeaponSelection::Equipped {
            item: internal_item,
            socket: internal_socket,
        } = invocation.weapon
        else {
            unreachable!()
        };
        assert_eq!(internal_socket, socket);
        assert_eq!(wire.service.engine.encode_action(SOURCE, &backend), action);
        let request = wire.request(SOURCE, action.clone());
        let first = ack(wire.send(SOURCE, "selected", request.clone()));
        let identity = exact_receipt(&first);
        let queued = saved(&wire.service);
        assert_eq!(ack(wire.send(SOURCE, "selected", request.clone())), first);
        assert_eq!(saved(&wire.service), queued);
        let hp_before = hp(&wire.service, TARGET);
        wire.service.run_until_blocked();
        for actor in ACTORS {
            wire.drain(actor);
        }
        let started = ack(wire.send(SOURCE, "selected", request.clone()));
        assert!(matches!(
            started,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ));
        assert_eq!(exact_receipt(&started), identity);
        assert_eq!(
            (
                resource(&wire.service, "mana").balance,
                resource(&wire.service, "mana").reserved
            ),
            (19, 2)
        );
        assert_eq!(
            (
                resource(&wire.service, "focus").balance,
                resource(&wire.service, "focus").reserved
            ),
            (20, 2)
        );
        let pending = saved(&wire.service);
        assert!(
            contains_object(&pending, &|object| object.get("weapon").is_some_and(
                |weapon| weapon["kind"] == "equipped"
                    && weapon["item"] == internal_item
                    && weapon["socket"] == socket
                    && weapon["secondary"] == true
            )),
            "frozen source is the exact secondary item/socket"
        );
        assert_eq!(ack(wire.send(SOURCE, "selected", request.clone())), started);
        assert_eq!(saved(&wire.service), pending);
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
            "restored Engine preserves exact frozen state before session startup"
        );
        let service = Service::new(engine);
        let paused_state = saved(&service);
        assert_eq!(
            paused_state["game"]["rules_rng"],
            pending["game"]["rules_rng"]
        );
        assert_eq!(paused_state["game"]["tick"], pending["game"]["tick"]);
        let previous = live_actor(&pending, SOURCE);
        let paused_actor = live_actor(&paused_state, SOURCE);
        assert_eq!(previous["pending"]["active"], true);
        assert_eq!(
            paused_actor["pending"]["active"], false,
            "Service startup deliberately pauses human preparation"
        );
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
                "startup preserves frozen pending {field}"
            );
        }
        for field in ["combat", "equipment", "location"] {
            assert_eq!(
                paused_actor[field], previous[field],
                "startup preserves {field}, including resource balances and holds"
            );
        }
        let mut wire = Wire::connect_service(service);
        assert_eq!(
            saved(&wire.service),
            paused_state,
            "connecting owners does not implicitly resume work"
        );
        let paused = ack(wire.send(SOURCE, "selected", request.clone()));
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
        ack(wire.send(SOURCE, "resume-selected", resume));
        let mut completed = None;
        for ordinal in 0..12 {
            wire.service.run_until_blocked();
            for actor in ACTORS {
                wire.drain(actor);
            }
            let receipt = ack(wire.send(SOURCE, "selected", request.clone()));
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
        let completed = completed.expect("ordinary scheduler resolves selected attack");
        assert_eq!(
            hp(&wire.service, TARGET),
            hp_before - 7,
            "exactly one secondary strike, no primary or extra attack"
        );
        assert_eq!(
            (
                resource(&wire.service, "mana").balance,
                resource(&wire.service, "mana").reserved
            ),
            (17, 0)
        );
        assert_eq!(
            (
                resource(&wire.service, "focus").balance,
                resource(&wire.service, "focus").reserved
            ),
            (18, 0)
        );
        let history = wire
            .service
            .engine
            .history(SOURCE, "weapon-1", None, 100)
            .unwrap();
        assert_eq!(history.entries.iter().filter(|entry| matches!(&entry.content, HistoryContent::Action { action: found, .. } if found == &action)).count(), 2, "history retains the exact initial start and explicit continuation; the HP and resource assertions prove one strike and one settlement");
        let settled = saved(&wire.service);
        assert_eq!(
            ack(wire.send(SOURCE, "selected", request.clone())),
            completed
        );
        assert_eq!(saved(&wire.service), settled);
        wire.service.engine.flush().unwrap();
        drop(wire);
        let engine = Engine::open_with_policy(&path, crate::Scenario::two_room(0), policy).unwrap();
        if interval == 4096 {
            assert!(engine.recovery_profile().records_replayed > 0);
        }
        let mut wire = Wire::connect(engine);
        assert_eq!(
            saved(&wire.service),
            settled,
            "completed replay preserves exact source, settlement and RNG"
        );
        assert_eq!(
            ack(wire.send(SOURCE, "selected", request.clone())),
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
            unreachable!()
        };
        invocation.weapon = WeaponSelection::Default;
        assert!(
            wire.send(SOURCE, "selected", altered)
                .iter()
                .any(|message| matches!(message, ServerMessage::Error { .. })),
            "same request ID cannot silently change its weapon"
        );
        assert_eq!(saved(&wire.service), settled);
    }
}

fn preview_request(wire: &mut Wire, action: &Action) -> Request {
    let snapshot = wire
        .send(SOURCE, "preview-snapshot", Request::Snapshot)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::Snapshot { snapshot, .. } => Some(snapshot),
            _ => None,
        })
        .unwrap();
    let Action::UseAbility {
        id,
        invocation,
        supplies,
    } = action
    else {
        panic!("ability")
    };
    Request::Command {
        context: snapshot.reply_context().input,
        branch: snapshot.branch.clone(),
        command: Command::PreviewAbilityWeapon {
            expected_revision: snapshot.state.revision,
            ability: id.clone(),
            invocation: *invocation,
            supplies: supplies.clone(),
        },
    }
}
fn exact_preview(wire: &mut Wire, action: &Action) -> AbilityWeaponPreview {
    let request = preview_request(wire, action);
    let before = saved(&wire.service);
    let messages = wire.send(SOURCE, "preview", request);
    assert_eq!(
        saved(&wire.service),
        before,
        "preview changes no RNG, clock, resources, holds, intentions, history or receipts"
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Error { .. })),
        "{messages:?}"
    );
    let (context, preview) = messages
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::AbilityWeaponPreview {
                context,
                request_id,
                preview,
            } => {
                assert_eq!(request_id, "preview");
                Some((context, *preview))
            }
            _ => None,
        })
        .expect("encoded preview");
    assert_eq!(context.actor, SOURCE);
    let Action::UseAbility {
        id,
        invocation,
        supplies,
    } = action
    else {
        unreachable!()
    };
    assert_eq!(
        (&preview.ability, preview.invocation, &preview.supplies),
        (id, *invocation, supplies)
    );
    preview
}
#[test]
fn encoded_exact_weapon_preview_matches_capture_and_reservations_and_refusals_do_not_mutate() {
    let mut wire = Wire::connect(Engine::memory(scenario()).unwrap());
    let observation = wire.service.engine.observation(SOURCE).unwrap();
    let item = observation
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
        .find(|offer| offer.item == item)
        .unwrap()
        .equipped_slot
        .unwrap();
    let target = observation
        .visible_actors
        .iter()
        .find(|actor| actor.id != observation.self_target)
        .unwrap()
        .id;
    let action = Action::UseAbility {
        id: "basic_melee".into(),
        invocation: AbilityInvocation {
            aim: AbilityAim::ActorTarget { target },
            weapon: WeaponSelection::Equipped { item, socket },
        },
        supplies: SupplySelection::Carried,
    };
    let fresh = exact_preview(&mut wire, &action);
    assert_eq!(fresh.phase, AbilityWeaponPreviewPhase::Fresh);
    assert_eq!((fresh.preparation, fresh.recovery), (300, 100));
    let cost = fresh.cost.as_ref().unwrap();
    assert!(cost.stamina.is_none());
    assert_eq!(
        (
            cost.mana.as_ref().unwrap().start,
            cost.mana.as_ref().unwrap().resolution
        ),
        (1, 2)
    );
    assert_eq!(
        (
            cost.focus.as_ref().unwrap().start,
            cost.focus.as_ref().unwrap().resolution
        ),
        (0, 2)
    );
    let backend = wire.service.engine.decode_action(SOURCE, &action).unwrap();
    let crate::actions::Action::UseAbility {
        invocation:
            crate::actions::AbilityInvocation {
                weapon:
                    crate::actions::WeaponSelection::Equipped {
                        item: internal_item,
                        ..
                    },
                ..
            },
        ..
    } = backend
    else {
        unreachable!()
    };
    let foreign_item = wire
        .service
        .engine
        .target_scope(TARGET)
        .item(tor_simulation::ItemId(internal_item));
    // Read-only requests have ordinary actor/branch/revision/item/target authority.
    for bad in 0..5 {
        let mut request = preview_request(&mut wire, &action);
        let Request::Command {
            branch,
            command:
                Command::PreviewAbilityWeapon {
                    expected_revision,
                    invocation,
                    ..
                },
            ..
        } = &mut request
        else {
            unreachable!()
        };
        match bad {
            0 => *expected_revision = expected_revision.saturating_add(1),
            1 => *branch = BranchId("not-current".into()),
            2 => {
                invocation.weapon = WeaponSelection::Equipped {
                    item: foreign_item,
                    socket,
                }
            }
            3 => invocation.weapon = WeaponSelection::Equipped { item, socket: 0 },
            4 => {
                invocation.aim = AbilityAim::ActorTarget {
                    target: ActorTarget::from_digest([77; 32]),
                }
            }
            _ => unreachable!(),
        }
        let before = saved(&wire.service);
        let messages = wire.send(SOURCE, &format!("invalid-preview-{bad}"), request);
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, ServerMessage::Error { .. })),
            "{messages:?}"
        );
        if bad == 0 {
            // A future display generation is not an issued picture capability.
            assert!(messages.iter().any(|message| matches!(
                message,
                ServerMessage::Error {
                    code: ErrorCode::StaleContext,
                    ..
                }
            )));
        }
        assert!(!messages
            .iter()
            .any(|message| matches!(message, ServerMessage::AbilityWeaponPreview { .. })));
        assert_eq!(
            saved(&wire.service),
            before,
            "failed preview {bad} is read-only"
        );
    }
    let stale = preview_request(&mut wire, &action);
    let request = wire.request(SOURCE, action.clone());
    ack(wire.send(SOURCE, "use-previewed", request));
    wire.service.run_until_blocked();
    for actor in ACTORS {
        wire.drain(actor);
    }
    let pending = saved(&wire.service);
    let captured: tor_simulation::abilities::Plan =
        serde_json::from_value(live_actor(&pending, SOURCE)["pending"]["plan"].clone()).unwrap();
    let tor_simulation::abilities::Plan::Ability(captured) = captured else {
        panic!("captured ability")
    };
    assert_eq!(captured.preparation, fresh.preparation);
    assert_eq!(captured.recovery, fresh.recovery);
    assert_eq!(
        captured.cost.map(crate::creature_view::cost_bundle),
        fresh.cost
    );
    for charge in cost.entries() {
        let paid = resource(&wire.service, &charge.resource.id);
        assert_eq!(paid.balance, 20 - charge.start);
        assert_eq!(paid.reserved, charge.resolution);
    }
    let messages = wire.send(SOURCE, "stale-preview", stale);
    assert!(
        messages.iter().any(|message| matches!(
            message,
            ServerMessage::Error {
                code: ErrorCode::StaleRevision | ErrorCode::StaleContext,
                ..
            }
        )),
        "{messages:?}"
    );
    assert_eq!(saved(&wire.service), pending);
    wire.send(SOURCE, "pause-preview", Request::ReleaseControl);
    wire.send(SOURCE, "reacquire-preview", Request::AcquireControl);
    let resumed = exact_preview(&mut wire, &action);
    assert_eq!(resumed.phase, AbilityWeaponPreviewPhase::Resuming);
    assert_eq!(resumed.cost, fresh.cost);
    assert_eq!(
        (resumed.preparation, resumed.recovery),
        (fresh.preparation, fresh.recovery)
    );
    let request = preview_request(&mut wire, &action);
    wire.send(SOURCE, "release-preview-authority", Request::ReleaseControl);
    // Use the new context after control loss so the controller check is exercised.
    let Request::Command { command, .. } = request else {
        unreachable!()
    };
    let snapshot = wire
        .send(SOURCE, "lost-control-snapshot", Request::Snapshot)
        .into_iter()
        .find_map(|message| match message {
            ServerMessage::Snapshot { snapshot, .. } => Some(snapshot),
            _ => None,
        })
        .unwrap();
    let request = Request::Command {
        context: snapshot.reply_context().input,
        branch: snapshot.branch.clone(),
        command,
    };
    let before = saved(&wire.service);
    let messages = wire.send(SOURCE, "no-control-preview", request);
    assert!(
        messages.iter().any(|message| matches!(
            message,
            ServerMessage::Error {
                code: ErrorCode::NotController,
                ..
            }
        )),
        "{messages:?}"
    );
    assert_eq!(saved(&wire.service), before);
}

fn carried_secondary_scenario() -> crate::Scenario {
    let mut scenario = scenario();
    let package = scenario.package.take().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    let secondary = manifest["characters"][0]["loadout"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|item| item["archetype"] == "fixture.secondary")
        .unwrap();
    secondary["equipped_slot"] = Value::Null;
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
fn encoded_frozen_equip_allocation_pending_completed_source_free_reopen_is_exact() {
    use tor_simulation::abilities::{CapturedWeaponProfile, Plan};
    use tor_simulation::{EquipmentSlotId, ItemId};

    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("frozen-equip.db");
        let policy = crate::SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let mut wire = Wire::connect(
            Engine::open_with_policy(&path, carried_secondary_scenario(), policy.clone()).unwrap(),
        );
        let observation = wire.service.engine.observation(SOURCE).unwrap();
        let item = observation
            .inventory
            .iter()
            .find(|item| item.name == "secondary sword")
            .unwrap()
            .id;
        let primary = observation
            .inventory
            .iter()
            .find(|item| item.name == "primary sword")
            .unwrap()
            .id;
        let interactions = observation.interactions.as_ref().unwrap();
        assert_eq!(
            interactions
                .inventory
                .iter()
                .find(|offer| offer.item == item)
                .unwrap()
                .equipped_slot,
            None
        );
        assert_eq!(
            interactions
                .inventory
                .iter()
                .find(|offer| offer.item == primary)
                .unwrap()
                .equipped_slot,
            Some(0)
        );
        assert!(interactions.slots[10].requires_attack);
        let action = Action::Equip { item, slot: 10 };
        let backend = wire.service.engine.decode_action(SOURCE, &action).unwrap();
        let crate::actions::Action::Equip {
            item: internal_item,
            slot: 10,
        } = backend
        else {
            panic!("exact equip adapter")
        };
        let request = wire.request(SOURCE, action.clone());
        let queued_receipt = ack(wire.send(SOURCE, "draw-secondary", request.clone()));
        let identity = exact_receipt(&queued_receipt);
        let queued = saved(&wire.service);
        assert_eq!(
            ack(wire.send(SOURCE, "draw-secondary", request.clone())),
            queued_receipt
        );
        assert_eq!(saved(&wire.service), queued, "queued retry is a no-op");
        wire.service.run_until_blocked();
        for actor in ACTORS {
            wire.drain(actor);
        }
        let started = ack(wire.send(SOURCE, "draw-secondary", request.clone()));
        assert_eq!(exact_receipt(&started), identity);
        assert!(matches!(
            started,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ));
        let pending = saved(&wire.service);
        let previous = live_actor(&pending, SOURCE);
        let frozen: Plan = serde_json::from_value(previous["pending"]["plan"].clone()).unwrap();
        let Plan::Equip {
            actor,
            item: captured_item,
            slot,
            allocation,
            preparation,
        } = &frozen
        else {
            panic!("pending Equip carries its required allocation")
        };
        assert_eq!(*actor, tor_simulation::ActorId(SOURCE.0));
        assert_eq!(*captured_item, ItemId(internal_item));
        assert_eq!(*slot, EquipmentSlotId(10));
        assert!(
            *preparation > 0,
            "ordinary draw must reach a pending save boundary"
        );
        assert_eq!(previous["pending"]["remaining"], *preparation);
        assert_eq!(previous["pending"]["active"], true);
        assert_eq!(allocation.target, CapturedWeaponProfile::Ordinary);
        assert_eq!(
            allocation.occupants.len(),
            1,
            "the complete pool includes the existing primary"
        );
        let occupant = &allocation.occupants[0];
        assert_eq!(occupant.socket, EquipmentSlotId(0));
        assert_eq!(occupant.profile, CapturedWeaponProfile::Ordinary);
        assert_eq!(
            wire.service.engine.target_scope(SOURCE).item(occupant.item),
            primary
        );
        let primary_item = occupant.item;
        let equipment: BTreeMap<EquipmentSlotId, ItemId> =
            serde_json::from_value(previous["equipment"].clone()).unwrap();
        assert_eq!(equipment, [(EquipmentSlotId(0), primary_item)].into());
        assert_eq!(
            ack(wire.send(SOURCE, "draw-secondary", request.clone())),
            started
        );
        assert_eq!(
            saved(&wire.service),
            pending,
            "started retry preserves the frozen allocation"
        );
        wire.service.engine.flush().unwrap();
        drop(wire);

        let engine =
            Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy.clone())
                .unwrap();
        if interval == 4096 {
            assert!(
                engine.recovery_profile().records_replayed > 0,
                "pending allocation must traverse actual journal replay"
            );
        }
        assert_eq!(
            saved_engine(&engine),
            pending,
            "source-free restore preserves the complete pending Engine before session startup"
        );
        let service = Service::new(engine);
        let paused_state = saved(&service);
        let paused_actor = live_actor(&paused_state, SOURCE);
        assert_eq!(
            paused_actor["pending"]["active"], false,
            "ordinary reconnect deliberately suspends human preparation"
        );
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
                "startup retains frozen Equip {field}"
            );
        }
        for field in ["combat", "equipment", "location"] {
            assert_eq!(
                paused_actor[field], previous[field],
                "startup preserves {field}"
            );
        }
        for field in ["rules_rng", "tick"] {
            assert_eq!(
                paused_state["game"][field], pending["game"][field],
                "startup preserves {field}"
            );
        }
        let restored: Plan =
            serde_json::from_value(paused_actor["pending"]["plan"].clone()).unwrap();
        assert_eq!(restored, frozen);
        let mut wire = Wire::connect_service(service);
        assert_eq!(
            saved(&wire.service),
            paused_state,
            "owner connection never auto-resumes Equip"
        );
        let paused_receipt = ack(wire.send(SOURCE, "draw-secondary", request.clone()));
        assert_eq!(exact_receipt(&paused_receipt), identity);
        assert!(matches!(
            paused_receipt,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Paused,
                ..
            }
        ));
        assert_eq!(saved(&wire.service), paused_state);
        let snapshot = wire
            .send(SOURCE, "equip-resume-snapshot", Request::Snapshot)
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
        ack(wire.send(SOURCE, "resume-draw-secondary", resume));
        let mut completed = None;
        for ordinal in 0..32 {
            wire.service.run_until_blocked();
            for actor in ACTORS {
                wire.drain(actor);
            }
            let receipt = ack(wire.send(SOURCE, "draw-secondary", request.clone()));
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
            ack(wire.send(actor, &format!("equip-clock-{ordinal}"), wait));
        }
        let completed = completed.expect("ordinary scheduler resolves the original frozen Equip");
        let settled = saved(&wire.service);
        let equipment: BTreeMap<EquipmentSlotId, ItemId> =
            serde_json::from_value(live_actor(&settled, SOURCE)["equipment"].clone()).unwrap();
        assert_eq!(
            equipment,
            [
                (EquipmentSlotId(0), primary_item),
                (EquipmentSlotId(10), ItemId(internal_item))
            ]
            .into(),
            "only the captured secondary socket changes; primary is never displaced"
        );
        let observation = wire.service.engine.observation(SOURCE).unwrap();
        let interactions = observation.interactions.as_ref().unwrap();
        assert_eq!(
            interactions
                .inventory
                .iter()
                .find(|offer| offer.item == item)
                .unwrap()
                .equipped_slot,
            Some(10)
        );
        assert_eq!(
            interactions
                .inventory
                .iter()
                .find(|offer| offer.item == primary)
                .unwrap()
                .equipped_slot,
            Some(0)
        );
        assert_eq!(
            ack(wire.send(SOURCE, "draw-secondary", request.clone())),
            completed
        );
        assert_eq!(
            saved(&wire.service),
            settled,
            "completed retry changes no equipment, time, RNG or receipt"
        );
        wire.service.engine.flush().unwrap();
        drop(wire);

        let engine =
            Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy).unwrap();
        if interval == 4096 {
            assert!(
                engine.recovery_profile().records_replayed > 0,
                "completed allocation must traverse actual journal replay"
            );
        }
        assert_eq!(
            saved_engine(&engine),
            settled,
            "source-free completed restore retains exact socket mapping and receipt"
        );
        let mut wire = Wire::connect(engine);
        assert_eq!(saved(&wire.service), settled);
        assert_eq!(ack(wire.send(SOURCE, "draw-secondary", request)), completed);
        assert_eq!(
            saved(&wire.service),
            settled,
            "completed reopened retry is a no-op"
        );
    }
}

#[path = "projectile_session_tests.rs"]
mod projectile_tests;
