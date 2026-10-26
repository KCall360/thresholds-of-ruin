//! Remaining hazard payloads share ordinary checks, condition clocks and gravity.
use super::*;
use crate::rules::{Modifier, ModifierOp, Scope, Stat};
use std::{collections::BTreeMap, num::NonZeroU64};
use tor_world::{Extent, Position, Region, Trap, World};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture(modifiers: Vec<Modifier>) -> (Game, ActorId) {
    fixture_with_resistance(modifiers, 100)
}
fn fixture_with_resistance(modifiers: Vec<Modifier>, difficulty: i32) -> (Game, ActorId) {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.descriptors.push(crate::rules::NamedDescriptor {
        id: crate::content_ids::DescriptorId::new("poison").unwrap(),
        name: "Poison".into(),
        resistance: Some(crate::rules::DescriptorResistance {
            skill: crate::attributes::Skill::Athletics,
            difficulty,
        }),
    });
    for value in [
        serde_json::json!({
            "id":"fixture_sleep","family":"fixture_sleep","name":"Sleeping","lifecycle":"timed",
            "modifiers":[],"stacking":{"kind":"extend"},"source_scoped":false,"descriptors":[],"capacity":"reject",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},"restrictions":["voluntary_actions"],"preparation":"pause"
        }),
        serde_json::json!({
            "id":"fixture_bear","family":"fixture_bear","name":"Caught","lifecycle":"timed",
            "modifiers":[],"stacking":{"kind":"extend"},"source_scoped":false,"descriptors":[],"capacity":"reject",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},"restrictions":["ordinary_movement","movement_abilities"],"preparation":"preserve"
        }),
        serde_json::json!({
            "id":"fixture_hover","family":"fixture_hover","name":"Hovering","lifecycle":"timed",
            "modifiers":[{"scope":"always","op":{"kind":"locomotion","value":{"kind":"levitating","enabled":true}}}],
            "stacking":{"kind":"extend"},"source_scoped":false,"descriptors":[],"capacity":"reject",
            "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},"restrictions":[],"preparation":"preserve"
        }),
    ] {
        document
            .conditions
            .push(serde_json::from_value(value).unwrap());
    }
    let mut opponent = document
        .conditions
        .iter()
        .find(|entry| entry.id.as_str() == "distracted")
        .unwrap()
        .clone();
    opponent.id = condition("fixture_opponent");
    opponent.family = crate::content_ids::ConditionFamilyId::new("fixture_opponent").unwrap();
    opponent.source_scoped = true;
    opponent.lifecycle = crate::rules::ConditionLifecycle::Timed;
    opponent.modifiers = vec![Modifier {
        cancellable: false,
        scope: Scope::VersusSource,
        op: ModifierOp::Add {
            stat: Stat::IncomingEdge { defense: None },
            value: 3,
        },
    }];
    document.conditions.push(opponent);
    let mut species = crate::test_creatures::species();
    species.default_attributes =
        crate::attributes::Attributes::new(crate::attributes::ByAttribute {
            strength: 2,
            ..Default::default()
        })
        .unwrap();
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
    let mut arrow = crate::ItemSpec::ordinary("ordinary arrow".into());
    arrow.stackable = true;
    arrow.class = crate::ItemClass::Weapon;
    let component = crate::damage::DamageComponent::fixed(crate::combat::DamageType::Keen, None, 3);
    let primary = component.key();
    arrow.projectile = Some(crate::projectile_profiles::ProjectileSpec {
        family: crate::content_ids::LauncherFamilyId::new("arrow").unwrap(),
        damage: crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
        recovery: crate::projectile_profiles::HitRecovery::ConsumedOnHit,
    });
    let content = crate::creatures::Content::with_trap_ammunition(
        &content,
        BTreeMap::from([("fixture.arrow".into(), arrow)]),
    )
    .unwrap();
    let mut world = World::new(vec![], vec![]).unwrap();
    world
        .add_chamber(Region {
            id: RegionId(1),
            name: "hazards".into(),
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
            name: "subject".into(),
            faction: "test".into(),
        },
        build,
    )
    .unwrap();
    (game, actor)
}
fn place(game: &mut Game, kind: TrapKind) {
    game.place_trap(
        at(2),
        Trap {
            id: 1,
            dc: 99,
            avoidance_dc: None,
            kind,
        },
    )
    .unwrap();
}
fn condition(id: &str) -> crate::content_ids::ConditionId {
    crate::content_ids::ConditionId::new(id).unwrap()
}

/// These fixtures use 100-tick Wait actions. Each ordinary action already drives
/// physics to the next decision; check the horizon before every new paid action.
fn advance_with_paid_waits(game: &mut Game, until: u64) {
    let action_budget = game.actors.len() * 3;
    for _ in 0..action_budget {
        if game.tick >= until {
            break;
        }
        let id = game.next_actor().expect("fixture has a live waiting actor");
        assert!(game.actors[&id].ready_at <= game.tick);
        let previous_ready = game.actors[&id].ready_at;
        let previous_tick = game.tick;
        game.act(id, crate::Action::Wait).unwrap();
        assert!(
            game.actors[&id].ready_at > previous_ready,
            "Wait pays action time"
        );
        assert!(
            game.tick >= previous_tick && game.tick <= until,
            "bounded scheduler progress"
        );
    }
    assert_eq!(
        game.tick, until,
        "bounded paid waits reach the exact physics horizon"
    );
}

fn plain_body_fixture() -> (Game, ActorId) {
    let (content_owner, _) = fixture(vec![]);
    let mut game = Game::new(
        (*content_owner.world).clone(),
        43,
        content_owner.content().clone(),
    );
    // A fresh ordinary actor, not a creature with its combat state stripped.
    let actor = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert!(game.creature(actor).is_none());
    (game, actor)
}

