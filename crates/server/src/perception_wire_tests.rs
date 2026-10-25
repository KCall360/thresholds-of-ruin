//! Encoded adapter contracts. Simulation admission owns capability/reach and
//! touch effects; these tests own domains, immutable projections and save facts.
use super::*;
use crate::journal::{Event, JournalContent, JournalEntry};
use s::perception::{ActorDisclosure, ActorReferenceKind, ApparentObjectView, GuiseAppearance};
use serde_json::{json, Value};

const SUBJECT: s::ActorId = s::ActorId(9_007_199_254_740_993);

fn fixture() -> (s::Observation, Vec<tor_world::SightCell>, TargetScope) {
    let mut game = s::Game::two_room_in_stone(42, crate::engine::diagnostic_content().unwrap());
    let actor = game
        .spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    let (mut observation, scene) = game.observe_scene(actor).unwrap();
    observation.ground_items.clear();
    observation.visible_actors.clear();
    let scope = TargetScope::new(uuid::Uuid::from_u128(7), actor);
    (observation, scene, scope)
}

fn guise(observation: &mut s::Observation) {
    observation.apparent_objects.push(ApparentObjectView {
        subject: SUBJECT,
        location: observation.location,
        appearance: GuiseAppearance {
            name: "a weathered chest".into(),
            description: "A small iron-bound chest.".into(),
            class: s::ItemClass::Misc,
            asset: Some("chest_asset".into()),
        },
    });
}

fn contact(observation: &mut s::Observation, disclosure: ActorDisclosure) {
    observation.visible_actors.clear();
    observation.visible_actors.push(s::ActorView {
        disclosure,
        id: SUBJECT,
        location: observation.location,
        name: if disclosure == ActorDisclosure::Creature {
            "private_subject_name".into()
        } else {
            String::new()
        },
        asset: (disclosure == ActorDisclosure::Creature).then(|| "private_species_asset".into()),
    });
}

