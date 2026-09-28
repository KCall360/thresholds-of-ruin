//! Run with cargo run -p tor-world --release --example fov_bench.
//! Measures complete scene construction, including topology and sorting, for the
//! 2D shadowcasting, voxel height-slice, and reference 3D sight builders.
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
            report(name, radius, "eye", || world.eye_scene(*origin, 0, radius));
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
        report(name, 8, "eye", || world.eye_scene(eye, 0, 8));
        report(name, 8, "eye_reference", || {
            world.eye_scene_reference(eye, 0, 8)
        });
    }
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
