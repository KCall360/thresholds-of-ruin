//! Last-disclosed appearance and witnessed placement, never movement authority.
//! Projection receives saved knowledge and a disclosed scene, not a World.
use crate::navigation_map::{RegionKey, RegionMap};
use crate::{ActorId, Game, GameError, ItemClass};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tor_world::{Direction, Location, Position, RegionId, Shared, SightCell};

const MAX_STATES: usize = 32_768;
const MAX_RELATIONS: usize = 262_144;
const MAX_SEEKS: usize = 131_072;
const MAX_OUTPUT: usize = 12_474;
const DIRECTIONS: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapDoorAppearance {
    pub name: String,
    pub description: String,
    pub open: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapTrapAppearance {
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub family: Option<tor_world::TrapFamily>,
}
impl<'de> Deserialize<'de> for MapTrapAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            family: Option<tor_world::TrapFamily>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            family: fields.family,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapItemAppearance {
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub presentation: Option<crate::appearance_catalog::ObjectPresentation>,
    pub class: ItemClass,
    pub description: String,
    pub name: String,
    pub appearance: String,
    pub identified: bool,
    pub quantity: u64,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub asset: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MapAppearance {
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub object_presentation: Option<crate::appearance_catalog::ObjectPresentation>,
    pub wall: bool,
    pub material: String,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub asset: Option<String>,
    pub place_hint: bool,
    pub stairs_up: bool,
    pub stairs_down: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub door: Option<MapDoorAppearance>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub trap: Option<MapTrapAppearance>,
    #[serde(deserialize_with = "read_items")]
    pub ground_items: Vec<MapItemAppearance>,
}

fn read_items<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<MapItemAppearance>, D::Error> {
    // Existing ground stacks have no independent count ceiling. The enclosing
    // save frame bounds decoding; presentation budgets must not reject a turn.
    Vec::<MapItemAppearance>::deserialize(d)
}

fn read_map_text<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let text = String::deserialize(d)?;
    if text.len() > 4096 || text.chars().any(char::is_control) {
        return Err(serde::de::Error::custom("invalid explored-map text"));
    }
    Ok(text)
}

fn read_map_asset<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    // Preserve the ordinary source value, including values too large to page.
    Option::<String>::deserialize(d)
}

fn source_item_label(value: &str) -> bool {
    !value.is_empty() && value.len() <= 80 && !value.chars().any(char::is_control)
}

fn read_item_label<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if !source_item_label(&value) {
        return Err(serde::de::Error::custom("invalid explored-map item label"));
    }
    Ok(value)
}

impl<'de> Deserialize<'de> for MapDoorAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "read_map_text")]
            name: String,
            #[serde(deserialize_with = "read_map_text")]
            description: String,
            open: bool,
            #[serde(deserialize_with = "read_map_asset")]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            name: fields.name,
            description: fields.description,
            open: fields.open,
            asset: fields.asset,
        })
    }
}

impl<'de> Deserialize<'de> for MapItemAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            presentation: Option<crate::appearance_catalog::ObjectPresentation>,
            class: ItemClass,
            #[serde(deserialize_with = "read_map_text")]
            description: String,
            #[serde(deserialize_with = "read_item_label")]
            name: String,
            #[serde(deserialize_with = "read_item_label")]
            appearance: String,
            identified: bool,
            quantity: u64,
            #[serde(deserialize_with = "read_map_asset")]
            asset: Option<String>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        if fields.quantity == 0
            || (fields.presentation.is_some()
                && (fields.name != "object"
                    || fields.appearance != "object"
                    || !fields.description.is_empty()
                    || fields.asset.is_some()))
        {
            return Err(serde::de::Error::custom("invalid explored-map item"));
        }
        Ok(Self {
            presentation: fields.presentation,
            class: fields.class,
            description: fields.description,
            name: fields.name,
            appearance: fields.appearance,
            identified: fields.identified,
            quantity: fields.quantity,
            asset: fields.asset,
        })
    }
}

impl<'de> Deserialize<'de> for MapAppearance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            object_presentation: Option<crate::appearance_catalog::ObjectPresentation>,
            wall: bool,
            #[serde(deserialize_with = "read_map_text")]
            material: String,
            #[serde(deserialize_with = "read_map_asset")]
            asset: Option<String>,
            place_hint: bool,
            stairs_up: bool,
            stairs_down: bool,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            door: Option<MapDoorAppearance>,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            trap: Option<MapTrapAppearance>,
            #[serde(deserialize_with = "read_items")]
            ground_items: Vec<MapItemAppearance>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            object_presentation: fields.object_presentation,
            wall: fields.wall,
            material: fields.material,
            asset: fields.asset,
            place_hint: fields.place_hint,
            stairs_up: fields.stairs_up,
            stairs_down: fields.stairs_down,
            door: fields.door,
            trap: fields.trap,
            ground_items: fields.ground_items,
        })
    }
}

