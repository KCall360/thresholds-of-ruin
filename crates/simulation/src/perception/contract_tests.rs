//! Behavior contracts at the shared simulation perception and lifecycle seams.
use super::*;
use crate::{
    abilities::{AbilityInvocation, OutcomeStatus, RuleError},
    combat::CombatEvent,
    Action, IntentionOrigin,
};
use tor_world::Direction;

#[path = "contract_fixtures.rs"]
mod fixtures;
use fixtures::*;

fn invisible() -> PerceptionValue {
    PerceptionValue::Invisible { enabled: true }
}

fn disclosure(game: &Game, observer: ActorId, subject: ActorId) -> ActorDisclosure {
    game.classified_actors(observer)
        .unwrap()
        .get(&subject)
        .map_or(ActorDisclosure::Hidden, |row| row.disclosure)
}

#[test]
fn see_invisible_requires_an_illuminated_eye_witness_even_with_local_contact() {
    let mut game = fixture(
        &[PerceptionValue::SeeInvisible { enabled: true }],
        &[invisible()],
    );
    assert_eq!(disclosure(&game, OWNER, SUBJECT), ActorDisclosure::Creature);
    assert!(game.genuinely_sees_actor(OWNER, SUBJECT, 6));

    game.set_cell_light(at(1, 3, 2), false).unwrap();
    assert_eq!(
        disclosure(&game, OWNER, SUBJECT),
        ActorDisclosure::AnonymousContact
    );
    assert!(!game.genuinely_sees_actor(OWNER, SUBJECT, 6));
    assert!(game
        .ability_plan(OWNER, &id("fixture_sight"), SUBJECT)
        .is_err());

    game.set_cell_light(at(1, 3, 2), true).unwrap();
    apply(&mut game, OWNER, "fixture_blind", 1000);
    assert_eq!(
        disclosure(&game, OWNER, SUBJECT),
        ActorDisclosure::AnonymousContact
    );
    assert!(!game.genuinely_sees_actor(OWNER, SUBJECT, 6));
}

#[test]
fn a_stair_landing_never_becomes_an_eye_witness_or_physical_contact() {
    let mut game = fixture(
        &[PerceptionValue::SeeInvisible { enabled: true }],
        &[invisible()],
    );
    let landing = at(2, 2, 2);
    game.teleport(SUBJECT, landing).unwrap();
    game.register_named_anchors(
        landing.region,
        std::collections::BTreeMap::from([("landing".into(), landing.position)]),
    )
    .unwrap();
    game.connect_named_stair(
        at(1, 2, 2),
        Direction::Up,
        tor_world::NamedAnchor {
            region: landing.region,
            name: "landing".into(),
        },
    )
    .unwrap();
    assert!(game
        .scene(OWNER)
        .unwrap()
        .iter()
        .any(|cell| cell.location == landing && !cell.physical));
    assert_eq!(disclosure(&game, OWNER, SUBJECT), ActorDisclosure::Hidden);
    assert!(!game.genuinely_sees_actor(OWNER, SUBJECT, 6));
    let before = game.clone();
    assert!(game
        .admit_intention(OWNER, contact(SUBJECT), IntentionOrigin::Human)
        .is_err());
    assert_eq!(game, before);
}

#[test]
fn observing_invisibility_and_guises_is_rng_clock_and_lifecycle_pure() {
    let game = fixture(&[], &[invisible(), guise(), ambush()]);
    let before = game.clone();
    let first = game.observe(OWNER).unwrap();
    for _ in 0..8 {
        assert_eq!(game.observe(OWNER).unwrap(), first);
        game.classified_actors(OWNER).unwrap();
        game.scene(OWNER).unwrap();
        assert!(!game.genuinely_sees_actor(OWNER, SUBJECT, 6));
        assert!(game
            .capture_public_plan_observation(OWNER, SUBJECT)
            .is_none());
    }
    assert_eq!(game.rules_rng, before.rules_rng);
    assert_eq!(game.tick(), before.tick());
    assert_eq!(game, before);
    assert!(game.actors[&SUBJECT].guise.ambush_available());
}

#[test]
fn one_observation_classifies_each_local_actor_once_for_all_projections() {
    let mut game = fixture(&[], &[invisible()]);
    spawn(&mut game, "fixture_perception_owner", at(1, 2, 3));
    game.observe(OWNER).unwrap();
    let before = crate::diagnostics::work_counts();
    let view = game.observe(OWNER).unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(view.visible_actors.len(), 2);
    assert_eq!(after.observations - before.observations, 1);
    assert_eq!(after.actor_candidates - before.actor_candidates, 3);
    assert_eq!(
        after.perception_projections - before.perception_projections,
        1
    );
    assert_eq!(after.actor_disclosures - before.actor_disclosures, 3);
}

