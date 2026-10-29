//! Verb semantics: what a resolved command means in the game.
//!
//! Many verbs share one game action, and one verb chooses its action by what
//! it acts on. Verbs the game has no rules for yet are recognized and refused
//! plainly, so supporting one later is one more entry here.
use tor_client_common::items::{item_action, ItemOperation};
use tor_client_common::narration;
use tor_protocol::*;

use super::{
    prose,
    resolve::{left_behind, phrase, resolve, Domain, Referents, Resolution},
    scene::{bearing, direction_name, Key, Kind, Referent, Scene, Surface},
};
use crate::{
    parser::{Meta, NounPhrase, ParsedCommand, Preposition, SessionCommand, Verb},
    safe, Input,
};

/// Something the character sets out to do. Each runs as one or more game
/// actions; see [`super::turn`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Goal {
    MagicItem {
        action: Action,
    },
    Escape {
        hold: HoldTarget,
    },
    UseItem {
        item: ItemTarget,
        operation: ItemOperation,
    },
    Take {
        item: ItemTarget,
        quantity: Option<u64>,
    },
    Drop {
        item: ItemTarget,
        quantity: Option<u64>,
    },
    ExamineTrap {
        trap: TrapTarget,
    },
    Door {
        door: DoorTarget,
        open: bool,
    },
    Attack {
        target: ActorTarget,
    },
    UseAbility {
        ability: String,
        invocation: AbilityInvocation,
        supplies: SupplySelection,
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
        /// Whether the character has been there before.
        back: bool,
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
    /// A refusal: said, and the rest of the command chain is dropped.
    Refuse(String),
    /// A command missing what it acts on, and the question asking for it.
    /// The answer completes the command: `before`, the answer, `after`.
    Orphan {
        prompt: String,
        before: String,
        after: String,
    },
    /// Something done only if the player says yes.
    Confirm {
        prompt: String,
        then: Box<Interpretation>,
    },
    /// Several in turn, stopping where one stops.
    Sequence(Vec<Interpretation>),
    /// A sentence to read afresh, against the scene as it is once what came
    /// before is done: "eat ration" after picking it up.
    Reread(String),
    /// Back to the place before this one, fleeing when `flee`.
    Back {
        flee: bool,
    },
    /// Attack a figure, and keep attacking until it falls or something
    /// calls a halt.
    Fight(ActorTarget),
    /// Close in on a figure, following it as it moves.
    Follow(ActorTarget),
    /// Show the client's settings.
    Options,
    /// What `take all` leaves where it lies from now on.
    AllSkips(Vec<ItemClass>),
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

pub const HELP: &str = "\
Look and ask: charts, chart monsters|food|magic|mapping, chart next|previous|cancel, look, examine <thing>, look <direction>, exits, where am i, what now, inventory, stats, status.
Handle things: take <thing>, take all, drop <thing>, wear or wield <item>, remove <item>, ignite <torch>, extinguish <torch>, drink <potion>, eat <food>, read <scroll>, zap <device> <direction>, identify <item> or identify cancel, open or close <door>.
Fight: attack <creature> [with <equipped weapon>], fight <creature> to keep attacking, follow <creature>, flee, back.
Move: a direction (north, ne, up...) walks until there's something to see; go to <thing or place>, go to exit, go to start, places, name room <name>.
Time: wait, resume, cancel [direction], escape [hold <number|#token|name>], escape <direction>, advancement.
Techniques, if granted: rattling blow <creature>, measured cut <creature>, brace for impact, spark lance fire <creature>, falter <creature>, shell impact. When the ability permits weapon selection, add with <equipped weapon> to choose its exact socket; optional using <supplies> comes last. Explicit choices show exact cost and next-strike/recovery timing, then ask yes/no before acting. For projectile abilities, enter the ability name, choose a numbered source (next/previous pages, cancel), ammunition if launching, then a direction. stats lists yours; a first word is enough when it's unique, as in brace.
Talking to the game: again repeats a command (attack again repeats an attack); chain commands with then, commas or full stops; answer a question with a name or its number; it, them, him and her mean what was last mentioned.
Settings: brief, verbose and superbrief choose how places are described when you arrive; all skips <kinds> chooses what take all leaves behind; options shows them. Type help session for more, and quit to save and leave.";
pub const SESSION_HELP: &str = "control, release, sync, save, history, note <text>, bookmark <text>, pace [milliseconds].\nstep <direction> makes one careful step. Developer commands require wizard authority. wizard creature list discovers named spawn recipes; add depth <N> or after <selector> to refine the listing. After creature inspection: strikes natural, strikes selected, strikes ability <id>, strikes next, strikes previous.";

/// What there's no point trying, as the refusal says it. The game has no
/// rules for these; recognizing them only lets the answer be a sensible one.
fn unsupported(verb: Verb) -> Option<&'static str> {
    Some(match verb {
        Verb::Give | Verb::Show => "No one here is interested.",
        Verb::Throw | Verb::Fire => "You'd rather keep hold of it; use attack to fight.",
        Verb::Unlock | Verb::Lock => "There's no lock here you can work.",
        Verb::Push | Verb::Pull | Verb::Turn | Verb::Kick | Verb::Break | Verb::Cut => {
            "That would achieve nothing."
        }
        Verb::Burn | Verb::Light | Verb::Extinguish => {
            "You have no way to light or put out anything."
        }
        Verb::Dig => "Digging here would get you nowhere.",
        Verb::Fill | Verb::Pour => "There's nothing here to fill or pour.",
        Verb::Apply => "Say how: wield, wear, drink or eat it.",
        Verb::Engrave => "Writing here would serve no purpose. To remember something, use note.",
        Verb::Zap | Verb::Rub | Verb::Tie | Verb::Untie | Verb::Wave | Verb::Knock => {
            "Nothing would come of that."
        }
        Verb::Sit => "You'd rather stay on your feet.",
        Verb::Jump => "You jump on the spot. Nothing comes of it.",
        Verb::Swim => "You can't swim here.",
        Verb::Sleep => "Sleeping here could be the last mistake you ever make. Use wait to rest.",
        Verb::Pray => "You murmur a prayer. If anything hears it, it gives no sign.",
        Verb::Talk | Verb::Say | Verb::Ask | Verb::Tell => "No one here seems inclined to talk.",
        Verb::Search => "You search, but find nothing you hadn't already seen.",
        Verb::Climb => "There's nothing here to climb. Use up or down on stairs.",
        Verb::Enter => "To go somewhere, use a direction, or go to <place>.",
        Verb::Exit => "To leave, use a direction, or go to exit.",
        _ => return None,
    })
}

fn cannot(verb: Verb) -> Interpretation {
    Interpretation::Refuse(unsupported(verb).unwrap_or("You can't do that.").into())
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
        Resolution::Missing(text) => Interpretation::Refuse(text),
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
            Interpretation::Say(text) | Interpretation::Refuse(text) => {
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

fn refuse(text: impl Into<String>) -> Interpretation {
    Interpretation::Refuse(text.into())
}

fn orphan(prompt: impl Into<String>, before: impl Into<String>) -> Interpretation {
    Interpretation::Orphan {
        prompt: prompt.into(),
        before: before.into(),
        after: String::new(),
    }
}

/// Whether a thing's class is the kind an item verb is for, so picking it
/// up first makes sense.
fn suits(operation: ItemOperation, class: ItemClass) -> bool {
    match operation {
        ItemOperation::Equip => matches!(
            class,
            ItemClass::Weapon
                | ItemClass::Armor
                | ItemClass::Amulet
                | ItemClass::Ring
                | ItemClass::Wand
        ),
        ItemOperation::Drink => class == ItemClass::Potion,
        ItemOperation::Eat => class == ItemClass::Food,
        _ => false,
    }
}

/// The one hostile figure in sight, when there's exactly one.
fn lone_foe<'s>(scene: &'s Scene) -> Option<&'s Referent> {
    let mut foes = scene.of(Kind::Figure).filter(|r| r.hostile != Some(false));
    let foe = foes.next()?;
    foes.next().is_none().then_some(foe)
}

/// A verb that acts on a figure, given none: the one foe in sight, or the
/// question who.
fn at_foe(verb: Verb, scene: &Scene, referents: &mut Referents) -> Interpretation {
    let Some(foe) = lone_foe(scene) else {
        if scene.of(Kind::Figure).next().is_none() {
            return refuse(match verb {
                Verb::Follow => "There's no one here to follow.",
                _ => "There's no one here to fight.",
            });
        }
        return orphan(
            format!("Who do you want to {}?", verb.as_str()),
            verb.as_str(),
        );
    };
    referents.mention(foe);
    let Key::Actor(id) = foe.key else {
        return refuse("There's no one here to fight.");
    };
    let act = match verb {
        Verb::Fight => Interpretation::Fight(id),
        Verb::Follow if foe.reachable => {
            return say(format!("{} is right here.", prose::capitalize(&foe.the())))
        }
        Verb::Follow => Interpretation::Follow(id),
        _ => goal(Goal::Attack { target: id }),
    };
    // Which one was meant, as the game understood it.
    Interpretation::Sequence(vec![say(format!("({})", foe.the())), act])
}

/// Levenshtein distance, for suggesting the word that was probably meant.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut last = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let here = row[j + 1];
            row[j + 1] = (last + usize::from(ca != *cb))
                .min(row[j] + 1)
                .min(here + 1);
            last = here;
        }
    }
    row[b.len()]
}

