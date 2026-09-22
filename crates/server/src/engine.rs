use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::fs;
use std::io::{BufWriter, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tor_protocol::*;
use tor_simulation::{ActorId as SimActor, Game};
use uuid::Uuid;

use crate::adapt;
use crate::journal::{
    Command, HistoryContent, HistoryEntry, Position, WizardItem, WizardOperation, WizardResult,
};

const ARCHIVE_VERSION: u32 = 3;
const REWIND_BOUNDARIES: usize = 128;
const RULESET: &str = "diagonal-v11";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: ErrorCode,
    pub message: String,
}

impl Failure {
    pub(crate) fn new(code: ErrorCode, message: &str) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}
impl std::error::Error for Failure {}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorSetup {
    pub position: Position,
    pub turn_ticks: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub seed: u64,
    pub actors: Vec<ActorSetup>,
}

impl Scenario {
    pub fn two_room(seed: u64) -> Self {
        Self {
            seed,
            actors: vec![ActorSetup {
                position: Position {
                    region: 1,
                    x: 1,
                    y: 1,
                    z: 0,
                },
                turn_ticks: 100,
            }],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    user: String,
    frontend: String,
    request_id: String,
    actor: ActorId,
    branch: BranchId,
    command: Command,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    entry: HistoryEntry,
    receipt: Option<Receipt>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Archive {
    view_salt: String,
    wizard_game: bool,
    version: u32,
    ruleset: String,
    scenario: Scenario,
    branch: BranchId,
    records: Vec<Record>,
}

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub entry: HistoryEntry,
    pub duplicate: bool,
}

/// Diagnostic phase measurements for the checked-in performance harness.
/// Durations are deliberately not used as correctness thresholds; the counts
/// and byte totals provide stable regression contracts.
#[derive(Clone, Debug, Default)]
pub struct CommandProfile {
    pub rollback_capture: Duration,
    pub simulation_transition: Duration,
    pub perception: Duration,
    pub revision_detection: Duration,
    pub rollback_snapshot: Duration,
    pub journal_serialization: Duration,
    pub journal_write: Duration,
    pub journal_sync: Duration,
    pub actors_observed: usize,
    pub revision_comparisons: usize,
    pub rollback_snapshots: usize,
    pub records_serialized: usize,
    pub bytes_written: u64,
}

#[derive(Clone, Debug)]
struct Boundary {
    id: Option<EntryId>,
    game: Game,
    revisions: BTreeMap<ActorId, u64>,
}

/// Durable chronological journal, including retained futures and explicit forks.
#[derive(Debug)]
pub struct Engine {
    current_branch: BranchId,
    wizard_enabled: bool,
    boundaries: VecDeque<Arc<Boundary>>,
    game: Game,
    archive: Archive,
    revisions: BTreeMap<ActorId, u64>,
    receipts: BTreeMap<(String, String), usize>,
    path: Option<PathBuf>,
    lock: Option<Arc<fs::File>>,
}

impl Engine {
    pub fn memory(scenario: Scenario) -> Result<Self, Failure> {
        let mut game = Game::two_room_in_stone(scenario.seed);
        let mut revisions = BTreeMap::new();
        if scenario.actors.is_empty() {
            return Err(invalid_archive());
        }
        for actor in &scenario.actors {
            let ticks = NonZeroU64::new(actor.turn_ticks).ok_or_else(invalid_archive)?;
            let id = game
                .spawn_actor(adapt::location(actor.position), ticks)
                .map_err(|_| invalid_archive())?;
            revisions.insert(ActorId(id.0), 0);
        }
        game.refresh_navigation();
        let branch = BranchId(Uuid::new_v4().to_string());
        let initial = Arc::new(Boundary {
            id: None,
            game: game.clone(),
            revisions: revisions.clone(),
        });
        Ok(Self {
            current_branch: branch.clone(),
            wizard_enabled: false,
            boundaries: VecDeque::from([initial]),
            game,
            revisions,
            receipts: BTreeMap::new(),
            path: None,
            lock: None,
            archive: Archive {
                view_salt: Uuid::new_v4().to_string(),
                wizard_game: false,
                version: ARCHIVE_VERSION,
                ruleset: RULESET.into(),
                scenario,
                branch,
                records: Vec::new(),
            },
        })
    }

    /// Existing saves own their scenario; `scenario` is used only for a new file.
    pub fn open(path: impl AsRef<Path>, scenario: Scenario) -> Result<Self, Failure> {
        let (path, lock) = lock_save(path.as_ref())?;
        let mut engine = match fs::read(&path) {
            Ok(bytes) => {
                let archive: Archive =
                    serde_json::from_slice(&bytes).map_err(|_| invalid_archive())?;
                Self::replay(archive)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::memory(scenario)?,
            Err(_) => return Err(storage_failure()),
        };
        engine.path = Some(path);
        engine.lock = Some(lock);
        engine.persist()?;
        Ok(engine)
    }

    fn replay(archive: Archive) -> Result<Self, Failure> {
        if archive.version != ARCHIVE_VERSION
            || Uuid::parse_str(&archive.view_salt).is_err()
            || archive.ruleset != RULESET
            || Uuid::parse_str(&archive.branch.0).is_err()
        {
            return Err(invalid_archive());
        }
        let mut engine = Self::memory(archive.scenario)?;
        engine.archive.view_salt = archive.view_salt;
        engine.current_branch = archive.branch.clone();
        engine.archive.wizard_game = archive.wizard_game;
        engine.wizard_enabled = archive.wizard_game;
        engine.archive.branch = archive.branch;
        for record in archive.records {
            if Uuid::parse_str(&record.entry.id.0).is_err()
                || engine
                    .archive
                    .records
                    .iter()
                    .any(|old| old.entry.id == record.entry.id)
            {
                return Err(invalid_archive());
            }
            let result = if let Some(receipt) = record.receipt {
                engine
                    .apply_command(&receipt, Some(record.entry.id.clone()))
                    .map(|result| result.entry)
            } else if let (
                Author::Backend { component },
                HistoryContent::Annotation {
                    anchor,
                    category,
                    text,
                },
            ) = (&record.entry.author, &record.entry.content)
            {
                engine.backend_note(
                    record.entry.actor,
                    component,
                    anchor.clone(),
                    *category,
                    text,
                    Some(record.entry.id.clone()),
                )
            } else {
                return Err(invalid_archive());
            };
            if result.map_err(|_| invalid_archive())? != record.entry {
                return Err(invalid_archive());
            }
        }
        engine.wizard_enabled = false;
        Ok(engine)
    }

    pub fn branch(&self) -> &BranchId {
        &self.current_branch
    }
    /// Trusted administration operation. The marker commits before authority changes.
    pub fn enable_wizard(&mut self) -> Result<(), Failure> {
        let mut candidate = self.candidate();
        candidate.archive.wizard_game = true;
        candidate.persist()?;
        candidate.wizard_enabled = true;
        *self = candidate;
        Ok(())
    }
    pub fn wizard_enabled(&self) -> bool {
        self.wizard_enabled
    }
    pub fn actors(&self) -> Vec<ActorId> {
        self.revisions.keys().copied().collect()
    }
    pub fn revision(&self, actor: ActorId) -> Result<u64, Failure> {
        self.revisions
            .get(&actor)
            .copied()
            .ok_or_else(|| Failure::new(ErrorCode::Unauthorized, "Actor is unavailable"))
    }
    pub fn observation(&self, actor: ActorId) -> Result<Observation, Failure> {
        let view = self
            .game
            .observe(SimActor(actor.0))
            .map_err(|_| Failure::new(ErrorCode::Unauthorized, "Actor is unavailable"))?;
        Ok(adapt::observation(
            view,
            self.game
                .scene(SimActor(actor.0))
                .map_err(|_| invalid_archive())?,
            &self.archive.view_salt,
            self.game.next_actor() == Some(SimActor(actor.0)),
        ))
    }
    fn revision_view(
        &self,
        actor: ActorId,
    ) -> Result<(tor_simulation::Observation, Vec<tor_world::SightCell>), Failure> {
        let view = self
            .game
            .observe(SimActor(actor.0))
            .map_err(|_| invalid_archive())?;
        let scene = self
            .game
            .scene(SimActor(actor.0))
            .map_err(|_| invalid_archive())?;
        Ok((view, scene))
    }

    pub fn travel_route(
        &self,
        actor: ActorId,
        destination: &str,
    ) -> Result<Vec<tor_simulation::TravelStep>, Failure> {
        let unavailable = || {
            Failure::new(
                ErrorCode::InvalidAction,
                "Travel destination or known route is unavailable",
            )
        };
        let location = self
            .game
            .known_cells(SimActor(actor.0))
            .find(|&cell| adapt::cell_key(&self.archive.view_salt, actor.0, cell) == destination)
            .ok_or_else(unavailable)?;
        self.game
            .travel_route(SimActor(actor.0), location)
            .map_err(|_| unavailable())
    }

    pub fn state(&self, actor: ActorId) -> Result<StateView, Failure> {
        Ok(StateView {
            wizard_game: self.archive.wizard_game,
            revision: self.revision(actor)?,
            observation: self.observation(actor)?,
        })
    }

    pub fn history(
        &self,
        actor: ActorId,
        user: &str,
        before: Option<&EntryId>,
        limit: usize,
    ) -> Result<HistoryPage, Failure> {
        self.history_branch(actor, user, self.branch(), before, limit)
    }

    pub fn history_branch(
        &self,
        actor: ActorId,
        user: &str,
        branch: &BranchId,
        before: Option<&EntryId>,
        limit: usize,
    ) -> Result<HistoryPage, Failure> {
        self.revision(actor)?;
        if limit == 0 || limit > MAX_HISTORY_PAGE {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid history page size",
            ));
        }
        let visible: Vec<_> = self
            .archive
            .records
            .iter()
            .map(|record| &record.entry)
            .filter(|entry| &entry.branch == branch && entry.visible_to(actor, user))
            .collect();
        let end = match before {
            Some(id) => visible
                .iter()
                .position(|entry| &entry.id == id)
                .ok_or_else(invalid_anchor)?,
            None => visible.len(),
        };
        let start = end.saturating_sub(limit);
        Ok(HistoryPage {
            entries: visible[start..end]
                .iter()
                .map(|entry| entry.disclosed())
                .collect(),
            older_before: (start > 0).then(|| visible[start].id.clone()),
        })
    }

    /// Recover a durable receipt before checking ephemeral control ownership.
    pub fn retry(
        &self,
        user: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: &Command,
    ) -> Result<Option<CommandResult>, Failure> {
        let Some(&index) = self.receipts.get(&(user.into(), request_id.into())) else {
            return Ok(None);
        };
        let record = &self.archive.records[index];
        let receipt = record
            .receipt
            .as_ref()
            .expect("receipt index only contains client records");
        if receipt.actor != actor || &receipt.branch != branch || &receipt.command != command {
            return Err(Failure::new(
                ErrorCode::RequestConflict,
                "Request ID was already used for another command",
            ));
        }
        Ok(Some(CommandResult {
            entry: record.entry.clone(),
            duplicate: true,
        }))
    }

    pub fn command(
        &mut self,
        user: &str,
        frontend: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: Command,
    ) -> Result<CommandResult, Failure> {
        self.apply_command_profiled(
            &Receipt {
                user: user.into(),
                frontend: frontend.into(),
                actor,
                request_id: request_id.into(),
                branch: branch.clone(),
                command,
            },
            None,
            None,
        )
    }

    /// Execute the production command path while collecting phase diagnostics.
    pub fn command_profiled(
        &mut self,
        user: &str,
        frontend: &str,
        actor: ActorId,
        request_id: &str,
        branch: &BranchId,
        command: Command,
    ) -> Result<(CommandResult, CommandProfile), Failure> {
        let mut profile = CommandProfile::default();
        let result = self.apply_command_profiled(
            &Receipt {
                user: user.into(),
                frontend: frontend.into(),
                actor,
                request_id: request_id.into(),
                branch: branch.clone(),
                command,
            },
            None,
            Some(&mut profile),
        )?;
        Ok((result, profile))
    }

    /// Persist the current archive to a diagnostic target and report the same
    /// serialization/write/sync phases used by normal saves. The target should
    /// be disposable; this does not attach it to the engine or publish state.
    pub fn profile_persistence(&self, path: impl AsRef<Path>) -> Result<CommandProfile, Failure> {
        let mut candidate = self.candidate();
        candidate.path = Some(path.as_ref().to_path_buf());
        candidate.lock = None;
        let mut profile = CommandProfile::default();
        candidate.persist_profiled(Some(&mut profile))?;
        Ok(profile)
    }

    /// Populate an in-memory diagnostic fixture in linear time. Records are
    /// ordinary deterministic wait actions and remain replayable; persistence
    /// is intentionally bypassed so a 10,000-action baseline does not spend its
    /// setup time exercising the quadratic behavior being measured.
    pub fn seed_profile_history(&mut self, count: usize) -> Result<(), Failure> {
        for index in 0..count {
            let actors = self.actors();
            let actor = actors[index % actors.len()];
            let expected_revision = self.revision(actor)?;
            let before: BTreeMap<_, _> = actors
                .into_iter()
                .map(|id| Ok((id, self.revision_view(id)?)))
                .collect::<Result<_, Failure>>()?;
            let tick = self.game.tick();
            let outcome = self
                .game
                .act(SimActor(actor.0), tor_simulation::Action::Wait)
                .map_err(|_| invalid_archive())?;
            for (id, old) in before {
                if self.revision_view(id)? != old {
                    *self.revisions.get_mut(&id).expect("known actor") += 1;
                }
            }
            self.game.refresh_navigation();
            let entry = HistoryEntry {
                id: new_id(),
                branch: self.branch().clone(),
                actor,
                tick,
                author: Author::User {
                    user: "bench".into(),
                },
                audience: Audience::Actor,
                content: HistoryContent::Action {
                    action: tor_protocol::Action::Wait,
                    event: adapt::event(outcome.kind),
                },
            };
            let receipt = Receipt {
                user: "bench".into(),
                frontend: "headless".into(),
                request_id: format!("seed-{index}"),
                actor,
                branch: self.branch().clone(),
                command: Command::Act {
                    expected_revision,
                    action: tor_protocol::Action::Wait,
                },
            };
            self.receipts.insert(
                (receipt.user.clone(), receipt.request_id.clone()),
                self.archive.records.len(),
            );
            self.archive.records.push(Record {
                entry: entry.clone(),
                receipt: Some(receipt),
            });
            self.boundaries.push_back(Arc::new(Boundary {
                id: Some(entry.id),
                game: self.game.clone(),
                revisions: self.revisions.clone(),
            }));
            if self.boundaries.len() > REWIND_BOUNDARIES {
                self.boundaries.pop_front();
            }
        }
        Ok(())
    }

    fn apply_command(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
    ) -> Result<CommandResult, Failure> {
        self.apply_command_profiled(receipt, recorded_id, None)
    }

    fn apply_command_profiled(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
        mut profile: Option<&mut CommandProfile>,
    ) -> Result<CommandResult, Failure> {
        if matches!(receipt.command, Command::Wizard { .. }) && !self.wizard_enabled {
            return Err(Failure::new(
                ErrorCode::Unauthorized,
                "Wizard operations are disabled",
            ));
        }
        if !valid_label(&receipt.user)
            || !valid_label(&receipt.frontend)
            || !valid_label(&receipt.request_id)
        {
            return Err(Failure::new(
                ErrorCode::InvalidRequest,
                "Invalid identity or request ID",
            ));
        }
        if let Some(result) = self.retry(
            &receipt.user,
            receipt.actor,
            &receipt.request_id,
            &receipt.branch,
            &receipt.command,
        )? {
            return Ok(result);
        }
        if &receipt.branch != self.branch() {
            return Err(Failure::new(
                ErrorCode::WrongBranch,
                "Reconnect to the current branch",
            ));
        }
        let revision = self.revision(receipt.actor)?;
        let started = Instant::now();
        let mut candidate = self.candidate();
        if let Some(profile) = profile.as_deref_mut() {
            profile.rollback_capture += started.elapsed();
        }
        let mut tick = candidate.game.tick();
        let entry_id = recorded_id.unwrap_or_else(new_id);
        let (author, audience, content) = match &receipt.command {
            Command::Travel {
                expected_revision,
                destination,
            } => {
                if *expected_revision != revision {
                    return Err(Failure::new(
                        ErrorCode::StaleRevision,
                        "Refresh before travelling",
                    ));
                }
                self.travel_route(receipt.actor, destination)?;
                if self.game.next_actor() != Some(SimActor(receipt.actor.0)) {
                    return Err(Failure::new(ErrorCode::InvalidAction, "Actor is not ready"));
                }
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Actor,
                    HistoryContent::Travel {
                        destination: destination.clone(),
                    },
                )
            }

            Command::Wizard {
                expected_revision,
                operation,
            } => {
                if *expected_revision != revision {
                    return Err(Failure::new(
                        ErrorCode::StaleRevision,
                        "Refresh before a wizard operation",
                    ));
                }
                let result = candidate.apply_wizard(receipt, operation, &entry_id)?;
                tick = candidate.game.tick();
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Private,
                    HistoryContent::Wizard {
                        operation: operation.clone(),
                        result,
                    },
                )
            }
            Command::Act {
                expected_revision,
                action,
            } => {
                if *expected_revision != revision {
                    return Err(Failure::new(
                        ErrorCode::StaleRevision,
                        "Refresh the observation before acting",
                    ));
                }
                let started = Instant::now();
                let before: BTreeMap<_, _> = self
                    .actors()
                    .into_iter()
                    .map(|actor| Ok((actor, self.revision_view(actor)?)))
                    .collect::<Result<_, Failure>>()?;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.perception += started.elapsed();
                    profile.actors_observed += before.len();
                }
                let started = Instant::now();
                let outcome = candidate
                    .game
                    .act(SimActor(receipt.actor.0), adapt::action(action))
                    .map_err(|_| Failure::new(ErrorCode::InvalidAction, "Action is unavailable"))?;
                if let Some(profile) = profile.as_deref_mut() {
                    profile.simulation_transition += started.elapsed();
                }
                let started = Instant::now();
                for (actor, old) in before {
                    let perception_started = Instant::now();
                    let changed = candidate.revision_view(actor)? != old;
                    if let Some(profile) = profile.as_deref_mut() {
                        profile.perception += perception_started.elapsed();
                        profile.actors_observed += 1;
                        profile.revision_comparisons += 1;
                    }
                    if changed {
                        let revision = candidate.revisions.get_mut(&actor).expect("known actor");
                        *revision = revision.checked_add(1).ok_or_else(|| {
                            Failure::new(ErrorCode::InvalidAction, "Revision exhausted")
                        })?;
                    }
                }
                if let Some(profile) = profile.as_deref_mut() {
                    profile.revision_detection += started.elapsed();
                }
                (
                    Author::User {
                        user: receipt.user.clone(),
                    },
                    Audience::Actor,
                    HistoryContent::Action {
                        action: action.clone(),
                        event: adapt::event(outcome.kind),
                    },
                )
            }
            Command::Annotate {
                anchor,
                text,
                source,
                audience,
                category,
            } => {
                self.validate_note(receipt.actor, &receipt.user, *audience, anchor, text)?;
                let author = match source {
                    ClientSource::User => Author::User {
                        user: receipt.user.clone(),
                    },
                    ClientSource::Frontend => Author::Frontend {
                        user: receipt.user.clone(),
                        component: receipt.frontend.clone(),
                    },
                };
                (
                    author,
                    *audience,
                    HistoryContent::Annotation {
                        anchor: anchor.clone(),
                        category: *category,
                        text: text.clone(),
                    },
                )
            }
        };
        candidate.game.refresh_navigation();
        let entry = HistoryEntry {
            id: entry_id,
            branch: candidate.branch().clone(),
            actor: receipt.actor,
            tick,
            author,
            audience,
            content,
        };
        candidate.receipts.insert(
            (receipt.user.clone(), receipt.request_id.clone()),
            candidate.archive.records.len(),
        );
        candidate.archive.records.push(Record {
            entry: entry.clone(),
            receipt: Some(receipt.clone()),
        });
        if !matches!(entry.content, HistoryContent::Annotation { .. }) {
            let started = Instant::now();
            candidate.boundaries.push_back(Arc::new(Boundary {
                id: Some(entry.id.clone()),
                game: candidate.game.clone(),
                revisions: candidate.revisions.clone(),
            }));
            if candidate.boundaries.len() > REWIND_BOUNDARIES {
                candidate.boundaries.pop_front();
            }
            if let Some(profile) = profile.as_deref_mut() {
                profile.rollback_snapshot += started.elapsed();
                profile.rollback_snapshots += 1;
            }
        }
        candidate.persist_profiled(profile.as_deref_mut())?;
        *self = candidate;
        Ok(CommandResult {
            entry,
            duplicate: false,
        })
    }

    pub fn annotate_backend(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: Anchor,
        category: AnnotationCategory,
        text: &str,
    ) -> Result<HistoryEntry, Failure> {
        self.backend_note(actor, component, anchor, category, text, None)
    }

    fn apply_wizard(
        &mut self,
        receipt: &Receipt,
        operation: &WizardOperation,
        entry_id: &EntryId,
    ) -> Result<WizardResult, Failure> {
        let invalid = || {
            Failure::new(
                ErrorCode::InvalidAction,
                "Wizard target or settings are unavailable",
            )
        };
        let before: BTreeMap<_, _> = self
            .actors()
            .into_iter()
            .map(|actor| Ok((actor, self.revision_view(actor)?)))
            .collect::<Result<_, Failure>>()?;
        let result = match operation {
            WizardOperation::PlaceDoor { position, open } => {
                let door = self
                    .game
                    .place_door(adapt::location(*position), *open)
                    .map_err(|_| {
                        Failure::new(ErrorCode::InvalidAction, "Invalid door placement")
                    })?;
                WizardResult::DoorPlaced { door }
            }
            WizardOperation::ConnectArea {
                from,
                direction,
                to,
                quarter_turns,
                width,
                height,
            } => {
                self.game
                    .connect_area(
                        tor_world::Passage {
                            from: adapt::location(*from),
                            direction: adapt::direction(*direction),
                            to: adapt::location(*to),
                        },
                        *quarter_turns,
                        *width,
                        *height,
                    )
                    .map_err(|_| invalid())?;
                WizardResult::Connected
            }
            WizardOperation::PlaceRoom { region } | WizardOperation::PlaceChamber { region } => {
                let chamber = matches!(operation, WizardOperation::PlaceChamber { .. });
                // Bound setup and disclosure work independently of wire limits.
                if region.id == 0
                    || region.name.is_empty()
                    || region.name.len() > 80
                    || region.name.chars().any(char::is_control)
                    || !(1..=32).contains(&region.width)
                    || !(1..=32).contains(&region.depth)
                    || !(1..=8).contains(&region.height)
                {
                    return Err(invalid());
                }
                let room = tor_world::Region {
                    id: tor_world::RegionId(region.id),
                    name: region.name.clone(),
                    bounds: tor_world::Extent::new(region.width, region.depth, region.height)
                        .ok_or_else(invalid)?,
                };
                if chamber {
                    self.game.add_chamber(room)
                } else {
                    self.game.add_region(room)
                }
                .map_err(|_| invalid())?;
                WizardResult::RoomPlaced { region: region.id }
            }
            WizardOperation::Connect {
                from,
                direction,
                to,
                quarter_turns,
            } => {
                self.game
                    .connect(
                        tor_world::Passage {
                            from: adapt::location(*from),
                            direction: adapt::direction(*direction),
                            to: adapt::location(*to),
                        },
                        *quarter_turns,
                    )
                    .map_err(|_| invalid())?;
                WizardResult::Connected
            }
            WizardOperation::SetPlaceHint { position, present } => {
                self.game
                    .set_place_hint(adapt::location(*position), *present)
                    .map_err(|_| invalid())?;
                WizardResult::PlaceHintSet
            }
            WizardOperation::SetWall { position, wall } => {
                self.game
                    .set_wall(adapt::location(*position), *wall)
                    .map_err(|_| invalid())?;
                WizardResult::WallSet
            }
            WizardOperation::PlaceItem { kind, position } => {
                let name = match kind {
                    WizardItem::Token => "copper token",
                    WizardItem::Tablet => "stone tablet",
                };
                let item = self
                    .game
                    .place_item(adapt::location(*position), name.into())
                    .map_err(|_| invalid())?;
                WizardResult::ItemPlaced { item: item.0 }
            }
            WizardOperation::SpawnActor {
                position,
                turn_ticks,
            } => {
                let duration = NonZeroU64::new(*turn_ticks).ok_or_else(invalid)?;
                let actor = self
                    .game
                    .spawn_actor(adapt::location(*position), duration)
                    .map_err(|_| invalid())?;
                self.revisions.insert(ActorId(actor.0), 0);
                WizardResult::ActorSpawned {
                    actor: ActorId(actor.0),
                }
            }
            WizardOperation::Teleport { actor, position } => {
                self.game
                    .teleport(SimActor(actor.0), adapt::location(*position))
                    .map_err(|_| invalid())?;
                WizardResult::Teleported { actor: *actor }
            }
            WizardOperation::Rewind { target } => {
                if let Some(id) = target {
                    if !self.archive.records.iter().any(|r| {
                        &r.entry.id == id && r.entry.visible_to(receipt.actor, &receipt.user)
                    }) {
                        return Err(invalid());
                    }
                }
                let boundary = self
                    .boundaries
                    .iter()
                    .find(|b| &b.id == target)
                    .cloned()
                    .ok_or_else(invalid)?;
                if !boundary.revisions.contains_key(&receipt.actor) {
                    return Err(invalid());
                }
                self.game = boundary.game.clone();
                self.revisions = boundary.revisions.clone();
                let from_branch = self.current_branch.clone();
                self.current_branch = BranchId(entry_id.0.clone());
                return Ok(WizardResult::Rewound {
                    from_branch,
                    branch: self.current_branch.clone(),
                    tick: self.game.tick(),
                    next_actor: ActorId(self.game.next_actor().ok_or_else(invalid)?.0),
                });
            }
        };
        for (actor, old) in before {
            if actor == receipt.actor || self.revision_view(actor)? != old {
                let revision = self.revisions.get_mut(&actor).expect("existing actor");
                *revision = revision.checked_add(1).ok_or_else(invalid)?;
            }
        }
        Ok(result)
    }

    fn backend_note(
        &mut self,
        actor: ActorId,
        component: &str,
        anchor: Anchor,
        category: AnnotationCategory,
        text: &str,
        recorded_id: Option<EntryId>,
    ) -> Result<HistoryEntry, Failure> {
        if !valid_label(component) {
            return Err(Failure::new(
                ErrorCode::InvalidAnnotation,
                "Invalid backend component",
            ));
        }
        self.validate_note(actor, "", Audience::Actor, &anchor, text)?;
        let entry = HistoryEntry {
            id: recorded_id.unwrap_or_else(new_id),
            branch: self.branch().clone(),
            actor,
            tick: self.game.tick(),
            author: Author::Backend {
                component: component.into(),
            },
            audience: Audience::Actor,
            content: HistoryContent::Annotation {
                anchor,
                category,
                text: text.into(),
            },
        };
        let mut candidate = self.candidate();
        candidate.archive.records.push(Record {
            entry: entry.clone(),
            receipt: None,
        });
        candidate.persist()?;
        *self = candidate;
        Ok(entry)
    }

    fn validate_note(
        &self,
        actor: ActorId,
        user: &str,
        audience: Audience,
        anchor: &Anchor,
        text: &str,
    ) -> Result<(), Failure> {
        let revision = self.revision(actor)?;
        if text.trim().is_empty()
            || text.len() > MAX_NOTE_BYTES
            || text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t')
        {
            return Err(Failure::new(
                ErrorCode::InvalidAnnotation,
                "Notes require 1–4096 bytes of plain text",
            ));
        }
        match anchor {
            Anchor::State { revision: target } if *target <= revision => Ok(()),
            Anchor::Entry { id } => {
                let entry = self
                    .archive
                    .records
                    .iter()
                    .map(|r| &r.entry)
                    .find(|entry| &entry.id == id)
                    .ok_or_else(invalid_anchor)?;
                if &entry.branch != self.branch()
                    || !entry.visible_to(actor, user)
                    || (audience == Audience::Actor && entry.audience == Audience::Private)
                {
                    return Err(invalid_anchor());
                }
                Ok(())
            }
            _ => Err(invalid_anchor()),
        }
    }

    // Only internal transactional candidates may share the save lock. Exposing
    // Clone would allow two independent engines to overwrite each other's journal.
    fn candidate(&self) -> Self {
        Self {
            current_branch: self.current_branch.clone(),
            wizard_enabled: self.wizard_enabled,
            boundaries: self.boundaries.clone(),
            game: self.game.clone(),
            archive: self.archive.clone(),
            revisions: self.revisions.clone(),
            receipts: self.receipts.clone(),
            path: self.path.clone(),
            lock: self.lock.clone(),
        }
    }

    fn persist(&self) -> Result<(), Failure> {
        self.persist_profiled(None)
    }

    fn persist_profiled(&self, mut profile: Option<&mut CommandProfile>) -> Result<(), Failure> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|_| storage_failure())?;
        let started = Instant::now();
        let bytes = serde_json::to_vec(&self.archive).map_err(|_| storage_failure())?;
        if let Some(profile) = profile.as_deref_mut() {
            profile.journal_serialization += started.elapsed();
            profile.records_serialized += self.archive.records.len();
            profile.bytes_written += bytes.len() as u64;
        }
        let started = Instant::now();
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|_| storage_failure())?;
        {
            let mut writer = BufWriter::new(temporary.as_file_mut());
            writer.write_all(&bytes).map_err(|_| storage_failure())?;
            writer.flush().map_err(|_| storage_failure())?;
        }
        if let Some(profile) = profile.as_deref_mut() {
            profile.journal_write += started.elapsed();
        }
        let started = Instant::now();
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| storage_failure())?;
        temporary.persist(path).map_err(|_| storage_failure())?;
        if let Some(profile) = profile.as_deref_mut() {
            profile.journal_sync += started.elapsed();
        }
        Ok(())
    }
}

