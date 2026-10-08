//! Backend-only deterministic checkpoint state. Equal world and navigation
//! regions, actor tables and intentions are encoded once across rewind boundaries.
//! This module does not perform storage or I/O.
use crate::streaming::Lifecycle;
use crate::{travel::Navigation, Actor, ActorId, Game, Item, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::Location;
use tor_world::{Shared, World};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    combat: crate::combat::CombatWorld,
    physics: crate::physics::Physics,
    world: usize,
    navigation: BTreeMap<ActorId, usize>,
    seed: u64,
    tick: u64,
    actors: usize,
    items: usize,
    next_actor_id: u64,
    next_item_id: u64,
    next_door_id: u64,
    intentions: crate::intention::checkpoint::Snapshot,
    /// Region streaming state in [`SharedState`], omitted while empty so
    /// games that never stream save exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lifecycle: Option<usize>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedState {
    #[serde(with = "tor_world::checkpoint_worlds")]
    worlds: Vec<World>,
    #[serde(with = "navigation_regions")]
    navigation: Vec<Navigation>,
    items: Vec<BTreeMap<ItemId, Item>>,
    actors: Vec<BTreeMap<ActorId, Actor>>,
    intentions: crate::intention::checkpoint::Pool,
    /// Identical lifecycle states (with their identity directories) across
    /// rewind boundaries are encoded once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lifecycles: Vec<Lifecycle>,
}

impl SharedState {
    pub fn valid_world_count(&self, maximum: usize) -> bool {
        !self.worlds.is_empty() && self.worlds.len() <= maximum
    }
}

/// Ownership shared while restoring one decoded checkpoint and its rewind states.
/// This context is never serialized. Dropping it releases its temporary pool
/// references; restored games retain only the definitions they actually use.
pub struct RestoreContext<'a> {
    shared: &'a SharedState,
    worlds: BTreeMap<usize, Shared<World>>,
    navigation: BTreeMap<usize, Shared<Navigation>>,
    items: BTreeMap<usize, crate::item_store::ItemStore>,
    actors: BTreeMap<usize, crate::actor_store::ActorStore>,
    intentions: crate::intention::checkpoint::Restore,
    bodies: BTreeSet<Shared<crate::BodySpec>>,
    item_definitions: BTreeSet<Shared<crate::ItemSpec>>,
    combat_definitions: BTreeSet<Shared<crate::combat::CombatSpec>>,
    anatomies: BTreeSet<Shared<crate::AnatomySpec>>,
}

impl<'a> RestoreContext<'a> {
    pub fn new(shared: &'a SharedState) -> Self {
        Self {
            shared,
            worlds: BTreeMap::new(),
            navigation: BTreeMap::new(),
            items: BTreeMap::new(),
            actors: BTreeMap::new(),
            intentions: crate::intention::checkpoint::Restore::default(),
            bodies: BTreeSet::new(),
            item_definitions: BTreeSet::new(),
            combat_definitions: BTreeSet::new(),
            anatomies: BTreeSet::new(),
        }
    }

    pub fn restore(&mut self, snapshot: Snapshot) -> Option<Game> {
        Game::restore_with_context(snapshot, self)
    }

    fn world(&mut self, index: usize) -> Option<Shared<World>> {
        if let Some(world) = self.worlds.get(&index) {
            return Some(world.clone());
        }
        let world = Shared::new(self.shared.worlds.get(index)?.clone());
        self.worlds.insert(index, world.clone());
        Some(world)
    }

    fn navigation(&mut self, index: usize) -> Option<Shared<Navigation>> {
        if let Some(navigation) = self.navigation.get(&index) {
            return Some(navigation.clone());
        }
        let navigation = Shared::new(self.shared.navigation.get(index)?.clone());
        self.navigation.insert(index, navigation.clone());
        Some(navigation)
    }

    fn items(&mut self, index: usize) -> Option<crate::item_store::ItemStore> {
        if let Some(items) = self.items.get(&index) {
            return Some(items.clone());
        }
        let mut entries = self.shared.items.get(index)?.clone();
        for item in entries.values_mut() {
            share_definition(&mut self.item_definitions, &mut item.spec);
        }
        let items = crate::item_store::ItemStore::from_entries(entries);
        self.items.insert(index, items.clone());
        Some(items)
    }

