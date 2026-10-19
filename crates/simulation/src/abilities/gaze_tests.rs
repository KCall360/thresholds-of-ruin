//! Mutual sight and confusion use ordinary checked abilities and active timers.
//! Fixtures make existing personal hostility explicit; choices cannot read foe conditions.
use super::*;
use crate::attributes::{ManaBinding, Skill};
use crate::content_ids::{AbilityId, ConditionId};
use crate::locomotion::{LocomotionValue, MovementChance};
use crate::rules::*;
use crate::{Action, ActorId, Game, IntentionOrigin};
use std::num::NonZeroU64;
use tor_world::{Location, Position, RegionId};

fn condition() -> ConditionDef {
    serde_json::from_value(serde_json::json!({
        "id":"fixture_confused","family":"fixture_confused","lifecycle":"timed",
        "name":"Confused","modifiers":[{"scope":"always","op":{"kind":"locomotion",
            "value":{"kind":"confused","chance":{"numerator":4,"denominator":5}}}}],
        "stacking":{"kind":"keep"},"source_scoped":false,"descriptors":[],"capacity":"reject",
        "timing":{"kind":"actor_active","on_expiry":{"kind":"protect","duration":400}},
        "restrictions":[],"preparation":"preserve"
    }))
    .unwrap()
}
fn fixture() -> Game {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.conditions.push(condition());
    let mut gaze = document
        .abilities
        .iter()
        .find(|ability| ability.basic_attack)
        .unwrap()
        .clone();
    gaze.id = AbilityId::new("fixture_gaze").unwrap();
    gaze.basic_attack = false;
    gaze.reach = Reach::MutualSight { range: 4 };
    gaze.timing = Timing {
        source: TimingSource::Fixed {
            prepare: 100,
            recover: 100,
        },
        scaled_by: None,
    };
    gaze.cooldown = NonZeroU64::new(700);
    gaze.cost = None;
    gaze.strikes = StrikeSource::Authored {
        strikes: vec![StrikeDef {
            offset: 0,
            unlock: None,
            check: Some(CheckDef {
                actor: CheckActor::User,
                skill: SkillSelector::Named {
                    skill: Skill::Spellcasting,
                },
                against: Against::Defense {
                    defense: DefenseKind::Spiritual,
                },
            }),
            on_success: vec![Effect::ApplyCondition {
                condition: condition().id,
                duration: 300,
            }],
            on_failure: vec![],
            descriptors: vec![],
            riders: vec![],
        }],
    };
    document.abilities.push(gaze.clone());
    let mut species = crate::test_creatures::species();
    species.modifiers.extend([
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::GrantAbility { ability: gaze.id },
        },
        Modifier {
            scope: Scope::Always,
            op: ModifierOp::Set {
                stat: Stat::Health,
                value: 100,
            },
        },
    ]);
    let id = species.id.clone();
    let content = crate::creatures::Content::new(
        Ruleset::validate(document).unwrap(),
        crate::creatures::ScenarioCatalog {
            species: [(id.clone(), species)].into(),
            templates: Default::default(),
        },
    )
    .unwrap();
    let build = crate::creatures::CreatureBuild::new(
        content.clone(),
        id,
        crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
            .unwrap(),
        ManaBinding::Presence,
        [Skill::Deception, Skill::Persuasion],
    )
    .unwrap();
    let mut game = Game::two_room_in_stone(42, content);
    for x in [1, 3] {
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: format!("subject {x}"),
                faction: format!("side{x}"),
            },
            build.clone(),
        )
        .unwrap();
    }
    game
}
fn gaze() -> Action {
    Action::UseAbility {
        id: AbilityId::new("fixture_gaze").unwrap(),
        invocation: AbilityInvocation::actor_target(ActorId(2)),
        supplies: Default::default(),
    }
}
fn start(game: &mut Game) {
    game.admit_intention(ActorId(1), gaze(), IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
}

#[test]
fn mutual_sight_requires_real_eyes_in_both_directions_before_preparation() {
    let game = fixture();
    let id = AbilityId::new("fixture_gaze").unwrap();
    assert!(game.ability_plan(ActorId(1), &id, ActorId(2)).is_ok());
    for dark_actor in [ActorId(1), ActorId(2)] {
        let mut dark = game.clone();
        let at = dark.actors[&dark_actor].location;
        dark.set_cell_light(at, false).unwrap();
        let before = dark.clone();
        assert!(dark.ability_plan(ActorId(1), &id, ActorId(2)).is_err());
        assert_eq!(dark, before);
    }
}

#[test]
fn mutual_sight_is_rechecked_at_completion_without_spending_a_check_roll() {
    let mut game = fixture();
    start(&mut game);
    let rng = game.rules_rng;
    let source = game.actors[&ActorId(1)].location;
    game.set_cell_light(source, false).unwrap();
    game.tick = 100;
    game.resolve_preparations().unwrap();
    assert!(game
        .creature(ActorId(2))
        .unwrap()
        .conditions()
        .instances()
        .all(|entry| entry.condition != condition().id));
    assert_eq!(game.rules_rng, rng);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .cooldowns()
            .remaining(&AbilityId::new("fixture_gaze").unwrap()),
        600
    );
}

