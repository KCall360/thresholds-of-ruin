use super::*;
use crate::{EquipmentSlotId, EquipmentSpec, ItemSpec};
use std::num::NonZeroU64;
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn fixture() -> (Game, ActorId) {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.conditions.push(serde_json::from_value(serde_json::json!({
        "id":"cadence_sleep","family":"cadence_sleep","name":"Asleep","lifecycle":"timed",
        "modifiers":[],"stacking":{"kind":"extend"},"source_scoped":false,"descriptors":[],"capacity":"reject",
        "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},"restrictions":["voluntary_actions"],"preparation":"pause"
    })).unwrap());
    let species = crate::test_creatures::species();
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "room".into(),
            bounds: Extent::new(6, 4, 2).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 43, content.clone());
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content,
        species_id,
        crate::progression::HdLedger::new(vec![crate::progression::HitDie::new(
            crate::progression::HdSource::Racial,
            42,
        )])
        .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            name: "wearer".into(),
            faction: "test".into(),
        },
        build,
    )
    .unwrap();
    (game, actor)
}
fn wear(game: &mut Game, actor: ActorId, id: u64, ordinal: usize) -> ItemId {
    let mut spec = ItemSpec::ordinary(format!("source-{id}"));
    spec.class = crate::ItemClass::Ring;
    spec.equipment = Some(EquipmentSpec {
        periodic_relocation: Some(PeriodicRelocation {
            interval: 100,
            chance_denominator: 50,
        }),
        slot: crate::EquipmentSlot::new("ring").unwrap(),
        armor: None,
        attack: None,
        weapon_profile: None,
        modifiers: vec![],
        potency_modifiers: vec![],
    });
    let item = game
        .place_item_stack(id, game.actors[&actor].location, Some(actor), 1, spec)
        .unwrap();
    assert_eq!(game.items[&item].relocation_elapsed, Some(0));
    let socket = game
        .anatomy(actor)
        .unwrap()
        .slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| slot.as_str() == "ring")
        .nth(ordinal)
        .unwrap()
        .0;
    game.equip_authored(actor, item, EquipmentSlotId(socket as u16))
        .unwrap();
    assert_eq!(game.items[&item].relocation_elapsed, None);
    item
}

#[test]
fn two_exact_worn_sources_can_both_proc_in_source_order_at_one_deadline() {
    let (mut game, actor) = fixture();
    wear(&mut game, actor, 71, 0);
    wear(&mut game, actor, 72, 1);
    let candidates = game.random_relocation_candidates(actor).unwrap().len() as u64;
    assert!(candidates > 1);
    let trial = crate::quantity::QuantityRange::new(1, 50).unwrap();
    let destination = crate::quantity::QuantityRange::new(1, candidates).unwrap();
    let seed = (1..100_000)
        .find(|seed| {
            let mut rng = *seed;
            trial.sample(&mut rng).unwrap() == 1 && {
                destination.sample(&mut rng).unwrap();
                trial.sample(&mut rng).unwrap() == 1
            }
        })
        .expect("bounded deterministic two-hit seed");
    game.rules_rng = seed;
    game.advance_physics(99).unwrap();
    assert_eq!(game.rules_rng, seed, "neither source may fire early");
    let mut expected = game.clone();
    let mut rng = seed;
    assert_eq!(trial.sample(&mut rng).unwrap(), 1);
    expected.rules_rng = rng;
    let first = expected
        .relocate_random_in_transaction(actor, &mut rng, false)
        .unwrap()
        .unwrap();
    assert_eq!(trial.sample(&mut rng).unwrap(), 1);
    expected.rules_rng = rng;
    let second = expected
        .relocate_random_in_transaction(actor, &mut rng, false)
        .unwrap()
        .unwrap();
    assert_ne!(
        first, second,
        "the second source relocates from the first result"
    );
    game.advance_physics(100).unwrap();
    assert_eq!(game.actors[&actor].location, second.location);
    assert_eq!(
        game.rules_rng, rng,
        "both independent trials and both placements are committed"
    );
    assert!(!game.actors[&actor].periodic_relocations.pending());
}