fn nonspatial_trap_kinds() -> [TrapKind; 6] {
    [
        TrapKind::Rust {
            slot: "body_armor".into(),
            amount: 1,
        },
        TrapKind::SleepingGas {
            condition: "fixture_sleep".into(),
            duration: 200,
            exposure: Some("poison".into()),
        },
        TrapKind::BearTrap {
            condition: "fixture_bear".into(),
            duration: 300,
        },
        TrapKind::Arrow {
            attack_bonus: 100,
            ammunition: "fixture.arrow".into(),
        },
        TrapKind::PoisonDart {
            attack_bonus: 100,
            exposure: "poison".into(),
        },
        TrapKind::Mysterious {},
    ]
}

fn restored_contact_game(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let checkpoint = serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    let restored = Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    assert_eq!(restored, *game);
    restored
}

#[test]
fn plain_body_paid_trap_contacts_preserve_disclosure_and_reentry_without_creature_payloads() {
    for kind in nonspatial_trap_kinds() {
        let (mut game, actor) = plain_body_fixture();
        let rng = game.rules_rng;
        // Establish valid ordinary movement before introducing this trap.
        game.act(actor, crate::Action::Move(Direction::East))
            .unwrap();
        assert_eq!((game.actors[&actor].location, game.tick), (at(2), 100));
        game.act(actor, crate::Action::Move(Direction::West))
            .unwrap();
        assert_eq!((game.actors[&actor].location, game.tick), (at(1), 200));
        let arrow = matches!(kind, TrapKind::Arrow { .. });
        game.place_trap(
            at(2),
            Trap {
                id: 1,
                dc: 99,
                avoidance_dc: Some(0),
                kind: kind.clone(),
            },
        )
        .unwrap();
        assert!(!game.feature_known(actor, ConcealedFeatureId::Trap(1)));
        game.act(actor, crate::Action::Move(Direction::East))
            .unwrap_or_else(|error| panic!("valid plain-body contact with {kind:?}: {error:?}"));
        assert_eq!((game.actors[&actor].location, game.tick), (at(2), 300));
        assert!(game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
        assert_eq!(
            game.actors[&actor].trap_contacts.ids().collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(
            game.rules_rng, rng,
            "no invented creature check or payload roll"
        );
        assert!(game.creature(actor).is_none() && game.health(actor).is_none());
        assert!(game.actors[&actor].spatial_condition_anchors.0.is_empty());
        assert!(game.actors[&actor].equipment.is_empty());
        assert_eq!(game.items.len(), usize::from(arrow));
        if arrow {
            let item = game.items.iter().next().unwrap().1;
            assert_eq!(item.location, crate::ItemLocation::Ground(at(2)));
            assert_eq!(item.quantity, 1);
            assert_eq!(item.spec.name, "ordinary arrow");
            assert!(item.spec.projectile.is_some() && !item.spec.magical);
        }
        game.act(actor, crate::Action::Wait).unwrap();
        game = restored_contact_game(&game);
        let stationary = game.clone();
        for _ in 0..2 {
            game.scene(actor).unwrap();
            game.reconcile_trap_contacts(actor, None).unwrap();
        }
        assert_eq!(
            game, stationary,
            "standing, queries and restore cannot retrigger"
        );
        game.act(actor, crate::Action::Move(Direction::West))
            .unwrap();
        assert!(game.actors[&actor].trap_contacts.ids().next().is_none());
        // Already-owned knowledge licenses no fabricated Agility for a plain body.
        game.act(actor, crate::Action::Move(Direction::East))
            .unwrap();
        assert_eq!((game.actors[&actor].location, game.tick), (at(2), 600));
        assert_eq!(game.rules_rng, rng);
        assert_eq!(
            game.items.values().map(|item| item.quantity).sum::<u64>(),
            if arrow { 2 } else { 0 }
        );
        assert!(game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
        assert_eq!(
            game.actors[&actor].trap_contacts.ids().collect::<Vec<_>>(),
            vec![1]
        );
    }
}

#[test]
fn plain_body_known_teleport_trap_still_relocates_and_restores_without_repeat() {
    let (mut game, actor) = plain_body_fixture();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.place_trap(
        at(2),
        Trap {
            id: 1,
            dc: 99,
            avoidance_dc: Some(0),
            kind: TrapKind::Teleport {},
        },
    )
    .unwrap();
    game.reveal_feature_family(actor, ConcealedFeatureId::Trap(1))
        .unwrap();
    let rng = game.rules_rng;
    game.act(actor, crate::Action::Move(Direction::East))
        .unwrap();
    let destination = game.actors[&actor].location;
    assert_ne!(
        destination,
        at(2),
        "plain bodies retain the physical relocation payload"
    );
    assert_eq!(destination.region, RegionId(1));
    assert_eq!(
        destination.position.z, 0,
        "relocation chooses supported floor"
    );
    assert_eq!(game.tick, 100);
    assert_ne!(
        game.rules_rng, rng,
        "real relocation still spends its destination draw"
    );
    assert!(game.creature(actor).is_none());
    assert!(game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
    assert!(game.actors[&actor].trap_contacts.ids().next().is_none());
    game = restored_contact_game(&game);
    let restored = game.clone();
    game.scene(actor).unwrap();
    game.reconcile_trap_contacts(actor, None).unwrap();
    assert_eq!(game, restored);
}

#[test]
fn plain_body_known_trapdoor_still_opens_and_falls_with_loose_stock() {
    let (mut game, actor) = plain_body_fixture();
    let target = Location {
        region: RegionId(2),
        position: Position { x: 3, y: 1, z: 1 },
    };
    game.add_chamber(Region {
        id: target.region,
        name: "below".into(),
        bounds: Extent::new(6, 4, 2).unwrap(),
    })
    .unwrap();
    game.register_named_anchors(
        target.region,
        BTreeMap::from([("landing".into(), target.position)]),
    )
    .unwrap();
    for region in [RegionId(1), target.region] {
        game.set_gravity(region, [0, 0, -1]).unwrap();
    }
    game.place_trap(
        at(2),
        Trap {
            id: 1,
            dc: 99,
            avoidance_dc: Some(0),
            kind: TrapKind::TrapDoor {
                destination: tor_world::NamedAnchor {
                    region: target.region,
                    name: "landing".into(),
                },
                open: false,
            },
        },
    )
    .unwrap();
    game.reveal_feature_family(actor, ConcealedFeatureId::Trap(1))
        .unwrap();
    let item = game.place_item(at(2), "stone".into()).unwrap();
    let rng = game.rules_rng;
    game.act(actor, crate::Action::Move(Direction::East))
        .unwrap();
    assert_eq!(
        game.world.physics_neighbor(at(2), Direction::Down),
        Some((target, 0))
    );
    assert!(!game.world.is_stair(at(2), Direction::Down));
    assert!(matches!(
        game.world.trap(at(2)).unwrap().kind,
        TrapKind::TrapDoor { open: true, .. }
    ));
    advance_with_paid_waits(&mut game, 200);
    assert_eq!(game.actors[&actor].location.region, target.region);
    assert!(matches!(game.items[&item].location,
        crate::ItemLocation::Ground(location) if location.region == target.region));
    assert_eq!(
        game.rules_rng, rng,
        "no fabricated avoidance or injury roll"
    );
    assert!(game.creature(actor).is_none() && game.health(actor).is_none());
    assert!(game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
    let _ = restored_contact_game(&game);
}

#[test]
fn trap_payload_eligibility_never_treats_a_missing_actor_as_a_plain_body() {
    let (mut game, _) = plain_body_fixture();
    let physical = [
        TrapKind::Teleport {},
        TrapKind::TrapDoor {
            destination: tor_world::NamedAnchor {
                region: RegionId(2),
                name: "landing".into(),
            },
            open: false,
        },
    ];
    for kind in nonspatial_trap_kinds().into_iter().chain(physical) {
        let before = game.clone();
        assert_eq!(
            game.apply_trap_contact(ActorId(999), at(2), kind),
            Err(GameError::UnknownActor)
        );
        assert_eq!(game, before);
    }
}

#[test]
fn plain_body_contact_rejects_invalid_raw_world_payload_without_partial_movement() {
    let invalid = [
        TrapKind::Rust {
            slot: "missing_slot".into(),
            amount: 1,
        },
        TrapKind::SleepingGas {
            condition: "missing_condition".into(),
            duration: 200,
            exposure: Some("poison".into()),
        },
        TrapKind::SleepingGas {
            condition: "fixture_sleep".into(),
            duration: 200,
            exposure: Some("missing_exposure".into()),
        },
        TrapKind::BearTrap {
            condition: "missing_condition".into(),
            duration: 300,
        },
        TrapKind::PoisonDart {
            attack_bonus: 0,
            exposure: "missing_exposure".into(),
        },
        TrapKind::Arrow {
            attack_bonus: 0,
            ammunition: "missing_ammunition".into(),
        },
    ];
    for kind in invalid {
        let (template, _) = plain_body_fixture();
        let mut world = (*template.world).clone();
        // Bypass Game::place_trap deliberately: raw World has no content
        // catalog and Game::new is infallible. Contact still validates it.
        world
            .place_trap(
                at(2),
                Trap {
                    id: 1,
                    dc: 99,
                    avoidance_dc: None,
                    kind: kind.clone(),
                },
            )
            .unwrap();
        let mut game = Game::new(world, 43, template.content().clone());
        let actor = game
            .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.act(actor, crate::Action::Move(Direction::West))
            .unwrap();
        assert_eq!((game.actors[&actor].location, game.tick), (at(0), 100));
        game.act(actor, crate::Action::Move(Direction::East))
            .unwrap();
        assert_eq!((game.actors[&actor].location, game.tick), (at(1), 200));
        let before = game.clone();
        assert_eq!(
            game.apply_trap_contact(ActorId(999), at(2), kind.clone()),
            Err(GameError::UnknownActor),
            "missing identity precedes content validation"
        );
        assert_eq!(game, before);
        assert_eq!(
            game.act(actor, crate::Action::Move(Direction::East)),
            Err(GameError::InvalidLocation),
            "malformed contact payload {kind:?}"
        );
        assert_eq!(
            game, before,
            "rejection cannot publish movement, time, RNG, contact or stock"
        );
    }
}

#[test]
fn reusable_arrow_misses_leave_real_ordinary_ammunition_and_do_not_repeat_in_place() {
    let (mut game, actor) = fixture(vec![]);
    place(
        &mut game,
        TrapKind::Arrow {
            attack_bonus: -100,
            ammunition: "fixture.arrow".into(),
        },
    );
    game.teleport(actor, at(2)).unwrap();
    let arrows = |game: &Game| {
        game.items
            .iter()
            .filter(|(_, item)| item.spec.name == "ordinary arrow")
            .map(|(_, item)| item.quantity)
            .sum::<u64>()
    };
    assert_eq!(arrows(&game), 1);
    let rng = game.rules_rng;
    game.reconcile_trap_contacts(actor, None).unwrap();
    assert_eq!(game.rules_rng, rng);
    assert_eq!(arrows(&game), 1);
    game.teleport(actor, at(1)).unwrap();
    game.teleport(actor, at(2)).unwrap();
    assert_eq!(arrows(&game), 2);
    assert!(game
        .items
        .iter()
        .all(|(_, item)| item.spec.projectile.is_some() && !item.spec.magical));
}
#[test]
fn arrow_hit_absorbed_by_live_armor_is_not_a_miss_or_an_arrow_drop() {
    let (mut game, actor) = fixture(vec![]);
    let mut armor = crate::ItemSpec::ordinary("armor".into());
    armor.class = crate::ItemClass::Armor;
    armor.equipment = Some(crate::EquipmentSpec {
        periodic_relocation: None,
        slot: crate::EquipmentSlot::new("body_armor").unwrap(),
        armor: Some(crate::ArmorSpec {
            physical_reduction: 100,
            material: crate::ArmorMaterial::Metal,
            rustproof: false,
        }),
        attack: None,
        weapon_profile: None,
        modifiers: vec![],
        potency_modifiers: vec![],
    });
    let item = game
        .place_item_stack(41, at(1), Some(actor), 1, armor)
        .unwrap();
    let socket = game
        .anatomy(actor)
        .unwrap()
        .slots
        .iter()
        .position(|slot| slot.as_str() == "body_armor")
        .unwrap();
    game.equip_authored(actor, item, crate::EquipmentSlotId(socket as u16))
        .unwrap();
    place(
        &mut game,
        TrapKind::Arrow {
            attack_bonus: 100,
            ammunition: "fixture.arrow".into(),
        },
    );
    let health = game.health(actor).unwrap();
    game.teleport(actor, at(2)).unwrap();
    assert_eq!(game.health(actor).unwrap(), health);
    assert_eq!(game.items.len(), 1);
}
#[test]
fn trap_damage_never_borrows_recipient_melee_damage_bonus() {
    let (mut ordinary, a) = fixture(vec![]);
    let (mut powerful, b) = fixture(vec![Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::AttackDamageRollBonus,
            value: 100,
        },
    }]);
    for game in [&mut ordinary, &mut powerful] {
        place(
            game,
            TrapKind::Arrow {
                attack_bonus: 100,
                ammunition: "fixture.arrow".into(),
            },
        );
    }
    ordinary.teleport(a, at(2)).unwrap();
    powerful.teleport(b, at(2)).unwrap();
    assert_eq!(ordinary.health(a), powerful.health(b));
    assert_eq!(ordinary.rules_rng, powerful.rules_rng);
}
#[test]
fn bear_anchor_survives_hover_but_real_separation_frees_without_a_second_timer() {
    let (mut game, actor) = fixture(vec![]);
    place(
        &mut game,
        TrapKind::BearTrap {
            condition: "fixture_bear".into(),
            duration: 300,
        },
    );
    game.teleport(actor, at(2)).unwrap();
    assert!(game.condition_allows_voluntary_input(actor));
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .conditions()
            .remaining(&condition("fixture_bear"), actor),
        Some(300)
    );
    game.apply_effects(
        actor,
        &[crate::rules::Effect::ApplyCondition {
            condition: condition("fixture_hover"),
            duration: 400,
        }],
    )
    .unwrap();
    game.reconcile_trap_contacts(actor, None).unwrap();
    assert!(game.actors[&actor].trap_contacts.ids().next().is_none());
    assert_eq!(game.actors[&actor].spatial_condition_anchors.0.len(), 1);
    let mut shared = crate::checkpoint::SharedState::default();
    let restored = Game::restore_checkpoint(
        game.checkpoint(&mut shared),
        &shared,
        game.content().clone(),
    )
    .unwrap();
    assert_eq!(restored, game);
    game.teleport(actor, at(3)).unwrap();
    assert!(!game
        .creature(actor)
        .unwrap()
        .conditions()
        .has_active(&condition("fixture_bear")));
    assert!(game.actors[&actor].spatial_condition_anchors.0.is_empty());
}
#[test]
fn bear_expiry_cleans_only_its_anchor_at_exact_active_tick() {
    let (mut game, actor) = fixture(vec![]);
    place(
        &mut game,
        TrapKind::BearTrap {
            condition: "fixture_bear".into(),
            duration: 300,
        },
    );
    game.teleport(actor, at(2)).unwrap();
    game.advance_physics(299).unwrap();
    assert!(game
        .creature(actor)
        .unwrap()
        .conditions()
        .has_active(&condition("fixture_bear")));
    game.advance_physics(300).unwrap();
    assert!(!game
        .creature(actor)
        .unwrap()
        .conditions()
        .has_active(&condition("fixture_bear")));
    assert!(game.actors[&actor].spatial_condition_anchors.0.is_empty());
}
#[test]
fn trapdoor_opens_before_any_actor_or_loose_item_falls_and_never_teleports_them() {
    let (mut game, actor) = fixture(vec![]);
    let target = Location {
        region: RegionId(2),
        position: Position { x: 3, y: 1, z: 1 },
    };
    game.add_chamber(Region {
        id: target.region,
        name: "below".into(),
        bounds: Extent::new(6, 4, 2).unwrap(),
    })
    .unwrap();
    game.register_named_anchors(
        target.region,
        BTreeMap::from([("landing".into(), target.position)]),
    )
    .unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.set_gravity(RegionId(2), [0, 0, -1]).unwrap();
    place(
        &mut game,
        TrapKind::TrapDoor {
            destination: tor_world::NamedAnchor {
                region: target.region,
                name: "landing".into(),
            },
            open: false,
        },
    );
    game.set_body(
        actor,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    let item = game.place_item(at(2), "stone".into()).unwrap();
    game.teleport(actor, at(2)).unwrap();
    assert_eq!(
        game.actors[&actor].location,
        at(2),
        "opening is topology, not relocation"
    );
    assert_eq!(
        game.items[&item].location,
        crate::ItemLocation::Ground(at(2))
    );
    assert_eq!(
        game.world.physics_neighbor(at(2), Direction::Down),
        Some((target, 0))
    );
    assert!(!game.world.is_stair(at(2), Direction::Down));
    assert!(
        game.translate_body(
            crate::physics::PhysicsEntity::Actor(actor),
            at(2),
            0,
            &game.actors[&actor].body,
            [0, 0, -1],
        )
        .is_some(),
        "the complete two-cell body fits the physical aperture"
    );
    // Setup teleport leaves the actor ready at tick zero; ordinary paid Wait
    // actions drive every input boundary while gravity advances.
    advance_with_paid_waits(&mut game, 200);
    assert_eq!(game.actors[&actor].location.region, target.region);
    assert!(
        matches!(game.items[&item].location, crate::ItemLocation::Ground(at) if at.region == target.region)
    );
}

#[test]
fn poison_dart_hit_and_athletics_resistance_are_separate_and_loss_is_restorable_with_sustain() {
    use crate::attributes::Attribute;
    for sustained in [false, true] {
        let modifiers = sustained
            .then_some(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::SustainAttribute {
                    attribute: Attribute::Strength,
                },
            })
            .into_iter()
            .collect();
        let (mut game, actor) = fixture(modifiers);
        let strength = game
            .creature(actor)
            .unwrap()
            .derived()
            .attributes
            .get(Attribute::Strength);
        let health = game.health(actor).unwrap().0;
        let mut expected = game.rules_rng;
        crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
        let damage = crate::dice::DicePool::new(1, 4, 0)
            .unwrap()
            .roll(&mut expected, crate::dice::Edge::default());
        crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
        // Call the contact payload directly to isolate its three rolls from
        // separate passive-discovery receipts produced by actor movement.
        game.apply_trap_contact(
            actor,
            at(2),
            TrapKind::PoisonDart {
                attack_bonus: 100,
                exposure: "poison".into(),
            },
        )
        .unwrap();
        assert_eq!(game.rules_rng, expected);
        assert!(game.health(actor).unwrap().0 < health);
        let _ = damage; // The damage pool's draw count is the contract here.
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .derived()
                .attributes
                .get(Attribute::Strength),
            strength - u16::from(!sustained)
        );
        game.apply_effects(
            actor,
            &[crate::rules::Effect::RestoreAttribute {
                attribute: Attribute::Strength,
                amount: crate::rules::AmountExpr::Fixed { amount: 1 },
            }],
        )
        .unwrap();
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .derived()
                .attributes
                .get(Attribute::Strength),
            strength
        );
    }
}

