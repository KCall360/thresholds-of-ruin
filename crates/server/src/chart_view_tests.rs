use super::*;
use tor_simulation::scope_projection::Alignment;
fn candidate(kind: chart::Kind, rows: usize) -> chart::Candidate {
    chart::Candidate::from_complete(
        kind,
        vec![chart::Component {
            alignment: Alignment::Aligned,
        }],
        (0..rows)
            .map(|_| chart::Row {
                component: 0,
                position: Some([7, -2, 1]),
                mark: match kind {
                    chart::Kind::Monsters => chart::Mark::Monster(
                        tor_simulation::appearance_catalog::CreatureAppearance {
                            name: Arc::from("sample"),
                            asset: None,
                        },
                    ),
                    chart::Kind::Food => chart::Mark::Food,
                    chart::Kind::Magic => chart::Mark::Magic,
                    chart::Kind::Mapping => chart::Mark::Mapping(chart::MappingMark::Passage),
                },
            })
            .collect(),
        chart::Budget {
            components: 1,
            rows,
            json_bytes: usize::MAX,
        },
    )
    .unwrap()
}
fn prepared() -> PreparedCharts {
    PreparedCharts(
        [
            chart::Kind::Monsters,
            chart::Kind::Food,
            chart::Kind::Magic,
            chart::Kind::Mapping,
        ]
        .map(|kind| DetectionView {
            kind,
            supported: true,
            historical: kind != chart::Kind::Monsters,
            acquired_at: None,
            candidate: None,
            latest_attempt: None,
        }),
    )
}
#[test]
fn summary_distinguishes_inactive_empty_positive_unavailable_and_independent_latest_attempt() {
    let mut views = prepared();
    let summaries = views.summaries("salt", ActorId(1), &BranchId("b".into()), 2);
    validate_summaries(&summaries).unwrap();
    assert!(summaries
        .iter()
        .all(|s| s.status == ChartStatus::Inactive && s.chart.is_none()));
    views.0[0].candidate = Some(candidate(chart::Kind::Monsters, 0));
    views.0[0].acquired_at = Some(50);
    views.0[1].candidate = Some(candidate(chart::Kind::Food, 2));
    views.0[1].acquired_at = Some(20);
    views.0[1].latest_attempt = Some(tor_simulation::chart_history::Attempt {
        attempted_at: 50,
        status: tor_simulation::chart_history::AttemptStatus::Empty,
    });
    views.0[2].candidate = Some(chart::Candidate::unavailable(chart::Unavailable::Budget));
    views.0[2].acquired_at = Some(50);
    let summaries = views.summaries("salt", ActorId(1), &BranchId("b".into()), 2);
    validate_summaries(&summaries).unwrap();
    assert_eq!(
        summaries.iter().map(|s| s.status).collect::<Vec<_>>(),
        vec![
            ChartStatus::Empty,
            ChartStatus::Available,
            ChartStatus::Unavailable,
            ChartStatus::Inactive
        ]
    );
    assert!(summaries[0].chart.is_some());
    assert_eq!(summaries[1].mode, ChartMode::Historical);
    assert_eq!(summaries[1].acquired_at, Some(20));
    assert_eq!(
        summaries[1].latest_attempt.as_ref().unwrap().status,
        ChartAttemptStatus::Empty
    );
    assert!(summaries[2].chart.is_none());
}
#[test]
fn stable_opaque_tokens_are_separated_by_authority_actor_revision_and_kind() {
    let base = token(
        "secret",
        ActorId(1),
        &BranchId("a".into()),
        7,
        chart::Kind::Food,
    );
    assert_eq!(base.len(), 64);
    assert_eq!(
        base,
        token(
            "secret",
            ActorId(1),
            &BranchId("a".into()),
            7,
            chart::Kind::Food
        )
    );
    for alternate in [
        token(
            "other",
            ActorId(1),
            &BranchId("a".into()),
            7,
            chart::Kind::Food,
        ),
        token(
            "secret",
            ActorId(2),
            &BranchId("a".into()),
            7,
            chart::Kind::Food,
        ),
        token(
            "secret",
            ActorId(1),
            &BranchId("b".into()),
            7,
            chart::Kind::Food,
        ),
        token(
            "secret",
            ActorId(1),
            &BranchId("a".into()),
            8,
            chart::Kind::Food,
        ),
        token(
            "secret",
            ActorId(1),
            &BranchId("a".into()),
            7,
            chart::Kind::Magic,
        ),
    ] {
        assert_ne!(base, alternate);
    }
}
#[test]
fn adapter_preserves_component_prefix_duplicate_occurrences_and_nonactionable_fields() {
    let candidate = candidate(chart::Kind::Monsters, 2);
    let chart = candidate.chart().unwrap();
    let records = (0..3)
        .map(|n| record(chart, n).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(records[0], ChartRecord::Component(_)));
    assert_eq!(records[1], records[2]);
    assert!(record(chart, 3).is_none());
    let value = serde_json::to_value(&records[1]).unwrap();
    assert_eq!(
        value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from(["record", "component", "position", "mark"])
    );
    assert_eq!(
        value["mark"],
        serde_json::json!({"kind":"monster","name":"sample","asset":null})
    );
}
#[test]
fn semantic_prepared_equality_detects_far_rows_without_ordinary_sight_inputs() {
    let mut before = prepared();
    before.0[0].acquired_at = Some(100);
    before.0[0].candidate = Some(candidate(chart::Kind::Monsters, 1));
    let mut moved = before.clone();
    let chart = moved.0[0].candidate.as_ref().unwrap().chart().unwrap();
    let mut rows = chart.rows().to_vec();
    rows[0].position = Some([8, -2, 1]);
    moved.0[0].candidate = Some(
        chart::Candidate::from_complete(
            chart::Kind::Monsters,
            chart.components().to_vec(),
            rows,
            chart::Budget {
                components: 1,
                rows: 1,
                json_bytes: usize::MAX,
            },
        )
        .unwrap(),
    );
    assert_ne!(
        before, moved,
        "same acquisition tick still detects far peer movement"
    );
    let mut expired = before.clone();
    expired.0[0].candidate = None;
    expired.0[0].acquired_at = None;
    assert_ne!(before, expired);
    let mut unavailable = prepared();
    unavailable.0[0].candidate = Some(chart::Candidate::unavailable(
        chart::Unavailable::Projection,
    ));
    assert_eq!(
        unavailable,
        unavailable.clone(),
        "no private phase participates in semantic equality"
    );
}

