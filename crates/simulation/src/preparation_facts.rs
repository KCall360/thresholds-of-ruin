//! Preparation-owned classification of supplemental-region requirements.
//!
//! Derived, bounded and branch local. Only a matching actor publication can
//! reuse the immutable-plan classification; availability always runs live.
use crate::abilities::{Plan, PlannedEffect};
use crate::mutation::MutationStamp;
use crate::{ActorId, Game};
use std::collections::BTreeMap;
use std::sync::Mutex;
use tor_world::Shared;

const MAX_OWNERS: usize = 512;

#[derive(Default)]
pub(crate) struct PreparationFacts(Mutex<Shared<BTreeMap<ActorId, Facts>>>);
impl Clone for PreparationFacts {
    fn clone(&self) -> Self {
        Self(Mutex::new(
            self.0.lock().expect("preparation facts").clone(),
        ))
    }
}
impl std::fmt::Debug for PreparationFacts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparationFacts")
            .field("owners", &self.0.lock().expect("preparation facts").len())
            .finish()
    }
}
impl PartialEq for PreparationFacts {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for PreparationFacts {}

#[derive(Clone, Copy)]
struct Facts {
    owner: MutationStamp,
    no_supplemental_regions: bool,
}
impl Game {
    fn preparation_facts_for(&self, actor: ActorId) -> Option<Facts> {
        let owner = self.actor_mutation_stamp(actor)?;
        if let Some(facts) = self
            .preparation_facts
            .0
            .lock()
            .expect("preparation facts")
            .get(&actor)
            .filter(|facts| facts.owner == owner)
            .copied()
        {
            return Some(facts);
        }
        let preparation = self.preparation(actor)?;
        let facts = Facts {
            owner,
            no_supplemental_regions: no_supplemental_regions(&preparation.plan),
        };
        self.store_preparation_facts(actor, facts);
        Some(facts)
    }
    fn store_preparation_facts(&self, actor: ActorId, facts: Facts) {
        let mut rows = self.preparation_facts.0.lock().expect("preparation facts");
        if rows.len() >= MAX_OWNERS && !rows.contains_key(&actor) {
            rows.pop_first();
        }
        rows.insert(actor, facts);
    }
    pub(crate) fn preparation_has_no_supplemental_regions(&self, actor: ActorId) -> bool {
        self.preparation_facts_for(actor)
            .is_some_and(|facts| facts.no_supplemental_regions)
    }
}

/// Mirrors the supplemental-region policies, not general effect legality.
/// Exhaustive matching makes newly added effects demand an explicit decision.
fn effect_needs_regions(effect: &PlannedEffect) -> bool {
    match effect {
        PlannedEffect::Theft { .. }
        | PlannedEffect::TeleportRandom
        | PlannedEffect::TeleportToRayOrigin
        | PlannedEffect::RoomLight
        | PlannedEffect::DetectMonsters { .. }
        | PlannedEffect::DetectFood
        | PlannedEffect::MapTopology
        | PlannedEffect::DetectMagic
        | PlannedEffect::DrainLife
        | PlannedEffect::CancelSpecials
        | PlannedEffect::BalefulPolymorph { .. } => true,
        PlannedEffect::AlterEquipment { .. }
        | PlannedEffect::CorrodeArmor { .. }
        | PlannedEffect::ApplyLinkedHold { .. }
        | PlannedEffect::HoldDamage { .. }
        | PlannedEffect::DrainLatestHitDie
        | PlannedEffect::RestoreHitDie
        | PlannedEffect::GainStrength
        | PlannedEffect::RaiseLevel
        | PlannedEffect::HasteDose { .. }
        | PlannedEffect::ArrestDescent
        | PlannedEffect::StepSelfOneCell
        | PlannedEffect::PushTargetOneCell { .. }
        | PlannedEffect::Damage { .. }
        | PlannedEffect::Heal { .. }
        | PlannedEffect::HealPerHitDie { .. }
        | PlannedEffect::Nourish { .. }
        | PlannedEffect::LoseAttribute { .. }
        | PlannedEffect::RestoreAttribute { .. }
        | PlannedEffect::LoseMaximumHealth { .. }
        | PlannedEffect::RestoreMaximumHealth { .. }
        | PlannedEffect::ApplyCondition { .. }
        | PlannedEffect::ApplyPersistentCondition { .. }
        | PlannedEffect::ApplyConditionForTurns { .. }
        | PlannedEffect::RemoveCondition { .. }
        | PlannedEffect::Restore { .. }
        | PlannedEffect::SearchAdjacentGuises { .. }
        | PlannedEffect::ObservePublicPlan
        | PlannedEffect::ApplyQualifiedConditionToSource { .. } => false,
    }
}
fn no_supplemental_regions(plan: &Plan) -> bool {
    // Complex policies stay cold even when a particular instance would be empty.
    let Plan::Ability(ability) = plan else {
        return false;
    };
    !plan.on_success().iter().any(effect_needs_regions)
        && ability.strikes.iter().all(|strike| {
            !strike
                .on_success
                .iter()
                .chain(&strike.on_failure)
                .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
                .any(effect_needs_regions)
        })
}