#[test]
fn environmental_check_uses_ordinary_incoming_edge_but_has_no_fabricated_opponent() {
    let (mut game, actor) = fixture(vec![Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::IncomingEdge {
                defense: Some(crate::rules::DefenseKind::Physical),
            },
            value: 1,
        },
    }]);
    game.apply_condition(actor, &condition("fixture_opponent"), actor, 100)
        .unwrap();
    let mut expected = game.rules_rng;
    crate::dice::roll_check(&mut expected, crate::dice::Edge::from_counts(1, 0));
    assert!(!game
        .resolve_environmental_attack_in_transaction(actor, -100, &[], &[])
        .unwrap());
    assert_eq!(
        game.rules_rng, expected,
        "one ordinary incoming advantage, no three VersusSource advantages"
    );
}

#[test]
fn trapdoor_contact_waits_for_exact_residency_without_spending_contact_or_opening_lid() {
    let (mut game, actor) = fixture(vec![]);
    let target = Location {
        region: RegionId(2),
        position: Position { x: 3, y: 1, z: 1 },
    };
    game.add_chamber(Region {
        id: target.region,
        name: "below".into(),
        bounds: Extent::new(6, 4, 2).unwrap(),
    })
    .unwrap();
    game.register_named_anchors(
        target.region,
        BTreeMap::from([("landing".into(), target.position)]),
    )
    .unwrap();
    place(
        &mut game,
        TrapKind::TrapDoor {
            destination: tor_world::NamedAnchor {
                region: target.region,
                name: "landing".into(),
            },
            open: false,
        },
    );
    let record = game.world.detach_region(target.region).unwrap();
    // Stage just the committed physical placement; this isolates the contact
    // preflight from unrelated movement/discovery work in the outer transaction.
    game.actors.get_mut(&actor).unwrap().location = at(2);
    let rng = game.rules_rng;
    game.host_region_continuation = true;
    game.reconcile_trap_contacts(actor, None).unwrap();
    game.host_region_continuation = false;
    assert!(game.region_transition_pending());
    assert_eq!(
        game.trap_missing_regions(game.trap_region_boundary().unwrap()),
        BTreeSet::from([target.region])
    );
    assert_eq!(game.rules_rng, rng);
    assert!(game.actors[&actor].trap_contacts.ids().next().is_none());
    assert!(!game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
    assert!(matches!(
        game.world.trap(at(2)).unwrap().kind,
        TrapKind::TrapDoor { open: false, .. }
    ));
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            game.checkpoint(&mut crate::checkpoint::SharedState::default());
        }))
        .is_err(),
        "unfinished contact cannot become a save checkpoint"
    );
    game.world.attach_region(record).unwrap();
    game.complete_region_transition(&crate::TransitionReport::default())
        .unwrap();
    assert!(!game.region_transition_pending());
    assert!(game.trap_resolution.is_none());
    assert!(matches!(
        game.world.trap(at(2)).unwrap().kind,
        TrapKind::TrapDoor { open: true, .. }
    ));
    assert!(game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
}

