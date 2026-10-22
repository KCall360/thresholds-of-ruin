//! A woman's chest is one form attached across her rib cage, varying by
//! individual about her kind's typical volume and contour. Necklines are cut
//! from it: they plunge down the sternum and keep the fullness covered.
use super::super::anatomy::{chest_wall, BustContour, BustProfile};
use super::*;

fn woman(model: CreatureModel, bust: u8, bust_form: u8, build: u8) -> HumanoidBody {
    HumanoidBody::resolve(&Look::with_choice(
        model,
        7,
        Some(Variant {
            woman: true,
            build,
            bust,
            bust_form,
            ..default()
        }),
    ))
}

/// The contour family's own form index, independent of the kind's typical one.
fn family(contour: BustContour) -> u8 {
    3 + BustContour::FAMILIES
        .iter()
        .position(|&c| c == contour)
        .unwrap() as u8
}

fn front(shape: &crate::sdf::Sculpt, x: f32, y: f32) -> f32 {
    shape.onto(Vec3::new(x, y, 0.6), Vec3::NEG_Z).z
}

/// How far the fullest point stands proud of the rib cage behind it.
fn projection(body: HumanoidBody) -> f32 {
    let bust = body.bust.unwrap();
    let apex = bust.apex(1.0);
    apex.z - chest_wall(body.build, body.shoulders * body.build, apex.x, apex.y)
}

#[test]
fn art_contract_bust_rises_from_the_chest_wall_in_one_slope() {
    for build in [0, 2] {
        for bust in 0..5 {
            for form in 3..3 + BustContour::FAMILIES.len() as u8 {
                let body = woman(CreatureModel::Human, bust, form, build);
                let b = body.bust.unwrap();
                let torso = torso_shape(body, None, true);
                let apex = b.apex(1.0);
                let what = format!("{:?} volume {bust} build {build}", b.contour);
                // From where the slope leaves the wall down to the fullest
                // point the profile only comes forward: no notch where a
                // ball would meet the chest.
                let mut last = f32::MIN;
                for k in 0..=12 {
                    let t = k as f32 / 12.0;
                    let x = b.slope_start.x + (apex.x - b.slope_start.x) * t;
                    let y = b.top + (apex.y - b.top) * t;
                    let z = front(&torso, x, y);
                    assert!(
                        z >= last - 0.002,
                        "{what}: the upper slope dips at {y:.3} ({z:.3} after {last:.3})"
                    );
                    last = last.max(z);
                }
                // The sculpt reaches the landmark garments are cut from.
                let reached = front(&torso, apex.x, apex.y);
                assert!(
                    (reached - apex.z).abs() < 0.012,
                    "{what}: fullest point at {reached:.3}, resolved {:.3}",
                    apex.z
                );
                // Beneath the fullness the form turns back to the fold.
                let under = front(&torso, apex.x, b.fold - 0.03);
                assert!(
                    under < apex.z - 0.025,
                    "{what}: the fullness has no underside ({under:.3} below {:.3})",
                    apex.z
                );
            }
        }
    }
}

