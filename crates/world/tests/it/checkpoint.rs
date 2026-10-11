//! Current world-pool and ordered-map readers reject independently malformed records.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tor_world::{Extent, Location, Position, Region, RegionId, Shared, World};

#[derive(Debug, Serialize, Deserialize)]
struct States {
    #[serde(with = "tor_world::checkpoint_worlds")]
    worlds: Vec<World>,
}
fn world() -> World {
    World::new(
        vec![Region {
            id: RegionId(1),
            name: "pool boundary".into(),
            bounds: Extent::new(3, 1, 1).unwrap(),
        }],
        vec![],
    )
    .unwrap()
}
fn saved() -> Value {
    serde_json::to_value(States {
        worlds: vec![world()],
    })
    .unwrap()
}
fn rejects(value: Value, label: &str) {
    let encoded = serde_json::to_vec(&value).unwrap();
    assert!(
        serde_json::from_slice::<States>(&encoded).is_err(),
        "{label}"
    );
}

#[test]
fn world_pool_writer_and_reader_accept_empty_one_and_129_but_reject_130() {
    for count in [0, 1, 129] {
        let states = States {
            worlds: vec![world(); count],
        };
        let encoded = serde_json::to_vec(&states).unwrap();
        let restored: States = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(restored.worlds, states.worlds, "count {count}");
    }
    assert!(serde_json::to_vec(&States {
        worlds: vec![world(); 130]
    })
    .is_err());
    for pool in ["geometry", "instances"] {
        for count in [129, 130] {
            let mut value = saved();
            let entry = value["worlds"][pool][0].clone();
            value["worlds"][pool] = Value::Array(vec![entry; count]);
            let decoded = serde_json::from_slice::<States>(&serde_json::to_vec(&value).unwrap());
            if count == 129 {
                let restored = decoded.unwrap();
                assert_eq!(
                    restored.worlds,
                    vec![world(); if pool == "instances" { 129 } else { 1 }]
                );
            } else {
                assert!(
                    decoded.is_err(),
                    "{pool} must independently enforce its bound"
                );
            }
        }
    }
}

#[test]
fn world_pool_rejects_missing_and_invalid_geometry_references_and_unknown_fields() {
    for index in [1, usize::MAX] {
        let mut value = saved();
        value["worlds"]["instances"][0]["geometry"] = json!(index);
        rejects(value, "out-of-range geometry");
    }
    let mut absent = saved();
    absent["worlds"]["instances"][0]
        .as_object_mut()
        .unwrap()
        .remove("geometry");
    rejects(absent, "missing geometry reference");
    let mut empty = saved();
    empty["worlds"]["geometry"] = json!([]);
    rejects(empty, "reference to empty pool");
    for target in ["pool", "instance", "geometry"] {
        let mut value = saved();
        let object = match target {
            "pool" => &mut value["worlds"],
            "instance" => &mut value["worlds"]["instances"][0],
            _ => &mut value["worlds"]["geometry"][0],
        };
        object["unrecognized"] = json!(true);
        rejects(value, target);
    }
}

#[test]
fn dynamic_doors_belong_to_instances_and_never_shared_geometry() {
    let mut with_door = world();
    with_door
        .place_door(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 0, z: 0 },
            },
            7,
            false,
            1,
        )
        .unwrap();
    let states = States {
        worlds: vec![with_door.clone()],
    };
    let mut value = serde_json::to_value(&states).unwrap();
    assert_eq!(value["worlds"]["geometry"][0]["doors"], json!([]));
    assert_eq!(
        serde_json::from_value::<States>(value.clone())
            .unwrap()
            .worlds,
        vec![with_door]
    );
    value["worlds"]["geometry"][0]["doors"] = value["worlds"]["instances"][0]["doors"].clone();
    rejects(value, "dynamic door embedded in shared geometry");
}

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Maps {
    #[serde(with = "tor_world::checkpoint_map")]
    ordinary: BTreeMap<(u64, u64), u64>,
    #[serde(with = "tor_world::checkpoint_map::shared")]
    shared: Shared<BTreeMap<(u64, u64), u64>>,
}
#[test]
fn ordinary_and_shared_ordered_maps_reject_duplicate_and_descending_keys() {
    let map = BTreeMap::from([((1, 2), 3), ((1, 4), 5)]);
    let maps = Maps {
        ordinary: map.clone(),
        shared: Shared::new(map),
    };
    let valid = serde_json::to_value(&maps).unwrap();
    assert_eq!(serde_json::from_value::<Maps>(valid.clone()).unwrap(), maps);
    for field in ["ordinary", "shared"] {
        for entries in [
            json!([[[1, 2], 3], [[1, 2], 5]]),
            json!([[[1, 4], 5], [[1, 2], 3]]),
        ] {
            let mut value = valid.clone();
            value[field] = entries;
            assert!(
                serde_json::from_slice::<Maps>(&serde_json::to_vec(&value).unwrap()).is_err(),
                "{field}"
            );
        }
    }
}
