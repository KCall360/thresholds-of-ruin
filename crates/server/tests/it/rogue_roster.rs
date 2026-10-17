//! Newly reconstructed source-qualified roster compilation and record coverage.
use std::{num::NonZeroU64, path::Path};
use tor_server::{
    creature_authoring::actor_health_seed,
    scenario_package::{resolve_rules_source, resolve_source},
};
use tor_simulation::{
    combat::CombatEvent,
    content_ids::{AbilityId, SpeciesId, TalentId},
    creatures::record::BuildRecord,
    rules::Ruleset,
    Action, ActorId, CreatureIdentity, Game,
};
use tor_world::{Extent, Location, Position, Region, RegionId, World};

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
        ("bat", 1, 1),
        ("kestrel", 1, 1),
        ("venus_flytrap", 8, 1),
        ("griffin", 13, 2),
        ("emu", 1, 1),
        ("snake", 1, 1),
        ("rattlesnake", 2, 1),
        ("hobgoblin", 1, 1),
        ("zombie", 2, 1),
        ("quagga", 3, 2),
        ("centaur", 4, 3),
        ("yeti", 4, 2),
        ("troll", 6, 3),
        ("wraith", 5, 1),
        ("black_unicorn", 7, 3),
        ("ice_monster", 1, 1),
        ("vampire", 8, 1),
        ("jabberwock", 15, 2),
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
        "trigger":"success", "condition":null, "check":{"skill":{"kind":"natural_attack"},"defense":"physical"},
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

