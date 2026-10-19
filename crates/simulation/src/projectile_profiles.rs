//! Physical ammunition payloads. Launchers own their checks, timing and range;
//! projectiles supply only compatible family, base damage and hit recovery.
use crate::bounded::{named_map, required_option, Bounded};
use crate::combat::DamageType;
use crate::content_ids::{DescriptorId, LauncherFamilyId};
use crate::damage::{DamageAmount, DamageComponent, DamageKey, DamageSpec};
use crate::dice::DicePool;
use crate::weapon_profiles::LauncherProfile;
use serde::{Deserialize, Serialize};

/// Authored physical delivery semantics; a definition emits exactly one unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectileDeliveryKind {
    Throw,
    Launch,
}

/// Deterministic recovery on a successful hit. Miss handling belongs to flight
/// resolution; this definition does not introduce a random breakage roll.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HitRecovery {
    ConsumedOnHit,
    Recoverable,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProjectileSpec {
    pub family: LauncherFamilyId,
    pub damage: DamageSpec,
    pub recovery: HitRecovery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectileProfileError {
    MissingPrimary,
    InvalidDamage,
}
impl std::fmt::Display for ProjectileProfileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MissingPrimary => "projectile damage requires a primary component",
            Self::InvalidDamage => "invalid projectile damage payload",
        })
    }
}
impl std::error::Error for ProjectileProfileError {}

impl ProjectileSpec {
    pub fn validate_shape(&self) -> Result<(), ProjectileProfileError> {
        let primary = self
            .damage
            .primary()
            .ok_or(ProjectileProfileError::MissingPrimary)?;
        DamageSpec::new(self.damage.components().to_vec(), Some(primary))
            .map_err(|_| ProjectileProfileError::InvalidDamage)?;
        Ok(())
    }

    /// Compatibility never falls back to a broad weapon class or expertise.
    pub fn compatible_with(&self, launcher: &LauncherProfile) -> bool {
        self.family == launcher.family
    }
}

