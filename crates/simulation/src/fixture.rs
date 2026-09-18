use tor_world::{Direction, Extent, Location, Passage, Position, Region, RegionId, World};

use crate::Game;

impl Game {
    /// A hand-authored two-room scenario with seed-selected collectible material.
    ///
    /// This is not a procedural dungeon generator. No PRNG is needed for this
    /// fixture; future random mechanics must retain explicit generator state.
    /// The game starts without actors; callers choose spawn and timing explicitly.
    pub fn two_room(seed: u64) -> Self {
        let entry = RegionId(1);
        let gallery = RegionId(2);
        let boundary = Location {
            region: entry,
            position: Position { x: 4, y: 1, z: 0 },
        };
        let landing = Location {
            region: gallery,
            position: Position { x: 0, y: 1, z: 0 },
        };
        let bounds = Extent::new(5, 3, 1).expect("positive fixture dimensions");
        let world = World::new(
            vec![
                Region {
                    id: entry,
                    name: "Entry chamber".into(),
                    bounds,
                },
                Region {
                    id: gallery,
                    name: "Gallery".into(),
                    bounds,
                },
            ],
            vec![
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
            ],
        )
        .expect("valid fixture topology");
        let mut game = Self::new(world, seed);
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
