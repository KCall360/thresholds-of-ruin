//! Ordinary admitted self-device delivery, immutable preparation and source-free replay.
use super::*;

fn nothing_source(root: &Path, with_other: bool) -> crate::Scenario {
    self_device_source(root, with_other, false)
}

fn self_device_source(root: &Path, with_other: bool, condition: bool) -> crate::Scenario {
    use tor_simulation::{
        abilities::InvocationKind,
        rules::{Interaction, Reach, StrikeSource},
    };
    let base = scenario(with_other);
    let source = base.package.as_ref().unwrap();
    let mut document = source.rules().unwrap().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    ability.invocation = InvocationKind::ActorTarget;
    ability.interaction = Interaction::NonHostile;
    ability.reach = Reach::SelfOnly;
    ability.ai.hostile = false;
    ability.tags.clear();
    let StrikeSource::Authored { strikes } = &mut ability.strikes else {
        panic!("authored fixture")
    };
    assert_eq!(strikes.len(), 1);
    strikes[0].check = None;
    strikes[0].on_success.clear();
    strikes[0].on_failure.clear();
    strikes[0].descriptors.clear();
    strikes[0].riders.clear();
    if condition {
        strikes[0]
            .on_success
            .push(tor_simulation::rules::Effect::ApplyCondition {
                condition: tor_simulation::content_ids::ConditionId::new("fixture.self_device")
                    .unwrap(),
                duration: 2000,
            });
        document.conditions.push(
            serde_json::from_value(json!({
                "id":"fixture.self_device", "family":"fixture.self_device", "lifecycle":"timed",
                "name":"Self device", "modifiers":[], "stacking":{"kind":"keep"},
                "source_scoped":false, "descriptors":[], "capacity":"reject",
                "timing":{"kind":"actor_active","on_expiry":{"kind":"remove"}},
                "restrictions":[], "preparation":"preserve"
            }))
            .unwrap(),
        );
    }
    assert_eq!(document.constants.item_timing.consume_turns, 1);
    // Preserve original valid timing100/100, cost-free source and real compiler.
    let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
    let validated = rules
        .document()
        .abilities
        .iter()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    assert!(
        validated.cost.is_none()
            && validated.cooldown.is_none()
            && validated.supply_costs.is_empty()
    );
    let rules = toml::to_string(rules.document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["archetypes"]["ember"]["name"] = json!("wand of nothing");
    manifest["archetypes"]["ember"]["identity"] = json!("fixture.nothing");
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("never");
    let manifest = serde_json::from_value(manifest).unwrap();
    let regions = source.region_defs().unwrap();
    let checked = Package::from_parts(manifest, regions, &rules).unwrap();
    checked.check().unwrap();
    // Use the ordinary source writer so deletion below is real filesystem
    // absence, not only replacement of an in-memory Scenario argument.
    crate::scenario_package::write_package(
        root,
        &checked.manifest,
        &checked.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}

fn nothing_actor(saved: &Value, actor: ActorId) -> &Value {
    let index = saved["game"]["actors"].as_u64().unwrap() as usize;
    &saved["shared"]["actors"][index][actor.0.to_string()]
}
fn nothing_items(saved: &Value) -> &serde_json::Map<String, Value> {
    let index = saved["game"]["items"].as_u64().unwrap() as usize;
    saved["shared"]["items"][index].as_object().unwrap()
}
fn nothing_wand_id(wire: &Wire, item: ItemTarget) -> String {
    match wire
        .service
        .engine
        .decode_action(
            OWNER,
            &Action::Zap {
                item,
                aim: DeviceAim::Direction {
                    direction: Direction::East,
                },
            },
        )
        .unwrap()
    {
        crate::actions::Action::Zap { item, .. } => item.to_string(),
        _ => panic!("resolved ordinary item handle"),
    }
}
fn nothing_affordance(snapshot: &Snapshot, item: ItemTarget) -> &ItemInteractionView {
    snapshot
        .state
        .observation
        .interactions
        .as_ref()
        .unwrap()
        .inventory
        .iter()
        .find(|r| r.item == item)
        .unwrap()
}
fn nothing_identified(snapshot: &Snapshot, item: ItemTarget) -> bool {
    snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|r| r.id == item)
        .unwrap()
        .identified
}

#[test]
fn paid_nothing_direction_and_known_self_spend_once_without_rng_or_identification() {
    for (known, aim) in [
        (
            false,
            DeviceAim::Direction {
                direction: Direction::East,
            },
        ),
        (
            false,
            DeviceAim::Direction {
                direction: Direction::West,
            },
        ),
        (
            true,
            DeviceAim::Direction {
                direction: Direction::East,
            },
        ),
        (true, DeviceAim::SelfTarget {}),
    ] {
        let source = tempfile::tempdir().unwrap();
        let mut wire = Wire::new(Engine::memory(nothing_source(source.path(), false)).unwrap());
        let wand = wire.take(ItemClass::Wand);
        let physical = nothing_wand_id(&wire, wand);
        let unknown = wire.snapshot(OWNER);
        assert!(!nothing_identified(&unknown, wand));
        assert_eq!(
            nothing_affordance(&unknown, wand).device_activation,
            Some(DeviceActivation::Direction)
        );
        let invalid = wire.request(
            OWNER,
            Action::Zap {
                item: wand,
                aim: DeviceAim::SelfTarget {},
            },
        );
        let untouched = wire.checkpoint();
        rejected(wire.send(OWNER, "unknown-self", invalid));
        assert_eq!(wire.checkpoint(), untouched);
        if known {
            let scroll = wire.take(ItemClass::Scroll);
            wire.act("read-identify", Action::Read { item: scroll });
            let paid = choice(&wire.snapshot(OWNER));
            wire.act(
                "identify",
                Action::Identify {
                    choice: paid.choice,
                    item: Some(wand),
                },
            );
            assert_eq!(
                nothing_affordance(&wire.snapshot(OWNER), wand).device_activation,
                Some(DeviceActivation::SelfTarget)
            );
        }
        let request = wire.request(OWNER, Action::Zap { item: wand, aim });
        let before = wire.checkpoint();
        assert_eq!(nothing_actor(&before, OWNER)["turn_ticks"], 100);
        assert_eq!(nothing_items(&before)[&physical]["charges"], 3);
        let admitted = ack(wire.send(OWNER, "nothing-once", request.clone()));
        let completed = wire.finish(OWNER, "nothing-once", &request);
        assert_eq!(admitted.entry_id(), completed.entry_id());
        let after = wire.checkpoint();
        assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
        assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
        // Existing source has speed0, turn100, consume_turns1; only one owner.
        assert_eq!(
            after["game"]["tick"].as_u64().unwrap(),
            before["game"]["tick"].as_u64().unwrap() + 100
        );
        let view = wire.snapshot(OWNER);
        assert_eq!(nothing_identified(&view, wand), known);
        assert_eq!(nothing_affordance(&view, wand).charges, known.then_some(2));
        assert_eq!(ack(wire.send(OWNER, "nothing-once", request)), completed);
        assert_eq!(wire.checkpoint(), after);
    }
}

#[test]
fn paused_nothing_preserves_direction_and_payload_across_identification_and_deleted_source_reopen()
{
    for interval in [1, 4096] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("source");
        let path = directory.path().join("nothing.db");
        let policy = crate::SavePolicy {
            checkpoint_interval: interval,
            ..Default::default()
        };
        let mut wire = Wire::new(
            Engine::open_with_policy(&path, nothing_source(&root, true), policy.clone()).unwrap(),
        );
        let scroll = wire.take(ItemClass::Scroll);
        let wand = wire.take(ItemClass::Wand);
        let physical = nothing_wand_id(&wire, wand);
        // Pay for a genuine outstanding selection before starting the unknown
        // device. Identify itself is zero-time, not an injected knowledge edit.
        wire.act("pay-identify", Action::Read { item: scroll });
        let selection = choice(&wire.snapshot(OWNER));
        let request = wire.request(
            OWNER,
            Action::Zap {
                item: wand,
                aim: DeviceAim::Direction {
                    direction: Direction::West,
                },
            },
        );
        let admitted = ack(wire.send(OWNER, "frozen-nothing", request.clone()));
        wire.service.run_until_blocked();
        wire.drain(OWNER);
        wire.drain(OTHER);
        let started = ack(wire.send(OWNER, "frozen-nothing", request.clone()));
        assert!(matches!(
            started,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ));
        assert_eq!(started.entry_id(), admitted.entry_id());
        let pending = wire.checkpoint();
        let previous = nothing_actor(&pending, OWNER);
        assert_eq!(previous["pending"]["active"], true);
        assert_eq!(previous["pending"]["remaining"], 100);
        let preparation: tor_simulation::combat::Preparation =
            serde_json::from_value(previous["pending"].clone()).unwrap();
        // Public typed equivalent of has_prefix: only an Ability next_strike>0
        // is committed. Do not infer prefix absence from a missing JSON field.
        match &preparation.progress {
            tor_simulation::combat::PreparationProgress::Ability { next_strike, .. } => {
                assert_eq!(*next_strike, 0)
            }
            tor_simulation::combat::PreparationProgress::Item {}
            | tor_simulation::combat::PreparationProgress::Ray {} => {}
            other => panic!("unexpected device progress: {other:?}"),
        }
        assert_eq!(nothing_items(&pending)[&physical]["charges"], 3);
        let frozen = &previous["pending"]["plan"];
        assert_eq!(
            frozen["aim"],
            json!({"kind":"direction","direction":"west"})
        );
        assert_eq!(frozen["delivery"]["kind"], "self_effect");
        assert_eq!(frozen["delivery"]["plan"]["actor"], OWNER.0);
        assert_eq!(frozen["delivery"]["plan"]["ability"], "magic.ember_ray");
        assert_eq!(frozen["delivery"]["plan"]["effects"], json!([]));
        let plan: tor_simulation::abilities::Plan = serde_json::from_value(frozen.clone()).unwrap();
        assert_eq!(
            plan.work(),
            tor_simulation::Work::Zap {
                item: tor_simulation::ItemId(physical.parse().unwrap()),
                aim: tor_simulation::DeviceAim::Direction {
                    direction: tor_simulation::abilities::StepDirection::West
                },
            }
        );
        // Freeze at an actual started boundary, then exercise the supported
        // control-release pause rather than pretending queue admission is work.
        wire.send(OWNER, "pause", Request::ReleaseControl);
        ack(wire.send(OWNER, "reacquire", Request::AcquireControl));
        let paused = wire.checkpoint();
        assert_eq!(nothing_actor(&paused, OWNER)["pending"]["active"], false);
        assert_eq!(
            nothing_actor(&paused, OWNER)["pending"]["plan"],
            previous["pending"]["plan"]
        );
        assert_eq!(
            nothing_actor(&paused, OWNER)["pending"]["progress"],
            previous["pending"]["progress"]
        );
        assert_eq!(paused["game"]["tick"], pending["game"]["tick"]);
        assert_eq!(
            wire.service.engine.next_actor(),
            Some(OWNER),
            "owner must actually be executable, not only admit input"
        );
        let ready = wire.snapshot(OWNER);
        assert!(ready.has_control && ready.readiness.admission);
        assert_eq!(ready.readiness.resume.len(), 1);
        assert!(
            nothing_actor(&paused, OWNER)["ready_at"].as_u64().unwrap()
                <= paused["game"]["tick"].as_u64().unwrap()
        );
        let identify = wire.request(
            OWNER,
            Action::Identify {
                choice: selection.choice,
                item: Some(wand),
            },
        );
        ack(wire.send(OWNER, "identify-during-pause", identify.clone()));
        // Unlike Wire::finish, this boundary has no helper-generated peer Wait.
        // A setup ordering mistake must fail here, not advance time to hide it.
        wire.service.run_until_blocked();
        wire.drain(OWNER);
        wire.drain(OTHER);
        let identified = ack(wire.send(OWNER, "identify-during-pause", identify));
        assert!(matches!(
            identified,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Resolved,
                ..
            }
        ));
        let learned = wire.checkpoint();
        assert_eq!(
            nothing_actor(&learned, OWNER)["pending"]["plan"],
            previous["pending"]["plan"]
        );
        assert_eq!(
            nothing_actor(&learned, OWNER)["pending"]["remaining"],
            previous["pending"]["remaining"]
        );
        assert_eq!(learned["game"]["tick"], pending["game"]["tick"]);
        assert_eq!(learned["game"]["rules_rng"], pending["game"]["rules_rng"]);
        assert_eq!(
            nothing_affordance(&wire.snapshot(OWNER), wand).device_activation,
            Some(DeviceActivation::SelfTarget)
        );
        wire.service.engine.flush().unwrap();
        let clock_watermark = wire.serial;
        drop(wire);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(!root.exists());
        let engine =
            Engine::open_with_policy(&path, crate::Scenario::two_room(999), policy).unwrap();
        assert_eq!(
            checkpoint(&engine),
            learned,
            "restore frozen facts before Service attachment"
        );
        if interval == 4096 {
            assert!(engine.recovery_profile().records_replayed > 0);
        }
        let mut wire = Wire::new(engine); // exactly two real external owners, no AI conversion
                                          // Request receipts are durable across reconnect; keep the existing
                                          // caller's sequence instead of reusing clock-1 for different work.
        wire.serial = clock_watermark;
        assert!(wire.serial.checked_add(1).unwrap() > clock_watermark);
        let snapshot = wire.snapshot(OWNER);
        assert_eq!(snapshot.readiness.resume.len(), 1);
        let resume = Request::Command {
            context: snapshot.reply_context().input,
            branch: snapshot.branch.clone(),
            command: Command::ResumeIntention {
                intention: snapshot.readiness.resume[0].clone(),
            },
        };
        ack(wire.send(OWNER, "resume-captured", resume));
        let completed = wire.finish(OWNER, "frozen-nothing", &request);
        assert_eq!(completed.entry_id(), admitted.entry_id());
        let after = wire.checkpoint();
        assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
        assert_eq!(after["game"]["rules_rng"], pending["game"]["rules_rng"]);
        assert_eq!(
            nothing_affordance(&wire.snapshot(OWNER), wand).charges,
            Some(2)
        );
        assert_eq!(ack(wire.send(OWNER, "frozen-nothing", request)), completed);
        assert_eq!(wire.checkpoint(), after);
    }
}