/// Words commands begin with, for suggestions.
const COMMAND_WORDS: &[&str] = &[
    "look",
    "examine",
    "read",
    "inventory",
    "take",
    "drop",
    "open",
    "close",
    "attack",
    "fight",
    "follow",
    "flee",
    "back",
    "wait",
    "wear",
    "wield",
    "remove",
    "drink",
    "eat",
    "north",
    "south",
    "east",
    "west",
    "northeast",
    "northwest",
    "southeast",
    "southwest",
    "up",
    "down",
    "step",
    "help",
    "stats",
    "status",
    "places",
    "again",
    "listen",
    "smell",
    "search",
    "resume",
    "cancel",
    "escape",
    "quit",
    "exits",
    "options",
    "diagnose",
    "advancement",
];

/// The answer to a sentence that starts with no word the game knows.
fn unknown_words(np: &NounPhrase, scene: &Scene) -> Interpretation {
    let first = np.raw.split_whitespace().next().unwrap_or("").to_owned();
    if super::scene::known_noun(&first) {
        return refuse(format!("You can't see any {} here.", np.raw));
    }
    let known: Vec<String> = COMMAND_WORDS
        .iter()
        .map(|w| (*w).to_owned())
        .chain(scene.referents.iter().flat_map(|r| r.heads.clone()))
        .chain(
            tor_client_common::abilities::available(&scene.state.observation)
                .iter()
                .filter(|a| !a.basic_attack)
                .map(|a| a.name.to_lowercase()),
        )
        .collect();
    let limit = if first.chars().count() >= 5 { 2 } else { 1 };
    let nearest = known
        .iter()
        .map(|w| (distance(&first, w), w))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, w)| (*d, w.len()));
    match nearest {
        Some((_, word)) => refuse(format!(
            "I don't know the word \"{first}\". Did you mean \"{word}\"?"
        )),
        None if np.raw.split_whitespace().count() > 1 => refuse(format!(
            "I don't know the word \"{first}\". Type help for things you can try."
        )),
        None => refuse(format!(
            "I don't know the word \"{first}\". Type help for things you can try, or what now for ideas."
        )),
    }
}

/// When the end of a phrase names nothing but its start does: what the
/// start names. "examine the scout's lantern" names the scout so far.
fn understood_part(
    np: &NounPhrase,
    scene: &Scene,
    referents: &Referents,
    domain: Domain,
) -> Option<String> {
    let words: Vec<String> = np
        .adjectives
        .iter()
        .chain(np.head.iter())
        .cloned()
        .collect();
    (1..words.len()).rev().find_map(|n| {
        let mut shorter = np.clone();
        shorter.head = words.get(n - 1).cloned();
        shorter.adjectives = words[..n - 1].to_vec();
        match resolve(&shorter, scene, referents, domain) {
            Resolution::One(key) => scene.get(key).map(Referent::the),
            _ => None,
        }
    })
}

