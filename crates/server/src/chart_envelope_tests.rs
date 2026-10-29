//! Independent maximum-envelope proof through the production chart adapter.
use std::sync::Arc;
use tor_protocol::chart_pages as wire;
use tor_simulation::chart::{self, delivery as core};

fn record(chart: &chart::Chart, ordinal: usize) -> wire::ChartRecord {
    crate::chart_view::record(chart, ordinal).unwrap()
}

#[test]
fn actual_complete_chart_wire_pages_fit_core_admission_with_maximum_correlated_envelopes() {
    assert_eq!(core::MAX_CHART_PAGE_RECORDS, wire::MAX_CHART_PAGE_RECORDS);
    assert_eq!(core::MAX_CHART_PAGE_BYTES, wire::MAX_CHART_PAGE_BYTES);
    assert_eq!(core::CHART_ENVELOPE_BYTES, wire::CHART_ENVELOPE_BYTES);
    assert_eq!(core::CHART_COMPONENT_BYTES, wire::CHART_COMPONENT_BYTES);
    assert_eq!(core::CHART_ROW_BYTES, wire::CHART_ROW_BYTES);
    assert_eq!(
        core::CHART_MAPPING_MARK_BYTES,
        wire::CHART_MAPPING_MARK_BYTES
    );
    assert_eq!(
        core::CHART_MONSTER_MARK_BYTES,
        wire::CHART_MONSTER_MARK_BYTES
    );
    assert_eq!(
        core::CHART_PRESENCE_MARK_BYTES,
        wire::CHART_PRESENCE_MARK_BYTES
    );
    let candidate = chart::Candidate::from_complete(
        chart::Kind::Monsters,
        vec![
            chart::Component {
                alignment: tor_simulation::scope_projection::Alignment::Aligned
            };
            700
        ],
        vec![
            chart::Row {
                component: 699,
                position: Some([i64::MIN, i64::MAX, -1]),
                mark: chart::Mark::Monster(
                    tor_simulation::appearance_catalog::CreatureAppearance {
                        name: Arc::from("\\\"".repeat(128)),
                        asset: Some(Arc::from("a".repeat(80)))
                    }
                )
            };
            800
        ],
        chart::Budget {
            components: 700,
            rows: 800,
            json_bytes: usize::MAX,
        },
    )
    .unwrap();
    let chart = candidate.chart().unwrap();
    let limits = core::PageLimits::canonical();
    let summary = chart.delivery_summary(limits).unwrap();
    let mut start = 0;
    let mut pages = 0;
    let mut actual_bytes = 0;
    let mut validator = wire::ChartStreamValidator::new(
        tor_protocol::ActorId(u64::MAX),
        wire::ChartAssemblyBudget {
            components: 700,
            records: 1500,
            encoded_bytes: u64::try_from(summary.encoded_bytes_bound).unwrap(),
        },
    );
    while start < summary.records {
        let span = chart.page_span(start, limits).unwrap();
        let records: Vec<_> = (start..span.end)
            .map(|ordinal| {
                let record = record(chart, ordinal);
                assert_eq!(
                    record.encoded_size_bound().unwrap(),
                    chart.record_encoded_size_bound(ordinal).unwrap()
                );
                assert!(
                    serde_json::to_vec(&record).unwrap().len()
                        <= chart.record_encoded_size_bound(ordinal).unwrap()
                );
                record
            })
            .collect();
        let reply = wire::ChartPageReply {
            message_type: wire::ChartReplyType::ChartPage,
            context: tor_protocol::ReplyContext {
                snapshot: tor_protocol::SnapshotId("fixture".into()),
                input: tor_protocol::InputContext {
                    stream: tor_protocol::StreamContext {
                        stream: tor_protocol::StreamId("\"".repeat(128)),
                        epoch: u64::MAX,
                    },
                    readiness_revision: u64::MAX,
                },
                actor: tor_protocol::ActorId(u64::MAX),
                branch: tor_protocol::BranchId("\\".repeat(128)),
                cursor: tor_protocol::StreamCursor {
                    sequence: u64::MAX,
                    tick: u64::MAX,
                },
                revision: u64::MAX,
            },
            request_id: "\\".repeat(128),
            page: wire::ChartPage {
                context: tor_protocol::StreamContext {
                    stream: tor_protocol::StreamId("\"".repeat(128)),
                    epoch: u64::MAX,
                },
                branch: tor_protocol::BranchId("\\".repeat(128)),
                base: tor_protocol::ObservationBase {
                    snapshot: tor_protocol::SnapshotId("fixture".into()),
                    cursor: tor_protocol::StreamCursor {
                        sequence: u64::MAX,
                        tick: u64::MAX,
                    },
                    revision: u64::MAX,
                },
                chart: "x".repeat(128),
                kind: wire::ChartKind::Monsters,
                mode: wire::ChartMode::Historical,
                acquired_at: u64::MAX,
                total_components: 700,
                total_records: 1500,
                start: u64::try_from(start).unwrap(),
                records,
                next: (span.end < summary.records).then(|| u64::try_from(span.end).unwrap()),
            },
        };
        reply.validate().unwrap();
        let message = tor_protocol::ServerMessage::ChartPage {
            context: reply.context.clone(),
            request_id: reply.request_id.clone(),
            page: Box::new(reply.page.clone()),
        };
        let text = tor_protocol::encode_response(&message, None, tor_protocol::MAX_RESPONSE_BYTES)
            .unwrap()
            .text;
        assert_eq!(text, serde_json::to_string(&reply).unwrap());
        assert_eq!(tor_protocol::decode_response(&text).unwrap(), message);
        assert!(text.len() <= span.encoded_bytes_bound);
        assert!(span.encoded_bytes_bound <= wire::MAX_CHART_PAGE_BYTES);
        let decoded = wire::decode_chart_page_reply(&text).unwrap();
        assert_eq!(decoded, reply);
        let query = wire::ChartQuery {
            context: reply.page.context.clone(),
            branch: reply.page.branch.clone(),
            base: reply.page.base,
            chart: reply.page.chart.clone(),
            cursor: (start != 0).then(|| u64::try_from(start).unwrap()),
        };
        validator
            .accept(&query, &reply.request_id, &decoded)
            .unwrap();
        actual_bytes += text.len();
        pages += 1;
        start = span.end;
    }
    assert!(validator.is_complete());
    assert_eq!(validator.records_seen(), 1500);
    assert_eq!(pages, summary.pages);
    assert!(actual_bytes <= summary.encoded_bytes_bound);
}

