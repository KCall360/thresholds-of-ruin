//! Ordered-strike contracts. These tests are added before the implementation.
use super::AbilityPlan;
use serde_json::{json, Value};

fn self_target_shared_roles(opposed: bool) {
    use crate::content_ids::{AbilityId, ConditionId};
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    use crate::{rules::*, ActorId};
    // The distinct-target fixture runs first as an unchanged-output control.
    let ability_id = AbilityId::new("self_shared_probe").unwrap();
    for target in [ActorId(2), ActorId(1)] {
        let (mut game, _) = sequence_game_custom(
            &[0],
            |_| {},
            |rules, species| {
                let mut ability = rules
                    .abilities
                    .iter()
                    .find(|ability| ability.basic_attack)
                    .unwrap()
                    .clone();
                ability.id = ability_id.clone();
                ability.basic_attack = false;
                ability.reach = if target == ActorId(1) {
                    Reach::SelfOnly
                } else {
                    Reach::Melee
                };
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                strikes[0].check = Some(CheckDef {
                    actor: if opposed {
                        CheckActor::User
                    } else {
                        CheckActor::Target
                    },
                    skill: SkillSelector::NaturalAttack,
                    against: if opposed {
                        Against::Opposed {
                            skill: SkillSelector::NaturalAttack,
                        }
                    } else {
                        Against::Difficulty {
                            formula: Formula {
                                base: 10,
                                skill: None,
                                attribute: None,
                            },
                        }
                    },
                });
                rules.abilities.push(ability);
                species.modifiers.extend([
                    Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Add {
                            stat: Stat::Edge,
                            value: 5,
                        },
                    },
                    Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Add {
                            stat: Stat::CheckEdge,
                            value: -6,
                        },
                    },
                    Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::GrantAbility {
                            ability: ability_id.clone(),
                        },
                    },
                ]);
            },
        );
        game.apply_condition(
            ActorId(1),
            &ConditionId::new("distracted").unwrap(),
            ActorId(1),
            500,
        )
        .unwrap();
        let captured = game.ability_plan(ActorId(1), &ability_id, target).unwrap();
        let ability = captured.as_ability().unwrap();
        let strike = &ability.strikes[0];
        let check = strike.check.as_ref().unwrap();
        assert_eq!(
            check.edge,
            if opposed || target == ActorId(1) {
                -2
            } else {
                -1
            }
        );
        if opposed {
            let super::CheckThreshold::Opposed { edge, .. } = check.threshold else {
                panic!("opposed")
            };
            assert_eq!(
                edge, -1,
                "target role retains shared Edge even when both roles have one identity"
            );
        }
        assert_eq!(strike.edge, 5);
        assert_eq!(strike.shared_check.is_some(), opposed);
        assert_eq!(
            ability.tokens.len(),
            usize::from(opposed || target == ActorId(1))
        );
        assert!(ability.valid(game.content().rules()));
        let mut trace = ResolutionTrace::default();
        super::resolve(
            captured.resolution().unwrap(),
            &super::Snapshot::capture(&game.creature(target).unwrap()),
            &mut 42,
            &mut trace,
        );
        let check_trace = trace
            .steps()
            .iter()
            .find(|step| step.id == "check")
            .unwrap();
        assert_eq!(
            check_trace
                .fields
                .iter()
                .find(|field| field.id == "shared_damage_edge")
                .unwrap()
                .value,
            TraceValueRecord::Signed(if opposed { 2 } else { 5 })
        );
    }
}

#[test]
fn self_target_shared_roles_resistance_keeps_the_full_damage_budget() {
    self_target_shared_roles(false);
}

#[test]
fn self_target_shared_roles_opposed_check_debits_only_the_user_role() {
    self_target_shared_roles(true);
}

fn frozen_strike(ordinal: u16, offset: u64) -> Value {
    json!({
        "offset": offset,
        "origin": {"kind": "ability", "ordinal": ordinal},
        "check": null,
        "on_success": [],
        "on_failure": [],
        "descriptors": [],
        "riders": [],
        "edge": 0,
        "shared_check": null
    })
}

fn frozen_plan(strikes: Vec<Value>) -> Value {
    json!({
        "actor": 1,
        "target": 2,
        "ability": "basic_melee",
        "preparation": 10,
        "recovery": 20,
        "cost": null,
        "reach": {"kind": "melee"},
        "strikes": strikes,
        "tokens": []
    })
}

#[test]
fn ordered_strikes_decode_one_immutable_plan_without_a_cursor() {
    let value = frozen_plan(vec![
        frozen_strike(0, 0),
        frozen_strike(1, 10),
        frozen_strike(2, 20),
    ]);
    let decoded: AbilityPlan = serde_json::from_value(value.clone())
        .expect("ordered strike plans must decode through the production plan type");
    assert_eq!(serde_json::to_value(&decoded).unwrap(), value);

    let mut mutated = value;
    mutated["next_strike"] = json!(1);
    assert!(serde_json::from_value::<AbilityPlan>(mutated).is_err());
}

#[test]
fn ordered_strikes_decode_bounded_zero_offset_strikes_and_riders() {
    let mut first = frozen_strike(0, 0);
    first["riders"] = json!([{
        "trigger": "success", "check": {
            "actor": "user", "bonus": 0,
            "threshold": {"kind": "fixed", "value": 10}, "edge": -1
        },
        "effects": [], "descriptors": []
    }]);
    first["edge"] = json!(5);
    first["shared_check"] = json!({"phase": "rider", "ordinal": 0});
    let value = frozen_plan(vec![first, frozen_strike(1, 0)]);
    let decoded: AbilityPlan = serde_json::from_value(value.clone())
        .expect("a checkless primary may reserve shared edge for an authored checked rider");
    assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
}

#[test]
fn ordered_strikes_reject_unbounded_or_missing_frozen_lists_on_decode() {
    let mut missing = frozen_plan(vec![frozen_strike(0, 0)]);
    missing.as_object_mut().unwrap().remove("strikes");
    assert!(serde_json::from_value::<AbilityPlan>(missing).is_err());

    let over_bound = frozen_plan((0..17).map(|ordinal| frozen_strike(ordinal, 0)).collect());
    assert!(serde_json::from_value::<AbilityPlan>(over_bound).is_err());

    let mut too_many_riders = frozen_strike(0, 0);
    too_many_riders["riders"] = json!((0..9)
        .map(|_| json!({
            "trigger": "success", "check": null, "effects": [], "descriptors": []
        }))
        .collect::<Vec<_>>());
    assert!(serde_json::from_value::<AbilityPlan>(frozen_plan(vec![too_many_riders])).is_err());
}

