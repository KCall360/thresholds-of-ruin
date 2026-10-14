//! Test-owned accepted sources for bounded private strike inspection.
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};
use tor_protocol as p;

pub(crate) fn descriptor_id(index: usize) -> String {
    format!("d{index:03}")
}

pub(crate) fn scenario(abilities: usize, maximum_source: bool) -> crate::Scenario {
    use crate::scenario_package::{Manifest, Package, RegionDef};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/two-room");
    let mut manifest: Manifest =
        toml::from_str(&std::fs::read_to_string(root.join("scenario.toml")).unwrap()).unwrap();
    let mut document: tor_simulation::rules::RulesetDocument =
        toml::from_str(tor_simulation::rules::DEFAULT_RULESET_TOML).unwrap();
    let escaped = "\\".repeat(256);
    for index in 0..if maximum_source { 16 } else { 128 } {
        let id = descriptor_id(index);
        assert_eq!(id.len(), 4);
        document
            .descriptors
            .push(serde_json::from_value(json!({"id":id,"name":escaped})).unwrap());
    }
    let empty = |offset| {
        json!({"offset":offset,"unlock":null,"check":null,
        "on_success":[],"on_failure":[],"descriptors":[],"riders":[]})
    };
    let strikes = if maximum_source {
        for index in 1..16 {
            document.talents.push(
                serde_json::from_value(json!({
                    "id":format!("page_unlock_{index}"),"name":escaped,"prerequisites":[],
                    "requirement":{"kind":"total_hd","minimum":2},"modifiers":[]
                }))
                .unwrap(),
            );
        }
        let components: Vec<_> = (0..32)
            .map(|index| {
                json!({"category":"impact",
            "descriptor":null,"amount":{"kind":"fixed","amount":index+1}})
            })
            .collect();
        let effect =
            json!({"kind":"damage","damage":{"kind":"components","components":components}});
        (0..16).map(|index| {
            let mut row = empty(index);
            row["unlock"] = if index == 0 { Value::Null } else { json!(format!("page_unlock_{index}")) };
            row["check"] = json!({"actor":"user","skill":{"kind":"named","skill":"heavy_weaponry"},
                "against":{"kind":"difficulty","formula":{"base":5,"skill":{"kind":"named","skill":"athletics"},"attribute":"strength"}}});
            row["descriptors"] = json!((0..16).map(descriptor_id).collect::<Vec<_>>());
            row["riders"] = json!((0..8).map(|_| json!({"trigger":"success","check":{"skill":{"kind":"named","skill":"heavy_weaponry"},
                "defense":"physical"},"effects":[effect.clone(),effect.clone()],"descriptors":[]})).collect::<Vec<_>>());
            row
        }).collect::<Vec<_>>()
    } else {
        (0..2)
            .map(|index| {
                let mut row = empty(index);
                row["descriptors"] = json!((0..128).map(descriptor_id).collect::<Vec<_>>());
                row
            })
            .collect()
    };
    let mut grants = Vec::new();
    for index in 0..abilities {
        let id = format!("page_ability_{index:03}");
        document.abilities.push(serde_json::from_value(json!({"id":id,"name":escaped,
            "tags":[],"basic_attack":false,"cost":null,
            "timing":{"source":{"kind":"fixed","prepare":100,"recover":100},"scaled_by":"speed"},
            "reach":{"kind":"melee"},"strikes":{"kind":"authored","strikes":strikes},
            "ai":{"priority":0,"hostile":false,"minimum_resource_percent":0,"range_preference":"any"}})).unwrap());
        grants.push(json!({"scope":"always","op":{"kind":"grant_ability","ability":id}}));
    }
    manifest.creatures = serde_json::from_value(json!({"species":{"page_subject":{
        "kind":"humanoid","attributes":{"strength":0,"speed":0,"intellect":0,"willpower":0,"awareness":0,"presence":0},
        "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":100,"recovery":100,
            "strikes":if maximum_source && abilities == 0 { strikes } else { vec![empty(0)] }},"modifiers":grants
    }}})).unwrap();
    manifest.characters[0].creature = Some(
        serde_json::from_value(json!({"species":"page_subject",
        "name":"Page subject","faction":"neutral","binding":"intellect",
        "starting_skills":["heavy_weaponry","athletics"],"restoration":"authored", "nutrition": {"kind":"exempt"},
        "hit_dice":[{"source":"racial"}]}))
        .unwrap(),
    );
    let regions: Vec<RegionDef> = [1, 2]
        .into_iter()
        .map(|id| {
            toml::from_str(
                &std::fs::read_to_string(root.join(format!("regions/{id}.toml"))).unwrap(),
            )
            .unwrap()
        })
        .collect();
    let source = toml::to_string(&document).unwrap();
    assert!(
        source.len() < 8 * 1024 * 1024,
        "accepted canonical rules input must fit"
    );
    let package = Package::from_parts(manifest, regions, &source).unwrap();
    // Engine admission has a stricter envelope than the source input ceiling.
    // Keep the exact existing stored metadata path in the fixture, before inspection.
    let compiled = crate::storage::schema::scenario::encode_content_package(&package)
        .expect("the compiler-backed fixture fits the unchanged960 KiB metadata ceiling");
    let stored: Value = serde_json::from_slice(&compiled).unwrap();
    let metadata_bytes = serde_json::to_vec(&stored["package"]).unwrap().len();
    assert!(
        metadata_bytes <= crate::storage::schema::scenario::MAX_SAVE_METADATA_BYTES,
        "actual compiled metadata is {metadata_bytes} bytes, above960 KiB"
    );
    crate::Scenario {
        actors: vec![],
        package: Some(Arc::new(package)),
        ..crate::Scenario::two_room(42)
    }
}

