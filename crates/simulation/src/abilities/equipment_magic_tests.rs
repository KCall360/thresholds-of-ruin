//! Equipment scrolls own one paid action and one exact prepared item witness.
use crate::rules::Effect;
use crate::{
    Action, ActorId, ArmorMaterial, ArmorSpec, EquipmentAlteration, EquipmentSlot, EquipmentSlotId,
    EquipmentSpec, EquippedSelection, Game, GameError, IntentionOrigin, ItemClass, ItemDiscovery,
    ItemId, ItemSpec, ReadableSpec,
};
fn fixture() -> Game {
    super::ray_tests::fixture_with(|_, _, _| {})
}
fn armor(game: &mut Game, id: u64, equipped: bool) -> ItemId {
    let actor = ActorId(1);
    let at = game.actors[&actor].location;
    let mut spec = ItemSpec::ordinary("mail".into());
    spec.class = ItemClass::Armor;
    spec.equipment = Some(EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        slot: EquipmentSlot::new("body_armor").unwrap(),
        armor: Some(ArmorSpec {
            physical_reduction: 2,
            material: ArmorMaterial::Metal,
            rustproof: false,
        }),
        attack: None,
        modifiers: vec![],
    });
    let item = game.place_item_stack(id, at, Some(actor), 1, spec).unwrap();
    if equipped {
        let slot = game.actors[&actor]
            .anatomy
            .slots
            .iter()
            .position(|s| s.as_str() == "body_armor")
            .unwrap();
        game.equip_authored(actor, item, EquipmentSlotId(slot as u16))
            .unwrap();
    }
    item
}
fn effect(alteration: EquipmentAlteration) -> Effect {
    Effect::AlterEquipment {
        selection: if alteration == EquipmentAlteration::RemoveCurse {
            EquippedSelection::All
        } else {
            EquippedSelection::Slot {
                slot: EquipmentSlot::new("body_armor").unwrap(),
                class: ItemClass::Armor,
                last: false,
            }
        },
        alteration,
    }
}
fn scroll(game: &mut Game, id: u64, effects: Vec<Effect>) -> ItemId {
    let mut spec = ItemSpec::ordinary("unread scroll".into());
    spec.class = ItemClass::Scroll;
    spec.stackable = true;
    spec.concealed = true;
    spec.identity = "fixture.gear-scroll".into();
    spec.name = "equipment scroll".into();
    spec.readable = Some(ReadableSpec {
        effects,
        descriptors: vec![],
        identify: None,
        discovery: ItemDiscovery::Never,
    });
    game.place_item_stack(
        id,
        game.actors[&ActorId(1)].location,
        Some(ActorId(1)),
        1,
        spec,
    )
    .unwrap()
}
fn execute(game: &mut Game, action: Action) {
    game.admit_intention(ActorId(1), action, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
}
#[test]
fn read_consumes_without_eligible_gear_and_never_identifies_with_or_without_effect() {
    for equipped in [false, true] {
        let mut game = fixture();
        let armor = equipped.then(|| armor(&mut game, 20, true));
        let page = scroll(
            &mut game,
            30,
            vec![effect(EquipmentAlteration::EnchantArmor)],
        );
        let rng = game.rules_rng;
        let tick = game.tick;
        execute(&mut game, Action::Read { item: page });
        assert!(!game.items.contains_key(&page));
        assert!(game.tick > tick);
        assert_eq!(game.rules_rng, rng);
        assert!(!game.actors[&ActorId(1)]
            .knowledge
            .contains("fixture.gear-scroll"));
        if let Some(armor) = armor {
            assert_eq!(game.items[&armor].condition.armor_enchantment, 1);
        }
    }
}
#[test]
fn sequential_equipment_alterations_validate_original_witness_once() {
    let mut game = fixture();
    let armor = armor(&mut game, 20, true);
    game.items
        .edit(armor, |i| i.condition.cursed = true)
        .unwrap();
    game.apply_effects(
        ActorId(1),
        &[
            effect(EquipmentAlteration::EnchantArmor),
            effect(EquipmentAlteration::EnchantArmor),
            effect(EquipmentAlteration::ProtectArmor),
        ],
    )
    .unwrap();
    let state = game.items[&armor].condition;
    assert_eq!(state.armor_enchantment, 2);
    assert!(state.protected);
    assert!(!state.cursed);
}
#[test]
fn remove_curse_only_touches_equipped_instances_and_keeps_penalty_and_rust() {
    let mut game = fixture();
    let worn = armor(&mut game, 20, true);
    let carried = armor(&mut game, 21, false);
    for id in [worn, carried] {
        game.items
            .edit(id, |i| {
                i.condition.cursed = true;
                i.condition.armor_penalty = 1;
                i.condition.corrosion = 1;
            })
            .unwrap();
    }
    game.apply_effects(ActorId(1), &[effect(EquipmentAlteration::RemoveCurse)])
        .unwrap();
    assert!(!game.items[&worn].condition.cursed);
    assert!(game.items[&carried].condition.cursed);
    for id in [worn, carried] {
        assert_eq!(game.items[&id].condition.armor_penalty, 1);
        assert_eq!(game.items[&id].condition.corrosion, 1);
    }
}
#[test]
fn frozen_empty_and_stale_equipment_selection_cannot_retarget() {
    let mut game = fixture();
    let Effect::AlterEquipment {
        selection,
        alteration,
    } = effect(EquipmentAlteration::EnchantArmor)
    else {
        unreachable!()
    };
    let empty = game
        .compile_equipment_alteration(ActorId(1), &selection, alteration)
        .unwrap();
    let worn = armor(&mut game, 20, true);
    let mut outcome = game.actor_outcome(ActorId(1)).unwrap();
    assert!(!game
        .alter_equipment_in_outcome(
            &mut outcome,
            &crate::ResolvedEquipmentAlteration {
                plan: empty,
                hit: false
            }
        )
        .unwrap());
    assert_eq!(game.items[&worn].condition.armor_enchantment, 0);
    let plan = game
        .compile_equipment_alteration(ActorId(1), &selection, alteration)
        .unwrap();
    let encoded = serde_json::to_value(&plan).unwrap();
    let plan = serde_json::from_value(encoded).unwrap();
    game.items
        .edit(worn, |i| i.condition.protected = true)
        .unwrap();
    let effects = [super::ResolvedEffect::AlterEquipment {
        alteration: crate::ResolvedEquipmentAlteration { plan, hit: false },
    }];
    assert_eq!(
        game.validate_equipment_alterations(ActorId(1), &effects),
        Err(GameError::ItemUnavailable)
    );
}
#[test]
fn frozen_equipment_witness_must_match_authored_slot_class_and_capability() {
    let mut game = fixture();
    let worn = armor(&mut game, 20, true);
    let blade = weapon(&mut game, 21);
    let Effect::AlterEquipment {
        selection,
        alteration,
    } = effect(EquipmentAlteration::EnchantArmor)
    else {
        unreachable!()
    };
    let mut plan = game
        .compile_equipment_alteration(ActorId(1), &selection, alteration)
        .unwrap();
    assert_eq!(plan.targets[0].item, worn);
    // The replacement is a real, correctly located equipped item with an exact
    // condition witness, but it does not satisfy the authored armor selector.
    plan.targets[0].item = blade;
    plan.targets[0].socket = *game.actors[&ActorId(1)]
        .equipment
        .iter()
        .find(|(_, item)| **item == blade)
        .unwrap()
        .0;
    plan.targets[0].condition = game.items[&blade].condition;
    assert!(plan.valid(&game.content.rules));
    let encoded = serde_json::to_value(&plan).unwrap();
    let plan = serde_json::from_value(encoded).unwrap();
    let effects = [super::ResolvedEffect::AlterEquipment {
        alteration: crate::ResolvedEquipmentAlteration { plan, hit: false },
    }];
    let before = game.clone();
    assert_eq!(
        game.validate_equipment_alterations(ActorId(1), &effects),
        Err(GameError::ItemUnavailable)
    );
    assert_eq!(game, before);
}

#[test]
fn paid_read_overflow_rolls_back_prior_weapon_draw_equipment_and_payment() {
    let mut game = fixture();
    let worn = armor(&mut game, 20, true);
    let blade = weapon(&mut game, 21);
    game.items
        .edit(worn, |item| {
            item.condition.armor_enchantment = crate::limits::NUMERIC as u32 - 2;
        })
        .unwrap();
    let sources = game.actors[&ActorId(1)]
        .equipment
        .iter()
        .map(|(socket, id)| {
            game.items[id]
                .equipment_source(*id, *socket, &game.content.rules)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let context = game
        .equipment_recovery_context(ActorId(1), &sources)
        .unwrap();
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .set_equipment_in_context(sources, context)
        .unwrap();
    let page = scroll(
        &mut game,
        30,
        vec![
            Effect::AlterEquipment {
                selection: EquippedSelection::Slot {
                    slot: EquipmentSlot::new("weapon").unwrap(),
                    class: ItemClass::Weapon,
                    last: true,
                },
                alteration: EquipmentAlteration::EnchantWeapon,
            },
            effect(EquipmentAlteration::EnchantArmor),
        ],
    );
    assert!(
        game.items[&worn]
            .condition
            .enchant_armor(
                game.items[&worn]
                    .spec
                    .equipment
                    .as_ref()
                    .unwrap()
                    .armor
                    .unwrap()
            )
            .is_none(),
        "the later effect specifically exceeds the checked armor bound"
    );
    let intention = game
        .admit_intention(
            ActorId(1),
            Action::Read { item: page },
            IntentionOrigin::Human,
        )
        .unwrap();
    let before = game.clone();
    assert_eq!(
        game.act_with_intention(ActorId(1), Action::Read { item: page }, Some(intention)),
        Err(GameError::Rule(crate::abilities::RuleError::InvalidPlan))
    );
    assert_eq!(game, before);
    assert_eq!(game.rules_rng, before.rules_rng);
    assert_eq!(game.items[&page].quantity, 1);
    assert_eq!(game.items[&blade].condition, before.items[&blade].condition);
    // The same paid operation succeeds when the later armor bound permits it,
    // proving the fixture reaches the earlier random weapon effect and payment.
    game.items
        .edit(worn, |item| item.condition.armor_enchantment -= 1)
        .unwrap();
    let sources = game.actors[&ActorId(1)]
        .equipment
        .iter()
        .map(|(socket, id)| {
            game.items[id]
                .equipment_source(*id, *socket, &game.content.rules)
                .unwrap()
        })
        .collect::<Vec<_>>();
    let context = game
        .equipment_recovery_context(ActorId(1), &sources)
        .unwrap();
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .combat
        .as_mut()
        .unwrap()
        .set_equipment_in_context(sources, context)
        .unwrap();
    let mut expected_rng = game.rules_rng;
    let hit = crate::dice::random_word(&mut expected_rng) & 1 == 0;
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(game.rules_rng, expected_rng);
    assert!(!game.items.contains_key(&page));
    assert_eq!(game.items[&blade].condition.hit_enchantment, i32::from(hit));
    assert_eq!(
        game.items[&blade].condition.damage_enchantment,
        i32::from(!hit)
    );
}

#[test]
fn later_failure_rolls_back_equipment_sheet_and_rng() {
    let mut game = fixture();
    let worn = armor(&mut game, 20, true);
    let before = game.clone();
    let result = game.with_recovery_transition(true, |candidate| {
        candidate.apply_effects(ActorId(1), &[effect(EquipmentAlteration::EnchantArmor)])?;
        assert_eq!(candidate.items[&worn].condition.armor_enchantment, 1);
        Err::<(), _>(GameError::IdentityExhausted)
    });
    assert_eq!(result, Err(GameError::IdentityExhausted));
    assert_eq!(game, before);
}

#[test]
fn instance_magic_capability_changes_without_mutating_shared_definition() {
    let mut game = fixture();
    let worn = armor(&mut game, 20, true);
    let sibling = armor(&mut game, 21, false);
    assert!(!game.items[&worn].magical());
    game.apply_effects(ActorId(1), &[effect(EquipmentAlteration::EnchantArmor)])
        .unwrap();
    assert!(game.items[&worn].magical());
    assert!(!game.items[&sibling].magical());
    assert!(!game.items[&worn].spec.magical);
    assert_eq!(game.items[&worn].spec, game.items[&sibling].spec);
}

fn weapon(game: &mut Game, id: u64) -> ItemId {
    let actor = ActorId(1);
    let mut spec = ItemSpec::ordinary("test blade".into());
    spec.class = ItemClass::Weapon;
    spec.equipment = Some(EquipmentSpec {
        periodic_relocation: None,
        potency_modifiers: Vec::new(),
        weapon_profile: None,
        slot: EquipmentSlot::new("weapon").unwrap(),
        armor: None,
        attack: Some(
            crate::attacks::MeleeAttack::fixed(
                crate::attributes::Skill::HeavyWeaponry,
                0,
                60,
                40,
                crate::combat::DamageType::Keen,
                None,
                2,
            )
            .unwrap(),
        ),
        modifiers: vec![],
    });
    let item = game
        .place_item_stack(id, game.actors[&actor].location, Some(actor), 1, spec)
        .unwrap();
    let slot = game.actors[&actor]
        .anatomy
        .slots
        .iter()
        .position(|s| s.as_str() == "weapon")
        .unwrap();
    game.equip_authored(actor, item, EquipmentSlotId(slot as u16))
        .unwrap();
    item
}
#[test]
fn weapon_enchantment_draws_once_only_for_eligible_weapon_and_uncurses() {
    let operation = Effect::AlterEquipment {
        selection: EquippedSelection::Slot {
            slot: EquipmentSlot::new("weapon").unwrap(),
            class: ItemClass::Weapon,
            last: true,
        },
        alteration: EquipmentAlteration::EnchantWeapon,
    };
    let mut branches = std::collections::BTreeSet::new();
    for seed in 1..=32 {
        let mut game = fixture();
        game.rules_rng = seed;
        game.apply_effects(ActorId(1), std::slice::from_ref(&operation))
            .unwrap();
        assert_eq!(game.rules_rng, seed);
        let item = weapon(&mut game, 20);
        game.items
            .edit(item, |i| i.condition.cursed = true)
            .unwrap();
        let mut expected = game.rules_rng;
        let hit = crate::dice::random_word(&mut expected) & 1 == 0;
        game.apply_effects(ActorId(1), std::slice::from_ref(&operation))
            .unwrap();
        let condition = game.items[&item].condition;
        assert!(!condition.cursed);
        assert_eq!(condition.hit_enchantment, i32::from(hit));
        assert_eq!(condition.damage_enchantment, i32::from(!hit));
        assert_eq!(game.rules_rng, expected);
        branches.insert(hit);
    }
    assert_eq!(branches.len(), 2);
}
#[test]
fn rust_leaves_ward_and_psychic_damage_bypasses_it_in_actual_actor_effects() {
    let mut game = fixture();
    let item = armor(&mut game, 20, true);
    game.apply_effects(
        ActorId(1),
        &[
            effect(EquipmentAlteration::EnchantArmor),
            effect(EquipmentAlteration::EnchantArmor),
        ],
    )
    .unwrap();
    game.apply_effects(
        ActorId(1),
        &[Effect::CorrodeArmor {
            slot: EquipmentSlot::new("body_armor").unwrap(),
            amount: 100,
        }],
    )
    .unwrap();
    assert_eq!(game.items[&item].condition.corrosion, 2);
    assert_eq!(game.items[&item].condition.armor_enchantment, 2);
    let damage = |category| Effect::Damage {
        damage: crate::rules::DamageExpr::Components {
            components: vec![crate::rules::DamageComponentExpr {
                category,
                descriptor: None,
                amount: crate::rules::AmountExpr::Fixed { amount: 3 },
            }],
        },
    };
    let before = game.health(ActorId(1)).unwrap().0;
    game.apply_effects(ActorId(1), &[damage(crate::combat::DamageType::Energy)])
        .unwrap();
    assert_eq!(game.health(ActorId(1)).unwrap().0, before - 1);
    game.apply_effects(ActorId(1), &[damage(crate::combat::DamageType::Psychic)])
        .unwrap();
    assert_eq!(game.health(ActorId(1)).unwrap().0, before - 4);
}
