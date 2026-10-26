//! Canonical spatial compilation from validated source geometry and World links.
//! Source decoding, lazy installation and restored-root certification are owned
//! by the adjacent authoring/runtime adapters, not by this typed compiler.
#[cfg(test)]
use super::Package;
use crate::Failure;
use std::collections::{BTreeMap, BTreeSet};
use tor_protocol::ErrorCode;
use tor_simulation::spatial_scope::{
    registry::{Budget, CellRun, PrimaryRun, RoomSurfaces, Scope},
    Membership, PassageId, PrimarySpace, RoomId,
};
use tor_world::{Direction, Location, Position, RegionId, World};

#[derive(Clone, Debug)]
pub(super) struct CanonicalSpatialSource {
    pub mapping: Option<Vec<tor_simulation::spatial_scope::mapping::MappingRoomFrame>>,
    pub id: String,
    pub members: Vec<RegionId>,
    pub rooms: BTreeMap<RoomId, Vec<Location>>,
    pub passages: Vec<Location>,
    /// Explicit ordinary room ownership, separate from true interior volume.
    pub doorways: BTreeMap<Location, RoomId>,
    /// Exact retained closure face cells, never an extra six-face dilation basis.
    pub closure_faces: BTreeSet<Location>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CompilerWorkBudget {
    pub source_entries: u64,
    pub terrain_cells: u64,
}

fn spatial_invalid(message: &str) -> Failure {
    Failure::new(ErrorCode::InvalidAction, message)
}
fn spatial_capacity(message: &str) -> Failure {
    Failure::new(ErrorCode::ResourceLimit, message)
}

const FACES: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];
const PLANAR: [Direction; 4] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
];
type Key = (RegionId, i32, i32, i32);
fn key(p: Location) -> Key {
    (p.region, p.position.z, p.position.y, p.position.x)
}
fn location((region, z, y, x): Key) -> Location {
    Location {
        region,
        position: Position { x, y, z },
    }
}
fn push_run(runs: &mut Vec<CellRun>, p: Location, max: usize) -> Result<(), Failure> {
    if let Some(last) = runs.last_mut().filter(|last| {
        last.region == p.region
            && last.z == p.position.z
            && last.y == p.position.y
            && last.last_x.checked_add(1) == Some(p.position.x)
    }) {
        last.last_x = p.position.x;
        return Ok(());
    }
    if runs.len() >= max {
        return Err(spatial_capacity("run capacity"));
    }
    runs.push(CellRun {
        region: p.region,
        z: p.position.z,
        y: p.position.y,
        first_x: p.position.x,
        last_x: p.position.x,
    });
    Ok(())
}

