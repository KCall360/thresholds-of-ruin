//! Qualification and attempt ownership use source-authored, admitted sequences.
use crate::{
    combat::{CombatEvent, DamageType, PreparationProgress},
    content_ids::DescriptorId,
    rules::*,
    ActorId,
};

fn damage(category: DamageType, amount: u32) -> Effect {
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

fn rider(minimum: Option<u16>, amount: u32) -> RiderDef {
    RiderDef {
        trigger: RiderTrigger::Success,
        condition: minimum.map(|minimum| RiderCondition::SequenceSuccessfulHits { minimum }),
        check: None,
        effects: vec![damage(DamageType::Keen, amount)],
        descriptors: vec![],
    }
}

fn strike(index: usize, hit: bool) -> StrikeDef {
    StrikeDef {
        offset: index as u64 * 10,
        unlock: None,
        check: Some(CheckDef {
            actor: CheckActor::User,
            skill: SkillSelector::SelectedWeapon,
            against: Against::Defense {
                defense: if hit {
                    DefenseKind::Physical
                } else {
                    DefenseKind::Cognitive
                },
            },
        }),
        on_success: vec![damage(DamageType::Impact, 3)],
        on_failure: vec![],
        descriptors: vec![],
        riders: vec![],
    }
}

fn natural_game(
    strikes: Vec<StrikeDef>,
    additional_riders: Vec<RiderDef>,
    configure: impl FnOnce(&mut RulesetDocument, &mut crate::creatures::Species),
) -> crate::Game {
    let offsets = strikes
        .iter()
        .map(|strike| strike.offset)
        .collect::<Vec<_>>();
    let (game, _) = super::strike_tests::sequence_game_custom(
        &offsets,
        |ability| ability.strikes = StrikeSource::NaturalMelee { additional_riders },
        |rules, species| {
            // SelectedWeapon under NaturalMelee captures this +100 bonus. The
            // Cognitive threshold makes the alternate initiating check miss.
            species.melee = crate::attacks::NaturalAttack::new(
                crate::attributes::Skill::HeavyWeaponry,
                100,
                10,
                20,
                strikes,
            )
            .unwrap();
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::Defense {
                        defense: DefenseKind::Cognitive,
                    },
                    value: 1000,
                },
            });
            configure(rules, species);
        },
    );
    game
}

fn resolve(game: &mut crate::Game, index: usize) {
    game.tick = (index as u64 + 1) * 10;
    game.resolve_preparations().unwrap();
}

fn primary(game: &crate::Game, index: usize) -> &super::Outcome {
    game.combat_events()
        .iter()
        .find_map(|event| match event {
            CombatEvent::Resolved {
                outcome,
                strike: Some(ordinal),
            } if outcome.actor == ActorId(1) && usize::from(*ordinal) == index => Some(outcome),
            _ => None,
        })
        .unwrap()
}

fn riders(game: &crate::Game) -> Vec<&super::Outcome> {
    game.combat_events()
        .iter()
        .filter_map(|event| match event {
            CombatEvent::Resolved {
                outcome,
                strike: None,
            } if outcome.actor == ActorId(1) && outcome.ability.is_none() => Some(outcome),
            _ => None,
        })
        .collect()
}

fn assert_progress(game: &crate::Game, next: u16, hits: u16, receipts: [u64; 2]) {
    assert_eq!(
        game.preparation(ActorId(1)).unwrap().progress,
        PreparationProgress::Ability {
            next_strike: next,
            successful_primaries: hits,
            attempted_riders: receipts,
            linked_hold: None,
        }
    );
}

#[test]
fn sequence_rider_hit_miss_table_counts_only_successful_primaries_and_attempts_once() {
    for pattern in ["HH", "HM", "MH", "MM", "HHHH", "HMHH", "MHHH", "HMMH"] {
        let strikes = pattern
            .bytes()
            .enumerate()
            .map(|(index, branch)| strike(index, branch == b'H'))
            .collect();
        let mut game = natural_game(strikes, vec![rider(Some(2), 7)], |_, _| {});
        let mut hit_mask = 0u16;
        for (index, branch) in pattern.bytes().enumerate() {
            resolve(&mut game, index);
            let hit = branch == b'H';
            let check = primary(&game, index).check.unwrap();
            assert_eq!(check.actor, CheckActor::User);
            assert_eq!(check.success, hit, "fixture branch {pattern}/{index}");
            if hit {
                hit_mask |= 1 << index;
            }
            let fired = usize::from(hit_mask.count_ones() >= 2);
            assert_eq!(riders(&game).len(), fired, "{pattern}/{index}");
            assert_eq!(
                game.health(ActorId(2)).unwrap().0,
                100 - u64::from(hit_mask.count_ones()) * 3 - fired as u64 * 7,
                "{pattern}/{index}"
            );
            if index + 1 < pattern.len() {
                assert_progress(&game, index as u16 + 1, hit_mask, [fired as u64, 0]);
            }
        }
        assert!(game.preparation(ActorId(1)).is_none());
    }
}