// Keep the shared damage record's shape, but require named records at every
// projectile definition boundary. All legality and canonicalization still run
// through DamageSpec::new and DicePool::new, never parallel damage math.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    components: Bounded<Component, { crate::limits::DAMAGE_COMPONENTS }>,
    #[serde(deserialize_with = "named_map")]
    primary: Primary,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Primary {
    category: DamageType,
    #[serde(deserialize_with = "required_option")]
    descriptor: Option<DescriptorId>,
    #[serde(deserialize_with = "required_option")]
    sides: Option<u16>,
}
#[derive(Serialize)]
struct Component {
    category: DamageType,
    descriptor: Option<DescriptorId>,
    amount: Amount,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Amount {
    Fixed { value: u32 },
    Rolled { count: u16, sides: u16, bonus: i32 },
}
impl<'de> Deserialize<'de> for Component {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            category: DamageType,
            #[serde(deserialize_with = "required_option")]
            descriptor: Option<DescriptorId>,
            #[serde(deserialize_with = "named_map")]
            amount: Amount,
        }
        let fields: Fields = named_map(d)?;
        Ok(Self {
            category: fields.category,
            descriptor: fields.descriptor,
            amount: fields.amount,
        })
    }
}
impl Serialize for ProjectileSpec {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.validate_shape().map_err(serde::ser::Error::custom)?;
        #[derive(Serialize)]
        struct Fields<'a> {
            family: &'a LauncherFamilyId,
            damage: Payload,
            recovery: HitRecovery,
        }
        let key = self
            .damage
            .primary()
            .ok_or_else(|| serde::ser::Error::custom(ProjectileProfileError::MissingPrimary))?;
        Fields {
            family: &self.family,
            recovery: self.recovery,
            damage: Payload {
                primary: Primary {
                    category: key.category,
                    descriptor: key.descriptor,
                    sides: key.sides,
                },
                components: Bounded(
                    self.damage
                        .components()
                        .iter()
                        .map(|component| {
                            let key = component.key();
                            Component {
                                category: key.category,
                                descriptor: key.descriptor,
                                amount: match component.amount() {
                                    DamageAmount::Fixed(value) => Amount::Fixed { value },
                                    DamageAmount::Rolled(pool) => Amount::Rolled {
                                        count: pool.count(),
                                        sides: pool.sides(),
                                        bonus: pool.bonus(),
                                    },
                                },
                            }
                        })
                        .collect(),
                ),
            },
        }
        .serialize(s)
    }
}
impl<'de> Deserialize<'de> for ProjectileSpec {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            family: LauncherFamilyId,
            #[serde(deserialize_with = "named_map")]
            damage: Payload,
            recovery: HitRecovery,
        }
        let fields: Fields = named_map(d)?;
        let components = fields
            .damage
            .components
            .0
            .into_iter()
            .map(|component| {
                Ok(match component.amount {
                    Amount::Fixed { value } => {
                        DamageComponent::fixed(component.category, component.descriptor, value)
                    }
                    Amount::Rolled {
                        count,
                        sides,
                        bonus,
                    } => DamageComponent::rolled(
                        component.category,
                        component.descriptor,
                        DicePool::new(count, sides, bonus).map_err(|_| {
                            <D::Error as serde::de::Error>::custom(
                                ProjectileProfileError::InvalidDamage,
                            )
                        })?,
                    ),
                })
            })
            .collect::<Result<Vec<_>, D::Error>>()?;
        let primary = fields.damage.primary;
        let damage = DamageSpec::new(
            components,
            Some(DamageKey {
                category: primary.category,
                descriptor: primary.descriptor,
                sides: primary.sides,
            }),
        )
        .map_err(|_| {
            <D::Error as serde::de::Error>::custom(ProjectileProfileError::InvalidDamage)
        })?;
        let result = Self {
            family: fields.family,
            damage,
            recovery: fields.recovery,
        };
        result.validate_shape().map_err(serde::de::Error::custom)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::weapon_profiles::RangeBands;
    use serde_json::json;

    fn projectile() -> ProjectileSpec {
        let component =
            DamageComponent::rolled(DamageType::Keen, None, DicePool::new(1, 6, 0).unwrap());
        let primary = component.key();
        ProjectileSpec {
            family: LauncherFamilyId::new("fixture.bow").unwrap(),
            damage: DamageSpec::new(vec![component], Some(primary)).unwrap(),
            recovery: HitRecovery::ConsumedOnHit,
        }
    }

    #[test]
    fn exact_family_compatibility_and_deterministic_recovery_round_trip() {
        let mut source = projectile();
        let mut launcher = LauncherProfile {
            family: source.family.clone(),
            range: RangeBands {
                short_feet: 20,
                long_feet: 60,
            },
        };
        assert!(source.compatible_with(&launcher));
        launcher.family = LauncherFamilyId::new("fixture.crossbow").unwrap();
        assert!(!source.compatible_with(&launcher));
        for recovery in [HitRecovery::ConsumedOnHit, HitRecovery::Recoverable] {
            source.recovery = recovery;
            let value = serde_json::to_value(&source).unwrap();
            assert_eq!(
                value["recovery"],
                match recovery {
                    HitRecovery::ConsumedOnHit => "consumed_on_hit",
                    HitRecovery::Recoverable => "recoverable",
                }
            );
            assert_eq!(
                serde_json::from_value::<ProjectileSpec>(value).unwrap(),
                source
            );
        }
    }

    #[test]
    fn payload_requires_primary_and_preserves_shared_damage_canonicalization() {
        let mut source = projectile();
        source.damage = DamageSpec::new(source.damage.components().to_vec(), None).unwrap();
        assert_eq!(
            source.validate_shape(),
            Err(ProjectileProfileError::MissingPrimary)
        );
        assert!(serde_json::to_value(&source).is_err());
        let mut value = serde_json::to_value(projectile()).unwrap();
        let component = value["damage"]["components"][0].clone();
        value["damage"]["components"]
            .as_array_mut()
            .unwrap()
            .push(component);
        let decoded = serde_json::from_value::<ProjectileSpec>(value).unwrap();
        assert_eq!(decoded.damage.components().len(), 1);
        assert_eq!(
            decoded.damage.components()[0].amount(),
            DamageAmount::Rolled(DicePool::new(2, 6, 0).unwrap())
        );
        assert!(decoded
            .damage
            .with_melee_modifiers(0, 1_000_001, 0)
            .is_err());
    }

    #[test]
    fn fixed_and_mixed_payloads_use_shared_bounds() {
        let fixed = DamageComponent::fixed(DamageType::Impact, None, 7);
        let primary = fixed.key();
        let mut source = projectile();
        source.damage = DamageSpec::new(
            vec![fixed, source.damage.components()[0].clone()],
            Some(primary),
        )
        .unwrap();
        let value = serde_json::to_value(&source).unwrap();
        assert_eq!(
            serde_json::from_value::<ProjectileSpec>(value).unwrap(),
            source
        );
        let mut value = serde_json::to_value(projectile()).unwrap();
        value["damage"]["components"][0]["amount"] = json!({"type":"fixed","value":1_000_001});
        value["damage"]["primary"]["sides"] = serde_json::Value::Null;
        assert!(serde_json::from_value::<ProjectileSpec>(value).is_err());
    }

    #[test]
    fn codec_requires_bounded_named_fields_and_rejects_unknowns() {
        let canonical = serde_json::to_value(projectile()).unwrap();
        let mut bad = vec![json!([
            "fixture.bow",
            canonical["damage"],
            "consumed_on_hit"
        ])];
        for key in ["family", "damage", "recovery"] {
            let mut value = canonical.clone();
            value.as_object_mut().unwrap().remove(key);
            bad.push(value);
        }
        for (pointer, replacement) in [
            ("/family", json!("Bad Family")),
            ("/family", json!("x".repeat(crate::limits::ID_BYTES + 1))),
            ("/recovery", json!("random_breakage")),
            (
                "/damage",
                json!([
                    canonical["damage"]["components"],
                    canonical["damage"]["primary"]
                ]),
            ),
            ("/damage/primary", serde_json::Value::Null),
            ("/damage/primary", json!(["keen", null, 6])),
            ("/damage/primary/sides", json!(8)),
            ("/damage/components", json!([])),
            (
                "/damage/components",
                json!(vec![
                    canonical["damage"]["components"][0].clone();
                    crate::limits::DAMAGE_COMPONENTS + 1
                ]),
            ),
            (
                "/damage/components/0",
                json!(["keen", null, {"type":"fixed","value":1}]),
            ),
            ("/damage/components/0/amount", json!(["rolled", 1, 6, 0])),
            ("/damage/components/0/amount/count", json!(0)),
            ("/damage/components/0/amount/bonus", json!(1_000_001)),
        ] {
            let mut value = canonical.clone();
            *value.pointer_mut(pointer).unwrap() = replacement;
            bad.push(value);
        }
        for pointer in [
            "",
            "/damage",
            "/damage/primary",
            "/damage/components/0",
            "/damage/components/0/amount",
        ] {
            let mut value = canonical.clone();
            value
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), json!(true));
            bad.push(value);
        }
        for (pointer, key) in [
            ("/damage", "primary"),
            ("/damage", "components"),
            ("/damage/primary", "descriptor"),
            ("/damage/primary", "sides"),
            ("/damage/components/0", "descriptor"),
        ] {
            let mut value = canonical.clone();
            value
                .pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            bad.push(value);
        }
        for value in bad {
            assert!(
                serde_json::from_value::<ProjectileSpec>(value.clone()).is_err(),
                "accepted {value}"
            );
        }
        let encoded = serde_json::to_string(&projectile()).unwrap();
        let duplicate =
            encoded.replacen("\"family\":", "\"family\":\"fixture.bow\",\"family\":", 1);
        assert!(serde_json::from_str::<ProjectileSpec>(&duplicate).is_err());
    }
}
