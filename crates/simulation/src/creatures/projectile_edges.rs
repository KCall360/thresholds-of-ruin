//! Frozen outgoing operands for a targetless physical release. Opponent filters
//! and the shared family/talent fold run only when a real recipient is known.
use super::{Content, DerivedCreature, SourceId};
use crate::bounded::{named_map, Bounded};
use crate::content_ids::{
    AbilityId, ClassId, ConditionId, SpeciesId, SubtypeId, TalentId, TemplateId, TypeId,
};
use crate::rules::{AbilityStat, Modifier, ModifierOp, Ruleset, Stat};
use crate::{ActorId, ItemId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// A valid sheet can carry a catalog of subtypes as well as bounded HD, talents,
// conditions, templates and equipment. Do not impose the wand's smaller cap.
const MAX_SOURCES: usize = 2 * crate::limits::CATALOG_ENTRIES
    + crate::limits::HIT_DICE
    + crate::limits::OWNED_TALENTS
    + crate::limits::CONDITIONS
    + crate::limits::TEMPLATES
    + crate::limits::EQUIPMENT_SOCKETS
    + 2;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Owner {
    Species {
        id: SpeciesId,
    },
    Type {
        id: TypeId,
    },
    Subtype {
        id: SubtypeId,
    },
    Class {
        id: ClassId,
    },
    Template {
        id: TemplateId,
    },
    Talent {
        id: TalentId,
    },
    Equipment {
        item: ItemId,
    },
    Condition {
        condition: ConditionId,
        source: ActorId,
    },
    QualifiedCondition {
        condition: ConditionId,
        source: ActorId,
        qualification: crate::conditions::TokenQualification,
    },
}
impl Owner {
    fn source(&self) -> SourceId {
        match self {
            Self::Species { id } => SourceId::Species(id.clone()),
            Self::Type { id } => SourceId::Type(id.clone()),
            Self::Subtype { id } => SourceId::Subtype(id.clone()),
            Self::Class { id } => SourceId::Class(id.clone()),
            Self::Template { id } => SourceId::Template(id.clone()),
            Self::Talent { id } => SourceId::Talent(id.clone()),
            Self::Equipment { item } => SourceId::Equipment(*item),
            Self::Condition { condition, source } => {
                SourceId::Condition(condition.clone(), *source)
            }
            Self::QualifiedCondition {
                condition,
                source,
                qualification,
            } => SourceId::QualifiedCondition {
                condition: condition.clone(),
                source: *source,
                qualification: qualification.clone(),
            },
        }
    }
    fn from_source(source: &SourceId) -> Self {
        match source {
            SourceId::Species(id) => Self::Species { id: id.clone() },
            SourceId::Type(id) => Self::Type { id: id.clone() },
            SourceId::Subtype(id) => Self::Subtype { id: id.clone() },
            SourceId::Class(id) => Self::Class { id: id.clone() },
            SourceId::Template(id) => Self::Template { id: id.clone() },
            SourceId::Talent(id) => Self::Talent { id: id.clone() },
            SourceId::Equipment(item) => Self::Equipment { item: *item },
            SourceId::Condition(condition, source) => Self::Condition {
                condition: condition.clone(),
                source: *source,
            },
            SourceId::QualifiedCondition {
                condition,
                source,
                qualification,
            } => Self::QualifiedCondition {
                condition: condition.clone(),
                source: *source,
                qualification: qualification.clone(),
            },
        }
    }
    fn valid(&self, rules: &Ruleset) -> bool {
        match self {
            Self::Type { id } => rules.creature_type(id).is_some(),
            Self::Subtype { id } => rules.subtype(id).is_some(),
            Self::Class { id } => rules.class(id).is_some(),
            Self::Talent { id } => rules.talent(id).is_some(),
            Self::Equipment { item } => item.0 > 0 && item.0 < u64::MAX,
            Self::Condition { condition, source } => {
                source.0 < u64::MAX && rules.condition(condition).is_some()
            }
            Self::QualifiedCondition {
                condition,
                source,
                qualification,
            } => {
                source.0 < u64::MAX
                    && rules
                        .condition(condition)
                        .is_some_and(|def| qualification.valid(def, rules))
            }
            Self::Species { .. } | Self::Template { .. } => true,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Row {
    owner: Owner,
    modifiers: Vec<Modifier>,
}
impl<'de> Deserialize<'de> for Row {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "named_map")]
            owner: Owner,
            modifiers: Bounded<Modifier, { crate::limits::MODIFIERS_PER_SOURCE }>,
        }
        let fields: Fields = named_map(d)?;
        Ok(Self {
            owner: fields.owner,
            modifiers: fields.modifiers.0,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct CapturedAttackEdges(Vec<Row>);
impl<'de> Deserialize<'de> for CapturedAttackEdges {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let rows = Bounded::<Row, MAX_SOURCES>::deserialize(d)?.0;
        if rows.windows(2).any(|pair| pair[0].owner >= pair[1].owner)
            || rows.iter().any(|row| row.modifiers.is_empty())
        {
            return Err(serde::de::Error::custom(
                "noncanonical captured attack edge sources",
            ));
        }
        Ok(Self(rows))
    }
}
fn relevant(modifier: &Modifier, rules: &Ruleset, ability: &AbilityId) -> bool {
    let (ModifierOp::Add { stat, .. } | ModifierOp::Set { stat, .. }) = &modifier.op else {
        return false;
    };
    match stat {
        Stat::Edge | Stat::CheckEdge => true,
        Stat::Ability {
            ability: selected,
            stat: AbilityStat::CheckEdge,
        } => selected == ability,
        Stat::AbilityTag {
            tag,
            stat: AbilityStat::CheckEdge,
        } => rules
            .abilities_with_tag(tag)
            .is_some_and(|members| members.contains(ability)),
        _ => false,
    }
}
impl CapturedAttackEdges {
    pub(crate) fn capture(sheet: &DerivedCreature, rules: &Ruleset, ability: &AbilityId) -> Self {
        Self(
            sheet
                .sources
                .iter()
                .filter_map(|(owner, modifiers)| {
                    let modifiers: Vec<_> = modifiers
                        .iter()
                        .filter(|m| relevant(m, rules, ability))
                        .cloned()
                        .collect();
                    (!modifiers.is_empty()).then(|| Row {
                        owner: Owner::from_source(owner),
                        modifiers,
                    })
                })
                .collect(),
        )
    }
    pub(crate) fn valid(&self, rules: &Ruleset, ability: &AbilityId) -> bool {
        self.0.len() <= MAX_SOURCES
            && self.0.windows(2).all(|pair| pair[0].owner < pair[1].owner)
            && self.0.iter().all(|row| {
                row.owner.valid(rules)
                    && !row.modifiers.is_empty()
                    && row.modifiers.len() <= crate::limits::MODIFIERS_PER_SOURCE
                    && rules.validate_condition_modifiers(&row.modifiers).is_ok()
                    && row.modifiers.iter().all(|m| relevant(m, rules, ability))
            })
    }
    pub(crate) fn contains_token(&self, token: &crate::conditions::TokenReservation) -> bool {
        self.0.iter().any(|row| match &row.owner {
            Owner::Condition { condition, source } => {
                token.qualification.is_none()
                    && *condition == token.condition
                    && *source == token.source
            }
            Owner::QualifiedCondition {
                condition,
                source,
                qualification,
            } => {
                token.qualification.as_ref() == Some(qualification)
                    && *condition == token.condition
                    && *source == token.source
            }
            _ => false,
        })
    }
    pub(crate) fn sources(&self) -> BTreeMap<SourceId, Vec<Modifier>> {
        self.0
            .iter()
            .map(|row| (row.owner.source(), row.modifiers.clone()))
            .collect()
    }
    pub(crate) fn catalog_valid(&self, content: &Content, next_item: u64, next_actor: u64) -> bool {
        self.0.iter().all(|row| match &row.owner {
            Owner::Species { id } => content.catalog().species.contains_key(id),
            Owner::Template { id } => content.catalog().templates.contains_key(id),
            Owner::Equipment { item } => item.0 < next_item,
            Owner::Condition { source, .. } => source.0 > 0 && source.0 < next_actor,
            Owner::QualifiedCondition {
                source,
                qualification,
                ..
            } => source.0 > 0 && source.0 < next_actor && qualification.opponent.0 < next_actor,
            _ => true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::creatures::CheckContext;
    use crate::rules::{CheckActor, DefenseKind, OutgoingEdgePolicy, Scope};

    #[test]
    fn captured_source_and_live_recipient_keep_one_shared_talent_cap() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let rules = build.ruleset();
        let ability = rules.document().abilities[0].id.clone();
        let talent = rules.document().talents[0].id.clone();
        let mut shooter = build.derive().unwrap();
        let mut defender = shooter.clone();
        shooter.sources.clear();
        defender.sources.clear();
        shooter.sources.insert(
            SourceId::Talent(talent.clone()),
            vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::CheckEdge,
                    value: 3,
                },
            }],
        );
        defender.sources.insert(
            SourceId::Talent(talent.clone()),
            vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::IncomingEdge {
                        defense: Some(DefenseKind::Physical),
                    },
                    value: 4,
                },
            }],
        );
        let captured = CapturedAttackEdges::capture(&shooter, rules, &ability);
        assert!(captured.valid(rules, &ability));
        let context = || CheckContext {
            ability: Some(&ability),
            roller_role: CheckActor::User,
            roller: ActorId(1),
            opponent: ActorId(2),
            defense: Some(DefenseKind::Physical),
        };
        let ordinary = shooter
            .check_edge_with_selection(
                &defender,
                rules,
                context(),
                true,
                OutgoingEdgePolicy::Ordinary,
                &[],
            )
            .unwrap();
        assert_eq!(
            ordinary, 1,
            "two positive talent participants share one cap"
        );
        assert_eq!(
            defender
                .projectile_edges(
                    &captured,
                    rules,
                    context(),
                    OutgoingEdgePolicy::Ordinary,
                    &[]
                )
                .unwrap(),
            (ordinary, 0)
        );
        shooter.sources.clear();
        assert_eq!(
            defender
                .projectile_edges(
                    &captured,
                    rules,
                    context(),
                    OutgoingEdgePolicy::Ordinary,
                    &[]
                )
                .unwrap()
                .0,
            1,
            "source remains frozen"
        );
        defender.sources.get_mut(&SourceId::Talent(talent)).unwrap()[0].op = ModifierOp::Add {
            stat: Stat::IncomingEdge {
                defense: Some(DefenseKind::Physical),
            },
            value: -4,
        };
        assert_eq!(
            defender
                .projectile_edges(
                    &captured,
                    rules,
                    context(),
                    OutgoingEdgePolicy::Ordinary,
                    &[]
                )
                .unwrap()
                .0,
            0,
            "recipient remains live"
        );
    }

    #[test]
    fn captured_sources_require_named_sorted_unique_rows_and_relevant_operands() {
        let build = crate::test_creatures::build(crate::test_creatures::species());
        let rules = build.ruleset();
        let ability = rules.document().abilities[0].id.clone();
        let mut sheet = build.derive().unwrap();
        sheet.sources.clear();
        sheet.sources.insert(
            SourceId::Equipment(ItemId(2)),
            vec![Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Edge,
                    value: 2,
                },
            }],
        );
        let captured = CapturedAttackEdges::capture(&sheet, rules, &ability);
        let value = serde_json::to_value(&captured).unwrap();
        assert_eq!(
            serde_json::from_value::<CapturedAttackEdges>(value.clone()).unwrap(),
            captured
        );
        let mut duplicate = value.clone();
        duplicate.as_array_mut().unwrap().push(value[0].clone());
        assert!(serde_json::from_value::<CapturedAttackEdges>(duplicate).is_err());
        for field in ["owner", "modifiers"] {
            let mut missing = value.clone();
            missing[0].as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<CapturedAttackEdges>(missing).is_err());
        }
        let mut positional = value.clone();
        positional[0] = serde_json::json!([value[0]["owner"], value[0]["modifiers"]]);
        assert!(serde_json::from_value::<CapturedAttackEdges>(positional).is_err());
        let mut unrelated = captured;
        unrelated.0[0].modifiers[0].op = ModifierOp::Add {
            stat: Stat::IncomingEdge { defense: None },
            value: 2,
        };
        assert!(!unrelated.valid(rules, &ability));
    }
}
