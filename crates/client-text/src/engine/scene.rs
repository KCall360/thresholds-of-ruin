//! One disclosed view, interpreted for language: what is here, what it is
//! called, and where it is from the character's point of view.
use std::collections::{BTreeMap, BTreeSet};

use tor_client_common::{surfaces, Palette};
use tor_protocol::*;

use crate::{adventure, safe};

/// A referent's identity, stable across views while the thing stays known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Item(ItemTarget),
    Actor(ActorTarget),
    Door(DoorTarget),
    Surface(Surface),
    /// A feature of the place: a way out, the exit, the dark beyond.
    Feature(u8),
    Me,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Surface {
    Floor,
    Walls,
    Ceiling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Thing,
    Figure,
    Door,
    Surface,
    /// Part of the place itself: a passage, the exit.
    Feature,
    Me,
}

/// Something the player can refer to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Referent {
    pub key: Key,
    pub kind: Kind,
    /// Singular name as disclosed, or the client's word for an unnamed thing.
    pub name: String,
    /// The words that name it: its name's words and synonyms.
    pub words: Vec<String>,
    /// The nouns that can head a phrase naming it; the last word of its name
    /// first.
    pub heads: Vec<String>,
    pub description: String,
    /// Items: the stack size. Others: 1.
    pub quantity: u64,
    pub carried: bool,
    /// Within reach without moving: an item underfoot, a door the server
    /// reports reachable, a figure next to the character.
    pub reachable: bool,
    /// Offset from the character's eye; `None` for carried things, the self
    /// and surfaces.
    pub position: Option<Position>,
    /// Indistinguishable things share it: the player can't tell them apart.
    pub identity: String,
    /// Doors: whether open.
    pub open: Option<bool>,
    /// Items: their disclosed class.
    pub class: Option<ItemClass>,
    /// Carried items in use: how, as the rules say it ("wielded").
    pub equipped: Option<String>,
    /// Figures: whether hostile to the character, when disclosed.
    pub hostile: Option<bool>,
    /// Figures: how many cells tall their body stands.
    pub height: u8,
    /// Features: the way they lead.
    pub way: Option<Direction>,
}

impl Referent {
    pub fn is(&self, kind: Kind) -> bool {
        self.kind == kind
    }

    /// "the copper token", "the two copper tokens", "you".
    pub fn the(&self) -> String {
        match self.kind {
            Kind::Me => "yourself".into(),
            Kind::Surface => match self.key {
                Key::Surface(Surface::Walls) => "the walls".into(),
                _ => format!("the {}", self.name),
            },
            _ => super::prose::counted_definite(self.quantity, &self.name),
        }
    }

    /// "a copper token", "two copper tokens".
    pub fn a(&self) -> String {
        match self.kind {
            Kind::Me | Kind::Surface => self.the(),
            _ => super::prose::counted(self.quantity, &self.name),
        }
    }

    /// The direction or place of something not carried: "at your feet",
    /// "to the east".
    pub fn whereabouts(&self) -> Option<String> {
        if self.carried {
            return None;
        }
        self.position.map(whereabouts)
    }
}

/// Synonyms for a name's last word: ordinary English, not any one
/// scenario's vocabulary.
fn synonyms(head: &str) -> &'static [&'static str] {
    match head {
        "corpse" => &["body", "remains", "carcass", "cadaver"],
        "token" => &["disc", "disk"],
        "tablet" => &["slab"],
        "gate" | "portcullis" | "hatch" | "grate" => &["door"],
        "ration" | "rations" => &["food", "meal"],
        "bandage" | "bandages" => &["dressing", "cloth"],
        _ => &[],
    }
}

/// Whether a word is a noun the game knows, whether or not anything it
/// names is in sight: "corpse", "sword", "door".
pub fn known_noun(word: &str) -> bool {
    COMPOUND_HEADS.contains(&word)
        || !synonyms(word).is_empty()
        || super::settings::class_of(word).is_some()
        || [
            "thing", "item", "object", "figure", "creature", "someone", "monster", "enemy", "foe",
            "door", "floor", "wall", "walls", "ceiling", "passage", "exit", "stairs", "archway",
            "darkness", "token", "tablet", "key", "seal", "lamp", "ration", "bandage", "corpse",
            "body",
        ]
        .contains(&word)
}

