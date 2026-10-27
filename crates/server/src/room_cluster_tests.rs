//! Shared generated-room authoring, entry publication, and immutable recovery.
use super::*;

fn cluster_fixture(minimum: u32) -> Scenario {
    let mut scenario = carried_stock::carrier_fixture(minimum);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    let actor = source["floor_activation"]["profiles"]["baseline"]["stages"][2].clone();
    source["floor_activation"]["profiles"]["baseline"]["stages"] = json!([{
        "kind":"room_cluster", "id":"treasure", "version":1,
        "chance":{"numerator":1,"denominator":1}, "item_count":[2,2], "actor_count":[1,1],
        "actor_depth_offset":1, "quantity":1, "ai":"wander", "depth_growth":{"kind":"racial_after","depth":26},
        "item_bands":[{"depth":{"kind":"open","minimum":minimum},"choices":[{"archetype":"marker","weight":1}]}],
        "actor_bands":[{"depth":{"kind":"open","minimum":minimum+1},"choices":actor["bands"][0]["choices"]}]
    }]);
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap(),
    ));
    scenario
}
fn selection(engine: &Engine, depth: u32) -> Value {
    floor(engine, depth)["decisions"][0]["result"]["selection"].clone()
}

#[test]
fn generated_cluster_entry_recovers_nested_births_and_revisits_without_reroll() {
    let mut engine = Engine::memory(cluster_fixture(2)).unwrap();
    let initial = decisions(&engine);
    engine.start_preloading();
    engine.settle_preloading();
    assert_eq!(
        decisions(&engine),
        initial,
        "preparation does not activate a cluster"
    );
    walk_to_port(&mut engine, 1, "down");
    step(&mut engine, Direction::Down);
    let selected = selection(&engine, 2);
    assert_eq!(selected["kind"], "selected");
    assert_eq!(selected["actor_depth"], 3);
    assert_eq!(selected["items"].as_array().unwrap().len(), 2);
    assert_eq!(selected["actors"].as_array().unwrap().len(), 1);
    assert_eq!(
        selected["actors"][0]["entity"]["carried_loot"]["quantity"],
        3
    );
    restored_both_ways(&engine);
    let before = decisions(&engine);
    for direction in [Direction::Up, Direction::Down] {
        for _ in 0..16 {
            if engine.next_actor() == Some(ActorId(1)) {
                break;
            }
            let actor = engine.next_actor().unwrap();
            assert!(engine.is_ai(actor));
            engine.advance_ai(actor).unwrap();
        }
        step(&mut engine, direction);
    }
    assert_eq!(decisions(&engine), before);
    restored_both_ways(&engine);
}

#[test]
fn generated_cluster_admits_tall_creatures_but_rejects_blocked_birth_height() {
    let mut placed = None;
    for height in [2, 3] {
        let mut scenario = cluster_fixture(1);
        let original = scenario.package.as_ref().unwrap();
        let mut source = serde_json::to_value(&original.manifest).unwrap();
        source["archetypes"]["rat"]["body"] = json!({
            "cells":(0..height).map(|z| [0,0,z]).collect::<Vec<_>>(),
            "eye":[0,0,0],"mass":80
        });
        scenario.package = Some(Arc::new(
            Package::from_parts(
                serde_json::from_value(source).unwrap(),
                vec![],
                &original.rules_source,
            )
            .unwrap(),
        ));
        let engine = Engine::memory(scenario).unwrap();
        let selected = selection(&engine, 1);
        let package = engine.archive.scenario.package.as_ref().unwrap();
        let record = engine.game.activations_since(0).unwrap().remove(0);
        let configuration = package.population_configuration(42).unwrap();
        assert!(configuration.valid_record(&record));
        if height == 2 {
            let id = selected["actors"][0]["entity"]["id"]
                .as_u64()
                .expect("a real two-cell racial creature fits the room's open vertical column");
            assert!(engine.game.creature(SimActor(id)).is_some());
            placed = Some(record);
        } else {
            assert_eq!(selected["actors"][0]["reason"], "no_eligible_cell");
            assert!(
                !configuration.valid_record(placed.as_ref().unwrap()),
                "an otherwise identical birth receipt cannot hide a body above pinned room clearance"
            );
        }
        restored_both_ways(&engine);
    }
}

