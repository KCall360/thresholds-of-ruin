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
use sha2::{Digest, Sha256};

use crate::scenario_package::{Actor, Item, RegionDef};
use crate::Failure;

/// The generator and version a region asks for.
pub const ROOMS: &str = "rooms";
pub const ROOMS_VERSION: u32 = 2;
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
    /// Coordinated recipe group. Required only by the `group` generator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Structural boundary references may remain in solid terrain.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub boundary_anchors: Vec<String>,
    pub generator: String,
    pub version: u32,
    /// Explicit generation variation; omitted and zero mean the same thing.
    #[serde(default)]
    pub salt: u64,
    /// How many rooms, at least and at most.
    pub rooms: [u32; 2],
    /// Stable names whose positions are selected from generated open floor.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stair_anchors: Vec<String>,
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

/// Only geometry inputs participate in geometry and placement streams.
/// Region names, raw file bytes, population and loot recipes do not reroll the floor.
#[derive(Serialize)]
struct GeometryParameters<'a> {
    size: [i32; 3],
    anchors: &'a BTreeMap<String, [i32; 3]>,
    rooms: [u32; 2],
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum RandomStream {
    Geometry,
    Placement,
    Population,
    Loot,
    Stairs,
}

#[derive(Serialize)]
struct StreamSeed<'a, P> {
    contract: &'static str,
    generator: &'a str,
    version: u32,
    salt: u64,
    game_seed: u64,
    region: u64,
    stream: RandomStream,
    parameters: &'a P,
}

fn stream_rng<P: Serialize>(
    def: &RegionDef,
    generate: &Generate,
    game_seed: u64,
    stream: RandomStream,
    parameters: &P,
) -> Result<Rng, Failure> {
    // These owned schema fields, ordered maps and integer/string values form
    // the versioned canonical seed encoding. No authoring text or integrity
    // digest is admitted here.
    let bytes = serde_json::to_vec(&StreamSeed {
        contract: "tor-region-generation-v2",
        generator: &generate.generator,
        version: generate.version,
        salt: generate.salt,
        game_seed,
        region: def.id,
        stream,
        parameters,
    })
    .map_err(|e| fail(format!("Region {}: seed encoding: {e}", def.id)))?;
    let digest = Sha256::digest(bytes);
    let mut seed = [0; 8];
    seed.copy_from_slice(&digest[..8]);
    Ok(Rng(u64::from_le_bytes(seed)))
}

