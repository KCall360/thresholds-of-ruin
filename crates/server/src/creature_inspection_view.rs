//! Explicit private projections of source data. No actor mutation or perception.
use super::{
    attribute, attributes, category, condition, descriptor, hd_source, hd_source_kind, label,
    named, own_stats, skill, talent,
};
use s::rules::{Modifier, ModifierOp, Ruleset, Scope, Selector, Stat};
use tor_protocol as p;
use tor_simulation as s;

fn restoration_view(build: &s::creatures::CreatureBuild) -> p::InspectionRestoration {
    if build.uses_authored_restoration() {
        p::InspectionRestoration::Authored
    } else {
        p::InspectionRestoration::ChooseAgain
    }
}

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
        damage_enchantment: value.damage_enchantment(),
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
fn skill_selector(id: &str, value: &s::rules::SkillSelector) -> p::DiagnosticField {
    match value {
        s::rules::SkillSelector::Named { skill: value } => content(id, skill(*value)),
        s::rules::SkillSelector::NaturalAttack => text(id, "Natural attack"),
        s::rules::SkillSelector::SelectedWeapon => text(id, "Selected melee"),
    }
}
fn check_view(value: &s::rules::CheckDef) -> p::CombatTraceStep {
    let mut fields = vec![
        content("actor", super::primitive(value.actor)),
        skill_selector("skill", &value.skill),
    ];
    match &value.against {
        s::rules::Against::Defense { defense } => {
            fields.push(content("defense", super::primitive(*defense)))
        }
        s::rules::Against::Difficulty { formula } => {
            fields.push(signed("difficulty_base", i64::from(formula.base)));
            if let Some(skill) = &formula.skill {
                fields.push(skill_selector("difficulty_skill", skill));
            }
            if let Some(value) = formula.attribute {
                fields.push(content("difficulty_attribute", attribute(value)));
            }
        }
        s::rules::Against::Opposed { skill } => fields.push(skill_selector("opposed_skill", skill)),
    }
    p::CombatTraceStep {
        id: "primary_check".into(),
        name: "Primary check".into(),
        fields,
    }
}
fn rider_check_view(value: &s::rules::RiderCheckDef) -> p::CombatTraceStep {
    p::CombatTraceStep {
        id: "rider_check".into(),
        name: "Rider check".into(),
        fields: vec![
            content("actor", super::primitive(s::rules::CheckActor::User)),
            skill_selector("skill", &value.skill),
            content("defense", super::primitive(value.defense)),
        ],
    }
}
fn amount_text(value: &s::rules::AmountExpr) -> String {
    match value {
        s::rules::AmountExpr::Fixed { amount } => amount.to_string(),
        s::rules::AmountExpr::Rolled { dice } => {
            format!("{}d{}{:+}", dice.count, dice.sides, dice.bonus)
        }
    }
}
fn effect_view(value: &s::rules::Effect, rules: &Ruleset) -> p::CombatTraceStep {
    let (id, fields) = match value {
        s::rules::Effect::Theft {
            selection,
            relocation,
        } => {
            let (selection, amount) = match selection {
                s::theft::TheftSelection::MagicalUnit => {
                    ("unequipped magical unit", "1".to_owned())
                }
                s::theft::TheftSelection::Currency { currency, amount } => {
                    let currency = match currency {
                        s::CurrencyKind::Gold => "carried gold",
                    };
                    let amount = match amount {
                        s::quantity::QuantityExpr::Fixed { quantity } => quantity.to_string(),
                        s::quantity::QuantityExpr::Uniform { minimum, maximum } => {
                            format!("{minimum}..={maximum}")
                        }
                        s::quantity::QuantityExpr::DepthScaledUniform {
                            minimum,
                            maximum_base,
                            per_depth,
                        } => format!("{minimum}..=({maximum_base}+{per_depth}*floor depth)"),
                    };
                    (currency, amount)
                }
            };
            (
                "theft",
                vec![
                    text("selection", selection),
                    text("amount", amount),
                    text(
                        "relocation",
                        relocation.as_ref().map_or_else(
                            || "None".into(),
                            |gate| {
                                format!("Same floor after actual transfer; requires {}", gate.flag)
                            },
                        ),
                    ),
                ],
            )
        }
        s::rules::Effect::AlterEquipment {
            selection,
            alteration,
        } => (
            "alter_equipment",
            vec![
                text("selection", format!("{selection:?}")),
                text("alteration", format!("{alteration:?}")),
            ],
        ),
        s::rules::Effect::CorrodeArmor { slot, amount } => (
            "corrode_armor",
            vec![
                text("slot", slot.to_string()),
                unsigned("amount", u64::from(*amount)),
            ],
        ),
        s::rules::Effect::ApplyLinkedHold { policy } => (
            "apply_linked_hold",
            vec![
                text("sustaining", policy.sustaining.to_string()),
                unsigned("preparation", policy.preparation),
                unsigned("recovery", policy.recovery),
                unsigned("protection", policy.protection),
            ],
        ),
        s::rules::Effect::HoldDamage {
            category,
            descriptor,
        } => (
            "hold_damage",
            vec![
                content("category", super::primitive(*category)),
                text(
                    "descriptor",
                    descriptor
                        .as_ref()
                        .map_or_else(|| "None".into(), ToString::to_string),
                ),
            ],
        ),
        s::rules::Effect::Damage { damage } => {
            let fields = match damage {
                s::rules::DamageExpr::SelectedWeapon { impact_bonus } => vec![
                    text("source", "Selected melee primary damage"),
                    signed("impact_bonus", i64::from(*impact_bonus)),
                ],
                s::rules::DamageExpr::Components { components } => components
                    .iter()
                    .enumerate()
                    .map(|(ordinal, component)| {
                        text(
                            &format!(
                                "{}_component_{ordinal}",
                                if ordinal == 0 {
                                    "primary"
                                } else {
                                    "additional"
                                }
                            ),
                            format!(
                                "{}{}: {}",
                                super::primitive(component.category).name,
                                component
                                    .descriptor
                                    .as_ref()
                                    .map_or_else(String::new, |id| format!(" ({id})")),
                                amount_text(&component.amount)
                            ),
                        )
                    })
                    .collect(),
            };
            ("damage", fields)
        }
        s::rules::Effect::Heal { amount } => ("heal", vec![text("amount", amount_text(amount))]),
        s::rules::Effect::DrainLatestHitDie => ("drain_latest_hit_die", vec![]),
        s::rules::Effect::RestoreHitDie => ("restore_hit_die", vec![]),
        s::rules::Effect::GainStrength => ("gain_strength", vec![]),
        s::rules::Effect::RaiseLevel => ("raise_level", vec![]),
        s::rules::Effect::RoomLight => ("room_light", vec![]),
        s::rules::Effect::DetectMonsters { condition: id } => (
            "detect_monsters",
            vec![content("condition", condition(id, rules))],
        ),
        s::rules::Effect::DetectFood => ("detect_food", vec![]),
        s::rules::Effect::DetectMagic => ("detect_magic", vec![]),
        s::rules::Effect::MapTopology => ("map_topology", vec![]),
        s::rules::Effect::DrainLife => ("drain_life", vec![]),
        s::rules::Effect::BalefulPolymorph { forms } => (
            "baleful_polymorph",
            vec![unsigned("forms_count", forms.len() as u64)],
        ),
        s::rules::Effect::CancelSpecials => ("cancel_specials", vec![]),
        s::rules::Effect::StepSelfOneCell => ("step_self_one_cell", vec![]),
        s::rules::Effect::TeleportRandom => ("teleport_random", vec![]),
        s::rules::Effect::TeleportToRayOrigin => ("teleport_to_ray_origin", vec![]),
        s::rules::Effect::ArrestDescent => ("arrest_descent", vec![]),
        s::rules::Effect::PushTargetOneCell { direction } => (
            "push_target_one_cell",
            vec![text(
                "direction",
                match direction {
                    s::abilities::PushDirection::AwayFromSource => "away_from_source",
                },
            )],
        ),
        s::rules::Effect::LoseAttribute { attribute, amount } => (
            "lose_attribute",
            vec![
                content("attribute", super::attribute(*attribute)),
                text("amount", amount_text(amount)),
            ],
        ),
        s::rules::Effect::RestoreAttribute { attribute, amount } => (
            "restore_attribute",
            vec![
                content("attribute", super::attribute(*attribute)),
                text("amount", amount_text(amount)),
            ],
        ),
        s::rules::Effect::LoseMaximumHealth { amount } => (
            "lose_maximum_health",
            vec![text("amount", amount_text(amount))],
        ),
        s::rules::Effect::RestoreMaximumHealth { amount } => (
            "restore_maximum_health",
            vec![text("amount", amount_text(amount))],
        ),
        s::rules::Effect::Nourish { amount } => {
            ("nourish", vec![text("amount", amount_text(amount))])
        }
        s::rules::Effect::SearchAdjacentGuises => ("search_adjacent_guises", vec![]),
        s::rules::Effect::ObservePublicPlan => ("observe_public_plan", vec![]),
        s::rules::Effect::ApplyQualifiedConditionToSource {
            condition: id,
            duration,
            ability_tag,
        } => (
            "apply_qualified_condition_to_source",
            vec![
                content("condition", condition(id, rules)),
                field(
                    "duration",
                    p::DiagnosticValue::Unsigned { value: *duration },
                ),
                text("ability_tag", ability_tag.as_str()),
            ],
        ),
        s::rules::Effect::ApplyConditionForTurns {
            condition: id,
            duration,
        } => (
            "apply_condition_for_turns",
            vec![
                content("condition", condition(id, rules)),
                text("duration_turns", amount_text(duration)),
            ],
        ),
        s::rules::Effect::HasteDose {
            condition: id,
            rest,
        } => (
            "haste_dose",
            vec![
                content("condition", condition(id, rules)),
                content("rest", condition(rest, rules)),
            ],
        ),
        s::rules::Effect::ApplyPersistentCondition {
            condition: id,
            cancels,
        } => {
            let mut fields = vec![content("condition", condition(id, rules))];
            if let Some(opposite) = cancels {
                fields.push(content("cancels", condition(opposite, rules)));
            }
            ("apply_persistent_condition", fields)
        }
        s::rules::Effect::RemoveCondition { condition: id } => (
            "remove_condition",
            vec![content("condition", condition(id, rules))],
        ),
        s::rules::Effect::HealPerHitDie {
            sides,
            bonus,
            overflow,
        } => (
            "heal_per_hit_die",
            vec![
                unsigned("sides", u64::from(*sides)),
                signed("bonus", i64::from(*bonus)),
                text(
                    "overflow",
                    match overflow {
                        s::rules::HealingOverflow::None => "none",
                        s::rules::HealingOverflow::ImproveEarliestHitDie => {
                            "improve_earliest_hit_die"
                        }
                        s::rules::HealingOverflow::ImproveEarliestHitDiceByExcess => {
                            "improve_earliest_hit_dice_by_excess"
                        }
                    },
                ),
            ],
        ),
        s::rules::Effect::ApplyCondition {
            condition: id,
            duration,
        } => (
            "apply_condition",
            vec![
                content("condition", condition(id, rules)),
                field(
                    "duration",
                    p::DiagnosticValue::Unsigned { value: *duration },
                ),
            ],
        ),
        s::rules::Effect::Restore { resource, amount } => (
            "restore",
            vec![
                content("resource", super::resource(*resource)),
                text("amount", amount_text(amount)),
            ],
        ),
    };
    p::CombatTraceStep {
        id: id.into(),
        name: label(id),
        fields,
    }
}
fn natural_attack_view(
    value: &s::attacks::NaturalAttack,
    source: p::ContentRef,
    _rules: &Ruleset,
) -> p::NaturalAttackView {
    p::NaturalAttackView {
        source,
        skill: skill(value.skill()),
        bonus: value.bonus(),
        wind_up: value.wind_up() as u32,
        recovery: value.recovery() as u32,
        strike_count: value.strikes().len() as u16,
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
fn unsigned(id: &str, value: u64) -> p::DiagnosticField {
    field(id, p::DiagnosticValue::Unsigned { value })
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
        Stat::AttackDamageRollBonus => vec![text("stat", "attack_damage_roll_bonus")],
        Stat::NutritionRateThirtieths => vec![text("stat", "nutrition_rate_thirtieths")],
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
            Scope::WhileNoSensedHostile => "while_no_sensed_hostile",
        },
    )];
    let id = match &value.op {
        ModifierOp::SignedReduction {
            selector: value,
            amount,
        } => {
            fields.push(selector(value, rules));
            fields.push(signed("amount", i64::from(*amount)));
            "signed_reduction"
        }
        ModifierOp::AttributeWithoutDefense {
            attribute,
            defense,
            value,
        } => {
            fields.push(content("attribute", super::primitive(*attribute)));
            fields.push(content("excluded_defense", super::primitive(*defense)));
            fields.push(signed("value", *value));
            "attribute_without_defense"
        }
        ModifierOp::SustainAttribute { attribute } => {
            fields.push(content("attribute", super::primitive(*attribute)));
            "sustain_attribute"
        }
        ModifierOp::ProtectArmor {} => "protect_armor",
        ModifierOp::HealthRecoveryRate { multiplier } => {
            fields.push(signed("multiplier", i64::from(*multiplier)));
            "health_recovery_rate"
        }
        ModifierOp::ActionDuration { value } => {
            fields.push(text(
                "rate",
                match value {
                    s::action_duration::ActionDuration::Normal => "normal",
                    s::action_duration::ActionDuration::Haste => "haste",
                    s::action_duration::ActionDuration::Slow => "slow",
                },
            ));
            "action_duration"
        }
        ModifierOp::Perception { value } => {
            use s::perception::PerceptionValue;
            let (name, enabled) = match value {
                PerceptionValue::Invisible { enabled } => ("invisible", *enabled),
                PerceptionValue::SeeInvisible { enabled } => ("see_invisible", *enabled),
                PerceptionValue::Sight { enabled } => ("sight", *enabled),
                PerceptionValue::ObjectGuise {
                    appearance,
                    skill,
                    edge,
                } => {
                    if let Some(appearance) = appearance {
                        fields.push(text("appearance", &appearance.name));
                    }
                    fields.push(text("skill", skill.as_str()));
                    fields.push(signed("edge", *edge));
                    ("object_guise", appearance.is_some())
                }
                PerceptionValue::AmbushFromGuise { enabled } => ("ambush_from_guise", *enabled),
                PerceptionValue::ReadiedTouchAmbush { enabled } => {
                    ("readied_touch_ambush", *enabled)
                }
                PerceptionValue::ContinualStealth { edge } => {
                    fields.push(unsigned("edge", u64::from(*edge)));
                    ("continual_stealth", true)
                }
                PerceptionValue::Aggravation {
                    interval,
                    chance_denominator,
                    stealth_disadvantage,
                } => {
                    fields.push(unsigned("interval", *interval));
                    fields.push(unsigned(
                        "chance_denominator",
                        u64::from(*chance_denominator),
                    ));
                    fields.push(unsigned(
                        "stealth_disadvantage",
                        u64::from(*stealth_disadvantage),
                    ));
                    ("aggravation", true)
                }
                PerceptionValue::PeriodicSearch { interval } => {
                    fields.push(unsigned("interval", *interval));
                    ("periodic_search", true)
                }
            };
            fields.push(text("capability", name));
            fields.push(text("enabled", if enabled { "true" } else { "false" }));
            "perception"
        }
        ModifierOp::Locomotion { value } => {
            use s::locomotion::LocomotionValue;
            match value {
                LocomotionValue::Stationary { enabled }
                | LocomotionValue::Flying { enabled }
                | LocomotionValue::Levitating { enabled } => {
                    fields.push(text(
                        "capability",
                        if matches!(value, LocomotionValue::Stationary { .. }) {
                            "stationary"
                        } else if matches!(value, LocomotionValue::Levitating { .. }) {
                            "levitating"
                        } else {
                            "flying"
                        },
                    ));
                    fields.push(text("enabled", if *enabled { "true" } else { "false" }));
                }
                LocomotionValue::Erratic { chance } | LocomotionValue::Confused { chance } => {
                    fields.push(text(
                        "capability",
                        if matches!(value, LocomotionValue::Confused { .. }) {
                            "confused"
                        } else {
                            "erratic"
                        },
                    ));
                    if let Some(chance) = chance {
                        fields.push(signed("numerator", i64::from(chance.numerator)));
                        fields.push(signed("denominator", i64::from(chance.denominator)));
                    } else {
                        fields.push(text("enabled", "false"));
                    }
                }
            }
            "locomotion"
        }
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
        ModifierOp::TalentReduction { category, amount } => {
            fields.push(selector(
                &s::rules::Selector::Category {
                    category: *category,
                },
                rules,
            ));
            fields.push(signed("amount", i64::from(*amount)));
            "talent_reduction"
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
        S::Condition(id, _) | S::QualifiedCondition { condition: id, .. } => {
            ("condition", condition(id, rules))
        }
    };
    (kind.into(), value)
}
pub(crate) fn inspect(
    game: &s::Game,
    actor: s::ActorId,
    revision: u64,
) -> Option<p::CreatureInspectionView> {
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
        source: selected_source.clone(),
        socket: selected.socket.map(|socket| socket.0),
        attack: match &selected.attack {
            s::attacks::SelectedMeleeAttack::Weapon(attack) => p::SelectedMeleeView::Weapon {
                attack: attack_view(attack, Some(rules)),
            },
            s::attacks::SelectedMeleeAttack::Natural(attack) => p::SelectedMeleeView::Natural {
                attack: natural_attack_view(attack, selected_source.clone(), rules),
            },
        },
        attribute: attribute(selected.attribute),
        attribute_value: selected.attribute_value,
        rank: selected.rank,
        check_bonus: selected.check_bonus,
        abilities: derived
            .ability_timings
            .iter()
            .filter(|(id, _)| {
                !matches!(
                    rules.ability(id).expect("validated ability").strikes,
                    s::rules::StrikeSource::Projectile { .. }
                )
            })
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
        revision,
        tick: game.tick(),
        name: view.identity.name,
        faction: view.identity.faction,
        species: named(build.species_id(), None),
        starting_skills: build.starting_skills().map(skill),
        restoration: restoration_view(build),
        dormant_talents: derived
            .dormant_reasons
            .iter()
            .map(|(id, reasons)| p::InspectionDormantTalent {
                talent: talent(id, rules),
                reasons: reasons
                    .iter()
                    .map(|reason| match reason {
                        s::creatures::DormantReason::InactiveParent(id) => {
                            format!("Requires active parent {id}")
                        }
                        s::creatures::DormantReason::Requirement => {
                            let requirement =
                                &rules.talent(id).expect("validated talent").requirement;
                            format!("Unmet requirement: {}", requirement_text(requirement))
                        }
                    })
                    .collect(),
            })
            .collect(),
        natural_attack: natural_attack_view(
            &build.species().melee,
            named(build.species_id(), None),
            rules,
        ),
        ability_strikes: derived
            .ability_timings
            .keys()
            .map(|id| {
                let definition = rules.ability(id).expect("derived ability is validated");
                p::AbilityStrikesView {
                    ability: id.to_string(),
                    source: match &definition.strikes {
                        s::rules::StrikeSource::Authored { strikes } => {
                            p::StrikeSourceInspectionView::Authored {
                                strike_count: strikes.len() as u16,
                            }
                        }
                        s::rules::StrikeSource::Projectile { delivery } => {
                            p::StrikeSourceInspectionView::Projectile {
                                delivery: match delivery {
                                    s::projectile_profiles::ProjectileDeliveryKind::Throw => {
                                        p::ProjectileDeliveryKind::Throw
                                    }
                                    s::projectile_profiles::ProjectileDeliveryKind::Launch => {
                                        p::ProjectileDeliveryKind::Launch
                                    }
                                },
                            }
                        }
                        s::rules::StrikeSource::SelectedMelee => {
                            p::StrikeSourceInspectionView::SelectedMelee {}
                        }
                        s::rules::StrikeSource::NaturalMelee { additional_riders } => {
                            p::StrikeSourceInspectionView::NaturalMelee {
                                strike_count: build.species().melee.strikes().len() as u16,
                                additional_riders: additional_riders.len() as u16,
                            }
                        }
                    },
                }
            })
            .collect(),
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
            .zip(derived.health_contributions.iter())
            .enumerate()
            .map(
                |(index, ((die, choice), base_health))| p::InspectionHitDie {
                    ordinal: s::HdIndex::from_usize(index)
                        .and_then(s::HdIndex::ordinal)
                        .expect("resident HD ordinal")
                        .get(),
                    source_kind: hd_source_kind(&die.source()),
                    source: hd_source(die.source(), rules),
                    health_seed: die.health_seed(),
                    health_die: base_health.die,
                    base_health: base_health.health,
                    point_pools: build
                        .point_balances(s::HdIndex::from_usize(index).expect("resident HD index"))
                        .expect("validated point ledger")
                        .into_iter()
                        .map(|balance| {
                            let definition = rules
                                .point_pool(&balance.pool)
                                .expect("validated point pool");
                            p::InspectionPointPool {
                                pool: named(&definition.id, Some(&definition.name)),
                                granted: balance.granted,
                                spent: balance.spent,
                                allowed: definition
                                    .allowed
                                    .iter()
                                    .map(|kind| match kind {
                                        s::rules::PurchaseKind::SkillRank => {
                                            p::AdvancementKind::SkillRank
                                        }
                                        s::rules::PurchaseKind::Expertise => {
                                            p::AdvancementKind::Expertise
                                        }
                                        s::rules::PurchaseKind::Talent => {
                                            p::AdvancementKind::Talent
                                        }
                                    })
                                    .collect(),
                            }
                        })
                        .collect(),
                    purchases: choice
                        .purchases
                        .iter()
                        .map(|purchase| {
                            let definition = rules
                                .point_pool(&purchase.pool)
                                .expect("validated point pool");
                            p::InspectionPurchase {
                                pool: named(&definition.id, Some(&definition.name)),
                                selection: match &purchase.selection {
                                    s::creatures::PurchaseSelection::SkillRank(value) => {
                                        p::InspectionPurchaseSelection::SkillRank {
                                            skill: skill(*value),
                                        }
                                    }
                                    s::creatures::PurchaseSelection::Expertise(id) => {
                                        p::InspectionPurchaseSelection::Expertise {
                                            expertise: named(
                                                id,
                                                rules.expertise(id).map(|def| def.name.as_str()),
                                            ),
                                        }
                                    }
                                    s::creatures::PurchaseSelection::Talent(id) => {
                                        p::InspectionPurchaseSelection::Talent {
                                            talent: talent(id, rules),
                                        }
                                    }
                                    s::creatures::PurchaseSelection::TalentForm {
                                        talent: id,
                                        form,
                                    } => {
                                        let definition =
                                            rules.form(form).expect("validated form choice");
                                        p::InspectionPurchaseSelection::TalentForm {
                                            talent: talent(id, rules),
                                            form: named(form, Some(&definition.name)),
                                        }
                                    }
                                },
                            }
                        })
                        .collect(),
                    attribute: choice.attribute.map(attribute),
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
                phase: match entry.phase {
                    tor_simulation::conditions::ConditionPhase::Active => p::ConditionPhase::Active,
                    tor_simulation::conditions::ConditionPhase::Protection => {
                        p::ConditionPhase::Protection
                    }
                },
                source: p::ActorId(entry.source.0),
                remaining_ticks: entry.remaining,
            })
            .collect(),
    })
}