#[test]
fn rogue_wraith_owns_five_seeded_rows_parentless_drain_and_four_unspent_points() {
    use tor_simulation::{
        creatures::RestorationPolicy,
        progression::{HdLedger, HdSource},
    };
    let (catalog, manifest) = catalog();
    let spec = manifest.archetypes["rogue.wraith"]
        .creature
        .as_ref()
        .unwrap();
    let seed = actor_health_seed(42, 3);
    let build = catalog.prepare(spec).unwrap().instantiate(seed).unwrap();
    assert_eq!(
        build.ledger(),
        &HdLedger::seeded(vec![HdSource::Racial; 5], seed).unwrap()
    );
    assert_eq!(build.unspent_points(), 4);
    assert_eq!(
        build.choices()[0]
            .talents()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        ["rogue.energy_drain"]
    );
    assert!(build.choices()[1..]
        .iter()
        .all(|choice| *choice == Default::default()));
    let RestorationPolicy::Authored(plan) = build.restoration_policy() else {
        panic!("authored W restoration")
    };
    assert_eq!(plan.len(), 5);
    for (ordinal, row) in plan.iter().enumerate() {
        assert_eq!(row.source, HdSource::Racial);
        assert_eq!(row.choices, build.choices()[ordinal]);
    }
    let drain = AbilityId::new("rogue.energy_drain").unwrap();
    let derived = build.derive().unwrap();
    assert!(derived.dormant_talents.is_empty());
    assert!(derived.abilities.contains(&drain));
    assert_eq!(
        derived.active_talents,
        [TalentId::new("rogue.energy_drain").unwrap()]
            .into_iter()
            .collect()
    );
    let mut untrained = spec.clone();
    untrained.hit_dice[0].purchases.clear();
    let untrained = catalog
        .prepare(&untrained)
        .unwrap()
        .instantiate(seed)
        .unwrap();
    assert_eq!(untrained.ledger(), build.ledger());
    assert_eq!(untrained.unspent_points(), 5);
    assert!(!untrained.derive().unwrap().abilities.contains(&drain));
    let mut ineligible = spec.clone();
    ineligible.species = "rogue.human".into();
    assert!(catalog.prepare(&ineligible).is_err());
    // Actual HD1 ownership alone qualifies: DR2 has no DR1 parent or HD2 gate.
    let mut first = spec.clone();
    first.hit_dice.truncate(1);
    let first = catalog.prepare(&first).unwrap().instantiate(seed).unwrap();
    assert!(first.derive().unwrap().abilities.contains(&drain));
    assert_eq!(first.unspent_points(), 0);

    let species = &catalog.content().catalog().species[&SpeciesId::new("rogue.wraith").unwrap()];
    let natural = serde_json::to_value(species.melee.strikes()).unwrap();
    assert_eq!(natural.as_array().unwrap().len(), 1);
    assert_eq!(natural[0]["descriptors"], serde_json::json!([]));
    assert_eq!(natural[0]["riders"], serde_json::json!([]));
    assert_eq!(
        natural[0]["on_success"][0]["damage"]["components"][0]["amount"],
        serde_json::json!({"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}})
    );
    let signature =
        serde_json::to_value(catalog.content().rules().ability(&drain).unwrap()).unwrap();
    assert_eq!(
        signature["strikes"],
        serde_json::json!({"kind":"natural_melee", "additional_riders":[{
            "trigger":"success", "condition":null, "check":{"skill":{"kind":"natural_attack"},"defense":"spiritual"},
            "effects":[{"kind":"drain_latest_hit_die"}], "descriptors":[]}]})
    );
    // Shared resolver/loss tests own fatality, injury, ceilings and rollback.
    // This test owns W's exact compiled purchase, eligibility and signature.
}

#[test]
fn rogue_jabberwock_owns_exact_seeded_rows_gates_and_restorable_rending_grant() {
    use tor_simulation::{
        creatures::{record::CreatureRecord, CreatureState, RestorationPolicy},
        progression::{HdLedger, HdSource},
    };
    let (catalog, manifest) = catalog();
    let spec = manifest.archetypes["rogue.jabberwock"]
        .creature
        .as_ref()
        .unwrap();
    let seed = actor_health_seed(42, 3);
    let build = catalog.prepare(spec).unwrap().instantiate(seed).unwrap();
    assert_eq!(
        build.ledger(),
        &HdLedger::seeded(vec![HdSource::Racial; 15], seed).unwrap()
    );
    assert_eq!(build.unspent_points(), 13);
    for (ordinal, choices) in build.choices().iter().enumerate() {
        let expected = match ordinal {
            1 => vec!["second_strike"],
            7 => vec!["rending_pair"],
            _ => vec![],
        };
        assert_eq!(
            choices.talents().map(|id| id.as_str()).collect::<Vec<_>>(),
            expected
        );
    }
    let RestorationPolicy::Authored(plan) = build.restoration_policy() else {
        panic!("authored J restoration")
    };
    assert_eq!(plan.len(), 15);
    for (ordinal, row) in plan.iter().enumerate() {
        assert_eq!(row.source, HdSource::Racial);
        assert_eq!(row.choices, build.choices()[ordinal]);
    }
    let rending = AbilityId::new("rending_pair").unwrap();
    let derived = build.derive().unwrap();
    assert!(derived.dormant_talents.is_empty());
    assert!(derived.abilities.contains(&rending));
    assert_eq!(
        derived.active_talents,
        ["second_strike", "rending_pair"]
            .into_iter()
            .map(|id| TalentId::new(id).unwrap())
            .collect()
    );
    let mut untrained = spec.clone();
    untrained.hit_dice[7].purchases.clear();
    let untrained = catalog
        .prepare(&untrained)
        .unwrap()
        .instantiate(seed)
        .unwrap();
    assert_eq!(untrained.ledger(), build.ledger());
    assert!(!untrained.derive().unwrap().abilities.contains(&rending));
    let mut missing_parent = spec.clone();
    missing_parent.hit_dice[1].purchases.clear();
    assert!(catalog.prepare(&missing_parent).is_err());
    for (owner, earlier) in [(1, 0), (7, 6)] {
        let mut too_early = spec.clone();
        too_early.hit_dice.swap(owner, earlier);
        // The row owns a point, not a historical purchase time. Eligibility
        // requires a genuinely below-threshold current build.
        too_early.hit_dice.truncate(owner);
        assert!(catalog.prepare(&too_early).is_err());
    }
    let mut ineligible = spec.clone();
    ineligible.species = "rogue.human".into();
    assert!(catalog.prepare(&ineligible).is_err());
    // Shared flag/HD eligibility is reusable, not a Jabberwock identity branch.
    let mut other_sequence = spec.clone();
    other_sequence.species = "rogue.troll".into();
    assert!(catalog
        .prepare(&other_sequence)
        .unwrap()
        .instantiate(seed)
        .unwrap()
        .derive()
        .unwrap()
        .abilities
        .contains(&rending));

    let mut state = CreatureState::new(build.clone()).unwrap();
    for _ in 0..8 {
        assert_eq!(state.drain_latest_hit_die().unwrap().applied, 1);
    }
    assert_eq!(state.build().ledger().total_hd().get(), 7);
    assert_eq!(
        state.build().ledger().entries(),
        &build.ledger().entries()[..7]
    );
    assert!(!state.derived().abilities.contains(&rending));
    assert!(state
        .derived()
        .active_talents
        .contains(&TalentId::new("second_strike").unwrap()));
    let encoded = serde_json::to_vec(&CreatureRecord::capture(&state)).unwrap();
    let record: CreatureRecord = serde_json::from_slice(&encoded).unwrap();
    let mut restored = record.restore(catalog.content().clone(), vec![]).unwrap();
    assert_eq!(restored, state);
    assert_eq!(restored.restore_hit_die().unwrap().applied, 1);
    assert_eq!(
        restored.build().ledger().entries(),
        &build.ledger().entries()[..8]
    );
    assert_eq!(restored.build().choices()[7], build.choices()[7]);
    assert!(restored.derived().abilities.contains(&rending));
    for _ in 0..7 {
        assert_eq!(restored.restore_hit_die().unwrap().applied, 1);
    }
    assert_eq!(restored.build(), &build);
}

#[test]
fn rogue_jabberwock_signature_imports_natural_rows_and_one_shared_conditional_rider() {
    let (catalog, _) = catalog();
    let source = &catalog.content().catalog().species[&SpeciesId::new("rogue.jabberwock").unwrap()];
    let natural = serde_json::to_value(source.melee.strikes()).unwrap();
    assert_eq!(natural.as_array().unwrap().len(), 2);
    for (index, sides) in [12, 4].into_iter().enumerate() {
        assert_eq!(natural[index]["riders"], serde_json::json!([]));
        assert_eq!(natural[index]["descriptors"], serde_json::json!([]));
        assert_eq!(
            natural[index]["on_success"][0]["damage"]["components"][0],
            serde_json::json!({"category":"keen", "descriptor":null,
                "amount":{"kind":"rolled","dice":{"count":2,"sides":sides,"bonus":0}}})
        );
    }
    let ability = AbilityId::new("rending_pair").unwrap();
    let signature =
        serde_json::to_value(catalog.content().rules().ability(&ability).unwrap()).unwrap();
    assert_eq!(
        signature["timing"],
        serde_json::json!({"source":{"kind":"selected_weapon"},"scaled_by":"speed"})
    );
    assert_eq!(
        signature["strikes"],
        serde_json::json!({"kind":"natural_melee", "additional_riders":[{
            "trigger":"success", "condition":{"kind":"sequence_successful_hits","minimum":2}, "check":null,
            "effects":[{"kind":"damage","damage":{"kind":"components","components":[{
                "category":"keen","descriptor":null,"amount":{"kind":"rolled","dice":{"count":1,"sides":6,"bonus":0}}}]}}],
            "descriptors":[]}]})
    );
}

#[test]
fn rogue_jabberwock_growth_preserves_15_owned_rows_and_refuses_257_hd_at_268() {
    assert_roster_growth("jabberwock", 15, 267);
}

#[test]
fn rogue_griffin_growth_preserves_13_owned_rows_and_refuses_257_hd_at_270() {
    assert_roster_growth("griffin", 13, 269);
}

fn assert_roster_growth(name: &str, owned: usize, last_depth: u32) {
    use tor_simulation::{
        creatures::RestorationPolicy,
        prepared_spawn::{PopulationCapacity, PreparedSpawnError},
        progression::{HdLedger, HdSource},
    };
    let (catalog, manifest) = catalog();
    let spec = manifest.archetypes[&format!("rogue.{name}")]
        .creature
        .as_ref()
        .unwrap();
    let seed = actor_health_seed(42, 3);
    let base = catalog.prepare(spec).unwrap().instantiate(seed).unwrap();
    for (depth, expected) in [(26u32, owned), (27, owned + 1), (last_depth, 256)] {
        let grown = base
            .instantiate_seeded_racial(seed, u64::from(depth.saturating_sub(26)))
            .unwrap();
        assert_eq!(
            grown.ledger(),
            &HdLedger::seeded(vec![HdSource::Racial; expected], seed).unwrap()
        );
        assert_eq!(&grown.ledger().entries()[..owned], base.ledger().entries());
        assert_eq!(&grown.choices()[..owned], base.choices());
        assert!(grown.choices()[owned..]
            .iter()
            .all(|choice| *choice == Default::default()));
        let RestorationPolicy::Authored(plan) = grown.restoration_policy() else {
            panic!("authored {name} growth restoration")
        };
        assert_eq!(plan.len(), expected);
        for (ordinal, row) in plan.iter().enumerate() {
            assert_eq!(row.source, HdSource::Racial);
            assert_eq!(row.choices, grown.choices()[ordinal]);
        }
        let saved = serde_json::to_vec(&BuildRecord::capture(&grown)).unwrap();
        let restored: BuildRecord = serde_json::from_slice(&saved).unwrap();
        assert_eq!(restored.restore(catalog.content().clone()).unwrap(), grown);
    }
    assert_eq!(
        base.instantiate_seeded_racial(seed, 257 - owned as u64),
        Err(PreparedSpawnError::Capacity(PopulationCapacity::HitDice {
            requested: 257,
            maximum: 256
        }))
    );
}

fn completed(game: &mut Game, actor: ActorId, action: Action) -> Vec<CombatEvent> {
    let mut events = Vec::new();
    for _ in 0..32 {
        if game.next_actor() == Some(actor) {
            break;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        events.extend_from_slice(game.combat_events());
    }
    assert_eq!(game.next_actor(), Some(actor));
    game.act(actor, action).unwrap();
    events.extend_from_slice(game.combat_events());
    for _ in 0..32 {
        if game.preparation(actor).is_none() && game.next_actor() == Some(actor) {
            return events;
        }
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
        events.extend_from_slice(game.combat_events());
    }
    panic!("ordinary fixture action must settle within its bounded turn budget");
}

fn production_combat_fixture(name: &str) -> (Game, ActorId, ActorId) {
    let (original, manifest) = catalog();
    let mut authored = manifest.creatures.clone();
    // Only the diagnostic victim is adjusted: bounded contact defense, enough
    // health, and a guaranteed single opposed Escape. The attacking production
    // recipe, racial rows, purchases, body, natural checks, and rules stay intact.
    authored.species.get_mut("rogue.human").unwrap().modifiers.extend(
        serde_json::from_value::<Vec<tor_server::creature_authoring::Modifier>>(serde_json::json!([
            {"scope":"always","op":{"kind":"add","stat":{"kind":"attribute","attribute":"strength"},"value":100}},
            {"scope":"always","op":{"kind":"set","stat":{"kind":"defense","defense":"physical"},"value":0}},
            {"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":200}}
        ])).unwrap(),
    );
    let catalog = authored
        .compile(original.content().rules().clone())
        .unwrap();
    let species_id = SpeciesId::new(format!("rogue.{name}")).unwrap();
    assert_eq!(
        catalog.content().catalog().species[&species_id],
        original.content().catalog().species[&species_id]
    );
    let recipe = &manifest.archetypes[&format!("rogue.{name}")];
    let spec = recipe.creature.as_ref().unwrap();
    let mut game = Game::new(
        World::new(
            vec![Region {
                id: RegionId(1),
                name: "Production combat witness".into(),
                bounds: Extent::new(7, 5, 3).unwrap(),
            }],
            vec![],
        )
        .unwrap(),
        42,
        catalog.content().clone(),
    );
    let at = |x| Location {
        region: RegionId(1),
        position: Position { x, y: 2, z: 0 },
    };
    let holder = game
        .spawn_actor(at(2), NonZeroU64::new(recipe.turn_ticks.unwrap()).unwrap())
        .unwrap();
    game.configure_creature(
        holder,
        CreatureIdentity {
            name: spec.name.clone(),
            faction: spec.faction.clone(),
        },
        catalog
            .prepare(spec)
            .unwrap()
            .instantiate(actor_health_seed(42, holder.0))
            .unwrap(),
    )
    .unwrap();
    game.set_body(holder, recipe.body.clone().unwrap().into())
        .unwrap();
    let subject_spec = manifest.characters[0].creature.as_ref().unwrap();
    let subject = game
        .spawn_actor(at(3), NonZeroU64::new(100).unwrap())
        .unwrap();
    game.configure_creature(
        subject,
        CreatureIdentity {
            name: "Combat subject".into(),
            faction: "delvers".into(),
        },
        catalog
            .prepare(subject_spec)
            .unwrap()
            .instantiate(actor_health_seed(42, subject.0))
            .unwrap(),
    )
    .unwrap();
    game.set_body(subject, manifest.characters[0].body.clone().unwrap().into())
        .unwrap();
    (game, holder, subject)
}

#[test]
fn production_flytrap_uses_ordinary_clutch_tightening_and_single_escape() {
    use tor_simulation::abilities::{AbilityInvocation, ResolvedEffect};
    use tor_world::Direction;
    let (mut game, holder, subject) = production_combat_fixture("venus_flytrap");
    let at = |x| Location {
        region: RegionId(1),
        position: Position { x, y: 2, z: 0 },
    };
    let before = game.clone();
    assert!(game.act(holder, Action::Move(Direction::North)).is_err());
    assert_eq!(
        game, before,
        "the real F recipe stays stationary without charging a turn"
    );
    let clutch = || Action::UseAbility {
        id: AbilityId::new("clutch").unwrap(),
        invocation: AbilityInvocation::ActorTarget { target: subject },
        supplies: Default::default(),
    };
    for _ in 0..12 {
        completed(&mut game, holder, clutch());
        if !game.linked_holds(subject).is_empty() {
            break;
        }
    }
    let links = game.linked_holds(subject);
    assert_eq!(
        links.len(),
        1,
        "an ordinary production Clutch hit establishes exactly one hold"
    );
    assert_eq!(links[0].holder, holder);
    assert_eq!(
        game.creature(subject).unwrap().health().injury(),
        0,
        "the creating F contact has zero damage"
    );
    // Later qualifying attacks use the same live relationship regardless of ability ID.
    let basic_melee = || Action::UseAbility {
        id: AbilityId::new("basic_melee").unwrap(),
        invocation: AbilityInvocation::ActorTarget { target: subject },
        supplies: Default::default(),
    };
    let mut tightened = false;
    for attempt in 1..=12u32 {
        let events = completed(&mut game, holder, basic_melee());
        assert_eq!(
            game.linked_holds(subject),
            links,
            "later hits preserve the exact relationship"
        );
        let damage: u32 = events
            .iter()
            .filter_map(|event| match event {
                CombatEvent::Resolved { outcome, .. }
                    if outcome.actor == holder && outcome.target == subject =>
                {
                    Some(
                        outcome
                            .effects
                            .iter()
                            .filter_map(|effect| match effect {
                                ResolvedEffect::Damage { amount } => Some(*amount),
                                _ => None,
                            })
                            .sum::<u32>(),
                    )
                }
                _ => None,
            })
            .sum();
        if damage > 0 {
            assert_eq!(
                damage, attempt,
                "GR2 increments each later primary, including any miss"
            );
            tightened = true;
            break;
        }
    }
    assert!(
        tightened,
        "the real F natural HoldDamage channel must deal its later GR2 damage"
    );
    let events = completed(&mut game, subject, Action::Escape { hold: links[0].id });
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event,
        CombatEvent::EscapeResolved { actor, hold, success: true, .. }
            if *actor == subject && *hold == links[0].id))
            .count(),
        1
    );
    assert!(game.linked_holds(subject).is_empty());
    completed(&mut game, subject, Action::Move(Direction::East));
    assert_eq!(game.observe(subject).unwrap().location, at(4));
    assert_eq!(
        game.observe(holder).unwrap().location,
        at(2),
        "attacking and tightening never move F"
    );
}