fn mapping_budget(
    world: &World,
    scope: &Scope,
    frame_count: usize,
    exterior_runs: usize,
    source: &CompilerWorkBudget,
) -> Result<tor_simulation::spatial_scope::mapping::CertificateBudget, Failure> {
    use tor_simulation::spatial_scope::{codec, mapping};
    let capacity = || spatial_capacity("Mapping certificate arithmetic capacity");
    let shells = scope
        .rooms
        .iter()
        .try_fold(0usize, |n, room| n.checked_add(room.runs.len()))
        .ok_or_else(capacity)?;
    let joins = scope
        .members
        .iter()
        .try_fold(0usize, |n, region| {
            n.checked_add(world.exits(*region).count())
        })
        .ok_or_else(capacity)?;
    // Existing validated source/terrain envelopes remain unchanged. This is a
    // conservative operation count for actual input, not a smaller geometry cap.
    let cells = usize::try_from(source.source_entries).map_err(|_| capacity())?;
    let factor = (usize::BITS as usize)
        .checked_mul(8)
        .and_then(|n| n.checked_add(32))
        .ok_or_else(capacity)?;
    let work = cells
        .checked_mul(2)
        .and_then(|n| n.checked_add(frame_count))
        .and_then(|n| n.checked_add(shells))
        .and_then(|n| n.checked_add(joins.checked_mul(shells.checked_add(2)?)?))
        .and_then(|n| n.checked_mul(factor))
        .ok_or_else(capacity)?;
    // ShellRun's two fields plus one alignment allowance; no Vec size_hint or
    // arbitrary retained floor cap. The constructor prepays its actual scratch.
    let shell_bytes = std::mem::size_of::<CellRun>()
        .checked_add(std::mem::size_of::<RoomId>())
        .and_then(|n| n.checked_add(std::mem::align_of::<CellRun>()))
        .ok_or_else(capacity)?;
    let allocation_bytes =
        shells
            .checked_mul(shell_bytes)
            .and_then(|n| {
                n.checked_add(frame_count.checked_mul(
                    std::mem::size_of::<mapping::MappingRoomFrame>().checked_add(1)?,
                )?)
            })
            .and_then(|n| n.checked_add(exterior_runs.checked_mul(std::mem::size_of::<CellRun>())?))
            .ok_or_else(capacity)?;
    // One directory/root reserve is the existing codec's 30+21+3+11 charge.
    // The fixed 75-byte mapping allowance is already in the base Scope charge.
    let base = codec::scope_charge(scope)
        .map_err(|_| capacity())?
        .checked_sub(75)
        .ok_or_else(capacity)?;
    let cache_bytes = (tor_simulation::checkpoint::ARCHIVE_BYTE_LIMIT as u64)
        .saturating_sub(65)
        .saturating_sub(base);
    Ok(mapping::CertificateBudget {
        work,
        allocation_bytes,
        cache_bytes,
    })
}
pub(super) fn compile(
    world: &World,
    s: &CanonicalSpatialSource,
    b: &Budget,
    work: &CompilerWorkBudget,
) -> Result<Scope, Failure> {
    if s.id.is_empty() || s.members.is_empty() || s.members.windows(2).any(|r| r[0] >= r[1]) {
        return Err(spatial_invalid("source header"));
    }
    if b.scopes == 0
        || s.id.len() > b.label_bytes
        || s.members.len() > b.members
        || s.rooms.len() > b.rooms
    {
        return Err(spatial_capacity("source header capacity"));
    }
    // Bound input entries and member scan BEFORE allocating secondary maps.
    let mut input = u64::try_from(s.passages.len()).map_err(|_| spatial_capacity("overflow"))?;
    for n in s
        .rooms
        .values()
        .map(Vec::len)
        .chain([s.doorways.len(), s.closure_faces.len()])
    {
        input = input
            .checked_add(u64::try_from(n).map_err(|_| spatial_capacity("overflow"))?)
            .ok_or_else(|| spatial_capacity("overflow"))?;
    }
    if input > work.source_entries {
        return Err(spatial_capacity("source capacity"));
    }
    let mut scan = 0u64;
    for r in &s.members {
        let bounds = world
            .region(*r)
            .ok_or_else(|| spatial_invalid("missing member"))?
            .bounds;
        let (x, y, z) = bounds.dimensions();
        let n = u64::try_from(x)
            .map_err(|_| spatial_invalid("bounds"))?
            .checked_mul(u64::try_from(y).map_err(|_| spatial_invalid("bounds"))?)
            .and_then(|n| n.checked_mul(u64::try_from(z).ok()?))
            .ok_or_else(|| spatial_invalid("bounds"))?;
        scan = scan
            .checked_add(n)
            .ok_or_else(|| spatial_invalid("bounds"))?;
    }
    if scan > work.terrain_cells {
        return Err(spatial_capacity("geometry work capacity"));
    }
    let valid = |p: Location| {
        s.members.binary_search(&p.region).is_ok() && world.contains(p) && !world.is_wall(p)
    };
    let mut primary = BTreeMap::<Key, PrimarySpace>::new();
    for (room, cells) in &s.rooms {
        if cells.is_empty() {
            return Err(spatial_invalid("empty room"));
        }
        for p in cells {
            if !valid(*p) || primary.insert(key(*p), PrimarySpace::Room(*room)).is_some() {
                return Err(spatial_invalid("room ownership"));
            }
        }
    }
    let mut passage_cells = BTreeSet::new();
    for p in &s.passages {
        if !valid(*p) || primary.contains_key(&key(*p)) || !passage_cells.insert(key(*p)) {
            return Err(spatial_invalid("passage ownership"));
        }
    }
    let interiors = primary.clone(); // bounded by preflight input count; preserves true basis
    for (p, owner) in &s.doorways {
        if !valid(*p) || !s.rooms.contains_key(owner) {
            return Err(spatial_invalid("doorway owner"));
        }
        if interiors.get(&key(*p)) != Some(&PrimarySpace::Room(*owner)) {
            // Exterior ownership must be unique across physical planar room
            // neighbors, not merely contain the supplied owner. At most four
            // neighbors are visited; repeated cells of one room remain valid.
            let mut adjacent_owner = None;
            for direction in PLANAR {
                if let Some(PrimarySpace::Room(room)) = world
                    .physics_neighbor(*p, direction)
                    .and_then(|(neighbor, _)| interiors.get(&key(neighbor)))
                {
                    if adjacent_owner.is_some_and(|previous| previous != *room) {
                        return Err(spatial_invalid("ambiguous exterior doorway owner"));
                    }
                    adjacent_owner = Some(*room);
                }
            }
            if adjacent_owner != Some(*owner) {
                return Err(spatial_invalid("doorway not adjacent to owner"));
            }
        }
        if primary
            .insert(key(*p), PrimarySpace::Room(*owner))
            .is_some_and(|old| old != PrimarySpace::Room(*owner))
        {
            return Err(spatial_invalid("conflicting doorway"));
        }
        // Revised adapter emits passage-primary EXCLUDING room-owned doorways.
        if passage_cells.contains(&key(*p)) {
            return Err(spatial_invalid("doorway also passage-primary"));
        }
    }
    if s.closure_faces.iter().any(|p| !s.doorways.contains_key(p)) {
        return Err(spatial_invalid("closure owner"));
    }
    // One bounded grouping pass, not a rooms × closures rescan.
    let mut closure_by_room = BTreeMap::<RoomId, Vec<Location>>::new();
    for p in &s.closure_faces {
        closure_by_room.entry(s.doorways[p]).or_default().push(*p);
    }
    // Explicit source must cover source-open cells. Future portal-shell review
    // must establish this exact volume convention before general integration.
    for r in &s.members {
        let bounds = world
            .region(*r)
            .ok_or_else(|| spatial_invalid("member"))?
            .bounds;
        let o = bounds.origin();
        let (w, d, h) = bounds.dimensions();
        for dz in 0..h {
            for dy in 0..d {
                for dx in 0..w {
                    let p = Location {
                        region: *r,
                        position: Position {
                            x: o.x
                                .checked_add(dx)
                                .ok_or_else(|| spatial_capacity("coordinate"))?,
                            y: o.y
                                .checked_add(dy)
                                .ok_or_else(|| spatial_capacity("coordinate"))?,
                            z: o.z
                                .checked_add(dz)
                                .ok_or_else(|| spatial_capacity("coordinate"))?,
                        },
                    };
                    if !world.is_wall(p)
                        && !primary.contains_key(&key(p))
                        && !passage_cells.contains(&key(p))
                    {
                        return Err(spatial_invalid("omitted primary"));
                    }
                }
            }
        }
    }
    let mut remaining = passage_cells.clone();
    remaining.extend(s.doorways.keys().copied().map(key));
    let mut affiliation = BTreeMap::new();
    let mut ordinal = 0u32;
    while let Some(start) = remaining.pop_first() {
        if usize::try_from(ordinal).map_err(|_| spatial_capacity("component count"))? >= b.passages
        {
            return Err(spatial_capacity("passage capacity"));
        }
        ordinal = ordinal
            .checked_add(1)
            .ok_or_else(|| spatial_capacity("component count"))?;
        let id = PassageId::new(ordinal).ok_or_else(|| spatial_capacity("component id"))?;
        let mut pending = vec![start];
        while let Some(k) = pending.pop() {
            affiliation.insert(k, id);
            for direction in FACES {
                if let Some((p, _)) = world.physics_neighbor(location(k), direction) {
                    let n = key(p);
                    if remaining.remove(&n) {
                        pending.push(n)
                    }
                }
            }
        }
    }
    for p in &passage_cells {
        primary.insert(*p, PrimarySpace::Passage(affiliation[p]));
    }
    let mut refs =
        u64::try_from(primary.len()).map_err(|_| spatial_capacity("reference overflow"))?;
    if refs > b.expanded_refs {
        return Err(spatial_capacity("primary capacity"));
    }
    let mut primary_runs = Vec::<PrimaryRun>::new();
    for (k, space) in primary {
        let p = location(k);
        let doorway = s.doorways.contains_key(&p).then(|| affiliation[&k]);
        let membership =
            Membership::new(space, doorway).ok_or_else(|| spatial_invalid("membership"))?;
        if let Some(last) = primary_runs.last_mut().filter(|last| {
            last.membership == membership
                && last.cells.region == p.region
                && last.cells.z == p.position.z
                && last.cells.y == p.position.y
                && last.cells.last_x.checked_add(1) == Some(p.position.x)
        }) {
            last.cells.last_x = p.position.x;
            continue;
        }
        if primary_runs.len() >= b.runs {
            return Err(spatial_capacity("run capacity"));
        }
        primary_runs.push(PrimaryRun {
            cells: CellRun {
                region: p.region,
                z: p.position.z,
                y: p.position.y,
                first_x: p.position.x,
                last_x: p.position.x,
            },
            membership,
        });
    }
    let mut room_surfaces = Vec::new();
    let mut all_runs = primary_runs.len();
    for (room, interior) in &s.rooms {
        let mut cells = BTreeSet::new();
        for p in interior {
            let solid_faces = FACES
                .into_iter()
                .filter_map(|d| world.physics_neighbor(*p, d).map(|n| n.0))
                .filter(|q| world.is_wall(*q));
            for q in std::iter::once(*p).chain(solid_faces) {
                if !cells.contains(&key(q)) {
                    if refs >= b.expanded_refs {
                        return Err(spatial_capacity("surface capacity"));
                    };
                    refs += 1;
                    cells.insert(key(q));
                }
            }
        }
        for p in closure_by_room.get(room).into_iter().flatten() {
            if !cells.contains(&key(*p)) {
                if refs >= b.expanded_refs {
                    return Err(spatial_capacity("surface capacity"));
                };
                refs += 1;
                cells.insert(key(*p));
            }
        }
        let mut runs = Vec::new();
        for k in cells {
            push_run(&mut runs, location(k), b.runs.saturating_sub(all_runs))?;
        }
        all_runs = all_runs
            .checked_add(runs.len())
            .ok_or_else(|| spatial_capacity("runs"))?;
        room_surfaces.push(RoomSurfaces { room: *room, runs });
    }
    let mut scope = Scope {
        mapping: tor_simulation::spatial_scope::mapping::MappingCertification::Uncertified,
        id: s.id.clone(),
        members: s.members.clone(),
        primary: primary_runs,
        rooms: room_surfaces,
    };
    scope.validate(b).map_err(|error| match error {
        tor_simulation::spatial_scope::registry::Error::Capacity => {
            spatial_capacity("scope output capacity")
        }
        _ => spatial_invalid("scope structure"),
    })?;
    if let Some(frames) = &s.mapping {
        let mut exterior: Vec<_> = s
            .doorways
            .keys()
            .filter(|p| !interiors.contains_key(&key(**p)))
            .copied()
            .map(key)
            .collect();
        exterior.sort_unstable();
        let mut exterior_runs = Vec::new();
        for k in exterior {
            push_run(&mut exterior_runs, location(k), usize::MAX)?;
        }
        let cache = mapping_budget(world, &scope, frames.len(), exterior_runs.len(), work)?;
        scope.mapping = tor_simulation::spatial_scope::mapping::certify(
            world,
            &scope,
            frames,
            &exterior_runs,
            cache,
        )
        .map_err(|error| match error {
            tor_simulation::spatial_scope::mapping::Error::InvalidSource => {
                spatial_invalid("Invalid mapping construction certificate")
            }
            _ => spatial_capacity("Mapping certificate validation capacity"),
        })?;
    }
    Ok(scope)
}

