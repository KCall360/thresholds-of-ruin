//! Scene reuse must never change results: see docs/sight-3d.md.
use std::collections::{BTreeMap, BTreeSet};

use tor_world::{
    Direction, Extent, Location, Passage, Position, Region, RegionId, RegionSlice, World,
};

fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

/// Deterministic xorshift for reproducible random edits.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// The server's latency fixture: 9x9x2 rooms in a chain, each with a low wall
/// and a closed door at (4, 4, 0), a stair, a straight join at z = 0 and a
/// rotated join at z = 1.
fn fixture(regions: u64) -> World {
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
            world.set_wall(at(region, 4, y, 0), true).unwrap();
        }
        world.set_wall(at(region, 2, 2, 0), true).unwrap();
        for (from, direction, to) in [
            (at(region, 2, 4, 0), Direction::Up, at(region, 2, 4, 1)),
            (at(region, 2, 4, 1), Direction::Down, at(region, 2, 4, 0)),
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
        world
            .place_door(at(region, 4, 4, 0), region, false, 1)
            .unwrap();
        if region < regions {
            for (from, direction, to, back, turns) in [
                ((8, 4, 0), Direction::East, (0, 4, 0), Direction::West, 0),
                ((4, 0, 1), Direction::North, (0, 4, 1), Direction::West, 1),
            ] {
                let from = at(region, from.0, from.1, from.2);
                let to = at(region + 1, to.0, to.1, to.2);
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

/// The first dungeon's layout: 7x5x2 chambers in a row, joined by one-wide,
/// two-high doorways. Steps across a chamber's stone shell use rim
/// projection, which reads walls in the neighbouring chamber.
fn dungeon() -> World {
    let mut world = World::new(vec![], vec![]).unwrap();
    for id in 1..=3 {
        world
            .add_chamber(Region {
                id: RegionId(id),
                name: String::new(),
                bounds: Extent::new(7, 5, 2).unwrap(),
            })
            .unwrap();
    }
    for id in 1..=2 {
        for (from, direction, to) in [
            (at(id, 6, 2, 0), Direction::East, at(id + 1, 0, 2, 0)),
            (at(id + 1, 0, 2, 0), Direction::West, at(id, 6, 2, 0)),
        ] {
            world
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
    world
}

/// 3x3x2 rooms in a chain, joined across whole faces. Small views reach the
/// next room only at their edge, where a blocker's bevels read cells in a
/// room no route has stepped into.
fn corridor() -> World {
    let rooms = (1..=6)
        .map(|id| Region {
            id: RegionId(id),
            name: String::new(),
            bounds: Extent::new(3, 3, 2).unwrap(),
        })
        .collect();
    let mut world = World::new(rooms, vec![]).unwrap();
    for id in 1..6 {
        for (from, direction, to) in [
            (at(id, 2, 0, 0), Direction::East, at(id + 1, 0, 0, 0)),
            (at(id + 1, 0, 0, 0), Direction::West, at(id, 2, 0, 0)),
        ] {
            world
                .connect_area(
                    Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                    3,
                    2,
                )
                .unwrap();
        }
    }
    world
}

struct Layout {
    world: World,
    regions: u64,
    /// Cells are drawn from `lo..=hi` in every region.
    lo: [i32; 3],
    hi: [i32; 3],
    /// A door cell in every region, if there is one.
    door: Option<[i32; 3]>,
    radii: &'static [u8],
}

/// Random views of clones of a world under random wall and door edits, with
/// whole regions detached and attached again.
fn random_edits(layout: Layout) {
    let Layout {
        world,
        regions,
        lo,
        hi,
        door,
        radii,
    } = layout;
    for seed in 1..=4u64 {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed);
        let cell = |rng: &mut Rng| {
            let [x, y, z] = [0, 1, 2].map(|a| lo[a] + rng.below((hi[a] - lo[a] + 1) as u64) as i32);
            at(1 + rng.below(regions), x, y, z)
        };
        // A small pool of views, so that views repeat and scenes get reused.
        let views: Vec<_> = (0..12)
            .map(|_| {
                let radius = radii[rng.below(radii.len() as u64) as usize];
                (cell(&mut rng), rng.below(24) as u8, radius)
            })
            .collect();
        // Clones stand in for checkpoints, rewinds and branches: they share
        // the cache but diverge through their own edits.
        let mut worlds = vec![world.clone()];
        // Each clone's detached regions.
        let mut held = vec![BTreeMap::<RegionId, RegionSlice>::new()];
        let (mut reused, mut rebuilt) = (0, 0);
        for step in 0..800 {
            let i = rng.below(worlds.len() as u64) as usize;
            match rng.below(100) {
                0..=74 => {
                    let (eye, frame, radius) = views[rng.below(views.len() as u64) as usize];
                    let world = &worlds[i];
                    if world.eye_scene_cached(eye, frame, radius) {
                        reused += 1;
                    } else {
                        rebuilt += 1;
                    }
                    let uncached = world.eye_scene_uncached(eye, frame, radius);
                    assert_eq!(
                        world.eye_scene(eye, frame, radius),
                        uncached,
                        "seed {seed}, step {step}: eye {eye:?}, frame {frame}, radius {radius}"
                    );
                    let mut visible: Vec<_> = uncached.iter().map(|c| c.location.region).collect();
                    visible.sort();
                    visible.dedup();
                    assert_eq!(
                        world.eye_scene_visible_regions(eye, frame, radius),
                        visible,
                        "seed {seed}, step {step}"
                    );
                    // Reach searches share the cache and its invalidation.
                    let cells = BTreeSet::from([eye]);
                    assert_eq!(
                        world.reach_regions(&cells, 3),
                        world.reach_regions_uncached(&cells, 3),
                        "seed {seed}, step {step}: reach from {eye:?}"
                    );
                }
                75..=81 if door.is_some() => {
                    let [x, y, z] = door.unwrap();
                    let region = 1 + rng.below(regions);
                    let open = rng.below(2) == 0;
                    if !held[i].contains_key(&RegionId(region)) {
                        worlds[i].set_door(at(region, x, y, z), open);
                    }
                }
                75..=89 => {
                    // Door cells can't become walls, and detached regions
                    // can't be edited; those edits are refused.
                    let target = cell(&mut rng);
                    let _ = worlds[i].set_wall(target, rng.below(3) == 0);
                }
                90..=93 if worlds.len() < 6 => {
                    worlds.push(worlds[i].clone());
                    held.push(held[i].clone());
                }
                94..=96 => {
                    let region = RegionId(1 + rng.below(regions));
                    match held[i].remove(&region) {
                        Some(slice) => worlds[i].attach_region(slice).unwrap(),
                        None => {
                            let slice = worlds[i].detach_region(region).unwrap();
                            held[i].insert(region, slice);
                        }
                    }
                }
                _ => {
                    let j = rng.below(worlds.len() as u64) as usize;
                    worlds[i] = worlds[j].clone();
                    held[i] = held[j].clone();
                }
            }
        }
        assert!(
            reused > 100 && rebuilt > 100,
            "seed {seed}: {reused} reused, {rebuilt} rebuilt"
        );
    }
}

#[test]
fn cached_scenes_match_uncached_scenes_in_the_latency_fixture() {
    random_edits(Layout {
        world: fixture(4),
        regions: 4,
        lo: [0, 0, 0],
        hi: [8, 8, 1],
        door: Some([4, 4, 0]),
        radii: &[3, 8],
    });
}

#[test]
fn cached_scenes_match_uncached_scenes_across_chamber_shells() {
    // Storage includes the one-cell stone shell around each chamber.
    random_edits(Layout {
        world: dungeon(),
        regions: 3,
        lo: [-1, -1, -1],
        hi: [7, 5, 2],
        door: None,
        radii: &[3, 8],
    });
}

#[test]
fn cached_scenes_match_uncached_scenes_at_the_edge_of_small_views() {
    random_edits(Layout {
        world: corridor(),
        regions: 6,
        lo: [0, 0, 0],
        hi: [2, 2, 1],
        door: None,
        radii: &[1, 2, 3],
    });
}

#[test]
fn an_edit_invalidates_only_scenes_that_read_its_region() {
    let mut world = fixture(6);
    let view = (at(1, 6, 4, 0), 0, 8);
    let check = |world: &World, cached: bool, context: &str| {
        let (eye, frame, radius) = view;
        assert_eq!(
            world.eye_scene_cached(eye, frame, radius),
            cached,
            "{context}"
        );
        assert_eq!(
            world.eye_scene(eye, frame, radius),
            world.eye_scene_uncached(eye, frame, radius),
            "{context}"
        );
        assert!(world.eye_scene_cached(eye, frame, radius), "{context}");
    };
    check(&world, false, "first view");
    check(&world, true, "unchanged world");

    // The view reaches region 2; regions 5 and 6 are out of range.
    world.set_wall(at(6, 1, 1, 0), true).unwrap();
    world.set_door(at(5, 4, 4, 0), true);
    check(&world, true, "edits in regions the scene never read");
    world.set_place_hint(at(1, 6, 6, 0), true).unwrap();
    world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    check(&world, true, "place hints and gravity don't affect sight");

    world.set_wall(at(2, 1, 1, 0), true).unwrap();
    check(&world, false, "a wall in a region the scene saw");
    world.set_door(at(1, 4, 4, 0), true);
    check(&world, false, "the door in the eye's region");

    let mut branch = world.clone();
    check(&branch, true, "a clone shares the cache");
    branch.set_door(at(1, 4, 4, 0), false);
    check(&branch, false, "the clone's own edit");
    check(
        &world,
        false,
        "the original's scene was replaced by the clone's",
    );
    check(&branch, false, "and the clone's by the original's");

    world
        .add_region(Region {
            id: RegionId(7),
            name: String::new(),
            bounds: Extent::new(3, 3, 1).unwrap(),
        })
        .unwrap();
    check(&world, false, "any topology change");
}

#[test]
fn detaching_invalidates_only_scenes_that_list_the_region() {
    let mut world = fixture(6);
    let view = (at(1, 6, 4, 0), 0, 8);
    let check = |world: &World, cached: bool, context: &str| {
        let (eye, frame, radius) = view;
        assert_eq!(
            world.eye_scene_cached(eye, frame, radius),
            cached,
            "{context}"
        );
        assert_eq!(
            world.eye_scene(eye, frame, radius),
            world.eye_scene_uncached(eye, frame, radius),
            "{context}"
        );
    };
    // The view enters regions 1 and 2, so it lists them and region 3, which
    // region 2 links to. Regions 5 and 6 are out of range.
    check(&world, false, "first view");
    let five = world.detach_region(RegionId(5)).unwrap();
    let six = world.detach_region(RegionId(6)).unwrap();
    check(&world, true, "detaching unlisted regions");
    world.attach_region(six).unwrap();
    check(&world, true, "attaching an unlisted region");

    let three = world.detach_region(RegionId(3)).unwrap();
    check(
        &world,
        false,
        "detaching a region linked from one the view entered",
    );
    world.attach_region(three).unwrap();
    check(&world, false, "attaching it again");

    let two = world.detach_region(RegionId(2)).unwrap();
    check(&world, false, "detaching a region the view entered");
    check(&world, true, "the rebuilt scene without it");
    world.attach_region(two).unwrap();
    check(&world, false, "attaching it again");
    world.attach_region(five).unwrap();
    check(&world, true, "attaching the last unlisted region");
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Checkpoint(#[serde(with = "tor_world::checkpoint_worlds")] Vec<World>);

#[test]
fn checkpoint_worlds_that_differ_only_in_doors_do_not_share_scenes() {
    // A checkpoint stores one geometry and restores each world by cloning it
    // and replacing its doors.
    let closed = fixture(2);
    let mut open = closed.clone();
    open.set_door(at(1, 4, 4, 0), true);
    let view = (at(1, 3, 4, 0), 0, 8);
    let (eye, frame, radius) = view;
    assert_ne!(
        closed.eye_scene_uncached(eye, frame, radius),
        open.eye_scene_uncached(eye, frame, radius),
        "the door must matter to this view"
    );
    let saved = serde_json::to_string(&Checkpoint(vec![closed, open])).unwrap();
    let Checkpoint(loaded) = serde_json::from_str(&saved).unwrap();
    for world in &loaded {
        assert!(!world.eye_scene_cached(eye, frame, radius));
        assert_eq!(
            world.eye_scene(eye, frame, radius),
            world.eye_scene_uncached(eye, frame, radius)
        );
    }
    assert_ne!(
        loaded[0].eye_scene(eye, frame, radius),
        loaded[1].eye_scene(eye, frame, radius)
    );
}
