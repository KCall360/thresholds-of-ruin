//! Native source metadata / lawful captured World authority regressions.
//! Core restores or structurally validates each positive World before the host
//! hook; no paid chart acquisition or event-history authentication is claimed.
//! Source scope and bounds/chamber stay exact. Ordinary/wizard solidity, portals,
//! trap state, opacity, gravity and light belong to the captured World authority.
use super::*;
use serde::Deserialize;
use serde_json::{json, Value};
use tor_simulation::spatial_scope::registry::Registry;

const SEED: u64 = 42;

fn source_package(portal: bool, trapdoor: bool) -> Package {
    let base = super::tests::package();
    let mut manifest = serde_json::to_value(&base.manifest).unwrap();
    manifest["spatial_scopes"] = json!({
        "study":{"members":[1],"rooms":{"1":[{"region":1,"min":[0,0,0],"max":[6,4,1]}]},
            "passages":[],"doorways":[],"mapping":{"room_frames":[{"room":1,"region":1,"up":"up"}]}},
        "elsewhere":{"members":[2],"rooms":{"1":[{"region":2,"min":[0,0,0],"max":[2,2,1]}]},
            "passages":[],"doorways":[],"mapping":{"room_frames":[{"room":1,"region":2,"up":"up"}]}}
    });
    let mut first = json!({"id":1,"name":"Historical source","size":[7,5,2],"chamber":true,
        "anchors":{"start":[1,2,0]},
        "doors":[{"id":1,"at":[2,2,0],"open":true,"height":2}]});
    if portal {
        first["portals"] = json!([{"at":[6,2,0],"direction":"east","to":"2/entry",
            "kind":"portal","turns":0,"width":1,"height":2}]);
    }
    if trapdoor {
        first["traps"] = json!([{"id":1,"at":[3,2,0],"dc":12,"avoidance_dc":null,
            "kind":{"kind":"trap_door","destination":{"region":2,"name":"landing"},"open":false}}]);
    }
    let second = json!({"id":2,"name":"Other source scope","size":[3,3,2],"chamber":true,
        "anchors":{"entry":[0,2,0],"landing":[1,1,1],"other_landing":[2,1,1]}});
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![
            serde_json::from_value(first).unwrap(),
            serde_json::from_value(second).unwrap(),
        ],
        &base.rules_source,
    )
    .unwrap();
    package
        .check()
        .expect("valid complete source before every mutation");
    package
}

