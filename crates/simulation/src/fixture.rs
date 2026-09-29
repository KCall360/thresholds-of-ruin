use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

use crate::Game;

impl Game {
    /// Two-room diagnostic fixture with authored room anchors.
    pub fn two_room_with_place_hints(seed: u64) -> Self {
        let mut game = Self::two_room(seed);
        game.add_room_hints();
        game
    }

    /// Two 5x3 rooms separated by a one-cell hall, with no doorway place hint.
    pub fn two_room_with_doorway(seed: u64) -> Self {
        Self::doorway_fixture(seed, false)
    }

    /// Five-foot cubes: two-cell-high voids carved inside finite stone volumes.
    pub fn two_room_in_stone(seed: u64) -> Self {
        Self::doorway_fixture(seed, true)
    }

    fn doorway_fixture(seed: u64, enclosed: bool) -> Self {
        let mut game = Self::two_room_layout(seed, true, enclosed);
        game.add_room_hints();
        let doorway = Location {
            region: RegionId(1),
            position: Position { x: 5, y: 1, z: 0 },
        };
        // The door reaches the ceiling, however tall this layout makes it.
        let height = game.world.door_clearance(doorway);
        game.place_door(doorway, true, height)
            .expect("valid hallway door");
        game
    }

    fn add_room_hints(&mut self) {
        for region in [RegionId(1), RegionId(2)] {
            self.set_place_hint(
                Location {
                    region,
                    position: Position { x: 2, y: 1, z: 0 },
                },
                true,
            )
            .expect("valid authored anchor");
        }
    }

    /// `count` open 12x3 regions in a row, each joined to the next along its
    /// whole east face, for region lifecycle tests. Sight (8 cells) from
    /// x = 2 stays inside a region. No actors or items.
    pub fn region_corridor(seed: u64, count: u64) -> Self {
        let mut world = World::new(vec![], vec![]).unwrap();
        for id in 1..=count {
            world
                .add_region(Region {
                    id: RegionId(id),
                    name: format!("Corridor {id}"),
                    bounds: Extent::new(12, 3, 1).expect("positive fixture dimensions"),
                })
                .expect("valid fixture region");
        }
        let at = |region, x| Location {
            region: RegionId(region),
            position: Position { x, y: 0, z: 0 },
        };
        for id in 1..count {
            for (from, direction, to) in [
                (at(id, 11), Direction::East, at(id + 1, 0)),
                (at(id + 1, 0), Direction::West, at(id, 11)),
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
                        1,
                    )
                    .expect("valid fixture topology");
            }
        }
        Self::new(world, seed)
    }

    /// A hand-authored two-room scenario with seed-selected collectible material.
    ///
    /// This is not a procedural dungeon generator. No PRNG is needed for this
    /// fixture; future random mechanics must retain explicit generator state.
    /// The game starts without actors; callers choose spawn and timing explicitly.
    pub fn two_room(seed: u64) -> Self {
        Self::two_room_layout(seed, false, false)
    }

    fn two_room_layout(seed: u64, doorway: bool, enclosed: bool) -> Self {
        let entry = RegionId(1);
        let gallery = RegionId(2);
        let boundary = Location {
            region: entry,
            position: Position {
                x: if doorway { 5 } else { 4 },
                y: 1,
                z: 0,
            },
        };
        let landing = Location {
            region: gallery,
            position: Position { x: 0, y: 1, z: 0 },
        };
        let height = if enclosed { 2 } else { 1 };
        let bounds = Extent::new(5, 3, height).expect("positive fixture dimensions");
        let regions = vec![
            Region {
                id: entry,
                name: "Entry chamber".into(),
                bounds: Extent::new(if doorway { 6 } else { 5 }, 3, height).unwrap(),
            },
            Region {
                id: gallery,
                name: "Gallery".into(),
                bounds,
            },
        ];
        let passages = vec![
            Passage {
                from: boundary,
                direction: Direction::East,
                to: landing,
            },
            Passage {
                from: landing,
                direction: Direction::West,
                to: boundary,
            },
        ];
        let mut world = World::new(vec![], vec![]).unwrap();
        for region in regions {
            if enclosed {
                world.add_chamber(region)
            } else {
                world.add_region(region)
            }
            .expect("valid fixture region");
        }
        for passage in passages {
            world
                .connect_area(passage, 0, 1, height as u16)
                .expect("valid fixture topology");
        }
        let mut game = Self::new(world, seed);
        if doorway {
            // The hall shares storage with the entry room; its only floor is
            // (5,1). The flanking walls separate the rooms at every other row.
            for (y, z) in [0, 2]
                .into_iter()
                .flat_map(|y| (0..height).map(move |z| (y, z)))
            {
                game.set_wall(
                    Location {
                        region: entry,
                        position: Position { x: 5, y, z },
                    },
                    true,
                )
                .expect("valid hallway wall");
            }
        }
        let material = ["copper", "silver", "iron"][(seed % 3) as usize];
        game.place_item(
            Location {
                region: entry,
                position: Position { x: 1, y: 1, z: 0 },
            },
            format!("{material} token"),
        )
        .expect("valid fixture item location");
        game.place_item(
            Location {
                region: gallery,
                position: Position { x: 2, y: 1, z: 0 },
            },
            "stone tablet".into(),
        )
        .expect("valid fixture item location");
        game
    }
}
