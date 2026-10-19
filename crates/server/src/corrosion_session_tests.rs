//! One encoded Service composition for production CO1 and ordinary armor ownership.
use super::*;
use crate::scenario_package::{resolve_rules_source, resolve_source, Package, RegionDef};
use serde_json::{json, Value};
use std::path::Path;

const OWNER: ActorId = ActorId(1);
const AQUATOR: ActorId = ActorId(2);

fn scenario() -> crate::Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let production = resolve_source(&root).unwrap();
    let rules = std::fs::read_to_string(root.join(&production.manifest.ruleset)).unwrap();
    let rules = resolve_rules_source(&rules, "rules.toml").unwrap().source;
    let mut manifest = serde_json::to_value(production.manifest).unwrap();
    let original = (
        manifest["archetypes"]["rogue.aquator"].clone(),
        manifest["creatures"]["species"]["rogue.aquator"].clone(),
    );
    manifest["id"] = json!("corrosion-service");
    manifest["characters"].as_array_mut().unwrap().truncate(1);
    let owner = &mut manifest["characters"][0];
    owner["anchor"] = json!("1/start");
    owner["known_identities"] = json!(["rogue.ring_mail"]);
    owner["loadout"] = json!([
        {"archetype":"rogue.ring_mail","quantity":1,"equipped_slot":2},
        {"archetype":"rogue.ring_mail","quantity":1}
    ]);
    owner["creature"]["nutrition"] = json!({"kind":"exempt"});
    owner["creature"]["future_growth"] = json!({"kind":"disabled"});
    for key in [
        "procedural_worlds",
        "generation_recipes",
        "generation_groups",
        "stair_pairs",
    ] {
        manifest[key] = json!({});
    }
    for key in ["floor_activation", "unique_retrieval", "objective"] {
        manifest[key] = Value::Null;
    }
    assert_eq!(
        original,
        (
            manifest["archetypes"]["rogue.aquator"].clone(),
            manifest["creatures"]["species"]["rogue.aquator"].clone()
        )
    );
    let region: RegionDef = serde_json::from_value(json!({
        "id":1,"name":"Corrosion laboratory","size":[7,5,2],"chamber":true,
        "anchors":{"start":[2,2,0]},"actors":[{"id":2,"at":[3,2,0],"archetype":"rogue.aquator","controller":"external"}]
    })).unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![region],
        &rules,
    )
    .unwrap();
    crate::Scenario {
        actors: vec![],
        regions: 1,
        package: Some(Arc::new(package)),
        ..crate::Scenario::two_room(42)
    }
}

struct Wire {
    service: Service,
    clients: BTreeMap<ActorId, Connection>,
    serial: usize,
}
impl Wire {
    fn new(engine: Engine) -> Self {
        let mut this = Self {
            service: Service::new(engine),
            clients: BTreeMap::new(),
            serial: 0,
        };
        for actor in [OWNER, AQUATOR] {
            let client = this
                .service
                .connect(
                    &Account {
                        user: format!("armor-{}", actor.0),
                        token: "fixture".into(),
                        role: AccessRole::Player,
                        actors: BTreeSet::from([actor]),
                    },
                    "encoded-corrosion".into(),
                )
                .unwrap();
            this.clients.insert(actor, client);
            this.send(actor, "attach", Request::Attach { actor });
            this.send(actor, "control", Request::AcquireControl);
        }
        this
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
            panic!()
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
        for _ in 0..24 {
            self.service.run_until_blocked();
            for actor in [OWNER, AQUATOR] {
                assert!(!self
                    .drain(actor)
                    .iter()
                    .any(|m| matches!(m, ServerMessage::Error { .. })));
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
            let wait = self.request(next, Action::Wait);
            let id = format!("clock-{}", self.serial);
            ack(self.send(next, &id, wait));
        }
        panic!("bounded ordinary corrosion preparation failed to resolve")
    }
    fn saved(&self) -> Value {
        serde_json::to_value(
            crate::engine::Checkpoint::capture(&self.service.engine).encode("corrosion", 0),
        )
        .unwrap()
    }
}
fn ack(messages: Vec<ServerMessage>) -> RequestReceipt {
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, ServerMessage::Error { .. })),
        "{messages:?}"
    );
    messages
        .into_iter()
        .find_map(|m| match m {
            ServerMessage::Ack { receipt, .. } => Some(receipt),
            _ => None,
        })
        .unwrap()
}
fn corrosion(saved: &Value) -> Vec<u64> {
    let pool = saved["game"]["items"].as_u64().unwrap() as usize;
    let mut values = saved["shared"]["items"][pool]
        .as_object()
        .unwrap()
        .values()
        .map(|item| item["condition"]["corrosion"].as_u64().unwrap())
        .collect::<Vec<_>>();
    values.sort();
    values
}
fn reduction_amounts(snapshot: &Snapshot) -> Vec<i64> {
    let mut amounts = snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .flat_map(|item| item.known_equipment.iter())
        .flat_map(|gear| &gear.modifiers)
        .filter(|modifier| modifier.id == "reduction")
        .flat_map(|modifier| &modifier.fields)
        .filter_map(|field| match (&*field.id, &field.value) {
            ("amount", DiagnosticValue::Signed { value }) => Some(*value),
            _ => None,
        })
        .collect::<Vec<_>>();
    amounts.sort();
    amounts
}

