//! Region lifecycle for streaming. Reference points decide which regions must
//! stay active and loaded; other regions freeze, and a frozen region can be
//! detached into a self-contained record. Transitions happen only between
//! actions and depend on game state alone, so replay reproduces them. See
//! `docs/region-streaming.md#region-lifecycle-contract`.
use crate::ai::Ai;
use crate::observation::SIGHT_RANGE;
use crate::travel::Navigation;
use crate::{Actor, ActorId, Game, GameError, Impact, Item, ItemId, ItemLocation, PhysicsEntity};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Direction, Location, Region, RegionId, RegionSlice, Shared};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ReferencePointId(pub u64);

/// What a reference point follows. An item point follows its carrier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceTarget {
    Actor(ActorId),
    Item(ItemId),
    Location(Location),
}

/// A source of region activity. Players aren't special: characters get
/// observing points by default, and other rules (scrying, machines) may add
/// more.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferencePoint {
    pub target: ReferenceTarget,
    /// Portal hops kept active around the target; `None` uses the default.
    pub active_radius: Option<u32>,
    /// Portal hops kept loaded around the target; `None` uses the default.
    pub load_radius: Option<u32>,
    /// Someone sees from this point, so everything it sees must be active.
    pub observes: bool,
}

/// A reference point resolved to the region its target is in, for the
/// server's preload planner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionRoot {
    pub point: ReferencePointId,
    pub region: RegionId,
    pub active_radius: Option<u32>,
    pub load_radius: Option<u32>,
    pub observes: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionState {
    /// Simulated.
    Active,
    /// In memory, with time stopped.
    Frozen,
    /// Held as a record, with time stopped.
    Detached,
    /// Known from its metadata only: never needed, so never built. A
    /// [`RecordStore`] builds its starting record when it's first loaded.
    Unbuilt,
}

/// Region sets after a transition. `active` must be a subset of `loaded`;
/// loaded regions outside `active` are frozen, and known regions outside
/// `loaded` are detached.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionTransition {
    pub active: BTreeSet<RegionId>,
    pub loaded: BTreeSet<RegionId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TransitionReport {
    pub attached: Vec<RegionId>,
    /// The attached regions that were unbuilt, built from their source.
    pub built: Vec<RegionId>,
    pub frozen: Vec<RegionId>,
    pub thawed: Vec<RegionId>,
    pub detached: Vec<RegionId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionError {
    UnknownRegion(RegionId),
    ActiveNotLoaded(RegionId),
    /// A pin requires this region to be active.
    MustBeActive(RegionId),
    /// A pin requires this region to be loaded.
    MustBeLoaded(RegionId),
    /// A region record doesn't fit the game it's being attached to.
    InvalidRecord(RegionId),
    /// The record store couldn't provide this region's record.
    RecordUnavailable(RegionId),
}

/// Identity of a detached region's record. The game allocates these in
/// order, so replay reproduces them, and a record never changes after it's
/// made, so the identity also names its content. See
/// [`Game::continue_record_ids`] for keeping identities unique across rewinds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RecordId(pub u64);

/// Where detached region records are kept: in memory, or on disk by the
/// server. Transitions read records only through this, so where a record is
/// kept can never change a result.
pub trait RecordStore {
    /// Keep a newly detached record. Called only after the transition that
    /// made it succeeds, and at most once per identity; different content
    /// under an existing identity is a storage invariant violation.
    fn put(&mut self, id: RecordId, record: Shared<RegionRecord>);
    /// The record kept under `id`, or `None` if it can't be provided.
    fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>>;
    /// The starting record of an unbuilt region, from its source (a
    /// scenario package, say). It must depend only on the source, never on
    /// which regions were built before, and its actors must carry freeze
    /// stamps of zero, so the region's time starts when it's first active;
    /// see [`Game::into_region_record`]. `None` if this store has no source.
    fn build(&mut self, region: RegionId) -> Option<RegionRecord> {
        let _ = region;
        None
    }
}

/// The identities an unbuilt region will hold once built, declared by its
/// source in advance so references to them stay checkable.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionIdentities {
    pub actors: BTreeSet<ActorId>,
    pub items: BTreeSet<ItemId>,
    pub doors: BTreeSet<u64>,
}

/// Records kept in memory.
#[derive(Clone, Debug, Default)]
pub struct MemoryRecords(BTreeMap<RecordId, Shared<RegionRecord>>);

impl MemoryRecords {
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn ids(&self) -> impl Iterator<Item = RecordId> + '_ {
        self.0.keys().copied()
    }
    /// Forget records no longer referenced; the caller decides which.
    pub fn retain(&mut self, mut keep: impl FnMut(RecordId) -> bool) {
        self.0.retain(|id, _| keep(*id));
    }
}

impl RecordStore for MemoryRecords {
    fn put(&mut self, id: RecordId, record: Shared<RegionRecord>) {
        let previous = self.0.insert(id, record.clone());
        assert!(
            previous.is_none_or(|previous| previous == record),
            "record {id:?} was put again with different content"
        );
    }
    fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>> {
        self.0.get(&id).cloned()
    }
}

/// Identities in detached regions. Identities stay reserved while detached,
/// so references to them stay valid and are never reused.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Directory {
    actors: BTreeMap<ActorId, RegionId>,
    items: BTreeMap<ItemId, RegionId>,
    doors: BTreeMap<u64, RegionId>,
}

/// Everything located in one detached region: its world slice, the actors
/// anchored there (with their AI and navigation), its ground items, and the
/// items those actors carry. A record needs nothing outside itself to be
/// stored or restored. Its contents are private: storage encodes it and
/// hands it back, and only the game reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionRecord {
    world: RegionSlice,
    actors: BTreeMap<ActorId, Actor>,
    stamps: BTreeMap<ActorId, u64>,
    ai: BTreeMap<ActorId, Ai>,
    navigation: BTreeMap<ActorId, Navigation>,
    items: BTreeMap<ItemId, Item>,
    displaced: BTreeSet<ActorId>,
    impacts: Vec<Impact>,
}

