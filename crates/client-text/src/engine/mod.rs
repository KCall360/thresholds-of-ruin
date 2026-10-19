//! The interactive fiction engine: what the player types becomes game
//! actions, and everything that happens before the next prompt becomes one
//! passage. See docs/architecture/clients.md.
pub mod atmosphere;
pub mod chronicle;
pub mod narrate;
pub mod place;
pub mod prose;
pub mod resolve;
pub mod rhetoric;
pub mod scene;
pub mod scenery;
pub use scenery::seen;
pub mod settings;
pub mod turn;
pub mod verbs;

use std::collections::{BTreeSet, VecDeque};

use tor_protocol::*;

use crate::{
    parser::{self, idiom, lexicon, Meta, ParsedCommand, Reading, Token},
    Input,
};
use chronicle::{Beat, Chronicler};
use narrate::{Entry, Record};
use resolve::Referents;
use scene::Scene;
use turn::{Error, Link};
use verbs::{Interpretation, Question, Verbosity};

/// What a line of input led to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The passage to show before the next prompt.
    Passage(String),
    /// The player asked to leave; the passage comes first.
    Quit(String),
}

/// What a question waiting for its answer asked.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Asked {
    /// Which of these.
    Choice(Question),
    /// What a command acts on, or what to do with something named alone:
    /// the answer goes between `before` and `after`.
    Missing { before: String, after: String },
    /// Yes or no; yes does this.
    Confirm(Interpretation),
}

/// A question waiting for its answer, and the rest of the chain it
/// interrupted.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    asked: Asked,
    revision: u64,
    rest: Vec<String>,
}

/// What this client session observed, for a qualified account when the run ends.
#[derive(Clone, Debug, Default)]
struct Tally {
    /// Goals set about.
    goals: u64,
    /// Figures seen to fall, by name, in order.
    fallen: Vec<String>,
    /// HP lost across recoveries, which can exceed one u64 Health domain.
    wounds: u128,
    /// Whether the end has been told.
    told: bool,
}

/// What the engine keeps between turns. None of it is game state: it's lost
/// on restart, and a snapshot clears it.
#[derive(Default)]
pub struct Engine {
    pub referents: Referents,
    pending: Option<Pending>,
    last_line: Option<String>,
    chronicler: Chronicler,
    verbosity: Verbosity,
    /// Places described in full, by key: a brief arrival there names them.
    described: BTreeSet<String>,
    /// The keys places were first seen under.
    places: crate::narrative::Places,
    /// The place the character was in when the last passage ended, so
    /// moves told between turns (a spectator's) can arrive somewhere too.
    last_place: Option<String>,
    /// The player's settings for this client.
    pub settings: settings::Settings,
    /// The last sentence that did something, as read: what `attack again`
    /// repeats.
    last_sentence: Option<String>,
    /// Places the character has been in, in order, each with how to name
    /// it: where `back` goes.
    trail: Vec<(String, String)>,
    tally: Tally,
}

/// The sentences of a line, each with its own text as typed, so names and
/// notes keep their capitals.
fn sentences(line: &str) -> Vec<String> {
    let tokens = parser::tokenize(line);
    if parser::split_sentences(&tokens).len() <= 1 {
        return split_commands(line.trim());
    }
    let mut sentences = Vec::new();
    let mut current = String::new();
    let mut words =
        line.split_inclusive(|c: char| c.is_whitespace() || matches!(c, '.' | ';' | '!' | '?'));
    let mut quoted = false;
    let mut flush = |current: &mut String| {
        let mut text = current.trim().trim_end_matches(',').trim().to_owned();
        if let Some(rest) = text.strip_suffix(" and") {
            text = rest.trim_end_matches(',').trim().to_owned();
        }
        if !text.is_empty() {
            sentences.push(text);
        }
        current.clear();
    };
    for piece in words.by_ref() {
        quoted ^= piece.matches('"').count() % 2 == 1;
        let word = piece.trim_end_matches(|c: char| c.is_whitespace());
        if !quoted && word.eq_ignore_ascii_case("then") {
            flush(&mut current);
        } else if !quoted && word.ends_with(['.', ';', '!', '?']) {
            current.push_str(word.trim_end_matches(['.', ';', '!', '?']));
            flush(&mut current);
        } else {
            current.push_str(piece);
        }
    }
    flush(&mut current);
    sentences
        .into_iter()
        .flat_map(|s| split_commands(&s))
        .collect()
}

/// Whether a word begins a command of its own.
fn begins_command(word: &str) -> bool {
    let word = word.trim_matches(|c: char| !c.is_alphanumeric());
    lexicon::parse_verb(word).is_some()
        || matches!(
            word,
            "north"
                | "south"
                | "east"
                | "west"
                | "northeast"
                | "northwest"
                | "southeast"
                | "southwest"
                | "up"
                | "down"
                | "inventory"
                | "look"
                | "again"
        )
}

