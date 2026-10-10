//! Integration tests for this crate, compiled as one binary so each build
//! links one test executable instead of one per file.

mod adventure;
mod commands;
mod parser;
mod turns;

fn creature_combat(abilities: Vec<tor_protocol::Technique>) -> tor_protocol::CombatView {
    use tor_protocol::*;
    CombatView {
        hp: 12,
        max_hp: 12,
        preparation_remaining: None,
        preparation_active: false,
        recovery_remaining: 0,
        actors: vec![],
        events: vec![],
        objective: None,
        exit: None,
        victory: false,
        dead: false,
        terminal: false,
        own_stats: Some(OwnStats {
            kind: CreatureType::Humanoid,
            subtypes: vec![],
            hit_dice: vec![HitDieSource::Warrior, HitDieSource::Mage],
            attributes: AttributeView {
                strength: 2,
                speed: 1,
                intellect: 3,
                willpower: 2,
                awareness: 1,
                presence: 0,
            },
            skills: [
                Skill::Athletics,
                Skill::HeavyWeaponry,
                Skill::Agility,
                Skill::LightWeaponry,
                Skill::Stealth,
                Skill::Thievery,
                Skill::Crafting,
                Skill::Deduction,
                Skill::Lore,
                Skill::Medicine,
                Skill::Discipline,
                Skill::Intimidation,
                Skill::Insight,
                Skill::Perception,
                Skill::Survival,
                Skill::Deception,
                Skill::Leadership,
                Skill::Persuasion,
                Skill::Spellcasting,
            ]
            .into_iter()
            .map(|skill| SkillView { skill, rank: 0 })
            .collect(),
            defenses: DefenseView {
                physical: 13,
                cognitive: 15,
                spiritual: 11,
            },
            binding: ManaBinding::Intellect,
            resources: [Resource::Stamina, Resource::Focus, Resource::Mana]
                .into_iter()
                .map(|resource| ResourceView {
                    resource,
                    balance: 0,
                    maximum: 5,
                    available: 0,
                    reserved: 0,
                })
                .collect(),
            active_talents: vec![],
            dormant_talents: vec![],
            abilities,
        }),
    }
}

// Synthetic entity references for client-model tests, not server target derivation.
fn fixture_digest(index: u64) -> [u8; 32] {
    let mut digest = [0; 32];
    digest[..8].copy_from_slice(&index.to_le_bytes());
    digest
}
fn actor_target(index: u64) -> tor_protocol::ActorTarget {
    tor_protocol::ActorTarget::from_digest(fixture_digest(index))
}
fn item_target(index: u64) -> tor_protocol::ItemTarget {
    tor_protocol::ItemTarget::from_digest(fixture_digest(index))
}
fn door_target(index: u64) -> tor_protocol::DoorTarget {
    tor_protocol::DoorTarget::from_digest(fixture_digest(index))
}