fn sequence_game(offsets: &[u64]) -> (crate::Game, crate::IntentionId) {
    sequence_game_with(offsets, |_| {})
}
fn sequence_game_with(
    offsets: &[u64],
    configure: impl FnOnce(&mut crate::rules::AbilityDef),
) -> (crate::Game, crate::IntentionId) {
    sequence_game_custom(offsets, configure, |_, _| {})
}
fn sequence_game_custom(
    offsets: &[u64],
    configure: impl FnOnce(&mut crate::rules::AbilityDef),
    configure_sources: impl FnOnce(&mut crate::rules::RulesetDocument, &mut crate::creatures::Species),
) -> (crate::Game, crate::IntentionId) {
    sequence_game_on_world(offsets, configure, configure_sources, None)
}
fn sequence_game_on_world(
    offsets: &[u64],
    configure: impl FnOnce(&mut crate::rules::AbilityDef),
    configure_sources: impl FnOnce(&mut crate::rules::RulesetDocument, &mut crate::creatures::Species),
    regions: Option<u64>,
) -> (crate::Game, crate::IntentionId) {
    use crate::rules::*;
    use crate::{Action, ActorId, CreatureIdentity, IntentionOrigin};
    let mut document = crate::test_creatures::ruleset().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|ability| ability.basic_attack)
        .unwrap();
    ability.cost = None;
    ability.timing = Timing {
        source: TimingSource::Fixed {
            prepare: 10,
            recover: 20,
        },
        scaled_by: None,
    };
    ability.strikes = StrikeSource::Authored {
        strikes: offsets
            .iter()
            .map(|offset| StrikeDef {
                offset: *offset,
                unlock: None,
                check: None,
                on_success: vec![Effect::Damage {
                    damage: DamageExpr::Components {
                        components: vec![DamageComponentExpr {
                            category: crate::combat::DamageType::Impact,
                            descriptor: None,
                            amount: AmountExpr::Fixed { amount: 3 },
                        }],
                    },
                }],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            })
            .collect(),
    };
    configure(ability);
    let id = ability.id.clone();
    let mut species = crate::test_creatures::species();
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::Health,
            value: 100,
        },
    });
    configure_sources(&mut document, &mut species);
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: [(species_id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content.clone(),
        species_id,
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
            .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    let mut game = match regions {
        Some(regions) => crate::Game::region_corridor(42, regions, content),
        None => crate::Game::two_room_in_stone(42, content),
    };
    for x in [1, 2] {
        let actor = game
            .spawn_actor(
                tor_world::Location {
                    region: tor_world::RegionId(1),
                    position: tor_world::Position { x, y: 1, z: 0 },
                },
                std::num::NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("subject {x}"),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
    }
    let intention = game
        .admit_intention(
            ActorId(1),
            Action::UseAbility {
                id,
                target: ActorId(2),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 1000;
    (game, intention)
}

#[test]
fn ordered_strikes_schedule_keeps_frozen_prefix_and_starts_recovery_after_last() {
    let (mut game, _) = sequence_game(&[0, 10, 20]);
    let original = game.preparation(crate::ActorId(1)).unwrap().plan.clone();
    for (tick, hp, completed) in [(10, 97, 1), (20, 94, 2)] {
        game.tick = tick;
        game.resolve_preparations().unwrap();
        assert_eq!(game.health(crate::ActorId(2)).unwrap().0, hp);
        let preparation = game
            .preparation(crate::ActorId(1))
            .expect("a resolved prefix retains its tail");
        assert_eq!(
            preparation.plan, original,
            "cursor advancement cannot mutate frozen operands"
        );
        assert_eq!(
            serde_json::to_value(preparation).unwrap()["progress"],
            json!({"kind":"ability","next_strike":completed})
        );
    }
    game.tick = 30;
    game.resolve_preparations().unwrap();
    assert_eq!(game.health(crate::ActorId(2)).unwrap().0, 91);
    assert!(game.preparation(crate::ActorId(1)).is_none());
    assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 50);
}

#[test]
fn ordered_strikes_in_place_abandonment_pays_active_or_frozen_remaining_tail() {
    for (pause, cancel, ready) in [(false, 15, 50), (true, 35, 70)] {
        let (mut game, intention) = sequence_game(&[0, 10, 20]);
        game.tick = 10;
        game.resolve_preparations().unwrap();
        assert!(
            game.preparation(crate::ActorId(1)).is_some(),
            "the tail remains committed"
        );
        game.tick = 15;
        if pause {
            game.pause_preparation(crate::ActorId(1)).unwrap();
        }
        game.tick = cancel;
        game.cancel_intention(crate::ActorId(1), intention).unwrap();
        assert!(game.preparation(crate::ActorId(1)).is_none());
        assert_eq!(game.actors[&crate::ActorId(1)].ready_at, ready);
        assert_eq!(game.health(crate::ActorId(2)).unwrap().0, 97);
    }
}

#[test]
fn ordered_strikes_resume_reanchors_only_the_unfinished_work() {
    let (mut game, intention) = sequence_game(&[0, 10, 20]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert!(game.preparation(crate::ActorId(1)).is_some());
    game.tick = 15;
    game.pause_preparation(crate::ActorId(1)).unwrap();
    let frozen = game.preparation(crate::ActorId(1)).unwrap().plan.clone();
    let rng = game.rules_rng;
    game.tick = 25;
    game.start_work(frozen.clone(), Some(intention)).unwrap();
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.preparation(crate::ActorId(1)).unwrap().plan, frozen);
    for tick in [30, 40] {
        game.tick = tick;
        game.resolve_preparations().unwrap();
    }
    assert_eq!(game.health(crate::ActorId(2)).unwrap().0, 91);
    assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 60);
}

#[test]
fn ordered_strikes_reserve_later_check_owner_before_primary_damage_even_when_lethal() {
    use crate::damage::{DamageComponent, DamageSpec};
    use crate::dice::DicePool;
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    let mut strike = frozen_strike(0, 0);
    strike["edge"] = json!(5);
    strike["shared_check"] = json!({"phase":"rider","ordinal":0});
    strike["riders"] = json!([{"trigger": "success", "check":{"actor":"user","bonus":0,"threshold":{"kind":"fixed","value":0},"edge":-1},"effects":[],"descriptors":[]}]);
    strike["on_success"] = serde_json::to_value(vec![super::PlannedEffect::Damage {
        damage: DamageSpec::new(
            vec![DamageComponent::rolled(
                crate::combat::DamageType::Impact,
                None,
                DicePool::new(3, 6, 100).unwrap(),
            )],
            None,
        )
        .unwrap(),
    }])
    .unwrap();
    let plan: AbilityPlan = serde_json::from_value(frozen_plan(vec![strike])).unwrap();
    let creature = crate::creatures::CreatureState::new(crate::test_creatures::build(
        crate::test_creatures::species(),
    ))
    .unwrap();
    let mut trace = ResolutionTrace::default();
    super::resolve(
        &plan,
        &super::Snapshot::capture(&creature),
        &mut 42,
        &mut trace,
    );
    let component = trace
        .steps()
        .iter()
        .find(|step| step.id == "damage.component")
        .unwrap();
    assert_eq!(component.fields.iter().find(|field|field.id=="edge").unwrap().value,TraceValueRecord::Signed(2),
        "S=5 reserves three for its later owner even though Q=-1 and lethal primary skips the rider");
}

#[test]
fn ordered_strikes_escape_is_one_atomic_ordinary_move_with_tail_debt() {
    use crate::{Action, ActorId};
    use tor_world::Direction;
    let (mut game, intention) = sequence_game(&[0, 10, 20]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.tick = 15;
    let original = game.clone();
    // East is occupied by the target. Reject without retiring queue, plan or RNG.
    assert!(game
        .cancel_intention_with_step(ActorId(1), intention, Some(Direction::East))
        .is_err());
    assert_eq!(game, original);
    assert!(game.act(ActorId(1), Action::Move(Direction::West)).is_err());
    assert_eq!(game, original);
    game.cancel_intention_with_step(ActorId(1), intention, Some(Direction::West))
        .unwrap();
    assert_eq!(game.actors[&ActorId(1)].location.position.x, 0);
    assert_eq!(game.actors[&ActorId(1)].ready_at, 150);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 97);
    let escaped = game.clone();
    assert!(game
        .cancel_intention_with_step(ActorId(1), intention, Some(Direction::West))
        .is_err());
    assert_eq!(game, escaped);
}

#[test]
fn ordered_strikes_equal_offsets_have_ordered_events_and_one_terminal_event() {
    use crate::combat::{CombatEvent, SequenceEnd};
    let (mut game, intention) = sequence_game(&[0, 0, 0]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert_eq!(game.health(crate::ActorId(2)).unwrap().0, 91);
    assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 30);
    let ordinals: Vec<_> = game
        .combat_events()
        .iter()
        .filter_map(|event| match event {
            CombatEvent::Resolved {
                strike: Some(ordinal),
                ..
            } => Some(*ordinal),
            _ => None,
        })
        .collect();
    assert_eq!(ordinals, [0, 1, 2]);
    assert_eq!(game.combat_events().iter().filter(|event| matches!(event,
        CombatEvent::SequenceEnded {intention: Some(id), reason: SequenceEnd::Completed, ..} if *id == intention)).count(), 1);
}

#[test]
fn ordered_strikes_target_leaves_then_returns_without_erasing_or_replaying_prefix() {
    use crate::ActorId;
    let (mut game, _) = sequence_game(&[0, 10, 20]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let home = game.actors[&ActorId(2)].location;
    let mut distant = home;
    distant.position.x = 4;
    game.teleport(ActorId(2), distant).unwrap();
    game.tick = 20;
    let rng = game.rules_rng;
    game.resolve_preparations().unwrap();
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 97);
    assert_eq!(game.preparation(ActorId(1)).unwrap().next_strike(), Some(2));
    game.teleport(ActorId(2), home).unwrap();
    game.tick = 30;
    game.resolve_preparations().unwrap();
    assert_eq!(game.health(ActorId(2)).unwrap().0, 94);
    assert_eq!(game.actors[&ActorId(1)].ready_at, 50);
}

#[test]
fn ordered_strikes_checkless_zero_damage_primary_runs_ordered_riders_but_death_stops_them() {
    use crate::rules::{
        AmountExpr, DamageComponentExpr, DamageExpr, Effect, RiderDef, StrikeSource,
    };
    for lethal in [false, true] {
        let (mut game, _) = sequence_game_with(&[0], |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            let damage = |amount| Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: crate::combat::DamageType::Impact,
                        descriptor: None,
                        amount: AmountExpr::Fixed { amount },
                    }],
                },
            };
            strikes[0].on_success = vec![damage(if lethal { 100 } else { 0 })];
            strikes[0].riders = vec![
                RiderDef {
                    trigger: crate::rules::RiderTrigger::Success,
                    check: None,
                    effects: vec![damage(4)],
                    descriptors: vec![],
                },
                RiderDef {
                    trigger: crate::rules::RiderTrigger::Success,
                    check: None,
                    effects: vec![damage(5)],
                    descriptors: vec![],
                },
            ];
        });
        game.tick = 10;
        game.resolve_preparations().unwrap();
        assert_eq!(
            game.health(crate::ActorId(2)).unwrap().0,
            if lethal { 0 } else { 91 }
        );
        let resolved = game
            .combat_events()
            .iter()
            .filter(|event| matches!(event, crate::combat::CombatEvent::Resolved { .. }))
            .count();
        assert_eq!(resolved, if lethal { 1 } else { 3 });
    }
}

#[test]
fn ordered_strikes_snapshot_copies_keep_original_operands_when_one_copy_is_edited() {
    let (game, _) = sequence_game(&[0, 10, 20]);
    let original = game.preparation(crate::ActorId(1)).unwrap().plan.clone();
    let captured = serde_json::to_value(&original).unwrap();
    let mut copy = original.clone();
    let super::Plan::Ability(plan) = &mut copy else {
        unreachable!()
    };
    plan.strikes[1].edge = 3;
    plan.strikes[1].on_success.clear();
    assert_eq!(serde_json::to_value(&original).unwrap(), captured);
    assert_eq!(game.preparation(crate::ActorId(1)).unwrap().plan, original);
    assert_ne!(copy, original);
}

#[test]
fn ordered_strikes_tokens_bind_only_first_qualifying_site_of_strike_zero() {
    use crate::rules::*;
    use crate::ActorId;
    for first_rider in [false, true] {
        let (mut game, _) = sequence_game_with(&[0, 10], |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            let checked = CheckDef {
                actor: CheckActor::User,
                skill: SkillSelector::SelectedWeapon,
                against: Against::Defense {
                    defense: DefenseKind::Physical,
                },
            };
            strikes[1].check = Some(checked);
            if first_rider {
                strikes[0].riders = vec![
                    RiderDef {
                        trigger: crate::rules::RiderTrigger::Success,
                        check: Some(RiderCheckDef {
                            skill: SkillSelector::SelectedWeapon,
                            defense: DefenseKind::Physical
                        }),
                        effects: vec![],
                        descriptors: vec![]
                    };
                    2
                ];
            }
        });
        game.apply_effects(
            ActorId(1),
            &[Effect::ApplyCondition {
                condition: crate::content_ids::ConditionId::new("readied").unwrap(),
                duration: 100,
            }],
        )
        .unwrap();
        let ability = game
            .preparation(ActorId(1))
            .unwrap()
            .plan
            .as_ability()
            .unwrap()
            .ability
            .clone();
        let plan = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
        let captured = plan.as_ability().unwrap();
        assert_eq!(captured.tokens.len(), usize::from(first_rider));
        assert_eq!(captured.strikes[1].check.as_ref().unwrap().edge, 0);
        if first_rider {
            assert_eq!(
                captured.tokens[0].site,
                super::CheckSite {
                    strike: 0,
                    check: super::CheckPhase::Rider { ordinal: 0 }
                }
            );
            assert_eq!(
                captured.strikes[0].riders[0].check.as_ref().unwrap().edge,
                1
            );
            assert_eq!(
                captured.strikes[0].riders[1].check.as_ref().unwrap().edge,
                0
            );
        }
        assert_eq!(game.creature(ActorId(1)).unwrap().conditions().qualifying_tokens(game.content.rules(), ActorId(1), false).len(), 1,
            "read-only capture neither consumes an unclaimed token nor mutates the earlier preparation");
    }
}