#[test]
fn sleeping_counts_active_ticks_but_queries_and_source_free_restore_never_pulse() {
    let (mut game, actor) = fixture();
    wear(&mut game, actor, 71, 0);
    game.advance_physics(25).unwrap();
    game.apply_effects(
        actor,
        &[crate::rules::Effect::ApplyCondition {
            condition: crate::content_ids::ConditionId::new("cadence_sleep").unwrap(),
            duration: 200,
        }],
    )
    .unwrap();
    assert!(!game.condition_allows_voluntary_input(actor));
    let rng = game.rules_rng;
    game.advance_physics(99).unwrap();
    assert_eq!(game.rules_rng, rng);
    let before = game.clone();
    game.observe(actor).unwrap();
    assert_eq!(game, before);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, game);
    game.advance_physics(100).unwrap();
    restored.advance_physics(100).unwrap();
    assert_ne!(
        game.rules_rng, rng,
        "sleep does not suppress the independent trial"
    );
    assert_eq!(game, restored);
    assert!(!game.condition_allows_voluntary_input(actor));
}

#[test]
fn frozen_partial_interval_is_retained_without_ambient_catchup() {
    let (mut game, actor) = fixture();
    wear(&mut game, actor, 71, 0);
    // Streaming requires an ordinary decision boundary, not a physics-only
    // jump past this actor's still-unpaid ready_at tick.
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(30).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 30);
    assert_eq!(game.actors[&actor].ready_at, game.tick());
    let mut records = crate::streaming::MemoryRecords::default();
    game.apply_region_transition(
        &crate::streaming::RegionTransition {
            active: Default::default(),
            loaded: [RegionId(1)].into(),
        },
        &mut records,
    )
    .unwrap();
    let rng = game.rules_rng;
    game.advance_physics(1000).unwrap();
    assert_eq!(game.rules_rng, rng);
    game.apply_region_transition(
        &crate::streaming::RegionTransition {
            active: [RegionId(1)].into(),
            loaded: [RegionId(1)].into(),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(game.rules_rng, rng, "thaw is not a proc");
    game.advance_physics(1069).unwrap();
    assert_eq!(game.rules_rng, rng);
    game.advance_physics(1070).unwrap();
    assert_ne!(game.rules_rng, rng);
}

#[test]
fn unequipped_fraction_changes_owner_once_and_survives_restore_then_reequip() {
    let (mut game, actor) = fixture();
    let item = wear(&mut game, actor, 71, 0);
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(25).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    game.act(actor, crate::Action::Unequip { item }).unwrap();
    let fraction = game.items[&item]
        .relocation_elapsed
        .expect("unequipped item owns its fraction");
    assert!(fraction > 0 && fraction < 100);
    assert!(game.actors[&actor].periodic_relocations.sources.is_empty());
    let rng = game.rules_rng;
    for _ in 0..12 {
        game.act(actor, crate::Action::Wait).unwrap();
    }
    assert_eq!(game.rules_rng, rng);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    game = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(game.items[&item].relocation_elapsed, Some(fraction));
    let slot = game
        .anatomy(actor)
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.as_str() == "ring")
        .unwrap();
    game.equip_authored(actor, item, EquipmentSlotId(slot as u16))
        .unwrap();
    assert_eq!(game.items[&item].relocation_elapsed, None);
    assert_eq!(game.rules_rng, rng);
    let next = game.tick() + 100 - fraction;
    game.advance_physics(next - 1).unwrap();
    assert_eq!(game.rules_rng, rng);
    game.advance_physics(next).unwrap();
    assert_ne!(game.rules_rng, rng);
}