#[test]
fn anonymous_contact_discloses_only_individually_reachable_body_cells() {
    for (extension, blocked_corner) in [([1, 0, 0], false), ([0, 1, 0], true)] {
        let mut game = fixture(&[], &[invisible()]);
        game.set_body(
            SUBJECT,
            crate::BodySpec {
                cells: vec![[0, 0, 0], extension],
                ..Default::default()
            },
        )
        .unwrap();
        if blocked_corner {
            game.set_wall(at(1, 2, 3), true).unwrap();
        }
        let projected = |game: &Game| {
            game.observe(OWNER)
                .unwrap()
                .visible_actors
                .into_iter()
                .filter(|actor| actor.id == SUBJECT)
                .map(|actor| actor.location)
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(projected(&game), BTreeSet::from([at(1, 3, 2)]));
        assert!(game
            .ability_plan_with_supplies(
                OWNER,
                &id("basic_melee"),
                AbilityInvocation::ContactTarget { target: SUBJECT },
                Default::default()
            )
            .is_ok());
        apply(&mut game, OWNER, "fixture_detection", 1000);
        assert_eq!(disclosure(&game, OWNER, SUBJECT), ActorDisclosure::Creature);
        assert_eq!(projected(&game).len(), 2);
    }
}

#[test]
fn anonymous_contact_allows_only_ordinary_basic_melee_in_its_own_domain() {
    let mut game = fixture(&[], &[invisible()]);
    assert_eq!(
        disclosure(&game, OWNER, SUBJECT),
        ActorDisclosure::AnonymousContact
    );
    for rejected in [
        attack(SUBJECT),
        action(
            "fixture_paid_melee",
            AbilityInvocation::ContactTarget { target: SUBJECT },
        ),
        action(
            "fixture_sight",
            AbilityInvocation::ContactTarget { target: SUBJECT },
        ),
        action(
            "fixture_ally",
            AbilityInvocation::ContactTarget { target: SUBJECT },
        ),
        action(
            "fixture_read_plan",
            AbilityInvocation::ContactTarget { target: SUBJECT },
        ),
        action(
            "fixture_sight",
            AbilityInvocation::ActorTarget { target: SUBJECT },
        ),
        action(
            "fixture_ally",
            AbilityInvocation::ActorTarget { target: SUBJECT },
        ),
        action(
            "fixture_read_plan",
            AbilityInvocation::ActorTarget { target: SUBJECT },
        ),
    ] {
        let before = game.clone();
        assert_eq!(
            game.admit_intention(OWNER, rejected, IntentionOrigin::Human),
            Err(GameError::Rule(RuleError::OutOfReach)),
        );
        assert_eq!(game, before);
    }
    assert!(game
        .capture_public_plan_observation(OWNER, SUBJECT)
        .is_none());
    let intention = start(&mut game, OWNER, contact(SUBJECT));
    let pending = game.preparation(OWNER).unwrap();
    assert_eq!(pending.origin_intention(), Some(intention));
    assert_eq!(
        pending.plan.as_ability().unwrap().invocation,
        AbilityInvocation::ContactTarget { target: SUBJECT }
    );
}

#[test]
fn anonymous_contacts_disclose_no_name_asset_affiliation_injury_or_private_plan() {
    let mut game = fixture(&[], &[invisible()]);
    let baseline = game.observe(OWNER).unwrap();
    let contact = baseline
        .visible_actors
        .iter()
        .find(|actor| actor.id == SUBJECT)
        .unwrap();
    assert_eq!(contact.disclosure, ActorDisclosure::AnonymousContact);
    assert!(contact.name.is_empty());
    assert_eq!(contact.asset, None);
    assert!(baseline.combat.as_ref().unwrap().actors.is_empty());

    crate::test_creatures::injure(&mut game, SUBJECT, 10).unwrap();
    {
        let mut actor = game.actors.get_mut(&SUBJECT).unwrap();
        let identity = &mut actor.combat.as_mut().unwrap().identity;
        identity.name = "a different secret name".into();
        identity.faction = "a different secret faction".into();
    }
    game.set_actor_asset(SUBJECT, Some("a_different_secret_asset".into()))
        .unwrap();
    assert_eq!(game.observe(OWNER).unwrap(), baseline);

    game.act(OWNER, Action::Wait).unwrap();
    game.actors.get_mut(&OWNER).unwrap().ready_at = 50;
    start(&mut game, SUBJECT, attack(OWNER));
    assert!(game.preparation(SUBJECT).is_some());
    assert!(game
        .capture_public_plan_observation(OWNER, SUBJECT)
        .is_none());
    assert!(game
        .observe(OWNER)
        .unwrap()
        .combat
        .unwrap()
        .actors
        .is_empty());
}

#[test]
fn apparent_objects_disclose_only_the_authored_object_appearance() {
    let mut game = fixture(&[], &[guise(), ambush()]);
    let baseline = game.observe(OWNER).unwrap();
    assert!(baseline.visible_actors.is_empty());
    assert_eq!(baseline.apparent_objects.len(), 1);
    assert_eq!(baseline.apparent_objects[0].appearance, appearance());
    assert!(baseline.combat.as_ref().unwrap().actors.is_empty());
    assert!(game
        .capture_public_plan_observation(OWNER, SUBJECT)
        .is_none());
    crate::test_creatures::injure(&mut game, SUBJECT, 10).unwrap();
    game.set_actor_asset(SUBJECT, Some("unrelated_private_asset".into()))
        .unwrap();
    assert_eq!(game.observe(OWNER).unwrap(), baseline);
}

#[test]
fn seeing_invisibility_neither_pierces_an_object_guise_nor_removes_invisibility_on_touch() {
    let mut game = fixture(
        &[PerceptionValue::SeeInvisible { enabled: true }],
        &[invisible(), guise(), ambush()],
    );
    assert_eq!(
        disclosure(&game, OWNER, SUBJECT),
        ActorDisclosure::ApparentObject
    );
    assert!(game
        .ability_plan(OWNER, &id("basic_melee"), SUBJECT)
        .is_err());
    assert!(game
        .capture_public_plan_observation(OWNER, SUBJECT)
        .is_none());
    game.act(
        OWNER,
        Action::TouchApparentObject {
            subject: SUBJECT,
            quantity: None,
        },
    )
    .unwrap();
    assert!(game.actor_perception(SUBJECT).unwrap().invisible);
    assert_eq!(disclosure(&game, OWNER, SUBJECT), ActorDisclosure::Creature);
    assert_eq!(game.actors[&SUBJECT].guise.ambush, AmbushState::Spent);
}

#[test]
fn a_paid_touch_with_implicit_or_single_quantity_reveals_the_guise_globally() {
    for quantity in [None, Some(1)] {
        let mut game = fixture(&[], &[guise(), ambush()]);
        let witness = spawn(&mut game, "fixture_perception_owner", at(1, 2, 3));
        assert_eq!(
            disclosure(&game, witness, SUBJECT),
            ActorDisclosure::ApparentObject
        );
        let rng = game.rules_rng;
        game.act(
            OWNER,
            Action::TouchApparentObject {
                subject: SUBJECT,
                quantity,
            },
        )
        .unwrap();
        assert_eq!(game.actors[&OWNER].ready_at, 50);
        assert_eq!(game.rules_rng, rng);
        for observer in [OWNER, witness] {
            let view = game.observe(observer).unwrap();
            assert!(view.apparent_objects.is_empty());
            assert!(view
                .visible_actors
                .iter()
                .any(|actor| actor.id == SUBJECT && actor.disclosure == ActorDisclosure::Creature));
        }
        assert_eq!(game.actors[&SUBJECT].guise.ambush, AmbushState::Spent);
    }
}

#[test]
fn zero_or_multiple_apparent_objects_are_rejected_without_time_or_reveal() {
    for quantity in [0, 2] {
        let mut game = fixture(&[], &[guise(), ambush()]);
        let before = game.clone();
        assert_eq!(
            game.admit_intention(
                OWNER,
                Action::TouchApparentObject {
                    subject: SUBJECT,
                    quantity: Some(quantity)
                },
                IntentionOrigin::Human,
            ),
            Err(GameError::InvalidQuantity),
        );
        assert_eq!(game, before);
    }
}

#[test]
fn ordinary_movement_into_a_guise_pays_for_contact_without_entering_or_attacking() {
    let mut game = fixture(&[], &[guise(), ambush()]);
    let health = game.health(SUBJECT);
    let rng = game.rules_rng;
    let outcome = game.act(OWNER, Action::Move(Direction::East)).unwrap();
    assert_eq!(outcome.kind, crate::OutcomeKind::Waited);
    assert_eq!(game.actors[&OWNER].location, at(1, 2, 2));
    assert_eq!(game.actors[&OWNER].ready_at, 100);
    assert_eq!(game.health(SUBJECT), health);
    assert_eq!(game.rules_rng, rng);
    assert!(game.actors[&SUBJECT].guise.appearance().is_none());
    assert_eq!(game.actors[&SUBJECT].guise.ambush, AmbushState::Spent);
    assert!(game.preparation(OWNER).is_none());
}

#[test]
fn queued_movement_contact_retains_its_destination_and_executes_after_restore() {
    let mut game = fixture(&[], &[guise(), ambush()]);
    let rng = game.rules_rng;
    let intention = game
        .admit_intention(OWNER, Action::Move(Direction::East), IntentionOrigin::Human)
        .unwrap();
    assert_eq!(
        game.pending_intention(OWNER)
            .unwrap()
            .movement_context
            .unwrap()
            .destination(),
        at(1, 3, 2)
    );
    assert_eq!(game.tick(), 0);
    assert!(game.actors[&SUBJECT].guise.appearance().is_some());
    let mut restored = restore(&game);
    let execution = restored.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, intention);
    assert_eq!(execution.outcome.unwrap().kind, crate::OutcomeKind::Waited);
    assert_eq!(restored.actors[&OWNER].location, at(1, 2, 2));
    assert_eq!(restored.actors[&OWNER].ready_at, 100);
    assert_eq!(restored.rules_rng, rng);
    assert!(restored.actors[&SUBJECT].guise.appearance().is_none());
}

