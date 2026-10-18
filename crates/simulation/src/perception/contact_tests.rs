//! Exact anonymous contact footprints, topology parity and bounded projection work.
use super::*;
use crate::abilities::Placement;
use tor_world::{Extent, Passage, Region, RegionId, World};

fn quarter_turn() -> u8 {
    (0..24)
        .find(|&turn| {
            tor_world::rotate_vector(turn, [1, 0, 0]) == [0, 1, 0]
                && tor_world::rotate_vector(turn, [0, 1, 0]) == [-1, 0, 0]
        })
        .unwrap()
}

fn joined_fixture(reverse: bool) -> Game {
    let content = fixture(&[], &[invisible()]).content().clone();
    let mut world = World::new(
        vec![
            Region {
                id: RegionId(1),
                name: "before quarter turn".into(),
                bounds: Extent::new(3, 5, 1).unwrap(),
            },
            Region {
                id: RegionId(2),
                name: "after quarter turn".into(),
                bounds: Extent::new(5, 3, 1).unwrap(),
            },
        ],
        vec![],
    )
    .unwrap();
    for (from, direction, to, turn) in [
        (at(1, 2, 0), Direction::East, at(2, 4, 0), quarter_turn()),
        (
            at(2, 0, 0),
            Direction::North,
            at(1, 2, 4),
            tor_world::inverse_rotation(quarter_turn()),
        ),
    ] {
        world
            .connect_portal_area(
                Passage {
                    from,
                    direction,
                    to,
                },
                turn,
                5,
                1,
            )
            .unwrap();
    }
    let mut game = Game::new(world, 42, content);
    assert_eq!(
        spawn(&mut game, "fixture_perception_owner", at(1, 2, 2)),
        OWNER
    );
    assert_eq!(
        spawn(&mut game, "fixture_perception_subject", at(2, 2, 1)),
        SUBJECT
    );
    let mut cells = vec![[0, 0, 0], [0, -1, 0]];
    if reverse {
        cells.reverse();
    }
    game.set_body(
        SUBJECT,
        crate::BodySpec {
            cells,
            ..Default::default()
        },
    )
    .unwrap();
    game
}

// Compare the new set-based query to the existing Boolean authority only
// for distinct living combat targets, which are that authority's domain.
fn checked_neighborhood(
    game: &Game,
    source: ActorId,
    targets: &[ActorId],
    scene: &[SightCell],
    placement: Option<Placement>,
) -> BTreeSet<Location> {
    let contacts = game.melee_contact_locations_in_scene(source, scene, placement);
    for &target in targets {
        let actor = &game.actors[&target];
        let body = game
            .body_cells(actor.location, actor.orientation, &actor.body)
            .unwrap();
        assert_eq!(
            body.iter().any(|(location, _)| contacts.contains(location)),
            game.melee_reachable_at_placement(source, target, scene, placement),
            "source {source:?}, target {target:?}, placement {placement:?}",
        );
    }
    contacts
}

#[test]
fn anonymous_contact_across_rotated_join_emits_only_the_reachable_limb() {
    for reverse in [false, true] {
        let mut game = joined_fixture(reverse);
        let limb = at(2, 2, 0);
        let reference = at(2, 2, 1);
        assert_eq!(
            game.world.physics_neighbor(at(1, 2, 2), Direction::East),
            Some((limb, quarter_turn())),
        );
        let (scene, sight) = game.scene_snapshot(OWNER).unwrap();
        assert!(scene
            .iter()
            .any(|cell| cell.location == limb && cell.physical));
        assert!(scene.iter().any(|cell| cell.location == reference));
        let contacts = checked_neighborhood(&game, OWNER, &[SUBJECT], &scene, None);
        assert!(contacts.contains(&limb));
        assert!(!contacts.contains(&reference));
        // Local awareness must retain the same physical contact even when
        // callers provide no scene occurrences.
        assert!(checked_neighborhood(&game, OWNER, &[SUBJECT], &[], None).contains(&limb));
        let before = game.clone();
        let rows = game.perception_projection(OWNER, &scene, &sight).unwrap();
        assert_eq!(rows[&SUBJECT].disclosure, ActorDisclosure::AnonymousContact);
        assert_eq!(
            rows[&SUBJECT]
                .body
                .iter()
                .map(|(location, _)| *location)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([limb]),
        );
        assert_eq!(
            game.observe(OWNER)
                .unwrap()
                .visible_actors
                .into_iter()
                .filter(|actor| actor.id == SUBJECT)
                .map(|actor| actor.location)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([limb]),
        );
        assert_eq!(game, before);
        assert!(game
            .actors
            .at(&game.world, reference)
            .contains_key(&SUBJECT));
        assert!(game.actors.at(&game.world, limb).contains_key(&SUBJECT));
        apply(&mut game, OWNER, "fixture_detection", 1000);
        let rows = game.classified_actors(OWNER).unwrap();
        assert_eq!(rows[&SUBJECT].disclosure, ActorDisclosure::Creature);
        assert_eq!(rows[&SUBJECT].body.len(), 2);
    }
}