#[test]
fn ordered_strikes_paid_prefix_has_no_hold_and_resume_never_pays_again() {
    use crate::{resources::Resource, ActorId};
    let (mut game, intention) = sequence_game_with(&[0, 10, 20], |ability| {
        ability.cost = Some(crate::rules::Cost {
            resource: Resource::Stamina,
            start: 1,
            resolution: 1,
        });
    });
    assert!(game.preparation(ActorId(1)).unwrap().hold.is_some());
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert!(game.preparation(ActorId(1)).unwrap().hold.is_none());
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    game.pause_preparation(ActorId(1)).unwrap();
    let before = game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .resources()
        .balance(Resource::Stamina);
    let plan = game.preparation(ActorId(1)).unwrap().plan.clone();
    game.start_work(plan, Some(intention)).unwrap();
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .costs()
            .resources()
            .balance(Resource::Stamina),
        before
    );
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    let mut expected = game.creature(ActorId(1)).unwrap().into_owned();
    expected.advance_active(20);
    for tick in [20, 30] {
        game.tick = tick;
        game.resolve_preparations().unwrap();
    }
    assert_eq!(
        game.creature(ActorId(1)).unwrap().costs().resources(),
        expected.costs().resources()
    );
}

#[test]
fn ordered_strikes_checkpoint_tail_roundtrips_and_rejects_impossible_cursor_or_hold() {
    use crate::{checkpoint::SharedState, ActorId};
    let (mut game, _) = sequence_game(&[0, 10, 20]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut restored = crate::Game::restore_checkpoint(
        serde_json::from_value(snapshot.clone()).unwrap(),
        &shared,
        game.content().clone(),
    )
    .unwrap();
    assert_eq!(restored, game);
    for tick in [20, 30] {
        game.tick = tick;
        restored.tick = tick;
        game.resolve_preparations().unwrap();
        restored.resolve_preparations().unwrap();
    }
    assert_eq!(restored, game);
    for (field, value) in [
        ("progress", json!({"kind":"ability","next_strike":3})),
        ("progress", json!({"kind":"item"})),
        ("hold", json!(1)),
        ("remaining", json!(11)),
        ("started", json!(11)),
    ] {
        let mut forged = serde_json::to_value(&shared).unwrap();
        forged["actors"][snapshot["actors"].as_u64().unwrap() as usize]["1"]["pending"][field] =
            value;
        let Ok(forged) = serde_json::from_value::<SharedState>(forged) else {
            continue;
        };
        assert!(crate::Game::restore_checkpoint(
            serde_json::from_value(snapshot.clone()).unwrap(),
            &forged,
            game.content().clone()
        )
        .is_err());
    }
    assert!(game.preparation(ActorId(1)).is_none());
}

#[test]
fn ordered_strikes_health_quiet_resets_once_across_riders_and_again_on_later_strike() {
    use crate::{rules::*, ActorId};
    for self_damage in [false, true] {
        let (mut game, _) = sequence_game_custom(
            &[0, 10],
            |ability| {
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                for strike in strikes {
                    strike.riders = vec![
                        RiderDef {
                            trigger: crate::rules::RiderTrigger::Success,
                            check: None,
                            effects: strike.on_success.clone(),
                            descriptors: vec![]
                        };
                        2
                    ];
                }
            },
            |_, species| {
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::HealthRecoveryResetPercent,
                        value: -50,
                    },
                })
            },
        );
        for actor in [ActorId(1), ActorId(2)] {
            crate::test_creatures::injure(&mut game, actor, 10).unwrap();
        }
        game.tick = 200;
        game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 200;
        for actor in [ActorId(1), ActorId(2)] {
            game.settle_actor_time(actor).unwrap();
        }
        let mut plan = game
            .preparation(ActorId(1))
            .unwrap()
            .plan
            .as_ability()
            .unwrap()
            .clone();
        if self_damage {
            plan.target = plan.actor;
            plan.reach = Reach::SelfOnly;
        }
        game.resolve_strike(&plan, 0, None).unwrap();
        for actor in [plan.actor, plan.target] {
            assert_eq!(
                game.creature(actor).unwrap().health_recovery().elapsed(),
                100,
                "one 50% reset spans primary and every rider, including self-damage"
            );
        }
        game.resolve_strike(&plan, 1, None).unwrap();
        for actor in [plan.actor, plan.target] {
            assert_eq!(
                game.creature(actor).unwrap().health_recovery().elapsed(),
                50
            );
        }
    }
}

#[test]
fn ordered_strikes_health_policy_is_selected_at_first_positive_damage_not_first_batch() {
    use crate::{rules::*, ActorId};
    for primary_damage in [false, true] {
        let (mut game, _) = sequence_game_custom(
            &[0],
            |ability| {
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                let damage = strikes[0].on_success.clone();
                if !primary_damage {
                    strikes[0].on_success.clear();
                }
                strikes[0].riders = vec![
                    RiderDef {
                        trigger: crate::rules::RiderTrigger::Success,
                        check: None,
                        effects: vec![Effect::ApplyCondition {
                            condition: crate::content_ids::ConditionId::new("braced").unwrap(),
                            duration: 1000,
                        }],
                        descriptors: vec![],
                    },
                    RiderDef {
                        trigger: crate::rules::RiderTrigger::Success,
                        check: None,
                        effects: damage,
                        descriptors: vec![],
                    },
                ];
            },
            |rules, species| {
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::HealthRecoveryResetPercent,
                        value: -50,
                    },
                });
                rules
                    .conditions
                    .iter_mut()
                    .find(|condition| condition.id.as_str() == "braced")
                    .unwrap()
                    .modifiers = vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::HealthRecoveryResetPercent,
                        value: 50,
                    },
                }];
            },
        );
        crate::test_creatures::injure(&mut game, ActorId(2), 10).unwrap();
        game.pause_preparation(ActorId(1)).unwrap();
        game.tick = 200;
        game.actors.get_mut(&ActorId(1)).unwrap().ready_at = 200;
        game.settle_actor_time(ActorId(2)).unwrap();
        let plan = game
            .preparation(ActorId(1))
            .unwrap()
            .plan
            .as_ability()
            .unwrap()
            .clone();
        game.resolve_strike(&plan, 0, None).unwrap();
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .health_recovery()
                .elapsed(),
            if primary_damage { 100 } else { 0 }
        );
    }
}

#[test]
fn ordered_strikes_terminal_event_rejects_unknown_or_missing_fields() {
    use crate::combat::CombatEvent;
    let event = json!({"SequenceEnded":{"actor":1,"intention":null,"reason":"completed"}});
    assert!(serde_json::from_value::<CombatEvent>(event.clone()).is_ok());
    let mut unknown = event.clone();
    unknown["SequenceEnded"]["future_strikes"] = json!([1, 2]);
    assert!(serde_json::from_value::<CombatEvent>(unknown).is_err());
    for field in ["actor", "intention", "reason"] {
        let mut missing = event.clone();
        missing["SequenceEnded"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<CombatEvent>(missing).is_err());
    }
}

#[test]
fn ordered_strikes_maximum_plan_reuses_capture_and_never_clones_actor_tables_per_rider() {
    use crate::rules::{RiderDef, StrikeSource};
    let (mut game, _) = sequence_game_with(&[0; crate::limits::STRIKES], |ability| {
        let StrikeSource::Authored { strikes } = &mut ability.strikes else {
            unreachable!()
        };
        for strike in strikes {
            strike.on_success.clear();
            strike.riders = vec![
                RiderDef {
                    trigger: crate::rules::RiderTrigger::Success,
                    check: None,
                    effects: vec![],
                    descriptors: vec![]
                };
                crate::limits::RIDERS_PER_STRIKE
            ];
        }
    });
    let before = crate::diagnostics::work_counts();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.ability_plans, before.ability_plans,
        "no operand recapture at any resolution boundary"
    );
    assert_eq!(
        after.actor_table_copies, before.actor_table_copies,
        "riders publish actor-local outcomes inside the existing transaction"
    );
    assert_eq!(after.game_clones - before.game_clones, 0);
    assert_eq!(
        after.preparation_candidates - before.preparation_candidates,
        2
    );
    assert_eq!(after.strike_resolutions - before.strike_resolutions, 16);
    assert_eq!(after.rider_resolutions - before.rider_resolutions, 128);
    assert_eq!(
        game.combat_events()
            .iter()
            .filter(|event| matches!(event, crate::combat::CombatEvent::Resolved { .. }))
            .count(),
        16 * 9
    );
    assert_eq!(game.actors[&crate::ActorId(1)].ready_at, 30);
}

#[test]
fn ordered_strikes_opposed_target_check_reserves_attacker_side_and_reports_that_site() {
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    let mut row = frozen_strike(0, 0);
    row["edge"] = json!(5);
    row["check"] = json!({"actor":"target","bonus":0,"edge":0,"threshold":{"kind":"opposed","bonus":0,"edge":5}});
    row["shared_check"] = json!({"phase":"primary","side":"opposing"});
    let plan: AbilityPlan = serde_json::from_value(frozen_plan(vec![row])).unwrap();
    let creature = crate::creatures::CreatureState::new(crate::test_creatures::build(
        crate::test_creatures::species(),
    ))
    .unwrap();
    let mut trace = ResolutionTrace::default();
    super::resolve(
        &plan,
        &super::Snapshot::capture(&creature),
        &mut 42,
        &mut trace,
    );
    for (id, expected) in [("opposed_check", 3), ("check", 0)] {
        let record = trace.steps().iter().find(|step| step.id == id).unwrap();
        assert_eq!(
            record
                .fields
                .iter()
                .find(|field| field.id == "shared_edge_spent")
                .unwrap()
                .value,
            TraceValueRecord::Signed(expected)
        );
    }
}

