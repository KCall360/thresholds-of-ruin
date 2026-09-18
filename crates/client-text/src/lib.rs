//! Deterministic commands and prose derived exclusively from disclosed state.
use tor_protocol::*;

pub const HELP: &str = "Commands: look (l), inventory (i), north/east/south/west/up/down (n/e/s/w/u/d), go <direction>, take <name or #id>, wait (.), control, release, sync, history [before-id], note <text>, bookmark <text>, quit (q), branch-history <branch> [before-id].\nWizard credential: wizard item <token|tablet> <region> <x> <y> <z>; wizard actor <turn-ticks> <region> <x> <y> <z>; wizard teleport <actor> <region> <x> <y> <z>; wizard rewind <initial|entry-id>.\nNotes/bookmarks are private user notes on the current state.\nannotate <user|frontend> <private|actor> <note|bookmark|explanation> <here|state:N|entry:ID> <text>";

#[derive(Debug, PartialEq, Eq)]
pub enum Input {
    Look,
    Inventory,
    Help,
    Quit,
    Request(Request),
    Command(Command),
}

pub fn parse(line: &str, state: &StateView) -> Result<Input, String> {
    let (verb, rest) = word(line.trim());
    let verb = verb.to_ascii_lowercase();
    let action = |action| {
        Ok(Input::Command(Command::Act {
            expected_revision: state.revision,
            action,
        }))
    };
    match (verb.as_str(), rest) {
        ("wizard", rest) => parse_wizard(rest, state.revision),
        ("branch-history", rest) => {
            let (branch, before) = word(rest);
            if branch.is_empty() || before.chars().any(char::is_whitespace) {
                return Err("Use branch-history <branch> [before-id]".into());
            }
            Ok(Input::Request(Request::HistoryBranch {
                branch: BranchId(branch.into()),
                before: (!before.is_empty()).then(|| EntryId(before.into())),
                limit: 50,
            }))
        }
        ("look" | "l", "") => Ok(Input::Look),
        ("inventory" | "i", "") => Ok(Input::Inventory),
        ("help" | "?", "") => Ok(Input::Help),
        ("quit" | "q", "") => Ok(Input::Quit),
        ("wait" | ".", "") => action(Action::Wait),
        ("control", "") => Ok(Input::Request(Request::AcquireControl)),
        ("release", "") => Ok(Input::Request(Request::ReleaseControl)),
        ("sync", "") => Ok(Input::Request(Request::Snapshot)),
        ("history", before) if !before.chars().any(char::is_whitespace) => {
            Ok(Input::Request(Request::History {
                before: (!before.is_empty()).then(|| EntryId(before.into())),
                limit: 50,
            }))
        }
        ("take" | "get", noun) if !noun.is_empty() => {
            let noun = noun.to_lowercase();
            let noun = noun.strip_prefix("the ").unwrap_or(&noun);
            let matches: Vec<_> = state
                .observation
                .ground_items
                .iter()
                .filter(|item| {
                    if let Some(id) = noun.strip_prefix('#') {
                        return id.parse::<u64>() == Ok(item.item.id);
                    }
                    let name = item.item.name.to_lowercase();
                    !noun.is_empty() && (name == noun || name.ends_with(&format!(" {noun}")))
                })
                .collect();
            match matches.as_slice() {
                [item] => action(Action::Take { item: item.item.id }),
                [] => Err("No disclosed ground item matches that name.".into()),
                _ => Err(format!(
                    "Which item? Use take #id: {}",
                    matches
                        .iter()
                        .map(|i| format!("{} (#{})", safe(&i.item.name), i.item.id))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
        ("note" | "bookmark", text) => {
            annotation("user", "private", &verb, "here", text, state.revision)
        }
        ("annotate", rest) => {
            let (source, rest) = word(rest);
            let (audience, rest) = word(rest);
            let (category, rest) = word(rest);
            let (anchor, text) = word(rest);
            annotation(source, audience, category, anchor, text, state.revision)
        }
        ("go", direction) => action(Action::Move {
            direction: parse_direction(direction)?,
        }),
        (direction, "") => action(Action::Move {
            direction: parse_direction(direction)?,
        }),
        _ => Err("Unknown command. Type help for commands.".into()),
    }
}

fn word(text: &str) -> (&str, &str) {
    let text = text.trim_start();
    text.split_once(char::is_whitespace)
        .map_or((text, ""), |(a, b)| (a, b.trim_start()))
}

fn parse_direction(text: &str) -> Result<Direction, String> {
    match text.to_ascii_lowercase().as_str() {
        "north" | "n" => Ok(Direction::North),
        "east" | "e" => Ok(Direction::East),
        "south" | "s" => Ok(Direction::South),
        "west" | "w" => Ok(Direction::West),
        "up" | "u" => Ok(Direction::Up),
        "down" | "d" => Ok(Direction::Down),
        _ => Err("Unknown command or direction. Type help for commands.".into()),
    }
}

fn annotation(
    source: &str,
    audience: &str,
    category: &str,
    anchor: &str,
    text: &str,
    revision: u64,
) -> Result<Input, String> {
    let source = match source {
        "user" => ClientSource::User,
        "frontend" => ClientSource::Frontend,
        _ => return Err("Source must be user or frontend.".into()),
    };
    let audience = match audience {
        "private" => Audience::Private,
        "actor" => Audience::Actor,
        _ => return Err("Audience must be private or actor.".into()),
    };
    let category = match category {
        "note" => AnnotationCategory::Note,
        "bookmark" => AnnotationCategory::Bookmark,
        "explanation" => AnnotationCategory::Explanation,
        _ => return Err("Unknown annotation category.".into()),
    };
    let anchor = if anchor == "here" {
        Anchor::State { revision }
    } else if let Some(n) = anchor.strip_prefix("state:") {
        Anchor::State {
            revision: n.parse().map_err(|_| "Invalid state revision.")?,
        }
    } else if let Some(id) = anchor.strip_prefix("entry:").filter(|id| !id.is_empty()) {
        Anchor::Entry {
            id: EntryId(id.into()),
        }
    } else {
        return Err("Anchor must be here, state:N, or entry:ID.".into());
    };
    if text.trim().is_empty()
        || text.len() > MAX_NOTE_BYTES
        || text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err("Notes need 1–4096 UTF-8 bytes and no terminal control characters.".into());
    }
    Ok(Input::Command(Command::Annotate {
        anchor,
        text: text.into(),
        source,
        audience,
        category,
    }))
}

/// Escape control characters even in names and error messages from the server.
pub fn safe(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

pub fn describe(state: &StateView) -> String {
    let o = &state.observation;
    let mut lines = vec![format!(
        "{} — position ({}, {}, {}); tick {}; revision {}. {}",
        safe(&o.region.name),
        o.position.x,
        o.position.y,
        o.position.z,
        o.tick,
        state.revision,
        if o.ready {
            "Ready to act."
        } else {
            "Waiting for another actor."
        }
    )];
    for item in &o.ground_items {
        lines.push(format!(
            "You see {} (#{}), at ({}, {}, {}).{}",
            safe(&item.item.name),
            item.item.id,
            item.position.x,
            item.position.y,
            item.position.z,
            if item.position == o.position {
                " Within reach."
            } else {
                ""
            }
        ));
    }
    for exit in &o.exits {
        lines.push(format!(
            "Passage {:?} at ({}, {}, {}).",
            exit.direction, exit.position.x, exit.position.y, exit.position.z
        ));
    }
    if state.wizard_game {
        lines.insert(0, "*** WIZARD GAME — permanently marked ***".into());
    }
    lines.join("\n")
}

pub fn inventory(state: &StateView) -> String {
    if state.observation.inventory.is_empty() {
        return "Inventory: empty.".into();
    }
    format!(
        "Inventory: {}.",
        state
            .observation
            .inventory
            .iter()
            .map(|i| format!("{} (#{})", safe(&i.name), i.id))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn history(entry: &HistoryEntry) -> String {
    let content = match &entry.content {
        HistoryContent::Wizard { operation, result } => format!("Wizard {operation:?}: {result:?}"),
        HistoryContent::Action { event, .. } => format!("{event:?}"),
        HistoryContent::Annotation {
            anchor,
            category,
            text,
        } => format!("{category:?} {anchor:?}: {}", safe(text)),
    };
    safe(&format!(
        "[{}] tick {} {:?} {:?}: {content} (branch {})",
        entry.id.0, entry.tick, entry.audience, entry.author, entry.branch.0
    ))
}

fn parse_wizard(text: &str, expected_revision: u64) -> Result<Input, String> {
    let words: Vec<_> = text.split_whitespace().collect();
    let usage = "Wizard commands: wizard item <token|tablet> <region> <x> <y> <z>; wizard actor <turn-ticks> <region> <x> <y> <z>; wizard teleport <actor> <region> <x> <y> <z>; wizard rewind <initial|entry-id>";
    let position = |v: &[&str]| -> Result<Position, String> {
        Ok(Position {
            region: v[0].parse().map_err(|_| usage)?,
            x: v[1].parse().map_err(|_| usage)?,
            y: v[2].parse().map_err(|_| usage)?,
            z: v[3].parse().map_err(|_| usage)?,
        })
    };
    let operation = match words.as_slice() {
        ["item", kind, r, x, y, z] => WizardOperation::PlaceItem {
            kind: match *kind {
                "token" => WizardItem::Token,
                "tablet" => WizardItem::Tablet,
                _ => return Err(usage.into()),
            },
            position: position(&[r, x, y, z])?,
        },
        ["actor", ticks, r, x, y, z] => WizardOperation::SpawnActor {
            turn_ticks: ticks.parse().map_err(|_| usage)?,
            position: position(&[r, x, y, z])?,
        },
        ["teleport", actor, r, x, y, z] => WizardOperation::Teleport {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            position: position(&[r, x, y, z])?,
        },
        ["rewind", target] => WizardOperation::Rewind {
            target: (*target != "initial").then(|| EntryId((*target).into())),
        },
        _ => return Err(usage.into()),
    };
    Ok(Input::Command(Command::Wizard {
        expected_revision,
        operation,
    }))
}