#[cfg(test)]
pub(super) fn generated_source(
    package: &Package,
    identity: &str,
    seed: u64,
    budget: &Budget,
    work: &CompilerWorkBudget,
) -> Result<CanonicalSpatialSource, Failure> {
    let group = package.generation_group(identity, seed)?;
    if budget.scopes == 0
        || identity.is_empty()
        || identity.len() > budget.label_bytes
        || group.members.len() > budget.members
    {
        return Err(super::fail("Spatial source header capacity"));
    }
    // Bound generator's whole geometric domain before any materialization or
    // compiler secondary index. This is work, not encoded/output capacity.
    let mut terrain = 0u64;
    for id in &group.members {
        let def = package.structural_definition(seed, *id)?;
        let mut volume = 1u64;
        for n in def.size {
            let side = i64::from(n)
                .checked_add(if def.chamber { 2 } else { 0 })
                .ok_or_else(|| super::fail("Geometry work overflow"))?;
            volume = volume
                .checked_mul(u64::try_from(side).map_err(|_| super::fail("Geometry bounds"))?)
                .ok_or_else(|| super::fail("Geometry work overflow"))?;
        }
        terrain = terrain
            .checked_add(volume)
            .ok_or_else(|| super::fail("Geometry work overflow"))?;
    }
    if terrain > work.terrain_cells {
        return Err(super::fail("Geometry work capacity"));
    }
    let generated = package.generated_group(identity, seed)?;
    source_from_generated(identity, &group, &generated, budget, work)
}