fn configure_guise_ai(game: &mut Game, known_foe: bool) {
    use std::collections::{BTreeMap, BTreeSet};
    game.configure_run(
        OWNER,
        BTreeSet::from([OWNER]),
        None,
        if known_foe {
            BTreeMap::from([(
                "private affiliation".into(),
                BTreeSet::from(["private affiliation".into()]),
            )])
        } else {
            BTreeMap::new()
        },
    )
    .unwrap();
    game.configure_ai(SUBJECT, crate::ai::AiProfile::default())
        .unwrap();
}

#[test]
fn intact_guise_ai_stays_stationary_without_an_adjacent_known_foe() {
    for (has_ambush, invisible_foe, known_foe, distant) in [
        (false, false, true, false),
        (true, false, false, false),
        (true, true, true, false),
        (true, false, true, true),
    ] {
        let mut values = vec![guise()];
        if has_ambush {
            values.push(ambush());
        }
        let owner_values = if invisible_foe {
            vec![invisible()]
        } else {
            vec![]
        };
        let mut game = fixture(&owner_values, &values);
        if distant {
            game.teleport(OWNER, at(1, 7, 2)).unwrap();
        }
        configure_guise_ai(&mut game, known_foe);
        let before = game.clone();
        let decision = game.choose_ai(SUBJECT).unwrap();
        assert_eq!(decision.action, Action::Wait);
        assert!(decision.plan.is_none());
        assert_eq!(game, before);
        game.act(OWNER, Action::Wait).unwrap();
        let intention = game.admit_ai_intention(SUBJECT).unwrap();
        let execution = game.execute_next_intention().unwrap();
        assert_eq!(execution.intention.id, intention);
        assert_eq!(execution.action, Some(Action::Wait));
        execution.outcome.unwrap();
        assert_eq!(game.actors[&SUBJECT].location, at(1, 3, 2));
        assert!(game.actors[&SUBJECT].guise.appearance().is_some());
    }
}

