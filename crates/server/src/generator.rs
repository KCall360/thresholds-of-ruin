//! Procedural region sources. A package declares a generated region's fixed
//! structure (bounds, zone, entry anchors and links) and the generator fills
//! it on first build: `rooms` carves rooms joined by corridors that reach
//! every entry, and places actors and items from the region's pools. See
//! `docs/scenario-packages.md#generated-regions`.
//!
//! A region's content depends only on its own definition and seed, never on
//! which regions were built before, and its identities come from a range
//! fixed by its region id, so building regions in any order, or ahead of
//! need on another thread, gives the same result.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::scenario_package::{Actor, Item, RegionDef};
use crate::Failure;

/// The generator and version a region asks for.
pub const ROOMS: &str = "rooms";
pub const ROOMS_VERSION: u32 = 1;
/// Identities each generated region may use, for actors and for items: a
/// region's range starts at `base + (region - 1) * IDENTITY_STRIDE`.
pub const IDENTITY_STRIDE: u64 = 256;
const MAX_ROOMS: u32 = 16;
const MAX_PLACED: u32 = 64;
/// Cells kept clear around each entry anchor, in each direction, so links of
/// up to this many extra cells land on open floor.
const CLEARING: i32 = 2;

/// A generated region's recipe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generate {
    pub generator: String,
    pub version: u32,
    /// How many rooms, at least and at most.
    pub rooms: [u32; 2],
    pub actors: Option<ActorPool>,
    pub items: Option<ItemPool>,
}

/// Actors placed in a generated region: each an archetype from the list,
/// controlled by the named AI profile.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorPool {
    pub archetypes: Vec<String>,
    pub ai: String,
    pub count: [u32; 2],
}

/// Items placed on a generated region's floor, each an archetype from the list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ItemPool {
    pub archetypes: Vec<String>,
    pub count: [u32; 2],
}

fn fail(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}

/// Deterministic SplitMix64, seeded per region.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
    /// Uniform in `low..=high`.
    fn range(&mut self, low: u32, high: u32) -> u32 {
        low + (self.next() % u64::from(high - low + 1)) as u32
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }
}

/// Check a recipe's shape, before any generation.
pub(crate) fn check(def: &RegionDef, generate: &Generate) -> Result<(), Failure> {
    let id = def.id;
    if generate.generator != ROOMS || generate.version != ROOMS_VERSION {
        return Err(fail(format!(
            "Region {id}: unknown generator {} version {}",
            generate.generator, generate.version
        )));
    }
    let range = |[low, high]: [u32; 2], max: u32| low <= high && high <= max;
    if !range(generate.rooms, MAX_ROOMS) || generate.rooms[1] == 0 {
        return Err(fail(format!("Region {id}: rooms must be 1..{MAX_ROOMS}")));
    }
    let placed = generate.actors.as_ref().map_or(0, |p| p.count[1])
        + generate.items.as_ref().map_or(0, |p| p.count[1]);
    if generate
        .actors
        .as_ref()
        .is_some_and(|p| !range(p.count, MAX_PLACED) || p.archetypes.is_empty())
        || generate
            .items
            .as_ref()
            .is_some_and(|p| !range(p.count, MAX_PLACED) || p.archetypes.is_empty())
        || u64::from(placed) > IDENTITY_STRIDE
    {
        return Err(fail(format!(
            "Region {id}: pools need archetypes and at most {MAX_PLACED} of each"
        )));
    }
    if !def.walls.is_empty()
        || !def.openings.is_empty()
        || !def.places.is_empty()
        || !def.doors.is_empty()
        || !def.items.is_empty()
        || !def.actors.is_empty()
        || !def.gravity_overrides.is_empty()
    {
        return Err(fail(format!(
            "Region {id}: a generated region authors only its bounds, anchors and links"
        )));
    }
    if def.anchors.is_empty() {
        return Err(fail(format!(
            "Region {id}: a generated region needs an entry anchor"
        )));
    }
    Ok(())
}

type Cell = (i32, i32);