fn placement_count(
    region: u64,
    kind: &str,
    rng: &mut Rng,
    [minimum, maximum]: [u32; 2],
    capacity: usize,
) -> Result<u32, Failure> {
    let maximum = maximum.min(capacity.min(MAX_PLACED as usize) as u32);
    if minimum > maximum {
        return Err(fail(format!(
            "Region {region}: {kind} placement capacity {capacity} is below requested minimum {minimum}"
        )));
    }
    Ok(rng.range(minimum, maximum))
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
    /// Select in `low..=high` using one deterministic draw.
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
    if generate.generator == "group" && generate.version == 1 {
        let boundaries: BTreeSet<_> = generate.boundary_anchors.iter().collect();
        if generate.group.as_ref().is_none_or(|name| name.is_empty())
            || boundaries.len() != generate.boundary_anchors.len()
            || boundaries
                .iter()
                .any(|name| !def.anchors.contains_key(*name))
            || generate.actors.is_some()
            || generate.items.is_some()
            || generate.rooms != [1, 1]
            || generate.salt != 0
            || !def.walls.is_empty()
            || !def.openings.is_empty()
            || !def.doors.is_empty()
            || !def.items.is_empty()
            || !def.actors.is_empty()
            || !def.places.is_empty()
            || !def.gravity_overrides.is_empty()
        {
            return Err(fail(format!(
                "Region {id}: invalid group generator structure"
            )));
        }
        let names: BTreeSet<_> = generate.stair_anchors.iter().collect();
        if names.len() != generate.stair_anchors.len()
            || names.iter().any(|name| {
                name.is_empty()
                    || name.len() > 80
                    || name.contains('/')
                    || def.anchors.contains_key(*name)
                    || name.chars().any(char::is_control)
            })
        {
            return Err(fail(format!("Region {id}: invalid group stair anchors")));
        }
        return Ok(());
    }
    if generate.group.is_some() || !generate.boundary_anchors.is_empty() {
        return Err(fail(format!(
            "Region {id}: group fields require the group generator"
        )));
    }
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
    let placed = u64::from(generate.actors.as_ref().map_or(0, |p| p.count[1]))
        + u64::from(generate.items.as_ref().map_or(0, |p| p.count[1]));
    if generate
        .actors
        .as_ref()
        .is_some_and(|p| !range(p.count, MAX_PLACED) || p.archetypes.is_empty())
        || generate
            .items
            .as_ref()
            .is_some_and(|p| !range(p.count, MAX_PLACED) || p.archetypes.is_empty())
        || placed > IDENTITY_STRIDE
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
    if def.anchors.is_empty() && generate.stair_anchors.is_empty() {
        return Err(fail(format!(
            "Region {id}: a generated region needs an entry anchor"
        )));
    }
    let names: BTreeSet<_> = generate.stair_anchors.iter().collect();
    if names.len() != generate.stair_anchors.len()
        || names.len() > MAX_PLACED as usize
        || names.iter().any(|name| {
            name.is_empty()
                || name.len() > 80
                || name.contains('/')
                || name.chars().any(char::is_control)
                || def.anchors.contains_key(*name)
        })
    {
        return Err(fail(format!(
            "Region {id}: invalid or duplicate generated stair anchor"
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
    let geometry = GeometryParameters {
        size: def.size,
        anchors: &def.anchors,
        rooms: generate.rooms,
    };
    let mut rng = stream_rng(def, generate, seed, RandomStream::Geometry, &geometry)?;
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
    // Each named endpoint has its own stream. Stable name order resolves
    // collisions; endpoints never depend on population, loot or build order.
    let mut stair_floor: Vec<_> = open
        .iter()
        .copied()
        .filter(|&(x, y)| !def.anchors.values().any(|p| *p == [x, y, 0]))
        .collect();
    for name in generate.stair_anchors.iter().collect::<BTreeSet<_>>() {
        if stair_floor.is_empty() {
            return Err(fail(format!(
                "Region {}: insufficient floor for stair anchor {name}",
                def.id
            )));
        }
        let mut stairs = stream_rng(
            def,
            generate,
            seed,
            RandomStream::Stairs,
            &(&geometry, name),
        )?;
        let selected = (stairs.next() % stair_floor.len() as u64) as usize;
        let (x, y) = stair_floor.remove(selected);
        out.anchors.insert(name.clone(), [x, y, 0]);
    }
    if generate.actors.is_none() && generate.items.is_none() {
        return Ok(out);
    }
    // Placement: open floor away from every entry's clearing.
    let near_entry = |(x, y): Cell| {
        entries
            .iter()
            .any(|(ex, ey)| (x - ex).abs() <= CLEARING && (y - ey).abs() <= CLEARING)
    };
    let mut floor: Vec<Cell> = open
        .iter()
        .copied()
        .filter(|&(x, y)| {
            !near_entry((x, y))
                && !generate
                    .stair_anchors
                    .iter()
                    .any(|name| out.anchors[name] == [x, y, 0])
        })
        .collect();
    // A shared deterministic permutation assigns disjoint alternating lanes.
    // Lane capacity and positions never depend on either pool's presence or
    // requested count; removing actors cannot move or crowd out loot.
    let mut placement = stream_rng(def, generate, seed, RandomStream::Placement, &geometry)?;
    for last in (1..floor.len()).rev() {
        let chosen = (placement.next() % (last + 1) as u64) as usize;
        floor.swap(last, chosen);
    }
    if let Some(pool) = &generate.actors {
        let mut population = stream_rng(def, generate, seed, RandomStream::Population, pool)?;
        let count = placement_count(
            def.id,
            "actor",
            &mut population,
            pool.count,
            floor.len().div_ceil(2),
        )?;
        for (n, &(x, y)) in floor.iter().step_by(2).take(count as usize).enumerate() {
            out.actors.push(Actor {
                creature: None,
                anatomy: None,
                known_identities: vec![],
                body: None,
                velocity: None,
                id: first_actor + n as u64,
                at: [x, y, 0],
                archetype: Some(population.pick(&pool.archetypes).clone()),
                turn_ticks: None,
                controller: "ai".into(),
                ai: Some(pool.ai.clone()),
            });
        }
    }
    if let Some(pool) = &generate.items {
        let mut loot = stream_rng(def, generate, seed, RandomStream::Loot, pool)?;
        let count = placement_count(def.id, "item", &mut loot, pool.count, floor.len() / 2)?;
        for (n, &(x, y)) in floor
            .iter()
            .skip(1)
            .step_by(2)
            .take(count as usize)
            .enumerate()
        {
            out.items.push(Item {
                equipped_slot: None,
                class: None,
                quantity: 1,
                stackable: None,
                properties: BTreeMap::new(),
                id: first_item + n as u64,
                at: [x, y, 0],
                archetype: Some(loot.pick(&pool.archetypes).clone()),
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
    let entries: Vec<Cell> = region
        .anchors
        .iter()
        .filter(|(name, _)| {
            !region
                .generate
                .as_ref()
                .is_some_and(|g| g.boundary_anchors.contains(name))
        })
        .map(|(_, [x, y, _])| (*x, *y))
        .collect();
    let Some(&start) = entries.first() else {
        return region.generate.as_ref().is_some_and(|g| g.group.is_some());
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
    fn generated_stair_anchors_are_connected_distinct_and_stream_isolated() {
        let (mut def, _) = cave();
        let mut recipe = serde_json::to_value(def.generate.as_ref().unwrap()).unwrap();
        recipe["stair_anchors"] = serde_json::json!(["up", "down"]);
        let generate: Generate = serde_json::from_value(recipe).unwrap();
        def.generate = Some(generate.clone());
        let first = materialize(&def, &generate, 42, 100, 200).unwrap();
        assert_ne!(first.anchors["up"], first.anchors["down"]);
        assert!(entries_connected(&first));
        for anchor in ["up", "down"] {
            assert!(!first.walls.contains(&first.anchors[anchor]));
            assert!(first.actors.iter().all(|a| a.at != first.anchors[anchor]));
            assert!(first.items.iter().all(|a| a.at != first.anchors[anchor]));
        }
        let mut changed = generate.clone();
        changed.actors = None;
        changed.items = None;
        let second = materialize(&def, &changed, 42, 100, 200).unwrap();
        assert_eq!(first.anchors, second.anchors);
        assert_eq!(first.walls, second.walls);
        let baseline = materialize(&def, def.generate.as_ref().unwrap(), 42, 100, 200).unwrap();
        assert_eq!(json(&first), json(&baseline));
    }

    #[test]
    fn generated_stair_names_and_capacity_are_validated() {
        let (mut def, mut generate) = cave();
        for names in [vec!["up", "up"], vec!["west"], vec!["bad/name"], vec![""]] {
            generate.stair_anchors = names.into_iter().map(String::from).collect();
            assert!(materialize(&def, &generate, 42, 100, 200).is_err());
        }
        def.size = [1, 1, 1];
        def.anchors = BTreeMap::from([("entry".into(), [0; 3])]);
        generate.stair_anchors = vec!["up".into()];
        assert!(materialize(&def, &generate, 42, 100, 200)
            .unwrap_err()
            .message
            .contains("insufficient floor"));
    }

    #[test]
    fn rooms_v2_keeps_its_generated_content_contract() {
        // Keep these inputs independent of editable scenario packages.
        let def: RegionDef = toml::from_str(
            r#"
            id = 2
            name = "Generation vector"
            zone = "vector"
            size = [24, 12, 1]
            anchors = { west = [0, 6, 0], east = [23, 6, 0] }
            [generate]
            generator = "rooms"
            version = 2
            salt = 0
            rooms = [3, 6]
            actors = { archetypes = ["rat"], ai = "wander", count = [1, 3] }
            items = { archetypes = ["coin"], count = [2, 4] }
            "#,
        )
        .unwrap();
        let generate = def.generate.as_ref().unwrap();
        assert_eq!(generate.version, 2);
        for seed in [0, 42, u64::MAX] {
            let region = materialize(&def, generate, seed, 100, 200).unwrap();
            let actors: Vec<_> = region
                .actors
                .iter()
                .map(|actor| (actor.id, actor.at, &actor.archetype, &actor.ai))
                .collect();
            let items: Vec<_> = region
                .items
                .iter()
                .map(|item| (item.id, item.at, &item.archetype))
                .collect();
            // Hash semantic outcomes, excluding author labels and DTO field
            // ordering. Changing these vectors requires a generator version decision.
            let bytes = serde_json::to_vec(&(&region.walls, actors, items)).unwrap();
            let digest: String = Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect();
            let expected = match seed {
                0 => "99628da25696e30d2a6d94fc045617362e3b9c19414f4fc93862939759a73eaa",
                42 => "ebbfc0ae1e72b68c5927b622e9a72ffbdd97d4a6ee4f702df55966b255a49833",
                u64::MAX => "1dd3271c039fec56c3723e707e2d5e6bec0f726ed89b1ad58f4e911e7df226ea",
                _ => unreachable!(),
            };
            assert_eq!(digest, expected, "rooms-v2 seed {seed}");
        }
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
    fn generator_salt_defaults_to_zero_and_explicit_salts_change_content() {
        let (def, generate) = cave();
        let mut value = serde_json::to_value(&generate).unwrap();
        value.as_object_mut().unwrap().remove("salt");
        let omitted: Generate = serde_json::from_value(value.clone()).unwrap();
        value["salt"] = serde_json::json!(0);
        let zero: Generate = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(omitted, zero);
        value["salt"] = serde_json::json!(1);
        let salted: Generate = serde_json::from_value(value).unwrap();
        assert!((0..8).any(|seed| {
            json(&materialize(&def, &zero, seed, 100, 200).unwrap())
                != json(&materialize(&def, &salted, seed, 100, 200).unwrap())
        }));
    }

    #[test]
    fn insufficient_placement_capacity_does_not_silently_violate_requested_minimum() {
        let (mut def, mut generate) = cave();
        def.size = [1, 1, 1];
        def.anchors = BTreeMap::from([("entry".into(), [0, 0, 0])]);
        def.portals.clear();
        generate.rooms = [1, 1];
        generate.actors.as_mut().unwrap().count = [1, 1];
        generate.items = None;
        let error = materialize(&def, &generate, 42, 100, 200).unwrap_err();
        assert!(error.message.contains("Region 2"), "{}", error.message);
        assert!(error.message.contains("capacity"), "{}", error.message);
    }

    #[test]
    fn actor_population_does_not_reroll_geometry_or_loot_placements() {
        let (def, generate) = cave();
        let mut sparse = generate.clone();
        sparse.actors.as_mut().unwrap().count = [0, 0];
        let mut populated = generate;
        populated.actors.as_mut().unwrap().count = [3, 3];
        for seed in [0, 1, 7, 42, u64::MAX] {
            let before = materialize(&def, &sparse, seed, 100, 200).unwrap();
            let after = materialize(&def, &populated, seed, 100, 200).unwrap();
            assert_eq!(before.walls, after.walls, "geometry, seed {seed}");
            assert_eq!(
                serde_json::to_value(&before.items).unwrap(),
                serde_json::to_value(&after.items).unwrap(),
                "loot, seed {seed}"
            );
            assert!(before.actors.is_empty());
            assert_eq!(after.actors.len(), 3);
        }
    }

    #[test]
    fn oversized_authored_pool_counts_are_rejected_without_arithmetic_overflow() {
        let (def, mut generate) = cave();
        generate.actors.as_mut().unwrap().count = [u32::MAX, u32::MAX];
        generate.items.as_mut().unwrap().count = [0, 1];
        let error = check(&def, &generate).unwrap_err();
        assert!(error.message.contains("Region 2"), "{}", error.message);
        assert!(error.message.contains("pools"), "{}", error.message);
    }

    #[test]
    fn loot_changes_do_not_reroll_geometry_or_actor_placements() {
        let (def, generate) = cave();
        let mut empty = generate.clone();
        empty.items = None;
        for seed in [0, 1, 7, 42, u64::MAX] {
            let before = materialize(&def, &generate, seed, 100, 200).unwrap();
            let after = materialize(&def, &empty, seed, 100, 200).unwrap();
            assert_eq!(before.walls, after.walls, "geometry, seed {seed}");
            assert_eq!(
                serde_json::to_value(&before.actors).unwrap(),
                serde_json::to_value(&after.actors).unwrap(),
                "population, seed {seed}"
            );
            assert!(after.items.is_empty());
        }
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
        newer.version = ROOMS_VERSION + 1;
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
