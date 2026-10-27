use super::*;
fn budget() -> Budget {
    Budget {
        components: 8,
        rows: 16,
        json_bytes: 8192,
    }
}
fn components() -> Vec<Component> {
    vec![Component {
        alignment: crate::scope_projection::Alignment::Aligned,
    }]
}
fn food(x: u32) -> Row {
    Row {
        component: 0,
        position: Some([i64::from(x), 0, 0]),
        mark: Mark::Food,
    }
}
fn make(rows: Vec<Row>, budget: Budget) -> Result<Candidate, Invalid> {
    Candidate::from_complete(Kind::Food, components(), rows, budget)
}
#[test]
fn empty_available_and_unavailable_are_distinct_without_identification_authority() {
    assert!(make(vec![], budget())
        .unwrap()
        .chart()
        .unwrap()
        .rows()
        .is_empty());
    for reason in [Unavailable::Projection, Unavailable::Budget] {
        let value = Candidate::unavailable(reason);
        assert!(value.chart().is_none());
        assert_eq!(value.unavailable_reason(), Some(reason));
    }
}
#[test]
fn kind_order_and_position_failures_are_not_empty_success() {
    assert_eq!(make(vec![food(2), food(1)], budget()), Err(Invalid::Order));
    let mut wrong_component = food(0);
    wrong_component.component = 1;
    assert_eq!(
        make(vec![wrong_component], budget()),
        Err(Invalid::Position)
    );
    for kind in [Kind::Monsters, Kind::Magic] {
        assert_eq!(
            Candidate::from_complete(kind, components(), vec![food(0)], budget()),
            Err(Invalid::Kind)
        );
    }
    assert_eq!(
        Candidate::from_complete(Kind::Food, vec![], vec![], budget()),
        Err(Invalid::Components)
    );
}
#[test]
fn each_budget_overflow_is_unavailable_never_positive_prefix() {
    let baseline = make(vec![food(0), food(1)], budget()).unwrap();
    let bytes = baseline.chart().unwrap().encoded_size_bound();
    for limit in [
        Budget {
            components: 0,
            ..budget()
        },
        Budget {
            rows: 1,
            ..budget()
        },
        Budget {
            json_bytes: bytes - 1,
            ..budget()
        },
    ] {
        let value = make(vec![food(0), food(1)], limit).unwrap();
        assert!(value.chart().is_none());
        assert_eq!(value.unavailable_reason(), Some(Unavailable::Budget));
    }
    assert!(make(
        vec![food(0), food(1)],
        Budget {
            components: 1,
            rows: 2,
            json_bytes: bytes
        }
    )
    .unwrap()
    .chart()
    .is_some());
}
#[test]
fn monster_tuple_bound_covers_json_escaping_and_component_row_overhead() {
    let appearance = crate::appearance_catalog::CreatureAppearance {
        name: Arc::from("\\\"é🦇".repeat(20)),
        asset: Some(Arc::from("creature.bat")),
    };
    let row = Row {
        component: 0,
        position: Some([i64::MIN, i64::MAX, -1]),
        mark: Mark::Monster(appearance.clone()),
    };
    let candidate =
        Candidate::from_complete(Kind::Monsters, components(), vec![row], budget()).unwrap();
    let representative = serde_json::json!({"kind":"monsters", "components":[{"alignment":"aligned"}],
        "rows":[{"component":0,"position":[i64::MIN,i64::MAX,-1],"mark":{"kind":"monster",
        "name":appearance.name.as_ref(),"asset":appearance.asset.as_deref(),"seed":0}}]});
    assert!(
        serde_json::to_vec(&representative).unwrap().len()
            <= candidate.chart().unwrap().encoded_size_bound()
    );
    assert!(escaped("\u{0001}").unwrap() >= 8);
    assert!(escaped("🦇").unwrap() >= 14);
}
#[test]
fn monster_tuple_grammar_reuses_existing_cosmetic_contract() {
    for (name, asset, valid) in [
        ("".to_owned(), None, true),
        ("a".repeat(crate::limits::DISPLAY_NAME_BYTES), None, true),
        (
            "a".repeat(crate::limits::DISPLAY_NAME_BYTES + 1),
            None,
            false,
        ),
        ("bad\nname".to_owned(), None, false),
        ("bat".to_owned(), Some("creature.bat"), true),
        ("bat".to_owned(), Some("Creature.bat"), false),
        ("bat".to_owned(), Some("creature..bat"), false),
    ] {
        let appearance = crate::appearance_catalog::CreatureAppearance {
            name: Arc::from(name),
            asset: asset.map(Arc::from),
        };
        let result = Candidate::from_complete(
            Kind::Monsters,
            components(),
            vec![Row {
                component: 0,
                position: Some([0, 0, 0]),
                mark: Mark::Monster(appearance),
            }],
            budget(),
        );
        assert_eq!(result.is_ok(), valid);
    }
}

