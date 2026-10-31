//! Adapter-local operation counts, not gameplay timings or entity placement tests.
use super::*;
use std::collections::BTreeSet;

fn at(x: i32) -> w::Location {
    w::Location {
        region: w::RegionId(1),
        position: w::Position { x, y: 0, z: 0 },
    }
}

fn fixture(portal: bool, rows: usize) -> (s::Observation, Vec<w::SightCell>) {
    let mut world = w::World::new(vec![], vec![]).unwrap();
    world
        .add_region(w::Region {
            id: w::RegionId(1),
            name: "count corridor".into(),
            bounds: w::Extent::new(4, 1, 1).unwrap(),
        })
        .unwrap();
    if portal {
        for (from, direction, to) in [
            (at(3), w::Direction::East, at(0)),
            (at(0), w::Direction::West, at(3)),
        ] {
            world
                .connect(
                    w::Passage {
                        from,
                        direction,
                        to,
                    },
                    0,
                )
                .unwrap();
        }
    }
    let mut game = s::Game::new(world, 42, crate::engine::diagnostic_content().unwrap());
    let observer = game
        .spawn_actor(at(0), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    let (mut view, scene) = game.observe_scene(observer).unwrap();
    view.ground_items.clear();
    view.apparent_objects.clear();
    view.visible_actors.clear();
    // Interleaved locations and descending identities forbid an index from
    // sorting entity rows by identity or deduplicating projected occurrences.
    for index in 0..rows {
        let location = at((index % 4) as i32);
        let id = 10_000 - index as u64;
        view.ground_items.push(s::GroundItemView {
            presentation: Some(s::appearance_catalog::ObjectPresentation {
                name: format!("shown-item-{index}").into(),
                asset: None,
                kind: s::appearance_catalog::ObjectPresentationKind::Item {
                    class: s::ItemClass::Misc,
                },
            }),
            corpse: None,
            class: s::ItemClass::Misc,
            description: "private item description".into(),
            id: s::ItemId(id),
            name: format!("private-item-{index}"),
            quantity: 2,
            appearance: "private item appearance".into(),
            identified: false,
            known_supply: None,
            known_currency: None,
            supply_reachable: false,
            location,
            asset: Some("private.item.asset".into()),
        });
        view.apparent_objects
            .push(s::perception::ApparentObjectView {
                presentation: Some(s::appearance_catalog::ObjectPresentation {
                    name: format!("shown-object-{index}").into(),
                    asset: None,
                    kind: s::appearance_catalog::ObjectPresentationKind::Stairs,
                }),
                subject: s::ActorId(id + 20_000),
                location,
                appearance: s::perception::GuiseAppearance {
                    name: "private guise name".into(),
                    description: "private guise description".into(),
                    class: s::ItemClass::Misc,
                    asset: None,
                },
            });
        view.visible_actors.push(s::ActorView {
            presentation: Some(s::appearance_catalog::CreaturePresentation {
                name: format!("shown-actor-{index}").into(),
                asset: None,
                seed: index as u64,
            }),
            disclosure: s::perception::ActorDisclosure::Creature,
            id: s::ActorId(id + 40_000),
            location,
            name: "private actor name".into(),
            asset: Some("private.actor.asset".into()),
            worn: Vec::new(),
        });
    }
    (view, scene)
}

fn measured(
    name: &str,
    view: &s::Observation,
    scene: &[w::SightCell],
) -> (p::OutcomeObservation, OutcomeWork) {
    let scope = crate::wire_adapter::TargetScope::new(uuid::Uuid::from_u128(7), view.actor);
    OUTCOME_WORK.with(|counter| counter.set(OutcomeWork::default()));
    let compact = outcome_observation(view, scene, "count-salt", true, &scope);
    let work = OUTCOME_WORK.with(std::cell::Cell::get);
    let full = observation(
        view.clone(),
        scene,
        "count-salt",
        true,
        &|_| [None, None, None],
        &scope,
    );
    assert_eq!(compact, p::OutcomeObservation::from(&full));
    // The compact inputs deliberately are not a wire schema. Compare their
    // complete typed value, then the actual serializable facts they produce.
    let stamp = p::EventProvenance {
        batch: 1,
        through: 0,
    };
    let event = p::Event::Moved {
        direction: p::Direction::East,
    };
    let mut before = compact.clone();
    before.visible_actors.clear();
    let mut full_before = full.clone();
    full_before.visible_actors.clear();
    let facts =
        p::OutcomeFacts::capture_compact_causal(&before, &compact, Some(&event), stamp, stamp)
            .unwrap();
    assert!(matches!(
        facts.facts.first(),
        Some(p::OutcomeFact::OwnAction {
            underfoot: Some(_),
            ..
        })
    ));
    if compact
        .visible_actors
        .iter()
        .any(|actor| actor.id != compact.self_target)
    {
        assert!(facts
            .facts
            .iter()
            .any(|fact| matches!(fact, p::OutcomeFact::Noticed { .. })));
    }
    let encoded = serde_json::to_vec(&facts).unwrap();
    assert_eq!(
        encoded,
        serde_json::to_vec(
            &p::OutcomeFacts::capture_causal(&full_before, &full, Some(&event), stamp, stamp)
                .unwrap()
        )
        .unwrap()
    );
    let text = format!("{compact:?}");
    assert!(
        !text.contains("private"),
        "undisclosed body/guise facts in {name}"
    );
    eprintln!("OUTCOME_WORK {name}: {work:?}; bytes={}", encoded.len());
    (compact, work)
}

#[test]
fn count_sparse_dense_and_portal_occurrences_without_changing_disclosure() {
    for (name, portal, rows) in [
        ("empty", false, 0),
        ("sparse", false, 1),
        ("dense", false, 64),
        ("portal-dense", true, 64),
    ] {
        let (view, scene) = fixture(portal, rows);
        let locations: BTreeSet<_> = scene.iter().map(|cell| cell.location).collect();
        assert_eq!(locations.len(), 4);
        assert_eq!(
            scene.len() > locations.len(),
            portal,
            "genuine portal aliases"
        );
        let (compact, work) = measured(name, &view, &scene);
        assert_eq!(work.disclosed_cells, scene.len());
        assert_eq!(work.item_rows, rows);
        assert_eq!(work.object_rows, rows);
        assert_eq!(work.actor_rows, rows);
        assert_eq!(work.location_lookups, scene.len() * 3);
        let emitted = scene
            .iter()
            .map(|cell| {
                (0..rows)
                    .filter(|index| at((*index % 4) as i32) == cell.location)
                    .count()
            })
            .sum::<usize>();
        assert_eq!(work.emitted_rows, emitted * 3);
        let expected: Vec<_> = scene
            .iter()
            .flat_map(|cell| {
                (0..rows)
                    .filter(move |index| at((*index % 4) as i32) == cell.location)
                    .map(|index| format!("shown-item-{index}"))
                    .chain(
                        (0..rows)
                            .filter(move |index| at((*index % 4) as i32) == cell.location)
                            .map(|index| format!("shown-object-{index}")),
                    )
            })
            .collect();
        assert_eq!(
            compact
                .ground_items
                .iter()
                .map(|row| row.name.text.clone())
                .collect::<Vec<_>>(),
            expected
        );
        let underfoot: Vec<_> = (0..rows)
            .filter(|index| index % 4 == 0)
            .map(|index| format!("shown-item-{index}"))
            .chain(
                (0..rows)
                    .filter(|index| index % 4 == 0)
                    .map(|index| format!("shown-object-{index}")),
            )
            .collect();
        assert_eq!(
            compact
                .underfoot
                .items
                .iter()
                .map(|row| row.name.clone())
                .collect::<Vec<_>>(),
            underfoot
        );
    }
}

#[test]
fn occurrence_order_and_disclosure_gate_survive_reordered_and_hidden_rows() {
    let (mut view, mut scene) = fixture(true, 8);
    scene.reverse();
    // A row not disclosed in the current frame must remain absent even if
    // there are remembered entity rows at that location.
    view.visible_cells.retain(|cell| cell.location != at(2));
    let (compact, work) = measured("filtered-reversed", &view, &scene);
    let disclosed = scene.iter().filter(|cell| cell.location != at(2)).count();
    assert_eq!(work.disclosed_cells, disclosed);
    assert_eq!(work.item_rows, 8);
    assert_eq!(work.object_rows, 8);
    assert_eq!(work.actor_rows, 8);
    assert_eq!(work.location_lookups, disclosed * 3);
    assert!(compact.ground_items.iter().all(|row| ![
        "shown-item-2",
        "shown-item-6",
        "shown-object-2",
        "shown-object-6"
    ]
    .contains(&row.name.text.as_str())));
    assert!(compact
        .visible_actors
        .iter()
        .all(|row| !["shown-actor-2", "shown-actor-6"].contains(&row.name.as_str())));
    let mut blank = view.clone();
    blank.visible_cells.clear();
    let (compact, work) = measured("no-disclosed-occurrences", &blank, &scene);
    assert_eq!(work, OutcomeWork::default());
    assert!(
        compact.visible_cells.is_empty()
            && compact.visible_actors.is_empty()
            && compact.ground_items.is_empty()
    );
}

#[test]
fn same_location_other_frame_is_not_disclosed_and_multicell_identity_keeps_occurrences() {
    let (mut view, mut scene) = fixture(true, 8);
    let mut other_cell = view.visible_actors[0].clone();
    other_cell.location = at(1);
    view.visible_actors.push(other_cell);
    let ordinary_occurrences = scene.len();
    // Adapter-local boundary mutation: a scene occurrence of the same location
    // in an undisclosed frame must not borrow the authorized frame's rows.
    let mut hidden = *scene.iter().find(|cell| cell.location == at(0)).unwrap();
    assert!(view
        .visible_cells
        .iter()
        .any(|cell| cell.location == hidden.location && cell.frame == hidden.rotation));
    hidden.rotation = (hidden.rotation + 1) % 4;
    hidden.offset = w::Position {
        x: 99,
        y: 98,
        z: 97,
    };
    assert!(!view
        .visible_cells
        .iter()
        .any(|cell| cell.location == hidden.location && cell.frame == hidden.rotation));
    scene.insert(0, hidden);
    let (compact, work) = measured("frame-gate-multicell", &view, &scene);
    assert_eq!(work.disclosed_cells, ordinary_occurrences);
    assert_eq!(work.item_rows, 8);
    assert_eq!(work.object_rows, 8);
    assert_eq!(work.actor_rows, 9);
    assert_eq!(work.location_lookups, ordinary_occurrences * 3);
    let scope = crate::wire_adapter::TargetScope::new(uuid::Uuid::from_u128(7), view.actor);
    let subject = scope.actor(view.visible_actors[0].id);
    let expected: Vec<_> = scene
        .iter()
        .skip(1)
        .filter(|cell| cell.location == at(0) || cell.location == at(1))
        .map(|cell| p::Position {
            x: cell.offset.x,
            y: cell.offset.y,
            z: cell.offset.z,
        })
        .collect();
    assert_eq!(
        compact
            .visible_actors
            .iter()
            .filter(|actor| actor.id == subject)
            .map(|actor| actor.position)
            .collect::<Vec<_>>(),
        expected
    );
    assert!(compact.visible_actors.iter().all(|actor| actor.position
        != p::Position {
            x: 99,
            y: 98,
            z: 97
        }));
}