#[test]
fn gaze_uses_presence_spellcasting_three_dice_and_own_seven_hundred_cooldown() {
    let mut game = fixture();
    let ability = AbilityId::new("fixture_gaze").unwrap();
    assert_eq!(
        Skill::Spellcasting.attribute(ManaBinding::Presence),
        crate::attributes::Attribute::Presence
    );
    let plan = game.ability_plan(ActorId(1), &ability, ActorId(2)).unwrap();
    assert_eq!((plan.preparation(), plan.recovery()), (100, 100));
    assert_eq!(
        plan.as_ability().unwrap().reach,
        Reach::MutualSight { range: 4 }
    );
    start(&mut game);
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .cooldowns()
            .remaining(&ability),
        700
    );
    let mut expected_rng = game.rules_rng;
    crate::dice::roll_check(&mut expected_rng, crate::dice::Edge::default());
    game.tick = 100;
    game.resolve_preparations().unwrap();
    assert_eq!(
        game.rules_rng, expected_rng,
        "one ordinary unmodified 3d6 check"
    );
    assert_eq!(game.actors[&ActorId(1)].ready_at, 200);
    game.tick = 699;
    game.settle_creature_time().unwrap();
    assert_eq!(
        game.creature(ActorId(1))
            .unwrap()
            .cooldowns()
            .remaining(&ability),
        1
    );
    assert!(game.ability_plan(ActorId(1), &ability, ActorId(2)).is_err());
    game.tick = 700;
    game.settle_creature_time().unwrap();
    assert!(game.ability_plan(ActorId(1), &ability, ActorId(2)).is_ok());
}

#[test]
fn confusion_three_hundred_active_then_four_hundred_protection_never_refreshes() {
    let mut game = fixture();
    let id = ConditionId::new("fixture_confused").unwrap();
    game.apply_condition(ActorId(2), &id, ActorId(1), 300)
        .unwrap();
    assert_eq!(
        game.actor_locomotion(ActorId(2)).unwrap().confused,
        Some(MovementChance {
            numerator: 4,
            denominator: 5
        })
    );
    game.tick = 299;
    game.apply_condition(ActorId(2), &id, ActorId(1), 300)
        .unwrap();
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .conditions()
            .instances()
            .find(|entry| entry.condition == id)
            .unwrap()
            .remaining,
        1
    );
    game.tick = 300;
    game.settle_creature_time().unwrap();
    assert_eq!(game.actor_locomotion(ActorId(2)).unwrap().confused, None);
    let protected = game.creature(ActorId(2)).unwrap();
    let entry = protected
        .conditions()
        .instances()
        .find(|entry| entry.condition == id)
        .unwrap();
    assert_eq!(entry.phase, crate::conditions::ConditionPhase::Protection);
    assert_eq!(entry.remaining, 400);
    drop(protected);
    game.tick = 699;
    game.apply_condition(ActorId(2), &id, ActorId(1), 300)
        .unwrap();
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .conditions()
            .instances()
            .find(|entry| entry.condition == id)
            .unwrap()
            .remaining,
        1
    );
    game.tick = 700;
    game.settle_creature_time().unwrap();
    assert!(game
        .creature(ActorId(2))
        .unwrap()
        .conditions()
        .instances()
        .all(|entry| entry.condition != id));
    game.apply_condition(ActorId(2), &id, ActorId(1), 300)
        .unwrap();
    assert!(game
        .actor_locomotion(ActorId(2))
        .unwrap()
        .confused
        .is_some());
}

