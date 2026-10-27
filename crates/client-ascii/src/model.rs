use serde::{Deserialize, Serialize};
use tor_client_common::holds::{hold_choices, render_holds, HoldChoice};
use tor_client_common::items::{
    identification_action, item_action, item_choices, zap_action, ItemOperation,
};
use tor_client_common::ClientState;
use tor_protocol::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    /// Open read-only detection charts.
    Charts,
    Advancement,
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
    Eat,
    Read,
    Zap,
    Ignite,
    Extinguish,
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
    /// Try to escape one exact disclosed incoming hold.
    EscapeHold,
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
    /// Stop only chart hydration; never a gameplay request.
    CancelCharts,
    None,
    Quit,
    Request(Box<Request>),
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

#[derive(serde::Serialize)]
pub enum SupplyPurpose {
    Ability {
        ability: Box<AbilityView>,
        invocation: AbilityInvocation,
    },
    Respec,
}
#[derive(serde::Serialize)]
pub struct SupplyDraft {
    pub purpose: SupplyPurpose,
    pub label: String,
    pub costs: Vec<AbilitySupplyCost>,
    pub candidates: Vec<tor_client_common::abilities::SupplyCandidate>,
    pub checked: std::collections::BTreeSet<ItemTarget>,
}

#[derive(serde::Serialize)]
pub struct WeaponDraft {
    pub choices: Vec<tor_client_common::weapons::WeaponChoice>,
    previous_selected: usize,
    previous_status: String,
}