#[test]
fn occupied_trapdoor_landing_opens_but_uses_collision_not_an_alternate_destination() {
    let (mut game, actor) = fixture(vec![]);
    let target = Location {
        region: RegionId(2),
        position: Position { x: 3, y: 1, z: 1 },
    };
    game.add_chamber(Region {
        id: target.region,
        name: "below".into(),
        bounds: Extent::new(6, 4, 2).unwrap(),
    })
    .unwrap();
    game.register_named_anchors(
        target.region,
        BTreeMap::from([("landing".into(), target.position)]),
    )
    .unwrap();
    let blocker = game
        .spawn_actor(
            Location {
                position: Position {
                    z: 0,
                    ..target.position
                },
                ..target
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    game.set_body(
        blocker,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 0, 1]],
            eye: [0, 0, 1],
            mass: 80,
        },
    )
    .unwrap();
    game.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    game.set_gravity(RegionId(2), [0, 0, -1]).unwrap();
    place(
        &mut game,
        TrapKind::TrapDoor {
            destination: tor_world::NamedAnchor {
                region: target.region,
                name: "landing".into(),
            },
            open: false,
        },
    );
    game.teleport(actor, at(2)).unwrap();
    assert!(
        game.translate_body(
            crate::physics::PhysicsEntity::Actor(actor),
            at(2),
            0,
            &game.actors[&actor].body,
            [0, 0, -1],
        )
        .is_none(),
        "the actual occupied endpoint blocks whole-body passage"
    );
    advance_with_paid_waits(&mut game, 200);
    assert_eq!(game.actors[&actor].location, at(2));
    assert_eq!(game.actors[&blocker].location.position.z, 0);
    assert!(matches!(
        game.world.trap(at(2)).unwrap().kind,
        TrapKind::TrapDoor { open: true, .. }
    ));
}

