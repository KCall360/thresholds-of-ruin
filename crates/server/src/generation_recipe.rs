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
    /// Columns and rows of equally sized region slots.
    #[serde(default = "default_grid")]
    pub grid: [u32; 2],
    /// Region IDs in row-major slot order.
    pub members: Vec<u64>,
}

pub(crate) const MAX_GRID_SIDE: u32 = 8;
pub const GRID_ROOMS_VERSION: u32 = 1;

fn default_grid() -> [u32; 2] {
    [3, 3]
}

/// Authored parameters for the bounded grid-room generator. Stage order and
/// stream identities are implementation details of this versioned algorithm.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridRoomsPreset {
    pub version: u32,
    pub room_width: [u32; 2],
    pub room_height: [u32; 2],
    pub extra_connections: [u32; 2],
    pub room_omissions: Option<RoomOmissions>,
    pub lighting: Option<RoomLighting>,
    pub secret_entrances: Option<SecretEntrances>,
    pub hidden_traps: Option<HiddenTraps>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomOmissions {
    pub count: [u32; 2],
}

impl<'de> Deserialize<'de> for RoomOmissions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            count: [u32; 2],
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self {
            count: fields.count,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomLighting {
    pub darkness_roll: u32,
    pub darkness_start: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretEntrances {
    pub chance_per_mille: u32,
    pub maximum: u32,
    pub dc: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiddenTraps {
    pub count: [u32; 2],
    pub dc: i32,
    pub rust_slot: String,
    pub rust_amount: u32,
    pub sleep_condition: String,
    pub sleep_duration: u64,
    #[serde(default)]
    pub sleep_exposure: Option<String>,
    #[serde(default)]
    pub hazards: Option<HazardTraps>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HazardTraps {
    pub bear_condition: String,
    pub bear_duration: u64,
    pub arrow_ammunition: String,
    pub attack_bonus: i32,
    pub poison_exposure: String,
    pub avoidance_dc: i32,
}
impl HazardTraps {
    fn valid(&self) -> bool {
        tor_world::Concealment {
            dc: self.avoidance_dc,
        }
        .valid()
            && tor_world::TrapKind::BearTrap {
                condition: self.bear_condition.clone(),
                duration: self.bear_duration,
            }
            .valid()
            && tor_world::TrapKind::Arrow {
                attack_bonus: self.attack_bonus,
                ammunition: self.arrow_ammunition.clone(),
            }
            .valid()
            && tor_world::TrapKind::PoisonDart {
                attack_bonus: self.attack_bonus,
                exposure: self.poison_exposure.clone(),
            }
            .valid()
    }
}

impl GridRoomsPreset {
    /// Lower authoring parameters into a checked, deterministic execution plan.
    pub fn to_recipe(&self) -> Result<Recipe, Failure> {
        if self.version != GRID_ROOMS_VERSION {
            return Err(fail("Unsupported grid_rooms preset algorithm version"));
        }
        let mut stages = vec![
            ("stone", Operation::StoneFill),
            ("slots", Operation::GridPartition),
            (
                "rooms",
                Operation::Rooms {
                    width: self.room_width,
                    height: self.room_height,
                },
            ),
            (
                "connections",
                Operation::ConnectedGraph {
                    extra: self.extra_connections,
                },
            ),
            ("corridors", Operation::Corridors),
            ("stairs", Operation::Stairs),
        ];
        if let Some(config) = &self.room_omissions {
            stages.push((
                "room_omissions",
                Operation::RoomOmissions {
                    config: config.clone(),
                },
            ));
        }
        if let Some(lighting) = &self.lighting {
            stages.push((
                "lighting",
                Operation::RoomLighting {
                    darkness_roll: lighting.darkness_roll,
                    darkness_start: lighting.darkness_start,
                },
            ));
        }
        if let Some(config) = &self.secret_entrances {
            stages.push((
                "secret_entrances",
                Operation::SecretEntrances {
                    config: config.clone(),
                },
            ));
        }
        if let Some(config) = &self.hidden_traps {
            stages.push((
                "hidden_traps",
                Operation::HiddenTraps {
                    config: config.clone(),
                },
            ));
        }
        let recipe = Recipe {
            version: if self.room_omissions.is_some() {
                4
            } else if self.secret_entrances.is_some() || self.hidden_traps.is_some() {
                3
            } else if self.lighting.is_some() {
                2
            } else {
                1
            },
            stages: stages
                .into_iter()
                .map(|(id, operation)| Stage {
                    id: id.into(),
                    version: 1,
                    operation,
                })
                .collect(),
        };
        check(&recipe)?;
        Ok(recipe)
    }
}

/// Compiled execution plan; source authors use the grid_rooms preset.
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
    RoomOmissions {
        config: RoomOmissions,
    },
    RoomLighting {
        darkness_roll: u32,
        darkness_start: u32,
    },
    SecretEntrances {
        config: SecretEntrances,
    },
    HiddenTraps {
        config: HiddenTraps,
    },
}

fn fail(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}

pub(crate) fn check_group(group: &GenerationGroup) -> Result<(), Failure> {
    let [columns, rows] = group.grid;
    if !(1..=MAX_GRID_SIDE).contains(&columns) || !(1..=MAX_GRID_SIDE).contains(&rows) {
        return Err(fail(
            "Generation grid dimensions must each be between 1 and 8",
        ));
    }
    let count = columns
        .checked_mul(rows)
        .ok_or_else(|| fail("Generation grid member count overflows"))? as usize;
    if group.members.len() != count || group.members.iter().collect::<BTreeSet<_>>().len() != count
    {
        return Err(fail(format!(
            "Generation grid requires exactly {count} distinct region members"
        )));
    }
    Ok(())
}

pub(crate) fn check(recipe: &Recipe) -> Result<(), Failure> {
    if !((recipe.version == 1 && recipe.stages.len() == 6)
        || (recipe.version == 2 && recipe.stages.len() == 7)
        || (recipe.version == 3 && (7..=9).contains(&recipe.stages.len()))
        || (recipe.version == 4
            && (7..=10).contains(&recipe.stages.len())
            && matches!(recipe.stages[6].operation, Operation::RoomOmissions { .. })))
    {
        return Err(fail(
            "Recipe requires ordered geometry, optional omissions, lighting, secret entrances and traps",
        ));
    }
    let mut names = BTreeSet::new();
    let mut extra_order = 0;
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
            (Operation::RoomOmissions { config }, 6) if recipe.version == 4 => {
                config.count[0] <= config.count[1]
                    && config.count[1] < MAX_GRID_SIDE * MAX_GRID_SIDE
            }
            (Operation::RoomLighting { darkness_roll, .. }, n)
                if n == if recipe.version == 4 { 7 } else { 6 } =>
            {
                extra_order = 1;
                (1..=10_000).contains(darkness_roll)
            }
            (Operation::SecretEntrances { config }, n)
                if n >= 6 && (3..=4).contains(&recipe.version) && extra_order < 2 =>
            {
                extra_order = 2;
                config.chance_per_mille <= 1000
                    && config.maximum <= 64
                    && tor_world::Concealment { dc: config.dc }.valid()
            }
            (Operation::HiddenTraps { config }, n)
                if n >= 6 && (3..=4).contains(&recipe.version) && extra_order < 3 =>
            {
                extra_order = 3;
                config.count[0] <= config.count[1]
                    && config.count[1] <= 64
                    && config.hazards.as_ref().is_none_or(HazardTraps::valid)
                    && tor_world::Concealment { dc: config.dc }.valid()
                    && tor_world::TrapKind::Rust {
                        slot: config.rust_slot.clone(),
                        amount: config.rust_amount,
                    }
                    .valid()
                    && tor_world::TrapKind::SleepingGas {
                        condition: config.sleep_condition.clone(),
                        duration: config.sleep_duration,
                        exposure: config.sleep_exposure.clone(),
                    }
                    .valid()
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

/// Validate recipe parameters against one equal-sized slot without carving.
/// Procedural families use this while checking their compact descriptors.
pub(crate) fn check_layout(recipe: &Recipe, size: [i32; 3]) -> Result<(), Failure> {
    check(recipe)?;
    if !(5..=32).contains(&size[0]) || !(5..=32).contains(&size[1]) || size[2] != 2 {
        return Err(fail(
            "Grid slots need bounds of 5 to 32 cells and two cells of clearance",
        ));
    }
    let Operation::Rooms { width, height } = recipe.stages[2].operation else {
        unreachable!("checked recipe room stage")
    };
    if width[1] > (size[0] - 2) as u32 || height[1] > (size[1] - 2) as u32 {
        return Err(fail(
            "Room ranges exceed bounded slots with two-cell clearance",
        ));
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

    fn present_rooms(&self, definitions: &BTreeMap<u64, RegionDef>) -> Result<Vec<bool>, Failure> {
        let mut present = vec![true; self.group.members.len()];
        let Some((index, config)) =
            self.recipe
                .stages
                .iter()
                .enumerate()
                .find_map(|(index, stage)| {
                    if let Operation::RoomOmissions { config } = &stage.operation {
                        Some((index, config))
                    } else {
                        None
                    }
                })
        else {
            return Ok(present);
        };
        let mut eligible: Vec<_> = self
            .group
            .members
            .iter()
            .enumerate()
            .filter_map(|(slot, id)| {
                let def = &definitions[id];
                let protected = def
                    .generate
                    .as_ref()
                    .is_some_and(|g| !g.stair_anchors.is_empty())
                    || def.anchors.keys().any(|name| {
                        !def.generate
                            .as_ref()
                            .is_some_and(|g| g.boundary_anchors.contains(name))
                    });
                (!protected).then_some(slot)
            })
            .collect();
        // Keep one real room even when no authored endpoint protects a slot.
        let capacity = eligible.len().min(present.len() - 1) as u32;
        if config.count[0] > capacity {
            return Err(fail(
                "Room omission minimum exceeds unprotected slot capacity",
            ));
        }
        let mut rng = self.rng(index, "selection")?;
        let count =
            rng.range(config.count[0] as i32, config.count[1].min(capacity) as i32) as usize;
        // Bounded partial shuffle, without replacement or rejection sampling.
        for i in 0..count {
            let j = rng.range(i as i32, eligible.len() as i32 - 1) as usize;
            eligible.swap(i, j);
            present[eligible[i]] = false;
        }
        Ok(present)
    }

    fn room(&self, def: &RegionDef) -> Result<Room, Failure> {
        let Operation::Rooms { width, height } = self.recipe.stages[2].operation else {
            unreachable!()
        };
        let size = def.size;
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
    if size[0] < 3
        || size[1] < 3
        || size[2] < 2
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
/// Geometry and stable room interiors are produced together, before any gameplay
/// population. The room metadata is retained by prepared activation scopes.
pub struct GeneratedGroup {
    pub regions: BTreeMap<u64, RegionDef>,
    pub rooms: Vec<tor_simulation::floor_activation::PreparedRoomGroup>,
}

#[cfg(test)]
pub fn materialize(
    identity: &str,
    group: &GenerationGroup,
    recipe: &Recipe,
    definitions: Vec<RegionDef>,
    seed: u64,
) -> Result<GeneratedGroup, Failure> {
    materialize_with_feature_bases(
        identity,
        group,
        recipe,
        definitions,
        seed,
        FeatureBases { doors: 1, traps: 1 },
    )
}

pub fn materialize_with_feature_bases(
    identity: &str,
    group: &GenerationGroup,
    recipe: &Recipe,
    definitions: Vec<RegionDef>,
    seed: u64,
    feature_bases: FeatureBases,
) -> Result<GeneratedGroup, Failure> {
    check(recipe)?;
    check_group(group)?;
    let count = group.members.len();
    if definitions.len() != count {
        return Err(fail(
            "Generation group must have one definition for every member",
        ));
    }
    let mut definitions: BTreeMap<_, _> = definitions.into_iter().map(|d| (d.id, d)).collect();
    if definitions.len() != count || group.members.iter().any(|id| !definitions.contains_key(id)) {
        return Err(fail(
            "Generation group definitions must exactly match its distinct members",
        ));
    }
    let size = definitions[&group.members[0]].size;
    if definitions.values().any(|d| d.size != size) {
        return Err(fail("Grid members need equal bounds"));
    }
    check_layout(recipe, size)?;
    let columns = group.grid[0] as usize;
    let rows = group.grid[1] as usize;
    let width = size[0]
        .checked_mul(group.grid[0] as i32)
        .ok_or_else(|| fail("Generation grid width overflows"))?;
    let height = size[1]
        .checked_mul(group.grid[1] as i32)
        .ok_or_else(|| fail("Generation grid height overflows"))?;
    let run = RecipeRun {
        identity,
        group,
        recipe,
        seed,
    };
    let mut open = BTreeSet::new();
    let present = run.present_rooms(&definitions)?;
    let mut centers = Vec::new();
    let mut room_cells = Vec::new();
    for (slot, id) in group.members.iter().enumerate() {
        let def = &definitions[id];
        let room = run.room(def)?;
        let offset = (
            (slot % columns) as i32 * size[0],
            (slot / columns) as i32 * size[1],
        );
        let cells: Vec<_> = room
            .cells
            .iter()
            .map(|&(x, y)| (x + offset.0, y + offset.1))
            .collect();
        if present[slot] {
            open.extend(cells.iter().copied());
            room_cells.push(room.cells);
        } else {
            room_cells.push(Vec::new());
        }
        centers.push((room.center.0 + offset.0, room.center.1 + offset.1));
    }
    let mut edges = Vec::new();
    for slot in 0..count {
        if slot % columns + 1 < columns {
            edges.push((slot, slot + 1));
        }
        if slot / columns + 1 < rows {
            edges.push((slot, slot + columns));
        }
    }
    let mut rng = stream(seed, identity, group, recipe, &recipe.stages[3], "graph")?;
    for i in (1..edges.len()).rev() {
        let j = rng.range(0, i as i32) as usize;
        edges.swap(i, j);
    }
    let mut labels: Vec<_> = (0..count).collect();
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
    let mut corridor_cells = BTreeSet::new();
    for (a, b) in connected {
        let (mut x, mut y) = centers[a];
        let (tx, ty) = centers[b];
        let horizontal_first = rng.next() & 1 == 0;
        corridor_cells.insert((x, y));
        for horizontal in [horizontal_first, !horizontal_first] {
            while if horizontal { x != tx } else { y != ty } {
                if horizontal {
                    x += (tx - x).signum();
                } else {
                    y += (ty - y).signum();
                }
                corridor_cells.insert((x, y));
            }
        }
    }
    open.extend(corridor_cells);
    validate_carving(&open, [width, height, size[2]])?;
    for (slot, id) in group.members.iter().enumerate() {
        let def = definitions.get_mut(id).expect("checked member");
        let offset = (
            (slot % columns) as i32 * size[0],
            (slot / columns) as i32 * size[1],
        );
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
        if let Some(stage) = recipe
            .stages
            .iter()
            .find(|stage| matches!(stage.operation, Operation::RoomLighting { .. }))
        {
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
    place_concealed_features(
        &run,
        &open,
        &room_cells,
        &mut definitions,
        size,
        feature_bases,
    )?;
    // Opening every discoverable closure restores exactly this connected carving;
    // features never erase a corridor, stair or room from the required route graph.
    validate_carving(&open, [width, height, size[2]])?;
    let rooms = group
        .members
        .iter()
        .enumerate()
        .filter(|(slot, _)| present[*slot])
        .map(|(slot, id)| {
            let mut cells: Vec<_> = room_cells[slot]
                .iter()
                .map(|&(x, y)| tor_world::Location {
                    region: tor_world::RegionId(*id),
                    position: tor_world::Position { x, y, z: 0 },
                })
                .collect();
            cells.sort();
            tor_simulation::floor_activation::PreparedRoomGroup {
                id: slot as u32 + 1,
                cells,
            }
        })
        .collect();
    Ok(GeneratedGroup {
        regions: definitions,
        rooms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn materialize(
        identity: &str,
        group: &GenerationGroup,
        recipe: &Recipe,
        definitions: Vec<RegionDef>,
        seed: u64,
    ) -> Result<BTreeMap<u64, RegionDef>, Failure> {
        super::materialize(identity, group, recipe, definitions, seed)
            .map(|generated| generated.regions)
    }

    #[test]
    fn prepared_rooms_preserve_slot_ids_and_only_bounded_interior_cells() {
        let (group, recipe, definitions) = fixture();
        let first =
            super::materialize("floor-1", &group, &recipe, definitions.clone(), 42).unwrap();
        let mut reversed = definitions;
        reversed.reverse();
        let second = super::materialize("floor-1", &group, &recipe, reversed, 42).unwrap();
        assert_eq!(first.rooms, second.rooms);
        assert_eq!(first.rooms.len(), group.members.len());
        for (slot, room) in first.rooms.iter().enumerate() {
            assert_eq!(room.id, slot as u32 + 1);
            assert!(!room.cells.is_empty());
            assert!(room.cells.len() <= tor_simulation::floor_activation::MAX_ROOM_CELLS);
            assert!(room.cells.windows(2).all(|pair| pair[0] < pair[1]));
            let region = &first.regions[&group.members[slot]];
            assert!(room.cells.iter().all(|cell| cell.region.0 == region.id
                && cell.position.z == 0
                && !region
                    .walls
                    .contains(&[cell.position.x, cell.position.y, 0])));
        }
    }

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
    fn every_interior_corner_accepts_an_entry_and_distinct_stairs_in_minimum_rooms() {
        use crate::procedural_world::{Domain, FamilyKind, WorldFamily};
        let recipe = GridRoomsPreset {
            version: 1,
            room_width: [3, 3],
            room_height: [3, 3],
            extra_connections: [0, 0],
            room_omissions: None,
            lighting: None,
            secret_entrances: None,
            hidden_traps: None,
        }
        .to_recipe()
        .unwrap();
        for size in [[5, 5, 2], [32, 32, 2]] {
            let family = WorldFamily {
                kind: FamilyKind::GridChain,
                version: 1,
                base_region: 1,
                domain: Domain::Open { max_depth: 2 },
                grid: [1, 1],
                size,
                recipe: "rooms".into(),
                gravity: None,
                zone: None,
            };
            for seed in [0, 1, 42] {
                let group = family.group("grid", seed, 1).unwrap();
                for x in [1, size[0] - 2] {
                    for y in [1, size[1] - 2] {
                        let mut definition = family.describe("grid", seed, 1).unwrap().unwrap();
                        definition.anchors.insert("start".into(), [x, y, 0]);
                        let generated =
                            materialize("grid/1", &group, &recipe, vec![definition], seed).unwrap();
                        let definition = &generated[&1];
                        assert_eq!(definition.anchors["start"], [x, y, 0]);
                        assert!(!definition.walls.contains(&[x, y, 0]));
                        assert!(!definition.walls.contains(&[x, y, 1]));
                        assert_ne!(definition.anchors["up"], [x, y, 0]);
                        assert_ne!(definition.anchors["down"], [x, y, 0]);
                    }
                }
            }
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
            grid: [3, 3],
            members: vec![1, 2, 3, 4, 5, 6, 7, 8, 9],
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
    fn typed_grid_room_presets_preserve_canonical_geometry_streams() {
        let (group, mut legacy, definitions) = fixture();
        for (stage, name) in legacy.stages.iter_mut().zip([
            "stone",
            "slots",
            "rooms",
            "connections",
            "corridors",
            "stairs",
        ]) {
            stage.id = name.into();
        }
        let preset = GridRoomsPreset {
            version: GRID_ROOMS_VERSION,
            room_width: [4, 24],
            room_height: [3, 5],
            extra_connections: [0, 2],
            room_omissions: None,
            lighting: None,
            secret_entrances: None,
            hidden_traps: None,
        };
        let recipe = preset.to_recipe().unwrap();
        assert_eq!(
            serde_json::to_value(&recipe).unwrap(),
            serde_json::to_value(&legacy).unwrap()
        );
        for seed in [0, 42, u64::MAX] {
            assert_eq!(
                serde_json::to_value(
                    materialize("floor", &group, &legacy, definitions.clone(), seed).unwrap()
                )
                .unwrap(),
                serde_json::to_value(
                    materialize("floor", &group, &recipe, definitions.clone(), seed).unwrap()
                )
                .unwrap()
            );
        }
        let lit = GridRoomsPreset {
            lighting: Some(RoomLighting {
                darkness_roll: 10,
                darkness_start: 1,
            }),
            ..preset
        }
        .to_recipe()
        .unwrap();
        let baseline = materialize("floor", &group, &recipe, definitions.clone(), 42).unwrap();
        let lit = materialize("floor", &group, &lit, definitions, 42).unwrap();
        for id in &group.members {
            assert_eq!(lit[id].walls, baseline[id].walls);
            assert_eq!(lit[id].anchors, baseline[id].anchors);
            assert!(!lit[id].lighting.is_empty());
        }
    }

    #[test]
    fn omissions_preserve_seeded_family_ports_and_future_trapdoor_landing_namespaces() {
        use crate::procedural_world::{Domain, FamilyKind, WorldFamily};
        let family = WorldFamily {
            kind: FamilyKind::GridChain,
            version: 1,
            base_region: 1,
            domain: Domain::Finite { count: 2 },
            grid: [3, 3],
            size: [26, 7, 2],
            recipe: "hazards".into(),
            zone: None,
            gravity: Some([0, 0, -1]),
        };
        let ordinary = hazard_recipe();
        let recipe = with_omissions(ordinary.clone(), [3, 3]);
        let mut trapdoors = 0;
        for seed in 0..32 {
            let build = |depth, recipe: &Recipe| {
                let group = family.group("grid", seed, depth).unwrap();
                let definitions = group
                    .members
                    .iter()
                    .map(|id| family.describe("grid", seed, *id).unwrap().unwrap())
                    .collect();
                super::materialize(&format!("grid/{depth}"), &group, recipe, definitions, seed)
                    .unwrap()
            };
            let lower = build(2, &recipe);
            let upper = build(1, &recipe);
            for (depth, generated) in [(1, &upper), (2, &lower)] {
                assert_eq!(
                    generated.rooms.len(),
                    6,
                    "boundary portal anchors must not protect every slot"
                );
                let baseline = build(depth, &ordinary);
                for (id, region) in &generated.regions {
                    let described = family.describe("grid", seed, *id).unwrap().unwrap();
                    for name in &described.generate.as_ref().unwrap().stair_anchors {
                        assert_eq!(region.anchors[name], baseline.regions[id].anchors[name]);
                        assert!(generated
                            .rooms
                            .iter()
                            .any(|room| room.cells.iter().any(|cell| cell.region.0 == *id)));
                    }
                    assert!(region.anchors.contains_key("trapdoor_landing_0"));
                    assert!(region.anchors.contains_key("trapdoor_landing_1"));
                }
            }
            for region in upper.regions.values() {
                for trap in &region.traps {
                    if let crate::scenario_package::TrapKind::TrapDoor { destination, open } =
                        &trap.kind
                    {
                        trapdoors += 1;
                        assert!(!open);
                        assert_eq!(destination.region.0, region.id + family.slots());
                        let target = &lower.regions[&destination.region.0];
                        let at = target.anchors[&destination.name];
                        assert!((0..2).all(|z| !target.walls.contains(&[at[0], at[1], z])));
                        assert!(!target.doors.iter().any(|door| door.at[..2] == at[..2]));
                    }
                }
            }
            assert!(lower
                .regions
                .values()
                .all(|region| region.trapdoor_destination.is_none()));
        }
        assert!(
            trapdoors > 0,
            "exercise an actual source trapdoor, not just unused anchors"
        );
    }

    fn hazard_recipe() -> Recipe {
        GridRoomsPreset {
            version: 1,
            room_width: [4, 6],
            room_height: [3, 4],
            extra_connections: [0, 0],
            room_omissions: None,
            lighting: Some(RoomLighting {
                darkness_roll: 10,
                darkness_start: 1,
            }),
            secret_entrances: Some(SecretEntrances {
                chance_per_mille: 1000,
                maximum: 64,
                dc: 12,
            }),
            hidden_traps: Some(HiddenTraps {
                count: [2, 2],
                dc: 12,
                rust_slot: "body_armor".into(),
                rust_amount: 1,
                sleep_condition: "sleep".into(),
                sleep_duration: 200,
                sleep_exposure: None,
                hazards: Some(HazardTraps {
                    bear_condition: "bear".into(),
                    bear_duration: 300,
                    arrow_ammunition: "arrow".into(),
                    attack_bonus: 0,
                    poison_exposure: "poison".into(),
                    avoidance_dc: 12,
                }),
            }),
        }
        .to_recipe()
        .unwrap()
    }

    #[test]
    fn omitted_slots_keep_clear_distinct_trapdoor_destinations_without_room_features() {
        let (group, _, definitions) = fixture();
        let recipe = with_omissions(hazard_recipe(), [3, 3]);
        for seed in [0, 1, 42, u64::MAX] {
            let generated =
                super::materialize("floor", &group, &recipe, definitions.clone(), seed).unwrap();
            let present: BTreeSet<_> = generated
                .rooms
                .iter()
                .map(|room| u64::from(room.id))
                .collect();
            assert_eq!(present.len(), 6);
            let mut reversed = definitions.clone();
            reversed.reverse();
            let repeated = super::materialize("floor", &group, &recipe, reversed, seed).unwrap();
            assert_eq!(
                serde_json::to_value(&generated.regions).unwrap(),
                serde_json::to_value(repeated.regions).unwrap()
            );
            assert_eq!(generated.rooms, repeated.rooms);
            for (id, region) in &generated.regions {
                let first = region.anchors["trapdoor_landing_0"];
                let second = region.anchors["trapdoor_landing_1"];
                assert_ne!(first, second);
                for at in [first, second] {
                    assert_eq!(at[2], 1);
                    for z in 0..2 {
                        assert!(!region.walls.contains(&[at[0], at[1], z]));
                    }
                    assert!(!region.doors.iter().any(|door| door.at[..2] == at[..2]));
                    assert!(!region.traps.iter().any(|trap| trap.at[..2] == at[..2]));
                }
                if !present.contains(id) {
                    assert!(
                        region.traps.is_empty(),
                        "omitted slot is not a room trap candidate"
                    );
                    // A nearby room may illuminate a closure surface, but never
                    // an omitted slot's corridor floor wholesale.
                    assert!(region.lighting.iter().all(|light| region
                        .doors
                        .iter()
                        .any(|door| door.at[..2] == light.at[..2])));
                }
            }
        }
    }

    #[test]
    fn zero_omissions_preserve_legacy_feature_endpoints_and_lighting() {
        let (group, _, definitions) = fixture();
        let recipe = hazard_recipe();
        for seed in [0, 42, u64::MAX] {
            let expected =
                super::materialize("floor", &group, &recipe, definitions.clone(), seed).unwrap();
            let actual = super::materialize(
                "floor",
                &group,
                &with_omissions(recipe.clone(), [0, 0]),
                definitions.clone(),
                seed,
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(expected.regions).unwrap(),
                serde_json::to_value(actual.regions).unwrap()
            );
            assert_eq!(expected.rooms, actual.rooms);
        }
    }

    #[test]
    fn omission_decisions_are_independent_of_lighting_and_feature_streams() {
        let (group, recipe, definitions) = fixture();
        let base = with_omissions(recipe, [3, 3]);
        let mut features = base.clone();
        features
            .stages
            .extend(hazard_recipe().stages.into_iter().skip(6));
        for seed in [0, 42, u64::MAX] {
            let expected =
                super::materialize("floor", &group, &base, definitions.clone(), seed).unwrap();
            let actual =
                super::materialize("floor", &group, &features, definitions.clone(), seed).unwrap();
            assert_eq!(expected.rooms, actual.rooms);
            for id in &group.members {
                assert_eq!(expected.regions[id].walls, actual.regions[id].walls);
            }
        }
    }

    #[test]
    fn insufficient_omitted_slot_landing_clearance_rejects_without_alcoves_or_rerolls() {
        let (group, _, definitions) = fixture();
        let mut recipe = with_omissions(hazard_recipe(), [3, 3]);
        // Every ordinary room has 120 cells; only corridor-only slots lack
        // the 64 distinct landings. Rejection must not silently grow alcoves.
        recipe.stages[2].operation = Operation::Rooms {
            width: [24, 24],
            height: [5, 5],
        };
        let Operation::HiddenTraps { config } = &mut recipe.stages.last_mut().unwrap().operation
        else {
            unreachable!()
        };
        config.count = [0, 64];
        check(&recipe).unwrap();
        let mut ordinary = recipe.clone();
        ordinary.stages.remove(6);
        ordinary.version = 3;
        super::materialize("floor", &group, &ordinary, definitions.clone(), 42).unwrap();
        let error = super::materialize("floor", &group, &recipe, definitions, 42)
            .err()
            .unwrap();
        assert!(
            error.message.contains("Trapdoor landing capacity"),
            "{}",
            error.message
        );
    }

    fn with_omissions(mut recipe: Recipe, count: [u32; 2]) -> Recipe {
        recipe.version = 4;
        recipe.stages.insert(
            6,
            Stage {
                id: "room_omissions".into(),
                version: 1,
                operation: Operation::RoomOmissions {
                    config: RoomOmissions { count },
                },
            },
        );
        check(&recipe).unwrap();
        recipe
    }

    #[test]
    fn zero_omissions_preserve_exact_legacy_geometry_anchors_light_and_rooms() {
        let (group, recipe, definitions) = fixture();
        for seed in [0, 1, 42, u64::MAX] {
            let expected =
                super::materialize("floor", &group, &recipe, definitions.clone(), seed).unwrap();
            let actual = super::materialize(
                "floor",
                &group,
                &with_omissions(recipe.clone(), [0, 0]),
                definitions.clone(),
                seed,
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(expected.regions).unwrap(),
                serde_json::to_value(actual.regions).unwrap()
            );
            assert_eq!(expected.rooms, actual.rooms);
        }
    }

    #[test]
    fn omissions_preserve_real_entry_and_stair_owners_and_noncontiguous_room_ids() {
        let (group, recipe, mut definitions) = fixture();
        // Protect actual owners, deliberately not just the first/last slots.
        for (slot, def) in definitions.iter_mut().enumerate() {
            if ![1, 4, 8].contains(&slot) {
                def.generate = Some(toml::from_str("generator='group'\nversion=1\ngroup='floor'\nrooms=[1,1]\nstair_anchors=['up','down']\n").unwrap());
            }
        }
        let legacy = super::materialize("floor", &group, &recipe, definitions.clone(), 42).unwrap();
        let generated = super::materialize(
            "floor",
            &group,
            &with_omissions(recipe.clone(), [3, 3]),
            definitions.clone(),
            42,
        )
        .unwrap();
        assert_eq!(
            generated.rooms.iter().map(|r| r.id).collect::<Vec<_>>(),
            [1, 3, 4, 6, 7, 8]
        );
        for id in [1, 3, 4, 6, 7, 8] {
            assert_eq!(generated.regions[&id].anchors, legacy.regions[&id].anchors);
        }
        for id in [2, 5, 9] {
            assert!(generated.regions[&id].walls.len() > legacy.regions[&id].walls.len());
        }
        definitions[1].anchors.insert("entry".into(), [13, 3, 0]);
        let error = super::materialize(
            "floor",
            &group,
            &with_omissions(recipe, [3, 3]),
            definitions,
            42,
        )
        .err()
        .unwrap();
        assert!(error.message.contains("unprotected slot capacity"));
    }

    #[test]
    fn omission_capacity_is_bounded_for_singletons_rectangles_and_maximum_grids() {
        let (_, recipe, _) = fixture();
        for grid in [[1, 1], [2, 3], [8, 8]] {
            let count = grid[0] * grid[1];
            let group = GenerationGroup {
                depth: 1,
                recipe: "floor".into(),
                grid,
                members: (1..=u64::from(count)).collect(),
            };
            let definitions: Vec<RegionDef> = group
                .members
                .iter()
                .map(|id| {
                    toml::from_str(&format!(
                        "id={id}\nname='slot'\nsize=[26,7,2]\nchamber=true\n"
                    ))
                    .unwrap()
                })
                .collect();
            let generated = super::materialize(
                "floor",
                &group,
                &with_omissions(recipe.clone(), [count - 1, 63]),
                definitions.clone(),
                7,
            )
            .unwrap();
            assert_eq!(generated.rooms.len(), 1);
            assert_eq!(generated.regions.len(), count as usize);
            if count < 64 {
                assert!(super::materialize(
                    "floor",
                    &group,
                    &with_omissions(recipe.clone(), [count, count]),
                    definitions,
                    7
                )
                .is_err());
            }
        }
    }

    #[test]
    fn omitted_geometry_preserves_two_cell_clearance_at_minimum_and_maximum_slot_sizes() {
        let group = GenerationGroup {
            depth: 1,
            recipe: "floor".into(),
            grid: [2, 3],
            members: (1..=6).collect(),
        };
        for (size, room_size) in [(5, 3), (32, 3), (32, 30)] {
            let recipe = GridRoomsPreset {
                version: 1,
                room_width: [room_size, room_size],
                room_height: [room_size, room_size],
                extra_connections: [0, 0],
                room_omissions: Some(RoomOmissions { count: [5, 5] }),
                lighting: None,
                secret_entrances: None,
                hidden_traps: None,
            }
            .to_recipe()
            .unwrap();
            let definitions = group
                .members
                .iter()
                .map(|id| {
                    toml::from_str(&format!(
                        "id={id}\nname='slot'\nsize=[{size},{size},2]\nchamber=true\n"
                    ))
                    .unwrap()
                })
                .collect();
            let generated = super::materialize("floor", &group, &recipe, definitions, 42).unwrap();
            assert_eq!(generated.rooms.len(), 1);
            let mut open = BTreeSet::new();
            for (slot, id) in group.members.iter().enumerate() {
                let region = &generated.regions[id];
                for x in 0..size {
                    for y in 0..size {
                        let floor_open = !region.walls.contains(&[x, y, 0]);
                        assert_eq!(floor_open, !region.walls.contains(&[x, y, 1]));
                        if floor_open {
                            open.insert((x + slot as i32 % 2 * size, y + slot as i32 / 2 * size));
                        }
                    }
                }
            }
            validate_carving(&open, [size * 2, size * 3, 2]).unwrap();
        }
    }

    #[test]
    fn omission_authoring_and_compiled_stage_reject_invalid_bounds_and_order() {
        let source = serde_json::json!({"version":1,"room_width":[4,24],"room_height":[3,5],"extra_connections":[0,2],"room_omissions":{"count":[1,3]}});
        let preset: GridRoomsPreset = serde_json::from_value(source.clone()).unwrap();
        let valid = preset.to_recipe().unwrap();
        for count in [[4, 3], [0, 64], [u32::MAX, u32::MAX]] {
            let mut invalid = source.clone();
            invalid["room_omissions"]["count"] = serde_json::json!(count);
            assert!(serde_json::from_value::<GridRoomsPreset>(invalid)
                .unwrap()
                .to_recipe()
                .is_err());
        }
        for policy in [
            serde_json::json!({}),
            serde_json::json!({"count":[1,3],"extra":0}),
            serde_json::json!({"count":true}),
            serde_json::json!({"count":[-1,3]}),
            serde_json::json!({"count":[1,2,3]}),
        ] {
            let mut invalid = source.clone();
            invalid["room_omissions"] = policy;
            assert!(serde_json::from_value::<GridRoomsPreset>(invalid).is_err());
        }
        for version in [0, 1, 2, 3, 5, u32::MAX] {
            let mut invalid = valid.clone();
            invalid.version = version;
            assert!(check(&invalid).is_err());
        }
        let mut duplicate = valid.clone();
        duplicate.stages.push(valid.stages[6].clone());
        duplicate.stages.last_mut().unwrap().id = "duplicate".into();
        assert!(check(&duplicate).is_err());
        let mut misplaced = valid.clone();
        misplaced.stages.swap(5, 6);
        assert!(check(&misplaced).is_err());
        let mut missing = valid;
        missing.stages.pop();
        assert!(check(&missing).is_err());
    }

    #[test]
    fn authored_room_omissions_reduce_present_rooms_without_removing_slots() {
        let source = serde_json::json!({
            "version": 1,
            "room_width": [4, 24],
            "room_height": [3, 5],
            "extra_connections": [0, 2]
        });
        let (group, _, definitions) = fixture();
        let baseline: GridRoomsPreset = serde_json::from_value(source.clone()).unwrap();
        let baseline = super::materialize(
            "floor",
            &group,
            &baseline.to_recipe().unwrap(),
            definitions.clone(),
            42,
        )
        .unwrap();
        assert_eq!(baseline.rooms.len(), 9);
        let mut source = source;
        source["room_omissions"] = serde_json::json!({"count": [3, 3]});
        let preset: GridRoomsPreset = serde_json::from_value(source).unwrap();
        let recipe = preset.to_recipe().unwrap();
        assert_eq!(recipe.version, 4);
        let generated = super::materialize("floor", &group, &recipe, definitions, 42).unwrap();
        assert_eq!(generated.rooms.len(), 6);
        assert_eq!(
            generated.regions.keys().copied().collect::<Vec<_>>(),
            group.members
        );
        assert!(generated.rooms.iter().all(|room| !room.cells.is_empty()));
    }

    #[test]
    fn typed_grid_room_presets_reject_unsupported_versions_and_invalid_parameters() {
        let source = serde_json::json!({
            "version": 1,
            "room_width": [4, 24],
            "room_height": [3, 5],
            "extra_connections": [0, 2]
        });
        let valid: GridRoomsPreset = serde_json::from_value(source.clone()).unwrap();
        assert!(valid.to_recipe().is_ok());
        for (field, value) in [
            ("version", serde_json::json!(0)),
            ("version", serde_json::json!(u32::MAX)),
            ("room_width", serde_json::json!([2, 24])),
            ("room_height", serde_json::json!([6, 5])),
            ("extra_connections", serde_json::json!([3, 2])),
            (
                "lighting",
                serde_json::json!({"darkness_roll": 0, "darkness_start": 1}),
            ),
        ] {
            let mut invalid = source.clone();
            invalid[field] = value;
            let preset: GridRoomsPreset = serde_json::from_value(invalid).unwrap();
            assert!(preset.to_recipe().is_err(), "accepted invalid {field}");
        }
        let mut unsupported_field = source.clone();
        unsupported_field["stages"] = serde_json::json!([]);
        assert!(serde_json::from_value::<GridRoomsPreset>(unsupported_field).is_err());
        let mut missing_version = source;
        missing_version.as_object_mut().unwrap().remove("version");
        assert!(serde_json::from_value::<GridRoomsPreset>(missing_version).is_err());
    }

    #[test]
    fn layout_validation_rejects_room_ranges_that_do_not_fit_the_slots() {
        let (_, recipe, _) = fixture();
        assert!(check_layout(&recipe, [26, 7, 2]).is_ok());
        for size in [
            [25, 7, 2],
            [26, 6, 2],
            [i32::MAX, 7, 2],
            [26, i32::MIN, 2],
            [26, 7, 3],
        ] {
            assert!(check_layout(&recipe, size).is_err());
        }
        let mut invalid = recipe;
        invalid.stages.clear();
        assert!(check_layout(&invalid, [26, 7, 2]).is_err());
    }

    #[test]
    fn invalid_carving_bounds_reject_without_coordinate_overflow() {
        let open = BTreeSet::from([(1, 1)]);
        for size in [
            [i32::MIN, 5, 2],
            [5, i32::MIN, 2],
            [5, 5, i32::MIN],
            [2, 5, 2],
            [5, 2, 2],
        ] {
            assert!(validate_carving(&open, size).is_err());
        }
    }

    fn rectangular_fixture(grid: [u32; 2]) -> (GenerationGroup, Recipe, Vec<RegionDef>) {
        let (group, recipe, definitions) = fixture();
        let count = (grid[0] * grid[1]) as usize;
        let mut group = serde_json::to_value(group).unwrap();
        group["grid"] = serde_json::json!(grid);
        group["members"] = serde_json::json!((1..=count as u64).collect::<Vec<_>>());
        let group = serde_json::from_value(group).unwrap();
        let definitions = (1..=count)
            .map(|id| {
                let mut definition = definitions[0].clone();
                definition.id = id as u64;
                definition
            })
            .collect();
        (group, recipe, definitions)
    }

    #[test]
    fn rectangular_grids_generate_connected_floors_at_every_supported_boundary() {
        for grid in [[1, 1], [1, 8], [8, 1], [2, 3], [3, 2], [8, 8]] {
            let (group, recipe, definitions) = rectangular_fixture(grid);
            for seed in [0, 42, u64::MAX] {
                let first =
                    materialize("grid", &group, &recipe, definitions.clone(), seed).unwrap();
                let second =
                    materialize("grid", &group, &recipe, definitions.clone(), seed).unwrap();
                assert_eq!(
                    serde_json::to_value(&first).unwrap(),
                    serde_json::to_value(second).unwrap()
                );
                assert_eq!(first.len(), (grid[0] * grid[1]) as usize);
                let mut open = BTreeSet::new();
                for (slot, id) in group.members.iter().enumerate() {
                    let definition = &first[id];
                    let walls: BTreeSet<_> = definition.walls.iter().copied().collect();
                    for y in 0..definition.size[1] {
                        for x in 0..definition.size[0] {
                            if !walls.contains(&[x, y, 0]) {
                                assert!(!walls.contains(&[x, y, 1]));
                                open.insert((
                                    x + (slot as u32 % grid[0]) as i32 * definition.size[0],
                                    y + (slot as u32 / grid[0]) as i32 * definition.size[1],
                                ));
                            }
                        }
                    }
                }
                validate_carving(&open, [26 * grid[0] as i32, 7 * grid[1] as i32, 2]).unwrap();
            }
        }
    }

    #[test]
    fn rectangular_grid_stairs_are_distinct_clear_and_seeded_in_every_member() {
        for grid in [[1, 1], [2, 3], [8, 1]] {
            let (group, recipe, mut definitions) = rectangular_fixture(grid);
            for definition in &mut definitions {
                definition.generate = Some(toml::from_str(
                    "generator='group'\nversion=1\ngroup='grid'\nrooms=[1,1]\nstair_anchors=['up','down']\n",
                ).unwrap());
            }
            let mut positions = BTreeSet::new();
            for seed in 0..8 {
                let generated =
                    materialize("grid", &group, &recipe, definitions.clone(), seed).unwrap();
                for definition in generated.values() {
                    let up = definition.anchors["up"];
                    let down = definition.anchors["down"];
                    assert_ne!(up, down);
                    for [x, y, z] in [up, down] {
                        assert_eq!(z, 0);
                        assert!(!definition.walls.contains(&[x, y, 0]));
                        assert!(!definition.walls.contains(&[x, y, 1]));
                    }
                    if definition.id == group.members[0] {
                        positions.insert((up, down));
                    }
                }
            }
            assert!(positions.len() > 1);
        }
    }

    #[test]
    fn malformed_grid_dimensions_and_member_counts_reject_before_generation() {
        let (group, recipe, definitions) = fixture();
        for grid in [[0, 3], [3, 0], [9, 1], [1, 9], [u32::MAX, u32::MAX], [2, 3]] {
            let mut encoded = serde_json::to_value(&group).unwrap();
            encoded["grid"] = serde_json::json!(grid);
            let group: GenerationGroup = serde_json::from_value(encoded).unwrap();
            assert!(materialize("grid", &group, &recipe, definitions.clone(), 42).is_err());
        }
        for members in [vec![], vec![1], vec![1; 9], (1..=65).collect::<Vec<_>>()] {
            let mut encoded = serde_json::to_value(&group).unwrap();
            encoded["members"] = serde_json::json!(members);
            let group: GenerationGroup = serde_json::from_value(encoded).unwrap();
            assert!(materialize("grid", &group, &recipe, definitions.clone(), 42).is_err());
        }
    }

    #[test]
    fn grid_members_require_equal_bounded_sizes_and_exact_distinct_definitions() {
        let (group, recipe, definitions) = fixture();
        for size in [
            [i32::MAX, 7, 2],
            [26, i32::MAX, 2],
            [26, 7, i32::MAX],
            [0, 7, 2],
            [26, 7, 1],
        ] {
            let mut definitions = definitions.clone();
            for definition in &mut definitions {
                definition.size = size;
            }
            assert!(materialize("grid", &group, &recipe, definitions, 42).is_err());
        }
        let mut unequal = definitions.clone();
        unequal[1].size[0] += 1;
        assert!(materialize("grid", &group, &recipe, unequal, 42).is_err());
        let mut duplicate = definitions.clone();
        duplicate[1].id = duplicate[0].id;
        assert!(materialize("grid", &group, &recipe, duplicate, 42).is_err());
        let mut unknown = definitions;
        unknown[1].id = 999;
        assert!(materialize("grid", &group, &recipe, unknown, 42).is_err());
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
        for id in &group.members {
            assert_eq!(first[id].walls, baseline[id].walls);
            assert_eq!(first[id].anchors, baseline[id].anchors);
            assert!(!first[id].lit);
            assert!(!first[id].lighting.is_empty());
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
    fn discovery_stages_preserve_geometry_stairs_and_each_others_streams() {
        let (group, baseline_recipe, mut defs) = fixture();
        defs[0].generate = Some(toml::from_str("generator='group'\nversion=1\ngroup='floor-1'\nrooms=[1,1]\nstair_anchors=['up','down']\n").unwrap());
        let secret = Stage {
            id: "secret_entrances".into(),
            version: 1,
            operation: Operation::SecretEntrances {
                config: SecretEntrances {
                    chance_per_mille: 1000,
                    maximum: 64,
                    dc: 12,
                },
            },
        };
        let traps = Stage {
            id: "hidden_traps".into(),
            version: 1,
            operation: Operation::HiddenTraps {
                config: HiddenTraps {
                    count: [8, 8],
                    dc: 12,
                    rust_slot: "body_armor".into(),
                    rust_amount: 1,
                    sleep_condition: "rogue.sleep".into(),
                    sleep_duration: 200,
                    sleep_exposure: None,
                    hazards: None,
                },
            },
        };
        let mut secret_recipe = baseline_recipe.clone();
        secret_recipe.version = 3;
        secret_recipe.stages.push(secret.clone());
        let mut trap_recipe = baseline_recipe.clone();
        trap_recipe.version = 3;
        trap_recipe.stages.push(traps.clone());
        let mut both_recipe = secret_recipe.clone();
        both_recipe.stages.push(traps);
        let mut doors = 0;
        for seed in [0, 42, u64::MAX] {
            let baseline =
                materialize("floor-1", &group, &baseline_recipe, defs.clone(), seed).unwrap();
            let secrets =
                materialize("floor-1", &group, &secret_recipe, defs.clone(), seed).unwrap();
            let traps = materialize("floor-1", &group, &trap_recipe, defs.clone(), seed).unwrap();
            let both = materialize("floor-1", &group, &both_recipe, defs.clone(), seed).unwrap();
            assert_eq!(both.values().map(|r| r.traps.len()).sum::<usize>(), 8);
            let mut door_ids = BTreeSet::new();
            let mut trap_ids = BTreeSet::new();
            for id in &group.members {
                assert_eq!(both[id].walls, baseline[id].walls);
                assert_eq!(both[id].anchors, baseline[id].anchors);
                assert_eq!(
                    serde_json::to_value(&both[id].doors).unwrap(),
                    serde_json::to_value(&secrets[id].doors).unwrap()
                );
                assert_eq!(
                    serde_json::to_value(&both[id].traps).unwrap(),
                    serde_json::to_value(&traps[id].traps).unwrap()
                );
                for door in &both[id].doors {
                    assert!(!door.open && door.height == 2 && door.concealment.is_some());
                    assert!(!both[id].walls.contains(&door.at));
                    assert!(!both[id].anchors.values().any(|at| *at == door.at));
                    assert!(door_ids.insert(door.id));
                    doors += 1;
                }
                for trap in &both[id].traps {
                    assert!(!both[id].walls.contains(&trap.at));
                    assert!(!both[id].anchors.values().any(|at| *at == trap.at));
                    assert!(!both[id].doors.iter().any(|d| d.at == trap.at));
                    assert!(trap_ids.insert(trap.id));
                }
            }
            // The all-open graph is the unchanged baseline carving, so required
            // routes cannot be removed by feature placement.
            assert!(both
                .values()
                .flat_map(|r| &r.doors)
                .all(|door| door.concealment.is_some()));
        }
        assert!(doors > 0, "test must exercise actual opaque entrances");
    }

    #[test]
    fn hidden_trap_minimum_cannot_silently_exceed_room_capacity() {
        let recipe = GridRoomsPreset {
            version: 1,
            room_width: [3, 3],
            room_height: [3, 3],
            extra_connections: [0, 0],
            room_omissions: None,
            lighting: None,
            secret_entrances: None,
            hidden_traps: Some(HiddenTraps {
                count: [10, 10],
                dc: 12,
                rust_slot: "body_armor".into(),
                rust_amount: 1,
                sleep_condition: "rogue.sleep".into(),
                sleep_duration: 200,
                sleep_exposure: None,
                hazards: None,
            }),
        }
        .to_recipe()
        .unwrap();
        let group = GenerationGroup {
            depth: 1,
            recipe: "small".into(),
            grid: [1, 1],
            members: vec![1],
        };
        let definition =
            toml::from_str("id=1\nname='small'\nsize=[5,5,2]\nchamber=true\n").unwrap();
        let error = materialize("small", &group, &recipe, vec![definition], 42).unwrap_err();
        assert!(error.message.contains("capacity"), "{}", error.message);
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
    #[test]
    fn physical_trapdoor_endpoints_are_order_independent_clear_and_absent_at_terminal_depth() {
        use crate::procedural_world::{Domain, FamilyKind, WorldFamily};
        let family = WorldFamily {
            kind: FamilyKind::GridChain,
            version: 1,
            base_region: 1,
            domain: Domain::Finite { count: 2 },
            grid: [2, 1],
            size: [10, 8, 2],
            recipe: "hazards".into(),
            zone: None,
            gravity: Some([0, 0, -1]),
        };
        let recipe = GridRoomsPreset {
            version: 1,
            room_width: [4, 6],
            room_height: [3, 4],
            extra_connections: [0, 0],
            room_omissions: None,
            lighting: None,
            secret_entrances: None,
            hidden_traps: Some(HiddenTraps {
                count: [2, 2],
                dc: 12,
                rust_slot: "body_armor".into(),
                rust_amount: 1,
                sleep_condition: "sleep".into(),
                sleep_duration: 200,
                sleep_exposure: None,
                hazards: Some(HazardTraps {
                    bear_condition: "bear".into(),
                    bear_duration: 300,
                    arrow_ammunition: "arrow".into(),
                    attack_bonus: 0,
                    poison_exposure: "poison".into(),
                    avoidance_dc: 12,
                }),
            }),
        }
        .to_recipe()
        .unwrap();
        let mut doors = 0;
        for seed in 0..32 {
            let build = |depth| {
                let group = family.group("grid", seed, depth).unwrap();
                let definitions = group
                    .members
                    .iter()
                    .map(|id| family.describe("grid", seed, *id).unwrap().unwrap())
                    .collect();
                materialize(&format!("grid/{depth}"), &group, &recipe, definitions, seed).unwrap()
            };
            // Build destinations first: there is no previous-depth mutable lookup.
            let lower = build(2);
            let upper = build(1);
            assert_eq!(
                serde_json::to_value(&upper).unwrap(),
                serde_json::to_value(build(1)).unwrap()
            );
            assert!(lower
                .values()
                .all(|region| region.trapdoor_destination.is_none()
                    && region.traps.iter().all(|trap| !matches!(
                        trap.kind,
                        crate::scenario_package::TrapKind::TrapDoor { .. }
                    ))));
            for source in upper.values() {
                for trap in &source.traps {
                    if let crate::scenario_package::TrapKind::TrapDoor { destination, open } =
                        &trap.kind
                    {
                        doors += 1;
                        assert!(!open);
                        assert_eq!(destination.region.0, source.id + family.slots());
                        let target = &lower[&destination.region.0];
                        let landing = target.anchors[&destination.name];
                        assert_eq!(landing[2], target.size[2] - 1);
                        assert!((0..target.size[2])
                            .all(|z| !target.walls.contains(&[landing[0], landing[1], z])));
                        assert!(!target.traps.iter().any(|trap| trap.at[..2] == landing[..2]));
                    }
                }
            }
        }
        assert!(doors > 0, "witness actual generated trapdoors");
    }
}

#[path = "generation_features.rs"]
mod features;
use features::place_concealed_features;
pub use features::{FeatureBases, FEATURE_IDENTITY_STRIDE};
