//! Encoded production Service ownership/retry and Engine publication contracts.
//! The ordinary magic-items package owns every capability and initial item.
use super::*;
use crate::scenario_package::{resolve_rules_source, Package};
use serde_json::{json, Value};
use std::path::Path;

const OWNER: ActorId = ActorId(1);
const OTHER: ActorId = ActorId(2);

fn scenario(with_other: bool) -> crate::Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    let mut scenario = crate::scenario_package::load(&root, 42, None, true).unwrap();
    if with_other {
        let source = scenario.package.as_ref().unwrap();
        let mut regions = source.region_defs().unwrap();
        let mut region = serde_json::to_value(&regions[0]).unwrap();
        // Only add a second ordinary external owner. Item definitions and stock
        // remain exactly those in the repository acceptance package.
        region["actors"] = json!([{
            "id":2,"at":[5,2,0],"turn_ticks":100,"controller":"external",
            "creature":source.manifest.characters[0].creature
        }]);
        regions[0] = serde_json::from_value(region).unwrap();
        let rules = std::fs::read_to_string(root.join("rules.toml")).unwrap();
        let rules = resolve_rules_source(&rules, "rules.toml").unwrap().source;
        scenario.package = Some(Arc::new(
            Package::from_parts(source.manifest.clone(), regions, &rules).unwrap(),
        ));
    }
    scenario
}

struct Wire {
    service: Service,
    clients: BTreeMap<ActorId, Connection>,
    serial: u64,
}

impl Wire {
    fn new(engine: Engine) -> Self {
        let actors = engine.actors();
        let mut wire = Self {
            service: Service::new(engine),
            clients: BTreeMap::new(),
            serial: 0,
        };
        for actor in actors {
            let connection = wire
                .service
                .connect(
                    &Account {
                        user: format!("magic-owner-{}", actor.0),
                        token: "fixture".into(),
                        role: AccessRole::Player,
                        actors: BTreeSet::from([actor]),
                    },
                    "encoded-magic-items".into(),
                )
                .unwrap();
            wire.clients.insert(actor, connection);
            wire.send(actor, "attach", Request::Attach { actor });
            ack(wire.send(actor, "control", Request::AcquireControl));
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
            panic!("encoded gameplay request expected")
        };
        self.service
            .handle(self.clients[&actor].id, request_id, *request);
        self.drain(actor)
    }

    fn snapshot(&mut self, actor: ActorId) -> Box<Snapshot> {
        self.send(actor, "snapshot", Request::Snapshot)
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::Snapshot { snapshot, .. } => Some(snapshot),
                ServerMessage::Error { .. } => panic!("{message:?}"),
                _ => None,
            })
            .unwrap()
    }

    fn request(&mut self, actor: ActorId, action: Action) -> Request {
        let snapshot = self.snapshot(actor);
        assert!(snapshot.has_control && snapshot.readiness.admission);
        Request::Command {
            context: snapshot.reply_context().input,
            branch: snapshot.branch.clone(),
            command: Command::Act {
                expected_revision: snapshot.state.revision,
                action,
            },
        }
    }

    fn finish(&mut self, actor: ActorId, id: &str, request: &Request) -> RequestReceipt {
        for _ in 0..64 {
            self.service.run_until_blocked();
            for other in self.clients.keys().copied().collect::<Vec<_>>() {
                assert!(!self
                    .drain(other)
                    .iter()
                    .any(|message| matches!(message, ServerMessage::Error { .. })));
            }
            let receipt = ack(self.send(actor, id, request.clone()));
            if matches!(
                receipt,
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Resolved,
                    ..
                }
            ) {
                return receipt;
            }
            let next = self.service.engine.next_actor().unwrap();
            self.serial += 1;
            let request = self.request(next, Action::Wait);
            ack(self.send(next, &format!("clock-{}", self.serial), request));
        }
        panic!("ordinary magic-item work did not resolve")
    }

    fn act(&mut self, id: &str, action: Action) -> RequestReceipt {
        let request = self.request(OWNER, action);
        ack(self.send(OWNER, id, request.clone()));
        self.finish(OWNER, id, &request)
    }

    fn take(&mut self, class: ItemClass) -> ItemTarget {
        let item = self
            .snapshot(OWNER)
            .state
            .observation
            .ground_items
            .iter()
            .find(|row| row.item.class == class)
            .unwrap()
            .item
            .id;
        self.act(
            &format!("take-{class:?}"),
            Action::Take {
                item,
                quantity: None,
            },
        );
        item
    }

    fn checkpoint(&self) -> Value {
        checkpoint(&self.service.engine)
    }
}