#[test]
fn adjacent_known_foe_ai_uses_one_owned_basic_ambush_despite_higher_priority_special_melee() {
    let mut game = fixture(&[], &[guise(), ambush()]);
    configure_guise_ai(&mut game, true);
    let before = game.clone();
    let decision = game.choose_ai(SUBJECT).unwrap();
    assert_eq!(decision.action, attack(OWNER));
    assert!(decision
        .plan
        .as_ref()
        .unwrap()
        .as_ability()
        .unwrap()
        .ambush
        .is_some());
    assert_eq!(
        game, before,
        "a preview must not consume the lifetime claim"
    );
    game.act(OWNER, Action::Wait).unwrap();
    game.actors.get_mut(&OWNER).unwrap().ready_at = 50;
    let origin = game.admit_ai_intention(SUBJECT).unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    assert_eq!(
        game.actors[&SUBJECT].guise.ambush,
        AmbushState::Reserved { origin }
    );
    assert!(game.actors[&SUBJECT].guise.appearance().is_none());
    let due = {
        let pending = game.preparation(SUBJECT).unwrap();
        pending.started + pending.remaining
    };
    game.tick = due;
    game.resolve_preparations().unwrap();
    assert_eq!(game.actors[&SUBJECT].guise.ambush, AmbushState::Spent);
    assert!(game
        .ability_plan(SUBJECT, &id("basic_melee"), OWNER)
        .unwrap()
        .as_ability()
        .unwrap()
        .ambush
        .is_none());
}

#[test]
fn search_pays_one_hundred_ticks_and_reveals_only_physical_adjacent_guises() {
    let mut game = fixture(&[], &[guise(), ambush()]);
    let diagonal = spawn(&mut game, "fixture_perception_subject", at(1, 3, 3));
    let distant = spawn(&mut game, "fixture_perception_subject", at(1, 6, 2));
    let landing = at(2, 2, 2);
    let stair = spawn(&mut game, "fixture_perception_subject", landing);
    game.register_named_anchors(
        landing.region,
        std::collections::BTreeMap::from([("landing".into(), landing.position)]),
    )
    .unwrap();
    game.connect_named_stair(
        at(1, 2, 2),
        Direction::Up,
        tor_world::NamedAnchor {
            region: landing.region,
            name: "landing".into(),
        },
    )
    .unwrap();
    let plan = game
        .ability_plan(OWNER, &id("fixture_search"), OWNER)
        .unwrap();
    assert_eq!((plan.preparation(), plan.recovery()), (50, 50));
    assert!(plan.as_ability().unwrap().strikes[0].check.is_none());
    let rng = game.rules_rng;
    start(
        &mut game,
        OWNER,
        action(
            "fixture_search",
            AbilityInvocation::ActorTarget { target: OWNER },
        ),
    );
    assert!(game.actors[&SUBJECT].guise.appearance().is_some());
    game.tick = 50;
    game.resolve_preparations().unwrap();
    assert_eq!(game.actors[&OWNER].ready_at, 100);
    assert_eq!(
        game.rules_rng, rng,
        "a guaranteed search rolls no discovery check"
    );
    for actor in [SUBJECT, diagonal] {
        assert!(game.actors[&actor].guise.appearance().is_none());
        assert_eq!(game.actors[&actor].guise.ambush, AmbushState::Spent);
    }
    for actor in [distant, stair] {
        assert!(game.actors[&actor].guise.appearance().is_some());
        assert!(game.actors[&actor].guise.ambush_available());
    }
}

#[test]
fn a_prearmed_ambush_enters_on_the_first_legitimate_guise_acquisition() {
    let mut game = fixture(&[ambush()], &[]);
    assert!(!game.actors[&OWNER].guise.ambush_available());
    assert!(game.actors[&OWNER].guise.appearance().is_none());
    apply(&mut game, OWNER, "fixture_guise", 1000);
    assert_eq!(game.actors[&OWNER].guise.appearance(), Some(&appearance()));
    assert!(game.actors[&OWNER].guise.ambush_available());
    assert!(game
        .ability_plan(OWNER, &id("basic_melee"), SUBJECT)
        .unwrap()
        .as_ability()
        .unwrap()
        .ambush
        .is_some());
    let restored = restore(&game);
    assert_eq!(restored, game);
    assert!(restored.actors[&OWNER].guise.ambush_available());
}