#[test]
fn ordered_strikes_ai_resumes_paid_prefix_through_actual_scheduling() {
    use crate::{resources::Resource, Action, ActorId};
    let (mut game, _) = sequence_game_with(&[0, 10, 20], |ability| {
        ability.cost = Some(crate::rules::Cost {
            resource: Resource::Stamina,
            start: 1,
            resolution: 1,
        });
    });
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.tick = 15;
    game.pause_preparation(ActorId(1)).unwrap();
    let frozen = game.preparation(ActorId(1)).unwrap().plan.clone();
    game.configure_ai(
        ActorId(1),
        crate::ai::AiProfile {
            memory_ticks: 1000,
            flee_percent: 0,
        },
    )
    .unwrap();
    let expected = frozen.work().action();
    assert!(matches!(expected, Action::UseAbility { .. }));
    assert_eq!(game.next_ai_action(), Some((ActorId(1), expected.clone())));
    game.admit_ai_intention(ActorId(1)).unwrap();
    let executed = game.execute_next_intention().unwrap();
    assert_eq!(executed.action, Some(expected));
    executed.outcome.unwrap();
    assert_eq!(game.health(ActorId(2)).unwrap().0, 91);
    assert_eq!(game.actors[&ActorId(1)].ready_at, 50);
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
}

#[test]
fn ordered_strikes_fleeing_ai_uses_atomic_escape_and_old_preparation_identity() {
    use crate::{Action, ActorId};
    let (mut game, original) = sequence_game(&[0, 10, 20]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.tick = 15;
    game.pause_preparation(ActorId(1)).unwrap();
    game.actors
        .get_mut(&ActorId(2))
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .identity
        .faction = "foe".into();
    game.combat
        .hostility
        .insert("neutral".into(), ["foe".into()].into());
    game.configure_ai(
        ActorId(1),
        crate::ai::AiProfile {
            memory_ticks: 1000,
            flee_percent: 100,
        },
    )
    .unwrap();
    let (_, action) = game.next_ai_action().unwrap();
    let Action::Move(direction) = action else {
        panic!("a valid ordinary retreat is available")
    };
    let duration = crate::movement_cost(100, direction).unwrap();
    let before = game.clone();
    // The public controlled/AI cancellation boundary applies the identical debt.
    game.cancel_intention_with_step(ActorId(1), original, Some(tor_world::Direction::East))
        .unwrap_err();
    assert_eq!(game, before);
    game.admit_ai_intention(ActorId(1)).unwrap();
    let executed = game.execute_next_intention().unwrap();
    assert_eq!(executed.action, Some(Action::Move(direction)));
    executed.outcome.unwrap();
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.actors[&ActorId(1)].ready_at, 50 + duration);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 97);
    assert!(game.combat_events().iter().any(
        |event| matches!(event,crate::combat::CombatEvent::SequenceEnded {
        intention:Some(id),reason:crate::combat::SequenceEnd::Abandoned,..} if *id==original)
    ));
}

#[test]
fn ordered_strikes_escape_and_in_place_overflow_reject_without_retiring_prefix() {
    use crate::ActorId;
    for (ticks_left, step) in [(20, None), (100, Some(tor_world::Direction::West))] {
        let (mut game, intention) = sequence_game(&[0, 10, 20]);
        game.tick = 10;
        game.resolve_preparations().unwrap();
        game.tick = 15;
        game.pause_preparation(ActorId(1)).unwrap();
        game.tick = u64::MAX - ticks_left;
        for actor in [ActorId(1), ActorId(2)] {
            let mut state = game.actors.get_mut(&actor).unwrap();
            state.ready_at = game.tick;
            state.active_since = game.tick;
            if let Some(pending) = state.pending.as_mut() {
                pending.started = game.tick;
            }
        }
        let before = game.clone();
        assert!(game
            .cancel_intention_with_step(ActorId(1), intention, step)
            .is_err());
        assert_eq!(game, before);
    }
}

#[test]
fn ordered_strikes_natural_fixed_primary_captures_flat_modifier_and_remains_source_valid() {
    use crate::rules::*;
    let (game, _) = sequence_game_custom(
        &[0],
        |ability| ability.strikes = StrikeSource::SelectedMelee,
        |rules, species| {
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Ability {
                        ability: rules
                            .abilities
                            .iter()
                            .find(|ability| ability.basic_attack)
                            .unwrap()
                            .id
                            .clone(),
                        stat: AbilityStat::FlatBonus,
                    },
                    value: 5,
                },
            })
        },
    );
    let plan = &game.preparation(crate::ActorId(1)).unwrap().plan;
    assert!(game.plan_source_valid(plan));
    let super::PlannedEffect::Damage { damage } =
        &plan.as_ability().unwrap().strikes[0].on_success[0]
    else {
        panic!("natural primary")
    };
    assert_eq!(
        damage.components()[0].amount(),
        crate::damage::DamageAmount::Fixed(9)
    );
    let decoded: super::Plan = serde_json::from_value(serde_json::to_value(plan).unwrap()).unwrap();
    assert_eq!(&decoded, plan);
    assert!(game.plan_source_valid(&decoded));
}

#[test]
fn ordered_strikes_fixed_actor_work_counts_do_not_scale_with_uninvolved_regions() {
    use crate::{rules::*, ActorId};
    for paused in [false, true] {
        let mut counts = vec![];
        for regions in [16, 256] {
            let (mut game, intention) = sequence_game_on_world(
                &[0, 10, 20],
                |ability| {
                    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                        unreachable!()
                    };
                    for strike in strikes {
                        strike.riders = vec![
                            RiderDef {
                                trigger: crate::rules::RiderTrigger::Success,
                                check: Some(RiderCheckDef {
                                    skill: SkillSelector::SelectedWeapon,
                                    defense: DefenseKind::Physical
                                }),
                                effects: vec![],
                                descriptors: vec![]
                            };
                            2
                        ];
                    }
                },
                |_, _| {},
                Some(regions),
            );
            let before = crate::diagnostics::work_counts();
            game.tick = 10;
            game.resolve_preparations().unwrap();
            if paused {
                game.tick = 15;
                game.pause_preparation(ActorId(1)).unwrap();
                game.tick = 25;
                game.start_work(
                    game.preparation(ActorId(1)).unwrap().plan.clone(),
                    Some(intention),
                )
                .unwrap();
            }
            for tick in if paused { [30, 40] } else { [20, 30] } {
                game.tick = tick;
                game.resolve_preparations().unwrap();
            }
            let after = crate::diagnostics::work_counts();
            counts.push((
                after.ability_plans - before.ability_plans,
                after.actor_table_copies - before.actor_table_copies,
                after.observations - before.observations,
                after.scenes - before.scenes,
                after.body_cells - before.body_cells,
                after.actor_candidates - before.actor_candidates,
                after.health_derivations - before.health_derivations,
                after.game_clones - before.game_clones,
                after.preparation_candidates - before.preparation_candidates,
                after.strike_resolutions - before.strike_resolutions,
                after.rider_resolutions - before.rider_resolutions,
            ));
        }
        assert_eq!(
            counts[0], counts[1],
            "fixed actors and strike sites must not scan uninvolved regions"
        );
        assert_eq!(
            counts[0].0, 0,
            "resolution and resume never recapture ability plans"
        );
        assert_eq!(counts[0].1, 0, "no actor table copy per rider or resume");
        assert_eq!(counts[0].7, 0, "no full Game snapshot per rider or resume");
        assert_eq!(
            counts[0].8, 6,
            "two actors visited in each of three schedules"
        );
        assert_eq!((counts[0].9, counts[0].10), (3, 6));
    }
}

#[test]
fn ordered_strikes_compiler_scopes_shared_sources_before_later_check_caps() {
    use crate::{rules::*, ActorId};
    for condition_family in [false, true] {
        let modifiers = vec![
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Edge,
                    value: 5,
                },
            },
            Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::CheckEdge,
                    value: -6,
                },
            },
        ];
        let (mut game, _) = sequence_game_custom(
            &[0, 10],
            |ability| {
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                for strike in strikes {
                    strike.riders = vec![
                        RiderDef {
                            trigger: crate::rules::RiderTrigger::Success,
                            check: Some(RiderCheckDef {
                                skill: SkillSelector::NaturalAttack,
                                defense: DefenseKind::Physical
                            }),
                            effects: vec![],
                            descriptors: vec![],
                        };
                        2
                    ];
                }
            },
            |rules, species| {
                if condition_family {
                    rules
                        .conditions
                        .iter_mut()
                        .find(|condition| condition.id.as_str() == "braced")
                        .unwrap()
                        .modifiers = modifiers.clone();
                } else {
                    species.modifiers.extend(modifiers.clone());
                }
            },
        );
        if condition_family {
            game.apply_condition(
                ActorId(1),
                &crate::content_ids::ConditionId::new("braced").unwrap(),
                ActorId(1),
                500,
            )
            .unwrap();
        }
        let plan = game
            .ability_plan(
                ActorId(1),
                &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                ActorId(2),
            )
            .unwrap();
        for strike in &plan.as_ability().unwrap().strikes {
            assert_eq!(strike.edge, 5);
            assert_eq!(
                strike.shared_check,
                Some(super::CheckPhase::Rider { ordinal: 0 })
            );
            assert_eq!(
                strike.riders[0].check.as_ref().unwrap().edge,
                -1,
                "the first attacker site combines S=5 and K=-6 before the final check cap"
            );
            assert_eq!(
                strike.riders[1].check.as_ref().unwrap().edge,
                if condition_family { -1 } else { -6 },
                "later checks exclude shared sources before scoped family caps"
            );
        }
    }
}

