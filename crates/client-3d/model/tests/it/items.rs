//! Reading, zapping and identification from the server's disclosed affordances.
use tor_client_3d_model::{request_from, view, Cmd};
use tor_client_common::{test_support, ClientState, Palette};
use tor_protocol::*;

const SCROLL: [u8; 32] = [61; 32];
const WAND: [u8; 32] = [62; 32];
const STAFF: [u8; 32] = [63; 32];
const BONE: [u8; 32] = [64; 32];

fn carried(id: [u8; 32], name: &str, class: ItemClass) -> ItemView {
    ItemView {
        id: ItemTarget::from_digest(id),
        name: name.into(),
        class,
        appearance: name.into(),
        identified: false,
        quantity: 1,
        description: String::new(),
        corpse: None,
        asset: None,
        known_currency: None,
        known_supply: None,
    }
}

fn affordance(id: [u8; 32]) -> ItemInteractionView {
    ItemInteractionView {
        item: ItemTarget::from_digest(id),
        slot: None,
        equipped_slot: None,
        known_equipment: None,
        known_projectile_bonus: None,
        known_cursed: None,
        known_protected: None,
        drinkable: false,
        edible: false,
        readable: false,
        zappable: false,
        device_activation: None,
        charges: None,
        portable_light: None,
    }
}

/// A scroll to read, a wand aimed along a direction and a staff aimed at
/// oneself; optionally a waiting identification choice for scroll and wand.
fn observation(identifying: bool) -> Observation {
    let mut o = test_support::observation();
    o.inventory = vec![
        carried(SCROLL, "scroll titled 'zelgo'", ItemClass::Scroll),
        carried(WAND, "oak wand", ItemClass::Wand),
        // Only an identified device may be aimed at its user.
        ItemView {
            identified: true,
            ..carried(STAFF, "ash staff", ItemClass::Wand)
        },
        ItemView {
            identified: true,
            ..carried(BONE, "bone wand", ItemClass::Wand)
        },
    ];
    let mut scroll = affordance(SCROLL);
    scroll.readable = true;
    let mut wand = affordance(WAND);
    wand.zappable = true;
    wand.device_activation = Some(DeviceActivation::Direction);
    let mut staff = affordance(STAFF);
    staff.zappable = true;
    staff.device_activation = Some(DeviceActivation::SelfTarget);
    let mut bone = affordance(BONE);
    bone.zappable = true;
    bone.device_activation = Some(DeviceActivation::DirectionOrSelf);
    o.interactions = Some(InteractionView {
        slots: vec![],
        preparation: None,
        completed: vec![],
        refused: vec![],
        curse_blocked: vec![],
        identification: identifying.then(|| IdentificationChoiceView {
            choice: IntentionId("identify-1".into()),
            classes: {
                let mut classes = vec![ItemClass::Scroll, ItemClass::Wand];
                classes.sort();
                classes
            },
            items: vec![
                ItemTarget::from_digest(SCROLL),
                ItemTarget::from_digest(WAND),
            ],
        }),
        inventory: vec![scroll, wand, staff, bone],
    });
    o
}

fn state(identifying: bool) -> ClientState {
    let observed = observation(identifying);
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness": {"revision": "0", "admission": true, "resume": [], "cancel": []},
        "context": StreamContext { stream: StreamId("items".into()), epoch: 0 },
        "actor": observed.actor, "branch": "play", "cursor": {"sequence": "0", "tick": "0"},
        "has_control": true, "intentions": [], "history": {"entries": [], "older_before": null},
        "state": {"wizard_game": false, "revision": "0", "observation": observed},
    }))
    .unwrap();
    ClientState::from_snapshot(snapshot).unwrap()
}

fn action(state: &ClientState, command: Cmd) -> Result<Action, String> {
    match request_from(state, &state.reply_context(), command)? {
        Request::Command {
            command: Command::Act { action, .. },
            ..
        } => Ok(action),
        other => panic!("expected an action, got {other:?}"),
    }
}

