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

/// Deterministic work a transition's settling did, for performance
/// contracts: none of it may grow with the size of the world.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PinWork {
    /// Loaded actors whose live references were followed.
    pub actors: usize,
    /// Active actors whose reach and view were looked up.
    pub reaches: usize,
}

impl TransitionReport {
    /// Whether the transition changed nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
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

/// A tentative attachment can reveal several pins at once. Keep that complete
/// set internally; the public atomic operation still reports its first error.
enum TransitionFailure {
    Error(TransitionError),
    Requirements(RegionTransition),
}

impl From<TransitionError> for TransitionFailure {
    fn from(error: TransitionError) -> Self {
        Self::Error(error)
    }
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
    /// A region this store's source can build that the game doesn't know
    /// yet, so a transition can declare it: a region it's asked to load, or
    /// one a newly built region links to. Declaring only what's needed keeps
    /// game state proportional to the regions played, not the source.
    /// `None` if this store has no source, or the source has no such region.
    fn unbuilt(&mut self, region: RegionId) -> Option<UnbuiltRegion> {
        let _ = region;
        None
    }
}

/// A region a source can build, as [`Game::add_unbuilt_region`] declares it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnbuiltRegion {
    pub region: Region,
    pub chamber: bool,
    pub identities: RegionIdentities,
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

/// Identities in detached and unbuilt regions. Identities stay reserved
/// while their region isn't loaded, so references to them stay valid and are
/// never reused.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "DirectoryData", into = "DirectoryData")]
struct Directory {
    actors: BTreeMap<ActorId, RegionId>,
    items: BTreeMap<ItemId, RegionId>,
    doors: BTreeMap<u64, RegionId>,
    /// Derived from the maps above and never saved: each region's
    /// identities, so attaching a region touches only its own.
    by_region: BTreeMap<RegionId, RegionIdentities>,
}

/// A directory as saved.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryData {
    actors: BTreeMap<ActorId, RegionId>,
    items: BTreeMap<ItemId, RegionId>,
    doors: BTreeMap<u64, RegionId>,
}

impl From<DirectoryData> for Directory {
    fn from(data: DirectoryData) -> Self {
        let mut by_region: BTreeMap<RegionId, RegionIdentities> = BTreeMap::new();
        for (id, region) in &data.actors {
            by_region.entry(*region).or_default().actors.insert(*id);
        }
        for (id, region) in &data.items {
            by_region.entry(*region).or_default().items.insert(*id);
        }
        for (id, region) in &data.doors {
            by_region.entry(*region).or_default().doors.insert(*id);
        }
        Self {
            actors: data.actors,
            items: data.items,
            doors: data.doors,
            by_region,
        }
    }
}

impl From<Directory> for DirectoryData {
    fn from(directory: Directory) -> Self {
        Self {
            actors: directory.actors,
            items: directory.items,
            doors: directory.doors,
        }
    }
}

impl Directory {
    /// Place `identities` in `region`. The index lists only regions with
    /// identities, as rebuilding it after a save round trip does.
    fn insert(&mut self, region: RegionId, identities: RegionIdentities) {
        if identities == RegionIdentities::default() {
            return;
        }
        self.actors
            .extend(identities.actors.iter().map(|id| (*id, region)));
        self.items
            .extend(identities.items.iter().map(|id| (*id, region)));
        self.doors
            .extend(identities.doors.iter().map(|id| (*id, region)));
        let listed = self.by_region.entry(region).or_default();
        listed.actors.extend(identities.actors);
        listed.items.extend(identities.items);
        listed.doors.extend(identities.doors);
    }

    /// Remove and return `region`'s identities, touching only those.
    fn remove_region(&mut self, region: RegionId) -> RegionIdentities {
        let identities = self.by_region.remove(&region).unwrap_or_default();
        for id in &identities.actors {
            self.actors.remove(id);
        }
        for id in &identities.items {
            self.items.remove(id);
        }
        for id in &identities.doors {
            self.doors.remove(id);
        }
        identities
    }
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

    /// The identities this record holds.
    fn identities(&self) -> RegionIdentities {
        RegionIdentities {
            actors: self.actors.keys().copied().collect(),
            items: self.items.keys().copied().collect(),
            doors: self.world.door_ids().collect(),
        }
    }

    /// Whether the record holds exactly the identities `directory` places in
    /// `region`.
    fn matches_directory(&self, region: RegionId, directory: &Directory) -> bool {
        let empty = RegionIdentities::default();
        *directory.by_region.get(&region).unwrap_or(&empty) == self.identities()
    }
}