#[test]
fn ordered_strikes_later_conditions_cannot_rederive_suspended_plan_or_its_copy() {
    use crate::ActorId;
    let (mut game, intention) = sequence_game(&[0, 10, 20]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.tick = 15;
    game.pause_preparation(ActorId(1)).unwrap();
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    let encoded = serde_json::to_value(&captured).unwrap();
    game.apply_condition(
        ActorId(1),
        &crate::content_ids::ConditionId::new("readied").unwrap(),
        ActorId(1),
        500,
    )
    .unwrap();
    game.apply_condition(
        ActorId(2),
        &crate::content_ids::ConditionId::new("braced").unwrap(),
        ActorId(2),
        500,
    )
    .unwrap();
    game.tick = 25;
    let resumed = game.plan_work(ActorId(1), captured.work()).unwrap();
    assert_eq!(resumed, captured);
    game.start_work(resumed, Some(intention)).unwrap();
    for tick in [30, 40] {
        game.tick = tick;
        game.resolve_preparations().unwrap();
        assert_eq!(serde_json::to_value(&captured).unwrap(), encoded);
    }
    assert_eq!(game.actors[&ActorId(1)].ready_at, 60);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .conditions()
            .qualifying_tokens(game.content.rules(), ActorId(1), false)
            .len(),
        1,
        "a newly granted token belongs to a future preparation"
    );
}

#[test]
fn ordered_strikes_checkpoint_preserves_captured_natural_source_across_transformation() {
    use crate::{checkpoint::SharedState, rules::StrikeSource, ActorId};
    let (mut game, _) = sequence_game_custom(
        &[0],
        |ability| {
            ability.strikes = StrikeSource::SelectedMelee;
        },
        |_, species| {
            let natural = &species.melee;
            let strikes = [0, 10, 20]
                .into_iter()
                .map(|offset| {
                    let mut strike = natural.strikes()[0].clone();
                    strike.offset = offset;
                    strike
                })
                .collect();
            species.melee = crate::attacks::NaturalAttack::new(
                natural.skill(),
                natural.bonus(),
                natural.wind_up(),
                natural.recovery(),
                strikes,
            )
            .unwrap();
        },
    );
    let mut catalog = game.content().catalog().clone();
    let mut other = catalog.species.values().next().unwrap().clone();
    other.id = crate::content_ids::SpeciesId::new("other_species").unwrap();
    let natural = &other.melee;
    other.melee = crate::attacks::NaturalAttack::new(
        natural.skill(),
        natural.bonus() + 7,
        natural.wind_up(),
        natural.recovery(),
        natural.strikes().to_vec(),
    )
    .unwrap();
    catalog.species.insert(other.id.clone(), other.clone());
    let content = crate::creatures::Content::new(game.content().rules().clone(), catalog).unwrap();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut game = crate::Game::restore_checkpoint(snapshot, &shared, content.clone()).unwrap();
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    let old_species = game
        .creature(ActorId(1))
        .unwrap()
        .build()
        .species_id()
        .clone();
    let build = game.creature(ActorId(1)).unwrap().build().clone();
    let transformed = crate::creatures::CreatureBuild::new(
        content.clone(),
        other.id.clone(),
        build.ledger().clone(),
        build.binding(),
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    game.rebuild_creature(ActorId(1), transformed).unwrap();
    assert_eq!(
        game.creature(ActorId(1)).unwrap().build().species_id(),
        &other.id
    );
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, captured);
    assert!(captured.as_ability().unwrap().strikes.iter().all(|strike|
        matches!(&strike.origin, super::StrikeOrigin::Natural { species, .. } if species == &old_species)));
    let fresh = game
        .ability_plan(
            ActorId(1),
            &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    assert_ne!(
        fresh.as_ability().unwrap().strikes[0].check,
        captured.as_ability().unwrap().strikes[0].check
    );
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let mut restored = crate::Game::restore_checkpoint(
        serde_json::from_value(snapshot.clone()).unwrap(),
        &shared,
        content.clone(),
    )
    .unwrap();
    assert_eq!(restored, game);
    for tick in [20, 30] {
        game.tick = tick;
        restored.tick = tick;
        game.resolve_preparations().unwrap();
        restored.resolve_preparations().unwrap();
    }
    assert_eq!(restored, game);
    // Frozen identity is checked against the pinned catalog, not today's body.
    // Historical capture authenticity belongs to deterministic journal replay.
    for unknown_source in [false, true] {
        let mut forged = serde_json::to_value(&shared).unwrap();
        let plan = &mut forged["actors"][snapshot["actors"].as_u64().unwrap() as usize]["1"]
            ["pending"]["plan"];
        if unknown_source {
            for strike in plan["strikes"].as_array_mut().unwrap() {
                strike["origin"]["species"] = json!("missing_species");
            }
        } else {
            plan["strikes"][0]["origin"]["ordinal"] = json!(15);
        }
        let forged: SharedState = serde_json::from_value(forged).unwrap();
        assert!(crate::Game::restore_checkpoint(
            serde_json::from_value(snapshot.clone()).unwrap(),
            &forged,
            content.clone()
        )
        .is_err());
    }
}

#[test]
fn ordered_strikes_frozen_or_detached_tail_preserves_cursor_and_shifts_commitment_once() {
    use crate::{ActorId, MemoryRecords, RegionTransition};
    use tor_world::RegionId;
    for detach in [false, true] {
        let (mut game, intention) = sequence_game(&[0, 10, 20]);
        game.tick = 10;
        game.resolve_preparations().unwrap();
        game.tick = 15;
        let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
        let health_work = game
            .creature(ActorId(2))
            .unwrap()
            .health_recovery()
            .elapsed();
        let mut records = MemoryRecords::default();
        let (frozen, _) = game
            .transition_regions(
                &RegionTransition {
                    active: Default::default(),
                    loaded: if detach {
                        Default::default()
                    } else {
                        [RegionId(1), RegionId(2)].into()
                    },
                },
                &mut records,
            )
            .unwrap();
        assert!(frozen.active.is_empty());
        let rng = game.rules_rng;
        game.tick = 1015;
        game.resolve_preparations().unwrap();
        assert_eq!(game.rules_rng, rng);
        let before = game.clone();
        assert!(game
            .cancel_intention_with_step(ActorId(1), intention, Some(tor_world::Direction::West))
            .is_err());
        assert_eq!(
            game, before,
            "a frozen actor cannot escape through inactive space"
        );
        game.transition_regions(
            &RegionTransition {
                active: [RegionId(1), RegionId(2)].into(),
                loaded: [RegionId(1), RegionId(2)].into(),
            },
            &mut records,
        )
        .unwrap();
        assert_eq!(game.preparation(ActorId(1)).unwrap().plan, captured);
        assert_eq!(game.preparation(ActorId(1)).unwrap().next_strike(), Some(1));
        assert_eq!(game.actors[&ActorId(1)].ready_at, 1020);
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .health_recovery()
                .elapsed(),
            health_work
        );
        for tick in [1020, 1030] {
            game.tick = tick;
            game.resolve_preparations().unwrap();
        }
        assert_eq!(game.health(ActorId(2)).unwrap().0, 91);
        assert_eq!(game.actors[&ActorId(1)].ready_at, 1050);
    }
}

#[test]
fn ordered_strikes_diagnostics_count_full_game_clones_independently_of_shared_tables() {
    let (game, _) = sequence_game(&[0, 10, 20]);
    let before = crate::diagnostics::work_counts();
    let snapshot = game.clone();
    let after = crate::diagnostics::work_counts();
    assert_eq!(after.game_clones - before.game_clones, 1);
    assert_eq!(after.actor_table_copies - before.actor_table_copies, 0);
    assert_eq!(snapshot, game);
}

#[test]
fn ordered_strikes_scheduler_counts_sparse_work_at_two_and_256_actor_density() {
    use crate::{rules::*, ActorId};
    for equal_offsets in [false, true] {
        for actor_count in [2, 256] {
            let offsets = if equal_offsets {
                vec![0; 16]
            } else {
                vec![0, 10, 20]
            };
            let rider_count = if equal_offsets { 8 } else { 2 };
            let (mut game, _) = sequence_game_on_world(
                &offsets,
                |ability| {
                    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                        unreachable!()
                    };
                    for strike in strikes {
                        strike.riders = vec![
                            RiderDef {
                                trigger: crate::rules::RiderTrigger::Success,
                                check: Some(RiderCheckDef {
                                    skill: SkillSelector::SelectedWeapon,
                                    defense: DefenseKind::Physical
                                }),
                                effects: vec![],
                                descriptors: vec![],
                            };
                            rider_count
                        ];
                    }
                },
                |_, _| {},
                Some(16),
            );
            for index in 0..actor_count - 2 {
                let slot = index % 36;
                let id = game
                    .spawn_actor(
                        tor_world::Location {
                            region: tor_world::RegionId(2 + (index / 36) as u64),
                            position: tor_world::Position {
                                x: slot % 12,
                                y: slot / 12,
                                z: 0,
                            },
                        },
                        std::num::NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                game.actors.get_mut(&id).unwrap().ready_at = 1000;
            }
            let before = crate::diagnostics::work_counts();
            let ticks: &[u64] = if equal_offsets { &[10] } else { &[10, 20, 30] };
            for &tick in ticks {
                game.tick = tick;
                game.resolve_preparations().unwrap();
            }
            let after = crate::diagnostics::work_counts();
            assert_eq!(
                after.preparation_candidates - before.preparation_candidates,
                actor_count as usize * ticks.len(),
                "the existing all-actor scan is measured honestly"
            );
            assert_eq!(
                after.strike_resolutions - before.strike_resolutions,
                offsets.len()
            );
            assert_eq!(
                after.rider_resolutions - before.rider_resolutions,
                offsets.len() * rider_count
            );
            assert_eq!(
                after.game_clones - before.game_clones,
                0,
                "atomic strike/rider publication never snapshots Game"
            );
            assert_eq!(after.ability_plans - before.ability_plans, 0);
            assert_eq!(after.actor_table_copies - before.actor_table_copies, 0);
            assert!(game.preparation(ActorId(1)).is_none());
        }
    }
}

