//! Region diagnostics count real lifecycle membership without expanding families.
use super::*;
use crate::procedural_world::{Domain, FamilyKind, WorldFamily};
use crate::scenario_package::{Package, RegionDef};

fn scenario(max_depth: u32, authored: bool) -> Scenario {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/explicit-grid-groups");
    let template = crate::scenario_package::read_package(&root).unwrap();
    let mut manifest = template.manifest.clone();
    manifest.generation_groups.clear();
    manifest.stair_pairs.clear();
    manifest.characters[0].anchor = "caves/1/start".into();
    manifest.procedural_worlds = BTreeMap::from([(
        "caves".into(),
        WorldFamily {
            kind: FamilyKind::GridChain,
            version: 1,
            base_region: 100,
            domain: Domain::Open { max_depth },
            grid: [3, 3],
            size: [26, 7, 2],
            recipe: "exploration".into(),
            zone: None,
        },
    )]);
    let definitions: Vec<RegionDef> = if authored {
        vec![
            toml::from_str("id=1\nname='Unvisited authored room'\nsize=[5,5,2]\nchamber=true\n")
                .unwrap(),
        ]
    } else {
        vec![]
    };
    let package = Package::from_parts(manifest, definitions, &template.rules_source).unwrap();
    Scenario {
        seed: 42,
        actors: vec![],
        regions: u64::from(authored),
        workload_version: None,
        package: Some(Arc::new(package)),
        streaming: Some(crate::regions::Streaming {
            active_radius: 0,
            load_radius: 0,
        }),
    }
}

fn lifecycle(counts: RegionCounts) -> (usize, usize, usize, usize) {
    (
        counts.active,
        counts.frozen,
        counts.detached,
        counts.unbuilt,
    )
}

fn total(counts: RegionCounts) -> usize {
    counts.active + counts.frozen + counts.detached + counts.unbuilt
}

// Generated stair endpoints declare one neighboring member without generating
// its group. Derive these expected IDs from the bounded generated groups, not
// from the diagnostic counter or the family's entire potential address space.
fn stair_targets_beyond(
    family: &WorldFamily,
    depths: std::ops::RangeInclusive<u32>,
) -> BTreeSet<tor_world::RegionId> {
    let built: BTreeSet<_> = depths
        .flat_map(|depth| family.group("caves", 42, depth).unwrap().members)
        .collect();
    built
        .iter()
        .flat_map(|id| family.connections("caves", 42, *id))
        .filter(|connection| !built.contains(&connection.destination))
        .map(|connection| tor_world::RegionId(connection.destination))
        .collect()
}

#[test]
fn descriptor_region_counts_follow_materialized_lifecycle_and_survive_restart() {
    let mut observations = Vec::new();
    for cap in [3, 1_000_000] {
        let scenario = scenario(cap, false);
        let package = scenario.package.as_ref().unwrap();
        assert!(package.index.regions.is_empty());
        let family = &package.manifest.procedural_worlds["caves"];
        let initial_unbuilt = stair_targets_beyond(family, 1..=1);
        let after_unbuilt = stair_targets_beyond(family, 1..=2);
        assert_eq!(initial_unbuilt.len(), 1);
        assert_eq!(after_unbuilt.len(), 1);
        let group = family.group("caves", 42, 2).unwrap();
        let definitions = crate::generation_recipe::materialize(
            "caves/2",
            &group,
            &package.manifest.generation_recipes[&group.recipe],
            group
                .members
                .iter()
                .map(|id| family.describe("caves", 42, *id).unwrap().unwrap())
                .collect(),
            42,
        )
        .unwrap();
        let def = &definitions.regions[&group.members[0]];
        let position = (1..def.size[1] - 1)
            .flat_map(|y| (1..def.size[0] - 1).map(move |x| [x, y, 0]))
            .find(|at| !def.walls.contains(at))
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let save = directory.path().join("counts.db");
        let mut engine = Engine::open(&save, scenario).unwrap();
        let initial = engine.region_counts().unwrap();
        assert_eq!(
            initial.active + initial.frozen + initial.detached,
            9,
            "the initial group has nine generated members"
        );
        assert_eq!(
            engine.game.unbuilt_regions().collect::<BTreeSet<_>>(),
            initial_unbuilt
        );
        assert_eq!(
            total(initial),
            9 + initial_unbuilt.len(),
            "generated members and explicitly declared stair targets are counted"
        );
        assert!(initial.active > 0);
        assert_eq!(
            initial.active + initial.frozen,
            engine.game.loaded_regions().count()
        );
        assert_eq!(initial.detached, engine.game.detached_records().count());
        assert_eq!(
            initial.unbuilt,
            initial_unbuilt.len(),
            "only declared stair targets count, not all unvisited procedural addresses"
        );
        engine.enable_wizard().unwrap();
        engine
            .command(
                "test",
                "test",
                ActorId(1),
                "depth-two",
                &engine.branch().clone(),
                Command::Wizard {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    operation: WizardOperation::Teleport {
                        actor: ActorId(1),
                        position: Position {
                            region: def.id,
                            x: position[0],
                            y: position[1],
                            z: position[2],
                        },
                    },
                },
            )
            .unwrap();
        engine.flush().unwrap();
        let after = engine.region_counts().unwrap();
        assert_eq!(after.active + after.frozen + after.detached, 18);
        // Publishing a whole generated group leaves some members detached.
        // Only attaching a member declares its external linked destinations;
        // the teleport need not attach the group's down-stair member.
        let declared: BTreeSet<_> = engine.game.unbuilt_regions().collect();
        assert!(declared.is_subset(&after_unbuilt));
        assert_eq!(after.unbuilt, declared.len());
        assert_eq!(total(after), 18 + declared.len());
        assert!(after.active > 0 && after.detached > 0);
        assert_eq!(
            after.active + after.frozen,
            engine.game.loaded_regions().count()
        );
        assert_eq!(after.detached, engine.game.detached_records().count());
        let state = engine.state(ActorId(1)).unwrap();
        drop(engine);
        let restored = Engine::open(&save, Scenario::two_room(0)).unwrap();
        assert_eq!(restored.state(ActorId(1)).unwrap(), state);
        assert_eq!(
            lifecycle(restored.region_counts().unwrap()),
            lifecycle(after)
        );
        observations.push((lifecycle(initial), lifecycle(after)));
    }
    assert_eq!(
        observations[0], observations[1],
        "operational depth cap does not inflate counts for equivalent interior states"
    );
}

#[test]
fn descriptor_region_counts_preserve_unvisited_authored_regions_without_double_counting() {
    let engine = Engine::memory(scenario(1_000_000, true)).unwrap();
    let counts = engine.region_counts().unwrap();
    // Nine generated members, one declared stair target, one authored room.
    assert_eq!(total(counts), 11);
    assert_eq!(counts.unbuilt, 2);
    assert_eq!(counts.active + counts.frozen + counts.detached, 9);
}