fn checkpoint(engine: &Engine) -> Value {
    serde_json::to_value(crate::engine::Checkpoint::capture(engine).encode("magic-items", 0))
        .unwrap()
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
        .unwrap()
}

fn rejected(messages: Vec<ServerMessage>) {
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Ack { .. })),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|message| matches!(
            message,
            ServerMessage::Error {
                code: ErrorCode::InvalidAction,
                ..
            }
        )),
        "{messages:?}"
    );
}

fn choice(snapshot: &Snapshot) -> IdentificationChoiceView {
    snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .identification
        .clone()
        .unwrap()
}

fn quantity(snapshot: &Snapshot, item: ItemTarget) -> u64 {
    snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.id == item)
        .unwrap()
        .quantity
}

#[test]
fn encoded_paid_identify_requires_exact_owner_category_and_carried_target_then_retries() {
    let mut wire = Wire::new(Engine::memory(scenario(true)).unwrap());
    let scroll = wire.take(ItemClass::Scroll);
    let wand = wire.take(ItemClass::Wand);
    let potion = wire.take(ItemClass::Potion);
    let ground_wand = wire
        .snapshot(OWNER)
        .state
        .observation
        .ground_items
        .iter()
        .find(|row| row.item.class == ItemClass::Wand)
        .unwrap()
        .item
        .id;
    wire.act("read", Action::Read { item: scroll });
    let paid = wire.snapshot(OWNER);
    let pending = choice(&paid);
    assert_eq!(quantity(&paid, scroll), 2);
    assert_eq!(pending.classes, vec![ItemClass::Wand]);
    assert_eq!(pending.items, vec![wand]);
    assert!(!pending.choice.0.is_empty());
    assert!(wire
        .snapshot(OTHER)
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .identification
        .is_none());

    // The exact disclosed token is necessary, as are current carried ownership
    // and the category paid for by the source scroll.
    for (id, actor, token, item) in [
        ("wrong-owner", OTHER, pending.choice.clone(), wand),
        ("wrong-category", OWNER, pending.choice.clone(), potion),
        ("not-carried", OWNER, pending.choice.clone(), ground_wand),
        (
            "wrong-choice",
            OWNER,
            IntentionId(format!("{}-stale", pending.choice.0)),
            wand,
        ),
    ] {
        let before = wire.checkpoint();
        let request = wire.request(
            actor,
            Action::Identify {
                choice: token,
                item: Some(item),
            },
        );
        rejected(wire.send(actor, id, request));
        assert_eq!(
            wire.checkpoint(),
            before,
            "{id} changed authoritative state"
        );
    }

    let request = wire.request(
        OWNER,
        Action::Identify {
            choice: pending.choice.clone(),
            item: Some(wand),
        },
    );
    let admitted = ack(wire.send(OWNER, "identify", request.clone()));
    let resolved = wire.finish(OWNER, "identify", &request);
    assert_eq!(resolved.entry_id(), admitted.entry_id());
    let identified = wire.snapshot(OWNER);
    assert!(identified
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .identification
        .is_none());
    assert_eq!(quantity(&identified, scroll), 2);
    assert!(
        identified
            .state
            .observation
            .inventory
            .iter()
            .find(|row| row.id == wand)
            .unwrap()
            .identified
    );
    assert!(
        !identified
            .state
            .observation
            .inventory
            .iter()
            .find(|row| row.id == potion)
            .unwrap()
            .identified
    );
    let before = wire.checkpoint();
    // Reuse every request byte, including its now-stale context and vanished
    // choice. Receipt lookup must precede fresh target/choice resolution.
    assert_eq!(ack(wire.send(OWNER, "identify", request)), resolved);
    assert_eq!(wire.checkpoint(), before, "retry paid or learned twice");
    let stale = wire.request(
        OWNER,
        Action::Identify {
            choice: pending.choice,
            item: Some(wand),
        },
    );
    rejected(wire.send(OWNER, "new-request-old-choice", stale));
    assert_eq!(wire.checkpoint(), before);
}