#[test]
fn production_flying_roster_executes_authored_basic_melee_primaries_and_damage() {
    use tor_simulation::abilities::{AbilityInvocation, ResolvedEffect};

    for (name, primary_count) in [("bat", 1_u16), ("kestrel", 1), ("griffin", 2)] {
        let (mut game, attacker, subject) = production_combat_fixture(name);
        let basic = AbilityId::new("basic_melee").unwrap();
        let expected: Vec<_> = (0..primary_count).collect();
        let mut damaged = vec![false; usize::from(primary_count)];
        for _ in 0..12 {
            let events = completed(
                &mut game,
                attacker,
                Action::UseAbility {
                    id: basic.clone(),
                    invocation: AbilityInvocation::ActorTarget { target: subject },
                    supplies: Default::default(),
                },
            );
            let primaries: Vec<_> = events
                .iter()
                .filter_map(|event| match event {
                    CombatEvent::Resolved {
                        outcome,
                        strike: Some(ordinal),
                    } if outcome.actor == attacker
                        && outcome.target == subject
                        && outcome.ability.as_ref() == Some(&basic) =>
                    {
                        Some((*ordinal, outcome))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                primaries
                    .iter()
                    .map(|(ordinal, _)| *ordinal)
                    .collect::<Vec<_>>(),
                expected,
                "{name}: ordinary basic melee executes every authored primary once, in order"
            );
            for (ordinal, outcome) in primaries {
                damaged[usize::from(ordinal)] |= outcome.effects.iter().any(
                    |effect| matches!(effect, ResolvedEffect::Damage { amount } if *amount > 0),
                );
            }
            if damaged.iter().all(|damage| *damage) {
                break;
            }
        }
        assert!(
            damaged.iter().all(|damage| *damage),
            "{name}: every authored natural primary eventually deals positive damage"
        );
        assert!(
            game.creature(subject).unwrap().health().injury() > 0,
            "{name}"
        );
    }
}

#[test]
fn rogue_flytrap_clutch_has_shared_policy_and_tightening_is_owned_at_hd_six() {
    use tor_simulation::creatures::CreatureState;
    let (catalog, manifest) = catalog();
    let spec = manifest.archetypes["rogue.venus_flytrap"]
        .creature
        .as_ref()
        .unwrap();
    let build = catalog.prepare(spec).unwrap().instantiate(42).unwrap();
    let clutch = AbilityId::new("clutch").unwrap();
    let basic = AbilityId::new("basic_melee").unwrap();
    for ability in [&clutch, &basic] {
        assert_eq!(
            build
                .derive()
                .unwrap()
                .ability_values(ability)
                .hold_damage_increment,
            1,
            "{ability}"
        );
    }
    let source =
        &catalog.content().catalog().species[&SpeciesId::new("rogue.venus_flytrap").unwrap()];
    let natural = serde_json::to_value(source.melee.strikes()).unwrap();
    assert_eq!(
        natural[0]["on_success"],
        serde_json::json!([{
            "kind":"hold_damage", "category":"impact", "descriptor":null
        }])
    );
    let ability =
        serde_json::to_value(catalog.content().rules().ability(&clutch).unwrap()).unwrap();
    assert_eq!(
        ability["strikes"],
        serde_json::json!({
            "kind":"natural_melee", "additional_riders":[{
                "trigger":"success", "condition":null, "check":null, "descriptors":[],
                "effects":[{"kind":"apply_linked_hold", "policy":{
                    "sustaining":"clutch", "preparation":100, "recovery":100, "protection":400
                }}]
            }]
        })
    );
    let mut without_gr2 = spec.clone();
    without_gr2.hit_dice[5].purchases.clear();
    let without_gr2 = catalog
        .prepare(&without_gr2)
        .unwrap()
        .instantiate(42)
        .unwrap();
    assert!(without_gr2.derive().unwrap().abilities.contains(&clutch));
    for ability in [&clutch, &basic] {
        assert_eq!(
            without_gr2
                .derive()
                .unwrap()
                .ability_values(ability)
                .hold_damage_increment,
            0,
            "{ability}"
        );
    }
    let mut missing_parent = spec.clone();
    missing_parent.hit_dice[0].purchases.clear();
    assert!(catalog.prepare(&missing_parent).is_err());
    let mut ineligible = spec.clone();
    ineligible.species = "rogue.griffin".into();
    assert!(catalog.prepare(&ineligible).is_err());
    let mut state = CreatureState::new(build.clone()).unwrap();
    for _ in 0..3 {
        assert_eq!(state.drain_latest_hit_die().unwrap().applied, 1);
    }
    assert_eq!(state.build().ledger().total_hd().get(), 5);
    assert!(state.derived().abilities.contains(&clutch));
    for ability in [&clutch, &basic] {
        assert_eq!(
            state
                .derived()
                .ability_values(ability)
                .hold_damage_increment,
            0,
            "{ability}"
        );
    }
    assert!(state.derived().locomotion.stationary);
    assert_eq!(state.restore_hit_die().unwrap().applied, 1);
    assert_eq!(state.build().choices()[5], build.choices()[5]);
    for ability in [&clutch, &basic] {
        assert_eq!(
            state
                .derived()
                .ability_values(ability)
                .hold_damage_increment,
            1,
            "{ability}"
        );
    }
}
