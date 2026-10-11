//! Actor-owned creature state. Rebuilds validate first, then reconcile mutable
//! state once; callers cannot edit a build or derived cache behind that boundary.

use super::{BuildError, CreatureBuild, DerivedCreature, SourceId};
use crate::conditions::{ConditionError, ConditionState, ConditionUpdate};
use crate::content_ids::ConditionId;
use crate::costs::{CostError, CostLedger, ResourceCost, StartCost};
use crate::health::{Health, HealthError};
use crate::resources::{Resource, ResourceError, Resources};
use crate::rules::{Modifier, Selector};
use crate::{ActorId, IntentionId};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::Shared;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureStateError {
    Build(BuildError),
    Health(HealthError),
    Resource(ResourceError),
    Cost(CostError),
    Condition(ConditionError),
    Inconsistent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RebuildOutcome {
    pub canceled: Vec<IntentionId>,
    pub died: bool,
    pub conditions_cleared: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveTimeOutcome {
    pub canceled: Vec<IntentionId>,
    pub died: bool,
    pub removed_conditions: Vec<crate::conditions::ConditionInstance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageOutcome {
    pub applied: u32,
    pub died: bool,
    pub canceled: Vec<IntentionId>,
}

/// Transient item-catalog sources, reconstructed from equipped item identities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EquipmentSource {
    pub item: crate::ItemId,
    pub socket: crate::EquipmentSlotId,
    pub modifiers: Vec<Modifier>,
    pub attack: Option<crate::attacks::MeleeAttack>,
}

pub(super) fn equipment_sources(
    equipment: &[EquipmentSource],
) -> Result<BTreeMap<SourceId, Vec<Modifier>>, CreatureStateError> {
    let mut result = BTreeMap::new();
    let mut sockets = BTreeSet::new();
    for entry in equipment {
        if entry.item.0 == 0
            || entry.item.0 == u64::MAX
            || usize::from(entry.socket.0) >= crate::limits::EQUIPMENT_SOCKETS
            || !sockets.insert(entry.socket)
            || result
                .insert(SourceId::Equipment(entry.item), entry.modifiers.clone())
                .is_some()
        {
            return Err(CreatureStateError::Inconsistent);
        }
    }
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureState {
    build: Shared<CreatureBuild>,
    derived: Shared<DerivedCreature>,
    baseline: Shared<DerivedCreature>,
    extra_sources: BTreeMap<SourceId, Vec<Modifier>>,
    equipment: Vec<EquipmentSource>,
    health: Health,
    costs: CostLedger,
    conditions: ConditionState,
}

fn condition_immune(build: &CreatureBuild, derived: &DerivedCreature, id: &ConditionId) -> bool {
    build.ruleset().condition(id).is_none_or(|definition| {
        definition.descriptors.iter().any(|descriptor| {
            derived.protection.has_immunity(&Selector::Descriptor {
                descriptor: descriptor.clone(),
            })
        })
    })
}
fn condition_sources(
    build: &CreatureBuild,
    sources: &BTreeMap<SourceId, Vec<Modifier>>,
    conditions: &ConditionState,
) -> BTreeMap<SourceId, Vec<Modifier>> {
    let mut result = sources.clone();
    result.retain(|source, _| !matches!(source, SourceId::Condition(..)));
    result.extend(conditions.sources(build.ruleset()));
    result
}

impl CreatureState {
    pub fn new(build: CreatureBuild) -> Result<Self, CreatureStateError> {
        let derived = build.derive().map_err(CreatureStateError::Build)?;
        let health = Health::new(derived.maximum_health);
        let costs = CostLedger::new(Resources::new(derived.resources, build.ruleset()));
        Ok(Self {
            build: Shared::new(build),
            baseline: Shared::new(derived.clone()),
            derived: Shared::new(derived),
            extra_sources: BTreeMap::new(),
            equipment: vec![],
            health,
            costs,
            conditions: ConditionState::default(),
        })
    }

    /// State restored independently must fit the freshly derived build. Death
    /// owns no preparations or conditions; descriptor immunity clears matching conditions.
    pub fn from_recorded(
        build: CreatureBuild,
        injury: u32,
        dead: bool,
        costs: CostLedger,
        conditions: ConditionState,
        equipment: Vec<EquipmentSource>,
    ) -> Result<Self, CreatureStateError> {
        if i64::from(injury) > crate::limits::NUMERIC {
            return Err(CreatureStateError::Inconsistent);
        }
        let extra_sources = equipment_sources(&equipment)?;
        let baseline_sources =
            condition_sources(&build, &extra_sources, &ConditionState::default());
        let baseline = super::derive::derive_with_sources(&build, &baseline_sources, &equipment)
            .map_err(CreatureStateError::Build)?;
        let extra_sources = condition_sources(&build, &extra_sources, &conditions);
        let derived = super::derive::derive_with_sources(&build, &extra_sources, &equipment)
            .map_err(CreatureStateError::Build)?;
        let health = Health::from_recorded(derived.maximum_health, injury, dead)
            .map_err(CreatureStateError::Health)?;
        if Resource::ALL
            .into_iter()
            .any(|resource| costs.resources().maximum(resource) != *derived.resources.get(resource))
            || (dead && !costs.reservations().is_empty())
            || (dead && !conditions.is_empty())
            || conditions
                .instances()
                .any(|entry| condition_immune(&build, &derived, &entry.condition))
        {
            return Err(CreatureStateError::Inconsistent);
        }
        Ok(Self {
            build: Shared::new(build),
            derived: Shared::new(derived),
            extra_sources,
            equipment,
            baseline: Shared::new(baseline),
            health,
            costs,
            conditions,
        })
    }

    pub fn build(&self) -> &CreatureBuild {
        &self.build
    }
    pub fn derived(&self) -> &DerivedCreature {
        &self.derived
    }
    pub fn health(&self) -> Health {
        self.health
    }
    pub fn costs(&self) -> &CostLedger {
        &self.costs
    }
    pub fn conditions(&self) -> &ConditionState {
        &self.conditions
    }

    /// Death releases unpaid reservations and removes conditions exactly once.
    fn clear_dead_state(&mut self) -> Vec<IntentionId> {
        let canceled: Vec<_> = self.costs.reservations().keys().copied().collect();
        for intention in &canceled {
            self.costs.cancel(*intention);
        }
        self.conditions.clear();
        self.extra_sources
            .retain(|source, _| !matches!(source, SourceId::Condition(..)));
        self.derived = self.baseline.clone();
        self.health.set_maximum(self.derived.maximum_health);
        self.costs.set_maxima(self.derived.resources);
        canceled
    }

    pub fn rebuild(&mut self, build: CreatureBuild) -> Result<RebuildOutcome, CreatureStateError> {
        let sources = condition_sources(&build, &self.extra_sources, &ConditionState::default());
        let baseline = super::derive::derive_with_sources(&build, &sources, &self.equipment)
            .map_err(CreatureStateError::Build)?;
        self.costs.set_recovery_rules(build.ruleset());
        self.build = Shared::new(build);
        self.baseline = Shared::new(baseline);
        let count = self.conditions.instances().count();
        let derived = self.reconcile_condition_sources();
        let mut outcome = self.replace_derived(derived);
        outcome.conditions_cleared |= count != self.conditions.instances().count();
        Ok(outcome)
    }

    /// Equipment definitions remain owned by the item catalog. The actor stores
    /// only these transient source contributions alongside its derived cache.
    pub fn set_equipment(
        &mut self,
        equipment: Vec<EquipmentSource>,
    ) -> Result<RebuildOutcome, CreatureStateError> {
        let sources = equipment_sources(&equipment)?;
        let baseline_sources = condition_sources(&self.build, &sources, &ConditionState::default());
        let baseline =
            super::derive::derive_with_sources(&self.build, &baseline_sources, &equipment)
                .map_err(CreatureStateError::Build)?;
        self.extra_sources = sources;
        self.equipment = equipment;
        self.baseline = Shared::new(baseline);
        let count = self.conditions.instances().count();
        let derived = self.reconcile_condition_sources();
        let mut outcome = self.replace_derived(derived);
        outcome.conditions_cleared |= count != self.conditions.instances().count();
        Ok(outcome)
    }

    /// A condition may mask an otherwise invalid combination of other timed
    /// sources. After expiry/immunity, evict oldest remaining applications until
    /// the fold succeeds. The separately validated permanent baseline bounds this
    /// process by the condition capacity and guarantees a valid final cache.
    fn reconcile_condition_sources(&mut self) -> DerivedCreature {
        loop {
            self.extra_sources =
                condition_sources(&self.build, &self.extra_sources, &self.conditions);
            if self.conditions.is_empty() {
                return (*self.baseline).clone();
            }
            if let Ok(derived) = super::derive::derive_with_sources(
                &self.build,
                &self.extra_sources,
                &self.equipment,
            ) {
                if !self
                    .conditions
                    .retain(|id| !condition_immune(&self.build, &derived, id))
                {
                    return derived;
                }
            } else {
                self.conditions.evict_oldest();
            }
        }
    }

    fn replace_derived(&mut self, derived: DerivedCreature) -> RebuildOutcome {
        let alive = !self.health.dead();
        let before_conditions = self.conditions.instances().count();
        let removed = self
            .conditions
            .retain(|id| !condition_immune(&self.build, &derived, id));
        let mut derived = if removed {
            self.reconcile_condition_sources()
        } else {
            derived
        };
        self.health.set_maximum(derived.maximum_health);
        let canceled = if self.health.dead() {
            let canceled = self.clear_dead_state();
            derived = (*self.baseline).clone();
            self.health.set_maximum(derived.maximum_health);
            self.costs.set_maxima(derived.resources);
            canceled
        } else {
            self.costs.set_maxima(derived.resources)
        };
        self.derived = Shared::new(derived);
        RebuildOutcome {
            canceled,
            died: alive && self.health.dead(),
            conditions_cleared: before_conditions != self.conditions.instances().count(),
        }
    }

    pub fn damage(&mut self, amount: u32) -> DamageOutcome {
        let alive = !self.health.dead();
        let applied = self.health.damage(amount);
        let died = alive && self.health.dead();
        let canceled = if died {
            self.clear_dead_state()
        } else {
            vec![]
        };
        DamageOutcome {
            applied,
            died,
            canceled,
        }
    }
    pub fn restore_resource(&mut self, resource: Resource, amount: u32) -> u32 {
        if self.health.dead() {
            0
        } else {
            self.costs.restore_resource(resource, amount)
        }
    }
    pub fn heal(&mut self, amount: u32) -> u32 {
        self.health.heal(amount)
    }
    pub fn kill(&mut self) -> DamageOutcome {
        let died = !self.health.dead();
        self.health.kill();
        let canceled = if died {
            self.clear_dead_state()
        } else {
            vec![]
        };
        DamageOutcome {
            applied: 0,
            died,
            canceled,
        }
    }
    pub fn start_cost(
        &mut self,
        intention: IntentionId,
        cost: ResourceCost,
    ) -> Result<StartCost, CostError> {
        if self.health.dead() {
            return Err(CostError::Invalid);
        }
        self.costs.start(intention, cost)
    }

    /// Preparation checks resources without changing the actor. Queue admission
    /// does not call the spending primitive; validation is repeated at execution.
    pub fn validate_cost(
        &self,
        intention: IntentionId,
        cost: ResourceCost,
    ) -> Result<StartCost, CostError> {
        if self.health.dead() {
            return Err(CostError::Invalid);
        }
        self.costs.validate_start(intention, cost)
    }
    pub fn finish_cost(&mut self, intention: IntentionId) -> Result<ResourceCost, CostError> {
        self.costs.finish(intention)
    }
    pub fn cancel_cost(&mut self, intention: IntentionId) -> Option<ResourceCost> {
        self.costs.cancel(intention)
    }
    pub fn apply_condition(
        &mut self,
        id: &ConditionId,
        source: ActorId,
        duration: u64,
    ) -> Result<ConditionUpdate, ConditionError> {
        let definition = self
            .build
            .ruleset()
            .condition(id)
            .ok_or(ConditionError::UnknownCondition)?;
        let mut conditions = self.conditions.clone();
        let update = conditions.apply(
            definition,
            source,
            duration,
            self.health.dead() || condition_immune(&self.build, &self.derived, id),
        )?;
        let sources = condition_sources(&self.build, &self.extra_sources, &conditions);
        let derived = super::derive::derive_with_sources(&self.build, &sources, &self.equipment)
            .map_err(|_| ConditionError::InvalidModifiers)?;
        self.conditions = conditions;
        self.extra_sources = sources;
        self.replace_derived(derived);
        Ok(update)
    }
    /// Whether active elapsed time can change this actor's mutable state.
    pub fn needs_active_time(&self) -> bool {
        self.next_change_in().is_some()
    }

    /// Earliest observable timer change in active simulation ticks. The loop
    /// converts this relative interval into a deadline on the actor's clock;
    /// frozen actors must not consume it until they become active again.
    pub fn next_change_in(&self) -> Option<u64> {
        if self.health.dead() {
            return None;
        }
        self.costs
            .resources()
            .next_recovery_in()
            .into_iter()
            .chain(self.conditions.next_expiry_in())
            .min()
    }

    /// The scheduler supplies active ticks only. Dead actors never regenerate.
    pub fn advance_active(&mut self, mut ticks: u64) -> ActiveTimeOutcome {
        let alive = !self.health.dead();
        let held: Vec<_> = self.costs.reservations().keys().copied().collect();
        let conditions: Vec<_> = self.conditions.instances().cloned().collect();
        while ticks > 0 && !self.health.dead() {
            let interval = self.conditions.next_expiry_in().unwrap_or(ticks).min(ticks);
            self.costs.advance_active(interval);
            ticks -= interval;
            if self.conditions.advance_active(interval) {
                let derived = self.reconcile_condition_sources();
                self.replace_derived(derived);
            }
        }
        ActiveTimeOutcome {
            canceled: held
                .into_iter()
                .filter(|id| self.costs.reservation(*id).is_none())
                .collect(),
            died: alive && self.health.dead(),
            removed_conditions: conditions
                .into_iter()
                .filter(|old| {
                    !self
                        .conditions
                        .instances()
                        .any(|entry| entry.condition == old.condition && entry.source == old.source)
                })
                .collect(),
        }
    }
}

impl serde::Serialize for CreatureState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        super::record::CreatureRecord::capture(self).serialize(serializer)
    }
}
