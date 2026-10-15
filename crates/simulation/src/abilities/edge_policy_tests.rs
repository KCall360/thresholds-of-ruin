//! Owner-outgoing source selection uses the ordinary frozen strike pipeline.
use super::strike_tests::sequence_game_custom;
use super::{CheckPhase, CheckSide, CheckThreshold};
use crate::content_ids::{AbilityId, ConditionId};
use crate::rules::*;
use crate::{ActorId, Game};
use serde_json::json;

fn policy(ability: &mut AbilityDef, kind: &str) {
    let mut value = serde_json::to_value(&ability).unwrap();
    value["outgoing_edge_policy"] = json!({"kind": kind});
    *ability = serde_json::from_value(value).expect("typed outgoing policy must decode");
}

fn add(stat: Stat, value: i64) -> Modifier {
    Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add { stat, value },
    }
}

fn primary(ability: &mut AbilityDef, actor: CheckActor, against: Against) {
    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
        unreachable!()
    };
    for strike in strikes {
        strike.check = Some(CheckDef {
            actor,
            skill: SkillSelector::NaturalAttack,
            against: against.clone(),
        });
    }
}

fn plan(game: &Game) -> super::Plan {
    game.ability_plan(
        ActorId(1),
        &AbilityId::new("basic_melee").unwrap(),
        ActorId(2),
    )
    .unwrap()
}

#[test]
fn outgoing_edge_policy_filters_before_independent_check_and_shared_budgets() {
    for (kind, check, shared, rider) in [("ordinary", -4, 3, -7), ("ignore_negative", 4, 5, -1)] {
        let (game, _) = sequence_game_custom(
            &[0, 10],
            |ability| {
                policy(ability, kind);
                primary(
                    ability,
                    CheckActor::User,
                    Against::Defense {
                        defense: DefenseKind::Physical,
                    },
                );
                let StrikeSource::Authored { strikes } = &mut ability.strikes else {
                    unreachable!()
                };
                for strike in strikes {
                    strike.riders.push(RiderDef {
                        trigger: RiderTrigger::Success,
                        check: Some(RiderCheckDef {
                            skill: SkillSelector::NaturalAttack,
                            defense: DefenseKind::Physical,
                        }),
                        effects: vec![],
                        descriptors: vec![],
                    });
                }
            },
            |_, species| {
                species.modifiers.extend([
                    add(Stat::Edge, 5),
                    add(Stat::Edge, -2),
                    add(Stat::CheckEdge, -6),
                    add(Stat::IncomingEdge { defense: None }, -1),
                ])
            },
        );
        let captured = plan(&game);
        for strike in &captured.as_ability().unwrap().strikes {
            assert_eq!(strike.check.as_ref().unwrap().edge, check);
            assert_eq!(strike.edge, shared);
            assert_eq!(strike.riders[0].check.as_ref().unwrap().edge, rider);
            assert_eq!(
                strike.shared_check,
                Some(CheckPhase::Primary {
                    side: CheckSide::Initiating
                })
            );
        }
    }
}

#[test]
fn outgoing_edge_policy_applies_only_to_the_owner_even_on_the_opposing_side() {
    for actor in [CheckActor::User, CheckActor::Target] {
        let (game, _) = sequence_game_custom(
            &[0],
            |ability| {
                policy(ability, "ignore_negative");
                primary(
                    ability,
                    actor,
                    Against::Opposed {
                        skill: SkillSelector::NaturalAttack,
                    },
                );
            },
            |_, species| {
                species.modifiers.extend([
                    add(Stat::CheckEdge, -4),
                    add(Stat::IncomingEdge { defense: None }, -2),
                ])
            },
        );
        let captured = plan(&game);
        let check = captured.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap();
        let CheckThreshold::Opposed { edge, .. } = check.threshold else {
            panic!("opposed check")
        };
        assert_eq!(
            (check.edge, edge),
            if actor == CheckActor::User {
                (-2, -6)
            } else {
                (-6, -2)
            }
        );
    }
}

