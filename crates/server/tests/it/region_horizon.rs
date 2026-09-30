use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;
use tor_server::{region_streaming::RegionCatalog, scenario_package};
use tor_world::RegionId;

fn ids(values: &[u64]) -> BTreeSet<RegionId> {
    values.iter().copied().map(RegionId).collect()
}

/// The package's region index, to change its structure.
fn index(package: &mut scenario_package::Package) -> &mut Vec<scenario_package::IndexedRegion> {
    &mut std::sync::Arc::make_mut(&mut package.index).regions
}

fn dungeon_package() -> scenario_package::Package {
    scenario_package::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/first-dungeon"),
        42,
        None,
        false,
    )
    .unwrap()
    .package
    .unwrap()
    .as_ref()
    .clone()
}

#[test]
fn horizon_uses_structure_without_constructing_entities() {
    let mut package = dungeon_package();
    // Invalid entity state would fail Game construction, but is irrelevant to
    // this structural query. No generation, RNG, or entity inspection is needed.
    package.manifest.characters[0].turn_ticks = 0;
    let catalog = RegionCatalog::from_package(&package).unwrap();
    let plan = catalog.plan(&ids(&[1]), 0, &ids(&[2])).unwrap();
    assert_eq!(plan.required, ids(&[1]));
    assert_eq!(plan.activate, ids(&[1]));
    assert_eq!(plan.deactivate, ids(&[2]));
    assert_eq!(plan.expanded_regions, 0);
    assert_eq!(plan.examined_links, 0);
}

#[test]
fn directed_cycles_multiple_roots_and_repeated_planning_are_deterministic() {
    let mut package = dungeon_package();
    for region in index(&mut package) {
        region.portals.clear();
        region.anchors.insert("entry".into(), [1, 1, 0]);
    }
    let portal = |target: u64| format!("{target}/entry");
    for (source, target) in [(1, 2), (2, 3), (3, 1), (4, 5)] {
        index(&mut package)
            .iter_mut()
            .find(|r| r.id == source)
            .unwrap()
            .portals
            .push(portal(target));
    }
    let catalog = RegionCatalog::from_package(&package).unwrap();
    let first = catalog.plan(&ids(&[1, 4]), 1, &ids(&[1, 3])).unwrap();
    assert_eq!(first.required, ids(&[1, 2, 4, 5]));
    assert_eq!(first.activate, ids(&[2, 4, 5]));
    assert_eq!(first.deactivate, ids(&[3]));
    assert_eq!(first.expanded_regions, 2);
    assert_eq!(first.examined_links, 2);
    assert_eq!(
        catalog.plan(&ids(&[5]), 10, &ids(&[])).unwrap().required,
        ids(&[5])
    );
    assert_eq!(
        catalog
            .plan(&ids(&[1]), usize::MAX, &ids(&[]))
            .unwrap()
            .required,
        ids(&[1, 2, 3])
    );
    let repeat = catalog.plan(&ids(&[1, 4]), 1, &first.required).unwrap();
    assert!(repeat.activate.is_empty() && repeat.deactivate.is_empty());
    index(&mut package).reverse();
    let reordered = RegionCatalog::from_package(&package).unwrap();
    assert_eq!(catalog, reordered);
    assert_eq!(
        first,
        reordered.plan(&ids(&[1, 4]), 1, &ids(&[1, 3])).unwrap()
    );
}

#[test]
fn invalid_metadata_and_unknown_roots_fail_without_a_partial_plan() {
    let mut package = dungeon_package();
    let catalog = RegionCatalog::from_package(&package).unwrap();
    assert!(catalog.plan(&ids(&[999]), 1, &ids(&[])).is_err());
    assert!(catalog.plan(&ids(&[1]), 1, &ids(&[999])).is_err());
    assert!(catalog.plan(&ids(&[]), 1, &ids(&[1])).is_err());
    index(&mut package)[0].portals[0] = "999/missing".into();
    assert!(RegionCatalog::from_package(&package).is_err());
    let mut package = dungeon_package();
    let first = package.index.regions[0].clone();
    index(&mut package).push(first);
    assert!(RegionCatalog::from_package(&package).is_err());
}