    fn actors(&mut self, index: usize) -> Option<crate::actor_store::ActorStore> {
        if let Some(actors) = self.actors.get(&index) {
            return Some(actors.clone());
        }
        let mut entries = self.shared.actors.get(index)?.clone();
        self.share_actor_definitions(&mut entries);
        let actors = crate::actor_store::ActorStore::from_entries(entries);
        self.actors.insert(index, actors.clone());
        Some(actors)
    }

    fn share_actor_definitions(&mut self, actors: &mut BTreeMap<ActorId, Actor>) {
        for actor in actors.values_mut() {
            share_definition(&mut self.anatomies, &mut actor.anatomy);
            share_definition(&mut self.bodies, &mut actor.body);
            if let Some(combat) = &mut actor.combat {
                share_definition(&mut self.combat_definitions, &mut combat.spec);
            }
        }
    }
}

/// Compare the complete value, never its identity label or allocation address.
/// Borrowing the key avoids copying a definition merely to look it up.
fn share_definition<T: Ord>(pool: &mut BTreeSet<Shared<T>>, definition: &mut Shared<T>) {
    if let Some(existing) = pool.get(&**definition) {
        *definition = existing.clone();
    } else {
        pool.insert(definition.clone());
    }
}

impl Game {
    pub fn checkpoint(&self, shared: &mut SharedState) -> Snapshot {
        let worlds = &mut shared.worlds;
        let world = worlds
            .iter()
            .position(|w| w == &*self.world)
            .unwrap_or_else(|| {
                worlds.push((*self.world).clone());
                worlds.len() - 1
            });
        Snapshot {
            combat: self.combat.clone(),
            physics: self.physics.clone(),
            world,
            navigation: self
                .navigation
                .iter()
                .map(|(actor, navigation)| {
                    let index = shared
                        .navigation
                        .iter()
                        .position(|n| n == &**navigation)
                        .unwrap_or_else(|| {
                            shared.navigation.push((**navigation).clone());
                            shared.navigation.len() - 1
                        });
                    (*actor, index)
                })
                .collect(),
            seed: self.seed,
            tick: self.tick,
            actors: shared
                .actors
                .iter()
                .position(|actors| actors == self.actors.raw_entries())
                .unwrap_or_else(|| {
                    shared.actors.push(self.actors.raw_entries().clone());
                    shared.actors.len() - 1
                }),
            items: shared
                .items
                .iter()
                .position(|items| items == &*self.items)
                .unwrap_or_else(|| {
                    shared.items.push((*self.items).clone());
                    shared.items.len() - 1
                }),
            next_actor_id: self.next_actor_id,
            next_item_id: self.next_item_id,
            next_door_id: self.next_door_id,
            intentions: shared.intentions.capture(&self.intentions),
            lifecycle: (!self.lifecycle.is_empty()).then(|| {
                let lifecycles = &mut shared.lifecycles;
                lifecycles
                    .iter()
                    .position(|l| l == &self.lifecycle)
                    .unwrap_or_else(|| {
                        lifecycles.push(self.lifecycle.clone());
                        lifecycles.len() - 1
                    })
            }),
        }
    }

    pub fn restore_checkpoint(snapshot: Snapshot, shared: &SharedState) -> Option<Self> {
        RestoreContext::new(shared).restore(snapshot)
    }

