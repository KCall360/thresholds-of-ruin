//! Deterministic commands and prose derived exclusively from disclosed state.
use tor_protocol::*;

pub mod admission;
pub mod adventure;
pub mod engine;
pub mod narrative;
pub mod parser;

pub const HELP: &str = "Commands: advancement (overview, ledger, balances, skills, talents, purchase, remove, replace, details, next, previous, supplies, confirm, resume, cancel), attack <actor>, <disclosed ability name> <actor>, places, name <place number> <new name>, look (l), inventory (i), stats, north/east/south/west/ne/se/sw/nw/up/down (n/e/s/w/ne/se/sw/nw/u/d), go <direction>, take/drop [quantity] <name or #id>, equip/remove/drink <name or #id>, wait (.), control, release, resume, cancel [direction], escape [hold <number|#token|name>], escape <direction>, sync, save, history [before-id], note <text>, bookmark <text>, quit (q), branch-history <branch> [before-id].\nWizard credential: wizard <server developer command>; wizard creature list discovers named spawn recipes. After creature inspection, strikes natural, strikes selected, strikes ability <id>, strikes next, strikes previous.\nNotes/bookmarks are private user notes on the current state.\nannotate <user|frontend> <private|actor> <note|bookmark|explanation> <here|state:N|entry:ID> <text>";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Advancement(tor_client_common::advancement::AdvancementInput),
    Look,
    Inventory,
    Stats,
    StrikeSource(StrikeSourceSelector),
    StrikeNext,
    StrikePrevious,
    Places,
    Help,
    Quit,
    /// Show, or set in milliseconds, the time between shown journey steps.
    Pace(Option<u64>),
    ResumeIntention,
    CancelIntention {
        step: Option<Direction>,
    },
    Request(Request),
    Command(Command),
}

/// Ordinary owner-only controls; these do not enter the wizard parser.
pub fn advancement_input(line: &str) -> Option<Result<Input, String>> {
    let (verb, rest) = word(line.trim());
    matches!(
        verb.to_ascii_lowercase().as_str(),
        "advancement" | "advance"
    )
    .then(|| tor_client_common::advancement::parse(rest).map(Input::Advancement))
}

/// Explicit private-detail controls shared by script and adventure input.
pub fn strike_input(line: &str) -> Option<Result<Input, String>> {
    let (verb, rest) = word(line.trim());
    if !verb.eq_ignore_ascii_case("strikes") {
        return None;
    }
    let parts: Vec<_> = rest.split_whitespace().collect();
    Some(match parts.as_slice() {
        ["natural"] => Ok(Input::StrikeSource(StrikeSourceSelector::Natural {})),
        ["selected"] => Ok(Input::StrikeSource(StrikeSourceSelector::SelectedMelee {})),
        ["ability", id] if (StrikeSourceSelector::Ability { id: (*id).into() }).valid() => Ok(Input::StrikeSource(StrikeSourceSelector::Ability { id: (*id).into() })),
        ["next"] => Ok(Input::StrikeNext),
        ["previous"] => Ok(Input::StrikePrevious),
        _ => Err("Use strikes natural, strikes selected, strikes ability <id>, strikes next or strikes previous.".into()),
    })
}

pub fn strike_command(
    input: &Input,
    pages: &mut tor_client_common::inspection_pages::InspectionPages,
    context: &ReplyContext,
) -> Result<Command, String> {
    match input {
        Input::StrikeSource(source) => pages.select(source.clone(), context),
        Input::StrikeNext => pages.next(context),
        Input::StrikePrevious => pages.previous(context),
        _ => Err("Not a strike detail command.".into()),
    }
}

/// Linked-hold Escape takes precedence over legacy flee idioms. Directions
/// retain the separate cancel-and-step preparation control.
pub fn hold_input(line: &str, state: &StateView) -> Option<Result<Input, String>> {
    let (verb, rest) = word(line.trim());
    if !verb.eq_ignore_ascii_case("escape") || (!rest.is_empty() && parse_direction(rest).is_ok()) {
        return None;
    }
    let (kind, selector) = word(rest);
    let selected = if rest.is_empty() {
        tor_client_common::holds::select_hold(&state.observation, "")
    } else if kind.eq_ignore_ascii_case("hold") && !selector.is_empty() {
        tor_client_common::holds::select_hold(&state.observation, selector)
    } else {
        Err("Use escape, escape hold <number|#token|name>, or escape <direction>.".into())
    };
    Some(selected.map(|hold| {
        Input::Command(Command::Act {
            expected_revision: state.revision,
            action: Action::Escape { hold },
        })
    }))
}

