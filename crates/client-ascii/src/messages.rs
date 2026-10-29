//! The message area: everything that happened since the player last acted,
//! composed from narrated lines, and a bounded log of earlier turns.
//!
//! Messages stay until the player's next command, so nothing is shown for
//! only a frame. Routine lines the map or status line already show (moving,
//! waiting, readiness, hit points) are left out.
use std::collections::{BTreeMap, VecDeque};
use tor_client_common::narration::{BasicAttack, Line, Topic};
use tor_protocol::{AbilityOutcome, Event, Observation};

/// Characters per message row.
pub const WIDTH: usize = 72;
/// Message rows shown at once.
pub const ROWS: usize = 3;
/// Earlier turns kept for the message log.
pub const LOG_LIMIT: usize = 400;
/// An actor out of sight this long is announced again when it returns.
pub const SIGHTING_TICKS: u64 = 1000;
const MORE: &str = "--More--";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Messages {
    /// Sentences since the player's last command, with repeat counts.
    turn: Vec<(String, usize)>,
    /// Rows of this turn already paged past.
    seen: usize,
    log: VecDeque<String>,
    /// When each actor was last in sight.
    sightings: BTreeMap<tor_protocol::ActorTarget, u64>,
}

impl Messages {
    /// The player acted: this turn's messages move to the log.
    pub fn begin_turn(&mut self) {
        let text = self.text();
        if !text.is_empty() {
            self.log.push_back(text);
            while self.log.len() > LOG_LIMIT {
                self.log.pop_front();
            }
        }
        self.turn.clear();
        self.seen = 0;
    }

    /// Forget sightings, as after a timeline change.
    pub fn reset_sightings(&mut self) {
        self.sightings.clear();
    }

    /// Add one sentence; an immediate repeat is counted rather than repeated.
    pub fn push(&mut self, sentence: impl Into<String>) {
        let sentence = sentence.into();
        if sentence.trim().is_empty() {
            return;
        }
        match self.turn.last_mut() {
            Some((last, count)) if *last == sentence => *count += 1,
            _ => self.turn.push((sentence, 1)),
        }
    }

    /// Compose ordered event-time lines without consulting a stale visual view.
    pub fn absorb_outcome(&mut self, lines: &[Line], tick: u64) {
        for line in lines {
            match &line.basic_attack {
                Some(BasicAttack::Preparing) => continue,
                Some(BasicAttack::Resolved {
                    target,
                    outcome: AbilityOutcome::Applied,
                }) => {
                    self.push(format!("You hit {target}."));
                    continue;
                }
                Some(BasicAttack::Resolved {
                    target,
                    outcome: AbilityOutcome::Miss,
                }) => {
                    self.push(format!("You miss {target}."));
                    continue;
                }
                _ => {}
            }
            match &line.topic {
                // Movement is shown by the demanded map and underfoot narration.
                // A routine move must not itself stop a directional run.
                Topic::Own(Event::Moved { .. } | Event::Waited)
                | Topic::LostSight(_)
                | Topic::Pacing => {}
                Topic::Finished => self.finish(&line.text),
                Topic::Noticed(actor) => {
                    let returning = self
                        .sightings
                        .get(actor)
                        .is_some_and(|seen| tick.saturating_sub(*seen) < SIGHTING_TICKS);
                    if !returning {
                        self.push(&line.text);
                    }
                    self.sightings.insert(*actor, tick);
                }
                _ => self.push(&line.text),
            }
        }
    }

    /// Compose one observation's narrated lines into this turn.
    pub fn absorb(&mut self, lines: &[Line], view: &Observation) {
        for line in lines {
            match &line.basic_attack {
                Some(BasicAttack::Preparing) => continue,
                Some(BasicAttack::Resolved {
                    target,
                    outcome: AbilityOutcome::Applied,
                }) => {
                    self.push(format!("You hit {target}."));
                    continue;
                }
                Some(BasicAttack::Resolved {
                    target,
                    outcome: AbilityOutcome::Miss,
                }) => {
                    self.push(format!("You miss {target}."));
                    continue;
                }
                // An unchanged primary does not reveal whether other effects
                // landed. Preserve its neutral narration and all special work.
                Some(BasicAttack::Resolved {
                    outcome: AbilityOutcome::Unaffected,
                    ..
                })
                | None => {}
            }
            match &line.topic {
                Topic::Own(Event::Moved { .. }) => self.here(view),
                Topic::Own(Event::Waited) => {}
                Topic::Own(_) | Topic::Door | Topic::Discovery | Topic::Motion => {
                    self.push(&line.text)
                }
                Topic::Finished => self.finish(&line.text),
                Topic::Noticed(actor) => {
                    let returning = self
                        .sightings
                        .get(actor)
                        .is_some_and(|seen| view.tick.saturating_sub(*seen) < SIGHTING_TICKS);
                    if !returning {
                        self.push(&line.text);
                    }
                }
                Topic::LostSight(_) | Topic::Pacing => {}
                Topic::Combat(_) | Topic::PublicPlan => self.push(&line.text),
                Topic::Status => {
                    if let Some(combat) = view.combat.as_ref() {
                        if combat.victory {
                            self.push(if combat.terminal {
                                "Victory! This run has ended."
                            } else {
                                "Victory! You may continue exploring."
                            });
                        } else if combat.dead {
                            self.push("This run has ended.");
                        }
                    }
                }
            }
        }
        for actor in &view.visible_actors {
            if actor.id != view.self_target {
                self.sightings.insert(actor.id, view.tick);
            }
        }
    }

