//! Shared one-cell movement contracts. Added before production implementation.
use serde_json::json;

#[test]
fn spatial_effects_have_closed_named_shapes() {
    for valid in [
        json!({"kind": "step_self_one_cell"}),
        json!({"kind": "push_target_one_cell", "direction": "away_from_source"}),
    ] {
        let bytes = serde_json::to_vec(&valid).unwrap();
        let effect: crate::rules::Effect = serde_json::from_slice(&bytes)
            .expect("the shared effect vocabulary includes one-cell translation");
        assert_eq!(serde_json::to_value(effect).unwrap(), valid);
    }
    for invalid in [
        json!({"kind": "step_self_one_cell", "distance": 2}),
        json!({"kind": "push_target_one_cell"}),
        json!({"kind": "push_target_one_cell", "direction": "east"}),
        json!({"kind": "push_target_one_cell", "direction": "away_from_source", "damage": 3}),
        json!(["step_self_one_cell"]),
    ] {
        assert!(serde_json::from_slice::<crate::rules::Effect>(
            &serde_json::to_vec(&invalid).unwrap()
        )
        .is_err());
    }
}

#[test]
fn ability_invocations_decode_current_named_selection_only() {
    for invocation in [
        json!({"kind": "actor_target", "target": 2}),
        json!({"kind": "self_step", "direction": "east"}),
    ] {
        let valid = json!({"type": "use_ability", "value": {
            "id": "guarded_step", "invocation": invocation,
            "supplies": {"kind": "carried"}
        }});
        let action: crate::Action = serde_json::from_slice(&serde_json::to_vec(&valid).unwrap())
            .expect("an invocation is an independently typed selected operand");
        assert_eq!(serde_json::to_value(action).unwrap(), valid);
    }
    for invocation in [
        json!({"kind": "self_step"}),
        json!({"kind": "self_step", "direction": "east", "target": 2}),
        json!({"kind": "self_step", "direction": "east", "to": [3, 4, 0]}),
        json!({"kind": "actor_target", "target": 2, "direction": "east"}),
        json!(["self_step", "east"]),
    ] {
        let invalid = json!({"type": "use_ability", "value": {
            "id": "guarded_step", "invocation": invocation,
            "supplies": {"kind": "carried"}
        }});
        assert!(
            serde_json::from_slice::<crate::Action>(&serde_json::to_vec(&invalid).unwrap())
                .is_err()
        );
    }
}

fn cell(x: i32, y: i32, z: i32) -> tor_world::Location {
    tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x, y, z },
    }
}

