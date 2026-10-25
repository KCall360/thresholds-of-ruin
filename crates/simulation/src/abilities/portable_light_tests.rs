//! Real paid item work; native optical and item invariants complement hosted
//! source-free residency fixtures, without claiming this native setup is hosted.
use crate::{Action, ActorId, EquipmentSlotId, Game, HeldLightSpec, IntentionOrigin, ItemId};
use tor_world::{Location, Position, RegionId};
const OWNER: ActorId = ActorId(1);
fn at(x: i32) -> Location {
    Location {
        region: RegionId(1),
        position: Position { x, y: 1, z: 0 },
    }
}
fn spec(fuel: u64, radius: u8) -> crate::ItemSpec {
    let mut spec = crate::ItemSpec::ordinary("ordinary torch".into());
    spec.class = crate::ItemClass::Tool;
    spec.held_light = Some(HeldLightSpec {
        radius,
        fuel_ticks: fuel,
    });
    spec.equipment = Some(crate::EquipmentSpec {
        slot: crate::content_ids::EquipmentSlot::new("held_light").unwrap(),
        periodic_relocation: None,
        armor: None,
        attack: None,
        weapon_profile: None,
        modifiers: vec![],
        potency_modifiers: vec![],
    });
    spec
}
fn fixture(fuel: u64, radius: u8) -> (Game, ItemId, EquipmentSlotId) {
    let mut game = super::ray_tests::fixture_with(|_, _, _| {});
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 100_000;
    game.world.set_region_light(RegionId(1), false).unwrap();
    let item = game
        .place_item_stack(30, at(1), Some(OWNER), 1, spec(fuel, radius))
        .unwrap();
    let slot = EquipmentSlotId(
        game.anatomy(OWNER)
            .unwrap()
            .slots
            .iter()
            .position(|slot| slot.as_str() == "held_light")
            .unwrap() as u16,
    );
    assert_eq!(game.items[&item].fuel.unwrap().remaining, fuel);
    assert!(!game.items[&item].fuel.unwrap().lit);
    (game, item, slot)
}
fn paid(game: &mut Game, action: Action, elapsed: u64) {
    let tick = game.tick();
    let rng = game.rules_rng;
    let id = game
        .admit_intention(OWNER, action, IntentionOrigin::Human)
        .unwrap();
    let done = game.execute_next_intention().unwrap();
    assert_eq!(done.intention.id, id);
    done.outcome.unwrap();
    assert_eq!(game.tick(), tick + elapsed);
    assert_eq!(game.rules_rng, rng);
}
fn equipped(fuel: u64, radius: u8) -> (Game, ItemId, EquipmentSlotId) {
    let (mut game, item, slot) = fixture(fuel, radius);
    game.equip_authored(OWNER, item, slot).unwrap();
    assert!(!game.items[&item].fuel.unwrap().lit);
    (game, item, slot)
}
#[test]
fn paid_torch_ignition_illuminates_without_ambient_mutation_and_wait_burns_exactly() {
    let (mut game, item, slot) = fixture(1000, 4);
    paid(&mut game, Action::Equip { item, slot }, 100);
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 1000);
    assert!(!game.items[&item].fuel.unwrap().lit);
    let world = game.world.clone();
    let absent = game.scene_snapshot(OWNER).unwrap().1;
    assert!(!absent.iter().any(|cell| cell.location == at(5)));
    paid(&mut game, Action::Ignite { item }, 100);
    assert!(game.items[&item].fuel.unwrap().lit);
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 1000);
    assert!(game.world.shares_storage(&world));
    assert_eq!(game.world.is_lit(at(5)), Some(false));
    assert!(game
        .scene_snapshot(OWNER)
        .unwrap()
        .1
        .iter()
        .any(|cell| cell.location == at(5)));
    paid(&mut game, Action::Wait, 100);
    assert_eq!(
        game.portable_light_view(OWNER, item).unwrap().remaining,
        900
    );
    paid(&mut game, Action::Extinguish { item }, 100);
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 800);
    assert!(!game.items[&item].fuel.unwrap().lit);
    assert!(
        !game.region_transition_pending(),
        "Resident never leaves host work"
    );
}
#[test]
fn torch_drop_preserves_flame_pickup_and_stow_extinguish_without_refill() {
    let (mut game, item, slot) = equipped(1000, 4);
    paid(&mut game, Action::Ignite { item }, 100);
    paid(
        &mut game,
        Action::Drop {
            item,
            quantity: None,
        },
        50,
    );
    assert_eq!(
        game.items[&item].location,
        crate::ItemLocation::Ground(at(1))
    );
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 950);
    assert!(game.items[&item].fuel.unwrap().lit);
    assert!(!game
        .equipment(OWNER)
        .unwrap()
        .values()
        .any(|held| *held == item));
    paid(
        &mut game,
        Action::Take {
            item,
            quantity: None,
        },
        50,
    );
    assert!(!game.items[&item].fuel.unwrap().lit);
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 950);
    paid(&mut game, Action::Equip { item, slot }, 100);
    paid(&mut game, Action::Ignite { item }, 100);
    paid(&mut game, Action::Unequip { item }, 100);
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 850);
    assert!(!game.items[&item].fuel.unwrap().lit);
}
#[test]
fn torch_cursed_drop_and_invalid_ignition_are_atomic() {
    let (mut game, item, _) = equipped(1000, 4);
    paid(&mut game, Action::Ignite { item }, 100);
    game.items
        .edit(item, |item| item.condition.cursed = true)
        .unwrap();
    let before = game.clone();
    assert!(game
        .act(
            OWNER,
            Action::Drop {
                item,
                quantity: None
            }
        )
        .is_err());
    assert_eq!(game, before);
    assert!(game.plan_work(OWNER, crate::Work::Ignite { item }).is_err());
    assert_eq!(game, before);
}
#[test]
fn torch_expiry_is_an_exact_physics_boundary_and_zero_fuel_cannot_reignite() {
    let (mut game, item, _) = equipped(7, 4);
    paid(&mut game, Action::Ignite { item }, 100);
    assert_eq!(game.advance_physics(1000).unwrap(), 107);
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 0);
    assert!(!game.items[&item].fuel.unwrap().lit);
    assert!(game.plan_work(OWNER, crate::Work::Ignite { item }).is_err());
}
#[test]
fn torch_managed_loaded_geometry_change_yields_once_before_time_continues() {
    let (mut game, item, _) = equipped(1000, 4);
    let id = game
        .admit_intention(OWNER, Action::Ignite { item }, IntentionOrigin::Human)
        .unwrap();
    let step = game.execute_next_intention_for_host().unwrap();
    let crate::CandidateStep::AwaitingRegions(done) = step else {
        panic!("changed loaded geometry needs same-tick settlement")
    };
    assert_eq!(done.intention.id, id);
    done.outcome.unwrap();
    assert_eq!(game.tick(), 100);
    let boundary = game.light_region_boundary().unwrap();
    assert!(game.light_missing_regions(boundary).is_empty());
    assert!(boundary.optical.is_some());
    game.complete_region_transition(&crate::TransitionReport::default())
        .unwrap();
    assert!(!game.region_transition_pending());
    assert_eq!(game.tick(), 100);
}

