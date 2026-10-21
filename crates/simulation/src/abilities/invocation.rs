//! Closed player/NPC-selected operands. Permissions and destinations are not
//! supplied by clients; ordinary movement supplies the frozen destination.
use super::projectile_selection::ProjectileDeliverySelection;
use crate::ActorId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationKind {
    #[default]
    ActorTarget,
    SelfStep,
    DirectionalRay,
    DirectionalContactRay,
    DirectionalProjectile,
}

impl<'de> Deserialize<'de> for InvocationKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "actor_target" => Ok(Self::ActorTarget),
            "self_step" => Ok(Self::SelfStep),
            "directional_ray" => Ok(Self::DirectionalRay),
            "directional_contact_ray" => Ok(Self::DirectionalContactRay),
            "directional_projectile" => Ok(Self::DirectionalProjectile),
            _ => Err(serde::de::Error::custom("invalid invocation kind")),
        }
    }
}
impl InvocationKind {
    pub const fn is_directional_ray(self) -> bool {
        matches!(self, Self::DirectionalRay | Self::DirectionalContactRay)
    }
    /// A direction requests an aim, never authority over its delivery behavior.
    pub(crate) fn accepts(self, requested: Self) -> bool {
        self == requested || (self.is_directional_ray() && requested == Self::DirectionalRay)
    }
}

