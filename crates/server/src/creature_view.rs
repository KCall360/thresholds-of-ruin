//! Explicit authoritatively named projections; clients receive no rules catalog.
use s::{content_ids::*, rules::Ruleset};
use tor_protocol as p;
use tor_simulation as s;

pub(crate) fn label(id: &str) -> String {
    let text = id.replace(['_', '.'], " ");
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + chars.as_str()
    })
}
fn named(id: impl ToString, name: Option<&str>) -> p::ContentRef {
    let id = id.to_string();
    p::ContentRef {
        name: name.map(str::to_owned).unwrap_or_else(|| label(&id)),
        id,
    }
}
fn primitive(value: impl serde::Serialize) -> p::ContentRef {
    // Mechanism enums have their independent stable text form; content is
    // always resolved through the selected ruleset below.
    let text = serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default();
    named(text, None)
}
pub(crate) fn resource(value: s::resources::Resource) -> p::ContentRef {
    primitive(value)
}
fn skill(value: s::attributes::Skill) -> p::ContentRef {
    primitive(value)
}
fn attribute(value: s::attributes::Attribute) -> p::ContentRef {
    primitive(value)
}
fn kind(value: &TypeId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .creature_type(value)
            .map(|definition| definition.name.as_str()),
    )
}
fn subtype(value: &SubtypeId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .subtype(value)
            .map(|definition| definition.name.as_str()),
    )
}
fn talent(value: &TalentId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .talent(value)
            .map(|definition| definition.name.as_str()),
    )
}
pub(crate) fn ability(value: &AbilityId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .ability(value)
            .map(|definition| definition.name.as_str()),
    )
}
fn condition(value: &ConditionId, rules: &Ruleset) -> p::ContentRef {
    named(
        value,
        rules
            .condition(value)
            .map(|definition| definition.name.as_str()),
    )
}
fn descriptor(value: &DescriptorId, rules: Option<&Ruleset>) -> p::ContentRef {
    named(
        value,
        rules
            .and_then(|rules| {
                rules
                    .document()
                    .descriptors
                    .iter()
                    .find(|definition| &definition.id == value)
            })
            .map(|definition| definition.name.as_str()),
    )
}
fn category(value: s::combat::DamageType) -> p::DamageType {
    match value {
        s::combat::DamageType::Energy => p::DamageType::Energy,
        s::combat::DamageType::Impact => p::DamageType::Impact,
        s::combat::DamageType::Keen => p::DamageType::Keen,
        s::combat::DamageType::Spirit => p::DamageType::Spirit,
        s::combat::DamageType::Vital => p::DamageType::Vital,
    }
}
fn attributes(value: s::attributes::Attributes) -> p::AttributeView {
    use s::attributes::Attribute as A;
    p::AttributeView {
        strength: value.get(A::Strength),
        speed: value.get(A::Speed),
        intellect: value.get(A::Intellect),
        willpower: value.get(A::Willpower),
        awareness: value.get(A::Awareness),
        presence: value.get(A::Presence),
    }
}
fn hd_source(value: s::progression::HdSource, rules: &Ruleset) -> p::ContentRef {
    match value {
        s::progression::HdSource::Racial => named("racial", Some("Racial")),
        s::progression::HdSource::Class(id) => named(
            &id,
            rules.class(&id).map(|definition| definition.name.as_str()),
        ),
    }
}
#[path = "combat_diagnostics_view.rs"]
mod diagnostics;
pub(crate) use diagnostics::{combat_diagnostics, DiagnosticProjectionError};
#[path = "creature_inspection_view.rs"]
mod inspection;
pub(crate) use inspection::{attack_view, inspect, modifier};

pub(crate) fn own_stats(stats: s::creatures::OwnStats) -> p::OwnStats {
    let rules = &stats.rules;
    p::OwnStats {
        kind: kind(&stats.kind, rules),
        subtypes: stats
            .subtypes
            .iter()
            .map(|value| subtype(value, rules))
            .collect(),
        hit_dice: stats
            .hit_dice
            .into_iter()
            .map(|value| hd_source(value, rules))
            .collect(),
        attributes: attributes(stats.attributes),
        skills: s::attributes::Skill::ALL
            .into_iter()
            .map(|value| p::SkillView {
                skill: skill(value),
                rank: stats.skills.get(value),
            })
            .collect(),
        defenses: p::DefenseView {
            physical: stats.defenses.physical,
            cognitive: stats.defenses.cognitive,
            spiritual: stats.defenses.spiritual,
        },
        binding: primitive(stats.binding),
        resources: stats
            .resources
            .into_iter()
            .map(|value| p::ResourceView {
                resource: resource(value.resource),
                balance: value.balance,
                maximum: value.maximum,
                available: value.available,
                reserved: value.reserved,
            })
            .collect(),
        active_talents: stats
            .active_talents
            .iter()
            .map(|value| talent(value, rules))
            .collect(),
        dormant_talents: stats
            .dormant_talents
            .iter()
            .map(|value| talent(value, rules))
            .collect(),
        abilities: stats
            .abilities
            .iter()
            .filter_map(|id| rules.ability(id))
            .map(|def| p::AbilityView {
                id: def.id.to_string(),
                name: def.name.clone(),
                basic_attack: def.basic_attack,
                cost: def.cost.as_ref().map(|value| p::AbilityCost {
                    resource: resource(value.resource),
                    start: value.start,
                    resolution: value.resolution,
                }),
                reach: match stats.ability_reaches[&def.id] {
                    s::rules::Reach::Melee => p::AbilityReach::Melee,
                    s::rules::Reach::Sight { range } => p::AbilityReach::Sight { range },
                    s::rules::Reach::SelfOnly => p::AbilityReach::SelfOnly,
                },
                target_kind: if matches!(def.reach, s::rules::Reach::SelfOnly) {
                    p::AbilityTargetKind::SelfOnly
                } else {
                    p::AbilityTargetKind::Actor
                },
            })
            .collect(),
    }
}

