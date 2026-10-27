//! Ring cadence and contact effects through authoritative simulation transitions.
use crate::rules::{Modifier, ModifierOp, Scope};
use crate::{ActorId, EquipmentSlotId, Game, ItemId, ItemSpec};
use std::{collections::BTreeMap, num::NonZeroU64};
use tor_world::{Extent, Location, Position, Region, RegionId, Trap, TrapKind, World};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn modifier(op: ModifierOp) -> Modifier {
    Modifier {
        cancellable: false,
        scope: Scope::Always,
        op,
    }
}
fn fixture(modifiers: Vec<Modifier>) -> (Game, ActorId) {
    fixture_with(modifiers, |_| {})
}
fn fixture_with(
    modifiers: Vec<Modifier>,
    configure: impl FnOnce(&mut crate::rules::RulesetDocument),
) -> (Game, ActorId) {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.conditions.push(serde_json::from_value(serde_json::json!({
        "id":"fixture_sleep","family":"fixture_sleep","name":"Asleep","lifecycle":"timed",
        "modifiers":[],"stacking":{"kind":"extend"},"source_scoped":false,"descriptors":[],"capacity":"reject",
        "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},"restrictions":["voluntary_actions"],"preparation":"pause"
    })).unwrap());
    configure(&mut document);
    let mut species = crate::test_creatures::species();
    species.modifiers.extend(modifiers);
    let species_id = species.id.clone();
    let content = crate::creatures::Content::new(
        crate::rules::Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: BTreeMap::from([(species_id.clone(), species)]),
            templates: BTreeMap::new(),
        },
    )
    .unwrap();
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "test room".into(),
            bounds: Extent::new(6, 4, 2).unwrap(),
        })
        .unwrap();
    let mut game = Game::new(world, 43, content.clone());
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content,
        species_id,
        crate::progression::HdLedger::new(vec![crate::progression::HitDie::new(
            crate::progression::HdSource::Racial,
            42,
        )])
        .unwrap(),
        crate::attributes::ManaBinding::Intellect,
        [
            crate::attributes::Skill::Deception,
            crate::attributes::Skill::Persuasion,
        ],
    )
    .unwrap();
    game.configure_creature(
        actor,
        crate::CreatureIdentity {
            name: "tester".into(),
            faction: "test".into(),
        },
        build,
    )
    .unwrap();
    (game, actor)
}
fn periodic() -> Modifier {
    modifier(ModifierOp::Perception {
        value: crate::perception::PerceptionValue::PeriodicSearch { interval: 100 },
    })
}
fn equip_armor(game: &mut Game, actor: ActorId, rustproof: bool) -> ItemId {
    let mut spec = ItemSpec::ordinary("test armor".into());
    spec.class = crate::ItemClass::Armor;
    spec.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        slot: crate::EquipmentSlot::new("body_armor").unwrap(),
        armor: Some(crate::ArmorSpec {
            physical_reduction: 5,
            material: crate::ArmorMaterial::Metal,
            rustproof,
        }),
        attack: None,
        weapon_profile: None,
        modifiers: vec![],
        potency_modifiers: vec![],
    });
    let item = game
        .place_item_stack(71, at(1), Some(actor), 1, spec)
        .unwrap();
    let socket = game
        .anatomy(actor)
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.as_str() == "body_armor")
        .unwrap();
    game.equip_authored(actor, item, EquipmentSlotId(socket as u16))
        .unwrap();
    item
}
#[test]
fn searching_cadence_is_one_scan_per_hundred_active_ticks_with_duplicate_sources() {
    let (mut one, _) = fixture(vec![periodic()]);
    let (mut two, _) = fixture(vec![periodic(), periodic()]);
    let initial = one.rules_rng;
    assert_eq!(one.advance_physics(99).unwrap(), 99);
    assert_eq!(two.advance_physics(99).unwrap(), 99);
    assert_eq!(one.rules_rng, initial);
    assert_eq!(two.rules_rng, initial);
    one.advance_physics(100).unwrap();
    two.advance_physics(100).unwrap();
    let mut expected = initial;
    // One active scan owns the shared six-die distance-band sample.
    crate::dice::roll_check(&mut expected, crate::dice::Edge::from_counts(3, 0));
    assert_eq!(one.rules_rng, expected);
    assert_eq!(two.rules_rng, expected);
    for _ in 0..3 {
        let before = one.clone();
        one.observe(ActorId(1)).unwrap();
        assert_eq!(one, before);
    }
}

