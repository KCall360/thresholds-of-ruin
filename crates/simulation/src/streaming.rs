//! Region lifecycle for streaming. Reference points decide which regions must
//! stay active and loaded; other regions freeze, and a frozen region can be
//! detached into a self-contained record. Transitions settle at action boundaries
//! or an explicit private host continuation, using game state alone for replay. See
//! `docs/architecture/streaming.md#region-lifecycle-contract`.
use crate::actor_record::ActorRecord;
use crate::ai::Ai;
use crate::creatures::Content;
use crate::explored_map::ExploredMap;
use crate::observation::SIGHT_RANGE;
use crate::travel::Navigation;
use crate::{Actor, ActorId, Game, GameError, Impact, Item, ItemId, ItemLocation, PhysicsEntity};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
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
    pub hold_owners: usize,
    pub hold_references: usize,
    pub hold_regions: usize,
}

impl TransitionReport {
    /// Whether the transition changed nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionError {
    Rule(GameError),
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

impl From<GameError> for TransitionFailure {
    fn from(error: GameError) -> Self {
        Self::Error(TransitionError::Rule(error))
    }
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
    /// Pure immutable source policy for optional preparation of unresolved
    /// named-stair destinations. It never relaxes physical, body, resolved-link
    /// or explicit requested-region pins. Existing stores allow preparation.
    fn can_prepare(&self, region: RegionId) -> bool {
        let _ = region;
        true
    }
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
    pub traps: BTreeSet<u64>,
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
    traps: BTreeMap<u64, RegionId>,
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
    traps: BTreeMap<u64, RegionId>,
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
        for (id, region) in &data.traps {
            by_region.entry(*region).or_default().traps.insert(*id);
        }
        Self {
            actors: data.actors,
            items: data.items,
            doors: data.doors,
            traps: data.traps,
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
            traps: directory.traps,
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
        self.traps
            .extend(identities.traps.iter().map(|id| (*id, region)));
        let listed = self.by_region.entry(region).or_default();
        listed.actors.extend(identities.actors);
        listed.items.extend(identities.items);
        listed.doors.extend(identities.doors);
        listed.traps.extend(identities.traps);
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
        for id in &identities.traps {
            self.traps.remove(id);
        }
        identities
    }
}

/// Everything located in one detached region: its world slice, the actors
/// anchored there (with their AI, navigation and complete explored map), its
/// ground items, and the items those actors carry. Stored actors refer to the save’s shared catalog.
/// Decoding resolves these references explicitly before the game attaches it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegionRecord {
    world: RegionSlice,
    actors: BTreeMap<ActorId, Actor>,
    stamps: BTreeMap<ActorId, u64>,
    ai: BTreeMap<ActorId, Ai>,
    navigation: BTreeMap<ActorId, Navigation>,
    explored_maps: BTreeMap<ActorId, Shared<ExploredMap>>,
    chart_history: Shared<BTreeMap<ActorId, crate::chart_history::Owner>>,
    items: BTreeMap<ItemId, Item>,
    displaced: BTreeSet<ActorId>,
    impacts: Vec<Impact>,
}

/// Independent stored region shape. Creature definitions are never repeated here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionRecordData {
    world: RegionSlice,
    actors: BTreeMap<ActorId, ActorRecord>,
    stamps: BTreeMap<ActorId, u64>,
    ai: BTreeMap<ActorId, Ai>,
    navigation: BTreeMap<ActorId, Navigation>,
    #[serde(with = "crate::checkpoint::explored_actor_maps")]
    explored_maps: BTreeMap<ActorId, Shared<ExploredMap>>,
    #[serde(with = "crate::chart_history::local_owners")]
    chart_history: Shared<BTreeMap<ActorId, crate::chart_history::Owner>>,
    items: BTreeMap<ItemId, Item>,
    displaced: BTreeSet<ActorId>,
    impacts: Vec<Impact>,
}
impl RegionRecord {
    pub fn historical_chart_authorities(
        &self,
    ) -> impl Iterator<
        Item = (
            &Shared<tor_world::World>,
            &Shared<crate::spatial_scope::registry::Registry>,
            &str,
        ),
    > {
        crate::chart_history::authority_sources(&self.chart_history)
    }
    pub fn historical_chart_sources(
        &self,
    ) -> impl Iterator<
        Item = (
            &tor_world::World,
            &crate::spatial_scope::registry::Registry,
            &str,
        ),
    > {
        crate::chart_history::sources(&self.chart_history)
    }
}
impl RegionRecordData {
    pub fn capture(record: &RegionRecord) -> Self {
        Self {
            world: record.world.clone(),
            actors: record
                .actors
                .iter()
                .map(|(id, actor)| (*id, ActorRecord::capture(actor)))
                .collect(),
            stamps: record.stamps.clone(),
            ai: record.ai.clone(),
            navigation: record.navigation.clone(),
            explored_maps: record.explored_maps.clone(),
            chart_history: record.chart_history.clone(),
            items: record.items.clone(),
            displaced: record.displaced.clone(),
            impacts: record.impacts.clone(),
        }
    }
    pub fn restore(
        self,
        content: Arc<Content>,
    ) -> Result<RegionRecord, crate::checkpoint::PersistError> {
        let mut maps = crate::checkpoint::ExploredOwnerValidation::new(self.explored_maps.values())
            .ok_or(crate::checkpoint::PersistError::ExploredMap)?;
        if self
            .explored_maps
            .iter()
            .any(|(id, map)| !self.actors.contains_key(id) || !maps.structure(map))
        {
            return Err(crate::checkpoint::PersistError::ExploredMap);
        }
        if self
            .world
            .traps()
            .any(|trap| !crate::traps::trap_content_valid(&content, &trap.kind))
        {
            return Err(crate::checkpoint::PersistError::World);
        }
        for (id, item) in &self.items {
            item.spec.validate_rules(content.rules()).map_err(|error| {
                crate::checkpoint::PersistError::ItemDefinition { item: *id, error }
            })?;
        }
        if !crate::chart_history::owners_valid(&self.chart_history, u64::MAX, |id| {
            self.actors.contains_key(&id)
        }) {
            return Err(crate::checkpoint::PersistError::ChartHistory);
        }
        let record = RegionRecord {
            world: self.world,
            actors: self
                .actors
                .into_iter()
                .map(|(id, record)| {
                    record
                        .restore(id, content.clone(), &self.items)
                        .map(|actor| (id, actor))
                })
                .collect::<Result<_, _>>()?,
            stamps: self.stamps,
            ai: self.ai,
            navigation: self.navigation,
            explored_maps: self.explored_maps,
            chart_history: self.chart_history,
            items: self.items,
            displaced: self.displaced,
            impacts: self.impacts,
        };
        for (id, item) in &record.items {
            let equipped = match item.location {
                ItemLocation::Carried(owner) => record
                    .actors
                    .get(&owner)
                    .is_some_and(|actor| actor.equipment.values().any(|owned| owned == id)),
                ItemLocation::Ground(_) => false,
            };
            if !crate::periodic_relocation::item_clock_valid(item, equipped) {
                return Err(crate::checkpoint::PersistError::ItemState { item: *id });
            }
        }
        Ok(record)
    }
}
impl Serialize for RegionRecord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        RegionRecordData::capture(self).serialize(serializer)
    }
}