#[test]
fn followed_spectator_uses_followed_identity_for_device_prompt_not_owner_only_gate() {
    let source = tempfile::tempdir().unwrap();
    let mut wire = Wire::new(Engine::memory(nothing_source(source.path(), false)).unwrap());
    let wand = wire.take(ItemClass::Wand);
    let mut spectator = wire
        .service
        .connect(
            &Account {
                user: "nothing-follower".into(),
                token: "fixture".into(),
                role: AccessRole::Spectator,
                actors: BTreeSet::from([OWNER]),
            },
            "nothing-follower".into(),
        )
        .unwrap();
    spectator.messages.try_recv_frame().unwrap();
    wire.service.handle(
        spectator.id,
        "follow".into(),
        Request::Attach { actor: OWNER },
    );
    let response = decode_response(&spectator.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = response else {
        panic!("follow snapshot")
    };
    assert!(!nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    let scroll = wire.take(ItemClass::Scroll);
    wire.act("read-for-follow", Action::Read { item: scroll });
    let paid = choice(&wire.snapshot(OWNER));
    wire.act(
        "learn-for-follow",
        Action::Identify {
            choice: paid.choice,
            item: Some(wand),
        },
    );
    while spectator.messages.try_recv_frame().is_ok() {}
    wire.service.handle(
        spectator.id,
        "known-follow-snapshot".into(),
        Request::Snapshot,
    );
    let response = decode_response(&spectator.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = response else {
        panic!("follow snapshot")
    };
    assert!(nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(
        nothing_affordance(&snapshot, wand).charges,
        None,
        "separate exact charge privacy remains"
    );
    assert!(!snapshot.has_control);
}

#[test]
fn paid_self_device_applies_nonempty_captured_condition_to_user_not_direction_recipient() {
    let source = tempfile::tempdir().unwrap();
    let mut wire =
        Wire::new(Engine::memory(self_device_source(source.path(), true, true)).unwrap());
    let wand = wire.take(ItemClass::Wand);
    let physical = nothing_wand_id(&wire, wand);
    let before = wire.checkpoint();
    for actor in [OWNER, OTHER] {
        let view = wire.snapshot(actor);
        assert!(!view
            .state
            .observation
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap()
            .conditions
            .iter()
            .any(|row| row.condition.id == "fixture.self_device"));
    }
    assert!(!nothing_identified(&wire.snapshot(OWNER), wand));
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::East,
            },
        },
    );
    let admitted = ack(wire.send(OWNER, "self-condition", request.clone()));
    let completed = wire.finish(OWNER, "self-condition", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    let owner = wire.snapshot(OWNER);
    let own = &owner
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions;
    let matches: Vec<_> = own
        .iter()
        .filter(|row| row.condition.id == "fixture.self_device")
        .collect();
    let [applied] = matches.as_slice() else {
        panic!("exactly one condition on the actual device user")
    };
    assert_eq!(applied.phase, ConditionPhase::Active);
    assert_eq!(applied.remaining_ticks, Some(2000));
    let other = wire.snapshot(OTHER);
    assert!(!other
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .any(|row| row.condition.id == "fixture.self_device"));
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert!(
        !nothing_identified(&owner, wand),
        "Never discovery remains independent of an observable condition"
    );
    assert_eq!(ack(wire.send(OWNER, "self-condition", request)), completed);
    assert_eq!(wire.checkpoint(), after);
}
// OnUse discovery is completion-owned, while another actor retains independent
// knowledge of its own device with the same identity.
#[test]
fn on_use_self_condition_identifies_only_on_completion_and_other_owner_remains_unknown() {
    let root = tempfile::tempdir().unwrap();
    let base = self_device_source(root.path(), true, true);
    let package = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("on_use");
    let mut regions = package.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    // Existing second device, same actual identity, another independently
    // controlled actor. Use supported authored carriage, not inventory injection.
    let peer_item = region["items"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|row| row["id"] == 31)
        .unwrap();
    peer_item["carried_by"] = json!(OTHER.0);
    peer_item["at"] = json!([5, 2, 0]);
    regions[0] = serde_json::from_value(region).unwrap();
    let rules = toml::to_string(package.rules().unwrap().document()).unwrap();
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    package.check().unwrap();
    crate::scenario_package::write_package(
        root.path(),
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root.path()).unwrap();
    let scenario = crate::scenario_package::load(root.path(), 42, None, true).unwrap();
    let mut wire = Wire::new(Engine::memory(scenario).unwrap());
    let wand = wire.take(ItemClass::Wand);
    // Pickup resolves immediately for the owner, but the other external actor
    // is still due at tick0. Advance that real actor once before measuring the
    // zap; admission readiness alone does not imply execution readiness.
    assert_eq!(wire.service.engine.next_actor(), Some(OTHER));
    let peer_wait = wire.request(OTHER, Action::Wait);
    ack(wire.send(OTHER, "on-use-peer-ready", peer_wait.clone()));
    wire.finish(OTHER, "on-use-peer-ready", &peer_wait);
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let ready = wire.checkpoint();
    let tick = ready["game"]["tick"].as_u64().unwrap();
    assert_eq!(nothing_actor(&ready, OWNER)["ready_at"], tick);
    let peer_ready = nothing_actor(&ready, OTHER)["ready_at"].as_u64().unwrap();
    assert!(
        tick < peer_ready && peer_ready < tick + 100,
        "peer boundary must stop inside, not before or after, the 100-tick preparation"
    );
    let physical = nothing_wand_id(&wire, wand);
    let peer = wire.snapshot(OTHER);
    let peer_wand = peer
        .state
        .observation
        .inventory
        .iter()
        .find(|row| row.class == ItemClass::Wand)
        .unwrap()
        .id;
    for (actor, item) in [(OWNER, wand), (OTHER, peer_wand)] {
        let snapshot = wire.snapshot(actor);
        assert!(!nothing_identified(&snapshot, item));
        assert_eq!(
            nothing_affordance(&snapshot, item).device_activation,
            Some(DeviceActivation::Direction)
        );
    }
    let mut follower = wire
        .service
        .connect(
            &Account {
                user: "on-use-follower".into(),
                token: "fixture".into(),
                role: AccessRole::Spectator,
                actors: BTreeSet::from([OWNER]),
            },
            "on-use-follower".into(),
        )
        .unwrap();
    follower.messages.try_recv_frame().unwrap();
    wire.service.handle(
        follower.id,
        "follow".into(),
        Request::Attach { actor: OWNER },
    );
    let response = decode_response(&follower.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = response else {
        panic!("follow snapshot")
    };
    assert!(!nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::Direction)
    );

    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::East,
            },
        },
    );
    let before = wire.checkpoint();
    assert_eq!(
        nothing_items(&before)[&physical]["spec"]["identity"],
        "fixture.nothing"
    );
    assert_eq!(
        nothing_items(&before)["31"]["spec"]["identity"],
        "fixture.nothing"
    );
    assert_eq!(
        nothing_items(&before)["31"]["location"],
        json!({"Carried": OTHER.0})
    );
    assert_eq!(nothing_items(&before)["31"]["charges"], 3);
    let admitted = ack(wire.send(OWNER, "on-use-self", request.clone()));
    assert!(
        !nothing_identified(&wire.snapshot(OWNER), wand),
        "admission cannot identify"
    );
    wire.service.run_until_blocked();
    wire.drain(OWNER);
    wire.drain(OTHER);
    let started = ack(wire.send(OWNER, "on-use-self", request.clone()));
    assert!(
        matches!(
            started,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ),
        "actual preparation receipt: {started:?}"
    );
    assert_eq!(started.entry_id(), admitted.entry_id());
    let pending = wire.checkpoint();
    assert_eq!(nothing_items(&pending)[&physical]["charges"], 3);
    let snapshot = wire.snapshot(OWNER);
    assert!(
        snapshot
            .state
            .observation
            .interactions
            .as_ref()
            .unwrap()
            .preparation
            .as_ref()
            .unwrap()
            .remaining
            > 0
    );
    assert!(
        !nothing_identified(&snapshot, wand),
        "preparation cannot identify"
    );
    assert!(!snapshot
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .any(|c| c.condition.id == "fixture.self_device"));
    let completed = wire.finish(OWNER, "on-use-self", &request);
    assert_eq!(completed.entry_id(), admitted.entry_id());
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    let owner = wire.snapshot(OWNER);
    assert!(nothing_identified(&owner, wand));
    assert_eq!(
        nothing_affordance(&owner, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(
        nothing_affordance(&owner, wand).charges,
        None,
        "identity is not exact balance knowledge"
    );
    let condition = owner
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .find(|c| c.condition.id == "fixture.self_device")
        .unwrap();
    assert_eq!(condition.remaining_ticks, Some(2000));
    let peer = wire.snapshot(OTHER);
    assert!(!nothing_identified(&peer, peer_wand));
    assert_eq!(
        nothing_affordance(&peer, peer_wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert_eq!(nothing_affordance(&peer, peer_wand).charges, None);
    assert_eq!(nothing_items(&after)["31"]["charges"], 3);
    while follower.messages.try_recv_frame().is_ok() {}
    wire.service
        .handle(follower.id, "completed-follow".into(), Request::Snapshot);
    let response = decode_response(&follower.messages.try_recv_frame().unwrap().text).unwrap();
    let ServerMessage::Snapshot { snapshot, .. } = response else {
        panic!("follow snapshot")
    };
    assert!(nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(nothing_affordance(&snapshot, wand).charges, None);
    assert!(!snapshot.has_control);
    assert_eq!(ack(wire.send(OWNER, "on-use-self", request)), completed);
    assert_eq!(wire.checkpoint(), after);
}

#[test]
fn late_store_rejection_rolls_back_self_effect_charge_and_on_use_identity() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("self-effect.db");
    let source = self_device_source(&root, false, true);
    let package = source.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&package.manifest).unwrap();
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("on_use");
    let rules = toml::to_string(package.rules().unwrap().document()).unwrap();
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    package.check().unwrap();
    crate::scenario_package::write_package(
        &root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    let fixture = crate::scenario_package::load(&root, 42, None, true).unwrap();
    let mut wire = Wire::new(Engine::open(&path, fixture.clone()).unwrap());
    let wand = wire.take(ItemClass::Wand);
    let physical = nothing_wand_id(&wire, wand);
    let observed = wire.snapshot(OWNER);
    assert!(!nothing_identified(&observed, wand));
    assert_eq!(
        nothing_affordance(&observed, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert!(!observed
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .any(|c| c.condition.id == "fixture.self_device"));
    assert_eq!(nothing_items(&wire.checkpoint())[&physical]["charges"], 3);
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    let admitted = ack(wire.send(OWNER, "atomic-self", request.clone()));
    wire.service.engine.flush().unwrap();
    drop(wire);
    let saved = crate::inspect_save(&path).unwrap();
    let mut engine = Engine::open_with_policy(
        &path,
        fixture.clone(),
        crate::SavePolicy {
            max_pending_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let before = checkpoint(&engine);
    // Prove the storage fault will reject this still-unexecuted self payload.
    assert_eq!(nothing_items(&before)[&physical]["charges"], 3);
    let observed = engine.observation(OWNER).unwrap();
    assert!(
        !observed
            .inventory
            .iter()
            .find(|row| row.id == wand)
            .unwrap()
            .identified
    );
    assert!(!observed
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions
        .iter()
        .any(|row| row.condition.id == "fixture.self_device"));
    let RequestReceipt::Admitted {
        intention, phase, ..
    } = &admitted
    else {
        panic!("actual admission")
    };
    assert_eq!(*phase, IntentionPhase::Queued);
    let statuses = engine.pending_intentions(OWNER);
    assert_eq!(statuses.len(), 1);
    assert_eq!(&statuses[0].intention, intention);
    assert_eq!(statuses[0].phase, IntentionPhase::Queued);
    let sequence = engine.save_status().accepted_sequence;
    for _ in 0..2 {
        let error = engine.execute_next_intention().unwrap_err();
        assert_eq!(error.code, ErrorCode::StorageFailure);
        assert_eq!(
            error.message,
            "Save queue is full; wait for saving to finish before retrying"
        );
        assert_eq!(checkpoint(&engine), before);
        assert_eq!(engine.save_status().accepted_sequence, sequence);
        assert_eq!(crate::inspect_save(&path).unwrap(), saved);
    }
    drop(engine);
    let mut engine = Engine::open(&path, fixture).unwrap();
    engine.execute_next_intention().unwrap().unwrap();
    let after = checkpoint(&engine);
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    let mut wire = Wire::new(engine);
    let observed = wire.snapshot(OWNER);
    assert!(nothing_identified(&observed, wand));
    assert_eq!(
        nothing_affordance(&observed, wand).charges,
        None,
        "OnUse identity does not reveal the exact charge balance"
    );
    assert_eq!(
        nothing_affordance(&observed, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    let conditions = &observed
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .conditions;
    let applied: Vec<_> = conditions
        .iter()
        .filter(|c| c.condition.id == "fixture.self_device")
        .collect();
    assert_eq!(applied.len(), 1);
    assert_eq!(applied[0].remaining_ticks, Some(2000));
    let settled = wire.checkpoint();
    let retry = ack(wire.send(OWNER, "atomic-self", request));
    assert_eq!(retry.entry_id(), admitted.entry_id());
    assert!(matches!(
        retry,
        RequestReceipt::Admitted {
            phase: IntentionPhase::Resolved,
            ..
        }
    ));
    assert_eq!(wire.checkpoint(), settled);
}

// No source/catalog mutation: production resolved archetypes and rules are retained.
fn imported_nothing_source(root: &Path) -> crate::Scenario {
    let production_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenarios/rogue-exploration");
    let production = crate::scenario_package::resolve_source(&production_root).unwrap();
    let rules =
        std::fs::read_to_string(production_root.join(&production.manifest.ruleset)).unwrap();
    let rules = resolve_rules_source(&rules, "rules.toml").unwrap().source;
    let mut manifest = serde_json::to_value(production.manifest).unwrap();
    let original_items = manifest["archetypes"].clone();
    let original_creatures = manifest["creatures"].clone();
    assert_eq!(
        original_items["rogue.wand_nothing"]["device"]["ability"],
        "rogue.wand_nothing"
    );
    assert_eq!(
        original_items["rogue.wand_nothing"]["device"]["discovery"],
        "never"
    );
    manifest["id"] = json!("imported-nothing-service");
    manifest["characters"].as_array_mut().unwrap().truncate(1);
    let owner = &mut manifest["characters"][0];
    owner["anchor"] = json!("1/start");
    owner["known_identities"] = json!([]);
    owner["loadout"] = json!([]);
    owner["creature"]["nutrition"] = json!({"kind":"exempt"});
    owner["creature"]["future_growth"] = json!({"kind":"disabled"});
    for key in [
        "procedural_worlds",
        "generation_recipes",
        "generation_groups",
        "stair_pairs",
    ] {
        manifest[key] = json!({});
    }
    for key in ["floor_activation", "unique_retrieval", "objective"] {
        manifest[key] = Value::Null;
    }
    assert_eq!(manifest["archetypes"], original_items);
    assert_eq!(manifest["creatures"], original_creatures);
    let region: crate::scenario_package::RegionDef = serde_json::from_value(json!({
        "id":1,"name":"Imported Nothing laboratory","size":[7,5,2],"chamber":true,
        "anchors":{"start":[2,2,0]},"items":[
            {"id":30,"at":[2,2,0],"archetype":"rogue.wand_nothing"},
            {"id":20,"at":[2,2,0],"archetype":"rogue.identify_wand","quantity":1}
        ]
    }))
    .unwrap();
    let checked = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        vec![region],
        &rules,
    )
    .unwrap();
    checked.check().unwrap();
    assert_eq!(
        checked
            .rules()
            .unwrap()
            .document()
            .constants
            .item_timing
            .consume_turns,
        1
    );
    crate::scenario_package::write_package(
        root,
        &checked.manifest,
        &checked.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}

#[test]
fn imported_nothing_paid_unknown_then_identified_self_survives_source_deleted_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let database = directory.path().join("nothing.db");
    let mut wire = Wire::new(Engine::open(&database, imported_nothing_source(&root)).unwrap());
    assert_eq!(wire.service.engine.actors(), vec![OWNER]);
    let wand = wire.take(ItemClass::Wand);
    let scroll = wire.take(ItemClass::Scroll);
    let physical = nothing_wand_id(&wire, wand);
    let unknown = wire.snapshot(OWNER);
    assert!(!nothing_identified(&unknown, wand));
    assert_eq!(
        nothing_affordance(&unknown, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert_eq!(nothing_affordance(&unknown, wand).charges, None);
    let bad = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    let untouched = wire.checkpoint();
    rejected(wire.send(OWNER, "unknown-self-rejected", bad));
    assert_eq!(wire.checkpoint(), untouched);
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let before = wire.checkpoint();
    let initial = nothing_items(&before)[&physical]["charges"]
        .as_u64()
        .unwrap();
    assert!((3..=7).contains(&initial));
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    let admitted = ack(wire.send(OWNER, "catalog-nothing-unknown", request.clone()));
    let completed = wire.finish(OWNER, "catalog-nothing-unknown", &request);
    assert_eq!(admitted.entry_id(), completed.entry_id());
    let after = wire.checkpoint();
    let zap_ticks =
        after["game"]["tick"].as_u64().unwrap() - before["game"]["tick"].as_u64().unwrap();
    assert!(zap_ticks > 0);
    assert_eq!(nothing_items(&after)[&physical]["charges"], initial - 1);
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert!(!nothing_identified(&wire.snapshot(OWNER), wand));
    assert_eq!(
        ack(wire.send(OWNER, "catalog-nothing-unknown", request)),
        completed
    );
    assert_eq!(wire.checkpoint(), after);

    // Independent same-source consume-turn baseline: ordinary paid Read uses
    // exactly the same turn law, without assuming Rogue's effective speed is0.
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let before_read = wire.checkpoint();
    wire.act("pay-catalog-identify", Action::Read { item: scroll });
    let after_read = wire.checkpoint();
    let read_ticks = after_read["game"]["tick"].as_u64().unwrap()
        - before_read["game"]["tick"].as_u64().unwrap();
    assert_eq!(zap_ticks, read_ticks);
    let paid = choice(&wire.snapshot(OWNER));
    wire.act(
        "catalog-identify",
        Action::Identify {
            choice: paid.choice,
            item: Some(wand),
        },
    );
    let known = wire.snapshot(OWNER);
    assert!(nothing_identified(&known, wand));
    assert_eq!(
        nothing_affordance(&known, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(
        nothing_affordance(&known, wand).charges,
        Some((initial - 1).try_into().unwrap())
    );
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let before_self = wire.checkpoint();
    let self_request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    ack(wire.send(OWNER, "catalog-nothing-self", self_request.clone()));
    let self_receipt = wire.finish(OWNER, "catalog-nothing-self", &self_request);
    let saved = wire.checkpoint();
    assert_eq!(nothing_items(&saved)[&physical]["charges"], initial - 2);
    assert_eq!(saved["game"]["rules_rng"], before_self["game"]["rules_rng"]);
    assert_eq!(
        saved["game"]["tick"].as_u64().unwrap() - before_self["game"]["tick"].as_u64().unwrap(),
        read_ticks
    );
    wire.service.engine.flush().unwrap();
    let serial = wire.serial;
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(!root.exists());
    let engine = Engine::open(&database, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&engine), saved);
    let mut wire = Wire::new(engine); // only the one real human owner
    wire.serial = serial;
    let restored = wire.snapshot(OWNER);
    assert!(nothing_identified(&restored, wand));
    assert_eq!(
        nothing_affordance(&restored, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    let retry_before = wire.checkpoint();
    assert_eq!(
        ack(wire.send(OWNER, "catalog-nothing-self", self_request)),
        self_receipt
    );
    assert_eq!(wire.checkpoint(), retry_before);
}

fn authored_room_light_source(root: &Path, passage: bool) -> crate::Scenario {
    let baseline = self_device_source(root, false, false);
    let source = baseline.package.as_ref().unwrap();
    let mut document = source.rules().unwrap().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    let tor_simulation::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
        panic!("authored self ability")
    };
    strikes[0].on_success = vec![tor_simulation::rules::Effect::RoomLight];
    let rules = toml::to_string(&document).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("on_use");
    manifest["spatial_scopes"] = json!({"study":{
        "members":[1],"rooms":if passage {json!({})}else{json!({"1":[{"region":1,"min":[0,0,0],"max":[6,4,1]}]})},
        "passages":if passage {(0..2).flat_map(|z|(0..5).map(move|y|json!({"region":1,"z":z,"y":y,"first_x":0,"last_x":6}))).collect::<Vec<_>>()}else{vec![]},"doorways":[]
    }});
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["lit"] = json!(false);
    region["lighting"] = json!([]);
    // Ordinary authored inventory avoids relying on visibility of a dark floor
    // item before testing the actual paid device. No synthetic item injection.
    region["items"] = json!([{"id":30,"at":[1,2,0],"archetype":"ember","carried_by":1}]);
    regions[0] = serde_json::from_value(region).unwrap();
    let checked =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    checked.check().unwrap();
    crate::scenario_package::write_package(
        root,
        &checked.manifest,
        &checked.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}
fn room_light_world(saved: &Value) -> tor_world::World {
    #[derive(serde::Deserialize)]
    struct Worlds(#[serde(with = "tor_world::checkpoint_worlds")] Vec<tor_world::World>);
    let worlds: Worlds = serde_json::from_value(saved["shared"]["worlds"].clone()).unwrap();
    worlds.0[saved["game"]["world"].as_u64().unwrap() as usize].clone()
}
fn paid_authored_room_light(passage: bool, stored_checkpoint: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("light.db");
    let policy = crate::SavePolicy {
        checkpoint_interval: if stored_checkpoint { 1 } else { 4096 },
        ..Default::default()
    };
    let mut wire = Wire::new(
        Engine::open_with_policy(&path, authored_room_light_source(&root, passage), policy)
            .unwrap(),
    );
    let snapshot = wire.snapshot(OWNER);
    let wand = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    assert!(!nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let physical = nothing_wand_id(&wire, wand);
    let before = wire.checkpoint();
    let at = tor_world::Location {
        region: tor_world::RegionId(1),
        position: tor_world::Position { x: 5, y: 3, z: 1 },
    };
    assert_eq!(room_light_world(&before).is_lit(at), Some(false));
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    ack(wire.send(OWNER, "room-light-once", request.clone()));
    let receipt = wire.finish(OWNER, "room-light-once", &request);
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    let known = wire.snapshot(OWNER);
    assert!(nothing_identified(&known, wand));
    assert_eq!(
        nothing_affordance(&known, wand).device_activation,
        Some(DeviceActivation::SelfTarget)
    );
    assert_eq!(nothing_affordance(&known, wand).charges, None);
    assert_eq!(
        room_light_world(&after).is_lit(at),
        Some(!passage),
        "paid RoomLight must use canonical Room versus Passage membership"
    );
    assert_eq!(ack(wire.send(OWNER, "room-light-once", request)), receipt);
    assert_eq!(wire.checkpoint(), after);
    wire.act(
        "room-light-repeat",
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    let repeated = wire.checkpoint();
    assert_eq!(nothing_items(&repeated)[&physical]["charges"], 1);
    assert_eq!(repeated["game"]["rules_rng"], after["game"]["rules_rng"]);
    assert_eq!(room_light_world(&repeated).is_lit(at), Some(!passage));
    wire.service.engine.flush().unwrap();
    if stored_checkpoint {
        assert_real_stored_checkpoint(&path, true);
    }
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), repeated);
}
#[test]
fn actual_paid_room_light_uses_canonical_room_and_survives_source_deleted_reopen() {
    paid_authored_room_light(false, false);
}
#[test]
fn actual_paid_room_light_in_explicit_passage_is_identified_paid_noop() {
    paid_authored_room_light(true, false);
}

fn assert_real_stored_checkpoint(path: &Path, spatial: bool) {
    let database =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let (sequence, payload): (i64, Vec<u8>) = database
        .query_row(
            "SELECT sequence,payload FROM checkpoint WHERE slot=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(sequence > 0);
    let decoded = crate::storage::checkpoint_codec::decode(&payload).unwrap();
    let _: crate::engine::DiskCheckpoint = crate::storage::codec::strict(&decoded).unwrap();
    let value: Value = serde_json::from_slice(&decoded).unwrap();
    assert_eq!(
        !value["shared"]["spatial"]["scopes"]
            .as_array()
            .unwrap()
            .is_empty(),
        spatial
    );
    assert!(!value["boundaries"].as_array().unwrap().is_empty());
}
#[test]
fn actual_room_light_compressed_interval_one_checkpoint_reopens_without_sources() {
    paid_authored_room_light(false, true);
}
#[test]
fn actual_passage_light_compressed_interval_one_checkpoint_reopens_without_sources() {
    paid_authored_room_light(true, true);
}
#[test]
fn ordinary_empty_spatial_compressed_interval_one_checkpoint_reopens_without_sources() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("ordinary.db");
    let scenario = nothing_source(&root, false);
    let policy = crate::SavePolicy {
        checkpoint_interval: 1,
        ..Default::default()
    };
    let mut wire = Wire::new(Engine::open_with_policy(&path, scenario, policy).unwrap());
    wire.act("ordinary-wait", Action::Wait);
    let expected = wire.checkpoint();
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, false);
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), expected);
}

#[test]
fn enabled_room_light_requires_metadata_but_unused_import_does_not() {
    let directory = tempfile::tempdir().unwrap();
    let scenario = authored_room_light_source(directory.path(), false);
    let source = scenario.package.as_ref().unwrap();
    source.check().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = source.manifest.clone();
    manifest.spatial_scopes.clear();
    let regions = source.region_defs().unwrap();
    let enabled = Package::from_parts(manifest.clone(), regions.clone(), &rules).unwrap();
    assert!(
        enabled.check().is_err(),
        "actually reachable RoomLight needs canonical metadata"
    );
    let mut unused_regions = regions;
    unused_regions[0].items.clear();
    let unused = Package::from_parts(manifest, unused_regions, &rules).unwrap();
    unused.check().unwrap();
    assert!(unused
        .manifest
        .archetypes
        .get("ember")
        .unwrap()
        .device
        .is_some());
}

fn managed_remote_room_light(foreign_face: bool, connected: bool) {
    assert!(!(foreign_face && connected));
    let remote_region = if connected { 6 } else { 2 };
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("remote-light.db");
    let base = authored_room_light_source(&root, false);
    let source = base.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    if foreign_face {
        manifest["spatial_scopes"]["study"]["rooms"]["1"][0]["max"] = json!([31, 4, 1]);
        let passages: Vec<_> = (0..2)
            .flat_map(|z| {
                (0..3).flat_map(move |y| {
                    let spans = if z == 1 && y == 1 {
                        vec![(0, 0), (2, 2)]
                    } else {
                        vec![(0, 2)]
                    };
                    spans
                        .into_iter()
                        .map(move |(a, b)| json!({"region":2,"z":z,"y":y,"first_x":a,"last_x":b}))
                })
            })
            .collect();
        manifest["spatial_scopes"]["neighbor"] =
            json!({"members":[2],"rooms":{},"passages":passages,"doorways":[]});
    } else if connected {
        manifest["spatial_scopes"]["study"]["members"] = json!([1, 2, 3, 4, 5, 6]);
        manifest["spatial_scopes"]["study"]["rooms"]["1"] = json!((1..=6)
            .map(|region| json!({"region":region,"min":[0,0,0],"max":[31,4,1]}))
            .collect::<Vec<_>>());
    } else {
        manifest["spatial_scopes"]["study"]["members"] = json!([1, 2]);
        manifest["spatial_scopes"]["study"]["rooms"]["1"]
            .as_array_mut()
            .unwrap()
            .push(json!({"region":2,"min":[0,0,0],"max":[2,2,1]}));
    }
    let creature = manifest["characters"][0]["creature"].clone();
    let mut definitions = source.region_defs().unwrap();
    let mut local = serde_json::to_value(&definitions[0]).unwrap();
    local["actors"] =
        json!([{"id":2,"at":[5,2,0],"turn_ticks":100,"controller":"external","creature":creature}]);
    if foreign_face {
        // Keep the linked target outside both observers' sight activation.
        // Linked targets are nevertheless loaded by streaming requirements,
        // so this branch proves preservation of an already-frozen source.
        local["size"] = json!([32, 5, 2]);
        local["portals"] = json!([{"at":[31,1,1],"direction":"east","to":"2/entry","kind":"boundary","width":2,"turns":1}]);
        let mouth_x = local["portals"][0]["at"][0].as_i64().unwrap();
        let peer_x = local["actors"][0]["at"][0].as_i64().unwrap();
        let owner_x = local["anchors"]["start"][0].as_i64().unwrap();
        assert!(mouth_x - peer_x > 16 && mouth_x - owner_x > 16);
    }
    if connected {
        local["size"] = json!([32, 5, 2]);
        local["anchors"]["east"] = json!([31, 2, 0]);
        local["portals"] =
            json!([{"at":[31,2,0],"direction":"east","to":"2/west","kind":"portal","height":2}]);
    }
    definitions[0] = serde_json::from_value(local).unwrap();
    let mut remote_definition = json!({"id":2,"name":"Remote surface","size":[3,3,2],"chamber":true,"lit":false,
        "actors":[{"id":3,"at":[0,0,0],"turn_ticks":100,"controller":"ai","creature":creature}]});
    if foreign_face {
        remote_definition["walls"] = json!([[1, 1, 1]]);
        remote_definition["anchors"] = json!({"entry":[2,1,1]});
    }
    if connected {
        for region in 2..=6 {
            let mut portals = vec![
                json!({"at":[0,2,0],"direction":"west","to":format!("{}/east",region-1),"kind":"portal","height":2}),
            ];
            if region < 6 {
                portals.push(json!({"at":[31,2,0],"direction":"east","to":format!("{}/west",region+1),"kind":"portal","height":2}));
            }
            let mut definition = json!({"id":region,"name":format!("Connected room member {region}"),"size":[32,5,2],"chamber":true,"lit":false,
                "anchors":{"west":[0,2,0],"east":[31,2,0]},"portals":portals});
            if region == 6 {
                definition["actors"] = remote_definition["actors"].clone();
            }
            definitions.push(serde_json::from_value(definition).unwrap());
        }
    } else {
        definitions.push(serde_json::from_value(remote_definition).unwrap());
    }
    let package = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        definitions,
        &rules,
    )
    .unwrap();
    package.check().unwrap();
    assert!(package.manifest.floor_activation.is_none());
    crate::scenario_package::write_package(
        &root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(&root).unwrap();
    let mut scenario = crate::scenario_package::load(&root, 42, None, true).unwrap();
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    let policy = crate::SavePolicy {
        checkpoint_interval: 4096,
        ..Default::default()
    };
    // Revision identities are not playable-owner authority. Keep scenario AI
    // outside client control, independently of its structural residency.
    let mut wire = Wire::for_actors(
        Engine::open_with_policy(&path, scenario, policy).unwrap(),
        [OWNER, OTHER],
    );
    assert_eq!(
        wire.clients.keys().copied().collect::<Vec<_>>(),
        vec![OWNER, OTHER],
        "never acquire control of scenario AI"
    );
    let initial = wire.checkpoint();
    if foreign_face {
        let actor = nothing_actor(&initial, ActorId(3));
        assert!(actor.is_object());
        assert_eq!(actor["ready_at"], 0);
        assert_eq!(actor["active_since"], 0);
        let lifecycle = initial["game"]["lifecycle"].as_u64().unwrap() as usize;
        let lifecycle = &initial["shared"]["lifecycles"][lifecycle];
        assert!(lifecycle["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(remote_region)));
        assert_eq!(lifecycle["stamps"]["3"], 0);
        let scopes = initial["shared"]["spatial"]["scopes"].as_array().unwrap();
        let study = scopes.iter().find(|s| s["id"] == "study").unwrap();
        assert_eq!(study["members"], json!([1]));
        assert!(study["primary"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["cells"]["region"] == 1));
        assert!(study["rooms"][0]["runs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["region"] == 2));
        assert_eq!(
            scopes.iter().find(|s| s["id"] == "neighbor").unwrap()["members"],
            json!([2]),
            "ordinary linked-region preparation installs its own primary scope"
        );
    } else {
        assert!(nothing_actor(&initial, ActorId(3)).is_null());
    }
    let remote = tor_world::Location {
        region: tor_world::RegionId(remote_region),
        position: tor_world::Position { x: 1, y: 1, z: 1 },
    };
    assert_eq!(
        room_light_world(&initial).is_lit(remote),
        if foreign_face { Some(false) } else { None }
    );
    if connected {
        let world = room_light_world(&initial);
        let near = tor_world::Location {
            region: tor_world::RegionId(2),
            position: tor_world::Position { x: 1, y: 1, z: 1 },
        };
        assert_eq!(
            world.is_lit(near),
            Some(false),
            "ordinary linked neighbor is loaded frozen"
        );
        assert_eq!(
            world.is_lit(tor_world::Location {
                region: tor_world::RegionId(3),
                ..near
            }),
            None,
            "a frozen linked neighbor is not an expansion root"
        );
        let scopes = initial["shared"]["spatial"]["scopes"].as_array().unwrap();
        assert_eq!(
            scopes.iter().find(|s| s["id"] == "study").unwrap()["members"],
            json!([1, 2, 3, 4, 5, 6])
        );
    }
    let snapshot = wire.snapshot(OWNER);
    let wand = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    let physical = nothing_wand_id(&wire, wand);
    assert_eq!(nothing_items(&initial)[&physical]["charges"], 3);
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::East,
            },
        },
    );
    ack(wire.send(OWNER, "cold-room-light", request.clone()));
    wire.service.run_until_blocked();
    wire.drain(OWNER);
    wire.drain(OTHER);
    let started = ack(wire.send(OWNER, "cold-room-light", request.clone()));
    assert!(matches!(
        started,
        RequestReceipt::Admitted {
            phase: IntentionPhase::Started,
            ..
        }
    ));
    let paused = wire.checkpoint();
    assert_eq!(paused["game"]["tick"], initial["game"]["tick"]);
    assert_eq!(room_light_world(&paused).is_lit(remote), Some(false));
    if connected {
        let world = room_light_world(&paused);
        for region in 1..6 {
            for z in 0..2 {
                let east = tor_world::Location {
                    region: tor_world::RegionId(region),
                    position: tor_world::Position { x: 31, y: 2, z },
                };
                let west = tor_world::Location {
                    region: tor_world::RegionId(region + 1),
                    position: tor_world::Position { x: 0, y: 2, z },
                };
                assert_eq!(
                    world
                        .physics_neighbor(east, tor_world::Direction::East)
                        .unwrap()
                        .0,
                    west
                );
                assert_eq!(
                    world
                        .physics_neighbor(west, tor_world::Direction::West)
                        .unwrap()
                        .0,
                    east
                );
            }
        }
    }
    let actor = nothing_actor(&paused, ActorId(3)).clone();
    assert!(actor.is_object());
    if foreign_face {
        assert_eq!(&actor, nothing_actor(&initial, ActorId(3)));
    }
    assert_eq!(actor["ready_at"], 0);
    assert_eq!(actor["active_since"], 0);
    let lifecycle = paused["game"]["lifecycle"].as_u64().unwrap() as usize;
    assert_eq!(paused["shared"]["lifecycles"][lifecycle]["stamps"]["3"], 0);
    assert!(paused["shared"]["lifecycles"][lifecycle]["frozen"]
        .as_array()
        .unwrap()
        .contains(&json!(remote_region)));
    if connected {
        for region in 2..=6 {
            assert!(paused["shared"]["lifecycles"][lifecycle]["frozen"]
                .as_array()
                .unwrap()
                .contains(&json!(region)));
        }
    }
    assert!(paused["game"]["floor_activation"].is_null());
    wire.finish(OWNER, "cold-room-light", &request);
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(after["game"]["tick"].as_u64().unwrap(), 100);
    assert_eq!(after["game"]["rules_rng"], paused["game"]["rules_rng"]);
    let mut world = room_light_world(&after);
    if foreign_face {
        assert!(
            world.is_lit(remote).is_some(),
            "active source retains its linked target loaded"
        );
    }
    if world.is_lit(remote).is_none() {
        // Read the real unpublished-to-disk immutable record carried by the
        // ordinary checkpoint. No private Game access or invented region state.
        let capture = crate::engine::Checkpoint::capture(&wire.service.engine);
        let record = capture
            .records
            .values()
            .map(|record| {
                serde_json::to_value(tor_simulation::RegionRecordData::capture(record)).unwrap()
            })
            .find(|record| record["world"]["region"]["id"] == remote_region)
            .expect("new detached remote record before4096-entry checkpoint interval");
        assert_eq!(record["actors"]["3"], actor);
        assert_eq!(record["stamps"]["3"], 0);
        let slice: tor_world::RegionSlice =
            serde_json::from_value(record["world"].clone()).unwrap();
        world.attach_region(slice).unwrap();
    } else {
        assert_eq!(nothing_actor(&after, ActorId(3)), &actor);
        let lifecycle = after["game"]["lifecycle"].as_u64().unwrap() as usize;
        assert_eq!(after["shared"]["lifecycles"][lifecycle]["stamps"]["3"], 0);
        assert!(after["shared"]["lifecycles"][lifecycle]["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(remote_region)));
    }
    assert_eq!(
        world.is_lit(remote),
        Some(true),
        "remote frozen surface must receive paid light"
    );
    if foreign_face {
        assert!(world.is_wall(remote));
        assert_eq!(
            world.is_lit(tor_world::Location {
                position: tor_world::Position { x: 0, y: 1, z: 1 },
                ..remote
            }),
            Some(false),
            "no illumination grant to foreign traversable primary"
        );
    }
    assert!(after["game"]["floor_activation"].is_null());
    wire.service.engine.flush().unwrap();
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), after);
}

#[test]
fn managed_room_light_loads_remote_same_room_surface_without_activating_actor() {
    managed_remote_room_light(false, false);
}
#[test]
fn managed_room_light_preserves_frozen_foreign_surface_without_claiming_primary_or_waking_ai() {
    managed_remote_room_light(true, false);
}

#[test]
fn managed_room_light_loads_physically_connected_far_room_member_without_waking_ai() {
    managed_remote_room_light(false, true);
}

fn paid_generated_room_light(maze: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("generated-light.db");
    let base = authored_room_light_source(&root, false);
    let source = base.package.as_ref().unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["spatial_scopes"] = json!({});
    manifest["characters"][0]["anchor"] = json!("dungeon/1/start");
    manifest["characters"][0]["loadout"] = json!([{"archetype":"ember","quantity":1}]);
    let recipe = crate::generation_recipe::GridRoomsPreset {
        version: 1,
        room_width: [30, 30],
        room_height: [30, 30],
        extra_connections: [0, 0],
        room_omissions: None,
        maze_rooms: maze.then_some(crate::generation_recipe::MazeRooms { one_in: 1 }),
        lighting: Some(crate::generation_recipe::RoomLighting {
            darkness_roll: 1,
            darkness_start: 0,
        }),
        secret_entrances: None,
        hidden_traps: None,
    }
    .to_recipe()
    .unwrap();
    manifest["generation_recipes"] = json!({"room":recipe});
    manifest["procedural_worlds"] = json!({"dungeon":{"kind":"grid_chain","version":1,"base_region":1,"grid":[2,1],"size":[32,32,2],"recipe":"room","domain":{"kind":"open","max_depth":2}}});
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), vec![], &rules).unwrap();
    package.check().unwrap();
    // The baseline wrote an authored region file. Do not let that stale file
    // collide with the genuine procedural family in the on-disk validator.
    std::fs::remove_dir_all(&root).unwrap();
    crate::scenario_package::write_package(&root, &package.manifest, &[], &rules).unwrap();
    crate::scenario_package::check(&root).unwrap();
    let scenario = crate::scenario_package::load(&root, 42, None, true).unwrap();
    let mut wire = Wire::new(Engine::open(&path, scenario).unwrap());
    let snapshot = wire.snapshot(OWNER);
    let wand = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    let physical = nothing_wand_id(&wire, wand);
    let before = wire.checkpoint();
    let at: tor_world::Location =
        serde_json::from_value(nothing_actor(&before, OWNER)["location"].clone()).unwrap();
    assert_eq!(room_light_world(&before).is_lit(at), Some(false));
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    wire.act(
        "generated-room-light",
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    let after = wire.checkpoint();
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(room_light_world(&after).is_lit(at), Some(!maze));
    assert!(nothing_identified(&wire.snapshot(OWNER), wand));
    wire.service.engine.flush().unwrap();
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), after);
}
#[test]
fn actual_paid_generated_room_light_uses_retained_room_geometry() {
    paid_generated_room_light(false);
}
#[test]
fn actual_paid_generated_maze_room_light_is_paid_identified_noop() {
    paid_generated_room_light(true);
}

// Intended placement oracle: A interior -> actor2 only; A doorway -> actors2,3;
// passage -> actors3,4. Actor4 is B doorway; actor5 is B interior.
// No inferred adjacency radius: full explicit three-dimensional partition.
fn drain_life_source(root: &Path, health: u64, caster: [i32; 3], targets: bool) -> crate::Scenario {
    let baseline = self_device_source(root, false, false);
    let source = baseline.package.as_ref().unwrap();
    let mut document = source.rules().unwrap().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    let tor_simulation::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
        panic!("authored self source")
    };
    strikes[0].on_success = vec![tor_simulation::rules::Effect::DrainLife];
    let rules = tor_simulation::rules::Ruleset::validate(document).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["archetypes"]["ember"]["device"]["discovery"] = json!("never");
    // Independent owner species; do not lower recipient Health with the caster.
    manifest["creatures"]["species"]["drain_owner"] =
        manifest["creatures"]["species"]["figure"].clone();
    manifest["characters"][0]["creature"]["species"] = json!("drain_owner");
    // Determine this fixed source's actual seeded maximum through the real catalog,
    // then adjust only its authored additive Health contribution. No seed search,
    // post-start Game mutation, or initial actor materialization assumption.
    let prior: crate::scenario_package::Manifest =
        serde_json::from_value(manifest.clone()).unwrap();
    let build = prior
        .creatures
        .compile(rules.clone())
        .unwrap()
        .prepare(
            prior.characters[0]
                .creature
                .as_ref()
                .expect("ordinary caster build"),
        )
        .unwrap()
        .instantiate(crate::creature_authoring::actor_health_seed(42, 1))
        .unwrap();
    let maximum = build.derive().unwrap().maximum_health;
    let adjustment = i64::try_from(health).unwrap() - i64::try_from(maximum).unwrap();
    manifest["creatures"]["species"]["drain_owner"]["modifiers"].as_array_mut().unwrap()
        .push(json!({"scope":"always","op":{"kind":"add","stat":{"kind":"health"},"value":adjustment}}));
    manifest["spatial_scopes"] = json!({"drain-study":{
        "members":[1],
        "rooms":{"1":[{"region":1,"min":[0,0,0],"max":[3,4,1]}],
                 "2":[{"region":1,"min":[7,0,0],"max":[10,4,1]}]},
        "passages":(0..2).flat_map(|z|(0..5).map(move|y|json!({"region":1,"z":z,"y":y,"first_x":4,"last_x":6}))).collect::<Vec<_>>(),
        "doorways":[{"room":1,"region":1,"z":0,"y":2,"first_x":3,"last_x":3},
                    {"room":2,"region":1,"z":0,"y":2,"first_x":7,"last_x":7}]
    }});
    let creature = serde_json::to_value(&source.manifest.characters[0].creature).unwrap();
    let mut definitions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&definitions[0]).unwrap();
    region["size"] = json!([11, 5, 2]);
    region["anchors"]["start"] = json!(caster);
    region["items"] = json!([{"id":30,"at":caster,"archetype":"ember","carried_by":1}]);
    region["actors"] = if targets {
        json!([(2,[1,2,0]),(3,[5,2,0]),(4,[7,2,0]),(5,[9,2,0])].into_iter()
            .map(|(id,at)|json!({"id":id,"at":at,"turn_ticks":100,"controller":"external","creature":creature})).collect::<Vec<_>>())
    } else {
        json!([])
    };
    definitions[0] = serde_json::from_value(region).unwrap();
    let rules_text = toml::to_string(rules.document()).unwrap();
    let checked = Package::from_parts(
        serde_json::from_value(manifest).unwrap(),
        definitions,
        &rules_text,
    )
    .unwrap();
    checked.check().unwrap();
    crate::scenario_package::write_package(
        root,
        &checked.manifest,
        &checked.region_defs().unwrap(),
        &rules_text,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    crate::scenario_package::load(root, 42, None, true).unwrap()
}

fn drain_health(wire: &mut Wire, actor: ActorId) -> (u64, u64) {
    let snapshot = wire.snapshot(actor);
    let combat = snapshot
        .state
        .observation
        .combat
        .as_ref()
        .expect("ordinary actor combat");
    (combat.hp, combat.max_hp)
}

fn paid_drain_life(health: u64, caster: [i32; 3], targets: bool, selected: &[u64]) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("drain.db");
    let scenario = drain_life_source(&root, health, caster, targets);
    let engine = Engine::open_with_policy(
        &path,
        scenario,
        crate::SavePolicy {
            checkpoint_interval: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let owners: Vec<_> = if targets {
        (1..=5).map(ActorId).collect()
    } else {
        vec![OWNER]
    };
    let mut wire = Wire::for_actors(engine, owners);
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    assert_eq!(drain_health(&mut wire, OWNER), (health, health));
    let before_health: BTreeMap<_, _> = (2..=if targets { 5 } else { 1 })
        .map(|id| (id, drain_health(&mut wire, ActorId(id))))
        .collect();
    let snapshot = wire.snapshot(OWNER);
    let wand = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    assert!(!nothing_identified(&snapshot, wand));
    let recovery = snapshot
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .health_recovery;
    assert!(
        recovery.period_ticks - recovery.elapsed_ticks > 100,
        "no recovery boundary during measured action"
    );
    let physical = nothing_wand_id(&wire, wand);
    let before = wire.checkpoint();
    assert_eq!(nothing_items(&before)[&physical]["charges"], 3);
    let action = Action::Zap {
        item: wand,
        aim: DeviceAim::Direction {
            direction: Direction::West,
        },
    };
    let request = wire.request(OWNER, action.clone());
    ack(wire.send(OWNER, "drain-once", request.clone()));
    let receipt = wire.finish(OWNER, "drain-once", &request);
    let after = wire.checkpoint();
    assert_eq!(
        after["game"]["tick"].as_u64().unwrap(),
        before["game"]["tick"].as_u64().unwrap() + 100
    );
    assert_eq!(after["game"]["rules_rng"], before["game"]["rules_rng"]);
    assert_eq!(
        nothing_items(&after)[&physical]["charges"],
        if health < 2 { 3 } else { 2 }
    );
    let applied = health >= 2 && !selected.is_empty();
    assert_eq!(
        drain_health(&mut wire, OWNER).0,
        if applied { health / 2 } else { health }
    );
    let share = if applied {
        health / 2 / u64::try_from(selected.len()).unwrap()
    } else {
        0
    };
    for (id, (old, maximum)) in before_health {
        assert_eq!(
            drain_health(&mut wire, ActorId(id)),
            (
                old - if selected.contains(&id) { share } else { 0 },
                maximum
            ),
            "recipient {id}"
        );
    }
    let snapshot = wire.snapshot(OWNER);
    assert!(!nothing_identified(&snapshot, wand));
    assert_eq!(
        nothing_affordance(&snapshot, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    assert_eq!(nothing_affordance(&snapshot, wand).charges, None);
    let interactions = snapshot.state.observation.interactions.as_ref().unwrap();
    if health < 2 {
        assert_eq!(
            interactions.refused,
            vec![ItemRefusalView {
                action,
                reason: ItemRefusalReason::TooWeak
            }]
        );
        assert!(interactions.completed.is_empty());
    } else {
        assert!(interactions.refused.is_empty());
    }
    if targets {
        let other = wire.snapshot(OTHER);
        assert!(other
            .state
            .observation
            .interactions
            .as_ref()
            .is_none_or(|v| v.refused.is_empty()));
    }
    assert_eq!(ack(wire.send(OWNER, "drain-once", request)), receipt);
    assert_eq!(wire.checkpoint(), after);
    wire.service.engine.flush().unwrap();
    assert_real_stored_checkpoint(&path, true);
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    let reopened = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(checkpoint(&reopened), after);
}

#[test]
fn actual_drain_life_too_weak_is_paid_completed_refusal_without_charge_or_identity() {
    paid_drain_life(1, [2, 1, 0], true, &[]);
}
#[test]
fn actual_drain_life_no_targets_spends_charge_but_no_health() {
    paid_drain_life(9, [2, 1, 0], false, &[]);
}
#[test]
fn actual_drain_life_room_doorway_and_passage_use_asymmetric_canonical_membership() {
    paid_drain_life(9, [2, 1, 0], true, &[2]);
    paid_drain_life(9, [3, 2, 0], true, &[2, 3]);
    paid_drain_life(9, [4, 2, 0], true, &[3, 4]);
}
#[test]
fn actual_drain_life_zero_share_still_sacrifices_odd_health_and_spends_once() {
    paid_drain_life(3, [4, 2, 0], true, &[3, 4]);
}

// Managed zero-share contact: same declared room, no physical link or sight.
#[test]
fn managed_drain_life_zero_share_alerts_remote_frozen_actor_without_waking_or_disclosing_it() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("remote-drain.db");
    let base = drain_life_source(&root, 3, [2, 1, 0], false);
    let source = base.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["spatial_scopes"]["drain-study"]["members"] = json!([1, 2]);
    manifest["spatial_scopes"]["drain-study"]["rooms"]["1"]
        .as_array_mut()
        .unwrap()
        .push(json!({"region":2,"min":[0,0,0],"max":[2,2,1]}));
    manifest["creatures"]["species"]["drain_remote"] =
        manifest["creatures"]["species"]["figure"].clone();
    manifest["creatures"]["species"]["drain_remote"]["behavior"]["memory_ticks"] = json!(37);
    let mut creature = manifest["characters"][0]["creature"].clone();
    creature["species"] = json!("figure");
    let mut regions = source.region_defs().unwrap();
    let mut local = serde_json::to_value(&regions[0]).unwrap();
    local["actors"] =
        json!([{"id":2,"at":[1,2,0],"turn_ticks":100,"controller":"external","creature":creature}]);
    regions[0] = serde_json::from_value(local).unwrap();
    creature["species"] = json!("drain_remote");
    creature["name"] = json!("remote drain sentinel");
    regions.push(
        serde_json::from_value(
            json!({"id":2,"name":"Remote drain recipient","size":[3,3,2],"chamber":true,"lit":false,
        "actors":[{"id":3,"at":[0,0,0],"turn_ticks":100,"controller":"ai","creature":creature}]}),
        )
        .unwrap(),
    );
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    package.check().unwrap();
    assert!(package.manifest.floor_activation.is_none());
    crate::scenario_package::write_package(
        &root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(&root).unwrap();
    let mut scenario = crate::scenario_package::load(&root, 42, None, true).unwrap();
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    // Existing managed fixture uses journal-only until explicit flush, retaining
    // the real freshly detached RegionRecord for the no-clock-advance oracle.
    let mut wire = Wire::for_actors(
        Engine::open_with_policy(
            &path,
            scenario,
            crate::SavePolicy {
                checkpoint_interval: 4096,
                ..Default::default()
            },
        )
        .unwrap(),
        [OWNER, OTHER],
    );
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    assert_eq!(drain_health(&mut wire, OWNER), (3, 3));
    let initial = wire.checkpoint();
    assert!(nothing_actor(&initial, ActorId(3)).is_null());
    let remote = tor_world::Location {
        region: tor_world::RegionId(2),
        position: tor_world::Position { x: 0, y: 0, z: 0 },
    };
    assert_eq!(room_light_world(&initial).is_lit(remote), None);
    let snapshot = wire.snapshot(OWNER);
    let wand = snapshot
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    let physical = nothing_wand_id(&wire, wand);
    assert_eq!(nothing_items(&initial)[&physical]["charges"], 3);
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    ack(wire.send(OWNER, "remote-drain", request.clone()));
    wire.service.run_until_blocked();
    wire.drain(OWNER);
    wire.drain(OTHER);
    assert!(matches!(
        ack(wire.send(OWNER, "remote-drain", request.clone())),
        RequestReceipt::Admitted {
            phase: IntentionPhase::Started,
            ..
        }
    ));
    let paused = wire.checkpoint();
    assert_eq!(paused["game"]["tick"], initial["game"]["tick"]);
    let actor = nothing_actor(&paused, ActorId(3)).clone();
    assert!(actor.is_object());
    assert_eq!(actor["ready_at"], 0);
    assert_eq!(actor["active_since"], 0);
    assert!(actor["awareness"]["alert"].is_null());
    assert!(actor["behavior"]["engagement"].is_null());
    let lifecycle = paused["game"]["lifecycle"].as_u64().unwrap() as usize;
    assert_eq!(paused["shared"]["lifecycles"][lifecycle]["stamps"]["3"], 0);
    assert!(paused["shared"]["lifecycles"][lifecycle]["frozen"]
        .as_array()
        .unwrap()
        .contains(&json!(2)));
    let receipt = wire.finish(OWNER, "remote-drain", &request);
    let after = wire.checkpoint();
    assert_eq!(after["game"]["tick"], 100);
    assert_eq!(after["game"]["rules_rng"], paused["game"]["rules_rng"]);
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(drain_health(&mut wire, OWNER), (1, 3));
    let actual = if nothing_actor(&after, ActorId(3)).is_null() {
        let capture = crate::engine::Checkpoint::capture(&wire.service.engine);
        let record = capture
            .records
            .values()
            .map(|r| serde_json::to_value(tor_simulation::RegionRecordData::capture(r)).unwrap())
            .find(|r| r["world"]["region"]["id"] == 2)
            .expect("real freshly detached remote record");
        assert_eq!(record["stamps"]["3"], 0);
        record["actors"]["3"].clone()
    } else {
        let lifecycle = after["game"]["lifecycle"].as_u64().unwrap() as usize;
        assert_eq!(after["shared"]["lifecycles"][lifecycle]["stamps"]["3"], 0);
        assert!(after["shared"]["lifecycles"][lifecycle]["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(2)));
        nothing_actor(&after, ActorId(3)).clone()
    };
    for field in [
        "ready_at",
        "active_since",
        "location",
        "pending",
        "combat",
        "behavior",
    ] {
        assert_eq!(
            actual[field], actor[field],
            "zero-share remote {field} unchanged"
        );
    }
    assert_eq!(
        actual["awareness"]["alert"],
        json!({"location":nothing_actor(&paused,OWNER)["location"],"expires":37})
    );
    let snapshot = wire.snapshot(OWNER);
    assert!(!nothing_identified(&snapshot, wand));
    // No direct-health event is generated for zero loss; no remote anonymous
    // event may turn the private census/contact into a public target count.
    assert!(snapshot
        .state
        .observation
        .visible_actors
        .iter()
        .all(|v| v.name != "remote drain sentinel"));
    let encoded = serde_json::to_value(&snapshot.state.observation.combat).unwrap();
    assert!(!encoded.to_string().contains("remote drain sentinel"));
    assert_eq!(ack(wire.send(OWNER, "remote-drain", request)), receipt);
    assert_eq!(wire.checkpoint(), after);
    wire.service.engine.flush().unwrap();
    drop(wire);
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        checkpoint(&Engine::open(&path, crate::Scenario::two_room(999)).unwrap()),
        after
    );
}

// Only typed opaque identities are normalized; every field, row, event and
// multiplicity remains in the compared ordinary observation.
// Cell keys are opaque String fields, not a protocol FromStr token. Normalize
// only the typed cell identity/reference fields; never arbitrary prose or hex.
fn drain_normalize_cells(cells: &mut [CellView], keys: &mut BTreeMap<String, String>) {
    fn key(value: &mut String, keys: &mut BTreeMap<String, String>) {
        let next = format!("cell-{}", keys.len());
        *value = keys.entry(value.clone()).or_insert(next).clone();
    }
    for cell in cells {
        key(&mut cell.key, keys);
        if let Some(door) = &mut cell.door {
            for approach in &mut door.approaches {
                key(approach, keys);
            }
        }
    }
}

#[test]
fn drain_cell_normalization_preserves_aliases_and_all_substantive_fields() {
    fn cell(key: &str, x: i32) -> CellView {
        serde_json::from_value(json!({"key":key,"position":{"x":x,"y":0,"z":0},
            "lit":false,"door":null,"trap":null,"object_presentation":null,"material":"unchanged narrative",
            "stairs_up":false,"stairs_down":false,"wall":false,"place_hint":false}))
        .unwrap()
    }
    let original = vec![cell("first-a", 0), cell("second-a", 1), cell("first-a", 2)];
    let mut a = original.clone();
    let mut b = vec![cell("first-b", 0), cell("second-b", 1), cell("first-b", 2)];
    let mut keys = BTreeMap::new();
    drain_normalize_cells(&mut a, &mut keys);
    drain_normalize_cells(&mut b, &mut BTreeMap::new());
    assert_eq!(a, b);
    assert_eq!(a[0].key, a[2].key);
    assert_ne!(a[0].key, a[1].key);
    // A later delta must reuse the same identity map, not remint its first row.
    let mut later = vec![cell("second-a", 1)];
    drain_normalize_cells(&mut later, &mut keys);
    assert_eq!(later[0].key, a[1].key);
    // Door approach references share the same cell-key namespace. Narrative
    // text that merely looks like a real key must never be normalized.
    fn door_cells(first: &str, second: &str, approach: &str) -> Vec<CellView> {
        let mut cells = vec![cell(first, 0), cell(second, 1)];
        cells[0].door = Some(DoorView {
            id: DoorTarget::from_digest([7; 32]),
            name: "a".repeat(40),
            description: "a".repeat(40),
            open: true,
            reachable: true,
            approaches: vec![approach.into()],
            asset: None,
        });
        cells[0].material = "a".repeat(40);
        cells
    }
    let raw_a = "a".repeat(40);
    let raw_b = "b".repeat(40);
    let raw_c = "c".repeat(40);
    let raw_d = "d".repeat(40);
    let mut references = door_cells(&raw_a, &raw_b, &raw_b);
    let mut renamed = door_cells(&raw_c, &raw_d, &raw_d);
    let mut mismatched = door_cells(&raw_c, &raw_d, &raw_c);
    drain_normalize_cells(&mut references, &mut BTreeMap::new());
    drain_normalize_cells(&mut renamed, &mut BTreeMap::new());
    drain_normalize_cells(&mut mismatched, &mut BTreeMap::new());
    assert_eq!(references, renamed);
    assert_ne!(references, mismatched, "broken door reference must survive");
    let door = references[0].door.as_ref().unwrap();
    assert_eq!(door.approaches, vec![references[1].key.clone()]);
    assert_eq!(door.name, raw_a);
    assert_eq!(door.description, raw_a);
    assert_eq!(references[0].material, raw_a);
    for change in 0..4 {
        let mut bad = original.clone();
        match change {
            0 => bad[0].position.x += 1,
            1 => bad[0].material = "different content".into(),
            2 => {
                bad.pop();
            }
            3 => bad[2].key = "not-the-first-alias".into(),
            _ => unreachable!(),
        }
        drain_normalize_cells(&mut bad, &mut BTreeMap::new());
        assert_ne!(a, bad, "substantive difference {change} must survive");
    }
}

fn drain_normalize_tokens(value: &mut Value, tokens: &mut BTreeMap<String, String>) {
    match value {
        Value::String(text)
            if text.parse::<ActorTarget>().is_ok()
                || text.parse::<ItemTarget>().is_ok()
                || text.parse::<AppearanceSeed>().is_ok() =>
        {
            let next = format!("opaque-{}-{}", &text[..1], tokens.len());
            *text = tokens.entry(text.clone()).or_insert(next).clone();
        }
        Value::Array(values) => {
            for value in values {
                drain_normalize_tokens(value, tokens);
            }
        }
        Value::Object(fields) => {
            for value in fields.values_mut() {
                drain_normalize_tokens(value, tokens);
            }
        }
        _ => {}
    }
}
fn drain_follower_observation(
    wire: &mut Wire,
    follower: &mut Connection,
    request: Request,
) -> Observation {
    // Initial Welcome or actual completion updates have already been consumed
    // by their owning caller. Never discard a queued publication here.
    assert!(
        follower.messages.try_recv_frame().is_err(),
        "uncollected follower frame"
    );
    wire.service
        .handle(follower.id, "drain-follow".into(), request);
    let response = decode_response(&follower.messages.try_recv_frame().unwrap().text).unwrap();
    match response {
        ServerMessage::Snapshot {
            request_id,
            snapshot,
        } => {
            assert_eq!(request_id, "drain-follow");
            snapshot.state.observation.clone()
        }
        other => panic!("drain-follow expected its requested snapshot, received {other:?}"),
    }
}

fn drain_capture_observation_updates(messages: &[ServerMessage], updates: &mut Vec<StreamUpdate>) {
    for message in messages {
        assert!(
            !matches!(message, ServerMessage::Error { .. }),
            "{message:?}"
        );
        if let ServerMessage::Update { update } = message {
            if matches!(
                update.body,
                UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }
            ) {
                updates.push((**update).clone());
            }
        }
    }
}
fn drain_normalized_updates(mut updates: Vec<StreamUpdate>) -> Value {
    assert!(
        !updates.is_empty(),
        "actual observation publication required"
    );
    assert!(
        updates.iter().any(|u| u.cursor.tick == 100),
        "paid completion publication required"
    );
    let mut identities = BTreeMap::<String, String>::new();
    let mut identity = |kind: &str, original: &mut String| {
        let next = format!("{kind}-{}", identities.len());
        *original = identities
            .entry(format!("{kind}:{original}"))
            .or_insert(next)
            .clone();
    };
    let mut cell_keys = BTreeMap::new();
    for update in &mut updates {
        match &mut update.body {
            UpdateBody::Observation { state, .. } => {
                let state = std::sync::Arc::make_mut(state);
                identity("snapshot", &mut state.snapshot.0);
                drain_normalize_cells(&mut state.observation.visible_cells, &mut cell_keys);
            }
            UpdateBody::ObservationDelta { base, state, .. } => {
                identity("snapshot", &mut base.snapshot.0);
                identity("snapshot", &mut state.base_snapshot.0);
                identity("snapshot", &mut state.snapshot.0);
                drain_normalize_cells(&mut state.cells.changed, &mut cell_keys)
            }
            _ => unreachable!("only actual observation updates captured"),
        }
        // CellChanges.removed contains Positions, not opaque keys: unchanged.
        identity("stream", &mut update.context.stream.0);
        identity("branch", &mut update.branch.0);
        let event = match &mut update.body {
            UpdateBody::Observation { event, .. } | UpdateBody::ObservationDelta { event, .. } => {
                event
            }
            _ => unreachable!("only actual observation updates captured"),
        };
        if let Some(event) = event {
            identity("entry", &mut event.id.0);
            identity("branch", &mut event.branch.0);
        }
    }
    let mut value = serde_json::to_value(updates).unwrap();
    drain_normalize_tokens(&mut value, &mut BTreeMap::new());
    value
}
fn hidden_drain_observations(count: u64) -> (Value, Value, Value, Value) {
    assert!((1..=2).contains(&count));
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let base = drain_life_source(&root, 21, [1, 2, 0], false);
    let source = base.package.as_ref().unwrap();
    let rules = toml::to_string(source.rules().unwrap().document()).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["spatial_scopes"] = json!({"hidden-room":{"members":[1],"rooms":{"1":[{"region":1,"min":[0,0,0],"max":[6,4,1]}]},"passages":[],"doorways":[]}});
    let creature = manifest["characters"][0]["creature"].clone();
    let mut regions = source.region_defs().unwrap();
    let mut region = serde_json::to_value(&regions[0]).unwrap();
    region["size"] = json!([7, 5, 2]);
    region["lit"] = json!(false);
    region["lighting"] = json!([]);
    region["actors"]=json!((0..count).map(|i|json!({"id":i+2,"at":[5,if i==0 {1}else{3},0],"turn_ticks":100,"controller":"external","creature":creature})).collect::<Vec<_>>());
    regions[0] = serde_json::from_value(region).unwrap();
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    package.check().unwrap();
    crate::scenario_package::write_package(
        &root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(&root).unwrap();
    let scenario = crate::scenario_package::load(&root, 42, None, true).unwrap();
    let mut wire = Wire::for_actors(
        Engine::memory(scenario).unwrap(),
        (1..=count + 1).map(ActorId),
    );
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    for id in 1..=count + 1 {
        assert_eq!(drain_health(&mut wire, ActorId(id)), (21, 21));
    }
    let before = wire.snapshot(OWNER);
    assert!(
        before.state.observation.visible_actors.is_empty(),
        "actual hidden-recipient prerequisite"
    );
    assert_eq!(
        before
            .state
            .observation
            .combat
            .as_ref()
            .unwrap()
            .own_stats
            .as_ref()
            .unwrap()
            .attributes
            .awareness,
        0
    );
    let mut follower = wire
        .service
        .connect(
            &Account {
                user: "drain-follower".into(),
                token: "fixture".into(),
                role: AccessRole::Spectator,
                actors: BTreeSet::from([OWNER]),
            },
            "drain-follower".into(),
        )
        .unwrap();
    follower.messages.try_recv_frame().unwrap();
    assert!(
        drain_follower_observation(&mut wire, &mut follower, Request::Attach { actor: OWNER })
            .visible_actors
            .is_empty()
    );
    let wand = before
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    let physical = nothing_wand_id(&wire, wand);
    let initial = wire.checkpoint();
    // Capture losslessly across admission and every scheduler drive. The ordinary
    // Wire::finish intentionally drains frames, so this small fixture-local loop
    // retains actual owner and followed observation/delta publications instead.
    let request = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::West,
            },
        },
    );
    let mut owner_updates = Vec::new();
    let admitted = wire.send(OWNER, "hidden-drain", request.clone());
    drain_capture_observation_updates(&admitted, &mut owner_updates);
    ack(admitted);
    let mut resolved = false;
    for _ in 0..16 {
        wire.service.run_until_blocked();
        drain_capture_observation_updates(&wire.drain(OWNER), &mut owner_updates);
        for id in 2..=count + 1 {
            assert!(!wire
                .drain(ActorId(id))
                .iter()
                .any(|m| matches!(m, ServerMessage::Error { .. })));
        }
        let response = wire.send(OWNER, "hidden-drain", request.clone());
        drain_capture_observation_updates(&response, &mut owner_updates);
        let receipt = ack(response);
        if matches!(
            receipt,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Resolved,
                ..
            }
        ) {
            resolved = true;
            break;
        }
        assert!(
            matches!(
                receipt,
                RequestReceipt::Admitted {
                    phase: IntentionPhase::Queued | IntentionPhase::Started,
                    ..
                }
            ),
            "{receipt:?}"
        );
        let next = wire.service.engine.next_actor().unwrap();
        assert_ne!(
            next, OWNER,
            "fixture's other external owners advance this preparation"
        );
        wire.serial += 1;
        let wait = wire.request(next, Action::Wait);
        ack(wire.send(next, &format!("privacy-clock-{}", wire.serial), wait));
    }
    assert!(resolved, "actual admitted drain completion");
    // Both attachments demand a picture of the same committed result. Preserve
    // the actual encoded publication comparison, including delta bases.
    let observed = wire.send(OWNER, "drain-picture", Request::Observe);
    drain_capture_observation_updates(&observed, &mut owner_updates);
    wire.service
        .handle(follower.id, "drain-picture".into(), Request::Observe);
    drain_capture_observation_updates(&wire.drain(OWNER), &mut owner_updates);
    let follower_frames: Vec<_> = std::iter::from_fn(|| follower.messages.try_recv_frame().ok())
        .map(|f| decode_response(&f.text).unwrap())
        .collect();
    let mut follower_updates = Vec::new();
    drain_capture_observation_updates(&follower_frames, &mut follower_updates);
    let owner_updates = drain_normalized_updates(owner_updates);
    let follower_updates = drain_normalized_updates(follower_updates);
    let after = wire.checkpoint();
    assert_eq!(after["game"]["tick"], 100);
    assert_eq!(after["game"]["rules_rng"], initial["game"]["rules_rng"]);
    assert_eq!(nothing_items(&after)[&physical]["charges"], 2);
    assert_eq!(drain_health(&mut wire, OWNER), (10, 21));
    for id in 2..=count + 1 {
        assert_eq!(drain_health(&mut wire, ActorId(id)), (21 - 10 / count, 21));
    }
    let owner = wire.snapshot(OWNER);
    assert!(owner.state.observation.visible_actors.is_empty());
    assert!(!nothing_identified(&owner, wand));
    let followed = drain_follower_observation(&mut wire, &mut follower, Request::Snapshot);
    assert!(followed.visible_actors.is_empty());
    let mut owner = owner.state.observation.clone();
    let mut followed = followed;
    drain_normalize_cells(&mut owner.visible_cells, &mut BTreeMap::new());
    drain_normalize_cells(&mut followed.visible_cells, &mut BTreeMap::new());
    let mut owner = serde_json::to_value(owner).unwrap();
    let mut followed = serde_json::to_value(followed).unwrap();
    drain_normalize_tokens(&mut owner, &mut BTreeMap::new());
    drain_normalize_tokens(&mut followed, &mut BTreeMap::new());
    (owner, followed, owner_updates, follower_updates)
}
#[test]
fn actual_drain_life_hidden_recipient_count_does_not_change_owner_or_followed_observation() {
    let one = hidden_drain_observations(1);
    let two = hidden_drain_observations(2);
    assert_eq!(
        one.0, two.0,
        "ordinary owner cannot learn hidden target count"
    );
    assert_eq!(
        one.1, two.1,
        "followed spectator cannot learn hidden target count"
    );
    assert_eq!(
        one.2, two.2,
        "actual owner observation/delta publications hide recipient count"
    );
    assert_eq!(
        one.3, two.3,
        "actual followed observation/delta publications hide recipient count"
    );
}

// Physical first-body delivery must close each real World frontier without
// activating source population or replacing geometry with a room graph.
fn first_contact_chain_source(root: &Path, noncombat: bool, ordinary: bool) -> crate::Scenario {
    let baseline = self_device_source(root, false, false);
    let source = baseline.package.as_ref().unwrap();
    let mut document = source.rules().unwrap().document().clone();
    let ability = document
        .abilities
        .iter_mut()
        .find(|a| a.id.as_str() == "magic.ember_ray")
        .unwrap();
    ability.invocation = tor_simulation::abilities::InvocationKind::DirectionalContactRay;
    ability.reach = tor_simulation::rules::Reach::UntilObstruction;
    ability.interaction = tor_simulation::rules::Interaction::HostileToTarget;
    ability.ai.hostile = true;
    let tor_simulation::rules::StrikeSource::Authored { strikes } = &mut ability.strikes else {
        panic!("authored source prerequisite")
    };
    strikes[0].on_success = vec![tor_simulation::rules::Effect::CancelSpecials];
    assert!(strikes[0].check.is_none() && strikes[0].on_failure.is_empty());
    let mut ordinary_ability = ability.clone();
    ordinary_ability.id =
        tor_simulation::content_ids::AbilityId::new("fixture.ordinary_cancellation").unwrap();
    ordinary_ability.cost = Some(
        serde_json::from_value(json!({"resource":"stamina","start":0,"resolution":1})).unwrap(),
    );
    document.abilities.push(ordinary_ability);
    tor_simulation::rules::Ruleset::validate(document.clone()).unwrap();
    let rules = toml::to_string(&document).unwrap();
    let mut manifest = serde_json::to_value(&source.manifest).unwrap();
    manifest["spatial_scopes"] = json!({});
    let owner_species = manifest["characters"][0]["creature"]["species"]
        .as_str()
        .unwrap()
        .to_owned();
    manifest["creatures"]["species"]["fixture_remote"] =
        manifest["creatures"]["species"][&owner_species].clone();
    manifest["creatures"]["species"]["fixture_remote"]["modifiers"].as_array_mut().unwrap().push(json!({"cancellable":true,"scope":"always","op":{"kind":"perception","value":{"kind":"invisible","enabled":true}}}));
    if ordinary {
        manifest["creatures"]["species"][&owner_species]["modifiers"].as_array_mut().unwrap().push(json!({"scope":"always","op":{"kind":"grant_ability","ability":"fixture.ordinary_cancellation"}}));
    }
    let creature = manifest["characters"][0]["creature"].clone();
    let mut remote_creature = creature.clone();
    remote_creature["species"] = json!("fixture_remote");
    let mut local = serde_json::to_value(&source.region_defs().unwrap()[0]).unwrap();
    local["size"] = json!([32, 5, 2]);
    local["lit"] = json!(false);
    local["lighting"] = json!([]);
    local["anchors"]["east"] = json!([31, 2, 0]);
    local["portals"] =
        json!([{"at":[31,2,0],"direction":"east","to":"2/west","kind":"portal","height":2}]);
    local["actors"] =
        json!([{"id":2,"at":[5,3,0],"turn_ticks":100,"controller":"external","creature":creature}]);
    local["items"] = json!([{"id":30,"at":[1,2,0],"archetype":"ember","carried_by":1}, {"id":20,"at":[1,2,0],"archetype":"identify","quantity":1,"carried_by":1}]);
    let mut regions = vec![serde_json::from_value(local).unwrap()];
    for region in 2..=5 {
        let mut portals = vec![
            json!({"at":[0,2,0],"direction":"west","to":format!("{}/east",region-1),"kind":"portal","height":2}),
        ];
        if region < 5 {
            portals.push(json!({"at":[31,2,0],"direction":"east","to":format!("{}/west",region+1),"kind":"portal","height":2}));
        }
        let mut value = json!({"id":region,"name":format!("Contact member {region}"),"size":[32,5,2],"chamber":true,"lit":false,"anchors":{"west":[0,2,0],"east":[31,2,0]},"portals":portals});
        if region == 5 {
            value["actors"] = json!([{"id":3,"at":[20,2,0],"turn_ticks":100,"controller":"ai","creature":remote_creature}]);
        }
        if region == 3 && noncombat {
            value["actors"] =
                json!([{"id":4,"at":[10,2,0],"turn_ticks":100,"controller":"external"}]);
        }
        regions.push(serde_json::from_value(value).unwrap());
    }
    let package =
        Package::from_parts(serde_json::from_value(manifest).unwrap(), regions, &rules).unwrap();
    package.check().unwrap();
    assert!(package.manifest.floor_activation.is_none());
    assert!(package.manifest.spatial_scopes.is_empty());
    crate::scenario_package::write_package(
        root,
        &package.manifest,
        &package.region_defs().unwrap(),
        &rules,
    )
    .unwrap();
    crate::scenario_package::check(root).unwrap();
    let mut scenario = crate::scenario_package::load(root, 42, None, true).unwrap();
    scenario.streaming = Some(crate::Streaming {
        active_radius: 0,
        load_radius: 0,
    });
    scenario
}

fn first_contact_remote_record(
    wire: &Wire,
    saved: &Value,
    path: &Path,
    content: Arc<tor_simulation::creatures::Content>,
    actor: ActorId,
    region: u64,
) -> Value {
    let lifecycle = saved["game"]["lifecycle"].as_u64().unwrap() as usize;
    if !nothing_actor(saved, actor).is_null() {
        assert_eq!(
            saved["shared"]["lifecycles"][lifecycle]["stamps"][actor.0.to_string()],
            0
        );
        assert!(saved["shared"]["lifecycles"][lifecycle]["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(region)));
        return nothing_actor(saved, actor).clone();
    }
    let id = tor_simulation::RecordId(
        saved["shared"]["lifecycles"][lifecycle]["detached"][region.to_string()]
            .as_u64()
            .expect("current root owns exact detached record"),
    );
    let capture = crate::engine::Checkpoint::capture(&wire.service.engine);
    let record = if let Some(record) = capture.records.get(&id) {
        tor_simulation::RegionRecordData::capture(record)
    } else {
        let record = crate::storage::test_read_region_record(path, id, content)
            .unwrap()
            .expect("current interval-one detached record is durable");
        tor_simulation::RegionRecordData::capture(&record)
    };
    let record = serde_json::to_value(record).unwrap();
    assert_eq!(record["world"]["region"]["id"], region);
    assert_eq!(record["stamps"][actor.0.to_string()], 0);
    record["actors"][actor.0.to_string()].clone()
}

fn first_contact_stamina(wire: &mut Wire) -> ResourceView {
    wire.snapshot(OWNER)
        .state
        .observation
        .combat
        .as_ref()
        .unwrap()
        .own_stats
        .as_ref()
        .unwrap()
        .resources
        .iter()
        .find(|r| r.resource.id == "stamina")
        .unwrap()
        .clone()
}

fn first_contact_saved_plan_negatives(wire: &Wire, saved: &Value, ordinary: bool) {
    let bytes = serde_json::to_vec(saved).unwrap();
    wire.service.engine.test_restore_checkpoint(&bytes).unwrap();
    let actors = saved["game"]["actors"].as_u64().unwrap();
    let path = format!(
        "/shared/actors/{actors}/1/pending/plan{}",
        if ordinary { "" } else { "/delivery/plan" }
    );
    let baseline = saved.pointer(&path).unwrap();
    assert_eq!(baseline["traversal"], json!({"kind":"first_body"}));
    assert_eq!(baseline["resolution"], json!({"kind":"checkless"}));
    let mutations = [
        ("traversal", json!({"kind":"bounded","clear_visits":6})),
        ("traversal", json!({"kind":"first_body","clear_visits":6})),
        ("traversal", json!(["first_body"])),
        ("resolution", json!({"kind":"checkless","tokens":[]})),
        (
            "resolution",
            json!({"kind":"opposed","source_skill":"lore","target_skill":"agility","source_bonus":0,"source_edge":0,"effect_edge":0,"incoming":[],"tokens":[]}),
        ),
        ("resolution", Value::Null),
        (
            "ability",
            json!(if ordinary {
                "magic.ember_ray"
            } else {
                "fixture.ordinary_cancellation"
            }),
        ),
        ("effects", json!([])),
        ("actor", json!(OTHER.0)),
    ];
    for (field, value) in mutations {
        let mut bad = saved.clone();
        bad.pointer_mut(&path).unwrap()[field] = value;
        assert!(
            wire.service
                .engine
                .test_restore_checkpoint(&serde_json::to_vec(&bad).unwrap())
                .is_err(),
            "forged pending {field}"
        );
    }
    for field in ["traversal", "resolution"] {
        let mut bad = saved.clone();
        bad.pointer_mut(&path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            wire.service
                .engine
                .test_restore_checkpoint(&serde_json::to_vec(&bad).unwrap())
                .is_err(),
            "missing pending {field}"
        );
    }
}

fn paid_first_contact_chain(noncombat: bool, ordinary: bool) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("source");
    let path = directory.path().join("first-contact.db");
    let scenario = first_contact_chain_source(&root, noncombat, ordinary);
    // Decoder catalog only: never passed to the source-free Engine reopen.
    let content = scenario.package.as_ref().unwrap().content().unwrap();
    let owners = if noncombat {
        vec![OWNER, OTHER, ActorId(4)]
    } else {
        vec![OWNER, OTHER]
    };
    let mut wire = Wire::for_actors(
        Engine::open_with_policy(
            &path,
            scenario,
            crate::SavePolicy {
                checkpoint_interval: 1,
                ..Default::default()
            },
        )
        .unwrap(),
        owners.iter().copied(),
    );
    assert_eq!(wire.service.engine.next_actor(), Some(OWNER));
    let initial = wire.checkpoint();
    let before_stamina = first_contact_stamina(&mut wire);
    assert_eq!(
        (
            before_stamina.balance,
            before_stamina.maximum,
            before_stamina.reserved
        ),
        (2, 2, 0)
    );
    assert!(nothing_actor(&initial, ActorId(3)).is_null());
    // A declared external actor is itself an initial observing root. The
    // noncombat branch deliberately tests an ACTIVE blocker, not cold residency.
    let cold_regions = if noncombat { 5..=5 } else { 3..=5 };
    for region in cold_regions {
        assert_eq!(
            room_light_world(&initial).is_lit(tor_world::Location {
                region: tor_world::RegionId(region),
                position: tor_world::Position { x: 20, y: 2, z: 0 }
            }),
            None
        );
    }
    if noncombat {
        let body = nothing_actor(&initial, ActorId(4));
        assert!(body.is_object() && body["combat"].is_null() && body["body"].is_object());
        assert_eq!(body["location"]["position"], json!({"x":10,"y":2,"z":0}));
        let lifecycle = initial["game"]["lifecycle"].as_u64().unwrap() as usize;
        assert!(!initial["shared"]["lifecycles"][lifecycle]["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(3)));
        assert!(initial["shared"]["lifecycles"][lifecycle]["stamps"]["4"].is_null());
        assert_eq!(wire.clients.keys().copied().collect::<Vec<_>>(), owners);
    }
    let wand = wire
        .snapshot(OWNER)
        .state
        .observation
        .inventory
        .iter()
        .find(|i| i.class == ItemClass::Wand)
        .unwrap()
        .id;
    let physical = nothing_wand_id(&wire, wand);
    let unknown = wire.snapshot(OWNER);
    assert!(!nothing_identified(&unknown, wand));
    assert_eq!(
        nothing_affordance(&unknown, wand).device_activation,
        Some(DeviceActivation::Direction)
    );
    let bad = wire.request(
        OWNER,
        Action::Zap {
            item: wand,
            aim: DeviceAim::SelfTarget {},
        },
    );
    assert!(wire
        .send(OWNER, "contact-self-is-not-licensed", bad)
        .iter()
        .any(|m| matches!(m, ServerMessage::Error { .. })));
    assert_eq!(
        wire.checkpoint(),
        initial,
        "unknown directional source refuses self aim without payment or preparation"
    );
    assert_eq!(nothing_items(&initial)[&physical]["charges"], 3);
    let action = if ordinary {
        Action::UseAbility {
            id: "fixture.ordinary_cancellation".into(),
            invocation: AbilityInvocation::directional_ray(Direction::East),
            supplies: Default::default(),
        }
    } else {
        Action::Zap {
            item: wand,
            aim: DeviceAim::Direction {
                direction: Direction::East,
            },
        }
    };
    let request = wire.request(OWNER, action);
    ack(wire.send(OWNER, "physical-first-body", request.clone()));
    wire.service.run_until_blocked();
    for actor in owners.iter().copied() {
        let messages = wire.drain(actor);
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, ServerMessage::Error { .. })),
            "{messages:?}"
        );
    }
    let receipt = ack(wire.send(OWNER, "physical-first-body", request.clone()));
    assert!(
        matches!(
            receipt,
            RequestReceipt::Admitted {
                phase: IntentionPhase::Started,
                ..
            }
        ),
        "{receipt:?}"
    );
    let paused = wire.checkpoint();
    let held_stamina = first_contact_stamina(&mut wire);
    assert_eq!(held_stamina.balance, before_stamina.balance);
    assert_eq!(held_stamina.reserved, u32::from(ordinary));
    assert_eq!(paused["game"]["tick"], initial["game"]["tick"]);
    assert_eq!(paused["game"]["rules_rng"], initial["game"]["rules_rng"]);
    assert_eq!(nothing_items(&paused)[&physical]["charges"], 3);
    if !noncombat {
        let world = room_light_world(&paused);
        for region in 1..5 {
            assert_eq!(
                world
                    .physics_neighbor(
                        tor_world::Location {
                            region: tor_world::RegionId(region),
                            position: tor_world::Position { x: 31, y: 2, z: 0 }
                        },
                        tor_world::Direction::East
                    )
                    .unwrap()
                    .0,
                tor_world::Location {
                    region: tor_world::RegionId(region + 1),
                    position: tor_world::Position { x: 0, y: 2, z: 0 }
                }
            );
        }
        let mut physical = tor_world::Location {
            region: tor_world::RegionId(1),
            position: tor_world::Position { x: 1, y: 2, z: 0 },
        };
        for _ in 0..147 {
            physical = world
                .physics_neighbor(physical, tor_world::Direction::East)
                .unwrap()
                .0;
        }
        assert_eq!(
            physical,
            tor_world::Location {
                region: tor_world::RegionId(5),
                position: tor_world::Position { x: 20, y: 2, z: 0 }
            },
            "actual physical route crosses 147 cells, not a legacy sight/probe allowance"
        );
        let remote = nothing_actor(&paused, ActorId(3));
        assert!(remote.is_object());
        assert_eq!(remote["combat"]["creature"]["cancelled"], false);
        assert_eq!(remote["ready_at"], 0);
        assert_eq!(remote["active_since"], 0);
        let lifecycle = paused["game"]["lifecycle"].as_u64().unwrap() as usize;
        assert_eq!(paused["shared"]["lifecycles"][lifecycle]["stamps"]["3"], 0);
        assert!(paused["shared"]["lifecycles"][lifecycle]["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(5)));
    } else {
        assert!(nothing_actor(&paused, ActorId(4))["combat"].is_null());
        assert!(nothing_actor(&paused, ActorId(4)).is_object());
        assert!(nothing_actor(&paused, ActorId(4))["body"].is_object());
        assert_eq!(
            nothing_actor(&paused, ActorId(4))["location"]["position"],
            json!({"x":10,"y":2,"z":0})
        );
        let lifecycle = paused["game"]["lifecycle"].as_u64().unwrap() as usize;
        assert!(paused["shared"]["lifecycles"][lifecycle]["stamps"]["4"].is_null());
        assert!(!paused["shared"]["lifecycles"][lifecycle]["frozen"]
            .as_array()
            .unwrap()
            .contains(&json!(3)));
        assert!(
            nothing_actor(&paused, ActorId(3)).is_null(),
            "first noncombat body stops before later frontier"
        );
    }
    first_contact_saved_plan_negatives(&wire, &paused, ordinary);
    wire.service.engine.flush().unwrap();
    let serial = wire.serial;
    drop(wire);
    std::fs::remove_dir_all(&root).unwrap();
    let engine = Engine::open(&path, crate::Scenario::two_room(999)).unwrap();
    assert_eq!(
        checkpoint(&engine),
        paused,
        "source-free Engine restore preserves full pending checkpoint before Service pause"
    );
    let mut wire = Wire::for_actors(engine, owners.iter().copied());
    wire.serial = serial;
    assert!(wire.serial.checked_add(1).unwrap() > serial);
    let service_paused = wire.checkpoint();
    assert_eq!(
        nothing_actor(&service_paused, OWNER)["pending"]["active"],
        false,
        "Service reconnect deliberately pauses human work"
    );
    assert_eq!(
        nothing_actor(&service_paused, OWNER)["pending"]["plan"],
        nothing_actor(&paused, OWNER)["pending"]["plan"]
    );
    assert_eq!(
        nothing_actor(&service_paused, OWNER)["pending"]["remaining"],
        nothing_actor(&paused, OWNER)["pending"]["remaining"]
    );
    let snapshot = wire.snapshot(OWNER);
    assert_eq!(snapshot.readiness.resume.len(), 1);
    let resume = Request::Command {
        context: snapshot.reply_context().input,
        branch: snapshot.branch.clone(),
        command: Command::ResumeIntention {
            intention: snapshot.readiness.resume[0].clone(),
        },
    };
    ack(wire.send(OWNER, "resume-physical-first-body", resume));
    let terminal = wire.finish(OWNER, "physical-first-body", &request);
    let after = wire.checkpoint();
    let after_stamina = first_contact_stamina(&mut wire);
    assert_eq!(
        after_stamina.balance,
        before_stamina.balance - u32::from(ordinary)
    );
    assert_eq!(after_stamina.reserved, 0);
    wire.service.engine.flush().unwrap();
    assert_eq!(
        after["game"]["rules_rng"], paused["game"]["rules_rng"],
        "cancellation has no opposed check or RNG claim"
    );
    assert!(after["game"]["tick"].as_u64().unwrap() > paused["game"]["tick"].as_u64().unwrap());
    assert_eq!(
        nothing_items(&after)[&physical]["charges"],
        if ordinary { 3 } else { 2 }
    );
    if !noncombat {
        let remote = first_contact_remote_record(&wire, &after, &path, content, ActorId(3), 5);
        assert_eq!(remote["combat"]["creature"]["cancelled"], true);
        for field in [
            "ready_at",
            "active_since",
            "location",
            "pending",
            "behavior",
        ] {
            assert_eq!(
                remote[field],
                nothing_actor(&paused, ActorId(3))[field],
                "frozen {field}"
            );
        }
    }
    if noncombat {
        assert!(
            nothing_actor(&after, ActorId(3)).is_null(),
            "active physical blocker did not touch later unbuilt recipient"
        );
        assert_eq!(
            room_light_world(&after).is_lit(tor_world::Location {
                region: tor_world::RegionId(5),
                position: tor_world::Position { x: 20, y: 2, z: 0 }
            }),
            None
        );
        assert!(nothing_actor(&after, ActorId(4))["combat"].is_null());
        assert_eq!(
            nothing_actor(&after, ActorId(4))["location"],
            nothing_actor(&paused, ActorId(4))["location"]
        );
    }
    let public = serde_json::to_string(&wire.snapshot(OWNER)).unwrap();
    for private in [
        "cancel_specials",
        "cancelled",
        "rules_rng",
        "first_body",
        "checkless",
    ] {
        assert!(
            !public.contains(private),
            "ordinary observation leaked {private}"
        );
    }
    assert!(after["game"]["floor_activation"].is_null());
    assert_eq!(
        ack(wire.send(OWNER, "physical-first-body", request)),
        terminal
    );
    assert_eq!(
        wire.checkpoint(),
        after,
        "durable retry cannot pay or apply twice"
    );
    if !ordinary && !noncombat {
        let scroll = wire
            .snapshot(OWNER)
            .state
            .observation
            .inventory
            .iter()
            .find(|i| i.class == ItemClass::Scroll)
            .unwrap()
            .id;
        wire.act("contact-read-identify", Action::Read { item: scroll });
        let paid = choice(&wire.snapshot(OWNER));
        wire.act(
            "contact-identify",
            Action::Identify {
                choice: paid.choice,
                item: Some(wand),
            },
        );
        let known = wire.snapshot(OWNER);
        assert!(nothing_identified(&known, wand));
        assert_eq!(
            nothing_affordance(&known, wand).device_activation,
            Some(DeviceActivation::Direction),
            "known first-contact device never becomes self-target convenience"
        );
        let before_repeat = wire.checkpoint();
        wire.act(
            "known-contact-repeat",
            Action::Zap {
                item: wand,
                aim: DeviceAim::Direction {
                    direction: Direction::East,
                },
            },
        );
        let repeated = wire.checkpoint();
        assert_eq!(nothing_items(&repeated)[&physical]["charges"], 1);
        assert_eq!(
            repeated["game"]["rules_rng"],
            before_repeat["game"]["rules_rng"]
        );
    }
}
#[test]
fn cancellation_first_body_closes_multiple_cold_frontiers_and_reopens_pending_without_sources() {
    paid_first_contact_chain(false, false);
}
#[test]
fn cancellation_first_active_noncombat_body_blocks_later_cold_recipient() {
    paid_first_contact_chain(true, false);
}
#[test]
fn ordinary_cancellation_grant_retains_resource_ledger_across_cold_frontiers() {
    paid_first_contact_chain(false, true);
}

#[path = "portable_torch_session_tests.rs"]
mod portable_torch;

#[path = "haste_dose_session_tests.rs"]
mod haste_dose;

#[path = "growth_potion_session_tests.rs"]
mod growth_potions;

#[path = "hallucination_session_tests.rs"]
mod hallucination;

#[path = "chart_paid_session_tests.rs"]
mod charts;