#[test]
fn periodic_settlement_visits_only_pending_owners_not_unrelated_loaded_actors() {
    let (mut game, owner) = fixture(vec![periodic()]);
    game.world
        .add_chamber(Region {
            id: RegionId(2),
            name: "unrelated observers".into(),
            bounds: Extent::new(34, 3, 2).unwrap(),
        })
        .unwrap();
    for x in 1..33 {
        game.spawn_actor(
            Location {
                region: RegionId(2),
                position: Position { x, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    }
    let before = crate::search_clock::search_settlement_candidates();
    game.settle_periodic_searches().unwrap();
    assert_eq!(
        crate::search_clock::search_settlement_candidates() - before,
        0,
        "an empty periodic deadline queue must not sweep unrelated actors"
    );
    let check = game.capture_discovery_check(owner).unwrap();
    game.actors
        .get_mut(&owner)
        .unwrap()
        .search_clock
        .advance(100, check)
        .unwrap();
    let before = crate::search_clock::search_settlement_candidates();
    game.settle_periodic_searches().unwrap();
    assert_eq!(
        crate::search_clock::search_settlement_candidates() - before,
        1
    );
    assert!(!game.actors[&owner].search_clock.pending());
    let before = crate::search_clock::search_settlement_candidates();
    game.settle_periodic_searches().unwrap();
    assert_eq!(
        crate::search_clock::search_settlement_candidates() - before,
        0
    );
}
#[test]
fn sleeping_suppresses_ring_accrual_without_waking_catchup() {
    let (mut game, actor) = fixture(vec![periodic()]);
    let initial = game.rules_rng;
    game.advance_physics(50).unwrap();
    game.apply_effects(
        actor,
        &[crate::rules::Effect::ApplyCondition {
            condition: crate::content_ids::ConditionId::new("fixture_sleep").unwrap(),
            duration: 150,
        }],
    )
    .unwrap();
    game.advance_physics(200).unwrap();
    assert_eq!(game.rules_rng, initial);
    assert!(game.condition_allows_voluntary_input(actor));
    game.advance_physics(249).unwrap();
    assert_eq!(game.rules_rng, initial);
    game.advance_physics(250).unwrap();
    assert_ne!(game.rules_rng, initial);
}
#[test]
fn rust_contact_triggers_once_then_retriggers_only_after_leaving_and_returning() {
    let (mut game, actor) = fixture(vec![]);
    let armor = equip_armor(&mut game, actor, false);
    game.place_trap(
        at(2),
        Trap {
            id: 11,
            dc: 99,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        },
    )
    .unwrap();
    game.teleport(actor, at(2)).unwrap();
    assert_eq!(game.items[&armor].condition.corrosion, 1);
    let rng = game.rules_rng;
    game.observe(actor).unwrap();
    game.reconcile_trap_contacts(actor, None).unwrap();
    assert_eq!(game.items[&armor].condition.corrosion, 1);
    assert_eq!(game.rules_rng, rng);
    assert!(game.feature_family_known(actor, tor_world::ConcealedFeatureId::Trap(11)));
    game.teleport(actor, at(1)).unwrap();
    game.teleport(actor, at(2)).unwrap();
    assert_eq!(game.items[&armor].condition.corrosion, 2);
}
#[test]
fn rust_protection_and_flight_do_not_erase_imposed_floor_collision() {
    let flying = modifier(ModifierOp::Locomotion {
        value: crate::locomotion::LocomotionValue::Flying { enabled: true },
    });
    for rustproof in [false, true] {
        let (mut game, actor) = fixture(vec![flying.clone()]);
        let armor = equip_armor(&mut game, actor, rustproof);
        game.place_trap(
            at(2),
            Trap {
                id: 11,
                dc: 99,
                avoidance_dc: None,
                kind: TrapKind::Rust {
                    slot: "body_armor".into(),
                    amount: 1,
                },
            },
        )
        .unwrap();
        game.teleport(actor, at(2)).unwrap();
        assert_eq!(game.items[&armor].condition.corrosion, 0);
        game.reconcile_trap_contacts(actor, Some([0, 0, 1]))
            .unwrap();
        assert_eq!(
            game.items[&armor].condition.corrosion,
            u32::from(!rustproof)
        );
        game.reconcile_trap_contacts(actor, Some([0, 0, 1]))
            .unwrap();
        assert_eq!(
            game.items[&armor].condition.corrosion,
            u32::from(!rustproof)
        );
    }
}
#[test]
fn sleep_trap_uses_ordinary_timed_condition_and_no_query_retrigger() {
    let (mut game, actor) = fixture(vec![]);
    game.place_trap(
        at(2),
        Trap {
            id: 11,
            dc: 99,
            avoidance_dc: None,
            kind: TrapKind::SleepingGas {
                condition: "fixture_sleep".into(),
                duration: 200,
                exposure: None,
            },
        },
    )
    .unwrap();
    game.teleport(actor, at(2)).unwrap();
    assert!(!game.condition_allows_voluntary_input(actor));
    game.advance_physics(200).unwrap();
    assert!(game.condition_allows_voluntary_input(actor));
    game.observe(actor).unwrap();
    game.reconcile_trap_contacts(actor, None).unwrap();
    assert!(game.condition_allows_voluntary_input(actor));
}

#[test]
fn earned_periodic_scan_settles_before_sleep_source_change_at_its_deadline() {
    let (mut game, actor) = fixture(vec![periodic()]);
    let mut expected = game.rules_rng;
    // One active scan owns the shared six-die distance-band sample.
    crate::dice::roll_check(&mut expected, crate::dice::Edge::from_counts(3, 0));
    // Deliberately exercise an unswept trusted API boundary, not ordinary time travel.
    game.tick = 100;
    game.apply_effects(
        actor,
        &[crate::rules::Effect::ApplyCondition {
            condition: crate::content_ids::ConditionId::new("fixture_sleep").unwrap(),
            duration: 150,
        }],
    )
    .unwrap();
    assert_eq!(game.rules_rng, expected);
    assert!(!game.condition_allows_voluntary_input(actor));
    assert!(!game.actors[&actor].search_clock.pending());
}

#[test]
fn earned_periodic_scan_precedes_nutrition_sleep_at_the_same_deadline() {
    let (mut game, actor) = fixture_with(vec![periodic()], |document| {
        document.nutrition = serde_json::from_value(serde_json::json!({
            "kind":"enabled","initial":0,"maximum":2000,"hungry_below":300,
            "weak_below":150,"starve_at":-852,"unit_ticks":100,
            "hungry_recovery":{"kind":"normal"},"weak_recovery":{"kind":"normal"},
            "empty_recovery":{"kind":"normal"},
            "regeneration_cost":{"denominator":1,"health":0,"stamina":0,"focus":0,"mana":0}
        }))
        .unwrap();
        document.nutrition_condition_trigger = serde_json::from_value(serde_json::json!({
            "kind":"on_empty_depletion","condition":"fixture_sleep",
            "chance":{"one_in":1},"duration":{"kind":"fixed","amount":150},"ticks_per_unit":1
        }))
        .unwrap();
    });
    game.configure_nutrition(actor, crate::nutrition::NutritionRequirement::Required)
        .unwrap();
    let mut expected = game.rules_rng;
    game.content
        .rules()
        .document()
        .nutrition_condition_trigger
        .sample(&mut expected);
    // One active scan owns the shared six-die distance-band sample.
    crate::dice::roll_check(&mut expected, crate::dice::Edge::from_counts(3, 0));
    game.advance_physics(100).unwrap();
    assert_eq!(game.rules_rng, expected);
    assert!(!game.condition_allows_voluntary_input(actor));
    assert!(!game.actors[&actor].search_clock.pending());
}

#[test]
fn earned_scan_uses_old_place_before_authorized_relocation() {
    let (mut game, actor) = fixture(vec![periodic()]);
    game.world
        .add_chamber(Region {
            id: RegionId(2),
            name: "other room".into(),
            bounds: Extent::new(6, 4, 2).unwrap(),
        })
        .unwrap();
    let destination = Location {
        region: RegionId(2),
        ..at(1)
    };
    game.place_trap(
        at(2),
        Trap {
            id: 11,
            dc: 0,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        },
    )
    .unwrap();
    game.place_trap(
        Location {
            region: RegionId(2),
            ..at(2)
        },
        Trap {
            id: 12,
            dc: 0,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        },
    )
    .unwrap();
    game.tick = 100;
    game.teleport(actor, destination).unwrap();
    assert!(game.feature_known(actor, tor_world::ConcealedFeatureId::Trap(11)));
    assert!(
        !game.feature_known(actor, tor_world::ConcealedFeatureId::Trap(12)),
        "relocation must not redirect an old-place scan; bootstrap passive is still disabled"
    );
}

#[test]
fn active_contact_and_ring_fraction_survive_checkpoint_without_an_extra_pulse() {
    let (mut game, actor) = fixture(vec![periodic()]);
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(50).unwrap();
    let armor = equip_armor(&mut game, actor, false);
    game.place_trap(
        at(2),
        Trap {
            id: 11,
            dc: 100,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        },
    )
    .unwrap();
    game.teleport(actor, at(2)).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 50);
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    let rng = restored.rules_rng;
    restored.observe(actor).unwrap();
    restored.reconcile_trap_contacts(actor, None).unwrap();
    assert_eq!(restored.rules_rng, rng);
    assert_eq!(restored.items[&armor].condition.corrosion, 1);
    restored.act(actor, crate::Action::Wait).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(restored, game);
    assert_ne!(restored.rules_rng, rng);
}

#[test]
fn checkpoint_rejects_period_inconsistent_with_active_search_source() {
    for period in [1, 200] {
        let (mut game, actor) = fixture(vec![periodic()]);
        game.actors.get_mut(&actor).unwrap().search_clock =
            serde_json::from_value(serde_json::json!({"period":period,"elapsed":0,"pending":null}))
                .unwrap();
        let mut shared = crate::checkpoint::SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
    }
    // An absent source intentionally retains the prior cadence's earned fraction.
    let (mut game, actor) = fixture(vec![]);
    game.actors.get_mut(&actor).unwrap().search_clock =
        serde_json::from_value(serde_json::json!({"period":200,"elapsed":50,"pending":null}))
            .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert_eq!(
        Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap(),
        game
    );
}

#[test]
fn frozen_ring_preserves_fraction_without_remote_time_catchup() {
    let (mut game, actor) = fixture(vec![periodic()]);
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(50).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 50);
    let rng = game.rules_rng;
    let mut records = crate::streaming::MemoryRecords::default();
    game.apply_region_transition(
        &crate::streaming::RegionTransition {
            active: Default::default(),
            loaded: [RegionId(1)].into(),
        },
        &mut records,
    )
    .unwrap();
    game.advance_physics(500).unwrap();
    assert_eq!(game.rules_rng, rng);
    game.apply_region_transition(
        &crate::streaming::RegionTransition {
            active: [RegionId(1)].into(),
            loaded: [RegionId(1)].into(),
        },
        &mut records,
    )
    .unwrap();
    assert_eq!(game.rules_rng, rng);
    game.advance_physics(549).unwrap();
    assert_eq!(game.rules_rng, rng);
    game.advance_physics(550).unwrap();
    assert_ne!(game.rules_rng, rng);
}

#[test]
fn ordinary_movement_triggers_rust_once_per_new_ground_entry() {
    let (mut game, actor) = fixture(vec![]);
    let armor = equip_armor(&mut game, actor, false);
    game.place_trap(
        at(2),
        Trap {
            id: 11,
            dc: 100,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        },
    )
    .unwrap();
    game.act(actor, crate::Action::Move(tor_world::Direction::East))
        .unwrap();
    assert_eq!(game.actors[&actor].location, at(2));
    assert_eq!(game.items[&armor].condition.corrosion, 1);
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(game.items[&armor].condition.corrosion, 1);
    game.act(actor, crate::Action::Move(tor_world::Direction::West))
        .unwrap();
    game.act(actor, crate::Action::Move(tor_world::Direction::East))
        .unwrap();
    assert_eq!(game.items[&armor].condition.corrosion, 2);
}

#[test]
fn subcell_lift_breaks_contact_and_real_landing_retriggers() {
    let (mut game, actor) = fixture(vec![]);
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    let armor = equip_armor(&mut game, actor, false);
    game.place_trap(
        at(2),
        Trap {
            id: 11,
            dc: 100,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        },
    )
    .unwrap();
    game.teleport(actor, at(2)).unwrap();
    assert_eq!(game.items[&armor].condition.corrosion, 1);
    // Keep this actor's next input outside the integration window; otherwise
    // the scheduler correctly returns at the first ready decision.
    game.actors.get_mut(&actor).unwrap().ready_at = 100;
    game.actors.get_mut(&actor).unwrap().motion.velocity[2] = 64;
    game.advance_physics(1).unwrap();
    assert_eq!(game.actors[&actor].location, at(2));
    assert!(game.actors[&actor].trap_contacts.ids().next().is_none());
    // Seed an imposed return before the existing cell-based resting rule can
    // stop the tiny lift at its apex; this tests contact, not new flight physics.
    game.actors.get_mut(&actor).unwrap().motion.velocity[2] = -128;
    game.advance_physics(100).unwrap();
    assert_eq!(game.actors[&actor].location, at(2));
    assert_eq!(game.items[&armor].condition.corrosion, 2);
}

#[test]
fn standalone_resolved_sleep_settles_earned_search_before_publication() {
    let (mut game, actor) = fixture(vec![periodic()]);
    let snapshot = crate::abilities::Snapshot::capture(&game.creature(actor).unwrap());
    let effects = [crate::abilities::PlannedEffect::ApplyCondition {
        id: crate::content_ids::ConditionId::new("fixture_sleep").unwrap(),
        duration: 150,
    }];
    let mut rng = game.rules_rng;
    let outcome = crate::abilities::resolve_checked(
        crate::abilities::ResolutionPlan::Effects {
            actor,
            target: actor,
            effects: &effects,
        },
        &snapshot,
        &mut rng,
        &mut (),
    )
    .unwrap();
    game.tick = 100;
    let mut expected = game.rules_rng;
    // One active scan owns the shared six-die distance-band sample.
    crate::dice::roll_check(&mut expected, crate::dice::Edge::from_counts(3, 0));
    game.apply(outcome).unwrap();
    assert_eq!(game.rules_rng, expected);
    assert!(!game.condition_allows_voluntary_input(actor));
}

#[test]
fn relocation_from_frozen_region_establishes_new_active_ground_contact() {
    let (mut game, actor) = fixture(vec![]);
    game.world
        .add_chamber(Region {
            id: RegionId(2),
            name: "destination".into(),
            bounds: Extent::new(6, 4, 2).unwrap(),
        })
        .unwrap();
    let destination = Location {
        region: RegionId(2),
        ..at(2)
    };
    let armor = equip_armor(&mut game, actor, false);
    game.place_trap(
        destination,
        Trap {
            id: 11,
            dc: 100,
            avoidance_dc: None,
            kind: TrapKind::Rust {
                slot: "body_armor".into(),
                amount: 1,
            },
        },
    )
    .unwrap();
    let mut records = crate::streaming::MemoryRecords::default();
    game.apply_region_transition(
        &crate::streaming::RegionTransition {
            active: [RegionId(2)].into(),
            loaded: [RegionId(1), RegionId(2)].into(),
        },
        &mut records,
    )
    .unwrap();
    assert!(game.actor_frozen(actor));
    game.teleport(actor, destination).unwrap();
    assert!(!game.actor_frozen(actor));
    assert_eq!(game.items[&armor].condition.corrosion, 1);
}

#[test]
fn real_worn_ring_preserves_fraction_across_unequip_and_re_equip() {
    let (mut game, actor) = fixture(vec![]);
    let mut spec = ItemSpec::ordinary("searching ring fixture".into());
    spec.class = crate::ItemClass::Ring;
    spec.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        slot: crate::EquipmentSlot::new("ring").unwrap(),
        armor: None,
        attack: None,
        weapon_profile: None,
        modifiers: vec![periodic()],
        potency_modifiers: vec![],
    });
    let ring = game
        .place_item_stack(71, at(1), Some(actor), 1, spec)
        .unwrap();
    let slot = EquipmentSlotId(
        game.anatomy(actor)
            .unwrap()
            .slots
            .iter()
            .position(|slot| slot.as_str() == "ring")
            .unwrap() as u16,
    );
    let original = game.rules_rng;
    game.equip_authored(actor, ring, slot).unwrap();
    assert_eq!(game.rules_rng, original, "equipping cannot pulse");
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(25).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 25);
    game.act(actor, crate::Action::Unequip { item: ring })
        .unwrap();
    assert!(!game
        .equipment(actor)
        .unwrap()
        .values()
        .any(|item| *item == ring));
    let fraction = serde_json::to_value(&game.actors[&actor].search_clock).unwrap()["elapsed"]
        .as_u64()
        .unwrap();
    assert!(fraction > 0 && fraction < 100);
    let rng = game.rules_rng;
    for _ in 0..12 {
        game.act(actor, crate::Action::Wait).unwrap();
    }
    assert_eq!(game.rules_rng, rng, "unworn time cannot accrue scans");
    let worn_at = game.tick();
    game.equip_authored(actor, ring, slot).unwrap();
    assert_eq!(game.rules_rng, rng, "re-equipping cannot pulse");
    game.advance_physics(worn_at + 100 - fraction - 1).unwrap();
    assert_eq!(game.rules_rng, rng);
    game.advance_physics(worn_at + 100 - fraction).unwrap();
    let mut expected = rng;
    // One active scan owns the shared six-die distance-band sample.
    crate::dice::roll_check(&mut expected, crate::dice::Edge::from_counts(3, 0));
    assert_eq!(game.rules_rng, expected);
}