/// Game-wide lifecycle state. Empty in games that never stream, and then
/// omitted from saves.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lifecycle {
    #[serde(skip)]
    pub(crate) scenes: crate::observation::ActorScenes,
    points: BTreeMap<ReferencePointId, ReferencePoint>,
    next_point: u64,
    /// Loaded regions whose time is stopped.
    frozen: BTreeSet<RegionId>,
    /// When each actor in a frozen region stopped. On thaw its tick fields
    /// move forward by the time since, so nothing catches up.
    stamps: BTreeMap<ActorId, u64>,
    /// Each detached region's record, kept in a [`RecordStore`]. Shared, like
    /// `unbuilt` and the directory, so cloning a game (every command does)
    /// doesn't copy state that grows with the world.
    detached: Shared<BTreeMap<RegionId, RecordId>>,
    next_record: u64,
    /// Regions known from metadata only, never built.
    unbuilt: Shared<BTreeSet<RegionId>>,
    /// Identities in detached and unbuilt regions. Shared, so cloning a game
    /// doesn't copy them.
    directory: Shared<Directory>,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            scenes: crate::observation::ActorScenes::default(),
            points: BTreeMap::new(),
            next_point: 1,
            frozen: BTreeSet::new(),
            stamps: BTreeMap::new(),
            detached: Shared::default(),
            next_record: 1,
            unbuilt: Shared::default(),
            directory: Shared::default(),
        }
    }
}

impl Lifecycle {
    pub(crate) fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Pins computed for one game state, reused while proposed sets grow.
#[derive(Default)]
struct PinCache {
    points: Option<RegionTransition>,
    live: BTreeMap<ActorId, BTreeSet<RegionId>>,
    /// An active actor's reach and the regions its view reads.
    reach: BTreeMap<ActorId, (BTreeSet<RegionId>, Vec<RegionId>)>,
    linked: BTreeMap<RegionId, BTreeSet<RegionId>>,
}

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