#[test]
fn saved_source_identity_policy_and_exclusive_fraction_ownership_are_checked() {
    let (mut game, actor) = fixture();
    let item = wear(&mut game, actor, 71, 0);
    // Prove the positive save is at an admitted scheduler boundary before
    // independently corrupting exact-source identity, policy, or ownership.
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(30).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 30);
    assert_eq!(game.actors[&actor].ready_at, game.tick());
    let mut shared = crate::checkpoint::SharedState::default();
    let positive = game.checkpoint(&mut shared);
    assert_eq!(
        Game::restore_checkpoint(positive, &shared, game.content().clone()).unwrap(),
        game
    );
    let mutations: [fn(&mut Game, ActorId, ItemId); 4] = [
        |game: &mut Game, actor, _| {
            game.actors
                .get_mut(&actor)
                .unwrap()
                .periodic_relocations
                .sources[0]
                .policy
                .interval = 200;
        },
        |game: &mut Game, actor, _| {
            game.actors
                .get_mut(&actor)
                .unwrap()
                .periodic_relocations
                .sources[0]
                .item = ItemId(999);
        },
        |game: &mut Game, _, item| {
            game.items
                .edit(item, |item| item.relocation_elapsed = Some(30));
        },
        |game: &mut Game, actor, _| {
            let mut state = game.actors.get_mut(&actor).unwrap();
            let book = &mut state.periodic_relocations;
            book.pending.push(PendingTrial {
                item: ItemId(71),
                policy: book.sources[0].policy,
            });
        },
    ];
    for mutate in mutations {
        let mut malformed = game.clone();
        mutate(&mut malformed, actor, item);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = malformed.checkpoint(&mut shared);
        assert!(Game::restore_checkpoint(snapshot, &shared, malformed.content().clone()).is_err());
    }
}

#[test]
fn earned_trial_survives_source_removal_at_its_publication_boundary() {
    let (mut game, actor) = fixture();
    let item = wear(&mut game, actor, 71, 0);
    game.advance_physics(99).unwrap();
    let trial = crate::quantity::QuantityRange::new(1, 50).unwrap();
    let seed = (1..100)
        .find(|seed| {
            let mut rng = *seed;
            trial.sample(&mut rng).unwrap() != 1
        })
        .unwrap();
    game.rules_rng = seed;
    // Exercise lifecycle's exact-boundary candidate, not a timer shortcut:
    // the old source earns its trial before equipment removal is published.
    game.tick = 100;
    let mut outcome = game.settled_actor_outcome(actor).unwrap();
    outcome.state.equipment.retain(|_, owned| *owned != item);
    game.apply_outcome(outcome).unwrap();
    assert!(game.actors[&actor].periodic_relocations.sources.is_empty());
    assert!(game.actors[&actor].periodic_relocations.pending());
    assert_eq!(game.items[&item].relocation_elapsed, Some(0));
    assert_eq!(
        game.rules_rng, seed,
        "projection/publication do not draw early"
    );
    let mut expected = seed;
    assert_ne!(trial.sample(&mut expected).unwrap(), 1);
    game.settle_periodic_relocations().unwrap();
    assert_eq!(
        game.rules_rng, expected,
        "removing a source cannot erase its earned draw"
    );
    game.settle_periodic_relocations().unwrap();
    assert_eq!(
        game.rules_rng, expected,
        "earned trial retires exactly once"
    );
}

