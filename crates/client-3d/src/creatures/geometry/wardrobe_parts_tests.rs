//! The garment parts outfits are composed from: hems, patterns, how a
//! bodice is held up and how far down it reaches, keyholes and sleeves.
use super::super::outfit::{Hem, Pattern, Sleeve, Waist};
use super::*;

fn her() -> HumanoidBody {
    HumanoidBody::resolve(&Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    ))
}

#[test]
fn hems_cut_in_points_or_rise_in_front() {
    assert_eq!(Hem::Even.reach(1.0), 1.0);
    let points = Hem::Points {
        count: 6,
        depth: 0.4,
    };
    let reaches: Vec<f32> = (0..60)
        .map(|k| points.reach(k as f32 / 60.0 * TAU))
        .collect();
    let (low, high) = reaches
        .iter()
        .fold((1.0f32, 0.0f32), |(l, h), &r| (l.min(r), h.max(r)));
    assert!(
        (high - 1.0).abs() < 0.05 && (low - 0.6).abs() < 0.05,
        "{low}..{high}"
    );
    let high_low = Hem::HighLow(0.5);
    assert!((high_low.reach(0.0) - 0.5).abs() < 1e-5, "short in front");
    assert!((high_low.reach(PI) - 1.0).abs() < 1e-5, "long behind");
}

#[test]
fn patterns_leave_plain_cloth_alone_and_print_the_rest() {
    let base = Vec3::new(0.8, 0.6, 0.3);
    let at = Vec3::new(0.03, 0.2, 0.1);
    assert_eq!(Pattern::Plain.paint(base, at), base);
    let spot = Vec3::new(0.2, 0.1, 0.05);
    let printed = (0..400)
        .filter(|k| {
            let at = Vec3::new(*k as f32 * 0.0031, 0.2 + *k as f32 * 0.0017, 0.1);
            Pattern::Leopard(spot).paint(base, at) == spot
        })
        .count();
    assert!(
        (20..240).contains(&printed),
        "rosettes cover part of the hide: {printed}"
    );
}

#[test]
fn a_bodice_covers_only_what_holds_it_up() {
    let body = her();
    let bust = body.bust.unwrap();
    let apex = bust.apex(1.0);
    let plain = Bodice::PLAIN;
    let midriff = Vec3::new(0.0, bust.fold - 0.08, 0.1);
    let shoulder_top = Vec3::new(0.12, 0.5, 0.0);
    let back = Vec3::new(0.0, 0.42, -0.1);
    // A full bodice with straps covers chest, midriff and back.
    for at in [apex, midriff, back] {
        assert!(bodice_covers(body, plain, true, at));
    }
    let crop = Bodice {
        cover: Cover::Crop(Hem::Even),
        ..plain
    };
    assert!(
        !bodice_covers(body, crop, true, midriff),
        "a crop top bares the midriff"
    );
    assert!(bodice_covers(body, crop, true, apex));
    let strapless = Bodice {
        straps: Straps::Strapless,
        ..plain
    };
    assert!(
        bodice_covers(body, strapless, false, apex),
        "it covers her bust"
    );
    assert!(!bodice_covers(body, strapless, false, shoulder_top));
    assert!(!bodice_covers(
        body,
        strapless,
        false,
        Vec3::new(0.0, 0.48, 0.1)
    ));
    let halter = Bodice {
        straps: Straps::Halter,
        ..plain
    };
    assert!(bodice_covers(body, halter, true, apex));
    assert!(
        !bodice_covers(body, halter, true, back),
        "a halter bares her back"
    );
    assert!(!bodice_covers(body, halter, true, shoulder_top));
    let one = Bodice {
        straps: Straps::OneShoulder,
        ..plain
    };
    assert!(
        bodice_covers(body, one, true, Vec3::new(-0.06, 0.52, 0.05)),
        "over her right"
    );
    assert!(
        !bodice_covers(body, one, true, Vec3::new(0.08, 0.47, 0.05)),
        "bare on her left"
    );
}

#[test]
fn a_mans_bodice_is_closed_over_both_shoulders_in_the_same_cloth() {
    let styled = Bodice {
        cover: Cover::Crop(Hem::Even),
        straps: Straps::Strapless,
        cut: Neckline::DEEP,
        pattern: Pattern::Ribbed,
        ..Bodice::PLAIN
    };
    let his = styled.fitted(false);
    assert_eq!(his.cover, Cover::Full);
    assert_eq!(his.straps, Straps::Shoulders);
    assert_eq!(his.cut, Neckline::Closed);
    assert_eq!(his.pattern, Pattern::Ribbed);
    assert_eq!(styled.fitted(true), styled);
}

#[test]
fn a_keyhole_opens_over_the_valley_beneath_a_closed_collar() {
    let body = her();
    let cut = NecklineCut::resolve(
        Neckline::Keyhole {
            depth: 0.7,
            width: 0.13,
        },
        body,
    )
    .unwrap();
    assert!(cut.keyhole);
    let mid = (cut.bottom + cut.collar - 0.05) * 0.5;
    assert!(cut.open(Vec3::new(0.0, mid, 0.1)), "open over the valley");
    assert!(
        !cut.open(Vec3::new(0.0, cut.collar - 0.01, 0.1)),
        "closed at the collar"
    );
    assert!(
        !cut.open(Vec3::new(0.0, cut.bottom - 0.01, 0.1)),
        "closed beneath"
    );
    let apex = body.bust.unwrap().apex(1.0);
    assert!(!cut.open(apex), "the fullness stays covered");
}

#[test]
fn styled_sleeves_say_which_arms_they_leave_bare() {
    let sleeve = |cut| {
        Arms::Styled(Sleeve {
            color: Vec3::ONE,
            cut,
            puff: false,
            puff_lining: None,
            sheer: false,
            flare: None,
        })
    };
    assert!(sleeve(SleeveCut::Puff).bare());
    assert!(sleeve(SleeveCut::OffShoulder).bare());
    assert!(sleeve(SleeveCut::Gauntlet).bare());
    assert!(!sleeve(SleeveCut::Bell).bare());
    assert!(Arms::Bare.bare() && !Arms::Plate.bare());
}

#[test]
fn boots_pulled_on_over_bare_legs_keep_their_shafts() {
    // The tallest satin face in the shin: a boot's shaft spans its whole
    // height; straps and cuffs are only a few centimetres tall.
    let shaft = |wardrobe: Wardrobe| {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(wardrobe);
        let outfit = Outfit::worn(&look);
        assert!(
            matches!(outfit.legs, Legs::Bare(_)),
            "{wardrobe:?} bares the legs"
        );
        let (world, rig, library, meshes) = crate::creatures::rogue_tests::spawn_one(&look);
        let satin = library.materials.satin();
        let shin = rig.joint(J::ShinL).unwrap();
        let mut tallest = 0.0f32;
        for child in world
            .get::<Children>(shin)
            .into_iter()
            .flat_map(|c| c.iter())
        {
            let is_satin = world
                .get::<MeshMaterial3d<StandardMaterial>>(child)
                .is_some_and(|m| m.0 == satin);
            let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
                continue;
            };
            let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(ps)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                continue;
            };
            if !is_satin {
                continue;
            }
            let indices: Vec<usize> = mesh.indices().map_or(Vec::new(), |i| i.iter().collect());
            for tri in indices.chunks(3) {
                let ys = tri.iter().map(|&i| ps[i][1]);
                let (low, high) = ys.fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(y), h.max(y)));
                tallest = tallest.max(high - low);
            }
        }
        tallest
    };
    for wardrobe in [Wardrobe::Queen, Wardrobe::Pirate] {
        assert!(
            shaft(wardrobe) > 0.15,
            "{wardrobe:?} boots keep their shafts"
        );
    }
    assert!(shaft(Wardrobe::Princess) < 0.06, "pumps bare the shin");
}

