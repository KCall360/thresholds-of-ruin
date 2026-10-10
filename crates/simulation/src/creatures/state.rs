//! Actor-owned creature state. Rebuilds validate first, then reconcile mutable
//! state once; callers cannot edit a build or derived cache behind that boundary.

use super::{BuildError, CreatureBuild, DerivedCreature};
use crate::costs::{CostError, CostLedger, ResourceCost, StartCost};
use crate::fear::{FearError, FearState, FearUpdate};
use crate::grants::{Descriptor, Grant, Selector};
use crate::health::{Health, HealthError};
use crate::resources::{Resource, ResourceError, Resources};
use crate::{ActorId, IntentionId};
use tor_world::Shared;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureStateError {
    Build(BuildError),
    Health(HealthError),
    Resource(ResourceError),
    Cost(CostError),
    Fear(FearError),
    Inconsistent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RebuildOutcome {
    pub canceled: Vec<IntentionId>,
    pub died: bool,
    pub fear_cleared: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DamageOutcome {
    pub applied: u32,
    pub died: bool,
    pub canceled: Vec<IntentionId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreatureState {
    build: Shared<CreatureBuild>,
    derived: Shared<DerivedCreature>,
    health: Health,
    costs: CostLedger,
    fear: FearState,
}

fn fear_immune(derived: &DerivedCreature) -> bool {
    derived.grants.values().flatten().any(|grant| {
        matches!(
            grant,
            Grant::Immunity(Selector::Descriptor(
                Descriptor::Fear | Descriptor::MindAffecting
            ))
        )
    })
}

impl CreatureState {
    pub fn new(build: CreatureBuild) -> Result<Self, CreatureStateError> {
        let derived = build.derive().map_err(CreatureStateError::Build)?;
        let health = Health::new(derived.maximum_health);
        let costs = CostLedger::new(Resources::new(derived.resources));
        Ok(Self {
            build: Shared::new(build),
            derived: Shared::new(derived),
            health,
            costs,
            fear: FearState::default(),
        })
    }

    /// State restored independently must fit the freshly derived build. Death
    /// owns no preparations or fear; immunity cannot coexist with active fear.
    pub fn from_recorded(
        build: CreatureBuild,
        injury: u32,
        dead: bool,
        costs: CostLedger,
        fear: FearState,
    ) -> Result<Self, CreatureStateError> {
        if injury > 1_000_000 {
            return Err(CreatureStateError::Inconsistent);
        }
        let derived = build.derive().map_err(CreatureStateError::Build)?;
        let health = Health::from_recorded(derived.maximum_health, injury, dead)
            .map_err(CreatureStateError::Health)?;
        let maxima = [
            derived.resources.stamina,
            derived.resources.focus,
            derived.resources.mana,
        ];
        if Resource::ALL
            .into_iter()
            .zip(maxima)
            .any(|(resource, maximum)| costs.resources().maximum(resource) != maximum)
            || (dead && !costs.reservations().is_empty())
            || ((dead || fear_immune(&derived)) && !fear.sources().is_empty())
        {
            return Err(CreatureStateError::Inconsistent);
        }
        Ok(Self {
            build: Shared::new(build),
            derived: Shared::new(derived),
            health,
            costs,
            fear,
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
    pub fn fear(&self) -> &FearState {
        &self.fear
    }

    /// Death releases unpaid reservations and removes conditions exactly once.
    fn clear_dead_state(&mut self) -> Vec<IntentionId> {
        let canceled: Vec<_> = self.costs.reservations().keys().copied().collect();
        for intention in &canceled {
            self.costs.cancel(*intention);
        }
        self.fear.reconcile_immunity(true);
        canceled
    }

    pub fn rebuild(&mut self, build: CreatureBuild) -> Result<RebuildOutcome, CreatureStateError> {
        let derived = build.derive().map_err(CreatureStateError::Build)?;
        let alive = !self.health.dead();
        let had_fear = !self.fear.sources().is_empty();
        self.health.set_maximum(derived.maximum_health);
        let canceled = if self.health.dead() {
            // Cancel before clamping so the result has every released hold,
            // including holds that a capacity reduction would otherwise remove.
            let canceled = self.clear_dead_state();
            self.costs.set_maxima(derived.resources);
            canceled
        } else {
            self.fear.reconcile_immunity(fear_immune(&derived));
            self.costs.set_maxima(derived.resources)
        };
        self.build = Shared::new(build);
        self.derived = Shared::new(derived);
        Ok(RebuildOutcome {
            canceled,
            died: alive && self.health.dead(),
            fear_cleared: had_fear && self.fear.sources().is_empty(),
        })
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
    pub fn apply_fear(&mut self, source: ActorId, duration: u64) -> Result<FearUpdate, FearError> {
        self.fear.apply(
            source,
            duration,
            self.health.dead() || fear_immune(&self.derived),
        )
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
            .chain(self.fear.next_expiry_in())
            .min()
    }

    /// The scheduler supplies active ticks only. Dead actors never regenerate.
    pub fn advance_active(&mut self, ticks: u64) {
        if self.health.dead() {
            return;
        }
        self.costs.advance_active(ticks);
        self.fear.advance_active(ticks);
    }
}

impl serde::Serialize for CreatureState {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        super::record::CreatureRecord::capture(self).serialize(serializer)
    }
}
impl<'de> serde::Deserialize<'de> for CreatureState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        super::record::CreatureRecord::deserialize(deserializer)?
            .restore()
            .map_err(|error| D::Error::custom(format!("invalid creature state: {error:?}")))
    }
}