/// "inventory, look" and "take the sword and drop it" are two commands;
/// "take the token and the tablet" is one. A comma or "and" splits a
/// sentence only where a new command begins after it.
fn split_commands(sentence: &str) -> Vec<String> {
    let tokens = parser::tokenize(sentence);
    let first = tokens.first().and_then(Token::as_word).unwrap_or("");
    if tokens.iter().any(|t| matches!(t, Token::Quoted(_)))
        || matches!(
            first,
            "say" | "note" | "bookmark" | "name" | "annotate" | "wizard"
        )
    {
        return vec![sentence.to_owned()];
    }
    let words: Vec<&str> = sentence.split_whitespace().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    for (i, word) in words.iter().enumerate() {
        let next = words.get(i + 1).copied().unwrap_or("");
        let joiner = *word == "and" || *word == ",";
        if joiner && !current.is_empty() && begins_command(next) {
            parts.push(current.join(" "));
            current.clear();
            continue;
        }
        if let Some(stem) = word.strip_suffix(',') {
            if begins_command(next) {
                current.push(stem);
                parts.push(current.join(" "));
                current.clear();
                continue;
            }
        }
        current.push(word);
    }
    if !current.is_empty() {
        parts.push(current.join(" "));
    }
    parts
        .into_iter()
        .map(|p| p.trim().trim_end_matches(',').trim().to_owned())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Whether a line reads as a new command rather than an answer.
fn commands(line: &str, observation: &Observation) -> bool {
    let tokens = parser::tokenize(line);
    let first = tokens.first().and_then(Token::as_word).unwrap_or("");
    let (normalized, _) = idiom::normalize(line);
    tor_client_common::abilities::match_command(observation, line)
        .is_ok_and(|value| value.is_some())
        || matches!(idiom::read(&normalized), Some(Reading::Command(_)))
        || matches!(idiom::read(&normalized), Some(Reading::Meta(m)) if !matches!(m, Meta::Yes | Meta::No))
        || lexicon::parse_verb(first).is_some()
        || (tokens.len() == 1 && lexicon::parse_direction(first).is_some())
        || matches!(
            first,
            "save"
                | "sync"
                | "control"
                | "release"
                | "places"
                | "pace"
                | "history"
                | "note"
                | "bookmark"
                | "name"
                | "annotate"
                | "wizard"
                | "branch-history"
                | "all"
        )
}

/// Whether a line answers yes or no.
fn yes_or_no(line: &str) -> Option<bool> {
    match idiom::read(&idiom::normalize(line).0) {
        Some(Reading::Meta(Meta::Yes)) => Some(true),
        Some(Reading::Meta(Meta::No)) => Some(false),
        _ => None,
    }
}

/// The run is over: the character died, or it ended in victory.
fn ended(state: &StateView) -> bool {
    state
        .observation
        .combat
        .as_ref()
        .is_some_and(|c| c.terminal || c.dead)
}

fn hp(link: &impl Link) -> (u64, u64) {
    link.client()
        .state()
        .observation
        .combat
        .as_ref()
        .map_or((0, 0), |c| (c.hp, c.max_hp))
}

/// The most rounds one `fight` goes before stopping to let the player look.
const ROUNDS: usize = 24;
/// The most legs one `follow` goes.
const LEGS: usize = 6;
/// How often a fight gives chase before giving up.
const CHASES: usize = 3;
/// Bound an inter-round drain even if updates keep arriving. Hitting the
/// bound returns to the prompt without submitting another command.
const CATCH_UP_UPDATES: usize = 256;
/// Quiet that ends an inter-round drain; it never supplies input permission.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(150);

/// Verbs aimed at a figure, which "again" aims at the same one.
fn needs_figure(verb: lexicon::Verb) -> bool {
    matches!(
        verb,
        lexicon::Verb::Attack | lexicon::Verb::Fight | lexicon::Verb::Follow
    )
}

/// The choice an answer picks, if exactly one.
fn answer<'q>(question: &'q Question, line: &str) -> Option<&'q verbs::Choice> {
    let words: Vec<String> = parser::tokenize(line)
        .iter()
        .map(Token::text)
        .filter(|w| !lexicon::is_determiner(w) && w != "one" && w != "please")
        .collect();
    if let [single] = words.as_slice() {
        let index = single
            .parse::<usize>()
            .ok()
            .or_else(|| lexicon::parse_ordinal(single));
        if let Some(index) = index {
            return question.choices.get(index.wrapping_sub(1));
        }
    }
    if words.is_empty() {
        return None;
    }
    let ordinal = words.iter().find_map(|w| lexicon::parse_ordinal(w));
    let words: Vec<&String> = words
        .iter()
        .filter(|w| lexicon::parse_ordinal(w).is_none())
        .collect();
    let fits: Vec<&verbs::Choice> = question
        .choices
        .iter()
        .filter(|c| {
            words
                .iter()
                .all(|w| c.words.contains(w) || c.label.split_whitespace().any(|l| l == w.as_str()))
        })
        .collect();
    match (fits.as_slice(), ordinal) {
        ([one], None) => Some(one),
        (many, Some(n)) => many.get(n.wrapping_sub(1)).copied(),
        _ => None,
    }
}

impl Engine {
    /// Forget everything tied to the old state, as after a rewind.
    pub fn reset(&mut self) {
        self.referents.clear();
        self.pending = None;
        self.last_sentence = None;
    }