    fn restore_with_context(snapshot: Snapshot, context: &mut RestoreContext<'_>) -> Option<Self> {
        let game = Self {
            combat: snapshot.combat,
            physics: snapshot.physics,
            world: context.world(snapshot.world)?,
            navigation: snapshot
                .navigation
                .into_iter()
                .map(|(actor, index)| Some((actor, context.navigation(index)?)))
                .collect::<Option<_>>()?,
            seed: snapshot.seed,
            tick: snapshot.tick,
            actors: context.actors(snapshot.actors)?,
            items: context.items(snapshot.items)?,
            next_actor_id: snapshot.next_actor_id,
            next_item_id: snapshot.next_item_id,
            next_door_id: snapshot.next_door_id,
            intentions: context
                .intentions
                .restore(snapshot.intentions, &context.shared.intentions)?,
            lifecycle: match snapshot.lifecycle {
                None => Lifecycle::default(),
                // An empty state is always omitted, so encodings stay unique.
                Some(index) => {
                    Some(context.shared.lifecycles.get(index)?.clone()).filter(|l| !l.is_empty())?
                }
            },
        };
        let mut occupied = BTreeSet::new();
        if !game.intentions_valid()
            || !game.lifecycle_state_valid()
            || !game.physics_valid()
            || !game.combat_valid()
            || (game.actors.is_empty() && !game.has_detached_actors())
            || game.next_actor_id == 0
            || game.next_item_id == 0
            || game.next_door_id == 0
            || game
                .next_actor()
                .is_some_and(|id| game.actors[&id].ready_at != game.tick)
            || game.actors.iter().any(|(id, actor)| {
                !game.actor_state_valid(*id, actor) || !game.body_has_room(actor, &mut occupied)
            })
            || game
                .items
                .iter()
                .any(|(id, item)| !game.item_state_valid(*id, item))
            || game.navigation.iter().any(|(id, navigation)| {
                !game.actors.contains_key(id) || !navigation.checkpoint_valid(&game.world)
            })
            || !game.world.checkpoint_valid(game.next_door_id)
        {
            return None;
        }
        Some(game)
    }

    /// Checks restoring gives a loaded actor, apart from its body's room;
    /// attaching a stored region gives its actors the same.
    pub(crate) fn actor_state_valid(&self, id: ActorId, actor: &Actor) -> bool {
        id.0 != 0
            && actor
                .combat
                .as_ref()
                .is_none_or(|c| c.spec.valid() && c.hp <= c.spec.max_hp)
            && id.0 < self.next_actor_id
            && actor.orientation < 24
            && (!actor.alive() || actor.ready_at >= self.actor_clock(id))
            && self.world.contains(actor.location)
            && actor.visited.iter().all(|id| self.world.knows_region(*id))
            && actor.body.valid()
            && actor.motion.valid()
            && actor
                .motion
                .acceleration_remainder
                .iter()
                .all(|v| v.unsigned_abs() < actor.body.cells.len() as u64)
    }

    /// Whether a living actor's body fits walkable cells that no body in
    /// `occupied` holds, adding its cells. Only call this for an actor whose
    /// state is valid.
    pub(crate) fn body_has_room(&self, actor: &Actor, occupied: &mut BTreeSet<Location>) -> bool {
        !actor.alive()
            || self
                .body_cells(actor.location, actor.orientation, &actor.body)
                .is_some_and(|cells| {
                    cells
                        .iter()
                        .all(|(at, _)| self.world.walkable(*at) && occupied.insert(*at))
                })
    }

    /// Checks restoring gives a loaded item; attaching a stored region gives
    /// its items the same.
    pub(crate) fn item_state_valid(&self, id: ItemId, item: &Item) -> bool {
        id.0 != 0
            && id.0 < self.next_item_id
            && item.quantity != 0
            && (item.spec.stackable || item.quantity == 1)
            && item.spec.valid()
            && item.motion.valid()
            && item.motion.acceleration_remainder == [0; 3]
            && item.orientation < 24
            && match item.location {
                ItemLocation::Ground(location) => self.world.contains(location),
                ItemLocation::Carried(actor) => self.actors.contains_key(&actor),
            }
    }