/// Fill a generated region: the definition an authored region with this
/// content would have. Actors and items take identities from `first_actor`
/// and `first_item` up.
pub(crate) fn materialize(
    def: &RegionDef,
    generate: &Generate,
    seed: u64,
    first_actor: u64,
    first_item: u64,
) -> Result<RegionDef, Failure> {
    check(def, generate)?;
    let [width, depth, height] = def.size;
    let mut rng = Rng(seed);
    let mut open: BTreeSet<Cell> = BTreeSet::new();
    let inside = |(x, y): Cell| x >= 0 && y >= 0 && x < width && y < depth;
    // Entries, with their clearings, are always open.
    let entries: Vec<Cell> = def.anchors.values().map(|[x, y, _]| (*x, *y)).collect();
    for &(x, y) in &entries {
        for dx in -CLEARING..=CLEARING {
            for dy in -CLEARING..=CLEARING {
                if inside((x + dx, y + dy)) {
                    open.insert((x + dx, y + dy));
                }
            }
        }
    }
    // Rooms, then corridors joining every room and entry in turn.
    let mut nodes = entries.clone();
    let rooms = rng.range(generate.rooms[0].max(1), generate.rooms[1]);
    for _ in 0..rooms {
        let w = rng.range(1, (width as u32).clamp(1, 6)) as i32;
        let d = rng.range(1, (depth as u32).clamp(1, 6)) as i32;
        let x = rng.range(0, (width - w) as u32) as i32;
        let y = rng.range(0, (depth - d) as u32) as i32;
        for cx in x..x + w {
            for cy in y..y + d {
                open.insert((cx, cy));
            }
        }
        nodes.push((x + w / 2, y + d / 2));
    }
    for pair in nodes.windows(2) {
        let ((ax, ay), (bx, by)) = (pair[0], pair[1]);
        let horizontal_first = rng.next().is_multiple_of(2);
        let corner = if horizontal_first { (bx, ay) } else { (ax, by) };
        for (from, to) in [((ax, ay), corner), (corner, (bx, by))] {
            let (mut x, mut y) = from;
            open.insert((x, y));
            while (x, y) != to {
                x += (to.0 - x).signum();
                y += (to.1 - y).signum();
                open.insert((x, y));
            }
        }
    }
    let mut out = def.clone();
    out.walls = (0..width)
        .flat_map(|x| (0..depth).map(move |y| (x, y)))
        .filter(|cell| !open.contains(cell))
        .flat_map(|(x, y)| (0..height).map(move |z| [x, y, z]))
        .collect();
    // Placement: open floor away from every entry's clearing.
    let near_entry = |(x, y): Cell| {
        entries
            .iter()
            .any(|(ex, ey)| (x - ex).abs() <= CLEARING && (y - ey).abs() <= CLEARING)
    };
    let mut floor: Vec<Cell> = open.iter().copied().filter(|c| !near_entry(*c)).collect();
    let mut take = |rng: &mut Rng| -> Option<[i32; 3]> {
        if floor.is_empty() {
            return None;
        }
        let (x, y) = floor.remove((rng.next() % floor.len() as u64) as usize);
        Some([x, y, 0])
    };
    if let Some(pool) = &generate.actors {
        let count = rng.range(pool.count[0], pool.count[1]);
        for n in 0..u64::from(count) {
            let Some(at) = take(&mut rng) else { break };
            out.actors.push(Actor {
                combat: None,
                body: None,
                velocity: None,
                id: first_actor + n,
                at,
                archetype: Some(rng.pick(&pool.archetypes).clone()),
                turn_ticks: None,
                controller: "ai".into(),
                ai: Some(pool.ai.clone()),
            });
        }
    }
    if let Some(pool) = &generate.items {
        let count = rng.range(pool.count[0], pool.count[1]);
        for n in 0..u64::from(count) {
            let Some(at) = take(&mut rng) else { break };
            out.items.push(Item {
                quantity: 1,
                stackable: None,
                properties: BTreeMap::new(),
                id: first_item + n,
                at,
                archetype: Some(rng.pick(&pool.archetypes).clone()),
                name: None,
                carried_by: None,
                seed_names: Vec::new(),
            });
        }
    }
    Ok(out)
}

