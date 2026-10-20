use super::*;
use crate::abilities::{AbilityInvocation, PlannedEffect};
use crate::perception::PerceptionValue;
use tor_world::{Concealment, Direction, Trap, TrapKind};

use crate::perception::contract_fixtures::*;

fn trap(
    game: &mut Game,
    number: u64,
    location: tor_world::Location,
    dc: i32,
) -> ConcealedFeatureId {
    if !game.world.is_chamber(location.region) {
        let regions: Vec<_> = game
            .world
            .loaded_regions()
            .map(|id| game.world.region(id).unwrap().clone())
            .collect();
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        for region in regions {
            world.add_chamber(region).unwrap();
        }
        game.world = Shared::new(world);
    }
    game.world
        .place_trap(
            location,
            Trap {
                id: number,
                dc,
                kind: TrapKind::Rust {
                    slot: "body_armor".into(),
                    amount: 1,
                },
            },
        )
        .unwrap();
    ConcealedFeatureId::Trap(number)
}
fn check(game: &Game) -> FrozenDiscoveryCheck {
    game.capture_discovery_check(OWNER).unwrap()
}
fn attempts(game: &Game, actor: ActorId) -> usize {
    game.actors[&actor]
        .discovery
        .0
        .iter()
        .filter(|fact| fact.attempted)
        .count()
}
fn checkpoint(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    // An in-memory checkpoint clone retains derived caches. Exercise actual
    // encoded persistence so reload has only authoritative receipts and clocks.
    let saved = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap()
}
fn paid(game: &mut Game) {
    start(
        game,
        OWNER,
        action("fixture_search", AbilityInvocation::actor_target(OWNER)),
    );
    let pending = game.actors[&OWNER].pending.as_ref().unwrap();
    let due = pending.started + pending.remaining;
    game.tick = due;
    game.resolve_preparations().unwrap();
}

#[test]
fn passive_success_and_failure_are_exact_observer_owned_persistent_receipts() {
    let mut game = fixture(&[], &[]);
    let found = trap(&mut game, 11, at(1, 4, 2), 0);
    let missed = trap(&mut game, 12, at(1, 5, 2), 100);
    let mut expected_rng = game.rules_rng;
    check(&game).roll(&mut expected_rng, true);
    check(&game).roll(&mut expected_rng, true);
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert_eq!(game.rules_rng, expected_rng);
    assert_eq!(attempts(&game, OWNER), 2);
    assert!(game.feature_known(OWNER, found));
    assert!(!game.feature_family_known(OWNER, found));
    assert!(!game.feature_known(OWNER, missed));
    assert!(!game.feature_known(SUBJECT, found));
    assert_eq!(attempts(&game, SUBJECT), 0);
    let mut restored = checkpoint(&game);
    for _ in 0..3 {
        restored.reconcile_passive_discovery_for(OWNER).unwrap();
    }
    assert_eq!(restored.rules_rng, expected_rng);
    assert_eq!(
        restored.actors[&OWNER].discovery,
        game.actors[&OWNER].discovery
    );
    restored.reconcile_passive_discovery_for(SUBJECT).unwrap();
    assert_eq!(attempts(&restored, SUBJECT), 2);
    assert!(restored.feature_known(SUBJECT, found));
}

#[test]
fn readonly_queries_never_attempt_discovery_or_reroll() {
    let mut game = fixture(&[], &[]);
    trap(&mut game, 11, at(1, 4, 2), 0);
    let before = game.clone();
    for _ in 0..4 {
        game.observe(OWNER).unwrap();
        game.scene(OWNER).unwrap();
        game.ability_plan(OWNER, &id("fixture_search"), OWNER)
            .unwrap();
        game.capture_discovery_check(OWNER).unwrap();
    }
    assert_eq!(game, before);
}

#[test]
fn failed_passive_does_not_prevent_repeatable_paid_discovery() {
    let mut game = fixture(&[], &[]);
    let feature = trap(&mut game, 11, at(1, 4, 2), 100);
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert!(!game.feature_known(OWNER, feature));
    let before = game.rules_rng;
    let generous = FrozenDiscoveryCheck {
        bonus: 100,
        edge: 0,
    };
    game.run_active_discovery(OWNER, &generous).unwrap();
    assert_ne!(game.rules_rng, before);
    assert!(game.feature_known(OWNER, feature));
    assert_eq!(attempts(&game, OWNER), 1);
    assert!(!game.feature_family_known(OWNER, feature));
    game.reveal_feature_family(OWNER, feature).unwrap();
    assert!(game.feature_family_known(OWNER, feature));
    assert!(!game.feature_family_known(SUBJECT, feature));
}

