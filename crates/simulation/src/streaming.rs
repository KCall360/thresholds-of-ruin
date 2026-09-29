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
use tor_world::{Direction, Location, RegionId, RegionSlice, Shared};

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
/// stored or restored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RegionRecord {
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
    detached: BTreeMap<RegionId, Shared<RegionRecord>>,
    directory: Directory,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            points: BTreeMap::new(),
            next_point: 1,
            frozen: BTreeSet::new(),
            stamps: BTreeMap::new(),
            detached: BTreeMap::new(),
            directory: Directory::default(),
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
        if self.lifecycle.detached.contains_key(&region) {
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
    ) -> Result<(RegionTransition, TransitionReport), TransitionError> {
        let mut t = self.settle_region_transition(proposed);
        loop {
            match self.apply_region_transition(&t) {
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
    /// error the game is unchanged. If the actor due next froze, time
    /// advances to the next decision, as after an action.
    pub fn apply_region_transition(
        &mut self,
        t: &RegionTransition,
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
            .filter(|r| next.lifecycle.detached.contains_key(r))
            .copied()
            .collect();
        for region in attach {
            next.attach_record(region)?;
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
        for region in loaded {
            if !t.loaded.contains(&region) {
                next.detach_record(region)?;
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
        if !report.frozen.is_empty() {
            next.advance_to_next_decision();
        }
        debug_assert!(
            next.lifecycle_valid(),
            "transition kept lifecycle state valid"
        );
        *self = next;
        Ok(report)
    }

    fn detach_record(&mut self, region: RegionId) -> Result<(), TransitionError> {
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
        self.lifecycle.frozen.remove(&region);
        self.lifecycle.detached.insert(region, Shared::new(record));
        Ok(())
    }

    fn attach_record(&mut self, region: RegionId) -> Result<(), TransitionError> {
        let invalid = TransitionError::InvalidRecord(region);
        let record = self.lifecycle.detached.remove(&region).ok_or(invalid)?;
        // Checked here, not only on restore, because the record may come
        // from storage that restore never read.
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

    /// Lifecycle invariants for checkpoint restoration.
    pub(crate) fn lifecycle_valid(&self) -> bool {
        self.lifecycle_state_valid() && self.records_valid()
    }

    /// Every detached record is valid, and together they hold exactly the
    /// directory's identities. Reads every record.
    fn records_valid(&self) -> bool {
        let l = &self.lifecycle;
        let mut expected = Directory::default();
        l.detached.iter().all(|(region, record)| {
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
        }) && expected == l.directory
    }

    /// Game-wide lifecycle invariants. Doesn't read record contents, so it
    /// holds while records are stored elsewhere.
    fn lifecycle_state_valid(&self) -> bool {
        let l = &self.lifecycle;
        let detached = |region: &RegionId| l.detached.contains_key(region);
        l.directory.actors.values().all(detached)
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
            && l.detached.keys().copied().eq(self.world.detached_regions())
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
        let only = |ids: &[u64]| ids.iter().map(|id| RegionId(*id)).collect::<BTreeSet<_>>();
        game.transition_regions(&RegionTransition {
            active: only(&[1]),
            loaded: only(&[1]),
        })
        .unwrap();
        for _ in 0..4 {
            game.act(player, Action::Wait).unwrap();
        }
        // Longer than the watcher's memory, but it was frozen throughout.
        assert_eq!(game.tick(), 400);
        game.transition_regions(&RegionTransition {
            active: only(&[1, 2, 3, 4]),
            loaded: only(&[1, 2, 3, 4]),
        })
        .unwrap();
        let (_, _, seen) = game.combat.ai[&watcher].target.unwrap();
        assert_eq!(seen, 400);
    }

    #[test]
    fn attach_rejects_a_record_that_does_not_match_the_directory() {
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
        let only = |ids: &[u64]| ids.iter().map(|id| RegionId(*id)).collect::<BTreeSet<_>>();
        game.transition_regions(&RegionTransition {
            active: only(&[1]),
            loaded: only(&[1]),
        })
        .unwrap();
        assert_eq!(game.region_state(RegionId(3)), Some(RegionState::Detached));
        assert!(game.lifecycle_valid());

        // A record that's valid on its own but lost an actor the directory
        // still places in its region, as a damaged store might return.
        let three = RegionId(3);
        let mut record = (*game.lifecycle.detached[&three]).clone();
        assert!(record.actors.remove(&other).is_some());
        record.stamps.remove(&other);
        record.ai.remove(&other);
        record.navigation.remove(&other);
        record.displaced.remove(&other);
        assert!(record.valid_contents(three, game.tick()));
        assert!(!record.matches_directory(three, &game.lifecycle.directory));
        game.lifecycle.detached.insert(three, Shared::new(record));
        assert!(game.lifecycle_state_valid(), "needs no record contents");
        assert!(!game.records_valid());

        let before = game.clone();
        let error = game
            .transition_regions(&RegionTransition {
                active: only(&[1, 2, 3, 4]),
                loaded: only(&[1, 2, 3, 4]),
            })
            .unwrap_err();
        assert_eq!(error, TransitionError::InvalidRecord(three));
        assert_eq!(game, before);
    }
}