#[test]
fn sequence_rider_authored_minimum_is_not_hardcoded_to_two() {
    for (minimum, count) in [(1, 1), (1, 4), (3, 5), (16, 16)] {
        let mut game = natural_game(
            (0..count).map(|index| strike(index, true)).collect(),
            vec![rider(Some(minimum), 7)],
            |_, _| {},
        );
        for index in 0..count {
            resolve(&mut game, index);
            assert_eq!(
                riders(&game).len(),
                usize::from(index + 1 >= usize::from(minimum))
            );
        }
        assert_eq!(
            game.health(ActorId(2)).unwrap().0,
            100 - count as u64 * 3 - 7
        );
    }
}

#[test]
fn sequence_rider_equal_offsets_preserve_order_once_and_residual_edge() {
    use crate::resolution_diagnostics::TraceValueRecord;
    let mut strikes = (0..4).map(|index| strike(index, true)).collect::<Vec<_>>();
    for strike in &mut strikes {
        strike.offset = 0;
    }
    // The first row assigns its token to the gated rider structurally, even
    // though its checkless primary cannot qualify. Later copies have no token.
    strikes[0].check = None;
    let mut conditional = rider(Some(2), 0);
    conditional.check = Some(RiderCheckDef {
        skill: SkillSelector::SelectedWeapon,
        defense: DefenseKind::Physical,
    });
    conditional.effects = vec![Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![DamageComponentExpr {
                category: DamageType::Keen,
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
    let mut game = natural_game(strikes, vec![conditional], |_, species| {
        for (stat, value) in [(Stat::Edge, 5), (Stat::CheckEdge, -6)] {
            species.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Add { stat, value },
            });
        }
    });
    let original = game.preparation(ActorId(1)).unwrap().clone();
    game.cancel_intention(ActorId(1), original.intention.unwrap())
        .unwrap();
    game.apply_condition(
        ActorId(1),
        &crate::content_ids::ConditionId::new("readied").unwrap(),
        ActorId(1),
        100,
    )
    .unwrap();
    game.admit_intention(
        ActorId(1),
        original.plan.work().action(),
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    game.execute_next_intention().unwrap().outcome.unwrap();
    let captured = game.preparation(ActorId(1)).unwrap().plan.clone();
    let plan = captured.as_ability().unwrap();
    assert_eq!(plan.strikes[0].riders[0].check.as_ref().unwrap().edge, 0);
    assert_eq!(plan.strikes[1].riders[0].check.as_ref().unwrap().edge, -6);
    assert!(captured.valid(game.content().rules()));
    assert!(game.plan_source_valid(&captured));
    assert_eq!(
        serde_json::from_value::<super::Plan>(serde_json::to_value(&captured).unwrap()).unwrap(),
        captured
    );
    game.set_combat_diagnostics(true);
    resolve(&mut game, 0);
    let order = game
        .combat_events()
        .iter()
        .filter_map(|event| match event {
            CombatEvent::Resolved { outcome, strike } if outcome.actor == ActorId(1) => {
                Some(*strike)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(order, [Some(0), Some(1), Some(2), None, Some(3)]);
    for index in 1..4 {
        assert!(primary(&game, index).check.unwrap().success);
    }
    assert_eq!(riders(&game).len(), 1);
    assert!(riders(&game)[0].check.unwrap().success);
    let records = game.combat_diagnostics().unwrap().records();
    assert_eq!(records.len(), 4);
    let rider_damage = records[2]
        .trace
        .steps()
        .iter()
        .rfind(|step| step.id == "damage.component")
        .unwrap();
    assert_eq!(
        rider_damage
            .fields
            .iter()
            .find(|field| field.id == "edge")
            .unwrap()
            .value,
        TraceValueRecord::Signed(2),
        "five shared Edge loses the primary's three-unit debit before the gated rider"
    );
    assert!((70..=85).contains(&game.health(ActorId(2)).unwrap().0));
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.actors[&ActorId(1)].ready_at, 30);
}

#[test]
fn sequence_rider_zero_post_protection_primary_damage_still_qualifies() {
    for immune_component in [false, true] {
        let mut game = natural_game(
            (0..3).map(|index| strike(index, true)).collect(),
            vec![rider(Some(2), 7)],
            |_, species| {
                let selector = Selector::Category {
                    category: DamageType::Impact,
                };
                species.modifiers.push(Modifier {
                    scope: Scope::Always,
                    op: if immune_component {
                        ModifierOp::Immunity { selector }
                    } else {
                        ModifierOp::Reduction {
                            selector,
                            amount: 100,
                        }
                    },
                });
            },
        );
        resolve(&mut game, 0);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
        assert!(primary(&game, 0).check.unwrap().success);
        assert_progress(&game, 1, 1, [0, 0]);
        resolve(&mut game, 1);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 93);
        assert_eq!(riders(&game).len(), 1);
        assert_progress(&game, 2, 3, [1, 0]);
        resolve(&mut game, 2);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 93);
        assert_eq!(riders(&game).len(), 1);
    }
}

#[derive(Clone, Copy, Debug)]
enum Nonqualifying {
    Miss,
    Checkless,
    PersonalSuccess,
    TargetSaveSuccess,
    TargetSaveFailure,
    WholeImmunity,
}

fn exclude(strike: &mut StrikeDef, kind: Nonqualifying) {
    match kind {
        Nonqualifying::Miss => *strike = self::strike((strike.offset / 10) as usize, false),
        Nonqualifying::Checkless => strike.check = None,
        Nonqualifying::PersonalSuccess
        | Nonqualifying::TargetSaveSuccess
        | Nonqualifying::TargetSaveFailure => {
            strike.check = Some(CheckDef {
                actor: if matches!(kind, Nonqualifying::PersonalSuccess) {
                    CheckActor::User
                } else {
                    CheckActor::Target
                },
                skill: SkillSelector::SelectedWeapon,
                against: Against::Difficulty {
                    formula: Formula {
                        base: if matches!(kind, Nonqualifying::TargetSaveFailure) {
                            1000
                        } else {
                            -1000
                        },
                        skill: None,
                        attribute: None,
                    },
                },
            });
        }
        Nonqualifying::WholeImmunity => {
            strike.descriptors = vec![DescriptorId::new("fire").unwrap()];
        }
    }
}

fn fire_immunity(_: &mut RulesetDocument, species: &mut crate::creatures::Species) {
    species.modifiers.push(Modifier {
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: Selector::Descriptor {
                descriptor: DescriptorId::new("fire").unwrap(),
            },
        },
    });
}

#[test]
fn sequence_rider_nonqualifying_primary_does_not_advance_the_hit_prefix() {
    for kind in [
        Nonqualifying::Miss,
        Nonqualifying::Checkless,
        Nonqualifying::PersonalSuccess,
        Nonqualifying::TargetSaveSuccess,
        Nonqualifying::TargetSaveFailure,
        Nonqualifying::WholeImmunity,
    ] {
        let mut strikes = (0..4).map(|index| strike(index, true)).collect::<Vec<_>>();
        exclude(&mut strikes[1], kind);
        let mut game = natural_game(strikes, vec![rider(Some(2), 7)], fire_immunity);
        resolve(&mut game, 0);
        let rng = game.rules_rng;
        resolve(&mut game, 1);
        assert_eq!(riders(&game).len(), 0, "{kind:?}");
        assert_progress(&game, 2, 1, [0, 0]);
        match kind {
            Nonqualifying::WholeImmunity => {
                assert!(primary(&game, 1).check.is_none());
                assert_eq!(
                    game.rules_rng, rng,
                    "whole immunity skips the primary check"
                );
            }
            Nonqualifying::Checkless => assert!(primary(&game, 1).check.is_none()),
            _ => {
                let check = primary(&game, 1).check.unwrap();
                assert_eq!(
                    check.success,
                    !matches!(kind, Nonqualifying::Miss | Nonqualifying::TargetSaveFailure),
                    "{kind:?}"
                );
            }
        }
        resolve(&mut game, 2);
        assert_eq!(riders(&game).len(), 1, "{kind:?}");
        assert_progress(&game, 3, 5, [1, 0]);
        resolve(&mut game, 3);
        assert_eq!(riders(&game).len(), 1, "{kind:?}");
    }
}

#[test]
fn sequence_rider_later_source_occurrence_requires_its_own_qualifying_primary() {
    for kind in [
        None,
        Some(Nonqualifying::Miss),
        Some(Nonqualifying::Checkless),
        Some(Nonqualifying::PersonalSuccess),
        Some(Nonqualifying::TargetSaveSuccess),
        Some(Nonqualifying::WholeImmunity),
    ] {
        let mut strikes = (0..4).map(|index| strike(index, true)).collect::<Vec<_>>();
        if let Some(kind) = kind {
            exclude(&mut strikes[2], kind);
        }
        strikes[2].riders.push(rider(Some(2), 7));
        let mut game = natural_game(strikes, vec![], fire_immunity);
        resolve(&mut game, 0);
        resolve(&mut game, 1);
        assert_progress(&game, 2, 3, [0, 0]);
        resolve(&mut game, 2);
        let fired = kind.is_none();
        assert_eq!(riders(&game).len(), usize::from(fired), "{kind:?}");
        assert_progress(
            &game,
            3,
            if fired { 7 } else { 3 },
            [if fired { 1 << 16 } else { 0 }, 0],
        );
        resolve(&mut game, 3);
        assert_eq!(riders(&game).len(), usize::from(fired), "{kind:?}");
    }
}

#[test]
fn sequence_rider_unreachable_primary_never_counts_and_returned_target_can_qualify() {
    let mut game = natural_game(
        (0..4).map(|index| strike(index, true)).collect(),
        vec![rider(Some(2), 7)],
        |_, _| {},
    );
    resolve(&mut game, 0);
    let near = game.actors[&ActorId(2)].location;
    let mut far = near;
    far.position.x = 4;
    game.teleport(ActorId(2), far).unwrap();
    let rng = game.rules_rng;
    resolve(&mut game, 1);
    assert_eq!(primary(&game, 1).result, super::OutcomeStatus::Miss);
    assert!(primary(&game, 1).check.is_none());
    assert_eq!(game.rules_rng, rng);
    assert_progress(&game, 2, 1, [0, 0]);
    assert!(riders(&game).is_empty());
    game.teleport(ActorId(2), near).unwrap();
    resolve(&mut game, 2);
    assert_progress(&game, 3, 5, [1, 0]);
    resolve(&mut game, 3);
    assert_eq!(riders(&game).len(), 1);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 84);
}

