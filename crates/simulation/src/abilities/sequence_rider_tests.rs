//! Sequence-local rider contracts use ordinary paid preparation and publication.
use crate::{combat::CombatEvent, rules::*, ActorId};

fn rending_rider() -> RiderDef {
    RiderDef {
        trigger: RiderTrigger::Success,
        condition: Some(RiderCondition::SequenceSuccessfulHits { minimum: 2 }),
        check: None,
        effects: vec![Effect::Damage {
            damage: DamageExpr::Components {
                components: vec![DamageComponentExpr {
                    category: crate::combat::DamageType::Keen,
                    descriptor: None,
                    amount: AmountExpr::Rolled {
                        dice: DiceExpr {
                            count: 1,
                            sides: 6,
                            bonus: 0,
                        },
                    },
                }],
            },
        }],
        descriptors: vec![],
    }
}

pub(super) fn sequence_game() -> crate::Game {
    let (game, _) = super::strike_tests::sequence_game_custom(
        &[0, 10],
        |ability| {
            ability.strikes = StrikeSource::NaturalMelee {
                additional_riders: vec![rending_rider()],
            };
        },
        |_, species| {
            let strikes = [0, 10]
                .into_iter()
                .map(|offset| StrikeDef {
                    offset,
                    unlock: None,
                    check: Some(CheckDef {
                        actor: CheckActor::User,
                        skill: SkillSelector::SelectedWeapon,
                        against: Against::Defense {
                            defense: DefenseKind::Physical,
                        },
                    }),
                    on_success: vec![Effect::Damage {
                        damage: DamageExpr::Components {
                            components: vec![DamageComponentExpr {
                                category: crate::combat::DamageType::Impact,
                                descriptor: None,
                                amount: AmountExpr::Fixed { amount: 3 },
                            }],
                        },
                    }],
                    on_failure: vec![],
                    descriptors: vec![],
                    riders: vec![],
                })
                .collect();
            species.melee = crate::attacks::NaturalAttack::new(
                crate::attributes::Skill::HeavyWeaponry,
                100,
                10,
                20,
                strikes,
            )
            .unwrap();
        },
    );
    game
}

pub(super) fn rider_count(game: &crate::Game) -> usize {
    game.combat_events()
        .iter()
        .filter(|event| {
            matches!(event,
                CombatEvent::Resolved { outcome, strike: None }
                if outcome.actor == ActorId(1) && outcome.target == ActorId(2)
                    && outcome.ability.is_none()
            )
        })
        .count()
}

#[test]
fn sequence_rider_frozen_condition_is_required_and_bounded() {
    use serde_json::json;
    let valid = json!({"trigger":"success", "condition":{"kind":"sequence_successful_hits","minimum":2},
        "check":null,"effects":[],"descriptors":[]});
    let decoded: super::FrozenRider = serde_json::from_value(valid.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), valid);
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("condition");
    assert!(serde_json::from_value::<super::FrozenRider>(missing).is_err());
    for minimum in [0, 1, 17, u16::MAX] {
        let mut bad = valid.clone();
        bad["condition"]["minimum"] = json!(minimum);
        assert!(serde_json::from_value::<super::FrozenRider>(bad).is_err());
    }
    let mut ordinary = valid;
    ordinary["condition"] = json!(null);
    assert!(serde_json::from_value::<super::FrozenRider>(ordinary).is_ok());
}

#[test]
fn sequence_rider_frozen_copies_must_match_their_authored_predicate_and_payload() {
    use serde_json::json;
    let game = sequence_game();
    let captured = &game.preparation(ActorId(1)).unwrap().plan;
    assert!(game.plan_source_valid(captured));
    let valid = serde_json::to_value(captured).unwrap();
    for (field, value) in [
        (
            "condition",
            json!({"kind":"sequence_successful_hits","minimum":3}),
        ),
        ("condition", json!(null)),
        ("effects", json!([])),
    ] {
        let mut forged = valid.clone();
        forged["strikes"][1]["riders"][0][field] = value;
        let forged: super::Plan = serde_json::from_value(forged).unwrap();
        assert!(!game.plan_source_valid(&forged), "changed copied {field}");
    }
}

#[test]
fn sequence_rider_changed_copied_dice_reject_at_initial_plan_and_checkpoint_admission() {
    use serde_json::json;
    let mut game = sequence_game();
    game.actors.get_mut(&ActorId(2)).unwrap().ready_at = game.tick;
    let captured = &game.preparation(ActorId(1)).unwrap().plan;
    assert!(captured.valid(game.content().rules()));
    let valid = serde_json::to_value(captured).unwrap();
    let amount =
        &valid["strikes"][1]["riders"][0]["effects"][0]["damage"]["components"][0]["amount"];
    assert_eq!(amount["count"], json!(1));
    assert_eq!(amount["bonus"], json!(0));
    let mut shared = crate::checkpoint::SharedState::default();
    let snapshot = serde_json::to_value(game.checkpoint(&mut shared)).unwrap();
    let stored = serde_json::to_value(shared).unwrap();
    assert_eq!(
        crate::Game::restore_checkpoint(
            serde_json::from_value(snapshot.clone()).unwrap(),
            &serde_json::from_value(stored.clone()).unwrap(),
            game.content().clone(),
        )
        .unwrap(),
        game
    );
    for (field, changed) in [("count", 2), ("bonus", 1)] {
        let mut forged = valid.clone();
        forged["strikes"][1]["riders"][0]["effects"][0]["damage"]["components"][0]["amount"]
            [field] = json!(changed);
        let decoded: super::Plan = serde_json::from_value(forged.clone()).unwrap();
        // Source shape intentionally permits frozen modifier operands. Copy
        // consistency must reject this before the first hit consults any slot.
        assert!(super::plan::effects_match_definition(
            &rending_rider().effects,
            &decoded.as_ability().unwrap().strikes[1].riders[0].effects,
        ));
        assert!(
            !decoded.valid(game.content().rules()),
            "changed copied dice {field}"
        );
        let mut bad = stored.clone();
        bad["actors"][snapshot["actors"].as_u64().unwrap() as usize]["1"]["pending"]["plan"] =
            forged;
        assert!(
            crate::Game::restore_checkpoint(
                serde_json::from_value(snapshot.clone()).unwrap(),
                &serde_json::from_value(bad).unwrap(),
                game.content().clone(),
            )
            .is_err(),
            "initial checkpoint accepted changed copied dice {field}"
        );
    }
}

#[test]
fn sequence_rider_two_hits_apply_one_rider_only_after_the_second_primary() {
    let mut game = sequence_game();
    game.tick = 10;
    game.resolve_preparations().unwrap();
    assert_eq!(game.combat_events().iter().filter(|event| matches!(event,
        CombatEvent::Resolved { outcome, strike: Some(0) }
        if outcome.actor == ActorId(1) && outcome.target == ActorId(2)
            && outcome.check.is_some_and(|check| check.actor == CheckActor::User && check.success)
    )).count(), 1, "tick 10 must resolve the first initiating primary successfully");
    assert_eq!(
        rider_count(&game),
        0,
        "one successful primary must not run the conditional rider"
    );
    assert_eq!(game.health(ActorId(2)).unwrap().0, 97);
    game.tick = 20;
    game.resolve_preparations().unwrap();
    assert_eq!(
        rider_count(&game),
        1,
        "two successful primaries run the copied source rider exactly once"
    );
    assert!((88..=93).contains(&game.health(ActorId(2)).unwrap().0));
    assert!(game.preparation(ActorId(1)).is_none());
}
