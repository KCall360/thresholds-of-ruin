//! Authoritative actors, anchor-region membership and derived portal-local bodies.
//! Mutations cannot escape through a mutable map. Geometry witnesses and dirty
//! identities control reuse; neither participates in saved state or equality.
use crate::navigation_map::RegionMap;
use crate::{Actor, ActorId, BodySpec};
use std::collections::{BTreeMap, BTreeSet};
use std::ops::{Deref, DerefMut};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use tor_world::{GeometrySnapshot, Location, Position, RegionId, Shared, World};

type Members = Shared<BTreeMap<RegionId, Shared<BTreeSet<ActorId>>>>;
type Occupants = Shared<BTreeMap<ActorId, u8>>;
pub(crate) type BodyCells = Shared<Vec<(Location, u8)>>;
type PerceivedBodies = Vec<(ActorId, BodyCells)>;

// Like World perception versions, this is only a branch-safe derived cache
// witness. It is never persisted, ordered as gameplay, or compared by Game::eq.
static NEXT_PERCEPTION_REVISION: AtomicU64 = AtomicU64::new(1);
fn fresh_perception_revision() -> u64 {
    NEXT_PERCEPTION_REVISION.fetch_add(1, Ordering::Relaxed)
}

/// Every actor input used by sensed-hostile recovery. Future sensory capabilities
/// must update this witness together with observation::SceneWitness.
struct PerceptionInputs {
    perception: crate::perception::PerceptionCapabilities,
    nonvisual_range: u8,
    guise: crate::perception::GuiseState,
    recognized_guises: Vec<ActorId>,
    location: Location,
    orientation: u8,
    body: Shared<BodySpec>,
    alive: bool,
    faction: Option<String>,
    recovery_owner: bool,
    engagement: Option<crate::behavior::Engagement>,
    behavior_policy: Option<crate::behavior::BehaviorPolicy>,
}
impl PerceptionInputs {
    fn capture(actor: &Actor) -> Self {
        crate::diagnostics::perception_witness_capture();
        Self {
            perception: actor.perception(),
            nonvisual_range: actor.nonvisual_range(),
            guise: actor.guise.clone(),
            recognized_guises: actor.discovery.recognized_guises(),
            location: actor.location,
            orientation: actor.orientation,
            body: actor.body.clone(),
            alive: actor.alive(),
            faction: actor
                .combat
                .as_ref()
                .map(|combat| combat.identity.faction.clone()),
            recovery_owner: watches_recovery(actor),
            engagement: actor.behavior.engagement,
            behavior_policy: actor.behavior_policy(),
        }
    }
    fn matches(&self, actor: &Actor) -> bool {
        self.perception == actor.perception()
            && self.nonvisual_range == actor.nonvisual_range()
            && self.guise == actor.guise
            && self.recognized_guises == actor.discovery.recognized_guises()
            && self.location == actor.location
            && self.orientation == actor.orientation
            && self.body == actor.body
            && self.alive == actor.alive()
            && self.faction.as_deref()
                == actor
                    .combat
                    .as_ref()
                    .map(|combat| combat.identity.faction.as_str())
            && self.recovery_owner == watches_recovery(actor)
            && self.engagement == actor.behavior.engagement
            && self.behavior_policy == actor.behavior_policy()
    }
}

/// Compact always-on visibility inputs. Guise recognition changes identity
/// authority; other knowledge, failed receipts, health amounts and clocks do not.
#[derive(PartialEq, Eq)]
struct VisibilityInputs {
    faction: Option<String>,
    continual_stealth: Option<u32>,
    invisible: bool,
    see_invisible: bool,
    sight: bool,
    nonvisual_range: u8,
    guised: bool,
    recognized_guises: Vec<ActorId>,
    location: Location,
    orientation: u8,
    body: Shared<BodySpec>,
    alive: bool,
}
impl VisibilityInputs {
    fn capture(actor: &Actor) -> Self {
        let perception = actor
            .combat
            .as_ref()
            .map(|combat| &combat.creature().derived().perception);
        Self {
            faction: actor.combat.as_ref().map(|c| c.identity.faction.clone()),
            continual_stealth: perception.and_then(|p| p.continual_stealth),
            invisible: perception.is_some_and(|p| p.invisible),
            see_invisible: perception.is_some_and(|p| p.see_invisible),
            sight: perception.is_none_or(|p| p.sight),
            nonvisual_range: actor.nonvisual_range(),
            guised: actor.guise.appearance().is_some(),
            recognized_guises: actor.discovery.recognized_guises(),
            location: actor.location,
            orientation: actor.orientation,
            body: actor.body.clone(),
            alive: actor.alive(),
        }
    }
}

fn add_member(regions: &mut Members, region: RegionId, id: ActorId) {
    regions.entry(region).or_default().insert(id);
}

