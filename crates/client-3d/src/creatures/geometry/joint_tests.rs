//! Articulated flesh must cover terminal disks, while closed buried caps remain valid.
use super::*;
use crate::creatures::rogue_tests::spawn_one;
use bevy::render::mesh::VertexAttributeValues;

#[derive(Clone, Copy)]
struct Face {
    p: [Vec3; 3],
    color: [Vec3; 3],
}

impl Face {
    fn normal(self) -> Vec3 {
        (self.p[1] - self.p[0])
            .cross(self.p[2] - self.p[0])
            .normalize_or_zero()
    }
    fn area(self) -> f32 {
        (self.p[1] - self.p[0])
            .cross(self.p[2] - self.p[0])
            .length()
            * 0.5
    }
    fn moved(self, rotation: Quat, offset: Vec3) -> Self {
        Self {
            p: self.p.map(|p| rotation * p + offset),
            ..self
        }
    }
}

fn faces(world: &World, meshes: &Assets<Mesh>, joint: Entity) -> Vec<Face> {
    let mut out = Vec::new();
    for child in world.get::<Children>(joint).unwrap().iter() {
        let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
            continue;
        };
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("positions")
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("colors")
        };
        let transform = world.get::<Transform>(child).unwrap().compute_affine();
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for tri in indices.as_chunks::<3>().0 {
            out.push(Face {
                p: tri.map(|i| transform.transform_point3(points[i].into())),
                color: tri.map(|i| Vec3::new(colors[i][0], colors[i][1], colors[i][2])),
            });
        }
    }
    assert!(!out.is_empty());
    out
}

fn ray(face: Face, origin: Vec3, direction: Vec3) -> Option<f32> {
    let e1 = face.p[1] - face.p[0];
    let e2 = face.p[2] - face.p[0];
    let h = direction.cross(e2);
    let det = e1.dot(h);
    if det.abs() < 1e-9 {
        return None;
    }
    let s = origin - face.p[0];
    let u = s.dot(h) / det;
    if !(-1e-5..=1.00001).contains(&u) {
        return None;
    }
    let q = s.cross(e1);
    let v = direction.dot(q) / det;
    if v < -1e-5 || u + v > 1.00001 {
        return None;
    }
    let distance = e2.dot(q) / det;
    (distance > 1e-6).then_some(distance)
}

fn first_hit(surface: &[Face], origin: Vec3, direction: Vec3) -> Option<f32> {
    surface
        .iter()
        .filter_map(|f| ray(*f, origin, direction))
        .min_by(f32::total_cmp)
}

// Identify actual planar terminal faces geometrically, without depending on
// the emitter's ring count, cap indices, or hard-coded endpoint multiplier.
fn terminal_disks(surface: &[Face], axis: Vec3, distal: bool) -> Vec<Face> {
    let extent = surface
        .iter()
        .flat_map(|f| f.p)
        .map(|p| p.dot(axis))
        .reduce(if distal { f32::max } else { f32::min })
        .unwrap();
    surface
        .iter()
        .copied()
        .filter(|f| {
            f.area() > 1e-9
                && f.normal().dot(axis).abs() > 0.999
                && f.p.iter().all(|p| (p.dot(axis) - extent).abs() < 1e-5)
        })
        .collect()
}

fn exposed_area(caps: &[Face], surface: &[Face], outward: Vec3) -> f32 {
    let mut area = 0.0;
    for cap in caps {
        for bary in [
            Vec3::splat(1.0 / 3.0),
            Vec3::new(0.6, 0.2, 0.2),
            Vec3::new(0.2, 0.6, 0.2),
            Vec3::new(0.2, 0.2, 0.6),
        ] {
            let sample = cap.p[0] * bary.x + cap.p[1] * bary.y + cap.p[2] * bary.z;
            // Only a terminal disk that is the first actual surface from
            // outside contributes. Caps buried beneath flesh are permitted.
            let hit = first_hit(surface, sample + outward * 1.5, -outward).unwrap();
            if hit >= 1.5 - 0.0001 {
                area += cap.area() * 0.25;
            }
        }
    }
    area
}

#[test]
fn articulated_joint_checker_distinguishes_exposed_and_buried_terminal_faces() {
    let cap = Face {
        p: [
            Vec3::new(-0.1, -0.1, 0.0),
            Vec3::new(0.1, -0.1, 0.0),
            Vec3::new(0.0, 0.1, 0.0),
        ],
        color: [Vec3::ONE; 3],
    };
    let cover = cap.moved(Quat::IDENTITY, Vec3::Z * 0.02);
    assert!(exposed_area(&[cap], &[cap], Vec3::Z) > 0.019);
    assert!(exposed_area(&[cap], &[cap, cover], Vec3::Z) < 1e-7);
    assert_eq!(terminal_disks(&[cap], Vec3::Z, true).len(), 1);
}

