//! Adventure presentation and intentions, using only the disclosed observer scene.
use std::collections::{BTreeMap, BTreeSet};

use tor_protocol::*;

use crate::{parse, parse_direction, safe, Input};

pub const HELP: &str = "look (l), examine <thing> (x), inventory (i), get <thing>, go to <thing>, north/east/south/west/up/down, wait, stop, quit.\nAnswer a question with a name or its number. You can type stop while walking.";
pub const SESSION_HELP: &str = "control, release, sync, history, note <text>, bookmark <text>.\nstep <direction> makes one careful step. Developer commands require wizard authority.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Look,
    Say(String),
    Action(Action),
    Travel {
        destination: String,
        take: Option<u64>,
        label: String,
        direction: Option<Direction>,
    },
    Stop,
    Tools(Input),
}

#[derive(Clone, Debug)]
struct Choice {
    label: String,
    intent: Intent,
    item: Option<u64>,
}

#[derive(Default)]
pub struct Dialogue {
    choices: Option<(u64, Vec<Choice>)>,
    item: Option<u64>,
}

impl Dialogue {
    /// A setup/rewind boundary discards conversational references to old state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn interpret(&mut self, line: &str, state: &StateView) -> Intent {
        let normalized = line.trim().to_lowercase();
        if let Some((revision, choices)) = self.choices.take() {
            if revision == state.revision {
                let matches: Vec<_> = choices
                    .iter()
                    .enumerate()
                    .filter(|(i, c)| {
                        normalized.parse::<usize>() == Ok(i + 1)
                            || noun_matches(&normalized, &c.label)
                    })
                    .map(|(_, c)| c.clone())
                    .collect();
                if matches.len() == 1 {
                    self.item = matches[0].item;
                    return matches[0].intent.clone();
                }
            }
        }
        let (verb, rest) = crate::word(&normalized);
        match (verb, rest) {
            ("look" | "l", "") => Intent::Look,
            ("inventory" | "i", "") => Intent::Say(inventory(state)),
            ("help", "session") => Intent::Say(SESSION_HELP.into()),
            ("help" | "?", _) => Intent::Say(HELP.into()),
            ("stop" | "cancel", "") => Intent::Stop,
            ("step", direction) => match parse_direction(direction) {
                Ok(direction) => Intent::Action(Action::Move { direction }),
                Err(_) => Intent::Say("Which direction would you like to step?".into()),
            },
            ("examine" | "x" | "inspect", noun) => self.object(noun, state, "examine"),
            ("look", noun) if noun.starts_with("at ") => self.object(&noun[3..], state, "examine"),
            ("take" | "get", noun) => self.object(noun, state, "take"),
            ("go" | "approach", noun) if parse_direction(noun).is_err() => {
                self.object(noun.strip_prefix("to ").unwrap_or(noun), state, "go")
            }
            _ => {
                let direction = if verb == "go" {
                    parse_direction(rest)
                } else if rest.is_empty() {
                    parse_direction(verb)
                } else {
                    Err(String::new())
                };
                if let Ok(direction) = direction {
                    let choices = destinations(state, direction)
                        .into_iter()
                        .map(|d| Choice {
                            label: d.label.clone(),
                            item: None,
                            intent: Intent::Travel {
                                destination: d.key,
                                take: None,
                                label: d.label,
                                direction: Some(direction),
                            },
                        })
                        .collect();
                    return self.choose(
                        choices,
                        state.revision,
                        &format!("You can't see a way {}.", direction_name(direction)),
                    );
                }
                match parse(line, state) {
                    Ok(Input::Command(Command::Act { action, .. })) => Intent::Action(action),
                    Ok(input) => Intent::Tools(input),
                    Err(_) => Intent::Say(
                        "I don't understand that. Type help for things you can try.".into(),
                    ),
                }
            }
        }
    }

    fn choose(&mut self, choices: Vec<Choice>, revision: u64, missing: &str) -> Intent {
        match choices.as_slice() {
            [] => Intent::Say(missing.into()),
            [choice] => {
                self.item = choice.item;
                choice.intent.clone()
            }
            _ => {
                let question = format!(
                    "Which do you mean? {}",
                    choices
                        .iter()
                        .enumerate()
                        .map(|(i, c)| format!("{}) {}", i + 1, safe(&c.label)))
                        .collect::<Vec<_>>()
                        .join("; ")
                );
                self.choices = Some((revision, choices));
                Intent::Say(question)
            }
        }
    }

    fn object(&mut self, noun: &str, state: &StateView, verb: &str) -> Intent {
        if verb == "examine"
            && matches!(noun, "wall" | "walls" | "floor" | "the walls" | "the floor")
        {
            let walls = noun.contains("wall");
            let materials: BTreeSet<_> = state
                .observation
                .visible_cells
                .iter()
                .filter(|c| c.wall == walls)
                .map(surface)
                .collect();
            return Intent::Say(if materials.is_empty() {
                "You cannot see that here.".into()
            } else {
                format!(
                    "The visible {} {} made of {}.",
                    if walls { "walls" } else { "floor" },
                    if walls { "are" } else { "is" },
                    materials.into_iter().collect::<Vec<_>>().join(" and ")
                )
            });
        }
        let mut items: BTreeMap<u64, &ItemView> = BTreeMap::new();
        for ground in &state.observation.ground_items {
            items.insert(ground.item.id, &ground.item);
        }
        for item in &state.observation.inventory {
            items.insert(item.id, item);
        }
        let mut choices = Vec::new();
        for (id, item) in items {
            if !(noun.is_empty()
                || noun_matches(noun, &item.name)
                || noun == "it" && self.item == Some(id))
            {
                continue;
            }
            let intent = if verb == "examine" {
                Intent::Say(if item.description.is_empty() {
                    "You notice no further distinguishing details.".into()
                } else {
                    safe(&item.description)
                })
            } else if state.observation.inventory.iter().any(|i| i.id == id) {
                Intent::Say("You are already carrying that.".into())
            } else {
                let ground = state
                    .observation
                    .ground_items
                    .iter()
                    .filter(|g| g.item.id == id)
                    .min_by_key(|g| (!g.reachable, distance(g.position)));
                let Some(ground) = ground else {
                    continue;
                };
                if ground.reachable {
                    if verb == "take" {
                        Intent::Action(Action::Take { item: id })
                    } else {
                        Intent::Say("You are already there.".into())
                    }
                } else {
                    let Some(cell) = state
                        .observation
                        .visible_cells
                        .iter()
                        .find(|c| c.position == ground.position && !c.wall)
                    else {
                        continue;
                    };
                    Intent::Travel {
                        destination: cell.key.clone(),
                        take: (verb == "take").then_some(id),
                        label: format!("the {}", safe(&item.name)),
                        direction: None,
                    }
                }
            };
            choices.push(Choice {
                label: item.name.clone(),
                intent,
                item: Some(id),
            });
        }
        if verb == "examine" {
            let mut actors = BTreeSet::new();
            for actor in &state.observation.visible_actors {
                if noun_matches(noun, &actor.name) && actors.insert(actor.id) {
                    choices.push(Choice {
                        label: format!("{} {}", safe(&actor.name), whereabouts(actor.position)),
                        intent: Intent::Say(safe(&actor.description)),
                        item: None,
                    });
                }
            }
        }
        self.choose(
            choices,
            state.revision,
            "You cannot see anything like that here.",
        )
    }
}

