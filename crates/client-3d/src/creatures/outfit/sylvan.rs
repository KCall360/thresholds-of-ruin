//! Open-front forest garments assembled from shared fitted parts.
use super::*;

pub(super) fn duelist(wearer: &Look) -> Outfit {
    debug_assert!(
        wearer.woman(),
        "catalog must resolve the body-specific recipe"
    );
    let green = Vec3::new(0.065, 0.22, 0.15);
    let dark = Vec3::new(0.10, 0.075, 0.065);
    let gold = GILT * 0.8;
    Outfit {
        tunic: green,
        bodice: Bodice {
            straps: Straps::Panels { width: 0.5 },
            edge: gold,
            pattern: Pattern::Brocade {
                thread: gold,
                motif: Motif::Knot,
            },
            ..Bodice::PLAIN
        },
        arms: Arms::Styled(Sleeve {
            color: green,
            cut: SleeveCut::Draped,
            puff: false,
            puff_lining: None,
            sheer: false,
            flare: Some(green),
        }),
        legs: Legs::Trousers(dark),
        feet: Feet::Boots(Boots {
            color: dark,
            tall: true,
            heel: true,
            buckles: true,
            cuffed: true,
        }),
        belt: true,
        trimmings: Trimmings {
            earrings: Some((gold, gold)),
            neck: Some(Neck::Pendant {
                metal: gold,
                gem: gold,
            }),
            waist: Some(Waist::CrossedBelts(dark)),
            ..Trimmings::NONE
        },
        skirt: Some(Skirt {
            color: green,
            length: 0.80,
            fullness: 1.05,
            split: 0.18,
            cutaway: true,
            center_panel: Some(0.16),
            waistline: Some(0.04),
            trim: Some(gold),
            pattern: Pattern::Brocade {
                thread: gold,
                motif: Motif::Scroll,
            },
            ..Skirt::PLAIN
        }),
        cloak: Some(Cloak::new(green, green * 0.8)),
        ..Outfit::base(wearer)
    }
}
