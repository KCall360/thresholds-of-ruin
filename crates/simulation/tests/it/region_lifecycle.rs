//! Region lifecycle: reference points, pins, frozen time without catch-up,
//! and detached region records that reattach exactly.
use std::{
    collections::{BTreeMap, BTreeSet},
    num::NonZeroU64,
};
use tor_simulation::{
    ai::AiProfile,
    checkpoint::SharedState,
    combat::{CombatSpec, Objective},
    Action, ActorId, BodySpec, Game, ItemId, MemoryRecords, ReferencePoint, ReferenceTarget,
    RegionState, RegionTransition, TransitionError,
};
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}

fn regions(ids: &[u64]) -> BTreeSet<RegionId> {
    ids.iter().map(|id| RegionId(*id)).collect()
}

fn sets(active: &[u64], loaded: &[u64]) -> RegionTransition {
    RegionTransition {
        active: regions(active),
        loaded: regions(loaded),
    }
}

fn ticks(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).unwrap()
}

fn fighter(game: &mut Game, id: ActorId, faction: &str) {
    let mut spec = CombatSpec {
        faction: faction.into(),
        ..Default::default()
    };
    spec.attack.bonus = 100;
    spec.attack.wind_up = 30;
    spec.attack.recovery = 40;
    game.configure_combat(id, spec).unwrap();
}

/// A four-region corridor with the character in region 1.
fn corridor() -> (Game, ActorId) {
    let mut game = Game::region_corridor(7, 4);
    let player = game.spawn_actor(at(1, 2, 1), ticks(100)).unwrap();
    fighter(&mut game, player, "player");
    (game, player)
}

fn run(game: &mut Game, player: ActorId, hostility: &[(&str, &str)]) {
    let hostility = hostility.iter().fold(
        BTreeMap::<String, BTreeSet<String>>::new(),
        |mut map, (a, b)| {
            map.entry((*a).into()).or_default().insert((*b).into());
            map
        },
    );
    game.configure_run(player, BTreeSet::from([player]), None, hostility)
        .unwrap();
    game.refresh_navigation();
    game.add_default_reference_points().unwrap();
}

/// Take `n` actions: AI actors choose theirs, everyone else waits.
fn step(game: &mut Game, n: usize) {
    for _ in 0..n {
        let (id, action) = game
            .next_ai_action()
            .unwrap_or_else(|| (game.next_actor().expect("someone acts"), Action::Wait));
        game.act(id, action).unwrap();
    }
}

fn round_trip(game: &Game) -> Game {
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot = serde_json::from_value(serde_json::to_value(&snapshot).unwrap()).unwrap();
    let shared = serde_json::from_value(serde_json::to_value(&shared).unwrap()).unwrap();
    Game::restore_checkpoint(snapshot, &shared).expect("valid checkpoint")
}

#[test]
fn keeping_everything_active_changes_nothing() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    run(&mut game, player, &[]);
    let before = game.clone();
    let all = [1, 2, 3, 4];
    let (_, report) = game
        .transition_regions(&sets(&all, &all), &mut records)
        .unwrap();
    assert_eq!(report, Default::default());
    assert_eq!(game, before);
}

#[test]
fn default_points_follow_characters_and_nothing_else() {
    let (mut game, player) = corridor();
    let other = game.spawn_actor(at(3, 5, 1), ticks(100)).unwrap();
    run(&mut game, player, &[]);
    let roots = game.region_roots();
    assert_eq!(roots.len(), 1);
    assert_eq!(roots[0].region, RegionId(1));
    assert!(roots[0].observes);
    // Adding defaults again adds nothing; any rule may add more points.
    assert!(game.add_default_reference_points().unwrap().is_empty());
    let point = game
        .add_reference_point(ReferencePoint {
            target: ReferenceTarget::Actor(other),
            active_radius: Some(0),
            load_radius: None,
            observes: false,
        })
        .unwrap();
    assert_eq!(game.region_roots().len(), 2);
    assert!(game.remove_reference_point(point));
    assert!(game
        .add_reference_point(ReferencePoint {
            target: ReferenceTarget::Location(at(9, 0, 0)),
            active_radius: None,
            load_radius: None,
            observes: false,
        })
        .is_err());
}