/// Interpret one parsed sentence against the scene.
pub fn interpret(
    command: &ParsedCommand,
    scene: &Scene,
    referents: &mut Referents,
) -> Interpretation {
    match command {
        ParsedCommand::Projectile { id } => Interpretation::Tool(Input::Projectile(id.clone())),
        ParsedCommand::Ability {
            id,
            selection: operand,
            supplies,
            weapon,
        } => {
            let weapon = match weapon
                .as_deref()
                .map_or(Ok(WeaponSelection::Default), |text| {
                    tor_client_common::weapons::selection_from_text(&scene.state.observation, text)
                }) {
                Ok(weapon) => weapon,
                Err(reason) => return refuse(reason),
            };
            let selection = match supplies {
                None => SupplySelection::Carried,
                Some(text) => {
                    let Some(ability) =
                        tor_client_common::abilities::available(&scene.state.observation)
                            .iter()
                            .find(|ability| &ability.id == id)
                    else {
                        return refuse("That ability is unavailable.");
                    };
                    match tor_client_common::abilities::selection_from_text(
                        &scene.state.observation,
                        ability,
                        text,
                    ) {
                        Ok(selection) => selection,
                        Err(reason) => return refuse(reason),
                    }
                }
            };
            let invoke = |invocation| match tor_client_common::abilities::invocation_action(
                &scene.state.observation,
                id,
                invocation,
                selection.clone(),
            ) {
                Ok(_) => goal(Goal::UseAbility {
                    ability: id.clone(),
                    invocation,
                    supplies: selection.clone(),
                }),
                Err(reason) => refuse(reason),
            };
            let direct = match operand {
                crate::parser::grammar::AbilitySelection::ActorTarget { direct } => direct,
                crate::parser::grammar::AbilitySelection::SelfStep { direction } => {
                    return invoke(tor_protocol::AbilityInvocation {
                        aim: tor_protocol::AbilityAim::SelfStep {
                            direction: *direction,
                        },
                        weapon,
                    });
                }
                crate::parser::grammar::AbilitySelection::DirectionalRay { direction } => {
                    return invoke(tor_protocol::AbilityInvocation {
                        aim: tor_protocol::AbilityAim::DirectionalRay {
                            direction: *direction,
                        },
                        weapon,
                    });
                }
            };
            bind(direct, scene, referents, Domain::Figures, &|r| {
                let target = match r.key {
                    Key::Actor(target) => target,
                    Key::Me => scene.state.observation.self_target,
                    _ => return refuse("Choose a living creature."),
                };
                invoke(tor_protocol::AbilityInvocation {
                    aim: tor_protocol::AbilityAim::ActorTarget { target },
                    weapon,
                })
            })
        }
        ParsedCommand::Intransitive { verb } if needs_figure(*verb) => {
            at_foe(*verb, scene, referents)
        }
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
        ParsedCommand::Say { .. } => cannot(Verb::Say),
        ParsedCommand::Session(session) => session_command(session, scene),
        ParsedCommand::Clarification(np) => match resolve(np, scene, referents, Domain::Any) {
            Resolution::Missing(_) => unknown_words(np, scene),
            _ => bind(np, scene, referents, Domain::Any, &|r| {
                Interpretation::Orphan {
                    prompt: format!("What do you want to do with {}?", r.the()),
                    before: String::new(),
                    after: np.raw.clone(),
                }
            }),
        },
        ParsedCommand::Meta(meta) => meta_answer(*meta, scene),
        ParsedCommand::Orphan {
            prompt,
            before,
            after,
        } => Interpretation::Orphan {
            prompt: prompt.clone(),
            before: before.clone(),
            after: after.clone(),
        },
        ParsedCommand::LookToward { direction } => say(look_toward(*direction, scene)),
        ParsedCommand::Refused(why) => refuse(why.clone()),
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

/// Verbs that act on a figure, and with none named act on the one foe in
/// sight.
fn needs_figure(verb: Verb) -> bool {
    matches!(verb, Verb::Attack | Verb::Fight | Verb::Follow)
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
        Verb::Stats => say(crate::stats(scene.state)),
        Verb::Wait => goal(Goal::Wait),
        Verb::Quit => Interpretation::Quit,
        Verb::Diagnose => say(condition(scene.state)),
        Verb::Listen => say(crate::narrative::listen(
            scene.view,
            scene.palette,
            scene.places,
        )),
        Verb::Smell => say(crate::narrative::smell(
            scene.view,
            scene.palette,
            scene.places,
        )),
        Verb::Verbose => Interpretation::Describe(Verbosity::Verbose),
        Verb::Brief => Interpretation::Describe(Verbosity::Brief),
        Verb::Superbrief => Interpretation::Describe(Verbosity::Superbrief),
        Verb::Go => orphan("Where do you want to go?", "go to"),
        Verb::Step => orphan("Which way do you want to step?", "step"),
        Verb::Flee => Interpretation::Back { flee: true },
        Verb::Back => Interpretation::Back { flee: false },
        verb if needs_object(verb) => orphan(
            format!("What do you want to {}?", verb.as_str()),
            verb.as_str(),
        ),
        verb => cannot(verb),
    }
}

/// Where a feature of the place leads, as a sentence.
fn feature_text(r: &Referent) -> String {
    match (r.name.as_str(), r.way) {
        ("exit", _) => match r.position {
            Some(p) if p.x == 0 && p.y == 0 => {
                "You are standing at the exit: the way out, once your task is done.".into()
            }
            Some(p) => format!(
                "The exit is {}: the way out, once your task is done.",
                super::scene::whereabouts(p)
            ),
            None => "The exit is the way out, once your task is done.".into(),
        },
        (name, Some(d)) if name.starts_with("view limit to the ") => format!(
            "To the {} the place continues beyond your view. You can't see what lies beyond until you go closer.",
            direction_name(d)
        ),
        ("stairs", Some(d)) => format!("Stairs lead {}.", direction_name(d)),
        ("archway", Some(d)) => format!("An archway opens to the {}.", direction_name(d)),
        (_, Some(d)) => format!("A {} leads {}.", r.name, direction_name(d)),
        _ => format!("You see nothing special about {}.", r.the()),
    }
}

/// What lies one way: who and what is there, and the ways on.
fn look_toward(direction: Direction, scene: &Scene) -> String {
    match direction {
        Direction::Up => {
            return match scene.get(Key::Surface(Surface::Ceiling)) {
                Some(r) => surface(r, scene),
                None => "You cannot see what lies above.".into(),
            }
        }
        Direction::Down => {
            return match scene.get(Key::Surface(Surface::Floor)) {
                Some(r) => surface(r, scene),
                None => "You can't make out what's underfoot.".into(),
            }
        }
        _ => {}
    }
    let mut sentences = Vec::new();
    let mut seen: Vec<&Referent> = scene
        .referents
        .iter()
        .filter(|r| {
            matches!(r.kind, Kind::Thing | Kind::Figure | Kind::Door | Kind::Trap) && !r.carried
        })
        .filter(|r| r.position.and_then(bearing) == Some(direction))
        .collect();
    seen.sort_by_key(|r| r.position.map_or(0, super::scene::distance));
    if !seen.is_empty() {
        let names: Vec<String> = seen.iter().map(|r| r.a()).collect();
        sentences.push(format!(
            "To the {} you see {}.",
            direction_name(direction),
            prose::and_list(&names)
        ));
    }
    let ways: Vec<String> = crate::adventure::exits(scene.view, direction)
        .into_iter()
        .map(|e| e.label)
        .collect();
    if !ways.is_empty() {
        sentences.push(format!(
            "There {} {}.",
            if ways.len() == 1 { "is" } else { "are" },
            prose::and_list(&ways)
        ));
    }
    if scene
        .of(Kind::Feature)
        .any(|r| r.name.starts_with("view limit to the ") && r.way == Some(direction))
    {
        sentences.push(format!(
            "Beyond, to the {}, the place continues beyond your view.",
            direction_name(direction)
        ));
    }
    if sentences.is_empty() {
        format!(
            "You see nothing of note to the {}.",
            direction_name(direction)
        )
    } else {
        sentences.join(" ")
    }
}

const ALL_DIRECTIONS: [Direction; 10] = [
    Direction::North,
    Direction::NorthEast,
    Direction::East,
    Direction::SouthEast,
    Direction::South,
    Direction::SouthWest,
    Direction::West,
    Direction::NorthWest,
    Direction::Up,
    Direction::Down,
];

/// The ways on from here, as an answer to "exits".
fn exits_answer(scene: &Scene) -> String {
    let mut ways: Vec<String> = Vec::new();
    for d in ALL_DIRECTIONS {
        for exit in crate::adventure::exits(scene.view, d) {
            if !ways.contains(&exit.label) {
                ways.push(exit.label);
            }
        }
    }
    let mut sentences = Vec::new();
    if ways.is_empty() {
        sentences.push("You see no way onward from here.".to_owned());
    } else {
        sentences.push(format!("Ways on: {}.", prose::and_list(&ways)));
    }
    if let Some(exit) = scene.of(Kind::Feature).find(|r| r.name == "exit") {
        sentences.push(feature_text(exit));
    }
    sentences.join(" ")
}

/// Who the character is.
fn who(scene: &Scene) -> String {
    let o = &scene.state.observation;
    let name = o
        .visible_actors
        .iter()
        .find(|a| a.id == o.self_target)
        .map(|a| safe(&a.name).to_lowercase())
        .filter(|n| !n.trim().is_empty());
    let mut sentences = vec![match &name {
        Some(name) => format!("You are {}.", prose::indefinite(name)),
        None => "You are yourself, and that will have to do.".into(),
    }];
    if let Some(stats) = o.combat.as_ref().and_then(|c| c.own_stats.as_ref()) {
        let dice: Vec<String> = stats
            .hit_dice
            .sources
            .iter()
            .map(|row| {
                format!(
                    "{} {}",
                    prose::number(row.count),
                    safe(&row.source.name).to_lowercase()
                )
            })
            .collect();
        if !dice.is_empty() {
            sentences.push(format!(
                "You are {} with {} hit {}.",
                prose::indefinite(&safe(&stats.kind.name).to_lowercase()),
                prose::and_list(&dice),
                if stats.hit_dice.total == 1 {
                    "die"
                } else {
                    "dice"
                }
            ));
        }
    }
    sentences.push(diagnose(scene.state));
    sentences.push("stats shows your full build.".into());
    sentences.join(" ")
}

/// Ideas for what to do next, from what's in sight.
fn what_now(scene: &Scene) -> String {
    let mut ideas: Vec<String> = Vec::new();
    let foes: Vec<&Referent> = scene
        .of(Kind::Figure)
        .filter(|r| r.hostile != Some(false))
        .collect();
    match foes.as_slice() {
        [] => {}
        [one] => ideas.push(format!("attack {} (or fight it to keep at it)", one.the())),
        [first, ..] => ideas.push(format!(
            "fight {}, or flee back the way you came",
            first.the()
        )),
    }
    let here: Vec<&Referent> = scene
        .of(Kind::Thing)
        .filter(|r| !r.carried && r.class != Some(ItemClass::Corpse))
        .collect();
    match here.as_slice() {
        [] => {}
        [one] => ideas.push(format!("take {}", one.the())),
        _ => ideas.push("take all".into()),
    }
    let ways: Vec<String> = ALL_DIRECTIONS
        .iter()
        .filter(|d| !crate::adventure::exits(scene.view, **d).is_empty())
        .map(|d| direction_name(*d).to_owned())
        .collect();
    if !ways.is_empty() {
        ideas.push(format!("head {}", prose::or_list(&ways)));
    }
    if let Some(c) = &scene.state.observation.combat {
        if u128::from(c.hp) * 2 < u128::from(c.max_hp) && foes.is_empty() {
            ideas.push("wait to recover".into());
        }
    }
    let mut text = if ideas.is_empty() {
        "Look around, check your inventory, or try a direction.".to_owned()
    } else {
        prose::sentence(&format!("you could {}", prose::or_list(&ideas)))
    };
    if let Some(goal) = objective_text(scene.state) {
        text.push_str(&format!(" Your task: {}", goal.to_lowercase()));
    }
    text.push_str(" Type help for every command.");
    text
}

/// The run's objective, as the character would put it.
pub fn objective_text(state: &StateView) -> Option<String> {
    let c = state.observation.combat.as_ref()?;
    if c.dead || c.victory {
        return None;
    }
    Some(objective_phrase(c.objective?).to_owned())
}

/// An objective as the character would put it.
pub fn objective_phrase(kind: ObjectiveKind) -> &'static str {
    match kind {
        ObjectiveKind::RetrieveAndReturn => {
            "Find what you came for and carry it back out through the exit."
        }
        ObjectiveKind::RetrieveAndExit => {
            "Find what you came for and carry it up through the surface exit."
        }
        ObjectiveKind::ReachExit => "Find your way to the exit.",
    }
}

