//! Deterministic rigid cell bodies. Motion is expressed in the persistent body
//! frame, so portal transitions transform vectors without rounding or facing.
use crate::{ActorId, Game, GameError, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
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

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(test), derive(Clone))]
#[serde(deny_unknown_fields)]
pub struct BodySpec {
    pub cells: Vec<[i32; 3]>,
    /// The body cell whose centre every sight line starts from. Required in
    /// every declaration; see `docs/sight-3d.md`.
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionState {
    /// Fixed-point cells per tick, in the persistent body frame.
    pub velocity: [i64; 3],
    pub displacement: [i64; 3],
    pub acceleration_remainder: [i64; 3],
}
impl MotionState {
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
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Physics {
    pub impacts: Vec<Impact>,
    pub displaced: BTreeSet<ActorId>,
}

impl Game {
    pub(crate) fn physics_valid(&self) -> bool {
        self.physics
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
        self.world
            .set_gravity(region, vector)
            .map_err(|_| GameError::InvalidLocation)
    }
    pub fn set_cell_gravity(&mut self, at: Location, vector: [i32; 3]) -> Result<(), GameError> {
        self.world
            .set_cell_gravity(at, vector)
            .map_err(|_| GameError::InvalidLocation)
    }
    pub fn set_body(&mut self, id: ActorId, body: BodySpec) -> Result<(), GameError> {
        let actor = self.actors.get(&id).ok_or(GameError::UnknownActor)?;
        if !body.valid() || !self.body_fits(id, actor.location, actor.orientation, &body) {
            return Err(GameError::Blocked);
        }
        let mut actor = self.actors.get_mut(&id).expect("validated actor");
        actor.body = tor_world::Shared::new(body);
        actor.motion.acceleration_remainder = [0; 3];
        Ok(())
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
            || self.body_has_field(a.location, a.orientation, &a.body)
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
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        actor.motion.velocity = velocity;
        cap_velocity(&mut actor.motion.velocity);
        Ok(())
    }
    pub fn connect_portal_area(
        &mut self,
        passage: tor_world::Passage,
        rotation: u8,
        width: u16,
        height: u16,
    ) -> Result<(), GameError> {
        self.world
            .connect_portal_area(passage, rotation, width, height)
            .map_err(|_| GameError::InvalidLocation)
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
    fn translate_body(
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
    pub(crate) fn actor_translation(
        &self,
        id: ActorId,
        direction: Direction,
    ) -> Option<(Location, u8)> {
        let a = &self.actors[&id];
        let local = direction.rotated(a.orientation);
        if self.world.is_stair(a.location, direction) {
            let (to, r) = self.reach(a.location, direction)?;
            let frame = compose_rotation(a.orientation, r);
            return self
                .body_fits(id, to, frame, &a.body)
                .then_some((to, frame));
        }
        if matches!(direction, Direction::Up | Direction::Down)
            && self.world.passage(a.location, local).is_none()
        {
            return None;
        }
        let physical = self.body_has_field(a.location, a.orientation, &a.body);
        if physical && !self.supported(id) {
            return None;
        }
        let (x, y, z) = direction.delta();
        if a.body.cells.len() == 1 && !physical {
            if matches!(direction, Direction::Up | Direction::Down)
                && self.world.passage(a.location, local).is_none()
            {
                return None;
            }
            // Retain established beveled-corner walking for diagnostic fixtures.
            let (to, r) = self.reach(a.location, local)?;
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
                let (side, frame) =
                    self.translate_body(entity, a.location, a.orientation, &a.body, first)?;
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
    fn supported(&self, id: ActorId) -> bool {
        let a = &self.actors[&id];
        let gravity = self.gravity_sum(a.location, a.orientation, &a.body);
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
    pub(crate) fn advance_physics(&mut self, until: u64) -> u64 {
        if until <= self.tick {
            return self.tick;
        }
        let moving = self.actors.values().any(|a| a.motion.velocity != [0; 3])
            || self.items.values().any(|i| i.motion.velocity != [0; 3]);
        if !self.world.has_gravity() && !moving {
            return until;
        }
        for tick in self.tick + 1..=until {
            self.tick = tick;
            let mut active = false;
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
                );
                if at != a.location || frame != a.orientation {
                    self.physics.displaced.insert(id);
                }
                let mut a = self.actors.get_mut(&id).expect("scheduled actor");
                a.location = at;
                a.orientation = frame;
                a.motion = motion;
                a.visited.insert(at.region);
                drop(a);
                // Falling into a frozen region freezes the actor there.
                self.sync_actor_lifecycle(id);
                if self.combat.outcome.terminal {
                    return tick;
                }
            }
            let ids: Vec<_> = self
                .items
                .iter()
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
                );
                self.items
                    .edit(id, |i| {
                        i.location = ItemLocation::Ground(at);
                        i.orientation = frame;
                        i.motion = motion;
                    })
                    .expect("scheduled item");
            }
            if !active {
                break;
            }
            self.resolve_attacks();
            self.check_objective();
            if self.combat.outcome.terminal {
                return tick;
            }
            if self
                .next_actor()
                .is_some_and(|id| self.actors[&id].ready_at <= tick)
            {
                return tick;
            }
        }
        until
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
        let gravity = self.gravity_sum(at, frame, body);
        (0..3).any(|axis| {
            if gravity[axis] == 0 {
                return false;
            }
            let mut delta = [0; 3];
            delta[axis] = gravity[axis].signum();
            self.translate_body(entity, at, frame, body, delta)
                .is_some()
        })
    }
    fn integrate(
        &mut self,
        entity: PhysicsEntity,
        mut at: Location,
        mut frame: u8,
        body: &BodySpec,
        mut motion: MotionState,
        tick: u64,
    ) -> (Location, u8, MotionState) {
        crate::diagnostics::physics_step();
        let gravity = self.gravity_sum(at, frame, body);
        for axis in 0..3 {
            let mut delta = [0; 3];
            delta[axis] = gravity[axis].signum();
            let resting = gravity[axis] != 0
                && motion.velocity[axis] == 0
                && self
                    .translate_body(entity, at, frame, body, delta)
                    .is_none();
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
                            if damage > 0 {
                                let mut actor = self.actors.get_mut(&id).expect("integrated actor");
                                actor.location = at;
                                actor.orientation = frame;
                                actor.motion = motion.clone();
                                drop(actor);
                                self.apply_damage(
                                    id,
                                    &std::collections::BTreeMap::from([(
                                        crate::combat::DamageType::Impact,
                                        damage,
                                    )]),
                                );
                                if !self.alive(id) {
                                    return (at, frame, MotionState::default());
                                }
                            }
                        }
                    }
                }
            }
        }
        (at, frame, motion)
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
        let mut game = Game::two_room(42);
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
            encoded["actors"][actor.0.to_string()]["body"],
            serde_json::to_value(&body).unwrap()
        );
        let decoded = serde_json::from_value(encoded).unwrap();
        let restored = Game::restore_checkpoint(decoded, &shared).unwrap();
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
            let mut game = Game::new(world, 42);
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
