//! Production Rogue policy in bounded geometry, without generated-floor searches.
use super::*;
use tor_simulation::{Action as SimAction, ActorId as Actor};
use tor_world::Direction;

fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: WorldPosition { x, y, z: 0 },
    }
}

fn fixture() -> Game {
    let mut game = fixture_with(false, 1);
    game.act(Actor(1), SimAction::Wait).unwrap();
    assert_eq!(game.next_actor(), Some(Actor(3)));
    game
}

fn fixture_with(allow_stairs: bool, foes: u64) -> Game {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let production = crate::scenario_package::read_package(&root).unwrap();
    let mut manifest = production.manifest.clone();
    if allow_stairs {
        manifest.capabilities.as_mut().unwrap().npc_stair_traversal = true;
    }
    manifest.characters.retain(|character| character.id == 1);
    manifest.characters[0].anchor = "2/start".into();
    manifest.procedural_worlds.clear();
    manifest.generation_recipes.clear();
    manifest.generation_groups.clear();
    manifest.stair_pairs.clear();
    manifest.floor_activation = None;
    manifest.unique_retrieval = None;
    manifest.objective = None;
    let regions = vec![
        serde_json::from_value(json!({
            "id":1,"name":"Hobgoblin room","size":[17,9,2],
            "chamber":true,"gravity":[0,0,-1],
            "actors":(0..foes).map(|index| json!({"id":3+index,"at":[3+3*index,2,0],
                "archetype":"rogue.hobgoblin","controller":"ai","ai":"rogue_hostile"})).collect::<Vec<_>>()
        }))
        .unwrap(),
        serde_json::from_value(json!({
            "id":2,"name":"Unseen player room","size":[9,5,2],
            "chamber":true,"gravity":[0,0,-1],"anchors":{"start":[2,2,0]}
        }))
        .unwrap(),
    ];
    let rules = production
        .resolved_rules_source
        .as_deref()
        .unwrap_or(&production.rules_source);
    let package = Package::from_parts(manifest, regions, rules).unwrap();
    package.build(42, true).unwrap()
}

fn commit_ai(game: &mut Game) -> SimAction {
    game.admit_ai_intention(Actor(3)).unwrap();
    let result = game.execute_next_intention().unwrap();
    result.outcome.unwrap();
    result.action.unwrap()
}

fn ready_ai(game: &mut Game) {
    for _ in 0..4 {
        if game.next_actor() == Some(Actor(3)) {
            return;
        }
        game.act(Actor(1), SimAction::Wait).unwrap();
    }
    panic!("bounded fixture did not reach the hobgoblin turn");
}

#[test]
fn hobgoblin_roams_before_perception_without_tracking_an_unseen_player() {
    let mut game = fixture();
    let before = game.observe(Actor(3)).unwrap();
    assert!(before
        .visible_actors
        .iter()
        .all(|actor| actor.id != Actor(1)));
    let mut other = game.clone();
    other.teleport(Actor(1), at(2, 6, 2)).unwrap();
    assert_eq!(before, other.observe(Actor(3)).unwrap());
    let action = game.next_ai_action().unwrap();
    assert!(
        matches!(action, (Actor(3), SimAction::Move(_))),
        "unengaged production hobgoblin must roam"
    );
    assert_eq!(other.next_ai_action(), Some(action.clone()));
    assert_eq!(commit_ai(&mut game), action.1);
    assert_eq!(commit_ai(&mut other), action.1);
    assert_ne!(game.observe(Actor(3)).unwrap().location, before.location);
    assert_eq!(
        game.observe(Actor(3)).unwrap(),
        other.observe(Actor(3)).unwrap()
    );
}

#[test]
fn hobgoblin_idle_opens_a_reachable_door() {
    let mut game = fixture();
    let door = game.place_door(at(1, 4, 2), false, 2).unwrap();
    assert_eq!(
        game.next_ai_action(),
        Some((Actor(3), SimAction::SetDoor { door, open: true }))
    );
    assert_eq!(
        commit_ai(&mut game),
        SimAction::SetDoor { door, open: true }
    );
    assert!(game
        .observe(Actor(3))
        .unwrap()
        .visible_cells
        .iter()
        .any(|cell| cell
            .door
            .is_some_and(|state| state.id == door && state.open)));
}

fn wall_column(game: &mut Game, x: i32, y: i32) {
    for z in 0..2 {
        let mut cell = at(1, x, y);
        cell.position.z = z;
        game.set_wall(cell, true).unwrap();
    }
}

#[test]
fn hobgoblin_stair_policy_blocks_the_only_exit_and_allowed_counterfactual_takes_it() {
    for allow_stairs in [false, true] {
        let mut game = fixture_with(allow_stairs, 1);
        for x in 2..=4 {
            for y in 1..=3 {
                if (x, y) != (3, 2) {
                    wall_column(&mut game, x, y);
                }
            }
        }
        game.connect(
            tor_world::Passage {
                from: at(1, 3, 2),
                direction: Direction::Down,
                to: at(2, 6, 2),
            },
            0,
        )
        .unwrap();
        game.refresh_navigation();
        game.act(Actor(1), SimAction::Wait).unwrap();
        assert!(game
            .observe(Actor(3))
            .unwrap()
            .visible_cells
            .iter()
            .any(|cell| cell.location == at(2, 6, 2)));
        let expected = if allow_stairs {
            SimAction::Move(Direction::Down)
        } else {
            SimAction::Wait
        };
        assert_eq!(game.next_ai_action(), Some((Actor(3), expected.clone())));
        assert_eq!(commit_ai(&mut game), expected);
        assert_eq!(
            game.observe(Actor(3)).unwrap().location,
            if allow_stairs {
                at(2, 6, 2)
            } else {
                at(1, 3, 2)
            }
        );
    }
}

