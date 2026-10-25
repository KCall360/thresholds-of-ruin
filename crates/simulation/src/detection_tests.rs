use super::*;
use crate::perception::contract_fixtures::{at, OWNER, SUBJECT};
use crate::spatial_scope::registry::{Budget, CellRun, PrimaryRun, Scope};
use crate::spatial_scope::{Membership, PassageId, PrimarySpace};
use std::num::NonZeroU64;
use tor_world::RegionId;
fn install_scopes(game: &mut Game) {
    let budget = Budget {
        scopes: 2,
        members: 2,
        rooms: 0,
        passages: 2,
        runs: 14,
        expanded_refs: 126,
        label_bytes: 128,
    };
    for region in [RegionId(1), RegionId(2)] {
        game.register_spatial_scope(
            Scope {
                id: format!("detection-floor-{}", region.0),
                members: vec![region],
                rooms: vec![],
                mapping: crate::spatial_scope::mapping::MappingCertification::Uncertified,
                primary: (0..7)
                    .map(|y| PrimaryRun {
                        cells: CellRun {
                            region,
                            z: 0,
                            y,
                            first_x: 0,
                            last_x: 8,
                        },
                        membership: Membership::new(
                            PrimarySpace::Passage(PassageId::new(1).unwrap()),
                            None,
                        )
                        .unwrap(),
                    })
                    .collect(),
            },
            &budget,
        )
        .unwrap();
    }
}
fn fixture() -> Game {
    let mut game = crate::detection_lifecycle::tests::fixture();
    install_scopes(&mut game);
    game
}
fn prepared(game: &Game, kind: Kind) -> PreparedDetection {
    game.prepare_detection_chart(OWNER, kind, CensusBudget::canonical())
        .unwrap()
}
fn count(game: &Game, kind: Kind) -> usize {
    prepared(game, kind).candidate.chart().unwrap().rows().len()
}
fn place(
    game: &mut Game,
    id: u64,
    at: Location,
    carrier: Option<ActorId>,
    class: ItemClass,
    magical: bool,
) -> crate::ItemId {
    let mut spec = crate::ItemSpec::ordinary("private item name".into());
    spec.class = class;
    spec.magical = magical;
    game.place_item_stack(id, at, carrier, 1, spec).unwrap()
}
#[test]
fn monster_census_includes_friends_but_not_self_dead_or_noncreature_bodies() {
    let mut game = fixture();
    assert_eq!(count(&game, Kind::Monsters), 1);
    let view = prepared(&game, Kind::Monsters);
    let row = &view.candidate.chart().unwrap().rows()[0];
    assert!(
        matches!(&row.mark,Mark::Monster(label) if label.name.is_empty() && label.asset.is_none())
    );
    assert_eq!(row.position, Some([3, 2, 0]));
    game.spawn_actor(at(1, 6, 2), NonZeroU64::new(100).unwrap())
        .unwrap();
    assert_eq!(count(&game, Kind::Monsters), 1);
    game.apply_damage(
        SUBJECT,
        &crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                crate::combat::DamageType::Impact,
                None,
                10000,
            )],
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!game.alive(SUBJECT));
    assert_eq!(count(&game, Kind::Monsters), 0);
}
#[test]
fn canonical_floor_follows_owner_instead_of_remembering_casting_region() {
    let mut game = fixture();
    assert_eq!(count(&game, Kind::Monsters), 1);
    assert!(game
        .relocate_exact_in_transaction(
            OWNER,
            crate::abilities::Placement {
                location: at(2, 2, 2),
                orientation: 0
            },
            false
        )
        .unwrap());
    assert_eq!(count(&game, Kind::Monsters), 0);
    crate::perception::contract_fixtures::spawn(
        &mut game,
        "fixture_perception_subject",
        at(2, 4, 2),
    );
    assert_eq!(count(&game, Kind::Monsters), 1);
}
#[test]
fn food_chart_is_true_ground_food_presence_not_nutrition_or_carried_stock() {
    let mut game = fixture();
    place(&mut game, 30, at(1, 4, 2), None, ItemClass::Food, false);
    place(&mut game, 31, at(1, 4, 2), None, ItemClass::Food, false);
    place(
        &mut game,
        32,
        at(1, 3, 2),
        Some(SUBJECT),
        ItemClass::Food,
        false,
    );
    place(&mut game, 33, at(2, 4, 2), None, ItemClass::Food, false);
    let mut edible = crate::ItemSpec::ordinary("edible tool".into());
    edible.class = ItemClass::Tool;
    edible.edible = Some(crate::EdibleSpec {
        nutrition: crate::rules::AmountExpr::Fixed { amount: 5 },
    });
    game.place_item_stack(34, at(1, 5, 2), None, 1, edible)
        .unwrap();
    let result = prepared(&game, Kind::Food);
    assert_eq!(result.source_points, vec![at(1, 4, 2)]);
    assert_eq!(result.candidate.chart().unwrap().rows().len(), 1);
}
#[test]
fn magic_includes_other_carried_stock_without_any_ground_item_and_excludes_own_pack() {
    let mut game = fixture();
    place(
        &mut game,
        40,
        at(1, 2, 2),
        Some(OWNER),
        ItemClass::Tool,
        true,
    );
    assert_eq!(count(&game, Kind::Magic), 0);
    place(
        &mut game,
        41,
        at(1, 3, 2),
        Some(SUBJECT),
        ItemClass::Tool,
        true,
    );
    place(
        &mut game,
        42,
        at(1, 3, 2),
        Some(SUBJECT),
        ItemClass::Tool,
        true,
    );
    assert!(game
        .items
        .values()
        .all(|item| matches!(item.location, ItemLocation::Carried(_))));
    let result = prepared(&game, Kind::Magic);
    assert_eq!(result.source_points, vec![at(1, 3, 2)]);
    assert_eq!(result.candidate.chart().unwrap().rows().len(), 1);
}
#[test]
fn corrosion_does_not_become_detection_magic_or_redefine_existing_theft_predicate() {
    let mut game = fixture();
    let mut spec = crate::ItemSpec::ordinary("rusted armor".into());
    spec.class = ItemClass::Armor;
    spec.equipment = Some(crate::EquipmentSpec {
        slot: crate::EquipmentSlot::new("body_armor").unwrap(),
        periodic_relocation: None,
        armor: Some(crate::ArmorSpec {
            physical_reduction: 3,
            material: crate::ArmorMaterial::Metal,
            rustproof: false,
        }),
        attack: None,
        weapon_profile: None,
        modifiers: vec![],
        potency_modifiers: vec![],
    });
    let item = game
        .place_item_stack(43, at(1, 4, 2), None, 1, spec)
        .unwrap();
    game.items
        .edit(item, |item| {
            item.condition.corrosion = 1;
            item.condition.armor_penalty = 1;
        })
        .unwrap();
    assert!(game.items[&item].condition_valid());
    assert!(game.items[&item].magical());
    assert_eq!(count(&game, Kind::Magic), 0);
    game.items
        .edit(item, |item| item.condition.armor_enchantment = 1)
        .unwrap();
    assert!(game.items[&item].condition_valid());
    assert_eq!(count(&game, Kind::Magic), 1);
}
#[test]
fn repeated_census_is_pure_and_final_envelope_refusal_is_not_positive_prefix() {
    let game = fixture();
    let before = game.clone();
    let first = prepared(&game, Kind::Monsters);
    assert_eq!(first.candidate.chart().unwrap().rows().len(), 1);
    assert_eq!(prepared(&game, Kind::Monsters).candidate, first.candidate);
    let mut budget = CensusBudget::canonical();
    budget.pages.bytes = budget.pages.envelope_bytes + 1;
    let refused = game
        .prepare_detection_chart(OWNER, Kind::Monsters, budget)
        .unwrap();
    assert_eq!(
        refused.candidate.unavailable_reason(),
        Some(Unavailable::Budget)
    );
    assert!(refused.source_points.is_empty());
    assert_eq!(game, before);
}
#[test]
fn paid_nonempty_monster_reveal_identifies_but_empty_then_later_arrival_does_not() {
    let mut positive = fixture();
    let item = crate::detection_lifecycle::tests::potion(
        &mut positive,
        crate::ItemDiscovery::SelfEffect,
        crate::detection_lifecycle::tests::dose(),
    );
    let due = crate::detection_lifecycle::tests::start(&mut positive, item);
    crate::detection_lifecycle::tests::finish(&mut positive, due);
    assert!(positive.actors[&OWNER]
        .knowledge
        .contains("fixture.monster_chart"));
    assert_eq!(positive.items[&item].quantity, 2);
    let mut empty = fixture();
    empty
        .relocate_exact_in_transaction(
            SUBJECT,
            crate::abilities::Placement {
                location: at(2, 3, 2),
                orientation: 0,
            },
            false,
        )
        .unwrap();
    let item = crate::detection_lifecycle::tests::potion(
        &mut empty,
        crate::ItemDiscovery::SelfEffect,
        crate::detection_lifecycle::tests::dose(),
    );
    let due = crate::detection_lifecycle::tests::start(&mut empty, item);
    crate::detection_lifecycle::tests::finish(&mut empty, due);
    assert!(!empty.actors[&OWNER]
        .knowledge
        .contains("fixture.monster_chart"));
    assert_eq!(empty.items[&item].quantity, 2);
    empty
        .relocate_exact_in_transaction(
            SUBJECT,
            crate::abilities::Placement {
                location: at(1, 3, 2),
                orientation: 0,
            },
            false,
        )
        .unwrap();
    assert_eq!(
        empty
            .detection_chart(OWNER, Kind::Monsters)
            .unwrap()
            .candidate
            .unwrap()
            .chart()
            .unwrap()
            .rows()
            .len(),
        1
    );
    assert!(!empty.actors[&OWNER]
        .knowledge
        .contains("fixture.monster_chart"));
}
#[test]
fn empty_food_attempt_keeps_prior_positive_sample_historical_without_tracking_item() {
    let mut game = fixture();
    let item = place(&mut game, 50, at(1, 4, 2), None, ItemClass::Food, false);
    let completed = game.complete_detection_chart(OWNER, Kind::Food).unwrap();
    assert!(completed.admitted_nonempty());
    let prior = game.detection_chart(OWNER, Kind::Food).unwrap();
    assert!(prior.historical);
    game.items
        .edit(item, |item| item.location = ItemLocation::Carried(OWNER))
        .unwrap();
    let completed = game.complete_detection_chart(OWNER, Kind::Food).unwrap();
    assert!(!completed.admitted_nonempty());
    let current = game.detection_chart(OWNER, Kind::Food).unwrap();
    assert_eq!(current.candidate, prior.candidate);
    assert!(matches!(
        current.latest_attempt.unwrap().status,
        crate::chart_history::AttemptStatus::Empty
    ));
    assert_eq!(
        current.candidate.unwrap().chart().unwrap().rows()[0].position,
        Some([4, 2, 0])
    );
}

