//! Deterministic rules and actor-specific observations, without I/O or a clock.
//!
//! [`Game`] and [`ActionOutcome`] are backend-only types. Network adapters must
//! disclose observations and filter events; they must not serialize raw game state.

pub mod abilities;
mod actions;
mod actor_creatures;
mod actor_record;
mod actor_store;
pub mod arena;
pub mod content_ids;
mod creature_melee;
mod creature_time;
mod lifecycle;
pub mod linked_hold;
pub use linked_hold::HoldId;
pub mod escape;
pub mod limits;
mod recovery_context;
pub mod rules;
pub mod run_policy;
mod sequence;
mod sequence_execution;
#[cfg(test)]
mod test_support;
pub mod unique_retrieval;
pub use actor_creatures::{CreatureIdentity, CreatureInspection};
pub mod ai;
mod ai_items;
pub mod attacks;
pub mod attributes;
pub mod behavior;
mod bounded;
pub mod combat;
pub mod conditions;
pub mod cooldowns;
pub mod costs;
pub mod creatures;
pub mod damage;
pub mod dice;
pub mod floor_activation;
pub use floor_activation::CandidateStep;
pub mod hd;
pub use hd::{HdCount, HdIndex};
pub mod health;
pub mod health_recovery;
mod intention;
pub mod nutrition;
pub mod player_advancement;
pub mod player_respec;
pub mod prepared_spawn;
pub mod supplies;
pub use intention::{
    IntentionControl, IntentionControlState, IntentionExecution, IntentionId, IntentionInput,
    IntentionOrigin, IntentionState, IntentionWork, MovementContext, QueuedIntention,
    TerminalExitContext,
};
pub mod locomotion;
mod physics;
pub use physics::{BodySpec, Impact, MotionState, PhysicsEntity};
mod item_store;
mod items;
pub use items::{CurrencyKind, ItemClass, ItemSpec};
mod interactions;
pub use interactions::{
    AnatomySpec, ConsumableSpec, EdibleSpec, EquipmentSlot, EquipmentSlotId, EquipmentSlotView,
    EquipmentSpec, InteractionView, ItemInteractionView, PreparationView, Work,
};
pub mod checkpoint;
pub use checkpoint::PersistError;
pub mod diagnostics;
pub mod earned_growth;
mod explored_map;
mod fixture;
pub use explored_map::{
    map_work_counts, reset_map_work_counts, MapAppearance, MapCellInput, MapDoorAppearance,
    MapItemAppearance, MapKnowledgeRoot, MapProjection, MapWindow, MapWorkCounts, ProjectedMapCell,
};
mod navigation_map;
mod observation;
mod places;
pub mod progression;
pub mod public_plan;
pub mod quantity;
pub mod resolution_diagnostics;
pub mod resources;
pub mod survival_diagnostics;
pub mod theft;
pub use places::{NameOrigin, PlaceName};
mod streaming;
pub use streaming::{
    MemoryRecords, PinWork, RecordId, RecordStore, ReferencePoint, ReferencePointId,
    ReferenceTarget, RegionIdentities, RegionRecord, RegionRecordData, RegionRoot, RegionState,
    RegionTransition, TransitionError, TransitionReport, UnbuiltRegion,
};
mod travel;
pub use travel::TravelStep;