fn guarded_step_game(move_ticks: u64) -> crate::Game {
    guarded_step_game_with(move_ticks, |_| {})
}
pub(super) fn guarded_step_game_with(
    move_ticks: u64,
    configure: impl FnOnce(&mut serde_json::Value),
) -> crate::Game {
    use crate::attributes::{ManaBinding, Skill};
    use crate::creatures::{Content, CreatureBuild, ScenarioCatalog};
    use crate::progression::{HdLedger, HdSource};
    use crate::rules::{Modifier, ModifierOp, Ruleset, Scope, Stat};
    use std::num::NonZeroU64;
    use tor_world::{Extent, Region, RegionId, World};

    let mut document = serde_json::to_value(crate::test_creatures::ruleset().document()).unwrap();
    let mut definition = document["abilities"][0].clone();
    definition["id"] = json!("guarded_step_probe");
    definition["name"] = json!("Guarded Step Probe");
    definition["basic_attack"] = json!(false);
    definition["invocation"] = json!("self_step");
    definition["cost"] = json!({"resource":"stamina","start":1,"resolution":1});
    definition["timing"] = json!({
        "source":{"kind":"fixed","prepare":100,"recover":100},
        "scaled_by":"speed"
    });
    definition["reach"] = json!({"kind":"self_only"});
    definition["strikes"] = json!({"kind":"authored","strikes":[{
        "offset":0,"unlock":null,"check":null,
        "on_success":[{"kind":"step_self_one_cell"},
            {"kind":"apply_condition","condition":"braced","duration":200}],
        "on_failure":[],"descriptors":[],"riders":[]
    }]});
    definition["ai"]["hostile"] = json!(false);
    document["abilities"]
        .as_array_mut()
        .unwrap()
        .push(definition);
    document["constants"]["default_abilities"]
        .as_array_mut()
        .unwrap()
        .push(json!("guarded_step_probe"));
    configure(&mut document);
    let rules =
        Ruleset::validate(serde_json::from_value(document).expect("one-cell mechanism authoring"))
            .unwrap();
    let mut species = crate::test_creatures::species();
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::ResourceMax {
                resource: crate::resources::Resource::Stamina,
            },
            value: 20,
        },
    });
    let species_id = species.id.clone();
    let content = Content::new(
        rules,
        ScenarioCatalog {
            species: [(species_id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let mut game = crate::Game::new(
        World::new(
            vec![Region {
                id: RegionId(1),
                name: "movement room".into(),
                bounds: Extent::new(8, 8, 4).unwrap(),
            }],
            vec![],
        )
        .unwrap(),
        42,
        content.clone(),
    );
    for (index, at) in [cell(2, 2, 0), cell(3, 2, 0)].into_iter().enumerate() {
        let actor = game
            .spawn_actor(at, NonZeroU64::new(move_ticks).unwrap())
            .unwrap();
        let build = CreatureBuild::new(
            content.clone(),
            species_id.clone(),
            HdLedger::seeded(vec![HdSource::Racial], 42).unwrap(),
            ManaBinding::Intellect,
            [Skill::Deception, Skill::Persuasion],
        )
        .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: format!("movement subject {index}"),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
    }
    game
}

fn guarded_step_action(direction: &str) -> crate::Action {
    serde_json::from_value(json!({"type":"use_ability","value":{
        "id":"guarded_step_probe","invocation":{"kind":"self_step","direction":direction},
        "supplies":{"kind":"carried"}
    }}))
    .expect("typed selected step")
}

fn start_step(game: &mut crate::Game, direction: &str) -> crate::IntentionId {
    let actor = crate::ActorId(1);
    let action = guarded_step_action(direction);
    let before = game
        .creature(actor)
        .unwrap()
        .costs()
        .resources()
        .balance(crate::resources::Resource::Stamina);
    let intention = game
        .admit_intention(actor, action.clone(), crate::IntentionOrigin::Human)
        .unwrap();
    assert!(
        game.preparation(actor).is_none(),
        "queue admission does not prepare work"
    );
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        before
    );
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, intention);
    assert_eq!(execution.action, Some(action));
    execution.outcome.unwrap();
    assert_eq!(game.preparation(actor).unwrap().intention, Some(intention));
    intention
}

#[test]
fn guarded_step_moves_only_at_preparation_completion_and_pays_one_move() {
    use crate::{Action, ActorId};
    let mut game = guarded_step_game(100);
    let actor = ActorId(1);
    let before = game
        .creature(actor)
        .unwrap()
        .costs()
        .resources()
        .balance(crate::resources::Resource::Stamina);
    let identity = start_step(&mut game, "north");
    let paid = game.creature(actor).unwrap().costs().clone();
    assert_eq!(
        paid.resources()
            .balance(crate::resources::Resource::Stamina),
        before - 1
    );
    assert_eq!(paid.reservation(identity).unwrap().resolution, 1);
    assert_eq!(game.tick(), 0);
    assert_eq!(game.actors[&actor].location, cell(2, 2, 0));
    assert_eq!(
        game.next_actor(),
        Some(ActorId(2)),
        "no nested movement scheduler skips another actor"
    );
    game.actors.get_mut(&ActorId(2)).unwrap().turn_ticks = std::num::NonZeroU64::new(99).unwrap();
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(game.tick(), 99);
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        before - 1
    );
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .recovery_elapsed(crate::resources::Resource::Stamina),
        99
    );
    game.actors.get_mut(&ActorId(2)).unwrap().turn_ticks = std::num::NonZeroU64::new(1).unwrap();
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(game.tick(), 100);
    assert_eq!(game.actors[&actor].location, cell(2, 1, 0));
    assert_eq!(game.actors[&actor].ready_at, 300);
    assert!(game.preparation(actor).is_none());
    assert!(
        !game.physics.displaced.contains(&actor),
        "voluntary step is not forced displacement"
    );
    let creature = game.creature(actor).unwrap();
    // At active tick 100, ordinary recovery restores one before the single
    // resolution payment. Compare the entire ledger, including hold retirement.
    let mut expected = paid;
    expected.advance_active(100);
    assert_eq!(
        expected
            .resources()
            .balance(crate::resources::Resource::Stamina),
        before
    );
    expected.finish(identity).unwrap();
    assert_eq!(creature.costs(), &expected);
    assert_eq!(
        creature
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        before - 1
    );
    let braced: Vec<_> = creature
        .conditions()
        .instances()
        .filter(|condition| condition.condition.as_str() == "braced")
        .collect();
    assert_eq!(braced.len(), 1);
    assert_eq!(braced[0].remaining, 200);
}

#[test]
fn initially_blocked_step_is_atomic_and_does_not_allocate_identity() {
    let mut game = guarded_step_game(100);
    game.set_wall(cell(2, 1, 0), true).unwrap();
    let before = game.clone();
    assert!(game
        .admit_intention(
            crate::ActorId(1),
            guarded_step_action("north"),
            crate::IntentionOrigin::Human
        )
        .is_err());
    assert_eq!(game, before);
    game.set_wall(cell(2, 1, 0), false).unwrap();
    assert_eq!(
        game.admit_intention(
            crate::ActorId(1),
            guarded_step_action("north"),
            crate::IntentionOrigin::Human
        )
        .unwrap(),
        crate::IntentionId(1)
    );
}

#[test]
fn newly_occupied_step_retains_braced_and_all_committed_timing_and_cost() {
    use crate::{Action, ActorId};
    let mut game = guarded_step_game(100);
    let identity = start_step(&mut game, "north");
    let mut expected = game.creature(ActorId(1)).unwrap().costs().clone();
    assert_eq!(
        expected
            .resources()
            .balance(crate::resources::Resource::Stamina),
        19
    );
    assert_eq!(expected.reservation(identity).unwrap().resolution, 1);
    expected.advance_active(100);
    expected.finish(identity).unwrap();
    expected.advance_active(42);
    // Another actor takes an ordinary turn and occupies the selected cell first.
    game.act(ActorId(2), Action::Move(tor_world::Direction::NorthWest))
        .unwrap();
    assert_eq!(game.tick(), 142);
    assert_eq!(game.actors[&ActorId(2)].location, cell(2, 1, 0));
    assert_eq!(game.actors[&ActorId(1)].location, cell(2, 2, 0));
    assert_eq!(game.actors[&ActorId(1)].ready_at, 300);
    assert!(game.preparation(ActorId(1)).is_none());
    let creature = game.creature(ActorId(1)).unwrap();
    assert_eq!(
        creature.costs(),
        &expected,
        "one normal recovery and one resolution payment"
    );
    assert_eq!(
        creature
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        19
    );
    assert_eq!(
        creature
            .conditions()
            .instances()
            .find(|condition| condition.condition.as_str() == "braced")
            .unwrap()
            .remaining,
        158
    );
    drop(creature);
    game.act(ActorId(2), Action::Move(tor_world::Direction::North))
        .unwrap();
    assert_eq!(
        game.actors[&ActorId(1)].location,
        cell(2, 2, 0),
        "a later opening cannot restart the completed attempted step"
    );
}

