//! Verb semantics: what a resolved command means in the game.
//!
//! Many verbs share one game action, and one verb chooses its action by what
//! it acts on. Verbs the game has no rules for yet are recognized and refused
//! plainly, so supporting one later is one more entry here.
use tor_client_common::narration;
use tor_protocol::*;

use super::{
    prose,
    resolve::{phrase, resolve, Domain, Referents, Resolution},
    scene::{Key, Kind, Referent, Scene, Surface},
};
use crate::{
    parser::{NounPhrase, ParsedCommand, Preposition, SessionCommand, Verb},
    safe, Input,
};

/// Something the character sets out to do. Each runs as one or more game
/// actions; see [`super::turn`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goal {
    Take {
        item: u64,
        quantity: Option<u64>,
    },
    Drop {
        item: u64,
        quantity: Option<u64>,
    },
    Door {
        door: u64,
        open: bool,
    },
    Attack {
        target: ActorId,
    },
    Approach {
        target: Key,
    },
    /// Head for a way onward in a direction.
    Go {
        direction: Direction,
        destination: String,
    },
    Step {
        direction: Direction,
    },
    /// Travel to a place the character remembers by name.
    Visit {
        /// The cell key the place was learned at.
        destination: String,
        name: String,
    },
    Wait,
}

/// How fully a place is described on arriving there. `look` always
/// describes it in full.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Verbosity {
    /// In full the first time; after that, its name, ways and contents.
    #[default]
    Brief,
    /// In full every time.
    Verbose,
    /// Its name and contents only, even the first time.
    Superbrief,
}

/// What a sentence means, before anything runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Interpretation {
    /// An answer that takes no game time.
    Say(String),
    Look,
    /// How fully places are described on arrival.
    Describe(Verbosity),
    Goals(Vec<Goal>),
    Ask(Question),
    Tool(Input),
    Quit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub prompt: String,
    pub choices: Vec<Choice>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub label: String,
    /// The words that pick this choice, beyond its number.
    pub words: Vec<String>,
    pub key: Option<Key>,
    pub then: Interpretation,
}

pub const HELP: &str = "Try look, examine <thing>, take <thing>, drop <thing>, open or close <door>, attack <creature>, a direction (north, ne, up...), go to <thing or place>, go to exit, go to start, wait, inventory, status, places, name room <name>, again and quit. A direction keeps walking until there's something to see. brief, verbose and superbrief choose how places are described when you arrive. You can chain commands: take the token, then go east. Answer a question with a name or its number. Type help session for more.";
pub const SESSION_HELP: &str = "control, release, sync, save, history, note <text>, bookmark <text>, pace [milliseconds].\nstep <direction> makes one careful step. Developer commands require wizard authority.";

/// What the game can't do yet, as the refusal says it.
fn unsupported(verb: Verb) -> Option<&'static str> {
    Some(match verb {
        Verb::Wear => "wear anything",
        Verb::Wield => "wield anything",
        Verb::Remove => "take anything off",
        Verb::Eat => "eat anything",
        Verb::Drink => "drink anything",
        Verb::Give => "give anything away",
        Verb::Show => "show anything to anyone",
        Verb::Throw => "throw anything",
        Verb::Fire => "fire anything",
        Verb::Unlock => "unlock anything",
        Verb::Lock => "lock anything",
        Verb::Push => "push anything",
        Verb::Pull => "pull anything",
        Verb::Turn => "turn anything",
        Verb::Kick => "kick anything",
        Verb::Break => "break anything",
        Verb::Cut => "cut anything",
        Verb::Burn => "burn anything",
        Verb::Light => "light anything",
        Verb::Extinguish => "put anything out",
        Verb::Dig => "dig",
        Verb::Fill => "fill anything",
        Verb::Pour => "pour anything",
        Verb::Apply => "use things that way",
        Verb::Engrave => "engrave anything",
        Verb::Zap => "zap anything",
        Verb::Rub => "rub anything",
        Verb::Tie => "tie anything",
        Verb::Untie => "untie anything",
        Verb::Wave => "wave anything",
        Verb::Knock => "knock on anything",
        Verb::Sit => "sit down",
        Verb::Jump => "jump",
        Verb::Swim => "swim",
        Verb::Sleep => "sleep",
        Verb::Pray => "pray",
        Verb::Talk | Verb::Say => "talk with anyone",
        Verb::Ask => "ask anyone anything",
        Verb::Tell => "tell anyone anything",
        Verb::Search => "search for hidden things",
        Verb::Climb => "climb anything",
        Verb::Enter => "enter anything",
        Verb::Exit => "leave that way",
        _ => return None,
    })
}