#[test]
fn late_store_rejection_rolls_back_magic_charge_and_paid_selection_then_retries() {
    // These are the three new mutable publication facts, not a scenario matrix:
    // charge spend, creation of a paid choice, and consumption of that choice.
    for operation in ["zap", "read", "identify"] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("magic-items.db");
        let fixture = scenario(false);
        let mut wire = Wire::new(Engine::open(&path, fixture.clone()).unwrap());
        let scroll = wire.take(ItemClass::Scroll);
        let wand = wire.take(ItemClass::Wand);
        let action = match operation {
            "zap" => {
                wire.act("read", Action::Read { item: scroll });
                let pending = choice(&wire.snapshot(OWNER));
                wire.act(
                    "learn-charges",
                    Action::Identify {
                        choice: pending.choice,
                        item: Some(wand),
                    },
                );
                Action::Zap {
                    item: wand,
                    aim: tor_protocol::DeviceAim::Direction {
                        direction: Direction::East,
                    },
                }
            }
            "read" => Action::Read { item: scroll },
            "identify" => {
                wire.act("read", Action::Read { item: scroll });
                Action::Identify {
                    choice: choice(&wire.snapshot(OWNER)).choice,
                    item: Some(wand),
                }
            }
            _ => unreachable!(),
        };
        let request = wire.request(OWNER, action);
        let admitted = ack(wire.send(OWNER, "publish", request.clone()));
        wire.service.engine.flush().unwrap();
        drop(wire);
        let saved = crate::inspect_save(&path).unwrap();
        let refusing = crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        };
        // Exercise Engine publication directly after reopening: Service startup
        // deliberately suspends pending human work and is a different contract.
        let mut engine = Engine::open_with_policy(&path, fixture.clone(), refusing).unwrap();
        let before = checkpoint(&engine);
        let sequence = engine.save_status().accepted_sequence;
        for _ in 0..2 {
            // This executes a private candidate before Store refuses publication,
            // unlike an admission/preflight rejection of an invalid action.
            let error = engine.execute_next_intention().unwrap_err();
            assert_eq!(error.code, ErrorCode::StorageFailure, "{operation}");
            assert_eq!(
                error.message,
                "Save queue is full; wait for saving to finish before retrying"
            );
            assert_eq!(checkpoint(&engine), before, "{operation}");
            assert_eq!(engine.save_status().accepted_sequence, sequence);
            assert_eq!(crate::inspect_save(&path).unwrap(), saved);
        }
        drop(engine);
        let mut engine = Engine::open(&path, fixture).unwrap();
        // One successful execution of the same saved candidate demonstrates that
        // the failed publication above reached the specific mutable fact.
        engine.execute_next_intention().unwrap().unwrap();
        let mut wire = Wire::new(engine);
        let after = wire.snapshot(OWNER);
        let interaction = after.state.observation.interactions.as_ref().unwrap();
        match operation {
            "zap" => {
                assert_eq!(
                    interaction
                        .inventory
                        .iter()
                        .find(|row| row.item == wand)
                        .unwrap()
                        .charges,
                    Some(2)
                );
                assert!(interaction.identification.is_none());
                assert_eq!(quantity(&after, scroll), 2);
            }
            "read" => {
                assert_eq!(choice(&after).items, vec![wand]);
                assert_eq!(quantity(&after, scroll), 2);
            }
            "identify" => {
                assert!(interaction.identification.is_none());
                assert!(
                    after
                        .state
                        .observation
                        .inventory
                        .iter()
                        .find(|row| row.id == wand)
                        .unwrap()
                        .identified
                );
                assert_eq!(quantity(&after, scroll), 2);
            }
            _ => unreachable!(),
        }
        let completed = wire.checkpoint();
        let receipt = ack(wire.send(OWNER, "publish", request));
        assert_eq!(receipt.entry_id(), admitted.entry_id());
        assert!(matches!(
            receipt,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Resolved,
                ..
            }
        ));
        assert_eq!(wire.checkpoint(), completed);
    }
}

