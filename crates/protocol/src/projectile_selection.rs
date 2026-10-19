//! Exact owner-scoped physical projectile operands. Tokens confer no authority.
use crate::ItemTarget;
use serde::{Deserialize, Serialize};

fn valid_socket(socket: u16) -> bool {
    usize::from(socket) < crate::MAX_EQUIPMENT_SLOTS
}

/// The selected equipped launcher, with no implicit primary-weapon fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct EquippedProjectileWeapon {
    pub item: ItemTarget,
    pub socket: u16,
}
impl EquippedProjectileWeapon {
    pub(crate) fn valid(self) -> bool {
        valid_socket(self.socket)
    }
}
impl<'de> Deserialize<'de> for EquippedProjectileWeapon {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemTarget,
            socket: u16,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let result = Self {
            item: fields.item,
            socket: fields.socket,
        };
        if !result.valid() {
            return Err(serde::de::Error::custom(
                "invalid projectile weapon selector",
            ));
        }
        Ok(result)
    }
}

/// Throw an equipped item or a carried stack using the selected socket.
/// The socket remains explicit for either source mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThrownSourceSelection {
    Equipped { item: ItemTarget, socket: u16 },
    Carried { item: ItemTarget, socket: u16 },
}
impl ThrownSourceSelection {
    pub(crate) fn valid(self) -> bool {
        let (Self::Equipped { socket, .. } | Self::Carried { socket, .. }) = self;
        valid_socket(socket)
    }
}
impl<'de> Deserialize<'de> for ThrownSourceSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Equipped { item: ItemTarget, socket: u16 },
            Carried { item: ItemTarget, socket: u16 },
        }
        let result = match crate::bounded::named_map(d)? {
            Fields::Equipped { item, socket } => Self::Equipped { item, socket },
            Fields::Carried { item, socket } => Self::Carried { item, socket },
        };
        if !result.valid() {
            return Err(serde::de::Error::custom("invalid thrown source selector"));
        }
        Ok(result)
    }
}

/// A closed delivery choice for exactly one projectile. Launch always names
/// both the equipped launcher and the carried ammunition stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProjectileDeliverySelection {
    Throw {
        source: ThrownSourceSelection,
    },
    Launch {
        launcher: EquippedProjectileWeapon,
        ammunition: ItemTarget,
    },
}
impl ProjectileDeliverySelection {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Throw { source } => source.valid(),
            Self::Launch {
                launcher,
                ammunition,
            } => launcher.valid() && launcher.item != ammunition,
        }
    }
}
impl<'de> Deserialize<'de> for ProjectileDeliverySelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Throw {
                source: ThrownSourceSelection,
            },
            Launch {
                launcher: EquippedProjectileWeapon,
                ammunition: ItemTarget,
            },
        }
        let result = match crate::bounded::named_map(d)? {
            Fields::Throw { source } => Self::Throw { source },
            Fields::Launch {
                launcher,
                ammunition,
            } => Self::Launch {
                launcher,
                ammunition,
            },
        };
        if !result.valid() {
            return Err(serde::de::Error::custom(
                "invalid projectile delivery selector",
            ));
        }
        Ok(result)
    }
}