fn cannot_yet(verb: Verb) -> Interpretation {
    Interpretation::Say(format!(
        "You can't {} yet.",
        unsupported(verb).unwrap_or("do that")
    ))
}

/// Resolve a phrase and apply `then` to what it names. Ambiguity becomes a
/// question whose choices are already interpreted.
fn bind(
    np: &NounPhrase,
    scene: &Scene,
    referents: &mut Referents,
    domain: Domain,
    then: &dyn Fn(&Referent) -> Interpretation,
) -> Interpretation {
    match resolve(np, scene, referents, domain) {
        Resolution::One(key) => {
            let r = scene.get(key).expect("resolved in scene");
            referents.mention(r);
            then(r)
        }
        Resolution::Many(keys) => {
            referents.mention_many(&keys);
            combine(
                keys.iter()
                    .filter_map(|k| scene.get(*k))
                    .map(|r| (r, then(r)))
                    .collect(),
            )
        }
        Resolution::Ask(keys) => ask(scene, &keys, then),
        Resolution::Missing(text) => Interpretation::Say(text),
    }
}

/// Several referents' interpretations as one: goals run in order; answers
/// are given one per thing.
fn combine(parts: Vec<(&Referent, Interpretation)>) -> Interpretation {
    let mut goals = Vec::new();
    let mut said: Vec<(String, String)> = Vec::new();
    for (r, part) in parts {
        match part {
            Interpretation::Goals(more) => goals.extend(more),
            Interpretation::Say(text) => {
                let line = (prose::capitalize(&r.name), text);
                if !said.contains(&line) {
                    said.push(line);
                }
            }
            other => return other,
        }
    }
    // One answer for all of them needs no names in front.
    let said: Vec<String> = match said.as_slice() {
        [(_, only)] => vec![only.clone()],
        _ => said
            .into_iter()
            .map(|(name, text)| format!("{name}: {text}"))
            .collect(),
    };
    match (goals.is_empty(), said.is_empty()) {
        (false, true) => Interpretation::Goals(goals),
        (true, false) => Interpretation::Say(said.join("\n")),
        // Some can be done and some can't: do what can be done; the rest is
        // said first.
        (false, false) => Interpretation::Goals(goals),
        (true, true) => Interpretation::Say("There's nothing to do that to.".into()),
    }
}

fn ask(scene: &Scene, keys: &[Key], then: &dyn Fn(&Referent) -> Interpretation) -> Interpretation {
    let candidates: Vec<&Referent> = keys.iter().filter_map(|k| scene.get(*k)).collect();
    let label = |r: &Referent| {
        let the = r.the();
        let twins = candidates.iter().filter(|o| o.the() == the).count() > 1;
        match r.whereabouts() {
            Some(place) if twins => format!("{the} {place}"),
            _ if twins && r.carried => format!("{the} you're carrying"),
            _ => the,
        }
    };
    let choices: Vec<Choice> = candidates
        .iter()
        .map(|r| Choice {
            label: label(r),
            words: r.words.clone(),
            key: Some(r.key),
            then: then(r),
        })
        .collect();
    let labels: Vec<String> = choices.iter().map(|c| c.label.clone()).collect();
    Interpretation::Ask(Question {
        prompt: format!("Which do you mean, {}?", prose::or_list(&labels)),
        choices,
    })
}

fn goal(goal: Goal) -> Interpretation {
    Interpretation::Goals(vec![goal])
}

fn say(text: impl Into<String>) -> Interpretation {
    Interpretation::Say(text.into())
}

