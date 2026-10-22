//! Active tactile discovery is local, paid (or explicitly periodic), and weaker
//! than an adjacent visual check. It never grants tactile creature recognition.
use super::*;

fn conceal_adjacent_door(game: &mut Game, dc: i32) -> ConcealedFeatureId {
    let location = at(1, 1, 2);
    let door = game.place_authored_door(33, location, false, 1).unwrap();
    game.world
        .conceal_door(location, Concealment { dc })
        .unwrap();
    ConcealedFeatureId::Door(door)
}

fn darken_local_features(game: &mut Game) {
    for location in [at(1, 2, 3), at(1, 1, 2), at(1, 4, 2)] {
        game.world.set_cell_light(location, false).unwrap();
    }
}

#[test]
fn adjacent_tactile_uses_middle_total_once_for_darkness_blindness_or_both() {
    // Independent supplied totals distinguish the adjacent +1, ordinary +0,
    // and distant -1 columns. A tactile check must use exactly the middle one,
    // including when darkness and blindness coincide.
    for (dark, blind) in [(false, false), (true, false), (false, true), (true, true)] {
        let values = if blind {
            vec![PerceptionValue::Sight { enabled: false }]
        } else {
            vec![]
        };
        let mut game = fixture(&values, &[]);
        let low = trap(&mut game, 11, at(1, 2, 3), 10);
        let high = conceal_adjacent_door(&mut game, 11);
        if dark {
            darken_local_features(&mut game);
        }
        let before_rng = game.rules_rng;
        game.apply_discovery_roll(
            OWNER,
            DiscoveryRoll {
                near: 20,
                middle: 10,
                far: 0,
            },
        )
        .unwrap();
        assert!(game.feature_known(OWNER, low), "dark={dark}, blind={blind}");
        assert_eq!(game.feature_known(OWNER, high), !dark && !blind);
        assert_eq!(
            game.rules_rng, before_rng,
            "applying a sample never rerolls"
        );
        assert!(!game.feature_known(SUBJECT, low));
        assert!(!game.world.door(at(1, 1, 2)).unwrap().open);
    }
}

#[test]
fn real_paid_search_feels_adjacent_trap_and_secret_without_sight() {
    for blind in [false, true] {
        let values = if blind {
            vec![PerceptionValue::Sight { enabled: false }]
        } else {
            vec![]
        };
        let mut game = fixture(&values, &[]);
        let nearby = trap(&mut game, 11, at(1, 2, 3), 0);
        let remote = trap(&mut game, 12, at(1, 4, 2), 0);
        let door = conceal_adjacent_door(&mut game, 0);
        darken_local_features(&mut game);
        let before_rng = game.rules_rng;
        game.reconcile_passive_discovery_for(OWNER).unwrap();
        assert_eq!(attempts(&game, OWNER), 0);
        assert_eq!(game.rules_rng, before_rng);
        assert!(!game.feature_known(OWNER, nearby));
        let before_tick = game.tick;
        paid(&mut game);
        assert!(
            game.tick > before_tick,
            "tactile discovery still pays preparation"
        );
        assert!(
            game.actors[&OWNER].ready_at > game.tick,
            "normal recovery remains"
        );
        assert_ne!(game.rules_rng, before_rng, "paid Search samples its check");
        assert!(game.feature_known(OWNER, nearby));
        assert!(game.feature_known(OWNER, door));
        assert!(
            !game.feature_known(OWNER, remote),
            "touch does not reach two cells"
        );
        assert!(!game.feature_family_known(OWNER, nearby));
        assert!(!game.feature_known(SUBJECT, nearby));
        assert!(!game.world.door(at(1, 1, 2)).unwrap().open);
    }
}

#[test]
fn passive_dark_or_blind_adjacency_never_spends_a_discovery_attempt() {
    for blind in [false, true] {
        let values = if blind {
            vec![PerceptionValue::Sight { enabled: false }]
        } else {
            vec![]
        };
        let mut game = fixture(&values, &[]);
        let nearby = trap(&mut game, 11, at(1, 2, 3), 0);
        conceal_adjacent_door(&mut game, 0);
        darken_local_features(&mut game);
        let before = game.clone();
        game.reconcile_passive_discovery_for(OWNER).unwrap();
        assert_eq!(game.rules_rng, before.rules_rng);
        assert_eq!(
            game.actors[&OWNER].discovery,
            before.actors[&OWNER].discovery
        );
        assert!(!game.feature_known(OWNER, nearby));
        assert_eq!(attempts(&game, OWNER), 0);
    }
}

#[test]
fn tactile_search_never_recognizes_an_adjacent_creature_guise() {
    for blind in [false, true] {
        let values = if blind {
            vec![PerceptionValue::Sight { enabled: false }]
        } else {
            vec![]
        };
        let mut game = fixture(&values, &[guise()]);
        let nearby = trap(&mut game, 11, at(1, 2, 3), 0);
        darken_local_features(&mut game);
        game.world.set_cell_light(at(1, 3, 2), false).unwrap();
        let subject = game.actors[&SUBJECT].guise.clone();
        game.apply_discovery_roll(
            OWNER,
            DiscoveryRoll {
                near: 1000,
                middle: 1000,
                far: 1000,
            },
        )
        .unwrap();
        assert!(
            game.feature_known(OWNER, nearby),
            "positive tactile feature witness"
        );
        assert!(!game.guise_known(OWNER, SUBJECT));
        assert_eq!(game.actors[&SUBJECT].guise, subject);
    }
}

