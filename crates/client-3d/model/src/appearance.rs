//! Selection of cosmetic recipes, separate from mesh construction.
use std::sync::OnceLock;
use tor_client_common::{AssetTable, Palette};
use tor_protocol::{
    ActorView, ItemClass, ItemView, ObjectPresentation, ObjectPresentationKind, WornView,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CreatureModel {
    Placeholder,
    Humanoid,
    Human,
    Sentinel,
    RedSentinel,
    BlueSentinel,
    Mage,
    RedMage,
    Scout,
    Guardian,
    Hobgoblin,
    Zombie,
    Vampire,
    Yeti,
    Troll,
    Bird,
    Emu,
    Equine,
    Quagga,
    Unicorn,
    BlackUnicorn,
    Centaur,
    Serpent,
    Snake,
    Wisp,
    Ice,
    Orc,
    Leprechaun,
    Nymph,
    Medusa,
    Phantom,
    Wraith,
    Bat,
    Kestrel,
    Griffin,
    Dragon,
    Jabberwock,
    Aquator,
    Xeroc,
    Flytrap,
    Rattlesnake,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemModel {
    Stairs,
    Placeholder,
    Mace,
    Greatsword,
    ShortSword,
    LeatherArmor,
    StuddedLeather,
    RingMail,
    ScaleMail,
    ChainMail,
    SplintMail,
    BandedMail,
    PlateMail,
    Bandage,
    Ration,
    Corpse,
    Seal,
    Coins,
    Tablet,
    Potion,
    Ring,
    Amulet,
    Scroll,
    Spellbook,
    Wand,
    Gem,
    Pick,
    Pouch,
}

fn creatures() -> &'static AssetTable<CreatureModel> {
    static TABLE: OnceLock<AssetTable<CreatureModel>> = OnceLock::new();
    TABLE.get_or_init(|| {
        AssetTable::new([
            ("creature", CreatureModel::Placeholder),
            ("creature.humanoid", CreatureModel::Humanoid),
            ("creature.humanoid.human", CreatureModel::Human),
            ("creature.humanoid.human.warrior", CreatureModel::Human),
            ("creature.humanoid.human.mage", CreatureModel::Mage),
            ("creature.humanoid.sentinel", CreatureModel::Sentinel),
            ("creature.humanoid.sentinel.red", CreatureModel::RedSentinel),
            (
                "creature.humanoid.sentinel.blue",
                CreatureModel::BlueSentinel,
            ),
            ("creature.humanoid.mage", CreatureModel::Mage),
            ("creature.humanoid.mage.red", CreatureModel::RedMage),
            ("creature.humanoid.scout", CreatureModel::Scout),
            ("creature.construct.stone", CreatureModel::Guardian),
            ("creature.goblin", CreatureModel::Hobgoblin),
            ("creature.goblin.hobgoblin", CreatureModel::Hobgoblin),
            ("creature.undead.zombie", CreatureModel::Zombie),
            ("creature.undead.vampire", CreatureModel::Vampire),
            ("creature.ape.yeti", CreatureModel::Yeti),
            ("creature.troll", CreatureModel::Troll),
            ("creature.bird", CreatureModel::Bird),
            ("creature.bird.emu", CreatureModel::Emu),
            ("creature.equine", CreatureModel::Equine),
            ("creature.equine.quagga", CreatureModel::Quagga),
            ("creature.equine.unicorn", CreatureModel::Unicorn),
            ("creature.equine.unicorn.black", CreatureModel::BlackUnicorn),
            ("creature.centaur", CreatureModel::Centaur),
            ("creature.serpent", CreatureModel::Serpent),
            ("creature.serpent.snake", CreatureModel::Snake),
            ("creature.elemental.ember", CreatureModel::Wisp),
            ("creature.elemental.ice", CreatureModel::Ice),
            ("creature.humanoid.orc", CreatureModel::Orc),
            ("creature.fey.leprechaun", CreatureModel::Leprechaun),
            ("creature.fey.nymph", CreatureModel::Nymph),
            ("creature.medusa", CreatureModel::Medusa),
            ("creature.undead.phantom", CreatureModel::Phantom),
            ("creature.undead.wraith", CreatureModel::Wraith),
            ("creature.bat", CreatureModel::Bat),
            ("creature.bird.kestrel", CreatureModel::Kestrel),
            ("creature.griffin", CreatureModel::Griffin),
            ("creature.dragon", CreatureModel::Dragon),
            ("creature.jabberwock", CreatureModel::Jabberwock),
            ("creature.aquator", CreatureModel::Aquator),
            ("creature.xeroc", CreatureModel::Xeroc),
            ("creature.plant.venus.flytrap", CreatureModel::Flytrap),
            ("creature.serpent.rattlesnake", CreatureModel::Rattlesnake),
        ])
    })
}

fn items() -> &'static AssetTable<ItemModel> {
    static TABLE: OnceLock<AssetTable<ItemModel>> = OnceLock::new();
    TABLE.get_or_init(|| {
        AssetTable::new([
            ("item.weapon", ItemModel::ShortSword),
            ("item.weapon.mace", ItemModel::Mace),
            ("item.weapon.greatsword", ItemModel::Greatsword),
            ("item.weapon.dagger", ItemModel::ShortSword),
            ("item.armor", ItemModel::RingMail),
            ("item.armor.leather", ItemModel::LeatherArmor),
            ("item.armor.leather.studded", ItemModel::StuddedLeather),
            ("item.armor.mail", ItemModel::RingMail),
            ("item.armor.mail.ring", ItemModel::RingMail),
            ("item.armor.mail.scale", ItemModel::ScaleMail),
            ("item.armor.mail.chain", ItemModel::ChainMail),
            ("item.armor.plate", ItemModel::PlateMail),
            ("item.armor.plate.splint", ItemModel::SplintMail),
            ("item.armor.plate.banded", ItemModel::BandedMail),
            ("item.bandage", ItemModel::Bandage),
            ("item.food", ItemModel::Ration),
            ("item.corpse", ItemModel::Corpse),
            ("item.seal", ItemModel::Seal),
            ("item.coin", ItemModel::Coins),
            ("item.tablet", ItemModel::Tablet),
            ("item.potion", ItemModel::Potion),
            ("item.ring", ItemModel::Ring),
            ("item.amulet", ItemModel::Amulet),
            ("item.scroll", ItemModel::Scroll),
            ("item.spellbook", ItemModel::Spellbook),
            ("item.wand", ItemModel::Wand),
            ("item.gem", ItemModel::Gem),
            ("item.tool", ItemModel::Pick),
            ("item.pouch", ItemModel::Pouch),
        ])
    })
}

