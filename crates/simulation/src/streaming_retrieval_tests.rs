use crate::floor_activation::{
    PopulationPlan, PopulationProfile, PreparedScope, ScenarioCapabilities,
};
use crate::unique_retrieval::{TerminalExit, UniqueRetrievalPolicy, UniqueRetrievalState};

fn retrieval_at(region: u64, x: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y: 1, z: 0 },
    }
}

/// Three unrelated two-member floors. The remote source and its possible
/// dependency have their own members which must never become load roots.
fn retrieval_residency_fixture(
    carried: bool,
    claims: bool,
) -> (Game, MemoryRecords, ItemId, Option<(ActorId, Actor)>) {
    retrieval_residency_fixture_with_destination_member(carried, claims, 3)
}

fn retrieval_residency_fixture_with_destination_member(
    carried: bool,
    claims: bool,
    destination_member: u64,
) -> (Game, MemoryRecords, ItemId, Option<(ActorId, Actor)>) {
    let mut game = Game::region_corridor(42, 12, crate::test_creatures::content());
    let player = game
        .spawn_actor(retrieval_at(1, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_scenario_policy(ScenarioCapabilities::default(), Some(player))
        .unwrap();
    game.configure_population(PopulationPlan {
        protagonist: player,
        carried_item_pools: BTreeMap::new(),
        profiles: BTreeMap::from([(
            "plain".into(),
            PopulationProfile {
                version: 1,
                stages: vec![],
            },
        )]),
    })
    .unwrap();
    for (depth, members) in [(26, [1, destination_member]), (4, [8, 10]), (5, [9, 11])] {
        game.register_activation_scope(PreparedScope {
            scope: format!("dungeon/{depth}"),
            depth,
            profile: "plain".into(),
            members: members.into_iter().map(RegionId).collect(),
            cells: members
                .into_iter()
                .map(|region| retrieval_at(region, 5))
                .collect(),
            protected: BTreeSet::new(),
        })
        .unwrap();
    }
    let carrier = carried.then(|| {
        let actor = game
            .spawn_actor(retrieval_at(8, 11), NonZeroU64::new(100).unwrap())
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                eye: [0, 0, 0],
                mass: 80,
            },
        )
        .unwrap();
        game.configure_ai(
            actor,
            crate::ai::AiProfile {
                care: true,
                equipment: true,
                loot: true,
                flee_percent: 25,
                idle: crate::behavior::IdleBehavior::Wait,
        doors: false,
},
        )
        .unwrap();
        actor
    });
    let mut spec = crate::ItemSpec::ordinary("Amulet of Yendor".into());
    spec.class = crate::ItemClass::Amulet;
    let item = game
        .place_item_stack(100, retrieval_at(8, 5), carrier, 1, spec.clone())
        .unwrap();
    if claims {
        let owner = game
            .spawn_actor(retrieval_at(9, 0), NonZeroU64::new(100).unwrap())
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            owner,
            "neutral",
            crate::test_creatures::species(),
        );
        let mut bandage = crate::ItemSpec::ordinary("bandage".into());
        bandage.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
        let supply = game
            .place_item_stack(101, retrieval_at(8, 11), None, 1, bandage)
            .unwrap();
        // The selected character otherwise owns the first zero-tick input
        // boundary, so let the remote claimant start this preparation first.
        game.actors.get_mut(&player).unwrap().ready_at = 100;
        game.admit_intention(
            owner,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                invocation: crate::abilities::AbilityInvocation::ActorTarget { target: owner },
                supplies: crate::supplies::SupplySelection::Explicit {
                    items: vec![crate::supplies::SelectedSupply {
                        cost_ordinal: 0,
                        item: supply,
                    }],
                },
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
    }
    let mut records = MemoryRecords::default();
    game.transition_regions_counted_deferred(&sets(&[1], &[1]), &mut records)
        .unwrap();
    let frozen_carrier = carrier.map(|id| {
        let record_id = game.lifecycle.detached[&RegionId(8)];
        (id, records.0[&record_id].actors[&id].clone())
    });
    // This fixture starts after a legitimate artifact birth: source residency
    // consumes its exact bound identity, not historical entry evidence.
    game.unique_retrieval = Some(UniqueRetrievalState {
        policy: Shared::new(UniqueRetrievalPolicy {
            version: 1,
            protagonist: player,
            family: "dungeon".into(),
            minimum_depth: 26,
            item: spec,
            exit: TerminalExit {
                at: retrieval_at(1, 2),
                direction: Direction::Up,
            },
        }),
        initialized: true,
        item: Some(item),
        entry_count: 1,
        placement_count: 1,
        entries: vec![],
    });
    (game, records, item, frozen_carrier)
}

#[test]
fn retrieval_ground_source_is_load_only_without_remote_floor_expansion() {
    retrieval_source_is_bounded(false);
}

#[test]
fn retrieval_carried_source_keeps_body_dependency_frozen_without_remote_floor_expansion() {
    retrieval_source_is_bounded(true);
}

