//! Diagnostic timings, not pass/fail thresholds. Run in debug and release.
use std::{hint::black_box, num::NonZeroU64, time::Instant};
use tor_protocol::{Action, ActorId, Direction};
use tor_server::{journal::Command, Engine, Scenario};
use tor_simulation::Game;
use tor_world::{Location, Position, RegionId};

fn main() {
    println!("profile,case,average_ms");
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    for (name, mut game) in [("diagonal-v11", Game::two_room_in_stone(42))] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 4, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let start = Instant::now();
        for _ in 0..1000 {
            black_box(game.scene(actor).unwrap());
        }
        println!(
            "{profile},{name}-scene,{:.3}",
            start.elapsed().as_secs_f64()
        );
        let start = Instant::now();
        for _ in 0..1000 {
            black_box(game.observe(actor).unwrap());
        }
        println!(
            "{profile},{name}-observe,{:.3}",
            start.elapsed().as_secs_f64()
        );
    }
    let dir = tempfile::tempdir().unwrap();
    for (name, mut engine) in [
        ("memory", Engine::memory(Scenario::two_room(42)).unwrap()),
        (
            "disk",
            Engine::open(dir.path().join("bench.json"), Scenario::two_room(42)).unwrap(),
        ),
    ] {
        for batch in 0..3 {
            let start = Instant::now();
            for i in 0..100 {
                let action = Action::Move {
                    direction: if i % 2 == 0 {
                        Direction::East
                    } else {
                        Direction::West
                    },
                };
                let command = Command::Act {
                    expected_revision: engine.revision(ActorId(1)).unwrap(),
                    action,
                };
                engine
                    .command(
                        "bench",
                        "headless",
                        ActorId(1),
                        &format!("{batch}-{i}"),
                        &engine.branch().clone(),
                        command,
                    )
                    .unwrap();
                black_box(engine.observation(ActorId(1)).unwrap());
            }
            println!(
                "{profile},{name}-actions-{}-{},{:.3}",
                batch * 100,
                (batch + 1) * 100,
                start.elapsed().as_secs_f64() * 10.0
            );
        }
    }
}