#[test]
fn blindness_suppresses_ring_time_without_a_cure_pulse() {
    let (mut game, actor) = fixture_with(vec![periodic()], |document| {
        document.conditions.push(serde_json::from_value(serde_json::json!({
            "id":"fixture_blind", "family":"fixture_blind", "name":"Blind", "lifecycle":"timed",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
            "restrictions":[], "preparation":"preserve",
            "modifiers":[modifier(ModifierOp::Perception { value:crate::perception::PerceptionValue::Sight { enabled:false } })],
            "stacking":{"kind":"longest"}, "source_scoped":false, "descriptors":[], "capacity":"reject"
        })).unwrap());
    });
    game.advance_physics(50).unwrap();
    let rng = game.rules_rng;
    game.apply_effects(
        actor,
        &[crate::rules::Effect::ApplyCondition {
            condition: crate::content_ids::ConditionId::new("fixture_blind").unwrap(),
            duration: 150,
        }],
    )
    .unwrap();
    game.advance_physics(200).unwrap();
    assert!(game.actors[&actor].perception().sight);
    assert_eq!(game.rules_rng, rng);
    game.advance_physics(249).unwrap();
    assert_eq!(game.rules_rng, rng);
    game.advance_physics(250).unwrap();
    assert_ne!(game.rules_rng, rng);
}

