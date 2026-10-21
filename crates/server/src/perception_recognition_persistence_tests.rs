//! Persistent observer receipts and the rolled disguise outlive source-free eviction.
use super::*;
const SUBJECT: u64 = 3;

fn setup(engine: &mut Engine) -> bool {
    teleport(engine, OWNER, 1, 4, 3);
    spawn(engine, "fixture.observer", DETECTOR, 1, 4, 4);
    spawn(engine, "rogue.xeroc", SUBJECT, DISTANT, 4, 3);
    teleport(engine, OWNER, DISTANT, 3, 3);
    teleport(engine, DETECTOR, DISTANT, 4, 4);
    [OWNER, DETECTOR].into_iter().all(|observer| {
        disclosure(&engine.game, SimActor(observer), SimActor(SUBJECT))
            == ActorDisclosure::ApparentObject
    })
}
fn search(engine: &mut Engine) {
    command(
        engine,
        OWNER,
        Command::AdmitIntention {
            expected_revision: engine.revision(ActorId(OWNER)).unwrap(),
            action: Action::UseAbility {
                id: AbilityId::new("rogue.search").unwrap(),
                invocation: crate::actions::AbilityInvocation {
                    aim: crate::actions::AbilityAim::ActorTarget {
                        target: SimActor(OWNER),
                    },
                    weapon: crate::actions::WeaponSelection::Default,
                },
                supplies: Default::default(),
            },
        },
    );
    for _ in 0..32 {
        if disclosure(&engine.game, SimActor(OWNER), SimActor(SUBJECT)) == ActorDisclosure::Creature
        {
            return;
        }
        step(engine);
    }
    panic!("bounded ordinary paid Search did not resolve");
}

#[test]
fn private_recognition_and_failed_peer_receipt_survive_detach_source_free_reopen() {
    let base = scenario();
    let (source, mut warm) = (1..=64)
        .find_map(|seed| {
            let mut source = base.clone();
            source.seed = seed;
            let mut engine = Engine::memory(source.clone()).unwrap();
            setup(&mut engine).then_some((source, engine))
        })
        .expect("bounded ordinary failed-passive seed");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("recognition.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut cold = Engine::open_with_policy(&path, source, policy.clone()).unwrap();
    assert!(setup(&mut cold));
    assert_exact(&warm, &cold);
    let original_guise = guise(&warm, SUBJECT);
    for engine in [&mut warm, &mut cold] {
        search(engine);
        assert_eq!(
            guise(engine, SUBJECT),
            original_guise,
            "private recognition neither rerolls nor breaks guise"
        );
        assert_eq!(
            disclosure(&engine.game, SimActor(DETECTOR), SimActor(SUBJECT)),
            ActorDisclosure::ApparentObject
        );
        teleport(engine, OWNER, 1, 4, 3);
        teleport(engine, DETECTOR, 1, 4, 4);
        assert_eq!(
            engine.game.region_state(RegionId(DISTANT)),
            Some(RegionState::Detached)
        );
    }
    assert_exact(&warm, &cold);
    let saved = authority(&cold);
    cold.flush().unwrap();
    if let Some((on_disk, watermark)) = cold.store.as_ref().unwrap().take_written() {
        cold.regions.as_mut().unwrap().written(on_disk, watermark);
    }
    cold.regions.as_mut().unwrap().evict_durable();
    assert_eq!(cold.regions.as_ref().unwrap().resident_count(), 0);
    drop(cold);
    let mut reopened = Engine::open_with_policy(&path, Scenario::two_room(999), policy).unwrap();
    assert_eq!(authority(&reopened), saved);
    assert_exact(&warm, &reopened);
    for engine in [&mut warm, &mut reopened] {
        attach_distant(engine, false);
        assert_eq!(guise(engine, SUBJECT), original_guise);
        attach_distant(engine, true);
        let rng = authority(engine)["game"]["rules_rng"].clone();
        teleport(engine, OWNER, DISTANT, 3, 3);
        teleport(engine, DETECTOR, DISTANT, 4, 4);
        assert_eq!(
            disclosure(&engine.game, SimActor(OWNER), SimActor(SUBJECT)),
            ActorDisclosure::Creature
        );
        assert_eq!(
            disclosure(&engine.game, SimActor(DETECTOR), SimActor(SUBJECT)),
            ActorDisclosure::ApparentObject
        );
        assert_eq!(guise(engine, SUBJECT), original_guise);
        assert_eq!(
            authority(engine)["game"]["rules_rng"],
            rng,
            "no first-sight/adjacency reroll on reentry"
        );
    }
    assert_exact(&warm, &reopened);
}