#[test]
fn pointed_hems_repeat_across_the_back_seam_and_mirrored_angles() {
    let hem = Hem::Points {
        count: 7,
        depth: 0.42,
    };
    for step in -160..=160 {
        let angle = step as f32 * TAU / 80.0;
        let reach = hem.reach(angle);
        assert!(
            (0.58 - 1e-5..=1.0 + 1e-5).contains(&reach),
            "a mirrored panel must stay within its authored hem: angle={angle}, reach={reach}"
        );
        assert!(
            (reach - hem.reach(angle + TAU)).abs() < 1e-5,
            "the same panel must meet itself at the back seam"
        );
        assert!(
            (reach - hem.reach(-angle)).abs() < 1e-5,
            "mirrored panels must have matching lengths"
        );
    }
}

#[test]
fn independent_underlayers_keep_their_cut_material_and_motion_length() {
    use super::super::outfit::{Layer, Motif};
    let parent = Skirt {
        length: 0.35,
        split: 0.2,
        fullness: 1.4,
        pattern: Pattern::Leopard(Vec3::ZERO),
        ..Skirt::PLAIN
    };
    let layer = Layer {
        longer: 0.27,
        split: Some(0.04),
        fullness: Some(1.1),
        hem: Hem::HighLow(0.2),
        trim: Some(Vec3::Y),
        pattern: Pattern::Brocade {
            thread: Vec3::X,
            motif: Motif::Sun,
        },
        ..Layer::petticoat(Vec3::Z, true)
    };
    let resolved = layer.resolve(parent);
    assert!((resolved.length - 0.62).abs() < 1e-6);
    assert_eq!(resolved.split, 0.04);
    assert_eq!(resolved.fullness, 1.1);
    assert_eq!(resolved.hem, layer.hem);
    assert_eq!(resolved.pattern, layer.pattern);
    assert_eq!(resolved.color, Vec3::Z);
    assert!(resolved.sheer);
    assert_eq!(resolved.trim, Some(Vec3::Y));
    assert!(
        resolved.layers.iter().all(Option::is_none),
        "a layer must not recursively duplicate its parent's petticoats"
    );
    let surface = ClothSurface::new(
        skirt_cut(her(), 0.25, resolved.length, false, false),
        resolved.length,
        12,
        resolved.split,
        0,
    )
    .with_hem(resolved.hem);
    assert!(
        surface.point(1.0, PI, false).y < -0.60,
        "the long back reaches its authored hem"
    );
    assert!(
        surface.point(1.0, 0.0, false).y > surface.point(1.0, PI, false).y,
        "the underlayer owns its high-low silhouette"
    );
}

#[test]
fn embroidered_motifs_keep_both_thread_and_ground_cloth_visible() {
    use super::super::outfit::Motif;
    for motif in [
        Motif::Leaf,
        Motif::Sun,
        Motif::Lily,
        Motif::Knot,
        Motif::Bone,
        Motif::Scroll,
    ] {
        let pattern = Pattern::Brocade {
            thread: Vec3::X,
            motif,
        };
        let mut stitches = 0;
        for row in 0..50 {
            for column in 0..50 {
                let at = Vec3::new(column as f32 * 0.115 / 50.0, row as f32 * 0.145 / 50.0, 0.0);
                let color = pattern.paint(Vec3::Z, at);
                assert!(color == Vec3::X || color == Vec3::Z);
                stitches += usize::from(color == Vec3::X);
                assert_eq!(
                    color,
                    pattern.paint(Vec3::Z, at),
                    "stable cloth-space pigment"
                );
            }
        }
        assert!(
            (100..1600).contains(&stitches),
            "{motif:?}: visible stitches without coating all the cloth: {stitches}"
        );
    }
}

#[test]
fn paired_study_garments_choose_their_own_openings_without_changing_the_body() {
    use super::super::outfit::Outfit;
    for wardrobe in Wardrobe::STUDIES {
        for woman in [false, true] {
            let original = Look::with_choice(
                CreatureModel::Human,
                7,
                Some(Variant { woman, ..default() }),
            );
            let dressed = original.clone().dressed(wardrobe);
            assert_eq!(
                dressed.woman(),
                original.woman(),
                "clothes retain the wearer's sex"
            );
            assert_eq!(
                dressed.skin, original.skin,
                "clothes retain the wearer's skin"
            );
            let outfit = Outfit::worn(&dressed);
            assert!(
                outfit.skirt.is_some(),
                "{wardrobe:?} has its study's layered lower silhouette"
            );
            if woman {
                let body = HumanoidBody::resolve(&dressed);
                let cut = NecklineCut::resolve(outfit.bodice.cut, body)
                    .expect("adult fantasy study opening");
                assert!(
                    cut.open(Vec3::new(0.0, 0.41, 0.2)),
                    "{wardrobe:?} exposes the sternum"
                );
                let apex = body.bust.unwrap().apex(1.0);
                assert!(
                    !cut.open(apex),
                    "{wardrobe:?} keeps the garment fitted over the fullness"
                );
            }
        }
    }
    assert_eq!(Wardrobe::parse("elven-ranger"), Some(Wardrobe::ElvenRanger));
    assert_eq!(
        Wardrobe::parse("vampire-noble"),
        Some(Wardrobe::VampireNoble)
    );
}

fn garment_positions(parts: Parts) -> Vec<Vec3> {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut meshes = Assets::<Mesh>::default();
    let model = parts.finish(
        &mut meshes,
        &crate::props::PropMaterials::new(&mut materials),
    );
    model
        .0
        .iter()
        .flat_map(|(handle, _)| {
            let mesh = meshes.get(handle).unwrap();
            match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                bevy::render::mesh::VertexAttributeValues::Float32x3(positions) => positions
                    .iter()
                    .copied()
                    .map(Vec3::from)
                    .collect::<Vec<_>>(),
                _ => panic!("garment vertex positions"),
            }
        })
        .collect()
}

#[test]
fn shared_fringe_scales_with_its_support_and_hangs_below_the_rim() {
    use super::super::outfit::{Fringe, FringeKind};
    for kind in [FringeKind::Fur, FringeKind::Feathers, FringeKind::Leaves] {
        let positions = |scale| {
            let mut parts = Parts::default();
            fringe_parts::rim(
                &mut parts,
                Fringe {
                    kind,
                    color: Vec3::ONE,
                    accent: Vec3::splat(0.3),
                    length: 0.16,
                },
                Vec3::ZERO,
                Vec2::new(0.10, 0.08) * scale,
                scale,
            );
            garment_positions(parts)
        };
        let small = positions(0.7);
        let large = positions(1.4);
        assert!(!small.is_empty());
        assert_eq!(small.len(), large.len());
        assert!(small.iter().all(|p| p.is_finite()));
        assert!(
            small.iter().any(|p| p.y < -0.09),
            "{kind:?} has a hanging silhouette"
        );
        assert!(
            small.iter().any(|p| p.x > 0.06) && small.iter().any(|p| p.x < -0.06),
            "{kind:?} wraps both sides of its support"
        );
        for (a, b) in small.into_iter().zip(large) {
            assert!(
                b.distance(a * 2.0) < 1e-5,
                "{kind:?} must fit larger bodies without fixed-size tips"
            );
        }
    }
}

#[test]
fn winged_headwear_has_two_solid_fans_and_leaves_the_lower_face_open() {
    let mut parts = Parts::default();
    headwear_parts::winged_helm(&mut parts, STEEL, Vec3::Z);
    let points = garment_positions(parts);
    assert!(points.iter().all(|p| p.is_finite() && p.y > 0.10));
    for side in [-1.0, 1.0] {
        assert!(
            points.iter().any(|p| p.x * side > 0.16 && p.y > 0.34),
            "each side of the open helmet has an upward feather fan"
        );
    }
}