#[test]
fn sequence_rider_successful_ordinary_rider_check_does_not_count_as_a_primary() {
    let mut strikes = (0..4)
        .map(|index| strike(index, index != 1))
        .collect::<Vec<_>>();
    let mut ordinary = rider(None, 1);
    ordinary.check = Some(RiderCheckDef {
        skill: SkillSelector::SelectedWeapon,
        defense: DefenseKind::Physical,
    });
    strikes[0].riders.push(ordinary);
    let mut game = natural_game(strikes, vec![rider(Some(2), 7)], |_, _| {});
    resolve(&mut game, 0);
    assert_eq!(riders(&game).len(), 1);
    let check = riders(&game)[0].check.unwrap();
    assert_eq!(check.actor, CheckActor::User);
    assert!(check.success);
    assert_progress(&game, 1, 1, [0, 0]);
    resolve(&mut game, 1);
    assert_eq!(riders(&game).len(), 1);
    assert_progress(&game, 2, 1, [0, 0]);
    resolve(&mut game, 2);
    assert_eq!(riders(&game).len(), 2);
    assert_progress(&game, 3, 5, [1 << 1, 0]);
    resolve(&mut game, 3);
    assert_eq!(riders(&game).len(), 2);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 83);
}

#[test]
fn sequence_rider_attempt_is_consumed_by_checked_failure_protection_or_whole_immunity() {
    for mode in ["checked_failure", "full_protection", "whole_immunity"] {
        let mut conditional = rider(Some(2), 7);
        if mode == "checked_failure" {
            conditional.check = Some(RiderCheckDef {
                skill: SkillSelector::SelectedWeapon,
                defense: DefenseKind::Cognitive,
            });
        } else if mode == "whole_immunity" {
            conditional.descriptors = vec![DescriptorId::new("fire").unwrap()];
        }
        let mut game = natural_game(
            (0..4).map(|index| strike(index, true)).collect(),
            vec![conditional],
            |rules, species| {
                fire_immunity(rules, species);
                if mode == "full_protection" {
                    species.modifiers.push(Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Reduction {
                            selector: Selector::Category {
                                category: DamageType::Keen,
                            },
                            amount: 100,
                        },
                    });
                }
            },
        );
        resolve(&mut game, 0);
        assert!(riders(&game).is_empty());
        resolve(&mut game, 1);
        assert_eq!(riders(&game).len(), 1, "{mode}");
        let outcome = riders(&game)[0];
        if mode == "checked_failure" {
            assert!(!outcome.check.unwrap().success);
            assert_eq!(outcome.result, super::OutcomeStatus::Miss);
        } else {
            assert_eq!(outcome.result, super::OutcomeStatus::Unaffected);
        }
        assert_progress(&game, 2, 3, [1, 0]);
        resolve(&mut game, 2);
        assert_eq!(riders(&game).len(), 1, "{mode}");
        assert_progress(&game, 3, 7, [1, 0]);
        resolve(&mut game, 3);
        assert_eq!(riders(&game).len(), 1, "{mode}");
        assert_eq!(game.health(ActorId(2)).unwrap().0, 88, "{mode}");
    }
}