/// Interpret one parsed sentence against the scene.
pub fn interpret(
    command: &ParsedCommand,
    scene: &Scene,
    referents: &mut Referents,
) -> Interpretation {
    match command {
        ParsedCommand::Intransitive { verb } => intransitive(*verb, scene),
        ParsedCommand::Directional { direction } => directional(*direction, scene),
        ParsedCommand::Step { direction } => goal(Goal::Step {
            direction: *direction,
        }),
        ParsedCommand::Stop => say("There's nothing to stop."),
        ParsedCommand::Again => say("There's nothing to repeat."),
        ParsedCommand::Help { topic } => say(if topic.as_deref() == Some("session") {
            SESSION_HELP
        } else {
            HELP
        }),
        ParsedCommand::Say { .. } => cannot_yet(Verb::Say),
        ParsedCommand::Session(session) => session_command(session, scene.state),
        ParsedCommand::Clarification(np) => match resolve(np, scene, referents, Domain::Any) {
            Resolution::Missing(_) => say(format!("I don't understand \"{}\".", np.raw)),
            _ => bind(np, scene, referents, Domain::Any, &|r| {
                say(format!("What do you want to do with {}?", r.the()))
            }),
        },
        ParsedCommand::Transitive { verb, direct } => {
            transitive(*verb, direct, None, scene, referents)
        }
        ParsedCommand::Ditransitive {
            verb,
            direct,
            preposition,
            indirect,
        } => transitive(
            *verb,
            direct,
            Some((*preposition, indirect)),
            scene,
            referents,
        ),
        ParsedCommand::MultiTransitive { verb, direct_list } => {
            let parts: Vec<Interpretation> = direct_list
                .iter()
                .map(|np| transitive(*verb, np, None, scene, referents))
                .collect();
            let mut goals = Vec::new();
            for part in parts {
                match part {
                    Interpretation::Goals(more) => goals.extend(more),
                    // The first thing that can't be done stops the list, as
                    // a chain would.
                    other if goals.is_empty() => return other,
                    _ => break,
                }
            }
            Interpretation::Goals(goals)
        }
    }
}

/// Verbs that mean nothing without something to act on.
fn needs_object(verb: Verb) -> bool {
    matches!(
        verb,
        Verb::Examine
            | Verb::Read
            | Verb::Take
            | Verb::Drop
            | Verb::Put
            | Verb::Open
            | Verb::Close
            | Verb::Unlock
            | Verb::Lock
            | Verb::Attack
            | Verb::Wear
            | Verb::Wield
            | Verb::Remove
            | Verb::Eat
            | Verb::Drink
            | Verb::Give
            | Verb::Show
            | Verb::Throw
            | Verb::Fire
            | Verb::Push
            | Verb::Pull
            | Verb::Turn
            | Verb::Kick
            | Verb::Break
            | Verb::Cut
            | Verb::Burn
            | Verb::Light
            | Verb::Extinguish
            | Verb::Fill
            | Verb::Pour
            | Verb::Apply
            | Verb::Engrave
            | Verb::Zap
            | Verb::Rub
            | Verb::Tie
            | Verb::Untie
            | Verb::Wave
            | Verb::Knock
            | Verb::Ask
            | Verb::Tell
            | Verb::Touch
    )
}

fn intransitive(verb: Verb, scene: &Scene) -> Interpretation {
    match verb {
        Verb::Look => Interpretation::Look,
        Verb::Inventory => say(inventory(scene)),
        Verb::Wait => goal(Goal::Wait),
        Verb::Quit => Interpretation::Quit,
        Verb::Diagnose => say(condition(scene.state)),
        Verb::Listen => say(crate::narrative::listen(
            scene.state,
            scene.palette,
            scene.places,
        )),
        Verb::Smell => say(crate::narrative::smell(
            scene.state,
            scene.palette,
            scene.places,
        )),
        Verb::Verbose => Interpretation::Describe(Verbosity::Verbose),
        Verb::Brief => Interpretation::Describe(Verbosity::Brief),
        Verb::Superbrief => Interpretation::Describe(Verbosity::Superbrief),
        Verb::Go => say("Where do you want to go?"),
        Verb::Step => say("Which way do you want to step?"),
        verb if needs_object(verb) => say(format!("What do you want to {}?", verb.as_str())),
        verb => cannot_yet(verb),
    }
}

