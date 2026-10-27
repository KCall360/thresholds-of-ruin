//! Checked recognition is private knowledge, not destruction of a disguise.
use super::*;
use crate::perception::{ActorDisclosure, AmbushState};

fn disguised() -> Game {
    let mut game = fixture(&[], &[guise(), ambush()]);
    game.teleport(OWNER, at(1, 7, 2)).unwrap();
    game
}
fn disclosed(game: &Game, observer: ActorId) -> ActorDisclosure {
    game.classified_actors(observer)
        .unwrap()
        .get(&SUBJECT)
        .map_or(ActorDisclosure::Hidden, |row| row.disclosure)
}
fn seed_for(game: &Game, distance: u32, success: bool) -> u64 {
    let threshold = game.actors[&SUBJECT].guise.concealment_total().unwrap();
    (1..100_000)
        .find(|seed| {
            let mut rng = *seed;
            (check(game).roll_at(&mut rng, true, distance) >= threshold) == success
        })
        .expect("fixture supports both contest outcomes")
}

#[test]
fn passive_guise_receipts_are_private_and_survive_encoded_restore() {
    for success in [true, false] {
        let mut game = disguised();
        let witness = spawn(&mut game, "fixture_perception_owner", at(1, 7, 3));
        game.rules_rng = seed_for(&game, 4, success);
        let subject = game.actors[&SUBJECT].clone();
        let mut expected = game.rules_rng;
        check(&game).roll_at(&mut expected, true, 4);
        game.reconcile_passive_discovery_for(OWNER).unwrap();
        assert_eq!(game.rules_rng, expected);
        assert_eq!(game.guise_known(OWNER, SUBJECT), success);
        assert!(!game.guise_known(witness, SUBJECT));
        assert_eq!(attempts(&game, OWNER), 1);
        assert_eq!(
            game.actors[&SUBJECT], subject,
            "recognition never informs or activates the subject"
        );
        assert_eq!(game.actors[&SUBJECT].guise.ambush, AmbushState::Available);
        if success {
            assert_eq!(disclosed(&game, OWNER), ActorDisclosure::Creature);
            assert!(game.genuinely_sees_actor(OWNER, SUBJECT, 6));
        }
        assert!(!game.observe(witness).unwrap().apparent_objects.is_empty());
        let mut restored = checkpoint(&game);
        for _ in 0..3 {
            restored.reconcile_passive_discovery_for(OWNER).unwrap();
        }
        assert_eq!(restored.rules_rng, expected);
        assert_eq!(restored.guise_known(OWNER, SUBJECT), success);
        assert_eq!(restored.actors[&SUBJECT].guise, subject.guise);
    }
}

#[test]
fn first_adjacency_is_a_second_persisted_opportunity_not_a_reentry_reroll() {
    let mut game = disguised();
    game.rules_rng = seed_for(&game, 4, false);
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    let mut game = checkpoint(&game);
    game.teleport(OWNER, at(1, 4, 2)).unwrap();
    game.rules_rng = seed_for(&game, 1, false);
    let mut expected = game.rules_rng;
    check(&game).roll_at(&mut expected, true, 1);
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert_eq!(game.rules_rng, expected);
    let fact = game.actors[&OWNER]
        .discovery
        .fact(DiscoveryTarget::Guise(SUBJECT))
        .unwrap();
    assert!(fact.attempted && fact.adjacent_attempted && !fact.presence);
    let mut game = checkpoint(&game);
    for x in [7, 4, 7, 4] {
        game.teleport(OWNER, at(1, x, 2)).unwrap();
        game.reconcile_passive_discovery_for(OWNER).unwrap();
    }
    assert_eq!(game.rules_rng, expected);
}

#[test]
fn initially_adjacent_uses_one_roll_and_marks_both_opportunities() {
    let mut game = disguised();
    game.teleport(OWNER, at(1, 4, 2)).unwrap();
    game.rules_rng = seed_for(&game, 1, false);
    let mut expected = game.rules_rng;
    check(&game).roll_at(&mut expected, true, 1);
    game.initialize_discovery().unwrap();
    assert_eq!(game.rules_rng, expected);
    let fact = game.actors[&OWNER]
        .discovery
        .fact(DiscoveryTarget::Guise(SUBJECT))
        .unwrap();
    assert!(fact.attempted && fact.adjacent_attempted);
    game.initialize_discovery().unwrap();
    assert_eq!(game.rules_rng, expected);
}

#[test]
fn active_total_can_retry_failed_guise_without_global_reveal_or_an_extra_roll() {
    let mut game = disguised();
    game.rules_rng = seed_for(&game, 4, false);
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    let threshold = game.actors[&SUBJECT].guise.concealment_total().unwrap();
    let rng = game.rules_rng;
    game.apply_discovery_total(OWNER, threshold - 1).unwrap();
    assert!(!game.guise_known(OWNER, SUBJECT));
    game.apply_discovery_total(OWNER, threshold).unwrap();
    assert!(
        game.guise_known(OWNER, SUBJECT),
        "initiating Perception wins ties"
    );
    assert_eq!(disclosed(&game, OWNER), ActorDisclosure::Creature);
    assert_eq!(game.rules_rng, rng);
    assert!(game.actors[&SUBJECT].guise.appearance().is_some());
    assert_eq!(game.actors[&SUBJECT].guise.ambush, AmbushState::Available);
}

