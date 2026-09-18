use serde::{Deserialize, Serialize};
use tor_client_common::ClientState;
use tor_protocol::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Travel,
    Up,
    Down,
    Left,
    Right,
    Ascend,
    Descend,
    Wait,
    Pickup,
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
    pub travel_cursor: Option<Position>,
    pub role: AccessRole,
    pub state: Option<ClientState>,
    pub connected: bool,
    pub busy: bool,
    pub status: String,
    pub note: Option<NoteDraft>,
    pub pickup: Vec<ItemView>,
    pub selected: usize,
    pub history_page: Option<HistoryPage>,
    pub history_scroll: usize,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            travel_cursor: None,
            role: AccessRole::Spectator,
            state: None,
            connected: false,
            busy: true,
            status: "Connecting to the local server...".into(),
            note: None,
            pickup: vec![],
            selected: 0,
            history_page: None,
            history_scroll: 0,
        }
    }

    pub fn set_state(&mut self, state: ClientState) {
        if self
            .state
            .as_ref()
            .is_some_and(|old| old.branch() != state.branch())
        {
            self.travel_cursor = None;
            self.note = None;
            self.pickup.clear();
            self.history_page = None;
            self.history_scroll = 0;
            self.status = "Timeline changed; pending selections cleared.".into();
        }
        // Do not let an old selection silently target a changed observation.
        if self
            .state
            .as_ref()
            .is_some_and(|old| old.state().revision != state.state().revision)
        {
            self.pickup.clear();
            self.travel_cursor = None;
        }
        if !state.has_control() {
            self.travel_cursor = None;
        }
        self.connected = true;
        self.state = Some(state);
    }

    pub fn ready(&mut self) {
        self.busy = false;
    }

    pub fn disconnect(&mut self, message: String) {
        self.travel_cursor = None;
        self.connected = false;
        self.busy = false;
        self.note = None;
        self.pickup.clear();
        self.status = message;
    }

    pub fn input(&mut self, input: Input) -> Effect {
        if let Input::Key { key: Key::Escape } = input {
            if self.travel_cursor.take().is_some() {
                self.status = "Travel selection cancelled.".into();
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
            {
                self.pickup.clear();
                self.status = "Cancelled.".into();
                return Effect::None;
            }
            return Effect::Quit;
        }
        if !self.connected || self.busy {
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
            if self.history_page.is_some() || !self.pickup.is_empty() {
                return Effect::None;
            }
            if let Some(position) = self
                .state
                .as_ref()
                .and_then(|s| crate::render::cell_at(&s.state().observation, x, y))
            {
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
            && !matches!(key, Key::History | Key::OlderHistory | Key::RecentHistory)
        {
            self.status = "Spectator access is read-only.".into();
            return Effect::None;
        }
        if !self.pickup.is_empty() {
            match key {
                Key::Up => self.selected = self.selected.saturating_sub(1),
                Key::Down => self.selected = (self.selected + 1).min(self.pickup.len() - 1),
                Key::Enter => {
                    let item = self.pickup[self.selected].id;
                    self.pickup.clear();
                    return self.act(Action::Take { item });
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
                Key::Ascend => cursor.z += 1,
                Key::Descend => cursor.z -= 1,
                Key::Enter => return self.travel_to(cursor),
                _ => return Effect::None,
            }
            cursor.x = cursor.x.clamp(-16, 16);
            cursor.y = cursor.y.clamp(-16, 16);
            cursor.z = cursor.z.clamp(-16, 16);
            self.travel_cursor = Some(cursor);
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
                        "Travel: arrows/HJKL select, U/D height, Enter confirms, Esc cancels."
                            .into();
                } else {
                    self.status = "Acquire control before travelling.".into();
                }
                Effect::None
            }
            Key::Up => self.act(Action::Move {
                direction: Direction::North,
            }),
            Key::Down => self.act(Action::Move {
                direction: Direction::South,
            }),
            Key::Left => self.act(Action::Move {
                direction: Direction::West,
            }),
            Key::Right => self.act(Action::Move {
                direction: Direction::East,
            }),
            Key::Ascend => self.act(Action::Move {
                direction: Direction::Up,
            }),
            Key::Descend => self.act(Action::Move {
                direction: Direction::Down,
            }),
            Key::Wait => self.act(Action::Wait),
            Key::Control => self.request(Request::AcquireControl),
            Key::Release => self.request(Request::ReleaseControl),
            Key::Pickup => {
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
                items.sort_by_key(|item| item.id);
                items.dedup_by_key(|item| item.id);
                match items.as_slice() {
                    [] => {
                        self.status = "There is nothing at your feet to pick up.".into();
                        Effect::None
                    }
                    [item] => self.act(Action::Take { item: item.id }),
                    _ => {
                        self.pickup = items;
                        self.selected = 0;
                        self.status = "Choose an item with Up/Down, then Enter.".into();
                        Effect::None
                    }
                }
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
        let Some(cell) = state
            .state()
            .observation
            .visible_cells
            .iter()
            .find(|c| c.position == position && !c.wall)
        else {
            self.status = "Select a visible floor cell.".into();
            return Effect::None;
        };
        let command = Command::Travel {
            expected_revision: state.state().revision,
            destination: cell.key.clone(),
        };
        self.travel_cursor = None;
        self.command(command)
    }

    fn act(&mut self, action: Action) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() {
            self.status = "You are observing. Press C to request control.".into();
            return Effect::None;
        }
        if !state.state().observation.ready {
            self.status = "Waiting for another actor to act.".into();
            return Effect::None;
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

/// Render only cells disclosed in the observer's scene.
pub fn glyph_at(o: &Observation, x: i32, y: i32) -> char {
    glyph_at_level(o, x, y, 0)
}

pub fn glyph_at_level(o: &Observation, x: i32, y: i32, z: i32) -> char {
    let position = Position { x, y, z };
    let Some(cell) = o
        .visible_cells
        .iter()
        .find(|cell| cell.position == position)
    else {
        return ' ';
    };
    if cell.wall {
        return '#';
    }
    if position == o.position {
        return '@';
    }
    if o.visible_actors.iter().any(|a| a.position == position) {
        return '&';
    }
    if o.ground_items.iter().any(|i| i.position == position) {
        return '!';
    }
    if cell.stairs_up {
        return '<';
    }
    if cell.stairs_down {
        return '>';
    }
    '.'
}

pub fn history_text(entry: &HistoryEntry) -> String {
    match &entry.content {
        HistoryContent::Travel { .. } => "Travel requested.".into(),
        HistoryContent::Wizard { summary, .. } => summary.clone(),
        HistoryContent::Action { event, .. } => match event {
            Event::Moved { direction } => format!("Moved {direction:?}."),
            Event::Taken { item } => format!("Picked up item #{item}."),
            Event::Waited => "Waited.".into(),
        },
        HistoryContent::Annotation { text, category, .. } => {
            format!("{:?} {:?}: {text}", entry.audience, category)
        }
    }
}

pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let chars: Vec<char> = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    chars
        .chunks(width.max(1))
        .map(|line| line.iter().collect())
        .collect()
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