#[test]
fn prospective_contact_uses_location_and_orientation_without_mutating_the_actor() {
    let mut game = fixture(&[], &[invisible()]);
    game.teleport(SUBJECT, at(1, 4, 2)).unwrap();
    game.set_body(
        OWNER,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [1, 0, 0]],
            ..Default::default()
        },
    )
    .unwrap();
    let before = game.clone();
    for (placement, expected) in [
        (
            Placement {
                location: at(1, 2, 2),
                orientation: 0,
            },
            true,
        ),
        (
            Placement {
                location: at(1, 2, 2),
                orientation: quarter_turn(),
            },
            false,
        ),
        (
            Placement {
                location: at(1, 3, 2),
                orientation: quarter_turn(),
            },
            true,
        ),
    ] {
        let (scene, sight) = game.scene_snapshot_at_placement(OWNER, placement).unwrap();
        let contacts = checked_neighborhood(&game, OWNER, &[SUBJECT], &scene, Some(placement));
        assert_eq!(contacts.contains(&at(1, 4, 2)), expected);
        let rows = game
            .perception_projection_at(OWNER, &scene, &sight, Some(placement))
            .unwrap();
        assert_eq!(
            rows[&SUBJECT].disclosure,
            if expected {
                ActorDisclosure::AnonymousContact
            } else {
                ActorDisclosure::Hidden
            }
        );
        if expected {
            assert_eq!(rows[&SUBJECT].body.as_slice(), &[(at(1, 4, 2), 0)]);
        }
    }
    assert_eq!(game, before);
    assert_eq!(
        Placement::capture(&game, OWNER),
        Some(Placement {
            location: at(1, 2, 2),
            orientation: 0,
        })
    );
}

#[test]
fn noncombat_observer_cannot_acquire_anonymous_physical_contact() {
    let mut game = fixture(&[], &[invisible()]);
    let observer = game
        .spawn_actor(at(1, 3, 3), std::num::NonZeroU64::new(100).unwrap())
        .unwrap();
    assert!(game.alive(observer));
    assert!(game.actors[&observer].combat.is_none());
    let (scene, sight) = game.scene_snapshot(observer).unwrap();
    assert!(scene.iter().any(|cell| cell.location == at(1, 3, 2)));
    assert!(checked_neighborhood(&game, observer, &[SUBJECT], &scene, None).is_empty());
    let rows = game
        .perception_projection(observer, &scene, &sight)
        .unwrap();
    assert_eq!(rows[&SUBJECT].disclosure, ActorDisclosure::Hidden);
}

#[test]
fn ordinary_and_visually_identified_candidates_build_no_contact_neighborhood() {
    for concealed in [false, true] {
        let game = if concealed {
            fixture(
                &[PerceptionValue::SeeInvisible { enabled: true }],
                &[invisible()],
            )
        } else {
            fixture(&[], &[])
        };
        let (scene, sight) = game.scene_snapshot(OWNER).unwrap();
        let before = crate::diagnostics::work_counts();
        let rows = game.perception_projection(OWNER, &scene, &sight).unwrap();
        let after = crate::diagnostics::work_counts();
        assert_eq!(rows[&SUBJECT].disclosure, ActorDisclosure::Creature);
        assert_eq!(
            after.perception_projections - before.perception_projections,
            1
        );
        assert_eq!(
            after.contact_neighborhoods - before.contact_neighborhoods,
            0
        );
        assert_eq!(after.contact_route_probes - before.contact_route_probes, 0);
    }
}

