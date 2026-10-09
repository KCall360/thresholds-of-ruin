use crate::support;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use tor_protocol::ActorId;
use tor_server::journal::{Action, Direction};
use tor_server::{generation_recipe, scenario_package, Engine, SavePolicy, Scenario, Streaming};

fn definitions(
    package: &scenario_package::Package,
    depth: u32,
) -> BTreeMap<u64, scenario_package::RegionDef> {
    let name = format!("floor-{depth}");
    let group = &package.manifest.generation_groups[&name];
    generation_recipe::materialize(
        &name,
        group,
        &package.manifest.generation_recipes[&group.recipe],
        group
            .members
            .iter()
            .map(|id| (*package.region_def(*id).unwrap()).clone())
            .collect(),
        42,
    )
    .unwrap()
}

fn path(
    defs: &BTreeMap<u64, scenario_package::RegionDef>,
    first: u64,
    start: (i32, i32),
    target: (i32, i32),
) -> Vec<Direction> {
    let mut open = BTreeSet::new();
    for (id, def) in defs {
        let slot = (id - first) as i32;
        let walls: BTreeSet<_> = def.walls.iter().copied().collect();
        for y in 0..7 {
            for x in 0..26 {
                if !walls.contains(&[x, y, 0]) {
                    open.insert((x + slot % 3 * 26, y + slot / 3 * 7));
                }
            }
        }
    }
    let mut back = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let mut seen = BTreeSet::from([start]);
    while let Some(at) = queue.pop_front() {
        if at == target {
            break;
        }
        for (next, direction) in [
            ((at.0 - 1, at.1), Direction::West),
            ((at.0 + 1, at.1), Direction::East),
            ((at.0, at.1 - 1), Direction::North),
            ((at.0, at.1 + 1), Direction::South),
        ] {
            if open.contains(&next) && seen.insert(next) {
                back.insert(next, (at, direction));
                queue.push_back(next);
            }
        }
    }
    let mut at = target;
    let mut moves = Vec::new();
    while at != start {
        let (previous, direction) = back[&at];
        moves.push(direction);
        at = previous;
    }
    moves.reverse();
    moves
}

#[test]
fn corruption_of_a_detached_members_pinned_source_rejects_group_recovery() {
    let mut scenario = support::load("rogue-exploration", 42);
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let directory = tempfile::tempdir().unwrap();
    let save = directory.path().join("corrupt.db");
    let policy = SavePolicy {
        checkpoint_interval: 0,
        ..SavePolicy::default()
    };
    let engine = Engine::open_with_policy(&save, scenario, policy.clone()).unwrap();
    assert!(engine.region_counts().unwrap().detached > 0);
    engine.flush().unwrap();
    drop(engine);
    let connection = rusqlite::Connection::open(&save).unwrap();
    assert_eq!(
        connection
            .execute(
                "UPDATE region_sources SET source = source || '# damaged' WHERE region = 9",
                [],
            )
            .unwrap(),
        1
    );
    drop(connection);
    let before = std::fs::read(&save).unwrap();
    let error = Engine::open_with_policy(&save, Scenario::two_room(0), policy).unwrap_err();
    assert!(error.message.contains("changed since"), "{error}");
    assert_eq!(std::fs::read(&save).unwrap(), before);
}