#[test]
fn outgoing_edge_policy_does_not_claim_ignored_negative_tokens() {
    let (mut game, _) = sequence_game_custom(
        &[0, 10],
        |ability| {
            policy(ability, "ignore_negative");
            primary(
                ability,
                CheckActor::User,
                Against::Defense {
                    defense: DefenseKind::Physical,
                },
            );
        },
        |_, _| {},
    );
    for (holder, name) in [
        (1, "distracted"),
        (1, "readied"),
        (2, "exposed"),
        (2, "braced"),
    ] {
        game.apply_condition(
            ActorId(holder),
            &ConditionId::new(name).unwrap(),
            ActorId(2),
            500,
        )
        .unwrap();
    }
    let captured = plan(&game);
    let ability = captured.as_ability().unwrap();
    assert_eq!(
        ability
            .tokens
            .iter()
            .map(|claim| claim.reservation.condition.as_str())
            .collect::<Vec<_>>(),
        ["readied", "exposed"]
    );
    assert_eq!(ability.strikes[0].check.as_ref().unwrap().edge, 1);
    assert_eq!(ability.strikes[1].check.as_ref().unwrap().edge, -1);
    assert_eq!(
        ability.strikes[0].edge, 0,
        "tokens never become damage edge"
    );
    assert!(ability.valid(game.content().rules()));
    let mut forged = serde_json::to_value(ability).unwrap();
    forged["tokens"][0]["reservation"]["condition"] = json!("distracted");
    let forged: super::AbilityPlan = serde_json::from_value(forged).unwrap();
    assert!(
        !forged.valid(game.content().rules()),
        "pinned policy rejects an inapplicable claim"
    );
}

#[test]
fn outgoing_edge_policy_consumes_a_mixed_sign_token_for_its_remaining_positive_source() {
    let (mut game, _) = sequence_game_custom(
        &[0],
        |ability| {
            policy(ability, "ignore_negative");
            primary(
                ability,
                CheckActor::User,
                Against::Defense {
                    defense: DefenseKind::Physical,
                },
            );
        },
        |rules, _| {
            rules
                .conditions
                .iter_mut()
                .find(|condition| condition.id.as_str() == "readied")
                .unwrap()
                .modifiers
                .push(add(Stat::CheckEdge, -1));
        },
    );
    game.apply_condition(
        ActorId(1),
        &ConditionId::new("readied").unwrap(),
        ActorId(1),
        500,
    )
    .unwrap();
    let captured = plan(&game);
    assert_eq!(captured.as_ability().unwrap().tokens.len(), 1);
    assert_eq!(
        captured.as_ability().unwrap().strikes[0]
            .check
            .as_ref()
            .unwrap()
            .edge,
        1
    );
}

#[test]
fn outgoing_edge_policy_filters_negative_sets_but_keeps_zero_and_positive_overrides() {
    for (set, expected) in [(-2, 5), (0, 0), (2, 2)] {
        let (game, _) = sequence_game_custom(
            &[0],
            |ability| {
                policy(ability, "ignore_negative");
                primary(
                    ability,
                    CheckActor::User,
                    Against::Defense {
                        defense: DefenseKind::Physical,
                    },
                );
            },
            |_, species| {
                species.modifiers.extend([
                    add(Stat::Edge, 5),
                    Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Set {
                            stat: Stat::Edge,
                            value: set,
                        },
                    },
                ])
            },
        );
        let captured = plan(&game);
        assert_eq!(captured.as_ability().unwrap().strikes[0].edge, expected);
        assert_eq!(
            captured.as_ability().unwrap().strikes[0]
                .check
                .as_ref()
                .unwrap()
                .edge,
            expected
        );
    }
}