#[test]
fn restored_spatial_restraint_rejects_forged_noncontact_anchor() {
    let (mut game, actor) = fixture(vec![]);
    place(
        &mut game,
        TrapKind::BearTrap {
            condition: "fixture_bear".into(),
            duration: 300,
        },
    );
    game.teleport(actor, at(2)).unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    assert_eq!(
        Game::restore_checkpoint(
            game.checkpoint(&mut shared),
            &shared,
            game.content().clone()
        )
        .unwrap(),
        game
    );
    game.actors
        .get_mut(&actor)
        .unwrap()
        .spatial_condition_anchors
        .0[0]
        .location = at(4);
    let mut shared = crate::checkpoint::SharedState::default();
    assert!(Game::restore_checkpoint(
        game.checkpoint(&mut shared),
        &shared,
        game.content().clone()
    )
    .is_err());
}

#[test]
fn paid_cross_region_entry_then_teleport_trap_preserves_both_real_entries() {
    use crate::floor_activation::{EntryCause, PopulationPlan, PopulationProfile, PreparedScope};
    let (mut game, actor) = fixture(vec![]);
    for region in [RegionId(2), RegionId(3)] {
        game.add_chamber(Region {
            id: region,
            name: format!("floor member {}", region.0),
            bounds: Extent::new(6, 4, 2).unwrap(),
        })
        .unwrap();
    }
    let entrance = Location {
        region: RegionId(2),
        position: Position { x: 0, y: 1, z: 0 },
    };
    let landing = Location {
        region: RegionId(3),
        position: Position { x: 2, y: 1, z: 0 },
    };
    game.world
        .connect(
            tor_world::Passage {
                from: at(5),
                direction: Direction::East,
                to: entrance,
            },
            0,
        )
        .unwrap();
    game.teleport(actor, at(5)).unwrap();
    game.configure_scenario_policy(Default::default(), Some(actor))
        .unwrap();
    game.configure_population(PopulationPlan {
        protagonist: actor,
        carried_item_pools: BTreeMap::new(),
        profiles: BTreeMap::from([(
            "empty".into(),
            PopulationProfile {
                version: 1,
                stages: vec![],
            },
        )]),
    })
    .unwrap();
    for scope in [
        PreparedScope {
            scope: "floor/1".into(),
            depth: 1,
            profile: "empty".into(),
            members: BTreeSet::from([RegionId(1)]),
            cells: vec![at(1)],
            protected: BTreeSet::new(),
            rooms: vec![],
        },
        PreparedScope {
            scope: "floor/2".into(),
            depth: 2,
            profile: "empty".into(),
            members: BTreeSet::from([RegionId(2), RegionId(3)]),
            cells: vec![landing],
            protected: BTreeSet::new(),
            rooms: vec![],
        },
    ] {
        game.register_activation_scope(scope).unwrap();
    }
    game.activate_initial_floor().unwrap();
    game.place_trap(
        entrance,
        Trap {
            id: 1,
            dc: 100,
            avoidance_dc: None,
            kind: TrapKind::Teleport {},
        },
    )
    .unwrap();
    game.act(actor, crate::Action::Move(Direction::East))
        .unwrap();
    assert_eq!(
        game.actors[&actor].location, landing,
        "outer movement must not overwrite nested relocation"
    );
    assert!(game.actors[&actor].visited.contains(&RegionId(2)));
    assert!(game.actors[&actor].visited.contains(&RegionId(3)));
    let records = game.activations_since(0).unwrap();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].cause, EntryCause::Initial);
    assert_eq!(records[1].scope, "floor/2");
    assert_eq!(
        records[1].cause,
        EntryCause::Movement,
        "first real entry activates the floor before its trap's teleport"
    );
    assert!(game.trap_resolution.is_none() && !game.region_transition_pending());
}