/// Aim authority and ordinary equipped-weapon selection are separate operands.
/// Physical projectiles carry their complete delivery selection inside the aim
/// and require the outer weapon selector to remain `Default`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct AbilityInvocation {
    pub aim: AbilityAim,
    pub weapon: WeaponSelection,
}
impl<'de> Deserialize<'de> for AbilityInvocation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            aim: AbilityAim,
            weapon: WeaponSelection,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let result = Self {
            aim: fields.aim,
            weapon: fields.weapon,
        };
        if !result.valid() {
            return Err(serde::de::Error::custom("invalid ability invocation"));
        }
        Ok(result)
    }
}
impl AbilityInvocation {
    pub fn actor_target(target: ActorId) -> Self {
        Self {
            aim: AbilityAim::ActorTarget { target },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn contact_target(target: ActorId) -> Self {
        Self {
            aim: AbilityAim::ContactTarget { target },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn self_step(direction: StepDirection) -> Self {
        Self {
            aim: AbilityAim::SelfStep { direction },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn directional_ray(direction: StepDirection) -> Self {
        Self {
            aim: AbilityAim::DirectionalRay { direction },
            weapon: WeaponSelection::Default,
        }
    }
    pub fn directional_projectile(
        direction: StepDirection,
        delivery: ProjectileDeliverySelection,
    ) -> Self {
        Self {
            aim: AbilityAim::DirectionalProjectile {
                direction,
                delivery,
            },
            weapon: WeaponSelection::Default,
        }
    }
    pub(crate) fn valid(self) -> bool {
        self.weapon.valid()
            && match self.aim {
                AbilityAim::DirectionalProjectile { delivery, .. } => {
                    self.weapon == WeaponSelection::Default && delivery.valid()
                }
                _ => true,
            }
    }
    pub fn target(self, actor: ActorId) -> ActorId {
        self.aim.target(actor)
    }
    pub fn reference(self) -> crate::perception::ActorReferenceKind {
        self.aim.reference()
    }
    pub fn kind(self) -> InvocationKind {
        self.aim.kind()
    }
    pub fn step(self) -> Option<StepDirection> {
        self.aim.step()
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WeaponSelection {
    #[default]
    Default,
    Equipped {
        item: crate::ItemId,
        socket: crate::EquipmentSlotId,
    },
}
impl WeaponSelection {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Default => true,
            Self::Equipped { item, socket } => {
                item.0 != 0
                    && item.0 != u64::MAX
                    && usize::from(socket.0) < crate::limits::EQUIPMENT_SOCKETS
            }
        }
    }
}
impl<'de> Deserialize<'de> for WeaponSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Default {},
            Equipped {
                item: crate::ItemId,
                socket: crate::EquipmentSlotId,
            },
        }
        let result = match crate::bounded::named_map(d)? {
            Fields::Default {} => Self::Default,
            Fields::Equipped { item, socket } => Self::Equipped { item, socket },
        };
        if !result.valid() {
            return Err(serde::de::Error::custom("invalid weapon selector"));
        }
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AbilityAim {
    ActorTarget {
        target: ActorId,
    },
    ContactTarget {
        target: ActorId,
    },
    SelfStep {
        direction: StepDirection,
    },
    DirectionalRay {
        direction: StepDirection,
    },
    DirectionalProjectile {
        direction: StepDirection,
        delivery: ProjectileDeliverySelection,
    },
}
impl<'de> Deserialize<'de> for AbilityAim {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            ActorTarget {
                target: ActorId,
            },
            ContactTarget {
                target: ActorId,
            },
            SelfStep {
                direction: StepDirection,
            },
            DirectionalRay {
                direction: StepDirection,
            },
            DirectionalProjectile {
                direction: StepDirection,
                delivery: ProjectileDeliverySelection,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::ActorTarget { target } => Self::ActorTarget { target },
            Fields::ContactTarget { target } => Self::ContactTarget { target },
            Fields::SelfStep { direction } => Self::SelfStep { direction },
            Fields::DirectionalRay { direction } => Self::DirectionalRay { direction },
            Fields::DirectionalProjectile {
                direction,
                delivery,
            } => Self::DirectionalProjectile {
                direction,
                delivery,
            },
        })
    }
}
impl AbilityAim {
    pub fn target(self, actor: ActorId) -> ActorId {
        match self {
            Self::ActorTarget { target } | Self::ContactTarget { target } => target,
            Self::SelfStep { .. }
            | Self::DirectionalRay { .. }
            | Self::DirectionalProjectile { .. } => actor,
        }
    }
    pub fn reference(self) -> crate::perception::ActorReferenceKind {
        match self {
            Self::ContactTarget { .. } => crate::perception::ActorReferenceKind::Contact,
            _ => crate::perception::ActorReferenceKind::Creature,
        }
    }
    pub fn kind(self) -> InvocationKind {
        match self {
            Self::ActorTarget { .. } | Self::ContactTarget { .. } => InvocationKind::ActorTarget,
            Self::SelfStep { .. } => InvocationKind::SelfStep,
            Self::DirectionalRay { .. } => InvocationKind::DirectionalRay,
            Self::DirectionalProjectile { .. } => InvocationKind::DirectionalProjectile,
        }
    }
    pub fn step(self) -> Option<StepDirection> {
        match self {
            Self::SelfStep { direction } => Some(direction),
            Self::ActorTarget { .. }
            | Self::ContactTarget { .. }
            | Self::DirectionalRay { .. }
            | Self::DirectionalProjectile { .. } => None,
        }
    }
}

/// Ordinary selected directions, distinct from 26-vector physical push contact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepDirection {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Up,
    Down,
}
impl<'de> Deserialize<'de> for StepDirection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let token = String::deserialize(deserializer)?;
        match token.as_str() {
            "north" => Ok(Self::North),
            "east" => Ok(Self::East),
            "south" => Ok(Self::South),
            "west" => Ok(Self::West),
            "north_east" => Ok(Self::NorthEast),
            "south_east" => Ok(Self::SouthEast),
            "south_west" => Ok(Self::SouthWest),
            "north_west" => Ok(Self::NorthWest),
            "up" => Ok(Self::Up),
            "down" => Ok(Self::Down),
            _ => Err(serde::de::Error::unknown_variant(
                &token,
                &[
                    "north",
                    "east",
                    "south",
                    "west",
                    "north_east",
                    "south_east",
                    "south_west",
                    "north_west",
                    "up",
                    "down",
                ],
            )),
        }
    }
}
impl From<StepDirection> for tor_world::Direction {
    fn from(direction: StepDirection) -> Self {
        match direction {
            StepDirection::North => Self::North,
            StepDirection::East => Self::East,
            StepDirection::South => Self::South,
            StepDirection::West => Self::West,
            StepDirection::NorthEast => Self::NorthEast,
            StepDirection::SouthEast => Self::SouthEast,
            StepDirection::SouthWest => Self::SouthWest,
            StepDirection::NorthWest => Self::NorthWest,
            StepDirection::Up => Self::Up,
            StepDirection::Down => Self::Down,
        }
    }
}

