//! Source-only rule syntax. Only ability strike payloads have authored defaults;
//! compiled rules and mutable state keep their independent strict decoders.
use crate::creature_authoring::bounded_strike_vec as bounded_vec;
use serde::Deserialize;
use tor_simulation::{content_ids::*, rules::*};

pub(super) struct Document(pub(super) RulesetDocument);
struct Ability(AbilityDef);

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            id: RulesetId,
            constants: Constants,
            point_pools: Vec<PointPoolDef>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
            )]
            supplies: Vec<SupplyDef>,
            abilities: Vec<Ability>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
            )]
            forms: Vec<FormDef>,
            ability_tags: Vec<AbilityTagDef>,
            talents: Vec<TalentDef>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::CATALOG_ENTRIES }>"
            )]
            expertises: Vec<tor_simulation::expertise::ExpertiseDef>,
            creature_types: Vec<TypeDef>,
            subtypes: Vec<SubtypeDef>,
            classes: Vec<ClassDef>,
            conditions: Vec<ConditionDef>,
            descriptors: Vec<NamedDescriptor>,
            flags: Vec<FlagDef>,
            resources: Vec<ResourceDef>,
            health_recovery: HealthRecoveryDef,
            nutrition: tor_simulation::nutrition::NutritionPolicy,
            nutrition_condition_trigger: tor_simulation::nutrition::NutritionConditionTrigger,
            advancement: AdvancementPolicy,
            growth: GrowthPolicy,
            equipment_slots: Vec<EquipmentSlotDef>,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self(RulesetDocument {
            id: fields.id,
            constants: fields.constants,
            point_pools: fields.point_pools,
            supplies: fields.supplies,
            abilities: fields
                .abilities
                .into_iter()
                .map(|ability| ability.0)
                .collect(),
            forms: fields.forms,
            ability_tags: fields.ability_tags,
            talents: fields.talents,
            expertises: fields.expertises,
            creature_types: fields.creature_types,
            subtypes: fields.subtypes,
            classes: fields.classes,
            conditions: fields.conditions,
            descriptors: fields.descriptors,
            flags: fields.flags,
            resources: fields.resources,
            health_recovery: fields.health_recovery,
            nutrition: fields.nutrition,
            nutrition_condition_trigger: fields.nutrition_condition_trigger,
            advancement: fields.advancement,
            growth: fields.growth,
            equipment_slots: fields.equipment_slots,
        }))
    }
}

impl<'de> Deserialize<'de> for Ability {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            interaction: Interaction,
            id: AbilityId,
            #[serde(default)]
            invocation: tor_simulation::abilities::InvocationKind,
            tags: Vec<AbilityTagId>,
            name: String,
            basic_attack: bool,
            #[serde(default)]
            outgoing_edge_policy: OutgoingEdgePolicy,
            cost: Option<Cost>,
            #[serde(default)]
            cooldown: Option<std::num::NonZeroU64>,
            #[serde(
                default,
                deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::SUPPLY_COSTS }>"
            )]
            supply_costs: Vec<SupplyCost>,
            timing: Timing,
            reach: Reach,
            #[serde(deserialize_with = "authored_strikes")]
            strikes: StrikeSource,
            ai: AiHints,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self(AbilityDef {
            interaction: fields.interaction,
            id: fields.id,
            invocation: fields.invocation,
            tags: fields.tags,
            name: fields.name,
            basic_attack: fields.basic_attack,
            outgoing_edge_policy: fields.outgoing_edge_policy,
            cost: fields.cost,
            cooldown: fields.cooldown,
            supply_costs: fields.supply_costs,
            timing: fields.timing,
            reach: fields.reach,
            strikes: fields.strikes,
            ai: fields.ai,
        }))
    }
}

fn authored_strikes<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<StrikeSource, D::Error> {
    #[derive(Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
    enum Source {
        Authored {
            #[serde(deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::STRIKES }>")]
            strikes: Vec<Strike>,
        },
        SelectedMelee {},
        Projectile {
            delivery: tor_simulation::projectile_profiles::ProjectileDeliveryKind,
        },
        NaturalMelee {
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::RIDERS_PER_STRIKE }>"
            )]
            additional_riders: Vec<Rider>,
        },
    }
    let authored: Source = crate::creature_authoring::named_map(deserializer)?;
    let source = match authored {
        Source::Authored { strikes } => {
            let strikes = strikes
                .into_iter()
                .map(|strike| strike.0)
                .collect::<Vec<_>>();
            validate_strike_shapes(&strikes).map_err(serde::de::Error::custom)?;
            StrikeSource::Authored { strikes }
        }
        Source::SelectedMelee {} => StrikeSource::SelectedMelee,
        Source::Projectile { delivery } => StrikeSource::Projectile { delivery },
        Source::NaturalMelee { additional_riders } => {
            let additional_riders = additional_riders
                .into_iter()
                .map(|rider| rider.0)
                .collect::<Vec<_>>();
            validate_rider_shapes(&additional_riders).map_err(serde::de::Error::custom)?;
            StrikeSource::NaturalMelee { additional_riders }
        }
    };
    Ok(source)
}