/// Answers to questions about the game, and remarks that change nothing.
fn meta_answer(meta: Meta, scene: &Scene) -> Interpretation {
    let ended = scene
        .state
        .observation
        .combat
        .as_ref()
        .is_some_and(|c| c.terminal || c.dead);
    match meta {
        Meta::Exits => say(exits_answer(scene)),
        Meta::Where => say(crate::adventure::describe_place_with(
            scene.view,
            scene.palette,
            scene.places,
        )),
        Meta::Who => say(who(scene)),
        Meta::WhatNow if ended => say(
            "This run is over. You can still look around, read your history, or quit and start a new game.",
        ),
        Meta::WhatNow => say(what_now(scene)),
        Meta::Time => say("Down here there's no telling the hour. Only your own weariness keeps time."),
        Meta::Undo => say(if scene.state.wizard_game {
            "What's done is done. (A wizard game can rewind; see help session.)"
        } else {
            "What's done is done; there's no taking it back."
        }),
        Meta::Restart if ended => say(
            "This run has ended. To play again, quit and start a new game with a fresh save.",
        ),
        Meta::Restart => say(
            "To start over, quit and begin a new game with a fresh save. This game is kept as it is.",
        ),
        Meta::About => say(
            "Thresholds of Ruin, played in words. Type help for commands and help session for connection tools.",
        ),
        Meta::Options => Interpretation::Options,
        Meta::Yes | Meta::No => say("There's no question to answer."),
        Meta::Flavor(text) => say(text),
    }
}