#[test]
fn frozen_actors_do_not_act_and_keep_their_remaining_recovery() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let far = game.spawn_actor(at(3, 5, 1), ticks(150)).unwrap();
    run(&mut game, player, &[]);
    game.act(player, Action::Wait).unwrap();
    let outcome = game.act(far, Action::Wait).unwrap();
    // The far actor still has 50 ticks of recovery at tick 100.
    assert_eq!((outcome.next_actor, outcome.next_tick), (Some(player), 100));

    let (applied, report) = game
        .transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert_eq!(applied.active, regions(&[1]));
    assert_eq!(
        applied.loaded,
        regions(&[1, 2]),
        "linked regions stay loaded"
    );
    // Every region leaving the active set freezes; unloaded ones then detach.
    assert_eq!(report.frozen, [RegionId(2), RegionId(3), RegionId(4)]);
    assert_eq!(report.detached, [RegionId(3), RegionId(4)]);
    assert_eq!(game.region_state(RegionId(3)), Some(RegionState::Detached));

    for _ in 0..3 {
        let outcome = game.act(player, Action::Wait).unwrap();
        assert_eq!(outcome.next_actor, Some(player));
    }
    assert_eq!(game.tick(), 400);

    game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
        .unwrap();
    // Without the shift the far actor would act at once; it has 50 left.
    let outcome = game.act(player, Action::Wait).unwrap();
    assert_eq!((outcome.next_actor, outcome.next_tick), (Some(far), 450));
}

#[test]
fn frozen_wind_up_resumes_where_it_stopped() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let attacker = game.spawn_actor(at(3, 5, 1), ticks(100)).unwrap();
    let target = game.spawn_actor(at(3, 6, 1), ticks(100)).unwrap();
    fighter(&mut game, attacker, "a");
    fighter(&mut game, target, "b");
    run(&mut game, player, &[("a", "b")]);
    game.act(player, Action::Wait).unwrap();
    game.act(attacker, Action::Attack { target }).unwrap();
    let started = game.preparation(attacker).unwrap().clone();
    assert!(started.active);

    // The target is due next, so freezing it advances time to the player.
    let frozen_at = game.tick();
    game.transition_regions(&sets(&[1], &[1, 2, 3]), &mut records)
        .unwrap();
    assert_eq!(game.region_state(RegionId(3)), Some(RegionState::Frozen));
    assert_eq!(game.next_actor(), Some(player));
    game.act(player, Action::Wait).unwrap();
    game.act(player, Action::Wait).unwrap();
    let elapsed = game.tick() - frozen_at;
    assert_eq!(
        game.preparation(attacker),
        Some(&started),
        "no progress while frozen"
    );

    game.transition_regions(&sets(&[1, 2, 3], &[1, 2, 3, 4]), &mut records)
        .unwrap();
    let resumed = game.preparation(attacker).unwrap();
    assert_eq!(resumed.started, started.started + elapsed);
    assert_eq!(resumed.remaining, started.remaining);
}

#[test]
fn queued_native_attack_survives_frozen_and_detached_checkpoint_boundaries() {
    for detach in [false, true] {
        let mut records = MemoryRecords::default();
        let (mut game, player) = corridor();
        let attacker = game.spawn_actor(at(3, 5, 1), ticks(100)).unwrap();
        let target = game.spawn_actor(at(3, 6, 1), ticks(100)).unwrap();
        fighter(&mut game, attacker, "a");
        fighter(&mut game, target, "b");
        run(&mut game, player, &[("a", "b")]);
        let intention = game
            .admit_intention(
                attacker,
                Action::Attack { target },
                tor_simulation::IntentionOrigin::Human,
            )
            .unwrap();
        let queued = game.pending_intention(attacker).unwrap().clone();
        let loaded = if detach { vec![1, 2] } else { vec![1, 2, 3] };
        game.transition_regions(&sets(&[1], &loaded), &mut records)
            .unwrap();
        assert_eq!(
            game.region_state(RegionId(3)),
            Some(if detach {
                RegionState::Detached
            } else {
                RegionState::Frozen
            })
        );
        assert_eq!(game.pending_intention(attacker), Some(&queued));
        assert!(game.execute_next_intention().is_none());
        for _ in 0..2 {
            game.act(player, Action::Wait).unwrap();
        }
        let restored = round_trip(&game);
        assert_eq!(restored, game);
        assert_eq!(restored.pending_intention(attacker), Some(&queued));
        assert!(restored.detached_records_valid(&mut records));
        let mut expected = None;
        for mut candidate in [game, restored] {
            candidate
                .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
                .unwrap();
            // Equal-time ordering gives the original character its turn first.
            assert_eq!(candidate.next_actor(), Some(player));
            candidate.act(player, Action::Wait).unwrap();
            let execution = candidate.execute_next_intention().unwrap();
            assert_eq!(execution.intention.id, intention);
            execution.outcome.unwrap();
            let progress = candidate.preparation(attacker).unwrap();
            assert_eq!(progress.intention, Some(intention));
            assert_eq!(progress.target, target);
            if let Some(expected) = &expected {
                assert_eq!(&candidate, expected);
            } else {
                expected = Some(candidate);
            }
        }
    }
}

