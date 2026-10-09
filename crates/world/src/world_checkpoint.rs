//! Compact backend world snapshots: opening a door must not duplicate topology.
use super::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Instance {
    geometry: usize,
    #[serde(with = "crate::checkpoint_map")]
    doors: BTreeMap<Location, Door>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Worlds {
    geometry: Vec<World>,
    instances: Vec<Instance>,
}

fn same_geometry(a: &World, b: &World) -> bool {
    // Exhaustive destructuring makes adding world state require an explicit
    // checkpoint sharing decision, rather than silently coalescing distinct worlds.
    let World {
        doors: _,
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
        sight: _,
    } = a;
    regions == &b.regions
        && passages == &b.passages
        && rotations == &b.rotations
        && physical_vertical == &b.physical_vertical
        && solid_boundaries == &b.solid_boundaries
        && named_stairs == &b.named_stairs
        && named_anchors == &b.named_anchors
        && region_gravity == &b.region_gravity
        && cell_gravity == &b.cell_gravity
        && terrain == &b.terrain
        && chambers == &b.chambers
        && place_hints == &b.place_hints
        && absent == &b.absent
}

pub fn serialize<S: Serializer>(worlds: &[World], serializer: S) -> Result<S::Ok, S::Error> {
    if worlds.len() > 129 {
        return Err(serde::ser::Error::custom("too many checkpoint worlds"));
    }
    let mut geometry = Vec::new();
    let instances = worlds
        .iter()
        .map(|world| {
            let index = geometry
                .iter()
                .position(|base| same_geometry(base, world))
                .unwrap_or_else(|| {
                    let mut base = world.clone();
                    base.doors.clear();
                    base.sight.topology_changed();
                    geometry.push(base);
                    geometry.len() - 1
                });
            Instance {
                geometry: index,
                doors: (*world.doors).clone(),
            }
        })
        .collect();
    Worlds {
        geometry,
        instances,
    }
    .serialize(serializer)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<World>, D::Error> {
    let saved = Worlds::deserialize(deserializer)?;
    // Current state plus at most 128 rewind boundaries. Check before expanding
    // references so a small instance list cannot request unbounded world clones.
    if saved.instances.len() > 129
        || saved.geometry.len() > 129
        || saved.geometry.iter().any(|world| !world.doors.is_empty())
    {
        return Err(serde::de::Error::custom(
            "checkpoint geometry contains dynamic doors",
        ));
    }
    saved
        .instances
        .into_iter()
        .map(|instance| {
            let mut world = saved
                .geometry
                .get(instance.geometry)
                .cloned()
                .ok_or_else(|| serde::de::Error::custom("unknown checkpoint geometry"))?;
            // Instances of one geometry differ in their doors, so each needs
            // its own sight versions.
            world.doors = Shared::new(instance.doors);
            world.sight.topology_changed();
            Ok(world)
        })
        .collect()
}