#[test]
fn guarded_step_diagonal_cost_uses_the_configured_ordinary_duration() {
    for (base, step) in [(1, 2), (75, 107), (100, 142)] {
        let mut game = guarded_step_game(base);
        start_step(&mut game, "north_west");
        let actor = crate::ActorId(1);
        let captured = game.preparation(actor).unwrap().plan.clone();
        assert_eq!(captured.preparation(), 100);
        assert_eq!(captured.recovery(), 100 + step);
        // Exercise ordinary queued work until preparation reaches its actual boundary.
        while game.preparation(actor).is_some() {
            let next = game.next_actor().expect("ordinary decision");
            game.act(next, crate::Action::Wait).unwrap();
        }
        assert_eq!(game.actors[&actor].location, cell(1, 1, 0));
        assert_eq!(game.actors[&actor].ready_at, 200 + step);
    }
}

#[test]
fn queued_guarded_step_never_reinterprets_a_direction_after_relocation() {
    let mut game = guarded_step_game(100);
    let actor = crate::ActorId(1);
    let identity = game
        .admit_intention(
            actor,
            guarded_step_action("north"),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    game.teleport(actor, cell(5, 5, 0)).unwrap();
    let before = game
        .creature(actor)
        .unwrap()
        .costs()
        .resources()
        .balance(crate::resources::Resource::Stamina);
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, identity);
    assert!(execution.outcome.is_err());
    assert_eq!(game.actors[&actor].location, cell(5, 5, 0));
    assert_eq!(game.tick(), 0);
    assert!(game.preparation(actor).is_none());
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .costs()
            .resources()
            .balance(crate::resources::Resource::Stamina),
        before
    );
}

#[test]
fn guarded_step_checkpoint_preserves_exact_work_and_single_movement() {
    let mut game = guarded_step_game(100);
    let actor = crate::ActorId(1);
    let intention = start_step(&mut game, "north");
    let work = game.preparation(actor).unwrap().plan.work();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored =
        crate::Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.preparation(actor).unwrap().plan.work(), work);
    assert_eq!(
        restored.preparation(actor).unwrap().intention,
        Some(intention)
    );
    for _ in 0..5 {
        let next = game.next_actor().unwrap();
        assert_eq!(restored.next_actor(), Some(next));
        assert_eq!(
            game.act(next, crate::Action::Wait),
            restored.act(next, crate::Action::Wait)
        );
        assert_eq!(game, restored);
    }
    assert_eq!(game.actors[&actor].location, cell(2, 1, 0));
    assert!(game.preparation(actor).is_none());
}
// Explicit species profiles are independent of physical body dimensions.
use crate::creatures::{CreatureSize, DisplacementProfile};

fn profile(size: CreatureSize, anchored: bool) -> Option<DisplacementProfile> {
    Some(DisplacementProfile { size, anchored })
}

fn push_content(
    source: Option<DisplacementProfile>,
    target: Option<DisplacementProfile>,
    with_push: bool,
) -> std::sync::Arc<crate::creatures::Content> {
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};
    let mut document = serde_json::to_value(crate::test_creatures::ruleset().document()).unwrap();
    let mut ability = serde_json::to_value(crate::test_creatures::paid_melee()).unwrap();
    ability["id"] = json!("push_probe");
    ability["name"] = json!("Push Probe");
    ability["invocation"] = json!("actor_target");
    ability["cost"] = json!({"resource":"stamina","start":1,"resolution":2});
    ability["timing"] = json!({"source":{
        "kind":"selected_weapon_adjusted","preparation":50,"recovery":0
    },"scaled_by":"speed"});
    ability["strikes"]["strikes"][0]["on_success"] = json!([
        {"kind":"damage","damage":{"kind":"selected_weapon","impact_bonus":0}}
    ]);
    ability["strikes"]["strikes"][0]["riders"] = if with_push {
        json!([{
            "trigger":"success","check":null,"descriptors":[],
            "effects":[{"kind":"push_target_one_cell","direction":"away_from_source"}]
        }])
    } else {
        json!([])
    };
    document["abilities"].as_array_mut().unwrap().push(ability);
    document["constants"]["default_abilities"]
        .as_array_mut()
        .unwrap()
        .push(json!("push_probe"));
    let rules = crate::rules::Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
    let mut catalog = crate::creatures::ScenarioCatalog::default();
    for (name, displacement) in [
        ("pushing_source", source),
        ("pushed_target", target),
        ("anchored_target", profile(CreatureSize::Medium, true)),
        ("missing_target", None),
        ("large_target", profile(CreatureSize::Large, false)),
        ("huge_target", profile(CreatureSize::Huge, false)),
    ] {
        let mut species = crate::test_creatures::species();
        species.id = crate::content_ids::SpeciesId::new(name).unwrap();
        species.displacement = displacement;
        species.melee =
            crate::test_creatures::natural_melee(100, 60, crate::combat::DamageType::Impact, 4);
        species.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::ResourceMax {
                    resource: crate::resources::Resource::Stamina,
                },
                value: 20,
            },
        });
        catalog.species.insert(species.id.clone(), species);
    }
    crate::creatures::Content::new(rules, catalog).unwrap()
}

fn configure_push_subject(game: &mut crate::Game, actor: crate::ActorId, name: &str) {
    let build = crate::creatures::CreatureBuild::new(
        game.content().clone(),
        crate::content_ids::SpeciesId::new(name).unwrap(),
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
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
            name: name.into(),
            faction: "neutral".into(),
        },
        build,
    )
    .unwrap();
}

