//! Everything that happened between two prompts, as one passage.
use std::collections::BTreeSet;

use tor_client_common::narration;
use tor_protocol::*;

use super::{
    chronicle::{Beat, Figure, Who},
    prose::{self, capitalize, sentence},
    scene::direction_name,
    verbs::Goal,
};

/// What a turn did, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Record {
    pub entries: Vec<Entry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    /// An answer, question or refusal, said as is.
    Said(String),
    /// A description of the scene, set apart from the narration.
    Description(String),
    Episode(Episode),
    /// Things that happened outside any goal of the player's.
    Beats(Vec<Beat>),
}

/// One goal's attempt and what happened during it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Episode {
    pub goal: Goal,
    /// How its object was named when it began: "the copper token", "three
    /// arrows", "east".
    pub object: String,
    /// Whether the character set off on a journey for it.
    pub approached: bool,
    pub beats: Vec<Beat>,
    pub end: End,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum End {
    Done,
    /// The journey ended before arriving.
    Stopped(TravelPhase),
    /// Arrived, but a figure came into view, so the rest waits.
    Wary,
    /// What the goal was for is no longer there or within reach.
    Gone,
    /// The game refused; the text says why.
    Refused(String),
    /// Control was lost or the state replaced.
    Lost,
}

impl Record {
    pub fn say(&mut self, text: impl Into<String>) {
        let text = text.into();
        if !text.is_empty() {
            self.entries.push(Entry::Said(text));
        }
    }

    pub fn is_empty(&self) -> bool {
        self.entries.iter().all(|e| match e {
            Entry::Beats(beats) => beats.is_empty(),
            _ => false,
        })
    }
}

/// Builds sentences and decides how to refer to figures: "a ruin scout" when
/// first noticed, "the ruin scout" after, "it" in the very next sentence
/// when no other figure has been mentioned.
#[derive(Default)]
struct Teller {
    sentences: Vec<String>,
    named: BTreeSet<ActorId>,
    last: Option<ActorId>,
    next_last: Option<ActorId>,
    dead: BTreeSet<ActorId>,
}

impl Teller {
    fn the(&mut self, f: &Figure) -> String {
        self.refer(f, false)
    }

    fn a(&mut self, f: &Figure) -> String {
        self.refer(f, true)
    }

    fn refer(&mut self, f: &Figure, introduce: bool) -> String {
        let text = if self.last == Some(f.id) && self.named.len() == 1 {
            "it".to_owned()
        } else if introduce && !self.named.contains(&f.id) {
            prose::indefinite(&f.name)
        } else {
            prose::definite(&f.name)
        };
        self.named.insert(f.id);
        self.next_last = Some(f.id);
        text
    }

    fn who(&mut self, who: &Who) -> String {
        match who {
            Who::Me => "you".into(),
            Who::Figure(f) => self.the(f),
            Who::Unseen => "something".into(),
        }
    }

    fn say(&mut self, text: impl AsRef<str>) {
        let text = sentence(text.as_ref());
        // The same thing twice in a row is told once: "Time passes."
        if !text.is_empty() && self.sentences.last() != Some(&text) {
            self.sentences.push(text);
        }
        self.last = self.next_last.take();
    }

    fn take(&mut self) -> Option<String> {
        self.last = None;
        (!self.sentences.is_empty()).then(|| prose::paragraph(&std::mem::take(&mut self.sentences)))
    }
}

/// Whether a beat is told on its own, rather than through the goal it
/// belongs to.
fn told(beat: &Beat) -> bool {
    !matches!(
        beat,
        Beat::Stepped(_)
            | Beat::Took { .. }
            | Beat::Dropped { .. }
            | Beat::SetDoor { .. }
            | Beat::Waited
            | Beat::AttackBegan
            | Beat::Journey { .. }
            | Beat::Hp { .. }
            | Beat::Resync
    )
}

