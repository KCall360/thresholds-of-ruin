use super::*;
use crate::mapping_chart::prepare_scope_mapping;
use crate::spatial_scope::mapping::tests::{
    box_cells, certify, chamber_world, frame, source_scope,
};
use tor_world::Direction;

fn root() -> Shared<HistoricalChart> {
    let world = chamber_world(1, [2, 2, 2]);
    let mut scope = source_scope(&world, &[(1, box_cells(1, [0, 0, 0], [1, 1, 1]))], &[], &[]);
    certify(&world, &mut scope, &[frame(1, 1, Direction::Up)], &[]);
    let prepared =
        prepare_scope_mapping(&world, &scope, crate::detection::CensusBudget::canonical());
    let chart = prepared
        .candidate
        .shared_chart()
        .expect("complete certified mapping")
        .clone();
    assert_eq!(chart.rows().len(), 16);
    let mut registry = Registry::default();
    registry.register_archive_checked(scope.clone()).unwrap();
    Shared::new(
        HistoricalChart::checked_mapping(
            12,
            scope.id,
            Shared::new(AcquisitionAuthority {
                world: Shared::new(world),
                registry: Shared::new(registry),
            }),
            chart,
            prepared.source_rows,
            archive_budget(),
        )
        .unwrap(),
    )
}
fn owners(root: Shared<HistoricalChart>) -> BTreeMap<ActorId, Owner> {
    let mut owner = Owner::default();
    owner
        .record_attempt(
            Kind::Mapping,
            Attempt {
                attempted_at: 12,
                status: AttemptStatus::Positive,
            },
            Some(root),
        )
        .unwrap();
    BTreeMap::from([(ActorId(1), owner)])
}
#[test]
fn mapping_history_roundtrip_retains_exact_acquisition_and_empty_unavailable_do_not_erase() {
    let root = root();
    let mut owners = owners(root.clone());
    let owner = owners.get_mut(&ActorId(1)).unwrap();
    for (tick, status) in [
        (13, AttemptStatus::Empty),
        (
            14,
            AttemptStatus::Unavailable(UnavailableReason::Projection),
        ),
    ] {
        owner
            .record_attempt(
                Kind::Mapping,
                Attempt {
                    attempted_at: tick,
                    status,
                },
                None,
            )
            .unwrap();
        assert!(owner
            .get(Kind::Mapping)
            .latest()
            .unwrap()
            .shares_storage(&root));
    }
    owner
        .record_attempt(
            Kind::Food,
            Attempt {
                attempted_at: 14,
                status: AttemptStatus::Empty,
            },
            None,
        )
        .unwrap();
    owner
        .record_attempt(
            Kind::Magic,
            Attempt {
                attempted_at: 14,
                status: AttemptStatus::Unavailable(UnavailableReason::Budget),
            },
            None,
        )
        .unwrap();
    let bytes = serde_json::to_vec(&codec::LocalHistory::capture(&owners)).unwrap();
    let decoded: codec::LocalHistory = serde_json::from_slice(&bytes).unwrap();
    let decoded = decoded.restore().unwrap();
    assert_eq!(*decoded, owners);
    assert_eq!(
        decoded[&ActorId(1)]
            .get(Kind::Mapping)
            .latest()
            .unwrap()
            .acquired_tick(),
        12
    );
}
#[test]
fn mapping_history_rejects_changed_wall_proof_normal_missing_occurrence_and_kind() {
    let original = serde_json::to_value(codec::LocalHistory::capture(&owners(root()))).unwrap();
    let positive: codec::LocalHistory = serde_json::from_value(original.clone()).unwrap();
    assert!(positive.restore().is_ok());
    for mutation in 0..7 {
        let mut forged = original.clone();
        let point = &mut forged["history"]["roots"][0]["points"][0];
        match mutation {
            0 => point["mapping"]["room"] = serde_json::json!(999),
            1 => point["mapping"]["normal"] = serde_json::json!([0, 0, 1]),
            2 => point["mapping"]["toward"] = serde_json::json!("Up"),
            3 => point["mapping"] = serde_json::Value::Null,
            4 => {
                forged["history"]["roots"][0]["points"]
                    .as_array_mut()
                    .unwrap()
                    .pop();
            }
            5 => forged["history"]["roots"][0]["kind"] = serde_json::json!("food"),
            6 => {
                point["mapping"].as_object_mut().unwrap().remove("normal");
            }
            _ => unreachable!(),
        }
        let accepted = serde_json::from_value::<codec::LocalHistory>(forged)
            .ok()
            .and_then(|v| v.restore().ok());
        assert!(accepted.is_none(), "accepted mutation {mutation}");
    }
}
#[test]
fn mapping_history_uses_captured_world_not_later_geometry() {
    let root = root();
    let before = root.chart.clone();
    let mut later = root.authority.world.clone();
    later
        .set_gravity(tor_world::RegionId(1), [0, 0, -1])
        .unwrap();
    assert!(!later.shares_storage(&root.authority.world));
    assert!(later.historical_checkpoint_valid());
    root.validate_geometry(archive_budget()).unwrap();
    let restored = codec::LocalHistory::capture(&owners(root))
        .restore()
        .unwrap();
    assert_eq!(
        restored[&ActorId(1)]
            .get(Kind::Mapping)
            .latest()
            .unwrap()
            .chart(),
        before.as_ref()
    );
}

#[test]
fn mapping_completion_captures_foreign_wall_and_history_survives_unavailable_attempt() {
    let mut game = crate::mapping_chart::tests::foreign_game_fixture();
    let actor = ActorId(1);
    let completion = game
        .complete_mapping_chart_with_budget(actor, crate::detection::CensusBudget::canonical())
        .unwrap();
    assert!(completion.admitted_nonempty() && completion.history_changed);
    let saved = game
        .chart_history(actor)
        .unwrap()
        .get(Kind::Mapping)
        .latest()
        .unwrap()
        .clone();
    assert!(saved
        .chart()
        .rows()
        .iter()
        .any(|row| row.position == Some([2, 0, 0])
            && matches!(
                row.mark,
                chart::Mark::Mapping(chart::MappingMark::WallFace {
                    normal: Some([-1, 0, 0])
                })
            )));
    let mut budget = crate::detection::CensusBudget::canonical();
    budget.work = 0;
    let completion = game
        .complete_mapping_chart_with_budget(actor, budget)
        .unwrap();
    assert_eq!(
        completion.candidate.unavailable_reason(),
        Some(chart::Unavailable::Budget)
    );
    assert!(game
        .chart_history(actor)
        .unwrap()
        .get(Kind::Mapping)
        .latest()
        .unwrap()
        .shares_storage(&saved));
    assert_eq!(
        game.chart_history(actor)
            .unwrap()
            .get(Kind::Mapping)
            .attempt()
            .unwrap()
            .status,
        AttemptStatus::Unavailable(UnavailableReason::Budget)
    );
    let restored = crate::hallucination_clock::integration_tests::roundtrip(&game);
    assert_eq!(restored, game);
    assert_eq!(
        restored.mapping_chart(actor).unwrap().acquired_at,
        Some(saved.acquired_tick())
    );
}
