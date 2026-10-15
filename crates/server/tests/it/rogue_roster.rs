//! Newly reconstructed source-qualified roster compilation and record coverage.
use std::path::Path;
use tor_server::{
    creature_authoring::actor_health_seed,
    scenario_package::{resolve_rules_source, resolve_source},
};
use tor_simulation::{
    content_ids::{AbilityId, SpeciesId, TalentId},
    creatures::record::BuildRecord,
    rules::Ruleset,
};

pub(super) fn catalog() -> (
    tor_server::creature_authoring::CompiledCatalog,
    tor_server::scenario_package::Manifest,
) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let manifest = resolve_source(&root).unwrap().manifest;
    let authored = std::fs::read_to_string(root.join("rules.toml")).unwrap();
    let resolved = resolve_rules_source(&authored, "rules.toml").unwrap();
    let rules = Ruleset::validate(toml::from_str(&resolved.source).unwrap()).unwrap();
    (manifest.creatures.compile(rules).unwrap(), manifest)
}

#[test]
fn rogue_roster_actual_hd_seeds_choices_and_natural_rows_round_trip_without_source() {
    let (catalog, manifest) = catalog();
    for (name, hd, strikes) in [
        ("emu", 1, 1),
        ("snake", 1, 1),
        ("hobgoblin", 1, 1),
        ("zombie", 2, 1),
        ("quagga", 3, 2),
        ("centaur", 4, 3),
        ("yeti", 4, 2),
        ("black_unicorn", 7, 3),
    ] {
        let identity = format!("rogue.{name}");
        let recipe = catalog
            .prepare(manifest.archetypes[&identity].creature.as_ref().unwrap())
            .unwrap();
        let build = recipe.instantiate(actor_health_seed(42, 3)).unwrap();
        let other = recipe.instantiate(actor_health_seed(42, 4)).unwrap();
        assert_eq!(build.ledger().entries().len(), hd, "{name}");
        assert_ne!(
            build.ledger().entries(),
            other.ledger().entries(),
            "individual HD seeds: {name}"
        );
        assert_eq!(build.choices(), other.choices());
        let derived = build.derive().unwrap();
        assert!(derived.dormant_talents.is_empty(), "{name}");
        let source = &catalog.content().catalog().species[&SpeciesId::new(&identity).unwrap()];
        assert_eq!(source.melee.strikes().len(), strikes, "{name}");
        for strike in source.melee.strikes() {
            assert!(strike.check.is_some());
            if let Some(unlock) = &strike.unlock {
                assert!(derived.active_talents.contains(unlock));
            }
        }
        let stored = serde_json::to_vec(&BuildRecord::capture(&build)).unwrap();
        let restored: BuildRecord = serde_json::from_slice(&stored).unwrap();
        assert_eq!(
            restored.restore(catalog.content().clone()).unwrap(),
            build,
            "{name}"
        );
    }
}

#[test]
fn rogue_roster_unlock_purchases_are_required_and_hobgoblin_owns_its_signature() {
    let (catalog, manifest) = catalog();
    let mut quagga = manifest.archetypes["rogue.quagga"]
        .creature
        .clone()
        .unwrap();
    quagga.hit_dice[1].purchases.clear();
    let untrained = catalog.prepare(&quagga).unwrap().instantiate(17).unwrap();
    assert!(!untrained
        .derive()
        .unwrap()
        .active_talents
        .contains(&TalentId::new("second_strike").unwrap()));
    let trained = catalog
        .prepare(
            manifest.archetypes["rogue.quagga"]
                .creature
                .as_ref()
                .unwrap(),
        )
        .unwrap()
        .instantiate(17)
        .unwrap();
    assert!(trained
        .derive()
        .unwrap()
        .active_talents
        .contains(&TalentId::new("second_strike").unwrap()));
    let hobgoblin = catalog
        .prepare(
            manifest.archetypes["rogue.hobgoblin"]
                .creature
                .as_ref()
                .unwrap(),
        )
        .unwrap()
        .instantiate(17)
        .unwrap();
    assert!(hobgoblin
        .derive()
        .unwrap()
        .abilities
        .contains(&AbilityId::new("rattling_blow").unwrap()));
}
