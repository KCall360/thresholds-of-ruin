//! One region's share of the world, detached for region streaming. A detached
//! region keeps its metadata in the world, so references into it stay
//! checkable, while its terrain, doors and outgoing links live in the slice.
//! See `docs/region-streaming.md#region-lifecycle-contract`.
use super::*;
use serde::{Deserialize, Serialize};

/// Everything the world stores for one region. Links *into* the region stay
/// with their source regions; links *out of* it travel with the slice.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionSlice {
    region: Region,
    chamber: Option<Extent>,
    gravity: Option<[i32; 3]>,
    #[serde(with = "crate::checkpoint_map")]
    doors: BTreeMap<Location, Door>,
    /// Outgoing links with their crossing rotations.
    #[serde(with = "crate::checkpoint_map")]
    passages: BTreeMap<(Location, Direction), (Passage, u8)>,
    physical_vertical: BTreeSet<(Location, Direction)>,
    #[serde(with = "crate::checkpoint_map")]
    cell_gravity: BTreeMap<Location, [i32; 3]>,
    #[serde(with = "crate::checkpoint_map")]
    terrain: BTreeMap<Location, Terrain>,
    place_hints: BTreeSet<Location>,
}

impl RegionSlice {
    pub fn id(&self) -> RegionId {
        self.region.id
    }
    pub fn region(&self) -> &Region {
        &self.region
    }
    /// Identities of the doors in this region.
    pub fn door_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.doors.values().map(|door| door.id)
    }
    /// Structural validity, independent of the rest of the world.
    pub fn valid(&self) -> bool {
        let id = self.region.id;
        let inside = |at: &Location| at.region == id && self.region.bounds.contains(at.position);
        let (x, y, z) = self.region.bounds.dimensions();
        x > 0
            && y > 0
            && z > 0
            && self.gravity.is_none_or(valid_gravity)
            && self.doors.iter().all(|(at, door)| {
                inside(at) && door.id > 0 && (1..=MAX_DOOR_HEIGHT).contains(&door.height)
            })
            && self
                .passages
                .iter()
                .all(|((from, direction), (passage, turns))| {
                    inside(from)
                        && passage.from == *from
                        && passage.direction == *direction
                        && *turns < 24
                })
            && self
                .physical_vertical
                .iter()
                .all(|key| self.passages.contains_key(key))
            && self
                .cell_gravity
                .iter()
                .all(|(at, g)| inside(at) && valid_gravity(*g))
            && self.terrain.keys().all(inside)
            && self.place_hints.iter().all(inside)
    }
}

/// Every key of `map` in `region`, removed and returned in order.
fn take_region<K: Ord + Clone, V: Clone>(
    map: &mut Shared<BTreeMap<K, V>>,
    region: RegionId,
    key_region: impl Fn(&K) -> RegionId,
) -> BTreeMap<K, V> {
    let keys: Vec<K> = map
        .keys()
        .filter(|key| key_region(key) == region)
        .cloned()
        .collect();
    if keys.is_empty() {
        return BTreeMap::new();
    }
    keys.into_iter()
        .map(|key| {
            let value = map.remove(&key).expect("listed key");
            (key, value)
        })
        .collect()
}

fn take_set<K: Ord + Clone>(
    set: &mut Shared<BTreeSet<K>>,
    region: RegionId,
    key_region: impl Fn(&K) -> RegionId,
) -> BTreeSet<K> {
    let keys: BTreeSet<K> = set
        .iter()
        .filter(|key| key_region(key) == region)
        .cloned()
        .collect();
    if !keys.is_empty() {
        set.retain(|key| !keys.contains(key));
    }
    keys
}

impl World {
    /// Whether `region` is loaded or detached: known to this world either way.
    pub fn knows_region(&self, region: RegionId) -> bool {
        self.regions.contains_key(&region) || self.absent.contains_key(&region)
    }

    /// Whether `location` lies inside a loaded or detached region. References
    /// held as knowledge (memory, objectives) are checked with this; anything
    /// that acts on terrain uses [`World::contains`].
    pub fn knows(&self, location: Location) -> bool {
        self.contains(location)
            || self
                .absent
                .get(&location.region)
                .is_some_and(|region| region.bounds.contains(location.position))
    }

