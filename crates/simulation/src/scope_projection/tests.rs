use super::*;
use tor_world::{Direction, Extent, Passage, Position, Region};
fn at(r: u64, x: i32, y: i32) -> Location {
    Location {
        region: RegionId(r),
        position: Position { x, y, z: 0 },
    }
}
fn world() -> World {
    World::new(
        (1..=3)
            .map(|id| Region {
                id: RegionId(id),
                name: format!("r{id}"),
                bounds: Extent::new(2, 2, 1).unwrap(),
            })
            .collect(),
        vec![],
    )
    .unwrap()
}
fn budget() -> Budget {
    Budget {
        work: 10_000,
        allocation_bytes: 100_000,
        regions: 3,
        directed_constraints: 20,
        samples: 20,
    }
}
fn members() -> [RegionId; 3] {
    [RegionId(1), RegionId(2), RegionId(3)]
}
#[test]
fn rotated_join_uses_inverse_crossing_and_one_cell_step() {
    let mut w = world();
    w.connect(
        Passage {
            from: at(1, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 0),
        },
        1,
    )
    .unwrap();
    let p = project(
        &w,
        &members(),
        &[at(1, 1, 0), at(2, 0, 0), at(2, 0, 1), at(3, 0, 0)],
        budget(),
    )
    .unwrap();
    assert_eq!(
        p.components,
        [Component {
            alignment: Alignment::Aligned
        }; 2]
    );
    assert_eq!(
        p.samples,
        [
            Sample {
                component: 0,
                position: Some([1, 0, 0])
            },
            Sample {
                component: 0,
                position: Some([2, 0, 0])
            },
            Sample {
                component: 0,
                position: Some([3, 0, 0])
            },
            Sample {
                component: 1,
                position: Some([0, 0, 0])
            },
        ]
    );
}
#[test]
fn disconnected_local_origins_do_not_claim_common_positions() {
    let p = project(
        &world(),
        &members(),
        &[at(1, 0, 0), at(2, 0, 0), at(3, 0, 0)],
        budget(),
    )
    .unwrap();
    assert_eq!(p.components.len(), 3);
    assert_eq!(
        p.samples.iter().map(|s| s.component).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(p.samples.iter().all(|s| s.position == Some([0, 0, 0])));
}
#[test]
fn contradictory_cycle_invalidates_entire_component_not_only_last_edge() {
    let mut w = world();
    w.connect(
        Passage {
            from: at(1, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 0),
        },
        0,
    )
    .unwrap();
    w.connect(
        Passage {
            from: at(2, 0, 0),
            direction: Direction::West,
            to: at(1, 0, 0),
        },
        0,
    )
    .unwrap();
    let p = project(
        &w,
        &members(),
        &[at(1, 0, 0), at(2, 0, 0), at(3, 0, 0)],
        budget(),
    )
    .unwrap();
    assert_eq!(p.components[0].alignment, Alignment::Inconsistent);
    assert_eq!(p.samples[0].position, None);
    assert_eq!(p.samples[1].position, None);
    assert_eq!(
        p.samples[2],
        Sample {
            component: 1,
            position: Some([0, 0, 0])
        }
    );
}
#[test]
fn transport_stair_is_not_a_physical_alignment_constraint() {
    let mut w = world();
    w.connect(
        Passage {
            from: at(1, 0, 0),
            direction: Direction::Up,
            to: at(2, 0, 0),
        },
        0,
    )
    .unwrap();
    let p = project(&w, &members(), &[at(1, 0, 0), at(2, 0, 0)], budget()).unwrap();
    assert_ne!(p.samples[0].component, p.samples[1].component);
}
#[test]
fn budget_failures_are_atomic_and_distinct_from_empty_chart() {
    let w = world();
    let mut b = budget();
    b.work = 0;
    assert!(matches!(
        project(&w, &members(), &[], b),
        Err(Error::Budget)
    ));
    let mut b = budget();
    b.allocation_bytes = 0;
    assert!(matches!(
        project(&w, &members(), &[], b),
        Err(Error::Budget)
    ));
    let mut w = world();
    w.connect(
        Passage {
            from: at(1, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 0),
        },
        0,
    )
    .unwrap();
    let mut b = budget();
    b.directed_constraints = 1;
    assert!(matches!(
        project(&w, &members(), &[], b),
        Err(Error::Budget)
    ));
    assert!(project(&w, &members(), &[], budget())
        .unwrap()
        .samples
        .is_empty());
}
#[test]
fn checked_arithmetic_never_wraps_into_plausible_position() {
    assert_eq!(rotate(2, [i64::MIN, 0, 0]), Err(Error::Arithmetic));
    assert_eq!(
        Frame {
            rotation: 0,
            translation: [i64::MAX, 0, 0]
        }
        .apply([1, 0, 0]),
        Err(Error::Arithmetic)
    );
}

#[test]
fn rotated_physical_vertical_aperture_aligns_in_three_dimensions() {
    let mut w = world();
    // Rotation 1 preserves z, maps source +x to destination +y.
    w.connect_portal_area(
        Passage {
            from: at(1, 0, 0),
            direction: Direction::Up,
            to: at(2, 0, 0),
        },
        1,
        2,
        1,
    )
    .unwrap();
    assert!(!w.is_stair(at(1, 0, 0), Direction::Up));
    assert_eq!(
        w.physics_neighbor(at(1, 1, 0), Direction::Up),
        Some((at(2, 0, 1), 1))
    );
    let p = project(
        &w,
        &members(),
        &[at(1, 0, 0), at(2, 0, 0), at(2, 0, 1)],
        budget(),
    )
    .unwrap();
    assert_eq!(
        p.samples,
        [
            Sample {
                component: 0,
                position: Some([0, 0, 0])
            },
            Sample {
                component: 0,
                position: Some([0, 0, 1])
            },
            Sample {
                component: 0,
                position: Some([1, 0, 1])
            },
        ]
    );
}

#[test]
fn solid_boundary_constraint_does_not_depend_on_walkability() {
    let mut w = world();
    let from = at(1, 1, 0);
    let to = at(2, 0, 0);
    w.set_wall(from, true).unwrap();
    w.set_wall(to, true).unwrap();
    w.connect_boundary_area(
        Passage {
            from,
            direction: Direction::East,
            to,
        },
        0,
        1,
        1,
    )
    .unwrap();
    assert_eq!(w.physics_neighbor(from, Direction::East), None);
    let before = project(&w, &members(), &[from, to], budget()).unwrap();
    assert_eq!(
        before.samples[1],
        Sample {
            component: 0,
            position: Some([2, 0, 0])
        }
    );
    w.set_wall(from, false).unwrap();
    w.set_wall(to, false).unwrap();
    assert_eq!(w.physics_neighbor(from, Direction::East), Some((to, 0)));
    assert_eq!(
        before,
        project(&w, &members(), &[from, to], budget()).unwrap()
    );
}

#[test]
fn resolved_named_stair_is_not_an_alignment_constraint() {
    let mut w = world();
    w.register_named_anchors(
        RegionId(2),
        std::collections::BTreeMap::from([("landing".into(), at(2, 0, 0).position)]),
    )
    .unwrap();
    w.connect_named_stair(
        at(1, 0, 0),
        Direction::Down,
        tor_world::NamedAnchor {
            region: RegionId(2),
            name: "landing".into(),
        },
    )
    .unwrap();
    assert_eq!(
        w.movement_neighbor(at(1, 0, 0), Direction::Down),
        Some((at(2, 0, 0), 0))
    );
    assert_eq!(w.exits(RegionId(1)).count(), 0);
    let p = project(&w, &members(), &[at(1, 0, 0), at(2, 0, 0)], budget()).unwrap();
    assert_ne!(p.samples[0].component, p.samples[1].component);
}

#[test]
fn external_scope_region_cannot_bridge_disconnected_members() {
    let mut w = world();
    w.connect(
        Passage {
            from: at(1, 1, 0),
            direction: Direction::East,
            to: at(3, 0, 0),
        },
        0,
    )
    .unwrap();
    w.connect(
        Passage {
            from: at(3, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 0),
        },
        0,
    )
    .unwrap();
    let all = project(&w, &members(), &[at(1, 0, 0), at(2, 0, 0)], budget()).unwrap();
    assert_eq!(all.samples[0].component, all.samples[1].component);
    assert_eq!(all.samples[1].position, Some([4, 0, 0]));
    let subset = project(
        &w,
        &[RegionId(1), RegionId(2)],
        &[at(1, 0, 0), at(2, 0, 0)],
        budget(),
    )
    .unwrap();
    assert_ne!(subset.samples[0].component, subset.samples[1].component);
    assert_eq!(subset.samples[1].position, Some([0, 0, 0]));
}

#[test]
fn canonical_member_order_and_point_containment_are_required() {
    let w = world();
    for input in [
        vec![RegionId(1), RegionId(1)],
        vec![RegionId(2), RegionId(1)],
        vec![RegionId(4)],
    ] {
        assert_eq!(
            project(&w, &input, &[], budget()),
            Err(Error::InvalidSource)
        );
    }
    assert_eq!(
        project(&w, &[RegionId(1)], &[at(2, 0, 0)], budget()),
        Err(Error::InvalidSource)
    );
    assert_eq!(
        project(&w, &members(), &[at(1, 2, 0)], budget()),
        Err(Error::InvalidSource)
    );
}

#[test]
fn duplicate_occurrences_and_coincident_positions_are_preserved() {
    let mut w = world();
    for to in [2, 3] {
        w.connect(
            Passage {
                from: at(to, 0, 0),
                direction: Direction::West,
                to: at(1, 1, 0),
            },
            0,
        )
        .unwrap();
    }
    let p = project(
        &w,
        &members(),
        &[at(2, 0, 0), at(3, 0, 0), at(2, 0, 0)],
        budget(),
    )
    .unwrap();
    assert_eq!(
        p.components,
        [Component {
            alignment: Alignment::Aligned
        }]
    );
    assert_eq!(
        p.samples,
        [Sample {
            component: 0,
            position: Some([2, 0, 0])
        }; 3]
    );
}

#[test]
fn insertion_order_does_not_change_projection() {
    let links = [
        Passage {
            from: at(1, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 0),
        },
        Passage {
            from: at(2, 1, 0),
            direction: Direction::East,
            to: at(3, 0, 0),
        },
    ];
    let mut a = world();
    let mut b = world();
    for p in links {
        a.connect(p, 0).unwrap();
    }
    for p in links.into_iter().rev() {
        b.connect(p, 0).unwrap();
    }
    let points = [at(1, 0, 0), at(2, 0, 0), at(3, 0, 0)];
    assert_eq!(
        project(&a, &members(), &points, budget()).unwrap(),
        project(&b, &members(), &points, budget()).unwrap()
    );
}

#[test]
fn empty_single_member_and_explicit_cardinality_caps() {
    let w = world();
    assert_eq!(
        project(&w, &[], &[], budget()).unwrap(),
        Projection {
            usage: Usage {
                work: 1,
                allocation_bytes: std::mem::size_of::<usize>()
            },
            components: vec![],
            samples: vec![]
        }
    );
    assert_eq!(
        project(&w, &[RegionId(2)], &[at(2, 1, 1)], budget())
            .unwrap()
            .samples,
        [Sample {
            component: 0,
            position: Some([1, 1, 0])
        }]
    );
    let mut b = budget();
    b.regions = 2;
    assert_eq!(project(&w, &members(), &[], b), Err(Error::Budget));
    let mut b = budget();
    b.samples = 0;
    assert_eq!(
        project(&w, &members(), &[at(1, 0, 0)], b),
        Err(Error::Budget)
    );
}

#[test]
fn all_cube_rotations_preserve_checked_frame_inverse() {
    for rotation in 0..24 {
        let frame = Frame {
            rotation,
            translation: [17, -29, 41],
        };
        let point = [-7, 13, 23];
        assert_eq!(
            frame
                .inverse()
                .unwrap()
                .apply(frame.apply(point).unwrap())
                .unwrap(),
            point
        );
        assert_eq!(
            frame.after(frame.inverse().unwrap()).unwrap(),
            Frame {
                rotation: 0,
                translation: [0; 3]
            }
        );
    }
}

#[test]
fn detached_external_destination_is_not_required_for_local_projection() {
    let mut w = world();
    w.connect(
        Passage {
            from: at(1, 1, 0),
            direction: Direction::East,
            to: at(3, 0, 0),
        },
        0,
    )
    .unwrap();
    w.detach_region(RegionId(3)).unwrap();
    assert!(!w.contains(at(3, 0, 0)));
    // The outgoing passage survives independently of destination residency.
    assert!(w.passage(at(1, 1, 0), Direction::East).is_some());
    let p = project(&w, &[RegionId(1)], &[at(1, 1, 0)], budget()).unwrap();
    assert_eq!(
        p.samples,
        [Sample {
            component: 0,
            position: Some([1, 0, 0])
        }]
    );
}

// Staged additions only. Append to scope_projection/tests.rs after source freeze.
#[test]
fn noncommuting_crossings_compose_in_physical_path_order() {
    let mut w = world();
    w.connect(
        Passage {
            from: at(1, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 0),
        },
        1,
    )
    .unwrap();
    w.connect(
        Passage {
            from: at(2, 1, 0),
            direction: Direction::East,
            to: at(3, 0, 0),
        },
        8,
    )
    .unwrap();
    let p = project(
        &w,
        &members(),
        &[at(2, 0, 0), at(3, 0, 0), at(3, 0, 1)],
        budget(),
    )
    .unwrap();
    // Crossing 1: (x,y,z)->(-y,x,z); crossing 8: (x,y,z)->(x,z,-y).
    // Inverse 8 followed by inverse 1 maps C +y to A +z.
    // The two one-cell seam anchors place B origin at (2,0,0), C at (2,-2,0).
    // Swapping composition order instead maps C +y to A +x.
    assert_eq!(
        p.components,
        [Component {
            alignment: Alignment::Aligned
        }]
    );
    assert_eq!(
        p.samples,
        [
            Sample {
                component: 0,
                position: Some([2, 0, 0])
            },
            Sample {
                component: 0,
                position: Some([2, -2, 0])
            },
            Sample {
                component: 0,
                position: Some([2, -2, 1])
            },
        ]
    );
}

#[test]
fn rotation_only_cycle_conflict_rejects_matching_origin_proposals() {
    let fixture = |reverse_rotation| {
        let mut w = world();
        w.connect(
            Passage {
                from: at(1, 1, 0),
                direction: Direction::East,
                to: at(2, 0, 0),
            },
            0,
        )
        .unwrap();
        w.connect(
            Passage {
                from: at(2, 0, 0),
                direction: Direction::West,
                to: at(1, 1, 0),
            },
            reverse_rotation,
        )
        .unwrap();
        w
    };
    let points = [at(1, 0, 0), at(2, 0, 0), at(3, 0, 0)];
    let good = project(&fixture(0), &members(), &points, budget()).unwrap();
    assert_eq!(good.samples[0].position, Some([0, 0, 0]));
    assert_eq!(good.samples[1].position, Some([2, 0, 0]));
    assert_eq!(good.components[0].alignment, Alignment::Aligned);
    // Rotation 8 preserves x. The backward link therefore proposes exactly
    // the same region-1 origin (0,0,0), but disagrees on its y/z axes.
    let bad = project(&fixture(8), &members(), &points, budget()).unwrap();
    assert_eq!(bad.components[0].alignment, Alignment::Inconsistent);
    assert_eq!(bad.samples[0].position, None);
    assert_eq!(bad.samples[1].position, None);
    assert_eq!(
        bad.samples[2],
        Sample {
            component: 1,
            position: Some([0, 0, 0])
        }
    );
}

#[test]
fn exact_work_and_logical_allocation_boundaries_admit_then_reject_one_less() {
    let mut w = world();
    w.connect(
        Passage {
            from: at(1, 1, 0),
            direction: Direction::East,
            to: at(2, 0, 0),
        },
        0,
    )
    .unwrap();
    let members = [RegionId(1), RegionId(2)];
    let points = [at(1, 0, 0), at(2, 0, 0)];
    // Two members, one retained passage/two directed constraints, two samples.
    // Named backing stores: 2 Edges; 3 offsets + 2 cursors + 2 order indexes +
    // 2 groups + 2 queue entries = 11 usize; 2 frames/components/samples each.
    // This is a target-layout expression, not allocator resident-byte precision.
    let bytes = 2 * std::mem::size_of::<Edge>()
        + 11 * std::mem::size_of::<usize>()
        + 2 * std::mem::size_of::<Option<Frame>>()
        + 2 * std::mem::size_of::<Component>()
        + 2 * std::mem::size_of::<Sample>();
    // Independent hand ledger: validation 2; both candidate passes 6 each;
    // reserve initialization slots 19 (2+3+2+2+2+2+2+2+2);
    // adjacency construction loops 6; seed/queue/edge traversal 6;
    // sample validation+binary lookup 8. Total 2+12+19+6+6+8 = 53.
    let exact = Budget {
        work: 53,
        allocation_bytes: bytes,
        regions: 2,
        directed_constraints: 2,
        samples: 2,
    };
    let projected = project(&w, &members, &points, exact).unwrap();
    assert_eq!(
        projected.samples,
        [
            Sample {
                component: 0,
                position: Some([0, 0, 0])
            },
            Sample {
                component: 0,
                position: Some([2, 0, 0])
            },
        ]
    );
    assert_eq!(
        project(&w, &members, &points, Budget { work: 52, ..exact }),
        Err(Error::Budget)
    );
    assert_eq!(
        project(
            &w,
            &members,
            &points,
            Budget {
                allocation_bytes: bytes - 1,
                ..exact
            }
        ),
        Err(Error::Budget)
    );
}
