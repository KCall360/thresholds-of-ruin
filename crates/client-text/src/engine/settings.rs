//! The player's settings for this client. They change how commands are
//! read, never what the game allows, and live only as long as the client.
use tor_protocol::ItemClass;

/// Every class, in the order settings list them.
pub const CLASSES: [ItemClass; 14] = [
    ItemClass::Weapon,
    ItemClass::Armor,
    ItemClass::Potion,
    ItemClass::Food,
    ItemClass::Corpse,
    ItemClass::Tool,
    ItemClass::Amulet,
    ItemClass::Ring,
    ItemClass::Scroll,
    ItemClass::Spellbook,
    ItemClass::Wand,
    ItemClass::Coin,
    ItemClass::Gem,
    ItemClass::Misc,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// What a plain `take all` leaves where it lies. Naming a thing, or
    /// saying `all corpses`, takes it anyway.
    pub all_skips: Vec<ItemClass>,
}

impl Default for Settings {
    /// Bodies stay where they fall: `take all` after a fight means the gear.
    fn default() -> Self {
        Self {
            all_skips: vec![ItemClass::Corpse],
        }
    }
}

/// The defaults, for scenes read without an engine.
pub static DEFAULT: std::sync::LazyLock<Settings> = std::sync::LazyLock::new(Settings::default);

/// A class as players name it, in the plural: "corpses", "food".
pub fn class_name(class: ItemClass) -> &'static str {
    match class {
        ItemClass::Misc => "other things",
        ItemClass::Weapon => "weapons",
        ItemClass::Armor => "armor",
        ItemClass::Potion => "potions",
        ItemClass::Food => "food",
        ItemClass::Corpse => "corpses",
        ItemClass::Tool => "tools",
        ItemClass::Amulet => "amulets",
        ItemClass::Ring => "rings",
        ItemClass::Scroll => "scrolls",
        ItemClass::Spellbook => "spellbooks",
        ItemClass::Wand => "wands",
        ItemClass::Coin => "coins",
        ItemClass::Gem => "gems",
    }
}

/// The class a word names: "corpse", "bodies", "armour", "misc".
pub fn class_of(word: &str) -> Option<ItemClass> {
    Some(match word {
        "weapon" | "weapons" => ItemClass::Weapon,
        "armor" | "armour" | "armors" | "armours" => ItemClass::Armor,
        "potion" | "potions" => ItemClass::Potion,
        "food" | "foods" | "rations" | "ration" => ItemClass::Food,
        "corpse" | "corpses" | "body" | "bodies" | "remains" => ItemClass::Corpse,
        "tool" | "tools" => ItemClass::Tool,
        "amulet" | "amulets" => ItemClass::Amulet,
        "ring" | "rings" => ItemClass::Ring,
        "scroll" | "scrolls" => ItemClass::Scroll,
        "spellbook" | "spellbooks" | "book" | "books" => ItemClass::Spellbook,
        "wand" | "wands" => ItemClass::Wand,
        "coin" | "coins" | "money" => ItemClass::Coin,
        "gem" | "gems" | "jewel" | "jewels" => ItemClass::Gem,
        "misc" | "other" | "others" => ItemClass::Misc,
        _ => return None,
    })
}

/// The classes a list of words names: "corpses and food", "nothing".
pub fn classes(text: &str) -> Result<Vec<ItemClass>, String> {
    let words: Vec<String> = text
        .to_lowercase()
        .replace(',', " ")
        .split_whitespace()
        .filter(|w| !matches!(*w, "and" | "or" | "the" | "all" | "things"))
        .map(str::to_owned)
        .collect();
    if words.is_empty() || matches!(words.as_slice(), [w] if w == "nothing" || w == "none") {
        return Ok(Vec::new());
    }
    let mut found = Vec::new();
    for word in &words {
        let class = class_of(word).ok_or_else(|| {
            format!(
                "I don't know a kind of thing called \"{word}\". Try {}.",
                crate::engine::prose::or_list(
                    &CLASSES
                        .iter()
                        .map(|c| class_name(*c).to_owned())
                        .collect::<Vec<_>>()
                )
            )
        })?;
        if !found.contains(&class) {
            found.push(class);
        }
    }
    found.sort_by_key(|c| CLASSES.iter().position(|o| o == c));
    Ok(found)
}

impl Settings {
    /// What `take all` leaves behind, as a sentence.
    pub fn all_sentence(&self) -> String {
        if self.all_skips.is_empty() {
            "Take all gathers everything it can.".into()
        } else {
            let names: Vec<String> = self
                .all_skips
                .iter()
                .map(|c| class_name(*c).to_owned())
                .collect();
            format!(
                "Take all leaves {} where they lie; name them to take them.",
                crate::engine::prose::and_list(&names)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpses_are_left_by_default() {
        assert_eq!(Settings::default().all_skips, [ItemClass::Corpse]);
    }

    #[test]
    fn classes_are_read_from_plain_words() {
        assert_eq!(
            classes("food, bodies and armour"),
            Ok(vec![ItemClass::Armor, ItemClass::Food, ItemClass::Corpse])
        );
        assert_eq!(classes("nothing"), Ok(vec![]));
        assert!(classes("sandwiches").is_err());
    }
}