pub(crate) fn outcome_status(value: s::abilities::OutcomeStatus) -> p::AbilityOutcome {
    match value {
        s::abilities::OutcomeStatus::Applied => p::AbilityOutcome::Applied,
        s::abilities::OutcomeStatus::Unaffected => p::AbilityOutcome::Unaffected,
        s::abilities::OutcomeStatus::Miss => p::AbilityOutcome::Miss,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;

    fn game() -> (s::Game, s::ActorId, s::ActorId) {
        let mut document: s::rules::RulesetDocument =
            toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap();
        let mut ability = document
            .abilities
            .iter()
            .find(|def| def.id.as_str() == "fear")
            .unwrap()
            .clone();
        ability.id = AbilityId::new("fixture_fear").unwrap();
        ability.name = "Dreadful Regard".into();
        document.abilities.push(ability);
        let rules = s::rules::Ruleset::validate(document).unwrap();
        let catalog:crate::creature_authoring::Catalog=serde_json::from_value(serde_json::json!({"species":{"inspected":{
            "kind":"humanoid","subtypes":["goblinoid"],"attributes":{"strength":2,"speed":1,"intellect":3,"willpower":2,"awareness":1,"presence":0},
            "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"damage":{"primary":{"category":"impact","sides":6},"components":[{"category":"impact","amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}]}},
            "modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"fixture_fear"}}, {"scope":"always","op":{"kind":"add","stat":{"kind":"ability","ability":"fixture_fear","stat":"range"},"value":3}}]
        }}})).unwrap();
        let catalog = catalog.compile(rules).unwrap();
        let recipe:crate::creature_authoring::BuildSpec=serde_json::from_value(serde_json::json!({
            "species":"inspected","name":"subject","faction":"neutral","binding":"intellect","hit_dice":[{"source":"racial"},{"source":"warrior"},{"source":"mage"}]
        })).unwrap();
        let build = catalog.prepare(&recipe).unwrap().instantiate(91).unwrap();
        let mut game = s::Game::two_room_in_stone(42, catalog.content().clone());
        let mut actors = Vec::new();
        for x in [1, 2] {
            let actor = game
                .spawn_actor(
                    tor_world::Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position { x, y: 1, z: 0 },
                    },
                    NonZeroU64::new(100).unwrap(),
                )
                .unwrap();
            game.configure_creature(
                actor,
                s::CreatureIdentity {
                    name: format!("subject {x}"),
                    faction: format!("team {x}"),
                },
                build.clone(),
            )
            .unwrap();
            actors.push(actor);
        }
        game.refresh_navigation();
        (game, actors[0], actors[1])
    }

    fn view(game: &s::Game, actor: s::ActorId, revision: u64) -> p::StateView {
        let (observation, scene) = game.observe_scene(actor).unwrap();
        p::StateView {
            wizard_game: false,
            revision,
            observation: crate::adapt::observation(
                observation,
                scene,
                "inspection",
                true,
                &|_| [None, None, None],
                &crate::wire_adapter::TargetScope::new(uuid::Uuid::nil(), actor),
            ),
        }
    }

    #[test]
    fn disclosed_ability_reach_uses_shared_effective_range() {
        let (game, actor, _) = game();
        let snapshot = view(&game, actor, 1);
        let stats = snapshot.observation.combat.unwrap().own_stats.unwrap();
        let ability = stats
            .abilities
            .iter()
            .find(|ability| ability.id == "fixture_fear")
            .unwrap();
        let id = AbilityId::new("fixture_fear").unwrap();
        let creature = game.creature(actor).unwrap();
        let base = match creature.build().ruleset().ability(&id).unwrap().reach {
            s::rules::Reach::Sight { range } => range,
            _ => panic!("fixture uses sight reach"),
        };
        assert_eq!(ability.reach, p::AbilityReach::Sight { range: base + 3 });
        assert_eq!(
            creature
                .derived()
                .ability_reach(&id, creature.build().ruleset())
                .unwrap(),
            s::rules::Reach::Sight { range: base + 3 }
        );
        let mut invalid = creature.derived().clone();
        invalid.ability_values.get_mut(&id).unwrap().range = 16 - i32::from(base);
        assert_eq!(
            invalid
                .ability_reach(&id, creature.build().ruleset())
                .unwrap(),
            s::rules::Reach::Sight { range: 16 }
        );
        invalid.ability_values.get_mut(&id).unwrap().range += 1;
        assert!(invalid
            .ability_reach(&id, creature.build().ruleset())
            .is_err());
    }

    #[test]
    fn zero_hd_death_snapshot_and_delta_validate() {
        let (mut game, actor, _) = game();
        let base = view(&game, actor, 1);
        let mut build = game.creature(actor).unwrap().build().clone();
        while build.remove_latest().is_some() {}
        assert!(game.rebuild_creature(actor, build).unwrap().died);
        let next = view(&game, actor, 2);
        assert!(next.observation.combat.as_ref().unwrap().dead);
        assert!(next.validate().is_ok());
        let delta = p::StateDelta::between(&base, &next).unwrap();
        assert_eq!(delta.apply(&base).unwrap(), next);
    }

    #[test]
    fn personal_stats_survive_wire_and_delta_with_qualitative_enemy_views() {
        let (mut game, actor, target) = game();
        let base = view(&game, actor, 1);
        assert!(base.validate().is_ok());
        let combat = base.observation.combat.as_ref().unwrap();
        let stats = combat.own_stats.as_ref().unwrap();
        assert_eq!(
            stats
                .hit_dice
                .iter()
                .map(|source| source.id.as_str())
                .collect::<Vec<_>>(),
            ["racial", "warrior", "mage"]
        );
        assert!(stats
            .abilities
            .iter()
            .any(|ability| ability.id == "fixture_fear" && ability.name == "Dreadful Regard"));
        assert_eq!(stats.attributes.intellect, 3);
        assert_eq!(stats.skills.len(), 19);
        assert_eq!(stats.resources.len(), 3);
        assert_eq!(stats.binding.id, "intellect");
        assert_eq!(stats.subtypes[0].id, "goblinoid");
        assert_eq!(combat.actors.len(), 1);
        let encoded = serde_json::to_string(&base).unwrap();
        assert_eq!(encoded.matches("own_stats").count(), 1);
        for private in ["health_seed", "reservation_owner", "grant_sources"] {
            assert!(!encoded.contains(private));
        }
        assert_eq!(
            serde_json::from_str::<p::StateView>(&encoded).unwrap(),
            base
        );

        game.admit_intention(
            actor,
            s::Action::UseAbility {
                id: AbilityId::new("fixture_fear").unwrap(),
                target,
            },
            s::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let next = view(&game, actor, 2);
        assert!(next.validate().is_ok());
        let stats = next
            .observation
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap();
        let focus = stats
            .resources
            .iter()
            .find(|r| r.resource.id == "focus")
            .unwrap();
        assert_eq!(
            (
                focus.balance,
                focus.maximum,
                focus.available,
                focus.reserved
            ),
            (3, 4, 2, 1)
        );
        let delta = p::StateDelta::between(&base, &next).unwrap();
        let delta: p::StateDelta =
            serde_json::from_str(&serde_json::to_string(&delta).unwrap()).unwrap();
        assert_eq!(delta.apply(&base).unwrap(), next);
        let enemy = view(&game, target, 1);
        assert!(enemy
            .observation
            .combat
            .unwrap()
            .own_stats
            .unwrap()
            .resources
            .iter()
            .all(|r| r.reserved == 0));

        for mutate in [0, 1, 2, 3, 4, 5, 6, 7] {
            let mut invalid = next.clone();
            let stats = invalid
                .observation
                .combat
                .as_mut()
                .unwrap()
                .own_stats
                .as_mut()
                .unwrap();
            match mutate {
                0 => stats.skills.push(stats.skills[0].clone()),
                1 => stats.resources[0].available = u32::MAX,
                2 => stats.resources[0].resource = stats.resources[1].resource.clone(),
                3 => stats.subtypes.push(stats.subtypes[0].clone()),
                4 => stats.skills[0].rank = 6,
                5 => {
                    stats.active_talents.push(named("fear", Some("Fear")));
                    stats.dormant_talents.push(named("fear", Some("Fear")));
                }
                6 => stats.attributes.strength = 1_001,
                7 => stats.abilities.push(stats.abilities[0].clone()),
                _ => unreachable!(),
            }
            assert_eq!(invalid.validate(), Err(p::InvalidState::InvalidCombat));
        }
    }

    #[test]
    fn private_source_inspection_has_generic_rows_and_valid_owned_ledger() {
        let (game, actor, _) = game();
        let report = inspect(&game, actor).unwrap();
        report.validate().unwrap();
        assert!(report.sources.iter().flat_map(|source|&source.rows).flat_map(|row|&row.fields).any(|field|matches!(&field.value,p::DiagnosticValue::Content{value} if value.id=="fixture_fear" && value.name=="Dreadful Regard")));
        assert_eq!(report.hit_dice.len(), 3);
    }
}