#[test]
fn monster_sense_bypasses_eyes_and_guise_but_uses_its_own_observer_cosmetic_channel() {
    for subject in [
        crate::perception::PerceptionValue::Invisible { enabled: true },
        crate::perception::contract_fixtures::guise(),
    ] {
        let mut game = crate::hallucination::tests::named_fixture(&[subject]);
        install_scopes(&mut game);
        let other = crate::perception::contract_fixtures::spawn(
            &mut game,
            "fixture_perception_owner",
            at(1, 3, 3),
        );
        let blind = crate::content_ids::ConditionId::new("fixture_blind").unwrap();
        for observer in [OWNER, other] {
            game.apply_condition(observer, &blind, observer, 1000)
                .unwrap();
            game.apply_condition(
                observer,
                &crate::hallucination_clock::integration_tests::condition(),
                observer,
                1000,
            )
            .unwrap();
            let (_, sight) = game.scene_snapshot(observer).unwrap();
            assert!(!game.actor_visible_in_sight(observer, SUBJECT, &sight));
        }
        let before = game.clone();
        // Independently evaluated unsigned recipe: MonsterSense seed42/subject2,
        // phase0, sorted [a,b,c] => observer1 index0, observer3 index1.
        for (observer, expected) in [(OWNER, "cosmetic a"), (other, "cosmetic b")] {
            let view = game
                .prepare_detection_chart(observer, Kind::Monsters, CensusBudget::canonical())
                .unwrap();
            let row = view
                .candidate
                .chart()
                .unwrap()
                .rows()
                .iter()
                .find(|row| row.position == Some([3, 2, 0]))
                .expect("real subject census");
            assert!(matches!(&row.mark, Mark::Monster(label)
                if label.name.as_ref() == expected && label.asset.is_none()));
        }
        assert_eq!(game, before, "cosmetic chart reads are pure");
        game.apply_effects(
            OWNER,
            &[crate::rules::Effect::RemoveCondition {
                condition: crate::hallucination_clock::integration_tests::condition(),
            }],
        )
        .unwrap();
        let view = prepared(&game, Kind::Monsters);
        let row = view
            .candidate
            .chart()
            .unwrap()
            .rows()
            .iter()
            .find(|row| row.position == Some([3, 2, 0]))
            .unwrap();
        assert!(matches!(&row.mark, Mark::Monster(label) if label.name.is_empty()));
        let (_, sight) = game.scene_snapshot(OWNER).unwrap();
        assert!(!game.actor_visible_in_sight(OWNER, SUBJECT, &sight));
    }
}