#[test]
fn ordered_strikes_death_ends_source_once_but_dead_target_keeps_tail_timing() {
    use crate::{
        combat::{CombatEvent, SequenceEnd},
        rules::*,
        ActorId,
    };
    for dead in [ActorId(1), ActorId(2)] {
        let (mut game, intention) = sequence_game(&[0, 10, 20]);
        game.tick = 10;
        game.resolve_preparations().unwrap();
        game.tick = 15;
        game.apply_effects(
            dead,
            &[Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: crate::combat::DamageType::Vital,
                        descriptor: None,
                        amount: AmountExpr::Fixed { amount: 1000 },
                    }],
                },
            }],
        )
        .unwrap();
        let rng = game.rules_rng;
        for tick in [20, 30] {
            game.tick = tick;
            game.resolve_preparations().unwrap();
        }
        assert_eq!(
            game.rules_rng, rng,
            "unavailable target tails never draw dice"
        );
        assert!(game.preparation(ActorId(1)).is_none());
        let ends: Vec<_> = game
            .combat_events()
            .iter()
            .filter_map(|event| match event {
                CombatEvent::SequenceEnded {
                    actor: ActorId(1),
                    intention: Some(id),
                    reason,
                } if *id == intention => Some(*reason),
                _ => None,
            })
            .collect();
        assert_eq!(
            ends,
            [if dead == ActorId(1) {
                SequenceEnd::Abandoned
            } else {
                SequenceEnd::Completed
            }]
        );
        if dead == ActorId(2) {
            assert_eq!(game.actors[&ActorId(1)].ready_at, 50);
        } else {
            assert_eq!(game.health(ActorId(2)).unwrap().0, 97);
        }
    }
}

#[test]
fn ordered_strikes_suspended_work_and_actor_active_health_use_distinct_clocks() {
    use crate::{rules::*, ActorId};
    let (mut game, intention) = sequence_game(&[0, 10, 20]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.tick = 15;
    game.apply_effects(
        ActorId(1),
        &[Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![DamageComponentExpr {
                    category: crate::combat::DamageType::Vital,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount: 1 },
                }],
            },
        }],
    )
    .unwrap();
    assert!(!game.preparation(ActorId(1)).unwrap().active);
    let before = game.clone();
    for _ in 0..20 {
        assert_eq!(
            game.creature(ActorId(1))
                .unwrap()
                .health_recovery()
                .elapsed(),
            0
        );
        assert_eq!(
            game.preparation(ActorId(1))
                .unwrap()
                .remaining_at(game.tick),
            5
        );
    }
    assert_eq!(game, before, "queries/input waiting do not earn time");
    game.tick = 35;
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .health_recovery()
            .elapsed(),
        20
    );
    assert_eq!(
        game.preparation(ActorId(1))
            .unwrap()
            .remaining_at(game.tick),
        5
    );
    game.cancel_intention(ActorId(1), intention).unwrap();
    assert_eq!(game.actors[&ActorId(1)].ready_at, 70);
}

#[test]
fn ordered_strikes_checkpoint_rejects_invalid_frozen_offsets_ordinals_and_timing_bounds() {
    use crate::{checkpoint::SharedState, ActorId};
    let (mut game, _) = sequence_game(&[0, 10, 20]);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let original = game.clone();
    for (path, value) in [
        ("/strikes/0/offset", json!(1)),
        ("/strikes/1/offset", json!(21)),
        ("/strikes/2/offset", json!(9)),
        ("/strikes/2/offset", json!(1_000_000)),
        ("/strikes/2/offset", json!(1_000_001)),
        ("/strikes/1/origin/ordinal", json!(0)),
        ("/strikes/1/origin/ordinal", json!(2)),
        ("/preparation", json!(0)),
        ("/preparation", json!(1_000_001)),
        ("/recovery", json!(1_000_001)),
    ] {
        let mut forged = serde_json::to_value(&shared).unwrap();
        let plan = &mut forged["actors"][snapshot["actors"].as_u64().unwrap() as usize]["1"]
            ["pending"]["plan"];
        *plan.pointer_mut(path).unwrap() = value;
        let Ok(forged) = serde_json::from_value::<SharedState>(forged) else {
            continue;
        };
        assert!(
            crate::Game::restore_checkpoint(
                serde_json::from_value(snapshot.clone()).unwrap(),
                &forged,
                game.content().clone(),
            )
            .is_err(),
            "invalid frozen shape at {path}"
        );
    }
    assert_eq!(game, original, "admission never mutates the source game");
}

#[test]
fn ordered_strikes_checkpoint_keeps_captured_timing_after_a_real_speed_change() {
    use crate::{attributes::Attribute, checkpoint::SharedState, rules::*, ActorId};
    let (mut game, intention) = sequence_game_custom(
        &[0, 100, 200],
        |ability| {
            ability.timing = Timing {
                source: TimingSource::Fixed {
                    prepare: 100,
                    recover: 100,
                },
                scaled_by: Some(Attribute::Speed),
            };
        },
        |rules, _| {
            rules
                .conditions
                .iter_mut()
                .find(|condition| condition.id.as_str() == "braced")
                .unwrap()
                .modifiers = vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Attribute {
                        attribute: Attribute::Speed,
                    },
                    value: 100,
                },
            }];
        },
    );
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    let first = captured.preparation();
    game.tick = first;
    game.resolve_preparations().unwrap();
    game.tick += 1;
    game.pause_preparation(ActorId(1)).unwrap();
    game.apply_condition(
        ActorId(1),
        &crate::content_ids::ConditionId::new("braced").unwrap(),
        ActorId(1),
        10_000,
    )
    .unwrap();
    let fresh = game
        .ability_plan(
            ActorId(1),
            &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    assert_ne!(fresh.preparation(), captured.preparation());
    assert_ne!(
        fresh.as_ability().unwrap().strikes[1].offset,
        captured.as_ability().unwrap().strikes[1].offset
    );
    assert_eq!(game.preparation(ActorId(1)).unwrap().plan, captured);
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let mut shared = SharedState::default();
    let mut restored = crate::Game::restore_checkpoint(
        game.checkpoint(&mut shared),
        &shared,
        game.content().clone(),
    )
    .unwrap();
    assert_eq!(restored, game);
    for candidate in [&mut game, &mut restored] {
        candidate.tick += 10;
        let delay = candidate
            .preparation(ActorId(1))
            .unwrap()
            .completion_delay(candidate.tick)
            .unwrap();
        let expected_ready = candidate.tick + delay;
        let resumed = candidate.plan_work(ActorId(1), captured.work()).unwrap();
        assert_eq!(resumed, captured);
        candidate.start_work(resumed, Some(intention)).unwrap();
        while let Some(preparation) = candidate.preparation(ActorId(1)) {
            candidate.tick = preparation.started + preparation.remaining;
            candidate.resolve_preparations().unwrap();
        }
        assert_eq!(candidate.actors[&ActorId(1)].ready_at, expected_ready);
        assert_eq!(candidate.health(ActorId(2)).unwrap().0, 91);
    }
    assert_eq!(restored, game);
}

#[test]
fn ordered_strikes_frozen_rider_trigger_roundtrips_without_an_implicit_default() {
    for trigger in ["success", "failure"] {
        let value = json!({"trigger":trigger,"check":null,"effects":[],"descriptors":[]});
        let decoded: super::FrozenRider = serde_json::from_value(value.clone())
            .expect("frozen rider has an explicit primary branch trigger");
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        let mut missing = value;
        missing.as_object_mut().unwrap().remove("trigger");
        assert!(serde_json::from_value::<super::FrozenRider>(missing).is_err());
    }
}

#[test]
fn ordered_strikes_rider_trigger_selects_the_declared_branch_for_either_primary_roller() {
    use crate::{attributes::Skill, rules::*, ActorId};
    let mut outcomes = Vec::new();
    for actor in [CheckActor::User, CheckActor::Target] {
        for success in [false, true] {
            for primary_zero in [false, true] {
                let damage = |amount| Effect::Damage {
                    damage: DamageExpr::Components {
                        components: vec![DamageComponentExpr {
                            category: crate::combat::DamageType::Impact,
                            descriptor: None,
                            amount: AmountExpr::Fixed { amount },
                        }],
                    },
                };
                let (mut game, _) = sequence_game_with(&[0], |ability| {
                    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                        unreachable!()
                    };
                    let strike = &mut strikes[0];
                    strike.check = Some(CheckDef {
                        actor,
                        skill: SkillSelector::Named {
                            skill: Skill::Athletics,
                        },
                        against: Against::Difficulty {
                            formula: Formula {
                                base: if success { -1000 } else { 1000 },
                                skill: None,
                                attribute: None,
                            },
                        },
                    });
                    strike.on_success = vec![damage(if primary_zero { 0 } else { 1 })];
                    strike.on_failure = vec![damage(if primary_zero { 0 } else { 3 })];
                    strike.riders = [("success", 5), ("failure", 7)]
                        .into_iter()
                        .map(|(trigger, amount)| {
                            serde_json::from_value(json!({"trigger":trigger,"check":null,
                            "effects":[damage(amount)],"descriptors":[]}))
                            .unwrap()
                        })
                        .collect();
                });
                game.tick = 10;
                game.resolve_preparations().unwrap();
                let primary = if primary_zero {
                    0
                } else if success {
                    1
                } else {
                    3
                };
                let rider = if success { 5 } else { 7 };
                outcomes.push((
                    actor,
                    success,
                    primary_zero,
                    game.health(ActorId(2)).unwrap().0,
                    100 - primary - rider,
                ));
            }
        }
    }
    assert!(
        outcomes
            .iter()
            .all(|(_, _, _, actual, expected)| actual == expected),
        "roller/branch/zero-damage/actual/expected: {outcomes:?}"
    );
}

