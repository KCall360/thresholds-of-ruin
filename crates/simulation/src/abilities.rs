//! Validated action plans, pure effect resolution, and runtime orchestration.
mod strength_gain;
pub use strength_gain::StrengthGain;
mod compile;
mod haste_dose;
pub use haste_dose::HasteDoseOutcome;
pub(crate) use haste_dose::{haste_dose_conditions_valid, valid_haste_turn_ticks};
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
mod cancelled_payload;
mod contact_execution;
mod contact_policy;
mod contact_trace;
mod ray;
pub use contact_policy::{RayResolution, RayTraversal};
pub(crate) use contact_trace::MAX_CONTACT_STATES;
#[cfg(test)]
mod contact_trace_tests;
mod resolve;
pub(crate) use ray::definition_shape_valid as ray_definition_shape_valid;
pub use ray::{RayDelivery, RayIncomingOwner, RayIncomingSource, RayPlan};
mod spatial;
mod strikes;
pub use spatial::{MovementMode, MovementResult, Placement, SpatialInput, StepCommitment};

pub use participation::{
    AttemptedParticipation, CheckParticipation, PayloadParticipation, UnselectedReason,
};
pub(crate) use plan::item_effects_match_definition;
pub use plan::{
    AbilityPlan, CheckThreshold, DeviceDeliveryPlan, Plan, PlannedAmount, PlannedCheck,
    PlannedEffect, RuleError, SelfDevicePlan,
};
pub use resolve::{
    resolve, ApplicationSnapshot, EffectApplicationError, EffectRecipient, HoldDamageExecution,
    Outcome, OutcomeStatus, RelocationInput, ResolutionContext, ResolutionPlan, ResolvedCheck,
    ResolvedEffect, Snapshot,
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
        ((d.invocation.is_directional_ray() && ray::definition_shape_valid(d))
            || self_device_definition_valid(d))
            && d.cost.is_none()
            && d.cooldown.is_none()
            && d.supply_costs.is_empty()
    })
}

/// Self devices use one checkless, cost-free item payload, never a granted ability.
pub(crate) fn self_device_definition_valid(d: &crate::rules::AbilityDef) -> bool {
    let crate::rules::StrikeSource::Authored { strikes } = &d.strikes else {
        return false;
    };
    let [strike] = strikes.as_slice() else {
        return false;
    };
    d.invocation == InvocationKind::ActorTarget
        && d.reach == crate::rules::Reach::SelfOnly
        && d.interaction == crate::rules::Interaction::NonHostile
        && !d.basic_attack
        && d.cost.is_none()
        && d.cooldown.is_none()
        && d.supply_costs.is_empty()
        && strike.offset == 0
        && strike.unlock.is_none()
        && strike.check.is_none()
        && strike.riders.is_empty()
        && strike.on_failure.is_empty()
        && crate::magic_items::valid_payload_descriptor_shape(&strike.descriptors)
        && strike.on_success.len() <= crate::limits::CONSUMABLE_EFFECTS
        && strike
            .on_success
            .iter()
            .all(|effect| effect.valid_shape() && !effect.spatial())
}

#[cfg(test)]
mod consumable_effect_tests;

#[cfg(test)]
mod drain_life_tests;
#[cfg(test)]
mod magic_item_tests;
#[cfg(test)]
mod room_light_tests;

#[cfg(test)]
mod equipment_magic_tests;

#[cfg(test)]
mod weapon_selection_tests;

#[cfg(test)]
mod payload_resistance_tests;

#[cfg(test)]
mod cancellation_missile_tests;

#[cfg(test)]
mod growth_dose_tests;
#[cfg(test)]
mod haste_dose_tests;
#[cfg(test)]
mod portable_light_tests;
