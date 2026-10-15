//! Captured shape admission: immutable sources, never today's derived operands.
use super::*;
use crate::rules::{
    Against, AllyRange, CheckActor, ConditionLifecycle, Reach, StrikeDef, StrikeSource,
};
use crate::{Game, ItemId};
use std::collections::BTreeSet;

fn check_matches(expected: Option<&crate::rules::CheckDef>, actual: Option<&PlannedCheck>) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(expected), Some(actual)) => {
            expected.actor == actual.actor
                && expected.target_participates() == actual.target_participates
                && matches!(
                    (&expected.against, &actual.threshold),
                    (
                        Against::Defense { .. } | Against::Difficulty { .. },
                        CheckThreshold::Fixed { .. }
                    ) | (Against::Opposed { .. }, CheckThreshold::Opposed { .. })
                )
        }
        _ => false,
    }
}
fn strike_matches(expected: &StrikeDef, actual: &FrozenStrike, natural: bool) -> bool {
    expected.descriptors == actual.descriptors
        && check_matches(expected.check.as_ref(), actual.check.as_ref())
        && plan::effects_match_definition_scoped(&expected.on_success, &actual.on_success, natural)
        && plan::effects_match_definition_scoped(&expected.on_failure, &actual.on_failure, natural)
        && expected.riders.len() == actual.riders.len()
        && expected
            .riders
            .iter()
            .zip(&actual.riders)
            .all(|(expected, actual)| {
                expected.trigger == actual.trigger
                    && expected.descriptors == actual.descriptors
                    && plan::effects_match_definition(&expected.effects, &actual.effects)
                    && match (&expected.check, &actual.check) {
                        (None, None) => true,
                        (Some(_), Some(actual)) => {
                            actual.actor == CheckActor::User
                                && actual.target_participates
                                && matches!(actual.threshold, CheckThreshold::Fixed { .. })
                        }
                        _ => false,
                    }
            })
}
fn source_matches(authored: &[StrikeDef], frozen: &[FrozenStrike], natural: bool) -> bool {
    let mut actual = frozen.iter().peekable();
    for (ordinal, expected) in authored.iter().enumerate() {
        match actual.peek() {
            Some(strike) if usize::from(strike.origin.ordinal()) == ordinal => {
                if !strike_matches(expected, strike, natural) {
                    return false;
                }
                actual.next();
            }
            _ if expected.unlock.is_some() => {}
            _ => return false,
        }
    }
    actual.next().is_none()
}
fn effect_descriptors(effects: &[PlannedEffect]) -> usize {
    effects
        .iter()
        .map(|effect| match effect {
            PlannedEffect::Damage { damage } => damage
                .components()
                .iter()
                .filter(|component| component.key().descriptor.is_some())
                .count(),
            _ => 0,
        })
        .sum()
}
impl AbilityPlan {
    pub fn valid(&self, rules: &crate::rules::Ruleset) -> bool {
        let Some(definition) = rules.ability(&self.ability) else {
            return false;
        };
        let cost = definition
            .cost
            .as_ref()
            .map(|cost| crate::costs::ResourceCost {
                resource: cost.resource,
                start: cost.start,
                resolution: cost.resolution,
            });
        if self.invocation.kind() != definition.invocation
            || self.invocation.target(self.actor) != self.target
            || match (self.invocation, self.step) {
                (AbilityInvocation::ActorTarget { .. }, None) => false,
                (AbilityInvocation::SelfStep { direction }, Some(step)) => {
                    direction != step.direction || !step.valid()
                }
                _ => true,
            }
            || !plan::valid_actor(self.actor)
            || !plan::valid_actor(self.target)
            || !(1..=crate::limits::NUMERIC as u64).contains(&self.preparation)
            || self.recovery > crate::limits::NUMERIC as u64
            || self.cost != cost
            || self.cooldown != definition.cooldown
            || self.supply_costs != definition.supply_costs
            || !self.supplies.valid(self.supply_costs.len())
            || self.strikes.is_empty()
            || self.strikes.len() > crate::limits::STRIKES
            || self.strikes[0].offset != 0
            || self
                .strikes
                .windows(2)
                .any(|pair| pair[0].offset > pair[1].offset)
            || self.strikes.iter().any(|strike| !strike.valid(rules))
            || self
                .preparation
                .checked_add(self.strikes.last().unwrap().offset)
                .and_then(|value| value.checked_add(self.recovery))
                .and_then(|value| value.checked_add(self.step.map_or(0, |step| step.duration)))
                .is_none_or(|value| value > crate::limits::NUMERIC as u64)
            || self
                .strikes
                .iter()
                .map(|strike| {
                    strike.on_success.len()
                        + strike.on_failure.len()
                        + strike
                            .riders
                            .iter()
                            .map(|rider| rider.effects.len())
                            .sum::<usize>()
                })
                .sum::<usize>()
                > crate::limits::EFFECTS_PER_SEQUENCE
            || self
                .strikes
                .iter()
                .map(|strike| {
                    strike.descriptors.len()
                        + effect_descriptors(&strike.on_success)
                        + effect_descriptors(&strike.on_failure)
                        + strike
                            .riders
                            .iter()
                            .map(|rider| {
                                rider.descriptors.len() + effect_descriptors(&rider.effects)
                            })
                            .sum::<usize>()
                })
                .sum::<usize>()
                > crate::limits::DESCRIPTORS_PER_SEQUENCE
            || self.tokens.len() > 2 * crate::limits::CONDITIONS
            || self
                .tokens
                .iter()
                .map(|claim| &claim.reservation)
                .collect::<BTreeSet<_>>()
                .len()
                != self.tokens.len()
        {
            return false;
        }
        if rules
            .validate_planned_interaction(
                definition.interaction,
                self.strikes.iter().flat_map(|strike| {
                    strike
                        .on_success
                        .iter()
                        .chain(&strike.on_failure)
                        .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                }),
            )
            .is_err()
        {
            return false;
        }
        let reach = match (&definition.reach, &self.reach) {
            (Reach::SelfOnly, Reach::SelfOnly) => self.actor == self.target,
            (
                Reach::SelfOrAlly {
                    range: AllyRange::Touch,
                },
                Reach::SelfOrAlly {
                    range: AllyRange::Touch,
                },
            ) => true,
            (
                Reach::SelfOrAlly {
                    range: AllyRange::Sight { .. },
                },
                Reach::SelfOrAlly {
                    range: AllyRange::Sight { range },
                },
            ) => (1..=crate::limits::SIGHT_RANGE).contains(range),
            (Reach::Melee, Reach::Melee) => self.actor != self.target,
            (Reach::Sight { .. }, Reach::Sight { range }) => {
                self.actor != self.target && (1..=crate::limits::SIGHT_RANGE).contains(range)
            }
            _ => false,
        };
        if !reach {
            return false;
        }
        let source = match &definition.strikes {
            StrikeSource::Authored { strikes } => {
                self.strikes
                    .iter()
                    .all(|strike| matches!(strike.origin, StrikeOrigin::Ability { .. }))
                    && source_matches(strikes, &self.strikes, false)
            }
            StrikeSource::NaturalMelee { additional_riders } => match &self.strikes[0].origin {
                StrikeOrigin::NaturalMelee { species, .. } => self.strikes.iter().all(|strike| {
                    matches!(&strike.origin, StrikeOrigin::NaturalMelee { species: other, natural_riders, .. }
                        if other == species && usize::from(*natural_riders) + additional_riders.len() == strike.riders.len())
                }),
                _ => false,
            },
            StrikeSource::SelectedMelee => match &self.strikes[0].origin {
                StrikeOrigin::Natural { species, .. } => self.strikes.iter().all(|strike| {
                    matches!(&strike.origin, StrikeOrigin::Natural { species: other, .. } if other == species)
                }),
                StrikeOrigin::Weapon { item } => {
                    let first = &self.strikes[0];
                    self.strikes.len() == 1
                        && item.0 > 0
                        && item.0 != u64::MAX
                        && first.riders.is_empty()
                        && first.on_failure.is_empty()
                        && first.descriptors.is_empty()
                        && matches!(first.on_success.as_slice(), [PlannedEffect::Damage { .. }])
                        && matches!(first.check, Some(PlannedCheck {
                            actor: CheckActor::User,
                            target_participates: true,
                            threshold: CheckThreshold::Fixed { .. },
                            ..
                        }))
                }
                StrikeOrigin::Ability { .. } | StrikeOrigin::NaturalMelee { .. } => false,
            },
        };
        if !source {
            return false;
        }
        let first = &self.strikes[0];
        let mut sites = Vec::new();
        for side in [CheckSide::Initiating, CheckSide::Opposing] {
            let phase = CheckPhase::Primary { side };
            if let Some(participants) = first.check_participants(phase, self.actor, self.target) {
                sites.push((phase, participants));
            }
        }
        for ordinal in 0..first.riders.len() {
            let phase = CheckPhase::Rider {
                ordinal: ordinal as u16,
            };
            if let Some(participants) = first.check_participants(phase, self.actor, self.target) {
                sites.push((phase, participants));
            }
        }
        self.tokens.iter().all(|claim| {
            let token = &claim.reservation;
            if claim.site.strike != 0 || !plan::valid_actor(token.source) {
                return false;
            }
            let Some(condition) = rules.condition(&token.condition) else {
                return false;
            };
            if token
                .qualification
                .as_ref()
                .is_some_and(|scope| !scope.valid(condition, rules))
            {
                return false;
            }
            sites
                .iter()
                .find(|(phase, (roller, opponent))| match condition.lifecycle {
                    ConditionLifecycle::NextCheck => {
                        token.holder == *roller
                            && token.qualification.as_ref().is_none_or(|scope| {
                                first.check_role(*phase) == Some(CheckActor::User)
                                    && scope.matches(*opponent, Some(&self.ability), rules)
                            })
                            && first.check_role(*phase).is_some_and(|role| {
                                definition
                                    .outgoing_edge_policy
                                    .for_role(role)
                                    .selects_token(condition)
                            })
                    }
                    ConditionLifecycle::NextCheckAgainst => token.holder == *opponent,
                    ConditionLifecycle::Timed => false,
                })
                .is_some_and(|(phase, _)| *phase == claim.site.check)
        })
    }
}
impl Game {
    pub(crate) fn plan_source_valid(&self, plan: &Plan) -> bool {
        if let Plan::Respec(plan) = plan {
            return match &plan.supplies {
                crate::supplies::SupplySelection::Carried => true,
                crate::supplies::SupplySelection::Explicit { items } => {
                    items.iter().all(|row| row.item.0 < self.next_item_id)
                }
            };
        }
        let Some(ability) = plan.as_ability() else {
            return true;
        };
        if let crate::public_plan::PublicAim::Cell { location } = ability.public_aim {
            if !self.world.knows(location) {
                return false;
            }
        }
        if let crate::supplies::SupplySelection::Explicit { items } = &ability.supplies {
            // Historical sources may be deleted after Spend, but their IDs must
            // still have been issued. No live source lookup or payment history.
            if items.iter().any(|row| row.item.0 >= self.next_item_id) {
                return false;
            }
        }
        match ability.strikes.first().map(|strike| &strike.origin) {
            Some(StrikeOrigin::Natural { species, .. }) => self
                .content
                .catalog()
                .species
                .get(species)
                .is_some_and(|source| {
                    source_matches(source.melee.strikes(), &ability.strikes, true)
                }),
            Some(StrikeOrigin::NaturalMelee { species, .. }) => {
                let Some(source) = self.content.catalog().species.get(species) else {
                    return false;
                };
                let Some(definition) = self.content.rules().ability(&ability.ability) else {
                    return false;
                };
                let StrikeSource::NaturalMelee { additional_riders } = &definition.strikes else {
                    return false;
                };
                crate::rules::append_natural_riders(source.melee.strikes(), additional_riders)
                    .is_ok_and(|strikes| {
                        source_matches(&strikes, &ability.strikes, true)
                            && ability.strikes.iter().all(|strike| match &strike.origin {
                                StrikeOrigin::NaturalMelee {
                                    ordinal,
                                    natural_riders,
                                    ..
                                } => source
                                    .melee
                                    .strikes()
                                    .get(usize::from(*ordinal))
                                    .is_some_and(|source| {
                                        source.riders.len() == usize::from(*natural_riders)
                                    }),
                                _ => false,
                            })
                    })
            }
            Some(StrikeOrigin::Weapon { item: ItemId(item) }) => *item < self.next_item_id,
            Some(StrikeOrigin::Ability { .. }) => true,
            None => false,
        }
    }
}
