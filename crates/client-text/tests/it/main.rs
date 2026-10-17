//! Integration tests for this crate, compiled as one binary so each build
//! links one test executable instead of one per file.

mod adventure;
mod commands;
mod corpus;
mod parser;
mod public_plan_observation;
mod turns;

fn creature_combat(abilities: Vec<tor_protocol::AbilityView>) -> tor_protocol::CombatView {
    use tor_protocol::*;
    CombatView {
        public_plan: None,
        sequence: None,
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
            kind: tor_client_common::test_support::content("humanoid"),
            subtypes: vec![],
            hit_dice: HitDiceSummary {
                total: 2,
                sources: vec![
                    HitDiceSourceCount {
                        source_kind: HitDieSourceKind::Class,
                        source: tor_client_common::test_support::content("mage"),
                        count: 1,
                    },
                    HitDiceSourceCount {
                        source_kind: HitDieSourceKind::Class,
                        source: tor_client_common::test_support::content("warrior"),
                        count: 1,
                    },
                ],
            },
            attributes: AttributeView {
                strength: 2,
                speed: 1,
                intellect: 3,
                willpower: 2,
                awareness: 1,
                presence: 0,
            },
            skills: [
                tor_client_common::test_support::content("athletics"),
                tor_client_common::test_support::content("heavy_weaponry"),
                tor_client_common::test_support::content("agility"),
                tor_client_common::test_support::content("light_weaponry"),
                tor_client_common::test_support::content("stealth"),
                tor_client_common::test_support::content("thievery"),
                tor_client_common::test_support::content("crafting"),
                tor_client_common::test_support::content("deduction"),
                tor_client_common::test_support::content("lore"),
                tor_client_common::test_support::content("medicine"),
                tor_client_common::test_support::content("discipline"),
                tor_client_common::test_support::content("intimidation"),
                tor_client_common::test_support::content("insight"),
                tor_client_common::test_support::content("perception"),
                tor_client_common::test_support::content("survival"),
                tor_client_common::test_support::content("deception"),
                tor_client_common::test_support::content("leadership"),
                tor_client_common::test_support::content("persuasion"),
                tor_client_common::test_support::content("spellcasting"),
            ]
            .into_iter()
            .map(|skill| SkillView { skill, rank: 0 })
            .collect(),
            defenses: DefenseView {
                physical: 13,
                cognitive: 15,
                spiritual: 11,
            },
            binding: tor_client_common::test_support::content("intellect"),
            nutrition: tor_protocol::NutritionView::Exempt,
            restorable_losses: RestorableLossesView {
                hit_dice: 0,
                attributes: AttributeView {
                    strength: 0,
                    speed: 0,
                    intellect: 0,
                    willpower: 0,
                    awareness: 0,
                    presence: 0,
                },
                maximum_health: 0,
            },
            conditions: vec![],
            health_recovery: tor_protocol::HealthRecoveryView {
                period_ticks: 1200,
                elapsed_ticks: 0,
                stopped: false,
                reset_percent: 100,
            },
            resources: [
                tor_client_common::test_support::content("stamina"),
                tor_client_common::test_support::content("focus"),
                tor_client_common::test_support::content("mana"),
            ]
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

mod strike_sequences;

mod linked_holds;
