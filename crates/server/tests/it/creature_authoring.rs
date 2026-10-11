use tor_server::creature_authoring::{actor_health_seed, BuildSpec, Catalog};
use tor_simulation::attributes::{Attribute, Skill};

fn catalog() -> Catalog {
    serde_json::from_value(serde_json::json!({
        "species": {"human": {
            "kind": "humanoid", "attributes": {"strength": 2, "speed": 1, "intellect": 2, "willpower": 2, "awareness": 1, "presence": 1},
            "melee":{"skill":"heavy_weaponry","bonus":0,"wind_up":60,"recovery":40,"damage":{"primary":{"category":"impact","descriptor":null,"sides":6},"components":[{"category":"impact","descriptor":null,"amount":{"type":"rolled","count":1,"sides":6,"bonus":0}}]}},
            "grants": [{"type": "ability", "ability": "fear"}]
        }},
        "templates": {"undead": {"priority": 0, "kind": "undead", "adjustments": {"strength": 2, "speed": -1}, "overrides": {"intellect": 0}, "grants": [{"type": "mindless"}]}}
    })).unwrap()
}

fn specification() -> BuildSpec {
    serde_json::from_value(serde_json::json!({
        "species": "human", "name": "test subject", "faction": "neutral", "binding": "intellect",
        "hit_dice": [{"source": "warrior", "training": ["heavy_weaponry"], "talent": "power_strike"}, {"source": "mage", "training": ["spellcasting"], "talent": "magic_bolt"}]
    })).unwrap()
}

#[test]
fn authored_creature_recipes_validate_owned_choices_and_assign_stable_actor_health_streams() {
    let compiled = catalog().compile().unwrap();
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
        .contains(&tor_simulation::grants::Ability::PowerStrike));
    assert!(derived
        .abilities
        .contains(&tor_simulation::grants::Ability::MagicBolt));
    assert_eq!(recipe.identity().name, "test subject");
    assert_eq!(recipe.identity().faction, "neutral");
}

#[test]
fn authored_builds_reject_unknown_references_invalid_budgets_and_dormant_talents() {
    let compiled = catalog().compile().unwrap();
    for mutate in 0..6 {
        let mut spec = specification();
        match mutate {
            0 => spec.species = "missing".into(),
            1 => spec.templates.push("missing".into()),
            2 => spec.hit_dice.clear(),
            3 => {
                spec.hit_dice[0].training =
                    vec![tor_server::creature_authoring::Skill::Athletics; 3]
            }
            4 => {
                spec.templates.push("undead".into());
                spec.hit_dice[1].talent = Some(tor_server::creature_authoring::Talent::Fear);
                spec.hit_dice[1].training =
                    vec![tor_server::creature_authoring::Skill::Intimidation];
            }
            5 => spec.hit_dice[1].talent = spec.hit_dice[0].talent,
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
                record["damage"]["components"][0]["amount"]["sides"] = serde_json::json!(0);
                species.melee = serde_json::from_value(record).unwrap();
            }
            2 => {
                let mut record = serde_json::to_value(&species.melee).unwrap();
                record["skill"] = serde_json::json!("spellcasting");
                species.melee = serde_json::from_value(record).unwrap();
            }
            3 => species.subtypes = vec![tor_server::creature_authoring::Subtype::Fire; 2],
            4 => species.grants.push(species.grants[0]),
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
                template
                    .add_subtypes
                    .push(tor_server::creature_authoring::Subtype::Fire);
                template
                    .remove_subtypes
                    .push(tor_server::creature_authoring::Subtype::Fire);
            }
            _ => unreachable!(),
        }
        assert!(
            author.compile().is_err(),
            "accepted invalid catalog case {case}"
        );
    }
}

