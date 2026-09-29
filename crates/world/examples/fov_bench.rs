//! Run with cargo run -p tor-world --release --example fov_bench.
//! Measures complete scene construction, including topology and sorting, for the
//! 2D shadowcasting, voxel height-slice, and reference and accelerated 3D sight
//! builders. `eye_reused` times a scene served from the cache.
use std::{hint::black_box, time::Instant};
use tor_world::{
    Direction, Extent, Location, Passage, Position, Region, RegionId, SightCell, World,
};
fn at(region: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z: 0 },
    }
}
fn room(id: u64, w: i32, h: i32) -> Region {
    Region {
        id: RegionId(id),
        name: String::new(),
        bounds: Extent::new(w, h, 1).unwrap(),
    }
}
fn main() {
    let open = World::new(vec![room(1, 65, 65)], vec![]).unwrap();
    let mut pillars = open.clone();
    for x in (20..45).step_by(4) {
        for y in (20..45).step_by(4) {
            if (x, y) != (32, 32) {
                pillars.set_wall(at(1, x, y), true).unwrap();
            }
        }
    }
    let mut joined = World::new(vec![room(1, 33, 65), room(2, 65, 33)], vec![]).unwrap();
    joined
        .connect_area(
            Passage {
                from: at(1, 32, 0),
                direction: Direction::East,
                to: at(2, 64, 0),
            },
            1,
            65,
            1,
        )
        .unwrap();
    let mut cycle = World::new(vec![room(1, 3, 3)], vec![]).unwrap();
    cycle
        .connect_area(
            Passage {
                from: at(1, 2, 0),
                direction: Direction::East,
                to: at(1, 0, 0),
            },
            0,
            3,
            1,
        )
        .unwrap();
    // Dungeon-style chambers: a humanoid eye one cell above the feet, and a
    // giant's eye in a three-cell-tall hall with scattered pillars.
    let chamber = |w, d, h| {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_chamber(Region {
                id: RegionId(1),
                name: String::new(),
                bounds: Extent::new(w, d, h).unwrap(),
            })
            .unwrap();
        world
    };
    let room = chamber(17, 17, 2);
    let mut hall = chamber(17, 17, 3);
    for x in (2..16).step_by(4) {
        for y in (2..16).step_by(4) {
            for z in 0..3 {
                hall.set_wall(lift(at(1, x, y), z), true).unwrap();
            }
        }
    }
    // The first dungeon's layout: 7x5x2 rooms in a row, joined by one-wide,
    // two-high doorways in the middle of their east and west walls.
    let mut dungeon = World::new(vec![], vec![]).unwrap();
    for id in 1..=3 {
        dungeon
            .add_chamber(Region {
                id: RegionId(id),
                name: String::new(),
                bounds: Extent::new(7, 5, 2).unwrap(),
            })
            .unwrap();
    }
    for id in 1..=2 {
        for (from, direction, to) in [
            (at(id, 6, 2), Direction::East, at(id + 1, 0, 2)),
            (at(id + 1, 0, 2), Direction::West, at(id, 6, 2)),
        ] {
            dungeon
                .connect_area(
                    Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                    1,
                    2,
                )
                .unwrap();
        }
    }
    let fixture = latency_fixture(5);
    println!("scenario,radius,builder,mean_us");
    let planar = [
        ("open", open, at(1, 32, 32)),
        ("pillars", pillars, at(1, 32, 32)),
        ("rotated_join", joined, at(1, 32, 32)),
        ("cycle", cycle, at(1, 1, 1)),
    ];
    for (name, world, origin) in &planar {
        for radius in [8, 16] {
            report(name, radius, "shadow", || {
                world.shadow_scene(*origin, 0, radius)
            });
            report(name, radius, "eye", || {
                world.eye_scene_uncached(*origin, 0, radius)
            });
        }
    }
    for (name, world, feet, eye) in [
        ("room", &room, at(1, 8, 8), lift(at(1, 8, 8), 1)),
        ("hall", &hall, at(1, 8, 8), lift(at(1, 8, 8), 2)),
        (
            "dungeon_centre",
            &dungeon,
            at(2, 3, 2),
            lift(at(2, 3, 2), 1),
        ),
        (
            "dungeon_doorway",
            &dungeon,
            at(2, 6, 2),
            lift(at(2, 6, 2), 1),
        ),
    ] {
        report(name, 8, "shadow", || world.shadow_scene(feet, 0, 8));
        report(name, 8, "volume", || world.volume_scene(feet, 0, 8));
        report(name, 8, "eye", || world.eye_scene_uncached(eye, 0, 8));
        report(name, 8, "eye_reference", || {
            world.eye_scene_reference(eye, 0, 8)
        });
    }
    // The latency fixture's plain rooms, seen from a middle region: a low wall
    // with a door, a stair, a straight join and a rotated join. Single-cell
    // observers stand at the fixture's actor cells.
    for (name, x, y, z) in [
        ("fixture_stair", 2, 4, 0),
        ("fixture_east", 6, 1, 0),
        ("fixture_corner", 0, 0, 0),
        ("fixture_upper", 4, 4, 1),
    ] {
        let eye = lift(at(3, x, y), z);
        report(name, 8, "shadow", || fixture.shadow_scene(eye, 0, 8));
        report(name, 8, "eye", || fixture.eye_scene_uncached(eye, 0, 8));
        report(name, 8, "eye_reused", || fixture.eye_scene(eye, 0, 8));
    }
}

