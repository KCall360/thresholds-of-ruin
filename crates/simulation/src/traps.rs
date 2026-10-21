//! Floor-contact hazards. Actor-owned contact identity survives saves; a bounded
//! private work queue prevents teleport chains from recursively dispatching.
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, VecDeque};
use tor_world::{ConcealedFeatureId, Direction, Location, RegionId, Shared, TrapKind};

const MAX_CHAIN_CONTACTS: usize = 4096;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct TrapContacts(Vec<u64>);
impl TrapContacts {
    pub(crate) fn ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.0.iter().copied()
    }
}
pub(crate) fn read_contacts<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<TrapContacts>, D::Error> {
    let values = crate::bounded::Bounded::<u64, 64>::deserialize(d)?.0;
    if values.iter().any(|id| *id == 0 || *id == u64::MAX)
        || !values.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err(serde::de::Error::custom("invalid trap contacts"));
    }
    Ok(Shared::new(TrapContacts(values)))
}

/// A spatially anchored condition has exactly one condition-owned timer. Losing
/// ground contact alone does not release it; leaving its occupied cell does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct SpatialConditionAnchor {
    condition: crate::content_ids::ConditionId,
    location: Location,
}
impl<'de> Deserialize<'de> for SpatialConditionAnchor {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            condition: crate::content_ids::ConditionId,
            #[serde(deserialize_with = "crate::bounded::location")]
            location: Location,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            condition: fields.condition,
            location: fields.location,
        })
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct SpatialConditionAnchors(Vec<SpatialConditionAnchor>);
impl<'de> Deserialize<'de> for SpatialConditionAnchors {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let values = crate::bounded::Bounded::<SpatialConditionAnchor, 64>::deserialize(d)?.0;
        if !values
            .windows(2)
            .all(|pair| pair[0].condition < pair[1].condition)
            || values
                .iter()
                .any(|entry| entry.location.region.0 == 0 || entry.location.region.0 == u64::MAX)
        {
            return Err(serde::de::Error::custom(
                "invalid spatial condition anchors",
            ));
        }
        Ok(Self(values))
    }
}
impl SpatialConditionAnchors {
    pub(crate) fn valid_for(&self, actor: &crate::Actor) -> bool {
        self.0.len() <= 64
            && self.0.iter().all(|entry| {
                actor.alive()
                    && actor.combat.as_ref().is_some_and(|combat| {
                        let creature = combat.creature();
                        creature.conditions().has_active(&entry.condition)
                            && trap_kind_valid(
                                creature.build().ruleset(),
                                &TrapKind::BearTrap {
                                    condition: entry.condition.as_str().to_owned(),
                                    duration: 1,
                                },
                            )
                    })
            })
    }
}

/// Private host continuation: no contact mutation, discovery, roll or lid opening
/// occurs until every member is resident. Not an ordinary saved event queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrapRegionBoundary {
    pub actor: ActorId,
    pub location: Location,
    pub orientation: u8,
    pub avoidable_traps: BTreeSet<u64>,
    pub imposed_normal: Option<[i64; 3]>,
    pub members: BTreeSet<RegionId>,
}
type PendingTrapContact = (ActorId, u64, Location, Vec<ActorId>, bool);
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct TrapResolution {
    visited: BTreeSet<(ActorId, u64)>,
    pending: VecDeque<PendingTrapContact>,
    processing: bool,
}

pub fn trap_kind_valid(rules: &crate::rules::Ruleset, kind: &TrapKind) -> bool {
    use crate::content_ids::{ConditionId, EquipmentSlot};
    use crate::rules::{
        ConditionLifecycle, ConditionPreparation, ConditionRestriction, ConditionTiming,
    };
    let exposure = match kind {
        TrapKind::SleepingGas { exposure, .. } => exposure.as_deref(),
        TrapKind::PoisonDart { exposure, .. } => Some(exposure.as_str()),
        _ => None,
    };
    let exposure_valid = exposure.is_none_or(|id| {
        crate::content_ids::DescriptorId::new(id)
            .ok()
            .is_some_and(|id| rules.descriptor(&id).is_some())
    });
    kind.valid()
        && exposure_valid
        && match kind {
            TrapKind::Rust { slot, .. } => EquipmentSlot::new(slot)
                .ok()
                .is_some_and(|id| rules.equipment_slot(&id).is_some()),
            TrapKind::SleepingGas { condition, .. } | TrapKind::BearTrap { condition, .. } => {
                ConditionId::new(condition)
                    .ok()
                    .and_then(|id| rules.condition(&id))
                    .is_some_and(|definition| {
                        definition.lifecycle == ConditionLifecycle::Timed
                            && matches!(definition.timing, ConditionTiming::ActorActive { .. })
                            && if matches!(kind, TrapKind::SleepingGas { .. }) {
                                definition.preparation == ConditionPreparation::Pause
                                    && definition
                                        .restrictions
                                        .contains(&ConditionRestriction::VoluntaryActions)
                            } else {
                                definition.preparation == ConditionPreparation::Preserve
                                    && !definition.source_scoped
                                    && definition
                                        .restrictions
                                        .contains(&ConditionRestriction::OrdinaryMovement)
                                    && definition
                                        .restrictions
                                        .contains(&ConditionRestriction::MovementAbilities)
                                    && !definition
                                        .restrictions
                                        .contains(&ConditionRestriction::VoluntaryActions)
                            }
                    })
            }

            _ => true,
        }
}
pub(crate) fn trap_content_valid(content: &crate::creatures::Content, kind: &TrapKind) -> bool {
    trap_kind_valid(content.rules(), kind)
        && match kind {
            TrapKind::Arrow { ammunition, .. } => content.trap_ammunition(ammunition).is_some(),
            _ => true,
        }
}