impl RegionRecord {
    /// Invariants of the record alone, as the record of `region` detached no
    /// later than `tick`.
    fn valid_contents(&self, region: RegionId, tick: u64) -> bool {
        self.valid_contents_with_map_trust(region, tick, false)
    }
    fn valid_contents_with_map_trust(
        &self,
        region: RegionId,
        tick: u64,
        trusted_maps: bool,
    ) -> bool {
        let mut maps = if trusted_maps {
            None
        } else {
            match crate::checkpoint::ExploredOwnerValidation::new(self.explored_maps.values()) {
                Some(validation) => Some(validation),
                None => return false,
            }
        };
        self.world.id() == region
            && self.world.valid()
            && self.actors.values().all(|a| {
                a.location.region == region
                    && a.body.valid()
                    && a.motion.valid()
                    && a.holds.valid_shape()
                    && a.periodic_relocations.valid_for(a, &self.items)
            })
            && self.stamps.keys().eq(self.actors.keys())
            && self.actors.iter().all(|(id, actor)| {
                self.stamps
                    .get(id)
                    .is_some_and(|stamp| actor.active_since <= *stamp)
                    && actor.motion.active_fall.is_none_or(|fall| {
                        actor.alive() && fall.owner.0 > 0 && fall.started_at <= tick
                    })
                    && actor.combat.as_ref().is_none_or(|combat| {
                        combat.creature().recovery_context_valid_at(
                            self.stamps.get(id).copied(),
                            actor.active_since,
                        )
                    })
            })
            && self.stamps.values().all(|stamp| *stamp <= tick)
            && self.ai.keys().all(|id| self.actors.contains_key(id))
            && self
                .navigation
                .keys()
                .all(|id| self.actors.contains_key(id))
            && self.explored_maps.iter().all(|(id, map)| {
                self.actors.contains_key(id)
                    && (trusted_maps || maps.as_mut().is_some_and(|maps| maps.structure(map)))
            })
            && crate::chart_history::owners_valid(&self.chart_history, tick, |id| {
                self.actors.contains_key(&id)
            })
            && self.displaced.iter().all(|id| self.actors.contains_key(id))
            && self.items.iter().all(|(id, item)| {
                let equipped = match item.location {
                    ItemLocation::Carried(owner) => self
                        .actors
                        .get(&owner)
                        .is_some_and(|actor| actor.equipment.values().any(|owned| owned == id)),
                    ItemLocation::Ground(_) => false,
                };
                item.holds_valid()
                    && crate::periodic_relocation::item_clock_valid(item, equipped)
                    && item.motion.active_fall.is_none()
                    && match item.location {
                        ItemLocation::Ground(at) => at.region == region,
                        ItemLocation::Carried(carrier) => self.actors.contains_key(&carrier),
                    }
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
            traps: self.world.trap_ids().collect(),
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
    #[serde(skip)]
    pub(crate) recovery_contexts: crate::recovery_context::RecoveryContexts,
    #[serde(skip)]
    pub(crate) discovery_contexts: crate::discovery::DiscoveryContexts,
    #[serde(skip)]
    pub(crate) awareness_contexts: crate::awareness::AwarenessContexts,
    /// Construction-only gate; never persisted into a published game.
    #[serde(skip)]
    pub(crate) initial_awareness_deferred: bool,
    pub(crate) discovery_started: bool,
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
            recovery_contexts: crate::recovery_context::RecoveryContexts::default(),
            discovery_contexts: crate::discovery::DiscoveryContexts::default(),
            awareness_contexts: Default::default(),
            initial_awareness_deferred: false,
            discovery_started: false,
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
    hold_owners: BTreeMap<ActorId, Vec<ActorId>>,
    hold_regions: BTreeMap<RegionId, Vec<ActorId>>,
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

    pub(crate) fn detached_item(&self, item: ItemId) -> bool {
        self.lifecycle.directory.items.contains_key(&item)
    }
    pub(crate) fn target_region(&self, target: ReferenceTarget) -> Option<RegionId> {
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

    pub(crate) fn knows_concealed_feature(&self, feature: tor_world::ConcealedFeatureId) -> bool {
        match feature {
            tor_world::ConcealedFeatureId::Door(id) => {
                self.world.door_location(id).is_some()
                    || self.lifecycle.directory.doors.contains_key(&id)
            }
            tor_world::ConcealedFeatureId::Trap(id) => {
                self.world.trap_location(id).is_some()
                    || self.lifecycle.directory.traps.contains_key(&id)
            }
        }
    }

    pub(crate) fn actor_frozen(&self, id: ActorId) -> bool {
        !self.lifecycle.stamps.is_empty() && self.lifecycle.stamps.contains_key(&id)
    }

    /// The tick an actor's time stands at: when it froze, or now.
    pub(crate) fn actor_clock(&self, id: ActorId) -> u64 {
        self.lifecycle.stamps.get(&id).copied().unwrap_or(self.tick)
    }

    /// Reconstruct derived timer activity after restoring raw actor records.
    pub(crate) fn rebuild_timer_activity(&mut self) {
        for id in self.lifecycle.stamps.keys() {
            self.actors.set_timer_active(*id, false);
        }
    }

    /// Freeze or thaw one actor to match its region, after it moved.
    pub(crate) fn sync_actor_lifecycle(&mut self, id: ActorId) -> Result<(), GameError> {
        if self.lifecycle.frozen.is_empty() && self.lifecycle.stamps.is_empty() {
            return Ok(());
        }
        let Some(region) = self.actors.get(&id).map(|a| a.location.region) else {
            return Ok(());
        };
        match (
            self.lifecycle.frozen.contains(&region),
            self.lifecycle.stamps.contains_key(&id),
        ) {
            (true, false) => {
                self.settle_actor_time(id)?;
                self.lifecycle.stamps.insert(id, self.tick);
                self.actors.set_timer_active(id, false);
            }
            (false, true) => self.thaw_actor(id)?,
            _ => {}
        }
        Ok(())
    }

    fn actors_in(&self, region: RegionId) -> Vec<ActorId> {
        self.actors.in_region(region).collect()
    }

    fn freeze_region(&mut self, region: RegionId) -> Result<(), GameError> {
        for id in self.actors_in(region) {
            self.settle_actor_time(id)?;
            self.lifecycle.stamps.entry(id).or_insert(self.tick);
            self.actors.set_timer_active(id, false);
        }
        self.lifecycle.frozen.insert(region);
        Ok(())
    }

    fn thaw_region(&mut self, region: RegionId) -> Result<(), GameError> {
        self.lifecycle.frozen.remove(&region);
        for id in self.actors_in(region) {
            self.thaw_actor(id)?;
        }
        Ok(())
    }

    /// Shift every absolute tick an actor holds by the time it was frozen.
    fn thaw_actor(&mut self, id: ActorId) -> Result<(), GameError> {
        self.settle_actor_time(id)?;
        self.resolve_frozen_recovery_context(id)?;
        if let Some(horizon) = self.actors.get(&id).and_then(|actor| actor.holds.horizon()) {
            self.tick
                .checked_add(horizon)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        }
        if let Some(horizon) = self
            .actors
            .get(&id)
            .and_then(|actor| actor.combat.as_ref())
            .and_then(|combat| combat.creature().condition_horizon())
        {
            self.tick
                .checked_add(horizon)
                .ok_or(GameError::InvalidLocation)?;
        }
        let Some(stamp) = self.lifecycle.stamps.remove(&id) else {
            return Ok(());
        };
        let delta = self
            .tick
            .checked_sub(stamp)
            .ok_or(GameError::InvalidLocation)?;
        if delta == 0 {
            self.actors.set_timer_active(id, true);
            self.retire_settled_fall(id);
            return Ok(());
        }
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        let ready_at = actor
            .ready_at
            .checked_add(delta)
            .ok_or(GameError::InvalidLocation)?;
        if let Some(pending) = actor.pending.as_ref() {
            self.tick
                .checked_add(pending.completion_delay(stamp)?)
                .ok_or(GameError::InvalidLocation)?;
        }
        let mut behavior = actor.behavior.clone();
        if let Some(engagement) = behavior.engagement.as_mut() {
            engagement.last_seen = engagement
                .last_seen
                .checked_add(delta)
                .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
        }
        behavior.validate_deadline(actor.behavior_policy())?;
        actor.active_since = self.tick;
        actor.ready_at = ready_at;
        if let Some(pending) = actor.pending.as_mut() {
            pending.started = pending
                .started
                .checked_add(delta)
                .ok_or(GameError::InvalidLocation)?;
        }
        actor.behavior = behavior;
        actor.awareness.shift(delta)?;
        drop(actor);
        self.actors.set_timer_active(id, true);
        self.retire_settled_fall(id);
        Ok(())
    }

    // ----- Pins -----

    /// Complete known pins for the caller's ordinary proposed sets. Detached
    /// regions may reveal further requirements when attached. Retain the
    /// original proposed sets for applying or replanning a transition: the
    /// result also contains supplemental load-only dependencies.
    pub fn region_requirements(
        &self,
        active: &BTreeSet<RegionId>,
        loaded: &BTreeSet<RegionId>,
    ) -> RegionTransition {
        self.settle_region_transition(&RegionTransition {
            active: active.clone(),
            loaded: loaded.clone(),
        })
    }

    /// Ordinary body, ownership, activity and view pins, without expanding
    /// floor families. Each actor's pins are cached for this game state even
    /// when both the ordinary and supplemental frontiers need them.
    fn requirements(
        &self,
        active: &BTreeSet<RegionId>,
        loaded: &BTreeSet<RegionId>,
        cache: &mut PinCache,
        can_prepare: &dyn Fn(RegionId) -> bool,
    ) -> RegionTransition {
        let mut need = cache
            .points
            .get_or_insert_with(|| self.point_requirements(can_prepare))
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
                    let view = self.perception_scene_regions(*id);
                    let reach = self.reach_regions(*id);
                    cache.reach.insert(*id, (reach, view));
                }
                let (reach, view) = &cache.reach[id];
                need.active.extend(reach.iter().copied());
                need.loaded.extend(view.iter().copied());
            }
        }
        // A pending physical fall needs its original XP owner available for
        // atomic credit. This is deliberately load-only: credit must not thaw
        // the owner's region, grant elapsed time, or become a horizon root.
        for id in self.actors.active_fall_owners() {
            let actor = &self.actors[&id];
            if active.contains(&actor.location.region) {
                if let Some(owner) = actor
                    .motion
                    .active_fall
                    .and_then(|fall| self.actor_region(fall.owner))
                {
                    need.loaded.insert(owner);
                }
            }
        }
        // Item-owned claims provide the reverse edge. It has the same active
        // strength as its source region, independent of endpoint attach order.
        for (_, item) in self.items.claimed() {
            let source_region = match item.location {
                ItemLocation::Ground(at) => at.region,
                ItemLocation::Carried(actor) => match self.actor_region(actor) {
                    Some(region) => region,
                    None => continue,
                },
            };
            let is_active = active.contains(&source_region);
            if !is_active && !loaded.contains(&source_region) {
                continue;
            }
            for owner in item
                .holds
                .iter()
                .filter_map(|hold| self.actor_region(hold.key.lineage.actor))
            {
                if is_active {
                    need.active.insert(owner);
                } else {
                    need.loaded.insert(owner);
                }
            }
        }
        // Anything in an active region can move one cell at a time into a
        // linked region, so linked regions stay loaded and it freezes there.
        let sources: Vec<_> = active.iter().chain(need.active.iter()).copied().collect();
        for region in sources {
            let linked = cache.linked.entry(region).or_insert_with(|| {
                let required = self.world.required_linked_regions(region);
                self.world
                    .linked_regions(region)
                    .into_iter()
                    .filter(|id| required.contains(id) || can_prepare(*id))
                    .collect()
            });
            need.loaded.extend(linked.iter().copied());
        }
        need.loaded.extend(need.active.iter().copied());
        need
    }

    /// Pins that don't depend on the proposed sets: reference points, what
    /// observing points see, and actors awaiting controller input.
    fn point_requirements(&self, can_prepare: &dyn Fn(RegionId) -> bool) -> RegionTransition {
        let mut need = RegionTransition::default();
        for point in self.lifecycle.points.values() {
            let Some(region) = self.target_region(point.target) else {
                continue;
            };
            need.active.insert(region);
            if point.observes {
                if let Some((eye, frame, stairs)) = self.observer_eye(point.target, can_prepare) {
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
                            self.illuminated_scene(eye, frame, SIGHT_RANGE),
                        )
                    };
                    need.active
                        .extend(scene.into_iter().map(|cell| cell.location.region));
                    need.active.extend(stairs.iter().copied());
                    if let Some(actor) = actor {
                        need.loaded.extend(self.perception_scene_regions(actor));
                    } else if let Some(regions) =
                        self.world
                            .illuminated_eye_scene_regions(eye, frame, SIGHT_RANGE)
                    {
                        need.loaded.extend(regions);
                    }
                }
            }
        }
        for (id, eligibility) in &self.combat.input_boundaries {
            if *eligibility == crate::combat::InputEligibility::Denied
                || (self.actors.contains_key(id) && !self.condition_allows_voluntary_input(*id))
            {
                continue;
            }
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
            .and_then(|p| p.plan.work().target());
        if let Some(target) = target {
            if self.actors.contains_key(&target) {
                live.extend(self.body_regions(target));
            } else if let Some(region) = self.actor_region(target) {
                live.insert(region);
            }
        }
        if let Some(pending) = &self.actors[&id].pending {
            if let Some(ray) = pending.plan.as_ray() {
                // Keep only the frozen launch cells resident, never a forecast path.
                live.extend(ray.launch_footprint.iter().map(|cell| cell.region));
            }
            if let Some(projectile) = pending.plan.as_projectile() {
                live.extend(projectile.launch_footprint.iter().map(|cell| cell.region));
            }
            if let Some(step) = pending.plan.as_ability().and_then(|plan| plan.step) {
                // Frozen/paused work still owns this exact destination. Its live
                // body footprint can cross more regions than the anchor alone.
                live.extend(self.placement_regions(
                    step.destination(),
                    step.context.destination_orientation(),
                    &self.actors[&id].body,
                ));
            }
            for reference in pending.item_payment.refs() {
                if let Some(region) = self.target_region(ReferenceTarget::Item(reference.item)) {
                    live.insert(region);
                }
            }
        }
        if let Some(combat) = &self.actors[&id].combat {
            for owner in combat
                .creature()
                .conditions()
                .instances()
                .filter_map(|entry| entry.reserved_by)
            {
                if self.actors.contains_key(&owner) {
                    live.extend(self.body_regions(owner));
                } else if let Some(region) = self.actor_region(owner) {
                    live.insert(region);
                }
            }
        }
        live
    }