#[test]
fn equipment_projection_preserves_duplicate_sockets_and_only_carried_owner_facts() {
    let mut o = observation(false);
    let slot = EquipmentSlot {
        id: "socket.unusual".into(),
        name: "Grasp".into(),
        equipped_phrase: "in a grasp".into(),
        requires_attack: false,
    };
    let interactions = o.interactions.as_mut().unwrap();
    interactions.slots = vec![slot.clone(), slot.clone()];
    interactions.inventory[1].slot = Some(slot);
    interactions.inventory[1].equipped_slot = Some(1);
    interactions.inventory[1].known_cursed = Some(false);
    let mut absent = affordance([99; 32]);
    absent.slot = interactions.inventory[1].slot.clone();
    interactions.inventory.push(absent);
    let equipment = tor_client_3d_model::Equipment::from_observation(&o);
    assert_eq!(equipment.slots.len(), 2);
    assert_eq!(equipment.slots[1].name, "Grasp");
    assert_eq!(
        equipment.items.len(),
        4,
        "absent carried identities are not display targets"
    );
    let wand = equipment
        .items
        .iter()
        .find(|item| item.item == ItemTarget::from_digest(WAND))
        .unwrap();
    assert_eq!(wand.equipped_slot, Some(1));
    assert_eq!(wand.known_cursed, Some(false));
    assert!(
        wand.known_equipment.is_none(),
        "unknown equipment must stay unknown"
    );
    o.interactions = None;
    let hidden = tor_client_3d_model::Equipment::from_observation(&o);
    assert!(hidden.slots.is_empty() && hidden.items.is_empty());
}

#[test]
fn equipment_compares_socket_identity_and_tracks_free_positions_without_a_swap() {
    let slot = EquipmentSlot {
        id: "grasp".into(),
        name: "Grip".into(),
        equipped_phrase: "in use".into(),
        requires_attack: false,
    };
    let mut candidate = affordance(WAND);
    candidate.slot = Some(slot.clone());
    let mut equipped = affordance(STAFF);
    equipped.slot = Some(slot.clone());
    equipped.equipped_slot = Some(0);
    let mut equipment = tor_client_3d_model::Equipment {
        slots: vec![slot.clone(), slot.clone()],
        items: vec![candidate, equipped],
    };
    let wand = ItemTarget::from_digest(WAND);
    assert_eq!(equipment.comparison_slot(wand, None), Some(0));
    assert_eq!(equipment.comparison_slot(wand, Some(1)), Some(1));
    assert!(equipment.has_free_slot(wand));
    equipment.items[0].slot.as_mut().unwrap().id = "different socket with same label".into();
    assert_eq!(equipment.comparison_slot(wand, Some(0)), None);
    assert!(!equipment.has_free_slot(wand));
    equipment.items[0].slot = Some(slot);
    equipment.items[0].equipped_slot = Some(1);
    assert_eq!(equipment.comparison_slot(wand, Some(0)), Some(1));
    assert!(!equipment.has_free_slot(wand));
    equipment.items[0].known_cursed = Some(false);
    assert_eq!(
        equipment.facts(wand),
        ["Combat properties not known.", "Not cursed"]
    );
    equipment.items[0].known_equipment = Some(EquipmentView {
        attack: None,
        modifiers: vec![],
    });
    equipment.items[0].known_cursed = None;
    assert_eq!(equipment.facts(wand), ["No remembered modifiers."]);
    assert_eq!(
        equipment.facts(ItemTarget::from_digest([99; 32])),
        ["Properties not known."]
    );
}

#[test]
fn equipment_facts_keep_remembered_damage_components_timing_and_unknowns_distinct() {
    let mut offer = affordance(WAND);
    let key = DamageKeyView {
        category: DamageType::Keen,
        descriptor: None,
        sides: None,
    };
    offer.known_equipment = Some(EquipmentView {
        attack: Some(AttackView {
            skill: ContentRef {
                id: "unusual_skill".into(),
                name: "Blade craft".into(),
            },
            bonus: -3,
            damage_enchantment: 2,
            wind_up: 3,
            recovery: 5,
            damage: DamageView {
                primary: key,
                components: vec![
                    DamageComponentView {
                        category: DamageType::Keen,
                        descriptor: None,
                        amount: DamageAmountView::Fixed { value: 7 },
                    },
                    DamageComponentView {
                        category: DamageType::Energy,
                        descriptor: Some(ContentRef {
                            id: "searing".into(),
                            name: "Searing".into(),
                        }),
                        amount: DamageAmountView::Rolled {
                            count: 2,
                            sides: 6,
                            bonus: -1,
                        },
                    },
                ],
            },
        }),
        modifiers: vec![CombatTraceStep {
            id: "ward".into(),
            name: "Disclosed ward".into(),
            fields: vec![DiagnosticField {
                id: "armor".into(),
                name: "Armor".into(),
                value: DiagnosticValue::Signed { value: 2 },
            }],
        }],
    });
    offer.known_projectile_bonus = Some(ProjectileBonusView { hit: 0, damage: -1 });
    offer.known_protected = Some(false);
    let equipment = tor_client_3d_model::Equipment {
        slots: vec![],
        items: vec![offer],
    };
    assert_eq!(
        equipment.facts(ItemTarget::from_digest(WAND)),
        [
            "Blade craft attack bonus: -3",
            "Damage enchantment: +2",
            "Wind-up: 3 ticks  |  Recovery: 5 ticks",
            "Keen damage: 7",
            "Searing damage: 2d6-1",
            "Disclosed ward",
            "  Armor: 2",
            "Projectile hit bonus: +0  |  Damage bonus: -1",
            "Not protected",
        ]
    );
}

