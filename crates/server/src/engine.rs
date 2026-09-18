use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tor_protocol::*;
use tor_simulation::{ActorId as SimActor, Game};
use uuid::Uuid;

use crate::adapt;

const ARCHIVE_VERSION: u32 = 1;
const RULESET: &str = "two-room-v1";

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

/// A durable, single-branch journal. Notes never enter the game simulation.
/// New timeline branches are reserved for the later undo milestone.
#[derive(Debug)]
pub struct Engine {
    game: Game,
    archive: Archive,
    revisions: BTreeMap<ActorId, u64>,
    receipts: BTreeMap<(String, String), usize>,
    path: Option<PathBuf>,
    lock: Option<Arc<fs::File>>,
}

impl Engine {
    pub fn memory(scenario: Scenario) -> Result<Self, Failure> {
        let mut game = Game::two_room(scenario.seed);
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
        Ok(Self {
            game,
            revisions,
            receipts: BTreeMap::new(),
            path: None,
            lock: None,
            archive: Archive {
                version: ARCHIVE_VERSION,
                ruleset: RULESET.into(),
                scenario,
                branch: BranchId(Uuid::new_v4().to_string()),
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
            || archive.ruleset != RULESET
            || Uuid::parse_str(&archive.branch.0).is_err()
        {
            return Err(invalid_archive());
        }
        let mut engine = Self::memory(archive.scenario)?;
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
        Ok(engine)
    }

    pub fn branch(&self) -> &BranchId {
        &self.archive.branch
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
            self.game.next_actor() == Some(SimActor(actor.0)),
        ))
    }
    pub fn state(&self, actor: ActorId) -> Result<StateView, Failure> {
        Ok(StateView {
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
            .filter(|entry| entry.visible_to(actor, user))
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
                .map(|entry| (*entry).clone())
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
        self.apply_command(
            &Receipt {
                user: user.into(),
                frontend: frontend.into(),
                actor,
                request_id: request_id.into(),
                branch: branch.clone(),
                command,
            },
            None,
        )
    }

    fn apply_command(
        &mut self,
        receipt: &Receipt,
        recorded_id: Option<EntryId>,
    ) -> Result<CommandResult, Failure> {
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
        let mut candidate = self.candidate();
        let tick = candidate.game.tick();
        let (author, audience, content) = match &receipt.command {
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
                let before: BTreeMap<_, _> = self
                    .actors()
                    .into_iter()
                    .map(|actor| Ok((actor, self.observation(actor)?)))
                    .collect::<Result<_, Failure>>()?;
                let outcome = candidate
                    .game
                    .act(SimActor(receipt.actor.0), adapt::action(action))
                    .map_err(|_| Failure::new(ErrorCode::InvalidAction, "Action is unavailable"))?;
                for (actor, old) in before {
                    if candidate.observation(actor)? != old {
                        let revision = candidate.revisions.get_mut(&actor).expect("known actor");
                        *revision = revision.checked_add(1).ok_or_else(|| {
                            Failure::new(ErrorCode::InvalidAction, "Revision exhausted")
                        })?;
                    }
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
        let entry = HistoryEntry {
            id: recorded_id.unwrap_or_else(new_id),
            branch: self.branch().clone(),
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
        candidate.persist()?;
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
                if !entry.visible_to(actor, user)
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
            game: self.game.clone(),
            archive: self.archive.clone(),
            revisions: self.revisions.clone(),
            receipts: self.receipts.clone(),
            path: self.path.clone(),
            lock: self.lock.clone(),
        }
    }

    fn persist(&self) -> Result<(), Failure> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).map_err(|_| storage_failure())?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|_| storage_failure())?;
        serde_json::to_writer(temporary.as_file_mut(), &self.archive)
            .map_err(|_| storage_failure())?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| storage_failure())?;
        temporary.persist(path).map_err(|_| storage_failure())?;
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
