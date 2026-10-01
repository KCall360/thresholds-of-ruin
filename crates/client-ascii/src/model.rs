use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, BTreeSet};
use tor_client_common::ClientState;
use tor_client_hack::{
    adjacent, bump, creature_at, decide_autopickup, door_at, door_command, feet_items, fight,
    gone_sentence, look_at, offset, other_actors, pickup_sentence, repeat_intent,
    repeat_interrupted, take_quantity, to_action, AutoPickup, AutoQuery, Intent, IntentQueue,
    InventoryLetters, Resolved, BAD_QUANTITY, BUFFER_FULL, CANCEL_TRAVEL_FIRST, HELP_LINE,
    ILLEGAL_TARGET, NOTHING_HERE, RUN_CAP, UNAVAILABLE,
};
use tor_protocol::*;

pub use tor_client_hack::{column_glyph, BumpAttacks, Click, SessionConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Places,
    Travel,
    Up,
    Down,
    Left,
    Right,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Ascend,
    Descend,
    Wait,
    Space,
    Pickup,
    Drop,
    OpenDoor,
    CloseDoor,
    Control,
    Release,
    Note,
    Enter,
    Escape,
    Backspace,
    Tab,
    History,
    OlderHistory,
    RecentHistory,
    Scrollback,
    Fight,
    Quit,
    Go,
    Suppress,
    SuppressRun,
    DropMany,
    Inventory,
    Look,
    WhatIs,
    Describe,
    Autopickup,
    Extended,
    Help,
    KeyHelp,
    Key0,
    Key1,
    Key2,
    Key3,
    Key4,
    Key5,
    Key6,
    Key7,
    Key8,
    Key9,
    Numpad0,
    Numpad1,
    Numpad2,
    Numpad3,
    Numpad4,
    Numpad5,
    Numpad6,
    Numpad7,
    Numpad8,
    Numpad9,
    RunNorth,
    RunSouth,
    RunEast,
    RunWest,
    RunNorthEast,
    RunSouthEast,
    RunSouthWest,
    RunNorthWest,
    Letter(char),
}

impl Key {
    pub fn name(self) -> String {
        match self {
            Key::Places => "places",
            Key::Travel => "travel",
            Key::Up => "up",
            Key::Down => "down",
            Key::Left => "left",
            Key::Right => "right",
            Key::NorthEast => "north_east",
            Key::SouthEast => "south_east",
            Key::SouthWest => "south_west",
            Key::NorthWest => "north_west",
            Key::Ascend => "ascend",
            Key::Descend => "descend",
            Key::Wait => "wait",
            Key::Space => "space",
            Key::Pickup => "pickup",
            Key::Drop => "drop",
            Key::OpenDoor => "open_door",
            Key::CloseDoor => "close_door",
            Key::Control => "control",
            Key::Release => "release",
            Key::Note => "note",
            Key::Enter => "enter",
            Key::Escape => "escape",
            Key::Backspace => "backspace",
            Key::Tab => "tab",
            Key::History => "history",
            Key::OlderHistory => "older_history",
            Key::RecentHistory => "recent_history",
            Key::Scrollback => "scrollback",
            Key::Fight => "fight",
            Key::Quit => "f9",
            Key::Go => "go",
            Key::Suppress => "suppress",
            Key::SuppressRun => "suppress_run",
            Key::DropMany => "drop_many",
            Key::Inventory => "inventory",
            Key::Look => "look",
            Key::WhatIs => "what_is",
            Key::Describe => "describe",
            Key::Autopickup => "autopickup",
            Key::Extended => "extended",
            Key::Help => "help",
            Key::KeyHelp => "key_help",
            Key::Key0 => "key0",
            Key::Key1 => "key1",
            Key::Key2 => "key2",
            Key::Key3 => "key3",
            Key::Key4 => "key4",
            Key::Key5 => "key5",
            Key::Key6 => "key6",
            Key::Key7 => "key7",
            Key::Key8 => "key8",
            Key::Key9 => "key9",
            Key::Numpad0 => "numpad0",
            Key::Numpad1 => "numpad1",
            Key::Numpad2 => "numpad2",
            Key::Numpad3 => "numpad3",
            Key::Numpad4 => "numpad4",
            Key::Numpad5 => "numpad5",
            Key::Numpad6 => "numpad6",
            Key::Numpad7 => "numpad7",
            Key::Numpad8 => "numpad8",
            Key::Numpad9 => "numpad9",
            Key::RunNorth => "run_north",
            Key::RunSouth => "run_south",
            Key::RunEast => "run_east",
            Key::RunWest => "run_west",
            Key::RunNorthEast => "run_north_east",
            Key::RunSouthEast => "run_south_east",
            Key::RunSouthWest => "run_south_west",
            Key::RunNorthWest => "run_north_west",
            Key::Letter(ch) => return ch.to_string(),
        }
        .into()
    }

    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "places" => Key::Places,
            "travel" => Key::Travel,
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "north_east" => Key::NorthEast,
            "south_east" => Key::SouthEast,
            "south_west" => Key::SouthWest,
            "north_west" => Key::NorthWest,
            "ascend" => Key::Ascend,
            "descend" => Key::Descend,
            "wait" => Key::Wait,
            "space" => Key::Space,
            "pickup" => Key::Pickup,
            "drop" => Key::Drop,
            "open_door" => Key::OpenDoor,
            "close_door" => Key::CloseDoor,
            "control" => Key::Control,
            "release" => Key::Release,
            "note" => Key::Note,
            "enter" => Key::Enter,
            "escape" => Key::Escape,
            "backspace" => Key::Backspace,
            "tab" => Key::Tab,
            "history" => Key::History,
            "older_history" => Key::OlderHistory,
            "recent_history" => Key::RecentHistory,
            "scrollback" => Key::Scrollback,
            "fight" => Key::Fight,
            "f9" => Key::Quit,
            "go" => Key::Go,
            "suppress" => Key::Suppress,
            "suppress_run" => Key::SuppressRun,
            "drop_many" => Key::DropMany,
            "inventory" => Key::Inventory,
            "look" => Key::Look,
            "what_is" => Key::WhatIs,
            "describe" => Key::Describe,
            "autopickup" => Key::Autopickup,
            "extended" => Key::Extended,
            "help" => Key::Help,
            "key_help" => Key::KeyHelp,
            "key0" => Key::Key0,
            "key1" => Key::Key1,
            "key2" => Key::Key2,
            "key3" => Key::Key3,
            "key4" => Key::Key4,
            "key5" => Key::Key5,
            "key6" => Key::Key6,
            "key7" => Key::Key7,
            "key8" => Key::Key8,
            "key9" => Key::Key9,
            "numpad0" => Key::Numpad0,
            "numpad1" => Key::Numpad1,
            "numpad2" => Key::Numpad2,
            "numpad3" => Key::Numpad3,
            "numpad4" => Key::Numpad4,
            "numpad5" => Key::Numpad5,
            "numpad6" => Key::Numpad6,
            "numpad7" => Key::Numpad7,
            "numpad8" => Key::Numpad8,
            "numpad9" => Key::Numpad9,
            "run_north" => Key::RunNorth,
            "run_south" => Key::RunSouth,
            "run_east" => Key::RunEast,
            "run_west" => Key::RunWest,
            "run_north_east" => Key::RunNorthEast,
            "run_south_east" => Key::RunSouthEast,
            "run_south_west" => Key::RunSouthWest,
            "run_north_west" => Key::RunNorthWest,
            other => {
                let mut chars = other.chars();
                let ch = chars.next()?;
                if chars.next().is_none() && ch.is_ascii_alphabetic() {
                    return Some(Key::Letter(ch));
                }
                return None;
            }
        })
    }
}

impl Serialize for Key {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.name())
    }
}

impl<'de> Deserialize<'de> for Key {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        Key::parse(&name).ok_or_else(|| serde::de::Error::unknown_variant(&name, &[]))
    }
}

/// The native keyboard and opt-in process-test driver share this input boundary.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Input {
    Click { x: usize, y: usize },
    Key { key: Key },
    Text { text: String },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
    /// Escape queued `CancelTravel` while a request was already in flight.
    Enqueued,
    Request(Request),
}

#[derive(Clone, Debug)]
enum Prompt {
    None,
    Fight,
    Open,
    Close,
    Step { pickup: bool, suppress_attack: bool },
    Run { pickup: bool },
    Drop { count: Option<u32> },
    DropMany,
    Extended(String),
    WhatIs(String),
    Look,
    Inventory,
    Adjust(Option<char>),
    KeyHelp,
}

#[derive(Clone, Debug)]
enum FlightKind {
    OneShot(Intent),
    /// `remaining` lives on the queued intent. The flight only remembers which step it was.
    Repeat {
        direction: Direction,
    },
    RepeatWait,
}

struct Flight {
    kind: FlightKind,
}

#[derive(Clone, Debug)]
struct Composed {
    via_travel: bool,
    name: String,
    item: u64,
}

#[derive(Clone, Debug)]
struct Journey {
    actors: BTreeSet<ActorId>,
    branch: BranchId,
    via_travel: bool,
    pickup: bool,
}

pub struct NoteDraft {
    pub text: String,
    pub audience: Audience,
    revision: u64,
}

pub struct App {
    pub config: SessionConfig,
    pub attack_targets: Vec<ActorView>,
    viewport: Viewport,
    pub places_open: bool,
    pub place_selected: usize,
    pub place_name: Option<String>,
    pub travel_cursor: Option<Position>,
    pub role: AccessRole,
    pub state: Option<ClientState>,
    pub connected: bool,
    pub busy: bool,
    pub status: String,
    pub note: Option<NoteDraft>,
    pub pickup: Vec<ItemView>,
    pub dropping: bool,
    pub quantity: String,
    pub door_direction: Option<bool>,
    pub selected: usize,
    pub history_page: Option<HistoryPage>,
    pub history_scroll: usize,
    log: tor_client_hack::MessageLog,
    letters: InventoryLetters,
    queue: IntentQueue,
    prompt: Prompt,
    flight: Option<Flight>,
    composed: Option<Composed>,
    look_cursor: Option<Position>,
    pub(crate) row_on: Vec<bool>,
    pub(crate) row_count: Vec<Option<u64>>,
    journey: Option<Journey>,
    repeat_seen: Option<BTreeSet<ActorId>>,
    pickup_armed: bool,
    typed_count: Option<u32>,
}

