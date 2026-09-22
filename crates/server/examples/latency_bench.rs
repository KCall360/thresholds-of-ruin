//! Phase A diagnostic harness. Timings are observations, never CI thresholds.
//!
//! Run with `cargo run -p tor-server --release --example latency_bench`.
use std::{hint::black_box, num::NonZeroU64, time::Instant};
use tor_client_ascii::{render::Canvas, App};
use tor_client_common::ClientState;
use tor_protocol::{
    Action, ActorId, HistoryPage, Snapshot, StreamCursor, StreamUpdate, UpdateBody,
};
use tor_server::{journal::Command, ActorSetup, CommandProfile, Engine, Scenario};
use tor_simulation::{Action as SimAction, Game};
use tor_world::{Extent, Location, Passage, Position, Region, RegionId, World};

const SAMPLES: usize = 100;
const REGIONS: [u64; 4] = [1, 8, 64, 256];
const HISTORIES: [usize; 4] = [0, 100, 1_000, 10_000];

fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

fn complex_game(regions: u64, actors: usize) -> Game {
    let rooms = (1..=regions)
        .map(|id| Region {
            id: RegionId(id),
            name: format!("Room {id}"),
            bounds: Extent::new(17, 17, 2).unwrap(),
        })
        .collect();
    let mut world = World::new(rooms, vec![]).unwrap();
    for id in 1..regions {
        for z in 0..2 {
            world
                .connect(
                    Passage {
                        from: at(id, 16, 8, z),
                        direction: tor_world::Direction::East,
                        to: at(id + 1, 0, 8, z),
                    },
                    0,
                )
                .unwrap();
            world
                .connect(
                    Passage {
                        from: at(id + 1, 0, 8, z),
                        direction: tor_world::Direction::West,
                        to: at(id, 16, 8, z),
                    },
                    0,
                )
                .unwrap();
        }
    }
    for id in 1..=regions {
        for z in 0..2 {
            for (x, y) in [(4, 4), (4, 12), (12, 4), (12, 12)] {
                world.set_wall(at(id, x, y, z), true).unwrap();
            }
        }
    }
    let mut game = Game::new(world, 42);
    for index in 0..actors {
        game.spawn_actor(
            at(
                (index as u64 % regions) + 1,
                1 + (index % 12) as i32,
                1,
                (index % 2) as i32,
            ),
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    }
    if regions > 1 {
        let _ = game.place_door(at(1, 16, 8, 0), true);
    }
    game.refresh_navigation();
    game
}

fn scenario(actors: usize) -> Scenario {
    let positions = [
        (1, 1),
        (2, 1),
        (3, 1),
        (1, 2),
        (2, 2),
        (3, 2),
        (4, 1),
        (4, 2),
    ];
    Scenario {
        seed: 42,
        actors: positions[..actors]
            .iter()
            .map(|&(x, y)| ActorSetup {
                position: tor_server::journal::Position {
                    region: 1,
                    x,
                    y,
                    z: 0,
                },
                turn_ticks: 100,
            })
            .collect(),
    }
}

fn summarize(case: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    let percentile = |p: usize| samples[(samples.len() - 1) * p / 100];
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    println!(
        "timing,{case},{mean:.3},{:.3},{:.3},{:.3}",
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

fn add_profile(total: &mut CommandProfile, p: CommandProfile) {
    total.rollback_capture += p.rollback_capture;
    total.simulation_transition += p.simulation_transition;
    total.perception += p.perception;
    total.revision_detection += p.revision_detection;
    total.rollback_snapshot += p.rollback_snapshot;
    total.journal_serialization += p.journal_serialization;
    total.journal_write += p.journal_write;
    total.journal_sync += p.journal_sync;
    total.actors_observed += p.actors_observed;
    total.revision_comparisons += p.revision_comparisons;
    total.rollback_snapshots += p.rollback_snapshots;
    total.records_serialized += p.records_serialized;
    total.bytes_written += p.bytes_written;
}

fn print_profile(case: &str, p: &CommandProfile, divisor: u32) {
    let ms = |d: std::time::Duration| d.as_secs_f64() * 1_000.0 / f64::from(divisor);
    println!(
        "phases,{case},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{:.3},{},{},{},{},{}",
        ms(p.simulation_transition),
        ms(p.perception),
        ms(p.revision_detection),
        ms(p.rollback_capture),
        ms(p.rollback_snapshot),
        ms(p.journal_serialization),
        ms(p.journal_write),
        ms(p.journal_sync),
        p.actors_observed / divisor as usize,
        p.revision_comparisons / divisor as usize,
        p.rollback_snapshots / divisor as usize,
        p.records_serialized / divisor as usize,
        p.bytes_written / u64::from(divisor)
    );
}

fn main() {
    println!("kind,case,mean_ms,p50_ms,p95_ms,max_ms");
    for regions in REGIONS {
        for actors in [1, 8] {
            let game = complex_game(regions, actors);
            summarize(
                &format!("perception-r{regions}-a{actors}"),
                sample(|| {
                    for id in 1..=actors {
                        black_box(game.observe(tor_simulation::ActorId(id as u64)).unwrap());
                        black_box(game.scene(tor_simulation::ActorId(id as u64)).unwrap());
                    }
                }),
            );
            let mut transition = game;
            summarize(
                &format!("simulation-r{regions}-a{actors}"),
                sample(|| {
                    let actor = transition.next_actor().unwrap();
                    transition.act(actor, SimAction::Wait).unwrap();
                    transition.refresh_navigation();
                }),
            );
        }
    }
    println!("kind,case,simulation_ms,perception_ms,revision_ms,candidate_clone_ms,rollback_ms,serialize_ms,write_ms,sync_ms,views,comparisons,snapshots,records,bytes");
    let directory = tempfile::tempdir().unwrap();
    for actors in [1, 8] {
        for target in HISTORIES {
            let mut engine = Engine::memory(scenario(actors)).unwrap();
            engine.seed_profile_history(target).unwrap();
            let mut totals = CommandProfile::default();
            for index in 0..SAMPLES {
                let actor = engine.actors()[(target + index) % actors];
                let command = Command::Act {
                    expected_revision: engine.revision(actor).unwrap(),
                    action: Action::Wait,
                };
                let (_, profile) = engine
                    .command_profiled(
                        "bench",
                        "headless",
                        actor,
                        &format!("sample-{actors}-{target}-{index}"),
                        &engine.branch().clone(),
                        command,
                    )
                    .unwrap();
                add_profile(&mut totals, profile);
            }
            let persistence = engine
                .profile_persistence(directory.path().join(format!("h{target}-a{actors}.json")))
                .unwrap();
            print_profile(
                &format!("history-{target}-actors-{actors}"),
                &totals,
                SAMPLES as u32,
            );
            print_profile(
                &format!("persistence-{target}-actors-{actors}"),
                &persistence,
                1,
            );
        }
    }

    let mut engine = Engine::memory(scenario(1)).unwrap();
    let actor = ActorId(1);
    let initial = Snapshot {
        travel: None,
        actor,
        branch: engine.branch().clone(),
        cursor: StreamCursor {
            sequence: 0,
            tick: 0,
        },
        state: engine.state(actor).unwrap(),
        has_control: true,
        history: HistoryPage {
            entries: vec![],
            older_before: None,
        },
    };
    let mut client = ClientState::from_snapshot(initial).unwrap();
    let result = engine
        .command(
            "bench",
            "ascii",
            actor,
            "client-update",
            &engine.branch().clone(),
            Command::Act {
                expected_revision: 0,
                action: Action::Wait,
            },
        )
        .unwrap();
    let state = engine.state(actor).unwrap();
    let update = StreamUpdate {
        actor,
        branch: engine.branch().clone(),
        cursor: StreamCursor {
            sequence: 1,
            tick: state.observation.tick,
        },
        body: UpdateBody::Observation {
            state: Box::new(state),
            event: Some(Box::new(result.entry.disclosed())),
        },
    };
    summarize(
        "client-state-application",
        sample(|| {
            let mut copy = client.clone();
            copy.apply(black_box(update.clone())).unwrap();
        }),
    );
    client.apply(update).unwrap();
    let mut app = App::new();
    app.set_state(client);
    let mut canvas = Canvas::default();
    summarize(
        "ascii-rendering",
        sample(|| {
            canvas.draw(black_box(&app));
            black_box(&canvas.pixels);
        }),
    );
}
