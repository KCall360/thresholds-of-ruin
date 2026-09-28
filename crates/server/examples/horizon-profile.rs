//! Versioned structural-query workload; timings exclude package parsing and I/O.
use std::collections::BTreeSet;
use std::hint::black_box;
use std::path::Path;
use std::time::Instant;
use tor_server::{region_streaming::RegionCatalog, scenario_package};
use tor_world::RegionId;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scenario = scenario_package::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/first-dungeon"),
        42,
        None,
        false,
    )?;
    let original = scenario.package.ok_or("Missing package")?;
    for count in [5, 256, 8192] {
        let mut package = (*original).clone();
        let mut unrelated = package.regions[4].clone();
        unrelated.portals.clear();
        unrelated.items.clear();
        unrelated.actors.clear();
        for id in 6..=count {
            unrelated.id = id;
            package.regions.push(unrelated.clone());
        }
        let started = Instant::now();
        let catalog = RegionCatalog::from_package(&package)?;
        let catalog_ns = started.elapsed().as_nanos();
        let roots = BTreeSet::from([RegionId(1)]);
        let active = BTreeSet::from([RegionId(1), RegionId(5)]);
        let expected = catalog.plan(&roots, 2, &active)?;
        for _ in 0..100 {
            black_box(catalog.plan(black_box(&roots), 2, black_box(&active))?);
        }
        let mut timings = Vec::with_capacity(10_000);
        for _ in 0..10_000 {
            let started = Instant::now();
            let result = black_box(catalog.plan(black_box(&roots), 2, black_box(&active))?);
            timings.push(started.elapsed().as_nanos());
            assert_eq!(result, expected);
        }
        timings.sort_unstable();
        println!(
            "{}",
            serde_json::json!({
                "workload": "structural-horizon-v1", "regions": count,
                "samples": timings.len(), "hops": 2, "catalog_build_ns": catalog_ns,
                "p50_ns": timings[4999], "p95_ns": timings[9499], "max_ns": timings[9999],
                "expanded_regions": expected.expanded_regions,
                "examined_links": expected.examined_links, "required": expected.required,
            })
        );
    }
    Ok(())
}