/// The passage for a whole turn.
pub fn compose(record: &Record) -> String {
    let mut blocks: Vec<String> = Vec::new();
    let mut teller = Teller::default();
    let mut simple: Option<(&'static str, Vec<String>)> = None;
    let flush_simple = |teller: &mut Teller, simple: &mut Option<(&'static str, Vec<String>)>| {
        if let Some((verb, objects)) = simple.take() {
            teller.say(format!("you {verb} {}", prose::and_list(&alike(&objects))));
        }
    };
    for entry in &record.entries {
        // Plain pickups and drops in a row read as one sentence.
        if let Entry::Episode(e) = entry {
            if let Some(verb) = simple_verb(e) {
                match &mut simple {
                    Some((v, objects)) if *v == verb => objects.push(e.object.clone()),
                    _ => {
                        flush_simple(&mut teller, &mut simple);
                        simple = Some((verb, vec![e.object.clone()]));
                    }
                }
                continue;
            }
        }
        flush_simple(&mut teller, &mut simple);
        match entry {
            Entry::Said(text) | Entry::Description(text) => {
                blocks.extend(teller.take());
                blocks.push(text.clone());
            }
            Entry::Episode(e) => episode(&mut teller, e),
            Entry::Beats(beats) => {
                own_actions(&mut teller, beats);
                tell(&mut teller, beats);
            }
        }
    }
    flush_simple(&mut teller, &mut simple);
    blocks.extend(teller.take());
    if let Some(hp) = hp_status(record) {
        blocks.push(hp);
    }
    blocks.join("\n")
}

/// Repeated objects counted together: "the two copper tokens".
fn alike(objects: &[String]) -> Vec<String> {
    let mut counted: Vec<(&String, u64)> = Vec::new();
    for object in objects {
        match counted.iter_mut().find(|(o, _)| *o == object) {
            Some((_, n)) => *n += 1,
            None => counted.push((object, 1)),
        }
    }
    counted
        .into_iter()
        .map(|(object, n)| match object.strip_prefix("the ") {
            Some(name) if n > 1 => format!("the {} {}", prose::number(n), prose::plural(name)),
            _ => object.clone(),
        })
        .collect()
}

fn simple_verb(e: &Episode) -> Option<&'static str> {
    if e.approached || e.end != End::Done || e.beats.iter().any(told) {
        return None;
    }
    match e.goal {
        Goal::Take { .. } => Some("pick up"),
        Goal::Drop { .. } => Some("drop"),
        _ => None,
    }
}

/// HP after the turn, when it changed.
fn hp_status(record: &Record) -> Option<String> {
    let changes: Vec<(u32, u32, u32)> = record
        .entries
        .iter()
        .flat_map(|e| match e {
            Entry::Episode(e) => e.beats.as_slice(),
            Entry::Beats(beats) => beats.as_slice(),
            _ => &[],
        })
        .filter_map(|b| match b {
            Beat::Hp { from, to, max } => Some((*from, *to, *max)),
            _ => None,
        })
        .collect();
    let (first, last) = (changes.first()?, changes.last()?);
    (first.0 != last.1).then(|| format!("HP {}/{}", last.1, last.2))
}

/// "pick it up", "open it".
fn finish(goal: &Goal, plural: bool) -> Option<String> {
    let it = if plural { "them" } else { "it" };
    Some(match goal {
        Goal::Take { .. } => format!("pick {it} up"),
        Goal::Door { open: true, .. } => format!("open {it}"),
        Goal::Door { open: false, .. } => format!("close {it}"),
        _ => return None,
    })
}

/// "picking it up".
fn purpose(goal: &Goal, plural: bool) -> Option<String> {
    let it = if plural { "them" } else { "it" };
    Some(match goal {
        Goal::Take { .. } => format!("picking {it} up"),
        Goal::Door { open: true, .. } => format!("opening {it}"),
        Goal::Door { open: false, .. } => format!("closing {it}"),
        Goal::Attack { .. } => format!("attacking {it}"),
        _ => return None,
    })
}

fn plural(object: &str) -> bool {
    let first = object
        .split_whitespace()
        .nth(usize::from(object.starts_with("the ")))
        .unwrap_or("");
    first.parse::<u64>().is_ok_and(|n| n > 1)
        || matches!(
            first,
            "two"
                | "three"
                | "four"
                | "five"
                | "six"
                | "seven"
                | "eight"
                | "nine"
                | "ten"
                | "eleven"
                | "twelve"
        )
}