#[test]
fn paused_native_portal_attack_keeps_required_regions_active_without_losing_progress() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let attacker = game.spawn_actor(at(3, 11, 1), ticks(100)).unwrap();
    let target = game.spawn_actor(at(4, 0, 1), ticks(12)).unwrap();
    fighter(&mut game, attacker, "a");
    fighter(&mut game, target, "b");
    run(&mut game, player, &[("a", "b")]);
    game.act(player, Action::Wait).unwrap();
    let intention = game
        .admit_intention(
            attacker,
            Action::Attack { target },
            tor_simulation::IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.act(target, Action::Wait).unwrap();
    game.suspend_intention(attacker, intention).unwrap();
    let progress = game.preparation(attacker).unwrap().clone();
    assert!(!progress.active);
    assert!(progress.remaining > 0 && progress.remaining < 30);
    let before = game.clone();
    assert_eq!(
        game.apply_region_transition(&sets(&[1], &[1, 2]), &mut records),
        Err(TransitionError::MustBeActive(RegionId(3)))
    );
    assert_eq!(game, before);
    game.transition_regions(&sets(&[1], &[1, 2]), &mut records)
        .unwrap();
    for region in [3, 4] {
        assert_eq!(
            game.region_state(RegionId(region)),
            Some(RegionState::Active)
        );
    }
    assert_eq!(game.preparation(attacker), Some(&progress));
    let mut restored = round_trip(&game);
    restored.resume_intention(attacker, intention).unwrap();
    assert_eq!(restored.preparation(attacker), Some(&progress));
    let execution = restored.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, intention);
    execution.outcome.unwrap();
    let resumed = restored.preparation(attacker).unwrap();
    assert_eq!(resumed.intention, Some(intention));
    assert_eq!(resumed.target, target);
    assert!(resumed.active);
    assert!(resumed.remaining <= progress.remaining);
}

#[test]
fn pins_reject_transitions_without_changing_the_game() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let edge = game.spawn_actor(at(2, 11, 1), ticks(100)).unwrap();
    run(&mut game, player, &[]);
    game.add_reference_point(ReferencePoint {
        target: ReferenceTarget::Actor(edge),
        active_radius: None,
        load_radius: None,
        observes: false,
    })
    .unwrap();
    let before = game.clone();
    // An actor at a region's edge can reach the next region.
    assert_eq!(
        game.apply_region_transition(&sets(&[1, 2], &[1, 2, 3]), &mut records),
        Err(TransitionError::MustBeActive(RegionId(3)))
    );
    // Linked regions of active ones stay loaded.
    assert_eq!(
        game.apply_region_transition(&sets(&[1, 2, 3], &[1, 2, 3]), &mut records),
        Err(TransitionError::MustBeLoaded(RegionId(4)))
    );
    assert_eq!(
        game.apply_region_transition(&sets(&[1], &[2]), &mut records),
        Err(TransitionError::ActiveNotLoaded(RegionId(1)))
    );
    assert_eq!(
        game.apply_region_transition(&sets(&[1], &[1, 9]), &mut records),
        Err(TransitionError::UnknownRegion(RegionId(9)))
    );
    assert_eq!(game, before);
    let (applied, _) = game
        .transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert_eq!(applied.active, regions(&[1, 2, 3]));
}

#[test]
fn an_observer_keeps_everything_it_sees_active() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    game.teleport(player, at(1, 6, 1)).unwrap();
    run(&mut game, player, &[]);
    // Sight from x = 6 reaches two cells into region 2.
    let visible: BTreeSet<_> = game
        .scene(player)
        .unwrap()
        .iter()
        .map(|c| c.location.region)
        .collect();
    assert!(visible.contains(&RegionId(2)));
    assert_eq!(
        game.apply_region_transition(&sets(&[1], &[1, 2]), &mut records),
        Err(TransitionError::MustBeActive(RegionId(2)))
    );
    let (applied, _) = game
        .transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert!(applied.active.contains(&RegionId(2)));
}

