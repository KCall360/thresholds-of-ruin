// STAGED ONLY: register as a cfg(test) child of scenario_spatial_runtime.rs.
// These are package/native checkpoint witnesses, not Service/process or paid
// Mapping acceptance. No runtime result, deleted-directory proof, or detached
// history qualification is claimed merely by staging these tests.
use super::*;
use serde_json::{json, Value};
use tor_simulation::spatial_scope::mapping::{
    self, FaceBudget, MappingCertification, MappingUnavailable,
};

fn mapping_package(up: Option<&str>) -> Package {
    let base = super::tests::package();
    let mut manifest = serde_json::to_value(&base.manifest).unwrap();
    if let Some(up) = up {
        manifest["spatial_scopes"]["study"]["mapping"] = json!({"room_frames":[
            {"room":1,"region":1,"up":up}
        ]});
    }
    let parsed = serde_json::from_value(manifest);
    assert!(
        parsed.is_ok(),
        "valid supplied mapping metadata rejected: {parsed:?}"
    );
    let package = Package::from_parts(
        parsed.unwrap(),
        base.region_defs().unwrap(),
        &base.rules_source,
    )
    .unwrap();
    package
        .check()
        .expect("positive package must validate before any mutation");
    package
}
fn mapping_scope(package: &Package) -> Scope {
    package
        .compile_authored_spatial("study", 42, &mut 0, &mut 0)
        .unwrap()
}
fn mapping_world(package: &Package) -> World {
    let index = package.index(42).unwrap();
    let first = index.region(package, 1).unwrap();
    package.spatial_source_world(42, &[&first], &mut 0).unwrap()
}
fn prepared_faces(package: &Package, scope: &Scope) -> Vec<(RoomId, Location, Direction)> {
    mapping::wall_faces(
        &mapping_world(package),
        scope,
        FaceBudget {
            work: 1_000_000,
            allocation_bytes: 64 * 1024 * 1024,
            faces: 100_000,
        },
    )
    .unwrap()
    .into_iter()
    .map(|f| (f.room, f.location, f.normal))
    .collect()
}

#[test]
fn ordinary_uncertified_package_remains_admissible_but_preparation_is_explicitly_unavailable() {
    let package = mapping_package(None);
    let scope = mapping_scope(&package);
    assert!(matches!(scope.mapping, MappingCertification::Uncertified));
    assert!(matches!(
        mapping::wall_faces(
            &mapping_world(&package),
            &scope,
            FaceBudget {
                work: 1_000_000,
                allocation_bytes: 64 * 1024 * 1024,
                faces: 100_000
            }
        ),
        Err(MappingUnavailable::Uncertified)
    ));
    let game = package.build(42, true).unwrap();
    SpatialAudit::new(Some(&package), 42)
        .unwrap()
        .check(&game)
        .unwrap();
    let (_saved_inputs, reader) = super::tests::retained_authored_inputs(&package);
    let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    let saved = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    assert!(saved.directory.is_none());
    super::tests::attach_retained_authored_inputs(&saved, reader);
    assert_eq!(scope, mapping_scope(&saved));
}

#[test]
fn genuine_source_free_package_and_core_checkpoint_preserve_frames_and_prepared_oriented_faces() {
    let package = mapping_package(Some("east"));
    let source = mapping_scope(&package);
    let before = prepared_faces(&package, &source);
    assert!(
        before.contains(&(RoomId::new(1).unwrap(), loc(1, [1, 2, -1]), Direction::Up)),
        "local-z floor is a lateral wall under explicit East construction"
    );
    assert!(
        !before.iter().any(|(_, p, _)| *p == loc(1, [-1, 2, 0])),
        "local-west is construction floor and must not be a mapping wall"
    );
    let game = package.build(42, true).unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let root = game.checkpoint(&mut shared);
    let shared_bytes = serde_json::to_vec(&shared).unwrap();
    let root_bytes = serde_json::to_vec(&root).unwrap();
    let (_saved_inputs, reader) = super::tests::retained_authored_inputs(&package);
    let content_bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    drop(package);
    let restored =
        crate::storage::schema::scenario::decode_content_package(&content_bytes).unwrap();
    assert!(
        restored.directory.is_none(),
        "decoder must use retained source, not a path fallback"
    );
    super::tests::attach_retained_authored_inputs(&restored, reader);
    let restored_scope = mapping_scope(&restored);
    assert_eq!(source, restored_scope);
    assert_eq!(prepared_faces(&restored, &restored_scope), before);
    let restored_game = Game::restore_checkpoint(
        serde_json::from_slice(&root_bytes).unwrap(),
        &serde_json::from_slice(&shared_bytes).unwrap(),
        game.content().clone(),
    )
    .unwrap();
    assert_eq!(
        restored_game
            .spatial_registry()
            .scopes()
            .collect::<Vec<_>>(),
        game.spatial_registry().scopes().collect::<Vec<_>>()
    );
    SpatialAudit::new(Some(&restored), 42)
        .unwrap()
        .check(&restored_game)
        .unwrap();
    // A separately valid complete orientation is not the retained source claim.
    // Rebuilding each package avoids a stale source hash as the rejection cause.
    let contradictory_source = mapping_package(Some("north"));
    assert_ne!(mapping_scope(&contradictory_source), restored_scope);
    let error = SpatialAudit::new(Some(&contradictory_source), 42)
        .unwrap()
        .check(&restored_game)
        .unwrap_err();
    assert_eq!(error.code, tor_protocol::ErrorCode::InvalidArchive);
    assert_eq!(
        error.message,
        "Spatial registry differs from canonical source"
    );
}

