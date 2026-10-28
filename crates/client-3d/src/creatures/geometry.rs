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
mod face;
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
pub(in crate::creatures) use face::*;
mod hair;
pub(in crate::creatures) use hair::*;
mod cloth;
pub(in crate::creatures) use cloth::*;
mod simple_shapes;
use simple_shapes::*;
mod fitting;
pub(in crate::creatures) use fitting::*;
mod torso;
pub(in crate::creatures) use torso::*;
mod biped;
use biped::*;
mod bird;
pub(in crate::creatures) use bird::*;
mod quadruped;
use quadruped::*;
mod serpent;
pub(in crate::creatures) use serpent::*;
mod floater;
use floater::*;

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
                        biped_pauldron(&mut parts, side, body, PauldronFinish::STEEL);
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
