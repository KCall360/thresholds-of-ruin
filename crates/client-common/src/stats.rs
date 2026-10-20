//! Personal inspection prose shared by Text and ASCII. Only the attached
//! actor's disclosed combat projection is an input to this presentation.
use tor_protocol::*;

/// Separate rows let native views wrap and scroll without losing any details.
pub fn lines(combat: Option<&CombatView>) -> Vec<String> {
    let Some(combat) = combat else {
        return vec!["No creature stats are available.".into()];
    };
    let mut rows = vec![format!("Health: {}/{}", combat.hp, combat.max_hp)];
    if let Some(record) = &combat.public_plan {
        rows.push(crate::public_plan::line(record));
    }
    let Some(stats) = &combat.own_stats else {
        rows.push("Detailed creature stats are unavailable.".into());
        return rows;
    };
    rows.push(health_recovery_line(&stats.health_recovery));
    if let Some(line) = nutrition_line(stats.nutrition) {
        rows.push(line);
    }
    if let Some(line) = restorable_losses_line(stats.restorable_losses) {
        rows.push(line);
    }
    for condition in &stats.conditions {
        rows.push(format!(
            "{}: {}",
            crate::inspection::phase_label(&condition.condition.name, condition.phase),
            crate::inspection::condition_lifetime(condition.remaining_ticks)
        ));
    }
    rows.push(format!("Type: {}", stats.kind.name));
    if !stats.subtypes.is_empty() {
        rows.push(format!(
            "Subtypes: {}",
            stats
                .subtypes
                .iter()
                .map(|value| value.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let mut counts: Vec<_> = stats.hit_dice.sources.iter().collect();
    counts.sort_by_key(|row| (&row.source.name, row.source_kind, &row.source.id));
    rows.push(format!(
        "Hit dice: {} ({})",
        stats.hit_dice.total,
        counts
            .iter()
            .map(|row| format!("{} {}", row.count, row.source.name))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    let a = &stats.attributes;
    rows.push(format!(
        "Strength {}   Speed {}   Intellect {}",
        a.strength, a.speed, a.intellect
    ));
    rows.push(format!(
        "Willpower {}   Awareness {}   Presence {}",
        a.willpower, a.awareness, a.presence
    ));
    let d = &stats.defenses;
    rows.push(format!(
        "Defenses: Physical {}   Cognitive {}   Spiritual {}",
        d.physical, d.cognitive, d.spiritual
    ));
    rows.push(format!("Mana bound to {}", stats.binding.name));
    for pool in &stats.resources {
        rows.push(format!(
            "{}: {}/{} ({} available, {} reserved)",
            pool.resource.name, pool.balance, pool.maximum, pool.available, pool.reserved
        ));
    }
    rows.push("Skills:".into());
    rows.extend(
        stats
            .skills
            .iter()
            .map(|value| format!("  {}: {}", value.skill.name, value.rank)),
    );
    for (heading, talents) in [
        ("Active talents", &stats.active_talents),
        ("Dormant talents", &stats.dormant_talents),
    ] {
        rows.push(format!(
            "{heading}:{}",
            if talents.is_empty() { " none" } else { "" }
        ));
        rows.extend(talents.iter().map(|value| format!("  {}", value.name)));
    }
    for expertise in &stats.expertises {
        rows.extend(expertise_lines(expertise));
    }
    rows.push("Granted abilities:".into());
    rows.extend(
        stats
            .abilities
            .iter()
            .map(|value| format!("  {}", crate::abilities::ability_label(value))),
    );
    rows
}

/// Shared by personal stats and privileged inspection; values are server-owned.
pub(crate) fn health_recovery_line(recovery: &HealthRecoveryView) -> String {
    format!(
        "Health recovery: {}1 per {} active ticks; progress {}/{}; damage resets {}%",
        if recovery.stopped { "stopped; " } else { "" },
        recovery.period_ticks,
        recovery.elapsed_ticks,
        recovery.period_ticks,
        recovery.reset_percent,
    )
}

/// Render a compact server-owned summary; detailed provenance stays authoritative on the server.
pub(crate) fn expertise_lines(row: &AdvancementExpertiseRow) -> Vec<String> {
    vec![format!(
        "Expertise {} ({}): {:?}; sources {}",
        row.expertise.name, row.expertise.id, row.status, row.source_count
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarized_hit_dice_keep_distinct_identities_with_matching_display_names() {
        let mut combat = crate::test_support::observation().combat.unwrap();
        combat.own_stats.as_mut().unwrap().hit_dice = HitDiceSummary {
            total: 256,
            sources: vec![
                HitDiceSourceCount {
                    source_kind: HitDieSourceKind::Class,
                    source: ContentRef {
                        id: "first".into(),
                        name: "Shared name".into(),
                    },
                    count: 1,
                },
                HitDiceSourceCount {
                    source_kind: HitDieSourceKind::Class,
                    source: ContentRef {
                        id: "second".into(),
                        name: "Shared name".into(),
                    },
                    count: 255,
                },
            ],
        };
        assert!(
            lines(Some(&combat)).contains(&"Hit dice: 256 (1 Shared name, 255 Shared name)".into())
        );
        combat.own_stats.as_mut().unwrap().hit_dice = HitDiceSummary {
            total: 0,
            sources: vec![],
        };
        assert!(lines(Some(&combat)).contains(&"Hit dice: 0 ()".into()));
    }

    #[test]
    fn inspection_shows_balances_dormant_choices_and_grants_without_claiming_readiness() {
        let abilities = serde_json::json!([{"cooldown_remaining":"0","supply_costs":[],"id":"basic_melee","name":"Basic Melee","cost":null,"reach":{"kind":"melee"},"target_kind":"actor","basic_attack":true,"weapon_selection":true}, {"cooldown_remaining":"0","supply_costs":[],"id":"magic_bolt","name":"Magic Bolt","cost":null,"reach":{"kind":"sight","range":6},"target_kind":"actor","basic_attack":false,"weapon_selection":false}]);
        let combat: CombatView = serde_json::from_value(serde_json::json!({
            "hp": "7", "max_hp": "12", "public_plan":null,"sequence":null,"preparation_remaining": "50", "preparation_active": true,
            "recovery_remaining": "0", "actors": [], "events": [], "objective": null,
            "victory": false, "dead": false, "terminal": false,
            "own_stats": {
                "nutrition":{"kind":"exempt"}, "conditions":[],
                "restorable_losses":{"hit_dice":"0","attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},"maximum_health":0},
                "health_recovery": {"period_ticks":"1200", "elapsed_ticks":"0", "stopped":false, "reset_percent":100},
                "kind": {"id":"undead","name":"Undead"}, "subtypes": [{"id":"augmented","name":"Augmented"}], "hit_dice": {"total":"2","sources":[{"source_kind":"class","source":{"id":"mage","name":"Mage"},"count":"1"},{"source_kind":"racial","source":{"id":"racial","name":"Racial"},"count":"1"}]},
                "attributes": {"strength": 3, "speed": 0, "intellect": 2, "willpower": 1, "awareness": 1, "presence": 0},
                "skills": [{"skill": {"id":"spellcasting","name":"Spellcasting"}, "rank": 2}, {"skill": {"id":"breath","name":"Breath"}, "rank": 2}],
                "defenses": {"physical": 13, "cognitive": 13, "spiritual": 11},
                "binding": {"id":"awareness","name":"Awareness"}, "resources": [{"resource": {"id":"mana","name":"Mana"}, "balance": 2, "maximum": 3, "available": 1, "reserved": 1}],
                "expertises": [], "active_talents": [{"id":"hardiness","name":"Hardiness"}], "dormant_talents": [{"id":"fear","name":"Fear"}], "abilities": abilities
            }
        })).unwrap();
        let text = lines(Some(&combat)).join("\n");
        for expected in [
            "Health: 7/12",
            "Health recovery: 1 per 1200 active ticks; progress 0/1200; damage resets 100%",
            "Type: Undead",
            "Subtypes: Augmented",
            "Hit dice: 2 (1 Mage, 1 Racial)",
            "Strength 3",
            "Spellcasting: 2",
            "Breath: 2",
            "Mana bound to Awareness",
            "Mana: 2/3 (1 available, 1 reserved)",
            "Active talents:\n  Hardiness",
            "Dormant talents:\n  Fear",
            "Granted abilities:\n  Basic Melee (weapon choice)\n  Magic Bolt",
        ] {
            assert!(text.contains(expected), "missing {expected}");
        }
        assert!(!text.contains("ready"));
        assert_eq!(lines(None), ["No creature stats are available."]);
        let mut profile = combat;
        profile.own_stats = None;
        assert_eq!(
            lines(Some(&profile)),
            ["Health: 7/12", "Detailed creature stats are unavailable."]
        );
    }
}

/// Only personal or authorized inspection projections contain this reserve.
pub fn nutrition_line(nutrition: NutritionView) -> Option<String> {
    match nutrition {
        NutritionView::Exempt => None,
        NutritionView::Required {
            reserve,
            maximum,
            hunger,
            unit_ticks,
            elapsed,
            starve_at,
        } => Some(format!(
            "Nutrition: {reserve}/{maximum} ({}); next unit in {} active ticks; starvation at {starve_at}",
            hunger_label(hunger),
            unit_ticks - elapsed
        )),
    }
}
pub fn hunger_label(hunger: Hunger) -> &'static str {
    match hunger {
        Hunger::Nourished => "nourished",
        Hunger::Hungry => "hungry",
        Hunger::Weak => "weak",
        Hunger::Empty => "empty",
    }
}

#[cfg(test)]
mod personal_condition_tests {
    use super::*;
    #[test]
    fn active_and_protected_personal_conditions_share_the_inspection_labels() {
        assert_eq!(
            crate::inspection::phase_label("Fixture Faint", ConditionPhase::Active),
            "Fixture Faint"
        );
        assert_eq!(
            crate::inspection::phase_label("Fixture Ice", ConditionPhase::Protection),
            "Protection against Fixture Ice"
        );
    }
}

#[cfg(test)]
mod restorable_loss_tests {
    use super::*;

    #[test]
    fn personal_hit_die_loss_is_named_and_hidden_when_zero_or_not_owned() {
        let mut combat = crate::test_support::observation().combat.unwrap();
        combat
            .own_stats
            .as_mut()
            .unwrap()
            .restorable_losses
            .hit_dice = 2;
        assert!(lines(Some(&combat))
            .join("\n")
            .contains("Restorable losses: Hit Dice 2"));
        combat
            .own_stats
            .as_mut()
            .unwrap()
            .restorable_losses
            .hit_dice = 0;
        assert!(!lines(Some(&combat))
            .join("\n")
            .contains("Restorable losses:"));
        combat.own_stats = None;
        assert!(!lines(Some(&combat))
            .join("\n")
            .contains("Restorable losses:"));
    }

    #[test]
    fn personal_stats_name_only_nonzero_restorable_deficits_without_timers_or_sources() {
        let mut combat = crate::test_support::observation().combat.unwrap();
        let mut value = serde_json::to_value(combat.own_stats.as_ref().unwrap()).unwrap();
        value["restorable_losses"] = serde_json::json!({
            "attributes":{"strength":2,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":1},
            "hit_dice":"0","maximum_health":3
        });
        combat.own_stats = Some(serde_json::from_value(value).unwrap());
        let text = lines(Some(&combat)).join("\n");
        assert!(text.contains("Restorable losses: Strength 2, Presence 1, maximum Health 3"));
        assert!(!text.contains("Speed 0"));
        assert!(!text.contains("from actor"));
        combat.own_stats = None;
        assert!(!lines(Some(&combat))
            .join("\n")
            .contains("Restorable losses"));
    }
}

/// Shared by personal stats and authorized inspection. No timing is implied.
pub(crate) fn restorable_losses_line(losses: RestorableLossesView) -> Option<String> {
    let a = losses.attributes;
    let mut parts = [
        ("Strength", a.strength),
        ("Speed", a.speed),
        ("Intellect", a.intellect),
        ("Willpower", a.willpower),
        ("Awareness", a.awareness),
        ("Presence", a.presence),
    ]
    .into_iter()
    .filter(|(_, amount)| *amount > 0)
    .map(|(label, amount)| format!("{label} {amount}"))
    .collect::<Vec<_>>();
    if losses.hit_dice > 0 {
        parts.push(format!("Hit Dice {}", losses.hit_dice));
    }
    if losses.maximum_health > 0 {
        parts.push(format!("maximum Health {}", losses.maximum_health));
    }
    (!parts.is_empty()).then(|| format!("Restorable losses: {}", parts.join(", ")))
}
