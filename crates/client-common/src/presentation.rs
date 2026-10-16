//! Small, built-in presentation vocabulary shared by clients.
//!
//! These are cosmetic defaults, not rules or scenario overrides. Assets resolve
//! only through the connection's current palette. Disclosed names remain the
//! authority for identity and commands; nouns are for otherwise unnamed things.
use std::sync::OnceLock;

use crate::{AssetTable, Palette};

/// A keyed English noun message with explicit grammatical forms. The stable
/// key identifies the whole group for a future locale; assets are never prose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NounForms {
    pub message_key: &'static str,
    pub singular: &'static str,
    pub plural: &'static str,
    pub indefinite: &'static str,
}

/// A client's cosmetic ASCII default. It never determines a gameplay fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsciiAppearance {
    pub glyph: char,
    pub color: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Presentation {
    pub noun: NounForms,
    pub ascii: AsciiAppearance,
}

/// Version-one common entries. Dotted-prefix inheritance uses `AssetTable`;
/// scenario-specific client overrides and appearance traits are not implemented.
pub fn vocabulary() -> &'static AssetTable<Presentation> {
    static TABLE: OnceLock<AssetTable<Presentation>> = OnceLock::new();
    TABLE.get_or_init(|| {
        AssetTable::new([
            (
                "terrain.floor",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.terrain.floor.noun",
                        singular: "flagstone",
                        plural: "flagstones",
                        indefinite: "flagstone",
                    },
                    ascii: AsciiAppearance {
                        glyph: '.',
                        color: 0x7890a2,
                    },
                },
            ),
            (
                "terrain.floor.cave",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.terrain.floor.cave.noun",
                        singular: "packed earth",
                        plural: "packed earth",
                        indefinite: "packed earth",
                    },
                    ascii: AsciiAppearance {
                        glyph: '.',
                        color: 0xa58f6c,
                    },
                },
            ),
            (
                "terrain.floor.marble",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.terrain.floor.marble.noun",
                        singular: "polished marble",
                        plural: "polished marble",
                        indefinite: "polished marble",
                    },
                    ascii: AsciiAppearance {
                        glyph: '.',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "terrain.wall",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.terrain.wall.noun",
                        singular: "dressed stone",
                        plural: "dressed stone",
                        indefinite: "dressed stone",
                    },
                    ascii: AsciiAppearance {
                        glyph: '#',
                        color: 0x7890a2,
                    },
                },
            ),
            (
                "terrain.wall.cave",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.terrain.wall.cave.noun",
                        singular: "rough cave rock",
                        plural: "rough cave rock",
                        indefinite: "rough cave rock",
                    },
                    ascii: AsciiAppearance {
                        glyph: '#',
                        color: 0xa58f6c,
                    },
                },
            ),
            (
                "terrain.wall.marble",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.terrain.wall.marble.noun",
                        singular: "polished marble",
                        plural: "polished marble",
                        indefinite: "polished marble",
                    },
                    ascii: AsciiAppearance {
                        glyph: '#',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "terrain.door",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.terrain.door.noun",
                        singular: "door",
                        plural: "doors",
                        indefinite: "a door",
                    },
                    ascii: AsciiAppearance {
                        glyph: '+',
                        color: 0xc9a46a,
                    },
                },
            ),
            (
                "creature",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.noun",
                        singular: "creature",
                        plural: "creatures",
                        indefinite: "a creature",
                    },
                    ascii: AsciiAppearance {
                        glyph: '&',
                        color: 0xef958c,
                    },
                },
            ),
            (
                "creature.rat",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.rat.noun",
                        singular: "rat",
                        plural: "rats",
                        indefinite: "a rat",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'r',
                        color: 0xa58f6c,
                    },
                },
            ),
            (
                "creature.humanoid",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.humanoid.noun",
                        singular: "humanoid",
                        plural: "humanoids",
                        indefinite: "a humanoid",
                    },
                    ascii: AsciiAppearance {
                        glyph: '@',
                        color: 0xef958c,
                    },
                },
            ),
            (
                "creature.humanoid.human",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.humanoid.human.noun",
                        singular: "human",
                        plural: "humans",
                        indefinite: "a human",
                    },
                    ascii: AsciiAppearance {
                        glyph: '@',
                        color: 0x67d8bd,
                    },
                },
            ),
            (
                "creature.goblin",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.goblin.noun",
                        singular: "goblin",
                        plural: "goblins",
                        indefinite: "a goblin",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'o',
                        color: 0xa6e07a,
                    },
                },
            ),
            (
                "creature.goblin.hobgoblin",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.goblin.hobgoblin.noun",
                        singular: "hobgoblin",
                        plural: "hobgoblins",
                        indefinite: "a hobgoblin",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'o',
                        color: 0xef958c,
                    },
                },
            ),
            (
                "creature.bird",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.bird.noun",
                        singular: "bird",
                        plural: "birds",
                        indefinite: "a bird",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'B',
                        color: 0xf2c14e,
                    },
                },
            ),
            (
                "creature.bird.emu",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.bird.emu.noun",
                        singular: "emu",
                        plural: "emus",
                        indefinite: "an emu",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'B',
                        color: 0xa58f6c,
                    },
                },
            ),
            (
                "creature.serpent",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.serpent.noun",
                        singular: "serpent",
                        plural: "serpents",
                        indefinite: "a serpent",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'S',
                        color: 0xa6e07a,
                    },
                },
            ),
            (
                "creature.serpent.snake",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.serpent.snake.noun",
                        singular: "snake",
                        plural: "snakes",
                        indefinite: "a snake",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'S',
                        color: 0xa6e07a,
                    },
                },
            ),
            (
                "creature.serpent.rattlesnake",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.serpent.rattlesnake.noun",
                        singular: "rattlesnake",
                        plural: "rattlesnakes",
                        indefinite: "a rattlesnake",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'S',
                        color: 0xf2c14e,
                    },
                },
            ),
            (
                "creature.equine",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.equine.noun",
                        singular: "horse-like beast",
                        plural: "horse-like beasts",
                        indefinite: "a horse-like beast",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'u',
                        color: 0xa58f6c,
                    },
                },
            ),
            (
                "creature.equine.quagga",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.equine.quagga.noun",
                        singular: "quagga",
                        plural: "quaggas",
                        indefinite: "a quagga",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'u',
                        color: 0xa58f6c,
                    },
                },
            ),
            (
                "creature.equine.unicorn",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.equine.unicorn.noun",
                        singular: "unicorn",
                        plural: "unicorns",
                        indefinite: "a unicorn",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'u',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "creature.equine.unicorn.black",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.equine.unicorn.black.noun",
                        singular: "black unicorn",
                        plural: "black unicorns",
                        indefinite: "a black unicorn",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'u',
                        color: 0x869ba9,
                    },
                },
            ),
            (
                "creature.centaur",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.centaur.noun",
                        singular: "centaur",
                        plural: "centaurs",
                        indefinite: "a centaur",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'C',
                        color: 0xa58f6c,
                    },
                },
            ),
            (
                "creature.ape",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.ape.noun",
                        singular: "ape",
                        plural: "apes",
                        indefinite: "an ape",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'Y',
                        color: 0xa58f6c,
                    },
                },
            ),
            (
                "creature.ape.yeti",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.ape.yeti.noun",
                        singular: "yeti",
                        plural: "yetis",
                        indefinite: "a yeti",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'Y',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "creature.troll",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.troll.noun",
                        singular: "troll",
                        plural: "trolls",
                        indefinite: "a troll",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'T',
                        color: 0xa6e07a,
                    },
                },
            ),
            (
                "creature.elemental",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.elemental.noun",
                        singular: "elemental",
                        plural: "elementals",
                        indefinite: "an elemental",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'E',
                        color: 0x7fd4f0,
                    },
                },
            ),
            (
                "creature.elemental.ice",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.elemental.ice.noun",
                        singular: "ice creature",
                        plural: "ice creatures",
                        indefinite: "an ice creature",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'E',
                        color: 0x8cbafa,
                    },
                },
            ),
            (
                "creature.jabberwock",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.jabberwock.noun",
                        singular: "jabberwock",
                        plural: "jabberwocks",
                        indefinite: "a jabberwock",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'J',
                        color: 0xd98cf0,
                    },
                },
            ),
            (
                "creature.undead",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.undead.noun",
                        singular: "undead creature",
                        plural: "undead creatures",
                        indefinite: "an undead creature",
                    },
                    ascii: AsciiAppearance {
                        glyph: '&',
                        color: 0x869ba9,
                    },
                },
            ),
            (
                "creature.undead.zombie",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.undead.zombie.noun",
                        singular: "zombie",
                        plural: "zombies",
                        indefinite: "a zombie",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'Z',
                        color: 0xa6e07a,
                    },
                },
            ),
            (
                "creature.undead.vampire",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.undead.vampire.noun",
                        singular: "vampire",
                        plural: "vampires",
                        indefinite: "a vampire",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'V',
                        color: 0xd98cf0,
                    },
                },
            ),
            (
                "creature.undead.wraith",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.creature.undead.wraith.noun",
                        singular: "wraith",
                        plural: "wraiths",
                        indefinite: "a wraith",
                    },
                    ascii: AsciiAppearance {
                        glyph: 'W',
                        color: 0x869ba9,
                    },
                },
            ),
            (
                "item",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.noun",
                        singular: "object",
                        plural: "objects",
                        indefinite: "an object",
                    },
                    ascii: AsciiAppearance {
                        glyph: '*',
                        color: 0xedc579,
                    },
                },
            ),
            (
                "item.weapon",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.weapon.noun",
                        singular: "weapon",
                        plural: "weapons",
                        indefinite: "a weapon",
                    },
                    ascii: AsciiAppearance {
                        glyph: ')',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "item.weapon.mace",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.weapon.mace.noun",
                        singular: "mace",
                        plural: "maces",
                        indefinite: "a mace",
                    },
                    ascii: AsciiAppearance {
                        glyph: ')',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "item.armor",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.armor.noun",
                        singular: "armor",
                        plural: "armor",
                        indefinite: "armor",
                    },
                    ascii: AsciiAppearance {
                        glyph: '[',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "item.armor.mail",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.armor.mail.noun",
                        singular: "mail",
                        plural: "mail",
                        indefinite: "mail",
                    },
                    ascii: AsciiAppearance {
                        glyph: '[',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "item.bandage",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.bandage.noun",
                        singular: "bandage",
                        plural: "bandages",
                        indefinite: "a bandage",
                    },
                    ascii: AsciiAppearance {
                        glyph: '*',
                        color: 0xc9cbd4,
                    },
                },
            ),
            (
                "item.food",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.food.noun",
                        singular: "food",
                        plural: "food",
                        indefinite: "food",
                    },
                    ascii: AsciiAppearance {
                        glyph: '%',
                        color: 0xedc579,
                    },
                },
            ),
            (
                "item.food.ration",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.food.ration.noun",
                        singular: "food ration",
                        plural: "food rations",
                        indefinite: "a food ration",
                    },
                    ascii: AsciiAppearance {
                        glyph: '%',
                        color: 0xedc579,
                    },
                },
            ),
            (
                "item.amulet",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.amulet.noun",
                        singular: "amulet",
                        plural: "amulets",
                        indefinite: "an amulet",
                    },
                    ascii: AsciiAppearance {
                        glyph: '"',
                        color: 0xf2c14e,
                    },
                },
            ),
            (
                "item.potion",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.potion.noun",
                        singular: "potion",
                        plural: "potions",
                        indefinite: "a potion",
                    },
                    ascii: AsciiAppearance {
                        glyph: '!',
                        color: 0xd98cf0,
                    },
                },
            ),
            (
                "item.coin",
                Presentation {
                    noun: NounForms {
                        message_key: "presentation.item.coin.noun",
                        singular: "coin",
                        plural: "coins",
                        indefinite: "a coin",
                    },
                    ascii: AsciiAppearance {
                        glyph: '$',
                        color: 0xf2c14e,
                    },
                },
            ),
        ])
    })
}