/// Nouns that end longer words: a greatsword is a sword, chainmail is mail.
const COMPOUND_HEADS: &[&str] = &[
    "sword", "axe", "bow", "mail", "shield", "helm", "staff", "spear", "knife", "hammer", "club",
    "blade", "book", "plate", "coat", "cloak", "boots", "gloves", "mace", "dagger",
];

/// Nouns every thing of a class answers to.
fn class_heads(class: ItemClass) -> &'static [&'static str] {
    match class {
        ItemClass::Weapon => &["weapon"],
        ItemClass::Armor => &["armor", "armour"],
        ItemClass::Potion => &["potion"],
        ItemClass::Food => &["food"],
        ItemClass::Corpse => &["corpse", "body", "remains", "carcass", "cadaver"],
        ItemClass::Tool => &["tool"],
        ItemClass::Amulet => &["amulet", "necklace"],
        ItemClass::Ring => &["ring"],
        ItemClass::Scroll => &["scroll"],
        ItemClass::Spellbook => &["book", "spellbook", "tome"],
        ItemClass::Wand => &["wand"],
        ItemClass::Coin => &["coin", "money"],
        ItemClass::Gem => &["gem", "jewel"],
        ItemClass::Misc => &[],
    }
}

fn naming(name: &str, kind: Kind, class: Option<ItemClass>) -> (Vec<String>, Vec<String>) {
    let mut words: Vec<String> = name.split_whitespace().map(str::to_lowercase).collect();
    let mut heads = Vec::new();
    if let Some(last) = words.last().cloned() {
        heads.push(last.clone());
        heads.extend(synonyms(&last).iter().map(|s| (*s).to_owned()));
    }
    for word in &words {
        for head in COMPOUND_HEADS {
            if word.len() >= head.len() + 3 && word.ends_with(head) {
                heads.push((*head).to_owned());
            }
        }
    }
    if let Some(class) = class {
        heads.extend(class_heads(class).iter().map(|s| (*s).to_owned()));
    }
    let generic: &[&str] = match kind {
        Kind::Thing => &["thing", "item", "object"],
        Kind::Figure => &["figure", "creature", "someone", "monster", "enemy", "foe"],
        Kind::Door => &["door"],
        Kind::Surface | Kind::Feature | Kind::Me => &[],
    };
    heads.extend(generic.iter().map(|s| (*s).to_owned()));
    let mut unique: Vec<String> = Vec::new();
    for head in heads {
        if !unique.contains(&head) {
            unique.push(head);
        }
    }
    let heads = unique;
    for head in &heads {
        if !words.contains(head) {
            words.push(head.clone());
        }
    }
    (words, heads)
}

pub fn distance(p: Position) -> u64 {
    u64::from(p.x.unsigned_abs()) + u64::from(p.y.unsigned_abs()) + u64::from(p.z.unsigned_abs())
}

/// The compass bearing of an offset, or up and down when it's mostly
/// vertical. Diagonal sectors cover ratios from 1:2 to 2:1.
pub fn bearing(p: Position) -> Option<Direction> {
    if p.z != 0 && p.x == 0 && p.y == 0 {
        return Some(if p.z > 0 {
            Direction::Up
        } else {
            Direction::Down
        });
    }
    let (x, y) = (u64::from(p.x.unsigned_abs()), u64::from(p.y.unsigned_abs()));
    if x == 0 && y == 0 {
        None
    } else if x * 2 >= y && y * 2 >= x {
        Some(match (p.x > 0, p.y > 0) {
            (true, false) => Direction::NorthEast,
            (true, true) => Direction::SouthEast,
            (false, true) => Direction::SouthWest,
            (false, false) => Direction::NorthWest,
        })
    } else if x > y {
        Some(if p.x > 0 {
            Direction::East
        } else {
            Direction::West
        })
    } else {
        Some(if p.y > 0 {
            Direction::South
        } else {
            Direction::North
        })
    }
}

pub fn direction_name(d: Direction) -> &'static str {
    adventure::direction_name(d)
}

/// "at your feet", "to the east", "above you".
pub fn whereabouts(p: Position) -> String {
    match bearing(p) {
        None => "at your feet".into(),
        Some(Direction::Up) => "above you".into(),
        Some(Direction::Down) => "below you".into(),
        Some(d) => format!("to the {}", direction_name(d)),
    }
}

/// The disclosed view as referents.
pub struct Scene<'a> {
    pub state: &'a StateView,
    pub palette: &'a Palette,
    /// Places remembered, for the atmosphere of the place in view.
    pub places: &'a crate::narrative::Places,
    /// How the player has asked for commands to be read.
    pub settings: &'a super::settings::Settings,
    pub referents: Vec<Referent>,
}