#[test]
fn sequence_rider_appended_identity_uses_each_natural_boundary_and_keeps_local_occurrences() {
    let mut strikes = (0..4).map(|index| strike(index, true)).collect::<Vec<_>>();
    strikes[0].riders = vec![rider(None, 2)];
    strikes[1].riders = vec![rider(None, 1), rider(Some(2), 5)];
    strikes[2].riders = vec![rider(Some(2), 5)];
    let mut game = natural_game(
        strikes,
        vec![rider(Some(2), 7), rider(Some(3), 11)],
        |_, _| {},
    );
    let plan = game
        .preparation(ActorId(1))
        .unwrap()
        .plan
        .as_ability()
        .unwrap();
    let boundaries = plan
        .strikes
        .iter()
        .map(|strike| match &strike.origin {
            super::StrikeOrigin::NaturalMelee { natural_riders, .. } => *natural_riders,
            _ => panic!("natural melee source provenance"),
        })
        .collect::<Vec<_>>();
    assert_eq!(boundaries, [1, 2, 1, 0]);
    resolve(&mut game, 0);
    assert_eq!(riders(&game).len(), 1);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 95);
    assert_progress(&game, 1, 1, [0, 0]);
    resolve(&mut game, 1);
    assert_eq!(riders(&game).len(), 4);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 79);
    assert_progress(&game, 2, 3, [(1 << 1) | (1 << 9), 0]);
    resolve(&mut game, 2);
    assert_eq!(riders(&game).len(), 6);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 60);
    assert_progress(&game, 3, 7, [(1 << 1) | (1 << 2) | (1 << 9) | (1 << 16), 0]);
    resolve(&mut game, 3);
    assert_eq!(riders(&game).len(), 6);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 57);
}