fn remove_member(regions: &mut Members, region: RegionId, id: ActorId) {
    let members = regions.get_mut(&region).expect("indexed actor region");
    assert!(members.remove(&id), "indexed actor identity");
    if members.is_empty() {
        regions.remove(&region);
    }
}

#[derive(Clone)]
struct BodyEntry {
    location: Location,
    orientation: u8,
    body: Shared<BodySpec>,
    cells: Option<BodyCells>,
}

impl BodyEntry {
    fn matches(&self, actor: &Actor) -> bool {
        self.location == actor.location
            && self.orientation == actor.orientation
            && self.body == actor.body
    }
}

#[derive(Clone, Default)]
struct Bodies {
    geometry: Option<GeometrySnapshot>,
    entries: Shared<BTreeMap<ActorId, Shared<BodyEntry>>>,
    cells: Shared<RegionMap<Location, Occupants>>,
    dirty: BTreeSet<ActorId>,
}

impl Bodies {
    fn refresh(&mut self, actors: &BTreeMap<ActorId, Actor>, world: &World) {
        let geometry = world.geometry_snapshot();
        if self.geometry.as_ref() != Some(&geometry) {
            *self = Self {
                geometry: Some(geometry),
                dirty: actors.keys().copied().collect(),
                ..Self::default()
            };
        }
        for id in std::mem::take(&mut self.dirty) {
            let actor = actors.get(&id);
            if actor
                .zip(self.entries.get(&id))
                .is_some_and(|(actor, entry)| entry.matches(actor))
            {
                continue;
            }
            if let Some(old) = self.entries.remove(&id) {
                for &(at, _) in old.cells.iter().flat_map(|cells| cells.iter()) {
                    let occupants = self.cells.get_mut(&at).expect("indexed body cell");
                    assert!(occupants.remove(&id).is_some(), "indexed body identity");
                    if occupants.is_empty() {
                        self.cells.remove(&at);
                    }
                }
            }
            if let Some(actor) = actor {
                let cells = crate::physics::resolve_body(
                    world,
                    actor.location,
                    actor.orientation,
                    &actor.body,
                );
                for &(at, frame) in cells.iter().flatten() {
                    if !self.cells.contains_key(&at) {
                        self.cells.insert(at, Shared::default());
                    }
                    self.cells
                        .get_mut(&at)
                        .expect("inserted body cell")
                        .insert(id, frame);
                }
                self.entries.insert(
                    id,
                    Shared::new(BodyEntry {
                        location: actor.location,
                        orientation: actor.orientation,
                        body: actor.body.clone(),
                        cells: cells.map(Shared::new),
                    }),
                );
            }
        }
    }
}

#[derive(Default)]
pub(crate) struct ActorStore {
    entries: Shared<BTreeMap<ActorId, Actor>>,
    regions: Members,
    held_owners: Members,
    timer_deadlines: Shared<BTreeSet<(u64, ActorId)>>,
    suspended_timers: Shared<BTreeSet<ActorId>>,
    contextual_recovery: Shared<BTreeSet<ActorId>>,
    active_falls: Shared<BTreeSet<ActorId>>,
    pending_searches: Shared<BTreeSet<ActorId>>,
    pending_relocations: Shared<BTreeSet<ActorId>>,
    pending_aggravation: Shared<BTreeSet<ActorId>>,
    guised_owners: Shared<BTreeSet<ActorId>>,
    perception_revision: u64,
    visibility_epoch: u64,
    bodies: Mutex<Bodies>,
}

impl Clone for ActorStore {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            regions: self.regions.clone(),
            held_owners: self.held_owners.clone(),
            timer_deadlines: self.timer_deadlines.clone(),
            suspended_timers: self.suspended_timers.clone(),
            contextual_recovery: self.contextual_recovery.clone(),
            active_falls: self.active_falls.clone(),
            pending_searches: self.pending_searches.clone(),
            pending_relocations: self.pending_relocations.clone(),
            pending_aggravation: self.pending_aggravation.clone(),
            guised_owners: self.guised_owners.clone(),
            perception_revision: self.perception_revision,
            visibility_epoch: self.visibility_epoch,
            bodies: Mutex::new(
                self.bodies
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .clone(),
            ),
        }
    }
}

impl std::fmt::Debug for ActorStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorStore")
            .field("entries", &self.entries)
            .finish()
    }
}

impl PartialEq for ActorStore {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}
impl Eq for ActorStore {}

impl Deref for ActorStore {
    type Target = BTreeMap<ActorId, Actor>;
    fn deref(&self) -> &Self::Target {
        &self.entries
    }
}