#[test]
fn ordered_strikes_failure_trigger_respects_whole_immunity_but_not_zero_component_damage() {
    use crate::{
        attributes::Skill, combat::DamageType, content_ids::DescriptorId, rules::*, ActorId,
    };
    for whole_immune in [false, true] {
        let (mut game, _) = sequence_game_custom(
            &[0],
            |ability| {
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                let strike = &mut strikes[0];
                strike.check = Some(CheckDef {
                    actor: CheckActor::Target,
                    skill: SkillSelector::Named {
                        skill: Skill::Athletics,
                    },
                    against: Against::Difficulty {
                        formula: Formula {
                            base: 1000,
                            skill: None,
                            attribute: None,
                        },
                    },
                });
                strike.on_failure = std::mem::take(&mut strike.on_success);
                strike.descriptors = vec![DescriptorId::new("fire").unwrap()];
                strike.riders =
                    vec![
                        serde_json::from_value(json!({"trigger":"failure","check":null,
                "effects":[{"kind":"damage","damage":{"kind":"components","components":[{
                    "category":"keen","descriptor":null,"amount":{"kind":"fixed","amount":2}}]}}],
                "descriptors":[]}))
                        .unwrap(),
                    ];
            },
            |_, species| {
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Immunity {
                        selector: if whole_immune {
                            Selector::Descriptor {
                                descriptor: DescriptorId::new("fire").unwrap(),
                            }
                        } else {
                            Selector::Category {
                                category: DamageType::Impact,
                            }
                        },
                    },
                });
            },
        );
        let before = game.rules_rng;
        game.tick = 10;
        game.resolve_preparations().unwrap();
        assert_eq!(
            game.health(ActorId(2)).unwrap().0,
            if whole_immune { 100 } else { 98 }
        );
        assert_eq!(
            game.rules_rng == before,
            whole_immune,
            "whole immunity skips the primary roll and every branch trigger"
        );
    }
}

#[test]
fn ordered_strikes_frozen_rider_trigger_must_match_the_pinned_authored_branch() {
    use crate::{attributes::Skill, rules::*, ActorId};
    let (game, _) = sequence_game_with(&[0], |ability| {
        let StrikeSource::Authored { strikes } = &mut ability.strikes else {
            unreachable!()
        };
        strikes[0].check = Some(CheckDef {
            actor: CheckActor::Target,
            skill: SkillSelector::Named {
                skill: Skill::Athletics,
            },
            against: Against::Difficulty {
                formula: Formula {
                    base: 10,
                    skill: None,
                    attribute: None,
                },
            },
        });
        strikes[0].riders = vec![
            serde_json::from_value(json!({"trigger":"failure","check":null,
            "effects":[],"descriptors":[]}))
            .unwrap(),
        ];
    });
    let captured = game
        .preparation(ActorId(1))
        .unwrap()
        .plan
        .as_ability()
        .unwrap();
    assert!(captured.valid(game.content().rules()));
    let mut forged = serde_json::to_value(captured).unwrap();
    assert_eq!(forged["strikes"][0]["riders"][0]["trigger"], "failure");
    forged["strikes"][0]["riders"][0]["trigger"] = json!("success");
    let forged: AbilityPlan = serde_json::from_value(forged).unwrap();
    assert!(
        !forged.valid(game.content().rules()),
        "a trigger is frozen authored structure, not a freely captured numeric operand"
    );
}

#[test]
fn ordered_strikes_unchosen_rider_branch_keeps_structural_shared_debit_and_token_binding() {
    use crate::resolution_diagnostics::TraceValueRecord;
    use crate::{attributes::Skill, content_ids::ConditionId, rules::*, ActorId};
    let (mut game, intention) = sequence_game_custom(
        &[0],
        |ability| {
            let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                unreachable!()
            };
            let strike = &mut strikes[0];
            strike.check = Some(CheckDef {
                actor: CheckActor::Target,
                skill: SkillSelector::Named {
                    skill: Skill::Athletics,
                },
                against: Against::Difficulty {
                    formula: Formula {
                        base: -1000,
                        skill: None,
                        attribute: None,
                    },
                },
            });
            strike.on_success = vec![Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: crate::combat::DamageType::Impact,
                        descriptor: None,
                        amount: AmountExpr::Rolled {
                            dice: DiceExpr {
                                count: 3,
                                sides: 6,
                                bonus: 0,
                            },
                        },
                    }],
                },
            }];
            strike.riders = [RiderTrigger::Failure, RiderTrigger::Success]
                .into_iter()
                .map(|trigger| RiderDef {
                    trigger,
                    check: Some(RiderCheckDef {
                        skill: SkillSelector::NaturalAttack,
                        defense: DefenseKind::Physical,
                    }),
                    effects: vec![],
                    descriptors: vec![],
                })
                .collect();
        },
        |_, species| {
            species.modifiers.extend([
                Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::Edge,
                        value: 5,
                    },
                },
                Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Add {
                        stat: Stat::CheckEdge,
                        value: -6,
                    },
                },
            ]);
        },
    );
    game.cancel_intention(ActorId(1), intention).unwrap();
    game.apply_condition(
        ActorId(1),
        &ConditionId::new("readied").unwrap(),
        ActorId(1),
        100,
    )
    .unwrap();
    let plan = game
        .ability_plan(
            ActorId(1),
            &crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    let captured = plan.as_ability().unwrap();
    assert_eq!(captured.strikes[0].edge, 5);
    assert_eq!(
        captured.strikes[0].shared_check,
        Some(super::CheckPhase::Rider { ordinal: 0 })
    );
    assert_eq!(captured.tokens.len(), 1);
    assert_eq!(
        captured.tokens[0].site.check,
        super::CheckPhase::Rider { ordinal: 0 }
    );
    assert_eq!(
        captured.strikes[0].riders[0].check.as_ref().unwrap().edge,
        0
    );
    assert_eq!(
        captured.strikes[0].riders[1].check.as_ref().unwrap().edge,
        -6
    );
    let encoded = serde_json::to_value(&plan).unwrap();
    game.start_work(plan.clone(), None).unwrap();
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .conditions()
        .qualifying_tokens(game.content().rules(), ActorId(1), false)
        .is_empty());
    game.set_combat_diagnostics(true);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let record = game.combat_diagnostics().unwrap().records().back().unwrap();
    let checks: Vec<_> = record
        .trace
        .steps()
        .iter()
        .filter(|step| step.id == "check")
        .collect();
    assert_eq!(
        checks.len(),
        2,
        "primary and matching rider only; the bound failure rider never rolls"
    );
    let component = record
        .trace
        .steps()
        .iter()
        .find(|step| step.id == "damage.component")
        .unwrap();
    assert_eq!(
        component
            .fields
            .iter()
            .find(|field| field.id == "edge")
            .unwrap()
            .value,
        TraceValueRecord::Signed(2),
        "unchosen branch cannot refund its reserved shared debit into primary damage"
    );
    assert_eq!(serde_json::to_value(&plan).unwrap(), encoded);
    assert!(game
        .creature(ActorId(1))
        .unwrap()
        .conditions()
        .qualifying_tokens(game.content().rules(), ActorId(1), false)
        .is_empty());
}

