use std::num::NonZeroU64;
use tor_simulation::{diagnostics::work_counts, Game, GameError};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

#[test]
fn repeated_occupancy_queries_do_not_resolve_unrelated_actor_bodies() {
    for unrelated in [16, 256, 4096] {
        let mut world = World::new(vec![], vec![]).unwrap();
        for region in 1..=unrelated + 1 {
            world
                .add_region(Region {
                    id: RegionId(region),
                    name: format!("room-{region}"),
                    bounds: Extent::new(16, 16, 2).unwrap(),
                })
                .unwrap();
        }
        let at = |region| Location {
            region: RegionId(region),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let mut game = Game::new(world, 42);
        let turn = NonZeroU64::new(100).unwrap();
        for region in 1..=unrelated + 1 {
            game.spawn_actor(at(region), turn).unwrap();
        }
        let occupied = at(unrelated + 1);
        // Warm derived data without changing authoritative actors or geometry.
        assert_eq!(game.spawn_actor(occupied, turn), Err(GameError::Occupied));
        let before = work_counts().body_cells;
        let candidates = work_counts().actor_candidates;
        assert_eq!(game.spawn_actor(occupied, turn), Err(GameError::Occupied));
        assert_eq!(
            work_counts().body_cells - before,
            0,
            "unrelated actors: {unrelated}"
        );
        let before = work_counts().body_cells;
        let view = game.observe(tor_simulation::ActorId(1)).unwrap();
        assert_eq!(
            work_counts().actor_candidates - candidates,
            1,
            "hidden actors entered disclosure work: {unrelated}"
        );
        assert!(view.visible_actors.is_empty());
        assert!(
            work_counts().body_cells - before <= 4,
            "observation resolved unrelated bodies: {unrelated}"
        );
    }
}
