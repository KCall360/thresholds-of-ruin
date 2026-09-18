use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

use crate::Game;

impl Game {
    /// Historical map with two unnamed room anchors.
    /// The original fixture remains available for deterministic old-save replay.
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
        game.place_door(
            Location {
                region: RegionId(1),
                position: Position { x: 5, y: 1, z: 0 },
            },
            true,
        )
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
        game.material_surfaces = enclosed;
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
