//! One bounded host composition for P/X disclosure, MI2 ownership and persistence.
//! Production Phantom/Xeroc builds and MI1/MI2 definitions are unchanged. Only
//! the observers, guaranteed lethal human strike and five-room geometry are
//! synthetic, so this is a persistence witness rather than a combat tuning test.
use super::*;
use crate::scenario_package::Package;
use serde_json::{json, Value};
use tor_simulation::{
    combat::DisclosedCombatEvent,
    content_ids::AbilityId,
    perception::{ActorDisclosure, ActorReferenceKind},
    RegionState, RegionTransition,
};
use tor_world::RegionId;

// This module is registered only under cfg(test). Keep the authoritative probe
// in Engine's owning module rather than exposing its Game to session clients.
impl Engine {
    pub(crate) fn assert_perception_subject_idle(&self, actor: ActorId) {
        assert!(
            self.game.preparation(SimActor(actor.0)).is_none(),
            "private recognition must not start subject preparation"
        );
    }
}

const OWNER: u64 = 1;
const DETECTOR: u64 = 2;
const PHANTOM: u64 = 3;
const XEROC: u64 = 4;
const VICTIM: u64 = 5;
// Immediate neighbors remain loaded even with zero radii. This member is beyond
// both the local sight closure and the ordinary neighbor pins of region one.
const DISTANT: u64 = 5;

fn scenario() -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let production = crate::scenario_package::resolve_source(&root).unwrap();
    let rules = std::fs::read_to_string(root.join(&production.manifest.ruleset)).unwrap();
    let mut source = serde_json::to_value(production.manifest).unwrap();
    source["id"] = json!("perception-persistence");
    source["characters"].as_array_mut().unwrap().truncate(1);
    source["characters"][0]["known_identities"] = json!([]);
    source["characters"][0]["loadout"] = json!([{ "archetype":"rogue.mace", "quantity":1 }]);
    source["characters"][0]["creature"]["nutrition"] = json!({"kind":"exempt"});
    source["characters"][0]["creature"]["future_growth"] = json!({"kind":"disabled"});
    // Retain ordinary basic-melee resolution and its contact targeting, but
    // remove hit/damage luck from the lethal historical-disclosure witness.
    source["creatures"]["species"]["rogue.human"]["melee"] = json!({
        "skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,
        "strikes":[{"offset":0,"on_success":[{"kind":"damage","damage":{
            "kind":"components","components":[{"category":"impact",
                "amount":{"kind":"fixed","amount":10000}}]}}],
            "on_failure":[],"descriptors":[],"riders":[]}]});
    source["creatures"]["templates"]["fixture_detection"] = json!({
        "priority":0,"modifiers":[{"scope":"always","op":{"kind":"perception",
            "value":{"kind":"see_invisible","enabled":true}}}]});
    source["creatures"]["templates"]["fixture_paid_search"] = json!({
        "priority":0,"modifiers":[{"scope":"always","op":{"kind":"add","stat":{"kind":"ability",
            "ability":"rogue.search","stat":"check_bonus"},"value":50}}]});
    source["characters"][0]["creature"]["templates"] = json!(["fixture_paid_search"]);
    let mut observer = source["characters"][0]["creature"].clone();
    observer["name"] = json!("fixture observer");
    observer["templates"] = json!(["fixture_detection"]);
    source["archetypes"]["fixture.observer"] = json!({"turn_ticks":100,
        "body":source["characters"][0]["body"],"creature":observer});
    let room = crate::generation_recipe::GridRoomsPreset {
        version: 1,
        room_width: [7, 7],
        room_height: [5, 5],
        extra_connections: [0, 0],
        lighting: None,
        secret_entrances: None,
        hidden_traps: None,
    }
    .to_recipe()
    .unwrap();
    source["generation_recipes"] = json!({"room":room});
    source["procedural_worlds"] = json!({"dungeon":{
        "kind":"grid_chain","version":1,"base_region":1,"grid":[5,1],
        "size":[9,7,2],"recipe":"room","domain":{"kind":"open","max_depth":100}}});
    // No unrelated population, survival or retrieval work participates here.
    source["floor_activation"] = Value::Null;
    source["unique_retrieval"] = Value::Null;
    source["objective"] = Value::Null;
    let package =
        Package::from_parts(serde_json::from_value(source).unwrap(), vec![], &rules).unwrap();
    assert!(!package.sources.has_directory());
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

