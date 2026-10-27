//! Item mutation and sheet publication share the ordinary paid action boundary.
use crate::rules::*;
use crate::{
    ActorId, ArmorMaterial, ArmorSpec, EquipmentSlot, EquipmentSlotId, EquipmentSpec, Game, ItemId,
    ItemSpec,
};

fn armor() -> ArmorSpec {
    ArmorSpec {
        physical_reduction: 2,
        material: ArmorMaterial::Metal,
        rustproof: false,
    }
}
fn fixture(armor: Option<ArmorSpec>, contacts: usize) -> Game {
    fixture_with_tail(armor, contacts, false)
}
fn fixture_with_tail(armor: Option<ArmorSpec>, contacts: usize, fatal_tail: bool) -> Game {
    fixture_config(armor, contacts, fatal_tail, false)
}
fn fixture_config(
    armor: Option<ArmorSpec>,
    contacts: usize,
    fatal_tail: bool,
    mixed_tail: bool,
) -> Game {
    let offsets = (0..contacts).map(|i| i as u64 * 10).collect::<Vec<_>>();
    let mut game = super::strike_tests::sequence_game_on_world_configured(
        &offsets,
        |ability| {
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![RiderDef {
                    cancellable: false,
                    trigger: RiderTrigger::Success,
                    condition: Some(RiderCondition::SequenceSuccessfulHits { minimum: 1 }),
                    check: None,
                    effects: vec![Effect::CorrodeArmor {
                        slot: EquipmentSlot::new("body_armor").unwrap(),
                        amount: 1,
                    }],
                    descriptors: vec![],
                }],
            };
            if fatal_tail {
                ability.cost = Some(Cost {
                    resource: crate::resources::Resource::Stamina,
                    start: 1,
                    resolution: 2,
                });
                let StrikeSource::NaturalMelee { additional_riders } = &mut ability.strikes else {
                    unreachable!()
                };
                additional_riders.push(RiderDef {
                    cancellable: false,
                    trigger: RiderTrigger::Success,
                    condition: None,
                    check: None,
                    descriptors: vec![],
                    effects: vec![Effect::Damage {
                        damage: DamageExpr::Components {
                            components: vec![DamageComponentExpr {
                                category: crate::combat::DamageType::Vital,
                                descriptor: None,
                                amount: AmountExpr::Fixed { amount: 1000 },
                            }],
                        },
                    }],
                });
            }
        },
        |document, species| {
            if fatal_tail {
                for resource in &mut document.resources {
                    resource.capacity.base = 100;
                }
            }
            // NaturalAttack resolves the real named skill, not legacy melee
            // metadata accuracy. Keep this synthetic contact witness explicit.
            species.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Add {
                    stat: Stat::Ability {
                        ability: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                        stat: AbilityStat::CheckBonus,
                    },
                    value: 100,
                },
            });
            species.melee = crate::attacks::NaturalAttack::new(
                crate::attributes::Skill::HeavyWeaponry,
                0,
                10,
                20,
                offsets
                    .iter()
                    .map(|offset| StrikeDef {
                        cancellable: false,
                        offset: *offset,
                        unlock: None,
                        check: Some(CheckDef {
                            actor: CheckActor::User,
                            skill: SkillSelector::NaturalAttack,
                            against: Against::Defense {
                                defense: DefenseKind::Physical,
                            },
                        }),
                        on_success: vec![Effect::Damage {
                            damage: DamageExpr::Components {
                                components: if mixed_tail && *offset > 0 {
                                    [
                                        (crate::combat::DamageType::Impact, 3),
                                        (crate::combat::DamageType::Keen, 3),
                                        (crate::combat::DamageType::Energy, 9),
                                    ]
                                    .into_iter()
                                    .map(|(category, amount)| DamageComponentExpr {
                                        category,
                                        descriptor: None,
                                        amount: AmountExpr::Fixed { amount },
                                    })
                                    .collect()
                                } else {
                                    vec![DamageComponentExpr {
                                        category: crate::combat::DamageType::Impact,
                                        descriptor: None,
                                        amount: AmountExpr::Fixed { amount: 0 },
                                    }]
                                },
                            },
                        }],
                        on_failure: vec![],
                        descriptors: vec![],
                        riders: vec![],
                    })
                    .collect(),
            )
            .unwrap();
        },
        None,
        |game| {
            if let Some(armor) = armor {
                let owner = ActorId(2);
                let slot = game.actors[&owner]
                    .anatomy
                    .slots
                    .iter()
                    .position(|s| s.as_str() == "body_armor")
                    .unwrap();
                let mut spec = ItemSpec::ordinary("mail".into());
                spec.class = crate::ItemClass::Armor;
                spec.equipment = Some(EquipmentSpec {
                    periodic_relocation: None,
                    potency_modifiers: Vec::new(),
                    weapon_profile: None,
                    slot: EquipmentSlot::new("body_armor").unwrap(),
                    armor: Some(armor),
                    attack: None,
                    modifiers: vec![Modifier {
                        cancellable: false,
                        scope: Scope::Always,
                        op: ModifierOp::Reduction {
                            selector: Selector::Category {
                                category: crate::combat::DamageType::Energy,
                            },
                            amount: 7,
                        },
                    }],
                });
                let at = game.actors[&owner].location;
                game.place_item_stack(20, at, Some(owner), 1, spec.clone())
                    .unwrap();
                game.place_item_stack(21, at, Some(owner), 1, spec).unwrap();
                game.equip_authored(owner, ItemId(20), EquipmentSlotId(slot as u16))
                    .unwrap();
            }
        },
    )
    .0;
    ready_peer(&mut game);
    game
}
fn ready_peer(game: &mut Game) {
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    assert_eq!(game.next_actor(), Some(ActorId(2)));
    // Direct inner resolution can append SequenceEnded after strike publication.
    // Complete the ordinary host publication before persisting this decision.
    game.seal_event_observations().unwrap();
}
fn round_trip(game: &Game) -> Game {
    assert!(game.event_receipts_valid(), "fixture event receipts");
    assert!(game.combat_valid(), "fixture combat state");
    assert!(game.loaded_holds_valid(), "fixture hold state");
    for event in game.combat_events() {
        if let crate::combat::CombatEvent::Resolved { outcome, .. } = event {
            assert!(outcome.valid(), "fixture outcome: {outcome:?}");
        }
    }
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    let raw = serde_json::to_vec(&checkpoint).unwrap();
    Game::restore_checkpoint(
        serde_json::from_slice(&raw).unwrap(),
        &shared,
        game.content().clone(),
    )
    .unwrap()
}
#[test]
fn zero_damage_contacts_rust_once_per_sequence_without_recompiling_permanent_sheet() {
    let mut game = fixture(Some(armor()), 3);
    let physical = game
        .creature(ActorId(2))
        .unwrap()
        .derived()
        .defenses
        .physical;
    let before = crate::diagnostics::work_counts();
    for index in 0..3 {
        game.tick = (index + 1) * 10;
        game.resolve_preparations().unwrap();
        assert_eq!(game.items[&ItemId(20)].condition.corrosion, 1);
        assert_eq!(game.items[&ItemId(21)].condition.corrosion, 0);
        assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .derived()
                .defenses
                .physical,
            physical
        );
    }
    let after = crate::diagnostics::work_counts();
    assert_eq!(
        after.permanent_creature_compiles,
        before.permanent_creature_compiles
    );
    assert_eq!(after.raw_hit_die_visits, before.raw_hit_die_visits);
    assert_eq!(
        after.advancement_choice_visits,
        before.advancement_choice_visits
    );
    let rust = game
        .combat_events()
        .iter()
        .filter_map(|event| match event {
            crate::combat::CombatEvent::Resolved { outcome, .. } => Some(&outcome.effects),
            _ => None,
        })
        .flatten()
        .filter(|effect| {
            matches!(
                effect,
                super::ResolvedEffect::CorrodeArmor { amount: 1, .. }
            )
        })
        .count();
    assert_eq!(rust, 1);
    ready_peer(&mut game);
    assert_eq!(round_trip(&game), game);
}
#[test]
fn rust_floor_exemptions_and_replay_preserve_exact_items() {
    for spec in [
        None,
        Some(ArmorSpec {
            material: ArmorMaterial::Nonmetal,
            ..armor()
        }),
        Some(ArmorSpec {
            rustproof: true,
            ..armor()
        }),
        Some(ArmorSpec {
            physical_reduction: 0,
            ..armor()
        }),
    ] {
        let mut game = fixture(spec, 1);
        let mut restored = round_trip(&game);
        for game in [&mut game, &mut restored] {
            game.tick = 10;
            game.resolve_preparations().unwrap();
            ready_peer(game);
            if let Some(item) = game.items.get(&ItemId(20)) {
                assert_eq!(item.condition.corrosion, 0);
            }
            assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
        }
        assert_eq!(restored, game);
        assert_eq!(round_trip(&game), game);
    }
    let mut game = fixture(Some(armor()), 1);
    for expected in [1, 2, 2] {
        // The helper is invoked only in a private candidate, as lifecycle does.
        let mut candidate = game.clone();
        let mut outcome = candidate.settled_actor_outcome(ActorId(2)).unwrap();
        let applied = candidate
            .corrode_armor_in_outcome(&mut outcome, &EquipmentSlot::new("body_armor").unwrap(), 1)
            .unwrap();
        assert_eq!(
            applied,
            u32::from(game.items[&ItemId(20)].condition.corrosion < 2)
        );
        candidate.apply_outcome(outcome).unwrap();
        game = candidate;
        assert_eq!(game.items[&ItemId(20)].condition.corrosion, expected);
        assert_eq!(round_trip(&game), game);
    }
}