    /// The scene in view, read with the player's settings.
    fn scene<'a>(
        &'a self,
        state: &'a dyn scenery::ScenicView,
        palette: &'a tor_client_common::Palette,
    ) -> Scene<'a> {
        let mut scene = Scene::remembering(state, palette, &self.places);
        scene.settings = &self.settings;
        scene
    }

    /// The sentence again, when it was a bare verb asked again: "attack
    /// again" is the last attack, or an attack on what was last mentioned.
    fn again(&self, text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        let [word] = words.as_slice() else {
            return text.to_owned();
        };
        let Some(verb) = lexicon::parse_verb(word) else {
            return text.to_owned();
        };
        let same = self.last_sentence.as_ref().filter(|last| {
            last.split_whitespace()
                .next()
                .and_then(lexicon::parse_verb)
                .is_some_and(|v| v == verb || (needs_figure(v) && needs_figure(verb)))
        });
        match same {
            Some(last) if needs_figure(verb) => {
                // The last fight's target, attacked the way asked.
                let rest: Vec<&str> = last.split_whitespace().skip(1).collect();
                if rest.is_empty() {
                    word.to_string()
                } else {
                    format!("{word} {}", rest.join(" "))
                }
            }
            Some(last) => last.clone(),
            None if needs_figure(verb) && self.referents.him.is_some() => format!("{word} him"),
            None => format!("{word} it"),
        }
    }

    /// Whether an answer names something in view, so it can complete a
    /// question about what a command acts on.
    fn names_something(&self, link: &impl Link, answer: &str) -> bool {
        let tokens = parser::tokenize(&idiom::normalize(answer).0);
        let Ok(np) = parser::parse_noun_phrase(&tokens) else {
            return false;
        };
        let state = seen(link.client());
        let scene = Scene::new(&state, link.palette());
        np.all
            || !matches!(
                resolve::resolve(&np, &scene, &self.referents, resolve::Domain::Any),
                resolve::Resolution::Missing(_)
            )
            || parser::lexicon::parse_direction(answer.trim()).is_some()
    }

    /// The client settings, as a paragraph.
    fn options(&self, link: &impl Link) -> String {
        let detail = match self.verbosity {
            Verbosity::Brief => "brief: places in full the first time, then by name",
            Verbosity::Verbose => "verbose: places in full every time",
            Verbosity::Superbrief => "superbrief: places by name only",
        };
        format!(
            "Descriptions are {detail}. {} Journey steps are shown {} ms apart (pace <ms> changes it).",
            self.settings.all_sentence(),
            link.pace().as_millis()
        )
    }

    /// The scene as the game opens.
    pub fn welcome(&mut self, link: &impl Link) -> String {
        self.places.begin(link.client().state());
        self.note_described(link);
        self.last_place = self.place_key(link);
        // Where the trail back begins.
        if let Some(key) = &self.last_place {
            let state = link.client().state();
            let name = crate::narrative::place_title(state).unwrap_or_else(|| {
                let place = place::survey(state);
                crate::narrative::place_noun(state, link.palette(), &place, &self.places, true)
            });
            self.trail = vec![(key.clone(), name)];
            self.places.visited.insert(key.clone());
        }
        let mut text =
            crate::adventure::describe_in(&seen(link.client()), link.palette(), &self.places);
        if let Some(prompt) =
            tor_client_common::items::identification_prompt(&link.client().state().observation)
        {
            text.push('\n');
            text.push_str(&prompt);
        }
        text
    }

    /// The key of the place the character is in, as first seen.
    fn place_key(&mut self, link: &impl Link) -> Option<String> {
        self.places.learn(&seen(link.client()))
    }

    /// Remember that the place in view has been described in full.
    fn note_described(&mut self, link: &impl Link) {
        if let Some(key) = self.place_key(link) {
            self.described.insert(key);
        }
    }

    /// The scene as `look` shows it.
    fn look(&mut self, link: &impl Link) -> String {
        let state = seen(link.client());
        self.places.learn(&state);
        crate::adventure::look_with(&state, link.palette(), &self.places)
    }

    /// The place on arriving there, as fully as the verbosity asks.
    fn arrival(&mut self, link: &impl Link) -> String {
        let known = self
            .place_key(link)
            .is_some_and(|key| self.described.contains(&key));
        let state = seen(link.client());
        let (state, palette, places) = (&state, link.palette(), &self.places);
        match self.verbosity {
            Verbosity::Superbrief => {
                crate::adventure::brief_place_with(state, palette, places, false)
            }
            Verbosity::Brief if known => {
                crate::adventure::brief_place_with(state, palette, places, true)
            }
            _ => {
                let text = crate::adventure::describe_place_with(state, palette, places);
                self.note_described(link);
                text
            }
        }
    }

    /// Learn the names in the view in hand, so later beats can name them.
    pub fn learn(&mut self, link: &impl Link) {
        self.chronicler.learn(link.client().state(), link.palette());
    }

    /// Play one line of input to the end of its turn.
    pub async fn play(&mut self, link: &mut impl Link, line: &str) -> Result<Outcome, Error> {
        let mut line = line.trim().to_owned();
        let mut record = Record::default();
        if line.is_empty() {
            return Ok(Outcome::Passage(String::new()));
        }
        let normalized = idiom::normalize(&line).0;
        let repeat = matches!(line.to_lowercase().as_str(), "again" | "g")
            || matches!(idiom::read(&normalized), Some(Reading::Command(c)) if c == "again");
        if repeat {
            match &self.last_line {
                Some(last) => line = last.clone(),
                None => {
                    record.say("There's nothing to repeat.");
                    return Ok(Outcome::Passage(narrate::compose(&record)));
                }
            }
        }
        // Freeze the selected offer before either the flee idiom or a later
        // repeated command can reinterpret a number against a refreshed list.
        if sentences(&line).len() == 1 {
            if let Some(input) = crate::hold_input(&line, link.client().state()) {
                self.pending = None;
                match input {
                    Ok(Input::Command(Command::Act {
                        action: Action::Escape { hold },
                        ..
                    })) => {
                        let canonical = format!("escape hold #{hold}");
                        self.last_line = Some(canonical.clone());
                        self.last_sentence = Some(canonical);
                        let start = self.place_key(link);
                        self.carry_out(
                            link,
                            &mut record,
                            Interpretation::Goals(vec![verbs::Goal::Escape { hold }]),
                            &mut VecDeque::new(),
                        )
                        .await?;
                        self.finish(link, &mut record, start.as_deref());
                    }
                    Err(error) => record.say(error),
                    _ => unreachable!("hold input produces only an Escape action"),
                }
                return Ok(Outcome::Passage(narrate::compose(&record)));
            }
        }
        if sentences(&line).len() == 1 {
            if let Some(input) = crate::magic_input(&line, link.client().state()) {
                self.pending = None;
                match input {
                    Ok(Input::Command(Command::Act { action, .. })) => {
                        let start = self.place_key(link);
                        self.carry_out(
                            link,
                            &mut record,
                            Interpretation::Goals(vec![verbs::Goal::MagicItem { action }]),
                            &mut VecDeque::new(),
                        )
                        .await?;
                        self.finish(link, &mut record, start.as_deref());
                    }
                    Err(error) => record.say(error),
                    _ => unreachable!("magic input produces only item actions"),
                }
                return Ok(Outcome::Passage(narrate::compose(&record)));
            }
        }
        if let Some(input) = crate::advancement_input(&line)
            .or_else(|| crate::strike_input(&line))
            .or_else(|| {
                if sentences(&line).len() == 1 {
                    crate::intention_input(&line)
                } else {
                    None
                }
            })
        {
            self.pending = None;
            match input {
                Ok(input) => {
                    self.tool(link, &mut record, input).await?;
                }
                Err(error) => record.say(error),
            }
            return Ok(Outcome::Passage(narrate::compose(&record)));
        }
        let start = self.place_key(link);
        let mut chain: VecDeque<Result<Interpretation, String>> = VecDeque::new();
        let mut rest: Vec<String> = Vec::new();
        if let Some(pending) = self.pending.take() {
            let current = link.client().state().revision == pending.revision;
            let is_command = commands(&line, &link.client().state().observation);
            match pending.asked {
                Asked::Choice(question) => match answer(&question, &line) {
                    Some(choice) if current => {
                        if let Some(key) = choice.key {
                            let scene = Scene::new(link.client().state(), link.palette());
                            if let Some(r) = scene.get(key) {
                                self.referents.mention(r);
                            }
                        }
                        chain.push_back(Ok(choice.then.clone()));
                        chain.extend(pending.rest.into_iter().map(Err));
                    }
                    None if current && !is_command => {
                        let labels: Vec<String> =
                            question.choices.iter().map(|c| c.label.clone()).collect();
                        record.say(format!(
                            "Please choose {}, or type a new command.",
                            prose::or_list(&labels)
                        ));
                        self.pending = Some(Pending {
                            asked: Asked::Choice(question),
                            ..pending
                        });
                        return Ok(Outcome::Passage(narrate::compose(&record)));
                    }
                    _ => {}
                },
                // "What do you want to attack?" "The scout."
                Asked::Missing { before, after }
                    if current
                        && !before.is_empty()
                        && !is_command
                        && (self.names_something(link, &line)
                            || line
                                .split_whitespace()
                                .next()
                                .is_some_and(|w| scene::known_noun(&w.to_lowercase()))) =>
                {
                    line = format!("{before} {line} {after}").trim().to_owned();
                    rest = pending.rest;
                }
                // "What do you want to do with the scout?" "Attack."
                Asked::Missing { before, after }
                    if current
                        && before.is_empty()
                        && line.split_whitespace().count() <= 2
                        && line
                            .split_whitespace()
                            .next()
                            .and_then(|w| lexicon::parse_verb(&w.to_lowercase()))
                            .is_some() =>
                {
                    line = format!("{line} {after}");
                    rest = pending.rest;
                }
                Asked::Confirm(then) => {
                    match yes_or_no(&line) {
                        Some(true) if current => {
                            chain.push_back(Ok(then));
                            chain.extend(pending.rest.into_iter().map(Err));
                        }
                        Some(true) => {
                            record.say("Things have changed since you were asked. Look around and try again.");
                            return Ok(Outcome::Passage(narrate::compose(&record)));
                        }
                        Some(false) => {
                            record.say("All right.");
                            return Ok(Outcome::Passage(narrate::compose(&record)));
                        }
                        None if current && !is_command => {
                            record.say("Please answer yes or no, or type a new command.");
                            self.pending = Some(Pending {
                                asked: Asked::Confirm(then),
                                ..pending
                            });
                            return Ok(Outcome::Passage(narrate::compose(&record)));
                        }
                        None => {}
                    }
                }
                Asked::Missing { .. } => {}
            }
        }
        if chain.is_empty() && line.parse::<u64>().is_ok() {
            record.say("There's no question to answer.");
            return Ok(Outcome::Passage(narrate::compose(&record)));
        }
        if chain.is_empty() {
            self.last_line = Some(line.clone());
            chain.extend(sentences(&line).into_iter().map(Err));
            chain.extend(rest.into_iter().map(Err));
        }
        // `Err` holds a sentence still to be read; `Ok` one already understood.
        while let Some(next) = chain.pop_front() {
            let interpretation = match next {
                Ok(interpretation) => interpretation,
                Err(text) => {
                    let (text, modifiers) = idiom::normalize(&text);
                    let text = if modifiers.again {
                        self.again(&text)
                    } else {
                        text
                    };
                    if let Some(input) = crate::hold_input(&text, link.client().state()) {
                        match input {
                            Ok(Input::Command(Command::Act {
                                action: Action::Escape { hold },
                                ..
                            })) => {
                                let canonical = format!("escape hold #{hold}");
                                self.last_sentence = Some(canonical.clone());
                                if let Some(last) = &mut self.last_line {
                                    let mut parts = sentences(last);
                                    if let Some(part) = parts
                                        .iter_mut()
                                        .find(|part| idiom::normalize(part).0 == text)
                                    {
                                        *part = canonical;
                                        *last = parts.join(". ");
                                    }
                                }
                                chain.push_front(Ok(Interpretation::Goals(vec![
                                    verbs::Goal::Escape { hold },
                                ])));
                            }
                            Err(error) => {
                                record.say(error);
                                break;
                            }
                            _ => unreachable!("hold input produces only an Escape action"),
                        }
                        continue;
                    }
                    if let Some(input) = crate::magic_input(&text, link.client().state()) {
                        match input {
                            Ok(Input::Command(Command::Act { action, .. })) => {
                                chain.push_front(Ok(Interpretation::Goals(vec![
                                    verbs::Goal::MagicItem { action },
                                ])))
                            }
                            Err(error) => {
                                record.say(error);
                                break;
                            }
                            _ => unreachable!("magic input produces only item actions"),
                        }
                        continue;
                    }
                    if let Some(input) = crate::intention_input(&text) {
                        match input {
                            Ok(input) => chain.push_front(Ok(Interpretation::Tool(input))),
                            Err(error) => {
                                record.say(error);
                                break;
                            }
                        }
                        continue;
                    }
                    let tokens = parser::tokenize(&text);
                    match parser::match_disclosed_sentence(
                        &tokens,
                        &text,
                        &seen(link.client()).observation,
                    ) {
                        Ok(ParsedCommand::Again) => match &self.last_sentence {
                            Some(last) => {
                                chain.push_front(Err(last.clone()));
                                continue;
                            }
                            None => {
                                record.say("There's nothing to repeat.");
                                break;
                            }
                        },
                        Ok(command) => {
                            let command = if modifiers.persist {
                                persist(command)
                            } else {
                                command
                            };
                            if acts(&command) {
                                self.last_sentence = Some(text.clone());
                            }
                            let state = seen(link.client());
                            let scene = self.scene(&state, link.palette());
                            let mut referents = self.referents.clone();
                            let meant = verbs::interpret(&command, &scene, &mut referents);
                            drop(scene);
                            self.referents = referents;
                            meant
                        }
                        Err(message) => {
                            record.say(message);
                            break;
                        }
                    }
                }
            };
            let go_on = self
                .carry_out(link, &mut record, interpretation, &mut chain)
                .await?;
            match go_on {
                Flow::Continue => {}
                Flow::Stop => break,
                Flow::Quit => {
                    self.finish(link, &mut record, start.as_deref());
                    return Ok(Outcome::Quit(narrate::compose(&record)));
                }
            }
        }
        self.finish(link, &mut record, start.as_deref());
        Ok(Outcome::Passage(narrate::compose(&record)))
    }

    fn ask(
        &mut self,
        link: &impl Link,
        record: &mut Record,
        prompt: &str,
        asked: Asked,
        chain: &mut VecDeque<Result<Interpretation, String>>,
    ) -> Flow {
        record.say(prompt);
        self.pending = Some(Pending {
            asked,
            revision: link.client().state().revision,
            rest: chain.drain(..).filter_map(|c| c.err()).collect(),
        });
        Flow::Stop
    }

    async fn carry_out(
        &mut self,
        link: &mut impl Link,
        record: &mut Record,
        interpretation: Interpretation,
        chain: &mut VecDeque<Result<Interpretation, String>>,
    ) -> Result<Flow, Error> {
        Ok(match interpretation {
            Interpretation::Say(text) => {
                record.say(text);
                Flow::Continue
            }
            Interpretation::Refuse(text) => {
                record.say(text);
                Flow::Stop
            }
            Interpretation::Look => {
                self.note_described(link);
                record.entries.push(Entry::Description(self.look(link)));
                Flow::Continue
            }
            Interpretation::Describe(verbosity) => {
                self.verbosity = verbosity;
                record.say(match verbosity {
                    Verbosity::Brief => {
                        "Places are described in full the first time you arrive, and named after that."
                    }
                    Verbosity::Verbose => "Places are described in full every time you arrive.",
                    Verbosity::Superbrief => "Places are only named when you arrive.",
                });
                Flow::Continue
            }
            Interpretation::Ask(question) => {
                let prompt = question.prompt.clone();
                self.ask(link, record, &prompt, Asked::Choice(question), chain)
            }
            Interpretation::Orphan {
                prompt,
                before,
                after,
            } => self.ask(
                link,
                record,
                &prompt,
                Asked::Missing { before, after },
                chain,
            ),
            Interpretation::Confirm { prompt, then } => {
                self.ask(link, record, &prompt, Asked::Confirm(*then), chain)
            }
            Interpretation::Sequence(parts) => {
                for part in parts {
                    match Box::pin(self.carry_out(link, record, part, chain)).await? {
                        Flow::Continue => {}
                        other => return Ok(other),
                    }
                }
                Flow::Continue
            }
            Interpretation::Reread(text) => {
                chain.push_front(Err(text));
                Flow::Continue
            }
            Interpretation::Quit => Flow::Quit,
            Interpretation::Goals(goals) => {
                if let Err(why) = turn::may_act(link) {
                    record.say(why);
                    return Ok(Flow::Stop);
                }
                let count = goals.len();
                for (i, goal) in goals.into_iter().enumerate() {
                    let fighting = matches!(
                        goal,
                        verbs::Goal::Attack { .. } | verbs::Goal::UseAbility { .. }
                    );
                    let (before, _) = hp(link);
                    let done = turn::run_goal(link, &mut self.chronicler, record, goal).await?;
                    self.tally.goals += 1;
                    self.after_beats(link, record);
                    if !done {
                        return Ok(Flow::Stop);
                    }
                    // A wound mid-plan ends the plan: the player decides what
                    // next, as they would on the way somewhere.
                    let (after, _) = hp(link);
                    if after < before && !fighting && (i + 1 < count || !chain.is_empty()) {
                        record.say("Hurt, you stop to take stock.");
                        chain.clear();
                        return Ok(Flow::Stop);
                    }
                }
                Flow::Continue
            }
            Interpretation::Fight(target) => self.fight(link, record, target).await?,
            Interpretation::Follow(target) => self.follow(link, record, target).await?,
            Interpretation::Back { flee } => self.back(link, record, flee, chain).await?,
            Interpretation::Options => {
                record.say(self.options(link));
                Flow::Continue
            }
            Interpretation::AllSkips(classes) => {
                self.settings.all_skips = classes;
                record.say(self.settings.all_sentence());
                Flow::Continue
            }
            Interpretation::Tool(input) => self.tool(link, record, input).await?,
        })
    }

    /// Read what the server says after an action before acting again, as
    /// the prompt does between commands: readiness and the context the next
    /// request needs, and anything that happened meanwhile.
    async fn catch_up(&mut self, link: &mut impl Link, record: &mut Record) -> Result<Flow, Error> {
        let mut reader = turn::Reader {
            chronicler: &mut self.chronicler,
            beats: Vec::new(),
            pages: Vec::new(),
            resynced: false,
            waiting: None,
        };
        let mut flow = None;
        let mut why = None;
        for _ in 0..CATCH_UP_UPDATES {
            let before = link.client().state().clone();
            let Some(message) = link.next(SETTLE).await? else {
                flow = Some(Flow::Continue);
                break;
            };
            reader.record(&before, link, &message);
            if let ServerMessage::Waiting { on } = message {
                if on != Waiting::You {
                    why = Some(turn::waiting_reason(Some(on)));
                    flow = Some(Flow::Stop);
                    break;
                }
            }
            // Never consume a stop boundary and then mistake a later update
            // for authorization to continue the old intention or its chain.
            if reader.execution_refused() || reader.resynced || turn::may_act(link).is_err() {
                flow = Some(Flow::Stop);
                break;
            }
        }
        if !reader.beats.is_empty() {
            record.entries.push(Entry::Beats(reader.beats));
        }
        if flow.is_none() {
            why = Some("Updates are still arriving; you pause to take stock.");
        } else if matches!(flow, Some(Flow::Continue)) && !link.client().can_admit_intention() {
            why = Some("Input permissions are still updating; you pause to take stock.");
            flow = Some(Flow::Stop);
        }
        if let Some(why) = why {
            record.say(why);
        }
        self.after_beats(link, record);
        Ok(flow.unwrap_or(Flow::Stop))
    }

    /// Caution follows every disclosed transition, including changes that
    /// arrive between rounds or are followed by another observation.
    fn goal_caution(
        record: &Record,
        since: usize,
        known: &BTreeSet<ActorTarget>,
        fighting: bool,
    ) -> Option<&'static str> {
        let beats = || {
            record.entries[since..]
                .iter()
                .flat_map(|entry| match entry {
                    Entry::Episode(episode) => episode.beats.as_slice(),
                    Entry::Beats(beats) => beats.as_slice(),
                    _ => &[],
                })
        };
        if beats().any(
            |beat| matches!(beat, Beat::Appeared { figure, .. } if !known.contains(&figure.id)),
        ) {
            Some(if fighting {
                "You break off the fight to take stock."
            } else {
                "You stop following to take stock."
            })
        } else if beats()
            .any(|beat| matches!(beat, Beat::Hp { from, to, max } if to < from && (!fighting || *to <= max / 4)))
        {
            Some(if fighting {
                "Badly hurt, you break off the fight."
            } else {
                "Hurt, you stop to take stock."
            })
        } else {
            None
        }
    }

    /// How the last goal in the record ended.
    fn last_end(record: &Record) -> Option<&narrate::End> {
        record.entries.iter().rev().find_map(|e| match e {
            Entry::Episode(e) => Some(&e.end),
            _ => None,
        })
    }

    /// Attack, and keep attacking, until the target falls or something
    /// calls a halt: someone new, a bad wound, a foe that won't be caught.
    async fn fight(
        &mut self,
        link: &mut impl Link,
        record: &mut Record,
        target: ActorTarget,
    ) -> Result<Flow, Error> {
        if let Err(why) = turn::may_act(link) {
            record.say(why);
            return Ok(Flow::Stop);
        }
        let known = Scene::new(link.client().state(), link.palette()).figure_ids();
        let mut chases = 0;
        for round in 0..ROUNDS {
            if round > 0 {
                let since = record.entries.len();
                if matches!(self.catch_up(link, record).await?, Flow::Stop) {
                    return Ok(Flow::Stop);
                }
                if let Some(why) = Self::goal_caution(record, since, &known, true) {
                    record.say(why);
                    return Ok(Flow::Stop);
                }
            }
            if Scene::new(link.client().state(), link.palette())
                .get(scene::Key::Actor(target))
                .is_none()
            {
                return Ok(Flow::Continue);
            }
            let since = record.entries.len();
            let done = turn::run_goal(
                link,
                &mut self.chronicler,
                record,
                verbs::Goal::Attack { target },
            )
            .await?;
            self.tally.goals += 1;
            self.after_beats(link, record);
            if ended(link.client().state()) {
                return Ok(Flow::Stop);
            }
            if !done {
                let away = matches!(Self::last_end(record),
                    Some(narrate::End::Refused(text)) if text == turn::OUT_OF_REACH);
                chases += 1;
                if !away {
                    return Ok(Flow::Stop);
                }
                if chases > CHASES {
                    record.say("It keeps away from you, and you give up the chase.");
                    return Ok(Flow::Stop);
                }
            }
            if let Some(why) = Self::goal_caution(record, since, &known, true) {
                record.say(why);
                return Ok(Flow::Stop);
            }
            if Scene::new(link.client().state(), link.palette())
                .get(scene::Key::Actor(target))
                .is_none()
            {
                return Ok(if done { Flow::Continue } else { Flow::Stop });
            }
        }
        record.say("You pause to catch your breath.");
        Ok(Flow::Stop)
    }

    /// Close in on a figure, following as it moves, until it's within
    /// reach or gone.
    async fn follow(
        &mut self,
        link: &mut impl Link,
        record: &mut Record,
        target: ActorTarget,
    ) -> Result<Flow, Error> {
        if let Err(why) = turn::may_act(link) {
            record.say(why);
            return Ok(Flow::Stop);
        }
        let name = Scene::new(link.client().state(), link.palette())
            .get(scene::Key::Actor(target))
            .map(scene::Referent::the)
            .unwrap_or_else(|| "it".into());
        let known = Scene::new(link.client().state(), link.palette()).figure_ids();
        for leg in 0..LEGS {
            if leg > 0 {
                let since = record.entries.len();
                if matches!(self.catch_up(link, record).await?, Flow::Stop) {
                    return Ok(Flow::Stop);
                }
                if let Some(why) = Self::goal_caution(record, since, &known, false) {
                    record.say(why);
                    return Ok(Flow::Stop);
                }
            }
            let scene = Scene::new(link.client().state(), link.palette());
            match scene.get(scene::Key::Actor(target)) {
                None => {
                    record.say(format!("You lose sight of {name}."));
                    return Ok(Flow::Stop);
                }
                Some(r) if r.reachable => return Ok(Flow::Continue),
                Some(_) => {}
            }
            drop(scene);
            let since = record.entries.len();
            let done = turn::run_goal(
                link,
                &mut self.chronicler,
                record,
                verbs::Goal::Approach {
                    target: scene::Key::Actor(target),
                },
            )
            .await?;
            self.tally.goals += 1;
            self.after_beats(link, record);
            if !done {
                return Ok(Flow::Stop);
            }
            if let Some(why) = Self::goal_caution(record, since, &known, false) {
                record.say(why);
                return Ok(Flow::Stop);
            }
        }
        if Scene::new(link.client().state(), link.palette())
            .get(scene::Key::Actor(target))
            .is_some_and(|r| r.reachable)
        {
            Ok(Flow::Continue)
        } else {
            record.say("You stop following to take stock.");
            Ok(Flow::Stop)
        }
    }

    /// Back to the place before this one, or where the character began.
    async fn back(
        &mut self,
        link: &mut impl Link,
        record: &mut Record,
        flee: bool,
        chain: &mut VecDeque<Result<Interpretation, String>>,
    ) -> Result<Flow, Error> {
        let here = self.place_key(link);
        let previous = self
            .trail
            .iter()
            .rev()
            .skip_while(|(key, _)| Some(key) == here.as_ref())
            .find(|(key, _)| Some(key) != here.as_ref())
            .cloned()
            .or_else(|| {
                self.places
                    .start
                    .clone()
                    .filter(|start| {
                        crate::narrative::here_key(link.client().state()) != Some(start)
                    })
                    .map(|start| (start, "where you started".to_owned()))
            });
        let Some((destination, name)) = previous else {
            record.say(if flee {
                "There's nowhere further back to flee to."
            } else {
                "There's nowhere further back to go."
            });
            return Ok(Flow::Stop);
        };
        if flee {
            record.say("You turn and flee.");
        }
        let visit = Interpretation::Goals(vec![verbs::Goal::Visit {
            destination,
            name,
            back: true,
        }]);
        Box::pin(self.carry_out(link, record, visit, chain)).await
    }

    async fn tool(
        &mut self,
        link: &mut impl Link,
        record: &mut Record,
        input: Input,
    ) -> Result<Flow, Error> {
        let request = match input {
            Input::Advancement(input) => match link.advancement_request(input) {
                Ok(Some(request)) => request,
                Ok(None) => {
                    record.say(link.advancement_lines().join("\n"));
                    return Ok(Flow::Continue);
                }
                Err(error) => {
                    record.say(error);
                    return Ok(Flow::Stop);
                }
            },
            Input::Look => {
                self.note_described(link);
                record.entries.push(Entry::Description(self.look(link)));
                return Ok(Flow::Continue);
            }
            Input::Places => {
                record.say(crate::places(link.client().state()));
                return Ok(Flow::Continue);
            }
            Input::Inventory => {
                let scene = Scene::new(link.client().state(), link.palette());
                record.say(verbs::inventory(&scene));
                return Ok(Flow::Continue);
            }
            Input::Help => {
                record.say(verbs::HELP);
                return Ok(Flow::Continue);
            }
            Input::Stats => {
                record.say(crate::stats(link.client().state()));
                return Ok(Flow::Continue);
            }
            Input::Quit => return Ok(Flow::Quit),
            Input::Pace(None) => {
                record.say(format!(
                    "Turns are shown with {} ms between updates.",
                    link.pace().as_millis()
                ));
                return Ok(Flow::Continue);
            }
            Input::Pace(Some(ms)) => {
                link.set_pace(std::time::Duration::from_millis(ms));
                record.say(format!("Turns are now shown with {ms} ms between updates."));
                return Ok(Flow::Continue);
            }
            input @ (Input::ResumeIntention | Input::CancelIntention { .. }) => {
                if let Err(why) = turn::may_act(link) {
                    record.say(why);
                    return Ok(Flow::Stop);
                }
                match crate::intention_request(&input, link.client()) {
                    Ok(request) => request,
                    Err(error) => {
                        record.say(error);
                        return Ok(Flow::Stop);
                    }
                }
            }
            input @ (Input::StrikeSource(_) | Input::StrikeNext | Input::StrikePrevious) => {
                if link.role() != AccessRole::Wizard {
                    record.say("Wizard authority is required.");
                    return Ok(Flow::Stop);
                }
                let context = link.client().snapshot().reply_context();
                let command = link
                    .inspection_pages()
                    .ok_or_else(|| "Open a creature inspection first.".to_owned())
                    .and_then(|pages| crate::strike_command(&input, pages, &context));
                match command {
                    Ok(command) => link.client().command_request(command),
                    Err(error) => {
                        record.say(error);
                        return Ok(Flow::Stop);
                    }
                }
            }
            Input::Request(request) => request,
            Input::Command(command) => {
                if matches!(command, Command::Wizard { .. }) && link.role() != AccessRole::Wizard {
                    record.say("Wizard authority is required.");
                    return Ok(Flow::Stop);
                }
                link.client().command_request(command)
            }
        };
        let confirmation = match &request {
            Request::Command {
                command: Command::RenamePlace { name, .. },
                ..
            } => Some(format!("You name the place {}.", crate::safe(name))),
            Request::Command {
                command: Command::Annotate { .. },
                ..
            } => Some("Noted.".to_owned()),
            Request::Save => Some("Saved.".to_owned()),
            _ => None,
        };
        let done = turn::run_request(link, &mut self.chronicler, record, request).await?;
        if let Some(confirmation) = confirmation.filter(|_| done) {
            record.say(confirmation);
        }
        self.after_beats(link, record);
        Ok(if done { Flow::Continue } else { Flow::Stop })
    }

    /// Keep pronouns and memory in step with what was just recorded.
    fn after_beats(&mut self, link: &impl Link, record: &Record) {
        let resynced = record.entries.iter().any(|e| match e {
            Entry::Episode(e) => e.beats.contains(&Beat::Resync),
            Entry::Beats(beats) => beats.contains(&Beat::Resync),
            _ => false,
        });
        if resynced {
            self.reset();
            return;
        }
        // Whatever the narration introduces last becomes "it".
        let scene = Scene::new(link.client().state(), link.palette());
        let last_seen = record
            .entries
            .last()
            .and_then(|e| match e {
                Entry::Episode(e) => Some(e.beats.as_slice()),
                Entry::Beats(beats) => Some(beats.as_slice()),
                _ => None,
            })
            .and_then(|beats| {
                beats.iter().rev().find_map(|b| match b {
                    Beat::Appeared { figure, .. } => Some(figure.id),
                    _ => None,
                })
            });
        if let Some(r) = last_seen.and_then(|id| scene.get(scene::Key::Actor(id))) {
            self.referents.mention(r);
        }
    }

    /// End-of-turn additions: a description after a snapshot or on arriving
    /// somewhere new. `start` is the place the turn began in, if any.
    fn finish(&mut self, link: &impl Link, record: &mut Record, start: Option<&str>) {
        if let Some(prompt) =
            tor_client_common::items::identification_prompt(&link.client().state().observation)
        {
            record.say(prompt);
        }
        let resynced = record.entries.iter().any(|e| match e {
            Entry::Episode(e) => e.beats.contains(&Beat::Resync),
            Entry::Beats(beats) => beats.contains(&Beat::Resync),
            _ => false,
        });
        // Arriving somewhere else, whether by a direction, by name or by
        // walking over to something; a direction across open ground can
        // leave the character in the same place.
        let here = self.place_key(link);
        self.last_place.clone_from(&here);
        if let Some(key) = &here {
            let n = self.trail.len();
            // Going back the way you came takes the last step off the trail,
            // so `back` again goes further back, never forward.
            if n >= 2 && &self.trail[n - 2].0 == key {
                self.trail.pop();
            } else if self.trail.last().map(|(k, _)| k) != Some(key) {
                let state = link.client().state();
                let name = crate::narrative::place_title(state).unwrap_or_else(|| {
                    let place = place::survey(state);
                    crate::narrative::place_noun(state, link.palette(), &place, &self.places, true)
                });
                self.trail.push((key.clone(), name));
                self.places.visited.insert(key.clone());
            }
        }
        self.count(record);
        let moved = start.is_some_and(|start| here.is_some_and(|here| here != start));
        if resynced {
            self.note_described(link);
            let text =
                crate::adventure::describe_in(&seen(link.client()), link.palette(), &self.places);
            record.entries.push(Entry::Description(text));
        } else if moved && !matches!(record.entries.last(), Some(Entry::Description(_))) {
            // A `look` after the move already described where it ended.
            // The description says who is there; sightings on the way in
            // would say it twice.
            let present: BTreeSet<ActorTarget> = link
                .client()
                .state()
                .observation
                .visible_actors
                .iter()
                .map(|a| a.id)
                .collect();
            for entry in &mut record.entries {
                let beats = match entry {
                    // A sighting that cut a goal short is why it stopped.
                    Entry::Episode(e) if e.end == narrate::End::Done => &mut e.beats,
                    Entry::Beats(beats) => beats,
                    _ => continue,
                };
                beats.retain(|b| match b {
                    Beat::Appeared { figure, .. } | Beat::Vanished(figure) => {
                        !present.contains(&figure.id)
                    }
                    Beat::Spotted { .. } => false,
                    _ => true,
                });
            }
            let arrival = self.arrival(link);
            record.entries.push(Entry::Description(arrival));
        }
        if ended(link.client().state()) && !self.tally.told {
            self.tally.told = true;
            record.entries.push(Entry::Description(self.ending(link)));
        }
    }

    /// Keep this session's tally from what was just recorded.
    fn count(&mut self, record: &Record) {
        for entry in &record.entries {
            let beats = match entry {
                Entry::Episode(e) => &e.beats,
                Entry::Beats(beats) => beats,
                _ => continue,
            };
            for beat in beats {
                match beat {
                    Beat::Died(chronicle::Who::Figure(f)) => self.tally.fallen.push(f.name.clone()),
                    Beat::Hp { from, to, .. } if to < from => {
                        self.tally.wounds += u128::from(from - to);
                    }
                    _ => {}
                }
            }
        }
    }

    /// The observed session account, never an authoritative whole-run history.
    fn ending(&self, link: &impl Link) -> String {
        let state = link.client().state();
        let combat = state.observation.combat.as_ref();
        let died = combat.is_some_and(|c| c.dead);
        let banner = if died {
            "*** You have died ***"
        } else {
            "*** You have won ***"
        };
        let mut fallen: Vec<String> = Vec::new();
        for name in &self.tally.fallen {
            if !fallen.contains(name) {
                fallen.push(name.clone());
            }
        }
        let foes = match fallen.len() {
            0 => "no foe was seen to fall".to_owned(),
            _ => format!(
                "you saw {} fall",
                prose::and_list(
                    &fallen
                        .iter()
                        .map(|n| prose::definite(n))
                        .collect::<Vec<_>>()
                )
            ),
        };
        let places = self.places.visited.len();
        let where_ = format!(
            " in {}",
            crate::narrative::place_title(state).unwrap_or_else(|| {
                let place = place::survey(state);
                crate::narrative::place_noun(state, link.palette(), &place, &self.places, true)
            })
        );
        let how = if died {
            format!("You fell{where_}")
        } else {
            "You made it out".to_owned()
        };
        format!(
            "{banner}\n{how}. This session recorded {} {} attempted, {} {} visited, and {} {} of observed HP loss; {foes}.\nType history to look back over the run, or quit. To play again, start a new game.",
            self.tally.goals,
            if self.tally.goals == 1 { "goal" } else { "goals" },
            prose::number(places as u64),
            if places == 1 { "place" } else { "places" },
            self.tally.wounds,
            if self.tally.wounds == 1 { "point" } else { "points" },
        )
    }

    /// Narrate updates that arrived between turns.
    pub fn between_turns(
        &mut self,
        link: &impl Link,
        before: &StateView,
        message: &ServerMessage,
    ) -> Vec<Beat> {
        let mut reader = turn::Reader {
            chronicler: &mut self.chronicler,
            beats: Vec::new(),
            pages: Vec::new(),
            resynced: false,
            waiting: None,
        };
        reader.record(before, link, message);
        let beats = reader.beats;
        if beats.contains(&Beat::Resync) {
            self.reset();
        }
        beats
    }

    /// The passage for beats gathered between turns.
    pub fn passage(&mut self, link: &impl Link, beats: Vec<Beat>) -> String {
        let mut record = Record::default();
        record.entries.push(Entry::Beats(beats));
        self.after_beats(link, &record);
        let start = self.last_place.clone();
        self.finish(link, &mut record, start.as_deref());
        narrate::compose(&record)
    }
}