#[test]
fn ordered_strikes_natural_damage_modifiers_capture_each_primary_branch_without_rider_leakage() {
    use crate::{
        attributes::Skill, combat::DamageType, damage::DamageAmount, dice::DicePool, rules::*,
        ActorId,
    };
    let mut outcomes = vec![];
    for natural in [true, false] {
        for rolled_primary in [false, true] {
            let rolled = || AmountExpr::Rolled {
                dice: DiceExpr {
                    count: 1,
                    sides: 6,
                    bonus: 0,
                },
            };
            let primary = Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![
                        DamageComponentExpr {
                            category: DamageType::Impact,
                            descriptor: None,
                            amount: if rolled_primary {
                                rolled()
                            } else {
                                AmountExpr::Fixed { amount: 3 }
                            },
                        },
                        DamageComponentExpr {
                            category: DamageType::Keen,
                            descriptor: None,
                            amount: rolled(),
                        },
                    ],
                },
            };
            let other = Effect::Damage {
                damage: DamageExpr::Components {
                    components: vec![DamageComponentExpr {
                        category: DamageType::Vital,
                        descriptor: None,
                        amount: rolled(),
                    }],
                },
            };
            let strike = StrikeDef {
                offset: 0,
                unlock: None,
                check: Some(CheckDef {
                    actor: CheckActor::Target,
                    skill: SkillSelector::Named {
                        skill: Skill::Athletics,
                    },
                    against: Against::Difficulty {
                        formula: Formula {
                            base: 1000,
                            skill: None,
                            attribute: None,
                        },
                    },
                }),
                on_success: vec![primary.clone(), other.clone()],
                on_failure: vec![primary, other.clone()],
                descriptors: vec![],
                riders: vec![RiderDef {
                    trigger: RiderTrigger::Failure,
                    check: None,
                    effects: vec![other],
                    descriptors: vec![],
                }],
            };
            let (mut game, _) = sequence_game_custom(
                &[0],
                |ability| {
                    ability.strikes = if natural {
                        StrikeSource::SelectedMelee
                    } else {
                        StrikeSource::Authored {
                            strikes: vec![strike.clone()],
                        }
                    };
                },
                |rules, species| {
                    if natural {
                        species.melee = crate::attacks::NaturalAttack::new(
                            Skill::HeavyWeaponry,
                            0,
                            10,
                            20,
                            vec![strike.clone()],
                        )
                        .unwrap();
                    }
                    let ability = rules
                        .abilities
                        .iter()
                        .find(|ability| ability.basic_attack)
                        .unwrap()
                        .id
                        .clone();
                    species.modifiers.extend(
                        [(AbilityStat::Dice, 2), (AbilityStat::FlatBonus, 5)]
                            .into_iter()
                            .map(|(stat, value)| Modifier {
                                scope: Scope::Always,
                                op: ModifierOp::Add {
                                    stat: Stat::Ability {
                                        ability: ability.clone(),
                                        stat,
                                    },
                                    value,
                                },
                            }),
                    );
                },
            );
            let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
            assert!(game.plan_source_valid(&captured));
            let frozen = &captured.as_ability().unwrap().strikes[0];
            let amounts = |effects: &[super::PlannedEffect]| {
                effects
                    .iter()
                    .map(|effect| {
                        let super::PlannedEffect::Damage { damage } = effect else {
                            panic!("damage fixture")
                        };
                        damage
                            .components()
                            .iter()
                            .map(|component| component.amount())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            let augmented = DamageAmount::Rolled(DicePool::new(3, 6, 5).unwrap());
            let plain = DamageAmount::Rolled(DicePool::new(1, 6, 0).unwrap());
            let expected_primary = if rolled_primary {
                augmented
            } else {
                DamageAmount::Fixed(if natural { 8 } else { 3 })
            };
            let other = if natural { plain } else { augmented };
            let expected_branch = vec![vec![expected_primary, other], vec![other]];
            outcomes.push((
                natural,
                rolled_primary,
                "success",
                amounts(&frozen.on_success),
                expected_branch.clone(),
            ));
            outcomes.push((
                natural,
                rolled_primary,
                "failure",
                amounts(&frozen.on_failure),
                expected_branch,
            ));
            outcomes.push((
                natural,
                rolled_primary,
                "rider",
                amounts(&frozen.riders[0].effects),
                vec![vec![other]],
            ));
            let decoded: super::Plan =
                serde_json::from_value(serde_json::to_value(&captured).unwrap()).unwrap();
            assert_eq!(decoded, captured);
            assert!(game.plan_source_valid(&decoded));
            game.tick = 10;
            game.resolve_preparations().unwrap();
            assert!(game.combat_events().iter().any(|event| matches!(event,
                crate::combat::CombatEvent::Resolved { outcome, strike: Some(0) }
                    if outcome.check.as_ref().is_some_and(|check| check.actor == CheckActor::Target && !check.success)
                        && outcome.effects.len() == 2)));
        }
    }
    assert!(
        outcomes
            .iter()
            .all(|(_, _, _, actual, expected)| actual == expected),
        "natural/rolled/branch/actual/expected: {outcomes:?}"
    );
}
#[test]
fn ordered_strikes_owned_unlock_drain_restore_affects_new_capture_not_frozen_work() {
    use crate::{
        checkpoint::SharedState,
        content_ids::TalentId,
        creatures::{BuildAdvancement, CreatureBuild, PlannedAdvancement, RestorationPolicy},
        progression::HdSource,
        rules::*,
        ActorId,
    };
    for natural in [false, true] {
        let unlock = TalentId::new("hardiness").unwrap();
        let rows = [0, 10, 20]
            .into_iter()
            .enumerate()
            .map(|(ordinal, offset)| StrikeDef {
                offset,
                unlock: (ordinal == 1).then(|| unlock.clone()),
                check: None,
                on_success: vec![Effect::Damage {
                    damage: DamageExpr::Components {
                        components: vec![DamageComponentExpr {
                            category: crate::combat::DamageType::Impact,
                            descriptor: None,
                            amount: AmountExpr::Fixed { amount: 3 },
                        }],
                    },
                }],
                on_failure: vec![],
                descriptors: vec![],
                riders: vec![],
            })
            .collect::<Vec<_>>();
        let (mut game, original) = sequence_game_custom(
            &[0, 10, 20],
            |ability| {
                ability.strikes = if natural {
                    StrikeSource::SelectedMelee
                } else {
                    StrikeSource::Authored {
                        strikes: rows.clone(),
                    }
                };
            },
            |document, species| {
                let talent = document
                    .talents
                    .iter_mut()
                    .find(|talent| talent.id == unlock)
                    .unwrap();
                talent.requirement = Requirement::TotalHd { minimum: 2 };
                talent.prerequisites.clear();
                talent.modifiers.clear();
                if natural {
                    species.melee = crate::attacks::NaturalAttack::new(
                        crate::attributes::Skill::HeavyWeaponry,
                        0,
                        10,
                        20,
                        rows.clone(),
                    )
                    .unwrap();
                }
            },
        );
        game.cancel_intention(ActorId(1), original).unwrap();
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        build.add_hit_die(HdSource::Racial, 42).unwrap();
        build.select_talent(0, unlock.clone()).unwrap();
        let restoration = RestorationPolicy::Authored(
            build
                .ledger()
                .entries()
                .iter()
                .zip(build.choices())
                .map(|(die, choices)| PlannedAdvancement {
                    source: die.source(),
                    choices: choices.clone(),
                })
                .collect(),
        );
        let mut build = CreatureBuild::from_recorded(
            build.content().clone(),
            build.species_id().clone(),
            build.initial_attributes(),
            build.ledger().clone(),
            build.binding(),
            BuildAdvancement {
                restoration,
                ..build.advancement().clone()
            },
            build.template_ids().to_vec(),
        )
        .unwrap();
        game.rebuild_creature(ActorId(1), build.clone()).unwrap();
        assert!(game
            .creature(ActorId(1))
            .unwrap()
            .derived()
            .active_talents
            .contains(&unlock));
        let ability = game.basic_attack(ActorId(1)).unwrap();
        let capture =
            |game: &crate::Game| game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
        let ordinals = |plan: &super::Plan| {
            plan.as_ability()
                .unwrap()
                .strikes
                .iter()
                .map(|strike| strike.origin.ordinal())
                .collect::<Vec<_>>()
        };
        let frozen = capture(&game);
        assert_eq!(ordinals(&frozen), [0, 1, 2]);
        game.start_work(frozen.clone(), None).unwrap();
        game.tick = 10;
        game.resolve_preparations().unwrap();
        game.tick = 15;
        game.pause_preparation(ActorId(1)).unwrap();
        build.remove_latest().unwrap();
        assert!(
            build.choices()[0].talents().any(|talent| talent == &unlock),
            "the talent stays owned on the retained HD"
        );
        game.rebuild_creature(ActorId(1), build.clone()).unwrap();
        let creature = game.creature(ActorId(1)).unwrap();
        assert!(creature.derived().dormant_talents.contains(&unlock));
        assert!(
            creature.derived().abilities.contains(&ability),
            "the entire ability remains granted"
        );
        assert_eq!(
            ordinals(&capture(&game)),
            [0, 2],
            "new capture omits dormant ordinal without renumbering source provenance"
        );
        assert_eq!(game.preparation(ActorId(1)).unwrap().plan, frozen);
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut reopened =
            crate::Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(reopened, game);
        for world in [&mut game, &mut reopened] {
            world.tick = 25;
            world.start_work(frozen.clone(), None).unwrap();
            for tick in [30, 40] {
                world.tick = tick;
                world.resolve_preparations().unwrap();
            }
            assert_eq!(
                world.health(ActorId(2)).unwrap().0,
                91,
                "captured unlocked strike still resolves while talent is dormant"
            );
            assert!(world.preparation(ActorId(1)).is_none());
        }
        assert_eq!(reopened, game);
        build.restore_hit_die(42).unwrap();
        game.rebuild_creature(ActorId(1), build).unwrap();
        assert!(game
            .creature(ActorId(1))
            .unwrap()
            .derived()
            .active_talents
            .contains(&unlock));
        assert_eq!(
            ordinals(&capture(&game)),
            [0, 1, 2],
            "restoration re-enables later preparations"
        );
    }
}
#[test]
fn ordered_strikes_selected_damage_uses_only_first_natural_success_primary() {
    use crate::{attacks::SelectedDamageError, rules::*, ActorId};
    let damage = |amount| Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![DamageComponentExpr {
                category: crate::combat::DamageType::Impact,
                descriptor: None,
                amount: AmountExpr::Fixed { amount },
            }],
        },
    };
    for first_success_damage in [false, true] {
        let (game, _) = sequence_game_custom(
            &[0],
            |_| {},
            |_, species| {
                let mut success = vec![Effect::Heal {
                    amount: AmountExpr::Fixed { amount: 1 },
                }];
                if first_success_damage {
                    success.extend([damage(3), damage(4)]);
                }
                let first = StrikeDef {
                    offset: 0,
                    unlock: None,
                    check: Some(CheckDef {
                        actor: CheckActor::Target,
                        skill: SkillSelector::Named {
                            skill: crate::attributes::Skill::Athletics,
                        },
                        against: Against::Difficulty {
                            formula: Formula {
                                base: 1000,
                                skill: None,
                                attribute: None,
                            },
                        },
                    }),
                    on_success: success,
                    on_failure: vec![damage(30)],
                    descriptors: vec![],
                    riders: vec![RiderDef {
                        trigger: RiderTrigger::Failure,
                        check: None,
                        effects: vec![damage(40)],
                        descriptors: vec![],
                    }],
                };
                let mut later = first.clone();
                later.offset = 10;
                later.on_success = vec![damage(50)];
                later.on_failure.clear();
                later.riders.clear();
                species.melee = crate::attacks::NaturalAttack::new(
                    crate::attributes::Skill::HeavyWeaponry,
                    0,
                    10,
                    20,
                    vec![first, later],
                )
                .unwrap();
            },
        );
        let selected = game.selected_melee_attack(ActorId(1)).unwrap();
        let captured = game.compile_effects(
            ActorId(1),
            &[Effect::Damage {
                damage: DamageExpr::SelectedWeapon { impact_bonus: 0 },
            }],
            &crate::creatures::AbilityValues::default(),
        );
        if first_success_damage {
            let expected = crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    crate::combat::DamageType::Impact,
                    None,
                    3,
                )],
                Some(
                    crate::damage::DamageComponent::fixed(
                        crate::combat::DamageType::Impact,
                        None,
                        3,
                    )
                    .key(),
                ),
            )
            .unwrap();
            assert_eq!(selected.first_primary_damage().unwrap(), expected);
            assert_eq!(
                captured.unwrap(),
                vec![super::PlannedEffect::Damage { damage: expected }]
            );
        } else {
            assert_eq!(
                selected.first_primary_damage(),
                Err(SelectedDamageError::NoPrimaryDamage)
            );
            assert!(
                captured.is_err(),
                "no fallback to failure, rider or later strike damage"
            );
        }
    }
}
