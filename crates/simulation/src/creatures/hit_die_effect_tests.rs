//! Reconstructed ordinary HD effects retain earned accounting and runtime state.
use super::{
    BuildAdvancement, CreatureBuild, CreatureState, PlannedAdvancement, RestorationPolicy,
};
use crate::earned_growth::{FutureGrowth, GrowthSource};
use crate::progression::{HdLedger, HdSource};
use crate::{ActorId, Game};

fn state(hd: usize, authored: bool, enabled: bool) -> CreatureState {
    let original = crate::test_creatures::build(crate::test_creatures::species());
    let mut build = CreatureBuild::new(
        original.content().clone(),
        original.species_id().clone(),
        HdLedger::seeded(vec![HdSource::Racial; hd], 42).unwrap(),
        original.binding(),
        *original.starting_skills(),
    )
    .unwrap();
    if authored {
        let restoration = RestorationPolicy::Authored(
            build
                .ledger()
                .entries()
                .iter()
                .zip(build.choices())
                .map(|(die, choices)| PlannedAdvancement {
                    source: die.source(),
                    choices: choices.clone(),
                })
                .collect(),
        );
        build = CreatureBuild::from_recorded(
            build.content().clone(),
            build.species_id().clone(),
            build.initial_attributes(),
            build.ledger().clone(),
            build.binding(),
            BuildAdvancement {
                restoration,
                ..build.advancement().clone()
            },
            vec![],
        )
        .unwrap();
    }
    if enabled {
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
    }
    CreatureState::new(build).unwrap()
}
fn install(game: &mut Game, state: CreatureState) {
    let mut actor = game.actors.get_mut(&ActorId(1)).unwrap();
    let identity = actor.combat.as_ref().unwrap().identity.clone();
    actor.combat = Some(crate::actor_creatures::CombatState::from_creature(
        identity, state,
    ));
}
fn game(state: CreatureState) -> Game {
    let mut game = Game::two_room_in_stone(42, state.build().content().clone());
    let actor = game
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            name: "growth subject".into(),
            faction: "neutral".into(),
        },
        state.build().clone(),
    )
    .unwrap();
    install(&mut game, state);
    game
}
fn effect(kind: &str) -> crate::rules::Effect {
    serde_json::from_value(serde_json::json!({"kind":kind})).unwrap()
}
fn apply(game: &mut Game, kind: &str) -> bool {
    game.apply_effects(ActorId(1), &[effect(kind)]).unwrap()
}
fn award(game: &mut Game, xp: u32) {
    let mut state = game.creature(ActorId(1)).unwrap().into_owned();
    state.award_xp(xp).unwrap();
    install(game, state);
}

#[test]
fn restoration_cannot_convert_funded_unapplied_growth_into_free_hit_dice() {
    let mut state = state(2, true, false);
    state.award_xp(500).unwrap();
    assert_eq!(state.earned_progress().highest_earned_tier().get(), 4);
    let mut game = game(state);
    assert!(apply(&mut game, "drain_latest_hit_die"));
    assert!(apply(&mut game, "restore_hit_die"));
    assert!(!apply(&mut game, "restore_hit_die"));
    let state = game.creature(ActorId(1)).unwrap();
    assert_eq!(state.build().ledger().total_hd().get(), 2);
    assert_eq!(state.earned_progress().highest_earned_tier().get(), 4);
    assert_eq!(state.earned_progress().held_xp(), 500);
    assert_eq!(state.earned_progress().spent_xp(), 0);
}

#[test]
fn sixty_percent_then_drain_requires_the_remaining_credit_to_regrow_six() {
    let mut game = game(state(6, false, true));
    let original = game.creature(ActorId(1)).unwrap().build().ledger().clone();
    award(&mut game, 360);
    let peak = game
        .creature(ActorId(1))
        .unwrap()
        .earned_progress()
        .peak_threat();
    assert!(apply(&mut game, "drain_latest_hit_die"));
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get(),
        5
    );
    award(&mut game, 239);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get(),
        5
    );
    award(&mut game, 1);
    let state = game.creature(ActorId(1)).unwrap();
    assert_eq!(state.build().ledger(), &original);
    assert_eq!(state.earned_progress().spent_xp(), 600);
    assert_eq!(state.earned_progress().held_xp(), 0);
    assert_eq!(state.earned_progress().highest_earned_tier().get(), 6);
    assert_eq!(state.earned_progress().peak_threat(), peak);
    assert!(!apply(&mut game, "restore_hit_die"));
}

