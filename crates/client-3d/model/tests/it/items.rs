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