#[test]
fn winged_helm_tucks_crown_hair_and_pointed_ears_under_its_cap() {
    use super::super::outfit::Wardrobe;
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Leprechaun,
    ] {
        for woman in [false, true] {
            for style in 0..5 {
                let wearer = Look::with_choice(
                    model,
                    7,
                    Some(Variant {
                        woman,
                        hair: style,
                        ..default()
                    }),
                );
                let covered = wearer.clone().dressed(Wardrobe::Valkyrie);
                assert!(
                    !shows_hair(&covered),
                    "{model:?} {woman} {style}: crown must fit under the cap"
                );
                assert!(
                    !loose_fall(&covered),
                    "a crown-rooted fall cannot pierce the helmet"
                );
                assert!(
                    !rogue::hair_shows(&covered),
                    "native topknots also tuck under the cap"
                );
                let tip = rogue::ear_tip(&covered, Vec3::new(0.24, 0.2, -0.08));
                assert!(
                    tip.x <= rogue::EAR_UNDER_COVER,
                    "pointed ears fit inside the cap"
                );
                assert!(
                    shows_hair(&wearer.dressed(Wardrobe::Commoner)),
                    "removing the cap restores the hairstyle"
                );
            }
        }
    }
}

#[test]
fn open_helmet_keeps_its_front_band_above_the_eyes() {
    let mut parts = Parts::default();
    headwear_parts::winged_helm(&mut parts, STEEL, Vec3::Z);
    let points = garment_positions(parts);
    let front: Vec<_> = points
        .iter()
        .filter(|p| p.z > 0.075 && p.x.abs() < 0.08)
        .collect();
    assert!(
        !front.is_empty(),
        "the open helmet retains its forehead band"
    );
    assert!(
        front.iter().all(|p| p.y > 0.205),
        "the forehead band must clear the eyes and brows; lowest front = {}",
        front.iter().map(|p| p.y).fold(f32::INFINITY, f32::min)
    );
}

#[test]
fn embroidered_skirts_use_narrow_threads_instead_of_coloring_whole_cloth_faces() {
    use super::super::outfit::Motif;
    for motif in [
        Motif::Leaf,
        Motif::Sun,
        Motif::Lily,
        Motif::Knot,
        Motif::Bone,
        Motif::Scroll,
    ] {
        let surface =
            ClothSurface::new(skirt_cut(her(), 0.25, 0.5, false, false), 0.5, 12, 0.12, 0);
        let mut parts = Parts::default();
        skirt_cloth(
            &mut parts.matte,
            surface,
            Skirt {
                color: Vec3::Z,
                trim: None,
                pattern: Pattern::Brocade {
                    thread: Vec3::X,
                    motif,
                },
                ..Skirt::PLAIN
            },
        );
        let mut materials = Assets::<StandardMaterial>::default();
        let mut meshes = Assets::<Mesh>::default();
        let model = parts.finish(
            &mut meshes,
            &crate::props::PropMaterials::new(&mut materials),
        );
        let mut threads = 0;
        for (handle, _) in &model.0 {
            let mesh = meshes.get(handle).unwrap();
            let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(points)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions")
            };
            let Some(bevy::render::mesh::VertexAttributeValues::Float32x4(colors)) =
                mesh.attribute(Mesh::ATTRIBUTE_COLOR)
            else {
                panic!("colors")
            };
            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
            for tri in indices.as_chunks::<3>().0 {
                if !tri
                    .iter()
                    .all(|&i| colors[i][0] > 0.1 && colors[i][1] < 0.001 && colors[i][2] < 0.001)
                {
                    continue;
                }
                threads += 1;
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| Vec3::from(points[i]));
                let longest = a.distance(b).max(b.distance(c)).max(c.distance(a));
                let width = (b - a).cross(c - a).length() / longest.max(1e-6);
                assert!(
                width <= 0.006,
                "embroidery must be a narrow fitted stroke, not a cloth-sized color patch: {width}"
            );
            }
        }
        assert!(
            threads > 50,
            "the ornament must remain present, not be removed to hide coarse sampling"
        );
    }
}

#[test]
fn male_studies_use_tunics_robes_and_coats_without_female_bodice_details() {
    use super::super::outfit::{Neckline, Outfit, Support, TorsoOrnament};
    for wardrobe in Wardrobe::STUDIES {
        let wearer = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: false,
                ..default()
            }),
        );
        let outfit = Outfit::worn(&wearer.clone().dressed(wardrobe));
        assert!(
            matches!(outfit.bodice.cut, Neckline::Closed)
                || matches!(outfit.bodice.cut, Neckline::Plunge { width, depth, .. }
                if width <= 0.1 && depth <= 0.55),
            "{wardrobe:?}: narrow male shirt collar"
        );
        assert!(
            outfit.bodice.lacing.is_none(),
            "{wardrobe:?}: no female corset lacing"
        );
        assert!(
            outfit.trimmings.garter.is_none(),
            "{wardrobe:?}: no female thigh garter"
        );
        assert!(
            !matches!(
                outfit.torso_ornament,
                Some(TorsoOrnament::LeafBodice { .. })
            ),
            "{wardrobe:?}: male leaf tunic rather than female leaf bodice"
        );
        if wardrobe == Wardrobe::Barbarian {
            assert_eq!(
                outfit.tunic, wearer.skin,
                "male barbarian has an exposed torso"
            );
            assert!(matches!(
                outfit.torso_ornament,
                Some(TorsoOrnament::Harness { .. })
            ));
        }
        if matches!(wardrobe, Wardrobe::Cleric | Wardrobe::Druid) {
            let robe = outfit.skirt.unwrap();
            assert!(robe.cutaway, "{wardrobe:?}: open male panels over trousers");
            assert!(matches!(
                outfit.legs,
                super::super::outfit::Legs::Trousers(_)
            ));
        }
        if !matches!(wardrobe, Wardrobe::Paladin | Wardrobe::Valkyrie) {
            assert_eq!(
                outfit.bodice.support,
                Support::Loose,
                "male garment construction"
            );
        }
    }
}

#[test]
fn bare_torso_is_uncovered_on_both_body_types() {
    for woman in [false, true] {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant { woman, ..default() }),
        );
        let body = HumanoidBody::resolve(&look);
        let bare = Bodice {
            cover: Cover::Bare,
            ..Bodice::PLAIN
        }
        .fitted(woman);
        for at in [
            Vec3::new(0.0, 0.4, 0.15),
            Vec3::new(0.1, 0.2, 0.1),
            Vec3::new(0.0, 0.4, -0.15),
        ] {
            assert!(!bodice_covers(body, bare, true, at));
        }
    }
}

