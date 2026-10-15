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
        ("ice_monster", 1, 1),
        ("vampire", 8, 1),
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

#[test]
fn rogue_vampire_owned_ordinals_and_species_eligibility_are_required() {
    let (catalog, manifest) = catalog();
    let vampire = manifest
        .archetypes
        .get("rogue.vampire")
        .expect("V recipe")
        .creature
        .as_ref()
        .unwrap();
    let trained = catalog.prepare(vampire).unwrap().instantiate(42).unwrap();
    let derived = trained.derive().unwrap();
    let drain = AbilityId::new("rogue.drain_vitality").unwrap();
    let regeneration = TalentId::new("rogue.regeneration").unwrap();
    assert_eq!(trained.ledger().racial_hd(), tor_simulation::HdCount(8));
    assert!(derived.dormant_talents.is_empty());
    assert!(derived.abilities.contains(&drain));
    assert!(derived.active_talents.contains(&regeneration));
    for ordinal in 0..8 {
        let talents: Vec<_> = trained.choices()[ordinal]
            .talents()
            .map(|id| id.as_str())
            .collect();
        assert_eq!(
            talents,
            match ordinal {
                0 => vec!["rogue.drain_vitality"],
                1 => vec!["rogue.regeneration"],
                _ => vec![],
            }
        );
    }
    for ordinal in [0, 1] {
        let mut without = vampire.clone();
        without.hit_dice[ordinal].purchases.clear();
        let without = catalog.prepare(&without).unwrap().instantiate(42).unwrap();
        let sheet = without.derive().unwrap();
        assert_eq!(sheet.abilities.contains(&drain), ordinal != 0);
        assert_eq!(sheet.active_talents.contains(&regeneration), ordinal != 1);
        assert_eq!(
            sheet.health_recovery_period,
            derived.health_recovery_period * if ordinal == 1 { 2 } else { 1 }
        );
        assert_eq!(without.ledger(), trained.ledger());
    }
    for (species, remove) in [("rogue.human", 1), ("rogue.zombie", 0)] {
        let mut invalid = vampire.clone();
        invalid.species = species.into();
        invalid.hit_dice[remove].purchases.clear();
        assert!(catalog.prepare(&invalid).is_err(), "ineligible {species}");
    }
}

#[test]
fn rogue_vampire_drain_signature_reuses_natural_damage_and_spiritual_rider() {
    let (catalog, _) = catalog();
    let source = &catalog.content().catalog().species[&SpeciesId::new("rogue.vampire").unwrap()];
    assert_eq!(source.melee.strikes().len(), 1);
    assert!(source.melee.strikes()[0].riders.is_empty());
    let natural = serde_json::to_value(&source.melee.strikes()[0]).unwrap();
    assert_eq!(
        natural["on_success"][0]["damage"]["components"][0]["amount"],
        serde_json::json!({"kind":"rolled","dice":{"count":1,"sides":10,"bonus":0}})
    );
    let ability = AbilityId::new("rogue.drain_vitality").unwrap();
    let signature =
        serde_json::to_value(catalog.content().rules().ability(&ability).unwrap()).unwrap();
    assert_eq!(signature["strikes"]["kind"], "natural_melee");
    let riders = signature["strikes"]["additional_riders"]
        .as_array()
        .unwrap();
    assert_eq!(riders.len(), 1);
    assert_eq!(riders[0]["trigger"], "success");
    assert_eq!(
        riders[0]["check"],
        serde_json::json!({"skill":{"kind":"natural_attack"},"defense":"spiritual"})
    );
    assert_eq!(
        riders[0]["effects"],
        serde_json::json!([{"kind":"lose_maximum_health","amount":{"kind":"rolled","dice":{"count":1,"sides":3,"bonus":0}}}])
    );
}