fn authored_effects<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<Effect>, D::Error> {
    let effects = bounded_vec::<_, AuthoredEffect, { tor_simulation::limits::EFFECTS_PER_BRANCH }>(
        deserializer,
    )?;
    Ok(effects.into_iter().map(|effect| effect.0).collect())
}

struct AuthoredEffect(Effect);
impl<'de> Deserialize<'de> for AuthoredEffect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = EffectSourceValue::deserialize(deserializer)?.0;
        if raw.get("kind").and_then(serde_json::Value::as_str) == Some("apply_persistent_condition")
        {
            #[derive(Deserialize)]
            #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
            enum Persistent {
                ApplyPersistentCondition {
                    condition: ConditionId,
                    #[serde(default)]
                    cancels: Option<ConditionId>,
                },
            }
            let Persistent::ApplyPersistentCondition { condition, cancels } =
                serde_json::from_value(raw).map_err(serde::de::Error::custom)?;
            Ok(Self(Effect::ApplyPersistentCondition {
                condition,
                cancels,
            }))
        } else {
            // Every existing effect retains its runtime decoder and raw operand
            // shapes. No authored fallback may widen those independent contracts.
            serde_json::from_value(raw)
                .map(Self)
                .map_err(serde::de::Error::custom)
        }
    }
}

