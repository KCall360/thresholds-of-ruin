//! Deterministic rigid cell bodies. Motion is expressed in the persistent body
//! frame, so portal transitions transform vectors without rounding or facing.
use crate::{ActorId, Game, GameError, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use tor_world::{compose_rotation, inverse_rotation, rotate_vector, Direction, Location, RegionId};

pub const CELL: i64 = 65_536;
pub const ACCELERATION: i64 = 16;
pub const TERMINAL: i64 = 8_192;

/// Resolve all paths through portal topology and retain each cell's body frame.
/// Both uncached rules and the derived occupancy index use this same resolver.
pub(crate) fn resolve_body(
    world: &tor_world::World,
    at: Location,
    frame: u8,
    body: &BodySpec,
) -> Option<Vec<(Location, u8)>> {
    crate::diagnostics::body_cells(body.cells.len());
    if body.cells.as_slice() == [[0; 3]] {
        return Some(vec![(at, frame)]);
    }
    let mut cache = BTreeMap::from([([0; 3], Some((at, frame)))]);
    fn resolve(
        world: &tor_world::World,
        offset: [i32; 3],
        cache: &mut BTreeMap<[i32; 3], Option<(Location, u8)>>,
    ) -> Option<(Location, u8)> {
        if let Some(v) = cache.get(&offset) {
            return *v;
        }
        let mut result = None;
        for axis in 0..3 {
            if offset[axis] == 0 {
                continue;
            }
            let mut prev = offset;
            prev[axis] -= offset[axis].signum();
            let (from, frame) = resolve(world, prev, cache)?;
            let mut delta = [0; 3];
            delta[axis] = i64::from(offset[axis].signum());
            let dir = Direction::from_delta(rotate_vector(frame, delta))?;
            let (to, turn) = world.physics_neighbor(from, dir)?;
            let candidate = (to, compose_rotation(frame, turn));
            if result.is_some_and(|old| old != candidate) {
                cache.insert(offset, None);
                return None;
            }
            result = Some(candidate);
        }
        cache.insert(offset, result);
        result
    }
    let cells = body
        .cells
        .iter()
        .map(|offset| resolve(world, *offset, &mut cache))
        .collect::<Option<Vec<_>>>()?;
    (cells
        .iter()
        .map(|(at, _)| at)
        .collect::<BTreeSet<_>>()
        .len()
        == cells.len())
    .then_some(cells)
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(not(test), derive(Clone))]
#[serde(deny_unknown_fields)]
pub struct BodySpec {
    pub cells: Vec<[i32; 3]>,
    /// The body cell whose centre every sight line starts from. Required in
    /// every declaration; see `docs/architecture/perception.md`.
    pub eye: [i32; 3],
    pub mass: u32,
}

#[cfg(test)]
thread_local! {
    // Count actual definition copies without changing production diagnostics.
    static BODY_DEFINITION_COPIES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Clone for BodySpec {
    fn clone(&self) -> Self {
        BODY_DEFINITION_COPIES.with(|copies| copies.set(copies.get() + 1));
        Self {
            cells: self.cells.clone(),
            eye: self.eye,
            mass: self.mass,
        }
    }
}
impl Default for BodySpec {
    /// The implicit single-cell body of an actor that declares none.
    fn default() -> Self {
        Self {
            cells: vec![[0; 3]],
            eye: [0; 3],
            mass: 80,
        }
    }
}
impl BodySpec {
    pub(crate) fn valid(&self) -> bool {
        !self.cells.is_empty()
            && self.cells.len() <= 64
            && self.mass > 0
            && self.cells.contains(&[0; 3])
            && self.cells.contains(&self.eye)
            && self
                .cells
                .iter()
                .all(|c| c.iter().all(|v| (-8..=8).contains(v)))
            && self.cells.iter().collect::<BTreeSet<_>>().len() == self.cells.len()
    }
}
/// One bounded causal episode, private to the actor's unsettled motion.
/// This is historical provenance, never a timer or a contributor history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ActiveFall {
    pub owner: ActorId,
    pub episode: NonZeroU64,
    pub started_at: u64,
}
impl<'de> Deserialize<'de> for ActiveFall {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            owner: ActorId,
            episode: NonZeroU64,
            started_at: u64,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        if fields.owner.0 == 0 {
            return Err(serde::de::Error::custom("fall owner must be nonzero"));
        }
        Ok(Self {
            owner: fields.owner,
            episode: fields.episode,
            started_at: fields.started_at,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MotionState {
    /// Fixed-point cells per tick, in the persistent body frame.
    pub velocity: [i64; 3],
    pub displacement: [i64; 3],
    pub acceleration_remainder: [i64; 3],
    pub(crate) active_fall: Option<ActiveFall>,
}
impl<'de> Deserialize<'de> for MotionState {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            velocity: [i64; 3],
            displacement: [i64; 3],
            acceleration_remainder: [i64; 3],
            #[serde(deserialize_with = "crate::bounded::required_option")]
            active_fall: Option<ActiveFall>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            velocity: fields.velocity,
            displacement: fields.displacement,
            acceleration_remainder: fields.acceleration_remainder,
            active_fall: fields.active_fall,
        })
    }
}
impl MotionState {
    /// Items inherit kinematics, never actor-owned fatal provenance.
    pub(crate) fn without_fall(&self) -> Self {
        Self {
            active_fall: None,
            ..self.clone()
        }
    }
    pub(crate) fn same_kinematics(&self, other: &Self) -> bool {
        self.velocity == other.velocity
            && self.displacement == other.displacement
            && self.acceleration_remainder == other.acceleration_remainder
    }
    pub(crate) fn valid(&self) -> bool {
        self.velocity
            .iter()
            .all(|v| v.unsigned_abs() <= TERMINAL as u64)
            && self
                .velocity
                .iter()
                .map(|v| i128::from(*v).pow(2))
                .sum::<i128>()
                <= i128::from(TERMINAL).pow(2)
            && self
                .displacement
                .iter()
                .all(|v| v.unsigned_abs() < CELL as u64)
            && self
                .acceleration_remainder
                .iter()
                .all(|v| v.unsigned_abs() < 64)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PhysicsEntity {
    Actor(ActorId),
    Item(ItemId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Impact {
    pub entity: PhysicsEntity,
    pub at_tick: u64,
    pub location: Location,
    pub axis: usize,
    /// Contact normal in the reference cell's region coordinates.
    pub normal: [i64; 3],
    pub other_actor: Option<ActorId>,
    pub incoming_velocity: i64,
    pub mass: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Physics {
    pub impacts: Vec<Impact>,
    pub displaced: BTreeSet<ActorId>,
    pub next_fall_episode: NonZeroU64,
}
impl Default for Physics {
    fn default() -> Self {
        Self {
            impacts: Vec::new(),
            displaced: BTreeSet::new(),
            next_fall_episode: NonZeroU64::MIN,
        }
    }
}
impl<'de> Deserialize<'de> for Physics {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            impacts: Vec<Impact>,
            displaced: BTreeSet<ActorId>,
            next_fall_episode: NonZeroU64,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            impacts: fields.impacts,
            displaced: fields.displaced,
            next_fall_episode: fields.next_fall_episode,
        })
    }
}

impl Game {
    pub(crate) fn physics_valid(&self) -> bool {
        let mut episodes = BTreeSet::new();
        self.actors.active_fall_owners().all(|id| {
            let actor = &self.actors[&id];
            actor.motion.active_fall.is_some_and(|fall| {
                actor.alive()
                    && fall.owner.0 > 0
                    && fall.owner.0 < self.next_actor_id
                    && fall.episode < self.physics.next_fall_episode
                    && fall.started_at <= self.tick
                    && episodes.insert(fall.episode)
            })
        }) && self
            .physics
            .displaced
            .iter()
            .all(|id| self.actors.contains_key(id))
            && self.physics.impacts.iter().all(|e| {
                e.axis < 3
                    && e.at_tick <= self.tick
                    && self.world.contains(e.location)
                    && e.mass > 0
                    && e.incoming_velocity != 0
                    && e.incoming_velocity.unsigned_abs() <= TERMINAL as u64
                    && e.normal.iter().all(|v| (-1..=1).contains(v))
                    && e.normal.iter().map(|v| v.unsigned_abs()).sum::<u64>() == 1
                    && e.other_actor
                        .is_none_or(|id| self.actors.contains_key(&id) || self.detached_actor(id))
                    && match e.entity {
                        PhysicsEntity::Actor(id) => self.actors.contains_key(&id),
                        PhysicsEntity::Item(id) => self.items.contains_key(&id),
                    }
            })
    }
    pub fn set_gravity(&mut self, region: RegionId, vector: [i32; 3]) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .set_gravity(region, vector)
                .map_err(|_| GameError::InvalidLocation)
        })
    }
    pub fn set_cell_gravity(&mut self, at: Location, vector: [i32; 3]) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .set_cell_gravity(at, vector)
                .map_err(|_| GameError::InvalidLocation)
        })
    }
    pub fn set_body(&mut self, id: ActorId, body: BodySpec) -> Result<(), GameError> {
        self.validate_body_change(id, &body)?;
        self.with_recovery_transition(false, |candidate| candidate.replace_body(id, body))
    }
    /// Birth construction already owns rollback and event publication.
    pub(crate) fn set_body_in_transaction(
        &mut self,
        id: ActorId,
        body: BodySpec,
    ) -> Result<(), GameError> {
        self.validate_body_change(id, &body)?;
        self.replace_body(id, body)?;
        self.retire_settled_falls();
        self.reconcile_recovery_contexts()
    }
    fn validate_body_change(&self, id: ActorId, body: &BodySpec) -> Result<(), GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if !body.valid() || !self.body_fits(id, actor.location, actor.orientation, body) {
            return Err(GameError::Blocked);
        }
        Ok(())
    }
    fn replace_body(&mut self, id: ActorId, body: BodySpec) -> Result<(), GameError> {
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        if *actor.body != body {
            actor.readied_ambush.retire();
        }
        actor.body = tor_world::Shared::new(body);
        actor.motion.acceleration_remainder = [0; 3];
        actor.motion.active_fall = None;
        drop(actor);
        self.reconcile_holds_for_actor(id)
    }
    fn body_has_field(&self, at: Location, frame: u8, body: &BodySpec) -> bool {
        self.body_cells(at, frame, body).is_some_and(|cells| {
            cells
                .iter()
                .any(|(at, _)| self.world.gravity(*at).is_some())
        })
    }
    pub(crate) fn motion_view_active(&self, id: ActorId) -> bool {
        let a = &self.actors[&id];
        a.body.cells.len() > 1
            || a.motion.velocity != [0; 3]
            || self.effective_gravity(PhysicsEntity::Actor(id), a.location, a.orientation, &a.body)
                != [0; 3]
            || self.physics.displaced.contains(&id)
            || self
                .physics
                .impacts
                .iter()
                .any(|e| e.entity == PhysicsEntity::Actor(id))
    }
    pub fn actor_motion(&self, id: ActorId) -> Option<&MotionState> {
        self.actors.get(&id).map(|a| &a.motion)
    }
    pub fn physics_enabled(&self) -> bool {
        self.world.has_gravity()
            || !self.physics.displaced.is_empty()
            || !self.physics.impacts.is_empty()
            || self.actors.values().any(|a| a.motion.velocity != [0; 3])
            || self.items.values().any(|i| i.motion.velocity != [0; 3])
    }
    pub fn physics_impacts(&self) -> &[Impact] {
        &self.physics.impacts
    }
    /// A wait may only hand control to another actor at this timestamp. Resting
    /// worlds also need no new scene, but clearing prior sensations is observable.
    pub fn wait_changes_perception(&self, id: ActorId) -> bool {
        if !self.physics.displaced.is_empty() || !self.physics.impacts.is_empty() {
            return true;
        }
        if !self.physics_enabled()
            || self
                .actors
                .iter()
                .any(|(other, a)| *other != id && a.ready_at <= self.tick)
        {
            return false;
        }
        self.actors.iter().any(|(id, a)| {
            self.needs_integration(
                PhysicsEntity::Actor(*id),
                a.location,
                a.orientation,
                &a.body,
                &a.motion,
            )
        }) || self.items.iter().any(|(id, i)| match i.location {
            ItemLocation::Ground(at) => self.needs_integration(
                PhysicsEntity::Item(*id),
                at,
                i.orientation,
                &BodySpec::default(),
                &i.motion,
            ),
            ItemLocation::Carried(_) => false,
        })
    }
    pub fn set_actor_velocity(&mut self, id: ActorId, velocity: [i64; 3]) -> Result<(), GameError> {
        if velocity.iter().any(|v| v.unsigned_abs() > TERMINAL as u64) {
            return Err(GameError::InvalidLocation);
        }
        let mut velocity = velocity;
        cap_velocity(&mut velocity);
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        if actor.motion.velocity != velocity {
            actor.motion.velocity = velocity;
            actor.motion.active_fall = None;
        }
        Ok(())
    }
    pub fn connect_portal_area(
        &mut self,
        passage: tor_world::Passage,
        rotation: u8,
        width: u16,
        height: u16,
    ) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .connect_portal_area(passage, rotation, width, height)
                .map_err(|_| GameError::InvalidLocation)
        })
    }

    /// Resolve every monotone route to a body offset. Inconsistent transforms,
    /// incomplete apertures, and self-overlap make a placement invalid.
    pub(crate) fn body_cells(
        &self,
        at: Location,
        frame: u8,
        body: &BodySpec,
    ) -> Option<Vec<(Location, u8)>> {
        resolve_body(&self.world, at, frame, body)
    }
    pub(crate) fn body_fits(&self, id: ActorId, at: Location, frame: u8, body: &BodySpec) -> bool {
        self.body_cells(at, frame, body).is_some_and(|cells| {
            cells.iter().all(|(at, _)| {
                self.world.walkable(*at)
                    && !self
                        .actors
                        .at(&self.world, *at)
                        .keys()
                        .any(|other| *other != id && self.actors[other].alive())
            })
        })
    }

    /// Geometry-only landing test for a fully materialized load-only floor.
    /// Region activity never changes the distribution of legal destinations.
    pub(crate) fn stable_landing(
        &self,
        id: ActorId,
        at: Location,
        frame: u8,
        body: &BodySpec,
    ) -> bool {
        if !self.body_fits(id, at, frame, body) {
            return false;
        }
        let entity = PhysicsEntity::Actor(id);
        let gravity = self.gravity_sum(at, frame, body);
        (0..3).all(|axis| {
            gravity[axis] == 0 || self.gravity_axis_blocked(entity, at, frame, body, gravity, axis)
        })
    }
    pub(crate) fn translate_body(
        &self,
        entity: PhysicsEntity,
        at: Location,
        frame: u8,
        body: &BodySpec,
        delta: [i64; 3],
    ) -> Option<(Location, u8)> {
        // Simultaneous diagonal translation requires every component ordering to
        // agree and stay clear. Failed combinations are resolved by sliding.
        let axes: Vec<_> = (0..3).filter(|a| delta[*a] != 0).collect();
        if axes.len() > 1 {
            let mut result = None;
            for first in &axes {
                let mut one = [0; 3];
                one[*first] = delta[*first];
                let (middle, mframe) = self.translate_body(entity, at, frame, body, one)?;
                let mut rest = delta;
                rest[*first] = 0;
                let candidate = self.translate_body(entity, middle, mframe, body, rest)?;
                if result.is_some_and(|old| old != candidate) {
                    return None;
                }
                result = Some(candidate);
            }
            return result;
        }
        let dir = Direction::from_delta(rotate_vector(frame, delta))?;
        let (to, turn) = self.world.physics_neighbor(at, dir)?;
        let next_frame = compose_rotation(frame, turn);
        let old = self.body_cells(at, frame, body)?;
        let next = self.body_cells(to, next_frame, body)?;
        for ((old, old_frame), (new, new_frame)) in old.iter().zip(&next) {
            let dir = Direction::from_delta(rotate_vector(*old_frame, delta))?;
            let (mapped, rotation) = self.world.physics_neighbor(*old, dir)?;
            if mapped != *new
                || compose_rotation(*old_frame, rotation) != *new_frame
                || !self.world.walkable(*new)
            {
                return None;
            }
        }
        let excluded = match entity {
            PhysicsEntity::Actor(id) => id,
            PhysicsEntity::Item(_) => ActorId(0),
        };
        // Loose items may share cells with actors, like existing pickup/drop.
        if matches!(entity, PhysicsEntity::Actor(_))
            && !self.body_fits(excluded, to, next_frame, body)
        {
            return None;
        }
        Some((to, next_frame))
    }
    /// Voluntary movement checks each resolved cardinal leg in its region's
    /// coordinates, including actor-local confusion samples. Imposed motion
    /// deliberately continues to use translate_body.
    fn voluntary_leg_allowed(
        &self,
        id: ActorId,
        from: Location,
        direction: Direction,
        flying: bool,
        allow_stairs: bool,
    ) -> bool {
        if self.world.is_stair(from, direction) {
            return allow_stairs && self.can_use_abstract_stairs(id);
        }
        !matches!(direction, Direction::Up | Direction::Down)
            || flying
            || self.world.passage(from, direction).is_some()
    }

    fn voluntary_reach(
        &self,
        id: ActorId,
        from: Location,
        direction: Direction,
        flying: bool,
        allow_stairs: bool,
    ) -> Option<(Location, u8)> {
        let Some((a, b)) = direction.components() else {
            if !self.voluntary_leg_allowed(id, from, direction, flying, allow_stairs) {
                return None;
            }
            return self.reach(from, direction);
        };
        // The same one-clear-side rule as World::diagonal_reach, with the
        // actor's movement permissions applied before either leg is admitted.
        let route = |first, second: Direction| {
            if !self.voluntary_leg_allowed(id, from, first, flying, allow_stairs) {
                return None;
            }
            let (side, r1) = self.world.movement_neighbor(from, first)?;
            // A multi-cell contact may pass through the mover's own footprint.
            // Other living actors still obstruct this ordering of the route.
            if !self.world.walkable(side)
                || self
                    .actors
                    .at(&self.world, side)
                    .keys()
                    .any(|other| *other != id && self.actors[other].alive())
            {
                return None;
            }
            let second = second.rotated(r1);
            if !self.voluntary_leg_allowed(id, side, second, flying, allow_stairs) {
                return None;
            }
            let (to, r2) = self.world.movement_neighbor(side, second)?;
            (!self.world.is_wall(to)).then_some((to, compose_rotation(r1, r2)))
        };
        match (route(a, b), route(b, a)) {
            (Some(a), Some(b)) if a == b => Some(a),
            (Some(a), None) | (None, Some(a)) => Some(a),
            _ => None,
        }
    }

    pub(crate) fn actor_translation(
        &self,
        id: ActorId,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        self.actor_translation_with_stairs(id, direction, true)
    }

    /// Confused samples are physical geometry, never an implicit stair choice.
    pub(crate) fn actor_physical_translation(
        &self,
        id: ActorId,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        self.actor_translation_with_stairs(id, direction, false)
    }

    /// An obstructed physical step may touch an authored guise without entering
    /// its occupied cells. This is bounded by the mover's body, never a scene scan.
    pub(crate) fn movement_guise_contacts(
        &self,
        id: ActorId,
        direction: Direction,
    ) -> Option<((Location, u8), Vec<ActorId>)> {
        let actor = self.actors.get(&id)?;
        let locomotion = actor.locomotion();
        let local = direction.rotated(actor.orientation);
        if locomotion.stationary
            || self.world.is_stair(actor.location, local)
            || self
                .voluntary_reach(id, actor.location, local, locomotion.flying, false)
                .is_none()
            || (self.body_has_field(actor.location, actor.orientation, &actor.body)
                && !locomotion.hovers()
                && !self.supported(id))
        {
            return None;
        }
        let (x, y, z) = direction.delta();
        let delta = [i64::from(x), i64::from(y), i64::from(z)];
        let (destination, rotation) =
            self.melee_neighbor_frame(actor.location, rotate_vector(actor.orientation, delta))?;
        let mut contacts = std::collections::BTreeSet::new();
        for (from, frame) in self.body_cells(actor.location, actor.orientation, &actor.body)? {
            if let Some((to, _)) = self.melee_neighbor_frame(from, rotate_vector(frame, delta)) {
                for other in self.actors.at(&self.world, to).keys() {
                    if *other != id
                        && self.actors[other].alive()
                        && self.actors[other].guise.appearance().is_some()
                    {
                        contacts.insert(*other);
                    }
                }
            }
        }
        (!contacts.is_empty()).then(|| {
            (
                (destination, compose_rotation(actor.orientation, rotation)),
                contacts.into_iter().collect(),
            )
        })
    }
    fn actor_translation_with_stairs(
        &self,
        id: ActorId,
        direction: Direction,
        allow_stairs: bool,
    ) -> Option<(Location, u8)> {
        let a = &self.actors[&id];
        let locomotion = a.locomotion();
        if locomotion.stationary {
            return None;
        }
        let local = direction.rotated(a.orientation);
        if !self.voluntary_leg_allowed(id, a.location, local, locomotion.flying, allow_stairs) {
            return None;
        }
        if self.world.is_stair(a.location, local) {
            let (to, r) = self.reach(a.location, local)?;
            let frame = compose_rotation(a.orientation, r);
            return self
                .body_fits(id, to, frame, &a.body)
                .then_some((to, frame));
        }
        let physical = self.body_has_field(a.location, a.orientation, &a.body);
        if physical && !locomotion.hovers() && !self.supported(id) {
            return None;
        }
        let (x, y, z) = direction.delta();
        if a.body.cells.len() == 1 && !physical {
            // Retain established beveled-corner walking for diagnostic fixtures.
            let (to, r) =
                self.voluntary_reach(id, a.location, local, locomotion.flying, allow_stairs)?;
            let frame = compose_rotation(a.orientation, r);
            return self
                .body_fits(id, to, frame, &a.body)
                .then_some((to, frame));
        }
        // A walking diagonal needs one clear side, as a single cell's does:
        // the body passes by either ordering of the two steps, and where both
        // are open they must arrive at the same place.
        if x != 0 && y != 0 && z == 0 {
            let entity = PhysicsEntity::Actor(id);
            let (across, along) = ([i64::from(x), 0, 0], [0, i64::from(y), 0]);
            let by = |first: [i64; 3], second: [i64; 3]| {
                let local = Direction::from_delta(rotate_vector(a.orientation, first))?;
                if !self.voluntary_leg_allowed(
                    id,
                    a.location,
                    local,
                    locomotion.flying,
                    allow_stairs,
                ) {
                    return None;
                }
                let (side, frame) =
                    self.translate_body(entity, a.location, a.orientation, &a.body, first)?;
                let local = Direction::from_delta(rotate_vector(frame, second))?;
                if !self.voluntary_leg_allowed(id, side, local, locomotion.flying, allow_stairs) {
                    return None;
                }
                self.translate_body(entity, side, frame, &a.body, second)
            };
            return match (by(across, along), by(along, across)) {
                (Some(one), Some(other)) if one == other => Some(one),
                (Some(one), None) | (None, Some(one)) => Some(one),
                _ => None,
            };
        }
        self.translate_body(
            PhysicsEntity::Actor(id),
            a.location,
            a.orientation,
            &a.body,
            [i64::from(x), i64::from(y), i64::from(z)],
        )
    }
    /// Completion-time arrest uses ambient gravity, even when the just-applied
    /// capability already suppresses effective acceleration. True flight never
    /// calls this hook. Tangential and upward motion survive the dose.
    pub(crate) fn arrest_descent_in_outcome(
        &self,
        outcome: &mut crate::lifecycle::ActorOutcome,
    ) -> Result<(), GameError> {
        if !outcome.state.alive() || !outcome.state.locomotion().levitating {
            return Ok(());
        }
        let actor = &mut outcome.state;
        let gravity = self.gravity_sum(actor.location, actor.orientation, &actor.body);
        // Project out only descent. Cardinal fields preserve tangential motion
        // exactly. Truncate the oblique projection toward zero so quantization
        // cannot increase speed beyond the ordinary terminal norm.
        let arrest = |vector: &mut [i64; 3]| {
            let dot: i128 = vector
                .iter()
                .zip(gravity)
                .map(|(v, g)| i128::from(*v) * i128::from(g))
                .sum();
            let norm: i128 = gravity.iter().map(|g| i128::from(*g).pow(2)).sum();
            if dot <= 0 || norm == 0 {
                return;
            }
            for axis in 0..3 {
                vector[axis] = ((i128::from(vector[axis]) * norm - i128::from(gravity[axis]) * dot)
                    / norm) as i64;
            }
            // Truncation can leave a sub-unit descending component. Remove it
            // from positive contributors only: every correction decreases both
            // descent and speed, and at most three components are visited.
            let mut remainder: i128 = vector
                .iter()
                .zip(gravity)
                .map(|(v, g)| i128::from(*v) * i128::from(g))
                .sum();
            let mut axes = [0, 1, 2];
            axes.sort_by_key(|axis| std::cmp::Reverse(gravity[*axis].unsigned_abs()));
            for axis in axes {
                if remainder <= 0 {
                    break;
                }
                if i128::from(vector[axis]) * i128::from(gravity[axis]) <= 0 {
                    continue;
                }
                let magnitude = i128::from(gravity[axis]).abs();
                let removed =
                    ((remainder + magnitude - 1) / magnitude).min(i128::from(vector[axis]).abs());
                vector[axis] -= removed as i64 * vector[axis].signum();
                remainder -= removed * magnitude;
            }
        };
        arrest(&mut actor.motion.velocity);
        arrest(&mut actor.motion.displacement);
        // A fractional accumulator is not a second placement. An oblique
        // projection can leave its per-axis cell bounds even while shortening
        // its norm; discard that fraction rather than invent a cross-cell move
        // (and contact/impact) during dose publication. Velocity remains intact.
        if actor
            .motion
            .displacement
            .iter()
            .any(|v| v.unsigned_abs() >= CELL as u64)
        {
            actor.motion.displacement = [0; 3];
        }
        actor.motion.acceleration_remainder = [0; 3];
        actor.motion.active_fall = None;
        Ok(())
    }

    fn gravity_sum(&self, at: Location, frame: u8, body: &BodySpec) -> [i64; 3] {
        let mut sum = [0; 3];
        if let Some(cells) = self.body_cells(at, frame, body) {
            for (at, frame) in cells {
                let g = self.world.gravity(at).unwrap_or([0; 3]);
                let g = rotate_vector(inverse_rotation(frame), g.map(i64::from));
                for axis in 0..3 {
                    sum[axis] += g[axis] * ACCELERATION;
                }
            }
        }
        sum
    }
    /// Creature flight changes acceleration, never imposed momentum or item gravity.
    fn effective_gravity(
        &self,
        entity: PhysicsEntity,
        at: Location,
        frame: u8,
        body: &BodySpec,
    ) -> [i64; 3] {
        if matches!(entity, PhysicsEntity::Actor(id) if self.actors[&id].locomotion().hovers()) {
            [0; 3]
        } else {
            self.gravity_sum(at, frame, body)
        }
    }
    fn supported(&self, id: ActorId) -> bool {
        let a = &self.actors[&id];
        let gravity =
            self.effective_gravity(PhysicsEntity::Actor(id), a.location, a.orientation, &a.body);
        (0..3).any(|axis| {
            if gravity[axis] == 0 {
                return false;
            }
            let mut delta = [0; 3];
            delta[axis] = gravity[axis].signum();
            self.translate_body(
                PhysicsEntity::Actor(id),
                a.location,
                a.orientation,
                &a.body,
                delta,
            )
            .is_none()
        })
    }
    /// The same per-axis obstruction query used by ordinary integration.
    fn gravity_axis_blocked(
        &self,
        entity: PhysicsEntity,
        at: Location,
        frame: u8,
        body: &BodySpec,
        gravity: [i64; 3],
        axis: usize,
    ) -> bool {
        if gravity[axis] == 0 {
            return false;
        }
        let mut delta = [0; 3];
        delta[axis] = gravity[axis].signum();
        self.translate_body(entity, at, frame, body, delta)
            .is_none()
    }

    /// A fall settles only when every gravity axis is supported and has no
    /// remaining velocity. Tangential motion alone cannot prolong a landing.
    fn fall_motion_continues(
        &self,
        entity: PhysicsEntity,
        at: Location,
        frame: u8,
        body: &BodySpec,
        motion: &MotionState,
    ) -> bool {
        if self.region_frozen(at.region) {
            return true;
        }
        let gravity = self.effective_gravity(entity, at, frame, body);
        if gravity == [0; 3] {
            return motion.velocity != [0; 3];
        }
        (0..3).any(|axis| {
            gravity[axis] != 0
                && (motion.velocity[axis] != 0
                    || !self.gravity_axis_blocked(entity, at, frame, body, gravity, axis))
        })
    }

    /// Only the current forced physical chain can license later impact damage.
    /// Unknown/frozen geometry is not permission to retire saved provenance.
    pub(crate) fn fall_continues(&self, id: ActorId, actor: &crate::Actor) -> bool {
        if self.actor_frozen(id) || self.region_frozen(actor.location.region) {
            return true;
        }
        let gravity = if actor.locomotion().hovers() {
            [0; 3]
        } else {
            self.gravity_sum(actor.location, actor.orientation, &actor.body)
        };
        if gravity == [0; 3] {
            return actor.motion.velocity != [0; 3];
        }
        (0..3).any(|axis| {
            gravity[axis] != 0
                && (actor.motion.velocity[axis] != 0
                    || !self.gravity_axis_blocked(
                        PhysicsEntity::Actor(id),
                        actor.location,
                        actor.orientation,
                        &actor.body,
                        gravity,
                        axis,
                    ))
        })
    }

    pub(crate) fn retire_settled_fall(&mut self, id: ActorId) {
        let actor = &self.actors[&id];
        if actor.motion.active_fall.is_some()
            && !self.actor_frozen(id)
            && (!actor.alive() || !self.fall_continues(id, actor))
        {
            self.actors
                .get_mut(&id)
                .expect("indexed fall")
                .motion
                .active_fall = None;
        }
    }

    pub(crate) fn retire_settled_falls(&mut self) {
        let owners: Vec<_> = self.actors.active_fall_owners().collect();
        for id in owners {
            self.retire_settled_fall(id);
        }
    }

    pub(crate) fn advance_physics(&mut self, until: u64) -> Result<u64, GameError> {
        // Never analytically step over an indexed stochastic depletion deadline.
        // The caller resumes through the shared scheduler after this boundary.
        let until = if self
            .content
            .rules()
            .document()
            .nutrition_condition_trigger
            .enabled()
        {
            self.next_creature_event_tick()
                .map_or(until, |deadline| until.min(deadline))
        } else {
            until
        };
        let until = self
            .actors
            .iter()
            .filter(|(id, _)| !self.actor_frozen(**id))
            .filter_map(|(_, actor)| {
                actor
                    .search_clock
                    .next_in(actor)
                    .into_iter()
                    .chain(actor.periodic_relocations.next_in(actor.alive()))
                    .chain(crate::hallucination_clock::next_in(actor))
                    .chain(actor.awareness.next_in(actor))
                    .min()
                    .and_then(|interval| actor.active_since.checked_add(interval))
            })
            .chain(self.next_wandering_event_tick())
            .chain(self.next_fuel_expiry())
            .min()
            .map_or(until, |deadline| until.min(deadline));
        if until <= self.tick {
            return Ok(self.tick);
        }
        self.retire_settled_falls();
        let moving = self.actors.values().any(|a| a.motion.velocity != [0; 3])
            || self.items.values().any(|i| i.motion.velocity != [0; 3]);
        if !self.world.has_gravity() && !moving {
            self.burn_light_fuel(until - self.tick);
            self.tick = until;
            self.settle_creature_time()?;
            self.refresh_light_boundary();
            return Ok(until);
        }
        for tick in self.tick + 1..=until {
            self.burn_light_fuel(1);
            self.tick = tick;
            // Expiry, death and stochastic depletion are authoritative before
            // same-tick physical effects can materialize a pure actor snapshot.
            // The end pass below still settles consequences born during physics.
            if self
                .next_creature_event_tick()
                .is_some_and(|deadline| deadline <= tick)
            {
                self.settle_creature_time()?;
                if self.run_outcome().terminal {
                    return Ok(tick);
                }
            }
            let mut active = false;
            let original_item_ceiling = self.next_item_id;
            let ids: Vec<_> = self.actors.keys().copied().collect();
            for id in ids {
                let a = &self.actors[&id];
                if !a.alive() || self.actor_frozen(id) {
                    continue;
                }
                if !self.needs_integration(
                    PhysicsEntity::Actor(id),
                    a.location,
                    a.orientation,
                    &a.body,
                    &a.motion,
                ) {
                    if a.motion.active_fall.is_some() {
                        self.actors
                            .get_mut(&id)
                            .expect("scheduled actor")
                            .motion
                            .active_fall = None;
                    }
                    continue;
                }
                active = true;
                let a = a.clone();
                let (at, frame, motion) = self.integrate(
                    PhysicsEntity::Actor(id),
                    a.location,
                    a.orientation,
                    &a.body,
                    a.motion,
                    tick,
                )?;
                if at != a.location || frame != a.orientation {
                    self.physics.displaced.insert(id);
                }
                let mut a = self.actors.get_mut(&id).expect("scheduled actor");
                a.location = at;
                a.orientation = frame;
                a.motion = motion;
                a.visited.insert(at.region);
                drop(a);
                // A sub-cell lift breaks ground contact even before the actor
                // crosses a cell boundary. Landing may therefore trigger anew.
                self.reconcile_trap_contacts(id, None)?;
                self.reconcile_holds_for_actor(id)?;
                self.reconcile_spatial_condition_anchors(id)?;
                // Falling into a frozen region freezes the actor there.
                self.sync_actor_lifecycle(id)?;
                if self.run_outcome().terminal && !self.region_transition_pending() {
                    self.settle_creature_time()?;
                    return Ok(tick);
                }
            }
            let mut newborn_items = self.activation_item_births_at(tick);
            newborn_items.extend(self.retrieval_placements_at(tick));
            let ids: Vec<_> = self
                .items
                .iter()
                .filter(|(id, _)| !newborn_items.contains(id))
                // A host entry finishes the original tick cohort. A corpse
                // created by a later collision is newborn too, even though it
                // has no population receipt. Ordinary terminal behavior stays
                // on its existing path when there is no pending entry.
                .filter(|(id, _)| !self.region_transition_pending() || id.0 < original_item_ceiling)
                .filter_map(|(id, i)| matches!(i.location, ItemLocation::Ground(_)).then_some(*id))
                .collect();
            for id in ids {
                let i = &self.items[&id];
                let ItemLocation::Ground(at) = i.location else {
                    continue;
                };
                if self.region_frozen(at.region) {
                    continue;
                }
                let body = BodySpec {
                    cells: vec![[0; 3]],
                    eye: [0; 3],
                    mass: 1,
                };
                if !self.needs_integration(
                    PhysicsEntity::Item(id),
                    at,
                    i.orientation,
                    &body,
                    &i.motion,
                ) {
                    continue;
                }
                active = true;
                let original_at = at;
                let (at, frame, motion) = self.integrate(
                    PhysicsEntity::Item(id),
                    at,
                    i.orientation,
                    &BodySpec {
                        cells: vec![[0; 3]],
                        eye: [0; 3],
                        mass: 1,
                    },
                    i.motion.clone(),
                    tick,
                )?;
                if at != original_at {
                    self.invalidate_item_claimants(id)?;
                }
                self.items
                    .edit(id, |i| {
                        i.location = ItemLocation::Ground(at);
                        i.orientation = frame;
                        i.motion = motion;
                    })
                    .expect("scheduled item");
            }
            self.refresh_light_boundary();
            if !active {
                if self.region_transition_pending() {
                    self.settle_creature_time()?;
                    return Ok(tick);
                }
                break;
            }
            // Due conditions and capacity changes precede work completion on
            // moving ticks, just as they do on the stationary fast path.
            self.settle_creature_time()?;
            if self.run_outcome().terminal {
                return Ok(tick);
            }
            self.resolve_preparations()?;
            self.check_run_policy()?;
            if self.run_outcome().terminal {
                self.settle_creature_time()?;
                return Ok(tick);
            }
            // An accepted entry is immediate, but all existing entities and
            // due work at this tick must finish before horizons are settled.
            // A mid-tick return would silently skip the remaining entities.
            if self.region_transition_pending() {
                return Ok(tick);
            }
            if self
                .next_actor()
                .is_some_and(|id| self.actors[&id].ready_at <= tick)
            {
                self.settle_creature_time()?;
                return Ok(tick);
            }
        }
        // Resting physics can stop early. Settle once at the requested boundary;
        // lifecycle changes already settled before switching active clocks.
        self.burn_light_fuel(until - self.tick);
        self.tick = until;
        self.settle_creature_time()?;
        self.refresh_light_boundary();
        Ok(until)
    }
    fn needs_integration(
        &self,
        entity: PhysicsEntity,
        at: Location,
        frame: u8,
        body: &BodySpec,
        motion: &MotionState,
    ) -> bool {
        if motion.velocity != [0; 3] {
            return true;
        }
        let gravity = self.effective_gravity(entity, at, frame, body);
        (0..3).any(|axis| {
            gravity[axis] != 0 && !self.gravity_axis_blocked(entity, at, frame, body, gravity, axis)
        })
    }
    fn publish_physical_entry(
        &mut self,
        entity: PhysicsEntity,
        at: Location,
        frame: u8,
        motion: &MotionState,
    ) -> Result<(), GameError> {
        let PhysicsEntity::Actor(id) = entity else {
            return Ok(());
        };
        let from = self.actors[&id].location;
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        actor.location = at;
        actor.orientation = frame;
        actor.motion = motion.clone();
        actor.visited.insert(at.region);
        drop(actor);
        self.reconcile_holds_for_actor(id)?;
        self.reconcile_spatial_condition_anchors(id)?;
        if self.floor_activation.is_some() && from.region != at.region {
            self.sync_actor_lifecycle(id)?;
            self.accepted_region_entry(id, from, at, crate::floor_activation::EntryCause::Physics)?;
        }
        self.reconcile_trap_contacts(id, None)
    }
    /// Contact effects may relocate or kill the body. Never overwrite their
    /// authoritative result with the integrator's pre-contact local cursor.
    fn contact_motion_override(
        &self,
        entity: PhysicsEntity,
        at: Location,
        frame: u8,
        motion: &MotionState,
    ) -> Option<(Location, u8, MotionState)> {
        let PhysicsEntity::Actor(id) = entity else {
            return None;
        };
        let actor = &self.actors[&id];
        (actor.location != at
            || actor.orientation != frame
            || !actor.motion.same_kinematics(motion)
            || !actor.alive())
        .then(|| (actor.location, actor.orientation, actor.motion.clone()))
    }

    fn integrate(
        &mut self,
        entity: PhysicsEntity,
        mut at: Location,
        mut frame: u8,
        body: &BodySpec,
        mut motion: MotionState,
        tick: u64,
    ) -> Result<(Location, u8, MotionState), GameError> {
        crate::diagnostics::physics_step();
        if motion.active_fall.is_some()
            && !self.fall_motion_continues(entity, at, frame, body, &motion)
        {
            motion.active_fall = None;
        }
        let gravity = self.effective_gravity(entity, at, frame, body);
        for axis in 0..3 {
            let resting = motion.velocity[axis] == 0
                && self.gravity_axis_blocked(entity, at, frame, body, gravity, axis);
            if resting {
                motion.acceleration_remainder[axis] = 0;
                continue;
            }
            let numerator = gravity[axis] + motion.acceleration_remainder[axis];
            motion.velocity[axis] += numerator / body.cells.len() as i64;
            motion.acceleration_remainder[axis] = numerator % body.cells.len() as i64;
        }
        cap_velocity(&mut motion.velocity);
        let mut delta = [0; 3];
        for (axis, component) in delta.iter_mut().enumerate() {
            motion.displacement[axis] += motion.velocity[axis];
            *component = motion.displacement[axis] / CELL;
        }
        if delta != [0; 3] {
            if let Some((next, next_frame)) = self.translate_body(entity, at, frame, body, delta) {
                at = next;
                frame = next_frame;
                for (axis, component) in delta.iter().enumerate() {
                    motion.displacement[axis] -= component * CELL;
                }
                self.publish_physical_entry(entity, at, frame, &motion)?;
                if let Some(authoritative) =
                    self.contact_motion_override(entity, at, frame, &motion)
                {
                    return Ok(authoritative);
                }
            } else {
                for axis in 0..3 {
                    if delta[axis] == 0 {
                        continue;
                    }
                    let mut component = [0; 3];
                    component[axis] = delta[axis];
                    if let Some((next, next_frame)) =
                        self.translate_body(entity, at, frame, body, component)
                    {
                        at = next;
                        frame = next_frame;
                        motion.displacement[axis] -= delta[axis] * CELL;
                        self.publish_physical_entry(entity, at, frame, &motion)?;
                        if let Some(authoritative) =
                            self.contact_motion_override(entity, at, frame, &motion)
                        {
                            return Ok(authoritative);
                        }
                    } else {
                        let normal = rotate_vector(frame, component.map(|v| -v.signum()));
                        let other_actor = self.body_cells(at, frame, body).and_then(|cells| {
                            cells.into_iter().find_map(|(cell, rotation)| {
                                let dir =
                                    Direction::from_delta(rotate_vector(rotation, component))?;
                                let (target, _) = self.world.physics_neighbor(cell, dir)?;
                                self.actors
                                    .at(&self.world, target)
                                    .keys()
                                    .copied()
                                    .find(|id| {
                                        entity != PhysicsEntity::Actor(*id)
                                            && self.actors[id].alive()
                                    })
                            })
                        });
                        self.physics.impacts.push(Impact {
                            entity,
                            at_tick: tick,
                            location: at,
                            axis,
                            normal,
                            other_actor,
                            incoming_velocity: motion.velocity[axis],
                            mass: match entity {
                                PhysicsEntity::Actor(_) => u64::from(body.mass),
                                PhysicsEntity::Item(id) => self.items[&id].quantity,
                            },
                        });
                        let damage = crate::combat::impact_damage(motion.velocity[axis]);
                        motion.velocity[axis] = 0;
                        motion.displacement[axis] = 0;
                        motion.acceleration_remainder[axis] = 0;
                        if let PhysicsEntity::Actor(id) = entity {
                            {
                                let mut actor = self.actors.get_mut(&id).expect("integrated actor");
                                actor.location = at;
                                actor.orientation = frame;
                                actor.motion = motion.clone();
                            }
                            if damage > 0 {
                                let damage = crate::damage::DamageSpec::new(
                                    vec![crate::damage::DamageComponent::fixed(
                                        crate::combat::DamageType::Impact,
                                        None,
                                        damage,
                                    )],
                                    None,
                                )
                                .map_err(|_| {
                                    GameError::Rule(crate::abilities::RuleError::InvalidDefinition)
                                })?;
                                // Keep the episode until the impact has settled. A deleted
                                // source is safely unattributed; a detached source must first
                                // arrive through its load-only region requirement.
                                let cause = match motion.active_fall {
                                    Some(fall) if self.detached_actor(fall.owner) => {
                                        return Err(GameError::UnknownActor);
                                    }
                                    Some(fall)
                                        if self
                                            .actors
                                            .get(&fall.owner)
                                            .is_some_and(|owner| owner.combat.is_some()) =>
                                    {
                                        Some(crate::earned_growth::FatalCause {
                                            owner: fall.owner,
                                            origin:
                                                crate::earned_growth::FatalOrigin::Displacement {
                                                    episode: fall.episode,
                                                },
                                        })
                                    }
                                    _ => None,
                                };
                                if let Some(cause) = cause {
                                    self.apply_damage_with_cause(id, &damage, Some(&cause))?;
                                } else {
                                    self.apply_damage(id, &damage)?;
                                }
                                if !self.alive(id) {
                                    let actor = &self.actors[&id];
                                    return Ok((
                                        actor.location,
                                        actor.orientation,
                                        MotionState::default(),
                                    ));
                                }
                            }
                            // The collision has already happened. Settle its
                            // damage before a trap can relocate the survivor.
                            self.reconcile_trap_contacts(id, Some(normal))?;
                            if let Some(authoritative) =
                                self.contact_motion_override(entity, at, frame, &motion)
                            {
                                return Ok(authoritative);
                            }
                        }
                    }
                }
            }
        }
        // Every impact in this physics step receives the pending episode.
        // Retire only after all components settle, preserving harmless contact
        // and earlier nonlethal impacts while another gravity axis continues.
        if motion.active_fall.is_some()
            && !self.fall_motion_continues(entity, at, frame, body, &motion)
        {
            motion.active_fall = None;
        }
        Ok((at, frame, motion))
    }
}
fn cap_velocity(v: &mut [i64; 3]) {
    let squared: u128 = v.iter().map(|n| u128::from(n.unsigned_abs()).pow(2)).sum();
    if squared > (TERMINAL as u128).pow(2) {
        let root = squared.isqrt();
        let ceiling = root + u128::from(root * root != squared);
        for n in v {
            *n = (i128::from(*n) * i128::from(TERMINAL) / ceiling as i128) as i64;
        }
    }
}