impl Game {
    pub(crate) fn loaded_traps_valid(&self) -> bool {
        self.world.loaded_regions().all(|region| {
            self.world.traps_in_region(region).all(|(at, trap)| {
                trap_content_valid(&self.content, &trap.kind)
                    && self.world.trap_geometry_valid(at, trap)
            })
        })
    }
    pub fn trap_region_boundary(&self) -> Option<&TrapRegionBoundary> {
        self.trap_region_boundary.as_ref()
    }
    pub fn trap_missing_regions(&self, boundary: &TrapRegionBoundary) -> BTreeSet<RegionId> {
        boundary
            .members
            .iter()
            .filter(|region| self.world.region(**region).is_none())
            .copied()
            .collect()
    }
    fn trap_required_regions(&self, region: RegionId, kind: &TrapKind) -> BTreeSet<RegionId> {
        let destination = match kind {
            TrapKind::TrapDoor {
                destination,
                open: false,
            } => destination.region,
            TrapKind::Teleport {} => region,
            _ => return BTreeSet::new(),
        };
        self.activation_scope_at(destination).map_or_else(
            || BTreeSet::from([destination]),
            |scope| scope.members.iter().copied().collect(),
        )
    }
    /// Supplemental destinations stay load-only and never root more trap floors.
    pub(crate) fn trap_loaded_closure(
        &self,
        ordinary_loaded: &BTreeSet<RegionId>,
        loaded: &mut BTreeSet<RegionId>,
    ) {
        for region in ordinary_loaded {
            for (_, trap) in self.world.traps_in_region(*region) {
                loaded.extend(self.trap_required_regions(*region, &trap.kind));
            }
        }
        if let Some(boundary) = &self.trap_region_boundary {
            loaded.extend(boundary.members.iter().copied());
        }
    }
    pub(crate) fn resume_trap_region_boundary(&mut self) -> Result<bool, GameError> {
        let Some(boundary) = self.trap_region_boundary.take() else {
            return Ok(false);
        };
        if !self.trap_missing_regions(&boundary).is_empty()
            || self.actors.get(&boundary.actor).is_none_or(|actor| {
                actor.location != boundary.location || actor.orientation != boundary.orientation
            })
        {
            return Err(GameError::InvalidLocation);
        }
        self.reconcile_trap_contacts_after_entry(
            boundary.actor,
            boundary.imposed_normal,
            boundary.avoidable_traps,
        )?;
        Ok(true)
    }

