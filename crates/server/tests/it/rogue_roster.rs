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
        ("rattlesnake", 2, 1),
        ("hobgoblin", 1, 1),
        ("zombie", 2, 1),
        ("quagga", 3, 2),
        ("centaur", 4, 3),
        ("yeti", 4, 2),
        ("troll", 6, 3),
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
fn rogue_troll_compiles_exact_racial_rows_owned_grants_and_authored_restoration() {
    use tor_simulation::{
        creatures::RestorationPolicy,
        progression::{HdLedger, HdSource},
    };
    let (catalog, manifest) = catalog();
    let troll = manifest.archetypes["rogue.troll"]
        .creature
        .as_ref()
        .unwrap();
    let seed = actor_health_seed(42, 3);
    let build = catalog.prepare(troll).unwrap().instantiate(seed).unwrap();
    assert_eq!(
        build.ledger(),
        &HdLedger::seeded(vec![HdSource::Racial; 6], seed).unwrap()
    );
    let expected = [
        vec!["rogue.regeneration"],
        vec!["second_strike"],
        vec![],
        vec!["third_strike"],
        vec![],
        vec![],
    ];
    for (choice, talents) in build.choices().iter().zip(&expected) {
        assert_eq!(
            choice.talents().map(|id| id.as_str()).collect::<Vec<_>>(),
            *talents
        );
    }
    let derived = build.derive().unwrap();
    assert!(derived.dormant_talents.is_empty());
    assert_eq!(
        derived.active_talents,
        ["rogue.regeneration", "second_strike", "third_strike"]
            .into_iter()
            .map(|id| TalentId::new(id).unwrap())
            .collect()
    );
    // Removing RG1 leaves the natural-strike purchases active. The unchanged
    // Vampire tests own RG1's half-period recovery and rebuild semantics.
    let mut without_rg1 = troll.clone();
    without_rg1.hit_dice[0].purchases.clear();
    let without_rg1 = catalog
        .prepare(&without_rg1)
        .unwrap()
        .instantiate(seed)
        .unwrap();
    assert_eq!(without_rg1.ledger(), build.ledger());
    assert_eq!(
        without_rg1.derive().unwrap().active_talents,
        ["second_strike", "third_strike"]
            .into_iter()
            .map(|id| TalentId::new(id).unwrap())
            .collect()
    );
    let RestorationPolicy::Authored(plan) = build.restoration_policy() else {
        panic!("authored restoration")
    };
    assert_eq!(plan.len(), 6);
    for (ordinal, planned) in plan.iter().enumerate() {
        assert_eq!(planned.source, HdSource::Racial);
        assert_eq!(planned.choices, build.choices()[ordinal]);
    }
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

#[test]
fn rogue_rattlesnake_owned_venom_and_canonical_strength_restoration_bind_existing_mechanics() {
    use tor_simulation::{
        attributes::Attribute,
        creatures::{record::CreatureRecord, CreatureState, RestorationPolicy},
        progression::{HdLedger, HdSource},
        resources::Resource,
    };
    let (catalog, manifest) = catalog();
    let spec = manifest.archetypes["rogue.rattlesnake"]
        .creature
        .as_ref()
        .unwrap();
    let seed = actor_health_seed(42, 3);
    let build = catalog.prepare(spec).unwrap().instantiate(seed).unwrap();
    assert_eq!(
        build.ledger(),
        &HdLedger::seeded(vec![HdSource::Racial; 2], seed).unwrap()
    );
    assert_eq!(build.unspent_points(), 1);
    assert_eq!(
        build.choices()[0]
            .talents()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        ["rogue.venomous_bite"]
    );
    assert_eq!(build.choices()[1].talents().count(), 0);
    let RestorationPolicy::Authored(plan) = build.restoration_policy() else {
        panic!("authored R restoration")
    };
    assert_eq!(plan.len(), 2);
    let venom = AbilityId::new("rogue.venomous_bite").unwrap();
    assert!(build.derive().unwrap().abilities.contains(&venom));
    let mut untrained = spec.clone();
    untrained.hit_dice[0].purchases.clear();
    let untrained = catalog
        .prepare(&untrained)
        .unwrap()
        .instantiate(seed)
        .unwrap();
    assert_eq!(untrained.ledger(), build.ledger());
    assert_eq!(untrained.unspent_points(), 2);
    assert!(!untrained.derive().unwrap().abilities.contains(&venom));
    let mut ineligible = spec.clone();
    ineligible.species = "rogue.snake".into();
    assert!(catalog.prepare(&ineligible).is_err());

    let species =
        &catalog.content().catalog().species[&SpeciesId::new("rogue.rattlesnake").unwrap()];
    let natural = serde_json::to_value(species.melee.strikes()).unwrap();
    assert_eq!(natural.as_array().unwrap().len(), 1);
    assert_eq!(natural[0]["descriptors"], serde_json::json!([]));
    assert_eq!(natural[0]["riders"], serde_json::json!([]));
    assert_eq!(
        natural[0]["on_success"][0]["damage"]["components"][0]["amount"],
        serde_json::json!({"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}})
    );
    let signature =
        serde_json::to_value(catalog.content().rules().ability(&venom).unwrap()).unwrap();
    assert_eq!(
        signature["strikes"],
        serde_json::json!({"kind":"natural_melee", "additional_riders":[{
        "trigger":"success", "check":{"skill":{"kind":"natural_attack"},"defense":"physical"},
        "effects":[{"kind":"lose_attribute","attribute":"strength","amount":{"kind":"fixed","amount":1}}],
        "descriptors":["poison"]}]})
    );
    // Generic resolver owners prove primary/rider misses and descriptor immunity;
    // this compiled binding proves R uses that exact existing two-check mechanism.
    let potion = &manifest.archetypes["rogue.restore_strength"];
    let dose = u32::from(tor_simulation::limits::DERIVED_ATTRIBUTE);
    assert_eq!(
        serde_json::to_value(potion.consumable.as_ref().unwrap()).unwrap(),
        serde_json::json!({"effects":[{"kind":"restore_attribute","attribute":"strength",
            "amount":{"kind":"fixed","amount":dose}}]})
    );
    let player = manifest.characters[0].creature.as_ref().unwrap();
    let mut state =
        CreatureState::new(catalog.prepare(player).unwrap().instantiate(42).unwrap()).unwrap();
    state.damage(1);
    let initial_strength = state.derived().attributes.get(Attribute::Strength);
    assert_eq!(
        state
            .lose_attribute(Attribute::Strength, 1)
            .unwrap()
            .applied,
        1
    );
    let injury = state.health().injury();
    let balances = Resource::ALL.map(|resource| state.costs().resources().balance(resource));
    let quiet = state.received_health_loss_remaining();
    assert_eq!(
        state
            .restore_attribute(Attribute::Strength, dose)
            .unwrap()
            .applied,
        1
    );
    assert_eq!(
        state.derived().attributes.get(Attribute::Strength),
        initial_strength
    );
    assert_eq!(state.health().injury(), injury);
    assert_eq!(state.received_health_loss_remaining(), quiet);
    assert_eq!(
        Resource::ALL.map(|resource| state.costs().resources().balance(resource)),
        balances
    );
    let restored = state.clone();
    assert_eq!(
        state
            .restore_attribute(Attribute::Strength, dose)
            .unwrap()
            .applied,
        0
    );
    assert_eq!(state, restored);
    let encoded = serde_json::to_vec(&CreatureRecord::capture(&state)).unwrap();
    let record: CreatureRecord = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        record.restore(catalog.content().clone(), vec![]).unwrap(),
        state
    );
}
