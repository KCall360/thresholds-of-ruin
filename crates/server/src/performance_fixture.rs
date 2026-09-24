//! Versioned diagnostic geometry shared by server, benchmark, and client driver.
use crate::{adapt, Failure};
use serde::Deserialize;
use tor_simulation::Game;
use tor_world::{Extent, Location, Passage, Position, Region, RegionId, World};

pub const SPEC: &str = include_str!("../fixtures/performance-v1.json");

#[derive(Deserialize)]
pub(crate) struct Fixture {
    pub version: u32,
    pub geometry: Geometry,
}
#[derive(Deserialize)]
pub(crate) struct Geometry {
    extent: [i32; 3],
    walls: Vec<[i32; 3]>,
    doors: Vec<[i32; 3]>,
    stairs: Vec<Link>,
    joins: Vec<Link>,
    items: Vec<Item>,
    pub actors: Vec<[i32; 3]>,
}
#[derive(Deserialize)]
struct Link {
    from: [i32; 3],
    to: [i32; 3],
    direction: tor_protocol::Direction,
    return_direction: Option<tor_protocol::Direction>,
    turns: u8,
}
#[derive(Deserialize)]
struct Item {
    at: [i32; 3],
    name: String,
    region: Option<u64>,
}
fn at(region: u64, [x, y, z]: [i32; 3]) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x, y, z },
    }
}
pub(crate) fn specification() -> Fixture {
    serde_json::from_str(SPEC).expect("tested fixture specification")
}
pub(crate) fn game(seed: u64, regions: u64) -> Result<Game, Failure> {
    let fixture = specification();
    let g = fixture.geometry;
    let [width, depth, height] = g.extent;
    let rooms = (1..=regions)
        .map(|id| Region {
            id: RegionId(id),
            name: format!("Region {id}"),
            bounds: Extent::new(width, depth, height).expect("fixture extent"),
        })
        .collect();
    let mut world = World::new(rooms, vec![]).expect("fixture regions");
    for region in 1..=regions {
        for &wall in &g.walls {
            world
                .set_wall(at(region, wall), true)
                .expect("fixture wall");
        }
        for link in &g.stairs {
            world
                .connect(
                    Passage {
                        from: at(region, link.from),
                        to: at(region, link.to),
                        direction: adapt::direction(link.direction),
                    },
                    link.turns,
                )
                .expect("fixture stairs");
        }
        if region < regions {
            for link in &g.joins {
                world
                    .connect(
                        Passage {
                            from: at(region, link.from),
                            to: at(region + 1, link.to),
                            direction: adapt::direction(link.direction),
                        },
                        link.turns,
                    )
                    .expect("fixture join");
                world
                    .connect(
                        Passage {
                            from: at(region + 1, link.to),
                            to: at(region, link.from),
                            direction: adapt::direction(
                                link.return_direction.expect("return join"),
                            ),
                        },
                        (4 - link.turns) % 4,
                    )
                    .expect("fixture return join");
            }
        }
    }
    let mut game = Game::new(world, seed);
    for region in 1..=regions {
        for &door in &g.doors {
            game.place_door(at(region, door), false)
                .expect("fixture door");
        }
        for item in &g.items {
            if item.region.is_none_or(|id| id == region) {
                game.place_item(at(region, item.at), item.name.clone())
                    .expect("fixture item");
            }
        }
    }
    Ok(game)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_simulation::{Action, OutcomeKind};
    use tor_world::Direction;

    #[test]
    fn rotated_join_reveals_and_hides_named_entities_and_cells_in_both_directions() {
        let mut game = game(42, 8).unwrap();
        let actor = game
            .spawn_actor(at(1, [4, 0, 1]), NonZeroU64::new(100).unwrap())
            .unwrap();
        let before = game.observe(actor).unwrap();
        let has = |view: &tor_simulation::Observation, name: &str| {
            view.ground_items.iter().any(|i| i.name == name)
        };
        assert!(has(&before, "rotated hide marker"));
        assert!(!has(&before, "rotated reveal marker"));
        let result = game.act(actor, Action::Move(Direction::North)).unwrap();
        assert_eq!(
            result.kind,
            OutcomeKind::Moved {
                from: at(1, [4, 0, 1]),
                to: at(2, [0, 4, 1])
            }
        );
        let after = game.observe(actor).unwrap();
        assert!(!after
            .visible_cells
            .iter()
            .any(|c| c.location == at(1, [4, 8, 1])));
        assert!(after
            .visible_cells
            .iter()
            .any(|c| c.location == at(2, [8, 4, 1])));
        assert!(has(&after, "rotated reveal marker"));
        game.act(actor, Action::Move(Direction::South)).unwrap();
        assert_eq!(
            game.observe(actor).unwrap().visible_cells,
            before.visible_cells
        );
    }

    #[test]
    fn obstacle_route_changes_named_item_los_and_diagonal_cost_is_142() {
        let mut game = game(42, 1).unwrap();
        let actor = game
            .spawn_actor(at(1, [2, 4, 0]), NonZeroU64::new(100).unwrap())
            .unwrap();
        assert!(!game
            .observe(actor)
            .unwrap()
            .ground_items
            .iter()
            .any(|i| i.name == "occluded marker"));
        let diagonal = game.act(actor, Action::Move(Direction::NorthWest)).unwrap();
        assert_eq!(diagonal.next_tick - diagonal.at_tick, 142);
        for _ in 0..2 {
            game.act(actor, Action::Move(Direction::North)).unwrap();
        }
        assert!(game
            .observe(actor)
            .unwrap()
            .ground_items
            .iter()
            .any(|i| i.name == "occluded marker"));
    }

    #[test]
    fn each_scale_has_exactly_the_requested_connected_regions() {
        for regions in [1, 8, 64, 256] {
            let mut game = game(42, regions).unwrap();
            let actor = game
                .spawn_actor(at(1, [8, 4, 0]), NonZeroU64::new(100).unwrap())
                .unwrap();
            for region in 1..=regions {
                game.teleport(actor, at(region, [8, 4, 0])).unwrap();
                let result = game.act(actor, Action::Move(Direction::East));
                if region == regions {
                    assert!(result.is_err());
                } else {
                    assert_eq!(
                        game.observe(actor).unwrap().location.region,
                        RegionId(region + 1)
                    );
                }
            }
            assert!(game.teleport(actor, at(regions + 1, [0, 4, 0])).is_err());
        }
    }
}
