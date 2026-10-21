//! One host composition: cache residency cannot change a paid theft commitment.
//! Supplemental frozen-plan closure is owned by simulation's focused theft tests.
use super::*;
use tor_simulation::{content_ids::AbilityId, RegionState, RegionTransition};

const SENTINEL: u64 = 2;
const THIEF: u64 = 3;
const BLOCKER: u64 = 4;
// Beyond both sight-activated regions and their ordinary neighbor load pins.
const DISTANT_MEMBER: u64 = 5;

fn scenario() -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let production = crate::scenario_package::resolve_source(&root).unwrap();
    let rules = std::fs::read_to_string(root.join(&production.manifest.ruleset)).unwrap();
    let mut source = serde_json::to_value(&production.manifest).unwrap();
    source["id"] = json!("theft-residency");
    source["characters"].as_array_mut().unwrap().truncate(1);
    source["characters"][0]["known_identities"] = json!([]);
    source["characters"][0]["loadout"] =
        json!([{"archetype":"rogue.restore_strength","quantity":1}]);
    source["characters"][0]["creature"]["nutrition"] = json!({"kind":"exempt"});
    source["characters"][0]["creature"]["future_growth"] = json!({"kind":"disabled"});
    // Both real checks still roll. An explicitly vulnerable fixture target
    // makes contact and the separate Cognitive rider deterministic, retaining
    // N's complete natural strike, TR2/TR3 purchases and relocation rules.
    source["creatures"]["species"]["rogue.human"]["modifiers"]
        .as_array_mut()
        .unwrap()
        .extend([
            json!({"scope":"always","op":{"kind":"set",
                "stat":{"kind":"defense","defense":"physical"},"value":0}}),
            json!({"scope":"always","op":{"kind":"set",
                "stat":{"kind":"defense","defense":"cognitive"},"value":0}}),
        ]);
    // from_parts consumes a resolved Manifest; lower the authoring preset with
    // the production conversion before serializing the immutable recipe.
    let room = crate::generation_recipe::GridRoomsPreset {
        version: 1,
        room_width: [7, 7],
        room_height: [5, 5],
        extra_connections: [0, 0],
        room_omissions: None,
        lighting: None,
        secret_entrances: None,
        hidden_traps: None,
    }
    .to_recipe()
    .unwrap();
    source["generation_recipes"] = json!({"room": room});
    source["procedural_worlds"] = json!({"dungeon":{
        "kind":"grid_chain","version":1,"base_region":1,"grid":[5,1],
        "size":[9,7,2],"recipe":"room","domain":{"kind":"open","max_depth":100}
    }});
    source["archetypes"]["marker"] = json!({"name":"residency marker"});
    source["floor_activation"] = json!({"version":1,"protagonist":"selected_character",
    "families":{"dungeon":"baseline"},"profiles":{"baseline":{"version":1,"stages":[
        {"kind":"items","id":"marker","version":1,"count":[1,1],"quantity":1,
         "bands":[{"depth":{"kind":"open","minimum":1},
            "choices":[{"archetype":"marker","weight":1}]}]}
    ]}}});
    let package =
        Package::from_parts(serde_json::from_value(source).unwrap(), vec![], &rules).unwrap();
    Scenario {
        seed: 42,
        actors: vec![],
        regions: 0,
        workload_version: None,
        package: Some(Arc::new(package)),
        streaming: Some(crate::regions::Streaming {
            active_radius: 0,
            load_radius: 0,
        }),
    }
}

fn position(region: u64, x: i32) -> Position {
    Position {
        region,
        x,
        y: 3,
        z: 0,
    }
}