#[test]
fn paid_final_page_envelope_failure_consumes_dose_without_learning_or_later_retro_learning() {
    let mut game = fixture();
    assert_eq!(
        count(&game, Kind::Monsters),
        1,
        "lawful nonempty whole census prerequisite"
    );
    let item = crate::detection_lifecycle::tests::potion(
        &mut game,
        crate::ItemDiscovery::SelfEffect,
        crate::detection_lifecycle::tests::dose(),
    );
    let due = crate::detection_lifecycle::tests::start(&mut game, item);
    with_completion_page_bytes(chart::delivery::CHART_ENVELOPE_BYTES + 1, || {
        crate::detection_lifecycle::tests::finish(&mut game, due);
    });
    assert_eq!(game.tick(), due);
    assert_eq!(game.items[&item].quantity, 2);
    assert!(game.monster_detection_active(OWNER));
    assert!(!game.actors[&OWNER]
        .knowledge
        .contains("fixture.monster_chart"));
    // Removing the test-only bound now yields the positive current view, but
    // cannot retroactively change the final-only paid receipt or discovery.
    let view = game.detection_chart(OWNER, Kind::Monsters).unwrap();
    assert_eq!(view.candidate.unwrap().chart().unwrap().rows().len(), 1);
    assert!(!game.actors[&OWNER]
        .knowledge
        .contains("fixture.monster_chart"));
}