#[test]
fn carried_items_show_reading_and_how_each_device_is_aimed() {
    let projected = view(&state(false), &Palette::default());
    let item = |id| {
        projected
            .inventory
            .iter()
            .find(|c| c.id == ItemTarget::from_digest(id))
            .unwrap()
    };
    assert!(item(SCROLL).readable && item(SCROLL).device.is_none());
    assert_eq!(item(WAND).device, Some(DeviceActivation::Direction));
    assert_eq!(item(STAFF).device, Some(DeviceActivation::SelfTarget));
    assert!(!item(WAND).readable);
    assert_eq!(projected.identify, None, "no choice waits");
}

#[test]
fn reading_and_zapping_send_the_server_actions_for_those_items() {
    use tor_client_common::items::ItemOperation;
    let state = state(false);
    let scroll = ItemTarget::from_digest(SCROLL);
    let wand = ItemTarget::from_digest(WAND);
    let staff = ItemTarget::from_digest(STAFF);
    assert_eq!(
        action(&state, Cmd::Use(scroll, ItemOperation::Read)),
        Ok(Action::Read { item: scroll })
    );
    assert_eq!(
        action(&state, Cmd::Zap(wand, Direction::NorthEast)),
        Ok(Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::NorthEast
            }
        })
    );
    assert_eq!(
        action(&state, Cmd::Use(staff, ItemOperation::Zap)),
        Ok(Action::Zap {
            item: staff,
            aim: DeviceAim::SelfTarget {}
        })
    );
    assert!(
        action(&state, Cmd::Use(wand, ItemOperation::Zap)).is_err(),
        "a directional device needs its direction"
    );
    assert!(action(&state, Cmd::Zap(scroll, Direction::North)).is_err());
}

#[test]
fn a_device_offering_either_aim_zaps_along_a_direction_or_at_yourself() {
    let state = state(false);
    let bone = ItemTarget::from_digest(BONE);
    let wand = ItemTarget::from_digest(WAND);
    assert_eq!(
        view(&state, &Palette::default())
            .inventory
            .iter()
            .find(|item| item.id == bone)
            .and_then(|item| item.device),
        Some(DeviceActivation::DirectionOrSelf)
    );
    assert_eq!(
        action(&state, Cmd::ZapSelf(bone)),
        Ok(Action::Zap {
            item: bone,
            aim: DeviceAim::SelfTarget {}
        })
    );
    assert_eq!(
        action(&state, Cmd::Zap(bone, Direction::West)),
        Ok(Action::Zap {
            item: bone,
            aim: DeviceAim::Direction {
                direction: Direction::West
            }
        })
    );
    assert!(
        action(&state, Cmd::ZapSelf(wand)).is_err(),
        "a directional-only device cannot target its user"
    );
}

#[test]
fn a_waiting_identification_offers_only_its_carried_choices() {
    let state = state(true);
    let projected = view(&state, &Palette::default());
    let scroll = ItemTarget::from_digest(SCROLL);
    let wand = ItemTarget::from_digest(WAND);
    assert_eq!(projected.identify, Some(vec![scroll, wand]));
    let choice = IntentionId("identify-1".into());
    assert_eq!(
        action(&state, Cmd::Identify(Some(wand))),
        Ok(Action::Identify {
            choice: choice.clone(),
            item: Some(wand)
        })
    );
    assert_eq!(
        action(&state, Cmd::Identify(None)),
        Ok(Action::Identify { choice, item: None }),
        "declining answers the same choice"
    );
    assert!(
        action(&state, Cmd::Identify(Some(ItemTarget::from_digest(STAFF)))).is_err(),
        "an item the choice does not offer is refused"
    );
}