#[test]
fn art_contract_bust_volume_and_contour_vary_independently() {
    for form in 3..3 + BustContour::FAMILIES.len() as u8 {
        let steps = (0..5).map(|bust| woman(CreatureModel::Human, bust, form, 1));
        let steps: Vec<_> = steps.collect();
        for pair in steps.windows(2) {
            let (a, b) = (pair[0].bust.unwrap(), pair[1].bust.unwrap());
            assert!(
                projection(pair[1]) > projection(pair[0]),
                "{:?}: a larger volume projects further",
                b.contour
            );
            assert!(
                b.fold <= a.fold,
                "{:?}: a larger volume carries its weight no higher",
                b.contour
            );
        }
    }
    let at = |contour| woman(CreatureModel::Human, 2, family(contour), 1);
    let (broad, projected) = (at(BustContour::Broad), at(BustContour::Projected));
    assert!(broad.bust.unwrap().footprint > projected.bust.unwrap().footprint * 1.25);
    assert!(projection(broad) < projection(projected) * 0.8);
    let (relaxed, upper) = (at(BustContour::Relaxed), at(BustContour::UpperFull));
    assert!(
        relaxed.bust.unwrap().apex(1.0).y < upper.bust.unwrap().apex(1.0).y - 0.01,
        "a relaxed contour carries its fullness lower than an upper-full one"
    );
    assert!(
        upper.bust.unwrap().slope > relaxed.bust.unwrap().slope * 2.0,
        "an upper-full contour rounds its upper slope"
    );
    // Wide-set, shallow forms leave the sternum open rather than forcing a
    // crease between them.
    for contour in [BustContour::Broad, BustContour::Tapered] {
        let body = at(contour);
        let apex = body.bust.unwrap().apex(1.0);
        let torso = torso_shape(body, None, true);
        let midline = front(&torso, 0.0, apex.y);
        let wall = chest_wall(body.build, body.shoulders * body.build, 0.0, apex.y);
        assert!(
            midline - wall < 0.5 * projection(body),
            "{contour:?}: the sternum fills in ({midline:.3} over a wall at {wall:.3})"
        );
    }
}

#[test]
fn art_contract_kinds_have_typical_busts_and_individuals_vary_about_them() {
    assert!(BustProfile::of(Style::Orc).volume > BustProfile::of(Style::Leprechaun).volume + 0.2);
    for (model, style) in [
        (CreatureModel::Orc, Style::Orc),
        (CreatureModel::Leprechaun, Style::Leprechaun),
        (CreatureModel::Nymph, Style::Nymph),
    ] {
        for form in 0..3 {
            let bust = woman(model, 2, form, 1).bust.unwrap();
            let typical = BustProfile::of(style);
            assert_eq!(bust.contour, typical.contour, "{model:?} typical contour");
            assert!((bust.volume - typical.volume).abs() < 1e-6);
        }
    }
    let mut volumes = std::collections::BTreeSet::new();
    let mut contours = Vec::new();
    for seed in 0..400u64 {
        let v = Variant::roll(seed * 7919 + 3, true);
        let look = Look::with_choice(CreatureModel::Human, seed, Some(v));
        let body = HumanoidBody::resolve(&look);
        assert_eq!(body.bust.is_some(), v.woman, "only women have a bust");
        if let Some(bust) = body.bust {
            volumes.insert((bust.volume * 100.0).round() as i32);
            if !contours.contains(&bust.contour) {
                contours.push(bust.contour);
            }
        }
    }
    assert_eq!(volumes.len(), 5, "every volume step occurs");
    assert_eq!(
        contours.len(),
        BustContour::FAMILIES.len(),
        "every contour occurs"
    );
}