fn head_triangles(parts: Parts) -> Vec<[Vec3; 3]> {
    let mut materials = Assets::<StandardMaterial>::default();
    let mut meshes = Assets::<Mesh>::default();
    let model = parts.finish(
        &mut meshes,
        &crate::props::PropMaterials::new(&mut materials),
    );
    model
        .0
        .iter()
        .flat_map(|(handle, _)| {
            let mesh = meshes.get(handle).unwrap();
            let bevy::render::mesh::VertexAttributeValues::Float32x3(positions) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
            else {
                panic!("positions")
            };
            let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
            indices
                .as_chunks::<3>()
                .0
                .iter()
                .map(|t| t.map(|i| Vec3::from(positions[i])))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn triangle_gap(point: Vec3, [a, b, c]: [Vec3; 3]) -> f32 {
    let normal = (b - a).cross(c - a).normalize_or_zero();
    let projected = point - normal * (point - a).dot(normal);
    let inside = [(a, b), (b, c), (c, a)]
        .iter()
        .all(|(start, end)| (*end - *start).cross(projected - *start).dot(normal) >= -1e-8);
    if inside && normal.length_squared() > 0.5 {
        return point.distance(projected);
    }
    [(a, b), (b, c), (c, a)]
        .iter()
        .map(|(start, end)| {
            let edge = *end - *start;
            let t = ((point - *start).dot(edge) / edge.length_squared().max(1e-20)).clamp(0.0, 1.0);
            point.distance(*start + edge * t)
        })
        .fold(f32::INFINITY, f32::min)
}

#[test]
fn surface_gap_measures_triangle_interiors_and_edges() {
    let triangle = [Vec3::ZERO, Vec3::X, Vec3::Y];
    assert!((triangle_gap(Vec3::new(0.3, 0.3, 0.01), triangle) - 0.01).abs() < 1e-6);
    assert!((triangle_gap(Vec3::new(-0.1, 0.3, 0.0), triangle) - 0.1).abs() < 1e-6);
}

#[test]
fn brow_ornaments_follow_the_wearers_head_instead_of_floating_rings() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Troll,
    ] {
        for woman in [false, true] {
            for style in 0..3 {
                let look = Look::with_choice(
                    model,
                    7,
                    Some(Variant {
                        woman,
                        hair: style,
                        ..default()
                    }),
                );
                let head = || {
                    let mut parts = Parts::default();
                    human_face(&mut parts, &look, true);
                    if shows_hair(&look) {
                        hair(&mut parts, &look, true);
                    }
                    parts
                };
                let surface = head_triangles(head());
                let mut parts = head();
                parts.metal = Builder::default();
                headwear_parts::circlet(&mut parts, GILT, None, false);
                let ring = garment_positions(Parts {
                    metal: parts.metal,
                    ..Parts::default()
                });
                let front: Vec<_> = ring.iter().filter(|p| p.z > 0.0).collect();
                assert!(!front.is_empty());
                for p in front {
                    let gap = surface
                        .iter()
                        .map(|triangle| triangle_gap(*p, *triangle))
                        .fold(f32::INFINITY, f32::min);
                    assert!(gap < 0.025,
                        "{model:?} woman={woman} hair={style}: band floats {gap} from actual head at {p:?}");
                    assert!(p.y > 0.215, "brow ornaments must clear the eyes");
                }
            }
        }
    }
}

#[test]
fn wreaths_leave_the_front_of_the_face_unobscured() {
    let mut parts = Parts::default();
    headwear_parts::wreath(&mut parts, Vec3::Y);
    let positions = garment_positions(parts);
    assert!(
        positions.iter().filter(|p| p.z > 0.04).all(|p| p.y > 0.215),
        "wreath stem and leaves sit above eyebrows, not across the eyes"
    );
}

#[test]
fn fur_shoulders_cover_the_support_instead_of_forming_hollow_brushes() {
    use super::super::outfit::{Fringe, FringeKind};
    for scale in [0.7, 1.0, 1.4] {
        let mut parts = Parts::default();
        let center = Vec3::new(0.03, 0.04, -0.02) * scale;
        fringe_parts::shoulder(
            &mut parts,
            Fringe {
                kind: FringeKind::Fur,
                color: Vec3::splat(0.4),
                accent: Vec3::splat(0.8),
                length: 0.13,
            },
            center,
            Vec2::new(0.105, 0.095) * scale,
            scale,
        );
        for x in [-0.06, 0.0, 0.06] {
            for z in [-0.05, 0.0, 0.05] {
                let origin = center + Vec3::new(x, -0.08, z) * scale;
                let top = parts
                    .matte
                    .outer_surface(origin, Vec3::Y)
                    .expect("fur shoulder needs continuous backing beneath its pile");
                assert!(top.y > center.y + 0.005 * scale);
                assert!(top.y < center.y + 0.10 * scale);
            }
        }
    }
}

#[test]
fn upper_pelvis_meets_the_torso_without_a_visible_waist_shelf() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Troll,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                for figure in 0..3 {
                    let look = Look::with_choice(
                        model,
                        7,
                        Some(Variant {
                            woman,
                            build,
                            figure,
                            ..default()
                        }),
                    );
                    let body = HumanoidBody::resolve(&look);
                    let pelvis = pelvic_supports(body, Vec3::ZERO);
                    let torso = torso_shape(body, Some(TorsoMaterial::Cloth), true);
                    for y in [0.06, 0.08, 0.095] {
                        for k in 0..16 {
                            let a = k as f32 * TAU / 16.0;
                            let out = Vec3::new(a.sin(), 0.0, a.cos());
                            let surface = pelvis_surface(&pelvis, y, out);
                            if pelvis.distance(surface).abs() < 0.002 {
                                let gap = torso.distance(surface - body.torso);
                                assert!(gap < 0.006,
                                    "{model:?} woman={woman} build={build} figure={figure} y={y}: pelvis projects {gap} beyond torso at {surface:?}");
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn fitted_belt_clears_the_torso_at_both_edges() {
    for woman in [false, true] {
        for build in 0..3 {
            let look = Look::with_choice(
                CreatureModel::Human,
                7,
                Some(Variant {
                    woman,
                    build,
                    figure: 2,
                    ..default()
                }),
            );
            let body = HumanoidBody::resolve(&look);
            for material in [
                None,
                Some(TorsoMaterial::Cloth),
                Some(TorsoMaterial::Leather),
            ] {
                let torso = torso_shape(body, material, true);
                let mut parts = Parts::default();
                waist_parts::band(
                    &mut parts.satin,
                    body,
                    &torso,
                    Vec2::new(BELT_BOTTOM_Y, BELT_TOP_Y),
                    Vec3::ONE,
                );
                let vertices = garment_positions(parts);
                assert!(!vertices.is_empty());
                for p in vertices {
                    let gap = torso.distance(p - body.torso);
                    assert!(
                        gap > 0.003 && gap < 0.008,
                        "woman={woman} build={build}: actual band clearance {gap} at {p:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn upper_seat_mesh_stops_below_the_waistband() {
    let body = HumanoidBody::resolve(&Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    ));
    let mut parts = Parts::default();
    pelvis_shape(body).mesh(
        &mut parts.matte,
        Vec3::new(-0.3, -0.2, -0.25),
        Vec3::new(0.3, 0.2, 0.25),
        0.0085,
        &|_, _| Vec3::ONE,
    );
    assert!(
        garment_positions(parts).iter().all(|p| p.y < BELT_BOTTOM_Y),
        "seat pigment must not overlap the bodice above the belt"
    );
}

#[test]
fn princess_reference_has_a_long_purple_gown_and_male_fallback() {
    for woman in [true, false] {
        let wearer = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant { woman, ..default() }),
        );
        let outfit = Outfit::from_wardrobe(Wardrobe::RoyalPrincess, &wearer);
        if woman {
            assert!(
                outfit.tunic.z > outfit.tunic.y * 2.0,
                "royal purple, not blue"
            );
            assert!(outfit.skirt.unwrap().length > 0.8, "floor length gown");
            assert!(matches!(outfit.arms, Arms::Styled(s) if s.cut == SleeveCut::OffShoulder));
            assert!(
                outfit.bodice.lacing.is_none(),
                "unobscured sweetheart opening"
            );
            assert!(matches!(outfit.feet, Feet::Shoes(s) if s.sandal && s.heel));
        } else {
            assert_eq!(
                Outfit::from_wardrobe(Wardrobe::Princess, &wearer),
                Outfit::from_wardrobe(Wardrobe::Courtier, &wearer),
                "both princess styles keep male figures in their own outfit"
            );
            assert_eq!(
                outfit,
                Outfit::from_wardrobe(Wardrobe::Courtier, &wearer),
                "male figures use the noble outfit, never the princess dress"
            );
        }
    }
}

#[test]
fn princess_sweetheart_exposes_the_valley_but_covers_each_bust_apex() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for build in 0..3 {
            for figure in 0..3 {
                let wearer = Look::with_choice(
                    model,
                    7,
                    Some(Variant {
                        woman: true,
                        build,
                        figure,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&wearer);
                let outfit = Outfit::from_wardrobe(Wardrobe::RoyalPrincess, &wearer);
                let bust = body.bust.unwrap();
                let apex = bust.apex(1.0);
                assert!(
                    bodice_covers(body, outfit.bodice, true, apex),
                    "covered cup {model:?}"
                );
                let valley = Vec3::new(0.0, apex.y, apex.z);
                assert!(
                    !bodice_covers(body, outfit.bodice, true, valley),
                    "open valley {model:?}"
                );
                let center = sweetheart_top(body, 0.0, 1.0, 0.55);
                assert!(center < apex.y - 0.01, "deep center dip");
                assert!(center > bust.fold, "dip stays above the underbust fold");
                assert!(!bodice_covers(
                    body,
                    outfit.bodice,
                    true,
                    Vec3::new(0.12, 0.51, 0.08)
                ));
            }
        }
    }
}

#[test]
fn princess_center_panel_leaves_two_thigh_openings() {
    let body = her();
    let skirt = Outfit::from_wardrobe(
        Wardrobe::RoyalPrincess,
        &Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        ),
    )
    .skirt
    .unwrap();
    let cut = skirt_cut(body, 0.4, skirt.length, false, skirt.cutaway);
    let surface = ClothSurface::new(cut, skirt.length, 12, skirt.split, 7);
    let width = skirt.center_panel.expect("central drape");
    for t in [0.25, 0.65, 1.0] {
        let right = surface.point(t, 0.0, false);
        let left = surface.point(t, TAU, false);
        let center_right = royal_parts::panel_point(&surface, t, 1.0, width, false);
        let center_left = royal_parts::panel_point(&surface, t, -1.0, width, false);
        assert!(right.x - center_right.x > 0.03, "right thigh opening");
        assert!(center_left.x - left.x > 0.03, "left thigh opening");
        assert!(
            center_left.x < 0.0 && center_right.x > 0.0,
            "modest central coverage"
        );
    }
}

#[test]
fn royal_princess_keeps_the_original_blue_princess_available() {
    let wearer = Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    let blue = Outfit::from_wardrobe(Wardrobe::Princess, &wearer);
    let royal = Outfit::from_wardrobe(Wardrobe::RoyalPrincess, &wearer);
    assert!(Wardrobe::ALL.contains(&Wardrobe::Princess));
    assert!(Wardrobe::ALL.contains(&Wardrobe::RoyalPrincess));
    assert_eq!(
        Wardrobe::parse("royal-princess"),
        Some(Wardrobe::RoyalPrincess)
    );
    assert_eq!(blue.tunic, Vec3::new(0.62, 0.78, 0.96));
    assert_eq!(blue.skirt.unwrap().length, 0.3);
    assert_eq!(blue.bodice.straps, Straps::Strapless);
    assert_eq!(royal.bodice.straps, Straps::Sweetheart);
}

#[test]
fn royal_princess_uses_fine_jewelry_and_fitted_boning() {
    use super::super::outfit::TorsoOrnament;
    let wearer = Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    let outfit = Outfit::from_wardrobe(Wardrobe::RoyalPrincess, &wearer);
    assert!(
        outfit.cuffs.is_none(),
        "bracelets must not become forearm armor"
    );
    assert!(outfit.trimmings.bracelets.is_some());
    assert!(outfit.trimmings.earrings.is_some());
    assert!(matches!(
        outfit.torso_ornament,
        Some(TorsoOrnament::Corset { .. })
    ));
    assert_eq!(
        outfit.bodice.pattern,
        Pattern::Plain,
        "fine gold geometry replaces coarse pigment patches"
    );
}

#[test]
fn cloth_support_sampling_matches_the_rendered_triangle_diagonal() {
    let surface = ClothSurface::new(
        SkirtCut::straight(Vec2::new(0.1, 0.13), Vec2::new(0.35, 0.28)),
        0.86,
        12,
        0.3,
        7,
    );
    let row = surface.rows() - 2;
    let side = 3;
    let a = surface.point(
        row as f32 / surface.rows() as f32,
        side as f32 * TAU / 64.0,
        false,
    );
    let c = surface.point(
        (row + 1) as f32 / surface.rows() as f32,
        (side + 1) as f32 * TAU / 64.0,
        false,
    );
    let midpoint = surface.mesh_point(
        (row as f32 + 0.5) / surface.rows() as f32,
        (side as f32 + 0.5) * TAU / 64.0,
        false,
    );
    assert!(midpoint.distance((a + c) * 0.5) < 1e-6);
}

#[test]
fn raised_gown_waist_clears_the_upper_hip_between_seam_and_thigh() {
    for figure in 0..3 {
        let wearer = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                figure,
                ..default()
            }),
        );
        let body = HumanoidBody::resolve(&wearer);
        let torso = torso_shape(body, None, false);
        let pelvis = pelvic_supports(body, Vec3::ZERO);
        let mut surface =
            ClothSurface::new(skirt_cut(body, 0.4, 0.86, false, true), 0.86, 12, 0.33, 7);
        surface.paired = true;
        surface.seam = Some(std::sync::Arc::new(
            (0..=64)
                .map(|k| {
                    let a = k as f32 * TAU / 64.0;
                    waist_parts::point(
                        body,
                        &torso,
                        0.135 - 0.075 * a.cos().max(0.0).powi(4),
                        a,
                        0.006,
                    )
                })
                .collect(),
        ));
        surface = surface.fit_pelvis(body);
        let mut witnesses = 0;
        for row in 0..=surface.rows() {
            for column in 0..=64 {
                let point = surface.point(
                    row as f32 / surface.rows() as f32,
                    column as f32 * TAU / 64.0,
                    true,
                );
                if (-0.08..=0.12).contains(&point.y) {
                    witnesses += 1;
                    assert!(
                        pelvis.distance(point) > 0.002,
                        "figure {figure}, hip intersection at {point:?}"
                    );
                }
            }
        }
        assert!(witnesses > 100, "exercise the complete hip transition");
    }
}

#[test]
fn corset_boning_edges_stay_on_narrow_torsos() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Nymph,
        CreatureModel::Orc,
    ] {
        for figure in 0..3 {
            let wearer = Look::with_choice(
                model,
                7,
                Some(Variant {
                    woman: true,
                    figure,
                    ..default()
                }),
            );
            let body = HumanoidBody::resolve(&wearer);
            let form = torso_shape(body, None, false);
            for side in [-1.0, 1.0] {
                for share in [0.42, 0.8] {
                    for (at, width) in royal_parts::boning_path(body, &form, side, share) {
                        for x in [at.x - width, at.x + width] {
                            assert!(
                                torso_surface_hit(&form, x, at.y, true).is_some(),
                                "{model:?} figure {figure}: gold seam leaves torso at {x},{}",
                                at.y
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn bard_puffs_extend_down_the_upper_arm_on_both_body_cuts() {
    for woman in [false, true] {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant { woman, ..default() }),
        )
        .dressed(Wardrobe::Bard);
        let Arms::Styled(sleeve) = Outfit::worn(&look).arms else {
            panic!("styled bard sleeve")
        };
        for scale in [0.7, 1.0, 1.4] {
            for side in [-1.0, 1.0] {
                let mut parts = Parts::default();
                styled_sleeve_top(&mut parts, sleeve, side, scale, scale);
                let positions = garment_positions(parts);
                assert!(positions.iter().all(|p| p.is_finite()));
                assert!(
                    positions.iter().any(|p| p.y < -0.17 * scale),
                    "gathered puff must extend down the upper arm, not form a shoulder cap"
                );
            }
        }
    }
}

#[test]
fn slashed_puffs_have_recessed_continuous_lining_between_raised_panels() {
    for scale in [0.7, 1.0, 1.4] {
        for length in [0.8, 1.2] {
            let mut b = Builder::default();
            sleeve_parts::slashed_puff(&mut b, 1.0, scale, length, Vec3::X, Vec3::Y);
            let center = Vec3::new(0.008 * scale, -0.086 * length, 0.0);
            for panel in 0..8 {
                let a = panel as f32 * TAU / 8.0;
                let radial = |angle: f32| {
                    let direction = Vec3::new(angle.sin(), 0.0, angle.cos());
                    b.outer_surface(center, direction)
                        .expect("continuous lining covers every slit")
                        .distance(center)
                };
                let ridge = radial(a);
                let lining = radial(a + TAU / 16.0);
                assert!(lining > 0.085 * scale, "lining retains the puff volume");
                assert!(
                    ridge > lining + 0.004 * scale,
                    "raised strip must stand above its lining"
                );
                assert!(
                    ridge < 0.105 * scale,
                    "puff stays within its authored silhouette"
                );
            }
        }
    }
}

#[test]
fn bard_lining_is_independent_of_flare_and_preserves_separate_body_cuts() {
    for woman in [false, true] {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant { woman, ..default() }),
        )
        .dressed(Wardrobe::Bard);
        let Arms::Styled(sleeve) = Outfit::worn(&look).arms else {
            panic!("styled sleeve")
        };
        let lining = sleeve.puff_lining.expect("Bard has cream slashed puffs");
        assert!(lining.min_element() > 0.6 && sleeve.color.y < 0.1);
        assert_eq!(
            sleeve.cut,
            if woman {
                SleeveCut::Bell
            } else {
                SleeveCut::Puff
            }
        );
        let original = Outfit::worn(&look.dressed(Wardrobe::Princess));
        if woman {
            let Arms::Styled(plain) = original.arms else {
                panic!("original princess puff")
            };
            assert_eq!(
                plain.puff_lining, None,
                "plain puffs remain a separate construction"
            );
        }
    }
}

#[test]
fn gathered_puffs_close_over_the_shoulder_without_open_top_rings() {
    for scale in [0.7, 1.0, 1.4] {
        let mut b = Builder::default();
        sleeve_parts::slashed_puff(&mut b, 1.0, scale, scale, Vec3::X, Vec3::Y);
        for x in [-0.025, 0.0, 0.025] {
            for z in [-0.025, 0.0, 0.025] {
                let origin = Vec3::new(0.008 + x, 0.0, z) * scale;
                let cap = b
                    .outer_surface(origin, Vec3::Y)
                    .expect("gathered puff must cover its shoulder opening");
                assert!(cap.y > 0.025 * scale && cap.y < 0.06 * scale);
            }
        }
    }
}

#[test]
fn embroidered_torsos_use_fine_threads_instead_of_mesh_sized_color_patches() {
    use super::super::outfit::Motif;
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            let mut look = Look::with_choice(model, 7, Some(Variant { woman, ..default() }))
                .dressed(Wardrobe::Bard);
            let mut outfit = Outfit::worn(&look);
            outfit.tunic = Vec3::Z;
            outfit.bodice.pattern = Pattern::Brocade {
                thread: Vec3::X,
                motif: Motif::Scroll,
            };
            look.outfit = Some(outfit);
            let (world, rig, _, meshes) = crate::creatures::rogue_tests::spawn_one(&look);
            let torso = rig.joint(J::Torso).unwrap();
            let mut threads = 0;
            for child in world
                .get::<Children>(torso)
                .into_iter()
                .flat_map(|c| c.iter())
            {
                let Some(mesh) = world.get::<Mesh3d>(child).and_then(|m| meshes.get(&m.0)) else {
                    continue;
                };
                let Some(bevy::render::mesh::VertexAttributeValues::Float32x3(points)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("positions")
                };
                let Some(bevy::render::mesh::VertexAttributeValues::Float32x4(colors)) =
                    mesh.attribute(Mesh::ATTRIBUTE_COLOR)
                else {
                    panic!("colors")
                };
                let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
                for tri in indices.as_chunks::<3>().0 {
                    if !tri.iter().all(|&i| {
                        colors[i][0] > 0.1 && colors[i][1] < 0.001 && colors[i][2] < 0.001
                    }) {
                        continue;
                    }
                    threads += 1;
                    let [a, b, c] = tri.map(|i| Vec3::from(points[i]));
                    let longest = a.distance(b).max(b.distance(c)).max(c.distance(a));
                    let width = (b - a).cross(c - a).length() / longest.max(1e-6);
                    assert!(width <= 0.006,
                        "{model:?} woman={woman}: thread must be finer than body mesh faces, width={width}");
                }
            }
            assert!(
                threads > 50,
                "{model:?} woman={woman}: visible thread geometry must remain"
            );
        }
    }
}

#[test]
fn fitted_stitches_stop_at_openings_instead_of_bridging_bare_skin() {
    let mut parts = Parts::default();
    embroidery_parts::fitted_stitches(
        &mut parts.matte,
        &[Vec2::new(-0.4, 0.0), Vec2::new(0.4, 0.0)],
        &|uv| (uv.x.abs() >= 0.08).then_some(uv.extend(0.0)),
        Vec3::X,
    );
    let positions = garment_positions(parts);
    assert!(positions.iter().any(|p| p.x < -0.1));
    assert!(positions.iter().any(|p| p.x > 0.1));
    assert!(
        positions.iter().all(|p| p.x.abs() > 0.075),
        "stitches must terminate at each edge of an exposed opening"
    );
}

#[test]
fn fitted_bodice_threads_stay_on_cloth_and_clear_the_support() {
    use super::super::outfit::Motif;
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        let look = Look::with_choice(
            model,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(Wardrobe::Bard);
        let body = HumanoidBody::resolve(&look);
        let base = Outfit::worn(&look).bodice;
        for bodice in [
            base,
            Bodice {
                straps: Straps::Sweetheart,
                ..base
            },
            Bodice {
                cover: Cover::Crop(Hem::Even),
                ..base
            },
        ] {
            let form = torso_shape(body, None, true);
            let neckline = NecklineCut::resolve(bodice.cut, body);
            let covered = |p| {
                bodice_covers(body, bodice, true, p) && !neckline.is_some_and(|cut| cut.open(p))
            };
            let mut parts = Parts::default();
            embroidery_parts::bodice(
                &mut parts.matte,
                &form,
                body.build,
                Motif::Scroll,
                Vec3::X,
                &|p, _| covered(p),
            );
            let points = garment_positions(parts);
            assert!(points.len() > 100);
            for point in points {
                assert!(
                    covered(point),
                    "{model:?}: thread crosses a bare edge at {point:?}"
                );
                let gap = form.distance(point);
                assert!(
                    (-0.0003..0.008).contains(&gap),
                    "{model:?}: thread must sit on its support, gap={gap} at {point:?}"
                );
            }
        }
    }
}

#[test]
fn cloth_bells_leave_the_wrist_open_instead_of_capping_it_with_a_disk() {
    for build in [0.7, 1.0, 1.4] {
        for length in [0.8, 1.2] {
            for muscle in [0.8, 1.2] {
                let mut b = Builder::default();
                biped_cuff(&mut b, build, length, muscle, true, Vec3::X, Vec3::Y);
                assert!(
                    b.outer_surface(Vec3::Y * -0.4 * length, Vec3::Y).is_none(),
                    "a cloth bell must be a hollow sleeve, not a capped forearm solid"
                );
            }
        }
    }
}

#[test]
fn cloth_bells_have_a_thin_lining_and_real_fold_relief_at_every_scale() {
    for build in [0.7, 1.0, 1.4] {
        for length in [0.8, 1.2] {
            let mut parts = Parts::default();
            biped_cuff(&mut parts.matte, build, length, 1.0, true, Vec3::X, Vec3::Y);
            let points = garment_positions(parts);
            assert!(points.iter().all(|p| p.is_finite()));
            // Near the wrist, the folds stand beyond the authored elliptical
            // envelope while the lining stays only a cloth thickness inside it.
            let radii: Vec<_> = points
                .iter()
                .filter(|p| p.y < -0.255 * length)
                .map(|p| Vec2::new(p.x / build, p.z / build).length())
                .collect();
            let min = radii.iter().copied().fold(f32::INFINITY, f32::min);
            let max = radii.iter().copied().fold(0.0, f32::max);
            assert!(min > 0.070, "lining must not fill the opening: {min}");
            assert!(max > 0.090, "folds need actual radial relief: {max}");
            // A radial ray from the empty lumen must meet real inner cloth.
            for angle in [0.0, 0.3, 1.0, 2.0] {
                let direction = Vec3::new(f32::sin(angle), 0.0, f32::cos(angle));
                assert!(parts_shell_hit(build, length, direction));
            }
        }
    }
}

fn parts_shell_hit(build: f32, length: f32, direction: Vec3) -> bool {
    let mut b = Builder::default();
    biped_cuff(&mut b, build, length, 1.0, true, Vec3::X, Vec3::Y);
    b.outer_surface(Vec3::Y * -0.23 * length, direction)
        .is_some()
}

#[test]
fn belt_pouches_clear_the_resolved_hip_instead_of_using_a_fixed_depth() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Nymph,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    7,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                let support = torso_shape(body, None, false);
                let mut parts = Parts::default();
                waist_parts::pouch(&mut parts.satin, body, &support, Vec3::X);
                for point in garment_positions(parts) {
                    let gap = support
                        .distance(point - body.torso)
                        .min(pelvis_shape(body).distance(point));
                    assert!(gap >= 0.002, "{model:?} woman={woman} build={build}: pouch intersects hip at {point:?}, gap={gap}");
                }
            }
        }
    }
}