impl RegionRecord {
    /// Invariants of the record alone, as the record of `region` detached no
    /// later than `tick`.
    fn valid_contents(&self, region: RegionId, tick: u64) -> bool {
        self.world.id() == region
            && self.world.valid()
            && self
                .actors
                .values()
                .all(|a| a.location.region == region && a.body.valid() && a.motion.valid())
            && self.stamps.keys().eq(self.actors.keys())
            && self.stamps.values().all(|stamp| *stamp <= tick)
            && self.ai.keys().all(|id| self.actors.contains_key(id))
            && self
                .navigation
                .keys()
                .all(|id| self.actors.contains_key(id))
            && self.displaced.iter().all(|id| self.actors.contains_key(id))
            && self.items.values().all(|item| match item.location {
                ItemLocation::Ground(at) => at.region == region,
                ItemLocation::Carried(carrier) => self.actors.contains_key(&carrier),
            })
            && self.impacts.iter().all(|impact| match impact.entity {
                PhysicsEntity::Actor(id) => self.actors.contains_key(&id),
                PhysicsEntity::Item(id) => self.items.contains_key(&id),
            })
    }

    /// Whether the record holds exactly the identities `directory` places in
    /// `region`.
    fn matches_directory(&self, region: RegionId, directory: &Directory) -> bool {
        fn listed<K: Copy + Ord>(map: &BTreeMap<K, RegionId>, region: RegionId) -> BTreeSet<K> {
            map.iter()
                .filter(|(_, r)| **r == region)
                .map(|(id, _)| *id)
                .collect()
        }
        listed(&directory.actors, region) == self.actors.keys().copied().collect()
            && listed(&directory.items, region) == self.items.keys().copied().collect()
            && listed(&directory.doors, region) == self.world.door_ids().collect()
    }
}

/// Game-wide lifecycle state. Empty in games that never stream, and then
/// omitted from saves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lifecycle {
    points: BTreeMap<ReferencePointId, ReferencePoint>,
    next_point: u64,
    /// Loaded regions whose time is stopped.
    frozen: BTreeSet<RegionId>,
    /// When each actor in a frozen region stopped. On thaw its tick fields
    /// move forward by the time since, so nothing catches up.
    stamps: BTreeMap<ActorId, u64>,
    /// Each detached region's record, kept in a [`RecordStore`].
    detached: BTreeMap<RegionId, RecordId>,
    next_record: u64,
    /// Regions known from metadata only, never built.
    unbuilt: BTreeSet<RegionId>,
    /// Identities in detached and unbuilt regions. Shared, so cloning a game
    /// doesn't copy them.
    directory: Shared<Directory>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            points: BTreeMap::new(),
            next_point: 1,
            frozen: BTreeSet::new(),
            stamps: BTreeMap::new(),
            detached: BTreeMap::new(),
            next_record: 1,
            unbuilt: BTreeSet::new(),
            directory: Shared::default(),
        }
    }
}

impl Lifecycle {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Axis steps, in every direction a body can move, reach or fall.
const AXES: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];

/// Axis steps covering movement (a diagonal is two), melee (up to three)
/// and anything the next action could touch.
const REACH_STEPS: usize = 3;

impl Game {
    // ----- Reference points -----

    pub fn add_reference_point(
        &mut self,
        point: ReferencePoint,
    ) -> Result<ReferencePointId, GameError> {
        if self.target_region(point.target).is_none() {
            return Err(GameError::InvalidLocation);
        }
        let next = self
            .lifecycle
            .next_point
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let id = ReferencePointId(self.lifecycle.next_point);
        self.lifecycle.points.insert(id, point);
        self.lifecycle.next_point = next;
        Ok(id)
    }

    pub fn remove_reference_point(&mut self, id: ReferencePointId) -> bool {
        self.lifecycle.points.remove(&id).is_some()
    }

    pub fn reference_points(&self) -> impl Iterator<Item = (ReferencePointId, &ReferencePoint)> {
        self.lifecycle.points.iter().map(|(id, point)| (*id, point))
    }

    /// Give every character without a point following it an observing point
    /// with default radii. This is the default setup, not a rule: nothing
    /// else treats characters differently.
    pub fn add_default_reference_points(&mut self) -> Result<Vec<ReferencePointId>, GameError> {
        let followed: BTreeSet<_> = self
            .lifecycle
            .points
            .values()
            .filter_map(|p| match p.target {
                ReferenceTarget::Actor(id) => Some(id),
                _ => None,
            })
            .collect();
        let missing: Vec<_> = self
            .combat
            .characters
            .iter()
            .filter(|id| !followed.contains(id))
            .copied()
            .collect();
        missing
            .into_iter()
            .map(|id| {
                self.add_reference_point(ReferencePoint {
                    target: ReferenceTarget::Actor(id),
                    active_radius: None,
                    load_radius: None,
                    observes: true,
                })
            })
            .collect()
    }

    /// Every reference point with the region its target is in, in id order.
    pub fn region_roots(&self) -> Vec<RegionRoot> {
        self.lifecycle
            .points
            .iter()
            .filter_map(|(id, point)| {
                Some(RegionRoot {
                    point: *id,
                    region: self.target_region(point.target)?,
                    active_radius: point.active_radius,
                    load_radius: point.load_radius,
                    observes: point.observes,
                })
            })
            .collect()
    }