#[cfg(test)]
mod definition_sharing_tests {
    use super::*;
    use crate::Action;
    use std::num::NonZeroU64;
    use tor_world::{Extent, Position, Region, World};

    #[test]
    fn body_edits_detach_snapshots_and_keep_checkpoint_encoding() {
        let mut game = Game::two_room(42, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        game.observe(actor).unwrap();
        let old = game.clone();
        let body = BodySpec {
            cells: vec![[0, 0, 0], [0, 1, 0]],
            eye: [0, 1, 0],
            mass: 97,
        };
        game.set_body(actor, body.clone()).unwrap();
        let extra = Location {
            position: Position { x: 1, y: 2, z: 0 },
            ..at
        };
        assert!(old.actors.at(&old.world, extra).is_empty());
        assert!(game.actors.at(&game.world, extra).contains_key(&actor));
        assert_eq!(&*old.actors[&actor].body, &BodySpec::default());
        assert_eq!(&*game.actors[&actor].body, &body);
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let encoded = serde_json::to_value(&snapshot).unwrap();
        assert_eq!(
            serde_json::to_value(&shared).unwrap()["actors"]
                [encoded["actors"].as_u64().unwrap() as usize][actor.0.to_string()]["body"],
            serde_json::to_value(&body).unwrap()
        );
        let decoded = serde_json::from_value(encoded).unwrap();
        let restored = Game::restore_checkpoint(decoded, &shared, game.content().clone()).unwrap();
        assert_eq!(restored, game);
        assert_eq!(
            restored.actors.at(&restored.world, extra),
            game.actors.at(&game.world, extra)
        );
    }

    #[test]
    fn readiness_changes_do_not_copy_unrelated_body_definitions() {
        for unrelated in [16, 256, 4096] {
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_region(Region {
                    id: RegionId(1),
                    name: "actors".into(),
                    bounds: Extent::new(unrelated + 2, 3, 1).unwrap(),
                })
                .unwrap();
            let mut game = Game::new(world, 42, crate::test_creatures::content());
            for x in 0..=unrelated {
                game.spawn_actor(
                    Location {
                        region: RegionId(1),
                        position: Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            }
            game.observe(ActorId(1)).unwrap();
            let old = game.clone();
            let before = BODY_DEFINITION_COPIES.with(|copies| copies.get());
            game.act(ActorId(1), Action::Wait).unwrap();
            let copied = BODY_DEFINITION_COPIES.with(|copies| copies.get()) - before;
            assert_eq!(copied, 0, "unrelated actors: {unrelated}");
            assert_eq!(old.next_actor(), Some(ActorId(1)));
            assert_eq!(game.next_actor(), Some(ActorId(2)));
            assert_eq!(old.actors[&ActorId(1)].body, game.actors[&ActorId(1)].body);
        }
    }
}
