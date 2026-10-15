//! Data-defined plans and the one production resolver.
use std::{num::NonZeroU64, sync::Arc};
use tor_simulation::abilities::{
    resolve, OutcomeStatus, Plan, PlannedEffect, ResolvedEffect, RuleError, Snapshot,
};
use tor_simulation::attributes::{Attributes, Skill};
use tor_simulation::content_ids::{
    AbilityId, ConditionId, DescriptorId, FlagId, TemplateId, TypeId,
};
use tor_simulation::creatures::{Content, CreatureBuild, Template};
use tor_simulation::damage::{DamageSpec, Protection};
use tor_simulation::dice::{DicePool, Edge};
use tor_simulation::resolution_diagnostics::ResolutionTrace;
use tor_simulation::rules::{
    AbilityStat, AmountExpr, DamageComponentExpr, DamageExpr, Effect, Modifier, ModifierOp, Reach,
    Scope, Selector, Stat,
};
use tor_simulation::{ActorId, CreatureIdentity, Game, GameError};
use tor_world::{Location, Position, RegionId};
pub(super) fn id(value: &str) -> AbilityId {
    AbilityId::new(value).unwrap()
}
fn tid(value: &str) -> TemplateId {
    TemplateId::new(value).unwrap()
}
fn add(ability: &str, stat: AbilityStat, value: i64) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Ability {
                ability: id(ability),
                stat,
            },
            value,
        },
    }
}
pub(super) fn content() -> Arc<Content> {
    let base = super::creature_state::build();
    let mut catalog = (*base.content().catalog()).clone();
    let mut techniques = Template::new(tid("techniques"), 0);
    techniques.modifiers = [
        "fixture_paid_melee",
        "fixture_mana_ray",
        "fixture_focus_fear",
    ]
    .into_iter()
    .map(|name| Modifier {
        scope: Scope::Always,
        op: ModifierOp::GrantAbility { ability: id(name) },
    })
    .collect();
    techniques.modifiers.extend([
        add("fixture_mana_ray", AbilityStat::Dice, 2),
        add("fixture_mana_ray", AbilityStat::FlatBonus, 2),
        add("fixture_focus_fear", AbilityStat::CheckBonus, 3),
        add("fixture_focus_fear", AbilityStat::Duration, 100),
    ]);
    let mut zombie = Template::new(tid("zombified"), 0);
    zombie.kind = Some(TypeId::new("undead").unwrap());
    zombie.adjustments.strength = 2;
    zombie.adjustments.speed = -1;
    zombie.overrides.intellect = Some(0);
    zombie.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Flag {
            flag: FlagId::new("mindless").unwrap(),
        },
    });
    let mut ward = Template::new(tid("fearward"), 10);
    ward.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fear").unwrap(),
            },
        },
    });
    let mut fireward = Template::new(tid("fireward"), 0);
    fireward.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap(),
            },
        },
    });
    let mut mindward = Template::new(tid("mindward"), 0);
    mindward.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("mind_affecting").unwrap(),
            },
        },
    });
    for template in [techniques, zombie, ward, fireward, mindward] {
        catalog.templates.insert(template.id.clone(), template);
    }
    let mut document = base.ruleset().document().clone();
    document.abilities.extend([
        super::creature_fixture::paid_melee(),
        super::creature_fixture::mana_ray(),
        super::creature_fixture::focus_fear(),
    ]);
    Content::new(
        tor_simulation::rules::Ruleset::validate(document).unwrap(),
        catalog,
    )
    .unwrap()
}
pub(super) fn build(content: Arc<Content>, granted: bool) -> CreatureBuild {
    let base = super::creature_state::build();
    let mut build = CreatureBuild::from_recorded(
        content,
        base.species_id().clone(),
        base.initial_attributes(),
        base.ledger().clone(),
        base.binding(),
        base.advancement().clone(),
        if granted {
            vec![tid("techniques")]
        } else {
            vec![]
        },
    )
    .unwrap();
    build
        .train(tor_simulation::HdIndex(0), Skill::Intimidation)
        .unwrap();
    build
}
pub(super) fn duel(seed: u64) -> (Game, ActorId, ActorId) {
    duel_with_content(seed, content())
}
fn duel_with_content(seed: u64, content: Arc<Content>) -> (Game, ActorId, ActorId) {
    let mut game = Game::two_room_in_stone(seed, content.clone());
    let mut ids = vec![];
    for x in [1, 2] {
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
            build(content.clone(), true),
        )
        .unwrap();
        ids.push(actor);
    }
    game.refresh_navigation();
    (game, ids[0], ids[1])
}
pub(super) fn snapshot(game: &Game, target: ActorId) -> Snapshot {
    let creature = game.creature(target).unwrap();
    Snapshot::capture(&creature)
}
pub(super) fn damage(amount: u32, category: tor_simulation::combat::DamageType) -> Effect {
    Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![DamageComponentExpr {
                category,
                descriptor: None,
                amount: AmountExpr::Fixed { amount },
            }],
        },
    }
}
pub(super) fn damage_total(outcome: &tor_simulation::abilities::Outcome) -> u32 {
    outcome
        .effects
        .iter()
        .filter_map(|effect| match effect {
            ResolvedEffect::Damage { amount } => Some(*amount),
            _ => None,
        })
        .sum()
}
fn planned_damage(plan: &Plan) -> &DamageSpec {
    match &plan.on_success()[0] {
        PlannedEffect::Damage { damage } => damage,
        _ => panic!("expected damage expression"),
    }
}
#[test]
fn plans_capture_grants_dice_binding_timing_costs_and_reach_without_mutation() {
    let (game, actor, target) = duel(42);
    let before = game.clone();
    for name in ["basic_melee", "fixture_paid_melee"] {
        let plan = game.ability_plan(actor, &id(name), target).unwrap();
        assert_eq!(plan.as_ability().unwrap().ability, id(name));
        assert_eq!((plan.preparation(), plan.recovery()), (50, 34));
        assert_eq!(plan.as_ability().unwrap().reach, Reach::Melee);
        assert_eq!(plan.cost().is_some(), name != "basic_melee");
        let mut rng = 42;
        assert_eq!(
            planned_damage(&plan)
                .resolve(&mut rng, Edge::default(), &Protection::default(), &mut ())
                .total,
            if name == "basic_melee" { 3 } else { 6 }
        );
    }
    let bolt = game
        .ability_plan(actor, &id("fixture_mana_ray"), target)
        .unwrap();
    assert_eq!((bolt.preparation(), bolt.recovery()), (84, 84));
    assert_eq!(bolt.as_ability().unwrap().reach, Reach::Sight { range: 6 });
    assert_eq!(
        planned_damage(&bolt).components()[0].amount(),
        tor_simulation::damage::DamageAmount::Rolled(DicePool::new(3, 6, 3).unwrap())
    );
    let fear = game
        .ability_plan(actor, &id("fixture_focus_fear"), target)
        .unwrap();
    assert_eq!(
        fear.on_success(),
        vec![PlannedEffect::ApplyCondition {
            id: ConditionId::new("fear").unwrap(),
            duration: 400
        }]
    );
    assert_eq!(
        fear.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .threshold,
        tor_simulation::abilities::CheckThreshold::Fixed { value: 12 }
    );
    assert_eq!(
        fear.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .actor,
        tor_simulation::rules::CheckActor::User
    );
    assert_eq!(
        fear.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .bonus,
        6
    );
    assert_eq!(
        bolt.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .threshold,
        tor_simulation::abilities::CheckThreshold::Fixed { value: 14 }
    );
    assert_eq!(game, before);
}
#[test]
fn plans_reject_unknown_dead_ungranted_and_unreachable_targets() {
    let (mut game, actor, target) = duel(42);
    assert!(game
        .ability_plan(ActorId(999), &id("fixture_mana_ray"), target)
        .is_err());
    assert_eq!(
        game.ability_plan(actor, &id("missing"), target),
        Err(GameError::Rule(RuleError::UnknownAbility))
    );
    let mut build = game.creature(actor).unwrap().build().clone();
    build.set_templates(vec![]).unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert!(game.ability_plan(actor, &id("basic_melee"), target).is_ok());
    assert!(game
        .ability_plan(actor, &id("fixture_mana_ray"), target)
        .is_err());
    game.apply_effects(
        actor,
        &[damage(1_000_000, tor_simulation::combat::DamageType::Vital)],
    )
    .unwrap();
    assert!(game
        .ability_plan(actor, &id("basic_melee"), target)
        .is_err());
}
#[test]
fn bolt_resolution_uses_one_check_and_the_shared_edge_damage_bundle() {
    let (game, actor, target) = duel(42);
    let mut plan = game
        .ability_plan(actor, &id("fixture_mana_ray"), target)
        .unwrap();
    let Plan::Ability(ability) = &mut plan else {
        panic!("ability plan")
    };
    ability.strikes[0].check.as_mut().unwrap().threshold =
        tor_simulation::abilities::CheckThreshold::Fixed { value: 0 };
    ability.strikes[0].check.as_mut().unwrap().edge = 2;
    ability.strikes[0].edge = 2;
    let mut actual_rng = 42;
    let actual = resolve(
        plan.resolution().unwrap(),
        &snapshot(&game, target),
        &mut actual_rng,
        &mut (),
    )
    .unwrap();
    let mut expected_rng = 42;
    let check = tor_simulation::dice::roll_check(&mut expected_rng, Edge::from_counts(2, 0));
    assert!(check.kept > 0);
    let expected = planned_damage(&plan).resolve(
        &mut expected_rng,
        Edge::default(),
        &Protection::default(),
        &mut (),
    );
    assert_eq!(damage_total(&actual), expected.total);
    assert_eq!(actual_rng, expected_rng);
}

