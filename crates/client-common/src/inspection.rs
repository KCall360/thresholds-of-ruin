//! Privileged inspection renders the captured catalog sources, without re-derivation.
use tor_protocol::*;
pub fn lines(report: &CreatureInspectionView) -> Result<Vec<String>, InvalidCreatureInspection> {
    report.validate()?;
    let mut rows = vec![
        format!(
            "{}: actor {}, faction {}; species {} ({})",
            report.name, report.actor.0, report.faction, report.species.name, report.species.id
        ),
        format!(
            "Health: {}/{}; injury {}; dead {}",
            report.hp, report.max_hp, report.injury, report.dead
        ),
    ];
    let a = &report.initial_attributes;
    rows.push(format!(
        "Initial attributes: STR {} SPD {} INT {} WIL {} AWA {} PRE {}",
        a.strength, a.speed, a.intellect, a.willpower, a.awareness, a.presence
    ));
    let attack = &report.natural_attack;
    rows.push(format!(
        "Species melee: {}; check {:+}; wind-up {}; recovery {}",
        attack.skill.name, attack.bonus, attack.wind_up, attack.recovery
    ));
    for component in &attack.damage.components {
        let amount = match component.amount {
            DamageAmountView::Fixed { value } => value.to_string(),
            DamageAmountView::Rolled {
                count,
                sides,
                bonus,
            } => format!("{count}d{sides}{bonus:+}"),
        };
        rows.push(format!(
            "Natural damage: {:?}{} {amount}{}",
            component.category,
            component
                .descriptor
                .as_ref()
                .map_or_else(String::new, |d| format!(" ({})", d.name)),
            if component.key() == attack.damage.primary {
                " [primary]"
            } else {
                ""
            }
        ));
    }
    for template in &report.templates {
        rows.push(format!("Template: {} ({})", template.name, template.id));
    }
    for die in &report.hit_dice {
        rows.push(format!(
            "Hit die {}: {}; health d{} rolled {}; seed {}",
            die.ordinal, die.source.name, die.health_die, die.base_health, die.health_seed
        ));
        rows.push(format!(
            "  Training: {}",
            die.training
                .iter()
                .map(|x| x.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        rows.push(format!(
            "  Attribute: {}; talent: {}",
            die.attribute.as_ref().map_or("none", |x| x.name.as_str()),
            die.talent.as_ref().map_or("none", |x| x.name.as_str())
        ));
    }
    let a = &report.stats.attributes;
    rows.push(format!(
        "Attributes: STR {} SPD {} INT {} WIL {} AWA {} PRE {}",
        a.strength, a.speed, a.intellect, a.willpower, a.awareness, a.presence
    ));
    let d = &report.stats.defenses;
    rows.push(format!(
        "Defenses: Physical {} Cognitive {} Spiritual {}",
        d.physical, d.cognitive, d.spiritual
    ));
    rows.push(format!("Mana binding: {}", report.stats.binding.name));
    for skill in &report.stats.skills {
        rows.push(format!("Skill {}: {}", skill.skill.name, skill.rank));
    }
    for talent in &report.stats.active_talents {
        rows.push(format!("Active talent: {}", talent.name));
    }
    for talent in &report.stats.dormant_talents {
        rows.push(format!("Dormant talent: {}", talent.name));
    }
    for ability in &report.stats.abilities {
        rows.push(format!("Ability: {} ({})", ability.name, ability.id));
    }
    for pool in &report.stats.resources {
        rows.push(format!(
            "{}: {}/{}; available {}, reserved {}",
            pool.resource.name, pool.balance, pool.maximum, pool.available, pool.reserved
        ));
    }
    for source in &report.sources {
        rows.push(format!(
            "{}: {} ({})",
            source.category, source.source.name, source.source.id
        ));
        if let Some(owner) = source.owner {
            rows.push(format!("  Source actor: {}", owner.0));
        }
        for step in &source.rows {
            rows.extend(crate::combat_diagnostics::trace_lines(step));
        }
    }
    for condition in &report.conditions {
        rows.push(format!(
            "{} from actor {}: {} active ticks remaining",
            condition.condition.name, condition.source.0, condition.remaining_ticks
        ));
    }
    Ok(rows)
}
#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub(crate) fn reference(id: &str, name: &str) -> ContentRef {
        ContentRef {
            id: id.into(),
            name: name.into(),
        }
    }
    pub(crate) fn report() -> CreatureInspectionView {
        let a = AttributeView {
            strength: 1,
            speed: 1,
            intellect: 1,
            willpower: 1,
            awareness: 1,
            presence: 1,
        };
        let component = DamageComponentView {
            category: DamageType::Impact,
            descriptor: None,
            amount: DamageAmountView::Rolled {
                count: 1,
                sides: 6,
                bonus: 0,
            },
        };
        CreatureInspectionView {
            actor: ActorId(2),
            tick: 0,
            name: "test mage".into(),
            faction: "blue".into(),
            species: reference("human", "Human"),
            natural_attack: AttackView {
                skill: reference("heavy_weaponry", "Heavy weaponry"),
                bonus: 0,
                wind_up: 60,
                recovery: 40,
                damage: DamageView {
                    primary: component.key(),
                    components: vec![component],
                },
            },
            initial_attributes: a,
            templates: vec![],
            max_hp: 7,
            hp: 4,
            injury: 3,
            dead: false,
            stats: OwnStats {
                kind: reference("humanoid", "Humanoid"),
                subtypes: vec![],
                hit_dice: vec![reference("mage", "Mage")],
                attributes: a,
                skills: vec![],
                defenses: DefenseView {
                    physical: 12,
                    cognitive: 12,
                    spiritual: 12,
                },
                binding: reference("intellect", "Intellect"),
                resources: vec![],
                active_talents: vec![],
                dormant_talents: vec![],
                abilities: vec![],
            },
            hit_dice: vec![InspectionHitDie {
                ordinal: 1,
                source: reference("mage", "Mage"),
                health_seed: u64::MAX,
                health_die: 6,
                base_health: 6,
                training: vec![reference("spellcasting", "Spellcasting")],
                attribute: None,
                talent: None,
            }],
            sources: vec![InspectionSource {
                owner: None,
                category: "species".into(),
                source: reference("human", "Human"),
                rows: vec![CombatTraceStep {
                    id: "custom".into(),
                    name: "Custom modifier".into(),
                    fields: vec![],
                }],
            }],
            conditions: vec![],
        }
    }
    #[test]
    fn source_rows_and_exact_large_seeds_are_disclosed() {
        let mut value = report();
        let text = lines(&value).unwrap().join("\n");
        for expected in [
            "test mage",
            "Human",
            "blue",
            "Hit die 1",
            "Mage",
            "18446744073709551615",
            "Spellcasting",
            "injury 3",
            "Custom modifier",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        value.hp += 1;
        assert!(lines(&value).is_err());
    }
}