impl ActorStore {
    pub fn from_entries(entries: BTreeMap<ActorId, Actor>) -> Self {
        let mut regions = Members::default();
        let mut held_owners = Members::default();
        let mut timer_deadlines = BTreeSet::new();
        let mut pending_searches = BTreeSet::new();
        let mut pending_relocations = BTreeSet::new();
        let mut pending_aggravation = BTreeSet::new();
        let mut guised_owners = BTreeSet::new();
        for (&id, actor) in &entries {
            add_member(&mut regions, actor.location.region, id);
            if actor.guise.appearance().is_some() {
                guised_owners.insert(id);
            }
            if actor.awareness.pending() {
                pending_aggravation.insert(id);
            }
            if actor.periodic_relocations.pending() {
                pending_relocations.insert(id);
            }
            if actor.search_clock.pending() {
                pending_searches.insert(id);
            }
            if actor.holds.has_links() {
                add_member(&mut held_owners, actor.location.region, id);
            }
            if let Some(interval) = next_timer(actor) {
                timer_deadlines.insert((interval, id));
            }
        }
        let contextual_recovery = entries
            .iter()
            .filter_map(|(&id, actor)| watches_recovery(actor).then_some(id))
            .collect();
        let active_falls = entries
            .iter()
            .filter_map(|(&id, actor)| actor.motion.active_fall.map(|_| id))
            .collect();
        Self {
            entries: Shared::new(entries),
            regions,
            held_owners,
            timer_deadlines: Shared::new(timer_deadlines),
            suspended_timers: Shared::default(),
            contextual_recovery: Shared::new(contextual_recovery),
            active_falls: Shared::new(active_falls),
            pending_searches: Shared::new(pending_searches),
            pending_relocations: Shared::new(pending_relocations),
            pending_aggravation: Shared::new(pending_aggravation),
            guised_owners: Shared::new(guised_owners),
            perception_revision: fresh_perception_revision(),
            visibility_epoch: fresh_perception_revision(),
            bodies: Mutex::default(),
        }
    }

    #[cfg(test)]
    pub fn raw_entries(&self) -> &BTreeMap<ActorId, Actor> {
        &self.entries
    }