#[test]
fn a_body_spanning_a_portal_pins_both_regions() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let wide = game.spawn_actor(at(3, 11, 1), ticks(100)).unwrap();
    game.set_body(
        wide,
        BodySpec {
            cells: vec![[0, 0, 0], [1, 0, 0]],
            eye: [0, 0, 0],
            mass: 80,
        },
    )
    .unwrap();
    run(&mut game, player, &[]);
    // Region 3 frozen but loaded; the body's other half is in region 4.
    assert_eq!(
        game.apply_region_transition(&sets(&[1], &[1, 2, 3]), &mut records),
        Err(TransitionError::MustBeLoaded(RegionId(4)))
    );
    let (applied, _) = game
        .transition_regions(&sets(&[1], &[1, 2, 3]), &mut records)
        .unwrap();
    assert!(applied.loaded.contains(&RegionId(4)));
}

#[test]
fn a_frozen_attack_keeps_its_target_loaded() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let attacker = game.spawn_actor(at(3, 11, 1), ticks(100)).unwrap();
    let target = game.spawn_actor(at(4, 0, 1), ticks(100)).unwrap();
    fighter(&mut game, attacker, "a");
    fighter(&mut game, target, "b");
    run(&mut game, player, &[("a", "b")]);
    game.act(player, Action::Wait).unwrap();
    game.act(attacker, Action::Attack { target }).unwrap();
    assert_eq!(
        game.apply_region_transition(&sets(&[1], &[1, 2, 3]), &mut records),
        Err(TransitionError::MustBeLoaded(RegionId(4)))
    );
}

#[test]
fn an_item_point_follows_its_carrier() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let carrier = game.spawn_actor(at(3, 5, 1), ticks(100)).unwrap();
    let item = game
        .place_authored_item(50, at(3, 5, 1), "lantern".into(), Some(carrier))
        .unwrap();
    run(&mut game, player, &[]);
    game.add_reference_point(ReferencePoint {
        target: ReferenceTarget::Item(item),
        active_radius: None,
        load_radius: None,
        observes: false,
    })
    .unwrap();
    assert_eq!(
        game.apply_region_transition(&sets(&[1], &[1, 2]), &mut records),
        Err(TransitionError::MustBeActive(RegionId(3)))
    );
}

#[test]
fn a_point_on_a_detached_actor_brings_its_region_back() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let far = game.spawn_actor(at(4, 5, 1), ticks(100)).unwrap();
    run(&mut game, player, &[]);
    game.transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert_eq!(game.region_state(RegionId(4)), Some(RegionState::Detached));
    game.add_reference_point(ReferencePoint {
        target: ReferenceTarget::Actor(far),
        active_radius: None,
        load_radius: None,
        observes: true,
    })
    .unwrap();
    assert_eq!(
        game.region_roots()
            .iter()
            .map(|r| r.region)
            .collect::<Vec<_>>(),
        [RegionId(1), RegionId(4)]
    );
    let (applied, report) = game
        .transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert!(applied.active.contains(&RegionId(4)));
    assert_eq!(report.attached, [RegionId(3), RegionId(4)]);
    assert_eq!(game.region_state(RegionId(4)), Some(RegionState::Active));
}

