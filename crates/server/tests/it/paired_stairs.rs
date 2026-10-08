use crate::support;
use std::path::Path;
use tor_protocol::ActorId;
use tor_server::journal::{Action, Direction};
use tor_server::{scenario_package, Engine, SavePolicy, Scenario, Streaming};

fn package() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/tests/paired-stairs")
}

#[test]
fn generated_pair_round_trip_survives_resume() {
    for interval in [0, 1] {
        let directory = tempfile::tempdir().unwrap();
        let save = directory.path().join("stairs.db");
        let mut scenario = scenario_package::load(&package(), 42, None, true).unwrap();
        scenario.streaming = Some(Streaming {
            active_radius: 0,
            load_radius: 0,
        });
        let policy = SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let mut engine = Engine::open_with_policy(&save, scenario, policy.clone()).unwrap();
        let start = engine.state(ActorId(1)).unwrap();
        let origin_key = |state: &tor_protocol::StateView| {
            state
                .observation
                .visible_cells
                .iter()
                .find(|c| c.position == tor_protocol::Position { x: 0, y: 0, z: 0 })
                .unwrap()
                .key
                .clone()
        };
        let expected_landing = start
            .observation
            .visible_cells
            .iter()
            .find(|c| c.stairs_up)
            .unwrap()
            .key
            .clone();
        assert!(start
            .observation
            .visible_cells
            .iter()
            .any(|c| c.stairs_down));
        support::act(
            &mut engine,
            Action::Move {
                direction: Direction::Down,
            },
        )
        .unwrap();
        let arrival = engine.state(ActorId(1)).unwrap();
        assert_eq!(origin_key(&arrival), expected_landing);
        assert_ne!(origin_key(&arrival), origin_key(&start));
        assert!(arrival
            .observation
            .visible_cells
            .iter()
            .any(|c| c.stairs_up));
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
        assert_eq!(
            origin_key(&engine.state(ActorId(1)).unwrap()),
            origin_key(&start)
        );
    }
}