#[test]
fn sequence_rider_source_local_upper_receipt_word_roundtrips_with_an_unfinished_tail() {
    let mut strikes = (0..10).map(|index| strike(index, true)).collect::<Vec<_>>();
    strikes[8].riders.push(rider(Some(2), 7));
    let mut game = natural_game(strikes, vec![], |_, _| {});
    for index in 0..8 {
        resolve(&mut game, index);
    }
    assert!(riders(&game).is_empty());
    assert_progress(&game, 8, 255, [0, 0]);
    resolve(&mut game, 8);
    assert_eq!(riders(&game).len(), 1);
    assert_progress(&game, 9, 511, [0, 1]);
    // As in the ordered-strike checkpoint fixture, persist at a next-decision
    // boundary rather than keeping the helper's artificial target delay.
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = crate::Game::restore_checkpoint(
        serde_json::from_value(snapshot).unwrap(),
        &shared,
        game.content().clone(),
    )
    .unwrap();
    assert_eq!(restored, game);
    assert_progress(&restored, 9, 511, [0, 1]);
    resolve(&mut game, 9);
    resolve(&mut restored, 9);
    assert_eq!(restored, game);
    assert_eq!(riders(&restored).len(), 1);
    assert_eq!(restored.health(ActorId(2)).unwrap().0, 63);
    assert!(restored.preparation(ActorId(1)).is_none());
}

