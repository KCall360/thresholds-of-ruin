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

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum DiagnosticEffect {
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
    Damage {
        target: ActorId,
        amount: u32,
    },
    Heal {
        target: ActorId,
        amount: u32,
    },
    HealAndImproveHitDie {
        target: ActorId,
        amount: u32,
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
        #[serde(with = "crate::integers::unsigned")]
        duration: u64,
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
                DiagnosticEffect::ConditionRemoved { target, condition } => {
                    *target == self.target && condition.valid()
                }
                DiagnosticEffect::Translate { target, .. } => *target == self.target,
                DiagnosticEffect::LinkedHold { target, holder } => {
                    *target == self.target && *holder == self.actor && target != holder
                }
                DiagnosticEffect::Observation { target } => *target == self.actor,
                DiagnosticEffect::CorrodeArmor { target, amount }
                | DiagnosticEffect::Damage { target, amount }
                | DiagnosticEffect::Heal { target, amount }
                | DiagnosticEffect::HealAndImproveHitDie { target, amount }
                | DiagnosticEffect::Nourish { target, amount } => {
                    target.0 != 0 && *amount <= 1_000_000
                }
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
                        && (1..=1_000_000).contains(duration)
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
            Damage {
                target: ActorId,
                amount: u32,
            },
            Heal {
                target: ActorId,
                amount: u32,
            },
            HealAndImproveHitDie {
                target: ActorId,
                amount: u32,
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
                #[serde(with = "crate::integers::unsigned")]
                duration: u64,
            },
            Observation {
                target: ActorId,
            },
        }
        Ok(match crate::bounded::named_map(deserializer)? {
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
            Fields::Damage { target, amount } => Self::Damage { target, amount },
            Fields::Heal { target, amount } => Self::Heal { target, amount },
            Fields::HealAndImproveHitDie { target, amount } => {
                Self::HealAndImproveHitDie { target, amount }
            }
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