fn episode(teller: &mut Teller, e: &Episode) {
    let object = &e.object;
    let them = plural(object);
    let split = e
        .beats
        .iter()
        .position(|b| matches!(b, Beat::Journey { .. }))
        .map_or(0, |i| i + 1);
    let (travel, after) = e.beats.split_at(split);
    let noted_on_the_way = travel.iter().any(told);
    match (&e.end, e.approached) {
        (End::Done, true) => {
            match &e.goal {
                Goal::Go { direction, .. } => {
                    teller.say(format!("you walk {}", direction_name(*direction)));
                    // The new place's description shows who is there.
                    let travel: Vec<Beat> = travel
                        .iter()
                        .filter(|b| !matches!(b, Beat::Appeared { .. }))
                        .cloned()
                        .collect();
                    tell(teller, &travel);
                }
                Goal::Attack { target } => {
                    let who = teller.the(&Figure {
                        id: *target,
                        name: object.trim_start_matches("the ").to_owned(),
                    });
                    teller.say(format!("you close in on {who}"));
                    tell(teller, travel);
                }
                goal => match finish(goal, them) {
                    Some(finish) if !noted_on_the_way => {
                        teller.say(format!("you walk over to {object} and {finish}"));
                    }
                    Some(finish) => {
                        teller.say(format!("you walk over to {object}"));
                        tell(teller, travel);
                        teller.say(format!("you {finish}"));
                    }
                    None => {
                        teller.say(format!("you walk over to {object}"));
                        tell(teller, travel);
                    }
                },
            }
            done_beats(teller, e, after);
        }
        (End::Done, false) => {
            match &e.goal {
                Goal::Take { .. } => teller.say(format!("you pick up {object}")),
                Goal::Drop { .. } => teller.say(format!("you drop {object}")),
                Goal::Door { open, .. } => teller.say(format!(
                    "you {} {object}",
                    if *open { "open" } else { "close" }
                )),
                Goal::Step { direction } => {
                    teller.say(format!("you step {}", direction_name(*direction)))
                }
                Goal::Attack { target } => {
                    // When the other side acts first, say what was attempted.
                    let first = after.iter().find(|b| told(b));
                    if !matches!(
                        first,
                        Some(Beat::Blow {
                            attacker: Who::Me,
                            ..
                        })
                    ) {
                        let figure = Figure {
                            id: *target,
                            name: object.trim_start_matches("the ").to_owned(),
                        };
                        let who = teller.the(&figure);
                        teller.say(format!("you ready an attack on {who}"));
                    }
                }
                Goal::Wait if !after.iter().any(told) => teller.say("time passes"),
                Goal::Wait => teller.say("you wait"),
                _ => {}
            }
            done_beats(teller, e, after);
        }
        (End::Stopped(phase), _) => {
            let intent = match &e.goal {
                Goal::Go { direction, .. } => {
                    format!("you set off {}", direction_name(*direction))
                }
                Goal::Attack { .. } => format!("you advance on {object}"),
                goal => match purpose(goal, them) {
                    Some(purpose) => format!("you head toward {object}, intent on {purpose}"),
                    None => format!("you head toward {object}"),
                },
            };
            match phase {
                TravelPhase::Hazard => {
                    teller.say(intent);
                    let mut stopped = false;
                    for beat in travel {
                        match beat {
                            Beat::Appeared {
                                figure,
                                whereabouts,
                            } if !stopped => {
                                stopped = true;
                                let who = teller.a(figure);
                                teller.say(format!(
                                    "{who} comes into view {whereabouts}, and you stop warily"
                                ));
                            }
                            beat => tell(teller, std::slice::from_ref(beat)),
                        }
                    }
                    if !stopped {
                        teller.say("something catches your eye, and you stop warily");
                    }
                }
                TravelPhase::Blocked => {
                    teller.say(format!("{intent}, but the way is blocked"));
                    tell(teller, travel);
                }
                TravelPhase::DecisionRequired => {
                    teller.say(intent);
                    tell(teller, travel);
                    teller.say("you are thrown off course and stop");
                }
                TravelPhase::ControlLost => {
                    teller.say(format!(
                        "{intent}, but stop as control passes to someone else"
                    ));
                    tell(teller, travel);
                }
                TravelPhase::WorldChanged => {
                    teller.say(format!(
                        "{intent}, but the world shifts around you, and you stop"
                    ));
                    tell(teller, travel);
                }
                TravelPhase::Active => {
                    // Still under way when play went quiet.
                    teller.say(format!("{intent}, but must wait for others to act"));
                    tell(teller, travel);
                }
                _ => {
                    teller.say(format!("{intent}, but can't find a way"));
                    tell(teller, travel);
                }
            }
            tell(teller, after);
        }
        (End::Wary, _) => {
            let appeared = e
                .beats
                .iter()
                .position(|b| matches!(b, Beat::Appeared { .. }));
            let reason = appeared.and_then(|i| match &e.beats[i] {
                Beat::Appeared {
                    figure,
                    whereabouts,
                } => Some((figure, whereabouts)),
                _ => None,
            });
            let purpose = purpose(&e.goal, them).unwrap_or_else(|| "going on".into());
            match reason {
                Some((figure, whereabouts)) => {
                    let figure = figure.clone();
                    let who = teller.a(&figure);
                    teller.say(format!(
                        "you walk over to {object}, but stop short of {purpose} as {who} comes into view {whereabouts}"
                    ));
                }
                None => teller.say(format!(
                    "you walk over to {object}, but stop short of {purpose}"
                )),
            }
            let rest: Vec<Beat> = e
                .beats
                .iter()
                .enumerate()
                .filter(|(i, _)| Some(*i) != appeared)
                .map(|(_, b)| b.clone())
                .collect();
            tell(teller, &rest);
        }
        (End::Gone, approached) => {
            let it = if them { "they're" } else { "it's" };
            match (&e.goal, approached) {
                (Goal::Door { .. }, true) => teller.say(format!(
                    "you walk over to {object}, but can't reach it from there"
                )),
                (_, true) => teller.say(format!(
                    "you walk over to where {object} was, but {it} no longer there"
                )),
                (_, false) => teller.say(format!("{} is no longer there", capitalize(object))),
            }
            tell(teller, &e.beats);
        }
        (End::Refused(text), approached) => {
            if approached {
                teller.say(format!("you walk over to {object}"));
            }
            tell(teller, &e.beats);
            teller.say(text);
        }
        (End::Lost, _) => tell(teller, &e.beats),
    }
}

