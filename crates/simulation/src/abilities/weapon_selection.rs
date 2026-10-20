//! One invocation's weapon source, independent of target disclosure authority.
use super::{AbilityInvocation, AbilityPlan, WeaponSelection};
use crate::content_ids::{AbilityId, SpeciesId};
use crate::costs::{CostBundle, ResourceCost};
use crate::creatures::SelectedAttack;
use crate::equipment_capacity::{EquipmentCapacityUse, PrimaryAttackSocket};
use crate::rules::{
    Against, CheckActor, DamageExpr, Effect, SkillSelector, StrikeSource, TimingSource,
};
use crate::weapon_profiles::{EffectiveWeaponProfile, WeaponProfileSpec};
use crate::{ActorId, EquipmentSlotId, Game, GameError, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CapturedWeaponProfile {
    Ordinary,
    Profile {
        spec: WeaponProfileSpec,
        expert: bool,
    },
}
impl<'de> Deserialize<'de> for CapturedWeaponProfile {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Ordinary {},
            Profile {
                spec: WeaponProfileSpec,
                expert: bool,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Ordinary {} => Self::Ordinary,
            Fields::Profile { spec, expert } => Self::Profile { spec, expert },
        })
    }
}
impl CapturedWeaponProfile {
    pub(crate) fn resolve(
        &self,
        rules: &crate::rules::Ruleset,
    ) -> Result<Option<EffectiveWeaponProfile>, GameError> {
        match self {
            Self::Ordinary => Ok(None),
            Self::Profile { spec, expert } => Ok(Some(
                spec.validate(rules)
                    .map_err(|_| invalid())?
                    .state(*expert)
                    .clone(),
            )),
        }
    }
}
/// A deleted historical item does not erase the legal captured profile. Exact
/// occupancy is a live execution check; catalog/source consistency is structural.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FrozenWeaponSelection {
    Unselected,
    Natural,
    Equipped {
        species: SpeciesId,
        item: ItemId,
        socket: EquipmentSlotId,
        secondary: bool,
        profile: CapturedWeaponProfile,
    },
}
impl<'de> Deserialize<'de> for FrozenWeaponSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Unselected {},
            Natural {},
            Equipped {
                species: SpeciesId,
                item: ItemId,
                socket: EquipmentSlotId,
                secondary: bool,
                profile: CapturedWeaponProfile,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Unselected {} => Self::Unselected,
            Fields::Natural {} => Self::Natural,
            Fields::Equipped {
                species,
                item,
                socket,
                secondary,
                profile,
            } => Self::Equipped {
                species,
                item,
                socket,
                secondary,
                profile,
            },
        })
    }
}
fn invalid() -> GameError {
    GameError::Rule(super::RuleError::InvalidDefinition)
}
fn selected(skill: &SkillSelector) -> bool {
    matches!(skill, SkillSelector::SelectedWeapon)
}
fn effects_use_weapon(effects: &[Effect]) -> bool {
    effects.iter().any(|effect| {
        matches!(
            effect,
            Effect::Damage {
                damage: DamageExpr::SelectedWeapon { .. }
            }
        )
    })
}
fn check_uses_weapon(check: &crate::rules::CheckDef) -> bool {
    (check.actor == CheckActor::User && selected(&check.skill))
        || match &check.against {
            Against::Opposed { skill } => check.actor == CheckActor::Target && selected(skill),
            Against::Difficulty { formula } => formula.skill.as_ref().is_some_and(selected),
            Against::Defense { .. } => false,
        }
}
impl crate::rules::AbilityDef {
    /// Callers select the emitted rows, never a locked-only authored branch.
    pub(crate) fn consumes_caster_weapon(&self, included: impl Fn(usize) -> bool) -> bool {
        if matches!(
            self.invocation,
            super::InvocationKind::DirectionalRay
                | super::InvocationKind::DirectionalContactRay
                | super::InvocationKind::DirectionalProjectile
        ) || matches!(self.strikes, StrikeSource::NaturalMelee { .. })
        {
            return false;
        }
        if matches!(self.strikes, StrikeSource::SelectedMelee)
            || matches!(
                self.timing.source,
                TimingSource::SelectedWeapon | TimingSource::SelectedWeaponAdjusted { .. }
            )
        {
            return true;
        }
        let StrikeSource::Authored { strikes } = &self.strikes else {
            return false;
        };
        strikes.iter().enumerate().any(|(ordinal, strike)| {
            included(ordinal)
                && (strike.check.as_ref().is_some_and(check_uses_weapon)
                    || effects_use_weapon(&strike.on_success)
                    || effects_use_weapon(&strike.on_failure)
                    || strike.riders.iter().any(|rider| {
                        rider
                            .check
                            .as_ref()
                            .is_some_and(|check| selected(&check.skill))
                            || effects_use_weapon(&rider.effects)
                    }))
        })
    }
}
pub(crate) fn selected_weapon_cost(
    rules: &crate::rules::Ruleset,
    profile: &CapturedWeaponProfile,
    secondary: bool,
    base: Option<CostBundle>,
) -> Result<Option<CostBundle>, GameError> {
    let profile = profile.resolve(rules)?;
    if !secondary {
        return Ok(base);
    }
    let surcharge = CostBundle::single(ResourceCost {
        resource: crate::resources::Resource::Focus,
        start: 0,
        resolution: if profile
            .as_ref()
            .is_some_and(EffectiveWeaponProfile::offhand)
        {
            1
        } else {
            2
        },
    })
    .map_err(|_| invalid())?;
    Ok(Some(match base {
        Some(base) => base.checked_add(surcharge).map_err(|_| invalid())?,
        None => surcharge,
    }))
}