enum LogAppend {
    Narration,
    Line(String),
    Nothing,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            config: SessionConfig::default(),
            attack_targets: Vec::new(),
            viewport: Viewport::default(),
            places_open: false,
            place_selected: 0,
            place_name: None,
            travel_cursor: None,
            role: AccessRole::Spectator,
            state: None,
            connected: false,
            busy: true,
            status: "Connecting to the local server...".into(),
            note: None,
            pickup: vec![],
            dropping: false,
            quantity: String::new(),
            door_direction: None,
            selected: 0,
            history_page: None,
            history_scroll: 0,
            log: tor_client_hack::MessageLog::default(),
            letters: InventoryLetters::default(),
            queue: IntentQueue::new(),
            prompt: Prompt::None,
            flight: None,
            composed: None,
            look_cursor: None,
            row_on: Vec::new(),
            row_count: Vec::new(),
            journey: None,
            repeat_seen: None,
            pickup_armed: false,
            typed_count: None,
        }
    }

    pub fn accepts_text(&self) -> bool {
        self.note.is_some()
            || self.place_name.is_some()
            || matches!(self.prompt, Prompt::Extended(_) | Prompt::WhatIs(_))
    }

    /// `_` starts travel only when no editor, menu, or prompt is using the keyboard.
    pub fn accepts_travel_chord(&self) -> bool {
        !self.accepts_text()
            && !self.places_open
            && self.pickup.is_empty()
            && self.travel_cursor.is_none()
            && self.door_direction.is_none()
            && self.history_page.is_none()
            && matches!(self.prompt, Prompt::None)
    }

    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    pub fn inventory_letters(&self) -> BTreeMap<u64, char> {
        self.letters.pairs().collect()
    }

    pub fn look_cursor(&self) -> Option<Position> {
        self.look_cursor
    }

    pub fn inventory_open(&self) -> bool {
        matches!(self.prompt, Prompt::Inventory)
    }

    /// `try_send` returned `Full`: the intent stays queued and the session stays up.
    pub fn unsend(&mut self) {
        self.busy = false;
        if let Some(flight) = self.flight.take() {
            if let FlightKind::OneShot(intent) = flight.kind {
                self.queue.push_front(intent);
            }
        }
    }

    pub fn set_state(&mut self, state: ClientState) {
        let old = self
            .state
            .as_ref()
            .map(|s| (s.branch().clone(), s.state().revision));
        let reset = self
            .state
            .as_ref()
            .is_none_or(|current| current.branch() != state.branch());
        self.state = Some(state);
        if reset {
            self.viewport.clear();
        }
        self.state_changed(old);
        self.adopt_letters(reset);
        self.follow();
    }

    /// Apply every disclosed boundary, even when several updates share a frame.
    pub fn update(&mut self, update: StreamUpdate) -> Result<(), tor_client_common::StreamError> {
        let take_or_drop = matches!(
            &update.body,
            UpdateBody::Observation { event, .. } | UpdateBody::ObservationDelta { event, .. }
                if event.as_ref().is_some_and(|entry| {
                    matches!(
                        &entry.content,
                        HistoryContent::Action {
                            event: Event::Taken { .. } | Event::Dropped { .. },
                            ..
                        }
                    )
                })
        );
        let taken = match &update.body {
            UpdateBody::Observation { event, .. } | UpdateBody::ObservationDelta { event, .. } => {
                event.as_ref().and_then(|entry| match &entry.content {
                    HistoryContent::Action {
                        event: Event::Taken { item, result, .. },
                        ..
                    } => Some((*item, *result)),
                    _ => None,
                })
            }
            _ => None,
        };
        let travel_phase = match &update.body {
            UpdateBody::Travel { status, .. } => Some(status.phase),
            _ => None,
        };
        let observation_update = matches!(
            &update.body,
            UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }
        );
        let appended = match &update.body {
            UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. } => {
                LogAppend::Narration
            }
            UpdateBody::Annotation { entry } => match &entry.content {
                HistoryContent::Annotation { text, .. } => LogAppend::Line(format!("Note: {text}")),
                _ => LogAppend::Nothing,
            },
            _ => LogAppend::Nothing,
        };
        let (before_branch, before, old) = {
            let state = self
                .state
                .as_mut()
                .ok_or(tor_client_common::StreamError::InconsistentState)?;
            let old = Some((state.branch().clone(), state.state().revision));
            let before_branch = state.branch().clone();
            let before = state.state().observation.clone();
            state.apply(update)?;
            (before_branch, before, old)
        };
        self.shift_chart(&before_branch, &before);
        self.state_changed(old);
        let branch_changed = self
            .state
            .as_ref()
            .is_some_and(|state| state.branch() != &before_branch);
        let suppress = if branch_changed {
            self.adopt_letters(true);
            false
        } else {
            self.sync_letters(take_or_drop);
            self.resolve_flight(observation_update, travel_phase, taken)
        };
        self.follow();
        if !suppress {
            self.append_logged(appended);
        }
        Ok(())
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: Snapshot,
    ) -> Result<(), tor_client_common::StreamError> {
        let old = self
            .state
            .as_ref()
            .map(|s| (s.branch().clone(), s.state().revision));
        if self.state.is_none() {
            self.state = Some(ClientState::from_snapshot(snapshot)?);
            self.viewport.clear();
        } else {
            let before_branch = self.state.as_ref().unwrap().branch().clone();
            let before = self.state.as_ref().unwrap().state().observation.clone();
            self.state.as_mut().unwrap().replace_snapshot(snapshot)?;
            self.shift_chart(&before_branch, &before);
        }
        self.log.clear_for_snapshot();
        self.state_changed(old.clone());
        let reset = match &old {
            None => true,
            Some((branch, _)) => self
                .state
                .as_ref()
                .is_some_and(|state| state.branch() != branch),
        };
        self.adopt_letters(reset);
        self.follow();
        Ok(())
    }

    /// `None` from `chart_shift` leaves the origin where it is: the old chart was cleared.
    fn shift_chart(&mut self, before_branch: &BranchId, before: &Observation) {
        let (same_branch, after) = {
            let Some(state) = &self.state else {
                return;
            };
            (
                state.branch() == before_branch,
                state.state().observation.clone(),
            )
        };
        if !same_branch {
            self.viewport.clear();
            return;
        }
        let Some((dx, dy, _)) = tor_client_hack::chart_shift(before, &after) else {
            return;
        };
        if self.viewport.x.placed && self.viewport.y.placed {
            if let Some((x, y)) = tor_client_hack::shift_origin(
                self.viewport.x.origin,
                self.viewport.y.origin,
                dx,
                dy,
            ) {
                self.viewport.x.origin = x;
                self.viewport.y.origin = y;
            }
        }
    }

    pub(crate) fn map_origin(&self) -> Option<(i32, i32)> {
        (self.viewport.x.placed && self.viewport.y.placed)
            .then_some((self.viewport.x.origin, self.viewport.y.origin))
    }

    fn follow(&mut self) {
        let cursor = self.look_cursor.or(self.travel_cursor);
        let spans = self.state.as_ref().map(|state| {
            let observation = &state.state().observation;
            let chart: Vec<_> = state.map_memory().collect();
            let columns = tor_client_hack::map_columns(observation, &chart);
            let focus = cursor
                .map(|cursor| (cursor.x, cursor.y))
                .unwrap_or((observation.position.x, observation.position.y));
            (
                span(columns.iter().map(|column| column.x), focus.0),
                span(columns.iter().map(|column| column.y), focus.1),
                focus,
            )
        });
        let Some(((min_x, max_x), (min_y, max_y), focus)) = spans else {
            return;
        };
        follow_axis(
            &mut self.viewport.x,
            min_x,
            max_x,
            focus.0,
            AxisWindow {
                length: 75,
                center: 37,
                margin: 4,
                margin_end: 70,
            },
        );
        follow_axis(
            &mut self.viewport.y,
            min_y,
            max_y,
            focus.1,
            AxisWindow {
                length: 45,
                center: 22,
                margin: 4,
                margin_end: 40,
            },
        );
    }

    fn state_changed(&mut self, old: Option<(BranchId, u64)>) {
        let state = self.state.as_ref().expect("validated state");
        if old
            .as_ref()
            .is_some_and(|(branch, _)| branch != state.branch())
        {
            self.log.clear_for_branch_change();
            self.travel_cursor = None;
            self.note = None;
            self.pickup.clear();
            self.attack_targets.clear();
            self.door_direction = None;
            self.places_open = false;
            self.place_name = None;
            self.history_page = None;
            self.history_scroll = 0;
            self.prompt = Prompt::None;
            self.look_cursor = None;
            self.row_on.clear();
            self.row_count.clear();
            self.typed_count = None;
            self.pickup_armed = false;
            self.repeat_seen = None;
            self.journey = None;
            self.status = "Timeline changed; pending selections cleared.".into();
        }
        if old.is_some_and(|(_, revision)| revision != state.state().revision) {
            self.place_name = None;
            self.place_selected = self
                .place_selected
                .min(state.state().observation.places.len().saturating_sub(1));
            self.pickup.clear();
            self.attack_targets.clear();
            self.door_direction = None;
            self.travel_cursor = None;
            self.row_on.clear();
            self.row_count.clear();
            self.typed_count = None;
            if matches!(
                self.prompt,
                Prompt::Fight
                    | Prompt::Open
                    | Prompt::Close
                    | Prompt::Step { .. }
                    | Prompt::Run { .. }
                    | Prompt::Drop { .. }
                    | Prompt::DropMany
            ) {
                self.prompt = Prompt::None;
            }
        }
        if !state.has_control() {
            self.attack_targets.clear();
            self.place_name = None;
            self.door_direction = None;
            self.travel_cursor = None;
        }
        self.connected = true;
    }

    pub fn ready(&mut self) {
        self.busy = false;
        self.settle_flight();
    }

    pub fn accept_status(&mut self, status: String) {
        let failed = status.starts_with("InvalidAction") || status.starts_with("StaleRevision");
        if failed {
            let repeating = matches!(
                self.flight.as_ref().map(|flight| &flight.kind),
                Some(FlightKind::Repeat { .. } | FlightKind::RepeatWait)
            );
            if repeating
                && matches!(
                    self.queue.front(),
                    Some(Intent::Repeat { .. } | Intent::RepeatWait { .. })
                )
            {
                self.queue.pop_front();
            }
            self.flight = None;
            self.pickup_armed = false;
            self.journey = None;
            self.repeat_seen = None;
            if let Some(composed) = self.composed.take() {
                self.log.append(&gone_sentence(&composed.name));
            }
        }
        self.log.append(&status);
        self.status = status;
    }

    pub fn message_lines(&self) -> Vec<String> {
        self.log.display()
    }

    pub fn more(&self) -> bool {
        self.log.more()
    }

    pub fn scrollback_open(&self) -> bool {
        self.log.scrollback_open()
    }

    pub fn scrollback_lines(&self) -> &[String] {
        self.log.scrollback()
    }

    fn append_logged(&mut self, appended: LogAppend) {
        match appended {
            LogAppend::Narration => {
                let lines = self
                    .state
                    .as_ref()
                    .map(|state| state.narration().to_vec())
                    .unwrap_or_default();
                self.log.append_narration(lines.iter().map(String::as_str));
            }
            LogAppend::Line(text) => self.log.append(&text),
            LogAppend::Nothing => {}
        }
    }

    /// Page `--More--` and local scrollback before any command, including while busy.
    fn begin_input(&mut self, input: Input) -> Option<Input> {
        if self.log.scrollback_open() {
            if matches!(input, Input::Key { key: Key::Escape }) {
                self.log.close_scrollback();
            }
            return None;
        }
        if self.log.more() {
            if let Input::Key { key } = input {
                if matches!(key, Key::Space | Key::Enter | Key::Escape) {
                    self.log.acknowledge();
                    return None;
                }
                if key == Key::Scrollback {
                    self.log.acknowledge();
                    self.log.open_scrollback();
                    return None;
                }
                // This key also runs a command, so finish paging first. One
                // page can leave `--More--` up and the command queued forever.
                while self.log.acknowledge() {}
                return Some(Input::Key { key });
            }
            if matches!(input, Input::Click { .. }) {
                while self.log.acknowledge() {}
            }
        } else if matches!(
            input,
            Input::Key {
                key: Key::Scrollback
            }
        ) {
            self.log.open_scrollback();
            return None;
        }
        Some(input)
    }

    pub fn disconnect(&mut self, message: String) {
        self.travel_cursor = None;
        self.place_name = None;
        self.places_open = false;
        self.connected = false;
        self.busy = false;
        self.note = None;
        self.pickup.clear();
        self.attack_targets.clear();
        self.door_direction = None;
        self.queue.clear();
        self.flight = None;
        self.composed = None;
        self.prompt = Prompt::None;
        self.look_cursor = None;
        self.row_on.clear();
        self.row_count.clear();
        self.journey = None;
        self.repeat_seen = None;
        self.pickup_armed = false;
        self.typed_count = None;
        self.status = message;
    }

    fn adopt_letters(&mut self, reset: bool) {
        let ids = self.inventory_ids();
        if reset {
            self.letters.rebuild(&ids);
            self.queue.clear();
            self.flight = None;
            self.composed = None;
            self.journey = None;
            self.repeat_seen = None;
            self.pickup_armed = false;
            self.prompt = Prompt::None;
            self.look_cursor = None;
            self.door_direction = None;
            self.typed_count = None;
        } else {
            self.letters.retain(&ids);
        }
    }

    fn sync_letters(&mut self, take_or_drop: bool) {
        let ids = self.inventory_ids();
        self.letters.update(&ids, take_or_drop);
    }

    fn inventory_ids(&self) -> Vec<u64> {
        self.state
            .as_ref()
            .map(|state| {
                state
                    .state()
                    .observation
                    .inventory
                    .iter()
                    .map(|item| item.id)
                    .collect()
            })
            .unwrap_or_default()
    }

    fn resolve_flight(
        &mut self,
        observation_update: bool,
        travel_phase: Option<TravelPhase>,
        taken: Option<(u64, u64)>,
    ) -> bool {
        let mut suppress = self.composed.is_some() && observation_update;
        if let Some((item, result)) = taken {
            if let Some(composed) = self.composed.clone() {
                if item == composed.item || result == composed.item {
                    self.log
                        .append(&pickup_sentence(composed.via_travel, &composed.name));
                    self.composed = None;
                    suppress = true;
                }
            }
        }
        if let Some(phase) = travel_phase {
            match phase {
                TravelPhase::Active => {}
                TravelPhase::Arrived => {
                    if self
                        .journey
                        .as_ref()
                        .is_some_and(|journey| journey.via_travel)
                    {
                        suppress |= self.finish_autopickup(true);
                    }
                    self.journey = None;
                    self.pickup_armed = false;
                }
                _ => {
                    self.journey = None;
                    self.pickup_armed = false;
                    self.flight = None;
                }
            }
        }
        if observation_update {
            self.settle_flight();
            if self.pickup_armed {
                self.pickup_armed = false;
                suppress |= self.finish_autopickup(false);
                if self.repeat_seen.is_none() {
                    self.journey = None;
                }
            }
        }
        suppress
    }

    fn settle_flight(&mut self) {
        let Some(flight) = self.flight.take() else {
            return;
        };
        match flight.kind {
            FlightKind::Repeat { direction } => self.advance_repeat(direction),
            FlightKind::RepeatWait => self.advance_wait(),
            FlightKind::OneShot(_) => {}
        }
    }

    fn advance_repeat(&mut self, direction: Direction) {
        let seen = self.repeat_seen.clone().unwrap_or_default();
        let interrupted = self
            .state
            .as_ref()
            .is_some_and(|state| repeat_interrupted(&state.state().observation, direction, &seen));
        let Some(Intent::Repeat {
            remaining,
            direction: queued,
            ..
        }) = self.queue.front_mut()
        else {
            return;
        };
        if *queued != direction {
            return;
        }
        *remaining = remaining.saturating_sub(1);
        let done = *remaining == 0 || interrupted;
        if done {
            self.queue.pop_front();
            self.repeat_seen = None;
        }
    }

    fn advance_wait(&mut self) {
        let Some(Intent::RepeatWait { remaining }) = self.queue.front_mut() else {
            return;
        };
        *remaining = remaining.saturating_sub(1);
        let done = *remaining == 0;
        if done {
            self.queue.pop_front();
        }
    }

    fn finish_autopickup(&mut self, via_travel: bool) -> bool {
        let actors = self
            .journey
            .as_ref()
            .map(|journey| journey.actors.clone())
            .unwrap_or_default();
        let movement_allows = if via_travel {
            self.journey.as_ref().is_some_and(|journey| journey.pickup)
        } else {
            true
        };
        let same_branch = match (&self.journey, &self.state) {
            (Some(journey), Some(state)) => state.branch() == &journey.branch,
            _ => true,
        };
        let decision = {
            let Some(state) = &self.state else {
                return false;
            };
            let observation = &state.state().observation;
            decide_autopickup(AutoQuery {
                session_on: self.config.autopickup,
                movement_allows,
                has_control: state.has_control() && self.role != AccessRole::Spectator,
                same_branch,
                ready: observation.ready,
                travel: state.travel().map(|travel| travel.phase),
                actors_at_send: &actors,
                observation,
            })
        };
        match decision {
            AutoPickup::Skip => false,
            AutoPickup::Take(id) => {
                let name = self.item_name(id);
                let queued = self.queue.push_front(Intent::Act {
                    action: Action::Take {
                        item: id,
                        quantity: None,
                    },
                    pickup: false,
                });
                if !queued {
                    self.say(BUFFER_FULL);
                    return false;
                }
                self.composed = Some(Composed {
                    via_travel,
                    name,
                    item: id,
                });
                true
            }
            AutoPickup::Menu => {
                self.open_stacks(self.ground_stacks(), false);
                if matches!(
                    self.queue.front(),
                    Some(Intent::Repeat { .. } | Intent::RepeatWait { .. })
                ) {
                    self.queue.pop_front();
                    self.repeat_seen = None;
                }
                true
            }
        }
    }

    fn item_name(&self, id: u64) -> String {
        self.state
            .as_ref()
            .and_then(|state| {
                feet_items(&state.state().observation)
                    .into_iter()
                    .find(|item| item.item.id == id)
                    .map(|item| item.item.name.clone())
            })
            .unwrap_or_else(|| "item".into())
    }

    fn note_journey(&mut self, pickup: bool, via_travel: bool, keep_actors: bool) {
        let Some((current, branch)) = self.state.as_ref().map(|state| {
            (
                other_actors(&state.state().observation),
                state.branch().clone(),
            )
        }) else {
            return;
        };
        let actors = if keep_actors {
            if self.repeat_seen.is_none() {
                self.repeat_seen = Some(current.clone());
            }
            self.repeat_seen.clone().unwrap_or(current)
        } else {
            current
        };
        if self.journey.is_none() || !keep_actors {
            self.journey = Some(Journey {
                actors,
                branch,
                via_travel,
                pickup,
            });
        }
    }

    /// Send the front intent when the flight slot, the log, and any menu allow it.
    pub fn pump(&mut self) -> Effect {
        loop {
            if self.busy || !self.connected || self.blocks() {
                return Effect::None;
            }
            let Some(intent) = self.queue.front().cloned() else {
                return Effect::None;
            };
            match self.prepare(&intent) {
                Prepared::Hold => return Effect::None,
                Prepared::NotReady => {
                    self.say("Waiting for another actor to act.");
                    return Effect::None;
                }
                Prepared::Drop => {
                    self.queue.pop_front();
                }
                Prepared::Send { request, kind, pop } => {
                    if pop {
                        self.queue.pop_front();
                    }
                    self.arm(&intent, &request);
                    self.flight = Some(Flight { kind });
                    self.busy = true;
                    self.status = "Waiting for server...".into();
                    return Effect::Request(request);
                }
            }
        }
    }

    fn arm(&mut self, intent: &Intent, request: &Request) {
        if matches!(intent, Intent::Repeat { .. }) && self.repeat_seen.is_none() {
            let seen = self
                .state
                .as_ref()
                .map(|state| other_actors(&state.state().observation));
            if let Some(seen) = seen {
                self.repeat_seen = Some(seen);
            }
        }
        let Request::Command { command, .. } = request else {
            return;
        };
        match command {
            Command::Act {
                action: Action::Move { .. },
                ..
            } => {
                let pickup = match intent {
                    Intent::Step { pickup, .. }
                    | Intent::Repeat { pickup, .. }
                    | Intent::Act { pickup, .. } => *pickup,
                    _ => false,
                };
                let repeat = matches!(intent, Intent::Repeat { .. });
                self.note_journey(pickup, false, repeat);
                self.pickup_armed = pickup;
            }
            Command::Travel { .. } => {
                let pickup = matches!(intent, Intent::Travel { pickup: true, .. });
                self.note_journey(pickup, true, false);
                self.pickup_armed = false;
            }
            _ => {}
        }
    }

    fn prepare(&mut self, intent: &Intent) -> Prepared {
        let Some(state) = self.state.clone() else {
            return Prepared::Drop;
        };
        let observation = &state.state().observation;
        let travel_active = state
            .travel()
            .is_some_and(|travel| travel.phase == TravelPhase::Active);
        let needs_control = matches!(
            intent,
            Intent::Act { .. }
                | Intent::Step { .. }
                | Intent::Repeat { .. }
                | Intent::RepeatWait { .. }
                | Intent::Travel { .. }
        );
        if needs_control && (!state.has_control() || self.role == AccessRole::Spectator) {
            self.say("You are observing. Press F3 to request control.");
            return Prepared::Drop;
        }
        match intent {
            Intent::CancelTravel => {
                let Some(travel) = state.travel() else {
                    return Prepared::Drop;
                };
                Prepared::Send {
                    request: Request::CancelTravel {
                        branch: state.branch().clone(),
                        travel_id: travel.id.clone(),
                    },
                    kind: FlightKind::OneShot(intent.clone()),
                    pop: true,
                }
            }
            Intent::Continue => Prepared::Send {
                request: Request::Continue,
                kind: FlightKind::OneShot(intent.clone()),
                pop: true,
            },
            Intent::Acquire => Prepared::Send {
                request: Request::AcquireControl,
                kind: FlightKind::OneShot(intent.clone()),
                pop: true,
            },
            Intent::Release => Prepared::Send {
                request: Request::ReleaseControl,
                kind: FlightKind::OneShot(intent.clone()),
                pop: true,
            },
            Intent::History { before, limit } => Prepared::Send {
                request: Request::History {
                    before: before.clone(),
                    limit: *limit,
                },
                kind: FlightKind::OneShot(intent.clone()),
                pop: true,
            },
            Intent::Rename { key, name } => Prepared::Send {
                request: self.command_request(
                    &state,
                    Command::RenamePlace {
                        expected_revision: state.state().revision,
                        key: key.clone(),
                        name: name.clone(),
                    },
                ),
                kind: FlightKind::OneShot(intent.clone()),
                pop: true,
            },
            Intent::Annotate(command) => Prepared::Send {
                request: self.command_request(&state, command.clone()),
                kind: FlightKind::OneShot(intent.clone()),
                pop: true,
            },
            Intent::Travel { destination, .. } => {
                if travel_active {
                    return Prepared::Hold;
                }
                if !observation.ready {
                    return Prepared::NotReady;
                }
                Prepared::Send {
                    request: self.command_request(
                        &state,
                        Command::Travel {
                            expected_revision: state.state().revision,
                            destination: destination.clone(),
                        },
                    ),
                    kind: FlightKind::OneShot(intent.clone()),
                    pop: true,
                }
            }
            Intent::RepeatWait { .. } => {
                if travel_active {
                    return Prepared::Hold;
                }
                self.prepare_wait(&state, intent, true)
            }
            Intent::Act {
                action: Action::Wait,
                ..
            } => {
                if travel_active {
                    return Prepared::Hold;
                }
                self.prepare_wait(&state, intent, false)
            }
            Intent::Act { action, .. } => {
                if travel_active {
                    return Prepared::Hold;
                }
                if !observation.ready {
                    return Prepared::NotReady;
                }
                Prepared::Send {
                    request: self.command_request(
                        &state,
                        Command::Act {
                            expected_revision: state.state().revision,
                            action: action.clone(),
                        },
                    ),
                    kind: FlightKind::OneShot(intent.clone()),
                    pop: true,
                }
            }
            Intent::Step {
                direction,
                suppress_attack,
                ..
            } => {
                if travel_active {
                    return Prepared::Hold;
                }
                if !observation.ready {
                    return Prepared::NotReady;
                }
                self.prepare_bump(&state, intent, *direction, *suppress_attack, false, true)
            }
            Intent::Repeat {
                direction,
                suppress_attack,
                ..
            } => {
                if travel_active {
                    return Prepared::Hold;
                }
                if !observation.ready {
                    return Prepared::NotReady;
                }
                self.prepare_bump(&state, intent, *direction, *suppress_attack, true, false)
                    .with_repeat(*direction)
            }
        }
    }

    fn prepare_wait(&self, state: &ClientState, intent: &Intent, repeated: bool) -> Prepared {
        let observation = &state.state().observation;
        // A finished run never becomes ready. Holding the wait would leave
        // the key queued, so send it and let the server refuse a dead actor.
        if !observation.ready
            && !observation
                .combat
                .as_ref()
                .is_some_and(|combat| combat.terminal)
        {
            if observation
                .combat
                .as_ref()
                .is_some_and(|combat| !combat.terminal)
            {
                return Prepared::Send {
                    request: Request::Continue,
                    kind: if repeated {
                        FlightKind::RepeatWait
                    } else {
                        FlightKind::OneShot(intent.clone())
                    },
                    pop: !repeated,
                };
            }
            return Prepared::NotReady;
        }
        Prepared::Send {
            request: self.command_request(
                state,
                Command::Act {
                    expected_revision: state.state().revision,
                    action: Action::Wait,
                },
            ),
            kind: if repeated {
                FlightKind::RepeatWait
            } else {
                FlightKind::OneShot(intent.clone())
            },
            pop: !repeated,
        }
    }

    fn prepare_bump(
        &mut self,
        state: &ClientState,
        intent: &Intent,
        direction: Direction,
        suppress_attack: bool,
        running: bool,
        pop: bool,
    ) -> Prepared {
        match bump(
            &state.state().observation,
            direction,
            self.config.bump_attacks,
            suppress_attack,
            running,
        ) {
            Resolved::Stop => {
                self.repeat_seen = None;
                Prepared::Drop
            }
            Resolved::Miss(text) => {
                self.record(text);
                self.repeat_seen = None;
                Prepared::Drop
            }
            resolved => {
                let Some(action) = to_action(&resolved) else {
                    return Prepared::Drop;
                };
                Prepared::Send {
                    request: self.command_request(
                        state,
                        Command::Act {
                            expected_revision: state.state().revision,
                            action,
                        },
                    ),
                    kind: FlightKind::OneShot(intent.clone()),
                    pop,
                }
            }
        }
    }

    fn command_request(&self, state: &ClientState, command: Command) -> Request {
        Request::Command {
            branch: state.branch().clone(),
            command,
        }
    }

    /// A pickup that finishes a move this client already sent is part of that
    /// move. Reading `--More--` does not leave it waiting in front of the queue.
    fn autopickup_take_pending(&self) -> bool {
        self.composed.is_some()
            && matches!(
                self.queue.front(),
                Some(Intent::Act {
                    action: Action::Take { .. },
                    ..
                })
            )
    }

    fn blocks(&self) -> bool {
        (self.log.more() && !self.autopickup_take_pending())
            || self.log.scrollback_open()
            || self.note.is_some()
            || (self.places_open && !matches!(self.queue.front(), Some(Intent::Rename { .. })))
            || (self.history_page.is_some()
                && !matches!(self.queue.front(), Some(Intent::History { .. })))
            || !self.pickup.is_empty()
            || self.travel_cursor.is_some()
            || self.door_direction.is_some()
            || !matches!(self.prompt, Prompt::None)
    }

    fn travel_active(&self) -> bool {
        self.state.as_ref().is_some_and(|state| {
            state
                .travel()
                .is_some_and(|travel| travel.phase == TravelPhase::Active)
        })
    }

    fn enqueue(&mut self, intent: Intent) -> Effect {
        if !self.connected {
            return Effect::None;
        }
        if !self.queue.push_back(intent) {
            self.say(BUFFER_FULL);
            return Effect::None;
        }
        if self.busy || self.blocks() {
            return Effect::None;
        }
        self.pump()
    }

    fn say(&mut self, text: &str) {
        self.status = text.to_owned();
    }

    fn record(&mut self, text: &str) {
        self.status = text.to_owned();
        self.log.append(text);
    }

    pub fn input(&mut self, input: Input) -> Effect {
        if matches!(input, Input::Key { key: Key::Quit }) {
            return Effect::Quit;
        }
        let Some(input) = self.begin_input(input) else {
            return Effect::None;
        };
        if self.note.is_some() {
            return self.note_input(input);
        }
        if self.places_open {
            return self.places_input(input);
        }
        if !self.pickup.is_empty() && !matches!(input, Input::Key { key: Key::Escape }) {
            return self.menu_input(input);
        }
        if matches!(input, Input::Key { key: Key::Escape }) {
            return self.escape();
        }
        if let Input::Click { x, y } = input {
            return self.click(x, y);
        }
        if let Input::Text { text } = input {
            return self.text_input(text);
        }
        let Input::Key { key } = input else {
            return Effect::None;
        };
        if self.history_page.is_some() {
            return self.history_input(key);
        }
        if self.travel_cursor.is_some() {
            return self.travel_cursor_input(key);
        }
        if !matches!(self.prompt, Prompt::None) {
            return self.prompt_input(key);
        }
        if self.role == AccessRole::Spectator && !Self::spectator_local(key) {
            self.say("Spectator access is read-only.");
            return Effect::None;
        }
        if !self.connected {
            return Effect::None;
        }
        self.playing(key)
    }

    fn spectator_local(key: Key) -> bool {
        matches!(
            key,
            Key::Places
                | Key::History
                | Key::OlderHistory
                | Key::RecentHistory
                | Key::Inventory
                | Key::Describe
                | Key::Look
                | Key::WhatIs
                | Key::Help
                | Key::KeyHelp
        )
    }

    fn note_input(&mut self, input: Input) -> Effect {
        if matches!(input, Input::Key { key: Key::Escape }) {
            self.note = None;
            self.say("Cancelled.");
            return Effect::None;
        }
        match input {
            Input::Text { text } => {
                if let Some(draft) = &mut self.note {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        if draft.text.len() + ch.len_utf8() <= MAX_NOTE_BYTES {
                            draft.text.push(ch);
                        }
                    }
                }
            }
            Input::Key {
                key: Key::Backspace,
            } => {
                if let Some(draft) = &mut self.note {
                    draft.text.pop();
                }
            }
            Input::Key { key: Key::Tab } => {
                if let Some(draft) = &mut self.note {
                    draft.audience = if draft.audience == Audience::Private {
                        Audience::Actor
                    } else {
                        Audience::Private
                    };
                }
            }
            Input::Key { key: Key::Enter } => {
                let Some(draft) = self.note.as_ref() else {
                    return Effect::None;
                };
                if draft.text.trim().is_empty() {
                    return Effect::None;
                }
                let command = Command::Annotate {
                    anchor: Anchor::State {
                        revision: draft.revision,
                    },
                    text: draft.text.clone(),
                    audience: draft.audience,
                    source: ClientSource::User,
                    category: AnnotationCategory::Note,
                };
                self.note = None;
                return self.enqueue(Intent::Annotate(command));
            }
            _ => {}
        }
        Effect::None
    }

    fn places_input(&mut self, input: Input) -> Effect {
        if matches!(input, Input::Key { key: Key::Escape }) {
            if self.place_name.take().is_some() {
                return Effect::None;
            }
            self.places_open = false;
            return Effect::None;
        }
        if self.place_name.is_some() {
            match input {
                Input::Text { text } => {
                    if let Some(name) = &mut self.place_name {
                        for ch in text.chars().filter(|ch| !ch.is_control()) {
                            if name.len() + ch.len_utf8() <= 80 {
                                name.push(ch);
                            }
                        }
                    }
                }
                Input::Key {
                    key: Key::Backspace,
                } => {
                    if let Some(name) = &mut self.place_name {
                        name.pop();
                    }
                }
                Input::Key { key: Key::Enter } => {
                    let Some(raw) = self.place_name.as_ref() else {
                        return Effect::None;
                    };
                    if raw.trim().is_empty() {
                        return Effect::None;
                    }
                    let name = raw.trim().to_owned();
                    let allowed = self.state.as_ref().is_some_and(|state| {
                        self.role != AccessRole::Spectator && state.has_control()
                    });
                    let key = self.state.as_ref().and_then(|state| {
                        state
                            .state()
                            .observation
                            .places
                            .get(self.place_selected)
                            .map(|place| place.key.clone())
                    });
                    self.place_name = None;
                    if !allowed {
                        self.say("Naming places requires control.");
                        return Effect::None;
                    }
                    let Some(key) = key else {
                        return Effect::None;
                    };
                    return self.enqueue(Intent::Rename { key, name });
                }
                _ => {}
            }
            return Effect::None;
        }
        let count = self
            .state
            .as_ref()
            .map(|state| state.state().observation.places.len())
            .unwrap_or(0);
        match input {
            Input::Key { key: Key::Up } => {
                self.place_selected = self.place_selected.saturating_sub(1)
            }
            Input::Key { key: Key::Down } => {
                self.place_selected = (self.place_selected + 1).min(count.saturating_sub(1))
            }
            Input::Key { key: Key::Enter } if count > 0 => {
                let allowed = self
                    .state
                    .as_ref()
                    .is_some_and(|state| self.role != AccessRole::Spectator && state.has_control());
                if allowed {
                    self.place_name = Some(String::new());
                } else {
                    self.say("Naming places requires control.");
                }
            }
            _ => {}
        }
        Effect::None
    }

    fn menu_input(&mut self, input: Input) -> Effect {
        let Input::Key { key } = input else {
            return Effect::None;
        };
        if let Some(digit) = digit_value(key, true) {
            return self.push_digit(digit);
        }
        if key == Key::Space {
            return self.confirm_menu();
        }
        if let Some(ch) = typed_letter(key) {
            self.toggle_row(ch);
        }
        Effect::None
    }

    fn toggle_row(&mut self, ch: char) {
        let Some(index) = letter_index(ch) else {
            return;
        };
        if index >= self.pickup.len() {
            return;
        }
        let count = if self.quantity.is_empty() {
            None
        } else {
            self.quantity.parse::<u64>().ok()
        };
        self.quantity.clear();
        if let Some(0) = count {
            self.say(BAD_QUANTITY);
            return;
        }
        if let Some(count) = count {
            self.row_count[index] = Some(count);
            self.row_on[index] = true;
        } else {
            self.row_on[index] = !self.row_on[index];
            if !self.row_on[index] {
                self.row_count[index] = None;
            }
        }
    }

    fn confirm_menu(&mut self) -> Effect {
        let dropping = self.dropping;
        let mut intents = Vec::new();
        for (index, item) in self.pickup.iter().enumerate() {
            if !self.row_on.get(index).copied().unwrap_or(false) {
                continue;
            }
            match take_quantity(item.quantity, self.row_count.get(index).copied().flatten()) {
                Ok(quantity) => intents.push(Intent::Act {
                    action: if dropping {
                        Action::Drop {
                            item: item.id,
                            quantity,
                        }
                    } else {
                        Action::Take {
                            item: item.id,
                            quantity,
                        }
                    },
                    pickup: false,
                }),
                Err(text) => {
                    self.say(text);
                    return Effect::None;
                }
            }
        }
        if intents.is_empty() {
            self.say("Nothing selected.");
            return Effect::None;
        }
        self.clear_menu();
        let mut sent = Effect::None;
        for intent in intents {
            let effect = self.enqueue(intent);
            if matches!(sent, Effect::None) && !matches!(effect, Effect::None) {
                sent = effect;
            }
        }
        sent
    }

    fn clear_menu(&mut self) {
        self.pickup.clear();
        self.row_on.clear();
        self.row_count.clear();
        self.quantity.clear();
        self.dropping = false;
        if matches!(self.prompt, Prompt::DropMany) {
            self.prompt = Prompt::None;
        }
    }

    fn escape(&mut self) -> Effect {
        if self.cancel_local() {
            return Effect::None;
        }
        if !self.connected || !self.travel_active() {
            return Effect::None;
        }
        if self.queue.has_cancel() {
            self.queue.clear();
            self.repeat_seen = None;
            return Effect::None;
        }
        if !self.queue.push_front(Intent::CancelTravel) {
            self.say(BUFFER_FULL);
            return Effect::None;
        }
        if self.busy {
            return Effect::Enqueued;
        }
        self.pump()
    }

    fn cancel_local(&mut self) -> bool {
        if self.travel_cursor.take().is_some() {
            self.say("Travel selection cancelled.");
            self.follow();
            return true;
        }
        if self.note.take().is_some() {
            self.say("Cancelled.");
            return true;
        }
        if self.history_page.take().is_some() {
            self.history_scroll = 0;
            return true;
        }
        if !self.pickup.is_empty() {
            self.clear_menu();
            self.say("Cancelled.");
            return true;
        }
        if self.door_direction.take().is_some() || !matches!(self.prompt, Prompt::None) {
            self.prompt = Prompt::None;
            self.look_cursor = None;
            self.say("Cancelled.");
            return true;
        }
        if self.typed_count.take().is_some() || self.look_cursor.take().is_some() {
            self.say("Cancelled.");
            return true;
        }
        false
    }

    fn click(&mut self, x: usize, y: usize) -> Effect {
        if self.history_page.is_some() || self.places_open || self.note.is_some() {
            return Effect::None;
        }
        let Some(position) = crate::render::cell_at(self, x, y) else {
            return Effect::None;
        };
        match self.prompt {
            Prompt::Fight => self.click_fight(position),
            Prompt::Open => self.click_door(position, true),
            Prompt::Close => self.click_door(position, false),
            Prompt::Look => {
                self.look_cursor = Some(position);
                self.record(&self.look_sentence(position.x, position.y));
                self.follow();
                Effect::None
            }
            _ if !self.pickup.is_empty() => Effect::None,
            _ => self.idle_click(position),
        }
    }

    fn click_fight(&mut self, position: Position) -> Effect {
        let target = self.state.as_ref().map(|state| {
            let observation = &state.state().observation;
            if !adjacent(observation.position, position) {
                return Err(());
            }
            creature_at(observation, position.x, position.y).ok_or(())
        });
        match target {
            Some(Ok(target)) => {
                self.prompt = Prompt::None;
                self.enqueue(Intent::Act {
                    action: Action::Attack { target },
                    pickup: false,
                })
            }
            Some(Err(())) => {
                self.record(ILLEGAL_TARGET);
                Effect::None
            }
            None => Effect::None,
        }
    }

    fn click_door(&mut self, position: Position, open: bool) -> Effect {
        let door = self.state.as_ref().map(|state| {
            let observation = &state.state().observation;
            if !adjacent(observation.position, position) {
                return Err(());
            }
            door_at(observation, position, open).ok_or(())
        });
        match door {
            Some(Ok(door)) => {
                self.prompt = Prompt::None;
                self.door_direction = None;
                self.enqueue(Intent::Act {
                    action: Action::SetDoor { door, open },
                    pickup: false,
                })
            }
            Some(Err(())) => {
                self.record(ILLEGAL_TARGET);
                Effect::None
            }
            None => Effect::None,
        }
    }

    fn idle_click(&mut self, position: Position) -> Effect {
        if self.config.click == Click::Look {
            self.record(&self.look_sentence(position.x, position.y));
            return Effect::None;
        }
        self.typed_count = None;
        self.travel_to(position)
    }

    fn text_input(&mut self, text: String) -> Effect {
        match &mut self.prompt {
            Prompt::Extended(buffer) | Prompt::WhatIs(buffer) => {
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    if buffer.chars().count() >= 32 {
                        break;
                    }
                    buffer.push(ch);
                }
            }
            _ => {}
        }
        Effect::None
    }

    fn history_input(&mut self, key: Key) -> Effect {
        match key {
            Key::Up => self.history_scroll = self.history_scroll.saturating_sub(1),
            Key::Down => {
                let len = self
                    .history_page
                    .as_ref()
                    .map(|page| history_lines(&page.entries).len())
                    .unwrap_or(0);
                self.history_scroll = (self.history_scroll + 1).min(len.saturating_sub(20));
            }
            Key::OlderHistory | Key::RecentHistory | Key::History => {
                return self.playing(key);
            }
            _ => {}
        }
        Effect::None
    }

    fn prompt_input(&mut self, key: Key) -> Effect {
        if let Some(digit) = digit_value(key, true) {
            if matches!(self.prompt, Prompt::Drop { .. } | Prompt::Extended(_)) {
                return self.push_digit(digit);
            }
        }
        match self.prompt.clone() {
            Prompt::Fight => self.directed(key, |this, direction| {
                let resolved = this
                    .state
                    .as_ref()
                    .map(|state| fight(&state.state().observation, direction))
                    .unwrap_or(Resolved::Miss(ILLEGAL_TARGET));
                this.finish_resolved(resolved, false)
            }),
            Prompt::Open => self.directed(key, |this, direction| this.door_key(direction, true)),
            Prompt::Close => self.directed(key, |this, direction| this.door_key(direction, false)),
            Prompt::Step {
                pickup,
                suppress_attack,
            } => self.directed(key, move |this, direction| {
                this.typed_count = None;
                this.prompt = Prompt::None;
                this.step(direction, None, false, pickup, suppress_attack)
            }),
            Prompt::Run { pickup } => self.directed(key, move |this, direction| {
                let count = this.typed_count.take();
                this.prompt = Prompt::None;
                this.step(direction, count, true, pickup, false)
            }),
            Prompt::Drop { count } => {
                if let Some(ch) = typed_letter(key) {
                    self.drop_letter(ch, count)
                } else if let Some(digit) = digit_value(key, true) {
                    self.push_digit(digit)
                } else {
                    Effect::None
                }
            }
            Prompt::Extended(_) => self.extended_key(key),
            Prompt::WhatIs(_) => self.what_is_key(key),
            Prompt::Look => self.look_key(key),
            Prompt::Adjust(first) => self.adjust_key(first, key),
            Prompt::KeyHelp => {
                self.record(key_line(key));
                self.prompt = Prompt::None;
                Effect::None
            }
            Prompt::Inventory | Prompt::DropMany | Prompt::None => Effect::None,
        }
    }

    fn directed(&mut self, key: Key, apply: impl FnOnce(&mut Self, Direction) -> Effect) -> Effect {
        let Some(direction) = plain_direction(key) else {
            return Effect::None;
        };
        apply(self, direction)
    }

    fn finish_resolved(&mut self, resolved: Resolved, click: bool) -> Effect {
        match resolved {
            Resolved::Attack(target) => {
                self.prompt = Prompt::None;
                self.enqueue(Intent::Act {
                    action: Action::Attack { target },
                    pickup: false,
                })
            }
            Resolved::Open(door) => self.enqueue_door(door, true),
            Resolved::Close(door) => self.enqueue_door(door, false),
            Resolved::Miss(text) if click => {
                self.record(text);
                Effect::None
            }
            Resolved::Miss(text) => {
                self.prompt = Prompt::None;
                self.door_direction = None;
                self.record(text);
                Effect::None
            }
            Resolved::Move(_) | Resolved::Stop => Effect::None,
        }
    }

    fn enqueue_door(&mut self, door: u64, open: bool) -> Effect {
        self.prompt = Prompt::None;
        self.door_direction = None;
        self.enqueue(Intent::Act {
            action: Action::SetDoor { door, open },
            pickup: false,
        })
    }

    fn door_key(&mut self, direction: Direction, open: bool) -> Effect {
        let resolved = self
            .state
            .as_ref()
            .map(|state| door_command(&state.state().observation, direction, open))
            .unwrap_or(Resolved::Miss(ILLEGAL_TARGET));
        self.finish_resolved(resolved, false)
    }

    fn drop_letter(&mut self, ch: char, count: Option<u32>) -> Effect {
        self.prompt = Prompt::None;
        let Some(id) = self.letters.id_for(ch) else {
            self.say("That isn't an inventory letter.");
            return Effect::None;
        };
        let disclosed = self
            .state
            .as_ref()
            .and_then(|state| {
                state
                    .state()
                    .observation
                    .inventory
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| item.quantity)
            })
            .unwrap_or(0);
        let requested = count.map(u64::from);
        match take_quantity(disclosed, requested) {
            Ok(quantity) => self.enqueue(Intent::Act {
                action: Action::Drop { item: id, quantity },
                pickup: false,
            }),
            Err(text) => {
                self.say(text);
                Effect::None
            }
        }
    }

    fn extended_key(&mut self, key: Key) -> Effect {
        if let Some(digit) = digit_value(key, true) {
            return self.push_digit(digit);
        }
        match key {
            Key::Backspace => {
                if let Prompt::Extended(buffer) = &mut self.prompt {
                    buffer.pop();
                }
                Effect::None
            }
            Key::Letter(ch) if self.append_buffer(ch) => Effect::None,
            Key::Enter => self.submit_extended(),
            _ => Effect::None,
        }
    }

    fn what_is_key(&mut self, key: Key) -> Effect {
        match key {
            Key::Backspace => {
                if let Prompt::WhatIs(buffer) = &mut self.prompt {
                    buffer.pop();
                }
                Effect::None
            }
            Key::Letter(ch) if self.append_buffer(ch) => Effect::None,
            Key::Enter => {
                let query = match &self.prompt {
                    Prompt::WhatIs(text) => text.clone(),
                    _ => String::new(),
                };
                self.prompt = Prompt::None;
                self.record(&self.what_is_line(&query));
                Effect::None
            }
            _ => Effect::None,
        }
    }

    fn append_buffer(&mut self, ch: char) -> bool {
        let buffer = match &mut self.prompt {
            Prompt::Extended(buffer) | Prompt::WhatIs(buffer) => buffer,
            _ => return false,
        };
        if buffer.chars().count() >= 32 {
            return true;
        }
        buffer.push(ch);
        true
    }

    fn submit_extended(&mut self) -> Effect {
        let name = match &self.prompt {
            Prompt::Extended(text) => text.trim().to_owned(),
            _ => return Effect::None,
        };
        match name.as_str() {
            "adjust" => {
                self.prompt = Prompt::Adjust(None);
                self.say("Swap which letters?");
            }
            _ => {
                self.prompt = Prompt::None;
                self.say(UNAVAILABLE);
            }
        }
        Effect::None
    }

    fn adjust_key(&mut self, first: Option<char>, key: Key) -> Effect {
        let Some(ch) = typed_letter(key) else {
            return Effect::None;
        };
        match first {
            None => {
                self.prompt = Prompt::Adjust(Some(ch));
                self.say("Swap with which letter?");
                Effect::None
            }
            Some(first) => {
                self.prompt = Prompt::None;
                match self.letters.adjust(first, ch) {
                    Ok(()) => self.say("Letters adjusted."),
                    Err(text) => self.say(text),
                }
                Effect::None
            }
        }
    }

    fn look_key(&mut self, key: Key) -> Effect {
        let Some(direction) = plain_direction(key) else {
            return Effect::None;
        };
        let (dx, dy, dz) = offset(direction);
        let origin = self.look_cursor.or_else(|| self.player_position());
        let Some(mut cursor) = origin else {
            return Effect::None;
        };
        cursor.x += dx;
        cursor.y += dy;
        cursor.z += dz;
        self.look_cursor = Some(cursor);
        self.record(&self.look_sentence(cursor.x, cursor.y));
        self.follow();
        Effect::None
    }

    fn push_digit(&mut self, digit: u32) -> Effect {
        if let Prompt::Drop { count } = &mut self.prompt {
            let next = count
                .unwrap_or(0)
                .saturating_mul(10)
                .saturating_add(digit)
                .min(1_000_000);
            *count = Some(next);
            return Effect::None;
        }
        if let Prompt::Extended(buffer) = &mut self.prompt {
            if buffer.chars().count() < 32 {
                if let Some(ch) = char::from_digit(digit, 10) {
                    buffer.push(ch);
                }
            }
            return Effect::None;
        }
        if !self.pickup.is_empty() {
            if self.quantity.len() < 7 {
                if let Some(ch) = char::from_digit(digit, 10) {
                    self.quantity.push(ch);
                }
            }
            return Effect::None;
        }
        let next = self
            .typed_count
            .unwrap_or(0)
            .saturating_mul(10)
            .saturating_add(digit)
            .min(10_000);
        self.typed_count = Some(next);
        Effect::None
    }

    fn playing(&mut self, key: Key) -> Effect {
        if let Some(digit) = digit_value(key, self.typed_count.is_some()) {
            return self.push_digit(digit);
        }
        if matches!(key, Key::Numpad5 | Key::Wait) {
            return self.wait_key();
        }
        if let Some(direction) = shifted_run(key) {
            let count = self.typed_count.take();
            return self.step(direction, count, true, true, false);
        }
        if let Some(direction) = plain_direction(key) {
            let count = self.typed_count.take();
            return self.step(direction, count, false, true, false);
        }
        match key {
            Key::Go | Key::Letter('g') => self.start_run(true),
            Key::SuppressRun | Key::Letter('M') => self.start_run(false),
            Key::Suppress | Key::Letter('m') => self.start_step(),
            Key::Fight | Key::Letter('F') => self.start_fight(),
            Key::OpenDoor | Key::Letter('o') => self.start_door(true),
            Key::CloseDoor | Key::Letter('c') => self.start_door(false),
            Key::Pickup | Key::Letter(',') => self.pickup_key(),
            Key::Drop | Key::Letter('d') => self.start_drop(),
            Key::DropMany | Key::Letter('D') => self.start_drop_many(),
            Key::Inventory | Key::Letter('i') => {
                self.prompt = Prompt::Inventory;
                self.say("Inventory. Esc closes.");
                Effect::None
            }
            Key::Describe => {
                if let Some(position) = self.player_position() {
                    self.record(&self.look_sentence(position.x, position.y));
                }
                Effect::None
            }
            Key::Look => self.start_look(),
            Key::WhatIs => {
                self.prompt = Prompt::WhatIs(String::new());
                self.say("What is?");
                Effect::None
            }
            Key::Help => {
                self.record(HELP_LINE);
                Effect::None
            }
            Key::KeyHelp => {
                self.prompt = Prompt::KeyHelp;
                self.say("Press a key.");
                Effect::None
            }
            Key::Extended => {
                self.prompt = Prompt::Extended(String::new());
                self.say("#");
                Effect::None
            }
            Key::Autopickup => {
                self.config.autopickup = !self.config.autopickup;
                self.say(if self.config.autopickup {
                    "Autopickup on."
                } else {
                    "Autopickup off."
                });
                Effect::None
            }
            Key::Travel => self.start_travel_cursor(),
            Key::Control => self.enqueue(Intent::Acquire),
            Key::Release => self.enqueue(Intent::Release),
            Key::Note => self.start_note(),
            Key::Places => {
                self.places_open = true;
                self.place_selected = 0;
                Effect::None
            }
            Key::History => self.enqueue(Intent::History {
                before: None,
                limit: 50,
            }),
            Key::OlderHistory => self.older_history(),
            Key::RecentHistory => {
                self.history_page = None;
                Effect::None
            }
            Key::Letter(ch) if unavailable_letter(ch) => {
                self.say(UNAVAILABLE);
                Effect::None
            }
            _ => Effect::None,
        }
    }

    fn travel_cursor_input(&mut self, key: Key) -> Effect {
        if key == Key::Enter {
            let Some(cursor) = self.travel_cursor else {
                return Effect::None;
            };
            return self.travel_to(cursor);
        }
        let Some(direction) = plain_direction(key) else {
            return Effect::None;
        };
        let Some(mut cursor) = self.travel_cursor else {
            return Effect::None;
        };
        let (dx, dy, dz) = offset(direction);
        cursor.x = (cursor.x + dx).clamp(-16, 16);
        cursor.y = (cursor.y + dy).clamp(-16, 16);
        cursor.z = (cursor.z + dz).clamp(-16, 16);
        self.travel_cursor = Some(cursor);
        self.follow();
        Effect::None
    }

    fn wait_key(&mut self) -> Effect {
        if self.blocked_by_travel() {
            return Effect::None;
        }
        let count = self.typed_count.take();
        match count {
            Some(0) | None => self.enqueue(Intent::Act {
                action: Action::Wait,
                pickup: false,
            }),
            Some(count) => self.enqueue(Intent::RepeatWait {
                remaining: u16::try_from(count.min(u32::from(tor_client_hack::REPEAT_CAP)))
                    .unwrap_or(tor_client_hack::REPEAT_CAP),
            }),
        }
    }

    fn step(
        &mut self,
        direction: Direction,
        count: Option<u32>,
        run: bool,
        pickup: bool,
        suppress_attack: bool,
    ) -> Effect {
        if self.blocked_by_travel() {
            return Effect::None;
        }
        let intent = match count {
            Some(0) | None if !run => Intent::Step {
                direction,
                pickup,
                suppress_attack,
            },
            Some(0) | None => {
                repeat_intent(direction, u32::from(RUN_CAP), true, pickup, suppress_attack)
            }
            Some(count) => repeat_intent(direction, count, run, pickup, suppress_attack),
        };
        self.enqueue(intent)
    }

    fn blocked_by_travel(&mut self) -> bool {
        if self.travel_active() {
            self.say(CANCEL_TRAVEL_FIRST);
            true
        } else {
            false
        }
    }

    fn start_run(&mut self, pickup: bool) -> Effect {
        if self.blocked_by_travel() {
            return Effect::None;
        }
        self.prompt = Prompt::Run { pickup };
        self.say("Run in which direction?");
        Effect::None
    }

    fn start_step(&mut self) -> Effect {
        if self.blocked_by_travel() {
            return Effect::None;
        }
        self.prompt = Prompt::Step {
            pickup: false,
            suppress_attack: true,
        };
        self.say("Move in which direction?");
        Effect::None
    }

    fn start_fight(&mut self) -> Effect {
        if self.blocked_by_travel() {
            return Effect::None;
        }
        self.prompt = Prompt::Fight;
        self.say("Fight in which direction?");
        Effect::None
    }

    fn start_door(&mut self, open: bool) -> Effect {
        let gate = self.state.as_ref().map(|state| {
            (
                self.role != AccessRole::Spectator && state.has_control(),
                state.state().observation.ready,
            )
        });
        let Some((controlled, ready)) = gate else {
            return Effect::None;
        };
        if !controlled {
            self.say("You are observing. Press F3 to request control.");
            return Effect::None;
        }
        if !ready {
            self.say("Waiting for another actor to act.");
            return Effect::None;
        }
        if self.travel_active() {
            self.say("Press Esc to cancel travel before using a door.");
            return Effect::None;
        }
        self.prompt = if open { Prompt::Open } else { Prompt::Close };
        self.door_direction = Some(open);
        self.say(&format!(
            "{} in which direction? HJKL/YUBN; Esc cancels.",
            if open { "Open" } else { "Close" }
        ));
        Effect::None
    }

    fn pickup_key(&mut self) -> Effect {
        if self.blocked_by_travel() {
            return Effect::None;
        }
        let count = self.typed_count.take();
        let stacks = self.ground_stacks();
        match stacks.as_slice() {
            [] => {
                self.say(NOTHING_HERE);
                Effect::None
            }
            [only] => {
                let requested = count.map(u64::from);
                match take_quantity(only.quantity, requested) {
                    Ok(quantity) => self.enqueue(Intent::Act {
                        action: Action::Take {
                            item: only.id,
                            quantity,
                        },
                        pickup: false,
                    }),
                    Err(text) => {
                        self.say(text);
                        Effect::None
                    }
                }
            }
            _ => {
                if let Some(count) = count {
                    self.quantity = count.to_string();
                }
                self.open_stacks(stacks, false);
                Effect::None
            }
        }
    }

    fn start_drop(&mut self) -> Effect {
        if self.inventory_ids().is_empty() {
            self.say("Your inventory is empty.");
            return Effect::None;
        }
        self.typed_count = None;
        self.prompt = Prompt::Drop { count: None };
        self.say("Drop which item?");
        Effect::None
    }

    fn start_drop_many(&mut self) -> Effect {
        let items = self.inventory_items();
        if items.is_empty() {
            self.say("Your inventory is empty.");
            return Effect::None;
        }
        self.open_stacks(items, true);
        Effect::None
    }

    fn ground_stacks(&self) -> Vec<ItemView> {
        self.state
            .as_ref()
            .map(|state| {
                feet_items(&state.state().observation)
                    .into_iter()
                    .map(|item| item.item.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn inventory_items(&self) -> Vec<ItemView> {
        let mut items = self
            .state
            .as_ref()
            .map(|state| state.state().observation.inventory.clone())
            .unwrap_or_default();
        items.sort_by_key(|item| item.id);
        items.dedup_by_key(|item| item.id);
        items
    }

    fn open_stacks(&mut self, items: Vec<ItemView>, dropping: bool) {
        let count = items.len();
        self.pickup = items;
        self.dropping = dropping;
        self.selected = 0;
        self.row_on = vec![false; count];
        self.row_count = vec![None; count];
        self.prompt = if dropping {
            Prompt::DropMany
        } else {
            Prompt::None
        };
        self.say("Toggle a letter, then Space. Esc cancels.");
    }

    fn start_look(&mut self) -> Effect {
        let Some(position) = self.player_position() else {
            return Effect::None;
        };
        self.look_cursor = Some(position);
        self.prompt = Prompt::Look;
        self.record(&self.look_sentence(position.x, position.y));
        self.follow();
        Effect::None
    }

    fn start_travel_cursor(&mut self) -> Effect {
        if self.travel_active() {
            self.say(CANCEL_TRAVEL_FIRST);
            return Effect::None;
        }
        let position = self.state.as_ref().and_then(|state| {
            if state.has_control() && self.role != AccessRole::Spectator {
                Some(state.state().observation.position)
            } else {
                None
            }
        });
        if let Some(position) = position {
            self.travel_cursor = Some(position);
            self.say("Travel: HJKL/YUBN select, </> height, Enter confirms, Esc cancels.");
            self.follow();
        } else {
            self.say("Acquire control before travelling.");
        }
        Effect::None
    }

    fn start_note(&mut self) -> Effect {
        let revision = self.state.as_ref().map(|state| state.state().revision);
        if let Some(revision) = revision {
            self.note = Some(NoteDraft {
                text: String::new(),
                audience: Audience::Private,
                revision,
            });
        }
        Effect::None
    }

    fn older_history(&mut self) -> Effect {
        let cursor = self
            .history_page
            .as_ref()
            .map(|page| page.older_before.clone())
            .unwrap_or_else(|| {
                self.state
                    .as_ref()
                    .and_then(|state| state.older_before().cloned())
            });
        if let Some(before) = cursor {
            self.enqueue(Intent::History {
                before: Some(before),
                limit: 50,
            })
        } else {
            self.say("No older history entries.");
            Effect::None
        }
    }

    fn player_position(&self) -> Option<Position> {
        self.state
            .as_ref()
            .map(|state| state.state().observation.position)
    }

    fn look_sentence(&self, x: i32, y: i32) -> String {
        let Some(state) = &self.state else {
            return "You have never seen that place.".into();
        };
        let chart: Vec<_> = state.map_memory().collect();
        look_at(&state.state().observation, &chart, x, y)
    }

    fn what_is_line(&self, query: &str) -> String {
        let needle = query.trim();
        if needle.is_empty() {
            return "Name something you can see.".into();
        }
        let Some(state) = &self.state else {
            return format!("You don't recognize \"{needle}\".");
        };
        let observation = &state.state().observation;
        if let Some(item) = observation
            .inventory
            .iter()
            .chain(observation.ground_items.iter().map(|item| &item.item))
            .find(|item| item.name.eq_ignore_ascii_case(needle))
        {
            if item.identified {
                return format!("{} x {}.", item.quantity, item.name);
            }
            return format!("{} x {} ({})", item.quantity, item.appearance, item.name);
        }
        format!("You don't recognize \"{needle}\".")
    }

    fn travel_to(&mut self, position: Position) -> Effect {
        let decision = self.state.as_ref().map(|state| {
            if self.role == AccessRole::Spectator
                || !state.has_control()
                || !state.state().observation.ready
            {
                return Err("Travel requires control of a ready actor.");
            }
            if state
                .travel()
                .is_some_and(|travel| travel.phase == TravelPhase::Active)
            {
                return Err(CANCEL_TRAVEL_FIRST);
            }
            state
                .state()
                .observation
                .visible_cells
                .iter()
                .find(|cell| {
                    cell.position == position
                        && !cell.wall
                        && cell.door.as_ref().is_none_or(|door| door.open)
                })
                .map(|cell| cell.key.clone())
                .ok_or("Select a visible floor cell.")
        });
        match decision {
            Some(Ok(destination)) => {
                self.travel_cursor = None;
                self.follow();
                self.enqueue(Intent::Travel {
                    destination,
                    pickup: self.config.autopickup,
                })
            }
            Some(Err(text)) => {
                self.say(text);
                Effect::None
            }
            None => Effect::None,
        }
    }
}

enum Prepared {
    Hold,
    NotReady,
    Drop,
    Send {
        request: Request,
        kind: FlightKind,
        pop: bool,
    },
}

impl Prepared {
    fn with_repeat(self, direction: Direction) -> Self {
        match self {
            Prepared::Send {
                request,
                pop: false,
                ..
            } => Prepared::Send {
                request,
                kind: FlightKind::Repeat { direction },
                pop: false,
            },
            other => other,
        }
    }
}

fn unavailable_letter(ch: char) -> bool {
    matches!(
        ch,
        'a' | 'A'
            | 'C'
            | 'e'
            | 'E'
            | 'f'
            | 'I'
            | 'O'
            | 'p'
            | 'P'
            | 'q'
            | 'Q'
            | 'r'
            | 'R'
            | 's'
            | 'S'
            | 't'
            | 'T'
            | 'w'
            | 'W'
            | 'x'
            | 'z'
            | 'Z'
            | '$'
    )
}

fn digit_value(key: Key, counting: bool) -> Option<u32> {
    let named = match key {
        Key::Key0 | Key::Numpad0 => Some(0),
        Key::Key1 => Some(1),
        Key::Key2 => Some(2),
        Key::Key3 => Some(3),
        Key::Key4 => Some(4),
        Key::Key5 => Some(5),
        Key::Key6 => Some(6),
        Key::Key7 => Some(7),
        Key::Key8 => Some(8),
        Key::Key9 => Some(9),
        _ => None,
    };
    if named.is_some() {
        return named;
    }
    if !counting {
        return None;
    }
    match key {
        Key::Numpad1 => Some(1),
        Key::Numpad2 => Some(2),
        Key::Numpad3 => Some(3),
        Key::Numpad4 => Some(4),
        Key::Numpad6 => Some(6),
        Key::Numpad7 => Some(7),
        Key::Numpad8 => Some(8),
        Key::Numpad9 => Some(9),
        _ => None,
    }
}

fn typed_letter(key: Key) -> Option<char> {
    match key {
        Key::Letter(ch) if ch.is_ascii_alphabetic() => Some(ch),
        Key::Left => Some('h'),
        Key::Down => Some('j'),
        Key::Up => Some('k'),
        Key::Right => Some('l'),
        Key::NorthWest => Some('y'),
        Key::NorthEast => Some('u'),
        Key::SouthWest => Some('b'),
        Key::SouthEast => Some('n'),
        _ => None,
    }
}

fn letter_index(ch: char) -> Option<usize> {
    if ch.is_ascii_lowercase() {
        Some((ch as u8 - b'a') as usize)
    } else if ch.is_ascii_uppercase() {
        Some(26 + (ch as u8 - b'A') as usize)
    } else {
        None
    }
}

fn plain_direction(key: Key) -> Option<Direction> {
    Some(match key {
        Key::Up | Key::Letter('k') => Direction::North,
        Key::Down | Key::Letter('j') => Direction::South,
        Key::Left | Key::Letter('h') => Direction::West,
        Key::Right | Key::Letter('l') => Direction::East,
        Key::NorthEast | Key::Letter('u') => Direction::NorthEast,
        Key::SouthEast | Key::Letter('n') => Direction::SouthEast,
        Key::SouthWest | Key::Letter('b') => Direction::SouthWest,
        Key::NorthWest | Key::Letter('y') => Direction::NorthWest,
        Key::Ascend => Direction::Up,
        Key::Descend => Direction::Down,
        Key::Numpad8 => Direction::North,
        Key::Numpad2 => Direction::South,
        Key::Numpad4 => Direction::West,
        Key::Numpad6 => Direction::East,
        Key::Numpad9 => Direction::NorthEast,
        Key::Numpad3 => Direction::SouthEast,
        Key::Numpad1 => Direction::SouthWest,
        Key::Numpad7 => Direction::NorthWest,
        _ => return None,
    })
}

fn shifted_run(key: Key) -> Option<Direction> {
    Some(match key {
        Key::RunNorth | Key::Letter('K') => Direction::North,
        Key::RunSouth | Key::Letter('J') => Direction::South,
        Key::RunEast | Key::Letter('L') => Direction::East,
        Key::RunWest | Key::Letter('H') => Direction::West,
        Key::RunNorthEast | Key::Letter('U') => Direction::NorthEast,
        Key::RunSouthEast | Key::Letter('N') => Direction::SouthEast,
        Key::RunSouthWest | Key::Letter('B') => Direction::SouthWest,
        Key::RunNorthWest | Key::Letter('Y') => Direction::NorthWest,
        _ => return None,
    })
}

fn key_line(key: Key) -> &'static str {
    match key {
        Key::Help => "?: list the commands this client implements. S does not save.",
        Key::KeyHelp => "&: describe the next key.",
        Key::Pickup => ",: pick up what is at your feet.",
        Key::Drop => "d: drop one stack. Digits set the quantity.",
        Key::DropMany => "D: drop several stacks.",
        Key::Fight | Key::Letter('F') => "F: fight the creature in a direction.",
        Key::OpenDoor => "o: open an adjacent door.",
        Key::CloseDoor => "c: close an adjacent door.",
        Key::Go => "g: run, picking up along the way.",
        Key::SuppressRun | Key::Letter('M') => "M: run without autopickup.",
        Key::Suppress | Key::Letter('m') => "m: one step, without a bump attack or autopickup.",
        Key::Inventory => "i: inventory.",
        Key::Look => ";: look around.",
        Key::Describe => ":: describe where you are standing.",
        Key::WhatIs => "/: name something you can see.",
        Key::Autopickup => "@: toggle autopickup for this session.",
        Key::Extended => "#: extended command. #adjust swaps inventory letters.",
        Key::Travel => "_: choose a travel destination.",
        Key::Wait | Key::Numpad5 => ".: wait.",
        Key::Space => "Space confirms a menu or the message log.",
        Key::Escape => "Esc cancels the current mode or travel.",
        Key::Quit => "F9 quits.",
        Key::Up | Key::Letter('k') => "k: move north.",
        Key::Down | Key::Letter('j') => "j: move south.",
        Key::Left | Key::Letter('h') => "h: move west.",
        Key::Right | Key::Letter('l') => "l: move east.",
        Key::NorthWest | Key::Letter('y') => "y: move northwest.",
        Key::NorthEast | Key::Letter('u') => "u: move northeast.",
        Key::SouthWest | Key::Letter('b') => "b: move southwest.",
        Key::SouthEast | Key::Letter('n') => "n: move southeast.",
        Key::Ascend => "<: go up.",
        Key::Descend => ">: go down.",
        Key::Control => "F3: request control.",
        Key::Release => "R: release control.",
        Key::Note => "F4: write a note.",
        Key::Places => "F5: remembered places.",
        Key::History => "F2: history.",
        Key::Scrollback => "Ctrl-P: earlier messages.",
        _ => "That key has no command in this client.",
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Axis {
    origin: i32,
    placed: bool,
    fitting: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct Viewport {
    x: Axis,
    y: Axis,
}

impl Viewport {
    fn clear(&mut self) {
        *self = Self::default();
    }
}

fn span(coords: impl Iterator<Item = i32>, focus: i32) -> (i32, i32) {
    let mut min = None;
    let mut max = None;
    for value in coords {
        min = Some(min.map_or(value, |current: i32| current.min(value)));
        max = Some(max.map_or(value, |current: i32| current.max(value)));
    }
    (min.unwrap_or(focus), max.unwrap_or(focus))
}

struct AxisWindow {
    length: i64,
    center: i64,
    margin: i64,
    margin_end: i64,
}

fn follow_axis(axis: &mut Axis, min: i32, max: i32, focus: i32, window: AxisWindow) {
    let AxisWindow {
        length: view,
        center,
        margin,
        margin_end,
    } = window;
    let length = i64::from(max) - i64::from(min) + 1;
    if length <= view {
        let pad = (view - length) / 2;
        if let Ok(origin) = i32::try_from(i64::from(min) - pad) {
            axis.origin = origin;
            axis.placed = true;
            axis.fitting = true;
        }
        return;
    }
    let was_fitting = axis.fitting;
    let placed = axis.placed;
    axis.fitting = false;
    if !placed || was_fitting {
        if let Ok(origin) = i32::try_from(i64::from(focus) - center) {
            axis.origin = origin;
            axis.placed = true;
        }
        return;
    }
    let screen = i64::from(focus) - i64::from(axis.origin);
    let delta = if screen > margin_end {
        screen - margin_end
    } else if screen < margin {
        screen - margin
    } else {
        0
    };
    if delta != 0 {
        if let Some(origin) = i64::from(axis.origin)
            .checked_add(delta)
            .and_then(|value| i32::try_from(value).ok())
        {
            axis.origin = origin;
        }
    }
}

pub fn history_text(entry: &HistoryEntry) -> String {
    match &entry.content {
        HistoryContent::PlaceRenamed { name, .. } => format!("Place named {}.", name),
        HistoryContent::Travel { .. } => "Travel requested.".into(),
        HistoryContent::Wizard { summary, .. } => summary.clone(),
        HistoryContent::Action { event, .. } => match event {
            Event::Moved { direction } => format!("Moved {direction:?}."),
            Event::Taken { item, quantity, .. } => {
                format!("Picked up {quantity} from item #{item}.")
            }
            Event::Dropped { item, quantity, .. } => {
                format!("Dropped {quantity} from item #{item}.")
            }
            Event::DoorChanged { open, .. } => {
                format!("{} door.", if *open { "Opened" } else { "Closed" })
            }
            Event::Waited => "Waited.".into(),
            Event::PreparationPaused => "Preparation paused.".into(),
            Event::AttackStarted { .. } => "Prepared an attack.".into(),
        },
        HistoryContent::Annotation { text, category, .. } => {
            format!("{:?} {:?}: {text}", entry.audience, category)
        }
    }
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    tor_client_hack::wrap(text, width)
}

pub fn history_lines(entries: &[HistoryEntry]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|e| {
            let mut lines = vec![
                format!("tick {}  {}", e.tick, e.id.0),
                format!("{:?} / {:?}", e.author, e.audience),
            ];
            lines.extend(wrap(&history_text(e), 66));
            lines.push(String::new());
            lines
        })
        .collect()
}
