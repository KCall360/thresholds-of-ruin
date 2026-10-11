use std::{collections::BTreeMap, num::NonZeroU64};
use tor_simulation::content_ids::TemplateId;
use tor_simulation::rules::{
    AmountExpr, DefenseKind, Effect, Modifier, ModifierOp, Scope, Selector, Stat,
};
use tor_simulation::EquipmentSlot;
use tor_simulation::{
    checkpoint::SharedState, combat::DamageType, Action, ActorId, AnatomySpec, ConsumableSpec,
    EquipmentSlotId, EquipmentSpec, Game, IntentionOrigin, ItemClass, ItemId, ItemSpec,
};
use tor_world::{Location, Position, RegionId};

fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn fixture(combat: bool) -> (Game, ActorId, ActorId) {
    fixture_with_anatomy(
        combat,
        AnatomySpec {
            slots: vec![
                EquipmentSlot::BodyArmor,
                EquipmentSlot::Ring,
                EquipmentSlot::Ring,
            ],
        },
    )
}
fn fixture_with_anatomy(combat: bool, anatomy: AnatomySpec) -> (Game, ActorId, ActorId) {
    fixture_with_rules(combat, anatomy, super::creature_fixture::ruleset())
}
fn fixture_with_rules(
    combat: bool,
    anatomy: AnatomySpec,
    rules: std::sync::Arc<tor_simulation::rules::Ruleset>,
) -> (Game, ActorId, ActorId) {
    let mut species = super::creature_fixture::species();
    species.anatomy = anatomy.clone();
    let mut ward =
        tor_simulation::creatures::Template::new(TemplateId::new("vital_immunity").unwrap(), 0);
    ward.modifiers = vec![Modifier {
        scope: Scope::Always,
        op: ModifierOp::Immunity {
            selector: Selector::Category {
                category: DamageType::Vital,
            },
        },
    }];
    let content = tor_simulation::creatures::Content::new(
        rules,
        tor_simulation::creatures::ScenarioCatalog {
            species: BTreeMap::from([(species.id.clone(), species.clone())]),
            templates: BTreeMap::from([(ward.id.clone(), ward)]),
        },
    )
    .unwrap();
    let mut game = Game::two_room(42, content);
    let first = game
        .spawn_actor(at(1), NonZeroU64::new(100).unwrap())
        .unwrap();
    let second = game
        .spawn_actor(at(2), NonZeroU64::new(50).unwrap())
        .unwrap();
    if combat {
        let build = tor_simulation::creatures::CreatureBuild::new(
            game.content().clone(),
            species.id,
            tor_simulation::progression::HdLedger::seeded(
                vec![tor_simulation::progression::HdSource::Racial],
                42,
            )
            .unwrap(),
            tor_simulation::attributes::ManaBinding::Intellect,
        )
        .unwrap();
        game.configure_creature(
            first,
            tor_simulation::CreatureIdentity {
                name: "figure".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
    } else {
        game.configure_anatomy(first, anatomy).unwrap();
    }
    game.refresh_navigation();
    (game, first, second)
}
fn armor(game: &mut Game, actor: ActorId) {
    let mut spec = ItemSpec::ordinary("mail".into());
    spec.class = ItemClass::Armor;
    spec.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::BodyArmor,
        attack: None,
        modifiers: defense(3),
    });
    game.place_item_stack(10, at(1), Some(actor), 1, spec)
        .unwrap();
}
fn finish(game: &mut Game, actor: ActorId) {
    while game.preparation(actor).is_some() {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
}

#[test]
fn noncombat_armor_uses_anatomy_and_takes_three_turns_to_complete() {
    let (mut game, actor, observer) = fixture(false);
    armor(&mut game, actor);
    let id = game
        .admit_intention(
            actor,
            Action::Equip {
                item: ItemId(10),
                slot: EquipmentSlotId(0),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert!(game.equipment(actor).unwrap().is_empty());
    assert_eq!(game.preparation(actor).unwrap().intention, Some(id));
    let view = game.observe(actor).unwrap();
    assert!(view.combat.is_none());
    let interaction = view.interactions.unwrap();
    assert_eq!(interaction.preparation.unwrap().remaining, 300);
    assert_eq!(interaction.inventory[0].equipped_slot, None);
    finish(&mut game, actor);
    assert_eq!(
        game.observe(actor).unwrap().interactions.unwrap().completed,
        vec![tor_simulation::Work::Equip {
            item: ItemId(10),
            slot: EquipmentSlotId(0)
        }]
    );
    assert!(game
        .observe(observer)
        .unwrap()
        .interactions
        .is_none_or(|view| view.completed.is_empty()));
    assert_eq!(game.tick(), 300);
    assert_eq!(
        game.equipment(actor).unwrap()[&EquipmentSlotId(0)],
        ItemId(10)
    );
    assert!(game
        .admit_intention(
            actor,
            Action::Drop {
                item: ItemId(10),
                quantity: None
            },
            IntentionOrigin::Human
        )
        .is_err());
    game.admit_intention(
        actor,
        Action::Unequip { item: ItemId(10) },
        IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    finish(&mut game, actor);
    assert!(game.equipment(actor).unwrap().is_empty());
    assert!(game.observe(actor).unwrap().combat.is_none());
    assert_eq!(game.tick(), 600);
}

#[test]
fn damage_pauses_item_work_and_saved_resume_keeps_original_intention() {
    let (mut game, actor, _) = fixture(true);
    armor(&mut game, actor);
    let id = game
        .admit_intention(
            actor,
            Action::Equip {
                item: ItemId(10),
                slot: EquipmentSlotId(0),
            },
            IntentionOrigin::Human,
        )
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    game.apply_effects(actor, &[super::abilities::damage(1, DamageType::Vital)])
        .unwrap();
    let progress = game.preparation(actor).unwrap().clone();
    assert!(!progress.active);
    assert_eq!(
        game.creature(actor).unwrap().derived().defenses.physical,
        10
    );
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot = serde_json::from_value(serde_json::to_value(snapshot).unwrap()).unwrap();
    let shared = serde_json::from_value(serde_json::to_value(shared).unwrap()).unwrap();
    let mut restored = Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
    assert_eq!(restored.preparation(actor), Some(&progress));
    assert!(restored.cancel_intention(actor, id).is_err());
    restored.resume_intention(actor, id).unwrap();
    // Let the independently due actor run before the queued continuation.
    while restored.next_intention_actor() != Some(actor) {
        restored
            .act(restored.next_actor().unwrap(), Action::Wait)
            .unwrap();
    }
    restored.execute_next_intention().unwrap().outcome.unwrap();
    finish(&mut restored, actor);
    assert_eq!(
        restored
            .creature(actor)
            .unwrap()
            .derived()
            .defenses
            .physical,
        13
    );
}

#[test]
fn duplicate_ring_slots_are_distinct_and_a_slot_must_match_the_item() {
    let (mut game, actor, _) = fixture(false);
    let mut spec = ItemSpec::ordinary("ring".into());
    spec.class = ItemClass::Ring;
    spec.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::Ring,
        attack: None,
        modifiers: defense(0),
    });
    for id in [11, 12] {
        game.place_item_stack(id, at(1), Some(actor), 1, spec.clone())
            .unwrap();
    }
    game.equip_authored(actor, ItemId(11), EquipmentSlotId(1))
        .unwrap();
    game.equip_authored(actor, ItemId(12), EquipmentSlotId(2))
        .unwrap();
    assert_eq!(game.equipment(actor).unwrap().len(), 2);
    assert!(game
        .equip_authored(actor, ItemId(12), EquipmentSlotId(0))
        .is_err());
}

#[test]
fn drinking_consumes_at_completion_and_identifies_only_observable_effects() {
    let (mut game, actor, _) = fixture(true);
    let mut spec = ItemSpec::ordinary("healing".into());
    spec.class = ItemClass::Potion;
    spec.concealed = true;
    spec.appearance = "red vial".into();
    spec.stackable = true;
    spec.consumable = Some(ConsumableSpec {
        effects: vec![Effect::Heal {
            amount: AmountExpr::Fixed { amount: 10 },
        }],
    });
    game.place_item_stack(20, at(1), Some(actor), 2, spec)
        .unwrap();
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    assert_eq!(game.observe(actor).unwrap().inventory[0].quantity, 2);
    finish(&mut game, actor);
    let view = game.observe(actor).unwrap();
    assert_eq!(view.inventory[0].quantity, 1);
    assert!(!view.inventory[0].identified);
    game.apply_effects(actor, &[super::abilities::damage(5, DamageType::Vital)])
        .unwrap();
    while game.next_actor() != Some(actor) {
        game.act(game.next_actor().unwrap(), Action::Wait).unwrap();
    }
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    finish(&mut game, actor);
    assert!(game.observe(actor).unwrap().inventory.is_empty());
    assert_eq!(
        game.observe(actor).unwrap().interactions.unwrap().completed,
        vec![tor_simulation::Work::Drink { item: ItemId(20) }]
    );
    assert_eq!(game.health(actor).unwrap().0, 30);
    let mut replacement = ItemSpec::ordinary("healing".into());
    replacement.class = ItemClass::Potion;
    replacement.concealed = true;
    replacement.appearance = "red vial".into();
    game.place_item_stack(21, at(1), Some(actor), 1, replacement)
        .unwrap();
    assert!(game.observe(actor).unwrap().inventory[0].identified);
}

#[test]
fn consumed_objective_remains_unmet_and_its_checkpoint_is_valid() {
    let (mut game, actor, _) = fixture(true);
    let mut spec = ItemSpec::ordinary("objective potion".into());
    spec.class = ItemClass::Potion;
    spec.consumable = Some(ConsumableSpec {
        effects: vec![Effect::Heal {
            amount: AmountExpr::Fixed { amount: 1 },
        }],
    });
    game.place_item_stack(20, at(1), Some(actor), 1, spec)
        .unwrap();
    game.configure_run(
        actor,
        std::collections::BTreeSet::from([actor]),
        Some(tor_simulation::combat::Objective {
            anchor: at(4),
            item: Some(ItemId(20)),
            continue_play: false,
            disclosed: true,
        }),
        BTreeMap::new(),
    )
    .unwrap();
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    finish(&mut game, actor);
    assert!(game.observe(actor).unwrap().inventory.is_empty());
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    assert!(Game::restore_checkpoint(snapshot, &shared, game.content().clone()).is_ok());
    assert!(!game.observe(actor).unwrap().combat.unwrap().victory);
}

#[test]
fn invalid_effect_sequence_is_atomic_and_lethal_sequence_never_revives() {
    let (mut game, actor, _) = fixture(true);
    let before = game.clone();
    assert!(game
        .apply_effects(
            actor,
            &[
                super::abilities::damage(5, DamageType::Vital),
                Effect::ApplyCondition {
                    condition: tor_simulation::content_ids::ConditionId::new("missing").unwrap(),
                    duration: 1
                },
            ]
        )
        .is_err());
    assert_eq!(game, before);
    game.apply_effects(
        actor,
        &[
            super::abilities::damage(100, DamageType::Vital),
            Effect::Heal {
                amount: AmountExpr::Fixed { amount: 100 },
            },
        ],
    )
    .unwrap();
    assert_eq!(game.health(actor).unwrap().0, 0);
    assert!(!game.alive(actor));
}

#[test]
fn lethal_final_potion_reports_completion_without_anatomy_or_remaining_inventory() {
    let (mut game, actor, observer) = fixture_with_anatomy(true, AnatomySpec::default());
    let mut potion = ItemSpec::ordinary("poison".into());
    potion.class = ItemClass::Potion;
    potion.concealed = true;
    potion.appearance = "red potion".into();
    potion.consumable = Some(ConsumableSpec {
        effects: vec![super::abilities::damage(100, DamageType::Vital)],
    });
    game.place_item_stack(20, at(1), Some(actor), 1, potion)
        .unwrap();
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    finish(&mut game, actor);
    assert!(!game.alive(actor));
    let view = game.observe(actor).unwrap();
    assert!(view.inventory.is_empty());
    assert_eq!(
        view.interactions.unwrap().completed,
        vec![tor_simulation::Work::Drink { item: ItemId(20) }]
    );
    assert!(game
        .observe(observer)
        .unwrap()
        .interactions
        .is_none_or(|view| view.completed.is_empty()));
}

#[test]
fn armor_protects_until_removal_finishes_and_death_drops_it_once() {
    let (mut game, actor, witness) = fixture(true);
    armor(&mut game, actor);
    game.equip_authored(actor, ItemId(10), EquipmentSlotId(0))
        .unwrap();
    game.act(actor, Action::Unequip { item: ItemId(10) })
        .unwrap();
    assert_eq!(
        game.creature(actor).unwrap().derived().defenses.physical,
        13
    );
    finish(&mut game, actor);
    assert_eq!(
        game.creature(actor).unwrap().derived().defenses.physical,
        10
    );
    game.equip_authored(actor, ItemId(10), EquipmentSlotId(0))
        .unwrap();
    game.apply_effects(actor, &[super::abilities::damage(100, DamageType::Vital)])
        .unwrap();
    assert!(game.equipment(actor).unwrap().is_empty());
    let view = game.observe(witness).unwrap();
    assert_eq!(
        view.ground_items
            .iter()
            .filter(|item| item.id == ItemId(10))
            .count(),
        1
    );
    assert_eq!(
        view.ground_items
            .iter()
            .filter(|item| item.class == ItemClass::Corpse)
            .count(),
        1
    );
}

#[test]
fn concealed_equipment_discloses_its_slot_but_stats_require_actor_knowledge() {
    let (mut game, actor, witness) = fixture(true);
    let mut spec = ItemSpec::ordinary("enchanted ring".into());
    spec.class = ItemClass::Ring;
    spec.concealed = true;
    spec.appearance = "silver ring".into();
    spec.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::Ring,
        attack: None,
        modifiers: defense(3),
    });
    game.place_item_stack(30, at(1), Some(actor), 1, spec)
        .unwrap();
    let interaction = game.observe(actor).unwrap().interactions.unwrap();
    assert_eq!(interaction.inventory[0].slot, Some(EquipmentSlot::Ring));
    assert!(interaction.inventory[0].known_equipment.is_none());
    assert!(game.observe(witness).unwrap().interactions.is_none());
    game.learn_identity(actor, "enchanted ring").unwrap();
    let interaction = game.observe(actor).unwrap().interactions.unwrap();
    assert_eq!(
        interaction.inventory[0]
            .known_equipment
            .as_ref()
            .unwrap()
            .modifiers,
        defense(3)
    );
}

