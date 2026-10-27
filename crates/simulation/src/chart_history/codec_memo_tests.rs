use super::*;

fn captured(count: usize, distinct_world: bool) -> LocalHistory {
    let original = super::super::mapping_tests::root();
    let mut owners = BTreeMap::new();
    for i in 0..count {
        let mut root = (*original).clone();
        root.acquired_tick = i as u64;
        if distinct_world && i == 1 {
            let mut authority = (*root.authority).clone();
            authority
                .world
                .set_gravity(tor_world::RegionId(1), [0, 0, -1])
                .unwrap();
            root.authority = Shared::new(authority);
        }
        let mut owner = Owner::default();
        owner
            .record_attempt(
                Kind::Mapping,
                Attempt {
                    attempted_at: i as u64,
                    status: AttemptStatus::Positive,
                },
                Some(Shared::new(root)),
            )
            .unwrap();
        owners.insert(ActorId(i as u64 + 1), owner);
    }
    let bytes = serde_json::to_vec(&LocalHistory::capture(&owners)).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}
fn restored(
    local: &LocalHistory,
    cache: &mut RestoreCache,
) -> Result<Shared<BTreeMap<ActorId, Owner>>, Error> {
    local.restore_with_cache(cache)
}
#[test]
fn repeated_mapping_authority_replays_once_but_preserves_each_acquisition_and_full_proof() {
    let local = captured(300, false);
    assert_eq!(local.history.roots.len(), 300);
    let mut cache = RestoreCache::default();
    let owners = restored(&local, &mut cache).unwrap();
    assert_eq!(cache.mapping_replays, 1);
    assert_eq!(cache.mapping.len(), 1);
    assert_eq!(owners.len(), 300);
    let first = owners[&ActorId(1)].mapping.latest.as_ref().unwrap();
    for (&id, owner) in owners.iter() {
        let root = owner.mapping.latest.as_ref().unwrap();
        assert_eq!(root.acquired_tick, id.0 - 1);
        assert_eq!(owner.mapping.attempt.unwrap().attempted_at, id.0 - 1);
        assert!(Arc::ptr_eq(&first.chart, &root.chart));
        assert!(Arc::ptr_eq(&first.source, &root.source));
    }
    let mut no_memo = RestoreCache {
        memo_work_left: 0,
        ..RestoreCache::default()
    };
    let uncached = restored(&local, &mut no_memo).unwrap();
    assert_eq!(no_memo.mapping_replays, 300);
    assert!(no_memo.mapping.is_empty());
    assert_eq!(owners, uncached);
}
#[test]
fn repeated_authority_never_licenses_changed_rows_or_private_witnesses() {
    for mutation in 0..5 {
        let mut local = captured(2, false);
        // Establish lawful original before the selected negative mutation.
        assert!(restored(&local, &mut RestoreCache::default()).is_ok());
        let root = &mut local.history.roots[1];
        match mutation {
            0 => root.points[0].mapping.as_mut().unwrap().normal = Some([0, 0, 1]),
            1 => root.points[0].mapping.as_mut().unwrap().toward = Some(tor_world::Direction::Up),
            2 => {
                root.points.pop();
            }
            3 => root.points[0].component = usize::MAX,
            4 => root.points[0].position = Some([999, 999, 999]),
            _ => unreachable!(),
        }
        let mut cache = RestoreCache::default();
        assert!(matches!(
            restored(&local, &mut cache),
            Err(Error::Projection)
        ));
        assert_eq!(cache.mapping_replays, 1);
        assert_eq!(cache.roots.len(), 1, "failed sample was never memoized");
    }
}
#[test]
fn mapping_memo_is_exact_world_identity_and_restore_namespace_local() {
    let first = captured(2, true);
    assert_eq!(first.worlds.len(), 2);
    let mut a = RestoreCache::default();
    assert!(restored(&first, &mut a).is_ok());
    assert_eq!(a.mapping_replays, 2);
    let second = captured(2, false);
    let mut b = RestoreCache::default();
    assert!(restored(&second, &mut b).is_ok());
    assert_eq!(
        b.mapping_replays, 1,
        "fresh local pool establishes its own authority"
    );
}