#[test]
fn generated_cluster_uses_next_depth_for_ordinary_racial_builds() {
    let mut scenario = cluster_fixture(26);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["characters"][0]["anchor"] = json!("dungeon/26/up");
    source["characters"].as_array_mut().unwrap().truncate(1);
    let base_hd = source["archetypes"]["rat"]["creature"]["hit_dice"]
        .as_array()
        .unwrap()
        .len();
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap(),
    ));
    let engine = Engine::memory(scenario).unwrap();
    let selected = selection(&engine, 26);
    assert_eq!(selected["actor_depth"], 27);
    let actor = tor_simulation::ActorId(selected["actors"][0]["entity"]["id"].as_u64().unwrap());
    assert_eq!(
        engine
            .game
            .creature(actor)
            .unwrap()
            .build()
            .ledger()
            .total_hd()
            .get() as usize,
        base_hd + 1
    );
    restored_both_ways(&engine);
}

#[test]
fn cluster_late_store_refusal_rolls_back_items_actors_stock_and_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("cluster.db");
    let mut engine = Engine::open(&path, cluster_fixture(2)).unwrap();
    walk_to_port(&mut engine, 1, "down");
    admit(&mut engine, "cluster-entry", Direction::Down);
    engine.flush().unwrap();
    drop(engine);
    let mut rejected = Engine::open_with_policy(
        &path,
        Scenario::two_room(0),
        crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let before = rejected.diagnostic_copy();
    let mut healthy = Engine::replay(rejected.archive.clone(), None, None).unwrap();
    healthy.execute_next_intention().unwrap().unwrap();
    assert_eq!(selection(&healthy, 2)["kind"], "selected");
    assert!(rejected.execute_next_intention().is_err());
    exact_equal(&rejected, &before);
    assert_eq!(authority(&rejected), authority(&before));
    drop(before);
    drop(rejected);
    let mut retry = Engine::open(&path, Scenario::two_room(0)).unwrap();
    retry.execute_next_intention().unwrap().unwrap();
    assert_eq!(retry.game, healthy.game);
    assert_eq!(decisions(&retry), decisions(&healthy));
    restored_both_ways(&retry);
}

#[test]
fn cluster_receipt_wrong_room_depth_and_nested_stock_reject_replay_and_checkpoint() {
    let engine = Engine::memory(cluster_fixture(1)).unwrap();
    restored_both_ways(&engine);
    for field in ["room", "actor_depth", "quantity", "identity"] {
        let mut archive = serde_json::to_value(&engine.archive).unwrap();
        let selected =
            &mut archive["initial_activations"][0]["decisions"][0]["result"]["selection"];
        match field {
            "room" => selected["room"] = json!(64),
            "actor_depth" => selected["actor_depth"] = json!(1),
            "quantity" => selected["actors"][0]["entity"]["carried_loot"]["quantity"] = json!(4),
            "identity" => {
                selected["actors"][0]["entity"]["carried_loot"]["item"] = json!(u64::MAX - 1)
            }
            _ => unreachable!(),
        }
        let archive: Archive = serde_json::from_value(archive).unwrap();
        assert!(
            Engine::replay(archive.clone(), None, None).is_err(),
            "{field}"
        );
        let disk = Checkpoint::capture(&engine).encode("cluster-corruption", 0);
        assert!(disk.restore(archive).is_err(), "{field}");
    }
}

#[test]
fn cluster_authoring_rejects_bad_counts_depth_chance_ai_and_unreachable_choices() {
    let scenario = cluster_fixture(1);
    let original = scenario.package.as_ref().unwrap();
    original.check().unwrap();
    let good = serde_json::to_value(&original.manifest).unwrap();
    for case in 0..10 {
        let mut bad = good.clone();
        let stage = &mut bad["floor_activation"]["profiles"]["baseline"]["stages"][0];
        match case {
            0 => stage["item_count"] = json!([0, 0]),
            1 => {
                stage["item_count"] = json!([128, 128]);
                stage["actor_count"] = json!([129, 129]);
            }
            2 => stage["actor_depth_offset"] = json!(2),
            3 => stage["chance"]["denominator"] = json!(0),
            4 => stage["chance"]["numerator"] = json!(2),
            5 => stage["ai"] = json!("absent"),
            6 => stage["item_bands"][0]["choices"][0]["archetype"] = json!("absent"),
            7 => {
                stage["chance"]["numerator"] = json!(0);
                stage["actor_bands"][0]["choices"][0]["archetype"] = json!("absent");
            }
            8 => {
                let band = stage["actor_bands"][0].clone();
                stage["actor_bands"] = json!([band.clone(), band]);
            }
            9 => stage["actor_bands"][0]["choices"][0]["carried_loot"]["pool"] = json!("absent"),
            _ => unreachable!(),
        }
        assert!(
            Package::from_parts(
                serde_json::from_value(bad).unwrap(),
                vec![],
                &original.rules_source
            )
            .and_then(|package| package.check())
            .is_err(),
            "case {case}"
        );
    }
}

