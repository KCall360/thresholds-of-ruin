use std::collections::BTreeMap;
use std::num::NonZeroU64;
use tor_simulation::combat::{CombatEvent, DamageType};
use tor_simulation::creatures::Template;
use tor_simulation::{Action, ActorId, CreatureIdentity, EffectSpec, Game};
use tor_world::{Location, Position, RegionId};

fn game() -> Game {
    let mut game = Game::two_room_in_stone(42);
    for x in 1..=2 {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            CreatureIdentity {
                name: format!("subject {x}"),
                faction: format!("team{x}"),
            },
            super::creature_state::build(),
        )
        .unwrap();
    }
    game.refresh_navigation();
    game
}

#[test]
fn preparation_timing_survives_speed_changes_resume_and_checkpoint() {
    use tor_simulation::attributes::Attributes;
    for paused in [false, true] {
        let mut game = game();
        let actor = ActorId(1);
        let target = ActorId(2);
        game.act(actor, Action::Attack { target }).unwrap();
        assert_eq!(game.preparation(actor).unwrap().remaining, 50);
        if paused {
            game.pause_preparation(actor).unwrap();
        }
        let mut build = game.creature(actor).unwrap().build().clone();
        build
            .set_initial_attributes(Attributes::new([2, 5, 2, 2, 1, 1]).unwrap())
            .unwrap();
        game.rebuild_creature(actor, build).unwrap();
        assert_eq!(game.effective_combat(actor).unwrap().attack.wind_up, 30);
        assert_eq!(game.effective_combat(actor).unwrap().attack.recovery, 20);
        if paused {
            game.act(actor, Action::Attack { target }).unwrap();
        }
        assert_eq!(game.preparation(actor).unwrap().remaining, 50);
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let checkpoint = game.checkpoint(&mut shared);
        let mut restored = Game::restore_checkpoint(checkpoint, &shared).unwrap();
        assert_eq!(game, restored);
        for state in [&mut game, &mut restored] {
            state.act(target, Action::Wait).unwrap();
            assert_eq!(state.tick(), 84, "the underway attack retains 50/34 phases");
            assert!(state.preparation(actor).is_none());
        }
        assert_eq!(game, restored);
        game.act(actor, Action::Attack { target }).unwrap();
        assert_eq!(
            game.preparation(actor).unwrap().remaining,
            30,
            "new work uses the new Speed"
        );
    }
}

#[test]
fn game_damage_healing_and_transformations_share_one_injury_balance() {
    let mut game = game();
    let actor = ActorId(1);
    let original = game.health(actor).unwrap().1;
    game.apply_effects(
        actor,
        &[EffectSpec::Damage {
            components: BTreeMap::from([(DamageType::Impact, 5)]),
        }],
    )
    .unwrap();
    assert_eq!(game.health(actor), Some((original - 5, original)));
    assert_eq!(game.creature(actor).unwrap().health().injury(), 5);
    let mut build = game.creature(actor).unwrap().build().clone();
    build.set_templates(vec![Template::zombified(0)]).unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert_eq!(game.health(actor), Some((original - 3, original + 2)));
    game.apply_effects(actor, &[EffectSpec::Heal { amount: 2 }])
        .unwrap();
    assert_eq!(game.creature(actor).unwrap().health().injury(), 3);
    let mut build = game.creature(actor).unwrap().build().clone();
    build.set_templates(vec![]).unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert_eq!(game.health(actor), Some((original - 3, original)));
}

#[test]
fn removing_last_hd_runs_ordinary_death_cleanup_once_and_cannot_revive() {
    let mut game = game();
    let actor = ActorId(1);
    let original = game.creature(actor).unwrap().build().clone();
    let mut drained = original.clone();
    while drained.remove_latest().is_some() {}
    assert!(game.rebuild_creature(actor, drained).unwrap().died);
    assert!(!game.alive(actor));
    assert_eq!(
        game.combat_events()
            .iter()
            .filter(|event| matches!(event, CombatEvent::Died { actor: ActorId(1) }))
            .count(),
        1
    );
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let data = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let (snapshot, shared) = serde_json::from_slice(&data).unwrap();
    game = Game::restore_checkpoint(snapshot, &shared).unwrap();
    assert_eq!(game.health(actor), Some((0, 0)));
    game.rebuild_creature(actor, original).unwrap();
    assert!(!game.alive(actor));
    assert!(game
        .apply_effects(actor, &[EffectSpec::Heal { amount: 100 }])
        .is_err());
    assert_eq!(
        game.combat_events()
            .iter()
            .filter(|event| matches!(event, CombatEvent::Died { actor: ActorId(1) }))
            .count(),
        1
    );
}

