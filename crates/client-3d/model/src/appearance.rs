//! Selection of cosmetic recipes, separate from mesh construction.
use std::sync::OnceLock;
use tor_client_common::{AssetTable, Palette};
use tor_protocol::{ActorView, ItemClass, ItemView};

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
    if let Some(model) = palette.resolve(items(), item.asset.as_deref()) {
        return *model;
    }
    match item.class {
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