#[test]
fn expired_abandoned_cluster_still_binds_selected_room_area_and_full_footprint() {
    // Initial streaming may prepare linked neighbors without activation. Even
    // with 0/0 radii, three-axis-step reach may activate floor 2 and pin 3;
    // distinct start/up/down anchors keep floor 4 beyond that initial closure.
    const DEPTH: u32 = 4;
    let identity = format!("dungeon/{DEPTH}");
    let mut scenario = cluster_fixture(DEPTH);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["archetypes"]["rat"]["body"] =
        json!({"cells":[[0,0,0],[0,0,1],[1,0,0]],"eye":[0,0,0],"mass":80});
    scenario.package = Some(Arc::new(
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap(),
    ));
    let mut engine = Engine::memory(scenario).unwrap();
    assert!(engine
        .game
        .activation_scopes()
        .all(|scope| scope.scope != identity));
    for depth in 1..DEPTH {
        walk_to_port(&mut engine, depth, "down");
        step(&mut engine, Direction::Down);
    }
    let abandoned = engine.archive.records.len() - 1;
    let abandoned_id = engine.archive.records[abandoned].entry.id.clone();
    wizard(&mut engine, WizardOperation::Rewind { target: None });
    for _ in 0..REWIND_BOUNDARIES {
        engine
            .command(
                "player",
                "cluster-history",
                ActorId(1),
                &uuid::Uuid::new_v4().to_string(),
                &engine.branch().clone(),
                Command::Act {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action: Action::Wait,
                },
            )
            .unwrap();
    }
    assert!(engine
        .boundaries
        .iter()
        .all(|boundary| boundary.id.as_ref() != Some(&abandoned_id)));
    assert!(
        engine
            .game
            .activation_scopes()
            .all(|scope| scope.scope != identity),
        "the live Game must not supply the historical scope"
    );
    restored_both_ways(&engine);
    let package = engine.archive.scenario.package.as_ref().unwrap();
    let record = &engine.archive.records[abandoned].activations[0];
    let configuration = package.population_configuration(42).unwrap();
    assert!(configuration.valid_record(record));
    let encoded = serde_json::to_value(record).unwrap();
    let selected = &encoded["decisions"][0]["result"]["selection"];
    let room_id = selected["room"].as_u64().unwrap() as u32;
    let family = &package.manifest.procedural_worlds["dungeon"];
    let group = family.group("dungeon", 42, DEPTH).unwrap();
    let generated = crate::generation_recipe::materialize(
        &identity,
        &group,
        &package.manifest.generation_recipes[&group.recipe],
        group
            .members
            .iter()
            .map(|id| family.describe("dungeon", 42, *id).unwrap().unwrap())
            .collect(),
        42,
    )
    .unwrap();
    let room = generated
        .rooms
        .iter()
        .find(|room| room.id == room_id)
        .unwrap();
    let protected: BTreeSet<_> = generated
        .regions
        .values()
        .flat_map(|region| {
            region.anchors.values().map(|at| Location {
                region: RegionId(region.id),
                position: WorldPosition {
                    x: at[0],
                    y: at[1],
                    z: at[2],
                },
            })
        })
        .collect();
    let edge_x = room.cells.iter().map(|at| at.position.x).max().unwrap();
    let spill = room
        .cells
        .iter()
        .find(|at| at.position.x == edge_x && !protected.contains(*at))
        .unwrap();
    for change in ["room", "area", "footprint"] {
        let mut archive = serde_json::to_value(&engine.archive).unwrap();
        let selection = &mut archive["records"][abandoned]["activations"][0]["decisions"][0]
            ["result"]["selection"];
        match change {
            "room" => selection["room"] = json!(64),
            "area" => selection["room_area"] = json!(room.cells.len() + 1),
            "footprint" => {
                selection["actors"][0]["location"] = serde_json::to_value(spill).unwrap()
            }
            _ => unreachable!(),
        }
        let archive: Archive = serde_json::from_value(archive).unwrap();
        assert!(
            !configuration.valid_record(&archive.records[abandoned].activations[0]),
            "source binding: {change}"
        );
        let disk = Checkpoint::capture(&engine)
            .encode("expired-cluster", engine.archive.records.len() as u64);
        assert!(
            disk.restore(archive).is_err(),
            "expired checkpoint: {change}"
        );
    }
}

