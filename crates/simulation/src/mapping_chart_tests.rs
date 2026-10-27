use super::*;
use crate::chart::{Budget as ChartBudget, MappingMark};
use crate::perception::contract_fixtures::{OWNER, SUBJECT};
use crate::spatial_scope::mapping::tests::{
    at, box_cells, certify, chamber_world, frame, room, source_scope,
};
use crate::spatial_scope::mapping::MappingCertification;
use std::collections::BTreeSet;
use std::num::NonZeroU64;
use tor_world::{Passage, RegionId};
fn budget() -> CensusBudget {
    CensusBudget::canonical()
}
fn prepared(world: &World, scope: &Scope) -> PreparedMapping {
    let out = prepare_scope_mapping(world, scope, budget());
    assert!(
        out.candidate.chart().is_some(),
        "{:?}",
        out.candidate.unavailable_reason()
    );
    out
}
fn box_scope(world: &World) -> Scope {
    let mut scope = source_scope(world, &[(1, box_cells(1, [0, 0, 0], [1, 1, 1]))], &[], &[]);
    certify(world, &mut scope, &[frame(1, 1, Direction::Up)], &[]);
    scope
}
#[test]
fn certified_mapping_contains_only_wall_faces_not_room_interior_floor_ceiling_or_material() {
    let world = chamber_world(1, [2, 2, 2]);
    let scope = box_scope(&world);
    let out = prepared(&world, &scope);
    let chart = out.candidate.chart().unwrap();
    let actual: BTreeSet<_> = chart
        .rows()
        .iter()
        .map(|r| {
            let Mark::Mapping(MappingMark::WallFace { normal: Some(n) }) = r.mark else {
                panic!("wall-only room");
            };
            (r.position.unwrap(), n)
        })
        .collect();
    let mut expected = BTreeSet::new();
    for a in 0..=1 {
        for z in 0..=1 {
            expected.insert(([-1, a, z], [1, 0, 0]));
            expected.insert(([2, a, z], [-1, 0, 0]));
            expected.insert(([a, -1, z], [0, 1, 0]));
            expected.insert(([a, 2, z], [0, -1, 0]));
        }
    }
    assert_eq!(actual, expected);
    assert_eq!(chart.rows().len(), 16);
    assert_eq!(out.source_rows.len(), 16);
}
#[test]
fn sideways_construction_frame_controls_walls_independently_of_gravity() {
    let mut world = chamber_world(1, [2, 2, 2]);
    world.set_gravity(RegionId(1), [0, 0, -1]).unwrap();
    let mut scope = source_scope(&world, &[(1, box_cells(1, [0, 0, 0], [1, 1, 1]))], &[], &[]);
    certify(&world, &mut scope, &[frame(1, 1, Direction::East)], &[]);
    let out = prepared(&world, &scope);
    assert!(out
        .candidate
        .chart()
        .unwrap()
        .rows()
        .iter()
        .all(|r| matches!(
            r.mark,
            Mark::Mapping(MappingMark::WallFace {
                normal: Some([0, _, _])
            })
        )));
    assert!(out
        .candidate
        .chart()
        .unwrap()
        .rows()
        .iter()
        .any(|r| r.position == Some([0, 0, -1])));
}
#[test]
fn foreign_rotated_shell_uses_member_anchor_offset_and_inward_normal_without_foreign_alignment() {
    let mut world = chamber_world(2, [2, 2, 2]);
    let foreign = at(2, 0, 0, 0);
    world.set_wall(foreign, true).unwrap();
    let anchor = at(1, 1, 0, 0);
    world
        .connect_boundary_area(
            Passage {
                from: anchor,
                direction: Direction::East,
                to: foreign,
            },
            1,
            1,
            1,
        )
        .unwrap();
    assert_eq!(
        world.physical_geometry_neighbor(anchor, Direction::East),
        Some((foreign, 1))
    );
    let scope = box_scope(&world);
    assert_eq!(scope.members, vec![RegionId(1)]);
    let raw = mapping::wall_faces(
        &world,
        &scope,
        mapping::FaceBudget {
            work: 1_000_000,
            allocation_bytes: 1_000_000,
            faces: 1000,
        },
    )
    .unwrap();
    assert!(raw.iter().any(|f| f.anchor == anchor
        && f.toward == Direction::East
        && f.location == foreign
        && f.normal == Direction::North));
    let out = prepared(&world, &scope);
    let i = out
        .source_rows
        .iter()
        .position(|s| {
            *s == SourceRow {
                location: anchor,
                mark: SourceMark::WallFace {
                    room: room(1),
                    toward: Direction::East,
                },
            }
        })
        .unwrap();
    assert_eq!(out.candidate.chart().unwrap().components().len(), 1);
    assert_eq!(
        out.candidate.chart().unwrap().rows()[i],
        Row {
            component: 0,
            position: Some([2, 0, 0]),
            mark: Mark::Mapping(MappingMark::WallFace {
                normal: Some([-1, 0, 0])
            })
        }
    );
}
#[test]
fn disconnected_components_keep_independent_coordinates_and_inconsistent_components_omit_normals() {
    let mut world = chamber_world(2, [2, 2, 2]);
    let interiors = [
        (1, box_cells(1, [0, 0, 0], [1, 1, 1])),
        (2, box_cells(2, [0, 0, 0], [1, 1, 1])),
    ];
    let mut scope = source_scope(&world, &interiors, &[], &[]);
    certify(
        &world,
        &mut scope,
        &[frame(1, 1, Direction::Up), frame(2, 2, Direction::Up)],
        &[],
    );
    let out = prepared(&world, &scope);
    let chart = out.candidate.chart().unwrap();
    assert_eq!(chart.components().len(), 2);
    let rows = |component| {
        chart
            .rows()
            .iter()
            .filter(|r| r.component == component)
            .map(|r| (r.position, r.mark.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rows(0),
        rows(1),
        "disconnected panels do not invent a shared offset"
    );
    world
        .connect(
            Passage {
                from: at(1, 1, 0, 0),
                direction: Direction::East,
                to: at(2, 0, 0, 0),
            },
            0,
        )
        .unwrap();
    world
        .connect(
            Passage {
                from: at(2, 0, 0, 0),
                direction: Direction::West,
                to: at(1, 0, 0, 0),
            },
            0,
        )
        .unwrap();
    let out = prepared(&world, &scope);
    let chart = out.candidate.chart().unwrap();
    assert_eq!(chart.components().len(), 1);
    assert_eq!(
        chart.components()[0].alignment,
        crate::scope_projection::Alignment::Inconsistent
    );
    assert!(chart.rows().iter().all(|r| r.position.is_none()
        && matches!(
            r.mark,
            Mark::Mapping(MappingMark::WallFace { normal: None })
        )));
}
#[test]
fn changed_solid_anchor_cannot_dilate_new_wall_faces_and_corridor_solidity_is_current() {
    let mut world = chamber_world(1, [2, 2, 2]);
    let scope = box_scope(&world);
    let prior = prepared(&world, &scope);
    let anchor = at(1, 0, 0, 0);
    assert!(prior.source_rows.iter().any(|s| s.location == anchor));
    world.set_wall(anchor, true).unwrap();
    let after = prepared(&world, &scope);
    assert!(after.source_rows.iter().all(|s| s.location != anchor));
    assert!(
        after
            .candidate
            .chart()
            .unwrap()
            .rows()
            .iter()
            .any(|r| r.position == Some([0, 0, 0])),
        "neighboring clear interior may still disclose this newly solid wall face"
    );
    let mut world = chamber_world(1, [3, 1, 1]);
    let cells = vec![at(1, 0, 0, 0), at(1, 1, 0, 0), at(1, 2, 0, 0)];
    let mut scope = source_scope(&world, &[], &[], &cells);
    certify(&world, &mut scope, &[], &[]);
    assert_eq!(prepared(&world, &scope).source_rows.len(), 3);
    world.set_wall(cells[1], true).unwrap();
    assert_eq!(prepared(&world, &scope).source_rows.len(), 2);
}
#[test]
fn sparse_topology_discloses_door_stair_secret_and_trap_presence_only() {
    let mut world = chamber_world(2, [3, 1, 1]);
    let cells = vec![at(1, 0, 0, 0), at(1, 1, 0, 0), at(1, 2, 0, 0)];
    world.place_door(cells[0], 1, false, 1).unwrap();
    world
        .conceal_door(cells[0], tor_world::Concealment { dc: 12 })
        .unwrap();
    world
        .connect(
            Passage {
                from: cells[1],
                direction: Direction::Down,
                to: at(2, 1, 0, 0),
            },
            0,
        )
        .unwrap();
    world
        .place_trap(
            cells[2],
            tor_world::Trap {
                id: 7,
                dc: 12,
                avoidance_dc: None,
                kind: tor_world::TrapKind::Rust {
                    slot: "body_armor".into(),
                    amount: 1,
                },
            },
        )
        .unwrap();
    let mut scope = source_scope(&world, &[], &[], &cells);
    certify(&world, &mut scope, &[], &[]);
    let out = prepared(&world, &scope);
    let marks: BTreeSet<_> = out
        .candidate
        .chart()
        .unwrap()
        .rows()
        .iter()
        .map(|r| r.mark.clone())
        .collect();
    assert_eq!(
        marks,
        [
            MappingMark::Passage,
            MappingMark::Doorway,
            MappingMark::Stairs,
            MappingMark::SecretDoor,
            MappingMark::TrapPresence
        ]
        .into_iter()
        .map(Mark::Mapping)
        .collect()
    );
    assert_eq!(
        out.candidate.chart().unwrap().components().len(),
        1,
        "transport stair never loads or aligns destination"
    );
}
#[test]
fn unsupported_and_final_delivery_budget_failure_never_return_partial_available_prefix() {
    let world = chamber_world(1, [2, 2, 2]);
    let mut scope = box_scope(&world);
    let positive = prepared(&world, &scope);
    assert_eq!(positive.source_rows.len(), 16);
    for limits in [
        chart::delivery::PageLimits {
            records: 0,
            ..budget().pages
        },
        chart::delivery::PageLimits {
            bytes: budget().pages.envelope_bytes + 1,
            ..budget().pages
        },
    ] {
        let out = prepare_scope_mapping(
            &world,
            &scope,
            CensusBudget {
                pages: limits,
                ..budget()
            },
        );
        assert_eq!(
            out.candidate.unavailable_reason(),
            Some(Unavailable::Budget)
        );
        assert!(out.source_rows.is_empty());
    }
    scope.mapping = MappingCertification::Uncertified;
    assert_eq!(
        prepare_scope_mapping(&world, &scope, budget())
            .candidate
            .unavailable_reason(),
        Some(Unavailable::Projection)
    );
    scope.mapping = MappingCertification::CapacityUnavailable;
    assert_eq!(
        prepare_scope_mapping(&world, &scope, budget())
            .candidate
            .unavailable_reason(),
        Some(Unavailable::Budget)
    );
}
#[test]
fn chart_mark_constructor_requires_exact_cardinal_normal_and_matching_alignment() {
    let make = |alignment, position, normal| {
        Candidate::from_complete(
            Kind::Mapping,
            vec![Component { alignment }],
            vec![Row {
                component: 0,
                position,
                mark: Mark::Mapping(MappingMark::WallFace { normal }),
            }],
            ChartBudget {
                components: 1,
                rows: 1,
                json_bytes: 10_000,
            },
        )
    };
    use crate::scope_projection::Alignment::{Aligned, Inconsistent};
    make(Aligned, Some([i64::MIN, 0, i64::MAX]), Some([-1, 0, 0])).unwrap();
    make(Inconsistent, None, None).unwrap();
    for normal in [
        None,
        Some([0, 0, 0]),
        Some([1, 1, 0]),
        Some([2, 0, 0]),
        Some([i8::MIN; 3]),
    ] {
        assert!(make(Aligned, Some([0, 0, 0]), normal).is_err());
    }
    assert!(make(Inconsistent, None, Some([0, 1, 0])).is_err());
    for mark in [
        MappingMark::Passage,
        MappingMark::Doorway,
        MappingMark::Stairs,
        MappingMark::SecretDoor,
        MappingMark::TrapPresence,
        MappingMark::WallFace {
            normal: Some([0, 0, -1]),
        },
    ] {
        let row = Row {
            component: 0,
            position: Some([0, 0, 0]),
            mark: Mark::Mapping(mark),
        };
        assert_eq!(
            chart::delivery::row_encoded_size_bound(&row).unwrap(),
            chart::delivery::CHART_ROW_BYTES + chart::delivery::CHART_MAPPING_MARK_BYTES
        );
    }
}

pub(crate) fn game_fixture(certified: bool) -> Game {
    let mut game = crate::perception::contract_fixtures::fixture(&[], &[]);
    let cells = [at(1, 1, 2, 0), at(1, 2, 2, 0), at(1, 3, 2, 0)];
    let mut scope = source_scope(&game.world, &[], &[], &cells);
    if certified {
        certify(&game.world, &mut scope, &[], &[]);
    }
    game.register_spatial_scope(
        scope,
        &crate::spatial_scope::registry::Budget {
            scopes: 1,
            members: 1,
            rooms: 0,
            passages: 1,
            runs: 1,
            expanded_refs: 3,
            label_bytes: 128,
        },
    )
    .unwrap();
    game
}
fn scroll(game: &mut Game) -> crate::ItemId {
    let mut spec = crate::ItemSpec::ordinary("unread mapping page".into());
    spec.class = crate::ItemClass::Scroll;
    spec.identity = "fixture.mapping".into();
    spec.concealed = true;
    spec.stackable = true;
    spec.readable = Some(crate::ReadableSpec {
        control: None,
        descriptors: vec![],
        effects: vec![crate::rules::Effect::MapTopology],
        identify: None,
        discovery: crate::ItemDiscovery::OnUse,
    });
    game.place_item_stack(70, game.actors[&OWNER].location, Some(OWNER), 3, spec)
        .unwrap()
}
#[test]
fn unsupported_scope_rejects_paid_preparation_and_reports_support_without_mutation() {
    let mut game = game_fixture(false);
    let item = scroll(&mut game);
    let before = game.clone();
    assert!(!game.mapping_supported(OWNER));
    assert!(game.plan_work(OWNER, crate::Work::Read { item }).is_err());
    let view = game.detection_chart(OWNER, Kind::Mapping).unwrap();
    assert!(!view.supported);
    assert!(view.historical);
    assert_eq!(
        view.candidate.unwrap().unavailable_reason(),
        Some(Unavailable::Projection)
    );
    assert_eq!(game, before);
}
#[test]
fn paid_read_acquires_private_historical_chart_without_exploration_discovery_or_rng() {
    let mut game = game_fixture(true);
    let item = scroll(&mut game);
    let map = game.explored_maps.clone();
    let discovery = game.actors[&OWNER].discovery.clone();
    let rng = game.rules_rng;
    let plan = game.plan_work(OWNER, crate::Work::Read { item }).unwrap();
    assert!(matches!(
        plan.on_success(),
        [crate::abilities::PlannedEffect::MapTopology]
    ));
    let due = game.tick() + plan.preparation();
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = game.tick();
    let id = game
        .admit_intention(
            OWNER,
            crate::Action::Read { item },
            crate::IntentionOrigin::Human,
        )
        .unwrap();
    let execution = game.execute_next_intention().unwrap();
    assert_eq!(execution.intention.id, id);
    execution.outcome.unwrap();
    game.actors.get_mut(&SUBJECT).unwrap().turn_ticks = NonZeroU64::new(due - game.tick()).unwrap();
    game.act(SUBJECT, crate::Action::Wait).unwrap();
    assert_eq!(game.items[&item].quantity, 2);
    assert!(game.actors[&OWNER].knowledge.contains("fixture.mapping"));
    let view = game.detection_chart(OWNER, Kind::Mapping).unwrap();
    assert!(view.supported && view.historical);
    assert_eq!(view.acquired_at, Some(due));
    assert_eq!(view.candidate.unwrap().chart().unwrap().rows().len(), 3);
    assert_eq!(game.rules_rng, rng);
    assert_eq!(game.actors[&OWNER].discovery, discovery);
    assert_eq!(game.explored_maps, map);
    assert_eq!(
        crate::hallucination_clock::integration_tests::roundtrip(&game),
        game
    );
}

/// Shared lawful wall proof fixture for strict historical replay mutations.
pub(crate) fn foreign_game_fixture() -> Game {
    let mut world = chamber_world(2, [2, 2, 2]);
    let foreign = at(2, 0, 0, 0);
    world.set_wall(foreign, true).unwrap();
    world
        .connect_boundary_area(
            Passage {
                from: at(1, 1, 0, 0),
                direction: Direction::East,
                to: foreign,
            },
            1,
            1,
            1,
        )
        .unwrap();
    let scope = box_scope(&world);
    let content = crate::perception::contract_fixtures::fixture(&[], &[])
        .content()
        .clone();
    let mut game = Game::new(world, 42, content);
    assert_eq!(
        crate::perception::contract_fixtures::spawn(
            &mut game,
            "fixture_perception_owner",
            at(1, 0, 0, 0)
        ),
        OWNER
    );
    assert_eq!(
        crate::perception::contract_fixtures::spawn(
            &mut game,
            "fixture_perception_subject",
            at(1, 1, 1, 0)
        ),
        SUBJECT
    );
    game.register_spatial_scope(
        scope,
        &crate::spatial_scope::registry::Budget {
            scopes: 1,
            members: 1,
            rooms: 1,
            passages: 0,
            runs: 100,
            expanded_refs: 1000,
            label_bytes: 128,
        },
    )
    .unwrap();
    game
}

#[test]
fn same_foreign_solid_face_from_disconnected_member_anchors_keeps_both_occurrences() {
    let mut world = chamber_world(3, [2, 2, 2]);
    let foreign = at(3, 0, 0, 0);
    world.set_wall(foreign, true).unwrap();
    for region in [1, 2] {
        world
            .connect_boundary_area(
                Passage {
                    from: at(region, 1, 0, 0),
                    direction: Direction::East,
                    to: foreign,
                },
                1,
                1,
                1,
            )
            .unwrap();
    }
    let interiors = [
        box_cells(1, [0, 0, 0], [1, 1, 1]),
        box_cells(2, [0, 0, 0], [1, 1, 1]),
    ]
    .concat();
    let mut scope = source_scope(&world, &[(1, interiors)], &[], &[]);
    certify(
        &world,
        &mut scope,
        &[frame(1, 1, Direction::Up), frame(1, 2, Direction::Up)],
        &[],
    );
    let out = prepared(&world, &scope);
    let chart = out.candidate.chart().unwrap();
    assert_eq!(chart.components().len(), 2);
    assert_eq!(chart.rows().len(), 32);
    let faces: Vec<_> = chart
        .rows()
        .iter()
        .filter(|row| {
            row.position == Some([2, 0, 0])
                && row.mark
                    == Mark::Mapping(MappingMark::WallFace {
                        normal: Some([-1, 0, 0]),
                    })
        })
        .collect();
    assert_eq!(faces.len(), 2);
    assert_ne!(faces[0].component, faces[1].component);
}
#[test]
fn mapping_source_plan_and_resolved_operands_are_closed_and_sole_self_payloads() {
    use crate::abilities::{PlannedEffect, ResolvedEffect};
    use crate::rules::Effect;
    let game = game_fixture(true);
    game.content
        .rules()
        .validate_effects(&[Effect::MapTopology])
        .unwrap();
    assert!(game
        .content
        .rules()
        .validate_effects(&[Effect::MapTopology, Effect::DetectFood])
        .is_err());
    for raw in [
        r#"{"kind":"map_topology","scope":"private"}"#,
        r#"["map_topology"]"#,
    ] {
        assert!(serde_json::from_str::<Effect>(raw).is_err());
        assert!(serde_json::from_str::<PlannedEffect>(raw).is_err());
    }
    let source: Effect = serde_json::from_str(r#"{"kind":"map_topology"}"#).unwrap();
    assert_eq!(source, Effect::MapTopology);
    let planned: PlannedEffect = serde_json::from_str(r#"{"kind":"map_topology"}"#).unwrap();
    assert_eq!(planned, PlannedEffect::MapTopology);
    assert!(serde_json::from_str::<ResolvedEffect>(r#"{"kind":"map_topology"}"#).is_err());
    assert!(
        serde_json::from_str::<ResolvedEffect>(r#"{"kind":"map_topology","changed":null}"#)
            .is_err()
    );
    assert_eq!(
        serde_json::from_str::<ResolvedEffect>(r#"{"kind":"map_topology","changed":true}"#)
            .unwrap(),
        ResolvedEffect::MapTopology { changed: true }
    );
}

#[test]
fn foreign_shell_missing_geometry_is_unavailable_and_pending_pin_loads_without_aligning_it() {
    let mut game = foreign_game_fixture();
    let scope = game.spatial_registry.scopes().next().unwrap().clone();
    assert_eq!(scope.members, vec![RegionId(1)]);
    let mut incomplete = (*game.world).clone();
    incomplete.detach_region(RegionId(2)).unwrap();
    let unavailable = prepare_scope_mapping(&incomplete, &scope, budget());
    assert_eq!(
        unavailable.candidate.unavailable_reason(),
        Some(Unavailable::Projection)
    );
    assert!(unavailable.source_rows.is_empty());
    let item = scroll(&mut game);
    game.actors.get_mut(&SUBJECT).unwrap().ready_at = game.tick();
    game.admit_intention(
        OWNER,
        crate::Action::Read { item },
        crate::IntentionOrigin::Human,
    )
    .unwrap();
    game.execute_next_intention().unwrap().outcome.unwrap();
    let active: BTreeSet<_> = [RegionId(1)].into();
    let mut loaded = active.clone();
    let before = game.clone();
    game.relocation_loaded_closure(&active, &mut loaded);
    assert_eq!(loaded, [RegionId(1), RegionId(2)].into());
    assert_eq!(active, [RegionId(1)].into());
    assert_eq!(game, before);
}