fn directional(direction: Direction, scene: &Scene) -> Interpretation {
    // Stairs and terminal exits are explicitly taken in one move.
    if matches!(direction, Direction::Up | Direction::Down)
        && super::place::survey(scene.view).ways(direction).any(|w| {
            matches!(
                w.kind,
                super::place::Opening::Stairs | super::place::Opening::SurfaceExit
            )
        })
    {
        return goal(Goal::Step { direction });
    }
    let ways = crate::adventure::exits(scene.view, direction);
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
        ([], None) => toward_something(direction, scene).unwrap_or_else(|| {
            refuse(format!(
                "You can't see a way {}.",
                super::scene::direction_name(direction)
            ))
        }),
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

/// With no way onward that way, something in sight that way to walk to:
/// the exit, or the nearest door or thing.
fn toward_something(direction: Direction, scene: &Scene) -> Option<Interpretation> {
    if matches!(direction, Direction::Up | Direction::Down) {
        return None;
    }
    let that_way = |r: &&Referent| r.position.and_then(bearing) == Some(direction);
    if let Some(exit) = scene
        .of(Kind::Feature)
        .filter(that_way)
        .find(|r| r.name == "exit")
    {
        let key = scene.state.observation.combat.as_ref()?.exit.clone()?;
        let _ = exit;
        return Some(goal(Goal::Visit {
            destination: key,
            name: EXIT.into(),
            back: false,
        }));
    }
    let nearest = scene
        .referents
        .iter()
        .filter(|r| matches!(r.kind, Kind::Thing | Kind::Door) && !r.carried && !r.reachable)
        .filter(that_way)
        .min_by_key(|r| r.position.map_or(0, super::scene::distance));
    // Open floor is not a way onward; `step` crosses it one move at a time.
    nearest.map(|r| goal(Goal::Approach { target: r.key }))
}

fn transitive(
    verb: Verb,
    direct: &NounPhrase,
    indirect: Option<(Preposition, &NounPhrase)>,
    scene: &Scene,
    referents: &mut Referents,
) -> Interpretation {
    let meant = transitive_meaning(verb, direct, indirect, scene, referents);
    // "examine the scout's lantern": the scout was understood, the rest not.
    match &meant {
        Interpretation::Refuse(text) if text.starts_with("You can't see any") => {
            match understood_part(direct, scene, referents, Domain::Any) {
                Some(part) => refuse(format!(
                    "I only understood you as far as wanting to {} {part}.",
                    verb.as_str()
                )),
                None => meant,
            }
        }
        _ => meant,
    }
}

fn transitive_meaning(
    verb: Verb,
    direct: &NounPhrase,
    indirect: Option<(Preposition, &NounPhrase)>,
    scene: &Scene,
    referents: &mut Referents,
) -> Interpretation {
    let quantity = direct.quantity.filter(|q| *q > 0);
    match (verb, indirect) {
        (
            Verb::Examine | Verb::Look,
            Some((Preposition::Under | Preposition::Behind | Preposition::In, _)),
        )
        | (Verb::Search, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            say(match r.kind {
                Kind::Me => "To search for concealed features nearby, use SEARCH.".into(),
                Kind::Figure => format!(
                    "You can't search {} while it's alive and watching you.",
                    r.the()
                ),
                _ => "To search for concealed features nearby, use SEARCH.".into(),
            })
        }),
        (Verb::Examine | Verb::Look, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            if let Key::Trap(trap) = r.key {
                let unknown = scene
                    .state
                    .observation
                    .visible_cells
                    .iter()
                    .filter_map(|cell| cell.trap.as_ref())
                    .any(|candidate| candidate.id == trap && candidate.family.is_none());
                if unknown {
                    return if r.reachable {
                        goal(Goal::ExamineTrap { trap })
                    } else {
                        say("Come within reach to examine the trap's mechanism.")
                    };
                }
            }
            say(examine(r, scene))
        }),
        (Verb::Fight | Verb::Follow, _) => {
            bind(
                direct,
                scene,
                referents,
                Domain::Figures,
                &|r| match r.key {
                    Key::Actor(target) if verb == Verb::Fight => Interpretation::Fight(target),
                    Key::Actor(_) if r.reachable => {
                        say(format!("{} is right here.", prose::capitalize(&r.the())))
                    }
                    Key::Actor(target) => Interpretation::Follow(target),
                    Key::Me => refuse("You'd rather not hurt yourself."),
                    _ if r.class == Some(ItemClass::Corpse) => refuse(format!(
                        "The {} is already dead.",
                        r.name.trim_end_matches(" corpse")
                    )),
                    _ => refuse(format!(
                        "{} isn't going anywhere.",
                        prose::capitalize(&r.the())
                    )),
                },
            )
        }
        (Verb::Flee, _) => Interpretation::Back { flee: true },
        (Verb::Back, _) => Interpretation::Back { flee: false },
        (Verb::Read, _) => bind(direct, scene, referents, Domain::Any, &|r| {
            if let Key::Item(item) = r.key {
                if scene
                    .state
                    .observation
                    .interactions
                    .as_ref()
                    .is_some_and(|interactions| {
                        interactions
                            .inventory
                            .iter()
                            .any(|offer| offer.item == item && offer.readable)
                    })
                {
                    return match item_action(&scene.state.observation, item, ItemOperation::Read) {
                        Ok(_) => goal(Goal::UseItem {
                            item,
                            operation: ItemOperation::Read,
                        }),
                        Err(reason) => refuse(reason),
                    };
                }
            }
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
            let left = left_behind(direct, scene, Domain::Ground);
            let taken = bind(direct, scene, referents, Domain::Ground, &|r| {
                take(r, quantity)
            });
            let names: Vec<String> = left
                .iter()
                .filter_map(|k| scene.get(*k))
                .map(Referent::the)
                .collect();
            if names.is_empty() {
                return taken;
            }
            let them = if left.len() > 1 { "them" } else { "it" };
            match taken {
                // Say what was left, so nothing is skipped silently.
                Interpretation::Goals(_) => Interpretation::Sequence(vec![
                    taken,
                    say(format!(
                        "You leave {} where {} {}.",
                        prose::and_list(&names),
                        if left.len() > 1 { "they" } else { "it" },
                        if left.len() > 1 { "lie" } else { "lies" }
                    )),
                ]),
                // Only what's left is here: ask before taking it.
                Interpretation::Refuse(_) => {
                    let goals: Vec<Goal> = left
                        .iter()
                        .filter_map(|k| match k {
                            Key::Item(item) => Some(Goal::Take {
                                item: *item,
                                quantity: None,
                            }),
                            _ => None,
                        })
                        .collect();
                    let mut kinds: Vec<String> = Vec::new();
                    for class in left.iter().filter_map(|k| scene.get(*k)?.class) {
                        let kind = super::settings::class_name(class).to_owned();
                        if !kinds.contains(&kind) {
                            kinds.push(kind);
                        }
                    }
                    Interpretation::Confirm {
                        prompt: format!(
                            "There's only {} here, and take all leaves {} alone. Take {them} anyway?",
                            prose::and_list(&names),
                            prose::and_list(&kinds)
                        ),
                        then: Box::new(Interpretation::Goals(goals)),
                    }
                }
                other => other,
            }
        }
        (Verb::Drop, None | Some((Preposition::On | Preposition::In, _))) => {
            bind(direct, scene, referents, Domain::Carried, &|r| {
                drop(r, quantity, scene)
            })
        }
        (
            Verb::Wear
            | Verb::Wield
            | Verb::Remove
            | Verb::Drink
            | Verb::Eat
            | Verb::Light
            | Verb::Extinguish,
            _,
        ) => {
            let operation = match verb {
                Verb::Remove => ItemOperation::Unequip,
                Verb::Drink => ItemOperation::Drink,
                Verb::Eat => ItemOperation::Eat,
                Verb::Light => ItemOperation::Ignite,
                Verb::Extinguish => ItemOperation::Extinguish,
                _ => ItemOperation::Equip,
            };
            bind(direct, scene, referents, Domain::Carried, &|r| {
                if quantity.is_some_and(|q| q > 1) {
                    return refuse("Use one item at a time.");
                }
                match r.key {
                    // Picked up first, then used, when it's the kind of thing
                    // the verb is for.
                    Key::Item(item)
                        if !r.carried
                            && operation != ItemOperation::Unequip
                            && r.class.is_some_and(|c| suits(operation, c)) =>
                    {
                        Interpretation::Sequence(vec![
                            goal(Goal::Take {
                                item,
                                quantity: (r.quantity > 1).then_some(1),
                            }),
                            Interpretation::Reread(format!(
                                "{} my {}",
                                verb.as_str(),
                                phrase(direct)
                            )),
                        ])
                    }
                    Key::Item(item) => match item_action(&scene.state.observation, item, operation)
                    {
                        Ok(_) => goal(Goal::UseItem { item, operation }),
                        Err(reason) => refuse(reason),
                    },
                    _ => refuse(format!("You can't {} {}.", verb.as_str(), r.the())),
                }
            })
        }
        (Verb::Put, Some((Preposition::On | Preposition::In, place)))
            if place
                .head
                .as_deref()
                .is_some_and(|h| matches!(h, "floor" | "ground")) =>
        {
            bind(direct, scene, referents, Domain::Carried, &|r| {
                drop(r, quantity, scene)
            })
        }
        (Verb::Put, _) => bind(direct, scene, referents, Domain::Carried, &|_| {
            refuse("There's nowhere here to put things but the floor.")
        }),
        (Verb::Open | Verb::Close, _) => {
            let open = verb == Verb::Open;
            bind(direct, scene, referents, Domain::Doors, &|r| door(r, open))
        }
        (Verb::Attack, weapon) => {
            let selected = if let Some((Preposition::With, weapon)) = weapon {
                match tor_client_common::weapons::selection_from_text(
                    &scene.state.observation,
                    &weapon.raw,
                ) {
                    Ok(selected) => Some(selected),
                    Err(reason) => return refuse(reason),
                }
            } else {
                None
            };
            bind(
                direct,
                scene,
                referents,
                Domain::Figures,
                &|r| match r.key {
                    Key::Actor(target) => match selected {
                        None => goal(Goal::Attack { target }),
                        Some(weapon) => {
                            match tor_client_common::abilities::basic_action_with_weapon(
                                &scene.state.observation,
                                target,
                                weapon,
                            ) {
                                Ok(Action::UseAbility {
                                    id,
                                    invocation,
                                    supplies,
                                }) => goal(Goal::UseAbility {
                                    ability: id,
                                    invocation,
                                    supplies,
                                }),
                                Ok(_) => unreachable!("basic attack is an ability"),
                                Err(reason) => refuse(reason),
                            }
                        }
                    },
                    Key::Me => refuse("You'd rather not hurt yourself."),
                    _ if r.class == Some(ItemClass::Corpse) || r.name.ends_with(" corpse") => {
                        refuse(format!(
                            "The {} is already dead.",
                            r.name.trim_end_matches(" corpse")
                        ))
                    }
                    _ => refuse(format!("Attacking {} would achieve nothing.", r.the())),
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
                    back: false,
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
                    back: true,
                })
            }
        }
        (Verb::Go, _) if remembered(direct, scene).is_some() => {
            let (key, name) = remembered(direct, scene).expect("checked");
            if crate::narrative::current_place_key(scene.view) == Some(key) {
                say(format!("You're already in {name}."))
            } else {
                goal(Goal::Visit {
                    destination: key.to_owned(),
                    back: scene.places.visited.contains(key),
                    name,
                })
            }
        }
        (Verb::Go | Verb::Step, _) => {
            bind(direct, scene, referents, Domain::Any, &|r| match r.way {
                Some(direction) if r.is(Kind::Feature) => directional(direction, scene),
                _ => approach(r),
            })
        }
        // Through a passage, up the stairs: the way it leads.
        (Verb::Enter | Verb::Climb, _)
            if matches!(resolve(direct, scene, referents, Domain::Any),
                Resolution::One(key) if scene.get(key).is_some_and(|r| r.is(Kind::Feature) && r.way.is_some())) =>
        {
            bind(direct, scene, referents, Domain::Any, &|r| match r.way {
                Some(direction) => directional(direction, scene),
                None => cannot(verb),
            })
        }
        (verb, _) if unsupported(verb).is_some() => {
            // Name what's meant first: "You can't see any lamp here" comes
            // before what there's no point doing.
            match resolve(direct, scene, referents, Domain::Any) {
                Resolution::Missing(text) => refuse(text),
                _ => cannot(verb),
            }
        }
        (verb, _) => refuse(format!(
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
        (Kind::Me, _) => refuse("You can't take yourself."),
        (Kind::Figure, _) => refuse(format!("You can't carry {}.", r.the())),
        _ => refuse(format!("You can't take {}.", r.the())),
    }
}

fn drop(r: &Referent, quantity: Option<u64>, scene: &Scene) -> Interpretation {
    let held_light = match r.key {
        Key::Item(item) => scene
            .state
            .observation
            .interactions
            .as_ref()
            .is_some_and(|view| {
                view.inventory
                    .iter()
                    .any(|row| row.item == item && row.portable_light.is_some())
            }),
        _ => false,
    };
    if let Some(how) = r.equipped.as_ref().filter(|_| !held_light) {
        return refuse(format!("You'll have to remove {} first ({how}).", r.the()));
    }
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
        _ => refuse(format!("You aren't carrying {}.", r.the())),
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
        _ => refuse(format!(
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
        Kind::Me => {
            let held: Vec<String> = scene
                .referents
                .iter()
                .filter_map(|t| t.equipped.as_ref().map(|how| format!("{} ({how})", t.a())))
                .collect();
            if held.is_empty() {
                diagnose(scene.state)
            } else {
                format!(
                    "{} You have {}.",
                    diagnose(scene.state),
                    prose::and_list(&held)
                )
            }
        }
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
            let the = prose::capitalize(&r.the());
            let place = if r.reachable {
                "within reach".to_owned()
            } else {
                r.whereabouts().unwrap_or_else(|| "here".into())
            };
            let mut sentences = vec![match (r.description.is_empty(), stature(r, scene)) {
                (false, _) => prose::sentence(&r.description),
                (true, Some(stature)) => format!("{the} {stature}, {place}."),
                (true, None) => format!("{the} is {place}."),
            }];
            if let Some(hurt) = injury(r, scene) {
                sentences.push(hurt.replacen(&prose::capitalize(&r.the()), "It", 1));
            }
            if let Some(attire) = attire(r, scene) {
                sentences.push(attire);
            }
            sentences.push(match r.hostile {
                Some(true) => "It means you harm.".into(),
                Some(false) => "It doesn't seem hostile.".into(),
                None => String::new(),
            });
            if !r.description.is_empty() {
                sentences.push(format!("It's {place}."));
            }
            sentences.retain(|s| !s.is_empty());
            sentences.join(" ")
        }
        Kind::Trap => prose::sentence(&r.description),
        Kind::Feature => feature_text(r),
        Kind::Thing => {
            let mut sentences = vec![if r.description.is_empty() {
                kind_of_thing(r)
            } else {
                prose::sentence(&r.description)
            }];
            if r.quantity > 1 {
                sentences.push(format!("There are {} of them.", prose::number(r.quantity)));
            }
            sentences.push(match (&r.equipped, r.carried) {
                (Some(how), _) => format!("You're carrying it ({how})."),
                (None, true) => "You're carrying it.".into(),
                (None, false) => match r.whereabouts() {
                    Some(w) if w == "at your feet" => "It lies at your feet.".into(),
                    Some(w) => format!("It lies {w}."),
                    None => String::new(),
                },
            });
            sentences.retain(|s| !s.is_empty());
            sentences.join(" ")
        }
    }
}

/// What kind of thing it is, from its disclosed class, when nothing more is
/// written about it.
fn kind_of_thing(r: &Referent) -> String {
    let the = prose::capitalize(&r.the());
    let is = if r.quantity > 1 { "are" } else { "is" };
    match r.class {
        Some(ItemClass::Corpse) => {
            let whose = r.name.trim_end_matches(" corpse");
            if whose == r.name {
                format!("{the} {is} lifeless.")
            } else {
                format!("{the} {is} all that's left of the {whose}.")
            }
        }
        Some(ItemClass::Weapon) => format!("{the} {is} a weapon."),
        Some(ItemClass::Armor) => format!("{the} {is} armor, made to be worn."),
        Some(ItemClass::Potion) => format!("{the} {is} a potion, to drink."),
        Some(ItemClass::Food) => format!("{the} {is} something to eat."),
        Some(ItemClass::Tool) => format!("{the} {is} a tool."),
        Some(ItemClass::Amulet) => format!("{the} {is} an amulet, to wear."),
        Some(ItemClass::Ring) => format!("{the} {is} a ring, to wear."),
        Some(ItemClass::Scroll) => format!("{the} {is} a scroll."),
        Some(ItemClass::Spellbook) => format!("{the} {is} a spellbook."),
        Some(ItemClass::Wand) => format!("{the} {is} a {}.", super::scene::device_form(&r.name)),
        Some(ItemClass::Coin) => format!("{the} {is} money."),
        Some(ItemClass::Gem) => format!("{the} {is} a gem."),
        Some(ItemClass::Misc) | None => {
            format!(
                "You see nothing more about {} than its name tells you.",
                r.the()
            )
        }
    }
}

/// How a figure stands beside the character: "stands about your height".
/// What a figure visibly wears and holds, in the order the server gave it:
/// "It wears ring mail and holds a mace." Absent when nothing is disclosed,
/// which says nothing about what it may be wearing out of sight.
fn attire(r: &Referent, scene: &Scene) -> Option<String> {
    let Key::Actor(id) = r.key else {
        return None;
    };
    let o = &scene.state.observation;
    let actor = o.visible_actors.iter().find(|a| a.id == id)?;
    let attire = narration::Attire::of(&actor.worn);
    // Armour names are mass nouns ("ring mail"); weapons are counted.
    let held: Vec<String> = attire
        .holds
        .iter()
        .map(|name| prose::indefinite(name))
        .collect();
    let clauses: Vec<String> = [("wears", attire.wears), ("holds", held)]
        .into_iter()
        .filter(|(_, items)| !items.is_empty())
        .map(|(verb, items)| format!("{verb} {}", prose::and_list(&items)))
        .collect();
    (!clauses.is_empty()).then(|| format!("It {}.", prose::and_list(&clauses)))
}

fn stature(r: &Referent, scene: &Scene) -> Option<&'static str> {
    let o = &scene.state.observation;
    let mine = o
        .visible_actors
        .iter()
        .filter(|a| a.id == o.self_target)
        .map(|a| a.position.z)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    // Bodies are told apart only by cells seen, so only a match is sure.
    (r.height > 0 && usize::from(r.height) == mine).then_some("stands about your height")
}