#[test]
fn all_twenty_six_floors_are_reversible_and_generation_rewinds() {
    let mut scenario = support::load("rogue-exploration", 42);
    let package = scenario.package.clone().unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let mut engine = Engine::memory(scenario.clone()).unwrap();
    engine.enable_wizard().unwrap();
    let initial_counts = engine.region_counts().unwrap();
    let mut legs = Vec::new();
    for depth in 1..=25 {
        let defs = definitions(&package, depth);
        let first = (depth as u64 - 1) * 9 + 1;
        let start = if depth == 1 {
            [13, 3, 0]
        } else {
            defs[&first].anchors["up"]
        };
        let down = defs[&(first + 8)].anchors["down"];
        let moves = path(
            &defs,
            first,
            (start[0], start[1]),
            (down[0] + 52, down[1] + 14),
        );
        for direction in &moves {
            support::act(
                &mut engine,
                Action::Move {
                    direction: *direction,
                },
            )
            .unwrap();
        }
        support::act(
            &mut engine,
            Action::Move {
                direction: Direction::Down,
            },
        )
        .unwrap();
        let counts = engine.region_counts().unwrap();
        assert!(
            counts.active + counts.frozen < 18 && counts.detached > 0,
            "floor {}: {counts:?}",
            depth + 1
        );
        legs.push(moves);
    }
    assert!(!definitions(&package, 26)[&234].anchors.contains_key("down"));
    for moves in legs.iter().rev() {
        support::act(
            &mut engine,
            Action::Move {
                direction: Direction::Up,
            },
        )
        .unwrap();
        for direction in moves.iter().rev() {
            let direction = match direction {
                Direction::North => Direction::South,
                Direction::South => Direction::North,
                Direction::East => Direction::West,
                Direction::West => Direction::East,
                _ => unreachable!(),
            };
            support::act(&mut engine, Action::Move { direction }).unwrap();
        }
    }
    // Rewind within the ordinary 128-boundary window; a complete 26-floor
    // expedition deliberately exceeds that window.
    let mut engine = Engine::memory(scenario).unwrap();
    engine.enable_wizard().unwrap();
    let initial = engine.state(ActorId(1)).unwrap();
    let defs = definitions(&package, 1);
    let down = defs[&9].anchors["down"];
    let moves = path(&defs, 1, (13, 3), (down[0] + 52, down[1] + 14));
    for direction in &moves {
        support::act(
            &mut engine,
            Action::Move {
                direction: *direction,
            },
        )
        .unwrap();
    }
    support::act(
        &mut engine,
        Action::Move {
            direction: Direction::Down,
        },
    )
    .unwrap();
    let first_arrival = engine.state(ActorId(1)).unwrap();
    let expected_revision = engine.revision(ActorId(1)).unwrap();
    support::submit(
        &mut engine,
        ActorId(1),
        tor_server::journal::Command::Wizard {
            expected_revision,
            operation: tor_server::journal::WizardOperation::Rewind { target: None },
        },
    )
    .unwrap();
    let restored = engine.region_counts().unwrap();
    assert_eq!(restored.active, initial_counts.active);
    assert_eq!(restored.frozen, initial_counts.frozen);
    assert_eq!(restored.detached, initial_counts.detached);
    assert_eq!(restored.unbuilt, initial_counts.unbuilt);
    assert_eq!(
        engine.state(ActorId(1)).unwrap().observation,
        initial.observation
    );
    // The selected boundary restores generation status, not just actor position.
    for direction in moves {
        support::act(&mut engine, Action::Move { direction }).unwrap();
    }
    support::act(
        &mut engine,
        Action::Move {
            direction: Direction::Down,
        },
    )
    .unwrap();
    assert_eq!(
        engine.state(ActorId(1)).unwrap().observation,
        first_arrival.observation
    );
}