#[test]
fn game_checkpoint_rebuilds_creature_caches_and_preserves_next_attack_outcome() {
    let mut original = game();
    original
        .act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = original.checkpoint(&mut shared);
    let data = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let text = std::str::from_utf8(&data).unwrap();
    assert!(!text.contains("maximum_health"));
    assert!(!text.contains("active_talents"));
    let (snapshot, shared) = serde_json::from_slice(&data).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared).unwrap();
    assert_eq!(restored.creature(ActorId(1)), original.creature(ActorId(1)));
    original.act(ActorId(2), Action::Wait).unwrap();
    restored.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(restored.health(ActorId(2)), original.health(ActorId(2)));
    assert_eq!(restored.combat_events(), original.combat_events());
    assert!(original
        .combat_events()
        .iter()
        .any(|event| matches!(event, CombatEvent::Resolved { .. })));
}

#[test]
fn initial_configuration_cannot_reset_an_existing_creature_or_accept_invalid_identity() {
    let mut game = game();
    let actor = ActorId(1);
    let before = game.creature(actor).unwrap().clone();
    assert!(game
        .configure_creature(
            actor,
            CreatureIdentity {
                name: "reset".into(),
                faction: "a".into()
            },
            super::creature_state::build()
        )
        .is_err());
    assert_eq!(game.creature(actor), Some(&before));
    let actor = game
        .spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 3, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    assert!(game
        .configure_creature(
            actor,
            CreatureIdentity {
                name: "\n".into(),
                faction: "a".into()
            },
            super::creature_state::build()
        )
        .is_err());
    assert!(game.creature(actor).is_none());
    assert!(game.health(actor).is_none());
}

#[test]
fn queued_natural_melee_matches_the_shared_seeded_check_and_damage_kernel() {
    let mut game = game();
    let attacker = game.creature(ActorId(1)).unwrap();
    let target = game.creature(ActorId(2)).unwrap();
    let damage = attacker.derived().melee_damage(0).unwrap();
    let mut rng = 42;
    let expected = tor_simulation::damage::AttackCheck {
        check: tor_simulation::attributes::SkillCheck {
            skill: attacker.build().species().melee.skill(),
            binding: attacker.build().binding(),
            modifier: 0,
            threshold: target.derived().defenses.physical,
        },
        attributes: attacker.derived().attributes,
        skills: attacker.derived().skills,
    }
    .resolve(
        &mut rng,
        tor_simulation::dice::Edge::default(),
        &damage,
        &target.derived().protection,
    );
    let expected_damage = expected.damage.as_ref().map_or(0, |damage| damage.total);
    assert!(expected.check.success);
    assert!(expected_damage > 0);
    let maximum = game.health(ActorId(2)).unwrap().1;
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 50);
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(
        game.health(ActorId(2)),
        Some((maximum - expected_damage, maximum))
    );
    assert!(game.combat_events().iter().any(|event| matches!(event,
        CombatEvent::Resolved { actor: ActorId(1), target: ActorId(2), hit, damage, .. }
            if *hit == expected.check.success && *damage == expected_damage)));
}

#[test]
fn creature_actor_records_reject_mixed_fields_missing_state_and_stale_anatomy() {
    let game = game();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let value = serde_json::to_value((snapshot, shared)).unwrap();
    assert!(value[1]["actors"][0]["1"]["combat"]["creature"].is_object());
    for field in ["hp", "derived"] {
        let mut forged = value.clone();
        forged[1]["actors"][0]["1"]["combat"][field] = serde_json::json!(10);
        assert!(serde_json::from_value::<(
            tor_simulation::checkpoint::Snapshot,
            tor_simulation::checkpoint::SharedState
        )>(forged)
        .is_err());
    }
    let mut forged = value.clone();
    forged[1]["actors"][0]["1"]["combat"]
        .as_object_mut()
        .unwrap()
        .remove("creature");
    assert!(serde_json::from_value::<(
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState
    )>(forged)
    .is_err());
    let mut forged = value;
    forged[1]["actors"][0]["1"]["anatomy"]["slots"] = serde_json::json!([]);
    let (snapshot, shared) = serde_json::from_value(forged).unwrap();
    assert!(Game::restore_checkpoint(snapshot, &shared).is_none());
}

#[test]
fn identity_text_is_bounded_and_required_when_decoding() {
    for (name, faction) in [
        ("x".repeat(61), "a".into()),
        ("x".into(), "a".repeat(81)),
        ("\n".into(), "a".into()),
        ("x".into(), "".into()),
    ] {
        let data =
            serde_json::to_vec(&serde_json::json!({ "name": name, "faction": faction })).unwrap();
        assert!(serde_json::from_slice::<CreatureIdentity>(&data).is_err());
    }
    assert!(serde_json::from_str::<CreatureIdentity>("{\"name\":\"x\"}").is_err());
}

