use super::anatomy::{
    storybook_part, BootLast, BootShape, Bust, HumanoidBody, LimbShape, ReptileHead,
};
use super::animation::ease;
use super::outfit::{
    Arms, Baldric, Bodice, Composition, Cover, Feet, Headwear, Hem, Legs, NeckDrape, Neckline,
    Outer, Outfit, ShellOpening, Shoes, Skirt, SleeveCut, Straps, Support, TorsoOrnament,
};
use super::outfit::{Heraldry, Pattern};
use super::*;

mod accessory_parts;
mod armor_parts;
mod beast_parts;
mod boot_parts;
#[cfg(test)]
mod boot_tests;
#[cfg(test)]
mod dragon_tests;
mod embroidery_parts;
mod equine_parts;
#[cfg(test)]
mod equine_tests;
mod feather_parts;
mod fringe_parts;
mod fur_parts;
mod garment_details;
#[cfg(test)]
mod griffin_tests;
mod headwear_parts;
mod membrane_parts;
#[cfg(test)]
mod membrane_tests;
mod neckline_parts;
mod ornament_parts;
mod panel_parts;
mod raptor_parts;
mod royal_parts;
mod sleeve_parts;
mod waist_parts;

#[cfg(test)]
mod serpent_tests;

#[cfg(test)]
mod bird_tests;
pub(super) mod rogue;

#[cfg(test)]
mod amphibian_limb_tests;

#[cfg(test)]
mod aquator_tests;

#[cfg(test)]
mod flytrap_tests;

#[cfg(test)]
mod bust_tests;

#[cfg(test)]
mod wardrobe_parts_tests;

#[cfg(test)]
mod eye_tests;

#[cfg(test)]
mod garment_tests;

#[cfg(test)]
mod joint_tests;

#[cfg(test)]
mod xeroc_tests;

#[cfg(test)]
mod art_contract_tests {

    #[test]
    fn art_contract_fitted_cloth_keeps_closed_front_edges_coincident() {
        let under = super::ClothSurface::new(
            super::SkirtCut::straight(
                bevy::prelude::Vec2::new(0.22, 0.18),
                bevy::prelude::Vec2::new(0.25, 0.20),
            ),
            0.16,
            8,
            0.0,
            4,
        );
        for split in [0.0, 0.06] {
            let plain = super::ClothSurface::new(
                super::SkirtCut::straight(
                    bevy::prelude::Vec2::new(0.14, 0.12),
                    bevy::prelude::Vec2::new(0.22, 0.16),
                ),
                0.08,
                6,
                split,
                2,
            );
            let fitted = plain.clone().fit_over(
                under.clone(),
                bevy::math::Affine3A::from_translation(bevy::prelude::Vec3::new(
                    0.025, -0.035, 0.004,
                )),
                true,
            );
            for row in 0..=plain.rows() {
                let t = row as f32 / plain.rows() as f32;
                for inner in [false, true] {
                    let seam = |surface: &super::ClothSurface| {
                        surface.point(t, 0.0, inner).distance(surface.point(
                            t,
                            std::f32::consts::TAU,
                            inner,
                        ))
                    };
                    if seam(&plain) < 1e-6 {
                        assert!(
                            seam(&fitted) < 1e-6,
                            "split {split}, row {row}, inner {inner}: fitted closed seam opened {}",
                            seam(&fitted)
                        );
                    }
                }
            }
        }
    }
    use super::*;

    #[test]
    fn art_contract_pauldrons_clear_the_resolved_arm_on_every_build() {
        for kind in [
            CreatureModel::Human,
            CreatureModel::Orc,
            CreatureModel::Nymph,
        ] {
            for woman in [false, true] {
                for build in 0..3 {
                    let look = Look::with_choice(
                        kind,
                        2,
                        Some(Variant {
                            woman,
                            build,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    for side in [-1.0, 1.0] {
                        let arm = upper_arm_form(
                            side,
                            body.build,
                            body.arm_length,
                            body.profile.muscle,
                            0.94,
                        );
                        let mut parts = Parts::default();
                        biped_pauldron(&mut parts, side, body);
                        let mut materials = Assets::<StandardMaterial>::default();
                        let mut meshes = Assets::<Mesh>::default();
                        let model = parts.finish(
                            &mut meshes,
                            &crate::props::PropMaterials::new(&mut materials),
                        );
                        let mut minimum = f32::INFINITY;
                        for (handle, _) in &model.0 {
                            let mesh = meshes.get(handle).unwrap();
                            let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(
                                positions,
                            )) = mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                            else {
                                panic!("plate positions")
                            };
                            let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
                            for tri in indices.as_chunks::<3>().0 {
                                let v = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(positions[i]));
                                // Facet interiors must clear the body too; checking
                                // only ring vertices would miss chords through it.
                                for at in [
                                    v[0],
                                    v[1],
                                    v[2],
                                    (v[0] + v[1] + v[2]) / 3.0,
                                    (v[0] + v[1]) * 0.5,
                                    (v[1] + v[2]) * 0.5,
                                    (v[2] + v[0]) * 0.5,
                                ] {
                                    minimum = minimum.min(arm.distance(at));
                                }
                            }
                        }
                        assert!(minimum >= 0.003, "shoulder plate penetrates arm: {kind:?}, woman={woman}, build={build}, side={side}, clearance={minimum}");
                    }
                }
            }
        }
    }

    #[test]
    fn art_contract_buckle_has_open_windows_a_frame_and_a_tongue() {
        let mut parts = Parts::default();
        buckle_frame(
            &mut parts.metal,
            Vec3::ZERO,
            Quat::IDENTITY,
            Vec2::new(0.034, 0.027),
            GILT,
        );
        let mut materials = Assets::<StandardMaterial>::default();
        let mut meshes = Assets::<Mesh>::default();
        let model = parts.finish(
            &mut meshes,
            &crate::props::PropMaterials::new(&mut materials),
        );
        // Orthographic rays through the hardware's local face. Ignore
        // projected edge-on triangles; visible metal must bound real openings.
        let metal_at = |point: Vec2| {
            model.0.iter().any(|(handle, _)| {
                let mesh = meshes.get(handle).unwrap();
                let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("buckle positions")
                };
                let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
                indices.as_chunks::<3>().0.iter().any(|tri| {
                    let v = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(positions[i]).truncate());
                    let cross = |a: Vec2, b: Vec2, c: Vec2| (b - a).perp_dot(c - a);
                    if cross(v[0], v[1], v[2]).abs() < 1e-9 {
                        return false;
                    }
                    let d = [
                        cross(v[0], v[1], point),
                        cross(v[1], v[2], point),
                        cross(v[2], v[0], point),
                    ];
                    d.iter().all(|&x| x >= -1e-9) || d.iter().all(|&x| x <= 1e-9)
                })
            })
        };
        assert!(
            metal_at(Vec2::new(0.032, 0.0)),
            "the side frame carries the strap"
        );
        assert!(metal_at(Vec2::ZERO), "the tongue crosses the middle");
        for side in [-1.0, 1.0] {
            assert!(
                !metal_at(Vec2::new(side * 0.012, 0.011)),
                "leather must show through both buckle windows"
            );
        }
    }

    #[test]
    fn art_contract_armored_baldric_has_no_overlapping_folds() {
        let look = Look::with_choice(
            CreatureModel::Human,
            2,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let body = HumanoidBody::resolve(&look);
        let plate = torso_garment(body, TorsoMaterial::Plate, true);
        let mut parts = Parts::default();
        fitted_baldric(
            &mut parts,
            &plate,
            body,
            Baldric {
                color: LEATHER,
                reverse: true,
                buckle: true,
            },
            EMBLEM_FIELD_OFFSET,
        );
        // Inspect the front strip alone. A camera ray through an unfolded, closed
        // strip enters and exits it once; extra depth layers expose a fold.
        parts.metal = Builder::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut meshes = Assets::<Mesh>::default();
        let model = parts.finish(
            &mut meshes,
            &crate::props::PropMaterials::new(&mut materials),
        );
        let mut triangles = Vec::new();
        for (handle, _) in model.0 {
            let mesh = meshes.get(&handle).unwrap();
            let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("strap positions")
            };
            let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
            triangles.extend(
                indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .map(|tri| tri.map(|i| Vec3::from(positions[i]))),
            );
        }
        let mut sampled = 0;
        for iy in 0..70 {
            for ix in 0..60 {
                let point = Vec2::new(-0.15 + ix as f32 * 0.005, 0.05 + iy as f32 * 0.005);
                let mut depths = Vec::new();
                for &[a, b, c] in &triangles {
                    let (a2, b2, c2) = (a.truncate(), b.truncate(), c.truncate());
                    let area = (b2 - a2).perp_dot(c2 - a2);
                    if area.abs() < 1e-9 {
                        continue;
                    }
                    let u = (b2 - point).perp_dot(c2 - point) / area;
                    let v = (c2 - point).perp_dot(a2 - point) / area;
                    let w = 1.0 - u - v;
                    if u >= 0.0 && v >= 0.0 && w >= 0.0 {
                        let depth = u * a.z + v * b.z + w * c.z;
                        if depth > 0.0 {
                            depths.push(depth);
                        }
                    }
                }
                depths.sort_by(f32::total_cmp);
                depths.dedup_by(|a, b| (*a - *b).abs() < 0.0005);
                if !depths.is_empty() {
                    sampled += 1;
                }
                assert!(
                    depths.len() <= 2,
                    "strap folds over itself at {point}: {} depth layers",
                    depths.len()
                );
            }
        }
        assert!(
            sampled > 100,
            "sample a substantial area of the generated strap"
        );
    }

    #[test]
    fn art_contract_baldric_reaches_the_shoulder_and_waist_band() {
        let look = Look::with_choice(
            CreatureModel::Human,
            2,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let body = HumanoidBody::resolve(&look);
        let plate = torso_garment(body, TorsoMaterial::Plate, true);
        let mut parts = Parts::default();
        fitted_baldric(
            &mut parts,
            &plate,
            body,
            Baldric {
                color: LEATHER,
                reverse: true,
                buckle: true,
            },
            EMBLEM_FIELD_OFFSET,
        );
        let (low, high) = parts.satin.bounds();
        assert!(
            high.y >= body.shoulder(1.0).y - 0.025,
            "strap must reach the shoulder, not stop on the chest: {high}"
        );
        assert!(
            low.y <= 0.02 && low.y > -0.03,
            "strap must finish within the waist band: {low}"
        );
    }

    #[test]
    fn art_contract_baldric_is_one_closed_loop_without_open_seams() {
        use std::collections::{BTreeMap, BTreeSet};
        let look = Look::with_choice(
            CreatureModel::Human,
            2,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let body = HumanoidBody::resolve(&look);
        let plate = torso_garment(body, TorsoMaterial::Plate, true);
        let mut parts = Parts::default();
        fitted_baldric(
            &mut parts,
            &plate,
            body,
            Baldric {
                color: LEATHER,
                reverse: true,
                buckle: true,
            },
            EMBLEM_FIELD_OFFSET,
        );
        parts.metal = Builder::default();
        let mut materials = Assets::<StandardMaterial>::default();
        let mut meshes = Assets::<Mesh>::default();
        let model = parts.finish(
            &mut meshes,
            &crate::props::PropMaterials::new(&mut materials),
        );
        let mut vertices = BTreeMap::new();
        let mut edges = BTreeMap::<[usize; 2], usize>::new();
        let mut faces = 0;
        for (handle, _) in model.0 {
            let mesh = meshes.get(&handle).unwrap();
            let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("strap positions")
            };
            let indices: Vec<usize> = mesh.indices().unwrap().iter().collect();
            for tri in indices.as_chunks::<3>().0 {
                let tri = tri.map(|i| {
                    let key = positions[i].map(|v| (v as f64 * 1_000_000.0).round() as i64);
                    let next = vertices.len();
                    *vertices.entry(key).or_insert(next)
                });
                assert!(
                    tri[0] != tri[1] && tri[1] != tri[2] && tri[2] != tri[0],
                    "no collapsed strap faces"
                );
                faces += 1;
                for [a, b] in [[tri[0], tri[1]], [tri[1], tri[2]], [tri[2], tri[0]]] {
                    *edges.entry([a.min(b), a.max(b)]).or_default() += 1;
                }
            }
        }
        assert!(!vertices.is_empty());
        assert!(
            edges.values().all(|&uses| uses == 2),
            "strap must have no open or non-manifold seams"
        );
        let mut neighbours = vec![Vec::new(); vertices.len()];
        for &[a, b] in edges.keys() {
            neighbours[a].push(b);
            neighbours[b].push(a);
        }
        let mut visited = BTreeSet::new();
        let mut pending = vec![0];
        while let Some(v) = pending.pop() {
            if visited.insert(v) {
                pending.extend(&neighbours[v]);
            }
        }
        assert_eq!(visited.len(), vertices.len(), "one continuous accessory");
        assert_eq!(
            vertices.len() + faces,
            edges.len(),
            "a closed strap must have one loop, not capped ends"
        );
    }

    #[test]
    fn art_contract_long_hair_falls_in_locks_and_tails_lie_along_the_nape() {
        let look = |woman: bool, hair: u8| {
            Look::with_choice(
                CreatureModel::Human,
                3,
                Some(Variant {
                    woman,
                    hair,
                    ..default()
                }),
            )
        };
        let mut long = Parts::default();
        hair(&mut long, &look(true, 0), true);
        let (low, high) = long.matte.bounds();
        assert!(low.y < -0.18, "long hair reaches well down the back: {low}");
        assert!(
            high.x > 0.14 && low.x < -0.14,
            "locks fall over both shoulders"
        );
        // Sample the emitted surface, not an analytic surrogate that can
        // remain green after the renderer changes representation.
        for k in -4..=4 {
            let x = k as f32 * 0.02;
            let behind = long
                .matte
                .outer_surface(Vec3::new(x, 0.05, 0.4), -Vec3::Z)
                .expect("rendered fall covers the nape");
            assert!(
                behind.z < -0.09,
                "the fall covers the nape at x {x}: {behind}"
            );
        }
        let tips = (0..=40)
            .map(|k| Vec3::new(-0.16 + k as f32 * 0.008, -0.19, -0.12))
            .filter(|&at| {
                let back = long.matte.outer_surface(at + Vec3::Z, -Vec3::Z);
                let front = long.matte.outer_surface(at - Vec3::Z, Vec3::Z);
                matches!((back, front), (Some(back), Some(front)) if back.z < at.z && front.z > at.z)
            })
            .count();
        assert!(
            tips > 0 && tips < 36,
            "the tips part into locks: {tips} of 41 inside"
        );
        let (low, high) = long.matte.bounds();
        assert!(
            high.z > 0.08 && low.y < -0.2,
            "locks lie forward over the chest: {high}"
        );
        let mut bob = Parts::default();
        hair(&mut bob, &look(true, 4), true);
        let (low, _) = bob.matte.bounds();
        assert!(
            low.y > -0.02 && low.y < 0.06,
            "a bob ends at the jaw: {low}"
        );
        let mut tied = Parts::default();
        hair(&mut tied, &look(false, 4), true);
        let (low, _) = tied.matte.bounds();
        assert!(
            low.z > -0.18,
            "a tied tail lies along the nape, not jutting back: {low}"
        );
    }

    #[test]
    fn art_contract_adult_brows_stay_seated_on_the_resolved_forehead() {
        use bevy::render::mesh::VertexAttributeValues;
        for model in [
            CreatureModel::Human,
            CreatureModel::Nymph,
            CreatureModel::Medusa,
            CreatureModel::Orc,
            CreatureModel::Vampire,
        ] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    42,
                    Some(Variant {
                        woman: true,
                        build,
                        ..default()
                    }),
                );
                let head = face_shape(&look, true).form;
                let mut parts = Parts::default();
                face_brows(&mut parts, &look, &head);
                let mut materials = Assets::<StandardMaterial>::default();
                let materials = crate::props::PropMaterials::new(&mut materials);
                let mut meshes = Assets::<Mesh>::default();
                let model_mesh = parts.finish(&mut meshes, &materials);
                let mut samples = 0;
                for (handle, _) in &model_mesh.0 {
                    let mesh = meshes.get(handle).unwrap();
                    let Some(VertexAttributeValues::Float32x3(points)) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                    else {
                        panic!("brow positions")
                    };
                    let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                    for triangle in indices.as_chunks::<3>().0 {
                        let q = triangle.map(|i| Vec3::from_array(points[i]));
                        for at in [
                            q[0],
                            q[1],
                            q[2],
                            (q[0] + q[1]) * 0.5,
                            (q[1] + q[2]) * 0.5,
                            (q[2] + q[0]) * 0.5,
                            (q[0] + q[1] + q[2]) / 3.0,
                        ] {
                            let gap = head.distance(at);
                            assert!(
                                (-0.0007..=0.0055).contains(&gap),
                                "{model:?}/{build}: brow must follow forehead, gap={gap} at={at}"
                            );
                            assert!(
                                at.y > 0.182,
                                "{model:?}/{build}: brow meets existing upper lid at {at}"
                            );
                            samples += 1;
                        }
                    }
                }
                assert!(samples > 100, "inspect actual emitted brow surfaces");
            }
        }
    }

    #[test]
    fn art_contract_adult_eye_openings_are_almonds_with_contained_irises() {
        use bevy::render::mesh::VertexAttributeValues;
        let look = Look::with_choice(
            CreatureModel::Human,
            42,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let head = face_shape(&look, true).form;
        let white = Vec3::new(0.82, 0.81, 0.76);
        let iris = Vec3::new(0.12, 0.43, 0.69);
        let mut parts = Parts::default();
        soft_eyes(
            &mut parts,
            Vec3::new(0.0, 0.16, 0.088),
            0.038,
            0.0158,
            iris,
            &look,
            &head,
        );
        let mut material_assets = Assets::<StandardMaterial>::default();
        let materials = crate::props::PropMaterials::new(&mut material_assets);
        let mut meshes = Assets::<Mesh>::default();
        let model = parts.finish(&mut meshes, &materials);
        let linear = |c: Vec3| {
            let c = Color::srgb(c.x, c.y, c.z).to_linear();
            Vec3::new(c.red, c.green, c.blue).normalize()
        };
        let triangles = |pigment: Vec3| {
            let mut result = Vec::new();
            for (handle, _) in &model.0 {
                let mesh = meshes.get(handle).unwrap();
                let Some(VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("eye positions")
                };
                let Some(VertexAttributeValues::Float32x4(colors)) =
                    mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                else {
                    panic!("eye colors")
                };
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                for tri in indices.as_chunks::<3>().0 {
                    if tri.iter().all(|&i| {
                        (Vec3::from_array(colors[i][..3].try_into().unwrap()).normalize()
                            - linear(pigment))
                        .length()
                            < 0.0001
                    }) {
                        result.push(tri.map(|i| Vec3::from_array(points[i])));
                    }
                }
            }
            assert!(!result.is_empty(), "positive pigment witness {pigment}");
            result
        };
        let whites = triangles(white);
        let irises = triangles(iris);
        for side in [-1.0, 1.0] {
            let points: Vec<_> = whites
                .iter()
                .flatten()
                .copied()
                .filter(|p| p.x * side > 0.0)
                .collect();
            let low = points
                .iter()
                .copied()
                .fold(Vec3::splat(f32::INFINITY), Vec3::min);
            let high = points
                .iter()
                .copied()
                .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
            let aspect = (high.y - low.y) / (high.x - low.x);
            assert!(
                (0.25..=0.52).contains(&aspect),
                "adult almond opening aspect={aspect}"
            );
        }
        let contains = |at: Vec2| {
            whites.iter().any(|tri| {
                let [a, b, c] = tri.map(Vec3::truncate);
                let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
                let d = cross(b - a, c - a);
                if d.abs() < 1e-12 {
                    return false;
                }
                let v = cross(at - a, c - a) / d;
                let w = cross(b - a, at - a) / d;
                v >= -0.0001 && w >= -0.0001 && v + w <= 1.0001
            })
        };
        for q in irises {
            for at in [
                q[0],
                q[1],
                q[2],
                (q[0] + q[1]) * 0.5,
                (q[1] + q[2]) * 0.5,
                (q[2] + q[0]) * 0.5,
                (q[0] + q[1] + q[2]) / 3.0,
            ] {
                assert!(
                    contains(at.truncate()),
                    "iris escapes emitted white at {at}"
                );
            }
        }
    }

    #[test]
    fn art_contract_glowing_eyes_keep_their_lids_and_light_only_the_iris() {
        let mut look = Look::with_choice(
            CreatureModel::Human,
            42,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let head = face_shape(&look, true).form;
        look.skin = SKIN;
        look.eyes = Some(Accent::Danger);
        let mut glowing = Parts::default();
        soft_eyes(
            &mut glowing,
            Vec3::new(0.0, 0.16, 0.088),
            0.04,
            0.0158,
            Vec3::splat(0.3),
            &look,
            &head,
        );
        assert_eq!(glowing.accent, Some(Accent::Danger));
        assert!(!glowing.glow.is_empty(), "the iris glows");
        assert!(
            !glowing.matte.is_empty(),
            "lids, lashes and the white stay solid"
        );
        look.v.woman = false;
        look.eyes = None;
        let mut plain = Parts::default();
        soft_eyes(
            &mut plain,
            Vec3::new(0.0, 0.16, 0.088),
            0.04,
            0.0145,
            Vec3::splat(0.3),
            &look,
            &head,
        );
        assert!(plain.glow.is_empty() && plain.accent.is_none());
    }

    #[test]
    fn art_contract_structured_front_is_continuous_and_keeps_a_fitted_waist() {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Human,
                42,
                Some(Variant {
                    woman: true,
                    build,
                    ..default()
                }),
            );
            let body = HumanoidBody::resolve(&look);
            let leather = torso_garment(body, TorsoMaterial::Leather, true);
            let plate = torso_garment(body, TorsoMaterial::Plate, true);
            let front =
                |shape: &crate::sdf::Sculpt, x, y| shape.onto(Vec3::new(x, y, 0.6), Vec3::NEG_Z).z;
            let apex = body.bust.unwrap().apex(1.0);
            let centre = front(&plate, 0.0, apex.y);
            let side = front(&plate, apex.x, apex.y);
            assert!(
                centre >= side - 0.01,
                "plate must span the central chest valley"
            );
            assert!(
                centre > front(&leather, 0.0, apex.y),
                "materials need different construction"
            );
            assert!(
                front(&plate, 0.0, 0.03) < centre * 0.85,
                "waist must remain fitted"
            );
            for x in [-0.08, 0.0, 0.08] {
                let at = plate.onto(Vec3::new(x, apex.y, 0.6), Vec3::NEG_Z);
                assert!(at.is_finite() && plate.distance(at).abs() < 0.001);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Building rigs.

struct Kit<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    library: &'a mut PropLibrary,
    meshes: &'a mut Assets<Mesh>,
    rig: Rig,
    key: String,
    /// The cache identity of parts that only some of the outfit shapes:
    /// the head (by its headwear and whether a gorget hides the neck) and
    /// hanging hair (by nothing worn at all). Redressing reuses them.
    head_key: String,
    hair_key: String,
    /// Every part is drawn translucent (a phantom).
    ghost: bool,
}

impl Kit<'_, '_, '_> {
    fn cloak(&mut self, parent: Entity, recipe: super::cloak::Recipe) {
        let cloak = super::cloak::spawn(
            self.commands,
            self.library,
            self.meshes,
            parent,
            recipe,
            self.ghost,
        );
        self.rig.drapes.push((cloak.entity, Drape::Cloak));
        self.rig.cloaks.push(cloak);
    }
    /// A joint at a rest offset from its parent, carrying one cached part.
    fn joint(
        &mut self,
        j: J,
        parent: Entity,
        at: Vec3,
        part: &str,
        build: impl FnOnce(&mut Parts),
    ) -> Entity {
        let model = self.part(part, build);
        let e = self
            .commands
            .spawn((Transform::from_translation(at), Visibility::default()))
            .id();
        let proportion = storybook_part(self.rig.plan, j);
        let path = match j {
            J::ArmL | J::ArmR if self.rig.plan == Plan::Biped => Some(vec![j]),
            J::ForeL if self.rig.plan == Plan::Biped => Some(vec![J::ArmL, j]),
            J::ForeR if self.rig.plan == Plan::Biped => Some(vec![J::ArmR, j]),
            _ => None,
        };
        if let Some(path) = path {
            self.rig
                .visual_contacts
                .push(super::visual_contact::Part::from_model(
                    path,
                    &model,
                    self.meshes,
                    proportion,
                ));
        }

        if model.1.is_empty() {
            self.commands
                .entity(e)
                .with_children(|p| model.attach_scaled(p, proportion));
        } else {
            model.attach_soft(self.commands, e, proportion);
        }
        self.commands.entity(parent).add_child(e);
        self.rig.joints[j as usize] = Some(e);
        self.rig.rest[j as usize] = at;
        e
    }

    fn part(&mut self, part: &str, build: impl FnOnce(&mut Parts)) -> Model {
        let soft = true;
        // A phantom's face and hair stay solid, as on its sheet, so its
        // hair can never show through its face; the rest is translucent.
        // Long hair hangs as its own part, and stays solid with the head.
        let hair_part = part == "hair" || part.starts_with("hair/");
        let ghost = self.ghost && part != "head" && !hair_part;
        self.library.get(
            format!(
                "creature/{}/{part}",
                match part {
                    "head" => &self.head_key,
                    _ if hair_part => &self.hair_key,
                    _ => &self.key,
                }
            ),
            self.meshes,
            || {
                let mut p = Parts::default();
                if soft {
                    p.soften();
                }
                build(&mut p);
                if ghost {
                    p.ghostify();
                }
                p
            },
        )
    }

    /// Hanging cloth on its own entity, swung by the animation.
    fn drape(
        &mut self,
        parent: Entity,
        at: Transform,
        kind: Drape,
        part: &str,
        build: impl FnOnce(&mut Parts),
    ) {
        self.drape_scaled(parent, at, kind, part, Vec3::ONE, build);
    }

    /// A drape carrying the Storybook proportion of the part it hangs from,
    /// as if it were still built into that part.
    fn drape_scaled(
        &mut self,
        parent: Entity,
        at: Transform,
        kind: Drape,
        part: &str,
        proportion: Vec3,
        build: impl FnOnce(&mut Parts),
    ) -> Entity {
        let model = self.part(part, build);
        let e = self.commands.spawn((at, Visibility::default())).id();
        self.commands
            .entity(e)
            .with_children(|p| model.attach_scaled(p, proportion));
        self.commands.entity(parent).add_child(e);
        self.rig.drapes.push((e, kind));
        e
    }

    fn attach(
        &mut self,
        parent: Entity,
        at: Transform,
        part: &str,
        build: impl FnOnce(&mut Parts),
    ) {
        let model = self.part(part, build);
        let e = self.commands.spawn((at, Visibility::default())).id();
        let proportion = if part == "cloak" {
            Vec3::new(1.18, 1.0, 1.0)
        } else {
            Vec3::ONE
        };
        self.commands
            .entity(e)
            .with_children(|p| model.attach_scaled(p, proportion));
        self.commands.entity(parent).add_child(e);
    }
}

/// Spawn a creature's rig under `parent` (the actor's body entity).
pub fn spawn(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    parent: Entity,
    look: &Look,
) -> Rig {
    let mut kit = Kit {
        commands,
        library,
        meshes,
        rig: Rig {
            plan: look.plan,
            humanoid: (look.plan == Plan::Biped).then(|| HumanoidBody::resolve(look)),
            joints: [None; JOINTS],
            rest: [Vec3::ZERO; JOINTS],
            segments: Vec::new(),
            stride: look.stride,
            hunch: look.hunch,
            style: look.style,
            scale: look.scale,
            robe: None,
            cloaks: Vec::new(),
            hair: None,
            visual_contacts: Vec::new(),
            ankle_floor: 0.081,
            legs: 1.0,
            limb_roles: [LimbRole::Support; 4],
            girth: 0.2,
            drapes: Vec::new(),
            skirt_layers: Vec::new(),
        },
        // Catalog keys alone do not describe direct authored-recipe overrides.
        // Keep the complete local value, rather than a collision-prone truncated
        // digest; these entries retain the library's existing eviction lifetime.
        // Head and immutable hair retain their narrower dependency keys below.
        key: match &look.outfit {
            Some(recipe) => format!("{}:recipe:{recipe:?}", look.key),
            None => look.key.clone(),
        },
        head_key: {
            let outfit = Outfit::worn(look);
            format!(
                "{}:{:?}:{}:{:?}",
                look.body_key.as_deref().unwrap_or(&look.key),
                outfit.head,
                outfit.outer == Some(Outer::Cuirass),
                outfit.trimmings.earrings
            )
        },
        hair_key: look.body_key.clone().unwrap_or_else(|| look.key.clone()),
        ghost: look.style.ghostly(),
    };
    let lift = kit
        .commands
        .spawn((
            Transform::from_scale(Vec3::splat(look.scale)),
            Visibility::default(),
        ))
        .id();
    kit.commands.entity(parent).add_child(lift);
    kit.rig.joints[J::Lift as usize] = Some(lift);
    match look.plan {
        Plan::Biped => biped(&mut kit, lift, look),
        Plan::Bird if look.style.flies() => rogue::flier(&mut kit, lift, look),
        Plan::Bird => bird(&mut kit, lift, look),
        Plan::Quadruped if look.style.rogue_beast() => rogue::beast(&mut kit, lift, look),
        Plan::Quadruped => quadruped(&mut kit, lift, look, false),
        Plan::Rooted => rogue::flytrap(&mut kit, lift, look),
        Plan::Centaur => quadruped(&mut kit, lift, look, true),
        Plan::Serpent => serpent(&mut kit, lift, look),
        Plan::Floater => floater(&mut kit, lift, look),
        Plan::Marker => {
            let marker = kit.part("placeholder", |p| {
                p.matte.blob(
                    Vec3::Y * 0.65,
                    Vec3::new(0.36, 0.55, 0.36),
                    0,
                    0.0,
                    Vec3::new(0.38, 0.6, 0.68),
                );
            });
            kit.commands
                .entity(lift)
                .with_children(|parent| marker.attach(parent));
        }
    }
    if let Some((color, intensity)) = look.light() {
        let at = if look.plan == Plan::Floater {
            Vec3::Y * 1.1
        } else {
            Vec3::Y * 1.2
        };
        let light = kit
            .commands
            .spawn((
                PointLight {
                    color,
                    intensity,
                    range: 5.0,
                    shadows_enabled: false,
                    ..default()
                },
                Transform::from_translation(at),
            ))
            .id();
        kit.commands.entity(lift).add_child(light);
    }
    kit.rig
}

fn eyes(p: &mut Parts, accent: Option<Accent>, center: Vec3, spread: f32, size: f32) {
    if let Some(accent) = accent {
        p.accent = Some(accent);
        for side in [-1.0, 1.0] {
            p.glow.blob(
                center + Vec3::X * side * spread,
                Vec3::splat(size),
                0,
                0.0,
                Vec3::ONE,
            );
        }
    } else {
        cartoon_eyes(p, center, spread, size, Vec3::new(0.36, 0.22, 0.11));
    }
}

/// The colour of this individual's hair, by kind.
fn hair_color(l: &Look) -> Vec3 {
    let t = l.v.hair_tone as usize;
    match l.style {
        Style::Vampire => [HAIRS[0], HAIRS[0], HAIRS[1], HAIRS[5], HAIRS[0], HAIRS[1]][t],
        Style::Hobgoblin => [
            DARK * 3.0,
            HAIRS[0],
            HAIRS[1],
            DARK * 3.0,
            HAIRS[1],
            HAIRS[0],
        ][t],
        Style::Zombie => HAIRS[t] * 0.75 + Vec3::splat(0.05),
        Style::Orc | Style::Troll => {
            [HAIRS[0], HAIRS[1], DARK * 3.0, HAIRS[0], HAIRS[1], HAIRS[1]][t]
        }
        // Every leprechaun is a redhead, from copper to deep auburn.
        Style::Leprechaun => [
            Vec3::new(0.78, 0.32, 0.1),
            Vec3::new(0.7, 0.26, 0.08),
            Vec3::new(0.86, 0.4, 0.14),
            Vec3::new(0.6, 0.22, 0.08),
            Vec3::new(0.82, 0.36, 0.12),
            Vec3::new(0.66, 0.28, 0.12),
        ][t],
        Style::Nymph => [HAIRS[4], HAIRS[2], HAIRS[1], HAIRS[4], HAIRS[3], HAIRS[2]][t],
        // The dead go white: silver, ash and bone.
        Style::Phantom | Style::Wraith => [
            Vec3::new(0.86, 0.86, 0.9),
            Vec3::new(0.78, 0.8, 0.86),
            Vec3::new(0.9, 0.88, 0.84),
            Vec3::new(0.72, 0.74, 0.8),
            Vec3::new(0.84, 0.84, 0.86),
            Vec3::new(0.8, 0.78, 0.82),
        ][t],
        _ => HAIRS[t],
    }
}

/// A sculpted lock of hair: one solid clump, broad at the root and tapering
/// to its tip, laid along `path`.
fn lock(b: &mut Builder, path: &[Vec3], width: f32, thick: f32, color: Vec3) {
    let n = path.len().max(2);
    let radii: Vec<Vec2> = (0..path.len())
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            Vec2::new(
                (width * (1.0 - 0.97 * t * t)).max(0.0015),
                (thick * (1.0 - 0.95 * t * t)).max(0.0015),
            )
        })
        .collect();
    // Broad side along the scalp: across the lock and around the head.
    let tangent = (path[1] - path[0]).normalize_or(Vec3::Y);
    let outward = (path[0] - Vec3::new(0.0, 0.17, -0.005)).normalize_or(Vec3::Y);
    let across = tangent.cross(outward).normalize_or(Vec3::X);
    b.tube_framed(path, &radii, 10, &|_, _, _| color, Some(across));
}

/// One strand of a sculpted hairstyle: points down its length and its
/// radius at the root and the tip.
struct Strand {
    attached: Vec<bool>,
    support_side: Option<super::deformation::SupportSide>,
    path: Vec<Vec3>,
    root: f32,
    tip: f32,
}

const HAIR_SCALP_END_Y: f32 = 0.1;

fn long_hair_strands(seed: u64) -> Vec<Strand> {
    let skull = Vec3::new(0.0, 0.17, -0.005);
    // The skull's horizontal radius at a height, a little proud of the scalp.
    let around = |y: f32| {
        let h = ((y - skull.y) / 0.118).clamp(-1.0, 1.0);
        (1.0 - h * h).max(0.0).sqrt()
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let mut strands = Vec::new();
    // Down the back, from one ear round to the other.
    let count = 11;
    for k in 0..count {
        let a = (k as f32 / (count - 1) as f32 - 0.5) * 2.3;
        let reach = -0.21 - 0.05 * rand(seed, 60 + k);
        let phase = rand(seed, 90 + k) * TAU;
        // Six segments carry its slow waves; each extra one costs every
        // sample of the sculpt.
        let path: Vec<Vec3> = (0..=6)
            .map(|i| {
                let s = i as f32 / 6.0;
                let y = lerp(0.27, reach, s);
                // Over the skull, then hanging clear of the neck and
                // spreading a little over the shoulder blades.
                let (wide, deep) = if y > HAIR_SCALP_END_Y {
                    let r = around(y);
                    (0.112 * r.max(0.62), 0.122 * r.max(0.62))
                } else {
                    let below = (HAIR_SCALP_END_Y - y) / (HAIR_SCALP_END_Y - reach);
                    (lerp(0.112 * 0.62, 0.15, below), lerp(0.138, 0.105, below))
                };
                // Slow, broad waves: the fall moves as one sheet of hair.
                let wave = (y * 9.0 + phase).sin() * 0.014 * s;
                Vec3::new(
                    a.sin() * wide + a.cos() * wave,
                    y,
                    skull.z - a.cos() * deep - 0.012 + a.sin().abs() * 0.02,
                )
            })
            .collect();
        strands.push(Strand {
            attached: path.iter().map(|p| p.y > HAIR_SCALP_END_Y).collect(),
            support_side: Some(super::deformation::SupportSide::Back),
            path,
            root: 0.03,
            tip: 0.008,
        });
    }
    for side in [-1.0f32, 1.0] {
        // Two locks spilling forward over each shoulder onto the chest, as
        // on the sheets: down past the ear, over the collarbone, and down.
        for (k, (depth, reach)) in [(0.03f32, -0.25f32), (-0.01, -0.21)]
            .into_iter()
            .enumerate()
        {
            let phase = rand(seed, 70 + k as u32 + side.max(0.0) as u32 * 4) * TAU;
            let path: Vec<Vec3> = (0..=6)
                .map(|i| {
                    let s = i as f32 / 6.0;
                    let y = lerp(0.22, reach, s);
                    let wave = (y * 10.0 + phase).sin() * 0.012 * s;
                    // Forward over the shoulder once past the jaw.
                    let over = ((s - 0.4) / 0.6).clamp(0.0, 1.0);
                    Vec3::new(
                        side * (0.097 + 0.035 * s.powf(0.7) - 0.02 * over + wave),
                        y,
                        depth - 0.03 * (1.0 - s) * (1.0 - s) + 0.1 * over * over,
                    )
                })
                .collect();
            strands.push(Strand {
                attached: path.iter().map(|p| p.y > HAIR_SCALP_END_Y).collect(),
                support_side: Some(super::deformation::SupportSide::Front),
                path,
                root: 0.03,
                tip: 0.013,
            });
        }
    }
    // A soft lock swept from the parting across the brow to the temple.
    let part = if seed.is_multiple_of(2) { 1.0f32 } else { -1.0 };
    strands.push(Strand {
        attached: vec![true; 5],
        support_side: None,
        path: vec![
            Vec3::new(part * 0.03, 0.3, 0.035),
            Vec3::new(-part * 0.02, 0.285, 0.085),
            Vec3::new(-part * 0.075, 0.25, 0.09),
            Vec3::new(-part * 0.103, 0.19, 0.06),
            Vec3::new(-part * 0.11, 0.12, 0.035),
        ],
        root: 0.026,
        tip: 0.014,
    });
    strands
}

/// Combed hair: fine streaks running down each strand, a soft sheen where
/// it turns to the light and darker undersides.
fn hair_sheen(at: Vec3, normal: Vec3, seed: u64) -> f32 {
    let a = at.x.atan2(-at.z);
    let streak = 1.0
        + 0.07 * (a * 41.0 + at.y * 5.0 + seed as f32).sin()
        + 0.035 * (a * 97.0 - at.y * 3.0).sin();
    let sheen = 1.0 + 0.16 * normal.y.max(0.0).powi(2) * (1.0 - normal.z.abs());
    let under = if normal.y < -0.4 { 0.86 } else { 1.0 };
    0.94 * streak * sheen * under
}

/// A plait hanging from `top`, ending in a little tuft.
fn braid(p: &mut Parts, top: Vec3, color: Vec3) {
    for k in 0..8 {
        let t = k as f32 / 7.0;
        let at = top
            + Vec3::new(
                if k % 2 == 0 { 0.008 } else { -0.008 },
                -t * 0.26,
                -t * 0.03,
            );
        p.matte.blob(
            at,
            Vec3::splat(0.032 - 0.012 * t),
            1,
            0.0,
            color * (if k % 2 == 0 { 1.0 } else { 0.92 }),
        );
    }
    lock(
        &mut p.matte,
        &[
            top + Vec3::new(0.0, -0.27, -0.03),
            top + Vec3::new(0.0, -0.32, -0.035),
        ],
        0.02,
        0.014,
        color,
    );
}

/// A face of moderate, modelled features: skull and jaw, cheekbones, a nose
/// with a bridge and wings, lidded eyes, brows, lips and ears. Women's faces
/// are softer in the jaw, with finer brows, lashes and fuller lips.
/// A sculpted human head and the mouth it carries.
pub(super) struct FaceShape {
    pub form: crate::sdf::Sculpt,
    pub mouth_width: f32,
    /// How far the mouth corners lift, and how far one side does.
    pub smile: f32,
    pub smirk: f32,
}

pub(super) fn face_shape(l: &Look, nose: bool) -> FaceShape {
    let fem = l.v.woman;
    // The head is sculpted as one surface: cranium and jaw blended,
    // cheekbones and a brow over carved eye sockets, a nose that grows out of
    // the face with its wings, lips with a fitted mouth line, a chin, ears.
    use crate::sdf::{Sculpt, Shape::*};
    // Her nose is finer and narrower, as drawn on the sheets.
    let n = if fem { 0.76 } else { 1.0 };
    let gaunt = matches!(
        l.style,
        Style::Zombie | Style::Vampire | Style::Phantom | Style::Wraith
    );
    let brute = matches!(l.style, Style::Hobgoblin | Style::Orc | Style::Troll);
    // A living woman's face is soft and rounded, as on the concept sheets:
    // full lower cheeks, a small chin and full lips. The undead keep their
    // hollows and the brutes their heavy planes.
    let soft = fem && !gaunt && !brute;
    let mut jaw = if fem {
        Vec3::new(0.069, 0.066, 0.075)
    } else {
        [
            Vec3::new(0.078, 0.067, 0.078),
            Vec3::new(0.083, 0.069, 0.08),
            Vec3::new(0.089, 0.07, 0.082),
        ][l.v.build as usize]
    };
    if fem {
        jaw.x *= [0.96, 1.0, 1.06][l.v.build as usize];
    }
    if gaunt {
        jaw *= Vec3::new(0.87, 1.0, 0.95);
    } else if brute {
        jaw *= Vec3::new(1.12, 0.96, 1.04);
    }
    let mut head = Sculpt::default();
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.182, -0.012),
            Vec3::new(0.096, 0.102, 0.102),
        ),
        0.0,
    )
    .add(Ellipsoid(Vec3::new(0.0, 0.112, 0.025), jaw), 0.022)
    .add(
        Ellipsoid(
            // Taller support joins the lower lip to the jaw without changing
            // the chin's forward plane, width, or lower extent.
            Vec3::new(0.0, 0.065, if soft { 0.07 } else { 0.068 }),
            Vec3::new(0.037, 0.031, 0.027)
                * if soft {
                    Vec3::new(0.8, 0.88, 0.95)
                } else if fem {
                    Vec3::splat(0.9)
                } else {
                    Vec3::ONE
                },
        ),
        0.024,
    )
    .add(
        Capsule(
            Vec3::new(0.0, 0.02, -0.01),
            Vec3::new(0.0, 0.1, -0.01),
            0.044 * n,
            0.046,
        ),
        0.03,
    );
    for side in [-1.0f32, 1.0] {
        // A broad malar pad flows into the midcheek. A thin raised ridge
        // over an empty lower cheek ages healthy faces at portrait distance.
        head.add(
            Ellipsoid(
                Vec3::new(side * 0.053, if gaunt { 0.135 } else { 0.13 }, 0.055),
                if gaunt {
                    Vec3::new(0.03, 0.022, 0.025)
                } else {
                    Vec3::new(0.032, 0.027, 0.027)
                },
            ),
            if gaunt { 0.024 } else { 0.03 },
        )
        .add(
            Ellipsoid(
                Vec3::new(side * 0.098, 0.15, -0.005),
                Vec3::new(0.014, 0.03, 0.022),
            ),
            0.01,
        );
    }
    if soft {
        // Full, soft cheeks round the face under the cheekbones, instead of
        // a flat front meeting flat sides; held above the jaw line so they
        // never sag into jowls.
        for side in [-1.0f32, 1.0] {
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.046, 0.108, 0.061),
                    Vec3::new(0.031, 0.027, 0.03),
                ),
                0.03,
            );
        }
    }
    // The brow, heavier on men.
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.186, 0.078),
            Vec3::new(0.072, if brute { 0.022 } else { 0.013 }, 0.022)
                * if fem && !brute { 0.9 } else { 1.0 },
        ),
        0.012,
    );
    if nose {
        head.add(
            Capsule(
                Vec3::new(0.0, 0.18, 0.094),
                Vec3::new(0.0, 0.135, 0.118),
                0.009 * n,
                0.014 * n,
            ),
            0.012,
        )
        .add(
            Ellipsoid(
                Vec3::new(0.0, 0.13, 0.117),
                Vec3::new(0.018, 0.015, 0.016) * n,
            ),
            0.008,
        );
        for side in [-1.0f32, 1.0] {
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.014 * n, 0.126, 0.106),
                    Vec3::splat(0.011 * n),
                ),
                0.006,
            );
        }
    }
    // A shallow perioral plane supports the lips and flows into the chin.
    // Keep its sides inside the cheek planes, including the undead hollows.
    // On a soft face it is wider and shallower, melting into the cheeks so
    // no edge of it shows beside the mouth corners.
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.088, 0.071),
            if soft {
                Vec3::new(0.05, 0.028, 0.023)
            } else {
                Vec3::new(if gaunt { 0.035 } else { 0.04 }, 0.028, 0.025)
            },
        ),
        if soft { 0.028 } else { 0.014 },
    );
    // Lip heights exceed the surface-net spacing; narrow carved slits below
    // that spacing produce alternating normals and dark, broken mouth corners.
    let mouth_width = if brute {
        0.032
    } else if gaunt {
        0.027
    } else {
        0.03
    };
    let full = if soft {
        1.5
    } else if fem {
        1.25
    } else {
        1.0
    };
    let pout = if soft { 0.0055 } else { 0.0 };
    head.add(
        Ellipsoid(
            Vec3::new(0.0, 0.098, 0.093 + pout * 0.5),
            Vec3::new(mouth_width, 0.0065 * full, 0.007 + pout),
        ),
        0.01,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.086, 0.091 + pout),
            Vec3::new(mouth_width * 0.92, 0.007 * full, 0.007 + pout),
        ),
        0.01,
    );
    if soft {
        // The cupid's bow: the upper lip's two peaks either side of the
        // philtrum.
        for side in [-1.0f32, 1.0] {
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.008, 0.1025, 0.095),
                    Vec3::new(0.009, 0.004, 0.005),
                ),
                0.004,
            );
        }
    }
    // Each kind wears its own expression: adventurers half-smile, soldiers
    // and hobgoblins scowl, vampires smirk to one side, the dead hang slack.
    let (smile, _scowl, smirk) = match l.style {
        Style::Hero | Style::Scout | Style::Caster | Style::Common | Style::Centaur => {
            (0.006, -0.004, 0.0)
        }
        // Fey grin; the medusa smirks like the vampire.
        Style::Leprechaun | Style::Nymph => (0.009, -0.004, 0.0),
        Style::Hobgoblin | Style::Sentinel | Style::Orc | Style::Troll => (-0.004, 0.009, 0.0),
        Style::Vampire | Style::Medusa => (0.0, 0.005, 0.006),
        Style::Phantom | Style::Wraith => (-0.003, 0.004, 0.0),
        Style::Zombie => (-0.006, -0.006, 0.0),
        _ => (0.0, 0.0, 0.0),
    };
    // The expression follows the continuous lip surface. Its narrow seam is
    // explicit fitted geometry below, independent of the face mesh grid.
    if smile > 0.0 {
        // Cheeks lift with the smile.
        for side in [-1.0f32, 1.0] {
            // On a soft face the lift melts into the full cheek around it.
            head.add(
                Ellipsoid(
                    Vec3::new(side * 0.036, 0.104, 0.082),
                    if soft {
                        Vec3::new(0.009, 0.007, 0.007)
                    } else {
                        Vec3::new(0.012, 0.009, 0.009)
                    },
                ),
                if soft { 0.02 } else { 0.014 },
            );
        }
    }
    for side in [-1.0f32, 1.0] {
        // Broad cheek planes and a defined chin keep the larger storybook
        // head adult. Undead have hollow lower cheeks, not a healthy round
        // face recoloured green; their skull still remains one closed form.
        head.carve(
            Beyond(
                Vec3::new(side * 0.8, 0.0, 0.6),
                if brute {
                    0.113
                } else if soft {
                    0.116
                } else {
                    0.106
                },
            ),
            if soft { 0.028 } else { 0.01 },
        );
        if gaunt {
            // A shallow, tall hollow below the cheekbone reads as a lean
            // facial plane rather than a second round eye socket.
            let hollow = if l.style == Style::Zombie {
                0.026
            } else {
                0.021
            };
            head.carve(
                Ellipsoid(
                    Vec3::new(side * 0.085, 0.105, 0.074),
                    Vec3::new(hollow, 0.037, 0.019),
                ),
                0.015,
            );
        }
        // Eye sockets under the brow.
        head.carve(
            Ellipsoid(
                Vec3::new(side * 0.038, 0.16, 0.1),
                Vec3::new(0.023, 0.017, 0.016),
            ),
            0.01,
        );
    }
    FaceShape {
        form: head,
        mouth_width,
        smile,
        smirk,
    }
}

fn human_face(p: &mut Parts, l: &Look, nose: bool) {
    let fem = l.v.woman;
    let face = l.skin;
    let gaunt = matches!(
        l.style,
        Style::Zombie | Style::Vampire | Style::Phantom | Style::Wraith
    );
    let FaceShape {
        form: head,
        mouth_width,
        smile,
        smirk,
    } = face_shape(l, nose);
    let mouth_height = |x: f32| {
        let span = (x.abs() / mouth_width).min(1.0);
        0.091 + (0.001 + smile) * span.powi(2) + smirk * (x / mouth_width).clamp(0.0, 1.0)
    };
    // Women's lips are full and rose-coloured, their cheeks warmly flushed;
    // the undead keep a cold, dark mouth either way.
    let lips = if gaunt {
        (face * 0.6).lerp(Vec3::new(0.42, 0.18, 0.3), if fem { 0.3 } else { 0.12 })
    } else if fem {
        (face * 0.78).lerp(Vec3::new(0.78, 0.3, 0.36), 0.5)
    } else {
        (face * 0.8).lerp(Vec3::new(0.72, 0.36, 0.36), 0.15)
    };
    let flush = if gaunt {
        face * 0.96
    } else {
        face.lerp(Vec3::new(0.88, 0.45, 0.44), if fem { 0.2 } else { 0.05 })
    };
    head.mesh(
        &mut p.matte,
        Vec3::new(-0.125, 0.0, -0.13),
        Vec3::new(0.125, 0.3, 0.14),
        0.0055,
        &|at, _| {
            let mouth_y = mouth_height(at.x);
            let front = ((at.z - 0.075) / 0.015).clamp(0.0, 1.0);
            let lip =
                (1.2 - (at.x / (mouth_width + 0.002)).powi(2) - ((at.y - mouth_y) / 0.016).powi(2))
                    .clamp(0.0, 1.0)
                    * front;
            let cheek = (1.0 - Vec2::new(at.x.abs() - 0.055, at.y - 0.122).length() / 0.03)
                .clamp(0.0, 1.0)
                * f32::from(u8::from(at.z > 0.05));
            let mottle = 1.0
                + 0.008
                    * ((at.x * 173.0).sin() * (at.y * 151.0).cos()
                        + (at.z * 197.0 + at.x * 61.0).sin() * 0.5);
            let warm = ((1.0 - Vec2::new(at.x, at.y - 0.13).length() / 0.02).clamp(0.0, 1.0)
                * f32::from(u8::from(at.z > 0.1)))
            .max(
                (1.0 - (at.x.abs() - 0.098).abs() / 0.015).clamp(0.0, 1.0)
                    * f32::from(u8::from((at.y - 0.15).abs() < 0.03))
                    * 0.6,
            );
            let base = face.lerp(flush, cheek.max(warm * 0.6)).lerp(lips, lip);
            base * mottle
        },
    );
    let slack = l.style == Style::Zombie;
    let mouth_path: [Vec2; 25] = std::array::from_fn(|i| {
        let u = i as f32 / 24.0 * 2.0 - 1.0;
        if slack {
            Vec2::new(0.004 + u * 0.018, 0.089)
        } else {
            let x = u * mouth_width;
            Vec2::new(x, mouth_height(x))
        }
    });
    surface_stroke(
        &mut p.matte,
        &mouth_path,
        if slack {
            0.009
        } else if fem {
            0.00155
        } else {
            0.0014
        },
        &|at| torso_surface_frame(&head, at, true),
        if slack { DARK * 2.0 } else { lips * 0.6 },
    );
    if l.style == Style::Vampire {
        for side in [-1.0f32, 1.0] {
            let x = side * 0.018;
            let (root, normal) = torso_surface_frame(&head, Vec2::new(x, mouth_height(x)), true);
            p.matte.prism(
                root - normal * 0.002,
                Vec3::new(x, 0.06, 0.1),
                0.006,
                0.0,
                3,
                BONE,
                0.0,
            );
        }
    }
    let iris = [
        Vec3::new(0.36, 0.22, 0.11),
        Vec3::new(0.2, 0.12, 0.07),
        Vec3::new(0.45, 0.4, 0.18),
        Vec3::new(0.25, 0.45, 0.25),
        Vec3::new(0.25, 0.4, 0.62),
    ][((l.v.pattern + l.v.skin) % 5) as usize];
    // Her eyes are a little larger, as on the concept sheets. Eyes that glow
    // keep their lids and lashes, the light held in the iris.
    soft_eyes(
        p,
        Vec3::new(0.0, 0.16, 0.088),
        0.038,
        if fem { 0.0158 } else { 0.0145 },
        iris,
        l,
        &head,
    );
    face_brows(p, l, &head);
}

fn face_brows(p: &mut Parts, l: &Look, head: &crate::sdf::Sculpt) {
    let brow = hair_color(l) * 0.85;
    for side in [-1.0f32, 1.0] {
        let profile = brow_profile(l, side);
        if l.v.woman {
            fitted_brow(&mut p.matte, head, &profile, brow);
            continue;
        }
        let points: Vec<Vec3> = profile.iter().map(|(at, _)| *at).collect();
        let radii: Vec<(f32, f32)> = profile.iter().map(|(_, r)| *r).collect();
        tube(&mut p.matte, &points, &radii, brow);
    }
}

/// A shallow tapered brow band. Fit every edge to the forehead, rather than
/// floating a wider tube above it or burying its sides in a curved temple.
fn fitted_brow(
    b: &mut Builder,
    head: &crate::sdf::Sculpt,
    profile: &[(Vec3, (f32, f32))],
    color: Vec3,
) {
    let mut path = Vec::new();
    for pair in profile.windows(2) {
        for row in 0..4 {
            let t = row as f32 / 4.0;
            path.push((
                pair[0].0.truncate().lerp(pair[1].0.truncate(), t),
                Vec2::from(pair[0].1).lerp(Vec2::from(pair[1].1), t),
            ));
        }
    }
    let Some(&(end, radii)) = profile.last() else {
        return;
    };
    path.push((end.truncate(), Vec2::from(radii)));
    let mut grid = Vec::with_capacity(path.len() * 5);
    for (i, &(at, radii)) in path.iter().enumerate() {
        let tangent = (path[(i + 1).min(path.len() - 1)].0 - path[i.saturating_sub(1)].0)
            .normalize_or(Vec2::X);
        let across = Vec2::new(-tangent.y, tangent.x);
        for column in 0..=4 {
            let v = column as f32 * 0.5 - 1.0;
            let (surface, normal) = torso_surface_frame(head, at + across * radii.x * v, true);
            grid.push([
                surface + normal * 0.0014,
                surface + normal * (0.002 + radii.y * (1.0 - v * v)),
            ]);
        }
    }
    for row in 0..path.len() - 1 {
        for column in 0..4 {
            let q = [
                grid[row * 5 + column],
                grid[(row + 1) * 5 + column],
                grid[(row + 1) * 5 + column + 1],
                grid[row * 5 + column + 1],
            ];
            let center = q.iter().flatten().copied().sum::<Vec3>() * 0.125;
            let outward = q
                .iter()
                .map(|pair| pair[1] - pair[0])
                .sum::<Vec3>()
                .normalize_or(Vec3::Z);
            b.quad_out(q.map(|pair| pair[1]), center - outward * 0.01, color);
            b.quad_out(q.map(|pair| pair[0]), center + outward * 0.01, color);
            if column == 0 {
                b.quad_out([q[0][0], q[1][0], q[1][1], q[0][1]], center, color);
            }
            if column == 3 {
                b.quad_out([q[3][0], q[2][0], q[2][1], q[3][1]], center, color);
            }
            if row == 0 {
                b.quad_out([q[0][0], q[3][0], q[3][1], q[0][1]], center, color);
            }
            if row + 2 == path.len() {
                b.quad_out([q[1][0], q[2][0], q[2][1], q[1][1]], center, color);
            }
        }
    }
}

/// One eyebrow on `side`: points along it from the inner end to the outer
/// and its half-width and half-thickness at each. Adult female brows have a
/// substantial inner third, a controlled arch and a fine outer taper. Species
/// expressions lower the inner end; actual forehead fitting supplies depth.
pub(super) fn brow_profile(l: &Look, side: f32) -> Vec<(Vec3, (f32, f32))> {
    let fem = l.v.woman;
    let scowl = match l.style {
        Style::Hobgoblin | Style::Sentinel | Style::Orc | Style::Troll => 0.009,
        Style::Vampire | Style::Medusa | Style::Wraith => 0.005,
        Style::Zombie => -0.004,
        _ => -0.002,
    } * if fem { 0.6 } else { 1.0 };
    if fem {
        let (w, h) = (0.0048, 0.0025);
        vec![
            (
                Vec3::new(side * 0.017, 0.192 - scowl, 0.101),
                (w * 0.65, h * 0.65),
            ),
            (Vec3::new(side * 0.033, 0.196 - scowl * 0.3, 0.1), (w, h)),
            (Vec3::new(side * 0.051, 0.197, 0.097), (w * 0.85, h * 0.85)),
            (
                Vec3::new(side * 0.068, 0.191 + scowl * 0.2, 0.089),
                (w * 0.25, h * 0.25),
            ),
        ]
    } else {
        let (w, h) = (0.009, 0.006);
        vec![
            (Vec3::new(side * 0.018, 0.187 - scowl, 0.101), (w, h)),
            (Vec3::new(side * 0.042, 0.197 - scowl * 0.3, 0.099), (w, h)),
            (
                Vec3::new(side * 0.066, 0.189 + scowl * 0.3, 0.09),
                (w * 0.6, h * 0.6),
            ),
        ]
    }
}

/// A small, closed surface accent with a tapered cross-section. Project each
/// edge independently so a mouth stays seated on the actual lip curvature.
/// Explicit geometry resolves the seam without a sub-grid carved cavity.
fn surface_stroke(
    b: &mut Builder,
    path: &[Vec2],
    half_width: f32,
    surface: &dyn Fn(Vec2) -> (Vec3, Vec3),
    color: Vec3,
) {
    if path.len() < 2 {
        return;
    }
    // Wide accents, such as a slack open mouth, need samples across the lip
    // bulge as well as along the expression curve. Narrow seams stay one cell.
    let across_steps = ((2.0 * half_width / 0.004).ceil() as usize).clamp(1, 5);
    let stride = across_steps + 1;
    let mut grid = Vec::with_capacity(path.len() * stride);
    for (i, &at) in path.iter().enumerate() {
        let tangent =
            (path[(i + 1).min(path.len() - 1)] - path[i.saturating_sub(1)]).normalize_or(Vec2::X);
        let across = Vec2::new(-tangent.y, tangent.x);
        let u = i as f32 / (path.len() - 1) as f32 * 2.0 - 1.0;
        let width = half_width * (1.0 - u * u).max(0.0).sqrt().max(0.08);
        for j in 0..=across_steps {
            let v = j as f32 / across_steps as f32 * 2.0 - 1.0;
            let (point, normal) = surface(at + across * (width * v));
            // Face surface nets average crossings inside a convex lip. The
            // normal clearance also covers their small local chord error.
            grid.push([point + normal * 0.0018, point + normal * (0.0018 + 0.0007)]);
        }
    }
    for i in 0..path.len() - 1 {
        for j in 0..across_steps {
            let q = [
                grid[i * stride + j],
                grid[(i + 1) * stride + j],
                grid[(i + 1) * stride + j + 1],
                grid[i * stride + j + 1],
            ];
            let center = q.iter().flatten().copied().sum::<Vec3>() * 0.125;
            let outward = q
                .iter()
                .map(|pair| pair[1] - pair[0])
                .sum::<Vec3>()
                .normalize_or(Vec3::Z);
            // Use the actual surface normal for front/back winding even if
            // the four fitted corners are not perfectly coplanar.
            b.quad_out(q.map(|pair| pair[1]), center - outward * 0.01, color);
            b.quad_out(q.map(|pair| pair[0]), center + outward * 0.01, color);
            if j == 0 {
                b.quad_out([q[0][0], q[1][0], q[1][1], q[0][1]], center, color);
            }
            if j + 1 == across_steps {
                b.quad_out([q[3][0], q[2][0], q[2][1], q[3][1]], center, color);
            }
            if i == 0 {
                b.quad_out([q[0][0], q[3][0], q[3][1], q[0][1]], center, color);
            }
            if i + 2 == path.len() {
                b.quad_out([q[1][0], q[2][0], q[2][1], q[1][1]], center, color);
            }
        }
    }
}

/// The authored globe follows the local socket plane. All visible pigments
/// share its surface and the same piecewise-linear almond boundary.
struct EyeSurface<'a> {
    head: &'a crate::sdf::Sculpt,
    at: Vec3,
    slope: Vec2,
    size: f32,
    side: f32,
}

impl<'a> EyeSurface<'a> {
    const COLUMNS: usize = 32;

    fn new(head: &'a crate::sdf::Sculpt, at: Vec3, size: f32, side: f32) -> Self {
        let center = torso_surface(head, at.x, at.y, true);
        let e = size * 0.7;
        let depth = |x, y| torso_surface(head, x, y, true).z;
        let slope = Vec2::new(
            (depth(at.x + e, at.y) - depth(at.x - e, at.y)) / (2.0 * e),
            (depth(at.x, at.y + e) - depth(at.x, at.y - e)) / (2.0 * e),
        )
        .clamp(Vec2::splat(-0.7), Vec2::splat(0.7));
        Self {
            head,
            at: center + Vec3::Z * 0.0012,
            slope,
            size,
            side,
        }
    }

    fn half_width(&self) -> f32 {
        self.size * 1.35
    }

    fn opening(&self, x: f32) -> (f32, f32) {
        // Interpolate the actual white-mesh boundary, rather than clipping a
        // disc against a slightly different analytic curve between columns.
        let column = ((x - self.at.x) / self.half_width() + 1.0) * 0.5 * Self::COLUMNS as f32;
        let left = column.floor().clamp(0.0, (Self::COLUMNS - 1) as f32);
        let t = (column - left).clamp(0.0, 1.0);
        let bounds = |column: f32| {
            let u = column / Self::COLUMNS as f32 * 2.0 - 1.0;
            let tilt = self.side * u * 0.1;
            Vec2::new(-0.39 * (1.0 - u * u) + tilt, 0.58 * (1.0 - u * u) + tilt) * self.size
                + Vec2::splat(self.at.y)
        };
        bounds(left).lerp(bounds(left + 1.0), t).into()
    }

    fn point(&self, xy: Vec2, offset: f32) -> Vec3 {
        let delta = xy - self.at.truncate();
        let u = delta.x / self.half_width();
        let v = (delta.y - self.side * u * self.size * 0.1) / (self.size * 0.9);
        let globe = self.at.z
            + self.slope.dot(delta)
            + self.size * 0.5 * (1.0 - u * u - v * v).max(0.0).sqrt();
        let skin = torso_surface(self.head, xy.x, xy.y, true).z;
        Vec3::new(xy.x, xy.y, globe.max(skin + 0.0008) + offset)
    }
}

/// Close a sampled surface with a shallow back and side walls. The grid runs
/// along rows, with `columns + 1` points across each row.
fn closed_eye_grid(b: &mut Builder, grid: &[[Vec3; 2]], columns: usize, color: Vec3) {
    let stride = columns + 1;
    let rows = grid.len() / stride;
    for row in 0..rows - 1 {
        for column in 0..columns {
            let q = [
                grid[row * stride + column],
                grid[(row + 1) * stride + column],
                grid[(row + 1) * stride + column + 1],
                grid[row * stride + column + 1],
            ];
            let center = q.iter().flatten().copied().sum::<Vec3>() * 0.125;
            b.quad_out(q.map(|p| p[1]), center - Vec3::Z * 0.01, color);
            b.quad_out(q.map(|p| p[0]), center + Vec3::Z * 0.01, color);
            if column == 0 {
                b.quad_out([q[0][0], q[1][0], q[1][1], q[0][1]], center, color);
            }
            if column + 1 == columns {
                b.quad_out([q[3][0], q[2][0], q[2][1], q[3][1]], center, color);
            }
            if row == 0 {
                b.quad_out([q[0][0], q[3][0], q[3][1], q[0][1]], center, color);
            }
            if row + 2 == rows {
                b.quad_out([q[1][0], q[2][0], q[2][1], q[1][1]], center, color);
            }
        }
    }
}

/// Paint a white or elliptical disc on the globe, clipping every layer to the
/// emitted opening. Shared column breaks keep the tessellated edges aligned.
fn eye_patch(
    b: &mut Builder,
    eye: &EyeSurface,
    disc: Option<(Vec2, f32)>,
    offset: f32,
    color: Vec3,
) {
    let radius = disc.map_or(eye.half_width(), |(r, _)| r.x);
    let mut xs = vec![-radius, radius];
    for column in 0..=EyeSurface::COLUMNS {
        let x = (column as f32 / EyeSurface::COLUMNS as f32 * 2.0 - 1.0) * eye.half_width();
        if x > -radius && x < radius {
            xs.push(x);
        }
    }
    xs.sort_by(f32::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < 1e-7);
    let mut grid = Vec::with_capacity(xs.len() * 9);
    for x in xs {
        let (mut low, mut high) = eye.opening(eye.at.x + x);
        if let Some((r, y)) = disc {
            let extent = r.y * (1.0 - (x / r.x).powi(2)).max(0.0).sqrt();
            low = low.max(eye.at.y + y - extent);
            high = high.min(eye.at.y + y + extent);
        }
        for row in 0..=8 {
            let y = low + (high - low) * row as f32 / 8.0;
            let point = eye.point(Vec2::new(eye.at.x + x, y), offset);
            grid.push([point - Vec3::Z * 0.00025, point]);
        }
    }
    closed_eye_grid(b, &grid, 8, color);
}

fn eye_lid(b: &mut Builder, eye: &EyeSurface, upper: bool, color: Vec3) {
    let mut grid = Vec::with_capacity((EyeSurface::COLUMNS + 1) * 5);
    for row in 0..=EyeSurface::COLUMNS {
        let u = row as f32 / EyeSurface::COLUMNS as f32 * 2.0 - 1.0;
        let x = eye.at.x + eye.half_width() * u;
        let (low, high) = eye.opening(x);
        let inner = eye.point(Vec2::new(x, if upper { high } else { low }), 0.0002);
        let outer_xy = eye.at.truncate()
            + Vec2::new(
                eye.size * 1.6 * u,
                eye.size
                    * (eye.side * u * 0.1
                        + if upper { 0.98 } else { -0.9 } * (1.0 - u * u).max(0.0).sqrt()),
            );
        let outer = torso_surface(eye.head, outer_xy.x, outer_xy.y, true) + Vec3::Z * 0.0012;
        for column in 0..=4 {
            let v = column as f32 / 4.0;
            let point = inner.lerp(outer, v) + Vec3::Z * (0.001 * (v * PI).sin());
            grid.push([point - Vec3::Z * 0.0007, point]);
        }
    }
    closed_eye_grid(b, &grid, 4, color);
}

fn adult_eye(
    p: &mut Parts,
    eye: &EyeSurface,
    iris: Vec3,
    pupil: Vec2,
    face: Vec3,
    glow: Option<Accent>,
) {
    eye_patch(
        &mut p.matte,
        eye,
        None,
        0.0,
        if glow.is_some() {
            face * 0.3
        } else {
            Vec3::new(0.82, 0.81, 0.76)
        },
    );
    eye_patch(
        if glow.is_some() {
            &mut p.glow
        } else {
            &mut p.matte
        },
        eye,
        Some((Vec2::splat(eye.size * 0.7), -eye.size * 0.04)),
        0.0005,
        if glow.is_some() { Vec3::ONE } else { iris },
    );
    eye_patch(
        &mut p.matte,
        eye,
        Some((pupil, -eye.size * 0.04)),
        0.001,
        DARK,
    );
    eye_lid(
        &mut p.matte,
        eye,
        true,
        face.lerp(Vec3::new(0.55, 0.36, 0.42), 0.12),
    );
    eye_lid(&mut p.matte, eye, false, face * 0.98);
    let mut line = Vec::new();
    let mut radii = Vec::new();
    for k in 0..=EyeSurface::COLUMNS {
        let u = k as f32 / EyeSurface::COLUMNS as f32 * 2.0 - 1.0;
        let x = eye.at.x + eye.half_width() * u;
        line.push(eye.point(Vec2::new(x, eye.opening(x).1), 0.0016));
        let radius = eye.size * (0.04 + 0.085 * (1.0 - u * u));
        radii.push((radius, radius));
    }
    tube(&mut p.matte, &line, &radii, DARK);
    for u in [0.65, 0.88, 1.0] {
        let x = eye.at.x + eye.side * eye.half_width() * u;
        let root = eye.point(Vec2::new(x, eye.opening(x).1), 0.0016);
        p.matte.prism(
            root,
            root + Vec3::new(eye.side * eye.size * 0.22, eye.size * 0.18, 0.0005),
            eye.size * 0.035,
            0.0,
            3,
            DARK,
            0.0,
        );
    }
    let glint_xy = eye.at.truncate() + Vec2::new(-0.18, 0.1) * eye.size;
    p.matte.blob(
        eye.point(glint_xy, 0.0015),
        Vec3::splat(eye.size * 0.075),
        0,
        0.0,
        Vec3::splat(1.3),
    );
}

/// Adult female eyes use one fitted opening and lid assembly. Male eyes retain
/// their established construction; glow ownership stays restricted to the iris.
fn soft_eyes(
    p: &mut Parts,
    center: Vec3,
    spread: f32,
    size: f32,
    iris: Vec3,
    l: &Look,
    head: &crate::sdf::Sculpt,
) {
    let (face, lashes, glow) = (l.skin, l.v.woman, l.eyes);
    if glow.is_some() {
        p.accent = glow;
    }
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        if lashes {
            let pupil = if l.style == Style::Medusa {
                Vec2::new(0.26, 0.43)
            } else {
                Vec2::splat(0.3)
            } * size;
            adult_eye(
                p,
                &EyeSurface::new(head, at, size, side),
                iris,
                pupil,
                face,
                glow,
            );
            continue;
        }
        p.matte.blob(
            at,
            Vec3::new(size * 1.35, size * 0.68, size * 0.65),
            0,
            0.0,
            if glow.is_some() {
                face * 0.3
            } else {
                Vec3::new(0.82, 0.81, 0.76)
            },
        );
        let iris_builder = if glow.is_some() {
            &mut p.glow
        } else {
            &mut p.matte
        };
        iris_builder.blob(
            at + Vec3::new(0.0, -size * 0.05, size * 0.5),
            Vec3::new(size * 0.7, size * 0.65, size * 0.3),
            0,
            0.0,
            if glow.is_some() { Vec3::ONE } else { iris },
        );
        p.matte.blob(
            at + Vec3::new(0.0, -size * 0.05, size * 0.68),
            Vec3::new(size * 0.32, size * 0.43, size * 0.18),
            0,
            0.0,
            DARK,
        );
        p.matte.blob(
            at + Vec3::new(size * 0.3, size * 0.3, size * 0.85),
            Vec3::splat(size * 0.12),
            0,
            0.0,
            Vec3::splat(1.6),
        );
        p.matte.blob(
            at + Vec3::new(0.0, size * 0.7, size * 0.1),
            Vec3::new(size * 1.45, size * 0.36, size * 0.7),
            1,
            0.0,
            face * 0.98,
        );
        tube(
            &mut p.matte,
            &[
                at + Vec3::new(-size * 1.35, size * 0.42, size * 0.62),
                at + Vec3::new(0.0, size * 0.66, size * 0.86),
                at + Vec3::new(size * 1.35, size * 0.42, size * 0.62),
            ],
            &[(0.0022, 0.0022), (0.0028, 0.0028), (0.0022, 0.0022)],
            DARK * 2.0,
        );
    }
}

/// A short beard along the jaw to the chin, and a moustache.
fn beard(p: &mut Parts, color: Vec3) {
    let c = color * 0.95;
    for side in [-1.0f32, 1.0] {
        lock(
            &mut p.matte,
            &[
                Vec3::new(side * 0.088, 0.13, 0.0),
                Vec3::new(side * 0.075, 0.08, 0.055),
                Vec3::new(side * 0.03, 0.055, 0.095),
                Vec3::new(0.0, 0.05, 0.1),
            ],
            0.026,
            0.02,
            c,
        );
        lock(
            &mut p.matte,
            &[
                Vec3::new(side * 0.004, 0.1, 0.112),
                Vec3::new(side * 0.025, 0.097, 0.108),
                Vec3::new(side * 0.034, 0.086, 0.1),
            ],
            0.01,
            0.008,
            c,
        );
    }
    p.matte.blob(
        Vec3::new(0.0, 0.058, 0.088),
        Vec3::new(0.036, 0.022, 0.026),
        2,
        0.0,
        c,
    );
}

/// A crown of sculpted clumps: broad locks radiating from the top of the
/// head down to the hairline, so the edge of the hair is a run of grouped
/// locks rather than the rim of a cap. `front`, `side` and `back` are the
/// heights the locks end at; `sweep` pushes them toward a parting.
#[allow(clippy::too_many_arguments)]
fn crown(
    p: &mut Parts,
    tone: &dyn Fn(u32) -> Vec3,
    front: f32,
    side: f32,
    back: f32,
    sweep: f32,
    width: f32,
    count: u32,
) {
    let skull = Vec3::new(0.0, 0.17, -0.005);
    for k in 0..count {
        let a = k as f32 / count as f32 * TAU;
        let facing = a.cos(); // 1 at the front, -1 at the back.
        let end_y = if facing >= 0.0 {
            side + (front - side) * facing
        } else {
            side + (back - side) * -facing
        };
        let dir = Vec3::new(a.sin(), 0.0, a.cos());
        let root = Vec3::new(0.0, 0.292, -0.015) + dir * 0.025;
        // Points on the skull surface, a little proud of it.
        let on_skull = |y: f32, out: f32| {
            let h = ((y - skull.y) / 0.112).clamp(-1.0, 1.0);
            let r = (1.0 - h * h).max(0.0).sqrt();
            skull + Vec3::new(dir.x * 0.1 * r, y - skull.y, dir.z * 0.108 * r) * out
        };
        let mid_y = (0.29 + end_y) * 0.5 + 0.02;
        let mid = on_skull(mid_y, 1.16) + Vec3::X * sweep * 0.5;
        let end = on_skull(end_y, 1.1) + Vec3::X * sweep;
        lock(&mut p.matte, &[root, mid, end], width, 0.03, tone(k));
    }
}

/// Hair as sculpted clumps over a fitted cap, with volume where the style
/// carries it. Men: swept, cropped, tousled, slicked, tied back. Women: long,
/// ponytail, bun, braid, bob, all falling over the ears and temples.
/// Which cut a person's hair takes: for men swept, cropped, tousled,
/// slicked or tied back; for women long, ponytail, bun, braid or bob.
fn hair_style(l: &Look) -> u8 {
    let v = l.v;
    match l.style {
        Style::Vampire if !v.woman => 3,
        Style::Vampire => [0, 2, 0, 2, 4][v.hair as usize],
        Style::Zombie if !v.woman => 2,
        Style::Zombie => 0,
        // As drawn on their sheets: tousled or swept fey heads, a long
        // braid on her; the dead wear theirs long and loose.
        Style::Leprechaun if !v.woman => [2, 0, 2, 2, 0][v.hair as usize],
        Style::Leprechaun => 3,
        Style::Nymph if !v.woman => 2,
        Style::Nymph => 0,
        Style::Phantom if !v.woman => [4, 3, 4, 4, 3][v.hair as usize],
        Style::Phantom => 0,
        _ => v.hair,
    }
}

/// Whether a person's head is covered by a hood or helm rather than
/// showing hair.
fn hooded(l: &Look) -> bool {
    matches!(
        Outfit::worn(l).head,
        Headwear::Hood { .. } | Headwear::Helm | Headwear::WingedHelm { .. }
    )
}

/// Whether a person shows hair at all: not under a hood, and not a kind
/// whose head is bald, crested or crowned with serpents.
fn shows_hair(l: &Look) -> bool {
    !hooded(l) && grows_scalp_hair(l)
}

/// Species hair identity stays separate from whether headwear covers it.
fn grows_scalp_hair(l: &Look) -> bool {
    !matches!(
        l.style,
        Style::Guardian
            | Style::Yeti
            | Style::Hobgoblin
            | Style::Orc
            | Style::Troll
            | Style::Medusa
    )
}

/// Whether a person wears long loose hair, which hangs from the crown on
/// its own and swings as she moves.
pub(super) fn loose_fall(l: &Look) -> bool {
    l.plan == Plan::Biped && l.v.woman && shows_hair(l) && hair_style(l) == 0
}

/// The point on the crown long hair hangs from, in the head's space.
const CROWN: Vec3 = Vec3::new(0.0, 0.25, -0.05);

/// The long fall of loose hair as it would sit on the head, for tests.
#[cfg(test)]
pub(super) fn long_hair_form_for(p: &mut Parts, l: &Look) {
    long_fall(p, l);
}

/// Each lock has its own topology; the head's fitted cap covers their roots.
fn long_fall(p: &mut Parts, l: &Look) {
    for strand in long_hair_strands(hair_seed(l, 0)) {
        hanging_lock(p, l, &strand);
    }
}

fn hanging_lock(p: &mut Parts, l: &Look, strand: &Strand) {
    let color = hair_color(l);
    let seed = hair_seed(l, 0);
    // Linear subdivision stays inside the authored control envelope. Spline
    // overshoot would require a larger collider than the recipe advertises.
    let mut path = Vec::new();
    let mut radii = Vec::new();
    let last = strand.path.len() - 1;
    for (segment, pair) in strand.path.windows(2).enumerate() {
        for step in 0..4 {
            let u = step as f32 / 4.0;
            let t0 = segment as f32 / last as f32;
            let t1 = (segment + 1) as f32 / last as f32;
            let radius = |t: f32| strand.root + (strand.tip - strand.root) * t * t;
            path.push(pair[0].lerp(pair[1], u));
            radii.push(Vec2::splat(radius(t0) + (radius(t1) - radius(t0)) * u));
        }
    }
    path.push(*strand.path.last().unwrap());
    radii.push(Vec2::splat(strand.tip));
    let smooth = p.matte.smooth;
    let sides_mul = p.matte.sides_mul;
    p.matte.smooth = false;
    p.matte.sides_mul = 1;
    p.matte.tube_framed(
        &path,
        &radii,
        20,
        &|at, normal, _| color * hair_sheen(at, normal, seed),
        None,
    );
    p.matte.smooth = smooth;
    p.matte.sides_mul = sides_mul;
}

fn hair_seed(l: &Look, style: u8) -> u64 {
    u64::from(l.v.hair_tone) * 31 + u64::from(style) + u64::from(l.v.skin) * 7
}

/// A person's hair, without the long fall when that hangs on its own.
fn hair(p: &mut Parts, l: &Look, with_fall: bool) {
    let c = hair_color(l);
    let v = l.v;
    let style = hair_style(l);
    let seed = hair_seed(l, style);
    let tone = move |k: u32| c * (0.9 + 0.18 * rand(seed, k));
    let mass = |p: &mut Parts, at: Vec3, r: Vec3, color: Vec3| p.matte.blob(at, r, 2, 0.0, color);
    // The fitted cap under the clumps fills any gaps between them.
    mass(
        p,
        Vec3::new(0.0, 0.205, -0.02),
        Vec3::new(0.105, 0.094, 0.11),
        c * 0.9,
    );
    if !v.woman {
        match style {
            // Swept up and over: a quiff above a crown of clumps.
            0 => {
                crown(p, &tone, 0.245, 0.19, 0.13, 0.0, 0.062, 11);
                lock(
                    &mut p.matte,
                    &[
                        Vec3::new(-0.06, 0.27, 0.05),
                        Vec3::new(-0.01, 0.315, 0.085),
                        Vec3::new(0.05, 0.3, 0.1),
                        Vec3::new(0.085, 0.255, 0.085),
                    ],
                    0.055,
                    0.04,
                    tone(40),
                );
            }
            // Cropped short and close.
            1 => crown(p, &tone, 0.25, 0.2, 0.15, 0.0, 0.05, 13),
            // Tousled: uneven clumps, tips lifting.
            2 => {
                crown(p, &tone, 0.235, 0.19, 0.12, 0.012, 0.06, 10);
                for k in 0..6 {
                    let a = k as f32 * 2.4;
                    let at = Vec3::new(a.cos() * 0.05, 0.29, a.sin() * 0.055);
                    let out = Vec3::new(a.cos(), 0.5, a.sin()).normalize();
                    lock(
                        &mut p.matte,
                        &[at, at + out * 0.05, at + out * 0.08 - Vec3::Y * 0.015],
                        0.045,
                        0.026,
                        tone(20 + k),
                    );
                }
            }
            // Slicked straight back.
            3 => {
                crown(p, &tone, 0.24, 0.19, 0.12, 0.0, 0.07, 9);
                mass(
                    p,
                    Vec3::new(0.0, 0.25, -0.05),
                    Vec3::new(0.104, 0.06, 0.1),
                    c,
                );
            }
            // Swept back and tied at the nape.
            _ => {
                crown(p, &tone, 0.245, 0.19, 0.14, 0.0, 0.065, 10);
                p.matte.torus(
                    Vec3::new(0.0, 0.165, -0.13),
                    Vec3::new(0.0, 0.3, -1.0).normalize(),
                    0.02,
                    0.007,
                    10,
                    4,
                    c * 0.6,
                );
                // A slim tail that lies along the nape instead of jutting out.
                lock(
                    &mut p.matte,
                    &[
                        Vec3::new(0.0, 0.165, -0.13),
                        Vec3::new(0.0, 0.11, -0.148),
                        Vec3::new(0.0, 0.04, -0.142),
                        Vec3::new(0.0, -0.04, -0.125),
                    ],
                    0.034,
                    0.026,
                    c,
                );
            }
        }
        return;
    }
    // Hers: a side-parted crown falling over the ears to the nape.
    crown(p, &tone, 0.235, 0.12, 0.09, 0.015, 0.068, 12);
    match style {
        // Long and loose over the shoulders, locks framing the face: one
        // sculpted fall of hair whose waving strands merge into a soft mass
        // at the scalp and part into separate locks towards their tips.
        0 => {
            if with_fall {
                long_fall(p, l);
            }
        }
        // A high ponytail.
        1 => {
            p.matte.torus(
                Vec3::new(0.0, 0.215, -0.125),
                Vec3::new(0.0, 0.5, -1.0).normalize(),
                0.024,
                0.009,
                10,
                4,
                c * 0.6,
            );
            lock(
                &mut p.matte,
                &[
                    Vec3::new(0.0, 0.215, -0.135),
                    Vec3::new(0.0, 0.17, -0.21),
                    Vec3::new(0.0, 0.04, -0.21),
                    Vec3::new(0.0, -0.07, -0.16),
                ],
                0.06,
                0.05,
                c,
            );
        }
        // A neat bun.
        2 => {
            mass(
                p,
                Vec3::new(0.0, 0.25, -0.115),
                Vec3::new(0.062, 0.056, 0.056),
                tone(6),
            );
            p.matte.torus(
                Vec3::new(0.0, 0.25, -0.115),
                Vec3::new(0.0, 0.3, -1.0).normalize(),
                0.055,
                0.008,
                12,
                4,
                c * 0.75,
            );
        }
        // A single long braid.
        3 => braid(p, Vec3::new(0.0, 0.13, -0.13), c),
        // A bob to the jaw.
        _ => {
            for side in [-1.0f32, 1.0] {
                lock(
                    &mut p.matte,
                    &[
                        Vec3::new(side * 0.09, 0.25, 0.02),
                        Vec3::new(side * 0.115, 0.15, 0.02),
                        Vec3::new(side * 0.105, 0.07, 0.0),
                    ],
                    0.06,
                    0.04,
                    tone(50 + side as u32),
                );
            }
            // Locks round the back of the head curve in under the jaw line,
            // a softer, layered cut instead of one solid helmet of hair.
            for k in 0..7 {
                let a = (k as f32 / 6.0 - 0.5) * 2.6 + PI;
                let out = Vec3::new(a.sin(), 0.0, a.cos());
                let root = Vec3::new(out.x * 0.07, 0.24, out.z * 0.06 - 0.02);
                lock(
                    &mut p.matte,
                    &[
                        root,
                        Vec3::new(out.x * 0.11, 0.15, out.z * 0.11 - 0.02),
                        Vec3::new(out.x * 0.115, 0.07, out.z * 0.115 - 0.02),
                        Vec3::new(out.x * 0.095, 0.03, out.z * 0.095 - 0.02),
                    ],
                    0.056,
                    0.034,
                    tone(80 + k),
                );
            }
            // A side-swept fringe over the brow.
            lock(
                &mut p.matte,
                &[
                    Vec3::new(-0.06, 0.272, 0.04),
                    Vec3::new(0.0, 0.258, 0.072),
                    Vec3::new(0.055, 0.228, 0.083),
                ],
                0.044,
                0.016,
                tone(90),
            );
        }
    }
}

/// A sculpted limb segment hanging down from its joint: a tapering core
/// from `r0` to `r1` over `len`, a muscle belly of `belly` a third of the way
/// down (pushed forward by `forward`), rounded at both ends so neighbouring
/// segments overlap without a seam.
fn limb(b: &mut Builder, len: f32, r0: f32, r1: f32, belly: f32, forward: f32, color: Vec3) {
    sculpted_limb(b, LimbShape::new(len, r0, r1, belly, forward), color);
}

pub(super) fn limb_form(shape: LimbShape) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    if let Some(contour) = shape.contour {
        let mut form = Sculpt::default();
        for mass in contour.masses {
            form.add(Ellipsoid(mass.center, mass.radii), contour.blend);
        }
        return form;
    }
    let LimbShape {
        length: len,
        radius_top: r0,
        radius_bottom: r1,
        belly,
        belly_center,
        ..
    } = shape;
    let mut s = Sculpt::default();
    s.add(Capsule(Vec3::ZERO, Vec3::Y * -len, r0, r1), 0.0);
    if belly.x > 0.0 {
        s.add(Ellipsoid(belly_center, belly), 0.03);
    }
    s
}

fn sculpted_limb(b: &mut Builder, shape: LimbShape, color: Vec3) {
    let (low, high) = shape.mesh_bounds(0.02);
    limb_form(shape).mesh(b, low, high, 0.007, &|at, _| {
        color * crate::sdf::Sculpt::weave(at)
    });
}

/// How far cloth stands off the body beneath it, by build.
pub(super) const SLEEVE_EASE: f32 = 0.007;
pub(super) const TROUSER_EASE: f32 = 0.008;

/// A cloth layer over a limb: the anatomy grown outward by `ease` so the
/// muscle hollows fill and it reads as fabric, gathered into soft rolls at
/// each fraction of its length in `rolls` (the crook of a joint).
pub(super) fn cloth_over(
    mut form: crate::sdf::Sculpt,
    ease: f32,
    rolls: &[(f32, f32)],
) -> crate::sdf::Sculpt {
    use crate::sdf::Shape::Ellipsoid;
    form.inflate(ease);
    for &(y, radius) in rolls {
        form.add(
            Ellipsoid(
                Vec3::new(0.0, y, 0.004),
                Vec3::new(radius, 0.013, radius * 1.04),
            ),
            0.012,
        );
    }
    form
}

/// Trousers over a thigh or a shin, rolled at the knee.
fn trouser_leg(b: &mut Builder, shape: LimbShape, build: f32, knee: bool, color: Vec3) {
    let ease = TROUSER_EASE * build;
    let rolls: &[(f32, f32)] = if knee {
        &[(-0.035, shape.radius_top + ease + 0.004)]
    } else {
        &[(-shape.length + 0.03, shape.radius_bottom + ease + 0.005)]
    };
    let (low, high) = shape.mesh_bounds(0.03);
    cloth_over(limb_form(shape), ease, rolls).mesh(b, low, high, 0.007, &|at, _| {
        color * crate::sdf::Sculpt::weave(at)
    });
}

/// How hanging cloth is cut: fitted at the waist, rounding out over the
/// hips, then falling to its hem. Radii are across and front to back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SkirtCut {
    pub waist: Vec2,
    pub hip: Vec2,
    /// How far down the cloth, 0 at the waist and 1 at the hem, the hip is.
    pub hip_at: f32,
    pub hem: Vec2,
    /// A cutaway coat: its fronts part from the belt down and sweep back
    /// around the legs instead of closing over them.
    pub cutaway: bool,
}

impl SkirtCut {
    /// A plain flare from the top edge to the hem, with no hip of its own.
    pub fn straight(top: Vec2, bottom: Vec2) -> Self {
        Self {
            waist: top,
            hip: top,
            hip_at: 0.0,
            hem: bottom,
            cutaway: false,
        }
    }

    /// The radii `t` of the way down: a rounded swell from the waist to the
    /// hip, then an eased fall to the hem.
    pub fn radius(&self, t: f32) -> Vec2 {
        if t < self.hip_at {
            let s = t / self.hip_at;
            self.waist.lerp(self.hip, 1.0 - (1.0 - s) * (1.0 - s))
        } else {
            let s = (t - self.hip_at) / (1.0 - self.hip_at).max(1e-4);
            self.hip.lerp(self.hem, ease(s))
        }
    }
}

/// One point of a draped garment at `t` down its length and angle `a`.
/// A free hem hangs loose: its folds deepen toward the edge, and the cloth
/// drops lower where each fold swings out, so the hem is never a level ring.
/// Short pieces such as cowls keep their neat edge.
#[allow(clippy::too_many_arguments)]
pub(super) fn drape_point(
    cut: SkirtCut,
    length: f32,
    folds: u32,
    split: f32,
    phase: f32,
    t: f32,
    a: f32,
    inner: bool,
) -> Vec3 {
    let r = cut.radius(t);
    let free = (length / 0.3).clamp(0.0, 1.0);
    let wave = (a * folds as f32 + phase).cos();
    let fold = wave * (0.007 * t.powf(1.2) + 0.01 * free * t.powi(3));
    let drop = 0.013 * free * (0.5 + 0.5 * wave) * ease((t - 0.6) / 0.4);
    let thickness = if inner { 0.007 } else { 0.0 };
    let hem_split =
        split * a.cos().max(0.0).powi(if split < 0.0 { 2 } else { 14 }) * ease((t - 0.55) / 0.45);
    // A slit parts only at the front; a cutaway's fronts part from the belt
    // and swing back across the whole front of the legs.
    let opening = if cut.cutaway {
        split.max(0.0) * a.cos().max(0.0).powi(2) * ease(t / 0.6)
    } else {
        split.max(0.0) * a.cos().max(0.0).powi(12) * ease((t - 0.3) / 0.7)
    };
    let side = if a < PI { 1.0 } else { -1.0 };
    Vec3::new(
        a.sin() * (r.x + fold - thickness) + side * opening,
        0.04 - (length + 0.04) * t + hem_split - drop,
        a.cos() * (r.y + fold - thickness),
    )
}

/// Radial ease at the emitted cloth grid's vertices. Local fitting keeps a
/// lifted front split from forcing unrelated side and back panels outward.
struct ClothFit {
    lift: Vec<f32>,
    rows: usize,
}

/// The surface used by both cloth emission and fitting another layer over it.
/// Keeping phase here prevents the fitter from silently using different folds.
#[derive(Clone)]
struct ClothSurface {
    cut: SkirtCut,
    length: f32,
    folds: u32,
    split: f32,
    phase: f32,
    /// How the hem is cut round the garment.
    hem: Hem,
    fit: Option<std::sync::Arc<ClothFit>>,
    seam: Option<std::sync::Arc<Vec<Vec3>>>,
    paired: bool,
}

impl ClothSurface {
    fn new(cut: SkirtCut, length: f32, folds: u32, split: f32, seed: u64) -> Self {
        Self {
            cut,
            length,
            folds,
            split,
            phase: rand(seed + 73, 0) * 0.4,
            hem: Hem::Even,
            fit: None,
            seam: None,
            paired: false,
        }
    }

    fn with_hem(self, hem: Hem) -> Self {
        Self { hem, ..self }
    }

    fn rows(&self) -> usize {
        ((self.length / 0.025).ceil() as usize).max(12)
    }

    fn point(&self, t: f32, angle: f32, inner: bool) -> Vec3 {
        let mut point = drape_point(
            self.cut,
            self.length * self.hem.reach(angle),
            self.folds,
            self.split,
            self.phase,
            t,
            angle,
            inner,
        );
        if self.paired {
            point.y -= self.split * angle.cos().max(0.0).powi(14) * ease((t - 0.55) / 0.45);
        }
        if let Some(seam) = &self.seam {
            let column = (angle / TAU).clamp(0.0, 1.0) * 64.0;
            let low = (column.floor() as usize).min(63);
            let target = seam[low].lerp(seam[low + 1], column - low as f32);
            let original = drape_point(
                self.cut,
                self.length,
                self.folds,
                self.split,
                self.phase,
                0.0,
                angle,
                false,
            );
            point += (target - original) * (1.0 - ease(t / 0.25));
        }
        if let Some(fit) = &self.fit {
            let row = t.clamp(0.0, 1.0) * fit.rows as f32;
            let low = (row.floor() as usize).min(fit.rows - 1);
            let column = (angle / TAU).clamp(0.0, 1.0) * 64.0;
            let left = (column.floor() as usize).min(63);
            let along = column - left as f32;
            let at = |r: usize| {
                let a = fit.lift[r * 65 + left];
                let b = fit.lift[r * 65 + left + 1];
                a + (b - a) * along
            };
            let lift = at(low) + (at(low + 1) - at(low)) * (row - low as f32);
            let direction = Vec3::new(point.x, 0.0, point.z).normalize_or_zero();
            point += direction * lift;
        }
        point
    }

    /// A raised waist must clear the full pelvis on the way down to the
    /// original hip cut, not only meet the body at its top seam.
    fn fit_pelvis(mut self, body: HumanoidBody) -> Self {
        let support = pelvic_supports(body, Vec3::ZERO);
        let rows = self.rows();
        let mut fit = ClothFit {
            lift: vec![0.0; (rows + 1) * 65],
            rows,
        };
        for row in 0..=rows {
            for column in 0..=64 {
                let t = row as f32 / rows as f32;
                let angle = column as f32 * TAU / 64.0;
                let point = self.point(t, angle, true);
                if !(-0.10..=0.16).contains(&point.y) {
                    continue;
                }
                let out = Vec3::new(point.x, 0.0, point.z).normalize_or(Vec3::Z);
                let body_point = pelvis_surface(&support, point.y, out);
                if support.distance(body_point).abs() < 0.002 {
                    fit.lift[row * 65 + column] =
                        (body_point.dot(out) + 0.008 - point.dot(out)).max(0.0);
                }
            }
        }
        self.fit = Some(std::sync::Arc::new(fit));
        self
    }

    /// Sample the emitted triangles, not the analytic curve between vertices.
    /// Concave folds can put a triangle outside that curve.
    fn mesh_point(&self, t: f32, angle: f32, inner: bool) -> Vec3 {
        let row = t.clamp(0.0, 1.0) * self.rows() as f32;
        let low = (row.floor() as usize).min(self.rows() - 1);
        let v = row - low as f32;
        let column = (angle / TAU).clamp(0.0, 1.0) * 64.0;
        let left = (column.floor() as usize).min(63);
        let u = column - left as f32;
        let at = |r, c| self.point(r as f32 / self.rows() as f32, c as f32 * TAU / 64.0, inner);
        let a = at(low, left);
        let b = at(low, left + 1);
        let c = at(low + 1, left + 1);
        let d = at(low + 1, left);
        if u >= v {
            a * (1.0 - u) + b * (u - v) + c * v
        } else {
            a * (1.0 - v) + c * u + d * (v - u)
        }
    }

    /// Fit an upright lower layer in the outer garment's coordinates. Each
    /// support sample expands only the cloth cells sharing its height and angle.
    /// This is resting fit; independently animated layers still need contact.
    fn fit_over(mut self, under: Self, under_to_self: Affine3A, under_trim: bool) -> Self {
        let rows = self.rows();
        let mut fit = ClothFit {
            lift: vec![0.0; (rows + 1) * 65],
            rows,
        };
        // Bound each actual emitted quad, including its midpoints. Radius
        // changes preserve angle and height, so these bounds remain valid.
        let mut cells = Vec::with_capacity(rows * 64);
        let delta = |angle: f32, center: f32| (angle - center + PI).rem_euclid(TAU) - PI;
        for row in 0..rows {
            for side in 0..64 {
                let center = (side as f32 + 0.5) * TAU / 64.0;
                let mut low_y = f32::INFINITY;
                let mut high_y = f32::NEG_INFINITY;
                let mut low_angle = f32::INFINITY;
                let mut high_angle = f32::NEG_INFINITY;
                for vertical in 0..=2 {
                    for horizontal in 0..=2 {
                        let t = (row as f32 + vertical as f32 * 0.5) / rows as f32;
                        let a = (side as f32 + horizontal as f32 * 0.5) * TAU / 64.0;
                        let point = self.point(t, a, true);
                        low_y = low_y.min(point.y);
                        high_y = high_y.max(point.y);
                        let angle = delta(point.x.atan2(point.z), center);
                        low_angle = low_angle.min(angle);
                        high_angle = high_angle.max(angle);
                    }
                }
                let corners = [
                    (row, side),
                    (row, side + 1),
                    (row + 1, side),
                    (row + 1, side + 1),
                ]
                .map(|(r, c)| {
                    let point = self.point(r as f32 / rows as f32, c as f32 * TAU / 64.0, true);
                    (r * 65 + c, point)
                });
                cells.push((center, low_y, high_y, low_angle, high_angle, corners));
            }
        }
        // Sample between the lower mesh's emitted rows/columns too. The
        // margin covers triangle chords and the raised trim's radial extent.
        const GAP: f32 = 0.005;
        for row in 0..=under.rows() * 2 {
            let t = row as f32 / (under.rows() * 2) as f32;
            for side in 0..=128 {
                let angle = side as f32 * TAU / 128.0;
                let mut point = under.mesh_point(t, angle, false);
                if under_trim {
                    point += Vec3::new(angle.sin(), 0.0, angle.cos()) * 0.002;
                }
                let point = under_to_self.transform_point3(point);
                let radius = point.xz().length() + GAP;
                let direction = point.x.atan2(point.z);
                for &(center, low_y, high_y, low_angle, high_angle, corners) in &cells {
                    if point.y < low_y - GAP || point.y > high_y + GAP {
                        continue;
                    }
                    let angle = delta(direction, center);
                    if angle < low_angle - 0.015 || angle > high_angle + 0.015 {
                        continue;
                    }
                    // Compare at the support's actual height and direction.
                    // Using every corner's radius at once turns a sloping quad
                    // into its widest enclosing cylinder and creates shelves.
                    let y = point.y.clamp(low_y + 1e-6, high_y - 1e-6);
                    let origin = Vec3::Y * y;
                    let ray = Vec3::new(direction.sin(), 0.0, direction.cos());
                    let vertices = corners.map(|(_, p)| p);
                    let mut hit = None;
                    for indices in [[0, 1, 3], [0, 3, 2]] {
                        let [a, b, c] = indices.map(|i| vertices[i]);
                        let e1 = b - a;
                        let e2 = c - a;
                        let h = ray.cross(e2);
                        let determinant = e1.dot(h);
                        if determinant.abs() < 1e-9 {
                            continue;
                        }
                        let delta = origin - a;
                        let u = delta.dot(h) / determinant;
                        let q = delta.cross(e1);
                        let v = ray.dot(q) / determinant;
                        let reach = e2.dot(q) / determinant;
                        if u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001 && reach > 0.0 {
                            hit = Some(reach);
                        }
                    }
                    // Neighbouring support inside the clearance margin can
                    // lie just outside the triangle. Extend its edge profile
                    // conservatively instead of dropping that support.
                    let inner_radius = hit.unwrap_or_else(|| {
                        let u = ((angle - low_angle) / (high_angle - low_angle).max(1e-6))
                            .clamp(0.0, 1.0);
                        let top = vertices[0].lerp(vertices[1], u);
                        let bottom = vertices[2].lerp(vertices[3], u);
                        let v = ((top.y - y) / (top.y - bottom.y).max(1e-6)).clamp(0.0, 1.0);
                        top.xz().length() + (bottom.xz().length() - top.xz().length()) * v
                    });
                    let lift = (radius - inner_radius).max(0.0);
                    for (index, _) in corners {
                        fit.lift[index] = fit.lift[index].max(lift);
                    }
                }
            }
        }
        // The front's duplicate grid columns are one sewn edge until a slit
        // actually opens. Asymmetric support must not separate that edge.
        for row in 0..=rows {
            let t = row as f32 / rows as f32;
            if self.point(t, 0.0, true).distance(self.point(t, TAU, true)) < 1e-6 {
                let lift = fit.lift[row * 65].max(fit.lift[row * 65 + 64]);
                fit.lift[row * 65] = lift;
                fit.lift[row * 65 + 64] = lift;
            }
        }
        // This pass measures additional clearance from the already fitted
        // surface. Keep earlier supports when fitting a stack of underlayers.
        if let Some(previous) = &self.fit {
            for (lift, previous) in fit.lift.iter_mut().zip(&previous.lift) {
                *lift += previous;
            }
        }
        self.fit = Some(std::sync::Arc::new(fit));
        self
    }
}

/// One articulated plate: a band from `top` down to `bottom` (heights along a
/// limb), with its radius at each edge.
#[derive(Clone, Copy, Debug)]
pub(super) struct Lame {
    pub top: f32,
    pub bottom: f32,
    pub radius_top: f32,
    pub radius_bottom: f32,
}

/// The cuisse over a thigh: four lames from the hip to the knee cop, cut
/// for this wearer's thigh.
pub(super) fn cuisse_lames(body: HumanoidBody) -> Vec<Lame> {
    fitted_lames(&limb_form(body.thigh), -0.04, 4, 0.085, PLATE_CLEARANCE)
}

/// The rerebrace below a pauldron: three lames down the upper arm, cut for
/// this wearer's arm.
pub(super) fn rerebrace_lames(body: HumanoidBody) -> Vec<Lame> {
    let arm = upper_arm_form(1.0, body.build, body.arm_length, body.profile.muscle, 0.94);
    let length = body.arm_length;
    fitted_lames(&arm, -0.08 * length, 3, 0.055 * length, PLATE_CLEARANCE)
}

/// How far plate stands off the limb it covers: room for the padding and
/// mail beneath it, and for the limb's mesh facets.
const PLATE_CLEARANCE: f32 = 0.007;

/// How far a sculpted limb reaches from its axis at a height.
fn limb_reach(form: &crate::sdf::Sculpt, y: f32) -> f32 {
    (0..24)
        .filter_map(|k| {
            let a = k as f32 / 24.0 * TAU;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            let hit = form.onto(Vec3::new(0.0, y, 0.0) + out * 0.3, -out);
            (form.distance(hit).abs() < 0.002).then(|| Vec2::new(hit.x, hit.z).length())
        })
        .fold(0.0, f32::max)
}

/// Overlapping lames fitted over a limb's sculpted form. Each band clears
/// the limb along its whole span, and its upper edge tucks inside the lower
/// edge of the band above, so the plates slide as the limb bends without
/// opening a gap; a swelling muscle widens the plates above it rather than
/// turning a shingle the wrong way.
pub(super) fn fitted_lames(
    form: &crate::sdf::Sculpt,
    top: f32,
    count: usize,
    length: f32,
    clearance: f32,
) -> Vec<Lame> {
    const TUCK: f32 = 0.004;
    const FLARE: f32 = 0.003;
    let overlap = length * 0.3;
    let mut bands: Vec<Lame> = (0..count)
        .map(|k| {
            let upper = top - k as f32 * (length - overlap);
            let lower = upper - length;
            let need = (0..=4)
                .map(|s| limb_reach(form, upper + (lower - upper) * s as f32 / 4.0))
                .fold(0.0, f32::max)
                + clearance;
            Lame {
                top: upper,
                bottom: lower,
                radius_top: need,
                radius_bottom: need + FLARE,
            }
        })
        .collect();
    for k in (0..count.saturating_sub(1)).rev() {
        let inside = bands[k + 1].radius_top + TUCK;
        if bands[k].radius_bottom < inside {
            bands[k].radius_bottom = inside;
            bands[k].radius_top = bands[k].radius_top.max(inside - FLARE);
        }
    }
    bands
}

/// Draw overlapping lames as shallow metal bands with darker lower edges.
fn plate_lames(b: &mut Builder, bands: &[Lame]) {
    for (k, &lame) in bands.iter().enumerate() {
        let shade = 1.0 - 0.04 * k as f32;
        b.tube(
            &[Vec3::Y * lame.top, Vec3::Y * lame.bottom],
            &[
                Vec2::new(lame.radius_top, lame.radius_top * 0.96),
                Vec2::new(lame.radius_bottom, lame.radius_bottom * 0.96),
            ],
            16,
            &|_, _, along| STEEL * shade * if along > 0.85 { 0.82 } else { 1.0 },
        );
    }
}

/// Whether a kind's tunic skirt is a cutaway coat, as the leprechauns wear
/// on their sheet: open from the belt over trousered legs.
pub(super) fn cutaway_coat(style: Style) -> bool {
    style == Style::Leprechaun
}

/// Where the armhole's curve turns under the arm.
const ARMPIT: f32 = 0.44;

/// Where a sleeveless bodice's armhole edge lies at a height, from the
/// midline: down from the shoulder strap and closing under the arm, so the
/// bodice wraps the whole torso below the armpit. In front (`front` 1, the
/// back 0) it curves out round a chest so the side of it stays covered.
pub(super) fn armhole_edge(body: HumanoidBody, y: f32, front: f32) -> f32 {
    let sh = body.shoulders * body.build;
    let side = 0.16 * sh;
    let under = ((ARMPIT - y) / 0.06).max(0.0);
    let edge = side + under * under * 0.2 * sh;
    let Some(bust) = body.bust else {
        return edge;
    };
    let (center, radii) = bust.mass(1.0, 0.0);
    let reach = (center.x + radii.x + 0.012).min(0.183 * sh * 0.97);
    let t = ((y - center.y) / (radii.y + 0.05)).clamp(-1.0, 1.0);
    edge.max(side + (reach - side).max(0.0) * (1.0 - t * t) * front)
}

/// Where a bikini top covers the torso: a cup over each side of her chest,
/// fitted to its own size and contour, and the band beneath it all round.
/// Returns how near the covered point lies to a cup's bound rim, 0 at its
/// heart to 1 at the edge; a man has neither cups nor band.
pub(super) fn bikini_cover(body: HumanoidBody, at: Vec3) -> Option<f32> {
    let bust = body.bust?;
    if (at.y - (bust.fold + 0.004)).abs() < 0.011 {
        return Some(0.0);
    }
    [-1.0f32, 1.0]
        .into_iter()
        .filter_map(|side| {
            let (center, radii) = bust.mass(side, 0.0);
            let dx = (at.x - center.x) / (radii.x * 1.12);
            let dy = (at.y - (center.y + 0.12 * radii.y)) / (radii.y * 1.28);
            let d = (dx * dx + dy * dy).sqrt();
            (d < 1.0 && at.z > center.z - radii.z * 0.8).then_some(d)
        })
        .reduce(f32::min)
}

/// Where a strapless bodice's top edge lies across the front at `x`: a
/// sweetheart rising over each side of her chest and dipping between; behind
/// (`front` 0) a level band at her underarm.
pub(super) fn strapless_top(body: HumanoidBody, x: f32, front: f32) -> f32 {
    sweetheart_top(body, x, front, 0.1)
}

fn sweetheart_top(body: HumanoidBody, x: f32, front: f32, dip_share: f32) -> f32 {
    let Some(bust) = body.bust else {
        return 0.37 + 0.0 * front;
    };
    let (left, l) = bust.mass(-1.0, 0.0);
    let (right, r) = bust.mass(1.0, 0.0);
    let (center, radii) = if x < 0.0 { (left, l) } else { (right, r) };
    let peak = center.y + radii.y * 0.55;
    // Independent peaks share one center dip and level back, avoiding a
    // discontinuity where two differently fitted halves meet.
    let dip = (left.y - l.y * dip_share).min(right.y - r.y * dip_share);
    let d = x.abs() / center.x.abs();
    let sweetheart = if d < 1.0 {
        dip + (peak - dip) * (1.0 - (1.0 - d) * (1.0 - d))
    } else {
        peak - (d - 1.0) * 0.06
    };
    let back = (left.y + l.y * 0.3 + right.y + r.y * 0.3) * 0.5;
    back + (sweetheart - back) * front
}

/// Where a one-shoulder bodice's top edge lies at `x`: over her right
/// shoulder, falling diagonally beneath her left arm.
pub(super) fn one_shoulder_top(body: HumanoidBody, x: f32) -> f32 {
    let sh = body.shoulders * body.build;
    let low = body
        .bust
        .map_or(0.36, |bust| bust.apex(1.0).y + bust.radii.y * 0.45);
    let t = ((x + 0.1 * sh) / (0.27 * sh)).clamp(0.0, 1.0);
    0.56 + (low - 0.56) * t
}

/// Shared continuous blend from back coverage to front coverage.
fn bodice_front(at: Vec3) -> f32 {
    ((at.z + 0.02) / 0.08).clamp(0.0, 1.0)
}

/// Whether a bodice covers a point of the torso, by what holds it up and
/// how far down it reaches; its neckline is cut separately.
pub(super) fn bodice_covers(
    body: HumanoidBody,
    bodice: Bodice,
    sleeveless: bool,
    at: Vec3,
) -> bool {
    if bodice.cover == Cover::Bare {
        return false;
    }
    let front = bodice_front(at);
    let fold = body.bust.map_or(0.27, |bust| bust.fold);
    if let Cover::Crop(hem) = bodice.cover {
        let angle = at.x.atan2(at.z);
        let edge = fold - 0.03 - 0.04 * (1.0 - hem.reach(angle));
        if at.y < edge {
            return false;
        }
    }
    let armhole = |front: f32| at.x.abs() > armhole_edge(body, at.y, front) || at.y > 0.51;
    match bodice.straps {
        Straps::Shoulders => !(sleeveless && armhole(front)),
        Straps::Strapless => at.y < strapless_top(body, at.x, front),
        Straps::Sweetheart => at.y < sweetheart_top(body, at.x, front, 0.55),
        Straps::Panels { width } => panel_parts::covers(body, at, width),
        // The strap itself runs over the top of her right shoulder; only
        // the arm's side of it is bare.
        Straps::OneShoulder => {
            at.y < one_shoulder_top(body, at.x)
                && !(at.x < 0.0 && sleeveless && at.x.abs() > armhole_edge(body, at.y, front))
        }
        Straps::Halter => {
            if front < 0.5 {
                // The back is bare above the band beneath her chest.
                return at.y < fold + 0.02;
            }
            let (from, wide) = body.bust.map_or((0.33, 0.12), |bust| {
                let (center, radii) = bust.mass(1.0, 0.0);
                (center.y + radii.y * 0.3, center.x + radii.x)
            });
            // Each panel narrows from the side of her chest to the neck.
            let t = ((at.y - from) / (0.55 - from)).clamp(0.0, 1.0);
            at.y < 0.55 && at.x.abs() < wide + (0.045 * body.build - wide) * t
        }
    }
}

/// A garment's V opening resolved on one wearer: its point on the sternum
/// and its edges rising to the collarbones, in torso coordinates.
#[derive(Clone, Copy, Debug)]
pub(super) struct NecklineCut {
    /// Height of the V's point and its half-width there.
    pub bottom: f32,
    pub point: f32,
    /// Height of the collarbones and the opening's half-width there.
    pub collar: f32,
    pub wide: f32,
    /// Whether a panel crosses over beneath the point.
    pub wrap: bool,
    /// A closed teardrop beneath the collar rather than a V to it.
    pub keyhole: bool,
}

impl NecklineCut {
    const THROAT: f32 = 0.46;
    const COLLAR: f32 = 0.53;

    /// The cut a neckline makes on this body, if it opens at all. The point
    /// follows the chest down the sternum; the edges always keep the fullest
    /// part of each side covered, however deep the plunge.
    pub fn resolve(neckline: Neckline, body: HumanoidBody) -> Option<Self> {
        let (depth, width, wrap, keyhole) = match neckline {
            Neckline::Closed => return None,
            Neckline::Plunge { depth, width, wrap } => (depth, width, wrap, false),
            Neckline::Keyhole { depth, width } => (depth, width, false, true),
        };
        let sh = body.shoulders * body.build;
        let fold = body.bust.map_or(0.3, |bust| bust.fold);
        let bottom = Self::THROAT + (fold - Self::THROAT) * depth;
        let mut cut = Self {
            bottom,
            point: 0.006,
            collar: Self::COLLAR,
            wide: width * sh,
            wrap,
            keyhole,
        };
        if let Some(bust) = body.bust {
            // At the height of the fullest point the edge stays well inside
            // it: the opening shows the upper slope and the sternum between,
            // never the fullness itself.
            // A centered opening is constrained by both independently fitted
            // sides. Tightening for the second side must never reopen the first.
            for side in [-1.0, 1.0] {
                let apex = bust.apex(side);
                let (_, radii) = bust.mass(side, 0.0);
                let limit = apex.x.abs() - 0.55 * radii.x;
                if apex.y > bottom && cut.edge(apex.y) > limit {
                    let t = (apex.y - bottom) / (Self::COLLAR - bottom);
                    cut.point = cut.point.min(limit * 0.5);
                    cut.wide = cut.point + (limit - cut.point) / t;
                }
            }
        }
        Some(cut)
    }

    /// The opening's half-width at a height, where it is open at all.
    pub fn edge(&self, y: f32) -> f32 {
        self.point + (self.wide - self.point) * (y - self.bottom) / (self.collar - self.bottom)
    }

    /// Whether a point on the front of the torso lies in the opening.
    pub fn open(&self, at: Vec3) -> bool {
        if self.keyhole {
            return at.z > 0.0 && self.keyhole_reach(at) < 1.0;
        }
        at.z > 0.0 && at.y > self.bottom && at.x.abs() < self.edge(at.y)
    }

    /// The keyhole's top, below the collar, and how far a point lies out
    /// across it: under 1 inside. Its edges follow the plunge's, narrowing
    /// to a round top.
    fn keyhole_reach(&self, at: Vec3) -> f32 {
        let top = self.collar - 0.05;
        let mid = (self.bottom + top) * 0.5;
        let half = (top - self.bottom) * 0.5;
        let wide = self.edge(mid).max(0.012);
        let v = (at.y - mid) / half;
        // Fuller below, as a drop.
        let w = wide * (1.0 - 0.35 * v.max(0.0));
        (at.x / w).powi(2) + v * v
    }

    /// Points round the keyhole's rim, for its binding.
    pub fn keyhole_rim(&self) -> Vec<Vec2> {
        let top = self.collar - 0.05;
        let mid = (self.bottom + top) * 0.5;
        let half = (top - self.bottom) * 0.5;
        let wide = self.edge(mid).max(0.012);
        (0..=20)
            .map(|k| {
                let a = k as f32 / 20.0 * TAU;
                let v = a.sin();
                Vec2::new(a.cos() * wide * (1.0 - 0.35 * v.max(0.0)), mid + v * half)
            })
            .collect()
    }

    /// The same opening carved through a layer worn over the garment, up
    /// to that layer's top: a tapering cut along the chest's front, `margin`
    /// wider so the outer edge frames the inner one.
    pub fn opening(&self, body: HumanoidBody, top: f32, margin: f32) -> crate::sdf::Shape {
        let b = body.build;
        let sh = body.shoulders * b;
        let above = top + 0.06;
        let low = body.bust.map_or(
            super::anatomy::chest_wall(b, sh, 0.0, self.bottom) + 0.02,
            |bust| bust.apex(1.0).z + 0.01,
        );
        crate::sdf::Shape::Capsule(
            Vec3::new(0.0, self.bottom - margin * 0.5, low),
            Vec3::new(
                0.0,
                above,
                super::anatomy::chest_wall(b, sh, 0.0, above) + 0.03,
            ),
            self.point + margin,
            self.edge(above) + margin,
        )
    }
}

/// How a tunic skirt is cut for a body, before the hips' Storybook
/// proportion is applied. The waistband hugs the pelvis it hangs from; the
/// cloth rounds out over the hips where the thighs join them, then falls to
/// a hem flared just enough to contain both thighs, diagonals included, at
/// every height down its `length`, so fuller hips or heavier builds never
/// push a thigh through.
pub(super) fn skirt_cut(
    body: HumanoidBody,
    flare: f32,
    length: f32,
    trousered: bool,
    cutaway: bool,
) -> SkirtCut {
    use super::anatomy::{HIP_SCALE, LIMB_SCALE};
    let b = body.build;
    let thigh = body.thigh;
    let ease_room = if trousered { TROUSER_EASE * b } else { 0.0 };
    let proportion = Vec2::new(HIP_SCALE.x, HIP_SCALE.z);
    let root = body.leg_root(1.0);
    // The thigh as sculpted, in the hips' space and as worn: its surface
    // found round each height, grown by any trouser ease and a margin.
    let form = limb_form(thigh);
    let surface = |y: f32| -> Vec<Vec2> {
        let along = y - root.y;
        (0..24)
            .filter_map(|k| {
                let a = k as f32 / 24.0 * TAU;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                let hit = form.onto(Vec3::new(0.0, along, 0.0) + out * 0.25, -out);
                (form.distance(hit).abs() < 0.002).then(|| {
                    let worn = hit * LIMB_SCALE + out * (ease_room + CLEARANCE);
                    Vec2::new(root.x + worn.x, worn.z)
                })
            })
            .collect()
    };
    // Room for the thigh mesh's surface-net step and the cloth's own
    // thickness, so neither surface's facets meet the other's.
    const CLEARANCE: f32 = 0.014;
    // The fold troughs that `drape_point` pulls in at this far down.
    let free = (length / 0.3).clamp(0.0, 1.0);
    let trough = |t: f32| 0.007 * t.powf(1.2) + 0.01 * free * t.powi(3);
    let contains = |radii: Vec2, t: f32, points: &[Vec2]| {
        let worn = radii * proportion - Vec2::splat(trough(t));
        points
            .iter()
            .all(|&point| (point / worn).length_squared() <= 1.0)
    };
    // The waistband sits on the pelvis where it hangs from, just below the
    // belt: the sculpted pelvis's own section at that height, measured
    // across and front to back, its seat included.
    let pelvis = pelvic_supports(body, Vec3::ZERO);
    let waist_y = 0.04;
    // It hangs from beneath the belt, so it is never shallower than the
    // belt above it, front to back.
    let reach = |dir: Vec3| pelvis_surface(&pelvis, waist_y, dir).dot(dir);
    let belt = super::robe::belt_radii(body, BELT_BOTTOM_Y);
    let depth = reach(Vec3::Z).max(reach(Vec3::NEG_Z)).max(belt.y);
    let top = Vec2::new(reach(Vec3::X), depth) * 1.01 + Vec2::splat(0.004);
    let bottom = Vec2::new(flare * b * body.hips_width, flare * 0.74 * b);
    // Below the crotch, where the thighs part from the pelvis, down to the hem.
    // The hip falls just below where the thighs join the pelvis.
    let hip_y = root.y - 0.015;
    let mut cut = SkirtCut {
        waist: top,
        hip: top,
        hip_at: ((waist_y - hip_y) / (length + waist_y)).min(0.6),
        hem: bottom,
        cutaway,
    };
    let rings: Vec<(f32, Vec<Vec2>)> = (0..)
        .map(|k| -0.02 * k as f32)
        .take_while(|&y| y > -length)
        .map(|y| {
            let mut points = surface(y);
            // A cutaway's fronts part around the legs.
            points.retain(|p| !cutaway || p.y < 0.02);
            (y, points)
        })
        .collect();
    // Which way the cloth is short at this ring: across, or front to back.
    let short = |radii: Vec2, t: f32, points: &[Vec2]| {
        let worn = radii * proportion - Vec2::splat(trough(t));
        points
            .iter()
            .map(|&point| point / worn)
            .filter(|q| q.length_squared() > 1.0)
            .fold(Vec2::ZERO, |most, q| most.max(q.abs()))
    };
    for _ in 0..120 {
        let (mut hip_short, mut hem_short) = (Vec2::ZERO, false);
        for (y, points) in &rings {
            let t = (waist_y - y) / (length + waist_y);
            if !contains(cut.radius(t), t, points) {
                // Widen whichever shapes the cloth here: the hip until the
                // fall towards the hem is well under way, then the hem.
                let fall = (t - cut.hip_at) / (1.0 - cut.hip_at).max(1e-4);
                if ease(fall) < 0.5 {
                    hip_short = hip_short.max(short(cut.radius(t), t, points));
                } else {
                    hem_short = true;
                }
            }
        }
        if hip_short == Vec2::ZERO && !hem_short {
            break;
        }
        // The hip widens only the way the thighs press on it, so a skirt
        // clearing the thighs at the sides still hugs the seat and belly.
        if hip_short.x >= hip_short.y && hip_short != Vec2::ZERO {
            cut.hip.x *= 1.015;
        } else if hip_short != Vec2::ZERO {
            cut.hip.y *= 1.015;
        }
        if hem_short {
            cut.hem *= 1.015;
        }
        // Cloth hangs from the hip; it never draws in beneath it.
        cut.hem = cut.hem.max(cut.hip);
    }
    cut
}

/// Whether her boots have heels: the kinds whose women wear them on their
/// concept sheets.
pub(super) fn heeled_boots(style: Style, woman: bool) -> bool {
    woman
        && matches!(
            style,
            Style::Vampire | Style::Medusa | Style::Phantom | Style::Wraith | Style::Leprechaun
        )
}

/// A hollow garment with vertical folds, an elliptical silhouette and a
/// narrow turned hem. Its length is the hem position, not a capsule's centre.
fn draped_cloth(b: &mut Builder, surface: ClothSurface, color: Vec3, hem_color: Option<Vec3>) {
    draped_cloth_painted(b, surface, &|_, _| color, hem_color);
}

/// A skirt's cloth as its recipe describes it: its colour, a front panel of
/// another and any pattern, painted onto the draped surface.
fn skirt_cloth(b: &mut Builder, surface: ClothSurface, skirt: Skirt) {
    let paint = move |at: Vec3, angle: f32| {
        let angle = (angle + PI).rem_euclid(TAU) - PI;
        let base = match skirt.panel {
            Some((color, share)) if angle.abs() < share * PI => color,
            _ => skirt.color,
        };
        match skirt.pattern {
            Pattern::Brocade { .. } => base,
            _ => skirt.pattern.paint(base, at),
        }
    };
    // Embroidery has its own narrow strokes; it must not inherit the much
    // larger quad size used to shade the underlying cloth.
    let thread_surface = surface.clone();
    if let Some(width) = skirt.center_panel {
        royal_parts::center_panel(b, &surface, width, skirt.color, skirt.trim);
    }
    if let Some(trim) = skirt.trim.filter(|_| skirt.split > 0.0) {
        let rows = surface.rows();
        for angle in [0.0, TAU] {
            let path: Vec<_> = (1..=rows)
                .map(|row| surface.point(row as f32 / rows as f32, angle, false) + Vec3::Z * 0.002)
                .collect();
            tube(b, &path, &vec![(0.003, 0.003); path.len()], trim);
        }
    }
    draped_cloth_painted(b, surface, &paint, skirt.trim);
    if let Pattern::Brocade { thread, motif } = skirt.pattern {
        embroidery_parts::embroider(b, &thread_surface, motif, thread);
    }
}

/// Hanging cloth whose colour is painted per quad: `paint` gets a point on
/// the cloth and its angle round, 0 at the front.
fn draped_cloth_painted(
    b: &mut Builder,
    surface: ClothSurface,
    paint: &dyn Fn(Vec3, f32) -> Vec3,
    hem_color: Option<Vec3>,
) {
    let ClothSurface {
        length,
        folds,
        split,
        phase,
        ..
    } = surface;
    let sides = 64;
    // Rows no more than a couple of centimetres apart, so a long skirt
    // still follows the round of the hip instead of cutting across it.
    let rows = surface.rows();
    let point = |t: f32, a: f32, inner: bool| surface.point(t, a, inner);
    for row in 0..rows {
        let t0 = row as f32 / rows as f32;
        let t1 = (row + 1) as f32 / rows as f32;
        for side in 0..sides {
            let a0 = side as f32 / sides as f32 * TAU;
            let a1 = (side + 1) as f32 / sides as f32 * TAU;
            let mid = (a0 + a1) * 0.5;
            let y = 0.04 - (length + 0.04) * (t0 + t1) * 0.5;
            let shade = 0.97 + 0.03 * (mid * folds as f32 + phase).cos();
            for inner in [false, true] {
                let inside = if inner {
                    Vec3::new(mid.sin() * 2.0, y, mid.cos() * 2.0)
                } else {
                    Vec3::Y * y
                };
                let color = paint(point((t0 + t1) * 0.5, mid, false), mid);
                b.quad_out(
                    [
                        point(t0, a0, inner),
                        point(t0, a1, inner),
                        point(t1, a1, inner),
                        point(t1, a0, inner),
                    ],
                    inside,
                    color * shade * if inner { 0.7 } else { 1.0 },
                );
            }
        }
    }
    for t in [0.0, 1.0] {
        for side in 0..sides {
            let a0 = side as f32 / sides as f32 * TAU;
            let a1 = (side + 1) as f32 / sides as f32 * TAU;
            b.quad_out(
                [
                    point(t, a0, false),
                    point(t, a1, false),
                    point(t, a1, true),
                    point(t, a0, true),
                ],
                Vec3::Y * (-length * 0.5),
                paint(point(t, a0, false), a0) * 0.83,
            );
        }
    }
    if split > 0.0 {
        // Close the turned front edges of the two panels, leaving an actual
        // opening rather than lifting a continuous bell-shaped hem.
        for a in [0.0f32, TAU] {
            for row in 0..rows {
                let t0 = row as f32 / rows as f32;
                let t1 = (row + 1) as f32 / rows as f32;
                let inside = Vec3::new(if a == 0.0 { 2.0 } else { -2.0 }, -length * 0.5, 0.0);
                b.quad_out(
                    [
                        point(t0, a, false),
                        point(t1, a, false),
                        point(t1, a, true),
                        point(t0, a, true),
                    ],
                    inside,
                    paint(point(t0, a, false), a) * 0.83,
                );
            }
        }
    }
    if let Some(hem_color) = hem_color {
        // The edging shares the actual elliptical hem and its folds; a
        // circular torus floats away from the front of a fitted long robe.
        let top_t = (1.0 - 0.018 / (length + 0.04)).max(0.0);
        for column in 0..sides {
            let a0 = column as f32 * TAU / sides as f32;
            let a1 = (column + 1) as f32 * TAU / sides as f32;
            let raised = |t, angle: f32| {
                point(t, angle, false) + Vec3::new(angle.sin(), 0.0, angle.cos()) * 0.002
            };
            b.quad_out(
                [
                    raised(top_t, a0),
                    raised(top_t, a1),
                    raised(1.0, a1),
                    raised(1.0, a0),
                ],
                Vec3::Y * -length,
                hem_color,
            );
        }
    }
}

/// A weathered stone slab: softly bevelled, its tone drifting across the
/// face, with moss settled on the upper surfaces.
fn stone(b: &mut Builder, center: Vec3, half: Vec3, color: Vec3, key: u64) {
    let moss = Vec3::new(0.38, 0.56, 0.17);
    let seed = key as f32 * 1.7;
    b.slab(center, half, Quat::IDENTITY, key, 0.06, &|dir, at| {
        let n = (at.x * 23.0 + seed).sin() * (at.z * 19.0 - seed).cos()
            + (at.y * 29.0 + at.x * 7.0).sin() * 0.6;
        let tone = color * (0.9 + 0.1 * (at.y * 9.0 + at.x * 5.0 + seed).sin());
        if (dir.y > 0.3 && n > 0.55) || (dir.y > -0.3 && n > 1.05) {
            moss
        } else {
            tone
        }
    });
}

/// A fist: a palm, four fingers curled under at the knuckles, and a thumb
/// wrapped across. `side` is +1 for the left hand, -1 for the right; `b`
/// scales it with the body.
fn fist(p: &mut Parts, at: Vec3, side: f32, b: f32, color: Vec3) {
    // The arm hangs with the palm toward the thigh (toward -side on x):
    // the knuckles run front to back, the fingers curl in toward the body,
    // and the thumb sits at the front, wrapping over them.
    let inward = -side;
    p.matte
        .blob(at, Vec3::new(0.03, 0.05, 0.044) * b, 2, 0.0, color);
    for k in 0..4 {
        let z = (1.5 - k as f32) * 0.019 * b;
        let knuckle = at + Vec3::new(side * 0.004, -0.036 * b, z);
        let r = (0.0115 - k as f32 * 0.0008) * b;
        tube(
            &mut p.matte,
            &[
                knuckle,
                knuckle + Vec3::new(inward * 0.018, -0.022, 0.0) * b,
                knuckle + Vec3::new(inward * 0.036, -0.012, 0.0) * b,
                knuckle + Vec3::new(inward * 0.036, 0.008, 0.0) * b,
            ],
            &[(r, r), (r, r), (r * 0.95, r * 0.95), (r * 0.8, r * 0.8)],
            color * (0.97 - k as f32 * 0.01),
        );
    }
    let base = at + Vec3::new(inward * 0.012, -0.004, 0.03) * b;
    tube(
        &mut p.matte,
        &[
            base,
            base + Vec3::new(inward * 0.014, -0.02, 0.016) * b,
            base + Vec3::new(inward * 0.03, -0.032, 0.004) * b,
        ],
        &[
            (0.014 * b, 0.014 * b),
            (0.012 * b, 0.012 * b),
            (0.01 * b, 0.01 * b),
        ],
        color,
    );
}

/// A free hand with separated long fingers: a relaxed casting palm or an
/// undead reaching claw. Held props retain the existing closed grip.
fn open_hand(p: &mut Parts, at: Vec3, side: f32, build: f32, color: Vec3, claw: bool) {
    let inward = -side;
    p.matte
        .blob(at, Vec3::new(0.027, 0.045, 0.044) * build, 2, 0.0, color);
    for finger in 0..4 {
        let spread = (1.5 - finger as f32) * 0.021;
        let short = [0.012, 0.0, 0.006, 0.023][finger];
        let root = at + Vec3::new(side * 0.003, -0.034, spread) * build;
        let radius = (0.011 - finger as f32 * 0.00065) * build;
        tube(
            &mut p.matte,
            &[
                root,
                at + Vec3::new(0.0, -0.071 + short * 0.4, spread * 1.15) * build,
                at + Vec3::new(inward * 0.012, -0.103 + short, spread * 1.35) * build,
                at + Vec3::new(
                    inward * if claw { 0.035 } else { 0.018 },
                    -0.112 + short + if claw { 0.012 } else { 0.0 },
                    spread * 1.4,
                ) * build,
            ],
            &[
                (radius, radius),
                (radius, radius),
                (radius * 0.83, radius * 0.83),
                (radius * 0.55, radius * 0.55),
            ],
            color,
        );
    }
    tube(
        &mut p.matte,
        &[
            at + Vec3::new(inward * 0.014, 0.0, 0.029) * build,
            at + Vec3::new(inward * 0.027, -0.015, 0.057) * build,
            at + Vec3::new(inward * 0.029, -0.042, 0.069) * build,
        ],
        &[
            (0.014 * build, 0.014 * build),
            (0.012 * build, 0.012 * build),
            (0.008 * build, 0.008 * build),
        ],
        color,
    );
}

/// Tufts of hair or fur: `count` tapered strands rising from around a
/// crown at `center`, swept along `sweep` and drooping at the tips. The
/// same strands make a fringe, a mane or a beard.
fn hair_tufts(
    p: &mut Parts,
    center: Vec3,
    radius: f32,
    count: u32,
    sweep: Vec3,
    color: Vec3,
    seed: u64,
) {
    let sweep = sweep.normalize_or(Vec3::Z);
    for k in 0..count {
        let a = k as f32 / count as f32 * TAU + rand(seed, k) * 0.5;
        let root = center
            + Vec3::new(
                a.cos() * radius * 0.7,
                rand(seed, k + 50) * 0.02,
                a.sin() * radius * 0.7,
            );
        let out = (Vec3::new(a.cos(), 0.1, a.sin()) * 0.6 + sweep).normalize();
        let length = radius * (0.7 + rand(seed, k + 100) * 0.4);
        let w = radius * (0.36 + rand(seed, k + 150) * 0.1);
        // Each lock lies along the head, lifting a little before it falls.
        tube(
            &mut p.matte,
            &[
                root,
                root + out * length * 0.5 + Vec3::Y * length * 0.05,
                root + out * length - Vec3::Y * length * 0.45,
            ],
            &[(w, w * 0.6), (w * 0.8, w * 0.5), (0.003, 0.003)],
            color * (0.92 + rand(seed, k + 200) * 0.16),
        );
    }
}

/// A smooth tube of one colour through `path` with elliptical `radii`.
fn tube(b: &mut Builder, path: &[Vec3], radii: &[(f32, f32)], color: Vec3) {
    let radii: Vec<Vec2> = radii.iter().map(|(x, z)| Vec2::new(*x, *z)).collect();
    b.tube(path, &radii, 10, &|_, _, _| color);
}

/// A soft leaf-shaped feather from `root`, curving outward along `out`
/// and drooping as it goes; flat, broad in the middle, pointed at the tip.
fn feather(p: &mut Parts, root: Vec3, out: Vec3, length: f32, width: f32, color: Vec3) {
    let down = Vec3::NEG_Y;
    let path = [
        root,
        root + out * length * 0.22 + down * length * 0.25,
        root + out * length * 0.34 + down * length * 0.6,
        root + out * length * 0.38 + down * length,
    ];
    let w = width * 0.5;
    let radii = [
        Vec2::new(w * 0.4, 0.006),
        Vec2::new(w, 0.01),
        Vec2::new(w * 0.62, 0.007),
        Vec2::new(0.002, 0.002),
    ];
    p.matte
        .tube(&path, &radii, 6, &|_, _, t| color * (1.0 - 0.18 * t));
}

/// Eyes with whites, a coloured iris, a pupil and a catchlight, set a
/// little forward of `center` on either side.
fn cartoon_eyes(p: &mut Parts, center: Vec3, spread: f32, size: f32, iris: Vec3) {
    for side in [-1.0f32, 1.0] {
        let at = center + Vec3::X * side * spread;
        p.matte.blob(
            at,
            Vec3::new(size * 1.55, size * 1.75, size * 1.0),
            0,
            0.0,
            Vec3::new(0.95, 0.94, 0.9),
        );
        p.matte.blob(
            at + Vec3::new(0.0, 0.0, size * 0.62),
            Vec3::new(size * 1.0, size * 1.1, size * 0.5),
            0,
            0.0,
            iris,
        );
        p.matte.blob(
            at + Vec3::new(0.0, 0.0, size * 0.92),
            Vec3::new(size * 0.55, size * 0.62, size * 0.25),
            0,
            0.0,
            DARK,
        );
        p.matte.blob(
            at + Vec3::new(size * 0.35, size * 0.42, size * 1.05),
            Vec3::splat(size * 0.22),
            0,
            0.0,
            Vec3::splat(1.6),
        );
    }
}

/// Find the first actual front/back hit at a fixed lateral position. Whether
/// z=0 lies inside is not a reliable silhouette test on a curved or cut garment.
/// A miss remains explicit so a sewn edge can end at the silhouette.
pub(super) fn torso_surface_hit(
    form: &crate::sdf::Sculpt,
    x: f32,
    y: f32,
    front: bool,
) -> Option<Vec3> {
    torso_surface_hit_offset(form, x, y, front, 0.0)
}

/// Trace a clearance isosurface, reserving volume for attachments rather than
/// flattening every vertex against the unexpanded support.
pub(super) fn torso_surface_hit_offset(
    form: &crate::sdf::Sculpt,
    x: f32,
    y: f32,
    front: bool,
    clearance: f32,
) -> Option<Vec3> {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    surface_ray_hit(form, Vec3::new(x, y, 0.0), outward, clearance, 0.45)
}

/// Exterior-to-interior ray against an offset support surface.
pub(super) fn surface_ray_hit(
    form: &crate::sdf::Sculpt,
    origin: Vec3,
    outward: Vec3,
    clearance: f32,
    reach: f32,
) -> Option<Vec3> {
    let at = |depth: f32| origin + outward * depth;
    let mut depth = reach;
    let mut previous = depth;
    for _ in 0..128 {
        let distance = form.distance(at(depth)) - clearance;
        if distance <= 0.0 {
            let (mut inside, mut outside) = (depth, previous);
            for _ in 0..10 {
                let middle = (inside + outside) * 0.5;
                if form.distance(at(middle)) <= clearance {
                    inside = middle;
                } else {
                    outside = middle;
                }
            }
            return Some(at((inside + outside) * 0.5));
        }
        if distance < 0.0002 {
            return Some(at(depth));
        }
        previous = depth;
        depth -= distance.clamp(0.002, 0.025);
        if depth < -0.35 {
            break;
        }
    }
    None
}

/// Point attachments may slide to a lateral edge when their authored ray misses.
/// Full-width ribbons instead retain the exact pattern plane and end as a row.
fn torso_surface(form: &crate::sdf::Sculpt, x: f32, y: f32, front: bool) -> Vec3 {
    let hit = |lateral| torso_surface_hit(form, lateral, y, front);
    if let Some(surface) = hit(x) {
        return surface;
    }
    // All fitted profiles stay within the garment's vertical extent. Near
    // an armhole or waist edge, preserve height and find the last supported x.
    let Some(mut surface) = hit(0.0) else {
        return Vec3::new(x, y, 0.0);
    };
    let (mut inside, mut outside) = (0.0, x);
    for _ in 0..10 {
        let middle = (inside + outside) * 0.5;
        if let Some(at) = hit(middle) {
            inside = middle;
            surface = at;
        } else {
            outside = middle;
        }
    }
    surface
}

/// Outward sculpt normal shared by torso, shoulder and waist attachments.
fn sculpt_surface_normal(form: &crate::sdf::Sculpt, surface: Vec3, outward: Vec3) -> Vec3 {
    let e = 0.0015;
    Vec3::new(
        form.distance(surface + Vec3::X * e) - form.distance(surface - Vec3::X * e),
        form.distance(surface + Vec3::Y * e) - form.distance(surface - Vec3::Y * e),
        form.distance(surface + Vec3::Z * e) - form.distance(surface - Vec3::Z * e),
    )
    .normalize_or(outward)
}

/// A front/back hit paired with its outward surface normal.
fn torso_surface_frame(form: &crate::sdf::Sculpt, at: Vec2, front: bool) -> (Vec3, Vec3) {
    let surface = torso_surface(form, at.x, at.y, front);
    (
        surface,
        sculpt_surface_normal(form, surface, if front { Vec3::Z } else { Vec3::NEG_Z }),
    )
}

/// Cloth laid onto a chest or back, rather than a second closed torso that
/// intersects the first. Profile entries are height and half width.
fn fitted_panel(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    profile: &[(f32, f32)],
    front: bool,
    offset: f32,
    color: Vec3,
) {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let at = |y: f32, x: f32| torso_surface(form, x, y, front) + outward * offset;
    let columns = (profile
        .iter()
        .map(|(_, width)| *width)
        .fold(0.0f32, f32::max)
        * 2.0
        / 0.012)
        .ceil()
        .max(4.0) as usize;
    for pair in profile.windows(2) {
        // Long chords cut through a curved bust even when their endpoints
        // touch the surface. Keep both directions close to the torso mesh's
        // resolution, including the narrow vertical trim on fitted robes.
        let rows = ((pair[1].0 - pair[0].0).abs() / 0.012).ceil().max(1.0) as usize;
        let points: Vec<Vec<Vec3>> = (0..=rows)
            .map(|row| {
                let t = row as f32 / rows as f32;
                let (height, width) = if row == rows {
                    pair[1]
                } else {
                    (
                        pair[0].0 + (pair[1].0 - pair[0].0) * t,
                        pair[0].1 + (pair[1].1 - pair[0].1) * t,
                    )
                };
                (0..=columns)
                    .map(|column| at(height, (column as f32 / columns as f32 * 2.0 - 1.0) * width))
                    .collect()
            })
            .collect();
        for row in 0..rows {
            for column in 0..columns {
                b.quad_out(
                    [
                        points[row][column],
                        points[row][column + 1],
                        points[row + 1][column + 1],
                        points[row + 1][column],
                    ],
                    Vec3::Y * (points[row][column].y + points[row + 1][column].y) * 0.5,
                    color,
                );
            }
        }
    }
}

// Waist-band landmarks in the hips' frame, shared by belt construction and
// torso accessory anchoring.
pub(super) const BELT_BOTTOM_Y: f32 = 0.045;
pub(super) const BELT_TOP_Y: f32 = 0.095;
pub(super) const BELT_CENTER_Y: f32 = (BELT_BOTTOM_Y + BELT_TOP_Y) * 0.5;

/// An open buckle frame and tongue in a caller-supplied attachment frame.
/// The strap remains visible through its windows; the tongue joins the rim.
fn buckle_frame(b: &mut Builder, at: Vec3, rotation: Quat, half_size: Vec2, color: Vec3) {
    let bar = half_size.min_element() * 0.17;
    let depth = bar * 0.8;
    for side in [-1.0, 1.0] {
        b.cuboid(
            at + rotation * Vec3::new(side * (half_size.x - bar), 0.0, 0.0),
            Vec3::new(bar, half_size.y, depth),
            rotation,
            color,
        );
        b.cuboid(
            at + rotation * Vec3::new(0.0, side * (half_size.y - bar), 0.0),
            Vec3::new(half_size.x - bar * 2.0, bar, depth),
            rotation,
            color,
        );
    }
    b.cuboid(
        at + rotation * Vec3::new(0.0, half_size.y * 0.18, depth * 0.3),
        Vec3::new(bar * 0.6, half_size.y * 0.65, depth * 0.7),
        rotation,
        color,
    );
}

/// A shared accessory assembly: the strap and its hardware use the same
/// garment surface, including the fuller female chest and fitted waist.
fn fitted_baldric(
    p: &mut Parts,
    garment: &crate::sdf::Sculpt,
    body: HumanoidBody,
    baldric: Baldric,
    overlay: f32,
) {
    let b = body.build;
    let side = if baldric.reverse { -1.0 } else { 1.0 };
    let path = [
        (
            Vec2::new(
                -side * 0.20 * body.shoulders * b,
                body.shoulder(1.0).y - 0.01,
            ),
            0.025 * b,
        ),
        (
            Vec2::new(-side * 0.16 * body.shoulders * b, 0.43),
            0.025 * b,
        ),
        (Vec2::new(-side * 0.065 * b, 0.325), 0.025 * b),
        (Vec2::new(side * 0.065 * b, 0.195), 0.025 * b),
        (
            Vec2::new(side * 0.13 * body.waist * b, BELT_CENTER_Y - body.torso.y),
            0.022 * b,
        ),
    ];
    // Trace the complete support envelope at the authored x/y coordinates.
    // Large normal offsets on a tightly curved waist can cross each other
    // and fold the strip; resolving the grown surface first keeps it a sheet.
    let skin = torso_shape(body, None, true);
    let mut support = torso_layer_support(&skin, garment);
    support.inflate(BALDRIC_SURFACE_OFFSET + overlay);
    let garment = &support;
    let front = ribbon_surface_rows(garment, &path, true, 0.0);
    let back = ribbon_surface_rows(garment, &path, false, 0.0);
    let shoulder = ribbon_surface_turn(garment, &front[0], &back[0], Vec3::Y);
    let waist = ribbon_surface_turn(
        garment,
        front.last().unwrap(),
        back.last().unwrap(),
        Vec3::X * side,
    );
    for (rows, outward) in [
        (&front, Vec3::Z),
        (&back, Vec3::NEG_Z),
        (&shoulder, Vec3::Y),
        (&waist, Vec3::X * side),
    ] {
        emit_ribbon_strip(&mut p.satin, rows, outward, baldric.color, false);
    }
    if baldric.buckle {
        // Place hardware on the strap's centreline, rather than choosing a
        // separate species-specific point that may drift off it.
        let segment = path
            .windows(2)
            .find(|pair| pair[0].0.y >= 0.3 && pair[1].0.y <= 0.3)
            .expect("baldric crosses the buckle height");
        let t = (segment[0].0.y - 0.3) / (segment[0].0.y - segment[1].0.y);
        let at = segment[0].0.lerp(segment[1].0, t);
        let (surface, normal) = torso_surface_frame(garment, at, true);
        let centre = surface + normal * 0.007;
        let direction = (segment[1].0 - segment[0].0).normalize();
        let before = torso_surface_frame(garment, at - direction * 0.01, true).0;
        let after = torso_surface_frame(garment, at + direction * 0.01, true).0;
        let across = (after - before).cross(normal).normalize_or(Vec3::X);
        let along = normal.cross(across).normalize_or(Vec3::Y);
        let frame = Quat::from_mat3(&Mat3::from_cols(across, along, normal));
        buckle_frame(
            &mut p.metal,
            centre,
            frame,
            Vec2::new(0.034 * b, 0.027 * b),
            Vec3::new(0.74, 0.5, 0.26),
        );
    }
}

/// A flat strap follows the garment across its full width. Sweeping a tube
/// through a few surface points can twist its wide axis away from the cloth
/// and interpolate back inside a fuller chest. Each row here is projected
/// after interpolation; the narrow turned edges give leather real thickness.
fn ribbon_surface_rows(
    form: &crate::sdf::Sculpt,
    path: &[(Vec2, f32)],
    front: bool,
    offset: f32,
) -> Vec<Vec<(Vec3, Vec3)>> {
    ribbon_path_rows(path)
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|at| {
                    let (surface, normal) = torso_surface_frame(form, at, front);
                    (surface + normal * offset, normal)
                })
                .collect()
        })
        .collect()
}

/// Sample a strip in the garment's pattern plane before fitting its depth.
/// Keeping the complete width in each row prevents independent edge clamps
/// from pinching a binding into overlapping triangles at the body silhouette.
fn ribbon_path_rows(path: &[(Vec2, f32)]) -> Vec<Vec<Vec2>> {
    if path.len() < 2 {
        return Vec::new();
    }
    let across = |i: usize| {
        let tangent = (path[(i + 1).min(path.len() - 1)].0 - path[i.saturating_sub(1)].0)
            .normalize_or(Vec2::Y);
        Vec2::new(tangent.y, -tangent.x)
    };
    let columns = (path.iter().map(|(_, width)| *width).fold(0.0f32, f32::max) * 2.0 / 0.006)
        .ceil()
        .max(4.0) as usize;
    let mut points = Vec::new();
    for (i, pair) in path.windows(2).enumerate() {
        let rows = ((pair[1].0 - pair[0].0).length() / 0.006).ceil().max(1.0) as usize;
        for row in 0..rows {
            let t = row as f32 / rows as f32;
            let center = pair[0].0.lerp(pair[1].0, t);
            let side = across(i).lerp(across(i + 1), t).normalize_or(Vec2::X);
            let width = pair[0].1 + (pair[1].1 - pair[0].1) * t;
            points.push(
                (0..=columns)
                    .map(|column| {
                        center + side * ((column as f32 / columns as f32 * 2.0 - 1.0) * width)
                    })
                    .collect(),
            );
        }
    }
    let (center, width) = path[path.len() - 1];
    points.push(
        (0..=columns)
            .map(|column| {
                center
                    + across(path.len() - 1)
                        * ((column as f32 / columns as f32 * 2.0 - 1.0) * width)
            })
            .collect(),
    );
    points
}

/// Join two already sampled edge rows around the top or flank of the support.
/// The cloned end rows keep adjoining patches watertight without hidden caps.
fn ribbon_surface_turn(
    form: &crate::sdf::Sculpt,
    start: &[(Vec3, Vec3)],
    end: &[(Vec3, Vec3)],
    outward: Vec3,
) -> Vec<Vec<(Vec3, Vec3)>> {
    assert_eq!(start.len(), end.len());
    let span = start
        .iter()
        .zip(end)
        .map(|(a, b)| a.0.distance(b.0))
        .fold(0.0f32, f32::max);
    let steps = (span / 0.012).ceil().max(4.0) as usize;
    let mut rows = vec![start.to_vec()];
    for row in 1..steps {
        let t = row as f32 / steps as f32;
        rows.push(
            start
                .iter()
                .zip(end)
                .map(|(a, b)| {
                    let from = a.0.lerp(b.0, t) + outward * 0.65;
                    let surface = form.onto(from, -outward);
                    debug_assert!(
                        surface.is_finite() && form.distance(surface).abs() < 0.002,
                        "ribbon turn must stay on its support: {surface}"
                    );
                    (surface, sculpt_surface_normal(form, surface, outward))
                })
                .collect(),
        );
    }
    rows.push(end.to_vec());
    rows
}

fn emit_ribbon_strip(
    b: &mut Builder,
    points: &[Vec<(Vec3, Vec3)>],
    outward: Vec3,
    color: Vec3,
    cap_ends: bool,
) {
    if points.len() < 2 || points[0].len() < 2 {
        return;
    }
    let columns = points[0].len() - 1;
    let thickness = 0.007;
    let inside_between = |a: (Vec3, Vec3), z: (Vec3, Vec3)| {
        (a.0 + z.0) * 0.5 - (a.1 + z.1).normalize_or(outward) * (thickness * 0.5)
    };
    for pair in points.windows(2) {
        for column in 0..columns {
            let q = [
                pair[0][column],
                pair[0][column + 1],
                pair[1][column + 1],
                pair[1][column],
            ];
            let center = q.iter().map(|point| point.0).sum::<Vec3>() * 0.25;
            let normal = q
                .iter()
                .map(|point| point.1)
                .sum::<Vec3>()
                .normalize_or(outward);
            let top = q.map(|point| point.0);
            // Each fitted face is a graph along this patch's projection ray.
            // A mean sculpt normal can point across a sharp shell opening,
            // reversing one half of a non-planar quad even without a fold.
            for face in [[top[0], top[1], top[2]], [top[0], top[2], top[3]]] {
                let center = face.iter().sum::<Vec3>() / 3.0;
                b.tri_out(face, center - outward * 0.1, color);
            }
            b.quad_out(
                q.map(|(point, normal)| point - normal * thickness),
                center + normal * 0.1,
                color * 0.8,
            );
        }
        let inside = inside_between(pair[0][columns / 2], pair[1][columns / 2]);
        for column in [0, columns] {
            let (a, z) = (pair[0][column], pair[1][column]);
            b.quad_out(
                [a.0, z.0, z.0 - z.1 * thickness, a.0 - a.1 * thickness],
                inside,
                color * 0.8,
            );
        }
    }
    if cap_ends {
        for (end, next) in [(0, 1), (points.len() - 1, points.len() - 2)] {
            let inside = inside_between(points[end][columns / 2], points[next][columns / 2]);
            for column in 0..columns {
                let (a, z) = (points[end][column], points[end][column + 1]);
                b.quad_out(
                    [a.0, z.0, z.0 - z.1 * thickness, a.0 - a.1 * thickness],
                    inside,
                    color * 0.8,
                );
            }
        }
    }
}

fn fitted_ribbon(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    path: &[(Vec2, f32)],
    front: bool,
    offset: f32,
    color: Vec3,
) {
    // Grow the support before tracing it. Moving already fitted vertices
    // along changing normals also moves their x/y and can fold a narrow seam.
    let mut support = form.clone();
    support.inflate(offset);
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let fit = |row: &[Vec2]| -> Option<Vec<(Vec3, Vec3)>> {
        row.iter()
            .map(|at| {
                // A sewn edge ends where its complete width leaves the
                // garment; no unsupported vertex is pulled toward the neck
                // or waist while its neighbours remain at their authored x/y.
                let surface = torso_surface_hit(&support, at.x, at.y, front)?;
                Some((surface, sculpt_surface_normal(&support, surface, outward)))
            })
            .collect()
    };
    let rows = ribbon_path_rows(path);
    let mut points = Vec::new();
    for row in rows {
        if let Some(row) = fit(&row) {
            points.push(row);
        } else if !points.is_empty() {
            emit_ribbon_strip(b, &points, outward, color, true);
            points.clear();
        }
    }
    emit_ribbon_strip(b, &points, outward, color, true);
}

/// An attachment can cross an opening or the top of an outer garment.
/// Preserve the body underneath instead of tracing only that clipped layer.
fn torso_layer_support(
    skin: &crate::sdf::Sculpt,
    garment: &crate::sdf::Sculpt,
) -> crate::sdf::Sculpt {
    let mut support = skin.clone();
    support.union(garment.clone(), 0.0);
    support
}

// Torso attachments reserve the same layers that the tabard renderer emits.
const TABARD_SURFACE_OFFSET: f32 = 0.014;
pub(super) const BALDRIC_SURFACE_OFFSET: f32 = 0.017;
const EMBLEM_SURFACE_OFFSET: f32 = 0.022;
const EMBLEM_FIELD_OFFSET: f32 = 0.026;

/// Heraldry is laid onto the tabard like embroidery. Project each small
/// triangle, so a badge follows both the chest and the hollow below it.
#[allow(clippy::too_many_arguments)]
fn fitted_emblem(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    center: Vec2,
    size: f32,
    front: bool,
    color: Vec3,
    field: Vec3,
    kind: u8,
) {
    let outward = if front { Vec3::Z } else { Vec3::NEG_Z };
    let triangle = |b: &mut Builder, tri: [Vec2; 3], offset: f32, pigment: Vec3| {
        let steps = (tri[0]
            .distance(tri[1])
            .max(tri[1].distance(tri[2]))
            .max(tri[2].distance(tri[0]))
            * size
            / 0.012)
            .ceil()
            .max(1.0) as usize;
        let points: Vec<Vec<Vec3>> = (0..=steps)
            .map(|row| {
                (0..=steps - row)
                    .map(|column| {
                        let uv = tri[0]
                            + (tri[1] - tri[0]) * (row as f32 / steps as f32)
                            + (tri[2] - tri[0]) * (column as f32 / steps as f32);
                        let at = center + Vec2::new(uv.x * outward.z, uv.y) * size;
                        torso_surface(form, at.x, at.y, front) + outward * offset
                    })
                    .collect()
            })
            .collect();
        for row in 0..steps {
            for column in 0..steps - row {
                let a = [
                    points[row][column],
                    points[row + 1][column],
                    points[row][column + 1],
                ];
                b.tri_out(a, a[0] - outward * 0.1, pigment);
                if column + row + 1 < steps {
                    let z = [
                        points[row + 1][column],
                        points[row + 1][column + 1],
                        points[row][column + 1],
                    ];
                    b.tri_out(z, z[0] - outward * 0.1, pigment);
                }
            }
        }
    };
    let disc = |b: &mut Builder, at: Vec2, radius: f32, offset: f32, pigment: Vec3| {
        for k in 0..32 {
            let angle = k as f32 * TAU / 32.0;
            let next = (k + 1) as f32 * TAU / 32.0;
            triangle(
                b,
                [
                    at,
                    at + Vec2::new(angle.cos(), angle.sin()) * radius,
                    at + Vec2::new(next.cos(), next.sin()) * radius,
                ],
                offset,
                pigment,
            );
        }
    };
    match kind % 4 {
        0 => {
            disc(b, Vec2::ZERO, 0.38, EMBLEM_SURFACE_OFFSET, color);
            for k in 0..8 {
                let angle = k as f32 * TAU / 8.0;
                let dir = Vec2::new(angle.cos(), angle.sin());
                let side = Vec2::new(-dir.y, dir.x) * 0.1;
                let radius = if k % 2 == 0 { 0.8 } else { 0.66 };
                triangle(
                    b,
                    [dir * 0.42 + side, dir * radius, dir * 0.42 - side],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
        }
        1 => {
            for k in 0..16 {
                let angle = k as f32 * TAU / 16.0;
                let next = (k + 1) as f32 * TAU / 16.0;
                let radius = |i: usize| match i % 4 {
                    0 => 0.85,
                    2 => 0.55,
                    _ => 0.22,
                };
                triangle(
                    b,
                    [
                        Vec2::ZERO,
                        Vec2::new(angle.cos(), angle.sin()) * radius(k),
                        Vec2::new(next.cos(), next.sin()) * radius(k + 1),
                    ],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
            disc(b, Vec2::ZERO, 0.15, EMBLEM_FIELD_OFFSET, field);
        }
        2 => {
            disc(b, Vec2::ZERO, 0.62, EMBLEM_SURFACE_OFFSET, color);
            disc(b, Vec2::new(0.0, 0.26), 0.56, EMBLEM_FIELD_OFFSET, field);
        }
        _ => {
            for (x, height, width) in [(0.0, 0.85, 0.3), (-0.3, 0.45, 0.2), (0.3, 0.45, 0.2)] {
                triangle(
                    b,
                    [
                        Vec2::new(x - width, -0.6),
                        Vec2::new(x * 0.6, height),
                        Vec2::new(x + width, -0.6),
                    ],
                    EMBLEM_SURFACE_OFFSET,
                    color,
                );
            }
            disc(b, Vec2::new(0.0, -0.45), 0.32, EMBLEM_SURFACE_OFFSET, color);
        }
    }
}

/// A belt or edging seated on the whole circumference of an authored torso.
fn fitted_band(
    b: &mut Builder,
    form: &crate::sdf::Sculpt,
    low: f32,
    high: f32,
    offset: f32,
    color: Vec3,
) {
    let at = |y: f32, angle: f32| {
        let outward = Vec3::new(angle.sin(), 0.0, angle.cos());
        form.onto(Vec3::Y * y + outward * 0.45, -outward) + outward * offset
    };
    let points: [[Vec3; 65]; 2] = [low, high]
        .map(|height| std::array::from_fn(|column| at(height, column as f32 * TAU / 64.0)));
    for column in 0..64 {
        b.quad_out(
            [
                points[0][column],
                points[0][column + 1],
                points[1][column + 1],
                points[1][column],
            ],
            Vec3::Y * (low + high) * 0.5,
            color,
        );
    }
}

/// The deltoid, biceps and triceps overlap the clavicle at the arm root. No
/// static shoulder ball is left on the torso when the arm rotates away.
fn biped_upper_arm(
    b: &mut Builder,
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    bare: bool,
    color: Vec3,
) {
    use crate::sdf::Sculpt;
    let fullness = if bare { 0.94 } else { 1.0 };
    let arm = upper_arm_form(side, build, length, muscle, fullness);
    arm.mesh(
        b,
        Vec3::new(-0.13 * build, -0.3 * length - 0.06 * build, -0.09 * build),
        Vec3::new(0.13 * build, 0.075 * build.max(1.0), 0.095 * build),
        0.007,
        &|at, _| color * if bare { 1.0 } else { Sculpt::weave(at) },
    );
}

/// A sleeve over the upper arm: eased off the deltoid and biceps, hanging a
/// little fuller beneath, gathered in the crook of the elbow so the bent
/// forearm never shows a gap between the two pieces of cloth.
fn biped_upper_sleeve(
    b: &mut Builder,
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    color: Vec3,
) {
    use crate::sdf::{Sculpt, Shape::*};
    let mut sleeve = upper_sleeve_form(side, build, length, muscle);
    // The underside of the sleeve falls away from the arm.
    sleeve.add(
        Ellipsoid(
            Vec3::new(side * 0.012 * build, -0.13 * length, -0.012 * build),
            Vec3::new(0.05 * build, 0.08 * length, 0.05 * build),
        ),
        0.03 * build,
    );
    sleeve.mesh(
        b,
        Vec3::new(-0.14 * build, -0.3 * length - 0.07 * build, -0.1 * build),
        Vec3::new(0.14 * build, 0.085 * build.max(1.0), 0.105 * build),
        0.007,
        &|at, _| color * Sculpt::weave(at),
    );
}

pub(super) fn upper_sleeve_form(
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
) -> crate::sdf::Sculpt {
    // Ease around the arm, with small compression folds only in its crook.
    let arm = upper_arm_form(side, build, length, 1.0 + (muscle - 1.0) * 0.5, 1.0);
    sleeve_parts::gathers(
        cloth_over(arm, SLEEVE_EASE * build, &[]),
        build,
        length,
        &[-0.245, -0.28],
    )
}

pub(super) fn upper_arm_form(
    side: f32,
    build: f32,
    length: f32,
    muscle: f32,
    fullness: f32,
) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let mut arm = Sculpt::default();
    // The root overlaps the clavicle's end and the deltoid caps it, both
    // rounding off level with the shoulder line rather than rising over it.
    arm.add(
        Capsule(
            Vec3::new(-side * 0.082 * build, -0.014 * build, -0.005),
            Vec3::new(-side * 0.012 * build, -0.06 * length, 0.0),
            0.03 * build,
            0.057 * build * fullness,
        ),
        0.0,
    )
    .add(
        Capsule(
            Vec3::new(0.0, -0.045 * length, 0.0),
            Vec3::Y * -0.3 * length,
            0.056 * build * fullness,
            0.037 * build,
        ),
        0.025 * build,
    )
    .add(
        Ellipsoid(
            Vec3::new(-side * 0.008 * build, -0.08 * length, -0.003),
            Vec3::new(
                0.066 * build * fullness * muscle,
                0.082 * length,
                0.064 * build * muscle,
            ),
        ),
        0.024 * build,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, -0.15 * length, 0.012 * build),
            Vec3::new(
                0.053 * build * fullness * muscle,
                0.09 * length,
                0.057 * build * muscle,
            ),
        ),
        0.02 * build,
    );
    arm
}

/// A forearm has a full extensor mass below the elbow and a slim wrist;
/// bare arms keep smooth skin instead of inheriting the cloth weave.
fn biped_forearm(b: &mut Builder, build: f32, length: f32, muscle: f32, bare: bool, color: Vec3) {
    use crate::sdf::Sculpt;
    let arm = if bare {
        forearm_form(build, length, muscle)
    } else {
        forearm_sleeve_form(build, length, muscle)
    };
    arm.mesh(
        b,
        Vec3::new(-0.085 * build, -0.3 * length, -0.085 * build),
        Vec3::new(0.085 * build, 0.065 * build, 0.085 * build),
        0.007,
        &|at, _| color * if bare { 1.0 } else { Sculpt::weave(at) },
    );
}

/// A forearm sleeve eases over the extensor mass and bunches just below
/// the elbow, where it tucks under the upper sleeve's folds.
pub(super) fn forearm_sleeve_form(build: f32, length: f32, muscle: f32) -> crate::sdf::Sculpt {
    sleeve_parts::gathers(
        cloth_over(
            forearm_form(build, length, 1.0 + (muscle - 1.0) * 0.5),
            SLEEVE_EASE * build * 0.85,
            &[],
        ),
        build,
        length,
        &[-0.025],
    )
}

pub(super) fn forearm_form(build: f32, length: f32, muscle: f32) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let mut arm = Sculpt::default();
    arm.add(
        // Both arm segments share this hinge-centred radius. Their
        // overlapping caps remain seated as the forearm bends, without a
        // larger second bulb forming a belt around the straight elbow.
        Capsule(
            Vec3::ZERO,
            Vec3::Y * -0.26 * length,
            0.037 * build,
            0.034 * build,
        ),
        0.0,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, -0.085 * length, 0.006 * build),
            Vec3::new(
                0.049 * build * muscle,
                0.095 * length,
                0.045 * build * muscle,
            ),
        ),
        0.016 * build,
    );
    arm
}

/// A neck widens into the collar and narrows beneath the jaw. The lower
/// station is buried below the garment opening even when the head turns.
/// The neck from the shoulders up into the head, in the head's space:
/// points up its middle and its radii (across, front to back) at each.
pub(super) fn neck_profile(build: f32, slim: f32) -> [(Vec3, Vec2); 4] {
    let scale = build * slim;
    [
        (
            Vec3::new(0.0, -0.065, -0.008),
            Vec2::new(0.056, 0.052) * scale,
        ),
        (
            Vec3::new(0.0, -0.022, -0.007),
            Vec2::new(0.05, 0.047) * scale,
        ),
        (
            Vec3::new(0.0, 0.033, -0.008),
            Vec2::new(0.045, 0.043) * scale,
        ),
        (
            Vec3::new(0.0, 0.085, -0.01),
            Vec2::new(0.045, 0.045) * scale,
        ),
    ]
}

/// How slender a neck is: hers finer than his, though never a stalk under
/// the storybook head.
pub(super) fn neck_slim(woman: bool) -> f32 {
    if woman {
        0.9
    } else {
        1.0
    }
}

fn biped_neck(b: &mut Builder, build: f32, slim: f32, color: Vec3) {
    // Straight profiles need no spline subdivision. Finish still smooths
    // the resulting mesh normally; restoring this flag leaves later forms
    // unchanged and keeps these small transition pieces inexpensive.
    let smooth = b.smooth;
    b.smooth = false;
    let profile = neck_profile(build, slim);
    tube(
        b,
        &profile.map(|(at, _)| at),
        &profile.map(|(_, r)| (r.x, r.y)),
        color,
    );
    b.smooth = smooth;
}

/// What a styled sleeve adds over the shoulder and upper arm: a puff, or a
/// gathered off-shoulder band, sheer or not.
fn styled_sleeve_top(
    p: &mut Parts,
    sleeve: super::outfit::Sleeve,
    side: f32,
    b: f32,
    arm_len: f32,
) {
    let builder = if sleeve.sheer {
        &mut p.sheer
    } else {
        &mut p.matte
    };
    let color = sleeve.color;
    if let Some(lining) = sleeve.puff_lining {
        sleeve_parts::slashed_puff(builder, side, b, arm_len, color, lining);
    } else if sleeve.puff || sleeve.cut == SleeveCut::Puff {
        // A gathered puff over the shoulder, banded beneath.
        builder.blob(
            Vec3::new(side * 0.008, -0.045 * arm_len, 0.0),
            Vec3::new(0.083, 0.078, 0.083) * b,
            7,
            0.08,
            color,
        );
        builder.torus(
            Vec3::Y * -0.105 * arm_len,
            Vec3::Y,
            0.052 * b,
            0.011 * b,
            14,
            4,
            sleeve.flare.unwrap_or(color * 0.85),
        );
    }
    if sleeve.cut == SleeveCut::OffShoulder {
        royal_parts::sleeve_swag(builder, b, arm_len, color, sleeve.flare);
    }
}

/// One sleeve or bracer follows the forearm's full upper mass and narrow
/// wrist. A thin coloured hem replaces stacked cylindrical cuff rings.
fn biped_cuff(
    b: &mut Builder,
    build: f32,
    length: f32,
    muscle: f32,
    bell: bool,
    color: Vec3,
    trim: Vec3,
) {
    let heights = if bell {
        [0.055, 0.11, 0.19, 0.25, 0.263]
    } else {
        [0.11, 0.145, 0.205, 0.25, 0.268]
    };
    let radii = if bell {
        [
            Vec2::new(0.052, 0.047),
            Vec2::new(0.057, 0.05),
            Vec2::new(0.072, 0.063),
            Vec2::new(0.084, 0.075),
            Vec2::new(0.083, 0.074),
        ]
    } else {
        [
            Vec2::new(0.053, 0.047),
            Vec2::new(0.057, 0.05),
            Vec2::new(0.049, 0.042),
            Vec2::new(0.042, 0.037),
            Vec2::new(0.043, 0.038),
        ]
    };
    let path = heights.map(|height| Vec3::Y * -height * length);
    // Follow proximal forearm fullness, then return to the authored wrist and
    // hem. Both recipes keep their final two stations and flare unchanged.
    // Bracers and cuffs are worn over the eased sleeve beneath them.
    let radii: [Vec2; 5] = std::array::from_fn(|i| {
        let influence = 1.0 - ease((heights[i] - 0.145) / (0.25 - 0.145));
        radii[i] * build * (1.0 + (muscle - 1.0) * influence) + Vec2::splat(SLEEVE_EASE * build)
    });
    if bell {
        let profile = std::array::from_fn(|i| (heights[i], radii[i]));
        sleeve_parts::bell(b, &profile, build, length, color, trim);
        return;
    }
    let smooth = b.smooth;
    b.smooth = false;
    b.tube(&path, &radii, 12, &|_, _at, along| {
        if along > 0.94 {
            trim
        } else {
            color
        }
    });
    b.smooth = smooth;
}

/// A broad forged crown over a thinner sliding lame. Ring profiles give the
/// metal deliberate planes; the hollow underside is fitted to the actual arm.
fn biped_pauldron(p: &mut Parts, side: f32, body: HumanoidBody) {
    let build = body.build;
    let mut support = upper_arm_form(side, build, body.arm_length, body.profile.muscle, 0.94);
    // Facet chords need room as well as their vertices. Padding is part of the
    // support, so it follows both the clavicle insertion and the muscle belly.
    support.inflate(0.012 * build);
    for (profile, tint, crown) in [
        (
            &[
                (0.042, 0.025),
                (0.030, 0.055),
                (0.014, 0.078),
                (-0.030, 0.098),
                (-0.072, 0.100),
                (-0.078, 0.100),
            ][..],
            1.0,
            true,
        ),
        (
            &[
                (-0.065, 0.088),
                (-0.098, 0.094),
                (-0.134, 0.087),
                (-0.140, 0.086),
            ][..],
            0.92,
            false,
        ),
    ] {
        const SIDES: usize = 20;
        let inner: Vec<[Vec3; SIDES]> = profile
            .iter()
            .enumerate()
            .map(|(row, &(height, radius))| {
                std::array::from_fn(|k| {
                    let angle = k as f32 / SIDES as f32 * TAU;
                    let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
                    let center = Vec3::Y * height * body.arm_length;
                    // Both ends of a panel bound its complete support span.
                    // A clavicle bulge between ring rows must not pierce a flat
                    // face whose endpoints happen to clear it.
                    let top = profile[row.saturating_sub(1)].0;
                    let bottom = profile[(row + 1).min(profile.len() - 1)].0;
                    let fitted = (0..=8)
                        .map(|step| {
                            let y = (top + (bottom - top) * step as f32 / 8.0) * body.arm_length;
                            let origin = Vec3::Y * y;
                            let hit = support.onto(origin + outward * 0.4, -outward);
                            if support.distance(hit).abs() < 0.002 {
                                (hit - origin).dot(outward)
                            } else {
                                0.0
                            }
                        })
                        .fold(0.0, f32::max);
                    center + outward * (radius * build).max(fitted)
                })
            })
            .collect();
        let outer: Vec<[Vec3; SIDES]> = inner
            .iter()
            .enumerate()
            .map(|(row, ring)| {
                std::array::from_fn(|k| {
                    let angle = k as f32 / SIDES as f32 * TAU;
                    let outward = Vec3::new(angle.cos(), 0.0, angle.sin());
                    // A turned lower edge catches a single readable gold line.
                    let rim = if row + 1 == inner.len() { 0.003 } else { 0.0 };
                    ring[k]
                        + outward * (0.008 + rim) * build
                        + if row == 0 && crown {
                            Vec3::Y * 0.008 * build
                        } else {
                            Vec3::ZERO
                        }
                })
            })
            .collect();
        for row in 0..inner.len() - 1 {
            for k in 0..SIDES {
                let next = (k + 1) % SIDES;
                let inside =
                    Vec3::Y * (profile[row].0 + profile[row + 1].0) * 0.5 * body.arm_length;
                p.metal.quad_out(
                    [
                        outer[row][k],
                        outer[row + 1][k],
                        outer[row + 1][next],
                        outer[row][next],
                    ],
                    inside,
                    if row + 2 == inner.len() {
                        GILT
                    } else {
                        STEEL * tint
                    },
                );
                // Reverse winding on the inside: the surface faces the arm.
                let q = [
                    inner[row][k],
                    inner[row][next],
                    inner[row + 1][next],
                    inner[row + 1][k],
                ];
                let normal = (q[1] - q[0]).cross(q[2] - q[0]);
                let mut q = q;
                if normal.dot((q[0] + q[2]) * 0.5 - inside) > 0.0 {
                    q.reverse();
                }
                p.metal.tri([q[0], q[1], q[2]], STEEL * 0.68);
                p.metal.tri([q[0], q[2], q[3]], STEEL * 0.68);
            }
        }
        for row in [0, inner.len() - 1] {
            let upward = if row == 0 { Vec3::Y } else { Vec3::NEG_Y };
            let inside = Vec3::Y * profile[row].0 * body.arm_length - upward;
            for k in 0..SIDES {
                let next = (k + 1) % SIDES;
                p.metal.quad_out(
                    [
                        inner[row][k],
                        outer[row][k],
                        outer[row][next],
                        inner[row][next],
                    ],
                    inside,
                    GILT,
                );
            }
        }
        if crown {
            let inner_center = Vec3::Y * profile[0].0 * body.arm_length;
            let outer_center = inner_center + Vec3::Y * 0.008 * build;
            for k in 0..SIDES {
                let next = (k + 1) % SIDES;
                p.metal.tri_out(
                    [outer_center, outer[0][k], outer[0][next]],
                    outer_center - Vec3::Y,
                    STEEL,
                );
                p.metal.tri_out(
                    [inner_center, inner[0][k], inner[0][next]],
                    inner_center + Vec3::Y,
                    STEEL * 0.68,
                );
            }
        }
    }
}

/// Construction of an existing outer garment, independent of its pigment.
#[derive(Clone, Copy)]
pub(super) enum TorsoMaterial {
    Cloth,
    Leather,
    Plate,
}

fn torso_garment(
    body: HumanoidBody,
    material: TorsoMaterial,
    trim_neck: bool,
) -> crate::sdf::Sculpt {
    torso_shape(body, Some(material), trim_neck)
}

/// Where a shell ends at the top: its own height, or for an under-chest
/// corset just beneath the wearer's chest.
pub(super) fn shell_top(body: HumanoidBody, top: f32, underbust: bool) -> f32 {
    if underbust {
        body.bust.map_or(top, |bust| bust.fold + 0.03)
    } else {
        top
    }
}

/// Resolve the shell once for emission and attachments. Openings belong to
/// this same surface so fitting cannot silently trace a different garment.
fn outfit_shell(
    body: HumanoidBody,
    outfit: &Outfit,
    trim_neck: bool,
) -> Option<crate::sdf::Sculpt> {
    use crate::sdf::Shape::*;
    let Some(Outer::Shell {
        top,
        sleeveless,
        leather,
        underbust,
        opening,
        ..
    }) = outfit.outer
    else {
        return None;
    };
    let top = shell_top(body, top, underbust);
    let b = body.build;
    let sh = body.shoulders * b;
    let plunge = NecklineCut::resolve(outfit.bodice.cut, body);
    let material = if leather {
        TorsoMaterial::Leather
    } else {
        TorsoMaterial::Cloth
    };
    let mut shell = torso_garment(body, material, trim_neck);
    shell
        .carve(Beyond(Vec3::Y, top), 0.01)
        .carve(Beyond(-Vec3::Y, 0.0), 0.0);
    if sleeveless {
        for side in [-1.0f32, 1.0] {
            shell.carve(Beyond(Vec3::X * side, 0.185 * sh), 0.008);
        }
    }
    match opening {
        ShellOpening::Closed => {}
        ShellOpening::ShapedNeck => {
            if let Some(cut) = plunge {
                shell.carve(cut.opening(body, top, 0.014), 0.006);
            } else {
                shell.carve(
                    Ellipsoid(
                        Vec3::new(0.0, top + 0.055, 0.135 * b),
                        Vec3::new(0.11 * sh, 0.105, 0.15 * b),
                    ),
                    0.006,
                );
            }
        }
        ShellOpening::PlungingCoat => {
            match plunge {
                Some(cut) => shell.carve(cut.opening(body, top, 0.01), 0.006),
                None => shell.carve(
                    Ellipsoid(
                        Vec3::new(0.0, top + 0.01, 0.15 * b),
                        Vec3::new(0.085 * sh, 0.15, 0.13 * b),
                    ),
                    0.006,
                ),
            };
        }
        ShellOpening::OpenCoat => {
            shell.carve(
                Ellipsoid(
                    Vec3::new(0.0, 0.2, 0.2 * b),
                    Vec3::new(0.075 * sh, 0.3, 0.12 * b),
                ),
                0.008,
            );
        }
    }
    Some(shell)
}

/// A closed waist section of the selected shell, expressed in hips space.
/// Current peplum recipes all attach below the neckline and arm openings.
/// Match both depth and centre: a fitted torso is not centred on z=0.
fn shell_waist(form: &crate::sdf::Sculpt, body: HumanoidBody, height: f32) -> (Vec2, Vec3) {
    let scale = storybook_part(Plan::Biped, J::Torso);
    let y = (height - body.torso.y) / scale.y;
    let front = torso_surface(form, 0.0, y, true).z;
    let back = torso_surface(form, 0.0, y, false).z;
    let centre = (front + back) * 0.5;
    let (mut inside, mut outside) = (0.0, 0.5 * body.build);
    for _ in 0..20 {
        let x = (inside + outside) * 0.5;
        if form.distance(Vec3::new(x, y, centre)) <= 0.0 {
            inside = x;
        } else {
            outside = x;
        }
    }
    // The 7mm turned cloth overlaps its support slightly at the sewn edge.
    let radii =
        Vec2::new((inside + outside) * 0.5, (front - back) * 0.5) * scale.xz() + Vec2::splat(0.003);
    let offset = body.torso + Vec3::new(0.0, y, centre) * scale - Vec3::Y * 0.04;
    (radii, offset)
}

/// A woman's chest, each side one form attached across the rib cage: a broad
/// base thickening the chest wall from sternum edge to the side, an upper
/// slope leaving the wall high on the chest, and the lower fullness it runs
/// into. `grow` lays a garment over the same supports.
pub(super) fn bust_supports(body: &mut crate::sdf::Sculpt, bust: &Bust, grow: f32) {
    use crate::sdf::Shape::*;
    for side in [-1.0f32, 1.0] {
        let (center, radii) = bust.mass(side, grow);
        let mid = (bust.top + bust.fold) * 0.5;
        // The base lies on the wall, not in front of it: it widens the
        // attachment so the fullness grows out of the chest.
        let depth = 0.022 + 0.3 * bust.radii.z;
        body.add(
            Ellipsoid(
                Vec3::new(side * bust.root, mid, center.z - radii.z * 0.7),
                Vec3::new(bust.footprint, (bust.top - bust.fold) * 0.5, depth) + Vec3::splat(grow),
            ),
            0.05,
        );
        // The upper slope: a tapering ramp from high on the chest wall down
        // into the fullness, its curve set by the contour.
        let start = bust.slope_start * Vec3::new(side, 1.0, 1.0);
        body.add(
            Capsule(
                start,
                center + Vec3::Y * radii.y * 0.25,
                bust.slope + grow,
                radii.x.min(radii.z) * 0.82,
            ),
            0.045,
        )
        .add(Ellipsoid(center, radii), 0.03);
    }
}

/// The two halves of the seat, left then right: centre and radii.
pub(super) fn seat_masses(body: HumanoidBody) -> [(Vec3, Vec3); 2] {
    let r = body.pelvis_radii();
    let depth = if body.woman { 0.58 } else { 0.48 };
    [-1.0f32, 1.0].map(|side| {
        (
            Vec3::new(side * r.x * 0.36, -0.035, -r.z * 0.4),
            r * Vec3::new(0.44, 0.68, depth),
        )
    })
}

/// The pelvis sculpted as one form inside its fitted envelope: a waist
/// tapering up into the torso's own, shelves over the hip joints, a lower
/// belly and a rounded seat, fuller on a woman. Garments that fit the
/// envelope (skirts, belts, trousers) still contain it.
fn pelvic_supports(body: HumanoidBody, offset: Vec3) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let b = body.build;
    let r = body.pelvis_radii();
    let seat = seat_masses(body)[1];
    let mut pelvis = Sculpt::default();
    pelvis
        .add(
            Ellipsoid(
                offset + Vec3::new(0.0, -0.005, 0.0),
                r * Vec3::new(0.86, 0.82, 0.95),
            ),
            0.0,
        )
        // Up into the waist: just inside the torso's lowest support, so the
        // two parts meet without a ridge and the torso's own garment, not
        // the seat's, shows above the belt.
        .add(
            Ellipsoid(
                offset + body.torso + Vec3::new(0.0, 0.075, -0.004),
                Vec3::new(0.15 * body.waist * b, 0.115, 0.108 * b) - Vec3::splat(0.005),
            ),
            0.045,
        )
        // The lower belly, soft and forward of the hip line.
        .add(
            Ellipsoid(
                offset + Vec3::new(0.0, -0.015, r.z * 0.16),
                r * Vec3::new(0.7, 0.56, 0.6),
            ),
            0.045,
        );
    for side in [-1.0f32, 1.0] {
        pelvis
            // The hip shelf over the joint the thigh hangs from, as wide as
            // the thigh's head beneath it up to the waistband.
            .add(
                Ellipsoid(
                    offset + Vec3::new(side * r.x * 0.6, -0.005, 0.0),
                    r * Vec3::new(0.4, 0.8, 0.6),
                ),
                0.05,
            )
            // The seat: rounder and fuller on a woman.
            .add(
                Ellipsoid(offset + seat.0 * Vec3::new(side, 1.0, 1.0), seat.1),
                0.04,
            );
    }
    pelvis
}

/// The lower skin/seat ends beneath the upper garment; both use the same supports.
pub(super) fn pelvis_shape(body: HumanoidBody) -> crate::sdf::Sculpt {
    let mut pelvis = pelvic_supports(body, Vec3::ZERO);
    pelvis.carve(
        crate::sdf::Shape::Beyond(Vec3::Y, BELT_BOTTOM_Y - 0.005),
        0.005,
    );
    pelvis
}

/// Where the sculpted pelvis's surface lies at a height, out along a
/// horizontal direction: the anchor for anything hung round the hips.
pub(super) fn pelvis_surface(pelvis: &crate::sdf::Sculpt, y: f32, out: Vec3) -> Vec3 {
    pelvis.onto(Vec3::new(0.0, y, 0.0) + out * 0.5, -out)
}

/// A supporting garment does not follow the fold beneath a chest: it falls
/// from the fullness to the rib cage, by `share` of the chest's depth.
pub(super) fn bust_underbridge(
    body: &mut crate::sdf::Sculpt,
    bust: &Bust,
    share: f32,
    grow: f32,
    allowance: f32,
) {
    use crate::sdf::Shape::*;
    let low = bust.fold - 0.075;
    for side in [-1.0f32, 1.0] {
        let (center, radii) = bust.mass(side, 0.0);
        // It falls straight down as a broad sheet whose lower edge lies
        // flush on the rib cage, never a ridge or a lump on it.
        let x = center.x * 0.9;
        let spread = radii.x * 0.7;
        let end = Vec3::new(x, low, bust.wall.front(x, low) - spread + 0.002);
        body.add(
            Capsule(
                center,
                end,
                radii.z * share + grow + allowance,
                spread + grow,
            ),
            0.04,
        );
    }
}

/// Body supports remain exact; garment ease is local to ribs and back so
/// the waist stays fitted and the shoulder/neck openings keep their anchors.
pub(super) fn torso_shape(
    fit: HumanoidBody,
    material: Option<TorsoMaterial>,
    trim_neck: bool,
) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    let b = fit.build;
    let (sh, w) = (fit.shoulders * b, fit.waist * b);
    let fem = fit.woman;
    // Structured fronts span the valley between the chest's fullness: by a
    // share of its depth and an allowance past the material's own growth.
    let (grow, ease, bridge) = match material {
        None => (0.0, Vec2::ZERO, None),
        Some(TorsoMaterial::Cloth) => (0.010, Vec2::new(0.006, 0.004), Some((0.55, 0.0))),
        Some(TorsoMaterial::Leather) => (0.014, Vec2::new(0.010, 0.006), Some((0.8, 0.004))),
        Some(TorsoMaterial::Plate) => (0.018, Vec2::new(0.012, 0.008), Some((1.0, 0.008))),
    };
    let g = Vec3::splat(grow);
    let rib_g = if material.is_some() {
        g + Vec3::new(ease.x, 0.0, ease.y)
    } else {
        g
    };
    let mut body = Sculpt::default();
    body.add(
        Ellipsoid(
            Vec3::new(0.0, 0.075, -0.004),
            Vec3::new(0.15 * w, 0.115, 0.108 * b) + g,
        ),
        0.0,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.29, -0.014),
            Vec3::new(0.183 * sh.max(w), 0.175, 0.119 * b) + rib_g,
        ),
        0.055,
    )
    .add(
        Ellipsoid(
            Vec3::new(0.0, 0.385, -0.047 * b),
            Vec3::new(0.18 * sh, 0.125, 0.09 * b) + rib_g,
        ),
        0.045,
    )
    .add(
        // The neck's root stays just inside the neck itself, as slender as
        // it is, so the neck rises out of the shoulders' slope: never out of
        // a wider stub whose flat top would show as a ledge round it.
        Capsule(
            Vec3::new(0.0, 0.505, -0.008),
            Vec3::new(0.0, super::anatomy::NECK_SEAT, -0.008),
            0.059 * b * neck_slim(fem) - 0.002 + grow,
            0.05 * b * neck_slim(fem) - 0.004 + grow,
        ),
        0.035,
    );
    for side in [-1.0f32, 1.0] {
        // The clavicle slopes into the arm's moving deltoid instead
        // of leaving a separate spherical cap on the torso.
        body.add(
            Capsule(
                Vec3::new(side * 0.035 * b, 0.485, -0.005),
                Vec3::new(side * 0.195 * sh, 0.465, -0.005),
                0.038 * b + grow,
                0.034 * b + grow,
            ),
            0.035,
        );
    }
    if let Some(bust) = fit.bust {
        bust_supports(&mut body, &bust, grow);
    }
    if !fem {
        // One broad, shallow pectoral plane joins both shoulders to
        // the rib cage. Paired forward lobes read as breast cups,
        // especially when the fitted leather follows their shape.
        body.add(
            Ellipsoid(
                Vec3::new(0.0, 0.39, -0.006 * b),
                Vec3::new(0.185 * sh, 0.11, 0.095 * b) + g,
            ),
            0.04,
        );
    }
    // Structured fronts span the chest valley while the chest's own supports
    // retain the upper and lateral form. A single replacement ellipsoid
    // would not enclose those offset supports without flattening the shape.
    if let (Some(bust), Some((share, allowance))) = (fit.bust, bridge) {
        let (left, l) = bust.mass(-1.0, 0.0);
        let (right, r) = bust.mass(1.0, 0.0);
        body.add(
            Capsule(
                left,
                right,
                l.z * share + grow + allowance,
                r.z * share + grow + allowance,
            ),
            0.02,
        );
        // Plate is forged as one protective volume across the chest, never
        // a pair of cups: a single shell spans both sides and the valley.
        if matches!(material, Some(TorsoMaterial::Plate)) {
            let span = right.x + r.x * 0.75;
            body.add(
                Ellipsoid(
                    Vec3::new(0.0, (left.y + right.y) * 0.5 + 0.01, right.z),
                    Vec3::new(span, r.y * 1.35, r.z) + Vec3::splat(grow + allowance),
                ),
                0.04,
            );
        }
        bust_underbridge(&mut body, &bust, share, grow, allowance);
    }
    // Carry the same upper-pelvis curve into the lower torso. Independent
    // waist ellipsoids left a visible step, especially on fuller hips.
    let mut lower = pelvic_supports(fit, -fit.torso);
    lower.inflate(grow);
    lower.carve(
        Beyond(Vec3::NEG_Y, -(BELT_BOTTOM_Y - 0.01 - fit.torso.y)),
        0.005,
    );
    body.union(lower, 0.0);
    if trim_neck && material.is_some() {
        // The tunic ends at its fitted opening rather than climbing
        // inside the moving skin neck as a second thick cylinder. Bare
        // skin is not trimmed: the neck rises from it unbroken.
        body.carve(Beyond(Vec3::Y, 0.552), 0.004);
    }
    body
}

/// Upright figures: people, the dead, constructs and the yeti.
///
/// The body is shaped rather than stacked: a chest that narrows to the
/// waist, shoulders, a skirt over the hips, tapered limbs with knees and
/// elbows, calves, hands with thumbs, cuffed boots. Clothing is layered in
/// separate colours so a figure reads at a glance even from above.
fn biped(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let style = l.style;
    let body = kit
        .rig
        .humanoid
        .expect("biped anatomy resolved before generation");
    let fem = body.woman;
    let b = body.build;
    let (shoulders, waist) = (body.shoulders, body.waist);
    let arm_len = body.arm_length;
    // What it wears, slot by slot: its kind's own outfit unless it was
    // dressed in another. A kind's own trimmings (lacing, straps, rags,
    // charms) come only with its own outfit; another outfit brings its own.
    let composition = Composition::resolve(&l);
    let outfit = composition.outfit;
    let heraldry = outfit.trimmings.heraldry.unwrap_or(Heraldry {
        field: l.cloth,
        edge: GILT,
        emblem: CREAM,
        mark: l.v.pattern,
    });
    let native = l.outfit.is_none();
    let fitted_robe = outfit.robe.is_some();
    let boots = match outfit.feet {
        Feet::Boots(boots) => Some(boots),
        Feet::Bare(_) | Feet::Shoes(_) => None,
    };
    let shoes: Option<Shoes> = match outfit.feet {
        Feet::Shoes(shoes) => Some(shoes),
        _ => None,
    };
    // Footwear is fitted to the resolved wearer.
    let boot_shape = BootShape::fitted(
        b,
        if boots.is_some_and(|boots| boots.tall) {
            -0.075
        } else {
            -0.24
        },
    );
    let blocky = style == Style::Guardian;
    let furry = style == Style::Yeti;
    let cuirass = outfit.outer == Some(Outer::Cuirass);
    // The colour of each layer.
    let tunic = outfit.tunic;
    let pants = outfit.pants(l.skin);
    let bare_arms = outfit.arms.bare();
    let barefoot = matches!(outfit.feet, Feet::Bare(_));
    let trousered = matches!(outfit.legs, Legs::Trousers(_));
    let boot = outfit.boot(l.skin);
    let sleeve = outfit.sleeve(l.skin);
    let fore_color = outfit.forearm(l.skin);
    let styled = match outfit.arms {
        Arms::Styled(sleeve) => Some(sleeve),
        _ => None,
    };
    let hand_color = outfit.hands.resolve(l.skin);
    let trim_neck = !native
        || !matches!(
            style,
            Style::Sentinel | Style::Hobgoblin | Style::Zombie | Style::Orc | Style::Troll
        );
    let shell_shape = outfit_shell(body, &outfit, trim_neck);
    let native_plate = cuirass.then(|| {
        let mut plate = torso_garment(body, TorsoMaterial::Plate, trim_neck);
        plate
            .carve(crate::sdf::Shape::Beyond(Vec3::Y, 0.525), 0.004)
            .carve(crate::sdf::Shape::Beyond(Vec3::NEG_Y, -0.012), 0.0);
        if let Some(cut) = NecklineCut::resolve(outfit.bodice.cut, body) {
            plate.carve(cut.opening(body, 0.525, 0.008), 0.006);
        }
        plate
    });
    // Every torso armour shares one fitted shell; its recipe dresses the surface.
    let armor_item = composition.body;
    let equipped_plate = armor_item.map(|_| {
        armor_parts::shell(
            body,
            native_plate.as_ref().or(shell_shape.as_ref()),
            trim_neck,
            if cuirass { EMBLEM_FIELD_OFFSET } else { 0.0 },
        )
    });
    let mut waist_support = torso_shape(body, cuirass.then_some(TorsoMaterial::Plate), trim_neck);
    if let Some(shell) = &shell_shape {
        waist_support.union(shell.clone(), 0.0);
    }
    if let Some(plate) = &equipped_plate {
        waist_support.union(plate.clone(), 0.0);
    }
    let hips = kit.joint(J::Hips, lift, body.hips, "hips", |p| {
        if blocky {
            stone(
                &mut p.matte,
                Vec3::ZERO,
                Vec3::new(0.2, 0.1, 0.14),
                l.skin,
                1,
            );
            return;
        }
        if furry {
            p.matte.blob(
                Vec3::ZERO,
                body.pelvis_radii(),
                3,
                0.05,
                outfit.seat(l.skin),
            );
        } else {
            let seat = outfit.seat(l.skin);
            let high_cut = matches!(outfit.legs, Legs::Bare(_));
            let r = body.pelvis_radii();
            // Over bare legs the garment is cut high: each leg opening
            // rises from the crotch to the hip bone, by its share of the
            // pelvis's half-width.
            let leg_line = |x: f32| -0.075 * body.build + (x - 0.22).max(0.0).powf(1.3) * 0.24;
            pelvis_shape(body).mesh(
                &mut p.matte,
                -r * 1.15 - Vec3::Y * 0.02,
                r * 1.15 + Vec3::Y * 0.1,
                0.0085,
                &|at, _| {
                    let x = at.x.abs() / r.x;
                    if high_cut && x > 0.22 && at.y < leg_line(x) {
                        l.skin
                    } else {
                        seat
                    }
                },
            );
            // Her seat gives as she moves, as her chest does, unless plate
            // holds it.
            if body.woman && outfit.legs != Legs::Plate && style != Style::Zombie {
                let give = if high_cut { 0.7 } else { 0.55 };
                for (center, radii) in seat_masses(body) {
                    p.soft.push(crate::props::SoftRegion {
                        center,
                        radii,
                        facing: -1.0,
                        give,
                    });
                }
            }
            if high_cut {
                // A sewn binding finishes each leg opening, front and back.
                for front in [true, false] {
                    for side in [-1.0f32, 1.0] {
                        let at = |x: f32| (Vec2::new(side * x * r.x, leg_line(x)), 0.01);
                        fitted_ribbon(
                            &mut p.matte,
                            &pelvis_shape(body),
                            &[0.2, 0.35, 0.5, 0.62, 0.74, 0.84, 0.92].map(at),
                            front,
                            0.003,
                            outfit.bodice.edge,
                        );
                    }
                }
            }
        }
        if furry {
            p.matte.blob(
                Vec3::new(0.0, -0.05, 0.0),
                Vec3::new(0.24, 0.16, 0.2),
                6,
                0.25,
                l.cloth,
            );
            return;
        }
        // The tunic's own skirt is a separate drape (see below).
        if native && style == Style::Zombie {
            for k in 0..5 {
                let a = k as f32 * 1.3;
                let at = Vec3::new(a.cos() * 0.2 * body.profile.hips, -0.2, a.sin() * 0.2);
                p.matte.prism(
                    at,
                    at + Vec3::new(a.cos() * 0.03 * body.profile.hips, -0.1, a.sin() * 0.03),
                    0.04,
                    0.0,
                    3,
                    tunic * 0.7,
                    0.0,
                );
            }
        }
        if cuirass {
            // Long tabard panels front and back, gilt-edged, falling past the belt.
            for (z, lean) in [(0.19f32, -0.07f32), (-0.19, 0.07)] {
                let down = Vec3::new(0.0, 1.0, lean).normalize();
                let out = Vec3::Z * z.signum();
                p.metal.extrude(
                    &[
                        Vec2::new(-0.11 * body.profile.hips, 0.0),
                        Vec2::new(0.11 * body.profile.hips, 0.0),
                        Vec2::new(0.125 * body.profile.hips, -0.4),
                        Vec2::new(-0.125 * body.profile.hips, -0.4),
                    ],
                    Vec3::new(0.0, 0.05, z),
                    Vec3::X,
                    down,
                    0.012,
                    heraldry.edge,
                );
                p.matte.extrude(
                    &[
                        Vec2::new(-0.097 * body.profile.hips, 0.0),
                        Vec2::new(0.097 * body.profile.hips, 0.0),
                        Vec2::new(0.11 * body.profile.hips, -0.38),
                        Vec2::new(-0.11 * body.profile.hips, -0.38),
                    ],
                    Vec3::new(0.0, 0.05, z) + out * 0.01,
                    Vec3::X,
                    down,
                    0.012,
                    heraldry.field,
                );
            }
        }
        let belt = super::robe::belt_on(body, BELT_CENTER_Y, &waist_support);
        // Caster cloth and its band already share a dedicated hip-frame rig.
        if outfit.belt && fitted_robe {
            waist_parts::belt(p, body, &waist_support);
        }
        if let Some(worn) = outfit
            .trimmings
            .waist
            .filter(|w| fitted_robe || w.attachment() == J::Hips)
        {
            accessory_parts::waist(p, worn, body, &waist_support);
        }
        if let Some(robe) = outfit.robe {
            // The knot and its surface-bound tails share the garment/belt
            // frame. Fit depth to that belt while retaining the authored knot.
            let x = 0.06;
            let front = belt.y * (1.0 - (x / belt.x).powi(2)).max(0.0).sqrt();
            p.matte.blob(
                Vec3::new(x, 0.11, front + 0.01),
                Vec3::new(0.035, 0.03, 0.02),
                1,
                0.0,
                robe.trim * 0.8,
            );
        }
        if let Some(pouch) = outfit.pouch {
            waist_parts::pouch(&mut p.satin, body, &waist_support, pouch.color);
        }
    });
    // Resolve the lower layer once so the tunic emitter and outer fit share
    // its cut, folds, phase and placement.
    let skirt_surface = |skirt: Skirt, layer: u8| {
        let flare = if cuirass {
            0.185
        } else {
            0.208 + skirt.length * 0.045
        } * skirt.fullness
            * (1.0 - 0.025 * f32::from(layer));
        let length = skirt.length;
        let cut = skirt_cut(body, flare, length, trousered, skirt.cutaway);
        let mut surface = ClothSurface::new(
            cut,
            length,
            if length > 0.5 || skirt.fullness > 1.2 {
                12
            } else {
                8
            },
            skirt.split,
            u64::from(l.v.shade) + u64::from(layer) * 31,
        )
        .with_hem(skirt.hem);
        surface.paired = skirt.center_panel.is_some();
        if surface.paired || skirt.waistline.is_some() {
            let seam = (0..=64)
                .map(|k| {
                    let angle = k as f32 * TAU / 64.0;
                    let height = skirt.waist_height(angle);
                    waist_parts::point(body, &waist_support, height, angle, 0.006)
                })
                .collect();
            surface.seam = Some(std::sync::Arc::new(seam));
            surface = surface.fit_pelvis(body);
        }
        surface
    };
    let mut skirt_stack = Vec::new();
    if let Some(skirt) = outfit.skirt.filter(|_| !blocky && !furry) {
        skirt_stack.push((0u8, skirt, skirt_surface(skirt, 0)));
        for (slot, layer) in skirt.layers.into_iter().enumerate() {
            if let Some(layer) = layer {
                let index = slot as u8 + 1;
                let under = layer.resolve(skirt);
                skirt_stack.push((index, under, skirt_surface(under, index)));
            }
        }
    }
    // Resolve the stack from the innermost garment outward. Each outer
    // layer clears every lower layer, including through another layer's slit.
    for outer in (0..skirt_stack.len()).rev() {
        for inner in outer + 1..skirt_stack.len() {
            let under = skirt_stack[inner].2.clone();
            let garment = skirt_stack[inner].1;
            let raised_trim =
                garment.trim.is_some() || matches!(garment.pattern, Pattern::Brocade { .. });
            skirt_stack[outer].2 =
                skirt_stack[outer]
                    .2
                    .clone()
                    .fit_over(under, Affine3A::IDENTITY, raised_trim);
        }
    }
    let skirt_layer = skirt_stack.first().map(|(_, _, surface)| surface.clone());
    let skirt_scale = storybook_part(Plan::Biped, J::Hips);
    for (index, skirt, surface) in skirt_stack {
        let name = if index == 0 {
            "skirt".to_owned()
        } else {
            format!("skirt{index}")
        };
        let entity = kit.drape_scaled(
            hips,
            Transform::IDENTITY,
            Drape::Skirt(skirt.length),
            &name,
            skirt_scale,
            |p| {
                let b = if skirt.sheer {
                    &mut p.sheer
                } else {
                    &mut p.matte
                };
                skirt_cloth(b, surface, skirt);
            },
        );
        kit.rig.skirt_layers.push(entity);
    }
    // A jerkin's free region: below the belt its leather is no longer
    // supported by the body and hangs as a short flared peplum over the tunic.
    if let Some(Outer::Shell {
        peplum: Some(peplum),
        ..
    }) = outfit.outer
    {
        let (top, offset) = shell_waist(
            shell_shape.as_ref().expect("peplum has a supporting shell"),
            body,
            0.075,
        );
        let bottom = Vec2::new(0.215 * b * body.hips_width, 0.162 * b);
        // The fitted seam and the free hip/hem have different supports.
        // Starting the hip at the narrow waist would make lower-layer fitting
        // enlarge an already sufficient hem by the same excessive factor.
        let cut = SkirtCut {
            waist: top,
            hip: bottom,
            hip_at: 0.5,
            hem: bottom,
            cutaway: peplum.cutaway,
        };
        let mut surface =
            ClothSurface::new(cut, peplum.length, 6, peplum.split, u64::from(l.v.pattern));
        let support = shell_shape.as_ref().expect("peplum has a supporting shell");
        let scale = storybook_part(Plan::Biped, J::Torso);
        surface.seam = Some(std::sync::Arc::new(
            (0..=64)
                .map(|k| {
                    waist_parts::point(body, support, 0.075, k as f32 * TAU / 64.0, 0.003) * scale
                        - offset
                })
                .collect(),
        ));
        if let Some(under) = skirt_layer {
            surface = surface.fit_over(
                under,
                Affine3A::from_scale_rotation_translation(skirt_scale, Quat::IDENTITY, -offset),
                outfit.skirt.is_some_and(|skirt| skirt.trim.is_some()),
            );
        }
        kit.drape(
            hips,
            Transform::from_translation(offset),
            Drape::Skirt(peplum.length),
            "peplum",
            |p| draped_cloth(&mut p.satin, surface, peplum.color, peplum.trim),
        );
    }
    if let Some(robe) = outfit.robe {
        let sashes = [(0.05f32, 0.32f32), (0.085, 0.26)].map(|(dx, end)| super::robe::SashTail {
            path: [
                Vec2::new(dx, 0.10),
                Vec2::new(dx + 0.01, -0.04),
                Vec2::new(dx + 0.015, 0.06 - end),
            ],
            radii: [
                Vec2::new(0.022, 0.006),
                Vec2::new(0.024, 0.006),
                Vec2::new(0.026, 0.006),
            ],
            color: robe.trim * 0.8,
        });
        kit.rig.robe = Some(super::robe::spawn(
            kit.commands,
            kit.library,
            kit.meshes,
            hips,
            robe,
            body,
            boot_shape,
            if (boots.is_some() || shoes.is_some()) && !blocky && !furry {
                BootLast {
                    build: b,
                    heeled: boots.is_some_and(|boots| boots.heel)
                        || shoes.is_some_and(|shoes| shoes.heel),
                }
                .contacts()
            } else {
                body.feet
            },
            &sashes,
        ));
    }
    let torso = kit.joint(J::Torso, hips, body.torso, "torso", |p| {
        match style {
            Style::Guardian => {
                stone(
                    &mut p.matte,
                    Vec3::Y * 0.3,
                    Vec3::new(0.27, 0.27, 0.17),
                    l.skin,
                    2,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.5, 0.0),
                    Vec3::new(0.32, 0.1, 0.19),
                    l.skin * 1.08,
                    3,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.1, 0.0),
                    Vec3::new(0.2, 0.08, 0.13),
                    l.skin * 0.9,
                    4,
                );
                p.accent = Some(Accent::Teal);
                p.glow.torus(
                    Vec3::new(0.0, 0.3, 0.172),
                    Vec3::Z,
                    0.07,
                    0.012,
                    14,
                    3,
                    Vec3::ONE,
                );
                p.glow.cuboid(
                    Vec3::new(0.0, 0.3, 0.172),
                    Vec3::new(0.012, 0.06, 0.004),
                    Quat::IDENTITY,
                    Vec3::ONE,
                );
                return;
            }
            Style::Yeti => {
                p.matte
                    .blob(Vec3::Y * 0.28, Vec3::new(0.3, 0.34, 0.24), 9, 0.08, l.cloth);
                p.matte.blob(
                    Vec3::new(0.0, 0.28, 0.12),
                    Vec3::new(0.18, 0.2, 0.14),
                    4,
                    0.05,
                    l.trim,
                );
                p.matte.blob(
                    Vec3::new(0.0, 0.5, -0.05),
                    Vec3::new(0.26, 0.12, 0.2),
                    7,
                    0.1,
                    l.cloth,
                );
                // A mantle of long fur over the shoulders and down the chest.
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.56, 0.0),
                    0.24,
                    14,
                    Vec3::new(0.0, -1.0, 0.0),
                    l.cloth,
                    31,
                );
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.42, 0.12),
                    0.14,
                    7,
                    Vec3::new(0.0, -1.0, 0.6),
                    l.cloth * 0.96,
                    37,
                );
                return;
            }
            _ => {}
        }
        // The torso is sculpted as one form: waist, ribcage, a chest that
        // broadens into the shoulders, and deltoids that round over the top
        // of the arms, all blended so no seam shows. Outer garments use the
        // same supports with material-specific room and chest construction.
        use crate::sdf::Sculpt;
        let (sh, w) = (shoulders * b, waist * b);
        let neckline = NecklineCut::resolve(outfit.bodice.cut, body);
        let mut body_shape = torso_shape(body, None, trim_neck);
        // The tunic painted on the body carries her chest as it is made to.
        if let (Some(bust), Some(share)) = (body.bust, outfit.bodice.support.share()) {
            bust_underbridge(&mut body_shape, &bust, share, 0.0, 0.0);
        }
        let body_shape = body_shape;
        let bare_shoulders = bare_arms;
        let skin_patch = |at: Vec3, normal: Vec3| {
            if style == Style::Zombie && normal.z > 0.35 {
                let tear =
                    ((at.x - 0.055 * b) / (0.068 * b)).powi(2) + ((at.y - 0.25) / 0.105).powi(2);
                let wound =
                    ((at.x + 0.1 * b) / (0.043 * b)).powi(2) + ((at.y - 0.38) / 0.055).powi(2);
                if tear < 1.0 + 0.12 * (at.y * 93.0).sin() {
                    return Some(l.skin * 0.85);
                }
                if wound < 1.0 {
                    return Some(Vec3::new(0.35, 0.12, 0.1));
                }
            }
            None
        };
        body_shape.mesh_partitioned(
            &mut p.matte,
            Vec3::new(-0.38, -0.06, -0.21) * b.max(1.0),
            Vec3::new(0.38, 0.64, 0.24) * b.max(1.0),
            0.0085,
            &|at, normal| {
                if outfit.bodice.cover == Cover::Bikini {
                    // Each cup's rim is bound in a darker edge.
                    return match bikini_cover(body, at) {
                        Some(rim) if rim > 0.86 => outfit.bodice.edge,
                        Some(_) => tunic * Sculpt::weave(at),
                        None => l.skin,
                    };
                }
                let bare = !bodice_covers(body, outfit.bodice, bare_shoulders, at);
                // Women's bodices plunge down the sternum, showing the upper
                // chest and the valley between.
                if neckline.is_some_and(|cut| cut.open(at)) {
                    return l.skin;
                }
                if let Some(pigment) = skin_patch(at, normal) {
                    return pigment;
                }

                if bare {
                    l.skin
                } else {
                    let ground = match outfit.bodice.pattern {
                        Pattern::Brocade { .. } => tunic,
                        pattern => pattern.paint(tunic, at),
                    };
                    ground * Sculpt::weave(at)
                }
            },
            Some(&|at| {
                if outfit.bodice.cover == Cover::Bikini {
                    bikini_cover(body, at).is_some()
                } else {
                    bodice_covers(body, outfit.bodice, bare_shoulders, at)
                        && !neckline.is_some_and(|cut| cut.open(at))
                }
            }),
        );
        if let Pattern::Brocade { thread, motif } = outfit.bodice.pattern {
            if outfit.bodice.cover != Cover::Bikini {
                embroidery_parts::bodice(
                    &mut p.matte,
                    &body_shape,
                    body.build,
                    motif,
                    thread,
                    &|at, normal| {
                        bodice_covers(body, outfit.bodice, bare_shoulders, at)
                            && !neckline.is_some_and(|cut| cut.open(at))
                            && skin_patch(at, normal).is_none()
                    },
                );
            }
        }
        // The free edges of cropped, strapless and one-shoulder bodices
        // are bound like any other.
        let bound = outfit.bodice.edge;
        if let Cover::Crop(hem) = outfit.bodice.cover {
            let fold = body.bust.map_or(0.27, |bust| bust.fold);
            for front in [true, false] {
                let path: Vec<(Vec2, f32)> = (0..=12)
                    .map(|k| {
                        let x = (k as f32 / 12.0 - 0.5) * 0.36 * sh;
                        let z = if front { 0.1 } else { -0.1 };
                        let reach = hem.reach(x.atan2(z));
                        (
                            Vec2::new(x, fold - 0.03 - 0.04 * (1.0 - reach) + 0.004),
                            0.009,
                        )
                    })
                    .collect();
                fitted_ribbon(&mut p.matte, &body_shape, &path, front, 0.004, bound);
            }
        }
        match outfit.bodice.straps {
            Straps::Strapless | Straps::Sweetheart => {
                let dip = if outfit.bodice.straps == Straps::Sweetheart {
                    0.55
                } else {
                    0.1
                };
                neckline_parts::bind(p, body, &body_shape, dip, bound);
            }
            Straps::OneShoulder => {
                for front in [true, false] {
                    let path: Vec<(Vec2, f32)> = (0..=12)
                        .map(|k| {
                            let x = -0.1 * sh + k as f32 / 12.0 * 0.27 * sh;
                            (Vec2::new(x, one_shoulder_top(body, x) - 0.004), 0.009)
                        })
                        .collect();
                    fitted_ribbon(&mut p.matte, &body_shape, &path, front, 0.004, bound);
                }
            }
            Straps::Panels { width } => panel_parts::bind(p, &body_shape, body, width, bound),
            Straps::Halter | Straps::Shoulders => {}
        }
        if let (Cover::Bikini, Some(bust)) = (outfit.bodice.cover, body.bust) {
            // Halter strings rise from the top of each cup to the neck.
            for side in [-1.0f32, 1.0] {
                let (center, radii) = bust.mass(side, 0.0);
                let cup = Vec2::new(
                    side * (center.x - 0.15 * radii.x),
                    center.y + radii.y * 1.15,
                );
                let neck = Vec2::new(side * 0.055 * b, 0.53);
                fitted_ribbon(
                    &mut p.matte,
                    &body_shape,
                    &[0.0, 0.33, 0.66, 1.0].map(|t| (cup.lerp(neck, t), 0.007)),
                    true,
                    0.003,
                    tunic * 0.8,
                );
                // A bound rim traced round the cup's own outline.
                let rim: Vec<(Vec2, f32)> = (0..=16)
                    .map(|k| {
                        let a = k as f32 / 16.0 * TAU;
                        let at = Vec2::new(
                            center.x + a.cos() * radii.x * 1.12 * 0.95,
                            center.y + 0.12 * radii.y + a.sin() * radii.y * 1.28 * 0.95,
                        );
                        (at, 0.008)
                    })
                    .collect();
                fitted_ribbon(
                    &mut p.matte,
                    &body_shape,
                    &rim,
                    true,
                    0.003,
                    outfit.bodice.edge,
                );
            }
        }
        if let Some(cut) = neckline.filter(|cut| cut.keyhole) {
            let rim: Vec<(Vec2, f32)> = cut
                .keyhole_rim()
                .into_iter()
                .map(|at| (at, 0.009))
                .collect();
            fitted_ribbon(
                &mut p.matte,
                &body_shape,
                &rim,
                true,
                0.004,
                outfit.bodice.edge,
            );
        }
        if let Some(cut) = neckline.filter(|cut| !cut.keyhole) {
            // A sewn edge finishes the neckline on both sides, following
            // the chest's upper slope to the collarbones.
            let edge = outfit.bodice.edge;
            for side in [-1.0f32, 1.0] {
                let at = |t: f32| {
                    let y = cut.bottom - 0.004 + (cut.collar - cut.bottom + 0.004) * t;
                    (Vec2::new(side * cut.edge(y.max(cut.bottom)), y), 0.011)
                };
                let mut path: Vec<(Vec2, f32)> =
                    [1.0, 0.8, 0.6, 0.4, 0.2, 0.0].into_iter().map(at).collect();
                if cut.wrap && side > 0.0 {
                    // Her left panel crosses over beneath the point and
                    // runs down to the tie at the right of her waist.
                    let waist = Vec2::new(-0.11 * sh, 0.1);
                    let point = path[path.len() - 1].0;
                    for t in [0.25f32, 0.5, 0.75, 1.0] {
                        let along = point.lerp(waist, t);
                        // Bowed a little, as the panel wraps the rib cage.
                        let bow = (std::f32::consts::PI * t).sin() * 0.012;
                        path.push((along + Vec2::new(0.0, -bow), 0.011));
                    }
                }
                fitted_ribbon(&mut p.matte, &body_shape, &path, true, 0.004, edge);
            }
        }
        if let (Some(cut), Some(cord)) = (neckline, outfit.bodice.lacing) {
            // Laces cross the lower plunge between eyelets on its edges,
            // drawn straight across the valley rather than into it.
            let rows = 4;
            let top = cut.bottom + (cut.collar - cut.bottom) * 0.55;
            let eyelet = |side: f32, k: usize| {
                let y = cut.bottom + 0.016 + (top - cut.bottom - 0.016) * k as f32 / rows as f32;
                let at = torso_surface(&body_shape, side * (cut.edge(y) + 0.007), y, true);
                at + Vec3::Z * 0.004
            };
            for k in 0..=rows {
                for side in [-1.0f32, 1.0] {
                    p.metal
                        .blob(eyelet(side, k), Vec3::splat(0.0045), 0, 0.0, GILT * 0.8);
                }
            }
            for k in 0..rows {
                for side in [-1.0f32, 1.0] {
                    tube(
                        &mut p.matte,
                        &[eyelet(side, k), eyelet(-side, k + 1)],
                        &[(0.0032, 0.0032), (0.0032, 0.0032)],
                        cord,
                    );
                }
            }
        }
        // Her chest gives as she moves, less under structured leather and
        // not at all under plate; a fuller chest gives more.
        let support = if let Some(item) = armor_item {
            if item.flexible() {
                Support::Structured.give()
            } else {
                0.0
            }
        } else {
            match outfit.outer {
                Some(Outer::Cuirass) => 0.0,
                Some(Outer::Shell { leather: true, .. }) => Support::Structured.give(),
                _ => 1.0,
            }
            .min(outfit.bodice.support.give())
        };
        if let Some(bust) = body.bust.filter(|_| support > 0.0) {
            let give = support * (0.5 + 0.6 * bust.volume);
            for side in [-1.0f32, 1.0] {
                let (center, radii) = bust.mass(side, 0.0);
                p.soft.push(crate::props::SoftRegion {
                    center,
                    radii,
                    facing: 1.0,
                    give,
                });
            }
        }
        if let Some(edge) = outfit
            .armhole_edge
            .filter(|_| bare_arms && outfit.bodice.straps == Straps::Shoulders)
        {
            // A sleeveless bodice ends in sewn edges where the shoulders
            // and arms are bare, front and back.
            for front in [true, false] {
                for side in [-1.0f32, 1.0] {
                    let at = |y: f32| {
                        let x = armhole_edge(body, y, if front { 1.0 } else { 0.0 });
                        (Vec2::new(side * x, y), 0.011)
                    };
                    fitted_ribbon(
                        &mut p.matte,
                        &body_shape,
                        &[at(0.425), at(0.445), at(0.47), at(0.5)],
                        front,
                        0.004,
                        edge,
                    );
                }
            }
        }
        // This is the same resolved surface used by the lower seam.
        let shell = shell_shape.as_ref();
        if let (
            Some(shell),
            Some(Outer::Shell {
                color,
                top,
                quilted,
                underbust,
                ..
            }),
        ) = (shell, outfit.outer)
        {
            let top = shell_top(body, top, underbust);
            let c = color;
            // Padded cloth is matte; leather and fine cloth keep their sheen.
            let builder = if quilted { &mut p.matte } else { &mut p.satin };
            shell.mesh(
                builder,
                Vec3::new(-0.36, -0.03, -0.2) * b.max(1.0),
                Vec3::new(0.36, top + 0.03, 0.22) * b.max(1.0),
                0.0075,
                &move |at, n| {
                    // Seams darken the front opening and the edges.
                    let seam = if at.x.abs() < 0.006 && n.z > 0.5 {
                        0.7
                    } else {
                        1.0
                    };
                    // A gambeson is stitched in diamonds, each cell padded.
                    let quilting = if quilted {
                        let (u, v) = ((at.x + at.y) / 0.045, (at.x - at.y) / 0.045);
                        let (du, dv) = (u - u.round(), v - v.round());
                        if du.abs() < 0.07 || dv.abs() < 0.07 {
                            0.72
                        } else {
                            1.0 - 0.12 * (du.abs() + dv.abs())
                        }
                    } else {
                        1.0
                    };
                    c * seam * quilting * (1.0 + 0.03 * (at.y * 57.0 + at.x * 31.0).sin())
                },
            );
            if underbust {
                // A corset laces up its front between gilt eyelets.
                let rows = 5;
                let eyelet = |side: f32, k: usize| {
                    let y = 0.07 + (top - 0.095) * k as f32 / rows as f32;
                    torso_surface(shell, side * 0.022 * b, y, true) + Vec3::Z * 0.003
                };
                for k in 0..=rows {
                    for side in [-1.0f32, 1.0] {
                        p.metal
                            .blob(eyelet(side, k), Vec3::splat(0.0045), 0, 0.0, GILT * 0.8);
                    }
                }
                for k in 0..rows {
                    for side in [-1.0f32, 1.0] {
                        tube(
                            &mut p.matte,
                            &[eyelet(side, k), eyelet(-side, k + 1)],
                            &[(0.0032, 0.0032), (0.0032, 0.0032)],
                            CREAM * 0.8,
                        );
                    }
                }
            }
        }
        // Where something sits on the front (or back) of the outer garment.
        // Resolve every structural layer once. Rendering and attachments
        // use the same plate shape instead of tracing the naked body under it.
        let plate = &native_plate;
        let garment = plate.as_ref().or(shell).unwrap_or(&body_shape);
        let mut attachment_support = torso_layer_support(&body_shape, garment);
        if let Some(armor) = &equipped_plate {
            attachment_support.union(armor.clone(), 0.0);
        }
        let garment = &attachment_support;
        let accessory_overlay = if plate.is_some() {
            EMBLEM_FIELD_OFFSET
        } else {
            0.0
        };
        if cuirass {
            // The separate gorget belongs to the articulated plate armour.
            p.matte.torus(
                Vec3::Y * 0.555,
                Vec3::Y,
                0.07,
                0.022,
                12,
                4,
                outfit.collar_edge,
            );
        } else if !bare_shoulders
            && !matches!(outfit.bodice.cover, Cover::Bare | Cover::Bikini)
            && matches!(outfit.bodice.straps, Straps::Shoulders | Straps::Halter)
        {
            fitted_band(
                &mut p.matte,
                &body_shape,
                0.535,
                0.552,
                0.0015,
                outfit.collar_edge,
            );
        }
        // A soft cowl or scarf gathered round the neck and draped over the shoulders.
        if let Some(NeckDrape { color, tail, .. }) = outfit.neck_drape {
            p.set_transform(Affine3A::from_translation(Vec3::new(0.0, 0.515, -0.012)));
            draped_cloth(
                &mut p.matte,
                ClothSurface::new(
                    SkirtCut::straight(Vec2::new(0.087, 0.078) * b, Vec2::new(0.15, 0.118) * b),
                    0.045,
                    8,
                    -0.025,
                    u64::from(l.v.pattern),
                ),
                color,
                None,
            );
            p.set_transform(Affine3A::IDENTITY);
            if tail {
                fitted_ribbon(
                    &mut p.matte,
                    garment,
                    &[
                        (Vec2::new(0.07 * b, 0.455), 0.027 * b),
                        (Vec2::new(0.075 * b, 0.36), 0.028 * b),
                        (Vec2::new(0.09 * b, 0.26), 0.022 * b),
                    ],
                    true,
                    0.014,
                    color,
                );
            }
        }

        if let Some(cuirass) = &plate {
            // One fitted cuirass follows the rib cage and waist. Cloth
            // is an open panel laid on its surface, never a second torso
            // intersecting the metal or several circular gold bands.
            cuirass.mesh(
                &mut p.metal,
                Vec3::new(-0.34, -0.025, -0.23) * b.max(1.0),
                Vec3::new(0.34, 0.55, 0.255) * b.max(1.0),
                0.008,
                &|_, normal| STEEL * (0.92 + 0.08 * normal.z.abs()),
            );
            fitted_band(&mut p.metal, cuirass, 0.018, 0.034, 0.004, GILT);
            if let Some(cut) = neckline {
                // A gilt rolled edge finishes the lowered V.
                for side in [-1.0f32, 1.0] {
                    let at = |t: f32| {
                        let y = cut.bottom + (0.515 - cut.bottom) * t;
                        (Vec2::new(side * (cut.edge(y) + 0.007), y), 0.019)
                    };
                    fitted_ribbon(
                        &mut p.metal,
                        cuirass,
                        &[0.02, 0.2, 0.4, 0.6, 0.8, 1.0].map(at),
                        true,
                        0.003,
                        GILT,
                    );
                }
            }
            for front in [true, false] {
                // Below a low-cut front the tabard rises only to a point
                // beneath the plunge.
                let top = match neckline.filter(|_| front) {
                    Some(cut) => cut.bottom - 0.03,
                    None => 0.512,
                };
                let rise = |y: f32| 0.035 + (y - 0.035) * (top - 0.035) / (0.512 - 0.035);
                let edge = [
                    (0.035, 0.112 * w),
                    (0.16, 0.143 * sh),
                    (0.31, 0.166 * sh),
                    (0.435, 0.149 * sh),
                    (0.512, 0.075 * b),
                ];
                let cloth = [
                    (0.048, 0.1 * w),
                    (0.16, 0.13 * sh),
                    (0.31, 0.153 * sh),
                    (0.435, 0.136 * sh),
                    (0.501, 0.064 * b),
                ];
                let edge = edge.map(|(y, x)| (rise(y), x));
                let cloth = cloth.map(|(y, x)| (rise(y), x));
                fitted_panel(&mut p.matte, cuirass, &edge, front, 0.009, heraldry.edge);
                fitted_panel(
                    &mut p.matte,
                    cuirass,
                    &cloth,
                    front,
                    TABARD_SURFACE_OFFSET,
                    heraldry.field,
                );
                fitted_emblem(
                    &mut p.matte,
                    cuirass,
                    Vec2::new(0.0, rise(0.31)),
                    0.075 * b * ((top - 0.035) / 0.477).sqrt(),
                    front,
                    heraldry.emblem,
                    heraldry.field,
                    heraldry.mark,
                );
            }
        }
        if let Some(worn) = outfit.trimmings.neck {
            accessory_parts::neck_collar(
                p,
                worn,
                b,
                if equipped_plate.is_some() {
                    garment
                } else {
                    &body_shape
                },
            );
        }
        if let Some(baldric) = outfit.baldric {
            fitted_baldric(p, garment, body, baldric, accessory_overlay);
        }
        if let (Some(item), Some(armor)) = (armor_item, &equipped_plate) {
            armor_parts::emit(p, armor, body, item);
        }
        if let Some(ornament) = outfit.torso_ornament {
            ornament_parts::emit(p, ornament, body, garment, &body_shape, neckline);
        }
        let fitted_waist = outfit
            .trimmings
            .waist
            .filter(|w| w.attachment() == J::Torso);
        if !fitted_robe && (outfit.belt || fitted_waist.is_some()) {
            // Every fitted waist piece follows the torso surface it encloses.
            // Continuous robe bands retain their dedicated hip/robe frame.
            p.set_transform(reparent_mesh_transform(
                Plan::Biped,
                J::Hips,
                J::Torso,
                body.torso,
            ));
            if outfit.belt {
                waist_parts::belt(p, body, &waist_support);
            }
            if let Some(worn) = fitted_waist {
                accessory_parts::waist(p, worn, body, &waist_support);
            }
            p.set_transform(Affine3A::IDENTITY);
        }
    });
    let head = kit.joint(J::Head, torso, body.head, "head", |p| {
        let neck = if furry { l.cloth } else { l.skin };
        let slim = neck_slim(fem);
        if blocky || furry || cuirass {
            p.matte.prism(
                Vec3::Y * -0.03,
                Vec3::Y * 0.08,
                0.06 * b * slim,
                0.052 * b * slim,
                7,
                neck,
                0.0,
            );
        } else {
            biped_neck(&mut p.matte, b, slim, neck);
            if let Some(worn) = outfit.trimmings.neck {
                accessory_parts::neck_band(p, worn, b, slim);
            }
        }
        match style {
            Style::Guardian => {
                stone(
                    &mut p.matte,
                    Vec3::Y * 0.15,
                    Vec3::new(0.13, 0.12, 0.13),
                    l.skin,
                    5,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.2, 0.115),
                    Vec3::new(0.12, 0.03, 0.03),
                    l.skin * 0.8,
                    6,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.052, 0.11),
                    Vec3::new(0.08, 0.022, 0.03),
                    l.skin * 0.75,
                    7,
                );
                eyes(p, l.eyes, Vec3::new(0.0, 0.15, 0.13), 0.05, 0.022);
                // The chin ends below the smile instead of hiding its middle.
                // Wear chips inward from the face slab's .13 front plane.
                let smile: [Vec2; 25] = std::array::from_fn(|i| {
                    let u = i as f32 / 24.0 * 2.0 - 1.0;
                    Vec2::new(u * 0.052, 0.085 + u * u * 0.01)
                });
                surface_stroke(
                    &mut p.matte,
                    &smile,
                    0.003,
                    &|at| (Vec3::new(at.x, at.y, 0.13), Vec3::Z),
                    DARK * 2.0,
                );
            }
            Style::Yeti => {
                p.matte.blob(
                    Vec3::new(0.0, 0.14, 0.0),
                    Vec3::new(0.15, 0.15, 0.14),
                    6,
                    0.06,
                    l.cloth,
                );
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.24, -0.02),
                    0.12,
                    9,
                    Vec3::new(0.0, -0.4, -1.0),
                    l.cloth,
                    41,
                );
                // One dark ape face joins brow, flat nose and muzzle. The
                // fitted mouth is independent of the surrounding fur shell.
                use crate::sdf::{Sculpt, Shape::Ellipsoid};
                let mut face = Sculpt::default();
                face.add(
                    Ellipsoid(Vec3::new(0.0, 0.12, 0.09), Vec3::new(0.1, 0.1, 0.07)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.175, 0.12), Vec3::new(0.09, 0.025, 0.03)),
                    0.018,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.069, 0.132), Vec3::new(0.061, 0.035, 0.041)),
                    0.018,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.119, 0.16), Vec3::new(0.033, 0.019, 0.021)),
                    0.012,
                );
                face.mesh(
                    &mut p.matte,
                    Vec3::new(-0.115, 0.012, 0.012),
                    Vec3::new(0.115, 0.228, 0.195),
                    0.0055,
                    &|at, _| {
                        let muzzle =
                            (1.0 - (at.x / 0.07).powi(2) - ((at.y - 0.069) / 0.045).powi(2))
                                .clamp(0.0, 1.0);
                        l.skin * (1.0 - muzzle * 0.12)
                    },
                );
                let mouth_height = |x: f32| 0.08 + (x / 0.046).powi(2) * 0.006;
                let mouth: [Vec2; 25] = std::array::from_fn(|i| {
                    let x = (i as f32 / 24.0 * 2.0 - 1.0) * 0.046;
                    Vec2::new(x, mouth_height(x))
                });
                surface_stroke(
                    &mut p.matte,
                    &mouth,
                    0.0032,
                    &|at| torso_surface_frame(&face, at, true),
                    l.skin * 0.42,
                );
                for side in [-1.0f32, 1.0] {
                    let (at, normal) =
                        torso_surface_frame(&face, Vec2::new(side * 0.013, 0.114), true);
                    p.matte.blob(
                        at + normal * 0.0012,
                        Vec3::new(0.006, 0.0035, 0.002),
                        0,
                        0.0,
                        l.skin * 0.42,
                    );
                }
                eyes(p, l.eyes, Vec3::new(0.0, 0.15, 0.15), 0.035, 0.016);
                for side in [-1.0f32, 1.0] {
                    p.matte.prism(
                        Vec3::new(side * 0.1, 0.24, 0.0),
                        Vec3::new(side * 0.18, 0.34, -0.06),
                        0.035,
                        0.0,
                        6,
                        BONE,
                        0.0,
                    );
                    let x = side * 0.035;
                    let (root, normal) =
                        torso_surface_frame(&face, Vec2::new(x, mouth_height(x)), true);
                    let root = root - normal * 0.002;
                    p.matte.prism(
                        root,
                        root + Vec3::new(side * 0.005, -0.04, 0.01),
                        0.012,
                        0.0,
                        4,
                        BONE,
                        0.0,
                    );
                }
            }
            _ => {
                // A slightly large head reads well from above.
                let size = if fem { 1.1 } else { 1.14 };
                p.set_transform(
                    Affine3A::from_translation(Vec3::Y * 0.08)
                        * Affine3A::from_scale(Vec3::splat(size))
                        * Affine3A::from_translation(Vec3::Y * -0.08),
                );
                human_face(p, &l, !matches!(style, Style::Hobgoblin | Style::Troll));
                if shows_hair(&l) {
                    hair(p, &l, !loose_fall(&l));
                }
                if (l.v.beard
                    && matches!(
                        style,
                        Style::Hero | Style::Caster | Style::Scout | Style::Common | Style::Zombie
                    ))
                    || (style == Style::Leprechaun && !fem)
                {
                    beard(p, hair_color(&l));
                }
                // What the outfit puts on the head.
                match outfit.head {
                    Headwear::Helm => {
                        tube(
                            &mut p.metal,
                            &[
                                Vec3::Y * 0.04,
                                Vec3::Y * 0.14,
                                Vec3::Y * 0.26,
                                Vec3::Y * 0.31,
                            ],
                            &[(0.128, 0.132), (0.136, 0.14), (0.13, 0.134), (0.11, 0.112)],
                            STEEL,
                        );
                        p.metal
                            .blob(Vec3::Y * 0.3, Vec3::new(0.11, 0.05, 0.112), 0, 0.0, STEEL);
                        // The visor: a dark slit, a raised ridge down the face, breathing holes.
                        p.matte.cuboid(
                            Vec3::new(0.0, 0.17, 0.128),
                            Vec3::new(0.095, 0.011, 0.02),
                            Quat::IDENTITY,
                            DARK,
                        );
                        p.metal.cuboid(
                            Vec3::new(0.0, 0.12, 0.138),
                            Vec3::new(0.01, 0.06, 0.012),
                            Quat::IDENTITY,
                            STEEL * 1.1,
                        );
                        for side in [-1.0f32, 1.0] {
                            for k in 0..3 {
                                p.matte.blob(
                                    Vec3::new(side * 0.05, 0.085 + k as f32 * 0.02, 0.124),
                                    Vec3::splat(0.006),
                                    0,
                                    0.0,
                                    DARK,
                                );
                            }
                        }
                        p.metal.torus(
                            Vec3::Y * 0.05,
                            Vec3::Y,
                            0.13,
                            0.014,
                            18,
                            4,
                            Vec3::new(0.8, 0.62, 0.3),
                        );
                        if fem {
                            braid(p, Vec3::new(0.0, 0.05, -0.12), hair_color(&l));
                        }
                        // A plume of the house colour sweeping back from the crest.
                        for k in 0..6 {
                            let spread = (k as f32 - 2.5) * 0.012;
                            let base = Vec3::new(spread, 0.33, 0.03);
                            tube(
                                &mut p.matte,
                                &[
                                    base,
                                    base + Vec3::new(0.0, 0.09, -0.04),
                                    base + Vec3::new(spread, 0.1, -0.16),
                                    base + Vec3::new(spread * 2.0, 0.02, -0.26),
                                ],
                                &[
                                    (0.016, 0.022),
                                    (0.018, 0.026),
                                    (0.013, 0.018),
                                    (0.002, 0.002),
                                ],
                                l.cloth * (0.9 + 0.04 * k as f32),
                            );
                        }
                    }
                    Headwear::HoodDown(hood) => {
                        {
                            use crate::sdf::{Sculpt, Shape::*};
                            let mut cowl = Sculpt::default();
                            cowl.add(
                                Ellipsoid(Vec3::new(0.0, 0.0, -0.12), Vec3::new(0.12, 0.07, 0.06)),
                                0.0,
                            );
                            for side in [-1.0f32, 1.0] {
                                cowl.add(
                                    Ellipsoid(
                                        Vec3::new(side * 0.1, -0.02, -0.06),
                                        Vec3::new(0.05, 0.05, 0.07),
                                    ),
                                    0.05,
                                );
                            }
                            // The hollow of the hood, open toward the neck.
                            cowl.carve(
                                Ellipsoid(
                                    Vec3::new(0.0, 0.03, -0.1),
                                    Vec3::new(0.085, 0.045, 0.04),
                                ),
                                0.02,
                            );
                            cowl.mesh(
                                &mut p.matte,
                                Vec3::new(-0.2, -0.1, -0.22),
                                Vec3::new(0.2, 0.1, 0.02),
                                0.007,
                                &|at, _| hood * 0.95 * crate::sdf::Sculpt::weave(at),
                            );
                        }
                    }
                    Headwear::Hood {
                        color: hood,
                        pointed,
                    } => {
                        // A deep hood: the shell rounds over the head, a thick
                        // rolled rim frames the face, and the cloth drapes down
                        // the back of the neck onto the shoulders.
                        // One sculpted piece of cloth: a shell rounding over the
                        // head, the face opening cut cleanly with only a soft
                        // thickness at its edge, the cloth falling behind the
                        // neck onto the shoulders.
                        {
                            use crate::sdf::{Sculpt, Shape::*};
                            let mut h = Sculpt::default();
                            h.add(
                                Ellipsoid(
                                    Vec3::new(0.0, 0.195, -0.035),
                                    Vec3::new(0.138, 0.165, 0.145),
                                ),
                                0.0,
                            )
                            .add(
                                Ellipsoid(Vec3::new(0.0, 0.07, -0.11), Vec3::new(0.12, 0.13, 0.07)),
                                0.05,
                            )
                            .add(
                                Ellipsoid(
                                    Vec3::new(0.0, -0.03, -0.09),
                                    Vec3::new(0.16, 0.05, 0.09),
                                ),
                                0.05,
                            )
                            // The face opening, tilted to follow the face.
                            .carve(
                                Ellipsoid(Vec3::new(0.0, 0.14, 0.13), Vec3::new(0.088, 0.13, 0.11)),
                                0.012,
                            )
                            // The underside around the neck and the front of the shoulders.
                            .carve(
                                Ellipsoid(Vec3::new(0.0, -0.06, 0.1), Vec3::new(0.2, 0.08, 0.14)),
                                0.02,
                            );
                            h.mesh(
                                &mut p.matte,
                                Vec3::new(-0.2, -0.1, -0.22),
                                Vec3::new(0.2, 0.38, 0.14),
                                0.0065,
                                &|_at, n| {
                                    // A slightly darker edge where the cloth turns inward.
                                    (if n.z > 0.6 { hood * 0.88 } else { hood })
                                        * crate::sdf::Sculpt::weave(_at)
                                },
                            );
                        }
                        if fem || style == Style::Wraith {
                            // Locks of hair escape the hood to frame the face.
                            let c = hair_color(&l);
                            for side in [-1.0f32, 1.0] {
                                lock(
                                    &mut p.matte,
                                    &[
                                        Vec3::new(side * 0.075, 0.2, 0.07),
                                        Vec3::new(side * 0.088, 0.13, 0.07),
                                        Vec3::new(side * 0.08, 0.05, 0.06),
                                    ],
                                    0.026,
                                    0.016,
                                    c,
                                );
                            }
                        }
                        // Some hoods end in a long soft point that flops back.
                        if pointed {
                            tube(
                                &mut p.matte,
                                &[
                                    Vec3::new(0.0, 0.3, -0.06),
                                    Vec3::new(0.0, 0.37, -0.14),
                                    Vec3::new(0.0, 0.33, -0.24),
                                    Vec3::new(0.0, 0.22, -0.27),
                                ],
                                &[(0.075, 0.06), (0.05, 0.045), (0.03, 0.028), (0.004, 0.004)],
                                hood * 0.95,
                            );
                        }
                    }
                    Headwear::WingedHelm { metal, feathers } => {
                        headwear_parts::winged_helm(p, metal, feathers);
                        // The crown is tucked under the cap; a braid emerges
                        // below its rear rim, preserving the wearer's hair color.
                        if fem && grows_scalp_hair(&l) {
                            braid(p, headwear_parts::WINGED_HELM_HAIR_ROOT, hair_color(&l));
                        }
                    }
                    Headwear::Crown { metal, gem } => headwear_parts::crown(p, metal, gem),
                    Headwear::Bare => {}
                    Headwear::Tricorn { color, trim } => headwear_parts::tricorn(p, color, trim),
                    Headwear::WitchHat { color, band } => headwear_parts::witch_hat(p, color, band),
                    Headwear::Wreath(color) => headwear_parts::wreath(p, color),
                    Headwear::Circlet { color, gem, broad } => {
                        headwear_parts::circlet(p, color, gem, broad)
                    }
                    Headwear::Veiled { cap, veil } => headwear_parts::veiled(p, cap, veil),
                }
                if let Some((metal, gem)) = outfit.trimmings.earrings {
                    for side in [-1.0, 1.0] {
                        let at = Vec3::new(side * 0.117, 0.085, 0.005);
                        p.metal
                            .prism(at, at - Vec3::Y * 0.035, 0.002, 0.002, 5, metal, 0.0);
                        p.metal.blob(
                            at - Vec3::Y * 0.045,
                            Vec3::new(0.010, 0.017, 0.006),
                            1,
                            0.0,
                            metal,
                        );
                        p.satin.blob(
                            at + Vec3::new(0.0, -0.045, 0.005),
                            Vec3::new(0.007, 0.012, 0.005),
                            1,
                            0.0,
                            gem,
                        );
                    }
                }
                match style {
                    Style::Hero => {}
                    Style::Hobgoblin => {
                        for side in [-1.0f32, 1.0] {
                            p.matte.prism(
                                Vec3::new(side * 0.1, 0.17, 0.0),
                                Vec3::new(side * 0.26, 0.23, -0.05),
                                0.035,
                                0.0,
                                5,
                                l.skin,
                                0.0,
                            );
                            p.matte.prism(
                                Vec3::new(side * 0.03, 0.08, 0.1),
                                Vec3::new(side * 0.035, 0.12, 0.11),
                                0.01,
                                0.0,
                                3,
                                BONE,
                                0.0,
                            );
                        }
                        // A big ruddy nose and a stiff dark mohawk, or, for her, a wild topknot.
                        p.matte.blob(
                            Vec3::new(0.0, 0.132, 0.115),
                            Vec3::new(0.04, 0.031, 0.045),
                            2,
                            0.0,
                            Vec3::new(0.78, 0.36, 0.3),
                        );
                        if fem {
                            let c = hair_color(&l);
                            p.matte.blob(
                                Vec3::new(0.0, 0.28, -0.04),
                                Vec3::new(0.035, 0.03, 0.035),
                                1,
                                0.0,
                                c,
                            );
                            for k in 0..5 {
                                let a = k as f32 * 1.25;
                                lock(
                                    &mut p.matte,
                                    &[
                                        Vec3::new(0.0, 0.29, -0.04),
                                        Vec3::new(a.cos() * 0.05, 0.33, -0.05 + a.sin() * 0.05),
                                        Vec3::new(a.cos() * 0.1, 0.27, -0.06 + a.sin() * 0.1),
                                    ],
                                    0.024,
                                    0.014,
                                    c,
                                );
                            }
                        }
                        for k in 0..(if fem { 0 } else { 6 }) {
                            let z = 0.06 - k as f32 * 0.045;
                            let root = Vec3::new(0.0, 0.25 - (k as f32 - 2.0).powi(2) * 0.004, z);
                            tube(
                                &mut p.matte,
                                &[root, root + Vec3::new(0.0, 0.06, -0.03)],
                                &[(0.012, 0.03), (0.002, 0.004)],
                                DARK * 3.0,
                            );
                        }
                    }
                    Style::Zombie => {
                        p.matte.blob(
                            Vec3::new(-0.06, 0.2, 0.08),
                            Vec3::new(0.025, 0.02, 0.015),
                            2,
                            0.3,
                            Vec3::new(0.35, 0.12, 0.1),
                        );
                    }
                    Style::Vampire => {
                        p.matte.prism(
                            Vec3::new(0.0, 0.25, 0.07),
                            Vec3::new(0.0, 0.18, 0.11),
                            0.03,
                            0.0,
                            4,
                            DARK,
                            0.0,
                        );
                    }
                    _ => rogue::head_details(p, &l),
                }
            }
        }
    });
    for (side, arm, fore, hand) in [
        (1.0f32, J::ArmL, J::ForeL, J::HandL),
        (-1.0, J::ArmR, J::ForeR, J::HandR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let shoulder = body.shoulder(side);
        let metal = outfit.arms == Arms::Plate;
        let upper = kit.joint(arm, torso, shoulder, &format!("arm{tag}"), |p| {
            let m = if metal { &mut p.metal } else { &mut p.matte };
            if blocky {
                stone(m, Vec3::Y * -0.16, Vec3::new(0.085, 0.17, 0.085), sleeve, 8);
                return;
            }
            if furry {
                m.blob(
                    Vec3::new(-side * 0.012, -0.02, 0.0),
                    Vec3::new(0.062, 0.06, 0.062) * b,
                    2,
                    0.0,
                    sleeve,
                );
                tube(
                    m,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -0.1 * arm_len, 0.006),
                        Vec3::Y * -0.2 * arm_len,
                        Vec3::Y * -0.3 * arm_len,
                    ],
                    &[
                        (0.06 * b, 0.06 * b),
                        (0.064 * b, 0.068 * b),
                        (0.058 * b, 0.06 * b),
                        (0.05 * b, 0.05 * b),
                    ],
                    sleeve,
                );
            } else if metal || bare_arms {
                biped_upper_arm(m, side, b, arm_len, body.profile.muscle, true, sleeve);
            } else {
                biped_upper_sleeve(m, side, b, arm_len, body.profile.muscle, sleeve);
            }
            if furry {
                m.blob(Vec3::Y * -0.12, Vec3::new(0.1, 0.16, 0.1), 3, 0.1, sleeve);
                // Shaggy fur spilling off the shoulder.
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.02, 0.0),
                    0.1,
                    8,
                    Vec3::new(side, -1.2, 0.0),
                    l.cloth * 0.95,
                    29 + side as u64,
                );
            }
            if metal {
                biped_pauldron(p, side, body);
                // Rerebrace lames below the pauldron, each tucked under the last.
                plate_lames(&mut p.metal, &rerebrace_lames(body));
            }
            if let Some(fringe) = outfit.trimmings.shoulders {
                fringe_parts::shoulder(
                    p,
                    fringe,
                    Vec3::new(side * 0.015, 0.025, 0.0),
                    Vec2::new(0.105, 0.095) * b,
                    b,
                );
            }
            if let Some(styled) = styled {
                styled_sleeve_top(p, styled, side, b, arm_len);
            }
            if let Some(color) = outfit.trimmings.armlets {
                accessory_parts::armlet(p, color, b, arm_len);
            }
            if let Some(color) = outfit.trimmings.peaks {
                accessory_parts::peak(p, color, side, b);
            }
        });
        let lower = kit.joint(fore, upper, body.elbow(), &format!("fore{tag}"), |p| {
            if blocky {
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.15,
                    Vec3::new(0.075, 0.15, 0.075),
                    l.skin * 0.9,
                    9,
                );
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.34,
                    Vec3::new(0.105, 0.085, 0.105),
                    l.skin,
                    10,
                );
                return;
            }
            if metal {
                // An elbow cop with a fan.
                p.metal.blob(
                    Vec3::new(0.0, 0.0, -0.01),
                    Vec3::new(0.07, 0.065, 0.07),
                    2,
                    0.0,
                    STEEL * 1.05,
                );
                p.metal.blob(
                    Vec3::new(side * 0.055, 0.0, -0.01),
                    Vec3::new(0.012, 0.05, 0.045),
                    2,
                    0.0,
                    STEEL,
                );
            }
            let m = if metal { &mut p.metal } else { &mut p.matte };
            if metal {
                m.blob(Vec3::ZERO, Vec3::splat(0.058 * b), 2, 0.0, sleeve);
                tube(
                    m,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -0.07 * arm_len, 0.004),
                        Vec3::Y * -0.17 * arm_len,
                        Vec3::Y * -0.26 * arm_len,
                    ],
                    &[
                        (0.05 * b, 0.05 * b),
                        (
                            0.06 * b * body.profile.muscle,
                            0.056 * b * body.profile.muscle,
                        ),
                        (
                            0.05 * b * (0.5 + 0.5 * body.profile.muscle),
                            0.046 * b * (0.5 + 0.5 * body.profile.muscle),
                        ),
                        (0.04 * b, 0.036 * b),
                    ],
                    sleeve,
                );
            } else {
                let fore_bare = bare_arms
                    && !styled.is_some_and(|styled| {
                        matches!(styled.cut, SleeveCut::Gauntlet | SleeveCut::Draped)
                    });
                biped_forearm(m, b, arm_len, body.profile.muscle, fore_bare, fore_color);
            }
            let cuff = outfit.cuffs;
            if let Some(styled) = styled.filter(|styled| {
                matches!(
                    styled.cut,
                    SleeveCut::Bell | SleeveCut::Gauntlet | SleeveCut::Draped
                )
            }) {
                // A bell flares from the elbow; a gauntlet's drape is sheer.
                let flare = styled.flare.unwrap_or(styled.color);
                let builder = if styled.cut == SleeveCut::Gauntlet {
                    &mut p.sheer
                } else {
                    &mut p.matte
                };
                biped_cuff(
                    builder,
                    b * 1.15,
                    arm_len,
                    body.profile.muscle,
                    true,
                    flare,
                    flare * 0.8,
                );
            } else if let Some(robe) = outfit.robe {
                biped_cuff(m, b, arm_len, body.profile.muscle, true, sleeve, robe.trim);
            } else if let Some(cuff) = cuff {
                if metal {
                    // Keep the vambrace separate from its elbow cop.
                    m.prism(
                        Vec3::Y * -0.13 * arm_len,
                        Vec3::Y * -0.25 * arm_len,
                        0.056 * b,
                        0.05 * b,
                        8,
                        cuff,
                        0.0,
                    );
                } else {
                    biped_cuff(m, b, arm_len, body.profile.muscle, false, cuff, cuff * 0.86);
                }
            }
            if let Some(metal) = outfit.trimmings.bracelets {
                p.metal.torus(
                    Vec3::Y * (-0.255 * arm_len),
                    Vec3::Y,
                    0.041 * b,
                    0.004 * b,
                    24,
                    5,
                    metal,
                );
            }
            if furry {
                p.matte.blob(
                    Vec3::Y * -0.1,
                    Vec3::new(0.085, 0.15, 0.085),
                    4,
                    0.1,
                    l.cloth,
                );
                hair_tufts(
                    p,
                    Vec3::Y * -0.06,
                    0.085,
                    7,
                    Vec3::new(0.0, -1.0, 0.0),
                    l.cloth * 0.97,
                    43 + side as u64,
                );
            }
        });
        let grip = kit
            .commands
            .spawn((
                Transform::from_translation(body.hand).with_scale(Vec3::splat(1.22)),
                Visibility::default(),
            ))
            .id();
        kit.commands.entity(lower).add_child(grip);
        kit.rig.joints[hand as usize] = Some(grip);
        kit.rig.rest[hand as usize] = body.hand;
        if !blocky {
            kit.attach(grip, Transform::IDENTITY, &format!("hand{tag}"), |p| {
                // Equipment presentation attaches props to HandR. The free
                // left hand can express the role without opening a held grip.
                let clawed = matches!(
                    style,
                    Style::Zombie
                        | Style::Vampire
                        | Style::Troll
                        | Style::Medusa
                        | Style::Phantom
                        | Style::Wraith
                );
                // A woman's free hand is open and relaxed, ready to gesture.
                let open = clawed
                    || body.bust.is_some()
                    || matches!(style, Style::Caster | Style::Leprechaun | Style::Nymph);
                if side > 0.0 && open {
                    open_hand(p, Vec3::Y * (0.01 * arm_len), side, b, hand_color, clawed);
                } else {
                    fist(p, Vec3::Y * (0.01 * arm_len), side, b, hand_color);
                }
                if furry {
                    for k in 0..3 {
                        let x = (k as f32 - 1.0) * 0.03;
                        p.matte.prism(
                            Vec3::new(x, -0.09 * arm_len, 0.04),
                            Vec3::new(x, -0.14 * arm_len, 0.09),
                            0.012,
                            0.0,
                            4,
                            BONE,
                            0.0,
                        );
                    }
                }
            });
        }
    }
    for (side, leg, shin, foot) in [
        (1.0f32, J::LegL, J::ShinL, J::FootL),
        (-1.0, J::LegR, J::ShinR, J::FootR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let thigh = kit.joint(leg, hips, body.leg_root(side), &format!("leg{tag}"), |p| {
            if let Some(color) = outfit.trimmings.garter.filter(|_| side > 0.0 && !blocky) {
                accessory_parts::garter(p, color, b);
            }
            if blocky {
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.22,
                    Vec3::new(0.095, 0.22, 0.095),
                    pants,
                    11,
                );
                return;
            }
            if furry {
                p.matte
                    .blob(Vec3::ZERO, Vec3::splat(0.094 * b), 3, 0.0, pants);
                tube(
                    &mut p.matte,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -0.13, 0.008),
                        Vec3::Y * -0.3,
                        Vec3::Y * -0.44,
                    ],
                    &[
                        (0.094 * b, 0.096 * b),
                        (0.09 * b, 0.096 * b),
                        (0.072 * b, 0.074 * b),
                        (0.06 * b, 0.062 * b),
                    ],
                    pants,
                );
            } else if outfit.legs == Legs::Plate {
                // Cuisses over the mail: overlapping lames down to the knee cop.
                sculpted_limb(&mut p.matte, body.thigh, pants);
                plate_lames(&mut p.metal, &cuisse_lames(body));
            } else if trousered {
                // Trousers ease over the thigh, which stays fullest above the middle.
                trouser_leg(&mut p.matte, body.thigh, b, false, pants);
            } else {
                // Hip to knee: the thigh is fullest above the middle.
                sculpted_limb(&mut p.matte, body.thigh, pants);
            }
            if furry {
                p.matte
                    .blob(Vec3::Y * -0.18, Vec3::new(0.12, 0.2, 0.12), 5, 0.1, pants);
                hair_tufts(
                    p,
                    Vec3::Y * -0.12,
                    0.11,
                    8,
                    Vec3::new(0.0, -1.0, 0.0),
                    pants * 0.97,
                    47 + side as u64,
                );
            }
        });
        let lower_leg = kit.joint(shin, thigh, body.knee(), &format!("shin{tag}"), |p| {
            if blocky {
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.2,
                    Vec3::new(0.088, 0.2, 0.088),
                    pants * 0.92,
                    12,
                );
                return;
            }
            if outfit.legs == Legs::Plate {
                // A knee cop with a wing.
                p.metal.blob(
                    Vec3::new(0.0, 0.0, 0.03),
                    Vec3::new(0.07, 0.07, 0.06),
                    2,
                    0.0,
                    STEEL * 1.05,
                );
                p.metal.blob(
                    Vec3::new(side * 0.05, 0.0, 0.02),
                    Vec3::new(0.015, 0.05, 0.04),
                    2,
                    0.0,
                    STEEL,
                );
            }
            // Knee, calf, boot with a turned-down cuff, foot and toe.
            if furry {
                p.matte
                    .blob(Vec3::ZERO, Vec3::splat(0.07 * b), 4, 0.0, pants);
                tube(
                    &mut p.matte,
                    &[
                        Vec3::ZERO,
                        Vec3::new(0.0, -0.13, -0.012),
                        Vec3::Y * -0.28,
                        Vec3::Y * -0.4,
                    ],
                    &[
                        (0.062 * b, 0.064 * b),
                        (0.07 * b, 0.076 * b),
                        (0.054 * b, 0.056 * b),
                        (0.05 * b, 0.05 * b),
                    ],
                    pants,
                );
            } else if trousered {
                // Cloth gathers below the knee before the boot.
                trouser_leg(&mut p.matte, body.shin, b, true, pants);
            } else {
                // Knee to ankle, the calf full at the back.
                sculpted_limb(&mut p.matte, body.shin, pants);
            }
            let boot_top = boot_shape.top;
            if boots.is_some() {
                if let Some(fringe) = outfit.trimmings.boot_tops {
                    fringe_parts::rim(
                        p,
                        fringe,
                        Vec3::Y * boot_top,
                        Vec2::splat(boot_shape.radius_top + 0.008 * b),
                        b,
                    );
                }
            }
            // Bare legs end in the foot itself, not in a shaft round the
            // ankle, unless boots are pulled on over them.
            let bare_leg = matches!(outfit.legs, Legs::Bare(_)) && boots.is_none();
            if !bare_leg {
                p.satin.prism(
                    Vec3::Y * boot_top,
                    Vec3::Y * boot_shape.bottom,
                    boot_shape.radius_top,
                    boot_shape.radius_bottom,
                    8,
                    boot,
                    0.0,
                );
            }
            if let Some(shoes) = shoes {
                // An ankle strap, and for gladiator sandals straps laced
                // up the shin, each round its own girth.
                let low = boot_shape.bottom + 0.012;
                let bands = 1 + (shoes.straps * 6.0).round() as usize;
                for k in 0..bands {
                    let y = low + 0.05 * k as f32;
                    let girth = 0.045 + 0.022 * ((y - low) / 0.27).clamp(0.0, 1.0);
                    p.satin.torus(
                        Vec3::Y * y,
                        Vec3::Y,
                        girth * b + 0.004,
                        0.006,
                        12,
                        3,
                        shoes.color,
                    );
                }
            }
            if barefoot && !bare_leg {
                // Wrappings bind the ankle and calf of a bare leg.
                for y in [-0.3f32, -0.36] {
                    p.satin
                        .torus(Vec3::Y * y, Vec3::Y, 0.062 * b, 0.01, 12, 3, LEATHER * 0.8);
                }
            } else if boots.is_some_and(|boots| boots.cuffed) {
                let (radius, thickness) = (boot_shape.cuff_radius, boot_shape.cuff_thickness);
                p.satin.torus(
                    Vec3::Y * boot_top,
                    Vec3::Y,
                    radius,
                    thickness,
                    12,
                    3,
                    boot * 1.15,
                );
            }
            if boots.is_some_and(|boots| boots.buckles) {
                for y in [-0.3f32, -0.37] {
                    p.satin
                        .torus(Vec3::Y * y, Vec3::Y, 0.068 * b, 0.009, 12, 3, boot * 0.6);
                    p.metal.cuboid(
                        Vec3::new(side * 0.06 * b, y, 0.02),
                        Vec3::new(0.008, 0.014, 0.014),
                        Quat::IDENTITY,
                        Vec3::new(0.74, 0.5, 0.26),
                    );
                }
            }
        });
        kit.joint(foot, lower_leg, body.ankle(), &format!("foot{tag}"), |p| {
            p.set_transform(Affine3A::from_translation(Vec3::Y * 0.4));
            if blocky {
                // The same stone foot, now articulated at the ankle
                // so its broad sole can stay level during support.
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, -0.43, 0.05),
                    Vec3::new(0.105, 0.045, 0.16),
                    boot,
                    13,
                );
                return;
            }
            if let Some(shoes) = shoes.filter(|_| !furry) {
                p.set_transform(Affine3A::IDENTITY);
                let upper = if shoes.sandal { l.skin } else { shoes.color };
                boot_parts::emit_shoe(
                    p,
                    BootLast {
                        build: b,
                        heeled: shoes.heel,
                    },
                    upper,
                    shoes.color,
                );
            } else if let Some(boots) = boots.filter(|_| !furry) {
                p.set_transform(Affine3A::IDENTITY);
                boot_parts::emit(
                    p,
                    BootLast {
                        build: b,
                        heeled: boots.heel,
                    },
                    boot,
                );
            } else {
                // Bare and furry families retain their anatomical foot recipe.
                for (i, (center, radii)) in body.feet.into_iter().enumerate() {
                    p.satin.blob(
                        center - Vec3::Y * 0.4,
                        radii,
                        (2 - i) as u64,
                        0.0,
                        if i == 2 { boot * 0.55 } else { boot },
                    );
                }
            }
            if furry || style == Style::Troll {
                for k in 0..3 {
                    let x = (k as f32 - 1.0) * 0.035;
                    p.matte.prism(
                        Vec3::new(x, -0.44, 0.17),
                        Vec3::new(x, -0.46, 0.22),
                        0.012,
                        0.0,
                        4,
                        BONE,
                        0.0,
                    );
                }
            }
        });
    }
    rogue::biped_features(kit, &l);
    // Long loose hair hangs from the crown on its own and swings as she
    // moves, cut in the head's space exactly as it would sit on the head.
    if loose_fall(&l) {
        let size = if fem { 1.1 } else { 1.14 };
        let face = Affine3A::from_translation(Vec3::Y * 0.08)
            * Affine3A::from_scale(Vec3::splat(size))
            * Affine3A::from_translation(Vec3::Y * -0.08);
        let proportion = storybook_part(Plan::Biped, J::Head);
        let strands = long_hair_strands(hair_seed(&l, 0));
        let models: Vec<_> = strands
            .iter()
            .enumerate()
            .map(|(index, strand)| {
                kit.part(&format!("hair/{index}"), |p| {
                    p.set_transform(Affine3A::from_translation(-CROWN) * face);
                    hanging_lock(p, &l, strand);
                })
            })
            .collect();
        let mut support = torso_shape(body, None, trim_neck);
        if let (Some(bust), Some(share)) = (body.bust, outfit.bodice.support.share()) {
            bust_underbridge(&mut support, &bust, share, 0.0, 0.0);
        }
        for layer in [&shell_shape, &native_plate, &equipped_plate]
            .into_iter()
            .flatten()
        {
            support.union(layer.clone(), 0.0);
        }
        let hair_frame = Affine3A::from_translation(-CROWN) * face;
        let curves: Vec<super::deformation::ControlCurve> = strands
            .into_iter()
            .map(|strand| {
                let last = strand.path.len() - 1;
                let points = strand
                    .path
                    .into_iter()
                    .enumerate()
                    .map(|(i, at)| {
                        let t = i as f32 / last as f32;
                        (
                            hair_frame.transform_point3(at),
                            (strand.root + (strand.tip - strand.root) * t * t) * size,
                        )
                    })
                    .collect();
                super::deformation::ControlCurve {
                    points,
                    attached: strand.attached,
                    support_side: strand.support_side,
                }
            })
            .collect();
        let contacts = super::visual_contact::posed(&kit.rig.visual_contacts, |joint| {
            Some(Affine3A::from_translation(kit.rig.rest[joint as usize]))
        });
        let hair = super::hair_contact::spawn(
            kit.commands,
            kit.meshes,
            head,
            Transform::from_translation(CROWN * proportion),
            proportion,
            &models,
            support,
            Affine3A::from_translation(body.head),
            &curves,
            contacts,
        );
        kit.rig.drapes.push((hair.entity, Drape::Hair));
        kit.rig.hair = Some(hair);
    }
    garment_details::spawn(kit, outfit.details, body, hips);
    // The garment owns its palette and cut; the wearer supplies its fit.
    if let Some(cloak) = outfit.cloak {
        let mut recipe =
            super::cloak::Recipe::new(body, cloak.exterior, cloak.lining, cloak.ragged);
        recipe.length = cloak.length;
        if let Some(armor) = &equipped_plate {
            recipe.reserve_body_layer(armor);
        }
        if outfit.baldric.is_some() {
            recipe.reserve_back_layer(BALDRIC_SURFACE_OFFSET);
        }
        kit.cloak(torso, recipe);
    }
}

/// Seat sockets on the actual skull surface and return a local frame for
/// each eye. Animals look out to the sides as well as forward; a forward-only
/// eye can disappear behind a lateral brow or show through the opposite cheek.
fn animal_eye_sockets(
    skull: &mut crate::sdf::Sculpt,
    center: Vec3,
    spread: f32,
    size: f32,
    outward: Vec3,
) -> [(Vec3, Vec3); 2] {
    use crate::sdf::Shape::Ellipsoid;
    [-1.0f32, 1.0].map(|side| {
        let normal = Vec3::new(side * outward.x, outward.y, outward.z).normalize();
        let hint = center + Vec3::X * side * spread;
        let at = skull.onto(hint + normal * 0.18, -normal);
        skull.carve(
            Ellipsoid(at + normal * size * 0.18, Vec3::splat(size * 1.02)),
            size * 0.16,
        );
        (at - normal * size * 0.12, normal)
    })
}

/// An ellipsoid expressed in an eye's tangent frame. The builder still owns
/// the part transform, so larger storybook skulls keep correctly oriented eyes.
fn animal_eye_lobe(b: &mut Builder, center: Vec3, normal: Vec3, radii: Vec3, color: Vec3) {
    let right = Vec3::Y.cross(normal).normalize_or(Vec3::X);
    let up = normal.cross(right).normalize();
    let point = |row: usize, column: usize| {
        let latitude = row as f32 * PI / 8.0;
        let longitude = column as f32 * TAU / 12.0;
        let local = Vec3::new(
            latitude.sin() * longitude.cos(),
            latitude.cos(),
            latitude.sin() * longitude.sin(),
        );
        let at = center
            + right * local.x * radii.x
            + up * local.y * radii.y
            + normal * local.z * radii.z;
        let n = (right * local.x / radii.x + up * local.y / radii.y + normal * local.z / radii.z)
            .normalize();
        (at, n)
    };
    for row in 0..8 {
        for column in 0..12 {
            let a = point(row, column);
            let b0 = point(row + 1, column);
            let c = point(row + 1, column + 1);
            let d = point(row, column + 1);
            for (enabled, mut face) in [(row > 0, [a, b0, d]), (row < 7, [b0, c, d])] {
                if enabled {
                    if (face[1].0 - face[0].0)
                        .cross(face[2].0 - face[0].0)
                        .dot(face[0].1 + face[1].1 + face[2].1)
                        < 0.0
                    {
                        face.swap(1, 2);
                    }
                    b.tri_smooth(face.map(|v| v.0), face.map(|v| v.1), [color; 3]);
                }
            }
        }
    }
}

/// Bold, hooded animal eyes inside broad carved sockets. Their coloured iris
/// fills most of the opening, giving expression without exposed white spheres.
fn animal_eyes(
    p: &mut Parts,
    frames: [(Vec3, Vec3); 2],
    size: f32,
    iris: Vec3,
    skin: Vec3,
    accent: Option<Accent>,
) {
    if let Some(accent) = accent {
        p.accent = Some(accent);
    }
    for (at, normal) in frames {
        let right = Vec3::Y.cross(normal).normalize_or(Vec3::X);
        let up = normal.cross(right).normalize();
        animal_eye_lobe(
            &mut p.matte,
            at,
            normal,
            Vec3::new(size * 1.08, size * 0.86, size * 0.65),
            DARK * 1.4,
        );
        let colored = if accent.is_some() {
            &mut p.glow
        } else {
            &mut p.satin
        };
        animal_eye_lobe(
            colored,
            at + normal * size * 0.54,
            normal,
            Vec3::new(size * 0.8, size * 0.73, size * 0.26),
            if accent.is_some() { Vec3::ONE } else { iris },
        );
        if accent.is_none() {
            animal_eye_lobe(
                &mut p.satin,
                at + normal * size * 0.78,
                normal,
                Vec3::new(size * 0.32, size * 0.54, size * 0.12),
                DARK,
            );
            animal_eye_lobe(
                &mut p.satin,
                at + normal * size * 0.87 + (right + up) * size * 0.23,
                normal,
                Vec3::splat(size * 0.13),
                Vec3::splat(1.5),
            );
        }
        tube(
            &mut p.matte,
            &[
                at - right * size * 1.1 + up * size * 0.12,
                at + up * size * 0.8 + normal * size * 0.36,
                at + right * size * 1.1 + up * size * 0.12,
            ],
            &[
                (size * 0.2, size * 0.18),
                (size * 0.26, size * 0.2),
                (size * 0.2, size * 0.18),
            ],
            skin * 0.95,
        );
    }
}

/// Convert authored parent-mesh coordinates into a child joint's mesh frame.
/// The pivot is in unscaled parent-joint coordinates; baked mesh proportions
/// are applied independently of joint offsets. In neutral pose, pivot +
/// child_scale * this_transform * vertex equals parent_scale * vertex.
/// This is a rest-frame conversion, not a surface fit or animation transform.
fn reparent_mesh_transform(plan: Plan, parent: J, terminal: J, pivot: Vec3) -> Affine3A {
    let parent_scale = storybook_part(plan, parent);
    let terminal_scale = storybook_part(plan, terminal);
    Affine3A::from_translation(-pivot / terminal_scale)
        * Affine3A::from_scale(parent_scale / terminal_scale)
}

/// Bone endpoint shared by the bird mesh and its support solver.
pub(super) const BIRD_ANKLE: Vec3 = super::anatomy::BirdBody::LOWER;

fn bird(kit: &mut Kit, lift: Entity, look: &Look) {
    use super::anatomy::BirdBody;
    let l = look.clone();
    kit.rig.ankle_floor = BirdBody::ANKLE_HEIGHT + 0.002;
    let body = kit.joint(
        J::Body,
        lift,
        Vec3::Y * BirdBody::BODY_HEIGHT,
        "body",
        |p| {
            p.matte.blob(
                Vec3::ZERO,
                Vec3::new(0.245, 0.26, 0.355),
                11,
                0.0,
                l.skin * 0.7,
            );
            // Each shingle travels down the ovoid, turning inward below its
            // widest latitude. Lower rows shorten toward the belly rather than
            // all flaring out into one horizontal skirt.
            for row in 0..7 {
                let latitude = 0.12 + row as f32 * 0.31;
                let count = 14 + row * 4;
                for k in 0..count {
                    let n = (row * 50 + k) as u32;
                    let longitude = (k as f32 + 0.5 * (row % 2) as f32 + rand(84, n) * 0.3)
                        / count as f32
                        * TAU;
                    let sweep = 0.5 + 0.08 * rand(81, n);
                    let path: [Vec3; 4] = std::array::from_fn(|i| {
                        let t = i as f32 / 3.0;
                        let a = latitude + sweep * t;
                        let normal = Vec3::new(
                            a.sin() * longitude.cos(),
                            a.cos(),
                            a.sin() * longitude.sin(),
                        );
                        let surface = normal * Vec3::new(0.245, 0.26, 0.355);
                        // Fine tips leave the surface only slightly. This retains
                        // shaggy texture around a connected body silhouette.
                        surface
                            + normal * (0.012 + 0.012 * (PI * t).sin())
                            + Vec3::NEG_Y * (0.024 * t * t)
                    });
                    let width = (0.052 + 0.004 * row as f32) * 0.5;
                    let radii = [
                        Vec2::new(width * 0.4, 0.006),
                        Vec2::new(width, 0.009),
                        Vec2::new(width * 0.62, 0.006),
                        Vec2::splat(0.0015),
                    ];
                    let across = Vec3::new(-longitude.sin(), 0.0, longitude.cos());
                    let shade = 0.82 + 0.05 * row as f32 / 6.0 + 0.26 * rand(82, n);
                    p.matte.tube_framed(
                        &path,
                        &radii,
                        6,
                        &|_, _, t| l.skin * shade * (1.0 - 0.13 * t),
                        Some(across),
                    );
                }
            }
            for k in 0..7 {
                let a = (k as f32 - 3.0) * 0.28;
                feather(
                    p,
                    Vec3::new(a * 0.08, 0.08, -0.3),
                    Vec3::new(a * 0.4, 0.0, -1.0).normalize(),
                    0.24,
                    0.048,
                    l.skin * (0.85 + 0.05 * k as f32),
                );
            }
        },
    );
    let neck = kit.joint(J::Neck, body, Vec3::new(0.0, 0.12, 0.3), "neck", |p| {
        use crate::sdf::{Sculpt, Shape::*};
        // A continuous throat descends into the feathered breast. The
        // tapered S-neck emerges from that mass instead of ending as a pipe.
        let mut throat = Sculpt::default();
        throat.add(
            Ellipsoid(Vec3::new(0.0, -0.025, 0.035), Vec3::new(0.095, 0.09, 0.09)),
            0.0,
        );
        let path = [
            Vec3::ZERO,
            Vec3::new(0.0, 0.16, 0.08),
            Vec3::new(0.0, 0.32, 0.03),
            Vec3::new(0.0, 0.47, 0.08),
            Vec3::new(0.0, 0.5, 0.1),
        ];
        let radii = [0.078, 0.06, 0.048, 0.043, 0.04];
        for i in 0..path.len() - 1 {
            throat.add(Capsule(path[i], path[i + 1], radii[i], radii[i + 1]), 0.02);
        }
        throat.mesh(
            &mut p.matte,
            Vec3::new(-0.13, -0.14, -0.1),
            Vec3::new(0.13, 0.56, 0.19),
            0.007,
            &|_, _| l.cloth,
        );
        // A ruff of feathers where the neck meets the body.
        for k in 0..12 {
            let a = k as f32 * TAU / 12.0;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            feather(
                p,
                out * 0.06 + Vec3::Y * 0.06,
                out,
                0.11,
                0.045,
                l.skin * (0.9 + 0.2 * rand(83, k)),
            );
        }
    });
    kit.joint(J::Head, neck, Vec3::new(0.0, 0.47, 0.08), "head", |p| {
        // Scale the complete authored skull/eye/beak assembly coherently;
        // the neck pivot stays fixed at the occipital attachment.
        p.set_transform(Affine3A::from_scale(Vec3::splat(1.5)));
        use crate::sdf::{Sculpt, Shape::*};
        let mut skull = Sculpt::default();
        skull
            .add(
                Ellipsoid(Vec3::new(0.0, 0.03, 0.02), Vec3::new(0.06, 0.058, 0.075)),
                0.0,
            )
            .add(
                Ellipsoid(Vec3::new(0.0, 0.005, -0.007), Vec3::new(0.047, 0.05, 0.055)),
                0.02,
            );
        let eye_frames = animal_eye_sockets(
            &mut skull,
            Vec3::new(0.0, 0.047, 0.043),
            0.048,
            0.014,
            Vec3::new(0.88, 0.08, 0.48),
        );
        skull.mesh(
            &mut p.matte,
            Vec3::new(-0.085, -0.06, -0.08),
            Vec3::new(0.085, 0.11, 0.115),
            0.0045,
            &|_, _| l.cloth,
        );
        p.matte.prism(
            Vec3::new(0.0, 0.02, 0.07),
            Vec3::new(0.0, 0.0, 0.17),
            0.028,
            0.006,
            6,
            DARK * 2.5,
            0.0,
        );
        p.matte.prism(
            Vec3::new(0.0, 0.005, 0.07),
            Vec3::new(0.0, -0.005, 0.15),
            0.018,
            0.004,
            5,
            DARK * 2.0,
            0.0,
        );
        animal_eyes(
            p,
            eye_frames,
            0.014,
            Vec3::new(0.72, 0.4, 0.08),
            l.cloth,
            None,
        );
        // A shaggy dark crest.
        hair_tufts(
            p,
            Vec3::new(0.0, 0.08, -0.01),
            0.04,
            7,
            Vec3::new(0.0, 0.6, -1.0),
            l.skin * 0.55,
            19,
        );
    });
    for (side, leg, shin, foot) in [
        (1.0f32, J::LegL, J::ShinL, J::FootL),
        (-1.0, J::LegR, J::ShinR, J::FootR),
    ] {
        let tag = if side > 0.0 { "l" } else { "r" };
        let thigh = kit.joint(leg, body, BirdBody::hip(side), &format!("leg{tag}"), |p| {
            p.matte.blob(
                Vec3::new(0.0, -0.065, 0.02),
                Vec3::new(0.105, 0.16, 0.12),
                3,
                0.15,
                l.skin * 0.85,
            );
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, -0.09, 0.02),
                    BirdBody::UPPER * 0.65,
                    BirdBody::UPPER,
                ],
                &[(0.066, 0.075), (0.058, 0.062), (0.046, 0.05)],
                l.trim,
            );
        });
        let lower = kit.joint(shin, thigh, BirdBody::UPPER, &format!("shin{tag}"), |p| {
            p.matte
                .blob(Vec3::ZERO, Vec3::new(0.056, 0.06, 0.06), 2, 0.0, l.trim);
            tube(
                &mut p.matte,
                &[
                    Vec3::Y * 0.025,
                    Vec3::ZERO,
                    BIRD_ANKLE * 0.35,
                    BIRD_ANKLE * 0.75,
                    BIRD_ANKLE,
                ],
                &[
                    (0.044, 0.048),
                    (0.05, 0.052),
                    (0.043, 0.045),
                    (0.034, 0.036),
                    (0.03, 0.033),
                ],
                l.trim * 0.95,
            );
            for k in 0..6 {
                let t = 0.15 + k as f32 * 0.13;
                p.matte.torus(
                    BIRD_ANKLE * t,
                    BIRD_ANKLE.normalize(),
                    0.05 - 0.02 * t,
                    0.0028,
                    8,
                    3,
                    l.trim * 0.8,
                );
            }
        });
        let foot_transform = reparent_mesh_transform(Plan::Bird, shin, foot, BIRD_ANKLE);
        kit.joint(foot, lower, BIRD_ANKLE, &format!("foot{tag}"), |p| {
            // A broad palm and three long curved toes share the articulated
            // ankle frame; the native sole stays independent of shin pitch.
            p.set_transform(foot_transform);
            p.matte.blob(
                BIRD_ANKLE + Vec3::new(0.0, -0.015, 0.025),
                Vec3::new(0.07, 0.025, 0.075),
                2,
                0.0,
                l.trim,
            );
            for k in -1..=1 {
                let a = k as f32 * 0.68;
                let toe = BIRD_ANKLE + Vec3::new(a.sin() * 0.15, -0.032, a.cos() * 0.17);
                let root = BIRD_ANKLE + Vec3::new(k as f32 * 0.035, -0.012, 0.025);
                tube(
                    &mut p.matte,
                    &[root, root.lerp(toe, 0.5) + Vec3::Y * 0.009, toe],
                    &[(0.028, 0.023), (0.024, 0.018), (0.017, 0.014)],
                    l.trim * 0.8,
                );
                p.matte.prism(
                    toe,
                    toe + Vec3::new(a.sin(), -0.2, a.cos()) * 0.04,
                    0.014,
                    0.0,
                    4,
                    DARK * 3.0,
                    0.0,
                );
            }
        });
    }
}

/// Blue-violet charcoal keeps a black coat readable under ordinary light.
/// The dorsal planes carry a little more pigment value; this is not emission.
fn equine_hide(base: Vec3, style: Style, normal: Vec3) -> Vec3 {
    if style == Style::Unicorn {
        let dorsal = (normal.y * 0.5 + 0.5).clamp(0.0, 1.0);
        base.lerp(Vec3::new(0.23, 0.25, 0.34), dorsal * 0.3)
    } else {
        base
    }
}

/// Horses and their kin; with `centaur`, a rider's torso rises from the withers.
fn quadruped(kit: &mut Kit, lift: Entity, look: &Look, centaur: bool) {
    let l = look.clone();
    let style = l.style;
    let quagga = style == Style::Quagga;
    // All four unposed soles meet the floor; movement crouches this linked
    // rest height rather than inheriting unrelated horse-length dimensions.
    let equine = super::anatomy::EquineBody::resolve(&l);
    let body_height = super::anatomy::EquineBody::BODY_HEIGHT;
    kit.rig.ankle_floor = super::anatomy::EquineBody::ANKLE_HEIGHT;
    kit.rig.legs = (super::anatomy::EquineBody::upper(true).length()
        + super::anatomy::EquineBody::lower(true).length())
        / 0.84;
    // Shorter supports take shorter distance steps at the same travel speed.
    kit.rig.stride = kit.rig.stride.max(0.85 / kit.rig.legs);
    kit.rig.girth = 0.32 * equine.breadth;
    // A quagga's coat: chestnut behind, cream and brown stripes over the
    // forequarters, neck and face, a pale belly. Painted, not banded.
    let coat = move |dir: Vec3, at: Vec3| -> Vec3 {
        if !quagga {
            return equine_hide(l.skin, style, dir);
        }
        let dark = Vec3::new(0.36, 0.2, 0.1);
        let cream = l.cloth;
        // A cream chest and belly; stripes band the sides.
        if dir.y < -0.5 || (dir.z > 0.45 && at.z > 0.2) {
            return cream;
        }
        let front = ease((at.z + 0.1) / 0.5);
        // Each quagga's stripes are its own: spacing, slant and phase.
        let pv = f32::from(l.v.pattern);
        let wave = (at.z * (30.0 + 3.0 * pv)
            + at.y * (4.0 + 2.5 * (pv - 1.5))
            + (at.y * 5.0).sin() * 1.2
            + pv * 1.7)
            .sin();
        let striped = cream.lerp(dark, ((wave - 0.15) * 6.0).clamp(0.0, 1.0));
        l.skin.lerp(striped, front)
    };
    let body = kit.joint(
        J::Body,
        lift,
        Vec3::new(0.0, body_height, 0.0),
        "body",
        |p| {
            // One continuous hide over the rib cage, scapulae and haunches. The
            // limbs emerge inside these masses; separate round shoulder patches
            // make every horse, unicorn and centaur look assembled from toys.
            use crate::sdf::{Sculpt, Shape::*};
            let wide = Vec3::new(equine.breadth, 1.0, 1.0);
            let mut barrel = Sculpt::default();
            barrel
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, -0.015, -0.015),
                        Vec3::new(0.215, 0.265, 0.43) * wide,
                    ),
                    0.0,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, -0.055, 0.30),
                        Vec3::new(0.18, 0.245, 0.20) * wide,
                    ),
                    0.075,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, 0.025, -0.36),
                        Vec3::new(0.225, 0.245, 0.215) * wide,
                    ),
                    0.075,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, 0.18, 0.25),
                        Vec3::new(0.105, 0.09, 0.19) * wide,
                    ),
                    0.06,
                );
            for side in [-1.0f32, 1.0] {
                barrel
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.09 * equine.breadth, -0.10, 0.34),
                            Vec3::new(0.11, 0.18, 0.14) * wide,
                        ),
                        0.05,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.11 * equine.breadth, 0.015, 0.29),
                            Vec3::new(0.125, 0.22, 0.17) * wide,
                        ),
                        0.075,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.125 * equine.breadth, 0.015, -0.35),
                            Vec3::new(0.14, 0.24, 0.205) * wide,
                        ),
                        0.075,
                    );
            }
            barrel.mesh(
                &mut p.matte,
                Vec3::new(-0.37, -0.38, -0.65),
                Vec3::new(0.37, 0.34, 0.62),
                0.012,
                &|at, normal| coat(normal, at),
            );
        },
    );
    let neck_at = Vec3::new(0.0, 0.12, 0.45);
    if centaur {
        let torso = kit.joint(J::Torso, body, Vec3::new(0.0, 0.15, 0.42), "torso", |p| {
            // One torso rising out of the withers: waist, ribs, chest.
            let fem = l.v.woman;
            let w = if fem { 0.88 } else { 1.0 };
            // Her chest is fitted to the rider's own rib cage, as a biped's is.
            let bust = super::anatomy::centaur_bust(&l);
            // Her leather wrap: from under the chest to a sweetheart edge
            // over the upper slope, dipping between the two sides.
            let wrap = bust.map(|bust| {
                let apex = bust.apex(1.0);
                let low = bust.fold - 0.05;
                let high = apex.y + bust.radii.y * 0.62;
                let dip = apex.y - 0.005;
                move |x: f32| {
                    let t = (x.abs() / apex.x).min(1.6);
                    let edge = if t < 1.0 {
                        dip + (high - dip) * (1.0 - (1.0 - t).powi(2))
                    } else {
                        high - (t - 1.0) * 0.06
                    };
                    (low, edge)
                }
            });
            if let Some(bust) = bust {
                for side in [-1.0f32, 1.0] {
                    let (center, radii) = bust.mass(side, 0.0);
                    p.soft.push(crate::props::SoftRegion {
                        center,
                        radii,
                        facing: 1.0,
                        give: 0.45 * (0.5 + 0.6 * bust.volume),
                    });
                }
            }
            let body_shape = {
                // Sculpted, as the people's torsos are: the waist rising out
                // of the withers into ribs, chest and rounded shoulders.
                use crate::sdf::{Sculpt, Shape::*};
                let mut body = Sculpt::default();
                body.add(
                    // The lower human root sits inside the withers instead
                    // of hanging out over the horse's breast below the belt.
                    Ellipsoid(Vec3::new(0.0, -0.1, -0.055), Vec3::new(0.15, 0.15, 0.12)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.1, 0.0), Vec3::new(0.15 * w, 0.14, 0.12)),
                    0.06,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.3, 0.005), Vec3::new(0.2 * w, 0.17, 0.13)),
                    0.06,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.44, -0.01), Vec3::new(0.2 * w, 0.1, 0.11)),
                    0.14,
                )
                .add(
                    Capsule(
                        Vec3::new(0.0, 0.5, -0.005),
                        Vec3::new(0.0, 0.6, -0.005),
                        0.065,
                        0.055,
                    ),
                    0.04,
                );
                for side in [-1.0f32, 1.0] {
                    body.add(
                        Ellipsoid(
                            Vec3::new(side * 0.235 * w, 0.47, 0.0),
                            Vec3::new(0.072, 0.068, 0.075),
                        ),
                        0.06,
                    );
                }
                if let Some(bust) = bust {
                    bust_supports(&mut body, &bust, 0.0);
                } else {
                    // The male rider has the same broad continuous chest
                    // plane as the bipeds, with no paired forward cups.
                    body.add(
                        Ellipsoid(Vec3::new(0.0, 0.38, 0.015), Vec3::new(0.185, 0.1, 0.1)),
                        0.04,
                    )
                    .add(
                        Ellipsoid(Vec3::new(0.0, 0.17, 0.075), Vec3::new(0.07, 0.09, 0.04)),
                        0.04,
                    );
                }
                body.mesh(
                    &mut p.matte,
                    Vec3::new(-0.34, -0.3, -0.22),
                    Vec3::new(0.34, 0.64, 0.27),
                    0.0085,
                    &|at, _| {
                        // The wrapped leather follows the sculpted chest and
                        // waist, preserving the figure instead of a rigid tube.
                        let wrapped = wrap.is_some_and(|edges| {
                            let (low, high) = edges(at.x);
                            at.y > low && at.y < if at.z > 0.0 { high } else { 0.41 }
                        });
                        if at.y < 0.035 {
                            // The coat transition is concealed by the belt;
                            // any overlap below it belongs to the equine body.
                            l.skin
                        } else if wrapped {
                            LEATHER
                        } else {
                            l.cloth
                        }
                    },
                );
                body
            };
            if let Some(edges) = wrap {
                // Its edges are turned and stitched, top and bottom.
                for top in [true, false] {
                    let at = |x: f32| {
                        let (low, high) = edges(x);
                        (Vec2::new(x, if top { high } else { low }), 0.012)
                    };
                    fitted_ribbon(
                        &mut p.matte,
                        &body_shape,
                        &[-0.16, -0.12, -0.08, -0.04, 0.0, 0.04, 0.08, 0.12, 0.16].map(at),
                        true,
                        0.004,
                        LEATHER * 0.7,
                    );
                }
            }
            // A close-fitting diagonal baldric follows the chest and back;
            // it never projects as a rectangular plank past the shoulders.
            for front in [false, true] {
                fitted_ribbon(
                    &mut p.satin,
                    &body_shape,
                    &[
                        (Vec2::new(-0.105, 0.1), 0.026),
                        (Vec2::new(-0.055, 0.24), 0.026),
                        (Vec2::new(0.04, 0.37), 0.026),
                        (Vec2::new(0.15, 0.48), 0.025),
                    ],
                    front,
                    0.01,
                    LEATHER * 0.72,
                );
            }
            let (buckle, normal) = torso_surface_frame(&body_shape, Vec2::new(0.04, 0.37), true);
            p.metal.blob(
                buckle + normal * 0.018,
                Vec3::splat(0.018),
                0,
                0.0,
                Vec3::new(0.74, 0.5, 0.26),
            );
            tube(
                &mut p.satin,
                &[Vec3::Y * 0.015, Vec3::Y * 0.035, Vec3::Y * 0.055],
                &[(0.164, 0.15), (0.16, 0.146), (0.157, 0.143)],
                LEATHER * 0.72,
            );
        });
        kit.joint(J::Head, torso, Vec3::Y * 0.6, "head", |p| {
            // The rider's face and hair are a person's; men wear full beards.
            let rider = Look {
                skin: l.cloth,
                ..l.clone()
            };
            biped_neck(&mut p.matte, 1.0, neck_slim(rider.v.woman), l.cloth);
            p.set_transform(
                Affine3A::from_translation(Vec3::Y * 0.08)
                    * Affine3A::from_scale(Vec3::splat(1.1))
                    * Affine3A::from_translation(Vec3::Y * -0.08),
            );
            human_face(p, &rider, true);
            hair(p, &rider, true);
            if !rider.v.woman {
                beard(p, hair_color(&rider));
            }
        });
        for (side, arm, fore, hand) in [
            (1.0f32, J::ArmL, J::ForeL, J::HandL),
            (-1.0, J::ArmR, J::ForeR, J::HandR),
        ] {
            let tag = if side > 0.0 { "l" } else { "r" };
            let reach = if l.v.woman { 0.215 } else { 0.24 };
            let upper = kit.joint(
                arm,
                torso,
                Vec3::new(side * reach, 0.48, 0.0),
                &format!("arm{tag}"),
                |p| {
                    limb(&mut p.matte, 0.3, 0.064, 0.05, 0.068, 0.008, l.cloth);
                },
            );
            let lower = kit.joint(fore, upper, Vec3::Y * -0.3, &format!("fore{tag}"), |p| {
                limb(&mut p.matte, 0.26, 0.05, 0.038, 0.056, 0.004, l.cloth);
                p.satin.prism(
                    Vec3::Y * -0.13,
                    Vec3::Y * -0.25,
                    0.058,
                    0.052,
                    10,
                    LEATHER,
                    0.0,
                );
                fist(p, Vec3::Y * -0.3, side, 1.0, l.cloth);
            });
            let grip = kit
                .commands
                .spawn((
                    Transform::from_translation(Vec3::Y * -0.31),
                    Visibility::default(),
                ))
                .id();
            kit.commands.entity(lower).add_child(grip);
            kit.rig.joints[hand as usize] = Some(grip);
            // Without its rest offset the grip would be posed at the elbow.
            kit.rig.rest[hand as usize] = Vec3::Y * -0.31;
        }
        let hand = kit.rig.joint(J::HandR).unwrap();
        kit.attach(
            hand,
            Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            "spear",
            |p| {
                p.matte
                    .prism(Vec3::Y * -0.7, Vec3::Y * 0.9, 0.02, 0.02, 6, WOOD, 0.0);
                p.satin
                    .prism(Vec3::Y * 0.78, Vec3::Y * 0.9, 0.024, 0.024, 6, LEATHER, 0.0);
                p.metal
                    .prism(Vec3::Y * 0.9, Vec3::Y * 1.14, 0.045, 0.0, 4, STEEL, 0.0);
                p.matte.prism(
                    Vec3::Y * 0.86,
                    Vec3::new(0.05, 0.7, 0.0),
                    0.012,
                    0.0,
                    3,
                    Vec3::new(0.6, 0.15, 0.1),
                    0.0,
                );
            },
        );
    } else {
        let neck = kit.joint(J::Neck, body, neck_at, "neck", |p| {
            let top = Vec3::new(0.0, 0.36, 0.3);
            let neck_coat = |dir: Vec3, at: Vec3| -> Vec3 {
                if quagga {
                    let wave = (at.y * 52.0 + at.z * 12.0).sin();
                    l.cloth.lerp(
                        Vec3::new(0.38, 0.21, 0.11),
                        ((wave - 0.05) * 3.0).clamp(0.0, 1.0),
                    )
                } else {
                    equine_hide(l.skin, style, dir)
                }
            };
            // The crest, throat and root form one angled neck. Its broad
            // root continues into the chest, covering the articulated join.
            use crate::sdf::{Sculpt, Shape::*};
            let mut neck_shape = Sculpt::default();
            neck_shape
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.045, -0.06), Vec3::new(0.125, 0.18, 0.165)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.1, 0.065), Vec3::new(0.112, 0.19, 0.15)),
                    0.055,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.245, 0.19), Vec3::new(0.085, 0.15, 0.12)),
                    0.045,
                )
                .add(Ellipsoid(top, Vec3::new(0.074, 0.095, 0.09)), 0.035);
            neck_shape.mesh(
                &mut p.matte,
                Vec3::new(-0.17, -0.25, -0.25),
                Vec3::new(0.17, 0.49, 0.42),
                0.009,
                &|at, normal| {
                    if quagga && normal.z > 0.7 && at.y < 0.32 {
                        l.cloth
                    } else {
                        neck_coat(normal, at)
                    }
                },
            );
            let mane = if style == Style::Unicorn {
                l.cloth
            } else {
                l.trim
            };
            // The mane: locks along the crest. A unicorn's falls long and
            // sweeps to one side; a quagga's stands up in a short brush.
            let locks = if style == Style::Unicorn { 20 } else { 18 };
            for k in 0..locks {
                let t = k as f32 / (locks - 1) as f32;
                // Along the crest, the top of the arched neck.
                let crest = Vec3::new(0.0, 0.06 + t * 0.36, -0.16 + t * 0.4);
                let root = neck_shape.onto(crest - Vec3::Z * 0.18, Vec3::Z);
                let shade = mane * (0.8 + 0.35 * rand(5, k));
                if style == Style::Unicorn {
                    let side = if k % 3 == 0 { -1.0 } else { 1.0 };
                    let len = 0.26 + 0.12 * rand(6, k);
                    let curl = 0.03 * (k as f32 * 1.3).sin();
                    // Each lock lifts off the crest, then falls close along the neck.
                    let path = [
                        root,
                        root + Vec3::new(side * 0.03, 0.05, -0.05),
                        root + Vec3::new(side * 0.08, 0.0, -0.09),
                        root + Vec3::new(side * 0.1 + curl, -len * 0.55, -0.1 + 0.05 * rand(7, k)),
                        root + Vec3::new(side * 0.09 - curl + 0.02 * rand(8, k), -len, -0.06),
                    ];
                    let r = 0.03 + 0.012 * rand(9, k);
                    tube(
                        &mut p.matte,
                        &path,
                        &[
                            (r, r * 0.7),
                            (r * 1.1, r * 0.8),
                            (r, r * 0.7),
                            (r * 0.6, r * 0.5),
                            (0.003, 0.003),
                        ],
                        shade,
                    );
                } else {
                    let path = [
                        root,
                        root + Vec3::new(0.0, 0.06, -0.035),
                        root + Vec3::new(0.0, 0.1, -0.06),
                    ];
                    tube(
                        &mut p.matte,
                        &path,
                        &[(0.016, 0.03), (0.014, 0.026), (0.002, 0.004)],
                        shade,
                    );
                }
            }
        });
        kit.joint(J::Snout, neck, Vec3::new(0.0, 0.4, 0.31), "head", |p| {
            let face = |normal: Vec3, at: Vec3| -> Vec3 {
                if quagga && (at.z * 30.0 - at.y * 20.0).sin() > 0.35 && at.z < 0.2 {
                    Vec3::new(0.38, 0.21, 0.11)
                } else if quagga {
                    l.cloth
                } else {
                    equine_hide(l.skin, style, normal)
                }
            };
            // A touch larger than life, as the renders draw it.
            p.set_transform(Affine3A::from_scale(Vec3::splat(1.28)));
            use crate::sdf::{Sculpt, Shape::*};
            let mut skull = Sculpt::default();
            skull
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.01, 0.03), Vec3::new(0.093, 0.105, 0.13)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.018, 0.16), Vec3::new(0.079, 0.072, 0.16)),
                    0.03,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.075, 0.16), Vec3::new(0.070, 0.047, 0.15)),
                    0.025,
                );
            for side in [-1.0f32, 1.0] {
                skull
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.052, -0.035, 0.03),
                            Vec3::new(0.039, 0.057, 0.077),
                        ),
                        0.025,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.061, 0.042, 0.057),
                            Vec3::new(0.027, 0.024, 0.043),
                        ),
                        0.016,
                    );
            }
            skull
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.055, 0.29), Vec3::new(0.083, 0.066, 0.095)),
                    0.04,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.105, 0.235), Vec3::new(0.067, 0.038, 0.12)),
                    0.025,
                );
            let eye_frames = animal_eye_sockets(
                &mut skull,
                Vec3::new(0.0, 0.034, 0.076),
                0.077,
                0.018,
                Vec3::new(0.92, 0.08, 0.38),
            );
            skull.mesh(
                &mut p.matte,
                Vec3::new(-0.14, -0.17, -0.12),
                Vec3::new(0.14, 0.15, 0.405),
                0.006,
                &|at, normal| {
                    if at.z > 0.275 && at.y < -0.025 {
                        if quagga {
                            DARK * 3.0
                        } else {
                            equine_hide(l.skin, style, normal) * 0.88
                        }
                    } else {
                        face(normal, at)
                    }
                },
            );
            for side in [-1.0f32, 1.0] {
                p.matte.blob(
                    skull.onto(Vec3::new(side * 0.043, -0.045, 0.46), -Vec3::Z) + Vec3::Z * 0.003,
                    Vec3::new(0.012, 0.016, 0.01),
                    0,
                    0.0,
                    DARK,
                );
                // Leaf-shaped ears, cupped forward, darker inside.
                let base = Vec3::new(side * 0.045, 0.08, -0.01);
                let ear = [
                    base,
                    base + Vec3::new(side * 0.01, 0.07, 0.0),
                    base + Vec3::new(side * 0.02, 0.14, -0.03),
                ];
                tube(
                    &mut p.matte,
                    &ear,
                    &[(0.034, 0.016), (0.03, 0.014), (0.004, 0.004)],
                    l.skin,
                );
                tube(
                    &mut p.matte,
                    &ear.map(|v| v + Vec3::Z * 0.008),
                    &[(0.022, 0.007), (0.019, 0.007), (0.002, 0.002)],
                    l.skin * 0.55,
                );
            }
            // The forelock spills over the brow.
            let lock = if style == Style::Unicorn {
                l.cloth
            } else {
                l.trim
            };
            hair_tufts(
                p,
                Vec3::new(0.0, 0.1, 0.0),
                0.05,
                5,
                Vec3::new(0.0, -0.3, 1.0),
                lock,
                11,
            );
            animal_eyes(
                p,
                eye_frames,
                0.018,
                Vec3::new(0.44, 0.24, 0.09),
                l.skin,
                l.eyes,
            );
            if style == Style::Unicorn {
                p.accent = Some(Accent::Violet);
                let (a, b) = (Vec3::new(0.0, 0.09, 0.07), Vec3::new(0.0, 0.4, 0.28));
                p.metal
                    .prism(a, b, 0.034, 0.0, 10, Vec3::new(0.12, 0.1, 0.16), 0.0);
                for k in 0..4 {
                    let t = 0.12 + k as f32 * 0.2;
                    p.glow.torus(
                        a.lerp(b, t),
                        (b - a).normalize(),
                        0.034 * (1.0 - t) + 0.005,
                        0.005,
                        12,
                        3,
                        Vec3::ONE,
                    );
                }
                hair_tufts(
                    p,
                    Vec3::new(0.0, -0.12, 0.27),
                    0.025,
                    4,
                    Vec3::new(0.0, -1.0, -0.2),
                    l.cloth,
                    61,
                );
            }
        });
    }
    kit.joint(J::Tail, body, Vec3::new(0.0, 0.1, -0.50), "tail", |p| {
        let c = if style == Style::Unicorn {
            l.cloth
        } else {
            l.trim
        };
        tube(
            &mut p.matte,
            &[
                Vec3::ZERO,
                Vec3::new(0.0, -0.06, -0.06),
                Vec3::new(0.0, -0.14, -0.09),
            ],
            &[(0.05, 0.05), (0.04, 0.042), (0.03, 0.03)],
            l.skin,
        );
        // A flowing switch of hair: strands that fan and curl.
        let strands = if style == Style::Unicorn { 12 } else { 9 };
        for k in 0..strands {
            let a = (k as f32 - (strands - 1) as f32 * 0.5) * 0.11;
            let sway = (k as f32 * 1.7).sin() * 0.03;
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, -0.1, -0.08),
                    Vec3::new(a * 0.12, -0.26, -0.15),
                    Vec3::new(a * 0.25 + sway, -0.42, -0.16),
                    Vec3::new(a * 0.32 + sway, -0.56, -0.12),
                ],
                &[(0.034, 0.034), (0.034, 0.03), (0.024, 0.02), (0.002, 0.002)],
                c * (0.8 + 0.4 * rand(13, k)),
            );
        }
    });
    let legs = [
        (J::FL, J::FLs, J::HoofFL, equine.hip(1.0, true), "fl", true),
        (J::FR, J::FRs, J::HoofFR, equine.hip(-1.0, true), "fr", true),
        (
            J::BL,
            J::BLs,
            J::HoofBL,
            equine.hip(1.0, false),
            "bl",
            false,
        ),
        (
            J::BR,
            J::BRs,
            J::HoofBR,
            equine.hip(-1.0, false),
            "br",
            false,
        ),
    ];
    for (thigh, shin, hoof, at, tag, front) in legs {
        let leg_color = if quagga { l.cloth } else { l.skin };
        let upper_at = super::anatomy::EquineBody::upper(front);
        let ankle_at = super::anatomy::EquineBody::lower(front);
        let upper_end = Vec2::new(0.05, 0.058);
        let lower_root = Vec2::new(0.05, 0.056);
        let upper_scale = storybook_part(kit.rig.plan, thigh);
        let lower_scale = storybook_part(kit.rig.plan, shin);
        // A pivot-centered cover contains both scaled end rings as the lower
        // leg turns. Size it from those rings, not the larger muscle above.
        let joint_radius = (upper_end.max_element() * upper_scale.max_element())
            .max(lower_root.max_element() * lower_scale.max_element())
            + 0.0015;
        let joint_radii = Vec3::splat(joint_radius) / upper_scale;
        let root_overlap = lower_root.min_element() * 0.25;
        let upper = kit.joint(thigh, body, at, &format!("leg{tag}"), |p| {
            // A muscled upper leg tapering to a bony knee.
            let (top, mid) = if front { (0.125, 0.092) } else { (0.15, 0.11) };
            let path = [
                Vec3::Y * 0.13,
                upper_at * 0.28 + Vec3::new(0.0, 0.0, if front { 0.005 } else { -0.025 }),
                upper_at * 0.76,
                upper_at,
            ]
            .map(|p| p / upper_scale);
            tube(
                &mut p.matte,
                &path,
                &[
                    (top * 0.6, top * 0.8),
                    (mid * 0.82, mid),
                    (0.056, 0.064),
                    (upper_end.x, upper_end.y),
                ],
                l.skin,
            );
            // Enough rings keep the small cover inside its silhouette margin.
            // The original offset knee/hock landmark remains visible below.
            let resolution = p.matte.res;
            p.matte.res = (18, 32);
            p.matte
                .blob(upper_at / upper_scale, joint_radii, 1, 0.0, l.skin);
            p.matte.res = resolution;
            p.matte.blob(
                upper_at / upper_scale,
                Vec3::new(0.048, 0.054, 0.05),
                1,
                0.0,
                l.skin,
            );
        });
        let lower = kit.joint(shin, upper, upper_at, &format!("shin{tag}"), |p| {
            // Keep the hidden overlap separate: tube paint uses arc length,
            // so inserting it into the cannon would shift the stocking fade.
            p.matte.tube(
                &[Vec3::Y * root_overlap, Vec3::ZERO],
                &[lower_root * 0.85, lower_root],
                12,
                &|_, _, _| l.skin,
            );
            // The stocking fades into the cannon bone instead of ending in
            // a hard sleeve. The fetlock and forward pastern carry the hoof.
            let radii = [
                lower_root,
                Vec2::new(0.035, 0.041),
                Vec2::new(0.043, 0.051),
                Vec2::new(0.038, 0.045),
                Vec2::new(0.045, 0.05),
            ];
            p.matte.tube(
                &[
                    Vec3::ZERO,
                    ankle_at * 0.48 / lower_scale,
                    ankle_at * 0.81 / lower_scale,
                    ankle_at * 0.94 / lower_scale,
                    ankle_at / lower_scale,
                ],
                &radii,
                12,
                &|_, _, t| l.skin.lerp(leg_color, ease((t - 0.08) / 0.28)),
            );
        });
        kit.joint(hoof, lower, ankle_at, &format!("hoof{tag}"), |p| {
            equine_parts::hoof(&mut p.matte, DARK * 2.4);
        });
    }
}

/// A banded snake: a smooth tapering body laid along a travelling wave, a
/// wedge-shaped head with a jaw, and a tongue that flickers.
fn serpent(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let rattler = l.style == Style::Rattlesnake;
    for i in 0..SNAKE {
        let r = snake_segment_radius(i);
        let model = kit.part(&format!("seg{i}"), |p| {
            if i == 0 {
                serpent_skull(p, &l);
            } else if rattler && i >= SNAKE - 3 {
                // The rattle: stacked hollow segments, each a ridged ring.
                let k = (i - (SNAKE - 3)) as f32;
                let tone = Vec3::new(0.86, 0.74, 0.5) * (1.0 - 0.06 * k);
                for ring in 0..3 {
                    let z = (ring as f32 - 1.0) * 0.022;
                    p.satin.blob(
                        Vec3::new(0.0, 0.0, z),
                        Vec3::new(0.034 - k * 0.004, 0.026 - k * 0.003, 0.016),
                        ring,
                        0.0,
                        tone * (0.9 + 0.05 * ring as f32),
                    );
                }
            } else if rattler {
                // Dark diamonds outlined in cream down the olive back.
                let half_length = (r * 1.9).max(0.052);
                p.satin.blob_painted(
                    Vec3::ZERO,
                    Vec3::new(r, r * 0.75, half_length),
                    i as u64,
                    0.0,
                    &|normal, at| {
                        let phase = (i % 2) as f32 * half_length;
                        let diamond = at.x.abs() / (r * 0.85)
                            + ((at.z + phase).rem_euclid(half_length * 2.0) - half_length).abs()
                                / (half_length * 0.9);
                        if normal.y < -0.25 {
                            l.trim
                        } else if normal.y > -0.1 && diamond < 0.75 {
                            Vec3::new(0.24, 0.17, 0.1)
                        } else if normal.y > -0.1 && diamond < 0.95 {
                            l.cloth
                        } else {
                            l.skin
                        }
                    },
                );
            } else {
                // A painted belly and flush dorsal diamonds share the same
                // surface as the banded hide. No floating blocks on the spine.
                let band = (i / 2) % 2 == 1;
                let c = if band { l.cloth } else { l.skin };
                let half_length = (r * 1.9).max(0.052);
                p.satin.blob_painted(
                    Vec3::ZERO,
                    Vec3::new(r, r * 0.75, half_length),
                    i as u64,
                    0.0,
                    &|normal, at| {
                        let diamond = at.x.abs() / (r * 0.7) + at.z.abs() / (half_length * 0.6);
                        if normal.y < -0.25 {
                            l.trim
                        } else if i % 2 == 0 && normal.y > 0.35 && diamond < 1.0 {
                            l.skin * 0.68
                        } else {
                            c
                        }
                    },
                );
            }
        });
        let e = kit
            .commands
            .spawn((Transform::default(), Visibility::default()))
            .id();
        kit.commands.entity(e).with_children(|p| model.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
    // The tongue rides on the head and flicks out now and then.
    let head = kit.rig.segments[0];
    let dimensions = super::anatomy::SerpentHead::resolve(l.style);
    let jaw = kit.joint(J::Jaw, head, dimensions.hinge(), "jaw", |p| {
        serpent_jaw(p, &l);
    });
    kit.joint(J::Fangs, head, dimensions.fangs(), "fangs", |p| {
        p.set_transform(Affine3A::from_scale(dimensions.scale));
        beast_parts::FangArc {
            frame: Transform::from_xyz(0.0, 0.0, -1.3f32.cos() * 0.11)
                .with_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            half_width: 0.068,
            rise: 0.11,
            depth: 0.0,
            growth: Vec3::Z,
            length: if l.style == Style::Rattlesnake {
                0.08
            } else {
                0.058
            },
            radius: 0.008,
            count: 2,
        }
        .emit(&mut p.matte, BONE);
    });
    let tongue = kit.part("tongue", |p| {
        let red = Vec3::new(0.7, 0.12, 0.12);
        p.matte.prism(
            Vec3::ZERO,
            Vec3::new(0.0, 0.0, 0.08),
            0.006,
            0.005,
            4,
            red,
            0.0,
        );
        for side in [-1.0f32, 1.0] {
            p.matte.prism(
                Vec3::new(0.0, 0.0, 0.08),
                Vec3::new(side * 0.015, 0.0, 0.11),
                0.004,
                0.0,
                3,
                red,
                0.0,
            );
        }
    });
    let e = kit
        .commands
        .spawn((
            Transform::from_translation(dimensions.tongue()),
            Visibility::default(),
        ))
        .id();
    kit.commands.entity(e).with_children(|p| tongue.attach(p));
    kit.commands.entity(jaw).add_child(e);
    kit.rig.joints[J::Snout as usize] = Some(e);
    kit.rig.rest[J::Snout as usize] = dimensions.tongue();
}

/// Both snakes share a broad sculpted skull, seated eyes and a separate jaw.
/// The pit viper adds broad rear cheek masses; ornamental size cannot stand in
/// for the head itself. Teeth reuse the same mouth-frame rows as other beasts.
fn serpent_skull(p: &mut Parts, l: &Look) {
    use crate::sdf::{Sculpt, Shape::*};
    let dimensions = super::anatomy::SerpentHead::resolve(l.style);
    p.set_transform(Affine3A::from_scale(dimensions.scale));
    let mut skull = Sculpt::default();
    skull
        .add(
            Ellipsoid(Vec3::new(0.0, 0.02, 0.005), Vec3::new(0.095, 0.07, 0.115)),
            0.0,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.005, 0.13), Vec3::new(0.074, 0.043, 0.09)),
            0.025,
        );
    for side in [-1.0f32, 1.0] {
        if l.style == Style::Rattlesnake {
            skull.add(
                Ellipsoid(
                    Vec3::new(side * 0.055, 0.002, -0.025),
                    Vec3::new(0.05, 0.052, 0.075),
                ),
                0.015,
            );
        }
        skull.add(
            Ellipsoid(
                Vec3::new(side * 0.06, 0.052, 0.075),
                Vec3::new(0.03, 0.025, 0.045),
            ),
            0.018,
        );
    }
    skull.carve(Beyond(Vec3::NEG_Y, 0.018), 0.0);
    let eyes = animal_eye_sockets(
        &mut skull,
        Vec3::new(0.0, 0.036, 0.09),
        0.078,
        0.022,
        Vec3::new(0.84, 0.10, 0.54),
    );
    skull.mesh(
        &mut p.satin,
        Vec3::new(-0.13, -0.022, -0.13),
        Vec3::new(0.13, 0.125, 0.23),
        0.005,
        &|at, normal| {
            if normal.y < -0.35 && at.z > 0.025 {
                Vec3::new(0.42, 0.085, 0.11)
            } else {
                l.skin
            }
        },
    );
    animal_eyes(p, eyes, 0.022, Vec3::new(0.86, 0.57, 0.1), l.skin, None);
    for side in [-1.0f32, 1.0] {
        p.matte.blob(
            Vec3::new(side * 0.032, 0.014, 0.208),
            Vec3::new(0.005, 0.0035, 0.003),
            0,
            0.0,
            DARK,
        );
    }
    let frame =
        Transform::from_xyz(0.0, -0.016, 0.055).with_rotation(Quat::from_rotation_x(FRAC_PI_2));
    beast_parts::FangArc {
        frame,
        half_width: 0.068,
        rise: 0.11,
        depth: 0.0,
        growth: Vec3::Z,
        length: 0.027,
        radius: 0.004,
        count: 7,
    }
    .emit(&mut p.matte, BONE);
}

fn serpent_jaw(p: &mut Parts, l: &Look) {
    use crate::sdf::{Sculpt, Shape::*};
    let dimensions = super::anatomy::SerpentHead::resolve(l.style);
    p.set_transform(
        Affine3A::from_translation(-dimensions.hinge()) * Affine3A::from_scale(dimensions.scale),
    );
    let mut jaw = Sculpt::default();
    jaw.add(
        Ellipsoid(
            Vec3::new(0.0, -0.038, 0.085),
            Vec3::new(0.074, 0.023, 0.132),
        ),
        0.0,
    );
    jaw.mesh(
        &mut p.satin,
        Vec3::new(-0.083, -0.066, -0.055),
        Vec3::new(0.083, -0.01, 0.225),
        0.005,
        &|_, normal| {
            if normal.y > 0.25 {
                Vec3::new(0.48, 0.10, 0.14)
            } else {
                l.trim
            }
        },
    );
}

/// Segments in a snake's body.
pub(super) const SNAKE: usize = 20;

/// Horizontal radius shared by the tapered mesh and its floor placement.
/// Body sections are three quarters as high as they are wide; the separately
/// sculpted head has its own low jaw profile.
pub(super) fn snake_segment_radius(index: usize) -> f32 {
    if index == 0 {
        0.08
    } else {
        let fraction = index as f32 / SNAKE as f32;
        0.093 * (1.0 - fraction).powf(0.72) + 0.005
    }
}

/// A crystal has broad shaded faces and a sharply terminating cap. Emission
/// belongs at the tip; a completely glowing cone loses all of its volume.
fn ice_shard(p: &mut Parts, root: Vec3, tip: Vec3, width: f32, color: Vec3) {
    let shoulder = root.lerp(tip, 0.68);
    let cap = root.lerp(tip, 0.9);
    p.satin
        .prism(root, shoulder, width * 0.55, width, 6, color, 0.0);
    p.satin
        .prism(shoulder, cap, width, width * 0.3, 6, color * 1.15, 0.0);
    p.glow.prism(cap, tip, width * 0.3, 0.0, 6, Vec3::ONE, 0.0);
}

/// Things that float: a burning wisp, a drifting knot of ice.
fn floater(kit: &mut Kit, lift: Entity, look: &Look) {
    let l = look.clone();
    let aura = l.aura.unwrap_or(Accent::Ember);
    let core = kit.joint(J::Core, lift, Vec3::Y * 1.1, "core", |p| {
        p.accent = Some(aura);
        if l.style == Style::Ice {
            // Dark blue recesses and pale cyan planes keep the cluster
            // readable in daylight as well as against the dark dungeon.
            p.satin.smooth = false;
            p.satin.sides_mul = 1;
            p.satin.res = (5, 8);
            p.glow.smooth = false;
            p.glow.sides_mul = 1;
            p.satin.blob(
                Vec3::ZERO,
                Vec3::new(0.17, 0.205, 0.15),
                4,
                0.13,
                Vec3::new(0.12, 0.27, 0.4),
            );
            for k in 0..9 {
                let a = k as f32 * TAU / 9.0;
                let dir = Vec3::new(a.cos(), (k as f32 * 1.3).sin() * 0.8, a.sin()).normalize();
                let color = Vec3::new(0.16, 0.43, 0.61).lerp(l.skin, 0.15 + 0.5 * rand(17, k));
                ice_shard(
                    p,
                    dir * 0.06,
                    dir * (0.3 + rand(k as u64, 3) * 0.15),
                    0.055 + 0.025 * rand(19, k),
                    color,
                );
            }
            eyes(
                p,
                Some(Accent::White),
                Vec3::new(0.0, 0.04, 0.15),
                0.06,
                0.02,
            );
            p.accent = Some(aura);
        } else {
            // Shaded outer petals frame the luminous heart and curling tongues.
            for k in 0..6 {
                let a = k as f32 * TAU / 6.0 + 0.3;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                tube(
                    &mut p.matte,
                    &[
                        out * 0.05 - Vec3::Y * 0.09,
                        out * 0.19 + Vec3::Y * 0.02,
                        out * 0.12 + Vec3::Y * 0.17,
                    ],
                    &[(0.05, 0.02), (0.06, 0.025), (0.004, 0.004)],
                    Vec3::new(0.85, 0.22, 0.035),
                );
            }
            for (k, h) in [(0, 0.52f32), (1, 0.36), (2, 0.4)] {
                let a = k as f32 * 2.2;
                let out = Vec3::new(a.cos(), 0.0, a.sin()) * 0.04;
                let curl = Vec3::new(-a.sin(), 0.0, a.cos()) * 0.06;
                tube(
                    &mut p.glow,
                    &[
                        out,
                        out * 1.4 + Vec3::Y * h * 0.4 + curl * 0.5,
                        out + Vec3::Y * h * 0.75 - curl * 0.3,
                        Vec3::Y * h + curl,
                    ],
                    &[(0.075, 0.05), (0.06, 0.04), (0.03, 0.02), (0.003, 0.003)],
                    Vec3::ONE,
                );
            }
        }
    });
    if l.style == Style::Wisp {
        // A brighter golden heart inside the flames.
        kit.attach(core, Transform::IDENTITY, "heart", |p| {
            p.accent = Some(Accent::Amber);
            p.glow
                .blob(Vec3::Y * 0.02, Vec3::splat(0.085), 1, 0.0, Vec3::ONE);
        });
    }
    if l.style == Style::Ice {
        for (side, arm) in [(1.0f32, J::ArmL), (-1.0, J::ArmR)] {
            kit.joint(
                arm,
                core,
                Vec3::new(side * 0.42, -0.1, 0.0),
                &format!("shard{side}"),
                |p| {
                    p.accent = Some(aura);
                    p.satin.smooth = false;
                    p.satin.sides_mul = 1;
                    p.glow.smooth = false;
                    p.glow.sides_mul = 1;
                    for k in 0..4 {
                        let dir = Vec3::new(side * 0.3, -1.0, 0.2 * (k as f32 - 1.5)).normalize();
                        ice_shard(
                            p,
                            Vec3::new(0.0, 0.0, (k as f32 - 1.5) * 0.022),
                            dir * (0.32 + k as f32 * 0.04),
                            0.045 + k as f32 * 0.004,
                            Vec3::new(0.22, 0.5, 0.69).lerp(l.skin, k as f32 * 0.18),
                        );
                    }
                },
            );
        }
    }
    let mote = kit.part("mote", |p| {
        p.accent = Some(aura);
        p.glow
            .blob(Vec3::ZERO, Vec3::splat(0.035), 0, 0.0, Vec3::ONE);
    });
    for _ in 0..6 {
        let e = kit
            .commands
            .spawn((Transform::default(), Visibility::default()))
            .id();
        kit.commands.entity(e).with_children(|p| mote.attach(p));
        kit.commands.entity(lift).add_child(e);
        kit.rig.segments.push(e);
    }
}

#[cfg(test)]
mod strand_support_tests {
    use super::*;
    #[test]
    fn authored_hanging_locks_retain_their_intended_support_side() {
        use super::super::deformation::SupportSide;
        for seed in [0, 7, 81] {
            let strands = long_hair_strands(seed);
            assert!(strands
                .iter()
                .any(|s| s.support_side == Some(SupportSide::Front)));
            assert!(strands
                .iter()
                .any(|s| s.support_side == Some(SupportSide::Back)));
            for strand in strands {
                assert_eq!(strand.path.len(), strand.attached.len());
                assert!(strand.attached[0], "the authored root must remain attached");
                for (point, attached) in strand.path.iter().zip(&strand.attached) {
                    if point.y < 0.08 {
                        assert!(!attached, "jaw/neck fall was attached as scalp");
                    }
                    if point.y > 0.18 {
                        assert!(*attached, "crown lost attachment ownership");
                    }
                }
                let tip = strand.path.last().unwrap();
                match strand.support_side {
                    Some(SupportSide::Front) => assert!(tip.z > 0.0 && tip.y < 0.0),
                    Some(SupportSide::Back) => assert!(tip.z < 0.0 && tip.y < 0.0),
                    None => assert!(
                        strand.path.iter().all(|p| p.y >= 0.1),
                        "an unclassified hanging lock lost fitting intent"
                    ),
                }
            }
        }
    }
}