#[test]
fn periodic_active_algorithm_shares_tactile_candidates_and_one_sample() {
    // This directly owns the active algorithm called by the periodic ring;
    // existing periodic scheduling tests own its clock/token lifecycle.
    let mut game = fixture(&[], &[guise()]);
    let nearby = trap(&mut game, 11, at(1, 2, 3), 0);
    let remote = trap(&mut game, 12, at(1, 4, 2), 0);
    let door = conceal_adjacent_door(&mut game, 0);
    darken_local_features(&mut game);
    game.world.set_cell_light(at(1, 3, 2), false).unwrap();
    let captured = check(&game);
    let mut expected_rng = game.rules_rng;
    captured.sample(&mut expected_rng);
    let tick = game.tick;
    game.run_active_discovery(OWNER, &captured).unwrap();
    assert_eq!(game.rules_rng, expected_rng);
    assert_eq!(
        game.tick, tick,
        "automatic Search does not create a paid action"
    );
    assert!(game.feature_known(OWNER, nearby));
    assert!(game.feature_known(OWNER, door));
    assert!(!game.feature_known(OWNER, remote));
    assert!(!game.guise_known(OWNER, SUBJECT));
    assert!(!game.world.door(at(1, 1, 2)).unwrap().open);
}

#[test]
fn automatic_search_retains_blindness_eligibility_gate() {
    let mut game = fixture(&[PerceptionValue::Sight { enabled: false }], &[]);
    let nearby = trap(&mut game, 11, at(1, 2, 3), 0);
    let before = game.clone();
    game.run_active_discovery(OWNER, &check(&before)).unwrap();
    assert_eq!(game.rules_rng, before.rules_rng);
    assert!(!game.feature_known(OWNER, nearby));
}

#[test]
fn tactile_diagonal_requires_clear_component_routes_and_never_crosses_a_closed_surface() {
    for blocked in [false, true] {
        let mut game = fixture(&[PerceptionValue::Sight { enabled: false }], &[]);
        // Establish chamber-backed trap storage before adding terrain/doors:
        // the shared fixture helper converts plain regions on its first trap.
        let nearby = trap(&mut game, 11, at(1, 2, 3), 0);
        let behind = trap(&mut game, 12, at(1, 1, 4), 0);
        let location = at(1, 1, 3);
        let door = game.place_authored_door(33, location, false, 1).unwrap();
        game.world
            .conceal_door(location, Concealment { dc: 0 })
            .unwrap();
        if blocked {
            game.world.set_wall(at(1, 1, 2), true).unwrap();
        }
        assert!(game.world.door(location).is_some());
        assert_eq!(game.world.is_wall(at(1, 1, 2)), blocked);
        game.apply_discovery_total(OWNER, 100).unwrap();
        assert!(game.feature_known(OWNER, nearby));
        assert_eq!(
            game.feature_known(OWNER, ConcealedFeatureId::Door(door)),
            !blocked
        );
        assert!(!game.feature_known(OWNER, behind));
    }
}

#[test]
fn tactile_search_uses_every_physical_body_cell_not_the_reference_radius() {
    for reverse in [false, true] {
        let mut game = fixture(&[PerceptionValue::Sight { enabled: false }], &[]);
        let mut cells = vec![[0, 0, 0], [0, 1, 0]];
        if reverse {
            cells.reverse();
        }
        game.set_body(
            OWNER,
            crate::BodySpec {
                cells,
                ..Default::default()
            },
        )
        .unwrap();
        let near_limb = trap(&mut game, 11, at(1, 2, 4), 0);
        let remote = trap(&mut game, 12, at(1, 2, 5), 0);
        let underfoot = trap(&mut game, 13, at(1, 2, 2), 0);
        game.apply_discovery_total(OWNER, 100).unwrap();
        assert!(game.feature_known(OWNER, near_limb));
        assert!(game.feature_known(OWNER, underfoot));
        assert!(!game.feature_known(OWNER, remote));
        assert!(
            game.actors[&OWNER].trap_contacts.ids().next().is_none(),
            "searching does not physically enter or trigger a trap"
        );
    }
}

#[test]
fn tactile_search_never_uses_an_abstract_stair_projection() {
    let mut game = fixture(&[PerceptionValue::Sight { enabled: false }], &[]);
    let landing = at(2, 2, 2);
    let remote = trap(&mut game, 11, landing, 0);
    let local = trap(&mut game, 12, at(1, 2, 3), 0);
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
    game.apply_discovery_total(OWNER, 100).unwrap();
    assert!(game.feature_known(OWNER, local));
    assert!(!game.feature_known(OWNER, remote));
}