#[test]
fn anchor_resolution_and_zone_pools_do_not_need_loaded_regions() {
    let mut package = dungeon_package();
    package.manifest.zones.insert(
        "empty".into(),
        scenario_package::Zone {
            themes: Some(vec![]),
        },
    );
    package
        .manifest
        .zones
        .insert("inherit".into(), scenario_package::Zone { themes: None });
    package.manifest.zones.insert(
        "replace".into(),
        scenario_package::Zone {
            themes: Some(vec!["ice".into(), "ice".into()]),
        },
    );
    index(&mut package)[0].zone = Some("empty".into());
    index(&mut package)[1].zone = Some("inherit".into());
    index(&mut package)[2].zone = Some("replace".into());
    let catalog = RegionCatalog::from_package(&package).unwrap();
    assert!(catalog.region(RegionId(1)).unwrap().themes.is_empty());
    assert_eq!(
        catalog.region(RegionId(2)).unwrap().themes,
        package.manifest.themes.iter().cloned().collect()
    );
    assert_eq!(
        catalog.region(RegionId(3)).unwrap().themes,
        BTreeSet::from(["ice".into()])
    );
    let anchor = catalog.resolve_anchor("5/west").unwrap();
    assert_eq!(anchor.region, RegionId(5));
    assert_eq!(anchor.position, tor_world::Position { x: 0, y: 2, z: 0 });
    assert!(catalog.resolve_anchor("05/west").is_err());
    assert!(catalog.resolve_anchor("5/missing").is_err());
    index(&mut package)[0].zone = Some("missing".into());
    assert!(RegionCatalog::from_package(&package).is_err());
    index(&mut package)[0].zone = None;
    index(&mut package)[0]
        .anchors
        .insert("outside".into(), [999, 0, 0]);
    assert!(RegionCatalog::from_package(&package).is_err());
}

#[test]
fn local_horizon_work_does_not_expand_with_unrelated_regions() {
    let mut package = dungeon_package();
    let small = RegionCatalog::from_package(&package)
        .unwrap()
        .plan(&ids(&[1]), 1, &ids(&[]))
        .unwrap();
    let mut unrelated = package.index.regions[4].clone();
    unrelated.portals.clear();
    for id in 6..=8192 {
        unrelated.id = id;
        index(&mut package).push(unrelated.clone());
    }
    let large = RegionCatalog::from_package(&package)
        .unwrap()
        .plan(&ids(&[1]), 1, &ids(&[]))
        .unwrap();
    assert_eq!(small, large);
    assert_eq!(large.required, ids(&[1, 2]));
    assert_eq!(large.expanded_regions, 1);
    assert_eq!(large.examined_links, 1);
}

#[test]
fn every_authored_package_has_usable_structural_metadata() {
    let base = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios");
    let mut paths = vec![base.join("two-room"), base.join("first-dungeon")];
    paths.extend(
        std::fs::read_dir(base.join("tests"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.is_dir()),
    );
    for path in paths {
        let scenario = scenario_package::load(&path, 42, None, false).unwrap();
        let package = scenario.package.unwrap();
        let catalog = RegionCatalog::from_package(&package)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        for character in &package.manifest.characters {
            let start = catalog.resolve_anchor(&character.anchor).unwrap();
            assert!(catalog
                .plan(&ids(&[start.region.0]), 1, &ids(&[]))
                .unwrap()
                .required
                .contains(&start.region));
        }
    }
}

#[test]
fn real_utility_reports_horizon_and_errors_as_json() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/first-dungeon");
    let output = Command::new(env!("CARGO_BIN_EXE_tor-scenario"))
        .arg("horizon")
        .arg(&root)
        .args(["1", "0"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["required"], serde_json::json!([1]));
    assert_eq!(report["activate"], serde_json::json!([1]));
    let output = Command::new(env!("CARGO_BIN_EXE_tor-scenario"))
        .arg("horizon")
        .arg(root)
        .args(["999", "1"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(report["error"]["code"], "scenario_invalid");
}