/// A normal template owns one grant so real wizard mutations can revoke it.
pub(crate) fn revocable_scenario() -> crate::Scenario {
    let mut scenario = scenario(2, false);
    // Wizard template operations resolve their catalog through ordinary streaming.
    scenario.streaming = Some(crate::regions::Streaming::default());
    let package = scenario.package.as_ref().unwrap();
    let mut manifest = package.manifest.clone();
    let grant = manifest
        .creatures
        .species
        .get_mut("page_subject")
        .unwrap()
        .modifiers
        .pop()
        .unwrap();
    manifest.creatures.templates.insert(
        "revocable".into(),
        crate::creature_authoring::Template {
            modifiers: vec![grant],
            ..Default::default()
        },
    );
    manifest.characters[0]
        .creature
        .as_mut()
        .unwrap()
        .templates
        .push("revocable".into());
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            manifest,
            package.region_defs().unwrap(),
            &package.rules_source,
        )
        .unwrap(),
    ));
    scenario
}

pub(crate) fn reply_context() -> p::ReplyContext {
    p::ReplyContext {
        input: p::InputContext {
            stream: p::StreamContext {
                stream: p::StreamId("f".repeat(36)),
                epoch: u64::MAX,
            },
            readiness_revision: u64::MAX,
        },
        actor: p::ActorId(1),
        branch: p::BranchId("b".repeat(36)),
        cursor: p::StreamCursor {
            sequence: u64::MAX,
            tick: u64::MAX,
        },
        revision: u64::MAX,
    }
}

