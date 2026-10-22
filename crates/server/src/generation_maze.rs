//! Opt-in bounded dark-room carving. Legacy feature execution remains untouched
//! unless at least one eligible room actually realizes a maze.
use super::*;

type Cell = (i32, i32);

pub(super) fn carve(
    run: &RecipeRun<'_>,
    open: &mut BTreeSet<Cell>,
    corridors: &BTreeSet<Cell>,
    rooms: &mut [Vec<Cell>],
    dark: &[bool],
    definitions: &mut BTreeMap<u64, RegionDef>,
    size: [i32; 3],
) -> Result<Vec<bool>, Failure> {
    let mut selected = vec![false; rooms.len()];
    let Some((stage_index, one_in)) =
        run.recipe.stages.iter().enumerate().find_map(|(i, stage)| {
            if let Operation::MazeRooms { config } = &stage.operation {
                Some((i, config.one_in))
            } else {
                None
            }
        })
    else {
        return Ok(selected);
    };
    let traps = run.recipe.stages.iter().find_map(|stage| {
        if let Operation::HiddenTraps { config } = &stage.operation {
            Some((stage, config))
        } else {
            None
        }
    });
    let maximum = traps.map_or(0, |(_, config)| config.count[1] as usize);
    let landings = traps.map_or(0, |(_, config)| {
        if config.hazards.is_some() {
            config.count[1] as usize
        } else {
            0
        }
    });
    let columns = run.group.grid[0] as usize;
    let offset = |slot: usize| {
        (
            (slot % columns) as i32 * size[0],
            (slot / columns) as i32 * size[1],
        )
    };
    let reserved: Vec<BTreeSet<Cell>> = run
        .group
        .members
        .iter()
        .map(|id| {
            let def = &definitions[id];
            def.anchors
                .iter()
                .filter(|(name, _)| {
                    !def.generate
                        .as_ref()
                        .is_some_and(|g| g.boundary_anchors.contains(name))
                })
                .map(|(_, at)| (at[0], at[1]))
                .collect()
        })
        .collect();
    // Each ordinary rectangle stays open. Its actual entry/stair XY and at
    // most L landing cells are unavailable; secrets cannot enter its interior.
    // This conservative proof does not depend on a favorable landing draw.
    let capacities: Vec<_> = rooms
        .iter()
        .enumerate()
        .map(|(slot, room)| {
            room.iter()
                .filter(|cell| !reserved[slot].contains(*cell))
                .count()
                .saturating_sub(landings)
        })
        .collect();
    if capacities.iter().sum::<usize>() < maximum {
        return Err(fail(
            "Maze ordinary-room capacity cannot preserve authored trap maximum",
        ));
    }
    let mut retained: usize = capacities
        .iter()
        .enumerate()
        .filter(|(slot, _)| !dark[*slot])
        .map(|(_, n)| n)
        .sum();
    let mut eligible: Vec<_> = rooms
        .iter()
        .enumerate()
        .filter_map(|(slot, room)| (!room.is_empty() && dark[slot]).then_some(slot))
        .collect();
    let mut rng = run.rng(stage_index, "ordinary-reserve")?;
    for i in (1..eligible.len()).rev() {
        let j = rng.range(0, i as i32) as usize;
        eligible.swap(i, j);
    }
    let mut held = BTreeSet::new();
    for &slot in &eligible {
        if retained >= maximum {
            break;
        }
        retained += capacities[slot];
        held.insert(slot);
    }
    for slot in eligible {
        if held.contains(&slot) {
            continue;
        }
        let mut rng = run.rng(stage_index, &format!("trial/{}", run.group.members[slot]))?;
        selected[slot] = rng.range(1, one_in as i32) == 1;
    }
    if !selected.iter().any(|value| *value) {
        return Ok(selected);
    }

    // Reserve the entire per-slot namespace on baseline floor, independently
    // of predecessor materialization and before secrets or maze walls exist.
    let mut planned = vec![Vec::<Cell>::new(); rooms.len()];
    if let Some((stage, _)) = traps.filter(|_| landings > 0) {
        for (slot, id) in run.group.members.iter().enumerate() {
            let (ox, oy) = offset(slot);
            let def = &definitions[id];
            let mut available: Vec<_> = open
                .iter()
                .filter_map(|&(x, y)| {
                    let cell = (x - ox, y - oy);
                    ((0..size[0]).contains(&cell.0)
                        && (0..size[1]).contains(&cell.1)
                        && !reserved[slot].contains(&cell)
                        && !(0..2).any(|z| def.walls.contains(&[cell.0, cell.1, z]))
                        && !def.doors.iter().any(|door| {
                            !door.open
                                && door.at[..2] == [cell.0, cell.1]
                                && door.at[2] < 2
                                && door.at[2] + door.height as i32 > 0
                        }))
                    .then_some(cell)
                })
                .collect();
            available.sort();
            let mut rng = stream(
                run.seed,
                run.identity,
                run.group,
                run.recipe,
                stage,
                &format!("landing/{id}"),
            )?;
            for i in (1..available.len()).rev() {
                let j = rng.range(0, i as i32) as usize;
                available.swap(i, j);
            }
            if available.len() < landings {
                return Err(fail(
                    "Trapdoor landing capacity exceeds clear floor before maze carving",
                ));
            }
            planned[slot].extend(available.into_iter().take(landings));
        }
    }
    for (slot, room) in rooms.iter_mut().enumerate() {
        if !selected[slot] {
            continue;
        }
        let id = run.group.members[slot];
        let (ox, oy) = offset(slot);
        let rectangle: BTreeSet<_> = room.iter().copied().collect();
        let min_x = room.iter().map(|c| c.0).min().expect("present room");
        let max_x = room.iter().map(|c| c.0).max().expect("present room");
        let min_y = room.iter().map(|c| c.1).min().expect("present room");
        let max_y = room.iter().map(|c| c.1).max().expect("present room");
        let root = (min_x, min_y);
        let mut carved = BTreeSet::from([root]);
        let mut visited = BTreeSet::from([root]);
        let mut stack = vec![root];
        let mut rng = run.rng(stage_index, &format!("carve/{id}"))?;
        while let Some(&(x, y)) = stack.last() {
            let next: Vec<_> = [(x - 2, y), (x + 2, y), (x, y - 2), (x, y + 2)]
                .into_iter()
                .filter(|&(x, y)| {
                    x >= min_x
                        && x <= max_x
                        && y >= min_y
                        && y <= max_y
                        && !visited.contains(&(x, y))
                })
                .collect();
            if next.is_empty() {
                stack.pop();
                continue;
            }
            let next = next[rng.range(0, next.len() as i32 - 1) as usize];
            visited.insert(next);
            carved.insert(((x + next.0) / 2, (y + next.1) / 2));
            carved.insert(next);
            stack.push(next);
        }
        let mandatory: BTreeSet<_> = rectangle
            .iter()
            .copied()
            .filter(|&(x, y)| corridors.contains(&(x + ox, y + oy)))
            .chain(
                reserved[slot]
                    .iter()
                    .copied()
                    .filter(|cell| rectangle.contains(cell)),
            )
            .chain(
                planned[slot]
                    .iter()
                    .copied()
                    .filter(|cell| rectangle.contains(cell)),
            )
            .collect();
        // Bounded deterministic connectors may introduce loops. Every original
        // corridor cell survives; this is a connected labyrinth, not a tree claim.
        for (mut x, mut y) in mandatory {
            if carved.contains(&(x, y)) {
                continue;
            }
            carved.insert((x, y));
            while x != root.0 {
                x += (root.0 - x).signum();
                carved.insert((x, y));
            }
            while y != root.1 {
                y += (root.1 - y).signum();
                carved.insert((x, y));
            }
        }
        let def = definitions.get_mut(&id).expect("member");
        for &(x, y) in room.iter() {
            if !carved.contains(&(x, y)) {
                open.remove(&(x + ox, y + oy));
                for z in 0..2 {
                    def.walls.push([x, y, z]);
                }
            }
        }
        // Full actual open rectangle, including all mandatory connectors, owns
        // both population metadata and the exterior-only secret-mouth boundary.
        *room = rectangle
            .into_iter()
            .filter(|&(x, y)| open.contains(&(x + ox, y + oy)))
            .collect();
    }
    for (slot, cells) in planned.iter().enumerate() {
        let def = definitions
            .get_mut(&run.group.members[slot])
            .expect("member");
        for (ordinal, &(x, y)) in cells.iter().enumerate() {
            if (0..2).any(|z| def.walls.contains(&[x, y, z])) {
                return Err(fail("Maze erased reserved landing clearance"));
            }
            if def
                .anchors
                .insert(format!("trapdoor_landing_{ordinal}"), [x, y, size[2] - 1])
                .is_some()
            {
                return Err(fail("Duplicate trapdoor landing anchor"));
            }
        }
    }
    Ok(selected)
}