#[test]
fn detached_ring_retains_fraction_through_saved_game_restore() {
    use crate::streaming::RecordStore;
    let (mut game, actor) = fixture(vec![periodic()]);
    game.actors.get_mut(&actor).unwrap().turn_ticks = NonZeroU64::new(50).unwrap();
    game.act(actor, crate::Action::Wait).unwrap();
    assert_eq!(game.tick(), 50);
    let rng = game.rules_rng;
    let mut records = crate::streaming::MemoryRecords::default();
    game.apply_region_transition(&crate::streaming::RegionTransition::default(), &mut records)
        .unwrap();
    assert!(!game.actors.contains_key(&actor));
    let mut reopened_records = crate::streaming::MemoryRecords::default();
    let record_ids: Vec<_> = records.ids().collect();
    for id in record_ids {
        let data = crate::streaming::RegionRecordData::capture(&records.get(id).unwrap());
        let bytes = serde_json::to_vec(&data).unwrap();
        let decoded: crate::streaming::RegionRecordData = serde_json::from_slice(&bytes).unwrap();
        reopened_records.put(
            id,
            tor_world::Shared::new(decoded.restore(game.content().clone()).unwrap()),
        );
    }
    let mut records = reopened_records;
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    restored.advance_physics(500).unwrap();
    restored
        .apply_region_transition(
            &crate::streaming::RegionTransition {
                active: [RegionId(1)].into(),
                loaded: [RegionId(1)].into(),
            },
            &mut records,
        )
        .unwrap();
    assert_eq!(restored.rules_rng, rng);
    restored.advance_physics(549).unwrap();
    assert_eq!(restored.rules_rng, rng);
    restored.advance_physics(550).unwrap();
    assert_ne!(restored.rules_rng, rng);
}