/// The operation is frozen; its contact vector is captured at the due primary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PushDirection {
    AwayFromSource,
}

impl crate::Action {
    pub(crate) fn selected_step(&self) -> Option<tor_world::Direction> {
        match self {
            Self::Move(direction) => Some(*direction),
            Self::UseAbility { invocation, .. } => invocation.step().map(Into::into),
            _ => None,
        }
    }
}

impl crate::rules::Effect {
    pub(crate) fn spatial(&self) -> bool {
        matches!(
            self,
            Self::StepSelfOneCell
                | Self::PushTargetOneCell { .. }
                | Self::TeleportRandom
                | Self::TeleportToRayOrigin
        )
    }
}
impl super::PlannedEffect {
    pub(crate) fn spatial(&self) -> bool {
        matches!(
            self,
            Self::StepSelfOneCell
                | Self::PushTargetOneCell { .. }
                | Self::TeleportRandom
                | Self::TeleportToRayOrigin
        )
    }
}
impl crate::rules::AbilityDef {
    pub(crate) fn spatial_shape_valid(&self) -> bool {
        use crate::rules::{Effect, Reach, StrikeSource};
        if self.invocation == InvocationKind::DirectionalProjectile
            || matches!(self.reach, Reach::Projectile)
            || matches!(self.strikes, StrikeSource::Projectile { .. })
        {
            return super::projectile::definition_shape_valid(self);
        }
        if let StrikeSource::NaturalMelee { additional_riders } = &self.strikes {
            return self.invocation == InvocationKind::ActorTarget
                && matches!(self.reach, Reach::Melee)
                && additional_riders.iter().all(|rider| {
                    rider
                        .effects
                        .iter()
                        .enumerate()
                        .all(|(index, effect)| match effect {
                            Effect::StepSelfOneCell | Effect::TeleportToRayOrigin => false,
                            Effect::PushTargetOneCell { .. } => index == 0,
                            _ => true,
                        })
                });
        }
        let StrikeSource::Authored { strikes } = &self.strikes else {
            return self.invocation == InvocationKind::ActorTarget;
        };
        if self.invocation.is_directional_ray() {
            return super::ray::definition_shape_valid(self);
        }
        if self.invocation == InvocationKind::SelfStep {
            return matches!(self.reach, Reach::SelfOnly)
                && strikes.len() == 1
                && strikes[0].offset == 0
                && strikes[0].unlock.is_none()
                && strikes[0].check.is_none()
                && strikes[0].riders.is_empty()
                && strikes[0].on_failure.is_empty()
                && matches!(strikes[0].on_success.first(), Some(Effect::StepSelfOneCell))
                && strikes[0]
                    .on_success
                    .iter()
                    .skip(1)
                    .all(|effect| !effect.spatial());
        }
        strikes.iter().all(|strike| {
            [&strike.on_success, &strike.on_failure]
                .into_iter()
                .chain(strike.riders.iter().map(|rider| &rider.effects))
                .all(|effects| {
                    effects
                        .iter()
                        .enumerate()
                        .all(|(index, effect)| match effect {
                            Effect::StepSelfOneCell | Effect::TeleportToRayOrigin => false,
                            Effect::PushTargetOneCell { .. } => {
                                index == 0 && matches!(self.reach, Reach::Melee)
                            }
                            _ => true,
                        })
                })
        })
    }
}