fn surface(r: &Referent, scene: &Scene) -> String {
    let cells = &scene.view.scenery().observation.visible_cells;
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
    let mut kinds: Vec<(&str, &str, u64, Option<&str>)> = Vec::new();
    for r in scene
        .referents
        .iter()
        .filter(|r| r.is(Kind::Thing) && r.carried)
    {
        let how = r.equipped.as_deref();
        match kinds
            .iter_mut()
            .find(|(identity, _, _, h)| *identity == r.identity && *h == how)
        {
            Some((_, _, count, _)) => *count += r.quantity,
            None => kinds.push((&r.identity, &r.name, r.quantity, how)),
        }
    }
    let carried: Vec<String> = kinds
        .iter()
        .map(|(_, name, count, how)| match how {
            Some(how) => format!("{} ({how})", prose::counted(*count, name)),
            None => prose::counted(*count, name),
        })
        .collect();
    let mut text = if carried.is_empty() {
        "You are empty-handed.".into()
    } else {
        format!("You are carrying {}.", prose::and_list(&carried))
    };
    for item in &scene.state.observation.inventory {
        if let Some(details) =
            tor_client_common::items::equipment_details(&scene.state.observation, item.id)
        {
            text.push_str(&format!(
                " {} (#{}), known: {details}.",
                safe(&item.name),
                item.id
            ));
        }
        if let Some(charges) = tor_client_common::items::charges(&scene.state.observation, item.id)
        {
            text.push_str(&format!(
                " {} (#{}) has {charges} charges.",
                safe(&item.name),
                item.id
            ));
        }
    }
    if let Some(prompt) = tor_client_common::items::identification_prompt(&scene.state.observation)
    {
        text.push(' ');
        text.push_str(&prompt);
    }
    text
}

