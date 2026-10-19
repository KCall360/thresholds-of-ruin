//! One encoded Service contract: ordinary player input cannot widen P/X disclosure.
//! Production Phantom/Xeroc builds, MI1/MI2 and basic melee are unchanged. Only
//! room geometry, external control and the observers' harmless probe grants are synthetic.
use super::*;
use crate::scenario_package::{resolve_rules_source, resolve_source, Package, RegionDef};
use serde_json::{json, Value};
use std::path::Path;

const OWNER: ActorId = ActorId(1);
const SUBJECT: ActorId = ActorId(2);
const WITNESS: ActorId = ActorId(3);

fn scenario(xeroc: bool) -> crate::Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let production = resolve_source(&root).unwrap();
    let source = std::fs::read_to_string(root.join(&production.manifest.ruleset)).unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(&resolve_rules_source(&source, "rules.toml").unwrap().source).unwrap();
    let mut manifest = serde_json::to_value(production.manifest).unwrap();
    let production_subjects = ["rogue.phantom", "rogue.xeroc"].map(|id| {
        (
            manifest["archetypes"][id].clone(),
            manifest["creatures"]["species"][id].clone(),
        )
    });
    let mut grants = Vec::new();
    for (id, reach, effects) in [
        (
            "fixture_sight",
            json!({"kind":"sight","range":6}),
            json!([]),
        ),
        (
            "fixture_ally",
            json!({"kind":"self_or_ally","range":{"kind":"touch"}}),
            json!([]),
        ),
        (
            "fixture_read_plan",
            json!({"kind":"sight","range":6}),
            json!([{"kind":"observe_public_plan"}]),
        ),
    ] {
        rules.abilities.push(serde_json::from_value(json!({
            "id":id,"name":id,"tags":[],"basic_attack":false,"invocation":"actor_target",
            "cost":null,"timing":{"source":{"kind":"fixed","prepare":50,"recover":50},"scaled_by":null},
            "reach":reach,"strikes":{"kind":"authored","strikes":[{
                "offset":0,"unlock":null,"check":null,"on_success":effects,
                "on_failure":[],"descriptors":[],"riders":[]}]},
            "interaction":"non_hostile",
            "ai":{"priority":0,"hostile":false,"minimum_resource_percent":0,"range_preference":"any"}
        })).unwrap());
        grants.push(json!({"scope":"always","op":{"kind":"grant_ability","ability":id}}));
    }
    manifest["creatures"]["templates"]["fixture_probes"] = json!({"priority":0,"modifiers":grants});
    manifest["id"] = json!("perception-service");
    manifest["characters"].as_array_mut().unwrap().truncate(1);
    let character = &mut manifest["characters"][0];
    character["anchor"] = json!("1/start");
    character["known_identities"] = json!([]);
    character["loadout"] = if xeroc {
        json!([{"archetype":"rogue.mace","quantity":1}])
    } else {
        json!([])
    };
    character["creature"]["nutrition"] = json!({"kind":"exempt"});
    character["creature"]["future_growth"] = json!({"kind":"disabled"});
    character["creature"]["templates"] = json!(["fixture_probes"]);
    let mut witness = character["creature"].clone();
    witness["name"] = json!("fixture witness");
    let body = character["body"].clone();
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
    for (index, id) in ["rogue.phantom", "rogue.xeroc"].into_iter().enumerate() {
        assert_eq!(
            production_subjects[index],
            (
                manifest["archetypes"][id].clone(),
                manifest["creatures"]["species"][id].clone()
            )
        );
    }
    let region: RegionDef = serde_json::from_value(json!({
        "id":1,"name":"Perception laboratory","size":[7,5,2],"chamber":true,
        "anchors":{"start":[2,2,0]},"actors":[
            {"id":2,"at":[3,2,0],
             "archetype":if xeroc {"rogue.xeroc"} else {"rogue.phantom"},"controller":"external"},
            {"id":3,"at":[2,3,0],"turn_ticks":100,"body":body,"creature":witness,"controller":"external"}
        ]
    })).unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![region],
        &toml::to_string(&rules).unwrap(),
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
    owner_frames: Vec<(ServerMessage, String)>,
    serial: usize,
}