    pub fn region_state(&self, region: RegionId) -> Option<RegionState> {
        if self.lifecycle.unbuilt.contains(&region) {
            Some(RegionState::Unbuilt)
        } else if self.lifecycle.detached.contains_key(&region) {
            Some(RegionState::Detached)
        } else if self.world.region(region).is_none() {
            None
        } else if self.lifecycle.frozen.contains(&region) {
            Some(RegionState::Frozen)
        } else {
            Some(RegionState::Active)
        }
    }

    fn actor_region(&self, id: ActorId) -> Option<RegionId> {
        match self.actors.get(&id) {
            Some(actor) => Some(actor.location.region),
            None => self.lifecycle.directory.actors.get(&id).copied(),
        }
    }

    fn target_region(&self, target: ReferenceTarget) -> Option<RegionId> {
        match target {
            ReferenceTarget::Actor(id) => self.actor_region(id),
            ReferenceTarget::Item(id) => match self.items.get(&id) {
                Some(item) => match item.location {
                    ItemLocation::Ground(at) => Some(at.region),
                    ItemLocation::Carried(carrier) => self.actor_region(carrier),
                },
                None => self.lifecycle.directory.items.get(&id).copied(),
            },
            ReferenceTarget::Location(at) => self.world.knows(at).then_some(at.region),
        }
    }

    // ----- Frozen time -----

    pub(crate) fn region_frozen(&self, region: RegionId) -> bool {
        !self.lifecycle.frozen.is_empty() && self.lifecycle.frozen.contains(&region)
    }

    pub(crate) fn actor_frozen(&self, id: ActorId) -> bool {
        !self.lifecycle.stamps.is_empty() && self.lifecycle.stamps.contains_key(&id)
    }

    /// The tick an actor's time stands at: when it froze, or now.
    pub(crate) fn actor_clock(&self, id: ActorId) -> u64 {
        self.lifecycle.stamps.get(&id).copied().unwrap_or(self.tick)
    }

    /// Freeze or thaw one actor to match its region, after it moved.
    pub(crate) fn sync_actor_lifecycle(&mut self, id: ActorId) {
        if self.lifecycle.frozen.is_empty() && self.lifecycle.stamps.is_empty() {
            return;
        }
        let Some(region) = self.actors.get(&id).map(|a| a.location.region) else {
            return;
        };
        match (
            self.lifecycle.frozen.contains(&region),
            self.lifecycle.stamps.contains_key(&id),
        ) {
            (true, false) => {
                self.lifecycle.stamps.insert(id, self.tick);
            }
            (false, true) => self.thaw_actor(id),
            _ => {}
        }
    }

    fn actors_in(&self, region: RegionId) -> Vec<ActorId> {
        self.actors
            .iter()
            .filter(|(_, a)| a.location.region == region)
            .map(|(id, _)| *id)
            .collect()
    }

    fn freeze_region(&mut self, region: RegionId) {
        for id in self.actors_in(region) {
            self.lifecycle.stamps.entry(id).or_insert(self.tick);
        }
        self.lifecycle.frozen.insert(region);
    }

    fn thaw_region(&mut self, region: RegionId) {
        self.lifecycle.frozen.remove(&region);
        for id in self.actors_in(region) {
            self.thaw_actor(id);
        }
    }

    /// Shift every absolute tick an actor holds by the time it was frozen.
    fn thaw_actor(&mut self, id: ActorId) {
        let Some(stamp) = self.lifecycle.stamps.remove(&id) else {
            return;
        };
        let delta = self.tick - stamp;
        if delta == 0 {
            return;
        }
        let actor = self.actors.get_mut(&id).expect("stamped actor");
        actor.ready_at = actor.ready_at.saturating_add(delta);
        if let Some(pending) = actor.combat.as_mut().and_then(|c| c.pending.as_mut()) {
            pending.started += delta;
        }
        if let Some((_, _, seen)) = self
            .combat
            .ai
            .get_mut(&id)
            .and_then(|ai| ai.target.as_mut())
        {
            *seen += delta;
        }
    }

    // ----- Pins -----

    /// Regions that must be active and loaded, given proposed sets. Computed
    /// from loaded state: a detached region's own requirements appear once
    /// it's attached, which [`Game::settle_region_transition`] handles.
    pub fn region_requirements(
        &self,
        active: &BTreeSet<RegionId>,
        loaded: &BTreeSet<RegionId>,
    ) -> RegionTransition {
        let mut need = RegionTransition::default();
        for point in self.lifecycle.points.values() {
            let Some(region) = self.target_region(point.target) else {
                continue;
            };
            need.active.insert(region);
            if point.observes {
                if let Some((eye, frame, stairs)) = self.observer_eye(point.target) {
                    // Everything an observer sees is active; everything its
                    // view reads is loaded, so the view is exact.
                    need.active.extend(
                        self.world
                            .eye_scene(eye, frame, SIGHT_RANGE)
                            .iter()
                            .map(|cell| cell.location.region),
                    );
                    need.active.extend(stairs.iter().copied());
                    if let Some(regions) = self.world.eye_scene_regions(eye, frame, SIGHT_RANGE) {
                        need.loaded.extend(regions);
                    }
                }
            }
        }
        for id in &self.combat.input_boundaries {
            if let Some(region) = self.actor_region(*id) {
                need.active.insert(region);
            }
        }
        for (id, actor) in &self.actors {
            let region = actor.location.region;
            let is_active = active.contains(&region);
            if !is_active && !loaded.contains(&region) {
                continue;
            }
            // Live references: the body and any attack in progress.
            let mut live = self.body_regions(*id);
            let target = actor
                .combat
                .as_ref()
                .and_then(|c| c.pending.as_ref())
                .map(|p| p.target);
            if let Some(target) = target {
                if self.actors.contains_key(&target) {
                    live.extend(self.body_regions(target));
                } else if let Some(region) = self.actor_region(target) {
                    live.insert(region);
                }
            }
            if is_active {
                need.active.extend(live);
                if actor.alive() {
                    need.active.extend(self.reach_regions(*id));
                    if let Some((eye, frame)) = self.eye(*id) {
                        if let Some(regions) = self.world.eye_scene_regions(eye, frame, SIGHT_RANGE)
                        {
                            need.loaded.extend(regions);
                        }
                    }
                }
            } else {
                need.loaded.extend(live);
            }
        }
        // Anything in an active region can move one cell at a time into a
        // linked region, so linked regions stay loaded and it freezes there.
        for region in active.iter().chain(need.active.iter()) {
            need.loaded.extend(self.world.linked_regions(*region));
        }
        need.loaded.extend(need.active.iter().copied());
        need
    }

