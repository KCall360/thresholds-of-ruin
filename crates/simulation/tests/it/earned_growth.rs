use super::creature_fixture as fixture;
use tor_simulation::creatures::{record::CreatureRecord, CreatureBuild};
use tor_simulation::progression::{HdLedger, HdSource};
use tor_simulation::{ActorId, CreatureIdentity, Game};
#[test]
fn fatal_effect_awards_earned_xp_to_its_owner() {
    let original = fixture::build(fixture::species());
    let build = CreatureBuild::new(
        original.content().clone(),
        original.species_id().clone(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap();
    let mut game = Game::two_room_in_stone(42, build.content().clone());
    for x in [1, 2] {
        let id = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: format!("subject {x}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.apply(tor_simulation::abilities::Outcome {
        actor: ActorId(1),
        target: ActorId(2),
        ability: None,
        intention: None,
        result: tor_simulation::abilities::OutcomeStatus::Applied,
        check: None,
        participation: tor_simulation::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![tor_simulation::abilities::ResolvedEffect::Damage { amount: 1_000_000 }],
    })
    .unwrap();
    assert!(!game.alive(ActorId(2)));
    let owner =
        serde_json::to_value(CreatureRecord::capture(&game.creature(ActorId(1)).unwrap())).unwrap();
    assert_eq!(owner["earned_progress"]["earned_xp"], 25);
}

fn three_actor_game() -> Game {
    let original = fixture::build(fixture::species());
    let build = CreatureBuild::new(
        original.content().clone(),
        original.species_id().clone(),
        HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap();
    let mut game = Game::two_room_in_stone(42, build.content().clone());
    for x in [1, 2, 3] {
        let id = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: format!("subject {x}"),
                faction: "allies".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game
}
fn damage(game: &mut Game, source: ActorId, target: ActorId, amount: u32) {
    game.apply(tor_simulation::abilities::Outcome {
        actor: source,
        target,
        ability: None,
        intention: None,
        result: tor_simulation::abilities::OutcomeStatus::Applied,
        check: None,
        participation: tor_simulation::abilities::AttemptedParticipation::target(),
        target_rider_participated: false,
        effects: vec![tor_simulation::abilities::ResolvedEffect::Damage { amount }],
    })
    .unwrap();
}
fn xp(game: &Game, actor: ActorId) -> u64 {
    game.creature(actor).unwrap().earned_progress().earned_xp()
}
#[test]
fn ally_fatal_effect_gets_credit_without_contribution_sharing() {
    let mut game = three_actor_game();
    damage(&mut game, ActorId(1), ActorId(2), 1);
    assert_eq!(xp(&game, ActorId(1)), 0);
    damage(&mut game, ActorId(3), ActorId(2), 1_000_000);
    assert_eq!(xp(&game, ActorId(1)), 0);
    assert_eq!(xp(&game, ActorId(3)), 25);
}
#[test]
fn zero_damage_self_alias_and_unattributed_environment_do_not_earn_xp() {
    let mut game = three_actor_game();
    damage(&mut game, ActorId(1), ActorId(2), 0);
    assert_eq!(xp(&game, ActorId(1)), 0);
    damage(&mut game, ActorId(2), ActorId(2), 1_000_000);
    assert_eq!(xp(&game, ActorId(2)), 0);
    assert_eq!(xp(&game, ActorId(1)), 0);
}
#[test]
fn repeated_dead_target_and_checkpoint_reopen_cannot_award_twice() {
    let mut game = three_actor_game();
    damage(&mut game, ActorId(1), ActorId(2), 1_000_000);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(checkpoint, shared)).unwrap();
    let (checkpoint, shared) = serde_json::from_slice(&bytes).unwrap();
    let mut reopened =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(reopened, game);
    for state in [&mut game, &mut reopened] {
        damage(state, ActorId(1), ActorId(2), 1_000_000);
        assert_eq!(xp(state, ActorId(1)), 25);
    }
    assert_eq!(reopened, game);
}
#[test]
fn selecting_a_future_source_applies_held_xp_and_stales_older_previews() {
    use tor_simulation::earned_growth::GrowthSource;
    let mut game = three_actor_game();
    // Each ordinary death gives 25 at this source-composition baseline. Held
    // XP is tested at a lower authored threshold in the creature kernel tests.
    damage(&mut game, ActorId(1), ActorId(2), 1_000_000);
    let basis = tor_simulation::player_advancement::AdvancementBasis {
        actor: ActorId(1),
        branch: "main".into(),
        stream: "owner".into(),
        reset: 0,
        build_epoch: game.creature(ActorId(1)).unwrap().build_epoch(),
    };
    let before = game.clone();
    let source = GrowthSource::Racial {};
    let preview = game
        .preview_player_growth_source(ActorId(1), &basis, &source)
        .unwrap();
    assert_eq!(game, before);
    game.act(
        ActorId(1),
        tor_simulation::Action::ChooseGrowthSource {
            basis: basis.clone(),
            source: source.clone(),
        },
    )
    .unwrap();
    assert_eq!(game.creature(ActorId(1)).unwrap().build(), preview.build());
    assert_eq!(xp(&game, ActorId(1)), 25);
    let after = game.clone();
    assert!(game
        .act(
            ActorId(1),
            tor_simulation::Action::ChooseGrowthSource { basis, source }
        )
        .is_err());
    assert_eq!(game, after);
}