impl Wire {
    fn new(xeroc: bool) -> Self {
        let mut engine = Engine::memory(scenario(xeroc)).unwrap();
        if xeroc {
            // Trusted fixture setup, before any client connects. Start all
            // bodies in distinct cells. Drop a genuine source-owned loadout
            // item and move only the observer. X stays at its authored cell:
            // any relocation would legitimately reveal its one-shot guise.
            // No made-up inventory ID, occupancy bypass, or guise edit is used.
            let item = engine.observation(OWNER).unwrap().inventory[0].id;
            let action = engine
                .decode_action(
                    OWNER,
                    &Action::Drop {
                        item,
                        quantity: None,
                    },
                )
                .unwrap();
            engine
                .command(
                    "fixture-setup",
                    "fixture-setup",
                    OWNER,
                    "fixture-drop",
                    &engine.branch().clone(),
                    crate::journal::Command::Act {
                        expected_revision: engine.revision(OWNER).unwrap(),
                        action,
                    },
                )
                .unwrap();
            engine.enable_wizard().unwrap();
            engine
                .command(
                    "fixture-setup",
                    "fixture-setup",
                    OWNER,
                    "fixture-place-owner",
                    &engine.branch().clone(),
                    crate::journal::Command::Wizard {
                        expected_revision: engine.revision(OWNER).unwrap(),
                        operation: crate::journal::WizardOperation::Teleport {
                            actor: OWNER,
                            position: crate::journal::Position {
                                region: 1,
                                x: 2,
                                y: 1,
                                z: 0,
                            },
                        },
                    },
                )
                .unwrap();
        }
        let mut wire = Self {
            service: Service::new(engine),
            clients: BTreeMap::new(),
            owner_frames: vec![],
            serial: 0,
        };
        for actor in [OWNER, SUBJECT, WITNESS] {
            let client = wire
                .service
                .connect(
                    &Account {
                        user: format!("fixture-{}", actor.0),
                        token: "test-only".into(),
                        role: AccessRole::Player,
                        actors: BTreeSet::from([actor]),
                    },
                    "encoded-perception".into(),
                )
                .unwrap();
            wire.clients.insert(actor, client);
            wire.send(actor, "attach", Request::Attach { actor });
            wire.send(actor, "control", Request::AcquireControl);
        }
        wire
    }

    fn drain(&mut self, actor: ActorId) -> Vec<ServerMessage> {
        let mut messages = vec![];
        while let Ok(frame) = self
            .clients
            .get_mut(&actor)
            .unwrap()
            .messages
            .try_recv_frame()
        {
            let message = decode_response(&frame.text).unwrap();
            if actor == OWNER {
                self.owner_frames
                    .push((message.clone(), frame.text.to_string()));
            }
            messages.push(message);
        }
        messages
    }

    fn send(&mut self, actor: ActorId, id: &str, request: Request) -> Vec<ServerMessage> {
        // This is the same protocol decoder used by transport, followed by the
        // normal Service seam. No fabricated response DTO enters this contract.
        let text = serde_json::to_string(&ClientMessage::Request {
            request_id: id.into(),
            request: Box::new(request),
        })
        .unwrap();
        let ClientMessage::Request {
            request_id,
            request,
        } = decode_request(&text).unwrap()
        else {
            panic!("request")
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
                ServerMessage::Error { .. } => panic!("snapshot failed: {message:?}"),
                _ => None,
            })
            .expect("actual encoded snapshot")
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

    fn act(&mut self, actor: ActorId, id: &str, action: Action) -> (Request, RequestReceipt) {
        let request = self.request(actor, action);
        let receipt = ack(self.send(actor, id, request.clone()));
        (request, receipt)
    }

    fn finish(&mut self, id: &str, request: &Request) -> RequestReceipt {
        for _ in 0..24 {
            self.service.run_until_blocked();
            for actor in [OWNER, SUBJECT, WITNESS] {
                assert!(!self
                    .drain(actor)
                    .iter()
                    .any(|m| matches!(m, ServerMessage::Error { .. })));
            }
            let receipt = ack(self.send(OWNER, id, request.clone()));
            if matches!(
                receipt,
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Resolved,
                    ..
                }
            ) {
                return receipt;
            }
            assert!(
                matches!(
                    receipt,
                    RequestReceipt::Admitted {
                        phase: IntentionPhase::Queued | IntentionPhase::Started,
                        ..
                    }
                ),
                "unexpected lifecycle: {receipt:?}"
            );
            let next = self
                .service
                .engine
                .next_actor()
                .expect("external fixture turn");
            self.serial += 1;
            let id = format!("clock-{}", self.serial);
            self.act(next, &id, Action::Wait);
        }
        panic!("bounded ordinary action clock did not resolve {id}");
    }

    fn reject(&mut self, id: &str, action: Action, code: ErrorCode) {
        let request = self.request(OWNER, action);
        let before = self.snapshot_state();
        rejected(self.send(OWNER, id, request), code);
        assert_eq!(
            self.snapshot_state(),
            before,
            "rejection must not spend a turn or reveal"
        );
    }

    fn snapshot_state(&self) -> StateView {
        self.service.engine.state(OWNER).unwrap()
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
        .expect("encoded acknowledgement")
}