fn directional(direction: Direction, scene: &Scene) -> Interpretation {
    // Stairs are taken in one move, wherever they lead.
    if matches!(direction, Direction::Up | Direction::Down)
        && super::place::survey(scene.state)
            .ways(direction)
            .any(|w| w.kind == super::place::Opening::Stairs)
    {
        return goal(Goal::Step { direction });
    }
    let ways = crate::adventure::exits(scene.state, direction);
    let go = |key: &str| {
        goal(Goal::Go {
            direction,
            destination: key.to_owned(),
        })
    };
    // A closed door is a way, but not one a journey can take.
    let (shut, open): (Vec<_>, Vec<_>) = ways.into_iter().partition(|w| w.closed);
    let open: Vec<_> = open
        .into_iter()
        .filter_map(|w| w.destination.map(|key| (key, w.label)))
        .collect();
    match (open.as_slice(), shut.first()) {
        ([], Some(door)) => say(format!(
            "{} is closed.",
            prose::capitalize(&door.label.replacen("a closed ", "the ", 1))
        )),
        ([], None) => say(format!(
            "You can't see a way {}.",
            super::scene::direction_name(direction)
        )),
        ([(key, _)], _) => go(key),
        (many, _) => {
            let choices: Vec<Choice> = many
                .iter()
                .map(|(key, label)| Choice {
                    label: label.clone(),
                    words: label.split_whitespace().map(str::to_lowercase).collect(),
                    key: None,
                    then: go(key),
                })
                .collect();
            let labels: Vec<String> = choices.iter().map(|c| c.label.clone()).collect();
            Interpretation::Ask(Question {
                prompt: format!("Which way do you mean, {}?", prose::or_list(&labels)),
                choices,
            })
        }
    }
}

fn transitive(
    verb: Verb,
    direct: &NounPhrase,
    indirect: Option<(Preposition, &NounPhrase)>,
    scene: &Scene,
    referents: &mut Referents,
) -> Interpretation {
    let quantity = direct.quantity.filter(|q| *q > 0);
    match (verb, indirect) {
        (Verb::Examine | Verb::Look, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            say(examine(r, scene))
        }),
        (Verb::Read, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            say(if r.is(Kind::Thing) && !r.description.is_empty() {
                r.description.clone()
            } else {
                format!("There's nothing written on {}.", r.the())
            })
        }),
        (Verb::Touch, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            say(format!("You feel nothing unexpected about {}.", r.the()))
        }),
        (Verb::Smell, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            say(format!("You smell nothing unusual about {}.", r.the()))
        }),
        (Verb::Listen, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            say(format!("You hear nothing from {}.", r.the()))
        }),
        (Verb::Diagnose, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            say(match r.kind {
                Kind::Me => diagnose(scene.state),
                Kind::Figure => injury(r, scene)
                    .unwrap_or_else(|| format!("You can't tell how {} is.", r.the())),
                _ => format!("{} isn't alive.", prose::capitalize(&r.the())),
            })
        }),
        (Verb::Take, None | Some((Preposition::From | Preposition::Off, _))) => {
            bind(direct, scene, referents, Domain::Ground, &|r| {
                take(r, quantity)
            })
        }
        (Verb::Drop, None | Some((Preposition::On | Preposition::In, _))) => {
            bind(direct, scene, referents, Domain::Carried, &|r| {
                drop(r, quantity)
            })
        }
        (Verb::Put, Some((Preposition::On | Preposition::In, place)))
            if place
                .head
                .as_deref()
                .is_some_and(|h| matches!(h, "floor" | "ground")) =>
        {
            bind(direct, scene, referents, Domain::Carried, &|r| {
                drop(r, quantity)
            })
        }
        (Verb::Put, _) => bind(direct, scene, referents, Domain::Carried, &|_| {
            say("You can't put things anywhere but the floor yet.")
        }),
        (Verb::Open | Verb::Close, _) => {
            let open = verb == Verb::Open;
            bind(direct, scene, referents, Domain::Doors, &|r| door(r, open))
        }
        (Verb::Attack, weapon) => {
            if let Some((Preposition::With, weapon)) = weapon {
                if let Resolution::Missing(text) =
                    resolve(weapon, scene, referents, Domain::Carried)
                {
                    return say(text);
                }
            }
            bind(
                direct,
                scene,
                referents,
                Domain::Figures,
                &|r| match r.key {
                    Key::Actor(target) => goal(Goal::Attack { target }),
                    Key::Me => say("You'd rather not hurt yourself."),
                    _ if r.name.ends_with(" corpse") => say(format!(
                        "The {} is already dead.",
                        r.name.trim_end_matches(" corpse")
                    )),
                    _ => say(format!("Attacking {} would achieve nothing.", r.the())),
                },
            )
        }
        (Verb::Go, _) if exit(direct, scene).is_some() => {
            let key = exit(direct, scene).expect("checked");
            if crate::narrative::here_key(scene.state) == Some(key) {
                say("You're already at the exit.")
            } else {
                goal(Goal::Visit {
                    destination: key.to_owned(),
                    name: EXIT.into(),
                })
            }
        }
        (Verb::Go, _) if start(direct, scene).is_some() => {
            let key = start(direct, scene).expect("checked");
            if crate::narrative::here_key(scene.state) == Some(key) {
                say("You're already where you started.")
            } else {
                goal(Goal::Visit {
                    destination: key.to_owned(),
                    name: "where you started".into(),
                })
            }
        }
        (Verb::Go, _) if remembered(direct, scene).is_some() => {
            let (key, name) = remembered(direct, scene).expect("checked");
            if crate::narrative::current_place_key(scene.state) == Some(key) {
                say(format!("You're already in {name}."))
            } else {
                goal(Goal::Visit {
                    destination: key.to_owned(),
                    name,
                })
            }
        }
        (Verb::Go | Verb::Step, _) => bind(direct, scene, referents, Domain::Any, &|r| approach(r)),
        (verb, _) if unsupported(verb).is_some() => {
            // Name what's meant first: "You can't see any lamp here" comes
            // before what the game can't do.
            match resolve(direct, scene, referents, Domain::Any) {
                Resolution::Missing(text) => say(text),
                _ => cannot_yet(verb),
            }
        }
        (verb, _) => say(format!(
            "You can't {} {} like that.",
            verb.as_str(),
            phrase(direct)
        )),
    }
}