#[test]
fn protected_confusion_admits_only_the_infallible_typed_modifier() {
    let mut document = crate::test_creatures::ruleset().document().clone();
    document.conditions.push(condition());
    assert!(Ruleset::validate(document.clone()).is_ok());
    for value in [
        ModifierOp::Add {
            stat: Stat::Health,
            value: 1,
        },
        ModifierOp::Locomotion {
            value: LocomotionValue::Flying { enabled: false },
        },
        ModifierOp::Locomotion {
            value: LocomotionValue::Stationary { enabled: true },
        },
    ] {
        let mut invalid = document.clone();
        invalid.conditions.last_mut().unwrap().modifiers[0].op = value;
        assert!(Ruleset::validate(invalid).is_err());
    }
}

#[test]
fn mutual_sight_saved_reach_is_strict_and_range_bounded() {
    assert_eq!(
        serde_json::from_value::<Reach>(serde_json::json!({"kind":"mutual_sight","range":4}))
            .unwrap(),
        Reach::MutualSight { range: 4 }
    );
    for value in [
        serde_json::json!({"kind":"mutual_sight"}),
        serde_json::json!({"kind":"mutual_sight","range":4,"awareness":true}),
        serde_json::json!(["mutual_sight", 4]),
    ] {
        assert!(serde_json::from_value::<Reach>(value).is_err());
    }
    for range in [0, crate::limits::SIGHT_RANGE + 1] {
        let mut document = crate::test_creatures::ruleset().document().clone();
        document.abilities[0].reach = Reach::MutualSight { range };
        assert!(Ruleset::validate(document).is_err());
    }
}

#[test]
fn gaze_ai_does_not_inspect_hidden_target_confusion_or_protection() {
    let mut game = fixture();
    game.configure_ai(ActorId(1), crate::ai::AiProfile::default())
        .unwrap();
    // Isolate ability selection from independent mean-creature acquisition and
    // wake rolls: the target is already personally known in the creature domain.
    game.actors
        .get_mut(&ActorId(1))
        .unwrap()
        .behavior
        .engagement = Some(crate::behavior::Engagement {
        reference: crate::perception::ActorReferenceKind::Creature,
        target: ActorId(2),
        location: game.actors[&ActorId(2)].location,
        last_seen: 0,
    });
    let clear = game.next_ai_action().unwrap();
    assert_eq!(clear.1, gaze());
    game.apply_condition(ActorId(2), &condition().id, ActorId(1), 300)
        .unwrap();
    assert_eq!(game.next_ai_action(), Some(clear.clone()));
    game.tick = 300;
    game.settle_creature_time().unwrap();
    assert_eq!(
        game.creature(ActorId(2))
            .unwrap()
            .conditions()
            .instances()
            .find(|entry| entry.condition == condition().id)
            .unwrap()
            .phase,
        crate::conditions::ConditionPhase::Protection
    );
    assert_eq!(game.next_ai_action(), Some(clear));
}

#[test]
fn confused_is_direct_harm_and_cannot_hide_in_a_non_hostile_ability() {
    let game = fixture();
    let mut document = game.content().rules().document().clone();
    document
        .abilities
        .iter_mut()
        .find(|ability| ability.id.as_str() == "fixture_gaze")
        .unwrap()
        .interaction = Interaction::NonHostile;
    assert!(Ruleset::validate(document).is_err());
}

#[test]
fn confusion_does_not_redirect_or_recompile_a_paid_movement_ability() {
    let mut game = super::movement_tests::guarded_step_game_with(100, |document| {
        document["conditions"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(condition()).unwrap());
    });
    let actor = ActorId(1);
    let action = Action::UseAbility {
        id: AbilityId::new("guarded_step_probe").unwrap(),
        invocation: AbilityInvocation::self_step(StepDirection::North),
        supplies: Default::default(),
    };
    game.admit_intention(actor, action, IntentionOrigin::Human)
        .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let frozen = game.preparation(actor).unwrap().plan.clone();
    game.apply_condition(actor, &condition().id, ActorId(2), 300)
        .unwrap();
    assert!(game.preparation(actor).unwrap().active);
    assert_eq!(game.preparation(actor).unwrap().plan, frozen);
    let rng = game.rules_rng;
    game.tick = 100;
    game.resolve_preparations().unwrap();
    assert_eq!(
        game.actors[&actor].location.position,
        Position { x: 2, y: 1, z: 0 }
    );
    assert_eq!(game.rules_rng, rng);
}