#[test]
fn equipped_fixed_melee_keeps_flat_talent_damage_and_scales_its_physical_phases() {
    let mut game = game();
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build
        .train(0, tor_simulation::attributes::Skill::HeavyWeaponry)
        .unwrap();
    build
        .select_talent(0, tor_simulation::talents::Talent::HeavyBlows)
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    let mut weapon = tor_simulation::ItemSpec::ordinary("test weapon".into());
    weapon.class = tor_simulation::ItemClass::Weapon;
    weapon.equipment = Some(tor_simulation::EquipmentSpec {
        slot: tor_simulation::EquipmentSlot::Weapon,
        attack: Some(
            tor_simulation::attacks::MeleeAttack::fixed(
                tor_simulation::attributes::Skill::HeavyWeaponry,
                2,
                60,
                40,
                DamageType::Impact,
                None,
                4,
            )
            .unwrap(),
        ),
        defense: 0,
        reductions: BTreeMap::new(),
    });
    let item = game
        .place_item_stack(
            100,
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            Some(ActorId(1)),
            1,
            weapon,
        )
        .unwrap();
    game.equip_authored(ActorId(1), item, tor_simulation::EquipmentSlotId(0))
        .unwrap();
    let maximum = game.health(ActorId(2)).unwrap().1;
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 50);
    assert_eq!(
        game.effective_combat(ActorId(1)).unwrap().attack.recovery,
        34
    );
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(game.health(ActorId(2)), Some((maximum - 5, maximum)));
}

#[test]
fn anatomy_configuration_cannot_desynchronize_a_creature_build() {
    let mut game = game();
    let before = game.anatomy(ActorId(1)).unwrap().clone();
    assert!(game
        .configure_anatomy(ActorId(1), tor_simulation::AnatomySpec::default())
        .is_err());
    assert_eq!(game.anatomy(ActorId(1)), Some(&before));
    assert_eq!(game.creature(ActorId(1)).unwrap().derived().anatomy, before);
}

#[test]
fn equipped_attack_uses_its_skill_timing_and_mixed_damage_through_checkpoint() {
    use tor_simulation::attributes::{Skill, SkillCheck};
    use tor_simulation::damage::{AttackCheck, DamageComponent, DamageSpec, Protection};
    use tor_simulation::dice::{DicePool, Edge};
    use tor_simulation::grants::Descriptor;
    let mut game = game();
    let gear: tor_simulation::EquipmentSpec = serde_json::from_value(serde_json::json!({
        "slot":"weapon", "defense":0, "reductions":{},
        "attack":{"skill":"light_weaponry", "bonus":100,"wind_up":90,"recovery":70,
            "damage":{"primary":{"category":"energy","descriptor":"fire","sides":6},
                "components":[
                    {"category":"energy","descriptor":"fire","amount":{"type":"rolled","count":2,"sides":6,"bonus":-1}},
                    {"category":"keen","amount":{"type":"fixed","value":3}}
                ]}}
    })).unwrap();
    let mut weapon = tor_simulation::ItemSpec::ordinary("fire blade".into());
    weapon.class = tor_simulation::ItemClass::Weapon;
    weapon.equipment = Some(gear);
    let item = game
        .place_item_stack(
            100,
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            Some(ActorId(1)),
            1,
            weapon,
        )
        .unwrap();
    game.equip_authored(ActorId(1), item, tor_simulation::EquipmentSlotId(0))
        .unwrap();
    let actor = game.creature(ActorId(1)).unwrap();
    let target = game.creature(ActorId(2)).unwrap();
    let fire = DamageComponent::rolled(
        DamageType::Energy,
        Some(Descriptor::Fire),
        DicePool::new(2, 6, -1).unwrap(),
    );
    let primary = fire.key();
    let damage = DamageSpec::new(
        vec![fire, DamageComponent::fixed(DamageType::Keen, None, 3)],
        Some(primary),
    )
    .unwrap();
    let mut rng = 42;
    let expected = AttackCheck {
        check: SkillCheck {
            skill: Skill::LightWeaponry,
            binding: actor.build().binding(),
            modifier: 100,
            threshold: target.derived().defenses.physical,
        },
        attributes: actor.derived().attributes,
        skills: actor.derived().skills,
    }
    .resolve(&mut rng, Edge::default(), &damage, &Protection::default());
    assert!(expected.check.success);
    let expected_damage = expected.damage.unwrap().total;
    let maximum = game.health(ActorId(2)).unwrap().1;
    game.set_combat_diagnostics(true);
    game.act(ActorId(1), Action::Attack { target: ActorId(2) })
        .unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 75);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut restored = Game::restore_checkpoint(checkpoint, &shared).unwrap();
    for game in [&mut game, &mut restored] {
        game.set_combat_diagnostics(true);
        game.act(ActorId(2), Action::Wait).unwrap();
        assert_eq!(
            game.health(ActorId(2)),
            Some((maximum.saturating_sub(expected_damage), maximum))
        );
        let record = game.combat_diagnostics().unwrap().records().back().unwrap();
        assert!(record.trace.steps().iter().any(|step| matches!(step,
            tor_simulation::resolution_diagnostics::ResolutionRecord::Check(check)
                if check.check.skill == Skill::LightWeaponry && check.check.modifier == 100 && check.rank == 0)));
    }
}
