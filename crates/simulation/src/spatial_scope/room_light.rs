//! Room illumination uses immutable source volume; it does not infer rooms from sight.
use super::{registry::CellRun, PrimarySpace};
use crate::{Game, GameError};
use std::collections::BTreeSet;
use tor_world::{Location, Position, RegionId};

impl Game {
    fn room_light_runs(&self, at: Location) -> Result<&[CellRun], GameError> {
        let scope = self
            .spatial_registry
            .scopes()
            .find(|scope| scope.members.binary_search(&at.region).is_ok())
            .ok_or(GameError::InvalidLocation)?;
        let membership = self
            .spatial_registry
            .lookup(&scope.id, at)
            .ok_or(GameError::InvalidLocation)?;
        match membership.primary {
            PrimarySpace::Passage(_) => Ok(&[]),
            PrimarySpace::Room(room) => scope
                .rooms
                .iter()
                .find(|entry| entry.room == room)
                .map(|entry| entry.runs.as_slice())
                .ok_or(GameError::InvalidLocation),
        }
    }
    pub(crate) fn room_light_regions(&self, at: Location) -> Result<BTreeSet<RegionId>, GameError> {
        Ok(self
            .room_light_runs(at)?
            .iter()
            .map(|run| run.region)
            .collect())
    }
    pub(crate) fn apply_room_light(&mut self, at: Location) -> Result<bool, GameError> {
        // Prove complete physical residency/geometry before any mutation. Unknown
        // geometry is an error, never a passage/no-op or partially lit room.
        for run in self.room_light_runs(at)? {
            for x in [run.first_x, run.last_x] {
                let cell = Location {
                    region: run.region,
                    position: Position {
                        x,
                        y: run.y,
                        z: run.z,
                    },
                };
                if !self.world.contains(cell) {
                    return Err(GameError::InvalidLocation);
                }
            }
            for x in run.first_x..=run.last_x {
                let cell = Location {
                    region: run.region,
                    position: Position {
                        x,
                        y: run.y,
                        z: run.z,
                    },
                };
                self.world.is_lit(cell).ok_or(GameError::InvalidLocation)?;
            }
        }
        // Hold only the immutable registry root while mutating World. No expanded
        // cell copy or alternate adjacency/transform authority is introduced.
        let registry = self.spatial_registry.clone();
        let scope = registry
            .scopes()
            .find(|scope| scope.members.binary_search(&at.region).is_ok())
            .ok_or(GameError::InvalidLocation)?;
        let membership = registry
            .lookup(&scope.id, at)
            .ok_or(GameError::InvalidLocation)?;
        let PrimarySpace::Room(room) = membership.primary else {
            return Ok(false);
        };
        let runs = &scope
            .rooms
            .iter()
            .find(|entry| entry.room == room)
            .ok_or(GameError::InvalidLocation)?
            .runs;
        let mut changed = false;
        for run in runs {
            for x in run.first_x..=run.last_x {
                let cell = Location {
                    region: run.region,
                    position: Position {
                        x,
                        y: run.y,
                        z: run.z,
                    },
                };
                if self.world.is_lit(cell) == Some(false) {
                    self.world
                        .set_cell_light(cell, true)
                        .map_err(|_| GameError::InvalidLocation)?;
                    changed = true;
                }
            }
        }
        Ok(changed)
    }
}
