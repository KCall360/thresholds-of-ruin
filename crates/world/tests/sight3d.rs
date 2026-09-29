//! Reference 3D sight: see docs/sight-3d.md.
use std::collections::BTreeSet;

use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}

fn chamber(width: i32, depth: i32, height: i32) -> World {
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "test".into(),
            bounds: Extent::new(width, depth, height).unwrap(),
        })
        .unwrap();
    world
}

/// Visible absolute positions in region 1 from an eye cell.
fn seen(world: &World, eye: Location) -> BTreeSet<(i32, i32, i32)> {
    world
        .eye_scene(eye, 0, 8)
        .iter()
        .map(|c| {
            let p = c.location.position;
            (p.x, p.y, p.z)
        })
        .collect()
}

fn manhattan(a: (i32, i32, i32), b: (i32, i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs()
}

/// Deterministic xorshift for reproducible random layouts.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

#[test]
fn open_room_discloses_every_floor_ceiling_and_wall_face_for_every_eye_height() {
    let (w, d, h) = (7, 7, 3);
    let world = chamber(w, d, h);
    // One-cell rat, two-cell humanoid, and three-cell giant eyes.
    for eye_z in 0..h {
        let eye = (3, 3, eye_z);
        let visible = seen(&world, at(1, eye.0, eye.1, eye.2));
        let mut expected = BTreeSet::new();
        for z in -1..=h {
            for y in -1..=d {
                for x in -1..=w {
                    let interior =
                        (0..w).contains(&x) && (0..d).contains(&y) && (0..h).contains(&z);
                    let outside = [x < 0 || x >= w, y < 0 || y >= d, z < 0 || z >= h];
                    // A shell cell has an exposed face only when it is outside
                    // the interior along exactly one axis.
                    let exposed = outside.iter().filter(|o| **o).count() == 1;
                    if (interior || exposed) && manhattan(eye, (x, y, z)) <= 8 {
                        expected.insert((x, y, z));
                    }
                }
            }
        }
        assert_eq!(visible, expected, "eye height {eye_z}");
        for cell in world.eye_scene(at(1, eye.0, eye.1, eye.2), 0, 8) {
            assert!(
                world.terrain(cell.location).is_some(),
                "never discloses missing geometry"
            );
        }
    }
}

#[test]
fn waist_wall_hides_nearby_floor_from_a_humanoid_and_all_of_it_from_a_rat() {
    let world = {
        let mut world = chamber(8, 3, 2);
        world.set_wall(at(1, 1, 1, 0), true).unwrap();
        world
    };
    let humanoid = seen(&world, at(1, 0, 1, 1));
    let rat = seen(&world, at(1, 0, 1, 0));
    assert!(
        !humanoid.contains(&(1, 1, -1)),
        "floor under the wall has no exposed face"
    );
    assert!(
        !humanoid.contains(&(2, 1, -1)),
        "floor just behind the wall"
    );
    assert!(
        humanoid.contains(&(3, 1, -1)),
        "floor further back, seen over the wall"
    );
    assert!(humanoid.contains(&(1, 1, 0)), "the wall itself");
    for x in 2..=5 {
        assert!(
            !rat.contains(&(x, 1, -1)),
            "rat sees no floor behind the wall at x={x}"
        );
    }
    assert!(
        rat.contains(&(1, 1, 1)),
        "rat sees the air just above the wall"
    );
    assert!(
        !rat.contains(&(4, 1, 1)),
        "a low line to air further back cuts the wall"
    );
}

#[test]
fn head_height_air_is_seen_from_every_eye_height() {
    let world = chamber(6, 3, 2);
    for eye_z in 0..2 {
        let visible = seen(&world, at(1, 0, 1, eye_z));
        for x in 0..6 {
            assert!(
                visible.contains(&(x, 1, 1)),
                "air at x={x} from eye height {eye_z}"
            );
        }
    }
}

#[test]
fn a_low_creature_can_see_legs_its_taller_neighbour_cannot_see_back() {
    // A shelf at head height in front of the humanoid; the rat is beyond it.
    let mut world = chamber(6, 3, 2);
    world.set_wall(at(1, 1, 1, 1), true).unwrap();
    world.set_wall(at(1, 2, 1, 1), true).unwrap();
    let humanoid_eye = seen(&world, at(1, 0, 1, 1));
    let rat_eye = seen(&world, at(1, 3, 1, 0));
    assert!(rat_eye.contains(&(0, 1, 0)), "rat sees the humanoid's legs");
    assert!(
        !humanoid_eye.contains(&(3, 1, 0)),
        "humanoid's eye can't see the rat"
    );
    assert!(
        !rat_eye.contains(&(0, 1, 1)),
        "eye-to-eye sight stays reciprocal"
    );
}

#[test]
fn sight_between_empty_cells_is_reciprocal_in_random_volumes() {
    for seed in 1..=4u64 {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed);
        let (w, d, h) = (5, 5, 3);
        let mut world = chamber(w, d, h);
        let mut empty = Vec::new();
        for z in 0..h {
            for y in 0..d {
                for x in 0..w {
                    if rng.chance(25) {
                        world.set_wall(at(1, x, y, z), true).unwrap();
                    } else {
                        empty.push((x, y, z));
                    }
                }
            }
        }
        let scenes: Vec<_> = empty
            .iter()
            .map(|&(x, y, z)| seen(&world, at(1, x, y, z)))
            .collect();
        for (i, a) in empty.iter().enumerate() {
            for (j, b) in empty.iter().enumerate() {
                assert_eq!(
                    scenes[i].contains(b),
                    scenes[j].contains(a),
                    "seed {seed}: {a:?} and {b:?}"
                );
            }
        }
    }
}