#[test]
fn articulated_joint_flesh_covers_terminal_disks_through_bent_poses() {
    let mut failures = Vec::new();
    for model in [
        CreatureModel::Jabberwock,
        CreatureModel::Aquator,
        CreatureModel::Xeroc,
    ] {
        for build in 0..3 {
            let look = Look::with_choice(model, 42, Some(Variant { build, ..default() }));
            let (world, rig, _, meshes) = spawn_one(&look);
            let body = faces(&world, &meshes, rig.joint(J::Body).unwrap());
            for (upper, lower, foot) in [
                (J::FL, J::FLs, J::HoofFL),
                (J::FR, J::FRs, J::HoofFR),
                (J::BL, J::BLs, J::HoofBL),
                (J::BR, J::BRs, J::HoofBR),
            ] {
                let a = rig.rest[lower as usize];
                let b = rig.rest[foot as usize];
                let first = faces(&world, &meshes, rig.joint(upper).unwrap());
                let second = faces(&world, &meshes, rig.joint(lower).unwrap());
                // Cap plane normals require the inverse transpose under the
                // actual nonuniform Storybook scale, unlike bone positions.
                let upper_axis = (a / storybook_part(rig.plan, upper)).normalize();
                let lower_axis = (b / storybook_part(rig.plan, lower)).normalize();
                let upper_caps = terminal_disks(&first, upper_axis, true);
                let lower_caps = terminal_disks(&second, lower_axis, false);
                let bend_axis = a.cross(b).normalize_or(Vec3::X);
                for angle in [-0.35, 0.0, 0.35] {
                    let rotation = Quat::from_axis_angle(bend_axis, angle);
                    let mut surface: Vec<_> =
                        first.iter().map(|f| f.moved(Quat::IDENTITY, -a)).collect();
                    surface.extend(second.iter().map(|f| f.moved(rotation, Vec3::ZERO)));
                    let joint_face_count = surface.len();
                    surface.extend(
                        body.iter()
                            .map(|f| f.moved(Quat::IDENTITY, -rig.rest[upper as usize] - a)),
                    );
                    let caps_a: Vec<_> = upper_caps
                        .iter()
                        .map(|f| f.moved(Quat::IDENTITY, -a))
                        .collect();
                    let caps_b: Vec<_> = lower_caps
                        .iter()
                        .map(|f| f.moved(rotation, Vec3::ZERO))
                        .collect();
                    let exposed = exposed_area(&caps_a, &surface, upper_axis)
                        + exposed_area(&caps_b, &surface, -(rotation * lower_axis));
                    // 0.3 square centimetres is already a readable flat patch
                    // on these 7–10 cm joints; microscopic rim contacts are not.
                    if exposed > 0.00003 {
                        failures.push(format!("{model:?} build={build} {lower:?} bend={angle:.2}: exposed terminal area={exposed:.7} m2"));
                    }
                    // An empty seam cannot pass merely by removing its caps.
                    // Actual flesh must surround the articulated joint centre.
                    for direction in [
                        Vec3::X,
                        Vec3::NEG_X,
                        Vec3::Y,
                        Vec3::NEG_Y,
                        Vec3::Z,
                        Vec3::NEG_Z,
                    ] {
                        // Cast inward from outside: an internal buried cap is
                        // not the envelope and must not shorten this witness.
                        let reach =
                            first_hit(&surface[..joint_face_count], direction * 0.6, -direction);
                        assert!(reach.is_some_and(|r| (0.1..=0.575).contains(&r)),
                            "{model:?} build={build} {lower:?} bend={angle}: uncovered joint in {direction:?}: {reach:?}");
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn linear(pigment: Vec3) -> Vec3 {
    let c = Color::srgb(pigment.x, pigment.y, pigment.z).to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

fn pigment_area(surface: &[Face], pigment: Vec3) -> f32 {
    let color = linear(pigment);
    surface
        .iter()
        .filter(|f| f.color.iter().all(|c| c.distance(color) < 1e-5))
        .map(|f| f.area())
        .sum()
}

#[test]
fn articulated_joint_xeroc_shanks_and_pads_keep_skin_with_pale_nails() {
    let mut failures = Vec::new();
    for build in 0..3 {
        let look = Look::with_choice(
            CreatureModel::Xeroc,
            42,
            Some(Variant { build, ..default() }),
        );
        let (world, rig, _, meshes) = spawn_one(&look);
        for (shin, foot) in [
            (J::FLs, J::HoofFL),
            (J::FRs, J::HoofFR),
            (J::BLs, J::HoofBL),
            (J::BRs, J::HoofBR),
        ] {
            let shank = faces(&world, &meshes, rig.joint(shin).unwrap());
            let pad = faces(&world, &meshes, rig.joint(foot).unwrap());
            for (name, surface) in [("shank", &shank), ("pad", &pad)] {
                let skin = pigment_area(surface, look.skin);
                let cloth = pigment_area(surface, look.cloth * 0.9);
                if skin < 0.01 || cloth > 1e-6 {
                    failures.push(format!("build={build} {shin:?} {name}: purple skin area={skin:.5}, tan cloth area={cloth:.5}"));
                }
            }
            assert!(
                pigment_area(&pad, BONE * 0.85) > 0.001,
                "build={build} {foot:?}: missing pale nails"
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn reparented_mesh_preserves_placement_across_proportions_and_parent_pose() {
    let world = Affine3A::from_rotation_translation(
        Quat::from_rotation_y(0.7) * Quat::from_rotation_z(-0.3),
        Vec3::new(3.0, -2.0, 0.5),
    );
    for (plan, parent, child, pivot) in [
        (Plan::Biped, J::Hips, J::Torso, Vec3::new(0.0, 0.12, -0.01)),
        (Plan::Bird, J::ShinL, J::FootL, BIRD_ANKLE),
    ] {
        let conversion = reparent_mesh_transform(plan, parent, child, pivot);
        for vertex in [
            Vec3::ZERO,
            Vec3::new(-0.2, 0.03, 0.14),
            Vec3::new(0.1, -0.3, -0.2),
        ] {
            let original = world.transform_point3(storybook_part(plan, parent) * vertex);
            let moved = world.transform_point3(
                pivot + storybook_part(plan, child) * conversion.transform_point3(vertex),
            );
            assert!(
                original.distance(moved) < 1e-6,
                "{parent:?} -> {child:?}: {original:?} != {moved:?}"
            );
        }
    }
}
