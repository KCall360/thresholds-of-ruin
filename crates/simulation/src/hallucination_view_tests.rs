//! Native presentation witnesses. Actual production paid/source-free callers are
//! additionally tested by the server's imported-catalog acceptance suite.
use super::*;
use crate::appearance_catalog::{ObjectPresentationKind, PresentationCatalog};
use crate::explored_map::{MapAppearance, MapCellInput, MapItemAppearance};
use crate::hallucination_clock::integration_tests::{dose, finish, fixture, potion, start};
use crate::perception::contract_fixtures::{at, OWNER, SUBJECT};
use std::sync::Arc;

fn drink(game: &mut Game) {
    let item = potion(game, "fixture.hallucination_view", vec![dose()]);
    let due = start(game, item);
    finish(game, due);
}
fn blank_map() -> MapAppearance {
    MapAppearance {
        object_presentation: None,
        wall: false,
        material: "".into(),
        asset: None,
        place_hint: false,
        stairs_up: false,
        stairs_down: false,
        door: None,
        trap: None,
        ground_items: vec![],
    }
}
fn restore(game: &Game) -> Game {
    let mut shared = crate::checkpoint::SharedState::default();
    let saved = game.checkpoint(&mut shared);
    let (saved, shared) =
        serde_json::from_slice(&serde_json::to_vec(&(saved, shared)).unwrap()).unwrap();
    Game::restore_checkpoint(saved, &shared, game.content().clone()).unwrap()
}
#[test]
fn paid_visual_projection_sanitizes_siblings_preserves_ids_and_is_query_pure() {
    let mut game = fixture();
    let location = game.actors[&SUBJECT].location;
    let mut spec = crate::ItemSpec::ordinary("genuine secret sword".into());
    spec.asset = Some("sword".into());
    spec.class = crate::ItemClass::Weapon;
    let item = game.place_item_stack(71, location, None, 1, spec).unwrap();
    let before = game.observe(OWNER).unwrap();
    assert!(before
        .visible_actors
        .iter()
        .any(|a| a.id == SUBJECT && !a.name.is_empty()));
    assert!(before
        .ground_items
        .iter()
        .any(|i| i.id == item && i.name == "genuine secret sword"));
    let (_, sight) = game.scene_snapshot(OWNER).unwrap();
    assert!(game.actor_visible_in_sight(OWNER, SUBJECT, &sight));
    drink(&mut game);
    let view = game.observe(OWNER).unwrap();
    let object = view.ground_items.iter().find(|i| i.id == item).unwrap();
    assert_eq!(
        (object.class, object.quantity, object.location),
        (crate::ItemClass::Weapon, 1, location)
    );
    assert_eq!(
        (
            &*object.name,
            &*object.appearance,
            &*object.description,
            object.asset.as_ref(),
            object.corpse.as_ref()
        ),
        ("object", "object", "", None, None)
    );
    let expected = game.content.presentation_catalog().object(AppearanceKey {
        seed: 42,
        observer: OWNER.0,
        subject: item.0,
        channel: AppearanceChannel::Item,
        phase: 0,
    });
    assert_eq!(object.presentation.as_ref(), Some(&expected));
    let creature = view
        .visible_actors
        .iter()
        .find(|a| a.id == SUBJECT)
        .unwrap();
    let expected_creature = CreaturePresentation {
        name: Arc::from(""),
        asset: None,
        seed: 0,
    };
    assert_eq!(creature.presentation.as_ref(), Some(&expected_creature));
    assert!(creature.name.is_empty() && creature.asset.is_none());
    let stable = game.clone();
    for _ in 0..4 {
        assert_eq!(game.observe(OWNER).unwrap(), view);
    }
    assert_eq!(game, stable);
    assert_eq!(restore(&game), game);
}
#[test]
fn blind_awareness_is_factual_and_cure_does_not_reveal_visual_scene() {
    let mut game = fixture();
    drink(&mut game);
    game.apply_condition(
        OWNER,
        &crate::content_ids::ConditionId::new("fixture_blind").unwrap(),
        SUBJECT,
        1000,
    )
    .unwrap();
    let (_, sight) = game.scene_snapshot(OWNER).unwrap();
    assert!(!game.actor_visible_in_sight(OWNER, SUBJECT, &sight));
    let view = game.observe(OWNER).unwrap();
    assert!(view.visible_actors.iter().all(|a| a.presentation.is_none()));
    assert!(view.ground_items.iter().all(|i| i.presentation.is_none()));
    assert!(view
        .visible_cells
        .iter()
        .all(|c| c.object_presentation.is_none()));
    let before_cells = view.visible_cells.clone();
    game.apply_effects(
        OWNER,
        &[crate::rules::Effect::RemoveCondition {
            condition: crate::content_ids::ConditionId::new("fixture_hallucination").unwrap(),
        }],
    )
    .unwrap();
    let cured = game.observe(OWNER).unwrap();
    assert_eq!(cured.visible_cells, before_cells);
}
#[test]
fn genuine_stair_memory_survives_and_false_stair_sample_never_creates_recognition() {
    let mut game = fixture();
    let stair = at(1, 2, 1);
    let destination = at(2, 2, 1);
    game.register_named_anchors(
        destination.region,
        [("landing".into(), destination.position)].into(),
    )
    .unwrap();
    game.connect_named_stair(
        stair,
        Direction::Up,
        tor_world::NamedAnchor {
            region: destination.region,
            name: "landing".into(),
        },
    )
    .unwrap();
    let factual = game.observe(OWNER).unwrap();
    assert!(factual
        .visible_cells
        .iter()
        .find(|cell| cell.location == stair)
        .expect("visible factual stair before dosing")
        .object_presentation
        .is_none());
    let expected_factual = (*factual.visible_cells).clone();
    let mut known = game.clone();
    let mut appearance = blank_map();
    appearance.stairs_up = true;
    known
        .record_explored_observation(
            OWNER,
            &[MapCellInput {
                location: stair,
                appearance,
            }],
            &[],
        )
        .unwrap();
    drink(&mut game);
    drink(&mut known);
    let before_projection = crate::diagnostics::work_counts().observation_cells_copied;
    let current = game.observe(OWNER).unwrap();
    let copied = crate::diagnostics::work_counts().observation_cells_copied - before_projection;
    let unknown_art = current
        .visible_cells
        .iter()
        .find(|c| c.location == stair)
        .expect("actual visible stair")
        .object_presentation
        .clone();
    assert!(unknown_art.is_some());
    assert_eq!(
        copied,
        current.visible_cells.len(),
        "hallucinated stair projection detaches the factual cell array exactly once"
    );
    assert!(!current.visible_cells.shares_storage(&factual.visible_cells));
    assert_eq!(*factual.visible_cells, expected_factual);
    let known_view = known.observe(OWNER).unwrap();
    assert!(known_view
        .visible_cells
        .iter()
        .find(|c| c.location == stair)
        .unwrap()
        .object_presentation
        .is_none());
    assert_eq!(current.exits, known_view.exits);
    assert_eq!(
        current
            .visible_cells
            .iter()
            .find(|cell| cell.location == stair)
            .unwrap()
            .object_presentation,
        unknown_art,
        "the known branch's projection cannot rewrite retained unknown artwork"
    );
    assert_eq!(*factual.visible_cells, expected_factual);
    let fake_stairs = ObjectPresentation {
        name: Arc::from(""),
        asset: None,
        kind: ObjectPresentationKind::Stairs,
    };
    let mut appearance = blank_map();
    appearance.stairs_up = true;
    appearance.object_presentation = Some(fake_stairs);
    game.record_explored_observation(
        OWNER,
        &[MapCellInput {
            location: stair,
            appearance,
        }],
        &[],
    )
    .unwrap();
    assert!(
        !game.genuinely_known_stair(OWNER, stair),
        "cosmetic Stair plus factual exit must not bootstrap recognition"
    );
    assert_eq!(
        game.observe(OWNER)
            .unwrap()
            .visible_cells
            .iter()
            .find(|c| c.location == stair)
            .unwrap()
            .object_presentation,
        unknown_art
    );
    assert_eq!(restore(&game), game);
}
#[test]
fn cosmetic_memory_is_exact_and_strict_not_reselected_on_restore() {
    let mut game = fixture();
    let sample = ObjectPresentation {
        name: Arc::from(""),
        asset: None,
        kind: ObjectPresentationKind::Stairs,
    };
    let mut appearance = blank_map();
    appearance.ground_items.push(MapItemAppearance {
        presentation: Some(sample.clone()),
        class: crate::ItemClass::Weapon,
        description: "".into(),
        name: "object".into(),
        appearance: "object".into(),
        identified: false,
        quantity: 1,
        asset: None,
    });
    let location = game.actors[&OWNER].location;
    game.record_explored_observation(
        OWNER,
        &[MapCellInput {
            location,
            appearance: appearance.clone(),
        }],
        &[],
    )
    .unwrap();
    assert_eq!(restore(&game), game);
    let encoded = serde_json::to_value(&appearance).unwrap();
    for field in ["name", "appearance", "description"] {
        let mut invalid = encoded.clone();
        invalid["ground_items"][0][field] = "leaked original".into();
        assert!(
            serde_json::from_value::<MapAppearance>(invalid).is_err(),
            "{field}"
        );
    }
    let mut invalid = encoded.clone();
    invalid["ground_items"][0]
        .as_object_mut()
        .unwrap()
        .remove("presentation");
    assert!(serde_json::from_value::<MapAppearance>(invalid).is_err());
    let empty = PresentationCatalog::new(vec![], vec![]).unwrap();
    assert!(appearance.valid_for_catalog(&empty));
    appearance.ground_items[0]
        .presentation
        .as_mut()
        .unwrap()
        .name = Arc::from("not a canonical sample");
    assert!(!appearance.valid_for_catalog(&empty));
}

