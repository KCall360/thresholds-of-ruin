//! One disclosed view, interpreted for language: what is here, what it is
//! called, and where it is from the character's point of view.
use std::collections::BTreeMap;

use tor_client_common::{surfaces, Palette};
use tor_protocol::*;

use crate::{adventure, safe};

/// A referent's identity, stable across views while the thing stays known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Item(u64),
    Actor(ActorId),
    Door(u64),
    Surface(Surface),
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

/// Synonyms for a name's last word.
fn synonyms(head: &str) -> &'static [&'static str] {
    match head {
        "corpse" => &["body", "remains", "carcass", "cadaver"],
        "token" => &["disc", "disk"],
        "tablet" => &["slab"],
        "gate" | "portcullis" | "hatch" | "grate" => &["door"],
        "scout" | "guardian" | "wisp" | "rat" | "goblin" | "figure" => &["creature", "monster"],
        _ => &[],
    }
}

fn naming(name: &str, kind: Kind) -> (Vec<String>, Vec<String>) {
    let mut words: Vec<String> = name.split_whitespace().map(str::to_lowercase).collect();
    let mut heads = Vec::new();
    if let Some(last) = words.last().cloned() {
        heads.push(last.clone());
        heads.extend(synonyms(&last).iter().map(|s| (*s).to_owned()));
    }
    let generic: &[&str] = match kind {
        Kind::Thing => &["thing", "item", "object"],
        Kind::Figure => &["figure", "creature", "someone"],
        Kind::Door => &["door"],
        Kind::Surface | Kind::Me => &[],
    };
    heads.extend(generic.iter().map(|s| (*s).to_owned()));
    heads.dedup();
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
    pub referents: Vec<Referent>,
}

impl<'a> Scene<'a> {
    pub fn new(state: &'a StateView, palette: &'a Palette) -> Self {
        let o = &state.observation;
        let mut referents = Vec::new();
        for item in &o.inventory {
            referents.push(thing(item, true, true, None));
        }
        let mut ground: BTreeMap<u64, &GroundItemView> = BTreeMap::new();
        for g in &o.ground_items {
            // A portal can show one item twice; the nearest view counts.
            let entry = ground.entry(g.item.id).or_insert(g);
            if (!g.reachable, distance(g.position)) < (!entry.reachable, distance(entry.position)) {
                *entry = g;
            }
        }
        for g in ground.values() {
            referents.push(thing(&g.item, false, g.reachable, Some(g.position)));
        }
        for figure in figures(o, palette) {
            referents.push(figure);
        }
        let mut doors = BTreeMap::new();
        for cell in &o.visible_cells {
            if let Some(door) = &cell.door {
                doors.entry(door.id).or_insert((door, cell.position));
            }
        }
        for (door, position) in doors.values() {
            let name = if door.name.trim().is_empty() {
                "door".to_owned()
            } else {
                safe(&door.name).to_lowercase()
            };
            let (words, heads) = naming(&name, Kind::Door);
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
                position: Some(*position),
                open: Some(door.open),
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
                });
            }
        }
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
        });
        Self {
            state,
            palette,
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
    pub fn figure_ids(&self) -> std::collections::BTreeSet<ActorId> {
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
    let (words, heads) = naming(&name, Kind::Thing);
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
    }
}

/// One referent per visible actor other than the character, however many
/// cells its body covers. Its position is its lowest, nearest cell.
fn figures(o: &Observation, palette: &Palette) -> Vec<Referent> {
    let mut bodies: BTreeMap<ActorId, Vec<&ActorView>> = BTreeMap::new();
    for actor in o.visible_actors.iter().filter(|a| a.id != o.actor) {
        bodies.entry(actor.id).or_default().push(actor);
    }
    let mut result: Vec<Referent> = bodies
        .into_iter()
        .map(|(id, cells)| {
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
            let (words, heads) = naming(&name, Kind::Figure);
            Referent {
                key: Key::Actor(id),
                kind: Kind::Figure,
                identity: format!("actor:{}", id.0),
                name,
                words,
                heads,
                description: safe(&base.description),
                quantity: 1,
                carried: false,
                reachable,
                position: Some(base.position),
                open: None,
            }
        })
        .collect();
    result.sort_by_key(|r| {
        let p = r.position.unwrap_or(Position { x: 0, y: 0, z: 0 });
        (distance(p), p.z, p.y, p.x)
    });
    result
}