/// The watched character's own actions, as seen between turns: by a
/// spectator, or after reconnecting.
fn own_actions(teller: &mut Teller, beats: &[Beat]) {
    for beat in beats {
        match beat {
            Beat::Stepped(direction) => {
                teller.say(format!("you move {}", direction_name(*direction)))
            }
            Beat::Took { name, quantity } => {
                teller.say(format!("you pick up {}", prose::counted(*quantity, name)))
            }
            Beat::Dropped { name, quantity } => {
                teller.say(format!("you drop {}", prose::counted(*quantity, name)))
            }
            Beat::SetDoor { name, open } => teller.say(format!(
                "you {} the {name}",
                if *open { "open" } else { "close" }
            )),
            Beat::Waited => teller.say("time passes"),
            Beat::AttackBegan => teller.say("you ready an attack"),
            _ => {}
        }
    }
}

/// The beats after a goal's action; an attack with no blow yet still says
/// what was done.
fn done_beats(teller: &mut Teller, e: &Episode, after: &[Beat]) {
    if matches!(e.goal, Goal::Attack { .. })
        && !after
            .iter()
            .any(|b| matches!(b, Beat::Blow { .. } | Beat::Interrupted))
    {
        teller.say(format!("you attack {}", e.object));
    }
    tell(teller, after);
}

