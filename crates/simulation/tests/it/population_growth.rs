//! Exact actor birth rows and recoverable operational refusal through ordinary entry.
//! JSON uses only baseline APIs so the first red is a behavioral/schema failure.
use super::creature_fixture as fixture;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{
    checkpoint::SharedState,
    creatures::{
        record::BuildRecord, BuildAdvancement, CreatureBuild, HdChoices, PlannedAdvancement,
        RestorationPolicy,
    },
    floor_activation::{
        DepthBand, DepthInterval, PopulationContent, PopulationPlan, PopulationProfile,
        PopulationStage, PreparedActorChoice, PreparedItemChoice, PreparedScope,
        ScenarioCapabilities,
    },
    prepared_spawn::PreparedActor,
    progression::{HdLedger, HdSource},
    ActorId, CreatureIdentity, Game, ItemSpec,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn at(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    }
}
fn base_build(base: usize) -> CreatureBuild {
    let mut sources = vec![HdSource::Racial; base];
    sources[0] = HdSource::Class(tor_simulation::content_ids::ClassId::new("warrior").unwrap());
    let mut ordinary = fixture::build_with(fixture::species(), vec![], &sources);
    ordinary
        .train(0, tor_simulation::attributes::Skill::Athletics)
        .unwrap();
    CreatureBuild::from_recorded(
        ordinary.content().clone(),
        ordinary.species_id().clone(),
        ordinary.initial_attributes(),
        ordinary.ledger().clone(),
        ordinary.binding(),
        BuildAdvancement {
            starting_skills: *ordinary.starting_skills(),
            choices: ordinary.choices().to_vec(),
            restoration: RestorationPolicy::Authored(
                sources
                    .into_iter()
                    .zip(ordinary.choices())
                    .map(|(source, choices)| PlannedAdvancement {
                        source,
                        choices: choices.clone(),
                    })
                    .collect(),
            ),
        },
        ordinary.template_ids().to_vec(),
    )
    .unwrap()
}
fn actor_choice(build: &CreatureBuild) -> PreparedActorChoice {
    let actor = PreparedActor {
        build: Some(BuildRecord::capture(build)),
        nutrition: tor_simulation::nutrition::NutritionRequirement::Exempt,
        identity: Some(CreatureIdentity {
            name: "growth subject".into(),
            faction: "test".into(),
        }),
        anatomy: None,
        body: None,
        ai: None,
        known_identities: vec![],
        velocity: None,
        asset: None,
    };
    serde_json::from_value(json!({
        "content":"subject", "weight":1, "turn_ticks":100, "actor":actor,
        "depth_growth":{"kind":"racial_after", "depth":26}, "carried_loot":null
    }))
    .expect("racial-after policy must be a strict compiled actor choice")
}
fn configured(depth: u32, base: usize, destination: u64) -> (Game, CreatureBuild) {
    let build = base_build(base);
    let world = World::new(
        (1..=2)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("growth room {id}"),
                bounds: Extent::new(8, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42, build.content().clone());
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(ActorId(1)))
        .unwrap();
    game.spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let item = PopulationStage {
        id: "stock".into(),
        version: 1,
        count: [1, 1],
        population: PopulationContent::Items {
            bands: vec![DepthBand {
                depth: DepthInterval::Open { minimum: 1 },
                choices: vec![PreparedItemChoice {
                    content: "marker".into(),
                    weight: 1,
                    quantity: 1,
                    spec: ItemSpec::ordinary("marker".into()),
                }],
            }],
        },
    };
    let actor = PopulationStage {
        id: "foes".into(),
        version: 1,
        count: [1, 1],
        population: PopulationContent::Actors {
            bands: vec![DepthBand {
                depth: DepthInterval::Open { minimum: 1 },
                choices: vec![actor_choice(&build)],
            }],
            avoid_protagonist_sight: false,
        },
    };
    game.configure_population(PopulationPlan {
        carried_item_pools: BTreeMap::new(),
        protagonist: ActorId(1),
        profiles: BTreeMap::from([(
            "growth".into(),
            PopulationProfile {
                version: 1,
                stages: vec![item, actor],
            },
        )]),
    })
    .unwrap();
    game.register_activation_scope(PreparedScope {
        scope: format!("dungeon/{depth}"),
        depth,
        profile: "growth".into(),
        members: BTreeSet::from([RegionId(destination)]),
        cells: vec![at(destination, 4), at(destination, 5)],
        protected: BTreeSet::new(),
    })
    .unwrap();
    (game, build)
}
#[test]
fn population_racial_growth_keeps_authored_class_rows_and_every_ordinal_seed() {
    for (depth, count) in [(25, 7), (26, 7), (27, 8), (275, 256)] {
        let (mut game, base) = configured(depth, 7, 1);
        game.activate_initial_floor().unwrap();
        let creature = game.creature(ActorId(2)).unwrap();
        let born = creature.build();
        assert_eq!(usize::from(born.ledger().total_hd()), count);
        let root = born.ledger().health_root_seed().unwrap();
        let mut sources: Vec<_> = base
            .ledger()
            .entries()
            .iter()
            .map(|row| row.source())
            .collect();
        sources.resize(count, HdSource::Racial);
        assert_eq!(
            born.ledger(),
            &HdLedger::seeded(sources.clone(), root).unwrap()
        );
        assert_eq!(&born.choices()[..7], base.choices());
        assert!(born.choices()[7..]
            .iter()
            .all(|row| row == &HdChoices::default()));
        let RestorationPolicy::Authored(plan) = born.restoration_policy() else {
            panic!()
        };
        assert_eq!(plan.len(), count);
        assert_eq!(
            plan.iter()
                .map(|row| row.source.clone())
                .collect::<Vec<_>>(),
            sources
        );
        assert_eq!(born.future_growth(), base.future_growth());
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut restored =
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        restored.activate_initial_floor().unwrap();
        assert_eq!(
            restored, game,
            "revisit/restart must keep the exact birth rows and receipt"
        );
    }
}
#[test]
fn population_racial_capacity_refusal_restores_entry_stock_rng_and_identities() {
    for depth in [276, 999_999, u32::MAX] {
        for destination in [1, 2] {
            let (mut game, _) = configured(depth, 7, destination);
            let before = game.clone();
            for _ in 0..2 {
                let error = if destination == 1 {
                    game.activate_initial_floor()
                } else {
                    game.teleport(ActorId(1), at(2, 1))
                };
                assert!(
                    error.is_err(),
                    "oversized actor cannot be silently truncated or skipped"
                );
                assert_eq!(
                    game, before,
                    "refused entry must not leave earlier stock, IDs, time or RNG"
                );
            }
        }
    }
}