/// Whether the eye-to-centre line for `offset` passes exactly through the
/// centre of a face of some opaque cell: a bevel tip, which only touches.
fn grazes_face_centre(world: &World, eye: Location, offset: (i32, i32)) -> bool {
    let (dx, dy) = (2 * offset.0, 2 * offset.1);
    (-9..=9).any(|x: i32| {
        (-9..=9).any(|y: i32| {
            let cell = at(1, eye.position.x + x, eye.position.y + y, 0);
            world.opaque(cell)
                && [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|(fx, fy)| {
                    let (px, py) = (2 * x + fx, 2 * y + fy);
                    px * dy == py * dx
                        && px * dx + py * dy > 0
                        && px.abs() <= dx.abs()
                        && py.abs() <= dy.abs()
                })
        })
    })
}

/// On single-level maps, empty cells match Ford's 2D shadowcasting except where
/// a line passes exactly through a bevel tip. Touching never blocks in 3D; Ford's
/// row scan breaks those exact ties by rounding. Wall cells differ because faces
/// are tested at their centres rather than by any exposed portion.
#[test]
fn single_level_maps_match_two_dimensional_shadowcasting_for_open_cells() {
    let mut wall_differences = 0;
    let mut grazes = 0;
    for seed in 1..=40u64 {
        let mut rng = Rng(0xD1B5_4A32_D192_ED03 ^ seed);
        let region = Region {
            id: RegionId(1),
            name: "plane".into(),
            bounds: Extent::new(11, 11, 1).unwrap(),
        };
        let mut world = World::new(vec![region], vec![]).unwrap();
        for y in 0..11 {
            for x in 0..11 {
                if (x, y) != (5, 5) && rng.chance(30) {
                    world.set_wall(at(1, x, y, 0), true).unwrap();
                }
            }
        }
        let eye = at(1, 5, 5, 0);
        let split = |cells: Vec<tor_world::SightCell>| {
            let mut open = BTreeSet::new();
            let mut walls = BTreeSet::new();
            for c in cells {
                let key = (c.offset.x, c.offset.y, c.offset.z);
                if world.opaque(c.location) {
                    walls.insert(key);
                } else {
                    open.insert(key);
                }
            }
            (open, walls)
        };
        let (open3, walls3) = split(world.eye_scene(eye, 0, 8));
        let (open2, walls2) = split(world.shadow_scene(eye, 0, 8));
        assert!(
            open2.is_subset(&open3),
            "seed {seed}: 3D hides an open cell"
        );
        for extra in open3.difference(&open2) {
            assert!(
                grazes_face_centre(&world, eye, (extra.0, extra.1)),
                "seed {seed}: {extra:?} is extra without an exact bevel-tip graze"
            );
            grazes += 1;
        }
        wall_differences += walls3.symmetric_difference(&walls2).count();
    }
    assert!(grazes > 0, "the fixture exercises exact bevel-tip grazes");
    eprintln!("bevel-tip grazes: {grazes}; wall-cell differences: {wall_differences}");
}