    pub fn scheduled_timers(&self) -> impl Iterator<Item = (u64, ActorId)> + '_ {
        self.timer_deadlines
            .iter()
            .copied()
            .inspect(|_| crate::diagnostics::timer_candidate())
    }

    /// Derived pending owners, ordered like ordinary timer settlement. This is
    /// rebuilt from authoritative actor clocks, never serialized or polled by UI.
    pub(crate) fn pending_searches(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.pending_searches.iter().copied()
    }

    pub(crate) fn pending_aggravation(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.pending_aggravation.iter().copied()
    }

    pub(crate) fn pending_relocations(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.pending_relocations.iter().copied()
    }

    /// Lifecycle-derived index state; never changes the actor's raw clock.
    pub fn set_timer_active(&mut self, id: ActorId, active: bool) {
        let Some(actor) = self.entries.get(&id) else {
            return;
        };
        let deadline = next_timer(actor);
        if active {
            if self.suspended_timers.remove(&id) {
                if !self.contextual_recovery.is_empty() {
                    self.perception_revision = fresh_perception_revision();
                }
                if let Some(deadline) = deadline {
                    self.timer_deadlines.insert((deadline, id));
                }
            }
        } else if self.suspended_timers.insert(id) {
            if !self.contextual_recovery.is_empty() {
                self.perception_revision = fresh_perception_revision();
            }
            if let Some(deadline) = deadline {
                self.timer_deadlines.remove(&(deadline, id));
            }
        }
    }

    pub fn contextual_recovery_owners(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.contextual_recovery.iter().copied()
    }

    /// Derived lookup over only actors carrying one unsettled causal episode.
    pub fn active_fall_owners(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.active_falls.iter().copied()
    }

    pub(crate) fn visibility_epoch(&self) -> u64 {
        self.visibility_epoch
    }

    pub fn perception_revision(&self) -> u64 {
        self.perception_revision
    }

    pub fn in_region(&self, region: RegionId) -> impl Iterator<Item = ActorId> + '_ {
        self.regions
            .get(&region)
            .into_iter()
            .flat_map(|members| members.iter().copied())
    }

    /// Sparse source index excludes protection-only books.
    pub(crate) fn held_owners_in(&self, region: RegionId) -> impl Iterator<Item = ActorId> + '_ {
        self.held_owners
            .get(&region)
            .into_iter()
            .flat_map(|owners| owners.iter().copied())
    }

    /// Includes every loaded actor's body, including corpses. Callers apply their
    /// existing life/freeze rules against authoritative entries after lookup.
    pub fn at(&self, world: &World, location: Location) -> Occupants {
        let mut bodies = self.bodies.lock().unwrap_or_else(|e| e.into_inner());
        bodies.refresh(&self.entries, world);
        bodies.cells.get(&location).cloned().unwrap_or_default()
    }

    /// Ordered identities and authored body-cell order. Query only visible
    /// regions, using the smaller of their occupied and visible cell sets.
    pub fn perceived(&self, world: &World, visible: &BTreeSet<Location>) -> PerceivedBodies {
        let mut bodies = self.bodies.lock().unwrap_or_else(|e| e.into_inner());
        bodies.refresh(&self.entries, world);
        let mut ids = BTreeSet::new();
        let mut previous_region = None;
        for at in visible {
            let region = at.region;
            if previous_region == Some(region) {
                continue;
            }
            previous_region = Some(region);
            let Some(occupied) = bodies.cells.region(region) else {
                continue;
            };
            let region_view = visible.range(
                Location {
                    region,
                    position: Position {
                        x: i32::MIN,
                        y: i32::MIN,
                        z: i32::MIN,
                    },
                }..=Location {
                    region,
                    position: Position {
                        x: i32::MAX,
                        y: i32::MAX,
                        z: i32::MAX,
                    },
                },
            );
            if region_view.clone().take(occupied.len()).count() == occupied.len() {
                for (_, occupants) in occupied.iter().filter(|(at, _)| visible.contains(at)) {
                    ids.extend(occupants.keys().copied());
                }
            } else {
                for occupants in region_view.filter_map(|at| occupied.get(at)) {
                    ids.extend(occupants.keys().copied());
                }
            }
        }
        crate::diagnostics::actor_candidates(ids.len());
        ids.into_iter()
            .filter_map(|id| Some((id, bodies.entries.get(&id)?.cells.clone()?)))
            .collect()
    }

    pub(crate) fn has_guises(&self) -> bool {
        !self.guised_owners.is_empty()
    }

    pub fn insert(&mut self, id: ActorId, actor: Actor) -> Option<Actor> {
        update_pending_search(
            &mut self.guised_owners,
            id,
            actor.guise.appearance().is_some(),
        );
        update_pending_search(&mut self.pending_searches, id, actor.search_clock.pending());
        update_pending_search(&mut self.pending_aggravation, id, actor.awareness.pending());
        update_pending_search(
            &mut self.pending_relocations,
            id,
            actor.periodic_relocations.pending(),
        );
        if self
            .entries
            .get(&id)
            .is_none_or(|old| VisibilityInputs::capture(old) != VisibilityInputs::capture(&actor))
        {
            self.visibility_epoch = fresh_perception_revision();
        }
        let changed = if self.contextual_recovery.is_empty() {
            // No retained witness can remain valid when the first owner arrives.
            // Ordinary no-owner mutations allocate no faction/body snapshot.
            watches_recovery(&actor)
        } else {
            self.entries
                .get(&id)
                .is_none_or(|old| !PerceptionInputs::capture(old).matches(&actor))
        };
        if changed {
            self.perception_revision = fresh_perception_revision();
        }
        if watches_recovery(&actor) {
            self.contextual_recovery.insert(id);
        } else {
            self.contextual_recovery.remove(&id);
        }
        if actor.motion.active_fall.is_some() {
            self.active_falls.insert(id);
        } else {
            self.active_falls.remove(&id);
        }
        let region = actor.location.region;
        let deadline = (!self.suspended_timers.contains(&id))
            .then(|| next_timer(&actor))
            .flatten();
        let held = actor.holds.has_links();
        let old = self.entries.insert(id, actor);
        if let Some(old) = &old {
            if let Some(interval) = next_timer(old) {
                self.timer_deadlines.remove(&(interval, id));
            }
            remove_member(&mut self.regions, old.location.region, id);
            if old.holds.has_links() {
                remove_member(&mut self.held_owners, old.location.region, id);
            }
        }
        if let Some(interval) = deadline {
            self.timer_deadlines.insert((interval, id));
        }
        add_member(&mut self.regions, region, id);
        if held {
            add_member(&mut self.held_owners, region, id);
        }
        self.bodies
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .dirty
            .insert(id);
        old
    }

    pub fn remove(&mut self, id: &ActorId) -> Option<Actor> {
        let old = self.entries.remove(id)?;
        self.visibility_epoch = fresh_perception_revision();
        if !self.contextual_recovery.is_empty() {
            self.perception_revision = fresh_perception_revision();
        }
        self.suspended_timers.remove(id);
        self.contextual_recovery.remove(id);
        self.active_falls.remove(id);
        self.pending_searches.remove(id);
        self.pending_relocations.remove(id);
        self.pending_aggravation.remove(id);
        self.guised_owners.remove(id);
        if let Some(interval) = next_timer(&old) {
            self.timer_deadlines.remove(&(interval, *id));
        }
        remove_member(&mut self.regions, old.location.region, *id);
        if old.holds.has_links() {
            remove_member(&mut self.held_owners, old.location.region, *id);
        }
        self.bodies
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .dirty
            .insert(*id);
        Some(old)
    }

    pub fn extend(&mut self, entries: impl IntoIterator<Item = (ActorId, Actor)>) {
        for (id, actor) in entries {
            self.insert(id, actor);
        }
    }

    pub fn get_mut(&mut self, id: &ActorId) -> Option<ActorEdit<'_>> {
        let before = std::ptr::from_ref(&*self.entries);
        let entries = &mut *self.entries;
        if std::ptr::from_ref(entries) != before {
            crate::diagnostics::actor_table_copy(entries.len());
        }
        let actor = entries.get_mut(id)?;
        let timer_active = !self.suspended_timers.contains(id);
        let before_deadline = timer_active.then(|| next_timer(actor)).flatten();
        self.bodies
            .get_mut()
            .unwrap_or_else(|e| e.into_inner())
            .dirty
            .insert(*id);
        Some(ActorEdit {
            id: *id,
            before: actor.location.region,
            before_held: actor.holds.has_links(),
            before_deadline,
            before_visibility: VisibilityInputs::capture(actor),
            visibility_epoch: &mut self.visibility_epoch,
            before_perception: (!self.contextual_recovery.is_empty())
                .then(|| PerceptionInputs::capture(actor)),
            perception_revision: &mut self.perception_revision,
            timer_active,
            actor,
            regions: &mut self.regions,
            held_owners: &mut self.held_owners,
            timer_deadlines: &mut self.timer_deadlines,
            contextual_recovery: &mut self.contextual_recovery,
            active_falls: &mut self.active_falls,
            pending_searches: &mut self.pending_searches,
            pending_relocations: &mut self.pending_relocations,
            pending_aggravation: &mut self.pending_aggravation,
            guised_owners: &mut self.guised_owners,
        })
    }
}

