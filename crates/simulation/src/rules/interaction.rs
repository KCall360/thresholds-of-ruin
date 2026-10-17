//! Finite direct-harm validation, separate from AI priority or target ownership.
use super::*;

#[cfg(test)]
thread_local! { static MODIFIER_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Interaction {
    NonHostile,
    HostileToTarget,
}
impl<'de> Deserialize<'de> for Interaction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        match String::deserialize(d)?.as_str() {
            "non_hostile" => Ok(Self::NonHostile),
            "hostile_to_target" => Ok(Self::HostileToTarget),
            _ => Err(serde::de::Error::custom("invalid Interaction")),
        }
    }
}

/// Keep direct-harm invariants distinct from ordinary operand-bound conflicts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EffectiveInteractionError {
    DirectHarm,
    InvalidDamage,
}

impl Ruleset {
    /// Called for source-composed strikes as well as ordinary authored rows.
    pub fn validate_interaction(
        &self,
        interaction: Interaction,
        strikes: &[StrikeDef],
    ) -> Result<(), RulesetError> {
        self.validate_interaction_effects(
            interaction,
            strikes.iter().flat_map(|strike| {
                strike
                    .on_success
                    .iter()
                    .chain(&strike.on_failure)
                    .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
            }),
        )
    }
    /// Revalidate current unlocked operands after each modifier fold. Source harm
    /// still covers every authored row, including dormant ones.
    pub(crate) fn validate_effective_interaction(
        &self,
        interaction: Interaction,
        strikes: &[StrikeDef],
        modifiers: &crate::creatures::AbilityValues,
        natural: bool,
        active_talents: &std::collections::BTreeSet<TalentId>,
    ) -> Result<(), EffectiveInteractionError> {
        self.validate_interaction(interaction, strikes)
            .map_err(|_| EffectiveInteractionError::DirectHarm)?;
        if interaction == Interaction::HostileToTarget {
            return Ok(());
        }
        let mut invalid_damage = false;
        let mut effective_branch = |effects: &[Effect], primary: bool| {
            let mut first_damage = primary;
            for effect in effects {
                let Effect::Damage {
                    damage: DamageExpr::Components { components },
                } = effect
                else {
                    continue;
                };
                // Authored components augment every rolled pool. Natural melee
                // augments only the first damage payload's canonical primary in
                // each primary branch, never a rider or a secondary payload.
                let (dice, flat) = if natural {
                    (0, 0)
                } else {
                    (modifiers.dice, modifiers.flat)
                };
                let damage = compile_damage_components(components, dice, flat).and_then(|damage| {
                    if first_damage {
                        damage.with_melee_modifiers(modifiers.dice, modifiers.flat, 0)
                    } else {
                        Ok(damage)
                    }
                });
                first_damage = false;
                match damage {
                    Ok(damage) if damage.can_harm() => {
                        return Err(EffectiveInteractionError::DirectHarm);
                    }
                    Ok(_) => {}
                    // Keep checking bounded later payloads: a numeric conflict
                    // must not conceal an independently positive harm operand.
                    Err(_) => invalid_damage = true,
                }
            }
            Ok(())
        };
        for strike in strikes {
            if strike
                .unlock
                .as_ref()
                .is_some_and(|id| !active_talents.contains(id))
            {
                continue;
            }
            effective_branch(&strike.on_success, natural)?;
            effective_branch(&strike.on_failure, natural)?;
            for rider in &strike.riders {
                effective_branch(&rider.effects, false)?;
            }
        }
        if invalid_damage {
            Err(EffectiveInteractionError::InvalidDamage)
        } else {
            Ok(())
        }
    }
    /// Frozen admission uses the captured operands and pinned interaction only;
    /// it must not derive today's equipment, attributes, or source selection.
    pub(crate) fn validate_planned_interaction<'a>(
        &self,
        interaction: Interaction,
        mut effects: impl Iterator<Item = &'a crate::abilities::PlannedEffect>,
    ) -> Result<(), RulesetError> {
        use crate::abilities::{PlannedAmount, PlannedEffect};
        let positive = |amount: &PlannedAmount| match amount {
            PlannedAmount::Fixed { amount } => *amount > 0,
            PlannedAmount::Rolled { dice } => dice.maximum() > 0,
        };
        bound(
            interaction != Interaction::NonHostile
                || !effects.any(|effect| match effect {
                    PlannedEffect::ApplyLinkedHold { .. } | PlannedEffect::HoldDamage { .. } => {
                        true
                    }
                    PlannedEffect::Damage { damage } => damage.can_harm(),
                    PlannedEffect::PushTargetOneCell { .. } | PlannedEffect::DrainLatestHitDie => {
                        true
                    }
                    PlannedEffect::LoseAttribute { amount, .. }
                    | PlannedEffect::LoseMaximumHealth { amount } => positive(amount),
                    PlannedEffect::ApplyCondition { id, .. } => self.condition_directly_harms(id),
                    PlannedEffect::StepSelfOneCell
                    | PlannedEffect::RestoreHitDie
                    | PlannedEffect::Heal { .. }
                    | PlannedEffect::Nourish { .. }
                    | PlannedEffect::Restore { .. }
                    | PlannedEffect::RestoreAttribute { .. }
                    | PlannedEffect::RestoreMaximumHealth { .. }
                    | PlannedEffect::ObservePublicPlan
                    | PlannedEffect::ApplyQualifiedConditionToSource { .. } => false,
                }),
            "non_hostile_direct_harm",
        )
    }
    pub(super) fn validate_rider_interaction(
        &self,
        interaction: Interaction,
        riders: &[RiderDef],
    ) -> Result<(), RulesetError> {
        self.validate_interaction_effects(
            interaction,
            riders.iter().flat_map(|rider| &rider.effects),
        )
    }
    fn validate_interaction_effects<'a>(
        &self,
        interaction: Interaction,
        mut effects: impl Iterator<Item = &'a Effect>,
    ) -> Result<(), RulesetError> {
        bound(
            interaction != Interaction::NonHostile
                || !effects.any(|effect| self.effect_directly_harms(effect)),
            "non_hostile_direct_harm",
        )
    }
    fn effect_directly_harms(&self, effect: &Effect) -> bool {
        match effect {
            Effect::ApplyLinkedHold { .. } | Effect::HoldDamage { .. } => true,
            Effect::Damage {
                damage: DamageExpr::SelectedWeapon { .. },
            }
            | Effect::PushTargetOneCell { .. }
            | Effect::DrainLatestHitDie => true,
            Effect::Damage {
                damage: DamageExpr::Components { components },
            } => components
                .iter()
                .any(|component| positive_amount(&component.amount)),
            Effect::LoseAttribute { amount, .. } | Effect::LoseMaximumHealth { amount } => {
                positive_amount(amount)
            }
            Effect::ApplyCondition { condition, .. } => self.condition_directly_harms(condition),
            // Future consequences and source-only benefits do not imply intent.
            Effect::StepSelfOneCell
            | Effect::RestoreHitDie
            | Effect::Heal { .. }
            | Effect::Nourish { .. }
            | Effect::Restore { .. }
            | Effect::RestoreAttribute { .. }
            | Effect::RestoreMaximumHealth { .. }
            | Effect::ObservePublicPlan
            | Effect::ApplyQualifiedConditionToSource { .. } => false,
        }
    }
    fn condition_directly_harms(&self, condition: &ConditionId) -> bool {
        self.condition(condition).is_some_and(|definition| {
            !definition.restrictions.is_empty()
                || definition.preparation == ConditionPreparation::Pause
                || self.modifiers_directly_harm(&definition.modifiers)
        })
    }
    fn modifiers_directly_harm(&self, modifiers: &[Modifier]) -> bool {
        self.modifiers_directly_harm_once(modifiers, &mut std::collections::BTreeSet::new())
    }
    fn modifiers_directly_harm_once(
        &self,
        modifiers: &[Modifier],
        seen: &mut std::collections::BTreeSet<FlagId>,
    ) -> bool {
        #[cfg(test)]
        MODIFIER_VISITS.with(|visits| visits.set(visits.get() + modifiers.len()));
        // Catalog flag graphs and expansion sizes have already been validated.
        modifiers.iter().any(|modifier| match &modifier.op {
            ModifierOp::Locomotion { value } => matches!(
                value,
                crate::locomotion::LocomotionValue::Stationary { enabled: true }
                    | crate::locomotion::LocomotionValue::Flying { enabled: false }
            ),
            ModifierOp::Flag { flag } => {
                seen.insert(flag.clone())
                    && self.flag(flag).is_some_and(|definition| {
                        self.modifiers_directly_harm_once(&definition.modifiers, seen)
                    })
            }
            ModifierOp::Add { stat, value } => match stat {
                Stat::IncomingEdge { .. }
                | Stat::HealthRecoveryPeriod
                | Stat::HealthRecoveryPeriodPercent
                | Stat::HealthRecoveryResetPercent => *value > 0,
                Stat::Ability {
                    stat: AbilityStat::Preparation | AbilityStat::Recovery,
                    ..
                }
                | Stat::AbilityTag {
                    stat: AbilityStat::Preparation | AbilityStat::Recovery,
                    ..
                } => *value > 0,
                Stat::Ability {
                    stat: AbilityStat::Difficulty,
                    ..
                }
                | Stat::AbilityTag {
                    stat: AbilityStat::Difficulty,
                    ..
                } => false,
                _ => *value < 0,
            },
            // These floors immediately kill or clamp away existing resource
            // balance, independent of the recipient's positive starting value.
            ModifierOp::Set {
                stat: Stat::Health | Stat::ResourceMax { .. },
                value,
            } if *value <= 0 => true,
            // Other Sets depend on the recipient's existing derived value.
            // Do not predict arbitrary future state from it or a granted ability.
            ModifierOp::Set { .. }
            | ModifierOp::GrantAbility { .. }
            | ModifierOp::Immunity { .. }
            | ModifierOp::Reduction { .. }
            | ModifierOp::TalentReduction { .. } => false,
        })
    }
}