#[test]
fn passive_composes_one_disadvantage_with_existing_edge_through_shared_kernel() {
    for edge in [-4, -1, 0, 1, 4] {
        let captured = FrozenDiscoveryCheck { bonus: 7, edge };
        let balance = edge - 1;
        let kernel_edge = if balance >= 0 {
            crate::dice::Edge::from_counts(balance as u32, 0)
        } else {
            crate::dice::Edge::from_counts(0, balance.unsigned_abs() as u32)
        };
        let mut actual = 123;
        let mut expected = 123;
        let value = captured.roll(&mut actual, true);
        assert_eq!(
            value,
            i64::from(crate::dice::roll_check(&mut expected, kernel_edge).kept) + 7
        );
        assert_eq!(actual, expected);
    }
}

#[test]
fn darkness_blindness_and_occlusion_do_not_burn_first_observation() {
    let mut dark = fixture(&[], &[]);
    let feature = trap(&mut dark, 11, at(1, 4, 2), 0);
    dark.world.set_cell_light(at(1, 4, 2), false).unwrap();
    let rng = dark.rules_rng;
    dark.reconcile_passive_discovery_for(OWNER).unwrap();
    assert_eq!(dark.rules_rng, rng);
    assert_eq!(attempts(&dark, OWNER), 0);
    dark.world.set_cell_light(at(1, 4, 2), true).unwrap();
    dark.reconcile_passive_discovery_for(OWNER).unwrap();
    assert!(dark.feature_known(OWNER, feature));

    let mut blind = fixture(&[PerceptionValue::Sight { enabled: false }], &[]);
    trap(&mut blind, 11, at(1, 4, 2), 0);
    blind.reconcile_passive_discovery_for(OWNER).unwrap();
    assert_eq!(attempts(&blind, OWNER), 0);

    let mut blocked = fixture(&[], &[]);
    trap(&mut blocked, 11, at(1, 6, 2), 0);
    for y in 0..7 {
        blocked.world.set_wall(at(1, 4, y), true).unwrap();
    }
    blocked.reconcile_passive_discovery_for(OWNER).unwrap();
    assert_eq!(attempts(&blocked, OWNER), 0);
}

#[test]
fn sleep_and_waking_defer_the_first_genuine_observation() {
    let mut game = fixture_with(&[], &[], |rules, _, _| {
        rules.conditions.push(serde_json::from_value(serde_json::json!({
            "id":"fixture_sleep", "family":"fixture_sleep", "name":"Asleep", "lifecycle":"timed",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}}, "restrictions":["voluntary_actions"],
            "preparation":"pause", "modifiers":[],"stacking":{"kind":"extend"},"source_scoped":false,"descriptors":[],"capacity":"reject"
        })).unwrap());
    });
    apply(&mut game, OWNER, "fixture_sleep", 10);
    let feature = trap(&mut game, 11, at(1, 4, 2), 0);
    let rng = game.rules_rng;
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert_eq!(game.rules_rng, rng);
    assert_eq!(attempts(&game, OWNER), 0);
    game.tick = 10;
    game.settle_actor_time(OWNER).unwrap();
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert!(game.feature_known(OWNER, feature));
    assert_eq!(attempts(&game, OWNER), 1);
}

#[test]
fn abstract_stair_awareness_is_not_an_exterior_cue() {
    let mut game = fixture(&[], &[]);
    let landing = at(2, 2, 2);
    let feature = trap(&mut game, 11, landing, 0);
    game.register_named_anchors(
        landing.region,
        BTreeMap::from([("landing".into(), landing.position)]),
    )
    .unwrap();
    game.connect_named_stair(
        at(1, 2, 2),
        Direction::Up,
        tor_world::NamedAnchor {
            region: landing.region,
            name: "landing".into(),
        },
    )
    .unwrap();
    assert!(game
        .scene(OWNER)
        .unwrap()
        .iter()
        .any(|cell| cell.location == landing && !cell.physical));
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert!(!game.feature_known(OWNER, feature));
    assert_eq!(attempts(&game, OWNER), 0);
}