#[test]
fn concealed_candidates_share_one_source_body_neighborhood_per_projection() {
    let mut game = fixture(&[], &[invisible()]);
    game.set_body(
        OWNER,
        crate::BodySpec {
            cells: vec![[0, 0, 0], [0, 1, 0]],
            ..Default::default()
        },
    )
    .unwrap();
    let mut targets = vec![SUBJECT];
    for (x, y) in [(3, 1), (3, 3), (4, 1), (4, 2), (4, 3)] {
        targets.push(spawn(&mut game, "fixture_perception_subject", at(1, x, y)));
    }
    let (scene, sight) = game.scene_snapshot(OWNER).unwrap();
    let contacts = checked_neighborhood(&game, OWNER, &targets, &scene, None);
    // Repeating the projection proves the cache is projection-local, not a
    // persisted observer/world cache. Count only the projection itself.
    for _ in 0..2 {
        let before = crate::diagnostics::work_counts();
        let rows = game.perception_projection(OWNER, &scene, &sight).unwrap();
        let after = crate::diagnostics::work_counts();
        assert_eq!(rows.len(), targets.len() + 1);
        for &target in &targets {
            let expected = if contacts.contains(&game.actors[&target].location) {
                ActorDisclosure::AnonymousContact
            } else {
                ActorDisclosure::Hidden
            };
            assert_eq!(rows[&target].disclosure, expected);
        }
        assert_eq!(
            after.perception_projections - before.perception_projections,
            1
        );
        assert_eq!(
            after.contact_neighborhoods - before.contact_neighborhoods,
            1
        );
        assert_eq!(
            after.contact_route_probes - before.contact_route_probes,
            26 * 2
        );
    }
}

#[test]
fn empty_contact_neighborhood_is_cached_across_concealed_stair_candidates() {
    let mut game = fixture(&[], &[invisible()]);
    let other = spawn(&mut game, "fixture_perception_subject", at(1, 4, 2));
    let origin = at(2, 4, 3);
    game.teleport(OWNER, origin).unwrap();
    for (x, y) in [(3, 3), (5, 3), (4, 2), (4, 4)] {
        game.set_wall(at(2, x, y), true).unwrap();
    }
    game.register_named_anchors(
        RegionId(1),
        BTreeMap::from([
            ("up_contact".into(), at(1, 3, 2).position),
            ("down_contact".into(), at(1, 4, 2).position),
        ]),
    )
    .unwrap();
    for (direction, name) in [
        (Direction::Up, "up_contact"),
        (Direction::Down, "down_contact"),
    ] {
        game.connect_named_stair(
            origin,
            direction,
            tor_world::NamedAnchor {
                region: RegionId(1),
                name: name.into(),
            },
        )
        .unwrap();
    }
    let (scene, sight) = game.scene_snapshot(OWNER).unwrap();
    let targets = [SUBJECT, other];
    for target in targets {
        let location = game.actors[&target].location;
        assert!(scene
            .iter()
            .any(|cell| cell.location == location && !cell.physical));
    }
    assert!(checked_neighborhood(&game, OWNER, &targets, &scene, None).is_empty());
    let before = crate::diagnostics::work_counts();
    let rows = game.perception_projection(OWNER, &scene, &sight).unwrap();
    let after = crate::diagnostics::work_counts();
    assert_eq!(rows.len(), 3);
    for target in targets {
        assert_eq!(rows[&target].disclosure, ActorDisclosure::Hidden);
    }
    assert_eq!(
        after.contact_neighborhoods - before.contact_neighborhoods,
        1
    );
    assert_eq!(after.contact_route_probes - before.contact_route_probes, 26);
}