#[test]
fn cluster_actor_catalog_and_selectors_use_explicit_birth_depth_without_mutation() {
    let engine = Engine::memory(cluster_fixture(1)).unwrap();
    let before = engine.diagnostic_copy();
    let regions = engine.regions.as_ref().unwrap();
    let selector = "population:baseline/treasure/rat";
    let page =
        crate::wizard_creatures::catalog(&engine.game, Some(regions), Some(2), None).unwrap();
    assert!(page.entries.iter().any(|entry| entry.selector == selector));
    let early =
        crate::wizard_creatures::catalog(&engine.game, Some(regions), Some(1), None).unwrap();
    assert!(!early.entries.iter().any(|entry| entry.selector == selector));
    for supplied in ["rat", selector] {
        let resolved =
            crate::wizard_creatures::resolve(&engine.game, regions, supplied, Some(2)).unwrap();
        assert!(
            matches!(resolved, crate::wizard_creatures::ResolvedCreature::Population {
            profile, stage, archetype, selector: actual,
        } if profile == "baseline" && stage == "treasure" && archetype == "rat" && actual == selector)
        );
        assert!(
            crate::wizard_creatures::resolve(&engine.game, regions, supplied, Some(1)).is_err()
        );
    }
    exact_equal(&engine, &before);
}

#[test]
fn cluster_item_pool_cannot_bypass_reserved_retrieval_even_with_zero_chance() {
    let scenario = cluster_fixture(1);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["archetypes"]["amulet"] = json!({
        "name":"Amulet of Yendor","identity":"amulet-of-yendor","class":"amulet"
    });
    source["unique_retrieval"] = json!({
        "version":1,"protagonist":"selected_character","family":"dungeon",
        "minimum_depth":26,"archetype":"amulet","exit_anchor":"dungeon/1/up"
    });
    let candidate = |source| {
        Package::from_parts(
            serde_json::from_value(source).unwrap(),
            vec![],
            &original.rules_source,
        )
        .unwrap()
    };
    candidate(source.clone()).check().unwrap();
    let stage = &mut source["floor_activation"]["profiles"]["baseline"]["stages"][0];
    stage["chance"]["numerator"] = json!(0);
    stage["item_bands"][0]["choices"][0]["archetype"] = json!("amulet");
    let error = candidate(source).check().unwrap_err();
    assert!(
        format!("{error:?}").contains("Reserved retrieval item cannot appear in floor population")
    );
}