#[test]
fn newly_visible_hostility_pauses_work_but_resume_accepts_the_existing_threat() {
    let (mut game, actor, other) = fixture(true);
    game.configure_run(
        actor,
        std::collections::BTreeSet::from([actor]),
        None,
        BTreeMap::from([(
            "neutral".into(),
            std::collections::BTreeSet::from(["enemy".into()]),
        )]),
    )
    .unwrap();
    armor(&mut game, actor);
    game.act(
        actor,
        Action::Equip {
            item: ItemId(10),
            slot: EquipmentSlotId(0),
        },
    )
    .unwrap();
    game.configure_creature(
        other,
        tor_simulation::CreatureIdentity {
            name: "enemy".into(),
            faction: "enemy".into(),
        },
        game.creature(actor).unwrap().build().clone(),
    )
    .unwrap();
    game.act(other, Action::Wait).unwrap();
    assert!(!game.preparation(actor).unwrap().active);
    assert!(game.equipment(actor).unwrap().is_empty());
    game.act(
        actor,
        Action::Equip {
            item: ItemId(10),
            slot: EquipmentSlotId(0),
        },
    )
    .unwrap();
    assert!(game.preparation(actor).unwrap().active);
    finish(&mut game, actor);
    assert_eq!(game.equipment(actor).unwrap().len(), 1);
}

