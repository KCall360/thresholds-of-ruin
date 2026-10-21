//! Effective movement capabilities are source-composed, never species switches.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MovementChance {
    pub numerator: u16,
    pub denominator: u16,
}
impl MovementChance {
    pub fn valid(self) -> bool {
        self.denominator > 0 && self.numerator <= self.denominator
    }
    fn normalized(self) -> Self {
        let (mut a, mut b) = (self.numerator, self.denominator);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        Self {
            numerator: self.numerator / a,
            denominator: self.denominator / a,
        }
    }
}
impl<'de> Deserialize<'de> for MovementChance {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            numerator: u16,
            denominator: u16,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            numerator: fields.numerator,
            denominator: fields.denominator,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid movement chance"));
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocomotionValue {
    Stationary { enabled: bool },
    Flying { enabled: bool },
    Levitating { enabled: bool },
    Erratic { chance: Option<MovementChance> },
    Confused { chance: Option<MovementChance> },
}
impl<'de> Deserialize<'de> for LocomotionValue {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Stationary {
                enabled: bool,
            },
            Flying {
                enabled: bool,
            },
            Levitating {
                enabled: bool,
            },
            Erratic {
                #[serde(deserialize_with = "crate::bounded::required_option")]
                chance: Option<MovementChance>,
            },
            Confused {
                #[serde(deserialize_with = "crate::bounded::required_option")]
                chance: Option<MovementChance>,
            },
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Stationary { enabled } => Self::Stationary { enabled },
            Fields::Flying { enabled } => Self::Flying { enabled },
            Fields::Levitating { enabled } => Self::Levitating { enabled },
            Fields::Erratic { chance } => Self::Erratic { chance },
            Fields::Confused { chance } => Self::Confused { chance },
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocomotionCapabilities {
    pub stationary: bool,
    pub flying: bool,
    /// Hover without voluntary vertical control or access to ground interactions.
    pub levitating: bool,
    pub erratic: Option<MovementChance>,
    /// Execution-only redirection of fresh ordinary horizontal movement.
    pub confused: Option<MovementChance>,
}
impl LocomotionCapabilities {
    /// Gravity and ordinary ground contact are suppressed by either capability.
    /// Vertical navigation must still test `flying`, never this predicate.
    pub(crate) fn hovers(self) -> bool {
        self.flying || self.levitating
    }

    /// Grants combine by OR / strongest exact probability. Suppression wins
    /// across all active sources, independent of their ordering.
    pub(crate) fn from_modifiers<'a>(
        values: impl Iterator<Item = &'a crate::rules::Modifier>,
    ) -> Self {
        use crate::rules::{ModifierOp, Scope};
        let mut result = Self::default();
        let (
            mut stationary_off,
            mut flying_off,
            mut levitating_off,
            mut erratic_off,
            mut confused_off,
        ) = (false, false, false, false, false);
        for modifier in values.filter(|m| m.scope == Scope::Always) {
            if let ModifierOp::Locomotion { value } = modifier.op {
                match value {
                    LocomotionValue::Stationary { enabled } => {
                        result.stationary |= enabled;
                        stationary_off |= !enabled;
                    }
                    LocomotionValue::Flying { enabled } => {
                        result.flying |= enabled;
                        flying_off |= !enabled;
                    }
                    LocomotionValue::Levitating { enabled } => {
                        result.levitating |= enabled;
                        levitating_off |= !enabled;
                    }
                    LocomotionValue::Confused { chance: None } => confused_off = true,
                    LocomotionValue::Confused {
                        chance: Some(chance),
                    } => {
                        if result.confused.is_none_or(|old| {
                            u32::from(chance.numerator) * u32::from(old.denominator)
                                > u32::from(old.numerator) * u32::from(chance.denominator)
                        }) {
                            result.confused = Some(chance.normalized());
                        }
                    }
                    LocomotionValue::Erratic { chance: None } => erratic_off = true,
                    LocomotionValue::Erratic {
                        chance: Some(chance),
                    } => {
                        if result.erratic.is_none_or(|old| {
                            u32::from(chance.numerator) * u32::from(old.denominator)
                                > u32::from(old.numerator) * u32::from(chance.denominator)
                        }) {
                            result.erratic = Some(chance.normalized());
                        }
                    }
                }
            }
        }
        result.stationary &= !stationary_off;
        result.flying &= !flying_off;
        result.levitating &= !levitating_off;
        if erratic_off {
            result.erratic = None;
        }
        if confused_off {
            result.confused = None;
        }
        result
    }
}
impl crate::Actor {
    pub(crate) fn locomotion(&self) -> LocomotionCapabilities {
        self.combat
            .as_ref()
            .map_or_else(LocomotionCapabilities::default, |combat| {
                combat.creature().derived().locomotion
            })
    }
}
impl crate::Game {
    pub fn actor_locomotion(&self, id: crate::ActorId) -> Option<LocomotionCapabilities> {
        self.actors.get(&id).map(crate::Actor::locomotion)
    }
    pub(crate) fn locomotion_blocks_action(
        &self,
        id: crate::ActorId,
        action: &crate::Action,
    ) -> bool {
        action.selected_step().is_some()
            && self
                .actor_locomotion(id)
                .is_some_and(|value| value.stationary)
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod confusion_tests;
