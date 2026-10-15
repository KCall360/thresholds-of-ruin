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
            .and_then(|rules| rules.descriptor(value))
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
fn hd_source_kind(value: &s::progression::HdSource) -> p::HitDieSourceKind {
    match value {
        s::progression::HdSource::Racial => p::HitDieSourceKind::Racial,
        s::progression::HdSource::Class(_) => p::HitDieSourceKind::Class,
    }
}
#[path = "combat_diagnostics_view.rs"]
mod diagnostics;
pub(crate) use diagnostics::{combat_diagnostics, DiagnosticProjectionError};
#[path = "creature_inspection_view.rs"]
mod inspection;
pub(crate) use inspection::pages::{project as strike_page, PageBudget, ProjectedPage};
pub(crate) use inspection::{attack_view, inspect, modifier};

pub(crate) fn own_stats(stats: s::creatures::OwnStats) -> p::OwnStats {
    let rules = &stats.rules;
    let mut sources: Vec<_> = stats
        .hit_dice
        .classes
        .iter()
        .map(|(class, count)| p::HitDiceSourceCount {
            source_kind: p::HitDieSourceKind::Class,
            source: hd_source(s::progression::HdSource::Class(class.clone()), rules),
            count: count.get(),
        })
        .collect();
    if stats.hit_dice.racial.get() > 0 {
        sources.push(p::HitDiceSourceCount {
            source_kind: p::HitDieSourceKind::Racial,
            source: hd_source(s::progression::HdSource::Racial, rules),
            count: stats.hit_dice.racial.get(),
        });
    }
    p::OwnStats {
        kind: kind(&stats.kind, rules),
        subtypes: stats
            .subtypes
            .iter()
            .map(|value| subtype(value, rules))
            .collect(),
        hit_dice: p::HitDiceSummary {
            total: stats.hit_dice.total.get(),
            sources,
        },
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
        restorable_losses: p::RestorableLossesView {
            hit_dice: stats.restorable_hit_dice.get(),
            attributes: p::AttributeView {
                strength: stats.restorable_losses.attributes.strength,
                speed: stats.restorable_losses.attributes.speed,
                intellect: stats.restorable_losses.attributes.intellect,
                willpower: stats.restorable_losses.attributes.willpower,
                awareness: stats.restorable_losses.attributes.awareness,
                presence: stats.restorable_losses.attributes.presence,
            },
            maximum_health: stats.restorable_losses.maximum_health,
        },
        nutrition: match stats.nutrition {
            s::nutrition::NutritionView::Exempt => p::NutritionView::Exempt,
            s::nutrition::NutritionView::Required {
                reserve,
                maximum,
                hunger,
                unit_ticks,
                elapsed,
                starve_at,
            } => p::NutritionView::Required {
                reserve,
                maximum,
                unit_ticks,
                elapsed,
                starve_at,
                hunger: match hunger {
                    s::nutrition::Hunger::Nourished => p::Hunger::Nourished,
                    s::nutrition::Hunger::Hungry => p::Hunger::Hungry,
                    s::nutrition::Hunger::Weak => p::Hunger::Weak,
                    s::nutrition::Hunger::Empty => p::Hunger::Empty,
                },
            },
        },
        conditions: stats
            .conditions
            .into_iter()
            .map(|entry| p::PersonalCondition {
                condition: condition(&entry.condition, rules),
                phase: match entry.phase {
                    s::conditions::ConditionPhase::Active => p::ConditionPhase::Active,
                    s::conditions::ConditionPhase::Protection => p::ConditionPhase::Protection,
                },
                remaining_ticks: entry.remaining_ticks,
            })
            .collect(),
        health_recovery: p::HealthRecoveryView {
            period_ticks: stats.health_recovery.period(),
            elapsed_ticks: stats.health_recovery.elapsed(),
            stopped: stats.health_recovery.stopped(),
            reset_percent: stats.health_recovery_reset_percent,
        },
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
                cooldown_remaining: stats.ability_cooldowns[&def.id],
                supply_costs: def
                    .supply_costs
                    .iter()
                    .map(|cost| p::AbilitySupplyCost {
                        supply: cost.supply.to_string(),
                        name: rules
                            .supply(&cost.supply)
                            .expect("validated supply")
                            .name
                            .clone(),
                        quantity: cost.quantity,
                        access: match cost.access {
                            s::rules::SupplyAccess::CarriedOnly => p::SupplyAccess::CarriedOnly,
                            s::rules::SupplyAccess::CarriedOrGroundTouch => {
                                p::SupplyAccess::CarriedOrGroundTouch
                            }
                        },
                    })
                    .collect(),
                cost: def.cost.as_ref().map(|value| p::AbilityCost {
                    resource: resource(value.resource),
                    start: value.start,
                    resolution: value.resolution,
                }),
                reach: match stats.ability_reaches[&def.id] {
                    s::rules::Reach::Melee => p::AbilityReach::Melee,
                    s::rules::Reach::Sight { range } => p::AbilityReach::Sight { range },
                    s::rules::Reach::SelfOnly => p::AbilityReach::SelfOnly,
                    s::rules::Reach::SelfOrAlly { range } => p::AbilityReach::SelfOrAlly {
                        range: match range {
                            s::rules::AllyRange::Touch => p::AbilityAllyRange::Touch,
                            s::rules::AllyRange::Sight { range } => {
                                p::AbilityAllyRange::Sight { range }
                            }
                        },
                    },
                },
                target_kind: if def.invocation == s::abilities::InvocationKind::SelfStep {
                    p::AbilityTargetKind::SelfStep
                } else {
                    match def.reach {
                        s::rules::Reach::SelfOnly => p::AbilityTargetKind::SelfOnly,
                        s::rules::Reach::SelfOrAlly { .. } => p::AbilityTargetKind::SelfOrAlly,
                        s::rules::Reach::Melee | s::rules::Reach::Sight { .. } => {
                            p::AbilityTargetKind::Actor
                        }
                    }
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
        game_with_reach(None)
    }
    fn game_with_reach(reach: Option<s::rules::Reach>) -> (s::Game, s::ActorId, s::ActorId) {
        let mut document: s::rules::RulesetDocument =
            toml::from_str(s::rules::DEFAULT_RULESET_TOML).unwrap();
        // Exact range and Focus arithmetic are fixture contracts, independent
        // of the shipped Mage form's reach or payment resource.
        let mut ability: s::rules::AbilityDef = serde_json::from_value(serde_json::json!({
            "id":"fixture_fear","name":"Dreadful Regard","tags":[],
            "basic_attack":false,"invocation":"actor_target",
            "cost":{"resource":"focus","start":1,"resolution":1},
            "timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":"speed"},
            "reach":{"kind":"sight","range":6},
            "strikes":{"kind":"authored","strikes":[{"offset":0,
                "check":{"actor":"user","skill":{"kind":"named","skill":"intimidation"},"against":{"kind":"defense","defense":"spiritual"}},
                "on_success":[{"kind":"apply_condition","condition":"fear","duration":300}],
                "on_failure":[],"descriptors":["fear","mind_affecting"],"riders":[]}]},
            "interaction": "hostile_to_target", "ai":{"priority":30,"hostile":true,"minimum_resource_percent":100,"range_preference":"any"}
        })).unwrap();
        if let Some(reach) = reach {
            ability.reach = reach;
        }
        document.abilities.push(ability);
        let rules = s::rules::Ruleset::validate(document).unwrap();
        let catalog:crate::creature_authoring::Catalog=serde_json::from_value(serde_json::json!({"species":{"inspected":{
            "kind":"humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000},"subtypes":["goblinoid"],"attributes":{"strength":2,"speed":1,"intellect":3,"willpower":2,"awareness":1,"presence":0},
            "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"strikes":[{"offset":0,"unlock":null,"check":{"actor":"user","skill":{"kind":"selected_weapon"},"against":{"kind":"defense","defense":"physical"}},"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}}}]}}],"on_failure":[],"descriptors":[],"riders":[]}]},
            "modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"fixture_fear"}}, {"scope":"always","op":{"kind":"add","stat":{"kind":"ability","ability":"fixture_fear","stat":"range"},"value":3}}]
        }}})).unwrap();
        let catalog = catalog.compile(rules).unwrap();
        let recipe:crate::creature_authoring::BuildSpec=serde_json::from_value(serde_json::json!({
            "species":"inspected","name":"subject","faction":"neutral","binding":"intellect","starting_skills": ["deception", "persuasion"], "restoration": "choose_again", "nutrition": {"kind":"exempt"}, "hit_dice":[{"source":"racial"},{"source":"warrior"},{"source":"mage"}]
        })).unwrap();
        let build = catalog.prepare(&recipe).unwrap().instantiate(91).unwrap();
        configured_game(build)
    }

    fn configured_game(build: s::creatures::CreatureBuild) -> (s::Game, s::ActorId, s::ActorId) {
        let mut game = s::Game::two_room_in_stone(42, build.content().clone());
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
    fn typed_racial_class_growth_remains_distinct_from_racial_hit_dice() {
        use s::earned_growth::{FutureGrowth, GrowthSource};
        use s::progression::{HdLedger, HdSource};
        let (template, actor, _) = game();
        let original_creature = template.creature(actor).unwrap();
        let original = original_creature.build();
        let class = ClassId::new("racial").unwrap();
        let mut document = original.ruleset().document().clone();
        let mut definition = document.classes[0].clone();
        definition.id = class.clone();
        definition.name = "A class named racial".into();
        document.classes.push(definition);
        let content = s::creatures::Content::new(
            Ruleset::validate(document).unwrap(),
            original.content().catalog().clone(),
        )
        .unwrap();
        let mut build = s::creatures::CreatureBuild::new(
            content,
            original.species_id().clone(),
            HdLedger::seeded(vec![HdSource::Racial, HdSource::Class(class.clone())], 42).unwrap(),
            original.binding(),
            *original.starting_skills(),
        )
        .unwrap();
        build
            .set_future_growth(FutureGrowth::Selected {
                source: GrowthSource::Class { class },
            })
            .unwrap();
        let mut creature = s::creatures::CreatureState::new(build).unwrap();
        let records = creature.build().ledger().clone();
        let complete = creature.build().clone();
        let mut reduced = complete.clone();
        reduced.remove_latest().unwrap();
        creature.rebuild(reduced).unwrap();
        creature.rebuild(complete).unwrap();
        assert_eq!(creature.build().ledger(), &records);
        let (game, actor, _) = configured_game(creature.build().clone());
        let state = view(&game, actor, 1);
        state.validate().unwrap();
        let ordinary = serde_json::to_value(state).unwrap();
        assert_eq!(
            ordinary["observation"]["combat"]["own_stats"]["hit_dice"],
            serde_json::json!({"total":"2","sources":[
                {"source_kind":"class","source":{"id":"racial","name":"A class named racial"},"count":"1"},
                {"source_kind":"racial","source":{"id":"racial","name":"Racial"},"count":"1"}
            ]})
        );
        let detailed = inspect(&game, actor, 1).unwrap();
        detailed.validate().unwrap();
        let detailed = serde_json::to_value(detailed).unwrap();
        assert_eq!(detailed["hit_dice"][0]["source_kind"], "racial");
        assert_eq!(detailed["hit_dice"][1]["source_kind"], "class");
    }

    #[test]
    fn ordinary_hit_dice_projection_stays_one_row_for_repeated_sources() {
        for count in [2, 256] {
            let (template, actor, _) = game();
            let original_creature = template.creature(actor).unwrap();
            let original = original_creature.build();
            let build = s::creatures::CreatureBuild::new(
                original.content().clone(),
                original.species_id().clone(),
                s::progression::HdLedger::seeded(vec![s::progression::HdSource::Racial; count], 42)
                    .unwrap(),
                original.binding(),
                *original.starting_skills(),
            )
            .unwrap();
            let (game, actor, _) = configured_game(build);
            let before = s::diagnostics::work_counts();
            for _ in 0..3 {
                let state = view(&game, actor, 1);
                state.validate().unwrap();
                let value = serde_json::to_value(state).unwrap();
                assert_eq!(
                    value["observation"]["combat"]["own_stats"]["hit_dice"],
                    serde_json::json!({"total":count.to_string(),"sources":[{"source_kind":"racial","source":{"id":"racial","name":"Racial"},"count":count.to_string()}]})
                );
                let encoded = serde_json::to_string(&value).unwrap();
                for private in ["health_seed", "purchases", "ordinal"] {
                    assert!(!encoded.contains(private));
                }
            }
            let after = s::diagnostics::work_counts();
            assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
            assert_eq!(
                after.advancement_choice_visits,
                before.advancement_choice_visits
            );
            assert_eq!(
                after.health_provenance_copies,
                before.health_provenance_copies
            );
            assert_eq!(after.creature_build_copies, before.creature_build_copies);
        }
    }

    #[test]
    fn restorable_loss_projection_is_fixed_size_and_owned_by_the_observer() {
        let (game, actor, other) = game();
        let (observation, _) = game.observe_scene(actor).unwrap();
        let mut stats = observation.combat.unwrap().own_stats.unwrap();
        stats.restorable_losses = s::creatures::RestorableLosses {
            attributes: s::attributes::ByAttribute {
                strength: 1,
                speed: 2,
                intellect: 3,
                willpower: 4,
                awareness: 5,
                presence: 6,
            },
            maximum_health: 7,
        };
        stats.restorable_hit_dice = tor_simulation::HdCount(8);
        let projected = own_stats(stats);
        let value = serde_json::to_value(&projected).unwrap();
        assert_eq!(
            value["restorable_losses"],
            serde_json::json!({
                "attributes":{"strength":1,"speed":2,"intellect":3,"willpower":4,"awareness":5,"presence":6},
                "maximum_health":7,
                "hit_dice":"8"
            })
        );
        let unrelated = serde_json::to_value(view(&game, other, 0)).unwrap();
        assert_eq!(
            unrelated["observation"]["combat"]["own_stats"]["restorable_losses"]["maximum_health"],
            0
        );
        assert_eq!(
            unrelated["observation"]["combat"]["own_stats"]["restorable_losses"]["hit_dice"],
            "0"
        );
        for visible in unrelated["observation"]["combat"]["actors"]
            .as_array()
            .unwrap()
        {
            assert!(visible.get("restorable_losses").is_none());
        }
    }

    #[test]
    fn personal_condition_projection_never_transmits_private_causer_or_foreign_status() {
        let (mut game, actor, causer) = game();
        game.apply_condition(actor, &ConditionId::new("fear").unwrap(), causer, 200)
            .unwrap();
        let owned = view(&game, actor, 1);
        owned.validate().unwrap();
        let json = serde_json::to_value(&owned).unwrap();
        let conditions = &json["observation"]["combat"]["own_stats"]["conditions"];
        assert_eq!(conditions.as_array().unwrap().len(), 1);
        assert_eq!(conditions[0]["condition"]["id"], "fear");
        assert_eq!(conditions[0]["phase"], "active");
        assert_eq!(conditions[0]["remaining_ticks"], "200");
        assert_eq!(conditions[0].as_object().unwrap().len(), 3);
        for hidden in [
            "source",
            "qualification",
            "opponent",
            "reserved_by",
            "stacks",
            "order",
        ] {
            assert!(conditions[0].get(hidden).is_none());
        }
        let foreign = view(&game, causer, 1);
        assert!(foreign
            .observation
            .combat
            .unwrap()
            .own_stats
            .unwrap()
            .conditions
            .is_empty());
        let mut shared = s::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let restored =
            s::Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
        assert_eq!(view(&restored, actor, 1), owned);
    }

    #[test]
    fn hidden_attacker_token_reservation_preserves_full_personal_projection_events_and_readiness() {
        let (mut idle, attacker, subject) = game();
        let subject_at = tor_world::Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 4, y: 1, z: 0 },
        };
        idle.teleport(subject, subject_at).unwrap();
        idle.set_region_light(tor_world::RegionId(1), false)
            .unwrap();
        idle.set_cell_light(subject_at, true).unwrap();
        idle.apply_condition(subject, &ConditionId::new("exposed").unwrap(), subject, 500)
            .unwrap();
        assert!(idle
            .observe(subject)
            .unwrap()
            .visible_actors
            .iter()
            .all(|actor| actor.id != attacker));
        let mut preparing = idle.clone();
        preparing
            .admit_intention(
                attacker,
                s::Action::UseAbility {
                    id: AbilityId::new("fixture_fear").unwrap(),
                    invocation: s::abilities::AbilityInvocation::ActorTarget { target: subject },
                    supplies: Default::default(),
                },
                s::IntentionOrigin::Human,
            )
            .unwrap();
        preparing.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(preparing.tick(), idle.tick());
        assert!(preparing
            .creature(subject)
            .unwrap()
            .conditions()
            .instances()
            .any(|entry| entry.reserved_by == Some(attacker)));
        assert!(preparing
            .observe(subject)
            .unwrap()
            .visible_actors
            .iter()
            .all(|actor| actor.id != attacker));
        let before = serde_json::to_value(view(&idle, subject, 1)).unwrap();
        let after = serde_json::to_value(view(&preparing, subject, 1)).unwrap();
        let before = &before["observation"]["combat"];
        let after = &after["observation"]["combat"];
        assert_eq!(before["own_stats"], after["own_stats"]);
        assert_eq!(before["own_stats"]["conditions"], serde_json::json!([]));
        assert_eq!(before["events"], after["events"]);
        for field in [
            "preparation_remaining",
            "preparation_active",
            "recovery_remaining",
            "sequence",
        ] {
            assert_eq!(before[field], after[field], "{field}");
        }
        assert_eq!(
            idle.intention_input(subject),
            preparing.intention_input(subject)
        );
        let mut shared = s::checkpoint::SharedState::default();
        let snapshot = preparing.checkpoint(&mut shared);
        let restored =
            s::Game::restore_checkpoint(snapshot, &shared, preparing.content().clone()).unwrap();
        assert_eq!(view(&restored, subject, 1), view(&preparing, subject, 1));
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
            creature.derived().ability_reach(&id).unwrap(),
            s::rules::Reach::Sight { range: base + 3 }
        );
        let cached = creature.derived().ability_reaches[&id].clone();
        assert_eq!(creature.derived().ability_reach(&id).unwrap(), cached);
    }

    #[test]
    fn phase5_supportive_reach_discloses_scoped_self_and_no_faction_identity() {
        for range in [
            s::rules::AllyRange::Touch,
            s::rules::AllyRange::Sight { range: 6 },
        ] {
            let (game, actor, _) = game_with_reach(Some(s::rules::Reach::SelfOrAlly { range }));
            let state = view(&game, actor, 1);
            state.validate().unwrap();
            let combat = state.observation.combat.as_ref().unwrap();
            let ability = combat
                .own_stats
                .as_ref()
                .unwrap()
                .abilities
                .iter()
                .find(|a| a.id == "fixture_fear")
                .unwrap();
            assert_eq!(ability.target_kind, p::AbilityTargetKind::SelfOrAlly);
            assert_eq!(
                ability.reach,
                p::AbilityReach::SelfOrAlly {
                    range: match range {
                        s::rules::AllyRange::Touch => p::AbilityAllyRange::Touch,
                        s::rules::AllyRange::Sight { range } =>
                            p::AbilityAllyRange::Sight { range: range + 3 },
                    }
                }
            );
            assert!(combat
                .actors
                .iter()
                .all(|other| !other.allied && !other.hostile));
            let encoded = serde_json::to_string(&state).unwrap();
            assert!(
                !encoded.contains("team 1")
                    && !encoded.contains("team 2")
                    && !encoded.contains("faction")
            );
            let action = p::Action::UseAbility {
                id: "fixture_fear".into(),
                invocation: tor_protocol::AbilityInvocation::ActorTarget {
                    target: state.observation.self_target,
                },
                supplies: Default::default(),
            };
            let observed = game.observe(actor).unwrap();
            let scope = crate::wire_adapter::TargetScope::new(uuid::Uuid::nil(), actor);
            let decoded = crate::wire_adapter::decode_action(&action, &scope, &observed).unwrap();
            assert!(
                matches!(decoded, crate::actions::Action::UseAbility { invocation: crate::actions::AbilityInvocation::ActorTarget { target }, .. } if target == actor)
            );
            let foreign_scope =
                crate::wire_adapter::TargetScope::new(uuid::Uuid::from_u128(1), actor);
            assert!(
                crate::wire_adapter::decode_action(&action, &foreign_scope, &observed).is_err()
            );
        }
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
                .sources
                .iter()
                .map(|row| (row.source.id.as_str(), row.count))
                .collect::<Vec<_>>(),
            [("mage", 1), ("warrior", 1), ("racial", 1)]
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
                invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget { target },
                supplies: Default::default(),
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

    fn many_talent_build(
        count: usize,
        points: u16,
    ) -> Result<s::creatures::CreatureBuild, s::creatures::BuildError> {
        use s::content_ids::{PointPoolId, TalentId};
        use s::creatures::{
            AdvancementPurchase, BuildAdvancement, Content, CreatureBuild, HdChoices,
            PurchaseSelection, RestorationPolicy,
        };
        use s::progression::{HdLedger, HdSource};
        let (game, actor, _) = game();
        let original = game.creature(actor).unwrap().build().clone();
        let mut document = original.ruleset().document().clone();
        document.constants.racial_point_grants[0].points = points;
        document.talents = (0..count)
            .map(|index| s::rules::TalentDef {
                form_choices: Vec::new(),
                id: TalentId::new(format!("talent_{index:04}")).unwrap(),
                name: format!("Talent {index}"),
                prerequisites: vec![],
                requirement: s::rules::Requirement::TotalHd {
                    minimum: tor_simulation::HdCount(1),
                },
                modifiers: vec![],
            })
            .collect();
        let content = Content::new(
            s::rules::Ruleset::validate(document).unwrap(),
            original.content().catalog().clone(),
        )
        .unwrap();
        let choices = (0..count.div_ceil(usize::from(points)))
            .map(|owner| HdChoices {
                purchases: (owner * usize::from(points)
                    ..((owner + 1) * usize::from(points)).min(count))
                    .map(|index| AdvancementPurchase {
                        pool: PointPoolId::new("advancement").unwrap(),
                        selection: PurchaseSelection::Talent(
                            TalentId::new(format!("talent_{index:04}")).unwrap(),
                        ),
                    })
                    .collect(),
                attribute: None,
            })
            .collect::<Vec<_>>();
        CreatureBuild::from_recorded(
            content,
            original.species_id().clone(),
            original.initial_attributes(),
            HdLedger::seeded(vec![HdSource::Racial; choices.len()], 42).unwrap(),
            original.binding(),
            BuildAdvancement {
                starting_skills: *original.starting_skills(),
                choices,
                restoration: RestorationPolicy::ChooseAgain,
            },
            vec![],
        )
    }

    #[test]
    fn multi_point_build_ownership_boundary_round_trips_save_wire_inspection_and_client() {
        assert_eq!(s::limits::OWNED_TALENTS, p::MAX_OWNED_TALENTS);
        assert!(
            many_talent_build(257, 129).is_err(),
            "257 owned talents exceed the shared construction/wire contract"
        );
        assert!(
            many_talent_build(258, 129).is_err(),
            "two grants of129 do not expand total talent ownership"
        );
        let build = many_talent_build(256, 129).unwrap();
        let record = s::creatures::record::BuildRecord::capture(&build);
        let record: s::creatures::record::BuildRecord =
            serde_json::from_str(&serde_json::to_string(&record).unwrap()).unwrap();
        assert_eq!(record.restore(build.content().clone()).unwrap(), build);
        let (game, actor, _) = configured_game(build);
        let state = view(&game, actor, 1);
        assert_eq!(
            state
                .observation
                .combat
                .as_ref()
                .unwrap()
                .own_stats
                .as_ref()
                .unwrap()
                .active_talents
                .len(),
            256
        );
        state
            .validate()
            .expect("valid two-HD multi-point build must be representable");
        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: p::StateView = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded, state);
        decoded.validate().unwrap();
        let report = inspect(&game, actor, 0).unwrap();
        report.validate().unwrap();
        let lines = tor_client_common::inspection::lines(&report).unwrap();
        assert!(lines.iter().any(|line| line.contains("Talent 255")));
    }

    #[test]
    fn maximal_point_ledger_has_a_bounded_complete_inspection_and_client_rendering() {
        use s::attributes::Skill;
        use s::content_ids::{PointPoolId, TalentId};
        use s::creatures::{
            AdvancementPurchase, BuildAdvancement, Content, CreatureBuild, HdChoices,
            PurchaseSelection,
        };
        use s::progression::{HdLedger, HdSource};
        use s::rules::{PointGrant, PointPoolDef, PurchaseKind, Ruleset};
        let original = many_talent_build(256, 256).unwrap();
        let mut document = original.ruleset().document().clone();
        let escaped_name = "\"\\".repeat(128);
        assert_eq!(escaped_name.len(), 256);
        for (index, definition) in document.talents.iter_mut().enumerate() {
            definition.id = TalentId::new(format!("talent_{index:03}_{}", "x".repeat(49))).unwrap();
            definition.name = escaped_name.clone();
        }
        document.point_pools = (0..16)
            .map(|index| PointPoolDef {
                id: PointPoolId::new(format!("pool_{index:02}_{}", "x".repeat(52))).unwrap(),
                name: escaped_name.clone(),
                allowed: vec![PurchaseKind::SkillRank, PurchaseKind::Talent],
            })
            .collect();
        let pools = document
            .point_pools
            .iter()
            .map(|definition| definition.id.clone())
            .collect::<Vec<_>>();
        document.constants.racial_point_grants = pools
            .iter()
            .map(|pool| PointGrant {
                pool: pool.clone(),
                points: 16,
            })
            .collect();
        for class in &mut document.classes {
            class.point_grants.clear();
        }
        let mut choices = vec![HdChoices::default(); 256];
        choices[0].purchases = document
            .talents
            .iter()
            .enumerate()
            .map(|(index, definition)| AdvancementPurchase {
                pool: pools[index / 16].clone(),
                selection: PurchaseSelection::Talent(definition.id.clone()),
            })
            .collect();
        let mut paid_ranks = Vec::new();
        for skill in Skill::ALL {
            let starting = usize::from(original.starting_skills().contains(&skill));
            paid_ranks.extend(std::iter::repeat_n(skill, 5 - starting));
        }
        assert_eq!(paid_ranks.len(), 93);
        choices[1].purchases = paid_ranks
            .into_iter()
            .enumerate()
            .map(|(index, skill)| AdvancementPurchase {
                pool: pools[index / 16].clone(),
                selection: PurchaseSelection::SkillRank(skill),
            })
            .collect();
        let content = Content::new(
            Ruleset::validate(document).unwrap(),
            original.content().catalog().clone(),
        )
        .unwrap();
        let build = CreatureBuild::from_recorded(
            content,
            original.species_id().clone(),
            original.initial_attributes(),
            HdLedger::seeded(vec![HdSource::Racial; 256], 42).unwrap(),
            original.binding(),
            BuildAdvancement {
                choices,
                ..original.advancement().clone()
            },
            vec![],
        )
        .unwrap();
        let (game, actor, _) = configured_game(build);
        let report = inspect(&game, actor, 0).unwrap();
        report.validate().unwrap();
        assert_eq!(
            report
                .hit_dice
                .iter()
                .map(|die| die.point_pools.len())
                .sum::<usize>(),
            4096
        );
        assert_eq!(
            report
                .hit_dice
                .iter()
                .map(|die| die.purchases.len())
                .sum::<usize>(),
            349
        );
        let lines = tor_client_common::inspection::lines(&report).unwrap();
        let client_bytes = lines.iter().map(String::len).sum::<usize>();
        assert!(lines.len() < 6000);
        assert!(client_bytes < 4 * 1024 * 1024);
        let message = p::ServerMessage::CreatureInspection {
            context: p::ReplyContext {
                input: p::InputContext {
                    stream: p::StreamContext {
                        stream: p::StreamId("bounded-ledger".into()),
                        epoch: 1,
                    },
                    readiness_revision: 1,
                },
                actor: p::ActorId(actor.0),
                branch: p::BranchId("fixture".into()),
                cursor: p::StreamCursor {
                    sequence: 1,
                    tick: game.tick(),
                },
                revision: 1,
            },
            request_id: "maximum-ledger".into(),
            report: Box::new(report),
        };
        let encoded = p::encode_response(&message, None, p::MAX_RESPONSE_BYTES).unwrap();
        assert!(
            encoded.text.len() < 4 * 1024 * 1024,
            "ledger should leave substantial space for independent diagnostics"
        );
        let decoded = p::decode_response(&encoded.text).unwrap();
        assert_eq!(decoded, message);
        eprintln!(
            "maximum ledger: server response {} bytes, client {} rows / {} bytes",
            encoded.text.len(),
            lines.len(),
            client_bytes
        );
    }

    #[test]
    fn private_source_inspection_has_generic_rows_and_valid_owned_ledger() {
        let (game, actor, _) = game();
        let report = inspect(&game, actor, 0).unwrap();
        report.validate().unwrap();
        assert!(report.sources.iter().flat_map(|source|&source.rows).flat_map(|row|&row.fields).any(|field|matches!(&field.value,p::DiagnosticValue::Content{value} if value.id=="fixture_fear" && value.name=="Dreadful Regard")));
        assert_eq!(report.hit_dice.len(), 3);
    }
    #[test]
    fn ordered_strikes_private_inspection_discloses_authored_source_structure_only_when_authorized()
    {
        let (game, actor, _) = game();
        let report = inspect(&game, actor, 0).unwrap();
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(value["natural_attack"]["source"], value["species"]);
        assert_eq!(value["natural_attack"]["strike_count"], 1);
        assert!(value["natural_attack"].get("strikes").is_none());
        let page = strike_page(
            &game,
            actor,
            &p::StrikeSourceSelector::Natural {},
            None,
            None,
        )
        .unwrap();
        let entries = serde_json::to_value(page.entries).unwrap();
        assert_eq!(entries[0]["kind"], "strike_header");
        assert_eq!(entries[0]["strike"], 0);
        assert_eq!(entries[0]["offset"], "0");
        assert_eq!(entries[0]["check"]["id"], "primary_check");
        assert_eq!(entries[0]["success_count"], 1);
        assert_eq!(entries[1]["row"]["id"], "damage");
        assert_eq!(entries[0]["failure_count"], 0);
        assert_eq!(entries[0]["rider_count"], 0);
        assert!(value["natural_attack"].get("damage").is_none());
        assert_eq!(value["selected_attack"]["attack"]["kind"], "natural");
        let abilities = value["ability_strikes"]
            .as_array()
            .expect("authored ability sources");
        assert_eq!(abilities.len(), report.stats.abilities.len());
        assert!(abilities
            .iter()
            .any(|ability| ability["ability"] == "basic_melee"
                && ability["source"]["kind"] == "selected_melee"));
        assert!(abilities
            .iter()
            .any(|ability| ability["ability"] == "fixture_fear"
                && ability["source"]["kind"] == "authored"));
        report.validate().unwrap();
        let ordinary = serde_json::to_value(view(&game, actor, 1))
            .unwrap()
            .to_string();
        for private in [
            "natural_attack",
            "ability_strikes",
            "on_success",
            "on_failure",
            "riders",
        ] {
            assert!(
                !ordinary.contains(private),
                "private source escaped inspection: {private}"
            );
        }
    }
    #[test]
    fn phase5_own_stats_disclose_current_cooldown_and_known_supply_without_claims() {
        let (mut game, actor, other) = game();
        let at = game.observe(actor).unwrap().location;
        let mut bandage = s::ItemSpec::ordinary("clean bandage".into());
        bandage.supply = Some(SupplyId::new("bandage").unwrap());
        game.place_item_stack(10, at, Some(actor), 1, bandage)
            .unwrap();
        game.admit_intention(
            actor,
            s::Action::UseAbility {
                id: AbilityId::new("bind_wounds").unwrap(),
                invocation: tor_simulation::abilities::AbilityInvocation::ActorTarget {
                    target: actor,
                },
                supplies: s::supplies::SupplySelection::Carried,
            },
            s::IntentionOrigin::Human,
        )
        .unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        let state = view(&game, actor, 2);
        let care = state
            .observation
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap()
            .abilities
            .iter()
            .find(|ability| ability.id == "bind_wounds")
            .unwrap();
        assert_eq!(care.cooldown_remaining, 1200);
        assert_eq!(
            care.supply_costs,
            vec![p::AbilitySupplyCost {
                supply: "bandage".into(),
                name: "Bandage".into(),
                quantity: 1,
                access: p::SupplyAccess::CarriedOrGroundTouch,
            }]
        );
        assert_eq!(
            state.observation.inventory[0].known_supply,
            Some(p::SupplyView {
                id: "bandage".into(),
                name: "Bandage".into()
            })
        );
        let encoded = serde_json::to_string(&state).unwrap();
        for private in [
            "item_payment",
            "cost_ordinal",
            "claimant",
            "free_quantity",
            "holds",
        ] {
            assert!(
                !encoded.contains(private),
                "private authority escaped: {private}"
            );
        }
        let other = view(&game, other, 2);
        assert_eq!(
            other
                .observation
                .combat
                .unwrap()
                .own_stats
                .unwrap()
                .abilities
                .iter()
                .find(|ability| ability.id == "bind_wounds")
                .unwrap()
                .cooldown_remaining,
            0
        );
    }
}

#[cfg(test)]
#[path = "strike_inspection_tests.rs"]
pub(crate) mod strike_page_tests;