#[test]
fn duplicate_monster_occurrences_preserve_count_and_charge_every_row() {
    let appearance = crate::appearance_catalog::CreatureAppearance {
        name: Arc::from("bat"),
        asset: Some(Arc::from("creature.bat")),
    };
    let row = Row {
        component: 0,
        position: Some([0, 0, 0]),
        mark: Mark::Monster(appearance),
    };
    let one = Candidate::from_complete(Kind::Monsters, components(), vec![row.clone()], budget())
        .unwrap();
    let two = Candidate::from_complete(
        Kind::Monsters,
        components(),
        vec![row.clone(), row],
        budget(),
    )
    .unwrap();
    assert_eq!(two.chart().unwrap().rows().len(), 2);
    assert!(two.chart().unwrap().encoded_size_bound() > one.chart().unwrap().encoded_size_bound());
}
#[test]
fn alignment_controls_coordinate_presence_without_flattening() {
    let inconsistent = vec![Component {
        alignment: crate::scope_projection::Alignment::Inconsistent,
    }];
    assert_eq!(
        Candidate::from_complete(Kind::Food, inconsistent.clone(), vec![food(0)], budget()),
        Err(Invalid::Position)
    );
    let row = Row {
        component: 0,
        position: None,
        mark: Mark::Food,
    };
    assert_eq!(make(vec![row.clone()], budget()), Err(Invalid::Position));
    let value = Candidate::from_complete(Kind::Food, inconsistent, vec![row], budget()).unwrap();
    assert_eq!(value.chart().unwrap().rows()[0].position, None);
    let row = Row {
        component: 0,
        position: Some([i64::MIN, i64::MAX, -7]),
        mark: Mark::Food,
    };
    let value = make(vec![row.clone()], budget()).unwrap();
    assert_eq!(value.chart().unwrap().rows(), &[row]);
}

#[test]
fn magic_mark_and_disconnected_components_keep_local_three_dimensional_frames() {
    let components = vec![
        Component {
            alignment: crate::scope_projection::Alignment::Aligned,
        },
        Component {
            alignment: crate::scope_projection::Alignment::Aligned,
        },
    ];
    let rows = vec![
        Row {
            component: 0,
            position: Some([4, -2, 8]),
            mark: Mark::Magic,
        },
        Row {
            component: 1,
            position: Some([4, -2, 8]),
            mark: Mark::Magic,
        },
    ];
    let value =
        Candidate::from_complete(Kind::Magic, components.clone(), rows.clone(), budget()).unwrap();
    assert_eq!(value.chart().unwrap().kind(), Kind::Magic);
    assert_eq!(value.chart().unwrap().components(), components);
    assert_eq!(value.chart().unwrap().rows(), rows);
    assert_eq!(
        Candidate::from_complete(Kind::Monsters, components, rows, budget()),
        Err(Invalid::Kind)
    );
}