fn disclosure(game: &Game, observer: SimActor, subject: SimActor) -> ActorDisclosure {
    let view = game.observe(observer).unwrap();
    if let Some(actor) = view.visible_actors.iter().find(|actor| actor.id == subject) {
        actor.disclosure
    } else if view
        .apparent_objects
        .iter()
        .any(|object| object.subject == subject)
    {
        ActorDisclosure::ApparentObject
    } else {
        ActorDisclosure::Hidden
    }
}

fn position(region: u64, x: i32, y: i32) -> Position {
    Position { region, x, y, z: 0 }
}

fn command(engine: &mut Engine, actor: u64, command: Command) -> CommandResult {
    // Journal IDs differ between independent saves; gameplay and persisted
    // simulation authority must not. The monotonically bounded request key is
    // unique within each arm without introducing another random input.
    let request = format!("perception-{}", engine.archive.records.len());
    engine
        .command(
            "player",
            "perception-persistence",
            ActorId(actor),
            &request,
            &engine.branch().clone(),
            command,
        )
        .unwrap()
}

fn wizard(engine: &mut Engine, operation: WizardOperation) {
    engine.enable_wizard().unwrap();
    command(
        engine,
        OWNER,
        Command::Wizard {
            expected_revision: engine.revision(ActorId(OWNER)).unwrap(),
            operation,
        },
    );
}

fn teleport(engine: &mut Engine, actor: u64, region: u64, x: i32, y: i32) {
    wizard(
        engine,
        WizardOperation::Teleport {
            actor: ActorId(actor),
            position: position(region, x, y),
        },
    );
}

fn spawn(engine: &mut Engine, selector: &str, expected: u64, region: u64, x: i32, y: i32) {
    wizard(
        engine,
        WizardOperation::SpawnCreature {
            selector: format!("archetype:{selector}"),
            position: position(region, x, y),
            depth: None,
        },
    );
    assert!(
        engine.game.creature(SimActor(expected)).is_some()
            || engine.game.known_actor_region(SimActor(expected)) == Some(RegionId(region))
    );
}

fn admit_attack(engine: &mut Engine, actor: u64, target: u64, contact: bool) {
    command(
        engine,
        actor,
        Command::AdmitIntention {
            expected_revision: engine.revision(ActorId(actor)).unwrap(),
            action: Action::UseAbility {
                id: AbilityId::new("basic_melee").unwrap(),
                invocation: if contact {
                    crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ContactTarget {
                            target: SimActor(target),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    }
                } else {
                    crate::actions::AbilityInvocation {
                        aim: crate::actions::AbilityAim::ActorTarget {
                            target: SimActor(target),
                        },
                        weapon: crate::actions::WeaponSelection::Default,
                    }
                },
                supplies: Default::default(),
            },
        },
    );
}

fn step(engine: &mut Engine) {
    if engine.execute_next_intention().unwrap().is_some() {
        return;
    }
    let actor = engine.next_actor().expect("bounded external fixture turn");
    command(
        engine,
        actor.0,
        Command::Act {
            expected_revision: engine.revision(actor).unwrap(),
            action: Action::Wait,
        },
    );
}

fn authority(engine: &Engine) -> Value {
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let game = engine.game.checkpoint(&mut shared);
    json!({"game":game,"shared":shared})
}

fn guise(engine: &Engine, actor: u64) -> Value {
    let saved = authority(engine);
    let pool = saved["game"]["actors"].as_u64().unwrap() as usize;
    let state = &saved["shared"]["actors"][pool][actor.to_string()];
    assert!(state.is_object(), "the claim witness must be loaded");
    state["guise"].clone()
}

fn claim(engine: &Engine) -> Value {
    let preparation = engine.game.preparation(SimActor(XEROC)).unwrap();
    json!({"guise":guise(engine, XEROC),"plan":preparation.plan,
        "remaining":preparation.remaining,"origin":preparation.origin_intention()})
}