/// Two identical games; one detaches what the other only freezes. After
/// play and reattachment they must be equal, and the detached one must
/// survive a save round trip while detached.
#[test]
fn detaching_and_reattaching_equals_only_freezing() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    let hunter = game.spawn_actor(at(2, 9, 1), ticks(90)).unwrap();
    let prey = game.spawn_actor(at(3, 6, 1), ticks(120)).unwrap();
    let keeper = game.spawn_actor(at(3, 4, 2), ticks(110)).unwrap();
    for (id, faction) in [(hunter, "hunter"), (prey, "prey"), (keeper, "prey")] {
        fighter(&mut game, id, faction);
    }
    game.configure_ai(hunter, AiProfile::default()).unwrap();
    game.configure_ai(prey, AiProfile::default()).unwrap();
    let mut coin = tor_simulation::ItemSpec::ordinary("coin".into());
    coin.class = tor_simulation::ItemClass::Coin;
    game.place_item_stack(60, at(3, 7, 1), None, 1, coin)
        .unwrap();
    game.place_authored_item(61, at(3, 4, 2), "key".into(), Some(keeper))
        .unwrap();
    let relic = game
        .place_authored_item(62, at(4, 8, 2), "relic".into(), None)
        .unwrap();
    game.place_authored_door(70, at(4, 5, 1), false, 1).unwrap();
    game.configure_run(
        player,
        BTreeSet::from([player]),
        Some(Objective {
            anchor: at(1, 0, 1),
            item: Some(relic),
            disclosed: false,
            continue_play: false,
        }),
        BTreeMap::from([
            ("hunter".into(), BTreeSet::from(["prey".into()])),
            ("prey".into(), BTreeSet::from(["hunter".into()])),
        ]),
    )
    .unwrap();
    game.refresh_navigation();
    game.add_default_reference_points().unwrap();
    // Let the AI see and remember each other first.
    step(&mut game, 6);

    let mut frozen = game.clone();
    let mut detached = game.clone();
    let (applied, report) = detached
        .transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert!(report.detached.contains(&RegionId(4)), "{report:?}");
    frozen
        .transition_regions(
            &RegionTransition {
                active: applied.active.clone(),
                loaded: regions(&[1, 2, 3, 4]),
            },
            &mut records,
        )
        .unwrap();
    assert!(frozen
        .region_roots()
        .iter()
        .all(|r| applied.active.contains(&r.region)));

    step(&mut frozen, 5);
    step(&mut detached, 5);
    // Knowledge references into detached regions (the objective item, AI
    // memory, navigation) stay valid through a save round trip.
    // Restoring reads no records; they're checked when they attach.
    let restored = round_trip(&detached);
    assert_eq!(restored, detached);
    assert!(restored.detached_records_valid(&mut records));

    let all = sets(&[1, 2, 3, 4], &[1, 2, 3, 4]);
    let mut restored = restored;
    frozen.transition_regions(&all, &mut records).unwrap();
    detached.transition_regions(&all, &mut records).unwrap();
    restored.transition_regions(&all, &mut records).unwrap();
    // Record identities are never reused, so only the detached games moved
    // their allocator; everything else must match exactly.
    assert_ne!(detached, frozen);
    frozen.continue_record_ids(&detached);
    assert_eq!(detached, frozen);
    assert_eq!(restored, frozen);
    step(&mut frozen, 8);
    step(&mut detached, 8);
    assert_eq!(detached, frozen);
}

#[test]
fn saves_without_lifecycle_state_are_unchanged() {
    let (mut game, player) = corridor();
    run(&mut game, player, &[]);
    let point = game.reference_points().next().unwrap().0;
    assert!(game.remove_reference_point(point));
    // A game whose point allocator moved on is no longer empty.
    let mut shared = SharedState::default();
    let text = serde_json::to_string(&game.checkpoint(&mut shared)).unwrap();
    assert!(text.contains("lifecycle"));

    let (mut plain, player) = corridor();
    plain
        .configure_run(player, BTreeSet::from([player]), None, BTreeMap::new())
        .unwrap();
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_string(&plain.checkpoint(&mut shared)).unwrap();
    let worlds = serde_json::to_string(&shared).unwrap();
    assert!(!snapshot.contains("lifecycle"));
    assert!(!worlds.contains("absent"));
}

#[test]
fn with_no_reference_points_everything_can_detach() {
    let mut records = MemoryRecords::default();
    // No run: nothing awaits input and no point follows anyone.
    let (mut game, _) = corridor();
    game.spawn_actor(at(3, 5, 1), ticks(100)).unwrap();
    let (applied, report) = game
        .transition_regions(&sets(&[], &[]), &mut records)
        .unwrap();
    assert!(applied.loaded.is_empty());
    assert_eq!(report.detached.len(), 4);
    assert_eq!(game.next_actor(), None);
    // Characters are detached like anyone else; the save stays valid.
    assert_eq!(round_trip(&game), game);
}

#[test]
fn frozen_and_detached_checkpoints_round_trip() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    game.spawn_actor(at(3, 5, 1), ticks(150)).unwrap();
    run(&mut game, player, &[]);
    game.transition_regions(&sets(&[1], &[1, 2, 3]), &mut records)
        .unwrap();
    assert_eq!(round_trip(&game), game);
    game.act(player, Action::Wait).unwrap();
    game.transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    assert_eq!(round_trip(&game), game);
    assert!(game.detached_records_valid(&mut records));
    let item = ItemId(999);
    assert!(game
        .add_reference_point(ReferencePoint {
            target: ReferenceTarget::Item(item),
            active_radius: None,
            load_radius: None,
            observes: false,
        })
        .is_err());
}