pub use observation::{
    ActorView, CellView, CorpseAppearance, ExitView, GroundItemView, ItemView, KnownPlace,
    Observation, SupplyView,
};

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use tor_world::{Direction, Location, Passage, Position, Region, RegionId, Shared, World};

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ActorId(pub u64);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct ItemId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Action {
    Escape {
        hold: linked_hold::HoldId,
    },
    ChooseGrowthSource {
        basis: player_advancement::AdvancementBasis,
        source: earned_growth::GrowthSource,
    },
    Respec {
        basis: player_advancement::AdvancementBasis,
        replacement: player_respec::ReplacementOperation,
        supplies: supplies::SupplySelection,
    },
    Purchase {
        basis: player_advancement::AdvancementBasis,
        purchase: player_advancement::PurchaseOperation,
    },
    Equip {
        item: ItemId,
        slot: EquipmentSlotId,
    },
    Unequip {
        item: ItemId,
    },
    Drink {
        item: ItemId,
    },
    Eat {
        item: ItemId,
    },
    UseAbility {
        id: content_ids::AbilityId,
        invocation: crate::abilities::AbilityInvocation,
        supplies: supplies::SupplySelection,
    },
    SetDoor {
        door: u64,
        open: bool,
    },
    Move(Direction),
    Take {
        item: ItemId,
        quantity: Option<u64>,
    },
    Drop {
        item: ItemId,
        quantity: Option<u64>,
    },
    Wait,
}
impl<'de> serde::Deserialize<'de> for Action {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Adjacent-tag derives also accept a positional outer tuple. Admission
        // operands require their exact named object at every public boundary.
        #[derive(serde::Deserialize)]
        #[serde(
            tag = "type",
            content = "value",
            rename_all = "snake_case",
            deny_unknown_fields
        )]
        enum Fields {
            #[serde(deserialize_with = "crate::escape::read_action")]
            Escape {
                hold: linked_hold::HoldId,
            },
            ChooseGrowthSource {
                basis: player_advancement::AdvancementBasis,
                source: earned_growth::GrowthSource,
            },
            Respec {
                basis: player_advancement::AdvancementBasis,
                replacement: player_respec::ReplacementOperation,
                supplies: supplies::SupplySelection,
            },
            Purchase {
                basis: player_advancement::AdvancementBasis,
                purchase: player_advancement::PurchaseOperation,
            },
            Equip {
                item: ItemId,
                slot: EquipmentSlotId,
            },
            Unequip {
                item: ItemId,
            },
            Drink {
                item: ItemId,
            },
            Eat {
                item: ItemId,
            },
            UseAbility {
                id: content_ids::AbilityId,
                invocation: crate::abilities::AbilityInvocation,
                supplies: supplies::SupplySelection,
            },
            SetDoor {
                door: u64,
                open: bool,
            },
            Move(Direction),
            Take {
                item: ItemId,
                quantity: Option<u64>,
            },
            Drop {
                item: ItemId,
                quantity: Option<u64>,
            },
            Wait,
        }
        Ok(match bounded::named_map(deserializer)? {
            Fields::Escape { hold } => Self::Escape { hold },
            Fields::ChooseGrowthSource { basis, source } => {
                Self::ChooseGrowthSource { basis, source }
            }
            Fields::Respec {
                basis,
                replacement,
                supplies,
            } => Self::Respec {
                basis,
                replacement,
                supplies,
            },
            Fields::Purchase { basis, purchase } => Self::Purchase { basis, purchase },
            Fields::Equip { item, slot } => Self::Equip { item, slot },
            Fields::Unequip { item } => Self::Unequip { item },
            Fields::Drink { item } => Self::Drink { item },
            Fields::Eat { item } => Self::Eat { item },
            Fields::UseAbility {
                id,
                invocation,
                supplies,
            } => Self::UseAbility {
                id,
                invocation,
                supplies,
            },
            Fields::SetDoor { door, open } => Self::SetDoor { door, open },
            Fields::Move(direction) => Self::Move(direction),
            Fields::Take { item, quantity } => Self::Take { item, quantity },
            Fields::Drop { item, quantity } => Self::Drop { item, quantity },
            Fields::Wait => Self::Wait,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScheduleError {
    TimeExhausted,
    QueueFull,
    InvalidIntention,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameError {
    PopulationCapacity(prepared_spawn::PopulationCapacity),
    Build(creatures::BuildError),
    Creature(creatures::CreatureStateError),
    /// Automatic growth failed at candidate finalization, not action admission.
    /// Hosts must preserve the refused execution rather than retire its intention.
    Growth(creatures::CreatureStateError),
    Schedule(ScheduleError),
    Run(crate::run_policy::RunError),
    Rule(abilities::RuleError),
    UnknownActor,
    NotActorsTurn,
    InvalidLocation,
    Occupied,
    Blocked,
    /// No distinction between unknown, hidden, carried, and out-of-reach items.
    ItemUnavailable,
    InvalidQuantity,
    DoorUnavailable,
    IdentityExhausted,
    ActorBusy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutcomeKind {
    EscapeStarted {
        hold: linked_hold::HoldId,
    },
    GrowthSourceChosen {
        basis: player_advancement::AdvancementBasis,
        source: earned_growth::GrowthSource,
    },
    RespecStarted {
        basis: player_advancement::AdvancementBasis,
        replacement: player_respec::ReplacementOperation,
    },
    Purchased {
        basis: player_advancement::AdvancementBasis,
        purchase: player_advancement::PurchaseOperation,
    },
    AbilityStarted {
        id: content_ids::AbilityId,
        name: String,
        target: ActorId,
    },
    ItemStarted {
        work: Work,
    },
    DoorChanged {
        door: u64,
        open: bool,
    },
    Moved {
        from: Location,
        to: Location,
    },
    Taken {
        item: ItemId,
        result: ItemId,
        quantity: u64,
    },
    Dropped {
        item: ItemId,
        result: ItemId,
        quantity: u64,
    },
    Escaped {
        item: ItemId,
    },
    Waited,
}

/// An authoritative outcome, not a client message.
///
/// `at_tick` is the action's execution boundary. Immediate actions apply their
/// effects there, then recover; timed abilities and item work start or resume
/// preparation and apply their effects only when that work completes.
/// `next_tick` is the next scheduled actor's decision time, which may equal
/// `at_tick` when several actors are ready together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActionOutcome {
    pub actor: ActorId,
    pub at_tick: u64,
    pub kind: OutcomeKind,
    pub next_actor: Option<ActorId>,
    pub next_tick: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Actor {
    holds: linked_hold::HoldBook,
    behavior: behavior::Behavior,
    active_since: u64,
    anatomy: Shared<AnatomySpec>,
    equipment: BTreeMap<EquipmentSlotId, ItemId>,
    pending: Option<combat::Preparation>,
    combat: Option<combat::CombatState>,
    body: Shared<BodySpec>,
    motion: MotionState,
    location: Location,
    orientation: u8,
    turn_ticks: NonZeroU64,
    ready_at: u64,
    visited: BTreeSet<RegionId>,
    knowledge: Shared<BTreeSet<String>>,
    recognized_abilities: Shared<BTreeSet<content_ids::AbilityId>>,
    public_plan: Option<Shared<public_plan::PublicPlanObservation>>,
    /// The asset clients draw it with.
    asset: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
enum ItemLocation {
    Ground(Location),
    Carried(ActorId),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Item {
    /// Captured at death; authored item properties cannot create this provenance.
    corpse: Option<CorpseAppearance>,
    motion: MotionState,
    orientation: u8,
    spec: Shared<ItemSpec>,
    quantity: u64,
    #[serde(deserialize_with = "supplies::read_holds")]
    holds: Vec<supplies::ItemHold>,
    location: ItemLocation,
}
impl<'de> serde::Deserialize<'de> for Item {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "crate::bounded::required_option")]
            corpse: Option<CorpseAppearance>,
            motion: MotionState,
            orientation: u8,
            spec: Shared<ItemSpec>,
            quantity: u64,
            #[serde(deserialize_with = "supplies::read_holds")]
            holds: Vec<supplies::ItemHold>,
            location: ItemLocation,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            corpse: fields.corpse,
            motion: fields.motion,
            orientation: fields.orientation,
            spec: fields.spec,
            quantity: fields.quantity,
            holds: fields.holds,
            location: fields.location,
        })
    }
}

/// In-memory authoritative state. Persistence and controller ownership live in
/// the server layer. Scenario roles are explicit persisted identity data and do
/// not follow transient controller ownership.
#[derive(Debug, PartialEq, Eq)]
pub struct Game {
    content: std::sync::Arc<creatures::Content>,
    combat: combat::CombatWorld,
    rules_rng: u64,
    event_log: Vec<combat::CombatEvent>,
    /// Optional backend observations; deliberately excluded from checkpoints.
    combat_diagnostics: Option<Shared<resolution_diagnostics::CombatDiagnostics>>,
    survival_diagnostics: Option<Shared<survival_diagnostics::SurvivalBatch>>,
    physics: physics::Physics,
    navigation: BTreeMap<ActorId, Shared<travel::Navigation>>,
    explored_maps: BTreeMap<ActorId, Shared<explored_map::ExploredMap>>,
    map_validation: explored_map::MapValidationCache,
    world: Shared<World>,
    seed: u64,
    tick: u64,
    /// Actor-local clocks and the derived absolute deadline index avoid global timer walks.
    actors: actor_store::ActorStore,
    items: item_store::ItemStore,
    next_actor_id: u64,
    next_item_id: u64,
    next_door_id: u64,
    next_hold_id: u64,
    intentions: Shared<intention::IntentionQueue>,
    /// Region streaming state; empty unless regions were frozen or detached.
    lifecycle: streaming::Lifecycle,
    scenario_policy: floor_activation::ScenarioPolicy,
    floor_activation: Option<floor_activation::ActivationState>,
    unique_retrieval: Option<unique_retrieval::UniqueRetrievalState>,
    /// Internal continuation boundary, drained before engine admission. Not a
    /// client event or a serialized event queue.
    population_entry_boundary: Option<u64>,
    preparation_region_boundary: Option<theft::PreparationRegionBoundary>,
    host_region_continuation: bool,
}
impl Clone for Game {
    fn clone(&self) -> Self {
        crate::diagnostics::game_clone();
        Self {
            content: self.content.clone(),
            combat: self.combat.clone(),
            rules_rng: self.rules_rng,
            event_log: self.event_log.clone(),
            combat_diagnostics: self.combat_diagnostics.clone(),
            survival_diagnostics: self.survival_diagnostics.clone(),
            physics: self.physics.clone(),
            navigation: self.navigation.clone(),
            explored_maps: self.explored_maps.clone(),
            map_validation: self.map_validation.clone(),
            world: self.world.clone(),
            seed: self.seed,
            tick: self.tick,
            actors: self.actors.clone(),
            items: self.items.clone(),
            next_actor_id: self.next_actor_id,
            next_item_id: self.next_item_id,
            next_door_id: self.next_door_id,
            next_hold_id: self.next_hold_id,
            intentions: self.intentions.clone(),
            lifecycle: self.lifecycle.clone(),
            scenario_policy: self.scenario_policy.clone(),
            floor_activation: self.floor_activation.clone(),
            unique_retrieval: self.unique_retrieval.clone(),
            population_entry_boundary: self.population_entry_boundary,
            preparation_region_boundary: self.preparation_region_boundary.clone(),
            host_region_continuation: self.host_region_continuation,
        }
    }
}

/// Exact ceil(sqrt(2) * base), with checked integer arithmetic.
fn movement_cost(base: u64, direction: Direction) -> Result<u64, GameError> {
    if direction.components().is_none() {
        return Ok(base);
    }
    let squared = u128::from(base)
        .pow(2)
        .checked_mul(2)
        .ok_or(GameError::Schedule(crate::ScheduleError::TimeExhausted))?;
    let root = squared.isqrt();
    u64::try_from(root + u128::from(root * root != squared))
        .map_err(|_| GameError::Schedule(crate::ScheduleError::TimeExhausted))
}

impl Game {
    /// Package construction helpers keep authored identities stable across edits.
    pub fn authored_cell_valid(&self, at: Location) -> bool {
        self.world.contains(at) && !self.world.is_wall(at)
    }
    pub fn authored_links_clear(&self) -> bool {
        self.world.authored_links_clear()
    }
    /// Keep every allocation namespace beyond a retained future when restoring
    /// an older game. Existing entities keep their identity; newly created
    /// entities and work must not reuse identities from the abandoned branch.
    pub fn continue_identities(&mut self, later: &Game) {
        self.reserve_identities(later.next_actor_id, later.next_item_id, later.next_door_id);
        self.next_hold_id = self.next_hold_id.max(later.next_hold_id);
        self.continue_record_ids(later);
        self.continue_intention_ids(later);
    }

    pub fn spawn_authored_actor(
        &mut self,
        id: u64,
        at: Location,
        ticks: NonZeroU64,
    ) -> Result<ActorId, GameError> {
        if id == 0 || id == u64::MAX || self.actors.contains_key(&ActorId(id)) {
            return Err(GameError::IdentityExhausted);
        }
        let next = self.next_actor_id;
        self.next_actor_id = id;
        let result = self.spawn_actor(at, ticks);
        self.next_actor_id = if result.is_ok() {
            self.next_actor_id.max(next)
        } else {
            next
        };
        result
    }
    pub fn place_authored_item(
        &mut self,
        id: u64,
        at: Location,
        name: String,
        owner: Option<ActorId>,
    ) -> Result<ItemId, GameError> {
        if id == 0 || id == u64::MAX || self.items.contains_key(&ItemId(id)) {
            return Err(GameError::IdentityExhausted);
        }
        if owner.is_some_and(|a| !self.actors.contains_key(&a)) {
            return Err(GameError::UnknownActor);
        }
        let next = self.next_item_id;
        self.next_item_id = id;
        let result = self.place_item(at, name);
        self.next_item_id = if result.is_ok() {
            self.next_item_id.max(next)
        } else {
            next
        };
        if let (Ok(item), Some(actor)) = (&result, owner) {
            self.items
                .edit(*item, |item| item.location = ItemLocation::Carried(actor))
                .expect("placed item");
        }
        result
    }
    pub fn place_authored_door(
        &mut self,
        id: u64,
        at: Location,
        open: bool,
        height: u8,
    ) -> Result<u64, GameError> {
        if id == 0 || id == u64::MAX || self.world.door_location(id).is_some() {
            return Err(GameError::IdentityExhausted);
        }
        let next = self.next_door_id;
        self.next_door_id = id;
        let result = self.place_door(at, open, height);
        self.next_door_id = if result.is_ok() {
            self.next_door_id.max(next)
        } else {
            next
        };
        result
    }
    fn reach(&self, from: Location, direction: Direction) -> Option<(Location, u8)> {
        if direction.components().is_some() {
            self.world
                .diagonal_reach(from, direction, |side| side == from || !self.occupied(side))
        } else {
            self.world
                .adjacent(from, direction)
                .map(|to| (to, self.world.crossing_rotation(from, direction)))
        }
    }

    pub fn content(&self) -> &std::sync::Arc<creatures::Content> {
        &self.content
    }

    pub fn new(world: World, seed: u64, content: std::sync::Arc<creatures::Content>) -> Self {
        Self {
            content,
            combat: combat::CombatWorld::new(),
            rules_rng: seed,
            event_log: Vec::new(),
            combat_diagnostics: None,
            survival_diagnostics: None,
            physics: physics::Physics::default(),
            navigation: BTreeMap::new(),
            explored_maps: BTreeMap::new(),
            map_validation: Default::default(),
            world: Shared::new(world),
            seed,
            tick: 0,
            actors: actor_store::ActorStore::default(),
            items: item_store::ItemStore::default(),
            next_actor_id: 1,
            next_item_id: 1,
            next_door_id: 1,
            next_hold_id: 1,
            intentions: Shared::default(),
            lifecycle: streaming::Lifecycle::default(),
            scenario_policy: floor_activation::ScenarioPolicy::default(),
            floor_activation: None,
            unique_retrieval: None,
            population_entry_boundary: None,
            preparation_region_boundary: None,
            host_region_continuation: false,
        }
    }

    /// Scenario setup operation; a network client cannot spawn an actor directly.
    /// Movement/wait use `turn_ticks`; taking an item uses half, rounded upward.
    pub fn spawn_actor(
        &mut self,
        location: Location,
        turn_ticks: NonZeroU64,
    ) -> Result<ActorId, GameError> {
        if !self.world.walkable(location) {
            return Err(GameError::InvalidLocation);
        }
        if self.occupied(location) {
            return Err(GameError::Occupied);
        }
        let next = self
            .next_actor_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let id = ActorId(self.next_actor_id);
        self.actors.insert(
            id,
            Actor {
                holds: linked_hold::HoldBook::default(),
                behavior: behavior::Behavior::new(self.seed, id),
                active_since: self.tick,
                anatomy: Shared::default(),
                equipment: BTreeMap::new(),
                combat: None,
                pending: None,
                body: Shared::default(),
                motion: MotionState::default(),
                location,
                orientation: 0,
                turn_ticks,
                ready_at: self.tick,
                visited: BTreeSet::from([location.region]),
                knowledge: Shared::default(),
                recognized_abilities: Shared::default(),
                public_plan: None,
                asset: None,
            },
        );
        self.next_actor_id = next;
        self.sync_actor_lifecycle(id)?;
        Ok(id)
    }

    /// Scenario setup operation, distinct from player inventory manipulation.
    pub fn place_item(&mut self, location: Location, name: String) -> Result<ItemId, GameError> {
        if !self.world.walkable(location) {
            return Err(GameError::InvalidLocation);
        }
        let next = self
            .next_item_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let id = ItemId(self.next_item_id);
        self.items.insert(
            id,
            Item {
                corpse: None,
                holds: vec![],
                motion: MotionState::default(),
                orientation: 0,
                spec: Shared::new(ItemSpec::ordinary(name)),
                quantity: 1,
                location: ItemLocation::Ground(location),
            },
        );
        self.next_item_id = next;
        Ok(id)
    }

    /// How tall a door based at `at` could be.
    pub fn door_clearance(&self, at: Location) -> u8 {
        self.world.door_clearance(at)
    }

    /// Whether a door this tall at `at` would leave its doorway open above it.
    pub fn doorway_open_above(&self, at: Location, height: u8) -> bool {
        self.world.doorway_open_above(at, height)
    }

    /// Author a door `height` cells tall; a closed door cannot cover an actor
    /// or ground object in any of its cells.
    pub fn place_door(
        &mut self,
        location: Location,
        open: bool,
        height: u8,
    ) -> Result<u64, GameError> {
        if !open && self.door_obstructed(location, height) {
            return Err(GameError::InvalidLocation);
        }
        let next = self
            .next_door_id
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        let id = self.next_door_id;
        let affected = self.hold_geometry_candidates((0..i32::from(height)).filter_map(|offset| {
            location.position.z.checked_add(offset).map(|z| Location {
                position: Position {
                    z,
                    ..location.position
                },
                ..location
            })
        }));
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .place_door(location, id, open, height)
                .map_err(|_| GameError::InvalidLocation)?;
            candidate.next_door_id = next;
            candidate.reconcile_holds_for_actors(affected)?;
            Ok(id)
        })
    }
    /// Whether an actor or ground object is in any cell of a door based at
    /// `base` and `height` cells tall.
    fn door_obstructed(&self, base: Location, height: u8) -> bool {
        (0..i32::from(height)).any(|cells| {
            let Some(z) = base.position.z.checked_add(cells) else {
                return false;
            };
            let cell = Location {
                position: Position { z, ..base.position },
                ..base
            };
            self.occupied(cell) || self.items.at(ItemLocation::Ground(cell)).next().is_some()
        })
    }
    pub fn door_reachable_from(&self, from: Location, door: Location) -> bool {
        Direction::HORIZONTAL.into_iter().any(|direction| {
            self.reach(from, direction)
                .is_some_and(|(to, _)| to == door)
        })
    }

    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Authorized setup relocation; preserves recovery time and reveals the destination.
    pub fn teleport(&mut self, id: ActorId, location: Location) -> Result<(), GameError> {
        if !self.actors.contains_key(&id) {
            return Err(GameError::UnknownActor);
        }
        if !self.world.walkable(location) {
            return Err(GameError::InvalidLocation);
        }
        if !self.body_fits(id, location, 0, &self.actors[&id].body) {
            return Err(GameError::Occupied);
        }
        // A frozen actor's time stands at its freeze; syncing below shifts it.
        let clock = self.actor_clock(id);
        let mut outcome = self.actor_outcome(id)?;
        outcome.entry_cause = floor_activation::EntryCause::Teleport;
        outcome.discard(clock)?;
        let actor = &mut outcome.state;
        actor.location = location;
        actor.orientation = 0;
        actor.motion = MotionState::default();
        actor.visited.insert(location.region);
        // Entry population may fail after relocation or a successful spawn
        // prefix, even when no actor owns contextual recovery. Keep that work
        // inside the same candidate boundary as recovery reconciliation.
        let population_entry =
            self.population_participates_in_entry(id, location) || self.unique_retrieval.is_some();
        self.with_recovery_transition(population_entry, |candidate| {
            if !candidate.host_region_continuation {
                candidate.begin_retrieval_transaction();
            }
            candidate.apply_outcome(outcome)?;
            candidate.sync_actor_lifecycle(id)
        })
    }

    /// Set the asset clients draw an actor with.
    pub fn set_actor_asset(&mut self, id: ActorId, asset: Option<String>) -> Result<(), GameError> {
        let mut actor = self.actors.get_mut(&id).ok_or(GameError::UnknownActor)?;
        actor.asset = asset;
        Ok(())
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn connect_area(
        &mut self,
        passage: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), GameError> {
        self.world
            .connect_area(passage, turns, width, height)
            .map_err(|_| GameError::InvalidLocation)
    }

    /// Establish a terrain-preserving join across horizontal region faces.
    pub fn connect_boundary_area(
        &mut self,
        passage: Passage,
        turns: u8,
        width: u16,
        height: u16,
    ) -> Result<(), GameError> {
        self.world
            .connect_boundary_area(passage, turns, width, height)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn add_region(&mut self, region: Region) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            // Replacing known absent metadata can change membership bounds.
            // Ordinary new regions and residency changes preserve root proofs.
            if candidate.world.knows_region(region.id) {
                candidate.map_validation = Default::default();
            }
            candidate
                .world
                .add_region(region)
                .map_err(|_| GameError::InvalidLocation)
        })
    }

    pub fn add_chamber(&mut self, region: Region) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            // Replacing known absent metadata can change membership bounds.
            // Ordinary new regions and residency changes preserve root proofs.
            if candidate.world.knows_region(region.id) {
                candidate.map_validation = Default::default();
            }
            candidate
                .world
                .add_chamber(region)
                .map_err(|_| GameError::InvalidLocation)
        })
    }

    pub fn connect(&mut self, passage: Passage, quarter_turns: u8) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .connect(passage, quarter_turns)
                .map_err(|_| GameError::InvalidLocation)
        })
    }

    pub fn connect_named_stair(
        &mut self,
        from: Location,
        direction: Direction,
        to: tor_world::NamedAnchor,
    ) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .connect_named_stair(from, direction, to)
                .map_err(|_| GameError::InvalidLocation)
        })
    }

    pub fn register_named_anchors(
        &mut self,
        region: RegionId,
        anchors: BTreeMap<String, tor_world::Position>,
    ) -> Result<(), GameError> {
        self.world
            .register_named_anchors(region, anchors)
            .map_err(|_| GameError::InvalidLocation)
    }

    /// Map-authoring metadata; no action time, names, boundaries or travel rules.
    /// A place hint the character learns by this authored name.
    pub fn set_named_place_hint(
        &mut self,
        location: Location,
        name: &str,
    ) -> Result<(), GameError> {
        if !places::valid_name(name) {
            return Err(GameError::InvalidLocation);
        }
        self.world
            .set_named_place_hint(location, name)
            .map_err(|_| GameError::InvalidLocation)
    }

    pub fn set_place_hint(&mut self, location: Location, present: bool) -> Result<(), GameError> {
        self.world
            .set_place_hint(location, present)
            .map_err(|_| GameError::InvalidLocation)
    }

    /// Process-local geometry witness for derived backend work. Lighting and
    /// entity motion do not change it; terrain, doors and topology do.
    pub fn geometry_snapshot(&self) -> tor_world::GeometrySnapshot {
        self.world.geometry_snapshot()
    }

    pub fn set_region_light(&mut self, region: RegionId, lit: bool) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .set_region_light(region, lit)
                .map_err(|_| GameError::InvalidLocation)
        })
    }
    pub fn set_cell_light(&mut self, at: Location, lit: bool) -> Result<(), GameError> {
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .set_cell_light(at, lit)
                .map_err(|_| GameError::InvalidLocation)
        })
    }

    pub fn set_wall(&mut self, location: Location, wall: bool) -> Result<(), GameError> {
        if wall
            && (self.occupied(location)
                || self
                    .items
                    .at(ItemLocation::Ground(location))
                    .next()
                    .is_some())
        {
            return Err(GameError::InvalidLocation);
        }
        let affected = self.hold_geometry_candidates([location]);
        self.with_recovery_transition(false, |candidate| {
            candidate
                .world
                .set_wall(location, wall)
                .map_err(|_| GameError::InvalidLocation)?;
            candidate.reconcile_holds_for_actors(affected)
        })
    }

    /// Stable ordering: earliest ready time, then actor identity. Frozen
    /// actors never act.
    pub fn next_actor(&self) -> Option<ActorId> {
        if !self.arena_execution_enabled() {
            return None;
        }
        self.next_scheduled_actor()
    }

    /// Read-only scheduler identity, including while an arena is paused.
    /// This grants no permission to execute work.
    pub fn next_scheduled_actor(&self) -> Option<ActorId> {
        if self.run_outcome().terminal {
            return None;
        }
        if let Some(id) = self.combat.input_boundaries.keys().find(|id| {
            self.actors
                .get(id)
                .is_some_and(|a| a.alive() && a.ready_at <= self.tick)
                && !self.actor_frozen(**id)
                && self.condition_allows_voluntary_input(**id)
        }) {
            return Some(*id);
        }
        self.actors
            .iter()
            .filter(|(id, a)| {
                a.alive()
                    && a.pending.as_ref().is_none_or(|p| !p.active)
                    && !self.actor_frozen(**id)
                    && self.condition_allows_voluntary_input(**id)
            })
            .min_by_key(|(id, actor)| (actor.ready_at, **id))
            .map(|(id, _)| *id)
    }

    fn occupied(&self, location: Location) -> bool {
        self.actors
            .at(&self.world, location)
            .keys()
            .any(|id| self.actors[id].alive())
    }
}

