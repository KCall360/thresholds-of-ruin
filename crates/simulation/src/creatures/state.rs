//! Actor-owned creature state. Rebuilds validate first, then reconcile mutable
//! state once; callers cannot edit a build or derived cache behind that boundary.

use super::{
    BuildEpoch, BuildError, CreatureBuild, DerivedCreature, RestorableLossOutcome,
    RestorableLosses, SourceId,
};
use crate::conditions::{ConditionError, ConditionState, ConditionUpdate};
use crate::content_ids::{AbilityId, ConditionId};
use crate::cooldowns::{CooldownError, Cooldowns};
use crate::costs::{CostError, CostLedger, ResourceCost, StartCost};
use crate::health::{Health, HealthError};
use crate::health_recovery::{HealthRecovery, RecoveryContext, RecoveryNutrition};
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
    Growth(crate::earned_growth::GrowthError),
    HitDieRestoration(super::HitDieRestorationBlocker),
    Inconsistent,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RebuildOutcome {
    pub canceled: Vec<IntentionId>,
    pub died: bool,
    pub conditions_cleared: bool,
}
impl RebuildOutcome {
    pub(crate) fn merge(&mut self, other: Self) {
        for owner in other.canceled {
            if !self.canceled.contains(&owner) {
                self.canceled.push(owner);
            }
        }
        self.died |= other.died;
        self.conditions_cleared |= other.conditions_cleared;
    }
}

#[derive(Clone, Copy)]
enum ConditionReconciliation {
    Explicit,
    AutomaticGrowth,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActiveTimeOutcome {
    pub canceled: Vec<IntentionId>,
    pub died: bool,
    pub removed_conditions: Vec<crate::conditions::ConditionInstance>,
    pub regenerated: crate::nutrition::Regeneration,
    pub nutrition_spent: u32,
    /// Exact assessed components before the existing combined starvation clamp.
    pub nutrition_time_demand: u64,
    pub nutrition_regeneration_demand: u64,
    /// Living actor-active ticks actually processed, excluding a post-death suffix.
    pub active_ticks: u64,
    /// An observation overflow invalidates diagnostics without changing gameplay.
    pub diagnostics_incomplete: bool,
    pub(crate) empty_time_depletion: bool,
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
    earned_progress: crate::earned_growth::EarnedProgress,
    restorable_losses: RestorableLosses,
    /// Shared immutable permanent compilation; effective sheets never seed it.
    permanent: Shared<super::derive::PermanentCreature>,
    derived: Shared<DerivedCreature>,
    baseline: Shared<DerivedCreature>,
    extra_sources: BTreeMap<SourceId, Vec<Modifier>>,
    equipment: Vec<EquipmentSource>,
    health: Health,
    health_recovery: HealthRecovery,
    recovery_context: RecoveryContext,
    nutrition: crate::nutrition::Nutrition,
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
        let permanent = Shared::new(
            super::derive::PermanentCreature::compile(&build).map_err(CreatureStateError::Build)?,
        );
        let derived = permanent
            .fold(&build, &BTreeMap::new(), &[], RestorableLosses::default())
            .map_err(CreatureStateError::Build)?;
        let health = Health::new(derived.maximum_health);
        let health_recovery = HealthRecovery::new(derived.quiet_health_recovery_period);
        let costs = CostLedger::new(Resources::new(derived.resources, build.ruleset()));
        let threat = permanent.threat_tier();
        let earned_progress =
            crate::earned_growth::EarnedProgress::new(build.ledger().total_hd(), threat);
        Ok(Self {
            build: Shared::new(build),
            permanent,
            build_epoch: BuildEpoch::INITIAL,
            earned_progress,
            restorable_losses: RestorableLosses::default(),
            baseline: Shared::new(derived.clone()),
            derived: Shared::new(derived),
            extra_sources: BTreeMap::new(),
            equipment: vec![],
            health,
            health_recovery,
            recovery_context: RecoveryContext::Known {
                sensed_hostile: false,
            },
            nutrition: crate::nutrition::Nutrition::Exempt,
            received_health_loss_remaining: 0,
            costs,
            conditions: ConditionState::default(),
            cooldowns: Cooldowns::default(),
        })
    }

    /// State restored independently must fit the freshly derived build. Death
    /// owns no preparations or conditions; descriptor immunity clears matching conditions.
    #[allow(clippy::too_many_arguments)]
    pub fn from_recorded(
        build: CreatureBuild,
        build_epoch: BuildEpoch,
        restorable_losses: RestorableLosses,
        injury: u32,
        dead: bool,
        costs: CostLedger,
        conditions: ConditionState,
        equipment: Vec<EquipmentSource>,
    ) -> Result<Self, CreatureStateError> {
        Self::from_recorded_with_costs(
            build,
            build_epoch,
            restorable_losses,
            injury,
            dead,
            conditions,
            equipment,
            |_, _| Ok(costs),
        )
    }