#[test]
fn sequence_rider_lethal_primary_or_earlier_rider_skips_without_consuming_the_attempt() {
    for earlier_rider in [false, true] {
        let mut strikes = (0..3).map(|index| strike(index, true)).collect::<Vec<_>>();
        if earlier_rider {
            strikes[1].riders.push(rider(None, 1000));
        } else {
            strikes[1].on_success = vec![damage(DamageType::Impact, 1000)];
        }
        let mut game = natural_game(strikes, vec![rider(Some(2), 7)], |_, _| {});
        resolve(&mut game, 0);
        resolve(&mut game, 1);
        assert!(!game.alive(ActorId(2)));
        assert!(primary(&game, 1).check.unwrap().success);
        assert_eq!(riders(&game).len(), usize::from(earlier_rider));
        assert_progress(&game, 2, 3, [0, 0]);
        let rng = game.rules_rng;
        resolve(&mut game, 2);
        assert_eq!(
            game.rules_rng, rng,
            "dead target tail cannot check or attempt a rider"
        );
        assert!(primary(&game, 2).check.is_none());
        assert_eq!(riders(&game).len(), usize::from(earlier_rider));
        assert!(game.preparation(ActorId(1)).is_none());
    }
}

#[test]
fn sequence_rider_source_death_does_not_reinsert_the_preinstalled_tail() {
    let ability_id = crate::content_ids::AbilityId::new("self_lethal_sequence").unwrap();
    let mut strikes = (0..3).map(|index| strike(index, true)).collect::<Vec<_>>();
    strikes[1].on_success = vec![damage(DamageType::Impact, 1000)];
    let mut game = natural_game(strikes, vec![rider(Some(2), 7)], |rules, species| {
        let mut ability = rules
            .abilities
            .iter()
            .find(|ability| ability.basic_attack)
            .unwrap()
            .clone();
        ability.id = ability_id.clone();
        ability.basic_attack = false;
        ability.reach = Reach::SelfOnly;
        // NaturalMelee intentionally requires melee reach. This ordinary
        // authored self-target sequence owns the source-death lifecycle probe.
        let StrikeSource::NaturalMelee { additional_riders } = &ability.strikes else {
            unreachable!()
        };
        ability.strikes = StrikeSource::Authored {
            strikes: append_natural_riders(species.melee.strikes(), additional_riders).unwrap(),
        };
        rules.constants.default_abilities.push(ability_id.clone());
        rules.abilities.push(ability);
    });
    let original = game.preparation(ActorId(1)).unwrap().intention.unwrap();
    game.cancel_intention(ActorId(1), original).unwrap();
    let intention = game
        .admit_intention(
            ActorId(1),
            crate::Action::UseAbility {
                id: ability_id,
                invocation: super::AbilityInvocation::ActorTarget { target: ActorId(1) },
                supplies: Default::default(),
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    game.execute_next_intention().unwrap().outcome.unwrap();
    resolve(&mut game, 0);
    assert_eq!(game.health(ActorId(1)).unwrap().0, 97);
    assert_progress(&game, 1, 1, [0, 0]);
    let suspended = game.preparation(ActorId(1)).unwrap().clone();
    assert!(
        !suspended.active,
        "history publication must preserve self-damage suspension"
    );
    game.start_work(suspended.plan, suspended.intention)
        .unwrap();
    resolve(&mut game, 1);
    assert!(primary(&game, 1).check.unwrap().success);
    assert!(!game.alive(ActorId(1)));
    assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
    assert!(riders(&game).is_empty());
    assert!(game.preparation(ActorId(1)).is_none());
    let rng = game.rules_rng;
    let events = game.combat_events().to_vec();
    resolve(&mut game, 2);
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.combat_events(), events);
    assert!(game.preparation(ActorId(1)).is_none());
    assert_eq!(game.combat_events().iter().filter(|event| matches!(event,
        CombatEvent::SequenceEnded {
            actor: ActorId(1), intention: Some(id), reason: crate::combat::SequenceEnd::Abandoned,
        } if *id == intention
    )).count(), 1);
}
