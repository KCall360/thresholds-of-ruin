use crate::spatial_scope::registry::{Budget, CellRun, Error, PrimaryRun, RoomSurfaces, Scope};
use crate::spatial_scope::{Membership, PrimarySpace, RoomId};
use crate::{
    checkpoint::{RestoreContext, SharedState},
    Game,
};
use tor_world::{Location, Position, RegionId};
fn scope(region: u64) -> Scope {
    let room = RoomId::new(1).unwrap();
    let cells = CellRun {
        region: RegionId(region),
        z: 0,
        y: 1,
        first_x: 1,
        last_x: 1,
    };
    Scope {
        mapping: crate::spatial_scope::mapping::MappingCertification::Uncertified,
        id: format!("fixture/{region}"),
        members: vec![RegionId(region)],
        primary: vec![PrimaryRun {
            cells,
            membership: Membership::new(PrimarySpace::Room(room), None).unwrap(),
        }],
        rooms: vec![RoomSurfaces {
            room,
            runs: vec![cells],
        }],
    }
}
fn budget() -> Budget {
    Budget {
        scopes: 2,
        members: 2,
        rooms: 2,
        passages: 0,
        runs: 4,
        expanded_refs: 4,
        label_bytes: 32,
    }
}
fn at(region: u64) -> Location {
    Location {
        region: RegionId(region),
        position: Position { x: 1, y: 1, z: 0 },
    }
}
fn fixture() -> Game {
    // A restorable Game requires a live or detached actor; the two-room helper
    // supplies geometry only. Preserve the ordinary scheduler contract.
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
    assert!(game.world.walkable(at(1)));
    let actor = game
        .spawn_actor(at(1), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    assert_eq!(game.next_actor(), Some(actor));
    assert_eq!(game.actors[&actor].ready_at, game.tick);
    game
}
#[test]
fn immutable_spatial_game_roots_keep_old_subset_and_failed_install_unchanged() {
    let mut game = fixture();
    assert!(game.world.contains(at(1)));
    assert!(game.world.contains(at(2)));
    assert!(scope(1).validate(&budget()).is_ok());
    assert!(scope(2).validate(&budget()).is_ok());
    game.register_spatial_scope(scope(1), &budget()).unwrap();
    assert!(game.spatial_registry().lookup("fixture/1", at(1)).is_some());
    let old = game.clone();
    game.register_spatial_scope(scope(2), &budget()).unwrap();
    assert!(old.spatial_registry().lookup("fixture/2", at(2)).is_none());
    assert!(game.spatial_registry().lookup("fixture/2", at(2)).is_some());
    let original = game.clone();
    let mut alias = scope(1);
    alias.id = "alias".into();
    assert_eq!(
        game.register_spatial_scope(alias, &budget()),
        Err(Error::Conflict)
    );
    assert_eq!(game, original);
    assert_eq!(
        game.register_spatial_scope(
            scope(1),
            &Budget {
                runs: 3,
                ..budget()
            }
        ),
        Err(Error::Capacity)
    );
    assert_eq!(game, original);
}
#[test]
fn checkpoint_pools_shared_scope_payload_once_and_restores_all_subset_roots() {
    let mut old = fixture();
    assert!(old.world.contains(at(1)));
    assert!(old.world.contains(at(2)));
    old.register_spatial_scope(scope(1), &budget()).unwrap();
    let mut current = old.clone();
    current.register_spatial_scope(scope(2), &budget()).unwrap();
    let mut shared = SharedState::default();
    let before = old.checkpoint(&mut shared);
    let after = current.checkpoint(&mut shared);
    let encoded = serde_json::to_value(&shared).unwrap();
    assert_eq!(
        encoded
            .get("spatial")
            .and_then(|v| v.get("scopes"))
            .and_then(|v| v.as_array())
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        encoded["spatial"]["directories"].as_array().unwrap().len(),
        2
    );
    let decoded: SharedState = serde_json::from_value(encoded).unwrap();
    let mut restore = RestoreContext::new(&decoded, current.content().clone());
    restore.bind_spatial_roots(&[&before, &after]).unwrap();
    let earlier = restore.restore(before).unwrap();
    let later = restore.restore(after).unwrap();
    assert_eq!(earlier, old);
    assert_eq!(later, current);
    assert!(earlier
        .spatial_registry()
        .lookup("fixture/2", at(2))
        .is_none());
    // Private payload iterator used by capture can assert actual Shared identity;
    // no public mutable scope access or allocation identity gameplay dependency.
    let first = earlier.spatial_registry().payloads().next().unwrap();
    let second = later.spatial_registry().payloads().next().unwrap();
    assert!(first.shares_storage(second));
}
// Next same-cohort negatives after exact API settles: required spatial root
// omission, out-of-range/current+historical index, deferred final reserve excess,
// checked serializer conflict/overflow and no complete archive publication.
#[test]
fn archive_root_envelope_accepts_257_and_serialization_rejects_258() {
    let game = fixture();
    let mut shared = SharedState::default();
    let snapshots = (0..257)
        .map(|_| game.checkpoint(&mut shared))
        .collect::<Vec<_>>();
    assert_eq!(snapshots.len(), 257);
    let bytes =
        serde_json::to_vec(&(snapshots, &shared)).expect("current+256 retained archive envelope");
    let (snapshots, decoded): (Vec<crate::checkpoint::Snapshot>, SharedState) =
        serde_json::from_slice(&bytes).unwrap();
    let mut context = RestoreContext::new(&decoded, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    for snapshot in snapshots {
        assert_eq!(context.restore(snapshot).unwrap(), game);
    }
    // Trusted capture stays infallible; failed serialization produces no complete
    // archive. Server storage-transaction atomicity remains its own caller proof.
    let _extra = game.checkpoint(&mut shared);
    assert!(serde_json::to_vec(&shared).is_err());
}

#[test]
fn required_spatial_root_and_dangling_payload_fail_before_game_restore() {
    let mut game = fixture();
    game.register_spatial_scope(scope(1), &budget()).unwrap();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let named = serde_json::to_value(&snapshot).unwrap();
    let pooled = serde_json::to_value(&shared).unwrap();
    let mut good = RestoreContext::new(&shared, game.content().clone());
    good.bind_spatial_roots(&[&snapshot]).unwrap();
    assert_eq!(good.restore(snapshot).unwrap(), game);
    let mut missing = named.clone();
    missing.as_object_mut().unwrap().remove("spatial");
    assert!(serde_json::from_value::<crate::checkpoint::Snapshot>(missing).is_err());
    for invalid in [
        serde_json::json!([]),
        serde_json::json!("0"),
        serde_json::json!(u64::MAX),
    ] {
        let mut bad = named.clone();
        bad["spatial"] = invalid;
        assert!(serde_json::from_value::<crate::checkpoint::Snapshot>(bad).is_err());
    }
    let mut dangling = named.clone();
    dangling["spatial"] = serde_json::json!(u32::MAX);
    let bad = serde_json::from_value(dangling).unwrap();
    let mut context = RestoreContext::new(&shared, game.content().clone());
    assert!(context.bind_spatial_roots(&[&bad]).is_err());
    let mut missing_payload = pooled;
    missing_payload["spatial"]["scopes"] = serde_json::json!([]);
    let bad_shared: SharedState = serde_json::from_value(missing_payload).unwrap();
    let snapshot = serde_json::from_value(named).unwrap();
    let mut context = RestoreContext::new(&bad_shared, game.content().clone());
    assert!(context.bind_spatial_roots(&[&snapshot]).is_err());
}
#[test]
fn empty_decoded_pool_has_no_unbound_restore_or_encoding_exemption() {
    let game = fixture();
    let mut shared = SharedState::default();
    let snapshot = game.checkpoint(&mut shared);
    let decoded: SharedState =
        serde_json::from_value(serde_json::to_value(&shared).unwrap()).unwrap();
    let named = serde_json::to_value(snapshot).unwrap();
    let mut unbound = RestoreContext::new(&decoded, game.content().clone());
    assert!(unbound
        .restore(serde_json::from_value(named.clone()).unwrap())
        .is_err());
    assert!(serde_json::to_vec(&decoded).is_err());
    let snapshot = serde_json::from_value(named).unwrap();
    let mut bound = RestoreContext::new(&decoded, game.content().clone());
    bound.bind_spatial_roots(&[&snapshot]).unwrap();
    assert_eq!(bound.restore(snapshot).unwrap(), game);
}
#[test]
fn conflicting_historical_capture_is_a_serializer_failure_not_partial_authority() {
    let mut first = fixture();
    let mut second = first.clone();
    first.register_spatial_scope(scope(1), &budget()).unwrap();
    let mut alternative = scope(1);
    alternative.id = "alternative".into();
    second
        .register_spatial_scope(alternative, &budget())
        .unwrap();
    assert!(first
        .spatial_registry()
        .lookup("fixture/1", at(1))
        .is_some());
    assert!(second
        .spatial_registry()
        .lookup("alternative", at(1))
        .is_some());
    let original = (first.clone(), second.clone());
    let mut shared = SharedState::default();
    let _before = first.checkpoint(&mut shared);
    let _after = second.checkpoint(&mut shared);
    assert!(serde_json::to_vec(&shared).is_err());
    assert_eq!((first, second), original);
}
#[test]
fn append_to_decoded_capture_poison_blocks_the_complete_archive() {
    let game = fixture();
    let mut original = SharedState::default();
    let before = game.checkpoint(&mut original);
    let encoded = serde_json::to_vec(&(before, &original)).unwrap();
    let (_before, mut decoded): (crate::checkpoint::Snapshot, SharedState) =
        serde_json::from_slice(&encoded).unwrap();
    // Decoded pools require explicit root binding for restore, not capture append.
    // No reliance on whether a standalone placeholder scalar fits u32 on32-bit.
    let after = game.checkpoint(&mut decoded);
    assert!(serde_json::to_vec(&(after, &decoded)).is_err());
    let repeated = game.checkpoint(&mut decoded);
    assert!(serde_json::to_vec(&(repeated, &decoded)).is_err());
}
#[test]
fn over_snapshot_envelope_capture_poison_does_not_retain_more_root_references() {
    let mut state = super::PoolState::default();
    let root = tor_world::Shared::new(crate::spatial_scope::registry::Registry::default());
    for _ in 0..crate::checkpoint::MAX_CHECKPOINT_ROOTS {
        assert_eq!(state.capture_snapshot(&root), 0);
    }
    assert!(serde_json::to_vec(&state).is_ok());
    state.capture_snapshot(&root);
    assert!(matches!(&state, super::PoolState::InvalidCapture));
    for _ in 0..1000 {
        state.capture_snapshot(&root);
    }
    assert!(matches!(&state, super::PoolState::InvalidCapture));
    assert!(serde_json::to_vec(&state).is_err());
}

#[test]
fn decoded_canonical_serialization_requires_actual_roots_without_granting_restore_or_capture() {
    for populated in [false, true] {
        let mut game = fixture();
        if populated {
            game.register_spatial_scope(scope(1), &budget()).unwrap();
        }
        let mut captured = SharedState::default();
        let snapshot = game.checkpoint(&mut captured);
        let canonical = serde_json::to_value(&captured).unwrap();
        let named = serde_json::to_value(&snapshot).unwrap();
        let mut decoded: SharedState = serde_json::from_value(canonical.clone()).unwrap();
        assert!(serde_json::to_value(&decoded).is_err());
        decoded.prepare_spatial_serialization(&[&snapshot]).unwrap();
        assert_eq!(serde_json::to_value(&decoded).unwrap(), canonical);
        let mut unbound = RestoreContext::new(&decoded, game.content().clone());
        assert!(unbound
            .restore(serde_json::from_value(named.clone()).unwrap())
            .is_err());
        let mut bound = RestoreContext::new(&decoded, game.content().clone());
        bound.bind_spatial_roots(&[&snapshot]).unwrap();
        assert_eq!(
            bound
                .restore(serde_json::from_value(named.clone()).unwrap())
                .unwrap(),
            game
        );
        drop(bound);
        drop(unbound);

        let mut bad = named;
        bad["spatial"] = serde_json::json!(u32::MAX);
        let bad: crate::checkpoint::Snapshot = serde_json::from_value(bad).unwrap();
        assert!(decoded.prepare_spatial_serialization(&[&bad]).is_err());
        assert!(serde_json::to_value(&decoded).is_err());
        decoded.prepare_spatial_serialization(&[&snapshot]).unwrap();
        let excess = vec![&snapshot; crate::checkpoint::MAX_CHECKPOINT_ROOTS + 1];
        assert!(decoded.prepare_spatial_serialization(&excess).is_err());
        assert!(serde_json::to_value(&decoded).is_err());
        decoded.prepare_spatial_serialization(&[&snapshot]).unwrap();
        let _unsupported_append = game.checkpoint(&mut decoded);
        assert!(serde_json::to_value(&decoded).is_err());
        assert!(decoded.prepare_spatial_serialization(&[&snapshot]).is_err());
    }
}

#[test]
fn canonical_serialization_reprepare_rejects_omitted_history_directory() {
    let mut game = fixture();
    let mut shared = SharedState::default();
    let earlier = game.checkpoint(&mut shared);
    game.register_spatial_scope(scope(1), &budget()).unwrap();
    let current = game.checkpoint(&mut shared);
    let canonical = serde_json::to_value(&shared).unwrap();
    let mut decoded: SharedState = serde_json::from_value(canonical.clone()).unwrap();
    decoded
        .prepare_spatial_serialization(&[&current, &earlier])
        .unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), canonical);
    assert!(decoded.prepare_spatial_serialization(&[&current]).is_err());
    assert!(serde_json::to_value(&decoded).is_err());
    decoded
        .prepare_spatial_serialization(&[&current, &earlier])
        .unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), canonical);
}

