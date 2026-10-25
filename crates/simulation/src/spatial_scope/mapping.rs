//! Source-certified construction frames, not gravity or a second topology graph.
//! All budgets are caller supplied. Cache overflow is distinct from bad source;
//! declaration validation finishes before a capacity-unavailable cache is returned.
use super::registry::{CellRun, Scope};
use super::{Membership, PrimarySpace, RoomId};
use tor_world::{Direction, Location, Position, RegionId, World};

pub const CARDINAL: [Direction; 6] = [
    Direction::North,
    Direction::East,
    Direction::South,
    Direction::West,
    Direction::Up,
    Direction::Down,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappingRoomFrame {
    pub room: RoomId,
    pub region: RegionId,
    pub up: Direction,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingFrames {
    pub room_frames: Vec<MappingRoomFrame>,
    /// Derived by the source compiler, never authored. These room-owned mouths
    /// are not true interior and must not become a fresh shell-dilation basis.
    pub exterior_doorways: Vec<CellRun>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MappingCertification {
    Uncertified,
    Certified(MappingFrames),
    CapacityUnavailable,
}
#[derive(Clone, Copy, Debug)]
pub struct CertificateBudget {
    pub work: usize,
    pub allocation_bytes: usize,
    pub cache_bytes: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct FaceBudget {
    pub work: usize,
    pub allocation_bytes: usize,
    pub faces: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidSource,
    Work,
    Allocation,
    Arithmetic,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MappingUnavailable {
    Uncertified,
    Capacity,
    InvalidSource,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Private local proof. A chart projector rotates normal with its resolved
/// component frame (never translates it); inconsistent components must omit
/// both global position and orientation rather than assert an invented frame.
pub struct MappingFace {
    /// Member interior witness; foreign solid faces never expand scope ownership.
    pub anchor: Location,
    pub toward: Direction,
    pub room: RoomId,
    pub location: Location,
    pub normal: Direction,
}

type Key = (RegionId, i32, i32, i32);
fn key(p: Location) -> Key {
    (p.region, p.position.z, p.position.y, p.position.x)
}
fn start(r: CellRun) -> Key {
    (r.region, r.z, r.y, r.first_x)
}
fn at(r: CellRun, x: i32) -> Location {
    Location {
        region: r.region,
        position: Position { x, y: r.y, z: r.z },
    }
}
fn pay(work: &mut usize, amount: usize) -> Result<(), Error> {
    *work = work.checked_sub(amount).ok_or(Error::Work)?;
    Ok(())
}
fn reserve<T>(n: usize, work: &mut usize, allocation: &mut usize) -> Result<Vec<T>, Error> {
    pay(work, n)?;
    let bytes = n
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(Error::Arithmetic)?;
    *allocation = allocation.checked_sub(bytes).ok_or(Error::Allocation)?;
    let mut result = Vec::new();
    result.try_reserve_exact(n).map_err(|_| Error::Allocation)?;
    Ok(result)
}
fn lookup_work(work: &mut usize, n: usize) -> Result<(), Error> {
    pay(work, usize::BITS as usize - n.leading_zeros() as usize + 1)
}
fn membership(scope: &Scope, p: Location, work: &mut usize) -> Result<Option<Membership>, Error> {
    lookup_work(work, scope.primary.len())?;
    let i = scope.primary.partition_point(|r| start(r.cells) <= key(p));
    Ok(i.checked_sub(1)
        .and_then(|i| scope.primary.get(i))
        .filter(|r| r.cells.contains(p))
        .map(|r| r.membership))
}
fn contains(runs: &[CellRun], p: Location, work: &mut usize) -> Result<bool, Error> {
    lookup_work(work, runs.len())?;
    let i = runs.partition_point(|r| start(*r) <= key(p));
    Ok(i.checked_sub(1)
        .and_then(|i| runs.get(i))
        .is_some_and(|r| r.contains(p)))
}
fn frame(
    frames: &[MappingRoomFrame],
    room: RoomId,
    region: RegionId,
    work: &mut usize,
) -> Result<Option<usize>, Error> {
    lookup_work(work, frames.len())?;
    Ok(frames
        .binary_search_by_key(&(room, region), |f| (f.room, f.region))
        .ok())
}
fn canonical_runs(runs: &[CellRun]) -> bool {
    runs.iter().all(|r| r.expanded_len().is_some())
        && runs.windows(2).all(|w| {
            let (a, b) = (w[0], w[1]);
            start(a) < start(b)
                && ((a.region, a.z, a.y) != (b.region, b.z, b.y)
                    || i64::from(a.last_x) + 1 < i64::from(b.first_x))
        })
}
fn canonical_frames(frames: &[MappingRoomFrame]) -> bool {
    frames.iter().all(|f| CARDINAL.contains(&f.up))
        && frames
            .windows(2)
            .all(|w| (w[0].room, w[0].region) < (w[1].room, w[1].region))
}

/// Independent structural validation. Exact exterior provenance still belongs
/// to the source compiler/audit; this verifies all references and full incidence
/// coverage without a World, arbitrary source defaults, or a new scope limit.
pub fn validate_structure(
    scope: &Scope,
    frames: &MappingFrames,
    mut budget: CertificateBudget,
) -> Result<(), Error> {
    pay(
        &mut budget.work,
        frames
            .room_frames
            .len()
            .checked_add(frames.exterior_doorways.len())
            .and_then(|n| n.checked_mul(2))
            .ok_or(Error::Arithmetic)?,
    )?;
    if !canonical_frames(&frames.room_frames) || !canonical_runs(&frames.exterior_doorways) {
        return Err(Error::InvalidSource);
    }
    let mut seen = reserve::<bool>(
        frames.room_frames.len(),
        &mut budget.work,
        &mut budget.allocation_bytes,
    )?;
    seen.resize(frames.room_frames.len(), false);
    // Interval containment, not expanded-cell iteration: one legal i32-wide
    // saved run must not turn a small metadata record into billions of steps.
    for run in &frames.exterior_doorways {
        pay(&mut budget.work, 1)?;
        lookup_work(&mut budget.work, scope.primary.len())?;
        let first = at(*run, run.first_x);
        let mut i = scope
            .primary
            .partition_point(|r| start(r.cells) <= key(first))
            .checked_sub(1)
            .ok_or(Error::InvalidSource)?;
        let mut cursor = i64::from(run.first_x);
        while cursor <= i64::from(run.last_x) {
            pay(&mut budget.work, 1)?;
            let primary = scope.primary.get(i).ok_or(Error::InvalidSource)?;
            let p = at(*run, i32::try_from(cursor).map_err(|_| Error::Arithmetic)?);
            if !primary.cells.contains(p)
                || !matches!(primary.membership.primary, PrimarySpace::Room(_))
                || primary.membership.doorway.is_none()
            {
                return Err(Error::InvalidSource);
            }
            cursor = i64::from(primary.cells.last_x) + 1;
            i = i.checked_add(1).ok_or(Error::Arithmetic)?;
        }
    }
    for run in &scope.primary {
        pay(&mut budget.work, 1)?;
        let PrimarySpace::Room(room) = run.membership.primary else {
            continue;
        };
        lookup_work(&mut budget.work, frames.exterior_doorways.len())?;
        let first = at(run.cells, run.cells.first_x);
        let i = frames
            .exterior_doorways
            .partition_point(|r| start(*r) <= key(first));
        let fully_exterior = i
            .checked_sub(1)
            .and_then(|i| frames.exterior_doorways.get(i))
            .is_some_and(|r| r.contains(first) && r.last_x >= run.cells.last_x);
        // Canonical exclusion runs merge all adjacent cells, so anything not
        // covered by one run has at least one true interior cell.
        if fully_exterior {
            continue;
        }
        let i = frame(
            &frames.room_frames,
            room,
            run.cells.region,
            &mut budget.work,
        )?
        .ok_or(Error::InvalidSource)?;
        seen[i] = true;
    }
    pay(&mut budget.work, seen.len())?;
    if seen.iter().any(|v| !v) {
        return Err(Error::InvalidSource);
    }
    Ok(())
}

/// Conservative named-map JSON cache charge, including comma allowance.
/// Codec tests independently pin these constants against maximal operands.
pub fn cache_charge(frames: &MappingFrames) -> Result<u64, Error> {
    75u64
        .checked_add(
            u64::try_from(frames.room_frames.len())
                .map_err(|_| Error::Arithmetic)?
                .checked_mul(68)
                .ok_or(Error::Arithmetic)?,
        )
        .and_then(|n| {
            n.checked_add(
                u64::try_from(frames.exterior_doorways.len())
                    .ok()?
                    .checked_mul(107)?,
            )
        })
        .ok_or(Error::Arithmetic)
}

#[derive(Clone, Copy)]
struct ShellRun {
    room: RoomId,
    cells: CellRun,
}
fn room_affiliated(
    scope: &Scope,
    room: RoomId,
    p: Location,
    work: &mut usize,
) -> Result<bool, Error> {
    if membership(scope, p, work)?.is_some_and(|m| m.primary == PrimarySpace::Room(room)) {
        return Ok(true);
    }
    lookup_work(work, scope.rooms.len())?;
    let Ok(i) = scope.rooms.binary_search_by_key(&room, |r| r.room) else {
        return Ok(false);
    };
    contains(&scope.rooms[i].runs, p, work)
}
fn check_join(
    scope: &Scope,
    frames: &[MappingRoomFrame],
    room: RoomId,
    from: Location,
    to: Location,
    rotation: u8,
    work: &mut usize,
) -> Result<(), Error> {
    let (Some(a), Some(b)) = (
        frame(frames, room, from.region, work)?,
        frame(frames, room, to.region, work)?,
    ) else {
        return Ok(());
    };
    if room_affiliated(scope, room, to, work)? && frames[a].up.rotated(rotation) != frames[b].up {
        return Err(Error::InvalidSource);
    }
    Ok(())
}

/// The supplied typed vectors must already be source-bounded. Validation uses
/// explicit physical exits (not walkability), including self/opaque joins.
pub fn certify(
    world: &World,
    scope: &Scope,
    frames: &[MappingRoomFrame],
    exterior: &[CellRun],
    mut budget: CertificateBudget,
) -> Result<MappingCertification, Error> {
    // Borrowed structural helper avoids copying a potentially uncacheable input.
    pay(
        &mut budget.work,
        frames
            .len()
            .checked_add(exterior.len())
            .and_then(|n| n.checked_mul(2))
            .ok_or(Error::Arithmetic)?,
    )?;
    if !canonical_frames(frames) || !canonical_runs(exterior) {
        return Err(Error::InvalidSource);
    }
    let mut seen = reserve::<bool>(frames.len(), &mut budget.work, &mut budget.allocation_bytes)?;
    seen.resize(frames.len(), false);
    for run in exterior {
        pay(&mut budget.work, 1)?;
        for x in run.first_x..=run.last_x {
            pay(&mut budget.work, 1)?;
            let member =
                membership(scope, at(*run, x), &mut budget.work)?.ok_or(Error::InvalidSource)?;
            if !matches!(member.primary, PrimarySpace::Room(_)) || member.doorway.is_none() {
                return Err(Error::InvalidSource);
            }
        }
    }
    for run in &scope.primary {
        pay(&mut budget.work, 1)?;
        let PrimarySpace::Room(room) = run.membership.primary else {
            continue;
        };
        for x in run.cells.first_x..=run.cells.last_x {
            pay(&mut budget.work, 1)?;
            let p = at(run.cells, x);
            if contains(exterior, p, &mut budget.work)? {
                continue;
            }
            if !world.contains(p) || world.is_wall(p) {
                return Err(Error::InvalidSource);
            }
            seen[frame(frames, room, p.region, &mut budget.work)?.ok_or(Error::InvalidSource)?] =
                true;
        }
    }
    pay(&mut budget.work, seen.len())?;
    if seen.iter().any(|v| !v) {
        return Err(Error::InvalidSource);
    }
    pay(&mut budget.work, scope.rooms.len())?;
    let count = scope
        .rooms
        .iter()
        .try_fold(0usize, |n, r| n.checked_add(r.runs.len()))
        .ok_or(Error::Arithmetic)?;
    let mut shell = reserve::<ShellRun>(count, &mut budget.work, &mut budget.allocation_bytes)?;
    for room in &scope.rooms {
        pay(&mut budget.work, 1)?;
        for cells in &room.runs {
            shell.push(ShellRun {
                room: room.room,
                cells: *cells,
            });
        }
    }
    pay(
        &mut budget.work,
        count
            .checked_mul(usize::BITS as usize - count.leading_zeros() as usize + 1)
            .ok_or(Error::Arithmetic)?,
    )?;
    shell.sort_unstable_by_key(|r| {
        (
            r.cells.region,
            r.cells.z,
            r.cells.y,
            r.cells.first_x,
            r.room,
        )
    });
    for region in &scope.members {
        pay(&mut budget.work, 1)?;
        for join in world.exits(*region) {
            pay(&mut budget.work, 1)?;
            if world.is_stair(join.from, join.direction) {
                continue;
            }
            let rotation = world.crossing_rotation(join.from, join.direction);
            if let Some(Membership {
                primary: PrimarySpace::Room(room),
                ..
            }) = membership(scope, join.from, &mut budget.work)?
            {
                check_join(
                    scope,
                    frames,
                    room,
                    join.from,
                    join.to,
                    rotation,
                    &mut budget.work,
                )?;
            }
            lookup_work(&mut budget.work, shell.len())?;
            let row = (join.from.region, join.from.position.z, join.from.position.y);
            let first = shell.partition_point(|r| (r.cells.region, r.cells.z, r.cells.y) < row);
            for run in &shell[first..] {
                pay(&mut budget.work, 1)?;
                if (run.cells.region, run.cells.z, run.cells.y) != row {
                    break;
                }
                if run.cells.contains(join.from) {
                    check_join(
                        scope,
                        frames,
                        run.room,
                        join.from,
                        join.to,
                        rotation,
                        &mut budget.work,
                    )?;
                }
            }
        }
    }
    let charge = 75u64
        .checked_add(
            u64::try_from(frames.len())
                .map_err(|_| Error::Arithmetic)?
                .checked_mul(68)
                .ok_or(Error::Arithmetic)?,
        )
        .and_then(|n| n.checked_add(u64::try_from(exterior.len()).ok()?.checked_mul(107)?))
        .ok_or(Error::Arithmetic)?;
    if charge > budget.cache_bytes {
        return Ok(MappingCertification::CapacityUnavailable);
    }
    let mut owned = match reserve(frames.len(), &mut budget.work, &mut budget.allocation_bytes) {
        Ok(value) => value,
        Err(_) => return Ok(MappingCertification::CapacityUnavailable),
    };
    let mut mouths = match reserve(
        exterior.len(),
        &mut budget.work,
        &mut budget.allocation_bytes,
    ) {
        Ok(value) => value,
        Err(_) => return Ok(MappingCertification::CapacityUnavailable),
    };
    owned.extend_from_slice(frames);
    mouths.extend_from_slice(exterior);
    Ok(MappingCertification::Certified(MappingFrames {
        room_frames: owned,
        exterior_doorways: mouths,
    }))
}

fn opposite(d: Direction) -> Direction {
    let (x, y, z) = d.delta();
    Direction::from_delta([-i64::from(x), -i64::from(y), -i64::from(z)]).expect("cardinal opposite")
}
/// Frozen candidate producer only; callers publish no partial prefix on error.
/// Does not change ordinary map memory, feature knowledge, RNG or navigation.
pub fn wall_faces(
    world: &World,
    scope: &Scope,
    budget: FaceBudget,
) -> Result<Vec<MappingFace>, MappingUnavailable> {
    wall_faces_with_usage(world, scope, budget).map(|result| result.faces)
}
pub(crate) struct FaceResult {
    pub faces: Vec<MappingFace>,
    pub work: usize,
    pub allocation_bytes: usize,
}
pub(crate) fn wall_faces_with_usage(
    world: &World,
    scope: &Scope,
    mut budget: FaceBudget,
) -> Result<FaceResult, MappingUnavailable> {
    let initial = budget;
    pay(&mut budget.work, 1).map_err(|_| MappingUnavailable::Capacity)?;
    let frames = match &scope.mapping {
        MappingCertification::Uncertified => return Err(MappingUnavailable::Uncertified),
        MappingCertification::CapacityUnavailable => return Err(MappingUnavailable::Capacity),
        MappingCertification::Certified(frames) => frames,
    };
    let convert = |e| match e {
        Error::InvalidSource => MappingUnavailable::InvalidSource,
        Error::Work | Error::Allocation | Error::Arithmetic => MappingUnavailable::Capacity,
    };
    // geometry_step/rim_step may inspect EVERY exit of this source region,
    // even when its matching face has no aperture. Count once under the same
    // work budget; prepay that conservative hidden traversal for each request.
    let mut exit_counts = reserve::<usize>(
        scope.members.len(),
        &mut budget.work,
        &mut budget.allocation_bytes,
    )
    .map_err(convert)?;
    for region in &scope.members {
        pay(&mut budget.work, 1).map_err(convert)?;
        let mut count = 0usize;
        for _ in world.exits(*region) {
            pay(&mut budget.work, 1).map_err(convert)?;
            count = count.checked_add(1).ok_or(MappingUnavailable::Capacity)?;
        }
        exit_counts.push(count);
    }
    // Meter a complete counting pass first; a tiny face budget cannot cause a
    // massive maximum-capacity allocation or a truncated available result.
    let mut count = 0usize;
    for run in &scope.primary {
        pay(&mut budget.work, 1).map_err(convert)?;
        let PrimarySpace::Room(room) = run.membership.primary else {
            continue;
        };
        lookup_work(&mut budget.work, scope.members.len()).map_err(convert)?;
        let region = scope
            .members
            .binary_search(&run.cells.region)
            .map_err(|_| MappingUnavailable::InvalidSource)?;
        let hidden_exit_work = exit_counts[region];
        for x in run.cells.first_x..=run.cells.last_x {
            pay(&mut budget.work, 1).map_err(convert)?;
            let p = at(run.cells, x);
            if contains(&frames.exterior_doorways, p, &mut budget.work).map_err(convert)? {
                continue;
            }
            if !world.contains(p) {
                return Err(MappingUnavailable::InvalidSource);
            }
            if world.is_wall(p) {
                continue;
            }
            let i = frame(
                &frames.room_frames,
                room,
                run.cells.region,
                &mut budget.work,
            )
            .map_err(convert)?
            .ok_or(MappingUnavailable::InvalidSource)?;
            let up = frames.room_frames[i].up;
            for d in CARDINAL {
                pay(&mut budget.work, 1).map_err(convert)?;
                if d == up || d == opposite(up) {
                    continue;
                }
                pay(&mut budget.work, hidden_exit_work).map_err(convert)?;
                if world
                    .physical_geometry_neighbor(p, d)
                    .is_some_and(|(q, _)| world.is_wall(q))
                {
                    count = count.checked_add(1).ok_or(MappingUnavailable::Capacity)?;
                    if count > budget.faces {
                        return Err(MappingUnavailable::Capacity);
                    }
                }
            }
        }
    }
    let mut result =
        reserve(count, &mut budget.work, &mut budget.allocation_bytes).map_err(convert)?;
    for run in &scope.primary {
        pay(&mut budget.work, 1).map_err(convert)?;
        let PrimarySpace::Room(room) = run.membership.primary else {
            continue;
        };
        lookup_work(&mut budget.work, scope.members.len()).map_err(convert)?;
        let region = scope
            .members
            .binary_search(&run.cells.region)
            .map_err(|_| MappingUnavailable::InvalidSource)?;
        let hidden_exit_work = exit_counts[region];
        for x in run.cells.first_x..=run.cells.last_x {
            pay(&mut budget.work, 1).map_err(convert)?;
            let p = at(run.cells, x);
            if contains(&frames.exterior_doorways, p, &mut budget.work).map_err(convert)? {
                continue;
            }
            if !world.contains(p) {
                return Err(MappingUnavailable::InvalidSource);
            }
            if world.is_wall(p) {
                continue;
            }
            let i = frame(
                &frames.room_frames,
                room,
                run.cells.region,
                &mut budget.work,
            )
            .map_err(convert)?
            .ok_or(MappingUnavailable::InvalidSource)?;
            let up = frames.room_frames[i].up;
            for d in CARDINAL {
                pay(&mut budget.work, 1).map_err(convert)?;
                if d == up || d == opposite(up) {
                    continue;
                }
                pay(&mut budget.work, hidden_exit_work).map_err(convert)?;
                if let Some((q, rotation)) = world
                    .physical_geometry_neighbor(p, d)
                    .filter(|(q, _)| world.is_wall(*q))
                {
                    result.push(MappingFace {
                        anchor: p,
                        toward: d,
                        room,
                        location: q,
                        normal: opposite(d.rotated(rotation)),
                    });
                }
            }
        }
    }
    pay(
        &mut budget.work,
        count
            .checked_mul(usize::BITS as usize - count.leading_zeros() as usize + 1)
            .ok_or(MappingUnavailable::Capacity)?,
    )
    .map_err(convert)?;
    result.sort_unstable_by_key(|f| (f.room, key(f.anchor), f.toward, key(f.location), f.normal));
    pay(&mut budget.work, result.len()).map_err(convert)?;
    result.dedup();
    Ok(FaceResult {
        faces: result,
        work: initial.work - budget.work,
        allocation_bytes: initial.allocation_bytes - budget.allocation_bytes,
    })
}

#[cfg(test)]
pub(crate) mod tests;