    /// Every actor a checkpoint's revisions cover: loaded, detached or not
    /// built yet.
    pub fn checkpoint_actor_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.known_actor_ids().into_iter().map(|id| id.0)
    }

    /// Whether an actor is in a loaded region.
    pub fn has_actor(&self, id: ActorId) -> bool {
        self.actors.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Location, Position, RegionId};

    #[test]
    fn repeated_boundaries_do_not_repeat_unchanged_actor_payloads() {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "retention".into(),
                bounds: tor_world::Extent::new(66, 3, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42);
        for x in 1..=64 {
            game.spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        }
        let mut one = SharedState::default();
        let snapshot = game.checkpoint(&mut one);
        let single_bytes = serde_json::to_vec(&(vec![snapshot], one)).unwrap().len();
        let mut shared = SharedState::default();
        let snapshots: Vec<_> = (0..256)
            .map(|_| {
                let mut independently_owned = game.clone();
                independently_owned.actors =
                    crate::actor_store::ActorStore::from_entries(game.actors.raw_entries().clone());
                independently_owned.checkpoint(&mut shared)
            })
            .collect();
        assert_eq!(shared.actors.len(), 1);
        let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
        assert!(
            bytes.len() < single_bytes * 8,
            "unchanged actors repeated at retained boundaries: {} versus {}",
            bytes.len(),
            single_bytes
        );
        let (snapshots, shared): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&shared);
        let mut restored: Vec<_> = snapshots
            .into_iter()
            .map(|snapshot| context.restore(snapshot).unwrap())
            .collect();
        for boundary in &restored {
            assert_eq!(boundary, &game);
        }
        assert!(std::ptr::eq(
            &restored[0].actors[&ActorId(1)],
            &restored[1].actors[&ActorId(1)]
        ));
        restored[0].actors.get_mut(&ActorId(1)).unwrap().body.mass += 1;
        assert_eq!(restored[1], game);
    }

    #[test]
    fn retained_queues_share_complete_intentions_at_the_supported_capacity() {
        for count in [64, crate::intention::MAX_QUEUED_INTENTIONS] {
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_region(tor_world::Region {
                    id: RegionId(1),
                    name: "queue retention".into(),
                    bounds: tor_world::Extent::new(count as i32 + 2, 3, 1).unwrap(),
                })
                .unwrap();
            let mut queued = Game::new(world, 42);
            for x in 1..=count as i32 {
                let actor = queued
                    .spawn_actor(
                        Location {
                            region: RegionId(1),
                            position: Position { x, y: 1, z: 0 },
                        },
                        NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                queued
                    .admit_intention(
                        actor,
                        crate::Action::Wait,
                        crate::intention::IntentionOrigin::Human,
                    )
                    .unwrap();
            }
            let id = queued.pending_intention(ActorId(1)).unwrap().id;
            let mut suspended = queued.clone();
            suspended.suspend_intention(ActorId(1), id).unwrap();
            let mut single = SharedState::default();
            let snapshot = queued.checkpoint(&mut single);
            let single_bytes = serde_json::to_vec(&(vec![snapshot], single)).unwrap().len();
            let mut shared = SharedState::default();
            let snapshots: Vec<_> = (0..256)
                .map(|index| {
                    if index % 2 == 0 {
                        queued.checkpoint(&mut shared)
                    } else {
                        suspended.checkpoint(&mut shared)
                    }
                })
                .collect();
            let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
            assert!(bytes.len() < single_bytes * 8,
                "retained queues repeat complete payloads at {count} actors: {} versus {single_bytes}", bytes.len());
            assert!(bytes.len() < 16 * 1024 * 1024);
            let (snapshots, shared): (Vec<Snapshot>, SharedState) =
                serde_json::from_slice(&bytes).unwrap();
            let mut context = RestoreContext::new(&shared);
            let mut restored: Vec<_> = snapshots
                .into_iter()
                .map(|s| context.restore(s).unwrap())
                .collect();
            for (index, game) in restored.iter().enumerate() {
                assert_eq!(game, if index % 2 == 0 { &queued } else { &suspended });
            }
            restored[0].cancel_intention(ActorId(1), id).unwrap();
            assert_eq!(restored[2], queued);
            let next = restored[0]
                .admit_intention(
                    ActorId(1),
                    crate::Action::Wait,
                    crate::intention::IntentionOrigin::Human,
                )
                .unwrap();
            assert_eq!(next.0, count as u64 + 1);
        }
    }

    #[test]
    fn decoded_restore_shares_equal_bodies_without_changing_values() {
        for count in [16, 256, 4096] {
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_region(tor_world::Region {
                    id: RegionId(1),
                    name: "restore".into(),
                    bounds: tor_world::Extent::new(count * 3 + 3, 3, 1).unwrap(),
                })
                .unwrap();
            let mut game = Game::new(world, 42);
            for x in 0..count {
                let id = game
                    .spawn_actor(
                        Location {
                            region: RegionId(1),
                            position: Position {
                                x: x * 3,
                                y: 1,
                                z: 0,
                            },
                        },
                        NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                if x % 2 == 1 {
                    game.set_body(
                        id,
                        crate::BodySpec {
                            cells: vec![[0, 0, 0], [1, 0, 0]],
                            eye: [1, 0, 0],
                            mass: 91,
                        },
                    )
                    .unwrap();
                }
            }
            let mut shared = SharedState::default();
            let snapshot = game.checkpoint(&mut shared);
            let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
            let (snapshot, shared): (Snapshot, SharedState) =
                serde_json::from_slice(&bytes).unwrap();
            let restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
            assert_eq!(restored, game);
            let first = &restored.actors[&ActorId(1)].body;
            let second = &restored.actors[&ActorId(2)].body;
            assert!(!first.shares_storage(second));
            for (id, actor) in restored.actors.iter() {
                let expected = if id.0 % 2 == 1 { first } else { second };
                assert!(
                    actor.body.shares_storage(expected),
                    "actor {id:?}, count {count}"
                );
            }
            let mut encoded_shared = SharedState::default();
            let encoded_snapshot = restored.checkpoint(&mut encoded_shared);
            assert_eq!(
                serde_json::to_vec(&(encoded_snapshot, encoded_shared)).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn decoded_item_and_combat_definitions_share_across_distinct_boundaries() {
        let mut game = Game::two_room_in_stone(42);
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        game.configure_anatomy(actor, crate::AnatomySpec::humanoid())
            .unwrap();
        game.configure_combat(actor, crate::combat::CombatSpec::default())
            .unwrap();
        let mut spec = crate::ItemSpec::ordinary("token".into());
        spec.stackable = true;
        let item = game.place_item_stack(100, at, None, 2, spec).unwrap();
        let mut shared = SharedState::default();
        let before = game.checkpoint(&mut shared);
        game.items.edit(item, |entry| entry.quantity = 1).unwrap();
        game.actors
            .get_mut(&actor)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .hp = 29;
        let after = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(vec![before, after], shared)).unwrap();
        let (snapshots, shared): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&shared);
        let mut restored: Vec<_> = snapshots
            .into_iter()
            .map(|s| context.restore(s).unwrap())
            .collect();
        assert!(!restored[0].items.shares_storage(&restored[1].items));
        assert!(restored[0].actors[&actor]
            .anatomy
            .shares_storage(&restored[1].actors[&actor].anatomy));
        assert!(restored[0].items[&item]
            .spec
            .shares_storage(&restored[1].items[&item].spec));
        assert!(restored[0].actors[&actor]
            .combat
            .as_ref()
            .unwrap()
            .spec
            .shares_storage(&restored[1].actors[&actor].combat.as_ref().unwrap().spec));
        drop(context);
        let mut encoded_shared = SharedState::default();
        let encoded: Vec<_> = restored
            .iter()
            .map(|g| g.checkpoint(&mut encoded_shared))
            .collect();
        assert_eq!(
            serde_json::to_vec(&(encoded, encoded_shared)).unwrap(),
            bytes
        );
        restored[0]
            .items
            .edit(item, |entry| entry.spec.name = "changed".into())
            .unwrap();
        restored[0]
            .actors
            .get_mut(&actor)
            .unwrap()
            .combat
            .as_mut()
            .unwrap()
            .spec
            .defense = 20;
        assert_eq!(restored[1].items[&item].spec.name, "token");
        assert_eq!(
            restored[1].actors[&actor]
                .combat
                .as_ref()
                .unwrap()
                .spec
                .defense,
            10
        );
    }

    #[test]
    fn common_restore_context_shares_decoded_pools_and_preserves_copy_on_write() {
        let mut game = Game::two_room_in_stone(42);
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        let item = game.place_item(at, "token".into()).unwrap();
        game.refresh_navigation();
        let mut shared = SharedState::default();
        let before = game.checkpoint(&mut shared);
        let original = game.clone();
        game.act(actor, crate::Action::Wait).unwrap();
        let after = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(vec![before, after], shared)).unwrap();
        let (snapshots, shared): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&shared);
        let mut restored: Vec<_> = snapshots
            .into_iter()
            .map(|s| context.restore(s).unwrap())
            .collect();
        assert_eq!(restored[0], original);
        assert_eq!(restored[1], game);
        assert!(restored[0].world.shares_storage(&restored[1].world));
        assert!(restored[0].navigation[&actor].shares_storage(&restored[1].navigation[&actor]));
        assert!(restored[0].items.shares_storage(&restored[1].items));
        assert!(restored[0].actors[&actor]
            .body
            .shares_storage(&restored[1].actors[&actor].body));
        drop(context);
        let mut encoded_shared = SharedState::default();
        let encoded: Vec<_> = restored
            .iter()
            .map(|g| g.checkpoint(&mut encoded_shared))
            .collect();
        assert_eq!(
            serde_json::to_vec(&(encoded, encoded_shared)).unwrap(),
            bytes
        );
        restored[0]
            .set_body(
                actor,
                crate::BodySpec {
                    mass: 99,
                    ..Default::default()
                },
            )
            .unwrap();
        restored[0]
            .items
            .edit(item, |item| item.spec.name = "edited".into())
            .unwrap();
        restored[0]
            .navigation
            .get_mut(&actor)
            .unwrap()
            .cells
            .remove(&at);
        restored[0]
            .world
            .add_region(tor_world::Region {
                id: RegionId(9),
                name: "new".into(),
                bounds: tor_world::Extent::new(3, 3, 1).unwrap(),
            })
            .unwrap();
        assert_eq!(restored[1], game);
        assert_eq!(restored[1].items[&item].spec.name, "token");
        assert_eq!(restored[1].actors[&actor].body.mass, 80);
    }

    #[test]
    fn body_pool_preserves_cell_order_eye_and_mass_and_rejects_invalid_snapshots() {
        let base = crate::BodySpec {
            cells: vec![[0, 0, 0], [1, 0, 0]],
            eye: [0, 0, 0],
            mass: 80,
        };
        let mut reordered = base.clone();
        reordered.cells.reverse();
        let mut eye = base.clone();
        eye.eye = [1, 0, 0];
        let mut mass = base.clone();
        mass.mass = 81;
        let pool = BTreeSet::from([
            Shared::new(base),
            Shared::new(reordered),
            Shared::new(eye),
            Shared::new(mass),
        ]);
        assert_eq!(pool.len(), 4);
        let mut game = Game::two_room_in_stone(42);
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut shared = SharedState::default();
        let broken = game.checkpoint(&mut shared);
        shared.actors[broken.actors]
            .get_mut(&actor)
            .unwrap()
            .body
            .eye = [1, 0, 0];
        let valid = game.checkpoint(&mut shared);
        let mut context = RestoreContext::new(&shared);
        assert!(context.restore(broken).is_none());
        assert_eq!(context.restore(valid), Some(game));
    }

    #[test]
    fn snapshots_preserve_complete_game_and_reject_invalid_shared_references() {
        let mut game = Game::two_room_in_stone(42);
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.refresh_navigation();
        let mut shared = SharedState::default();
        let before = game.checkpoint(&mut shared);
        assert_eq!(
            Game::restore_checkpoint(before, &shared),
            Some(game.clone())
        );
        game.act(actor, crate::Action::Wait).unwrap();
        let after = game.checkpoint(&mut shared);
        assert_eq!(Game::restore_checkpoint(after, &shared), Some(game.clone()));
        let mut broken = game.checkpoint(&mut shared);
        broken.navigation.insert(actor, usize::MAX);
        assert!(Game::restore_checkpoint(broken, &shared).is_none());
        let mut broken = game.checkpoint(&mut shared);
        broken.world = usize::MAX;
        assert!(Game::restore_checkpoint(broken, &shared).is_none());
        let mut broken = game.checkpoint(&mut shared);
        broken.actors = usize::MAX;
        assert!(Game::restore_checkpoint(broken, &shared).is_none());
    }
}