    /// Loaded regions, in id order.
    pub fn loaded_regions(&self) -> impl Iterator<Item = RegionId> + '_ {
        self.regions.keys().copied()
    }

    /// Detached regions, in id order.
    pub fn detached_regions(&self) -> impl Iterator<Item = RegionId> + '_ {
        self.absent.keys().copied()
    }

    /// Regions that a link out of `region` leads to, in id order. Rim
    /// projection also follows these links, so they cover every region a
    /// step from `region` can reach.
    pub fn linked_regions(&self, region: RegionId) -> BTreeSet<RegionId> {
        self.exits(region).map(|p| p.to.region).collect()
    }

    /// Remove a loaded region's content, keeping its metadata so references
    /// into it stay checkable. Links into it from other regions stay, and
    /// lead nowhere until it's attached again.
    pub fn detach_region(&mut self, id: RegionId) -> Result<RegionSlice, WorldError> {
        let region = self
            .regions
            .get(&id)
            .cloned()
            .ok_or(WorldError::InvalidEndpoint)?;
        // Exhaustive, like `checkpoint::same_geometry`: new world state needs
        // an explicit decision about whether a region slice owns it.
        let World {
            doors,
            regions,
            passages,
            rotations,
            physical_vertical,
            region_gravity,
            cell_gravity,
            terrain,
            chambers,
            place_hints,
            absent,
            sight,
        } = self;
        let by_location = |at: &Location| at.region;
        let links = take_region(passages, id, |key| key.0.region);
        let mut turns = take_region(rotations, id, |key| key.0.region);
        let slice = RegionSlice {
            chamber: chambers.get(&id).copied(),
            gravity: region_gravity.get(&id).copied(),
            doors: take_region(doors, id, by_location),
            passages: links
                .into_iter()
                .map(|(key, passage)| {
                    let rotation = turns.remove(&key).unwrap_or(0);
                    (key, (passage, rotation))
                })
                .collect(),
            physical_vertical: take_set(physical_vertical, id, |key| key.0.region),
            cell_gravity: take_region(cell_gravity, id, by_location),
            terrain: take_region(terrain, id, by_location),
            place_hints: take_set(place_hints, id, by_location),
            region,
        };
        if slice.chamber.is_some() {
            chambers.remove(&id);
        }
        if slice.gravity.is_some() {
            region_gravity.remove(&id);
        }
        regions.remove(&id);
        absent.insert(id, slice.region.clone());
        sight.topology_changed();
        Ok(slice)
    }

    /// Restore a detached region exactly as it was detached. Fails, changing
    /// nothing, unless the region is detached with the same metadata and none
    /// of its door identities is in use.
    pub fn attach_region(&mut self, slice: RegionSlice) -> Result<(), WorldError> {
        let id = slice.id();
        if self.absent.get(&id) != Some(&slice.region) || !slice.valid() {
            return Err(WorldError::InvalidEndpoint);
        }
        if slice
            .door_ids()
            .any(|door| self.door_location(door).is_some())
        {
            return Err(WorldError::InvalidEndpoint);
        }
        let RegionSlice {
            region,
            chamber,
            gravity,
            doors,
            passages,
            physical_vertical,
            cell_gravity,
            terrain,
            place_hints,
        } = slice;
        self.absent.remove(&id);
        self.regions.insert(id, region);
        if let Some(chamber) = chamber {
            self.chambers.insert(id, chamber);
        }
        if let Some(gravity) = gravity {
            self.region_gravity.insert(id, gravity);
        }
        if !doors.is_empty() {
            self.doors.extend(doors);
        }
        if !passages.is_empty() {
            for (key, (passage, turns)) in passages {
                self.passages.insert(key, passage);
                self.rotations.insert(key, turns);
            }
        }
        if !physical_vertical.is_empty() {
            self.physical_vertical.extend(physical_vertical);
        }
        if !cell_gravity.is_empty() {
            self.cell_gravity.extend(cell_gravity);
        }
        if !terrain.is_empty() {
            self.terrain.extend(terrain);
        }
        if !place_hints.is_empty() {
            self.place_hints.extend(place_hints);
        }
        self.sight.topology_changed();
        Ok(())
    }
}