#[test]
fn bulk_racial_birth_matches_ordinary_ordinals_with_bounded_helper_compilation() {
    let base = base_build(7);
    for additional in [0, 1, 32, 249] {
        let before = tor_simulation::diagnostics::work_counts();
        let bulk = base
            .instantiate_seeded_racial(u64::MAX, additional)
            .unwrap();
        let after = tor_simulation::diagnostics::work_counts();
        // This narrow counter covers instrumented permanent compilation in the
        // helper. It neither counts every validation/derivation nor includes
        // prepared prototype restoration and actor installation boundaries.
        assert_eq!(
            after.permanent_creature_compiles - before.permanent_creature_compiles,
            1
        );
        let mut ordinary = CreatureBuild::from_recorded(
            base.content().clone(),
            base.species_id().clone(),
            base.initial_attributes(),
            base.instantiate_seeded(u64::MAX).unwrap().ledger().clone(),
            base.binding(),
            BuildAdvancement {
                starting_skills: *base.starting_skills(),
                choices: base.choices().to_vec(),
                restoration: bulk.restoration_policy().clone(),
            },
            base.template_ids().to_vec(),
        )
        .unwrap();
        ordinary
            .set_future_growth(base.future_growth().clone())
            .unwrap();
        for _ in 0..additional {
            ordinary.add_hit_die(HdSource::Racial, u64::MAX).unwrap();
        }
        assert_eq!(bulk, ordinary);
        assert_eq!(bulk.derive().unwrap(), ordinary.derive().unwrap());
    }
}