#[test]
fn outgoing_edge_policy_uses_the_existing_check_kernel_and_never_refunds_accuracy_to_damage() {
    use tor_simulation::rules::{Against, Formula, OutgoingEdgePolicy, StrikeSource};
    for (policy, check_edge, shared, damage_edge) in [
        (OutgoingEdgePolicy::Ordinary, -4, 3, 0),
        (OutgoingEdgePolicy::IgnoreNegative, 4, 5, 2),
    ] {
        let original = content();
        let mut document = original.rules().document().clone();
        let ability = document
            .abilities
            .iter_mut()
            .find(|ability| ability.id == id("fixture_mana_ray"))
            .unwrap();
        ability.outgoing_edge_policy = policy;
        let StrikeSource::Authored { strikes } = &mut ability.strikes else {
            panic!("authored bolt")
        };
        strikes[0].check.as_mut().unwrap().against = Against::Difficulty {
            formula: Formula {
                base: -1000,
                skill: None,
                attribute: None,
            },
        };
        let mut catalog = original.catalog().clone();
        for species in catalog.species.values_mut() {
            for (stat, value) in [
                (Stat::Edge, 5),
                (Stat::Edge, -2),
                (Stat::CheckEdge, -6),
                (Stat::IncomingEdge { defense: None }, -1),
            ] {
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add { stat, value },
                });
            }
        }
        let content = Content::new(
            tor_simulation::rules::Ruleset::validate(document).unwrap(),
            catalog,
        )
        .unwrap();
        let (game, actor, target) = duel_with_content(42, content);
        let before = game.clone();
        let captured = game
            .ability_plan(actor, &id("fixture_mana_ray"), target)
            .unwrap();
        let strike = &captured.as_ability().unwrap().strikes[0];
        assert_eq!(strike.check.as_ref().unwrap().edge, check_edge);
        assert_eq!(strike.edge, shared);
        let mut actual_rng = 42;
        let outcome = resolve(
            captured.resolution().unwrap(),
            &snapshot(&game, target),
            &mut actual_rng,
            &mut (),
        )
        .unwrap();
        let mut expected_rng = 42;
        tor_simulation::dice::roll_check(
            &mut expected_rng,
            if check_edge > 0 {
                Edge::from_counts(3, 0)
            } else {
                Edge::from_counts(0, 3)
            },
        );
        let expected = planned_damage(&captured).resolve(
            &mut expected_rng,
            Edge::from_counts(damage_edge, 0),
            &Protection::default(),
            &mut (),
        );
        assert_eq!(damage_total(&outcome), expected.total);
        assert_eq!(
            actual_rng, expected_rng,
            "the existing 3d6/damage kernels own every draw"
        );
        assert_eq!(
            game, before,
            "read-only capture and pure resolution preserve state"
        );
    }
}
#[test]
fn condition_resistance_and_descriptor_immunity_share_the_resolver() {
    let (game, actor, target) = duel(42);
    let plan = game
        .ability_plan(actor, &id("fixture_focus_fear"), target)
        .unwrap();
    let mut applied = false;
    let mut resisted = false;
    for seed in 0..32 {
        let outcome = resolve(
            plan.resolution().unwrap(),
            &snapshot(&game, target),
            &mut seed.clone(),
            &mut (),
        )
        .unwrap();
        applied |= outcome
            .effects
            .iter()
            .any(|e| matches!(e, ResolvedEffect::ApplyCondition { duration: 400, .. }));
        resisted |= outcome.effects.is_empty();
    }
    assert!(applied && resisted);
    for descriptor in ["fear", "mind_affecting"] {
        let mut build = game.creature(target).unwrap().build().clone();
        build
            .set_templates(vec![tid(if descriptor == "fear" {
                "fearward"
            } else {
                "mindward"
            })])
            .unwrap();
        let immune = tor_simulation::creatures::CreatureState::new(build).unwrap();
        let mut rng = 42;
        let mut trace = ResolutionTrace::default();
        let result = resolve(
            plan.resolution().unwrap(),
            &Snapshot::capture(&immune),
            &mut rng,
            &mut trace,
        )
        .unwrap();
        assert_eq!(result.result, OutcomeStatus::Unaffected);
        assert!(result.effects.is_empty());
        assert_eq!(rng, 42);
        assert_eq!(trace.steps()[0].id, "effect.immune");
    }
}
#[test]
fn observer_records_exact_outcome_and_randomness_for_every_ability_and_seed() {
    let (game, actor, target) = duel(42);
    for name in [
        "basic_melee",
        "fixture_paid_melee",
        "fixture_mana_ray",
        "fixture_focus_fear",
    ] {
        let plan = game.ability_plan(actor, &id(name), target).unwrap();
        for seed in 0..128 {
            let mut ordinary_rng = seed;
            let ordinary = resolve(
                plan.resolution().unwrap(),
                &snapshot(&game, target),
                &mut ordinary_rng,
                &mut (),
            )
            .unwrap();
            let mut observed_rng = seed;
            let mut trace = ResolutionTrace::default();
            let observed = resolve(
                plan.resolution().unwrap(),
                &snapshot(&game, target),
                &mut observed_rng,
                &mut trace,
            )
            .unwrap();
            assert_eq!(observed, ordinary);
            assert_eq!(observed_rng, ordinary_rng);
            assert!(!trace.truncated());
            assert!(trace.steps().iter().any(|step| step.id == "check"));
        }
    }
}
#[test]
fn unavailable_target_and_immunity_never_consume_randomness() {
    let (game, actor, target) = duel(42);
    let plan = game
        .ability_plan(actor, &id("fixture_mana_ray"), target)
        .unwrap();
    let mut rng = 42;
    let mut dead = game.creature(target).unwrap().into_owned();
    dead.kill();
    let result = resolve(
        plan.resolution().unwrap(),
        &Snapshot::capture(&dead),
        &mut rng,
        &mut (),
    )
    .unwrap();
    assert_eq!(result.result, OutcomeStatus::Unaffected);
    assert_eq!(rng, 42);
}
#[test]
fn permanent_binding_is_captured_in_the_planned_check() {
    let (mut game, actor, target) = duel(42);
    let before = game
        .ability_plan(actor, &id("fixture_mana_ray"), target)
        .unwrap();
    let mut build = game.creature(actor).unwrap().build().clone();
    let mut values = build.initial_attributes().values();
    values.intellect = 5;
    build
        .set_initial_attributes(Attributes::new(values).unwrap())
        .unwrap();
    game.rebuild_creature(actor, build).unwrap();
    let after = game
        .ability_plan(actor, &id("fixture_mana_ray"), target)
        .unwrap();
    assert_eq!(
        after.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .bonus,
        before.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .bonus
            + 3
    );
}
#[test]
fn fire_hazards_obey_the_same_descriptor_immunity() {
    let (mut game, actor, _) = duel(42);
    let mut effect = damage(20, tor_simulation::combat::DamageType::Energy);
    if let Effect::Damage {
        damage: DamageExpr::Components { components },
    } = &mut effect
    {
        components[0].descriptor = Some(DescriptorId::new("fire").unwrap());
    }
    let mut build = game.creature(actor).unwrap().build().clone();
    build
        .set_templates(vec![tid("techniques"), tid("fireward")])
        .unwrap();
    game.rebuild_creature(actor, build).unwrap();
    let before = game.health(actor);
    assert!(!game.apply_effects(actor, &[effect]).unwrap());
    assert_eq!(game.health(actor), before);
}