/// The character's condition, without numbers beyond HP.
/// How the character is, and what the run asks of them.
fn condition(state: &StateView) -> String {
    match objective_text(state) {
        Some(goal) => format!("{} {goal}", diagnose(state)),
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
    } else if u128::from(c.hp) * 4 >= u128::from(c.max_hp) * 3 {
        "You have a few cuts and bruises."
    } else if u128::from(c.hp) * 2 >= u128::from(c.max_hp) {
        "You are wounded."
    } else if u128::from(c.hp) * 4 >= u128::from(c.max_hp) {
        "You are badly wounded."
    } else {
        "You are close to death."
    };
    format!("{condition} (HP {}/{})", c.hp, c.max_hp)
}

fn session_command(session: &SessionCommand, scene: &Scene) -> Interpretation {
    let state = scene.state;
    let tool = Interpretation::Tool;
    match session {
        SessionCommand::AllSkips(None) => say(scene.settings.all_sentence()),
        SessionCommand::AllSkips(Some(text)) => match super::settings::classes(text) {
            Ok(classes) => Interpretation::AllSkips(classes),
            Err(why) => refuse(why),
        },
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
                snapshot: state.snapshot.clone(),
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
                match crate::narrative::named_place_key(scene.view) {
                    Some(key) => tool(Input::Command(Command::RenamePlace {
                        expected_revision: state.revision,
                        key: key.into(),
                        name: name.clone(),
                    })),
                    None => refuse("There's nothing here to name."),
                }
            } else {
                match crate::rename_place(&format!("{target} {name}"), state) {
                    Ok(input) => tool(input),
                    Err(e) => refuse(e),
                }
            }
        }
        SessionCommand::Annotate {
            source,
            audience,
            category,
            anchor,
            text,
        } => match crate::annotation(source, audience, category, anchor, text, &state.snapshot) {
            Ok(input) => tool(input),
            Err(e) => refuse(e),
        },
        SessionCommand::Wizard { command } => match crate::parse_wizard(command, state.revision) {
            Ok(input) => tool(input),
            Err(e) => refuse(e),
        },
    }
}
