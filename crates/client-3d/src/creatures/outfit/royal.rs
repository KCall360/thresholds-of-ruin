//! Royal garments composed from wearer-fitted shared parts.
use super::*;

pub(super) fn princess(wearer: &Look) -> Outfit {
    debug_assert!(
        wearer.woman(),
        "catalog must resolve the body-specific recipe"
    );
    let purple = Vec3::new(0.30, 0.095, 0.52);
    let lavender = Vec3::new(0.62, 0.43, 0.76);
    let gem = Vec3::new(0.59, 0.16, 0.85);
    Outfit {
        head: Headwear::Crown { metal: GILT, gem },
        tunic: purple,
        bodice: Bodice {
            straps: Straps::Sweetheart,
            edge: GILT,
            support: Support::Structured,
            ..Bodice::PLAIN
        },
        arms: Arms::Styled(Sleeve {
            color: purple,
            cut: SleeveCut::OffShoulder,
            puff: false,
            puff_lining: None,
            sheer: false,
            flare: Some(GILT),
        }),
        legs: Legs::Bare(Pigment::Skin(1.0)),
        belt: false,
        cuffs: None,
        torso_ornament: Some(TorsoOrnament::Corset { metal: GILT, gem }),
        trimmings: Trimmings {
            armlets: Some(GILT),
            bracelets: Some(GILT),
            earrings: Some((GILT, gem)),
            neck: Some(Neck::Pendant { metal: GILT, gem }),
            waist: Some(Waist::Jeweled { metal: GILT, gem }),
            ..Trimmings::NONE
        },
        skirt: Some(Skirt {
            color: purple,
            length: 0.86,
            fullness: 1.65,
            split: 0.33,
            cutaway: true,
            center_panel: Some(0.23),
            trim: Some(GILT),
            pattern: Pattern::Brocade {
                thread: GILT,
                motif: Motif::Scroll,
            },
            layers: [
                Some(Layer {
                    longer: -0.015,
                    fullness: Some(1.55),
                    trim: Some(GILT * 0.8),
                    ..Layer::petticoat(lavender, false)
                }),
                None,
            ],
            ..Skirt::PLAIN
        }),
        feet: Feet::Shoes(Shoes {
            color: GILT,
            heel: true,
            sandal: true,
            straps: 0.23,
        }),
        ..Outfit::base(wearer)
    }
}