    /// The record identity a detached region's record is kept under.
    pub fn detached_record(&self, region: RegionId) -> Option<RecordId> {
        self.lifecycle.detached.get(&region).copied()
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
                self.settle_creature_time();
                self.lifecycle.stamps.insert(id, self.tick);
            }
            (false, true) => self.thaw_actor(id),
            _ => {}
        }
    }

    fn actors_in(&self, region: RegionId) -> Vec<ActorId> {
        self.actors.in_region(region).collect()
    }

    fn freeze_region(&mut self, region: RegionId) {
        self.settle_creature_time();
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
        self.settle_creature_time();
        let Some(stamp) = self.lifecycle.stamps.remove(&id) else {
            return;
        };
        let delta = self.tick - stamp;
        if delta == 0 {
            return;
        }
        let mut actor = self.actors.get_mut(&id).expect("stamped actor");
        actor.ready_at = actor.ready_at.saturating_add(delta);
        if let Some(pending) = actor.pending.as_mut() {
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
        self.requirements(active, loaded, &mut PinCache::default())
    }

    /// [`Game::region_requirements`], reusing what `cache` already holds for
    /// this state. Each actor's pins are computed at most once however often
    /// the proposed sets grow.
    fn requirements(
        &self,
        active: &BTreeSet<RegionId>,
        loaded: &BTreeSet<RegionId>,
        cache: &mut PinCache,
    ) -> RegionTransition {
        let mut need = cache
            .points
            .get_or_insert_with(|| self.point_requirements())
            .clone();
        for (id, actor) in self.actors.iter() {
            let region = actor.location.region;
            let is_active = active.contains(&region);
            if !is_active && !loaded.contains(&region) {
                continue;
            }
            let live = cache
                .live
                .entry(*id)
                .or_insert_with(|| self.live_regions(*id));
            if !is_active {
                need.loaded.extend(live.iter().copied());
                continue;
            }
            need.active.extend(live.iter().copied());
            if actor.alive() {
                if !cache.reach.contains_key(id) {
                    let view = self
                        .eye(*id)
                        .and_then(|(eye, frame)| {
                            self.world
                                .illuminated_eye_scene_regions(eye, frame, SIGHT_RANGE)
                        })
                        .unwrap_or_default();
                    let reach = self.reach_regions(*id);
                    cache.reach.insert(*id, (reach, view));
                }
                let (reach, view) = &cache.reach[id];
                need.active.extend(reach.iter().copied());
                need.loaded.extend(view.iter().copied());
            }
        }
        // Anything in an active region can move one cell at a time into a
        // linked region, so linked regions stay loaded and it freezes there.
        let sources: Vec<_> = active.iter().chain(need.active.iter()).copied().collect();
        for region in sources {
            let linked = cache
                .linked
                .entry(region)
                .or_insert_with(|| self.world.linked_regions(region));
            need.loaded.extend(linked.iter().copied());
        }
        need.loaded.extend(need.active.iter().copied());
        need
    }

    /// Pins that don't depend on the proposed sets: reference points, what
    /// observing points see, and actors awaiting controller input.
    fn point_requirements(&self) -> RegionTransition {
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
                    let actor = match point.target {
                        ReferenceTarget::Actor(id) => Some(id),
                        ReferenceTarget::Item(id) => {
                            match self.items.get(&id).map(|i| i.location) {
                                Some(ItemLocation::Carried(actor)) => Some(actor),
                                _ => None,
                            }
                        }
                        ReferenceTarget::Location(_) => None,
                    };
                    let scene = if let Some(actor) = actor {
                        self.streaming_scene(actor).unwrap_or_default()
                    } else {
                        crate::observation::merge_perception(
                            self.world.neighborhood_scene(eye, frame),
                            self.world.illuminated_eye_scene(eye, frame, SIGHT_RANGE),
                        )
                    };
                    need.active
                        .extend(scene.into_iter().map(|cell| cell.location.region));
                    need.active.extend(stairs.iter().copied());
                    if let Some(regions) =
                        self.world
                            .illuminated_eye_scene_regions(eye, frame, SIGHT_RANGE)
                    {
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
        need
    }

    /// A loaded actor's live references: its body and any attack in progress.
    fn live_regions(&self, id: ActorId) -> BTreeSet<RegionId> {
        let mut live = self.body_regions(id);
        let target = self.actors[&id]
            .pending
            .as_ref()
            .and_then(|p| p.work.target());
        if let Some(target) = target {
            if self.actors.contains_key(&target) {
                live.extend(self.body_regions(target));
            } else if let Some(region) = self.actor_region(target) {
                live.insert(region);
            }
        }
        live
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
                .filter_map(|d| self.world.stair_region(at, d))
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
    /// farther. The world answers from its exit field where the body is far
    /// from every exit, so this rarely searches.
    fn reach_regions(&self, id: ActorId) -> BTreeSet<RegionId> {
        let actor = &self.actors[&id];
        let frontier: BTreeSet<Location> =
            match self.body_cells(actor.location, actor.orientation, &actor.body) {
                Some(cells) => cells.into_iter().map(|(at, _)| at).collect(),
                None => BTreeSet::from([actor.location]),
            };
        self.world.reach_regions(&frontier, REACH_STEPS)
    }

    /// Check the world's reach answers against an uncached search from every
    /// cell of every loaded region, for tests over real scenarios. Returns
    /// how many cells the exit field answered, or the first cell where the
    /// two disagree.
    pub fn check_reach(&self) -> Result<usize, Location> {
        self.world.check_reach(REACH_STEPS)
    }

    /// Grow proposed sets until every pin that loaded state reveals holds.
    pub fn settle_region_transition(&self, proposed: &RegionTransition) -> RegionTransition {
        self.settle(proposed, &mut PinCache::default())
    }

    fn settle(&self, proposed: &RegionTransition, cache: &mut PinCache) -> RegionTransition {
        let mut t = proposed.clone();
        t.loaded.extend(t.active.iter().copied());
        loop {
            let need = self.requirements(&t.active, &t.loaded, cache);
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
        self.transition_regions_counted(proposed, records)
            .map(|(t, report, _)| (t, report))
    }

    /// Settle `proposed` as [`Game::transition_regions`] would, counting the
    /// work, without applying anything.
    pub fn settle_counted(&self, proposed: &RegionTransition) -> (RegionTransition, PinWork) {
        let mut cache = PinCache::default();
        let t = self.settle(proposed, &mut cache);
        let work = PinWork {
            actors: cache.live.len(),
            reaches: cache.reach.len(),
        };
        (t, work)
    }

    /// Whether `t` names exactly the regions active and loaded now. A
    /// settled transition to them changes nothing.
    pub fn regions_are(&self, t: &RegionTransition) -> bool {
        self.current_regions() == *t
    }

    /// Regions declared but not built yet, in region order.
    pub fn unbuilt_regions(&self) -> impl Iterator<Item = RegionId> + '_ {
        self.lifecycle.unbuilt.iter().copied()
    }

    /// Loaded regions (active or frozen), in region order.
    pub fn loaded_regions(&self) -> impl Iterator<Item = RegionId> + '_ {
        self.world.loaded_regions()
    }

    /// Actors in loaded regions, in identity order.
    pub fn loaded_actor_ids(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.actors.keys().copied()
    }

    /// Known actors in regions that are detached or not built, in identity
    /// order.
    pub fn unloaded_actor_ids(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.lifecycle.directory.actors.keys().copied()
    }

    /// [`Game::transition_regions`], also counting the settling's work.
    pub fn transition_regions_counted(
        &mut self,
        proposed: &RegionTransition,
        records: &mut dyn RecordStore,
    ) -> Result<(RegionTransition, TransitionReport, PinWork), TransitionError> {
        let mut cache = PinCache::default();
        let mut t = self.settle(proposed, &mut cache);
        let work = PinWork {
            actors: cache.live.len(),
            reaches: cache.reach.len(),
        };
        // Settling proved every pin holds for these sets in this state, so
        // if they're the current sets there's nothing to apply or check.
        if self.current_regions() == t {
            return Ok((t, TransitionReport::default(), work));
        }
        loop {
            match self.try_region_transition(&t, records) {
                Ok(report) => return Ok((t, report, work)),
                Err(TransitionFailure::Requirements(need)) => {
                    t.active.extend(need.active);
                    t.loaded.extend(need.loaded);
                    t.loaded.extend(t.active.iter().copied());
                }
                Err(TransitionFailure::Error(error)) => return Err(error),
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
        self.try_region_transition(t, records)
            .map_err(|failure| match failure {
                TransitionFailure::Error(error) => error,
                TransitionFailure::Requirements(need) => {
                    if let Some(region) = need.active.difference(&t.active).next() {
                        TransitionError::MustBeActive(*region)
                    } else {
                        TransitionError::MustBeLoaded(
                            *need
                                .loaded
                                .difference(&t.loaded)
                                .next()
                                .expect("unsatisfied pin"),
                        )
                    }
                }
            })
    }

    fn try_region_transition(
        &mut self,
        t: &RegionTransition,
        records: &mut dyn RecordStore,
    ) -> Result<TransitionReport, TransitionFailure> {
        if let Some(region) = t.active.difference(&t.loaded).next() {
            return Err(TransitionError::ActiveNotLoaded(*region).into());
        }
        let mut next = self.clone();
        // Regions the game doesn't know yet are declared from the store's
        // source, if it has them.
        for region in t.loaded.iter().chain(&t.active) {
            if !next.world.knows_region(*region) {
                next.declare_unbuilt(*region, records)?;
            }
        }
        let mut report = TransitionReport::default();
        let attach: Vec<_> = t
            .loaded
            .iter()
            .filter(|r| {
                next.lifecycle.detached.contains_key(r) || next.lifecycle.unbuilt.contains(r)
            })
            .copied()
            .collect();
        let mut arrived = Vec::new();
        for region in attach {
            if next.lifecycle.unbuilt.contains(&region) {
                report.built.push(region);
            }
            arrived.push((region, next.attach_record(region, records)?));
            report.attached.push(region);
        }
        next.check_arrivals(&arrived)?;
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
        if !need.active.is_subset(&t.active) || !need.loaded.is_subset(&t.loaded) {
            return Err(TransitionFailure::Requirements(need));
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

    /// The region an actor is in, loaded or not.
    pub fn known_actor_region(&self, id: ActorId) -> Option<RegionId> {
        self.actor_region(id)
    }

    /// Every actor the game knows: loaded, detached or not built yet, in
    /// identity order.
    pub fn known_actor_ids(&self) -> BTreeSet<ActorId> {
        self.actors
            .keys()
            .chain(self.lifecycle.directory.actors.keys())
            .copied()
            .collect()
    }

    /// Every record identity this game will make is at least this one.
    /// Rewinds continue identities ([`Game::continue_record_ids`]), so along
    /// one engine's history records made later have larger identities.
    pub fn next_record_id(&self) -> RecordId {
        RecordId(self.lifecycle.next_record)
    }

    /// The regions active and loaded now.
    fn current_regions(&self) -> RegionTransition {
        let loaded: BTreeSet<_> = self.world.loaded_regions().collect();
        RegionTransition {
            active: loaded
                .iter()
                .filter(|r| !self.lifecycle.frozen.contains(r))
                .copied()
                .collect(),
            loaded,
        }
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
        self.lifecycle.directory.insert(id, identities);
        self.lifecycle.unbuilt.insert(id);
        (self.next_actor_id, self.next_item_id, self.next_door_id) =
            (next_actor, next_item, next_door);
        Ok(())
    }

    /// Atomically publish newly generated regions as ordinary detached records.
    /// Allocation follows region order, independent of preparation order. No
    /// member is attached or activated by registration. Store writes happen only
    /// after the entire batch has passed validation.
    pub fn register_generated_regions(
        &mut self,
        mut batch: Vec<(UnbuiltRegion, RegionRecord)>,
        records: &mut dyn RecordStore,
    ) -> Result<(), TransitionError> {
        batch.sort_by_key(|(definition, _)| definition.region.id);
        let mut next = self.clone();
        let mut made = Vec::with_capacity(batch.len());
        for (definition, record) in batch {
            let region = definition.region.id;
            let invalid = TransitionError::InvalidRecord(region);
            if !record.valid_contents(region, next.tick)
                || record.stamps.values().any(|stamp| *stamp != 0)
                || record.identities() != definition.identities
            {
                return Err(invalid);
            }
            if !next.world.knows_region(region) {
                next.add_unbuilt_region(
                    definition.region,
                    definition.chamber,
                    definition.identities,
                )
                .map_err(|_| invalid)?;
            }
            if !next.lifecycle.unbuilt.remove(&region)
                || next.world.known_region(region) != Some(record.world.region())
                || !record.matches_directory(region, &next.lifecycle.directory)
            {
                return Err(invalid);
            }
            let id = RecordId(next.lifecycle.next_record);
            next.world
                .publish_region_anchors(&record.world)
                .map_err(|_| invalid)?;
            next.lifecycle.next_record = id.0.checked_add(1).ok_or(invalid)?;
            next.lifecycle.detached.insert(region, id);
            made.push((id, Shared::new(record)));
        }
        if !next.lifecycle_state_valid() {
            return Err(TransitionError::InvalidRecord(RegionId(0)));
        }
        *self = next;
        for (id, record) in made {
            records.put(id, record);
        }
        Ok(())
    }

    /// Declare a region the store's source can build.
    fn declare_unbuilt(
        &mut self,
        region: RegionId,
        records: &mut dyn RecordStore,
    ) -> Result<(), TransitionError> {
        let unbuilt = records
            .unbuilt(region)
            .filter(|u| u.region.id == region)
            .ok_or(TransitionError::UnknownRegion(region))?;
        self.add_unbuilt_region(unbuilt.region, unbuilt.chamber, unbuilt.identities)
            .map_err(|_| TransitionError::InvalidRecord(region))
    }

    /// Allocate new actor, item and door identities from at least these, so
    /// they never collide with identities a region source authored for
    /// regions the game hasn't declared yet.
    pub fn reserve_identities(&mut self, actors: u64, items: u64, doors: u64) {
        self.next_actor_id = self.next_actor_id.max(actors);
        self.next_item_id = self.next_item_id.max(items);
        self.next_door_id = self.next_door_id.max(doors);
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
        self.lifecycle.directory.insert(
            region,
            RegionIdentities {
                actors: actors.iter().copied().collect(),
                items: items.iter().copied().collect(),
                doors: world.door_ids().collect(),
            },
        );
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

    /// Give what attached regions brought the checks restoring gives loaded
    /// state, now that everything that attaches together is in place. Runs
    /// before anything computes bodies or pins, so a bad record can't upset
    /// them.
    fn check_arrivals(
        &self,
        arrived: &[(RegionId, RegionIdentities)],
    ) -> Result<(), TransitionError> {
        if arrived.is_empty() {
            return Ok(());
        }
        for (region, identities) in arrived {
            let invalid = Err(TransitionError::InvalidRecord(*region));
            let actors_valid = identities.actors.iter().all(|id| {
                self.actors
                    .get(id)
                    .is_some_and(|actor| self.actor_state_valid(*id, actor))
                    && self
                        .navigation
                        .get(id)
                        .is_none_or(|n| n.checkpoint_valid(&self.world))
            });
            let items_valid = identities.items.iter().all(|id| {
                self.items
                    .get(id)
                    .is_some_and(|item| self.item_state_valid(*id, item))
            });
            if !actors_valid || !items_valid {
                return invalid;
            }
        }
        // Bodies may not overlap anyone's, loaded before or arriving now.
        let mut occupied = BTreeSet::new();
        let first = arrived[0].0;
        if !self
            .actors
            .values()
            .all(|actor| self.actor_state_valid_body(actor, &mut occupied))
            || !self.combat_valid()
            || !self.physics_valid()
        {
            return Err(TransitionError::InvalidRecord(first));
        }
        Ok(())
    }

    fn actor_state_valid_body(&self, actor: &Actor, occupied: &mut BTreeSet<Location>) -> bool {
        actor.orientation < 24 && actor.body.valid() && self.body_has_room(actor, occupied)
    }

    fn attach_record(
        &mut self,
        region: RegionId,
        records: &mut dyn RecordStore,
    ) -> Result<RegionIdentities, TransitionError> {
        let invalid = TransitionError::InvalidRecord(region);
        let unavailable = TransitionError::RecordUnavailable(region);
        let record = if self.lifecycle.unbuilt.remove(&region) {
            let record = records.build(region).ok_or(unavailable)?;
            Shared::new(record)
        } else {
            let id = self.lifecycle.detached.remove(&region).ok_or(invalid)?;
            records.get(id).ok_or(unavailable)?
        };
        // Newly generated detached records can also link to as-yet-unknown
        // regions. Declaration does not build or activate their destinations.
        for target in record.world.linked_regions() {
            if target != region && !self.world.knows_region(target) {
                self.declare_unbuilt(target, records).map_err(|_| invalid)?;
            }
        }
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
        let identities = self.lifecycle.directory.remove_region(region);
        self.lifecycle.frozen.insert(region);
        Ok(identities)
    }

    // ----- Validation -----

    /// Whether an identity refers to something detached.
    pub(crate) fn detached_actor(&self, id: ActorId) -> bool {
        self.lifecycle.directory.actors.contains_key(&id)
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
            expected.insert(*region, record.identities());
            record.valid_contents(*region, self.tick)
        }) && {
            // Unbuilt regions' identities are declared, not recorded.
            let mut directory = (*l.directory).clone();
            for region in l.unbuilt.iter() {
                directory.remove_region(*region);
            }
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
                .chain(l.unbuilt.iter())
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
    fn attaching_an_observer_batches_all_newly_discovered_pins() {
        struct Reads {
            records: MemoryRecords,
            counts: BTreeMap<RecordId, usize>,
        }
        impl RecordStore for Reads {
            fn put(&mut self, id: RecordId, record: Shared<RegionRecord>) {
                self.records.put(id, record);
            }
            fn get(&mut self, id: RecordId) -> Option<Shared<RegionRecord>> {
                *self.counts.entry(id).or_default() += 1;
                self.records.get(id)
            }
        }
        let mut game = Game::region_corridor(1, 7);
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(4),
                    position: Position { x: 6, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut store = Reads {
            records: MemoryRecords::default(),
            counts: BTreeMap::new(),
        };
        game.transition_regions(&sets(&[], &[]), &mut store)
            .unwrap();
        let observer_record = game
            .detached_records()
            .find(|(r, _)| *r == RegionId(4))
            .unwrap()
            .1;
        game.add_reference_point(ReferencePoint {
            target: ReferenceTarget::Actor(actor),
            active_radius: Some(0),
            load_radius: Some(0),
            observes: true,
        })
        .unwrap();
        let (sets, _) = game
            .transition_regions(&sets(&[4], &[4]), &mut store)
            .unwrap();
        assert_eq!(
            sets.active,
            BTreeSet::from([RegionId(3), RegionId(4), RegionId(5)])
        );
        assert_eq!(
            sets.loaded,
            BTreeSet::from([
                RegionId(2),
                RegionId(3),
                RegionId(4),
                RegionId(5),
                RegionId(6)
            ])
        );
        assert!(
            store.counts[&observer_record] <= 3,
            "observer reread {} times",
            store.counts[&observer_record]
        );
        assert!(game.lifecycle_state_valid());
    }

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
        crate::test_creatures::configure(
            &mut game,
            watcher,
            "neutral",
            crate::test_creatures::species(),
        );
        assert!(
            game.creature(watcher).is_some(),
            "frozen subjects own their builds"
        );
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
            .spawn_actor(at(4, 5), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_run(player, BTreeSet::from([player]), None, BTreeMap::new())
            .unwrap();
        game.add_default_reference_points().unwrap();
        let mut records = MemoryRecords::default();
        game.transition_regions(&sets(&[1], &[1]), &mut records)
            .unwrap();
        assert_eq!(game.region_state(RegionId(4)), Some(RegionState::Detached));
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
        let four = RegionId(4);
        let mut damaged = MemoryRecords::default();
        for (region, id) in game.detached_records() {
            let mut record = (*records.get(id).unwrap()).clone();
            if region == four {
                assert!(record.actors.remove(&other).is_some());
                record.stamps.remove(&other);
                record.ai.remove(&other);
                record.navigation.remove(&other);
                record.displaced.remove(&other);
                assert!(record.valid_contents(four, game.tick()));
                assert!(!record.matches_directory(four, &game.lifecycle.directory));
            }
            damaged.put(id, Shared::new(record));
        }
        assert!(!game.detached_records_valid(&mut damaged));

        let before = game.clone();
        let error = game
            .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut damaged)
            .unwrap_err();
        assert_eq!(error, TransitionError::InvalidRecord(four));
        assert_eq!(game, before);
    }

    #[test]
    fn a_missing_record_fails_the_transition_without_changes() {
        let (mut game, _, mut records) = detached_corridor();
        let (_, lost) = game
            .detached_records()
            .find(|(region, _)| *region == RegionId(4))
            .unwrap();
        records.retain(|id| id != lost);
        let before = game.clone();
        let error = game
            .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records)
            .unwrap_err();
        assert_eq!(error, TransitionError::RecordUnavailable(RegionId(4)));
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
        fn unbuilt(&mut self, region: RegionId) -> Option<UnbuiltRegion> {
            Some(UnbuiltRegion {
                region: self.template.world.region(region)?.clone(),
                chamber: false,
                identities: self
                    .template
                    .clone()
                    .into_region_record(region)
                    .ok()?
                    .identities(),
            })
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
    fn generated_batch_commits_detached_and_streams_independently() {
        let (template, mut game, _) = unbuilt_corridor();
        let mut source = TemplateSource {
            template,
            records: MemoryRecords::default(),
            lose: None,
        };
        let batch = (1..=4)
            .rev()
            .map(|id| {
                let region = RegionId(id);
                (
                    source.unbuilt(region).unwrap(),
                    source.build(region).unwrap(),
                )
            })
            .collect();
        game.register_generated_regions(batch, &mut source).unwrap();
        assert!(game.loaded_regions().next().is_none());
        assert_eq!(game.detached_records().count(), 4);
        assert_eq!(game.detached_record(RegionId(1)), Some(RecordId(1)));
        game.transition_regions(&sets(&[1], &[1]), &mut source)
            .unwrap();
        // Existing body/perception pins can additionally keep the adjoining
        // region; registration must not pin the complete batch.
        assert!(game.loaded_regions().count() < 4);
        assert!(game.loaded_regions().any(|id| id == RegionId(1)));
        assert_eq!(game.region_state(RegionId(4)), Some(RegionState::Detached));
        let before = game.clone();
        let duplicate = vec![(
            source.unbuilt(RegionId(1)).unwrap(),
            source.build(RegionId(1)).unwrap(),
        )];
        assert!(game
            .register_generated_regions(duplicate, &mut source)
            .is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn invalid_generated_batch_does_not_publish_any_member() {
        let (template, mut game, _) = unbuilt_corridor();
        let mut source = TemplateSource {
            template,
            records: MemoryRecords::default(),
            lose: None,
        };
        let mut batch = (1..=4)
            .map(|id| {
                let region = RegionId(id);
                (
                    source.unbuilt(region).unwrap(),
                    source.build(region).unwrap(),
                )
            })
            .collect::<Vec<_>>();
        batch[3].1.actors.clear();
        batch[3].1.stamps.clear();
        // Region 3 owns the actor; removing it invalidates that member.
        batch[2].1.actors.clear();
        batch[2].1.stamps.clear();
        let before = game.clone();
        assert!(game.register_generated_regions(batch, &mut source).is_err());
        assert_eq!(game, before);
        assert!(source.records.ids().next().is_none());
    }

    /// A game that knows no regions declares each from the source only when
    /// a transition loads it or a built region links to it, so game state
    /// grows with the regions played, not the source.
    #[test]
    fn transitions_declare_regions_from_their_source_only_as_needed() {
        let (template, _, _) = unbuilt_corridor();
        let mut game = Game::new(tor_world::World::new(vec![], vec![]).unwrap(), 1);
        game.reserve_identities(
            template.next_actor_id,
            template.next_item_id,
            template.next_door_id,
        );
        let before = game.clone();
        let error = game
            .clone()
            .transition_regions(&sets(&[1], &[1]), &mut MemoryRecords::default())
            .unwrap_err();
        assert_eq!(error, TransitionError::UnknownRegion(RegionId(1)));
        assert_eq!(game, before);

        let mut source = TemplateSource {
            template: template.clone(),
            records: MemoryRecords::default(),
            lose: None,
        };
        game.transition_regions(&sets(&[1], &[1]), &mut source)
            .unwrap();
        // Region 1 is active; pins keep region 2, which it links to, loaded;
        // region 2's build declared region 3; region 4 isn't known at all.
        assert_eq!(game.region_state(RegionId(1)), Some(RegionState::Active));
        assert_eq!(game.region_state(RegionId(2)), Some(RegionState::Frozen));
        assert_eq!(game.region_state(RegionId(3)), Some(RegionState::Unbuilt));
        assert_eq!(game.region_state(RegionId(4)), None);
        assert!(game.lifecycle_state_valid());
        game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut source)
            .unwrap();
        assert_eq!(game, template);
    }

    #[test]
    fn reserved_identities_are_never_allocated() {
        let mut game = Game::region_corridor(1, 2);
        game.reserve_identities(50, 60, 70);
        let at = Location {
            region: RegionId(1),
            position: Position { x: 2, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        assert_eq!(actor, ActorId(50));
        assert_eq!((game.next_item_id, game.next_door_id), (60, 70));
        // Reserving less never moves allocation back.
        game.reserve_identities(1, 1, 1);
        assert_eq!(game.next_actor_id, 51);
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

    /// The world's reach answers (exit field, then cached search) must never
    /// disagree with an uncached search, from any cell. The
    /// world has a tall chamber with a stair in its middle, joins across two of
    /// its faces to other chambers (rim projection), and walls
    /// near the faces.
    #[test]
    fn reach_answers_agree_with_the_search_everywhere() {
        use tor_world::{Extent, Passage, World};
        let at = |region, x, y, z| Location {
            region: RegionId(region),
            position: Position { x, y, z },
        };
        let mut game = Game::new(World::new(vec![], vec![]).unwrap(), 1);
        for (id, [x, y, z]) in [
            (1, [12, 12, 8]),
            (2, [9, 9, 3]),
            (3, [7, 7, 3]),
            (4, [5, 5, 2]),
        ] {
            game.add_chamber(Region {
                id: RegionId(id),
                name: String::new(),
                bounds: Extent::new(x, y, z).unwrap(),
            })
            .unwrap();
        }
        let join = |game: &mut Game, from, direction, to, turns, width, height| {
            game.connect_area(
                Passage {
                    from,
                    direction,
                    to,
                },
                turns,
                width,
                height,
            )
            .unwrap_or_else(|e| panic!("join {from:?} {direction:?}: {e:?}"));
        };
        join(
            &mut game,
            at(1, 11, 3, 0),
            Direction::East,
            at(2, 0, 3, 0),
            0,
            3,
            2,
        );
        join(
            &mut game,
            at(2, 0, 3, 0),
            Direction::West,
            at(1, 11, 3, 0),
            0,
            3,
            2,
        );
        join(
            &mut game,
            at(1, 4, 0, 0),
            Direction::North,
            at(3, 2, 6, 0),
            0,
            2,
            2,
        );
        join(
            &mut game,
            at(3, 2, 6, 0),
            Direction::South,
            at(1, 4, 0, 0),
            0,
            2,
            2,
        );
        for (from, direction, to) in [
            (at(1, 6, 6, 0), Direction::Up, at(4, 2, 2, 0)),
            (at(4, 2, 2, 0), Direction::Down, at(1, 6, 6, 0)),
        ] {
            game.connect(
                Passage {
                    from,
                    direction,
                    to,
                },
                0,
            )
            .unwrap();
        }
        for wall in [
            at(1, 10, 1, 0),
            at(1, 11, 7, 1),
            at(1, 1, 10, 0),
            at(2, 1, 6, 0),
            at(3, 5, 1, 1),
        ] {
            let _ = game.set_wall(wall, true);
        }
        let fielded = game
            .check_reach()
            .unwrap_or_else(|cell| panic!("disagreement at {cell:?}"));
        let cells: i32 = (1..=4)
            .map(|id| {
                let (x, y, z) = game.world.region(RegionId(id)).unwrap().bounds.dimensions();
                x * y * z
            })
            .sum();
        // Both kinds of cell occur: answered by the field, and searched.
        assert!(
            fielded > 50 && (cells as usize) - fielded > 50,
            "{fielded} of {cells}"
        );
    }

    /// Every command clones the game, so lifecycle state that grows with the
    /// world (unbuilt and detached regions, their identities) must be
    /// shared by the clone, not copied.
    #[test]
    fn cloning_a_game_shares_state_that_grows_with_the_world() {
        let (_, game, _) = unbuilt_corridor();
        let clone = game.clone();
        let (l, c) = (&game.lifecycle, &clone.lifecycle);
        assert!(l.unbuilt.shares_storage(&c.unbuilt));
        assert!(l.detached.shares_storage(&c.detached));
        assert!(l.directory.shares_storage(&c.directory));
    }

    /// The directory's region index, used when a region attaches, agrees with
    /// its maps through insertion, removal and a save round trip.
    #[test]
    fn the_directory_region_index_stays_consistent() {
        let identities = |actor, item, door| RegionIdentities {
            actors: BTreeSet::from([ActorId(actor)]),
            items: BTreeSet::from([ItemId(item)]),
            doors: BTreeSet::from([door]),
        };
        let mut directory = Directory::default();
        directory.insert(RegionId(1), identities(2, 3, 4));
        directory.insert(RegionId(5), identities(6, 7, 8));
        let saved = serde_json::to_string(&directory).unwrap();
        assert!(!saved.contains("by_region"), "the index is never saved");
        let restored: Directory = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored, directory);
        assert_eq!(directory.remove_region(RegionId(1)), identities(2, 3, 4));
        assert_eq!(directory.actors.keys().collect::<Vec<_>>(), [&ActorId(6)]);
        assert_eq!(
            directory.remove_region(RegionId(1)),
            RegionIdentities::default()
        );
        let mut expected = Directory::default();
        expected.insert(RegionId(5), identities(6, 7, 8));
        assert_eq!(directory, expected);
    }

    /// A record from storage gets the checks restoring a checkpoint gives
    /// loaded actors and items, not only its own consistency: one with an
    /// impossible actor (a 30th orientation) is rejected, changing nothing.
    #[test]
    fn attach_checks_a_record_as_thoroughly_as_restore() {
        let (mut game, other, mut records) = detached_corridor();
        let three = RegionId(4);
        let mut damaged = MemoryRecords::default();
        for (region, id) in game.detached_records() {
            let mut record = (*records.get(id).unwrap()).clone();
            if region == three {
                record.actors.get_mut(&other).unwrap().orientation = 30;
                assert!(record.valid_contents(three, game.tick()));
                assert!(record.matches_directory(three, &game.lifecycle.directory));
            }
            damaged.put(id, Shared::new(record));
        }
        let before = game.clone();
        let error = game
            .transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut damaged)
            .unwrap_err();
        assert_eq!(error, TransitionError::InvalidRecord(three));
        assert_eq!(game, before);
    }
    #[test]
    fn observing_points_activate_the_same_cells_as_merged_actor_perception() {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        for id in 1..=3 {
            world
                .add_chamber(Region {
                    id: RegionId(id),
                    name: "shaft".into(),
                    bounds: tor_world::Extent::new(3, 3, 2).unwrap(),
                })
                .unwrap();
        }
        let at = |region, z| Location {
            region: RegionId(region),
            position: Position { x: 1, y: 1, z },
        };
        for (from, direction, to) in [
            (at(1, 1), Direction::Up, at(2, 0)),
            (at(2, 0), Direction::Down, at(3, 1)),
        ] {
            world
                .connect_portal_area(
                    tor_world::Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                    1,
                    1,
                )
                .unwrap();
        }
        world.set_region_light(RegionId(3), false).unwrap();
        let mut game = Game::new(world, 42);
        let actor = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
        game.add_reference_point(ReferencePoint {
            target: ReferenceTarget::Actor(actor),
            active_radius: Some(0),
            load_radius: Some(0),
            observes: true,
        })
        .unwrap();
        let perceived: BTreeSet<_> = game
            .scene(actor)
            .unwrap()
            .iter()
            .map(|cell| cell.location.region)
            .collect();
        assert_eq!(perceived, BTreeSet::from([RegionId(1), RegionId(2)]));
        let need = game.point_requirements();
        assert_eq!(need.active, perceived);
        assert!(
            need.loaded.contains(&RegionId(3)),
            "dark geometry still needs to be read"
        );
    }
}