/// Whether every entry anchor of a materialized region reaches every other
/// over open floor.
pub(crate) fn entries_connected(region: &RegionDef) -> bool {
    let walls: BTreeSet<Cell> = region
        .walls
        .iter()
        .filter(|[_, _, z]| *z == 0)
        .map(|[x, y, _]| (*x, *y))
        .collect();
    let [width, depth, _] = region.size;
    let entries: Vec<Cell> = region.anchors.values().map(|[x, y, _]| (*x, *y)).collect();
    let Some(&start) = entries.first() else {
        return false;
    };
    let mut seen = BTreeSet::from([start]);
    let mut queue = VecDeque::from([start]);
    while let Some((x, y)) = queue.pop_front() {
        for next in [(x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)] {
            if next.0 >= 0
                && next.1 >= 0
                && next.0 < width
                && next.1 < depth
                && !walls.contains(&next)
                && seen.insert(next)
            {
                queue.push_back(next);
            }
        }
    }
    entries.iter().all(|e| seen.contains(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixture's first cave: 24x12, entries on row 6, 1-3 rats and 2-4
    /// coins.
    fn cave() -> (RegionDef, Generate) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/tests/generated-filler/regions/2.toml");
        let def: RegionDef = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let generate = def.generate.clone().unwrap();
        (def, generate)
    }

    fn json(def: &RegionDef) -> String {
        serde_json::to_string(def).unwrap()
    }

    #[test]
    fn a_seed_always_generates_the_same_region_and_other_seeds_differ() {
        let (def, generate) = cave();
        let first = materialize(&def, &generate, 7, 100, 200).unwrap();
        assert_eq!(
            json(&first),
            json(&materialize(&def, &generate, 7, 100, 200).unwrap())
        );
        let others: BTreeSet<_> = (0..8)
            .map(|seed| json(&materialize(&def, &generate, seed, 100, 200).unwrap()))
            .collect();
        assert!(others.len() > 1, "seeds should vary the cave");
    }

    #[test]
    fn entries_connect_and_placements_stay_on_open_floor_within_their_ranges() {
        let (def, generate) = cave();
        for seed in 0..300 {
            let region = materialize(&def, &generate, seed, 100, 200).unwrap();
            assert!(entries_connected(&region), "seed {seed}");
            let walls: BTreeSet<_> = region.walls.iter().copied().collect();
            assert!((1..=3).contains(&region.actors.len()), "seed {seed}");
            assert!((2..=4).contains(&region.items.len()), "seed {seed}");
            let mut cells = BTreeSet::new();
            for (id, at) in region
                .actors
                .iter()
                .map(|a| (a.id - 100, a.at))
                .chain(region.items.iter().map(|i| (i.id - 200, i.at)))
            {
                assert!(id < IDENTITY_STRIDE, "seed {seed}");
                assert!(!walls.contains(&at) && cells.insert(at), "seed {seed}");
            }
            // The two entries share row 6, so a straight corridor joins them.
            assert!((0..24).all(|x| !walls.contains(&[x, 6, 0])), "seed {seed}");
        }
    }

    #[test]
    fn a_generated_region_authors_only_its_structure_and_names_a_known_generator() {
        let (def, generate) = cave();
        let mut walled = def.clone();
        walled.walls.push([3, 3, 0]);
        assert!(check(&walled, &generate).is_err());
        let mut unknown = generate.clone();
        unknown.generator = "mazes".into();
        assert!(check(&def, &unknown).is_err());
        let mut newer = generate.clone();
        newer.version = 2;
        assert!(check(&def, &newer).is_err());
        let mut roomless = generate.clone();
        roomless.rooms = [0, 0];
        assert!(check(&def, &roomless).is_err());
        let mut crowded = generate.clone();
        crowded.items.as_mut().unwrap().count = [0, MAX_PLACED + 1];
        assert!(check(&def, &crowded).is_err());
        let mut sealed = def.clone();
        sealed.anchors.clear();
        assert!(check(&sealed, &generate).is_err());
    }
}