pub struct App {
    pub charts_open: bool,
    pub charts: tor_client_common::charts::ChartPages,
    pub chart_selected: usize,
    pub chart_scroll: usize,
    /// Current connection palette, including stale state during recovery.
    pub palette: tor_client_common::Palette,
    pub advancement_open: bool,
    pub advancement_command: String,
    pub advancement_scroll: usize,
    advancement_pages: tor_client_common::advancement::AdvancementPages,
    advancement_pending: Option<(u64, AdvancementQuery)>,
    pub bump_attacks: BumpAttacks,
    pub pace_ms: u64,
    pub attack_targets: Vec<ActorView>,
    pub ability_choices: Vec<AbilityView>,
    /// Frozen exact offers for the explicit hold picker, never rebound by index.
    pub hold_choices: Vec<HoldChoice>,
    pub selected_ability: Option<AbilityView>,
    pub supply_selection: Option<SupplyDraft>,
    pub selected_weapon: WeaponSelection,
    pub weapon_selection: Option<WeaponDraft>,
    pub weapon_confirmation: Option<tor_client_common::weapon_preview::WeaponPreview>,
    pub projectile_selection: Option<tor_client_common::projectiles::ProjectileMenu>,
    /// The look cursor, while choosing a cell to describe.
    pub look_cursor: Option<Position>,
    pub inventory_open: bool,
    pub help_open: bool,
    pub stats_open: bool,
    inspection: Option<Vec<String>>,
    inspection_title: &'static str,
    strike_pages: tor_client_common::inspection_pages::InspectionPages,
    strike_sources_open: bool,
    strike_source_selected: usize,
    strike_detail_open: bool,
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
    /// Exact carried device bound to the displayed observation, before aiming.
    zap_selection: Option<(ItemTarget, ReplyContext)>,
    pub identification_selected: usize,
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
            charts_open: false,
            charts: Default::default(),
            chart_selected: 0,
            chart_scroll: 0,
            palette: Default::default(),
            advancement_open: false,
            advancement_command: String::new(),
            advancement_scroll: 0,
            advancement_pages: Default::default(),
            advancement_pending: None,
            bump_attacks: BumpAttacks::Hostile,
            pace_ms: DEFAULT_PACE_MS,
            attack_targets: Vec::new(),
            ability_choices: Vec::new(),
            hold_choices: Vec::new(),
            selected_ability: None,
            supply_selection: None,
            selected_weapon: WeaponSelection::Default,
            weapon_selection: None,
            weapon_confirmation: None,
            projectile_selection: None,
            look_cursor: None,
            inventory_open: false,
            help_open: false,
            stats_open: false,
            inspection: None,
            inspection_title: "CREATURE INSPECTION",
            strike_pages: Default::default(),
            strike_sources_open: false,
            strike_source_selected: 0,
            strike_detail_open: false,
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
            zap_selection: None,
            identification_selected: 0,
            quantity: String::new(),
            door_direction: None,
            selected: 0,
            history_page: None,
            history_scroll: 0,
            messages: Default::default(),
            message_log: None,
        }
    }

    fn clear_magic_selection(&mut self) {
        self.zap_selection = None;
        self.identification_selected = 0;
        if matches!(
            self.item_operation,
            Some(
                ItemOperation::Read
                    | ItemOperation::Zap
                    | ItemOperation::Ignite
                    | ItemOperation::Extinguish
            )
        ) {
            self.item_operation = None;
            self.pickup.clear();
        }
    }

    pub fn set_state(&mut self, state: ClientState) {
        let old = self
            .state
            .as_ref()
            .map(|s| (s.branch().clone(), s.state().revision));
        self.state = Some(state);
        // A new transport owner has no matching local chart selection.
        self.charts.clear();
        self.chart_selected = 0;
        self.clear_magic_selection();
        self.hold_choices.clear();
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
        let control_changed = matches!(update.body, UpdateBody::Control { .. });
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
        if observed || control_changed {
            if matches!(
                self.item_operation,
                Some(
                    ItemOperation::Read
                        | ItemOperation::Zap
                        | ItemOperation::Ignite
                        | ItemOperation::Extinguish
                )
            ) {
                self.item_operation = None;
                self.pickup.clear();
            }
            self.zap_selection = None;
            self.identification_selected = 0;
            self.hold_choices.clear();
        }
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
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.inspection = None;
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
        // Recovery cancels the transport's chart request. Return to the catalog
        // only after validation succeeds rather than leaving a loading selection
        // that has no corresponding worker query.
        self.charts.clear();
        self.chart_selected = 0;
        self.hold_choices.clear();
        self.clear_magic_selection();
        self.inspection = None;
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.wizard_command = None;
        self.stats_open = false;
        self.close_advancement();
        self.state_changed(old);
        Ok(())
    }

    fn state_changed(&mut self, old: Option<(BranchId, u64)>) {
        if let Some(state) = &self.state {
            self.charts.observe(state);
        }
        self.chart_scroll = 0;
        if self
            .projectile_selection
            .as_ref()
            .is_some_and(|menu| self.state.as_ref().is_none_or(|state| !menu.current(state)))
        {
            self.projectile_selection = None;
            self.status = "Projectile choices changed; choose the ability again.".into();
        }
        if self.weapon_confirmation.as_ref().is_some_and(|preview| {
            self.state
                .as_ref()
                .is_none_or(|state| !preview.current(state))
        }) {
            self.weapon_confirmation = None;
            self.status = "Weapon preview changed; choose again before confirming.".into();
        }
        if (self.weapon_selection.is_some() || self.selected_weapon != WeaponSelection::Default)
            && self.state.as_ref().is_some_and(|state| {
                !state.has_control()
                    || old.as_ref().is_some_and(|(branch, revision)| {
                        branch != state.branch() || *revision != state.state().revision
                    })
            })
        {
            self.status = "Observation changed; choose your weapon again.".into();
        }
        if let Some(state) = &self.state {
            let had_private_page = self.advancement_pages.page().is_some();
            self.advancement_pages
                .observe(&state.snapshot().reply_context(), state.has_control());
            if !state.has_control() || (had_private_page && self.advancement_pages.page().is_none())
            {
                self.close_advancement();
            }
        }
        let state = self.state.as_ref().expect("validated state");
        if self
            .zap_selection
            .as_ref()
            .is_some_and(|(_, context)| *context != state.snapshot().reply_context())
        {
            self.zap_selection = None;
        }
        if old
            .as_ref()
            .is_some_and(|(branch, _)| branch != state.branch())
        {
            self.travel_cursor = None;
            self.note = None;
            self.pickup.clear();
            self.item_operation = None;
            self.zap_selection = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.hold_choices.clear();
            self.selected_ability = None;
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
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
            self.zap_selection = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.hold_choices.clear();
            self.selected_ability = None;
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
            self.door_direction = None;
            self.travel_cursor = None;
        }
        if !state.has_control() {
            self.pickup.clear();
            self.item_operation = None;
            self.zap_selection = None;
            self.attack_targets.clear();
            self.ability_choices.clear();
            self.hold_choices.clear();
            self.selected_ability = None;
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
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
        self.projectile_selection = None;
        self.weapon_confirmation = None;
        if self.advancement_open {
            self.advancement_pages.invalidate();
            self.advancement_pending = None;
        }
        if self.strike_detail_open || self.strike_sources_open {
            self.strike_pages.clear();
            self.strike_detail_open = false;
            self.strike_sources_open = false;
            self.inspection = None;
        }
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
        // Hazard disclosure wins over item/creature glyph precedence. Running
        // must not step onto a known trap merely because an item covers it.
        if o.visible_cells.iter().any(|cell| {
            cell.trap.is_some()
                && (cell.position.x, cell.position.y) == (o.position.x + dx, o.position.y + dy)
                && cell.position.z == o.position.z
        }) {
            return false;
        }
        crate::render::map_tiles(state, &self.palette)
            .into_iter()
            .any(|t| {
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

    /// Item picker rows use only this exact instance's disclosed charge count.
    pub fn item_rows(&self) -> Vec<String> {
        self.pickup
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let name = if !self.dropping && self.item_operation.is_none() {
                    self.state
                        .as_ref()
                        .and_then(|state| {
                            state
                                .state()
                                .observation
                                .ground_items
                                .iter()
                                .find(|g| g.item.id == item.id)
                        })
                        .map_or(item.name.as_str(), tor_client_common::items::ground_name)
                } else {
                    &item.name
                };
                let mut label = item_phrase(name, item.quantity);
                if let Some(charges) = self
                    .state
                    .as_ref()
                    .and_then(|state| state.state().observation.interactions.as_ref())
                    .and_then(|view| view.inventory.iter().find(|offer| offer.item == item.id))
                    .and_then(|offer| offer.charges)
                {
                    label.push_str(&format!(" [charges: {charges}]"));
                }
                format!(
                    "{} {} - {}",
                    if index == self.selected { ">" } else { " " },
                    self.choice_letter(index),
                    label
                )
            })
            .collect()
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
        // Admission opens a queue slot even while other actors are advancing.
        // A run must inspect their settled results before choosing its next step;
        // unlike explicit one-step input it must not queue from an intermediate view.
        if self.busy || !state.state().observation.ready || !state.can_admit_intention() {
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
        if state.can_cancel_sequence() {
            hint.push_str(" move to cancel and step;");
        }
        hint.push_str(if state.has_control() {
            " F3/R control; F2 history; Esc quit"
        } else {
            " F3 acquire control; F2 history; Esc quit"
        });
        Some(hint)
    }

    pub fn disconnect(&mut self, message: String) {
        self.charts.clear();
        self.charts_open = false;
        self.projectile_selection = None;
        self.close_advancement();
        self.travel_cursor = None;
        self.place_name = None;
        self.places_open = false;
        self.connected = false;
        self.busy = false;
        self.inspection = None;
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.wizard_command = None;
        self.stats_open = false;
        self.note = None;
        self.pickup.clear();
        self.item_operation = None;
        self.zap_selection = None;
        self.identification_selected = 0;
        self.attack_targets.clear();
        self.ability_choices.clear();
        self.hold_choices.clear();
        self.selected_ability = None;
        self.supply_selection = None;
        self.weapon_selection = None;
        self.selected_weapon = WeaponSelection::Default;
        self.door_direction = None;
        self.weapon_confirmation = None;
        self.status = message;
    }

    pub fn show_inspection(&mut self, report: &CreatureInspectionView) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Creature inspection requires wizard authority".into());
        }
        let context = self
            .state
            .as_ref()
            .ok_or("No current attachment")?
            .snapshot()
            .reply_context();
        self.strike_pages.set_report(report, &context)?;
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        let rows = tor_client_common::inspection::lines(report)
            .map_err(|error| error.to_string())?
            .into_iter()
            .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
            .collect();
        self.inspection_title = "CREATURE INSPECTION";
        self.inspection = Some(rows);
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn bind_strike_request(&mut self, request_id: &str) {
        self.strike_pages.bind_request(request_id);
    }

    pub fn show_strike_page(
        &mut self,
        context: &ReplyContext,
        request_id: &str,
        page: &CreatureStrikePage,
    ) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Strike inspection requires wizard authority".into());
        }
        if !self.stats_open || !(self.strike_sources_open || self.strike_detail_open) {
            self.strike_pages.clear();
            return Err("Strike detail is no longer open.".into());
        }
        let current = self
            .state
            .as_ref()
            .ok_or("No current attachment")?
            .snapshot()
            .reply_context();
        if &current != context {
            self.strike_pages.clear();
            return Err("Strike detail is stale. Reopen the creature inspection.".into());
        }
        self.strike_pages.accept(context, request_id, page)?;
        self.strike_detail_open = true;
        self.strike_sources_open = false;
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
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        let rows = tor_client_common::combat_diagnostics::lines(report)
            .map_err(|error| error.to_string())?
            .into_iter()
            .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
            .collect();
        self.inspection_title = "CREATURE INSPECTION";
        self.inspection = Some(rows);
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn show_creature_catalog(&mut self, page: &CreatureCatalogPage) -> Result<(), String> {
        if self.role != AccessRole::Wizard {
            return Err("Creature catalog requires wizard authority".into());
        }
        let rows = tor_client_common::creature_catalog::lines(page)
            .map_err(|error| error.to_string())?
            .into_iter()
            .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
            .collect();
        self.strike_pages.clear();
        self.strike_sources_open = false;
        self.strike_detail_open = false;
        self.inspection_title = "CREATURE CATALOG";
        self.inspection = Some(rows);
        self.stats_open = true;
        self.stats_scroll = 0;
        Ok(())
    }

    pub fn stats_title(&self) -> &'static str {
        if self.has_inspection() {
            self.inspection_title
        } else {
            "CREATURE STATS"
        }
    }

    pub fn has_inspection(&self) -> bool {
        self.inspection.is_some()
    }

    pub fn stats_rows(&self) -> std::borrow::Cow<'_, [String]> {
        if self.strike_sources_open {
            let mut rows =
                vec!["Strike sources: Up/Down choose; Enter opens; Escape returns".into()];
            rows.extend(
                self.strike_pages
                    .sources()
                    .iter()
                    .enumerate()
                    .map(|(index, source)| {
                        format!(
                            "{} {}",
                            if index == self.strike_source_selected {
                                ">"
                            } else {
                                " "
                            },
                            source.label
                        )
                    }),
            );
            return std::borrow::Cow::Owned(rows);
        }
        if self.strike_detail_open {
            if let Some(rows) = self.strike_pages.lines() {
                let mut rows: Vec<String> = rows
                    .iter()
                    .flat_map(|line| crate::messages::word_wrap(line, STATS_WIDTH))
                    .collect();
                rows.push("Tab: sources; [: previous page; ]: next page; Escape: back".into());
                return std::borrow::Cow::Owned(rows);
            }
        }
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
                .chain(self.hold_rows())
                .flat_map(|line| crate::messages::word_wrap(&line, STATS_WIDTH))
                .collect(),
        )
    }

    /// Read-only hold rows use only the current observation's disclosed names.
    pub fn hold_rows(&self) -> Vec<String> {
        self.state
            .as_ref()
            .map(|state| render_holds(&state.state().observation))
            .unwrap_or_default()
    }

    /// Native reports retain the wire offer shape and the modal's exact order.
    pub fn hold_choice_offers(&self) -> Vec<LinkedHold> {
        let Some(state) = &self.state else {
            return Vec::new();
        };
        self.hold_choices
            .iter()
            .filter_map(|choice| {
                state
                    .state()
                    .observation
                    .linked_holds
                    .iter()
                    .find(|offer| offer.target == choice.target)
                    .cloned()
            })
            .collect()
    }

    fn escape_hold(&mut self) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if self.role == AccessRole::Spectator || !state.has_control() {
            self.status = "Escaping a hold requires control of your character.".into();
            return Effect::None;
        }
        if !state.can_admit_intention() {
            self.status = "You can't act right now.".into();
            return Effect::None;
        }
        let choices = hold_choices(&state.state().observation);
        match choices.as_slice() {
            [] => {
                self.status = "You are not held.".into();
                Effect::None
            }
            [only] => self.act(Action::Escape { hold: only.target }),
            _ => {
                self.hold_choices = choices;
                self.selected = 0;
                self.status =
                    "Choose a hold. Up/Down select, Enter confirms, Esc or F11 cancels.".into();
                Effect::None
            }
        }
    }

    fn hold_input(&mut self, input: Input) -> Effect {
        match input {
            Input::Key {
                key: Key::Escape | Key::EscapeHold,
            } => {
                self.hold_choices.clear();
                self.status = "Never mind.".into();
            }
            Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
            Input::Key { key: Key::Down } => {
                self.selected = self
                    .selected
                    .saturating_add(1)
                    .min(self.hold_choices.len() - 1);
            }
            Input::Key { key: Key::Enter } if self.connected && !self.busy => {
                let hold = self
                    .hold_choices
                    .get(self.selected)
                    .map(|choice| choice.target);
                self.hold_choices.clear();
                let Some(hold) = hold else {
                    return Effect::None;
                };
                if self.role == AccessRole::Spectator {
                    self.status = "Spectator access is read-only.".into();
                    return Effect::None;
                }
                if self.state.as_ref().is_some_and(|state| {
                    state
                        .state()
                        .observation
                        .linked_holds
                        .iter()
                        .any(|offer| offer.target == hold)
                }) {
                    return self.act(Action::Escape { hold });
                }
                self.status = "That hold is no longer available.".into();
            }
            _ => {}
        }
        Effect::None
    }

    fn select_targets(&mut self, ability: Option<AbilityView>) {
        self.selected_ability = ability;
        self.attack_targets = self
            .state
            .as_ref()
            .map(|state| {
                let view = &state.state().observation;
                let mut targets: Vec<_> = view
                    .visible_actors
                    .iter()
                    .filter(|actor| actor.id != view.self_target)
                    .filter(|actor| {
                        self.selected_ability.as_ref().is_none_or(|ability| {
                            tor_client_common::abilities::target_eligible(view, ability, actor.id)
                        })
                    })
                    .cloned()
                    .collect();
                if self.selected_ability.as_ref().is_some_and(|ability| {
                    tor_client_common::abilities::target_eligible(view, ability, view.self_target)
                }) {
                    targets.insert(
                        0,
                        ActorView {
                            appearance_seed: None,
                            asset: None,
                            id: view.self_target,
                            name: "yourself".into(),
                            description: String::new(),
                            position: view.position,
                        },
                    );
                }
                targets
            })
            .unwrap_or_default();
        let mut seen = std::collections::BTreeSet::new();
        self.attack_targets.retain(|actor| seen.insert(actor.id));
        self.selected = 0;
        self.status = self.target_prompt();
        if self.attack_targets.is_empty() {
            self.selected_ability = None;
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
        }
    }

    fn target_prompt(&self) -> String {
        let Some(target) = self.attack_targets.get(self.selected) else {
            return "No target is visible.".into();
        };
        let operation = self.selected_ability.as_ref().map_or_else(
            || "Attack".to_owned(),
            |ability| format!("Use {} on", ability.name),
        );
        format!(
            "{operation} {}{}? Up/Down select, {}Enter confirms, Esc cancels.",
            target.name,
            self.selected_weapon_label(),
            if self.weapon_selection_available() {
                "Tab chooses a weapon, "
            } else {
                ""
            }
        )
    }

    fn selected_weapon_label(&self) -> String {
        let WeaponSelection::Equipped { item, socket } = self.selected_weapon else {
            return String::new();
        };
        self.state
            .as_ref()
            .and_then(|state| {
                tor_client_common::weapons::choices(&state.state().observation)
                    .into_iter()
                    .find(|row| row.item == item && row.socket == socket)
            })
            .map_or_else(
                || " with a stale weapon selection".into(),
                |row| format!(" with {}", row.name),
            )
    }

    pub(crate) fn weapon_selection_available(&self) -> bool {
        self.ability_choices
            .get(self.selected)
            .or(self.selected_ability.as_ref())
            .map_or_else(
                || {
                    !self.attack_targets.is_empty()
                        && self.state.as_ref().is_some_and(|state| {
                            tor_client_common::abilities::available(&state.state().observation)
                                .iter()
                                .any(|ability| ability.basic_attack && ability.weapon_selection)
                        })
                },
                |ability| ability.weapon_selection,
            )
    }

    fn open_weapon_picker(&mut self) -> Effect {
        if !self.weapon_selection_available() {
            self.status = "That ability does not select a weapon.".into();
            return Effect::None;
        }
        let Some(state) = self.state.as_ref().filter(|state| state.has_control()) else {
            return Effect::None;
        };
        let choices = tor_client_common::weapons::choices(&state.state().observation);
        let selected = choices
            .iter()
            .position(|row| {
                self.selected_weapon
                    == WeaponSelection::Equipped {
                        item: row.item,
                        socket: row.socket,
                    }
            })
            .map_or(0, |index| index + 1);
        self.weapon_selection = Some(WeaponDraft {
            choices,
            previous_selected: self.selected,
            previous_status: self.status.clone(),
        });
        self.selected = selected;
        self.status =
            "Choose one weapon for this action. Enter selects; Esc keeps the previous choice."
                .into();
        Effect::None
    }
    pub fn weapon_rows(&self) -> Vec<String> {
        let Some(draft) = &self.weapon_selection else {
            return Vec::new();
        };
        let mut rows = vec![format!(
            "{} Default primary weapon or natural attack",
            if self.selected == 0 { ">" } else { " " }
        )];
        rows.extend(draft.choices.iter().enumerate().map(|(index, row)| {
            format!(
                "{} {} ({}, socket {})",
                if self.selected == index + 1 { ">" } else { " " },
                row.name,
                row.equipped,
                row.socket
            )
        }));
        rows
    }
    fn weapon_input(&mut self, input: Input) -> Effect {
        let Some(draft) = self.weapon_selection.take() else {
            return Effect::None;
        };
        match input {
            Input::Key { key: Key::Escape } => {
                self.selected = draft.previous_selected;
                self.status = draft.previous_status;
                return Effect::None;
            }
            Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
            Input::Key { key: Key::Down } => {
                self.selected = (self.selected + 1).min(draft.choices.len())
            }
            Input::Key { key: Key::Enter } => {
                let selected = if self.selected == 0 {
                    WeaponSelection::Default
                } else {
                    let row = &draft.choices[self.selected - 1];
                    WeaponSelection::Equipped {
                        item: row.item,
                        socket: row.socket,
                    }
                };
                if let Some(state) = &self.state {
                    match tor_client_common::weapons::validate(&state.state().observation, selected)
                    {
                        Ok(()) => {
                            self.selected_weapon = selected;
                            self.selected = draft.previous_selected;
                            self.status = if self.attack_targets.is_empty() {
                                format!("Weapon selected. {}", draft.previous_status)
                            } else {
                                self.target_prompt()
                            };
                            return Effect::None;
                        }
                        Err(reason) => self.status = reason,
                    }
                }
            }
            _ => {}
        }
        self.weapon_selection = Some(draft);
        Effect::None
    }

    fn choose_ability_target(&mut self, ability: AbilityView, target: ActorTarget) -> Effect {
        self.choose_ability_invocation(
            ability,
            tor_protocol::AbilityInvocation {
                aim: tor_protocol::AbilityAim::ActorTarget { target },
                weapon: tor_protocol::WeaponSelection::Default,
            },
        )
    }
    fn choose_ability_invocation(
        &mut self,
        ability: AbilityView,
        mut invocation: AbilityInvocation,
    ) -> Effect {
        invocation.weapon = std::mem::replace(&mut self.selected_weapon, WeaponSelection::Default);
        let Some(state) = self.state.as_ref() else {
            return Effect::None;
        };
        if !ability.supply_costs.is_empty() {
            let candidates = tor_client_common::abilities::supply_candidates(
                &state.state().observation,
                &ability,
            );
            self.supply_selection = Some(SupplyDraft {
                label: tor_client_common::abilities::ability_label(&ability),
                costs: ability.supply_costs.clone(),
                purpose: SupplyPurpose::Ability {
                    ability: Box::new(ability),
                    invocation,
                },
                candidates,
                checked: Default::default(),
            });
            self.selected = 0;
            self.status = "Choose carried inventory automatically, or exact supplies. Space toggles; Enter confirms; Esc cancels.".into();
            return Effect::None;
        }
        match tor_client_common::abilities::invocation_action(
            &state.state().observation,
            &ability.id,
            invocation,
            SupplySelection::Carried,
        ) {
            Ok(action) => self.act(action),
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    pub fn supply_rows(&self) -> Vec<String> {
        let Some(draft) = &self.supply_selection else {
            return Vec::new();
        };
        let mut rows = vec![
            draft.label.clone(),
            format!(
                "{} Automatic carried inventory",
                if self.selected == 0 { ">" } else { " " }
            ),
        ];
        for (index, row) in draft.candidates.iter().enumerate() {
            rows.push(format!(
                "{} [{}] {} x{} ({})",
                if self.selected == index + 1 { ">" } else { " " },
                if draft.checked.contains(&row.item) {
                    "x"
                } else {
                    " "
                },
                row.name,
                row.quantity,
                if row.carried { "carried" } else { "ground" }
            ));
        }
        rows
    }

    fn supply_input(&mut self, input: Input) -> Effect {
        let Some(mut draft) = self.supply_selection.take() else {
            return Effect::None;
        };
        match input {
            Input::Key { key: Key::Escape } => {
                self.status = "Never mind.".into();
                return Effect::None;
            }
            Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
            Input::Key { key: Key::Down } => {
                self.selected = (self.selected + 1).min(draft.candidates.len())
            }
            Input::Key { key: Key::Wait } => {
                if self.selected == 0 {
                    draft.checked.clear();
                } else if let Some(row) = draft.candidates.get(self.selected - 1) {
                    if !draft.checked.remove(&row.item) {
                        if draft.checked.len() < tor_protocol::MAX_SUPPLY_FRAGMENTS {
                            draft.checked.insert(row.item);
                        } else {
                            self.status = "At most 32 exact supply stacks can be selected.".into();
                        }
                    }
                }
            }
            Input::Key { key: Key::Enter } => {
                if let Some(state) = self.state.as_ref() {
                    let view = &state.state().observation;
                    let selection = if self.selected == 0 {
                        Ok(SupplySelection::Carried)
                    } else {
                        if draft.checked.is_empty() {
                            if let Some(row) = draft.candidates.get(self.selected - 1) {
                                draft.checked.insert(row.item);
                            }
                        }
                        tor_client_common::abilities::selection_from_items_for_costs(
                            view,
                            &draft.costs,
                            &draft.checked.iter().copied().collect::<Vec<_>>(),
                        )
                    };
                    let command = selection.and_then(|selection| match &draft.purpose {
                        SupplyPurpose::Ability {
                            ability,
                            invocation,
                        } => tor_client_common::abilities::invocation_action(
                            view,
                            &ability.id,
                            *invocation,
                            selection,
                        )
                        .map(|action| Command::Act {
                            expected_revision: state.state().revision,
                            action,
                        }),
                        SupplyPurpose::Respec => self.advancement_pages.confirm(
                            &state.snapshot().reply_context(),
                            view,
                            selection,
                        ),
                    });
                    match command {
                        Ok(command) => {
                            self.last_move = false;
                            return match command {
                                Command::Act { action, .. } => self.act(action),
                                other => self.command(other),
                            };
                        }
                        Err(reason) => self.status = reason,
                    }
                }
            }
            _ => {}
        }
        self.supply_selection = Some(draft);
        Effect::None
    }

    fn close_advancement(&mut self) {
        self.advancement_open = false;
        self.advancement_command.clear();
        self.advancement_scroll = 0;
        self.advancement_pending = None;
        self.advancement_pages.clear();
        if self
            .supply_selection
            .as_ref()
            .is_some_and(|draft| matches!(draft.purpose, SupplyPurpose::Respec))
        {
            self.supply_selection = None;
            self.weapon_selection = None;
            self.selected_weapon = WeaponSelection::Default;
        }
    }

    pub fn advancement_rows(&self) -> Vec<String> {
        let rows = self.state.as_ref().map_or_else(Vec::new, |state| {
            self.advancement_pages.display(&state.state().observation)
        });
        rows.iter()
            .flat_map(|line| crate::messages::word_wrap(line, STATS_WIDTH))
            .collect()
    }

    pub fn bind_advancement_request(&mut self, id: &str) -> Result<(), String> {
        let Some((revision, query)) = self.advancement_pending.take() else {
            return Err("Advancement request is no longer open.".into());
        };
        let context = self
            .state
            .as_ref()
            .ok_or("No current attachment.")?
            .snapshot()
            .reply_context();
        self.advancement_pages.track(&context, id, revision, &query)
    }

    pub fn show_advancement_page(
        &mut self,
        context: &ReplyContext,
        id: &str,
        page: &AdvancementPage,
    ) -> Result<(), String> {
        let state = self.state.as_ref().ok_or("No current attachment.")?;
        if !self.advancement_open
            || !state.has_control()
            || self.role == AccessRole::Spectator
            || state.snapshot().reply_context() != *context
        {
            self.close_advancement();
            return Err("Advancement page is no longer current.".into());
        }
        // The network worker has already enforced its negotiated frame limit.
        self.advancement_pages
            .accept(context, id, page, MAX_ADVANCEMENT_PAGE_BYTES)?;
        self.advancement_scroll = 0;
        self.status = "Advancement loaded. Type a command below; Enter submits, Esc closes.".into();
        Ok(())
    }

    fn advancement_action(
        &mut self,
        input: tor_client_common::advancement::AdvancementInput,
    ) -> Effect {
        use tor_client_common::advancement::AdvancementInput;
        self.last_move = false;
        let Some(state) = &self.state else {
            return Effect::None;
        };
        let choose_ration = matches!(input, AdvancementInput::Confirm(None));
        match self.advancement_pages.request(input, state) {
            Ok(Some(request)) => {
                if choose_ration
                    && matches!(
                        &request,
                        Request::Command {
                            command: Command::Act {
                                action: Action::Respec { .. },
                                ..
                            },
                            ..
                        }
                    )
                {
                    match self.advancement_pages.supply_costs() {
                        Ok(costs) => {
                            self.supply_selection = Some(SupplyDraft {
                                purpose: SupplyPurpose::Respec,
                                label: "Confirm this respec with one ration. Choose carried or one exact source.".into(),
                                candidates: self.advancement_pages.supply_candidates(&state.state().observation),
                                costs,
                                checked: Default::default(),
                            });
                            self.selected = 0;
                            self.status = "Choose the ration source; Enter confirms the exact retained respec.".into();
                        }
                        Err(error) => self.status = error,
                    }
                    return Effect::None;
                }
                if let Request::Command {
                    command:
                        Command::Advancement {
                            expected_revision,
                            query,
                        },
                    ..
                } = &request
                {
                    self.advancement_pending = Some((*expected_revision, query.clone()));
                }
                self.request(request)
            }
            Ok(None) => {
                self.advancement_scroll = 0;
                Effect::None
            }
            Err(error) => {
                self.status = error;
                Effect::None
            }
        }
    }

    fn advancement_input(&mut self, input: Input) -> Effect {
        use tor_client_common::advancement::{parse, AdvancementInput};
        if matches!(
            input,
            Input::Key {
                key: Key::Escape | Key::Advancement
            }
        ) {
            self.close_advancement();
            return Effect::None;
        }
        if self.busy || !self.connected {
            return Effect::None;
        }
        match input {
            Input::Text { text } => {
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    if self.advancement_command.len() + ch.len_utf8() <= 512 {
                        self.advancement_command.push(ch);
                    }
                }
            }
            Input::Key {
                key: Key::Backspace,
            } => {
                self.advancement_command.pop();
            }
            Input::Key { key: Key::Enter } => {
                let text = std::mem::take(&mut self.advancement_command);
                match parse(&text) {
                    Ok(input) => return self.advancement_action(input),
                    Err(error) => self.status = error,
                }
            }
            Input::Key { key: Key::Up } => {
                self.advancement_scroll = self.advancement_scroll.saturating_sub(1)
            }
            Input::Key { key: Key::Down } => {
                self.advancement_scroll = (self.advancement_scroll + 1).min(
                    self.advancement_rows()
                        .len()
                        .saturating_sub(ADVANCEMENT_ROWS),
                )
            }
            Input::Key {
                key: Key::OlderHistory,
            } => return self.advancement_action(AdvancementInput::Previous),
            Input::Key {
                key: Key::RecentHistory,
            } => return self.advancement_action(AdvancementInput::Next),
            Input::Key {
                key: Key::ResumeIntention,
            } => return self.advancement_action(AdvancementInput::Resume),
            Input::Key {
                key: Key::CancelIntention,
            } => return self.advancement_action(AdvancementInput::Cancel),
            _ => {}
        }
        Effect::None
    }

    /// The owner-only paid choice currently disclosed by the server.
    pub fn identification_choice(&self) -> Option<&IdentificationChoiceView> {
        let state = self.state.as_ref()?;
        if !self.connected || self.role == AccessRole::Spectator || !state.has_control() {
            return None;
        }
        state
            .state()
            .observation
            .interactions
            .as_ref()?
            .identification
            .as_ref()
    }

    /// Exact inventory entries in the server's category-specific offer order.
    fn identification_entries(&self) -> Vec<InventoryEntry> {
        let Some(choice) = self.identification_choice() else {
            return vec![];
        };
        let view = &self
            .state
            .as_ref()
            .expect("current choice")
            .state()
            .observation;
        let entries = inventory_entries(self, view);
        choice
            .items
            .iter()
            .filter_map(|target| {
                view.inventory
                    .iter()
                    .position(|item| item.id == *target)
                    .and_then(|index| entries.get(index).cloned())
            })
            .collect()
    }

    pub fn identification_rows(&self) -> Vec<String> {
        self.identification_entries()
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                format!(
                    "{} {}",
                    if index == self.identification_selected {
                        ">"
                    } else {
                        " "
                    },
                    entry.long()
                )
            })
            .collect()
    }

    /// Aiming never outlives the exact observation where the device was selected.
    pub fn zap_item(&self) -> Option<ItemTarget> {
        let (item, context) = self.zap_selection.as_ref()?;
        let state = self.state.as_ref()?;
        (self.connected
            && self.role != AccessRole::Spectator
            && state.has_control()
            && state.snapshot().reply_context() == *context)
            .then_some(*item)
    }

    fn identification_input(&mut self, input: Input) -> Effect {
        if self.busy || !self.connected {
            return Effect::None;
        }
        let targets: Vec<_> = self
            .identification_choice()
            .map(|choice| choice.items.clone())
            .unwrap_or_default();
        let selected = match input {
            Input::Key { key: Key::Escape } => Some(None),
            Input::Key { key: Key::Up } => {
                self.identification_selected = self.identification_selected.saturating_sub(1);
                None
            }
            Input::Key { key: Key::Down } => {
                self.identification_selected =
                    (self.identification_selected + 1).min(targets.len().saturating_sub(1));
                None
            }
            Input::Key { key: Key::Enter } => {
                targets.get(self.identification_selected).copied().map(Some)
            }
            Input::Text { text } => {
                text.chars()
                    .find(char::is_ascii_alphabetic)
                    .and_then(|letter| {
                        let item = targets
                            .iter()
                            .copied()
                            .find(|target| self.letters.get(target) == Some(&letter));
                        if item.is_none() {
                            self.status = format!("No offered item is lettered {letter}.");
                        }
                        item.map(Some)
                    })
            }
            _ => None,
        };
        let Some(item) = selected else {
            return Effect::None;
        };
        match identification_action(
            &self
                .state
                .as_ref()
                .expect("current choice")
                .state()
                .observation,
            item,
        ) {
            Ok(action) => self.act(action),
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    fn zap_input(&mut self, input: Input) -> Effect {
        if self.busy || !self.connected {
            return Effect::None;
        }
        if matches!(input, Input::Key { key: Key::Escape }) {
            self.zap_selection = None;
            self.status = "Never mind.".into();
            return Effect::None;
        }
        let Input::Key { key } = input else {
            return Effect::None;
        };
        let direction = key.step_direction();
        if direction.is_none() && key != Key::Wait {
            return Effect::None;
        }
        let Some(item) = self.zap_item() else {
            self.zap_selection = None;
            self.status = "That device selection is no longer current.".into();
            return Effect::None;
        };
        let view = &self
            .state
            .as_ref()
            .expect("current device")
            .state()
            .observation;
        let result = if let Some(direction) = direction {
            zap_action(view, item, direction)
        } else {
            tor_client_common::items::zap_self_action(view, item)
        };
        self.zap_selection = None;
        match result {
            Ok(action) => self.act(action),
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    /// A delayed local timeout cannot cancel another view or selected sample.
    pub fn chart_cancelled(&mut self, query: &chart_pages::ChartQuery) {
        let Some(state) = &self.state else {
            return;
        };
        if query.context != *state.context()
            || query.branch != *state.branch()
            || query.base != state.observation_base()
            || !state.state().observation.charts.iter().any(|summary| {
                Some(summary.kind) == self.charts.selected()
                    && summary.chart.as_ref() == Some(&query.chart)
            })
        {
            return;
        }
        self.charts.clear();
        self.chart_selected = 0;
        self.chart_scroll = 0;
        self.status = "Chart page timed out; select the chart again to retry.".into();
    }

    pub fn chart_lines(&self) -> Vec<String> {
        let Some(state) = &self.state else {
            return vec!["No chart view is attached.".into()];
        };
        let mut lines = Vec::new();
        if self.charts.selected().is_none() {
            for (index, summary) in state.state().observation.charts.iter().enumerate() {
                lines.push(format!(
                    "{} {}",
                    if index == self.chart_selected {
                        ">"
                    } else {
                        " "
                    },
                    tor_client_common::charts::kind_label(summary.kind)
                ));
            }
        }
        lines.extend(self.charts.lines(state));
        lines
            .into_iter()
            .flat_map(|line| crate::wrap(&line, 96))
            .collect()
    }
    fn chart_input(&mut self, input: Input) -> Effect {
        use tor_client_common::charts::ChartInput as Chart;
        let Input::Key { key } = input else {
            return Effect::None;
        };
        if matches!(key, Key::Escape | Key::Charts) {
            self.charts_open = false;
            if let Some(state) = &self.state {
                let _ = self.charts.request(Chart::Cancel, state);
            }
            return Effect::CancelCharts;
        }
        let Some(state) = &self.state else {
            return Effect::None;
        };
        let command = match key {
            Key::Tab => {
                self.chart_scroll = 0;
                Some(Chart::Overview)
            }
            Key::Up if self.charts.selected().is_none() => {
                self.chart_selected = self.chart_selected.saturating_sub(1);
                None
            }
            Key::Down if self.charts.selected().is_none() => {
                self.chart_selected = (self.chart_selected + 1)
                    .min(state.state().observation.charts.len().saturating_sub(1));
                None
            }
            Key::Up => {
                self.chart_scroll = self.chart_scroll.saturating_sub(1);
                None
            }
            Key::Down => {
                self.chart_scroll =
                    (self.chart_scroll + 1).min(self.chart_lines().len().saturating_sub(24));
                None
            }
            Key::OlderHistory => {
                self.chart_scroll = 0;
                Some(Chart::Previous)
            }
            Key::RecentHistory => {
                self.chart_scroll = 0;
                Some(Chart::Next)
            }
            Key::Enter if self.charts.selected().is_none() => state
                .state()
                .observation
                .charts
                .get(self.chart_selected)
                .map(|s| Chart::Select(s.kind)),
            _ => None,
        };
        if let Some(command) = command {
            match self.charts.request(command, state) {
                Ok(Some(request)) => return Effect::Request(Box::new(request)),
                Ok(None) => {}
                Err(reason) => self.status = reason,
            }
        }
        // Every unhandled key/click is swallowed here, never navigation or an action.
        Effect::None
    }

    pub fn input(&mut self, input: Input) -> Effect {
        if self.charts_open {
            return self.chart_input(input);
        }
        if self.projectile_selection.is_some() {
            return self.projectile_input(input);
        }
        if self.weapon_confirmation.is_some() {
            return self.weapon_confirmation_input(input);
        }
        if self.weapon_selection.is_some() {
            return self.weapon_input(input);
        }
        if self.identification_choice().is_some() {
            return self.identification_input(input);
        }
        if self.zap_selection.is_some() {
            return self.zap_input(input);
        }
        if !self.hold_choices.is_empty() {
            return self.hold_input(input);
        }
        if self.supply_selection.is_some() {
            return self.supply_input(input);
        }
        if self.advancement_open {
            return self.advancement_input(input);
        }
        if self.stats_open {
            if let Input::Key { key } = &input {
                if *key == Key::Tab && !self.strike_pages.sources().is_empty() {
                    self.strike_sources_open = true;
                    self.strike_detail_open = false;
                    self.strike_source_selected = 0;
                    self.stats_scroll = 0;
                    return Effect::None;
                }
                if *key == Key::Escape && (self.strike_sources_open || self.strike_detail_open) {
                    if self.busy {
                        self.strike_pages.clear();
                        self.inspection = None;
                        self.strike_sources_open = false;
                        self.strike_detail_open = false;
                        self.stats_open = false;
                        return Effect::None;
                    }
                    if self.strike_detail_open {
                        self.strike_detail_open = false;
                        self.strike_sources_open = true;
                    } else {
                        self.strike_sources_open = false;
                    }
                    self.stats_scroll = 0;
                    return Effect::None;
                }
                if self.strike_sources_open {
                    match key {
                        Key::Up => {
                            self.strike_source_selected =
                                self.strike_source_selected.saturating_sub(1)
                        }
                        Key::Down => {
                            self.strike_source_selected = (self.strike_source_selected + 1)
                                .min(self.strike_pages.sources().len().saturating_sub(1))
                        }
                        Key::Enter if !self.busy => {
                            if let (Some(state), Some(source)) = (
                                &self.state,
                                self.strike_pages.sources().get(self.strike_source_selected),
                            ) {
                                let source = source.source.clone();
                                let context = state.snapshot().reply_context();
                                match self.strike_pages.select(source, &context) {
                                    Ok(command) => return self.command(command),
                                    Err(error) => self.status = error,
                                }
                            }
                        }
                        _ => {}
                    }
                    return Effect::None;
                }
                if self.strike_detail_open && matches!(key, Key::Slower | Key::Faster) && !self.busy
                {
                    if let Some(state) = &self.state {
                        let context = state.snapshot().reply_context();
                        let query = if *key == Key::Slower {
                            self.strike_pages.previous(&context)
                        } else {
                            self.strike_pages.next(&context)
                        };
                        match query {
                            Ok(command) => return self.command(command),
                            Err(error) => self.status = error,
                        }
                    }
                    return Effect::None;
                }
            }
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
        if matches!(input, Input::Key { key: Key::Tab })
            && (!self.ability_choices.is_empty()
                || !self.attack_targets.is_empty()
                || self.selected_ability.is_some())
        {
            return self.open_weapon_picker();
        }
        if !self.ability_choices.is_empty() {
            if let Input::Key { key } = input {
                match key {
                    Key::Escape => {
                        self.ability_choices.clear();
                        self.selected_weapon = WeaponSelection::Default;
                        self.status = "Never mind.".into();
                    }
                    Key::Up | Key::Down => {
                        let next = if key == Key::Up {
                            self.selected.saturating_sub(1)
                        } else {
                            self.selected
                                .saturating_add(1)
                                .min(self.ability_choices.len() - 1)
                        };
                        if next != self.selected && self.selected_weapon != WeaponSelection::Default
                        {
                            self.selected_weapon = WeaponSelection::Default;
                            self.status = "Ability changed; using its default weapon.".into();
                        }
                        self.selected = next;
                    }
                    Key::Enter => {
                        let ability = self.ability_choices[self.selected].clone();
                        self.ability_choices.clear();
                        if self.state.as_ref().is_some_and(|state| {
                            tor_client_common::abilities::choices(&state.state().observation)
                                .contains(&ability)
                        }) {
                            if ability.target_kind == AbilityTargetKind::DirectionalProjectile {
                                self.selected_weapon = WeaponSelection::Default;
                                return self.open_projectile_picker(&ability.id);
                            }
                            if matches!(
                                ability.target_kind,
                                AbilityTargetKind::SelfStep | AbilityTargetKind::DirectionalRay
                            ) {
                                self.status = format!(
                                    "Choose a direction for {}. Movement keys select; {}Esc cancels.",
                                    ability.name,
                                    if ability.weapon_selection { "Tab chooses a weapon; " } else { "" }
                                );
                                self.selected_ability = Some(ability);
                                return Effect::None;
                            }
                            if ability.target_kind == AbilityTargetKind::SelfOnly {
                                let target =
                                    self.state.as_ref().unwrap().state().observation.self_target;
                                return self.choose_ability_target(ability, target);
                            }
                            self.select_targets(Some(ability));
                        } else {
                            self.status = format!("You don't have {}.", ability.name);
                        }
                    }
                    _ => {}
                }
            }
            return Effect::None;
        }
        if self.selected_ability.as_ref().is_some_and(|ability| {
            matches!(
                ability.target_kind,
                AbilityTargetKind::SelfStep | AbilityTargetKind::DirectionalRay
            )
        }) {
            if let Input::Key { key } = input {
                if key == Key::Escape {
                    self.selected_ability = None;
                    self.selected_weapon = WeaponSelection::Default;
                    self.status = "Never mind.".into();
                } else if let Some(direction) = key.step_direction() {
                    let ability = self
                        .selected_ability
                        .take()
                        .expect("selected directional ability");
                    let invocation = if ability.target_kind == AbilityTargetKind::DirectionalRay {
                        tor_protocol::AbilityInvocation {
                            aim: tor_protocol::AbilityAim::DirectionalRay { direction },
                            weapon: tor_protocol::WeaponSelection::Default,
                        }
                    } else {
                        tor_protocol::AbilityInvocation {
                            aim: tor_protocol::AbilityAim::SelfStep { direction },
                            weapon: tor_protocol::WeaponSelection::Default,
                        }
                    };
                    return self.choose_ability_invocation(ability, invocation);
                }
            }
            return Effect::None;
        }
        if !self.attack_targets.is_empty() {
            match input {
                Input::Key { key: Key::Escape } => {
                    self.attack_targets.clear();
                    self.selected_ability = None;
                    self.supply_selection = None;
                    self.weapon_selection = None;
                    self.selected_weapon = WeaponSelection::Default;
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
                        return self.choose_ability_target(ability, target);
                    }
                    let weapon =
                        std::mem::replace(&mut self.selected_weapon, WeaponSelection::Default);
                    let action = self.state.as_ref().map(|state| {
                        tor_client_common::abilities::basic_action_with_weapon(
                            &state.state().observation,
                            target,
                            weapon,
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
                    | Key::Charts
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
                        if operation == ItemOperation::Zap {
                            let state = self.state.as_ref().expect("connected state");
                            if !item_choices(&state.state().observation, operation)
                                .any(|candidate| candidate.id == item)
                            {
                                self.pickup.clear();
                                self.status = "That device selection is no longer current.".into();
                                return Effect::None;
                            }
                            if let Ok(action) =
                                item_action(&state.state().observation, item, operation)
                            {
                                self.pickup.clear();
                                self.zap_selection = None;
                                return self.act(action);
                            }
                            self.zap_selection = Some((item, state.snapshot().reply_context()));
                            self.pickup.clear();
                            let dual = state
                                .state()
                                .observation
                                .interactions
                                .as_ref()
                                .and_then(|interactions| {
                                    interactions.inventory.iter().find(|row| row.item == item)
                                })
                                .is_some_and(|row| {
                                    row.device_activation
                                        == Some(tor_protocol::DeviceActivation::DirectionOrSelf)
                                });
                            self.status = if dual { "Zap in which direction? Movement keys select; . targets yourself; Esc cancels." }
                                else { "Zap in which direction? Movement keys select; Esc cancels." }.into();
                            return Effect::None;
                        }
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
                        crate::render::map_tiles(s, &self.palette)
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
                        let examination = state
                            .state()
                            .observation
                            .visible_cells
                            .iter()
                            .filter(|cell| {
                                (cell.position.x, cell.position.y) == (cursor.x, cursor.y)
                            })
                            .filter_map(|cell| cell.trap.as_ref())
                            .find(|trap| trap.reachable && trap.family.is_none())
                            .map(|trap| trap.id);
                        if let Some(trap) = examination {
                            return self.act(Action::ExamineTrap { trap });
                        }
                        let text = crate::describe(state, &self.palette, cursor.x, cursor.y);
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
            Key::EscapeHold => self.escape_hold(),
            Key::Advancement => {
                let Some(state) = &self.state else {
                    return Effect::None;
                };
                if !state.has_control() || self.role == AccessRole::Spectator {
                    self.status = "Advancement requires control of your character.".into();
                    return Effect::None;
                }
                self.advancement_open = true;
                self.advancement_command.clear();
                self.advancement_scroll = 0;
                self.advancement_action(tor_client_common::advancement::AdvancementInput::Overview)
            }
            Key::Look => {
                if let Some(state) = &self.state {
                    self.look_cursor = Some(state.state().observation.position);
                    self.status = "Look at what? hjklyubn moves, . or ; picks, Esc cancels.".into();
                }
                Effect::None
            }
            Key::Charts => {
                self.charts_open = true;
                self.chart_selected = 0;
                self.chart_scroll = 0;
                if let Some(state) = &self.state {
                    let _ = self
                        .charts
                        .request(tor_client_common::charts::ChartInput::Overview, state);
                }
                Effect::None
            }
            Key::Inventory => {
                self.inventory_open = true;
                Effect::None
            }
            Key::Stats => {
                self.inspection = None;
                self.strike_pages.clear();
                self.strike_sources_open = false;
                self.strike_detail_open = false;
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
                self.selected_weapon = WeaponSelection::Default;
                self.select_targets(None);
                Effect::None
            }
            Key::Abilities => {
                self.selected_weapon = WeaponSelection::Default;
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
                        "Choose an ability. Up/Down select, Tab chooses a weapon if permitted, Enter confirms, Esc cancels.".into()
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
                    // One reachable stack is taken whole; the server checks free units.
                    // Dropping and choosing among multiple stacks always ask.
                    [item] if !self.dropping => self.act(Action::Take {
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
            Key::Equip
            | Key::Unequip
            | Key::Drink
            | Key::Eat
            | Key::Read
            | Key::Zap
            | Key::Ignite
            | Key::Extinguish => {
                let operation = match key {
                    Key::Equip => ItemOperation::Equip,
                    Key::Unequip => ItemOperation::Unequip,
                    Key::Eat => ItemOperation::Eat,
                    Key::Read => ItemOperation::Read,
                    Key::Zap => ItemOperation::Zap,
                    Key::Ignite => ItemOperation::Ignite,
                    Key::Extinguish => ItemOperation::Extinguish,
                    _ => ItemOperation::Drink,
                };
                let Some(state) = &self.state else {
                    return Effect::None;
                };
                if matches!(
                    operation,
                    ItemOperation::Read
                        | ItemOperation::Zap
                        | ItemOperation::Ignite
                        | ItemOperation::Extinguish
                ) && !state.has_control()
                {
                    self.status = "You are observing. Press F3 to request control.".into();
                    return Effect::None;
                }
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

    fn open_projectile_picker(&mut self, ability: &str) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        match tor_client_common::projectiles::ProjectileMenu::new(state, ability) {
            Ok(menu) => {
                let command = menu.command().expect("new source query");
                self.projectile_selection = Some(menu);
                self.selected = 0;
                self.command(command)
            }
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }
    pub fn bind_projectile_request(&mut self, id: &str) -> Result<(), String> {
        if let Some(menu) = self.projectile_selection.as_mut() {
            menu.bind_request(id)?;
        }
        Ok(())
    }
    pub fn show_projectile_choices(
        &mut self,
        context: &ReplyContext,
        id: &str,
        page: &ProjectileChoicePage,
    ) -> Result<(), String> {
        let Some(menu) = self.projectile_selection.as_mut() else {
            return Ok(());
        };
        menu.accept(
            self.state.as_ref().ok_or("No current observation.")?,
            context,
            id,
            page,
        )?;
        self.selected = 0;
        self.status = "Choose a source. Left/Right pages; Esc cancels.".into();
        Ok(())
    }
    pub fn projectile_rows(&self) -> Vec<String> {
        let Some(menu) = &self.projectile_selection else {
            return vec![];
        };
        let mut rows: Vec<_> = menu
            .rows()
            .into_iter()
            .enumerate()
            .map(|(i, row)| format!("{} {row}", if i == self.selected { ">" } else { " " }))
            .collect();
        if menu.has_previous() {
            rows.push("Left: previous choices".into());
        }
        if menu.has_next() {
            rows.push("Right: more choices".into());
        }
        rows
    }
    fn projectile_input(&mut self, input: Input) -> Effect {
        use tor_client_common::projectiles::Step;
        if matches!(input, Input::Key { key: Key::Escape }) {
            self.projectile_selection = None;
            self.status = "Never mind.".into();
            return Effect::None;
        }
        let Some(state) = self.state.as_ref() else {
            self.projectile_selection = None;
            return Effect::None;
        };
        let menu = self
            .projectile_selection
            .as_mut()
            .expect("projectile picker");
        if !menu.current(state) {
            self.projectile_selection = None;
            self.status = "Projectile choices changed; choose the ability again.".into();
            return Effect::None;
        }
        if self.busy {
            return Effect::None;
        }
        let Input::Key { key } = input else {
            return Effect::None;
        };
        if menu.step() == Step::Direction {
            if let Some(direction) = key.step_direction() {
                let action = menu.action(state, direction);
                self.projectile_selection = None;
                return match action {
                    Ok(action) => self.act(action),
                    Err(reason) => {
                        self.status = reason;
                        Effect::None
                    }
                };
            }
            return Effect::None;
        }
        let command = match key {
            Key::Up => {
                self.selected = self.selected.saturating_sub(1);
                return Effect::None;
            }
            Key::Down => {
                self.selected = (self.selected + 1).min(menu.choices().len().saturating_sub(1));
                return Effect::None;
            }
            Key::Left => menu.previous(state).map(Some),
            Key::Right => menu.next(state).map(Some),
            Key::Enter => menu.choose(state, self.selected),
            _ => return Effect::None,
        };
        match command {
            Ok(Some(command)) => {
                self.selected = 0;
                self.command(command)
            }
            Ok(None) => {
                self.status = "Choose a direction. Movement keys select; Esc cancels.".into();
                Effect::None
            }
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    pub fn bind_weapon_preview(&mut self, id: &str) -> Result<(), String> {
        self.weapon_confirmation
            .as_mut()
            .ok_or("Weapon preview was cancelled.")?
            .bind_request(id)
    }
    pub fn show_weapon_preview(
        &mut self,
        context: &ReplyContext,
        id: &str,
        preview: &AbilityWeaponPreview,
    ) -> Result<(), String> {
        let Some(pending) = self.weapon_confirmation.as_mut() else {
            return Ok(());
        };
        let state = self.state.as_ref().ok_or("No current observation.")?;
        pending.accept(state, context, id, preview)?;
        self.status =
            "Review the exact weapon terms. Enter commits this action; Esc cancels.".into();
        Ok(())
    }
    pub fn weapon_preview_rows(&self) -> Vec<String> {
        self.weapon_confirmation
            .as_ref()
            .map_or_else(Vec::new, |preview| preview.lines())
    }
    fn weapon_confirmation_input(&mut self, input: Input) -> Effect {
        match input {
            Input::Key { key: Key::Escape } => {
                self.weapon_confirmation = None;
                self.status = "Never mind.".into();
            }
            Input::Key { key: Key::Enter }
                if !self.busy
                    && self
                        .weapon_confirmation
                        .as_ref()
                        .is_some_and(|preview| preview.ready()) =>
            {
                let preview = self.weapon_confirmation.take().expect("ready preview");
                let confirmed = self
                    .state
                    .as_ref()
                    .ok_or("No current observation.".into())
                    .and_then(|state| preview.confirm(state));
                match confirmed {
                    Ok(action) => return self.act_confirmed(action),
                    Err(error) => self.status = error,
                }
            }
            _ => {}
        }
        Effect::None
    }
    fn act(&mut self, action: Action) -> Effect {
        if tor_client_common::weapon_preview::explicit_action(&action) {
            let Some(state) = self.state.as_ref() else {
                return Effect::None;
            };
            match tor_client_common::weapon_preview::WeaponPreview::new(state, action) {
                Ok(preview) => {
                    let command = preview.command();
                    self.weapon_confirmation = Some(preview);
                    return self.command(command);
                }
                Err(error) => {
                    self.status = error;
                    return Effect::None;
                }
            }
        }
        self.act_confirmed(action)
    }
    fn act_confirmed(&mut self, mut action: Action) -> Effect {
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
        if let Action::Move { direction } = action {
            if let Some(request) = state.cancel_intention_with_step_request(direction) {
                self.messages.begin_turn();
                self.running = None;
                self.last_move = true;
                return self.request(request);
            }
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
            let columns = crate::map::CreatureColumns::new(view);
            let destination = match (
                view.position.x.checked_add(x),
                view.position.y.checked_add(y),
                view.position.z.checked_add(z),
            ) {
                (Some(x), Some(y), Some(z)) => Some(Position { x, y, z }),
                _ => None,
            };
            let target = destination.and_then(|at| {
                if z == 0 {
                    columns
                        .actor(at.x, at.y)
                        .filter(|actor| columns.near_body(actor.position))
                } else {
                    view.visible_actors
                        .iter()
                        .find(|actor| actor.id != view.self_target && actor.position == at)
                }
            });
            if let Some(target) = target.filter(|actor| match self.bump_attacks {
                BumpAttacks::Any => true,
                BumpAttacks::Off => false,
                BumpAttacks::Hostile => view.combat.as_ref().is_some_and(|c| {
                    c.actors
                        .iter()
                        .any(|other| other.actor == actor.id && other.hostile)
                }),
            }) {
                match tor_client_common::abilities::basic_action(view, target.id) {
                    Ok(attack) => action = attack,
                    Err(reason) => {
                        self.status = reason;
                        return Effect::None;
                    }
                }
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
        Effect::Request(Box::new(request))
    }
}

/// Rows the history screen shows at once.
pub const HISTORY_ROWS: usize = 22;
pub const STATS_ROWS: usize = 24;
pub const STATS_WIDTH: usize = 65;
pub const ADVANCEMENT_ROWS: usize = 16;

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
pub fn glyph_at(o: &Observation, palette: &tor_client_common::Palette, x: i32, y: i32) -> char {
    crate::map::tiles(o, palette, &|_| true)
        .into_iter()
        .find(|t| (t.position.x, t.position.y) == (x, y))
        .map_or(' ', |t| t.glyph)
}

pub fn history_text(entry: &HistoryEntry) -> String {
    match &entry.content {
        HistoryContent::PlaceRenamed { name, .. } => format!("Place named {}.", name),
        HistoryContent::Travel { .. } => "Travel requested.".into(),
        HistoryContent::Wizard { summary, .. } => summary.clone(),
        HistoryContent::Action { event, .. } => match event.as_ref() {
            Event::Moved { direction } => format!("Moved {}.", direction_name(*direction)),
            Event::Escaped { .. } => "Escaped with the objective item.".into(),
            Event::Crumbled { quantity, .. } => {
                if *quantity == 1 {
                    "An item crumbled during pickup.".into()
                } else {
                    format!("{quantity} items crumbled during pickup.")
                }
            }
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
            Event::AbilityStarted { ability, .. } => format!("Prepared {}.", ability.name),
            Event::EscapeStarted { .. } => "Began trying to escape a hold.".into(),
            Event::ItemStarted { action } => match action {
                tor_protocol::Action::Equip { .. } => "Began equipping an item.".into(),
                tor_protocol::Action::Unequip { .. } => "Began removing an item.".into(),
                tor_protocol::Action::Ignite { .. } => "Began igniting an item.".into(),
                tor_protocol::Action::Extinguish { .. } => "Began extinguishing an item.".into(),
                tor_protocol::Action::Drink { .. } => "Began drinking an item.".into(),
                tor_protocol::Action::Eat { .. } => "Began eating an item.".into(),
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
    pub color: u32,
    pub name: String,
    pub quantity: u64,
    /// The slot it's worn or held in, when equipped.
    pub equipped: Option<String>,
    /// Known equipment numbers, when the server discloses them.
    pub stats: Option<String>,
    /// Exact charge count only when disclosed for this carried instance.
    pub charges: Option<u16>,
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
        if let Some(charges) = self.charges {
            text.push_str(&format!("  [charges: {charges}]"));
        }
        text
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
                    .map_or_else(|| "in use".into(), |slot| slot.equipped_phrase.clone())
            });
            let stats = affordance
                .and_then(|a| a.known_equipment.as_ref())
                .map(|known| {
                    let mut parts = Vec::new();
                    if let Some(attack) = &known.attack {
                        parts.push(format!(
                            "attack {:+}; damage enchantment {:+}",
                            attack.bonus, attack.damage_enchantment
                        ));
                    }
                    for modifier in &known.modifiers {
                        parts.extend(tor_client_common::combat_diagnostics::trace_lines(modifier));
                    }
                    parts.join(", ")
                })
                .filter(|text| !text.is_empty());
            let mut parts: Vec<String> = stats.into_iter().collect();
            if let Some(light) = tor_client_common::items::portable_light_details(o, item.id) {
                parts.push(light);
            }
            if let Some(cursed) = affordance.and_then(|value| value.known_cursed) {
                parts.push(if cursed { "cursed" } else { "uncursed" }.into());
            }
            if let Some(protected) = affordance.and_then(|value| value.known_protected) {
                parts.push(
                    if protected {
                        "rust protected"
                    } else {
                        "not rust protected"
                    }
                    .into(),
                );
            }
            let stats = (!parts.is_empty()).then(|| parts.join(", "));
            let appearance = crate::map::item(&app.palette, item);
            InventoryEntry {
                letter: app.letters.get(&item.id).copied().unwrap_or('?'),
                glyph: appearance.glyph,
                color: appearance.color,
                name: item.name.clone(),
                quantity: item.quantity,
                equipped,
                stats,
                charges: affordance.and_then(|affordance| affordance.charges),
            }
        })
        .collect()
}

// A current cell's explicit null is factual; it must not fall back to old art.
fn stair_presentation<'a>(
    current: &'a [CellView],
    memory: impl Iterator<Item = tor_client_common::MapCell<'a>>,
    position: Position,
) -> Option<&'a ObjectPresentation> {
    if let Some(cell) = current.iter().find(|cell| cell.position == position) {
        return cell.object_presentation.as_ref();
    }
    memory
        .into_iter()
        .find(|cell| cell.position == position)
        .and_then(|cell| cell.cell.appearance.object_presentation.as_ref())
}

/// What the look command says about a column.
pub fn describe(
    state: &ClientState,
    palette: &tor_client_common::Palette,
    x: i32,
    y: i32,
) -> String {
    let o = &state.state().observation;
    if (x, y) == (o.position.x, o.position.y) {
        return o
            .visible_cells
            .iter()
            .filter(|cell| cell.position == o.position)
            .find_map(|cell| cell.trap.as_ref())
            .map_or_else(
                || "That's you.".into(),
                |trap| {
                    format!(
                        "That's you. A {} lies underfoot.",
                        trap.family.map_or("trap", TrapFamily::name)
                    )
                },
            );
    }
    if let Some(actor) = crate::map::CreatureColumns::new(o).actor(x, y) {
        let phrase = if actor.name.trim().is_empty() {
            tor_client_common::presentation::resolve(palette, actor.asset.as_deref())
                .map_or("a figure", |entry| entry.noun.indefinite)
                .to_owned()
        } else {
            item_phrase(&actor.name, 1)
        };
        let mut text = format!(
            "{} - {}",
            crate::map::creature(palette, actor.asset.as_deref()).glyph,
            phrase
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
    let tiles = crate::render::map_tiles(state, palette);
    let Some(tile) = tiles
        .iter()
        .find(|t| (t.position.x, t.position.y) == (x, y))
    else {
        return "You don't know what's there.".into();
    };
    if matches!(
        tile.kind,
        crate::map::Kind::StairsUp | crate::map::Kind::StairsDown
    ) {
        if let Some(presentation) =
            stair_presentation(&o.visible_cells, state.map_memory(), tile.position)
        {
            return format!(
                "{} - {}.",
                tile.glyph,
                item_phrase(tor_client_common::items::object_name(presentation), 1)
            );
        }
    }
    let what = match tile.kind {
        crate::map::Kind::Item => {
            let live = o
                .ground_items
                .iter()
                .find(|i| {
                    (i.position.x, i.position.y) == (x, y)
                        && (-2..=0).contains(&(i.position.z - o.position.z))
                })
                .map(|i| (tor_client_common::items::ground_name(i), i.item.quantity));
            let remembered = state
                .map_memory()
                .filter(|c| {
                    (c.position.x, c.position.y) == (x, y)
                        && (-2..=0).contains(&(c.position.z - o.position.z))
                })
                .find_map(|c| c.items().first())
                .map(|i| (tor_client_common::items::map_item_name(i), i.quantity));
            match live.or(remembered) {
                Some((name, quantity)) => item_phrase(name, quantity),
                None => "an object".into(),
            }
        }
        crate::map::Kind::Trap => {
            let current = o
                .visible_cells
                .iter()
                .filter(|cell| (cell.position.x, cell.position.y) == (x, y))
                .find_map(|cell| cell.trap.as_ref().map(|trap| trap.family));
            let remembered = state
                .map_memory()
                .filter(|cell| (cell.position.x, cell.position.y) == (x, y))
                .find_map(|cell| cell.trap.as_ref().map(|trap| trap.family));
            format!(
                "a {}",
                current
                    .or(remembered)
                    .flatten()
                    .map_or("trap", TrapFamily::name)
            )
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

fn sequence_label(combat: &CombatView) -> Option<String> {
    let sequence = combat.sequence.as_ref()?;
    Some(format!(
        "{}:{}/{} Next:{} Cancel:{}",
        if combat.preparation_active {
            "Sequence"
        } else {
            "Sequence paused"
        },
        sequence.completed,
        sequence.total,
        sequence.next_strike_remaining,
        sequence.cancel_recovery_remaining
    ))
}

/// Compact disclosed attributes and location, then health, resource balances,
/// reserved amounts, identified gold, time and conditions.
pub fn status_lines(app: &App, state: &ClientState) -> [String; 2] {
    let o = &state.state().observation;
    let me = o
        .visible_actors
        .iter()
        .find(|a| a.id == o.self_target && !a.name.trim().is_empty())
        .map(|a| a.name.as_str());
    let mut first = Vec::new();
    if let Some(stats) = o
        .combat
        .as_ref()
        .and_then(|combat| combat.own_stats.as_ref())
    {
        let a = &stats.attributes;
        first.push(format!(
            "Str:{} Spd:{} Int:{} Wil:{} Awr:{} Pre:{} HD:{}",
            a.strength,
            a.speed,
            a.intellect,
            a.willpower,
            a.awareness,
            a.presence,
            stats.hit_dice.total,
        ));
    }
    if let Some(place) = place_name(o).or(me) {
        first.push(place.to_owned());
    }
    let mut first = first.join("  ");
    if let Some(objective) = o.combat.as_ref().and_then(|c| c.objective) {
        if !first.is_empty() {
            first.push_str("   ");
        }
        first.push_str(tor_client_common::narration::objective(objective));
    }
    let mut second = Vec::new();
    if let Some(combat) = &o.combat {
        second.push(format!("HP:{}({})", combat.hp, combat.max_hp));
        if let Some(stats) = &combat.own_stats {
            for resource in &stats.resources {
                let label = match resource.resource.id.as_str() {
                    "stamina" => "Stm",
                    "focus" => "Foc",
                    "mana" => "Mana",
                    _ => resource.resource.name.as_str(),
                };
                let mut pool = format!("{label}:{}/{}", resource.balance, resource.maximum);
                if resource.reserved > 0 {
                    pool.push_str(&format!("[-{}]", resource.reserved));
                }
                second.push(pool);
            }
        }
    }
    // Currency is a disclosed typed capability, never inferred from appearance.
    // Sum in u128: the bounded inventory can contain several full u64 stacks.
    let gold: u128 = o
        .inventory
        .iter()
        .filter(|item| item.known_currency == Some(CurrencyKind::Gold))
        .map(|item| u128::from(item.quantity))
        .sum();
    second.push(format!("Gold:{gold}"));
    if let Some(NutritionView::Required { hunger, .. }) = o
        .combat
        .as_ref()
        .and_then(|combat| combat.own_stats.as_ref())
        .map(|stats| stats.nutrition)
    {
        second.push(tor_client_common::stats::hunger_label(hunger).into());
    }
    second.push(format!("T:{}", o.tick));
    if !o.linked_holds.is_empty() {
        second.push(format!("Held:{}", o.linked_holds.len()));
    }
    if let Some(combat) = &o.combat {
        if combat.dead {
            second.push("Dead".into());
        } else if combat.victory {
            second.push("Victorious".into());
            if !combat.terminal {
                second.extend(sequence_label(combat));
            }
        } else if let Some(sequence) = sequence_label(combat) {
            second.push(sequence);
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

/// Readable, bounded status rows. Whole fields wrap before optional details are
/// omitted; `+` marks omissions, never an abbreviated numeric value.
pub fn status_lines_at_width(app: &App, state: &ClientState, columns: usize) -> [String; 3] {
    let [first, second] = status_lines(app, state);
    let mut fields = Vec::new();
    for field in second.split("  ") {
        if field.starts_with("Sequence") {
            // Preserve the state even when the three exact counters are long.
            if let Some((label, detail)) = field.split_once(':') {
                fields.push(label.to_owned());
                let mut details = detail.split_whitespace();
                if let Some(progress) = details.next() {
                    fields.push(format!("Seq:{progress}"));
                }
                fields.extend(details.map(str::to_owned));
            }
        } else {
            fields.push(field.to_owned());
        }
    }
    fields.sort_by_key(|field| {
        if field.starts_with("HP:") {
            0
        } else if matches!(
            field.as_str(),
            "Dead" | "Victorious" | "hungry" | "weak" | "empty" | "Recovering"
        ) || field.starts_with("Held:")
            || field.starts_with("Sequence")
            || field.starts_with("Attack")
        {
            1
        } else if field.starts_with("Gold:") {
            2
        } else if field.starts_with("Stm:")
            || field.starts_with("Foc:")
            || field.starts_with("Mana:")
        {
            3
        } else if field.starts_with("T:") {
            5
        } else {
            4
        }
    });
    fn fit(fields: impl IntoIterator<Item = String>, columns: usize, rows: &mut [String]) {
        let mut packed: Vec<Vec<String>> = vec![Vec::new(); rows.len()];
        let mut row = 0;
        let mut omitted = false;
        for field in fields {
            if field.chars().count() > columns {
                omitted = true;
                continue;
            }
            while row < packed.len()
                && packed[row].join(" ").chars().count()
                    + usize::from(!packed[row].is_empty())
                    + field.chars().count()
                    > columns
            {
                row += 1;
            }
            if row == packed.len() {
                omitted = true;
                continue;
            }
            packed[row].push(field);
        }
        if omitted && columns > 0 {
            let last = packed.last_mut().expect("at least one status row");
            while last.join(" ").chars().count() + usize::from(!last.is_empty()) + 1 > columns {
                last.pop();
            }
            last.push("+".into());
        }
        for (row, fields) in rows.iter_mut().zip(packed) {
            *row = fields.join(" ");
        }
    }

    let mut rows = [String::new(), String::new(), String::new()];
    fit(
        first.split_whitespace().map(str::to_owned),
        columns,
        &mut rows[..1],
    );
    fit(fields, columns, &mut rows[1..]);
    rows
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
    "Tab     choose weapon for an ability that permits it",
    ". space wait                    i      inventory",
    "m       read-only detection charts",
    "g , d   pick up / drop          ^P     earlier messages",
    "w t q e equip/remove/drink/eat F2     game history",
    "o c     open / close a door     F5     remembered places",
    "F11     escape a hold           F4     write a note",
    "F6 read; F12 zap, then direction; X ignite; V extinguish; Esc cancels",
    "                                F7     wizard command editor",
    "                                F10    advancement / respec",
    "F3 R    take / release control  [ ]    slower / faster journeys",
    "F8 F9   resume / cancel action  Esc    cancel, close, or quit",
    "Move after a strike: cancel sequence and step (recovery still owed)",
    "Wizard discovery: F7, then creature list (optional depth N)",
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

impl Key {
    fn step_direction(self) -> Option<Direction> {
        Some(match self {
            Self::Up => Direction::North,
            Self::Down => Direction::South,
            Self::Left => Direction::West,
            Self::Right => Direction::East,
            Self::NorthEast => Direction::NorthEast,
            Self::SouthEast => Direction::SouthEast,
            Self::SouthWest => Direction::SouthWest,
            Self::NorthWest => Direction::NorthWest,
            Self::Ascend => Direction::Up,
            Self::Descend => Direction::Down,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod cosmetic_tests {
    use super::*;

    #[test]
    fn current_factual_stairs_do_not_resurrect_remembered_cosmetics() {
        let position = Position { x: 1, y: 0, z: 0 };
        let current = CellView {
            object_presentation: None,
            lit: true,
            door: None,
            trap: None,
            material: String::new(),
            key: "stairs".into(),
            stairs_up: true,
            stairs_down: false,
            position,
            wall: false,
            place_hint: false,
            asset: None,
        };
        let false_art = ObjectPresentation {
            name: "silver flask".into(),
            asset: None,
            kind: ObjectPresentationKind::Item {
                class: ItemClass::Potion,
            },
        };
        let old = tor_client_common::RememberedCell {
            appearance: RememberedMapCell {
                object_presentation: Some(false_art.clone()),
                key: "stairs".into(),
                position,
                wall: false,
                material: String::new(),
                asset: None,
                place_hint: false,
                stairs_up: true,
                stairs_down: false,
                door: None,
                trap: None,
                ground_items: vec![],
            },
            last_seen_tick: Some(1),
            last_seen_revision: Some(1),
        };
        let memory = || {
            std::iter::once(tor_client_common::MapCell {
                position,
                cell: &old,
            })
        };
        assert_eq!(
            stair_presentation(&[], memory(), position),
            Some(&false_art)
        );
        assert_eq!(stair_presentation(&[current], memory(), position), None);
    }
}
