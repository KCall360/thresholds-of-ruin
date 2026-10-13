//! Explicit private projections of source data. No actor mutation or perception.
use super::{
    attribute, attributes, category, condition, descriptor, hd_source, label, named, own_stats,
    skill, talent,
};
use s::rules::{Modifier, ModifierOp, Ruleset, Scope, Selector, Stat};
use tor_protocol as p;
use tor_simulation as s;

pub(crate) fn attack_view(
    value: &s::attacks::MeleeAttack,
    rules: Option<&Ruleset>,
) -> p::AttackView {
    let primary = value
        .damage()
        .primary()
        .expect("melee constructor requires a primary");
    p::AttackView {
        skill: skill(value.skill()),
        bonus: value.bonus(),
        wind_up: value.wind_up() as u32,
        recovery: value.recovery() as u32,
        damage: p::DamageView {
            primary: p::DamageKeyView {
                category: category(primary.category),
                descriptor: primary.descriptor.as_ref().map(|id| descriptor(id, rules)),
                sides: primary.sides,
            },
            components: value
                .damage()
                .components()
                .iter()
                .map(|component| p::DamageComponentView {
                    category: category(component.key().category),
                    descriptor: component
                        .key()
                        .descriptor
                        .as_ref()
                        .map(|id| descriptor(id, rules)),
                    amount: match component.amount() {
                        s::damage::DamageAmount::Fixed(value) => {
                            p::DamageAmountView::Fixed { value }
                        }
                        s::damage::DamageAmount::Rolled(pool) => p::DamageAmountView::Rolled {
                            count: pool.count(),
                            sides: pool.sides(),
                            bonus: pool.bonus(),
                        },
                    },
                })
                .collect(),
        },
    }
}
fn field(id: &str, value: p::DiagnosticValue) -> p::DiagnosticField {
    p::DiagnosticField {
        id: id.into(),
        name: label(id),
        value,
    }
}
fn text(id: &str, value: impl Into<String>) -> p::DiagnosticField {
    field(
        id,
        p::DiagnosticValue::Text {
            value: value.into(),
        },
    )
}
fn content(id: &str, value: p::ContentRef) -> p::DiagnosticField {
    field(id, p::DiagnosticValue::Content { value })
}
fn signed(id: &str, value: i64) -> p::DiagnosticField {
    field(id, p::DiagnosticValue::Signed { value })
}
fn selector(value: &Selector, rules: &Ruleset) -> p::DiagnosticField {
    match value {
        Selector::Category { category: value } => content("category", super::primitive(*value)),
        Selector::Descriptor { descriptor: value } => {
            content("descriptor", descriptor(value, Some(rules)))
        }
    }
}
fn stat(value: &Stat, rules: &Ruleset) -> Vec<p::DiagnosticField> {
    match value {
        Stat::Attribute { attribute: value } => vec![
            text("stat", "attribute"),
            content("attribute", attribute(*value)),
        ],
        Stat::Defense { defense } => vec![
            text("stat", "defense"),
            content("defense", super::primitive(*defense)),
        ],
        Stat::Health => vec![text("stat", "health")],
        Stat::HealthRecoveryPeriod => vec![text("stat", "health_recovery_period")],
        Stat::HealthRecoveryPeriodPercent => vec![text("stat", "health_recovery_period_percent")],
        Stat::HealthRecoveryResetPercent => vec![text("stat", "health_recovery_reset_percent")],
        Stat::ResourceMax { resource } => vec![
            text("stat", "resource_max"),
            content("resource", super::resource(*resource)),
        ],
        Stat::Ability { ability, stat } => vec![
            text("stat", "ability"),
            content("ability", super::ability(ability, rules)),
            content("ability_stat", super::primitive(*stat)),
        ],
        Stat::AbilityTag { tag, stat } => vec![
            text("stat", "ability_tag"),
            content(
                "ability_tag",
                named(
                    tag.as_str(),
                    rules
                        .ability_tag(tag)
                        .map(|definition| definition.name.as_str()),
                ),
            ),
            content("ability_stat", super::primitive(*stat)),
        ],
        Stat::Edge => vec![text("stat", "edge")],
        Stat::CheckEdge => vec![text("stat", "check_edge")],
        Stat::IncomingEdge { defense } => {
            let mut fields = vec![text("stat", "incoming_edge")];
            if let Some(defense) = defense {
                fields.push(content("defense", super::primitive(*defense)));
            }
            fields
        }
    }
}
pub(crate) fn modifier(value: &Modifier, rules: &Ruleset) -> p::CombatTraceStep {
    let mut fields = vec![text(
        "scope",
        match value.scope {
            Scope::Always => "always",
            Scope::VersusSource => "versus_source",
        },
    )];
    let id = match &value.op {
        ModifierOp::Add {
            stat: value_stat,
            value,
        } => {
            fields.extend(stat(value_stat, rules));
            fields.push(signed("value", *value));
            "add"
        }
        ModifierOp::Set {
            stat: value_stat,
            value,
        } => {
            fields.extend(stat(value_stat, rules));
            fields.push(signed("value", *value));
            "set"
        }
        ModifierOp::Flag { flag } => {
            fields.push(content(
                "flag",
                named(flag, rules.flag(flag).map(|def| def.name.as_str())),
            ));
            "flag"
        }
        ModifierOp::Immunity { selector: value } => {
            fields.push(selector(value, rules));
            "immunity"
        }
        ModifierOp::Reduction {
            selector: value,
            amount,
        } => {
            fields.push(selector(value, rules));
            fields.push(signed("amount", i64::from(*amount)));
            "reduction"
        }
        ModifierOp::GrantAbility { ability } => {
            fields.push(content("ability", super::ability(ability, rules)));
            "grant_ability"
        }
    };
    p::CombatTraceStep {
        id: id.into(),
        name: label(id),
        fields,
    }
}
fn source(value: &s::creatures::SourceId, rules: &Ruleset) -> (String, p::ContentRef) {
    use s::creatures::SourceId as S;
    let (kind, value) = match value {
        S::Species(id) => ("species", named(id, None)),
        S::Type(id) => ("type", super::kind(id, rules)),
        S::Subtype(id) => ("subtype", super::subtype(id, rules)),
        S::Class(id) => (
            "class",
            named(id, rules.class(id).map(|def| def.name.as_str())),
        ),
        S::Template(id) => ("template", named(id, None)),
        S::Talent(id) => ("talent", talent(id, rules)),
        S::Equipment(id) => (
            "equipment",
            named(id.0.to_string(), Some(&format!("Equipment {}", id.0))),
        ),
        S::Condition(id, _) => ("condition", condition(id, rules)),
    };
    (kind.into(), value)
}
pub(crate) fn inspect(game: &s::Game, actor: s::ActorId) -> Option<p::CreatureInspectionView> {
    let view = game.inspect_creature(actor)?;
    let state = view.creature;
    let build = state.build();
    let rules = build.ruleset();
    let health = state.health();
    let derived = state.derived();
    let selected = &derived.selected_attack;
    let (selected_category, selected_source) = source(&selected.source, rules);
    let selected_attack = p::SelectedAttackView {
        category: selected_category,
        source: selected_source,
        socket: selected.socket.map(|socket| socket.0),
        attack: attack_view(&selected.attack, Some(rules)),
        attribute: attribute(selected.attribute),
        attribute_value: selected.attribute_value,
        rank: selected.rank,
        check_bonus: selected.check_bonus,
        abilities: derived
            .ability_timings
            .iter()
            .map(|(id, timing)| {
                let modifiers = derived.ability_values(id);
                p::CombatTraceStep {
                    id: "ability_profile".into(),
                    name: "Ability profile".into(),
                    fields: vec![
                        content("ability", super::ability(id, rules)),
                        signed("wind_up", timing.preparation as i64),
                        signed("recovery", timing.recovery as i64),
                        signed("check_modifier", i64::from(modifiers.check_bonus)),
                        signed("extra_dice", i64::from(modifiers.dice)),
                        signed("flat_bonus", i64::from(modifiers.flat)),
                    ],
                }
            })
            .collect(),
    };
    let sources = state
        .derived()
        .sources
        .iter()
        .map(|(origin, values)| {
            let (category, source) = source(origin, rules);
            let mut rows = values
                .iter()
                .map(|value| modifier(value, rules))
                .collect::<Vec<_>>();
            if let s::creatures::SourceId::Template(id) = origin {
                if let Some(def) = build.content().catalog().templates.get(id) {
                    rows.push(p::CombatTraceStep {
                        id: "priority".into(),
                        name: "Priority".into(),
                        fields: vec![signed("value", i64::from(def.priority))],
                    });
                    for (attribute_id, amount) in def.adjustments.iter() {
                        if *amount != 0 {
                            rows.push(p::CombatTraceStep {
                                id: "attribute_adjustment".into(),
                                name: "Attribute adjustment".into(),
                                fields: vec![
                                    content("attribute", attribute(attribute_id)),
                                    signed("value", i64::from(*amount)),
                                ],
                            });
                        }
                    }
                    for (attribute_id, amount) in def.overrides.iter() {
                        if let Some(amount) = amount {
                            rows.push(p::CombatTraceStep {
                                id: "attribute_override".into(),
                                name: "Attribute override".into(),
                                fields: vec![
                                    content("attribute", attribute(attribute_id)),
                                    signed("value", i64::from(*amount)),
                                ],
                            });
                        }
                    }
                }
            }
            p::InspectionSource {
                owner: match origin {
                    s::creatures::SourceId::Condition(_, actor) => Some(p::ActorId(actor.0)),
                    _ => None,
                },
                category,
                source,
                rows,
            }
        })
        .collect();
    Some(p::CreatureInspectionView {
        actor: p::ActorId(actor.0),
        tick: game.tick(),
        name: view.identity.name,
        faction: view.identity.faction,
        species: named(build.species_id(), None),
        natural_attack: attack_view(&build.species().melee, Some(rules)),
        selected_attack,
        initial_attributes: attributes(build.initial_attributes()),
        templates: build
            .template_ids()
            .iter()
            .map(|id| named(id, None))
            .collect(),
        max_hp: health.maximum(),
        hp: health.current(),
        injury: health.injury(),
        dead: health.dead(),
        stats: own_stats(view.stats),
        hit_dice: build
            .ledger()
            .entries()
            .iter()
            .zip(build.choices())
            .zip(&derived.health_contributions)
            .enumerate()
            .map(
                |(index, ((die, choice), base_health))| p::InspectionHitDie {
                    ordinal: (index + 1) as u16,
                    source: hd_source(die.source(), rules),
                    health_seed: die.health_seed(),
                    health_die: base_health.die,
                    base_health: base_health.health,
                    training: choice.training.iter().copied().map(skill).collect(),
                    attribute: choice.attribute.map(attribute),
                    talent: choice.talent.as_ref().map(|id| talent(id, rules)),
                },
            )
            .collect(),
        sources,
        conditions: state
            .conditions()
            .instances()
            .filter(|entry| entry.reserved_by.is_none())
            .map(|entry| p::ConditionView {
                condition: condition(&entry.condition, rules),
                source: p::ActorId(entry.source.0),
                remaining_ticks: entry.remaining,
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_edge_modifier_discloses_its_defense_selector() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let authored: crate::creature_authoring::Modifier = serde_json::from_value(serde_json::json!({ "scope": "always", "op": { "kind": "add", "stat": { "kind": "incoming_edge", "defense": "physical" }, "value": -1 } })).unwrap();
        let row = modifier(&authored.compile().unwrap(), &rules);
        assert!(row.fields.iter().any(|field| field.id == "stat" && matches!(&field.value, p::DiagnosticValue::Text { value } if value == "incoming_edge")));
        assert!(row.fields.iter().any(|field| field.id == "defense" && matches!(&field.value, p::DiagnosticValue::Content { value } if value.id == "physical")));
    }
    #[test]
    fn generic_check_bonus_modifier_discloses_stat_identity_and_signed_value() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let value: Modifier = serde_json::from_value(serde_json::json!({"scope":"always","op":{
            "kind":"add","stat":{"kind":"ability","ability":"fear","stat":"check_bonus"},"value":-2
        }}))
        .unwrap();
        let row = modifier(&value, &rules);
        assert!(row.fields.iter().any(|field| field.id == "ability_stat" &&
            matches!(&field.value, p::DiagnosticValue::Content { value } if value.id == "check_bonus" && value.valid())));
        assert!(row
            .fields
            .iter()
            .any(|field| field.id == "value"
                && field.value == p::DiagnosticValue::Signed { value: -2 }));
    }

    #[test]
    fn condition_source_keeps_the_bounded_disclosed_name() {
        let mut document: s::rules::RulesetDocument =
            toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap();
        document.conditions[0].name = "x".repeat(256);
        let id = document.conditions[0].id.clone();
        let rules = Ruleset::validate(document).unwrap();
        let (_, disclosed) = source(
            &s::creatures::SourceId::Condition(id, s::ActorId(u64::MAX)),
            &rules,
        );
        assert!(disclosed.valid());
        assert_eq!(disclosed.name, "x".repeat(256));
    }
}