fn setup(engine: &mut Engine) {
    wizard(
        engine,
        WizardOperation::Teleport {
            actor: ActorId(1),
            position: position(1, 4),
        },
    );
    let sentinel = wizard(
        engine,
        WizardOperation::SpawnActor {
            position: position(DISTANT_MEMBER, 4),
            turn_ticks: 100,
        },
    );
    assert!(matches!(
        sentinel.entry.content,
        JournalContent::Wizard {
            result: WizardResult::ActorSpawned {
                actor: ActorId(SENTINEL)
            },
            ..
        }
    ));
    let thief = wizard(
        engine,
        WizardOperation::SpawnCreature {
            selector: "archetype:rogue.nymph".into(),
            position: position(1, 5),
            depth: None,
        },
    );
    assert!(matches!(
        thief.entry.content,
        JournalContent::Wizard {
            result: WizardResult::CreatureSpawned {
                actor: ActorId(THIEF),
                ..
            },
            ..
        }
    ));
    let blocker = wizard(
        engine,
        WizardOperation::SpawnActor {
            position: position(1, 3),
            turn_ticks: 100,
        },
    );
    assert!(matches!(
        blocker.entry.content,
        JournalContent::Wizard {
            result: WizardResult::ActorSpawned {
                actor: ActorId(BLOCKER)
            },
            ..
        }
    ));
    act(engine, 1, "yield-to-thief", Action::Wait);
    assert_eq!(engine.next_actor(), Some(ActorId(THIEF)));
    assert_eq!(
        engine.game.region_state(RegionId(DISTANT_MEMBER)),
        Some(RegionState::Detached)
    );
    assert!(!engine
        .game
        .loaded_actor_ids()
        .any(|actor| actor == SimActor(SENTINEL)));
}

fn act(engine: &mut Engine, actor: u64, request: &str, action: Action) {
    engine
        .command(
            "player",
            "theft-residency",
            ActorId(actor),
            request,
            &engine.branch().clone(),
            Command::Act {
                expected_revision: engine.revision(ActorId(actor)).unwrap(),
                action,
            },
        )
        .unwrap();
}

/// Test-only cache setup/readback: attach already-published records frozen. It
/// creates no action, active region, population entry, RNG roll or elapsed time.
/// Only the distant member is added to the existing ordinary load closure.
/// The cold arm does not call this before ordinary theft admission.
fn attach_frozen_member(engine: &mut Engine) {
    let before = engine.game.tick();
    let proposed = RegionTransition {
        active: BTreeSet::from([RegionId(1)]),
        loaded: engine
            .game
            .loaded_regions()
            .chain(std::iter::once(RegionId(DISTANT_MEMBER)))
            .collect(),
    };
    engine
        .game
        .transition_regions(&proposed, engine.regions.as_mut().unwrap())
        .unwrap();
    assert_eq!(engine.game.tick(), before);
    assert_eq!(
        engine.game.region_state(RegionId(DISTANT_MEMBER)),
        Some(RegionState::Frozen)
    );
}

fn actor_state(engine: &Engine, actor: u64) -> Value {
    let saved = authority(engine);
    let pool = saved["game"]["actors"].as_u64().unwrap() as usize;
    let state = saved["shared"]["actors"][pool][actor.to_string()].clone();
    assert!(state.is_object(), "positive control: actor is loaded");
    let lifecycle = saved["game"]["lifecycle"].as_u64().unwrap() as usize;
    let stamp = &saved["shared"]["lifecycles"][lifecycle]["stamps"][actor.to_string()];
    assert!(
        stamp.is_u64(),
        "positive control: unrelated actor is frozen"
    );
    json!({"actor":state, "freeze_stamp":stamp,
        "preparation":engine.game.preparation(SimActor(actor))})
}

fn entry_receipts(engine: &Engine) -> Value {
    json!({"population":decisions(engine), "retrieval":engine.game.unique_retrieval_state()})
}

fn start_theft(engine: &mut Engine) -> Value {
    let tick = engine.game.tick();
    let receipts = entry_receipts(engine);
    let rng = authority(engine)["game"]["rules_rng"].clone();
    act(
        engine,
        THIEF,
        "paid-theft",
        Action::UseAbility {
            id: AbilityId::new("filch_charm").unwrap(),
            invocation: crate::actions::AbilityInvocation {
                aim: crate::actions::AbilityAim::ActorTarget {
                    target: SimActor(1),
                },
                weapon: crate::actions::WeaponSelection::Default,
            },
            supplies: Default::default(),
        },
    );
    assert_eq!(
        engine.game.tick(),
        tick,
        "host admission only loads missing members"
    );
    assert_eq!(
        entry_receipts(engine),
        receipts,
        "residency is not a floor entry"
    );
    assert_eq!(
        authority(engine)["game"]["rules_rng"],
        rng,
        "admission does not resolve checks"
    );
    assert_eq!(
        engine.game.region_state(RegionId(DISTANT_MEMBER)),
        Some(RegionState::Frozen)
    );
    assert_eq!(engine.next_actor(), Some(ActorId(BLOCKER)));
    assert!(engine
        .game
        .observe(SimActor(THIEF))
        .unwrap()
        .inventory
        .is_empty());
    let preparation = engine.game.preparation(SimActor(THIEF)).unwrap();
    assert!(preparation.active && preparation.remaining > 0);
    serde_json::to_value(preparation).unwrap()
}