#[test]
fn source_compiler_derives_only_exterior_owned_doorway_subset_not_every_doorway() {
    let base = super::tests::package();
    let mut manifest = serde_json::to_value(&base.manifest).unwrap();
    manifest["spatial_scopes"]["study"] = json!({
        "members":[1], "rooms":{"1":[{"region":1,"min":[0,0,0],"max":[1,0,1]}]},
        "passages":[{"region":1,"z":0,"y":0,"first_x":3,"last_x":4},
                    {"region":1,"z":1,"y":0,"first_x":3,"last_x":4}],
        "doorways":[{"room":1,"region":1,"z":0,"y":0,"first_x":1,"last_x":2},
                    {"room":1,"region":1,"z":1,"y":0,"first_x":1,"last_x":2}],
        "mapping":{"room_frames":[{"room":1,"region":1,"up":"up"}]}
    });
    let definition: RegionDef = serde_json::from_value(json!({"id":1,"name":"Doorway basis",
        "size":[5,1,2],"chamber":true,"anchors":{"start":[0,0,0]}}))
    .unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![definition],
        &base.rules_source,
    )
    .unwrap();
    package.check().unwrap();
    let scope = mapping_scope(&package);
    let MappingCertification::Certified(frames) = &scope.mapping else {
        panic!("certificate unavailable")
    };
    assert_eq!(frames.exterior_doorways.len(), 2);
    for z in 0..2 {
        let row = &frames.exterior_doorways[z as usize];
        assert_eq!(
            (row.region, row.z, row.y, row.first_x, row.last_x),
            (RegionId(1), z, 0, 2, 2)
        );
        assert!(scope
            .primary
            .iter()
            .any(|row| row.cells.contains(loc(1, [1, 0, z]))));
        assert!(scope
            .primary
            .iter()
            .any(|row| row.cells.contains(loc(1, [2, 0, z]))));
        let actual = prepared_faces(&package, &scope);
        assert!(actual.contains(&(
            RoomId::new(1).unwrap(),
            loc(1, [1, -1, z]),
            Direction::South
        )));
        assert!(!actual.contains(&(
            RoomId::new(1).unwrap(),
            loc(1, [2, -1, z]),
            Direction::South
        )));
    }
}

#[test]
fn sixty_five_authored_members_are_not_rejected_or_silently_downgraded_by_certificate_cap() {
    let base = super::tests::package();
    let mut manifest = serde_json::to_value(&base.manifest).unwrap();
    let volumes: Vec<Value> = (1..=65)
        .map(|region| {
            json!({"region":region,
        "min":[0,0,0],"max":[0,0,1]})
        })
        .collect();
    // Reverse author order deliberately; source compilation, not the pure typed
    // certifier, owns canonical sorting of user input.
    let frames: Vec<Value> = (1..=65)
        .rev()
        .map(|region| json!({"room":1,"region":region,"up":"up"}))
        .collect();
    manifest["spatial_scopes"]["study"] = json!({"members":(1..=65).collect::<Vec<_>>(),
        "rooms":{"1":volumes},"passages":[],"doorways":[],"mapping":{"room_frames":frames}});
    let regions: Vec<RegionDef> = (1..=65)
        .map(|id| {
            serde_json::from_value(json!({
        "id":id,"name":format!("Member {id}"),"size":[1,1,2],"chamber":true,
        "anchors":{"start":[0,0,0]}}))
            .unwrap()
        })
        .collect();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        regions,
        &base.rules_source,
    )
    .unwrap();
    package.check().unwrap();
    let scope = mapping_scope(&package);
    assert_eq!(scope.members.len(), 65);
    let MappingCertification::Certified(frames) = &scope.mapping else {
        panic!("small complete 65-member certificate must fit the existing archive envelope")
    };
    assert_eq!(
        frames
            .room_frames
            .iter()
            .map(|frame| frame.region)
            .collect::<Vec<_>>(),
        (1..=65).map(RegionId).collect::<Vec<_>>()
    );
    assert!(frames.exterior_doorways.is_empty());
}
