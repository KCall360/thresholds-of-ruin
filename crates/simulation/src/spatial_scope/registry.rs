//! Trusted-input structural primitives, not source-certified geometry or a save codec.
//!
//! Explicit caller budgets bound validation/registration work. They are not final
//! production admission limits and do not bound allocation of an already-built Vec.
//! No Game, residency, population, portal transform or serialization authority lives here.

use super::{Membership, RoomId};
use std::collections::BTreeMap;
use tor_world::{Location, RegionId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRun {
    pub region: RegionId,
    pub z: i32,
    pub y: i32,
    pub first_x: i32,
    pub last_x: i32,
}
impl CellRun {
    pub fn expanded_len(self) -> Option<u64> {
        let width = i64::from(self.last_x)
            .checked_sub(i64::from(self.first_x))?
            .checked_add(1)?;
        (width > 0).then_some(width as u64)
    }
    pub fn contains(self, location: Location) -> bool {
        self.region == location.region
            && self.z == location.position.z
            && self.y == location.position.y
            && self.first_x <= location.position.x
            && location.position.x <= self.last_x
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimaryRun {
    pub cells: CellRun,
    pub membership: Membership,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomSurfaces {
    pub room: RoomId,
    pub runs: Vec<CellRun>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    pub id: String,
    pub members: Vec<RegionId>,
    pub primary: Vec<PrimaryRun>,
    pub rooms: Vec<RoomSurfaces>,
}

/// Aggregate limits supplied explicitly; there is deliberately no default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub scopes: usize,
    pub members: usize,
    pub rooms: usize,
    pub passages: usize,
    pub runs: usize,
    pub expanded_refs: u64,
    pub label_bytes: usize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub scopes: usize,
    pub members: usize,
    pub rooms: usize,
    pub passages: usize,
    pub runs: usize,
    pub expanded_refs: u64,
    pub label_bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Structure,
    Capacity,
    Conflict,
}

impl Scope {
    pub fn validate(&self, _budget: &Budget) -> Result<Usage, Error> {
        // First-red scaffold: count trusted inputs, but do not yet enforce
        // canonical structure or the caller's aggregate capacity contract.
        let mut usage = Usage {
            scopes: 1,
            members: self.members.len(),
            rooms: self.rooms.len(),
            label_bytes: self.id.len(),
            ..Usage::default()
        };
        for run in self
            .primary
            .iter()
            .map(|r| &r.cells)
            .chain(self.rooms.iter().flat_map(|r| &r.runs))
        {
            usage.runs = usage.runs.checked_add(1).ok_or(Error::Capacity)?;
            usage.expanded_refs = usage
                .expanded_refs
                .checked_add(run.expanded_len().ok_or(Error::Structure)?)
                .ok_or(Error::Capacity)?;
        }
        Ok(usage)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    scopes: BTreeMap<String, Scope>,
}
impl Registry {
    /// Identical registration must still satisfy the supplied aggregate budget.
    pub fn register(&mut self, scope: Scope, budget: &Budget) -> Result<(), Error> {
        scope.validate(budget)?;
        // First-red scaffold: deliberately missing registration.
        Ok(())
    }
    pub fn lookup(&self, scope: &str, _location: Location) -> Option<Membership> {
        self.scopes.get(scope)?;
        None
    }
}

#[cfg(test)]
mod tests;