impl<'de> Deserialize<'de> for ExploredMap {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            cells: RegionMap<Location, Shared<MapAppearance>>,
            neighbors: RegionMap<(Location, Direction), Neighbor>,
            witnesses: RegionMap<Location, Witness>,
            children: RegionMap<ChildKey, u8>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let map = Self {
            cells: fields.cells,
            neighbors: fields.neighbors,
            witnesses: fields.witnesses,
            children: fields.children,
        };
        if !map.structurally_valid() {
            return Err(serde::de::Error::custom("invalid explored-map evidence"));
        }
        Ok(map)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapCellInput {
    pub location: Location,
    pub appearance: MapAppearance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectedMapCell {
    pub location: Location,
    pub position: Position,
    pub appearance: Shared<MapAppearance>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapWindow {
    pub min: Position,
    pub max: Position,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapProjection {
    pub cells: Vec<ProjectedMapCell>,
    pub complete: bool,
    pub ambiguous: bool,
    pub limited: bool,
    /// Wire materialization would exceed its metadata budget, rather than a
    /// geometry-work cap. Hosts may report a correlated ResourceLimit for this.
    pub appearance_limited: bool,
    pub work: MapWorkCounts,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapWorkCounts {
    pub observed_cells: usize,
    pub placement_proposals: usize,
    pub appearance_changes: usize,
    pub neighbor_changes: usize,
    pub witness_changes: usize,
    pub owner_copies: usize,
    pub directory_entries_copied: usize,
    pub bucket_entries_copied: usize,
    pub appearance_payload_copies: usize,
    pub projection_states: usize,
    pub candidate_relations: usize,
    pub inverse_checks: usize,
    pub range_seeks: usize,
    pub reachability_edges: usize,
    pub validation_cells: usize,
    pub validation_relations: usize,
    pub world_location_checks: usize,
    pub validation_directory_entries_copied: usize,
    pub pool_bucket_allocations: usize,
    pub pool_identity_hits: usize,
    pub pool_compared_entries: usize,
}

thread_local! { static COUNTS: std::cell::Cell<MapWorkCounts> = const {
    std::cell::Cell::new(MapWorkCounts { observed_cells:0, placement_proposals:0,
        appearance_changes:0, neighbor_changes:0, witness_changes:0, owner_copies:0,
        directory_entries_copied:0, bucket_entries_copied:0, appearance_payload_copies:0,
        projection_states:0, candidate_relations:0, inverse_checks:0, range_seeks:0,
        reachability_edges:0, validation_cells:0, validation_relations:0,
        world_location_checks:0, validation_directory_entries_copied:0,
        pool_bucket_allocations:0, pool_identity_hits:0, pool_compared_entries:0 })
}; }

pub fn map_work_counts() -> MapWorkCounts {
    COUNTS.with(std::cell::Cell::get)
}
pub fn reset_map_work_counts() {
    COUNTS.with(|c| c.set(MapWorkCounts::default()));
}
pub(crate) fn count(f: impl FnOnce(&mut MapWorkCounts)) {
    COUNTS.with(|c| {
        let mut n = c.get();
        f(&mut n);
        c.set(n);
    });
}

pub(crate) fn count_map_validation(cells: usize, relations: usize, world_locations: usize) {
    count(|n| {
        n.validation_cells += cells;
        n.validation_relations += relations;
        n.world_location_checks += world_locations;
    });
}

type RootProofs = BTreeMap<ActorId, tor_world::SharedIdentity<ExploredMap>>;

/// Derived Game-owned validation evidence, never serialized or compared as
/// gameplay state. Weak roots do not keep detached appearance payloads loaded.
#[derive(Clone, Debug, Default)]
pub(crate) struct MapValidationCache {
    loaded: Shared<RootProofs>,
    detached: Shared<BTreeMap<crate::RecordId, Shared<RootProofs>>>,
}
impl PartialEq for MapValidationCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for MapValidationCache {}
impl MapValidationCache {
    fn loaded_mut(&mut self) -> &mut RootProofs {
        if self.loaded.is_shared() {
            count(|n| n.validation_directory_entries_copied += self.loaded.len());
        }
        &mut self.loaded
    }
    fn detached_mut(&mut self) -> &mut BTreeMap<crate::RecordId, Shared<RootProofs>> {
        if self.detached.is_shared() {
            count(|n| n.validation_directory_entries_copied += self.detached.len());
        }
        &mut self.detached
    }
    pub(crate) fn loaded_matches(&self, actor: ActorId, map: &Shared<ExploredMap>) -> bool {
        self.loaded
            .get(&actor)
            .is_some_and(|proof| proof.matches(map))
    }
    pub(crate) fn accept_loaded(&mut self, actor: ActorId, map: &Shared<ExploredMap>) {
        if !self.loaded_matches(actor, map) {
            self.loaded_mut().insert(actor, map.weak_identity());
        }
    }
    pub(crate) fn forget_loaded(&mut self, actor: ActorId) {
        if self.loaded.contains_key(&actor) {
            self.loaded_mut().remove(&actor);
        }
    }
    pub(crate) fn detach(
        &mut self,
        id: crate::RecordId,
        maps: &BTreeMap<ActorId, Shared<ExploredMap>>,
    ) {
        let valid = maps
            .iter()
            .all(|(actor, map)| self.loaded_matches(*actor, map));
        let roots = valid.then(|| {
            maps.iter()
                .map(|(actor, map)| (*actor, map.weak_identity()))
                .collect()
        });
        for actor in maps.keys() {
            self.forget_loaded(*actor);
        }
        if let Some(roots) = roots {
            if !maps.is_empty() {
                self.detached_mut().insert(id, Shared::new(roots));
            }
        }
    }
    pub(crate) fn detached_matches(
        &self,
        id: crate::RecordId,
        maps: &BTreeMap<ActorId, Shared<ExploredMap>>,
    ) -> bool {
        maps.is_empty()
            || self.detached.get(&id).is_some_and(|proofs| {
                proofs.len() == maps.len()
                    && maps.iter().all(|(actor, map)| {
                        proofs.get(actor).is_some_and(|proof| proof.matches(map))
                    })
            })
    }
    pub(crate) fn attach(
        &mut self,
        id: crate::RecordId,
        maps: &BTreeMap<ActorId, Shared<ExploredMap>>,
        trusted: bool,
    ) {
        if self.detached.contains_key(&id) {
            self.detached_mut().remove(&id);
        }
        if trusted {
            for (actor, map) in maps {
                self.accept_loaded(*actor, map);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Neighbor {
    Placed { target: Location, rotation: u8 },
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Witness {
    Placed {
        anchor: Location,
        delta: [i64; 3],
        rotation: u8,
    },
    Ambiguous,
}

pub(crate) type ChildKey = (Location, [i64; 3], Location);
impl RegionKey for ChildKey {
    fn region(&self) -> RegionId {
        self.0.region
    }
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExploredMap {
    pub(crate) cells: RegionMap<Location, Shared<MapAppearance>>,
    pub(crate) neighbors: RegionMap<(Location, Direction), Neighbor>,
    pub(crate) witnesses: RegionMap<Location, Witness>,
    pub(crate) children: RegionMap<ChildKey, u8>,
}

impl MapAppearance {
    pub(crate) fn valid_for_catalog(
        &self,
        catalog: &crate::appearance_catalog::PresentationCatalog,
    ) -> bool {
        self.object_presentation
            .as_ref()
            .is_none_or(|p| catalog.contains_object(p))
            && self.ground_items.iter().all(|i| {
                i.presentation
                    .as_ref()
                    .is_none_or(|p| catalog.contains_object(p))
            })
    }
    pub(crate) fn valid(&self) -> bool {
        let text = |s: &str| s.len() <= 4096 && !s.chars().any(char::is_control);
        text(&self.material)
            && self
                .object_presentation
                .as_ref()
                .is_none_or(|p| p.valid() && !self.wall && (self.stairs_up || self.stairs_down))
            && self
                .door
                .as_ref()
                .is_none_or(|d| text(&d.name) && text(&d.description))
            && self.ground_items.iter().all(|i| {
                i.quantity > 0
                    && i.presentation.as_ref().is_none_or(|p| {
                        p.valid()
                            && i.name == "object"
                            && i.appearance == "object"
                            && i.description.is_empty()
                            && i.asset.is_none()
                    })
                    && source_item_label(&i.name)
                    && text(&i.description)
                    && source_item_label(&i.appearance)
            })
    }
}

impl ExploredMap {
    pub(crate) fn checkpoint_valid(&self, world: &tor_world::World) -> bool {
        self.structurally_valid() && self.world_locations_valid(world)
    }
    pub(crate) fn world_locations_valid(&self, world: &tor_world::World) -> bool {
        self.cells.keys().all(|at| {
            count(|n| n.world_location_checks += 1);
            world
                .known_region(at.region)
                .is_some_and(|region| region.bounds.contains(at.position))
        })
    }
    pub(crate) fn structurally_valid(&self) -> bool {
        self.cells.iter().all(|(_, appearance)| {
            count(|n| n.validation_cells += 1);
            appearance.valid()
        }) && self.neighbors.iter().all(|((source, direction), edge)| {
            count(|n| n.validation_relations += 1);
            DIRECTIONS.contains(direction)
                && self.cells.contains_key(source)
                && match edge {
                    Neighbor::Ambiguous => true,
                    Neighbor::Placed { target, rotation } => {
                        *rotation < 24 && self.cells.contains_key(target)
                    }
                }
        }) && self.witnesses.iter().all(|(child, witness)| {
            count(|n| n.validation_relations += 1);
            self.cells.contains_key(child)
                && match witness {
                    Witness::Ambiguous => true,
                    Witness::Placed {
                        anchor,
                        delta,
                        rotation,
                    } => {
                        *rotation < 24
                            && self.cells.contains_key(anchor)
                            && checked_rotate(*rotation, *delta).is_some()
                            && delta.iter().all(|n| n.checked_neg().is_some())
                            && self.children.get(&(*anchor, *delta, *child)) == Some(rotation)
                    }
                }
        }) && self
            .children
            .iter()
            .all(|((anchor, delta, child), rotation)| {
                count(|n| n.validation_relations += 1);
                self.witnesses.get(child)
                    == Some(&Witness::Placed {
                        anchor: *anchor,
                        delta: *delta,
                        rotation: *rotation,
                    })
            })
    }
}

/// Cube rotations are signed permutations. Validate signs without ever negating MIN.
fn checked_rotate(rotation: u8, value: [i64; 3]) -> Option<[i64; 3]> {
    if rotation >= 24 {
        return None;
    }
    let mut out = [0; 3];
    for axis in 0..3 {
        let mut basis = [0; 3];
        basis[axis] = 1;
        let rotated = tor_world::rotate_vector(rotation, basis);
        for target in 0..3 {
            if rotated[target] != 0 {
                out[target] = if rotated[target] == 1 {
                    value[axis]
                } else {
                    value[axis].checked_neg()?
                };
            }
        }
    }
    Some(out)
}

impl Clone for ExploredMap {
    fn clone(&self) -> Self {
        count(|n| {
            n.owner_copies += 1;
            n.directory_entries_copied += self.cells.region_count()
                + self.neighbors.region_count()
                + self.witnesses.region_count()
                + self.children.region_count();
        });
        Self {
            cells: self.cells.clone(),
            neighbors: self.neighbors.clone(),
            witnesses: self.witnesses.clone(),
            children: self.children.clone(),
        }
    }
}

/// Opaque immutable ownership proof for a bounded transfer. Cloning is O(1).
#[derive(Clone, Debug)]
pub struct MapKnowledgeRoot(Option<Shared<ExploredMap>>);

fn vector(p: Position) -> [i64; 3] {
    [i64::from(p.x), i64::from(p.y), i64::from(p.z)]
}
fn offset(p: Position, delta: [i64; 3]) -> Option<Position> {
    let p = vector(p);
    Some(Position {
        x: i32::try_from(p[0].checked_add(delta[0])?).ok()?,
        y: i32::try_from(p[1].checked_add(delta[1])?).ok()?,
        z: i32::try_from(p[2].checked_add(delta[2])?).ok()?,
    })
}
fn difference(a: Position, b: Position) -> [i64; 3] {
    let a = vector(a);
    let b = vector(b);
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn direction_delta(d: Direction) -> [i64; 3] {
    let (x, y, z) = d.delta();
    [i64::from(x), i64::from(y), i64::from(z)]
}
fn opposite(d: Direction) -> Direction {
    Direction::from_delta(direction_delta(d).map(|v| -v)).expect("cardinal direction")
}
fn propose<K: Ord, V: PartialEq>(map: &mut BTreeMap<K, V>, key: K, value: V, ambiguous: V) {
    match map.entry(key) {
        std::collections::btree_map::Entry::Vacant(e) => {
            e.insert(value);
        }
        std::collections::btree_map::Entry::Occupied(mut e) if *e.get() != value => {
            e.insert(ambiguous);
        }
        _ => {}
    }
}
fn root(parents: &mut [usize], mut at: usize) -> usize {
    while parents[at] != at {
        parents[at] = parents[parents[at]];
        at = parents[at];
    }
    at
}

impl Game {
    /// Dependency identities only skip work. A changed identity conservatively
    /// rebuilds ordinary disclosure; identities never enter saved knowledge.
    pub fn map_disclosure_changed_since(&self, before: &Game, id: ActorId) -> bool {
        !self
            .map_disclosure_changed_actors_since(before, [id])
            .is_empty()
    }

    /// Compute shared optical dependencies once, preserving caller actor order.
    /// This is not caller-supplied authority: all skip proofs use the two Games.
    pub fn map_disclosure_changed_actors_since(
        &self,
        before: &Game,
        actors: impl IntoIterator<Item = ActorId>,
    ) -> Vec<ActorId> {
        let now = self.emitter_signature();
        let prior = before.emitter_signature();
        let shared_changed = !self.world.shares_storage(&before.world)
            || now != prior
            || (!self.items.shares_storage(&before.items)
                && (now.0.is_empty() || !self.map_items_equal_except_burning_fuel(before)));
        actors
            .into_iter()
            .filter(|id| self.map_actor_changed_since(before, *id, shared_changed))
            .collect()
    }

    fn map_actor_changed_since(&self, before: &Game, id: ActorId, shared_changed: bool) -> bool {
        if !self.explored_maps.contains_key(&id) || shared_changed {
            return true;
        }
        let (Some(a), Some(b)) = (self.actors.get(&id), before.actors.get(&id)) else {
            return true;
        };
        self.projected_hallucination(id)
            .map(|c| c.map(|c| c.phase()))
            != before
                .projected_hallucination(id)
                .map(|c| c.map(|c| c.phase()))
            || self.actor_scene_witness(id) != before.actor_scene_witness(id)
            || a.location != b.location
            || a.orientation != b.orientation
            || !a.body.shares_storage(&b.body)
            || a.alive() != b.alive()
            || !a.knowledge.shares_storage(&b.knowledge)
            || (self.combat.selected == Some(id)) != (before.combat.selected == Some(id))
    }

    fn map_items_equal_except_burning_fuel(&self, before: &Game) -> bool {
        if self.items.len() != before.items.len() {
            return false;
        }
        self.items
            .iter()
            .zip(before.items.iter())
            .all(|((id, item), (prior_id, old))| {
                if id != prior_id {
                    return false;
                }
                // Exhaustive field inventory: a new item field must be classified,
                // rather than silently inheriting this narrowly optical exception.
                let crate::Item {
                    corpse,
                    condition,
                    charges,
                    relocation_elapsed,
                    fuel,
                    motion,
                    orientation,
                    spec,
                    quantity,
                    holds,
                    location,
                } = item;
                corpse == &old.corpse
                    && condition == &old.condition
                    && charges == &old.charges
                    && relocation_elapsed == &old.relocation_elapsed
                    && motion == &old.motion
                    && orientation == &old.orientation
                    && spec == &old.spec
                    && quantity == &old.quantity
                    && holds == &old.holds
                    && location == &old.location
                    && (fuel == &old.fuel
                        || matches!((fuel, old.fuel),
                    (Some(crate::portable_light::FuelState { remaining, lit: true }),
                     Some(crate::portable_light::FuelState { remaining: previous, lit: true }))
                        if *remaining > 0 && *remaining <= previous))
            })
    }

    pub fn map_knowledge_root(&self, actor: ActorId) -> MapKnowledgeRoot {
        MapKnowledgeRoot(self.explored_maps.get(&actor).cloned())
    }
    pub fn map_knowledge_is_current(&self, actor: ActorId, root: &MapKnowledgeRoot) -> bool {
        match (self.explored_maps.get(&actor), &root.0) {
            (Some(a), Some(b)) => a.shares_storage(b),
            (None, None) => true,
            _ => false,
        }
    }

    pub fn record_explored_observation(
        &mut self,
        actor: ActorId,
        cells: &[MapCellInput],
        scene: &[SightCell],
    ) -> Result<(), GameError> {
        let origin = self
            .actors
            .get(&actor)
            .ok_or(GameError::UnknownActor)?
            .location;
        if let Some(previous) = self.explored_maps.get(&actor) {
            if !self.map_validation.loaded_matches(actor, previous)
                && !previous.checkpoint_valid(&self.world)
            {
                return Err(GameError::InvalidLocation);
            }
        }
        let mut appearances = BTreeMap::new();
        for cell in cells {
            count_map_validation(0, 0, 1);
            if !self.world.knows(cell.location)
                || !self
                    .world
                    .known_region(cell.location.region)
                    .is_some_and(|region| region.bounds.contains(cell.location.position))
                || !cell.appearance.valid()
                || !cell
                    .appearance
                    .valid_for_catalog(self.content.presentation_catalog())
            {
                return Err(GameError::InvalidLocation);
            }
            if appearances
                .insert(cell.location, &cell.appearance)
                .is_some_and(|previous| previous != &cell.appearance)
            {
                return Err(GameError::InvalidLocation);
            }
        }
        let physical: Vec<_> = scene.iter().filter(|c| c.physical).collect();
        let mut offsets = BTreeMap::new();
        for (i, cell) in physical.iter().enumerate() {
            if cell.rotation >= 24
                || !appearances.contains_key(&cell.location)
                || offsets.insert(cell.offset, i).is_some()
            {
                return Err(GameError::InvalidLocation);
            }
        }
        let mut parents: Vec<_> = (0..physical.len()).collect();
        let mut neighbors = BTreeMap::new();
        for (i, a) in physical.iter().enumerate() {
            for direction in DIRECTIONS {
                let Some(next) = offset(a.offset, direction_delta(direction)) else {
                    continue;
                };
                let Some(&j) = offsets.get(&next) else {
                    continue;
                };
                let b = physical[j];
                let ar = root(&mut parents, i);
                let br = root(&mut parents, j);
                parents[ar] = br;
                propose(
                    &mut neighbors,
                    (a.location, direction.rotated(a.rotation)),
                    Neighbor::Placed {
                        target: b.location,
                        rotation: tor_world::compose_rotation(
                            tor_world::inverse_rotation(a.rotation),
                            b.rotation,
                        ),
                    },
                    Neighbor::Ambiguous,
                );
            }
        }
        let mut representatives = BTreeMap::new();
        let mut primary = None;
        for (i, cell) in physical.iter().enumerate() {
            let component = root(&mut parents, i);
            let key = (cell.location, cell.rotation, cell.offset);
            representatives
                .entry(component)
                .and_modify(|rep: &mut usize| {
                    let old = physical[*rep];
                    if key < (old.location, old.rotation, old.offset) {
                        *rep = i;
                    }
                })
                .or_insert(i);
            if cell.location == origin && cell.offset == (Position { x: 0, y: 0, z: 0 }) {
                primary = Some(component);
            }
        }
        let primary = primary.or_else(|| {
            representatives
                .iter()
                .min_by_key(|(_, i)| {
                    let c = physical[**i];
                    (c.location, c.rotation, c.offset)
                })
                .map(|(c, _)| *c)
        });
        let mut witnesses = BTreeMap::new();
        if let Some(component) = primary {
            let a = physical[representatives[&component]];
            for (&other, &i) in &representatives {
                if other == component {
                    continue;
                }
                let b = physical[i];
                let delta = checked_rotate(a.rotation, difference(b.offset, a.offset))
                    .ok_or(GameError::InvalidLocation)?;
                propose(
                    &mut witnesses,
                    b.location,
                    Witness::Placed {
                        anchor: a.location,
                        delta,
                        rotation: tor_world::compose_rotation(
                            tor_world::inverse_rotation(a.rotation),
                            b.rotation,
                        ),
                    },
                    Witness::Ambiguous,
                );
            }
        }
        count(|n| {
            n.observed_cells += cells.len();
            n.placement_proposals += neighbors.len() + witnesses.len();
        });
        let previous = self.explored_maps.get(&actor);
        appearances.retain(|at, appearance| {
            previous
                .and_then(|m| m.cells.get(at))
                .is_none_or(|old| &**old != *appearance)
        });
        neighbors.retain(|key, value| previous.and_then(|m| m.neighbors.get(key)) != Some(value));
        witnesses.retain(|key, value| previous.and_then(|m| m.witnesses.get(key)) != Some(value));
        if appearances.is_empty() && neighbors.is_empty() && witnesses.is_empty() {
            if let Some(map) = self.explored_maps.get(&actor) {
                self.map_validation.accept_loaded(actor, map);
            }
            return Ok(());
        }
        self.map_validation.forget_loaded(actor);
        let map = &mut **self.explored_maps.entry(actor).or_default();
        for (at, appearance) in appearances {
            count(|n| {
                n.appearance_changes += 1;
                n.appearance_payload_copies += 1;
                n.bucket_entries_copied += map.cells.copy_on_write_entries(at.region);
            });
            map.cells.insert(at, Shared::new(appearance.clone()));
        }
        for (key, value) in neighbors {
            count(|n| {
                n.neighbor_changes += 1;
                n.bucket_entries_copied += map.neighbors.copy_on_write_entries(key.0.region);
            });
            map.neighbors.insert(key, value);
        }
        for (child, value) in witnesses {
            if let Some(Witness::Placed { anchor, delta, .. }) = map.witnesses.get(&child) {
                let key = (*anchor, *delta, child);
                count(|n| {
                    n.bucket_entries_copied += map.children.copy_on_write_entries(anchor.region)
                });
                map.children.remove(&key);
            }
            if let Witness::Placed {
                anchor,
                delta,
                rotation,
            } = value
            {
                count(|n| {
                    n.bucket_entries_copied += map.children.copy_on_write_entries(anchor.region)
                });
                map.children.insert((anchor, delta, child), rotation);
            }
            count(|n| {
                n.witness_changes += 1;
                n.bucket_entries_copied += map.witnesses.copy_on_write_entries(child.region);
            });
            map.witnesses.insert(child, value);
        }
        self.map_validation
            .accept_loaded(actor, &self.explored_maps[&actor]);
        Ok(())
    }
}

impl MapWindow {
    fn contains(self, p: Position) -> bool {
        p.x >= self.min.x
            && p.x <= self.max.x
            && p.y >= self.min.y
            && p.y <= self.max.y
            && p.z >= self.min.z
            && p.z <= self.max.z
    }
    fn halo(self) -> Option<Self> {
        for (a, b) in vector(self.min).into_iter().zip(vector(self.max)) {
            if b < a || b - a >= 129 {
                return None;
            }
        }
        Some(Self {
            min: offset(self.min, [-1; 3])?,
            max: offset(self.max, [1; 3])?,
        })
    }
    fn projection_halo(self, domain: Self) -> Option<Self> {
        self.halo()?;
        if !domain.contains(self.min) || !domain.contains(self.max) {
            return None;
        }
        domain.halo()
    }
    fn local_bounds(self, p: Position, rotation: u8) -> Option<([i64; 3], [i64; 3])> {
        let a = checked_rotate(rotation, difference(self.min, p))?;
        let b = checked_rotate(rotation, difference(self.max, p))?;
        Some((
            std::array::from_fn(|i| a[i].min(b[i])),
            std::array::from_fn(|i| a[i].max(b[i])),
        ))
    }
}

impl MapAppearance {
    /// Conservative JSON/wire bound; inspect lengths without serializing or
    /// cloning payloads. Valid appearance strings exclude controls, so two
    /// bytes per UTF-8 byte covers the remaining quote/backslash JSON escapes.
    fn encoded_size_bound(&self) -> usize {
        let text = |s: &str| s.len().saturating_mul(2).saturating_add(2);
        let asset = |s: &Option<String>| {
            s.as_deref().map_or(0, |s| {
                if s.len() > 120 * 1024 {
                    return usize::MAX;
                }
                s.bytes().fold(2usize, |n, b| {
                    n.saturating_add(match b {
                        b'"' | b'\\' => 2,
                        0..=31 => 6,
                        _ => 1,
                    })
                })
            })
        };
        let mut bytes = 400usize
            .saturating_add(text(&self.material))
            .saturating_add(asset(&self.asset));
        if let Some(p) = &self.object_presentation {
            bytes = bytes.saturating_add(p.encoded_size_bound());
        }
        if let Some(d) = &self.door {
            bytes = bytes
                .saturating_add(256)
                .saturating_add(text(&d.name))
                .saturating_add(text(&d.description))
                .saturating_add(asset(&d.asset));
        }
        if self.trap.is_some() {
            bytes = bytes.saturating_add(64);
        }
        for i in &self.ground_items {
            if let Some(p) = &i.presentation {
                bytes = bytes.saturating_add(p.encoded_size_bound());
            }
            bytes = bytes
                .saturating_add(256)
                .saturating_add(text(&i.name))
                .saturating_add(text(&i.description))
                .saturating_add(text(&i.appearance))
                .saturating_add(asset(&i.asset));
        }
        bytes
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Placement {
    at: Location,
    offset: Position,
    frame: u8,
}
type Priority = (u64, Position, Location, u8, usize);
fn priority(p: Placement, id: usize) -> Priority {
    let v = vector(p.offset);
    (
        v.into_iter().map(i64::unsigned_abs).sum(),
        p.offset,
        p.at,
        p.frame,
        id,
    )
}
struct Projection<'a> {
    map: &'a ExploredMap,
    window: MapWindow,
    halo: MapWindow,
    current: BTreeMap<Position, (Location, u8)>,
    states: Vec<Placement>,
    ids: BTreeMap<Placement, usize>,
    occupied: BTreeMap<Position, usize>,
    blocked: Vec<bool>,
    seeds: Vec<usize>,
    queue: BTreeSet<Priority>,
    edges: Vec<(usize, usize)>,
    work: MapWorkCounts,
    ambiguous: bool,
    limited: bool,
    appearance_limited: bool,
}
impl Projection<'_> {
    fn relation(&mut self) -> bool {
        if self.work.candidate_relations >= MAX_RELATIONS {
            self.limited = true;
            return false;
        }
        self.work.candidate_relations += 1;
        true
    }
    fn inverse(&mut self) -> bool {
        if self.work.inverse_checks >= MAX_RELATIONS {
            self.limited = true;
            return false;
        }
        self.work.inverse_checks += 1;
        true
    }
    fn seek(&mut self) -> bool {
        if self.work.range_seeks >= MAX_SEEKS {
            self.limited = true;
            return false;
        }
        self.work.range_seeks += 1;
        true
    }
    fn add(&mut self, p: Placement, seed: bool) -> Option<usize> {
        if !self.halo.contains(p.offset) || !self.map.cells.contains_key(&p.at) {
            return None;
        }
        if let Some(&id) = self.ids.get(&p) {
            return Some(id);
        }
        if self.states.len() >= MAX_STATES {
            self.limited = true;
            return None;
        }
        let id = self.states.len();
        let mut blocked = false;
        if let Some(&(at, frame)) = self.current.get(&p.offset) {
            if (p.at, p.frame) != (at, frame) {
                blocked = true;
                self.ambiguous = true;
            }
        } else if let Some(&other) = self.occupied.get(&p.offset) {
            if self.states[other] != p {
                blocked = true;
                self.blocked[other] = true;
                self.ambiguous = true;
            }
        }
        self.occupied.entry(p.offset).or_insert(id);
        self.ids.insert(p, id);
        self.states.push(p);
        self.blocked.push(blocked);
        self.work.projection_states += 1;
        if seed {
            self.seeds.push(id);
        }
        self.queue.insert(priority(p, id));
        Some(id)
    }
    fn edge(&mut self, source: usize, target: Location, delta: [i64; 3], rotation: u8) {
        let p = self.states[source];
        let Some(delta) = checked_rotate(tor_world::inverse_rotation(p.frame), delta) else {
            self.ambiguous = true;
            return;
        };
        let Some(position) = offset(p.offset, delta) else {
            self.ambiguous = true;
            return;
        };
        if rotation >= 24 {
            self.ambiguous = true;
            return;
        }
        let target = Placement {
            at: target,
            offset: position,
            frame: tor_world::compose_rotation(p.frame, rotation),
        };
        if let Some(id) = self.add(target, false) {
            if self.edges.len() >= MAX_RELATIONS {
                self.limited = true;
                return;
            }
            self.edges.push((source, id));
        }
    }
    fn expand(&mut self, id: usize) {
        let p = self.states[id];
        for direction in DIRECTIONS {
            if !self.relation() {
                return;
            }
            match self.map.neighbors.get(&(p.at, direction)).cloned() {
                Some(Neighbor::Placed { target, rotation }) if rotation < 24 => {
                    if !self.inverse() {
                        return;
                    }
                    let reverse = opposite(direction).rotated(rotation);
                    if self.map.neighbors.get(&(target, reverse))
                        == Some(&Neighbor::Placed {
                            target: p.at,
                            rotation: tor_world::inverse_rotation(rotation),
                        })
                    {
                        self.edge(id, target, direction_delta(direction), rotation);
                    } else {
                        self.ambiguous = true;
                    }
                }
                Some(_) => self.ambiguous = true,
                None => {}
            }
            if self.limited {
                return;
            }
        }
        if !self.relation() {
            return;
        }
        match self.map.witnesses.get(&p.at).cloned() {
            Some(Witness::Placed {
                anchor,
                delta,
                rotation,
            }) if rotation < 24 => {
                if !self.inverse() {
                    return;
                }
                if self.map.children.get(&(anchor, delta, p.at)) == Some(&rotation) {
                    let neg = delta[0]
                        .checked_neg()
                        .zip(delta[1].checked_neg())
                        .zip(delta[2].checked_neg())
                        .and_then(|((x, y), z)| checked_rotate(rotation, [x, y, z]));
                    if let Some(neg) = neg {
                        self.edge(id, anchor, neg, tor_world::inverse_rotation(rotation));
                    } else {
                        self.ambiguous = true;
                    }
                } else {
                    self.ambiguous = true;
                }
            }
            Some(_) => self.ambiguous = true,
            None => {}
        }
        if self.limited {
            return;
        }
        let Some((min, max)) = self.halo.local_bounds(p.offset, p.frame) else {
            self.ambiguous = true;
            return;
        };
        let lowest = Location {
            region: RegionId(0),
            position: Position {
                x: i32::MIN,
                y: i32::MIN,
                z: i32::MIN,
            },
        };
        use std::ops::Bound::{Excluded, Included, Unbounded};
        let mut lower = Included((p.at, min, lowest));
        loop {
            if !self.seek() {
                return;
            }
            // Charge before touching even the first row; an empty seek may
            // conservatively consume a candidate slot as well.
            if !self.relation() {
                return;
            }
            let entry = self
                .map
                .children
                .region(p.at.region)
                .and_then(|bucket| bucket.range((lower, Unbounded)).next())
                .map(|(k, r)| (*k, *r));
            let Some(((anchor, delta, child), rotation)) = entry else {
                break;
            };
            if anchor != p.at || delta[0] > max[0] {
                break;
            }
            // Seek over a whole excluded run. Historical distant children do
            // not consume one visit each or starve a nearby in-window child.
            let jump = if delta[1] < min[1] {
                Some([delta[0], min[1], min[2]])
            } else if delta[1] > max[1] {
                delta[0].checked_add(1).map(|x| [x, min[1], min[2]])
            } else if delta[2] < min[2] {
                Some([delta[0], delta[1], min[2]])
            } else if delta[2] > max[2] {
                delta[1].checked_add(1).map(|y| [delta[0], y, min[2]])
            } else {
                None
            };
            if let Some(jump) = jump {
                lower = Included((p.at, jump, lowest));
                continue;
            }
            lower = Excluded((anchor, delta, child));
            if !self.inverse() {
                return;
            }
            if self.map.witnesses.get(&child)
                == Some(&Witness::Placed {
                    anchor,
                    delta,
                    rotation,
                })
            {
                self.edge(id, child, delta, rotation);
            } else {
                self.ambiguous = true;
            }
            if self.limited {
                return;
            }
        }
    }
    fn finish(mut self) -> MapProjection {
        while !self.limited {
            let Some((_, _, _, _, id)) = self.queue.pop_first() else {
                break;
            };
            // Discovery is independent of queue order and absolute offsets.
            // Even a blocked parent can reveal a competing placement which
            // must be considered before the second reachability pass.
            self.expand(id);
        }
        let mut cells = Vec::new();
        if !self.limited {
            self.edges.sort_unstable();
            self.edges.dedup();
            let mut reachable = vec![false; self.states.len()];
            let mut queue = VecDeque::new();
            for id in self.seeds {
                if !self.blocked[id] && !reachable[id] {
                    reachable[id] = true;
                    queue.push_back(id);
                }
            }
            while let Some(id) = queue.pop_front() {
                let first = self.edges.partition_point(|&(from, _)| from < id);
                for &(_, to) in self.edges[first..]
                    .iter()
                    .take_while(|&&(from, _)| from == id)
                {
                    self.work.reachability_edges += 1;
                    if !self.blocked[to] && !reachable[to] {
                        reachable[to] = true;
                        queue.push_back(to);
                    }
                }
            }
            let mut bytes = 0usize;
            for (id, p) in self.states.iter().enumerate() {
                if !reachable[id] || !self.window.contains(p.offset) {
                    continue;
                }
                let appearance = self.map.cells.get(&p.at).expect("admitted known cell");
                let size = appearance.encoded_size_bound();
                bytes = bytes.saturating_add(size);
                if cells.len() >= MAX_OUTPUT || size > 120 * 1024 || bytes > 7 * 1024 * 1024 {
                    self.appearance_limited = size > 120 * 1024 || bytes > 7 * 1024 * 1024;
                    self.limited = true;
                    cells.clear();
                    break;
                }
                cells.push(ProjectedMapCell {
                    location: p.at,
                    position: p.offset,
                    appearance: appearance.clone(),
                });
            }
            cells.sort_by_key(|c| (c.position, c.location));
        }
        MapProjection {
            cells,
            complete: !self.limited,
            ambiguous: self.ambiguous,
            limited: self.limited,
            appearance_limited: self.appearance_limited,
            work: self.work,
        }
    }
}

impl ExploredMap {
    fn project(&self, scene: &[SightCell], window: MapWindow) -> Result<MapProjection, GameError> {
        self.project_region(scene, window, window)
    }

    fn project_region(
        &self,
        scene: &[SightCell],
        window: MapWindow,
        domain: MapWindow,
    ) -> Result<MapProjection, GameError> {
        let halo = window
            .projection_halo(domain)
            .ok_or(GameError::InvalidLocation)?;
        let mut current = BTreeMap::new();
        let mut seeds = BTreeSet::new();
        let mut omitted = false;
        for cell in scene
            .iter()
            .filter(|c| c.physical && halo.contains(c.offset))
        {
            if cell.rotation >= 24 {
                return Err(GameError::InvalidLocation);
            }
            if current.contains_key(&cell.offset) {
                return Err(GameError::InvalidLocation);
            }
            if current.len() < MAX_STATES {
                current.insert(cell.offset, (cell.location, cell.rotation));
            } else {
                omitted = true;
            }
            let p = Placement {
                at: cell.location,
                offset: cell.offset,
                frame: cell.rotation,
            };
            seeds.insert(priority(p, 0));
            if seeds.len() > MAX_STATES {
                seeds.pop_last();
                omitted = true;
            }
        }
        let mut projection = Projection {
            map: self,
            window,
            halo,
            current,
            states: Vec::new(),
            ids: BTreeMap::new(),
            occupied: BTreeMap::new(),
            blocked: Vec::new(),
            seeds: Vec::new(),
            queue: BTreeSet::new(),
            edges: Vec::new(),
            work: MapWorkCounts::default(),
            ambiguous: false,
            limited: omitted,
            appearance_limited: false,
        };
        for (_, offset, at, frame, _) in seeds {
            projection.add(Placement { at, offset, frame }, true);
        }
        Ok(projection.finish())
    }
}
impl Game {
    pub fn project_explored_map(
        &self,
        actor: ActorId,
        scene: &[SightCell],
        window: MapWindow,
    ) -> Result<MapProjection, GameError> {
        if !self.actors.contains_key(&actor) {
            return Err(GameError::UnknownActor);
        }
        match self.explored_maps.get(&actor) {
            Some(map) => map.project(scene, window),
            None => ExploredMap::default().project(scene, window),
        }
    }

    /// Project a subwindow using the containing domain's current scene anchors
    /// and one-cell traversal halo. Output clipping never narrows the evidence
    /// used to resolve placements, aliases or ambiguity. Both windows retain
    /// the ordinary bounds and all projection work limits remain in force.
    pub fn project_explored_map_region(
        &self,
        actor: ActorId,
        scene: &[SightCell],
        window: MapWindow,
        domain: MapWindow,
    ) -> Result<MapProjection, GameError> {
        // Reject malformed caller metadata before consulting actor-owned state.
        window
            .projection_halo(domain)
            .ok_or(GameError::InvalidLocation)?;
        if !self.actors.contains_key(&actor) {
            return Err(GameError::UnknownActor);
        }
        match self.explored_maps.get(&actor) {
            Some(map) => map.project_region(scene, window, domain),
            None => ExploredMap::default().project_region(scene, window, domain),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    #[test]
    fn trap_memory_is_strict_nullable_appearance_without_authority_or_private_operands() {
        for family in [
            None,
            Some(tor_world::TrapFamily::Rust),
            Some(tor_world::TrapFamily::SleepingGas),
        ] {
            let value = MapTrapAppearance { family };
            let encoded = serde_json::to_value(&value).unwrap();
            assert_eq!(encoded.as_object().unwrap().len(), 1);
            assert_eq!(
                serde_json::from_value::<MapTrapAppearance>(encoded).unwrap(),
                value
            );
        }
        for invalid in [
            serde_json::json!({}),
            serde_json::json!([null]),
            serde_json::json!({"family":null,"id":1}),
            serde_json::json!({"family":null,"dc":12}),
            serde_json::json!({"family":{"rust":null}}),
            serde_json::json!({"family":{"sleeping_gas":null}}),
        ] {
            assert!(serde_json::from_value::<MapTrapAppearance>(invalid).is_err());
        }
    }
    fn pos(x: i32, y: i32, z: i32) -> Position {
        Position { x, y, z }
    }
    fn at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: pos(x, 1, 0),
        }
    }
    fn appearance(label: &str) -> MapAppearance {
        MapAppearance {
            object_presentation: None,
            wall: true,
            material: label.into(),
            asset: Some("old-stone".into()),
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            trap: None,
            door: None,
            ground_items: Vec::new(),
        }
    }
    fn seen(location: Location, offset: Position, rotation: u8) -> SightCell {
        SightCell {
            location,
            offset,
            rotation,
            wall: true,
            physical: true,
        }
    }
    fn window() -> MapWindow {
        MapWindow {
            min: pos(-31, -16, -2),
            max: pos(31, 16, 3),
        }
    }
    fn game() -> (Game, ActorId) {
        let mut world = tor_world::World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "memory".into(),
                bounds: tor_world::Extent::new(6000, 4, 2).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        (game, actor)
    }
    fn record(game: &mut Game, actor: ActorId, scene: &[SightCell]) {
        let inputs: Vec<_> = scene
            .iter()
            .map(|c| MapCellInput {
                location: c.location,
                appearance: appearance("old"),
            })
            .collect();
        game.record_explored_observation(actor, &inputs, scene)
            .unwrap();
    }
    fn assert_clipped_projection(
        map: &ExploredMap,
        scene: &[SightCell],
        output: MapWindow,
        domain: MapWindow,
        full: &MapProjection,
    ) {
        let mut expected = full.clone();
        expected.cells.retain(|cell| output.contains(cell.position));
        assert_eq!(map.project_region(scene, output, domain).unwrap(), expected);
    }

    #[test]
    fn projection_regions_require_valid_containing_bounded_domains() {
        let (game, actor) = game();
        let point = MapWindow {
            min: pos(0, 0, 0),
            max: pos(0, 0, 0),
        };
        let reversed = MapWindow {
            min: pos(1, 0, 0),
            ..point
        };
        let outside = MapWindow {
            min: pos(32, 0, 0),
            max: pos(32, 0, 0),
        };
        let too_wide = MapWindow {
            min: pos(-64, 0, 0),
            max: pos(65, 0, 0),
        };
        let lowest = MapWindow {
            min: pos(i32::MIN, 0, 0),
            max: pos(i32::MIN, 0, 0),
        };
        let highest = MapWindow {
            min: pos(i32::MAX, 0, 0),
            max: pos(i32::MAX, 0, 0),
        };
        for (output, domain) in [
            (outside, window()),
            (reversed, window()),
            (point, reversed),
            (point, too_wide),
            (lowest, lowest),
            (highest, highest),
            (
                MapWindow {
                    min: pos(i32::MIN + 1, 0, 0),
                    max: pos(i32::MIN + 1, 0, 0),
                },
                MapWindow {
                    max: pos(i32::MIN + 1, 0, 0),
                    ..lowest
                },
            ),
            (
                MapWindow {
                    min: pos(i32::MAX - 1, 0, 0),
                    max: pos(i32::MAX - 1, 0, 0),
                },
                MapWindow {
                    min: pos(i32::MAX - 1, 0, 0),
                    ..highest
                },
            ),
            (window(), point),
        ] {
            assert_eq!(
                game.project_explored_map_region(actor, &[], output, domain),
                Err(GameError::InvalidLocation),
                "output={output:?}, domain={domain:?}"
            );
        }
        for output in [point, window()] {
            let projection = game
                .project_explored_map_region(actor, &[], output, window())
                .unwrap();
            assert!(projection.complete && projection.cells.is_empty());
        }
        assert_eq!(
            game.project_explored_map_region(ActorId(actor.0 + 1), &[], point, window()),
            Err(GameError::UnknownActor)
        );
        assert_eq!(
            game.project_explored_map_region(ActorId(actor.0 + 1), &[], point, too_wide),
            Err(GameError::InvalidLocation)
        );
    }

    #[test]
    fn off_sight_region_uses_only_its_actors_saved_appearances_and_bounded_anchors() {
        let (mut game, actor) = game();
        let other = game
            .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let anchor = seen(at(1, 1), pos(0, 0, 0), 0);
        let edge = seen(at(1, 32), pos(31, 0, 0), 0);
        record(&mut game, actor, &[anchor, edge]);
        record(&mut game, other, &[anchor]);
        let slab = MapWindow {
            min: pos(31, -16, -2),
            max: window().max,
        };
        // The saved wall deliberately differs from the live floor. Projection
        // must not refresh it or borrow another actor's knowledge.
        assert!(!game.world.is_wall(edge.location));
        let before = game.clone();
        reset_map_work_counts();
        let full = game
            .project_explored_map(actor, &[anchor], window())
            .unwrap();
        let partial = game
            .project_explored_map_region(actor, &[anchor], slab, window())
            .unwrap();
        assert_eq!(partial.cells.len(), 1);
        assert_eq!(partial.cells[0].location, edge.location);
        assert_eq!(&*partial.cells[0].appearance, &appearance("old"));
        assert_clipped_projection(
            &game.explored_maps[&actor],
            &[anchor],
            slab,
            window(),
            &full,
        );
        let unseen = game
            .project_explored_map_region(other, &[anchor], slab, window())
            .unwrap();
        assert!(unseen.complete && unseen.cells.is_empty());
        // A domain without a current anchor stays empty; no historical scan or
        // world fallback expands it. The legacy API keeps its local semantics.
        let unanchored = game
            .project_explored_map_region(actor, &[anchor], slab, slab)
            .unwrap();
        assert!(unanchored.complete && unanchored.cells.is_empty());
        assert_eq!(
            game.project_explored_map(actor, &[anchor], slab).unwrap(),
            unanchored
        );
        assert_eq!(game, before);
        assert_eq!(map_work_counts(), MapWorkCounts::default());
    }

    #[test]
    fn small_output_does_not_bypass_projection_domain_work_caps() {
        let mut map = ExploredMap::default();
        let cell = at(1, 1);
        map.cells.insert(cell, Shared::new(appearance("wall")));
        for direction in DIRECTIONS {
            map.neighbors.insert(
                (cell, direction),
                Neighbor::Placed {
                    target: cell,
                    rotation: 0,
                },
            );
        }
        let domain = MapWindow {
            min: pos(-64, -64, -64),
            max: pos(64, 64, 64),
        };
        let output = MapWindow {
            min: pos(64, 64, 64),
            max: pos(64, 64, 64),
        };
        let scene = [seen(cell, pos(0, 0, 0), 0)];
        let full = map.project(&scene, domain).unwrap();
        assert!(full.limited && !full.complete && full.cells.is_empty());
        assert!(!full.appearance_limited);
        assert!(full.work.projection_states <= MAX_STATES);
        assert!(full.work.candidate_relations <= MAX_RELATIONS);
        assert!(full.work.inverse_checks <= MAX_RELATIONS);
        assert!(full.work.range_seeks <= MAX_SEEKS);
        assert_clipped_projection(&map, &scene, output, domain, &full);
    }

    #[test]
    fn acquisition_checks_new_world_references_and_does_not_trust_forged_roots() {
        let (mut game, actor) = game();
        record(&mut game, actor, &[seen(at(1, 1), pos(0, 0, 0), 0)]);
        let before = game.clone();
        assert_eq!(
            game.record_explored_observation(
                actor,
                &[MapCellInput {
                    location: at(99, 1),
                    appearance: appearance("forged"),
                }],
                &[]
            ),
            Err(GameError::InvalidLocation)
        );
        assert_eq!(game, before);
        game.explored_maps
            .get_mut(&actor)
            .unwrap()
            .cells
            .insert(at(99, 1), Shared::new(appearance("forged")));
        assert!(!game
            .map_validation
            .loaded_matches(actor, &game.explored_maps[&actor]));
        assert_eq!(
            game.record_explored_observation(actor, &[], &[]),
            Err(GameError::InvalidLocation)
        );
    }

    #[test]
    fn validation_cache_is_derived_and_game_clones_do_not_copy_its_directories() {
        let (mut game, actor) = game();
        record(&mut game, actor, &[seen(at(1, 1), pos(0, 0, 0), 0)]);
        reset_map_work_counts();
        let mut clone = game.clone();
        assert_eq!(map_work_counts(), MapWorkCounts::default());
        clone.map_validation = MapValidationCache::default();
        assert_eq!(clone, game);
        let checkpoint = |game: &Game| {
            let mut shared = crate::checkpoint::SharedState::default();
            let snapshot = game.checkpoint(&mut shared);
            serde_json::to_vec(&(snapshot, shared)).unwrap()
        };
        assert_eq!(checkpoint(&clone), checkpoint(&game));
    }

    #[test]
    fn unchanged_observations_share_owners_and_translation_does_not_grow_evidence() {
        let (mut game, actor) = game();
        let a = seen(at(1, 1), pos(0, 0, 0), 0);
        let b = seen(at(1, 2), pos(1, 0, 0), 0);
        let c = seen(at(1, 9), pos(9, 0, 0), 0);
        record(&mut game, actor, &[a, b, c]);
        let before = game.clone();
        reset_map_work_counts();
        let translated = [a, b, c].map(|mut c| {
            c.offset.x += 1;
            c
        });
        record(&mut game, actor, &translated);
        assert!(game.explored_maps[&actor].shares_storage(&before.explored_maps[&actor]));
        let work = map_work_counts();
        assert_eq!(
            (
                work.owner_copies,
                work.appearance_changes,
                work.neighbor_changes,
                work.witness_changes
            ),
            (0, 0, 0, 0)
        );
        assert_eq!(game.explored_maps[&actor].children.iter().count(), 1);
    }
    #[test]
    fn all_cube_frames_reproject_nonwalkable_surfaces_and_disconnected_islands() {
        for frame in 0..24 {
            let (mut game, actor) = game();
            let scene = [
                seen(at(1, 1), pos(0, 0, 0), frame),
                seen(at(1, 2), pos(1, 0, 0), frame),
                seen(at(1, 9), pos(7, 3, 0), frame),
            ];
            record(&mut game, actor, &scene);
            for new_frame in 0..24 {
                let seed = seen(at(1, 1), pos(0, 0, 0), new_frame);
                let result = game
                    .project_explored_map(
                        actor,
                        &[seed],
                        MapWindow {
                            min: pos(-10, -10, -10),
                            max: pos(10, 10, 10),
                        },
                    )
                    .unwrap();
                assert!(
                    result.complete && !result.ambiguous,
                    "{frame}/{new_frame}: {result:?}"
                );
                for (cell, original) in result
                    .cells
                    .iter()
                    .map(|c| (c, scene.iter().find(|s| s.location == c.location).unwrap()))
                {
                    let expected = checked_rotate(
                        tor_world::inverse_rotation(new_frame),
                        checked_rotate(frame, vector(original.offset)).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(vector(cell.position), expected);
                    assert!(cell.appearance.wall);
                }
                assert_eq!(result.cells.len(), 3);
                for cell in &result.cells {
                    assert_clipped_projection(
                        &game.explored_maps[&actor],
                        &[seed],
                        MapWindow {
                            min: cell.position,
                            max: cell.position,
                        },
                        MapWindow {
                            min: pos(-10, -10, -10),
                            max: pos(10, 10, 10),
                        },
                        &result,
                    );
                }
            }
        }
    }
    #[test]
    fn synthetic_stair_landing_is_not_geometry_and_current_seed_beats_stale_rival() {
        let (mut game, actor) = game();
        let a = seen(at(1, 1), pos(0, 0, 0), 0);
        let b = seen(at(1, 2), pos(1, 0, 0), 0);
        let mut landing = seen(at(1, 30), pos(0, 0, 1), 0);
        landing.physical = false;
        record(&mut game, actor, &[a, b, landing]);
        let result = game.project_explored_map(actor, &[a], window()).unwrap();
        assert_eq!(result.cells.len(), 2);
        let rival = seen(at(1, 3), pos(1, 0, 0), 0);
        record(&mut game, actor, &[rival]);
        let result = game
            .project_explored_map(actor, &[a, rival], window())
            .unwrap();
        assert!(result.complete && result.ambiguous);
        assert!(result.cells.iter().any(|c| c.location == rival.location));
        assert!(!result.cells.iter().any(|c| c.location == b.location));
        assert_clipped_projection(
            &game.explored_maps[&actor],
            &[a, rival],
            MapWindow {
                min: rival.offset,
                max: rival.offset,
            },
            window(),
            &result,
        );
    }
    #[test]
    fn inverse_witness_index_seeks_past_distant_history_without_candidate_scan() {
        let anchor = at(1, 1);
        let near = at(2, 1);
        let mut map = ExploredMap::default();
        map.cells.insert(anchor, Shared::new(appearance("anchor")));
        map.cells.insert(near, Shared::new(appearance("near")));
        map.witnesses.insert(
            near,
            Witness::Placed {
                anchor,
                delta: [0, 2, 0],
                rotation: 0,
            },
        );
        map.children.insert((anchor, [0, 2, 0], near), 0);
        let seed = seen(anchor, pos(0, 0, 0), 0);
        let before = map.project(&[seed], window()).unwrap();
        assert_eq!(
            before
                .cells
                .iter()
                .map(|cell| cell.location)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([anchor, near])
        );
        let mut one_distant = None;
        for i in 0..40_000 {
            let child = at(3, i);
            map.cells.insert(child, Shared::new(appearance("far")));
            map.witnesses.insert(
                child,
                Witness::Placed {
                    anchor,
                    delta: [0, -100_000 + i64::from(i), 0],
                    rotation: 0,
                },
            );
            map.children
                .insert((anchor, [0, -100_000 + i64::from(i), 0], child), 0);
            if i == 0 {
                one_distant = Some(map.project(&[seed], window()).unwrap());
            }
        }
        let after = map.project(&[seed], window()).unwrap();
        let one_distant = one_distant.unwrap();
        assert_eq!(before.cells, after.cells);
        assert_eq!(one_distant.cells, after.cells);
        // Encountering an excluded run costs one seek to jump past it. Adding
        // the other 39,999 rows must not add per-row visits or hide nearby cells.
        assert!(one_distant.work.candidate_relations <= before.work.candidate_relations + 1);
        assert!(one_distant.work.range_seeks <= before.work.range_seeks + 1);
        assert_eq!(
            one_distant.work.candidate_relations,
            after.work.candidate_relations
        );
        assert_eq!(one_distant.work.range_seeks, after.work.range_seeks);
        assert!(after.complete && !after.ambiguous);
    }
    #[test]
    fn more_than_4096_sequential_sightings_survive_return_without_history_projection() {
        let (mut game, actor) = game();
        for x in 1..5000 {
            record(
                &mut game,
                actor,
                &[
                    seen(at(1, x), pos(0, 0, 0), 0),
                    seen(at(1, x + 1), pos(1, 0, 0), 0),
                ],
            );
        }
        assert_eq!(game.explored_maps[&actor].cells.iter().count(), 5000);
        let result = game
            .project_explored_map(actor, &[seen(at(1, 1), pos(0, 0, 0), 0)], window())
            .unwrap();
        assert!(result.complete);
        assert_eq!(result.cells.len(), 32);
        assert!(result.work.projection_states <= 33);
        assert!(result.work.candidate_relations < 400);
    }
    #[test]
    fn source_bucket_copy_growth_is_distinct_from_region_directory_growth() {
        let (mut game, actor) = game();
        for region in 2..=512 {
            game.add_region(tor_world::Region {
                id: RegionId(region),
                name: "memory".into(),
                bounds: tor_world::Extent::new(4, 3, 1).unwrap(),
            })
            .unwrap();
        }
        for (size, regions) in [(16, 1), (512, 1), (5000, 1), (3, 1), (3, 32), (3, 512)] {
            let mut map = ExploredMap::default();
            for x in 1..=size {
                map.cells.insert(at(1, x), Shared::new(appearance("old")));
                if x > 1 {
                    map.neighbors.insert(
                        (at(1, x - 1), Direction::East),
                        Neighbor::Placed {
                            target: at(1, x),
                            rotation: 0,
                        },
                    );
                    map.neighbors.insert(
                        (at(1, x), Direction::West),
                        Neighbor::Placed {
                            target: at(1, x - 1),
                            rotation: 0,
                        },
                    );
                }
            }
            for region in 2..=regions {
                for x in 1..=3 {
                    map.cells
                        .insert(at(region, x), Shared::new(appearance("old")));
                }
            }
            game.explored_maps.insert(actor, Shared::new(map));
            let old = [MapCellInput {
                location: at(1, 1),
                appearance: appearance("old"),
            }];
            let new = [MapCellInput {
                location: at(1, 1),
                appearance: appearance("new"),
            }];
            // Fixture validation, map population and snapshot clones are outside
            // the bounded timing witness. Timings are diagnostic, never gates.
            game.record_explored_observation(actor, &old, &[]).unwrap();
            let mut samples = Vec::new();
            let mut receipt = None;
            for _ in 0..3 {
                let mut changed = game.clone();
                reset_map_work_counts();
                let started = std::time::Instant::now();
                game.record_explored_observation(actor, &old, &[]).unwrap();
                let unchanged_ns = started.elapsed().as_nanos();
                let unchanged = map_work_counts();
                assert_eq!(
                    (
                        unchanged.owner_copies,
                        unchanged.validation_cells,
                        unchanged.appearance_changes
                    ),
                    (0, 0, 0)
                );
                assert_eq!(unchanged.world_location_checks, 1);
                reset_map_work_counts();
                let started = std::time::Instant::now();
                changed
                    .record_explored_observation(actor, &new, &[])
                    .unwrap();
                let changed_ns = started.elapsed().as_nanos();
                let work = map_work_counts();
                assert_eq!(work.owner_copies, 1);
                assert_eq!(work.directory_entries_copied, regions as usize + 1);
                assert_eq!(work.bucket_entries_copied, size as usize);
                assert_eq!(work.appearance_payload_copies, 1);
                assert_eq!(work.validation_cells, 0);
                assert_eq!(work.world_location_checks, 1);
                assert_eq!(
                    game.explored_maps[&actor]
                        .cells
                        .get(&at(1, 1))
                        .unwrap()
                        .material,
                    "old"
                );
                reset_map_work_counts();
                let started = std::time::Instant::now();
                let projection = game
                    .project_explored_map(actor, &[seen(at(1, 1), pos(0, 0, 0), 0)], window())
                    .unwrap();
                let reconnect_ns = started.elapsed().as_nanos();
                assert!(projection.complete);
                assert_eq!(projection.cells.len(), (size as usize).min(32));
                assert!(projection.work.projection_states <= 33);
                assert!(projection.work.candidate_relations < 400);
                assert_eq!(map_work_counts().validation_cells, 0);
                samples.push((unchanged_ns, changed_ns, reconnect_ns));
                receipt = Some((
                    work.directory_entries_copied,
                    work.bucket_entries_copied,
                    work.validation_directory_entries_copied,
                    projection.work.projection_states,
                    projection.work.candidate_relations,
                    projection.work.range_seeks,
                ));
            }
            println!("map_history_witness cells_in_source={size} regions={regions} samples_ns_unchanged_changed_reconnect={samples:?} counts_directory_bucket_validation_directory_states_candidates_seeks={:?}", receipt.unwrap());
        }
    }
    #[test]
    fn ordinary_full_six_plane_viewport_fits_and_oversize_appearance_is_limited_before_clone() {
        let mut map = ExploredMap::default();
        let mut scene = Vec::new();
        let mut n = 0;
        for z in -2..=3 {
            for y in -16..=16 {
                for x in -31..=31 {
                    let at = at(1, n);
                    n += 1;
                    map.cells.insert(at, Shared::new(appearance("stone")));
                    scene.push(seen(at, pos(x, y, z), 0));
                }
            }
        }
        let result = map.project(&scene, window()).unwrap();
        assert!(result.complete && !result.limited);
        assert_eq!(result.cells.len(), 12_474);
        assert_eq!(result.work.projection_states, 12_474);
        let mut huge = appearance("stone");
        huge.ground_items = (0..4096)
            .map(|_| MapItemAppearance {
                presentation: None,
                class: ItemClass::Potion,
                description: "x".repeat(80),
                name: "bottle".into(),
                appearance: "bottle".into(),
                identified: false,
                quantity: 1,
                asset: None,
            })
            .collect();
        map.cells.insert(scene[0].location, Shared::new(huge));
        let limited = map.project(&scene, window()).unwrap();
        assert!(limited.limited && !limited.complete && limited.cells.is_empty());
        let output = MapWindow {
            min: pos(31, 16, 3),
            max: pos(31, 16, 3),
        };
        let small = map.project_region(&scene, output, window()).unwrap();
        assert!(small.complete && !small.appearance_limited);
        assert_eq!(small.cells.len(), 1);
        assert_eq!(small.cells[0].position, output.min);
        assert_eq!(small.cells[0].appearance.material, "stone");
        let oversized = MapWindow {
            min: scene[0].offset,
            max: scene[0].offset,
        };
        let limited = map.project_region(&scene, oversized, window()).unwrap();
        assert!(limited.appearance_limited && !limited.complete && limited.cells.is_empty());
    }
    #[test]
    fn checked_rotation_rejects_sign_overflow_and_accepts_every_finite_cube_frame() {
        for rotation in 0..24 {
            assert_eq!(
                checked_rotate(rotation, [19, -7, 3]),
                Some(tor_world::rotate_vector(rotation, [19, -7, 3]))
            );
            for axis in 0..3 {
                let mut v = [0; 3];
                v[axis] = i64::MIN;
                let mut basis = [0; 3];
                basis[axis] = 1;
                let positive = tor_world::rotate_vector(rotation, basis)
                    .iter()
                    .all(|v| *v >= 0);
                assert_eq!(checked_rotate(rotation, v).is_some(), positive);
            }
        }
        assert!(checked_rotate(24, [0; 3]).is_none());
    }
    #[test]
    fn repeated_occurrences_of_a_cyclic_cell_remain_distinct_and_never_add_navigation() {
        let (mut game, actor) = game();
        let before = game.navigation.clone();
        let a = seen(at(1, 1), pos(0, 0, 0), 0);
        let repeated = seen(a.location, pos(1, 0, 0), 0);
        record(&mut game, actor, &[a, repeated]);
        assert_eq!(game.navigation, before);
        let result = game.project_explored_map(actor, &[a], window()).unwrap();
        assert!(result.complete && !result.ambiguous);
        assert_eq!(result.cells.len(), 63);
        assert!(result.cells.iter().all(|c| c.location == a.location));
        assert_eq!(
            result
                .cells
                .iter()
                .map(|c| c.position)
                .collect::<BTreeSet<_>>()
                .len(),
            63
        );
        for (min, max) in [(-31, -31), (-1, 1), (31, 31)] {
            assert_clipped_projection(
                &game.explored_maps[&actor],
                &[a],
                MapWindow {
                    min: pos(min, 0, 0),
                    max: pos(max, 0, 0),
                },
                window(),
                &result,
            );
        }
    }
    #[test]
    fn conflicting_cycle_suppresses_only_ambiguous_placements() {
        let mut map = ExploredMap::default();
        let [a, b, c, d, e] = [1, 2, 3, 4, 5].map(|x| at(1, x));
        for cell in [a, b, c, d, e] {
            map.cells.insert(cell, Shared::new(appearance("wall")));
        }
        let mut link = |from, direction, to| {
            map.neighbors.insert(
                (from, direction),
                Neighbor::Placed {
                    target: to,
                    rotation: 0,
                },
            );
            map.neighbors.insert(
                (to, opposite(direction)),
                Neighbor::Placed {
                    target: from,
                    rotation: 0,
                },
            );
        };
        link(a, Direction::East, b);
        link(a, Direction::North, d);
        link(b, Direction::North, c);
        link(d, Direction::East, e);
        let result = map.project(&[seen(a, pos(0, 0, 0), 0)], window()).unwrap();
        assert!(result.complete && result.ambiguous);
        assert_eq!(
            result
                .cells
                .iter()
                .map(|c| c.location)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([a, b, d])
        );
        for output in [
            MapWindow {
                min: pos(1, -1, 0),
                max: pos(1, 0, 0),
            },
            MapWindow {
                min: pos(31, 0, 0),
                max: pos(31, 0, 0),
            },
        ] {
            assert_clipped_projection(&map, &[seen(a, pos(0, 0, 0), 0)], output, window(), &result);
        }
    }
    #[test]
    fn observers_keep_independent_last_seen_values_and_observed_emptiness_clears_items() {
        let (mut game, first) = game();
        let second = game
            .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let cell = seen(at(1, 1), pos(0, 0, 0), 0);
        let mut old = appearance("closed wall");
        old.door = Some(MapDoorAppearance {
            name: "old gate".into(),
            description: "old gate".into(),
            open: false,
            asset: Some("old door".into()),
        });
        old.ground_items.push(MapItemAppearance {
            presentation: None,
            class: ItemClass::Potion,
            description: "unknown bottle".into(),
            name: "blue bottle".into(),
            appearance: "blue bottle".into(),
            identified: false,
            quantity: 2,
            asset: Some("old bottle".into()),
        });
        for observer in [first, second] {
            game.record_explored_observation(
                observer,
                &[MapCellInput {
                    location: cell.location,
                    appearance: old.clone(),
                }],
                &[cell],
            )
            .unwrap();
        }
        let root = game.map_knowledge_root(second);
        game.record_explored_observation(
            first,
            &[MapCellInput {
                location: cell.location,
                appearance: appearance("open floor"),
            }],
            &[cell],
        )
        .unwrap();
        assert!(game.map_knowledge_is_current(second, &root));
        let stale = game
            .project_explored_map(second, &[cell], window())
            .unwrap();
        assert_eq!(&*stale.cells[0].appearance, &old);
        let fresh = game.project_explored_map(first, &[cell], window()).unwrap();
        assert!(fresh.cells[0].appearance.ground_items.is_empty());
        assert!(fresh.cells[0].appearance.door.is_none());
        assert!(
            game.map_knowledge_is_current(second, &root),
            "read-only projection must not learn"
        );
    }
    #[test]
    fn legitimate_oversized_appearance_is_acquired_and_persisted_but_not_materialized_for_wire() {
        let (mut game, actor) = game();
        let cell = seen(at(1, 1), pos(0, 0, 0), 0);
        let mut large = appearance("stone");
        large.asset = Some("a".repeat(8192));
        large.ground_items = (0..4097)
            .map(|_| MapItemAppearance {
                presentation: None,
                class: ItemClass::Potion,
                description: String::new(),
                name: "bottle".into(),
                appearance: "bottle".into(),
                identified: false,
                quantity: 1,
                asset: None,
            })
            .collect();
        game.record_explored_observation(
            actor,
            &[MapCellInput {
                location: cell.location,
                appearance: large.clone(),
            }],
            &[cell],
        )
        .unwrap();
        let saved = serde_json::to_vec(&*game.explored_maps[&actor]).unwrap();
        let restored: ExploredMap = serde_json::from_slice(&saved).unwrap();
        assert!(restored.checkpoint_valid(&game.world));
        assert_eq!(&**restored.cells.get(&cell.location).unwrap(), &large);
        reset_map_work_counts();
        let result = restored.project(&[cell], window()).unwrap();
        assert!(result.limited && result.cells.is_empty());
        assert_eq!(map_work_counts().appearance_payload_copies, 0);
    }

    #[test]
    fn blocked_state_discovery_is_translation_invariant() {
        let mut map = ExploredMap::default();
        let [a, x, b, c, d, f] = [1, 2, 3, 4, 5, 6].map(|n| at(1, n));
        for cell in [a, x, b, c, d, f] {
            map.cells.insert(cell, Shared::new(appearance("wall")));
        }
        for (anchor, delta, child) in [(a, [2, 0, 0], b), (a, [3, 0, 0], f), (x, [-8, 0, 0], c)] {
            map.witnesses.insert(
                child,
                Witness::Placed {
                    anchor,
                    delta,
                    rotation: 0,
                },
            );
            map.children.insert((anchor, delta, child), 0);
        }
        map.neighbors.insert(
            (b, Direction::East),
            Neighbor::Placed {
                target: d,
                rotation: 0,
            },
        );
        map.neighbors.insert(
            (d, Direction::West),
            Neighbor::Placed {
                target: b,
                rotation: 0,
            },
        );
        let projected = |shift| {
            map.project(
                &[
                    seen(a, pos(shift, 0, 0), 0),
                    seen(x, pos(shift + 10, 0, 0), 0),
                ],
                window(),
            )
            .unwrap()
        };
        let first = projected(0);
        let translated = projected(-10);
        for result in [&first, &translated] {
            assert!(result.complete && result.ambiguous && !result.limited);
            assert_eq!(
                result
                    .cells
                    .iter()
                    .map(|c| c.location)
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([a, x])
            );
        }
        assert_eq!(
            first
                .cells
                .iter()
                .map(|c| (c.location, c.position))
                .collect::<Vec<_>>(),
            translated
                .cells
                .iter()
                .map(|c| (c.location, offset(c.position, [10, 0, 0]).unwrap()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn saved_item_labels_obey_the_ordinary_source_bounds() {
        let mut item = MapItemAppearance {
            presentation: None,
            class: ItemClass::Potion,
            description: String::new(),
            name: "bottle".into(),
            appearance: "blue bottle".into(),
            identified: false,
            quantity: 1,
            asset: Some("a".repeat(8192)),
        };
        for invalid in [String::new(), "x".repeat(81)] {
            let mut bad = item.clone();
            bad.name = invalid.clone();
            assert!(serde_json::from_value::<MapItemAppearance>(
                serde_json::to_value(bad).unwrap()
            )
            .is_err());
            let mut bad = item.clone();
            bad.appearance = invalid;
            assert!(serde_json::from_value::<MapItemAppearance>(
                serde_json::to_value(bad).unwrap()
            )
            .is_err());
        }
        item.name = "x".repeat(80);
        item.appearance = "y".repeat(80);
        assert_eq!(
            serde_json::from_value::<MapItemAppearance>(serde_json::to_value(&item).unwrap())
                .unwrap(),
            item
        );
        let mut value = appearance("stone");
        value.ground_items.push(item);
        assert!(value.valid());
        value.ground_items[0].name.clear();
        assert!(!value.valid());
        value.ground_items[0].name = "bottle".into();
        value.ground_items[0].appearance = "x".repeat(81);
        assert!(!value.valid());
    }
}
