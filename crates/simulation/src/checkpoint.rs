//! Backend-only deterministic checkpoint state. Identical worlds and navigation
//! regions are encoded once across navigation maps and rewind boundaries. This module does not perform storage or I/O.
use crate::{travel::Navigation, Actor, ActorId, Game, Item, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Shared, World};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    world: usize,
    material_surfaces: bool,
    navigation: BTreeMap<ActorId, usize>,
    seed: u64,
    tick: u64,
    actors: BTreeMap<ActorId, Actor>,
    items: usize,
    next_actor_id: u64,
    next_item_id: u64,
    next_door_id: u64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedState {
    #[serde(with = "tor_world::checkpoint_worlds")]
    worlds: Vec<World>,
    #[serde(with = "navigation_regions")]
    navigation: Vec<Navigation>,
    items: Vec<BTreeMap<ItemId, Item>>,
}

impl SharedState {
    pub fn valid_world_count(&self, maximum: usize) -> bool {
        !self.worlds.is_empty() && self.worlds.len() <= maximum
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
            world,
            material_surfaces: self.material_surfaces,
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
            actors: self.actors.clone(),
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
        }
    }

    pub fn restore_checkpoint(snapshot: Snapshot, shared: &SharedState) -> Option<Self> {
        let game = Self {
            world: Shared::new(shared.worlds.get(snapshot.world)?.clone()),
            material_surfaces: snapshot.material_surfaces,
            navigation: snapshot
                .navigation
                .into_iter()
                .map(|(actor, index)| {
                    Some((actor, Shared::new(shared.navigation.get(index)?.clone())))
                })
                .collect::<Option<_>>()?,
            seed: snapshot.seed,
            tick: snapshot.tick,
            actors: snapshot.actors,
            items: Shared::new(shared.items.get(snapshot.items)?.clone()),
            next_actor_id: snapshot.next_actor_id,
            next_item_id: snapshot.next_item_id,
            next_door_id: snapshot.next_door_id,
        };
        let mut occupied = BTreeSet::new();
        if game.actors.is_empty()
            || game.next_actor_id == 0
            || game.next_item_id == 0
            || game.next_door_id == 0
            || game.actors.values().map(|a| a.ready_at).min() != Some(game.tick)
            || game.actors.iter().any(|(id, actor)| {
                id.0 == 0
                    || id.0 >= game.next_actor_id
                    || actor.orientation >= 4
                    || actor.ready_at < game.tick
                    || !game.world.contains(actor.location)
                    || actor
                        .visited
                        .iter()
                        .any(|id| game.world.region(*id).is_none())
                    || !occupied.insert(actor.location)
            })
            || game.items.iter().any(|(id, item)| {
                id.0 == 0
                    || id.0 >= game.next_item_id
                    || match item.location {
                        ItemLocation::Ground(location) => !game.world.contains(location),
                        ItemLocation::Carried(actor) => !game.actors.contains_key(&actor),
                    }
            })
            || game.navigation.iter().any(|(id, navigation)| {
                !game.actors.contains_key(id) || !navigation.checkpoint_valid(&game.world)
            })
            || !game.world.checkpoint_valid(game.next_door_id)
        {
            return None;
        }
        Some(game)
    }

    pub fn checkpoint_actor_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.actors.keys().map(|id| id.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Location, Position, RegionId};

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
    }
}

/// Format-6 navigation pools retain sharing when decoded, including between
/// independently equal region maps. No historical-format fallback is accepted.
mod navigation_regions {
    use super::*;
    use crate::navigation_map::RegionMap;
    use serde::{Deserializer, Serializer};
    use tor_world::{Direction, Location};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Instance {
        cells: Vec<usize>,
        edges: Vec<usize>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Regions {
        cells: Vec<RegionMap<Location, bool>>,
        edges: Vec<RegionMap<(Location, Direction), (Location, u8)>>,
        instances: Vec<Instance>,
    }

    pub(super) fn serialize<S: Serializer>(
        navigation: &[Navigation],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut saved = Regions {
            cells: Vec::new(),
            edges: Vec::new(),
            instances: Vec::new(),
        };
        let mut cells = BTreeMap::new();
        let mut edges = BTreeMap::new();
        for map in navigation {
            saved.instances.push(Instance {
                cells: map.cells.checkpoint_regions(&mut saved.cells, &mut cells),
                edges: map.edges.checkpoint_regions(&mut saved.edges, &mut edges),
            });
        }
        saved.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Navigation>, D::Error> {
        let saved = Regions::deserialize(deserializer)?;
        if saved.cells.iter().any(|map| !map.is_checkpoint_region())
            || saved.edges.iter().any(|map| !map.is_checkpoint_region())
        {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation region",
            ));
        }
        saved
            .instances
            .into_iter()
            .map(|instance| {
                let cells = RegionMap::restore_regions(&instance.cells, &saved.cells);
                let edges = RegionMap::restore_regions(&instance.edges, &saved.edges);
                match (cells, edges) {
                    (Some(cells), Some(edges)) => Ok(Navigation { cells, edges }),
                    _ => Err(serde::de::Error::custom(
                        "invalid checkpoint navigation reference",
                    )),
                }
            })
            .collect()
    }
}
