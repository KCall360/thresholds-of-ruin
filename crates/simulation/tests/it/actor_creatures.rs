use std::num::NonZeroU64;
use tor_simulation::combat::{CombatEvent, DamageType};
use tor_simulation::content_ids::{AbilityId, TalentId, TemplateId};
use tor_simulation::rules::{AmountExpr, DamageComponentExpr, DamageExpr, Effect};
use tor_simulation::{Action, ActorId, CreatureIdentity, Game};
use tor_world::{Location, Position, RegionId};

fn game() -> Game {
    game_with_seed(42)
}

fn fixture_weapon_flat() -> tor_simulation::rules::TalentDef {
    super::creature_fixture::talent(
        "fixture_weapon_flat",
        vec![super::creature_fixture::add(
            tor_simulation::rules::Stat::AbilityTag {
                tag: tor_simulation::content_ids::AbilityTagId::new("weapon_attack").unwrap(),
                stat: tor_simulation::rules::AbilityStat::FlatBonus,
            },
            1,
        )],
    )
}
fn game_with_seed(seed: u64) -> Game {
    game_with_seed_and_talents(seed, vec![])
}
fn game_with_seed_and_talents(seed: u64, talents: Vec<tor_simulation::rules::TalentDef>) -> Game {
    let build = super::creature_fixture::with_talents(super::creature_state::build(), talents);
    let mut game = Game::two_room_in_stone(seed, build.content().clone());
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
            build.clone(),
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
        game.act(
            actor,
            Action::UseAbility {
                id: AbilityId::new("basic_melee").unwrap(),
                invocation: tor_simulation::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(game.preparation(actor).unwrap().remaining, 50);
        if paused {
            game.pause_preparation(actor).unwrap();
        }
        let mut build = game.creature(actor).unwrap().build().clone();
        build
            .set_initial_attributes(
                Attributes::new(tor_simulation::attributes::ByAttribute {
                    strength: 2,
                    speed: 5,
                    intellect: 2,
                    willpower: 2,
                    awareness: 1,
                    presence: 1,
                })
                .unwrap(),
            )
            .unwrap();
        game.rebuild_creature(actor, build).unwrap();
        assert_eq!(
            game.ability_plan(actor, &AbilityId::new("basic_melee").unwrap(), target)
                .unwrap()
                .preparation(),
            30
        );
        assert_eq!(
            game.ability_plan(actor, &AbilityId::new("basic_melee").unwrap(), target)
                .unwrap()
                .recovery(),
            20
        );
        if paused {
            game.act(
                actor,
                Action::UseAbility {
                    id: AbilityId::new("basic_melee").unwrap(),
                    invocation: tor_simulation::abilities::AbilityInvocation::actor_target(target),
                    supplies: Default::default(),
                },
            )
            .unwrap();
        }
        assert_eq!(game.preparation(actor).unwrap().remaining, 50);
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let checkpoint = game.checkpoint(&mut shared);
        let mut restored =
            Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
        assert_eq!(game, restored);
        for state in [&mut game, &mut restored] {
            state.act(target, Action::Wait).unwrap();
            assert_eq!(state.tick(), 84, "the underway attack retains 50/34 phases");
            assert!(state.preparation(actor).is_none());
        }
        assert_eq!(game, restored);
        game.act(
            actor,
            Action::UseAbility {
                id: AbilityId::new("basic_melee").unwrap(),
                invocation: tor_simulation::abilities::AbilityInvocation::actor_target(target),
                supplies: Default::default(),
            },
        )
        .unwrap();
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
        &[Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![DamageComponentExpr {
                    category: DamageType::Impact,
                    descriptor: None,
                    amount: AmountExpr::Fixed { amount: 5 },
                }],
            },
        }],
    )
    .unwrap();
    assert_eq!(game.health(actor), Some((original - 5, original)));
    assert_eq!(game.creature(actor).unwrap().health().injury(), 5);
    let mut build = game.creature(actor).unwrap().build().clone();
    build
        .set_templates(vec![TemplateId::new("immune_form").unwrap()])
        .unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert_eq!(game.health(actor), Some((original - 3, original + 2)));
    game.apply_effects(
        actor,
        &[Effect::Heal {
            amount: AmountExpr::Fixed { amount: 2 },
        }],
    )
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
    game = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(game.health(actor), Some((0, 0)));
    game.rebuild_creature(actor, original).unwrap();
    assert!(!game.alive(actor));
    assert_eq!(
        game.apply_effects(
            actor,
            &[Effect::Heal {
                amount: AmountExpr::Fixed { amount: 100 }
            }]
        ),
        Ok(false)
    );
    assert!(!game.alive(actor));
    assert_eq!(game.health(actor).unwrap().0, 0);
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
        .act(
            ActorId(1),
            Action::UseAbility {
                id: AbilityId::new("basic_melee").unwrap(),
                invocation: tor_simulation::abilities::AbilityInvocation::actor_target(ActorId(2)),
                supplies: Default::default(),
            },
        )
        .unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = original.checkpoint(&mut shared);
    let data = serde_json::to_vec(&(snapshot, shared)).unwrap();
    let text = std::str::from_utf8(&data).unwrap();
    let value: serde_json::Value = serde_json::from_slice(&data).unwrap();
    let actors = value[0]["actors"].as_u64().unwrap() as usize;
    for actor in value[1]["actors"][actors].as_object().unwrap().values() {
        let creature = &actor["combat"]["creature"];
        assert!(creature.get("maximum_health").is_none());
        assert!(creature.get("derived").is_none());
        assert!(creature.get("baseline").is_none());
        assert_eq!(creature["restorable_losses"]["maximum_health"], 0);
    }
    assert!(!text.contains("active_talents"));
    let (snapshot, shared) = serde_json::from_slice(&data).unwrap();
    let mut restored =
        Game::restore_checkpoint(snapshot, &shared, original.content().clone()).unwrap();
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
    let before = game.creature(actor).unwrap().into_owned();
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
    assert_eq!(game.creature(actor).as_deref(), Some(&before));
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
    // Seed 4 opens with a natural 14, exercising the successful-hit path in 3d6.
    let mut game = game_with_seed_and_talents(4, vec![fixture_weapon_flat()]);
    let expected = expected_attack(&game);
    let expected_damage = outcome_damage(&expected);
    assert!(expected_damage > 0);
    let maximum = game.health(ActorId(2)).unwrap().1;
    game.act(
        ActorId(1),
        Action::UseAbility {
            id: AbilityId::new("basic_melee").unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::actor_target(ActorId(2)),
            supplies: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 50);
    game.act(ActorId(2), Action::Wait).unwrap();
    assert_eq!(
        game.health(ActorId(2)),
        Some((maximum - u64::from(expected_damage), maximum))
    );
    assert!(game.combat_events().iter().any(|event| matches!(event,
        CombatEvent::Resolved { outcome, .. } if outcome == &expected)));
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
    // An out-of-range primary socket is rejected at the anatomy decoder.
    assert!(serde_json::from_value::<(
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState,
    )>(forged.clone())
    .is_err());
    // A structurally valid but stale anatomy still fails source-aware restore.
    forged[1]["actors"][0]["1"]["anatomy"] =
        serde_json::to_value(tor_simulation::AnatomySpec::default()).unwrap();
    let (snapshot, shared) = serde_json::from_value(forged).unwrap();
    assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_err());
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
    // Seed 4 opens with a natural 14, exercising the successful-hit path in 3d6.
    let mut game = game_with_seed_and_talents(4, vec![fixture_weapon_flat()]);
    let mut build = game.creature(ActorId(1)).unwrap().build().clone();
    build = super::creature_fixture::with_starting_skills(
        build,
        [
            tor_simulation::attributes::Skill::HeavyWeaponry,
            tor_simulation::attributes::Skill::Athletics,
        ],
    );
    build
        .select_talent(
            tor_simulation::HdIndex(0),
            TalentId::new("fixture_weapon_flat").unwrap(),
        )
        .unwrap();
    game.rebuild_creature(ActorId(1), build).unwrap();
    let mut weapon = tor_simulation::ItemSpec::ordinary("test weapon".into());
    weapon.class = tor_simulation::ItemClass::Weapon;
    weapon.equipment = Some(tor_simulation::EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        armor: None,
        slot: tor_simulation::EquipmentSlot::new("weapon").unwrap(),
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
        modifiers: vec![],
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
    game.act(
        ActorId(1),
        Action::UseAbility {
            id: AbilityId::new("basic_melee").unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::actor_target(ActorId(2)),
            supplies: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 50);
    assert_eq!(
        game.ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2)
        )
        .unwrap()
        .recovery(),
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
    let mut game = game();
    // Positive gameplay fixtures use exhaustive typed fields. Raw JSON shape
    // acceptance/rejection belongs to the independent schema tests.
    let fire = tor_simulation::damage::DamageComponent::rolled(
        DamageType::Energy,
        Some(tor_simulation::content_ids::DescriptorId::new("fire").unwrap()),
        tor_simulation::dice::DicePool::new(2, 6, -1).unwrap(),
    );
    let primary = fire.key();
    let damage = tor_simulation::damage::DamageSpec::new(
        vec![
            fire,
            tor_simulation::damage::DamageComponent::fixed(DamageType::Keen, None, 3),
        ],
        Some(primary),
    )
    .unwrap();
    let gear = tor_simulation::EquipmentSpec {
        periodic_relocation: None,
        slot: tor_simulation::EquipmentSlot::new("weapon").unwrap(),
        armor: None,
        attack: Some(
            tor_simulation::attacks::MeleeAttack::new(
                tor_simulation::attributes::Skill::LightWeaponry,
                100,
                90,
                70,
                damage,
            )
            .unwrap(),
        ),
        weapon_profile: None,
        modifiers: vec![],
        potency_modifiers: vec![],
    };
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
    let expected = expected_attack(&game);
    let expected_damage = outcome_damage(&expected);
    assert!(expected_damage > 0);
    let maximum = game.health(ActorId(2)).unwrap().1;
    game.set_combat_diagnostics(true);
    game.act(
        ActorId(1),
        Action::UseAbility {
            id: AbilityId::new("basic_melee").unwrap(),
            invocation: tor_simulation::abilities::AbilityInvocation::actor_target(ActorId(2)),
            supplies: Default::default(),
        },
    )
    .unwrap();
    assert_eq!(game.preparation(ActorId(1)).unwrap().remaining, 75);
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let mut restored =
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap();
    for game in [&mut game, &mut restored] {
        game.set_combat_diagnostics(true);
        game.act(ActorId(2), Action::Wait).unwrap();
        assert_eq!(
            game.health(ActorId(2)),
            Some((maximum.saturating_sub(u64::from(expected_damage)), maximum))
        );
        let record = game.combat_diagnostics().unwrap().records().back().unwrap();
        assert!(record.trace.steps().iter().any(|step| step.id == "check" && step.fields.iter().any(|field|
            field.id == "bonus" && matches!(field.value, tor_simulation::resolution_diagnostics::TraceValueRecord::Signed(value) if value >= 100))));
    }
}