fn retrieval_source_is_bounded(carried: bool) {
    let (mut game, mut records, item, frozen_carrier) = retrieval_residency_fixture(carried, false);
    let frontier = sets(&[1], &[1]);
    let population = game.floor_activation.clone();
    let objective = game.unique_retrieval.clone();
    let initial_records = records.len();
    let next_item = game.next_item_id;
    game.tick = 400;
    let planned = game.settle_region_transition(&frontier);
    assert!(
        planned.loaded.contains(&RegionId(8)),
        "exact detached source must be pinned"
    );
    assert_eq!(planned.active, BTreeSet::from([RegionId(1)]));
    assert!(
        !planned.loaded.contains(&RegionId(10)),
        "source floor is not an entry frontier"
    );
    let (settled, report, _) = game
        .transition_regions_counted_deferred(&frontier, &mut records)
        .unwrap();
    let expected = if carried {
        vec![1, 2, 3, 8, 9]
    } else {
        vec![1, 2, 3, 8]
    };
    assert_eq!(settled.loaded, expected.into_iter().map(RegionId).collect());
    assert_eq!(settled.active, BTreeSet::from([RegionId(1)]));
    assert!(report.thawed.is_empty());
    assert!(report.built.is_empty());
    assert_eq!(records.len(), initial_records);
    assert_eq!(
        game.floor_activation, population,
        "source acquisition never populates a floor"
    );
    assert_eq!(game.unique_retrieval, objective, "preload is not an entry");
    assert_eq!(game.next_item_id, next_item);
    assert_eq!(game.tick(), 400);
    assert_eq!(game.region_state(RegionId(8)), Some(RegionState::Frozen));
    assert_eq!(game.region_state(RegionId(10)), Some(RegionState::Detached));
    assert_eq!(game.region_state(RegionId(11)), Some(RegionState::Detached));
    if let Some((id, actor)) = frozen_carrier {
        assert_eq!(
            game.actors[&id], actor,
            "frozen carrier receives no AI or recovery catch-up"
        );
        assert_eq!(game.actor_clock(id), 0);
        assert!(game.combat.ai.contains_key(&id));
        assert_eq!(game.region_state(RegionId(9)), Some(RegionState::Frozen));
    }
    for _ in 0..3 {
        assert_eq!(game.settle_region_transition(&frontier), settled);
        let (_, report, _) = game
            .transition_regions_counted_deferred(&frontier, &mut records)
            .unwrap();
        assert!(report.is_empty());
    }
    game.items
        .edit(item, |entry| {
            entry.location = ItemLocation::Ground(retrieval_at(1, 5))
        })
        .unwrap();
    let (released, report, _) = game
        .transition_regions_counted_deferred(&frontier, &mut records)
        .unwrap();
    assert_eq!(
        released.loaded,
        [1, 2, 3].into_iter().map(RegionId).collect()
    );
    assert!(report.detached.contains(&RegionId(8)));
    assert_eq!(game.retrieval_source_region(), Some(RegionId(1)));
    assert_eq!(
        game.items[&item].location,
        ItemLocation::Ground(retrieval_at(1, 5))
    );
}

#[test]
fn retrieval_source_post_attach_claims_stay_load_only_and_do_not_expand_either_floor() {
    let (mut game, mut records, _, _) = retrieval_residency_fixture(false, true);
    let frontier = sets(&[1], &[1]);
    let before = game.floor_activation.clone();
    let (settled, _, _) = game
        .transition_regions_counted_deferred(&frontier, &mut records)
        .unwrap();
    assert_eq!(settled.active, BTreeSet::from([RegionId(1)]));
    assert_eq!(
        settled.loaded,
        [1, 2, 3, 8, 9].into_iter().map(RegionId).collect()
    );
    assert_eq!(game.region_state(RegionId(8)), Some(RegionState::Frozen));
    assert_eq!(game.region_state(RegionId(9)), Some(RegionState::Frozen));
    assert!(game.item_claims_valid(false));
    assert_eq!(game.floor_activation, before);
    assert_eq!(game.settle_region_transition(&frontier), settled);
}

#[test]
fn retrieval_corrupt_detached_source_rejects_without_state_or_store_changes() {
    let (mut game, mut records, item, _) = retrieval_residency_fixture(false, false);
    let source = game.lifecycle.detached[&RegionId(8)];
    records.0.get_mut(&source).unwrap().items.remove(&item);
    let before = game.clone();
    let stored = records.0.clone();
    assert!(game
        .transition_regions_counted_deferred(&sets(&[1], &[1]), &mut records)
        .is_err());
    assert_eq!(game, before);
    assert_eq!(records.0, stored);
}

#[test]
fn retrieval_eligible_revisit_completes_only_the_ordinary_destination_floor() {
    // Region 3 is within this corridor's conservative sight-read horizon.
    // Choose member 6 so only eligible floor completion can keep it loaded.
    let (mut game, mut records, _, _) =
        retrieval_residency_fixture_with_destination_member(false, false, 6);
    let objective = game.unique_retrieval.take();
    game.activate_initial_floor().unwrap();
    assert_eq!(game.activation_count(), 1);
    let (ordinary, _, _) = game
        .transition_regions_counted_deferred(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert_eq!(game.region_state(RegionId(6)), Some(RegionState::Detached));
    assert!(!ordinary.loaded.contains(&RegionId(6)));
    assert!(!ordinary.loaded.contains(&RegionId(8)));
    game.unique_retrieval = objective;
    let population = game.floor_activation.clone();
    let (settled, report, _) = game
        .transition_regions_counted_deferred(&sets(&[1], &[1]), &mut records)
        .unwrap();
    let mut expected = ordinary.loaded;
    expected.extend([RegionId(6), RegionId(8)]);
    assert_eq!(settled.active, BTreeSet::from([RegionId(1)]));
    assert_eq!(settled.loaded, expected);
    assert!(report.attached.contains(&RegionId(6)));
    assert!(report.attached.contains(&RegionId(8)));
    assert!(!settled.loaded.contains(&RegionId(10)));
    assert!(!settled.loaded.contains(&RegionId(11)));
    assert_eq!(
        game.floor_activation, population,
        "revisit residency preserves population receipts"
    );
    assert_eq!(game.region_state(RegionId(6)), Some(RegionState::Frozen));
    assert_eq!(game.region_state(RegionId(10)), Some(RegionState::Detached));
}
