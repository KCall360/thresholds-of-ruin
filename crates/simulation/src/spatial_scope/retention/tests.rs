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
#[test]
fn immutable_spatial_game_roots_keep_old_subset_and_failed_install_unchanged() {
    let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
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
    let mut old = Game::two_room_in_stone(42, crate::test_creatures::content());
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
    let game = Game::two_room_in_stone(42, crate::test_creatures::content());
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