#[test]
fn room_cluster_item_actor_and_carried_roots_require_spatial_scope() {
    use crate::creature_authoring as a;
    use tor_simulation::rules as r;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let scenario = crate::scenario_package::load(&root, 7, None, true).unwrap();
    let original = scenario.package.as_ref().unwrap();
    // An otherwise ordinary unused authored region makes enabled source closure
    // observable through the existing crate-visible admission boundary.
    let unscoped = serde_json::from_value(json!({
        "id":10000000,"name":"unscoped source witness","size":[7,5,2]
    }))
    .unwrap();
    let mut base = Package::from_parts(
        original.manifest.clone(),
        vec![unscoped],
        &original.rules_source,
    )
    .unwrap();
    let mut document = base.rules().unwrap().document().clone();
    let mut light = document
        .abilities
        .iter()
        .find(|ability| ability.id.as_str() == "rogue.wand_nothing")
        .unwrap()
        .clone();
    light.id = tor_simulation::content_ids::AbilityId::new("fixture.room_light").unwrap();
    let r::StrikeSource::Authored { strikes } = &mut light.strikes else {
        panic!()
    };
    assert_eq!(strikes.len(), 1);
    assert!(strikes[0].on_success.is_empty());
    strikes[0].on_success.push(r::Effect::RoomLight);
    document.abilities.push(light);
    base.rules_document =
        crate::storage::schema::scenario::encode_rules_document(&document).unwrap();
    let rules = base.rules().unwrap();
    let mut item = base.manifest.archetypes["rogue.wand_nothing"].clone();
    item.identity = Some("fixture.room_light".into());
    item.appearance_pool = None;
    item.forms.clear();
    item.device.as_mut().unwrap().ability = "fixture.room_light".into();
    item.device.as_ref().unwrap().compile(&rules).unwrap();
    base.manifest
        .archetypes
        .insert("fixture.room_light".into(), item);
    base.validate_spatial_sources().unwrap();
    for source_role in ["item", "actor", "stock"] {
        let mut package = base.clone();
        package.validate_spatial_sources().unwrap();
        let mut actor = package.manifest.archetypes["rogue.hobgoblin"].clone();
        actor.identity = Some("fixture.cluster_foe".into());
        if source_role == "actor" {
            let creature = actor.creature.as_mut().unwrap();
            let mut species = package.manifest.creatures.species[&creature.species].clone();
            species.modifiers.push(a::Modifier {
                cancellable: false,
                scope: a::Scope::Always,
                op: a::Op::GrantAbility {
                    ability: "fixture.room_light".into(),
                },
            });
            creature.species = "fixture.cluster_foe".into();
            package
                .manifest
                .creatures
                .species
                .insert(creature.species.clone(), species);
            let mut body = package.manifest.creatures.body_packages
                [creature.body_package.as_ref().unwrap()]
            .clone();
            body.species = creature.species.clone();
            body.polymorph_eligible = false;
            creature.body_package = Some("fixture.cluster_body".into());
            package
                .manifest
                .creatures
                .body_packages
                .insert("fixture.cluster_body".into(), body);
        }
        package
            .manifest
            .archetypes
            .insert("fixture.cluster_foe".into(), actor);
        package.validate_spatial_sources().unwrap();
        let index = package.index(7).unwrap();
        let definition = index.standalone_creature("fixture.cluster_foe").unwrap();
        assert!(
            definition.actor.build.is_some(),
            "actual prepared creature: {source_role}"
        );
        let mut choice = serde_json::json!({"archetype":"fixture.cluster_foe","weight":1});
        let plan = package.manifest.floor_activation.as_mut().unwrap();
        if source_role == "stock" {
            plan.carried_item_pools.insert(
                "fixture.cluster_stock".into(),
                serde_json::from_value(serde_json::json!({"version":1,"choices":[{
                    "archetype":"fixture.room_light","weight":1,"quantity":1
                }]}))
                .unwrap(),
            );
            choice["carried_loot"] = serde_json::json!({"version":1,"pool":"fixture.cluster_stock",
                    "chance":{"numerator":0,"denominator":1}});
        }
        let stage = serde_json::from_value(serde_json::json!({
                "kind":"room_cluster","id":"fixture.cluster","version":1,
                "chance":{"numerator":0,"denominator":1},"item_count":[1,1],"actor_count":[1,1],
                "actor_depth_offset":1,"quantity":1,
                "item_bands":[{"depth":{"kind":"open","minimum":1},"choices":[{
                    "archetype":if source_role == "item" { "fixture.room_light" } else { "rogue.ration" },"weight":1
                }]}],
                "actor_bands":[{"depth":{"kind":"open","minimum":2},"choices":[choice]}]
            })).unwrap();
        let profile = plan.families["dungeon"].clone();
        plan.profiles.get_mut(&profile).unwrap().stages.push(stage);
        let error = package.validate_spatial_sources().unwrap_err();
        assert!(
            format!("{error:?}").contains(
                "Enabled spatial effect requires canonical spatial scope for every authored region"
            ),
            "reachable cluster source: {source_role}: {error:?}"
        );
        package
            .manifest
            .floor_activation
            .as_mut()
            .unwrap()
            .profiles
            .get_mut(&profile)
            .unwrap()
            .stages
            .pop()
            .expect("the cluster source root was added");
        package.validate_spatial_sources().unwrap();
    }
}