    /// "You begin to drink the X." then "You finish drinking the X." reads as
    /// one action once it's done. When the item was learned about on the
    /// way (it has a new name), say what it turned out to be.
    fn finish(&mut self, finished: &str) {
        let parts = |text: &str, prefix: &str| -> Option<(String, String)> {
            let rest = text.strip_prefix(prefix)?.strip_suffix('.')?;
            let (verb, name) = rest.split_once(" the ")?;
            Some((verb.to_owned(), name.to_owned()))
        };
        let Some((verbing, name)) = parts(finished, "You finish ") else {
            return self.push(finished);
        };
        let mut learned = None;
        self.turn
            .retain(|(sentence, _)| match parts(sentence, "You begin to ") {
                Some((verb, began)) if verbing.starts_with(verb.trim_end_matches('e')) => {
                    if began != name {
                        learned = Some(began);
                    }
                    false
                }
                _ => true,
            });
        self.push(finished);
        if let Some(learned) = learned {
            self.push(format!("It was {}.", crate::item_phrase(&learned, 1)));
        }
    }

    /// This turn's sentences as one paragraph.
    /// What's underfoot after a move, as NetHack reports it.
    pub fn here(&mut self, view: &Observation) {
        let items: Vec<_> = view
            .ground_items
            .iter()
            .filter(|item| item.position == view.position)
            .collect();
        match items.as_slice() {
            [] => {}
            [item] => self.push(format!(
                "You see here {}.",
                crate::item_phrase(
                    tor_client_common::items::ground_name(item),
                    item.item.quantity
                )
            )),
            _ => self.push("There are several objects here."),
        }
        if let Some(cell) = view
            .visible_cells
            .iter()
            .find(|cell| cell.position == view.position)
        {
            if let Some(presentation) = &cell.object_presentation {
                self.push(format!(
                    "You see here {}.",
                    crate::item_phrase(tor_client_common::items::object_name(presentation), 1)
                ));
            } else if cell.stairs_down {
                self.push("There is a staircase down here.");
            } else if cell.stairs_up {
                self.push("There is a staircase up here.");
            }
        }
    }

    pub fn text(&self) -> String {
        self.turn
            .iter()
            .map(|(sentence, count)| {
                if *count > 1 {
                    format!("{sentence} (x{count})")
                } else {
                    sentence.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("  ")
    }

    fn rows(&self) -> Vec<String> {
        // Leave room for the --More-- marker on any row.
        word_wrap(&self.text(), WIDTH - MORE.len() - 1)
    }

    /// Whether rows remain beyond those shown.
    pub fn more(&self) -> bool {
        self.rows().len() > self.seen + ROWS
    }

    /// Show the next rows.
    pub fn page(&mut self) {
        if self.more() {
            self.seen += ROWS;
        }
    }

    /// Skip to the last rows.
    pub fn skip(&mut self) {
        self.seen = self.rows().len().saturating_sub(ROWS);
    }

    /// The rows to draw. A paging viewer sees the oldest unseen rows with a
    /// --More-- marker; one that can't page (a spectator) sees the newest.
    pub fn shown(&self, paging: bool) -> Vec<String> {
        let rows = self.rows();
        if !paging {
            return rows[rows.len().saturating_sub(ROWS)..].to_vec();
        }
        let mut shown: Vec<_> = rows.iter().skip(self.seen).take(ROWS).cloned().collect();
        if rows.len() > self.seen + ROWS {
            if let Some(last) = shown.last_mut() {
                last.push(' ');
                last.push_str(MORE);
            }
        }
        shown
    }

    /// Earlier turns, oldest first, then this turn.
    pub fn log(&self) -> impl Iterator<Item = &str> {
        self.log.iter().map(String::as_str)
    }

    /// Log and current turn as wrapped rows for the message history screen.
    pub fn history_rows(&self, width: usize) -> Vec<String> {
        let current = self.text();
        self.log
            .iter()
            .map(String::as_str)
            .chain((!current.is_empty()).then_some(current.as_str()))
            .flat_map(|turn| word_wrap(turn, width))
            .collect()
    }
}

/// Wrap at spaces; a word longer than a row is split.
pub fn word_wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut rows = Vec::new();
    let mut row = String::new();
    for word in text.split(' ') {
        let word: String = word
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let needed = if row.is_empty() {
            word.chars().count()
        } else {
            row.chars().count() + 1 + word.chars().count()
        };
        if needed > width && !row.is_empty() {
            rows.push(std::mem::take(&mut row));
        }
        if !row.is_empty() {
            row.push(' ');
        }
        row.push_str(&word);
        while row.chars().count() > width {
            let rest: String = row.chars().skip(width).collect();
            row = row.chars().take(width).collect();
            rows.push(std::mem::replace(&mut row, rest));
        }
    }
    if !row.trim().is_empty() {
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod outcome_tests {
    use super::*;

    #[test]
    fn routine_move_outcome_does_not_stop_a_directional_run() {
        let mut messages = Messages::default();
        messages.absorb_outcome(
            &[Line {
                topic: Topic::Own(Event::Moved {
                    direction: tor_protocol::Direction::East,
                }),
                text: "You move east.".into(),
                basic_attack: None,
            }],
            100,
        );
        assert!(messages.text().is_empty());
        messages.push("You see here a stone tablet.");
        assert_eq!(messages.text(), "You see here a stone tablet.");
    }

    #[test]
    fn outcome_messages_use_event_time_text_without_a_visual_view() {
        let mut messages = Messages::default();
        messages.absorb_outcome(
            &[
                Line {
                    topic: Topic::Own(Event::Moved {
                        direction: tor_protocol::Direction::North,
                    }),
                    text: "You move north.".into(),
                    basic_attack: None,
                },
                Line {
                    topic: Topic::Status,
                    text: "This run has ended.".into(),
                    basic_attack: None,
                },
            ],
            7,
        );
        assert!(!messages.text().contains("You move north."));
        assert!(messages.text().contains("This run has ended."));
    }
}