#[test]
fn secret_revelation_is_personal_and_never_opens_shared_geometry() {
    let mut game = fixture(&[], &[]);
    let at = at(1, 4, 2);
    let door = game.place_authored_door(33, at, false, 1).unwrap();
    game.world.conceal_door(at, Concealment { dc: 0 }).unwrap();
    assert!(game.world.opaque(at));
    let observation = game.observe(SUBJECT).unwrap();
    let closure = observation
        .visible_cells
        .iter()
        .find(|cell| cell.location == at)
        .unwrap();
    assert!(closure.wall);
    assert_eq!(closure.material, "stone");
    assert!(closure.door.is_none());
    assert!(!closure.door_secret && !closure.door_reachable);
    assert!(closure.door_approaches.is_empty());
    let before = game.clone();
    assert!(game
        .act(SUBJECT, crate::Action::SetDoor { door, open: true })
        .is_err());
    assert_eq!(
        game, before,
        "unknown doors have neither query affordance nor action authority"
    );
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert!(game.feature_known(OWNER, ConcealedFeatureId::Door(door)));
    assert!(!game.feature_known(SUBJECT, ConcealedFeatureId::Door(door)));
    assert!(game.world.opaque(at));
    assert!(!game.world.door(at).unwrap().open);
}

#[test]
fn paid_search_always_rolls_once_and_uses_only_outgoing_reserved_tokens() {
    let mut game = fixture(&[], &[]);
    apply(&mut game, OWNER, "readied", 1000);
    apply(&mut game, OWNER, "exposed", 1000);
    let automatic = check(&game);
    let plan = game
        .ability_plan(OWNER, &id("fixture_search"), OWNER)
        .unwrap();
    let ability = plan.as_ability().unwrap();
    assert_eq!(ability.tokens.len(), 1);
    assert_eq!(ability.tokens[0].reservation.condition.as_str(), "readied");
    let PlannedEffect::SearchAdjacentGuises { check: captured } = ability.strikes[0].on_success[0]
    else {
        panic!("search")
    };
    assert_eq!(captured.edge, automatic.edge + 1);
    let mut expected = game.rules_rng;
    captured.roll(&mut expected, false);
    paid(&mut game);
    assert_eq!(game.rules_rng, expected, "empty Search still rolls once");
    let creature = game.creature(OWNER).unwrap();
    assert!(!creature
        .conditions()
        .has_active(&crate::content_ids::ConditionId::new("readied").unwrap()));
    assert!(creature
        .conditions()
        .has_active(&crate::content_ids::ConditionId::new("exposed").unwrap()));
}

#[test]
fn one_active_roll_is_shared_by_all_candidates_and_ignores_token_inventory() {
    let mut game = fixture(&[], &[]);
    apply(&mut game, OWNER, "readied", 1000);
    let first = trap(&mut game, 11, at(1, 4, 2), 0);
    let second = trap(&mut game, 12, at(1, 5, 2), 0);
    let captured = check(&game);
    let mut expected = game.rules_rng;
    captured.roll(&mut expected, false);
    game.run_active_discovery(OWNER, &captured).unwrap();
    assert_eq!(game.rules_rng, expected);
    assert!(game.feature_known(OWNER, first) && game.feature_known(OWNER, second));
    assert!(game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .has_active(&crate::content_ids::ConditionId::new("readied").unwrap()));
}

#[test]
fn discovery_records_and_operands_reject_aliases_duplicates_and_impossible_flags() {
    let feature = serde_json::json!({"kind":"trap","value":11});
    let valid =
        serde_json::json!({"feature":feature,"attempted":true,"presence":false,"family":false});
    let read = |value: serde_json::Value| {
        let text = value.to_string();
        let mut de = serde_json::Deserializer::from_str(&text);
        read_discovery(&mut de).is_ok()
    };
    assert!(read(serde_json::json!([valid.clone()])));
    assert!(!read(serde_json::json!([valid.clone(), valid.clone()])));
    for (key, value) in [
        ("attempted", serde_json::json!(false)),
        ("family", serde_json::json!(true)),
    ] {
        let mut invalid = valid.clone();
        invalid[key] = value;
        assert!(!read(serde_json::json!([invalid])));
    }
    assert!(serde_json::from_value::<FrozenDiscoveryCheck>(serde_json::json!([0, 0])).is_err());
    assert!(serde_json::from_value::<FrozenDiscoveryCheck>(
        serde_json::json!({"bonus":0,"edge":1000001})
    )
    .is_err());
}

#[test]
fn completed_construction_not_partial_source_setup_owns_first_observation() {
    let mut game = fixture(&[], &[]);
    let feature = trap(&mut game, 11, at(1, 4, 2), 0);
    let rng = game.rules_rng;
    apply(&mut game, OWNER, "readied", 1000);
    assert_eq!(game.rules_rng, rng);
    assert!(!game.feature_known(OWNER, feature));
    assert!(!game.lifecycle.discovery_started);
    game.initialize_discovery().unwrap();
    assert!(game.lifecycle.discovery_started);
    assert!(game.feature_known(OWNER, feature));
    let mut restored = checkpoint(&game);
    let before = restored.rules_rng;
    restored.initialize_discovery().unwrap();
    assert_eq!(restored.rules_rng, before);
}

