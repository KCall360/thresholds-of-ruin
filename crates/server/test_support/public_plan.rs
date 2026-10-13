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
    let mut observe = rules
        .abilities
        .iter()
        .find(|ability| ability.id.as_str() == "fear")
        .unwrap()
        .clone();
    observe.id = tor_simulation::content_ids::AbilityId::new("read_the_rhythm").unwrap();
    observe.name = "Read the Rhythm".into();
    observe.tags.clear();
    observe.timing = serde_json::from_value(
        json!({"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":null}),
    )
    .unwrap();
    observe.cost =
        serde_json::from_value(json!({"resource":"focus","start":1,"resolution":1})).unwrap();
    observe.strikes = serde_json::from_value(json!({"kind":"authored","strikes":[effect(json!([
        {"kind":"observe_public_plan"},
        {"kind":"apply_qualified_condition_to_source","condition":"readied","duration":300,"ability_tag":"weapon_attack"}
    ]))]})).unwrap();
    let mut slow = observe.clone();
    slow.id = tor_simulation::content_ids::AbilityId::new("slow_plan").unwrap();
    slow.name = "Slow Plan".into();
    slow.cost = None;
    slow.timing = serde_json::from_value(
        json!({"source":{"kind":"fixed","prepare":300,"recover":100},"scaled_by":null}),
    )
    .unwrap();
    slow.strikes = serde_json::from_value(json!({"kind":"authored","strikes":[effect(json!([{"kind":"heal","amount":{"kind":"fixed","amount":0}}]))]})).unwrap();
    rules.constants.default_abilities.push(observe.id.clone());
    rules.abilities.extend([observe, slow]);
    manifest.creatures = serde_json::from_value(json!({"species":{"reader":{
        "kind":"humanoid","attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,"strikes":[effect(json!([{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":1}}]}}]))]},
        "modifiers":[{"scope":"always","op":{"kind":"set","stat":{"kind":"resource_max","resource":"focus"},"value":10}}]
    }},"templates":{"known_plan":{"priority":0,"kind":null,"modifiers":[{"scope":"always","op":{"kind":"grant_ability","ability":"slow_plan"}}]}}})).unwrap();
    let creature = |name, knows| {
        json!({"species":"reader","name":name,"faction":"neutral","binding":"intellect",
        "starting_skills":["heavy_weaponry","athletics"],"restoration":"authored","hit_dice":[{"source":"racial"}],
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