/// Mandatory correctness/count diagnostic; parallel suite timing is not performance evidence.
#[test]
fn measure_repeated_mapping_restore_replay_cost() {
    let repetitions = if cfg!(debug_assertions) { 1 } else { 5 };
    for count in [1, 32, 300, 1024] {
        let mut local = captured(count, false);
        let refs: Vec<_> = local.history.spatial_indices().collect();
        local
            .spatial
            .prepare_serialization(refs.into_iter())
            .unwrap();
        let bytes = serde_json::to_vec(&local).unwrap().len();
        for enabled in [false, true] {
            let mut elapsed = Vec::new();
            for _ in 0..repetitions {
                let mut cache = RestoreCache::default();
                if !enabled {
                    cache.memo_work_left = 0;
                }
                let start = std::time::Instant::now();
                let result = restored(&local, &mut cache).unwrap();
                elapsed.push(start.elapsed().as_nanos());
                assert_eq!(result.len(), count);
                assert_eq!(cache.mapping_replays, if enabled { 1 } else { count });
            }
            elapsed.sort_unstable();
            eprintln!("mapping_replay roots={count} rows_per_root=16 bytes={bytes} memo={enabled} n={repetitions} p50_ns={} max_ns={}",elapsed[repetitions/2],elapsed[repetitions-1]);
        }
    }
}

#[test]
fn memo_hit_still_rejects_bad_owner_time_and_wrong_kind_after_valid_first_root() {
    for mutation in 0..3 {
        let mut local = captured(2, false);
        assert!(restored(&local, &mut RestoreCache::default()).is_ok());
        // Call pool restore directly after a valid pool setup to exercise its
        // final per-owner checks, not just the earlier serialized structure gate.
        match mutation {
            0 => {
                local.history.owners[local.owners][1]
                    .slots
                    .mapping
                    .attempt
                    .as_mut()
                    .unwrap()
                    .attempted_at = 0
            }
            1 => {
                let slots = &mut local.history.owners[local.owners][1].slots;
                slots.food = slots.mapping;
                slots.mapping = SlotRef::default();
            }
            2 => local.history.roots[1].acquired_tick = 2,
            _ => unreachable!(),
        }
        let refs: Vec<_> = local.history.spatial_indices().collect();
        let spatial = local.spatial.bind(refs.into_iter()).unwrap();
        let worlds: Vec<_> = local.worlds.iter().cloned().map(Shared::new).collect();
        let mut cache = RestoreCache::default();
        let result = local.history.restore(
            local.owners,
            1,
            |_| true,
            &mut cache,
            &mut BTreeSet::new(),
            |w, s| {
                Ok(AcquisitionAuthority {
                    world: worlds[w].clone(),
                    registry: spatial.registry(s).unwrap(),
                })
            },
        );
        assert!(
            matches!(result, Err(Error::Reference)),
            "mutation {mutation}"
        );
        assert_eq!(
            cache.mapping_replays, 1,
            "same authority was actually reused"
        );
    }
}