#[test]
fn exploration_floor_streams_independently_and_round_trip_survives_resume() {
    let scenario =
        scenario_package::load(&support::package("rogue-exploration"), 42, None, true).unwrap();
    let package = scenario.package.clone().unwrap();
    let mut scenario = scenario;
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let directory = tempfile::tempdir().unwrap();
    let save = directory.path().join("exploration.db");
    for interval in [0, 1] {
        let save = save.with_extension(format!("{interval}.db"));
        let policy = SavePolicy {
            checkpoint_interval: interval,
            ..SavePolicy::default()
        };
        let mut engine = Engine::open_with_policy(&save, scenario.clone(), policy.clone()).unwrap();
        let counts = engine.region_counts().unwrap();
        assert!(counts.active + counts.frozen < 9);
        assert!(counts.detached > 0);
        let start = engine.state(ActorId(1)).unwrap();
        let here = |state: &tor_protocol::StateView| {
            state
                .observation
                .visible_cells
                .iter()
                .find(|cell| cell.position == tor_protocol::Position { x: 0, y: 0, z: 0 })
                .unwrap()
                .key
                .clone()
        };
        let defs = definitions(&package, 1);
        // Excavate an interior room edge, then leave its member while other
        // floor members remain loaded. The ordinary record must own the edit.
        let walls: BTreeSet<_> = defs[&1].walls.iter().copied().collect();
        let edit = [(0, -1), (1, 0), (0, 1), (-1, 0)]
            .into_iter()
            .find_map(|(dx, dy)| {
                (1..26)
                    .map(|n| [13 + dx * n, 3 + dy * n, 0])
                    .find(|at| walls.contains(at))
                    .filter(|at| (1..25).contains(&at[0]) && (1..6).contains(&at[1]))
            })
            .unwrap();
        engine.enable_wizard().unwrap();
        for z in 0..2 {
            let expected_revision = engine.revision(ActorId(1)).unwrap();
            support::submit(
                &mut engine,
                ActorId(1),
                tor_server::journal::Command::Wizard {
                    expected_revision,
                    operation: tor_server::journal::WizardOperation::SetWall {
                        position: tor_server::journal::Position {
                            region: 1,
                            x: edit[0],
                            y: edit[1],
                            z,
                        },
                        wall: false,
                    },
                },
            )
            .unwrap();
        }
        let down = defs[&9].anchors["down"];
        let moves = path(&defs, 1, (13, 3), (down[0] + 52, down[1] + 14));
        for direction in &moves {
            support::act(
                &mut engine,
                Action::Move {
                    direction: *direction,
                },
            )
            .unwrap();
        }
        assert!(engine
            .state(ActorId(1))
            .unwrap()
            .observation
            .visible_cells
            .iter()
            .any(|c| c.position == tor_protocol::Position { x: 0, y: 0, z: 0 } && c.stairs_down));
        support::act(
            &mut engine,
            Action::Move {
                direction: Direction::Down,
            },
        )
        .unwrap();
        let arrival = engine.state(ActorId(1)).unwrap();
        engine.flush().unwrap();
        drop(engine);
        let mut engine = Engine::open_with_policy(&save, Scenario::two_room(0), policy).unwrap();
        assert_eq!(engine.state(ActorId(1)).unwrap(), arrival);
        support::act(
            &mut engine,
            Action::Move {
                direction: Direction::Up,
            },
        )
        .unwrap();
        for direction in moves.iter().rev() {
            let direction = match direction {
                Direction::North => Direction::South,
                Direction::South => Direction::North,
                Direction::East => Direction::West,
                Direction::West => Direction::East,
                _ => unreachable!(),
            };
            support::act(&mut engine, Action::Move { direction }).unwrap();
        }
        let returned = engine.state(ActorId(1)).unwrap();
        assert_eq!(here(&returned), here(&start));
        let edited = returned
            .observation
            .visible_cells
            .iter()
            .find(|cell| {
                cell.position
                    == tor_protocol::Position {
                        x: edit[0] - 13,
                        y: edit[1] - 3,
                        z: 0,
                    }
            })
            .expect("excavated edge visible after reload");
        assert!(!edited.wall);
    }
}

#[test]
fn authored_entry_and_group_anchor_form_a_reversible_named_stair() {
    let original = support::load("rogue-exploration", 42).package.unwrap();
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("mixed");
    std::fs::create_dir_all(root.join("regions")).unwrap();
    let mut manifest = original.manifest.clone();
    manifest
        .generation_groups
        .retain(|name, _| name == "floor-1");
    for id in 1..=9 {
        std::fs::write(
            root.join(format!("regions/{id}.toml")),
            toml::to_string(&*original.region_def(id).unwrap()).unwrap(),
        )
        .unwrap();
    }
    manifest.characters[0].anchor = "235/entry".into();
    manifest.stair_pairs = BTreeMap::from([(
        "entrance".into(),
        scenario_package::StairPair {
            upper: "235/entry".into(),
            lower: "1/start".into(),
        },
    )]);
    std::fs::write(
        root.join("scenario.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    std::fs::write(
        root.join("regions/235.toml"),
        "id=235\nname='Entry'\nsize=[3,3,2]\nchamber=true\n[anchors]\nentry=[1,1,0]\n",
    )
    .unwrap();
    let mut scenario = scenario_package::load(&root, 42, None, true).unwrap();
    scenario.streaming = Some(Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let mut engine = Engine::open(directory.path().join("mixed.db"), scenario).unwrap();
    let here = |state: &tor_protocol::StateView| {
        state
            .observation
            .visible_cells
            .iter()
            .find(|cell| cell.position == tor_protocol::Position { x: 0, y: 0, z: 0 })
            .unwrap()
            .key
            .clone()
    };
    let entry = here(&engine.state(ActorId(1)).unwrap());
    support::act(
        &mut engine,
        Action::Move {
            direction: Direction::Down,
        },
    )
    .unwrap();
    assert!(engine
        .state(ActorId(1))
        .unwrap()
        .observation
        .visible_cells
        .iter()
        .any(|cell| cell.stairs_up));
    support::act(
        &mut engine,
        Action::Move {
            direction: Direction::Up,
        },
    )
    .unwrap();
    assert_eq!(here(&engine.state(ActorId(1)).unwrap()), entry);
    engine.flush().unwrap();
}
