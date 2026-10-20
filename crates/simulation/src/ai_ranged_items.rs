//! Bounded acquisition of known launcher/payload pairs. This policy never
//! compiles a numerical projectile or inspects an opponent's private state.
use crate::abilities::{ProjectileChoice, ProjectileChoiceQuery};
use crate::content_ids::{AbilityId, LauncherFamilyId};
use crate::{Action, ActorId, EquipmentSlotId, Game, ItemId, Observation};
use std::collections::BTreeMap;

const INVENTORY_BOUND: usize = 128;
const LAUNCHER_BOUND: usize = 16;
const SHOT_RESERVE: u64 = 6;

struct LauncherChoice {
    item: ItemId,
    socket: EquipmentSlotId,
    previous: Option<ItemId>,
    family: LauncherFamilyId,
}

impl Game {
    /// The existing explicit ability ordering supplies the preference. Expertise
    /// changes legal capabilities and prices, never an actor's tactical role.
    fn acquisition_launch_ability(&self, actor: ActorId) -> Option<(AbilityId, bool)> {
        let creature = self.creature(actor)?;
        let rules = self.content.rules();
        let basic_priority = rules.ability(&self.basic_attack(actor)?)?.ai.priority;
        creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                let def = rules.ability(id)?;
                (def.ai.hostile
                    && matches!(
                        def.strikes,
                        crate::rules::StrikeSource::Projectile {
                            delivery: crate::projectile_profiles::ProjectileDeliveryKind::Launch
                        }
                    ))
                .then_some((def.ai.priority, id))
            })
            .max_by(|(ap, ai), (bp, bi)| ap.cmp(bp).then_with(|| bi.cmp(ai)))
            .map(|(priority, id)| (id.clone(), priority > basic_priority))
    }

    pub(crate) fn known_launcher_family(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<LauncherFamilyId> {
        let owner = self.actors.get(&actor)?;
        let spec = &self.items.get(&item)?.spec;
        if spec.concealed && !owner.knowledge.contains(&spec.identity) {
            return None;
        }
        let profile = spec.equipment.as_ref()?.weapon_profile.as_ref()?;
        profile
            .validate(self.content.rules())
            .ok()?
            .state(
                self.creature(actor)?
                    .derived()
                    .expertises
                    .active(&profile.expertise),
            )
            .launcher()
            .map(|launcher| launcher.family.clone())
    }

    pub(crate) fn known_payload_family(
        &self,
        actor: ActorId,
        item: ItemId,
    ) -> Option<&LauncherFamilyId> {
        let owner = self.actors.get(&actor)?;
        if owner.equipment.values().any(|equipped| *equipped == item) {
            return None;
        }
        let spec = &self.items.get(&item)?.spec;
        if spec.concealed && !owner.knowledge.contains(&spec.identity) {
            return None;
        }
        Some(&spec.projectile.as_ref()?.family)
    }

    /// Collection can wait for ordinary recovery, but must not provision a shot
    /// whose complete known resource bundle exceeds this loadout's maxima.
    pub(crate) fn ranged_loadout_has_budget(
        &self,
        actor: ActorId,
        item: ItemId,
        socket: EquipmentSlotId,
    ) -> bool {
        let Some((ability, _)) = self.acquisition_launch_ability(actor) else {
            return false;
        };
        let Some(creature) = self.creature(actor) else {
            return false;
        };
        let Some(spec) = self
            .items
            .get(&item)
            .and_then(|item| item.spec.equipment.as_ref())
            .and_then(|gear| gear.weapon_profile.as_ref())
        else {
            return false;
        };
        let profile = crate::abilities::CapturedWeaponProfile::Profile {
            spec: spec.clone(),
            expert: creature.derived().expertises.active(&spec.expertise),
        };
        let Some(definition) = self.content.rules().ability(&ability) else {
            return false;
        };
        let Ok(base) = definition
            .cost
            .as_ref()
            .map(crate::costs::CostBundle::try_from)
            .transpose()
        else {
            return false;
        };
        let secondary = creature.derived().anatomy.primary_attack_socket.socket() != Some(socket);
        let Ok(cost) =
            crate::abilities::selected_weapon_cost(self.content.rules(), &profile, secondary, base)
        else {
            return false;
        };
        cost.is_none_or(|bundle| {
            bundle.entries().all(|entry| {
                entry
                    .start
                    .checked_add(entry.resolution)
                    .is_some_and(|total| {
                        total <= creature.costs().resources().maximum(entry.resource)
                    })
            })
        })
    }

    /// Bounded pairing uses only supplied disclosed candidates. Inventory comes
    /// before ground loot and equipped launchers come first, preventing spare
    /// collection and replacement oscillation. Large inventories fail closed:
    /// truncating the reserve count could otherwise keep collecting forever.
    fn known_launcher_choice(
        &self,
        actor: ActorId,
        view: &Observation,
        ground: &[ItemId],
    ) -> Option<LauncherChoice> {
        if view.inventory.len() > INVENTORY_BOUND {
            return None;
        }
        let (ability, prefer_ranged) = self.acquisition_launch_ability(actor)?;
        let owner = self.actors.get(&actor)?;
        let mut ranked: Vec<_> = view
            .inventory
            .iter()
            .filter(|row| row.identified)
            .map(|row| {
                (
                    !owner.equipment.values().any(|id| *id == row.id),
                    false,
                    row.id,
                )
            })
            .chain(ground.iter().map(|item| (true, true, *item)))
            .collect();
        ranked.sort_unstable();
        let items: Vec<_> = ranked.into_iter().map(|(_, _, item)| item).collect();
        let mut launchers = 0;
        let mut workspace = None;
        for item in items.iter().copied() {
            let Some(family) = self.known_launcher_family(actor, item) else {
                continue;
            };
            launchers += 1;
            if launchers > LAUNCHER_BOUND {
                break;
            }
            if !items.iter().any(|payload| {
                *payload != item && self.known_payload_family(actor, *payload) == Some(&family)
            }) {
                continue;
            }
            if let Some(ProjectileChoice::Launcher { launcher }) = self.projectile_choice_for_item(
                actor,
                &ability,
                ProjectileChoiceQuery::Launchers,
                item,
            ) {
                if !self.ranged_loadout_has_budget(actor, item, launcher.socket) {
                    continue;
                }
                return Some(LauncherChoice {
                    item,
                    socket: launcher.socket,
                    previous: Some(item),
                    family,
                });
            }
            // Do not try to relocate an already-equipped but presently unusable
            // launcher into another socket to evade a capacity deficit.
            if owner.equipment.values().any(|id| *id == item) {
                continue;
            }
            if let Some((socket, previous)) =
                self.ranged_gear_destination(&mut workspace, actor, item, prefer_ranged)
            {
                return Some(LauncherChoice {
                    item,
                    socket,
                    previous,
                    family,
                });
            }
        }
        None
    }

    pub(crate) fn choose_ranged_gear(&self, actor: ActorId, view: &Observation) -> Option<Action> {
        let choice = self.known_launcher_choice(actor, view, &[])?;
        if choice.previous == Some(choice.item) {
            return None;
        }
        Some(choice.previous.map_or(
            Action::Equip {
                item: choice.item,
                slot: choice.socket,
            },
            |item| Action::Unequip { item },
        ))
    }

    /// Ordinary gear improvements may not immediately remove the usable launcher
    /// selected by a strictly higher authored ability priority. Depletion removes
    /// this protection, so normal safe melee recovery needs no saved AI plan.
    pub(crate) fn retained_ranged_weapon(
        &self,
        actor: ActorId,
        view: &Observation,
    ) -> Option<ItemId> {
        if !self.acquisition_launch_ability(actor)?.1 {
            return None;
        }
        if view.inventory.len() > INVENTORY_BOUND {
            return None;
        }
        let (ability, _) = self.acquisition_launch_ability(actor)?;
        let owner = self.actors.get(&actor)?;
        owner.equipment.values().find_map(|item| {
            let family = self.known_launcher_family(actor, *item)?;
            let Some(ProjectileChoice::Launcher { launcher }) = self.projectile_choice_for_item(
                actor,
                &ability,
                ProjectileChoiceQuery::Launchers,
                *item,
            ) else {
                return None;
            };
            (self.ranged_loadout_has_budget(actor, *item, launcher.socket)
                && view
                    .inventory
                    .iter()
                    .filter(|row| row.identified)
                    .any(|row| self.known_payload_family(actor, row.id) == Some(&family)))
            .then_some(*item)
        })
    }

    pub(crate) fn ranged_loot_quantities(
        &self,
        actor: ActorId,
        view: &Observation,
        profile: &crate::ai::AiProfile,
        route: &mut impl FnMut(tor_world::Location) -> Option<Vec<crate::TravelStep>>,
    ) -> BTreeMap<ItemId, u64> {
        let mut result = BTreeMap::new();
        if !profile.equipment
            || !profile.loot
            || view.inventory.len() > INVENTORY_BOUND
            || self.acquisition_launch_ability(actor).is_none()
        {
            return result;
        }
        let ground: Vec<_> =
            view.ground_items
                .iter()
                .take(INVENTORY_BOUND)
                .filter(|row| row.identified)
                .filter(|row| {
                    self.known_launcher_family(actor, row.id).is_some()
                        || self.known_payload_family(actor, row.id).is_some()
                })
                .filter(|row| !self.exposed_to_visible_hostile(actor, row.location, view))
                .filter(|row| {
                    if row.location == view.location {
                        return true;
                    }
                    if profile.idle == crate::behavior::IdleBehavior::Wait {
                        return false;
                    }
                    route(row.location).is_some_and(|path| {
                        path.len() <= 3
                            && path.first().is_some_and(|step| {
                                self.actor_translation(actor, step.direction).is_some_and(
                                    |(at, _)| !self.exposed_to_visible_hostile(actor, at, view),
                                )
                            })
                    })
                })
                .map(|row| row.id)
                .collect();
        let Some(choice) = self.known_launcher_choice(actor, view, &ground) else {
            return result;
        };
        if ground.contains(&choice.item) {
            result.insert(choice.item, 1);
        }
        let carried = view
            .inventory
            .iter()
            .filter(|row| row.identified)
            .filter(|row| {
                row.id != choice.item
                    && self.known_payload_family(actor, row.id) == Some(&choice.family)
            })
            .fold(0_u64, |sum, row| sum.saturating_add(row.quantity));
        let deficit = SHOT_RESERVE.saturating_sub(carried);
        if deficit > 0 {
            for item in ground {
                if self.known_payload_family(actor, item) == Some(&choice.family) {
                    result.insert(item, deficit);
                }
            }
        }
        result
    }
}
