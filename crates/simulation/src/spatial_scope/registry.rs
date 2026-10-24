//! Trusted-input structural primitives, not source-certified geometry or a save codec.
//!
//! Explicit caller budgets bound validation/registration work. They are not final
//! production admission limits and do not bound allocation of an already-built Vec.
//! No Game, residency, population, portal transform or serialization authority lives here.

use super::{Membership, PrimarySpace, RoomId};
use std::collections::{BTreeMap, BTreeSet};
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

impl Usage {
    fn checked_add(self, other: Self) -> Result<Self, Error> {
        Ok(Self {
            scopes: self
                .scopes
                .checked_add(other.scopes)
                .ok_or(Error::Capacity)?,
            members: self
                .members
                .checked_add(other.members)
                .ok_or(Error::Capacity)?,
            rooms: self.rooms.checked_add(other.rooms).ok_or(Error::Capacity)?,
            passages: self
                .passages
                .checked_add(other.passages)
                .ok_or(Error::Capacity)?,
            runs: self.runs.checked_add(other.runs).ok_or(Error::Capacity)?,
            expanded_refs: self
                .expanded_refs
                .checked_add(other.expanded_refs)
                .ok_or(Error::Capacity)?,
            label_bytes: self
                .label_bytes
                .checked_add(other.label_bytes)
                .ok_or(Error::Capacity)?,
        })
    }
    fn within(self, budget: &Budget) -> Result<Self, Error> {
        if self.scopes > budget.scopes
            || self.members > budget.members
            || self.rooms > budget.rooms
            || self.passages > budget.passages
            || self.runs > budget.runs
            || self.expanded_refs > budget.expanded_refs
            || self.label_bytes > budget.label_bytes
        {
            return Err(Error::Capacity);
        }
        Ok(self)
    }
}

fn add_passage(
    set: &mut BTreeSet<super::PassageId>,
    passage: super::PassageId,
    limit: usize,
) -> Result<(), Error> {
    if !set.contains(&passage) {
        if set.len() >= limit {
            return Err(Error::Capacity);
        }
        set.insert(passage);
    }
    Ok(())
}

fn same_row(a: CellRun, b: CellRun) -> bool {
    (a.region, a.z, a.y) == (b.region, b.z, b.y)
}
fn ordered_runs(a: CellRun, b: CellRun, same_owner: bool) -> bool {
    if (a.region, a.z, a.y, a.first_x) >= (b.region, b.z, b.y, b.first_x) {
        return false;
    }
    !same_row(a, b)
        || (a.last_x < b.first_x
            && (!same_owner || i64::from(a.last_x) + 1 != i64::from(b.first_x)))
}

impl Scope {
    pub fn validate(&self, budget: &Budget) -> Result<Usage, Error> {
        // Reject aggregate sizes before allocating auxiliary sets. Inputs are
        // trusted typed values; this is deliberately not a decoder guard.
        let mut usage = Usage {
            scopes: 1,
            members: self.members.len(),
            rooms: self.rooms.len(),
            label_bytes: self.id.len(),
            ..Usage::default()
        }
        .within(budget)?;
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
            usage.within(budget)?;
        }
        if self.id.is_empty()
            || self.id.chars().any(char::is_control)
            || self.members.is_empty()
            || self.members.windows(2).any(|w| w[0] >= w[1])
            || self.rooms.windows(2).any(|w| w[0].room >= w[1].room)
        {
            return Err(Error::Structure);
        }
        let mut used_rooms = BTreeSet::new();
        let mut used_passages = BTreeSet::new();
        for primary in &self.primary {
            if self.members.binary_search(&primary.cells.region).is_err() {
                return Err(Error::Structure);
            }
            match primary.membership.primary {
                PrimarySpace::Room(room) => {
                    if self.rooms.binary_search_by_key(&room, |r| r.room).is_err() {
                        return Err(Error::Structure);
                    }
                    used_rooms.insert(room);
                }
                PrimarySpace::Passage(passage) => {
                    add_passage(&mut used_passages, passage, budget.passages)?;
                }
            }
            if let Some(passage) = primary.membership.doorway {
                add_passage(&mut used_passages, passage, budget.passages)?;
            }
        }
        if self
            .primary
            .windows(2)
            .any(|w| !ordered_runs(w[0].cells, w[1].cells, w[0].membership == w[1].membership))
        {
            return Err(Error::Structure);
        }
        usage.passages = used_passages.len();
        usage.within(budget)?;
        if used_rooms.len() != self.rooms.len() {
            return Err(Error::Structure);
        }
        for room in &self.rooms {
            if room.runs.is_empty()
                || room
                    .runs
                    .iter()
                    .any(|r| self.members.binary_search(&r.region).is_err())
                || room
                    .runs
                    .windows(2)
                    .any(|w| !ordered_runs(w[0], w[1], true))
            {
                return Err(Error::Structure);
            }
        }
        Ok(usage)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    scopes: BTreeMap<String, Scope>,
}
impl Registry {
    /// Identical registration still revalidates the whole aggregate budget.
    pub fn register(&mut self, scope: Scope, budget: &Budget) -> Result<(), Error> {
        let incoming = scope.validate(budget)?;
        if let Some(existing) = self.scopes.get(&scope.id) {
            if existing != &scope {
                return Err(Error::Conflict);
            }
        }
        let mut usage = Usage::default();
        for existing in self.scopes.values() {
            usage = usage
                .checked_add(existing.validate(budget)?)?
                .within(budget)?;
            if existing.id != scope.id
                && existing
                    .members
                    .iter()
                    .any(|r| scope.members.binary_search(r).is_ok())
            {
                return Err(Error::Conflict);
            }
        }
        if self.scopes.contains_key(&scope.id) {
            return Ok(());
        }
        usage.checked_add(incoming)?.within(budget)?;
        self.scopes.insert(scope.id.clone(), scope);
        Ok(())
    }
    pub fn lookup(&self, scope: &str, location: Location) -> Option<Membership> {
        let scope = self.scopes.get(scope)?;
        scope.members.binary_search(&location.region).ok()?;
        scope
            .primary
            .iter()
            .find(|r| r.cells.contains(location))
            .map(|r| r.membership)
    }
}

#[cfg(test)]
mod tests;
