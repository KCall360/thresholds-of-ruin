//! Exact, bounded source operands for one physical projectile invocation.
//! Selection does not confer ownership, occupancy, ammunition compatibility or
//! permission; those are checked against authoritative state when planning.
use crate::{EquipmentSlotId, ItemId};
use serde::{Deserialize, Serialize};

fn valid_item(item: ItemId) -> bool {
    item.0 != 0 && item.0 != u64::MAX
}
fn valid_socket(socket: EquipmentSlotId) -> bool {
    usize::from(socket.0) < crate::limits::EQUIPMENT_SOCKETS
}

/// The selected equipped launcher, with no implicit primary-weapon fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct EquippedProjectileWeapon {
    pub item: ItemId,
    pub socket: EquipmentSlotId,
}
impl EquippedProjectileWeapon {
    pub(crate) fn valid(self) -> bool {
        valid_item(self.item) && valid_socket(self.socket)
    }
}
impl<'de> Deserialize<'de> for EquippedProjectileWeapon {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: ItemId,
            socket: EquipmentSlotId,
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
    Equipped {
        item: ItemId,
        socket: EquipmentSlotId,
    },
    Carried {
        item: ItemId,
        socket: EquipmentSlotId,
    },
}
impl ThrownSourceSelection {
    pub(crate) fn valid(self) -> bool {
        let (Self::Equipped { item, socket } | Self::Carried { item, socket }) = self;
        valid_item(item) && valid_socket(socket)
    }
}
impl<'de> Deserialize<'de> for ThrownSourceSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Equipped {
                item: ItemId,
                socket: EquipmentSlotId,
            },
            Carried {
                item: ItemId,
                socket: EquipmentSlotId,
            },
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
        ammunition: ItemId,
    },
}
impl ProjectileDeliverySelection {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Throw { source } => source.valid(),
            Self::Launch {
                launcher,
                ammunition,
            } => launcher.valid() && valid_item(ammunition) && launcher.item != ammunition,
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
                ammunition: ItemId,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abilities::{
        AbilityAim, AbilityInvocation, InvocationKind, StepDirection, WeaponSelection,
    };
    use crate::ActorId;
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    fn deliveries() -> [ProjectileDeliverySelection; 3] {
        [
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Equipped {
                    item: ItemId(1),
                    socket: EquipmentSlotId(0),
                },
            },
            ProjectileDeliverySelection::Throw {
                source: ThrownSourceSelection::Carried {
                    item: ItemId(2),
                    socket: EquipmentSlotId(1),
                },
            },
            ProjectileDeliverySelection::Launch {
                launcher: EquippedProjectileWeapon {
                    item: ItemId(3),
                    socket: EquipmentSlotId(2),
                },
                ammunition: ItemId(4),
            },
        ]
    }

    /// Exercise the owning decoder directly, including duplicate map keys
    /// which a serde_json::Value fixture would otherwise discard.
    fn assert_strict_named<T: DeserializeOwned>(valid: Value) {
        assert!(serde_json::from_value::<T>(valid.clone()).is_ok());
        let fields = valid.as_object().unwrap();
        let encoded = serde_json::to_string(&valid).unwrap();
        for (key, value) in fields {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(
                serde_json::from_value::<T>(missing.clone()).is_err(),
                "{missing}"
            );
            let duplicate = format!(
                "{{{}:{},{}",
                serde_json::to_string(key).unwrap(),
                value,
                &encoded[1..]
            );
            assert!(
                serde_json::from_str::<T>(&duplicate).is_err(),
                "{duplicate}"
            );
        }
        let mut extra = valid.clone();
        extra["unknown"] = json!(0);
        for invalid in [
            extra,
            json!(fields.values().collect::<Vec<_>>()),
            Value::Null,
            json!(0),
            json!("default"),
        ] {
            assert!(
                serde_json::from_value::<T>(invalid.clone()).is_err(),
                "{invalid}"
            );
        }
    }

    #[test]
    fn projectile_launch_requires_distinct_launcher_and_payload_identity() {
        let invalid = ProjectileDeliverySelection::Launch {
            launcher: EquippedProjectileWeapon {
                item: ItemId(3),
                socket: EquipmentSlotId(0),
            },
            ammunition: ItemId(3),
        };
        assert!(!invalid.valid());
        assert!(serde_json::from_value::<ProjectileDeliverySelection>(
            serde_json::to_value(invalid).unwrap()
        )
        .is_err());
    }

    #[test]
    fn projectile_selectors_round_trip_without_target_or_weapon_fallback() {
        for delivery in deliveries() {
            for (token, direction) in [
                ("north", StepDirection::North),
                ("east", StepDirection::East),
                ("south", StepDirection::South),
                ("west", StepDirection::West),
                ("north_east", StepDirection::NorthEast),
                ("south_east", StepDirection::SouthEast),
                ("south_west", StepDirection::SouthWest),
                ("north_west", StepDirection::NorthWest),
                ("up", StepDirection::Up),
                ("down", StepDirection::Down),
            ] {
                let invocation = AbilityInvocation::directional_projectile(direction, delivery);
                let encoded = json!({
                    "aim": {"kind": "directional_projectile", "direction": token, "delivery": delivery},
                    "weapon": {"kind": "default"}
                });
                assert_eq!(serde_json::to_value(invocation).unwrap(), encoded);
                assert_eq!(
                    serde_json::from_value::<AbilityInvocation>(encoded).unwrap(),
                    invocation
                );
                assert!(invocation.valid());
                assert_eq!(invocation.target(ActorId(7)), ActorId(7));
                assert_eq!(
                    invocation.reference(),
                    crate::perception::ActorReferenceKind::Creature
                );
                assert_eq!(invocation.kind(), InvocationKind::DirectionalProjectile);
                assert_eq!(invocation.step(), None);
                assert_ne!(
                    invocation.kind(),
                    AbilityInvocation::directional_ray(direction).kind()
                );
            }
        }
        assert_eq!(
            serde_json::to_value(InvocationKind::DirectionalProjectile).unwrap(),
            json!("directional_projectile")
        );
        assert_eq!(
            serde_json::from_value::<InvocationKind>(json!("directional_projectile")).unwrap(),
            InvocationKind::DirectionalProjectile
        );
    }

    #[test]
    fn projectile_selection_records_require_complete_closed_named_fields() {
        assert_strict_named::<EquippedProjectileWeapon>(json!({"item": 1, "socket": 0}));
        for kind in ["equipped", "carried"] {
            assert_strict_named::<ThrownSourceSelection>(
                json!({"kind": kind, "item": 1, "socket": 0}),
            );
        }
        for delivery in deliveries() {
            assert_strict_named::<ProjectileDeliverySelection>(
                serde_json::to_value(delivery).unwrap(),
            );
            let invocation =
                AbilityInvocation::directional_projectile(StepDirection::North, delivery);
            assert_strict_named::<AbilityAim>(serde_json::to_value(invocation.aim).unwrap());
            assert_strict_named::<AbilityInvocation>(serde_json::to_value(invocation).unwrap());
        }
    }

    #[test]
    fn projectile_selection_rejects_unbounded_items_and_sockets() {
        for item in [0, u64::MAX] {
            assert!(!EquippedProjectileWeapon {
                item: ItemId(item),
                socket: EquipmentSlotId(0)
            }
            .valid());
            assert!(serde_json::from_value::<EquippedProjectileWeapon>(
                json!({"item": item, "socket": 0})
            )
            .is_err());
            for kind in ["equipped", "carried"] {
                assert!(serde_json::from_value::<ThrownSourceSelection>(
                    json!({"kind": kind, "item": item, "socket": 0})
                )
                .is_err());
            }
            let launch = ProjectileDeliverySelection::Launch {
                launcher: EquippedProjectileWeapon {
                    item: ItemId(1),
                    socket: EquipmentSlotId(0),
                },
                ammunition: ItemId(item),
            };
            assert!(!launch.valid());
            let invocation =
                AbilityInvocation::directional_projectile(StepDirection::North, launch);
            assert!(!invocation.valid());
            assert!(serde_json::from_value::<AbilityInvocation>(
                serde_json::to_value(invocation).unwrap()
            )
            .is_err());
        }
        for socket in [crate::limits::EQUIPMENT_SOCKETS as u16, u16::MAX] {
            assert!(!EquippedProjectileWeapon {
                item: ItemId(1),
                socket: EquipmentSlotId(socket)
            }
            .valid());
            assert!(serde_json::from_value::<EquippedProjectileWeapon>(
                json!({"item": 1, "socket": socket})
            )
            .is_err());
            for kind in ["equipped", "carried"] {
                assert!(serde_json::from_value::<ThrownSourceSelection>(
                    json!({"kind": kind, "item": 1, "socket": socket})
                )
                .is_err());
            }
        }
        let last_socket = (crate::limits::EQUIPMENT_SOCKETS - 1) as u16;
        let launcher: EquippedProjectileWeapon =
            serde_json::from_value(json!({"item": u64::MAX - 1, "socket": last_socket})).unwrap();
        assert!(launcher.valid());
        for kind in ["equipped", "carried"] {
            let source: ThrownSourceSelection = serde_json::from_value(
                json!({"kind": kind, "item": u64::MAX - 1, "socket": last_socket}),
            )
            .unwrap();
            assert!(source.valid());
        }
        let delivery = ProjectileDeliverySelection::Launch {
            launcher,
            ammunition: ItemId(u64::MAX - 2),
        };
        assert!(delivery.valid());
        assert!(serde_json::from_value::<ProjectileDeliverySelection>(
            serde_json::to_value(delivery).unwrap()
        )
        .is_ok());
    }

    #[test]
    fn projectile_invocation_rejects_competing_weapon_and_payload_shapes() {
        for delivery in deliveries() {
            let mut invocation =
                AbilityInvocation::directional_projectile(StepDirection::North, delivery);
            invocation.weapon = WeaponSelection::Equipped {
                item: ItemId(1),
                socket: EquipmentSlotId(0),
            };
            assert!(!invocation.valid());
            assert!(serde_json::from_value::<AbilityInvocation>(
                serde_json::to_value(invocation).unwrap()
            )
            .is_err());
        }
        for delivery in [
            json!({"kind": "default"}),
            json!({"kind": "throw", "source": {"kind": "default"}}),
            json!({"kind": "throw", "source": {"kind": "equipped", "item": 1, "socket": 0}, "ammunition": 2}),
            json!({"kind": "launch", "launcher": [1, 0], "ammunition": 2}),
            json!({"kind": "launch", "launcher": {"kind": "equipped", "item": 1, "socket": 0}, "ammunition": 2}),
            json!({"kind": "launch", "launcher": {"item": 1, "socket": 0}, "ammunition": 2, "source": {"kind": "carried", "item": 3, "socket": 0}}),
        ] {
            assert!(
                serde_json::from_value::<ProjectileDeliverySelection>(delivery.clone()).is_err(),
                "{delivery}"
            );
            let invocation = json!({"aim": {"kind": "directional_projectile", "direction": "north", "delivery": delivery}, "weapon": {"kind": "default"}});
            assert!(serde_json::from_value::<AbilityInvocation>(invocation).is_err());
        }
        let valid = serde_json::to_value(AbilityInvocation::directional_projectile(
            StepDirection::North,
            deliveries()[0],
        ))
        .unwrap();
        for field in ["target", "source", "launcher", "ammunition"] {
            let mut invalid = valid.clone();
            invalid["aim"][field] = json!(1);
            assert!(serde_json::from_value::<AbilityInvocation>(invalid).is_err());
        }
        for direction in [
            json!("north_up"),
            json!({"north": null}),
            json!(["north"]),
            Value::Null,
        ] {
            let mut invalid = valid.clone();
            invalid["aim"]["direction"] = direction;
            assert!(serde_json::from_value::<AbilityInvocation>(invalid).is_err());
        }
    }
}