fn rejected(messages: Vec<ServerMessage>, code: ErrorCode) {
    assert!(!messages
        .iter()
        .any(|m| matches!(m, ServerMessage::Ack { .. })));
    assert!(
        messages
            .iter()
            .any(|m| matches!(m, ServerMessage::Error { code: actual, .. } if *actual == code)),
        "{messages:?}"
    );
}

fn attack(id: &str, target: ActorTarget) -> Action {
    Action::UseAbility {
        id: id.into(),
        invocation: tor_protocol::AbilityInvocation::actor_target(target),
        supplies: SupplySelection::Carried,
    }
}

#[test]
fn encoded_service_preserves_contact_and_apparent_object_authority() {
    let mut p = Wire::new(false);
    let initial = p.snapshot(OWNER);
    let contacts: Vec<_> = initial
        .state
        .observation
        .visible_actors
        .iter()
        .filter(|row| row.name.is_empty() && row.id != initial.state.observation.self_target)
        .collect();
    assert!(
        !contacts.is_empty(),
        "production invisible Phantom supplies contact"
    );
    let contact = contacts[0].id;
    assert!(contacts.iter().all(|row| row.id == contact
        && row.asset.is_none()
        && row.appearance_seed.is_none()
        && row.description.is_empty()));
    let (_, text) = p
        .owner_frames
        .iter()
        .rev()
        .find(|(message, _)| matches!(message, ServerMessage::Snapshot { .. }))
        .unwrap();
    let actual: Value = serde_json::from_str(text).unwrap();
    for row in actual["snapshot"]["state"]["observation"]["visible_actors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|row| row["id"] == json!(contact))
    {
        assert_eq!(
            row.as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["description", "id", "name", "position"]),
            "anonymous contact has no extra private sheet, faction or plan fields"
        );
    }
    let combat = initial.state.observation.combat.as_ref().unwrap();
    assert!(combat.public_plan.is_none());
    assert!(!combat.actors.iter().any(|row| row.actor == contact));
    // Fixture knowledge is used only to forge forbidden credentials, never to
    // select a positive target. The player selects the actual disclosed row.
    let scope = p.service.engine.target_scope(OWNER);
    let normal = scope.actor(tor_simulation::ActorId(SUBJECT.0));
    assert_ne!(normal, contact);
    p.reject(
        "forged-creature",
        attack("basic_melee", normal),
        ErrorCode::InvalidAction,
    );
    for id in ["fixture_sight", "fixture_ally", "fixture_read_plan"] {
        p.reject(id, attack(id, contact), ErrorCode::InvalidAction);
    }
    let melee = attack("basic_melee", contact);
    let (request, _) = p.act(OWNER, "contact-melee", melee.clone());
    assert!(matches!(p.service.step(), Step::Progress));
    let prepared = p.snapshot(OWNER);
    let preparation = prepared
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .preparation
        .as_ref()
        .expect("ordinary paid preparation");
    assert_eq!(preparation.action, melee);
    assert!(preparation.remaining > 0 && preparation.active);
    p.finish("contact-melee", &request);
    let after = p.snapshot(OWNER);
    assert!(after.state.observation.tick > initial.state.observation.tick);
    assert!(after.history.entries.iter().any(|entry| matches!(&entry.content,
        HistoryContent::Action { action, event } if action == &melee
            && matches!(event.as_ref(), Event::AbilityStarted { target, .. } if *target == contact))));
    assert!(after.state.observation.combat.as_ref().unwrap().events.iter().any(|event| matches!(event, CombatEventView::Ability { target: Some(target), .. } if *target == contact)));
    for (message, text) in &p.owner_frames {
        // Theme palettes are public asset catalogs, not subject-linked disclosure.
        if matches!(message, ServerMessage::Palette { .. }) {
            continue;
        }
        for private in [
            normal.to_string(),
            scope
                .appearance_seed(tor_simulation::ActorId(SUBJECT.0))
                .to_string(),
            "phantom".into(),
            "dungeon_foes".into(),
        ] {
            assert!(
                !text.contains(&private),
                "private subject fact in actual frame: {private}"
            );
        }
    }

    let mut x = Wire::new(true);
    let passive = x.snapshot(OWNER);
    // Actor-audience Drop provenance survives setup; private wizard entries
    // have a distinct author and are not disclosed to this player.
    assert!(!passive
        .history
        .entries
        .iter()
        .any(|entry| matches!(entry.content, HistoryContent::Wizard { .. })));
    let dropped = passive
        .history
        .entries
        .iter()
        .find_map(|entry| match &entry.content {
            HistoryContent::Action {
                action:
                    Action::Drop {
                        item: requested,
                        quantity: None,
                    },
                event,
            } => match event.as_ref() {
                Event::Dropped { item, result, .. } if requested == item => Some(*result),
                _ => None,
            },
            _ => None,
        })
        .expect("real ordinary drop provenance");
    let rows: Vec<_> = passive
        .state
        .observation
        .ground_items
        .iter()
        .filter(|row| row.item.asset.as_deref() == Some("item.weapon.mace"))
        .collect();
    assert_eq!(rows.len(), 2);
    let ordinary = rows.iter().find(|row| row.item.id == dropped).unwrap();
    let apparent = rows.iter().find(|row| row.item.id != dropped).unwrap();
    let token = apparent.item.id;
    assert_ne!(ordinary.position, apparent.position);
    assert!(!ordinary.reachable && !apparent.reachable);
    let mut same = (**apparent).clone();
    same.item.id = dropped;
    // Public geometry is legitimate. Exact co-located projection equality is
    // covered by the separate persistence contract; never move X just to make
    // this comparison erase an intentional guise-lifecycle boundary.
    same.position = ordinary.position;
    assert_eq!(
        same, **ordinary,
        "all passive item metadata and reach flags match; only location and opaque handle differ"
    );
    assert!(passive.state.observation.inventory.is_empty());
    assert!(!passive
        .state
        .observation
        .visible_actors
        .iter()
        .any(|row| row.name == "xeroc"));
    assert!(!x
        .snapshot(WITNESS)
        .state
        .observation
        .visible_actors
        .iter()
        .any(|row| row.name == "xeroc"));
    x.reject(
        "invalid-quantity",
        Action::Take {
            item: token,
            quantity: Some(2),
        },
        ErrorCode::InvalidAction,
    );
    assert_eq!(x.snapshot(OWNER).state, passive.state);
    for (message, text) in &x.owner_frames {
        if !matches!(message, ServerMessage::Palette { .. }) {
            assert!(
                !text.contains("xeroc"),
                "passive disguise leaks its creature identity"
            );
        }
    }
    let (touch, admitted) = x.act(
        OWNER,
        "touch-guise",
        Action::Take {
            item: token,
            quantity: None,
        },
    );
    assert_eq!(
        x.snapshot_state().observation,
        passive.state.observation,
        "admission itself neither reveals nor transfers"
    );
    let resolved = x.finish("touch-guise", &touch);
    assert_eq!(admitted.entry_id(), resolved.entry_id());
    let revealed = x.snapshot(OWNER);
    assert!(
        revealed.state.observation.tick > passive.state.observation.tick,
        "ordinary clock pays for touch"
    );
    assert!(
        revealed.state.observation.inventory.is_empty(),
        "an apparent object never becomes loot"
    );
    assert_eq!(revealed.state.observation.ground_items.len(), 1);
    assert_eq!(revealed.state.observation.ground_items[0].item.id, dropped);
    for actor in [OWNER, WITNESS] {
        let snapshot = x.snapshot(actor);
        assert!(snapshot
            .state
            .observation
            .visible_actors
            .iter()
            .any(|row| row.name == "xeroc"
                && row.asset.as_deref() == Some("creature.xeroc")
                && row.appearance_seed.is_some()));
    }
    assert!(revealed
        .history
        .entries
        .iter()
        .any(|entry| matches!(&entry.content,
        HistoryContent::Action { action: Action::Take { item, quantity: None }, event }
            if *item == token && matches!(event.as_ref(), Event::Waited))));
    x.reject(
        "stale-apparent",
        Action::Take {
            item: token,
            quantity: None,
        },
        ErrorCode::InvalidAction,
    );
    let before_retry = x.snapshot_state();
    assert_eq!(
        ack(x.send(OWNER, "touch-guise", touch.clone())),
        resolved,
        "original request survives stale target, revision and stream context"
    );
    let mut different = touch;
    let Request::Command {
        command:
            Command::Act {
                action: Action::Take { quantity, .. },
                ..
            },
        ..
    } = &mut different
    else {
        panic!("take request")
    };
    *quantity = Some(1);
    rejected(
        x.send(OWNER, "touch-guise", different),
        ErrorCode::RequestConflict,
    );
    assert_eq!(
        x.snapshot_state(),
        before_retry,
        "exact retry and quantity conflict are mutation-free"
    );
}
