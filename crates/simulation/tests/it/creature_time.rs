use std::{collections::BTreeSet, num::NonZeroU64};
use tor_simulation::creatures::Template;
use tor_simulation::grants::Grant;
use tor_simulation::resources::Resource;
use tor_simulation::{Action, ActorId, CreatureIdentity, Game, MemoryRecords, RegionTransition};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

fn game() -> (Game, ActorId, ActorId) {
    let world = World::new(
        (1..=3)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("room{id}"),
                bounds: Extent::new(8, 3, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let mut game = Game::new(world, 42);
    let mut ids = vec![];
    for (region, ticks) in [(1, 99), (3, 150)] {
        let id = game
            .spawn_actor(
                Location {
                    region: RegionId(region),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(ticks).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: format!("subject{region}"),
                faction: "neutral".into(),
            },
            super::creature_state::build(),
        )
        .unwrap();
        ids.push(id);
    }
    game.configure_run(ids[0], BTreeSet::from([ids[0]]), None, Default::default())
        .unwrap();
    game.add_default_reference_points().unwrap();
    game.refresh_navigation();
    let mut build = game.creature(ids[1]).unwrap().build().clone();
    let mut template = Template::new("capacity", 0);
    template.grants = vec![Grant::Stamina(2), Grant::Focus(2), Grant::Mana(2)];
    build.set_templates(vec![template]).unwrap();
    game.rebuild_creature(ids[1], build).unwrap();
    game.apply_fear_condition(ids[1], ids[0], 400).unwrap();
    (game, ids[0], ids[1])
}

fn advance(game: &mut Game, until: u64) {
    while game.tick() < until {
        let id = game.next_actor().unwrap();
        game.act(id, Action::Wait).unwrap();
    }
}

fn round_trip(game: &Game) -> Game {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let data = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&data).unwrap();
    Game::restore_checkpoint(snapshot, &shared).unwrap()
}

#[test]
fn timer_deadlines_survive_checkpoint_and_shift_only_after_thaw() {
    let (mut game, player, subject) = game();
    assert_eq!(game.next_creature_event_tick(), Some(100));
    advance(&mut game, 99);
    assert_eq!(game.next_creature_event_tick(), Some(100));
    game = round_trip(&game);
    assert_eq!(game.next_creature_event_tick(), Some(100));
    let mut records = MemoryRecords::default();
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(game.next_creature_event_tick(), None);
    for _ in 0..5 {
        game.act(player, Action::Wait).unwrap();
    }
    assert_eq!(game.tick(), 594);
    game = round_trip(&game);
    assert_eq!(game.next_creature_event_tick(), None);
    game.transition_regions(
        &RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(3)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(game.next_creature_event_tick(), Some(595));
    game.act(player, Action::Wait).unwrap();
    assert_eq!(game.tick(), 645);
    assert_eq!(game.next_creature_event_tick(), Some(695));
    assert_eq!(
        game.creature(subject).unwrap().fear().remaining(player),
        Some(250)
    );
    advance(&mut game, 2500);
    assert_eq!(game.next_creature_event_tick(), None);
}

#[test]
fn recovery_and_fear_follow_fast_forward_resting_physics_and_moving_physics_equally() {
    for mode in 0..3 {
        let (mut game, source, subject) = game();
        if mode == 1 {
            game.set_gravity(RegionId(3), [0, 0, -1]).unwrap();
        }
        if mode == 2 {
            game.set_actor_velocity(subject, [512, 0, 0]).unwrap();
        }
        advance(&mut game, 99);
        assert_eq!(game.tick(), 99);
        let state = game.creature(subject).unwrap();
        for resource in Resource::ALL {
            assert_eq!(state.costs().resources().balance(resource), 4);
            assert_eq!(state.costs().resources().recovery_elapsed(resource), 99);
        }
        assert_eq!(state.fear().remaining(source), Some(301));
        let mut restored = round_trip(&game);
        advance(&mut game, 2000);
        advance(&mut restored, 2000);
        assert_eq!(game.creature(subject), restored.creature(subject));
        let state = game.creature(subject).unwrap();
        for resource in Resource::ALL {
            assert_eq!(state.costs().resources().balance(resource), 6);
            assert_eq!(state.costs().resources().recovery_elapsed(resource), 0);
        }
        assert!(state.fear().sources().is_empty());
    }
}

#[test]
fn frozen_and_detached_timers_keep_fractional_progress_without_thaw_catch_up() {
    for detach in [false, true] {
        let (mut game, player, subject) = game();
        advance(&mut game, 99);
        let before = game.creature(subject).unwrap().clone();
        let mut records = MemoryRecords::default();
        game.transition_regions(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1)]),
                loaded: if detach {
                    BTreeSet::from([RegionId(1)])
                } else {
                    BTreeSet::from([RegionId(1), RegionId(3)])
                },
            },
            &mut records,
        )
        .unwrap();
        for _ in 0..5 {
            game.act(player, Action::Wait).unwrap();
        }
        assert_eq!(game.tick(), 594);
        if !detach {
            assert_eq!(game.creature(subject), Some(&before));
        }
        game = round_trip(&game);
        game.transition_regions(
            &RegionTransition {
                active: BTreeSet::from([RegionId(1), RegionId(3)]),
                loaded: BTreeSet::from([RegionId(1), RegionId(3)]),
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(game.creature(subject), Some(&before));
        game.act(player, Action::Wait).unwrap();
        assert_eq!(game.tick(), 645);
        let state = game.creature(subject).unwrap();
        assert_eq!(state.costs().resources().balance(Resource::Stamina), 5);
        assert_eq!(
            state
                .costs()
                .resources()
                .recovery_elapsed(Resource::Stamina),
            50
        );
        assert_eq!(state.costs().resources().balance(Resource::Focus), 4);
        assert_eq!(
            state.costs().resources().recovery_elapsed(Resource::Focus),
            150
        );
        assert_eq!(state.fear().remaining(player), Some(250));
    }
}