fn equipment_scenario() -> crate::Scenario {
    let mut scenario = scenario(false);
    let source = scenario.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["id"] = json!("equipment-magic-service");
    manifest["archetypes"]["mail"] = json!({"name":"ring mail","identity":"fixture.mail","class":"armor","equipment":{"slot":"body_armor","armor":{"physical_reduction":2,"material":"metal","rustproof":false},"modifiers":[]},"equipment_generation":{"kind":"armor","curse_percent":100,"enchant_percent":0,"minimum":1,"maximum":1}});
    manifest["archetypes"]["armor_identify"] = json!({"name":"scroll of identify armor","identity":"fixture.identify_armor","class":"scroll","stackable":true,"appearance_pool":"armor_pages","readable":{"identify":["armor"],"discovery":"on_use"}});
    manifest["archetypes"]["armor_enchant"] = json!({"name":"scroll of enchant armor","identity":"fixture.enchant_armor","class":"scroll","stackable":true,"appearance_pool":"enchant_pages","readable":{"discovery":"never","effects":[{"kind":"alter_equipment","selection":{"kind":"slot","slot":"body_armor","class":"armor","last":false},"alteration":"enchant_armor"}]}});
    manifest["appearance_pools"]["armor_pages"] = json!({"appearances":["indigo page"]});
    manifest["appearance_pools"]["enchant_pages"] = json!({"appearances":["silver page"]});
    manifest["characters"][0]["known_identities"] = json!(["fixture.mail"]);
    manifest["characters"][0]["loadout"] = json!([
        {"archetype":"mail","quantity":1,"equipped_slot":1},
        {"archetype":"mail","quantity":1},
        {"archetype":"armor_identify","quantity":1},
        {"archetype":"armor_enchant","quantity":2}
    ]);
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["items"] = json!([]);
    regions[0] = serde_json::from_value(region).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    let rules = std::fs::read_to_string(root.join("rules.toml")).unwrap();
    let rules = resolve_rules_source(&rules, "rules.toml").unwrap().source;
    scenario.package = Some(Arc::new(
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap(),
    ));
    scenario
}