fn noun_matches(noun: &str, name: &str) -> bool {
    let words: Vec<_> = noun
        .split_whitespace()
        .filter(|w| !matches!(*w, "the" | "a" | "an" | "one"))
        .collect();
    let name = name.to_lowercase();
    !words.is_empty()
        && words
            .iter()
            .all(|word| name.split_whitespace().any(|w| w == *word))
}

fn distance(p: Position) -> u64 {
    u64::from(p.x.unsigned_abs()) + u64::from(p.y.unsigned_abs()) + u64::from(p.z.unsigned_abs())
}

pub fn direction_name(d: Direction) -> &'static str {
    match d {
        Direction::North => "north",
        Direction::East => "east",
        Direction::South => "south",
        Direction::West => "west",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

fn bearing(p: Position) -> Option<Direction> {
    if p.z != 0 {
        Some(if p.z > 0 {
            Direction::Up
        } else {
            Direction::Down
        })
    } else if p.x == 0 && p.y == 0 {
        None
    } else if p.x.unsigned_abs() > p.y.unsigned_abs() {
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

fn whereabouts(p: Position) -> String {
    bearing(p).map_or_else(
        || "at your feet".into(),
        |d| match d {
            Direction::Up => "above you".into(),
            Direction::Down => "below you".into(),
            _ => format!("to the {}", direction_name(d)),
        },
    )
}

// Use the same visible-anchor grouping for both exits and item descriptions.
fn place_at(state: &StateView, position: Position) -> Option<&str> {
    state
        .observation
        .visible_cells
        .iter()
        .filter(|c| c.place_hint && !c.wall && c.position.z == position.z)
        .min_by_key(|c| {
            (
                (i64::from(c.position.x) - i64::from(position.x)).unsigned_abs()
                    + (i64::from(c.position.y) - i64::from(position.y)).unsigned_abs(),
                &c.key,
            )
        })
        .map(|c| c.key.as_str())
}

fn in_current_place(state: &StateView, position: Position) -> bool {
    let origin = Position { x: 0, y: 0, z: 0 };
    position.z == 0 && place_at(state, position) == place_at(state, origin)
}

struct Destination {
    key: String,
    label: String,
}

fn destinations(state: &StateView, direction: Direction) -> Vec<Destination> {
    let cells = &state.observation.visible_cells;
    let origin = cells.iter().find(|c| distance(c.position) == 0);
    let current = place_at(state, Position { x: 0, y: 0, z: 0 });
    let mut seen = BTreeSet::new();
    let mut anchors: Vec<_> = cells
        .iter()
        .filter(|c| {
            !c.wall
                && c.place_hint
                && bearing(c.position) == Some(direction)
                && origin.is_none_or(|o| o.key != c.key)
                && current.is_none_or(|key| key != c.key)
        })
        .collect();
    anchors.sort_by_key(|c| (distance(c.position), &c.key));
    let result: Vec<_> = anchors
        .into_iter()
        .filter(|c| seen.insert(c.key.clone()))
        .map(|c| {
            let item = state
                .observation
                .ground_items
                .iter()
                .find(|i| i.position == c.position);
            Destination {
                key: c.key.clone(),
                label: item.map_or_else(
                    || format!("an open place to the {}", direction_name(direction)),
                    |i| format!("the place by the {}", safe(&i.item.name)),
                ),
            }
        })
        .collect();
    if !result.is_empty() {
        return result;
    }
    // Bare floor is movement within a place, not evidence of a way onward.
    // Stairs provide an explicit exception even in an unhinted space.
    if matches!(direction, Direction::Up | Direction::Down)
        && origin.is_some_and(|c| {
            if direction == Direction::Up {
                c.stairs_up
            } else {
                c.stairs_down
            }
        })
    {
        if let Some(c) = cells
            .iter()
            .filter(|c| {
                !c.wall
                    && c.position.x == 0
                    && c.position.y == 0
                    && bearing(c.position) == Some(direction)
            })
            .min_by_key(|c| distance(c.position))
        {
            return vec![Destination {
                key: c.key.clone(),
                label: format!("the stairs {}", direction_name(direction)),
            }];
        }
    }
    vec![]
}

fn surface(cell: &CellView) -> &str {
    if cell.material.is_empty() {
        "unremarkable material"
    } else {
        &cell.material
    }
}

fn indefinite(name: &str) -> String {
    let article = if name.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {}", safe(name))
}

pub fn describe(state: &StateView) -> String {
    let o = &state.observation;
    let mut lines = Vec::new();
    if state.wizard_game {
        lines.push("*** WIZARD GAME — permanently marked ***".into());
    }
    let floor = o
        .visible_cells
        .iter()
        .find(|c| distance(c.position) == 0 && !c.wall);
    lines.push(floor.map_or_else(
        || "Your surroundings".into(),
        |c| format!("You stand in a space with a {} floor.", safe(surface(c))),
    ));
    let walls: BTreeSet<_> = o
        .visible_cells
        .iter()
        .filter(|c| c.wall)
        .map(|c| safe(surface(c)))
        .collect();
    if !walls.is_empty() {
        lines.push(format!(
            "You can see walls of {}.",
            walls.into_iter().collect::<Vec<_>>().join(" and ")
        ));
    }
    let mut seen = BTreeSet::new();
    for item in &o.ground_items {
        if seen.insert(item.item.id) {
            lines.push(format!(
                "You see {} {}.",
                indefinite(&item.item.name),
                if item.reachable {
                    "at your feet".into()
                } else if in_current_place(state, item.position) {
                    "on the floor nearby".into()
                } else {
                    whereabouts(item.position)
                }
            ));
        }
    }
    let mut actors = BTreeSet::new();
    for actor in &o.visible_actors {
        if actors.insert(actor.id) {
            lines.push(format!(
                "You see {} {}.",
                if actor.id == o.actor {
                    "yourself".into()
                } else {
                    indefinite(if actor.name.is_empty() {
                        "figure"
                    } else {
                        &actor.name
                    })
                },
                whereabouts(actor.position)
            ));
        }
    }
    let ways: Vec<_> = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ]
    .into_iter()
    .filter(|d| !destinations(state, *d).is_empty())
    .map(direction_name)
    .collect();
    if !ways.is_empty() {
        lines.push(format!("You can head {}.", ways.join(" or ")));
    }
    if !o.ready {
        lines.push("For now, you must wait.".into());
    }
    lines.join("\n")
}

pub fn inventory(state: &StateView) -> String {
    if state.observation.inventory.is_empty() {
        "You are empty-handed.".into()
    } else {
        format!(
            "You are carrying: {}.",
            state
                .observation
                .inventory
                .iter()
                .map(|i| safe(&i.name))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

pub fn event(entry: &HistoryEntry, state: &StateView) -> String {
    match &entry.content {
        HistoryContent::Action {
            event: Event::Taken { item },
            ..
        } => state
            .observation
            .inventory
            .iter()
            .find(|i| i.id == *item)
            .map_or_else(
                || "Taken.".into(),
                |i| format!("You pick up the {}.", safe(&i.name)),
            ),
        HistoryContent::Action {
            event: Event::Moved { direction },
            ..
        } => format!("You move {}.", direction_name(*direction)),
        HistoryContent::Action {
            event: Event::Waited,
            ..
        } => "Time passes.".into(),
        HistoryContent::Annotation { text, .. } => format!("Note: {}", safe(text)),
        HistoryContent::Wizard { summary, .. } => safe(summary),
        HistoryContent::Travel { .. } => "You set off.".into(),
    }
}