#[test]
fn ambush_adds_exactly_one_edge_to_the_first_outgoing_primary_and_nothing_else() {
    let game = fixture_with(&[guise(), ambush()], &[], |_, owner, _| {
        let mut first = owner.melee.strikes()[0].clone();
        first.riders.push(crate::rules::RiderDef {
            trigger: crate::rules::RiderTrigger::Success,
            condition: None,
            check: Some(crate::rules::RiderCheckDef {
                skill: crate::rules::SkillSelector::SelectedWeapon,
                defense: crate::rules::DefenseKind::Physical,
            }),
            effects: vec![],
            descriptors: vec![],
        });
        let mut second = first.clone();
        second.offset = 20;
        owner.melee = crate::attacks::NaturalAttack::new(
            owner.melee.skill(),
            owner.melee.bonus(),
            owner.melee.wind_up(),
            owner.melee.recovery(),
            vec![first, second],
        )
        .unwrap();
    });
    let before = game.clone();
    let plan = game
        .ability_plan(OWNER, &id("basic_melee"), SUBJECT)
        .unwrap();
    assert_eq!(game, before, "preview cannot reserve an ambush");
    let mut revealed = game.clone();
    revealed.reveal_guise(OWNER).unwrap();
    let ordinary = revealed
        .ability_plan(OWNER, &id("basic_melee"), SUBJECT)
        .unwrap();
    let mut strikes = plan.as_ability().unwrap().strikes.clone();
    assert_eq!(strikes.len(), 2);
    strikes[0].check.as_mut().unwrap().edge -= 1;
    assert_eq!(strikes, ordinary.as_ability().unwrap().strikes);
    assert!(plan.as_ability().unwrap().ambush.is_some());
    assert!(ordinary.as_ability().unwrap().ambush.is_none());
}

#[test]
fn ambush_preparation_requires_an_issued_queue_owner_and_rejects_direct_action_atomically() {
    let mut game = fixture(&[guise(), ambush()], &[]);
    let before = game.clone();
    assert_eq!(
        game.act(OWNER, attack(SUBJECT)),
        Err(GameError::Schedule(crate::ScheduleError::InvalidIntention)),
    );
    assert_eq!(game, before);
    let origin = start(&mut game, OWNER, attack(SUBJECT));
    assert_eq!(
        game.actors[&OWNER].guise.ambush,
        AmbushState::Reserved { origin }
    );
    assert_eq!(
        game.preparation(OWNER).unwrap().origin_intention(),
        Some(origin)
    );
}

#[test]
fn paused_ambush_keeps_the_original_owner_and_frozen_check_through_reveal_and_restore() {
    let mut game = fixture(&[guise(), ambush()], &[]);
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = 50;
    let origin = start(&mut game, OWNER, attack(SUBJECT));
    let frozen = game.preparation(OWNER).unwrap().plan.clone();
    game.pause_preparation(OWNER).unwrap();
    let remaining = game.preparation(OWNER).unwrap().remaining;
    assert!(remaining > 0 && remaining < frozen.preparation());
    game.reveal_guise(OWNER).unwrap();
    assert_eq!(game.preparation(OWNER).unwrap().plan, frozen);
    assert_eq!(
        game.actors[&OWNER].guise.ambush,
        AmbushState::Reserved { origin }
    );
    let mut restored = restore(&game);
    assert_eq!(restored, game);
    restored.resume_intention(OWNER, origin).unwrap();
    let queued = restore(&restored);
    assert_eq!(queued.pending_intention(OWNER).unwrap().id, origin);
    assert_eq!(queued.preparation(OWNER).unwrap().plan, frozen);
    assert_eq!(queued.preparation(OWNER).unwrap().remaining, remaining);
    restored.execute_next_intention().unwrap().outcome.unwrap();
    if let Some(pending) = restored.preparation(OWNER) {
        assert_eq!(pending.origin_intention(), Some(origin));
        assert_eq!(pending.plan, frozen);
    } else {
        assert!(restored.combat_events().iter().any(|event| matches!(event,
            CombatEvent::Resolved { outcome, .. } if outcome.intention == Some(origin))));
    }
}

#[test]
fn losing_the_source_during_preparation_preserves_only_the_already_owned_ambush() {
    let mut game = fixture(&[guise(), ambush()], &[]);
    let origin = start(&mut game, OWNER, attack(SUBJECT));
    let frozen = game.preparation(OWNER).unwrap().plan.clone();
    apply(&mut game, OWNER, "fixture_suppression", 1000);
    assert!(!game.actor_perception(OWNER).unwrap().ambush_from_guise);
    assert!(game.actor_perception(OWNER).unwrap().object_guise.is_none());
    assert_eq!(game.preparation(OWNER).unwrap().plan, frozen);
    assert_eq!(
        game.actors[&OWNER].guise.ambush,
        AmbushState::Reserved { origin }
    );
    let mut restored = restore(&game);
    assert_eq!(restored, game);
    let due = {
        let pending = restored.preparation(OWNER).unwrap();
        pending.started + pending.remaining
    };
    restored.tick = due;
    restored.resolve_preparations().unwrap();
    assert_eq!(restored.actors[&OWNER].guise.ambush, AmbushState::Spent);
    assert!(restored.combat_events().iter().any(|event| matches!(event,
        CombatEvent::Resolved { outcome, .. } if outcome.intention == Some(origin))));
}