pub(crate) fn valid_label(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 128 && !value.chars().any(char::is_control)
}
fn new_id() -> EntryId {
    EntryId(Uuid::new_v4().to_string())
}
fn invalid_anchor() -> Failure {
    Failure::new(ErrorCode::InvalidAnchor, "Annotation target is unavailable")
}
fn storage_failure() -> Failure {
    Failure::new(
        ErrorCode::StorageFailure,
        "Could not commit the game journal",
    )
}
fn invalid_archive() -> Failure {
    Failure::new(
        ErrorCode::InvalidArchive,
        "Unsupported or inconsistent game journal",
    )
}

fn lock_save(path: &Path) -> Result<(PathBuf, Arc<fs::File>), Failure> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|_| storage_failure())?;
    let canonical = match fs::canonicalize(path) {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::canonicalize(parent)
            .map_err(|_| storage_failure())?
            .join(path.file_name().ok_or_else(storage_failure)?),
        Err(_) => return Err(storage_failure()),
    };
    let mut lock_name = canonical
        .file_name()
        .ok_or_else(storage_failure)?
        .to_os_string();
    lock_name.push(".lock");
    let file = fs::File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(canonical.with_file_name(lock_name))
        .map_err(|_| storage_failure())?;
    file.try_lock().map_err(|_| {
        Failure::new(
            ErrorCode::StorageFailure,
            "Game journal is already in use or cannot be locked",
        )
    })?;
    Ok((canonical, Arc::new(file)))
}
