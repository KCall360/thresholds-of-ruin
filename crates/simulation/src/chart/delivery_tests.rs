use super::*;

fn chart(components: usize, rows: Vec<Row>, kind: Kind) -> Arc<Chart> {
    let value = Candidate::from_complete(
        kind,
        vec![
            Component {
                alignment: crate::scope_projection::Alignment::Aligned
            };
            components
        ],
        rows,
        Budget {
            components: usize::MAX,
            rows: usize::MAX,
            json_bytes: usize::MAX,
        },
    )
    .unwrap();
    let CandidateValue::Available(chart) = value.0 else {
        panic!("lawful complete positive")
    };
    chart
}
fn largest_monster() -> Row {
    Row {
        component: 0,
        position: Some([i64::MIN, i64::MAX, -1]),
        mark: Mark::Monster(crate::appearance_catalog::CreatureAppearance {
            name: Arc::from("\\\"".repeat(128)),
            asset: Some(Arc::from("a".repeat(80))),
        }),
    }
}

#[test]
fn every_valid_max_record_fits_and_total_admission_checks_later_rows_before_publication() {
    let row = largest_monster();
    assert_eq!(row_encoded_size_bound(&row), Ok(2372));
    let chart = chart(1, vec![row], Kind::Monsters);
    assert_eq!(chart.record_count(), Ok(2));
    let one_page = chart.delivery_summary(PageLimits::canonical()).unwrap();
    assert_eq!(
        one_page,
        DeliverySummary {
            records: 2,
            pages: 1,
            encoded_bytes_bound: CHART_ENVELOPE_BYTES + 97 + 2373
        }
    );
    let exact = PageLimits {
        bytes: one_page.encoded_bytes_bound,
        ..PageLimits::canonical()
    };
    assert_eq!(chart.page_end(0, exact), Ok(2));
    let split = PageLimits {
        bytes: exact.bytes - 1,
        ..exact
    };
    assert_eq!(chart.page_end(0, split), Ok(1));
    assert_eq!(chart.page_end(1, split), Ok(2));
    assert_eq!(chart.delivery_summary(split).unwrap().pages, 2);
    let insufficient = PageLimits {
        bytes: CHART_ENVELOPE_BYTES + 2372,
        ..exact
    };
    assert_eq!(chart.page_end(0, insufficient), Ok(1));
    assert_eq!(
        chart.page_end(1, insufficient),
        Err(DeliveryError::RecordTooLarge)
    );
    assert_eq!(
        chart.delivery_summary(insufficient),
        Err(DeliveryError::RecordTooLarge)
    );
}

#[test]
fn declarations_and_empty_charts_stream_without_member_or_total_page_caps() {
    let chart = chart(700, vec![], Kind::Food);
    let ordinary = chart.delivery_summary(PageLimits::canonical()).unwrap();
    assert_eq!(ordinary.records, 700);
    assert_eq!(ordinary.pages, 2);
    assert_eq!(chart.page_end(0, PageLimits::canonical()), Ok(512));
    assert_eq!(chart.page_end(512, PageLimits::canonical()), Ok(700));
    let tiny = PageLimits {
        records: 1,
        ..PageLimits::canonical()
    };
    let summary = chart.delivery_summary(tiny).unwrap();
    assert_eq!(summary.pages, 700); // deliberately exceeds old map transfer128
    assert_eq!(
        summary.encoded_bytes_bound,
        700 * (CHART_ENVELOPE_BYTES + 97)
    );
}

#[test]
fn duplicate_rows_each_consume_bytes_and_cross_count_and_byte_page_boundaries() {
    let row = largest_monster();
    let chart = chart(1, vec![row; 800], Kind::Monsters);
    let limits = PageLimits::canonical();
    let first = chart.page_span(0, limits).unwrap();
    assert_eq!(first.end, 52); // one component and51 max-size occurrences
    assert_eq!(first.encoded_bytes_bound, 8192 + 97 + 51 * 2373);
    let summary = chart.delivery_summary(limits).unwrap();
    assert_eq!(summary.records, 801);
    assert_eq!(summary.pages, 16);
    assert_eq!(
        summary.encoded_bytes_bound,
        summary.pages * 8192 + 97 + 800 * 2373
    );
}

#[test]
fn invalid_limits_ranges_and_checked_overflow_do_not_emit_partial_pages() {
    let chart = chart(1, vec![], Kind::Magic);
    for limits in [
        PageLimits {
            records: 0,
            ..PageLimits::canonical()
        },
        PageLimits {
            bytes: 8192,
            ..PageLimits::canonical()
        },
        PageLimits {
            envelope_bytes: usize::MAX,
            ..PageLimits::canonical()
        },
    ] {
        assert_eq!(chart.delivery_summary(limits), Err(DeliveryError::Limits));
    }
    assert_eq!(
        chart.page_end(1, PageLimits::canonical()),
        Err(DeliveryError::Range)
    );
    assert_eq!(
        chart.record_encoded_size_bound(usize::MAX),
        Err(DeliveryError::Range)
    );
    let overflow = PageLimits {
        records: 1,
        bytes: usize::MAX,
        envelope_bytes: usize::MAX - 1,
    };
    assert_eq!(
        chart.delivery_summary(overflow),
        Err(DeliveryError::Arithmetic)
    );
}