impl TryFrom<tor_world::Direction> for StepDirection {
    type Error = ();
    fn try_from(direction: tor_world::Direction) -> Result<Self, Self::Error> {
        Ok(match direction {
            tor_world::Direction::North => Self::North,
            tor_world::Direction::East => Self::East,
            tor_world::Direction::South => Self::South,
            tor_world::Direction::West => Self::West,
            tor_world::Direction::NorthEast => Self::NorthEast,
            tor_world::Direction::SouthEast => Self::SouthEast,
            tor_world::Direction::SouthWest => Self::SouthWest,
            tor_world::Direction::NorthWest => Self::NorthWest,
            tor_world::Direction::Up => Self::Up,
            tor_world::Direction::Down => Self::Down,
            tor_world::Direction::EastUp
            | tor_world::Direction::WestUp
            | tor_world::Direction::NorthUp
            | tor_world::Direction::SouthUp
            | tor_world::Direction::EastDown
            | tor_world::Direction::WestDown
            | tor_world::Direction::NorthDown
            | tor_world::Direction::SouthDown => return Err(()),
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn invocation_kind_requires_exact_string_vocabulary() {
        for name in [
            "actor_target",
            "self_step",
            "directional_ray",
            "directional_contact_ray",
            "directional_projectile",
        ] {
            let value = serde_json::json!(name);
            let decoded: super::InvocationKind = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
            for invalid in [
                serde_json::json!({(name): null}),
                serde_json::json!([name]),
                serde_json::json!(null),
            ] {
                assert!(serde_json::from_value::<super::InvocationKind>(invalid).is_err());
            }
        }
    }

    use super::{AbilityInvocation, StepDirection};

    #[test]
    fn frozen_selected_directions_round_trip_only_ten_string_tokens() {
        for token in [
            "north",
            "east",
            "south",
            "west",
            "north_east",
            "south_east",
            "south_west",
            "north_west",
            "up",
            "down",
        ] {
            let raw = serde_json::to_string(token).unwrap();
            let direction: StepDirection = serde_json::from_str(&raw).unwrap();
            assert_eq!(serde_json::to_string(&direction).unwrap(), raw);
            let invocation = format!(
                r#"{{"aim":{{"kind":"self_step","direction":{raw}}},"weapon":{{"kind":"default"}}}}"#
            );
            let decoded: AbilityInvocation = serde_json::from_str(&invocation).unwrap();
            assert_eq!(decoded, AbilityInvocation::self_step(direction));
            assert_eq!(serde_json::to_string(&decoded).unwrap(), invocation);
        }
        for raw in [
            r#"{"north":null}"#,
            r#"["north"]"#,
            "null",
            "0",
            r#""north_up""#,
            r#""North""#,
        ] {
            assert!(serde_json::from_str::<StepDirection>(raw).is_err(), "{raw}");
            let invocation = format!(
                r#"{{"aim":{{"kind":"self_step","direction":{raw}}},"weapon":{{"kind":"default"}}}}"#
            );
            assert!(
                serde_json::from_str::<AbilityInvocation>(&invocation).is_err(),
                "{invocation}"
            );
        }
    }
    #[test]
    fn directional_projectile_request_round_trips_exact_delivery_operands() {
        for delivery in [
            serde_json::json!({"kind": "throw", "source": {"kind": "equipped", "item": 1, "socket": 0}}),
            serde_json::json!({"kind": "throw", "source": {"kind": "carried", "item": 2, "socket": 1}}),
            serde_json::json!({"kind": "launch", "launcher": {"item": 3, "socket": 2}, "ammunition": 4}),
        ] {
            let encoded = serde_json::json!({
                "aim": {"kind": "directional_projectile", "direction": "north", "delivery": delivery},
                "weapon": {"kind": "default"}
            });
            let decoded: AbilityInvocation = serde_json::from_value(encoded.clone())
                .expect("a complete exact projectile selection must be accepted");
            assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
            assert_eq!(decoded.target(crate::ActorId(7)), crate::ActorId(7));
            assert_eq!(decoded.step(), None);
        }
    }
}
