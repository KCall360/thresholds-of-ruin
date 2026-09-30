//! Detaching and reattaching one region, in worlds of 8 and 256 regions with
//! dense terrain. The cost should depend on the region, not the world; see
//! docs/region-streaming.md.
use std::hint::black_box;
use std::time::Instant;
use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

/// 16x16x4 rooms in a chain, every cell with terrain, a door and a link each
/// way to the next room.
fn world(regions: u64) -> World {
    let rooms = (1..=regions)
        .map(|id| Region {
            id: RegionId(id),
            name: String::new(),
            bounds: Extent::new(16, 16, 4).unwrap(),
        })
        .collect();
    let mut world = World::new(rooms, vec![]).unwrap();
    let at = |region, x, y, z| Location {
        region: RegionId(region),
        position: Position { x, y, z },
    };
    for region in 1..=regions {
        for z in 0..4 {
            for y in 0..16 {
                for x in 0..16 {
                    let wall = (x + y + z) % 3 == 0 && !(x == 8 && y == 8);
                    world.set_wall(at(region, x, y, z), wall).unwrap();
                }
            }
        }
        world
            .place_door(at(region, 8, 8, 0), region, false, 1)
            .unwrap();
        if region < regions {
            for (from, direction, to) in [
                (
                    at(region, 15, 1, 0),
                    Direction::East,
                    at(region + 1, 0, 1, 0),
                ),
                (
                    at(region + 1, 0, 1, 0),
                    Direction::West,
                    at(region, 15, 1, 0),
                ),
            ] {
                world
                    .connect(
                        Passage {
                            from,
                            direction,
                            to,
                        },
                        0,
                    )
                    .unwrap();
            }
        }
    }
    world
}

fn main() {
    for regions in [8, 256] {
        let mut world = world(regions);
        let target = RegionId(regions / 2);
        let mut samples = Vec::with_capacity(2_000);
        for round in 0..2_100 {
            let started = Instant::now();
            let slice = world.detach_region(black_box(target)).unwrap();
            world.attach_region(black_box(slice)).unwrap();
            if round >= 100 {
                samples.push(started.elapsed().as_nanos());
            }
        }
        samples.sort_unstable();
        let n = samples.len();
        println!(
            "regions={regions} n={n} p50_us={:.1} p95_us={:.1} max_us={:.1}",
            samples[n / 2] as f64 / 1e3,
            samples[n * 95 / 100] as f64 / 1e3,
            samples[n - 1] as f64 / 1e3,
        );
    }
}
