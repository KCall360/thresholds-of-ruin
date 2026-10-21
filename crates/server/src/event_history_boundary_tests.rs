//! Backend handoff and actual Engine observer selection preserve frozen history.
use super::*;

#[test]
fn direct_game_receipts_survive_engine_handoff_without_widening() {
    let mut engine = Engine::memory(linked_hold_boundary_tests::scenario()).unwrap();
    let mut direct = engine.game.clone();
    direct
        .apply(tor_simulation::abilities::Outcome {
            actor: SimActor(1),
            target: SimActor(2),
            ability: Some(tor_simulation::content_ids::AbilityId::new("basic_melee").unwrap()),
            intention: None,
            result: tor_simulation::abilities::OutcomeStatus::Applied,
            check: None,
            resistance: None,
            participation: tor_simulation::abilities::AttemptedParticipation::target(),
            target_rider_participated: false,
            effects: vec![tor_simulation::abilities::ResolvedEffect::Damage { amount: 1 }],
        })
        .unwrap();
    let expected: Vec<_> = [SimActor(1), SimActor(2), SimActor(3)]
        .into_iter()
        .map(|actor| direct.observe(actor).unwrap().combat.unwrap().events)
        .collect();
    assert!(!expected[0].is_empty());
    assert!(!expected[1].is_empty());
    assert!(
        expected[2].is_empty(),
        "a later host cannot invent a missing observer receipt"
    );
    engine.game = direct;
    engine.observations = ObservationCache::default();
    let mut candidate = Candidate::capture(&engine);
    candidate.record_observation_knowledge(None).unwrap();
    for (actor, expected) in [ActorId(1), ActorId(2), ActorId(3)]
        .into_iter()
        .zip(expected)
    {
        assert_eq!(
            candidate
                .revision_view(actor)
                .unwrap()
                .observation
                .combat
                .unwrap()
                .events,
            expected
        );
    }
    assert_eq!(candidate.game, engine.game);
}

#[test]
fn engine_free_read_cache_warmth_cannot_change_saved_event_receipts() {
    let scenario = linked_hold_boundary_tests::scenario();
    let mut cold = Engine::memory(scenario.clone()).unwrap();
    let mut warm = Engine::memory(scenario).unwrap();
    cold.observations = ObservationCache::default();
    for actor in warm.actors() {
        warm.state(actor).unwrap();
    }
    linked_hold_boundary_tests::establish_with_holders(&mut cold, &[ActorId(2)]);
    linked_hold_boundary_tests::establish_with_holders(&mut warm, &[ActorId(2)]);
    assert_eq!(cold.game, warm.game);
    let snapshot = |game: &Game| {
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        serde_json::to_value(game.checkpoint(&mut shared)).unwrap()["event_receipts"].clone()
    };
    assert_eq!(snapshot(&cold.game), snapshot(&warm.game));
    for actor in cold.actors() {
        assert_eq!(
            cold.revision_view(actor).unwrap(),
            warm.revision_view(actor).unwrap()
        );
    }
}
