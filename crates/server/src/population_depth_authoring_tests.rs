//! Newly reconstructed weighted-depth authoring contracts, not recovered tests.
use super::*;
use serde_json::{json, Value};

fn weighted_source() -> Value {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
    let mut source = serde_json::to_value(resolve_source(&root).unwrap().manifest).unwrap();
    source["archetypes"] = json!({"marker":{"name":"marker"},"alternate":{"name":"alternate"}});
    source["floor_activation"] = json!({"version":1,"protagonist":"selected_character",
    "families":{"dungeon":"plain"},"profiles":{"plain":{"version":1,"stages":[
    {"kind":"items","id":"weighted","version":1,"count":[1,1],"bands":[
        {"depth":{"kind":"finite","minimum":1,"maximum":2},"choices":[
            {"archetype":"marker","weight":1},{"archetype":"alternate","weight":3}]},
        {"depth":{"kind":"open","minimum":4},"choices":[{"archetype":"alternate","weight":1}]}
    ]}]}}});
    source
}

#[test]
fn weighted_depth_authoring_admits_finite_retirement_gap_and_open_tail() {
    let manifest = serde_json::from_value(weighted_source())
        .expect("first red: bounded typed weighted depth bands must be admitted");
    let package = Package::from_parts(
        manifest,
        vec![],
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    package.check().unwrap();
    let first = package.index(42).unwrap();
    let second = package.index(42).unwrap();
    assert_eq!(
        package
            .build_group(42, &first, RegionId(1))
            .unwrap()
            .records,
        package
            .build_group(42, &second, RegionId(1))
            .unwrap()
            .records
    );
}

#[test]
fn weighted_depth_authoring_rejects_invalid_intervals_weights_and_unknown_operands() {
    for case in 0..12 {
        let mut source = weighted_source();
        let stage = &mut source["floor_activation"]["profiles"]["plain"]["stages"][0];
        match case {
            0 => stage["bands"][0]["depth"]["minimum"] = json!(0),
            1 => stage["bands"][0]["depth"]["maximum"] = json!(0),
            2 => stage["bands"][1]["depth"]["minimum"] = json!(2),
            3 => stage["bands"][0]["depth"] = json!({"kind":"open","minimum":1}),
            4 => stage["bands"][0]["choices"][0]["weight"] = json!(0),
            5 => stage["bands"][0]["choices"][0]["weight"] = json!(u32::MAX),
            6 => stage["bands"][0]["choices"][1]["archetype"] = json!("marker"),
            7 => stage["bands"][0]["choices"][1]["archetype"] = json!("missing"),
            8 => stage["bands"] = json!([]),
            9 => stage["bands"][0]["choices"] = json!([]),
            10 => stage["bands"][0]["choices"][0]["expression"] = json!("depth * 2"),
            11 => stage["minimum_depth"] = json!(1),
            _ => unreachable!(),
        }
        match serde_json::from_value(source) {
            Err(_) => {}
            Ok(manifest) => {
                let package = Package::from_parts(
                    manifest,
                    vec![],
                    tor_simulation::rules::DEFAULT_RULESET_TOML,
                );
                assert!(
                    package.is_err() || package.unwrap().check().is_err(),
                    "case {case}"
                );
            }
        }
    }
}
