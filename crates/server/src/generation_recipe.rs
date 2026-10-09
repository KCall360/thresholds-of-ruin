//! Deterministic recipe composition over a bounded group of region slots.
//! Preparation owns temporary geometry only; published terrain is represented
//! by the ordinary simulation region records.
use crate::{scenario_package::RegionDef, Failure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationGroup {
    pub depth: u32,
    pub recipe: String,
    /// Nine region IDs in row-major slot order.
    pub members: [u64; 9],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub stages: Vec<Stage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub id: String,
    pub version: u32,
    pub operation: Operation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    StoneFill,
    GridPartition,
    Rooms {
        width: [u32; 2],
        height: [u32; 2],
    },
    ConnectedGraph {
        extra: [u32; 2],
    },
    Corridors,
    Stairs,
    RoomLighting {
        darkness_roll: u32,
        darkness_start: u32,
    },
}

fn fail(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}

pub(crate) fn check(recipe: &Recipe) -> Result<(), Failure> {
    if !((recipe.version == 1 && recipe.stages.len() == 6)
        || (recipe.version == 2 && recipe.stages.len() == 7))
    {
        return Err(fail(
            "Recipe requires six ordered stages in version 1, seven in version 2",
        ));
    }
    let mut names = BTreeSet::new();
    for (position, stage) in recipe.stages.iter().enumerate() {
        if stage.version != 1
            || stage.id.is_empty()
            || stage.id.len() > 80
            || !stage
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || !names.insert(&stage.id)
        {
            return Err(fail("Invalid or duplicate recipe stage identity/version"));
        }
        let valid = match (&stage.operation, position) {
            (Operation::StoneFill, 0)
            | (Operation::GridPartition, 1)
            | (Operation::Corridors, 4)
            | (Operation::Stairs, 5) => true,
            (Operation::RoomLighting { darkness_roll, .. }, 6) => {
                (1..=10_000).contains(darkness_roll)
            }
            (Operation::Rooms { width, height }, 2) => {
                width[0] >= 3
                    && width[0] <= width[1]
                    && width[1] <= 30
                    && height[0] >= 3
                    && height[0] <= height[1]
                    && height[1] <= 30
            }
            (Operation::ConnectedGraph { extra }, 3) => extra[0] <= extra[1] && extra[1] <= 4,
            _ => false,
        };
        if !valid {
            return Err(fail(format!(
                "Stage {}: invalid operation, parameters or order",
                stage.id
            )));
        }
    }
    Ok(())
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut n = self.0;
        n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
        n ^ (n >> 31)
    }
    fn range(&mut self, low: i32, high: i32) -> i32 {
        low + (self.next() % (high - low + 1) as u64) as i32
    }
}

fn stream(
    seed: u64,
    identity: &str,
    group: &GenerationGroup,
    _recipe: &Recipe,
    stage: &Stage,
    key: &str,
) -> Result<Rng, Failure> {
    let bytes = serde_json::to_vec(&(
        "tor-group-recipe-v1",
        seed,
        identity,
        group.depth,
        // Geometry stages retain their version-1 stream when lighting is appended.
        1u32,
        stage,
        key,
    ))
    .map_err(|e| fail(e.to_string()))?;
    let hash = Sha256::digest(bytes);
    Ok(Rng(u64::from_le_bytes(
        hash[..8].try_into().expect("digest length"),
    )))
}

struct RecipeRun<'a> {
    identity: &'a str,
    group: &'a GenerationGroup,
    recipe: &'a Recipe,
    seed: u64,
}

struct Room {
    cells: Vec<(i32, i32)>,
    center: (i32, i32),
}