/// Explicit preparation controls, shared by script and adventure input.
pub fn intention_input(line: &str) -> Option<Result<Input, String>> {
    let (verb, rest) = word(line.trim());
    match (verb.to_ascii_lowercase().as_str(), rest) {
        ("resume", "") => Some(Ok(Input::ResumeIntention)),
        ("cancel", "") => Some(Ok(Input::CancelIntention { step: None })),
        ("cancel" | "escape", direction) if !direction.is_empty() => {
            Some(parse_direction(direction).map(|step| Input::CancelIntention { step: Some(step) }))
        }
        ("resume" | "escape", _) => Some(Err("Use resume, cancel, or escape <direction>.".into())),
        _ => None,
    }
}

/// Build a fresh, permission-checked continuation without trusting typed IDs.
pub fn intention_request(
    input: &Input,
    state: &tor_client_common::ClientState,
) -> Result<Request, String> {
    match input {
        Input::ResumeIntention => state
            .resume_intention_request()
            .ok_or_else(|| "No suspended action to resume.".into()),
        Input::CancelIntention { step: None } => state
            .cancel_intention_request()
            .ok_or_else(|| "No action to cancel.".into()),
        Input::CancelIntention { step: Some(step) } => state
            .cancel_intention_with_step_request(*step)
            .ok_or_else(|| {
                "You can escape only after a strike, when cancellation is available.".into()
            }),
        _ => Err("That input is not a preparation control.".into()),
    }
}

