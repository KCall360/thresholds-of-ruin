use super::*;
pub(crate) fn package() -> Package {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/magic-items");
    let resolved = resolve_source(&root).unwrap();
    let rules = std::fs::read_to_string(root.join(&resolved.manifest.ruleset)).unwrap();
    let mut manifest = serde_json::to_value(resolved.manifest).unwrap();
    manifest["spatial_scopes"] = serde_json::json!({"study":{"members":[1],"rooms":{"1":[{"region":1,"min":[0,0,0],"max":[6,4,1]}]},"passages":[],"doorways":[]}});
    let region:RegionDef=serde_json::from_value(serde_json::json!({"id":1,"name":"Source audit","size":[7,5,2],"chamber":true,"anchors":{"start":[1,2,0]}})).unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![region],
        &rules,
    )
    .unwrap();
    package.check().unwrap();
    package
}
#[test]
fn canonical_authored_install_and_source_audit_reject_whole_scope_omission() {
    let package = package();
    let game = package.build(42, true).unwrap();
    assert_eq!(game.spatial_registry().scopes().count(), 1);
    let mut audit = SpatialAudit::new(Some(&package), 42).unwrap();
    audit.check(&game).unwrap();
    // Strip only the canonical registry via the real core checkpoint pool,
    // leaving actual structural members unchanged. Intrinsic empty pool remains
    // valid; independent server expected-set authority must reject omission.
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut encoded = serde_json::to_value(&shared).unwrap();
    encoded["spatial"]["scopes"] = serde_json::json!([]);
    encoded["spatial"]["directories"] = serde_json::json!([[]]);
    let mut root = serde_json::to_value(&snapshot).unwrap();
    root["spatial"] = serde_json::json!(0);
    let shared = serde_json::from_value(encoded).unwrap();
    let snapshot = serde_json::from_value(root).unwrap();
    let missing = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(
        missing.loaded_regions().collect::<Vec<_>>(),
        game.loaded_regions().collect::<Vec<_>>()
    );
    assert!(missing.spatial_registry().scopes().next().is_none());
    assert_eq!(
        audit.check(&missing).unwrap_err().code,
        tor_protocol::ErrorCode::InvalidArchive
    );
}
#[test]
fn canonical_authored_source_rejects_missing_primary_and_foreign_volume_before_install() {
    let package = package();
    let regions = package.region_defs().unwrap();
    let rules = package.rules_source.clone();
    for foreign in [false, true] {
        let mut manifest = package.manifest.clone();
        let scope = manifest.spatial_scopes.get_mut("study").unwrap();
        if foreign {
            scope.rooms.get_mut(&1).unwrap()[0].region = 2;
        } else {
            scope.rooms.get_mut(&1).unwrap()[0].max[0] = 5;
        }
        let invalid = Package::from_parts(manifest, regions.clone(), &rules).unwrap();
        assert!(invalid.check().is_err());
    }
}

#[test]
fn canonical_scope_for_unbuilt_future_members_is_not_expected() {
    let package = package();
    let mut game = package.start(42).unwrap();
    assert!(game.loaded_regions().next().is_none());
    assert!(game.detached_records().next().is_none());
    assert!(game.spatial_registry().scopes().next().is_none());
    let mut audit = SpatialAudit::new(Some(&package), 42).unwrap();
    audit.check(&game).unwrap();
    let scope = package
        .compile_authored_spatial("study", 42, &mut 0, &mut 0)
        .unwrap();
    // Intrinsic validity and exact source equality are not sufficient: this
    // root has not prepared any member and must not carry a future directory.
    game.register_spatial_scope(scope, &budget().unwrap())
        .unwrap();
    let error = audit.check(&game).unwrap_err();
    assert_eq!(error.code, tor_protocol::ErrorCode::InvalidArchive);
    assert_eq!(
        error.message,
        "Spatial registry differs from canonical source"
    );
}

