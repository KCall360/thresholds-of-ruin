//! Ordinary-package observation, committed recognition and historical persistence.
use crate::support;
use tor_protocol::{ActorId, PublicPlanStateView};
use tor_server as host;
use tor_server::{
    journal::{Action, Command},
    Engine, SavePolicy, Scenario,
};
#[path = "../../test_support/public_plan.rs"]
mod fixture;
use fixture::scenario;
const OBSERVER: ActorId = ActorId(1);
const SUBJECT: ActorId = ActorId(2);
fn action(id: &str, target: ActorId) -> Action {
    Action::UseAbility {
        id: tor_simulation::content_ids::AbilityId::new(id).unwrap(),
        invocation: tor_server::journal::AbilityInvocation {
            aim: tor_server::journal::AbilityAim::ActorTarget {
                target: tor_simulation::ActorId(target.0),
            },
            weapon: tor_server::journal::WeaponSelection::Default,
        },
        supplies: Default::default(),
    }
}
fn record(engine: &Engine) -> tor_protocol::PublicPlanObservationView {
    engine
        .state(OBSERVER)
        .unwrap()
        .observation
        .combat
        .unwrap()
        .public_plan
        .expect("paid ordinary result must reach its observer")
}
fn observe_preparation(engine: &mut Engine) {
    let expected_revision = engine.causal_boundary(OBSERVER).unwrap();
    support::submit(
        engine,
        OBSERVER,
        Command::AdmitIntention {
            expected_boundary: expected_revision,
            action: action("fixture_observe_plan", SUBJECT),
        },
    )
    .unwrap();
    engine.execute_next_intention().unwrap().unwrap();
    support::act_as(engine, SUBJECT, action("slow_plan", OBSERVER)).unwrap();
}
#[test]
fn public_plan_engine_projects_only_frozen_observer_facts_and_pure_repeated_reads() {
    let mut engine = Engine::memory(scenario(false)).unwrap();
    observe_preparation(&mut engine);
    let frozen = record(&engine);
    assert_eq!(
        (frozen.observed_at, frozen.subject_name.as_str()),
        (100, "subject")
    );
    assert!(matches!(
        &frozen.state,
        PublicPlanStateView::Preparing {
            ability: None,
            remaining_work: 200,
            ..
        }
    ));
    assert!(engine
        .state(SUBJECT)
        .unwrap()
        .observation
        .combat
        .unwrap()
        .public_plan
        .is_none());
    let before = engine.state(OBSERVER).unwrap();
    for _ in 0..3 {
        assert_eq!(engine.state(OBSERVER).unwrap(), before);
    }
    engine.pause_preparation(SUBJECT).unwrap();
    assert_eq!(record(&engine), frozen);
    let encoded = serde_json::to_value(frozen).unwrap();
    let text = encoded.to_string();
    for private in [
        "target_actor",
        "location",
        "region",
        "slow_plan",
        "Slow Plan",
    ] {
        assert!(!text.contains(private), "{private}");
    }
}
#[test]
fn public_plan_engine_wait_licenses_names_without_rewriting_old_results_and_replays() {
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("observation.db");
        let mut engine = Engine::open_with_policy(
            &path,
            scenario(false),
            SavePolicy {
                checkpoint_interval: interval,
                ..Default::default()
            },
        )
        .unwrap();
        observe_preparation(&mut engine);
        let unknown = record(&engine);
        support::act_as(&mut engine, OBSERVER, Action::Wait).unwrap();
        assert!(
            engine
                .state(OBSERVER)
                .unwrap()
                .observation
                .combat
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(event,
            tor_protocol::CombatEventView::Ability { ability, .. } if ability.id == "slow_plan")),
            "ordinary Wait publishes the licensed event"
        );
        assert_eq!(
            record(&engine),
            unknown,
            "new knowledge never fills an old unknown"
        );
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open(&path, Scenario::two_room(99)).unwrap();
        assert_eq!(record(&engine), unknown);
        // Reach the next jointly ready cycle without creating another slow-plan event.
        for _ in 0..16 {
            if engine.next_actor() == Some(OBSERVER)
                && engine.state(OBSERVER).unwrap().observation.tick >= 400
            {
                break;
            }
            let actor = engine.next_actor().unwrap();
            support::act_as(&mut engine, actor, Action::Wait).unwrap();
        }
        assert_eq!(engine.next_actor(), Some(OBSERVER));
        assert!(engine.state(OBSERVER).unwrap().observation.tick >= 400);
        observe_preparation(&mut engine);
        assert!(
            matches!(record(&engine).state, PublicPlanStateView::Preparing { ability: Some(ability), .. } if ability.id == "slow_plan"),
            "committed Wait recognition survives checkpoint and journal replay"
        );
    }
}
