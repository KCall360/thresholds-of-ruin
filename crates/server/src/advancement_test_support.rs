//! Ordinary authored fixtures shared by owner Service and durable Engine tests.
use crate::Scenario;
use serde_json::{json, Value};
use std::sync::Arc;

pub(crate) fn scenario() -> Scenario {
    use crate::scenario_package::{Manifest, Package, RegionDef};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut manifest: Value = serde_json::to_value(
        toml::from_str::<Manifest>(&std::fs::read_to_string(root.join("scenario.toml")).unwrap())
            .unwrap(),
    )
    .unwrap();
    manifest["factions"] = json!({"allies":[]});
    manifest["archetypes"]["ration"] =
        json!({"name":"ordinary ration","class":"food","stackable":true,"supply":"ration"});
    manifest["creatures"] = json!({"species":{"learner":{
        "kind":"humanoid","attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,"strikes":[{
            "offset":0,"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{
                "category":"impact","amount":{"kind":"fixed","amount":1}}]}}],"on_failure":[],"descriptors":[],"riders":[]}]}
    }}});
    let creature = |name: &str| {
        json!({"species":"learner","name":name,"faction":"allies","binding":"intellect",
        "starting_skills":["deception","persuasion"],"restoration":"authored", "nutrition": {"kind":"exempt"},
        "hit_dice":[{"source":"racial"},{"source":"racial"}]})
    };
    manifest["characters"][0]["creature"] = creature("owner");
    let mut regions: Vec<RegionDef> = [1, 2]
        .into_iter()
        .map(|id| {
            toml::from_str(
                &std::fs::read_to_string(root.join(format!("regions/{id}.toml"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["actors"] = json!([{"id":2,"at":[3,1,0],"turn_ticks":100,"controller":"external","creature":creature("partner")}]);
    region["items"] =
        json!([{"id":10,"at":[1,1,0],"carried_by":1,"archetype":"ration","quantity":2}]);
    regions[0] = serde_json::from_value(region).unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        regions,
        tor_simulation::rules::DEFAULT_RULESET_TOML,
    )
    .unwrap();
    Scenario {
        actors: vec![],
        package: Some(Arc::new(package)),
        ..Scenario::two_room(42)
    }
}

/// A funded, removable authored purchase and one last-unit ration. No developer
/// mutation is used to establish the replacement or its injury/Health contract.
pub(crate) fn replacement_scenario(lethal: bool) -> Scenario {
    use crate::scenario_package::Package;
    let base = scenario();
    let package = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    let mut rules: Value = serde_json::to_value(
        toml::from_str::<tor_simulation::rules::RulesetDocument>(
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )
        .unwrap(),
    )
    .unwrap();
    rules["talents"].as_array_mut().unwrap().push(json!({
        "id":"health_foundation","name":"Health Foundation","prerequisites":[],
        "requirement":{"kind":"total_hd","minimum":1},
        "modifiers":[{"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":if lethal {100} else {10}}}]
    }));
    manifest["characters"][0]["creature"]["hit_dice"][0]["purchases"] = json!([
        {"pool":"advancement","selection":{"kind":"talent","talent":"health_foundation"}}
    ]);
    let mut regions = package.region_defs().unwrap();
    let mut first = serde_json::to_value(&regions[0]).unwrap();
    first["items"][0]["quantity"] = json!(1);
    if lethal {
        // A bounded +100 source and 100-damage dose make the lethal control
        // independent of the two small HD rolls. Positive current Health and
        // the lethal replacement preview are asserted after legitimate recovery.
        manifest["archetypes"]["injury"] = json!({"name":"injury dose","class":"potion",
            "consumable":{"effects":[{"kind":"damage","damage":{"kind":"components","components":[{
                "category":"vital","amount":{"kind":"fixed","amount":100}}]}}]}});
        first["items"].as_array_mut().unwrap().push(json!({
            "id":11,"at":[1,1,0],"carried_by":1,"archetype":"injury","quantity":1
        }));
        manifest["archetypes"]["health_ring"] = json!({"name":"Health ring","class":"ring",
        "equipment":{"slot":"ring","attack":null,"modifiers":[
            {"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":5}},
            {"scope":"always","op":{"kind":"add","stat":{"kind":"resource_max","resource":"stamina"},"value":3}}
        ]}});
        first["items"].as_array_mut().unwrap().push(json!({
            "id":12,"at":[1,1,0],"carried_by":1,"archetype":"health_ring","quantity":1
        }));
    }
    regions[0] = serde_json::from_value(first).unwrap();
    let rules: tor_simulation::rules::RulesetDocument = serde_json::from_value(rules).unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        regions,
        &toml::to_string(&rules).unwrap(),
    )
    .unwrap();
    Scenario {
        package: Some(Arc::new(package)),
        ..base
    }
}
/// Maximum ledger/balance/catalog rows, compiled through source-owned authoring.
/// A single root reaches the 256-node expression bound; the other owned talents
/// form a one-parent chain. Making every expression maximal exceeded the 960 KiB
/// saved-metadata cap, so that rejected combination is not a legal joint bound.
/// Returns source bytes for explicit ingestion-budget and workload reporting.
pub(crate) fn maximum_scenario() -> (Scenario, usize) {
    use crate::scenario_package::Package;
    use tor_simulation::attributes::Skill;
    let base = scenario();
    let package = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    let mut rules: Value = serde_json::to_value(
        toml::from_str::<tor_simulation::rules::RulesetDocument>(
            tor_simulation::rules::DEFAULT_RULESET_TOML,
        )
        .unwrap(),
    )
    .unwrap();
    let pool = |i: usize| {
        if i == 0 {
            "advancement".to_owned()
        } else {
            format!("pool_{i:02}")
        }
    };
    rules["constants"]["racial_point_grants"] = json!((0..16)
        .map(|i| json!({"pool":pool(i),"points":if i == 0 {241} else {1}}))
        .collect::<Vec<_>>());
    rules["point_pools"] = json!((0..16)
        .map(|i| json!({"id":pool(i),"name":format!("Pool {i}"),"allowed":["skill_rank","talent"]}))
        .collect::<Vec<_>>());
    rules["talents"] = json!([]);
    let rules: tor_simulation::rules::RulesetDocument = serde_json::from_value(rules).unwrap();
    let mut source = toml::to_string(&rules)
        .unwrap()
        .replacen("talents = []\n", "", 1);
    // Compact valid inline requirements avoid making the serializer's table
    // layout, rather than authoring limits, the limiting factor in this fixture.
    for i in 0..4096 {
        let requirement = if i == 0 {
            format!(
                "{{kind=\"all\",requirements=[{}]}}",
                vec!["{kind=\"total_hd\",minimum=1}"; 255].join(",")
            )
        } else if i < 256 {
            format!("{{kind=\"owned_talent\",talent=\"talent_{:04}\"}}", i - 1)
        } else {
            "{kind=\"total_hd\",minimum=1}".to_owned()
        };
        source.push_str(&format!("\n[[talents]]\nid=\"talent_{i:04}\"\nname=\"Talent {i}\"\nprerequisites=[]\nrequirement={requirement}\nmodifiers=[]\n"));
    }
    assert!(
        source.len() <= 8 * 1024 * 1024,
        "real rules source ingestion cap"
    );
    let mut hit_dice = vec![json!({"source":"racial","purchases":[]}); 256];
    for i in 0..256 {
        hit_dice[i/16]["purchases"].as_array_mut().unwrap().push(json!({"pool":pool(i%16),"selection":{"kind":"talent","talent":format!("talent_{i:04}")}}));
    }
    let mut skill_purchases = 0;
    for skill in Skill::ALL {
        let starting = usize::from(matches!(skill, Skill::Deception | Skill::Persuasion));
        for _ in starting..5 {
            let owner = 16 + skill_purchases / 16;
            hit_dice[owner]["purchases"].as_array_mut().unwrap().push(json!({"pool":pool(skill_purchases%16),"selection":{"kind":"skill_rank","skill":skill.as_str()}}));
            skill_purchases += 1;
        }
    }
    assert_eq!(skill_purchases, 93);
    manifest["characters"][0]["creature"]["hit_dice"] = json!(hit_dice);
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        package.region_defs().unwrap(),
        &source,
    )
    .unwrap();
    (
        Scenario {
            package: Some(Arc::new(package)),
            ..base
        },
        source.len(),
    )
}