fn take(r: &Referent, quantity: Option<u64>) -> Interpretation {
    match (r.kind, r.key) {
        (Kind::Thing, Key::Item(_)) if r.carried => say(format!("You already have {}.", r.the())),
        (Kind::Thing, Key::Item(item)) => {
            if quantity.is_some_and(|q| q > r.quantity) {
                return say(format!(
                    "There {} only {}.",
                    if r.quantity == 1 { "is" } else { "are" },
                    prose::counted(r.quantity, &r.name)
                ));
            }
            goal(Goal::Take { item, quantity })
        }
        (Kind::Me, _) => say("You can't take yourself."),
        (Kind::Figure, _) => say(format!("You can't carry {}.", r.the())),
        _ => say(format!("You can't take {}.", r.the())),
    }
}

fn drop(r: &Referent, quantity: Option<u64>) -> Interpretation {
    match r.key {
        Key::Item(item) if r.carried => {
            if quantity.is_some_and(|q| q > r.quantity) {
                return say(format!(
                    "You only have {}.",
                    prose::counted(r.quantity, &r.name)
                ));
            }
            goal(Goal::Drop { item, quantity })
        }
        _ => say(format!("You aren't carrying {}.", r.the())),
    }
}

fn door(r: &Referent, open: bool) -> Interpretation {
    let (verb, state_word) = if open {
        ("open", "open")
    } else {
        ("close", "closed")
    };
    match (r.key, r.open) {
        (Key::Door(_), Some(state)) if state == open => say(format!(
            "{} is already {state_word}.",
            prose::capitalize(&r.the())
        )),
        (Key::Door(door), _) => goal(Goal::Door { door, open }),
        _ => say(format!(
            "{} isn't something you can {verb}.",
            prose::capitalize(&r.the())
        )),
    }
}

/// How the objective's exit is named.
pub const EXIT: &str = "the exit";

/// The phrase without "back to" and "the": "back to the start" is "start".
fn bare(np: &NounPhrase) -> String {
    let raw = np.raw.trim().to_lowercase();
    let raw = ["back to ", "back "]
        .iter()
        .find_map(|p| raw.strip_prefix(p))
        .unwrap_or(&raw);
    raw.strip_prefix("the ").unwrap_or(raw).trim().to_owned()
}

/// The objective's exit, when the phrase asks for it and the objective is
/// disclosed: `go to exit`, `go back to the way out`.
fn exit<'s>(np: &NounPhrase, scene: &'s Scene) -> Option<&'s str> {
    matches!(bare(np).as_str(), "exit" | "way out")
        .then_some(())
        .and(scene.state.observation.combat.as_ref()?.exit.as_deref())
}

