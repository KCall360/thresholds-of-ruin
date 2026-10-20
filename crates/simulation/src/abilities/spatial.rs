//! Frozen voluntary movement commitments and bounded live translation facts.
use super::{AbilityPlan, PushDirection, StepDirection};
use crate::{ActorId, Game, GameError, MovementContext};
use serde::{Deserialize, Serialize};
use tor_world::Location;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub location: Location,
    pub orientation: u8,
}
impl<'de> Deserialize<'de> for Placement {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::location")]
            location: Location,
            orientation: u8,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        if fields.location.region.0 == 0 || fields.orientation >= 24 {
            return Err(serde::de::Error::custom("invalid placement"));
        }
        Ok(Self {
            location: fields.location,
            orientation: fields.orientation,
        })
    }
}
impl Placement {
    pub(crate) fn capture(game: &Game, actor: ActorId) -> Option<Self> {
        let actor = game.actors.get(&actor)?;
        Some(Self {
            location: actor.location,
            orientation: actor.orientation,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StepCommitment {
    pub direction: StepDirection,
    pub context: MovementContext,
    pub duration: u64,
}
impl<'de> Deserialize<'de> for StepCommitment {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            direction: StepDirection,
            context: MovementContext,
            duration: u64,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        let value = Self {
            direction: fields.direction,
            context: fields.context,
            duration: fields.duration,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid step commitment"));
        }
        Ok(value)
    }
}
impl StepCommitment {
    pub fn destination(self) -> Location {
        self.context.destination()
    }
    pub(crate) fn valid(self) -> bool {
        self.context.valid() && (1..=crate::limits::NUMERIC as u64).contains(&self.duration)
    }
    pub(crate) fn capture(
        game: &Game,
        actor: ActorId,
        direction: StepDirection,
    ) -> Result<Self, GameError> {
        let context = game
            .movement_context(actor, direction.into())
            .ok_or(GameError::Blocked)?;
        let actor = game.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        Ok(Self {
            direction,
            context,
            duration: actor.movement_duration(direction.into())?,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementMode {
    Voluntary,
    Forced,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MovementResult {
    Moved { from: Placement, to: Placement },
    Blocked,
    Ineligible,
    Stale,
    Ambiguous,
}
impl MovementResult {
    pub(crate) fn changed(self) -> bool {
        matches!(self, Self::Moved { from, to } if from != to)
    }
}
/// A batch contains at most one leading translation, so a single candidate is
/// sufficient and cannot be invalidated by earlier effects in the same batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpatialInput {
    pub placement: Placement,
    pub mode: MovementMode,
    pub result: MovementResult,
}
impl Game {
    pub(crate) fn step_input(&self, plan: &AbilityPlan) -> Option<SpatialInput> {
        let step = plan.step?;
        let placement = Placement::capture(self, plan.actor)?;
        let result = self
            .actor_translation(plan.actor, step.direction.into())
            .filter(|(location, orientation)| {
                step.context.matches(
                    placement.location,
                    placement.orientation,
                    *location,
                    *orientation,
                )
            })
            .map_or(MovementResult::Blocked, |(location, orientation)| {
                MovementResult::Moved {
                    from: placement,
                    to: Placement {
                        location,
                        orientation,
                    },
                }
            });
        Some(SpatialInput {
            placement,
            mode: MovementMode::Voluntary,
            result,
        })
    }
    pub(crate) fn push_input(
        &self,
        plan: &AbilityPlan,
        direction: PushDirection,
        contact: Option<&crate::combat::contact::MeleeContact>,
    ) -> Option<SpatialInput> {
        let placement = Placement::capture(self, plan.target)?;
        let result = (|| {
            let Some(source) = self
                .creature(plan.actor)
                .and_then(|creature| creature.derived().displacement)
            else {
                return MovementResult::Ineligible;
            };
            let Some(target) = self
                .creature(plan.target)
                .and_then(|creature| creature.derived().displacement)
            else {
                return MovementResult::Ineligible;
            };
            if !source.permits_push(target) {
                return MovementResult::Ineligible;
            }
            let Some(contact) = contact else {
                return MovementResult::Stale;
            };
            if matches!(contact, crate::combat::contact::MeleeContact::Ambiguous) {
                return MovementResult::Ambiguous;
            }
            let delta = match direction {
                PushDirection::AwayFromSource => {
                    contact.current_away(self, plan.actor, plan.target)
                }
            };
            let Some(delta) = delta else {
                return MovementResult::Stale;
            };
            let actor = self.actors.get(&plan.target).expect("captured target");
            self.translate_body(
                crate::PhysicsEntity::Actor(plan.target),
                actor.location,
                actor.orientation,
                &actor.body,
                delta,
            )
            .map_or(MovementResult::Blocked, |(location, orientation)| {
                MovementResult::Moved {
                    from: placement,
                    to: Placement {
                        location,
                        orientation,
                    },
                }
            })
        })();
        Some(SpatialInput {
            placement,
            mode: MovementMode::Forced,
            result,
        })
    }
}

impl<'de> Deserialize<'de> for MovementResult {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Moved { from: Placement, to: Placement },
            Blocked {},
            Ineligible {},
            Stale {},
            Ambiguous {},
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::Moved { from, to } => Self::Moved { from, to },
            Fields::Blocked {} => Self::Blocked,
            Fields::Ineligible {} => Self::Ineligible,
            Fields::Stale {} => Self::Stale,
            Fields::Ambiguous {} => Self::Ambiguous,
        })
    }
}