pub fn parse(line: &str, state: &StateView) -> Result<Input, String> {
    if let Some(input) = advancement_input(line) {
        return input;
    }
    if let Some(input) = strike_input(line) {
        return input;
    }
    if let Some(input) = hold_input(line, state) {
        return input;
    }
    if let Some(input) = intention_input(line) {
        return input;
    }
    let (verb, rest) = word(line.trim());
    let verb = verb.to_ascii_lowercase();
    let action = |action| {
        Ok(Input::Command(Command::Act {
            expected_revision: state.revision,
            action,
        }))
    };
    if let Some((ability, noun)) =
        tor_client_common::abilities::match_command(&state.observation, line)?
    {
        let (noun, supplies) = tor_client_common::abilities::split_supply_clause(noun)?;
        let supplies = supplies.map_or(Ok(SupplySelection::Carried), |text| {
            tor_client_common::abilities::selection_from_text(&state.observation, ability, text)
        })?;
        if ability.target_kind == AbilityTargetKind::SelfStep {
            let direction = crate::parser::lexicon::parse_direction(&noun.to_ascii_lowercase())
                .ok_or("Choose a direction for that ability.")?;
            return action(tor_client_common::abilities::invocation_action(
                &state.observation,
                &ability.id,
                AbilityInvocation::SelfStep { direction },
                supplies,
            )?);
        }
        let allows_self = matches!(
            ability.target_kind,
            AbilityTargetKind::SelfOnly | AbilityTargetKind::SelfOrAlly
        );
        let opaque = noun
            .strip_prefix('#')
            .and_then(|value| value.parse::<ActorTarget>().ok());
        let target = if allows_self
            && (noun.is_empty()
                || matches!(noun.to_ascii_lowercase().as_str(), "me" | "self" | "myself")
                || opaque == Some(state.observation.self_target))
        {
            state.observation.self_target
        } else {
            let matches: Vec<_> = state
                .observation
                .visible_actors
                .iter()
                .filter(|a| {
                    tor_client_common::abilities::target_eligible(&state.observation, ability, a.id)
                        && (a.name.to_lowercase().contains(&noun.to_lowercase())
                            || opaque == Some(a.id))
                })
                .collect();
            match matches.as_slice() {
                [actor] => actor.id,
                [] => return Err("No matching actor is visible.".into()),
                _ => return Err("Which actor? Use the ability name followed by #id.".into()),
            }
        };
        return action(tor_client_common::abilities::action_with_supplies(
            &state.observation,
            &ability.id,
            target,
            supplies,
        )?);
    }
    match (verb.as_str(), rest) {
        ("equip" | "wear" | "wield" | "remove" | "unequip" | "drink" | "quaff" | "eat", noun)
            if !noun.is_empty() =>
        {
            let operation = match verb.as_str() {
                "remove" | "unequip" => tor_client_common::items::ItemOperation::Unequip,
                "drink" | "quaff" => tor_client_common::items::ItemOperation::Drink,
                "eat" => tor_client_common::items::ItemOperation::Eat,
                _ => tor_client_common::items::ItemOperation::Equip,
            };
            let noun = noun.to_lowercase();
            let noun = noun.strip_prefix("the ").unwrap_or(&noun);
            let matches: Vec<_> = state
                .observation
                .inventory
                .iter()
                .filter(|item| item_matches(noun, item))
                .collect();
            match matches.as_slice() {
                [item] => action(tor_client_common::items::item_action(
                    &state.observation,
                    item.id,
                    operation,
                )?),
                [] => Err("No carried item matches that name.".into()),
                _ => Err(format!(
                    "Which item? Use {} #id: {}",
                    operation.verb(),
                    matches
                        .iter()
                        .map(|item| format!("{} (#{})", safe(&item.name), item.id))
                        .collect::<Vec<_>>()
                        .join(", ")
                )),
            }
        }
        ("attack" | "hit" | "fight", noun) => {
            let mut actors: Vec<_> = state
                .observation
                .visible_actors
                .iter()
                .filter(|a| {
                    a.id != state.observation.self_target
                        && (a.name.to_lowercase().contains(&noun.to_lowercase())
                            || noun
                                .strip_prefix('#')
                                .and_then(|s| s.parse::<ActorTarget>().ok())
                                == Some(a.id))
                })
                .collect();
            actors.sort_by_key(|a| a.id);
            actors.dedup_by_key(|a| a.id);
            match actors.as_slice() {
                [actor] => action(tor_client_common::abilities::basic_action(
                    &state.observation,
                    actor.id,
                )?),
                [] => Err("No matching actor is visible.".into()),
                _ => Err(format!("Which actor? Use {verb} #id.")),
            }
        }
        ("places", "") => Ok(Input::Places),
        ("name", rest) => rename_place(rest, state),
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
        ("stats", "") => Ok(Input::Stats),
        ("help" | "?", "") => Ok(Input::Help),
        ("quit" | "q", "") => Ok(Input::Quit),
        ("open" | "close", noun) => {
            let mut doors: Vec<_> = state
                .observation
                .visible_cells
                .iter()
                .filter_map(|c| c.door.as_ref())
                .filter(|d| {
                    noun.is_empty()
                        || noun == "door"
                        || noun == d.name
                        || noun
                            .strip_prefix('#')
                            .and_then(|s| s.parse::<DoorTarget>().ok())
                            == Some(d.id)
                })
                .collect();
            doors.sort_by_key(|d| d.id);
            doors.dedup_by_key(|d| d.id);
            match doors.as_slice() {
                [door] => action(Action::SetDoor {
                    door: door.id,
                    open: verb == "open",
                }),
                [] => Err("No matching door is visible.".into()),
                _ => Err("Which door? Use open #id or close #id.".into()),
            }
        }
        ("wait" | ".", "")
            if !state.observation.ready
                && state
                    .observation
                    .combat
                    .as_ref()
                    .is_some_and(|c| !c.terminal) =>
        {
            Ok(Input::Request(Request::Continue))
        }
        ("wait" | ".", "") => action(Action::Wait),
        ("control", "") => Ok(Input::Request(Request::AcquireControl)),
        ("release", "") => Ok(Input::Request(Request::ReleaseControl)),
        ("save", "") => Ok(Input::Request(Request::Save)),
        ("sync", "") => Ok(Input::Request(Request::Snapshot)),
        ("pace", "") => Ok(Input::Pace(None)),
        ("pace", ms) => ms
            .parse::<u64>()
            .ok()
            .filter(|ms| *ms <= 5000)
            .map(|ms| Input::Pace(Some(ms)))
            .ok_or_else(|| "Use pace <milliseconds from 0 to 5000>.".into()),
        ("history", before) if !before.chars().any(char::is_whitespace) => {
            Ok(Input::Request(Request::History {
                before: (!before.is_empty()).then(|| EntryId(before.into())),
                limit: 50,
            }))
        }
        ("take" | "get" | "drop", noun) if !noun.is_empty() => {
            let (quantity, noun) = item_quantity(noun)?;
            let noun = noun.to_lowercase();
            let noun = noun.strip_prefix("the ").unwrap_or(&noun);
            let items: Vec<_> = if verb == "drop" {
                state.observation.inventory.iter().collect()
            } else {
                state
                    .observation
                    .ground_items
                    .iter()
                    .map(|i| &i.item)
                    .collect()
            };
            let mut matches: Vec<_> = items
                .into_iter()
                .filter(|item| item_matches(noun, item))
                .collect();
            matches.sort_by_key(|item| item.id);
            matches.dedup_by_key(|item| item.id);
            match matches.as_slice() {
                [item] => action(if verb == "drop" {
                    Action::Drop {
                        item: item.id,
                        quantity,
                    }
                } else {
                    Action::Take {
                        item: item.id,
                        quantity,
                    }
                }),
                [] => Err("No disclosed item matches that name.".into()),
                _ => Err(format!(
                    "Which item? Use {} [quantity] #id: {}",
                    verb,
                    matches
                        .iter()
                        .map(|i| format!("{} x {} (#{})", i.quantity, safe(&i.name), i.id))
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
        "northeast" | "north-east" | "ne" => Ok(Direction::NorthEast),
        "southeast" | "south-east" | "se" => Ok(Direction::SouthEast),
        "southwest" | "south-west" | "sw" => Ok(Direction::SouthWest),
        "northwest" | "north-west" | "nw" => Ok(Direction::NorthWest),
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
        "Your surroundings - tick {}. {}",
        o.tick,
        if o.ready {
            "Ready to act."
        } else {
            "Waiting for another actor."
        }
    )];
    if let Some(c) = &o.combat {
        lines.push(tor_client_common::narration::combat_status(c));
        lines.extend(
            c.public_plan
                .as_ref()
                .map(tor_client_common::public_plan::line),
        );
        lines.extend(
            c.objective
                .map(|o| tor_client_common::narration::objective(o).to_owned()),
        );
        lines.extend(
            c.events
                .iter()
                .map(|e| tor_client_common::narration::combat_event(e, o, o)),
        );
    }
    for item in &o.ground_items {
        lines.push(format!(
            "You see {} x {} (#{}), at offset ({}, {}, {}).{}",
            item.item.quantity,
            safe(&item.item.name),
            item.item.id,
            item.position.x,
            item.position.y,
            item.position.z,
            if item.reachable { " Within reach." } else { "" }
        ));
    }
    for actor in &o.visible_actors {
        lines.push(format!(
            "You see actor #{} at offset ({}, {}, {}).",
            actor.id, actor.position.x, actor.position.y, actor.position.z
        ));
    }
    for cell in &o.visible_cells {
        if let Some(door) = &cell.door {
            lines.push(format!(
                "You see {} {} (#{}), at offset ({}, {}, {}).{}",
                if door.open { "an open" } else { "a closed" },
                safe(&door.name),
                door.id,
                cell.position.x,
                cell.position.y,
                cell.position.z,
                if door.reachable { " Within reach." } else { "" }
            ));
        }
        if cell.stairs_up || cell.stairs_down {
            lines.push(format!(
                "Stairs {} at offset ({}, {}, {}).",
                if cell.stairs_up && cell.stairs_down {
                    "up and down"
                } else if cell.stairs_up {
                    "up"
                } else {
                    "down"
                },
                cell.position.x,
                cell.position.y,
                cell.position.z
            ));
        }
    }
    if state.wizard_game {
        lines.insert(0, "*** WIZARD GAME — permanently marked ***".into());
    }
    lines.join("\n")
}

/// An omitted quantity means the whole selected stack.
pub fn item_quantity(noun: &str) -> Result<(Option<u64>, &str), String> {
    let (first, rest) = word(noun);
    if first == "all" && !rest.is_empty() {
        return Ok((None, rest));
    }
    if first.chars().all(|c| c.is_ascii_digit()) && !first.is_empty() {
        let quantity = first
            .parse::<u64>()
            .ok()
            .filter(|q| *q > 0)
            .ok_or("Quantity must be a positive integer.")?;
        if rest.is_empty() {
            return Err("Which item?".into());
        }
        Ok((Some(quantity), rest))
    } else {
        Ok((None, noun))
    }
}

pub fn item_matches(noun: &str, item: &tor_protocol::ItemView) -> bool {
    if let Some(id) = noun.strip_prefix('#') {
        return id.parse::<ItemTarget>() == Ok(item.id);
    }
    let name = item.name.to_lowercase();
    let noun = noun.strip_prefix("the ").unwrap_or(noun);
    !noun.is_empty()
        && (name == noun
            || name.ends_with(&format!(" {noun}"))
            || format!("{name}s") == noun
            || format!("{name}s").ends_with(&format!(" {noun}")))
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
            .map(|i| format!("{} x {} (#{})", i.quantity, safe(&i.name), i.id))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn stats(state: &StateView) -> String {
    let mut lines = tor_client_common::stats::lines(state.observation.combat.as_ref());
    let holds = tor_client_common::holds::render_holds(&state.observation);
    if !holds.is_empty() {
        lines.push("Holds (escape hold <number|#token|name>):".into());
        lines.extend(holds);
    }
    lines.join("\n")
}

pub fn history(entry: &HistoryEntry) -> String {
    let content = match &entry.content {
        HistoryContent::PlaceRenamed { name, .. } => format!("Place named {}.", safe(name)),
        HistoryContent::Travel { .. } => "Travel requested.".into(),
        HistoryContent::Wizard { summary, .. } => safe(summary),
        HistoryContent::Action {
            event: Event::EscapeStarted { .. },
            ..
        } => "You begin trying to escape a hold.".into(),
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
    if text.trim().is_empty() {
        return Err("Enter a developer command after wizard.".into());
    }
    Ok(Input::Command(Command::Wizard {
        expected_revision,
        operation: text.into(),
    }))
}

/// Listing order is deterministic within a disclosed snapshot; names need not be unique.
pub fn places(state: &StateView) -> String {
    if state.observation.places.is_empty() {
        return "You have not discovered any places yet.".into();
    }
    state
        .observation
        .places
        .iter()
        .enumerate()
        .map(|(i, place)| {
            let visible = state
                .observation
                .visible_cells
                .iter()
                .any(|c| c.key == place.key && !c.wall);
            // Names the game made up are left unsaid.
            let name = match place.origin {
                PlaceNameOrigin::Invented => "An unnamed place".to_owned(),
                _ => safe(&place.name),
            };
            format!(
                "{}. {name} ({})",
                i + 1,
                if visible { "in sight" } else { "remembered" }
            )
        })
        .chain(
            state
                .observation
                .places
                .iter()
                .any(|p| p.origin == PlaceNameOrigin::Invented)
                .then(|| "Name one with name <number> <name>.".to_owned()),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

fn rename_place(rest: &str, state: &StateView) -> Result<Input, String> {
    let (number, name) = word(rest);
    let place = number
        .parse::<usize>()
        .ok()
        .and_then(|n| n.checked_sub(1))
        .and_then(|i| state.observation.places.get(i))
        .ok_or("Use places, then name <place number> <new name>.")?;
    if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
        return Err("Names need 1–80 UTF-8 bytes and no control characters.".into());
    }
    Ok(Input::Command(Command::RenamePlace {
        expected_revision: state.revision,
        key: place.key.clone(),
        name: name.into(),
    }))
}