fn expected_attack(game: &Game) -> tor_simulation::abilities::Outcome {
    let plan = game
        .ability_plan(
            ActorId(1),
            &AbilityId::new("basic_melee").unwrap(),
            ActorId(2),
        )
        .unwrap();
    let creature = game.creature(ActorId(2)).unwrap();
    let snapshot = tor_simulation::abilities::Snapshot::capture(&creature);
    tor_simulation::abilities::resolve(
        plan.resolution().unwrap(),
        &snapshot,
        &mut game.seed(),
        &mut (),
    )
    .unwrap()
}
fn outcome_damage(outcome: &tor_simulation::abilities::Outcome) -> u32 {
    outcome
        .effects
        .iter()
        .filter_map(|effect| match effect {
            tor_simulation::abilities::ResolvedEffect::Damage { amount } => Some(*amount),
            _ => None,
        })
        .sum()
}

#[test]
fn checkpoint_owns_rule_random_stream_and_event_log_outside_run_policy() {
    let mut original = game();
    original
        .act(
            ActorId(1),
            Action::UseAbility {
                id: AbilityId::new("basic_melee").unwrap(),
                invocation: tor_simulation::abilities::AbilityInvocation::actor_target(ActorId(2)),
                supplies: Default::default(),
            },
        )
        .unwrap();
    original.act(ActorId(2), Action::Wait).unwrap();
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let snapshot = original.checkpoint(&mut shared);
    let value = serde_json::to_value((snapshot, shared)).unwrap();
    assert!(value[0]["rules_rng"].is_u64());
    assert!(value[0]["events"].is_array());
    assert!(value[0]["combat"].get("rng").is_none());
    assert!(value[0]["combat"].get("events").is_none());
    let (snapshot, shared) = serde_json::from_value(value.clone()).unwrap();
    let restored = Game::restore_checkpoint(snapshot, &shared, original.content().clone()).unwrap();
    assert_eq!(original, restored);
    let mut forged = value;
    forged[0]["combat"]["rng"] = serde_json::json!(42);
    assert!(serde_json::from_value::<(
        tor_simulation::checkpoint::Snapshot,
        tor_simulation::checkpoint::SharedState
    )>(forged)
    .is_err());
}