/// Tell beats in order, joining those that belong together.
fn tell(teller: &mut Teller, beats: &[Beat]) {
    let mut i = 0;
    while i < beats.len() {
        let next = beats.get(i + 1);
        match &beats[i] {
            Beat::Blow {
                attacker,
                target,
                outcome,
            } => {
                let killed = matches!(
                    (target, next),
                    (Who::Figure(t), Some(Beat::Died(Who::Figure(d)))) if t.id == d.id
                ) && *outcome == AttackOutcome::Hit;
                let spoiled = *target == Who::Me
                    && matches!(next, Some(Beat::Interrupted))
                    && *outcome == AttackOutcome::Hit;
                let a = teller.who(attacker);
                let t = teller.who(target);
                let (verb, at) = if *attacker == Who::Me {
                    ("strike", "swing at")
                } else {
                    ("strikes", "swings at")
                };
                let (misses, harm) = if *attacker == Who::Me {
                    ("but miss", "but the blow does no harm")
                } else if *target == Who::Me {
                    ("and misses", "but the blow does you no harm")
                } else {
                    ("and misses", "but the blow does no harm")
                };
                let mut text = match outcome {
                    AttackOutcome::Hit => format!("{a} {verb} {t}"),
                    AttackOutcome::Miss => format!("{a} {at} {t} {misses}"),
                    AttackOutcome::NoInjury => format!("{a} {verb} {t}, {harm}"),
                };
                if killed {
                    if let Who::Figure(f) = target {
                        teller.dead.insert(f.id);
                    }
                    text.push_str(", and it falls dead");
                    i += 1;
                } else if spoiled {
                    text.push_str(", spoiling your attack");
                    i += 1;
                }
                teller.say(text);
            }
            Beat::Interrupted => teller.say("your attack is interrupted"),
            Beat::Died(Who::Me) => teller.say("you die. This run has ended"),
            Beat::Died(Who::Figure(f)) => {
                teller.dead.insert(f.id);
                let who = teller.the(f);
                teller.say(format!("{who} falls dead"));
            }
            Beat::Died(Who::Unseen) => {}
            Beat::Appeared {
                figure,
                whereabouts,
            } => {
                let who = teller.a(figure);
                teller.say(format!("you notice {who} {whereabouts}"));
            }
            Beat::Vanished(figure) => {
                if !teller.dead.contains(&figure.id) {
                    let who = teller.the(figure);
                    teller.say(format!("{who} is no longer in sight"));
                }
            }
            Beat::DoorChanged {
                name,
                whereabouts,
                open,
            } => teller.say(format!(
                "the {name} {whereabouts} swings {}",
                if *open { "open" } else { "shut" }
            )),
            Beat::Displaced => teller.say("you are moved against your will"),
            Beat::Impacted => teller.say("you slam into something solid"),
            Beat::Victory { terminal: true } => teller.say("you have done it! This run has ended."),
            Beat::Victory { terminal: false } => {
                teller.say("you have achieved your goal, and may keep exploring")
            }
            Beat::Objective(kind) => {
                let goal = narration::objective(*kind);
                let mut goal = goal.to_owned();
                if let Some(first) = goal.get(..1) {
                    goal.replace_range(..1, &first.to_lowercase());
                }
                teller.say(format!("your task is to {}", goal.trim_end_matches('.')));
            }
            Beat::Control(true) => teller.say("you are in control"),
            Beat::Control(false) => teller.say("you are now only watching"),
            Beat::Stepped(_)
            | Beat::Took { .. }
            | Beat::Dropped { .. }
            | Beat::SetDoor { .. }
            | Beat::Waited
            | Beat::AttackBegan
            | Beat::Journey { .. }
            | Beat::Hp { .. }
            | Beat::Resync => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scout() -> Figure {
        Figure {
            id: ActorId(2),
            name: "ruin scout".into(),
        }
    }

    fn episode(goal: Goal, object: &str, approached: bool, beats: Vec<Beat>, end: End) -> Entry {
        Entry::Episode(Episode {
            goal,
            object: object.into(),
            approached,
            beats,
            end,
        })
    }

    fn told(entries: Vec<Entry>) -> String {
        compose(&Record { entries })
    }

    const TOKEN: Goal = Goal::Take {
        item: 1,
        quantity: None,
    };

    #[test]
    fn a_blocked_journey_keeps_its_purpose() {
        let journey = vec![
            Beat::Stepped(Direction::East),
            Beat::Journey {
                phase: TravelPhase::Blocked,
            },
        ];
        assert_eq!(
            told(vec![episode(
                TOKEN,
                "the copper token",
                true,
                journey,
                End::Stopped(TravelPhase::Blocked)
            )]),
            "You head toward the copper token, intent on picking it up, but the way is blocked."
        );
    }

    #[test]
    fn something_gone_on_arrival_is_said_plainly() {
        assert_eq!(
            told(vec![episode(
                TOKEN,
                "the copper token",
                true,
                vec![],
                End::Gone
            )]),
            "You walk over to where the copper token was, but it's no longer there."
        );
    }

    #[test]
    fn waiting_with_nothing_happening_lets_time_pass() {
        assert_eq!(
            told(vec![episode(
                Goal::Wait,
                "it",
                false,
                vec![Beat::Waited],
                End::Done
            )]),
            "Time passes."
        );
        let blow = Beat::Blow {
            attacker: Who::Figure(scout()),
            target: Who::Me,
            outcome: AttackOutcome::Miss,
        };
        assert_eq!(
            told(vec![episode(
                Goal::Wait,
                "it",
                false,
                vec![Beat::Waited, blow],
                End::Done
            )]),
            "You wait. The ruin scout swings at you and misses."
        );
    }

    #[test]
    fn blows_by_unseen_attackers_name_no_one() {
        let beats = vec![
            Beat::Blow {
                attacker: Who::Unseen,
                target: Who::Me,
                outcome: AttackOutcome::NoInjury,
            },
            Beat::Hp {
                from: 50,
                to: 50,
                max: 50,
            },
        ];
        assert_eq!(
            told(vec![Entry::Beats(beats)]),
            "Something strikes you, but the blow does you no harm."
        );
    }

    #[test]
    fn a_figure_mentioned_again_is_it_unless_another_was_named() {
        let guardian = Figure {
            id: ActorId(3),
            name: "stone guardian".into(),
        };
        let beats = vec![
            Beat::Appeared {
                figure: scout(),
                whereabouts: "to the east".into(),
            },
            Beat::Blow {
                attacker: Who::Figure(scout()),
                target: Who::Me,
                outcome: AttackOutcome::Hit,
            },
            Beat::Appeared {
                figure: guardian.clone(),
                whereabouts: "to the north".into(),
            },
            Beat::Blow {
                attacker: Who::Figure(guardian),
                target: Who::Me,
                outcome: AttackOutcome::Miss,
            },
            Beat::Hp {
                from: 50,
                to: 47,
                max: 50,
            },
        ];
        assert_eq!(
            told(vec![Entry::Beats(beats)]),
            "You notice a ruin scout to the east. It strikes you. You notice a stone guardian to the north. The stone guardian swings at you and misses.\nHP 47/50"
        );
    }

    #[test]
    fn answers_and_descriptions_are_set_apart() {
        assert_eq!(
            told(vec![
                episode(TOKEN, "the copper token", false, vec![], End::Done),
                Entry::Description("A small room.".into()),
                Entry::Said("You are carrying a copper token.".into()),
            ]),
            "You pick up the copper token.\nA small room.\nYou are carrying a copper token."
        );
    }

    #[test]
    fn a_death_and_a_door_opened_elsewhere_are_told() {
        let beats = vec![
            Beat::DoorChanged {
                name: "oak door".into(),
                whereabouts: "to the west".into(),
                open: true,
            },
            Beat::Died(Who::Figure(scout())),
            Beat::Vanished(scout()),
        ];
        assert_eq!(
            told(vec![Entry::Beats(beats)]),
            "The oak door to the west swings open. The ruin scout falls dead."
        );
    }
}