#[test]
fn corrosion_owned_plan_amount_and_item_state_are_strict() {
    let slot = EquipmentSlot::new("body_armor").unwrap();
    for amount in [0, crate::limits::NUMERIC as u32 + 1] {
        let value = serde_json::json!({"kind":"corrode_armor","slot":slot,"amount":amount});
        assert!(serde_json::from_value::<Effect>(value.clone()).is_err());
        assert!(serde_json::from_value::<super::PlannedEffect>(value.clone()).is_err());
        assert!(serde_json::from_value::<super::ResolvedEffect>(value).is_err());
    }
    let mut game = fixture(Some(armor()), 1);
    game.items
        .edit(ItemId(20), |item| item.condition.corrosion = 3)
        .unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let checkpoint = game.checkpoint(&mut shared);
    assert!(Game::restore_checkpoint(checkpoint, &shared, game.content().clone()).is_err());
}

#[test]
fn later_lifecycle_failure_rolls_back_corrosion_sheet_rng_and_paid_action() {
    let mut game = fixture_with_tail(Some(armor()), 1, true);
    game.next_item_id = u64::MAX;
    let before = game.clone();
    let rng = game.rules_rng;
    let hold = game.preparation(ActorId(1)).unwrap().hold.unwrap();
    let mut expected = game.creature(ActorId(1)).unwrap().into_owned();
    assert_eq!(expected.costs().reservations().len(), 1);
    expected.advance_active(10);
    expected.finish_cost(hold).unwrap();
    let result = game.with_recovery_transition(true, |candidate| {
        candidate.tick = 10;
        let result = candidate.resolve_preparations();
        assert_eq!(result, Err(crate::GameError::IdentityExhausted));
        assert_eq!(
            candidate.items[&ItemId(20)].condition.corrosion,
            1,
            "the first rider must actually mutate armor before later corpse allocation fails"
        );
        result
    });
    assert_eq!(result, Err(crate::GameError::IdentityExhausted));
    assert_eq!(game, before);
    assert_eq!(game.rules_rng, rng);
    let mut legal = before;
    legal.next_item_id = 22;
    legal
        .with_recovery_transition(true, |candidate| {
            candidate.tick = 10;
            candidate.resolve_preparations()
        })
        .unwrap();
    assert_ne!(legal.rules_rng, rng);
    assert_eq!(
        legal.creature(ActorId(1)).unwrap().costs().resources(),
        expected.costs().resources()
    );
    assert!(legal
        .creature(ActorId(1))
        .unwrap()
        .costs()
        .reservations()
        .is_empty());
    assert_eq!(legal.items[&ItemId(20)].condition.corrosion, 1);
    assert!(matches!(
        legal.items[&ItemId(20)].location,
        crate::ItemLocation::Ground(_)
    ));
}