#[test]
fn rogue_vampire_growth_keeps_exact_rows_seeds_choices_and_authored_restoration() {
    use tor_simulation::{
        creatures::RestorationPolicy,
        prepared_spawn::{PopulationCapacity, PreparedSpawnError},
        progression::{HdLedger, HdSource},
    };
    let (catalog, manifest) = catalog();
    let vampire = manifest
        .archetypes
        .get("rogue.vampire")
        .expect("V recipe")
        .creature
        .as_ref()
        .unwrap();
    let seed = actor_health_seed(42, 3);
    let base = catalog.prepare(vampire).unwrap().instantiate(seed).unwrap();
    for (depth, expected) in [(26u32, 8), (27, 9), (274, 256)] {
        let grown = base
            .instantiate_seeded_racial(seed, u64::from(depth.saturating_sub(26)))
            .unwrap();
        assert_eq!(grown.ledger().total_hd().get(), expected as u64);
        assert_eq!(
            grown.ledger(),
            &HdLedger::seeded(vec![HdSource::Racial; expected], seed).unwrap()
        );
        assert_eq!(&grown.ledger().entries()[..8], base.ledger().entries());
        assert_eq!(&grown.choices()[..8], base.choices());
        assert!(grown.choices()[8..]
            .iter()
            .all(|choice| *choice == Default::default()));
        let RestorationPolicy::Authored(plan) = grown.restoration_policy() else {
            panic!("authored restoration")
        };
        assert_eq!(plan.len(), expected);
        for (ordinal, planned) in plan.iter().enumerate() {
            assert_eq!(planned.source, HdSource::Racial);
            assert_eq!(planned.choices, grown.choices()[ordinal]);
        }
        assert!(grown.derive().unwrap().dormant_talents.is_empty());
        let saved = serde_json::to_vec(&BuildRecord::capture(&grown)).unwrap();
        let restored: BuildRecord = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored.restore(catalog.content().clone()).unwrap(), grown);
    }
    assert_eq!(
        base.instantiate_seeded_racial(seed, 249),
        Err(PreparedSpawnError::Capacity(PopulationCapacity::HitDice {
            requested: 257,
            maximum: 256
        }))
    );
}

#[test]
fn rogue_vampire_recovery_and_restorable_injury_survive_owned_grant_rebuild() {
    use tor_simulation::creatures::{record::CreatureRecord, CreatureState};
    let (catalog, manifest) = catalog();
    let spec = manifest.archetypes["rogue.vampire"]
        .creature
        .as_ref()
        .unwrap();
    let build = catalog.prepare(spec).unwrap().instantiate(42).unwrap();
    let mut creature = CreatureState::new(build.clone()).unwrap();
    creature.damage(4);
    let period = creature.health_recovery().period();
    creature.advance_active(period / 2);
    let elapsed = creature.health_recovery().elapsed();
    assert_eq!(elapsed, period / 2);
    let injury = creature.health().injury();
    creature.lose_maximum_health(2).unwrap();
    assert_eq!(creature.health().injury(), injury);
    assert_eq!(creature.health_recovery().elapsed(), elapsed);
    let mut without = spec.clone();
    without.hit_dice[1].purchases.clear();
    creature
        .rebuild(catalog.prepare(&without).unwrap().instantiate(42).unwrap())
        .unwrap();
    assert_eq!(creature.health_recovery().period(), 2 * period);
    assert_eq!(creature.health_recovery().elapsed(), 2 * elapsed);
    assert_eq!(creature.restorable_losses().maximum_health, 2);
    creature.rebuild(build).unwrap();
    assert_eq!(creature.health_recovery().elapsed(), elapsed);
    let encoded = serde_json::to_vec(&CreatureRecord::capture(&creature)).unwrap();
    let record: CreatureRecord = serde_json::from_slice(&encoded).unwrap();
    let mut restored = record.restore(catalog.content().clone(), vec![]).unwrap();
    assert_eq!(restored, creature);
    restored.advance_active(period - elapsed);
    assert_eq!(restored.health().injury(), injury - 1);
    assert_eq!(restored.restorable_losses().maximum_health, 2);
    restored.restore_maximum_health(1).unwrap();
    assert_eq!(restored.health().injury(), injury - 1);
    assert_eq!(restored.restorable_losses().maximum_health, 1);
    restored.advance_active(period / 2);
    restored.damage(1);
    assert_eq!(restored.health_recovery().elapsed(), 0);
    restored.kill();
    let dead = restored.clone();
    restored.advance_active(u64::MAX);
    assert_eq!(restored.restore_maximum_health(1).unwrap().applied, 0);
    assert_eq!(restored, dead);
}
