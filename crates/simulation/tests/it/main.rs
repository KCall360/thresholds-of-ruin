//! Integration tests for this crate, compiled as one binary so each build
//! links one test executable instead of one per file.

mod abilities;
mod ability_targets;
mod action_boundaries;
mod actor_creatures;
mod actor_index;
mod ai_abilities;
mod ai_search;
mod arena_run;
mod attack_definition;
mod attributes;
mod combat;
mod condition_tokens;
mod conditions;
mod costs;
#[path = "../../test_support/creatures.rs"]
mod creature_fixture;
mod creature_inspection;
mod creature_records;
mod creature_state;
mod creature_time;
mod creatures;
mod damage;
mod diagonal;
mod dice;
mod doors;
mod enclosures;
mod equipment_modifiers;
mod health;
mod health_recovery;
mod interactions;
mod item_index;
mod items;
mod paid_abilities;
mod perception;
mod physics;
mod place_knowledge;
mod progression;
mod progression_requirements;
mod region_lifecycle;
mod resolution_diagnostics;
mod resources;
mod sight;
mod talents;
mod travel;
mod wizard_setup;

mod rules_mechanics;

mod fixture_contract;