#[test]
fn equipment_only_action_broadcast_redacts_spectator_wire_and_retained_state() {
    let mut wire = Wire::new(Engine::memory(equipment_scenario()).unwrap());
    let initial = wire.snapshot(OWNER);
    let worn = initial
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .find(|row| row.equipped_slot == Some(1))
        .unwrap()
        .item;
    let mut spectator = wire
        .service
        .connect(
            &Account {
                user: "equipment-observer".into(),
                token: "fixture".into(),
                role: AccessRole::Spectator,
                actors: BTreeSet::from([OWNER]),
            },
            "encoded-equipment-observer".into(),
        )
        .unwrap();
    spectator.messages.try_recv_frame().unwrap();
    wire.service.handle(
        spectator.id,
        "attach".into(),
        Request::Attach { actor: OWNER },
    );
    let attached = decode_response(&spectator.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = attached else {
        panic!("snapshot expected")
    };
    let mut received = snapshot.state.as_ref().clone();
    while spectator.messages.try_recv_frame().is_ok() {}

    wire.act("learn-cursed-removal", Action::Unequip { item: worn });
    let owner = wire.service.engine.state(OWNER).unwrap();
    let owner_items = owner.observation.interactions.as_ref().unwrap();
    assert!(owner_items.identification.is_none());
    assert!(owner_items
        .inventory
        .iter()
        .all(|row| row.charges.is_none()));
    assert_eq!(
        owner_items
            .inventory
            .iter()
            .find(|row| row.item == worn)
            .unwrap()
            .known_cursed,
        Some(true)
    );
    assert!(owner_items.curse_blocked.contains(&worn));
    let mut expected = owner.clone();
    restrict_item_disclosure(&mut expected, AccessRole::Spectator);
    let mut observations = 0;
    while let Ok(frame) = spectator.messages.try_recv_frame() {
        if let ServerMessage::Update { update } = decode_response(&frame.text).unwrap() {
            match update.body {
                UpdateBody::Observation { state, .. } => received = state.as_ref().clone(),
                UpdateBody::ObservationDelta { state, .. } => {
                    received = state.apply(&received).unwrap()
                }
                _ => continue,
            }
            observations += 1;
            let items = received.observation.interactions.as_ref().unwrap();
            assert!(
                items.curse_blocked.is_empty(),
                "curse block leaked on the action wire"
            );
            assert!(
                items
                    .inventory
                    .iter()
                    .all(|row| row.known_equipment.is_none()
                        && row.known_cursed.is_none()
                        && row.known_protected.is_none()),
                "private equipment facts leaked on the action wire"
            );
        }
    }
    assert!(
        observations > 0,
        "the action must actually publish an observation"
    );
    assert_eq!(received, expected);
    assert_eq!(
        wire.service.clients[&spectator.id]
            .last_observation
            .as_ref()
            .unwrap()
            .state
            .as_ref(),
        &expected
    );
}

#[test]
fn encoded_equipment_curse_identify_enchant_retry_and_reopen_preserve_exact_facts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("equipment.db");
    let fixture = equipment_scenario();
    let mut wire = Wire::new(Engine::open(&path, fixture.clone()).unwrap());
    let initial = wire.snapshot(OWNER);
    let interactions = initial.state.observation.interactions.as_ref().unwrap();
    let worn = interactions
        .inventory
        .iter()
        .find(|r| r.equipped_slot == Some(1))
        .unwrap()
        .item;
    let spare = initial
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Armor && i.id != worn)
        .unwrap()
        .id;
    let identify = initial
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.name == "indigo page")
        .unwrap()
        .id;
    let enchant = initial
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.name == "silver page")
        .unwrap()
        .id;
    assert!(interactions
        .inventory
        .iter()
        .filter(|r| r.item == worn || r.item == spare)
        .all(|r| r.known_cursed.is_none()));
    wire.act("try-cursed-removal", Action::Unequip { item: worn });
    let failed = wire.snapshot(OWNER);
    let row = failed
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .find(|r| r.item == worn)
        .unwrap();
    assert_eq!(row.equipped_slot, Some(1));
    assert_eq!(row.known_cursed, Some(true));
    wire.act("identify-page", Action::Read { item: identify });
    let pending = choice(&wire.snapshot(OWNER));
    wire.act(
        "identify-worn",
        Action::Identify {
            choice: pending.choice,
            item: Some(worn),
        },
    );
    let identified = wire.snapshot(OWNER);
    let rows = &identified
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory;
    assert!(rows
        .iter()
        .find(|r| r.item == worn)
        .unwrap()
        .known_equipment
        .is_some());
    assert!(rows
        .iter()
        .find(|r| r.item == spare)
        .unwrap()
        .known_equipment
        .is_none());
    let request = wire.request(OWNER, Action::Read { item: enchant });
    ack(wire.send(OWNER, "enchant-once", request.clone()));
    let resolved = wire.finish(OWNER, "enchant-once", &request);
    let enchanted = wire.snapshot(OWNER);
    let row = enchanted
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .find(|r| r.item == worn)
        .unwrap();
    assert_eq!(row.known_cursed, Some(false));
    assert_ne!(
        row.known_equipment,
        rows.iter()
            .find(|r| r.item == worn)
            .unwrap()
            .known_equipment
    );
    assert_eq!(quantity(&enchanted, enchant), 1);
    assert!(
        !enchanted
            .state
            .observation
            .inventory
            .iter()
            .find(|r| r.id == enchant)
            .unwrap()
            .identified
    );
    let before = wire.checkpoint();
    assert_eq!(ack(wire.send(OWNER, "enchant-once", request)), resolved);
    assert_eq!(wire.checkpoint(), before);
    wire.act("remove-recovered-armor", Action::Unequip { item: worn });
    wire.service.engine.flush().unwrap();
    let expected = wire
        .snapshot(OWNER)
        .state
        .observation
        .interactions
        .clone()
        .unwrap()
        .inventory;
    drop(wire);
    // Reopen is driven by stored package/content; the supplied unrelated seed is unused.
    let mut reopened = Wire::new(Engine::open(&path, crate::Scenario::two_room(999)).unwrap());
    assert_eq!(
        reopened
            .snapshot(OWNER)
            .state
            .observation
            .interactions
            .clone()
            .unwrap()
            .inventory,
        expected
    );
}