#[test]
fn paid_and_ring_share_one_sample_without_global_reveal() {
    for periodic in [false, true] {
        let mut game = disguised();
        // Resolve the separate first-sight opportunity before comparing active
        // paid/ring algorithms. A real paid publication also reconciles passive
        // discovery, unlike this direct periodic helper; it is not an extra
        // active Search roll and must not be omitted from the fixture baseline.
        game.rules_rng = seed_for(&game, 4, false);
        game.reconcile_passive_discovery_for(OWNER).unwrap();
        assert!(!game.guise_known(OWNER, SUBJECT));
        assert_eq!(attempts(&game, OWNER), 1);
        let threshold = game.actors[&SUBJECT].guise.concealment_total().unwrap();
        let mut expected = game.rules_rng;
        let captured = check(&game);
        let success = captured.sample(&mut expected).at(4) >= threshold;
        if periodic {
            game.run_active_discovery(OWNER, &captured).unwrap();
        } else {
            paid(&mut game);
        }
        assert_eq!(game.rules_rng, expected);
        assert_eq!(game.guise_known(OWNER, SUBJECT), success);
        assert!(game.actors[&SUBJECT].guise.appearance().is_some());
    }
}

#[test]
fn passive_initialization_discovers_guises_without_world_features() {
    let mut game = disguised();
    game.rules_rng = seed_for(&game, 4, true);
    assert!(!game.world.has_concealed_features());
    game.initialize_discovery().unwrap();
    assert!(game.guise_known(OWNER, SUBJECT));
    let rng = game.rules_rng;
    game.initialize_discovery().unwrap();
    game.observe(OWNER).unwrap();
    assert_eq!(game.rules_rng, rng);
}

#[test]
fn active_distance_bands_retain_one_shared_sample_through_the_standard_cap() {
    for edge in [-4, -1, 0, 1, 4] {
        let check = FrozenDiscoveryCheck { bonus: 2, edge };
        let mut sampled_rng = 123;
        let sampled = check.sample(&mut sampled_rng);
        let mut six_rng = 123;
        crate::dice::roll_check(&mut six_rng, crate::dice::Edge::from_counts(3, 0));
        assert_eq!(sampled_rng, six_rng);
        assert!(sampled.valid());
        for distance in [1, 2, 3, 4, 8] {
            let mut expected = 123;
            assert_eq!(
                sampled.at(distance),
                check.roll_at(&mut expected, false, distance)
            );
        }
    }
}

#[test]
fn recognition_not_failed_attempts_invalidates_identity_authority_witnesses() {
    let mut game = disguised();
    game.rules_rng = seed_for(&game, 4, false);
    let before = game.actors.visibility_epoch();
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert_eq!(game.actors.visibility_epoch(), before);
    game.apply_discovery_total(OWNER, 1000).unwrap();
    assert_ne!(game.actors.visibility_epoch(), before);
}

#[test]
fn guise_concealment_is_captured_once_and_queries_never_reroll_it() {
    let game = disguised();
    let total = game.actors[&SUBJECT].guise.concealment_total();
    assert!(total.is_some());
    let rng = game.rules_rng;
    for _ in 0..3 {
        game.observe(OWNER).unwrap();
        game.observe(SUBJECT).unwrap();
    }
    assert_eq!(game.rules_rng, rng);
    let mut restored = checkpoint(&game);
    assert_eq!(restored.actors[&SUBJECT].guise.concealment_total(), total);
    restored.reveal_guise(SUBJECT).unwrap();
    assert_eq!(restored.actors[&SUBJECT].guise.concealment_total(), total);
    assert_eq!(restored.rules_rng, rng);
}

#[test]
fn discovery_samples_and_second_opportunity_flags_are_strict() {
    for value in [
        serde_json::json!([12, 11, 10]),
        serde_json::json!({"near":10,"middle":11,"far":9}),
        serde_json::json!({"near":12,"middle":11}),
        serde_json::json!({"near":12,"middle":11,"far":10,"extra":true}),
    ] {
        assert!(serde_json::from_value::<DiscoveryRoll>(value).is_err());
    }
    let invalid = serde_json::json!({
        "feature":{"kind":"guise","value":SUBJECT.0},
        "attempted":false,"adjacent_attempted":true,"presence":true,"family":false,
    });
    assert!(serde_json::from_value::<DiscoveryFact>(invalid).is_err());
    let invalid = serde_json::json!({
        "feature":{"kind":"guise","value":SUBJECT.0},
        "attempted":true,"adjacent_attempted":false,"presence":true,"family":true,
    });
    assert!(serde_json::from_value::<DiscoveryFact>(invalid).is_err());
}

