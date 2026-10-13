//! Validated action plans, pure effect resolution, and runtime orchestration.
mod compile;
mod execution;
mod participation;
mod plan;
mod plan_validation;
mod resolve;
mod strikes;

pub use participation::{
    AttemptedParticipation, CheckParticipation, PayloadParticipation, UnselectedReason,
};
pub(crate) use plan::item_effects_match_definition;
pub use plan::{
    AbilityPlan, CheckThreshold, Plan, PlannedAmount, PlannedCheck, PlannedEffect, RuleError,
};
pub(crate) use resolve::ActorEffectState;
pub use resolve::{
    resolve, ApplicationSnapshot, EffectRecipient, Outcome, OutcomeStatus, ResolutionContext,
    ResolutionPlan, ResolvedCheck, ResolvedEffect, Snapshot,
};
pub use strikes::{
    CheckPhase, CheckSide, CheckSite, FrozenRider, FrozenStrike, StrikeOrigin, TokenClaim,
};

#[cfg(test)]
mod strike_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod participation_tests;