#[test]
fn effective_equipment_changes_attack_and_typed_damage_without_mutating_base_rules() {
    let (mut game, actor, _) = fixture_with_anatomy(
        true,
        AnatomySpec {
            slots: vec![EquipmentSlot::Weapon, EquipmentSlot::BodyArmor],
        },
    );
    let natural_attack = game.selected_melee_attack(actor).unwrap().clone();
    let mut weapon = ItemSpec::ordinary("sword".into());
    weapon.class = ItemClass::Weapon;
    let attack = tor_simulation::attacks::MeleeAttack::fixed(
        tor_simulation::attributes::Skill::HeavyWeaponry,
        100,
        30,
        40,
        DamageType::Keen,
        None,
        10,
    )
    .unwrap();
    weapon.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::Weapon,
        attack: Some(attack.clone()),
        modifiers: defense(0),
    });
    game.place_item_stack(30, at(1), Some(actor), 1, weapon)
        .unwrap();
    game.equip_authored(actor, ItemId(30), EquipmentSlotId(0))
        .unwrap();
    assert_eq!(game.selected_melee_attack(actor), Some(&attack));
    let mut armor = ItemSpec::ordinary("armor".into());
    armor.class = ItemClass::Armor;
    armor.equipment = Some(EquipmentSpec {
        slot: EquipmentSlot::BodyArmor,
        attack: None,
        modifiers: {
            let mut modifiers = defense(3);
            modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Reduction {
                    selector: Selector::Category {
                        category: DamageType::Impact,
                    },
                    amount: 2,
                },
            });
            modifiers
        },
    });
    game.place_item_stack(31, at(1), Some(actor), 1, armor)
        .unwrap();
    game.equip_authored(actor, ItemId(31), EquipmentSlotId(1))
        .unwrap();
    game.apply_effects(actor, &[super::abilities::damage(5, DamageType::Impact)])
        .unwrap();
    assert_eq!(game.health(actor).unwrap().0, 27);
    let mut build = game.creature(actor).unwrap().build().clone();
    build
        .set_templates(vec![TemplateId::new("vital_immunity").unwrap()])
        .unwrap();
    game.rebuild_creature(actor, build).unwrap();
    assert_eq!(game.creature(actor).unwrap().health().injury(), 3);
    assert!(!game
        .apply_effects(actor, &[super::abilities::damage(5, DamageType::Vital)])
        .unwrap());
    assert_eq!(game.health(actor).unwrap().0, 27);
    game.act(actor, Action::Unequip { item: ItemId(30) })
        .unwrap();
    finish(&mut game, actor);
    assert_eq!(game.selected_melee_attack(actor), Some(&natural_attack));
    assert_eq!(
        game.creature(actor).unwrap().derived().defenses.physical,
        13
    );
}