    /// Decode mutable costs against the same current fold that validates the
    /// restored state. Derived values never cross the public restore boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn from_recorded_with_costs(
        build: CreatureBuild,
        build_epoch: BuildEpoch,
        restorable_losses: RestorableLosses,
        injury: u32,
        dead: bool,
        conditions: ConditionState,
        equipment: Vec<EquipmentSource>,
        restore_costs: impl FnOnce(
            crate::resources::ResourceMaxima,
            &crate::rules::Ruleset,
        ) -> Result<CostLedger, CreatureStateError>,
    ) -> Result<Self, CreatureStateError> {
        if !restorable_losses.valid() || i64::from(injury) > crate::limits::NUMERIC {
            return Err(CreatureStateError::Inconsistent);
        }
        let permanent = Shared::new(
            super::derive::PermanentCreature::compile(&build).map_err(CreatureStateError::Build)?,
        );
        let extra_sources = equipment_sources(&equipment)?;
        let baseline_sources =
            condition_sources(&build, &extra_sources, &ConditionState::default());
        let baseline = Shared::new(
            permanent
                .fold(&build, &baseline_sources, &equipment, restorable_losses)
                .map_err(CreatureStateError::Build)?,
        );
        let extra_sources = condition_sources(&build, &extra_sources, &conditions);
        let derived = if conditions.is_empty() {
            baseline.clone()
        } else {
            Shared::new(
                permanent
                    .fold(&build, &extra_sources, &equipment, restorable_losses)
                    .map_err(CreatureStateError::Build)?,
            )
        };
        let costs = restore_costs(derived.resources, build.ruleset())?;
        let health = Health::from_recorded(derived.maximum_health, injury, dead)
            .map_err(CreatureStateError::Health)?;
        let health_recovery = HealthRecovery::new(derived.quiet_health_recovery_period);
        let threat = permanent.threat_tier();
        let earned_progress =
            crate::earned_growth::EarnedProgress::new(build.ledger().total_hd(), threat);
        if Resource::ALL
            .into_iter()
            .any(|resource| costs.resources().maximum(resource) != *derived.resources.get(resource))
            || (dead && !costs.reservations().is_empty())
            || (dead && !conditions.is_empty())
            || conditions.instances().any(|entry| {
                entry.phase == crate::conditions::ConditionPhase::Active
                    && entry.reserved_by.is_none()
                    && condition_immune(&build, &derived, &entry.condition)
            })
        {
            return Err(CreatureStateError::Inconsistent);
        }
        Ok(Self {
            build: Shared::new(build),
            permanent,
            build_epoch,
            earned_progress,
            restorable_losses,
            derived,
            extra_sources,
            equipment,
            baseline,
            health,
            health_recovery,
            recovery_context: RecoveryContext::Known {
                sensed_hostile: false,
            },
            nutrition: crate::nutrition::Nutrition::Exempt,
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
    pub fn earned_progress(&self) -> &crate::earned_growth::EarnedProgress {
        &self.earned_progress
    }
    pub(crate) fn replace_earned_progress(
        &mut self,
        progress: crate::earned_growth::EarnedProgress,
    ) -> Result<(), CreatureStateError> {
        progress
            .validate(
                self.build.ledger().total_hd(),
                &self.build.ruleset().document().growth,
            )
            .map_err(CreatureStateError::Growth)?;
        let current_threat = self.permanent.threat_tier();
        if progress.peak_threat() < current_threat {
            return Err(CreatureStateError::Inconsistent);
        }
        self.earned_progress = progress;
        Ok(())
    }
    pub fn derived(&self) -> &DerivedCreature {
        &self.derived
    }
    pub fn health(&self) -> Health {
        self.health
    }
    pub fn restorable_losses(&self) -> RestorableLosses {
        self.restorable_losses
    }

    pub fn restorable_hit_dice(&self) -> u16 {
        self.earned_progress.highest_applied_tier() - self.derived.total_hd()
    }

    fn restoration_source(&self) -> Result<crate::progression::HdSource, CreatureStateError> {
        if let super::RestorationPolicy::Authored(plan) = self.build.restoration_policy() {
            if let Some(ordinal) = plan.get(usize::from(self.derived.total_hd())) {
                return Ok(ordinal.source.clone());
            }
        }
        match self.build.future_growth() {
            crate::earned_growth::FutureGrowth::Selected { source } => Ok(source.hd_source()),
            _ => Err(CreatureStateError::HitDieRestoration(
                super::HitDieRestorationBlocker::MissingSource,
            )),
        }
    }

    pub fn hit_die_restoration_blocker(&self) -> Option<super::HitDieRestorationBlocker> {
        if self.health.dead() || self.restorable_hit_dice() == 0 {
            return None;
        }
        if self.restoration_source().is_err() {
            Some(super::HitDieRestorationBlocker::MissingSource)
        } else if self.build.ledger().health_root_seed().is_none() {
            Some(super::HitDieRestorationBlocker::MissingHealthRoot)
        } else {
            None
        }
    }

    /// Drain the latest actual ordinal and its purchases. The shared rebuild
    /// owns permanent gates, resource reconciliation and sticky zero-HD death.
    pub fn drain_latest_hit_die(&mut self) -> Result<RestorableLossOutcome, CreatureStateError> {
        self.change_hit_die(false, false)
    }

    /// Restore at most one previously applied ordinal, never a funded-but-blocked
    /// tier. A ChooseAgain slot uses the explicitly selected future source.
    pub fn restore_hit_die(&mut self) -> Result<RestorableLossOutcome, CreatureStateError> {
        self.change_hit_die(true, false)
    }

    pub(crate) fn stage_hit_die_effect(
        &mut self,
        restore: bool,
    ) -> Result<RestorableLossOutcome, CreatureStateError> {
        self.change_hit_die(restore, true)
    }

    fn change_hit_die(
        &mut self,
        restore: bool,
        staged: bool,
    ) -> Result<RestorableLossOutcome, CreatureStateError> {
        if self.health.dead() || (restore && self.restorable_hit_dice() == 0) {
            return Ok(RestorableLossOutcome::default());
        }
        let mut build = (*self.build).clone();
        if restore {
            if let Some(blocker) = self.hit_die_restoration_blocker() {
                return Err(CreatureStateError::HitDieRestoration(blocker));
            }
            let source = self.restoration_source()?;
            let root =
                build
                    .ledger()
                    .health_root_seed()
                    .ok_or(CreatureStateError::HitDieRestoration(
                        super::HitDieRestorationBlocker::MissingHealthRoot,
                    ))?;
            build
                .add_hit_die(source, root)
                .map_err(CreatureStateError::Build)?;
        } else if build.remove_latest().is_none() {
            return Ok(RestorableLossOutcome::default());
        }
        let mut candidate = self.clone();
        let prior_context = self.recovery_context;
        let had_context = self.derived.contextual_health_recovery();
        let base_period = self.health_recovery.base_period();
        let mut rebuild = candidate.rebuild_one_with_recovery_context(
            build,
            ConditionReconciliation::AutomaticGrowth,
            || {
                Ok(if staged && !had_context {
                    // Pure resolution cannot invent perception. Actor finalization
                    // resolves this lazily, or retains Pending on a frozen floor.
                    RecoveryContext::Pending { base_period }
                } else {
                    prior_context
                })
            },
        )?;
        if !staged {
            candidate.apply_held_growth(&mut rebuild)?;
        }
        *self = candidate;
        Ok(RestorableLossOutcome {
            applied: 1,
            rebuild,
        })
    }

    /// These ordinary effects alter one fixed aggregate. Their caller owns
    /// Game finalization, including held growth and fatal-effect attribution.
    pub fn lose_attribute(
        &mut self,
        attribute: crate::attributes::Attribute,
        amount: u32,
    ) -> Result<RestorableLossOutcome, CreatureStateError> {
        self.validate_loss_amount(amount)?;
        let mut losses = self.restorable_losses;
        let prior = *losses.attributes.get(attribute);
        let applied = amount
            .min(u32::from(self.derived.attributes.get(attribute)))
            .min(u32::from(crate::limits::DERIVED_ATTRIBUTE - prior));
        *losses.attributes.get_mut(attribute) = prior + applied as u16;
        self.change_restorable_losses(losses, applied)
    }

    pub fn restore_attribute(
        &mut self,
        attribute: crate::attributes::Attribute,
        amount: u32,
    ) -> Result<RestorableLossOutcome, CreatureStateError> {
        self.validate_loss_amount(amount)?;
        let mut losses = self.restorable_losses;
        let prior = *losses.attributes.get(attribute);
        let applied = amount.min(u32::from(prior));
        *losses.attributes.get_mut(attribute) = prior - applied as u16;
        self.change_restorable_losses(losses, applied)
    }

    pub fn lose_maximum_health(
        &mut self,
        amount: u32,
    ) -> Result<RestorableLossOutcome, CreatureStateError> {
        self.validate_loss_amount(amount)?;
        let mut losses = self.restorable_losses;
        let applied = amount
            .min(self.derived.maximum_health)
            .min(crate::limits::NUMERIC as u32 - losses.maximum_health);
        losses.maximum_health += applied;
        self.change_restorable_losses(losses, applied)
    }

    pub fn restore_maximum_health(
        &mut self,
        amount: u32,
    ) -> Result<RestorableLossOutcome, CreatureStateError> {
        self.validate_loss_amount(amount)?;
        let mut losses = self.restorable_losses;
        let applied = amount.min(losses.maximum_health);
        losses.maximum_health -= applied;
        self.change_restorable_losses(losses, applied)
    }

    fn validate_loss_amount(&self, amount: u32) -> Result<(), CreatureStateError> {
        if i64::from(amount) > crate::limits::NUMERIC || !self.restorable_losses.valid() {
            return Err(CreatureStateError::Inconsistent);
        }
        Ok(())
    }

    fn change_restorable_losses(
        &mut self,
        losses: RestorableLosses,
        applied: u32,
    ) -> Result<RestorableLossOutcome, CreatureStateError> {
        if self.health.dead() || applied == 0 {
            return Ok(RestorableLossOutcome::default());
        }
        let mut candidate = self.clone();
        let baseline_sources = condition_sources(
            &candidate.build,
            &candidate.extra_sources,
            &ConditionState::default(),
        );
        let baseline = candidate
            .permanent
            .fold(
                &candidate.build,
                &baseline_sources,
                &candidate.equipment,
                losses,
            )
            .map_err(CreatureStateError::Build)?;
        let derived = if candidate.conditions.is_empty() {
            baseline.clone()
        } else {
            // A loss cannot silently evict a valid condition to conceal an
            // invalid fold. Reject the complete effect before publication.
            candidate
                .permanent
                .fold(
                    &candidate.build,
                    &candidate.extra_sources,
                    &candidate.equipment,
                    losses,
                )
                .map_err(CreatureStateError::Build)?
        };
        candidate.restorable_losses = losses;
        candidate.baseline = Shared::new(baseline);
        let rebuild = candidate.replace_derived(derived);
        *self = candidate;
        Ok(RestorableLossOutcome { applied, rebuild })
    }

    pub fn nutrition(&self) -> crate::nutrition::Nutrition {
        self.nutrition
    }
    pub fn configure_nutrition(
        &mut self,
        requirement: crate::nutrition::NutritionRequirement,
    ) -> Result<(), CreatureStateError> {
        if self.health.dead() || self.nutrition != crate::nutrition::Nutrition::Exempt {
            return Err(CreatureStateError::Inconsistent);
        }
        let policy = self.build.ruleset().document().nutrition;
        let next = crate::nutrition::Nutrition::initial(requirement, policy)
            .ok_or(CreatureStateError::Inconsistent)?;
        let mut recovery = self.health_recovery;
        if let Some(nutrition) = next.recovery(policy) {
            recovery
                .set_nutrition(nutrition)
                .map_err(CreatureStateError::Health)?;
        }
        // Commit both candidates only after every fallible preparation succeeds.
        self.nutrition = next;
        self.health_recovery = recovery;
        Ok(())
    }
    pub(crate) fn restore_nutrition(
        &mut self,
        nutrition: crate::nutrition::Nutrition,
    ) -> Result<(), CreatureStateError> {
        let policy = self.build.ruleset().document().nutrition;
        if !nutrition.valid(policy, self.health.dead())
            || nutrition
                .recovery(policy)
                .is_some_and(|required| required != self.health_recovery.nutrition())
        {
            return Err(CreatureStateError::Inconsistent);
        }
        self.nutrition = nutrition;
        Ok(())
    }
    pub(crate) fn nourish(&mut self, amount: u32) -> u32 {
        if self.health.dead() {
            return 0;
        }
        let policy = self.build.ruleset().document().nutrition;
        let applied = self.nutrition.nourish(policy, amount);
        if let Some(nutrition) = self.nutrition.recovery(policy) {
            self.health_recovery
                .set_nutrition(nutrition)
                .expect("validated nutrition policy");
        }
        applied
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
        context: RecoveryContext,
    ) -> Result<(), CreatureStateError> {
        let period = match context {
            RecoveryContext::Known { sensed_hostile } => {
                if sensed_hostile
                    && (!self.derived.contextual_health_recovery() || self.health.dead())
                {
                    return Err(CreatureStateError::Inconsistent);
                }
                self.derived.recovery_period_in_context(sensed_hostile)
            }
            RecoveryContext::Pending { base_period } => {
                if self.health.dead() || !self.derived.contextual_health_recovery() {
                    return Err(CreatureStateError::Inconsistent);
                }
                base_period
            }
        };
        self.health_recovery =
            HealthRecovery::from_recorded(period, elapsed, nutrition, self.health)
                .map_err(CreatureStateError::Health)?;
        self.recovery_context = context;
        Ok(())
    }
    /// Caller settles prior active time before changing the nutrition policy.
    pub fn set_recovery_nutrition(
        &mut self,
        nutrition: RecoveryNutrition,
    ) -> Result<(), HealthError> {
        if self
            .nutrition
            .recovery(self.build.ruleset().document().nutrition)
            .is_some_and(|required| required != nutrition)
        {
            return Err(HealthError::InvalidState);
        }
        self.health_recovery.set_nutrition(nutrition)
    }
    pub(crate) fn recovery_context(&self) -> RecoveryContext {
        self.recovery_context
    }
    /// Pending clocks are legal only at a settled frozen actor boundary.
    pub(crate) fn recovery_context_valid_at(
        &self,
        frozen_at: Option<u64>,
        active_since: u64,
    ) -> bool {
        match self.recovery_context {
            RecoveryContext::Known { sensed_hostile } => {
                !sensed_hostile
                    || (!self.health.dead() && self.derived.contextual_health_recovery())
            }
            RecoveryContext::Pending { base_period } => {
                !self.health.dead()
                    && self.derived.contextual_health_recovery()
                    && frozen_at == Some(active_since)
                    && (1..=crate::limits::NUMERIC as u64).contains(&base_period)
                    && self.health_recovery.base_period() == base_period
            }
        }
    }
    /// The caller settles prior actor-active time before this one context transition.
    pub(crate) fn set_recovery_context(&mut self, sensed_hostile: bool) {
        self.apply_recovery_context(RecoveryContext::Known { sensed_hostile });
    }
    fn apply_recovery_context(&mut self, context: RecoveryContext) {
        self.recovery_context = if self.health.dead() || !self.derived.contextual_health_recovery()
        {
            RecoveryContext::Known {
                sensed_hostile: false,
            }
        } else {
            context
        };
        if let RecoveryContext::Known { sensed_hostile } = self.recovery_context {
            self.health_recovery
                .set_period(self.derived.recovery_period_in_context(sensed_hostile));
        }
    }
    /// Seed the recipient's actual context before one ordinary source mutation.
    /// A pending frozen clock remains unchanged through every contextual fold.
    pub(crate) fn with_recovery_context<R, E>(
        &mut self,
        context: RecoveryContext,
        apply: impl FnOnce(&mut Self) -> Result<R, E>,
    ) -> Result<R, E> {
        let before_context = self.recovery_context;
        let before_clock = self.health_recovery;
        self.recovery_context = context;
        match apply(self) {
            Ok(value) => {
                self.apply_recovery_context(context);
                Ok(value)
            }
            Err(error) => {
                self.recovery_context = before_context;
                self.health_recovery = before_clock;
                Err(error)
            }
        }
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
    pub(crate) fn condition_horizon(&self) -> Option<u64> {
        self.conditions.last_boundary_in()
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
        self.nutrition.clear_dead_fraction();
        self.extra_sources.retain(|source, _| {
            !matches!(
                source,
                SourceId::Condition(..) | SourceId::QualifiedCondition { .. }
            )
        });
        self.derived = self.baseline.clone();
        self.recovery_context = RecoveryContext::Known {
            sensed_hostile: false,
        };
        self.health.set_maximum(self.derived.maximum_health);
        self.health_recovery
            .set_period(self.derived.recovery_period_in_context(false));
        self.health_recovery.clear_if_saturated(self.health);
        self.costs.set_maxima(self.derived.resources);
        canceled
    }

    /// Preserve runtime nutrition enrollment. Required nutrition retains its
    /// exact policy; changing that policy requires a separate migration contract.
    pub fn rebuild(&mut self, build: CreatureBuild) -> Result<RebuildOutcome, CreatureStateError> {
        let mut candidate = self.clone();
        let mut outcome = candidate.stage_rebuild(build)?;
        candidate.apply_held_growth(&mut outcome)?;
        *self = candidate;
        Ok(outcome)
    }

    /// Game candidates converge once at actor finalization. Standalone callers
    /// use `rebuild`, which includes immediate growth before returning.
    pub(crate) fn stage_rebuild(
        &mut self,
        build: CreatureBuild,
    ) -> Result<RebuildOutcome, CreatureStateError> {
        self.rebuild_one(build, ConditionReconciliation::Explicit)
    }

    /// Actual XP delivery changes one candidate creature and never resurrects a
    /// dead owner. Dead sources retain their earned balance for exact recovery.
    /// Kernel tests use this immediate helper; Game owns production finalization.
    #[cfg(test)]
    pub(crate) fn award_xp(&mut self, amount: u32) -> Result<(), CreatureStateError> {
        let mut candidate = self.clone();
        candidate.stage_xp_award(amount)?;
        let mut outcome = RebuildOutcome::default();
        candidate.apply_held_growth(&mut outcome)?;
        *self = candidate;
        Ok(())
    }

    /// Credit alone is atomic; the owning Game candidate finalizer applies any
    /// eligible growth and its lifecycle consequences before publication.
    pub(crate) fn stage_xp_award(&mut self, amount: u32) -> Result<(), CreatureStateError> {
        self.earned_progress
            .award(
                amount,
                self.build.ledger().total_hd(),
                &self.build.ruleset().document().growth,
            )
            .map_err(CreatureStateError::Growth)
    }

    fn growth_pending(&self) -> bool {
        !self.health.dead()
            && self.build.ledger().health_root_seed().is_some()
            && usize::from(self.build.ledger().total_hd()) < crate::limits::HIT_DICE
            && self.earned_progress.held_xp()
                >= u64::from(
                    self.earned_progress
                        .next_threshold(&self.build.ruleset().document().growth),
                )
            && match self.build.future_growth() {
                crate::earned_growth::FutureGrowth::Disabled {} => false,
                crate::earned_growth::FutureGrowth::Selected { .. } => true,
                crate::earned_growth::FutureGrowth::Authored {} => {
                    matches!(self.build.restoration_policy(),
                    super::RestorationPolicy::Authored(plan) if plan.get(usize::from(self.build.ledger().total_hd())).is_some())
                }
            }
    }

    /// Converge after new XP or an authoritative source fold changed.
    /// Timer values, damage and reads do not retry an unchanged obstruction.
    pub(crate) fn retry_growth_after_fold(
        &mut self,
        before: &Self,
        mut context: impl FnMut(u64) -> Result<RecoveryContext, CreatureStateError>,
    ) -> Result<RebuildOutcome, CreatureStateError> {
        let mut outcome = RebuildOutcome::default();
        let pending_context = matches!(self.recovery_context, RecoveryContext::Pending { .. })
            && self.recovery_context != before.recovery_context;
        if pending_context
            || (self.growth_pending()
                && (self.earned_progress.earned_xp() != before.earned_progress.earned_xp()
                    || self.build_epoch != before.build_epoch
                    || self.extra_sources != before.extra_sources
                    || self.equipment != before.equipment
                    || self.restorable_losses != before.restorable_losses))
        {
            let mut candidate = self.clone();
            if pending_context {
                let RecoveryContext::Pending { base_period } = candidate.recovery_context else {
                    unreachable!("checked pending context")
                };
                candidate.apply_recovery_context(context(base_period)?);
            }
            candidate.apply_held_growth_with_recovery_context(&mut outcome, &mut context)?;
            *self = candidate;
        }
        Ok(outcome)
    }

    fn apply_held_growth(
        &mut self,
        outcome: &mut RebuildOutcome,
    ) -> Result<(), CreatureStateError> {
        self.apply_held_growth_with_recovery_context(outcome, &mut |_| {
            Ok(RecoveryContext::Known {
                sensed_hostile: false,
            })
        })
    }

    fn apply_held_growth_with_recovery_context(
        &mut self,
        outcome: &mut RebuildOutcome,
        context: &mut impl FnMut(u64) -> Result<RecoveryContext, CreatureStateError>,
    ) -> Result<(), CreatureStateError> {
        if !self.growth_pending() {
            return Ok(());
        }
        loop {
            let current = self.build.ledger().total_hd();
            if usize::from(current) >= crate::limits::HIT_DICE
                || self.earned_progress.held_xp()
                    < u64::from(
                        self.earned_progress
                            .next_threshold(&self.build.ruleset().document().growth),
                    )
            {
                break;
            }
            let intended = match self.build.restoration_policy() {
                super::RestorationPolicy::Authored(plan) => plan
                    .get(usize::from(current))
                    .map(|entry| entry.source.clone()),
                super::RestorationPolicy::ChooseAgain => None,
            };
            let source = match self.build.future_growth() {
                crate::earned_growth::FutureGrowth::Disabled {} => break,
                crate::earned_growth::FutureGrowth::Selected { source } => {
                    intended.unwrap_or_else(|| source.hd_source())
                }
                crate::earned_growth::FutureGrowth::Authored {} => {
                    let Some(source) = intended else {
                        break;
                    };
                    source
                }
            };
            let Some(root) = self.build.ledger().health_root_seed() else {
                break;
            };
            let mut build = (*self.build).clone();
            // A temporarily inadmissible authored choice retains the whole XP
            // balance. No retained ordinal or Health seed is changed on failure.
            if build.add_hit_die(source, root).is_err() {
                break;
            }
            let mut next = self.clone();
            next.earned_progress
                .spend(current + 1, &build.ruleset().document().growth)
                .map_err(CreatureStateError::Growth)?;
            let prior_context = next.recovery_context;
            let had_context = next.derived.contextual_health_recovery();
            let base_period = next.health_recovery.base_period();
            let change = match next.rebuild_one_with_recovery_context(
                build,
                ConditionReconciliation::AutomaticGrowth,
                || {
                    if had_context {
                        Ok(prior_context)
                    } else {
                        context(base_period)
                    }
                },
            ) {
                Ok(change) => change,
                // Valid temporary source combinations may be incompatible with
                // the next permanent HD. Keep its XP and exact Health ordinal.
                Err(CreatureStateError::Build(
                    BuildError::DerivedLimit | BuildError::ConflictingOverride,
                )) => break,
                Err(error) => return Err(error),
            };
            *self = next;
            outcome.merge(change);
            if self.health.dead() {
                break;
            }
        }
        Ok(())
    }

    pub(crate) fn stage_rebuild_with_recovery_context(
        &mut self,
        build: CreatureBuild,
        context: impl FnOnce() -> Result<RecoveryContext, CreatureStateError>,
    ) -> Result<RebuildOutcome, CreatureStateError> {
        self.rebuild_one_with_recovery_context(build, ConditionReconciliation::Explicit, context)
    }

    fn rebuild_one(
        &mut self,
        build: CreatureBuild,
        conditions: ConditionReconciliation,
    ) -> Result<RebuildOutcome, CreatureStateError> {
        let context = self.recovery_context;
        self.rebuild_one_with_recovery_context(build, conditions, || Ok(context))
    }

    fn rebuild_one_with_recovery_context(
        &mut self,
        build: CreatureBuild,
        conditions: ConditionReconciliation,
        context: impl FnOnce() -> Result<RecoveryContext, CreatureStateError>,
    ) -> Result<RebuildOutcome, CreatureStateError> {
        // Nutrition enrollment and both immutable policies survive every rebuild.
        if matches!(self.nutrition, crate::nutrition::Nutrition::Required { .. })
            && (self.build.ruleset().document().nutrition != build.ruleset().document().nutrition
                || self.build.ruleset().document().nutrition_condition_trigger
                    != build.ruleset().document().nutrition_condition_trigger)
        {
            return Err(CreatureStateError::Inconsistent);
        }
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
        if self.build.same_permanent_inputs(&build) {
            // Intent/provenance-only edits still invalidate paid build epochs,
            // but leave the current sheet and all runtime bookkeeping intact.
            self.build = Shared::new(build);
            self.build_epoch = epoch;
            return Ok(RebuildOutcome::default());
        }
        self.cooldowns
            .validate_ruleset(build.ruleset())
            .map_err(CreatureStateError::Cooldown)?;
        let permanent = Shared::new(
            super::derive::PermanentCreature::compile(&build).map_err(CreatureStateError::Build)?,
        );
        let sources = condition_sources(&build, &self.extra_sources, &ConditionState::default());
        let baseline = permanent
            .fold(&build, &sources, &self.equipment, self.restorable_losses)
            .map_err(CreatureStateError::Build)?;
        let mut earned_progress = self.earned_progress.clone();
        let threat = permanent.threat_tier();
        earned_progress
            .reconcile_build(
                build.ledger().total_hd(),
                threat,
                &build.ruleset().document().growth,
            )
            .map_err(CreatureStateError::Growth)?;
        let count = self.conditions.instances().count();
        self.conditions
            .reconcile_definitions(self.build.ruleset(), build.ruleset())
            .map_err(|_| CreatureStateError::Inconsistent)?;
        self.costs.set_recovery_rules(build.ruleset());
        self.build = Shared::new(build);
        self.permanent = permanent;
        self.build_epoch = epoch;
        self.earned_progress = earned_progress;
        self.baseline = Shared::new(baseline);
        let derived = match conditions {
            ConditionReconciliation::Explicit => self.reconcile_condition_sources_with(true)?,
            ConditionReconciliation::AutomaticGrowth => self.reconcile_growth_conditions()?,
        };
        self.recovery_context = if derived.contextual_health_recovery() {
            context()?
        } else {
            RecoveryContext::Known {
                sensed_hostile: false,
            }
        };
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
        // The combined condition fold can fail after source assignments. Keep
        // those assignments private even when no automatic growth is pending.
        let mut candidate = self.clone();
        let mut outcome = candidate.set_equipment_one(equipment)?;
        candidate.apply_held_growth(&mut outcome)?;
        *self = candidate;
        Ok(outcome)
    }

    fn set_equipment_one(
        &mut self,
        equipment: Vec<EquipmentSource>,
    ) -> Result<RebuildOutcome, CreatureStateError> {
        let sources = equipment_sources(&equipment)?;
        if equipment == self.equipment {
            return Ok(RebuildOutcome::default());
        }
        let baseline_sources = condition_sources(&self.build, &sources, &ConditionState::default());
        let baseline = self
            .permanent
            .fold(
                &self.build,
                &baseline_sources,
                &equipment,
                self.restorable_losses,
            )
            .map_err(CreatureStateError::Build)?;
        self.extra_sources = sources;
        self.equipment = equipment;
        self.baseline = Shared::new(baseline);
        let count = self.conditions.instances().count();
        let derived = self.reconcile_condition_sources_with(true)?;
        let mut outcome = self.replace_derived(derived);
        outcome.conditions_cleared |= count != self.conditions.instances().count();
        Ok(outcome)
    }

    /// Owned Game candidate equipment fold, without a second growth owner.
    pub(crate) fn stage_equipment(
        &mut self,
        equipment: Vec<EquipmentSource>,
    ) -> Result<RebuildOutcome, CreatureStateError> {
        self.set_equipment_one(equipment)
    }

    /// Automatic growth cannot buy its next HD by evicting a valid temporary
    /// source. Only genuine eligibility/immunity removals are allowed. Any
    /// incompatible combined fold rejects this whole owned growth candidate.
    fn reconcile_growth_conditions(&mut self) -> Result<DerivedCreature, CreatureStateError> {
        self.conditions
            .reconcile_immunity(self.build.ruleset(), |id| {
                condition_immune(&self.build, &self.baseline, id)
            });
        loop {
            self.extra_sources =
                condition_sources(&self.build, &self.extra_sources, &self.conditions);
            if self.conditions.is_empty() {
                return Ok((*self.baseline).clone());
            }
            let derived = self
                .permanent
                .fold(
                    &self.build,
                    &self.extra_sources,
                    &self.equipment,
                    self.restorable_losses,
                )
                .map_err(CreatureStateError::Build)?;
            if !self
                .conditions
                .reconcile_immunity(self.build.ruleset(), |id| {
                    condition_immune(&self.build, &derived, id)
                })
            {
                return Ok(derived);
            }
        }
    }

    /// A condition may mask an otherwise invalid combination of other timed
    /// sources. After expiry/immunity, evict oldest remaining applications until
    /// the fold succeeds. The separately validated condition-free baseline bounds this
    /// process by the condition capacity and guarantees a valid final cache.
    fn reconcile_condition_sources(&mut self) -> DerivedCreature {
        self.reconcile_condition_sources_with(false)
            .expect("expiry reconciliation never rejects a fold")
    }

    /// Explicit candidate changes may not erase a condition to conceal a new
    /// interaction contradiction. Expiry still reconciles residual sources, and
    /// ordinary soft modifier conflicts retain the bounded eviction policy.
    fn reconcile_condition_sources_with(
        &mut self,
        reject_interaction: bool,
    ) -> Result<DerivedCreature, CreatureStateError> {
        loop {
            self.conditions
                .reconcile_immunity(self.build.ruleset(), |id| {
                    condition_immune(&self.build, &self.baseline, id)
                });
            self.extra_sources =
                condition_sources(&self.build, &self.extra_sources, &self.conditions);
            if self.conditions.is_empty() {
                return Ok((*self.baseline).clone());
            }
            match self.permanent.fold(
                &self.build,
                &self.extra_sources,
                &self.equipment,
                self.restorable_losses,
            ) {
                Ok(derived) => {
                    if !self
                        .conditions
                        .reconcile_immunity(self.build.ruleset(), |id| {
                            condition_immune(&self.build, &derived, id)
                        })
                    {
                        return Ok(derived);
                    }
                }
                Err(BuildError::InvalidInteraction) if reject_interaction => {
                    return Err(CreatureStateError::Build(BuildError::InvalidInteraction));
                }
                Err(_) => {
                    if !self
                        .conditions
                        .evict_for_reconciliation(self.build.ruleset())
                    {
                        // Protected entries and reserved tombstones cannot modify
                        // the independently validated condition-free baseline.
                        return Ok((*self.baseline).clone());
                    }
                }
            }
        }
    }

    fn replace_derived(&mut self, derived: DerivedCreature) -> RebuildOutcome {
        let alive = !self.health.dead();
        let before_conditions = self.conditions.instances().count();
        let removed = self
            .conditions
            .reconcile_immunity(self.build.ruleset(), |id| {
                condition_immune(&self.build, &derived, id)
            });
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
        self.apply_recovery_context(self.recovery_context);
        self.health_recovery.clear_if_saturated(self.health);
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
    pub fn condition_restricts(&self, operation: crate::rules::ConditionRestriction) -> bool {
        self.conditions.restricts(self.build.ruleset(), operation)
    }
    pub(crate) fn condition_restricts_after(
        &self,
        operation: crate::rules::ConditionRestriction,
        elapsed: u64,
    ) -> bool {
        self.conditions
            .restricts_after(self.build.ruleset(), operation, elapsed)
    }
    pub(crate) fn condition_pauses_preparation(&self, action: &crate::Action) -> bool {
        self.conditions
            .pauses_preparation(self.build.ruleset(), action)
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
        if sources == self.extra_sources {
            self.conditions = conditions;
            return Ok(update);
        }
        let derived = self
            .permanent
            .fold(
                &self.build,
                &sources,
                &self.equipment,
                self.restorable_losses,
            )
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
            if sources == self.extra_sources {
                self.conditions = conditions;
                return Ok(());
            }
            let derived = self
                .permanent
                .fold(
                    &self.build,
                    &sources,
                    &self.equipment,
                    self.restorable_losses,
                )
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

    /// Random depletion work belongs to the existing actor deadline index.
    /// Denial removes only this deadline; condition expiry and digestion continue.
    fn next_nutrition_trigger_in(&self) -> Option<u64> {
        let document = self.build.ruleset().document();
        if !document.nutrition_condition_trigger.enabled()
            || self.condition_restricts(crate::rules::ConditionRestriction::VoluntaryActions)
        {
            return None;
        }
        self.nutrition.next_empty_depletion_in(document.nutrition)
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
            .chain(
                self.nutrition
                    .next_change_in(self.build.ruleset().document().nutrition),
            )
            .chain(self.next_nutrition_trigger_in())
            .chain(self.cooldowns.next_expiry_in())
            .chain(
                (self.received_health_loss_remaining > 0)
                    .then_some(self.received_health_loss_remaining),
            )
            .min()
    }

    /// The scheduler supplies active ticks only. Dead actors never regenerate.
    pub fn advance_active(&mut self, ticks: u64) -> ActiveTimeOutcome {
        self.advance_active_with_endpoint_context(ticks, None)
    }

    /// External actor-active boundaries join condition and nutrition changes at
    /// the final tick, rephasing the earned fraction only once.
    pub(crate) fn advance_active_with_endpoint_context(
        &mut self,
        mut ticks: u64,
        endpoint_context: Option<bool>,
    ) -> ActiveTimeOutcome {
        if ticks == 0 {
            if let Some(sensed_hostile) = endpoint_context {
                self.set_recovery_context(sensed_hostile);
            }
        }
        let alive = !self.health.dead();
        let held: Vec<_> = self.costs.reservations().keys().copied().collect();
        let conditions: Vec<_> = self.conditions.instances().cloned().collect();
        let mut regenerated = crate::nutrition::Regeneration::default();
        let nutrition_before = self.nutrition;
        let mut diagnostics_incomplete = false;
        let mut active_ticks = 0;
        let mut nutrition_time_demand = 0;
        let mut nutrition_regeneration_demand = 0;
        let mut empty_time_depletion = false;
        while ticks > 0 && !self.health.dead() {
            let policy = self.build.ruleset().document().nutrition;
            let mut interval = self
                .conditions
                .next_expiry_in()
                .into_iter()
                .chain(self.nutrition.next_change_in(policy))
                .chain(std::iter::once(ticks))
                .min()
                .expect("remaining interval");
            let mut gained = self.regeneration_after(interval);
            if self.nutrition.crosses_change(policy, interval, gained) {
                // Monotone exact integer projection finds the earliest crossing.
                // At most 64 probes per band/expiry, never one per tick or point.
                let mut low = 1;
                let mut high = interval;
                while low < high {
                    let middle = low + (high - low) / 2;
                    if self.nutrition.crosses_change(
                        policy,
                        middle,
                        self.regeneration_after(middle),
                    ) {
                        high = middle;
                    } else {
                        low = middle + 1;
                    }
                }
                interval = low;
                gained = self.regeneration_after(interval);
            }
            if let (
                crate::nutrition::Nutrition::Required {
                    elapsed,
                    regeneration_remainder,
                    ..
                },
                crate::nutrition::NutritionPolicy::Enabled {
                    unit_ticks,
                    regeneration_cost,
                    ..
                },
            ) = (self.nutrition, policy)
            {
                let time = (u128::from(elapsed) + u128::from(interval)) / u128::from(unit_ticks);
                let time = u64::try_from(time).unwrap_or_else(|_| {
                    diagnostics_incomplete = true;
                    u64::MAX
                });
                crate::survival_diagnostics::checked_add(
                    &mut nutrition_time_demand,
                    time,
                    &mut diagnostics_incomplete,
                );
                // Validated rates and all capped pool gains fit this widened numerator.
                let regeneration = (u64::from(regeneration_remainder)
                    + regeneration_cost.numerator(gained))
                    / u64::from(regeneration_cost.denominator);
                crate::survival_diagnostics::checked_add(
                    &mut nutrition_regeneration_demand,
                    regeneration,
                    &mut diagnostics_incomplete,
                );
            }
            crate::survival_diagnostics::checked_add(
                &mut active_ticks,
                interval,
                &mut diagnostics_incomplete,
            );
            regenerated.add(gained);
            self.costs.advance_active(interval);
            self.health_recovery
                .advance_active(&mut self.health, interval);
            self.cooldowns.advance_active(interval);
            self.received_health_loss_remaining =
                self.received_health_loss_remaining.saturating_sub(interval);
            ticks -= interval;
            // Keep work earned under the old combined denominator. A condition
            // expiry and hunger change at this tick are one final rephase.
            let mut settled_recovery = self.health_recovery;
            let mut time_only = self.nutrition;
            time_only.advance_active(policy, interval);
            empty_time_depletion = time_only.at_empty_depletion();
            let starved = self
                .nutrition
                .advance_with_regeneration(policy, interval, gained);
            if self.conditions.advance_active(interval)
                && condition_sources(&self.build, &self.extra_sources, &self.conditions)
                    != self.extra_sources
            {
                let derived = self.reconcile_condition_sources();
                self.replace_derived(derived);
            }
            let context = endpoint_context.filter(|_| ticks == 0);
            if let Some(sensed_hostile) = context {
                self.set_recovery_context(sensed_hostile);
            }
            let nutrition = self
                .nutrition
                .recovery(policy)
                .unwrap_or(settled_recovery.nutrition());
            if self.health.dead() {
                self.health_recovery
                    .set_nutrition(nutrition)
                    .expect("validated nutrition policy");
            } else if nutrition != settled_recovery.nutrition() || context.is_some() {
                settled_recovery.rephase(self.health_recovery.base_period(), nutrition);
                settled_recovery.clear_if_saturated(self.health);
                self.health_recovery = settled_recovery;
            }
            if starved && !self.health.dead() {
                // The shared injury/death primitive has no attacker. The actor
                // lifecycle later publishes ordinary death with no FatalCause.
                self.damage(self.health.current());
            }
        }
        ActiveTimeOutcome {
            diagnostics_incomplete,
            active_ticks,
            nutrition_time_demand,
            nutrition_regeneration_demand,
            regenerated,
            empty_time_depletion,
            nutrition_spent: match (nutrition_before, self.nutrition) {
                (
                    crate::nutrition::Nutrition::Required {
                        reserve: before, ..
                    },
                    crate::nutrition::Nutrition::Required { reserve: after, .. },
                ) => (before - after) as u32,
                _ => 0,
            },
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

    fn regeneration_after(&self, ticks: u64) -> crate::nutrition::Regeneration {
        crate::nutrition::Regeneration {
            health: self.health_recovery.regeneration_after(self.health, ticks),
            resources: self.costs.resources().regeneration_after(ticks),
        }
    }
}

impl serde::Serialize for CreatureState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        super::record::CreatureRecord::capture(self).serialize(serializer)
    }
}

#[cfg(test)]
mod compiled_tests {
    use super::*;
    use crate::attributes::{Attribute, Attributes, ByAttribute, ManaBinding, Skill};
    use crate::rules::{ModifierOp, Scope, Stat};

    fn reference(creature: &CreatureState) {
        let fresh = super::super::derive::derive_with_losses(
            creature.build(),
            &creature.extra_sources,
            &creature.equipment,
            creature.restorable_losses,
        )
        .unwrap();
        assert_eq!(creature.derived(), &fresh);
    }

    #[test]
    fn cached_dynamic_folds_match_fresh_records_without_double_applying_sources() {
        let mut species = crate::test_creatures::species();
        species.default_attributes = Attributes::new(ByAttribute {
            strength: 2,
            ..ByAttribute::default()
        })
        .unwrap();
        let mut creature = CreatureState::new(crate::test_creatures::build(species)).unwrap();
        let permanent = creature.permanent.clone();
        let initial = creature.derived().maximum_health;
        for set in [false, true] {
            creature
                .set_equipment(vec![EquipmentSource {
                    item: crate::ItemId(7),
                    socket: crate::EquipmentSlotId(0),
                    attack: Some(crate::test_creatures::melee(
                        3,
                        70,
                        crate::combat::DamageType::Impact,
                        5,
                    )),
                    modifiers: vec![
                        Modifier {
                            scope: Scope::Always,
                            op: if set {
                                ModifierOp::Set {
                                    stat: Stat::Health,
                                    value: 100,
                                }
                            } else {
                                ModifierOp::Add {
                                    stat: Stat::Health,
                                    value: 7,
                                }
                            },
                        },
                        Modifier {
                            scope: Scope::Always,
                            op: ModifierOp::Add {
                                stat: Stat::Attribute {
                                    attribute: Attribute::Strength,
                                },
                                value: 3,
                            },
                        },
                    ],
                }])
                .unwrap();
            assert_eq!(
                creature.derived().maximum_health,
                if set { 100 } else { initial + 10 }
            );
            reference(&creature);
            creature
                .apply_condition(&ConditionId::new("fear").unwrap(), ActorId(9), 30)
                .unwrap();
            reference(&creature);
            creature.lose_attribute(Attribute::Strength, 1).unwrap();
            creature.lose_maximum_health(2).unwrap();
            assert_eq!(
                creature.derived().maximum_health,
                if set { 98 } else { initial + 7 }
            );
            reference(&creature);
            creature.advance_active(30);
            reference(&creature);
            creature.restore_attribute(Attribute::Strength, 1).unwrap();
            creature.restore_maximum_health(2).unwrap();
            reference(&creature);
            assert!(creature.permanent.shares_storage(&permanent));
        }
    }

    #[test]
    fn every_statistical_input_invalidates_the_permanent_compilation() {
        let base = crate::test_creatures::build(crate::test_creatures::species());
        let mut variants = Vec::new();
        let mut binding = base.clone();
        binding.binding = ManaBinding::Presence;
        variants.push(binding);
        let mut attributes = base.clone();
        attributes
            .set_initial_attributes(
                Attributes::new(ByAttribute {
                    strength: 3,
                    ..ByAttribute::default()
                })
                .unwrap(),
            )
            .unwrap();
        variants.push(attributes);
        let mut starting = base.clone();
        starting.advancement.starting_skills = [Skill::Lore, Skill::Medicine];
        variants.push(starting);
        let mut rules = base.ruleset().document().clone();
        rules.constants.base_defense += 1;
        let mut changed_rules = base.clone();
        changed_rules.content = super::super::Content::new(
            crate::rules::Ruleset::validate(rules).unwrap(),
            base.content().catalog().clone(),
        )
        .unwrap();
        variants.push(changed_rules);
        let mut catalog = base.content().catalog().clone();
        let mut species = base.species().clone();
        species.id = crate::content_ids::SpeciesId::new("another_subject").unwrap();
        species.behavior = crate::behavior::BehaviorPolicy {
            disposition: crate::behavior::Disposition::Mean,
            idle: crate::behavior::IdleBehavior::Explore,
            memory_ticks: 1234,
        };
        species.displacement = Some(super::super::DisplacementProfile {
            size: super::super::CreatureSize::Huge,
            anchored: true,
        });
        let id = species.id.clone();
        species
            .modifiers
            .push(crate::test_creatures::add(Stat::Health, 4));
        catalog.species.insert(id.clone(), species);
        let mut changed_species = base.clone();
        changed_species.content =
            super::super::Content::new(base.content().rules().clone(), catalog).unwrap();
        changed_species.species = id;
        variants.push(changed_species);
        for changed in variants {
            let mut creature = CreatureState::new(base.clone()).unwrap();
            let permanent = creature.permanent.clone();
            let expected = changed.derive().unwrap();
            let before = crate::diagnostics::work_counts();
            creature.rebuild(changed).unwrap();
            assert_eq!(
                crate::diagnostics::work_counts().permanent_creature_compiles
                    - before.permanent_creature_compiles,
                1
            );
            assert!(
                crate::diagnostics::work_counts().raw_species_lookups > before.raw_species_lookups
            );
            assert!(!creature.permanent.shares_storage(&permanent));
            assert_eq!(creature.derived(), &expected);
        }
    }

    #[test]
    fn changed_token_qualification_refolds_but_timer_refresh_does_not() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let mut creature = CreatureState::new(build).unwrap();
        let id = ConditionId::new("readied").unwrap();
        let qualify = |opponent| {
            Some(crate::conditions::TokenQualification {
                opponent: ActorId(opponent),
                ability_tag: crate::content_ids::AbilityTagId::new("weapon_attack").unwrap(),
            })
        };
        creature
            .apply_qualified_condition(&id, ActorId(9), 20, qualify(2))
            .unwrap();
        let before = crate::diagnostics::work_counts();
        creature
            .apply_qualified_condition(&id, ActorId(9), 30, qualify(2))
            .unwrap();
        assert_eq!(
            crate::diagnostics::work_counts().dynamic_creature_folds,
            before.dynamic_creature_folds
        );
        creature
            .apply_qualified_condition(&id, ActorId(9), 40, qualify(3))
            .unwrap();
        assert_eq!(
            crate::diagnostics::work_counts().dynamic_creature_folds
                - before.dynamic_creature_folds,
            1
        );
        assert_eq!(
            crate::diagnostics::work_counts().permanent_creature_compiles,
            before.permanent_creature_compiles
        );
        assert_eq!(
            crate::diagnostics::work_counts().health_derivations,
            before.health_derivations
        );
        reference(&creature);
    }
}