#[cfg(test)]
mod sharing_tests {
    use super::*;
    use tor_world::Position;
    #[test]
    fn snapshots_share_unchanged_world_items_and_navigation_but_isolate_mutations() {
        let mut game = Game::two_room(42, crate::test_creatures::content());
        let location = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game
            .spawn_actor(location, NonZeroU64::new(100).unwrap())
            .unwrap();
        game.refresh_navigation();
        let original = game.clone();
        game.act(actor, Action::Wait).unwrap();
        game.refresh_navigation();
        assert!(game.world.shares_storage(&original.world));
        assert!(game.items.shares_storage(&original.items));
        assert!(game.navigation[&actor].shares_storage(&original.navigation[&actor]));
        game.place_item(location, "new token".into()).unwrap();
        assert_ne!(game.items, original.items);
        assert!(game.world.shares_storage(&original.world));
        let scene = game.scene(actor).unwrap();
        let observation = game.observe(actor).unwrap();
        let counts = diagnostics::work_counts();
        let (combined, projected) = game.observe_scene(actor).unwrap();
        let after = diagnostics::work_counts();
        assert_eq!(
            after.scenes - counts.scenes,
            0,
            "reuse the unchanged projected scene"
        );
        assert_eq!(after.observations - counts.observations, 1);
        assert_eq!(combined, observation);
        assert_eq!(projected, scene);
    }
}

#[cfg(test)]
extern crate self as tor_simulation;

#[cfg(test)]
#[path = "../test_support/creatures.rs"]
mod test_creatures;

#[cfg(test)]
mod behavior_tests;