#[test]
fn outgoing_edge_policy_selects_implied_ability_and_tag_sources_before_folding() {
    for (kind, expected) in [("ordinary", -9), ("ignore_negative", 0)] {
        let (game, _) = sequence_game_custom(
            &[0],
            |ability| {
                policy(ability, kind);
                primary(
                    ability,
                    CheckActor::User,
                    Against::Defense {
                        defense: DefenseKind::Physical,
                    },
                );
            },
            |rules, species| {
                let flag = crate::content_ids::FlagId::new("awkward_attack").unwrap();
                rules.flags.push(FlagDef {
                    id: flag.clone(),
                    name: "Awkward attack".into(),
                    modifiers: vec![
                        add(Stat::CheckEdge, -2),
                        add(
                            Stat::Ability {
                                ability: AbilityId::new("basic_melee").unwrap(),
                                stat: AbilityStat::CheckEdge,
                            },
                            -3,
                        ),
                        add(
                            Stat::AbilityTag {
                                tag: crate::content_ids::AbilityTagId::new("weapon_attack")
                                    .unwrap(),
                                stat: AbilityStat::CheckEdge,
                            },
                            -4,
                        ),
                    ],
                });
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Flag { flag },
                });
            },
        );
        assert_eq!(
            plan(&game).as_ability().unwrap().strikes[0]
                .check
                .as_ref()
                .unwrap()
                .edge,
            expected
        );
    }
}

#[test]
fn outgoing_edge_policy_preserves_target_resistance_and_unspent_owner_shared_edge() {
    let (game, _) = sequence_game_custom(
        &[0],
        |ability| {
            policy(ability, "ignore_negative");
            primary(
                ability,
                CheckActor::Target,
                Against::Difficulty {
                    formula: Formula {
                        base: 10,
                        skill: None,
                        attribute: None,
                    },
                },
            );
        },
        |_, species| {
            species.modifiers.extend([
                add(Stat::Edge, 5),
                add(Stat::Edge, -2),
                add(Stat::CheckEdge, -6),
            ])
        },
    );
    let captured = plan(&game);
    let strike = &captured.as_ability().unwrap().strikes[0];
    assert_eq!(strike.check.as_ref().unwrap().edge, -3);
    assert_eq!(strike.shared_check, None);
    assert_eq!(strike.edge, 5);
}

#[test]
fn outgoing_edge_policy_keeps_other_rollers_negative_tokens_qualifying() {
    for actor in [CheckActor::User, CheckActor::Target] {
        let (mut game, _) = sequence_game_custom(
            &[0, 10],
            |ability| {
                policy(ability, "ignore_negative");
                primary(
                    ability,
                    actor,
                    Against::Opposed {
                        skill: SkillSelector::NaturalAttack,
                    },
                );
            },
            |_, _| {},
        );
        for holder in [1, 2] {
            game.apply_condition(
                ActorId(holder),
                &ConditionId::new("distracted").unwrap(),
                ActorId(holder),
                500,
            )
            .unwrap();
        }
        let captured = plan(&game);
        let ability = captured.as_ability().unwrap();
        assert_eq!(ability.tokens.len(), 1);
        assert_eq!(ability.tokens[0].reservation.holder, ActorId(2));
        assert_eq!(
            ability.tokens[0].site,
            super::CheckSite {
                strike: 0,
                check: CheckPhase::Primary {
                    side: if actor == CheckActor::Target {
                        CheckSide::Initiating
                    } else {
                        CheckSide::Opposing
                    },
                },
            }
        );
        for (index, strike) in ability.strikes.iter().enumerate() {
            let check = strike.check.as_ref().unwrap();
            let CheckThreshold::Opposed { edge, .. } = check.threshold else {
                panic!("opposed check")
            };
            let expected = match (index, actor) {
                (0, CheckActor::User) => (0, -1),
                (0, CheckActor::Target) => (-1, 0),
                _ => (0, 0),
            };
            assert_eq!((check.edge, edge), expected);
        }
        assert!(ability.valid(game.content().rules()));
    }
}