#[test]
fn art_contract_plunging_neckline_opens_the_sternum_and_covers_the_fullness() {
    let presets = [Neckline::SPLIT, Neckline::DEEP, Neckline::COURT];
    assert!(NecklineCut::resolve(Neckline::Closed, woman(CreatureModel::Human, 2, 0, 1)).is_none());
    for build in 0..3 {
        for bust in 0..5 {
            for form in 3..3 + BustContour::FAMILIES.len() as u8 {
                let body = woman(CreatureModel::Human, bust, form, build);
                let b = body.bust.unwrap();
                let apex = b.apex(1.0);
                let cuts = presets.map(|n| NecklineCut::resolve(n, body).unwrap());
                let what = format!("{:?} volume {bust} build {build}", b.contour);
                assert!(
                    cuts[0].bottom > cuts[1].bottom && cuts[1].bottom > cuts[2].bottom,
                    "{what}: deeper cuts plunge lower"
                );
                for (cut, preset) in cuts.iter().zip(presets) {
                    let what = format!("{what} {preset:?}");
                    assert!(
                        cut.open(Vec3::new(0.0, cut.bottom + 0.01, 0.1)),
                        "{what}: sternum"
                    );
                    assert!(
                        cut.open(Vec3::new(0.5 * cut.wide, cut.collar - 0.005, 0.1)),
                        "{what}: open across the collarbones"
                    );
                    assert!(
                        !cut.open(Vec3::new(0.0, cut.bottom - 0.005, 0.1)),
                        "{what}: point"
                    );
                    assert!(!cut.open(Vec3::new(0.0, 0.45, -0.1)), "{what}: back");
                    for side in [-1.0, 1.0] {
                        let apex = b.apex(side);
                        let inside = apex - Vec3::X * side * 0.45 * b.radii.x;
                        assert!(
                            !cut.open(apex) && !cut.open(inside),
                            "{what}: the fullness stays covered"
                        );
                    }
                }
                // A deep cut's point lies alongside the fullness, not above it.
                for cut in &cuts[1..] {
                    assert!(
                        cut.bottom < apex.y + 0.5 * b.radii.y,
                        "{what}: a deep cut reaches the bust"
                    );
                }
                assert!(
                    cuts[2].bottom < apex.y,
                    "{what}: a court plunge passes the fullest point"
                );
            }
        }
    }
}

#[test]
fn art_contract_sleeveless_armholes_keep_the_side_of_the_bust_covered() {
    for build in 0..3 {
        for bust in 0..5 {
            for form in 3..3 + BustContour::FAMILIES.len() as u8 {
                let body = woman(CreatureModel::Orc, bust, form, build);
                let b = body.bust.unwrap();
                let (center, radii) = b.mass(1.0, 0.0);
                let rib = 0.183 * body.shoulders * body.build;
                assert!(
                    center.x + radii.x <= rib * 0.93,
                    "{:?} volume {bust} build {build}: the fullness overhangs the rib cage",
                    b.contour
                );
                assert!(
                    armhole_edge(body, center.y, 1.0) >= center.x + radii.x,
                    "{:?} volume {bust} build {build}: the armhole bares her side",
                    b.contour
                );
                assert!(
                    armhole_edge(body, 0.3, 0.0) >= rib,
                    "below the arm the bodice wraps the torso, back and sides"
                );
                assert!(
                    armhole_edge(body, 0.5, 0.0) <= 0.16 * body.shoulders * body.build + 1e-4,
                    "the shoulder strap keeps its width"
                );
            }
        }
    }
}

#[test]
fn art_contract_structured_fronts_fall_from_the_fullness_instead_of_cupping_it() {
    for bust in [0, 2, 4] {
        for form in 3..3 + BustContour::FAMILIES.len() as u8 {
            let body = woman(CreatureModel::Human, bust, form, 1);
            let b = body.bust.unwrap();
            let apex = b.apex(1.0);
            let drop = |shape: &crate::sdf::Sculpt| {
                front(shape, apex.x, apex.y) - front(shape, apex.x, b.fold - 0.02)
            };
            let skin = drop(&torso_shape(body, None, true));
            for material in [TorsoMaterial::Leather, TorsoMaterial::Plate] {
                let worn = drop(&torso_garment(body, material, true));
                assert!(
                    worn < skin * 0.7,
                    "{:?} volume {bust}: the garment tucks into the fold ({worn:.3} vs skin {skin:.3})",
                    b.contour
                );
            }
        }
    }
}