pub(crate) struct ActorEdit<'a> {
    id: ActorId,
    before: RegionId,
    before_held: bool,
    before_deadline: Option<u64>,
    before_visibility: VisibilityInputs,
    visibility_epoch: &'a mut u64,
    before_perception: Option<PerceptionInputs>,
    perception_revision: &'a mut u64,
    timer_active: bool,
    actor: &'a mut Actor,
    regions: &'a mut Members,
    held_owners: &'a mut Members,
    timer_deadlines: &'a mut Shared<BTreeSet<(u64, ActorId)>>,
    contextual_recovery: &'a mut Shared<BTreeSet<ActorId>>,
    active_falls: &'a mut Shared<BTreeSet<ActorId>>,
    pending_searches: &'a mut Shared<BTreeSet<ActorId>>,
    pending_relocations: &'a mut Shared<BTreeSet<ActorId>>,
    pending_aggravation: &'a mut Shared<BTreeSet<ActorId>>,
    guised_owners: &'a mut Shared<BTreeSet<ActorId>>,
}
impl Deref for ActorEdit<'_> {
    type Target = Actor;
    fn deref(&self) -> &Self::Target {
        self.actor
    }
}
impl DerefMut for ActorEdit<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.actor
    }
}
impl Drop for ActorEdit<'_> {
    fn drop(&mut self) {
        update_pending_search(
            self.guised_owners,
            self.id,
            self.actor.guise.appearance().is_some(),
        );
        update_pending_search(
            self.pending_searches,
            self.id,
            self.actor.search_clock.pending(),
        );
        update_pending_search(
            self.pending_aggravation,
            self.id,
            self.actor.awareness.pending(),
        );
        update_pending_search(
            self.pending_relocations,
            self.id,
            self.actor.periodic_relocations.pending(),
        );
        if self.before_visibility != VisibilityInputs::capture(self.actor) {
            *self.visibility_epoch = fresh_perception_revision();
        }
        let changed = self.before_perception.as_ref().map_or_else(
            || watches_recovery(self.actor),
            |before| !before.matches(self.actor),
        );
        if changed {
            *self.perception_revision = fresh_perception_revision();
        }
        if watches_recovery(self.actor) {
            self.contextual_recovery.insert(self.id);
        } else {
            self.contextual_recovery.remove(&self.id);
        }
        if self.actor.motion.active_fall.is_some() {
            self.active_falls.insert(self.id);
        } else {
            self.active_falls.remove(&self.id);
        }
        let deadline = self.timer_active.then(|| next_timer(self.actor)).flatten();
        if self.before_deadline != deadline {
            if let Some(interval) = self.before_deadline {
                self.timer_deadlines.remove(&(interval, self.id));
            }
            if let Some(interval) = deadline {
                self.timer_deadlines.insert((interval, self.id));
            }
        }
        let held = self.actor.holds.has_links();
        if self.before != self.actor.location.region || self.before_held != held {
            if self.before_held {
                remove_member(self.held_owners, self.before, self.id);
            }
            if held {
                add_member(self.held_owners, self.actor.location.region, self.id);
            }
        }
        if self.before != self.actor.location.region {
            remove_member(self.regions, self.before, self.id);
            add_member(self.regions, self.actor.location.region, self.id);
        }
    }
}

