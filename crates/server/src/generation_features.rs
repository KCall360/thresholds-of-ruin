//! Independent, bounded feature stages. They never mutate geometry or its streams.
use super::*;

/// A region owns one full maximum-sized slot's worth of feature identities.
pub const FEATURE_IDENTITY_STRIDE: u64 = 1024;
#[derive(Clone, Copy)]
pub struct FeatureBases {
    pub doors: u64,
    pub traps: u64,
}
fn identity(base: u64, region: u64, ordinal: usize) -> Result<u64, Failure> {
    if ordinal as u64 >= FEATURE_IDENTITY_STRIDE {
        return Err(fail("Generated feature capacity exceeded"));
    }
    region
        .checked_sub(1)
        .and_then(|n| n.checked_mul(FEATURE_IDENTITY_STRIDE))
        .and_then(|n| n.checked_add(base))
        .and_then(|n| n.checked_add(ordinal as u64))
        .filter(|id| *id > 0 && *id < u64::MAX)
        .ok_or_else(|| fail("Generated feature identity overflow"))
}

pub(super) fn place_concealed_features(
    run: &RecipeRun<'_>,
    open: &BTreeSet<(i32, i32)>,
    rooms: &[Vec<(i32, i32)>],
    definitions: &mut BTreeMap<u64, RegionDef>,
    size: [i32; 3],
    bases: FeatureBases,
) -> Result<(), Failure> {
    let columns = run.group.grid[0] as usize;
    let to_local = |(x, y): (i32, i32)| {
        let slot = (y / size[1]) as usize * columns + (x / size[0]) as usize;
        (run.group.members[slot], [x % size[0], y % size[1], 0])
    };
    let reserved: BTreeSet<_> = run
        .group
        .members
        .iter()
        .enumerate()
        .flat_map(|(slot, id)| {
            let offset = (
                (slot % columns) as i32 * size[0],
                (slot / columns) as i32 * size[1],
            );
            definitions[id]
                .anchors
                .iter()
                .filter(|(name, _)| {
                    !definitions[id]
                        .generate
                        .as_ref()
                        .is_some_and(|g| g.boundary_anchors.contains(name))
                })
                .map(move |(_, at)| (at[0] + offset.0, at[1] + offset.1))
        })
        .collect();
    let illuminated: BTreeSet<_> = run
        .group
        .members
        .iter()
        .enumerate()
        .flat_map(|(slot, id)| {
            let offset = (
                (slot % columns) as i32 * size[0],
                (slot / columns) as i32 * size[1],
            );
            definitions[id]
                .lighting
                .iter()
                .filter(|light| light.lit && light.at[2] == 0)
                .map(move |light| (light.at[0] + offset.0, light.at[1] + offset.1))
        })
        .collect();
    for stage in run.recipe.stages.iter().skip(6) {
        match &stage.operation {
            Operation::SecretEntrances { config } => {
                // Only a one-cell-wide corridor immediately outside a room may
                // receive a full-height closure. Wide ordinary entrances stay open.
                let mut candidates = BTreeSet::new();
                for (slot, room) in rooms.iter().enumerate() {
                    let offset = (
                        (slot % columns) as i32 * size[0],
                        (slot / columns) as i32 * size[1],
                    );
                    let cells: BTreeSet<_> = room
                        .iter()
                        .map(|&(x, y)| (x + offset.0, y + offset.1))
                        .collect();
                    for &(x, y) in &cells {
                        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                            let mouth = (x + dx, y + dy);
                            if !cells.contains(&mouth)
                                && open.contains(&mouth)
                                && open.contains(&(x + 2 * dx, y + 2 * dy))
                                && !open.contains(&(mouth.0 + dy, mouth.1 + dx))
                                && !open.contains(&(mouth.0 - dy, mouth.1 - dx))
                                && !reserved.contains(&mouth)
                            {
                                candidates.insert(mouth);
                            }
                        }
                    }
                }
                let mut placed = 0;
                let mut ordinals = BTreeMap::<u64, usize>::new();
                for candidate in candidates {
                    let (region, at) = to_local(candidate);
                    // A coordinate-keyed draw avoids unrelated candidate changes
                    // shifting this entrance's chance or consuming geometry RNG.
                    let mut rng = stream(
                        run.seed,
                        run.identity,
                        run.group,
                        run.recipe,
                        stage,
                        &format!("entrance/{region}/{}/{}", at[0], at[1]),
                    )?;
                    if placed >= config.maximum
                        || rng.range(0, 999) as u32 >= config.chance_per_mille
                    {
                        continue;
                    }
                    let ordinal = ordinals.entry(region).or_default();
                    let id = identity(bases.doors, region, *ordinal)?;
                    *ordinal += 1;
                    definitions.get_mut(&region).expect("member").doors.push(
                        crate::scenario_package::Door {
                            id,
                            at,
                            open: false,
                            height: 2,
                            concealment: Some(crate::scenario_package::Concealment {
                                dc: config.dc,
                            }),
                        },
                    );
                    // A closure's exterior face receives the adjacent room's
                    // authored light; it does not illuminate hidden corridor cells.
                    if !definitions[&region].lit
                        && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(dx, dy)| {
                            illuminated.contains(&(candidate.0 + dx, candidate.1 + dy))
                        })
                    {
                        let definition = definitions.get_mut(&region).expect("member");
                        for z in 0..2 {
                            let surface = [at[0], at[1], z];
                            if !definition.lighting.iter().any(|light| light.at == surface) {
                                definition
                                    .lighting
                                    .push(crate::scenario_package::CellLight {
                                        at: surface,
                                        lit: true,
                                    });
                            }
                        }
                    }
                    placed += 1;
                }
            }
            Operation::HiddenTraps { config } => {
                // Room interiors are disjoint from entrance mouths, so adding or
                // retuning secret entrances cannot reroll trap placement.
                let mut candidates: Vec<_> = rooms
                    .iter()
                    .enumerate()
                    .flat_map(|(slot, room)| {
                        let offset = (
                            (slot % columns) as i32 * size[0],
                            (slot / columns) as i32 * size[1],
                        );
                        room.iter().map(move |&(x, y)| (x + offset.0, y + offset.1))
                    })
                    .filter(|at| !reserved.contains(at))
                    .collect();
                candidates.sort();
                candidates.dedup();
                let mut rng = stream(
                    run.seed,
                    run.identity,
                    run.group,
                    run.recipe,
                    stage,
                    "placement",
                )?;
                let maximum = config.count[1].min(candidates.len() as u32);
                if config.count[0] > maximum {
                    return Err(fail(
                        "Hidden trap minimum exceeds room-interior placement capacity",
                    ));
                }
                let count = rng.range(config.count[0] as i32, maximum as i32) as usize;
                for i in (1..candidates.len()).rev() {
                    let j = rng.range(0, i as i32) as usize;
                    candidates.swap(i, j);
                }
                let mut ordinals = BTreeMap::<u64, usize>::new();
                for candidate in candidates.into_iter().take(count) {
                    let (region, at) = to_local(candidate);
                    let ordinal = ordinals.entry(region).or_default();
                    let id = identity(bases.traps, region, *ordinal)?;
                    *ordinal += 1;
                    let mut family = stream(
                        run.seed,
                        run.identity,
                        run.group,
                        run.recipe,
                        stage,
                        &format!("family/{region}/{}/{}", at[0], at[1]),
                    )?;
                    let kind = if family.next() & 1 == 0 {
                        crate::scenario_package::TrapKind::Rust {
                            slot: config.rust_slot.clone(),
                            amount: config.rust_amount,
                        }
                    } else {
                        crate::scenario_package::TrapKind::SleepingGas {
                            condition: config.sleep_condition.clone(),
                            duration: config.sleep_duration,
                        }
                    };
                    definitions.get_mut(&region).expect("member").traps.push(
                        crate::scenario_package::Trap {
                            id,
                            at,
                            dc: config.dc,
                            kind,
                        },
                    );
                }
            }
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn feature_namespaces_are_checked_and_disjoint_per_region() {
        assert_eq!(identity(50, 1, 0).unwrap(), 50);
        assert_eq!(identity(50, 2, 0).unwrap(), 1074);
        assert!(identity(1, 0, 0).is_err());
        assert!(identity(1, 1, 1024).is_err());
        assert!(identity(1, u64::MAX, 0).is_err());
    }
}