/// Fixed choices compiled through ordinary source authoring. Maximum labels use
/// the exact source byte limits and JSON-escaped quote/backslash characters.
pub(crate) fn form_scenario(count: usize, maximum_labels: bool) -> Scenario {
    use crate::scenario_package::Package;
    let base = scenario();
    let package = base.package.as_ref().unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let root = if maximum_labels {
        "r".repeat(60)
    } else {
        "choice_root".into()
    };
    let label = |ordinary: String| {
        if maximum_labels {
            "\\\"".repeat(128)
        } else {
            ordinary
        }
    };
    let forms: Vec<_> = (0..count)
        .map(|index| tor_simulation::rules::FormDef {
            id: tor_simulation::content_ids::FormId::new(if maximum_labels {
                format!("f{index:059}")
            } else {
                format!("choice.form_{index}")
            })
            .unwrap(),
            name: label(format!("Form {index}")),
            ability: tor_simulation::content_ids::AbilityId::new(if index == 0 {
                "spark_lance_fire"
            } else {
                "falter"
            })
            .unwrap(),
        })
        .collect();
    rules.talents.push(tor_simulation::rules::TalentDef {
        id: tor_simulation::content_ids::TalentId::new(root).unwrap(),
        name: label("Choice Root".into()),
        prerequisites: Vec::new(),
        requirement: tor_simulation::rules::Requirement::TotalHd { minimum: 1 },
        modifiers: Vec::new(),
        form_choices: forms.iter().map(|form| form.id.clone()).collect(),
    });
    rules.forms.extend(forms);
    let package = Package::from_parts(
        package.manifest.clone(),
        package.region_defs().unwrap(),
        &toml::to_string(&rules).unwrap(),
    )
    .unwrap();
    Scenario {
        package: Some(Arc::new(package)),
        ..base
    }
}
