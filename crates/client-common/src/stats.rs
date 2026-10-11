//! Personal inspection prose shared by Text and ASCII. Only the attached
//! actor's disclosed combat projection is an input to this presentation.
use tor_protocol::*;

macro_rules! labels {
    ($name:ident, $kind:ty; $($variant:ident => $label:literal),+ $(,)?) => {
        pub(crate) fn $name(value: $kind) -> &'static str {
            match value { $(<$kind>::$variant => $label),+ }
        }
    };
}

labels!(kind, CreatureType;
    Aberration => "Aberration", Animal => "Animal", Construct => "Construct",
    Dragon => "Dragon", Elemental => "Elemental", Fey => "Fey", Giant => "Giant",
    Humanoid => "Humanoid", MagicalBeast => "Magical beast",
    MonstrousHumanoid => "Monstrous humanoid", Ooze => "Ooze", Outsider => "Outsider",
    Plant => "Plant", Undead => "Undead", Vermin => "Vermin");
labels!(subtype, CreatureSubtype;
    Air => "Air", Angel => "Angel", Aquatic => "Aquatic", Archon => "Archon",
    Augmented => "Augmented", Chaotic => "Chaotic", Cold => "Cold", Earth => "Earth",
    Evil => "Evil", Extraplanar => "Extraplanar", Fire => "Fire", Goblinoid => "Goblinoid",
    Good => "Good", Incorporeal => "Incorporeal", Lawful => "Lawful", Native => "Native",
    Reptilian => "Reptilian", Shapechanger => "Shapechanger", Swarm => "Swarm", Water => "Water");
labels!(skill, Skill;
    Athletics => "Athletics", HeavyWeaponry => "Heavy weaponry", Agility => "Agility",
    LightWeaponry => "Light weaponry", Stealth => "Stealth", Thievery => "Thievery",
    Crafting => "Crafting", Deduction => "Deduction", Lore => "Lore", Medicine => "Medicine",
    Discipline => "Discipline", Intimidation => "Intimidation", Insight => "Insight",
    Perception => "Perception", Survival => "Survival", Deception => "Deception",
    Leadership => "Leadership", Persuasion => "Persuasion", Spellcasting => "Spellcasting");
labels!(talent, Talent;
    Hardiness => "Hardiness", Toughness => "Toughness", Unyielding => "Unyielding",
    Indomitable => "Indomitable", Endurance => "Endurance", DeepEndurance => "Deep endurance",
    Tireless => "Tireless", Guard => "Guard", GreaterGuard => "Greater guard", IronGuard => "Iron guard",
    HeavyBlows => "Heavy blows", MightyBlows => "Mighty blows", CrushingBlows => "Crushing blows",
    PerfectedBlows => "Perfected blows", ImpactWard => "Impact ward", KeenWard => "Keen ward",
    EnergyWard => "Energy ward", Resolve => "Resolve", PowerStrike => "Power strike",
    MagicBolt => "Magic bolt", Fear => "Fear", ArcaneReserve => "Arcane reserve",
    PotentBolt => "Potent bolt", EmpoweredBolt => "Empowered bolt", GreaterBolt => "Greater bolt",
    MasterBolt => "Master bolt", FearMastery => "Fear mastery");
labels!(technique, Technique;
    BasicMelee => "Basic melee", PowerStrike => "Power strike", MagicBolt => "Magic bolt", Fear => "Fear");
labels!(binding, ManaBinding;
    Intellect => "Intellect", Willpower => "Willpower", Awareness => "Awareness", Presence => "Presence");
labels!(resource, Resource; Stamina => "Stamina", Focus => "Focus", Mana => "Mana");

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
    rows.push(format!("Type: {}", kind(stats.kind)));
    if !stats.subtypes.is_empty() {
        rows.push(format!(
            "Subtypes: {}",
            stats
                .subtypes
                .iter()
                .map(|value| subtype(*value))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let count = |source| {
        stats
            .hit_dice
            .iter()
            .filter(|&&value| value == source)
            .count()
    };
    rows.push(format!(
        "Hit dice: {} ({} racial, {} Warrior, {} Mage)",
        stats.hit_dice.len(),
        count(HitDieSource::Racial),
        count(HitDieSource::Warrior),
        count(HitDieSource::Mage)
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
    rows.push(format!("Mana bound to {}", binding(stats.binding)));
    for pool in &stats.resources {
        rows.push(format!(
            "{}: {}/{} ({} available, {} reserved)",
            resource(pool.resource),
            pool.balance,
            pool.maximum,
            pool.available,
            pool.reserved
        ));
    }
    rows.push("Skills:".into());
    rows.extend(
        stats
            .skills
            .iter()
            .map(|value| format!("  {}: {}", skill(value.skill), value.rank)),
    );
    for (heading, talents) in [
        ("Active talents", &stats.active_talents),
        ("Dormant talents", &stats.dormant_talents),
    ] {
        rows.push(format!(
            "{heading}:{}",
            if talents.is_empty() { " none" } else { "" }
        ));
        rows.extend(talents.iter().map(|value| format!("  {}", talent(*value))));
    }
    rows.push("Granted abilities:".into());
    rows.extend(
        stats
            .abilities
            .iter()
            .map(|value| format!("  {}", technique(*value))),
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
                "kind": "undead", "subtypes": ["augmented"], "hit_dice": ["racial", "mage"],
                "attributes": {"strength": 3, "speed": 0, "intellect": 2, "willpower": 1, "awareness": 1, "presence": 0},
                "skills": [{"skill": "spellcasting", "rank": 2}],
                "defenses": {"physical": 13, "cognitive": 13, "spiritual": 11},
                "binding": "awareness", "resources": [{"resource": "mana", "balance": 2, "maximum": 3, "available": 1, "reserved": 1}],
                "active_talents": ["hardiness"], "dormant_talents": ["fear"], "abilities": ["basic_melee", "magic_bolt"]
            }
        })).unwrap();
        let text = lines(Some(&combat)).join("\n");
        for expected in [
            "Health: 7/12",
            "Type: Undead",
            "Subtypes: Augmented",
            "Hit dice: 2 (1 racial, 0 Warrior, 1 Mage)",
            "Strength 3",
            "Spellcasting: 2",
            "Mana bound to Awareness",
            "Mana: 2/3 (1 available, 1 reserved)",
            "Active talents:\n  Hardiness",
            "Dormant talents:\n  Fear",
            "Granted abilities:\n  Basic melee\n  Magic bolt",
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