fn views(engine: &Engine) -> [tor_simulation::Observation; 2] {
    [OWNER, DETECTOR].map(|actor| engine.game.observe(SimActor(actor)).unwrap())
}

fn history(engine: &Engine) -> [Vec<DisclosedCombatEvent>; 2] {
    views(engine).map(|view| view.combat.unwrap().events)
}

fn assert_exact(a: &Engine, b: &Engine) {
    assert_eq!(a.game, b.game);
    assert_eq!(
        authority(a),
        authority(b),
        "saved authority excludes transient proof epochs"
    );
    assert_eq!(views(a), views(b));
    for actor in [ActorId(OWNER), ActorId(DETECTOR)] {
        assert_eq!(
            a.revision_view(actor).unwrap(),
            b.revision_view(actor).unwrap()
        );
    }
}

fn assert_mundane_and_apparent_mace_rows(engine: &Engine) {
    for observer in [OWNER, DETECTOR] {
        let (view, scene) = engine.game.observe_scene(SimActor(observer)).unwrap();
        assert!(view.inventory.is_empty());
        let scope = crate::wire_adapter::TargetScope::new(Uuid::from_u128(7), SimActor(observer));
        let encoded = serde_json::to_value(crate::adapt::observation(
            view,
            scene,
            "perception-fixture",
            true,
            &|_| [None, None, None],
            &scope,
        ))
        .unwrap();
        let rows: Vec<_> = encoded["ground_items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["item"]["asset"] == "item.weapon.mace")
            .cloned()
            .collect();
        assert_eq!(rows.len(), 2, "actual ground mace and actual active guise");
        let mut ordinary = rows[0].clone();
        let mut apparent = rows[1].clone();
        let ordinary_id = ordinary["item"].as_object_mut().unwrap().remove("id");
        let apparent_id = apparent["item"].as_object_mut().unwrap().remove("id");
        assert_ne!(
            ordinary_id, apparent_id,
            "item and apparent-object target domains differ"
        );
        assert_eq!(
            ordinary, apparent,
            "without prior item knowledge, complete ground rows differ only in opaque target tokens"
        );
    }
}

fn prepare_distant_ambush(engine: &mut Engine) -> Option<Value> {
    teleport(engine, OWNER, 1, 4, 3);
    spawn(engine, "fixture.observer", DETECTOR, 1, 4, 4);
    spawn(engine, "rogue.phantom", PHANTOM, 1, 5, 3);
    teleport(engine, OWNER, DISTANT, 4, 3);
    let mace = engine.game.observe(SimActor(OWNER)).unwrap().inventory[0]
        .id
        .0;
    command(
        engine,
        OWNER,
        Command::Act {
            expected_revision: engine.revision(ActorId(OWNER)).unwrap(),
            action: Action::Drop {
                item: mace,
                quantity: None,
            },
        },
    );
    teleport(engine, OWNER, 1, 4, 3);
    spawn(engine, "rogue.xeroc", XEROC, DISTANT, 4, 3);
    spawn(engine, "fixture.observer", VICTIM, DISTANT, 5, 3);
    teleport(engine, OWNER, DISTANT, 3, 3);
    teleport(engine, DETECTOR, DISTANT, 4, 4);
    // This contract needs failed ordinary initial Perception, not a guarantee
    // that See Invisible prevents recognition. Reject only that setup outcome;
    // all authority, ambush and persistence assertions below remain mandatory.
    if [OWNER, DETECTOR].into_iter().any(|observer| {
        disclosure(&engine.game, SimActor(observer), SimActor(XEROC))
            != ActorDisclosure::ApparentObject
    }) {
        return None;
    }
    for observer in [OWNER, DETECTOR] {
        assert_eq!(
            disclosure(&engine.game, SimActor(observer), SimActor(XEROC)),
            ActorDisclosure::ApparentObject,
            "genuine failed Perception preserves the apparent-object domain"
        );
    }
    assert_mundane_and_apparent_mace_rows(engine);
    assert_eq!(guise(engine, XEROC)["ambush"]["state"], "available");
    admit_attack(engine, XEROC, VICTIM, false);
    for _ in 0..16 {
        if engine.game.preparation(SimActor(XEROC)).is_some() {
            break;
        }
        step(engine);
    }
    let reserved = claim(engine);
    assert_eq!(reserved["guise"]["ambush"]["state"], "reserved");
    assert!(reserved["remaining"].as_u64().unwrap() > 0);
    assert!(
        !reserved["plan"]["ambush"].is_null(),
        "production MI2 owns the frozen edge"
    );
    // Starting the attack reveals X globally but preserves its one issued claim.
    for observer in [OWNER, DETECTOR] {
        assert_eq!(
            disclosure(&engine.game, SimActor(observer), SimActor(XEROC)),
            ActorDisclosure::Creature
        );
    }
    teleport(engine, OWNER, 1, 4, 3);
    teleport(engine, DETECTOR, 1, 4, 4);
    assert_eq!(
        engine.game.region_state(RegionId(DISTANT)),
        Some(RegionState::Detached)
    );
    assert!(!engine
        .game
        .loaded_actor_ids()
        .any(|actor| actor == SimActor(XEROC)));
    assert_eq!(
        disclosure(&engine.game, SimActor(OWNER), SimActor(PHANTOM)),
        ActorDisclosure::AnonymousContact
    );
    assert_eq!(
        disclosure(&engine.game, SimActor(DETECTOR), SimActor(PHANTOM)),
        ActorDisclosure::Creature
    );
    Some(reserved)
}

fn kill_anonymous_phantom(engine: &mut Engine) {
    admit_attack(engine, OWNER, PHANTOM, true);
    let mut verified_fixture_strike = false;
    for _ in 0..24 {
        if let Some(preparation) = engine.game.preparation(SimActor(OWNER)) {
            let tor_simulation::abilities::Plan::Ability(plan) = &preparation.plan else {
                panic!("fixture must prepare its ordinary basic attack");
            };
            assert_eq!(plan.strikes.len(), 1);
            assert!(
                plan.strikes[0].check.is_none(),
                "synthetic lethal witness must not depend on a lucky inherited check"
            );
            verified_fixture_strike = true;
        }
        if !engine.game.alive(SimActor(PHANTOM)) {
            break;
        }
        step(engine);
    }
    assert!(
        verified_fixture_strike,
        "inspect the actual frozen lethal fixture plan"
    );
    assert!(
        !engine.game.alive(SimActor(PHANTOM)),
        "guaranteed fixture strike resolved"
    );
    let events = history(engine);
    assert!(events[0].iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::Ability {
            target: Some(SimActor(PHANTOM)),
            target_reference: Some(ActorReferenceKind::Contact),
            ..
        }
    )));
    assert!(events[0].iter().any(|event| matches!(
        event,
        DisclosedCombatEvent::Died {
            actor: SimActor(PHANTOM),
            reference: ActorReferenceKind::Contact
        }
    )));
    assert!(
        views(engine)[0].ground_items.iter().any(|item| item
            .corpse
            .as_ref()
            .is_some_and(|corpse| corpse.actor == SimActor(PHANTOM))),
        "a separately visible corpse may identify without widening past events"
    );
    wizard(
        engine,
        WizardOperation::SetCreatureTemplate {
            actor: ActorId(OWNER),
            template: "fixture_detection".into(),
            enabled: true,
        },
    );
    assert!(
        engine
            .game
            .actor_perception(SimActor(OWNER))
            .unwrap()
            .see_invisible
    );
    assert_eq!(
        history(engine),
        events,
        "later detection cannot rewrite attack/death domains"
    );
}