#[test]
fn unchanged_boundaries_skip_sight_and_reentry_never_retries_failure() {
    let mut game = fixture(&[], &[]);
    let feature = trap(&mut game, 11, at(1, 4, 2), 100);
    game.initialize_discovery().unwrap();
    let rng = game.rules_rng;
    let scenes = crate::diagnostics::work_counts().scenes;
    for _ in 0..4 {
        game.reconcile_passive_discovery().unwrap();
    }
    assert_eq!(crate::diagnostics::work_counts().scenes, scenes);
    game.teleport(OWNER, at(2, 2, 2)).unwrap();
    game.teleport(OWNER, at(1, 2, 2)).unwrap();
    assert_eq!(game.rules_rng, rng);
    assert!(!game.feature_known(OWNER, feature));
    assert_eq!(attempts(&game, OWNER), 1);
    assert_eq!(attempts(&checkpoint(&game), OWNER), 1);
}

#[test]
fn seeing_an_open_secret_entrance_is_retained_after_another_actor_closes_it() {
    let mut game = fixture(&[], &[]);
    let location = at(1, 4, 2);
    let door = game.place_authored_door(33, location, false, 1).unwrap();
    game.world
        .conceal_door(location, Concealment { dc: 100 })
        .unwrap();
    game.initialize_discovery().unwrap();
    let feature = ConcealedFeatureId::Door(door);
    assert!(!game.feature_known(OWNER, feature));
    assert!(!game.feature_known(SUBJECT, feature));
    let rng = game.rules_rng;
    game.world.set_door(location, true);
    game.reconcile_passive_discovery().unwrap();
    assert!(game.feature_known(OWNER, feature) && game.feature_known(SUBJECT, feature));
    assert_eq!(
        game.rules_rng, rng,
        "an exposed opening needs no hidden-feature roll"
    );
    game.world.set_door(location, false);
    game.reconcile_passive_discovery().unwrap();
    assert!(game.feature_known(OWNER, feature) && game.feature_known(SUBJECT, feature));
    assert!(game.world.opaque(location));
    assert_eq!(game.rules_rng, rng);
}

#[test]
fn physical_rotated_join_keeps_the_same_exact_feature_identity() {
    let content = fixture(&[], &[]).content().clone();
    let rotation = (0..24)
        .find(|&turn| {
            tor_world::rotate_vector(turn, [1, 0, 0]) == [0, 1, 0]
                && tor_world::rotate_vector(turn, [0, 1, 0]) == [-1, 0, 0]
        })
        .unwrap();
    let mut world = tor_world::World::new(vec![], vec![]).unwrap();
    for (id, width, height) in [(1, 3, 5), (2, 5, 3)] {
        world
            .add_chamber(tor_world::Region {
                id: tor_world::RegionId(id),
                name: format!("room {id}"),
                bounds: tor_world::Extent::new(width, height, 1).unwrap(),
            })
            .unwrap();
    }
    for (from, direction, to, turn) in [
        (at(1, 2, 0), Direction::East, at(2, 4, 0), rotation),
        (
            at(2, 0, 0),
            Direction::North,
            at(1, 2, 4),
            tor_world::inverse_rotation(rotation),
        ),
    ] {
        world
            .connect_portal_area(
                tor_world::Passage {
                    from,
                    direction,
                    to,
                },
                turn,
                5,
                1,
            )
            .unwrap();
    }
    let mut game = Game::new(world, 42, content);
    assert_eq!(
        spawn(&mut game, "fixture_perception_owner", at(1, 2, 2)),
        OWNER
    );
    let feature = trap(&mut game, 11, at(2, 2, 1), 0);
    assert!(game
        .scene_snapshot(OWNER)
        .unwrap()
        .1
        .iter()
        .any(|cell| cell.location == at(2, 2, 1)));
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert!(game.feature_known(OWNER, feature));
    assert_eq!(attempts(&game, OWNER), 1);
    let rng = game.rules_rng;
    game.reconcile_passive_discovery_for(OWNER).unwrap();
    assert_eq!(game.rules_rng, rng);
}

