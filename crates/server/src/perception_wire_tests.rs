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
        "invocation": {"kind": "actor_target", "target": target},
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
        attack(scope.actor(SUBJECT)),
        attack(scope.contact(SUBJECT)),
        p::Action::UseAbility {
            id: "basic_melee".into(),
            invocation: p::AbilityInvocation::ActorTarget {
                target: scope.actor(view.actor),
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
    assert_eq!(item["supply_reachable"], false);
    assert!(wire["visible_actors"].as_array().unwrap().is_empty());
    withheld(&wire, &scope);
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
        invocation: a::AbilityInvocation::ContactTarget { target }, ..
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
            invocation: a::AbilityInvocation::ContactTarget { .. },
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
        slots: vec![],
        preparation: None,
        inventory: vec![],
        identification: Some(s::IdentificationChoiceView {
            choice,
            class: s::ItemClass::Wand,
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
            direction: p::Direction::SouthWest,
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
