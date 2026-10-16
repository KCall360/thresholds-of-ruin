//! Validated action plans, pure effect resolution, and runtime orchestration.
mod compile;
mod execution;
mod invocation;
mod participation;
pub use invocation::{AbilityInvocation, InvocationKind, PushDirection, StepDirection};
mod plan;
mod plan_validation;
mod resolve;
mod spatial;
mod strikes;
pub use spatial::{MovementMode, MovementResult, Placement, SpatialInput, StepCommitment};

pub use participation::{
    AttemptedParticipation, CheckParticipation, PayloadParticipation, UnselectedReason,
};
pub(crate) use plan::item_effects_match_definition;
pub use plan::{
    AbilityPlan, CheckThreshold, Plan, PlannedAmount, PlannedCheck, PlannedEffect, RuleError,
};
pub use resolve::{
    resolve, ApplicationSnapshot, EffectApplicationError, EffectRecipient, Outcome, OutcomeStatus,
    ResolutionContext, ResolutionPlan, ResolvedCheck, ResolvedEffect, Snapshot,
};
pub(crate) use resolve::{resolve_checked, ActorEffectState};
pub use strikes::{
    CheckPhase, CheckSide, CheckSite, FrozenRider, FrozenStrike, StrikeOrigin, TokenClaim,
};

#[cfg(test)]
mod edge_policy_tests;
#[cfg(test)]
mod movement_tests;
#[cfg(test)]
mod sequence_rider_contact_tests;
#[cfg(test)]
mod sequence_rider_lifecycle_tests;
#[cfg(test)]
mod sequence_rider_tests;
#[cfg(test)]
mod status_condition_tests;
#[cfg(test)]
mod strike_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod participation_tests;
#[cfg(test)]
mod restorable_loss_tests;

#[cfg(test)]
mod natural_source_tests;

#[cfg(test)]
mod effective_interaction_tests;