fn encoded(value: &impl serde::Serialize) -> Value {
    let text = p::encode_bounded_json(value, p::MAX_SNAPSHOT_BYTES).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn projected(
    observation: s::Observation,
    scene: Vec<tor_world::SightCell>,
    scope: &TargetScope,
) -> Value {
    encoded(&crate::adapt::observation(
        observation,
        scene,
        "private-save-salt",
        true,
        &|_| [None, None, None],
        scope,
    ))
}

fn request(action: Value) -> p::Action {
    let encoded = json!({
        "type": "request", "request_id": "perception-request", "request": {
            "type": "command", "branch": "perception-branch",
            "context": {"stream": {"stream": "perception-stream", "epoch": "0"},
                "readiness_revision": "0"},
            "command": {"type": "act", "expected_revision": "0", "action": action}
        }
    })
    .to_string();
    let p::ClientMessage::Request { request, .. } = p::decode_request(&encoded).unwrap() else {
        panic!("gameplay request expected");
    };
    let p::Request::Command { command, .. } = *request else {
        panic!("gameplay command expected");
    };
    let DecodedCommand::Gameplay { action, .. } = decode_command(&command).unwrap() else {
        panic!("target resolution must remain deferred until after receipt lookup");
    };
    action
}

fn take(item: p::ItemTarget, quantity: Option<u64>) -> p::Action {
    request(json!({
        "type": "take", "item": item, "quantity": quantity.map(|value| value.to_string())
    }))
}

fn attack(target: p::ActorTarget) -> p::Action {
    request(json!({
        "type": "use_ability", "id": "basic_melee",
        "invocation": {"aim":{"kind": "actor_target", "target": target},"weapon":{"kind":"default"}},
        "supplies": {"kind": "carried"}
    }))
}

fn entry(action: a::Action, event: Event) -> JournalEntry {
    JournalEntry {
        id: p::EntryId("perception-entry".into()),
        branch: p::BranchId("perception-branch".into()),
        actor: p::ActorId(1),
        tick: 100,
        author: p::Author::User {
            user: "owner".into(),
        },
        audience: p::Audience::Actor,
        intention_suspensions: vec![],
        intention_ends: vec![],
        content: JournalContent::Action { action, event },
    }
}

fn withheld(value: &Value, scope: &TargetScope) {
    let text = value.to_string();
    for private in [
        SUBJECT.0.to_string(),
        scope.actor(SUBJECT).to_string(),
        scope.appearance_seed(SUBJECT).to_string(),
        "private_subject_name".into(),
        "private_species_asset".into(),
        "private_faction".into(),
        "private_ability_name".into(),
        "private_ability_id".into(),
        "private_plan".into(),
        "private-save-salt".into(),
    ] {
        assert!(!text.contains(&private), "withheld fact leaked: {private}");
    }
}

#[test]
fn apparent_take_round_trips_only_the_apparent_domain_and_exact_quantity() {
    let (mut view, _, scope) = fixture();
    guise(&mut view);
    for quantity in [None, Some(1), Some(0), Some(2), Some(u64::MAX)] {
        // Invalid quantities are retained for simulation rejection, never normalized.
        let wire = take(scope.apparent_object(SUBJECT), quantity);
        let backend = decode_action(&wire, &scope, &view).unwrap();
        assert_eq!(
            backend,
            a::Action::TouchApparentObject {
                subject: SUBJECT,
                quantity
            }
        );
        assert_eq!(
            crate::adapt::recorded_action(crate::adapt::action(&backend)),
            Some(backend.clone())
        );
        assert_eq!(crate::adapt::work(&backend), None);
        assert_eq!(encoded(&encode_action(&backend, &scope)), encoded(&wire));
        if !matches!(quantity, None | Some(1)) {
            continue;
        }
        let event = crate::adapt::event(s::OutcomeKind::ApparentObjectTouched { subject: SUBJECT });
        assert_eq!(event, Event::Waited);
        let journal = entry(backend, event);
        let restored: JournalEntry =
            crate::storage::codec::strict(&serde_json::to_vec(&journal).unwrap()).unwrap();
        let history = encoded(&disclose_entry(&restored, &scope).unwrap());
        assert_eq!(history["content"]["action"], encoded(&wire));
        assert_eq!(history["content"]["event"], json!({"type":"waited"}));
        withheld(&history, &scope);
    }
}

#[test]
fn apparent_tokens_never_become_inventory_supply_or_actor_authority() {
    let (mut view, _, scope) = fixture();
    guise(&mut view);
    let token = scope.apparent_object(SUBJECT);
    let other = TargetScope::new(uuid::Uuid::from_u128(7), s::ActorId(2));
    let foreign_save = TargetScope::new(uuid::Uuid::from_u128(8), view.actor);
    for wire in [
        take(scope.item(s::ItemId(SUBJECT.0)), None),
        take(other.apparent_object(SUBJECT), None),
        take(foreign_save.apparent_object(SUBJECT), None),
        take(scope.apparent_object(s::ActorId(SUBJECT.0 + 1)), None),
        p::Action::Drop {
            item: token,
            quantity: None,
        },
        p::Action::Equip {
            item: token,
            slot: 0,
        },
        p::Action::Unequip { item: token },
        p::Action::Eat { item: token },
        p::Action::Drink { item: token },
        p::Action::Ignite { item: token },
        p::Action::Extinguish { item: token },
        attack(scope.actor(SUBJECT)),
        attack(scope.contact(SUBJECT)),
        p::Action::UseAbility {
            id: "basic_melee".into(),
            invocation: p::AbilityInvocation {
                aim: p::AbilityAim::ActorTarget {
                    target: scope.actor(view.actor),
                },
                weapon: p::WeaponSelection::Default,
            },
            supplies: p::SupplySelection::Explicit {
                items: vec![p::SelectedSupply {
                    cost_ordinal: 0,
                    item: token,
                }],
            },
        },
    ] {
        let failure = decode_action(&wire, &scope, &view).unwrap_err();
        assert_eq!(failure.code, p::ErrorCode::InvalidAction);
        assert_eq!(failure.message, "Target is unavailable");
    }
    assert!(decode_action(&take(token, None), &other, &view).is_err());
    // Once the source stops disclosing the guise, fresh resolution cannot reuse it.
    view.apparent_objects.clear();
    contact(&mut view, ActorDisclosure::Creature);
    assert!(decode_action(&take(token, None), &scope, &view).is_err());
}

#[test]
fn apparent_object_projection_is_an_ordinary_item_without_creature_or_supply_facts() {
    let (mut view, scene, scope) = fixture();
    guise(&mut view);
    let wire = projected(view, scene, &scope);
    let item = wire["ground_items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["item"]["id"] == json!(scope.apparent_object(SUBJECT)))
        .unwrap();
    assert_eq!(item["item"]["name"], "a weathered chest");
    assert_eq!(item["item"]["asset"], "chest_asset");
    assert_eq!(item["item"]["quantity"], "1");
    assert_eq!(item["item"]["identified"], true);
    assert_eq!(item["item"]["corpse"], Value::Null);
    assert_eq!(item["item"]["known_supply"], Value::Null);
    assert_eq!(item["item"].get("known_currency"), Some(&Value::Null));
    assert_eq!(item["supply_reachable"], false);
    assert!(wire["visible_actors"].as_array().unwrap().is_empty());
    withheld(&wire, &scope);
}

#[test]
fn currency_projection_preserves_identification_ownership_and_exact_stack_units() {
    let mut game = s::Game::two_room_in_stone(42, crate::engine::diagnostic_content().unwrap());
    let at = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    let other = game
        .spawn_actor(
            tor_world::Location {
                position: tor_world::Position { x: 2, y: 1, z: 0 },
                ..at
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
    for (id, owner, concealed, currency, quantity) in [
        (80, Some(actor), false, true, 9_007_199_254_740_993),
        (81, Some(actor), true, true, 7),
        (82, Some(actor), false, false, 11),
        (83, None, false, true, 13),
        (84, None, true, true, 17),
        (85, Some(other), false, true, 19),
    ] {
        let mut spec = s::ItemSpec::ordinary(format!("currency fixture {id}"));
        spec.class = s::ItemClass::Coin;
        spec.stackable = true;
        spec.concealed = concealed;
        spec.appearance = "gold".into();
        spec.asset = Some("item.gold".into());
        spec.currency = currency.then_some(s::CurrencyKind::Gold);
        game.place_item_stack(id, at, owner, quantity, spec)
            .unwrap();
    }
    let scope = TargetScope::new(uuid::Uuid::from_u128(7), actor);
    let (mut observation, scene) = game.observe_scene(actor).unwrap();
    observation.apparent_objects.push(ApparentObjectView {
        subject: SUBJECT,
        location: at,
        appearance: GuiseAppearance {
            name: "gold".into(),
            description: String::new(),
            class: s::ItemClass::Coin,
            asset: Some("item.gold".into()),
        },
    });
    let wire = projected(observation, scene, &scope);
    let inventory = wire["inventory"].as_array().unwrap();
    assert_eq!(inventory.len(), 3);
    for (id, expected, quantity) in [
        (80, json!("gold"), "9007199254740993"),
        (81, Value::Null, "7"),
        (82, Value::Null, "11"),
    ] {
        let item = inventory
            .iter()
            .find(|item| item["id"] == json!(scope.item(s::ItemId(id))))
            .unwrap();
        assert_eq!(item.get("known_currency"), Some(&expected));
        assert_eq!(item["quantity"], quantity);
    }
    let ground = wire["ground_items"].as_array().unwrap();
    for (target, expected) in [
        (scope.item(s::ItemId(83)), json!("gold")),
        (scope.item(s::ItemId(84)), Value::Null),
        (scope.apparent_object(SUBJECT), Value::Null),
    ] {
        let item = &ground
            .iter()
            .find(|row| row["item"]["id"] == json!(target))
            .unwrap()["item"];
        assert_eq!(item.get("known_currency"), Some(&expected));
    }
    assert!(!wire
        .to_string()
        .contains(&scope.item(s::ItemId(85)).to_string()));
    game.learn_identity(actor, "currency fixture 81").unwrap();
    let (learned, learned_scene) = game.observe_scene(actor).unwrap();
    let learned = projected(learned, learned_scene, &scope);
    let revealed = learned["inventory"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == json!(scope.item(s::ItemId(81))))
        .unwrap();
    assert_eq!(revealed["known_currency"], "gold");
    let (other_view, other_scene) = game.observe_scene(other).unwrap();
    let other_scope = TargetScope::new(uuid::Uuid::from_u128(7), other);
    let other_wire = projected(other_view, other_scene, &other_scope);
    assert_eq!(other_wire["inventory"].as_array().unwrap().len(), 1);
    assert_eq!(other_wire["inventory"][0]["known_currency"], "gold");
    assert_eq!(other_wire["inventory"][0]["quantity"], "19");
}

#[test]
fn real_ground_item_and_guise_with_equal_private_numbers_resolve_separately() {
    let (mut view, _, scope) = fixture();
    guise(&mut view);
    view.ground_items.push(s::GroundItemView {
        corpse: None,
        class: s::ItemClass::Misc,
        description: String::new(),
        id: s::ItemId(SUBJECT.0),
        name: "stone".into(),
        quantity: 1,
        appearance: "stone".into(),
        identified: false,
        known_supply: None,
        known_currency: None,
        supply_reachable: true,
        location: view.location,
        asset: None,
    });
    assert_eq!(
        decode_action(&take(scope.item(s::ItemId(SUBJECT.0)), None), &scope, &view).unwrap(),
        a::Action::Take {
            item: SUBJECT.0,
            quantity: None
        }
    );
    assert_eq!(
        decode_action(&take(scope.apparent_object(SUBJECT), None), &scope, &view).unwrap(),
        a::Action::TouchApparentObject {
            subject: SUBJECT,
            quantity: None
        }
    );
}

#[test]
fn contact_action_and_preparation_never_upgrade_when_current_disclosure_changes() {
    let (mut view, scene, scope) = fixture();
    contact(&mut view, ActorDisclosure::AnonymousContact);
    let wire = attack(scope.contact(SUBJECT));
    let action = decode_action(&wire, &scope, &view).unwrap();
    assert!(matches!(&action, a::Action::UseAbility {
        invocation: a::AbilityInvocation { aim: a::AbilityAim::ContactTarget { target }, weapon: a::WeaponSelection::Default }, ..
    } if *target == SUBJECT));
    assert!(decode_action(&attack(scope.actor(SUBJECT)), &scope, &view).is_err());
    let work = crate::adapt::work(&action).unwrap();
    let journal = entry(
        action.clone(),
        Event::AbilityStarted {
            reference: ActorReferenceKind::Contact,
            id: s::content_ids::AbilityId::new("basic_melee").unwrap(),
            name: "Basic melee".into(),
            target: p::ActorId(SUBJECT.0),
        },
    );
    let history = encoded(&disclose_entry(&journal, &scope).unwrap());
    assert_eq!(history["content"]["action"], encoded(&wire));
    assert_eq!(
        history["content"]["event"]["target"],
        json!(scope.contact(SUBJECT))
    );
    withheld(&history, &scope);
    // The fixture changes only current disclosure: detection, reveal, and removal
    // cannot rewrite frozen backend invocation or independently saved reference.
    for current in [
        Some(ActorDisclosure::AnonymousContact),
        Some(ActorDisclosure::Creature),
        None,
    ] {
        if let Some(disclosure) = current {
            contact(&mut view, disclosure);
        } else {
            view.visible_actors.clear();
        }
        view.interactions = Some(s::InteractionView {
            curse_blocked: vec![],
            identification: None,
            completed: vec![],
            refused: vec![],
            slots: vec![],
            inventory: vec![],
            preparation: Some(s::PreparationView {
                work: work.clone(),
                remaining: 50,
                active: true,
            }),
        });
        let projected = projected(view.clone(), scene.clone(), &scope);
        assert_eq!(
            projected["interactions"]["preparation"]["action"],
            encoded(&wire)
        );
        let restored: JournalEntry =
            crate::storage::codec::strict(&serde_json::to_vec(&journal).unwrap()).unwrap();
        let reopened_scope = TargetScope::new(uuid::Uuid::from_u128(7), view.actor);
        assert_eq!(
            encoded(&disclose_entry(&restored, &reopened_scope).unwrap()),
            history
        );
        assert_eq!(
            encoded(&encode_action(&action, &reopened_scope)),
            encoded(&wire)
        );
        if current != Some(ActorDisclosure::AnonymousContact) {
            assert!(decode_action(&wire, &scope, &view).is_err());
        }
    }
}

#[test]
fn anonymous_projection_and_personally_experienced_activity_withhold_identity() {
    let (mut view, scene, scope) = fixture();
    contact(&mut view, ActorDisclosure::AnonymousContact);
    view.combat = Some(s::combat::CombatView {
        own_stats: None,
        public_plan: None,
        hp: 10,
        max_hp: 10,
        preparation_remaining: None,
        preparation_active: false,
        recovery_remaining: 0,
        sequence: None,
        actors: vec![],
        events: vec![s::combat::DisclosedCombatEvent::UnseenAbility {
            caster: Some(SUBJECT),
            target: view.actor,
            outcome: s::abilities::OutcomeStatus::Miss,
        }],
        objective: None,
        exit: None,
        victory: false,
        dead: false,
        terminal: false,
    });
    let wire = projected(view, scene, &scope);
    let contact = wire["visible_actors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|actor| actor["id"] == json!(scope.contact(SUBJECT)))
        .unwrap();
    assert_eq!(contact["name"], "");
    assert_eq!(contact["asset"], Value::Null);
    assert_eq!(contact["appearance_seed"], Value::Null);
    assert!(wire["combat"]["actors"].as_array().unwrap().is_empty());
    assert_eq!(wire["combat"]["public_plan"], Value::Null);
    let event = &wire["combat"]["events"][0];
    assert_eq!(event["type"], "unseen_ability");
    assert_eq!(event["caster"], json!(scope.contact(SUBJECT)));
    assert!(event.get("ability").is_none());
    assert!(event.get("strike").is_none());
    withheld(&wire, &scope);
}

#[test]
fn anonymous_ability_projection_preserves_reference_domains_without_source_identity() {
    for (source_reference, target_reference) in [
        (
            Some(ActorReferenceKind::Creature),
            Some(ActorReferenceKind::Creature),
        ),
        (
            Some(ActorReferenceKind::Contact),
            Some(ActorReferenceKind::Creature),
        ),
        (
            Some(ActorReferenceKind::Creature),
            Some(ActorReferenceKind::Contact),
        ),
        (None, Some(ActorReferenceKind::Creature)),
        (Some(ActorReferenceKind::Creature), None),
    ] {
        let (mut view, scene, scope) = fixture();
        let observer = view.actor;
        view.combat = Some(s::combat::CombatView {
            own_stats: None,
            public_plan: None,
            hp: 10,
            max_hp: 10,
            preparation_remaining: None,
            preparation_active: false,
            recovery_remaining: 0,
            sequence: None,
            actors: vec![],
            events: vec![s::combat::DisclosedCombatEvent::AnonymousAbility {
                caster: source_reference.map(|_| SUBJECT),
                caster_reference: source_reference,
                target: target_reference.map(|_| observer),
                target_reference,
                outcome: s::abilities::OutcomeStatus::Unaffected,
            }],
            objective: None,
            exit: None,
            victory: false,
            dead: false,
            terminal: false,
        });
        let wire = projected(view, scene, &scope);
        let event = &wire["combat"]["events"][0];
        assert_eq!(
            event,
            &json!({
                "type":"anonymous_ability",
                "caster":source_reference.map(|kind| scope.actor_reference(SUBJECT, kind)),
                "target":target_reference.map(|kind| scope.actor_reference(observer, kind)),
                "outcome":"unaffected"
            })
        );
        let decoded: p::CombatEventView = serde_json::from_value(event.clone()).unwrap();
        assert_eq!(encoded(&decoded), *event);
        for hidden in [
            "ability", "id", "name", "source", "item", "charges", "strike", "effects",
        ] {
            assert!(event.get(hidden).is_none());
        }
        assert!(!event.to_string().contains(&SUBJECT.0.to_string()));
    }
}

#[test]
fn ordinary_move_history_has_no_contact_or_apparent_target() {
    let (_, _, scope) = fixture();
    let action = a::Action::Move {
        direction: a::Direction::East,
    };
    let journal = entry(action.clone(), Event::Waited);
    let history = encoded(&disclose_entry(&journal, &scope).unwrap());
    assert_eq!(
        history["content"]["action"],
        json!({"type":"move", "direction":"east"})
    );
    assert!(!history
        .to_string()
        .contains(&scope.contact(SUBJECT).to_string()));
    assert!(!history
        .to_string()
        .contains(&scope.apparent_object(SUBJECT).to_string()));
    withheld(&history, &scope);
    assert_eq!(
        crate::adapt::recorded_action(crate::adapt::action(&action)),
        Some(action)
    );
}

#[test]
fn apparent_retry_matching_uses_saved_facts_before_live_availability() {
    let (mut view, _, scope) = fixture();
    guise(&mut view);
    let original_wire = take(scope.apparent_object(SUBJECT), None);
    let saved = Command::AdmitIntention {
        expected_revision: 17,
        action: decode_action(&original_wire, &scope, &view).unwrap(),
    };
    let restored: Command =
        crate::storage::codec::strict(&serde_json::to_vec(&saved).unwrap()).unwrap();
    view.apparent_objects.clear();
    for wire in [
        original_wire.clone(),
        take(scope.apparent_object(SUBJECT), Some(1)),
        take(scope.item(s::ItemId(SUBJECT.0)), None),
    ] {
        let decoded = decode_command(&p::Command::Act {
            expected_revision: 17,
            action: wire.clone(),
        })
        .unwrap();
        assert_eq!(decoded.matches(&restored, &scope), wire == original_wire);
        assert!(decode_action(&wire, &scope, &view).is_err());
    }
    let wrong_revision = decode_command(&p::Command::Act {
        expected_revision: 18,
        action: original_wire,
    })
    .unwrap();
    assert!(!wrong_revision.matches(&restored, &scope));
}

#[test]
fn old_creature_token_retry_remains_exact_when_new_actions_require_contact() {
    let (mut view, _, scope) = fixture();
    contact(&mut view, ActorDisclosure::Creature);
    let original_wire = attack(scope.actor(SUBJECT));
    let saved = Command::AdmitIntention {
        expected_revision: 11,
        action: decode_action(&original_wire, &scope, &view).unwrap(),
    };
    let restored: Command =
        crate::storage::codec::strict(&serde_json::to_vec(&saved).unwrap()).unwrap();
    contact(&mut view, ActorDisclosure::AnonymousContact);
    let current_wire = attack(scope.contact(SUBJECT));
    assert!(decode_action(&original_wire, &scope, &view).is_err());
    assert!(matches!(
        decode_action(&current_wire, &scope, &view).unwrap(),
        a::Action::UseAbility {
            invocation: a::AbilityInvocation {
                aim: a::AbilityAim::ContactTarget { .. },
                weapon: a::WeaponSelection::Default
            },
            ..
        }
    ));
    for wire in [original_wire.clone(), current_wire] {
        let decoded = decode_command(&p::Command::Act {
            expected_revision: 11,
            action: wire.clone(),
        })
        .unwrap();
        assert_eq!(decoded.matches(&restored, &scope), wire == original_wire);
    }
    view.visible_actors.clear();
    let decoded = decode_command(&p::Command::Act {
        expected_revision: 11,
        action: original_wire.clone(),
    })
    .unwrap();
    assert!(decode_action(&original_wire, &scope, &view).is_err());
    assert!(decoded.matches(
        &restored,
        &TargetScope::new(uuid::Uuid::from_u128(7), view.actor)
    ));
}

#[test]
fn apparent_mundane_mace_matches_ordinary_ground_mace_except_opaque_identity() {
    let (mut view, scene, scope) = fixture();
    let real = s::ItemId(55);
    view.ground_items.push(s::GroundItemView {
        corpse: None,
        class: s::ItemClass::Weapon,
        description: String::new(),
        id: real,
        name: "mace".into(),
        quantity: 1,
        appearance: "mace".into(),
        identified: true,
        known_supply: None,
        known_currency: None,
        supply_reachable: false,
        location: view.location,
        asset: Some("item.weapon.mace".into()),
    });
    view.apparent_objects.push(ApparentObjectView {
        subject: SUBJECT,
        location: view.location,
        appearance: GuiseAppearance {
            name: "mace".into(),
            description: String::new(),
            class: s::ItemClass::Weapon,
            asset: Some("item.weapon.mace".into()),
        },
    });
    let wire = projected(view, scene, &scope);
    let ground = wire["ground_items"].as_array().unwrap();
    let ordinary = ground
        .iter()
        .find(|row| row["item"]["id"] == json!(scope.item(real)))
        .unwrap();
    let apparent = ground
        .iter()
        .find(|row| row["item"]["id"] == json!(scope.apparent_object(SUBJECT)))
        .unwrap();
    assert_ne!(ordinary["item"]["id"], apparent["item"]["id"]);
    for row in [ordinary, apparent] {
        for (field, expected) in [
            ("name", json!("mace")),
            ("appearance", json!("mace")),
            ("class", json!("weapon")),
            ("quantity", json!("1")),
            ("description", json!("")),
            ("asset", json!("item.weapon.mace")),
            ("identified", json!(true)),
            ("known_supply", Value::Null),
            ("known_currency", Value::Null),
        ] {
            assert_eq!(row["item"].get(field), Some(&expected), "{field}");
        }
        assert!(
            row["item"].get("corpse").is_none(),
            "ordinary non-corpse metadata is omitted"
        );
        assert_eq!(row.get("reachable"), Some(&json!(true)));
        assert_eq!(row.get("supply_reachable"), Some(&json!(false)));
    }
    let mut ordinary = ordinary.clone();
    let mut apparent = apparent.clone();
    ordinary["item"]["id"] = Value::Null;
    apparent["item"]["id"] = Value::Null;
    assert_eq!(apparent, ordinary, "only opaque item identity may differ");
    assert!(wire["inventory"].as_array().unwrap().is_empty());
    assert!(wire["visible_actors"].as_array().unwrap().is_empty());
    withheld(&wire, &scope);
}

#[test]
fn magic_choices_resolve_exact_owner_tokens_category_and_carried_incarnation() {
    let (mut view, scene, scope) = fixture();
    let item = s::ItemId(700);
    let choice = s::IntentionId(80);
    view.inventory = vec![s::ItemView {
        corpse: None,
        class: s::ItemClass::Wand,
        description: String::new(),
        id: item,
        name: "ivory wand".into(),
        quantity: 1,
        appearance: "ivory wand".into(),
        identified: false,
        known_supply: None,
        known_currency: None,
        asset: None,
    }];
    view.interactions = Some(s::InteractionView {
        curse_blocked: vec![],
        completed: vec![],
        refused: vec![],
        slots: vec![],
        preparation: None,
        inventory: vec![],
        identification: Some(s::IdentificationChoiceView {
            choice,
            classes: vec![s::ItemClass::Wand],
            items: vec![item],
        }),
    });
    let projected = projected(view.clone(), scene, &scope);
    assert_eq!(
        projected["interactions"]["identification"]["choice"],
        json!(scope.identification_choice(choice))
    );
    assert_eq!(
        projected["interactions"]["identification"]["items"],
        json!([scope.item(item)])
    );
    let selected = p::Action::Identify {
        choice: scope.identification_choice(choice),
        item: Some(scope.item(item)),
    };
    let resolved = a::Action::Identify {
        choice,
        item: Some(item.0),
    };
    assert_eq!(decode_action(&selected, &scope, &view).unwrap(), resolved);
    assert_eq!(encode_action(&resolved, &scope), selected);
    let cancelled = p::Action::Identify {
        choice: scope.identification_choice(choice),
        item: None,
    };
    assert_eq!(
        decode_action(&cancelled, &scope, &view).unwrap(),
        a::Action::Identify { choice, item: None }
    );
    for action in [
        p::Action::Read {
            item: scope.item(item),
        },
        p::Action::Zap {
            item: scope.item(item),
            aim: p::DeviceAim::Direction {
                direction: p::Direction::SouthWest,
            },
        },
    ] {
        let resolved = decode_action(&action, &scope, &view).unwrap();
        assert_eq!(encode_action(&resolved, &scope), action);
        assert_eq!(
            crate::adapt::recorded_action(crate::adapt::action(&resolved)).unwrap(),
            resolved
        );
    }
    let other = TargetScope::new(uuid::Uuid::from_u128(7), s::ActorId(view.actor.0 + 1));
    assert_ne!(
        scope.identification_choice(choice),
        other.identification_choice(choice)
    );
    assert!(decode_action(&selected, &other, &view).is_err());
    for stale in [
        p::Action::Identify {
            choice: other.identification_choice(choice),
            item: Some(scope.item(item)),
        },
        p::Action::Identify {
            choice: scope.identification_choice(s::IntentionId(choice.0 + 1)),
            item: Some(scope.item(item)),
        },
        p::Action::Identify {
            choice: scope.identification_choice(choice),
            item: Some(other.item(item)),
        },
    ] {
        assert!(decode_action(&stale, &scope, &view).is_err());
    }
    view.inventory[0].class = s::ItemClass::Potion;
    assert!(decode_action(&selected, &scope, &view).is_err());
    view.inventory[0].class = s::ItemClass::Wand;
    view.inventory[0].id = s::ItemId(item.0 + 1);
    assert!(decode_action(&selected, &scope, &view).is_err());
    // Aborting is still permitted even when the last eligible item disappeared.
    assert!(decode_action(&cancelled, &scope, &view).is_ok());
    view.interactions.as_mut().unwrap().identification = None;
    assert!(decode_action(&cancelled, &scope, &view).is_err());
    // Historic receipts retain opaque stable mapping without re-reading live items.
    assert_eq!(encode_action(&resolved, &scope), selected);
    assert!(disclose_entry(
        &entry(
            resolved,
            Event::IdentificationResolved {
                choice,
                item: Some(item.0)
            }
        ),
        &scope
    )
    .is_none());
}

#[test]
fn equipped_weapon_selection_keeps_item_scope_socket_and_contact_authority_independent() {
    let (mut view, _, scope) = fixture();
    let item = s::ItemId(9_007_199_254_740_995);
    let socket = s::EquipmentSlotId(0);
    view.inventory.push(s::ItemView {
        corpse: None,
        class: s::ItemClass::Weapon,
        description: "An observed equipped item.".into(),
        id: item,
        name: "equipped item".into(),
        quantity: 1,
        appearance: "equipped_item".into(),
        identified: false,
        known_supply: None,
        known_currency: None,
        asset: None,
    });
    view.interactions = Some(s::InteractionView {
        identification: None,
        completed: Vec::new(),
        refused: Vec::new(),
        curse_blocked: Vec::new(),
        slots: vec![s::EquipmentSlotView {
            id: s::content_ids::EquipmentSlot::new("hand").unwrap(),
            name: "Hand".into(),
            equipped_phrase: "in hand".into(),
            requires_attack: true,
        }],
        preparation: None,
        inventory: vec![s::ItemInteractionView {
            item,
            slot: None,
            equipped_slot: Some(socket),
            known_projectile_bonus: None,
            known_equipment: None,
            known_cursed: None,
            known_protected: None,
            drinkable: false,
            readable: false,
            zappable: false,
            device_activation: None,
            charges: None,
            edible: false,
        }],
    });
    contact(&mut view, ActorDisclosure::AnonymousContact);
    let action = |target, item, socket| {
        request(json!({
            "type":"use_ability", "id":"basic_melee",
            "invocation":{"aim":{"kind":"actor_target","target":target},
                "weapon":{"kind":"equipped","item":item,"socket":socket}},
            "supplies":{"kind":"carried"}
        }))
    };
    let wire = action(scope.contact(SUBJECT), scope.item(item), socket.0);
    let backend = decode_action(&wire, &scope, &view).unwrap();
    assert!(matches!(&backend, a::Action::UseAbility {
        invocation: a::AbilityInvocation {
            aim: a::AbilityAim::ContactTarget { target: SUBJECT },
            weapon: a::WeaponSelection::Equipped { item: selected, socket: 0 },
        }, ..
    } if *selected == item.0));
    assert_eq!(encode_action(&backend, &scope), wire);
    assert_eq!(
        crate::adapt::recorded_action(crate::adapt::action(&backend)),
        Some(backend.clone())
    );

    let foreign = TargetScope::new(uuid::Uuid::from_u128(7), s::ActorId(view.actor.0 + 1));
    let errors = [
        action(scope.contact(SUBJECT), foreign.item(item), 0),
        action(scope.contact(SUBJECT), scope.item(s::ItemId(item.0 + 1)), 0),
        action(scope.contact(SUBJECT), scope.apparent_object(SUBJECT), 0),
        action(scope.contact(SUBJECT), scope.item(item), 1),
        action(scope.actor(SUBJECT), scope.item(item), 0),
    ]
    .map(|invalid| decode_action(&invalid, &scope, &view).unwrap_err());
    for error in &errors {
        assert_eq!(
            error, &errors[0],
            "unavailable operands reveal no private identity"
        );
    }
    assert!(decode_action(&wire, &foreign, &view).is_err());
    let mut unequipped = view.clone();
    unequipped.interactions.as_mut().unwrap().inventory[0].equipped_slot = None;
    assert_eq!(
        decode_action(&wire, &scope, &unequipped).unwrap_err(),
        errors[0]
    );
    let mut no_inventory = view.clone();
    no_inventory.inventory.clear();
    assert_eq!(
        decode_action(&wire, &scope, &no_inventory).unwrap_err(),
        errors[0]
    );
    let mut no_interactions = view.clone();
    no_interactions.interactions = None;
    assert_eq!(
        decode_action(&wire, &scope, &no_interactions).unwrap_err(),
        errors[0]
    );
    contact(&mut view, ActorDisclosure::Creature);
    assert!(
        decode_action(&wire, &scope, &view).is_err(),
        "contact freshness is separate from item authority"
    );
    assert!(decode_action(
        &action(scope.actor(SUBJECT), scope.item(item), 0),
        &scope,
        &view
    )
    .is_ok());

    let command = Command::AdmitIntention {
        expected_revision: 7,
        action: backend,
    };
    let decoded = decode_command(&p::Command::Act {
        expected_revision: 7,
        action: wire.clone(),
    })
    .unwrap();
    assert!(
        decoded.matches(&command, &scope),
        "durable receipt comparison does not resolve live equipment"
    );
    let mut changed = wire;
    let p::Action::UseAbility { invocation, .. } = &mut changed else {
        unreachable!()
    };
    invocation.weapon = p::WeaponSelection::Default;
    assert!(!decode_command(&p::Command::Act {
        expected_revision: 7,
        action: changed
    })
    .unwrap()
    .matches(&command, &scope));
}

#[test]
fn projectile_delivery_preserves_exact_owned_operands_without_hidden_profile_authority() {
    let (mut view, _, scope) = fixture();
    view.inventory.clear();
    let launcher = s::ItemId(9_007_199_254_740_995);
    let ammunition = s::ItemId(9_007_199_254_740_997);
    let slot = s::EquipmentSlotView {
        id: s::content_ids::EquipmentSlot::new("hand").unwrap(),
        name: "Hand".into(),
        equipped_phrase: "in hand".into(),
        requires_attack: true,
    };
    let inventory = [launcher, ammunition]
        .into_iter()
        .map(|item| {
            // Unknown appearance and no numeric/profile facts are sufficient for
            // identity resolution; actual delivery compatibility belongs simulation.
            view.inventory.push(s::ItemView {
                corpse: None,
                class: s::ItemClass::Misc,
                description: "Unknown item".into(),
                id: item,
                name: "unknown item".into(),
                quantity: 2,
                appearance: "unknown".into(),
                identified: false,
                known_supply: None,
                known_currency: None,
                asset: None,
            });
            s::ItemInteractionView {
                item,
                slot: None,
                equipped_slot: (item == launcher).then_some(s::EquipmentSlotId(0)),
                known_projectile_bonus: None,
                known_equipment: None,
                known_cursed: None,
                known_protected: None,
                drinkable: false,
                readable: false,
                zappable: false,
                device_activation: None,
                charges: None,
                edible: false,
            }
        })
        .collect();
    view.interactions = Some(s::InteractionView {
        identification: None,
        completed: Vec::new(),
        refused: Vec::new(),
        curse_blocked: Vec::new(),
        slots: vec![slot.clone(), slot],
        preparation: None,
        inventory,
    });
    let throw = |item, socket, equipped| p::ProjectileDeliverySelection::Throw {
        source: if equipped {
            p::ThrownSourceSelection::Equipped { item, socket }
        } else {
            p::ThrownSourceSelection::Carried { item, socket }
        },
    };
    let launch = |item, socket, ammunition| p::ProjectileDeliverySelection::Launch {
        launcher: p::EquippedProjectileWeapon { item, socket },
        ammunition,
    };
    let action = |delivery| p::Action::UseAbility {
        id: "fixture_projectile".into(),
        invocation: p::AbilityInvocation::directional_projectile(p::Direction::NorthEast, delivery),
        supplies: p::SupplySelection::Carried,
    };
    let deliveries = [
        throw(scope.item(launcher), 0, true),
        throw(scope.item(ammunition), 1, false),
        launch(scope.item(launcher), 0, scope.item(ammunition)),
    ];
    for delivery in deliveries {
        let wire = action(delivery);
        let before = view.clone();
        let backend = decode_action(&wire, &scope, &view).unwrap();
        assert_eq!(encode_action(&backend, &scope), wire);
        assert_eq!(
            crate::adapt::recorded_action(crate::adapt::action(&backend)),
            Some(backend.clone())
        );
        assert_eq!(view, before);
        let command = Command::AdmitIntention {
            expected_revision: 7,
            action: backend,
        };
        let decoded = decode_command(&p::Command::Act {
            expected_revision: 7,
            action: wire,
        })
        .unwrap();
        assert!(
            decoded.matches(&command, &scope),
            "receipt comparison needs no live item state"
        );
    }
    let foreign = TargetScope::new(uuid::Uuid::from_u128(8), view.actor);
    let invalid = [
        throw(foreign.item(launcher), 0, true),
        throw(scope.item(s::ItemId(launcher.0 + 100)), 0, true),
        throw(scope.apparent_object(SUBJECT), 0, true),
        throw(scope.item(launcher), 1, true),
        throw(scope.item(launcher), 0, false),
        throw(scope.item(ammunition), 0, true),
        throw(scope.item(ammunition), 2, false),
        throw(scope.item(ammunition), 64, false),
        launch(scope.item(launcher), 0, foreign.item(ammunition)),
        launch(scope.item(ammunition), 1, scope.item(launcher)),
        launch(scope.item(launcher), 0, scope.item(launcher)),
    ];
    let unavailable = decode_action(&action(invalid[0]), &scope, &view).unwrap_err();
    for delivery in invalid {
        assert_eq!(
            decode_action(&action(delivery), &scope, &view).unwrap_err(),
            unavailable
        );
    }
    for delivery in deliveries {
        let wire = action(delivery);
        let mut no_inventory = view.clone();
        no_inventory.inventory.clear();
        let mut no_interactions = view.clone();
        no_interactions.interactions = None;
        let mut no_offer = view.clone();
        no_offer.interactions.as_mut().unwrap().inventory.clear();
        let mut no_anatomy = view.clone();
        no_anatomy.interactions.as_mut().unwrap().slots.clear();
        for unavailable_view in [no_inventory, no_interactions, no_offer, no_anatomy] {
            assert_eq!(
                decode_action(&wire, &scope, &unavailable_view).unwrap_err(),
                unavailable
            );
        }
        let mut conflicting = wire.clone();
        let p::Action::UseAbility { invocation, .. } = &mut conflicting else {
            unreachable!()
        };
        invocation.weapon = p::WeaponSelection::Equipped {
            item: scope.item(launcher),
            socket: 0,
        };
        assert_eq!(
            decode_action(&conflicting, &scope, &view).unwrap_err(),
            unavailable
        );
        assert!(decode_action(
            &wire,
            &TargetScope::new(uuid::Uuid::from_u128(7), s::ActorId(view.actor.0 + 1)),
            &view
        )
        .is_err());
    }
    let mut plain_ammunition = view.clone();
    plain_ammunition
        .interactions
        .as_mut()
        .unwrap()
        .inventory
        .retain(|offer| offer.item != ammunition);
    let wire = action(deliveries[2]);
    let backend = decode_action(&wire, &scope, &plain_ammunition).unwrap();
    assert_eq!(
        encode_action(&backend, &scope),
        wire,
        "plain ammunition needs no equipment/use offer"
    );
    plain_ammunition
        .inventory
        .retain(|item| item.id != ammunition);
    assert_eq!(
        decode_action(&wire, &scope, &plain_ammunition).unwrap_err(),
        unavailable,
        "unowned or ground ammunition is not guessed"
    );
    let mut equipped_ammunition = view.clone();
    equipped_ammunition.interactions.as_mut().unwrap().inventory[1].equipped_slot =
        Some(s::EquipmentSlotId(1));
    assert_eq!(
        decode_action(&wire, &scope, &equipped_ammunition).unwrap_err(),
        unavailable
    );
    // Reclassification and freshness do not silently fall back to another item.
    view.interactions.as_mut().unwrap().inventory[0].equipped_slot = None;
    assert_eq!(
        decode_action(&action(deliveries[0]), &scope, &view).unwrap_err(),
        unavailable
    );
    assert_eq!(
        decode_action(&action(deliveries[2]), &scope, &view).unwrap_err(),
        unavailable
    );
}

#[test]
fn trap_projection_and_examination_use_only_actor_scoped_disclosed_presence() {
    let (mut observation, scene, scope) = fixture();
    let location = observation.visible_cells[0].location;
    observation.visible_cells[0].trap = Some(s::TrapView {
        id: 73,
        family: None,
        reachable: true,
    });
    let action = request(json!({"type":"examine_trap", "trap":scope.trap(73)}));
    assert_eq!(
        decode_action(&action, &scope, &observation).unwrap(),
        a::Action::ExamineTrap { trap: 73 }
    );
    let wire = projected(observation.clone(), scene.clone(), &scope);
    let trap = wire["visible_cells"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|cell| cell.get("trap").filter(|trap| !trap.is_null()))
        .unwrap();
    assert_eq!(trap["family"], Value::Null);
    assert_eq!(trap["id"], scope.trap(73).to_string());
    assert_eq!(trap.as_object().unwrap().len(), 3);
    let other = TargetScope::new(uuid::Uuid::from_u128(7), s::ActorId(2));
    assert!(decode_action(
        &request(json!({"type":"examine_trap", "trap":other.trap(73)})),
        &scope,
        &observation
    )
    .is_err());
    observation
        .visible_cells
        .iter_mut()
        .filter(|cell| cell.location == location)
        .for_each(|cell| cell.trap = None);
    assert!(decode_action(&action, &scope, &observation).is_err());
    let text = projected(observation, scene, &scope).to_string();
    assert!(!text.contains(&scope.trap(73).to_string()));
}

#[test]
fn concealed_closure_projection_uses_disclosed_wall_not_raw_scene_and_has_no_door_capability() {
    let (mut observation, mut scene, scope) = fixture();
    let location = observation.visible_cells[0].location;
    let frame = observation.visible_cells[0].frame;
    let offset = scene
        .iter()
        .find(|cell| cell.location == location && cell.rotation == frame)
        .unwrap()
        .offset;
    let cell = &mut observation.visible_cells[0];
    cell.wall = true;
    cell.material = "stone";
    cell.door = None;
    cell.door_secret = false;
    cell.door_reachable = false;
    cell.door_approaches.clear();
    for cell in scene.iter_mut().filter(|cell| cell.location == location) {
        cell.wall = false; // Shared scene geometry is not observer-specific disclosure.
    }
    let action = request(json!({"type":"set_door", "door":scope.door(79), "open":true}));
    assert!(decode_action(&action, &scope, &observation).is_err());
    let wire = projected(observation, scene, &scope);
    assert!(!wire.to_string().contains(&scope.door(79).to_string()));
    let wall = wire["visible_cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|cell| cell["position"] == json!({"x":offset.x,"y":offset.y,"z":offset.z}))
        .unwrap();
    assert_eq!(wall["wall"], true);
    assert_eq!(wall["material"], "stone");
    assert!(wall.get("door").is_none_or(Value::is_null));
}