fn defense(value: i64) -> Vec<Modifier> {
    vec![Modifier {
        scope: Scope::Always,
        op: ModifierOp::Add {
            stat: Stat::Defense {
                defense: DefenseKind::Physical,
            },
            value,
        },
    }]
}

#[test]
fn saved_item_preparation_rejects_missing_ground_and_foreign_items() {
    let (mut game, actor, other) = fixture(true);
    let mut potion = ItemSpec::ordinary("saved-potion".into());
    potion.class = ItemClass::Potion;
    potion.consumable = Some(ConsumableSpec {
        effects: vec![Effect::Heal {
            amount: AmountExpr::Fixed { amount: 1 },
        }],
    });
    for (id, owner) in [(20, Some(actor)), (21, None), (22, Some(other))] {
        game.place_item_stack(id, at(1), owner, 1, potion.clone())
            .unwrap();
    }
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    assert!(game.preparation(actor).is_some());
    let mut shared = SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    assert!(Game::restore_checkpoint(
        serde_json::from_value(snapshot.clone()).unwrap(),
        &shared,
        game.content().clone()
    )
    .is_ok());
    fn edit_pending(
        value: &mut serde_json::Value,
        edit: &mut impl FnMut(&mut serde_json::Value),
    ) -> usize {
        match value {
            serde_json::Value::Object(fields) => {
                if let Some(pending) = fields
                    .get_mut("pending")
                    .filter(|pending| !pending.is_null())
                {
                    edit(pending);
                    1
                } else {
                    fields
                        .values_mut()
                        .map(|value| edit_pending(value, edit))
                        .sum()
                }
            }
            serde_json::Value::Array(values) => values
                .iter_mut()
                .map(|value| edit_pending(value, edit))
                .sum(),
            _ => 0,
        }
    }
    for item in [999, 21, 22] {
        let mut encoded = serde_json::to_value(&shared).unwrap();
        assert_eq!(
            edit_pending(&mut encoded, &mut |pending| pending["plan"]["work"]
                ["item"] =
                serde_json::json!(item)),
            1
        );
        let corrupt = serde_json::from_value(encoded).unwrap();
        assert!(
            Game::restore_checkpoint(
                serde_json::from_value(snapshot.clone()).unwrap(),
                &corrupt,
                game.content().clone()
            )
            .is_err(),
            "accepted item {item}"
        );
    }
    for (field, value) in [
        (
            "cost",
            serde_json::json!({"resource":"mana", "start":1, "resolution":1}),
        ),
        ("edge", serde_json::json!(1)),
        ("reach", serde_json::json!({"kind":"melee"})),
        (
            "on_success",
            serde_json::json!([{"kind":"heal", "amount":{"kind":"fixed", "amount":2}}]),
        ),
    ] {
        let mut encoded = serde_json::to_value(&shared).unwrap();
        assert_eq!(
            edit_pending(&mut encoded, &mut |pending| pending["plan"]["resolution"]
                [field] = value.clone()),
            1
        );
        let corrupt = serde_json::from_value(encoded).unwrap();
        assert!(
            Game::restore_checkpoint(
                serde_json::from_value(snapshot.clone()).unwrap(),
                &corrupt,
                game.content().clone()
            )
            .is_err(),
            "accepted altered {field}"
        );
    }
}

