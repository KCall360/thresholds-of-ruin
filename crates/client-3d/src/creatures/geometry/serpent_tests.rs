//! Readable skulls and actual hinged bites on both independently placed snakes.
use super::*;
use crate::creatures::rogue_tests::{animate_look, spawn_one};
use bevy::render::mesh::VertexAttributeValues;

fn direct_vertices(world: &World, meshes: &Assets<Mesh>, part: Entity) -> Vec<Vec3> {
    let mut out = Vec::new();
    for child in world.get::<Children>(part).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions");
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        for point in points {
            let p = transform.transform_point3((*point).into());
            assert!(p.is_finite());
            out.push(p);
        }
    }
    assert!(!out.is_empty());
    out
}

fn direct_bounds(world: &World, meshes: &Assets<Mesh>, part: Entity) -> (Vec3, Vec3) {
    direct_vertices(world, meshes, part).into_iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(low, high), p| (low.min(p), high.max(p)),
    )
}

#[test]
fn serpent_skulls_remain_substantial_beside_the_supporting_neck() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        for build in 0..3 {
            let look = Look::with_choice(model, 42, Some(Variant { build, ..default() }));
            let (world, rig, _, meshes) = spawn_one(&look);
            let (low, high) = direct_bounds(&world, &meshes, rig.segments[0]);
            let (neck_low, neck_high) = direct_bounds(&world, &meshes, rig.segments[1]);
            let size = high - low;
            let breadth = size.x / (neck_high - neck_low).x;
            assert!(breadth > 1.45 && size.y > 0.18 && size.z > 0.34,
                "{model:?} build={build}: skull must read separately from neck: size={size:?}, breadth ratio={breadth:.3}");
        }
    }
}

#[test]
fn serpent_lower_jaws_are_separate_full_length_mouth_surfaces() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (world, rig, _, meshes) = spawn_one(&Look::of(model, 42));
        let jaw = rig
            .joint(J::Jaw)
            .expect("a biting snake needs a separate hinged lower jaw");
        assert!(world
            .get::<Children>(rig.segments[0])
            .unwrap()
            .contains(&jaw));
        let (low, high) = direct_bounds(&world, &meshes, jaw);
        let size = high - low;
        assert!(
            size.x > 0.19 && size.y > 0.035 && size.z > 0.28,
            "{model:?}: lower jaw must supply the broad lower mouth: {size:?}"
        );
    }
}

#[test]
fn serpent_bites_gape_in_preparation_close_at_impact_and_keep_the_root() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (mut app, root) = animate_look(Look::of(model, 42), 12, |_| Vec3::ZERO);
        let jaw = app
            .world()
            .get::<Rig>(root)
            .unwrap()
            .joint(J::Jaw)
            .expect("hinged bite");
        let angle = |app: &App| {
            app.world()
                .get::<Transform>(jaw)
                .unwrap()
                .rotation
                .angle_between(Quat::IDENTITY)
        };
        assert!(angle(&app) < 0.12, "closed resting mouth");
        let motion = if model == CreatureModel::Snake {
            Move::Strike
        } else {
            Move::Bite(Accent::Teal)
        };
        for (phase, open) in [(motion.impact() - 0.10, true), (motion.impact(), false)] {
            app.world_mut().get_mut::<Anim>(root).unwrap().action =
                Some((motion, phase - 0.016 / motion.duration()));
            app.update();
            let gape = angle(&app);
            assert!(
                if open { gape > 0.65 } else { gape < 0.20 },
                "{model:?} phase={phase}: gape={gape}"
            );
            assert!(
                app.world()
                    .get::<Transform>(root)
                    .unwrap()
                    .translation
                    .length()
                    < 1e-6
            );
        }
    }
}