#[test]
fn paid_search_freezes_operands_across_pause_checkpoint_and_resume() {
    let mut game = fixture(&[], &[]);
    apply(&mut game, OWNER, "readied", 1000);
    let intent = start(
        &mut game,
        OWNER,
        action("fixture_search", AbilityInvocation::actor_target(OWNER)),
    );
    let plan = game.preparation(OWNER).unwrap().plan.clone();
    let PlannedEffect::SearchAdjacentGuises { check: frozen } =
        plan.as_ability().unwrap().strikes[0].on_success[0]
    else {
        panic!("search")
    };
    let creature = game.creature(OWNER).unwrap();
    assert_eq!(
        frozen.bonus,
        i64::from(
            creature
                .derived()
                .attributes
                .get(crate::attributes::Attribute::Awareness)
        ) + i64::from(
            creature
                .derived()
                .skills
                .get(crate::attributes::Skill::Perception)
        )
    );
    game.tick = 20;
    game.pause_preparation(OWNER).unwrap();
    assert!(!game.preparation(OWNER).unwrap().active);
    apply(&mut game, OWNER, "distracted", 1000);
    assert_eq!(game.preparation(OWNER).unwrap().plan, plan);
    // This fixture advances time directly, outside the normal idle-actor scheduler.
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = game.tick;
    let mut game = checkpoint(&game);
    assert_eq!(game.preparation(OWNER).unwrap().plan, plan);
    let mut expected = game.rules_rng;
    frozen.roll(&mut expected, false);
    game.resume_intention(OWNER, intent).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    if let Some(pending) = game.preparation(OWNER) {
        game.tick = pending.started + pending.remaining;
        game.resolve_preparations().unwrap();
    }
    assert_eq!(game.rules_rng, expected);
    assert!(game.preparation(OWNER).is_none());
}

#[test]
fn trap_examination_requires_known_physical_adjacency_and_costs_no_time() {
    let mut game = fixture(&[], &[]);
    let adjacent = trap(&mut game, 11, at(1, 2, 3), 100);
    let distant = trap(&mut game, 12, at(1, 7, 2), 100);
    let before = game.clone();
    assert!(game
        .act(OWNER, crate::Action::ExamineTrap { trap: 11 })
        .is_err());
    assert_eq!(game, before, "unknown presence cannot be probed for free");
    game.apply_discovery_total(OWNER, 1000).unwrap();
    game.initialize_discovery().unwrap();
    assert!(game.feature_known(OWNER, adjacent));
    assert!(game.feature_known(OWNER, distant));
    assert!(!game.feature_family_known(OWNER, adjacent));
    let before = game.clone();
    assert!(game
        .act(OWNER, crate::Action::ExamineTrap { trap: 12 })
        .is_err());
    assert_eq!(game, before);
    let tick = game.tick;
    let ready = game.actors[&OWNER].ready_at;
    let rng = game.rules_rng;
    game.act(OWNER, crate::Action::ExamineTrap { trap: 11 })
        .unwrap();
    assert_eq!(
        (game.tick, game.actors[&OWNER].ready_at, game.rules_rng),
        (tick, ready, rng)
    );
    assert!(game.feature_family_known(OWNER, adjacent));
    assert!(!game.feature_family_known(SUBJECT, adjacent));
}

#[test]
fn trap_examination_cannot_cross_abstract_stairs_or_interrupt_paid_preparation() {
    let mut game = fixture(&[], &[]);
    let adjacent = trap(&mut game, 11, at(1, 2, 3), 0);
    let remote = trap(&mut game, 12, at(2, 2, 2), 0);
    game.register_named_anchors(
        tor_world::RegionId(2),
        BTreeMap::from([("landing".into(), at(2, 2, 2).position)]),
    )
    .unwrap();
    game.connect_named_stair(
        at(1, 2, 2),
        Direction::Up,
        tor_world::NamedAnchor {
            region: tor_world::RegionId(2),
            name: "landing".into(),
        },
    )
    .unwrap();
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .discovery
        .update(remote, false, true, false)
        .unwrap();
    game.initialize_discovery().unwrap();
    let before = game.clone();
    assert!(game
        .act(OWNER, crate::Action::ExamineTrap { trap: 12 })
        .is_err());
    assert_eq!(
        game, before,
        "stair awareness is not physical examination reach"
    );
    assert!(game.feature_known(OWNER, adjacent));
    start(
        &mut game,
        OWNER,
        action("fixture_search", AbilityInvocation::actor_target(OWNER)),
    );
    let before = game.clone();
    assert!(game
        .act(OWNER, crate::Action::ExamineTrap { trap: 11 })
        .is_err());
    assert_eq!(
        game, before,
        "examination cannot disturb paid work or its reservation"
    );
    assert!(!game.feature_family_known(OWNER, adjacent));
}