/// Exercise the actual region RecordStore, first frozen and then active. This
/// test-only residency operation changes no reference point, command or rule.
fn attach_distant(engine: &mut Engine, active: bool) {
    let mut active_regions: BTreeSet<_> = engine
        .game
        .loaded_regions()
        .filter(|region| engine.game.region_state(*region) == Some(RegionState::Active))
        .collect();
    if active {
        active_regions.insert(RegionId(DISTANT));
    }
    let loaded = engine
        .game
        .loaded_regions()
        .chain(std::iter::once(RegionId(DISTANT)))
        .collect();
    engine
        .game
        .transition_regions(
            &RegionTransition {
                active: active_regions,
                loaded,
            },
            engine.regions.as_mut().unwrap(),
        )
        .unwrap();
    engine.observations = ObservationCache::default();
    assert_eq!(
        engine.game.region_state(RegionId(DISTANT)),
        Some(if active {
            RegionState::Active
        } else {
            RegionState::Frozen
        })
    );
}

#[test]
fn observer_domains_and_owned_ambush_survive_eviction_source_free_reopen_and_thaw() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("perception.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let base = scenario();
    let (source, mut warm, reserved) = (1..=64)
        .find_map(|seed| {
            let mut source = base.clone();
            source.seed = seed;
            let mut engine = Engine::memory(source.clone()).unwrap();
            prepare_distant_ambush(&mut engine).map(|reserved| (source, engine, reserved))
        })
        .expect("bounded genuine double passive failure before the owned ambush");
    let mut cold = Engine::open_with_policy(&path, source, policy.clone()).unwrap();
    assert_eq!(prepare_distant_ambush(&mut cold), Some(reserved.clone()));
    assert_exact(&warm, &cold);
    let before_queries = authority(&warm);
    for _ in 0..3 {
        for actor in [ActorId(OWNER), ActorId(DETECTOR)] {
            warm.state(actor).unwrap();
        }
    }
    cold.observations = ObservationCache::default();
    assert_eq!(
        authority(&warm),
        before_queries,
        "queries preserve RNG, clocks, guise and claims"
    );
    kill_anonymous_phantom(&mut warm);
    kill_anonymous_phantom(&mut cold);
    assert_exact(&warm, &cold);
    let events = history(&cold);
    let saved = authority(&cold);
    let saved_wire = [ActorId(OWNER), ActorId(DETECTOR)].map(|actor| cold.state(actor).unwrap());
    cold.flush().unwrap();
    // Synchronize the completed checkpoint receipt before deliberately evicting
    // durable region rows. No game/projection state is changed by this cache work.
    if let Some((on_disk, watermark)) = cold.store.as_ref().unwrap().take_written() {
        cold.regions.as_mut().unwrap().written(on_disk, watermark);
    }
    cold.regions.as_mut().unwrap().evict_durable();
    assert_eq!(cold.regions.as_ref().unwrap().resident_count(), 0);
    assert_eq!(authority(&cold), saved);
    drop(cold);
    // There is no source directory and this unrelated fallback cannot provide
    // the production P/X recipes, MI2 claim, geometry or observer receipts.
    let mut reopened = Engine::open_with_policy(&path, Scenario::two_room(999), policy).unwrap();
    assert!(reopened.recovery_profile().checkpoint_sequence > 0);
    assert_eq!(authority(&reopened), saved);
    assert_eq!(
        [ActorId(OWNER), ActorId(DETECTOR)].map(|actor| reopened.state(actor).unwrap()),
        saved_wire
    );
    assert_exact(&warm, &reopened);
    assert_eq!(history(&reopened), events);
    let tick = reopened.game.tick();
    let rng = saved["game"]["rules_rng"].clone();
    for engine in [&mut warm, &mut reopened] {
        attach_distant(engine, false);
        assert_eq!(
            claim(engine),
            reserved,
            "frozen load preserves exact MI2 owner and work"
        );
        assert_eq!(
            history(engine),
            events,
            "detached history never uses a new scene"
        );
        assert_eq!(engine.game.tick(), tick);
        assert_eq!(authority(engine)["game"]["rules_rng"], rng);
    }
    assert_exact(&warm, &reopened);
    for engine in [&mut warm, &mut reopened] {
        attach_distant(engine, true);
        assert_eq!(
            claim(engine),
            reserved,
            "thaw shifts clocks without reclaiming MI2"
        );
        assert_eq!(history(engine), events);
        assert_eq!(engine.game.tick(), tick);
        assert_eq!(authority(engine)["game"]["rules_rng"], rng);
    }
    assert_exact(&warm, &reopened);
}

#[path = "perception_recognition_persistence_tests.rs"]
mod recognition_tests;