fn control_scroll_scenario() -> crate::Scenario {
    let mut scenario = scenario(false);
    let source = scenario.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["id"] = json!("control-scroll-service");
    let modes = [
        (
            "confuse",
            json!({"kind":"arm_confusion","condition":"fixture.control","duration":2000}),
        ),
        (
            "hold",
            json!({"kind":"hold_nearby","condition":"fixture.control","duration":1000,"radius":2}),
        ),
        ("aggravate", json!({"kind":"aggravate"})),
        ("nothing", json!({"kind":"nothing"})),
    ];
    for (id, control) in modes {
        manifest["archetypes"][id] = json!({"name":id,"identity":format!("fixture.{id}"),"class":"scroll","stackable":true,"readable":{"control":control,"discovery":"never"}});
    }
    manifest["characters"][0]["loadout"] = json!([
        {"archetype":"confuse","quantity":2},
        {"archetype":"hold","quantity":2},
        {"archetype":"aggravate","quantity":2},
        {"archetype":"nothing","quantity":2}
    ]);
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["items"] = json!([]);
    regions[0] = serde_json::from_value(region).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    let mut rules = std::fs::read_to_string(root.join("rules.toml")).unwrap();
    rules.push_str(r#"
[[overrides]]
op = "add"
path = ["conditions", "fixture.control"]
value = { id = "fixture.control", family = "fixture.control", lifecycle = "timed", name = "Control", modifiers = [], stacking = { kind = "keep" }, source_scoped = false, descriptors = ["mind_affecting"], capacity = "reject", timing = { kind = "actor_active", on_expiry = { kind = "remove" } }, restrictions = [], preparation = "preserve" }
"#);
    let rules = resolve_rules_source(&rules, "rules.toml").unwrap().source;
    scenario.package = Some(Arc::new(
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap(),
    ));
    scenario
}

#[test]
fn encoded_control_reads_pay_once_and_restore_armed_charge_from_stored_package() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("control.db");
    let mut wire = Wire::new(Engine::open(&path, control_scroll_scenario()).unwrap());
    for name in ["hold", "aggravate", "nothing", "confuse"] {
        let initial = wire.snapshot(OWNER);
        let item = initial
            .state
            .observation
            .inventory
            .iter()
            .find(|item| item.name == name)
            .unwrap()
            .id;
        let request = wire.request(OWNER, Action::Read { item });
        let id = format!("read-{name}");
        let admitted = ack(wire.send(OWNER, &id, request.clone()));
        let resolved = wire.finish(OWNER, &id, &request);
        assert_eq!(admitted.entry_id(), resolved.entry_id());
        assert_eq!(quantity(&wire.snapshot(OWNER), item), 1);
        let before = wire.checkpoint();
        assert_eq!(ack(wire.send(OWNER, &id, request)), resolved);
        assert_eq!(
            wire.checkpoint(),
            before,
            "retry must not consume or arm twice"
        );
    }
    // Checkpoint comparison observes the durable armed charge as well as inventory;
    // this is not merely a display assertion or a source-path reload.
    wire.service.engine.flush().unwrap();
    let expected = wire.checkpoint();
    fn has_armed_charge(value: &Value) -> bool {
        match value {
            Value::Object(fields) => {
                fields
                    .get("control")
                    .and_then(|control| control.get("confusion"))
                    == Some(&json!({"condition":"fixture.control","duration":2000}))
                    || fields.values().any(has_armed_charge)
            }
            Value::Array(values) => values.iter().any(has_armed_charge),
            _ => false,
        }
    }
    assert!(has_armed_charge(&expected));
    drop(wire);
    let reopened = Wire::new(Engine::open(&path, crate::Scenario::two_room(999)).unwrap());
    assert_eq!(reopened.checkpoint(), expected);
}

