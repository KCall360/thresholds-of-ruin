//! Newly reconstructed server-level weighted actor/item receipt tests.
use super::*;

fn weighted_fixture() -> Scenario {
    let mut scenario = single_member_foe_fixture(true);
    let original = scenario.package.as_ref().unwrap();
    let mut source = serde_json::to_value(&original.manifest).unwrap();
    source["archetypes"]["alternate"] = json!({"name":"alternate marker"});
    source["archetypes"]["other_rat"] = source["archetypes"]["rat"].clone();
    let stages = source["floor_activation"]["profiles"]["baseline"]["stages"]
        .as_array_mut()
        .unwrap();
    stages[0]["bands"] = json!([
        {"depth":{"kind":"finite","minimum":1,"maximum":2},"choices":[
            {"archetype":"marker","weight":1},{"archetype":"alternate","weight":3}]},
        {"depth":{"kind":"open","minimum":4},"choices":[{"archetype":"alternate","weight":1}]}
    ]);
    stages[2]["bands"] = json!([
        {"depth":{"kind":"finite","minimum":2,"maximum":2},"choices":[
            {"archetype":"rat","weight":1},{"archetype":"other_rat","weight":3}]}
    ]);
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

#[test]
fn weighted_floor_population_is_entry_owned_and_replays_actor_and_item_choices() {
    let mut engine = Engine::memory(weighted_fixture()).unwrap();
    let initial = decisions(&engine);
    let game = engine.game.clone();
    engine.start_preloading();
    engine.settle_preloading();
    assert_eq!(
        engine.game, game,
        "preparation must not draw weighted content"
    );
    assert_eq!(decisions(&engine), initial);
    walk_to_port(&mut engine, 1, "down");
    assert_eq!(decisions(&engine), initial, "visibility is not entry");
    step(&mut engine, Direction::Down);
    let entered = floor(&engine, 2);
    for (ordinal, eligible) in [(0, ["marker", "alternate"]), (2, ["rat", "other_rat"])] {
        assert_eq!(entered["decisions"][ordinal]["result"]["kind"], "evaluated");
        assert!(eligible.contains(
            &entered["decisions"][ordinal]["result"]["attempts"][0]["content"]
                .as_str()
                .unwrap()
        ));
    }
    restored_both_ways(&engine);
    // Movement may leave an ordinary NPC decision due before the protagonist's
    // next action, including while returning through the stair landing.
    for direction in [Direction::Up, Direction::Down] {
        for _ in 0..8 {
            if engine.next_actor() == Some(ActorId(1)) {
                break;
            }
            let actor = engine.next_actor().expect("newborn decision is ready");
            assert!(engine.is_ai(actor));
            engine.advance_ai(actor).unwrap();
        }
        assert_eq!(engine.next_actor(), Some(ActorId(1)), "{direction:?}");
        step(&mut engine, direction);
    }
    assert_eq!(
        floor(&engine, 2),
        entered,
        "revisits retain weighted decisions"
    );
    restored_both_ways(&engine);
}

#[test]
fn weighted_floor_receipt_cannot_substitute_another_eligible_choice() {
    let engine = Engine::memory(weighted_fixture()).unwrap();
    let mut archive = engine.archive.clone();
    use tor_simulation::floor_activation::{PlacementDecision, StageResult};
    let StageResult::Evaluated { attempts, .. } =
        &mut archive.initial_activations[0].decisions[0].result
    else {
        panic!()
    };
    let PlacementDecision::Placed { content, .. } = &mut attempts[0] else {
        panic!()
    };
    *content = if content == "marker" {
        "alternate"
    } else {
        "marker"
    }
    .into();
    assert!(Engine::replay(archive.clone(), None, None).is_err());
    let disk = Checkpoint::capture(&engine).encode("weighted-population", 0);
    assert!(disk.restore(archive).is_err());
}