/// Where the character began, when the phrase asks for it: `go to start`,
/// `go back to the beginning`.
fn start<'s>(np: &NounPhrase, scene: &'s Scene) -> Option<&'s str> {
    matches!(
        bare(np).as_str(),
        "start" | "beginning" | "where i started" | "where i began"
    )
    .then_some(())
    .and(scene.places.start.as_deref())
}

/// A place the character remembers, named in full: its key and name. Names
/// are matched whole and without regard to case, so "go to the entry chamber"
/// finds Entry chamber.
fn remembered<'s>(np: &NounPhrase, scene: &'s Scene) -> Option<(&'s str, String)> {
    let raw = np.raw.trim().to_lowercase();
    let raw = raw.strip_prefix("the ").unwrap_or(&raw).trim().to_owned();
    let words = phrase(np).to_lowercase();
    scene
        .state
        .observation
        .places
        .iter()
        .filter(|p| p.origin != PlaceNameOrigin::Invented)
        .find(|p| {
            let name = p.name.trim().to_lowercase();
            !name.is_empty() && (name == raw || name == words)
        })
        .map(|p| (p.key.as_str(), crate::safe(&p.name)))
}

fn approach(r: &Referent) -> Interpretation {
    match r.kind {
        Kind::Me => say("You're already here."),
        Kind::Surface => say(format!("You're already by {}.", r.the())),
        _ if r.carried => say(format!("You're carrying {}.", r.the())),
        _ if r.reachable => say(format!("{} is right here.", prose::capitalize(&r.the()))),
        _ => goal(Goal::Approach { target: r.key }),
    }
}

/// How a visible figure looks hurt, as a sentence.
fn injury(r: &Referent, scene: &Scene) -> Option<String> {
    let Key::Actor(id) = r.key else {
        return None;
    };
    let level = scene
        .state
        .observation
        .combat
        .as_ref()?
        .actors
        .iter()
        .find(|a| a.actor == id)?
        .injury;
    Some(format!(
        "{} looks {}.",
        prose::capitalize(&r.the()),
        narration::injury(level)
    ))
}

/// The answer to examining something.
pub fn examine(r: &Referent, scene: &Scene) -> String {
    match r.kind {
        Kind::Me => diagnose(scene.state),
        Kind::Surface => surface(r, scene),
        Kind::Door => {
            let state = format!(
                "It is {}.",
                if r.open == Some(true) {
                    "open"
                } else {
                    "closed"
                }
            );
            if r.description.is_empty() {
                state
            } else {
                format!("{} {state}", prose::sentence(&r.description))
            }
        }
        Kind::Figure => {
            let mut text = if r.description.is_empty() {
                format!("You see nothing special about {}.", r.the())
            } else {
                prose::sentence(&r.description)
            };
            if let Some(hurt) = injury(r, scene) {
                text = format!(
                    "{text} {}",
                    hurt.replacen(&prose::capitalize(&r.the()), "It", 1)
                );
            }
            text
        }
        Kind::Thing => {
            if r.description.is_empty() {
                format!("You see nothing special about {}.", r.the())
            } else {
                prose::sentence(&r.description)
            }
        }
    }
}

fn surface(r: &Referent, scene: &Scene) -> String {
    let cells = &scene.state.observation.visible_cells;
    let roles = tor_client_common::surfaces::roles_by(cells, |cell| {
        crate::adventure::surface(scene.palette, cell)
    });
    let (materials, what, verb) = match r.key {
        Key::Surface(Surface::Walls) => (roles.walls, "walls", "are"),
        Key::Surface(Surface::Ceiling) => (roles.ceilings, "ceiling", "is"),
        // Raw diagnostic regions have no solid floor; their open cells
        // carry what lies underfoot.
        _ if roles.floors.is_empty() => (
            cells
                .iter()
                .filter_map(|c| crate::adventure::open_surface(scene.palette, c))
                .collect(),
            "floor",
            "is",
        ),
        _ => (roles.floors, "floor", "is"),
    };
    let materials: std::collections::BTreeSet<&str> = materials.into_iter().collect();
    let materials: Vec<String> = materials.into_iter().map(safe).collect();
    format!("The {what} {verb} made of {}.", prose::and_list(&materials))
}