fn push_game(
    source: Option<DisplacementProfile>,
    target: Option<DisplacementProfile>,
    with_push: bool,
) -> crate::Game {
    use tor_world::{Extent, Region, RegionId, World};
    let mut game = crate::Game::new(
        World::new(
            vec![Region {
                id: RegionId(1),
                name: "push room".into(),
                bounds: Extent::new(9, 9, 9).unwrap(),
            }],
            vec![],
        )
        .unwrap(),
        42,
        push_content(source, target, with_push),
    );
    for (at, name) in [
        (cell(3, 3, 3), "pushing_source"),
        (cell(4, 3, 3), "pushed_target"),
    ] {
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        configure_push_subject(&mut game, actor, name);
    }
    game
}

fn push_action() -> crate::Action {
    serde_json::from_value(json!({"type":"use_ability","value":{
        "id":"push_probe","invocation":{"kind":"actor_target","target":2},
        "supplies":{"kind":"carried"}
    }}))
    .unwrap()
}

fn start_push(game: &mut crate::Game) -> crate::IntentionId {
    let identity = game
        .admit_intention(
            crate::ActorId(1),
            push_action(),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let plan = &game.preparation(crate::ActorId(1)).unwrap().plan;
    assert_eq!(plan.preparation(), 110);
    assert_eq!(plan.recovery(), 40);
    identity
}

fn finish_push(game: &mut crate::Game) {
    for _ in 0..4 {
        if game.preparation(crate::ActorId(1)).is_none() {
            return;
        }
        let actor = game
            .next_actor()
            .expect("other actor retains ordinary turns");
        assert_ne!(actor, crate::ActorId(1));
        game.act(actor, crate::Action::Wait).unwrap();
    }
    assert!(game.preparation(crate::ActorId(1)).is_none());
}

#[test]
fn two_cell_delvers_push_from_shortest_contact_under_every_cube_rotation() {
    for rotation in 0..24 {
        let facts = profile(CreatureSize::Medium, false);
        let mut game = push_game(facts, facts, true);
        let vector = tor_world::rotate_vector(rotation, [1, 0, 0]);
        let position = |multiple: i64| {
            cell(
                3 + (multiple * vector[0]) as i32,
                3 + (multiple * vector[1]) as i32,
                3 + (multiple * vector[2]) as i32,
            )
        };
        game.teleport(crate::ActorId(2), position(1)).unwrap();
        for actor in [crate::ActorId(1), crate::ActorId(2)] {
            game.actors.get_mut(&actor).unwrap().orientation = rotation;
            game.set_body(
                actor,
                crate::BodySpec {
                    cells: vec![[0, 0, 1], [0, 0, 0]],
                    eye: [0, 0, 1],
                    mass: 80,
                },
            )
            .unwrap();
        }
        assert!(game.attack_available(crate::ActorId(1), crate::ActorId(2)));
        let identity = start_push(&mut game);
        let source = crate::ActorId(1);
        let target = crate::ActorId(2);
        assert_eq!(
            game.creature(source)
                .unwrap()
                .costs()
                .resources()
                .balance(crate::resources::Resource::Stamina),
            19,
            "one start payment"
        );
        assert_eq!(
            game.creature(source)
                .unwrap()
                .costs()
                .reservation(identity)
                .unwrap()
                .resolution,
            2
        );
        game.act(target, crate::Action::Wait).unwrap();
        assert_eq!(game.tick(), 100);
        assert_eq!(game.actors[&target].location, position(1));
        assert_eq!(
            game.creature(source)
                .unwrap()
                .costs()
                .resources()
                .balance(crate::resources::Resource::Stamina),
            20,
            "ordinary recovery precedes impact"
        );
        game.actors.get_mut(&target).unwrap().turn_ticks = std::num::NonZeroU64::new(10).unwrap();
        game.act(target, crate::Action::Wait).unwrap();
        assert_eq!(game.tick(), 110);
        assert_eq!(
            game.actors[&crate::ActorId(2)].location,
            position(2),
            "rotation {rotation}"
        );
        assert_eq!(game.actors[&crate::ActorId(2)].orientation, rotation);
        assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
        assert_eq!(
            game.creature(crate::ActorId(1))
                .unwrap()
                .costs()
                .resources()
                .balance(crate::resources::Resource::Stamina),
            18,
            "one two-point resolution payment"
        );
        assert!(game
            .creature(source)
            .unwrap()
            .costs()
            .reservations()
            .is_empty());
        assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 150);
    }
}

#[test]
fn concave_shortest_contact_tie_is_body_local_order_independent_and_has_no_fallback() {
    let facts = profile(CreatureSize::Medium, false);
    for reverse in [false, true] {
        for blocked in [false, true] {
            let mut game = push_game(facts, facts, true);
            game.teleport(crate::ActorId(2), cell(4, 4, 3)).unwrap();
            let mut cells = vec![[0, 0, 0], [1, 0, 0], [0, 1, 0]];
            if reverse {
                cells.reverse();
            }
            game.set_body(
                crate::ActorId(1),
                crate::BodySpec {
                    cells,
                    eye: [0, 0, 0],
                    mass: 80,
                },
            )
            .unwrap();
            if blocked {
                game.set_wall(cell(5, 4, 3), true).unwrap();
            }
            assert!(game.attack_available(crate::ActorId(1), crate::ActorId(2)));
            start_push(&mut game);
            finish_push(&mut game);
            assert_eq!(
                game.actors[&crate::ActorId(2)].location,
                if blocked {
                    cell(4, 4, 3)
                } else {
                    cell(5, 4, 3)
                }
            );
            assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
        }
    }
}