    pub(crate) fn spatial_condition_anchors_valid(&self, actor: &crate::Actor) -> bool {
        actor.spatial_condition_anchors.valid_for(actor)
            && (actor.spatial_condition_anchors.0.is_empty()
                || self
                    .body_cells(actor.location, actor.orientation, &actor.body)
                    .is_some_and(|cells| {
                        actor
                            .spatial_condition_anchors
                            .0
                            .iter()
                            .all(|entry| cells.iter().any(|(at, _)| *at == entry.location))
                    }))
    }
    pub(crate) fn reconcile_spatial_condition_anchors(
        &mut self,
        id: ActorId,
    ) -> Result<(), GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if actor.spatial_condition_anchors.0.is_empty() {
            return Ok(());
        }
        let cells = self
            .body_cells(actor.location, actor.orientation, &actor.body)
            .ok_or(GameError::InvalidLocation)?;
        let mut remove = Vec::new();
        let retained = actor
            .spatial_condition_anchors
            .0
            .iter()
            .filter(|entry| {
                let active = actor.combat.as_ref().is_some_and(|combat| {
                    combat.creature().conditions().has_active(&entry.condition)
                });
                let attached = cells.iter().any(|(at, _)| *at == entry.location);
                if active && !attached {
                    remove.push(crate::rules::Effect::RemoveCondition {
                        condition: entry.condition.clone(),
                    });
                }
                active && attached
            })
            .cloned()
            .collect();
        // Publish the new anchor book before effects reenter actor publication.
        self.actors
            .get_mut(&id)
            .expect("validated actor")
            .spatial_condition_anchors = SpatialConditionAnchors(retained);
        if !remove.is_empty() {
            self.apply_effects_in_transaction(id, &remove)?;
        }
        Ok(())
    }
    /// Freeze only the moving actor's already-owned knowledge before entry can
    /// discover destination features. New knowledge at the destination is too late.
    pub(crate) fn known_traps_at_placement(
        &self,
        id: ActorId,
        location: Location,
        orientation: u8,
    ) -> Result<BTreeSet<u64>, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if !self.world.has_concealed_features() {
            return Ok(BTreeSet::new());
        }
        let cells = self
            .body_cells(location, orientation, &actor.body)
            .ok_or(GameError::InvalidLocation)?;
        Ok(cells
            .iter()
            .filter_map(|(at, _)| self.world.trap(*at))
            .filter(|trap| {
                trap.avoidance_dc.is_some()
                    && !matches!(trap.kind, TrapKind::TrapDoor { open: true, .. })
                    && self.feature_known(id, ConcealedFeatureId::Trap(trap.id))
            })
            .map(|trap| trap.id)
            .collect())
    }
    /// Non-movement contact is forced by default: physics, teleport, setup and
    /// losing hover never borrow a deliberate entry's avoidance permission.
    pub(crate) fn reconcile_trap_contacts(
        &mut self,
        id: ActorId,
        imposed_normal: Option<[i64; 3]>,
    ) -> Result<(), GameError> {
        self.reconcile_trap_contacts_after_entry(id, imposed_normal, BTreeSet::new())
    }
    pub(crate) fn reconcile_trap_contacts_after_entry(
        &mut self,
        id: ActorId,
        imposed_normal: Option<[i64; 3]>,
        avoidable_traps: BTreeSet<u64>,
    ) -> Result<(), GameError> {
        self.reconcile_spatial_condition_anchors(id)?;
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if !actor.alive() || self.actor_frozen(id) {
            return Ok(());
        }
        if !self.world.has_concealed_features() && actor.trap_contacts.ids().next().is_none() {
            return Ok(());
        }
        let hovering = actor.locomotion().hovers();
        let body_normal = imposed_normal.map(|normal| {
            tor_world::rotate_vector(tor_world::inverse_rotation(actor.orientation), normal)
        });
        let cells = self
            .body_cells(actor.location, actor.orientation, &actor.body)
            .ok_or(GameError::InvalidLocation)?;
        let mut contacts = BTreeSet::new();
        for &(at, frame) in &cells {
            let supported = self
                .world
                .physics_neighbor(at, Direction::Down)
                .is_some_and(|(below, _)| self.world.is_wall(below));
            let imposed = body_normal
                .is_some_and(|normal| tor_world::rotate_vector(frame, normal) == [0, 0, 1]);
            let velocity = tor_world::rotate_vector(frame, actor.motion.velocity);
            let displacement = tor_world::rotate_vector(frame, actor.motion.displacement);
            if supported && (imposed || (!hovering && velocity[2] == 0 && displacement[2] == 0)) {
                if let Some(trap) = self.world.trap(at) {
                    contacts.insert(trap.id);
                }
            }
        }
        let previous: BTreeSet<_> = actor.trap_contacts.ids().collect();
        let fresh: Vec<_> = contacts
            .difference(&previous)
            .copied()
            .filter(|trap| {
                self.trap_resolution
                    .as_ref()
                    .is_none_or(|chain| !chain.visited.contains(&(id, *trap)))
            })
            .collect();
        let location = actor.location;
        let orientation = actor.orientation;
        let mut members = BTreeSet::new();
        for trap in &fresh {
            let at = self
                .world
                .trap_location(*trap)
                .ok_or(GameError::InvalidLocation)?;
            members.extend(self.trap_required_regions(
                location.region,
                &self.world.trap(at).ok_or(GameError::InvalidLocation)?.kind,
            ));
        }
        if members
            .iter()
            .any(|region| self.world.region(*region).is_none())
        {
            if !self.host_region_continuation {
                return Err(GameError::InvalidLocation);
            }
            let boundary = TrapRegionBoundary {
                actor: id,
                location,
                orientation,
                avoidable_traps,
                imposed_normal,
                members,
            };
            if self
                .trap_region_boundary
                .as_ref()
                .is_some_and(|old| old != &boundary)
            {
                return Err(GameError::InvalidLocation);
            }
            self.trap_region_boundary = Some(boundary);
            return Ok(());
        }
        self.actors
            .get_mut(&id)
            .expect("validated actor")
            .trap_contacts = Shared::new(TrapContacts(contacts.into_iter().collect()));
        for trap in fresh {
            let at = self
                .world
                .trap_location(trap)
                .ok_or(GameError::InvalidLocation)?;
            let mut observers = Vec::new();
            for (&observer, owner) in self.actors.iter() {
                if !owner.alive()
                    || self.actor_frozen(observer)
                    || !self.condition_allows_voluntary_input(observer)
                {
                    continue;
                }
                if observer == id
                    || (owner.perception().sight
                        && self
                            .scene_snapshot(observer)?
                            .1
                            .iter()
                            .any(|cell| cell.physical && cell.location == at))
                {
                    observers.push(observer);
                }
            }
            let chain = self
                .trap_resolution
                .get_or_insert_with(TrapResolution::default);
            if chain.visited.len() >= MAX_CHAIN_CONTACTS {
                return Err(GameError::Rule(crate::abilities::RuleError::NumericBudget));
            }
            if chain.visited.insert((id, trap)) {
                chain
                    .pending
                    .push_back((id, trap, at, observers, avoidable_traps.contains(&trap)));
            }
        }
        if self
            .trap_resolution
            .as_ref()
            .is_none_or(|chain| chain.processing)
        {
            return Ok(());
        }
        self.trap_resolution.as_mut().expect("chain").processing = true;
        let result = self.drain_trap_contacts();
        if self.trap_region_boundary.is_none() {
            self.trap_resolution = None;
        } else if let Some(chain) = self.trap_resolution.as_mut() {
            chain.processing = false;
        }
        result
    }

    fn drain_trap_contacts(&mut self) -> Result<(), GameError> {
        while let Some((actor, id, at, observers, may_avoid)) = self
            .trap_resolution
            .as_mut()
            .and_then(|chain| chain.pending.pop_front())
        {
            if !self.actors.get(&actor).is_some_and(|owner| {
                owner.alive() && owner.trap_contacts.ids().any(|trap| trap == id)
            }) {
                continue;
            }
            let trap = self
                .world
                .trap(at)
                .filter(|trap| trap.id == id)
                .ok_or(GameError::InvalidLocation)?;
            let kind = trap.kind.clone();
            let avoidance_dc = trap.avoidance_dc.filter(|_| {
                may_avoid
                    && self.actors[&actor].combat.is_some()
                    && !matches!(kind, TrapKind::TrapDoor { open: true, .. })
            });
            if let Some(dc) = avoidance_dc {
                if self.resolve_hazard_skill_check_in_transaction(
                    actor,
                    crate::attributes::Skill::Agility,
                    dc,
                )? {
                    // The paid entry and contact receipt remain, but no payload
                    // or public family disclosure happened. Standing is no retry.
                    continue;
                }
            }
            self.apply_trap_contact(actor, at, kind)?;
            for observer in observers {
                self.reveal_feature_family(observer, ConcealedFeatureId::Trap(id))?;
            }
            if self.trap_region_boundary.is_some() {
                break;
            }
        }
        Ok(())
    }

    fn apply_trap_contact(
        &mut self,
        actor: ActorId,
        at: Location,
        kind: TrapKind,
    ) -> Result<(), GameError> {
        use crate::rules::{AmountExpr, Effect};
        // Exploration bodies are valid contact owners, but have no creature
        // checks, injury pool or condition/equipment-effect recipient. Keep
        // topology, relocation, discovery and real missed-arrow stock intact.
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        // World construction validates geometry, not the pinned creature
        // catalog. Ineligible recipients must not hide malformed content.
        if !trap_content_valid(&self.content, &kind) {
            return Err(GameError::InvalidLocation);
        }
        let has_creature = owner.combat.is_some();
        match kind {
            TrapKind::Rust { slot, amount } if has_creature => {
                self.apply_effects_in_transaction(
                    actor,
                    &[Effect::CorrodeArmor {
                        slot: crate::EquipmentSlot::new(slot)
                            .map_err(|_| GameError::InvalidLocation)?,
                        amount,
                    }],
                )?;
            }
            TrapKind::SleepingGas {
                condition,
                duration,
                exposure,
            } if has_creature => {
                let descriptors = exposure
                    .map(crate::content_ids::DescriptorId::new)
                    .transpose()
                    .map_err(|_| GameError::InvalidLocation)?
                    .into_iter()
                    .collect::<Vec<_>>();
                self.apply_exposure_in_transaction(
                    actor,
                    &[Effect::ApplyCondition {
                        condition: crate::content_ids::ConditionId::new(condition)
                            .map_err(|_| GameError::InvalidLocation)?,
                        duration,
                    }],
                    &descriptors,
                )?;
            }
            TrapKind::BearTrap {
                condition,
                duration,
            } if has_creature => {
                let condition = crate::content_ids::ConditionId::new(condition)
                    .map_err(|_| GameError::InvalidLocation)?;
                self.apply_effects_in_transaction(
                    actor,
                    &[Effect::ApplyCondition {
                        condition: condition.clone(),
                        duration,
                    }],
                )?;
                let mut owner = self.actors.get_mut(&actor).ok_or(GameError::UnknownActor)?;
                if owner
                    .combat
                    .as_ref()
                    .is_some_and(|combat| combat.creature().conditions().has_active(&condition))
                {
                    let entries = &mut owner.spatial_condition_anchors.0;
                    match entries.binary_search_by(|entry| entry.condition.cmp(&condition)) {
                        Ok(index) => entries[index].location = at,
                        Err(index) if entries.len() < 64 => entries.insert(
                            index,
                            SpatialConditionAnchor {
                                condition,
                                location: at,
                            },
                        ),
                        Err(_) => {
                            return Err(GameError::Rule(crate::abilities::RuleError::NumericBudget))
                        }
                    }
                }
            }
            TrapKind::TrapDoor { .. } => {
                self.world
                    .open_trapdoor(at)
                    .map_err(|_| GameError::InvalidLocation)?;
                self.reconcile_trap_contacts(actor, None)?;
            }
            TrapKind::Teleport {} => {
                let mut rng = self.rules_rng;
                self.relocate_random_in_transaction(actor, &mut rng, false)?;
                self.rules_rng = rng;
            }
            TrapKind::Arrow {
                attack_bonus,
                ammunition,
            } => {
                if !has_creature
                    || !self.resolve_environmental_attack_in_transaction(
                        actor,
                        attack_bonus,
                        &[trap_damage(6)],
                        &[],
                    )?
                {
                    let spec = self
                        .content
                        .trap_ammunition(&ammunition)
                        .ok_or(GameError::InvalidLocation)?
                        .clone();
                    self.place_item_stack(self.next_item_id, at, None, 1, spec)?;
                }
            }
            TrapKind::PoisonDart {
                attack_bonus,
                exposure,
            } if has_creature => {
                let hit = self.resolve_environmental_attack_in_transaction(
                    actor,
                    attack_bonus,
                    &[trap_damage(4)],
                    &[],
                )?;
                if hit && self.actors.get(&actor).is_some_and(|owner| owner.alive()) {
                    self.apply_exposure_in_transaction(
                        actor,
                        &[Effect::LoseAttribute {
                            attribute: crate::attributes::Attribute::Strength,
                            amount: AmountExpr::Fixed { amount: 1 },
                        }],
                        &[crate::content_ids::DescriptorId::new(exposure)
                            .map_err(|_| GameError::InvalidLocation)?],
                    )?;
                }
            }
            TrapKind::Rust { .. }
            | TrapKind::SleepingGas { .. }
            | TrapKind::BearTrap { .. }
            | TrapKind::PoisonDart { .. } => {}
            // Its discovery/contact is real, but a mysterious trap does not
            // invent a damaging, summoning or information-revealing mechanic.
            TrapKind::Mysterious {} => {}
        }
        Ok(())
    }
}
fn trap_damage(sides: u16) -> crate::rules::Effect {
    use crate::rules::{AmountExpr, DamageComponentExpr, DamageExpr, DiceExpr, Effect};
    Effect::Damage {
        damage: DamageExpr::Components {
            components: vec![DamageComponentExpr {
                category: crate::combat::DamageType::Keen,
                descriptor: None,
                amount: AmountExpr::Rolled {
                    dice: DiceExpr {
                        count: 1,
                        sides,
                        bonus: 0,
                    },
                },
            }],
        },
    }
}

#[cfg(test)]
#[path = "traps_validation_tests.rs"]
mod validation_tests;

#[cfg(test)]
#[path = "traps_hazards_tests.rs"]
mod hazards_tests;
