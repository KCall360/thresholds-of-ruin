//! Deterministic requests from disclosed capabilities. Numeric attack capture is
//! an execution step and must never choose a different unknown weapon for AI.
use super::*;
use crate::abilities::{
    AbilityInvocation, CapturedWeaponProfile, ProjectileChoice, ProjectileChoiceQuery,
    ProjectileDeliverySelection, ThrownSourceSelection,
};
use crate::content_ids::AbilityId;
use crate::projectile_profiles::ProjectileDeliveryKind;

impl Game {
    pub(super) fn choose_ai_projectile_request(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        ability: &AbilityId,
    ) -> Option<Action> {
        let definition = self.content.rules().ability(ability)?;
        if let Some(cooldown) = definition.cooldown {
            self.creature(actor)?
                .validate_cooldown(ability, cooldown.get())
                .ok()?;
        }
        let crate::rules::StrikeSource::Projectile { delivery } = definition.strikes else {
            return None;
        };
        let query = match delivery {
            ProjectileDeliveryKind::Throw => ProjectileChoiceQuery::ThrowSources,
            ProjectileDeliveryKind::Launch => ProjectileChoiceQuery::Launchers,
        };
        let mut launchers = 0;
        // Conservative finite candidate policy, never an unbounded inventory
        // cross-product. Public menus can page farther than this NPC heuristic.
        for row in view.inventory.iter().take(128).filter(|row| row.identified) {
            let Some(choice) = self.projectile_choice_for_item(actor, ability, query, row.id)
            else {
                continue;
            };
            match choice {
                ProjectileChoice::Throw { source } => {
                    if let ThrownSourceSelection::Equipped { item, .. } = source {
                        if self.actors.get(&actor)?.known_equipment.curse(item) == Some(true) {
                            continue;
                        }
                    }
                    if let Some(action) = self.known_projectile_request(
                        actor,
                        target,
                        sight,
                        view,
                        ability,
                        ProjectileDeliverySelection::Throw { source },
                    ) {
                        return Some(action);
                    }
                }
                ProjectileChoice::Launcher { launcher } => {
                    launchers += 1;
                    for ammo in view.inventory.iter().take(128).filter(|row| row.identified) {
                        if self
                            .projectile_choice_for_item(
                                actor,
                                ability,
                                ProjectileChoiceQuery::Ammunition { launcher },
                                ammo.id,
                            )
                            .is_none()
                        {
                            continue;
                        }
                        if let Some(action) = self.known_projectile_request(
                            actor,
                            target,
                            sight,
                            view,
                            ability,
                            ProjectileDeliverySelection::Launch {
                                launcher,
                                ammunition: ammo.id,
                            },
                        ) {
                            return Some(action);
                        }
                    }
                    if launchers == 16 {
                        break;
                    }
                }
                ProjectileChoice::Ammunition { .. } => unreachable!("source phase"),
            }
        }
        None
    }
    fn known_projectile_request(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        ability: &AbilityId,
        delivery: ProjectileDeliverySelection,
    ) -> Option<Action> {
        let (item, socket) = match delivery {
            ProjectileDeliverySelection::Throw {
                source:
                    ThrownSourceSelection::Equipped { item, socket }
                    | ThrownSourceSelection::Carried { item, socket },
            } => (item, socket),
            ProjectileDeliverySelection::Launch { launcher, .. } => {
                (launcher.item, launcher.socket)
            }
        };
        let creature = self.creature(actor)?;
        let owner = self.actors.get(&actor)?;
        // Type identity was disclosed before this authored profile lookup.
        let spec = self
            .items
            .get(&item)?
            .spec
            .equipment
            .as_ref()?
            .weapon_profile
            .as_ref()?;
        let captured = CapturedWeaponProfile::Profile {
            spec: spec.clone(),
            expert: creature.derived().expertises.active(&spec.expertise),
        };
        let profile = captured.resolve(self.content.rules()).ok()??;
        let range = match delivery {
            ProjectileDeliverySelection::Throw { .. } => profile.thrown()?,
            ProjectileDeliverySelection::Launch { .. } => profile.launcher()?.range,
        };
        let definition = self.content.rules().ability(ability)?;
        let base = definition
            .cost
            .as_ref()
            .map(crate::costs::CostBundle::try_from)
            .transpose()
            .ok()?;
        let secondary = owner.anatomy.primary_attack_socket.socket() != Some(socket);
        let cost = crate::abilities::selected_weapon_cost(
            self.content.rules(),
            &captured,
            secondary,
            base,
        )
        .ok()?;
        if cost.is_some_and(|cost| {
            !resource_budget_satisfies(
                creature.costs(),
                cost,
                definition.ai.minimum_resource_percent,
            )
        }) {
            return None;
        }
        let direction = disclosed_ray_direction(
            target,
            sight,
            view,
            owner.body.eye,
            u16::try_from(range.long_feet / crate::weapon_profiles::PROJECTILE_STEP_FEET).ok()?,
        )?;
        Some(Action::UseAbility {
            id: ability.clone(),
            invocation: AbilityInvocation::directional_projectile(direction, delivery),
            supplies: Default::default(),
        })
    }
}