#[test]
fn authored_build_bounds_and_template_conflicts_are_checked_before_spawning() {
    let mut author = catalog();
    let mut other = author.templates["undead"].clone();
    other.kind = Some(tor_server::creature_authoring::CreatureType::Construct);
    author.templates.insert("construct".into(), other);
    let compiled = author.compile().unwrap();
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
            source: tor_server::creature_authoring::HitDieSource::Racial,
            training: vec![],
            talent: None,
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
    for (field, value) in [
        ("unexpected", serde_json::json!(true)),
        ("binding", serde_json::json!("strength")),
        ("hit_dice", serde_json::json!([{"source": "rogue"}])),
    ] {
        let mut mutated = source.clone();
        mutated[field] = value;
        assert!(serde_json::from_value::<BuildSpec>(mutated).is_err());
    }
    let mut source = serde_json::to_value(catalog()).unwrap();
    source["species"]["human"]["grants"][0]["unexpected"] = serde_json::json!(1);
    assert!(serde_json::from_value::<Catalog>(source).is_err());
}

#[test]
fn authored_natural_attack_retains_skill_timing_and_mixed_damage_sources() {
    let author: Catalog = serde_json::from_value(serde_json::json!({
        "species": {"elemental": {
            "kind": "elemental",
            "attributes": {"strength": 2, "speed": 1, "intellect": 2, "willpower": 2, "awareness": 1, "presence": 1},
            "melee": {"skill": "light_weaponry", "bonus": 2, "wind_up": 90, "recovery": 70,
                "damage": {"primary": {"category": "energy", "descriptor": "fire", "sides": 6},
                    "components": [
                        {"category": "energy", "descriptor": "fire", "amount": {"type": "rolled", "count": 2, "sides": 6, "bonus": -1}},
                        {"category": "keen", "amount": {"type": "fixed", "value": 3}}
                    ]}}
        }}
    })).unwrap();
    let compiled = author.compile().unwrap();
    let mut spec = specification();
    spec.species = "elemental".into();
    spec.hit_dice[0].training = vec![tor_server::creature_authoring::Skill::LightWeaponry];
    let build = compiled.prepare(&spec).unwrap().instantiate(42).unwrap();
    let attack = &build.species().melee;
    assert_eq!(attack.skill(), Skill::LightWeaponry);
    assert_eq!(
        (attack.bonus(), attack.wind_up(), attack.recovery()),
        (2, 90, 70)
    );
    assert_eq!(attack.damage().components().len(), 2);
    assert_eq!(
        attack.damage().primary().unwrap().descriptor,
        Some(tor_simulation::grants::Descriptor::Fire)
    );
    let record = tor_simulation::creatures::record::BuildRecord::capture(&build);
    let restored = record.restore().unwrap();
    assert_eq!(restored, build);
    let mut game = tor_simulation::Game::two_room_in_stone(42);
    let actor = game
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.configure_creature(
        actor,
        tor_simulation::CreatureIdentity {
            name: "elemental".into(),
            faction: "blue".into(),
        },
        build,
    )
    .unwrap();
    let plan = game
        .ability_plan(actor, tor_simulation::grants::Ability::BasicMelee)
        .unwrap();
    assert_eq!((plan.preparation, plan.recovery), (75, 59));
    let tor_simulation::abilities::AbilityEffect::Melee { damage, .. } = plan.effect else {
        panic!("expected melee");
    };
    assert_eq!(damage.components().len(), 2);
    assert_eq!(
        damage.primary().unwrap().category,
        tor_simulation::combat::DamageType::Energy
    );
    let immunity =
        tor_simulation::damage::Protection::from_grants([tor_simulation::grants::Grant::Immunity(
            tor_simulation::grants::Selector::Descriptor(tor_simulation::grants::Descriptor::Fire),
        )])
        .unwrap();
    for mut seed in 0..32 {
        assert_eq!(
            damage
                .resolve(&mut seed, tor_simulation::dice::Edge::default(), &immunity)
                .total,
            3
        );
    }
}

#[test]
fn creature_identity_limits_match_spawn_and_checkpoint_byte_boundaries() {
    use std::num::NonZeroU64;
    use tor_simulation::{CreatureIdentity, Game};
    use tor_world::{Location, Position, RegionId};
    let compiled = catalog().compile().unwrap();
    for (name, faction) in [
        ("n".repeat(60), "f".repeat(80)),
        ("\u{e9}".repeat(30), "\u{e9}".repeat(40)),
    ] {
        let mut spec = specification();
        spec.name = name;
        spec.faction = faction;
        let recipe = compiled.prepare(&spec).unwrap();
        let identity = recipe.identity().clone();
        let mut game = Game::two_room(42);
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