impl RecipeRun<'_> {
    fn rng(&self, stage: usize, key: &str) -> Result<Rng, Failure> {
        stream(
            self.seed,
            self.identity,
            self.group,
            self.recipe,
            &self.recipe.stages[stage],
            key,
        )
    }

    fn room(&self, def: &RegionDef) -> Result<Room, Failure> {
        let Operation::Rooms { width, height } = self.recipe.stages[2].operation else {
            unreachable!()
        };
        let size = def.size;
        if size[0] < 5
            || size[1] < 5
            || size[2] != 2
            || size[0] > 32
            || size[1] > 32
            || width[1] as i32 > size[0] - 2
            || height[1] as i32 > size[1] - 2
        {
            return Err(fail(
                "Room ranges exceed bounded slots with two-cell clearance",
            ));
        }
        let mut rng = self.rng(2, &format!("{}/{size:?}", def.id))?;
        let w = rng.range(width[0] as i32, width[1] as i32);
        let h = rng.range(height[0] as i32, height[1] as i32);
        let boundary = def.generate.as_ref().map(|g| &g.boundary_anchors);
        let mut x_low = 1;
        let mut x_high = size[0] - w - 1;
        let mut y_low = 1;
        let mut y_high = size[1] - h - 1;
        for (_, at) in def
            .anchors
            .iter()
            .filter(|(name, _)| !boundary.is_some_and(|b| b.contains(name)))
        {
            if at[2] != 0
                || !(1..size[0] - 1).contains(&at[0])
                || !(1..size[1] - 1).contains(&at[1])
            {
                return Err(fail(format!(
                    "Region {}: Room entries must lie inside the slot margin on the floor plane",
                    def.id
                )));
            }
            x_low = x_low.max(at[0] - w + 1);
            x_high = x_high.min(at[0]);
            y_low = y_low.max(at[1] - h + 1);
            y_high = y_high.min(at[1]);
        }
        if x_low > x_high || y_low > y_high {
            return Err(fail(
                "Room cannot contain authored entries within its margin",
            ));
        }
        let x = rng.range(x_low, x_high);
        let y = rng.range(y_low, y_high);
        Ok(Room {
            cells: (y..y + h)
                .flat_map(|y| (x..x + w).map(move |x| (x, y)))
                .collect(),
            center: (x + w / 2, y + h / 2),
        })
    }

    fn stairs(&self, def: &mut RegionDef, cells: &[(i32, i32)]) -> Result<(), Failure> {
        let names = def
            .generate
            .as_ref()
            .map_or_else(Vec::new, |g| g.stair_anchors.clone());
        let mut occupied: BTreeSet<_> = def.anchors.values().copied().collect();
        for name in names {
            let candidates: Vec<_> = cells
                .iter()
                .map(|&(x, y)| [x, y, 0])
                .filter(|at| !occupied.contains(at))
                .collect();
            if candidates.is_empty() {
                return Err(fail("Insufficient room space for stair endpoints"));
            }
            let mut rng = self.rng(5, &format!("{}/{name}", def.id))?;
            let at = candidates[rng.range(0, candidates.len() as i32 - 1) as usize];
            occupied.insert(at);
            def.anchors.insert(name, at);
        }
        Ok(())
    }
}

fn validate_carving(open: &BTreeSet<(i32, i32)>, size: [i32; 3]) -> Result<(), Failure> {
    let start = open
        .first()
        .copied()
        .ok_or_else(|| fail("Floor has no carved cells"))?;
    if size[2] < 2
        || open
            .iter()
            .any(|&(x, y)| x <= 0 || y <= 0 || x >= size[0] - 1 || y >= size[1] - 1)
    {
        return Err(fail(
            "Carved floor must preserve its exterior and body clearance",
        ));
    }
    let mut visited = BTreeSet::from([start]);
    let mut pending = vec![start];
    while let Some((x, y)) = pending.pop() {
        for next in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if open.contains(&next) && visited.insert(next) {
                pending.push(next);
            }
        }
    }
    if visited.len() != open.len() {
        return Err(fail("Carved floor is disconnected"));
    }
    Ok(())
}

