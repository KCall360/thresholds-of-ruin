//! Physical traversal is release-only: it observes actual occupancy, never an
//! aimed actor or the launcher's perception. Terrain owns crossing rotations.
use super::{Placement, RuleError, StepDirection};
use crate::weapon_profiles::{RangeBands, PROJECTILE_STEP_FEET};
use crate::{ActorId, Game, GameError};
use tor_world::{Direction, Location, RayStep};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProjectileImpact {
    pub location: Location,
    pub distance_feet: u32,
    pub target: Option<ActorId>,
}

impl Game {
    /// Called inside the paid release transaction. Settling a prospective
    /// recipient may remove it; the first still-live creature stops flight.
    /// No terrain generation, reflection or repeated encounter is permitted.
    pub(super) fn trace_projectile_flight(
        &mut self,
        actor: ActorId,
        origin: Placement,
        direction: StepDirection,
        launch_footprint: &[Location],
        range: RangeBands,
    ) -> Result<ProjectileImpact, GameError> {
        if range.validate().is_err()
            || launch_footprint.is_empty()
            || launch_footprint.len() > 64
            || !launch_footprint.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(GameError::Rule(RuleError::InvalidPlan));
        }
        let mut impact = ProjectileImpact {
            location: origin.location,
            distance_feet: 0,
            target: None,
        };
        let mut direction = Direction::from(direction).rotated(origin.orientation);
        let mut left_launch_footprint = false;
        for _ in 0..range.long_feet / PROJECTILE_STEP_FEET {
            if self.region_frozen(impact.location.region) {
                break;
            }
            let RayStep::Clear { to, rotation } = self.world.ray_step(impact.location, direction)
            else {
                break;
            };
            if self.region_frozen(to.region) {
                break;
            }
            impact.location = to;
            impact.distance_feet += PROJECTILE_STEP_FEET;
            direction = direction.rotated(rotation);
            left_launch_footprint |= launch_footprint.binary_search(&to).is_err();
            let occupants: Vec<_> = self.actors.at(&self.world, to).keys().copied().collect();
            for target in occupants {
                if (target == actor && !left_launch_footprint)
                    || self.actor_frozen(target)
                    || !self.actors[&target].alive()
                    || self.creature(target).is_none()
                {
                    continue;
                }
                self.settle_actor_time(target)?;
                if self.actors.get(&target).is_some_and(|value| value.alive())
                    && self
                        .creature(target)
                        .is_some_and(|value| !value.health().dead())
                {
                    impact.target = Some(target);
                    return Ok(impact);
                }
            }
        }
        Ok(impact)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon_profiles::MAX_PROJECTILE_STEPS;
    use tor_world::{Passage, Position, RegionId};

    fn at(x: i32) -> Location {
        Location {
            region: RegionId(1),
            position: Position { x, y: 1, z: 0 },
        }
    }
    fn trace(game: &mut Game, long_feet: u32) -> ProjectileImpact {
        let origin = Placement::capture(game, ActorId(1)).unwrap();
        game.trace_projectile_flight(
            ActorId(1),
            origin,
            StepDirection::East,
            &[at(1)],
            RangeBands {
                short_feet: 5,
                long_feet,
            },
        )
        .unwrap()
    }
    fn fixture() -> Game {
        super::super::ray_tests::fixture_with(|_, _, _| {})
    }
    #[test]
    fn projectile_flight_stops_on_first_actual_creature_or_authored_distance() {
        let mut game = fixture();
        let before_world = game.world.clone();
        let before_rng = game.rules_rng;
        let short = trace(&mut game, 5);
        assert_eq!(
            short,
            ProjectileImpact {
                location: at(2),
                distance_feet: 5,
                target: None
            }
        );
        let hit = trace(&mut game, 60);
        assert_eq!(
            hit,
            ProjectileImpact {
                location: at(3),
                distance_feet: 10,
                target: Some(ActorId(2))
            }
        );
        assert_eq!(game.world, before_world);
        assert_eq!(game.rules_rng, before_rng);
    }
    #[test]
    fn projectile_flight_uses_open_doors_and_never_reflects_from_closed_terrain() {
        let mut game = fixture();
        game.world.place_door(at(2), 33, false, 1).unwrap();
        assert_eq!(
            trace(&mut game, 60),
            ProjectileImpact {
                location: at(1),
                distance_feet: 0,
                target: None
            }
        );
        game.world.set_door(at(2), true);
        assert_eq!(trace(&mut game, 60).target, Some(ActorId(2)));
        let mut game = fixture();
        game.world.set_wall(at(2), true).unwrap();
        assert_eq!(trace(&mut game, 60).distance_feet, 0);
    }
    #[test]
    fn projectile_flight_self_join_exhausts_exact_budget_without_generation() {
        let mut game = fixture();
        game.world
            .connect(
                Passage {
                    from: at(0),
                    direction: Direction::West,
                    to: at(0),
                },
                0,
            )
            .unwrap();
        game.actors.get_mut(&ActorId(1)).unwrap().location = at(0);
        let origin = Placement::capture(&game, ActorId(1)).unwrap();
        let before = game.world.clone();
        let maximum = MAX_PROJECTILE_STEPS * PROJECTILE_STEP_FEET;
        assert_eq!(
            game.trace_projectile_flight(
                ActorId(1),
                origin,
                StepDirection::West,
                &[at(0)],
                RangeBands {
                    short_feet: 5,
                    long_feet: maximum
                }
            )
            .unwrap(),
            ProjectileImpact {
                location: at(0),
                distance_feet: maximum,
                target: None
            }
        );
        assert_eq!(game.world, before);
    }
}