#[test]
fn current_mapping_support_is_separate_from_retained_acquisition() {
    let mut views = prepared();
    views.0[3].supported = false;
    views.0[3].historical = true;
    views.0[3].candidate = Some(candidate(chart::Kind::Mapping, 1));
    views.0[3].acquired_at = Some(10);
    let summaries = views.summaries("salt", ActorId(1), &BranchId("b".into()), 2);
    validate_summaries(&summaries).unwrap();
    assert_eq!(summaries.len(), 4);
    assert_eq!(summaries[3].kind, ChartKind::Mapping);
    assert!(!summaries[3].supported);
    assert_eq!(
        (
            summaries[3].mode,
            summaries[3].status,
            summaries[3].acquired_at
        ),
        (ChartMode::Historical, ChartStatus::Available, Some(10))
    );
    assert!(summaries[3].chart.is_some() && views.chart(ChartKind::Mapping).is_some());
    let mut newly_supported = views.clone();
    newly_supported.0[3].supported = true;
    assert_ne!(
        views, newly_supported,
        "floor support changes are semantic revision facts"
    );
}
#[test]
fn mapping_adapter_preserves_transformed_cardinal_or_omits_inconsistent_orientation() {
    for (alignment, position, normal) in [
        (Alignment::Aligned, Some([7, -2, 1]), Some([0, -1, 0])),
        (Alignment::Inconsistent, None, None),
    ] {
        let candidate = chart::Candidate::from_complete(
            chart::Kind::Mapping,
            vec![chart::Component { alignment }],
            vec![chart::Row {
                component: 0,
                position,
                mark: chart::Mark::Mapping(chart::MappingMark::WallFace { normal }),
            }],
            chart::Budget {
                components: 1,
                rows: 1,
                json_bytes: usize::MAX,
            },
        )
        .unwrap();
        let row = record(candidate.chart().unwrap(), 1).unwrap();
        let ChartRecord::Row(row) = row else {
            panic!("mapping row")
        };
        row.validate().unwrap();
        assert_eq!(row.position, position);
        assert_eq!(
            row.mark,
            ChartMark::Mapping {
                fact: MappingMark::WallFace { normal }
            }
        );
        let value = serde_json::to_value(row).unwrap();
        assert_eq!(
            value["mark"],
            serde_json::json!({"kind":"mapping","fact":{"kind":"wall_face","normal":normal}})
        );
    }
}