/// Prepare region definitions from a recipe without publishing simulation state.
pub fn materialize(
    identity: &str,
    group: &GenerationGroup,
    recipe: &Recipe,
    definitions: Vec<RegionDef>,
    seed: u64,
) -> Result<BTreeMap<u64, RegionDef>, Failure> {
    check(recipe)?;
    if definitions.len() != 9 || group.members.iter().collect::<BTreeSet<_>>().len() != 9 {
        return Err(fail(
            "Generation group requires nine unique member definitions",
        ));
    }
    let mut definitions: BTreeMap<_, _> = definitions.into_iter().map(|d| (d.id, d)).collect();
    if definitions.len() != 9 || group.members.iter().any(|id| !definitions.contains_key(id)) {
        return Err(fail(
            "Generation group must contain exactly nine distinct regions",
        ));
    }
    let size = definitions[&group.members[0]].size;
    if size[0] < 5 || size[1] < 5 || size[2] < 2 || definitions.values().any(|d| d.size != size) {
        return Err(fail(
            "Grid members need equal bounds and two cells of clearance",
        ));
    }
    let run = RecipeRun {
        identity,
        group,
        recipe,
        seed,
    };
    let mut open = BTreeSet::new();
    let mut centers = Vec::new();
    let mut room_cells = Vec::new();
    for (slot, id) in group.members.iter().enumerate() {
        let def = &definitions[id];
        let room = run.room(def)?;
        let offset = ((slot % 3) as i32 * size[0], (slot / 3) as i32 * size[1]);
        let cells: Vec<_> = room
            .cells
            .iter()
            .map(|&(x, y)| (x + offset.0, y + offset.1))
            .collect();
        open.extend(cells.iter().copied());
        room_cells.push(room.cells);
        centers.push((room.center.0 + offset.0, room.center.1 + offset.1));
    }
    let mut edges = Vec::new();
    for slot in 0..9 {
        if slot % 3 < 2 {
            edges.push((slot, slot + 1));
        }
        if slot / 3 < 2 {
            edges.push((slot, slot + 3));
        }
    }
    let mut rng = stream(seed, identity, group, recipe, &recipe.stages[3], "graph")?;
    for i in (1..edges.len()).rev() {
        let j = rng.range(0, i as i32) as usize;
        edges.swap(i, j);
    }
    let mut labels: Vec<_> = (0..9).collect();
    let mut connected = Vec::new();
    let mut unused = Vec::new();
    for (a, b) in edges {
        if labels[a] == labels[b] {
            unused.push((a, b));
            continue;
        }
        let old = labels[b];
        let new = labels[a];
        for label in &mut labels {
            if *label == old {
                *label = new;
            }
        }
        connected.push((a, b));
    }
    let Operation::ConnectedGraph { extra } = recipe.stages[3].operation else {
        unreachable!()
    };
    connected.extend(
        unused
            .into_iter()
            .take(rng.range(extra[0] as i32, extra[1] as i32) as usize),
    );
    let mut rng = stream(
        seed,
        identity,
        group,
        recipe,
        &recipe.stages[4],
        "corridors",
    )?;
    for (a, b) in connected {
        let (mut x, mut y) = centers[a];
        let (tx, ty) = centers[b];
        let horizontal_first = rng.next() & 1 == 0;
        open.insert((x, y));
        for horizontal in [horizontal_first, !horizontal_first] {
            while if horizontal { x != tx } else { y != ty } {
                if horizontal {
                    x += (tx - x).signum();
                } else {
                    y += (ty - y).signum();
                }
                open.insert((x, y));
            }
        }
    }
    validate_carving(&open, [size[0] * 3, size[1] * 3, size[2]])?;
    for (slot, id) in group.members.iter().enumerate() {
        let def = definitions.get_mut(id).expect("checked member");
        let offset = ((slot % 3) as i32 * size[0], (slot / 3) as i32 * size[1]);
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    if z >= 2 || !open.contains(&(x + offset.0, y + offset.1)) {
                        def.walls.push([x, y, z]);
                    }
                }
            }
        }
        run.stairs(def, &room_cells[slot])?;
        if let Some(stage) = recipe.stages.get(6) {
            let mut rng = stream(seed, identity, group, recipe, stage, &format!("room/{id}"))?;
            def.lit = false;
            def.lighting.clear();
            let Operation::RoomLighting {
                darkness_roll,
                darkness_start,
            } = stage.operation
            else {
                unreachable!()
            };
            if (rng.range(0, darkness_roll as i32 - 1) as u32)
                >= group.depth.saturating_sub(darkness_start)
            {
                let mut illuminated = BTreeSet::new();
                for &(x, y) in &room_cells[slot] {
                    for z in 0..2 {
                        illuminated.insert([x, y, z]);
                        for [dx, dy, dz] in [
                            [-1, 0, 0],
                            [1, 0, 0],
                            [0, -1, 0],
                            [0, 1, 0],
                            [0, 0, -1],
                            [0, 0, 1],
                        ] {
                            let at = [x + dx, y + dy, z + dz];
                            if at[2] < 0
                                || at[2] >= 2
                                || !open.contains(&(at[0] + offset.0, at[1] + offset.1))
                            {
                                illuminated.insert(at);
                            }
                        }
                    }
                }
                def.lighting.extend(
                    illuminated
                        .into_iter()
                        .map(|at| crate::scenario_package::CellLight { at, lit: true }),
                );
            }
        }
        let walls: BTreeSet<_> = def.walls.iter().copied().collect();
        for name in def.anchors.keys().filter(|name| {
            !def.generate
                .as_ref()
                .is_some_and(|g| g.boundary_anchors.contains(name))
        }) {
            let [x, y, z] = def.anchors[name];
            if z != 0 || walls.contains(&[x, y, 0]) || walls.contains(&[x, y, 1]) {
                return Err(fail(format!(
                    "Region {}: anchor {name} lacks body clearance",
                    def.id
                )));
            }
        }
    }
    Ok(definitions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extreme_authored_entries_return_validation_errors_without_overflow() {
        let (group, recipe, defs) = fixture();
        for at in [
            [i32::MIN, 3, 0],
            [13, i32::MIN, 0],
            [i32::MAX, 3, 0],
            [0, 3, 0],
        ] {
            let mut defs = defs.clone();
            defs[0].anchors.insert("start".into(), at);
            assert!(materialize("floor-1", &group, &recipe, defs, 42).is_err());
        }
    }

    #[test]
    fn carved_floor_rejects_disconnection_and_open_exterior() {
        let connected = BTreeSet::from([(1, 1), (2, 1), (2, 2)]);
        assert!(validate_carving(&connected, [5, 5, 2]).is_ok());
        let disconnected = BTreeSet::from([(1, 1), (3, 3)]);
        assert!(validate_carving(&disconnected, [5, 5, 2]).is_err());
        let exterior = BTreeSet::from([(0, 1), (1, 1)]);
        assert!(validate_carving(&exterior, [5, 5, 2]).is_err());
        assert!(validate_carving(&BTreeSet::new(), [5, 5, 2]).is_err());
    }

    fn fixture() -> (GenerationGroup, Recipe, Vec<RegionDef>) {
        let group = GenerationGroup {
            depth: 1,
            recipe: "exploration".into(),
            members: [1, 2, 3, 4, 5, 6, 7, 8, 9],
        };
        let recipe = Recipe {
            version: 1,
            stages: vec![
                Operation::StoneFill,
                Operation::GridPartition,
                Operation::Rooms {
                    width: [4, 24],
                    height: [3, 5],
                },
                Operation::ConnectedGraph { extra: [0, 2] },
                Operation::Corridors,
                Operation::Stairs,
            ]
            .into_iter()
            .enumerate()
            .map(|(i, operation)| Stage {
                id: format!("stage-{i}"),
                version: 1,
                operation,
            })
            .collect(),
        };
        let defs = (1..=9)
            .map(|id| {
                toml::from_str(&format!(
                    "id={id}\nname='slot'\nsize=[26,7,2]\nchamber=true\n"
                ))
                .unwrap()
            })
            .collect();
        (group, recipe, defs)
    }

    #[test]
    fn recipe_generates_nine_connected_rooms_deterministically() {
        let (group, recipe, defs) = fixture();
        for seed in [0, 1, 42, u64::MAX] {
            let first = materialize("floor-1", &group, &recipe, defs.clone(), seed).unwrap();
            let second = materialize("floor-1", &group, &recipe, defs.clone(), seed).unwrap();
            assert_eq!(
                serde_json::to_vec(&first).unwrap(),
                serde_json::to_vec(&second).unwrap()
            );
            assert_eq!(first.len(), 9);
            let open: BTreeSet<_> = first
                .values()
                .flat_map(|r| {
                    let walls: BTreeSet<_> = r.walls.iter().copied().collect();
                    let slot = group.members.iter().position(|id| *id == r.id).unwrap();
                    (0..7).flat_map(move |y| {
                        (0..26).filter_map({
                            let walls = walls.clone();
                            move |x| {
                                (!walls.contains(&[x, y, 0])).then_some((
                                    x + (slot % 3) as i32 * 26,
                                    y + (slot / 3) as i32 * 7,
                                ))
                            }
                        })
                    })
                })
                .collect();
            let start = *open.first().unwrap();
            let mut seen = BTreeSet::from([start]);
            let mut pending = vec![start];
            while let Some((x, y)) = pending.pop() {
                for next in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                    if open.contains(&next) && seen.insert(next) {
                        pending.push(next);
                    }
                }
            }
            assert_eq!(seen, open);
        }
    }

    #[test]
    fn lighting_is_depth_dependent_deterministic_and_geometry_independent() {
        let (mut group, mut recipe, defs) = fixture();
        let baseline = materialize("floor", &group, &recipe, defs.clone(), 42).unwrap();
        recipe.version = 2;
        recipe.stages.push(Stage {
            id: "lighting".into(),
            version: 1,
            operation: Operation::RoomLighting {
                darkness_roll: 10,
                darkness_start: 1,
            },
        });
        let first = materialize("floor", &group, &recipe, defs.clone(), 42).unwrap();
        let again = materialize("floor", &group, &recipe, defs.clone(), 42).unwrap();
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&again).unwrap()
        );
        for id in group.members {
            assert_eq!(first[&id].walls, baseline[&id].walls);
            assert_eq!(first[&id].anchors, baseline[&id].anchors);
            assert!(!first[&id].lit);
            assert!(!first[&id].lighting.is_empty());
        }
        group.depth = 11;
        assert!(materialize("floor", &group, &recipe, defs.clone(), 42)
            .unwrap()
            .values()
            .all(|d| d.lighting.is_empty()));
        group.depth = u32::MAX;
        assert!(materialize("floor", &group, &recipe, defs, 42)
            .unwrap()
            .values()
            .all(|d| d.lighting.is_empty()));
    }

    #[test]
    fn stair_positions_are_isolated_from_graph_and_corridor_streams() {
        let (group, mut recipe, mut defs) = fixture();
        defs[0].generate = Some(toml::from_str("generator='group'\nversion=1\ngroup='floor-1'\nrooms=[1,1]\nstair_anchors=['up','down']\n").unwrap());
        let first = materialize("floor-1", &group, &recipe, defs.clone(), 42).unwrap();
        recipe.stages[3].id = "different-graph".into();
        recipe.stages[4].id = "different-corridors".into();
        let changed = materialize("floor-1", &group, &recipe, defs, 42).unwrap();
        assert_eq!(first[&1].anchors, changed[&1].anchors);
        assert_ne!(first[&1].anchors["up"], first[&1].anchors["down"]);
    }

    #[test]
    fn room_rolls_light_whole_volumes_and_surfaces_but_leave_corridors_dark() {
        let (mut group, mut recipe, defs) = fixture();
        group.depth = 6;
        recipe.version = 2;
        recipe.stages.push(Stage {
            id: "lighting".into(),
            version: 1,
            operation: Operation::RoomLighting {
                darkness_roll: 10,
                darkness_start: 1,
            },
        });
        let mut lit_rooms = 0;
        let mut dark_rooms = 0;
        let mut dark_corridor_cells = 0;
        for seed in 0..20 {
            let run = RecipeRun {
                identity: "floor",
                group: &group,
                recipe: &recipe,
                seed,
            };
            let generated = materialize("floor", &group, &recipe, defs.clone(), seed).unwrap();
            for def in &defs {
                let room = run.room(def).unwrap();
                let lit = run.rng(6, &format!("room/{}", def.id)).unwrap().range(0, 9) >= 5;
                if lit {
                    lit_rooms += 1;
                } else {
                    dark_rooms += 1;
                }
                let generated = &generated[&def.id];
                assert!(!generated.lit);
                let light: BTreeSet<_> = generated
                    .lighting
                    .iter()
                    .map(|c| {
                        assert!(c.lit);
                        c.at
                    })
                    .collect();
                let walls: BTreeSet<_> = generated.walls.iter().copied().collect();
                for &(x, y) in &room.cells {
                    for z in 0..2 {
                        assert_eq!(light.contains(&[x, y, z]), lit);
                    }
                    assert_eq!(light.contains(&[x, y, -1]), lit);
                    assert_eq!(light.contains(&[x, y, 2]), lit);
                }
                for y in 0..def.size[1] {
                    for x in 0..def.size[0] {
                        if !walls.contains(&[x, y, 0]) && !room.cells.contains(&(x, y)) {
                            assert!(!light.contains(&[x, y, 0]));
                            dark_corridor_cells += 1;
                        }
                    }
                }
            }
        }
        assert!(
            lit_rooms > 60 && dark_rooms > 60,
            "independent 50% rolls across 180 rooms"
        );
        assert!(dark_corridor_cells > 0);
    }

    #[test]
    fn malformed_recipes_and_duplicate_members_reject_before_generation() {
        let (mut group, mut recipe, defs) = fixture();
        recipe.stages[1].id = recipe.stages[0].id.clone();
        assert!(materialize("floor-1", &group, &recipe, defs.clone(), 42).is_err());
        recipe.stages[1].id = "slots".into();
        group.members[8] = group.members[0];
        assert!(materialize("floor-1", &group, &recipe, defs, 42).is_err());
    }
}