fn finish_theft(engine: &mut Engine) -> Value {
    act(engine, BLOCKER, "release-paid-theft", Action::Wait);
    assert!(engine.game.preparation(SimActor(THIEF)).is_none());
    attach_frozen_member(engine);
    let source = engine.game.observe(SimActor(THIEF)).unwrap();
    let victim = engine.game.observe(SimActor(1)).unwrap();
    assert_eq!(
        source.inventory.len(),
        1,
        "one real magical unit was transferred"
    );
    assert!(victim.inventory.is_empty());
    assert!(!source.inventory[0].identified);
    assert_ne!(
        source.location,
        Location {
            region: RegionId(1),
            position: WorldPosition { x: 5, y: 3, z: 0 }
        }
    );
    let saved = authority(engine);
    let pool = saved["game"]["items"].as_u64().unwrap() as usize;
    let item = &saved["shared"]["items"][pool][source.inventory[0].id.0.to_string()];
    assert!(item.is_object());
    json!({"landing":source.location, "loot":item, "item_id":source.inventory[0].id.0,
        "rules_rng":saved["game"]["rules_rng"], "tick":engine.game.tick()})
}

#[test]
fn paid_theft_matches_warm_and_evicted_floor_after_source_free_checkpoint_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("theft.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut warm = Engine::memory(scenario()).unwrap();
    let mut cold = Engine::open_with_policy(&path, scenario(), policy.clone()).unwrap();
    setup(&mut warm);
    setup(&mut cold);
    let receipts = entry_receipts(&cold);
    assert_eq!(entry_receipts(&warm), receipts);
    attach_frozen_member(&mut warm);
    let sentinel = actor_state(&warm, SENTINEL);
    assert!(
        !cold
            .game
            .loaded_regions()
            .any(|region| region == RegionId(DISTANT_MEMBER)),
        "ordinary neighbor residency must not prewarm the distant cold member"
    );
    assert_eq!(
        start_theft(&mut warm),
        start_theft(&mut cold),
        "immutable paid plan and progress are independent of cache residency"
    );
    assert!(
        cold.game
            .loaded_regions()
            .any(|region| region == RegionId(DISTANT_MEMBER)),
        "ordinary paid admission must newly load the distant member"
    );
    assert_eq!(actor_state(&warm, SENTINEL), sentinel);
    assert_eq!(
        actor_state(&cold, SENTINEL),
        sentinel,
        "ordinary load-only admission does not thaw unrelated work or clocks"
    );
    let before_reopen = authority(&cold);
    let plan = serde_json::to_value(cold.game.preparation(SimActor(THIEF)).unwrap()).unwrap();
    cold.flush().unwrap();
    drop(cold);
    // Package::from_parts has no source directory. The unrelated fallback must
    // not supply geometry, rules, room metadata or theft operands on recovery.
    let mut reopened = Engine::open_with_policy(&path, Scenario::two_room(0), policy).unwrap();
    assert!(reopened.recovery_profile().checkpoint_sequence > 0);
    assert_eq!(authority(&reopened), before_reopen);
    assert_eq!(
        serde_json::to_value(reopened.game.preparation(SimActor(THIEF)).unwrap()).unwrap(),
        plan
    );
    assert_eq!(
        finish_theft(&mut warm),
        finish_theft(&mut reopened),
        "landing, transferred item, quantity, RNG and scheduler time must match"
    );
    assert_eq!(actor_state(&warm, SENTINEL), sentinel);
    assert_eq!(actor_state(&reopened, SENTINEL), sentinel);
    assert_eq!(entry_receipts(&warm), receipts);
    assert_eq!(entry_receipts(&reopened), receipts);
}