#[test]
fn outgoing_edge_policy_freezes_through_checkpoint_pause_and_later_conditions() {
    let (mut game, intention) = sequence_game_custom(
        &[0, 10],
        |ability| {
            policy(ability, "ignore_negative");
            primary(
                ability,
                CheckActor::User,
                Against::Difficulty {
                    formula: Formula {
                        base: -1000,
                        skill: None,
                        attribute: None,
                    },
                },
            );
        },
        |_, species| {
            species.modifiers.extend([
                add(Stat::CheckEdge, -6),
                add(Stat::Edge, 5),
                add(Stat::Edge, -2),
            ])
        },
    );
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    game.pause_preparation(ActorId(1)).unwrap();
    game.apply_condition(
        ActorId(1),
        &ConditionId::new("readied").unwrap(),
        ActorId(1),
        500,
    )
    .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    for current in [&mut game, &mut restored] {
        let resumed = current.plan_work(ActorId(1), captured.work()).unwrap();
        assert_eq!(resumed, captured);
        current.start_work(resumed, Some(intention)).unwrap();
        current.tick = 20;
        current.resolve_preparations().unwrap();
        assert_eq!(
            current
                .creature(ActorId(1))
                .unwrap()
                .conditions()
                .qualifying_tokens(current.content().rules(), ActorId(1), false)
                .len(),
            1
        );
    }
    assert_eq!(restored, game);
}

#[test]
fn outgoing_edge_policy_has_a_strict_named_authoring_shape() {
    let rules = crate::test_creatures::ruleset();
    let original = serde_json::to_value(&rules.document().abilities[0]).unwrap();
    let mut omitted = original.clone();
    omitted
        .as_object_mut()
        .unwrap()
        .remove("outgoing_edge_policy");
    let decoded: AbilityDef = serde_json::from_value(omitted).unwrap();
    assert_eq!(
        serde_json::to_value(decoded).unwrap()["outgoing_edge_policy"],
        json!({"kind":"ordinary"})
    );
    for invalid in [
        json!(null),
        json!("ordinary"),
        json!(["ordinary"]),
        json!({}),
        json!({"kind":"unknown"}),
        json!({"kind":"ordinary","extra":true}),
        json!({"kind":"ignore_negative","extra":true}),
    ] {
        let mut value = original.clone();
        value["outgoing_edge_policy"] = invalid;
        assert!(serde_json::from_value::<AbilityDef>(value).is_err());
    }
}

