//! The interactive fiction engine: what the player types becomes game
//! actions, and everything that happens before the next prompt becomes one
//! passage. See docs/if-engine.md.
pub mod chronicle;
pub mod narrate;
pub mod place;
pub mod prose;
pub mod resolve;
pub mod scene;
pub mod turn;
pub mod verbs;

use std::collections::VecDeque;

use tor_protocol::*;

use crate::{
    parser::{self, lexicon, ParsedCommand, Token},
    Input,
};
use chronicle::{Beat, Chronicler};
use narrate::{Entry, Record};
use resolve::Referents;
use scene::Scene;
use turn::{Error, Link};
use verbs::{Interpretation, Question};

/// What a line of input led to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The passage to show before the next prompt.
    Passage(String),
    /// The player asked to leave; the passage comes first.
    Quit(String),
}

/// A question waiting for its answer, and the rest of the chain it
/// interrupted.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Pending {
    question: Question,
    revision: u64,
    rest: Vec<String>,
}

/// What the engine keeps between turns. None of it is game state: it's lost
/// on restart, and a snapshot clears it.
#[derive(Default)]
pub struct Engine {
    pub referents: Referents,
    pending: Option<Pending>,
    last_line: Option<String>,
    chronicler: Chronicler,
}

/// The sentences of a line, each with its own text as typed, so names and
/// notes keep their capitals.
fn sentences(line: &str) -> Vec<String> {
    let tokens = parser::tokenize(line);
    if parser::split_sentences(&tokens).len() <= 1 {
        return vec![line.trim().to_owned()];
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
}

/// Whether a line reads as a new command rather than an answer.
fn commands(line: &str) -> bool {
    let tokens = parser::tokenize(line);
    let first = tokens.first().and_then(Token::as_word).unwrap_or("");
    lexicon::parse_verb(first).is_some()
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
        if matches!(line.to_lowercase().as_str(), "again" | "g") {
            match &self.last_line {
                Some(last) => line = last.clone(),
                None => {
                    record.say("There's nothing to repeat.");
                    return Ok(Outcome::Passage(narrate::compose(&record)));
                }
            }
        }
        let mut chain: VecDeque<Result<Interpretation, String>> = VecDeque::new();
        if let Some(pending) = self.pending.take() {
            let current = link.client().state().revision == pending.revision;
            match answer(&pending.question, &line) {
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
                None if current && !commands(&line) => {
                    let labels: Vec<String> = pending
                        .question
                        .choices
                        .iter()
                        .map(|c| c.label.clone())
                        .collect();
                    record.say(format!(
                        "Please choose {}, or type a new command.",
                        prose::or_list(&labels)
                    ));
                    self.pending = Some(pending);
                    return Ok(Outcome::Passage(narrate::compose(&record)));
                }
                _ => {}
            }
        }
        if chain.is_empty() && line.parse::<u64>().is_ok() {
            record.say("There's no question to answer.");
            return Ok(Outcome::Passage(narrate::compose(&record)));
        }
        if chain.is_empty() {
            self.last_line = Some(line.clone());
            chain.extend(sentences(&line).into_iter().map(Err));
        }
        // `Err` holds a sentence still to be read; `Ok` one already understood.
        while let Some(next) = chain.pop_front() {
            let interpretation = match next {
                Ok(interpretation) => interpretation,
                Err(text) => {
                    let tokens = parser::tokenize(&text);
                    match parser::match_sentence_with_raw(&tokens, Some(&text)) {
                        Ok(ParsedCommand::Again) => {
                            record.say("Say again on its own to repeat a command.");
                            break;
                        }
                        Ok(command) => {
                            let scene = Scene::new(link.client().state(), link.palette());
                            verbs::interpret(&command, &scene, &mut self.referents)
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
                    self.finish(link, &mut record);
                    return Ok(Outcome::Quit(narrate::compose(&record)));
                }
            }
        }
        self.finish(link, &mut record);
        Ok(Outcome::Passage(narrate::compose(&record)))
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
            Interpretation::Look => {
                record.entries.push(Entry::Description(describe(link)));
                Flow::Continue
            }
            Interpretation::Ask(question) => {
                record.say(&question.prompt);
                self.pending = Some(Pending {
                    question,
                    revision: link.client().state().revision,
                    rest: chain.drain(..).filter_map(|c| c.err()).collect(),
                });
                Flow::Stop
            }
            Interpretation::Quit => Flow::Quit,
            Interpretation::Goals(goals) => {
                if let Err(why) = turn::may_act(link) {
                    record.say(why);
                    return Ok(Flow::Stop);
                }
                for goal in goals {
                    let done = turn::run_goal(link, &mut self.chronicler, record, goal).await?;
                    self.after_beats(link, record);
                    if !done {
                        return Ok(Flow::Stop);
                    }
                }
                Flow::Continue
            }
            Interpretation::Tool(input) => self.tool(link, record, input).await?,
        })
    }

    async fn tool(
        &mut self,
        link: &mut impl Link,
        record: &mut Record,
        input: Input,
    ) -> Result<Flow, Error> {
        let request = match input {
            Input::Look => {
                record.entries.push(Entry::Description(describe(link)));
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
            Input::Request(request) => request,
            Input::Command(command) => {
                if matches!(command, Command::Wizard { .. }) && link.role() != AccessRole::Wizard {
                    record.say("Wizard authority is required.");
                    return Ok(Flow::Stop);
                }
                Request::Command {
                    branch: link.client().branch().clone(),
                    command,
                }
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
    /// somewhere new.
    fn finish(&mut self, link: &impl Link, record: &mut Record) {
        let resynced = record.entries.iter().any(|e| match e {
            Entry::Episode(e) => e.beats.contains(&Beat::Resync),
            Entry::Beats(beats) => beats.contains(&Beat::Resync),
            _ => false,
        });
        let arrived = record.entries.iter().any(|e| {
            matches!(e, Entry::Episode(e) if matches!(e.goal, verbs::Goal::Go { .. }) && e.end == narrate::End::Done)
        });
        if resynced {
            record.entries.push(Entry::Description(describe(link)));
        } else if arrived {
            record
                .entries
                .push(Entry::Description(crate::adventure::describe_place_with(
                    link.client().state(),
                    link.palette(),
                )));
        }
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
        self.finish(link, &mut record);
        narrate::compose(&record)
    }
}

enum Flow {
    Continue,
    Stop,
    Quit,
}

/// The scene described, as `look` shows it.
pub fn describe(link: &impl Link) -> String {
    crate::adventure::describe_with(link.client().state(), link.palette())
}