#[test]
fn successful_private_recognition_does_not_notify_or_activate_a_readied_predator() {
    let mut game = fixture(
        &[],
        &[
            guise(),
            ambush(),
            PerceptionValue::ReadiedTouchAmbush { enabled: true },
        ],
    );
    game.configure_run(
        OWNER,
        BTreeSet::from([OWNER]),
        None,
        BTreeMap::from([(
            "private affiliation".into(),
            BTreeSet::from(["private affiliation".into()]),
        )]),
    )
    .unwrap();
    game.configure_ai(SUBJECT, crate::ai::AiProfile::default())
        .unwrap();
    let subject = game.actors[&SUBJECT].clone();
    let ai = game.combat.ai[&SUBJECT].clone();
    let rng = game.rules_rng;
    game.apply_discovery_total(OWNER, 1000).unwrap();
    assert!(game.guise_known(OWNER, SUBJECT));
    assert_eq!(game.actors[&SUBJECT], subject);
    assert_eq!(game.combat.ai[&SUBJECT], ai);
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.choose_ai(SUBJECT).unwrap().action, crate::Action::Wait);
    assert!(game.actors[&SUBJECT].guise.appearance().is_some());
}

#[test]
fn environmental_attempt_reveals_on_hit_or_miss_but_not_immunity_or_private_recognition() {
    let descriptor = crate::content_ids::DescriptorId::new("fire").unwrap();
    for immune in [false, true] {
        for hit in [false, true] {
            let mut game = fixture_with(&[], &[guise(), ambush()], |_, _, subject| {
                if immune {
                    subject.modifiers.push(crate::rules::Modifier {
                        cancellable: false,
                        scope: crate::rules::Scope::Always,
                        op: crate::rules::ModifierOp::Immunity {
                            selector: crate::rules::Selector::Descriptor {
                                descriptor: descriptor.clone(),
                            },
                        },
                    });
                }
            });
            let guise_before = game.actors[&SUBJECT].guise.clone();
            game.apply_discovery_total(OWNER, i64::from(i32::MAX))
                .unwrap();
            assert!(game.guise_known(OWNER, SUBJECT));
            assert_eq!(
                game.actors[&SUBJECT].guise, guise_before,
                "private recognition does not notify the subject"
            );
            let rng_before = game.rules_rng;
            let applied = game
                .with_recovery_transition(true, |candidate| {
                    candidate.resolve_environmental_attack_in_transaction(
                        SUBJECT,
                        if hit { 100 } else { -100 },
                        &[crate::rules::Effect::Heal {
                            amount: crate::rules::AmountExpr::Fixed { amount: 1 },
                        }],
                        std::slice::from_ref(&descriptor),
                    )
                })
                .unwrap();
            assert_eq!(applied, hit && !immune);
            assert_eq!(game.actors[&SUBJECT].guise.appearance().is_some(), immune,
                "an actual attack check, including a miss, breaks guise; immunity skips the attempt");
            if immune {
                assert_eq!(game.rules_rng, rng_before);
                assert_eq!(game.actors[&SUBJECT].guise, guise_before);
            }
        }
    }
}

#[test]
fn checkpoint_rejects_positional_discovery_target_inside_valid_named_receipt() {
    let mut game = disguised();
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot_value = serde_json::to_value(snapshot).unwrap();
    let valid = serde_json::to_value(shared).unwrap();
    let decoded_snapshot = serde_json::from_value(snapshot_value.clone()).unwrap();
    let decoded_shared = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(
        Game::restore_checkpoint(decoded_snapshot, &decoded_shared, game.content().clone())
            .unwrap(),
        game
    );

    let pool = snapshot_value["actors"].as_u64().unwrap() as usize;
    let mut invalid = valid.clone();
    let feature = &mut invalid["actors"][pool][OWNER.0.to_string()]["discovery"][0]["feature"];
    assert_eq!(
        *feature,
        serde_json::json!({"kind":"guise", "value":SUBJECT.0})
    );
    *feature = serde_json::json!(["guise", SUBJECT.0]);
    assert!(serde_json::from_value::<crate::checkpoint::SharedState>(invalid).is_err(),
        "nested target aliases must fail even inside otherwise valid named actor and receipt records");
    assert!(serde_json::from_value::<crate::checkpoint::SharedState>(valid).is_ok());
}

#[test]
fn detached_actor_record_rejects_positional_discovery_target_independently() {
    let mut game = disguised();
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    let record = crate::actor_record::ActorRecord::capture(&game.actors[&OWNER]);
    let valid = serde_json::to_value(&record).unwrap();
    let decoded: crate::actor_record::ActorRecord = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(decoded, record);
    assert_eq!(
        decoded
            .restore(OWNER, game.content().clone(), &game.items)
            .unwrap(),
        game.actors[&OWNER]
    );

    let mut invalid = valid.clone();
    let feature = &mut invalid["discovery"][0]["feature"];
    assert_eq!(
        *feature,
        serde_json::json!({"kind":"guise", "value":SUBJECT.0})
    );
    *feature = serde_json::json!(["guise", SUBJECT.0]);
    assert!(
        serde_json::from_value::<crate::actor_record::ActorRecord>(invalid).is_err(),
        "detached actor DTOs must independently enforce the nested named target boundary"
    );
    assert!(serde_json::from_value::<crate::actor_record::ActorRecord>(valid).is_ok());
}