#[test]
fn historical_authority_references_exceed_snapshot_count_without_relaxing_snapshot_limit() {
    use super::PoolState;
    use crate::spatial_scope::registry::Registry;
    use tor_world::Shared;
    let registry = Shared::new(Registry::default());
    let mut pool = PoolState::default();
    let references: Vec<_> = (0..300).map(|_| pool.capture(&registry)).collect();
    assert!(pool.bind(references.clone().into_iter()).is_ok());
    let encoded = serde_json::to_vec(&pool).unwrap();
    let decoded: PoolState = serde_json::from_slice(&encoded).unwrap();
    assert!(decoded.bind(references.into_iter()).is_ok());
    assert!(decoded.bind(vec![usize::MAX; 300].into_iter()).is_err());
    let mut snapshots = PoolState::default();
    for _ in 0..=crate::checkpoint::MAX_CHECKPOINT_ROOTS {
        snapshots.capture_snapshot(&registry);
    }
    assert!(serde_json::to_vec(&snapshots).is_err());
}

#[test]
fn outer_snapshot_limit_failure_revokes_previous_serialization_proof_but_allows_explicit_rebind() {
    let game = fixture();
    let mut captured = SharedState::default();
    let snapshot = game.checkpoint(&mut captured);
    let canonical = serde_json::to_value(&captured).unwrap();
    let mut decoded: SharedState = serde_json::from_value(canonical.clone()).unwrap();
    decoded.prepare_spatial_serialization(&[&snapshot]).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), canonical);
    let excess = vec![&snapshot; crate::checkpoint::MAX_CHECKPOINT_ROOTS + 1];
    assert!(decoded.prepare_spatial_serialization(&excess).is_err());
    assert!(serde_json::to_value(&decoded).is_err());
    // Repeated outer failures cannot resurrect the old canonical witness.
    assert!(decoded.prepare_spatial_serialization(&excess).is_err());
    assert!(serde_json::to_value(&decoded).is_err());
    decoded.prepare_spatial_serialization(&[&snapshot]).unwrap();
    assert_eq!(serde_json::to_value(&decoded).unwrap(), canonical);
    let mut unbound = RestoreContext::new(&decoded, game.content().clone());
    assert!(unbound.restore(snapshot).is_err());
}

#[test]
fn historical_reference_admission_still_obeys_archive_meter_not_snapshot_count() {
    let root = tor_world::Shared::new(crate::spatial_scope::registry::Registry::default());
    let directories = [root];
    // No large allocation: the exact-size iterator is rejected by its declared
    // count before reference materialization. Equality also exceeds the actual
    // meter after its mandatory envelope/directory bytes are charged.
    for count in [
        super::codec::MAX_ROOT_REFERENCES,
        super::codec::MAX_ROOT_REFERENCES + 1,
    ] {
        assert!(matches!(
            super::codec::capture_pool(&directories, std::iter::repeat_n(0, count)),
            Err(super::codec::PoolError::Capacity)
        ));
    }
    assert!(super::codec::capture_pool(&directories, std::iter::repeat_n(0, 300)).is_ok());
}