impl FrozenWeaponSelection {
    pub(crate) fn cost(
        &self,
        rules: &crate::rules::Ruleset,
        base: Option<CostBundle>,
    ) -> Result<Option<CostBundle>, GameError> {
        let Self::Equipped {
            secondary, profile, ..
        } = self
        else {
            return Ok(base);
        };
        selected_weapon_cost(rules, profile, *secondary, base)
    }
    pub(crate) fn valid_for(&self, plan: &AbilityPlan, rules: &crate::rules::Ruleset) -> bool {
        let Some(definition) = rules.ability(&plan.ability) else {
            return false;
        };
        if !plan.invocation.valid() {
            return false;
        }
        let consumes = definition.consumes_caster_weapon(|ordinal| plan.strikes.iter().any(|strike| matches!(strike.origin, super::StrikeOrigin::Ability { ordinal: actual } if usize::from(actual) == ordinal)));
        let natural = matches!(definition.strikes, StrikeSource::NaturalMelee { .. });
        match self {
            Self::Unselected => {
                !consumes && !natural && plan.invocation.weapon == WeaponSelection::Default
            }
            Self::Natural => {
                (consumes || natural)
                    && plan.invocation.weapon == WeaponSelection::Default
                    && !plan
                        .strikes
                        .iter()
                        .any(|strike| matches!(strike.origin, super::StrikeOrigin::Weapon { .. }))
            }
            Self::Equipped {
                item,
                socket,
                secondary,
                profile,
                ..
            } => {
                consumes
                    && (WeaponSelection::Equipped {
                        item: *item,
                        socket: *socket,
                    })
                    .valid()
                    && profile.resolve(rules).is_ok()
                    && match plan.invocation.weapon {
                        WeaponSelection::Default => !secondary,
                        WeaponSelection::Equipped {
                            item: requested,
                            socket: requested_socket,
                        } => requested == *item && requested_socket == *socket,
                    }
                    && plan.strikes.iter().all(|strike| match strike.origin {
                        super::StrikeOrigin::Weapon { item: source } => source == *item,
                        super::StrikeOrigin::Natural { .. }
                        | super::StrikeOrigin::NaturalMelee { .. } => false,
                        super::StrikeOrigin::Ability { .. } => true,
                    })
            }
        }
    }
}
pub(super) struct InvocationWeapon {
    pub attack: SelectedAttack,
    pub frozen: FrozenWeaponSelection,
}
/// Settlement classification for the exact work being previewed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AbilityWeaponPreviewPhase {
    Fresh,
    Resuming,
    PaidPrefix,
}
/// Read-only own cost/timing. The full captured bundle is shown even when its
/// start charge is paid or its resolution charge settled; phase prevents a
/// client from presenting that bundle as a second payment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AbilityWeaponPreview {
    pub cost: Option<CostBundle>,
    pub preparation: u64,
    pub recovery: u64,
    pub phase: AbilityWeaponPreviewPhase,
}
impl Game {
    /// Uses the same exact work capture/resume path as ordinary preparation.
    /// This does not mutate RNG, reserve anything, or promise affordability.
    pub fn preview_ability_weapon(
        &self,
        actor: ActorId,
        ability: &AbilityId,
        invocation: AbilityInvocation,
        supplies: crate::supplies::SupplySelection,
    ) -> Result<AbilityWeaponPreview, GameError> {
        let work = crate::Work::UseAbility {
            id: ability.clone(),
            invocation,
            supplies,
        };
        let plan = self.plan_work(actor, work)?;
        let pending = self
            .preparation(actor)
            .filter(|pending| pending.plan == plan);
        Ok(AbilityWeaponPreview {
            cost: plan.cost(),
            preparation: pending.map_or_else(
                || plan.preparation(),
                |pending| pending.remaining_at(self.actor_clock(actor)),
            ),
            recovery: plan.recovery(),
            phase: pending.map_or(AbilityWeaponPreviewPhase::Fresh, |pending| {
                if pending.has_prefix() {
                    AbilityWeaponPreviewPhase::PaidPrefix
                } else {
                    AbilityWeaponPreviewPhase::Resuming
                }
            }),
        })
    }
}
impl Game {
    pub(super) fn capture_invocation_weapon(
        &self,
        actor: ActorId,
        definition: &crate::rules::AbilityDef,
        invocation: AbilityInvocation,
    ) -> Result<InvocationWeapon, GameError> {
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let consumes = definition.consumes_caster_weapon(|ordinal| match &definition.strikes {
            StrikeSource::Authored { strikes } => strikes[ordinal]
                .unlock
                .as_ref()
                .is_none_or(|id| creature.derived().active_talents.contains(id)),
            _ => true,
        });
        if !invocation.valid() || (!consumes && invocation.weapon != WeaponSelection::Default) {
            return Err(invalid());
        }
        if matches!(definition.strikes, StrikeSource::NaturalMelee { .. }) {
            return Ok(InvocationWeapon {
                attack: creature.derived().natural_attack.clone(),
                frozen: FrozenWeaponSelection::Natural,
            });
        }
        if !consumes {
            return Ok(InvocationWeapon {
                attack: creature.derived().selected_attack.clone(),
                frozen: FrozenWeaponSelection::Unselected,
            });
        }
        self.capture_selected_weapon(actor, invocation.weapon)
    }
    pub(super) fn capture_selected_weapon(
        &self,
        actor: ActorId,
        selection: WeaponSelection,
    ) -> Result<InvocationWeapon, GameError> {
        if !selection.valid() {
            return Err(invalid());
        }
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        let selected = match selection {
            WeaponSelection::Default => &creature.derived().selected_attack,
            WeaponSelection::Equipped { item, socket } => {
                let attack = &creature
                    .derived()
                    .equipped_attacks
                    .get(&socket)
                    .ok_or(GameError::ItemUnavailable)?
                    .attack;
                if attack.source != crate::creatures::SourceId::Equipment(item) {
                    return Err(GameError::ItemUnavailable);
                }
                attack
            }
        };
        let crate::creatures::SourceId::Equipment(item) = selected.source else {
            return Ok(InvocationWeapon {
                attack: selected.clone(),
                frozen: FrozenWeaponSelection::Natural,
            });
        };
        let socket = selected.socket.ok_or_else(invalid)?;
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let slot = self
            .content
            .rules()
            .equipment_slot(
                owner
                    .anatomy
                    .slots
                    .get(usize::from(socket.0))
                    .ok_or_else(invalid)?,
            )
            .ok_or_else(invalid)?;
        if let EquipmentCapacityUse::Shared { resource, .. } = &slot.capacity {
            if creature
                .derived()
                .equipment_capacity_deficits
                .contains_key(resource)
            {
                return Err(GameError::ItemUnavailable);
            }
        }
        let source = self.items.get(&item).ok_or(GameError::ItemUnavailable)?;
        if source.location != ItemLocation::Carried(actor)
            || owner.equipment.get(&socket) != Some(&item)
        {
            return Err(GameError::ItemUnavailable);
        }
        let equipment = source
            .spec
            .equipment
            .as_ref()
            .ok_or(GameError::ItemUnavailable)?;
        let profile =
            equipment
                .weapon_profile
                .as_ref()
                .map_or(CapturedWeaponProfile::Ordinary, |spec| {
                    CapturedWeaponProfile::Profile {
                        spec: spec.clone(),
                        expert: creature.derived().expertises.active(&spec.expertise),
                    }
                });
        Ok(InvocationWeapon {
            attack: selected.clone(),
            frozen: FrozenWeaponSelection::Equipped {
                species: creature.build().species_id().clone(),
                item,
                socket,
                secondary: owner.anatomy.primary_attack_socket
                    != PrimaryAttackSocket::Socket { slot: socket },
                profile,
            },
        })
    }
    pub(crate) fn frozen_weapon_source_valid_with_context(
        &self,
        selection: &FrozenWeaponSelection,
        sources: super::PlanSourceContext<'_>,
    ) -> bool {
        let FrozenWeaponSelection::Equipped {
            species,
            item,
            socket,
            secondary,
            profile,
        } = selection
        else {
            return true;
        };
        let Some(species) = self.content.catalog().species.get(species) else {
            return false;
        };
        let Some(slot) = species
            .anatomy
            .slots
            .get(usize::from(socket.0))
            .and_then(|id| self.content.rules().equipment_slot(id))
        else {
            return false;
        };
        let Ok(effective) = profile.resolve(self.content.rules()) else {
            return false;
        };
        let maximum = match profile {
            CapturedWeaponProfile::Ordinary => 1,
            CapturedWeaponProfile::Profile { spec, .. } => {
                match spec.validate(self.content.rules()) {
                    Ok(profile) => profile.maximum_grip_demand(),
                    Err(_) => return false,
                }
            }
        };
        let demand = u16::from(
            effective
                .as_ref()
                .map_or(1, EffectiveWeaponProfile::grip_demand),
        );
        let fits = match &slot.capacity {
            EquipmentCapacityUse::None => maximum == 1,
            EquipmentCapacityUse::Shared { resource, units } => {
                units.checked_mul(demand).is_some_and(|demand| {
                    species
                        .anatomy
                        .capacities
                        .get(resource)
                        .is_some_and(|capacity| demand <= capacity)
                })
            }
        };
        item.0 < self.next_item_id
            && crate::equipment_capture::captured_profile_matches_surviving_item(
                self, *item, &slot.id, profile, sources,
            )
            && slot.requires_attack
            && fits
            && *secondary
                == (species.anatomy.primary_attack_socket
                    != PrimaryAttackSocket::Socket { slot: *socket })
    }
    pub(crate) fn frozen_weapon_available(
        &self,
        actor: ActorId,
        selection: &FrozenWeaponSelection,
    ) -> bool {
        let FrozenWeaponSelection::Equipped { item, socket, .. } = selection else {
            return true;
        };
        self.actors.get(&actor).is_some_and(|owner| {
            owner.equipment.get(socket) == Some(item)
                && self
                    .items
                    .get(item)
                    .is_some_and(|item| item.location == ItemLocation::Carried(actor))
                && owner
                    .anatomy
                    .slots
                    .get(usize::from(socket.0))
                    .and_then(|id| self.content.rules().equipment_slot(id))
                    .is_some_and(|slot| slot.requires_attack)
        })
    }
}
