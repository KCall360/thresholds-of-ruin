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
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        with = "crate::checkpoint_map"
    )]
    named_stairs: BTreeMap<(Location, Direction), NamedAnchor>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    named_anchors: BTreeMap<String, Position>,
    region: Region,
    chamber: Option<Extent>,
    gravity: Option<[i32; 3]>,
    #[serde(with = "crate::checkpoint_map")]
    doors: BTreeMap<Location, Door>,
    /// Outgoing links with their crossing rotations.
    #[serde(with = "crate::checkpoint_map")]
    passages: BTreeMap<(Location, Direction), (Passage, u8)>,
    physical_vertical: BTreeSet<(Location, Direction)>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    solid_boundaries: BTreeSet<(Location, Direction)>,
    #[serde(with = "crate::checkpoint_map")]
    cell_gravity: BTreeMap<Location, [i32; 3]>,
    #[serde(with = "crate::checkpoint_map")]
    terrain: BTreeMap<Location, Terrain>,
    #[serde(with = "crate::checkpoint_map")]
    place_hints: BTreeMap<Location, String>,
}

impl RegionSlice {
    pub fn id(&self) -> RegionId {
        self.region.id
    }
    pub fn region(&self) -> &Region {
        &self.region
    }
    /// Identities of the doors in this region.
    /// Regions this region's links lead to, in id order.
    pub fn linked_regions(&self) -> BTreeSet<RegionId> {
        self.passages
            .values()
            .map(|(p, _)| p.to.region)
            .chain(self.named_stairs.values().map(|to| to.region))
            .collect()
    }
    pub fn door_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.doors.values().map(|door| door.id)
    }
    /// Structural validity, independent of the rest of the world.
    pub fn valid(&self) -> bool {
        let id = self.region.id;
        let inside = |at: &Location| at.region == id && self.region.bounds.contains(at.position);
        let (x, y, z) = self.region.bounds.dimensions();
        x > 0
            && self.named_anchors.iter().all(|(name, position)| {
                valid_anchor_name(name) && self.region.bounds.contains(*position)
            })
            && self.named_stairs.iter().all(|((from, direction), to)| {
                inside(from)
                    && matches!(direction, Direction::Up | Direction::Down)
                    && valid_anchor_name(&to.name)
                    && !self.passages.contains_key(&(*from, *direction))
            })
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
            && self.solid_boundaries.iter().all(|key| {
                !matches!(key.1, Direction::Up | Direction::Down) && self.passages.contains_key(key)
            })
            && self
                .cell_gravity
                .iter()
                .all(|(at, g)| inside(at) && valid_gravity(*g))
            && self.terrain.keys().all(inside)
            && self.place_hints.keys().all(inside)
    }
}

/// A world-table key that sorts by region first, so one region's keys are a
/// contiguous range starting at [`RegionKey::first`].
trait RegionKey: Ord + Clone {
    fn region(&self) -> RegionId;
    /// The smallest key in `region`.
    fn first(region: RegionId) -> Self;
}

impl RegionKey for Location {
    fn region(&self) -> RegionId {
        self.region
    }
    fn first(region: RegionId) -> Self {
        Location {
            region,
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        }
    }
}

impl RegionKey for (Location, Direction) {
    fn region(&self) -> RegionId {
        self.0.region
    }
    fn first(region: RegionId) -> Self {
        (Location::first(region), Direction::North)
    }
}

/// `region`'s keys in a sorted collection's key order, found without visiting
/// other regions.
fn region_keys<'a, K: RegionKey + 'a>(
    range: impl Iterator<Item = &'a K>,
    region: RegionId,
) -> Vec<K> {
    range
        .take_while(|key| key.region() == region)
        .cloned()
        .collect()
}

