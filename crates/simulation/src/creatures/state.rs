//! Actor-owned creature state. Rebuilds validate first, then reconcile mutable
//! state once; callers cannot edit a build or derived cache behind that boundary.

use super::{BuildEpoch, BuildError, CreatureBuild, DerivedCreature, SourceId};
use crate::conditions::{ConditionError, ConditionState, ConditionUpdate};
use crate::content_ids::{AbilityId, ConditionId};
use crate::cooldowns::{CooldownError, Cooldowns};
use crate::costs::{CostError, CostLedger, ResourceCost, StartCost};
use crate::health::{Health, HealthError};
use crate::health_recovery::{HealthRecovery, RecoveryNutrition};
use crate::resources::{Resource, ResourceError, Resources};
use crate::rules::{Modifier, Selector};
use crate::{ActorId, IntentionId};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::Shared;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureStateError {
    Build(BuildError),
    BuildEpochExhausted,
    Health(HealthError),
    Resource(ResourceError),
    Cost(CostError),
    Condition(ConditionError),
    Cooldown(CooldownError),
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
    build_epoch: BuildEpoch,
    derived: Shared<DerivedCreature>,
    baseline: Shared<DerivedCreature>,
    extra_sources: BTreeMap<SourceId, Vec<Modifier>>,
    equipment: Vec<EquipmentSource>,
    health: Health,
    health_recovery: HealthRecovery,
    received_health_loss_remaining: u64,
    costs: CostLedger,
    conditions: ConditionState,
    cooldowns: Cooldowns,
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
    result.retain(|source, _| {
        !matches!(
            source,
            SourceId::Condition(..) | SourceId::QualifiedCondition { .. }
        )
    });
    result.extend(conditions.sources(build.ruleset()));
    result
}

impl CreatureState {
    pub fn new(build: CreatureBuild) -> Result<Self, CreatureStateError> {
        let derived = build.derive().map_err(CreatureStateError::Build)?;
        let health = Health::new(derived.maximum_health);
        let health_recovery = HealthRecovery::new(derived.health_recovery_period);
        let costs = CostLedger::new(Resources::new(derived.resources, build.ruleset()));
        Ok(Self {
            build: Shared::new(build),
            build_epoch: BuildEpoch::INITIAL,
            baseline: Shared::new(derived.clone()),
            derived: Shared::new(derived),
            extra_sources: BTreeMap::new(),
            equipment: vec![],
            health,
            health_recovery,
            received_health_loss_remaining: 0,
            costs,
            conditions: ConditionState::default(),
            cooldowns: Cooldowns::default(),
        })
    }

    /// State restored independently must fit the freshly derived build. Death
    /// owns no preparations or conditions; descriptor immunity clears matching conditions.
    pub fn from_recorded(
        build: CreatureBuild,
        build_epoch: BuildEpoch,
        injury: u32,
        dead: bool,
        costs: CostLedger,
        conditions: ConditionState,
        equipment: Vec<EquipmentSource>,
    ) -> Result<Self, CreatureStateError> {
        Self::from_recorded_with_costs(
            build,
            build_epoch,
            injury,
            dead,
            conditions,
            equipment,
            |_, _| Ok(costs),
        )
    }

    /// Decode mutable costs against the same current fold that validates the
    /// restored state. Derived values never cross the public restore boundary.
    pub(super) fn from_recorded_with_costs(
        build: CreatureBuild,
        build_epoch: BuildEpoch,
        injury: u32,
        dead: bool,
        conditions: ConditionState,
        equipment: Vec<EquipmentSource>,
        restore_costs: impl FnOnce(
            crate::resources::ResourceMaxima,
            &crate::rules::Ruleset,
        ) -> Result<CostLedger, CreatureStateError>,
    ) -> Result<Self, CreatureStateError> {
        if i64::from(injury) > crate::limits::NUMERIC {
            return Err(CreatureStateError::Inconsistent);
        }
        let extra_sources = equipment_sources(&equipment)?;
        let baseline_sources =
            condition_sources(&build, &extra_sources, &ConditionState::default());
        let baseline = Shared::new(
            super::derive::derive_with_sources(&build, &baseline_sources, &equipment)
                .map_err(CreatureStateError::Build)?,
        );
        let extra_sources = condition_sources(&build, &extra_sources, &conditions);
        let derived = if conditions.is_empty() {
            baseline.clone()
        } else {
            Shared::new(
                super::derive::derive_with_sources(&build, &extra_sources, &equipment)
                    .map_err(CreatureStateError::Build)?,
            )
        };
        let costs = restore_costs(derived.resources, build.ruleset())?;
        let health = Health::from_recorded(derived.maximum_health, injury, dead)
            .map_err(CreatureStateError::Health)?;
        let health_recovery = HealthRecovery::new(derived.health_recovery_period);
        if Resource::ALL
            .into_iter()
            .any(|resource| costs.resources().maximum(resource) != *derived.resources.get(resource))
            || (dead && !costs.reservations().is_empty())
            || (dead && !conditions.is_empty())
            || conditions.instances().any(|entry| {
                entry.reserved_by.is_none() && condition_immune(&build, &derived, &entry.condition)
            })
        {
            return Err(CreatureStateError::Inconsistent);
        }
        Ok(Self {
            build: Shared::new(build),
            build_epoch,
            derived,
            extra_sources,
            equipment,
            baseline,
            health,
            health_recovery,
            received_health_loss_remaining: 0,
            costs,
            conditions,
            cooldowns: Cooldowns::default(),
        })
    }