/// `crates/server/fixtures/performance-v1.json`, as built by the server's
/// performance fixture: 9x9x2 rooms in a chain.
fn latency_fixture(regions: u64) -> World {
    let cell = |region, [x, y, z]: [i32; 3]| lift(at(region, x, y), z);
    let rooms = (1..=regions)
        .map(|id| Region {
            id: RegionId(id),
            name: String::new(),
            bounds: Extent::new(9, 9, 2).unwrap(),
        })
        .collect();
    let mut world = World::new(rooms, vec![]).unwrap();
    for region in 1..=regions {
        for y in [0, 1, 2, 3, 5, 6, 7, 8] {
            world.set_wall(cell(region, [4, y, 0]), true).unwrap();
        }
        world.set_wall(cell(region, [2, 2, 0]), true).unwrap();
        for (from, direction, to) in [
            ([2, 4, 0], Direction::Up, [2, 4, 1]),
            ([2, 4, 1], Direction::Down, [2, 4, 0]),
        ] {
            world
                .connect(
                    Passage {
                        from: cell(region, from),
                        direction,
                        to: cell(region, to),
                    },
                    0,
                )
                .unwrap();
        }
        world
            .place_door(cell(region, [4, 4, 0]), region, false, 1)
            .unwrap();
        if region < regions {
            for (from, direction, to, back, turns) in [
                ([8, 4, 0], Direction::East, [0, 4, 0], Direction::West, 0),
                ([4, 0, 1], Direction::North, [0, 4, 1], Direction::West, 1),
            ] {
                let (from, to) = (cell(region, from), cell(region + 1, to));
                world
                    .connect(
                        Passage {
                            from,
                            direction,
                            to,
                        },
                        turns,
                    )
                    .unwrap();
                world
                    .connect(
                        Passage {
                            from: to,
                            direction: back,
                            to: from,
                        },
                        (4 - turns) % 4,
                    )
                    .unwrap();
            }
        }
    }
    world
}

fn lift(mut location: Location, z: i32) -> Location {
    location.position.z = z;
    location
}

/// Repeat for at least half a second and print the mean scene time.
fn report(name: &str, radius: u8, builder: &str, mut scene: impl FnMut() -> Vec<SightCell>) {
    let start = Instant::now();
    let mut runs = 0u32;
    while runs < 10 || start.elapsed().as_secs_f64() < 0.5 {
        black_box(scene());
        runs += 1;
    }
    let mean = start.elapsed().as_secs_f64() * 1e6 / f64::from(runs);
    println!("{name},{radius},{builder},{mean:.1}");
}
