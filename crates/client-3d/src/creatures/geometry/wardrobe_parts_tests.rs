//! The garment parts outfits are composed from: hems, patterns, how a
//! bodice is held up and how far down it reaches, keyholes and sleeves.
use super::super::outfit::{Hem, Pattern, Sleeve};
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