/// Resolve only palette-approved appearance, with dotted-prefix inheritance.
pub fn actor_model(palette: &Palette, actor: &ActorView) -> CreatureModel {
    creature_model(palette, actor.asset.as_deref())
}

pub fn creature_model(palette: &Palette, asset: Option<&str>) -> CreatureModel {
    palette
        .resolve(creatures(), asset)
        .copied()
        .unwrap_or(CreatureModel::Placeholder)
}

/// Class is a disclosed shape fallback; display text never identifies a model.
pub fn item_model(palette: &Palette, item: &ItemView) -> ItemModel {
    item_shape(palette, item.class, item.asset.as_deref())
}

/// Use only the sampled visual recipe, never the real item's class or corpse.
pub fn object_model(palette: &Palette, presentation: &ObjectPresentation) -> ItemModel {
    match presentation.kind {
        ObjectPresentationKind::Item { class } => {
            item_shape(palette, class, presentation.asset.as_deref())
        }
        ObjectPresentationKind::Stairs => ItemModel::Stairs,
    }
}

fn item_shape(palette: &Palette, class: ItemClass, asset: Option<&str>) -> ItemModel {
    if let Some(model) = palette.resolve(items(), asset) {
        return *model;
    }
    match class {
        ItemClass::Weapon => ItemModel::ShortSword,
        ItemClass::Armor => ItemModel::RingMail,
        ItemClass::Potion => ItemModel::Potion,
        ItemClass::Food => ItemModel::Ration,
        ItemClass::Corpse => ItemModel::Corpse,
        ItemClass::Tool => ItemModel::Pick,
        ItemClass::Amulet => ItemModel::Amulet,
        ItemClass::Ring => ItemModel::Ring,
        ItemClass::Scroll => ItemModel::Scroll,
        ItemClass::Spellbook => ItemModel::Spellbook,
        ItemClass::Wand => ItemModel::Wand,
        ItemClass::Coin => ItemModel::Coins,
        ItemClass::Gem => ItemModel::Gem,
        ItemClass::Misc => ItemModel::Placeholder,
    }
}

