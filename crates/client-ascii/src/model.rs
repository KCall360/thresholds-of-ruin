use serde::{Deserialize, Serialize};
use tor_client_common::ClientState;
use tor_protocol::*;

pub use tor_client_hack::{column_glyph, BumpAttacks, Click, SessionConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Attack,
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
    Request(Request),
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
        self.follow();
    }

    /// Apply every disclosed boundary, even when several updates share a frame.
    pub fn update(&mut self, update: StreamUpdate) -> Result<(), tor_client_common::StreamError> {
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
        self.follow();
        self.append_logged(appended);
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
        self.state_changed(old);
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
        let cursor = self.travel_cursor;
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
    }

    pub fn accept_status(&mut self, status: String) {
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
                self.log.acknowledge();
                if matches!(key, Key::Space | Key::Enter | Key::Escape) {
                    return None;
                }
                if key == Key::Scrollback {
                    self.log.open_scrollback();
                    return None;
                }
                return Some(Input::Key { key });
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
        self.status = message;
    }

    pub fn input(&mut self, input: Input) -> Effect {
        let Some(input) = self.begin_input(input) else {
            return Effect::None;
        };
        if !self.attack_targets.is_empty() {
            match input {
                Input::Key { key: Key::Escape } => {
                    self.attack_targets.clear();
                    return Effect::None;
                }
                Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
                Input::Key { key: Key::Down } => {
                    self.selected = (self.selected + 1).min(self.attack_targets.len() - 1)
                }
                Input::Key { key: Key::Enter } => {
                    let target = self.attack_targets[self.selected].id;
                    self.attack_targets.clear();
                    return self.act(Action::Attack { target });
                }
                _ => {}
            }
            self.status = format!(
                "Attack {}? Up/Down select, Enter confirms, Esc cancels.",
                self.attack_targets[self.selected].name
            );
            return Effect::None;
        }
        if !self.pickup.is_empty() {
            if let Input::Text { text } = &input {
                for ch in text.chars().filter(char::is_ascii_digit) {
                    // Keep one overflow digit so an oversized request cannot
                    // silently become a smaller valid quantity.
                    if self.quantity.len() < 21 {
                        self.quantity.push(ch);
                    }
                }
                return Effect::None;
            }
        }
        if let Input::Key { key: Key::Escape } = input {
            if self.place_name.take().is_some() {
                return Effect::None;
            }
            if self.places_open {
                self.places_open = false;
                return Effect::None;
            }
            if self.travel_cursor.take().is_some() {
                self.status = "Travel selection cancelled.".into();
                self.follow();
                return Effect::None;
            }
            if self.connected
                && self.state.as_ref().is_some_and(|s| {
                    s.has_control() && s.travel().is_some_and(|t| t.phase == TravelPhase::Active)
                })
            {
                if self.busy {
                    return Effect::None;
                }
                let state = self.state.as_ref().expect("attached");
                return self.request(Request::CancelTravel {
                    branch: state.branch().clone(),
                    travel_id: state.travel().expect("active travel").id.clone(),
                });
            }
            if self.note.take().is_some()
                || self.history_page.take().is_some()
                || !self.pickup.is_empty()
                || self.door_direction.is_some()
            {
                self.pickup.clear();
                self.attack_targets.clear();
                self.door_direction = None;
                self.status = "Cancelled.".into();
                return Effect::None;
            }
            return Effect::Quit;
        }
        if !self.connected || self.busy {
            return Effect::None;
        }
        if self.places_open {
            if let Some(name) = &mut self.place_name {
                match input {
                    Input::Text { text } => {
                        for ch in text.chars().filter(|ch| !ch.is_control()) {
                            if name.len() + ch.len_utf8() <= 80 {
                                name.push(ch);
                            }
                        }
                    }
                    Input::Key {
                        key: Key::Backspace,
                    } => {
                        name.pop();
                    }
                    Input::Key { key: Key::Enter } if !name.trim().is_empty() => {
                        let name = name.trim().to_owned();
                        self.place_name = None;
                        let state = self.state.as_ref().expect("attached");
                        if self.role == AccessRole::Spectator || !state.has_control() {
                            self.status = "Naming places requires control.".into();
                            return Effect::None;
                        }
                        let Some(place) = state.state().observation.places.get(self.place_selected)
                        else {
                            return Effect::None;
                        };
                        return self.command(Command::RenamePlace {
                            expected_revision: state.state().revision,
                            key: place.key.clone(),
                            name,
                        });
                    }
                    _ => {}
                }
            } else {
                let state = self.state.as_ref().expect("attached");
                let count = state.state().observation.places.len();
                match input {
                    Input::Key { key: Key::Up } => {
                        self.place_selected = self.place_selected.saturating_sub(1)
                    }
                    Input::Key { key: Key::Down } => {
                        self.place_selected = (self.place_selected + 1).min(count.saturating_sub(1))
                    }
                    Input::Key { key: Key::Enter } if count > 0 => {
                        if self.role == AccessRole::Spectator || !state.has_control() {
                            self.status = "Naming places requires control.".into();
                        } else {
                            self.place_name = Some(String::new());
                        }
                    }
                    _ => {}
                }
            }
            return Effect::None;
        }
        if let Some(draft) = &mut self.note {
            match input {
                Input::Text { text } => {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        if draft.text.len() + ch.len_utf8() <= MAX_NOTE_BYTES {
                            draft.text.push(ch);
                        }
                    }
                }
                Input::Key {
                    key: Key::Backspace,
                } => {
                    draft.text.pop();
                }
                Input::Key { key: Key::Tab } => {
                    draft.audience = if draft.audience == Audience::Private {
                        Audience::Actor
                    } else {
                        Audience::Private
                    }
                }
                Input::Key { key: Key::Enter } if !draft.text.trim().is_empty() => {
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
                    return self.command(command);
                }
                _ => {}
            }
            return Effect::None;
        }
        if let Input::Click { x, y } = input {
            if self.history_page.is_some()
                || !self.pickup.is_empty()
                || self.door_direction.is_some()
                || self.places_open
                || self.note.is_some()
            {
                return Effect::None;
            }
            if self.config.click == Click::Look {
                return Effect::None;
            }
            if let Some(position) = crate::render::cell_at(self, x, y) {
                return self.travel_to(position);
            }
            return Effect::None;
        }
        let Input::Key { key } = input else {
            return Effect::None;
        };
        if let Some(page) = &self.history_page {
            match key {
                Key::Up => {
                    self.history_scroll = self.history_scroll.saturating_sub(1);
                    return Effect::None;
                }
                Key::Down => {
                    self.history_scroll = (self.history_scroll + 1)
                        .min(history_lines(&page.entries).len().saturating_sub(20));
                    return Effect::None;
                }
                Key::OlderHistory | Key::RecentHistory | Key::History => {}
                _ => return Effect::None,
            }
        }
        if self.role == AccessRole::Spectator
            && !matches!(
                key,
                Key::Places | Key::History | Key::OlderHistory | Key::RecentHistory
            )
        {
            self.status = "Spectator access is read-only.".into();
            return Effect::None;
        }
        if let Some(open) = self.door_direction {
            let Some(state) = &self.state else {
                return Effect::None;
            };
            let observation = &state.state().observation;
            let mut target = observation.position;
            match key {
                Key::Up => target.y -= 1,
                Key::Down => target.y += 1,
                Key::Left => target.x -= 1,
                Key::Right => target.x += 1,
                Key::NorthEast => {
                    target.x += 1;
                    target.y += -1;
                }
                Key::SouthEast => {
                    target.x += 1;
                    target.y += 1;
                }
                Key::SouthWest => {
                    target.x += -1;
                    target.y += 1;
                }
                Key::NorthWest => {
                    target.x += -1;
                    target.y += -1;
                }

                Key::Ascend => target.z += 1,
                Key::Descend => target.z -= 1,
                _ => return Effect::None,
            }
            let door = observation
                .visible_cells
                .iter()
                .find(|cell| cell.position == target)
                .and_then(|cell| cell.door.as_ref())
                .filter(|door| door.reachable)
                .map(|door| door.id);
            self.door_direction = None;
            if let Some(door) = door {
                return self.act(Action::SetDoor { door, open });
            }
            self.status = "There is no door in that direction.".into();
            return Effect::None;
        }
        if !self.pickup.is_empty() {
            match key {
                Key::Backspace => {
                    self.quantity.pop();
                }
                Key::Up => self.selected = self.selected.saturating_sub(1),
                Key::Down => self.selected = (self.selected + 1).min(self.pickup.len() - 1),
                Key::Enter => {
                    let item = self.pickup[self.selected].id;
                    let quantity = if self.quantity.is_empty() {
                        None
                    } else {
                        match self.quantity.parse::<u64>() {
                            Ok(q) if q > 0 => Some(q),
                            _ => {
                                self.status = "Quantity must be a positive integer.".into();
                                return Effect::None;
                            }
                        }
                    };
                    self.pickup.clear();
                    self.attack_targets.clear();
                    self.door_direction = None;
                    return self.act(if self.dropping {
                        Action::Drop { item, quantity }
                    } else {
                        Action::Take { item, quantity }
                    });
                }
                _ => {}
            }
            return Effect::None;
        }
        if let Some(mut cursor) = self.travel_cursor {
            match key {
                Key::Up => cursor.y -= 1,
                Key::Down => cursor.y += 1,
                Key::Left => cursor.x -= 1,
                Key::Right => cursor.x += 1,
                Key::NorthEast => {
                    cursor.x += 1;
                    cursor.y += -1;
                }
                Key::SouthEast => {
                    cursor.x += 1;
                    cursor.y += 1;
                }
                Key::SouthWest => {
                    cursor.x += -1;
                    cursor.y += 1;
                }
                Key::NorthWest => {
                    cursor.x += -1;
                    cursor.y += -1;
                }

                Key::Ascend => cursor.z += 1,
                Key::Descend => cursor.z -= 1,
                Key::Enter => return self.travel_to(cursor),
                _ => return Effect::None,
            }
            cursor.x = cursor.x.clamp(-16, 16);
            cursor.y = cursor.y.clamp(-16, 16);
            cursor.z = cursor.z.clamp(-16, 16);
            self.travel_cursor = Some(cursor);
            self.follow();
            return Effect::None;
        }
        match key {
            Key::Travel => {
                if self
                    .state
                    .as_ref()
                    .is_some_and(|s| s.travel().is_some_and(|t| t.phase == TravelPhase::Active))
                {
                    self.status =
                        "Press Esc to cancel travel before selecting another destination.".into();
                    return Effect::None;
                }
                if let Some(state) = self.state.as_ref().filter(|s| s.has_control()) {
                    self.travel_cursor = Some(state.state().observation.position);
                    self.status =
                        "Travel: HJKL/YUBN select, </> height, Enter confirms, Esc cancels.".into();
                    self.follow();
                } else {
                    self.status = "Acquire control before travelling.".into();
                }
                Effect::None
            }
            Key::Up => self.act(Action::Move {
                direction: Direction::North,
            }),
            Key::Attack => {
                if let Some(state) = &self.state {
                    self.attack_targets = state
                        .state()
                        .observation
                        .visible_actors
                        .iter()
                        .filter(|a| a.id != state.state().observation.actor)
                        .cloned()
                        .collect();
                    self.attack_targets.sort_by_key(|a| a.id);
                    self.attack_targets.dedup_by_key(|a| a.id);
                    self.selected = 0;
                    self.status = self.attack_targets.first().map_or_else(
                        || "No target is visible.".into(),
                        |a| {
                            format!(
                                "Attack {}? Up/Down select, Enter confirms, Esc cancels.",
                                a.name
                            )
                        },
                    );
                }
                Effect::None
            }
            Key::Down => self.act(Action::Move {
                direction: Direction::South,
            }),
            Key::Left => self.act(Action::Move {
                direction: Direction::West,
            }),
            Key::Right => self.act(Action::Move {
                direction: Direction::East,
            }),
            Key::NorthEast => self.act(Action::Move {
                direction: Direction::NorthEast,
            }),
            Key::SouthEast => self.act(Action::Move {
                direction: Direction::SouthEast,
            }),
            Key::SouthWest => self.act(Action::Move {
                direction: Direction::SouthWest,
            }),
            Key::NorthWest => self.act(Action::Move {
                direction: Direction::NorthWest,
            }),
            Key::Ascend => self.act(Action::Move {
                direction: Direction::Up,
            }),
            Key::Descend => self.act(Action::Move {
                direction: Direction::Down,
            }),
            Key::Wait | Key::Space => self.act(Action::Wait),
            Key::Control => self.request(Request::AcquireControl),
            Key::Release => self.request(Request::ReleaseControl),
            Key::OpenDoor | Key::CloseDoor => {
                let Some(state) = &self.state else {
                    return Effect::None;
                };
                if !state.has_control() {
                    self.attack_targets.clear();
                    self.place_name = None;
                    self.status = "You are observing. Press F3 to request control.".into();
                } else if !state.state().observation.ready {
                    self.status = "Waiting for another actor to act.".into();
                } else if state
                    .travel()
                    .is_some_and(|t| t.phase == TravelPhase::Active)
                {
                    self.status = "Press Esc to cancel travel before using a door.".into();
                } else {
                    let open = key == Key::OpenDoor;
                    self.door_direction = Some(open);
                    self.status = format!(
                        "{} in which direction? HJKL/YUBN; Esc cancels.",
                        if open { "Open" } else { "Close" }
                    );
                }
                Effect::None
            }
            Key::Pickup | Key::Drop => {
                self.dropping = key == Key::Drop;
                self.quantity.clear();
                let Some(state) = &self.state else {
                    return Effect::None;
                };
                let o = &state.state().observation;
                let mut items: Vec<_> = o
                    .ground_items
                    .iter()
                    .filter(|i| i.reachable)
                    .map(|i| i.item.clone())
                    .collect();
                if self.dropping {
                    items = o.inventory.clone();
                }
                items.sort_by_key(|item| item.id);
                items.dedup_by_key(|item| item.id);
                match items.as_slice() {
                    [] => {
                        self.status = if self.dropping {
                            "Your inventory is empty."
                        } else {
                            "There is nothing at your feet to pick up."
                        }
                        .into();
                        Effect::None
                    }
                    [item] if item.quantity == 1 => self.act(if self.dropping {
                        Action::Drop {
                            item: item.id,
                            quantity: None,
                        }
                    } else {
                        Action::Take {
                            item: item.id,
                            quantity: None,
                        }
                    }),
                    _ => {
                        self.pickup = items;
                        self.selected = 0;
                        self.status =
                            "Up/Down select; type quantity (blank = all); Enter confirms.".into();
                        Effect::None
                    }
                }
            }
            Key::Places => {
                self.places_open = true;
                self.place_selected = 0;
                Effect::None
            }
            Key::Note => {
                if let Some(state) = &self.state {
                    self.note = Some(NoteDraft {
                        text: String::new(),
                        audience: Audience::Private,
                        revision: state.state().revision,
                    });
                }
                Effect::None
            }
            Key::History => self.request(Request::History {
                before: None,
                limit: 50,
            }),
            Key::OlderHistory => {
                let cursor = self
                    .history_page
                    .as_ref()
                    .map(|p| p.older_before.clone())
                    .unwrap_or_else(|| self.state.as_ref().and_then(|s| s.older_before().cloned()));
                if let Some(before) = cursor {
                    self.request(Request::History {
                        before: Some(before),
                        limit: 50,
                    })
                } else {
                    self.status = "No older history entries.".into();
                    Effect::None
                }
            }
            Key::RecentHistory => {
                self.history_page = None;
                Effect::None
            }
            _ => Effect::None,
        }
    }

    fn travel_to(&mut self, position: Position) -> Effect {
        let Some(state) = self.state.as_ref() else {
            return Effect::None;
        };
        if self.role == AccessRole::Spectator
            || !state.has_control()
            || !state.state().observation.ready
        {
            self.status = "Travel requires control of a ready actor.".into();
            return Effect::None;
        }
        let Some(cell) =
            state.state().observation.visible_cells.iter().find(|c| {
                c.position == position && !c.wall && c.door.as_ref().is_none_or(|d| d.open)
            })
        else {
            self.status = "Select a visible floor cell.".into();
            return Effect::None;
        };
        let command = Command::Travel {
            expected_revision: state.state().revision,
            destination: cell.key.clone(),
        };
        self.travel_cursor = None;
        self.follow();
        self.command(command)
    }

    fn act(&mut self, mut action: Action) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() {
            self.attack_targets.clear();
            self.place_name = None;
            self.door_direction = None;
            self.status = "You are observing. Press F3 to request control.".into();
            return Effect::None;
        }
        if !state.state().observation.ready {
            if matches!(action, Action::Wait)
                && state
                    .state()
                    .observation
                    .combat
                    .as_ref()
                    .is_some_and(|c| !c.terminal)
            {
                return self.request(Request::Continue);
            }
            self.status = "Waiting for another actor to act.".into();
            return Effect::None;
        }
        if let Action::Move { direction } = action {
            let (x, y, z) = match direction {
                Direction::North => (0, -1, 0),
                Direction::South => (0, 1, 0),
                Direction::East => (1, 0, 0),
                Direction::West => (-1, 0, 0),
                Direction::NorthEast => (1, -1, 0),
                Direction::SouthEast => (1, 1, 0),
                Direction::SouthWest => (-1, 1, 0),
                Direction::NorthWest => (-1, -1, 0),
                Direction::Up => (0, 0, 1),
                Direction::Down => (0, 0, -1),
            };
            let view = &state.state().observation;
            if let Some(target) = view.visible_actors.iter().find(|a| {
                a.id != view.actor
                    && a.position == (Position { x, y, z })
                    && match self.config.bump_attacks {
                        BumpAttacks::Any => true,
                        BumpAttacks::Off => false,
                        BumpAttacks::Hostile => view.combat.as_ref().is_some_and(|c| {
                            c.actors
                                .iter()
                                .any(|other| other.actor == a.id && other.hostile)
                        }),
                    }
            }) {
                action = Action::Attack { target: target.id };
            }
        }
        self.command(Command::Act {
            expected_revision: state.state().revision,
            action,
        })
    }

    fn command(&mut self, command: Command) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        self.request(Request::Command {
            branch: state.branch().clone(),
            command,
        })
    }

    fn request(&mut self, request: Request) -> Effect {
        self.busy = true;
        self.status = "Waiting for server...".into();
        Effect::Request(request)
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