#[test]
fn explicit_restoration_preserves_sixty_percent_toward_seven_without_refunding() {
    let mut game = game(state(6, false, true));
    award(&mut game, 360);
    apply(&mut game, "drain_latest_hit_die");
    assert!(apply(&mut game, "restore_hit_die"));
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .held_xp(),
        360
    );
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .spent_xp(),
        0
    );
    award(&mut game, 240);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get(),
        7
    );
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .spent_xp(),
        600
    );
    apply(&mut game, "drain_latest_hit_die");
    award(&mut game, 0);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get(),
        6
    );
}

#[test]
fn choose_again_restoration_requires_selected_source_and_keeps_empty_purchase_slot() {
    let mut original = state(3, false, false);
    let mut build = original.build().clone();
    build
        .train(
            tor_simulation::HdIndex(2),
            crate::attributes::Skill::Athletics,
        )
        .unwrap();
    original.rebuild(build).unwrap();
    let ledger = original.build().ledger().clone();
    let mut game = game(original);
    apply(&mut game, "drain_latest_hit_die");
    let before = game.clone();
    assert!(game
        .apply_effects(ActorId(1), &[effect("restore_hit_die")])
        .is_err());
    assert_eq!(game, before);
    let mut current = game.creature(ActorId(1)).unwrap().into_owned();
    let mut selected = current.build().clone();
    selected
        .set_future_growth(FutureGrowth::Selected {
            source: GrowthSource::Racial {},
        })
        .unwrap();
    current.rebuild(selected).unwrap();
    install(&mut game, current);
    assert!(apply(&mut game, "restore_hit_die"));
    let restored = game.creature(ActorId(1)).unwrap();
    assert_eq!(restored.build().ledger(), &ledger);
    assert!(restored.build().choices()[2].purchases.is_empty());
    assert_eq!(restored.build().unspent_points(), 3);
}

#[test]
fn drain_restore_preserves_injury_cooldowns_paid_balances_and_received_loss_clock() {
    let mut original = state(3, true, false);
    original.damage(1);
    original.advance_active(7);
    original
        .restore_cooldowns(vec![(
            crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            80,
        )])
        .unwrap();
    let before = super::record::CreatureRecord::capture(&original);
    let health = original.health();
    let quiet = original.received_health_loss_remaining();
    let mut game = game(original);
    apply(&mut game, "drain_latest_hit_die");
    let drained = game.creature(ActorId(1)).unwrap();
    let balances: Vec<_> = crate::resources::Resource::ALL
        .into_iter()
        .map(|resource| drained.costs().resources().balance(resource))
        .collect();
    apply(&mut game, "restore_hit_die");
    let restored = game.creature(ActorId(1)).unwrap();
    assert_eq!(restored.health(), health);
    assert_eq!(restored.received_health_loss_remaining(), quiet);
    assert_eq!(
        restored
            .cooldowns()
            .remaining(&crate::content_ids::AbilityId::new("basic_melee").unwrap()),
        80
    );
    assert_eq!(
        crate::resources::Resource::ALL
            .into_iter()
            .map(|resource| restored.costs().resources().balance(resource))
            .collect::<Vec<_>>(),
        balances
    );
    let before = serde_json::to_value(before).unwrap();
    let after = serde_json::to_value(super::record::CreatureRecord::capture(&restored)).unwrap();
    assert_eq!(before["earned_progress"], after["earned_progress"]);
    assert_eq!(before["restorable_losses"], after["restorable_losses"]);
}

