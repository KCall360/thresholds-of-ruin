//! Run with cargo run -p tor-world --release --example fov_bench.
//! Measures complete scene construction, including topology and sorting.
use std::{hint::black_box, time::Instant};
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};
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
    println!("scenario,radius,ray_us,shadow_us");
    for (name, world, origin) in [
        ("open", open, at(1, 32, 32)),
        ("pillars", pillars, at(1, 32, 32)),
        ("rotated_join", joined, at(1, 32, 32)),
        ("cycle", cycle, at(1, 1, 1)),
    ] {
        for radius in [8, 16] {
            let mut elapsed = Vec::new();
            for shadow in [false, true] {
                let start = Instant::now();
                for _ in 0..10000 {
                    black_box(if shadow {
                        world.shadow_scene(black_box(origin), 0, radius)
                    } else {
                        world.scene(black_box(origin), 0, radius)
                    });
                }
                elapsed.push(start.elapsed().as_secs_f64() * 100.0);
            }
            println!("{name},{radius},{:.3},{:.3}", elapsed[0], elapsed[1]);
        }
    }
}