#[test]
fn later_strike_uses_worn_physical_reduction_and_preserves_other_categories() {
    use crate::combat::DamageType;
    let mut game = fixture_config(Some(armor()), 2, false, true);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert_eq!(game.items[&ItemId(20)].condition.corrosion, 1);
    assert_eq!(game.health(ActorId(2)).unwrap().0, 100);
    let creature = game.creature(ActorId(2)).unwrap();
    let protection = &creature.derived().protection;
    for (category, amount) in [
        (DamageType::Impact, 1),
        (DamageType::Keen, 1),
        (DamageType::Energy, 7),
    ] {
        assert_eq!(
            protection.reduction(&Selector::Category { category }),
            amount
        );
    }
    game.tick = 20;
    game.resolve_preparations().unwrap();
    assert_eq!(
        game.health(ActorId(2)).unwrap().0,
        94,
        "3 Impact -1, 3 Keen -1, 9 Energy -7"
    );
    assert_eq!(
        game.items[&ItemId(20)].condition.corrosion,
        1,
        "the once-rider cannot apply twice"
    );
    ready_peer(&mut game);
    assert_eq!(round_trip(&game), game);
}

#[test]
fn corrosion_survives_death_pickup_unequip_drop_and_re_equip() {
    use crate::{Action, ItemLocation};
    let actor = ActorId(1);
    let item = ItemId(20);
    let mut game = fixture_with_tail(Some(armor()), 1, true);
    game.tick = 10;
    game.resolve_preparations().unwrap();
    let ItemLocation::Ground(at) = game.items[&item].location else {
        panic!()
    };
    assert_eq!(game.items[&item].condition.corrosion, 1);
    game.advance_to_next_decision().unwrap();
    assert_eq!(game.next_actor(), Some(actor));
    game.teleport(actor, at).unwrap();
    game.act(
        actor,
        Action::Take {
            item,
            quantity: None,
        },
    )
    .unwrap();
    assert_eq!(game.items[&item].location, ItemLocation::Carried(actor));
    let slot = EquipmentSlotId(
        game.actors[&actor]
            .anatomy
            .slots
            .iter()
            .position(|slot| slot.as_str() == "body_armor")
            .unwrap() as u16,
    );
    for action in [
        Action::Equip { item, slot },
        Action::Unequip { item },
        Action::Drop {
            item,
            quantity: None,
        },
        Action::Take {
            item,
            quantity: None,
        },
        Action::Equip { item, slot },
    ] {
        game.act(actor, action).unwrap();
        if let Some(preparation) = game.preparation(actor) {
            game.tick += preparation.remaining;
            game.resolve_preparations().unwrap();
        }
        assert_eq!(game.items[&item].condition.corrosion, 1);
        assert_eq!(round_trip(&game), game);
    }
    assert_eq!(game.equipment(actor).unwrap().get(&slot), Some(&item));
    assert_eq!(
        game.creature(actor)
            .unwrap()
            .derived()
            .protection
            .reduction(&Selector::Category {
                category: crate::combat::DamageType::Impact
            }),
        1
    );
    assert_eq!(game.items[&ItemId(21)].condition.corrosion, 0);
}