impl<'a> Scene<'a> {
    pub fn new(state: &'a StateView, palette: &'a Palette) -> Self {
        Self::remembering(state, palette, &crate::narrative::NO_PLACES)
    }

    /// The scene, with the places the engine remembers.
    pub fn remembering(
        state: &'a StateView,
        palette: &'a Palette,
        places: &'a crate::narrative::Places,
    ) -> Self {
        let o = &state.observation;
        let mut referents = Vec::new();
        for item in &o.inventory {
            let mut r = thing(item, true, true, None);
            r.equipped = equipped(o, item.id);
            referents.push(r);
        }
        let mut ground: Vec<&GroundItemView> = Vec::new();
        let mut ground_indices = BTreeMap::new();
        for g in &o.ground_items {
            // Preserve disclosure order: opaque references carry no ordering semantics.
            // A portal can show one item twice; the nearest view counts.
            let index = *ground_indices.entry(g.item.id).or_insert_with(|| {
                ground.push(g);
                ground.len() - 1
            });
            let entry = &mut ground[index];
            if (!g.reachable, distance(g.position)) < (!entry.reachable, distance(entry.position)) {
                *entry = g;
            }
        }
        for g in ground {
            referents.push(thing(&g.item, false, g.reachable, Some(g.position)));
        }
        for figure in figures(o, palette) {
            referents.push(figure);
        }
        let mut doors = BTreeSet::new();
        for cell in &o.visible_cells {
            let Some(door) = &cell.door else { continue };
            if !doors.insert(door.id) {
                continue;
            }
            let name = if door.name.trim().is_empty() {
                "door".to_owned()
            } else {
                safe(&door.name).to_lowercase()
            };
            let (words, heads) = naming(&name, Kind::Door, None);
            referents.push(Referent {
                key: Key::Door(door.id),
                kind: Kind::Door,
                identity: format!("door:{}", door.id),
                name,
                words,
                heads,
                description: safe(&door.description),
                quantity: 1,
                carried: false,
                reachable: door.reachable,
                position: Some(cell.position),
                open: Some(door.open),
                class: None,
                equipped: None,
                hostile: None,
                height: 0,
                way: None,
            });
        }
        let roles = surfaces::roles_by(&o.visible_cells, |cell| adventure::surface(palette, cell));
        for (surface, name, words, seen) in [
            (
                Surface::Floor,
                "floor",
                &["floor", "ground"][..],
                // Raw diagnostic regions show their floor on the open cells.
                !roles.floors.is_empty()
                    || o.visible_cells
                        .iter()
                        .any(|c| !c.wall && (c.asset.is_some() || !c.material.is_empty())),
            ),
            (
                Surface::Walls,
                "walls",
                &["wall", "walls"][..],
                !roles.walls.is_empty(),
            ),
            (
                Surface::Ceiling,
                "ceiling",
                &["ceiling", "roof"][..],
                !roles.ceilings.is_empty(),
            ),
        ] {
            if seen {
                referents.push(Referent {
                    key: Key::Surface(surface),
                    kind: Kind::Surface,
                    identity: format!("surface:{name}"),
                    name: name.into(),
                    words: words.iter().map(|w| (*w).to_owned()).collect(),
                    heads: words.iter().map(|w| (*w).to_owned()).collect(),
                    description: String::new(),
                    quantity: 1,
                    carried: false,
                    reachable: true,
                    position: None,
                    open: None,
                    class: None,
                    equipped: None,
                    hostile: None,
                    height: 0,
                    way: None,
                });
            }
        }
        referents.extend(features(state));
        referents.push(Referent {
            key: Key::Me,
            kind: Kind::Me,
            identity: "me".into(),
            name: "yourself".into(),
            words: ["me", "myself", "self", "yourself"]
                .map(String::from)
                .to_vec(),
            heads: ["me", "myself", "self", "yourself"]
                .map(String::from)
                .to_vec(),
            description: String::new(),
            quantity: 1,
            carried: false,
            reachable: true,
            position: None,
            open: None,
            class: None,
            equipped: None,
            hostile: None,
            height: 0,
            way: None,
        });
        // Things, doors and figures in sight can be named by where they lie:
        // "the east door", "the rat to the north".
        for r in &mut referents {
            if let Some(d) = r.position.and_then(bearing) {
                r.words.push(direction_name(d).to_owned());
            }
        }
        Self {
            state,
            palette,
            places,
            settings: &super::settings::DEFAULT,
            referents,
        }
    }