fn large_captured(count: usize, distinct_worlds: bool) -> LocalHistory {
    use crate::spatial_scope::mapping::tests::{box_cells, chamber_world, frame, source_scope};
    let world = chamber_world(1, [32, 32, 8]);
    let mut scope = source_scope(
        &world,
        &[(1, box_cells(1, [0, 0, 0], [31, 31, 7]))],
        &[],
        &[],
    );
    let bytes = crate::checkpoint::ARCHIVE_BYTE_LIMIT;
    scope.mapping = crate::spatial_scope::mapping::certify(
        &world,
        &scope,
        &[frame(1, 1, tor_world::Direction::Up)],
        &[],
        crate::spatial_scope::mapping::CertificateBudget {
            work: bytes,
            allocation_bytes: bytes,
            cache_bytes: bytes as u64,
        },
    )
    .expect("lawful complete large certificate");
    let prepared = crate::mapping_chart::prepare_scope_mapping(
        &world,
        &scope,
        crate::detection::CensusBudget::canonical(),
    );
    let chart = prepared
        .candidate
        .shared_chart()
        .expect("large certified shape within actual budget")
        .clone();
    // Four side faces, width32 and height8; no floor/ceiling marks.
    assert_eq!(chart.rows().len(), 1024);
    let mut registry = Registry::default();
    registry.register_archive_checked(scope.clone()).unwrap();
    let original = HistoricalChart::checked_mapping(
        0,
        scope.id,
        Shared::new(AcquisitionAuthority {
            world: Shared::new(world),
            registry: Shared::new(registry),
        }),
        chart,
        prepared.source_rows,
        archive_budget(),
    )
    .unwrap();
    let mut owners = BTreeMap::new();
    for i in 0..count {
        let mut root = original.clone();
        root.acquired_tick = i as u64;
        if distinct_worlds {
            let mut authority = (*root.authority).clone();
            // Lawful illumination-only distinction: mapping geometry unchanged.
            authority
                .world
                .set_cell_light(
                    Location {
                        region: tor_world::RegionId(1),
                        position: tor_world::Position {
                            x: (i % 32) as i32,
                            y: (i / 32) as i32,
                            z: 0,
                        },
                    },
                    false,
                )
                .unwrap();
            root.authority = Shared::new(authority);
        }
        let mut owner = Owner::default();
        owner
            .record_attempt(
                Kind::Mapping,
                Attempt {
                    attempted_at: i as u64,
                    status: AttemptStatus::Positive,
                },
                Some(Shared::new(root)),
            )
            .unwrap();
        owners.insert(ActorId(i as u64 + 1), owner);
    }
    let bytes = serde_json::to_vec(&LocalHistory::capture(&owners)).unwrap();
    assert!(bytes.len() < crate::checkpoint::ARCHIVE_BYTE_LIMIT);
    serde_json::from_slice(&bytes).unwrap()
}
#[test]
fn large_certified_mapping_complete_detached_restore_matches_uncached_and_rejects_late_corruption()
{
    let mut local = large_captured(8, false);
    let mut memo = RestoreCache::default();
    let actual = restored(&local, &mut memo).unwrap();
    let mut reference = RestoreCache {
        memo_work_left: 0,
        ..RestoreCache::default()
    };
    assert_eq!(actual, restored(&local, &mut reference).unwrap());
    assert_eq!((memo.mapping_replays, reference.mapping_replays), (1, 8));
    local.history.roots[7].points[1000].position = Some([999, 999, 999]);
    assert!(matches!(
        restored(&local, &mut RestoreCache::default()),
        Err(Error::Projection)
    ));
    let distinct = large_captured(4, true);
    assert_eq!(distinct.worlds.len(), 4);
    let mut cache = RestoreCache::default();
    assert_eq!(restored(&distinct, &mut cache).unwrap().len(), 4);
    assert_eq!(cache.mapping_replays, 4);
}
#[test]
fn measure_large_mapping_complete_detached_restore() {
    let repetitions = if cfg!(debug_assertions) { 1 } else { 5 };
    for (count, distinct) in [(1, false), (8, false), (32, false), (8, true)] {
        let mut local = large_captured(count, distinct);
        let refs: Vec<_> = local.history.spatial_indices().collect();
        local
            .spatial
            .prepare_serialization(refs.into_iter())
            .unwrap();
        let bytes = serde_json::to_vec(&local).unwrap();
        let mut expected = None;
        for enabled in [false, true] {
            let mut times = Vec::new();
            for _ in 0..repetitions {
                let start = std::time::Instant::now();
                let decoded: LocalHistory = serde_json::from_slice(&bytes).unwrap();
                let mut cache = RestoreCache::default();
                if !enabled {
                    cache.memo_work_left = 0;
                }
                let actual = restored(&decoded, &mut cache).unwrap();
                times.push(start.elapsed().as_nanos());
                if let Some(ref expected) = expected {
                    assert_eq!(&actual, expected);
                } else {
                    expected = Some(actual);
                }
                assert_eq!(
                    cache.mapping_replays,
                    if enabled && !distinct { 1 } else { count }
                );
            }
            times.sort_unstable();
            eprintln!("mapping_full_detached shape=32x32x8 rows_per_root=1024 roots={count} distinct_worlds={distinct} encoded_bytes={} memo={enabled} n={repetitions} p50_ns={} max_ns={}",bytes.len(),times[repetitions/2],times[repetitions-1]);
        }
    }
}

#[test]
fn memo_work_exhaustion_after_first_hit_preserves_exact_uncached_result() {
    let local = captured(32, false);
    let mut partial = RestoreCache {
        memo_work_left: 100,
        ..RestoreCache::default()
    };
    let actual = restored(&local, &mut partial).unwrap();
    assert_eq!(partial.memo_work_left, 0);
    assert_eq!(
        partial.mapping.len(),
        1,
        "first representative was inserted"
    );
    assert!(
        partial.mapping_replays > 1 && partial.mapping_replays < 32,
        "at least one hit then actual fallback"
    );
    let mut disabled = RestoreCache {
        memo_work_left: 0,
        ..RestoreCache::default()
    };
    assert_eq!(actual, restored(&local, &mut disabled).unwrap());
}
