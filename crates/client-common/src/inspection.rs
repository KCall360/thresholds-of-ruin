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
    rows.push(crate::stats::health_recovery_line(
        &report.stats.health_recovery,
    ));
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
    rows.push(format!(
        "Natural source: {} ({})",
        attack.source.name, attack.source.id
    ));
    rows.push(format!(
        "Natural: {} authored strikes; strikes natural",
        attack.strike_count
    ));
    let selected = &report.selected_attack;
    let source_kind = match &selected.attack {
        SelectedMeleeView::Weapon { .. } => "weapon",
        SelectedMeleeView::Natural { .. } => "natural",
    };
    rows.push(format!(
        "Selected melee: {} ({}, {}{}); {}; {source_kind} check {:+}; wind-up {}; recovery {}",
        selected.source.name,
        selected.category,
        selected.source.id,
        selected
            .socket
            .map_or_else(String::new, |socket| format!(", socket {socket}")),
        selected.attack.skill().name,
        selected.attack.bonus(),
        selected.attack.wind_up(),
        selected.attack.recovery()
    ));
    rows.push(format!(
        "Selected check: {:+}; {} {}; skill rank {}; {source_kind} {:+}",
        selected.check_bonus,
        selected.attribute.name,
        selected.attribute_value,
        selected.rank,
        selected.attack.bonus()
    ));
    match &selected.attack {
        SelectedMeleeView::Weapon { attack } => damage_rows(&mut rows, "Selected", &attack.damage),
        SelectedMeleeView::Natural { attack } => {
            rows.push(format!(
                "Selected: {} authored strikes; strikes selected",
                attack.strike_count
            ));
        }
    }
    rows.push("Strike sources: strikes natural; strikes selected; strikes ability <id>".into());
    for (source, ability) in report.ability_strikes.iter().zip(&report.stats.abilities) {
        rows.push(format!("Ability: {} ({})", ability.name, source.ability));
        match &source.source {
            StrikeSourceInspectionView::Authored { strike_count } => {
                rows.push(format!(
                    "  {strike_count} authored strikes; strikes ability {}",
                    source.ability
                ));
            }
            StrikeSourceInspectionView::SelectedMelee {} => rows.push(format!(
                "  Uses selected melee; strikes ability {} (then strikes selected)",
                source.ability
            )),
        }
    }
    for profile in &selected.abilities {
        rows.extend(crate::combat_diagnostics::trace_lines(profile));
    }
    for template in &report.templates {
        rows.push(format!("Template: {} ({})", template.name, template.id));
    }
    rows.push(format!(
        "Starting skills (permanent): {}, {}",
        report.starting_skills[0].name, report.starting_skills[1].name
    ));
    rows.push(format!(
        "Restoration: {}",
        match report.restoration {
            InspectionRestoration::ChooseAgain => "choose again",
            InspectionRestoration::Authored => "authored build",
        }
    ));
    for die in &report.hit_dice {
        rows.push(format!(
            "Hit die {}: {}; health d{} rolled {}; seed {}",
            die.ordinal, die.source.name, die.health_die, die.base_health, die.health_seed
        ));
        for pool in &die.point_pools {
            rows.push(format!(
                "  {} points: {} granted, {} spent, {} unspent; buys {}",
                pool.pool.name,
                pool.granted,
                pool.spent,
                pool.granted - pool.spent,
                pool.allowed
                    .iter()
                    .map(|kind| match kind {
                        AdvancementKind::SkillRank => "skill ranks",
                        AdvancementKind::Talent => "talents",
                    })
                    .collect::<Vec<_>>()
                    .join(" or ")
            ));
        }
        for purchase in &die.purchases {
            let description = match &purchase.selection {
                InspectionPurchaseSelection::SkillRank { skill } => {
                    format!("skill rank {}", skill.name)
                }
                InspectionPurchaseSelection::Talent { talent } => format!("talent {}", talent.name),
                InspectionPurchaseSelection::TalentForm { talent, form } => {
                    format!("talent {} / form {} ({})", talent.name, form.name, form.id)
                }
            };
            rows.push(format!(
                "  Purchase ({}): {description}",
                purchase.pool.name
            ));
        }
        rows.push(format!(
            "  Attribute: {}",
            die.attribute.as_ref().map_or("none", |x| x.name.as_str())
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
    for entry in &report.dormant_talents {
        rows.push(format!(
            "Dormant talent: {} ({})",
            entry.talent.name,
            entry.reasons.join("; ")
        ));
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
fn damage_rows(rows: &mut Vec<String>, label: &str, damage: &DamageView) {
    for component in &damage.components {
        let amount = match component.amount {
            DamageAmountView::Fixed { value } => value.to_string(),
            DamageAmountView::Rolled {
                count,
                sides,
                bonus,
            } => format!("{count}d{sides}{bonus:+}"),
        };
        rows.push(format!(
            "{label} damage: {:?}{} {amount}{}",
            component.category,
            component
                .descriptor
                .as_ref()
                .map_or_else(String::new, |d| format!(" ({})", d.name)),
            if component.key() == damage.primary {
                " [primary]"
            } else {
                ""
            }
        ));
    }
}

/// Render exactly one bounded, validated detail page. No source is accumulated.
pub fn page_lines(page: &CreatureStrikePage) -> Result<Vec<String>, InvalidStrikePage> {
    page.validate()?;
    let source = match &page.resolved {
        StrikeSourceAttribution::Ability { ability } => {
            format!("Ability: {} ({})", ability.name, ability.id)
        }
        StrikeSourceAttribution::Natural { species } => {
            format!("Natural source: {} ({})", species.name, species.id)
        }
        StrikeSourceAttribution::Weapon { source, socket } => {
            format!("Weapon: {} ({}, socket {socket})", source.name, source.id)
        }
    };
    let mut rows = vec![format!(
        "Strike source: {source}; {} authored strikes",
        page.strike_count
    )];
    let trace = |rows: &mut Vec<String>, label: String, row: &CombatTraceStep| {
        rows.push(label);
        rows.extend(
            crate::combat_diagnostics::trace_lines(row)
                .into_iter()
                .map(|line| format!("  {line}")),
        );
    };
    for entry in &page.entries {
        match entry {
            StrikePageEntry::StrikeHeader {
                strike,
                offset,
                unlock,
                check,
                success_count,
                failure_count,
                descriptor_count,
                rider_count,
            } => {
                rows.push(format!(
                    "Strike {} (authored {strike}); offset {offset}; unlock {}",
                    strike + 1,
                    unlock.as_ref().map_or_else(
                        || "none".into(),
                        |value| format!("{} ({})", value.name, value.id)
                    )
                ));
                if let Some(check) = check {
                    trace(&mut rows, "  Primary check:".into(), check);
                } else {
                    rows.push("  Primary check: none".into());
                }
                rows.push(format!("  Primary success: {success_count} effects; Primary failure: {failure_count} effects; Descriptors: {descriptor_count}; Riders: {rider_count}"));
            }
            StrikePageEntry::SuccessEffect { strike, index, row } => trace(
                &mut rows,
                format!(
                    "Strike {} primary success effect {} (authored {index}):",
                    strike + 1,
                    index + 1
                ),
                row,
            ),
            StrikePageEntry::FailureEffect { strike, index, row } => trace(
                &mut rows,
                format!(
                    "Strike {} primary failure effect {} (authored {index}):",
                    strike + 1,
                    index + 1
                ),
                row,
            ),
            StrikePageEntry::StrikeDescriptor {
                strike,
                index,
                descriptor,
            } => rows.push(format!(
                "Strike {} descriptor {} (authored {index}): {} ({})",
                strike + 1,
                index + 1,
                descriptor.name,
                descriptor.id
            )),
            StrikePageEntry::RiderHeader {
                strike,
                rider,
                trigger,
                check,
                effect_count,
                descriptor_count,
            } => {
                rows.push(format!("Strike {} Rider {} (authored {rider}); trigger primary {}; effects {effect_count}; descriptors {descriptor_count}", strike + 1, rider + 1,
                    match trigger { RiderTrigger::Success => "success", RiderTrigger::Failure => "failure" }));
                if let Some(check) = check {
                    trace(&mut rows, "  Rider check:".into(), check);
                } else {
                    rows.push("  Rider check: none".into());
                }
            }
            StrikePageEntry::RiderEffect {
                strike,
                rider,
                index,
                row,
            } => trace(
                &mut rows,
                format!(
                    "Strike {} rider {} effect {} (authored {index}):",
                    strike + 1,
                    rider + 1,
                    index + 1
                ),
                row,
            ),
            StrikePageEntry::RiderDescriptor {
                strike,
                rider,
                index,
                descriptor,
            } => rows.push(format!(
                "Strike {} rider {} descriptor {} (authored {index}): {} ({})",
                strike + 1,
                rider + 1,
                index + 1,
                descriptor.name,
                descriptor.id
            )),
            StrikePageEntry::Weapon { attack } => {
                rows.push(format!(
                    "Selected weapon: {}; check {:+}; wind-up {}; recovery {}",
                    attack.skill.name, attack.bonus, attack.wind_up, attack.recovery
                ));
                damage_rows(&mut rows, "Selected", &attack.damage);
            }
            StrikePageEntry::SelectedMelee {} => {
                rows.push("Uses selected melee; strikes selected".into())
            }
        }
    }
    rows.push(
        if page.next.is_some() {
            "More: strikes next; Back: strikes previous"
        } else {
            "End of strike source; Back: strikes previous"
        }
        .into(),
    );
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
            revision: 5,
            tick: 0,
            name: "test mage".into(),
            faction: "blue".into(),
            species: reference("human", "Human"),
            starting_skills: [
                reference("athletics", "Athletics"),
                reference("lore", "Lore"),
            ],
            restoration: InspectionRestoration::ChooseAgain,
            dormant_talents: vec![],
            ability_strikes: vec![],
            natural_attack: NaturalAttackView {
                source: reference("human", "Human"),
                skill: attack.skill.clone(),
                bonus: attack.bonus,
                wind_up: attack.wind_up,
                recovery: attack.recovery,
                strike_count: 1,
            },
            selected_attack: SelectedAttackView {
                category: "equipment".into(),
                source: reference("100", "Training blade"),
                socket: Some(0),
                attack: SelectedMeleeView::Weapon { attack },
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
                skills: [
                    ("athletics", "Athletics"),
                    ("lore", "Lore"),
                    ("spellcasting", "Spellcasting"),
                ]
                .map(|(id, name)| SkillView {
                    skill: reference(id, name),
                    rank: 1,
                })
                .to_vec(),
                defenses: DefenseView {
                    physical: 12,
                    cognitive: 12,
                    spiritual: 12,
                },
                binding: reference("intellect", "Intellect"),
                health_recovery: tor_protocol::HealthRecoveryView {
                    period_ticks: 1200,
                    elapsed_ticks: 0,
                    stopped: false,
                    reset_percent: 100,
                },
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
                point_pools: vec![InspectionPointPool {
                    pool: reference("advancement", "Advancement"),
                    granted: 1,
                    spent: 1,
                    allowed: vec![AdvancementKind::SkillRank, AdvancementKind::Talent],
                }],
                purchases: vec![InspectionPurchase {
                    pool: reference("advancement", "Advancement"),
                    selection: InspectionPurchaseSelection::SkillRank {
                        skill: reference("spellcasting", "Spellcasting"),
                    },
                }],
                attribute: None,
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
            "Health recovery: 1 per 1200 active ticks",
            "Custom modifier",
            "Starting skills (permanent): Athletics, Lore",
            "Advancement points: 1 granted, 1 spent, 0 unspent; buys skill ranks or talents",
            "Purchase (Advancement): skill rank Spellcasting",
            "Restoration: choose again",
        ] {
            assert!(text.contains(expected), "{expected}");
        }
        value.hp += 1;
        assert!(lines(&value).is_err());
    }
    #[test]
    fn contradictory_skill_ownership_is_rejected_before_rendering() {
        let mut value = report();
        lines(&value).unwrap();
        value.stats.skills[2].rank = 2;
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
        let SelectedMeleeView::Weapon { attack } = &mut selected.attack else {
            panic!("weapon fixture")
        };
        attack.skill = tests::reference("light_weaponry", "Light weaponry");
        attack.bonus = 100;
        attack.wind_up = 90;
        attack.recovery = 70;
        selected.attribute = tests::reference("speed", "Speed");
        selected.attribute_value = 1;
        selected.rank = 0;
        selected.check_bonus = 101;
        attack.damage.components[0].descriptor = Some(tests::reference("fire", "Fire"));
        attack.damage.primary = attack.damage.components[0].key();
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

#[cfg(test)]
mod ordered_strike_tests {
    use super::*;

    fn source_page() -> CreatureStrikePage {
        let trace = |id: &str| CombatTraceStep {
            id: id.into(),
            name: id.replace('_', " "),
            fields: vec![],
        };
        CreatureStrikePage {
            inspection: StrikeInspectionId("private-detail".into()),
            actor: ActorId(2),
            target_revision: 5,
            source: StrikeSourceSelector::Ability {
                id: "triple_strike".into(),
            },
            resolved: StrikeSourceAttribution::Ability {
                ability: crate::test_support::content("triple_strike"),
            },
            strike_count: 2,
            start: StrikePagePosition::StrikeHeader { strike: 0 },
            next: None,
            entries: vec![
                StrikePageEntry::StrikeHeader {
                    strike: 0,
                    offset: 0,
                    unlock: None,
                    check: None,
                    success_count: 0,
                    failure_count: 0,
                    descriptor_count: 0,
                    rider_count: 0,
                },
                StrikePageEntry::StrikeHeader {
                    strike: 1,
                    offset: 20,
                    unlock: Some(crate::test_support::content("second_claw")),
                    check: None,
                    success_count: 1,
                    failure_count: 1,
                    descriptor_count: 0,
                    rider_count: 1,
                },
                StrikePageEntry::SuccessEffect {
                    strike: 1,
                    index: 0,
                    row: trace("primary_damage"),
                },
                StrikePageEntry::FailureEffect {
                    strike: 1,
                    index: 0,
                    row: trace("failure_effect"),
                },
                StrikePageEntry::RiderHeader {
                    strike: 1,
                    rider: 0,
                    trigger: RiderTrigger::Success,
                    check: Some(trace("rider_check")),
                    effect_count: 1,
                    descriptor_count: 1,
                },
                StrikePageEntry::RiderEffect {
                    strike: 1,
                    rider: 0,
                    index: 0,
                    row: trace("rider_effect"),
                },
                StrikePageEntry::RiderDescriptor {
                    strike: 1,
                    rider: 0,
                    index: 0,
                    descriptor: crate::test_support::content("mind_affecting"),
                },
            ],
        }
    }

    #[test]
    fn ordered_strikes_inspection_preserves_authored_order_unlocks_and_rider_branches() {
        let output = page_lines(&source_page()).unwrap().join("\n");
        for expected in [
            "Strike source:",
            "Strike 1 (authored 0); offset 0; unlock none",
            "Strike 2 (authored 1); offset 20; unlock Second Claw (second_claw)",
            "Primary check: none",
            "Primary success:",
            "primary damage",
            "Primary failure:",
            "failure effect",
            "Rider 1 (authored 0)",
            "rider check",
            "rider effect",
            "Mind Affecting",
            "Ability: Triple Strike (triple_strike)",
        ] {
            assert!(output.contains(expected), "missing {expected}: {output}");
        }
        assert!(output.find("authored 0").unwrap() < output.find("authored 1").unwrap());
        assert!(
            !output.contains("Natural damage:"),
            "checkless empty strike must not invent damage"
        );
    }

    #[test]
    fn ordered_strikes_inspection_rejects_invalid_source_before_rendering() {
        let mut page = source_page();
        if let StrikePageEntry::StrikeHeader { offset, .. } = &mut page.entries[0] {
            *offset = 1;
        }
        assert!(page_lines(&page).is_err());
    }

    #[test]
    fn ordered_strikes_page_rider_trigger_retains_primary_failure_attribution() {
        let mut page = source_page();
        if let StrikePageEntry::StrikeHeader { check, .. } = &mut page.entries[1] {
            *check = Some(CombatTraceStep {
                id: "primary_check".into(),
                name: "Primary check".into(),
                fields: vec![],
            });
        }
        if let StrikePageEntry::RiderHeader { trigger, .. } = &mut page.entries[4] {
            *trigger = RiderTrigger::Failure;
        }
        let output = page_lines(&page).unwrap().join("\n");
        assert!(output.contains("trigger primary failure"), "{output}");
        assert!(output.contains("rider check"));
        assert!(output.contains("rider effect"));
    }

    #[test]
    fn ordered_strikes_selected_melee_marker_links_to_separate_source() {
        let mut page = source_page();
        page.strike_count = 0;
        page.start = StrikePagePosition::SelectedMelee {};
        page.entries = vec![StrikePageEntry::SelectedMelee {}];
        let output = page_lines(&page).unwrap().join("\n");
        assert!(output.contains("Uses selected melee"));
        assert!(output.contains("strikes selected"));
        assert!(!output.contains("primary success"));
    }
}

#[cfg(test)]
mod ordered_strikes_page_contract_tests {
    use super::*;
    #[test]
    fn ordered_strikes_inspection_summary_renders_explicit_source_navigation_without_bodies() {
        let mut value = serde_json::to_value(super::tests::report()).unwrap();
        value["revision"] = serde_json::json!("5");
        value["natural_attack"]
            .as_object_mut()
            .unwrap()
            .remove("strikes");
        value["natural_attack"]["strike_count"] = serde_json::json!(16);
        value["selected_attack"]["attack"] =
            serde_json::json!({"kind":"natural","attack":value["natural_attack"].clone()});
        value["ability_strikes"]=serde_json::json!(value["stats"]["abilities"].as_array().unwrap().iter().map(|ability|
            serde_json::json!({"ability":ability["id"],"source":{"kind":"selected_melee"}})).collect::<Vec<_>>());
        let report: CreatureInspectionView =
            serde_json::from_value(value).expect("main private report carries source summaries");
        let rows = lines(&report).unwrap().join("\n");
        assert!(rows.contains("16 authored strikes"), "{rows}");
        assert!(rows.contains("strikes natural"), "{rows}");
        assert!(rows.contains("strikes selected"), "{rows}");
        assert!(rows.contains("strikes ability"), "{rows}");
        assert!(
            !rows.contains("Primary success"),
            "main summary must not format unseen detail bodies"
        );
        assert!(
            rows.len() < 64 * 1024,
            "summary rendering must be independent of the body size"
        );
    }
}
