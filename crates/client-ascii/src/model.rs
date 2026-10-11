use serde::{Deserialize, Serialize};
use tor_client_common::items::{item_action, item_choices, ItemOperation};
use tor_client_common::ClientState;
use tor_protocol::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Attack,
    Abilities,
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
    Pickup,
    Drop,
    Equip,
    Unequip,
    Drink,
    OpenDoor,
    CloseDoor,
    Control,
    Release,
    ResumeIntention,
    CancelIntention,
    Note,
    /// Open the authenticated wizard command editor.
    Wizard,
    Enter,
    Escape,
    Backspace,
    Tab,
    History,
    OlderHistory,
    RecentHistory,
    /// Show journey steps more slowly.
    Slower,
    /// Show journey steps more quickly.
    Faster,
    /// Open the log of earlier messages.
    MessageLog,
    /// Show everything carried.
    Inventory,
    /// Inspect the attached actor's disclosed creature stats.
    Stats,
    /// Describe a map cell without taking a turn.
    Look,
    /// Show commands and map symbols.
    Help,
    /// Keep moving in a direction until something interesting happens.
    RunUp,
    RunDown,
    RunLeft,
    RunRight,
    RunNorthEast,
    RunSouthEast,
    RunSouthWest,
    RunNorthWest,
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
    /// Show the rest of a journey without waiting.
    Skip,
    /// Space shown updates this many milliseconds apart.
    Pace(u64),
}

/// The step delays [`Key::Slower`] and [`Key::Faster`] move between.
pub const PACES_MS: [u64; 9] = [0, 25, 50, 75, 100, 150, 200, 300, 500];

/// The default delay between shown journey steps.
pub const DEFAULT_PACE_MS: u64 = 75;

pub struct NoteDraft {
    pub text: String,
    pub audience: Audience,
    revision: u64,
}