#[test]
fn saved_search_rejects_forged_incoming_token_even_when_holder_aliases_user() {
    use crate::abilities::{CheckPhase, CheckSide, CheckSite, Plan, TokenClaim};
    let mut game = fixture(&[], &[]);
    apply(&mut game, OWNER, "exposed", 1000);
    let incoming = game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .qualifying_tokens(game.content().rules(), OWNER, true)
        .pop()
        .unwrap();
    start(
        &mut game,
        OWNER,
        action("fixture_search", AbilityInvocation::actor_target(OWNER)),
    );
    let original = game.preparation(OWNER).unwrap().plan.clone();
    assert!(original.tokens().next().is_none());
    assert!(original.valid(game.content().rules()));
    let _ = checkpoint(&game);
    let mut forged = original;
    let Plan::Ability(plan) = &mut forged else {
        panic!("search ability")
    };
    plan.tokens.push(TokenClaim {
        reservation: incoming.clone(),
        site: CheckSite {
            strike: 0,
            check: CheckPhase::Primary {
                side: CheckSide::Initiating,
            },
        },
    });
    assert!(
        !forged.valid(game.content().rules()),
        "a self-target alias is not an incoming check participant"
    );
    {
        let mut owner = game.actors.get_mut(&OWNER).unwrap();
        owner
            .combat
            .as_mut()
            .unwrap()
            .reserve_tokens(&[incoming], OWNER)
            .unwrap();
        owner.pending.as_mut().unwrap().plan = forged;
    }
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    assert!(
        Game::restore_checkpoint(saved, &shared, game.content().clone()).is_err(),
        "even a matching live reservation cannot legitimize a forged incoming Search claim"
    );
}

#[test]
fn imposed_floor_contact_sleep_pauses_paid_search_and_preserves_its_reservation() {
    let mut game = fixture_with(&[], &[], |rules, owner, _| {
        rules.conditions.push(serde_json::from_value(serde_json::json!({
            "id":"fixture_sleep", "family":"fixture_sleep", "name":"Asleep", "lifecycle":"timed",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}}, "restrictions":["voluntary_actions"],
            "preparation":"pause", "modifiers":[],"stacking":{"kind":"extend"},"source_scoped":false,"descriptors":[],"capacity":"reject"
        })).unwrap());
        owner.modifiers.push(crate::rules::Modifier {
            scope: crate::rules::Scope::Always,
            op: crate::rules::ModifierOp::Locomotion {
                value: crate::locomotion::LocomotionValue::Flying { enabled: true },
            },
        });
    });
    let regions: Vec<_> = game
        .world
        .loaded_regions()
        .map(|id| game.world.region(id).unwrap().clone())
        .collect();
    let mut world = tor_world::World::new(vec![], vec![]).unwrap();
    for region in regions {
        world.add_chamber(region).unwrap();
    }
    world
        .place_trap(
            at(1, 2, 2),
            Trap {
                id: 11,
                dc: 100,
                kind: TrapKind::SleepingGas {
                    condition: "fixture_sleep".into(),
                    duration: 100,
                },
            },
        )
        .unwrap();
    game.world = Shared::new(world);
    apply(&mut game, OWNER, "readied", 1000);
    let intent = start(
        &mut game,
        OWNER,
        action("fixture_search", AbilityInvocation::actor_target(OWNER)),
    );
    let plan = game.preparation(OWNER).unwrap().plan.clone();
    let token = plan.tokens().next().unwrap().clone();
    assert!(game.preparation(OWNER).unwrap().active);
    assert!(
        game.actors[&OWNER].trap_contacts.ids().next().is_none(),
        "flight does not touch the floor"
    );
    // A valid subcell position just above the supported floor produces a real,
    // zero-damage imposed collision at the next physics tick, not a query trigger.
    game.actors.get_mut(&OWNER).unwrap().motion.displacement[2] = -(crate::physics::CELL - 1);
    game.set_actor_velocity(OWNER, [0, 0, -1]).unwrap();
    // The private physics engine normally runs inside an action/host publication.
    // Keep its condition events inside that same ordinary committed boundary.
    assert_eq!(
        game.with_effect_publication(|candidate| candidate.advance_physics(1))
            .unwrap(),
        1
    );
    assert!(game.event_receipts_valid());
    assert!(!game.condition_allows_voluntary_input(OWNER));
    assert_eq!(game.actors[&OWNER].location, at(1, 2, 2));
    let pending = game.preparation(OWNER).unwrap();
    assert!(!pending.active);
    assert_eq!(pending.plan, plan);
    assert_eq!(pending.plan.tokens().next(), Some(&token));
    assert!(game.feature_family_known(OWNER, ConcealedFeatureId::Trap(11)));
    // Physics is deliberately driven without scheduling the idle fixture peer.
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = game.tick;
    let mut game = checkpoint(&game);
    assert_eq!(
        game.with_effect_publication(|candidate| candidate.advance_physics(101))
            .unwrap(),
        101
    );
    // This test drives time instead of scheduling the idle peer's turns.
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = game.tick;
    assert!(game.condition_allows_voluntary_input(OWNER));
    assert!(
        !game.preparation(OWNER).unwrap().active,
        "waking does not resume paid work for free"
    );
    assert_eq!(game.preparation(OWNER).unwrap().plan, plan);
    let PlannedEffect::SearchAdjacentGuises { check } =
        plan.as_ability().unwrap().strikes[0].on_success[0]
    else {
        panic!("search")
    };
    let mut expected = game.rules_rng;
    check.roll(&mut expected, false);
    game.resume_intention(OWNER, intent).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let pending = game.preparation(OWNER).unwrap();
    game.tick = pending.started + pending.remaining;
    game.resolve_preparations().unwrap();
    assert_eq!(game.rules_rng, expected);
    assert!(game.preparation(OWNER).is_none());
    assert!(!game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .has_active(&token.condition));
}

