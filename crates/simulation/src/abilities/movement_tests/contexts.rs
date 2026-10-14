//! Context authority belongs to constructors and admission, never silent no-ops.
use super::*;

fn spatial_effects() -> [crate::rules::Effect; 2] {
    [
        crate::rules::Effect::StepSelfOneCell,
        crate::rules::Effect::PushTargetOneCell {
            direction: super::super::PushDirection::AwayFromSource,
        },
    ]
}

#[test]
fn natural_attack_constructor_rejects_spatial_branches_and_riders() {
    let ordinary = crate::test_creatures::species().melee;
    assert!(crate::attacks::NaturalAttack::new(
        ordinary.skill(),
        ordinary.bonus(),
        ordinary.wind_up(),
        ordinary.recovery(),
        ordinary.strikes().to_vec()
    )
    .is_ok());
    for effect in spatial_effects() {
        for branch in 0..3 {
            let mut strikes = ordinary.strikes().to_vec();
            match branch {
                0 => strikes[0].on_success.insert(0, effect.clone()),
                1 => strikes[0].on_failure.push(effect.clone()),
                _ => strikes[0].riders.push(
                    serde_json::from_value(json!({
                        "trigger":"success","check":null,"descriptors":[],"effects":[effect],
                    }))
                    .unwrap(),
                ),
            }
            assert!(
                crate::attacks::NaturalAttack::new(
                    ordinary.skill(),
                    ordinary.bonus(),
                    ordinary.wind_up(),
                    ordinary.recovery(),
                    strikes
                )
                .is_err(),
                "selected natural sources have no selected step or spatial reach authority"
            );
        }
    }
}

#[test]
fn species_source_cannot_import_spatial_natural_attacks_into_selected_melee() {
    let facts = profile(CreatureSize::Medium, false);
    let content = push_content(facts, facts, true);
    for reach in [json!({"kind":"melee"}), json!({"kind":"sight","range":6})] {
        let mut document = serde_json::to_value(content.rules().document()).unwrap();
        let ability = document["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["id"] == "push_probe")
            .unwrap();
        ability["strikes"] = json!({"kind":"selected_melee"});
        ability["reach"] = reach;
        let rules =
            crate::rules::Ruleset::validate(serde_json::from_value(document).unwrap()).unwrap();
        assert!(crate::creatures::Content::new(rules.clone(), content.catalog().clone()).is_ok());
        for effect in spatial_effects() {
            let mut catalog = content.catalog().clone();
            let species_id = crate::content_ids::SpeciesId::new("pushing_source").unwrap();
            let original = catalog.species[&species_id].melee.clone();
            let mut strikes = original.strikes().to_vec();
            strikes[0].on_success.insert(0, effect);
            let invalid = crate::attacks::NaturalAttack::new(
                original.skill(),
                original.bonus(),
                original.wind_up(),
                original.recovery(),
                strikes,
            )
            .ok()
            .and_then(|attack| {
                catalog.species.get_mut(&species_id).unwrap().melee = attack;
                crate::creatures::Content::new(rules.clone(), catalog).ok()
            });
            assert!(
                invalid.is_none(),
                "invalid natural source cannot reach any live ability"
            );
        }
    }
}

#[test]
fn authored_push_cannot_replace_a_non_melee_declared_reach() {
    let facts = profile(CreatureSize::Medium, false);
    let content = push_content(facts, facts, true);
    for reach in [
        json!({"kind":"self_only"}),
        json!({"kind":"sight","range":6}),
        json!({"kind":"self_or_ally","range":{"kind":"touch"}}),
    ] {
        let mut document = serde_json::to_value(content.rules().document()).unwrap();
        let ability = document["abilities"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|ability| ability["id"] == "push_probe")
            .unwrap();
        ability["reach"] = reach;
        assert!(
            crate::rules::Ruleset::validate(serde_json::from_value(document).unwrap()).is_err()
        );
    }
}

#[test]
fn environment_spatial_effects_reject_without_payment_rng_or_partial_effects() {
    let mut game = guarded_step_game(100);
    for effect in spatial_effects() {
        let before = game.clone();
        assert!(game
            .apply_effects(
                crate::ActorId(1),
                &[
                    crate::rules::Effect::ApplyCondition {
                        condition: crate::content_ids::ConditionId::new("braced").unwrap(),
                        duration: 200
                    },
                    effect,
                ]
            )
            .is_err());
        assert_eq!(game, before);
    }
    assert!(game
        .apply_effects(
            crate::ActorId(1),
            &[crate::rules::Effect::ApplyCondition {
                condition: crate::content_ids::ConditionId::new("braced").unwrap(),
                duration: 200,
            }]
        )
        .unwrap());
}