#[test]
fn identical_lifecycle_states_are_encoded_once() {
    let mut records = MemoryRecords::default();
    let (mut game, player) = corridor();
    game.spawn_actor(at(3, 5, 1), ticks(150)).unwrap();
    run(&mut game, player, &[]);
    game.transition_regions(&sets(&[1], &[1]), &mut records)
        .unwrap();
    // Like rewind boundaries: two unchanged games, then one that moved.
    let mut moved = game.clone();
    moved.act(player, Action::Wait).unwrap();
    let point = moved.reference_points().next().unwrap().0;
    assert!(moved.remove_reference_point(point));
    let mut shared = SharedState::default();
    let snapshots =
        [&game, &game, &moved].map(|g| serde_json::to_value(g.checkpoint(&mut shared)).unwrap());
    assert_eq!(snapshots[0]["lifecycle"], 0);
    assert_eq!(snapshots[1]["lifecycle"], 0);
    assert_eq!(snapshots[2]["lifecycle"], 1);
    let text = serde_json::to_string(&shared).unwrap();
    assert_eq!(text.matches("\"directory\"").count(), 2);
}

/// Two stacked regions joined by a physical portal, falling under gravity,
/// beside a separate room for the character.
fn shaft() -> (Game, ActorId, ActorId) {
    let mut world = World::new(vec![], vec![]).unwrap();
    for (id, bounds) in [
        (1, Extent::new(12, 3, 1).unwrap()),
        (10, Extent::new(3, 3, 6).unwrap()),
        (11, Extent::new(3, 3, 6).unwrap()),
    ] {
        world
            .add_region(Region {
                id: RegionId(id),
                name: format!("room {id}"),
                bounds,
            })
            .unwrap();
    }
    let cell = |region, z| Location {
        region: RegionId(region),
        position: Position { x: 0, y: 0, z },
    };
    world
        .connect_portal_area(
            Passage {
                from: cell(10, 0),
                direction: Direction::Down,
                to: cell(11, 5),
            },
            0,
            3,
            3,
        )
        .unwrap();
    world
        .connect_portal_area(
            Passage {
                from: cell(11, 5),
                direction: Direction::Up,
                to: cell(10, 0),
            },
            0,
            3,
            3,
        )
        .unwrap();
    let mut game = Game::new(world, 3);
    game.set_gravity(RegionId(10), [0, 0, -1]).unwrap();
    game.set_gravity(RegionId(11), [0, 0, -1]).unwrap();
    let player = game.spawn_actor(at(1, 2, 1), ticks(100)).unwrap();
    let faller = game
        .spawn_actor(
            Location {
                region: RegionId(10),
                position: Position { x: 1, y: 1, z: 5 },
            },
            ticks(100),
        )
        .unwrap();
    game.configure_run(player, BTreeSet::from([player]), None, BTreeMap::new())
        .unwrap();
    game.add_default_reference_points().unwrap();
    (game, player, faller)
}

#[test]
fn falling_into_a_frozen_region_freezes_the_faller_until_it_thaws() {
    let mut records = MemoryRecords::default();
    let (mut game, _, faller) = shaft();
    let (applied, _) = game
        .transition_regions(&sets(&[1, 10], &[1, 10]), &mut records)
        .unwrap();
    assert_eq!(applied.loaded, regions(&[1, 10, 11]));
    assert_eq!(game.region_state(RegionId(11)), Some(RegionState::Frozen));
    let mut entered = None;
    for _ in 0..40 {
        step(&mut game, 1);
        let location = game.observe(faller).unwrap().location;
        if location.region == RegionId(11) {
            entered = Some((location, game.actor_motion(faller).unwrap().clone()));
            break;
        }
    }
    let (location, motion) = entered.expect("the faller reaches the lower region");
    assert_ne!(motion.velocity, [0; 3], "it froze mid-fall");
    step(&mut game, 5);
    assert_eq!(game.observe(faller).unwrap().location, location);
    assert_eq!(game.actor_motion(faller), Some(&motion));
    assert_eq!(round_trip(&game), game);

    game.transition_regions(&sets(&[1, 10, 11], &[1, 10, 11]), &mut records)
        .unwrap();
    step(&mut game, 5);
    assert_ne!(
        game.observe(faller).unwrap().location,
        location,
        "falling resumed"
    );
}