enum Flow {
    Continue,
    Stop,
    Quit,
}

/// The scene described in full, as the game opens or after a snapshot, with
/// no places remembered.
pub fn describe(link: &impl Link) -> String {
    crate::adventure::describe_with(link.client().state(), link.palette())
}

/// "attack the scout until it dies" fights it.
fn persist(command: ParsedCommand) -> ParsedCommand {
    match command {
        ParsedCommand::Transitive {
            verb: lexicon::Verb::Attack,
            direct,
        } => ParsedCommand::Transitive {
            verb: lexicon::Verb::Fight,
            direct,
        },
        ParsedCommand::Intransitive {
            verb: lexicon::Verb::Attack,
        } => ParsedCommand::Intransitive {
            verb: lexicon::Verb::Fight,
        },
        other => other,
    }
}

/// Whether a command does something in the game, so "again" can repeat it.
fn acts(command: &ParsedCommand) -> bool {
    matches!(
        command,
        ParsedCommand::Transitive { .. }
            | ParsedCommand::Ditransitive { .. }
            | ParsedCommand::MultiTransitive { .. }
            | ParsedCommand::Ability { .. }
            | ParsedCommand::Directional { .. }
            | ParsedCommand::Step { .. }
    ) || matches!(command, ParsedCommand::Intransitive { verb } if needs_figure(*verb) || *verb == lexicon::Verb::Wait)
}
