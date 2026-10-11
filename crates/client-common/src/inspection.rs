//! Presentation of explicitly authorized creature reports, separate from personal stats.
use crate::stats::{binding, kind, resource, skill, subtype, talent, technique};
use tor_protocol::*;

fn attributes(value: &AttributeView) -> String {
    format!(
        "STR {} SPD {} INT {} WIL {} AWA {} PRE {}",
        value.strength,
        value.speed,
        value.intellect,
        value.willpower,
        value.awareness,
        value.presence
    )
}
fn list<T: Copy>(values: &[T], label: impl Fn(T) -> &'static str) -> String {
    if values.is_empty() {
        return "none".into();
    }
    values
        .iter()
        .map(|&value| label(value))
        .collect::<Vec<_>>()
        .join(", ")
}
fn attribute(value: InspectionAttribute) -> &'static str {
    match value {
        InspectionAttribute::Strength => "Strength",
        InspectionAttribute::Speed => "Speed",
        InspectionAttribute::Intellect => "Intellect",
        InspectionAttribute::Willpower => "Willpower",
        InspectionAttribute::Awareness => "Awareness",
        InspectionAttribute::Presence => "Presence",
    }
}
fn hd(value: HitDieSource) -> &'static str {
    match value {
        HitDieSource::Racial => "Racial",
        HitDieSource::Warrior => "Warrior",
        HitDieSource::Mage => "Mage",
    }
}
fn selector(value: InspectionSelector) -> &'static str {
    match value {
        InspectionSelector::Category { category } => match category {
            DamageType::Energy => "Energy damage",
            DamageType::Impact => "Impact damage",
            DamageType::Keen => "Keen damage",
            DamageType::Spirit => "Spirit damage",
            DamageType::Vital => "Vital damage",
        },
        InspectionSelector::Descriptor { descriptor } => match descriptor {
            InspectionDescriptor::Fire => "Fire",
            InspectionDescriptor::Cold => "Cold",
            InspectionDescriptor::Fear => "Fear",
            InspectionDescriptor::MindAffecting => "Mind-affecting",
        },
    }
}
fn source(value: &InspectionGrantSource) -> String {
    match value {
        InspectionGrantSource::Species { id } => format!("Species {id}"),
        InspectionGrantSource::Type { kind: value } => format!("Type {}", kind(*value)),
        InspectionGrantSource::Subtype { subtype: value } => format!("Subtype {}", subtype(*value)),
        InspectionGrantSource::Class { class } => format!(
            "Class {}",
            match class {
                InspectionClass::Warrior => "Warrior",
                InspectionClass::Mage => "Mage",
            }
        ),
        InspectionGrantSource::Template { id } => format!("Template {id}"),
        InspectionGrantSource::Talent { talent: value } => {
            format!("Talent {}", talent(*value).to_lowercase())
        }
    }
}
fn grant(value: InspectionGrant) -> String {
    match value {
        InspectionGrant::Health { amount } => format!("Health +{amount}"),
        InspectionGrant::Stamina { amount } => format!("Stamina +{amount}"),
        InspectionGrant::Focus { amount } => format!("Focus +{amount}"),
        InspectionGrant::Mana { amount } => format!("Mana +{amount}"),
        InspectionGrant::PhysicalDefense { modifier } => format!("Physical defense {modifier:+}"),
        InspectionGrant::MeleeFlat { modifier } => format!("Melee damage {modifier:+}"),
        InspectionGrant::MeleeDice { count } => format!("Melee dice +{count}"),
        InspectionGrant::BoltFlat { modifier } => format!("Bolt damage {modifier:+}"),
        InspectionGrant::BoltDice { count } => format!("Bolt dice +{count}"),
        InspectionGrant::FearDifficulty { modifier } => format!("Fear difficulty {modifier:+}"),
        InspectionGrant::FearDuration { ticks } => format!("Fear duration +{ticks} ticks"),
        InspectionGrant::Immunity { selector: value } => format!("Immunity: {}", selector(value)),
        InspectionGrant::Reduction {
            selector: value,
            amount,
        } => format!("Reduction: {} by {amount}", selector(value)),
        InspectionGrant::Ability { ability } => {
            format!("Ability: {}", technique(ability).to_lowercase())
        }
        InspectionGrant::Mindless => "Mindless".into(),
        InspectionGrant::Magical => "Magical".into(),
    }
}

