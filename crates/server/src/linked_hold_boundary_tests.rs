//! Unique durable boundary: both accepting Escape and publishing its resolution
//! must roll back exact links, paid work and check state when the Store rejects.
use super::*;
use serde_json::{json, Value};

pub(super) fn scenario() -> crate::Scenario {
    let manifest: crate::scenario_package::Manifest =
        toml::from_str(include_str!("../../../scenarios/two-room/scenario.toml")).unwrap();
    let mut manifest = serde_json::to_value(manifest).unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    rules.abilities.push(
        serde_json::from_value(json!({
            "id":"fixture_linked_hold", "name":"Fixture Contact", "tags":["attack"],
            "basic_attack":false, "invocation":"actor_target", "interaction":"hostile_to_target",
            "timing":{"source":{"kind":"fixed","prepare":25,"recover":25}},
            "reach":{"kind":"melee"}, "strikes":{"kind":"authored","strikes":[{
                "offset":0, "on_success":[{"kind":"damage","damage":{"kind":"components",
                    "components":[{"category":"impact","amount":{"kind":"fixed","amount":0}}]}}],
                "on_failure":[], "descriptors":[], "riders":[{"trigger":"success","effects":[{
                    "kind":"apply_linked_hold","policy":{"sustaining":"clutch","preparation":100,
                        "recovery":100,"protection":400}}],"descriptors":[]}]}]},
            "ai":{"priority":0,"hostile":true,"minimum_resource_percent":0,"range_preference":"any"}
        }))
        .unwrap(),
    );
    let holder = json!({"kind":"humanoid","attributes":{"strength":0,"speed":0,"intellect":0,
        "willpower":0,"awareness":0,"presence":0}, "melee":{"skill":"heavy_weaponry","bonus":0,
            "wind_up":100,"recovery":100,"strikes":[{"offset":0,"on_success":[],"on_failure":[],
                "descriptors":[],"riders":[]}]}, "modifiers":[
        {"scope":"always","op":{"kind":"flag","flag":"clutch"}},
        {"scope":"always","op":{"kind":"grant_ability","ability":"fixture_linked_hold"}},
        {"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":200}}]});
    let mut subject = holder.clone();
    subject["modifiers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"scope":"always","op":{"kind":"add",
        "stat":{"kind":"attribute","attribute":"strength"},"value":100}}));
    manifest["creatures"] = json!({"species":{"escape_subject":subject,"escape_holder":holder}});
    let creature = |subject: bool| {
        json!({"species":if subject {"escape_subject"} else {"escape_holder"},
        "name":if subject {"Subject"} else {"Holder"}, "faction":"neutral", "binding":"intellect",
        "starting_skills":["heavy_weaponry","athletics"], "restoration":"authored",
        "nutrition":{"kind":"exempt"}, "hit_dice":[{"source":"racial"}]})
    };
    manifest["characters"][0]["creature"] = creature(true);
    let body = manifest["characters"][0]["body"].clone();
    let region = serde_json::from_value(json!({"id":1,"name":"Linked holds","size":[6,4,2],
        "chamber":true,"gravity":[0,0,-1],"anchors":{"start":[1,1,0]},"actors":[
            {"id":2,"at":[2,1,0],"turn_ticks":100,"controller":"external","body":body,"creature":creature(false)},
            {"id":3,"at":[1,2,0],"turn_ticks":100,"controller":"external","body":body,"creature":creature(false)}]})).unwrap();
    let package = crate::scenario_package::Package::from_parts(
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

fn authority(game: &Game) -> Value {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    json!([game.checkpoint(&mut shared), shared])
}

fn command(engine: &mut Engine, actor: ActorId, id: &str, command: Command) -> CommandResult {
    engine
        .command(
            "player",
            "linked-hold-test",
            actor,
            id,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}

fn wait(engine: &mut Engine, serial: usize) {
    let actor = engine.next_actor().expect("ordinary external turn");
    let request = Command::Act {
        expected_revision: engine.revision(actor).unwrap(),
        action: Action::Wait,
    };
    command(engine, actor, &format!("wait-{serial}"), request);
}

pub(super) fn establish(engine: &mut Engine) {
    establish_with_holders(engine, &[ActorId(2), ActorId(3)]);
}

pub(super) fn establish_with_holders(engine: &mut Engine, holders: &[ActorId]) {
    for &actor in holders {
        let request = Command::AdmitIntention {
            expected_revision: engine.revision(actor).unwrap(),
            action: Action::UseAbility {
                id: tor_simulation::content_ids::AbilityId::new("fixture_linked_hold").unwrap(),
                invocation: crate::actions::AbilityInvocation::ActorTarget {
                    target: SimActor(1),
                },
                supplies: Default::default(),
            },
        };
        command(engine, actor, &format!("contact-{}", actor.0), request);
    }
    for serial in 0..40 {
        if engine.game.linked_holds(SimActor(1)).len() == holders.len()
            && engine.next_actor() == Some(ActorId(1))
        {
            return;
        }
        if engine.execute_next_intention().unwrap().is_none() {
            wait(engine, serial);
        }
    }
    panic!("ordinary contacts must establish the requested incoming holds");
}

#[test]
fn escape_admission_and_resolution_storage_failures_preserve_exact_links_rng_and_receipts() {
    for (resolution, move_attempt) in [(false, false), (true, false), (false, true), (true, true)] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("escape.db");
        let scenario = scenario();
        let mut engine = Engine::open(&path, scenario.clone()).unwrap();
        establish(&mut engine);
        let holds = engine.game.linked_holds(SimActor(1));
        let chosen = holds[usize::from(!move_attempt)].id;
        let remaining = holds[usize::from(move_attempt)];
        let escape = Command::AdmitIntention {
            expected_revision: engine.revision(ActorId(1)).unwrap(),
            action: if move_attempt {
                Action::Move {
                    direction: Direction::West,
                }
            } else {
                Action::Escape { hold: chosen }
            },
        };
        let mut original = None;
        let (actor, request) = if resolution {
            original = Some(command(&mut engine, ActorId(1), "escape", escape));
            engine.execute_next_intention().unwrap().unwrap();
            assert!(engine.game.preparation(SimActor(1)).unwrap().active);
            let mut crossing = None;
            for serial in 40..80 {
                let actor = engine.next_actor().unwrap();
                let mut probe = engine.game.clone();
                probe
                    .act(SimActor(actor.0), tor_simulation::Action::Wait)
                    .unwrap();
                if !probe
                    .linked_holds(SimActor(1))
                    .iter()
                    .any(|hold| hold.id == chosen)
                {
                    crossing = Some((
                        actor,
                        Command::Act {
                            expected_revision: engine.revision(actor).unwrap(),
                            action: Action::Wait,
                        },
                    ));
                    break;
                }
                wait(&mut engine, serial);
            }
            crossing.expect("bounded ordinary wait must cross actual selected Escape resolution")
        } else {
            (ActorId(1), escape)
        };
        let branch = engine.branch().clone();
        engine.flush().unwrap();
        drop(engine);
        let mut rejected = Engine::open_with_policy(
            &path,
            scenario.clone(),
            crate::SavePolicy {
                max_pending_bytes: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let game = rejected.game.clone();
        let exact = authority(&game);
        let archive = serde_json::to_value(&rejected.archive).unwrap();
        let revisions = rejected.revisions.clone();
        let receipts = rejected.receipts.clone();
        let pending = rejected.game.preparation(SimActor(1)).cloned();
        let status = rejected.save_status();
        let original_receipt = original
            .as_ref()
            .map(|result| rejected.request_receipt(result));
        let mut healthy = Engine::replay(rejected.archive.clone(), None, None).unwrap();
        healthy
            .command(
                "player",
                "linked-hold-test",
                actor,
                "fault-request",
                &branch,
                request.clone(),
            )
            .unwrap();
        assert_ne!(
            healthy.game, game,
            "fault must occur after a meaningful candidate transition"
        );
        if resolution {
            assert_eq!(healthy.game.linked_holds(SimActor(1)), vec![remaining]);
        } else {
            assert!(healthy.game.pending_intention(SimActor(1)).is_some());
        }
        for _ in 0..2 {
            let error = rejected
                .command(
                    "player",
                    "linked-hold-test",
                    actor,
                    "fault-request",
                    &branch,
                    request.clone(),
                )
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::StorageFailure);
            assert_eq!(rejected.game, game);
            assert_eq!(
                authority(&rejected.game),
                exact,
                "includes hold IDs, paid progress and RNG/check state"
            );
            assert_eq!(rejected.game.linked_holds(SimActor(1)), holds);
            assert_eq!(rejected.game.preparation(SimActor(1)).cloned(), pending);
            assert_eq!(serde_json::to_value(&rejected.archive).unwrap(), archive);
            assert_eq!(rejected.revisions, revisions);
            assert_eq!(rejected.receipts, receipts);
            assert_eq!(
                rejected.save_status().accepted_sequence,
                status.accepted_sequence
            );
            assert_eq!(
                original
                    .as_ref()
                    .map(|result| rejected.request_receipt(result)),
                original_receipt
            );
            assert!(rejected
                .retry("player", actor, "fault-request", &branch, &request)
                .unwrap()
                .is_none());
        }
        drop(rejected);
        let mut retry = Engine::open(&path, scenario).unwrap();
        assert_eq!(retry.game, game);
        let accepted = retry
            .command(
                "player",
                "linked-hold-test",
                actor,
                "fault-request",
                &branch,
                request.clone(),
            )
            .unwrap();
        assert_eq!(retry.game, healthy.game);
        let completed = authority(&retry.game);
        let duplicate = retry
            .command(
                "player",
                "linked-hold-test",
                actor,
                "fault-request",
                &branch,
                request,
            )
            .unwrap();
        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry, accepted.entry);
        assert_eq!(authority(&retry.game), completed);
        if resolution {
            assert_eq!(retry.game.linked_holds(SimActor(1)), vec![remaining]);
            assert!(matches!(
                retry.request_receipt(original.as_ref().unwrap()),
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Resolved,
                    ..
                }
            ));
        }
    }
}
