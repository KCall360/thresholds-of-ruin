use std::sync::Arc;
use tor_server::creature_authoring::{actor_health_seed, BuildSpec, Catalog};
use tor_simulation::attributes::{Attribute, Skill};
use tor_simulation::{
    content_ids::AbilityId,
    rules::{Ruleset, RulesetDocument, DEFAULT_RULESET_TOML},
};

fn rules() -> Arc<Ruleset> {
    Ruleset::validate(toml::from_str(DEFAULT_RULESET_TOML).unwrap()).unwrap()
}

fn catalog() -> Catalog {
    serde_json::from_value(serde_json::json!({
        "species": {"human": {
            "kind": "humanoid", "attributes": {"strength": 2, "speed": 1, "intellect": 2, "willpower": 2, "awareness": 1, "presence": 1},
            "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"strikes":[{"offset":0,"check":{"actor":"user","skill":{"kind":"selected_weapon"},"against":{"kind":"defense","defense":"physical"}},"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"impact","descriptor":null,"amount":{"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}}}]}}],"on_failure":[],"descriptors":[],"riders":[]}]},
            "modifiers": [{"scope":"always","op":{"kind":"grant_ability", "ability": "fear"}}]
        }},
        "templates": {"undead": {"priority": 0, "kind": "undead", "adjustments": {"strength": 2, "speed": -1}, "overrides": {"intellect": 0}, "modifiers": [{"scope":"always","op":{"kind":"flag","flag":"mindless"}}]}}
    })).unwrap()
}

fn specification() -> BuildSpec {
    serde_json::from_value(serde_json::json!({
        "species": "human", "name": "test subject", "faction": "neutral", "binding": "intellect",
        "starting_skills": ["heavy_weaponry", "spellcasting"], "restoration": "authored",
        "hit_dice": [{"source": "warrior", "purchases": [{"pool":"advancement", "selection":{"kind":"talent", "talent":"power_strike"}}]}, {"source": "mage", "purchases": [{"pool":"advancement", "selection":{"kind":"talent", "talent":"magic_bolt"}}]}]
    })).unwrap()
}

#[test]
fn authored_creature_recipes_validate_owned_choices_and_assign_stable_actor_health_streams() {
    let compiled = catalog().compile(rules()).unwrap();
    let spec = specification();
    let recipe = compiled.prepare(&spec).unwrap();
    let seed = actor_health_seed(42, 7);
    assert_eq!(seed, 4_807_626_138_893_413_574, "spawn stream format");
    assert_ne!(seed, actor_health_seed(7, 42), "seed and actor domains");
    assert_ne!(seed, actor_health_seed(43, 7), "world seed participates");
    let first = recipe.instantiate(seed).unwrap();
    assert_eq!(first, recipe.instantiate(seed).unwrap());
    let other = recipe.instantiate(actor_health_seed(42, 8)).unwrap();
    assert_ne!(first.ledger().entries(), other.ledger().entries());
    assert_eq!(first.choices(), other.choices());
    let derived = first.derive().unwrap();
    assert_eq!(derived.attributes.get(Attribute::Strength), 2);
    assert_eq!(derived.skills.get(Skill::Spellcasting), 1);
    assert!(derived
        .abilities
        .contains(&AbilityId::new("power_strike").unwrap()));
    assert!(derived
        .abilities
        .contains(&AbilityId::new("magic_bolt").unwrap()));
    assert_eq!(recipe.identity().name, "test subject");
    assert_eq!(recipe.identity().faction, "neutral");
}

#[test]
fn authored_builds_reject_unknown_references_invalid_budgets_and_dormant_talents() {
    let compiled = catalog().compile(rules()).unwrap();
    for mutate in 0..6 {
        let mut spec = specification();
        match mutate {
            0 => spec.species = "missing".into(),
            1 => spec.templates.push("missing".into()),
            2 => spec.hit_dice.clear(),
            3 => spec.hit_dice[0].purchases = vec![rank("athletics"); 3],
            4 => {
                spec.templates.push("undead".into());
                spec.hit_dice[1].purchases = vec![purchase("fear")];
                spec.starting_skills[1] = "intimidation".into();
            }
            5 => spec.hit_dice[1].purchases = spec.hit_dice[0].purchases.clone(),
            _ => unreachable!(),
        }
        assert!(
            compiled.prepare(&spec).is_err(),
            "accepted invalid authoring case {mutate}"
        );
    }
}

#[test]
fn authored_catalog_rejects_invalid_unused_definitions() {
    for case in 0..8 {
        let mut author = catalog();
        let species = author.species.get_mut("human").unwrap();
        match case {
            0 => species.attributes.strength = 6,
            1 => {
                let mut record = serde_json::to_value(&species.melee).unwrap();
                record["strikes"][0]["on_success"][0]["damage"]["components"][0]["amount"]
                    ["dice"]["sides"] = serde_json::json!(0);
                species.melee = serde_json::from_value(record).unwrap();
            }
            2 => {
                let mut record = serde_json::to_value(&species.melee).unwrap();
                record["skill"] = serde_json::json!("spellcasting");
                species.melee = serde_json::from_value(record).unwrap();
            }
            3 => species.subtypes = vec!["fire".into(); 2],
            4 => {
                species.modifiers[0].op = tor_server::creature_authoring::Op::GrantAbility {
                    ability: "missing".into(),
                }
            }
            5 => {
                author
                    .templates
                    .get_mut("undead")
                    .unwrap()
                    .adjustments
                    .strength = 1_001
            }
            6 => {
                author
                    .templates
                    .get_mut("undead")
                    .unwrap()
                    .overrides
                    .intellect = Some(1_001)
            }
            7 => {
                let template = author.templates.get_mut("undead").unwrap();
                template.add_subtypes.push("fire".into());
                template.remove_subtypes.push("fire".into());
            }
            _ => unreachable!(),
        }
        assert!(
            author.compile(rules()).is_err(),
            "accepted invalid catalog case {case}"
        );
    }
}

#[test]
fn authored_build_bounds_and_template_conflicts_are_checked_before_spawning() {
    let mut author = catalog();
    let mut other = author.templates["undead"].clone();
    other.kind = Some("construct".into());
    author.templates.insert("construct".into(), other);
    let compiled = author.compile(rules()).unwrap();
    let mut spec = specification();
    spec.templates = vec!["undead".into(), "construct".into()];
    assert!(
        compiled.prepare(&spec).is_err(),
        "same-priority type conflict"
    );
    spec.templates = vec!["undead".into(); 2];
    assert!(
        compiled.prepare(&spec).is_err(),
        "duplicate template ownership"
    );
    spec.templates.clear();
    spec.hit_dice = vec![
        tor_server::creature_authoring::Advancement {
            source: "racial".into(),
            purchases: vec![],
            attribute: None,
        };
        256
    ];
    assert!(
        compiled.prepare(&spec).is_ok(),
        "maximum supported HD count"
    );
    spec.hit_dice.push(spec.hit_dice[0].clone());
    assert!(compiled.prepare(&spec).is_err(), "excess HD count");
    spec = specification();
    for name in [String::new(), "x".repeat(81), "line\nbreak".into()] {
        spec.name = name;
        assert!(compiled.prepare(&spec).is_err(), "invalid identity label");
    }
}

#[test]
fn authored_schemas_reject_unknown_fields_and_catalog_values() {
    let source = serde_json::to_value(specification()).unwrap();
    let mut mutated = source.clone();
    mutated["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<BuildSpec>(mutated).is_err());
    let mut source = serde_json::to_value(catalog()).unwrap();
    source["species"]["human"]["modifiers"][0]["unexpected"] = serde_json::json!(1);
    assert!(serde_json::from_value::<Catalog>(source).is_err());
}

#[test]
fn authored_natural_attack_retains_skill_timing_and_mixed_damage_sources() {
    let author: Catalog = serde_json::from_value(serde_json::json!({
        "species": {"elemental": {
            "kind": "elemental",
            "attributes": {"strength": 2, "speed": 1, "intellect": 2, "willpower": 2, "awareness": 1, "presence": 1},
            "melee": {"skill":"light_weaponry","bonus":2,"wind_up":90,"recovery":70,"strikes":[{"offset":0,"check":{"actor":"user","skill":{"kind":"selected_weapon"},"against":{"kind":"defense","defense":"physical"}},"on_success":[{"kind":"damage","damage":{"kind":"components","components":[{"category":"energy","descriptor":"fire","amount":{"kind":"rolled","dice":{"count":2,"sides":6,"bonus":-1}}},{"category":"keen","amount":{"kind":"fixed","amount":3}}]}}],"on_failure":[],"descriptors":[],"riders":[]}]}
        }}
    })).unwrap();
    let compiled = author.compile(rules()).unwrap();
    let mut spec = specification();
    spec.species = "elemental".into();
    spec.starting_skills[0] = "light_weaponry".into();
    let build = compiled.prepare(&spec).unwrap().instantiate(42).unwrap();
    let attack = &build.species().melee;
    assert_eq!(attack.skill(), Skill::LightWeaponry);
    assert_eq!(
        (attack.bonus(), attack.wind_up(), attack.recovery()),
        (2, 90, 70)
    );
    assert_eq!(attack.first_primary_damage().unwrap().components().len(), 2);
    assert_eq!(
        attack
            .first_primary_damage()
            .unwrap()
            .primary()
            .unwrap()
            .descriptor,
        Some(tor_simulation::content_ids::DescriptorId::new("fire").unwrap())
    );
}

#[test]
fn creature_identity_limits_match_spawn_and_checkpoint_byte_boundaries() {
    use std::num::NonZeroU64;
    use tor_simulation::{CreatureIdentity, Game};
    use tor_world::{Location, Position, RegionId};
    let compiled = catalog().compile(rules()).unwrap();
    for (name, faction) in [
        ("n".repeat(60), "f".repeat(80)),
        ("\u{e9}".repeat(30), "\u{e9}".repeat(40)),
    ] {
        let mut spec = specification();
        spec.name = name;
        spec.faction = faction;
        let recipe = compiled.prepare(&spec).unwrap();
        let identity = recipe.identity().clone();
        let mut game = Game::two_room(42, compiled.content().clone());
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(actor, identity.clone(), recipe.instantiate(42).unwrap())
            .unwrap();
        assert_eq!(
            serde_json::from_value::<CreatureIdentity>(serde_json::to_value(&identity).unwrap())
                .unwrap(),
            identity
        );
        for field in ["name", "faction"] {
            let mut invalid = spec.clone();
            if field == "name" {
                invalid.name.push('x');
            } else {
                invalid.faction.push('x');
            }
            assert!(
                compiled.prepare(&invalid).is_err(),
                "over-bound {field} must reject during preparation"
            );
        }
    }
}

#[test]
fn new_talent_definitions_are_selected_without_authoring_code_changes() {
    let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    let definition = toml::from_str(include_str!("../fixtures/custom-talent.toml")).unwrap();
    document.talents.push(definition);
    let compiled = catalog()
        .compile(Ruleset::validate(document).unwrap())
        .unwrap();
    let mut spec = specification();
    spec.hit_dice[0].purchases = vec![purchase("fixture_vitality")];
    let build = compiled.prepare(&spec).unwrap().instantiate(42).unwrap();
    assert!(build
        .derive()
        .unwrap()
        .active_talents
        .iter()
        .any(|id| id.as_str() == "fixture_vitality"));
}

#[test]
fn authored_content_references_resolve_against_the_selected_ruleset() {
    let compiled = catalog().compile(rules()).unwrap();
    for case in 0..5 {
        let mut spec = specification();
        match case {
            0 => spec.hit_dice[0].source = "unknown_class".into(),
            1 => spec.hit_dice[0].purchases = vec![purchase("unknown_talent")],
            2 => spec.hit_dice[0].purchases = vec![rank("unknown_skill")],
            3 => spec.binding = "strength".into(),
            4 => spec.hit_dice[0].attribute = Some("unknown_attribute".into()),
            _ => unreachable!(),
        }
        assert!(
            compiled.prepare(&spec).is_err(),
            "accepted unknown reference {case}"
        );
    }
    for case in 0..4 {
        let mut author = catalog();
        let species = author.species.get_mut("human").unwrap();
        match case {
            0 => species.kind = "unknown_type".into(),
            1 => species.subtypes = vec!["unknown_subtype".into()],
            2 => {
                let tor_server::scenario_package::Effect::Damage {
                    damage: tor_server::scenario_package::DamageExpr::Components { components },
                } = &mut species.melee.strikes[0].on_success[0]
                else {
                    panic!("natural damage fixture");
                };
                components[0].descriptor = Some("unknown_descriptor".into());
            }
            3 => {
                species.modifiers[0].op = tor_server::creature_authoring::Op::Flag {
                    flag: "unknown_flag".into(),
                }
            }
            _ => unreachable!(),
        }
        assert!(
            author.compile(rules()).is_err(),
            "accepted unused unknown reference {case}"
        );
    }
}

#[test]
fn partial_template_attributes_are_named_strict_and_do_not_relax_base_attributes() {
    let template: tor_server::creature_authoring::Template = toml::from_str(
        "priority = 0\nadjustments = { strength = 2 }\noverrides = { intellect = 0 }\n",
    )
    .unwrap();
    assert_eq!(template.adjustments.strength, 2);
    assert_eq!(template.adjustments.speed, 0);
    assert_eq!(template.overrides.intellect, Some(0));
    assert_eq!(template.overrides.presence, None);
    assert!(toml::from_str::<tor_server::creature_authoring::Template>(
        "priority = 0\nadjustments = { unknown = 2 }\n",
    )
    .is_err());
    let mut source = serde_json::to_value(catalog()).unwrap();
    source["species"]["human"]["attributes"]
        .as_object_mut()
        .unwrap()
        .remove("strength");
    assert!(serde_json::from_value::<Catalog>(source).is_err());
}

#[test]
fn authored_generic_check_bonus_reaches_the_derived_ability() {
    let mut author = catalog();
    author.species.get_mut("human").unwrap().modifiers.push(
        serde_json::from_value(serde_json::json!({"scope":"always","op":{
            "kind":"add","stat":{"kind":"ability","ability":"fear","stat":"check_bonus"},"value":-2
        }}))
        .unwrap(),
    );
    let compiled = author.compile(rules()).unwrap();
    let build = compiled
        .prepare(&specification())
        .unwrap()
        .instantiate(42)
        .unwrap();
    assert_eq!(
        build
            .derive()
            .unwrap()
            .ability_values(&AbilityId::new("fear").unwrap())
            .check_bonus,
        -2
    );
}

#[test]
fn authored_attribute_bounds_follow_the_selected_ruleset_and_round_trip() {
    use tor_simulation::creatures::record::BuildRecord;
    let mut document = rules().document().clone();
    document.constants.attribute_cap = 6;
    let expanded = Ruleset::validate(document).unwrap();
    let mut author = catalog();
    author.species.get_mut("human").unwrap().attributes.strength = 6;
    assert!(
        author.compile(rules()).is_err(),
        "default cap five rejects six"
    );
    let compiled = author.compile(expanded).unwrap();
    for explicit in [false, true] {
        let mut spec = specification();
        if explicit {
            spec.attributes = Some(author.species["human"].attributes);
        }
        let original = compiled.prepare(&spec).unwrap().instantiate(42).unwrap();
        assert_eq!(original.initial_attributes().get(Attribute::Strength), 6);
        let saved = serde_json::to_vec(&BuildRecord::capture(&original)).unwrap();
        let restored = serde_json::from_slice::<BuildRecord>(&saved)
            .unwrap()
            .restore(compiled.content().clone())
            .unwrap();
        assert_eq!(restored, original);
    }
    let ordinary = catalog().compile(rules()).unwrap();
    let mut spec = specification();
    spec.attributes = Some(author.species["human"].attributes);
    assert!(
        ordinary.prepare(&spec).is_err(),
        "explicit attributes also obey cap five"
    );
}

#[test]
fn authored_health_recovery_curve_and_species_percent_are_validated_and_derived() {
    let mut document: RulesetDocument = toml::from_str(DEFAULT_RULESET_TOML).unwrap();
    document.health_recovery.base_period = 901;
    document.health_recovery.reduction_per_hd = 100;
    document.health_recovery.minimum_period = 101;
    let mut author = catalog();
    author.species.get_mut("human").unwrap().modifiers.push(serde_json::from_value(serde_json::json!({
        "scope":"always","op":{"kind":"add","stat":{"kind":"health_recovery_period_percent"},"value":-25}
    })).unwrap());
    let compiled = author
        .compile(Ruleset::validate(document.clone()).unwrap())
        .unwrap();
    let build = compiled
        .prepare(&specification())
        .unwrap()
        .instantiate(42)
        .unwrap();
    assert_eq!(build.derive().unwrap().health_recovery_period, 601);
    for (base, reduction, minimum) in [
        (0, 1, 1),
        (900, 1, 0),
        (900, 1, 901),
        (1_000_001, 1, 1),
        (900, 1_000_001, 1),
    ] {
        document.health_recovery = tor_simulation::rules::HealthRecoveryDef {
            base_period: base,
            reduction_per_hd: reduction,
            minimum_period: minimum,
        };
        assert!(Ruleset::validate(document.clone()).is_err());
    }
}

fn purchase(talent: &str) -> tor_server::creature_authoring::Purchase {
    tor_server::creature_authoring::Purchase {
        pool: "advancement".into(),
        selection: tor_server::creature_authoring::PurchaseSelection::Talent {
            talent: talent.into(),
        },
    }
}
fn rank(skill: &str) -> tor_server::creature_authoring::Purchase {
    tor_server::creature_authoring::Purchase {
        pool: "advancement".into(),
        selection: tor_server::creature_authoring::PurchaseSelection::SkillRank {
            skill: skill.into(),
        },
    }
}

#[test]
fn authored_creation_requires_distinct_starting_ranks_and_explicit_restoration_policy() {
    let compiled = catalog().compile(rules()).unwrap();
    let mut spec = specification();
    spec.starting_skills[1] = spec.starting_skills[0].clone();
    assert!(compiled.prepare(&spec).is_err());
    for field in ["starting_skills", "restoration"] {
        let mut value = serde_json::to_value(specification()).unwrap();
        value.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<BuildSpec>(value).is_err());
    }
}
#[test]
fn authored_point_pool_references_and_purchase_kinds_are_checked_before_spawn() {
    let compiled = catalog().compile(rules()).unwrap();
    let mut spec = specification();
    spec.hit_dice[0].purchases[0].pool = "missing".into();
    assert!(compiled.prepare(&spec).is_err());
    let mut document = rules().document().clone();
    document.point_pools[0].allowed = vec![tor_simulation::rules::PurchaseKind::SkillRank];
    let compiled = catalog()
        .compile(Ruleset::validate(document).unwrap())
        .unwrap();
    assert!(compiled.prepare(&specification()).is_err());
}
#[test]
fn authored_restore_plan_is_explicit_saved_provenance_and_keeps_spawn_seed() {
    let compiled = catalog().compile(rules()).unwrap();
    let recipe = compiled.prepare(&specification()).unwrap();
    let mut build = recipe.instantiate(42).unwrap();
    let original = build.clone();
    build.remove_latest().unwrap();
    let record = tor_simulation::creatures::record::BuildRecord::capture(&build);
    let bytes = serde_json::to_vec(&record).unwrap();
    let mut build =
        serde_json::from_slice::<tor_simulation::creatures::record::BuildRecord>(&bytes)
            .unwrap()
            .restore(build.content().clone())
            .unwrap();
    build
        .add_hit_die(
            tor_simulation::progression::HdSource::Class(
                tor_simulation::content_ids::ClassId::new("mage").unwrap(),
            ),
            42,
        )
        .unwrap();
    assert_eq!(build, original);
    let mut spec = specification();
    spec.restoration = tor_server::creature_authoring::Restoration::ChooseAgain;
    let mut build = compiled.prepare(&spec).unwrap().instantiate(42).unwrap();
    build.remove_latest().unwrap();
    build
        .add_hit_die(
            tor_simulation::progression::HdSource::Class(
                tor_simulation::content_ids::ClassId::new("mage").unwrap(),
            ),
            42,
        )
        .unwrap();
    assert!(build.choices()[1].purchases.is_empty());
    assert_eq!(build.starting_skills(), original.starting_skills());
}

#[test]
fn authored_multi_point_builds_accept_256_talents_and_reject_257_or_258_before_spawn() {
    use tor_simulation::content_ids::TalentId;
    use tor_simulation::rules::{Requirement, TalentDef};
    let mut document = rules().document().clone();
    document.constants.racial_point_grants[0].points = 129;
    document.talents = (0..258)
        .map(|index| TalentDef {
            id: TalentId::new(format!("limit_{index:03}")).unwrap(),
            name: format!("Limit {index}"),
            prerequisites: vec![],
            requirement: Requirement::TotalHd { minimum: 1 },
            modifiers: vec![],
        })
        .collect();
    let compiled = catalog()
        .compile(Ruleset::validate(document).unwrap())
        .unwrap();
    for count in [256, 257, 258] {
        let mut spec = specification();
        spec.hit_dice = (0..2)
            .map(|owner| tor_server::creature_authoring::Advancement {
                source: "racial".into(),
                purchases: (owner * 129..((owner + 1) * 129).min(count))
                    .map(|index| purchase(&format!("limit_{index:03}")))
                    .collect(),
                attribute: None,
            })
            .collect();
        let result = compiled.prepare(&spec);
        if count == 256 {
            let build = result.unwrap().instantiate(42).unwrap();
            assert_eq!(build.derive().unwrap().active_talents.len(), 256);
            assert_eq!(build.unspent_points(), 2);
        } else {
            assert!(
                result.is_err(),
                "{count} owned talents must reject before spawning"
            );
        }
    }
}

#[test]
fn future_growth_is_explicit_authoring_and_survives_seeded_instantiation() {
    use tor_server::creature_authoring::{FutureGrowth, GrowthSource};
    let compiled = catalog().compile(rules()).unwrap();
    let mut spec = specification();
    assert_eq!(spec.future_growth, FutureGrowth::Disabled {});
    assert_eq!(
        compiled
            .prepare(&spec)
            .unwrap()
            .instantiate(42)
            .unwrap()
            .future_growth(),
        &tor_simulation::earned_growth::FutureGrowth::Disabled {}
    );
    spec.future_growth = FutureGrowth::Selected {
        source: GrowthSource::Class {
            class: "warrior".into(),
        },
    };
    let recipe = compiled.prepare(&spec).unwrap();
    for seed in [1, 42, u64::MAX] {
        assert_eq!(
            recipe.instantiate(seed).unwrap().future_growth(),
            &tor_simulation::earned_growth::FutureGrowth::Selected {
                source: tor_simulation::earned_growth::GrowthSource::Class {
                    class: tor_simulation::content_ids::ClassId::new("warrior").unwrap()
                }
            }
        );
    }
    spec.future_growth = FutureGrowth::Selected {
        source: GrowthSource::Class {
            class: "missing".into(),
        },
    };
    assert!(compiled.prepare(&spec).is_err());
}
