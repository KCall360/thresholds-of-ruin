//! End-to-end performance harness, not pass/fail thresholds.
//!
//! Run with `cargo run -p tor-server --release --example latency_bench`.
use std::{hint::black_box, num::NonZeroU64, time::Instant};
use tor_protocol::{Action, ActorId, Direction};
use tor_server::{journal::Command, Engine, Scenario};
use tor_simulation::{Action as SimAction, Game};
use tor_world::{Extent, Location, Passage, Position, Region, RegionId, World};

const SAMPLES: usize = 1_000;

fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}

/// A long chain makes topology lookup realistically non-trivial while sparse
/// pillars exercise occlusion without blocking the sampled portal crossing.
fn complex_game(regions: u64) -> (Game, tor_simulation::ActorId) {
    let rooms = (1..=regions)
        .map(|id| Region {
            id: RegionId(id),
            name: format!("Room {id}"),
            bounds: Extent::new(17, 17, 1).unwrap(),
        })
        .collect();
    let mut world = World::new(rooms, vec![]).unwrap();
    for id in 1..regions {
        world
            .connect(
                Passage {
                    from: at(id, 16, 8),
                    direction: tor_world::Direction::East,
                    to: at(id + 1, 0, 8),
                },
                0,
            )
            .unwrap();
        world
            .connect(
                Passage {
                    from: at(id + 1, 0, 8),
                    direction: tor_world::Direction::West,
                    to: at(id, 16, 8),
                },
                0,
            )
            .unwrap();
    }
    for id in 1..=regions {
        for (x, y) in [(4, 4), (4, 12), (12, 4), (12, 12)] {
            world.set_wall(at(id, x, y), true).unwrap();
        }
    }
    let mut game = Game::new(world, 42);
    let actor = game
        .spawn_actor(at(regions / 2, 16, 8), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.refresh_navigation();
    (game, actor)
}

fn summarize(profile: &str, case: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    let percentile = |p: usize| samples[(samples.len() - 1) * p / 100];
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    println!(
        "{profile},{case},{mean:.3},{:.3},{:.3},{:.3}",
        percentile(50),
        percentile(95),
        samples[samples.len() - 1]
    );
}

fn sample(mut operation: impl FnMut()) -> Vec<f64> {
    (0..SAMPLES)
        .map(|_| {
            let start = Instant::now();
            operation();
            start.elapsed().as_secs_f64() * 1_000.0
        })
        .collect()
}

fn main() {
    println!("profile,case,mean_ms,p50_ms,p95_ms,max_ms");
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    {
        let (name, mut game) = ("diagonal-v11", Game::two_room_in_stone(42));
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 4, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        summarize(
            profile,
            &format!("{name}-scene"),
            sample(|| {
                black_box(game.scene(actor).unwrap());
            }),
        );
        summarize(
            profile,
            &format!("{name}-observe"),
            sample(|| {
                black_box(game.observe(actor).unwrap());
            }),
        );
    }
    let (mut complex, actor) = complex_game(64);
    summarize(
        profile,
        "complex-64-regions-scene",
        sample(|| {
            black_box(complex.scene(actor).unwrap());
        }),
    );
    summarize(
        profile,
        "complex-64-regions-observe",
        sample(|| {
            black_box(complex.observe(actor).unwrap());
        }),
    );
    let mut east = true;
    summarize(
        profile,
        "complex-64-regions-portal-move-and-navigation",
        sample(|| {
            complex
                .act(
                    actor,
                    SimAction::Move(if east {
                        tor_world::Direction::East
                    } else {
                        tor_world::Direction::West
                    }),
                )
                .unwrap();
            complex.refresh_navigation();
            east = !east;
        }),
    );
    let dir = tempfile::tempdir().unwrap();
    for (name, mut engine) in [
        ("memory", Engine::memory(Scenario::two_room(42)).unwrap()),
        (
            "disk",
            Engine::open(dir.path().join("bench.json"), Scenario::two_room(42)).unwrap(),
        ),
    ] {
        for batch in 0..3 {
            let mut samples = Vec::with_capacity(100);
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
                let start = Instant::now();
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
                samples.push(start.elapsed().as_secs_f64() * 1_000.0);
            }
            summarize(
                profile,
                &format!("{name}-actions-{}-{}", batch * 100, (batch + 1) * 100),
                samples,
            );
        }
    }
}
