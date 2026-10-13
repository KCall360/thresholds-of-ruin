//! Validated action plans, pure effect resolution, and runtime orchestration.
mod compile;
mod execution;
mod plan;
mod plan_validation;
mod resolve;
mod strikes;

pub(crate) use plan::item_effects_match_definition;
pub use plan::{
    AbilityPlan, CheckThreshold, Plan, PlannedAmount, PlannedCheck, PlannedEffect, RuleError,
};
pub use resolve::{
    resolve, Outcome, OutcomeStatus, ResolutionPlan, ResolvedCheck, ResolvedEffect, Snapshot,
};
pub use strikes::{
    CheckPhase, CheckSide, CheckSite, FrozenRider, FrozenStrike, StrikeOrigin, TokenClaim,
};

#[cfg(test)]
mod strike_tests;
#[cfg(test)]
mod tests;