#[test]
fn paid_search_obeys_normal_haste_and_slow_preparation_and_recovery() {
    use crate::action_duration::ActionDuration;
    for (duration, half) in [
        (ActionDuration::Normal, 50),
        (ActionDuration::Haste, 25),
        (ActionDuration::Slow, 100),
    ] {
        let mut game = fixture_with(&[], &[guise()], |_, owner, _| {
            owner.modifiers.push(crate::rules::Modifier {
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::ActionDuration { value: duration },
            });
        });
        let plan = game
            .ability_plan(OWNER, &id("fixture_search"), OWNER)
            .unwrap();
        assert_eq!((plan.preparation(), plan.recovery()), (half, half));
        start(
            &mut game,
            OWNER,
            action("fixture_search", AbilityInvocation::actor_target(OWNER)),
        );
        assert_eq!(game.preparation(OWNER).unwrap().remaining, half);
        game.tick = half;
        game.resolve_preparations().unwrap();
        assert!(game.preparation(OWNER).is_none());
        assert_eq!(game.actors[&OWNER].ready_at, 2 * half);
        assert!(game.actors[&SUBJECT].guise.appearance().is_none());
    }
}

#[test]
fn periodic_search_during_paid_preparation_never_borrows_or_spends_its_token() {
    let mut game = fixture_with(
        &[PerceptionValue::PeriodicSearch { interval: 100 }],
        &[],
        |_, owner, _| {
            owner.modifiers.push(crate::rules::Modifier {
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::ActionDuration {
                    value: crate::action_duration::ActionDuration::Slow,
                },
            });
        },
    );
    // Drive actor-active ring time without allowing the idle fixture peer to act.
    game.actors.get_mut(&OWNER).unwrap().ready_at = 50;
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = 50;
    assert_eq!(game.advance_physics(50).unwrap(), 50);
    apply(&mut game, OWNER, "readied", 1000);
    start(
        &mut game,
        OWNER,
        action("fixture_search", AbilityInvocation::actor_target(OWNER)),
    );
    assert_eq!(
        game.tick, 50,
        "the idle peer prevents start from advancing to the ring deadline"
    );
    assert_eq!(game.preparation(OWNER).unwrap().started, 50);
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = 100;
    let plan = game.preparation(OWNER).unwrap().plan.clone();
    let token = plan.tokens().next().unwrap().clone();
    let PlannedEffect::SearchAdjacentGuises { check: paid } =
        plan.as_ability().unwrap().strikes[0].on_success[0]
    else {
        panic!("search")
    };
    let automatic = check(&game);
    assert_eq!(paid.edge, automatic.edge + 1);
    let mut expected = game.rules_rng;
    automatic.roll(&mut expected, false);
    assert_eq!(game.advance_physics(100).unwrap(), 100);
    assert_eq!(
        game.rules_rng, expected,
        "periodic scan used no reserved edge"
    );
    let pending = game.preparation(OWNER).unwrap();
    assert!(pending.active);
    assert_eq!(pending.plan, plan);
    assert_eq!(pending.started + pending.remaining, 150);
    assert_eq!(
        game.creature(OWNER)
            .unwrap()
            .conditions()
            .instances()
            .find(|entry| entry.condition == token.condition)
            .unwrap()
            .reserved_by,
        Some(OWNER)
    );
    let mut game = checkpoint(&game);
    paid.roll(&mut expected, false);
    assert_eq!(game.advance_physics(150).unwrap(), 150);
    game.resolve_preparations().unwrap();
    assert_eq!(
        game.rules_rng, expected,
        "only the paid Search consumes the captured token"
    );
    assert!(game.preparation(OWNER).is_none());
    assert!(!game
        .creature(OWNER)
        .unwrap()
        .conditions()
        .has_active(&token.condition));
    assert_eq!(game.actors[&OWNER].ready_at, 250);
}

