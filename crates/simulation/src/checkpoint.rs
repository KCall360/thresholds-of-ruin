//! Backend-only deterministic checkpoint state. Identical worlds and navigation
//! regions are encoded once across navigation maps and rewind boundaries. This module does not perform storage or I/O.
use crate::{travel::Navigation, Actor, ActorId, Game, Item, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{Shared, World};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    combat: crate::combat::CombatWorld,
    physics: crate::physics::Physics,
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
            combat: self.combat.clone(),
            physics: self.physics.clone(),
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
            combat: snapshot.combat,
            physics: snapshot.physics,
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
        if !game.physics_valid()
            || !game.combat_valid()
            || game.actors.is_empty()
            || game.next_actor_id == 0
            || game.next_item_id == 0
            || game.next_door_id == 0
            || game
                .next_actor()
                .is_some_and(|id| game.actors[&id].ready_at != game.tick)
            || game.actors.iter().any(|(id, actor)| {
                id.0 == 0
                    || actor
                        .combat
                        .as_ref()
                        .is_some_and(|c| !c.spec.valid() || c.hp > c.spec.max_hp)
                    || id.0 >= game.next_actor_id
                    || actor.orientation >= 24
                    || (actor.alive() && actor.ready_at < game.tick)
                    || !game.world.contains(actor.location)
                    || actor
                        .visited
                        .iter()
                        .any(|id| game.world.region(*id).is_none())
                    || !actor.body.valid()
                    || !actor.motion.valid()
                    || actor
                        .motion
                        .acceleration_remainder
                        .iter()
                        .any(|v| v.unsigned_abs() >= actor.body.cells.len() as u64)
                    || (actor.alive()
                        && game
                            .body_cells(actor.location, actor.orientation, &actor.body)
                            .is_none_or(|cells| {
                                cells.iter().any(|(at, _)| {
                                    !game.world.walkable(*at) || !occupied.insert(*at)
                                })
                            }))
            })
            || game.items.iter().any(|(id, item)| {
                id.0 == 0
                    || id.0 >= game.next_item_id
                    || item.quantity == 0
                    || (!item.spec.stackable && item.quantity != 1)
                    || !item.spec.valid()
                    || !item.motion.valid()
                    || item.motion.acceleration_remainder != [0; 3]
                    || item.orientation >= 24
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
        places: Vec<RegionMap<Location, String>>,
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