#[test]
fn ordered_strikes_inspection_main_accepts_128_abilities_without_eager_descriptor_expansion() {
    let mut engine = crate::Engine::memory(scenario(128, false)).unwrap();
    engine.enable_wizard().unwrap();
    let before = engine.state(p::ActorId(1)).unwrap();
    let report = engine
        .inspect_creature(p::ActorId(1))
        .expect("accepted content needs a lightweight main report");
    let value = serde_json::to_value(&report).unwrap();
    assert_eq!(
        value["ability_strikes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["ability"]
                .as_str()
                .is_some_and(|id| id.starts_with("page_ability_")))
            .count(),
        128
    );
    assert!(value["natural_attack"].get("strikes").is_none());
    assert_eq!(value["natural_attack"]["strike_count"], 1);
    for (summary, ability) in value["ability_strikes"]
        .as_array()
        .unwrap()
        .iter()
        .zip(value["stats"]["abilities"].as_array().unwrap())
    {
        assert_eq!(summary["ability"], ability["id"]);
        assert!(summary.get("strikes").is_none());
    }
    let message = p::ServerMessage::CreatureInspection {
        context: reply_context(),
        request_id: "\\".repeat(128),
        report: Box::new(report),
    };
    let encoded = p::encode_response(&message, None, p::MAX_RESPONSE_BYTES).unwrap();
    assert!(
        encoded.text.len() < 1024 * 1024,
        "new summaries must not expand descriptor bodies"
    );
    assert_eq!(p::decode_response(&encoded.text).unwrap(), message);
    assert_eq!(engine.state(p::ActorId(1)).unwrap(), before);
}

#[test]
fn ordered_strikes_inspection_maximum_source_remains_a_lightweight_summary() {
    // Separate accepted packages avoid storing the maximum body twice.
    for abilities in [0, 1] {
        let mut engine = crate::Engine::memory(scenario(abilities, true)).unwrap();
        engine.enable_wizard().unwrap();
        let report = engine.inspect_creature(p::ActorId(1)).unwrap();
        let value = serde_json::to_value(&report).unwrap();
        assert_eq!(
            value["natural_attack"]["strike_count"],
            if abilities == 0 { 16 } else { 1 }
        );
        assert!(value["natural_attack"].get("strikes").is_none());
        assert!(value["selected_attack"]["attack"]["attack"]
            .get("strikes")
            .is_none());
        if abilities == 1 {
            let source = value["ability_strikes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["ability"] == "page_ability_000")
                .unwrap();
            assert_eq!(source["source"]["strike_count"], 16);
        }
        let message = p::ServerMessage::CreatureInspection {
            context: reply_context(),
            request_id: "\\".repeat(128),
            report: Box::new(report),
        };
        let encoded = p::encode_response(&message, None, p::MAX_RESPONSE_BYTES).unwrap();
        assert!(
            encoded.text.len() < 196_608,
            "one-source main inspection must not build the maximum payload"
        );
        assert_eq!(p::decode_response(&encoded.text).unwrap(), message);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PageWork {
    pub entries: usize,
    pub fields: usize,
    pub copied_string_bytes: usize,
    pub authority_projections: usize,
    pub owned_authority_projections: usize,
}
thread_local! { static PAGE_WORK: std::cell::Cell<PageWork> = const { std::cell::Cell::new(PageWork {entries:0,fields:0,copied_string_bytes:0,authority_projections:0,owned_authority_projections:0}) }; }
pub(crate) fn page_work() -> PageWork {
    PAGE_WORK.with(std::cell::Cell::get)
}
/// Called only by the bounded projector in test builds, never authoritative state.
pub(crate) fn projected(entries: usize, fields: usize, copied_string_bytes: usize) {
    PAGE_WORK.with(|counter| {
        let mut value = counter.get();
        value.entries += entries;
        value.fields += fields;
        value.copied_string_bytes += copied_string_bytes;
        counter.set(value);
    });
}

/// Current-state projection is separate from bounded entry work; it may rederive.
pub(crate) fn authority_projected(owned: bool) {
    PAGE_WORK.with(|counter| {
        let mut value = counter.get();
        value.authority_projections += 1;
        value.owned_authority_projections += usize::from(owned);
        counter.set(value);
    });
}

pub(crate) fn equipped_scenario() -> crate::Scenario {
    let mut scenario = scenario(1, false);
    let package = scenario.package.as_ref().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.archetypes.insert("page_blade".into(), serde_json::from_value(json!({
        "class":"weapon","name":"Page blade","equipment":{"slot":"weapon","attack":{
            "skill":"light_weaponry","bonus":2,"wind_up":10,"recovery":10,
            "damage":{"primary":{"category":"impact","descriptor":null,"sides":null},
                "components":[{"category":"impact","descriptor":null,"amount":{"type":"fixed","value":3}}]}
        },"modifiers":[]}
    })).unwrap());
    let mut regions = package.region_defs().unwrap();
    regions[0].items.push(
        serde_json::from_value(json!({"id":100,"at":[1,1,0],
        "archetype":"page_blade","carried_by":1,"equipped_slot":0}))
        .unwrap(),
    );
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(manifest, regions, &package.rules_source)
            .unwrap(),
    ));
    scenario
}

pub(crate) fn unload_target_scenario() -> crate::Scenario {
    let mut scenario = scenario(1, false);
    scenario.streaming = Some(crate::regions::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let package = scenario.package.as_ref().unwrap();
    let mut manifest = package.manifest.clone();
    manifest.ai_profiles.insert(
        "page_idle".into(),
        serde_json::from_value(json!({"memory_ticks":100,"flee_percent":0})).unwrap(),
    );
    let mut regions = package.region_defs().unwrap();
    let mut creature = package.manifest.characters[0].creature.clone().unwrap();
    creature.name = "Other page subject".into();
    regions[0].actors.push(serde_json::from_value(json!({"id":2,"at":[3,1,0],
        "body":package.manifest.characters[0].body,"creature":creature,"controller":"ai","ai":"page_idle","turn_ticks":100})).unwrap());
    regions.push(
        serde_json::from_value(json!({"id":3,"name":"Isolated inspection room",
        "size":[6,3,2],"chamber":true,"gravity":[0,0,-1],"anchors":{"safe":[1,1,0]}}))
        .unwrap(),
    );
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(manifest, regions, &package.rules_source)
            .unwrap(),
    ));
    scenario
}

