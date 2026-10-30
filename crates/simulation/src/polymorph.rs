//! Atomic baleful body changes. Spell delivery and paid selection are separate.
use crate::actor_creatures::CombatState;
use crate::creatures::{BodyPackage, BuildError};
use crate::health_recovery::RecoveryContext;
use crate::lifecycle::ActorOutcome;
use crate::{ActorId, Game, GameError};
use std::collections::BTreeMap;
use std::sync::Arc;
use tor_world::Shared;

/// Only an in-flight physics tick/host continuation owns this bounded queue.
/// It is never serialized and must be empty at a public checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct BodyReturnFinalization {
    pub(crate) deferring: bool,
    queued: BTreeMap<ActorId, u64>,
    authority: Option<BodyReturnAuthority>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BodyReturnAuthority {
    actor: ActorId,
    location: tor_world::Location,
    orientation: u8,
    clock: u64,
}
impl BodyReturnFinalization {
    pub(crate) fn settled(&self) -> bool {
        !self.deferring && self.queued.is_empty() && self.authority.is_none()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyTransitionResult {
    Changed,
    Unchanged,
    /// A paid application may fail to fit without refunding or rerolling.
    Blocked,
}
impl Game {
    pub fn polymorph_creature(
        &mut self,
        actor: ActorId,
        package: Arc<BodyPackage>,
    ) -> Result<BodyTransitionResult, GameError> {
        self.with_effect_publication(|game| {
            let mut outcome = game.settled_actor_outcome(actor)?;
            let result = game.stage_body_transition(&mut outcome, package)?;
            if result == BodyTransitionResult::Changed {
                let prepared = game.prepare_actor_outcome(outcome)?;
                game.publish_actor_outcome(prepared)?;
            }
            Ok(result)
        })
    }

    /// Only assign the candidate after all geometry, equipment and derivation
    /// checks succeed. A caller may keep its already-paid effect on Blocked.
    pub(crate) fn stage_body_transition(
        &self,
        outcome: &mut ActorOutcome,
        package: Arc<BodyPackage>,
    ) -> Result<BodyTransitionResult, GameError> {
        package.validate(&self.content).map_err(GameError::Build)?;
        if self
            .content
            .body_package(&package.id)
            .is_none_or(|authored| authored.as_ref() != package.as_ref())
        {
            return Err(GameError::Build(BuildError::InvalidDefinition));
        }
        let actor = &outcome.state;
        let combat = actor.combat.as_ref().ok_or(GameError::UnknownActor)?;
        if !actor.alive() {
            return Ok(BodyTransitionResult::Unchanged);
        }
        if combat.creature().build().content().as_ref() != self.content.as_ref() {
            return Err(GameError::Build(BuildError::InvalidDefinition));
        }
        if combat
            .creature()
            .build()
            .active_body_package()
            .is_some_and(|active| active.as_ref() == package.as_ref())
        {
            return Ok(BodyTransitionResult::Unchanged);
        }
        // Cross-package loss redistribution is deliberately unsupported. It is
        // a paid no-change boundary, not a malformed-definition rollback.
        if combat.creature().build().body_package_loss_count().get() > 0 {
            return Ok(BodyTransitionResult::Blocked);
        }
        if !self.body_fits(outcome.id, actor.location, actor.orientation, &package.body) {
            return Ok(BodyTransitionResult::Blocked);
        }
        let build = combat
            .creature()
            .build()
            .replace_body_package(package.clone())
            .map_err(GameError::Build)?;
        let permanent = build.derive().map_err(GameError::Build)?;
        let anatomy = permanent.anatomy.clone();
        let mut equipment = BTreeMap::new();
        // Existing socket identity must still denote the same kind. No implicit
        // equipment remapping, curse checks, transfer, duplication or destruction.
        for (socket, item) in &actor.equipment {
            let index = usize::from(socket.0);
            if actor.anatomy.slots.get(index) != anatomy.slots.get(index)
                || anatomy.slots.get(index).is_none()
            {
                continue;
            }
            equipment.insert(*socket, *item);
            if self
                .equipment_capacity_state_for_facts(
                    outcome.id,
                    &anatomy,
                    &permanent.expertises,
                    &equipment,
                )
                .is_none_or(|deficits| !deficits.is_empty())
            {
                equipment.remove(socket);
            }
        }
        let sources = equipment
            .iter()
            .map(|(socket, id)| {
                self.items
                    .get(id)
                    .and_then(|item| item.equipment_source(*id, *socket, &self.content.rules))
                    .ok_or(GameError::ItemUnavailable)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut creature = combat.creature().clone();
        creature
            .stage_body_rebuild_with_equipment(build, sources)
            .map_err(GameError::Creature)?;
        // Equipment-dependent expertise cannot silently make a retained kit
        // structurally invalid. Unsupported circular capacity changes reject.
        if self
            .equipment_capacity_state_for_facts(
                outcome.id,
                &anatomy,
                &creature.derived().expertises,
                &equipment,
            )
            .is_none_or(|deficits| !deficits.is_empty())
        {
            return Err(GameError::Build(BuildError::InvalidDefinition));
        }
        let extinguish = actor
            .equipment
            .values()
            .filter(|item| {
                !equipment.values().any(|retained| retained == *item)
                    && self
                        .items
                        .get(*item)
                        .is_some_and(|item| item.fuel.is_some_and(|fuel| fuel.lit))
            })
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let mut candidate = actor.clone();
        candidate.anatomy = Shared::new(anatomy);
        candidate.equipment = equipment;
        if *candidate.body != package.body {
            candidate.readied_ambush.retire();
        }
        candidate.body = Shared::new(package.body.clone());
        candidate.motion.acceleration_remainder = [0; 3];
        // Keep continuing forced-fall attribution; the publisher retires it
        // only when the final geometry/locomotion ends the causal episode.
        candidate.combat = Some(CombatState::from_creature(
            combat.identity.clone(),
            creature.clone(),
        ));
        if !creature.health().dead()
            && creature.derived().contextual_health_recovery()
            && (!self.actor_frozen(outcome.id)
                || self.body_return_continuation_authorized(outcome.id))
        {
            // A read-only candidate scene uses the NEW eyes/body/capabilities,
            // never the published predecessor's perception. No lifecycle is run
            // in this projection and no elapsed time belongs to the new form.
            let mut projection = self.clone();
            projection.actors.insert(outcome.id, candidate.clone());
            for item in outcome.extinguish_lights.iter().chain(extinguish.iter()) {
                projection.set_item_light(*item, false)?;
            }
            let sensed_hostile = projection.senses_hostile(outcome.id)?;
            creature.resolve_body_recovery_context(RecoveryContext::Known { sensed_hostile });
            candidate.combat = Some(CombatState::from_creature(
                combat.identity.clone(),
                creature,
            ));
        }
        outcome.extinguish_lights.extend(extinguish);
        outcome.state = candidate;
        Ok(BodyTransitionResult::Changed)
    }

    /// Suppression plus a legal return has one final Health boundary. Returning
    /// None delegates ordinary/blocked Cancellation to its existing path.
    pub(crate) fn stage_cancellation_return(
        &self,
        outcome: &mut ActorOutcome,
    ) -> Result<Option<bool>, GameError> {
        let Some(combat) = &outcome.state.combat else {
            return Ok(None);
        };
        let Some((base, active)) = combat
            .creature()
            .build()
            .base_body_package()
            .zip(combat.creature().build().active_body_package())
        else {
            return Ok(None);
        };
        if base.id == active.id
            || !outcome.state.alive()
            || !self.body_fits(
                outcome.id,
                outcome.state.location,
                outcome.state.orientation,
                &base.body,
            )
        {
            return Ok(None);
        }
        let base = base.clone();
        let mut creature = combat.creature().clone();
        creature
            .stage_cancellation_inputs()
            .map_err(GameError::Creature)?;
        let mut candidate = outcome.state.clone();
        candidate.combat = Some(CombatState::from_creature(
            combat.identity.clone(),
            creature,
        ));
        let original = std::mem::replace(&mut outcome.state, candidate);
        match self.stage_body_transition(outcome, base) {
            Ok(BodyTransitionResult::Changed) => Ok(Some(true)),
            Ok(_) => {
                outcome.state = original;
                Ok(None)
            }
            Err(error) => {
                outcome.state = original;
                Err(error)
            }
        }
    }

    pub(crate) fn stage_polymorph_return(
        &self,
        outcome: &mut ActorOutcome,
    ) -> Result<bool, GameError> {
        let Some(combat) = &outcome.state.combat else {
            return Ok(false);
        };
        let Some(base) = combat.creature().build().base_body_package().cloned() else {
            return Ok(false);
        };
        if !outcome.state.alive() {
            self.set_return_pending(outcome, false)?;
            return Ok(false);
        }
        let result = self.stage_body_transition(outcome, base)?;
        self.set_return_pending(outcome, result == BodyTransitionResult::Blocked)?;
        Ok(result == BodyTransitionResult::Changed)
    }
    pub(crate) fn set_return_pending(
        &self,
        outcome: &mut ActorOutcome,
        pending: bool,
    ) -> Result<(), GameError> {
        let Some(combat) = &outcome.state.combat else {
            return Ok(());
        };
        if combat.creature().polymorph_return_pending() == pending {
            return Ok(());
        }
        let mut creature = combat.creature().clone();
        creature
            .set_polymorph_return_pending(pending)
            .map_err(GameError::Creature)?;
        outcome.state.combat = Some(CombatState::from_creature(
            combat.identity.clone(),
            creature,
        ));
        Ok(())
    }
    pub(crate) fn body_return_continuation_authorized(&self, id: ActorId) -> bool {
        self.body_return_finalization
            .authority
            .is_some_and(|authority| {
                authority.actor == id
                    && authority.clock == self.tick
                    && self.actor_clock(id) == authority.clock
                    && self.actors.get(&id).is_some_and(|actor| {
                        actor.location == authority.location
                            && actor.orientation == authority.orientation
                    })
            })
    }

    /// Complete only the captured active actor's in-flight return. Frozen clock
    /// stamps and timer membership are untouched; no neighboring actor receives
    /// this authority. Ordinary lifecycle synchronization runs after the scope.
    pub(crate) fn with_body_return_authority<T>(
        &mut self,
        id: ActorId,
        clock: u64,
        returned_contact: bool,
        change: impl FnOnce(&mut Self) -> Result<T, GameError>,
    ) -> Result<T, GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if clock != self.tick
            || self.actor_clock(id) != clock
            || !actor.alive()
            || !actor.combat.as_ref().is_some_and(|combat| {
                let creature = combat.creature();
                if returned_contact {
                    !creature.polymorph_return_pending()
                        && creature
                            .build()
                            .base_body_package()
                            .zip(creature.build().active_body_package())
                            .is_some_and(|(base, active)| base.id == active.id)
                } else {
                    creature.polymorph_return_pending()
                }
            })
        {
            return Err(GameError::InvalidLocation);
        }
        let authority = BodyReturnAuthority {
            actor: id,
            location: actor.location,
            orientation: actor.orientation,
            clock,
        };
        let previous = self.body_return_finalization.authority.replace(authority);
        let result = change(self);
        self.body_return_finalization.authority = previous;
        let value = result?;
        self.sync_actor_lifecycle(id)?;
        Ok(value)
    }

    /// A nested trap may commit a real relocation of the same continuation
    /// owner. Follow that committed anchor only; unrelated actors and stale
    /// prior placements cannot inherit the authority.
    pub(crate) fn relocate_body_return_authority(
        &mut self,
        id: ActorId,
        from: tor_world::Location,
        frame: u8,
    ) {
        let Some(authority) = self.body_return_finalization.authority.as_mut() else {
            return;
        };
        if authority.actor == id
            && authority.location == from
            && authority.orientation == frame
            && authority.clock == self.tick
        {
            if let Some(actor) = self.actors.get(&id) {
                authority.location = actor.location;
                authority.orientation = actor.orientation;
            }
        }
    }

    pub(crate) fn with_deferred_body_returns<T>(
        &mut self,
        change: impl FnOnce(&mut Self) -> Result<T, GameError>,
    ) -> Result<T, GameError> {
        if self.body_return_finalization.deferring {
            return change(self);
        }
        self.body_return_finalization.deferring = true;
        let result = change(self);
        self.body_return_finalization.deferring = false;
        match result {
            Ok(value) => {
                if let Err(error) = self.flush_pending_body_returns() {
                    self.body_return_finalization.queued.clear();
                    return Err(error);
                }
                Ok(value)
            }
            Err(error) => {
                self.body_return_finalization.queued.clear();
                Err(error)
            }
        }
    }

    pub(crate) fn flush_pending_body_returns(&mut self) -> Result<(), GameError> {
        if self.body_return_finalization.deferring {
            return Ok(());
        }
        while !self.trap_contacts_unsettled() {
            let Some((id, clock)) = self.body_return_finalization.queued.pop_first() else {
                break;
            };
            if self.actors.get(&id).is_some_and(|actor| {
                actor.alive()
                    && actor
                        .combat
                        .as_ref()
                        .is_some_and(|combat| combat.creature().polymorph_return_pending())
            }) {
                self.with_body_return_authority(id, clock, false, |game| {
                    game.finish_pending_body_return(id)
                })?;
            }
        }
        Ok(())
    }

    /// The old form's placement, held state, guise and complete trap chain
    /// settle first. Residency only delays this final return, never replays
    /// the selected movement or advances another actor while it is pending.
    pub(crate) fn finish_pending_body_return(&mut self, id: ActorId) -> Result<(), GameError> {
        if (self.actor_frozen(id) && !self.body_return_continuation_authorized(id))
            || !self.actors.get(&id).is_some_and(|actor| {
                actor.alive()
                    && actor
                        .combat
                        .as_ref()
                        .is_some_and(|combat| combat.creature().polymorph_return_pending())
            })
        {
            return Ok(());
        }
        if self.body_return_finalization.deferring {
            let clock = self.actor_clock(id);
            self.body_return_finalization
                .queued
                .entry(id)
                .or_insert(clock);
            return Ok(());
        }
        if self.trap_contacts_unsettled() {
            return Ok(());
        }
        let members = self.pending_body_return_at_current_regions(id);
        if members
            .iter()
            .any(|region| self.world.region(*region).is_none())
        {
            if !self.host_region_continuation {
                return Err(GameError::InvalidLocation);
            }
            let actor = &self.actors[&id];
            self.trap_region_boundary = Some(crate::traps::TrapRegionBoundary {
                phase: crate::traps::TrapRegionPhase::PostContactBodyReturn,
                active_clock: Some(self.actor_clock(id)),
                actor: id,
                location: actor.location,
                orientation: actor.orientation,
                avoidable_traps: Default::default(),
                imposed_normal: None,
                members,
            });
            return Ok(());
        }
        let mut outcome = self.settled_actor_outcome(id)?;
        if !outcome.state.alive() {
            self.set_return_pending(&mut outcome, false)?;
            return self.apply_outcome(outcome);
        }
        if self.stage_polymorph_return(&mut outcome)? {
            let prepared = self.prepare_actor_outcome(outcome)?;
            self.publish_actor_outcome(prepared)?;
        }
        Ok(())
    }
}

// Residency is a pre-payment dependency, not a fit verdict. Resolve the same
// bounded cardinal offset paths as rigid bodies, recording a known destination
// before a missing slice stops that path. The host's load-only continuation
// repeats this probe; it never selects a form or activates its population.
fn body_footprint_regions(
    world: &tor_world::World,
    at: tor_world::Location,
    frame: u8,
    body: &crate::BodySpec,
) -> std::collections::BTreeSet<tor_world::RegionId> {
    type Cell = (tor_world::Location, u8);
    fn resolve(
        world: &tor_world::World,
        offset: [i32; 3],
        cache: &mut BTreeMap<[i32; 3], Option<Cell>>,
        regions: &mut std::collections::BTreeSet<tor_world::RegionId>,
    ) -> Option<Cell> {
        if let Some(cell) = cache.get(&offset) {
            return *cell;
        }
        let mut result = None;
        for axis in 0..3 {
            if offset[axis] == 0 {
                continue;
            }
            let mut previous = offset;
            previous[axis] -= offset[axis].signum();
            let Some((from, frame)) = resolve(world, previous, cache, regions) else {
                continue;
            };
            let mut delta = [0; 3];
            delta[axis] = i64::from(offset[axis].signum());
            let Some(direction) =
                tor_world::Direction::from_delta(tor_world::rotate_vector(frame, delta))
            else {
                continue;
            };
            let Some((to, turn)) = world.physics_neighbor(from, direction) else {
                continue;
            };
            if world.known_region(to.region).is_some() {
                regions.insert(to.region);
            }
            if world.contains(to) {
                // Conflicting paths cannot fit; retaining either path here is
                // sufficient for residency and leaves legality to resolve_body.
                result.get_or_insert((to, tor_world::compose_rotation(frame, turn)));
            }
        }
        cache.insert(offset, result);
        result
    }
    let mut regions = std::collections::BTreeSet::from([at.region]);
    let mut cache = BTreeMap::from([([0; 3], Some((at, frame)))]);
    for offset in &body.cells {
        resolve(world, *offset, &mut cache, &mut regions);
    }
    regions
}

impl Game {
    pub(crate) fn pending_body_return_at_current_regions(
        &self,
        id: ActorId,
    ) -> std::collections::BTreeSet<tor_world::RegionId> {
        let Some(actor) = self.actors.get(&id) else {
            return Default::default();
        };
        let Some(creature) = actor
            .combat
            .as_ref()
            .map(|combat| combat.creature())
            .filter(|creature| creature.polymorph_return_pending())
        else {
            return Default::default();
        };
        creature
            .build()
            .base_body_package()
            .map_or_else(Default::default, |base| {
                body_footprint_regions(&self.world, actor.location, actor.orientation, &base.body)
            })
    }

    pub(crate) fn pending_body_return_regions(
        &self,
        id: ActorId,
    ) -> std::collections::BTreeSet<tor_world::RegionId> {
        let mut regions = std::collections::BTreeSet::new();
        let Some(actor) = self.actors.get(&id) else {
            return regions;
        };
        let Some(creature) = actor
            .combat
            .as_ref()
            .map(|combat| combat.creature())
            .filter(|creature| creature.polymorph_return_pending())
        else {
            return regions;
        };
        let Some(base) = creature.build().base_body_package() else {
            return regions;
        };
        let mut placements =
            std::collections::BTreeSet::from([(actor.location, actor.orientation)]);
        // Ordinary immediate movement can be admitted without a preparation.
        // Probe its finite neighboring placements, never neighboring populations.
        for x in -1..=1 {
            for y in -1..=1 {
                for z in -1..=1 {
                    let Some(direction) = tor_world::Direction::from_delta([x, y, z]) else {
                        continue;
                    };
                    if let Some(context) = self.movement_context(id, direction) {
                        placements
                            .insert((context.destination(), context.destination_orientation()));
                    }
                }
            }
        }
        for intention in self
            .queued_intentions()
            .filter(|intention| intention.actor == id)
        {
            if let Some(context) = intention.movement_context {
                placements.insert((context.destination(), context.destination_orientation()));
            }
        }
        if let Some(step) = actor
            .pending
            .as_ref()
            .and_then(|preparation| preparation.plan.as_ability())
            .and_then(|ability| ability.step)
        {
            placements.insert((step.destination(), step.context.destination_orientation()));
        }
        for (at, frame) in placements {
            regions.extend(body_footprint_regions(&self.world, at, frame, &base.body));
        }
        regions
    }

    pub(crate) fn body_effect_required_regions(
        &self,
        target: ActorId,
        effects: &[crate::abilities::PlannedEffect],
    ) -> Result<std::collections::BTreeSet<tor_world::RegionId>, GameError> {
        let mut regions = std::collections::BTreeSet::new();
        if !effects.iter().any(|effect| {
            matches!(
                effect,
                crate::abilities::PlannedEffect::BalefulPolymorph { .. }
                    | crate::abilities::PlannedEffect::CancelSpecials
            )
        }) {
            return Ok(regions);
        }
        let Some(actor) = self.actors.get(&target) else {
            return Ok(regions);
        };
        let Some(creature) = actor.combat.as_ref().map(|combat| combat.creature()) else {
            return Ok(regions);
        };
        let Some(active) = creature.build().active_body_package() else {
            return Ok(regions);
        };
        for effect in effects {
            match effect {
                crate::abilities::PlannedEffect::BalefulPolymorph { forms } => {
                    for id in forms.iter().filter(|id| **id != active.id) {
                        let package = self
                            .content
                            .polymorph_body_package(id)
                            .ok_or(GameError::Rule(crate::abilities::RuleError::InvalidPlan))?;
                        regions.extend(body_footprint_regions(
                            &self.world,
                            actor.location,
                            actor.orientation,
                            &package.body,
                        ));
                    }
                }
                crate::abilities::PlannedEffect::CancelSpecials => {
                    if let Some(base) = creature
                        .build()
                        .base_body_package()
                        .filter(|base| base.id != active.id)
                    {
                        regions.extend(body_footprint_regions(
                            &self.world,
                            actor.location,
                            actor.orientation,
                            &base.body,
                        ));
                    }
                }
                _ => {}
            }
        }
        Ok(regions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attributes::{ManaBinding, Skill};
    use crate::content_ids::{BodyFormId, EquipmentSlot};
    use crate::creatures::{
        Content, CreatureBuild, HdChoices, PlannedAdvancement, ScenarioCatalog,
    };
    use crate::progression::{HdLedger, HdSource};
    use crate::{
        AnatomySpec, BodySpec, CreatureIdentity, EquipmentSlotId, EquipmentSpec, ItemClass,
        ItemLocation, ItemSpec,
    };
    use std::num::NonZeroU64;
    use tor_world::{Location, Position, RegionId};

    fn at(x: i32) -> Location {
        Location {
            region: RegionId(1),
            position: Position { x, y: 1, z: 0 },
        }
    }
    fn fixture() -> (
        Game,
        ActorId,
        Arc<BodyPackage>,
        Arc<BodyPackage>,
        Arc<BodyPackage>,
    ) {
        fixture_with_hit_dice(2)
    }
    fn fixture_with_hit_dice(
        total_hd: usize,
    ) -> (
        Game,
        ActorId,
        Arc<BodyPackage>,
        Arc<BodyPackage>,
        Arc<BodyPackage>,
    ) {
        let base_species = crate::test_creatures::species();
        let mut weak = crate::test_creatures::unmodified_species("polymorph_weak");
        weak.anatomy = AnatomySpec::default();
        let mut strong = weak.clone();
        strong.id = crate::content_ids::SpeciesId::new("polymorph_strong").unwrap();
        strong
            .modifiers
            .push(crate::test_creatures::add(crate::rules::Stat::Health, 100));
        strong.modifiers[0].cancellable = true;
        let mut rules = crate::test_creatures::ruleset().document().clone();
        rules.conditions.push(crate::rules::ConditionDef {
            detecting_monsters: false,
            hallucinating: false,
            id: crate::content_ids::ConditionId::new("body_tail_health").unwrap(),
            family: crate::content_ids::ConditionFamilyId::new("body_tail_health").unwrap(),
            lifecycle: crate::rules::ConditionLifecycle::Timed,
            timing: crate::rules::ConditionTiming::expiring(),
            restrictions: vec![],
            preparation: crate::rules::ConditionPreparation::Preserve,
            name: "Tail settlement health".into(),
            modifiers: vec![crate::test_creatures::add(crate::rules::Stat::Health, 20)],
            stacking: crate::rules::Stacking::Longest,
            source_scoped: true,
            descriptors: vec![],
            capacity: crate::rules::Eviction::Oldest,
        });
        let content = Content::new(
            crate::rules::Ruleset::validate(rules).unwrap(),
            ScenarioCatalog {
                species: [base_species.clone(), weak.clone(), strong.clone()]
                    .into_iter()
                    .map(|s| (s.id.clone(), s))
                    .collect(),
                templates: BTreeMap::new(),
            },
        )
        .unwrap();
        let package = |id: &str, species, body| {
            Arc::new(BodyPackage {
                id: BodyFormId::new(id).unwrap(),
                species,
                body,
                advancement: vec![PlannedAdvancement {
                    source: HdSource::Racial,
                    choices: HdChoices::default(),
                }],
            })
        };
        let base = package(
            "polymorph_base",
            base_species.id.clone(),
            BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                ..BodySpec::default()
            },
        );
        let weak = package("polymorph_weak", weak.id, BodySpec::default());
        let mut strong = package("polymorph_strong", strong.id, BodySpec::default());
        // Two package rows allow a living package-loss boundary witness.
        Arc::make_mut(&mut strong)
            .advancement
            .push(PlannedAdvancement {
                source: HdSource::Racial,
                choices: HdChoices::default(),
            });
        let content = Content::with_body_packages(
            &content,
            [base.clone(), weak.clone(), strong.clone()]
                .into_iter()
                .map(|p| (p.id.clone(), p))
                .collect(),
        )
        .unwrap();
        let mut arrow = ItemSpec::ordinary("body boundary arrow".into());
        arrow.stackable = true;
        arrow.class = ItemClass::Weapon;
        let component =
            crate::damage::DamageComponent::fixed(crate::combat::DamageType::Keen, None, 3);
        let primary = component.key();
        arrow.projectile = Some(crate::projectile_profiles::ProjectileSpec {
            family: crate::content_ids::LauncherFamilyId::new("arrow").unwrap(),
            damage: crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
            recovery: crate::projectile_profiles::HitRecovery::ConsumedOnHit,
        });
        let content = Content::with_trap_ammunition(
            &content,
            BTreeMap::from([("polymorph.arrow".into(), arrow)]),
        )
        .unwrap();
        let build = CreatureBuild::new(
            content.clone(),
            base_species.id,
            HdLedger::seeded(vec![HdSource::Racial; total_hd], 42).unwrap(),
            ManaBinding::Intellect,
            [Skill::Deception, Skill::Persuasion],
        )
        .unwrap()
        .with_body_package(base.clone(), 42)
        .unwrap();
        let mut game = Game::two_room_in_stone(42, content);
        let actor = game
            .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(actor, base.body.clone()).unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: "same person".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        (game, actor, base, weak, strong)
    }
    fn ring(game: &mut Game, actor: ActorId, health: i64) -> crate::ItemId {
        let slot = EquipmentSlot::new("ring").unwrap();
        let socket = EquipmentSlotId(
            game.actors[&actor]
                .anatomy
                .slots
                .iter()
                .position(|s| *s == slot)
                .unwrap() as u16,
        );
        let mut spec = ItemSpec::ordinary("cursed body witness".into());
        spec.class = ItemClass::Ring;
        spec.equipment = Some(EquipmentSpec {
            periodic_relocation: None,
            slot,
            armor: None,
            attack: None,
            weapon_profile: None,
            modifiers: vec![crate::test_creatures::add(
                crate::rules::Stat::Health,
                health,
            )],
            potency_modifiers: vec![],
        });
        let item = game
            .place_item_stack(game.next_item_id, at(1), Some(actor), 1, spec)
            .unwrap();
        game.items.edit(item, |item| item.condition.cursed = true);
        game.equip_authored(actor, item, socket).unwrap();
        item
    }
    #[test]
    fn removed_held_light_extinguishes_only_when_body_candidate_is_published() {
        let (mut game, actor, _, weak, _) = fixture();
        let slot = EquipmentSlot::new("held_light").unwrap();
        let socket = EquipmentSlotId(
            game.actors[&actor]
                .anatomy
                .slots
                .iter()
                .position(|s| *s == slot)
                .unwrap() as u16,
        );
        let mut spec = ItemSpec::ordinary("body transition torch".into());
        spec.class = ItemClass::Tool;
        spec.held_light = Some(crate::HeldLightSpec {
            radius: 2,
            fuel_ticks: 100,
        });
        spec.equipment = Some(EquipmentSpec {
            periodic_relocation: None,
            slot,
            armor: None,
            attack: None,
            weapon_profile: None,
            modifiers: vec![],
            potency_modifiers: vec![],
        });
        let item = game
            .place_item_stack(game.next_item_id, at(1), Some(actor), 1, spec)
            .unwrap();
        game.equip_authored(actor, item, socket).unwrap();
        game.set_item_light(item, true).unwrap();
        let before = game.clone();
        let mut outcome = game.actor_outcome(actor).unwrap();
        assert_eq!(
            game.stage_body_transition(&mut outcome, weak).unwrap(),
            BodyTransitionResult::Changed
        );
        assert_eq!(game, before, "preflight must not mutate item-owned fuel");
        assert!(outcome.extinguish_lights.contains(&item));
        let prepared = game.prepare_actor_outcome(outcome).unwrap();
        game.publish_actor_outcome(prepared).unwrap();
        assert!(!game.items[&item].fuel.unwrap().lit);
        assert_eq!(game.items[&item].fuel.unwrap().remaining, 100);
        assert_eq!(game.items[&item].location, ItemLocation::Carried(actor));
    }

    #[test]
    fn body_candidate_preserves_continuing_fall_causal_episode() {
        let (game, actor, _, weak, _) = fixture();
        let mut outcome = game.actor_outcome(actor).unwrap();
        let fall = crate::physics::ActiveFall {
            owner: actor,
            episode: NonZeroU64::MIN,
            started_at: game.tick,
        };
        outcome.state.motion.active_fall = Some(fall);
        outcome.state.motion.velocity = [0, 0, -100];
        assert_eq!(
            game.stage_body_transition(&mut outcome, weak).unwrap(),
            BodyTransitionResult::Changed
        );
        assert_eq!(outcome.state.motion.active_fall, Some(fall));
        assert_eq!(outcome.state.motion.velocity, [0, 0, -100]);
    }

    #[test]
    fn package_loss_blocks_transition_without_rolling_back_cancellation() {
        let (mut game, actor, base, _, strong) = fixture();
        game.polymorph_creature(actor, strong.clone()).unwrap();
        // Personal ownership drains first; the second loss belongs to the body.
        game.apply_effects(actor, &[crate::rules::Effect::DrainLatestHitDie])
            .unwrap();
        game.apply_effects(actor, &[crate::rules::Effect::DrainLatestHitDie])
            .unwrap();
        assert!(game.alive(actor));
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .build()
                .body_package_loss_count()
                .get(),
            1
        );
        let before = game.clone();
        assert_eq!(
            game.polymorph_creature(actor, base.clone()).unwrap(),
            BodyTransitionResult::Blocked
        );
        assert_eq!(game, before);
        game.apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
            .unwrap();
        let creature = game.creature(actor).unwrap();
        assert!(creature.cancelled());
        assert!(creature.polymorph_return_pending());
        assert_eq!(
            creature.build().active_body_package().unwrap().id,
            strong.id
        );
        assert_eq!(creature.build().body_package_loss_count().get(), 1);
        game.apply_effects(actor, &[crate::rules::Effect::RestoreHitDie])
            .unwrap();
        let creature = game.creature(actor).unwrap();
        assert_eq!(creature.build().active_body_package().unwrap().id, base.id);
        assert!(creature.cancelled());
        assert!(!creature.polymorph_return_pending());
    }

    #[test]
    fn final_body_and_cursed_equipment_transition_preserves_injury_without_intermediate_death() {
        let (mut game, actor, _, _, strong) = fixture();
        let item = ring(&mut game, actor, 60);
        crate::test_creatures::injure(&mut game, actor, 45).unwrap();
        let identity = game.actors[&actor]
            .combat
            .as_ref()
            .unwrap()
            .identity
            .clone();
        assert_eq!(
            game.polymorph_creature(actor, strong).unwrap(),
            BodyTransitionResult::Changed
        );
        let creature = game.creature(actor).unwrap();
        assert!(
            !creature.health().dead(),
            "only final body/equipment maximum may cause death"
        );
        assert_eq!(creature.health().injury(), 45);
        assert!(game.equipment(actor).unwrap().is_empty());
        assert_eq!(game.items[&item].location, ItemLocation::Carried(actor));
        assert!(game.items[&item].condition.cursed);
        assert_eq!(
            game.actors[&actor].combat.as_ref().unwrap().identity,
            identity
        );
    }
    #[test]
    fn blocked_cancellation_persists_then_returns_at_candidate_location_without_healing() {
        let (mut game, actor, base, weak, _) = fixture();
        game.polymorph_creature(actor, weak).unwrap();
        crate::test_creatures::injure(&mut game, actor, 2).unwrap();
        let blocker = game
            .spawn_actor(at(2), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
            .unwrap();
        assert!(game.creature(actor).unwrap().cancelled());
        assert!(game.creature(actor).unwrap().polymorph_return_pending());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
        let (snapshot, shared) = serde_json::from_slice(&bytes).unwrap();
        let mut game = Game::restore_checkpoint(snapshot, &shared, game.content.clone()).unwrap();
        game.teleport(actor, at(3)).unwrap();
        let creature = game.creature(actor).unwrap();
        assert_eq!(creature.build().active_body_package().unwrap().id, base.id);
        assert!(!creature.polymorph_return_pending());
        assert!(creature.cancelled());
        assert_eq!(creature.health().injury(), 2);
        assert_eq!(game.actors[&blocker].location, at(2));
    }
    #[test]
    fn cancellation_suppression_and_return_share_one_final_health_boundary() {
        let (mut game, actor, base, _, strong) = fixture();
        game.polymorph_creature(actor, strong).unwrap();
        crate::test_creatures::injure(&mut game, actor, 20).unwrap();
        game.apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
            .unwrap();
        let creature = game.creature(actor).unwrap();
        assert!(
            !creature.health().dead(),
            "suppressed intermediate shell must not kill before legal return"
        );
        assert!(creature.cancelled());
        assert_eq!(creature.health().injury(), 20);
        assert_eq!(creature.build().active_body_package().unwrap().id, base.id);
    }

    #[test]
    fn blocked_cast_is_atomic_and_frail_form_death_is_not_reversed() {
        let (mut game, actor, base, weak, _) = fixture();
        game.polymorph_creature(actor, weak.clone()).unwrap();
        game.spawn_actor(at(2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let before = game.clone();
        assert_eq!(
            game.polymorph_creature(actor, base).unwrap(),
            BodyTransitionResult::Blocked
        );
        assert_eq!(game, before);
        let (mut game, actor, _, weak, _) = fixture();
        crate::test_creatures::injure(&mut game, actor, 20).unwrap();
        assert!(game.alive(actor));
        game.polymorph_creature(actor, weak).unwrap();
        assert!(!game.alive(actor));
        let corpses = game
            .items
            .values()
            .filter(|item| item.corpse.is_some())
            .count();
        assert_eq!(corpses, 1);
        game.apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
            .unwrap();
        assert!(!game.alive(actor));
        assert_eq!(
            game.items
                .values()
                .filter(|item| item.corpse.is_some())
                .count(),
            corpses
        );
    }
    fn report_body_compile_work(
        case: &str,
        scope: &str,
        initial_hd: usize,
        before: crate::diagnostics::WorkCounts,
        after: crate::diagnostics::WorkCounts,
    ) {
        println!(
            "body_work_v1 {}",
            serde_json::json!({
                "case": case, "scope": scope, "initial_hd": initial_hd,
                "permanent_creature_compiles": after.permanent_creature_compiles - before.permanent_creature_compiles,
                "dynamic_creature_folds": after.dynamic_creature_folds - before.dynamic_creature_folds,
                "raw_hit_die_visits": after.raw_hit_die_visits - before.raw_hit_die_visits,
                "advancement_choice_visits": after.advancement_choice_visits - before.advancement_choice_visits,
                "creature_build_copies": after.creature_build_copies - before.creature_build_copies,
                "health_provenance_copies": after.health_provenance_copies - before.health_provenance_copies,
            })
        );
    }

    /// Legal builder-scaled fixtures, not claims about live Rogue populations.
    /// Separate plain/live-source rows retain the baseline-versus-effective fold
    /// distinction. Setup and independent state checks stay outside the counts.
    #[test]
    fn body_compile_work_successful_transition_baseline() {
        for total_hd in [2, 16, 64] {
            for live_sources in [false, true] {
                let (mut game, actor, _, _, strong) = fixture_with_hit_dice(total_hd);
                let personal =
                    game.creature(actor).unwrap().build().ledger().entries()[1..].to_vec();
                let item = live_sources.then(|| ring(&mut game, actor, 60));
                let condition = crate::content_ids::ConditionId::new("body_tail_health").unwrap();
                if live_sources {
                    game.apply_condition(actor, &condition, actor, 1_000)
                        .unwrap();
                }
                let injury = if live_sources { 45 } else { 2 };
                crate::test_creatures::injure(&mut game, actor, injury).unwrap();
                let conditions = game.creature(actor).unwrap().conditions().clone();
                let rng = game.rules_rng;
                let before = crate::diagnostics::work_counts();
                let result = game.polymorph_creature(actor, strong.clone()).unwrap();
                let after = crate::diagnostics::work_counts();

                assert_eq!(result, BodyTransitionResult::Changed);
                let creature = game.creature(actor).unwrap();
                assert_eq!(
                    creature.build().active_body_package().unwrap().id,
                    strong.id
                );
                assert_eq!(creature.build().ledger().entries()[2..], personal);
                assert_eq!(creature.health().injury(), u64::from(injury));
                assert!(
                    !creature.health().dead(),
                    "no intermediate equipment-loss death"
                );
                assert_eq!(creature.conditions(), &conditions);
                assert_eq!(game.rules_rng, rng, "body streams are not combat RNG");
                assert!(game.equipment(actor).unwrap().is_empty());
                if let Some(item) = item {
                    assert_eq!(game.items[&item].location, ItemLocation::Carried(actor));
                    assert!(game.items[&item].condition.cursed);
                }
                report_body_compile_work(
                    if live_sources {
                        "changed_with_condition_and_cursed_gear"
                    } else {
                        "changed_plain"
                    },
                    "body_transition",
                    total_hd,
                    before,
                    after,
                );
            }
        }
    }

    #[test]
    fn body_compile_work_noop_rejection_and_pending_return_baseline() {
        let (mut game, actor, base, weak, _) = fixture();
        let unchanged = game.clone();
        let before = crate::diagnostics::work_counts();
        assert_eq!(
            game.polymorph_creature(actor, base.clone()).unwrap(),
            BodyTransitionResult::Unchanged
        );
        let after = crate::diagnostics::work_counts();
        assert_eq!(game, unchanged);
        report_body_compile_work("already_effective", "body_transition", 2, before, after);

        let mut forged = (*base).clone();
        forged.body.cells.push([2, 0, 0]);
        let before = crate::diagnostics::work_counts();
        assert!(game.polymorph_creature(actor, Arc::new(forged)).is_err());
        let after = crate::diagnostics::work_counts();
        assert_eq!(game, unchanged, "catalog mismatch rejects atomically");
        report_body_compile_work("rejected_definition", "body_transition", 2, before, after);

        game.polymorph_creature(actor, weak.clone()).unwrap();
        crate::test_creatures::injure(&mut game, actor, 2).unwrap();
        let blocker = game
            .spawn_actor(at(2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let unchanged = game.clone();
        let before = crate::diagnostics::work_counts();
        assert_eq!(
            game.polymorph_creature(actor, base.clone()).unwrap(),
            BodyTransitionResult::Blocked
        );
        let after = crate::diagnostics::work_counts();
        assert_eq!(game, unchanged);
        report_body_compile_work("blocked_geometry", "body_transition", 2, before, after);

        let rng = game.rules_rng;
        let personal = game.creature(actor).unwrap().build().ledger().entries()[1].clone();
        let before = crate::diagnostics::work_counts();
        game.apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
            .unwrap();
        let after = crate::diagnostics::work_counts();
        let creature = game.creature(actor).unwrap();
        assert!(creature.cancelled() && creature.polymorph_return_pending());
        assert_eq!(creature.build().active_body_package().unwrap().id, weak.id);
        assert_eq!(creature.health().injury(), 2);
        assert_eq!(game.rules_rng, rng);
        report_body_compile_work(
            "cancellation_blocked_return",
            "effect_application",
            2,
            before,
            after,
        );

        let unchanged = game.clone();
        let before = crate::diagnostics::work_counts();
        game.finish_pending_body_return(actor).unwrap();
        let after = crate::diagnostics::work_counts();
        assert_eq!(game, unchanged);
        report_body_compile_work("pending_retry_blocked", "pending_return", 2, before, after);

        let before = crate::diagnostics::work_counts();
        game.teleport(actor, at(3)).unwrap();
        let after = crate::diagnostics::work_counts();
        let creature = game.creature(actor).unwrap();
        assert!(creature.cancelled() && !creature.polymorph_return_pending());
        assert_eq!(creature.build().active_body_package().unwrap().id, base.id);
        assert_eq!(creature.build().ledger().entries()[1], personal);
        assert_eq!(creature.health().injury(), 2);
        assert_eq!(game.actors[&actor].location, at(3));
        assert_eq!(game.actors[&blocker].location, at(2));
        assert_eq!(game.rules_rng, rng);
        report_body_compile_work(
            "pending_return_after_move",
            "teleport_and_return",
            2,
            before,
            after,
        );
    }

    #[test]
    fn body_compile_work_immediate_cancellation_return_baseline() {
        let (mut game, actor, base, _, strong) = fixture();
        game.polymorph_creature(actor, strong).unwrap();
        crate::test_creatures::injure(&mut game, actor, 20).unwrap();
        let rng = game.rules_rng;
        let before = crate::diagnostics::work_counts();
        game.apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
            .unwrap();
        let after = crate::diagnostics::work_counts();
        let creature = game.creature(actor).unwrap();
        assert!(
            !creature.health().dead(),
            "suppression and return share final Health"
        );
        assert!(creature.cancelled() && !creature.polymorph_return_pending());
        assert_eq!(creature.health().injury(), 20);
        assert_eq!(creature.build().active_body_package().unwrap().id, base.id);
        assert_eq!(game.rules_rng, rng);
        report_body_compile_work(
            "immediate_cancellation_return",
            "effect_application",
            3,
            before,
            after,
        );
    }

    fn pending_contact_fixture(
        lethal: bool,
        teleport_trap: bool,
    ) -> (Game, ActorId, Location, Arc<BodyPackage>) {
        use tor_world::{Direction, Extent, Passage, Region, World};
        let (mut game, actor, base, weak, _) = fixture();
        let mut world = World::new(
            (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: format!("post-contact body {id}"),
                    bounds: Extent::new(6, 4, 2).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        for region in 1..=2 {
            for x in 0..6 {
                for y in 0..4 {
                    world
                        .set_wall(
                            Location {
                                region: RegionId(region),
                                position: Position { x, y, z: 0 },
                            },
                            true,
                        )
                        .unwrap();
                }
            }
        }
        let origin = Location {
            position: Position {
                z: 1,
                ..at(1).position
            },
            ..at(1)
        };
        let target = Location {
            position: Position {
                x: 5,
                ..origin.position
            },
            ..origin
        };
        let side = Location {
            region: RegionId(2),
            position: Position {
                x: 0,
                ..origin.position
            },
        };
        world
            .connect(
                Passage {
                    from: target,
                    direction: Direction::East,
                    to: side,
                },
                0,
            )
            .unwrap();
        *game.world = world;
        game.actors.get_mut(&actor).unwrap().location = origin;
        game.polymorph_creature(actor, weak).unwrap();
        game.spawn_actor(
            Location {
                position: Position {
                    x: 2,
                    ..origin.position
                },
                ..origin
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        game.apply_effects(actor, &[crate::rules::Effect::CancelSpecials])
            .unwrap();
        assert!(game.creature(actor).unwrap().polymorph_return_pending());
        if lethal {
            let remaining = game.creature(actor).unwrap().health().current();
            crate::test_creatures::injure(&mut game, actor, u32::try_from(remaining - 1).unwrap())
                .unwrap();
            assert_eq!(game.creature(actor).unwrap().health().current(), 1);
        } else {
            assert!(game.creature(actor).unwrap().health().current() > 6);
        }
        game.world
            .place_trap(
                target,
                tor_world::Trap {
                    id: 1,
                    dc: 99,
                    avoidance_dc: None,
                    kind: if teleport_trap {
                        tor_world::TrapKind::Teleport {}
                    } else {
                        tor_world::TrapKind::Arrow {
                            attack_bonus: 100,
                            ammunition: "polymorph.arrow".into(),
                        }
                    },
                },
            )
            .unwrap();
        (game, actor, target, base)
    }

    #[test]
    fn pending_return_moves_and_contacts_in_old_form_before_residency_independent_return() {
        for lethal in [false, true] {
            let (mut loaded, actor, target, base) = pending_contact_fixture(lethal, false);
            let mut unloaded = loaded.clone();
            let slice = unloaded.world.detach_region(RegionId(2)).unwrap();
            let initial_tick = loaded.tick;
            let initial_rng = loaded.rules_rng;
            loaded.teleport_for_host(actor, target).unwrap();
            unloaded.teleport_for_host(actor, target).unwrap();
            assert_ne!(
                loaded.rules_rng, initial_rng,
                "the old-form arrow really rolls"
            );
            assert_eq!(loaded.rules_rng, unloaded.rules_rng);
            assert_eq!(loaded.tick, initial_tick);
            assert_eq!(unloaded.tick, initial_tick);
            assert_eq!(loaded.actors[&actor].location, target);
            assert_eq!(unloaded.actors[&actor].location, target);
            assert_eq!(loaded.alive(actor), !lethal);
            assert_eq!(unloaded.alive(actor), !lethal);
            if lethal {
                assert!(unloaded.trap_region_boundary().is_none());
                assert!(!unloaded.creature(actor).unwrap().polymorph_return_pending());
                assert_ne!(
                    loaded
                        .creature(actor)
                        .unwrap()
                        .build()
                        .active_body_package()
                        .unwrap()
                        .id,
                    base.id
                );
            } else {
                let boundary = unloaded.trap_region_boundary().unwrap();
                assert_eq!(
                    boundary.phase,
                    crate::TrapRegionPhase::PostContactBodyReturn
                );
                assert_eq!(boundary.location, target);
                assert!(unloaded.creature(actor).unwrap().polymorph_return_pending());
                assert!(!loaded.creature(actor).unwrap().polymorph_return_pending());
            }
            unloaded.world.attach_region(slice).unwrap();
            let drawn = unloaded.rules_rng;
            if !lethal {
                unloaded.host_region_continuation = true;
                assert!(unloaded.resume_trap_region_boundary().unwrap());
                unloaded.host_region_continuation = false;
            }
            assert_eq!(
                unloaded.rules_rng, drawn,
                "residency neither rerolls movement nor repeats the arrow"
            );
            assert_eq!(unloaded.tick, loaded.tick);
            assert_eq!(
                unloaded.creature(actor).unwrap(),
                loaded.creature(actor).unwrap()
            );
            assert_eq!(
                unloaded.actors[&actor].trap_contacts,
                loaded.actors[&actor].trap_contacts
            );
            assert_eq!(
                unloaded.actors[&actor].ready_at,
                loaded.actors[&actor].ready_at
            );
            assert!(unloaded.trap_region_boundary().is_none());
        }
    }

    #[test]
    fn physics_tick_finishes_original_cohort_before_warm_or_cold_pending_return() {
        let (mut loaded, actor, target, base) = pending_contact_fixture(false, false);
        let other = loaded
            .spawn_actor(
                Location {
                    position: Position { x: 1, y: 2, z: 1 },
                    ..target
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        for (id, x) in [(actor, 4), (other, 1)] {
            let mut state = loaded.actors.get_mut(&id).unwrap();
            state.location.position.x = x;
            state.motion.velocity = [crate::physics::TERMINAL, 0, 0];
            state.motion.displacement = [crate::physics::CELL - crate::physics::TERMINAL / 2, 0, 0];
            state.ready_at = 100;
        }
        let mut cold = loaded.clone();
        let slice = cold.world.detach_region(RegionId(2)).unwrap();
        for game in [&mut loaded, &mut cold] {
            game.host_region_continuation = true;
            assert_eq!(
                game.with_effect_publication(|game| game.advance_physics(1))
                    .unwrap(),
                1
            );
            game.host_region_continuation = false;
            assert_eq!(game.actors[&actor].location, target);
            assert_eq!(
                game.actors[&other].location.position.x, 2,
                "the other original entity is integrated before the residency yield"
            );
            assert!(!game.body_return_finalization.deferring);
        }
        assert_eq!(loaded.rules_rng, cold.rules_rng);
        assert_eq!(loaded.actors[&other], cold.actors[&other]);
        assert_eq!(
            loaded
                .creature(actor)
                .unwrap()
                .build()
                .active_body_package()
                .unwrap()
                .id,
            base.id
        );
        assert!(cold.trap_region_boundary().is_some());
        let drawn = cold.rules_rng;
        cold.world.attach_region(slice).unwrap();
        cold.host_region_continuation = true;
        cold.resume_trap_region_boundary().unwrap();
        cold.host_region_continuation = false;
        assert_eq!(cold.tick, loaded.tick);
        assert_eq!(cold.rules_rng, drawn);
        assert_eq!(
            cold.creature(actor).unwrap(),
            loaded.creature(actor).unwrap()
        );
        assert_eq!(cold.actors[&actor].motion, loaded.actors[&actor].motion);
        assert!(cold.body_return_finalization.settled());
    }

    #[test]
    fn gravity_present_resting_tail_settles_both_actors_before_pending_return() {
        let (mut game, actor, _, base) = pending_contact_fixture(false, false);
        let blocker = ActorId(2);
        let build = game.creature(actor).unwrap().build().clone();
        game.configure_creature(
            blocker,
            CreatureIdentity {
                name: "expiring blocker".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        let base_health = game.creature(blocker).unwrap().health().maximum();
        let condition = crate::content_ids::ConditionId::new("body_tail_health").unwrap();
        for id in [actor, blocker] {
            game.apply_condition(id, &condition, id, 10).unwrap();
            game.actors.get_mut(&id).unwrap().ready_at = 100;
        }
        crate::test_creatures::injure(&mut game, blocker, u32::try_from(base_health).unwrap())
            .unwrap();
        assert!(game.alive(blocker));
        assert!(game.creature(actor).unwrap().polymorph_return_pending());
        game.world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
        assert!(game.world.has_gravity());
        assert!(game
            .actors
            .values()
            .all(|owner| owner.motion.velocity == [0; 3]));
        assert_eq!(game.next_creature_event_tick(), Some(10));
        let before_rng = game.rules_rng;
        assert_eq!(
            game.with_effect_publication(|game| game.advance_physics(10))
                .unwrap(),
            10
        );
        assert_eq!(game.tick, 10);
        assert!(
            !game.alive(blocker),
            "the second actor's health expires in the resting tail"
        );
        assert_eq!(game.actors[&actor].active_since, 10);
        assert_eq!(game.actors[&blocker].active_since, 10);
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .build()
                .active_body_package()
                .unwrap()
                .id,
            base.id,
            "first actor's return follows the complete tail cohort, after the blocker dies"
        );
        assert!(!game.creature(actor).unwrap().polymorph_return_pending());
        assert_eq!(game.rules_rng, before_rng);
        assert!(game.body_return_finalization.settled());
    }

    #[test]
    fn tick_return_guard_cleans_up_early_error_and_preserves_pending_ownership() {
        let (mut game, actor, _, _) = pending_contact_fixture(false, false);
        let before = game.clone();
        let result: Result<(), GameError> = game.with_deferred_body_returns(|game| {
            game.finish_pending_body_return(actor)?;
            assert!(!game.body_return_finalization.queued.is_empty());
            Err(GameError::InvalidLocation)
        });
        assert!(result.is_err());
        assert!(game.body_return_finalization.settled());
        assert_eq!(game, before);
        let value = game
            .with_deferred_body_returns(|game| {
                game.finish_pending_body_return(actor)?;
                Ok(17)
            })
            .unwrap();
        assert_eq!(value, 17);
        assert!(game.body_return_finalization.settled());
        assert!(game.creature(actor).unwrap().polymorph_return_pending());
    }

    #[test]
    fn captured_active_return_finishes_after_host_freeze_without_thawing_neighbors() {
        let (mut warm, actor, target, _) = pending_contact_fixture(false, false);
        let side = Location {
            region: RegionId(2),
            position: Position {
                x: 0,
                ..target.position
            },
        };
        warm.world
            .place_trap(
                side,
                tor_world::Trap {
                    id: 2,
                    dc: 99,
                    avoidance_dc: None,
                    kind: tor_world::TrapKind::Arrow {
                        attack_bonus: 100,
                        ammunition: "polymorph.arrow".into(),
                    },
                },
            )
            .unwrap();
        let neighbor = warm
            .spawn_actor(
                Location {
                    position: Position { x: 3, y: 2, z: 1 },
                    ..side
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        warm.freeze_region_for_body_return_test(RegionId(2))
            .unwrap();
        let frozen_neighbor = warm.actors[&neighbor].clone();
        let mut cold = warm.clone();
        let slice = cold.world.detach_region(RegionId(2)).unwrap();
        warm.teleport_for_host(actor, target).unwrap();
        cold.teleport_for_host(actor, target).unwrap();
        assert!(cold.trap_region_boundary().is_some());
        // Host horizon settlement may freeze the already-completed entry before
        // its load-only continuation. It does not grant fresh actor activity.
        for game in [&mut warm, &mut cold] {
            game.freeze_region_for_body_return_test(RegionId(1))
                .unwrap();
        }
        cold.world.attach_region(slice).unwrap();
        let frozen_clocks = |game: &Game| {
            game.actors
                .keys()
                .copied()
                .map(|id| (id, game.actor_frozen(id), game.actor_clock(id)))
                .collect::<Vec<_>>()
        };
        let stamps = frozen_clocks(&cold);
        cold.host_region_continuation = true;
        cold.resume_trap_region_boundary().unwrap();
        cold.host_region_continuation = false;
        assert_eq!(
            cold.rules_rng, warm.rules_rng,
            "new-body arrow contact is identical after captured continuation"
        );
        assert_eq!(cold.creature(actor).unwrap(), warm.creature(actor).unwrap());
        assert_eq!(
            cold.actors[&actor].trap_contacts,
            warm.actors[&actor].trap_contacts
        );
        assert_eq!(frozen_clocks(&cold), stamps);
        assert_eq!(cold.actors[&neighbor], frozen_neighbor);
        assert!(cold.actor_frozen(actor) && cold.actor_frozen(neighbor));
        assert!(cold.body_return_finalization.settled());
    }

    #[test]
    fn returned_body_contact_phase_retains_only_its_captured_owner_authority() {
        let (mut warm, actor, target, _) = pending_contact_fixture(false, false);
        warm.teleport_for_host(actor, target).unwrap();
        assert!(!warm.creature(actor).unwrap().polymorph_return_pending());
        let side = Location {
            region: RegionId(2),
            position: Position {
                x: 0,
                ..target.position
            },
        };
        warm.world
            .place_trap(
                side,
                tor_world::Trap {
                    id: 2,
                    dc: 99,
                    avoidance_dc: None,
                    kind: tor_world::TrapKind::Arrow {
                        attack_bonus: 100,
                        ammunition: "polymorph.arrow".into(),
                    },
                },
            )
            .unwrap();
        let mut cold = warm.clone();
        let drawn = warm.rules_rng;
        warm.reconcile_trap_contacts(actor, None).unwrap();
        assert_ne!(warm.rules_rng, drawn);
        cold.freeze_region_for_body_return_test(RegionId(1))
            .unwrap();
        cold.trap_region_boundary = Some(crate::TrapRegionBoundary {
            phase: crate::TrapRegionPhase::Contacts,
            active_clock: Some(cold.tick),
            actor,
            location: target,
            orientation: 0,
            avoidable_traps: Default::default(),
            imposed_normal: None,
            members: [RegionId(1), RegionId(2)].into_iter().collect(),
        });
        cold.host_region_continuation = true;
        cold.resume_trap_region_boundary().unwrap();
        cold.host_region_continuation = false;
        assert_eq!(cold.rules_rng, warm.rules_rng);
        assert_eq!(cold.creature(actor).unwrap(), warm.creature(actor).unwrap());
        assert_eq!(
            cold.actors[&actor].trap_contacts,
            warm.actors[&actor].trap_contacts
        );
        assert!(cold.actor_frozen(actor));
        assert!(cold.body_return_finalization.settled());
    }

    #[test]
    fn blocked_return_stays_pending_and_frozen_subject_is_not_activated() {
        let (mut game, actor, _, base) = pending_contact_fixture(false, false);
        let blocked = game.clone();
        game.finish_pending_body_return(actor).unwrap();
        assert_eq!(game, blocked);
        assert!(game.creature(actor).unwrap().polymorph_return_pending());
        // Freeze through the lifecycle owner, then change only frozen geometry.
        // A loaded slice is not permission to activate it.
        game.freeze_region_for_body_return_test(RegionId(1))
            .unwrap();
        let blocker = ActorId(2);
        game.actors.get_mut(&blocker).unwrap().location.position.x = 3;
        let at = game.actors[&actor].location;
        assert!(game.body_fits(actor, at, 0, &base.body));
        let frozen = game.clone();
        game.finish_pending_body_return(actor).unwrap();
        assert_eq!(game, frozen);
        assert!(game.actor_frozen(actor));
        assert!(game.creature(actor).unwrap().polymorph_return_pending());
    }

    #[test]
    fn post_contact_return_rejects_stale_anchor_frame_or_dependencies() {
        let (mut game, actor, target, _) = pending_contact_fixture(false, false);
        let slice = game.world.detach_region(RegionId(2)).unwrap();
        game.teleport_for_host(actor, target).unwrap();
        assert!(game.trap_region_boundary().is_some());
        game.world.attach_region(slice).unwrap();
        for corruption in 0..4 {
            let mut forged = game.clone();
            let boundary = forged.trap_region_boundary.as_mut().unwrap();
            match corruption {
                0 => boundary.location.position.y += 1,
                1 => boundary.orientation = 1,
                2 => boundary.active_clock = Some(game.tick + 1),
                _ => {
                    boundary.members.insert(RegionId(99));
                }
            }
            assert!(forged.resume_trap_region_boundary().is_err());
        }
    }

    #[test]
    fn nested_contact_teleport_owns_the_return_anchor_and_never_replays_selection() {
        let (mut game, actor, trap, _) = pending_contact_fixture(false, true);
        game.teleport_for_host(actor, trap).unwrap();
        let after = game.actors[&actor].location;
        assert_ne!(
            after, trap,
            "the real nested teleport selected another landing"
        );
        let drawn = game.rules_rng;
        let tick = game.tick;
        game.finish_pending_body_return(actor).unwrap();
        assert_eq!(game.actors[&actor].location, after);
        assert_eq!(game.rules_rng, drawn);
        assert_eq!(game.tick, tick);
        assert!(game.trap_region_boundary().is_none());
    }
}

#[cfg(test)]
mod residency_tests {
    use super::body_footprint_regions;
    use std::collections::BTreeSet;
    use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

    #[test]
    fn off_ray_body_neighbor_is_a_load_only_dependency_before_fit() {
        let mut world = World::new(
            (1..=2)
                .map(|id| Region {
                    id: RegionId(id),
                    name: format!("body dependency {id}"),
                    bounds: Extent::new(3, 3, 1).unwrap(),
                })
                .collect(),
            vec![],
        )
        .unwrap();
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 2, z: 0 },
        };
        let side = Location {
            region: RegionId(2),
            position: Position { x: 1, y: 0, z: 0 },
        };
        // An eastward contact ray never visits this southward body extension.
        world
            .connect(
                Passage {
                    from: at,
                    direction: Direction::South,
                    to: side,
                },
                0,
            )
            .unwrap();
        let body = crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 1, 0], [0, 2, 0]],
            ..crate::BodySpec::default()
        };
        let expected = BTreeSet::from([RegionId(1), RegionId(2)]);
        assert_eq!(body_footprint_regions(&world, at, 0, &body), expected);
        let loaded_fit = crate::physics::resolve_body(&world, at, 0, &body).unwrap();
        assert_eq!(loaded_fit[1].0, side);
        let slice = world.detach_region(RegionId(2)).unwrap();
        let before = world.clone();
        assert!(crate::physics::resolve_body(&world, at, 0, &body).is_none());
        assert_eq!(body_footprint_regions(&world, at, 0, &body), expected);
        assert_eq!(
            world, before,
            "dependency probing does not load or activate geometry"
        );
        world.attach_region(slice).unwrap();
        assert_eq!(
            crate::physics::resolve_body(&world, at, 0, &body).unwrap(),
            loaded_fit
        );
        assert_eq!(body_footprint_regions(&world, at, 0, &body), expected);
    }
}