#[test]
fn rules_data_controls_equip_unequip_and_consumable_plan_timing() {
    use tor_simulation::rules::{EquipmentTiming, ItemTiming, Ruleset};
    let mut document = super::creature_fixture::ruleset().document().clone();
    document.constants.item_timing = ItemTiming {
        equip: EquipmentTiming {
            standard_turns: 2,
            body_armor_turns: 4,
        },
        unequip: EquipmentTiming {
            standard_turns: 5,
            body_armor_turns: 6,
        },
        consume_turns: 7,
    };
    let rules = Ruleset::validate(document).unwrap();
    for (slot, remove, turns) in [
        (EquipmentSlot::Ring, false, 2),
        (EquipmentSlot::BodyArmor, false, 4),
        (EquipmentSlot::Ring, true, 5),
        (EquipmentSlot::BodyArmor, true, 6),
    ] {
        let (mut game, actor, _) =
            fixture_with_rules(true, AnatomySpec { slots: vec![slot] }, rules.clone());
        let mut item = ItemSpec::ordinary("gear".into());
        item.class = if slot == EquipmentSlot::Ring {
            ItemClass::Ring
        } else {
            ItemClass::Armor
        };
        item.equipment = Some(EquipmentSpec {
            slot,
            attack: None,
            modifiers: vec![],
        });
        game.place_item_stack(20, at(1), Some(actor), 1, item)
            .unwrap();
        let action = if remove {
            game.equip_authored(actor, ItemId(20), EquipmentSlotId(0))
                .unwrap();
            Action::Unequip { item: ItemId(20) }
        } else {
            Action::Equip {
                item: ItemId(20),
                slot: EquipmentSlotId(0),
            }
        };
        game.act(actor, action).unwrap();
        assert_eq!(
            game.preparation(actor).unwrap().plan.preparation,
            100 * turns
        );
    }
    let (mut game, actor, _) = fixture_with_rules(true, AnatomySpec::humanoid(), rules);
    let mut potion = ItemSpec::ordinary("potion".into());
    potion.class = ItemClass::Potion;
    potion.consumable = Some(ConsumableSpec {
        effects: vec![Effect::Heal {
            amount: AmountExpr::Fixed { amount: 1 },
        }],
    });
    game.place_item_stack(20, at(1), Some(actor), 1, potion)
        .unwrap();
    game.act(actor, Action::Drink { item: ItemId(20) }).unwrap();
    assert_eq!(game.preparation(actor).unwrap().plan.preparation, 700);
}
#[test]
fn item_timing_product_cannot_create_an_unrestorable_plan() {
    let mut document = super::creature_fixture::ruleset().document().clone();
    document.constants.item_timing.equip.body_armor_turns = tor_simulation::limits::NUMERIC as u64;
    let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
    let (mut game, actor, _) = fixture_with_rules(true, AnatomySpec::humanoid(), rules);
    armor(&mut game, actor);
    let before = game.clone();
    assert!(game
        .act(
            actor,
            Action::Equip {
                item: ItemId(10),
                slot: EquipmentSlotId(2)
            }
        )
        .is_err());
    assert_eq!(game, before);
}