/// Format-6 navigation pools retain sharing when decoded, including between
/// independently equal region maps. No historical-format fallback is accepted.
mod navigation_regions {
    use super::*;
    use crate::navigation_map::RegionMap;
    use serde::{Deserializer, Serializer};
    use tor_world::{Direction, Location, RegionId};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Instance {
        places: Vec<usize>,
        cells: Vec<usize>,
        edges: Vec<usize>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Regions {
        places: Vec<RegionMap<Location, crate::PlaceName>>,
        cells: Vec<RegionMap<Location, bool>>,
        edges: Vec<EdgeRegion>,
        instances: Vec<Instance>,
    }

    type Edges = RegionMap<(Location, Direction), (Location, u8)>;

    /// Link directions, in mask bit order.
    const DIRECTIONS: [Direction; 6] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ];

    /// One pooled region of known links. Most links are plain: the ordinary
    /// neighbour in the same region with no rotation, fully determined by the
    /// start cell and direction. Those are stored as `[x, y, z, mask]` with bit
    /// `i` for `DIRECTIONS[i]`; only the rest are stored in full. Decoding
    /// accepts exactly one encoding per map.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EdgeRegion {
        region: RegionId,
        plain: Vec<[i32; 4]>,
        other: Vec<((Location, Direction), (Location, u8))>,
    }

    fn plain_target(from: Location, direction: Direction) -> Option<Location> {
        let (dx, dy, dz) = direction.delta();
        let p = from.position;
        Some(Location {
            position: tor_world::Position {
                x: p.x.checked_add(dx)?,
                y: p.y.checked_add(dy)?,
                z: p.z.checked_add(dz)?,
            },
            ..from
        })
    }

    fn is_plain(from: Location, direction: Direction, to: Location, turns: u8) -> bool {
        turns == 0 && plain_target(from, direction) == Some(to)
    }

    impl EdgeRegion {
        /// `map` is a pooled single-region map.
        fn encode(map: &Edges) -> Self {
            let mut plain = BTreeMap::<tor_world::Position, i32>::new();
            let mut other = Vec::new();
            let mut region = None;
            for (&(from, direction), &(to, turns)) in map.iter() {
                region = Some(from.region);
                match DIRECTIONS.iter().position(|d| *d == direction) {
                    Some(bit) if is_plain(from, direction, to, turns) => {
                        *plain.entry(from.position).or_default() |= 1 << bit;
                    }
                    _ => other.push(((from, direction), (to, turns))),
                }
            }
            Self {
                region: region.expect("pooled regions are nonempty"),
                plain: plain
                    .into_iter()
                    .map(|(p, mask)| [p.x, p.y, p.z, mask])
                    .collect(),
                other,
            }
        }

        fn decode(self) -> Option<Edges> {
            let mut map = Edges::default();
            let mut count = 0usize;
            let mut previous = None;
            for [x, y, z, mask] in self.plain {
                let position = tor_world::Position { x, y, z };
                if !(1..64).contains(&mask) || previous.is_some_and(|p| p >= position) {
                    return None;
                }
                previous = Some(position);
                let from = Location {
                    region: self.region,
                    position,
                };
                for (bit, direction) in DIRECTIONS.into_iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        map.insert((from, direction), (plain_target(from, direction)?, 0));
                        count += 1;
                    }
                }
            }
            for ((from, direction), (to, turns)) in self.other {
                if from.region != self.region
                    || (DIRECTIONS.contains(&direction) && is_plain(from, direction, to, turns))
                    || map.contains_key(&(from, direction))
                {
                    return None;
                }
                map.insert((from, direction), (to, turns));
                count += 1;
            }
            (map.iter().count() == count).then_some(map)
        }
    }

    pub(super) fn serialize<S: Serializer>(
        navigation: &[Navigation],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut saved = Regions {
            places: Vec::new(),
            cells: Vec::new(),
            edges: Vec::new(),
            instances: Vec::new(),
        };
        let mut places = BTreeMap::new();
        let mut cells = BTreeMap::new();
        let mut edges = BTreeMap::new();
        let mut edge_pool = Vec::new();
        for map in navigation {
            saved.instances.push(Instance {
                places: map
                    .places
                    .checkpoint_regions(&mut saved.places, &mut places),
                cells: map.cells.checkpoint_regions(&mut saved.cells, &mut cells),
                edges: map.edges.checkpoint_regions(&mut edge_pool, &mut edges),
            });
        }
        saved.edges = edge_pool.iter().map(EdgeRegion::encode).collect();
        saved.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Navigation>, D::Error> {
        let saved = Regions::deserialize(deserializer)?;
        let Some(edge_pool) = saved
            .edges
            .into_iter()
            .map(EdgeRegion::decode)
            .collect::<Option<Vec<_>>>()
        else {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation links",
            ));
        };
        if saved.places.iter().any(|map| !map.is_checkpoint_region())
            || saved.cells.iter().any(|map| !map.is_checkpoint_region())
            || edge_pool.iter().any(|map| !map.is_checkpoint_region())
        {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation region",
            ));
        }
        saved
            .instances
            .into_iter()
            .map(|instance| {
                let places = RegionMap::restore_regions(&instance.places, &saved.places);
                let cells = RegionMap::restore_regions(&instance.cells, &saved.cells);
                let edges = RegionMap::restore_regions(&instance.edges, &edge_pool);
                match (cells, edges, places) {
                    (Some(cells), Some(edges), Some(places)) => Ok(Navigation {
                        cells,
                        edges,
                        places,
                    }),
                    _ => Err(serde::de::Error::custom(
                        "invalid checkpoint navigation reference",
                    )),
                }
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tor_world::Position;

        fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
            Location {
                region: RegionId(region),
                position: Position { x, y, z },
            }
        }

        #[test]
        fn links_round_trip_with_plain_masks_and_unusual_links_in_full() {
            let mut map = Edges::default();
            map.insert((at(1, 0, 0, 0), Direction::East), (at(1, 1, 0, 0), 0));
            map.insert((at(1, 0, 0, 0), Direction::South), (at(1, 0, 1, 0), 0));
            map.insert((at(1, 0, 0, 1), Direction::Down), (at(1, 0, 0, 0), 0));
            // A portal, a rotated join, and a stair are stored in full.
            map.insert((at(1, 4, 0, 0), Direction::East), (at(2, 0, 0, 0), 0));
            map.insert((at(1, 0, 3, 0), Direction::West), (at(1, -1, 3, 0), 1));
            map.insert((at(1, 2, 2, 0), Direction::Up), (at(3, 2, 2, 0), 0));
            let encoded = EdgeRegion::encode(&map);
            assert_eq!(
                encoded.plain,
                vec![[0, 0, 0, 0b10 | 0b100], [0, 0, 1, 0b10_0000]]
            );
            assert_eq!(encoded.other.len(), 3);
            assert_eq!(encoded.decode(), Some(map));
        }

        #[test]
        fn non_canonical_or_malformed_links_are_rejected() {
            let plain = ((at(1, 0, 0, 0), Direction::East), (at(1, 1, 0, 0), 0));
            let decode = |plain: Vec<[i32; 4]>, other| {
                EdgeRegion {
                    region: RegionId(1),
                    plain,
                    other,
                }
                .decode()
            };
            // A plain link written in full has a second encoding.
            assert_eq!(decode(vec![], vec![plain]), None);
            // Duplicates, empty or out-of-range masks, and unsorted cells.
            assert_eq!(decode(vec![[0, 0, 0, 2]], vec![plain]), None);
            assert_eq!(decode(vec![[0, 0, 0, 0]], vec![]), None);
            assert_eq!(decode(vec![[0, 0, 0, 64]], vec![]), None);
            assert_eq!(decode(vec![[1, 0, 0, 2], [0, 0, 0, 2]], vec![]), None);
            // Links belong to the pooled region.
            let foreign = ((at(2, 4, 0, 0), Direction::East), (at(3, 0, 0, 0), 0));
            assert_eq!(decode(vec![], vec![foreign]), None);
        }
    }
}
