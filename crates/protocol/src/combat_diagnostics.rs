//! Private bounded outcome records. Validation checks structure and transport
//! invariants; rule formulas are owned solely by the simulation.
use crate::{
    AbilityOutcome, ActorId, CombatTraceView, ConditionView, ContentRef, InvalidCombatDiagnostics,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const MAX_COMBAT_REPORT_RECORDS: usize = 8;
pub const MAX_RETAINED_COMBAT_RECORDS: u8 = 64;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticResource {
    pub resource: ContentRef,
    pub maximum: u32,
    pub balance: u32,
    pub available: u32,
    #[serde(with = "crate::integers::unsigned")]
    pub recovery_elapsed: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticCombatant {
    #[serde(with = "crate::integers::unsigned")]
    pub health: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub maximum_health: u64,
    #[serde(with = "crate::integers::optional_unsigned")]
    pub injury: Option<u64>,
    pub dead: bool,
    pub resources: Vec<DiagnosticResource>,
    pub conditions: Vec<ConditionView>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticCost {
    pub resource: ContentRef,
    pub start: u32,
    pub resolution: u32,
}
/// Fixed-resource charge summary; each resource occurs in exactly one slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DiagnosticCostBundle {
    pub stamina: Option<DiagnosticCost>,
    pub focus: Option<DiagnosticCost>,
    pub mana: Option<DiagnosticCost>,
}
impl DiagnosticCostBundle {
    pub fn entries(&self) -> impl Iterator<Item = &DiagnosticCost> {
        [&self.stamina, &self.focus, &self.mana]
            .into_iter()
            .filter_map(Option::as_ref)
    }
    fn valid(&self) -> bool {
        self.entries().next().is_some()
            && [
                ("stamina", &self.stamina),
                ("focus", &self.focus),
                ("mana", &self.mana),
            ]
            .into_iter()
            .all(|(id, entry)| {
                entry.as_ref().is_none_or(|cost| {
                    cost.resource.valid()
                        && cost.resource.id == id
                        && u64::from(cost.start) + u64::from(cost.resolution) <= 1_000_000
                })
            })
    }
}
fn read_diagnostic_cost<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<DiagnosticCost>, D::Error> {
    #[derive(Deserialize)]
    struct Named(#[serde(deserialize_with = "crate::bounded::named_map")] DiagnosticCost);
    Option::<Named>::deserialize(d).map(|value| value.map(|named| named.0))
}
impl<'de> Deserialize<'de> for DiagnosticCostBundle {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "read_diagnostic_cost")]
            stamina: Option<DiagnosticCost>,
            #[serde(deserialize_with = "read_diagnostic_cost")]
            focus: Option<DiagnosticCost>,
            #[serde(deserialize_with = "read_diagnostic_cost")]
            mana: Option<DiagnosticCost>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            stamina: fields.stamina,
            focus: fields.focus,
            mana: fields.mana,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid cost bundle"));
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrengthGain {
    Unchanged,
    Restored,
    Acquired,
}
impl<'de> Deserialize<'de> for StrengthGain {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "unchanged" => Ok(Self::Unchanged),
            "restored" => Ok(Self::Restored),
            "acquired" => Ok(Self::Acquired),
            _ => Err(serde::de::Error::custom("unknown strength gain result")),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HasteDoseOutcome {
    Hasted {
        #[serde(with = "crate::integers::unsigned")]
        duration: u64,
    },
    Overdose {
        #[serde(with = "crate::integers::unsigned")]
        rest: u64,
    },
}
impl HasteDoseOutcome {
    fn valid(&self) -> bool {
        match self {
            Self::Hasted { duration } => (1..=1_000_000).contains(duration),
            Self::Overdose { rest } => *rest <= 1_000_000,
        }
    }
}
impl<'de> Deserialize<'de> for HasteDoseOutcome {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Hasted {
                #[serde(with = "crate::integers::unsigned")]
                duration: u64,
            },
            Overdose {
                #[serde(with = "crate::integers::unsigned")]
                rest: u64,
            },
        }
        let value = match crate::bounded::named_map(deserializer)? {
            Fields::Hasted { duration } => Self::Hasted { duration },
            Fields::Overdose { rest } => Self::Overdose { rest },
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid haste dose duration"));
        }
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiagnosticEffect {
    GainStrength { target: ActorId, change: StrengthGain },
    RaiseLevel {
        target: ActorId,
        #[serde(with = "crate::integers::unsigned")]
        amount: u64,
    },
    HasteDose {
        target: ActorId,
        condition: ContentRef,
        rest: ContentRef,
        outcome: HasteDoseOutcome,
    },
    CancelSpecials {
        target: ActorId,
        changed: bool,
    },
    DrainLife {
        target: ActorId,
        changed: bool,
    },
    DirectHealthLoss {
        target: ActorId,
        #[serde(with = "crate::integers::unsigned")]
        amount: u64,
    },
    RoomLight {
        target: ActorId,
        changed: bool,
    },
    ConditionRemoved {
        target: ActorId,
        condition: ContentRef,
    },
    CorrodeArmor {
        target: ActorId,
        amount: u32,
    },
    LinkedHold {
        target: ActorId,
        holder: ActorId,
    },
    Translate {
        target: ActorId,
        mode: DiagnosticMovementMode,
        result: DiagnosticMovementResult,
    },
    Relocate {
        target: ActorId,
        mode: DiagnosticMovementMode,
        result: DiagnosticMovementResult,
    },
    ArrestDescent {
        target: ActorId,
    },
    Damage {
        target: ActorId,
        amount: u32,
    },
    Heal {
        target: ActorId,
        amount: u32,
    },
    HealAndImproveHitDice {
        target: ActorId,
        amount: u32,
        credits: u32,
    },
    LoseAttribute {
        target: ActorId,
        attribute: ContentRef,
        amount: u32,
    },
    RestoreAttribute {
        target: ActorId,
        attribute: ContentRef,
        amount: u32,
    },
    DrainLatestHitDie {
        target: ActorId,
        amount: u32,
    },
    RestoreHitDie {
        target: ActorId,
        amount: u32,
    },
    LoseMaximumHealth {
        target: ActorId,
        amount: u32,
    },
    RestoreMaximumHealth {
        target: ActorId,
        amount: u32,
    },
    Nourish {
        target: ActorId,
        amount: u32,
    },
    Restore {
        target: ActorId,
        resource: ContentRef,
        amount: u32,
    },
    Condition {
        target: ActorId,
        condition: ContentRef,
        source: ActorId,
        #[serde(with = "crate::integers::optional_unsigned")]
        duration: Option<u64>,
    },
    Observation {
        target: ActorId,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticOutcome {
    pub result: AbilityOutcome,
    pub effects: Vec<DiagnosticEffect>,
}
impl<'de> Deserialize<'de> for DiagnosticOutcome {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            result: AbilityOutcome,
            #[serde(deserialize_with = "crate::bounded::bounded::<_, _, 128>")]
            effects: Vec<DiagnosticEffect>,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            result: fields.result,
            effects: fields.effects,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatDiagnosticRecord {
    #[serde(with = "crate::integers::unsigned")]
    pub sequence: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    pub actor: ActorId,
    pub target: ActorId,
    pub ability: Option<ContentRef>,
    #[serde(with = "crate::integers::optional_unsigned")]
    pub intention: Option<u64>,
    #[serde(with = "crate::integers::optional_unsigned")]
    pub origin_intention: Option<u64>,
    #[serde(with = "crate::integers::unsigned")]
    pub preparation: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub started: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub remaining: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub recovery: u64,
    pub charge: Option<DiagnosticCostBundle>,
    pub actor_before: DiagnosticCombatant,
    pub target_before: DiagnosticCombatant,
    pub actor_after: DiagnosticCombatant,
    pub target_after: DiagnosticCombatant,
    pub outcome: DiagnosticOutcome,
    pub trace: CombatTraceView,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatDiagnosticsView {
    pub enabled: bool,
    #[serde(with = "crate::integers::unsigned")]
    pub tick: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub captured: u64,
    #[serde(with = "crate::integers::unsigned")]
    pub dropped: u64,
    pub retained: u8,
    #[serde(with = "crate::integers::unsigned")]
    pub through: u64,
    pub records: Vec<CombatDiagnosticRecord>,
}
impl DiagnosticCombatant {
    fn valid(&self) -> bool {
        let mut resources = BTreeSet::new();
        self.health <= self.maximum_health
            && self.dead == (self.health == 0)
            && self.injury.is_none_or(|injury| {
                self.dead || self.maximum_health.checked_sub(injury) == Some(self.health)
            })
            && self.resources.len() <= 4096
            && self.resources.iter().all(|resource| {
                resource.resource.valid()
                    && resources.insert(&resource.resource.id)
                    && resource.maximum <= 1_000_000
                    && resource.balance <= resource.maximum
                    && resource.available <= resource.balance
            })
            && self.conditions.len() <= 128
            && self.conditions.iter().all(ConditionView::valid)
            && (!self.dead || self.conditions.is_empty())
    }
}
impl CombatDiagnosticRecord {
    fn valid(&self) -> bool {
        self.sequence != 0
            && self.actor.0 != 0
            && self.target.0 != 0
            && self.ability.as_ref().is_none_or(ContentRef::valid)
            && [self.intention, self.origin_intention]
                .into_iter()
                .flatten()
                .all(|id| id != 0 && id != u64::MAX)
            && self.remaining <= self.preparation
            && self
                .started
                .checked_add(self.remaining)
                .is_some_and(|due| due <= self.tick)
            && self.tick.checked_add(self.recovery).is_some()
            && [
                &self.actor_before,
                &self.target_before,
                &self.actor_after,
                &self.target_after,
            ]
            .into_iter()
            .all(DiagnosticCombatant::valid)
            && self.charge.as_ref().is_none_or(DiagnosticCostBundle::valid)
            && self.outcome.effects.len() <= 128
            && self.outcome.effects.iter().all(|effect| match effect {
                DiagnosticEffect::HasteDose {
                    target,
                    condition,
                    rest,
                    outcome,
                } => {
                    *target == self.target
                        && condition.valid()
                        && rest.valid()
                        && condition.id != rest.id
                        && outcome.valid()
                }
                DiagnosticEffect::ConditionRemoved { target, condition } => {
                    *target == self.target && condition.valid()
                }
                DiagnosticEffect::RoomLight { target, .. }
                | DiagnosticEffect::DrainLife { target, .. } => *target == self.actor,
                DiagnosticEffect::DirectHealthLoss { target, .. }
                | DiagnosticEffect::GainStrength { target, .. }
                | DiagnosticEffect::RaiseLevel { target, .. } => *target == self.target,
                DiagnosticEffect::CancelSpecials { target, .. }
                | DiagnosticEffect::Translate { target, .. }
                | DiagnosticEffect::Relocate { target, .. }
                | DiagnosticEffect::ArrestDescent { target } => *target == self.target,
                DiagnosticEffect::LinkedHold { target, holder } => {
                    *target == self.target && *holder == self.actor && target != holder
                }
                DiagnosticEffect::Observation { target } => *target == self.actor,
                DiagnosticEffect::CorrodeArmor { target, amount }
                | DiagnosticEffect::Damage { target, amount }
                | DiagnosticEffect::Heal { target, amount }
                | DiagnosticEffect::Nourish { target, amount } => {
                    target.0 != 0 && *amount <= 1_000_000
                }
                DiagnosticEffect::HealAndImproveHitDice {
                    target,
                    amount,
                    credits,
                } => target.0 != 0 && *amount <= 1_000_000 && (1..=2).contains(credits),
                DiagnosticEffect::LoseAttribute {
                    target,
                    attribute,
                    amount,
                }
                | DiagnosticEffect::RestoreAttribute {
                    target,
                    attribute,
                    amount,
                } => {
                    *target == self.target
                        && attribute.valid()
                        && *amount <= 1000
                        && matches!(
                            attribute.id.as_str(),
                            "strength"
                                | "speed"
                                | "intellect"
                                | "willpower"
                                | "awareness"
                                | "presence"
                        )
                }
                DiagnosticEffect::DrainLatestHitDie { target, amount }
                | DiagnosticEffect::RestoreHitDie { target, amount } => {
                    *target == self.target && *amount <= 1
                }
                DiagnosticEffect::LoseMaximumHealth { target, amount }
                | DiagnosticEffect::RestoreMaximumHealth { target, amount } => {
                    *target == self.target && *amount <= 1_000_000
                }
                DiagnosticEffect::Restore {
                    target,
                    resource,
                    amount,
                } => target.0 != 0 && resource.valid() && *amount <= 1_000_000,
                DiagnosticEffect::Condition {
                    target,
                    condition,
                    source,
                    duration,
                } => {
                    target.0 != 0
                        && source.0 != 0
                        && condition.valid()
                        && duration.is_none_or(|duration| (1..=1_000_000).contains(&duration))
                }
            })
            && self.trace.validate().is_ok()
    }
}
impl CombatDiagnosticsView {
    pub fn validate(&self) -> Result<(), InvalidCombatDiagnostics> {
        let valid = self.retained <= MAX_RETAINED_COMBAT_RECORDS
            && self.captured.checked_sub(u64::from(self.retained)) == Some(self.dropped)
            && (self.dropped == 0 || self.retained == MAX_RETAINED_COMBAT_RECORDS)
            && self.through >= self.dropped
            && self.through <= self.captured
            && self.records.len()
                == (self.through - self.dropped).min(MAX_COMBAT_REPORT_RECORDS as u64) as usize
            && (self.enabled || self.captured == 0)
            && self.records.iter().enumerate().all(|(index, record)| {
                record.sequence == self.through - self.records.len() as u64 + index as u64 + 1
                    && record.tick <= self.tick
                    && (index == 0 || self.records[index - 1].tick <= record.tick)
                    && record.valid()
            });
        if valid
            && matches!(
                crate::codec::encoded_length(self, crate::MAX_RESPONSE_BYTES),
                Ok(Some(_))
            )
        {
            Ok(())
        } else {
            Err(InvalidCombatDiagnostics)
        }
    }
}
impl<'de> Deserialize<'de> for DiagnosticEffect {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            GainStrength { target: ActorId, change: StrengthGain },
            RaiseLevel {
                target: ActorId,
                #[serde(with = "crate::integers::unsigned")]
                amount: u64,
            },
            HasteDose {
                target: ActorId,
                #[serde(deserialize_with = "crate::bounded::named_map")]
                condition: ContentRef,
                #[serde(deserialize_with = "crate::bounded::named_map")]
                rest: ContentRef,
                outcome: HasteDoseOutcome,
            },
            CancelSpecials {
                target: ActorId,
                changed: bool,
            },
            DrainLife {
                target: ActorId,
                changed: bool,
            },
            DirectHealthLoss {
                target: ActorId,
                #[serde(with = "crate::integers::unsigned")]
                amount: u64,
            },
            RoomLight {
                target: ActorId,
                changed: bool,
            },
            ConditionRemoved {
                target: ActorId,
                #[serde(deserialize_with = "crate::bounded::named_map")]
                condition: ContentRef,
            },
            CorrodeArmor {
                target: ActorId,
                amount: u32,
            },
            LinkedHold {
                target: ActorId,
                holder: ActorId,
            },
            Translate {
                target: ActorId,
                mode: DiagnosticMovementMode,
                result: DiagnosticMovementResult,
            },
            Relocate {
                target: ActorId,
                mode: DiagnosticMovementMode,
                result: DiagnosticMovementResult,
            },
            ArrestDescent {
                target: ActorId,
            },
            Damage {
                target: ActorId,
                amount: u32,
            },
            Heal {
                target: ActorId,
                amount: u32,
            },
            HealAndImproveHitDice {
                target: ActorId,
                amount: u32,
                credits: u32,
            },
            LoseAttribute {
                target: ActorId,
                #[serde(deserialize_with = "crate::bounded::named_map")]
                attribute: ContentRef,
                amount: u32,
            },
            RestoreAttribute {
                target: ActorId,
                #[serde(deserialize_with = "crate::bounded::named_map")]
                attribute: ContentRef,
                amount: u32,
            },
            DrainLatestHitDie {
                target: ActorId,
                amount: u32,
            },
            RestoreHitDie {
                target: ActorId,
                amount: u32,
            },
            LoseMaximumHealth {
                target: ActorId,
                amount: u32,
            },
            RestoreMaximumHealth {
                target: ActorId,
                amount: u32,
            },
            Nourish {
                target: ActorId,
                amount: u32,
            },
            Restore {
                target: ActorId,
                resource: ContentRef,
                amount: u32,
            },
            Condition {
                target: ActorId,
                condition: ContentRef,
                source: ActorId,
                #[serde(with = "crate::integers::optional_unsigned")]
                duration: Option<u64>,
            },
            Observation {
                target: ActorId,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
            Fields::GainStrength { target, change } => Self::GainStrength { target, change },
            Fields::RaiseLevel { target, amount } => Self::RaiseLevel { target, amount },
            Fields::HasteDose {
                target,
                condition,
                rest,
                outcome,
            } => Self::HasteDose {
                target,
                condition,
                rest,
                outcome,
            },
            Fields::CancelSpecials { target, changed } => Self::CancelSpecials { target, changed },
            Fields::DrainLife { target, changed } => Self::DrainLife { target, changed },
            Fields::DirectHealthLoss { target, amount } => {
                Self::DirectHealthLoss { target, amount }
            }
            Fields::RoomLight { target, changed } => Self::RoomLight { target, changed },
            Fields::ConditionRemoved { target, condition } => {
                Self::ConditionRemoved { target, condition }
            }
            Fields::CorrodeArmor { target, amount } => Self::CorrodeArmor { target, amount },
            Fields::LinkedHold { target, holder } => Self::LinkedHold { target, holder },
            Fields::Translate {
                target,
                mode,
                result,
            } => Self::Translate {
                target,
                mode,
                result,
            },
            Fields::Relocate {
                target,
                mode,
                result,
            } => Self::Relocate {
                target,
                mode,
                result,
            },
            Fields::ArrestDescent { target } => Self::ArrestDescent { target },
            Fields::Damage { target, amount } => Self::Damage { target, amount },
            Fields::Heal { target, amount } => Self::Heal { target, amount },
            Fields::HealAndImproveHitDice {
                target,
                amount,
                credits,
            } => Self::HealAndImproveHitDice {
                target,
                amount,
                credits,
            },
            Fields::LoseAttribute {
                target,
                attribute,
                amount,
            } => Self::LoseAttribute {
                target,
                attribute,
                amount,
            },
            Fields::RestoreAttribute {
                target,
                attribute,
                amount,
            } => Self::RestoreAttribute {
                target,
                attribute,
                amount,
            },
            Fields::DrainLatestHitDie { target, amount } => {
                Self::DrainLatestHitDie { target, amount }
            }
            Fields::RestoreHitDie { target, amount } => Self::RestoreHitDie { target, amount },
            Fields::LoseMaximumHealth { target, amount } => {
                Self::LoseMaximumHealth { target, amount }
            }
            Fields::RestoreMaximumHealth { target, amount } => {
                Self::RestoreMaximumHealth { target, amount }
            }
            Fields::Nourish { target, amount } => Self::Nourish { target, amount },
            Fields::Restore {
                target,
                resource,
                amount,
            } => Self::Restore {
                target,
                resource,
                amount,
            },
            Fields::Condition {
                target,
                condition,
                source,
                duration,
            } => Self::Condition {
                target,
                condition,
                source,
                duration,
            },
            Fields::Observation { target } => Self::Observation { target },
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticMovementMode {
    Voluntary,
    Forced,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticMovementResult {
    Moved,
    Blocked,
    Ineligible,
    Stale,
    Ambiguous,
}

#[cfg(test)]
mod room_light_diagnostic_tests {
    use super::*;
    #[test]
    fn room_light_requires_exact_target_and_changed_result() {
        for changed in [false, true] {
            let value = serde_json::json!({"type":"room_light","target":"1","changed":changed});
            let decoded: DiagnosticEffect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
        for value in [
            serde_json::json!({"type":"room_light","target":"1"}),
            serde_json::json!({"type":"room_light","target":"1","changed":true,"room":9}),
            serde_json::json!(["room_light", "1", true]),
        ] {
            assert!(serde_json::from_value::<DiagnosticEffect>(value).is_err());
        }
    }
}

#[cfg(test)]
mod direct_health_diagnostic_tests {
    use super::*;
    #[test]
    fn direct_health_loss_is_exact_full_width_decimal_and_closed() {
        for amount in [0, 9_007_199_254_740_993, u64::MAX] {
            let value = serde_json::json!({"type":"direct_health_loss","target":"2","amount":amount.to_string()});
            let decoded: DiagnosticEffect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
        for value in [
            serde_json::json!({"type":"direct_health_loss","target":"2","amount":"18446744073709551616"}),
            serde_json::json!({"type":"direct_health_loss","target":"2","amount":"1","recipients":2}),
            serde_json::json!({"type":"drain_life","target":"1"}),
            serde_json::json!(["drain_life", "1", false]),
        ] {
            assert!(serde_json::from_value::<DiagnosticEffect>(value).is_err());
        }
    }
}

#[cfg(test)]
mod haste_dose_diagnostic_tests {
    use super::*;
    #[test]
    fn haste_dose_outcomes_are_named_closed_and_have_branch_specific_tick_bounds() {
        for outcome in [
            serde_json::json!({"type":"hasted","duration":"1"}),
            serde_json::json!({"type":"hasted","duration":"1000000"}),
            serde_json::json!({"type":"overdose","rest":"0"}),
            serde_json::json!({"type":"overdose","rest":"1000000"}),
        ] {
            let value = serde_json::json!({"type":"haste_dose","target":"2",
                "condition":{"id":"rogue.hasted","name":"Hasted"},
                "rest":{"id":"rogue.haste_exhaustion","name":"Exhausted"},"outcome":outcome});
            let decoded: DiagnosticEffect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
            for field in ["condition", "rest", "outcome"] {
                let mut missing = value.clone();
                missing.as_object_mut().unwrap().remove(field);
                assert!(serde_json::from_value::<DiagnosticEffect>(missing).is_err());
            }
            for field in ["condition", "rest"] {
                let mut positional = value.clone();
                positional[field] = serde_json::json!(["rogue.hasted", "Hasted"]);
                assert!(serde_json::from_value::<DiagnosticEffect>(positional).is_err());
            }
        }
        for value in [
            serde_json::json!({"type":"hasted","duration":"0"}),
            serde_json::json!({"type":"hasted","duration":"1000001"}),
            serde_json::json!({"type":"overdose","rest":"1000001"}),
            serde_json::json!({"type":"overdose","rest":0}),
            serde_json::json!({"type":"hasted","duration":"01"}),
            serde_json::json!({"type":"hasted","duration":"18446744073709551616"}),
            serde_json::json!({"type":"hasted","duration":"1","rest":"0"}),
            serde_json::json!({"type":"overdose"}),
            serde_json::json!({"type":"other","duration":"1"}),
            serde_json::json!(["hasted", "1"]),
        ] {
            assert!(serde_json::from_value::<HasteDoseOutcome>(value).is_err());
        }
    }
}

#[cfg(test)]
mod growth_diagnostic_tests {
    use super::*;
    #[test]
    fn growth_results_are_closed_named_and_preserve_full_width_credit() {
        for change in ["unchanged", "restored", "acquired"] {
            let value = serde_json::json!({"type":"gain_strength","target":"2","change":change});
            let decoded: DiagnosticEffect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
        for amount in [0, u64::from(u32::MAX) + 1, u64::MAX] {
            let value = serde_json::json!({"type":"raise_level","target":"2","amount":amount.to_string()});
            let decoded: DiagnosticEffect = serde_json::from_value(value.clone()).unwrap();
            assert_eq!(serde_json::to_value(decoded).unwrap(), value);
        }
        for value in [
            serde_json::json!({"type":"gain_strength","target":"2","change":{"acquired":null}}),
            serde_json::json!({"type":"gain_strength","target":"2","change":"other"}),
            serde_json::json!({"type":"gain_strength","target":"2"}),
            serde_json::json!({"type":"gain_strength","target":"2","change":"restored","amount":1}),
            serde_json::json!({"type":"raise_level","target":"2","amount":1}),
            serde_json::json!({"type":"raise_level","target":"2","amount":"01"}),
            serde_json::json!({"type":"raise_level","target":"2","amount":"18446744073709551616"}),
            serde_json::json!({"type":"raise_level","target":"2"}),
            serde_json::json!(["raise_level","2","1"]),
        ] {
            assert!(serde_json::from_value::<DiagnosticEffect>(value).is_err());
        }
        assert!(serde_json::from_str::<DiagnosticEffect>(r#"{"type":"gain_strength","target":"2","change":"acquired","change":"restored"}"#).is_err());
    }
}
