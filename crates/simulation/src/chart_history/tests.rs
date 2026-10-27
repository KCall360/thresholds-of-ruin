use super::*;
use crate::checkpoint::{RestoreContext, SharedState, Snapshot};
use crate::spatial_scope::registry::{CellRun, PrimaryRun, RoomSurfaces, Scope};
use crate::spatial_scope::{Membership, PrimarySpace, RoomId};
use tor_world::{Position, RegionId};
pub(crate) fn fixture() -> (crate::Game, ActorId, Shared<HistoricalChart>) {
    let mut game = crate::Game::two_room_in_stone(42, crate::test_creatures::content());
    let at = Location {
        region: RegionId(1),
        position: Position { x: 1, y: 1, z: 0 },
    };
    let actor = game
        .spawn_actor(at, std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    let room = RoomId::new(1).unwrap();
    let cells = CellRun {
        region: RegionId(1),
        z: 0,
        y: 1,
        first_x: 1,
        last_x: 1,
    };
    let scope = Scope {
        id: "history/one".into(),
        members: vec![RegionId(1)],
        primary: vec![PrimaryRun {
            cells,
            membership: Membership::new(PrimarySpace::Room(room), None).unwrap(),
        }],
        rooms: vec![RoomSurfaces {
            room,
            runs: vec![cells],
        }],
        mapping: crate::spatial_scope::mapping::MappingCertification::Uncertified,
    };
    game.register_spatial_scope(
        scope,
        &crate::spatial_scope::codec::archive_structural_budget().unwrap(),
    )
    .unwrap();
    let projection = scope_projection::project(
        &game.world,
        &[RegionId(1)],
        &[at, at],
        archive_budget().projection,
    )
    .unwrap();
    let candidate = chart::Candidate::from_complete(
        chart::Kind::Food,
        projection
            .components
            .iter()
            .map(|c| chart::Component {
                alignment: c.alignment,
            })
            .collect(),
        projection
            .samples
            .iter()
            .map(|p| chart::Row {
                component: p.component,
                position: p.position,
                mark: chart::Mark::Food,
            })
            .collect(),
        archive_budget().chart,
    )
    .unwrap();
    let root = game
        .capture_historical_chart(
            Kind::Food,
            "history/one".into(),
            vec![at, at],
            candidate.shared_chart().unwrap().clone(),
            archive_budget(),
        )
        .unwrap();
    (game, actor, root)
}
#[test]
fn empty_and_unavailable_update_attempt_without_erasing_positive_or_cloning_chart() {
    let (_, _, root) = fixture();
    let mut owner = Owner::default();
    assert!(owner
        .record_attempt(
            Kind::Food,
            Attempt {
                attempted_at: 0,
                status: AttemptStatus::Positive
            },
            Some(root.clone())
        )
        .unwrap());
    assert!(owner
        .record_attempt(
            Kind::Food,
            Attempt {
                attempted_at: 1,
                status: AttemptStatus::Empty
            },
            None
        )
        .unwrap());
    assert!(owner
        .get(Kind::Food)
        .latest()
        .unwrap()
        .shares_storage(&root));
    assert!(owner
        .record_attempt(
            Kind::Food,
            Attempt {
                attempted_at: 2,
                status: AttemptStatus::Unavailable(UnavailableReason::Budget)
            },
            None
        )
        .unwrap());
    assert_eq!(
        owner.get(Kind::Food).attempt().unwrap().status,
        AttemptStatus::Unavailable(UnavailableReason::Budget)
    );
    assert!(owner
        .get(Kind::Food)
        .latest()
        .unwrap()
        .shares_storage(&root));
    assert!(owner.get(Kind::Magic).attempt().is_none());
    let before = owner.clone();
    assert!(owner
        .record_attempt(
            Kind::Magic,
            Attempt {
                attempted_at: 2,
                status: AttemptStatus::Positive
            },
            Some(root)
        )
        .is_err());
    assert_eq!(owner, before);
}
#[test]
fn current_and_retained_boundaries_share_one_exact_historical_payload() {
    let (mut game, actor, root) = fixture();
    let owner = game.chart_history.entry(actor).or_default();
    owner
        .record_attempt(
            Kind::Food,
            Attempt {
                attempted_at: 0,
                status: AttemptStatus::Positive,
            },
            Some(root),
        )
        .unwrap();
    let mut shared = SharedState::default();
    let first = game.checkpoint(&mut shared);
    game.chart_history
        .get_mut(&actor)
        .unwrap()
        .record_attempt(
            Kind::Food,
            Attempt {
                attempted_at: 0,
                status: AttemptStatus::Empty,
            },
            None,
        )
        .unwrap();
    let last = game.checkpoint(&mut shared);
    let bytes = serde_json::to_vec(&(vec![first, last], shared)).unwrap();
    let (snapshots, shared): (Vec<Snapshot>, SharedState) = serde_json::from_slice(&bytes).unwrap();
    assert!(shared.world_references_valid(snapshots.iter()));
    let mut context = RestoreContext::new(&shared, game.content().clone());
    context
        .bind_spatial_roots(&snapshots.iter().collect::<Vec<_>>())
        .unwrap();
    let games: Vec<_> = snapshots
        .into_iter()
        .map(|s| context.restore(s).unwrap())
        .collect();
    let a = games[0].chart_history(actor).unwrap().get(Kind::Food);
    let b = games[1].chart_history(actor).unwrap().get(Kind::Food);
    assert!(a.latest().unwrap().shares_storage(b.latest().unwrap()));
    assert_eq!(a.latest().unwrap().chart().rows().len(), 2);
    assert_eq!(b.attempt().unwrap().status, AttemptStatus::Empty);
}
#[test]
fn self_contained_detached_codec_deduplicates_multiple_owners_and_rejects_forgery() {
    let (_, actor, root) = fixture();
    let mut owner = Owner::default();
    owner
        .record_attempt(
            Kind::Food,
            Attempt {
                attempted_at: 0,
                status: AttemptStatus::Positive,
            },
            Some(root),
        )
        .unwrap();
    let owners: BTreeMap<_, _> = [(actor, owner.clone()), (ActorId(actor.0 + 1), owner)].into();
    let stored = codec::LocalHistory::capture(&owners);
    let original = serde_json::to_value(&stored).unwrap();
    assert_eq!(original["history"]["roots"].as_array().unwrap().len(), 1);
    assert_eq!(
        original["history"]["roots"][0]["world"],
        serde_json::json!(0)
    );
    let restored: codec::LocalHistory = serde_json::from_value(original.clone()).unwrap();
    let restored = restored.restore().unwrap();
    let first = restored[&actor].get(Kind::Food).latest().unwrap();
    let second = restored[&ActorId(actor.0 + 1)]
        .get(Kind::Food)
        .latest()
        .unwrap();
    assert!(first.shares_storage(second));
    for mutation in 0..3 {
        let mut forged = original.clone();
        match mutation {
            0 => forged["history"]["roots"][0]["points"][0]["component"] = serde_json::json!(99),
            1 => forged["history"]["roots"][0]["world"] = serde_json::json!(99),
            _ => {
                forged["history"]["owners"][0][0]["slots"]["food"]["latest"] = serde_json::json!(99)
            }
        }
        let outcome = serde_json::from_value::<codec::LocalHistory>(forged)
            .ok()
            .and_then(|v| v.restore().ok());
        assert!(outcome.is_none());
    }
}
// Append inside chart_history::tests; requires core-identity.rs companion.
#[test]
fn unrelated_live_clock_and_monster_chart_query_do_not_trigger_history_guard() {
    let (game, _, _) = fixture();
    let mut after = game.clone();
    after.tick += 1;
    let candidate = chart::Candidate::from_complete(
        chart::Kind::Monsters,
        vec![chart::Component {
            alignment: scope_projection::Alignment::Aligned,
        }],
        vec![chart::Row {
            component: 0,
            position: Some([1, 1, 0]),
            mark: chart::Mark::Monster(crate::appearance_catalog::CreatureAppearance {
                name: Arc::from("bat"),
                asset: None,
            }),
        }],
        archive_budget().chart,
    )
    .unwrap();
    assert_eq!(candidate.chart().unwrap().rows().len(), 1);
    assert!(!after.chart_history_changed(&game));
    // Allocating equal ownerbook storage must not turn identity into authority.
    after.chart_history = Shared::new((*game.chart_history).clone());
    assert!(!after.chart_history_changed(&game));
}
#[test]
fn food_and_magic_attempt_changes_trigger_but_equal_attempts_bypass() {
    let (game, actor, _) = fixture();
    for kind in [Kind::Food, Kind::Magic] {
        let mut after = game.clone();
        after
            .chart_history
            .entry(actor)
            .or_default()
            .record_attempt(
                kind,
                Attempt {
                    attempted_at: 0,
                    status: AttemptStatus::Empty,
                },
                None,
            )
            .unwrap();
        assert!(after.chart_history_changed(&game));
        let before = after.clone();
        assert!(!after
            .chart_history
            .get_mut(&actor)
            .unwrap()
            .record_attempt(
                kind,
                Attempt {
                    attempted_at: 0,
                    status: AttemptStatus::Empty
                },
                None
            )
            .unwrap());
        assert!(!after.chart_history_changed(&before));
    }
}

#[test]
fn malformed_historical_only_world_is_rejected_without_current_world_dependency() {
    let (game, actor, root) = fixture();
    assert!(game.world.checkpoint_valid(game.next_door_id));
    let mut value = serde_json::to_value(&*root.authority.world).unwrap();
    value["region_gravity"] = serde_json::json!({"999999":[2,0,0]});
    let invalid: World = serde_json::from_value(value).unwrap();
    assert!(!invalid.historical_checkpoint_valid());
    let mut forged = (*root).clone();
    forged.authority = Shared::new(AcquisitionAuthority {
        world: Shared::new(invalid),
        registry: root.authority.registry.clone(),
    });
    let mut owner = Owner::default();
    owner
        .record_attempt(
            Kind::Food,
            Attempt {
                attempted_at: 0,
                status: AttemptStatus::Positive,
            },
            Some(Shared::new(forged)),
        )
        .unwrap();
    let stored = codec::LocalHistory::capture(&[(actor, owner)].into());
    let bytes = serde_json::to_vec(&stored).unwrap();
    let decoded: codec::LocalHistory = serde_json::from_slice(&bytes).unwrap();
    assert!(matches!(decoded.restore(), Err(Error::Source)));
    assert!(game.world.checkpoint_valid(game.next_door_id));
}