/// Visible equipment as the renderer places it on a body, from the disclosed
/// worn list. Slots the renderer cannot place are ignored, never moved to
/// another part of the body; an empty list leaves the body's own costume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Wearing {
    pub head: Option<ItemModel>,
    pub body: Option<ItemModel>,
    pub hands: Option<ItemModel>,
    pub feet: Option<ItemModel>,
    pub back: Option<ItemModel>,
    pub shield: Option<ItemModel>,
    /// Main and off hand, by weapon socket occurrence.
    pub weapons: [Option<ItemModel>; 2],
}

impl Wearing {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Resolve each disclosed piece through the palette, as ground items are.
pub fn wearing(palette: &Palette, worn: &[WornView]) -> Wearing {
    let mut result = Wearing::default();
    for piece in worn {
        let model = palette
            .resolve(items(), piece.asset.as_deref())
            .copied()
            .unwrap_or(match piece.class {
                ItemClass::Weapon => ItemModel::ShortSword,
                _ => ItemModel::RingMail,
            });
        let slot = match (piece.slot.as_str(), piece.occurrence) {
            ("head_armor", 0) => &mut result.head,
            ("body_armor", 0) => &mut result.body,
            ("hands_armor", 0) => &mut result.hands,
            ("feet_armor", 0) => &mut result.feet,
            ("cloak", 0) => &mut result.back,
            ("shield", 0) => &mut result.shield,
            ("weapon", n @ (0 | 1)) => &mut result.weapons[usize::from(n)],
            _ => continue,
        };
        *slot = Some(model);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use tor_protocol::{PaletteBody, PaletteUpdate};

    /// The monsters the Rogue scenario names in its dungeon asset list; the
    /// player's own body is named elsewhere in the manifest.
    fn rogue_creature_assets() -> BTreeSet<String> {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scenarios/rogue-exploration/scenario.toml");
        let text = std::fs::read_to_string(&manifest).expect("rogue scenario manifest");
        let dungeon = text
            .split("dungeon = [")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("dungeon asset list");
        dungeon
            .split('"')
            .filter(|token| token.starts_with("creature."))
            .map(str::to_owned)
            .collect()
    }

    fn piece(slot: &str, occurrence: u8, class: ItemClass, asset: &str) -> WornView {
        WornView {
            slot: slot.into(),
            occurrence,
            class,
            appearance: "seen".into(),
            asset: Some(asset.into()),
        }
    }

    #[test]
    fn worn_pieces_resolve_by_slot_and_unplaceable_ones_are_ignored() {
        let mut palette = Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: ["item.armor", "item.weapon"].map(String::from).into(),
            },
        });
        let worn = [
            piece("body_armor", 0, ItemClass::Armor, "item.armor.mail"),
            piece("weapon", 0, ItemClass::Weapon, "item.weapon.mace"),
            piece("weapon", 1, ItemClass::Weapon, "item.weapon"),
            piece("weapon", 2, ItemClass::Weapon, "item.weapon"),
            piece("tail_armor", 0, ItemClass::Armor, "item.armor"),
        ];
        let resolved = wearing(&palette, &worn);
        assert_eq!(
            resolved,
            Wearing {
                body: Some(ItemModel::RingMail),
                // The palette approves only "item.weapon": the mace asset falls
                // back by prefix, never past what the palette allows.
                weapons: [Some(ItemModel::ShortSword), Some(ItemModel::ShortSword)],
                ..Wearing::default()
            },
            "a third weapon socket and an unknown slot are not moved elsewhere"
        );
        assert!(wearing(&palette, &[]).is_empty(), "nothing disclosed, nothing worn");
    }

    #[test]
    fn every_rogue_monster_resolves_to_its_own_model() {
        let assets = rogue_creature_assets();
        assert_eq!(assets.len(), 26, "one asset per Rogue monster: {assets:?}");
        let mut palette = Palette::default();
        palette.apply(&PaletteUpdate {
            revision: 1,
            body: PaletteBody::Full {
                assets: assets.clone(),
            },
        });
        let mut owners: BTreeMap<String, &str> = BTreeMap::new();
        for asset in &assets {
            let model = creature_model(&palette, Some(asset));
            assert!(
                !matches!(model, CreatureModel::Placeholder | CreatureModel::Humanoid),
                "{asset} has no model of its own: {model:?}"
            );
            if let Some(other) = owners.insert(format!("{model:?}"), asset) {
                panic!("{asset} and {other} share the {model:?} model");
            }
        }
    }
}