pub(super) fn source_from_generated(
    identity: &str,
    group: &crate::generation_recipe::GenerationGroup,
    generated: &crate::generation_recipe::GeneratedGroup,
    budget: &Budget,
    work: &CompilerWorkBudget,
) -> Result<CanonicalSpatialSource, Failure> {
    let meta = &generated.spatial;
    if meta.ordinary_room_ids.len() > budget.rooms {
        return Err(super::fail("Room output capacity"));
    }
    let mut used = 0u64;
    let mut charge = || -> Result<(), Failure> {
        if used >= work.source_entries {
            return Err(super::fail("Source entry work capacity"));
        }
        used = used
            .checked_add(1)
            .ok_or_else(|| super::fail("Source entry overflow"))?;
        Ok(())
    };
    let mut rooms = BTreeMap::new();
    let mut interior = BTreeSet::new();
    for population_room in &generated.rooms {
        if !meta.ordinary_room_ids.contains(&population_room.id) {
            continue;
        }
        let id = RoomId::new(population_room.id).ok_or_else(|| super::fail("Room identity"))?;
        let mut cells = Vec::new();
        for floor in &population_room.cells {
            let def = generated
                .regions
                .get(&floor.region.0)
                .ok_or_else(|| super::fail("Room member"))?;
            if floor.position.z != 0 || def.size[2] < 2 {
                return Err(super::fail("Generated clearance convention"));
            }
            for z in 0..2 {
                charge()?;
                let p = Location {
                    position: tor_world::Position {
                        z,
                        ..floor.position
                    },
                    ..*floor
                };
                if !interior.insert(p) {
                    return Err(super::fail("Overlapping generated room interiors"));
                }
                cells.push(p);
            }
        }
        cells.sort();
        rooms.insert(id, cells);
    }
    if rooms.len() != meta.ordinary_room_ids.len() {
        return Err(super::fail("Missing retained ordinary-room projection"));
    }
    let mut doorways = BTreeMap::new();
    for (p, room) in &meta.doorway_owners {
        charge()?;
        let room = RoomId::new(*room).ok_or_else(|| super::fail("Doorway room identity"))?;
        if !rooms.contains_key(&room) || interior.contains(p) {
            return Err(super::fail("Generated external doorway ownership"));
        }
        doorways.insert(*p, room);
    }
    let mut passages = Vec::new();
    for (region, def) in &generated.regions {
        // generator::check group/version1 rejects authored openings, and no
        // group/maze/feature stage creates them. Do not generalize to authored terrain.
        if !def.openings.is_empty() {
            return Err(super::fail("Generated openings violate source convention"));
        }
        // Already bounded by generator work preflight; constructing the static
        // wall set does not install runtime door opacity or consume a RNG draw.
        let walls: BTreeSet<_> = def.walls.iter().copied().collect();
        for z in 0..def.size[2] {
            for y in 0..def.size[1] {
                for x in 0..def.size[0] {
                    if walls.contains(&[x, y, z]) {
                        continue;
                    }
                    let p = Location {
                        region: RegionId(*region),
                        position: tor_world::Position { x, y, z },
                    };
                    if !interior.contains(&p) && !doorways.contains_key(&p) {
                        charge()?;
                        passages.push(p);
                    }
                }
            }
        }
    }
    let mut closure_faces = BTreeSet::new();
    for p in &meta.closure_faces {
        if !doorways.contains_key(p) {
            return Err(super::fail("Closure face lacks retained ordinary owner"));
        }
        charge()?;
        closure_faces.insert(*p);
    }
    // Scope directory order is independent of canonical generation slot order.
    // Never reorder group.members: slot ordinal owns RoomId/geometry/RNG keys.
    let mut members: Vec<_> = group.members.iter().copied().map(RegionId).collect();
    members.sort();
    let mut mapping = Vec::new();
    for (room, cells) in &rooms {
        let mut previous = None;
        for cell in cells {
            if previous != Some(cell.region) {
                mapping.push(tor_simulation::spatial_scope::mapping::MappingRoomFrame {
                    room: *room,
                    region: cell.region,
                    up: Direction::Up,
                });
                previous = Some(cell.region);
            }
        }
    }
    Ok(CanonicalSpatialSource {
        mapping: Some(mapping),
        id: identity.to_owned(),
        members,
        rooms,
        passages,
        doorways,
        closure_faces,
    })
}

#[cfg(test)]
#[path = "scenario_spatial_tests.rs"]
mod tests;