    pub fn build(&self) -> &CreatureBuild {
        &self.build
    }
    pub fn build_epoch(&self) -> BuildEpoch {
        self.build_epoch
    }
    pub fn derived(&self) -> &DerivedCreature {
        &self.derived
    }
    pub fn health(&self) -> Health {
        self.health
    }
    pub fn health_recovery(&self) -> HealthRecovery {
        self.health_recovery
    }
    /// Active quiet time still required after actual positive received damage.
    /// Healing, dealing damage and maximum changes never clear or renew it.
    pub fn received_health_loss_remaining(&self) -> u64 {
        self.received_health_loss_remaining
    }
    pub(crate) fn restore_received_health_loss(
        &mut self,
        remaining: u64,
    ) -> Result<(), CreatureStateError> {
        if remaining
            > self
                .build
                .ruleset()
                .document()
                .advancement
                .received_loss_interval
        {
            return Err(CreatureStateError::Inconsistent);
        }
        self.received_health_loss_remaining = remaining;
        Ok(())
    }
    pub(crate) fn restore_health_recovery(
        &mut self,
        elapsed: u64,
        nutrition: RecoveryNutrition,
    ) -> Result<(), CreatureStateError> {
        self.health_recovery = HealthRecovery::from_recorded(
            self.derived.health_recovery_period,
            elapsed,
            nutrition,
            self.health,
        )
        .map_err(CreatureStateError::Health)?;
        Ok(())
    }
    /// Caller settles prior active time before changing the nutrition policy.
    pub fn set_recovery_nutrition(
        &mut self,
        nutrition: RecoveryNutrition,
    ) -> Result<(), HealthError> {
        self.health_recovery.set_nutrition(nutrition)
    }
    /// Called once per resolved damage outcome, for each affected actor.
    pub(crate) fn damage_activity(&mut self) {
        self.health_recovery
            .damage_activity(self.derived.health_recovery_reset_percent);
    }
    pub fn costs(&self) -> &CostLedger {
        &self.costs
    }
    pub fn conditions(&self) -> &ConditionState {
        &self.conditions
    }

    pub fn cooldowns(&self) -> &Cooldowns {
        &self.cooldowns
    }

    pub(crate) fn restore_cooldowns(
        &mut self,
        rows: Vec<(AbilityId, u64)>,
    ) -> Result<(), CreatureStateError> {
        self.cooldowns = Cooldowns::from_recorded(rows, self.build.ruleset())
            .map_err(CreatureStateError::Cooldown)?;
        Ok(())
    }

    /// Read-only cooldown gate. Grants, costs and atomic preparation are checked
    /// by the action layer; a retained entry need not remain currently granted.
    pub fn validate_cooldown(
        &self,
        ability: &AbilityId,
        duration: u64,
    ) -> Result<(), CooldownError> {
        if self.health.dead() {
            return Err(CooldownError::Dead);
        }
        self.cooldowns
            .validate_start(ability, duration, self.build.ruleset())
            .map(|_| ())
    }

    /// Start one frozen interval after settling the actor's previous active
    /// time. Calling again while cooling down rejects without renewal.
    pub fn start_cooldown(
        &mut self,
        ability: &AbilityId,
        duration: u64,
    ) -> Result<(), CooldownError> {
        if self.health.dead() {
            return Err(CooldownError::Dead);
        }
        self.cooldowns
            .start(ability, duration, self.build.ruleset())
    }

    /// Death releases unpaid reservations and removes conditions exactly once.
    fn clear_dead_state(&mut self) -> Vec<IntentionId> {
        let canceled: Vec<_> = self.costs.reservations().keys().copied().collect();
        for intention in &canceled {
            self.costs.cancel(*intention);
        }
        self.conditions.clear();
        self.extra_sources.retain(|source, _| {
            !matches!(
                source,
                SourceId::Condition(..) | SourceId::QualifiedCondition { .. }
            )
        });
        self.derived = self.baseline.clone();
        self.health.set_maximum(self.derived.maximum_health);
        self.health_recovery
            .set_period(self.derived.health_recovery_period);
        self.health_recovery.clear_if_saturated(self.health);
        self.costs.set_maxima(self.derived.resources);
        canceled
    }