#[derive(Deserialize)]
struct PooledWorlds(#[serde(with = "tor_world::checkpoint_worlds")] Vec<World>);
fn captured(package: &Package) -> (World, Registry) {
    let game = package.build(SEED, true).unwrap();
    SpatialAudit::new(Some(package), SEED)
        .unwrap()
        .check(&game)
        .unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let root = game.checkpoint(&mut shared);
    let encoded_root = serde_json::to_value(&root).unwrap();
    let restored = Game::restore_checkpoint(root, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.spatial_registry(), game.spatial_registry());
    let root = encoded_root;
    let pool = serde_json::to_value(shared).unwrap();
    let worlds: PooledWorlds = serde_json::from_value(pool["worlds"].clone()).unwrap();
    let world = worlds.0[root["world"].as_u64().unwrap() as usize].clone();
    assert!(
        world.historical_checkpoint_valid(),
        "genuine native checkpoint World prerequisite"
    );
    let registry = restored.spatial_registry().clone();
    HistoricalSourceAudit::new(Some(package), SEED)
        .unwrap()
        .check(&world, &registry, "study")
        .expect("valid captured source root");
    (world, registry)
}
fn check(package: &Package, world: &World, registry: &Registry) -> Result<(), Failure> {
    assert!(
        world.historical_checkpoint_valid(),
        "core retained-World validation precondition"
    );
    HistoricalSourceAudit::new(Some(package), SEED)?.check(world, registry, "study")
}
fn reject(package: &Package, world: &World, registry: &Registry) {
    assert!(
        world.historical_checkpoint_valid(),
        "negative must reach source binding, not structural rejection"
    );
    let error =
        check(package, world, registry).expect_err("changed canonical source metadata must reject");
    assert_eq!(error.code, tor_protocol::ErrorCode::InvalidArchive);
}
fn changed_world(world: &World, edit: impl FnOnce(&mut Value)) -> World {
    let mut value = serde_json::to_value(world).unwrap();
    edit(&mut value);
    serde_json::from_value(value).unwrap()
}

#[test]
fn captured_source_world_allows_ordinary_multilevel_door_opacity_gravity_and_light_changes() {
    let package = source_package(false, false);
    let (mut world, registry) = captured(&package);
    let doorway = loc(1, [2, 2, 0]);
    assert!(world.door(doorway).unwrap().open);
    world.set_door(doorway, false);
    assert!(world.opaque(doorway) && world.opaque(loc(1, [2, 2, 1])));
    assert!(!world.is_wall(doorway));
    world.set_gravity(RegionId(1), [0, 0, 0]).unwrap();
    world
        .set_cell_gravity(loc(1, [1, 1, 0]), [1, 0, 0])
        .unwrap();
    world.set_region_light(RegionId(1), false).unwrap();
    world.set_cell_light(loc(1, [1, 1, 0]), true).unwrap();
    check(&package, &world, &registry).unwrap();
    world.set_door(doorway, true);
    check(&package, &world, &registry).unwrap();
}

#[test]
fn historical_scope_does_not_require_current_directory_membership_or_unrelated_world_geometry() {
    let package = source_package(false, false);
    let (mut world, registry) = captured(&package);
    let mut historical = Registry::default();
    historical
        .register_archive_checked(registry.scopes().find(|s| s.id == "study").unwrap().clone())
        .unwrap();
    let mut current = Registry::default();
    current
        .register_archive_checked(
            registry
                .scopes()
                .find(|s| s.id == "elsewhere")
                .unwrap()
                .clone(),
        )
        .unwrap();
    assert!(current.scopes().all(|scope| scope.id != "study"));
    // Region2 is not a source shell destination without a physical join. A
    // historical study check must not accidentally audit every current region.
    world.set_wall(loc(2, [1, 0, 0]), true).unwrap();
    check(&package, &world, &historical).unwrap();
    assert!(
        check(&package, &world, &current).is_err(),
        "supplied historical scope itself is required"
    );
    assert!(HistoricalSourceAudit::new(None, SEED)
        .unwrap()
        .check(&world, &historical, "study")
        .is_err());
}

#[test]
fn source_free_retained_package_roundtrip_authenticates_the_same_captured_world() {
    let package = source_package(true, false);
    let (mut world, registry) = captured(&package);
    world.set_door(loc(1, [2, 2, 0]), false);
    check(&package, &world, &registry).unwrap();
    let (_saved_inputs, reader) = super::tests::retained_authored_inputs(&package);
    let package_bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    let world_bytes = serde_json::to_vec(&world).unwrap();
    drop(package);
    drop(world);
    let restored =
        crate::storage::schema::scenario::decode_content_package(&package_bytes).unwrap();
    assert!(
        restored.directory.is_none(),
        "retained package needs no source directory"
    );
    super::tests::attach_retained_authored_inputs(&restored, reader);
    let world: World = serde_json::from_slice(&world_bytes).unwrap();
    assert!(world.historical_checkpoint_valid());
    check(&restored, &world, &registry).unwrap();
    // This is real package/native World encoding, not an actual-process or paid
    // chart acquisition claim. Those callers retain their separate obligations.
}

#[test]
fn valid_world_bounds_and_chamber_changes_reject_but_lawful_solidity_is_not_source_history() {
    let package = source_package(false, false);
    let (world, registry) = captured(&package);
    let bounds = changed_world(&world, |value| {
        assert_eq!(value["regions"]["1"]["bounds"]["width"], 9);
        value["regions"]["1"]["bounds"]["width"] = json!(10);
    });
    reject(&package, &bounds, &registry);
    let no_chamber = changed_world(&world, |value| {
        assert!(value["chambers"]
            .as_object_mut()
            .unwrap()
            .remove("1")
            .is_some());
    });
    reject(&package, &no_chamber, &registry);
    let mut missing = world.clone();
    let _record = missing.detach_region(RegionId(1)).unwrap();
    reject(&package, &missing, &registry);
    let mut changed_scope = registry
        .scopes()
        .find(|scope| scope.id == "study")
        .unwrap()
        .clone();
    changed_scope.mapping =
        tor_simulation::spatial_scope::mapping::MappingCertification::Uncertified;
    let mut changed_registry = Registry::default();
    changed_registry
        .register_archive_checked(changed_scope)
        .unwrap();
    reject(&package, &world, &changed_registry);
    let mut wall = world.clone();
    assert!(!wall.is_wall(loc(1, [0, 0, 0])));
    wall.set_wall(loc(1, [0, 0, 0]), true).unwrap();
    check(&package, &wall, &registry)
        .expect("lawful captured wall change is not initial-source equality");
}

#[test]
fn lawful_added_missing_rotated_exits_and_foreign_wall_do_not_invent_source_event_history() {
    let package = source_package(true, false);
    let (world, registry) = captured(&package);
    let from = loc(1, [6, 2, 0]);
    let key = json!([from, Direction::East]);
    assert_eq!(world.crossing_rotation(from, Direction::East), 0);
    let removed = changed_world(&world, |value| {
        for field in ["passages", "rotations"] {
            let rows = value[field].as_array_mut().unwrap();
            let before = rows.len();
            rows.retain(|row| row[0] != key);
            assert_eq!(rows.len() + 1, before);
        }
    });
    check(&package, &removed, &registry)
        .expect("structurally valid captured World is authoritative");
    let rotated = changed_world(&world, |value| {
        let row = value["rotations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row[0] == key)
            .unwrap();
        row[1] = json!(1);
    });
    // Canonical source frames are metadata, not a claim that captured joins
    // were never lawfully changed. Structural World authority is independent.
    assert_eq!(Direction::Up.rotated(1), Direction::Up);
    check(&package, &rotated, &registry)
        .expect("structurally valid captured World is authoritative");
    let mut added = world.clone();
    added
        .connect(
            Passage {
                from: loc(1, [0, 1, 0]),
                direction: Direction::West,
                to: loc(2, [2, 1, 0]),
            },
            0,
        )
        .unwrap();
    check(&package, &added, &registry).expect("structurally valid captured World is authoritative");
    let mut foreign_wall = world.clone();
    foreign_wall.set_wall(loc(2, [0, 2, 0]), true).unwrap();
    check(&package, &foreign_wall, &registry)
        .expect("structurally valid captured World is authoritative");
}

#[test]
fn genuine_opened_traps_and_other_structurally_valid_apertures_need_no_source_event_journal() {
    let package = source_package(false, true);
    let (closed, registry) = captured(&package);
    let from = loc(1, [3, 2, 0]);
    let to = loc(2, [1, 1, 1]);
    let mut opened = closed.clone();
    assert!(opened.open_trapdoor(from).unwrap());
    assert_eq!(
        opened.physics_neighbor(from, Direction::Down),
        Some((to, 0))
    );
    assert_eq!(opened.physics_neighbor(to, Direction::Up), Some((from, 0)));
    assert!(!opened.is_stair(from, Direction::Down));
    assert!(!opened.is_stair(to, Direction::Up));
    assert!(opened.historical_checkpoint_valid());
    check(&package, &opened, &registry).expect("ordinary paid trap topology is lawful");
    assert!(
        !opened.open_trapdoor(from).unwrap(),
        "opening is permanent and idempotent"
    );

    // These saved states satisfy structural World invariants and create their
    // reciprocal edges through the REAL transition. The host must not invent
    // an initial-source event-authentication requirement for them.
    for mutation in ["destination", "identity", "location"] {
        let mut forged = changed_world(&closed, |value| {
            let trap = &mut value["traps"][0];
            assert_eq!(trap[0], json!(from));
            assert_eq!(trap[1]["id"], 1);
            match mutation {
                "destination" => trap[1]["kind"]["destination"]["name"] = json!("other_landing"),
                "identity" => trap[1]["id"] = json!(9),
                "location" => trap[0] = json!(loc(1, [4, 2, 0])),
                _ => unreachable!(),
            }
        });
        let position = if mutation == "location" {
            loc(1, [4, 2, 0])
        } else {
            from
        };
        assert!(forged.open_trapdoor(position).unwrap());
        check(&package, &forged, &registry)
            .expect("captured trap authority, not source event history");
    }
    let mut extra = opened;
    extra
        .connect_portal_area(
            Passage {
                from: loc(1, [4, 2, 0]),
                direction: Direction::Down,
                to: loc(2, [2, 1, 1]),
            },
            0,
            1,
            1,
        )
        .unwrap();
    extra
        .connect_portal_area(
            Passage {
                from: loc(2, [2, 1, 1]),
                direction: Direction::Up,
                to: loc(1, [4, 2, 0]),
            },
            0,
            1,
            1,
        )
        .unwrap();
    check(&package, &extra, &registry).unwrap();
}

#[test]
fn opened_trap_endpoints_use_saved_world_invariants_when_the_other_region_is_detached() {
    let package = source_package(false, true);
    let (mut world, registry) = captured(&package);
    let from = loc(1, [3, 2, 0]);
    let to = loc(2, [1, 1, 1]);
    assert!(world.open_trapdoor(from).unwrap());
    check(&package, &world, &registry).unwrap();

    let mut below_only = world.clone();
    let _source_record = below_only.detach_region(RegionId(1)).unwrap();
    assert!(below_only.region(RegionId(1)).is_none());
    assert!(below_only.trap(from).is_none());
    assert_eq!(below_only.passage(to, Direction::Up).unwrap().to, from);
    assert!(below_only.historical_checkpoint_valid());
    HistoricalSourceAudit::new(Some(&package), SEED)
        .unwrap()
        .check(&below_only, &registry, "elsewhere")
        .expect(
            "retained reverse Up aperture is structurally valid without a resident source trap",
        );
    let forged = changed_world(&below_only, |value| {
        let key = json!([to, Direction::Up]);
        let row = value["passages"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|row| row[0] == key)
            .unwrap();
        row[1]["to"] = json!(loc(1, [4, 2, 0]));
    });
    assert!(forged.historical_checkpoint_valid());
    HistoricalSourceAudit::new(Some(&package), SEED)
        .unwrap()
        .check(&forged, &registry, "elsewhere")
        .expect("no invented event-history claim for a structurally valid captured reverse edge");

    let _target_record = world.detach_region(RegionId(2)).unwrap();
    assert!(world.region(RegionId(2)).is_none());
    assert!(matches!(
        &world.trap(from).unwrap().kind,
        tor_world::TrapKind::TrapDoor { open: true, .. }
    ));
    assert!(world.historical_checkpoint_valid());
    check(&package, &world, &registry)
        .expect("resident source requires reciprocal only when target is resident");
}

#[test]
fn generated_scope_uses_the_shared_geometry_seam_and_retained_recipe_without_a_directory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/rogue-geometry");
    let mut manifest = resolve_source(&root).unwrap().manifest;
    let family = manifest.procedural_worlds.get_mut("dungeon").unwrap();
    family.grid = [2, 1];
    family.check().unwrap();
    let package = Package::from_parts(
        manifest,
        vec![],
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    package.check().unwrap();
    // These are native generated compiler roots, not a paid chart publication.
    let generated = package.generated_group("dungeon/1", SEED).unwrap();
    let ordinary = package
        .canonical_generated_spatial("dungeon/1", SEED, &generated)
        .unwrap();
    let (canonical, world) = package.canonical_scope_geometry("dungeon/1", SEED).unwrap();
    assert_eq!(canonical, ordinary);
    assert_eq!(
        world.loaded_regions().collect::<Vec<_>>(),
        vec![RegionId(1), RegionId(2)]
    );
    let mut registry = Registry::default();
    registry.register_archive_checked(ordinary).unwrap();
    HistoricalSourceAudit::new(Some(&package), SEED)
        .unwrap()
        .check(&world, &registry, "dungeon/1")
        .unwrap();
    let bytes = crate::storage::schema::scenario::encode_content_package(&package).unwrap();
    let world_bytes = serde_json::to_vec(&world).unwrap();
    drop(package);
    drop(world);
    let restored = crate::storage::schema::scenario::decode_content_package(&bytes).unwrap();
    assert!(restored.directory.is_none());
    let mut world: World = serde_json::from_slice(&world_bytes).unwrap();
    assert!(world.historical_checkpoint_valid());
    let mut audit = HistoricalSourceAudit::new(Some(&restored), SEED).unwrap();
    audit.check(&world, &registry, "dungeon/1").unwrap();
    let open = registry
        .scopes()
        .next()
        .unwrap()
        .primary
        .iter()
        .find_map(|run| {
            (run.cells.first_x..=run.cells.last_x)
                .map(|x| loc(run.cells.region.0, [x, run.cells.y, run.cells.z]))
                .find(|at| !world.is_wall(*at))
        })
        .unwrap();
    world.set_wall(open, true).unwrap();
    assert!(world.historical_checkpoint_valid());
    audit
        .check(&world, &registry, "dungeon/1")
        .expect("retained generated scope does not prohibit lawful wall edits");
}