#[test]
fn derived_pending_owner_index_tracks_edit_clone_rebuild_remove_and_insert() {
    let (mut game, actor) = fixture();
    wear(&mut game, actor, 71, 0);
    assert_eq!(game.actors.pending_relocations().collect::<Vec<_>>(), []);
    game.actors
        .get_mut(&actor)
        .unwrap()
        .periodic_relocations
        .advance(100)
        .unwrap();
    assert_eq!(
        game.actors.pending_relocations().collect::<Vec<_>>(),
        [actor]
    );
    let mut cloned = game.actors.clone();
    let rebuilt = crate::actor_store::ActorStore::from_entries(
        game.actors
            .iter()
            .map(|(id, state)| (*id, state.clone()))
            .collect(),
    );
    assert_eq!(cloned.pending_relocations().collect::<Vec<_>>(), [actor]);
    assert_eq!(rebuilt.pending_relocations().collect::<Vec<_>>(), [actor]);
    let state = cloned.remove(&actor).unwrap();
    assert_eq!(cloned.pending_relocations().collect::<Vec<_>>(), []);
    cloned.insert(actor, state);
    assert_eq!(cloned.pending_relocations().collect::<Vec<_>>(), [actor]);
    cloned
        .get_mut(&actor)
        .unwrap()
        .periodic_relocations
        .pending
        .clear();
    assert_eq!(cloned.pending_relocations().collect::<Vec<_>>(), []);
    assert_eq!(
        game.actors.pending_relocations().collect::<Vec<_>>(),
        [actor],
        "candidate edits cannot consume the authoritative owner's trial"
    );
}

#[test]
fn detached_exact_sources_restore_strictly_without_time_or_query_catchup() {
    use crate::streaming::RecordStore;
    let (mut game, actor) = fixture();
    let item = wear(&mut game, actor, 71, 0);
    let unworn_spec = (*game.items[&item].spec).clone();
    game.place_item_stack(
        72,
        game.actors[&actor].location,
        Some(actor),
        1,
        unworn_spec,
    )
    .unwrap();
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(50).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 50);
    let rng = game.rules_rng;
    let mut records = crate::streaming::MemoryRecords::default();
    game.apply_region_transition(&crate::streaming::RegionTransition::default(), &mut records)
        .unwrap();
    assert!(!game.actors.contains_key(&actor));
    let mut reopened_records = crate::streaming::MemoryRecords::default();
    let record_ids: Vec<_> = records.ids().collect();
    assert_eq!(record_ids.len(), 1);
    for id in record_ids {
        let record = records.get(id).unwrap();
        let data = crate::streaming::RegionRecordData::capture(&record);
        let positive = serde_json::to_value(&data).unwrap();
        let decoded: crate::streaming::RegionRecordData =
            serde_json::from_value(positive.clone()).unwrap();
        let restored = decoded.restore(game.content().clone()).unwrap();
        assert_eq!(
            restored, *record,
            "valid detached state is the negative-test baseline"
        );
        let mutations: [fn(&mut serde_json::Value); 3] = [
            |value| value["items"]["71"]["relocation_elapsed"] = 0.into(),
            |value| value["items"]["72"]["relocation_elapsed"] = serde_json::Value::Null,
            |value| value["items"]["72"]["relocation_elapsed"] = 100.into(),
        ];
        for mutate in mutations {
            let mut malformed = positive.clone();
            mutate(&mut malformed);
            let decoded: crate::streaming::RegionRecordData =
                serde_json::from_value(malformed).unwrap();
            assert!(decoded.restore(game.content().clone()).is_err());
        }
        reopened_records.put(id, tor_world::Shared::new(restored));
    }
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    restored.advance_physics(500).unwrap();
    restored
        .apply_region_transition(
            &crate::streaming::RegionTransition {
                active: [RegionId(1)].into(),
                loaded: [RegionId(1)].into(),
            },
            &mut reopened_records,
        )
        .unwrap();
    assert_eq!(
        restored.rules_rng, rng,
        "source-free reattach cannot pulse or accrue remote time"
    );
    assert_eq!(restored.items[&ItemId(72)].relocation_elapsed, Some(0));
    let before = restored.clone();
    restored.observe(actor).unwrap();
    assert_eq!(restored, before);
    restored.advance_physics(549).unwrap();
    assert_eq!(restored.rules_rng, rng);
    restored.advance_physics(550).unwrap();
    assert_ne!(restored.rules_rng, rng);
}
