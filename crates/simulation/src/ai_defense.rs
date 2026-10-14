//! Narrow, data-shaped defensive candidates; ordinary plans own all execution.
use crate::abilities::InvocationKind;
use crate::rules::{
    AbilityDef, ConditionDef, Effect, Eviction, ModifierOp, RangePreference, Reach, Ruleset, Scope,
    Selector, Stacking, Stat, StrikeSource,
};
use crate::{ActorId, Game};
use std::collections::BTreeSet;

/// A recognized shape cannot conceal damage, a conditional failure branch,
/// granted behavior, source-qualified effects, stacking or eviction side effects.
fn defensive_condition<'a>(
    definition: &AbilityDef,
    rules: &'a Ruleset,
) -> Option<&'a ConditionDef> {
    if definition.ai.hostile || definition.invocation != InvocationKind::ActorTarget {
        return None;
    }
    let StrikeSource::Authored { strikes } = &definition.strikes else {
        return None;
    };
    let [strike] = strikes.as_slice() else {
        return None;
    };
    if strike.check.is_some() || !strike.on_failure.is_empty() || !strike.riders.is_empty() {
        return None;
    }
    let [Effect::ApplyCondition { condition, .. }] = strike.on_success.as_slice() else {
        return None;
    };
    let condition = rules.condition(condition)?;
    if condition.source_scoped
        || matches!(condition.stacking, Stacking::Stack { .. })
        || condition.capacity != Eviction::Reject
        || condition.modifiers.is_empty()
        || !condition.modifiers.iter().all(|modifier| {
            modifier.scope == Scope::Always
                && match modifier.op {
                    ModifierOp::Add {
                        stat: Stat::IncomingEdge { .. },
                        value,
                    } => value < 0,
                    ModifierOp::TalentReduction { amount, .. } => amount > 0,
                    _ => false,
                }
        })
    {
        return None;
    }
    Some(condition)
}

impl Game {
    pub(super) fn choose_ai_defense(
        &self,
        actor: ActorId,
        threat: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<crate::abilities::Plan> {
        let creature = self.creature(actor)?;
        let derived = creature.derived();
        let rules = creature.build().ruleset();
        let eligible = |definition: &AbilityDef| {
            if derived.ability_reach(&definition.id).ok() == Some(Reach::SelfOnly) {
                defensive_condition(definition, rules)
            } else {
                None
            }
        };
        // Exact paid work retains its frozen operands, claims and clocks even if
        // its fresh resource threshold or cooldown would now reject admission.
        if let Some(pending) = self
            .preparation(actor)
            .filter(|pending| !pending.active && !pending.has_prefix())
        {
            if let Some(plan) = pending.plan.as_ability() {
                if plan.target == actor
                    && derived.abilities.contains(&plan.ability)
                    && rules.ability(&plan.ability).and_then(eligible).is_some()
                    && self.preparation_still_available(pending, Some(sight))
                {
                    return self.physical_ai_plan(pending.plan.clone(), view);
                }
            }
        }
        let mut candidates: Vec<_> = derived
            .abilities
            .iter()
            .filter_map(|id| {
                let definition = rules.ability(id)?;
                eligible(definition).map(|condition| (definition, condition))
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        candidates.sort_by(|(a, _), (b, _)| {
            b.ai.priority
                .cmp(&a.ai.priority)
                .then_with(|| a.id.cmp(&b.id))
        });
        let mut active = BTreeSet::new();
        let mut condition_count = 0;
        let mut reserved_families = BTreeSet::new();
        for instance in creature.conditions().instances() {
            condition_count += 1;
            active.insert(&instance.condition);
            if instance.reserved_by.is_some() {
                reserved_families.insert(&rules.condition(&instance.condition)?.family);
            }
        }
        // The supported shapes never evict another condition to buy protection.
        if condition_count >= crate::limits::CONDITIONS {
            return None;
        }
        let incoming = derived.incoming_condition_edges(rules, threat).ok()?;
        let in_melee = self.attack_available_in_scene(actor, threat, sight);
        for (definition, condition) in candidates {
            crate::diagnostics::ai_defense_candidate();
            if active.contains(&condition.id)
                || reserved_families.contains(&condition.family)
                || (definition.ai.range_preference == RangePreference::OutsideMelee && in_melee)
            {
                continue;
            }
            let StrikeSource::Authored { strikes } = &definition.strikes else {
                unreachable!()
            };
            if condition
                .descriptors
                .iter()
                .chain(&strikes[0].descriptors)
                .any(|descriptor| {
                    derived.protection.has_immunity(&Selector::Descriptor {
                        descriptor: descriptor.clone(),
                    })
                })
            {
                continue;
            }
            let Some(mut improves) = incoming.improves(condition) else {
                continue;
            };
            let mut valid = true;
            for modifier in &condition.modifiers {
                if let ModifierOp::TalentReduction { category, amount } = modifier.op {
                    let selector = Selector::Category { category };
                    let current = derived.protection.talent_reduction(&selector);
                    let total =
                        derived.protection.reduction(&selector) - current + current.max(amount);
                    valid &= total <= crate::limits::NUMERIC as u32;
                    improves |= !derived.protection.has_immunity(&selector) && amount > current;
                }
            }
            if !valid || !improves {
                continue;
            }
            if let Some(cost) = &definition.cost {
                let available = creature.costs().available(cost.resource);
                if cost
                    .start
                    .checked_add(cost.resolution)
                    .is_none_or(|total| total > available)
                    || u64::from(available) * 100
                        < u64::from(creature.costs().resources().maximum(cost.resource))
                            * u64::from(definition.ai.minimum_resource_percent)
                {
                    continue;
                }
            }
            let Ok(plan) = self.ability_plan_in_scene(actor, &definition.id, actor, sight) else {
                continue;
            };
            if let Some(plan) = self.physical_ai_plan(plan, view) {
                return Some(plan);
            }
        }
        None
    }
}