#[test]
fn belt_pouch_loops_reach_the_band_and_follow_clothed_support() {
    for material in [TorsoMaterial::Leather, TorsoMaterial::Plate] {
        for model in [
            CreatureModel::Human,
            CreatureModel::Orc,
            CreatureModel::Nymph,
        ] {
            let body = HumanoidBody::resolve(&Look::with_choice(
                model,
                7,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            ));
            let support = torso_shape(body, Some(material), false);
            let pelvis = pelvis_shape(body);
            let mut parts = Parts::default();
            waist_parts::pouch(&mut parts.satin, body, &support, Vec3::X);
            let points = garment_positions(parts);
            let mut loops = 0;
            let mut nearest = f32::INFINITY;
            for point in points {
                assert!(point.is_finite());
                let gap = support
                    .distance(point - body.torso)
                    .min(pelvis.distance(point));
                assert!(
                    gap >= 0.002,
                    "pouch or attachment cuts into clothing: {point:?}, {gap}"
                );
                nearest = nearest.min(gap);
                if point.y >= BELT_TOP_Y {
                    loops += 1;
                    assert!(gap < 0.018, "belt attachment floats off its support: {gap}");
                }
            }
            assert!(
                loops >= 4,
                "both attachments must reach the top of the belt"
            );
            assert!(
                nearest < 0.015,
                "pouch must sit against the wearer rather than float"
            );
        }
    }
}

