//! One owning wizard command includes both preload continuation and teleport.
use super::*;
use crate::scenario_package::RegionDef;
use tor_simulation::floor_activation::EntryCause;

fn preload_fixture() -> (Scenario, u64, u64) {
    let mut scenario = at_depth(25);
    scenario.streaming = Some(crate::regions::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["procedural_worlds"]["dungeon"]["base_region"] = json!(100);
    source["procedural_worlds"]["dungeon"]["grid"] = json!([1, 1]);
    source["characters"][0]["anchor"] = json!("1/start");
    let empty = |offset| {
        json!({"offset":offset,"unlock":null,"check":null,
        "on_success":[],"on_failure":[],"descriptors":[],"riders":[]})
    };
    source["creatures"] = json!({"species":{"subject":{
        "kind":"humanoid",
        "behavior":{"disposition":"passive","idle":"wait","memory_ticks":1000},
        "attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,
            "strikes":[empty(0)]},
        "modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"preload_sequence"}}]
    }}});
    source["factions"] = json!({"neutral":[]});
    source["ai_profiles"] =
        json!({"idle":{"care":false,"equipment":false,"loot":false,"flee_percent":0}});
    let creature = json!({"species":"subject","name":"subject","faction":"neutral",
        "binding":"intellect","starting_skills":["deception","persuasion"],
        "restoration":"authored","nutrition":{"kind":"exempt"},"hit_dice":[{"source":"racial"}]});
    source["characters"][0]["creature"] = creature.clone();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let mut ability =
        serde_json::to_value(rules.abilities.iter().find(|a| a.basic_attack).unwrap()).unwrap();
    ability["id"] = json!("preload_sequence");
    ability["basic_attack"] = json!(false);
    ability["cost"] = Value::Null;
    ability["reach"] = json!({"kind":"sight","range":6});
    ability["timing"] =
        json!({"source":{"kind":"fixed","prepare":10,"recover":20},"scaled_by":null});
    ability["strikes"] = json!({"kind":"authored","strikes":[empty(0),empty(20)]});
    rules
        .abilities
        .push(serde_json::from_value(ability).unwrap());
    let regions: Vec<RegionDef> = vec![
        serde_json::from_value(json!({"id":1,"name":"Split room","size":[12,7,1],
            "gravity":[0,0,0],"anchors":{"start":[9,1,0],"remote":[4,0,0]},
            "walls":(0..7).map(|y| [8,y,0]).collect::<Vec<_>>(),
            "portals":[{"at":[4,0,0],"direction":"north","to":"8/near"}],
            "actors":[{"id":2,"at":[4,4,0],"turn_ticks":100,"controller":"external","creature":creature}]})).unwrap(),
        serde_json::from_value(json!({"id":8,"name":"Remote target","size":[12,7,1],
            "gravity":[0,0,0],"anchors":{"near":[4,6,0]},
            "portals":[{"at":[4,6,0],"direction":"south","to":"1/remote"}],
            "actors":[{"id":3,"at":[4,5,0],"turn_ticks":10,"controller":"ai","ai":"idle","creature":creature}]})).unwrap(),
    ];
    let package = Package::from_parts(
        serde_json::from_value(source).unwrap(),
        regions,
        &toml::to_string(&rules).unwrap(),
    )
    .unwrap();
    let family = &package.manifest.procedural_worlds["dungeon"];
    let deep = family.group("dungeon", scenario.seed, 26).unwrap().members[0];
    let deeper = family.group("dungeon", scenario.seed, 27).unwrap().members[0];
    scenario.package = Some(Arc::new(package));
    (scenario, deep, deeper)
}