/// Rows are shared by Text and the scrollable native inspection panel.
/// Validate before formatting so an incomplete or contradictory report is rejected.
pub fn lines(report: &CreatureInspectionView) -> Result<Vec<String>, InvalidCreatureInspection> {
    report.validate()?;
    let mut rows = vec![
        format!(
            "Creature {}: {} (faction {}; tick {})",
            report.actor.0, report.name, report.faction, report.tick
        ),
        format!(
            "Health: {}/{}; injury {}; {}",
            report.hp,
            report.max_hp,
            report.injury,
            if report.dead { "dead" } else { "alive" }
        ),
        format!(
            "Species: {} ({})",
            report.species.id,
            kind(report.species.kind)
        ),
        format!(
            "Species subtypes: {}",
            list(&report.species.subtypes, subtype)
        ),
        format!(
            "Species attributes: {}",
            attributes(&report.species.attributes)
        ),
        format!(
            "Species melee: {}; check {:+}; wind-up {}; recovery {}",
            skill(report.species.melee.skill),
            report.species.melee.bonus,
            report.species.melee.wind_up,
            report.species.melee.recovery
        ),
        format!(
            "Initial attributes: {}",
            attributes(&report.initial_attributes)
        ),
        format!(
            "Effective type: {}; subtypes: {}",
            kind(report.stats.kind),
            list(&report.stats.subtypes, subtype)
        ),
        format!(
            "Effective attributes: {}",
            attributes(&report.stats.attributes)
        ),
        format!(
            "Defenses: physical {} cognitive {} spiritual {}",
            report.stats.defenses.physical,
            report.stats.defenses.cognitive,
            report.stats.defenses.spiritual
        ),
        format!("Mana bound to {}", binding(report.stats.binding)),
    ];
    for value in &report.stats.skills {
        rows.push(format!(
            "Skill {}: {}",
            skill(value.skill).to_lowercase(),
            value.rank
        ));
    }
    for pool in &report.stats.resources {
        rows.push(format!(
            "{}: {}/{} ({} available, {} reserved)",
            resource(pool.resource),
            pool.balance,
            pool.maximum,
            pool.available,
            pool.reserved
        ));
    }
    rows.push(format!(
        "Active talents: {}",
        list(&report.stats.active_talents, talent).to_lowercase()
    ));
    rows.push(format!(
        "Dormant talents: {}",
        list(&report.stats.dormant_talents, talent).to_lowercase()
    ));
    rows.push(format!(
        "Abilities: {}",
        list(&report.stats.abilities, technique).to_lowercase()
    ));
    if report.templates.is_empty() {
        rows.push("Templates: none".into());
    }
    for component in &report.species.melee.damage.components {
        let amount = match component.amount {
            DamageAmountView::Fixed { value } => value.to_string(),
            DamageAmountView::Rolled {
                count,
                sides,
                bonus,
            } => format!("{count}d{sides}{bonus:+}"),
        };
        let descriptor = component
            .descriptor
            .map(|descriptor| selector(InspectionSelector::Descriptor { descriptor }))
            .map(|label| format!(" ({label})"))
            .unwrap_or_default();
        let primary = if component.key() == report.species.melee.damage.primary {
            " [primary]"
        } else {
            ""
        };
        rows.push(format!(
            "Natural damage: {}{} {}{}",
            selector(InspectionSelector::Category {
                category: component.category
            }),
            descriptor,
            amount,
            primary
        ));
    }
    for template in &report.templates {
        rows.push(format!(
            "Template {} (priority {}): type {}; add {}; remove {}",
            template.id,
            template.priority,
            template.kind.map(kind).unwrap_or("unchanged"),
            list(&template.add_subtypes, subtype),
            list(&template.remove_subtypes, subtype)
        ));
        rows.push(format!(
            "  Attribute adjustments: STR {:+} SPD {:+} INT {:+} WIL {:+} AWA {:+} PRE {:+}",
            template.adjustments[0],
            template.adjustments[1],
            template.adjustments[2],
            template.adjustments[3],
            template.adjustments[4],
            template.adjustments[5]
        ));
        for value in &template.overrides {
            rows.push(format!(
                "  Override {}: {}",
                attribute(value.attribute),
                value.value
            ));
        }
    }
    if report.hit_dice.is_empty() {
        rows.push("Hit dice: none".into());
    }
    for die in &report.hit_dice {
        rows.push(format!(
            "Hit die {}: {}; health d{} rolled {}; seed {}",
            die.ordinal,
            hd(die.source),
            die.health_die,
            die.base_health,
            die.health_seed
        ));
        rows.push(format!(
            "  Training: {}",
            list(&die.training, skill).to_lowercase()
        ));
        rows.push(format!(
            "  Attribute increase: {}",
            die.attribute.map(attribute).unwrap_or("none")
        ));
        rows.push(format!(
            "  Talent: {}",
            die.talent.map(talent).unwrap_or("none").to_lowercase()
        ));
    }
    for group in &report.grants {
        rows.push(format!("Grants from {}:", source(&group.source)));
        if group.grants.is_empty() {
            rows.push("  none".into());
        }
        rows.extend(
            group
                .grants
                .iter()
                .map(|value| format!("  {}", grant(*value))),
        );
    }
    if report.fear.is_empty() {
        rows.push("Fear: none".into());
    }
    for value in &report.fear {
        rows.push(format!(
            "Fear towards actor {}: {} active ticks remaining",
            value.causer.0, value.remaining_ticks
        ));
    }
    Ok(rows)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn report() -> CreatureInspectionView {
        let skills = [
            "athletics",
            "heavy_weaponry",
            "agility",
            "light_weaponry",
            "stealth",
            "thievery",
            "crafting",
            "deduction",
            "lore",
            "medicine",
            "discipline",
            "intimidation",
            "insight",
            "perception",
            "survival",
            "deception",
            "leadership",
            "persuasion",
            "spellcasting",
        ];
        let attributes = serde_json::json!({"strength":1,"speed":1,"intellect":1,"willpower":1,"awareness":1,"presence":1});
        serde_json::from_value(serde_json::json!({
        "actor":"2", "tick":"0", "name":"test mage", "faction":"blue",
        "species":{"id":"human","kind":"humanoid","subtypes":[],"attributes":attributes,"melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"damage":{"primary":{"category":"impact","descriptor":null,"sides":6},"components":[{"category":"impact","descriptor":null,"amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}]}}},
        "initial_attributes":attributes, "templates":[],
        "max_hp":7, "hp":4, "injury":3, "dead":false,
        "stats":{"kind":"humanoid","subtypes":[],"hit_dice":["mage"],"attributes":attributes,
            "skills":skills.iter().map(|skill| serde_json::json!({"skill":skill,"rank":if *skill=="spellcasting" {1} else {0}})).collect::<Vec<_>>(),
            "defenses":{"physical":2,"cognitive":2,"spiritual":2},"binding":"intellect",
            "resources":[{"resource":"stamina","balance":3,"maximum":3,"available":3,"reserved":0},{"resource":"focus","balance":3,"maximum":3,"available":3,"reserved":0},{"resource":"mana","balance":2,"maximum":2,"available":2,"reserved":0}],
            "active_talents":["magic_bolt"],"dormant_talents":[],"abilities":["basic_melee","magic_bolt"]},
        "hit_dice":[{"ordinal":1,"source":"mage","health_seed":"18446744073709551615","health_die":6,"base_health":6,"training":["spellcasting"],"attribute":null,"talent":"magic_bolt"}],
        "grants":[{"source":{"type":"species","id":"human"},"grants":[]},{"source":{"type":"class","class":"mage"},"grants":[{"type":"magical"}]},{"source":{"type":"talent","talent":"magic_bolt"},"grants":[{"type":"ability","ability":"magic_bolt"}]}],
        "fear":[]
    })).unwrap()
    }

    #[test]
    fn natural_attack_rows_show_skill_timing_primary_descriptors_and_each_amount() {
        let mut report = report();
        report.species.melee.skill = Skill::LightWeaponry;
        report.species.melee.bonus = 2;
        report.species.melee.wind_up = 90;
        report.species.melee.recovery = 70;
        report.species.melee.damage.primary = DamageKeyView {
            category: DamageType::Energy,
            descriptor: Some(InspectionDescriptor::Fire),
            sides: Some(6),
        };
        report.species.melee.damage.components = vec![
            DamageComponentView {
                category: DamageType::Energy,
                descriptor: Some(InspectionDescriptor::Fire),
                amount: DamageAmountView::Rolled {
                    count: 2,
                    sides: 6,
                    bonus: -1,
                },
            },
            DamageComponentView {
                category: DamageType::Keen,
                descriptor: None,
                amount: DamageAmountView::Fixed { value: 3 },
            },
        ];
        let text = lines(&report).unwrap().join("\n");
        assert!(text.contains("Species melee: Light weaponry; check +2; wind-up 90; recovery 70"));
        assert!(text.contains("Natural damage: Energy damage (Fire) 2d6-1 [primary]"));
        assert!(text.contains("Natural damage: Keen damage 3"));
    }

    #[test]
    fn inspection_rows_show_owned_sources_and_exact_large_seeds() {
        let report = report();
        let text = lines(&report).unwrap().join("\n");
        for expected in [
            "test mage",
            "human",
            "blue",
            "Hit die 1",
            "Mage",
            "18446744073709551615",
            "spellcasting",
            "magic bolt",
            "injury 3",
            "Magical",
            "available",
            "reserved",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        let mut invalid = report;
        invalid.hp += 1;
        assert!(lines(&invalid).is_err());
    }
}