#[test]
fn fixed_primary_combines_signed_penalties_and_power_before_clamping_without_draws() {
    for (amount, basic, power) in [(5, 0, 0), (12, 2, 5)] {
        let original = content();
        let mut catalog = (*original.catalog()).clone();
        let species = catalog
            .species
            .get_mut(&super::creature_state::build().species_id().clone())
            .unwrap();
        species.melee =
            tor_simulation::attacks::NaturalAttack::from_melee(super::creature_fixture::melee(
                0,
                60,
                tor_simulation::combat::DamageType::Impact,
                amount,
            ))
            .unwrap();
        let mut penalty = Template::new(tid("fixed_penalty"), 1);
        for ability in ["basic_melee", "fixture_paid_melee"] {
            penalty.modifiers.extend([
                add(ability, AbilityStat::FlatBonus, -10),
                add(ability, AbilityStat::Dice, 2),
            ]);
        }
        catalog.templates.insert(penalty.id.clone(), penalty);
        let (mut game, actor, target) =
            duel_with_content(42, Content::new(original.rules().clone(), catalog).unwrap());
        let mut build = game.creature(actor).unwrap().build().clone();
        build
            .set_templates(vec![tid("techniques"), tid("fixed_penalty")])
            .unwrap();
        game.rebuild_creature(actor, build).unwrap();
        for (ability, expected) in [("basic_melee", basic), ("fixture_paid_melee", power)] {
            let plan = game.ability_plan(actor, &id(ability), target).unwrap();
            for edge in [
                Edge::default(),
                Edge::from_counts(3, 0),
                Edge::from_counts(0, 3),
            ] {
                let mut rng = 42;
                assert_eq!(
                    planned_damage(&plan)
                        .resolve(&mut rng, edge, &Protection::default(), &mut ())
                        .total,
                    expected
                );
                assert_eq!(rng, 42);
            }
        }
    }
}
#[test]
fn effect_duration_boundary_includes_base_and_cannot_create_an_invalid_plan() {
    for (bonus, valid) in [(999_700, true), (999_701, false)] {
        let original = content();
        let mut catalog = (*original.catalog()).clone();
        let mut duration = Template::new(tid("long_condition"), 1);
        duration.modifiers = vec![
            add("fixture_focus_fear", AbilityStat::Duration, bonus),
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: id("fixture_focus_fear"),
                },
            },
        ];
        catalog.templates.insert(duration.id.clone(), duration);
        let (mut game, actor, target) =
            duel_with_content(42, Content::new(original.rules().clone(), catalog).unwrap());
        let mut build = game.creature(actor).unwrap().build().clone();
        let before = build.clone();
        match build.set_templates(vec![tid("long_condition")]) {
            Err(_) => {
                assert!(!valid);
                assert_eq!(build, before);
            }
            Ok(()) => {
                game.rebuild_creature(actor, build).unwrap();
                let plan = game.ability_plan(actor, &id("fixture_focus_fear"), target);
                assert_eq!(plan.is_ok(), valid);
                if let Ok(plan) = plan {
                    assert_eq!(
                        plan.on_success(),
                        vec![PlannedEffect::ApplyCondition {
                            id: ConditionId::new("fear").unwrap(),
                            duration: 1_000_000
                        }]
                    );
                }
            }
        }
    }
}
