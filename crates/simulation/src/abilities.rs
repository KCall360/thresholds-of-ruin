//! Validated action plans, pure effect resolution, and runtime orchestration.
mod execution;
mod plan;
mod resolve;

pub(crate) use plan::item_effects_match_definition;
pub use plan::{
    AbilityPlan, CheckThreshold, Plan, PlannedAmount, PlannedCheck, PlannedEffect, RuleError,
};
pub use resolve::{resolve, Outcome, OutcomeStatus, ResolutionPlan, ResolvedEffect, Snapshot};

#[cfg(test)]
mod tests;