#[test]
fn racial_birth_capacity_checks_are_typed_and_never_change_the_prototype() {
    use tor_simulation::prepared_spawn::{PopulationCapacity, PreparedSpawnError};
    let base = base_build(7);
    let before = base.clone();
    assert_eq!(
        base.instantiate_seeded_racial(42, 250),
        Err(PreparedSpawnError::Capacity(PopulationCapacity::HitDice {
            requested: 257,
            maximum: 256
        }))
    );
    assert_eq!(
        base.instantiate_seeded_racial(42, u64::MAX),
        Err(PreparedSpawnError::Capacity(PopulationCapacity::Arithmetic))
    );
    assert_eq!(base, before);
    let mut species = fixture::species();
    species.modifiers = vec![fixture::add(tor_simulation::rules::Stat::Health, 999_990)];
    let base = fixture::build_with(species, vec![], &[HdSource::Racial]);
    let before = base.clone();
    assert_eq!(
        base.instantiate_seeded_racial(42, 32),
        Err(PreparedSpawnError::Capacity(
            PopulationCapacity::DerivedSheet
        ))
    );
    assert_eq!(base, before);
}

#[test]
fn racial_policy_rejects_a_drained_authored_prototype_without_overwriting_future_rows() {
    use tor_simulation::prepared_spawn::DepthGrowth;
    let mut base = base_build(7);
    base.remove_latest().unwrap();
    let before = base.clone();
    assert_eq!(
        DepthGrowth::RacialAfter { depth: 26 }.validate_build(Some(&base)),
        Err(tor_simulation::creatures::BuildError::InvalidOwnership)
    );
    assert!(base.instantiate_seeded_racial(42, 1).is_err());
    assert_eq!(base, before);
    // Fixed birth can still represent the retained exact recipe faithfully.
    let fixed = base.instantiate_seeded(42).unwrap();
    assert_eq!(fixed.restoration_policy(), before.restoration_policy());
}

#[test]
fn grown_authored_and_choose_again_rows_keep_drain_restoration_and_applied_watermarks() {
    use tor_simulation::{
        creatures::CreatureState,
        earned_growth::{FutureGrowth, GrowthSource},
    };
    for authored in [true, false] {
        let mut base = if authored {
            base_build(7)
        } else {
            fixture::build_with(fixture::species(), vec![], &vec![HdSource::Racial; 7])
        };
        if !authored {
            base.set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Racial {},
            })
            .unwrap();
        }
        let grown = base.instantiate_seeded_racial(42, 3).unwrap();
        let mut creature = CreatureState::new(grown.clone()).unwrap();
        let before_progress = creature.earned_progress().clone();
        assert_eq!(before_progress.highest_applied_tier(), 10);
        creature.drain_latest_hit_die().unwrap();
        assert_eq!(creature.build().ledger().total_hd(), 9);
        assert_eq!(creature.earned_progress(), &before_progress);
        creature.restore_hit_die().unwrap();
        assert_eq!(creature.build(), &grown);
        assert_eq!(creature.earned_progress(), &before_progress);
        assert_eq!(creature.restorable_hit_dice(), 0);
    }
}

#[test]
fn racial_birth_receipt_cannot_claim_a_success_above_the_operational_row_capacity() {
    let (mut game, _) = configured(27, 7, 1);
    game.activate_initial_floor().unwrap();
    let mut record = game.activations_since(0).unwrap().remove(0);
    game.validate_activation_record(&record).unwrap();
    record.scope = "dungeon/276".into();
    assert!(
        game.validate_activation_record(&record).is_err(),
        "a retained success cannot claim a birth that would require truncated rows"
    );
}

#[test]
fn compiled_depth_growth_requires_its_explicit_strict_policy() {
    let choice = actor_choice(&base_build(7));
    let good = serde_json::to_value(&choice).unwrap();
    let mut missing = good.clone();
    missing.as_object_mut().unwrap().remove("depth_growth");
    assert!(serde_json::from_value::<PreparedActorChoice>(missing).is_err());
    for policy in [
        json!({"kind":"capped", "maximum":256}),
        json!({"kind":"racial_after"}),
        json!({"kind":"fixed", "depth":26}),
    ] {
        let mut invalid = good.clone();
        invalid["depth_growth"] = policy;
        assert!(serde_json::from_value::<PreparedActorChoice>(invalid).is_err());
    }
}