pub struct App {
    pub bump_attacks: BumpAttacks,
    pub pace_ms: u64,
    pub attack_targets: Vec<ActorView>,
    pub ability_choices: Vec<Ability>,
    pub selected_ability: Option<Ability>,
    /// The look cursor, while choosing a cell to describe.
    pub look_cursor: Option<Position>,
    pub inventory_open: bool,
    pub help_open: bool,
    pub stats_open: bool,
    inspection: Option<Vec<String>>,
    pub stats_scroll: usize,
    /// The end-of-run screen was closed.
    pub end_dismissed: bool,
    /// The direction being run in, between steps.
    pub running: Option<Direction>,
    /// Inventory letters, kept for as long as an item is carried.
    pub letters: std::collections::BTreeMap<ItemTarget, char>,
    /// The last request sent was a one-cell move, so a refusal means the
    /// way is blocked.
    last_move: bool,
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
    pub wizard_command: Option<String>,
    pub pickup: Vec<ItemView>,
    pub dropping: bool,
    pub item_operation: Option<ItemOperation>,
    pub quantity: String,
    pub door_direction: Option<bool>,
    pub selected: usize,
    pub history_page: Option<HistoryPage>,
    pub history_scroll: usize,
    /// What happened since the player last acted, and earlier turns.
    pub messages: crate::messages::Messages,
    /// Rows scrolled back from the newest in the message log, when it's open.
    pub message_log: Option<usize>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            bump_attacks: BumpAttacks::Hostile,
            pace_ms: DEFAULT_PACE_MS,
            attack_targets: Vec::new(),
            ability_choices: Vec::new(),
            selected_ability: None,
            look_cursor: None,
            inventory_open: false,
            help_open: false,
            stats_open: false,
            inspection: None,
            stats_scroll: 0,
            end_dismissed: false,
            running: None,
            letters: Default::default(),
            last_move: false,
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
            wizard_command: None,
            pickup: vec![],
            dropping: false,
            item_operation: None,
            quantity: String::new(),
            door_direction: None,
            selected: 0,
            history_page: None,
            history_scroll: 0,
            messages: Default::default(),
            message_log: None,
        }
    }

    pub fn set_state(&mut self, state: ClientState) {
        let old = self
            .state
            .as_ref()
            .map(|s| (s.branch().clone(), s.state().revision));
        self.state = Some(state);
        self.state_changed(old);
    }

    /// Apply every disclosed boundary, even when several updates share a frame.
    pub fn update(&mut self, update: StreamUpdate) -> Result<(), tor_client_common::StreamError> {
        let phase = match &update.body {
            UpdateBody::Intention { status } => Some(status.phase),
            _ => None,
        };
        let observed = matches!(
            update.body,
            UpdateBody::Observation { .. } | UpdateBody::ObservationDelta { .. }
        );
        let travel = match &update.body {
            UpdateBody::Travel { status, .. } => Some(status.phase),
            _ => None,
        };
        let state = self
            .state
            .as_mut()
            .ok_or(tor_client_common::StreamError::InconsistentState)?;
        let old = Some((state.branch().clone(), state.state().revision));
        state.apply(update)?;
        if observed {
            let lines = state.narration_lines();
            // A spectator never sends commands; the followed actor's next
            // action starts its turn instead.
            if self.role == AccessRole::Spectator
                && lines
                    .iter()
                    .any(|line| matches!(line.topic, tor_client_common::narration::Topic::Own(_)))
            {
                self.messages.begin_turn();
            }
            self.messages.absorb(lines, &state.state().observation);
        }
        if let Some(reason) = travel.and_then(travel_stop) {
            self.messages.push(reason);
        }
        self.state_changed(old);
        if self.role == AccessRole::Player {
            if let Some(phase) = phase {
                self.status = phase_status(phase).into();
            }
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
        if let Some(state) = &mut self.state {
            state.replace_snapshot(snapshot)?;
        } else {
            self.state = Some(ClientState::from_snapshot(snapshot)?);
        }
        self.inspection = None;
        self.wizard_command = None;
        self.stats_open = false;
        self.state_changed(old);
        Ok(())
    }

    fn state_changed(&mut self, old: Option<(BranchId, u64)>) {
        let state = self.state.as_ref().expect("validated state");
        if old
            .as_ref()
            .is_some_and(|(branch, _)| branch != state.branch())
        {
            self.travel_cursor = None;
            self.note = None;
            self.pickup.clear();
            self.item_operation = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.selected_ability = None;
            self.door_direction = None;
            self.places_open = false;
            self.place_name = None;
            self.history_page = None;
            self.history_scroll = 0;
            self.look_cursor = None;
            self.running = None;
            self.end_dismissed = false;
            self.messages.begin_turn();
            self.messages.reset_sightings();
            self.message_log = None;
            self.status = "Timeline changed; pending selections cleared.".into();
        }
        if old.is_some_and(|(_, revision)| revision != state.state().revision) {
            self.place_name = None;
            self.place_selected = self
                .place_selected
                .min(state.state().observation.places.len().saturating_sub(1));
            self.pickup.clear();
            self.item_operation = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.selected_ability = None;
            self.door_direction = None;
            self.travel_cursor = None;
        }
        if !state.has_control() {
            self.pickup.clear();
            self.item_operation = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.selected_ability = None;
            self.place_name = None;
            self.door_direction = None;
            self.travel_cursor = None;
        }
        let carried: std::collections::BTreeSet<_> = state
            .state()
            .observation
            .inventory
            .iter()
            .map(|item| item.id)
            .collect();
        self.letters.retain(|item, _| carried.contains(item));
        for item in &state.state().observation.inventory {
            if !self.letters.contains_key(&item.id) {
                let used: std::collections::BTreeSet<_> = self.letters.values().copied().collect();
                if let Some(letter) = ('a'..='z')
                    .chain('A'..='Z')
                    .find(|letter| !used.contains(letter))
                {
                    self.letters.insert(item.id, letter);
                }
            }
        }
        self.connected = true;
    }

    pub fn ready(&mut self) {
        self.busy = false;
    }

    /// The server refused a request: stop any run and say why, in terms of
    /// what the player tried.
    pub fn refused(&mut self, status: String) {
        self.running = None;
        self.status = if self.last_move && status == "You can't do that now." {
            "You can't go that way.".into()
        } else {
            status
        };
    }

    /// Whether the column one step in a direction is open floor or stairs.
    fn open_ahead(&self, direction: Direction) -> bool {
        let Some(state) = &self.state else {
            return false;
        };
        let o = &state.state().observation;
        let (dx, dy) = match direction {
            Direction::North => (0, -1),
            Direction::South => (0, 1),
            Direction::East => (1, 0),
            Direction::West => (-1, 0),
            Direction::NorthEast => (1, -1),
            Direction::SouthEast => (1, 1),
            Direction::SouthWest => (-1, 1),
            Direction::NorthWest => (-1, -1),
            Direction::Up | Direction::Down => return false,
        };
        crate::render::map_tiles(state).into_iter().any(|t| {
            (t.position.x, t.position.y) == (o.position.x + dx, o.position.y + dy)
                && matches!(
                    t.kind,
                    crate::map::Kind::Floor
                        | crate::map::Kind::StairsUp
                        | crate::map::Kind::StairsDown
                        | crate::map::Kind::Item
                )
        })
    }

    /// A menu row's letter: an item's inventory letter when choosing from
    /// what's carried, otherwise a, b, c... down the list.
    pub fn choice_letter(&self, index: usize) -> char {
        let carried = self.dropping || self.item_operation.is_some();
        self.pickup
            .get(index)
            .and_then(|item| {
                carried
                    .then(|| self.letters.get(&item.id).copied())
                    .flatten()
            })
            .unwrap_or_else(|| ('a'..='z').chain('A'..='Z').nth(index).unwrap_or('?'))
    }

    /// Whether another creature is in sight, which stops a run.
    pub fn creature_in_view(&self) -> bool {
        self.state.as_ref().is_some_and(|state| {
            let o = &state.state().observation;
            o.visible_actors.iter().any(|a| a.id != o.self_target)
        })
    }

    /// Take the next step of a run, or end it when something interesting
    /// happened: a message, a creature in sight, or the way ahead not open.
    pub fn continue_run(&mut self) -> Effect {
        let Some(direction) = self.running else {
            return Effect::None;
        };
        let Some(state) = &self.state else {
            self.running = None;
            return Effect::None;
        };
        if ended(state) {
            self.running = None;
            return Effect::None;
        }
        if self.busy || !state.can_admit_intention() {
            return Effect::None;
        }
        if !self.messages.text().is_empty()
            || self.creature_in_view()
            || !self.open_ahead(direction)
        {
            self.running = None;
            return Effect::None;
        }
        self.act(Action::Move { direction })
    }

    pub fn intention_hint(&self) -> Option<String> {
        if self.role == AccessRole::Spectator {
            return None;
        }
        let state = self.state.as_ref()?;
        let pending = state.intention_for_input()?;
        let label = match pending.phase {
            IntentionPhase::Paused => "Attack paused.",
            IntentionPhase::Started => "Attack in progress.",
            IntentionPhase::Suspended => "Action suspended.",
            _ => "Action queued.",
        };
        let mut hint = String::from(label);
        if state.can_resume_intention() {
            hint.push_str(" F8 resume;");
        }
        if state.can_cancel_intention() {
            hint.push_str(" F9 cancel;");
        }
        hint.push_str(if state.has_control() {
            " F3/R control; F2 history; Esc quit"
        } else {
            " F3 acquire control; F2 history; Esc quit"
        });
        Some(hint)
    }

    pub fn disconnect(&mut self, message: String) {
        self.travel_cursor = None;
        self.place_name = None;
        self.places_open = false;
        self.connected = false;
        self.busy = false;
        self.inspection = None;
        self.wizard_command = None;
        self.stats_open = false;
        self.note = None;
        self.pickup.clear();
        self.item_operation = None;
        self.attack_targets.clear();
        self.ability_choices.clear();
        self.selected_ability = None;
        self.door_direction = None;
        self.status = message;
    }

    pub fn show_inspection(&mut self, report: &CreatureInspectionView) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Creature inspection requires wizard authority".into());
        }
        let rows = tor_client_common::inspection::lines(report)
            .map_err(|error| error.to_string())?
            .into_iter()
            .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
            .collect();
        self.inspection = Some(rows);
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn show_combat_diagnostics(
        &mut self,
        report: &tor_protocol::CombatDiagnosticsView,
    ) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Combat diagnostics require wizard authority".into());
        }
        let rows = tor_client_common::combat_diagnostics::lines(report)
            .map_err(|error| error.to_string())?
            .into_iter()
            .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
            .collect();
        self.inspection = Some(rows);
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn has_inspection(&self) -> bool {
        self.inspection.is_some()
    }

    pub fn stats_rows(&self) -> std::borrow::Cow<'_, [String]> {
        if let Some(rows) = &self.inspection {
            return std::borrow::Cow::Borrowed(rows);
        }
        let combat = self
            .state
            .as_ref()
            .and_then(|state| state.state().observation.combat.as_ref());
        std::borrow::Cow::Owned(
            tor_client_common::stats::lines(combat)
                .into_iter()
                .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
                .collect(),
        )
    }

    fn select_targets(&mut self, ability: Option<Ability>) {
        self.selected_ability = ability;
        self.attack_targets = self
            .state
            .as_ref()
            .map(|state| {
                let view = &state.state().observation;
                view.visible_actors
                    .iter()
                    .filter(|actor| actor.id != view.self_target)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let mut seen = std::collections::BTreeSet::new();
        self.attack_targets.retain(|actor| seen.insert(actor.id));
        self.selected = 0;
        self.status = self.target_prompt();
        if self.attack_targets.is_empty() {
            self.selected_ability = None;
        }
    }

    fn target_prompt(&self) -> String {
        let Some(target) = self.attack_targets.get(self.selected) else {
            return "No target is visible.".into();
        };
        let operation = self.selected_ability.map_or_else(
            || "Attack".to_owned(),
            |ability| {
                format!(
                    "Use {} on",
                    tor_client_common::narration::ability_name(ability)
                )
            },
        );
        format!(
            "{operation} {}? Up/Down select, Enter confirms, Esc cancels.",
            target.name
        )
    }

    pub fn input(&mut self, input: Input) -> Effect {
        if self.stats_open {
            let most = self.stats_rows().len().saturating_sub(STATS_ROWS);
            self.stats_scroll = self.stats_scroll.min(most);
            if let Input::Key { key } = input {
                match key {
                    Key::Up => self.stats_scroll = self.stats_scroll.saturating_sub(1),
                    Key::Down => self.stats_scroll = self.stats_scroll.saturating_add(1).min(most),
                    Key::OlderHistory => {
                        self.stats_scroll = self.stats_scroll.saturating_sub(STATS_ROWS)
                    }
                    Key::RecentHistory => {
                        self.stats_scroll = self.stats_scroll.saturating_add(STATS_ROWS).min(most)
                    }
                    Key::Escape | Key::Stats => self.stats_open = false,
                    _ => {}
                }
            }
            return Effect::None;
        }
        if let Input::Key {
            key: key @ (Key::Slower | Key::Faster),
        } = input
        {
            let at = PACES_MS
                .iter()
                .position(|&pace| pace >= self.pace_ms)
                .unwrap_or(PACES_MS.len() - 1);
            let at = if key == Key::Slower {
                (at + 1).min(PACES_MS.len() - 1)
            } else {
                at.saturating_sub(1)
            };
            self.pace_ms = PACES_MS[at];
            self.status = format!("Journey steps {} ms apart.", self.pace_ms);
            return Effect::Pace(self.pace_ms);
        }
        // Only the server ends a journey; a key press shows the rest at once.
        if matches!(input, Input::Key { .. })
            && self.connected
            && self.state.as_ref().is_some_and(|s| {
                s.has_control() && s.travel().is_some_and(|t| t.phase == TravelPhase::Active)
            })
        {
            self.status = "Skipping ahead.".into();
            return Effect::Skip;
        }
        if self.running.take().is_some() && !matches!(input, Input::Click { .. }) {
            self.status = "You stop running.".into();
            return Effect::None;
        }
        if self.inventory_open || self.help_open {
            if let Input::Key { .. } = input {
                self.inventory_open = false;
                self.help_open = false;
            }
            return Effect::None;
        }
        if let Input::Key { key: Key::Enter } = input {
            if !self.end_dismissed
                && self.state.as_ref().is_some_and(|s| {
                    s.state()
                        .observation
                        .combat
                        .as_ref()
                        .is_some_and(|c| c.terminal)
                })
            {
                self.end_dismissed = true;
                return Effect::None;
            }
        }
        if let Some(scroll) = self.message_log {
            if let Input::Key { key } = input {
                let rows = self.messages.history_rows(crate::messages::WIDTH).len();
                let most = rows.saturating_sub(MESSAGE_LOG_ROWS);
                self.message_log = match key {
                    Key::Up => Some((scroll + 1).min(most)),
                    Key::Down => Some(scroll.saturating_sub(1)),
                    Key::OlderHistory => Some((scroll + MESSAGE_LOG_ROWS).min(most)),
                    Key::RecentHistory => Some(scroll.saturating_sub(MESSAGE_LOG_ROWS)),
                    Key::Escape | Key::MessageLog | Key::Enter => None,
                    _ => Some(scroll),
                };
            }
            return Effect::None;
        }
        if let Input::Key {
            key: Key::MessageLog,
        } = input
        {
            self.message_log = Some(0);
            return Effect::None;
        }
        // Unread messages come first: a key shows the next rows and does
        // nothing else, so no message scrolls away unseen. Esc skips to the end.
        if self.role != AccessRole::Spectator && self.messages.more() {
            match input {
                Input::Key { key: Key::Escape } => self.messages.skip(),
                Input::Key { .. } => self.messages.page(),
                _ => {}
            }
            return Effect::None;
        }
        if !self.ability_choices.is_empty() {
            if let Input::Key { key } = input {
                match key {
                    Key::Escape => {
                        self.ability_choices.clear();
                        self.status = "Never mind.".into();
                    }
                    Key::Up => self.selected = self.selected.saturating_sub(1),
                    Key::Down => {
                        self.selected = self
                            .selected
                            .saturating_add(1)
                            .min(self.ability_choices.len() - 1)
                    }
                    Key::Enter => {
                        let ability = self.ability_choices[self.selected];
                        self.ability_choices.clear();
                        if self.state.as_ref().is_some_and(|state| {
                            tor_client_common::abilities::choices(&state.state().observation)
                                .contains(&ability)
                        }) {
                            self.select_targets(Some(ability));
                        } else {
                            self.status = format!(
                                "You don't have {}.",
                                tor_client_common::narration::ability_name(ability)
                            );
                        }
                    }
                    _ => {}
                }
            }
            return Effect::None;
        }
        if !self.attack_targets.is_empty() {
            match input {
                Input::Key { key: Key::Escape } => {
                    self.attack_targets.clear();
                    self.selected_ability = None;
                    return Effect::None;
                }
                Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
                Input::Key { key: Key::Down } => {
                    self.selected = (self.selected + 1).min(self.attack_targets.len() - 1)
                }
                Input::Key { key: Key::Enter } => {
                    let target = self.attack_targets[self.selected].id;
                    let ability = self.selected_ability.take();
                    self.attack_targets.clear();
                    if let Some(ability) = ability {
                        let action = self.state.as_ref().map(|state| {
                            tor_client_common::abilities::action(
                                &state.state().observation,
                                ability,
                                target,
                            )
                        });
                        return match action {
                            Some(Ok(action)) => self.act(action),
                            Some(Err(reason)) => {
                                self.status = reason;
                                Effect::None
                            }
                            None => Effect::None,
                        };
                    }
                    return self.act(Action::Attack { target });
                }
                _ => {}
            }
            self.status = self.target_prompt();
            return Effect::None;
        }
        if !self.pickup.is_empty() {
            if let Input::Text { text } = &input {
                if let Some(letter) = text.chars().find(char::is_ascii_alphabetic) {
                    if let Some(index) =
                        (0..self.pickup.len()).find(|&i| self.choice_letter(i) == letter)
                    {
                        self.selected = index;
                        return self.input(Input::Key { key: Key::Enter });
                    }
                    self.status = format!("No item is lettered {letter}.");
                    return Effect::None;
                }
                if self.item_operation.is_some() {
                    return Effect::None;
                }
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
            if self.travel_cursor.take().is_some() || self.look_cursor.take().is_some() {
                self.status = "Never mind.".into();
                return Effect::None;
            }
            if self.wizard_command.take().is_some()
                || self.note.take().is_some()
                || self.history_page.take().is_some()
                || !self.pickup.is_empty()
                || self.door_direction.is_some()
            {
                self.pickup.clear();
                self.item_operation = None;
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
        if let Some(draft) = &mut self.wizard_command {
            match input {
                Input::Text { text } => {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        if draft.len() + ch.len_utf8() <= MAX_NOTE_BYTES {
                            draft.push(ch);
                        }
                    }
                }
                Input::Key {
                    key: Key::Backspace,
                } => {
                    draft.pop();
                }
                Input::Key { key: Key::Enter } if !draft.trim().is_empty() => {
                    let operation = draft.trim().to_owned();
                    self.wizard_command = None;
                    if let Some(state) = &self.state {
                        return self.command(Command::Wizard {
                            expected_revision: state.state().revision,
                            operation,
                        });
                    }
                }
                _ => {}
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
            {
                return Effect::None;
            }
            if let Some(position) = self
                .state
                .as_ref()
                .and_then(|s| crate::render::known_cell_at(s, x, y))
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
                    self.history_scroll = (self.history_scroll + 1).min(
                        history_lines(&page.entries)
                            .len()
                            .saturating_sub(HISTORY_ROWS),
                    );
                    return Effect::None;
                }
                Key::OlderHistory | Key::RecentHistory | Key::History => {}
                _ => return Effect::None,
            }
        }
        if self.role == AccessRole::Spectator
            && !matches!(
                key,
                Key::Places
                    | Key::History
                    | Key::OlderHistory
                    | Key::RecentHistory
                    | Key::Look
                    | Key::Inventory
                    | Key::Stats
                    | Key::Help
                    | Key::MessageLog
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
                    if let Some(operation) = self.item_operation.take() {
                        let result = item_action(
                            &self
                                .state
                                .as_ref()
                                .expect("connected state")
                                .state()
                                .observation,
                            item,
                            operation,
                        );
                        self.pickup.clear();
                        self.item_operation = None;
                        return match result {
                            Ok(action) => self.act(action),
                            Err(reason) => {
                                self.status = reason;
                                Effect::None
                            }
                        };
                    }
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
                    self.item_operation = None;
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

                Key::Ascend | Key::Descend => {
                    let up = key == Key::Ascend;
                    let wanted = if up {
                        crate::map::Kind::StairsUp
                    } else {
                        crate::map::Kind::StairsDown
                    };
                    let found = self.state.as_ref().and_then(|s| {
                        crate::render::map_tiles(s)
                            .into_iter()
                            .filter(|t| t.kind == wanted)
                            .min_by_key(|t| {
                                (t.position.x - cursor.x).abs() + (t.position.y - cursor.y).abs()
                            })
                            .map(|t| t.position)
                    });
                    match found {
                        Some(stairs) => cursor = stairs,
                        None => {
                            self.status = format!(
                                "You don't know of any stairs {} on this map.",
                                if up { "up" } else { "down" }
                            );
                        }
                    }
                }
                Key::Enter | Key::Wait | Key::Travel => {
                    let target = self
                        .state
                        .as_ref()
                        .and_then(|s| crate::render::known_column(s, cursor.x, cursor.y));
                    return match target {
                        Some(target) => self.travel_to(target),
                        None => {
                            self.status = "You don't know that spot. Pick a known floor.".into();
                            Effect::None
                        }
                    };
                }
                _ => return Effect::None,
            }
            if let Some(state) = &self.state {
                cursor = crate::map::clamp(&state.state().observation, cursor);
            }
            self.travel_cursor = Some(cursor);
            return Effect::None;
        }
        if let Some(mut cursor) = self.look_cursor {
            match key {
                Key::Up => cursor.y -= 1,
                Key::Down => cursor.y += 1,
                Key::Left => cursor.x -= 1,
                Key::Right => cursor.x += 1,
                Key::NorthEast => {
                    cursor.x += 1;
                    cursor.y -= 1;
                }
                Key::SouthEast => {
                    cursor.x += 1;
                    cursor.y += 1;
                }
                Key::SouthWest => {
                    cursor.x -= 1;
                    cursor.y += 1;
                }
                Key::NorthWest => {
                    cursor.x -= 1;
                    cursor.y -= 1;
                }
                Key::Enter | Key::Look | Key::Wait => {
                    self.look_cursor = None;
                    self.status.clear();
                    if let Some(state) = &self.state {
                        let text = crate::describe(state, cursor.x, cursor.y);
                        self.messages.push(text);
                    }
                    return Effect::None;
                }
                _ => return Effect::None,
            }
            if let Some(state) = &self.state {
                cursor = crate::map::clamp(&state.state().observation, cursor);
            }
            self.look_cursor = Some(cursor);
            return Effect::None;
        }
        match key {
            Key::Look => {
                if let Some(state) = &self.state {
                    self.look_cursor = Some(state.state().observation.position);
                    self.status = "Look at what? hjklyubn moves, . or ; picks, Esc cancels.".into();
                }
                Effect::None
            }
            Key::Inventory => {
                self.inventory_open = true;
                Effect::None
            }
            Key::Stats => {
                self.inspection = None;
                self.stats_open = true;
                self.stats_scroll = 0;
                Effect::None
            }
            Key::Help => {
                self.help_open = true;
                Effect::None
            }
            Key::RunUp
            | Key::RunDown
            | Key::RunLeft
            | Key::RunRight
            | Key::RunNorthEast
            | Key::RunSouthEast
            | Key::RunSouthWest
            | Key::RunNorthWest => {
                let direction = match key {
                    Key::RunUp => Direction::North,
                    Key::RunDown => Direction::South,
                    Key::RunLeft => Direction::West,
                    Key::RunRight => Direction::East,
                    Key::RunNorthEast => Direction::NorthEast,
                    Key::RunSouthEast => Direction::SouthEast,
                    Key::RunSouthWest => Direction::SouthWest,
                    _ => Direction::NorthWest,
                };
                if self.creature_in_view() {
                    self.status = "You can't run with a creature in view.".into();
                    return Effect::None;
                }
                if !self.open_ahead(direction) {
                    self.status = "You can't run that way.".into();
                    return Effect::None;
                }
                let effect = self.act(Action::Move { direction });
                if matches!(effect, Effect::Request(_)) {
                    self.running = Some(direction);
                }
                effect
            }
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
                        "Travel where? hjklyubn moves, < > stairs, . goes, Esc cancels.".into();
                } else {
                    self.status = "Acquire control before travelling.".into();
                }
                Effect::None
            }
            Key::Up => self.act(Action::Move {
                direction: Direction::North,
            }),
            Key::Attack => {
                self.select_targets(None);
                Effect::None
            }
            Key::Abilities => {
                if let Some(state) = &self.state {
                    if !state.has_control() {
                        self.status = "You are observing. Press F3 to request control.".into();
                        return Effect::None;
                    }
                    self.ability_choices =
                        tor_client_common::abilities::choices(&state.state().observation);
                    self.selected = 0;
                    self.status = if self.ability_choices.is_empty() {
                        "No abilities are available.".into()
                    } else {
                        "Choose an ability. Up/Down select, Enter confirms, Esc cancels.".into()
                    };
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
            Key::Wait => self.act(Action::Wait),
            Key::Control => self.request(Request::AcquireControl),
            Key::Release => self.request(Request::ReleaseControl),
            Key::ResumeIntention | Key::CancelIntention => {
                let Some(state) = &self.state else {
                    return Effect::None;
                };
                if !state.has_control() {
                    self.status = "You are observing. Press F3 to request control.".into();
                    return Effect::None;
                }
                let request = if key == Key::ResumeIntention {
                    state.resume_intention_request()
                } else {
                    state.cancel_intention_request()
                };
                if let Some(request) = request {
                    self.request(request)
                } else {
                    self.status = if key == Key::ResumeIntention {
                        "No suspended action to resume."
                    } else {
                        "No queued action to cancel."
                    }
                    .into();
                    Effect::None
                }
            }
            Key::OpenDoor | Key::CloseDoor => {
                let Some(state) = &self.state else {
                    return Effect::None;
                };
                if !state.has_control() {
                    self.attack_targets.clear();
                    self.place_name = None;
                    self.status = "You are observing. Press F3 to request control.".into();
                } else if !state.can_admit_intention() {
                    self.status = "You can't act right now.".into();
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
                self.item_operation = None;
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
                // Keep disclosed choice order; opaque identities have no ordinal meaning.
                let mut seen = std::collections::BTreeSet::new();
                items.retain(|item| seen.insert(item.id));
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
                    // A lone object is picked up at once; dropping always asks.
                    [item] if item.quantity == 1 && !self.dropping => self.act(Action::Take {
                        item: item.id,
                        quantity: None,
                    }),
                    _ => {
                        self.pickup = items;
                        self.selected = 0;
                        self.status = format!(
                            "What do you want to {}? Type its letter; digits first set a count.",
                            if self.dropping { "drop" } else { "pick up" }
                        );
                        Effect::None
                    }
                }
            }
            Key::Equip | Key::Unequip | Key::Drink => {
                let operation = match key {
                    Key::Equip => ItemOperation::Equip,
                    Key::Unequip => ItemOperation::Unequip,
                    _ => ItemOperation::Drink,
                };
                let Some(state) = &self.state else {
                    return Effect::None;
                };
                let view = &state.state().observation;
                let mut seen = std::collections::BTreeSet::new();
                let items: Vec<_> = item_choices(view, operation)
                    .filter(|item| seen.insert(item.id))
                    .cloned()
                    .collect();
                self.quantity.clear();
                match items.as_slice() {
                    [] => {
                        self.status = if view.interactions.is_none() && !view.inventory.is_empty() {
                            format!(
                                "Nothing you carry can be used to {} here.",
                                operation.verb()
                            )
                        } else {
                            format!("You don't have anything to {}.", operation.verb())
                        };
                        Effect::None
                    }
                    _ => {
                        self.pickup = items;
                        self.item_operation = Some(operation);
                        self.selected = 0;
                        self.status = format!(
                            "What do you want to {}? Type its letter; Esc cancels.",
                            operation.verb()
                        );
                        Effect::None
                    }
                }
            }
            Key::Places => {
                self.places_open = true;
                self.place_selected = 0;
                Effect::None
            }
            Key::Wizard => {
                if self.role == AccessRole::Wizard {
                    self.wizard_command = Some(String::new());
                } else {
                    self.status = "Wizard authority is required.".into();
                }
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
            || !state.can_admit_intention()
        {
            self.status = "Travel requires permission to admit an action.".into();
            return Effect::None;
        }
        let Some(cell) = state
            .map_cell(position)
            .filter(|c| !c.wall && c.door.as_ref().is_none_or(|d| d.open))
        else {
            self.status = "Select a known floor cell.".into();
            return Effect::None;
        };
        let command = Command::Travel {
            expected_revision: state.state().revision,
            destination: cell.key,
        };
        self.travel_cursor = None;
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
        if !state.can_admit_intention() {
            if !state.state().observation.ready
                && matches!(action, Action::Wait)
                && state
                    .state()
                    .observation
                    .combat
                    .as_ref()
                    .is_some_and(|c| !c.terminal)
            {
                self.messages.begin_turn();
                return self.request(Request::Continue);
            }
            self.status = if ended(state) {
                "This run has ended."
            } else {
                "You can't act right now."
            }
            .into();
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
                a.id != view.self_target
                    && a.position == (Position { x, y, z })
                    && match self.bump_attacks {
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
        let expected_revision = state.state().revision;
        let effect = self.command(Command::Act {
            expected_revision,
            action: action.clone(),
        });
        self.last_move = matches!(action, Action::Move { .. });
        effect
    }

    fn command(&mut self, command: Command) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if matches!(command, Command::Act { .. } | Command::Travel { .. }) {
            if !state.can_admit_intention() {
                self.status = if ended(state) {
                    "This run has ended."
                } else {
                    "You can't act right now."
                }
                .into();
                return Effect::None;
            }
            self.messages.begin_turn();
        }
        let request = state.command_request(command);
        self.request(request)
    }

    fn request(&mut self, request: Request) -> Effect {
        self.busy = true;
        self.status = "Waiting for server...".into();
        Effect::Request(request)
    }
}

/// Rows the history screen shows at once.
pub const HISTORY_ROWS: usize = 22;
pub const STATS_ROWS: usize = 24;
pub const STATS_WIDTH: usize = 65;

/// Rows the message log screen shows at once.
pub const MESSAGE_LOG_ROWS: usize = 24;

/// A request's progress in plain words; a finished action needs none.
pub fn phase_status(phase: IntentionPhase) -> &'static str {
    match phase {
        IntentionPhase::Queued => "Action queued.",
        IntentionPhase::Suspended => "Action suspended. F8 resumes, F9 cancels.",
        IntentionPhase::Paused => "Attack paused.",
        IntentionPhase::Started => "Action under way.",
        IntentionPhase::Resolved => "",
        IntentionPhase::Failed => "That didn't work.",
        IntentionPhase::Cancelled => "Action cancelled.",
    }
}

/// A server refusal in the player's words; unknown reasons keep the server's text.
pub fn plain_error(code: ErrorCode, message: &str) -> String {
    match (code, message) {
        (ErrorCode::InvalidAction, "Intention is unavailable") => "You can't do that now.".into(),
        (ErrorCode::InvalidAction, "Travel destination or known route is unavailable") => {
            "You don't know a way there.".into()
        }
        (ErrorCode::ActorBusy, _) => "You're still on your way.".into(),
        (ErrorCode::ControlTaken, _) => {
            "Another player has control; you can watch until they release it.".into()
        }
        (ErrorCode::NotController, _) => "You don't have control. Press F3 to request it.".into(),
        (ErrorCode::StaleRevision | ErrorCode::StaleContext | ErrorCode::WrongBranch, _) => {
            "The game moved on before that arrived; look again and retry.".into()
        }
        (ErrorCode::StorageFailure, _) => format!("The game couldn't be saved: {message}"),
        _ => {
            let mut text = message.trim().to_owned();
            if !text.ends_with(['.', '!', '?']) {
                text.push('.');
            }
            text
        }
    }
}

/// "a dawn seal", "an ember", "3 x arrow".
pub fn item_phrase(name: &str, quantity: u64) -> String {
    if quantity == 1 {
        let article = if name.to_lowercase().starts_with(['a', 'e', 'i', 'o', 'u']) {
            "an"
        } else {
            "a"
        };
        format!("{article} {name}")
    } else {
        format!("{quantity} x {name}")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BumpAttacks {
    #[default]
    Hostile,
    Any,
    Off,
}

/// The glyph the map draws for a column of the observer's scene, or a space
/// when nothing there is disclosed.
pub fn glyph_at(o: &Observation, x: i32, y: i32) -> char {
    crate::map::tiles(o, &|_| true)
        .into_iter()
        .find(|t| (t.position.x, t.position.y) == (x, y))
        .map_or(' ', |t| t.glyph)
}

pub fn history_text(entry: &HistoryEntry) -> String {
    match &entry.content {
        HistoryContent::PlaceRenamed { name, .. } => format!("Place named {}.", name),
        HistoryContent::Travel { .. } => "Travel requested.".into(),
        HistoryContent::Wizard { summary, .. } => summary.clone(),
        HistoryContent::Action { event, .. } => match event {
            Event::Moved { direction } => format!("Moved {}.", direction_name(*direction)),
            Event::Taken { quantity, .. } => {
                format!(
                    "Picked up {}.",
                    if *quantity == 1 {
                        "an item".to_owned()
                    } else {
                        format!("{quantity} items")
                    }
                )
            }
            Event::Dropped { quantity, .. } => {
                format!(
                    "Dropped {}.",
                    if *quantity == 1 {
                        "an item".to_owned()
                    } else {
                        format!("{quantity} items")
                    }
                )
            }
            Event::DoorChanged { open, .. } => {
                format!("{} door.", if *open { "Opened" } else { "Closed" })
            }
            Event::Waited => "Waited.".into(),
            Event::PreparationPaused => "Preparation paused.".into(),
            Event::AttackStarted { .. } => "Prepared an attack.".into(),
            Event::AbilityStarted { ability, .. } => format!(
                "Prepared {}.",
                tor_client_common::narration::ability_name(*ability)
            ),
            Event::ItemStarted { action } => match action {
                tor_protocol::Action::Equip { .. } => "Began equipping an item.".into(),
                tor_protocol::Action::Unequip { .. } => "Began removing an item.".into(),
                tor_protocol::Action::Drink { .. } => "Began drinking an item.".into(),
                _ => "Began preparation.".into(),
            },
        },
        HistoryContent::Annotation { text, .. } => {
            let audience = if entry.audience == Audience::Private {
                "Private note"
            } else {
                "Note"
            };
            format!("{audience}: {text}")
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

/// History entries for the history screen, newest last, one per turn time.
pub fn history_lines(entries: &[HistoryEntry]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|e| crate::messages::word_wrap(&format!("T:{}  {}", e.tick, history_text(e)), 66))
        .collect()
}

/// A direction as a word.
pub fn direction_name(direction: Direction) -> &'static str {
    match direction {
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

pub fn item_glyph(class: ItemClass) -> char {
    match class {
        ItemClass::Weapon => ')',
        ItemClass::Armor => '[',
        ItemClass::Potion => '!',
        ItemClass::Food | ItemClass::Corpse => '%',
        ItemClass::Misc | ItemClass::Tool => '(',
        ItemClass::Amulet => '"',
        ItemClass::Ring => '=',
        ItemClass::Scroll => '?',
        ItemClass::Spellbook => '+',
        ItemClass::Wand => '/',
        ItemClass::Coin => '$',
        ItemClass::Gem => '*',
    }
}

/// Why a journey stopped short, as a message; arriving needs none.
pub fn travel_stop(phase: TravelPhase) -> Option<&'static str> {
    Some(match phase {
        TravelPhase::Active | TravelPhase::Arrived => return None,
        TravelPhase::Blocked => "Your way is blocked.",
        TravelPhase::Hazard => "You stop. There may be danger in sight.",
        TravelPhase::DecisionRequired => "You are thrown off course.",
        TravelPhase::ControlLost => "You stop travelling; control was released.",
        TravelPhase::WorldChanged => "You stop. Something changed along the way.",
        TravelPhase::Failed => "You couldn't travel there.",
    })
}

/// One carried item as the inventory shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryEntry {
    pub letter: char,
    pub glyph: char,
    pub name: String,
    pub quantity: u64,
    /// The slot it's worn or held in, when equipped.
    pub equipped: Option<String>,
    /// Known equipment numbers, when the server discloses them.
    pub stats: Option<String>,
}

impl InventoryEntry {
    pub fn short(&self) -> String {
        format!(
            "{} - {}",
            self.letter,
            item_phrase(&self.name, self.quantity)
        )
    }
    pub fn long(&self) -> String {
        let mut text = format!(
            "{} - {} {}",
            self.letter,
            self.glyph,
            item_phrase(&self.name, self.quantity)
        );
        if let Some(slot) = &self.equipped {
            text.push_str(&format!(" ({})", slot));
        }
        if let Some(stats) = &self.stats {
            text.push_str(&format!("  [{stats}]"));
        }
        text
    }
}

fn slot_phrase(slot: EquipmentSlot) -> &'static str {
    match slot {
        EquipmentSlot::Weapon => "weapon in hand",
        EquipmentSlot::Ring => "on a finger",
        EquipmentSlot::Amulet => "around the neck",
        _ => "being worn",
    }
}

/// Everything carried, lettered and in disclosed order.
pub fn inventory_entries(app: &App, o: &Observation) -> Vec<InventoryEntry> {
    o.inventory
        .iter()
        .map(|item| {
            let affordance = o
                .interactions
                .as_ref()
                .and_then(|i| i.inventory.iter().find(|a| a.item == item.id));
            let equipped = affordance.and_then(|a| a.equipped_slot).map(|index| {
                o.interactions
                    .as_ref()
                    .and_then(|i| i.slots.get(index as usize))
                    .map_or_else(|| "in use".into(), |slot| slot_phrase(*slot).to_owned())
            });
            let stats = affordance
                .and_then(|a| a.known_equipment.as_ref())
                .map(|known| {
                    let mut parts = Vec::new();
                    if let Some(attack) = &known.attack {
                        parts.push(format!("attack {:+}", attack.bonus));
                    }
                    if known.defense != 0 {
                        parts.push(format!("defense {:+}", known.defense));
                    }
                    parts.join(", ")
                })
                .filter(|text| !text.is_empty());
            InventoryEntry {
                letter: app.letters.get(&item.id).copied().unwrap_or('?'),
                glyph: item_glyph(item.class),
                name: item.name.clone(),
                quantity: item.quantity,
                equipped,
                stats,
            }
        })
        .collect()
}

/// What the look command says about a column.
pub fn describe(state: &ClientState, x: i32, y: i32) -> String {
    let o = &state.state().observation;
    if (x, y) == (o.position.x, o.position.y) {
        return "That's you.".into();
    }
    if let Some(actor) = o
        .visible_actors
        .iter()
        .find(|a| a.id != o.self_target && (a.position.x, a.position.y) == (x, y))
    {
        let name = if actor.name.trim().is_empty() {
            "figure"
        } else {
            actor.name.as_str()
        };
        let mut text = format!(
            "{} - {}",
            crate::map::creature_glyph(name),
            item_phrase(name, 1)
        );
        if let Some(combat) = o
            .combat
            .as_ref()
            .and_then(|c| c.actors.iter().find(|c| c.actor == actor.id))
        {
            text.push_str(&format!(
                " ({}, {})",
                if combat.hostile {
                    "hostile"
                } else {
                    "peaceful"
                },
                tor_client_common::narration::injury(combat.injury)
            ));
        }
        if !actor.description.trim().is_empty() {
            text.push_str(&format!(". {}", actor.description.trim()));
        }
        return text + ".";
    }
    let tiles = crate::render::map_tiles(state);
    let Some(tile) = tiles
        .iter()
        .find(|t| (t.position.x, t.position.y) == (x, y))
    else {
        return "You don't know what's there.".into();
    };
    let what = match tile.kind {
        crate::map::Kind::Item => {
            let items: Vec<_> = o
                .ground_items
                .iter()
                .chain(state.map_memory().flat_map(|c| c.ground_items.iter()))
                .filter(|i| (i.position.x, i.position.y) == (x, y))
                .collect();
            match items.first() {
                Some(item) => item_phrase(&item.item.name, item.item.quantity),
                None => "an object".into(),
            }
        }
        crate::map::Kind::Wall => "a wall".into(),
        crate::map::Kind::LowWall => "a low wall".into(),
        crate::map::Kind::Drop => "a drop".into(),
        crate::map::Kind::Door => {
            if tile.glyph == '/' {
                "an open door".into()
            } else {
                "a closed door".into()
            }
        }
        crate::map::Kind::StairsUp => "a staircase up".into(),
        crate::map::Kind::StairsDown => "a staircase down".into(),
        crate::map::Kind::Floor => "the floor".into(),
        crate::map::Kind::Player => "you".into(),
        crate::map::Kind::Creature => "a creature".into(),
    };
    format!(
        "{} - {}{}.",
        tile.glyph,
        what,
        if tile.remembered { " (remembered)" } else { "" }
    )
}

/// The known place whose anchor in sight is nearest the player.
fn place_name(o: &Observation) -> Option<&str> {
    o.places
        .iter()
        .filter_map(|place| {
            o.visible_cells
                .iter()
                .filter(|c| c.key == place.key)
                .map(|c| {
                    let d = c.position;
                    (d.x - o.position.x).pow(2) + (d.y - o.position.y).pow(2)
                })
                .min()
                .map(|distance| (distance, place.name.as_str()))
        })
        .min()
        .map(|(_, name)| name)
}

/// NetHack's two status lines: where you are and what you're after, then
/// hit points, time and conditions.
pub fn status_lines(app: &App, state: &ClientState) -> [String; 2] {
    let o = &state.state().observation;
    let me = o
        .visible_actors
        .iter()
        .find(|a| a.id == o.self_target && !a.name.trim().is_empty())
        .map(|a| a.name.as_str());
    let mut first = place_name(o).or(me).unwrap_or_default().to_owned();
    if let Some(objective) = o.combat.as_ref().and_then(|c| c.objective) {
        if !first.is_empty() {
            first.push_str("   ");
        }
        first.push_str(tor_client_common::narration::objective(objective));
    }
    let mut second = Vec::new();
    if let Some(combat) = &o.combat {
        second.push(format!("HP:{}({})", combat.hp, combat.max_hp));
    }
    second.push(format!("T:{}", o.tick));
    if let Some(combat) = &o.combat {
        if combat.dead {
            second.push("Dead".into());
        } else if combat.victory {
            second.push("Victorious".into());
        } else if combat.preparation_remaining.is_some() {
            second.push(
                if combat.preparation_active {
                    "Attacking"
                } else {
                    "Attack paused"
                }
                .into(),
            );
        } else if combat.recovery_remaining > 0 {
            second.push("Recovering".into());
        }
    }
    if state
        .travel()
        .is_some_and(|t| t.phase == TravelPhase::Active)
    {
        second.push("Travelling".into());
    }
    if app.running.is_some() {
        second.push("Running".into());
    }
    if state.state().wizard_game {
        second.push("Wizard".into());
    }
    [first, second.join("  ")]
}

/// Who controls the character, and whether a request is out.
pub fn control_label(app: &App) -> String {
    let control = if !app.connected {
        "DISCONNECTED - relaunch to reconnect"
    } else if app.role == AccessRole::Spectator {
        "SPECTATOR"
    } else if app.state.as_ref().is_some_and(|s| s.has_control()) {
        "IN CONTROL"
    } else {
        "OBSERVING - F3 to take control"
    };
    if app.busy && app.connected {
        format!("{control}   waiting for the server...")
    } else {
        control.into()
    }
}

/// The end-of-run screen's lines.
pub fn end_summary(state: &ClientState) -> Vec<String> {
    let o = &state.state().observation;
    let mut lines = Vec::new();
    if let Some(combat) = &o.combat {
        lines.push(format!("Hit points {} of {}", combat.hp, combat.max_hp));
    }
    lines.push(format!("Time {}   Places known {}", o.tick, o.places.len()));
    let carried: Vec<_> = o
        .inventory
        .iter()
        .map(|item| item_phrase(&item.name, item.quantity))
        .collect();
    lines.push(if carried.is_empty() {
        "Carrying nothing".into()
    } else {
        format!("Carrying {}", carried.join(", "))
    });
    lines
}

/// The help screen.
pub const HELP: &[&str] = &[
    "MOVING                         MAP",
    "y k u   move; Shift runs        @ you      a-z creatures",
    "h @ l   (arrows also move)      # wall     # low wall (tan)",
    " b j n                          . floor    ^ drop or pit",
    "< >     go up / down stairs     < > stairs + / doors",
    "_       travel (< > jump)       ) [ ! % ( = \" ? / $ * items",
    "click   travel to a cell        grey: remembered, not in sight",
    "",
    "ACTING                         SEEING",
    "a       attack a target         ;      look at a cell",
    "z       ability, then target    @      creature stats",
    ". space wait                    i      inventory",
    "g , d   pick up / drop          ^P     earlier messages",
    "w t q   wear+wield/remove/drink F2     game history",
    "o c     open / close a door     F5     remembered places",
    "                                F4     write a note",
    "                                F7     wizard command editor",
    "F3 R    take / release control  [ ]    slower / faster journeys",
    "F8 F9   resume / cancel action  Esc    cancel, close, or quit",
];

/// Whether the run is over: won or lost with no further play.
fn ended(state: &ClientState) -> bool {
    state
        .state()
        .observation
        .combat
        .as_ref()
        .is_some_and(|c| c.terminal)
}