#[test]
fn push_eligibility_uses_explicit_size_and_anchor_without_physical_size_guessing() {
    use CreatureSize::*;
    for (source, target, allowed) in [
        (profile(Medium, false), profile(Tiny, false), true),
        (profile(Medium, false), profile(Medium, false), true),
        (profile(Medium, false), profile(Large, false), true),
        (profile(Medium, false), profile(Huge, false), false),
        (profile(Colossal, false), profile(Colossal, false), true),
        (None, profile(Tiny, false), false),
        (profile(Colossal, false), None, false),
        (profile(Colossal, false), profile(Tiny, true), false),
    ] {
        for mass in [1, 80, u32::MAX] {
            let mut game = push_game(source, target, true);
            game.set_body(
                crate::ActorId(2),
                crate::BodySpec {
                    cells: vec![[0, 0, 0]],
                    eye: [0, 0, 0],
                    mass,
                },
            )
            .unwrap();
            start_push(&mut game);
            finish_push(&mut game);
            assert_eq!(
                game.actors[&crate::ActorId(2)].location,
                if allowed {
                    cell(5, 3, 3)
                } else {
                    cell(4, 3, 3)
                }
            );
            assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
        }
    }
}

#[test]
fn target_moving_during_preparation_supplies_the_actual_primary_contact() {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    start_push(&mut game);
    game.act(
        crate::ActorId(2),
        crate::Action::Move(tor_world::Direction::NorthWest),
    )
    .unwrap();
    // The selected target is now north, still in contact when the primary resolves.
    finish_push(&mut game);
    assert_eq!(game.actors[&crate::ActorId(2)].location, cell(3, 1, 3));
    assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
}

