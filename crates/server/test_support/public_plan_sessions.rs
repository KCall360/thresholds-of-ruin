//! Paid knowledge follows the existing actor-view capability, not control ownership.
use super::*;
use crate::public_plan_test_fixture as fixture;

fn act(engine: &mut Engine, actor: ActorId, action: crate::journal::Action) {
    engine
        .command(
            "player",
            "test",
            actor,
            &uuid::Uuid::new_v4().to_string(),
            &engine.branch().clone(),
            crate::journal::Command::Act {
                expected_revision: engine.revision(actor).unwrap(),
                action,
            },
        )
        .unwrap();
}
fn paid_engine() -> Engine {
    use crate::journal::{Action, Command};
    let mut engine = Engine::memory(fixture::scenario(false)).unwrap();
    let observe = Action::UseAbility {
        id: tor_simulation::content_ids::AbilityId::new("fixture_observe_plan").unwrap(),
        invocation: crate::journal::AbilityInvocation {
            aim: crate::journal::AbilityAim::ActorTarget {
                target: tor_simulation::ActorId(2),
            },
            weapon: crate::journal::WeaponSelection::Default,
        },
        supplies: Default::default(),
    };
    engine
        .command(
            "player",
            "test",
            ActorId(1),
            "paid-observation",
            &engine.branch().clone(),
            Command::AdmitIntention {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                action: observe,
            },
        )
        .unwrap();
    engine.execute_next_intention().unwrap().unwrap();
    act(
        &mut engine,
        ActorId(2),
        Action::UseAbility {
            id: tor_simulation::content_ids::AbilityId::new("slow_plan").unwrap(),
            invocation: crate::journal::AbilityInvocation {
                aim: crate::journal::AbilityAim::ActorTarget {
                    target: tor_simulation::ActorId(1),
                },
                weapon: crate::journal::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
    );
    assert!(engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .combat
        .unwrap()
        .public_plan
        .is_some());
    engine
}
fn connect(service: &mut Service, role: AccessRole, allowed: ActorId) -> Connection {
    let account = Account {
        user: format!("observer-{}-{role:?}", allowed.0),
        token: "test-only".into(),
        role,
        actors: BTreeSet::from([allowed]),
    };
    let mut client = service.connect(&account, "headless".into()).unwrap();
    let ServerMessage::Welcome { actors, .. } = client.messages.try_recv().unwrap() else {
        panic!("welcome")
    };
    assert_eq!(actors, vec![allowed]);
    client
}
fn attach(service: &mut Service, client: &mut Connection, actor: ActorId) -> Box<Snapshot> {
    service.handle(client.id, "attach".into(), Request::Attach { actor });
    let ServerMessage::Snapshot { snapshot, .. } = client.messages.try_recv().unwrap() else {
        panic!("authorized snapshot")
    };
    snapshot
}
#[test]
fn public_plan_same_actor_spectators_share_paid_history_but_foreign_observers_do_not() {
    let mut service = Service::new(paid_engine());
    let mut player = connect(&mut service, AccessRole::Player, ActorId(1));
    let own = attach(&mut service, &mut player, ActorId(1));
    let mut spectator = connect(&mut service, AccessRole::Spectator, ActorId(1));
    let watched = attach(&mut service, &mut spectator, ActorId(1));
    assert_eq!(own.state, watched.state);
    let record = own
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .public_plan
        .clone()
        .unwrap();
    assert_eq!(record.observed_at, 100);
    assert!(!watched.has_control);
    for _ in 0..2 {
        service.handle(spectator.id, "read".into(), Request::Snapshot);
        let ServerMessage::Snapshot { snapshot, .. } = spectator.messages.try_recv().unwrap()
        else {
            panic!("repeat snapshot")
        };
        assert_eq!(
            snapshot
                .state
                .observation
                .combat
                .as_ref()
                .unwrap()
                .public_plan
                .as_ref(),
            Some(&record)
        );
    }
    for role in [AccessRole::Player, AccessRole::Spectator] {
        let mut foreign = connect(&mut service, role, ActorId(2));
        service.handle(
            foreign.id,
            "denied".into(),
            Request::Attach { actor: ActorId(1) },
        );
        let denied = foreign.messages.try_recv().unwrap();
        assert!(matches!(
            denied,
            ServerMessage::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ));
        assert!(!serde_json::to_string(&denied)
            .unwrap()
            .contains("public_plan"));
        let snapshot = attach(&mut service, &mut foreign, ActorId(2));
        assert!(snapshot
            .state
            .observation
            .combat
            .as_ref()
            .unwrap()
            .public_plan
            .is_none());
        for rows in [
            serde_json::to_value(&snapshot.state.observation.visible_actors).unwrap(),
            serde_json::to_value(&snapshot.state.observation.combat.as_ref().unwrap().actors)
                .unwrap(),
        ] {
            assert!(
                !rows.to_string().contains("public_plan"),
                "foreign records never enter public actor rows"
            );
        }
    }
}