#[test]
fn tactile_rotated_physical_join_reaches_the_closed_surface_at_both_body_heights() {
    use tor_world::{Extent, Passage, Region, RegionId, World};
    for blind in [false, true] {
        let content = fixture(&[PerceptionValue::Sight { enabled: !blind }], &[])
            .content()
            .clone();
        let turn = (0..24)
            .find(|&r| {
                tor_world::rotate_vector(r, [1, 0, 0]) == [0, 1, 0]
                    && tor_world::rotate_vector(r, [0, 1, 0]) == [-1, 0, 0]
            })
            .unwrap();
        let mut world = World::new(
            vec![
                Region {
                    id: RegionId(1),
                    name: "near".into(),
                    bounds: Extent::new(3, 5, 2).unwrap(),
                },
                Region {
                    id: RegionId(2),
                    name: "far".into(),
                    bounds: Extent::new(5, 3, 2).unwrap(),
                },
            ],
            vec![],
        )
        .unwrap();
        world
            .connect_portal_area(
                Passage {
                    from: at(1, 2, 0),
                    direction: Direction::East,
                    to: at(2, 4, 0),
                },
                turn,
                5,
                2,
            )
            .unwrap();
        let mut game = Game::new(world, 42, content);
        assert_eq!(
            spawn(&mut game, "fixture_perception_owner", at(1, 2, 2)),
            OWNER
        );
        game.set_body(
            OWNER,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                ..Default::default()
            },
        )
        .unwrap();
        let surface = at(2, 2, 0);
        assert_eq!(
            game.world.physics_neighbor(at(1, 2, 2), Direction::East),
            Some((surface, turn))
        );
        let door = game.place_authored_door(33, surface, false, 2).unwrap();
        game.world
            .conceal_door(
                surface,
                Concealment {
                    dc: if blind { 0 } else { 15 },
                },
            )
            .unwrap();
        for z in 0..2 {
            let mut origin = at(1, 2, 2);
            origin.position.z = z;
            game.world.set_cell_light(origin, false).unwrap();
        }
        let before_rng = game.rules_rng;
        game.apply_discovery_roll(
            OWNER,
            DiscoveryRoll {
                near: 20,
                middle: 10,
                far: 0,
            },
        )
        .unwrap();
        assert_eq!(game.rules_rng, before_rng);
        assert!(game.feature_known(OWNER, ConcealedFeatureId::Door(door)));
        assert_eq!(
            game.actors[&OWNER].discovery.0.len(),
            1,
            "tall aliases deduplicate"
        );
        assert!(!game.world.door(surface).unwrap().open);
        let restored = checkpoint(&game);
        assert_eq!(
            restored.actors[&OWNER].discovery,
            game.actors[&OWNER].discovery
        );
    }
}

#[test]
fn illuminated_adjacent_cue_keeps_visual_advantage_when_body_origin_is_dark() {
    for body_lit in [true, false] {
        let mut game = fixture(&[], &[]);
        let feature = trap(&mut game, 11, at(1, 2, 3), 15);
        game.world.set_cell_light(at(1, 2, 2), body_lit).unwrap();
        game.apply_discovery_roll(
            OWNER,
            DiscoveryRoll {
                near: 20,
                middle: 10,
                far: 0,
            },
        )
        .unwrap();
        assert!(game.feature_known(OWNER, feature), "body_lit={body_lit}");
    }
}

#[test]
fn mixed_visual_and_tactile_features_share_one_correlated_sample_and_penalty() {
    let mut distinct_outcomes = 0;
    for seed in 0..16 {
        let mut game = fixture(&[], &[]);
        let tactile = trap(&mut game, 11, at(1, 2, 3), 12);
        game.world.set_cell_light(at(1, 2, 3), false).unwrap();
        let visual = conceal_adjacent_door(&mut game, 12);
        game.rules_rng = seed;
        let captured = FrozenDiscoveryCheck { bonus: 0, edge: 0 };
        let mut expected_rng = seed;
        let sample = captured.sample(&mut expected_rng);
        game.run_active_discovery(OWNER, &captured).unwrap();
        assert_eq!(
            game.rules_rng, expected_rng,
            "one sample for both modalities"
        );
        assert_eq!(game.feature_known(OWNER, visual), sample.near >= 12);
        assert_eq!(game.feature_known(OWNER, tactile), sample.middle >= 12);
        distinct_outcomes +=
            usize::from(game.feature_known(OWNER, visual) && !game.feature_known(OWNER, tactile));
    }
    assert!(
        distinct_outcomes > 0,
        "fixed matrix witnesses a real disadvantage"
    );
}

#[test]
fn tactile_search_cannot_reach_through_another_body() {
    let mut game = fixture(&[PerceptionValue::Sight { enabled: false }], &[]);
    let obstructed = trap(&mut game, 11, at(1, 3, 2), 0);
    let nearby = trap(&mut game, 12, at(1, 2, 3), 0);
    game.apply_discovery_total(OWNER, 100).unwrap();
    assert!(game.feature_known(OWNER, nearby));
    assert!(!game.feature_known(OWNER, obstructed));
}
