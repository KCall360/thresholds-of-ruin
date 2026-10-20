//! Validated action plans, pure effect resolution, and runtime orchestration.
mod compile;
mod execution;
mod invocation;
mod projectile;
mod projectile_choices;
pub use projectile_choices::{ProjectileChoice, ProjectileChoiceQuery};
mod projectile_execution;
mod projectile_flight;
mod projectile_release;
mod projectile_selection;
pub use projectile::{ProjectilePlan, ProjectileSource};
pub use projectile_selection::{
    EquippedProjectileWeapon, ProjectileDeliverySelection, ThrownSourceSelection,
};
mod weapon_selection;
pub(crate) use weapon_selection::selected_weapon_cost;
pub use weapon_selection::{
    AbilityWeaponPreview, AbilityWeaponPreviewPhase, CapturedWeaponProfile, FrozenWeaponSelection,
};
mod participation;
pub use invocation::{
    AbilityAim, AbilityInvocation, InvocationKind, PushDirection, StepDirection, WeaponSelection,
};
mod plan;
mod plan_validation;
pub(crate) use plan_validation::PlanSourceContext;
mod ray;
mod resolve;
pub use ray::{RayDelivery, RayIncomingOwner, RayIncomingSource, RayPlan};
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
    resolve, ApplicationSnapshot, EffectApplicationError, EffectRecipient, HoldDamageExecution,
    Outcome, OutcomeStatus, ResolutionContext, ResolutionPlan, ResolvedCheck, ResolvedEffect,
    Snapshot,
};
pub(crate) use resolve::{resolve_checked, resolve_escape_check, ActorEffectState};
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

#[cfg(test)]
mod hold_damage_tests;

#[cfg(test)]
mod theft_tests;

#[cfg(test)]
mod gaze_tests;

#[cfg(test)]
mod corrosion_tests;

#[cfg(test)]
mod ray_tests;

pub(crate) fn device_definition_valid(
    rules: &crate::rules::Ruleset,
    id: &crate::content_ids::AbilityId,
) -> bool {
    rules.ability(id).is_some_and(|d| {
        d.invocation.is_directional_ray()
            && ray::definition_shape_valid(d)
            && d.cost.is_none()
            && d.cooldown.is_none()
            && d.supply_costs.is_empty()
    })
}

#[cfg(test)]
mod consumable_effect_tests;

#[cfg(test)]
mod magic_item_tests;

#[cfg(test)]
mod equipment_magic_tests;

#[cfg(test)]
mod weapon_selection_tests;