#[test]
fn serpent_fangs_fold_inside_the_closed_mouth_and_extend_with_the_bite() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (mut app, root) = animate_look(Look::of(model, 42), 12, |_| Vec3::ZERO);
        let rig = app.world().get::<Rig>(root).unwrap();
        let fangs = rig.joint(J::Fangs).expect("folding upper fangs");
        let head = rig.segments[0];
        let jaw = rig.joint(J::Jaw).unwrap();
        let motion = Move::Bite(Accent::Teal);
        for (phase, extended) in [
            (None, false),
            (Some(motion.impact() - 0.10), true),
            (Some(motion.impact()), false),
        ] {
            app.world_mut().get_mut::<Anim>(root).unwrap().action =
                phase.map(|t| (motion, t - 0.016 / motion.duration()));
            app.update();
            let rotation = app.world().get::<Transform>(fangs).unwrap().rotation;
            let angle = rotation.angle_between(Quat::IDENTITY);
            assert!(
                if extended { angle < 0.12 } else { angle > 1.4 },
                "{model:?}: folded/extended fangs angle={angle}"
            );
            if !extended {
                let world = app.world();
                let meshes = world.resource::<Assets<Mesh>>();
                let fang_transform = world.get::<Transform>(fangs).unwrap().compute_affine();
                let jaw_transform = world.get::<Transform>(jaw).unwrap().compute_affine();
                let low = |part, transform: Affine3A| {
                    direct_vertices(world, meshes, part)
                        .into_iter()
                        .map(|p| transform.transform_point3(p).y)
                        .fold(f32::INFINITY, f32::min)
                };
                assert!(
                    low(fangs, fang_transform) > low(jaw, jaw_transform),
                    "{model:?}: folded fangs must fit above the lower jaw exterior"
                );
                assert!(world.get::<Children>(head).unwrap().contains(&fangs));
            }
        }
    }
}

#[test]
fn serpent_emitted_heads_and_jaws_clear_the_floor_at_rest_bite_and_side_fall() {
    for model in [CreatureModel::Snake, CreatureModel::Rattlesnake] {
        let (mut app, root) = animate_look(Look::of(model, 42), 12, |_| Vec3::ZERO);
        for state in 0..4 {
            {
                let mut anim = app.world_mut().get_mut::<Anim>(root).unwrap();
                let motion = Move::Bite(Accent::Teal);
                anim.action = match state {
                    1 => Some((motion, motion.impact() - 0.10 - 0.016 / motion.duration())),
                    2 => Some((motion, motion.impact() - 0.016 / motion.duration())),
                    _ => None,
                };
                anim.dead = (state == 3).then_some(1.0);
            }
            app.update();
            let world = app.world();
            let rig = world.get::<Rig>(root).unwrap();
            let meshes = world.resource::<Assets<Mesh>>();
            let head = rig.segments[0];
            let head_transform = world.get::<Transform>(head).unwrap().compute_affine();
            for part in [
                head,
                rig.joint(J::Jaw).unwrap(),
                rig.joint(J::Fangs).unwrap(),
            ] {
                let transform = if part == head {
                    head_transform
                } else {
                    head_transform * world.get::<Transform>(part).unwrap().compute_affine()
                };
                let low = direct_vertices(world, meshes, part)
                    .into_iter()
                    .map(|p| transform.transform_point3(p).y)
                    .fold(f32::INFINITY, f32::min);
                assert!(
                    low >= -0.015,
                    "{model:?} state={state} part={part:?}: emitted mouth penetrates floor y={low}"
                );
            }
        }
    }
}

#[test]
fn rattlesnake_default_rest_has_an_extended_body_with_a_raised_head_and_rattle() {
    for quiet in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Rattlesnake,
                42,
                Some(Variant { build, ..default() }),
            );
            let (mut app, root) = animate_look(look, 12, |_| Vec3::ZERO);
            app.world_mut().get_mut::<Anim>(root).unwrap().quiet = quiet;
            app.update();
            let world = app.world();
            let rig = world.get::<Rig>(root).unwrap();
            let points: Vec<_> = rig
                .segments
                .iter()
                .map(|e| world.get::<Transform>(*e).unwrap().translation)
                .collect();
            let head = points[0];
            let tail = points[points.len() - 1];
            assert!(head.with_y(0.0).distance(tail.with_y(0.0))>0.85,
                "build={build} quiet={quiet}: default rest curls the body into a coil: head={head:?},tail={tail:?}");
            for segment in points.windows(2) {
                assert!(
                    (0.045..0.078).contains(&segment[0].distance(segment[1])),
                    "extended segments must remain connected"
                );
            }
            assert!(
                head.y > points[10].y + 0.09,
                "head stays raised over the extended body"
            );
            assert!(tail.y > points[14].y + 0.04, "warning rattle stays lifted");
        }
    }
}