    pub fn get(&self, key: Key) -> Option<&Referent> {
        self.referents.iter().find(|r| r.key == key)
    }

    pub fn of(&self, kind: Kind) -> impl Iterator<Item = &Referent> {
        self.referents.iter().filter(move |r| r.kind == kind)
    }

    pub fn ready(&self) -> bool {
        self.state.observation.ready
    }

    /// Visible figures other than the character.
    pub fn figure_ids(&self) -> std::collections::BTreeSet<ActorTarget> {
        self.referents
            .iter()
            .filter_map(|r| match r.key {
                Key::Actor(id) => Some(id),
                _ => None,
            })
            .collect()
    }

    /// The visible cell a journey to this referent should end on, if any.
    pub fn approach(&self, key: Key) -> Option<String> {
        let o = &self.state.observation;
        let open = |c: &&CellView| !c.wall && c.door.as_ref().is_none_or(|d| d.open);
        match key {
            Key::Item(id) => {
                let item = o
                    .ground_items
                    .iter()
                    .filter(|g| g.item.id == id)
                    .min_by_key(|g| distance(g.position))?;
                o.visible_cells
                    .iter()
                    .find(|c| c.position == item.position && !c.wall)
                    .map(|c| c.key.clone())
            }
            Key::Door(id) => {
                let door = o
                    .visible_cells
                    .iter()
                    .find_map(|c| c.door.as_ref().filter(|d| d.id == id))?;
                o.visible_cells
                    .iter()
                    .filter(|c| door.approaches.contains(&c.key))
                    .filter(open)
                    .min_by_key(|c| (distance(c.position), &c.key))
                    .map(|c| c.key.clone())
            }
            Key::Actor(id) => {
                let body: Vec<Position> = o
                    .visible_actors
                    .iter()
                    .filter(|a| a.id == id)
                    .map(|a| a.position)
                    .collect();
                let base = *body.iter().min_by_key(|p| (p.z, distance(**p)))?;
                let occupied = |p: Position| o.visible_actors.iter().any(|a| a.position == p);
                o.visible_cells
                    .iter()
                    .filter(open)
                    .filter(|c| {
                        c.position.z == base.z
                            && (c.position.x - base.x).abs() <= 1
                            && (c.position.y - base.y).abs() <= 1
                            && c.position != base
                            && !occupied(c.position)
                    })
                    .min_by_key(|c| (distance(c.position), &c.key))
                    .map(|c| c.key.clone())
            }
            // The exit, where it's in sight; ways out are taken by direction.
            Key::Feature(_) => {
                let at = self.get(key)?.position?;
                o.visible_cells
                    .iter()
                    .find(|c| c.position == at && !c.wall)
                    .map(|c| c.key.clone())
            }
            Key::Surface(_) | Key::Me => None,
        }
    }
}

fn thing(item: &ItemView, carried: bool, reachable: bool, position: Option<Position>) -> Referent {
    let name = if item.name.trim().is_empty() {
        "thing".to_owned()
    } else {
        safe(&item.name).to_lowercase()
    };
    let (words, heads) = naming(&name, Kind::Thing, Some(item.class));
    Referent {
        key: Key::Item(item.id),
        kind: Kind::Thing,
        identity: format!(
            "thing:{name}|{}|{}|{carried}",
            item.description, item.appearance
        ),
        name,
        words,
        heads,
        description: safe(&item.description),
        quantity: item.quantity,
        carried,
        reachable: carried || reachable,
        position,
        open: None,
        class: Some(item.class),
        equipped: None,
        hostile: None,
        height: 0,
        way: None,
    }
}

/// How a carried item is in use, if it is: its slot's phrase, "wielded".
fn equipped(o: &Observation, item: ItemTarget) -> Option<String> {
    let interactions = o.interactions.as_ref()?;
    let slot = interactions
        .inventory
        .iter()
        .find(|i| i.item == item)?
        .equipped_slot?;
    Some(
        interactions
            .slots
            .get(usize::from(slot))
            .map(|s| safe(&s.equipped_phrase))
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| "in use".into()),
    )
}