    pub fn rebuild(&mut self, build: CreatureBuild) -> Result<RebuildOutcome, CreatureStateError> {
        if self.received_health_loss_remaining
            > build
                .ruleset()
                .document()
                .advancement
                .received_loss_interval
        {
            return Err(CreatureStateError::Inconsistent);
        }
        let epoch = if *self.build == build {
            self.build_epoch
        } else {
            self.build_epoch
                .next()
                .ok_or(CreatureStateError::BuildEpochExhausted)?
        };
        self.cooldowns
            .validate_ruleset(build.ruleset())
            .map_err(CreatureStateError::Cooldown)?;
        let sources = condition_sources(&build, &self.extra_sources, &ConditionState::default());
        let baseline = super::derive::derive_with_sources(&build, &sources, &self.equipment)
            .map_err(CreatureStateError::Build)?;
        self.costs.set_recovery_rules(build.ruleset());
        self.build = Shared::new(build);
        self.build_epoch = epoch;
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
        self.health_recovery
            .set_period(derived.health_recovery_period);
        self.health_recovery.clear_if_saturated(self.health);
        self.derived = Shared::new(derived);
        RebuildOutcome {
            canceled,
            died: alive && self.health.dead(),
            conditions_cleared: before_conditions != self.conditions.instances().count(),
        }
    }

    pub fn damage(&mut self, amount: u32) -> DamageOutcome {
        let outcome = self.damage_without_reset(amount);
        if outcome.applied > 0 {
            self.damage_activity();
        }
        outcome
    }
    /// Resolved effect bundles reset quiet once at their publication boundary.
    pub(crate) fn damage_without_reset(&mut self, amount: u32) -> DamageOutcome {
        let alive = !self.health.dead();
        let applied = self.health.damage(amount);
        if applied > 0 {
            self.received_health_loss_remaining = self
                .build
                .ruleset()
                .document()
                .advancement
                .received_loss_interval;
        }
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
        let healed = self.health.heal(amount);
        self.health_recovery.clear_if_saturated(self.health);
        healed
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
        self.apply_qualified_condition(id, source, duration, None)
    }
    pub fn apply_qualified_condition(
        &mut self,
        id: &ConditionId,
        source: ActorId,
        duration: u64,
        qualification: Option<crate::conditions::TokenQualification>,
    ) -> Result<ConditionUpdate, ConditionError> {
        let definition = self
            .build
            .ruleset()
            .condition(id)
            .ok_or(ConditionError::UnknownCondition)?;
        if qualification
            .as_ref()
            .is_some_and(|scope| !scope.valid(definition, self.build.ruleset()))
        {
            return Err(ConditionError::InvalidQualification);
        }
        let mut conditions = self.conditions.clone();
        let update = conditions.apply_qualified(
            definition,
            source,
            duration,
            self.health.dead() || condition_immune(&self.build, &self.derived, id),
            qualification,
        )?;
        let sources = condition_sources(&self.build, &self.extra_sources, &conditions);
        let derived = super::derive::derive_with_sources(&self.build, &sources, &self.equipment)
            .map_err(|_| ConditionError::InvalidModifiers)?;
        self.conditions = conditions;
        self.extra_sources = sources;
        self.replace_derived(derived);
        Ok(update)
    }
    pub(crate) fn reserve_tokens(
        &mut self,
        tokens: &[crate::conditions::TokenReservation],
        owner: ActorId,
    ) -> Result<(), ConditionError> {
        let mut conditions = self.conditions.clone();
        let mut changed = false;
        for token in tokens {
            changed |= conditions.reserve(token, owner, self.build.ruleset())?;
        }
        if changed {
            let sources = condition_sources(&self.build, &self.extra_sources, &conditions);
            let derived =
                super::derive::derive_with_sources(&self.build, &sources, &self.equipment)
                    .map_err(|_| ConditionError::InvalidModifiers)?;
            self.conditions = conditions;
            self.extra_sources = sources;
            self.replace_derived(derived);
        }
        Ok(())
    }
    pub(crate) fn consume_tokens(
        &mut self,
        tokens: &[crate::conditions::TokenReservation],
        owner: ActorId,
    ) {
        // Reserved entries contribute no derived modifiers, so removing their
        // tombstones cannot change derived state or resource reservations.
        for token in tokens {
            self.conditions.consume_reservation(token, owner);
        }
    }

    /// Whether active elapsed time can change this actor's mutable state.
    pub fn needs_active_time(&self) -> bool {
        self.next_change_in().is_some()
            || (!self.health.dead()
                && self
                    .conditions
                    .instances()
                    .any(|entry| entry.reserved_by.is_some() && entry.remaining > 0))
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
            .chain(self.health_recovery.next_recovery_in(self.health))
            .chain(self.cooldowns.next_expiry_in())
            .chain(
                (self.received_health_loss_remaining > 0)
                    .then_some(self.received_health_loss_remaining),
            )
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
            self.health_recovery
                .advance_active(&mut self.health, interval);
            self.cooldowns.advance_active(interval);
            self.received_health_loss_remaining =
                self.received_health_loss_remaining.saturating_sub(interval);
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