fn update_pending_search(index: &mut Shared<BTreeSet<ActorId>>, id: ActorId, pending: bool) {
    if pending != index.contains(&id) {
        if pending {
            index.insert(id);
        } else {
            index.remove(&id);
        }
    }
}

fn watches_recovery(actor: &Actor) -> bool {
    actor.alive()
        && actor
            .combat
            .as_ref()
            .is_some_and(|combat| combat.creature().derived().contextual_health_recovery())
}

#[cfg(test)]
fn needs_time(actor: &Actor) -> bool {
    next_timer(actor).is_some()
}

fn next_timer(actor: &Actor) -> Option<u64> {
    let creature = actor
        .combat
        .as_ref()
        .map(|combat| combat.creature())
        .and_then(|creature| creature.next_change_in())
        .and_then(|interval| actor.active_since.checked_add(interval));
    creature
        .into_iter()
        .chain(
            crate::hallucination_clock::next_in(actor)
                .and_then(|interval| actor.active_since.checked_add(interval)),
        )
        .chain(
            actor
                .awareness
                .next_in(actor)
                .and_then(|interval| actor.active_since.checked_add(interval)),
        )
        .chain(
            actor
                .search_clock
                .next_in(actor)
                .and_then(|interval| actor.active_since.checked_add(interval)),
        )
        .chain(
            actor
                .periodic_relocations
                .next_in(actor.alive())
                .and_then(|interval| actor.active_since.checked_add(interval)),
        )
        .chain(
            actor
                .holds
                .next_change_in()
                .and_then(|interval| actor.active_since.checked_add(interval)),
        )
        .chain(
            actor
                .engagement_deadline()
                .filter(|deadline| *deadline > actor.active_since),
        )
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Game;
    use std::num::NonZeroU64;
    use tor_world::{Direction, Extent, Passage, Position, Region};

    fn timed_ids(actors: &ActorStore) -> Vec<ActorId> {
        let mut ids: Vec<_> = actors.scheduled_timers().map(|(_, id)| id).collect();
        ids.sort();
        ids
    }

    fn at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        }
    }

    fn game() -> Game {
        let mut world = World::new(vec![], vec![]).unwrap();
        for region in [1, 2] {
            world
                .add_region(Region {
                    id: RegionId(region),
                    name: format!("room-{region}"),
                    bounds: Extent::new(16, 16, 2).unwrap(),
                })
                .unwrap();
        }
        let mut game = Game::new(
            world,
            42,
            crate::creatures::Content::new(crate::test_support::test_ruleset(), Default::default())
                .unwrap(),
        );
        game.spawn_actor(at(1, 15), NonZeroU64::new(100).unwrap())
            .unwrap();
        game
    }

    #[test]
    fn pending_search_index_follows_actor_publication_edit_clone_rebuild_and_removal() {
        let mut game = game();
        let id = ActorId(1);
        let check = crate::discovery::FrozenDiscoveryCheck { bonus: 0, edge: 0 };
        assert!(game.actors.pending_searches().next().is_none());
        game.actors
            .get_mut(&id)
            .unwrap()
            .search_clock
            .advance(100, check)
            .unwrap();
        assert_eq!(game.actors.pending_searches().collect::<Vec<_>>(), vec![id]);
        let mut branch = game.actors.clone();
        assert!(branch
            .get_mut(&id)
            .unwrap()
            .search_clock
            .take_pending()
            .is_some());
        assert!(branch.pending_searches().next().is_none());
        assert_eq!(game.actors.pending_searches().collect::<Vec<_>>(), vec![id]);
        let rebuilt = ActorStore::from_entries(game.actors.raw_entries().clone());
        assert_eq!(rebuilt.pending_searches().collect::<Vec<_>>(), vec![id]);
        game.actors.insert(id, branch[&id].clone());
        assert!(game.actors.pending_searches().next().is_none());
        game.actors.insert(id, rebuilt[&id].clone());
        assert_eq!(game.actors.pending_searches().collect::<Vec<_>>(), vec![id]);
        game.actors.remove(&id).unwrap();
        assert!(game.actors.pending_searches().next().is_none());
    }

    fn assert_reference(game: &Game) {
        let mut deadlines: Vec<_> = game
            .actors
            .iter()
            .filter_map(|(&id, actor)| {
                let interval = actor.combat.as_ref()?.creature().next_change_in()?;
                Some((actor.active_since.checked_add(interval)?, id))
            })
            .collect();
        deadlines.sort();
        assert_eq!(
            game.actors.scheduled_timers().collect::<Vec<_>>(),
            deadlines
        );
        let timed: Vec<_> = game
            .actors
            .iter()
            .filter_map(|(&id, actor)| needs_time(actor).then_some(id))
            .collect();
        assert_eq!(timed_ids(&game.actors), timed);
        for region in [1, 2] {
            let members: Vec<_> = game
                .actors
                .iter()
                .filter(|(_, a)| a.location.region == RegionId(region))
                .map(|(&id, _)| id)
                .collect();
            assert_eq!(
                game.actors.in_region(RegionId(region)).collect::<Vec<_>>(),
                members
            );
            for x in 0..16 {
                let location = at(region, x);
                let expected: BTreeMap<_, _> = game
                    .actors
                    .iter()
                    .filter_map(|(&id, actor)| {
                        crate::physics::resolve_body(
                            &game.world,
                            actor.location,
                            actor.orientation,
                            &actor.body,
                        )?
                        .into_iter()
                        .find(|(cell, _)| *cell == location)
                        .map(|(_, frame)| (id, frame))
                    })
                    .collect();
                assert_eq!(&*game.actors.at(&game.world, location), &expected);
                let visible = BTreeSet::from([location]);
                let perceived = game.actors.perceived(&game.world, &visible);
                assert_eq!(
                    perceived.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
                    expected.keys().copied().collect::<Vec<_>>()
                );
                for (id, cells) in perceived {
                    let actor = &game.actors[&id];
                    assert_eq!(
                        &*cells,
                        &crate::physics::resolve_body(
                            &game.world,
                            actor.location,
                            actor.orientation,
                            &actor.body
                        )
                        .unwrap()
                    );
                }
            }
        }
    }

    #[test]
    fn timed_index_tracks_edits_death_clones_removal_insertion_and_reconstruction() {
        use crate::attributes::ManaBinding;
        use crate::content_ids::{ClassId, TemplateId};
        use crate::creatures::{CreatureBuild, Template};
        use crate::progression::{HdLedger, HdSource};
        use crate::rules::{Modifier, ModifierOp, Scope, Stat};
        let mut game = game();
        let source = game
            .spawn_actor(at(2, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let species = crate::test_creatures::unmodified_species("index_subject");
        let mut template = Template::new(TemplateId::new("capacity").unwrap(), 0);
        template.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Add {
                stat: Stat::ResourceMax {
                    resource: crate::resources::Resource::Stamina,
                },
                value: 2,
            },
        });
        let id = species.id.clone();
        let content = crate::creatures::Content::new(
            crate::test_support::test_ruleset(),
            crate::creatures::ScenarioCatalog {
                species: BTreeMap::from([(id.clone(), species)]),
                templates: BTreeMap::from([(template.id.clone(), template)]),
            },
        )
        .unwrap();
        let build = CreatureBuild::new(
            content,
            id,
            HdLedger::seeded(vec![HdSource::Class(ClassId::new("mage").unwrap())], 7).unwrap(),
            ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        game.content = build.content().clone();
        game.configure_creature(
            source,
            crate::CreatureIdentity {
                name: "source".into(),
                faction: "neutral".into(),
            },
            build.clone(),
        )
        .unwrap();
        game.configure_creature(
            ActorId(1),
            crate::CreatureIdentity {
                name: "subject".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
        assert!(game.actors.scheduled_timers().next().is_none());
        game.apply_condition(
            ActorId(1),
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            source,
            300,
        )
        .unwrap();
        assert_reference(&game);
        let retained = game.actors.clone();
        game.tick += 300;
        game.settle_creature_time().unwrap();
        assert!(
            std::ptr::eq(
                game.creature(source).unwrap().as_ref(),
                retained
                    .get(&source)
                    .unwrap()
                    .combat
                    .as_ref()
                    .unwrap()
                    .creature(),
            ),
            "updating one timer copied an unrelated creature state"
        );
        assert!(game.actors.scheduled_timers().next().is_none());
        assert_eq!(timed_ids(&retained), vec![ActorId(1)]);
        let mut reconstructed = ActorStore::from_entries((*retained.entries).clone());
        let actor = reconstructed.remove(&ActorId(1)).unwrap();
        assert!(reconstructed.scheduled_timers().next().is_none());
        reconstructed.insert(ActorId(1), actor);
        assert_eq!(timed_ids(&reconstructed), vec![ActorId(1)]);
        let mut build = game.creature(ActorId(1)).unwrap().build().clone();
        build
            .set_templates(vec![TemplateId::new("capacity").unwrap()])
            .unwrap();
        game.rebuild_creature(ActorId(1), build).unwrap();
        assert_eq!(timed_ids(&game.actors), vec![ActorId(1)]);
        game.tick += 200;
        game.settle_creature_time().unwrap();
        assert!(game.actors.scheduled_timers().next().is_none());
        game.apply_condition(
            ActorId(1),
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            source,
            300,
        )
        .unwrap();
        game.apply_condition(
            source,
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(1),
            100,
        )
        .unwrap();
        assert_eq!(
            game.actors.scheduled_timers().collect::<Vec<_>>(),
            vec![(game.tick + 100, source), (game.tick + 300, ActorId(1))]
        );
        game.apply_condition(
            source,
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(1),
            300,
        )
        .unwrap();
        assert_eq!(
            game.actors.scheduled_timers().collect::<Vec<_>>(),
            vec![(game.tick + 300, ActorId(1)), (game.tick + 300, source)]
        );
        game.apply_condition(
            source,
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            ActorId(1),
            400,
        )
        .unwrap();
        assert_eq!(
            game.actors.scheduled_timers().collect::<Vec<_>>(),
            vec![(game.tick + 300, ActorId(1)), (game.tick + 400, source)]
        );
        assert_reference(&game);
        game.tick += 400;
        game.settle_creature_time().unwrap();
        assert!(game.actors.scheduled_timers().next().is_none());
        game.apply_condition(
            ActorId(1),
            &crate::content_ids::ConditionId::new("fear").unwrap(),
            source,
            300,
        )
        .unwrap();
        crate::test_creatures::injure(&mut game, ActorId(1), 100).unwrap();
        assert!(game.actors.scheduled_timers().next().is_none());
        assert_reference(&game);
    }

    #[test]
    fn bodies_follow_portal_frames_geometry_edits_and_mutations() {
        let mut game = game();
        // Exercise a cached unresolved body before the portal exists.
        game.actors
            .get_mut(&ActorId(1))
            .unwrap()
            .body
            .cells
            .push([1, 0, 0]);
        assert_reference(&game);
        assert!(game.actors.at(&game.world, at(1, 15)).is_empty());
        game.connect(
            Passage {
                from: at(1, 15),
                direction: Direction::East,
                to: at(2, 0),
            },
            1,
        )
        .unwrap();
        assert_reference(&game);
        let portal_frame = game.actors.at(&game.world, at(2, 0))[&ActorId(1)];
        assert_ne!(portal_frame, 0);
        assert!(game.actors.in_region(RegionId(2)).next().is_none());
        let old = game.clone();
        game.teleport(ActorId(1), at(2, 4)).unwrap();
        assert_reference(&game);
        assert_reference(&old);
        assert_eq!(
            old.actors.at(&old.world, at(2, 0))[&ActorId(1)],
            portal_frame
        );
        assert!(game.actors.at(&game.world, at(2, 0)).is_empty());
        game.set_body(ActorId(1), BodySpec::default()).unwrap();
        assert_reference(&game);
        let entries = (*game.actors.entries).clone();
        game.actors = ActorStore::from_entries(entries);
        assert_reference(&game);
        game.actors.remove(&ActorId(1)).unwrap();
        assert_reference(&game);
    }

    #[test]
    fn interrupted_edits_still_update_region_membership_and_body_queries() {
        let mut game = game();
        assert_reference(&game);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut actor = game.actors.get_mut(&ActorId(1)).unwrap();
            actor.location = at(2, 5);
            panic!("interrupted edit");
        }));
        assert!(result.is_err());
        assert_reference(&game);
        assert!(game.actors.in_region(RegionId(1)).next().is_none());
        assert_eq!(
            game.actors.in_region(RegionId(2)).collect::<Vec<_>>(),
            vec![ActorId(1)]
        );
    }

    #[test]
    fn disclosure_preserves_identity_and_authored_cell_order_for_partial_views() {
        let mut game = game();
        game.connect(
            Passage {
                from: at(1, 15),
                direction: Direction::East,
                to: at(2, 0),
            },
            1,
        )
        .unwrap();
        game.set_body(
            ActorId(1),
            BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0]],
                ..BodySpec::default()
            },
        )
        .unwrap();
        let other = game
            .spawn_actor(at(2, 7), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.set_body(
            other,
            BodySpec {
                cells: vec![[0, 0, 0], [1, 0, 0], [0, 1, 0]],
                ..BodySpec::default()
            },
        )
        .unwrap();
        let samples = [at(1, 15), at(2, 0), at(2, 7), at(2, 8), at(1, 0)];
        for mask in 0..1 << samples.len() {
            let visible: BTreeSet<_> = samples
                .iter()
                .enumerate()
                .filter(|(bit, _)| mask & (1 << bit) != 0)
                .map(|(_, &at)| at)
                .collect();
            let expected: Vec<_> = game
                .actors
                .iter()
                .filter_map(|(&id, actor)| {
                    let cells = crate::physics::resolve_body(
                        &game.world,
                        actor.location,
                        actor.orientation,
                        &actor.body,
                    )?;
                    cells
                        .iter()
                        .any(|(at, _)| visible.contains(at))
                        .then_some((id, cells))
                })
                .collect();
            let actual: Vec<_> = game
                .actors
                .perceived(&game.world, &visible)
                .into_iter()
                .map(|(id, cells)| (id, (*cells).clone()))
                .collect();
            assert_eq!(actual, expected, "mask: {mask}");
        }
    }
}
