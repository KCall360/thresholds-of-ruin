//! Species-native outfit recipes; geometry and composition remain separate.
use super::*;

pub(super) fn compose(l: &Look) -> Outfit {
    let style = l.style;
    let fem = l.v.woman;
    let tunic = match style {
        Style::Orc | Style::Troll => LEATHER * 0.9,
        // A purple waistcoat under the open green coat.
        Style::Leprechaun => Vec3::new(0.34, 0.16, 0.36),
        _ => l.cloth,
    };
    let trousers = [
        Vec3::new(0.25, 0.17, 0.12),
        Vec3::new(0.2, 0.22, 0.16),
        Vec3::new(0.34, 0.28, 0.2),
        Vec3::new(0.16, 0.17, 0.22),
        Vec3::new(0.36, 0.2, 0.14),
    ];
    let pants = match style {
        Style::Hero => trousers[l.v.shade as usize],
        Style::Guardian => l.skin,
        Style::Yeti => l.cloth,
        Style::Zombie => Vec3::new(0.3, 0.26, 0.2),
        Style::Vampire => DARK * 2.0,
        Style::Scout => Vec3::new(0.22, 0.2, 0.15),
        Style::Orc => Vec3::new(0.3, 0.2, 0.13),
        Style::Troll | Style::Nymph => l.skin,
        Style::Leprechaun => Vec3::new(0.2, 0.17, 0.15),
        Style::Medusa => Vec3::new(0.16, 0.1, 0.1),
        Style::Phantom => Vec3::new(0.2, 0.18, 0.24),
        Style::Wraith => DARK * 2.6,
        _ => l.cloth * 0.55 + LEATHER * 0.45,
    };
    // Bare-limbed kinds show skin where others wear sleeves.
    let arms = match style {
        Style::Sentinel => Arms::Plate,
        Style::Hobgoblin
        | Style::Zombie
        | Style::Orc
        | Style::Troll
        | Style::Nymph
        | Style::Medusa
        | Style::Phantom => Arms::Bare,
        Style::Guardian => Arms::Sleeves(l.skin),
        Style::Yeti | Style::Leprechaun => Arms::Sleeves(l.cloth),
        Style::Vampire => Arms::Sleeves(Vec3::new(0.22, 0.12, 0.17)),
        _ => Arms::Sleeves(tunic),
    };
    // Cloth trousers stand off the leg; skin, stockings and mail stay fitted.
    let legs = match style {
        Style::Sentinel => Legs::Plate,
        Style::Hero
        | Style::Scout
        | Style::Common
        | Style::Vampire
        | Style::Hobgoblin
        | Style::Zombie
        | Style::Orc
        | Style::Leprechaun
        | Style::Phantom
        | Style::Wraith => Legs::Trousers(pants),
        Style::Guardian | Style::Troll | Style::Nymph => Legs::Fitted(Pigment::Skin(1.0)),
        _ => Legs::Fitted(Pigment::Fixed(pants)),
    };
    let feet = match style {
        Style::Troll => Feet::Bare(Pigment::Skin(0.88)),
        Style::Nymph => Feet::Bare(Pigment::Skin(1.0)),
        _ => Feet::Boots(Boots {
            color: match style {
                Style::Guardian => l.skin * 0.85,
                Style::Yeti => l.skin,
                Style::Sentinel => STEEL,
                Style::Zombie => l.skin * 0.8,
                Style::Vampire => DARK,
                Style::Leprechaun => LEATHER * 0.9,
                Style::Medusa => Vec3::new(0.4, 0.24, 0.14),
                Style::Wraith => Vec3::new(0.2, 0.16, 0.14),
                _ => LEATHER * 0.75,
            },
            tall: fem && matches!(style, Style::Hero | Style::Scout),
            heel: geometry::heeled_boots(style, fem),
            buckles: matches!(
                style,
                Style::Hero | Style::Scout | Style::Hobgoblin | Style::Orc | Style::Leprechaun
            ),
            cuffed: !matches!(style, Style::Zombie | Style::Yeti),
        }),
    };
    let hands = match style {
        Style::Sentinel => Pigment::Fixed(STEEL),
        Style::Hero | Style::Scout => Pigment::Fixed(LEATHER),
        _ => Pigment::Skin(1.0),
    };
    let cuffs = match style {
        Style::Hero | Style::Scout | Style::Hobgoblin => Some(LEATHER),
        Style::Orc | Style::Troll | Style::Nymph | Style::Phantom => Some(LEATHER * 0.85),
        Style::Leprechaun => Some(CREAM),
        Style::Medusa => Some(l.trim),
        Style::Sentinel => Some(STEEL * 0.85),
        Style::Caster | Style::Vampire | Style::Wraith => Some(l.trim),
        _ => None,
    };
    // A fitted outer garment: a jerkin, vest or coat over the tunic.
    let peplum = match style {
        Style::Hero | Style::Scout => Some((0.08, 0.06)),
        Style::Hobgoblin | Style::Orc => Some((0.06, 0.0)),
        _ => None,
    }
    .map(|(length, split)| {
        let color = LEATHER * if style == Style::Scout { 0.85 } else { 0.9 };
        Skirt {
            color,
            length,
            split,
            trim: Some(color * 0.7),
            cutaway: false,
            ..Skirt::PLAIN
        }
    });
    let shell = |color: Vec3, top: f32, sleeveless: bool, leather: bool| {
        Some(Outer::Shell {
            color,
            top,
            sleeveless,
            leather,
            quilted: false,
            underbust: false,
            opening: match style {
                Style::Hero | Style::Scout | Style::Orc => ShellOpening::ShapedNeck,
                Style::Vampire | Style::Wraith if fem => ShellOpening::PlungingCoat,
                Style::Leprechaun => ShellOpening::OpenCoat,
                _ => ShellOpening::Closed,
            },
            peplum,
        })
    };
    let outer = match style {
        Style::Hero => shell(LEATHER, 0.465, true, true),
        Style::Scout => shell(LEATHER * 0.85, 0.465, true, true),
        Style::Hobgoblin => shell(LEATHER * 0.9, 0.43, true, true),
        Style::Vampire => shell(Vec3::new(0.32, 0.05, 0.07), 0.47, false, false),
        Style::Orc => shell(LEATHER * 0.95, 0.44, true, true),
        Style::Leprechaun => shell(l.cloth, 0.47, false, false),
        Style::Wraith => shell(l.cloth * 1.15, 0.48, false, false),
        Style::Sentinel => Some(Outer::Cuirass),
        _ => None,
    };
    let skirt = match style {
        Style::Guardian | Style::Yeti | Style::Caster => None,
        Style::Sentinel => Some((CREAM * 0.86, 0.26)),
        Style::Zombie => Some((tunic * 0.8, 0.2)),
        Style::Vampire => Some((DARK, 0.3)),
        Style::Orc => Some((LEATHER * 0.8, 0.18)),
        Style::Troll => Some((l.cloth, 0.26)),
        Style::Leprechaun => Some((l.cloth, 0.4)),
        Style::Nymph => Some((l.cloth, 0.2)),
        Style::Medusa => Some((l.cloth, 0.62)),
        Style::Phantom => Some((l.cloth * 0.9, 0.5)),
        Style::Wraith => Some((l.cloth, 0.66)),
        Style::Hero | Style::Scout if fem => Some((tunic * 0.98, 0.19)),
        _ => Some((tunic * 0.98, 0.22)),
    };
    let skirt = skirt.map(|(color, length)| Skirt {
        color,
        length,
        split: match style {
            Style::Hero | Style::Scout => 0.055,
            Style::Leprechaun => 0.07,
            Style::Medusa => 0.05,
            _ => 0.0,
        },
        trim: matches!(style, Style::Medusa | Style::Leprechaun).then_some(l.trim),
        cutaway: geometry::cutaway_coat(style),
        ..Skirt::PLAIN
    });
    let cloak = match style {
        Style::Hero => Some(Cloak::new(l.cloth * 0.62, l.trim)),
        Style::Wraith => Some(Cloak {
            length: 1.2,
            ragged: true,
            ..Cloak::new(l.cloth * 0.8, l.trim * 0.7)
        }),
        Style::Vampire => Some(Cloak {
            length: 1.23,
            ..Cloak::new(DARK, l.trim)
        }),
        _ => None,
    };
    // Scouts and casters wear their hoods up unless pushed back; a
    // wraith's is always up. Hoods sometimes end in a long soft point.
    let hood_down = matches!(style, Style::Caster | Style::Scout) && l.v.pattern >= 2;
    let hood = match style {
        Style::Caster => l.cloth,
        Style::Wraith => l.cloth * 0.85,
        _ => Vec3::new(0.3, 0.42, 0.22),
    };
    let head = match style {
        Style::Sentinel => Headwear::Helm,
        Style::Caster | Style::Scout if hood_down => Headwear::HoodDown(hood),
        Style::Caster | Style::Scout | Style::Wraith => Headwear::Hood {
            color: hood,
            pointed: style == Style::Scout || l.aura == Some(Accent::Ember),
        },
        _ => Headwear::Bare,
    };
    let neckline_edge = if style == Style::Medusa {
        l.trim
    } else {
        tunic * 0.72
    };
    let torso_ornament = match style {
        Style::Hero => Some(TorsoOrnament::LacedFront { cord: BONE }),
        Style::Caster => Some(TorsoOrnament::Placket { color: l.trim }),
        Style::Hobgoblin => Some(TorsoOrnament::RivetedVest {
            strap: LEATHER * 0.75,
            rivets: IRON,
        }),
        Style::Zombie => Some(TorsoOrnament::StitchedFront { thread: BONE * 0.8 }),
        Style::Vampire => Some(TorsoOrnament::HighCollar {
            collar: l.trim,
            pads: Vec3::new(0.16, 0.08, 0.12),
            cravat: Vec3::new(0.88, 0.85, 0.8),
            buttons: Vec3::new(0.9, 0.75, 0.35),
        }),
        Style::Orc | Style::Troll => Some(TorsoOrnament::Harness {
            strap: LEATHER * 0.62,
            ring: Vec3::new(0.82, 0.58, 0.26),
            shoulder_straps: (style == Style::Troll).then_some(LEATHER * 0.7),
        }),
        Style::Leprechaun => Some(TorsoOrnament::StockAndButtons {
            stock: CREAM,
            buttons: GILT,
            belt: LEATHER * 0.6,
        }),
        Style::Nymph => Some(TorsoOrnament::LeafBodice {
            leaves: l.cloth,
            pin: l.trim,
        }),
        Style::Medusa => Some(TorsoOrnament::NeckBandAndDisc { metal: l.trim }),
        Style::Phantom | Style::Wraith => Some(TorsoOrnament::KnottedSash {
            sash: if style == Style::Wraith {
                l.trim
            } else {
                LEATHER * 0.7
            },
            scarf: (style == Style::Wraith).then_some(l.trim * 0.9),
        }),
        _ => None,
    };
    let neck_drape = match style {
        Style::Hero => Some((tunic * 0.75, false, false)),
        Style::Caster => Some((l.cloth * 0.85, false, true)),
        Style::Scout => Some((Vec3::new(0.3, 0.42, 0.22), true, false)),
        Style::Orc => Some((l.trim, true, false)),
        Style::Phantom => Some((l.cloth * 0.85, true, false)),
        _ => None,
    }
    .map(|(color, tail, covered_by_helm)| NeckDrape {
        color,
        tail,
        covered_by_helm,
    });
    Outfit {
        head,
        tunic,
        bodice: Bodice {
            cover: Cover::Full,
            cut: Neckline::fitted(
                fem && !matches!(
                    style,
                    Style::Sentinel
                        | Style::Guardian
                        | Style::Yeti
                        | Style::Hobgoblin
                        | Style::Zombie
                ),
                // Each kind's sheet sets how low her own garment plunges;
                // a mage's robe is the open-front court plunge.
                match style {
                    Style::Vampire | Style::Nymph | Style::Caster => Neckline::COURT,
                    // The ghosts' shrouds cross over, knotted at the waist.
                    Style::Phantom | Style::Wraith => Neckline::WRAP,
                    Style::Leprechaun | Style::Troll => Neckline::SPLIT,
                    _ => Neckline::DEEP,
                },
            ),
            edge: neckline_edge,
            lacing: matches!(style, Style::Hero | Style::Scout).then_some(LEATHER * 0.6),
            support: match style {
                Style::Orc
                | Style::Troll
                | Style::Vampire
                | Style::Leprechaun
                | Style::Sentinel
                | Style::Medusa => Support::Structured,
                Style::Nymph | Style::Zombie => Support::None,
                _ => Support::Loose,
            },
            ..Bodice::PLAIN
        },
        collar_edge: l.trim,
        details: GarmentDetails {
            hip: match style {
                Style::Orc => Some(HipDecoration::Loincloth {
                    cloth: l.trim,
                    edge: GILT * 0.8,
                    ring: Vec3::new(0.82, 0.58, 0.26),
                }),
                Style::Troll => Some(HipDecoration::RaggedApron {
                    cloth: l.cloth,
                    apron: LEATHER * 0.75,
                }),
                Style::Leprechaun => Some(HipDecoration::BucklePouch {
                    metal: GILT,
                    pouch: LEATHER * 0.8,
                }),
                Style::Nymph => Some(HipDecoration::Leaves {
                    cloth: l.cloth,
                    trim: l.trim,
                }),
                Style::Phantom => Some(HipDecoration::TornHem {
                    length: 0.5,
                    cloth: l.cloth * 0.9,
                }),
                Style::Wraith => Some(HipDecoration::TornHem {
                    length: 0.66,
                    cloth: l.cloth,
                }),
                _ => None,
            },
            shoulder_discs: (style == Style::Medusa).then_some(l.trim),
            sleeve_tatters: match style {
                Style::Phantom => Some(l.cloth * 0.9 * 0.95),
                Style::Wraith => Some(l.cloth * 0.95),
                _ => None,
            },
        },
        robe: (style == Style::Caster).then_some(FittedRobe {
            color: l.cloth,
            trim: l.trim,
            fold_seed: l.v.shade,
        }),
        neck_drape,
        armhole_edge: matches!(style, Style::Medusa | Style::Phantom | Style::Nymph)
            .then_some(neckline_edge),
        torso_ornament,
        pouch: matches!(style, Style::Hero | Style::Scout).then_some(Pouch { color: LEATHER }),
        belt: true,
        trimmings: Trimmings::NONE,
        baldric: match style {
            Style::Hero => Some(Baldric {
                color: LEATHER * 0.7,
                reverse: true,
                buckle: true,
            }),
            Style::Scout => Some(Baldric {
                color: LEATHER,
                reverse: false,
                buckle: false,
            }),
            _ => None,
        },
        arms,
        cuffs,
        hands,
        legs,
        feet,
        outer,
        skirt,
        cloak,
    }
}