#[test]
fn sylvan_duelist_is_additive_and_its_panels_cover_apices_but_leave_sides_open() {
    let wardrobe = Wardrobe::parse("sylvan-duelist").expect("additional reference outfit");
    assert!(Wardrobe::ALL.contains(&Wardrobe::Princess));
    assert!(Wardrobe::ALL.contains(&Wardrobe::RoyalPrincess));
    for model in [
        CreatureModel::Human,
        CreatureModel::Orc,
        CreatureModel::Mage,
    ] {
        for build in [0, 2] {
            for volume in [0, 4] {
                let look = Look::with_choice(
                    model,
                    7,
                    Some(Variant {
                        woman: true,
                        build,
                        bust: volume,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                let outfit = Outfit::from_wardrobe(wardrobe, &look);
                let bust = body.bust.unwrap();
                let apex = bust.apex(1.0);
                assert!(
                    bodice_covers(body, outfit.bodice, true, apex),
                    "covered apex {model:?}"
                );
                assert!(
                    !bodice_covers(body, outfit.bodice, true, Vec3::new(0.0, apex.y, apex.z)),
                    "open cleavage"
                );
                assert!(
                    !bodice_covers(
                        body,
                        outfit.bodice,
                        true,
                        Vec3::new(apex.x + bust.radii.x * 0.9, apex.y, apex.z)
                    ),
                    "open lateral bust"
                );
                assert!(
                    !bodice_covers(
                        body,
                        outfit.bodice,
                        true,
                        Vec3::new(0.14 * body.build, 0.18, 0.08)
                    ),
                    "open torso side"
                );
                assert!(matches!(outfit.legs, Legs::Trousers(_)));
                assert!(outfit.skirt.is_some());
                assert!(matches!(outfit.feet, Feet::Boots(_)));
            }
        }
    }
    let man = Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: false,
            ..default()
        }),
    );
    assert_eq!(
        Outfit::from_wardrobe(wardrobe, &man),
        Outfit::from_wardrobe(Wardrobe::Ranger, &man)
    );
}