pub(crate) fn named_fixture(subject_values: &[crate::perception::PerceptionValue]) -> Game {
    named_fixture_with_awareness(subject_values, false)
}
fn named_fixture_with_awareness(
    subject_values: &[crate::perception::PerceptionValue],
    awareness: bool,
) -> Game {
    let source = fixture();
    let marker = source
        .content()
        .rules()
        .document()
        .conditions
        .iter()
        .find(|c| c.hallucinating)
        .unwrap()
        .clone();
    let base = crate::perception::contract_fixtures::fixture_with(
        &[],
        subject_values,
        |document, owner, _| {
            document.conditions.push(marker);
            if awareness {
                let mut values = owner.default_attributes.values();
                values.awareness = 5;
                owner.default_attributes = crate::attributes::Attributes::new(values).unwrap();
            }
        },
    );
    let palette = PresentationCatalog::new(
        ["cosmetic a", "cosmetic b", "cosmetic c"]
            .into_iter()
            .map(|name| crate::appearance_catalog::CreatureAppearance {
                name: Arc::from(name),
                asset: None,
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let content = crate::creatures::Content::with_presentation_catalog(base.content(), palette);
    let mut game = Game::new((*base.world).clone(), 42, content);
    assert_eq!(
        crate::perception::contract_fixtures::spawn(
            &mut game,
            "fixture_perception_owner",
            at(1, 2, 2)
        ),
        OWNER
    );
    assert_eq!(
        crate::perception::contract_fixtures::spawn(
            &mut game,
            "fixture_perception_subject",
            at(1, 3, 2)
        ),
        SUBJECT
    );
    game
}
#[test]
fn observer_key_has_literal_distinct_result_without_subject_seed_leak() {
    let mut game = named_fixture(&[]);
    let other = crate::perception::contract_fixtures::spawn(
        &mut game,
        "fixture_perception_owner",
        at(1, 3, 3),
    );
    assert_eq!(other.0, 3);
    for observer in [OWNER, other] {
        game.apply_condition(
            observer,
            &crate::hallucination_clock::integration_tests::condition(),
            observer,
            1000,
        )
        .unwrap();
    }
    // Independent published recipe: seed42, subject2, Creature, phase0 gives
    // index2 for observer1 and index0 for observer3 in the sorted three rows.
    for (observer, name) in [(OWNER, "cosmetic c"), (other, "cosmetic a")] {
        let view = game.observe(observer).unwrap();
        let subject = view
            .visible_actors
            .iter()
            .find(|a| a.id == SUBJECT)
            .unwrap();
        assert_eq!(subject.name, name);
        assert_eq!(subject.presentation.as_ref().unwrap().seed, 0);
        assert!(subject.asset.is_none());
    }
    assert_eq!(restore(&game), game);
}
#[test]
fn intact_visual_guise_substitutes_as_object_but_invisible_awareness_does_not() {
    // Keep this witness about an intact guise, not a lucky passive discovery
    // contest triggered by completing the drink.
    let mut guise = crate::perception::contract_fixtures::guise();
    if let crate::perception::PerceptionValue::ObjectGuise { edge, .. } = &mut guise {
        *edge = 1000;
    }
    let mut guised = named_fixture(&[guise]);
    let before = guised.observe(OWNER).unwrap();
    assert!(before
        .apparent_objects
        .iter()
        .any(|o| o.subject == SUBJECT && o.appearance.name == "weathered chest"));
    drink(&mut guised);
    let view = guised.observe(OWNER).unwrap();
    let object = view
        .apparent_objects
        .iter()
        .find(|o| o.subject == SUBJECT)
        .unwrap();
    assert!(object.presentation.is_some());
    assert_eq!(object.appearance.name, "object");
    assert!(object.appearance.description.is_empty() && object.appearance.asset.is_none());
    assert!(!view.visible_actors.iter().any(|a| a.id == SUBJECT));
    let mut invisible = named_fixture_with_awareness(
        &[crate::perception::PerceptionValue::Invisible { enabled: true }],
        true,
    );
    let (_, sight) = invisible.scene_snapshot(OWNER).unwrap();
    assert!(!invisible.actor_visible_in_sight(OWNER, SUBJECT, &sight));
    let factual = invisible
        .observe(OWNER)
        .unwrap()
        .visible_actors
        .into_iter()
        .find(|a| a.id == SUBJECT)
        .expect("lawful nonvisual contact prerequisite");
    assert!(factual.presentation.is_none());
    drink(&mut invisible);
    let view = invisible.observe(OWNER).unwrap();
    let contact = view
        .visible_actors
        .iter()
        .find(|a| a.id == SUBJECT)
        .expect("same factual contact remains disclosed");
    assert_eq!(contact.disclosure, factual.disclosure);
    assert_eq!(contact.name, factual.name);
    assert!(contact.presentation.is_none());
    assert!(view
        .visible_actors
        .iter()
        .filter(|a| a.id == SUBJECT)
        .all(|a| a.presentation.is_none()));
    assert!(view
        .apparent_objects
        .iter()
        .filter(|o| o.subject == SUBJECT)
        .all(|o| o.presentation.is_none()));
}

#[test]
fn a_hallucinated_figure_shows_none_of_its_true_gear() {
    use crate::interactions::{AnatomySpec, EquipmentSlot, EquipmentSlotId};
    let mut game = fixture();
    let location = game.actors[&SUBJECT].location;
    let mut spec = crate::ItemSpec::ordinary("plate mail".into());
    spec.class = crate::ItemClass::Armor;
    spec.asset = Some("item.armor.plate".into());
    let mail = game
        .place_item_stack(72, location, Some(SUBJECT), 1, spec)
        .unwrap();
    {
        let mut wearer = game.actors.get_mut(&SUBJECT).unwrap();
        wearer.anatomy = tor_world::Shared::new(AnatomySpec {
            slots: vec![EquipmentSlot::new("body_armor").unwrap()],
            ..AnatomySpec::default()
        });
        wearer.equipment.insert(EquipmentSlotId(0), mail);
    }
    let seen = |view: &Observation| {
        view.visible_actors
            .iter()
            .find(|a| a.id == SUBJECT)
            .unwrap()
            .worn
            .len()
    };
    assert_eq!(
        seen(&game.observe(OWNER).unwrap()),
        1,
        "a clear eye sees the armour"
    );
    drink(&mut game);
    assert_eq!(
        seen(&game.observe(OWNER).unwrap()),
        0,
        "the hallucination hides it"
    );
}

#[test]
fn boundary_effects_include_observer_stair_memory_without_actor_mutation() {
    let mut before = fixture();
    let stair = at(1, 2, 1);
    let destination = at(2, 2, 1);
    before
        .register_named_anchors(
            destination.region,
            [("landing".into(), destination.position)].into(),
        )
        .unwrap();
    before
        .connect_named_stair(
            stair,
            Direction::Up,
            tor_world::NamedAnchor {
                region: destination.region,
                name: "landing".into(),
            },
        )
        .unwrap();
    drink(&mut before);
    let original = before.observe(OWNER).unwrap();
    assert!(
        original
            .visible_cells
            .iter()
            .find(|c| c.location == stair)
            .unwrap()
            .object_presentation
            .is_some(),
        "positive unknown hallucinated stair"
    );
    let mut after = before.clone();
    let mut appearance = blank_map();
    appearance.stairs_up = true;
    after
        .record_explored_observation(
            OWNER,
            &[MapCellInput {
                location: stair,
                appearance,
            }],
            &[],
        )
        .unwrap();
    assert!(
        before.actors.shares_actor_payload(&after.actors, OWNER),
        "map learning does not detach the actor"
    );
    let learned = after.observe(OWNER).unwrap();
    assert!(learned
        .visible_cells
        .iter()
        .find(|c| c.location == stair)
        .unwrap()
        .object_presentation
        .is_none());
    assert_ne!(original, learned);
    let effects = after.boundary_effects_since(&before);
    assert_eq!(effects.changed_actors().count(), 0);
    assert_eq!(
        effects.observation_change(OWNER).unwrap(),
        crate::ObservationChange::NeedsProjection
    );
}

#[test]
fn map_projection_matches_full_appearances_without_rich_materialization() {
    for guise in [false, true] {
        let values = if guise {
            vec![crate::perception::contract_fixtures::guise()]
        } else {
            vec![]
        };
        let mut game = named_fixture(&values);
        let stair = at(1, 2, 1);
        let destination = at(2, 2, 1);
        game.register_named_anchors(
            destination.region,
            [("map-landing".into(), destination.position)].into(),
        )
        .unwrap();
        game.connect_named_stair(
            stair,
            Direction::Up,
            tor_world::NamedAnchor {
                region: destination.region,
                name: "map-landing".into(),
            },
        )
        .unwrap();
        let mut spec = crate::ItemSpec::ordinary("genuine map object".into());
        spec.concealed = true;
        spec.appearance = "unidentified map object".into();
        spec.asset = Some("map_object".into());
        game.place_item_stack(71, game.actors[&SUBJECT].location, None, 1, spec)
            .unwrap();
        for phase in 0..3 {
            if phase == 1 {
                drink(&mut game);
            }
            if phase == 2 {
                let mut appearance = blank_map();
                appearance.stairs_up = true;
                game.record_explored_observation(
                    OWNER,
                    &[MapCellInput {
                        location: stair,
                        appearance,
                    }],
                    &[],
                )
                .unwrap();
            }
            let before = game.clone();
            let (full, scene, _) = game.observe_event_shared_scene(OWNER).unwrap();
            let counts = crate::diagnostics::work_counts();
            let (map, map_scene) = game.observe_map_shared_scene(OWNER).unwrap();
            let after = crate::diagnostics::work_counts();
            assert_eq!(
                map.visible_cells, full.visible_cells,
                "guise={guise}, phase={phase}"
            );
            assert_eq!(map.exits, full.exits);
            assert_eq!(map.ground_items, full.ground_items);
            assert_eq!(map_scene, scene);
            assert_eq!(
                after.observations, counts.observations,
                "map acquisition must not construct a discarded full Observation"
            );
            assert_eq!(
                after.own_stats_materializations,
                counts.own_stats_materializations
            );
            assert_eq!(game, before, "appearance queries remain pure");
        }
    }
}

#[test]
fn map_projection_preserves_invalid_observer_and_anatomy_errors() {
    let mut game = named_fixture(&[]);
    let missing = crate::ActorId(u64::MAX);
    assert_eq!(
        game.observe_map_shared_scene(missing).unwrap_err(),
        game.observe_event_shared_scene(missing).unwrap_err()
    );
    game.actors
        .get_mut(&OWNER)
        .unwrap()
        .anatomy
        .slots
        .push(crate::content_ids::EquipmentSlot::new("missing_map_fixture_slot").unwrap());
    assert_eq!(
        game.observe_map_shared_scene(OWNER).unwrap_err(),
        game.observe_event_shared_scene(OWNER).unwrap_err()
    );
}