#[test]
fn hobgoblin_prefers_the_unvisited_end_after_returning_to_corridor_center() {
    let mut game = fixture();
    for x in 1..=5 {
        for y in 1..=3 {
            if y != 2 || !(2..=4).contains(&x) {
                wall_column(&mut game, x, y);
            }
        }
    }
    for (direction, x) in [
        (Direction::West, 2),
        (Direction::East, 3),
        (Direction::East, 4),
    ] {
        ready_ai(&mut game);
        assert_eq!(commit_ai(&mut game), SimAction::Move(direction));
        assert_eq!(game.observe(Actor(3)).unwrap().location, at(1, x, 2));
    }
}

#[test]
fn roaming_hobgoblin_acquires_a_personally_perceived_hostile() {
    let mut game = fixture();
    assert!(matches!(commit_ai(&mut game), SimAction::Move(_)));
    // A bounded seeded acquisition witness, not a route through generated floors.
    let mut attacked = false;
    for _ in 0..8 {
        let here = game.observe(Actor(3)).unwrap().location;
        game.teleport(Actor(1), at(1, here.position.x + 1, here.position.y))
            .unwrap();
        ready_ai(&mut game);
        assert!(game
            .observe(Actor(3))
            .unwrap()
            .visible_actors
            .iter()
            .any(|actor| actor.id == Actor(1)));
        if matches!(
            commit_ai(&mut game),
            SimAction::UseAbility {
                invocation: tor_simulation::abilities::AbilityInvocation {
                    aim: tor_simulation::abilities::AbilityAim::ActorTarget { target: Actor(1) },
                    weapon: tor_simulation::abilities::WeaponSelection::Default
                },
                ..
            }
        ) {
            attacked = true;
            break;
        }
    }
    assert!(
        attacked,
        "seeded personal perception must replace idle roaming with hostility"
    );
}

#[test]
fn roaming_decision_work_records_growing_personal_history_without_extra_route_searches() {
    let mut game = fixture();
    for decisions in 0..=16 {
        ready_ai(&mut game);
        if decisions % 8 == 0 {
            let mut shared = tor_simulation::checkpoint::SharedState::default();
            let saved = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
            let history = saved["combat"]["ai"]["3"]["visits"]
                .as_array()
                .unwrap()
                .len();
            assert!(history <= decisions);
            if decisions > 0 {
                assert!(history > 0, "roaming records its own visited locations");
            }
            let before = tor_simulation::diagnostics::work_counts();
            assert!(matches!(
                game.next_ai_action(),
                Some((Actor(3), SimAction::Move(_)))
            ));
            let after = tor_simulation::diagnostics::work_counts();
            assert_eq!(after.route_searches - before.route_searches, 0);
            assert_eq!(after.scenes - before.scenes, 0);
            // This fixture stays in one region: the chosen visit update copies
            // that region's bucket, not every loaded actor's history. Structural
            // cross-region sharing is covered by simulation ai::idle_tests.
            let copied = after.ai_visit_entries_copied - before.ai_visit_entries_copied;
            assert_eq!(copied, history);
            println!("roaming decisions={decisions} copied_current_region_entries={copied} route_searches=0 scenes=0");
        }
        if decisions < 16 {
            assert!(matches!(commit_ai(&mut game), SimAction::Move(_)));
        }
    }
}

#[test]
fn player_actions_share_loaded_ai_histories_after_bounded_mature_roaming() {
    let mut game = fixture_with(false, 4);
    for rounds in 0..=64 {
        assert_eq!(game.next_actor(), Some(Actor(1)));
        if [0, 16, 64].contains(&rounds) {
            let mut shared = tor_simulation::checkpoint::SharedState::default();
            let saved = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
            let histories: Vec<_> = saved["combat"]["ai"]
                .as_object()
                .unwrap()
                .values()
                .map(|ai| ai["visits"].as_array().unwrap().len())
                .collect();
            assert_eq!(histories.len(), 4);
            assert_eq!(game.known_actor_ids().len(), 5);
            let entries: usize = histories.iter().sum();
            if rounds == 0 {
                assert_eq!(entries, 0);
            } else {
                assert!(entries >= 32);
            }
            let mut sample = game.clone();
            let before = tor_simulation::diagnostics::work_counts();
            sample.act(Actor(1), SimAction::Wait).unwrap();
            let after = tor_simulation::diagnostics::work_counts();
            let clones = after.game_clones - before.game_clones;
            assert!(clones > 0);
            assert_eq!(after.route_searches - before.route_searches, 0);
            assert_eq!(
                after.ai_visit_entries_copied - before.ai_visit_entries_copied,
                0
            );
            // The actor/region directory is cloned, but immutable visit buckets
            // remain shared. No visit entry is mutated by the player's Wait.
            println!("player_wait rounds={rounds} loaded_actors=5 ai_histories={histories:?} game_clones={clones} copied_visit_entries=0");
        }
        if rounds < 64 {
            game.act(Actor(1), SimAction::Wait).unwrap();
            for _ in 0..32 {
                let next = game.next_actor().unwrap();
                if next == Actor(1) {
                    break;
                }
                assert!((3..=6).contains(&next.0));
                game.admit_ai_intention(next).unwrap();
                game.execute_next_intention().unwrap().outcome.unwrap();
            }
            assert_eq!(
                game.next_actor(),
                Some(Actor(1)),
                "bounded four-foe round must return to the player"
            );
        }
    }
}