#[test]
fn mapping_fact_record_charges_match_core_and_actual_production_wire_adapter() {
    use chart::MappingMark as M;
    let marks = [
        M::WallFace {
            normal: Some([1, 0, 0]),
        },
        M::WallFace {
            normal: Some([-1, 0, 0]),
        },
        M::WallFace {
            normal: Some([0, 1, 0]),
        },
        M::WallFace {
            normal: Some([0, -1, 0]),
        },
        M::WallFace {
            normal: Some([0, 0, 1]),
        },
        M::WallFace {
            normal: Some([0, 0, -1]),
        },
        M::Passage,
        M::Doorway,
        M::Stairs,
        M::SecretDoor,
        M::TrapPresence,
    ];
    for mark in marks {
        let candidate = chart::Candidate::from_complete(
            chart::Kind::Mapping,
            vec![chart::Component {
                alignment: tor_simulation::scope_projection::Alignment::Aligned,
            }],
            vec![chart::Row {
                component: 0,
                position: Some([i64::MIN, i64::MAX, -1]),
                mark: chart::Mark::Mapping(mark),
            }],
            chart::Budget {
                components: 1,
                rows: 1,
                json_bytes: usize::MAX,
            },
        )
        .unwrap();
        let chart = candidate.chart().unwrap();
        let record = record(chart, 1);
        assert_eq!(
            record.encoded_size_bound().unwrap(),
            core::CHART_ROW_BYTES + core::CHART_MAPPING_MARK_BYTES
        );
        assert_eq!(
            record.encoded_size_bound().unwrap(),
            chart.record_encoded_size_bound(1).unwrap()
        );
        assert!(serde_json::to_vec(&record).unwrap().len() <= record.encoded_size_bound().unwrap());
    }
}
