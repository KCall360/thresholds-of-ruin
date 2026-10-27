//! Selection of cosmetic recipes, separate from mesh construction.
use std::sync::OnceLock;
use tor_client_common::{AssetTable, Palette};
use tor_protocol::{ActorView, ItemClass, ItemView, ObjectPresentation, ObjectPresentationKind};

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemModel {
    Stairs,
    Placeholder,
    Mace,
    Greatsword,
    ShortSword,
    RingMail,
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
            ("item.armor.mail", ItemModel::RingMail),
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