#[test]
fn worn_armor_protection_prevents_new_corrosion_without_repair_and_removal_restores_risk() {
    let mut game = fixture(Some(armor()), 1);
    let owner = ActorId(2);
    let body = EquipmentSlot::new("body_armor").unwrap();
    let corrode = |game: &mut Game| {
        let mut outcome = game.settled_actor_outcome(owner).unwrap();
        let applied = game
            .corrode_armor_in_outcome(&mut outcome, &body, 1)
            .unwrap();
        game.apply_outcome(outcome).unwrap();
        applied
    };
    assert_eq!(corrode(&mut game), 1);
    let (socket, slot) = game.actors[&owner]
        .anatomy
        .slots
        .iter()
        .enumerate()
        .find(|(socket, slot)| {
            slot.as_str() == "ring"
                && !game.actors[&owner]
                    .equipment
                    .contains_key(&EquipmentSlotId(*socket as u16))
        })
        .map(|(socket, slot)| (EquipmentSlotId(socket as u16), slot.clone()))
        .unwrap();
    let mut spec = ItemSpec::ordinary("armor ward".into());
    spec.class = crate::ItemClass::Ring;
    spec.equipment = Some(EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        slot,
        armor: None,
        attack: None,
        modifiers: vec![Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::ProtectArmor {},
        }],
    });
    let at = game.actors[&owner].location;
    game.place_item_stack(22, at, Some(owner), 1, spec).unwrap();
    game.equip_authored(owner, ItemId(22), socket).unwrap();
    assert!(game.creature(owner).unwrap().derived().protects_armor);
    assert_eq!(game.items[&ItemId(20)].condition.corrosion, 1);
    assert_eq!(corrode(&mut game), 0);
    assert_eq!(game.items[&ItemId(20)].condition.corrosion, 1);
    let mut outcome = game.settled_actor_outcome(owner).unwrap();
    let mut equipment = outcome.state.equipment.clone();
    equipment.remove(&socket);
    game.refresh_equipment_outcome(&mut outcome, equipment)
        .unwrap();
    game.apply_outcome(outcome).unwrap();
    assert!(!game.creature(owner).unwrap().derived().protects_armor);
    assert_eq!(corrode(&mut game), 1);
    assert_eq!(game.items[&ItemId(20)].condition.corrosion, 2);
}