#[test]
fn commoners_choose_a_laced_split_or_a_deep_wrap_by_their_own_roll() {
    let mut cuts = Vec::new();
    for pattern in 0..4 {
        let look = Look::with_choice(
            CreatureModel::Human,
            3,
            Some(Variant {
                woman: true,
                pattern,
                ..default()
            }),
        );
        let outfit = Outfit::from_wardrobe(Wardrobe::Commoner, &look);
        let Neckline::Plunge { wrap, .. } = outfit.bodice.cut else {
            panic!("a woman's commoner tunic plunges");
        };
        assert_eq!(
            outfit.bodice.lacing.is_some(),
            !wrap,
            "only the split V is laced"
        );
        cuts.push(wrap);
    }
    assert!(
        cuts.contains(&true) && cuts.contains(&false),
        "both cuts occur"
    );
    let man = Look::with_choice(CreatureModel::Human, 3, Some(Variant::default()));
    assert_eq!(
        Outfit::from_wardrobe(Wardrobe::Commoner, &man).bodice.cut,
        Neckline::Closed
    );
}

#[test]
fn a_deep_wrap_plunges_like_a_v_and_keeps_the_fullness_covered() {
    for bust in 0..5 {
        for form in 3..3 + BustContour::FAMILIES.len() as u8 {
            let body = woman(CreatureModel::Human, bust, form, 1);
            let b = body.bust.unwrap();
            let cut = NecklineCut::resolve(Neckline::WRAP, body).unwrap();
            assert!(cut.wrap);
            assert!(cut.open(Vec3::new(0.0, cut.bottom + 0.01, 0.1)));
            for side in [-1.0, 1.0] {
                assert!(!cut.open(b.apex(side)), "the fullness stays covered");
            }
        }
    }
}

#[test]
fn her_figure_ranges_from_straighter_to_hourglass() {
    let ratio = |figure: u8| {
        let body = HumanoidBody::resolve(&Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                figure,
                ..default()
            }),
        ));
        body.waist / body.hips_width
    };
    assert!(ratio(0) > ratio(1) && ratio(1) > ratio(2));
    assert!(ratio(0) / ratio(2) > 1.2, "the figures differ visibly");
}

#[test]
fn kinds_lean_toward_their_typical_figure() {
    let waist_to_hips = |model: CreatureModel| {
        let body = HumanoidBody::resolve(&Look::with_choice(
            model,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        ));
        body.waist / body.hips_width
    };
    assert!(waist_to_hips(CreatureModel::Nymph) < waist_to_hips(CreatureModel::Human));
    assert!(waist_to_hips(CreatureModel::Orc) > waist_to_hips(CreatureModel::Human));
}

#[test]
fn a_bikini_cups_each_bust_and_leaves_the_rest_bare() {
    for bust in 0..5 {
        for form in 3..3 + BustContour::FAMILIES.len() as u8 {
            let body = woman(CreatureModel::Human, bust, form, 1);
            let b = body.bust.unwrap();
            for side in [-1.0, 1.0] {
                assert!(
                    bikini_cover(body, b.apex(side)).is_some(),
                    "the fullness is cupped"
                );
            }
            let belly = Vec3::new(0.0, b.fold - 0.06, 0.1);
            let collar = Vec3::new(0.0, 0.48, 0.05);
            assert!(bikini_cover(body, belly).is_none(), "the midriff is bare");
            assert!(
                bikini_cover(body, collar).is_none(),
                "the upper chest is bare"
            );
        }
    }
    let man = HumanoidBody::resolve(&Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant::default()),
    ));
    assert!(bikini_cover(man, Vec3::new(0.07, 0.33, 0.1)).is_none());
}

#[test]
fn an_underbust_corset_ends_just_beneath_each_bust() {
    for bust in 0..5 {
        for form in 3..3 + BustContour::FAMILIES.len() as u8 {
            let body = woman(CreatureModel::Human, bust, form, 1);
            let b = body.bust.unwrap();
            let top = shell_top(body, 0.32, true);
            assert!(top > b.fold && top < b.apex(1.0).y, "beneath the fullness");
        }
    }
    let man = HumanoidBody::resolve(&Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant::default()),
    ));
    assert_eq!(shell_top(man, 0.32, true), 0.32);
    assert_eq!(
        shell_top(woman(CreatureModel::Human, 2, 0, 1), 0.465, false),
        0.465
    );
}
