//! One encoded directional invocation owns one ordinary scheduler/payment route.
//! Existing fixture skills isolate the boundary; this selects no production skill.
use super::*;
use serde_json::{json, Value};

const SOURCE: ActorId = ActorId(1);
const DODGER: ActorId = ActorId(2);
const INTERCEPTOR: ActorId = ActorId(3);
const ACTORS: [ActorId; 3] = [SOURCE, DODGER, INTERCEPTOR];

fn scenario() -> crate::Scenario {
    let mut scenario = crate::public_plan_test_fixture::scenario(false);
    let package = scenario.package.take().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    let mut rules = package.rules().unwrap().document().clone();
    let ray: tor_simulation::rules::AbilityDef = serde_json::from_value(json!({
        "id":"fixture_directional_ray", "name":"Fixture directional ray", "tags":[],
        "basic_attack":false, "invocation":"directional_ray", "cost":null,
        "timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":null},
        "reach":{"kind":"sight","range":6},
        "strikes":{"kind":"authored","strikes":[{
            "offset":0,"unlock":null,
            "check":{"actor":"target","skill":{"kind":"named","skill":"agility"},
                "against":{"kind":"opposed","skill":{"kind":"named","skill":"athletics"}}},
            "on_success":[], "on_failure":[{"kind":"damage","damage":{"kind":"components",
                "components":[{"category":"energy","descriptor":null,"amount":{"kind":"rolled",
                    "dice":{"count":6,"sides":6,"bonus":0}}}]}}],
            "descriptors":[],"riders":[]}]},
        "interaction":"hostile_to_target",
        "ai":{"priority":0,"hostile":true,"minimum_resource_percent":0,"range_preference":"any"}
    }))
    .unwrap();
    rules.constants.default_abilities.push(ray.id.clone());
    rules.abilities.push(ray);
    let modifiers = manifest["creatures"]["species"]["reader"]["modifiers"]
        .as_array_mut()
        .unwrap();
    modifiers.extend([
        json!({"scope":"always","op":{"kind":"set","stat":{"kind":"health"},"value":100}}),
        json!({"scope":"always","op":{"kind":"add","stat":{"kind":"ability",
            "ability":"fixture_directional_ray","stat":"check_bonus"},"value":50}}),
    ]);
    manifest["creatures"]["templates"]["fixture_ray_dodger"] = json!({"priority":0,"kind":null,
        "modifiers":[{"scope":"always","op":{"kind":"add","stat":{"kind":"attribute","attribute":"speed"},"value":100}}]});
    let mut regions = package.region_defs().unwrap();
    let mut first = serde_json::to_value(&regions[0]).unwrap();
    let mut interceptor = first["actors"][0].clone();
    interceptor["id"] = json!(3);
    interceptor["at"] = json!([3, 1, 0]);
    interceptor["creature"]["name"] = json!("ray interceptor");
    first["actors"][0]["creature"]["templates"]
        .as_array_mut()
        .unwrap()
        .push(json!("fixture_ray_dodger"));
    first["actors"].as_array_mut().unwrap().push(interceptor);
    regions[0] = serde_json::from_value(first).unwrap();
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
        let mut wire = Self {
            service: Service::new(engine),
            clients: BTreeMap::new(),
        };
        for actor in ACTORS {
            let client = wire
                .service
                .connect(
                    &Account {
                        user: format!("ray-{}", actor.0),
                        token: "test-only".into(),
                        role: AccessRole::Player,
                        actors: [actor].into(),
                    },
                    "encoded-ray".into(),
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
    serde_json::to_value(
        crate::engine::Checkpoint::capture(&service.engine).encode("ray-service", 0),
    )
    .unwrap()
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

#[test]
fn encoded_directional_ray_dodges_continues_intercepts_retries_and_replays() {
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("ray.db");
        let policy = crate::SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let engine = Engine::open_with_policy(&path, scenario(), policy.clone()).unwrap();
        let mut wire = Wire::connect(engine);
        let initial = ACTORS.map(|actor| hp(&wire.service, actor));
        let action: Action = serde_json::from_value(json!({"type":"use_ability","id":"fixture_directional_ray",
            "invocation":{"aim":{"kind":"directional_ray","direction":"east"},"weapon":{"kind":"default"}},"supplies":{"kind":"carried"}})).unwrap();
        let encoded_action = serde_json::to_value(&action).unwrap();
        assert_eq!(
            encoded_action["invocation"],
            json!({"aim":{"kind":"directional_ray","direction":"east"},"weapon":{"kind":"default"}})
        );
        let request = wire.request(SOURCE, action);
        let first = ack(wire.send(SOURCE, "ray", request.clone()));
        let queued = saved(&wire.service);
        assert_eq!(ack(wire.send(SOURCE, "ray", request.clone())), first);
        assert_eq!(
            saved(&wire.service),
            queued,
            "duplicate admission consumes no state or RNG"
        );
        let mut completed = None;
        for ordinal in 0..12 {
            wire.service.run_until_blocked();
            for actor in ACTORS {
                assert!(!wire
                    .drain(actor)
                    .iter()
                    .any(|message| matches!(message, ServerMessage::Error { .. })));
            }
            let receipt = ack(wire.send(SOURCE, "ray", request.clone()));
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
            assert!(matches!(
                receipt,
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Queued | IntentionPhase::Started,
                    ..
                }
            ));
            let actor = wire.service.engine.next_actor().unwrap();
            let wait = wire.request(actor, Action::Wait);
            ack(wire.send(actor, &format!("clock-{ordinal}"), wait));
        }
        let completed = completed.expect("bounded ordinary scheduler completes ray");
        assert_eq!(hp(&wire.service, SOURCE), initial[0]);
        assert_eq!(
            hp(&wire.service, DODGER),
            initial[1],
            "successful Agility dodge deals zero and continues"
        );
        let damage = initial[2] - hp(&wire.service, INTERCEPTOR);
        assert!((6..=36).contains(&damage), "failure receives full six dice");
        let settled = saved(&wire.service);
        assert_eq!(ack(wire.send(SOURCE, "ray", request)), completed);
        assert_eq!(
            saved(&wire.service),
            settled,
            "resolved retry never launches a second ray"
        );
        let states = ACTORS.map(|actor| wire.service.engine.state(actor).unwrap());
        wire.service.engine.flush().unwrap();
        drop(wire);
        let engine = Engine::open_with_policy(&path, crate::Scenario::two_room(0), policy).unwrap();
        if interval == 4096 {
            assert!(engine.recovery_profile().records_replayed > 0);
        }
        let reopened = Service::new(engine);
        assert_eq!(
            saved(&reopened),
            settled,
            "checkpoint/journal restore preserves exact ray authority and RNG"
        );
        assert_eq!(
            ACTORS.map(|actor| reopened.engine.state(actor).unwrap()),
            states
        );
    }
}