/// Resolve only disclosed, currently forecast assets. Unknown assets may use a
/// known dotted parent; absent, unforecast or stale assets use caller fallbacks.
pub fn resolve(palette: &Palette, asset: Option<&str>) -> Option<&'static Presentation> {
    palette.resolve(vocabulary(), asset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::{PaletteBody, PaletteUpdate};

    #[test]
    fn vocabulary_is_shared_keyed_and_falls_back_only_inside_a_current_palette() {
        let asset = "creature.equine.quagga.unfamiliar";
        let mut palette = Palette::default();
        assert!(resolve(&palette, Some(asset)).is_none());
        assert!(resolve(&palette, Some("creature.jabberwock")).is_none());
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: [
                    asset.to_owned(),
                    "unknown.thing".into(),
                    "creature.jabberwock".into(),
                ]
                .into(),
            },
        });
        let entry = resolve(&palette, Some(asset)).unwrap();
        let jabberwock = resolve(&palette, Some("creature.jabberwock")).unwrap();
        assert_eq!(jabberwock.ascii.glyph, 'J');
        assert_eq!(jabberwock.noun.singular, "jabberwock");
        assert_eq!(jabberwock.noun.plural, "jabberwocks");
        assert_eq!(jabberwock.noun.indefinite, "a jabberwock");
        assert_eq!(
            jabberwock.noun.message_key,
            "presentation.creature.jabberwock.noun"
        );
        assert_eq!(entry.ascii.glyph, 'u');
        assert_eq!(entry.noun.singular, "quagga");
        assert_eq!(entry.noun.plural, "quaggas");
        assert_eq!(entry.noun.indefinite, "a quagga");
        assert_eq!(
            entry.noun.message_key,
            "presentation.creature.equine.quagga.noun"
        );
        assert!(resolve(&palette, None).is_none());
        assert!(resolve(&palette, Some("creature.equine")).is_none());
        assert!(resolve(&palette, Some("unknown.thing")).is_none());
        assert!(palette.apply(&PaletteUpdate {
            revision: 3,
            body: PaletteBody::Delta {
                base: 2,
                added: Default::default(),
                removed: Default::default(),
            }
        }));
        assert!(resolve(&palette, Some(asset)).is_none());
        palette.apply(&PaletteUpdate {
            revision: 4,
            body: PaletteBody::Full {
                assets: [asset.to_owned()].into(),
            },
        });
        assert_eq!(resolve(&palette, Some(asset)).unwrap().ascii.glyph, 'u');
        assert!(resolve(&palette, Some("creature.jabberwock")).is_none());
    }
}