/// What the character carries, as a sentence.
pub fn inventory(scene: &Scene) -> String {
    // Things alike are counted together: "two copper tokens".
    let mut kinds: Vec<(&str, &str, u64)> = Vec::new();
    for r in scene
        .referents
        .iter()
        .filter(|r| r.is(Kind::Thing) && r.carried)
    {
        match kinds
            .iter_mut()
            .find(|(identity, ..)| *identity == r.identity)
        {
            Some((_, _, count)) => *count += r.quantity,
            None => kinds.push((&r.identity, &r.name, r.quantity)),
        }
    }
    let carried: Vec<String> = kinds
        .iter()
        .map(|(_, name, count)| prose::counted(*count, name))
        .collect();
    if carried.is_empty() {
        "You are empty-handed.".into()
    } else {
        format!("You are carrying {}.", prose::and_list(&carried))
    }
}

/// The character's condition, without numbers beyond HP.
/// How the character is, and what the run asks of them.
fn condition(state: &StateView) -> String {
    let objective = state
        .observation
        .combat
        .as_ref()
        .filter(|c| !c.dead && !c.victory)
        .and_then(|c| c.objective);
    match objective {
        Some(o) => format!(
            "{} {}",
            diagnose(state),
            tor_client_common::narration::objective(o)
        ),
        None => diagnose(state),
    }
}

pub fn diagnose(state: &StateView) -> String {
    let Some(c) = &state.observation.combat else {
        return "You feel fine.".into();
    };
    let condition = if c.dead {
        "You are dead."
    } else if c.hp == c.max_hp {
        "You are unhurt."
    } else if c.hp * 4 >= c.max_hp * 3 {
        "You have a few cuts and bruises."
    } else if c.hp * 2 >= c.max_hp {
        "You are wounded."
    } else if c.hp * 4 >= c.max_hp {
        "You are badly wounded."
    } else {
        "You are close to death."
    };
    format!("{condition} (HP {}/{})", c.hp, c.max_hp)
}

fn session_command(session: &SessionCommand, state: &StateView) -> Interpretation {
    let tool = Interpretation::Tool;
    match session {
        SessionCommand::Save => tool(Input::Request(Request::Save)),
        SessionCommand::Sync => tool(Input::Request(Request::Snapshot)),
        SessionCommand::Control => tool(Input::Request(Request::AcquireControl)),
        SessionCommand::Release => tool(Input::Request(Request::ReleaseControl)),
        SessionCommand::Places => tool(Input::Places),
        SessionCommand::Pace(pace) => tool(Input::Pace(*pace)),
        SessionCommand::History { before } => tool(Input::Request(Request::History {
            before: before.clone().map(EntryId),
            limit: 50,
        })),
        SessionCommand::BranchHistory { branch, before } => {
            tool(Input::Request(Request::HistoryBranch {
                branch: BranchId(branch.clone()),
                before: before.clone().map(EntryId),
                limit: 50,
            }))
        }
        SessionCommand::Note { text, is_bookmark } => tool(Input::Command(Command::Annotate {
            anchor: Anchor::State {
                revision: state.revision,
            },
            text: text.clone(),
            source: ClientSource::User,
            audience: Audience::Actor,
            category: if *is_bookmark {
                AnnotationCategory::Bookmark
            } else {
                AnnotationCategory::Note
            },
        })),
        SessionCommand::Name { target, name } => {
            if matches!(target.as_str(), "room" | "place" | "here") {
                match crate::narrative::named_place_key(state) {
                    Some(key) => tool(Input::Command(Command::RenamePlace {
                        expected_revision: state.revision,
                        key: key.into(),
                        name: name.clone(),
                    })),
                    None => say("There's nothing here to name."),
                }
            } else {
                match crate::rename_place(&format!("{target} {name}"), state) {
                    Ok(input) => tool(input),
                    Err(e) => say(e),
                }
            }
        }
        SessionCommand::Annotate {
            source,
            audience,
            category,
            anchor,
            text,
        } => match crate::annotation(source, audience, category, anchor, text, state.revision) {
            Ok(input) => tool(input),
            Err(e) => say(e),
        },
        SessionCommand::Wizard { command } => match crate::parse_wizard(command, state.revision) {
            Ok(input) => tool(input),
            Err(e) => say(e),
        },
    }
}