fn seed_map(game: &mut Game, observer: ActorId) {
    let (_, scene) = game.observe_scene(observer).unwrap();
    assert!(!scene.is_empty());
    let cells: Vec<_> = scene
        .iter()
        .map(|cell| crate::MapCellInput {
            location: cell.location,
            appearance: crate::MapAppearance {
                wall: game.world.is_wall(cell.location),
                material: "stone".into(),
                asset: None,
                place_hint: false,
                stairs_up: false,
                stairs_down: false,
                door: None,
                trap: None,
                ground_items: vec![],
            },
        })
        .collect();
    game.record_explored_observation(observer, &cells, &scene)
        .unwrap();
    assert!(!game.map_disclosure_changed_since(game, observer));
}
#[test]
fn torch_fuel_only_map_skip_keeps_real_fuel_observation() {
    let (mut game, item, _) = equipped(1000, 2);
    game.set_item_light(item, true).unwrap();
    seed_map(&mut game, ActorId(2));
    let before = game.clone();
    paid(&mut game, Action::Wait, 100);
    assert!(!game.items.shares_storage(&before.items));
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 900);
    assert!(!game.map_disclosure_changed_since(&before, ActorId(2)));
}
#[test]
fn torch_same_root_other_carrier_movement_invalidates_real_illumination() {
    let (mut game, item, _) = equipped(1000, 2);
    game.set_item_light(item, true).unwrap();
    seed_map(&mut game, ActorId(2));
    let moved_before = game.clone();
    let sight_before = game.scene_snapshot(ActorId(2)).unwrap().1;
    assert!(!sight_before.iter().any(|cell| cell.location == at(5)));
    assert!(game.world.walkable(at(4)));
    game.teleport(OWNER, at(4)).unwrap();
    assert_eq!(game.tick(), moved_before.tick());
    assert!(game.items.shares_storage(&moved_before.items));
    assert!(game
        .scene_snapshot(ActorId(2))
        .unwrap()
        .1
        .iter()
        .any(|cell| cell.location == at(5)));
    assert!(game.map_disclosure_changed_since(&moved_before, ActorId(2)));
}
#[test]
fn torch_map_skip_rejects_fuel_increase_extinction_and_nonfuel_changes() {
    let (mut base, item, _) = equipped(1000, 4);
    base.set_item_light(item, true).unwrap();
    paid(&mut base, Action::Wait, 100);
    seed_map(&mut base, ActorId(2));
    for mutation in 0..11 {
        let mut game = base.clone();
        game.items
            .edit(item, |entry| match mutation {
                0 => entry.fuel.as_mut().unwrap().remaining += 1,
                1 => entry.fuel.as_mut().unwrap().lit = false,
                2 => entry.orientation = 1,
                3 => entry.quantity = 2,
                4 => entry.condition.cursed = true,
                5 => entry.relocation_elapsed = Some(0),
                6 => entry.charges = Some(1),
                7 => entry.motion.velocity = [1, 0, 0],
                8 => entry.spec.name.push('x'),
                9 => {
                    entry.corpse = Some(crate::CorpseAppearance {
                        actor: OWNER,
                        asset: None,
                    })
                }
                10 => entry.holds.push(crate::supplies::ItemHold {
                    key: crate::supplies::ItemClaimKey {
                        lineage: crate::supplies::PreparationLineage {
                            actor: OWNER,
                            origin: crate::IntentionId(1),
                        },
                        cost_ordinal: 0,
                    },
                    quantity: 1,
                }),
                _ => unreachable!(),
            })
            .unwrap();
        assert!(
            game.map_disclosure_changed_since(&base, ActorId(2)),
            "mutation {mutation}"
        );
    }
}
#[test]
fn torch_strict_item_policy_and_runtime_fuel_reject_missing_and_invalid_shapes() {
    let (game, item, _) = equipped(1000, 4);
    let encoded = serde_json::to_value(&game.items[&item]).unwrap();
    let valid: crate::Item = serde_json::from_value(encoded.clone()).unwrap();
    assert!(game.item_state_valid(item, &valid));
    for field in ["fuel"] {
        let mut bad = encoded.clone();
        bad.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<crate::Item>(bad).is_err());
    }
    for fuel in [
        serde_json::json!({"remaining":0,"lit":true}),
        serde_json::json!([10, false]),
        serde_json::json!({"remaining":10,"lit":false,"extra":0}),
    ] {
        let mut bad = encoded.clone();
        bad["fuel"] = fuel;
        assert!(serde_json::from_value::<crate::Item>(bad).is_err());
    }
    let mut bad = encoded.clone();
    bad["fuel"] = serde_json::json!({"remaining":1001,"lit":false});
    let bad: crate::Item = serde_json::from_value(bad).unwrap();
    assert!(!game.item_state_valid(item, &bad));
    for policy in [
        serde_json::json!({"radius":0,"fuel_ticks":1000}),
        serde_json::json!({"radius":17,"fuel_ticks":1000}),
        serde_json::json!({"radius":4,"fuel_ticks":0}),
        serde_json::json!([4, 1000]),
    ] {
        assert!(serde_json::from_value::<HeldLightSpec>(policy).is_err());
    }
}
#[test]
fn torch_frozen_loaded_fuel_pauses_and_resumes_only_after_real_thaw() {
    let (mut game, item, _) = equipped(1000, 2);
    // Native setup establishes a lit item without creating controller input pins.
    game.set_item_light(item, true).unwrap();
    let mut records = crate::MemoryRecords::default();
    let (_, report) = game
        .transition_regions(
            &crate::RegionTransition {
                active: [RegionId(2)].into(),
                loaded: [RegionId(1), RegionId(2)].into(),
            },
            &mut records,
        )
        .unwrap();
    assert!(report.frozen.contains(&RegionId(1)));
    assert_eq!(
        game.region_state(RegionId(1)),
        Some(crate::RegionState::Frozen)
    );
    let clock = game.actor_clock(OWNER);
    let now = game.tick();
    game.advance_physics(now + 100).unwrap();
    assert_eq!(game.actor_clock(OWNER), clock);
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 1000);
    assert!(
        game.portable_lit_cells().contains(&at(2)),
        "frozen fuel still emits"
    );
    game.transition_regions(
        &crate::RegionTransition {
            active: [RegionId(1), RegionId(2)].into(),
            loaded: [RegionId(1), RegionId(2)].into(),
        },
        &mut records,
    )
    .unwrap();
    let now = game.tick();
    game.advance_physics(now + 100).unwrap();
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 900);
}
#[test]
fn torch_checkpoint_preserves_exact_fuel_and_expiry_after_restore() {
    let (mut game, item, _) = equipped(7, 2);
    paid(&mut game, Action::Ignite { item }, 100);
    // A genuine six-tick native actor turn produces a legal decision boundary
    // one tick before expiry, rather than saving behind the scheduler.
    game.actors.get_mut(&OWNER).unwrap().turn_ticks = std::num::NonZeroU64::new(6).unwrap();
    paid(&mut game, Action::Wait, 6);
    assert_eq!(game.tick(), 106);
    assert_eq!(game.actors[&OWNER].ready_at, 106);
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 1);
    game.seal_event_observations().unwrap();
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(snapshot, &shared)).unwrap();
    let (snapshot, shared): (crate::checkpoint::Snapshot, crate::checkpoint::SharedState) =
        serde_json::from_slice(&bytes).unwrap();
    let mut restore = crate::checkpoint::RestoreContext::new(&shared, game.content().clone());
    restore.bind_spatial_roots(&[&snapshot]).unwrap();
    let mut restored = restore.restore(snapshot).unwrap();
    assert_eq!(restored, game);
    assert_eq!(restored.advance_physics(1000).unwrap(), 107);
    assert_eq!(restored.items[&item].fuel.unwrap().remaining, 0);
    assert!(!restored.items[&item].fuel.unwrap().lit);
}
#[test]
fn torch_dropped_flame_is_independent_of_another_stowed_torch() {
    let (mut game, first, slot) = equipped(1000, 4);
    let second = game
        .place_item_stack(31, at(1), Some(OWNER), 1, spec(1000, 4))
        .unwrap();
    paid(&mut game, Action::Ignite { item: first }, 100);
    paid(
        &mut game,
        Action::Drop {
            item: first,
            quantity: None,
        },
        50,
    );
    paid(&mut game, Action::Equip { item: second, slot }, 100);
    paid(&mut game, Action::Ignite { item: second }, 100);
    paid(&mut game, Action::Unequip { item: second }, 100);
    assert_eq!(
        game.items[&first].location,
        crate::ItemLocation::Ground(at(1))
    );
    assert!(game.items[&first].fuel.unwrap().lit);
    assert_eq!(game.items[&first].fuel.unwrap().remaining, 650);
    assert!(!game.items[&second].fuel.unwrap().lit);
    assert_eq!(game.items[&second].fuel.unwrap().remaining, 900);
}
#[test]
fn torch_lethal_grip_removal_drops_remaining_inventory_once_and_preserves_torch() {
    use crate::rules::{Modifier, ModifierOp, Scope, Stat};
    let mut game = super::ray_tests::fixture_with(|_, _, species| {
        species.modifiers.retain(|modifier| {
            !matches!(
                modifier.op,
                ModifierOp::Set {
                    stat: Stat::Health,
                    ..
                }
            )
        });
    });
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = 50;
    let base = game.health(OWNER).unwrap();
    assert_eq!(base.0, base.1);
    assert!(base.0 > 0 && base.0 < 1000);
    let mut torch = spec(1000, 4);
    torch.equipment.as_mut().unwrap().modifiers.push(Modifier {
        cancellable: false,
        scope: Scope::Always,
        op: ModifierOp::Set {
            stat: Stat::Health,
            value: 1000,
        },
    });
    let item = game
        .place_item_stack(30, at(1), Some(OWNER), 1, torch)
        .unwrap();
    let spare = game
        .place_item_stack(
            31,
            at(1),
            Some(OWNER),
            1,
            crate::ItemSpec::ordinary("spare".into()),
        )
        .unwrap();
    let slot = EquipmentSlotId(
        game.anatomy(OWNER)
            .unwrap()
            .slots
            .iter()
            .position(|slot| slot.as_str() == "held_light")
            .unwrap() as u16,
    );
    game.equip_authored(OWNER, item, slot).unwrap();
    assert_eq!(game.health(OWNER).unwrap(), (1000, 1000));
    game.set_item_light(item, true).unwrap();
    let mut injury = game.actor_outcome(OWNER).unwrap();
    injury
        .state
        .combat
        .as_mut()
        .unwrap()
        .with_recovery_context(None, |creature| {
            creature.damage(999);
            Ok::<(), crate::GameError>(())
        })
        .unwrap();
    game.apply_outcome(injury).unwrap();
    assert_eq!(game.health(OWNER).unwrap().0, 1);
    let mut failed = game.clone();
    failed.next_item_id = u64::MAX;
    let before = failed.clone();
    assert!(failed
        .act(
            OWNER,
            Action::Drop {
                item,
                quantity: None
            }
        )
        .is_err());
    assert_eq!(failed, before);
    game.act(
        OWNER,
        Action::Drop {
            item,
            quantity: None,
        },
    )
    .unwrap();
    assert!(!game.alive(OWNER));
    assert_eq!(
        game.items[&item].location,
        crate::ItemLocation::Ground(at(1))
    );
    assert!(game.items[&item].fuel.unwrap().lit);
    assert_eq!(
        game.items[&spare].location,
        crate::ItemLocation::Ground(at(1))
    );
    assert_eq!(
        game.items
            .values()
            .filter(|item| item
                .corpse
                .as_ref()
                .is_some_and(|corpse| corpse.actor == OWNER))
            .count(),
        1
    );
    assert!(!game
        .items
        .values()
        .any(|item| item.location == crate::ItemLocation::Carried(OWNER)));
}
#[test]
fn torch_managed_observer_motion_with_stationary_ground_emitter_yields_same_tick() {
    let (mut game, item, _) = equipped(1000, 4);
    game.set_item_light(item, true).unwrap();
    game.act(
        OWNER,
        Action::Drop {
            item,
            quantity: None,
        },
    )
    .unwrap();
    let tick = game.tick();
    let emitters = game.emitter_signature();
    let step = game.teleport_for_host(ActorId(2), at(4)).unwrap();
    assert!(matches!(step, crate::CandidateStep::AwaitingRegions(())));
    assert_eq!(game.emitter_signature(), emitters);
    assert_eq!(game.tick(), tick);
    assert!(game
        .light_missing_regions(game.light_region_boundary().unwrap())
        .is_empty());
    game.complete_region_transition(&crate::TransitionReport::default())
        .unwrap();
    assert!(!game.region_transition_pending());
    assert_eq!(game.tick(), tick);
}
#[test]
fn torch_managed_door_change_with_stationary_emitter_yields_before_recovery_time() {
    let (mut game, item, _) = equipped(1000, 4);
    let door = game.place_door(at(2), false, 1).unwrap();
    game.set_item_light(item, true).unwrap();
    let emitters = game.emitter_signature();
    let tick = game.tick();
    let step = game
        .act_for_host(OWNER, Action::SetDoor { door, open: true })
        .unwrap();
    assert!(matches!(step, crate::CandidateStep::AwaitingRegions(_)));
    assert_eq!(game.emitter_signature(), emitters);
    assert_eq!(game.tick(), tick);
    assert!(game
        .light_missing_regions(game.light_region_boundary().unwrap())
        .is_empty());
    game.complete_region_transition(&crate::TransitionReport::default())
        .unwrap();
    assert!(!game.region_transition_pending());
    assert!(game.tick() > tick);
}
#[test]
fn torch_burnout_during_paid_extinguish_interrupts_exact_item_without_resurrection() {
    let (mut game, item, _) = equipped(50, 4);
    paid(&mut game, Action::Ignite { item }, 100);
    let events = game.combat_events().len();
    let id = game
        .admit_intention(OWNER, Action::Extinguish { item }, IntentionOrigin::Human)
        .unwrap();
    let done = game.execute_next_intention().unwrap();
    assert_eq!(done.intention.id, id);
    done.outcome.unwrap();
    assert_eq!(
        game.tick(),
        150,
        "ordinary discard retires unavailable item work at expiry"
    );
    assert_eq!(game.items[&item].fuel.unwrap().remaining, 0);
    assert!(!game.items[&item].fuel.unwrap().lit);
    assert!(game.preparation(OWNER).is_none());
    assert!(game.combat_events()[events..].iter().any(|event|matches!(event,
        crate::combat::CombatEvent::Interrupted{actor,intention:Some(origin)} if *actor==OWNER && *origin==id)));
    assert!(!game.combat_events()[events..].iter().any(|event| matches!(
        event,
        crate::combat::CombatEvent::ItemCompleted {
            work: crate::Work::Extinguish { .. },
            ..
        }
    )));
}
#[test]
fn torch_consumes_one_real_grip_and_failed_third_grip_is_atomic() {
    let (mut game, first, slot) = fixture(1000, 4);
    let mut anatomy = game.anatomy(OWNER).unwrap().clone();
    let second_slot = EquipmentSlotId(anatomy.slots.len() as u16);
    anatomy
        .slots
        .push(crate::content_ids::EquipmentSlot::new("held_light").unwrap());
    let third_slot = EquipmentSlotId(anatomy.slots.len() as u16);
    anatomy
        .slots
        .push(crate::content_ids::EquipmentSlot::new("held_light").unwrap());
    game.configure_anatomy(OWNER, anatomy).unwrap();
    let second = game
        .place_item_stack(31, at(1), Some(OWNER), 1, spec(1000, 4))
        .unwrap();
    let third = game
        .place_item_stack(32, at(1), Some(OWNER), 1, spec(1000, 4))
        .unwrap();
    game.equip_authored(OWNER, first, slot).unwrap();
    game.equip_authored(OWNER, second, second_slot).unwrap();
    let before = game.clone();
    assert!(game.equip_authored(OWNER, third, third_slot).is_err());
    assert_eq!(game, before);
    assert!(!game.items[&first].fuel.unwrap().lit);
    assert!(!game.items[&second].fuel.unwrap().lit);
}
