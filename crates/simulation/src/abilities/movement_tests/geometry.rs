//! Portal, ordinary stair and explicit eligibility boundary controls.
use super::*;
#[test]
fn guarded_step_preserves_portal_orientation_and_checks_every_destination_body_cell() {
    for blocked in [false, true] {
        let mut game = guarded_step_game(100);
        let actor = crate::ActorId(1);
        game.teleport(actor, cell(7, 2, 0)).unwrap();
        game.add_region(tor_world::Region {
            id: tor_world::RegionId(2),
            name: "step destination".into(),
            bounds: tor_world::Extent::new(8, 8, 4).unwrap(),
        })
        .unwrap();
        let destination = tor_world::Location {
            region: tor_world::RegionId(2),
            position: tor_world::Position { x: 2, y: 7, z: 0 },
        };
        let rotation = (0..24)
            .find(|rotation| {
                tor_world::rotate_vector(*rotation, [1, 0, 0]) == [0, -1, 0]
                    && tor_world::rotate_vector(*rotation, [0, 0, 1]) == [0, 0, 1]
            })
            .unwrap();
        game.connect_portal_area(
            tor_world::Passage {
                from: cell(7, 2, 0),
                direction: tor_world::Direction::East,
                to: destination,
            },
            rotation,
            1,
            2,
        )
        .unwrap();
        game.set_body(
            actor,
            crate::BodySpec {
                cells: vec![[0, 0, 0], [0, 0, 1]],
                eye: [0, 0, 1],
                mass: 80,
            },
        )
        .unwrap();
        let identity = start_step(&mut game, "east");
        let head = tor_world::Location {
            position: tor_world::Position {
                z: 1,
                ..destination.position
            },
            ..destination
        };
        assert_eq!(
            game.preparation(actor)
                .unwrap()
                .plan
                .as_ability()
                .unwrap()
                .step
                .unwrap()
                .destination(),
            destination
        );
        if blocked {
            game.set_wall(head, true).unwrap();
        }
        game.act(crate::ActorId(2), crate::Action::Wait).unwrap();
        assert_eq!(game.tick(), 100);
        assert_eq!(game.actors[&actor].ready_at, 300);
        assert_eq!(
            game.actors[&actor].location,
            if blocked { cell(7, 2, 0) } else { destination }
        );
        assert_eq!(
            game.actors[&actor].orientation,
            if blocked { 0 } else { rotation }
        );
        assert!(!game.physics.displaced.contains(&actor));
        assert!(game
            .creature(actor)
            .unwrap()
            .costs()
            .reservation(identity)
            .is_none());
        assert_eq!(
            game.creature(actor)
                .unwrap()
                .conditions()
                .instances()
                .find(|condition| condition.condition.as_str() == "braced")
                .unwrap()
                .remaining,
            Some(200)
        );
        if !blocked {
            assert!(game.actors[&actor].visited.contains(&destination.region));
            assert!(
                game.set_wall(head, true).is_err(),
                "published occupancy includes the transformed head"
            );
        }
    }
}

#[test]
fn guarded_step_can_take_the_same_explicit_stair_as_an_ordinary_step() {
    let mut game = guarded_step_game(75);
    let actor = crate::ActorId(1);
    let destination = cell(6, 6, 2);
    game.connect(
        tor_world::Passage {
            from: cell(2, 2, 0),
            direction: tor_world::Direction::Up,
            to: destination,
        },
        0,
    )
    .unwrap();
    let ordinary = game
        .actor_translation(actor, tor_world::Direction::Up)
        .unwrap();
    start_step(&mut game, "up");
    while game.preparation(actor).is_some() {
        let next = game.next_actor().unwrap();
        assert_ne!(next, actor);
        game.act(next, crate::Action::Wait).unwrap();
    }
    assert_eq!(
        (
            game.actors[&actor].location,
            game.actors[&actor].orientation
        ),
        ordinary
    );
    assert_eq!(game.actors[&actor].ready_at, 275);
    assert_eq!(game.actors[&actor].location, destination);
}
#[test]
fn explicit_size_ordinals_and_target_anchor_gate_are_exhaustive() {
    use crate::creatures::{CreatureSize, DisplacementProfile};
    let sizes = [
        CreatureSize::Tiny,
        CreatureSize::Small,
        CreatureSize::Medium,
        CreatureSize::Large,
        CreatureSize::Huge,
        CreatureSize::Colossal,
    ];
    for (source_ordinal, source) in sizes.into_iter().enumerate() {
        assert_eq!(source as usize, source_ordinal);
        for (target_ordinal, target) in sizes.into_iter().enumerate() {
            for anchored in [false, true] {
                assert_eq!(
                    DisplacementProfile {
                        size: source,
                        anchored: false
                    }
                    .permits_push(DisplacementProfile {
                        size: target,
                        anchored
                    }),
                    !anchored && target_ordinal <= source_ordinal + 1
                );
            }
        }
    }
}

#[test]
fn displacement_profile_bytes_are_named_current_only_and_have_no_inferred_defaults() {
    use crate::creatures::DisplacementProfile;
    for size in ["tiny", "small", "medium", "large", "huge", "colossal"] {
        for anchored in [false, true] {
            let value = serde_json::json!({"size":size,"anchored":anchored});
            let profile: DisplacementProfile =
                serde_json::from_slice(&serde_json::to_vec(&value).unwrap()).unwrap();
            assert_eq!(serde_json::to_value(profile).unwrap(), value);
        }
    }
    for raw in [
        r#"{}"#,
        r#"["medium",false]"#,
        r#"{"size":2,"anchored":false}"#,
        r#"{"size":"medium"}"#,
        r#"{"anchored":false}"#,
        r#"{"size":"Medium","anchored":false}"#,
        r#"{"size":"gargantuan","anchored":false}"#,
        r#"{"size":"medium","anchored":0}"#,
        r#"{"size":"medium","anchored":false,"mass":80}"#,
        r#"{"size":"medium","size":"tiny","anchored":false}"#,
        r#"{"size":"medium","anchored":false,"anchored":true}"#,
    ] {
        assert!(
            serde_json::from_slice::<DisplacementProfile>(raw.as_bytes()).is_err(),
            "{raw}"
        );
    }
}