    /// Where an observing point's view starts, with any stair landings it
    /// discloses.
    fn observer_eye(&self, target: ReferenceTarget) -> Option<(Location, u8, Vec<RegionId>)> {
        let actor_eye = |id: ActorId| {
            let (eye, frame) = self.eye(id)?;
            let at = self.actors[&id].location;
            let stairs = [Direction::Up, Direction::Down]
                .into_iter()
                .filter(|d| self.world.is_stair(at, *d))
                .filter_map(|d| self.world.passage(at, d).map(|p| p.to.region))
                .collect();
            Some((eye, frame, stairs))
        };
        match target {
            ReferenceTarget::Actor(id) => actor_eye(id),
            ReferenceTarget::Item(id) => match self.items.get(&id)?.location {
                ItemLocation::Ground(at) => Some((at, 0, Vec::new())),
                ItemLocation::Carried(carrier) => actor_eye(carrier),
            },
            ReferenceTarget::Location(at) => self.world.contains(at).then_some((at, 0, Vec::new())),
        }
    }

    /// Regions holding a loaded actor's body. A body that doesn't resolve
    /// (part of it is in a missing region) counts its anchor region and
    /// every region linked from it.
    fn body_regions(&self, id: ActorId) -> BTreeSet<RegionId> {
        let actor = &self.actors[&id];
        match self.body_cells(actor.location, actor.orientation, &actor.body) {
            Some(cells) => cells.iter().map(|(at, _)| at.region).collect(),
            None => {
                let mut regions = self.world.linked_regions(actor.location.region);
                regions.insert(actor.location.region);
                regions
            }
        }
    }

    /// Regions within [`REACH_STEPS`] axis steps of an actor's body, by any
    /// movement, physics or sight step. The next action can't touch anything
    /// farther.
    fn reach_regions(&self, id: ActorId) -> BTreeSet<RegionId> {
        let actor = &self.actors[&id];
        let mut frontier: BTreeSet<Location> =
            match self.body_cells(actor.location, actor.orientation, &actor.body) {
                Some(cells) => cells.into_iter().map(|(at, _)| at).collect(),
                None => BTreeSet::from([actor.location]),
            };
        let mut seen = frontier.clone();
        for _ in 0..REACH_STEPS {
            let mut next = BTreeSet::new();
            for at in &frontier {
                for direction in AXES {
                    let steps = [
                        self.world
                            .physics_neighbor(*at, direction)
                            .map(|(to, _)| to),
                        self.world
                            .movement_neighbor(*at, direction)
                            .map(|(to, _)| to),
                        self.world.adjacent(*at, direction),
                    ];
                    for to in steps.into_iter().flatten() {
                        if seen.insert(to) {
                            next.insert(to);
                        }
                    }
                }
            }
            frontier = next;
        }
        seen.into_iter().map(|at| at.region).collect()
    }

    /// Grow proposed sets until every pin that loaded state reveals holds.
    pub fn settle_region_transition(&self, proposed: &RegionTransition) -> RegionTransition {
        let mut t = proposed.clone();
        t.loaded.extend(t.active.iter().copied());
        loop {
            let need = self.region_requirements(&t.active, &t.loaded);
            let before = (t.active.len(), t.loaded.len());
            t.active.extend(need.active);
            t.loaded.extend(need.loaded);
            t.loaded.extend(t.active.iter().copied());
            if (t.active.len(), t.loaded.len()) == before {
                return t;
            }
        }
    }

    /// Settle and apply, growing the sets when attaching a region reveals
    /// further pins. Returns the sets applied.
    pub fn transition_regions(
        &mut self,
        proposed: &RegionTransition,
        records: &mut dyn RecordStore,
    ) -> Result<(RegionTransition, TransitionReport), TransitionError> {
        let mut t = self.settle_region_transition(proposed);
        loop {
            match self.apply_region_transition(&t, records) {
                Ok(report) => return Ok((t, report)),
                Err(TransitionError::MustBeActive(region)) if !t.active.contains(&region) => {
                    t.active.insert(region);
                    t.loaded.insert(region);
                }
                Err(TransitionError::MustBeLoaded(region)) if !t.loaded.contains(&region) => {
                    t.loaded.insert(region);
                }
                Err(error) => return Err(error),
            }
        }
    }

    // ----- Transitions -----