fn avoidable_mystery(game: &mut Game, dc: i32) {
    game.place_trap(
        at(2),
        Trap {
            id: 1,
            dc: 99,
            avoidance_dc: Some(dc),
            kind: TrapKind::Mysterious {},
        },
    )
    .unwrap();
}
fn learn_presence(game: &mut Game, actor: ActorId) {
    game.apply_discovery_total(actor, 100).unwrap();
    assert!(game.feature_known(actor, ConcealedFeatureId::Trap(1)));
    assert!(!game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
}
#[test]
fn known_trap_avoidance_is_one_paid_entry_check_not_a_query_or_save_reroll() {
    let (mut game, actor) = fixture(vec![]);
    avoidable_mystery(&mut game, 0);
    learn_presence(&mut game, actor);
    let mut expected = game.rules_rng;
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    game.act(actor, crate::Action::Move(Direction::East))
        .unwrap();
    assert_eq!(game.actors[&actor].location, at(2));
    assert_eq!(game.rules_rng, expected);
    assert_eq!(
        game.actors[&actor].trap_contacts.ids().collect::<Vec<_>>(),
        vec![1]
    );
    assert!(
        !game.feature_family_known(actor, ConcealedFeatureId::Trap(1)),
        "successful careful entry does not expose its payload family"
    );
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let checkpoint = serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    game = Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    for _ in 0..3 {
        game.scene(actor).unwrap();
        game.reconcile_trap_contacts(actor, None).unwrap();
    }
    assert_eq!(game.rules_rng, expected);
    assert!(!game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
    game.act(actor, crate::Action::Move(Direction::West))
        .unwrap();
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    game.act(actor, crate::Action::Move(Direction::East))
        .unwrap();
    assert_eq!(
        game.rules_rng, expected,
        "a distinct paid re-entry gets one new attempt"
    );
}
#[test]
fn failed_known_trap_avoidance_triggers_without_refunding_the_entry() {
    let (mut game, actor) = fixture(vec![]);
    avoidable_mystery(&mut game, 100);
    learn_presence(&mut game, actor);
    let mut expected = game.rules_rng;
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    game.act(actor, crate::Action::Move(Direction::East))
        .unwrap();
    assert_eq!(game.actors[&actor].location, at(2));
    assert_eq!(game.rules_rng, expected);
    assert!(game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
    game.reconcile_trap_contacts(actor, None).unwrap();
    assert_eq!(game.rules_rng, expected);
}
#[test]
fn another_observers_knowledge_does_not_license_avoidance_and_forced_entry_gets_none() {
    for forced in [false, true] {
        let (mut game, actor) = fixture(vec![]);
        avoidable_mystery(&mut game, 0);
        if forced {
            learn_presence(&mut game, actor);
        } else {
            let observer = game
                .spawn_actor(at(4), NonZeroU64::new(100).unwrap())
                .unwrap();
            game.reveal_feature_family(observer, ConcealedFeatureId::Trap(1))
                .unwrap();
            assert!(!game.feature_known(actor, ConcealedFeatureId::Trap(1)));
        }
        // Ordinary action commit initializes passive observation before movement.
        // Account for that real first opportunity before isolating avoidance.
        let mut after_observation = game.rules_rng;
        if !forced {
            game.capture_discovery_check(actor)
                .unwrap()
                .roll_at(&mut after_observation, true, 1);
        }
        game.initialize_discovery().unwrap();
        assert_eq!(game.rules_rng, after_observation);
        if !forced {
            assert!(!game.feature_known(actor, ConcealedFeatureId::Trap(1)));
        }
        let before = game.rules_rng;
        if forced {
            game.teleport(actor, at(2)).unwrap();
        } else {
            game.act(actor, crate::Action::Move(Direction::East))
                .unwrap();
        }
        assert_eq!(
            game.rules_rng, before,
            "no unilateral avoidance check without own knowledge and deliberate entry"
        );
        assert!(game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
    }
}
#[test]
fn ending_hover_over_a_known_trap_is_not_deliberate_movement_avoidance() {
    let (mut game, actor) = fixture(vec![]);
    avoidable_mystery(&mut game, 0);
    learn_presence(&mut game, actor);
    game.apply_condition(actor, &condition("fixture_hover"), actor, 100)
        .unwrap();
    game.teleport(actor, at(2)).unwrap();
    assert!(!game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
    let before = game.rules_rng;
    game.apply_effects(
        actor,
        &[crate::rules::Effect::RemoveCondition {
            condition: condition("fixture_hover"),
        }],
    )
    .unwrap();
    assert_eq!(game.rules_rng, before);
    assert!(game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
}

#[test]
fn athletics_resists_delivered_dart_poison_without_erasing_the_physical_hit() {
    let (mut game, actor) = fixture_with_resistance(vec![], 0);
    let strength = game
        .creature(actor)
        .unwrap()
        .derived()
        .attributes
        .get(crate::attributes::Attribute::Strength);
    let health = game.health(actor).unwrap().0;
    let mut expected = game.rules_rng;
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    crate::dice::DicePool::new(1, 4, 0)
        .unwrap()
        .roll(&mut expected, crate::dice::Edge::default());
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    game.apply_trap_contact(
        actor,
        at(2),
        TrapKind::PoisonDart {
            attack_bonus: 100,
            exposure: "poison".into(),
        },
    )
    .unwrap();
    assert_eq!(game.rules_rng, expected);
    assert!(game.health(actor).unwrap().0 < health);
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .derived()
            .attributes
            .get(crate::attributes::Attribute::Strength),
        strength
    );
}
#[test]
fn missed_dart_does_not_deliver_poison_or_roll_athletics() {
    let (mut game, actor) = fixture(vec![]);
    let before = game.health(actor).unwrap();
    let mut expected = game.rules_rng;
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    game.apply_trap_contact(
        actor,
        at(2),
        TrapKind::PoisonDart {
            attack_bonus: -100,
            exposure: "poison".into(),
        },
    )
    .unwrap();
    assert_eq!(game.rules_rng, expected);
    assert_eq!(game.health(actor).unwrap(), before);
}

#[test]
fn authored_gas_exposure_uses_athletics_and_failed_sleep_expires_at_200_active_ticks() {
    for difficulty in [0, 100] {
        let (mut game, actor) = fixture_with_resistance(vec![], difficulty);
        let mut expected = game.rules_rng;
        crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
        game.apply_trap_contact(
            actor,
            at(2),
            TrapKind::SleepingGas {
                condition: "fixture_sleep".into(),
                duration: 200,
                exposure: Some("poison".into()),
            },
        )
        .unwrap();
        assert_eq!(game.rules_rng, expected);
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .conditions()
                .has_active(&condition("fixture_sleep")),
            difficulty == 100
        );
        if difficulty == 100 {
            game.advance_physics(199).unwrap();
            assert!(game
                .creature(actor)
                .unwrap()
                .conditions()
                .has_active(&condition("fixture_sleep")));
            game.advance_physics(200).unwrap();
            assert!(!game
                .creature(actor)
                .unwrap()
                .conditions()
                .has_active(&condition("fixture_sleep")));
        }
    }
}
#[test]
fn poison_immunity_blocks_gas_without_a_check_but_does_not_classify_ordinary_sleep() {
    let (mut game, actor) = fixture(vec![Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: crate::rules::Selector::Descriptor {
                descriptor: crate::content_ids::DescriptorId::new("poison").unwrap(),
            },
        },
    }]);
    let before = game.rules_rng;
    game.apply_trap_contact(
        actor,
        at(2),
        TrapKind::SleepingGas {
            condition: "fixture_sleep".into(),
            duration: 200,
            exposure: Some("poison".into()),
        },
    )
    .unwrap();
    assert_eq!(
        game.rules_rng, before,
        "immunity precedes and prevents the Athletics check"
    );
    assert!(!game
        .creature(actor)
        .unwrap()
        .conditions()
        .has_active(&condition("fixture_sleep")));
    game.apply_effects(
        actor,
        &[crate::rules::Effect::ApplyCondition {
            condition: condition("fixture_sleep"),
            duration: 200,
        }],
    )
    .unwrap();
    assert!(
        game.creature(actor)
            .unwrap()
            .conditions()
            .has_active(&condition("fixture_sleep")),
        "the condition name is not an exposure classification"
    );
    assert_eq!(game.rules_rng, before);
}
#[test]
fn walking_into_an_already_open_known_trapdoor_falls_without_an_avoidance_roll() {
    let (mut game, actor) = fixture(vec![]);
    let target = Location {
        region: RegionId(2),
        position: Position { x: 3, y: 1, z: 1 },
    };
    game.add_chamber(Region {
        id: target.region,
        name: "below".into(),
        bounds: Extent::new(6, 4, 2).unwrap(),
    })
    .unwrap();
    game.register_named_anchors(
        target.region,
        BTreeMap::from([("landing".into(), target.position)]),
    )
    .unwrap();
    for region in [RegionId(1), RegionId(2)] {
        game.set_gravity(region, [0, 0, -1]).unwrap();
    }
    game.place_trap(
        at(2),
        Trap {
            id: 1,
            dc: 99,
            avoidance_dc: Some(0),
            kind: TrapKind::TrapDoor {
                destination: tor_world::NamedAnchor {
                    region: target.region,
                    name: "landing".into(),
                },
                open: false,
            },
        },
    )
    .unwrap();
    learn_presence(&mut game, actor);
    game.world.open_trapdoor(at(2)).unwrap();
    let before = game.rules_rng;
    game.act(actor, crate::Action::Move(Direction::East))
        .unwrap();
    assert_eq!(
        game.rules_rng, before,
        "an open physical hole cannot be avoided by a pressure-plate check"
    );
    game.advance_physics(200).unwrap();
    assert_eq!(game.actors[&actor].location.region, target.region);
}

#[test]
fn residency_continuation_preserves_pre_entry_avoidance_without_an_early_roll() {
    let (mut game, actor) = fixture(vec![]);
    let target = Location {
        region: RegionId(2),
        position: Position { x: 3, y: 1, z: 1 },
    };
    game.add_chamber(Region {
        id: target.region,
        name: "below".into(),
        bounds: Extent::new(6, 4, 2).unwrap(),
    })
    .unwrap();
    game.register_named_anchors(
        target.region,
        BTreeMap::from([("landing".into(), target.position)]),
    )
    .unwrap();
    game.place_trap(
        at(2),
        Trap {
            id: 1,
            dc: 99,
            avoidance_dc: Some(0),
            kind: TrapKind::TrapDoor {
                destination: tor_world::NamedAnchor {
                    region: target.region,
                    name: "landing".into(),
                },
                open: false,
            },
        },
    )
    .unwrap();
    learn_presence(&mut game, actor);
    let eligible = game.known_traps_at_placement(actor, at(2), 0).unwrap();
    assert_eq!(eligible, BTreeSet::from([1]));
    let record = game.world.detach_region(target.region).unwrap();
    game.actors.get_mut(&actor).unwrap().location = at(2);
    let mut expected = game.rules_rng;
    game.host_region_continuation = true;
    game.reconcile_trap_contacts_after_entry(actor, None, eligible.clone())
        .unwrap();
    game.host_region_continuation = false;
    let boundary = game.trap_region_boundary().unwrap();
    assert_eq!(boundary.avoidable_traps, eligible);
    assert_eq!(boundary.orientation, 0);
    assert_eq!(game.rules_rng, expected);
    assert!(game.actors[&actor].trap_contacts.ids().next().is_none());
    game.world.attach_region(record).unwrap();
    game.complete_region_transition(&crate::TransitionReport::default())
        .unwrap();
    crate::dice::roll_check(&mut expected, crate::dice::Edge::default());
    assert_eq!(game.rules_rng, expected);
    assert!(!game.region_transition_pending());
    assert!(game.trap_resolution.is_none());
    assert!(matches!(
        game.world.trap(at(2)).unwrap().kind,
        TrapKind::TrapDoor { open: false, .. }
    ));
    assert!(!game.feature_family_known(actor, ConcealedFeatureId::Trap(1)));
    assert_eq!(
        game.actors[&actor].trap_contacts.ids().collect::<Vec<_>>(),
        vec![1]
    );
}