pub(crate) fn changed_catalog_scenario() -> crate::Scenario {
    let mut scenario = scenario(1, false);
    let package = scenario.package.as_ref().unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(&package.rules_source).unwrap();
    rules
        .abilities
        .iter_mut()
        .find(|row| row.id.as_str() == "page_ability_000")
        .unwrap()
        .name = "Changed pinned technique".into();
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            package.manifest.clone(),
            package.region_defs().unwrap(),
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    scenario
}

pub(crate) fn mixed_trigger_scenario() -> crate::Scenario {
    let mut scenario = scenario(1, false);
    let package = scenario.package.as_ref().unwrap();
    let mut rules: tor_simulation::rules::RulesetDocument =
        toml::from_str(&package.rules_source).unwrap();
    let ability = rules
        .abilities
        .iter_mut()
        .find(|row| row.id.as_str() == "page_ability_000")
        .unwrap();
    let tor_simulation::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
        unreachable!()
    };
    let first = &mut strikes[0];
    first.check = Some(
        serde_json::from_value(
            json!({"actor":"user","skill":{"kind":"named","skill":"heavy_weaponry"},
        "against":{"kind":"difficulty","formula":{"base":10,"skill":null,"attribute":null}}}),
        )
        .unwrap(),
    );
    first.on_success = serde_json::from_value(json!((1..=15)
        .map(|amount| json!({
            "kind":"heal","amount":{"kind":"fixed","amount":amount}
        }))
        .collect::<Vec<_>>()))
    .unwrap();
    first.on_failure = serde_json::from_value(json!([{"kind":"damage","damage":{"kind":"components",
        "components":[{"category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":33}}]}}])).unwrap();
    first.riders = serde_json::from_value(json!([
        {"trigger":"success","check":null,"effects":(1..=14).map(|amount| json!({"kind":"heal","amount":{"kind":"fixed","amount":amount}})).collect::<Vec<_>>(),"descriptors":[]},
        {"trigger":"failure","check":null,"effects":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"fixed","amount":77}}]}}],"descriptors":[]}
    ])).unwrap();
    scenario.package = Some(Arc::new(
        crate::scenario_package::Package::from_parts(
            package.manifest.clone(),
            package.region_defs().unwrap(),
            &toml::to_string(&rules).unwrap(),
        )
        .unwrap(),
    ));
    scenario
}
