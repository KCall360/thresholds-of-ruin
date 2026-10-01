//! Adventure presentation and intentions, using only the disclosed observer scene.
use std::collections::{BTreeMap, BTreeSet, VecDeque};

use tor_client_common::{surfaces, AssetTable, Palette};
use tor_protocol::*;

use crate::{parse, parse_direction, safe, Input};

pub const HELP: &str = "attack <actor>, places, name <place number> <new name>, look (l), examine <thing> (x), inventory (i), get/drop [quantity] <thing>, open/close <door>, go to <thing>, north/east/south/west/ne/se/sw/nw/up/down, wait, stop, quit.\nAnswer a question with a name or its number. You can type stop while walking.";
pub const SESSION_HELP: &str = "control, release, sync, save, history, note <text>, bookmark <text>.\nstep <direction> makes one careful step. Developer commands require wizard authority.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Intent {
    Look,
    Say(String),
    Action(Action),
    Travel {
        destination: String,
        take: Option<(u64, Option<u64>)>,
        door: Option<(u64, bool)>,
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
    door: Option<u64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Verbosity {
    #[default]
    Brief,
    Verbose,
    Superbrief,
}

#[derive(Default)]
pub struct Dialogue {
    choices: Option<(u64, Vec<Choice>)>,
    item: Option<u64>,
    door: Option<u64>,
    pub queue: VecDeque<String>,
    pub verbosity: Verbosity,
    pub visited_places: BTreeSet<String>,
    pub last_command: Option<String>,
}

impl Dialogue {
    /// A setup/rewind boundary discards conversational references to old state.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// [`Dialogue::interpret_with`] without a palette: every thing keeps its
    /// disclosed material or name.
    pub fn interpret(&mut self, line: &str, state: &StateView) -> Intent {
        self.interpret_with(line, state, &Palette::default())
    }

    /// Interpret a line against the state in hand. Surfaces are described by
    /// their asset words where the palette holds their assets.
    pub fn interpret_with(&mut self, line: &str, state: &StateView, palette: &Palette) -> Intent {
        let trimmed = line.trim();
        let normalized = trimmed.to_lowercase();
        if normalized == "again" || normalized == "g" {
            if let Some(prev) = self.last_command.clone() {
                return self.interpret_with(&prev, state, palette);
            } else {
                return Intent::Say("There is no previous command to repeat.".into());
            }
        }
        if !trimmed.is_empty() {
            self.last_command = Some(trimmed.to_string());
        }
        let tokens = crate::parser::tokenize(line);
        let sentences = crate::parser::split_sentences(&tokens);
        if sentences.len() > 1 {
            for s in &sentences[1..] {
                let cmd = s.iter().map(|t| t.text()).collect::<Vec<_>>().join(" ");
                if !cmd.trim().is_empty() {
                    self.queue.push_back(cmd);
                }
            }
            let first = sentences[0]
                .iter()
                .map(|t| t.text())
                .collect::<Vec<_>>()
                .join(" ");
            return self.interpret_single(&first, state, palette);
        }
        self.interpret_single(line, state, palette)
    }

    pub fn interpret_single(&mut self, line: &str, state: &StateView, palette: &Palette) -> Intent {
        let normalized = line.trim().to_lowercase();
        if let Some((revision, choices)) = self.choices.take() {
            if revision == state.revision {
                let ordinal = crate::parser::lexicon::parse_ordinal(&normalized);
                let matches: Vec<_> = choices
                    .iter()
                    .enumerate()
                    .filter(|(i, c)| {
                        normalized.parse::<usize>() == Ok(i + 1)
                            || ordinal == Some(i + 1)
                            || noun_matches(&normalized, &c.label)
                    })
                    .map(|(_, c)| c.clone())
                    .collect();
                if matches.len() == 1 {
                    self.item = matches[0].item;
                    self.door = matches[0].door;
                    return matches[0].intent.clone();
                }
            }
        }
        let tokens = crate::parser::tokenize(line);
        if let Ok(cmd) = crate::parser::match_sentence(&tokens) {
            match cmd {
                crate::parser::ParsedCommand::Intransitive { verb } => match verb {
                    crate::parser::Verb::Look => return Intent::Look,
                    crate::parser::Verb::Inventory => return Intent::Say(inventory(state)),
                    crate::parser::Verb::Wait => return Intent::Action(Action::Wait),
                    crate::parser::Verb::Quit => return Intent::Tools(Input::Quit),
                    crate::parser::Verb::Stop => return Intent::Stop,
                    crate::parser::Verb::Again => {
                        if let Some(prev) = self.last_command.clone() {
                            return self.interpret_single(&prev, state);
                        } else {
                            return Intent::Say("There is no previous command to repeat.".into());
                        }
                    }
                    crate::parser::Verb::Help => return Intent::Say(HELP.into()),
                    crate::parser::Verb::Diagnose => {
                        return Intent::Say(crate::narrative::diagnose(state));
                    }
                    crate::parser::Verb::Talk => {
                        return Intent::Say("Who do you want to talk to?".into());
                    }
                    crate::parser::Verb::Listen => {
                        return Intent::Say(
                            crate::narrative::examine_scenery("sound", state)
                                .unwrap_or_else(|| "All is quiet.".into()),
                        )
                    }
                    crate::parser::Verb::Smell => {
                        return Intent::Say(
                            crate::narrative::examine_scenery("smell", state)
                                .unwrap_or_else(|| "The air carries no distinct scent.".into()),
                        )
                    }
                    crate::parser::Verb::Search => {
                        return Intent::Say(crate::narrative::search(state))
                    }
                    crate::parser::Verb::Verbose => {
                        self.verbosity = Verbosity::Verbose;
                        return Intent::Say("Maximum verbosity.".into());
                    }
                    crate::parser::Verb::Brief => {
                        self.verbosity = Verbosity::Brief;
                        return Intent::Say("Brief descriptions.".into());
                    }
                    crate::parser::Verb::Superbrief => {
                        self.verbosity = Verbosity::Superbrief;
                        return Intent::Say("Superbrief descriptions.".into());
                    }
                    _ => {}
                },
                crate::parser::ParsedCommand::Directional { direction } => {
                    let choices = destinations(state, direction)
                        .into_iter()
                        .map(|d| Choice {
                            label: d.label.clone(),
                            item: None,
                            door: None,
                            intent: Intent::Travel {
                                destination: d.key,
                                take: None,
                                door: None,
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
                crate::parser::ParsedCommand::Step { direction } => {
                    return Intent::Action(Action::Move { direction });
                }
                crate::parser::ParsedCommand::Stop => return Intent::Stop,
                crate::parser::ParsedCommand::Again => {
                    if let Some(prev) = self.last_command.clone() {
                        return self.interpret_single(&prev, state);
                    } else {
                        return Intent::Say("There is no previous command to repeat.".into());
                    }
                }
                crate::parser::ParsedCommand::Help { topic } => {
                    if topic.as_deref() == Some("session") {
                        return Intent::Say(SESSION_HELP.into());
                    }
                    return Intent::Say(HELP.into());
                }
                crate::parser::ParsedCommand::Say { text } => return Intent::Say(text),
                crate::parser::ParsedCommand::Ditransitive {
                    verb,
                    direct,
                    preposition,
                    indirect,
                } => {
                    if verb == crate::parser::Verb::Attack
                        && preposition == crate::parser::Preposition::With
                    {
                        let carried = state.observation.inventory.iter().any(|item| {
                            crate::item_matches(&indirect.raw, item)
                                || noun_matches(&indirect.raw, &item.name)
                                || indirect
                                    .head
                                    .as_deref()
                                    .is_some_and(|h| noun_matches(h, &item.name))
                        });
                        if !carried {
                            return Intent::Say(format!(
                                "You don't have the {}.",
                                safe(&indirect.raw)
                            ));
                        }
                        let target_noun = &direct.raw;
                        let mut actors: Vec<_> = state
                            .observation
                            .visible_actors
                            .iter()
                            .filter(|a| {
                                a.id != state.observation.actor
                                    && (noun_matches(target_noun, &a.name)
                                        || direct
                                            .head
                                            .as_deref()
                                            .is_some_and(|h| noun_matches(h, &a.name))
                                        || target_noun
                                            .strip_prefix('#')
                                            .and_then(|s| s.parse::<u64>().ok())
                                            == Some(a.id.0))
                            })
                            .collect();
                        actors.sort_by_key(|a| a.id);
                        actors.dedup_by_key(|a| a.id);
                        let choices = actors
                            .into_iter()
                            .map(|a| Choice {
                                label: format!("{} (#{})", a.name, a.id.0),
                                intent: Intent::Action(Action::Attack { target: a.id }),
                                item: None,
                                door: None,
                            })
                            .collect();
                        return self.choose(
                            choices,
                            state.revision,
                            "No matching actor is visible.",
                        );
                    } else if (verb == crate::parser::Verb::Open
                        || verb == crate::parser::Verb::Unlock)
                        && preposition == crate::parser::Preposition::With
                    {
                        let has_key = state.observation.inventory.iter().any(|item| {
                            crate::item_matches(&indirect.raw, item)
                                || noun_matches(&indirect.raw, &item.name)
                                || indirect
                                    .head
                                    .as_deref()
                                    .is_some_and(|h| noun_matches(h, &item.name))
                        });
                        if !has_key {
                            return Intent::Say(format!(
                                "You don't have the {}.",
                                safe(&indirect.raw)
                            ));
                        }
                        return self.object(&direct.raw, state, palette, "open");
                    } else if verb == crate::parser::Verb::Take
                        && (preposition == crate::parser::Preposition::From
                            || preposition == crate::parser::Preposition::Off)
                    {
                        return self.object(&direct.raw, state, palette, "take");
                    } else if verb == crate::parser::Verb::Put
                        && matches!(
                            preposition,
                            crate::parser::Preposition::In | crate::parser::Preposition::On
                        )
                    {
                        let carried = state.observation.inventory.iter().find(|item| {
                            crate::item_matches(&direct.raw, item)
                                || noun_matches(&direct.raw, &item.name)
                                || direct
                                    .head
                                    .as_deref()
                                    .is_some_and(|h| noun_matches(h, &item.name))
                        });
                        let Some(item) = carried else {
                            return Intent::Say(format!(
                                "You don't have the {}.",
                                safe(&direct.raw)
                            ));
                        };
                        let ind_str = indirect.raw.to_lowercase();
                        if ind_str.contains("floor") || ind_str.contains("ground") {
                            return Intent::Action(Action::Drop {
                                item: item.id,
                                quantity: direct.quantity,
                            });
                        }
                        return Intent::Say(format!(
                            "You cannot put the {} {} the {}.",
                            safe(&item.name),
                            preposition.as_str(),
                            safe(&indirect.raw)
                        ));
                    } else if verb == crate::parser::Verb::Give
                        && preposition == crate::parser::Preposition::To
                    {
                        let carried = state.observation.inventory.iter().find(|item| {
                            crate::item_matches(&direct.raw, item)
                                || noun_matches(&direct.raw, &item.name)
                                || direct
                                    .head
                                    .as_deref()
                                    .is_some_and(|h| noun_matches(h, &item.name))
                        });
                        let Some(item) = carried else {
                            return Intent::Say(format!(
                                "You don't have the {}.",
                                safe(&direct.raw)
                            ));
                        };
                        let target_noun = &indirect.raw;
                        let actors: Vec<_> = state
                            .observation
                            .visible_actors
                            .iter()
                            .filter(|a| {
                                a.id != state.observation.actor
                                    && (noun_matches(target_noun, &a.name)
                                        || indirect
                                            .head
                                            .as_deref()
                                            .is_some_and(|h| noun_matches(h, &a.name))
                                        || target_noun
                                            .strip_prefix('#')
                                            .and_then(|s| s.parse::<u64>().ok())
                                            == Some(a.id.0))
                            })
                            .collect();
                        if actors.is_empty() {
                            return Intent::Say("No matching actor is visible.".into());
                        }
                        return Intent::Say(format!(
                            "The {} does not seem interested in the {}.",
                            safe(&actors[0].name),
                            safe(&item.name)
                        ));
                    } else if verb == crate::parser::Verb::Talk
                        && (preposition == crate::parser::Preposition::About
                            || preposition == crate::parser::Preposition::At
                            || preposition == crate::parser::Preposition::To)
                    {
                        return self.talk_to(&direct.raw, Some(&indirect.raw), state);
                    }
                }
                crate::parser::ParsedCommand::MultiTransitive { verb, direct_list } => {
                    if verb == crate::parser::Verb::Take && !direct_list.is_empty() {
                        for d in &direct_list[1..] {
                            self.queue.push_back(format!("take {}", d.raw));
                        }
                        return self.object(&direct_list[0].raw, state, palette, "take");
                    } else if verb == crate::parser::Verb::Drop && !direct_list.is_empty() {
                        for d in &direct_list[1..] {
                            self.queue.push_back(format!("drop {}", d.raw));
                        }
                        return self.object(&direct_list[0].raw, state, palette, "drop");
                    }
                }
                crate::parser::ParsedCommand::Transitive { verb, direct } => {
                    if direct.all {
                        if verb == crate::parser::Verb::Take {
                            let items: Vec<_> = state
                                .observation
                                .ground_items
                                .iter()
                                .filter(|g| in_current_place(state, g.position))
                                .collect();
                            if items.is_empty() {
                                return Intent::Say("There is nothing here to take.".into());
                            }
                            for g in &items[1..] {
                                self.queue.push_back(format!("take {}", g.item.name));
                            }
                            return self.object(&items[0].item.name, state, palette, "take");
                        } else if verb == crate::parser::Verb::Drop {
                            if state.observation.inventory.is_empty() {
                                return Intent::Say("You are not carrying anything.".into());
                            }
                            for item in &state.observation.inventory[1..] {
                                self.queue.push_back(format!("drop {}", item.name));
                            }
                            return self.object(
                                &state.observation.inventory[0].name,
                                state,
                                palette,
                                "drop",
                            );
                        }
                    }
                    match verb {
                        crate::parser::Verb::Examine => {
                            return self.object(&direct.raw, state, palette, "examine")
                        }
                        crate::parser::Verb::Take => {
                            return self.object(&direct.raw, state, palette, "take")
                        }
                        crate::parser::Verb::Drop => {
                            return self.object(&direct.raw, state, palette, "drop")
                        }
                        crate::parser::Verb::Open => {
                            return self.object(&direct.raw, state, palette, "open")
                        }
                        crate::parser::Verb::Close => {
                            return self.object(&direct.raw, state, palette, "close")
                        }
                        crate::parser::Verb::Go => {
                            return self.object(&direct.raw, state, palette, "go")
                        }
                        crate::parser::Verb::Search => {
                            return Intent::Say(crate::narrative::search(state))
                        }
                        crate::parser::Verb::Read => {
                            return self.object(&direct.raw, state, "read")
                        }
                        crate::parser::Verb::Drink => {
                            return self.object(&direct.raw, state, "drink")
                        }
                        crate::parser::Verb::Eat => return self.object(&direct.raw, state, "eat"),
                        crate::parser::Verb::Wear => {
                            return self.object(&direct.raw, state, "wear")
                        }
                        crate::parser::Verb::Wield => {
                            return self.object(&direct.raw, state, "wield")
                        }
                        crate::parser::Verb::Remove => {
                            return self.object(&direct.raw, state, "remove")
                        }
                        crate::parser::Verb::Push => {
                            return self.object(&direct.raw, state, "push")
                        }
                        crate::parser::Verb::Pull => {
                            return self.object(&direct.raw, state, "pull")
                        }
                        crate::parser::Verb::Turn => {
                            return self.object(&direct.raw, state, "turn")
                        }
                        crate::parser::Verb::Talk => return self.talk_to(&direct.raw, None, state),
                        crate::parser::Verb::Diagnose => {
                            if direct.raw == "me"
                                || direct.raw == "myself"
                                || direct.raw == "player"
                            {
                                return Intent::Say(crate::narrative::diagnose(state));
                            }
                            let target_noun = &direct.raw;
                            if let Some(actor) = state.observation.visible_actors.iter().find(|a| {
                                noun_matches(target_noun, &a.name)
                                    || direct
                                        .head
                                        .as_deref()
                                        .is_some_and(|h| noun_matches(h, &a.name))
                                    || target_noun
                                        .strip_prefix('#')
                                        .and_then(|s| s.parse::<u64>().ok())
                                        == Some(a.id.0)
                            }) {
                                if let Some(injury) =
                                    state.observation.combat.as_ref().and_then(|c| {
                                        c.actors.iter().find(|ca| ca.actor == actor.id)
                                    })
                                {
                                    return Intent::Say(format!(
                                        "{} looks {}.",
                                        safe(&actor.name),
                                        injury.injury
                                    ));
                                } else {
                                    return Intent::Say(format!(
                                        "{} appears uninjured.",
                                        safe(&actor.name)
                                    ));
                                }
                            }
                            return Intent::Say("No matching actor is visible.".into());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        let (verb, rest) = crate::word(&normalized);
        match (verb, rest) {
            ("attack" | "hit" | "fight", noun) => {
                let mut actors: Vec<_> = state
                    .observation
                    .visible_actors
                    .iter()
                    .filter(|a| {
                        a.id != state.observation.actor
                            && (noun_matches(noun, &a.name)
                                || noun.strip_prefix('#').and_then(|s| s.parse::<u64>().ok())
                                    == Some(a.id.0))
                    })
                    .collect();
                actors.sort_by_key(|a| a.id);
                actors.dedup_by_key(|a| a.id);
                let choices = actors
                    .into_iter()
                    .map(|a| Choice {
                        label: format!("{} (#{})", a.name, a.id.0),
                        intent: Intent::Action(Action::Attack { target: a.id }),
                        item: None,
                        door: None,
                    })
                    .collect();
                self.choose(choices, state.revision, "No matching actor is visible.")
            }
            ("look" | "l", "") => Intent::Look,
            ("inventory" | "i", "") => Intent::Say(inventory(state)),
            ("help", "session") => Intent::Say(SESSION_HELP.into()),
            ("help" | "?", _) => Intent::Say(HELP.into()),
            ("stop" | "cancel", "") => Intent::Stop,
            ("step", direction) => match parse_direction(direction) {
                Ok(direction) => Intent::Action(Action::Move { direction }),
                Err(_) => Intent::Say("Which direction would you like to step?".into()),
            },
            ("examine" | "x" | "inspect", noun) => self.object(noun, state, palette, "examine"),
            ("look", noun) if noun.starts_with("at ") => {
                self.object(&noun[3..], state, palette, "examine")
            }
            ("open" | "close", noun) => self.object(noun, state, palette, verb),
            ("take" | "get", noun) => self.object(noun, state, palette, "take"),
            ("drop", noun) => self.object(noun, state, palette, "drop"),
            ("go" | "approach", noun) if parse_direction(noun).is_err() => self.object(
                noun.strip_prefix("to ").unwrap_or(noun),
                state,
                palette,
                "go",
            ),
            ("listen" | "hear", _) => Intent::Say(
                crate::narrative::examine_scenery("sound", state)
                    .unwrap_or_else(|| "All is quiet.".into()),
            ),
            ("smell" | "sniff", _) => Intent::Say(
                crate::narrative::examine_scenery("smell", state)
                    .unwrap_or_else(|| "The air carries no distinct scent.".into()),
            ),
            ("search", _) => Intent::Say(crate::narrative::search(state)),
            ("verbose", "") => {
                self.verbosity = Verbosity::Verbose;
                Intent::Say("Maximum verbosity.".into())
            }
            ("brief", "") => {
                self.verbosity = Verbosity::Brief;
                Intent::Say("Brief descriptions.".into())
            }
            ("superbrief", "") => {
                self.verbosity = Verbosity::Superbrief;
                Intent::Say("Superbrief descriptions.".into())
            }
            ("diagnose", _) => {
                if rest.is_empty() || matches!(rest, "me" | "myself" | "player") {
                    Intent::Say(crate::narrative::diagnose(state))
                } else {
                    let mut actors: Vec<_> = state
                        .observation
                        .visible_actors
                        .iter()
                        .filter(|a| {
                            noun_matches(rest, &a.name)
                                || rest.strip_prefix('#').and_then(|s| s.parse::<u64>().ok())
                                    == Some(a.id.0)
                        })
                        .collect();
                    actors.sort_by_key(|a| a.id);
                    actors.dedup_by_key(|a| a.id);
                    if let Some(actor) = actors.first() {
                        if let Some(injury) = state
                            .observation
                            .combat
                            .as_ref()
                            .and_then(|c| c.actors.iter().find(|ca| ca.actor == actor.id))
                        {
                            Intent::Say(format!("{} looks {}.", safe(&actor.name), injury.injury))
                        } else {
                            Intent::Say(format!("{} appears uninjured.", safe(&actor.name)))
                        }
                    } else {
                        Intent::Say("No matching actor is visible.".into())
                    }
                }
            }
            ("read", noun) => self.object(noun, state, "read"),
            ("drink" | "quaff" | "sip", noun) => self.object(noun, state, "drink"),
            ("eat" | "taste" | "consume", noun) => self.object(noun, state, "eat"),
            ("wear" | "don", noun) => self.object(noun, state, "wear"),
            ("wield" | "equip" | "brandish", noun) => self.object(noun, state, "wield"),
            ("remove" | "doff" | "unequip", noun) => self.object(noun, state, "remove"),
            ("push" | "shove" | "press", noun) => self.object(noun, state, "push"),
            ("pull" | "drag" | "tug", noun) => self.object(noun, state, "pull"),
            ("turn" | "rotate" | "twist", noun) => self.object(noun, state, "turn"),
            ("talk" | "speak", noun) => {
                let target = noun.strip_prefix("to ").unwrap_or(noun);
                self.talk_to(target, None, state)
            }
            ("ask", rest) => {
                if let Some((target, topic)) = rest.split_once(" about ") {
                    self.talk_to(target, Some(topic), state)
                } else {
                    self.talk_to(rest, None, state)
                }
            }
            ("put", rest) => {
                if let Some(item_name) = rest.strip_prefix("on ") {
                    self.object(item_name, state, "wear")
                } else if let Some(item_name) = rest.strip_prefix("down ") {
                    self.object(item_name, state, "drop")
                } else if let Some((obj, target)) = rest.split_once(" on ") {
                    if target.contains("floor") || target.contains("ground") {
                        self.object(obj, state, "drop")
                    } else {
                        Intent::Say(format!(
                            "You cannot put the {} on the {}.",
                            safe(obj),
                            safe(target)
                        ))
                    }
                } else if let Some((obj, target)) = rest.split_once(" in ") {
                    if target.contains("floor") || target.contains("ground") {
                        self.object(obj, state, "drop")
                    } else {
                        Intent::Say(format!(
                            "You cannot put the {} in the {}.",
                            safe(obj),
                            safe(target)
                        ))
                    }
                } else {
                    self.object(rest, state, "drop")
                }
            }
            ("give", rest) => {
                if let Some((obj, target)) = rest.split_once(" to ") {
                    let carried = state.observation.inventory.iter().find(|item| {
                        crate::item_matches(obj, item) || noun_matches(obj, &item.name)
                    });
                    let Some(item) = carried else {
                        return Intent::Say(format!("You don't have the {}.", safe(obj)));
                    };
                    let actors: Vec<_> = state
                        .observation
                        .visible_actors
                        .iter()
                        .filter(|a| {
                            a.id != state.observation.actor
                                && (noun_matches(target, &a.name)
                                    || target.strip_prefix('#').and_then(|s| s.parse::<u64>().ok())
                                        == Some(a.id.0))
                        })
                        .collect();
                    if actors.is_empty() {
                        Intent::Say("No matching actor is visible.".into())
                    } else {
                        Intent::Say(format!(
                            "The {} does not seem interested in the {}.",
                            safe(&actors[0].name),
                            safe(&item.name)
                        ))
                    }
                } else {
                    Intent::Say("What do you want to give?".into())
                }
            }
            ("name", rest) => {
                let (target, _) = crate::word(rest);
                if matches!(target, "room" | "place" | "here") {
                    let (_, original_rest) = crate::word(line.trim());
                    let (_, original_name) = crate::word(original_rest);
                    if let Some(key) = crate::narrative::current_place_key(state) {
                        Intent::Tools(crate::Input::Command(Command::RenamePlace {
                            expected_revision: state.revision,
                            key: key.into(),
                            name: original_name.into(),
                        }))
                    } else {
                        Intent::Say("You cannot discern an anchor here to name.".into())
                    }
                } else {
                    match crate::parse(line, state) {
                        Ok(input) => Intent::Tools(input),
                        Err(e) => Intent::Say(e),
                    }
                }
            }
            ("note" | "annotate", _) => {
                let (_, original_text) = crate::word(line.trim());
                if !original_text.trim().is_empty() {
                    Intent::Tools(crate::Input::Command(Command::Annotate {
                        anchor: Anchor::State {
                            revision: state.revision,
                        },
                        text: original_text.into(),
                        source: ClientSource::User,
                        audience: Audience::Actor,
                        category: AnnotationCategory::Note,
                    }))
                } else {
                    Intent::Say("What note would you like to make?".into())
                }
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
                            door: None,
                            intent: Intent::Travel {
                                destination: d.key,
                                take: None,
                                door: None,
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
            [] => {
                self.queue.clear();
                Intent::Say(missing.into())
            }
            [choice] => {
                self.item = choice.item;
                self.door = choice.door;
                choice.intent.clone()
            }
            _ => {
                self.queue.clear();
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

    fn talk_to(&mut self, direct_noun: &str, topic: Option<&str>, state: &StateView) -> Intent {
        let noun = direct_noun.trim();
        if noun.is_empty() {
            return Intent::Say("Who do you want to talk to?".into());
        }
        if noun == "myself" || noun == "me" || noun == "self" {
            return Intent::Say("Talking to yourself is a sure sign of madness.".into());
        }
        let mut actors: Vec<_> = state
            .observation
            .visible_actors
            .iter()
            .filter(|a| {
                noun_matches(noun, &a.name)
                    || noun.strip_prefix('#').and_then(|s| s.parse::<u64>().ok()) == Some(a.id.0)
            })
            .collect();
        actors.sort_by_key(|a| a.id);
        actors.dedup_by_key(|a| a.id);
        if actors.is_empty() {
            return Intent::Say("No matching actor is visible.".into());
        }
        let choices: Vec<_> = actors
            .into_iter()
            .map(|a| {
                let label = format!("{} (#{})", a.name, a.id.0);
                let response = if a.id == state.observation.actor {
                    "Talking to yourself is a sure sign of madness.".into()
                } else if let Some(t) = topic {
                    format!(
                        "The {} remains silent, offering no response about the {}.",
                        safe(&a.name),
                        safe(t)
                    )
                } else {
                    format!("The {} glares warily and offers no reply.", safe(&a.name))
                };
                Choice {
                    label,
                    intent: Intent::Say(response),
                    item: None,
                    door: None,
                }
            })
            .collect();
        self.choose(choices, state.revision, "No matching actor is visible.")
    }

    fn object(&mut self, noun: &str, state: &StateView, palette: &Palette, verb: &str) -> Intent {
        let (quantity, noun) = if matches!(verb, "take" | "drop") {
            match crate::item_quantity(noun) {
                Ok(value) => value,
                Err(error) => return Intent::Say(error),
            }
        } else {
            (None, noun)
        };
        if verb == "examine" {
            if let Some(desc) = crate::narrative::examine_scenery(noun, state) {
                return Intent::Say(desc);
            }
        } else if verb == "read"
            && matches!(
                noun,
                "wall"
                    | "walls"
                    | "floor"
                    | "the walls"
                    | "the floor"
                    | "ceiling"
                    | "the ceiling"
                    | "air"
                    | "room"
                    | "chamber"
                    | "place"
                    | "here"
            )
        {
            return Intent::Say("There is nothing written there.".into());
        }
        if verb == "examine"
            && matches!(
                noun,
                "wall" | "walls" | "floor" | "the walls" | "the floor" | "ceiling" | "the ceiling"
            )
        {
            let walls = noun.contains("wall");
            let ceiling = noun.contains("ceiling");
            let cells = &state.observation.visible_cells;
            let roles = surfaces::roles_by(cells, |cell| surface(palette, cell));
            let materials = if walls {
                roles.walls
            } else if ceiling {
                roles.ceilings
            } else if roles.floors.is_empty() {
                // Raw diagnostic regions have no solid floor; their open cells
                // carry a cosmetic material instead.
                cells
                    .iter()
                    .filter_map(|c| open_surface(palette, c))
                    .collect()
            } else {
                roles.floors
            };
            return Intent::Say(if materials.is_empty() {
                "You cannot see that here.".into()
            } else {
                format!(
                    "The visible {} {} made of {}.",
                    if walls {
                        "walls"
                    } else if ceiling {
                        "ceiling"
                    } else {
                        "floor"
                    },
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
            if matches!(verb, "open" | "close") {
                continue;
            }
            if !(noun.is_empty()
                || crate::item_matches(noun, item)
                || noun_matches(noun, &item.name)
                || noun == "it" && self.item == Some(id))
            {
                continue;
            }
            let carried = state.observation.inventory.iter().any(|i| i.id == id);
            if verb == "drop" && !carried {
                continue;
            }
            let intent = if verb == "drop" {
                Intent::Action(Action::Drop { item: id, quantity })
            } else if verb == "examine" {
                Intent::Say(if item.description.is_empty() {
                    "You notice no further distinguishing details.".into()
                } else {
                    safe(&item.description)
                })
            } else if verb == "read" {
                Intent::Say(if item.description.is_empty() {
                    format!("There is nothing written on the {}.", safe(&item.name))
                } else {
                    safe(&item.description)
                })
            } else if verb == "drink" {
                if !carried {
                    Intent::Say("You must take it first.".into())
                } else {
                    let lower = item.name.to_lowercase();
                    if lower.contains("potion")
                        || lower.contains("flask")
                        || lower.contains("elixir")
                        || lower.contains("brew")
                        || lower.contains("water")
                        || lower.contains("ale")
                        || lower.contains("wine")
                        || lower.contains("draught")
                        || lower.contains("bottle")
                    {
                        Intent::Say(format!(
                            "You take a sip of the {}. It is refreshing, though it has no further effect right now.",
                            safe(&item.name)
                        ))
                    } else {
                        Intent::Say(format!("You cannot drink the {}.", safe(&item.name)))
                    }
                }
            } else if verb == "eat" {
                if !carried {
                    Intent::Say("You must take it first.".into())
                } else {
                    let lower = item.name.to_lowercase();
                    if lower.contains("ration")
                        || lower.contains("bread")
                        || lower.contains("food")
                        || lower.contains("meat")
                        || lower.contains("fruit")
                        || lower.contains("apple")
                        || lower.contains("berry")
                        || lower.contains("herb")
                        || lower.contains("mushroom")
                        || lower.contains("leaf")
                    {
                        Intent::Say(format!(
                            "You sample the {}. It sustains you, though it has no further effect right now.",
                            safe(&item.name)
                        ))
                    } else {
                        Intent::Say(format!("The {} is not edible.", safe(&item.name)))
                    }
                }
            } else if verb == "wear" {
                if !carried {
                    Intent::Say("You don't have that.".into())
                } else {
                    Intent::Say(format!("You put on the {}.", safe(&item.name)))
                }
            } else if verb == "wield" {
                if !carried {
                    Intent::Say("You don't have that.".into())
                } else {
                    Intent::Say(format!("You ready the {} for combat.", safe(&item.name)))
                }
            } else if verb == "remove" {
                if !carried {
                    Intent::Say("You are not wearing that.".into())
                } else {
                    Intent::Say(format!("You take off the {}.", safe(&item.name)))
                }
            } else if verb == "push" {
                Intent::Say(format!(
                    "Pushing the {} achieves nothing.",
                    safe(&item.name)
                ))
            } else if verb == "pull" {
                Intent::Say(format!(
                    "Pulling the {} achieves nothing.",
                    safe(&item.name)
                ))
            } else if verb == "turn" {
                Intent::Say(format!(
                    "Turning the {} achieves nothing.",
                    safe(&item.name)
                ))
            } else if carried {
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
                        Intent::Action(Action::Take { item: id, quantity })
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
                        take: (verb == "take").then_some((id, quantity)),
                        door: None,
                        label: format!("the {}", safe(&item.name)),
                        direction: None,
                    }
                }
            };
            choices.push(Choice {
                label: format!("{} (count {})", item.name, item.quantity),
                intent,
                item: Some(id),
                door: None,
            });
        }
        let mut doors = BTreeSet::new();
        for cell in &state.observation.visible_cells {
            let Some(door) = &cell.door else {
                continue;
            };
            let label = format!("{} {}", door.name, whereabouts(cell.position));
            if !doors.insert(door.id)
                || !(noun.is_empty()
                    || noun_matches(noun, &label)
                    || noun == "it" && self.door == Some(door.id))
            {
                continue;
            }
            let target_open = match verb {
                "open" | "push" => true,
                "close" | "pull" => false,
                _ => verb == "open",
            };
            let intent = match verb {
                "examine" => Intent::Say(format!(
                    "{} It is {}.",
                    safe(&door.description),
                    if door.open { "open" } else { "closed" }
                )),
                "read" => Intent::Say(if door.description.is_empty() {
                    format!(
                        "There are no markings or inscriptions on the {}.",
                        safe(&door.name)
                    )
                } else {
                    safe(&door.description)
                }),
                "take" => Intent::Say("You cannot pick up a door.".into()),
                "turn" => Intent::Say("Turning the handle does nothing unusual.".into()),
                "drink" | "eat" => Intent::Say("That is not edible!".into()),
                "wear" | "wield" | "remove" => {
                    Intent::Say("You cannot wear or wield a door.".into())
                }
                "open" | "close" | "push" | "pull" if door.open == target_open => {
                    Intent::Say(format!(
                        "It is already {}.",
                        if target_open { "open" } else { "closed" }
                    ))
                }
                "open" | "close" | "push" | "pull" if door.reachable => {
                    Intent::Action(Action::SetDoor {
                        door: door.id,
                        open: target_open,
                    })
                }
                "go" if door.reachable => Intent::Say("You are already beside it.".into()),
                _ => {
                    let destination = state
                        .observation
                        .visible_cells
                        .iter()
                        .filter(|c| {
                            door.approaches.contains(&c.key)
                                && !c.wall
                                && c.door.as_ref().is_none_or(|d| d.open)
                        })
                        .min_by_key(|c| (distance(c.position), &c.key));
                    match destination {
                        Some(c) => Intent::Travel {
                            destination: c.key.clone(),
                            take: None,
                            door: matches!(verb, "open" | "close" | "push" | "pull")
                                .then_some((door.id, target_open)),
                            label: format!("the {}", safe(&door.name)),
                            direction: None,
                        },
                        None => Intent::Say("You cannot see a place to approach it from.".into()),
                    }
                }
            };
            choices.push(Choice {
                label,
                intent,
                item: None,
                door: Some(door.id),
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
                        door: None,
                    });
                }
            }
        } else if verb == "read" {
            let mut actors = BTreeSet::new();
            for actor in &state.observation.visible_actors {
                if noun_matches(noun, &actor.name) && actors.insert(actor.id) {
                    choices.push(Choice {
                        label: format!("{} {}", safe(&actor.name), whereabouts(actor.position)),
                        intent: Intent::Say("There is nothing written on them.".into()),
                        item: None,
                        door: None,
                    });
                }
            }
        } else if matches!(verb, "push" | "pull") {
            let mut actors = BTreeSet::new();
            for actor in &state.observation.visible_actors {
                if noun_matches(noun, &actor.name) && actors.insert(actor.id) {
                    choices.push(Choice {
                        label: format!("{} {}", safe(&actor.name), whereabouts(actor.position)),
                        intent: Intent::Say("They wouldn't appreciate that.".into()),
                        item: None,
                        door: None,
                    });
                }
            }
        } else if verb == "turn" {
            let mut actors = BTreeSet::new();
            for actor in &state.observation.visible_actors {
                if noun_matches(noun, &actor.name) && actors.insert(actor.id) {
                    choices.push(Choice {
                        label: format!("{} {}", safe(&actor.name), whereabouts(actor.position)),
                        intent: Intent::Say("They stare back at you.".into()),
                        item: None,
                        door: None,
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

pub(crate) fn distance(p: Position) -> u64 {
    u64::from(p.x.unsigned_abs()) + u64::from(p.y.unsigned_abs()) + u64::from(p.z.unsigned_abs())
}

pub fn direction_name(d: Direction) -> &'static str {
    match d {
        Direction::North => "north",
        Direction::East => "east",
        Direction::South => "south",
        Direction::West => "west",
        Direction::NorthEast => "northeast",
        Direction::SouthEast => "southeast",
        Direction::SouthWest => "southwest",
        Direction::NorthWest => "northwest",
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
    } else if u64::from(p.x.unsigned_abs()) * 2 >= u64::from(p.y.unsigned_abs())
        && u64::from(p.y.unsigned_abs()) * 2 >= u64::from(p.x.unsigned_abs())
    {
        Some(match (p.x > 0, p.y > 0) {
            (true, false) => Direction::NorthEast,
            (true, true) => Direction::SouthEast,
            (false, true) => Direction::SouthWest,
            (false, false) => Direction::NorthWest,
        })
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

/// Words for the assets this client knows. A lookup falls back through dotted
/// prefixes, so `terrain.floor.stone`, which has no entry, reads as
/// `terrain.floor`. A thing whose asset has no word, or isn't in the palette,
/// keeps its disclosed material or name.
pub fn words() -> &'static AssetTable<&'static str> {
    static WORDS: std::sync::OnceLock<AssetTable<&'static str>> = std::sync::OnceLock::new();
    WORDS.get_or_init(|| {
        AssetTable::new([
            ("terrain.floor", "flagstone"),
            ("terrain.floor.cave", "packed earth"),
            ("terrain.floor.marble", "polished marble"),
            ("terrain.wall", "dressed stone"),
            ("terrain.wall.cave", "rough cave rock"),
            ("terrain.wall.marble", "polished marble"),
            ("creature", "creature"),
            ("creature.rat", "rat"),
        ])
    })
}

/// A solid cell's word: its asset's, or its material.
fn surface<'a>(palette: &Palette, cell: &'a CellView) -> &'a str {
    palette
        .resolve(words(), cell.asset.as_deref())
        .copied()
        .unwrap_or_else(|| surfaces::material(cell))
}

/// What an open cell itself shows underfoot in raw diagnostic regions, which
/// have no solid floor: its asset's word, or its cosmetic material.
fn open_surface<'a>(palette: &Palette, cell: &'a CellView) -> Option<&'a str> {
    if cell.wall {
        return None;
    }
    palette
        .resolve(words(), cell.asset.as_deref())
        .copied()
        .or_else(|| (!cell.material.is_empty()).then_some(cell.material.as_str()))
}

/// The floor under an open cell: the seen solid cell below it, or, in raw
/// diagnostic regions without one, what the open cell itself shows.
pub(crate) fn floor_material<'a>(cells: &'a [CellView], cell: &'a CellView) -> Option<&'a str> {
    floor_material_with(&Palette::default(), cells, cell)
}

/// The floor under an open cell: the seen solid cell below it, or, in raw
/// diagnostic regions without one, what the open cell itself shows.
pub(crate) fn floor_material_with<'a>(
    palette: &Palette,
    cells: &'a [CellView],
    cell: &'a CellView,
) -> Option<&'a str> {
    surfaces::floor_below(cells, cell.position)
        .map(|floor| surface(palette, floor))
        .or_else(|| open_surface(palette, cell))
}

fn indefinite(name: &str) -> String {
    let article = if name.starts_with(['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U']) {
        "an"
    } else {
        "a"
    };
    format!("{article} {}", safe(name))
}

/// [`describe_with`] without a palette: every thing keeps its disclosed
/// material or name.
pub fn describe(state: &StateView) -> String {
    describe_with(state, &Palette::default())
}

/// The scene in prose. Floors, walls and unnamed figures are described by
/// their asset words where the palette holds their assets.
pub fn describe_with(state: &StateView, palette: &Palette) -> String {
    let o = &state.observation;
    let mut lines = Vec::new();
    if let Some(c) = &o.combat {
        lines.push(tor_client_common::narration::combat_status(c));
        lines.extend(c.objective.clone());
    }
    if state.wizard_game {
        lines.push("*** WIZARD GAME — permanently marked ***".into());
    }
    if let Some(title) = crate::narrative::place_title(state) {
        lines.push(title);
    }
    let floor = o
        .visible_cells
        .iter()
        .find(|c| distance(c.position) == 0 && !c.wall);
    lines.push(floor.map_or_else(
        || "Your surroundings".into(),
        |c| {
            floor_material_with(palette, &o.visible_cells, c).map_or_else(
                || "You stand in an open space.".into(),
                |m| format!("You stand in a space with a {} floor.", safe(m)),
            )
        },
    ));
    let walls: BTreeSet<_> = surfaces::roles_by(&o.visible_cells, |cell| surface(palette, cell))
        .walls
        .into_iter()
        .map(safe)
        .collect();
    if !walls.is_empty() {
        lines.push(format!(
            "You can see walls of {}.",
            walls.into_iter().collect::<Vec<_>>().join(" and ")
        ));
    }
    if let Some(sensory) = crate::narrative::sensory_atmosphere(state) {
        lines.push(sensory);
    }
    let mut seen = BTreeSet::new();
    for item in &o.ground_items {
        if seen.insert(item.item.id) {
            lines.push(format!(
                "You see {} {}.",
                if item.item.quantity == 1 {
                    indefinite(&item.item.name)
                } else {
                    format!("{} x {}", item.item.quantity, safe(&item.item.name))
                },
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
    let mut doors = BTreeSet::new();
    for cell in &o.visible_cells {
        if let Some(door) = &cell.door {
            if doors.insert(door.id) {
                lines.push(format!(
                    "You see {} {}.",
                    indefinite(&format!(
                        "{} {}",
                        if door.open { "open" } else { "closed" },
                        door.name
                    )),
                    whereabouts(cell.position)
                ));
            }
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
                        palette
                            .resolve(words(), actor.asset.as_deref())
                            .copied()
                            .unwrap_or("figure")
                    } else {
                        &actor.name
                    })
                },
                whereabouts(actor.position)
            ));
            if let Some(injury) = o
                .combat
                .as_ref()
                .and_then(|c| c.actors.iter().find(|c| c.actor == actor.id))
            {
                lines.push(format!("{} looks {}.", safe(&actor.name), injury.injury));
            }
        }
    }
    let ways: Vec<_> = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::NorthEast,
        Direction::SouthEast,
        Direction::SouthWest,
        Direction::NorthWest,
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
    if !o.ready && !o.combat.as_ref().is_some_and(|c| c.terminal) {
        lines.push("For now, you must wait.".into());
    }
    lines.join("\n")
}

pub fn describe_brief(state: &StateView) -> String {
    describe_brief_with(state, &Palette::default())
}

pub fn describe_brief_with(state: &StateView, palette: &Palette) -> String {
    let o = &state.observation;
    let mut lines = Vec::new();
    if let Some(c) = &o.combat {
        lines.push(tor_client_common::narration::combat_status(c));
        lines.extend(c.objective.clone());
    }
    if let Some(title) = crate::narrative::place_title(state) {
        lines.push(title);
    }
    let mut seen = BTreeSet::new();
    for item in &o.ground_items {
        if seen.insert(item.item.id) {
            lines.push(format!(
                "You see {} {}.",
                if item.item.quantity == 1 {
                    indefinite(&item.item.name)
                } else {
                    format!("{} x {}", item.item.quantity, safe(&item.item.name))
                },
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
    let mut doors = BTreeSet::new();
    for cell in &o.visible_cells {
        if let Some(door) = &cell.door {
            if doors.insert(door.id) {
                lines.push(format!(
                    "You see {} {}.",
                    indefinite(&format!(
                        "{} {}",
                        if door.open { "open" } else { "closed" },
                        door.name
                    )),
                    whereabouts(cell.position)
                ));
            }
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
                        palette
                            .resolve(words(), actor.asset.as_deref())
                            .copied()
                            .unwrap_or("figure")
                    } else {
                        &actor.name
                    })
                },
                whereabouts(actor.position)
            ));
            if let Some(injury) = o
                .combat
                .as_ref()
                .and_then(|c| c.actors.iter().find(|c| c.actor == actor.id))
            {
                lines.push(format!("{} looks {}.", safe(&actor.name), injury.injury));
            }
        }
    }
    let ways: Vec<_> = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::NorthEast,
        Direction::SouthEast,
        Direction::SouthWest,
        Direction::NorthWest,
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
    if !o.ready && !o.combat.as_ref().is_some_and(|c| c.terminal) {
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
                .map(|i| format!("{} x {}", i.quantity, safe(&i.name)))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

pub fn event(entry: &HistoryEntry, state: &StateView) -> String {
    match &entry.content {
        HistoryContent::Action { event, .. } => {
            tor_client_common::narration::action(event, &state.observation)
        }
        HistoryContent::Annotation { text, .. } => format!("Note: {}", safe(text)),
        HistoryContent::Wizard { summary, .. } => safe(summary),
        HistoryContent::PlaceRenamed { name, .. } => format!("Place named {}.", safe(name)),
        HistoryContent::Travel { .. } => "You set off.".into(),
    }
}