/// Every key of `map` in `region`, removed and returned in order. Costs a
/// lookup per key, and copies nothing when the region has no keys.
fn take_region<K: RegionKey, V: Clone>(
    map: &mut Shared<BTreeMap<K, V>>,
    region: RegionId,
) -> BTreeMap<K, V> {
    let keys = region_keys(map.range(K::first(region)..).map(|(k, _)| k), region);
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

fn take_set<K: RegionKey>(set: &mut Shared<BTreeSet<K>>, region: RegionId) -> BTreeSet<K> {
    let keys = region_keys(set.range(K::first(region)..), region);
    if !keys.is_empty() {
        for key in &keys {
            set.remove(key);
        }
    }
    keys.into_iter().collect()
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

    /// Know a region that hasn't been built: like a detached region, its
    /// metadata is here and its content arrives later as a [`RegionSlice`].
    /// A chamber's stored bounds include its stone shell, as
    /// [`World::add_chamber`] makes them.
    pub fn add_unbuilt_region(
        &mut self,
        mut region: Region,
        chamber: bool,
    ) -> Result<(), WorldError> {
        if self.knows_region(region.id) {
            return Err(WorldError::DuplicateRegion);
        }
        if chamber {
            region.bounds = region
                .bounds
                .with_shell()
                .ok_or(WorldError::InvalidEndpoint)?;
        }
        self.absent.insert(region.id, region);
        Ok(())
    }

    /// A loaded, detached or unbuilt region's metadata. Knowledge of a
    /// region (its name, say) outlives its content being loaded.
    pub fn known_region(&self, id: RegionId) -> Option<&Region> {
        self.region(id).or_else(|| self.absent.get(&id))
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
        self.exits(region)
            .map(|p| p.to.region)
            .chain(
                self.named_stairs
                    .range((Location::first(region), Direction::North)..)
                    .take_while(|((at, _), _)| at.region == region)
                    .map(|(_, to)| to.region),
            )
            .collect()
    }

    /// Publish a prepared record's structural anchors before any member attaches.
    pub fn publish_region_anchors(&mut self, slice: &RegionSlice) -> Result<(), WorldError> {
        if !slice.valid() || self.known_region(slice.id()) != Some(slice.region()) {
            return Err(WorldError::InvalidEndpoint);
        }
        self.register_named_anchors(slice.id(), slice.named_anchors.clone())
    }

    /// Remove a loaded region's content, keeping its metadata so references
    /// into it stay checkable. Links into it from other regions stay, and
    /// lead nowhere until it's attached again.
    ///
    /// Only sight scenes that list this region are invalidated. That's exact:
    /// a scene lists every region it entered and every region linked from
    /// them, and links into this region stay with their source regions.
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
            solid_boundaries,
            named_stairs,
            named_anchors,
            region_gravity,
            cell_gravity,
            terrain,
            chambers,
            place_hints,
            absent,
            sight,
        } = self;
        let links = take_region(passages, id);
        let mut turns = take_region(rotations, id);
        let slice = RegionSlice {
            named_stairs: take_region(named_stairs, id),
            named_anchors: named_anchors.get(&id).cloned().unwrap_or_default(),
            chamber: chambers.get(&id).copied(),
            gravity: region_gravity.get(&id).copied(),
            doors: take_region(doors, id),
            passages: links
                .into_iter()
                .map(|(key, passage)| {
                    let rotation = turns.remove(&key).unwrap_or(0);
                    (key, (passage, rotation))
                })
                .collect(),
            physical_vertical: take_set(physical_vertical, id),
            solid_boundaries: take_set(solid_boundaries, id),
            cell_gravity: take_region(cell_gravity, id),
            terrain: take_region(terrain, id),
            place_hints: take_region(place_hints, id),
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
        sight.region_changed(id);
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
        self.publish_region_anchors(&slice)?;
        let RegionSlice {
            named_stairs,
            named_anchors: _,
            region,
            chamber,
            gravity,
            doors,
            passages,
            physical_vertical,
            solid_boundaries,
            cell_gravity,
            terrain,
            place_hints,
        } = slice;
        self.absent.remove(&id);
        if !named_stairs.is_empty() {
            self.named_stairs.extend(named_stairs);
        }
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
        if !solid_boundaries.is_empty() {
            self.solid_boundaries.extend(solid_boundaries);
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
        self.sight.region_changed(id);
        Ok(())
    }
}