/// One referent per visible actor other than the character, however many
/// cells its body covers. Its position is its lowest, nearest cell.
fn figures(o: &Observation, palette: &Palette) -> Vec<Referent> {
    let mut bodies: Vec<Vec<&ActorView>> = Vec::new();
    let mut body_indices = BTreeMap::new();
    for actor in o.visible_actors.iter().filter(|a| a.id != o.self_target) {
        let index = *body_indices.entry(actor.id).or_insert_with(|| {
            bodies.push(Vec::new());
            bodies.len() - 1
        });
        bodies[index].push(actor);
    }
    let mut result: Vec<Referent> = bodies
        .into_iter()
        .map(|cells| {
            let id = cells[0].id;
            let base = cells
                .iter()
                .min_by_key(|a| (a.position.z, distance(a.position)))
                .expect("a visible actor has a cell");
            let name = if base.name.trim().is_empty() {
                palette
                    .resolve(adventure::words(), base.asset.as_deref())
                    .map_or("figure", |w| *w)
                    .to_owned()
            } else {
                safe(&base.name).to_lowercase()
            };
            let reachable = cells.iter().any(|a| {
                a.position.x.abs() <= 1 && a.position.y.abs() <= 1 && a.position.z.abs() <= 1
            });
            let (words, heads) = naming(&name, Kind::Figure, None);
            let hostile = o
                .combat
                .as_ref()
                .and_then(|c| c.actors.iter().find(|a| a.actor == id))
                .map(|a| a.hostile);
            let height = cells
                .iter()
                .map(|a| a.position.z)
                .collect::<BTreeSet<_>>()
                .len();
            Referent {
                key: Key::Actor(id),
                kind: Kind::Figure,
                identity: format!("actor:{id}"),
                name,
                words,
                heads,
                description: safe(&base.description),
                quantity: 1,
                carried: false,
                reachable,
                position: Some(base.position),
                open: None,
                class: None,
                equipped: None,
                hostile,
                height: u8::try_from(height).unwrap_or(u8::MAX),
                way: None,
            }
        })
        .collect();
    result.sort_by_key(|r| {
        let p = r.position.unwrap_or(Position { x: 0, y: 0, z: 0 });
        (distance(p), p.z, p.y, p.x)
    });
    result
}

/// The place's own features that can be named: its ways out, the exit, and
/// the dark it fades into.
fn features(state: &StateView) -> Vec<Referent> {
    use super::place::{survey, Opening};
    let place = survey(state);
    let mut found: Vec<Referent> = Vec::new();
    let mut feature =
        |name: &str, heads: &[&str], way: Option<Direction>, position: Option<Position>| {
            let index = u8::try_from(found.len()).unwrap_or(u8::MAX);
            let mut words: Vec<String> = name.split_whitespace().map(str::to_owned).collect();
            let heads: Vec<String> = heads.iter().map(|h| (*h).to_owned()).collect();
            for head in &heads {
                if !words.contains(head) {
                    words.push(head.clone());
                }
            }
            if let Some(d) = way {
                words.push(direction_name(d).to_owned());
            }
            found.push(Referent {
                key: Key::Feature(index),
                kind: Kind::Feature,
                identity: format!("feature:{index}"),
                name: name.to_owned(),
                words,
                heads,
                description: String::new(),
                quantity: 1,
                carried: false,
                reachable: false,
                position,
                open: None,
                class: None,
                equipped: None,
                hostile: None,
                height: 0,
                way,
            });
        };
    for way in &place.ways {
        match way.kind {
            Opening::Passage => feature(
                "passage",
                &["passage", "corridor", "tunnel", "way", "opening", "path"],
                Some(way.direction),
                None,
            ),
            Opening::Archway => feature(
                "archway",
                &["archway", "arch", "opening", "way", "doorway"],
                Some(way.direction),
                None,
            ),
            Opening::Stairs => feature(
                "stairs",
                &["stairs", "staircase", "stair", "steps", "stairway"],
                Some(way.direction),
                None,
            ),
            // Doors are things of their own.
            Opening::Door { .. } => {}
        }
    }
    let exit = state
        .observation
        .combat
        .as_ref()
        .and_then(|c| c.exit.as_ref())
        .and_then(|key| {
            state
                .observation
                .visible_cells
                .iter()
                .find(|c| &c.key == key && !c.wall)
        });
    if let Some(cell) = exit {
        feature(
            "exit",
            &["exit", "entrance", "out"],
            bearing(cell.position),
            Some(cell.position),
        );
    }
    for d in &place.continues {
        feature(
            "darkness",
            &["darkness", "dark", "shadows", "gloom", "shadow"],
            Some(*d),
            None,
        );
    }
    found
}
