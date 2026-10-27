//! One ordinary validated fixture shared by host boundary and projection tests.
use super::host::{
    scenario_package::{Manifest, Package, RegionDef},
    Scenario,
};
use serde_json::json;
use std::{path::Path, sync::Arc};
pub(super) fn scenario(known: bool) -> Scenario {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut manifest: Manifest =
        toml::from_str(&std::fs::read_to_string(root.join("scenario.toml")).unwrap()).unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let effect = |effects| json!({"offset":0,"unlock":null,"check":null,"on_success":effects,"on_failure":[],"descriptors":[],"riders":[]});
    // This paid observation probe has an explicit Focus/reach contract. It is
    // deliberately independent of the shipped Warrior observation talent.
    let observe: tor_simulation::rules::AbilityDef = serde_json::from_value(json!({
        "id":"fixture_observe_plan","name":"Fixture Observe Plan","tags":[],
        "basic_attack":false,"invocation":"actor_target",
        "timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":null},
        "cost":{"resource":"focus","start":1,"resolution":1},
        "reach":{"kind":"sight","range":6},
        "strikes":{"kind":"authored","strikes":[effect(json!([
            {"kind":"observe_public_plan"},
            {"kind":"apply_qualified_condition_to_source","condition":"readied","duration":300,"ability_tag":"weapon_attack"}
        ]))]},
        "interaction": "hostile_to_target", "ai":{"priority":30,"hostile":true,"minimum_resource_percent":100,"range_preference":"any"}
    })).unwrap();
    let mut slow = observe.clone();
    slow.id = tor_simulation::content_ids::AbilityId::new("slow_plan").unwrap();
    slow.name = "Slow Plan".into();
    slow.cost = None;
    // This harmless knowledge probe must not trigger a separate provocation view.
    slow.interaction = tor_simulation::rules::Interaction::NonHostile;
    slow.timing = serde_json::from_value(
        json!({"source":{"kind":"fixed","prepare":300,"recover":100},"scaled_by":null}),
    )
    .unwrap();
    slow.strikes = serde_json::from_value(json!({"kind":"authored","strikes":[effect(json!([{"kind":"heal","amount":{"kind":"fixed","amount":0}}]))]})).unwrap();
    rules.constants.default_abilities.push(observe.id.clone());
    rules.abilities.extend([observe, slow]);
    manifest.creatures = serde_json::from_value(json!({"species":{"reader":{
        "kind":"humanoid", "behavior": {"disposition":"mean","idle":"explore","memory_ticks":1000},"attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,"strikes":[effect(json!([{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":1}}]}}]))]},
        "modifiers":[{"scope":"always","op":{"kind":"set","stat":{"kind":"resource_max","resource":"focus"},"value":10}}]
    }},"templates":{"known_plan":{"priority":0,"kind":null,"modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"slow_plan"}}]}}})).unwrap();
    let creature = |name, knows| {
        json!({"species":"reader","name":name,"faction":"neutral","binding":"intellect",
        "starting_skills":["heavy_weaponry","athletics"],"restoration":"authored", "nutrition": {"kind":"exempt"},"hit_dice":[{"source":"racial"}],
        "templates":if knows {vec!["known_plan"]} else {vec![]}})
    };
    manifest.characters[0].creature =
        Some(serde_json::from_value(creature("observer", known)).unwrap());
    let mut regions: Vec<RegionDef> = [1, 2]
        .into_iter()
        .map(|id| {
            toml::from_str(
                &std::fs::read_to_string(root.join(format!("regions/{id}.toml"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    regions[0].items.clear();
    regions[0].actors = vec![serde_json::from_value(json!({"id":2,"at":[2,1,0],"turn_ticks":100,"controller":"external","creature":creature("subject", true)})).unwrap()];
    let package =
        Package::from_parts(manifest, regions, &toml::to_string(&rules).unwrap()).unwrap();
    Scenario {
        actors: vec![],
        package: Some(Arc::new(package)),
        ..Scenario::two_room(42)
    }
}

/// Combined ordinary growth/disclosure fixture. These grants isolate authority
/// seams; they are not the final playable class catalog or calibrated XP values.
#[allow(dead_code)] // Shared fixture module; only the library harness uses growth.
pub(super) fn growth_scenario() -> Scenario {
    let mut base = scenario(false);
    let package = base.package.take().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.characters[0]
        .creature
        .as_mut()
        .unwrap()
        .future_growth = super::host::creature_authoring::FutureGrowth::Selected {
        source: super::host::creature_authoring::GrowthSource::Class {
            class: "warrior".into(),
        },
    };
    let mut rules = package.rules().unwrap().document().clone();
    rules.growth.threshold_base = 100;
    rules.growth.threshold_per_tier = 0;
    rules.growth.reward_base = 100;
    rules.growth.reward_per_relative_tier = 0;
    rules.growth.minimum_reward = 100;
    rules.growth.maximum_reward = 100;
    let mut finisher = rules
        .abilities
        .iter()
        .find(|ability| ability.id.as_str() == "slow_plan")
        .unwrap()
        .clone();
    finisher.id = tor_simulation::content_ids::AbilityId::new("growth_finisher").unwrap();
    finisher.name = "Growth finisher".into();
    finisher.interaction = tor_simulation::rules::Interaction::HostileToTarget;
    finisher.timing = serde_json::from_value(
        json!({"source":{"kind":"fixed","prepare":1,"recover":100},"scaled_by":null}),
    )
    .unwrap();
    finisher.strikes = serde_json::from_value(json!({"kind":"authored","strikes":[{
        "offset":0,"unlock":null,"check":null,"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":100}}]}}],
        "on_failure":[],"descriptors":[],"riders":[]
    }]})).unwrap();
    rules.constants.default_abilities.push(finisher.id.clone());
    rules.abilities.push(finisher);
    rules
        .classes
        .iter_mut()
        .find(|class| class.id.as_str() == "warrior")
        .unwrap()
        .modifiers
        .push(tor_simulation::rules::Modifier {
            cancellable: false,
            scope: tor_simulation::rules::Scope::Always,
            op: tor_simulation::rules::ModifierOp::GrantAbility {
                ability: tor_simulation::content_ids::AbilityId::new("slow_plan").unwrap(),
            },
        });
    base.package = Some(Arc::new(
        Package::from_parts(
            manifest,
            package.region_defs().unwrap(),
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    base
}
