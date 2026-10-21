//! Ordinary effect relocation: bounded live destinations, shared lifecycle and
//! no privileged setup shortcuts. Random selection belongs to the owning RNG.
use crate::abilities::{Placement, StepDirection};
use crate::{ActorId, Game, GameError};
use tor_world::{compose_rotation, Direction, Location, Position, RayStep};

impl Game {
    /// Current-floor cells only. A generated scope supplies its deterministic
    /// bounded floor directory; standalone authored regions use their own bounds.
    pub(crate) fn random_relocation_candidates(
        &self,
        actor: ActorId,
    ) -> Result<Vec<Placement>, GameError> {
        let source = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        if !source.alive() {
            return Ok(Vec::new());
        }
        let scope = self.activation_scope_at(source.location.region);
        let cells = if let Some(scope) = scope {
            // Never change the draw distribution merely because an unloaded
            // part of this floor is currently unavailable.
            if scope
                .members
                .iter()
                .any(|id| self.world.region(*id).is_none())
            {
                return Ok(Vec::new());
            }
            scope.cells.clone()
        } else {
            let region = self
                .world
                .region(source.location.region)
                .ok_or(GameError::InvalidLocation)?;
            let (width, depth, height) = region.bounds.dimensions();
            // A finite candidate budget is a paid failed placement, not an
            // unbounded scan or a distribution biased toward a truncated prefix.
            if i128::from(width) * i128::from(depth) * i128::from(height) > 65_536 {
                return Ok(Vec::new());
            }
            let origin = region.bounds.origin();
            let mut cells = Vec::new();
            for z in origin.z..origin.z + height {
                for y in origin.y..origin.y + depth {
                    for x in origin.x..origin.x + width {
                        cells.push(Location {
                            region: region.id,
                            position: Position { x, y, z },
                        });
                    }
                }
            }
            cells
        };
        Ok(cells
            .into_iter()
            .filter(|at| {
                *at != source.location
                    && self
                        .body_cells(*at, source.orientation, &source.body)
                        .is_some_and(|cells| {
                            cells.iter().all(|(cell, _)| {
                                scope.map_or(cell.region == source.location.region, |scope| {
                                    scope.members.contains(&cell.region)
                                })
                            })
                        })
                    && self.stable_landing(actor, *at, source.orientation, &source.body)
            })
            .map(|location| Placement {
                location,
                orientation: source.orientation,
            })
            .collect())
    }

    /// The adjacent endpoint follows the frozen launch frame through *current*
    /// physical topology. Recipient placement/body remain live; no nearby search.
    pub(crate) fn ray_relocation_destination(
        &self,
        actor: ActorId,
        origin: Placement,
        direction: StepDirection,
    ) -> Result<Option<Placement>, GameError> {
        let source = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let direction = Direction::from(direction).rotated(origin.orientation);
        let RayStep::Clear { to, rotation } = self.world.ray_step(origin.location, direction)
        else {
            return Ok(None);
        };
        let destination = Placement {
            location: to,
            orientation: compose_rotation(source.orientation, rotation),
        };
        Ok(self
            .relocation_destination_valid(actor, destination)
            .then_some(destination))
    }

    pub(crate) fn relocation_destination_valid(&self, actor: ActorId, to: Placement) -> bool {
        self.actors.get(&actor).is_some_and(|source| {
            source.alive()
                && to.orientation < 24
                && (source.location != to.location || source.orientation != to.orientation)
                && self.stable_landing(actor, to.location, to.orientation, &source.body)
        })
    }

    pub(crate) fn relocate_exact_in_transaction(
        &mut self,
        actor: ActorId,
        destination: Placement,
        preserve_completed_work: bool,
    ) -> Result<bool, GameError> {
        if !self.relocation_destination_valid(actor, destination) {
            return Ok(false);
        }
        let mut outcome = self.settled_actor_outcome(actor)?;
        if !outcome.state.alive() {
            self.apply_outcome(outcome)?;
            return Ok(false);
        }
        if !preserve_completed_work {
            outcome.interrupt(
                crate::lifecycle::Interruption::Discard,
                self.actor_clock(actor),
            )?;
        }
        outcome.state.location = destination.location;
        outcome.state.orientation = destination.orientation;
        outcome.state.motion = crate::MotionState::default();
        outcome.state.visited.insert(destination.location.region);
        outcome.entry_cause = crate::floor_activation::EntryCause::Teleport;
        self.apply_outcome(outcome)?;
        self.sync_actor_lifecycle(actor)?;
        Ok(true)
    }

    pub(crate) fn relocate_random_in_transaction(
        &mut self,
        actor: ActorId,
        rng: &mut u64,
        preserve_completed_work: bool,
    ) -> Result<Option<Placement>, GameError> {
        let candidates = self.random_relocation_candidates(actor)?;
        if candidates.is_empty() {
            return Ok(None);
        }
        let index =
            crate::quantity::QuantityRange::new(1, candidates.len() as u64)?.sample(rng)? - 1;
        let destination = candidates[index as usize];
        // A destination contact can itself draw (or relocate). Publish this
        // draw before entering it, then return the fully evolved stream.
        self.rules_rng = *rng;
        let moved =
            self.relocate_exact_in_transaction(actor, destination, preserve_completed_work)?;
        *rng = self.rules_rng;
        Ok(moved.then_some(destination))
    }
}