const REQUIREMENT_SUMMARY_BYTES: usize = 4096;
const OMITTED_REQUIREMENT_DETAIL: &str = " [additional requirement detail omitted]";

/// Stop traversing as soon as a bounded disclosure summary is full. A valid
/// requirement can contain thousands of IDs; inspection must not expand all of
/// those IDs into an unbounded intermediate string merely to truncate it later.
struct RequirementSummary(String);
impl std::fmt::Write for RequirementSummary {
    fn write_str(&mut self, text: &str) -> std::fmt::Result {
        let remaining = REQUIREMENT_SUMMARY_BYTES - OMITTED_REQUIREMENT_DETAIL.len() - self.0.len();
        if text.len() <= remaining {
            self.0.push_str(text);
            return Ok(());
        }
        let mut end = remaining;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        self.0.push_str(&text[..end]);
        Err(std::fmt::Error)
    }
}
fn requirement_text(requirement: &s::rules::Requirement) -> String {
    let mut summary = RequirementSummary(String::new());
    if write_requirement(&mut summary, requirement).is_err() {
        summary.0.push_str(OMITTED_REQUIREMENT_DETAIL);
    }
    summary.0
}
fn write_requirement(
    output: &mut impl std::fmt::Write,
    requirement: &s::rules::Requirement,
) -> std::fmt::Result {
    use s::rules::{Requirement as R, SkillSelector};
    match requirement {
        R::All { requirements } | R::Any { requirements } => {
            output.write_str(if matches!(requirement, R::All { .. }) {
                "all ("
            } else {
                "any ("
            })?;
            for (index, child) in requirements.iter().enumerate() {
                if index > 0 {
                    output.write_str("; ")?;
                }
                write_requirement(output, child)?;
            }
            output.write_str(")")
        }
        R::TotalHd { minimum } => write!(output, "total HD at least {minimum}"),
        R::RacialHd { minimum } => write!(output, "racial HD at least {minimum}"),
        R::ClassLevel { class, minimum } => write!(output, "{class} HD at least {minimum}"),
        R::TypeIn { types } => {
            output.write_str("type in ")?;
            write_requirement_ids(output, types)
        }
        R::SubtypeIn { subtypes } => {
            output.write_str("subtype in ")?;
            write_requirement_ids(output, subtypes)
        }
        R::OwnedTalent { talent } => write!(output, "active owned talent {talent}"),
        R::SpeciesFlag { flag, present } => write!(output, "species flag {flag} = {present}"),
        R::Flag { flag, present } => write!(output, "permanent flag {flag} = {present}"),
        R::SkillRank {
            skill: selector,
            minimum,
        } => {
            let name = match selector {
                SkillSelector::Named { skill } => skill.as_str(),
                SkillSelector::NaturalAttack => "natural attack skill",
                SkillSelector::SelectedWeapon => "selected weapon skill",
            };
            write!(output, "{name} rank at least {minimum}")
        }
        R::AttributeMinimum {
            attribute: value,
            minimum,
        } => write!(output, "{} at least {minimum}", attribute(*value).name),
    }
}
fn write_requirement_ids(
    output: &mut impl std::fmt::Write,
    values: &[impl std::fmt::Display],
) -> std::fmt::Result {
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            output.write_str(", ")?;
        }
        write!(output, "{value}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_inspection_reports_declared_personal_policy_not_normalized_record_policy() {
        use s::creatures::{BodyPackage, CreatureBuild, PlannedAdvancement, RestorationPolicy};
        // A valid public authoring fixture: one replaceable racial body row
        // followed by one personally owned class row. No Engine internals.
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let catalog: crate::creature_authoring::Catalog = serde_json::from_value(
            serde_json::json!({"species":{"inspection_subject":{
                "kind":"humanoid",
                "behavior":{"disposition":"mean","idle":"explore","memory_ticks":1000},
                "attributes":{"strength":1,"speed":1,"intellect":1,"willpower":1,"awareness":1,"presence":1},
                "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,
                    "strikes":[{"offset":0,"check":{"actor":"user",
                        "skill":{"kind":"natural_attack"},
                        "against":{"kind":"defense","defense":"physical"}},
                        "on_success":[{"kind":"damage","damage":{"kind":"components",
                            "components":[{"category":"impact","amount":{"kind":"fixed","amount":1}}]}}],
                        "on_failure":[],"descriptors":[],"riders":[]}]},
                "modifiers":[]
            }}}),
        ).unwrap();
        let spec: crate::creature_authoring::BuildSpec = serde_json::from_value(
            serde_json::json!({"species":"inspection_subject","name":"subject","faction":"fixture",
                "binding":"intellect","starting_skills":["heavy_weaponry","discipline"],
                "restoration":"authored","nutrition":{"kind":"exempt"},
                "hit_dice":[{"source":"racial","purchases":[]},{"source":"warrior","purchases":[]}]}),
        ).unwrap();
        let catalog = catalog.compile(rules).unwrap();
        let original = catalog.prepare(&spec).unwrap().instantiate(73).unwrap();
        assert!(original.uses_authored_restoration());
        let package = std::sync::Arc::new(BodyPackage {
            id: s::content_ids::BodyFormId::new("inspection_body").unwrap(),
            species: original.species_id().clone(),
            body: s::BodySpec::default(),
            advancement: original
                .ledger()
                .entries()
                .iter()
                .zip(original.choices())
                .take(1)
                .map(|(die, choices)| PlannedAdvancement {
                    source: die.source(),
                    choices: choices.clone(),
                })
                .collect(),
        });
        for authored in [false, true] {
            let mut advancement = original.advancement().clone();
            if !authored {
                advancement.restoration = RestorationPolicy::ChooseAgain;
            }
            let raw = CreatureBuild::from_recorded(
                original.content().clone(),
                original.species_id().clone(),
                original.initial_attributes(),
                original.ledger().clone(),
                original.binding(),
                advancement,
                vec![],
            )
            .unwrap();
            let build = raw
                .with_body_package(package.clone(), raw.personal_health_root_seed().unwrap())
                .unwrap();
            assert_eq!(build.personal_hd_count().get(), 1);
            assert_eq!(build.ledger().total_hd().get(), 2);
            assert_eq!(build.restoration_policy(), &RestorationPolicy::ChooseAgain);
            assert_eq!(
                restoration_view(&build),
                if authored {
                    p::InspectionRestoration::Authored
                } else {
                    p::InspectionRestoration::ChooseAgain
                }
            );
        }
    }

    #[test]
    fn action_duration_modifier_discloses_typed_rate_without_attribute_changes() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        for rate in ["normal", "haste", "slow"] {
            let authored: crate::creature_authoring::Modifier = serde_json::from_value(
                serde_json::json!({"scope":"always","op":{"kind":"action_duration","value":rate}}),
            )
            .unwrap();
            let row = modifier(&authored.compile().unwrap(), &rules);
            assert_eq!(row.id, "action_duration");
            assert_eq!(row.fields.len(), 2);
            assert!(row.fields.iter().any(|field| field.id == "rate"
                && matches!(
                    &field.value, p::DiagnosticValue::Text { value } if value == rate
                )));
        }
    }

    #[test]
    fn large_polymorph_roster_inspection_is_bounded_and_does_not_disclose_ids() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        let effect = s::rules::Effect::BalefulPolymorph {
            forms: (0..4096)
                .map(|index| {
                    s::content_ids::BodyFormId::new(format!("private_body_{index}")).unwrap()
                })
                .collect(),
        };
        assert!(effect.valid_shape());
        let view = effect_view(&effect, &rules);
        p::CombatTraceView {
            steps: vec![view.clone()],
            truncated: false,
        }
        .validate()
        .unwrap();
        assert_eq!(view.fields, vec![unsigned("forms_count", 4096)]);
        assert!(!serde_json::to_string(&view)
            .unwrap()
            .contains("private_body"));
    }

    #[test]
    fn hit_die_effect_inspection_exposes_no_source_seed_or_authored_operands() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        for id in ["drain_latest_hit_die", "restore_hit_die"] {
            let effect: s::rules::Effect =
                serde_json::from_value(serde_json::json!({"kind":id})).unwrap();
            let row = effect_view(&effect, &rules);
            assert_eq!(row.id, id);
            assert!(row.fields.is_empty());
        }
    }

    #[test]
    fn restorable_effect_inspection_preserves_attribute_and_authored_amount() {
        let rules =
            Ruleset::validate(toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap()).unwrap();
        for id in [
            "lose_attribute",
            "restore_attribute",
            "lose_maximum_health",
            "restore_maximum_health",
        ] {
            let mut value = serde_json::json!({"kind":id,"amount":{"kind":"rolled","dice":{"count":1,"sides":3,"bonus":0}}});
            if id.ends_with("attribute") {
                value["attribute"] = "strength".into();
            }
            let effect: s::rules::Effect = serde_json::from_value(value).unwrap();
            let row = effect_view(&effect, &rules);
            assert_eq!(row.id, id);
            assert!(row.fields.iter().any(|field| field.id == "amount"
                && matches!(
                    &field.value, p::DiagnosticValue::Text { value } if value.contains("1d3")
                )));
            assert_eq!(
                row.fields.len(),
                if id.ends_with("attribute") { 2 } else { 1 }
            );
        }
    }

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
            "kind":"add","stat":{"kind":"ability","ability":"falter","stat":"check_bonus"},"value":-2
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
    fn large_requirement_disclosure_is_explicitly_bounded() {
        let requirement = s::rules::Requirement::SubtypeIn {
            subtypes: (0..4096)
                .map(|index| s::content_ids::SubtypeId::new(format!("subtype_{index}")).unwrap())
                .collect(),
        };
        let text = requirement_text(&requirement);
        assert!(
            text.len() <= 4096,
            "requirement disclosure must fit its bounded summary"
        );
        assert!(text.ends_with("[additional requirement detail omitted]"));
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

#[path = "strike_inspection_page.rs"]
pub(crate) mod pages;