#[test]
fn dividing_a_space_into_regions_does_not_change_the_3d_scene() {
    let whole = World::new(
        vec![Region {
            id: RegionId(1),
            name: "whole".into(),
            bounds: Extent::new(10, 5, 2).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    let mut split = World::new(
        vec![
            Region {
                id: RegionId(1),
                name: "a".into(),
                bounds: Extent::new(5, 5, 2).unwrap(),
            },
            Region {
                id: RegionId(2),
                name: "b".into(),
                bounds: Extent::new(5, 5, 2).unwrap(),
            },
        ],
        vec![],
    )
    .unwrap();
    split
        .connect_area(
            Passage {
                from: at(1, 4, 0, 0),
                direction: Direction::East,
                to: at(2, 0, 0, 0),
            },
            0,
            5,
            2,
        )
        .unwrap();
    for eye in [at(1, 3, 2, 0), at(1, 3, 2, 1)] {
        let view = |world: &World| {
            world
                .eye_scene(eye, 0, 8)
                .iter()
                .map(|c| (c.offset, c.wall))
                .collect::<Vec<_>>()
        };
        assert_eq!(view(&split), view(&whole));
    }
}

fn stone_room(world: &mut World, id: u64, (w, d, h): (i32, i32, i32)) {
    world
        .add_chamber(Region {
            id: RegionId(id),
            name: "room".into(),
            bounds: Extent::new(w, d, h).unwrap(),
        })
        .unwrap();
}

fn shape(world: &World, eye: Location, frame: u8) -> Vec<(Position, bool)> {
    world
        .eye_scene(eye, frame, 8)
        .into_iter()
        .map(|c| (c.offset, c.wall))
        .collect()
}

/// The far half of a 6x3x2 room stored in region 2 under cube rotation `r`,
/// shifted so its storage starts at zero. Returns the local-to-storage map.
fn rotated_half(r: u8) -> (World, impl Fn(i32, i32, i32) -> Location) {
    use tor_world::{inverse_rotation, rotate_vector};
    let dims = [3i64, 3, 2];
    let corners: Vec<[i64; 3]> = (0..8)
        .map(|i| {
            rotate_vector(
                r,
                [0, 1, 2].map(|a| if i >> a & 1 == 1 { dims[a] - 1 } else { 0 }),
            )
        })
        .collect();
    let min = [0, 1, 2].map(|a| corners.iter().map(|c| c[a]).min().unwrap());
    let size = rotate_vector(r, dims).map(|v| v.abs() as i32);
    let remote = move |x: i32, y: i32, z: i32| {
        let p = rotate_vector(r, [x, y, z].map(i64::from));
        at(
            2,
            (p[0] - min[0]) as i32,
            (p[1] - min[1]) as i32,
            (p[2] - min[2]) as i32,
        )
    };
    let mut world = World::new(vec![], vec![]).unwrap();
    stone_room(&mut world, 1, (3, 3, 2));
    stone_room(&mut world, 2, (size[0], size[1], size[2]));
    world
        .connect_area(
            Passage {
                from: at(1, 2, 0, 0),
                direction: Direction::East,
                to: remote(0, 0, 0),
            },
            r,
            3,
            2,
        )
        .unwrap();
    // The reverse join is anchored at the face's minimum storage corner.
    let face: Vec<_> = (0..3)
        .flat_map(|y| (0..2).map(move |z| (y, z)))
        .map(|(y, z)| (remote(0, y, z), (y, z)))
        .collect();
    let (anchor, (y0, z0)) = *face.iter().min_by_key(|(l, _)| l.position).unwrap();
    let back = Direction::West.rotated(r);
    let extent = |axis: fn(&Position) -> i32| {
        let values: Vec<_> = face.iter().map(|(l, _)| axis(&l.position)).collect();
        (values.iter().max().unwrap() - values.iter().min().unwrap() + 1) as u16
    };
    let (width, height) = match back {
        Direction::East | Direction::West => (extent(|p| p.y), extent(|p| p.z)),
        Direction::North | Direction::South => (extent(|p| p.x), extent(|p| p.z)),
        _ => (extent(|p| p.x), extent(|p| p.y)),
    };
    let passage = Passage {
        from: anchor,
        direction: back,
        to: at(1, 2, y0, z0),
    };
    // A vertical join must be physical; plain vertical links are stairs.
    if matches!(back, Direction::Up | Direction::Down) {
        world.connect_portal_area(passage, inverse_rotation(r), width, height)
    } else {
        world.connect_area(passage, inverse_rotation(r), width, height)
    }
    .unwrap();
    (world, remote)
}

#[test]
fn a_room_split_under_every_cube_rotation_looks_unsplit_from_both_sides() {
    let mut whole = World::new(vec![], vec![]).unwrap();
    stone_room(&mut whole, 1, (6, 3, 2));
    for r in 0..24 {
        let (split, remote) = rotated_half(r);
        for x in 1..5 {
            for y in 0..3 {
                for z in 0..2 {
                    let (eye, frame) = if x < 3 {
                        (at(1, x, y, z), 0)
                    } else {
                        (remote(x - 3, y, z), r)
                    };
                    assert_eq!(
                        shape(&split, eye, frame),
                        shape(&whole, at(1, x, y, z), 0),
                        "rotation {r}, eye ({x}, {y}, {z})"
                    );
                }
            }
        }
    }
}

#[test]
fn a_narrow_rotated_doorway_looks_unsplit_with_the_door_open_or_closed() {
    for turns in 0..4u8 {
        let remote = |x: i32, y: i32, z: i32| {
            let (x, y) = match turns {
                0 => (x, y),
                1 => (2 - y, x),
                2 => (4 - x, 2 - y),
                _ => (y, 4 - x),
            };
            at(2, x, y, z)
        };
        let mut split = World::new(vec![], vec![]).unwrap();
        let mut whole = World::new(vec![], vec![]).unwrap();
        for (world, width) in [(&mut split, 6), (&mut whole, 11)] {
            stone_room(world, 1, (width, 3, 2));
            for y in [0, 2] {
                for z in [0, 1] {
                    world.set_wall(at(1, 5, y, z), true).unwrap();
                }
            }
            world.place_door(at(1, 5, 1, 0), 1, true, 2).unwrap();
        }
        let far = if turns % 2 == 0 { (5, 3, 2) } else { (3, 5, 2) };
        stone_room(&mut split, 2, far);
        split
            .connect_area(
                Passage {
                    from: at(1, 5, 1, 0),
                    direction: Direction::East,
                    to: remote(0, 1, 0),
                },
                turns,
                1,
                2,
            )
            .unwrap();
        split
            .connect_area(
                Passage {
                    from: remote(0, 1, 0),
                    direction: Direction::West.rotated(turns),
                    to: at(1, 5, 1, 0),
                },
                (4 - turns) % 4,
                1,
                2,
            )
            .unwrap();
        for open in [true, false] {
            split.set_door(at(1, 5, 1, 0), open);
            whole.set_door(at(1, 5, 1, 0), open);
            for x in 0..11 {
                for y in 0..3 {
                    for z in 0..2 {
                        if !whole.walkable(at(1, x, y, z)) {
                            continue;
                        }
                        let (eye, frame) = if x < 6 {
                            (at(1, x, y, z), 0)
                        } else {
                            (remote(x - 6, y, z), turns)
                        };
                        assert_eq!(
                            shape(&split, eye, frame),
                            shape(&whole, at(1, x, y, z), 0),
                            "turns {turns}, eye ({x}, {y}, {z}), open {open}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn a_shaft_split_by_a_physical_vertical_portal_looks_unsplit() {
    let mut whole = World::new(vec![], vec![]).unwrap();
    stone_room(&mut whole, 1, (3, 3, 4));
    let mut split = World::new(vec![], vec![]).unwrap();
    stone_room(&mut split, 1, (3, 3, 2));
    stone_room(&mut split, 2, (3, 3, 2));
    split
        .connect_portal_area(
            Passage {
                from: at(1, 0, 0, 1),
                direction: Direction::Up,
                to: at(2, 0, 0, 0),
            },
            0,
            3,
            3,
        )
        .unwrap();
    split
        .connect_portal_area(
            Passage {
                from: at(2, 0, 0, 0),
                direction: Direction::Down,
                to: at(1, 0, 0, 1),
            },
            0,
            3,
            3,
        )
        .unwrap();
    for z in 0..4 {
        for y in 0..3 {
            for x in 0..3 {
                let eye = if z < 2 {
                    at(1, x, y, z)
                } else {
                    at(2, x, y, z - 2)
                };
                assert_eq!(
                    shape(&split, eye, 0),
                    shape(&whole, at(1, x, y, z), 0),
                    "eye ({x}, {y}, {z})"
                );
            }
        }
    }
}

#[test]
fn rotating_the_observer_frame_rotates_offsets_and_nothing_else() {
    use tor_world::rotate_vector;
    let mut world = chamber(5, 4, 3);
    world.set_wall(at(1, 3, 1, 0), true).unwrap();
    world.set_wall(at(1, 1, 2, 1), true).unwrap();
    let eye = at(1, 2, 2, 1);
    let upright: BTreeSet<_> = world
        .eye_scene(eye, 0, 8)
        .into_iter()
        .map(|c| (c.location, c.offset))
        .collect();
    for frame in 0..24 {
        let turned: BTreeSet<_> = world
            .eye_scene(eye, frame, 8)
            .into_iter()
            .map(|c| {
                let p = rotate_vector(frame, [c.offset.x, c.offset.y, c.offset.z].map(i64::from));
                let offset = Position {
                    x: p[0] as i32,
                    y: p[1] as i32,
                    z: p[2] as i32,
                };
                (c.location, offset)
            })
            .collect();
        assert_eq!(turned, upright, "frame {frame}");
    }
}

#[test]
fn abstract_stair_links_are_not_seen_through() {
    let mut world = World::new(vec![], vec![]).unwrap();
    stone_room(&mut world, 1, (3, 3, 2));
    stone_room(&mut world, 2, (3, 3, 2));
    world
        .connect(
            Passage {
                from: at(1, 1, 1, 1),
                direction: Direction::Up,
                to: at(2, 1, 1, 0),
            },
            0,
        )
        .unwrap();
    for z in 0..2 {
        let scene = world.eye_scene(at(1, 1, 1, z), 0, 8);
        assert!(scene.iter().all(|c| c.location.region == RegionId(1)));
        assert!(
            scene.iter().any(|c| c.location == at(1, 1, 1, 2)),
            "the physical ceiling above the stair is seen instead"
        );
    }
}

fn assert_matches_reference(world: &World, eye: Location, frame: u8, radius: u8, context: &str) {
    assert_eq!(
        world.eye_scene_uncached(eye, frame, radius),
        world.eye_scene_reference(eye, frame, radius),
        "{context}: eye {eye:?}, frame {frame}, radius {radius}"
    );
}

#[test]
fn accelerated_scene_matches_the_reference_in_random_rooms_with_doors() {
    for seed in 1..=12u64 {
        let mut rng = Rng(0xA076_1D64_78BD_642F ^ seed);
        let (w, d, h) = (7, 6, 3);
        let mut world = chamber(w, d, h);
        let mut doors = Vec::new();
        for z in 0..h {
            for y in 0..d {
                for x in 0..w {
                    let roll = rng.next() % 100;
                    if roll < 20 {
                        world.set_wall(at(1, x, y, z), true).unwrap();
                    } else if roll < 26 {
                        doors.push((at(1, x, y, z), roll < 23));
                    }
                }
            }
        }
        // Each door fills its opening; cells a lower door already fills are skipped.
        for (door_id, (location, open)) in (1..).zip(doors) {
            let height = world.door_clearance(location);
            if height > 0 {
                world.place_door(location, door_id, open, height).unwrap();
            }
        }
        for _ in 0..6 {
            let eye = at(
                1,
                (rng.next() % w as u64) as i32,
                (rng.next() % d as u64) as i32,
                (rng.next() % h as u64) as i32,
            );
            let frame = (rng.next() % 24) as u8;
            for radius in [3, 8] {
                assert_matches_reference(&world, eye, frame, radius, &format!("seed {seed}"));
            }
        }
    }
}

#[test]
fn accelerated_scene_matches_the_reference_across_portals_cycles_and_stairs() {
    for r in 0..24 {
        let (world, remote) = rotated_half(r);
        for (eye, frame) in [
            (at(1, 1, 1, 1), 0),
            (at(1, 2, 0, 0), 0),
            (remote(0, 2, 1), r),
            (remote(2, 1, 0), r),
        ] {
            assert_matches_reference(&world, eye, frame, 8, &format!("rotation {r}"));
        }
    }
    let mut cycle = World::new(
        vec![Region {
            id: RegionId(1),
            name: "cycle".into(),
            bounds: Extent::new(3, 3, 2).unwrap(),
        }],
        vec![],
    )
    .unwrap();
    cycle
        .connect_area(
            Passage {
                from: at(1, 2, 0, 0),
                direction: Direction::East,
                to: at(1, 0, 0, 0),
            },
            0,
            3,
            2,
        )
        .unwrap();
    for frame in 0..24 {
        assert_matches_reference(&cycle, at(1, 1, 1, 0), frame, 8, "cycle");
    }
    let mut stairs = World::new(vec![], vec![]).unwrap();
    stone_room(&mut stairs, 1, (4, 4, 2));
    stone_room(&mut stairs, 2, (4, 4, 2));
    stairs
        .connect(
            Passage {
                from: at(1, 1, 1, 1),
                direction: Direction::Up,
                to: at(2, 1, 1, 0),
            },
            0,
        )
        .unwrap();
    for z in 0..2 {
        assert_matches_reference(&stairs, at(1, 1, 1, z), 0, 8, "stairs");
        assert_matches_reference(&stairs, at(1, 2, 2, z), 5, 8, "stairs");
    }
}

#[test]
fn accelerated_scene_matches_the_reference_in_the_first_dungeon_layout() {
    // 7x5x2 rooms in a row, joined by one-wide, two-high doorways.
    let mut world = World::new(vec![], vec![]).unwrap();
    for id in 1..=3 {
        stone_room(&mut world, id, (7, 5, 2));
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
    world.place_door(at(2, 6, 2, 0), 1, false, 2).unwrap();
    for open in [false, true] {
        world.set_door(at(2, 6, 2, 0), open);
        for region in 1..=3 {
            for z in 0..2 {
                for y in 0..5 {
                    for x in 0..7 {
                        let eye = at(region, x, y, z);
                        if world.walkable(eye) {
                            assert_matches_reference(&world, eye, 0, 8, &format!("open {open}"));
                        }
                    }
                }
            }
        }
    }
}

/// Two 5x3x2 rooms joined by a one-wide, two-high doorway at (4, 1).
fn doorway_rooms() -> World {
    let mut world = World::new(vec![], vec![]).unwrap();
    stone_room(&mut world, 1, (5, 3, 2));
    stone_room(&mut world, 2, (5, 3, 2));
    for (from, direction, to) in [
        (at(1, 4, 1, 0), Direction::East, at(2, 0, 1, 0)),
        (at(2, 0, 1, 0), Direction::West, at(1, 4, 1, 0)),
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
    // Wall in the doorway's sides, so it is a one-wide, two-high gap.
    for y in [0, 2] {
        for z in 0..2 {
            world.set_wall(at(1, 4, y, z), true).unwrap();
        }
    }
    world
}

#[test]
fn a_closed_door_filling_its_doorway_hides_the_far_room_from_every_eye_height() {
    let mut world = doorway_rooms();
    let base = at(1, 4, 1, 0);
    assert_eq!(world.door_clearance(base), 2);
    assert!(
        world.place_door(base, 1, false, 3).is_err(),
        "taller than the doorway"
    );
    // A one-cell door leaves the walled doorway open above it: a humanoid
    // sees over it. Package validation rejects that.
    assert!(world.doorway_open_above(base, 1));
    assert!(!world.doorway_open_above(base, 2));
    let mut low = world.clone();
    low.place_door(base, 1, false, 1).unwrap();
    assert!(low
        .eye_scene(at(1, 1, 1, 1), 0, 8)
        .iter()
        .any(|c| c.location.region == RegionId(2)));
    world.place_door(base, 1, false, 2).unwrap();
    for eye_z in 0..2 {
        let scene = world.eye_scene(at(1, 1, 1, eye_z), 0, 8);
        assert!(
            scene.iter().all(|c| c.location.region == RegionId(1)),
            "eye height {eye_z} sees past the closed door"
        );
        // Both door cells are seen, as the same door.
        for z in 0..2 {
            let door = scene.iter().find(|c| c.location == at(1, 4, 1, z)).unwrap();
            assert_eq!(world.door(door.location).map(|d| d.id), Some(1));
        }
    }
    // Opening it from its upper cell opens the whole door.
    world.set_door(at(1, 4, 1, 1), true);
    assert!(!world.opaque(at(1, 4, 1, 0)));
    let scene = world.eye_scene(at(1, 1, 1, 1), 0, 8);
    assert!(scene.iter().any(|c| c.location.region == RegionId(2)));
}

#[test]
fn door_cells_cannot_become_walls_or_hold_a_second_door() {
    let mut world = doorway_rooms();
    world.place_door(at(1, 4, 1, 0), 7, true, 2).unwrap();
    assert_eq!(world.door_location(7), Some(at(1, 4, 1, 0)));
    assert_eq!(
        world.door_cells(at(1, 4, 1, 0)).collect::<Vec<_>>(),
        vec![at(1, 4, 1, 0), at(1, 4, 1, 1)]
    );
    assert!(world.set_wall(at(1, 4, 1, 1), true).is_err());
    assert_eq!(world.door_clearance(at(1, 4, 1, 1)), 0);
    assert!(world.place_door(at(1, 4, 1, 1), 8, true, 1).is_err());
    assert!(world.checkpoint_valid(8));
}