fn positive_amount(amount: &AmountExpr) -> bool {
    match amount {
        AmountExpr::Fixed { amount } => *amount > 0,
        AmountExpr::Rolled { dice } => {
            i64::from(dice.count) * i64::from(dice.sides) + i64::from(dice.bonus) > 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn condition_rules(
        modifiers: Vec<Modifier>,
        edit: impl FnOnce(&mut RulesetDocument),
    ) -> std::sync::Arc<Ruleset> {
        let mut document = crate::test_creatures::ruleset().document().clone();
        let condition = document
            .conditions
            .iter_mut()
            .find(|row| row.id.as_str() == "fear")
            .unwrap();
        condition.modifiers = modifiers;
        condition.restrictions.clear();
        condition.preparation = ConditionPreparation::Preserve;
        edit(&mut document);
        Ruleset::validate(document).unwrap()
    }
    fn rows() -> Vec<StrikeDef> {
        let mut rows = crate::test_creatures::species().melee.strikes().to_vec();
        rows[0].on_success = vec![Effect::ApplyCondition {
            condition: ConditionId::new("fear").unwrap(),
            duration: 100,
        }];
        rows
    }
    #[test]
    fn restorable_loss_interaction_uses_actual_positive_harm_not_restoration() {
        let rules = crate::test_creatures::ruleset();
        for amount in [0, 1] {
            for effect in [
                Effect::LoseAttribute {
                    attribute: crate::attributes::Attribute::Strength,
                    amount: AmountExpr::Fixed { amount },
                },
                Effect::LoseMaximumHealth {
                    amount: AmountExpr::Fixed { amount },
                },
            ] {
                let mut strikes = rows();
                strikes[0].on_success = vec![effect.clone()];
                assert_eq!(
                    rules
                        .validate_interaction(Interaction::NonHostile, &strikes)
                        .is_err(),
                    amount > 0
                );
                strikes[0].on_success.clear();
                strikes[0].riders = vec![RiderDef {
                    condition: None,
                    trigger: RiderTrigger::Success,
                    check: None,
                    effects: vec![effect],
                    descriptors: vec![],
                }];
                assert_eq!(
                    rules
                        .validate_interaction(Interaction::NonHostile, &strikes)
                        .is_err(),
                    amount > 0
                );
            }
        }
        let mut strikes = rows();
        strikes[0].on_success = vec![
            Effect::RestoreAttribute {
                attribute: crate::attributes::Attribute::Strength,
                amount: AmountExpr::Fixed { amount: 1 },
            },
            Effect::RestoreMaximumHealth {
                amount: AmountExpr::Fixed { amount: 1 },
            },
        ];
        rules
            .validate_interaction(Interaction::NonHostile, &strikes)
            .unwrap();
    }
    #[test]
    fn natural_additional_harm_is_rejected_without_waiting_for_a_species_grant() {
        for amount in [0, 1] {
            let mut document = crate::test_creatures::ruleset().document().clone();
            let ability = document
                .abilities
                .iter_mut()
                .find(|row| row.id.as_str() == "basic_melee")
                .unwrap();
            ability.interaction = Interaction::NonHostile;
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![RiderDef {
                    condition: None,
                    trigger: RiderTrigger::Success,
                    check: None,
                    effects: vec![Effect::LoseMaximumHealth {
                        amount: AmountExpr::Fixed { amount },
                    }],
                    descriptors: vec![],
                }],
            };
            assert_eq!(Ruleset::validate(document).is_err(), amount > 0);
        }
    }
    #[test]
    fn non_hostile_natural_grant_validates_even_dormant_species_strikes() {
        let base = crate::test_creatures::build(crate::test_creatures::species());
        let ability_id = AbilityId::new("basic_melee").unwrap();
        let mut document = base.ruleset().document().clone();
        let ability = document
            .abilities
            .iter_mut()
            .find(|row| row.id == ability_id)
            .unwrap();
        ability.interaction = Interaction::NonHostile;
        ability.strikes = StrikeSource::NaturalMelee {
            additional_riders: vec![],
        };
        let rules = Ruleset::validate(document).unwrap();
        let mut catalog = base.content().catalog().clone();
        let species = catalog.species.get_mut(base.species_id()).unwrap();
        let mut strikes = species.melee.strikes().to_vec();
        strikes[0].on_success = vec![Effect::Heal {
            amount: AmountExpr::Fixed { amount: 1 },
        }];
        strikes[0].on_failure.clear();
        strikes[0].riders.clear();
        let mut dormant = strikes[0].clone();
        dormant.unlock = Some(TalentId::new("rattling_blow").unwrap());
        dormant.on_success = vec![Effect::LoseMaximumHealth {
            amount: AmountExpr::Fixed { amount: 1 },
        }];
        strikes.push(dormant);
        species.melee = crate::attacks::NaturalAttack::new(
            species.melee.skill(),
            species.melee.bonus(),
            species.melee.wind_up(),
            species.melee.recovery(),
            strikes,
        )
        .unwrap();
        species.modifiers.push(Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: ability_id,
            },
        });
        let content = crate::creatures::Content::new(rules, catalog).unwrap();
        assert!(crate::creatures::CreatureBuild::new(
            content,
            base.species_id().clone(),
            base.ledger().clone(),
            base.binding(),
            *base.starting_skills()
        )
        .is_err());
    }
    #[test]
    fn non_hostile_rejects_immediate_zero_health_sets_through_flags() {
        for value in [-1, 0, 1] {
            let flag = FlagId::new("fixture_zero_health").unwrap();
            let rules = condition_rules(
                vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Flag { flag: flag.clone() },
                }],
                |document| {
                    document.flags.push(FlagDef {
                        id: flag,
                        name: "Zero Health".into(),
                        modifiers: vec![Modifier {
                            scope: Scope::Always,
                            op: ModifierOp::Set {
                                stat: Stat::Health,
                                value,
                            },
                        }],
                    });
                },
            );
            assert_eq!(
                rules
                    .validate_interaction(Interaction::NonHostile, &rows())
                    .is_err(),
                value <= 0,
                "Health Set {value}"
            );
        }
    }
    #[test]
    fn non_hostile_rejects_immediate_zero_resource_capacity_sets() {
        for value in [-1, 0, 1] {
            let rules = condition_rules(
                vec![Modifier {
                    scope: Scope::Always,
                    op: ModifierOp::Set {
                        stat: Stat::ResourceMax {
                            resource: crate::resources::Resource::Stamina,
                        },
                        value,
                    },
                }],
                |_| {},
            );
            assert_eq!(
                rules
                    .validate_interaction(Interaction::NonHostile, &rows())
                    .is_err(),
                value <= 0,
                "ResourceMax Set {value}"
            );
        }
    }
    #[test]
    fn non_hostile_flag_dag_visits_each_authored_modifier_at_most_once() {
        let flag = |i| FlagId::new(format!("fixture_branch_{i}")).unwrap();
        let reference = |i| Modifier {
            scope: Scope::Always,
            op: ModifierOp::Flag { flag: flag(i) },
        };
        let rules = condition_rules(vec![reference(0)], |document| {
            for i in 0..20 {
                document.flags.push(FlagDef {
                    id: flag(i),
                    name: "Branch".into(),
                    modifiers: if i == 19 {
                        vec![]
                    } else {
                        vec![reference(i + 1), reference(i + 1)]
                    },
                });
            }
        });
        let rows = rows();
        MODIFIER_VISITS.with(|visits| visits.set(0));
        rules
            .validate_interaction(Interaction::NonHostile, &rows)
            .unwrap();
        assert!(
            MODIFIER_VISITS.with(|visits| visits.get()) <= 41,
            "shared descendants must not multiply finite validation work"
        );
    }
    #[test]
    fn difficulty_modifiers_have_no_universal_harm_direction() {
        for actor in [CheckActor::User, CheckActor::Target] {
            for value in [-1, 1] {
                let ability = AbilityId::new("basic_melee").unwrap();
                let rules = condition_rules(
                    vec![Modifier {
                        scope: Scope::Always,
                        op: ModifierOp::Add {
                            stat: Stat::Ability {
                                ability: ability.clone(),
                                stat: AbilityStat::Difficulty,
                            },
                            value,
                        },
                    }],
                    |document| {
                        let definition = document
                            .abilities
                            .iter_mut()
                            .find(|row| row.id == ability)
                            .unwrap();
                        let mut strikes = crate::test_creatures::species().melee.strikes().to_vec();
                        strikes[0].check.as_mut().unwrap().actor = actor;
                        strikes[0].check.as_mut().unwrap().against = Against::Difficulty {
                            formula: Formula {
                                base: 10,
                                skill: None,
                                attribute: None,
                            },
                        };
                        definition.strikes = StrikeSource::Authored { strikes };
                    },
                );
                assert!(
                    rules
                        .validate_interaction(Interaction::NonHostile, &rows())
                        .is_ok(),
                    "{actor:?}: {value}"
                );
            }
        }
    }
}