    /// Apply region sets atomically, between actions. Attaches, freezes,
    /// thaws and detaches in region order, then checks every pin; on any
    /// error the game and `records` are unchanged. Records of detached
    /// regions are put in `records` only once everything has succeeded. If
    /// the actor due next froze, time advances to the next decision, as after
    /// an action.
    pub fn apply_region_transition(
        &mut self,
        t: &RegionTransition,
        records: &mut dyn RecordStore,
    ) -> Result<TransitionReport, TransitionError> {
        for region in t.loaded.iter().chain(&t.active) {
            if !self.world.knows_region(*region) {
                return Err(TransitionError::UnknownRegion(*region));
            }
        }
        if let Some(region) = t.active.difference(&t.loaded).next() {
            return Err(TransitionError::ActiveNotLoaded(*region));
        }
        let mut next = self.clone();
        let mut report = TransitionReport::default();
        let attach: Vec<_> = t
            .loaded
            .iter()
            .filter(|r| {
                next.lifecycle.detached.contains_key(r) || next.lifecycle.unbuilt.contains(r)
            })
            .copied()
            .collect();
        for region in attach {
            if next.lifecycle.unbuilt.contains(&region) {
                report.built.push(region);
            }
            next.attach_record(region, records)?;
            report.attached.push(region);
        }
        let loaded: Vec<_> = next.world.loaded_regions().collect();
        for region in &loaded {
            match (
                t.active.contains(region),
                next.lifecycle.frozen.contains(region),
            ) {
                (true, true) => {
                    next.thaw_region(*region);
                    report.thawed.push(*region);
                }
                (false, false) => {
                    next.freeze_region(*region);
                    report.frozen.push(*region);
                }
                _ => {}
            }
        }
        let mut made = Vec::new();
        for region in loaded {
            if !t.loaded.contains(&region) {
                made.push(next.detach_record(region)?);
                report.detached.push(region);
            }
        }
        let need = next.region_requirements(&t.active, &t.loaded);
        if let Some(region) = need.active.difference(&t.active).next() {
            return Err(TransitionError::MustBeActive(*region));
        }
        if let Some(region) = need.loaded.difference(&t.loaded).next() {
            return Err(TransitionError::MustBeLoaded(*region));
        }
        if !report.built.is_empty() {
            // A character may start where the objective is met, as it would
            // have if everything had been built at once.
            next.check_objective();
        }
        if !report.frozen.is_empty() {
            next.advance_to_next_decision();
        }
        debug_assert!(
            next.lifecycle_state_valid(),
            "transition kept lifecycle state valid"
        );
        for (id, record) in made {
            records.put(id, record);
        }
        *self = next;
        Ok(report)
    }

    /// Continue allocating record identities after `from`'s, so identities
    /// stay unique when this game replaces a later one. A rewind restores an
    /// earlier game whose allocator is behind records the abandoned future
    /// made, and retained boundaries may still refer to them.
    pub fn continue_record_ids(&mut self, from: &Game) {
        let next = &mut self.lifecycle.next_record;
        *next = (*next).max(from.lifecycle.next_record);
    }

