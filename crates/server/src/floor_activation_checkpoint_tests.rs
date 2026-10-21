//! Checkpoint size accounting for a bounded generated floor with unchanged
//! population across ordinary retained gameplay boundaries. No timing claims.
use super::*;
use std::io::{self, Write};

#[derive(Default)]
struct ByteCount(usize);

impl Write for ByteCount {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn raw_bytes(value: &impl Serialize) -> usize {
    let mut count = ByteCount::default();
    serde_json::to_writer(&mut count, value).unwrap();
    count.0
}

fn large_floor_fixture() -> Scenario {
    let mut scenario = fixture();
    let original = scenario.package.as_ref().unwrap();
    let mut manifest = original.manifest.clone();
    // Sixteen maximum-size rooms remain inside the admitted grid-chain limits
    // (8 x 8 members, 32 x 32 cells per member). The population is one marker.
    let family = manifest.procedural_worlds.get_mut("dungeon").unwrap();
    family.grid = [4, 4];
    family.size = [32, 32, 2];
    let recipe = manifest.generation_recipes.get_mut("exploration").unwrap();
    assert!(matches!(
        recipe.stages[2].operation,
        crate::generation_recipe::Operation::Rooms { .. }
    ));
    recipe.stages[2].operation = crate::generation_recipe::Operation::Rooms {
        width: [30, 30],
        height: [30, 30],
    };
    scenario.package = Some(Arc::new(
        Package::from_parts(manifest, vec![], &original.rules_source).unwrap(),
    ));
    scenario
}

#[test]
fn unchanged_population_with_128_ordinary_boundaries_fits_checkpoint_codec() {
    let mut engine = Engine::memory(large_floor_fixture()).unwrap();
    let initial = Checkpoint::capture(&engine).encode("bounded-population", 0);
    let initial_raw = raw_bytes(&initial);
    assert!(initial_raw < crate::storage::checkpoint_codec::LIMIT);
    let initial_decisions = decisions(&engine);
    let initial_count = engine.game.activation_count();

    for boundary in 0..128 {
        engine
            .command(
                "player",
                "checkpoint-population-test",
                ActorId(1),
                &format!("wait-{boundary}"),
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
    }
    assert_eq!(engine.boundaries.len(), 128);
    assert_eq!(engine.game.activation_count(), initial_count);
    assert_eq!(decisions(&engine), initial_decisions);
    assert!(engine
        .archive
        .records
        .iter()
        .all(|record| record.activations.is_empty()));

    let disk = Checkpoint::capture(&engine).encode("bounded-population", 128);
    let retained_raw = raw_bytes(&disk);
    let encoded = crate::storage::checkpoint_codec::write_json(&disk, Vec::new());
    eprintln!(
        "bounded population checkpoint bytes: initial_raw={initial_raw}, \
         retained_128_raw={retained_raw}, codec_limit={}, codec_ok={}",
        crate::storage::checkpoint_codec::LIMIT,
        encoded.is_ok()
    );
    assert!(
        retained_raw <= crate::storage::checkpoint_codec::LIMIT && encoded.is_ok(),
        "unchanged population must fit the checkpoint envelope: \
         initial_raw={initial_raw}, retained_128_raw={retained_raw}, codec={:?}",
        encoded.as_ref().map(Vec::len)
    );
    let encoded = encoded.unwrap();
    let decoded = crate::storage::checkpoint_codec::decode(&encoded).unwrap();
    assert_eq!(decoded.len(), retained_raw);
    let disk: DiskCheckpoint = serde_json::from_slice(&decoded).unwrap();
    exact_equal(&disk.restore(engine.archive.clone()).unwrap(), &engine);
}

#[test]
fn changing_retrieval_boundaries_keep_large_unchanged_population_pooled() {
    let mut scenario = large_floor_fixture();
    let original = scenario.package.as_ref().unwrap();
    let mut manifest = original.manifest.clone();
    manifest.characters[0].anchor = "dungeon/26/up".into();
    scenario.package = Some(Arc::new(
        Package::from_parts(manifest, vec![], &original.rules_source).unwrap(),
    ));
    let scenario = super::retrieval::attach_objective(scenario);
    let at = |depth| {
        let package = scenario.package.as_ref().unwrap();
        let family = &package.manifest.procedural_worlds["dungeon"];
        let identity = format!("dungeon/{depth}");
        let group = family.group("dungeon", scenario.seed, depth).unwrap();
        let definitions = crate::generation_recipe::materialize(
            &identity,
            &group,
            &package.manifest.generation_recipes[&group.recipe],
            group
                .members
                .iter()
                .map(|id| {
                    family
                        .describe("dungeon", scenario.seed, *id)
                        .unwrap()
                        .unwrap()
                })
                .collect(),
            scenario.seed,
        )
        .unwrap();
        let region = definitions
            .regions
            .values()
            .find(|region| region.anchors.contains_key("up"))
            .unwrap();
        let [x, y, z] = region.anchors["up"];
        Position {
            region: region.id,
            x,
            y,
            z,
        }
    };
    let destinations = [at(26), at(27)];
    let mut engine = Engine::memory(scenario).unwrap();
    wizard(
        &mut engine,
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: destinations[1],
        },
    );
    let population = decisions(&engine);
    let item = engine.game.unique_retrieval_state().unwrap().item;
    for boundary in 0..128 {
        wizard(
            &mut engine,
            WizardOperation::Teleport {
                actor: ActorId(1),
                position: destinations[boundary % 2],
            },
        );
    }
    assert_eq!(decisions(&engine), population);
    assert_eq!(engine.game.unique_retrieval_state().unwrap().item, item);
    assert_eq!(
        engine.game.unique_retrieval_state().unwrap().entry_count,
        130
    );
    let disk = Checkpoint::capture(&engine).encode(
        "retrieval-population-pool",
        engine.archive.records.len() as u64,
    );
    let value = serde_json::to_value(&disk).unwrap();
    assert_eq!(
        value["shared"]["floor_activations"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "latest retrieval batches must never change the independently pooled population state"
    );
    let reference = &value["game"]["floor_activation"];
    assert!(value["boundaries"]
        .as_array()
        .unwrap()
        .iter()
        .all(|boundary| &boundary["game"]["floor_activation"] == reference));
    let raw = raw_bytes(&disk);
    assert!(
        raw <= crate::storage::checkpoint_codec::LIMIT,
        "bounded snapshot raw bytes: {raw}"
    );
    let encoded = crate::storage::checkpoint_codec::write_json(&disk, Vec::new()).unwrap();
    let bytes = crate::storage::checkpoint_codec::decode(&encoded).unwrap();
    let restored: DiskCheckpoint = serde_json::from_slice(&bytes).unwrap();
    exact_equal(&restored.restore(engine.archive.clone()).unwrap(), &engine);
}

#[test]
fn omitted_group_capacity_failure_preserves_exact_checkpoint_and_region_authority() {
    let mut engine = Engine::memory(fixture()).unwrap();
    engine.enable_wizard().unwrap();
    let original = engine.archive.scenario.package.as_ref().unwrap();
    let family = &original.manifest.procedural_worlds["dungeon"];
    let group = family.group("dungeon", engine.game.seed(), 101).unwrap();
    let unprotected = group
        .members
        .iter()
        .filter(|id| {
            let definition = family
                .describe("dungeon", engine.game.seed(), **id)
                .unwrap()
                .unwrap();
            let generation = definition.generate.as_ref().unwrap();
            generation.stair_anchors.is_empty()
                && definition
                    .anchors
                    .keys()
                    .all(|name| generation.boundary_anchors.contains(name))
        })
        .count();
    let impossible = unprotected.min(group.members.len() - 1) as u32 + 1;
    // Validate the unchanged distant group first. Fault injection changes only
    // its recipe policy, not actor state or the command being rejected.
    assert_eq!(
        original
            .build_group(
                engine.game.seed(),
                &original.index(engine.game.seed()).unwrap(),
                RegionId(group.members[0])
            )
            .unwrap()
            .records
            .len(),
        9
    );
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["generation_recipes"]["exploration"]["version"] = json!(4);
    source["generation_recipes"]["exploration"]["stages"]
        .as_array_mut()
        .unwrap()
        .insert(
            6,
            json!({"id":"room_omissions", "version":1,
            "operation":{"kind":"room_omissions", "config":{"count":[impossible,impossible]}}}),
        );
    let rejected = Arc::new(
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap(),
    );
    let failure = rejected
        .build_group(
            engine.game.seed(),
            &rejected.index(engine.game.seed()).unwrap(),
            RegionId(group.members[0]),
        )
        .err()
        .unwrap();
    assert!(
        failure.message.contains("unprotected slot capacity"),
        "{failure:?}"
    );
    engine
        .regions
        .as_mut()
        .unwrap()
        .bind_package_inputs(rejected.clone());
    engine.archive.scenario.package = Some(rejected);
    let before = engine.diagnostic_copy();
    let checkpoint = || {
        serde_json::to_vec(&Checkpoint::capture(&engine).encode("omission-rollback", 0)).unwrap()
    };
    let bytes = checkpoint();
    let records = engine.regions.as_ref().unwrap().encoded_resident().unwrap();
    let failure = engine
        .command(
            "player",
            "omission-capacity",
            ActorId(1),
            "rejected-group",
            &engine.branch().clone(),
            Command::Wizard {
                expected_revision: engine.revision(ActorId(1)).unwrap(),
                operation: WizardOperation::Teleport {
                    actor: ActorId(1),
                    position: Position {
                        region: *group.members.first().unwrap(),
                        x: 1,
                        y: 1,
                        z: 0,
                    },
                },
            },
        )
        .expect_err("insufficient omission capacity must reject publication");
    assert!(
        failure.message.contains("unprotected slot capacity"),
        "{failure:?}"
    );
    exact_equal(&engine, &before);
    assert_eq!(authority(&engine), authority(&before));
    assert_eq!(
        engine.regions.as_ref().unwrap().encoded_resident().unwrap(),
        records
    );
    assert_eq!(
        serde_json::to_vec(&Checkpoint::capture(&engine).encode("omission-rollback", 0)).unwrap(),
        bytes
    );
}