#[test]
fn wizard_preload_physical_entry_and_teleport_share_one_saved_retrieval_batch() {
    let (scenario, deep, deeper) = preload_fixture();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("preload-entries.db");
    let mut engine = Engine::open_with_policy(
        &path,
        scenario,
        crate::SavePolicy {
            checkpoint_interval: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let position = |region, x, y| Position { region, x, y, z: 0 };
    // Source-backed, ordinary external caster and autonomous target. Only the
    // selected character and caster observe; target8 has no observer point.
    assert!(engine.is_ai(ActorId(3)));
    assert_eq!(engine.game.reference_points().count(), 2);
    for region in [deep, deeper] {
        wizard(
            &mut engine,
            WizardOperation::SetGravity {
                region,
                vector: [0; 3],
            },
        );
        for x in 0..8 {
            wizard(
                &mut engine,
                WizardOperation::SetWall {
                    position: position(region, x, 1),
                    wall: false,
                },
            );
        }
    }
    wizard(
        &mut engine,
        WizardOperation::ConnectPortal {
            from: position(1, 11, 1),
            direction: Direction::East,
            to: position(deep, 0, 1),
            rotation: 0,
            width: 1,
            height: 1,
        },
    );
    wizard(
        &mut engine,
        WizardOperation::SetVelocity {
            actor: ActorId(1),
            velocity: [8192, 0, 0],
        },
    );
    act(&mut engine, Action::Wait).unwrap();
    assert_eq!(engine.next_actor(), Some(ActorId(2)));
    let admitted = engine
        .command(
            "player",
            "retrieval-test",
            ActorId(2),
            "preload-cast",
            &engine.branch().clone(),
            Command::AdmitIntention {
                expected_boundary: engine.causal_boundary(ActorId(2)).unwrap(),
                action: Action::UseAbility {
                    id: tor_simulation::content_ids::AbilityId::new("preload_sequence").unwrap(),
                    invocation: crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: SimActor(3),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    },
                    supplies: Default::default(),
                },
            },
        )
        .unwrap();
    engine.execute_next_intention().unwrap().unwrap();
    assert_eq!(engine.next_actor(), Some(ActorId(3)));
    engine.admit_ai(ActorId(3)).unwrap();
    let target_wait = engine.execute_next_intention().unwrap().unwrap();
    assert!(matches!(
        target_wait.entry.content,
        JournalContent::IntentionStarted {
            action: Action::Wait,
            ..
        }
    ));
    assert_eq!(engine.game.tick(), 10);
    assert_eq!(
        engine.game.preparation(SimActor(2)).unwrap().progress,
        tor_simulation::combat::PreparationProgress::Ability {
            linked_hold: None,
            next_strike: 1,
            successful_primaries: 0,
            attempted_riders: [0; 2]
        }
    );
    assert!(retrieval(&engine).item.is_none());
    // A real visibility edit blocks the external caster's sight, while its
    // partially paid sequence continues to pin the remote ready target.
    for x in 0..8 {
        wizard(
            &mut engine,
            WizardOperation::SetWall {
                position: position(1, x, 2),
                wall: true,
            },
        );
        assert_eq!(engine.game.tick(), 10);
    }
    assert_eq!(engine.game.tick(), 10);
    assert_eq!(
        engine.game.region_state(RegionId(8)),
        Some(tor_simulation::RegionState::Active)
    );
    engine
        .command(
            "player",
            "retrieval-test",
            ActorId(2),
            "preload-cancel",
            &engine.branch().clone(),
            Command::CancelIntention {
                expected_boundary: engine.causal_boundary(ActorId(2)).unwrap(),
                admission: admitted.entry.id,
                step: None,
            },
        )
        .unwrap();
    assert_eq!(engine.game.tick(), 10);
    assert_eq!(
        engine.game.region_state(RegionId(8)),
        Some(tor_simulation::RegionState::Active)
    );
    assert!(retrieval(&engine).entries.is_empty());
    restored_both_ways(&engine);

    let count = engine.archive.records.len();
    wizard(
        &mut engine,
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: position(deeper, 0, 1),
        },
    );
    assert_eq!(engine.archive.records.len(), count + 1);
    assert_ne!(
        engine.game.region_state(RegionId(8)),
        Some(tor_simulation::RegionState::Active)
    );
    assert_eq!(retrieval(&engine).entry_count, 2);
    let record = engine.archive.records.last().unwrap();
    assert_eq!(
        record.retrievals.len(),
        2,
        "preload Created and later teleport relocation belong to the same record"
    );
    assert_eq!(record.retrievals[0].depth, 26);
    assert_eq!(record.retrievals[0].cause, EntryCause::Physics);
    assert!(matches!(
        record.retrievals[0].result,
        RetrievalResult::Created { .. }
    ));
    assert_eq!(record.retrievals[1].depth, 27);
    assert_eq!(record.retrievals[1].cause, EntryCause::Teleport);
    assert!(matches!(
        record.retrievals[1].result,
        RetrievalResult::Relocated { .. }
    ));
    assert_eq!(
        record.retrievals[0].result.item(),
        record.retrievals[1].result.item()
    );
    assert!(record
        .retrievals
        .iter()
        .all(|entry| entry.tick >= record.before_tick && entry.tick <= record.after_tick));
    restored_both_ways(&engine);
    let before = engine.game.clone();
    engine.flush().unwrap();
    drop(engine);
    let recovered = Engine::open(&path, Scenario::two_room(999)).unwrap();
    assert!(recovered.recovery_profile().checkpoint_sequence > 0);
    assert_eq!(recovered.game, before);
}