#[test]
fn previously_earned_unspent_credit_retries_when_drain_removes_its_growth_obstruction() {
    let original = state(2, false, true);
    let recovery = &original.build().ruleset().document().health_recovery;
    let base_period = recovery.period(original.build().ledger().total_hd());
    assert!(recovery.period(crate::HdCount(3)) < base_period);
    let mut build = crate::test_creatures::with_talents(
        original.build().clone(),
        [crate::test_creatures::talent(
            "fixture_hd_growth_obstruction",
            vec![
                crate::test_creatures::add(
                    crate::rules::Stat::HealthRecoveryPeriod,
                    -crate::limits::NUMERIC,
                ),
                crate::test_creatures::add(
                    crate::rules::Stat::HealthRecoveryPeriod,
                    -i64::try_from(base_period).unwrap(),
                ),
            ],
        )],
    );
    build
        .select_talent(
            tor_simulation::HdIndex(1),
            crate::content_ids::TalentId::new("fixture_hd_growth_obstruction").unwrap(),
        )
        .unwrap();
    let mut funded = CreatureState::new(build).unwrap();
    funded.award_xp(200).unwrap();
    assert_eq!(funded.build().ledger().total_hd().get(), 2);
    assert_eq!(funded.earned_progress().held_xp(), 200);
    assert_eq!(funded.earned_progress().highest_earned_tier().get(), 3);
    let ledger = funded.build().ledger().clone();
    let mut game = game(funded);
    assert!(apply(&mut game, "drain_latest_hit_die"));
    let after = game.creature(ActorId(1)).unwrap();
    assert_eq!(after.build().ledger(), &ledger);
    assert!(after.build().choices()[1].purchases.is_empty());
    assert_eq!(after.earned_progress().spent_xp(), 200);
    assert_eq!(after.earned_progress().held_xp(), 0);
    assert_eq!(after.earned_progress().highest_earned_tier().get(), 3);
    apply(&mut game, "drain_latest_hit_die");
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get(),
        1
    );
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .earned_progress()
            .held_xp(),
        0
    );
}

#[test]
fn authored_mixed_source_hit_die_restores_exact_class_health_purchase_and_earlier_gate() {
    use crate::content_ids::{ClassId, TalentId};
    let original = state(3, false, false);
    let mut gate = crate::test_creatures::talent("fixture_earlier_hd_gate", vec![]);
    gate.requirement = crate::rules::Requirement::TotalHd {
        minimum: crate::HdCount(3),
    };
    let available = crate::test_creatures::with_talents(original.build().clone(), [gate]);
    let mut build = CreatureBuild::new(
        available.content().clone(),
        available.species_id().clone(),
        HdLedger::seeded(
            vec![
                HdSource::Class(ClassId::new("warrior").unwrap()),
                HdSource::Racial,
                HdSource::Class(ClassId::new("mage").unwrap()),
            ],
            42,
        )
        .unwrap(),
        available.binding(),
        *available.starting_skills(),
    )
    .unwrap();
    let talent = TalentId::new("fixture_earlier_hd_gate").unwrap();
    build
        .select_talent(tor_simulation::HdIndex(0), talent.clone())
        .unwrap();
    build
        .train(
            tor_simulation::HdIndex(2),
            crate::attributes::Skill::Athletics,
        )
        .unwrap();
    let restoration = RestorationPolicy::Authored(
        build
            .ledger()
            .entries()
            .iter()
            .zip(build.choices())
            .map(|(die, choices)| PlannedAdvancement {
                source: die.source(),
                choices: choices.clone(),
            })
            .collect(),
    );
    let original = CreatureBuild::from_recorded(
        build.content().clone(),
        build.species_id().clone(),
        build.initial_attributes(),
        build.ledger().clone(),
        build.binding(),
        BuildAdvancement {
            restoration,
            ..build.advancement().clone()
        },
        vec![],
    )
    .unwrap();
    let mut game = game(CreatureState::new(original.clone()).unwrap());
    apply(&mut game, "drain_latest_hit_die");
    let drained = game.creature(ActorId(1)).unwrap();
    assert_eq!(
        drained.build().ledger().entries(),
        &original.ledger().entries()[..2]
    );
    assert_eq!(drained.build().choices(), &original.choices()[..2]);
    assert!(drained.derived().dormant_talents.contains(&talent));
    assert!(!drained.derived().active_talents.contains(&talent));
    apply(&mut game, "restore_hit_die");
    let restored = game.creature(ActorId(1)).unwrap();
    assert_eq!(restored.build(), &original);
    assert!(restored.derived().active_talents.contains(&talent));
}
