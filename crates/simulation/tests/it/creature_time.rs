use std::{collections::BTreeSet, num::NonZeroU64};
use tor_simulation::content_ids::{AbilityId, TemplateId};
use tor_simulation::creatures::Template;
use tor_simulation::resources::Resource;
use tor_simulation::rules::{DefenseKind, Modifier, ModifierOp, Scope, Stat};
use tor_simulation::{Action, ActorId, CreatureIdentity, Game, MemoryRecords, RegionTransition};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

pub(super) fn game() -> (Game, ActorId, ActorId) {
    let original = super::creature_state::build();
    let mut template = Template::new(TemplateId::new("capacity").unwrap(), 0);
    template.modifiers = Resource::ALL
        .into_iter()
        .map(|resource| Modifier {
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::ResourceMax { resource },
                value: 2,
            },
        })
        .collect();
    let enhanced = super::creature_state::with_template(&original, template);
    let mut base = enhanced.clone();
    base.set_templates(vec![]).unwrap();
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
    let mut game = Game::new(world, 42, base.content().clone());
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
            base.clone(),
        )
        .unwrap();
        ids.push(id);
    }
    game.configure_run(ids[0], BTreeSet::from([ids[0]]), None, Default::default())
        .unwrap();
    game.add_default_reference_points().unwrap();
    game.refresh_navigation();
    game.rebuild_creature(ids[1], enhanced).unwrap();
    game.apply_condition(
        ids[1],
        &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
        ids[0],
        400,
    )
    .unwrap();
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
    Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap()
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
        game.creature(subject).unwrap().conditions().remaining(
            &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            player
        ),
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
        assert_eq!(
            state.conditions().remaining(
                &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
                source
            ),
            Some(301)
        );
        let mut restored = round_trip(&game);
        advance(&mut game, 2000);
        advance(&mut restored, 2000);
        assert_eq!(game.creature(subject), restored.creature(subject));
        let state = game.creature(subject).unwrap();
        for resource in Resource::ALL {
            assert_eq!(state.costs().resources().balance(resource), 6);
            assert_eq!(state.costs().resources().recovery_elapsed(resource), 0);
        }
        assert!(state.conditions().is_empty());
    }
}

#[test]
fn frozen_and_detached_timers_keep_fractional_progress_without_thaw_catch_up() {
    for detach in [false, true] {
        let (mut game, player, subject) = game();
        advance(&mut game, 99);
        let before = game.creature(subject).unwrap().into_owned();
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
            assert_eq!(game.creature(subject).as_deref(), Some(&before));
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
        assert_eq!(game.creature(subject).as_deref(), Some(&before));
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
        assert_eq!(
            state.conditions().remaining(
                &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
                player
            ),
            Some(250)
        );
    }
}

#[test]
fn condition_expiry_keeps_the_check_operands_owned_by_preparation() {
    for duration in [50, 100] {
        let mut game =
            Game::two_room_in_stone(42, super::creature_state::build().content().clone());
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
        game.apply_condition(
            ActorId(1),
            &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(2),
            duration,
        )
        .unwrap();
        let ability = AbilityId::new("basic_melee").unwrap();
        let admitted = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
        let mut rng = 42;
        let defender = game.creature(ActorId(2)).unwrap();
        let snapshot = tor_simulation::abilities::Snapshot::capture(&defender);
        let expected = tor_simulation::abilities::resolve(
            admitted.resolution().unwrap(),
            &snapshot,
            &mut rng,
            &mut (),
        )
        .unwrap();
        game.act(
            ActorId(1),
            Action::UseAbility {
                id: ability,
                invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                    target: ActorId(2),
                },
                supplies: Default::default(),
            },
        )
        .unwrap();
        game.act(ActorId(2), Action::Wait).unwrap();
        assert!(
            game.combat_events().iter().any(|event| matches!(event,
            tor_simulation::combat::CombatEvent::Resolved { outcome, .. } if outcome == &expected))
        );
    }
}

#[test]
fn intermediate_expiry_does_not_stop_the_loop_while_all_actors_prepare_attacks() {
    // Keep both attacks as misses so an ordinary hit interruption cannot mask
    // whether an intermediate timer returned before preparation completed.
    let mut build = super::creature_state::build();
    let mut protection = Template::new(TemplateId::new("test_defense").unwrap(), 0);
    protection.modifiers = vec![Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Defense {
                defense: DefenseKind::Physical,
            },
            value: 100,
        },
    }];
    build = super::creature_state::with_template(&build, protection);
    let mut game = Game::two_room_in_stone(42, build.content().clone());
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
    game.apply_condition(
        ActorId(1),
        &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
        ActorId(2),
        10,
    )
    .unwrap();
    game.act(
        ActorId(1),
        Action::UseAbility {
            id: AbilityId::new("basic_melee").unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                target: ActorId(2),
            },
            supplies: Default::default(),
        },
    )
    .unwrap();
    game.act(
        ActorId(2),
        Action::UseAbility {
            id: AbilityId::new("basic_melee").unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                target: ActorId(1),
            },
            supplies: Default::default(),
        },
    )
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
        &[tor_simulation::rules::Effect::Damage {
            damage: tor_simulation::rules::DamageExpr::Components {
                components: vec![tor_simulation::rules::DamageComponentExpr {
                    category: tor_simulation::combat::DamageType::Impact,
                    descriptor: None,
                    amount: tor_simulation::rules::AmountExpr::Fixed {
                        amount: u32::try_from(injury).unwrap(),
                    },
                }],
            },
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
    assert_eq!(
        state.conditions().remaining(
            &tor_simulation::content_ids::ConditionId::new("fear").unwrap(),
            player
        ),
        Some(400 - game.tick())
    );
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
