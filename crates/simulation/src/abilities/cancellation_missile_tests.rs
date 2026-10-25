//! Actual paid shared-device acceptance; structural trace tests are separate.
use super::*;
use crate::attributes::Skill;
use crate::content_ids::{AbilityId, ConditionFamilyId, ConditionId};
use crate::rules::*;
use crate::{ActorId, Game, ItemId};
use tor_world::{Location, Position, RegionId};
fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture(cancellation: bool) -> (Game, ItemId) {
    let mut game = super::ray_tests::fixture_with(|document, ray, species| {
        let mut special = document
            .abilities
            .iter()
            .find(|a| a.basic_attack)
            .unwrap()
            .clone();
        special.id = AbilityId::new("fixture_cancellable_special").unwrap();
        special.basic_attack = false;
        special.cancellable = true;
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: special.id.clone(),
            },
        });
        document.abilities.push(special);
        let mut invisible = document
            .conditions
            .iter()
            .find(|c| c.id.as_str() == "distracted")
            .unwrap()
            .clone();
        invisible.id = ConditionId::new("fixture_cancellable_invisibility").unwrap();
        invisible.family = ConditionFamilyId::new("fixture_cancellable_invisibility").unwrap();
        invisible.lifecycle = ConditionLifecycle::Timed;
        invisible.restrictions.clear();
        invisible.modifiers = vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Perception {
                value: crate::perception::PerceptionValue::Invisible { enabled: true },
            },
        }];
        document.conditions.push(invisible);
        species.modifiers.push(Modifier {
            cancellable: true,
            scope: Scope::Always,
            op: ModifierOp::Perception {
                value: crate::perception::PerceptionValue::Invisible { enabled: true },
            },
        });
        // No inferred attack-skill bonus: this is a real captured Lore source.
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::Attribute {
                    attribute: crate::attributes::Attribute::Intellect,
                },
                value: 100,
            },
        });
        species.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::AttackDamageRollBonus,
                value: 3,
            },
        });
        ray.invocation = InvocationKind::DirectionalContactRay;
        ray.reach = Reach::UntilObstruction;
        ray.cancellable = false;
        let StrikeSource::Authored { strikes } = &mut ray.strikes else {
            unreachable!()
        };
        strikes[0].cancellable = false;
        strikes[0].descriptors.clear();
        strikes[0].riders.clear();
        if cancellation {
            strikes[0].check = None;
            strikes[0].on_success = vec![Effect::CancelSpecials];
            strikes[0].on_failure.clear();
        } else {
            strikes[0].check = Some(CheckDef {
                actor: CheckActor::Target,
                skill: SkillSelector::Named {
                    skill: Skill::Agility,
                },
                against: Against::Opposed {
                    skill: SkillSelector::Named { skill: Skill::Lore },
                },
            });
            strikes[0].on_success.clear();
            strikes[0].on_failure = vec![Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: crate::combat::DamageType::Energy,
                        descriptor: None,
                        amount: AmountExpr::Rolled {
                            dice: DiceExpr {
                                count: 1,
                                sides: 4,
                                bonus: 1,
                            },
                        },
                    }],
                },
            }];
        }
    });
    game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 0;
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 1000;
    let mut spec = crate::ItemSpec::ordinary("contact test stick".into());
    spec.class = crate::ItemClass::Tool;
    spec.identity = "fixture.contact_device".into();
    spec.concealed = true;
    spec.device = Some(crate::DeviceSpec {
        ability: AbilityId::new("fixture_ray").unwrap(),
        initial_charges: crate::ChargeRange {
            minimum: 3,
            maximum: 3,
        },
        discovery: crate::ItemDiscovery::OnUse,
    });
    let item = game
        .place_item_stack(10, at(1), Some(ActorId(1)), 1, spec)
        .unwrap();
    (game, item)
}
fn complete(game: &mut Game, item: ItemId) {
    let before = game.tick;
    let id = game
        .admit_intention(
            ActorId(1),
            crate::Action::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    execution.outcome.unwrap();
    assert_eq!(game.tick, before + 100);
    assert_eq!(game.items[&item].charges, Some(2));
    assert!(game.items[&item].holds.is_empty());
}
#[test]
fn paid_cancellation_is_checkless_and_preserves_owned_progression() {
    let (mut game, item) = fixture(true);
    let special = AbilityId::new("fixture_cancellable_special").unwrap();
    let before = game.creature(ActorId(2)).unwrap().into_owned();
    assert!(before.derived().abilities.contains(&special));
    assert!(before.derived().perception.invisible);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        )
        .unwrap();
    assert_eq!(plan.as_ray().unwrap().traversal, RayTraversal::FirstBody);
    assert_eq!(plan.as_ray().unwrap().resolution, RayResolution::Checkless);
    assert_eq!(plan.tokens().len(), 0);
    let rng = game.rules_rng;
    complete(&mut game, item);
    assert_eq!(game.rules_rng, rng);
    let after = game.creature(ActorId(2)).unwrap();
    assert!(after.cancelled());
    assert!(!after.derived().abilities.contains(&special));
    assert!(!after.derived().perception.invisible);
    assert_eq!(after.build(), before.build());
    assert_eq!(after.restorable_losses(), before.restorable_losses());
    assert_eq!(after.health(), before.health());
    assert!(game.combat_events().iter().any(|event| matches!(event,
        crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.target == ActorId(2)
        && outcome.check.is_none() && outcome.effects == [ResolvedEffect::CancelSpecials { changed: true }])));
}
#[test]
fn repeated_cancellation_clears_fresh_independent_invisibility_without_global_veto() {
    let (mut game, item) = fixture(true);
    complete(&mut game, item);
    let invisible = ConditionId::new("fixture_cancellable_invisibility").unwrap();
    game.apply_effects(
        ActorId(2),
        &[Effect::ApplyCondition {
            condition: invisible.clone(),
            duration: 1000,
        }],
    )
    .unwrap();
    assert!(
        game.creature(ActorId(2))
            .unwrap()
            .derived()
            .perception
            .invisible
    );
    let rng = game.rules_rng;
    assert!(game
        .apply_effects(ActorId(2), &[Effect::CancelSpecials])
        .unwrap());
    assert_eq!(game.rules_rng, rng);
    assert!(
        !game
            .creature(ActorId(2))
            .unwrap()
            .derived()
            .perception
            .invisible
    );
    assert!(!game
        .creature(ActorId(2))
        .unwrap()
        .conditions()
        .instances()
        .any(|row| row.condition == invisible));
}
#[test]
fn paid_missile_has_one_contest_and_one_universal_damage_bonus() {
    let (mut game, item) = fixture(false);
    let plan = game
        .plan_work(
            ActorId(1),
            crate::Work::Zap {
                item,
                aim: crate::DeviceAim::Direction {
                    direction: StepDirection::East,
                },
            },
        )
        .unwrap();
    let ray = plan.as_ray().unwrap();
    assert!(
        matches!(ray.resolution, RayResolution::Opposed { source_skill: Skill::Lore,
        target_skill: Skill::Agility, source_bonus, .. } if source_bonus >= 100)
    );
    let [PlannedEffect::Damage { damage }] = ray.effects.as_slice() else {
        panic!("one missile damage pool")
    };
    let crate::damage::DamageAmount::Rolled(pool) = damage.components()[0].amount() else {
        panic!("rolled missile")
    };
    assert_eq!((pool.count(), pool.sides(), pool.bonus()), (1, 4, 4));
    let hp = game.health(ActorId(2)).unwrap().0;
    game.set_combat_diagnostics(true);
    complete(&mut game, item);
    let loss = hp - game.health(ActorId(2)).unwrap().0;
    assert!((5..=8).contains(&loss));
    let diagnostics = game.combat_diagnostics().unwrap();
    let records = diagnostics.records();
    assert_eq!(records.len(), 1);
    let steps = records[0].trace.steps();
    assert_eq!(
        steps.iter().filter(|s| s.id == "ray_launch_check").count(),
        1
    );
    assert_eq!(steps.iter().filter(|s| s.id == "check").count(), 1);
    assert!(records[0]
        .outcome
        .check
        .is_some_and(|check| !check.success && check.opposed));
}

#[test]
fn ordinary_granted_contact_uses_the_same_plan_and_not_item_payment() {
    let (mut game, item) = fixture(false);
    let ability = AbilityId::new("fixture_ray").unwrap();
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .derived()
        .abilities
        .contains(&ability));
    let hp = game.health(ActorId(2)).unwrap().0;
    let id = game
        .admit_intention(
            ActorId(1),
            crate::Action::UseAbility {
                id: ability.clone(),
                invocation: AbilityInvocation::directional_ray(StepDirection::East),
                supplies: crate::supplies::SupplySelection::default(),
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    execution.outcome.unwrap();
    assert!(game.health(ActorId(2)).unwrap().0 < hp);
    assert_eq!(game.items[&item].charges, Some(3));
    assert!(game.combat_events().iter().any(|event| matches!(event,
        crate::combat::CombatEvent::Resolved { outcome, .. } if outcome.ability.as_ref() == Some(&ability)
        && outcome.intention == Some(id) && outcome.target == ActorId(2))));
}