#[test]
fn ambush_respects_the_three_die_check_cap_without_spilling_into_damage() {
    let mut ambushed = fixture_with(&[guise(), ambush()], &[], |_, owner, _| {
        owner
            .modifiers
            .push(crate::test_creatures::add(crate::rules::Stat::CheckEdge, 3));
        let component = crate::damage::DamageComponent::rolled(
            crate::combat::DamageType::Impact,
            None,
            crate::dice::DicePool::new(1, 4, 0).unwrap(),
        );
        let primary = component.key();
        owner.melee = crate::attacks::NaturalAttack::from_melee(
            crate::attacks::MeleeAttack::new(
                crate::attributes::Skill::HeavyWeaponry,
                100,
                60,
                40,
                crate::damage::DamageSpec::new(vec![component], Some(primary)).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    });
    let health_before = ambushed.health(SUBJECT).unwrap().0;
    let mut ordinary = ambushed.clone();
    ordinary.reveal_guise(OWNER).unwrap();
    for (game, edge) in [(&mut ordinary, 3), (&mut ambushed, 4)] {
        start(game, OWNER, attack(SUBJECT));
        let pending = game.preparation(OWNER).unwrap();
        assert_eq!(
            pending.plan.as_ability().unwrap().strikes[0]
                .check
                .as_ref()
                .unwrap()
                .edge,
            edge
        );
        game.tick = pending.started + pending.remaining;
        game.resolve_preparations().unwrap();
    }
    assert_eq!(ambushed.rules_rng, ordinary.rules_rng);
    assert_eq!(ambushed.health(SUBJECT), ordinary.health(SUBJECT));
    assert!(ambushed.health(SUBJECT).unwrap().0 < health_before);
    assert_eq!(ambushed.combat_events(), ordinary.combat_events());
    assert_eq!(ambushed.actors[&OWNER].guise.ambush, AmbushState::Spent);
}

#[test]
fn cancelling_a_reserved_ambush_spends_it_permanently() {
    let mut game = fixture(&[guise(), ambush()], &[]);
    let origin = start(&mut game, OWNER, attack(SUBJECT));
    game.cancel_intention(OWNER, origin).unwrap();
    assert!(game.preparation(OWNER).is_none());
    assert_eq!(game.actors[&OWNER].guise.ambush, AmbushState::Spent);
    apply(&mut game, OWNER, "fixture_guise", 1000);
    apply(&mut game, OWNER, "fixture_ambush", 1000);
    assert!(game.actors[&OWNER].guise.appearance().is_none());
    assert!(!game.actors[&OWNER].guise.ambush_available());
}

#[test]
fn a_missed_first_strike_spends_the_ambush_without_rearming() {
    let mut game = fixture_with(&[guise(), ambush()], &[], |_, owner, _| {
        owner.melee =
            crate::test_creatures::natural_melee(-100, 60, crate::combat::DamageType::Impact, 4);
    });
    let origin = start(&mut game, OWNER, attack(SUBJECT));
    let due = {
        let pending = game.preparation(OWNER).unwrap();
        pending.started + pending.remaining
    };
    game.tick = due;
    game.resolve_preparations().unwrap();
    assert!(game.combat_events().iter().any(|event| matches!(event,
        CombatEvent::Resolved { outcome, .. } if outcome.intention == Some(origin) && outcome.result == OutcomeStatus::Miss)));
    assert_eq!(game.actors[&OWNER].guise.ambush, AmbushState::Spent);
    assert!(!game.actors[&OWNER].guise.ambush_available());
    assert!(game
        .ability_plan(OWNER, &id("basic_melee"), SUBJECT)
        .unwrap()
        .as_ability()
        .unwrap()
        .ambush
        .is_none());
}

#[test]
fn source_loss_and_regrant_cannot_reenter_the_original_guise_or_rearm_ambush() {
    let mut game = fixture(&[guise(), ambush()], &[]);
    apply(&mut game, OWNER, "fixture_suppression", 1);
    assert!(game.actors[&OWNER].guise.appearance().is_none());
    assert_eq!(game.actors[&OWNER].guise.ambush, AmbushState::Spent);
    game.act(OWNER, Action::Wait).unwrap();
    game.act(SUBJECT, Action::Wait).unwrap();
    assert!(game.actor_perception(OWNER).unwrap().object_guise.is_some());
    assert!(game.actor_perception(OWNER).unwrap().ambush_from_guise);
    assert!(game.actors[&OWNER].guise.appearance().is_none());
    assert!(!game.actors[&OWNER].guise.ambush_available());
    assert_eq!(restore(&game), game);
}

#[test]
fn death_retires_the_reserved_claim_and_new_incarnations_get_their_own_lifetime() {
    let mut game = fixture(&[guise(), ambush()], &[]);
    start(&mut game, OWNER, attack(SUBJECT));
    crate::test_creatures::injure(&mut game, OWNER, 10000).unwrap();
    assert!(game.preparation(OWNER).is_none());
    assert_eq!(game.actors[&OWNER].guise.ambush, AmbushState::Spent);
    let replacement = spawn(&mut game, "fixture_perception_owner", at(1, 2, 2));
    assert!(replacement.0 > SUBJECT.0);
    assert_ne!(replacement, OWNER);
    assert!(game.actors[&replacement].guise.ambush_available());
    assert_eq!(restore(&game), game);
}

#[test]
fn incoming_and_outgoing_contact_attacks_never_remove_invisibility() {
    let mut game = fixture(&[], &[invisible()]);
    start(&mut game, OWNER, contact(SUBJECT));
    game.tick = 60;
    game.resolve_preparations().unwrap();
    assert!(game.actor_perception(SUBJECT).unwrap().invisible);
    assert_eq!(
        disclosure(&game, OWNER, SUBJECT),
        ActorDisclosure::AnonymousContact
    );
    start(&mut game, SUBJECT, attack(OWNER));
    let due = {
        let pending = game.preparation(SUBJECT).unwrap();
        pending.started + pending.remaining
    };
    game.tick = due;
    game.resolve_preparations().unwrap();
    assert!(game.actor_perception(SUBJECT).unwrap().invisible);
    assert_eq!(
        disclosure(&game, OWNER, SUBJECT),
        ActorDisclosure::AnonymousContact
    );
}

#[test]
fn invisible_contact_does_not_allow_attacks_or_movement_through_a_wall() {
    let mut game = fixture(&[], &[invisible()]);
    game.teleport(SUBJECT, at(1, 4, 2)).unwrap();
    game.set_wall(at(1, 3, 2), true).unwrap();
    let before = game.clone();
    assert!(game
        .admit_intention(OWNER, contact(SUBJECT), IntentionOrigin::Human)
        .is_err());
    assert!(game.act(OWNER, Action::Move(Direction::East)).is_err());
    assert_eq!(game, before);
    assert_eq!(game.actors[&OWNER].location, at(1, 2, 2));
    assert!(game.actor_perception(SUBJECT).unwrap().invisible);
}

fn corpse_fixture(lit: bool) -> (Game, crate::ItemId) {
    let mut game = fixture(&[], &[invisible()]);
    game.set_cell_light(at(1, 3, 2), lit).unwrap();
    assert_eq!(
        disclosure(&game, OWNER, SUBJECT),
        ActorDisclosure::AnonymousContact
    );
    crate::test_creatures::injure(&mut game, SUBJECT, 10000).unwrap();
    let corpse = game
        .items
        .iter()
        .find_map(|(id, item)| {
            item.corpse
                .as_ref()
                .is_some_and(|appearance| appearance.actor == SUBJECT)
                .then_some(*id)
        })
        .unwrap();
    (game, corpse)
}

#[test]
fn an_invisible_living_creature_leaves_an_identified_corpse_in_current_light() {
    let (game, corpse) = corpse_fixture(true);
    let view = game.observe(OWNER).unwrap();
    assert!(view.visible_actors.iter().all(|actor| actor.id != SUBJECT));
    let item = view
        .ground_items
        .iter()
        .find(|item| item.id == corpse)
        .unwrap();
    assert!(item.identified);
    assert_eq!(item.name, "private creature 2 corpse");
    assert_eq!(item.appearance, "private creature 2 corpse");
    let source = item.corpse.as_ref().unwrap();
    assert_eq!(source.actor, SUBJECT);
    assert_eq!(source.asset.as_deref(), Some("private_creature_asset"));
}

#[test]
fn a_dark_tactile_corpse_discloses_no_name_appearance_asset_or_source_identity() {
    let (game, corpse) = corpse_fixture(false);
    let view = game.observe(OWNER).unwrap();
    let item = view
        .ground_items
        .iter()
        .find(|item| item.id == corpse)
        .unwrap();
    assert_eq!(item.class, crate::ItemClass::Corpse);
    assert!(!item.identified);
    assert!(!item.name.is_empty() && !item.appearance.is_empty());
    assert!(!item.name.contains("private"));
    assert!(!item.appearance.contains("private"));
    assert!(!item.description.contains("private"));
    assert_eq!(item.asset, None);
    assert_eq!(
        item.corpse, None,
        "no incarnation or visual seed source through touch"
    );
}

#[test]
fn taking_a_dark_corpse_allows_ordinary_carried_identification() {
    let (mut game, corpse) = corpse_fixture(false);
    game.act(OWNER, Action::Move(Direction::East)).unwrap();
    game.act(
        OWNER,
        Action::Take {
            item: corpse,
            quantity: None,
        },
    )
    .unwrap();
    let view = game.observe(OWNER).unwrap();
    let item = view
        .inventory
        .iter()
        .find(|item| item.id == corpse)
        .unwrap();
    assert!(item.identified);
    assert_eq!(item.name, "private creature 2 corpse");
    assert_eq!(item.appearance, "private creature 2 corpse");
    assert_eq!(item.corpse.as_ref().unwrap().actor, SUBJECT);
    assert_eq!(
        item.corpse.as_ref().unwrap().asset.as_deref(),
        Some("private_creature_asset")
    );
    assert!(view.ground_items.iter().all(|item| item.id != corpse));
    assert_eq!(restore(&game), game);
}

#[test]
fn rewind_restores_the_original_guise_boundary_without_resetting_the_later_boundary() {
    let mut game = fixture(&[], &[guise(), ambush()]);
    let mut shared = crate::checkpoint::SharedState::default();
    let before = game.checkpoint(&mut shared);
    game.act(
        OWNER,
        Action::TouchApparentObject {
            subject: SUBJECT,
            quantity: None,
        },
    )
    .unwrap();
    let after = game.checkpoint(&mut shared);
    let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
    let earlier = Game::restore_checkpoint(before, &shared, game.content().clone()).unwrap();
    let later = Game::restore_checkpoint(after, &shared, game.content().clone()).unwrap();
    assert_eq!(
        disclosure(&earlier, OWNER, SUBJECT),
        ActorDisclosure::ApparentObject
    );
    assert!(earlier.actors[&SUBJECT].guise.ambush_available());
    assert_eq!(
        disclosure(&later, OWNER, SUBJECT),
        ActorDisclosure::Creature
    );
    assert_eq!(later.actors[&SUBJECT].guise.ambush, AmbushState::Spent);
    assert_eq!(later, game);
}

fn forged_checkpoint(
    game: &Game,
    actor: ActorId,
    edit: impl FnOnce(&mut serde_json::Value),
) -> Result<Game, crate::checkpoint::PersistError> {
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let snapshot_json = serde_json::to_value(&snapshot).unwrap();
    let pool = snapshot_json["actors"].as_u64().unwrap() as usize;
    let mut shared_json = serde_json::to_value(&shared).unwrap();
    edit(&mut shared_json["actors"][pool][actor.0.to_string()]);
    let shared = serde_json::from_value(shared_json).unwrap();
    Game::restore_checkpoint(snapshot, &shared, game.content().clone())
}

#[test]
fn restore_rejects_a_reserved_ambush_claim_owned_by_another_issued_intention() {
    let mut game = fixture(&[guise(), ambush()], &[]);
    let owner = start(&mut game, OWNER, attack(SUBJECT));
    let unrelated = game
        .admit_intention(SUBJECT, Action::Wait, IntentionOrigin::Human)
        .unwrap();
    assert_ne!(owner, unrelated);
    let result = forged_checkpoint(&game, OWNER, |actor| {
        actor["guise"]["ambush"]["origin"] = serde_json::json!(unrelated);
    });
    assert!(
        result.is_err(),
        "a reserved claim must have its exact original owner"
    );
}

#[test]
fn restore_rejects_a_live_guise_or_available_ambush_without_its_authored_source() {
    let game = fixture(&[], &[]);
    let result = forged_checkpoint(&game, SUBJECT, |actor| {
        actor["guise"] = serde_json::json!({
            "entered":true,"appearance":appearance(),"ambush":{"state":"available"}
        });
    });
    assert!(
        result.is_err(),
        "restore must not invent current guise or ambush grants"
    );
}

#[test]
fn saved_guise_and_ambush_records_reject_noncanonical_shapes_and_invalid_owners() {
    for invalid in [
        serde_json::json!([true, null, {"state":"spent"}]),
        serde_json::json!({"entered":true,"ambush":{"state":"spent"}}),
        serde_json::json!({"entered":false,"appearance":appearance(),"ambush":{"state":"unacquired"}}),
        serde_json::json!({"entered":true,"appearance":null,"ambush":{"state":"available"}}),
        serde_json::json!({"entered":true,"appearance":null,"ambush":{"state":"reserved","origin":0}}),
        serde_json::json!({"entered":true,"appearance":null,"ambush":{"state":"reserved","origin":u64::MAX}}),
        serde_json::json!({"entered":true,"appearance":null,"ambush":{"state":"spent","origin":1}}),
        serde_json::json!({"entered":true,"appearance":null,"ambush":{"state":"spent"},"reset":true}),
    ] {
        assert!(serde_json::from_value::<GuiseState>(invalid).is_err());
    }
    for state in [
        GuiseState::default(),
        GuiseState {
            entered: true,
            appearance: None,
            ambush: AmbushState::Reserved {
                origin: crate::IntentionId(1),
            },
        },
    ] {
        let restored: GuiseState =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored, state);
    }
}

#[test]
fn item_preparation_keeps_known_threat_reference_domains_separate() {
    for initially_detected in [false, true] {
        let mut game = fixture(&[], &[invisible()]);
        game.configure_run(
            OWNER,
            [OWNER].into(),
            None,
            [(
                "private affiliation".into(),
                ["private affiliation".into()].into(),
            )]
            .into(),
        )
        .unwrap();
        if initially_detected {
            apply(&mut game, OWNER, "fixture_detection", 1000);
        }
        // This fixture supplies a personally established contact hostility. The
        // event/provocation contract separately owns acquiring that knowledge.
        game.actors.get_mut(&OWNER).unwrap().behavior.engagement =
            Some(crate::behavior::Engagement {
                target: SUBJECT,
                reference: if initially_detected {
                    ActorReferenceKind::Creature
                } else {
                    ActorReferenceKind::Contact
                },
                location: at(1, 3, 2),
                last_seen: 0,
            });
        let mut potion = crate::ItemSpec::ordinary("ordinary potion".into());
        potion.class = crate::ItemClass::Potion;
        potion.consumable = Some(crate::ConsumableSpec {
            effects: vec![crate::rules::Effect::Heal {
                amount: crate::rules::AmountExpr::Fixed { amount: 1 },
            }],
        });
        game.place_item_stack(50, at(1, 2, 2), Some(OWNER), 1, potion)
            .unwrap();
        start(
            &mut game,
            OWNER,
            Action::Drink {
                item: crate::ItemId(50),
            },
        );
        assert!(game.preparation(OWNER).unwrap().active);
        let mut same_domain = restore(&game);
        same_domain.resolve_preparations().unwrap();
        assert!(
            same_domain.preparation(OWNER).unwrap().active,
            "same-domain known threat must not interrupt again"
        );
        if initially_detected {
            apply(&mut game, OWNER, "fixture_blind", 1000);
            game.actors
                .get_mut(&OWNER)
                .unwrap()
                .behavior
                .engagement
                .as_mut()
                .unwrap()
                .reference = ActorReferenceKind::Contact;
        } else {
            apply(&mut game, OWNER, "fixture_detection", 1000);
        }
        game.resolve_preparations().unwrap();
        // This internal resolution fixture owns the same final publication step
        // as the public action transaction before checkpointing the interruption.
        game.seal_participant_events().unwrap();
        assert!(
            !game.preparation(OWNER).unwrap().active,
            "a new disclosure domain is not the prior known threat"
        );
        assert_eq!(
            restore(&game),
            game,
            "historical domain remains valid after current senses change"
        );
    }
}

#[path = "contact_tests.rs"]
mod contact_topology_contracts;
