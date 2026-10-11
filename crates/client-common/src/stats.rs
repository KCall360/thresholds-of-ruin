//! Personal inspection prose shared by Text and ASCII. Only the attached
//! actor's disclosed combat projection is an input to this presentation.
use tor_protocol::*;

/// Separate rows let native views wrap and scroll without losing any details.
pub fn lines(combat: Option<&CombatView>) -> Vec<String> {
    let Some(combat) = combat else {
        return vec!["No creature stats are available.".into()];
    };
    let mut rows = vec![format!("Health: {}/{}", combat.hp, combat.max_hp)];
    let Some(stats) = &combat.own_stats else {
        rows.push("Detailed creature stats are unavailable.".into());
        return rows;
    };
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
    let mut counts = std::collections::BTreeMap::new();
    for source in &stats.hit_dice {
        *counts.entry(source.name.as_str()).or_insert(0_usize) += 1;
    }
    rows.push(format!(
        "Hit dice: {} ({})",
        stats.hit_dice.len(),
        counts
            .iter()
            .map(|(name, count)| format!("{count} {name}"))
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
    rows.push("Granted abilities:".into());
    rows.extend(
        stats
            .abilities
            .iter()
            .map(|value| format!("  {}", value.name)),
    );
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspection_shows_balances_dormant_choices_and_grants_without_claiming_readiness() {
        let combat: CombatView = serde_json::from_value(serde_json::json!({
            "hp": 7, "max_hp": 12, "preparation_remaining": "50", "preparation_active": true,
            "recovery_remaining": "0", "actors": [], "events": [], "objective": null,
            "victory": false, "dead": false, "terminal": false,
            "own_stats": {
                "kind": {"id":"undead","name":"Undead"}, "subtypes": [{"id":"augmented","name":"Augmented"}], "hit_dice": [{"id":"racial","name":"Racial"}, {"id":"mage","name":"Mage"}],
                "attributes": {"strength": 3, "speed": 0, "intellect": 2, "willpower": 1, "awareness": 1, "presence": 0},
                "skills": [{"skill": {"id":"spellcasting","name":"Spellcasting"}, "rank": 2}],
                "defenses": {"physical": 13, "cognitive": 13, "spiritual": 11},
                "binding": {"id":"awareness","name":"Awareness"}, "resources": [{"resource": {"id":"mana","name":"Mana"}, "balance": 2, "maximum": 3, "available": 1, "reserved": 1}],
                "active_talents": [{"id":"hardiness","name":"Hardiness"}], "dormant_talents": [{"id":"fear","name":"Fear"}], "abilities": [{"id":"basic_melee","name":"Basic Melee","cost":null,"reach":{"kind":"melee"},"target_kind":"actor","basic_attack":true}, {"id":"magic_bolt","name":"Magic Bolt","cost":null,"reach":{"kind":"sight","range":6},"target_kind":"actor","basic_attack":false}]
            }
        })).unwrap();
        let text = lines(Some(&combat)).join("\n");
        for expected in [
            "Health: 7/12",
            "Type: Undead",
            "Subtypes: Augmented",
            "Hit dice: 2 (1 Mage, 1 Racial)",
            "Strength 3",
            "Spellcasting: 2",
            "Mana bound to Awareness",
            "Mana: 2/3 (1 available, 1 reserved)",
            "Active talents:\n  Hardiness",
            "Dormant talents:\n  Fear",
            "Granted abilities:\n  Basic Melee\n  Magic Bolt",
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