#[test]
fn creature_rule_failures_keep_specific_errors_and_leave_state_unchanged() {
    use tor_simulation::{creatures::BuildError, GameError};
    let mut game = game();
    let before = game.clone();
    assert_eq!(
        game.configure_creature(
            ActorId(1),
            CreatureIdentity {
                name: "invalid\n".into(),
                faction: "team1".into()
            },
            super::creature_state::build()
        ),
        Err(GameError::Build(BuildError::InvalidDefinition))
    );
    assert_eq!(
        game.configure_creature(
            ActorId(1),
            CreatureIdentity {
                name: "valid".into(),
                faction: "team1".into()
            },
            super::creature_state::build()
        ),
        Err(GameError::ActorBusy)
    );
    assert_eq!(
        game.configure_anatomy(ActorId(1), tor_simulation::AnatomySpec::default()),
        Err(GameError::Build(BuildError::InvalidDefinition))
    );
    assert_eq!(game, before);
}

#[test]
fn rejected_condition_preserves_lazy_timer_state_and_checkpoint() {
    let mut game = game();
    let actor = ActorId(1);
    let fear = tor_simulation::content_ids::ConditionId::new("fear").unwrap();
    super::creature_fixture::injure(&mut game, actor, 1).unwrap();
    game.apply_condition(actor, &fear, ActorId(2), 1_000)
        .unwrap();
    game.act(actor, Action::Wait).unwrap();
    game.act(ActorId(2), Action::Wait).unwrap();
    let before = game.clone();
    assert_eq!(
        game.apply_condition(actor, &fear, ActorId(999), 10),
        Err(tor_simulation::GameError::UnknownActor)
    );
    assert!(
        game == before,
        "a rejected lifecycle edit must not publish projected timers"
    );
    let mut shared = tor_simulation::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    assert_eq!(
        Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).unwrap(),
        before
    );
}
