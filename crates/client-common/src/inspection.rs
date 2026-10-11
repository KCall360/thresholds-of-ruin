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
    let selected = &report.selected_attack;
    rows.push(format!(
        "Selected melee: {} ({}, {}{}); {}; weapon check {:+}; wind-up {}; recovery {}",
        selected.source.name,
        selected.category,
        selected.source.id,
        selected
            .socket
            .map_or_else(String::new, |socket| format!(", socket {socket}")),
        selected.attack.skill.name,
        selected.attack.bonus,
        selected.attack.wind_up,
        selected.attack.recovery
    ));
    rows.push(format!(
        "Selected check: {:+}; {} {}; skill rank {}; weapon {:+}",
        selected.check_bonus,
        selected.attribute.name,
        selected.attribute_value,
        selected.rank,
        selected.attack.bonus
    ));
    for component in &selected.attack.damage.components {
        let amount = match component.amount {
            DamageAmountView::Fixed { value } => value.to_string(),
            DamageAmountView::Rolled {
                count,
                sides,
                bonus,
            } => format!("{count}d{sides}{bonus:+}"),
        };
        rows.push(format!(
            "Selected damage: {:?}{} {amount}{}",
            component.category,
            component
                .descriptor
                .as_ref()
                .map_or_else(String::new, |d| format!(" ({})", d.name)),
            if component.key() == selected.attack.damage.primary {
                " [primary]"
            } else {
                ""
            }
        ));
    }
    for profile in &selected.abilities {
        rows.extend(crate::combat_diagnostics::trace_lines(profile));
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
        let attack = AttackView {
            skill: reference("heavy_weaponry", "Heavy weaponry"),
            bonus: 0,
            wind_up: 60,
            recovery: 40,
            damage: DamageView {
                primary: component.key(),
                components: vec![component],
            },
        };
        CreatureInspectionView {
            actor: ActorId(2),
            tick: 0,
            name: "test mage".into(),
            faction: "blue".into(),
            species: reference("human", "Human"),
            natural_attack: attack.clone(),
            selected_attack: SelectedAttackView {
                category: "species".into(),
                source: reference("human", "Human"),
                socket: None,
                attack,
                attribute: reference("strength", "Strength"),
                attribute_value: 1,
                rank: 0,
                check_bonus: 1,
                abilities: vec![],
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

#[cfg(test)]
mod selected_attack_tests {
    use super::*;
    #[test]
    fn selected_weapon_check_and_damage_are_rendered_separately_from_natural_attack() {
        let mut report = tests::report();
        let selected = &mut report.selected_attack;
        selected.category = "equipment".into();
        selected.source = tests::reference("100", "Fire blade");
        selected.socket = Some(0);
        selected.attack.skill = tests::reference("light_weaponry", "Light weaponry");
        selected.attack.bonus = 100;
        selected.attack.wind_up = 90;
        selected.attack.recovery = 70;
        selected.attribute = tests::reference("speed", "Speed");
        selected.attribute_value = 1;
        selected.rank = 0;
        selected.check_bonus = 101;
        selected.attack.damage.components[0].descriptor = Some(tests::reference("fire", "Fire"));
        selected.attack.damage.primary = selected.attack.damage.components[0].key();
        let output = lines(&report).unwrap().join("\n");
        for expected in [
            "Species melee: Heavy weaponry",
            "Selected melee: Fire blade",
            "socket 0",
            "Light weaponry; weapon check +100; wind-up 90; recovery 70",
            "Selected check: +101; Speed 1; skill rank 0; weapon +100",
            "Selected damage: Impact (Fire)",
        ] {
            assert!(output.contains(expected), "missing {expected}: {output}");
        }
    }
}