// Buffer only a typed effect position for exact tag dispatch. Keep duplicates
// invalid, preserve arrays and numeric types, and never turn nonfinite floats
// into null (which would grant an absent optional operand).
struct EffectSourceValue(serde_json::Value);
impl<'de> Deserialize<'de> for EffectSourceValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = EffectSourceValue;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("an effect value without duplicate fields or nonfinite numbers")
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(EffectSourceValue(value.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(EffectSourceValue(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(EffectSourceValue(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                let number = serde_json::Number::from_f64(value)
                    .ok_or_else(|| E::custom("nonfinite effect number"))?;
                Ok(EffectSourceValue(serde_json::Value::Number(number)))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(EffectSourceValue(value.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(EffectSourceValue(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut input: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = input.next_element::<EffectSourceValue>()? {
                    values.push(value.0);
                }
                Ok(EffectSourceValue(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut input: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = input.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate effect field"));
                    }
                    values.insert(key, input.next_value::<EffectSourceValue>()?.0);
                }
                Ok(EffectSourceValue(values.into()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

struct Strike(StrikeDef);
impl<'de> Deserialize<'de> for Strike {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            offset: u64,
            unlock: Option<TalentId>,
            check: Option<CheckDef>,
            #[serde(deserialize_with = "authored_effects")]
            on_success: Vec<Effect>,
            #[serde(deserialize_with = "authored_effects")]
            on_failure: Vec<Effect>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
            )]
            descriptors: Vec<DescriptorId>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::RIDERS_PER_STRIKE }>"
            )]
            riders: Vec<Rider>,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self(StrikeDef {
            offset: fields.offset,
            unlock: fields.unlock,
            check: fields.check,
            on_success: fields.on_success,
            on_failure: fields.on_failure,
            descriptors: fields.descriptors,
            riders: fields.riders.into_iter().map(|rider| rider.0).collect(),
        }))
    }
}

struct Rider(RiderDef);
impl<'de> Deserialize<'de> for Rider {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            trigger: RiderTrigger,
            condition: Option<RiderCondition>,
            check: Option<RiderCheckDef>,
            #[serde(deserialize_with = "authored_effects")]
            effects: Vec<Effect>,
            #[serde(
                deserialize_with = "bounded_vec::<_, _, { tor_simulation::limits::DESCRIPTORS_PER_EFFECT }>"
            )]
            descriptors: Vec<DescriptorId>,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self(RiderDef {
            trigger: fields.trigger,
            condition: fields.condition,
            check: fields.check,
            effects: fields.effects,
            descriptors: fields.descriptors,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_effect_adapter_changes_only_persistent_opposition_absence() {
        for (value, valid) in [
            (
                json!({"kind":"theft","selection":{"kind":"magical_unit"}}),
                false,
            ),
            (
                json!({"kind":"theft","selection":{"kind":"magical_unit"},"relocation":null}),
                true,
            ),
            (json!({"kind":"hold_damage","category":"impact"}), true),
            (
                json!({"kind":"corrode_armor","slot":"body_armor","amount":0}),
                false,
            ),
            (
                json!({"kind":"corrode_armor","slot":"body_armor","amount":1}),
                true,
            ),
            (
                json!({"kind":"heal","amount":{"kind":"rolled","dice":[1,6,0]}}),
                false,
            ),
            (
                json!({"kind":"heal","amount":{"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}}}),
                true,
            ),
        ] {
            assert_eq!(
                serde_json::from_value::<Effect>(value.clone()).is_ok(),
                valid
            );
            assert_eq!(
                serde_json::from_value::<AuthoredEffect>(value.clone()).is_ok(),
                valid,
                "{value}"
            );
        }
        let absent = json!({"kind":"apply_persistent_condition","condition":"fixture_a"});
        assert!(serde_json::from_value::<Effect>(absent.clone()).is_err());
        let effect = serde_json::from_value::<AuthoredEffect>(absent).unwrap().0;
        assert_eq!(
            serde_json::to_value(effect).unwrap(),
            json!({
                "kind":"apply_persistent_condition","condition":"fixture_a","cancels":null
            })
        );
    }

    #[test]
    fn source_effect_buffer_preserves_numbers_and_rejects_nonfinite_optional_operands() {
        let source = r#"[-9223372036854775808,18446744073709551615,1.25,-0.0]"#;
        let raw = serde_json::from_str::<EffectSourceValue>(source).unwrap().0;
        assert_eq!(raw[0].as_i64(), Some(i64::MIN));
        assert_eq!(raw[1].as_u64(), Some(u64::MAX));
        assert_eq!(raw[2].as_f64(), Some(1.25));
        assert!(raw[3].as_f64().unwrap().is_sign_negative());
        for amount in [
            "0",
            "4294967295",
            "4294967296",
            "18446744073709551615",
            "-1",
            "-9223372036854775808",
            "1.0",
            "1.25",
        ] {
            let source =
                format!(r#"{{"kind":"heal","amount":{{"kind":"fixed","amount":{amount}}}}}"#);
            let runtime = serde_json::from_str::<Effect>(&source);
            let authored = serde_json::from_str::<AuthoredEffect>(&source);
            assert_eq!(runtime.is_ok(), authored.is_ok(), "{source}");
            if let (Ok(runtime), Ok(authored)) = (runtime, authored) {
                assert_eq!(runtime, authored.0);
            }
        }
        for number in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let input = serde::de::value::F64Deserializer::<serde::de::value::Error>::new(number);
            assert!(EffectSourceValue::deserialize(input).is_err());
        }
        for number in ["nan", "+inf", "-inf"] {
            // Null would be legal for both optional operands, so this specifically
            // detects numeric buffering that silently changes nonfinite to null.
            for source in [
                format!("kind = 'apply_persistent_condition'\ncondition = 'fixture_a'\ncancels = {number}"),
                format!("kind = 'hold_damage'\ncategory = 'impact'\ndescriptor = {number}"),
            ] {
                assert!(toml::from_str::<Effect>(&source).is_err(), "{source}");
                assert!(toml::from_str::<AuthoredEffect>(&source).is_err(), "{source}");
            }
        }
    }

    #[test]
    fn source_effect_buffer_rejects_duplicates_aliases_unknown_fields_and_positional_shapes() {
        for source in [
            r#"{"kind":"apply_persistent_condition","condition":"fixture_a","condition":"fixture_a"}"#,
            r#"{"kind":"apply_persistent_condition","condition":"fixture_a","cancels":null,"cancels":null}"#,
            r#"{"kind":"apply_persistent_condition","condition":"fixture_a","cancels":null,"canc\u0065ls":null}"#,
            r#"{"kind":"heal","kind":"heal","amount":{"kind":"fixed","amount":1}}"#,
            r#"{"kind":"heal","amount":{"kind":"fixed","amount":1,"amount":1}}"#,
            r#"{"kind":"apply_persistent_condition"}"#,
            r#"{"kind":"apply_persistent_condition","condition":"fixture_a","unexpected":true}"#,
            r#"{"kind":"ApplyPersistentCondition","condition":"fixture_a"}"#,
            r#"{"kind":{"apply_persistent_condition":{}},"condition":"fixture_a"}"#,
            r#"["apply_persistent_condition","fixture_a",null]"#,
            r#"{"kind":"heal","amount":{"kind":"rolled","dice":[1,6,0]}}"#,
        ] {
            assert!(
                serde_json::from_str::<AuthoredEffect>(source).is_err(),
                "{source}"
            );
        }
        for value in [json!(null), json!("fixture_b")] {
            let source = json!({"kind":"apply_persistent_condition", "condition":"fixture_a", "cancels":value});
            let runtime = serde_json::from_value::<Effect>(source.clone()).unwrap();
            let authored = serde_json::from_value::<AuthoredEffect>(source).unwrap().0;
            assert_eq!(runtime, authored);
        }
    }
}