#[test]
fn fear_expiring_at_resolution_does_not_penalize_the_check() {
    for duration in [50, 100] {
        let mut game = Game::two_room_in_stone(42);
        for x in 1..=2 {
            let actor = game
                .spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                CreatureIdentity {
                    name: format!("subject{x}"),
                    faction: "neutral".into(),
                },
                super::creature_state::build(),
            )
            .unwrap();
        }
        game.refresh_navigation();
        game.apply_fear_condition(ActorId(1), ActorId(2), duration)
            .unwrap();
        let state = game.creature(ActorId(1)).unwrap();
        let damage = state.derived().melee_damage(0).unwrap();
        let mut rng = 42;
        let expected = tor_simulation::damage::AttackCheck {
            check: tor_simulation::attributes::SkillCheck {
                skill: state.build().species().melee.skill(),
                binding: state.build().binding(),
                modifier: 0,
                threshold: game
                    .creature(ActorId(2))
                    .unwrap()
                    .derived()
                    .defenses
                    .physical,
            },
            attributes: state.derived().attributes,
            skills: state.derived().skills,
        }
        .resolve(
            &mut rng,
            tor_simulation::dice::Edge::from_counts(0, u32::from(duration > 50)),
            &damage,
            &game.creature(ActorId(2)).unwrap().derived().protection,
        );
        game.act(ActorId(1), Action::Attack { target: ActorId(2) })
            .unwrap();
        game.act(ActorId(2), Action::Wait).unwrap();
        assert!(game.combat_events().iter().any(|event| matches!(event, tor_simulation::combat::CombatEvent::Resolved { hit, damage, .. } if *hit == expected.check.success && *damage == expected.damage.as_ref().map_or(0, |damage| damage.total))));
    }
}

#[test]
fn intermediate_expiry_does_not_stop_the_loop_while_all_actors_prepare_attacks() {
    let mut game = Game::two_room_in_stone(42);
    // Keep both attacks as misses so an ordinary hit interruption cannot mask
    // whether an intermediate timer returned before preparation completed.
    let mut build = super::creature_state::build();
    let mut protection = Template::new("test_defense", 0);
    protection.grants = vec![Grant::PhysicalDefense(100)];
    build.set_templates(vec![protection]).unwrap();
    for x in 1..=2 {
        let id = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            id,
            CreatureIdentity {
                name: format!("subject{x}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game.refresh_navigation();
    game.apply_fear_condition(ActorId(1), ActorId(2), 10)
        .unwrap();
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    game.act(ActorId(2), Action::Attack { target: ActorId(1) })
        .unwrap();
    assert!(
        game.tick() >= 50,
        "stopped at an internal timer before attacks resolved"
    );
    assert!(game.next_actor().is_some());
    assert_eq!(
        game.combat_events()
            .iter()
            .filter(|event| matches!(event, tor_simulation::combat::CombatEvent::Resolved { .. }))
            .count(),
        2
    );
}

#[test]
fn terminal_physics_stops_timer_credit_at_the_actual_tick_and_fear_outlives_its_causer() {
    let (mut game, player, subject) = game();
    game.teleport(
        player,
        Location {
            region: RegionId(1),
            position: Position { x: 7, y: 1, z: 0 },
        },
    )
    .unwrap();
    let injury = game.health(player).unwrap().0 - 1;
    game.apply_effects(
        player,
        &[tor_simulation::EffectSpec::Damage {
            components: std::collections::BTreeMap::from([(
                tor_simulation::combat::DamageType::Impact,
                injury,
            )]),
        }],
    )
    .unwrap();
    game.set_actor_velocity(player, [8192, 0, 0]).unwrap();
    game.act(player, Action::Wait).unwrap();
    game.act(subject, Action::Wait).unwrap();
    assert!((1..99).contains(&game.tick()));
    assert!(!game.alive(player));
    assert!(game.run_outcome().terminal);
    let state = game.creature(subject).unwrap();
    assert_eq!(state.fear().remaining(player), Some(400 - game.tick()));
    for resource in Resource::ALL {
        assert_eq!(state.costs().resources().balance(resource), 4);
        assert_eq!(
            state.costs().resources().recovery_elapsed(resource),
            game.tick()
        );
    }
    let restored = round_trip(&game);
    assert_eq!(restored.creature(subject), game.creature(subject));
}