#[test]
fn sylvan_cutaway_never_folds_back_across_its_front_panels() {
    let body = her();
    let look = Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    let skirt = Outfit::from_wardrobe(Wardrobe::SylvanDuelist, &look)
        .skirt
        .unwrap();
    let cut = skirt_cut(
        body,
        (0.208 + skirt.length * 0.045) * skirt.fullness,
        skirt.length,
        true,
        skirt.cutaway,
    );
    let surface = ClothSurface::new(cut, skirt.length, 12, skirt.split, 7);
    for row in 1..=surface.rows() {
        let t = row as f32 / surface.rows() as f32;
        let mut previous = -1.0;
        for col in 0..32 {
            let p = surface.point(t, col as f32 * PI / 32.0, false);
            let angle = p.x.atan2(p.z);
            assert!(
                angle > previous,
                "folded front at row {row}, column {col}: {angle} <= {previous}"
            );
            previous = angle;
        }
    }
}

#[test]
fn slanted_leather_bands_follow_the_body_and_have_real_width() {
    let body = her();
    let support = torso_shape(body, None, true);
    for tilt in [-0.04, 0.04] {
        let mut parts = Parts::default();
        waist_parts::slanted_band(
            &mut parts.matte,
            body,
            &support,
            Vec2::new(0.075, 0.105),
            Vec2::new(tilt, 0.008),
            Vec3::X,
        );
        let points = garment_positions(parts);
        assert!(points.len() > 500);
        let mut bottom = f32::INFINITY;
        let mut top = f32::NEG_INFINITY;
        for p in points {
            bottom = bottom.min(p.y);
            top = top.max(p.y);
            let gap = support.distance(p - body.torso);
            assert!((0.007..0.012).contains(&gap), "strap clearance {gap}");
        }
        assert!(top - bottom > 0.10, "wide tilted straps, not cords");
    }
    let look = Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    let outfit = Outfit::from_wardrobe(Wardrobe::SylvanDuelist, &look);
    let Arms::Styled(sleeve) = outfit.arms else {
        panic!("draped sleeve")
    };
    assert_eq!(sleeve.cut, SleeveCut::Draped);
    assert!(outfit.arms.bare());
    assert_eq!(outfit.sleeve(Vec3::X), Vec3::X);
    assert_eq!(outfit.forearm(Vec3::X), sleeve.color);
}