#[test]
fn encoded_unknown_ray_self_aim_rejects_without_payment_and_direction_retries_once() {
    let mut wire = Wire::new(Engine::memory(scenario(false)).unwrap());
    let wand = wire.take(ItemClass::Wand);
    let snapshot = wire.snapshot(OWNER);
    let item = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.id == wand)
        .unwrap();
    assert!(!item.identified);
    let affordance = snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .find(|row| row.item == wand)
        .unwrap();
    assert!(affordance.zappable);
    assert_eq!(
        affordance.device_activation,
        Some(DeviceActivation::Direction)
    );
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    let before = wire.checkpoint();
    rejected(wire.send(OWNER, "unknown-self", request));
    assert_eq!(
        wire.checkpoint(),
        before,
        "rejection must not admit, spend, learn or roll"
    );
    // Exact balances require the ordinary paid identification path; merely
    // firing an unknown device must not manufacture charge knowledge.
    let scroll = wire.take(ItemClass::Scroll);
    wire.act("identify-scroll", Action::Read { item: scroll });
    let pending = choice(&wire.snapshot(OWNER));
    wire.act(
        "identify-ray",
        Action::Identify {
            choice: pending.choice,
            item: Some(wand),
        },
    );
    let known = wire.snapshot(OWNER);
    let row = known
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .find(|row| row.item == wand)
        .unwrap();
    assert_eq!(row.charges, Some(3));
    assert_eq!(row.device_activation, Some(DeviceActivation::Direction));
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    let before = wire.checkpoint();
    rejected(wire.send(OWNER, "known-ray-self", request));
    assert_eq!(wire.checkpoint(), before);
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::East,
            },
        },
    );
    let admitted = ack(wire.send(OWNER, "ordinary-ray", request.clone()));
    let resolved = wire.finish(OWNER, "ordinary-ray", &request);
    assert_eq!(resolved.entry_id(), admitted.entry_id());
    let snapshot = wire.snapshot(OWNER);
    let affordance = snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .find(|row| row.item == wand)
        .unwrap();
    assert_eq!(affordance.charges, Some(2));
    assert_eq!(
        affordance.device_activation,
        Some(DeviceActivation::Direction)
    );
    let after = wire.checkpoint();
    assert_eq!(ack(wire.send(OWNER, "ordinary-ray", request)), resolved);
    assert_eq!(
        wire.checkpoint(),
        after,
        "receipt retry spends no second charge"
    );
}

#[path = "self_device_session_tests.rs"]
mod self_devices;