    /// Each detached region with its record's identity, in region order.
    pub fn detached_records(&self) -> impl Iterator<Item = (RegionId, RecordId)> + '_ {
        self.lifecycle.detached.iter().map(|(r, id)| (*r, *id))
    }

    /// Know a region that isn't built yet, with the identities it will hold.
    /// Scenario setup only: the region is built from its source when it's
    /// first loaded, and those identities stay reserved until then, so
    /// references to them (an objective item, a character) are checkable.
    pub fn add_unbuilt_region(
        &mut self,
        region: Region,
        chamber: bool,
        identities: RegionIdentities,
    ) -> Result<(), GameError> {
        let id = region.id;
        let directory = &self.lifecycle.directory;
        let taken = identities
            .actors
            .iter()
            .any(|a| a.0 == 0 || self.actors.contains_key(a) || directory.actors.contains_key(a))
            || identities
                .items
                .iter()
                .any(|i| i.0 == 0 || self.items.contains_key(i) || directory.items.contains_key(i))
            || identities.doors.iter().any(|d| {
                *d == 0 || self.world.door_location(*d).is_some() || directory.doors.contains_key(d)
            });
        let next = |ids: &mut dyn Iterator<Item = u64>, current: u64| {
            ids.max().map_or(Some(current), |max| {
                max.checked_add(1).map(|n| n.max(current))
            })
        };
        let (Some(next_actor), Some(next_item), Some(next_door)) = (
            next(
                &mut identities.actors.iter().map(|a| a.0),
                self.next_actor_id,
            ),
            next(&mut identities.items.iter().map(|i| i.0), self.next_item_id),
            next(&mut identities.doors.iter().copied(), self.next_door_id),
        ) else {
            return Err(GameError::IdentityExhausted);
        };
        if taken {
            return Err(GameError::IdentityExhausted);
        }
        let mut world = (*self.world).clone();
        world
            .add_unbuilt_region(region, chamber)
            .map_err(|_| GameError::InvalidLocation)?;
        self.world = Shared::new(world);
        let directory = &mut self.lifecycle.directory;
        directory
            .actors
            .extend(identities.actors.iter().map(|a| (*a, id)));
        directory
            .items
            .extend(identities.items.iter().map(|i| (*i, id)));
        directory
            .doors
            .extend(identities.doors.iter().map(|d| (*d, id)));
        self.lifecycle.unbuilt.insert(id);
        (self.next_actor_id, self.next_item_id, self.next_door_id) =
            (next_actor, next_item, next_door);
        Ok(())
    }

    /// Turn a loaded region of this game into a starting record for
    /// [`RecordStore::build`]. A region source builds the region in a
    /// scratch game (with its neighbours' geometry, so its links check) at
    /// tick zero, then takes its record here: the region freezes at tick
    /// zero, so its time starts when it's first active.
    pub fn into_region_record(mut self, region: RegionId) -> Result<RegionRecord, GameError> {
        if self.world.region(region).is_none() || self.tick != 0 {
            return Err(GameError::InvalidLocation);
        }
        self.freeze_region(region);
        let (_, record) = self
            .detach_record(region)
            .map_err(|_| GameError::InvalidLocation)?;
        Ok((*record).clone())
    }

    /// Detach a loaded region, returning its new record for the store.
    fn detach_record(
        &mut self,
        region: RegionId,
    ) -> Result<(RecordId, Shared<RegionRecord>), TransitionError> {
        let invalid = TransitionError::InvalidRecord(region);
        let world = self.world.detach_region(region).map_err(|_| invalid)?;
        let actors = self.actors_in(region);
        let carried: BTreeSet<_> = actors.iter().copied().collect();
        let items: Vec<ItemId> = self
            .items
            .iter()
            .filter(|(_, item)| match item.location {
                ItemLocation::Ground(at) => at.region == region,
                ItemLocation::Carried(carrier) => carried.contains(&carrier),
            })
            .map(|(id, _)| *id)
            .collect();
        let owned = |entity: &PhysicsEntity| match entity {
            PhysicsEntity::Actor(id) => carried.contains(id),
            PhysicsEntity::Item(id) => items.contains(id),
        };
        let (impacts, kept) = std::mem::take(&mut self.physics.impacts)
            .into_iter()
            .partition(|impact| owned(&impact.entity));
        self.physics.impacts = kept;
        let directory = &mut self.lifecycle.directory;
        directory
            .doors
            .extend(world.door_ids().map(|door| (door, region)));
        directory
            .actors
            .extend(actors.iter().map(|id| (*id, region)));
        directory.items.extend(items.iter().map(|id| (*id, region)));
        let mut record = RegionRecord {
            world,
            actors: BTreeMap::new(),
            stamps: BTreeMap::new(),
            ai: BTreeMap::new(),
            navigation: BTreeMap::new(),
            items: BTreeMap::new(),
            displaced: BTreeSet::new(),
            impacts,
        };
        for id in actors {
            record
                .actors
                .insert(id, self.actors.remove(&id).expect("listed actor"));
            if let Some(stamp) = self.lifecycle.stamps.remove(&id) {
                record.stamps.insert(id, stamp);
            }
            if let Some(ai) = self.combat.ai.remove(&id) {
                record.ai.insert(id, ai);
            }
            if let Some(navigation) = self.navigation.remove(&id) {
                record.navigation.insert(id, (*navigation).clone());
            }
            if self.physics.displaced.remove(&id) {
                record.displaced.insert(id);
            }
        }
        if !items.is_empty() {
            for id in items {
                record
                    .items
                    .insert(id, self.items.remove(&id).expect("listed item"));
            }
        }
        let id = RecordId(self.lifecycle.next_record);
        self.lifecycle.next_record = id.0.checked_add(1).ok_or(invalid)?;
        self.lifecycle.frozen.remove(&region);
        self.lifecycle.detached.insert(region, id);
        Ok((id, Shared::new(record)))
    }

    fn attach_record(
        &mut self,
        region: RegionId,
        records: &mut dyn RecordStore,
    ) -> Result<(), TransitionError> {
        let invalid = TransitionError::InvalidRecord(region);
        let unavailable = TransitionError::RecordUnavailable(region);
        let record = if self.lifecycle.unbuilt.remove(&region) {
            Shared::new(records.build(region).ok_or(unavailable)?)
        } else {
            let id = self.lifecycle.detached.remove(&region).ok_or(invalid)?;
            records.get(id).ok_or(unavailable)?
        };
        // Checked here, not only on restore, because restore never reads
        // records, and a built record must hold exactly the identities its
        // source declared.
        if !record.valid_contents(region, self.tick)
            || !record.matches_directory(region, &self.lifecycle.directory)
        {
            return Err(invalid);
        }
        let RegionRecord {
            world,
            actors,
            stamps,
            ai,
            navigation,
            items,
            displaced,
            impacts,
        } = (*record).clone();
        if actors.keys().any(|id| self.actors.contains_key(id))
            || items.keys().any(|id| self.items.contains_key(id))
        {
            return Err(invalid);
        }
        self.world.attach_region(world).map_err(|_| invalid)?;
        self.actors.extend(actors);
        self.lifecycle.stamps.extend(stamps);
        self.combat.ai.extend(ai);
        self.navigation
            .extend(navigation.into_iter().map(|(id, n)| (id, Shared::new(n))));
        if !items.is_empty() {
            self.items.extend(items);
        }
        self.physics.displaced.extend(displaced);
        self.physics.impacts.extend(impacts);
        let directory = &mut self.lifecycle.directory;
        directory.actors.retain(|_, r| *r != region);
        directory.items.retain(|_, r| *r != region);
        directory.doors.retain(|_, r| *r != region);
        self.lifecycle.frozen.insert(region);
        Ok(())
    }

    // ----- Validation -----

    /// Whether an identity refers to something detached.
    pub(crate) fn detached_actor(&self, id: ActorId) -> bool {
        self.lifecycle.directory.actors.contains_key(&id)
    }

    pub(crate) fn detached_item(&self, id: ItemId) -> bool {
        self.lifecycle.directory.items.contains_key(&id)
    }

    pub(crate) fn has_detached_actors(&self) -> bool {
        !self.lifecycle.directory.actors.is_empty()
    }

    /// Every detached record is available and valid, and together they hold
    /// exactly the directory's identities. Reads every record, so restoring
    /// a game doesn't do this; attaching checks each record instead.
    pub fn detached_records_valid(&self, records: &mut dyn RecordStore) -> bool {
        let l = &self.lifecycle;
        let mut expected = Directory::default();
        l.detached.iter().all(|(region, id)| {
            let Some(record) = records.get(*id) else {
                return false;
            };
            expected
                .doors
                .extend(record.world.door_ids().map(|door| (door, *region)));
            expected
                .actors
                .extend(record.actors.keys().map(|id| (*id, *region)));
            expected
                .items
                .extend(record.items.keys().map(|id| (*id, *region)));
            record.valid_contents(*region, self.tick)
        }) && {
            // Unbuilt regions' identities are declared, not recorded.
            let recorded = |r: &RegionId| !l.unbuilt.contains(r);
            let mut directory = (*l.directory).clone();
            directory.actors.retain(|_, r| recorded(r));
            directory.items.retain(|_, r| recorded(r));
            directory.doors.retain(|_, r| recorded(r));
            expected == directory
        }
    }

    /// Game-wide lifecycle invariants, checked when a game is restored.
    /// Doesn't read records, which may be stored elsewhere.
    pub(crate) fn lifecycle_state_valid(&self) -> bool {
        let l = &self.lifecycle;
        let detached =
            |region: &RegionId| l.detached.contains_key(region) || l.unbuilt.contains(region);
        let mut ids = BTreeSet::new();
        l.next_record > 0
            && l.unbuilt.iter().all(|r| !l.detached.contains_key(r))
            && l.detached
                .values()
                .all(|id| id.0 > 0 && id.0 < l.next_record && ids.insert(*id))
            && l.directory.actors.values().all(detached)
            && l.directory.items.values().all(detached)
            && l.directory.doors.values().all(detached)
            && l.directory
                .actors
                .keys()
                .all(|id| !self.actors.contains_key(id) && id.0 > 0 && id.0 < self.next_actor_id)
            && l.directory
                .items
                .keys()
                .all(|id| !self.items.contains_key(id) && id.0 > 0 && id.0 < self.next_item_id)
            && l.directory.doors.keys().all(|door| {
                self.world.door_location(*door).is_none() && *door > 0 && *door < self.next_door_id
            })
            && self.world.detached_regions().eq(l
                .detached
                .keys()
                .chain(&l.unbuilt)
                .copied()
                .collect::<BTreeSet<_>>())
            && l.frozen.iter().all(|r| self.world.region(*r).is_some())
            && l.next_point > 0
            && l.points.keys().all(|id| id.0 > 0 && id.0 < l.next_point)
            && l.points
                .values()
                .all(|point| self.target_region(point.target).is_some())
            && l.stamps.iter().all(|(id, stamp)| {
                *stamp <= self.tick
                    && self.actors.get(id).is_some_and(|a| {
                        l.frozen.contains(&a.location.region)
                            && (!a.alive() || a.ready_at >= *stamp)
                    })
            })
            && self
                .actors
                .iter()
                .all(|(id, a)| !l.frozen.contains(&a.location.region) || l.stamps.contains_key(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Action;
    use std::num::NonZeroU64;
    use tor_world::Position;

    #[test]
    fn thawing_shifts_ai_memory_so_it_cannot_expire_while_frozen() {
        let at = |region, x| Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        };
        let mut game = Game::region_corridor(1, 4);
        let player = game
            .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let watcher = game
            .spawn_actor(at(3, 5), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_combat(watcher, Default::default()).unwrap();
        let profile = crate::ai::AiProfile {
            memory_ticks: 150,
            flee_percent: 25,
        };
        game.configure_ai(watcher, profile).unwrap();
        game.configure_run(player, BTreeSet::from([player]), None, BTreeMap::new())
            .unwrap();
        game.add_default_reference_points().unwrap();
        game.combat.ai.get_mut(&watcher).unwrap().target = Some((player, at(1, 2), 0));
        let mut records = MemoryRecords::default();
        game.transition_regions(&sets(&[1], &[1]), &mut records)
            .unwrap();
        for _ in 0..4 {
            game.act(player, Action::Wait).unwrap();
        }
        // Longer than the watcher's memory, but it was frozen throughout.
        assert_eq!(game.tick(), 400);
        game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
            .unwrap();
        let (_, _, seen) = game.combat.ai[&watcher].target.unwrap();
        assert_eq!(seen, 400);
    }

    /// A player in region 1 of a four-region corridor and another actor in
    /// region 3, with everything but region 1 and its neighbour detached.
    fn detached_corridor() -> (Game, ActorId, MemoryRecords) {
        let at = |region, x| Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        };
        let mut game = Game::region_corridor(1, 4);
        let player = game
            .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let other = game
            .spawn_actor(at(3, 5), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_run(player, BTreeSet::from([player]), None, BTreeMap::new())
            .unwrap();
        game.add_default_reference_points().unwrap();
        let mut records = MemoryRecords::default();
        game.transition_regions(&sets(&[1], &[1]), &mut records)
            .unwrap();
        assert_eq!(game.region_state(RegionId(3)), Some(RegionState::Detached));
        (game, other, records)
    }

    fn sets(active: &[u64], loaded: &[u64]) -> RegionTransition {
        let only = |ids: &[u64]| ids.iter().map(|id| RegionId(*id)).collect::<BTreeSet<_>>();
        RegionTransition {
            active: only(active),
            loaded: only(loaded),
        }
    }

    #[test]
    fn attach_rejects_a_record_that_does_not_match_the_directory() {
        let (mut game, other, mut records) = detached_corridor();
        assert!(game.lifecycle_state_valid());
        assert!(game.detached_records_valid(&mut records));

        // A record that's valid on its own but lost an actor the directory
        // still places in its region, as a damaged store might return.
        let three = RegionId(3);
        let mut damaged = MemoryRecords::default();
        for (region, id) in game.detached_records() {
            let mut record = (*records.get(id).unwrap()).clone();
            if region == three {
                assert!(record.actors.remove(&other).is_some());
                record.stamps.remove(&other);
                record.ai.remove(&other);
                record.navigation.remove(&other);
                record.displaced.remove(&other);
                assert!(record.valid_contents(three, game.tick()));
                assert!(!record.matches_directory(three, &game.lifecycle.directory));
            }
            damaged.put(id, Shared::new(record));
        }
        assert!(!game.detached_records_valid(&mut damaged));

        let before = game.clone();
        let error = game
            .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut damaged)
            .unwrap_err();
        assert_eq!(error, TransitionError::InvalidRecord(three));
        assert_eq!(game, before);
    }

    #[test]
    fn a_missing_record_fails_the_transition_without_changes() {
        let (mut game, _, mut records) = detached_corridor();
        let (_, lost) = game
            .detached_records()
            .find(|(region, _)| *region == RegionId(3))
            .unwrap();
        records.retain(|id| id != lost);
        let before = game.clone();
        let error = game
            .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
            .unwrap_err();
        assert_eq!(error, TransitionError::RecordUnavailable(RegionId(3)));
        assert_eq!(game, before);
    }

    #[test]
    fn records_are_put_only_when_a_transition_succeeds() {
        let (mut game, _, mut records) = detached_corridor();
        game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
            .unwrap();
        let known: Vec<_> = records.ids().collect();
        // Region 1 holds the observing point, so it can't be detached.
        let before = game.clone();
        let error = game
            .apply_region_transition(&sets(&[], &[2, 3, 4]), &mut records)
            .unwrap_err();
        assert!(matches!(error, TransitionError::MustBeActive(_)));
        assert_eq!(game, before);
        assert_eq!(records.ids().collect::<Vec<_>>(), known);
    }

    #[test]
    fn record_ids_are_never_reused_across_a_rewind() {
        let (mut game, _, mut records) = detached_corridor();
        let earlier = game.clone();
        game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
            .unwrap();
        game.transition_regions(&sets(&[1], &[1]), &mut records)
            .unwrap();
        let later: BTreeSet<_> = game.detached_records().map(|(_, id)| id).collect();

        // Rewinding to `earlier` and detaching again must not reuse the
        // later records' identities, which retained boundaries may hold.
        let mut rewound = earlier.clone();
        rewound.continue_record_ids(&game);
        rewound
            .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
            .unwrap();
        rewound
            .transition_regions(&sets(&[1], &[1]), &mut records)
            .unwrap();
        assert!(rewound
            .detached_records()
            .all(|(_, id)| !later.contains(&id)));
        assert!(rewound.lifecycle_state_valid());
    }

    /// Builds regions from a fully built template game, as a scenario
    /// package would.
    struct TemplateSource {
        template: Game,
        records: MemoryRecords,
        /// Drop this actor from built records, as a faulty source might.
        lose: Option<ActorId>,
    }

    impl RecordStore for TemplateSource {
        fn put(&mut self, id: RecordId, record: Shared<RegionRecord>) {
            self.records.put(id, record);
        }
        fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>> {
            self.records.get(id)
        }
        fn build(&mut self, region: RegionId) -> Option<RegionRecord> {
            let mut record = self.template.clone().into_region_record(region).ok()?;
            if let Some(lost) = self.lose {
                record.actors.remove(&lost);
                record.stamps.remove(&lost);
            }
            Some(record)
        }
    }

    /// A four-region corridor with an actor in region 3, and the same game
    /// with every region unbuilt.
    fn unbuilt_corridor() -> (Game, Game, ActorId) {
        let mut template = Game::region_corridor(1, 4);
        let far = template
            .spawn_actor(
                Location {
                    region: RegionId(3),
                    position: Position { x: 5, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut game = Game::new(tor_world::World::new(vec![], vec![]).unwrap(), 1);
        for id in 1..=4 {
            let region = template.world.region(RegionId(id)).unwrap().clone();
            let mut identities = RegionIdentities::default();
            if id == 3 {
                identities.actors.insert(far);
            }
            game.add_unbuilt_region(region, false, identities).unwrap();
        }
        (template, game, far)
    }

    #[test]
    fn unbuilt_regions_build_from_their_source() {
        let (template, mut game, far) = unbuilt_corridor();
        assert_eq!(game.region_state(RegionId(3)), Some(RegionState::Unbuilt));
        assert!(game.lifecycle_state_valid());
        assert!(game.detached_actor(far), "declared identities are reserved");
        let mut source = TemplateSource {
            template: template.clone(),
            records: MemoryRecords::default(),
            lose: None,
        };
        let (_, report) = game
            .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut source)
            .unwrap();
        assert_eq!(report.built, report.attached);
        assert_eq!(report.built.len(), 4);
        assert_eq!(game, template);
    }

    #[test]
    fn building_needs_a_source_that_keeps_its_declared_identities() {
        let (template, game, far) = unbuilt_corridor();
        let all = sets(&[1, 2, 3, 4], &[1, 2, 3, 4]);

        let mut nothing = MemoryRecords::default();
        let mut unchanged = game.clone();
        let error = unchanged
            .transition_regions(&all, &mut nothing)
            .unwrap_err();
        assert_eq!(error, TransitionError::RecordUnavailable(RegionId(1)));
        assert_eq!(unchanged, game);

        let mut faulty = TemplateSource {
            template,
            records: MemoryRecords::default(),
            lose: Some(far),
        };
        let error = unchanged.transition_regions(&all, &mut faulty).unwrap_err();
        assert_eq!(error, TransitionError::InvalidRecord(RegionId(3)));
        assert_eq!(unchanged, game);
    }

    #[test]
    fn an_unbuilt_region_cannot_declare_an_identity_in_use() {
        let (template, mut game, far) = unbuilt_corridor();
        let mut region = template.world.region(RegionId(1)).unwrap().clone();
        region.id = RegionId(5);
        let identities = RegionIdentities {
            actors: BTreeSet::from([far]),
            ..Default::default()
        };
        let before = game.clone();
        assert!(game.add_unbuilt_region(region, false, identities).is_err());
        assert_eq!(game, before);
    }
}