#[test]
fn sylvan_panel_waist_stays_below_the_crossed_belts_without_lowering_royal_skirts() {
    let look = Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    let sylvan = Outfit::from_wardrobe(Wardrobe::SylvanDuelist, &look)
        .skirt
        .unwrap();
    let royal = Outfit::from_wardrobe(Wardrobe::RoyalPrincess, &look)
        .skirt
        .unwrap();
    for k in 0..64 {
        let a = k as f32 * TAU / 64.0;
        assert_eq!(sylvan.waist_height(a), 0.04);
        assert_eq!(
            royal.waist_height(a),
            0.135 - 0.075 * a.cos().max(0.0).powi(4)
        );
    }
}

#[test]
fn waist_attachment_distinguishes_fitted_pieces_from_hip_ornaments() {
    for waist in [
        Waist::Sash(Vec3::X),
        Waist::Crossed(Vec3::X),
        Waist::CrossedBelts(Vec3::X),
    ] {
        assert_eq!(waist.attachment(), J::Torso);
    }
    for waist in [
        Waist::Coins(Vec3::X),
        Waist::Jeweled {
            metal: Vec3::X,
            gem: Vec3::Y,
        },
    ] {
        assert_eq!(waist.attachment(), J::Hips);
    }
}

#[test]
fn paired_panels_fit_each_sides_resolved_bust_width() {
    for lean in [1, 2] {
        let body = HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                bust_lean: lean,
                ..default()
            }),
        ));
        let bust = body.bust.unwrap();
        for side in [-1.0, 1.0] {
            let apex = bust.apex(side);
            let (_, radii) = bust.mass(side, 0.0);
            let half = radii.x * 0.5;
            for edge in [-1.0, 1.0] {
                assert!(
                    panel_parts::covers(body, apex + Vec3::X * edge * half * 0.999, 0.5),
                    "lean={lean} side={side}: fitted panel must reach its own width"
                );
                assert!(
                    !panel_parts::covers(body, apex + Vec3::X * edge * half * 1.001, 0.5),
                    "lean={lean} side={side}: panel must not borrow the other side's width"
                );
            }
        }
    }
}

#[test]
fn paired_panel_edges_share_coverage_and_preserve_symmetric_bodies() {
    for lean in [0, 1, 2] {
        let body = HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                bust_lean: lean,
                ..default()
            }),
        ));
        for row in 0..=24 {
            let y = 0.06 + row as f32 / 24.0 * 0.45;
            let left = panel_parts::span(body, -1.0, y, 0.5).unwrap();
            let right = panel_parts::span(body, 1.0, y, 0.5).unwrap();
            if lean == 0 {
                assert_eq!(left, right);
            }
            for (side, (center, half)) in [(-1.0, left), (1.0, right)] {
                for edge in [-1.0, 1.0] {
                    let x = side * (center + edge * (half - 0.002));
                    assert!(
                        panel_parts::covers(body, Vec3::new(x, y, 0.3), 0.5),
                        "binding center must stay on its own panel"
                    );
                }
            }
        }
    }
}

#[test]
fn sweetheart_peaks_fit_both_sides_without_a_center_seam() {
    for lean in [1, 2] {
        let body = HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                bust_lean: lean,
                ..default()
            }),
        ));
        let bust = body.bust.unwrap();
        for side in [-1.0, 1.0] {
            let (center, radii) = bust.mass(side, 0.0);
            assert!(
                (sweetheart_top(body, center.x, 1.0, 0.55) - (center.y + radii.y * 0.55)).abs()
                    < 1e-6,
                "lean={lean} side={side}: neckline peak must fit its own chest slope"
            );
        }
        for front in [0.0, 0.5, 1.0] {
            assert!(
                (sweetheart_top(body, -1e-6, front, 0.55)
                    - sweetheart_top(body, 1e-6, front, 0.55))
                .abs()
                    < 1e-5,
                "left and right edges must meet continuously at the center"
            );
        }
    }
}

#[test]
fn plunge_clearance_is_constrained_by_both_bust_sides() {
    for lean in [1, 2] {
        for volume in [0, 4] {
            let body = HumanoidBody::resolve(&Look::with_choice(
                CreatureModel::Human,
                7,
                Some(Variant {
                    woman: true,
                    bust_lean: lean,
                    bust: volume,
                    ..default()
                }),
            ));
            let bust = body.bust.unwrap();
            let cut = NecklineCut::resolve(
                Neckline::Plunge {
                    depth: 1.0,
                    width: 0.4,
                    wrap: false,
                },
                body,
            )
            .unwrap();
            for side in [-1.0, 1.0] {
                let apex = bust.apex(side);
                let (_, radii) = bust.mass(side, 0.0);
                assert!(
                    cut.edge(apex.y) <= apex.x.abs() - 0.55 * radii.x + 1e-6,
                    "lean={lean} volume={volume} side={side}: opening exceeds fitted clearance"
                );
            }
        }
    }
}

#[test]
fn neckline_binding_tracks_the_coverage_transition_around_the_sides() {
    let body = HumanoidBody::resolve(&Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            bust: 0,
            bust_lean: 1,
            ..default()
        }),
    ));
    let surface = torso_shape(body, None, true);
    let mut parts = Parts::default();
    neckline_parts::bind(&mut parts, body, &surface, 0.55, Vec3::X);
    let points = garment_positions(parts);
    assert!(points.len() > 100);
    for point in points {
        let front = ((point.z + 0.02) / 0.08).clamp(0.0, 1.0);
        let edge = sweetheart_top(body, point.x, front, 0.55);
        assert!(
            (point.y - edge).abs() < 0.012,
            "binding departs from the actual coverage edge near {point:?}: edge={edge}"
        );
    }
}

#[test]
fn traced_upper_edges_are_closed_surface_bound_and_fit_both_cuts() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Mage,
        CreatureModel::Nymph,
    ] {
        let body = HumanoidBody::resolve(&Look::with_choice(
            model,
            7,
            Some(Variant {
                woman: true,
                bust: 4,
                bust_lean: 2,
                ..default()
            }),
        ));
        let surface = torso_shape(body, None, true);
        for dip in [0.1, 0.55] {
            let height = |at: Vec3| sweetheart_top(body, at.x, bodice_front(at), dip);
            let first =
                neckline_parts::edge_point(&surface, 0.0, Vec2::new(0.18, 0.49), &height, 0.0);
            let last =
                neckline_parts::edge_point(&surface, TAU, Vec2::new(0.18, 0.49), &height, 0.0);
            assert!(first.distance(last) < 1e-6, "closed at the center front");
            for i in 0..64 {
                let point = neckline_parts::edge_point(
                    &surface,
                    i as f32 * TAU / 64.0,
                    Vec2::new(0.18, 0.49),
                    &height,
                    0.0,
                );
                assert!(point.is_finite());
                assert!(
                    (point.y - height(point)).abs() < 3e-5,
                    "model={model:?} dip={dip} sample={i} point={point:?} target={}: edge and coverage agree around the torso", height(point)
                );
                assert!(
                    surface.distance(point).abs() < 0.001,
                    "edge stays on emitted support"
                );
            }
        }
    }
}

#[test]
fn raised_neckline_binding_straddles_the_visible_cut() {
    let body = her();
    let surface = torso_shape(body, None, true);
    for dip in [0.1, 0.55] {
        let mut parts = Parts::default();
        neckline_parts::bind(&mut parts, body, &surface, dip, Vec3::X);
        let points = garment_positions(parts);
        for (index, patch) in points.as_chunks::<6>().0.iter().enumerate() {
            let residuals =
                patch.map(|at| at.y - sweetheart_top(body, at.x, bodice_front(at), dip));
            let low = residuals.iter().copied().fold(f32::INFINITY, f32::min);
            let high = residuals.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(low < -0.001 && high > 0.001,
                "dip={dip} patch={index}: raised binding must overlap both sides of cut, residual={low}..{high}");
        }
    }
}