    /// Where an observing point's view starts, with any stair landings it
    /// discloses.
    pub(crate) fn observer_eye(
        &self,
        target: ReferenceTarget,
        can_prepare: &dyn Fn(RegionId) -> bool,
    ) -> Option<(Location, u8, Vec<RegionId>)> {
        let actor_eye = |id: ActorId| {
            let (eye, frame) = self.eye(id)?;
            let at = self.actors[&id].location;
            let stairs = [Direction::Up, Direction::Down]
                .into_iter()
                .filter(|d| self.world.is_stair(at, *d))
                .filter_map(|direction| {
                    let region = self.world.stair_region(at, direction)?;
                    (self.world.link_destination(at, direction).is_some() || can_prepare(region))
                        .then_some(region)
                })
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
        self.placement_regions(actor.location, actor.orientation, &actor.body)
    }
    fn placement_regions(
        &self,
        location: Location,
        orientation: u8,
        body: &crate::BodySpec,
    ) -> BTreeSet<RegionId> {
        match self.body_cells(location, orientation, body) {
            Some(cells) => cells.iter().map(|(at, _)| at.region).collect(),
            None => {
                let mut regions = self.world.linked_regions(location.region);
                regions.insert(location.region);
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

    /// Grow an ordinary frontier until every known pin holds. Supplemental
    /// source pins remain load-only and never become floor-expansion roots.
    /// Replanning must start from the original frontier, not this result.
    pub fn settle_region_transition(&self, proposed: &RegionTransition) -> RegionTransition {
        self.settle(proposed, &mut PinCache::default(), &|_| true)
    }

    fn settle(
        &self,
        proposed: &RegionTransition,
        cache: &mut PinCache,
        can_prepare: &dyn Fn(RegionId) -> bool,
    ) -> RegionTransition {
        // Complete ordinary roots, including active hold/claim dependencies,
        // before taking the relocation-root snapshot. Supplemental frozen actors
        // cannot recursively promote their saved floor policies into roots.
        let ordinary = self.settle_pins(proposed, cache, can_prepare, true);
        let mut t = self.settle_dependencies(ordinary, cache, can_prepare);
        let ordinary_active = t.active.clone();
        let ordinary_loaded = t.loaded.clone();
        self.relocation_loaded_closure(&ordinary_active, &mut t.loaded);
        self.trap_loaded_closure(&ordinary_loaded, &mut t.loaded);
        t.loaded.extend(self.light_required_regions());
        if let Some(source) = self.retrieval_source_region() {
            t.loaded.insert(source);
        }
        self.settle_dependencies(t, cache, can_prepare)
    }

    fn settle_dependencies(
        &self,
        mut t: RegionTransition,
        cache: &mut PinCache,
        can_prepare: &dyn Fn(RegionId) -> bool,
    ) -> RegionTransition {
        loop {
            let before = (t.active.len(), t.loaded.len());
            let regions: Vec<_> = t.loaded.iter().copied().collect();
            for region in regions {
                let owners = cache
                    .hold_regions
                    .entry(region)
                    .or_insert_with(|| self.actors.held_owners_in(region).collect())
                    .clone();
                for owner in owners {
                    let peers = cache
                        .hold_owners
                        .entry(owner)
                        .or_insert_with(|| self.actors[&owner].holds.peers().collect());
                    for peer in peers.iter().filter_map(|peer| self.actor_region(*peer)) {
                        if t.active.contains(&region) {
                            t.active.insert(peer);
                        }
                        t.loaded.insert(peer);
                    }
                }
            }
            t = self.settle_pins(&t, cache, can_prepare, false);
            if before == (t.active.len(), t.loaded.len()) {
                return t;
            }
        }
    }

    fn settle_pins(
        &self,
        proposed: &RegionTransition,
        cache: &mut PinCache,
        can_prepare: &dyn Fn(RegionId) -> bool,
        complete_floors: bool,
    ) -> RegionTransition {
        let mut t = proposed.clone();
        t.loaded.extend(t.active.iter().copied());
        loop {
            let need = self.requirements(&t.active, &t.loaded, cache, can_prepare);
            let before = (t.active.len(), t.loaded.len());
            t.active.extend(need.active);
            t.loaded.extend(need.loaded);
            t.loaded.extend(t.active.iter().copied());
            if complete_floors {
                self.activation_loaded_closure(&mut t.loaded);
            }
            if (t.active.len(), t.loaded.len()) == before {
                return t;
            }
        }
    }

    /// Settle and apply an ordinary frontier, growing the sets when attaching
    /// a region reveals further pins. Returns the sets applied. Keep the input
    /// frontier for subsequent planning; the output includes supplemental pins.
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
        self.settle_counted_with(proposed, &|_| true)
    }

    /// The same pure planning query with the source's optional-preparation policy.
    pub fn settle_counted_from_source(
        &self,
        proposed: &RegionTransition,
        records: &dyn RecordStore,
    ) -> (RegionTransition, PinWork) {
        self.settle_counted_with(proposed, &|region| records.can_prepare(region))
    }

    fn settle_counted_with(
        &self,
        proposed: &RegionTransition,
        can_prepare: &dyn Fn(RegionId) -> bool,
    ) -> (RegionTransition, PinWork) {
        let mut cache = PinCache::default();
        let t = self.settle(proposed, &mut cache, can_prepare);
        let work = PinWork {
            actors: cache.live.len(),
            reaches: cache.reach.len(),
            hold_owners: cache.hold_owners.len(),
            hold_references: cache.hold_owners.values().map(Vec::len).sum(),
            hold_regions: cache
                .hold_regions
                .values()
                .filter(|owners| !owners.is_empty())
                .count(),
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
        self.transition_regions_counted_inner(proposed, records, true)
    }

    /// Apply an ordinary frontier without advancing time. As with
    /// [`Game::transition_regions`], do not substitute a previously settled
    /// result for `proposed`: that result has lost load-frontier provenance.
    pub fn transition_regions_counted_deferred(
        &mut self,
        proposed: &RegionTransition,
        records: &mut dyn RecordStore,
    ) -> Result<(RegionTransition, TransitionReport, PinWork), TransitionError> {
        self.transition_regions_counted_inner(proposed, records, false)
    }

    fn transition_regions_counted_inner(
        &mut self,
        proposed: &RegionTransition,
        records: &mut dyn RecordStore,
        advance: bool,
    ) -> Result<(RegionTransition, TransitionReport, PinWork), TransitionError> {
        let mut cache = PinCache::default();
        let mut t = self.settle(proposed, &mut cache, &|region| records.can_prepare(region));
        let work = PinWork {
            actors: cache.live.len(),
            reaches: cache.reach.len(),
            hold_owners: cache.hold_owners.len(),
            hold_references: cache.hold_owners.values().map(Vec::len).sum(),
            hold_regions: cache
                .hold_regions
                .values()
                .filter(|owners| !owners.is_empty())
                .count(),
        };
        // Settling proved every pin holds for these sets in this state, so
        // if they're the current sets there's nothing to apply or check.
        if self.current_regions() == t {
            return Ok((t, TransitionReport::default(), work));
        }
        loop {
            match self.try_region_transition(&t, proposed, records, advance) {
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
        self.try_region_transition(t, t, records, true)
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
        ordinary_frontier: &RegionTransition,
        records: &mut dyn RecordStore,
        advance: bool,
    ) -> Result<TransitionReport, TransitionFailure> {
        if let Some(region) = t.active.difference(&t.loaded).next() {
            return Err(TransitionError::ActiveNotLoaded(*region).into());
        }
        let mut next = self.clone();
        if advance {
            next.begin_effect_publication()?;
        }
        next.settle_wandering_population()?;
        // Deferred host transitions retain the same unpublished event origins.
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
        next.check_arrival_states(&arrived)?;
        if !arrived.is_empty() {
            // Valid arriving actors may own a body or live dependency in
            // another detached region. Discover that dependency before asking
            // full body validation to require its geometry. Retries retain the
            // original ordinary frontier, never the supplemental loaded set.
            let need = next.settle(ordinary_frontier, &mut PinCache::default(), &|region| {
                records.can_prepare(region)
            });
            if !need.active.is_subset(&t.active) || !need.loaded.is_subset(&t.loaded) {
                return Err(TransitionFailure::Requirements(need));
            }
        }
        next.check_arrivals(&arrived)?;
        let loaded: Vec<_> = next.world.loaded_regions().collect();
        for region in &loaded {
            match (
                t.active.contains(region),
                next.lifecycle.frozen.contains(region),
            ) {
                (true, true) => {
                    next.thaw_region(*region)?;
                    report.thawed.push(*region);
                }
                (false, false) => {
                    next.freeze_region(*region)?;
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
        let need = next.settle(ordinary_frontier, &mut PinCache::default(), &|region| {
            records.can_prepare(region)
        });
        if !need.active.is_subset(&t.active) || !need.loaded.is_subset(&t.loaded) {
            return Err(TransitionFailure::Requirements(need));
        }
        if let Some((_, actor)) = next
            .actors
            .iter()
            .find(|(id, _)| !next.token_reservations_valid(**id, false))
        {
            return Err(TransitionError::InvalidRecord(actor.location.region).into());
        }
        if !next.item_claims_valid(false) || !next.intentions_valid() {
            return Err(TransitionError::InvalidRecord(RegionId(0)).into());
        }
        next.reconcile_wandering_activity()?;
        next.reconcile_recovery_contexts()?;
        if !report.built.is_empty() {
            // A character may start where the objective is met, as it would
            // have if everything had been built at once.
            next.check_run_policy()?;
        }
        if advance && !report.frozen.is_empty() {
            next.advance_to_next_decision()?;
        }
        debug_assert!(
            next.lifecycle_state_valid(),
            "transition kept lifecycle state valid"
        );
        next.finish_effect_publication()?;
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
            })
            || identities.traps.iter().any(|trap| {
                *trap == 0
                    || *trap == u64::MAX
                    || self.world.trap_location(*trap).is_some()
                    || directory.traps.contains_key(trap)
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
                || record
                    .world
                    .traps()
                    .any(|trap| !crate::traps::trap_content_valid(&next.content, &trap.kind))
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
            } else {
                // A compact source can declare its structural region before
                // bounded generation chooses concealed features. Refine only
                // an untouched unbuilt declaration, never existing entities or
                // a record that has already been published.
                let previous = next
                    .lifecycle
                    .directory
                    .by_region
                    .get(&region)
                    .cloned()
                    .unwrap_or_default();
                let wanted = &definition.identities;
                if !next.lifecycle.unbuilt.contains(&region)
                    || previous.actors != wanted.actors
                    || previous.items != wanted.items
                    || (!previous.doors.is_empty() && previous.doors != wanted.doors)
                    || (!previous.traps.is_empty() && previous.traps != wanted.traps)
                    || wanted.doors.iter().any(|id| {
                        *id == 0
                            || *id == u64::MAX
                            || next.world.door_location(*id).is_some()
                            || next
                                .lifecycle
                                .directory
                                .doors
                                .get(id)
                                .is_some_and(|owner| *owner != region)
                    })
                    || wanted.traps.iter().any(|id| {
                        *id == 0
                            || *id == u64::MAX
                            || next.world.trap_location(*id).is_some()
                            || next
                                .lifecycle
                                .directory
                                .traps
                                .get(id)
                                .is_some_and(|owner| *owner != region)
                    })
                {
                    return Err(invalid);
                }
                if let Some(maximum) = wanted.doors.iter().next_back() {
                    next.next_door_id = next
                        .next_door_id
                        .max(maximum.checked_add(1).ok_or(invalid)?);
                }
                next.lifecycle.directory.remove_region(region);
                next.lifecycle.directory.insert(region, wanted.clone());
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
        self.freeze_region(region)?;
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
                traps: world.trap_ids().collect(),
            },
        );
        let mut record = RegionRecord {
            world,
            actors: BTreeMap::new(),
            stamps: BTreeMap::new(),
            ai: BTreeMap::new(),
            navigation: BTreeMap::new(),
            explored_maps: BTreeMap::new(),
            chart_history: Shared::new(BTreeMap::new()),
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
            if let Some(history) = self.chart_history.remove(&id) {
                record.chart_history.insert(id, history);
            }
            if let Some(map) = self.explored_maps.remove(&id) {
                record.explored_maps.insert(id, map);
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
        self.map_validation.detach(id, &record.explored_maps);
        Ok((id, Shared::new(record)))
    }

    /// Validate arriving definitions before computing their bodies or pins.
    /// Complete cross-region geometry is checked once those pins are loaded.
    fn check_arrival_states(
        &mut self,
        arrived: &[(RegionId, RegionIdentities)],
    ) -> Result<(), TransitionError> {
        if arrived.is_empty() {
            return Ok(());
        }
        let mut maps = crate::checkpoint::ExploredOwnerValidation::new(
            arrived
                .iter()
                .flat_map(|(_, ids)| ids.actors.iter())
                .filter_map(|id| {
                    self.explored_maps
                        .get(id)
                        .filter(|map| !self.map_validation.loaded_matches(*id, map))
                }),
        )
        .ok_or(TransitionError::InvalidRecord(arrived[0].0))?;
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
                    && self.explored_maps.get(id).is_none_or(|map| {
                        self.map_validation.loaded_matches(*id, map) || maps.world(map, &self.world)
                    })
            });
            let items_valid = identities.items.iter().all(|id| {
                self.items
                    .get(id)
                    .is_some_and(|item| self.item_state_valid(*id, item))
            });
            if !actors_valid || !items_valid {
                return invalid;
            }
            for actor in &identities.actors {
                if let Some(map) = self.explored_maps.get(actor) {
                    self.map_validation.accept_loaded(*actor, map);
                }
            }
        }
        Ok(())
    }

    /// Finish arrival validation after safe dependency discovery. Bodies and
    /// reciprocal state still have to validate before the candidate can commit.
    fn check_arrivals(
        &self,
        arrived: &[(RegionId, RegionIdentities)],
    ) -> Result<(), TransitionError> {
        if arrived.is_empty() {
            return Ok(());
        }
        // Bodies may not overlap anyone's, loaded before or arriving now.
        let mut occupied = BTreeSet::new();
        let first = arrived[0].0;
        if !self
            .actors
            .values()
            .all(|actor| self.actor_state_valid_body(actor, &mut occupied))
            || !self.combat_valid_with_detached_tokens(true)
            // Check retained Resume state against arriving conditions before
            // thaw/reconciliation could normalize a malformed saved queue.
            || !self.intentions_valid()
            || !self.physics_valid()
            || !self.loaded_holds_valid()
            || !self.loaded_traps_valid()
        {
            return Err(TransitionError::InvalidRecord(first));
        }
        Ok(())
    }

    fn actor_state_valid_body(&self, actor: &Actor, occupied: &mut BTreeSet<Location>) -> bool {
        actor.orientation < 24
            && actor.body.valid()
            && self.spatial_condition_anchors_valid(actor)
            && self.body_has_room(actor, occupied)
    }

    fn attach_record(
        &mut self,
        region: RegionId,
        records: &mut dyn RecordStore,
    ) -> Result<RegionIdentities, TransitionError> {
        let invalid = TransitionError::InvalidRecord(region);
        let unavailable = TransitionError::RecordUnavailable(region);
        let expected_record = self.lifecycle.detached.get(&region).copied();
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
        let trusted_maps = expected_record.is_some_and(|id| {
            self.map_validation
                .detached_matches(id, &record.explored_maps)
        });
        if record
            .actors
            .iter()
            .any(|(id, actor)| !self.hold_actor_shape_valid(*id, actor))
            || !record.valid_contents_with_map_trust(region, self.tick, trusted_maps)
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
            explored_maps,
            chart_history,
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
        if let Some(id) = expected_record {
            self.map_validation.attach(id, &explored_maps, trusted_maps);
        }
        self.actors.extend(actors);
        for id in stamps.keys() {
            self.actors.set_timer_active(*id, false);
        }
        self.lifecycle.stamps.extend(stamps);
        self.combat.ai.extend(ai);
        self.navigation
            .extend(navigation.into_iter().map(|(id, n)| (id, Shared::new(n))));
        self.explored_maps.extend(explored_maps);
        self.chart_history.extend(
            chart_history
                .iter()
                .map(|(actor, owner)| (*actor, owner.clone())),
        );
        if !items.is_empty() {
            self.items.extend(items);
        }
        self.physics.displaced.extend(displaced);
        self.physics.impacts.extend(impacts);
        let identities = self.lifecycle.directory.remove_region(region);
        self.lifecycle.frozen.insert(region);
        // Only an explicit detached configuration can be Pending. Resolve it
        // before arrival validation/pins; known values must match the record
        // and are deliberately left unchanged for that validation to check.
        for id in &identities.actors {
            if self.combat.input_boundaries.get(id)
                == Some(&crate::combat::InputEligibility::Pending)
            {
                self.combat
                    .input_boundaries
                    .insert(*id, self.actor_input_eligibility(*id));
            }
        }
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
        let mut episodes: BTreeSet<_> = self
            .actors
            .active_fall_owners()
            .filter_map(|id| self.actors[&id].motion.active_fall.map(|fall| fall.episode))
            .collect();
        let mut retained = Vec::new();
        for (region, id) in l.detached.iter() {
            let Some(record) = records.get(*id) else {
                return false;
            };
            expected.insert(*region, record.identities());
            if !record.valid_contents(*region, self.tick)
                || record
                    .world
                    .traps()
                    .any(|trap| !crate::traps::trap_content_valid(&self.content, &trap.kind))
                || !record
                    .explored_maps
                    .values()
                    .all(|map| map.checkpoint_valid(&self.world))
                || !record.actors.values().all(|actor| {
                    actor.motion.active_fall.is_none_or(|fall| {
                        fall.owner.0 < self.next_actor_id
                            && fall.episode < self.physics.next_fall_episode
                            && episodes.insert(fall.episode)
                    })
                })
            {
                return false;
            }
            retained.push(record);
        }
        let mut actors: BTreeMap<_, _> =
            self.actors.iter().map(|(id, actor)| (*id, actor)).collect();
        let mut items: BTreeMap<_, _> = self.items.iter().map(|(id, item)| (*id, item)).collect();
        for record in &retained {
            for (id, actor) in &record.actors {
                if actors.insert(*id, actor).is_some() {
                    return false;
                }
            }
            for (id, item) in &record.items {
                if items.insert(*id, item).is_some() {
                    return false;
                }
            }
        }
        self.all_hold_records_valid(&actors, &items) && {
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
            && l.directory.traps.values().all(detached)
            && l.directory.traps.keys().all(|trap| {
                *trap != 0 && *trap != u64::MAX && self.world.trap_location(*trap).is_none()
            })
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

    include!("streaming_retrieval_tests.rs");

    #[test]
    fn checkpoint_rejects_a_token_reservation_with_only_one_loaded_partner() {
        use crate::content_ids::{AbilityId, ConditionId};
        for detach_owner in [false, true] {
            let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
            let mut actors = Vec::new();
            for (region, x) in [(1, 1), (2, 11), (3, 0)] {
                let actor = game
                    .spawn_actor(
                        Location {
                            region: RegionId(region),
                            position: Position { x, y: 1, z: 0 },
                        },
                        NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                crate::test_creatures::configure(
                    &mut game,
                    actor,
                    "neutral",
                    crate::test_creatures::species(),
                );
                actors.push(actor);
            }
            let (owner, holder) = (actors[1], actors[2]);
            game.apply_condition(holder, &ConditionId::new("exposed").unwrap(), holder, 500)
                .unwrap();
            let plan = game
                .ability_plan(owner, &AbilityId::new("basic_melee").unwrap(), holder)
                .unwrap();
            game.start_work(plan, None).unwrap();
            // Bypass the public transition's pins to model a malformed save.
            let region = RegionId(if detach_owner { 2 } else { 3 });
            game.freeze_region(region).unwrap();
            game.detach_record(region).unwrap();
            assert!(game.lifecycle_state_valid());
            let mut shared = crate::checkpoint::SharedState::default();
            let snapshot = game.checkpoint(&mut shared);
            assert!(matches!(
                Game::restore_checkpoint(snapshot, &shared, game.content().clone()),
                Err(crate::checkpoint::PersistError::RunState)
            ));
        }
    }

    #[test]
    fn phase5_checkpoint_rejects_a_supply_hold_with_missing_item() {
        checkpoint_with_one_supply_endpoint(false);
    }
    #[test]
    fn phase5_checkpoint_rejects_a_supply_hold_with_missing_owner() {
        checkpoint_with_one_supply_endpoint(true);
    }
    fn checkpoint_with_one_supply_endpoint(detach_owner: bool) {
        use crate::supplies::{SelectedSupply, SupplySelection};
        let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
        let owner = game
            .spawn_actor(
                Location {
                    region: RegionId(2),
                    position: Position { x: 11, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            owner,
            "neutral",
            crate::test_creatures::species(),
        );
        game.spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        let mut bandage = crate::ItemSpec::ordinary("bandage".into());
        bandage.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
        let item = game
            .place_item_stack(
                10,
                Location {
                    region: RegionId(3),
                    position: Position { x: 0, y: 1, z: 0 },
                },
                None,
                1,
                bandage,
            )
            .unwrap();
        game.admit_intention(
            owner,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(owner),
                supplies: SupplySelection::Explicit {
                    items: vec![SelectedSupply {
                        cost_ordinal: 0,
                        item,
                    }],
                },
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let region = RegionId(if detach_owner { 2 } else { 3 });
        // Deliberately bypass public reciprocal pins to model corrupt stored state.
        game.freeze_region(region).unwrap();
        game.detach_record(region).unwrap();
        assert!(game.lifecycle_state_valid());
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err(),
            "accepted a supply with only one loaded partner; detach_owner={detach_owner}"
        );
    }

    #[test]
    fn phase5_attach_rejects_duplicate_issued_supply_origins() {
        let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
        let mut origins = Vec::new();
        let mut holders = Vec::new();
        for region in [2, 3] {
            let at = Location {
                region: RegionId(region),
                position: Position { x: 5, y: 1, z: 0 },
            };
            let actor = game.spawn_actor(at, NonZeroU64::new(1).unwrap()).unwrap();
            crate::test_creatures::configure(
                &mut game,
                actor,
                "neutral",
                crate::test_creatures::species(),
            );
            holders.push((region, at, actor));
        }
        game.spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(1).unwrap(),
        )
        .unwrap();
        for (region, at, actor) in holders {
            let mut bandage = crate::ItemSpec::ordinary("bandage".into());
            bandage.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
            game.place_item_stack(10 + region, at, Some(actor), 1, bandage)
                .unwrap();
            game.admit_intention(
                actor,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(actor),
                    supplies: crate::supplies::SupplySelection::Carried,
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
            game.execute_next_intention().unwrap().outcome.unwrap();
            origins.push(game.preparation(actor).unwrap().origin.unwrap());
        }
        let mut records = MemoryRecords::default();
        for region in [2, 3] {
            game.freeze_region(RegionId(region)).unwrap();
            let (id, record) = game.detach_record(RegionId(region)).unwrap();
            let mut record = (*record).clone();
            if region == 3 {
                let pending = record
                    .actors
                    .values_mut()
                    .next()
                    .unwrap()
                    .pending
                    .as_mut()
                    .unwrap();
                pending.origin = Some(origins[0]);
                for item in record.items.values_mut() {
                    for hold in &mut item.holds {
                        hold.key.lineage.origin = origins[0];
                    }
                }
            }
            records.put(id, Shared::new(record));
        }
        let before = game.clone();
        assert!(
            game.transition_regions(&sets(&[], &[1, 2, 3]), &mut records)
                .is_err(),
            "completed attach accepted duplicate preparation origins"
        );
        assert_eq!(game, before);
    }

    #[test]
    fn phase5_supply_actor_first_arrival_preserves_frozen_reciprocal_claims() {
        supply_arrival_order(false);
    }
    #[test]
    fn phase5_supply_item_first_arrival_pins_the_owner_active() {
        supply_arrival_order(true);
    }
    fn supply_arrival_order(item_first: bool) {
        let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
        let owner_region = RegionId(if item_first { 3 } else { 2 });
        let item_region = RegionId(if item_first { 2 } else { 3 });
        let at = Location {
            region: owner_region,
            position: Position {
                x: if item_first { 0 } else { 11 },
                y: 1,
                z: 0,
            },
        };
        let item_at = Location {
            region: item_region,
            position: Position {
                x: if item_first { 11 } else { 0 },
                y: 1,
                z: 0,
            },
        };
        let owner = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        crate::test_creatures::configure(
            &mut game,
            owner,
            "neutral",
            crate::test_creatures::species(),
        );
        game.spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        let mut spec = crate::ItemSpec::ordinary("bandage".into());
        spec.supply = Some(crate::content_ids::SupplyId::new("bandage").unwrap());
        let item = game.place_item_stack(10, item_at, None, 1, spec).unwrap();
        game.admit_intention(
            owner,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("bind_wounds").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(owner),
                supplies: crate::supplies::SupplySelection::Explicit {
                    items: vec![crate::supplies::SelectedSupply {
                        cost_ordinal: 0,
                        item,
                    }],
                },
            },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let pending = game.preparation(owner).unwrap().clone();
        let mut records = MemoryRecords::default();
        game.transition_regions(&sets(&[], &[]), &mut records)
            .unwrap();
        game.transition_regions(&sets(&[], &[2, 3]), &mut records)
            .unwrap();
        assert_eq!(game.region_state(owner_region), Some(RegionState::Frozen));
        assert_eq!(game.region_state(item_region), Some(RegionState::Frozen));
        assert_eq!(game.preparation(owner), Some(&pending));
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
        game.add_reference_point(ReferencePoint {
            target: ReferenceTarget::Item(item),
            active_radius: Some(0),
            load_radius: Some(0),
            observes: false,
        })
        .unwrap();
        game.transition_regions(&sets(&[item_region.0], &[item_region.0]), &mut records)
            .unwrap();
        assert_eq!(game.region_state(item_region), Some(RegionState::Active));
        assert_eq!(
            game.region_state(owner_region),
            Some(RegionState::Active),
            "item-only activity must pin its claimant active"
        );
        assert_eq!(game.preparation(owner), Some(&pending));
        assert_eq!(game.items[&item].quantity, 1);
    }

    struct UnavailableSource;
    impl RecordStore for UnavailableSource {
        fn can_prepare(&self, _: RegionId) -> bool {
            false
        }
        fn put(&mut self, _: RecordId, _: Shared<RegionRecord>) {}
        fn get(&mut self, _: RecordId) -> Option<Shared<RegionRecord>> {
            None
        }
    }

    fn unresolved_stair_game() -> (Game, ActorId) {
        let mut game = Game::new(
            tor_world::World::new(vec![], vec![]).unwrap(),
            1,
            crate::test_creatures::content(),
        );
        let region = |id| Region {
            id: RegionId(id),
            name: "chamber".into(),
            bounds: tor_world::Extent::new(3, 3, 2).unwrap(),
        };
        game.add_chamber(region(1)).unwrap();
        game.add_unbuilt_region(region(2), true, RegionIdentities::default())
            .unwrap();
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        game.connect_named_stair(
            at,
            Direction::Down,
            tor_world::NamedAnchor {
                region: RegionId(2),
                name: "up".into(),
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
        (game, actor)
    }

    #[test]
    fn unavailable_unresolved_stair_is_deferred_without_erasing_topology() {
        let (mut game, actor) = unresolved_stair_game();
        let proposed = sets(&[1], &[1]);
        assert!(game
            .settle_counted(&proposed)
            .0
            .loaded
            .contains(&RegionId(2)));
        let mut source = UnavailableSource;
        let (settled, _) = game.settle_counted_from_source(&proposed, &source);
        assert_eq!(settled, proposed);
        game.transition_regions(&proposed, &mut source).unwrap();
        let at = game.actors[&actor].location;
        assert!(game.world.is_stair(at, Direction::Down));
        let before = game.clone();
        assert!(game.act(actor, Action::Move(Direction::Down)).is_err());
        assert_eq!(game, before);
        game.act(actor, Action::Wait).unwrap();
        game.act(actor, Action::Move(Direction::West)).unwrap();
    }

    #[test]
    fn unavailable_source_cannot_relax_resolved_or_explicit_destination_pins() {
        let (mut game, _) = unresolved_stair_game();
        let mut source = UnavailableSource;
        let requested = sets(&[1, 2], &[1, 2]);
        assert!(game
            .settle_counted_from_source(&requested, &source)
            .0
            .active
            .contains(&RegionId(2)));
        let before = game.clone();
        assert!(game.transition_regions(&requested, &mut source).is_err());
        assert_eq!(game, before);
        game.register_named_anchors(
            RegionId(2),
            BTreeMap::from([("up".into(), Position { x: 1, y: 1, z: 0 })]),
        )
        .unwrap();
        let proposed = sets(&[1], &[1]);
        assert!(game
            .settle_counted_from_source(&proposed, &source)
            .0
            .loaded
            .contains(&RegionId(2)));
        let before = game.clone();
        assert!(game.transition_regions(&proposed, &mut source).is_err());
        assert_eq!(game, before);
    }

    #[test]
    fn unavailable_source_cannot_relax_physical_link_pins() {
        let mut game = Game::region_corridor(1, 2, crate::test_creatures::content());
        let _record = game.detach_record(RegionId(2)).unwrap();
        let mut source = UnavailableSource;
        let proposed = sets(&[1], &[1]);
        assert!(game
            .settle_counted_from_source(&proposed, &source)
            .0
            .loaded
            .contains(&RegionId(2)));
        let before = game.clone();
        assert!(game.transition_regions(&proposed, &mut source).is_err());
        assert_eq!(game, before);
    }

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
        let mut game = Game::region_corridor(1, 7, crate::test_creatures::content());
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
        let mut species = crate::test_creatures::species();
        species.behavior.memory_ticks = 150;
        let mut game = Game::region_corridor(
            1,
            4,
            crate::test_creatures::content_with(species.clone(), vec![]),
        );
        let player = game
            .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let watcher = game
            .spawn_actor(at(3, 5), NonZeroU64::new(100).unwrap())
            .unwrap();
        crate::test_creatures::configure(&mut game, watcher, "neutral", species);
        assert!(
            game.creature(watcher).is_some(),
            "frozen subjects own their builds"
        );
        let profile = crate::ai::AiProfile {
            care: true,
            equipment: true,
            loot: true,
            gold: false,
            flee_percent: 25,
            idle: crate::behavior::IdleBehavior::Wait,
            doors: false,
        };
        game.configure_ai(watcher, profile).unwrap();
        game.configure_run(player, BTreeSet::from([player]), None, BTreeMap::new())
            .unwrap();
        game.add_default_reference_points().unwrap();
        game.actors.get_mut(&watcher).unwrap().behavior.engagement =
            Some(crate::behavior::Engagement {
                reference: crate::perception::ActorReferenceKind::Creature,
                target: player,
                location: at(1, 2),
                last_seen: 0,
            });
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
        let seen = game.actors[&watcher].behavior.engagement.unwrap().last_seen;
        assert_eq!(seen, 400);
    }

    #[test]
    fn thawing_rejects_engagement_deadline_overflow_atomically() {
        let at = |region, x| Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        };
        let mut game = Game::region_corridor(1, 4, crate::test_creatures::content());
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
        game.configure_run(player, BTreeSet::from([player]), None, BTreeMap::new())
            .unwrap();
        game.actors.get_mut(&watcher).unwrap().behavior.engagement =
            Some(crate::behavior::Engagement {
                reference: crate::perception::ActorReferenceKind::Creature,
                target: player,
                location: at(1, 2),
                last_seen: 0,
            });
        game.freeze_region(RegionId(3)).unwrap();
        game.tick = u64::MAX - 100;
        let before = game.clone();
        let mut records = MemoryRecords::default();
        assert_eq!(
            game.transition_regions(&sets(&[1, 2, 3, 4], &[1, 2, 3, 4]), &mut records),
            Err(TransitionError::Rule(GameError::Schedule(
                crate::ScheduleError::TimeExhausted
            )))
        );
        assert_eq!(game, before);
    }

    /// A player in region 1 of a four-region corridor and another actor in
    /// region 3, with everything but region 1 and its neighbour detached.
    fn detached_corridor() -> (Game, ActorId, MemoryRecords) {
        let at = |region, x| Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        };
        let mut game = Game::region_corridor(1, 4, crate::test_creatures::content());
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
                record.explored_maps.remove(&other);
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
        let mut template = Game::region_corridor(1, 4, crate::test_creatures::content());
        let far = template
            .spawn_actor(
                Location {
                    region: RegionId(3),
                    position: Position { x: 5, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut game = Game::new(
            tor_world::World::new(vec![], vec![]).unwrap(),
            1,
            crate::test_creatures::content(),
        );
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
    fn generated_feature_refinement_is_atomic_and_cannot_steal_reserved_identity() {
        use tor_world::{Extent, World};
        let make = |region: u64, door: u64, trap: u64| {
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_chamber(Region {
                    id: RegionId(region),
                    name: "features".into(),
                    bounds: Extent::new(5, 5, 2).unwrap(),
                })
                .unwrap();
            let at = Location {
                region: RegionId(region),
                position: Position { x: 2, y: 2, z: 0 },
            };
            let mut template = Game::new(world, 1, crate::test_creatures::content());
            template.place_authored_door(door, at, false, 2).unwrap();
            template
                .conceal_door(at, tor_world::Concealment { dc: 12 })
                .unwrap();
            template
                .place_trap(
                    at,
                    tor_world::Trap {
                        id: trap,
                        dc: 12,
                        avoidance_dc: None,
                        kind: tor_world::TrapKind::Rust {
                            slot: "body_armor".into(),
                            amount: 1,
                        },
                    },
                )
                .unwrap();
            let record = template.into_region_record(RegionId(region)).unwrap();
            (
                UnbuiltRegion {
                    region: Region {
                        id: RegionId(region),
                        name: "features".into(),
                        bounds: Extent::new(5, 5, 2).unwrap(),
                    },
                    chamber: true,
                    identities: record.identities(),
                },
                record,
            )
        };
        let mut game = Game::new(
            World::new(vec![], vec![]).unwrap(),
            1,
            crate::test_creatures::content(),
        );
        for id in [1, 2] {
            game.add_unbuilt_region(
                Region {
                    id: RegionId(id),
                    name: "features".into(),
                    bounds: Extent::new(5, 5, 2).unwrap(),
                },
                true,
                RegionIdentities::default(),
            )
            .unwrap();
        }
        let mut store = MemoryRecords::default();
        let before = game.clone();
        assert!(game
            .register_generated_regions(vec![make(1, 10, 20), make(2, 10, 21)], &mut store)
            .is_err());
        assert_eq!(game, before);
        assert_eq!(store.len(), 0);
        game.register_generated_regions(vec![make(1, 10, 20), make(2, 11, 21)], &mut store)
            .unwrap();
        assert!(game.lifecycle_state_valid());
        assert_eq!(store.len(), 2);
        assert_eq!(game.lifecycle.directory.traps[&20], RegionId(1));
        assert_eq!(game.next_door_id, 12);
        // Detached records retain metadata; reopening cannot reroll a generated choice.
        assert!(game.detached_records_valid(&mut store));
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
        let mut game = Game::new(
            tor_world::World::new(vec![], vec![]).unwrap(),
            1,
            crate::test_creatures::content(),
        );
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
        let mut game = Game::region_corridor(1, 2, crate::test_creatures::content());
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
        let mut game = Game::new(
            World::new(vec![], vec![]).unwrap(),
            1,
            crate::test_creatures::content(),
        );
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
            traps: BTreeSet::from([door + 100]),
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
        let mut game = Game::new(world, 42, crate::test_creatures::content());
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
        let need = game.point_requirements(&|_| true);
        assert_eq!(need.active, perceived);
        assert!(
            need.loaded.contains(&RegionId(3)),
            "dark geometry still needs to be read"
        );
    }
}

#[cfg(test)]
mod explored_map_tests {
    use super::*;
    use crate::explored_map::{MapAppearance, Witness};
    use std::num::NonZeroU64;
    use tor_world::Position;

    fn at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        }
    }

    fn remembered() -> Shared<MapAppearance> {
        Shared::new(MapAppearance {
            object_presentation: None,
            wall: true,
            material: "stale stone".into(),
            asset: None,
            place_hint: false,
            stairs_up: false,
            stairs_down: true,
            trap: None,
            door: None,
            ground_items: Vec::new(),
        })
    }

    fn detached() -> (Game, ActorId, RecordId, Shared<RegionRecord>) {
        let mut game = Game::region_corridor(42, 3, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut map = ExploredMap::default();
        map.cells.insert(at(1, 1), remembered());
        map.cells.insert(at(3, 9), remembered());
        map.witnesses.insert(
            at(3, 9),
            Witness::Placed {
                anchor: at(1, 1),
                delta: [32, 0, 0],
                rotation: 0,
            },
        );
        map.children.insert((at(1, 1), [32, 0, 0], at(3, 9)), 0);
        game.explored_maps.insert(actor, Shared::new(map));
        let original = game.explored_maps[&actor].clone();
        game.freeze_region(RegionId(1)).unwrap();
        let (id, record) = game.detach_record(RegionId(1)).unwrap();
        assert!(record.explored_maps[&actor].shares_storage(&original));
        (game, actor, id, record)
    }

    fn acquired(size: i32) -> (Game, ActorId) {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "memory".into(),
                bounds: tor_world::Extent::new(6000, 3, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let inputs = (1..=size)
            .map(|x| crate::MapCellInput {
                location: at(1, x),
                appearance: (*remembered()).clone(),
            })
            .collect::<Vec<_>>();
        game.record_explored_observation(actor, &inputs, &[])
            .unwrap();
        (game, actor)
    }

    #[test]
    fn trusted_repeat_arrivals_do_not_rescan_history_and_release_detached_proofs() {
        for size in [16, 4096, 5000] {
            let (mut game, actor) = acquired(size);
            game.freeze_region(RegionId(1)).unwrap();
            for _ in 0..3 {
                let (id, record) = game.detach_record(RegionId(1)).unwrap();
                assert!(game
                    .map_validation
                    .detached_matches(id, &record.explored_maps));
                assert!(!game
                    .map_validation
                    .detached_matches(RecordId(id.0 + 1), &record.explored_maps));
                let branch = game.clone();
                let mut records = MemoryRecords::default();
                records.put(id, record.clone());
                crate::reset_map_work_counts();
                let identities = game.attach_record(RegionId(1), &mut records).unwrap();
                game.check_arrival_states(&[(RegionId(1), identities)])
                    .unwrap();
                let work = crate::map_work_counts();
                assert_eq!(
                    (
                        work.validation_cells,
                        work.validation_relations,
                        work.world_location_checks
                    ),
                    (0, 0, 0)
                );
                assert!(work.validation_directory_entries_copied <= 1);
                assert!(!game
                    .map_validation
                    .detached_matches(id, &record.explored_maps));
                assert!(branch
                    .map_validation
                    .detached_matches(id, &record.explored_maps));
                assert!(game
                    .map_validation
                    .loaded_matches(actor, &game.explored_maps[&actor]));
            }
        }
    }

    #[test]
    fn trusted_arrival_proofs_reject_swapped_branch_and_cow_forged_roots() {
        let (mut original, actor) = acquired(16);
        let mut branch = original.clone();
        branch
            .add_region(tor_world::Region {
                id: RegionId(2),
                name: "other branch".into(),
                bounds: tor_world::Extent::new(3, 3, 1).unwrap(),
            })
            .unwrap();
        branch
            .record_explored_observation(
                actor,
                &[crate::MapCellInput {
                    location: at(2, 1),
                    appearance: (*remembered()).clone(),
                }],
                &[],
            )
            .unwrap();
        original.freeze_region(RegionId(1)).unwrap();
        branch.freeze_region(RegionId(1)).unwrap();
        let (id, record) = original.detach_record(RegionId(1)).unwrap();
        let (branch_id, branch_record) = branch.detach_record(RegionId(1)).unwrap();
        assert_eq!(
            id, branch_id,
            "rewound branches may reuse an allocation ordinal"
        );
        let mut forged = (*record).clone();
        forged
            .explored_maps
            .get_mut(&actor)
            .unwrap()
            .cells
            .insert(at(1, 999_999), remembered());
        for replacement in [branch_record, Shared::new(forged)] {
            assert!(!original
                .map_validation
                .detached_matches(id, &replacement.explored_maps));
            let mut records = MemoryRecords::default();
            records.put(id, replacement);
            let before = original.clone();
            crate::reset_map_work_counts();
            assert_eq!(
                original.transition_regions(
                    &RegionTransition {
                        active: BTreeSet::from([RegionId(1)]),
                        loaded: BTreeSet::from([RegionId(1)]),
                    },
                    &mut records
                ),
                Err(TransitionError::InvalidRecord(RegionId(1)))
            );
            assert_eq!(original, before);
            assert!(crate::map_work_counts().validation_cells >= 16);
            assert!(crate::map_work_counts().world_location_checks >= 16);
        }
    }

    #[test]
    fn untrusted_decoded_arrival_earns_proof_only_after_full_validation() {
        let (mut game, actor) = acquired(64);
        game.freeze_region(RegionId(1)).unwrap();
        let (id, record) = game.detach_record(RegionId(1)).unwrap();
        let bytes = serde_json::to_vec(&*record).unwrap();
        let restored = serde_json::from_slice::<RegionRecordData>(&bytes)
            .unwrap()
            .restore(game.content().clone())
            .unwrap();
        assert!(!game
            .map_validation
            .detached_matches(id, &restored.explored_maps));
        let mut records = MemoryRecords::default();
        records.put(id, Shared::new(restored));
        crate::reset_map_work_counts();
        let identities = game.attach_record(RegionId(1), &mut records).unwrap();
        game.check_arrival_states(&[(RegionId(1), identities)])
            .unwrap();
        let work = crate::map_work_counts();
        assert_eq!(
            (work.validation_cells, work.world_location_checks),
            (128, 64)
        );
        assert!(game
            .map_validation
            .loaded_matches(actor, &game.explored_maps[&actor]));
    }

    #[test]
    fn untrusted_shared_owner_roots_validate_once_per_record_operation() {
        const CELLS: i32 = 4096;
        for owners in [1, 32, 64] {
            let (mut game, actor) = acquired(CELLS);
            let map = game.explored_maps[&actor].clone();
            for x in 2..=owners {
                let other = game
                    .spawn_actor(at(1, x), NonZeroU64::new(100).unwrap())
                    .unwrap();
                game.explored_maps.insert(other, map.clone());
            }
            game.freeze_region(RegionId(1)).unwrap();
            let (id, record) = game.detach_record(RegionId(1)).unwrap();
            let bytes = serde_json::to_vec(&*record).unwrap();
            let decoded = serde_json::from_slice::<RegionRecordData>(&bytes).unwrap();
            crate::reset_map_work_counts();
            let restored = decoded.restore(game.content().clone()).unwrap();
            assert_eq!(crate::map_work_counts().validation_cells, CELLS as usize);
            crate::reset_map_work_counts();
            assert!(restored.valid_contents(RegionId(1), game.tick()));
            assert_eq!(crate::map_work_counts().validation_cells, CELLS as usize);
            let mut records = MemoryRecords::default();
            records.put(id, Shared::new(restored));
            crate::reset_map_work_counts();
            let identities = game.attach_record(RegionId(1), &mut records).unwrap();
            game.check_arrival_states(&[(RegionId(1), identities)])
                .unwrap();
            let work = crate::map_work_counts();
            assert_eq!(work.validation_cells, 2 * CELLS as usize);
            assert_eq!(work.world_location_checks, CELLS as usize);
        }
    }

    #[test]
    fn replacing_known_region_or_chamber_metadata_invalidates_loaded_and_detached_proofs() {
        for chamber in [false, true] {
            let (mut game, actor) = acquired(16);
            game.add_region(tor_world::Region {
                id: RegionId(2),
                name: "old bounds".into(),
                bounds: tor_world::Extent::new(10, 3, 1).unwrap(),
            })
            .unwrap();
            let other = game
                .spawn_actor(at(2, 1), NonZeroU64::new(100).unwrap())
                .unwrap();
            let input = [crate::MapCellInput {
                location: at(2, 9),
                appearance: (*remembered()).clone(),
            }];
            for owner in [actor, other] {
                game.record_explored_observation(owner, &input, &[])
                    .unwrap();
            }
            game.freeze_region(RegionId(2)).unwrap();
            let (id, record) = game.detach_record(RegionId(2)).unwrap();
            assert!(game
                .map_validation
                .loaded_matches(actor, &game.explored_maps[&actor]));
            assert!(game
                .map_validation
                .detached_matches(id, &record.explored_maps));
            let replacement = tor_world::Region {
                id: RegionId(2),
                name: "new bounds".into(),
                bounds: tor_world::Extent::new(2, 3, 1).unwrap(),
            };
            if chamber {
                game.add_chamber(replacement).unwrap();
            } else {
                game.add_region(replacement).unwrap();
            }
            assert!(!game
                .map_validation
                .loaded_matches(actor, &game.explored_maps[&actor]));
            assert!(!game
                .map_validation
                .detached_matches(id, &record.explored_maps));
            // Old absent metadata cannot be used to re-earn a proof while new
            // loaded metadata now owns the region's authoritative bounds.
            assert!(!game.explored_maps[&actor].world_locations_valid(&game.world));
            assert_eq!(
                game.record_explored_observation(actor, &input, &[]),
                Err(GameError::InvalidLocation)
            );
            // A new owner has no previous map to reject first. This isolates
            // admission of a newly supplied location against current bounds.
            let fresh = game
                .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
                .unwrap();
            assert!(
                game.world.knows(input[0].location),
                "old absent metadata still covers the input"
            );
            assert!(!game.explored_maps.contains_key(&fresh));
            assert_eq!(
                game.record_explored_observation(fresh, &input, &[]),
                Err(GameError::InvalidLocation)
            );
            assert!(!game.explored_maps.contains_key(&fresh));
        }
    }

    #[test]
    fn detaching_and_reattaching_an_actor_retains_its_complete_explored_map() {
        let (mut game, actor, id, record) = detached();
        assert!(!game.explored_maps.contains_key(&actor));
        assert_eq!(record.explored_maps[&actor].cells.iter().count(), 2);
        assert!(record.explored_maps[&actor].cells.contains_key(&at(3, 9)));
        let bytes = serde_json::to_vec(&*record).unwrap();
        let decoded: RegionRecordData = serde_json::from_slice(&bytes).unwrap();
        let restored = decoded.restore(game.content().clone()).unwrap();
        assert_eq!(restored, *record);
        assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
        // Reading saved appearance evidence does not attach its actor's terrain.
        assert_eq!(game.region_state(RegionId(1)), Some(RegionState::Detached));
        let expected = restored.explored_maps[&actor].clone();
        let mut records = MemoryRecords::default();
        records.put(id, Shared::new(restored));
        assert!(game.detached_records_valid(&mut records));
        let identities = game.attach_record(RegionId(1), &mut records).unwrap();
        game.check_arrival_states(&[(RegionId(1), identities)])
            .unwrap();
        assert!(game.explored_maps[&actor].shares_storage(&expected));
        assert_eq!(
            game.explored_maps[&actor]
                .cells
                .get(&at(3, 9))
                .unwrap()
                .material,
            "stale stone"
        );
        assert_eq!(game.explored_maps[&actor].children.iter().count(), 1);
    }

    #[test]
    fn detached_explored_maps_require_the_current_shape_and_valid_owner_references() {
        let (game, actor, _, record) = detached();
        let value = serde_json::to_value(RegionRecordData::capture(&record)).unwrap();
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("explored_maps");
        assert!(serde_json::from_value::<RegionRecordData>(missing).is_err());
        let mut dangling = value.clone();
        dangling["explored_maps"]["owners"][0][1] = serde_json::json!(999);
        assert!(serde_json::from_value::<RegionRecordData>(dangling).is_err());
        let mut duplicate = value.clone();
        let owner = duplicate["explored_maps"]["owners"][0].clone();
        duplicate["explored_maps"]["owners"]
            .as_array_mut()
            .unwrap()
            .push(owner);
        assert!(serde_json::from_value::<RegionRecordData>(duplicate).is_err());
        let mut other_actor = RegionRecordData::capture(&record);
        let map = other_actor.explored_maps.remove(&actor).unwrap();
        other_actor.explored_maps.insert(ActorId(999), map);
        assert_eq!(
            other_actor.restore(game.content().clone()),
            Err(crate::PersistError::ExploredMap)
        );
        let mut corrupt = value;
        corrupt["explored_maps"]["maps"]["children"][0][0][1] = serde_json::json!(23);
        assert!(serde_json::from_value::<RegionRecordData>(corrupt).is_err());
    }

    #[test]
    fn arrival_rejects_unknown_remembered_locations_without_publishing_partial_state() {
        let (mut game, actor, id, record) = detached();
        let mut damaged = (*record).clone();
        damaged
            .explored_maps
            .get_mut(&actor)
            .unwrap()
            .cells
            .insert(at(3, 999), remembered());
        assert!(damaged.valid_contents(RegionId(1), game.tick()));
        let mut records = MemoryRecords::default();
        records.put(id, Shared::new(damaged));
        assert!(!game.detached_records_valid(&mut records));
        let before = game.clone();
        let request = RegionTransition {
            active: BTreeSet::from([RegionId(1), RegionId(2), RegionId(3)]),
            loaded: BTreeSet::from([RegionId(1), RegionId(2), RegionId(3)]),
        };
        assert_eq!(
            game.transition_regions(&request, &mut records),
            Err(TransitionError::InvalidRecord(RegionId(1)))
        );
        assert_eq!(game, before);
        assert!(!game.explored_maps.contains_key(&actor));
    }
}

#[cfg(test)]
mod historical_chart_streaming_tests {
    use super::*;
    use crate::chart_history::{Attempt, AttemptStatus, Kind};
    #[test]
    fn frozen_owner_detach_sourcefree_record_roundtrip_and_attach_keeps_history() {
        let (mut game, actor, root) = crate::chart_history::tests::fixture();
        game.chart_history
            .entry(actor)
            .or_default()
            .record_attempt(
                Kind::Food,
                Attempt {
                    attempted_at: 0,
                    status: AttemptStatus::Positive,
                },
                Some(root.clone()),
            )
            .unwrap();
        game.freeze_region(RegionId(1)).unwrap();
        let (id, record) = game.detach_record(RegionId(1)).unwrap();
        assert!(game.chart_history(actor).is_none());
        assert!(record.chart_history[&actor]
            .get(Kind::Food)
            .latest()
            .unwrap()
            .shares_storage(&root));
        assert!(record
            .historical_chart_authorities()
            .all(|(world, _, _)| world.region(RegionId(1)).is_some()));
        let encoded = serde_json::to_vec(&RegionRecordData::capture(&record)).unwrap();
        let decoded: RegionRecordData = serde_json::from_slice(&encoded).unwrap();
        let decoded = decoded.restore(game.content().clone()).unwrap();
        let mut records = MemoryRecords::default();
        records.put(id, Shared::new(decoded));
        let identities = game.attach_record(RegionId(1), &mut records).unwrap();
        game.check_arrival_states(&[(RegionId(1), identities)])
            .unwrap();
        let history = game.chart_history(actor).unwrap().get(Kind::Food);
        assert_eq!(history.attempt().unwrap().status, AttemptStatus::Positive);
        assert_eq!(history.latest().unwrap().chart(), root.chart());
        assert_eq!(history.latest().unwrap().chart().rows().len(), 2);
    }
}