#[test]
fn one_moving_observer_does_not_rescan_unchanged_observers_cached_sight() {
    let mut game = fixture(&[], &[]);
    trap(&mut game, 11, at(1, 4, 2), 100);
    for ordinal in 0..32 {
        spawn(
            &mut game,
            "fixture_perception_subject",
            at(2, ordinal % 8, ordinal / 8),
        );
    }
    game.initialize_discovery().unwrap();
    let before = DISCOVERY_WORK.with(|count| count.get());
    game.teleport(OWNER, at(1, 2, 3)).unwrap();
    let after = DISCOVERY_WORK.with(|count| count.get());
    assert_eq!(
        after.0 - before.0,
        1,
        "another observer's movement must not clone and scan unchanged eye scenes"
    );
    let visible = game.scene_snapshot(OWNER).unwrap().1.len();
    assert_eq!(
        after.1 - before.1,
        visible,
        "candidate-cell work belongs only to the moved observer"
    );

    let before = DISCOVERY_WORK.with(|count| count.get());
    game.actors.get_mut(&OWNER).unwrap().orientation = 1;
    game.reconcile_passive_discovery().unwrap();
    assert_eq!(DISCOVERY_WORK.with(|count| count.get()).0 - before.0, 1);
    let mut restored = checkpoint(&game);
    let rng = restored.rules_rng;
    let before = DISCOVERY_WORK.with(|count| count.get());
    restored.reconcile_passive_discovery().unwrap();
    assert_eq!(
        DISCOVERY_WORK.with(|count| count.get()).0 - before.0,
        restored.actors.len(),
        "source-free restore rebuilds only derived witnesses"
    );
    assert_eq!(
        restored.rules_rng, rng,
        "rebuilt caches cannot erase first-observation receipts"
    );
}

#[test]
fn discovery_cache_retains_world_lighting_occlusion_and_waking_dependencies() {
    let mut game = fixture(&[], &[]);
    let feature = trap(&mut game, 11, at(1, 6, 2), 0);
    for y in 0..7 {
        game.world.set_wall(at(1, 4, y), true).unwrap();
    }
    game.world.set_cell_light(at(1, 6, 2), false).unwrap();
    game.initialize_discovery().unwrap();
    assert_eq!(attempts(&game, OWNER), 0);
    let before = DISCOVERY_WORK.with(|count| count.get());
    game.set_wall(at(1, 4, 2), false).unwrap();
    assert_eq!(DISCOVERY_WORK.with(|count| count.get()).0 - before.0, 2);
    assert_eq!(
        attempts(&game, OWNER),
        0,
        "an opened view still needs illuminated cues"
    );
    let before = DISCOVERY_WORK.with(|count| count.get());
    game.set_cell_light(at(1, 6, 2), true).unwrap();
    assert_eq!(DISCOVERY_WORK.with(|count| count.get()).0 - before.0, 2);
    assert!(game.feature_known(OWNER, feature));

    let mut game = fixture_with(&[], &[], |rules, _, _| {
        rules.conditions.push(serde_json::from_value(serde_json::json!({
            "id":"fixture_sleep", "family":"fixture_sleep", "name":"Asleep", "lifecycle":"timed",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}}, "restrictions":["voluntary_actions"],
            "preparation":"pause", "modifiers":[],"stacking":{"kind":"extend"},"source_scoped":false,"descriptors":[],"capacity":"reject"
        })).unwrap());
    });
    let feature = trap(&mut game, 11, at(1, 4, 2), 0);
    apply(&mut game, OWNER, "fixture_sleep", 10);
    game.initialize_discovery().unwrap();
    assert_eq!(attempts(&game, OWNER), 0);
    let before = DISCOVERY_WORK.with(|count| count.get());
    game.tick = 10;
    game.settle_actor_time(OWNER).unwrap();
    game.reconcile_passive_discovery().unwrap();
    assert_eq!(
        DISCOVERY_WORK.with(|count| count.get()).0 - before.0,
        1,
        "waking reconciles its own deferred first observation"
    );
    assert!(game.feature_known(OWNER, feature));
}