#[test]
fn canonical_authored_surface_uses_actual_directed_portal_destination() {
    let base = package();
    let mut manifest = base.manifest.clone();
    let mut regions = base.region_defs().unwrap();
    let mut first = serde_json::to_value(&regions[0]).unwrap();
    first["portals"] = serde_json::json!([{"at":[6,2,0],"direction":"east","to":"2/entry","kind":"portal","turns":1}]);
    regions[0] = serde_json::from_value(first).unwrap();
    regions.push(serde_json::from_value(serde_json::json!({"id":2,"name":"Physical neighbor","size":[3,3,2],"chamber":true,"anchors":{"entry":[1,1,0]}})).unwrap());
    // A distinct explicit scope remains a distinct scope; physical geometry is
    // still authoritative for the open mouth's illumination face.
    manifest.spatial_scopes.insert("neighbor".into(),serde_json::from_value(serde_json::json!({"members":[2],"rooms":{"1":[{"region":2,"min":[0,0,0],"max":[2,2,1]}]},"passages":[],"doorways":[]})).unwrap());
    let package = Package::from_parts(manifest, regions, &base.rules_source).unwrap();
    package.check().unwrap();
    let index = package.index(42).unwrap();
    let first = index.region(&package, 1).unwrap();
    let world = package.spatial_source_world(42, &[&first], &mut 0).unwrap();
    let mouth = loc(1, [6, 2, 0]);
    let destination = loc(2, [1, 1, 0]);
    assert_eq!(
        world.physics_neighbor(mouth, Direction::East).unwrap().0,
        destination
    );
    let scope = package
        .compile_authored_spatial("study", 42, &mut 0, &mut 0)
        .unwrap();
    assert_eq!(scope.members, vec![RegionId(1)]);
    assert!(scope.rooms[0].runs.iter().any(|r| r.contains(mouth)));
    assert!(!scope.rooms[0]
        .runs
        .iter()
        .any(|r| r.contains(loc(1, [7, 2, 0]))));
    assert!(!scope.rooms[0].runs.iter().any(|r| r.contains(destination)));
}

#[test]
fn canonical_foreign_solid_face_is_surface_only_and_source_certified() {
    let base = package();
    let mut manifest = base.manifest.clone();
    let mut regions = base.region_defs().unwrap();
    let mut first = serde_json::to_value(&regions[0]).unwrap();
    first["portals"] = serde_json::json!([{"at":[6,1,1],"direction":"east","to":"2/entry","kind":"boundary","width":2,"turns":1}]);
    regions[0] = serde_json::from_value(first).unwrap();
    regions.push(serde_json::from_value(serde_json::json!({"id":2,"name":"Solid neighbor","size":[3,3,2],"chamber":true,"walls":[[1,1,1]],"anchors":{"entry":[2,1,1]}})).unwrap());
    let passages:Vec<_>=(0..2).flat_map(|z|(0..3).flat_map(move |y|{
        let spans=if z==1&&y==1 {vec![(0,0),(2,2)]} else {vec![(0,2)]};
        spans.into_iter().map(move |(a,b)|serde_json::json!({"region":2,"z":z,"y":y,"first_x":a,"last_x":b}))
    })).collect();
    manifest.spatial_scopes.insert(
        "neighbor".into(),
        serde_json::from_value(
            serde_json::json!({"members":[2],"rooms":{},"passages":passages,"doorways":[]}),
        )
        .unwrap(),
    );
    let package = Package::from_parts(manifest, regions, &base.rules_source).unwrap();
    package.check().unwrap();
    let scope = package
        .compile_authored_spatial("study", 42, &mut 0, &mut 0)
        .unwrap();
    let face = loc(2, [1, 1, 1]);
    let open = loc(2, [0, 1, 1]);
    let index = package.index(42).unwrap();
    let first = index.region(&package, 1).unwrap();
    let world = package.spatial_source_world(42, &[&first], &mut 0).unwrap();
    assert_eq!(
        world
            .physics_neighbor(loc(1, [6, 2, 1]), Direction::East)
            .unwrap()
            .0,
        face
    );
    assert!(world.is_wall(face));
    assert!(!world.is_wall(open));
    assert_eq!(scope.members, vec![RegionId(1)]);
    assert!(scope.primary.iter().all(|r| r.cells.region == RegionId(1)));
    assert!(scope.rooms[0].runs.iter().any(|r| r.contains(face)));
    assert!(!scope.rooms[0].runs.iter().any(|r| r.contains(open)));
    let game = package.build(42, true).unwrap();
    assert_eq!(game.spatial_registry().lookup("study", face), None);
    assert_eq!(game.spatial_registry().lookup("study", open), None);
    assert!(game.spatial_registry().lookup("neighbor", open).is_some());
    SpatialAudit::new(Some(&package), 42)
        .unwrap()
        .check(&game)
        .unwrap();
    for foreign_region in [2, 999] {
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let root = game.checkpoint(&mut shared);
        let mut encoded = serde_json::to_value(&shared).unwrap();
        let study = encoded["spatial"]["scopes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["id"] == "study")
            .unwrap();
        let row = study["rooms"][0]["runs"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|r| r["region"] == 2)
            .unwrap();
        row["region"] = serde_json::json!(foreign_region);
        row["first_x"] = serde_json::json!(0);
        row["last_x"] = serde_json::json!(0);
        let shared = serde_json::from_value(encoded).unwrap();
        let changed = Game::restore_checkpoint(root, &shared, game.content().clone()).unwrap();
        let error = SpatialAudit::new(Some(&package), 42)
            .unwrap()
            .check(&changed)
            .unwrap_err();
        assert_eq!(error.code, tor_protocol::ErrorCode::InvalidArchive);
        assert_eq!(
            error.message,
            "Spatial registry differs from canonical source"
        );
    }
}