#[test]
fn encoded_co1_changes_one_item_once_with_retry_disclosure_and_source_free_reopen() {
    let directory =
        std::env::temp_dir().join(format!("tor-corrosion-service-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let save = directory.join("save.sqlite");
    let policy = crate::SavePolicy {
        checkpoint_interval: 4096,
        ..Default::default()
    };
    let mut wire = Wire::new(Engine::open_with_policy(&save, scenario(), policy.clone()).unwrap());
    let unknown = wire.snapshot(OWNER);
    let inventory = &unknown
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory;
    assert!(inventory.iter().all(|item| item.known_equipment.is_none()));
    assert!(inventory
        .iter()
        .all(|item| item.known_cursed.is_none() && item.known_protected.is_none()));
    let worn = inventory
        .iter()
        .find(|item| item.equipped_slot.is_some())
        .unwrap();
    let slot = worn.equipped_slot.unwrap();
    let worn = worn.item;
    let spare = inventory
        .iter()
        .find(|item| item.equipped_slot.is_none())
        .unwrap()
        .item;
    // Ordinary paid wearing observes each exact armor's numeric state without
    // granting type-wide or curse/protection knowledge.
    for (index, action) in [
        Action::Unequip { item: worn },
        Action::Equip { item: spare, slot },
        Action::Unequip { item: spare },
        Action::Equip { item: worn, slot },
    ]
    .into_iter()
    .enumerate()
    {
        for _ in 0..24 {
            wire.service.run_until_blocked();
            if wire.snapshot(OWNER).readiness.admission {
                break;
            }
            let next = wire.service.engine.next_actor().unwrap();
            wire.serial += 1;
            let wait = wire.request(next, Action::Wait);
            let id = format!("observe-clock-{}", wire.serial);
            ack(wire.send(next, &id, wait));
        }
        let request = wire.request(OWNER, action);
        let id = format!("observe-armor-{index}");
        ack(wire.send(OWNER, &id, request.clone()));
        wire.finish(OWNER, &id, &request);
    }
    let before = wire.snapshot(OWNER);
    let combat = before.state.observation.combat.as_ref().unwrap();
    let hp = combat.hp;
    let physical = combat.own_stats.as_ref().unwrap().defenses.physical;
    assert_eq!(reduction_amounts(&before), vec![2, 2, 2, 2]);
    assert!(before
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .all(|item| item.known_cursed.is_none() && item.known_protected.is_none()));
    assert_eq!(corrosion(&wire.saved()), vec![0, 0]);
    let mut last = None;
    for index in 0..32 {
        let snapshot = wire.snapshot(AQUATOR);
        let target = snapshot
            .state
            .observation
            .visible_actors
            .iter()
            .find(|a| a.id != snapshot.state.observation.self_target)
            .unwrap()
            .id;
        let request = wire.request(
            AQUATOR,
            Action::UseAbility {
                id: "rogue.corrode".into(),
                invocation: tor_protocol::AbilityInvocation::actor_target(target),
                supplies: SupplySelection::Carried,
            },
        );
        let id = format!("rust-{index}");
        ack(wire.send(AQUATOR, &id, request.clone()));
        let receipt = wire.finish(AQUATOR, &id, &request);
        if corrosion(&wire.saved()) == vec![0, 1] {
            last = Some((id, request, receipt));
            break;
        }
    }
    let (id, request, receipt) = last.expect("fixed-seed production contacts must include a hit");
    let after = wire.snapshot(OWNER);
    let combat = after.state.observation.combat.as_ref().unwrap();
    assert_eq!(combat.hp, hp);
    assert_eq!(
        combat.own_stats.as_ref().unwrap().defenses.physical,
        physical
    );
    assert_eq!(reduction_amounts(&after), vec![1, 1, 2, 2]);
    let encoded = serde_json::to_string(&after).unwrap();
    assert!(
        !encoded.contains("corrosion"),
        "public snapshot exposes current protection, not private wear storage"
    );
    let state = wire.saved();
    assert_eq!(ack(wire.send(AQUATOR, &id, request.clone())), receipt);
    let mut conflict = request.clone();
    let Request::Command {
        command:
            Command::Act {
                action: Action::UseAbility { id: ability_id, .. },
                ..
            },
        ..
    } = &mut conflict
    else {
        panic!()
    };
    *ability_id = "basic_melee".into();
    assert!(wire.send(AQUATOR, &id, conflict).iter().any(|m| matches!(
        m,
        ServerMessage::Error {
            code: ErrorCode::RequestConflict,
            ..
        }
    )));
    assert_eq!(
        wire.saved(),
        state,
        "retry and conflict publish no additional wear or sheet change"
    );
    wire.service.engine.flush().unwrap();
    drop(wire);
    let mut reopened =
        Wire::new(Engine::open_with_policy(&save, crate::Scenario::two_room(0), policy).unwrap());
    assert!(reopened.service.engine.recovery_profile().records_replayed > 0);
    assert_eq!(reopened.saved(), state);
    assert_eq!(
        reduction_amounts(&reopened.snapshot(OWNER)),
        vec![1, 1, 2, 2]
    );
    drop(reopened);
    std::fs::remove_dir_all(directory).unwrap();
}