#[test]
fn occupied_push_preserves_exact_primary_rng_damage_payment_and_recovery() {
    let facts = profile(CreatureSize::Medium, false);
    let mut with_push = push_game(facts, facts, true);
    let mut ordinary = push_game(facts, facts, false);
    for game in [&mut with_push, &mut ordinary] {
        let blocker = game
            .spawn_actor(cell(5, 3, 3), std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.actors.get_mut(&blocker).unwrap().ready_at = 1000;
        start_push(game);
        finish_push(game);
    }
    assert_eq!(with_push.actors[&crate::ActorId(2)].location, cell(4, 3, 3));
    assert_eq!(
        with_push.health(crate::ActorId(2)),
        ordinary.health(crate::ActorId(2))
    );
    assert_eq!(with_push.rules_rng, ordinary.rules_rng);
    assert_eq!(
        with_push.actors[&crate::ActorId(1)].ready_at,
        ordinary.actors[&crate::ActorId(1)].ready_at
    );
    assert_eq!(
        with_push.creature(crate::ActorId(1)).unwrap().costs(),
        ordinary.creature(crate::ActorId(1)).unwrap().costs()
    );
}

#[test]
fn forced_push_uses_physical_three_axis_clearance_without_walking_support() {
    let facts = profile(CreatureSize::Medium, false);
    for delta in [[0, 0, 1], [1, 1, 0], [1, 1, 1]] {
        let mut game = push_game(facts, facts, true);
        let at = |multiple| {
            cell(
                3 + delta[0] * multiple,
                3 + delta[1] * multiple,
                3 + delta[2] * multiple,
            )
        };
        game.teleport(crate::ActorId(2), at(1)).unwrap();
        game.set_gravity(tor_world::RegionId(1), [0, 0, 0]).unwrap();
        assert!(
            game.actor_translation(crate::ActorId(2), tor_world::Direction::East)
                .is_none(),
            "explicit zero gravity supplies no ordinary walking support"
        );
        start_push(&mut game);
        finish_push(&mut game);
        assert_eq!(game.actors[&crate::ActorId(2)].location, at(2));
        assert_eq!(
            game.actor_motion(crate::ActorId(2)).unwrap().velocity,
            [0, 0, 0]
        );
        assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
    }
}

#[test]
fn forced_diagonal_rejects_one_blocked_component_order_that_walking_allows() {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    game.teleport(crate::ActorId(2), cell(4, 4, 3)).unwrap();
    game.set_wall(cell(5, 4, 3), true).unwrap();
    assert_eq!(
        game.actor_translation(crate::ActorId(2), tor_world::Direction::SouthEast),
        Some((cell(5, 5, 3), 0))
    );
    start_push(&mut game);
    finish_push(&mut game);
    assert_eq!(game.actors[&crate::ActorId(2)].location, cell(4, 4, 3));
    assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
}

#[test]
fn occupied_nonreference_destination_blocks_a_tall_body_but_loose_items_do_not() {
    let facts = profile(CreatureSize::Medium, false);
    for head_blocked in [false, true] {
        let mut game = push_game(facts, facts, true);
        for actor in [crate::ActorId(1), crate::ActorId(2)] {
            game.set_body(
                actor,
                crate::BodySpec {
                    cells: vec![[0, 0, 0], [0, 0, 1]],
                    eye: [0, 0, 1],
                    mass: 80,
                },
            )
            .unwrap();
        }
        game.place_item(cell(5, 3, 3), "loose token".into())
            .unwrap();
        if head_blocked {
            game.set_wall(cell(5, 3, 4), true).unwrap();
        }
        start_push(&mut game);
        finish_push(&mut game);
        assert_eq!(
            game.actors[&crate::ActorId(2)].location,
            if head_blocked {
                cell(4, 3, 3)
            } else {
                cell(5, 3, 3)
            }
        );
        assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
    }
}

#[test]
fn explicit_eligibility_is_rechecked_after_preparation() {
    let facts = profile(CreatureSize::Medium, false);
    for (replacement, allowed) in [
        ("large_target", true),
        ("huge_target", false),
        ("anchored_target", false),
        ("missing_target", false),
    ] {
        let mut game = push_game(facts, facts, true);
        start_push(&mut game);
        let build = crate::creatures::CreatureBuild::new(
            game.content().clone(),
            crate::content_ids::SpeciesId::new(replacement).unwrap(),
            crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
                .unwrap(),
            crate::attributes::ManaBinding::Intellect,
            [
                crate::attributes::Skill::Deception,
                crate::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        let before = crate::diagnostics::work_counts();
        game.rebuild_creature(crate::ActorId(2), build).unwrap();
        let rebuilt = crate::diagnostics::work_counts();
        assert!(rebuilt.raw_species_lookups > before.raw_species_lookups);
        assert!(rebuilt.permanent_creature_compiles > before.permanent_creature_compiles);
        finish_push(&mut game);
        assert_eq!(
            crate::diagnostics::work_counts().raw_species_lookups,
            rebuilt.raw_species_lookups
        );
        assert_eq!(
            game.actors[&crate::ActorId(2)].location,
            if allowed {
                cell(5, 3, 3)
            } else {
                cell(4, 3, 3)
            },
            "{replacement}"
        );
        assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
    }
}

#[test]
fn rotated_portal_head_contact_continues_in_the_targets_persistent_body_frame() {
    use tor_world::{rotate_vector, Direction, Extent, Passage, Region, RegionId, World};
    let facts = profile(CreatureSize::Medium, false);
    let mut game = crate::Game::new(
        World::new(
            vec![
                Region {
                    id: RegionId(1),
                    name: "source shaft".into(),
                    bounds: Extent::new(8, 4, 16).unwrap(),
                },
                Region {
                    id: RegionId(2),
                    name: "target shaft".into(),
                    bounds: Extent::new(8, 4, 16).unwrap(),
                },
            ],
            vec![],
        )
        .unwrap(),
        42,
        push_content(facts, facts, true),
    );
    let from = cell(7, 1, 10);
    let head = tor_world::Location {
        region: RegionId(2),
        position: tor_world::Position { x: 2, y: 1, z: 15 },
    };
    let rotation = (0..24)
        .find(|rotation| {
            rotate_vector(*rotation, [1, 0, 0]) == [0, 0, -1]
                && rotate_vector(*rotation, [0, 0, 1]) == [1, 0, 0]
        })
        .unwrap();
    game.connect_portal_area(
        Passage {
            from,
            direction: Direction::East,
            to: head,
        },
        rotation,
        1,
        1,
    )
    .unwrap();
    let source = game
        .spawn_actor(from, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    let target_at = tor_world::Location {
        position: tor_world::Position {
            x: 1,
            ..head.position
        },
        ..head
    };
    let target = game
        .spawn_actor(target_at, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    game.actors.get_mut(&target).unwrap().orientation = rotation;
    game.set_body(
        target,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    configure_push_subject(&mut game, source, "pushing_source");
    configure_push_subject(&mut game, target, "pushed_target");
    assert!(game.attack_available(source, target));
    start_push(&mut game);
    finish_push(&mut game);
    assert_eq!(
        game.actors[&target].location,
        tor_world::Location {
            position: tor_world::Position {
                z: 14,
                ..target_at.position
            },
            ..target_at
        }
    );
    assert_eq!(game.actors[&target].orientation, rotation);
    assert_eq!(game.health(target), Some((26, 30)));
}

#[test]
fn equally_short_loop_embeddings_keep_damage_but_cannot_choose_a_push() {
    use tor_world::{Direction, Extent, Passage, Region, RegionId, World};
    let facts = profile(CreatureSize::Medium, false);
    for reversed in [false, true] {
        let mut game = crate::Game::new(
            World::new(
                vec![
                    Region {
                        id: RegionId(1),
                        name: "loop source".into(),
                        bounds: Extent::new(1, 5, 5).unwrap(),
                    },
                    Region {
                        id: RegionId(2),
                        name: "loop target".into(),
                        bounds: Extent::new(5, 5, 5).unwrap(),
                    },
                ],
                vec![],
            )
            .unwrap(),
            42,
            push_content(facts, facts, true),
        );
        let from = cell(0, 2, 2);
        let target_at = tor_world::Location {
            region: RegionId(2),
            position: tor_world::Position { x: 2, y: 2, z: 2 },
        };
        let mut exits = [Direction::East, Direction::West];
        if reversed {
            exits.reverse();
        }
        for direction in exits {
            game.connect_portal_area(
                Passage {
                    from,
                    direction,
                    to: target_at,
                },
                0,
                1,
                1,
            )
            .unwrap();
        }
        let source = game
            .spawn_actor(from, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        let target = game
            .spawn_actor(target_at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        configure_push_subject(&mut game, source, "pushing_source");
        configure_push_subject(&mut game, target, "pushed_target");
        assert!(game.attack_available(source, target));
        start_push(&mut game);
        finish_push(&mut game);
        assert_eq!(game.actors[&target].location, target_at);
        assert_eq!(game.health(target), Some((26, 30)));
    }
}

fn ordinary_control_game() -> crate::Game {
    let mut species = crate::test_creatures::species();
    species.melee =
        crate::test_creatures::natural_melee(100, 60, crate::combat::DamageType::Impact, 4);
    let build = crate::test_creatures::build(species);
    let mut game = crate::Game::new(
        tor_world::World::new(
            vec![tor_world::Region {
                id: tor_world::RegionId(1),
                name: "ordinary controls".into(),
                bounds: tor_world::Extent::new(8, 8, 4).unwrap(),
            }],
            vec![],
        )
        .unwrap(),
        42,
        build.content().clone(),
    );
    for at in [cell(2, 2, 0), cell(3, 2, 0)] {
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "control".into(),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    game
}

#[test]
fn ordinary_queued_movement_control_retains_one_identity_and_other_actor_turn() {
    let mut game = ordinary_control_game();
    let identity = game
        .admit_intention(
            crate::ActorId(1),
            crate::Action::Move(tor_world::Direction::North),
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, identity);
    execution.outcome.unwrap();
    assert_eq!(game.actors[&crate::ActorId(1)].location, cell(2, 1, 0));
    assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 100);
    assert_eq!(game.next_actor(), Some(crate::ActorId(2)));
    assert_eq!(game.tick(), 0);
}

#[test]
fn ordinary_primary_control_retains_damage_and_weapon_timing() {
    let mut game = ordinary_control_game();
    game.act(
        crate::ActorId(1),
        crate::Action::UseAbility {
            id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            invocation: crate::abilities::AbilityInvocation::ActorTarget {
                target: crate::ActorId(2),
            },
            supplies: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(
        game.preparation(crate::ActorId(1))
            .unwrap()
            .plan
            .preparation(),
        60
    );
    game.act(crate::ActorId(2), crate::Action::Wait).unwrap();
    assert_eq!(game.health(crate::ActorId(2)), Some((26, 30)));
    assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 100);
}

#[test]
fn captured_spatial_records_reject_positional_and_extra_field_spellings() {
    use super::{MovementResult, Placement, StepCommitment};
    let at = json!({"region":1,"position":{"x":2,"y":2,"z":0}});
    let next = json!({"region":1,"position":{"x":2,"y":1,"z":0}});
    let placement = json!({"location":at,"orientation":0});
    let moved = json!({"kind":"moved","from":placement,"to":{"location":next,"orientation":0}});
    for valid in [
        json!({"kind":"blocked"}),
        json!({"kind":"ineligible"}),
        json!({"kind":"stale"}),
        json!({"kind":"ambiguous"}),
        moved.clone(),
    ] {
        let result: MovementResult =
            serde_json::from_slice(&serde_json::to_vec(&valid).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(result).unwrap(), valid);
    }
    for invalid in [
        json!({"kind":"blocked","distance":1}),
        json!(["blocked"]),
        json!({"kind":"moved","from":[at,0],"to":placement}),
    ] {
        assert!(
            serde_json::from_slice::<MovementResult>(&serde_json::to_vec(&invalid).unwrap())
                .is_err()
        );
    }
    for invalid in [
        json!([at, 0]),
        json!({"location":[1,[2,2,0]],"orientation":0}),
        json!({"location":{"region":1,"position":[2,2,0]},"orientation":0}),
        json!({"location":at,"orientation":24}),
    ] {
        assert!(
            serde_json::from_slice::<Placement>(&serde_json::to_vec(&invalid).unwrap()).is_err()
        );
    }
    let commitment = json!({"direction":"north","context":{"from":at,"orientation":0,"to":next,"next_orientation":0},"duration":100});
    let decoded: StepCommitment =
        serde_json::from_slice(&serde_json::to_vec(&commitment).unwrap()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), commitment);
    for key in ["vector", "target", "distance"] {
        let mut invalid = commitment.clone();
        invalid[key] = json!(1);
        assert!(
            serde_json::from_slice::<StepCommitment>(&serde_json::to_vec(&invalid).unwrap())
                .is_err()
        );
    }
}
#[test]
fn guarded_step_keeps_captured_speed_and_ordinary_duration_while_rechecking_geometry() {
    for blocked in [false, true] {
        let mut game = guarded_step_game(100);
        let actor = crate::ActorId(1);
        start_step(&mut game, "north");
        let frozen = game.preparation(actor).unwrap().plan.clone();
        assert_eq!(frozen.preparation(), 100);
        assert_eq!(frozen.recovery(), 200);
        let mut build = game.creature(actor).unwrap().build().clone();
        let mut attributes = build.initial_attributes().values();
        attributes.speed = 5;
        build
            .set_initial_attributes(crate::attributes::Attributes::new(attributes).unwrap())
            .unwrap();
        game.rebuild_creature(actor, build).unwrap();
        game.actors.get_mut(&actor).unwrap().turn_ticks = std::num::NonZeroU64::new(75).unwrap();
        assert_eq!(
            game.ability_timing(
                actor,
                &crate::content_ids::AbilityId::new("guarded_step_probe").unwrap()
            )
            .unwrap()
            .preparation,
            50
        );
        assert_eq!(game.preparation(actor).unwrap().plan, frozen);
        if blocked {
            game.set_wall(cell(2, 1, 0), true).unwrap();
        }
        game.act(crate::ActorId(2), crate::Action::Wait).unwrap();
        assert_eq!(game.tick(), 100);
        assert_eq!(
            game.actors[&actor].ready_at, 300,
            "old speed and ordinary 100-tick step are paid once"
        );
        assert_eq!(
            game.actors[&actor].location,
            if blocked {
                cell(2, 2, 0)
            } else {
                cell(2, 1, 0)
            }
        );
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .costs()
                .resources()
                .balance(crate::resources::Resource::Stamina),
            19
        );
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .conditions()
                .instances()
                .find(|condition| condition.condition.as_str() == "braced")
                .unwrap()
                .remaining,
            200
        );
    }
}

#[path = "movement_tests/lifecycle.rs"]
mod lifecycle;
#[test]
fn consumables_cannot_author_context_dependent_spatial_effects() {
    for effect in [
        crate::rules::Effect::StepSelfOneCell,
        crate::rules::Effect::PushTargetOneCell {
            direction: super::PushDirection::AwayFromSource,
        },
    ] {
        let consumable = crate::ConsumableSpec {
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![effect],
        };
        assert!(
            !consumable.valid(),
            "a potion has no typed selected step or primary hit witness"
        );
    }
    assert!(crate::ConsumableSpec {
        discovery: crate::ItemDiscovery::SelfEffect,
        effects: vec![crate::rules::Effect::Heal {
            amount: crate::rules::AmountExpr::Fixed { amount: 2 },
        }]
    }
    .valid());
}

#[path = "movement_tests/contexts.rs"]
mod contexts;

#[path = "movement_tests/autonomous.rs"]
mod autonomous;
#[path = "movement_tests/geometry.rs"]
mod geometry;
#[path = "movement_tests/physics.rs"]
mod physics;

#[path = "movement_tests/diagnostics.rs"]
mod diagnostics;

#[path = "movement_tests/streaming.rs"]
mod streaming;

mod recovery;

#[test]
fn unowned_self_step_rejects_before_geometry_without_licensing_the_private_ability() {
    use crate::abilities::{AbilityInvocation, RuleError, StepDirection};
    use crate::{Action, ActorId, GameError};
    let template = guarded_step_game(100);
    let mut document = template.content.rules.document().clone();
    document
        .constants
        .default_abilities
        .retain(|ability| ability.as_str() != "guarded_step_probe");
    let content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(document).unwrap(),
        template.content.catalog().clone(),
    )
    .unwrap();
    let mut game = crate::Game::new((*template.world).clone(), 42, content.clone());
    let species = content.catalog().species.keys().next().unwrap().clone();
    for (index, at) in [cell(2, 2, 0), cell(3, 2, 0)].into_iter().enumerate() {
        let actor = game
            .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
            .unwrap();
        assert_eq!(actor, ActorId(index as u64 + 1));
        let build = crate::creatures::CreatureBuild::new(
            content.clone(),
            species.clone(),
            crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
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
                name: "observer".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
    }
    let action = Action::UseAbility {
        id: crate::content_ids::AbilityId::new("guarded_step_probe").unwrap(),
        invocation: AbilityInvocation::SelfStep {
            direction: StepDirection::North,
        },
        supplies: Default::default(),
    };
    for blocked in [false, true] {
        game.set_wall(cell(2, 1, 0), blocked).unwrap();
        let before = game.clone();
        assert_eq!(
            game.admit_intention(ActorId(1), action.clone(), crate::IntentionOrigin::Human),
            Err(GameError::Rule(RuleError::UnknownAbility)),
            "unowned selection must not consult whether the direction is blocked"
        );
        assert_eq!(game, before);
        assert!(!game.actors[&ActorId(1)]
            .recognized_abilities
            .contains(&crate::content_ids::AbilityId::new("guarded_step_probe").unwrap()));
    }
}

#[test]
fn ordinary_direct_lethal_push_fall_keeps_credit_through_checkpoint() {
    let facts = profile(CreatureSize::Medium, false);
    let mut game = push_game(facts, facts, true);
    game.set_wall(cell(3, 3, 2), true).unwrap();
    game.set_wall(cell(4, 3, 2), true).unwrap();
    game.set_gravity(tor_world::RegionId(1), [0, 0, -1])
        .unwrap();
    crate::test_creatures::injure(&mut game, crate::ActorId(2), 25).unwrap();
    start_push(&mut game);
    game.act(crate::ActorId(2), crate::Action::Wait).unwrap();
    game.actors.get_mut(&crate::ActorId(2)).unwrap().turn_ticks =
        std::num::NonZeroU64::new(10).unwrap();
    game.act(crate::ActorId(2), crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 110);
    assert_eq!(game.actors[&crate::ActorId(2)].location, cell(5, 3, 3));
    assert_eq!(game.health(crate::ActorId(2)), Some((1, 30)));
    assert_eq!(
        game.creature(crate::ActorId(1))
            .unwrap()
            .earned_progress()
            .earned_xp(),
        0
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let bytes = serde_json::to_vec(&(game.checkpoint(&mut shared), shared)).unwrap();
    let (checkpoint, shared) = serde_json::from_slice(&bytes).unwrap();
    let mut replay =
        crate::Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    for _ in 0..100 {
        let actor = game.next_actor().unwrap();
        assert_eq!(
            game.act(actor, crate::Action::Wait),
            replay.act(actor, crate::Action::Wait)
        );
        assert_eq!(game, replay);
        if !game.alive(crate::ActorId(2)) {
            break;
        }
    }
    assert!(
        !game.alive(crate::ActorId(2)),
        "ordinary landing must kill the one-Health target"
    );
    assert_eq!(
        game.creature(crate::ActorId(1))
            .unwrap()
            .earned_progress()
            .earned_xp(),
        25,
        "the continuing fall must retain its directly applied push owner"
    );
}

mod earned_context;

mod fall_credit;

#[test]
fn compiled_species_push_uses_live_compiled_eligibility_without_raw_lookups() {
    for count in [2, 256] {
        let facts = profile(CreatureSize::Medium, false);
        let mut game = push_game(facts, facts, true);
        for id in [crate::ActorId(1), crate::ActorId(2)] {
            let creature = game.creature(id).unwrap();
            let base = creature.build();
            let build = crate::creatures::CreatureBuild::new(
                base.content().clone(),
                base.species_id().clone(),
                crate::progression::HdLedger::seeded(
                    vec![crate::progression::HdSource::Racial; count],
                    42,
                )
                .unwrap(),
                base.binding(),
                *base.starting_skills(),
            )
            .unwrap();
            game.rebuild_creature(id, build).unwrap();
        }
        let health = game.health(crate::ActorId(2)).unwrap().0;
        let before = crate::diagnostics::work_counts();
        start_push(&mut game);
        finish_push(&mut game);
        let after = crate::diagnostics::work_counts();
        assert_eq!(game.actors[&crate::ActorId(2)].location, cell(5, 3, 3));
        assert_eq!(game.health(crate::ActorId(2)).unwrap().0, health - 4);
        assert!(after.strike_resolutions > before.strike_resolutions);
        assert_eq!(after.raw_species_lookups, before.raw_species_lookups);
        assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
    }
}