#[test]
fn outgoing_edge_policy_distinguishes_self_target_roll_roles_and_claims() {
    for actor in [CheckActor::User, CheckActor::Target] {
        for opposed in [false, true] {
            let ability_id = AbilityId::new("self_probe").unwrap();
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
                    ability.reach = Reach::SelfOnly;
                    policy(&mut ability, "ignore_negative");
                    primary(
                        &mut ability,
                        actor,
                        if opposed {
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
                    );
                    rules.abilities.push(ability);
                    species.modifiers.extend([
                        add(Stat::CheckEdge, -4),
                        add(Stat::IncomingEdge { defense: None }, -2),
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
            let captured = game
                .ability_plan(ActorId(1), &ability_id, ActorId(1))
                .unwrap();
            let ability = captured.as_ability().unwrap();
            let check = ability.strikes[0].check.as_ref().unwrap();
            assert_eq!(check.edge, if actor == CheckActor::User { -2 } else { -7 });
            if opposed {
                let CheckThreshold::Opposed { edge, .. } = check.threshold else {
                    panic!("opposed")
                };
                assert_eq!(edge, if actor == CheckActor::User { -7 } else { -2 });
            }
            assert_eq!(
                ability.tokens.len(),
                usize::from(opposed || actor == CheckActor::Target)
            );
            assert!(
                ability.valid(game.content().rules()),
                "self-target token binds to its first eligible roll role"
            );
        }
    }
}

fn self_shared_case(kind: &str, opposed: bool) {
    use crate::resolution_diagnostics::{ResolutionTrace, TraceValueRecord};
    let ability_id = AbilityId::new("self_shared_probe").unwrap();
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
            ability.reach = Reach::SelfOrAlly {
                range: AllyRange::Touch,
            };
            policy(&mut ability, kind);
            primary(
                &mut ability,
                if opposed {
                    CheckActor::User
                } else {
                    CheckActor::Target
                },
                if opposed {
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
            );
            rules.abilities.push(ability);
            species.modifiers.extend([
                add(Stat::Edge, 5),
                add(Stat::CheckEdge, -6),
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
    for target in [ActorId(2), ActorId(1)] {
        let captured = game.ability_plan(ActorId(1), &ability_id, target).unwrap();
        let ability = captured.as_ability().unwrap();
        let strike = &ability.strikes[0];
        let check = strike.check.as_ref().unwrap();
        let self_target = target == ActorId(1);
        let filtered = kind == "ignore_negative";
        let expected_check = if opposed {
            if filtered {
                5
            } else {
                -2
            }
        } else if self_target {
            -2
        } else {
            -1
        };
        assert_eq!(
            check.edge, expected_check,
            "{kind}, opposed={opposed}, target={target:?}"
        );
        if opposed {
            let CheckThreshold::Opposed { edge, .. } = check.threshold else {
                panic!("opposed")
            };
            assert_eq!(
                edge,
                if filtered && self_target { -2 } else { -1 },
                "the target role keeps its ordinary shared sources"
            );
        }
        assert_eq!(strike.edge, 5);
        assert_eq!(strike.shared_check.is_some(), opposed);
        assert_eq!(
            ability.tokens.len(),
            usize::from((opposed && !filtered) || self_target)
        );
        assert!(ability.valid(game.content().rules()));
        let mut trace = ResolutionTrace::default();
        super::resolve(
            captured.resolution().unwrap(),
            &super::Snapshot::capture(&game.creature(target).unwrap()),
            &mut 42,
            &mut trace,
        )
        .unwrap();
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
fn outgoing_edge_policy_self_shared_fixed_ordinary_keeps_resistance_budget() {
    self_shared_case("ordinary", false);
}
#[test]
fn outgoing_edge_policy_self_shared_fixed_filtered_keeps_resistance_budget() {
    self_shared_case("ignore_negative", false);
}
#[test]
fn outgoing_edge_policy_self_shared_opposed_ordinary_debits_only_owner() {
    self_shared_case("ordinary", true);
}
#[test]
fn outgoing_edge_policy_self_shared_opposed_filtered_debits_only_owner() {
    self_shared_case("ignore_negative", true);
}

#[test]
fn outgoing_edge_policy_consumes_selected_tokens_on_miss_or_cancel_and_preserves_ignored_tokens() {
    for cancelled in [false, true] {
        let (mut game, intention) = sequence_game_custom(
            &[0],
            |ability| {
                policy(ability, "ignore_negative");
                primary(
                    ability,
                    CheckActor::User,
                    Against::Difficulty {
                        formula: Formula {
                            base: 1000,
                            skill: None,
                            attribute: None,
                        },
                    },
                );
            },
            |_, _| {},
        );
        for token in ["distracted", "readied"] {
            game.apply_condition(
                ActorId(1),
                &ConditionId::new(token).unwrap(),
                ActorId(1),
                500,
            )
            .unwrap();
        }
        let captured = plan(&game);
        let before_rng = game.rules_rng;
        game.start_work(captured, Some(intention)).unwrap();
        if cancelled {
            game.cancel_intention(ActorId(1), intention).unwrap();
            assert_eq!(game.rules_rng, before_rng);
        } else {
            game.tick = 10;
            game.resolve_preparations().unwrap();
            assert_ne!(game.rules_rng, before_rng);
        }
        let creature = game.creature(ActorId(1)).unwrap();
        let remaining: Vec<_> = creature.conditions().instances().collect();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].condition.as_str(), "distracted");
        assert!(remaining[0].reserved_by.is_none());
        assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
    }
}
