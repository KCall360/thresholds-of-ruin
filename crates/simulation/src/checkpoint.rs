//! Backend-only deterministic checkpoint state. Equal world and navigation
//! regions, actor tables, map-owner directories, intentions and population states
//! are encoded once across rewind boundaries.
//! This module does not perform storage or I/O.
use crate::actor_record::ActorRecord;
use crate::creatures::Content;
use crate::explored_map::ExploredMap;
use crate::streaming::Lifecycle;
use crate::{travel::Navigation, Actor, ActorId, Game, Item, ItemId, ItemLocation};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use tor_world::Location;
use tor_world::{Shared, World};

/// A malformed checkpoint is rejected before any restored game is published.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PersistError {
    MissingShared {
        table: &'static str,
        index: usize,
    },
    Navigation,
    ExploredMap,
    /// Includes missing catalog references and invalid mutable creature state.
    CreatureRecord {
        actor: ActorId,
        error: crate::creatures::CreatureStateError,
    },
    Equipment {
        actor: ActorId,
        item: ItemId,
    },
    ActorClock {
        actor: ActorId,
    },
    ActorState {
        actor: ActorId,
    },
    ItemDefinition {
        item: ItemId,
        error: crate::rules::RulesetError,
    },
    ItemState {
        item: ItemId,
    },
    Intentions,
    Lifecycle,
    Population,
    Retrieval,
    Physics,
    RunState,
    Schedule,
    World,
}
impl std::fmt::Display for PersistError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid checkpoint: {self:?}")
    }
}
impl std::error::Error for PersistError {}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    combat: crate::combat::CombatWorld,
    rules_rng: u64,
    #[serde(deserialize_with = "read_events")]
    events: Vec<crate::combat::CombatEvent>,
    event_receipts: crate::event_history::EventReceipts,
    physics: crate::physics::Physics,
    world: usize,
    navigation: BTreeMap<ActorId, usize>,
    explored_maps: usize,
    seed: u64,
    tick: u64,
    actors: usize,
    items: usize,
    next_actor_id: u64,
    next_item_id: u64,
    next_door_id: u64,
    next_hold_id: u64,
    intentions: crate::intention::checkpoint::Snapshot,
    /// Region streaming state in [`SharedState`], omitted while empty so
    /// games that never stream save exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lifecycle: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scenario_policy: Option<crate::floor_activation::ScenarioPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    floor_activation: Option<usize>,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    unique_retrieval: Option<RetrievalSnapshot>,
}

fn read_events<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<crate::combat::CombatEvent>, D::Error> {
    Ok(crate::bounded::Bounded::<crate::combat::CombatEvent, { crate::limits::EVENTS_PER_BATCH }>::deserialize(d)?.0)
}

/// Mutable objective evidence stays small; neither visits nor policy changes
/// cause the large once-only population snapshot to lose pooling.
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct RetrievalSnapshot {
    policy: usize,
    initialized: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    item: Option<ItemId>,
    entry_count: u64,
    placement_count: u64,
    #[serde(deserialize_with = "crate::unique_retrieval::read_entries")]
    entries: Vec<crate::unique_retrieval::RetrievalEntry>,
}

impl<'de> Deserialize<'de> for RetrievalSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            policy: usize,
            initialized: bool,
            #[serde(deserialize_with = "crate::bounded::required_option")]
            item: Option<ItemId>,
            entry_count: u64,
            placement_count: u64,
            #[serde(deserialize_with = "crate::unique_retrieval::read_entries")]
            entries: Vec<crate::unique_retrieval::RetrievalEntry>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            policy: fields.policy,
            initialized: fields.initialized,
            item: fields.item,
            entry_count: fields.entry_count,
            placement_count: fields.placement_count,
            entries: fields.entries,
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedState {
    #[serde(with = "tor_world::checkpoint_worlds")]
    worlds: Vec<World>,
    #[serde(with = "navigation_regions")]
    navigation: Vec<Navigation>,
    #[serde(with = "explored_map_regions")]
    explored_maps: Vec<Shared<ExploredMap>>,
    /// Equal actor-to-root directories across retained boundaries are encoded once.
    #[serde(with = "explored_map_owners")]
    explored_map_owners: Vec<BTreeMap<ActorId, usize>>,
    items: Vec<BTreeMap<ItemId, Item>>,
    actors: Vec<BTreeMap<ActorId, ActorRecord>>,
    intentions: crate::intention::checkpoint::Pool,
    /// Identical lifecycle states (with their identity directories) across
    /// rewind boundaries are encoded once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    lifecycles: Vec<Lifecycle>,
    /// Complete equal population inputs and receipts are encoded once, rather
    /// than repeating the same prepared floor cells at every ordinary boundary.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    floor_activations: Vec<crate::floor_activation::ActivationState>,
    #[serde(deserialize_with = "read_retrieval_policies")]
    retrieval_policies: Vec<Shared<crate::unique_retrieval::UniqueRetrievalPolicy>>,
}
fn read_retrieval_policies<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<Shared<crate::unique_retrieval::UniqueRetrievalPolicy>>, D::Error> {
    // One run has one immutable policy, even across rewind boundaries.
    crate::bounded::Bounded::<Shared<crate::unique_retrieval::UniqueRetrievalPolicy>, 1>::deserialize(d).map(|policies| policies.0)
}

/// Each directory is an ordered sequence of actor/root bindings. The pool may
/// contain one empty directory, but never duplicate directories or bindings.
mod explored_map_owners {
    use super::*;

    pub fn serialize<S: serde::Serializer>(
        directories: &[BTreeMap<ActorId, usize>],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        #[serde(transparent)]
        struct Directory<'a>(
            #[serde(with = "tor_world::checkpoint_map")] &'a BTreeMap<ActorId, usize>,
        );
        serializer.collect_seq(directories.iter().map(Directory))
    }

    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<BTreeMap<ActorId, usize>>, D::Error> {
        #[derive(Deserialize)]
        #[serde(transparent)]
        struct Directory(#[serde(with = "tor_world::checkpoint_map")] BTreeMap<ActorId, usize>);
        let directories = Vec::<Directory>::deserialize(deserializer)?;
        let mut unique = BTreeSet::new();
        if directories.iter().any(|directory| {
            directory.0.keys().any(|actor| actor.0 == 0) || !unique.insert(&directory.0)
        }) {
            return Err(serde::de::Error::custom(
                "invalid or duplicate explored map owner directory",
            ));
        }
        drop(unique);
        Ok(directories
            .into_iter()
            .map(|directory| directory.0)
            .collect())
    }
}

impl SharedState {
    pub fn valid_world_count(&self, maximum: usize) -> bool {
        !self.worlds.is_empty() && self.worlds.len() <= maximum
    }

    /// Call with the current snapshot and every retained rewind snapshot before
    /// restoration. Each owner directory must be referenced, and every binding
    /// must name an actor present at that boundary and a referenced map root.
    pub fn explored_map_references_valid<'a>(
        &self,
        snapshots: impl IntoIterator<Item = &'a Snapshot>,
    ) -> bool {
        if !self.explored_map_owners_unique() {
            return false;
        }
        let mut used_owners = BTreeSet::new();
        let mut used_roots = BTreeSet::new();
        for snapshot in snapshots {
            let Some(owners) = self.explored_map_owners.get(snapshot.explored_maps) else {
                return false;
            };
            let Some(actors) = self.actors.get(snapshot.actors) else {
                return false;
            };
            used_owners.insert(snapshot.explored_maps);
            for (&actor, &index) in owners {
                if actor.0 == 0 || !actors.contains_key(&actor) || index >= self.explored_maps.len()
                {
                    return false;
                }
                used_roots.insert(index);
            }
        }
        used_owners.len() == self.explored_map_owners.len()
            && used_roots.len() == self.explored_maps.len()
    }

    fn explored_map_owners_unique(&self) -> bool {
        let mut unique = BTreeSet::new();
        self.explored_map_owners
            .iter()
            .all(|directory| unique.insert(directory))
    }
}

/// Ownership shared while restoring one decoded checkpoint and its rewind states.
/// This context is never serialized. Dropping it releases its temporary pool
/// references; restored games retain only the definitions they actually use.
pub struct RestoreContext<'a> {
    shared: &'a SharedState,
    content: Arc<Content>,
    worlds: BTreeMap<usize, Shared<World>>,
    navigation: BTreeMap<usize, Shared<Navigation>>,
    /// Validation is reusable only for these immutable decoded pool entries.
    validated_navigation: BTreeSet<(usize, usize)>,
    unique_explored_map_owners: bool,
    explored_maps: Option<explored_map_regions::RestoreValidation>,
    items: BTreeMap<usize, crate::item_store::ItemStore>,
    actors: BTreeMap<(usize, usize), crate::actor_store::ActorStore>,
    intentions: crate::intention::checkpoint::Restore,
    bodies: BTreeSet<Shared<crate::BodySpec>>,
    item_definitions: BTreeSet<Shared<crate::ItemSpec>>,
    anatomies: BTreeSet<Shared<crate::AnatomySpec>>,
}

impl<'a> RestoreContext<'a> {
    pub fn new(shared: &'a SharedState, content: Arc<Content>) -> Self {
        Self {
            shared,
            content,
            worlds: BTreeMap::new(),
            navigation: BTreeMap::new(),
            validated_navigation: BTreeSet::new(),
            unique_explored_map_owners: shared.explored_map_owners_unique(),
            explored_maps: None,
            items: BTreeMap::new(),
            actors: BTreeMap::new(),
            intentions: crate::intention::checkpoint::Restore::default(),
            bodies: BTreeSet::new(),
            item_definitions: BTreeSet::new(),
            anatomies: BTreeSet::new(),
        }
    }

    pub fn restore(&mut self, snapshot: Snapshot) -> Result<Game, PersistError> {
        Game::restore_with_context(snapshot, self)
    }

    fn floor_activation(
        &self,
        index: usize,
    ) -> Result<crate::floor_activation::ActivationState, PersistError> {
        // ActivationState's immutable fields already use Shared ownership.
        // Cloning the decoded pool value preserves those allocations across
        // restored games without caching boundary-dependent validation.
        self.shared
            .floor_activations
            .get(index)
            .cloned()
            .ok_or(PersistError::MissingShared {
                table: "floor_activations",
                index,
            })
    }

    fn world(&mut self, index: usize) -> Option<Shared<World>> {
        if let Some(world) = self.worlds.get(&index) {
            return Some(world.clone());
        }
        let world = Shared::new(self.shared.worlds.get(index)?.clone());
        self.worlds.insert(index, world.clone());
        Some(world)
    }

    fn navigation(&mut self, index: usize, world: usize) -> Option<Shared<Navigation>> {
        if !self.validated_navigation.contains(&(index, world)) {
            if !self
                .shared
                .navigation
                .get(index)?
                .checkpoint_valid(self.shared.worlds.get(world)?)
            {
                return None;
            }
            self.validated_navigation.insert((index, world));
        }
        if let Some(navigation) = self.navigation.get(&index) {
            return Some(navigation.clone());
        }
        let navigation = Shared::new(self.shared.navigation.get(index)?.clone());
        self.navigation.insert(index, navigation.clone());
        Some(navigation)
    }

    fn explored_maps(
        &mut self,
        index: usize,
        actors: usize,
        world: usize,
    ) -> Result<BTreeMap<ActorId, Shared<ExploredMap>>, PersistError> {
        if !self.unique_explored_map_owners {
            return Err(PersistError::ExploredMap);
        }
        let shared = self.shared;
        let owners = shared
            .explored_map_owners
            .get(index)
            .ok_or(PersistError::ExploredMap)?;
        let actors = shared
            .actors
            .get(actors)
            .ok_or(PersistError::MissingShared {
                table: "actors",
                index: actors,
            })?;
        if owners
            .keys()
            .any(|actor| actor.0 == 0 || !actors.contains_key(actor))
        {
            return Err(PersistError::ExploredMap);
        }
        owners
            .iter()
            .map(|(&actor, &root)| Some((actor, self.explored_map(root, world)?)))
            .collect::<Option<_>>()
            .ok_or(PersistError::ExploredMap)
    }

    fn explored_map(&mut self, index: usize, world: usize) -> Option<Shared<ExploredMap>> {
        if self.explored_maps.is_none() {
            self.explored_maps = Some(explored_map_regions::RestoreValidation::new(
                &self.shared.explored_maps,
            )?);
        }
        self.explored_maps
            .as_mut()?
            .validate(index, self.shared.worlds.get(world)?)?;
        self.shared.explored_maps.get(index).cloned()
    }

    fn items(&mut self, index: usize) -> Result<crate::item_store::ItemStore, PersistError> {
        if let Some(items) = self.items.get(&index) {
            return Ok(items.clone());
        }
        let mut entries = self
            .shared
            .items
            .get(index)
            .ok_or(PersistError::MissingShared {
                table: "items",
                index,
            })?
            .clone();
        for (id, item) in &mut entries {
            item.spec
                .validate_rules(self.content.rules())
                .map_err(|error| PersistError::ItemDefinition { item: *id, error })?;
            share_definition(&mut self.item_definitions, &mut item.spec);
        }
        let items = crate::item_store::ItemStore::from_entries(entries);
        self.items.insert(index, items.clone());
        Ok(items)
    }

    fn actors(
        &mut self,
        index: usize,
        item_index: usize,
    ) -> Result<crate::actor_store::ActorStore, PersistError> {
        if let Some(actors) = self.actors.get(&(index, item_index)) {
            return Ok(actors.clone());
        }
        let records = self
            .shared
            .actors
            .get(index)
            .ok_or(PersistError::MissingShared {
                table: "actors",
                index,
            })?;
        let items = self
            .shared
            .items
            .get(item_index)
            .ok_or(PersistError::MissingShared {
                table: "items",
                index: item_index,
            })?;
        let mut entries = records
            .iter()
            .map(|(id, record)| {
                record
                    .restore(*id, self.content.clone(), items)
                    .map(|actor| (*id, actor))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        self.share_actor_definitions(&mut entries);
        let actors = crate::actor_store::ActorStore::from_entries(entries);
        self.actors.insert((index, item_index), actors.clone());
        Ok(actors)
    }

    fn share_actor_definitions(&mut self, actors: &mut BTreeMap<ActorId, Actor>) {
        for actor in actors.values_mut() {
            share_definition(&mut self.anatomies, &mut actor.anatomy);
            share_definition(&mut self.bodies, &mut actor.body);
        }
    }
}

/// Compare the complete value, never its identity label or allocation address.
/// Borrowing the key avoids copying a definition merely to look it up.
fn share_definition<T: Ord>(pool: &mut BTreeSet<Shared<T>>, definition: &mut Shared<T>) {
    if let Some(existing) = pool.get(&**definition) {
        *definition = existing.clone();
    } else {
        pool.insert(definition.clone());
    }
}

impl Game {
    pub fn checkpoint(&self, shared: &mut SharedState) -> Snapshot {
        assert!(
            !self.region_transition_pending()
                && !self.host_region_continuation
                && !self.event_observations_open(),
            "a private host candidate must settle and seal events before checkpoint capture"
        );
        let worlds = &mut shared.worlds;
        let world = worlds
            .iter()
            .position(|w| w == &*self.world)
            .unwrap_or_else(|| {
                worlds.push((*self.world).clone());
                worlds.len() - 1
            });
        let actor_records: BTreeMap<_, _> = self
            .actors
            .iter()
            .map(|(id, actor)| (*id, ActorRecord::capture(actor)))
            .collect();
        let explored_map_owners: BTreeMap<_, _> = self
            .explored_maps
            .iter()
            .map(|(actor, map)| {
                let index = shared
                    .explored_maps
                    .iter()
                    .position(|existing| existing.shares_storage(map) || existing == map)
                    .unwrap_or_else(|| {
                        shared.explored_maps.push(map.clone());
                        shared.explored_maps.len() - 1
                    });
                (*actor, index)
            })
            .collect();
        Snapshot {
            combat: self.combat.clone(),
            rules_rng: self.rules_rng,
            events: self.event_log.clone(),
            event_receipts: self.event_receipts.clone(),
            physics: self.physics.clone(),
            world,
            navigation: self
                .navigation
                .iter()
                .map(|(actor, navigation)| {
                    let index = shared
                        .navigation
                        .iter()
                        .position(|n| n == &**navigation)
                        .unwrap_or_else(|| {
                            shared.navigation.push((**navigation).clone());
                            shared.navigation.len() - 1
                        });
                    (*actor, index)
                })
                .collect(),
            explored_maps: shared
                .explored_map_owners
                .iter()
                .position(|owners| owners == &explored_map_owners)
                .unwrap_or_else(|| {
                    shared.explored_map_owners.push(explored_map_owners);
                    shared.explored_map_owners.len() - 1
                }),
            seed: self.seed,
            tick: self.tick,
            actors: shared
                .actors
                .iter()
                .position(|actors| actors == &actor_records)
                .unwrap_or_else(|| {
                    shared.actors.push(actor_records);
                    shared.actors.len() - 1
                }),
            items: shared
                .items
                .iter()
                .position(|items| items == &*self.items)
                .unwrap_or_else(|| {
                    shared.items.push((*self.items).clone());
                    shared.items.len() - 1
                }),
            next_actor_id: self.next_actor_id,
            next_item_id: self.next_item_id,
            next_door_id: self.next_door_id,
            next_hold_id: self.next_hold_id,
            intentions: shared.intentions.capture(&self.intentions),
            scenario_policy: (self.scenario_policy != Default::default())
                .then(|| self.scenario_policy.clone()),
            floor_activation: self.floor_activation.as_ref().map(|state| {
                let states = &mut shared.floor_activations;
                states
                    .iter()
                    .position(|pooled| pooled == state)
                    .unwrap_or_else(|| {
                        states.push(state.clone());
                        states.len() - 1
                    })
            }),
            unique_retrieval: self.unique_retrieval.as_ref().map(|state| {
                let policies = &mut shared.retrieval_policies;
                let policy = policies
                    .iter()
                    .position(|policy| **policy == *state.policy)
                    .unwrap_or_else(|| {
                        policies.push(state.policy.clone());
                        policies.len() - 1
                    });
                RetrievalSnapshot {
                    policy,
                    initialized: state.initialized,
                    item: state.item,
                    entry_count: state.entry_count,
                    placement_count: state.placement_count,
                    entries: state.entries.clone(),
                }
            }),
            lifecycle: (!self.lifecycle.is_empty()).then(|| {
                let lifecycles = &mut shared.lifecycles;
                lifecycles
                    .iter()
                    .position(|l| l == &self.lifecycle)
                    .unwrap_or_else(|| {
                        lifecycles.push(self.lifecycle.clone());
                        lifecycles.len() - 1
                    })
            }),
        }
    }

    pub fn restore_checkpoint(
        snapshot: Snapshot,
        shared: &SharedState,
        content: Arc<Content>,
    ) -> Result<Self, PersistError> {
        RestoreContext::new(shared, content).restore(snapshot)
    }

    fn restore_with_context(
        snapshot: Snapshot,
        context: &mut RestoreContext<'_>,
    ) -> Result<Self, PersistError> {
        let items = context.items(snapshot.items)?;
        let mut game = Self {
            content: context.content.clone(),
            combat: snapshot.combat,
            rules_rng: snapshot.rules_rng,
            event_log: snapshot.events,
            event_receipts: snapshot.event_receipts,
            event_origins: Default::default(),
            combat_diagnostics: None,
            survival_diagnostics: None,
            physics: snapshot.physics,
            world: context
                .world(snapshot.world)
                .ok_or(PersistError::MissingShared {
                    table: "worlds",
                    index: snapshot.world,
                })?,
            navigation: snapshot
                .navigation
                .into_iter()
                .map(|(actor, index)| Some((actor, context.navigation(index, snapshot.world)?)))
                .collect::<Option<_>>()
                .ok_or(PersistError::Navigation)?,
            explored_maps: context.explored_maps(
                snapshot.explored_maps,
                snapshot.actors,
                snapshot.world,
            )?,
            map_validation: Default::default(),
            seed: snapshot.seed,
            tick: snapshot.tick,
            actors: context.actors(snapshot.actors, snapshot.items)?,
            items,
            next_actor_id: snapshot.next_actor_id,
            next_item_id: snapshot.next_item_id,
            next_door_id: snapshot.next_door_id,
            next_hold_id: snapshot.next_hold_id,
            intentions: context
                .intentions
                .restore(snapshot.intentions, &context.shared.intentions)
                .ok_or(PersistError::Intentions)?,
            scenario_policy: snapshot.scenario_policy.unwrap_or_default(),
            floor_activation: snapshot
                .floor_activation
                .map(|index| context.floor_activation(index))
                .transpose()?,
            unique_retrieval: snapshot
                .unique_retrieval
                .map(|state| {
                    let policy = context
                        .shared
                        .retrieval_policies
                        .get(state.policy)
                        .cloned()
                        .ok_or(PersistError::MissingShared {
                            table: "retrieval_policies",
                            index: state.policy,
                        })?;
                    Ok(crate::unique_retrieval::UniqueRetrievalState {
                        policy,
                        initialized: state.initialized,
                        item: state.item,
                        entry_count: state.entry_count,
                        placement_count: state.placement_count,
                        entries: state.entries,
                    })
                })
                .transpose()?,
            population_entry_boundary: None,
            preparation_region_boundary: None,
            host_region_continuation: false,
            lifecycle: match snapshot.lifecycle {
                None => Lifecycle::default(),
                // An empty state is always omitted, so encodings stay unique.
                Some(index) => context
                    .shared
                    .lifecycles
                    .get(index)
                    .filter(|l| !l.is_empty())
                    .cloned()
                    .ok_or(PersistError::Lifecycle)?,
            },
        };
        if !game.retrieval_state_valid() {
            return Err(PersistError::Retrieval);
        }
        if !game.activation_state_valid() {
            return Err(PersistError::Population);
        }
        if !game.intentions_valid() {
            return Err(PersistError::Intentions);
        }
        if !game.lifecycle_state_valid() {
            return Err(PersistError::Lifecycle);
        }
        if !game.item_claims_valid(false) {
            return Err(PersistError::Intentions);
        }
        game.rebuild_timer_activity();
        if !game.physics_valid() {
            return Err(PersistError::Physics);
        }
        if game.event_log.iter().any(|event| match event {
            crate::combat::CombatEvent::EscapeResolved { actor, hold, intention, .. } => actor.0 == 0
                || actor.0 >= game.next_actor_id || hold.0 == 0 || hold.0 >= game.next_hold_id
                || intention.is_some_and(|id| !game.intention_issued(id)),
            crate::combat::CombatEvent::Resolved { outcome, strike } => !outcome.valid()
                || (outcome.target_rider_participated && !strike.is_some_and(|ordinal| usize::from(ordinal) < crate::limits::STRIKES))
                || outcome.effects.iter().any(|effect| {
                matches!(effect, crate::abilities::ResolvedEffect::ObservePublicPlan { observation }
                    if observation.observed_at > game.tick || !observation.valid(&game.content.rules, &game.world))
            }),
            _ => false,
        }) {
            return Err(PersistError::RunState);
        }
        if !game.event_receipts_valid() {
            return Err(PersistError::RunState);
        }
        if !game.combat_valid() {
            return Err(PersistError::RunState);
        }
        if (game.actors.is_empty() && !game.has_detached_actors())
            || game.next_actor_id == 0
            || game.next_item_id == 0
            || game.next_door_id == 0
            || game.next_hold_id == 0
            || game
                .next_actor()
                .is_some_and(|id| game.actors[&id].ready_at != game.tick)
        {
            return Err(PersistError::Schedule);
        }
        let mut occupied = BTreeSet::new();
        for (id, actor) in game.actors.iter() {
            if actor.active_since > game.actor_clock(*id) {
                return Err(PersistError::ActorClock { actor: *id });
            }
            if !game.actor_state_valid(*id, actor) || !game.body_has_room(actor, &mut occupied) {
                return Err(PersistError::ActorState { actor: *id });
            }
        }
        for (id, item) in game.items.iter() {
            if !game.item_state_valid(*id, item) {
                return Err(PersistError::ItemState { item: *id });
            }
        }
        if game
            .navigation
            .keys()
            .any(|id| !game.actors.contains_key(id))
        {
            return Err(PersistError::Navigation);
        }
        if game
            .explored_maps
            .keys()
            .any(|id| !game.actors.contains_key(id))
        {
            return Err(PersistError::ExploredMap);
        }
        if !game.world.checkpoint_valid(game.next_door_id) {
            return Err(PersistError::World);
        }
        if !game.loaded_holds_valid() {
            return Err(PersistError::RunState);
        }
        if !game.recovery_contexts_valid() {
            return Err(PersistError::Schedule);
        }
        game.remember_recovery_contexts();
        for (actor, map) in &game.explored_maps {
            game.map_validation.accept_loaded(*actor, map);
        }
        Ok(game)
    }

    /// Checks restoring gives a loaded actor, apart from its body's room;
    /// attaching a stored region gives its actors the same.
    pub(crate) fn actor_state_valid(&self, id: ActorId, actor: &Actor) -> bool {
        id.0 != 0
            && actor
                .guise
                .valid_for(&actor.perception(), actor.pending.as_ref(), actor.alive())
            && self.hold_actor_shape_valid(id, actor)
            && actor.active_since <= self.actor_clock(id)
            && actor.motion.active_fall.is_none_or(|fall| {
                actor.alive()
                    && fall.owner.0 > 0
                    && fall.owner.0 < self.next_actor_id
                    && fall.episode < self.physics.next_fall_episode
                    && fall.started_at <= self.tick
            })
            && actor.combat.as_ref().is_none_or(|combat| {
                combat
                    .creature()
                    .condition_horizon()
                    .is_none_or(|horizon| actor.active_since.checked_add(horizon).is_some())
                    && combat.creature().recovery_context_valid_at(
                        self.actor_frozen(id).then(|| self.actor_clock(id)),
                        actor.active_since,
                    )
            })
            && actor.pending.as_ref().is_none_or(|p| {
                actor.alive()
                    && p.plan.actor() == id
                    && p.plan.valid(&self.content.rules)
                    && self.plan_source_valid(&p.plan)
                    && p.progress
                        .hold_binding()
                        .is_none_or(|binding| binding.id.0 < self.next_hold_id)
                    && p.schedule_valid(self.actor_clock(id), actor.ready_at)
                    && (!p.active
                        || actor.combat.as_ref().is_none_or(|combat| {
                            !combat
                                .creature()
                                .condition_pauses_preparation(&p.plan.work().action())
                        }))
                    && p.plan.target().0 < self.next_actor_id
                    && (!(p.plan.ability_id().is_some()
                        || matches!(
                            p.plan,
                            crate::abilities::Plan::Escape(_) | crate::abilities::Plan::Respec(_)
                        ))
                        || actor.combat.as_ref().map(|c| c.creature()).is_some())
            })
            && actor.combat.as_ref().is_none_or(|c| c.valid())
            && actor
                .combat
                .as_ref()
                .map(|combat| combat.creature())
                .is_none_or(|creature| {
                    creature.derived().anatomy == *actor.anatomy
                        && match actor.pending.as_ref() {
                            Some(preparation) => {
                                let plan = &preparation.plan;
                                let linked = plan.actor() == id
                                    && plan.valid(creature.build().ruleset())
                                    && match plan {
                                        crate::abilities::Plan::Respec(plan) => {
                                            plan.matches_build(creature)
                                        }
                                        _ => true,
                                    }
                                    && plan.target().0 > 0
                                    && plan.target().0 < self.next_actor_id;
                                let owned = preparation
                                    .intention
                                    .is_none_or(|latest| self.intention_issued(latest))
                                    && preparation.origin.is_none_or(|origin| {
                                        self.intention_issued(origin)
                                            && preparation
                                                .intention
                                                .is_some_and(|latest| origin <= latest)
                                    });
                                let funded = match plan.cost().filter(|_| !preparation.has_prefix())
                                {
                                    Some(charge) => preparation.hold.is_some_and(|owner| {
                                        self.intention_issued(owner)
                                            && preparation.origin == Some(owner)
                                            && preparation
                                                .intention
                                                .is_some_and(|latest| owner <= latest)
                                            && creature.costs().reservations().len() == 1
                                            && creature.costs().reservation(owner) == Some(charge)
                                    }),
                                    None => {
                                        creature.costs().reservations().is_empty()
                                            && preparation.hold.is_none()
                                    }
                                };
                                linked && owned && funded
                            }
                            None => creature.costs().reservations().is_empty(),
                        }
                        && creature.conditions().instances().all(|entry| {
                            self.actors.contains_key(&entry.source)
                                || self.detached_actor(entry.source)
                        })
                })
            && self.magic_knowledge_valid(id)
            && id.0 < self.next_actor_id
            && actor.recognized_abilities.len() <= crate::limits::CATALOG_ENTRIES
            && actor
                .recognized_abilities
                .iter()
                .all(|ability| self.content.rules.ability(ability).is_some())
            && actor.combat.as_ref().is_none_or(|combat| {
                combat
                    .creature()
                    .derived()
                    .abilities
                    .iter()
                    .all(|ability| actor.recognized_abilities.contains(ability))
            })
            && actor.public_plan.as_ref().is_none_or(|record| {
                record.observed_at <= self.tick
                    && record.valid(&self.content.rules, &self.world)
                    && match &record.state {
                        crate::public_plan::PublicPlanState::Preparing {
                            ability: Some(ability),
                            ..
                        } => actor.recognized_abilities.contains(&ability.id),
                        _ => true,
                    }
            })
            && actor.orientation < 24
            && (!actor.alive()
                || actor.ready_at >= self.actor_clock(id)
                || self
                    .condition_restricts(id, crate::rules::ConditionRestriction::VoluntaryActions))
            && self.world.contains(actor.location)
            && actor.visited.iter().all(|id| self.world.knows_region(*id))
            && actor.body.valid()
            && actor.motion.valid()
            && actor
                .motion
                .acceleration_remainder
                .iter()
                .all(|v| v.unsigned_abs() < actor.body.cells.len() as u64)
    }

    /// Whether a living actor's body fits walkable cells that no body in
    /// `occupied` holds, adding its cells. Only call this for an actor whose
    /// state is valid.
    pub(crate) fn body_has_room(&self, actor: &Actor, occupied: &mut BTreeSet<Location>) -> bool {
        !actor.alive()
            || self
                .body_cells(actor.location, actor.orientation, &actor.body)
                .is_some_and(|cells| {
                    cells
                        .iter()
                        .all(|(at, _)| self.world.walkable(*at) && occupied.insert(*at))
                })
    }

    /// Checks restoring gives a loaded item; attaching a stored region gives
    /// its items the same.
    pub(crate) fn item_state_valid(&self, id: ItemId, item: &Item) -> bool {
        if !item.condition_valid()
            || item.charges.is_some() != item.spec.device.is_some()
            || item
                .spec
                .device
                .as_ref()
                .is_some_and(|d| item.charges.is_none_or(|c| c > d.initial_charges.maximum))
        {
            return false;
        }
        id.0 != 0
            && id.0 < self.next_item_id
            && item.quantity != 0
            && item.holds_valid()
            && (item.spec.stackable || item.quantity == 1)
            && item.spec.valid()
            && item.corpse.as_ref().is_none_or(|corpse| {
                item.spec.class == crate::ItemClass::Corpse
                    && !item.spec.stackable
                    && corpse.actor.0 != 0
                    && corpse.actor.0 < self.next_actor_id
            })
            && item.spec.validate_rules(self.content.rules()).is_ok()
            && item.motion.valid()
            && item.motion.active_fall.is_none()
            && item.motion.acceleration_remainder == [0; 3]
            && item.orientation < 24
            && match item.location {
                ItemLocation::Ground(location) => self.world.contains(location),
                ItemLocation::Carried(actor) => self.actors.contains_key(&actor),
            }
    }

    /// Every actor a checkpoint's revisions cover: loaded, detached or not
    /// built yet.
    pub fn checkpoint_actor_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.known_actor_ids().into_iter().map(|id| id.0)
    }

    /// Whether an actor is in a loaded region.
    pub fn has_actor(&self, id: ActorId) -> bool {
        self.actors.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use tor_world::{Location, Position, RegionId};

    #[test]
    fn restored_actor_clocks_cannot_come_from_the_future() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        game.spawn_actor(
            Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            },
            NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        let mut value = serde_json::to_value(&shared).unwrap();
        value["actors"][snapshot.actors]["1"]["active_since"] = serde_json::json!(game.tick() + 1);
        let forged: SharedState = serde_json::from_value(value).unwrap();
        assert_eq!(
            Game::restore_checkpoint(snapshot, &forged, game.content().clone()),
            Err(PersistError::ActorClock { actor: ActorId(1) })
        );
    }

    #[test]
    fn shared_navigation_validation_cannot_cross_world_bounds_or_actor_membership() {
        let world = |width| {
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_region(tor_world::Region {
                    id: RegionId(1),
                    name: "validation".into(),
                    bounds: tor_world::Extent::new(width, 2, 1).unwrap(),
                })
                .unwrap();
            world
        };
        let mut game = Game::new(world(8), 42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 0, y: 0, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.refresh_navigation();
        assert!(game.known_cells(actor).any(|at| at.position.x == 6));
        let mut shared = SharedState::default();
        let valid = game.checkpoint(&mut shared);
        let mut other_world = game.checkpoint(&mut shared);
        shared.worlds.push(world(2));
        other_world.world = shared.worlds.len() - 1;
        let mut other_actor = game.checkpoint(&mut shared);
        let index = other_actor.navigation.remove(&actor).unwrap();
        other_actor.navigation.insert(ActorId(999), index);
        let mut context = RestoreContext::new(&shared, game.content().clone());
        assert!(context.restore(valid).is_ok());
        assert!(context.restore(other_world).is_err());
        assert!(context.restore(other_actor).is_err());
    }

    #[test]
    fn repeated_boundaries_do_not_repeat_unchanged_actor_payloads() {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(tor_world::Region {
                id: RegionId(1),
                name: "retention".into(),
                bounds: tor_world::Extent::new(66, 3, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, crate::test_creatures::content());
        for x in 1..=64 {
            game.spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        }
        let mut one = SharedState::default();
        let snapshot = game.checkpoint(&mut one);
        let single_bytes = serde_json::to_vec(&(vec![snapshot], one)).unwrap().len();
        let mut shared = SharedState::default();
        let snapshots: Vec<_> = (0..256)
            .map(|_| {
                let mut independently_owned = game.clone();
                independently_owned.actors =
                    crate::actor_store::ActorStore::from_entries(game.actors.raw_entries().clone());
                independently_owned.checkpoint(&mut shared)
            })
            .collect();
        assert_eq!(shared.actors.len(), 1);
        let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
        assert!(
            bytes.len() < single_bytes * 8,
            "unchanged actors repeated at retained boundaries: {} versus {}",
            bytes.len(),
            single_bytes
        );
        let (snapshots, shared): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&shared, game.content().clone());
        let mut restored: Vec<_> = snapshots
            .into_iter()
            .map(|snapshot| context.restore(snapshot).unwrap())
            .collect();
        for boundary in &restored {
            assert_eq!(boundary, &game);
        }
        assert!(std::ptr::eq(
            &restored[0].actors[&ActorId(1)],
            &restored[1].actors[&ActorId(1)]
        ));
        restored[0].actors.get_mut(&ActorId(1)).unwrap().body.mass += 1;
        assert_eq!(restored[1], game);
    }

    #[test]
    fn retained_queues_share_complete_intentions_at_the_supported_capacity() {
        for count in [64, crate::intention::MAX_QUEUED_INTENTIONS] {
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_region(tor_world::Region {
                    id: RegionId(1),
                    name: "queue retention".into(),
                    bounds: tor_world::Extent::new(count as i32 + 2, 3, 1).unwrap(),
                })
                .unwrap();
            let mut queued = Game::new(world, 42, crate::test_creatures::content());
            for x in 1..=count as i32 {
                let actor = queued
                    .spawn_actor(
                        Location {
                            region: RegionId(1),
                            position: Position { x, y: 1, z: 0 },
                        },
                        NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                queued
                    .admit_intention(
                        actor,
                        crate::Action::Wait,
                        crate::intention::IntentionOrigin::Human,
                    )
                    .unwrap();
            }
            let id = queued.pending_intention(ActorId(1)).unwrap().id;
            let mut suspended = queued.clone();
            suspended.suspend_intention(ActorId(1), id).unwrap();
            let mut single = SharedState::default();
            let snapshot = queued.checkpoint(&mut single);
            let single_bytes = serde_json::to_vec(&(vec![snapshot], single)).unwrap().len();
            let mut shared = SharedState::default();
            let snapshots: Vec<_> = (0..256)
                .map(|index| {
                    if index % 2 == 0 {
                        queued.checkpoint(&mut shared)
                    } else {
                        suspended.checkpoint(&mut shared)
                    }
                })
                .collect();
            let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
            assert!(
                bytes.len() < single_bytes * 8,
                "retained queues repeat complete payloads at {count} actors: {} versus {single_bytes}",
                bytes.len()
            );
            assert!(bytes.len() < 16 * 1024 * 1024);
            let (snapshots, shared): (Vec<Snapshot>, SharedState) =
                serde_json::from_slice(&bytes).unwrap();
            let mut context = RestoreContext::new(&shared, queued.content().clone());
            let mut restored: Vec<_> = snapshots
                .into_iter()
                .map(|s| context.restore(s).unwrap())
                .collect();
            for (index, game) in restored.iter().enumerate() {
                assert_eq!(game, if index % 2 == 0 { &queued } else { &suspended });
            }
            restored[0].cancel_intention(ActorId(1), id).unwrap();
            assert_eq!(restored[2], queued);
            let next = restored[0]
                .admit_intention(
                    ActorId(1),
                    crate::Action::Wait,
                    crate::intention::IntentionOrigin::Human,
                )
                .unwrap();
            assert_eq!(next.0, count as u64 + 1);
        }
    }

    #[test]
    fn decoded_restore_shares_equal_bodies_without_changing_values() {
        for count in [16, 256, 4096] {
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_region(tor_world::Region {
                    id: RegionId(1),
                    name: "restore".into(),
                    bounds: tor_world::Extent::new(count * 3 + 3, 3, 1).unwrap(),
                })
                .unwrap();
            let mut game = Game::new(world, 42, crate::test_creatures::content());
            for x in 0..count {
                let id = game
                    .spawn_actor(
                        Location {
                            region: RegionId(1),
                            position: Position {
                                x: x * 3,
                                y: 1,
                                z: 0,
                            },
                        },
                        NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                if x % 2 == 1 {
                    game.set_body(
                        id,
                        crate::BodySpec {
                            cells: vec![[0, 0, 0], [1, 0, 0]],
                            eye: [1, 0, 0],
                            mass: 91,
                        },
                    )
                    .unwrap();
                }
            }
            let mut shared = SharedState::default();
            let snapshot = game.checkpoint(&mut shared);
            let bytes = serde_json::to_vec(&(snapshot, shared)).unwrap();
            let (snapshot, shared): (Snapshot, SharedState) =
                serde_json::from_slice(&bytes).unwrap();
            let restored =
                Game::restore_checkpoint(snapshot, &shared, game.content().clone()).unwrap();
            assert_eq!(restored, game);
            let first = &restored.actors[&ActorId(1)].body;
            let second = &restored.actors[&ActorId(2)].body;
            assert!(!first.shares_storage(second));
            for (id, actor) in restored.actors.iter() {
                let expected = if id.0 % 2 == 1 { first } else { second };
                assert!(
                    actor.body.shares_storage(expected),
                    "actor {id:?}, count {count}"
                );
            }
            let mut encoded_shared = SharedState::default();
            let encoded_snapshot = restored.checkpoint(&mut encoded_shared);
            assert_eq!(
                serde_json::to_vec(&(encoded_snapshot, encoded_shared)).unwrap(),
                bytes
            );
        }
    }

    #[test]
    fn ground_item_references_are_checked_at_runtime_checkpoint_and_detached_restore() {
        use crate::content_ids::{AbilityId, ConditionId, DescriptorId};
        use crate::rules::{Effect, Modifier, ModifierOp, RulesetError, Scope};
        let mut modifier = crate::ItemSpec::ordinary("armor".into());
        modifier.class = crate::ItemClass::Armor;
        modifier.equipment = Some(crate::EquipmentSpec {
            weapon_profile: None,
            armor: None,
            slot: crate::EquipmentSlot::new("body_armor").unwrap(),
            attack: None,
            modifiers: vec![Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: AbilityId::new("missing_ability").unwrap(),
                },
            }],
        });
        let mut effect = crate::ItemSpec::ordinary("potion".into());
        effect.class = crate::ItemClass::Potion;
        effect.consumable = Some(crate::ConsumableSpec {
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![Effect::ApplyCondition {
                condition: ConditionId::new("missing_condition").unwrap(),
                duration: 100,
            }],
        });
        let mut attack = crate::ItemSpec::ordinary("weapon".into());
        attack.class = crate::ItemClass::Weapon;
        attack.equipment = Some(crate::EquipmentSpec {
            weapon_profile: None,
            armor: None,
            slot: crate::EquipmentSlot::new("weapon").unwrap(),
            modifiers: vec![],
            attack: Some(
                crate::attacks::MeleeAttack::fixed(
                    crate::attributes::Skill::HeavyWeaponry,
                    0,
                    100,
                    40,
                    crate::combat::DamageType::Impact,
                    Some(DescriptorId::new("missing_descriptor").unwrap()),
                    2,
                )
                .unwrap(),
            ),
        });
        let mut slot = crate::ItemSpec::ordinary("unknown slot armor".into());
        slot.class = crate::ItemClass::Armor;
        slot.equipment = Some(crate::EquipmentSpec {
            weapon_profile: None,
            armor: None,
            slot: crate::EquipmentSlot::new("missing_slot").unwrap(),
            attack: None,
            modifiers: vec![],
        });
        for (spec, category, missing) in [
            (modifier, "ability", "missing_ability"),
            (effect, "condition", "missing_condition"),
            (attack, "descriptor", "missing_descriptor"),
            (slot, "equipment_slot", "missing_slot"),
        ] {
            let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
            let at = Location {
                region: RegionId(1),
                position: Position { x: 1, y: 1, z: 0 },
            };
            game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
            let before = game.clone();
            assert_eq!(
                game.place_item_stack(100, at, None, 1, spec.clone()),
                Err(crate::GameError::Rule(
                    crate::abilities::RuleError::InvalidDefinition
                ))
            );
            assert_eq!(game, before, "invalid item insertion is atomic");
            let item = game
                .place_item_stack(100, at, None, 1, crate::ItemSpec::ordinary("valid".into()))
                .unwrap();
            let content = game.content().clone();
            let mut shared = SharedState::default();
            let snapshot = game.checkpoint(&mut shared);
            shared.items[snapshot.items].get_mut(&item).unwrap().spec = Shared::new(spec.clone());
            let shared = serde_json::from_slice(&serde_json::to_vec(&shared).unwrap()).unwrap();
            let expected = PersistError::ItemDefinition {
                item,
                error: RulesetError::UnknownReference {
                    category,
                    id: missing.into(),
                },
            };
            assert_eq!(
                Game::restore_checkpoint(snapshot, &shared, content.clone()),
                Err(expected.clone())
            );
            let record = game.into_region_record(RegionId(1)).unwrap();
            let mut value =
                serde_json::to_value(crate::streaming::RegionRecordData::capture(&record)).unwrap();
            value["items"][item.0.to_string()]["spec"] = serde_json::to_value(spec).unwrap();
            let saved: crate::streaming::RegionRecordData = serde_json::from_value(value).unwrap();
            assert_eq!(saved.restore(content), Err(expected));
        }
    }

    #[test]
    fn restoring_unknown_catalog_references_preserves_the_typed_cause() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        let empty = Content::new(
            game.content().rules.clone(),
            crate::creatures::ScenarioCatalog::default(),
        )
        .unwrap();
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert_eq!(
            Game::restore_checkpoint(snapshot, &shared, empty),
            Err(PersistError::CreatureRecord {
                actor,
                error: crate::creatures::CreatureStateError::Build(
                    crate::creatures::BuildError::UnknownSpecies
                ),
            })
        );
    }

    #[test]
    fn decoded_catalog_and_item_definitions_share_across_distinct_boundaries() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        crate::test_creatures::configure(
            &mut game,
            actor,
            "neutral",
            crate::test_creatures::species(),
        );
        assert!(
            game.creature(actor).is_some(),
            "checkpoint subjects own their builds"
        );
        let mut spec = crate::ItemSpec::ordinary("token".into());
        spec.stackable = true;
        let item = game.place_item_stack(100, at, None, 2, spec).unwrap();
        let mut shared = SharedState::default();
        let before = game.checkpoint(&mut shared);
        game.items.edit(item, |entry| entry.quantity = 1).unwrap();
        crate::test_creatures::injure(&mut game, actor, 1).unwrap();
        let after = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(vec![before, after], shared)).unwrap();
        let (snapshots, shared): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&shared, game.content().clone());
        let mut restored: Vec<_> = snapshots
            .into_iter()
            .map(|s| context.restore(s).unwrap())
            .collect();
        assert!(!restored[0].items.shares_storage(&restored[1].items));
        assert!(restored[0].actors[&actor]
            .anatomy
            .shares_storage(&restored[1].actors[&actor].anatomy));
        assert!(restored[0].items[&item]
            .spec
            .shares_storage(&restored[1].items[&item].spec));
        assert!(Arc::ptr_eq(
            restored[0].creature(actor).unwrap().build().content(),
            restored[1].creature(actor).unwrap().build().content()
        ));
        drop(context);
        let mut encoded_shared = SharedState::default();
        let encoded: Vec<_> = restored
            .iter()
            .map(|g| g.checkpoint(&mut encoded_shared))
            .collect();
        assert_eq!(
            serde_json::to_vec(&(encoded, encoded_shared)).unwrap(),
            bytes
        );
        restored[0]
            .items
            .edit(item, |entry| entry.spec.name = "changed".into())
            .unwrap();
        crate::test_creatures::injure(&mut restored[0], actor, 2).unwrap();
        assert_eq!(restored[0].creature(actor).unwrap().health().injury(), 2);
        assert_eq!(restored[1].creature(actor).unwrap().health().injury(), 1);
        assert_eq!(restored[1].items[&item].spec.name, "token");
    }

    #[test]
    fn common_restore_context_shares_decoded_pools_and_preserves_copy_on_write() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let at = Location {
            region: RegionId(1),
            position: Position { x: 1, y: 1, z: 0 },
        };
        let actor = game.spawn_actor(at, NonZeroU64::new(100).unwrap()).unwrap();
        let item = game.place_item(at, "token".into()).unwrap();
        game.refresh_navigation();
        let mut shared = SharedState::default();
        let before = game.checkpoint(&mut shared);
        let original = game.clone();
        game.act(actor, crate::Action::Wait).unwrap();
        let after = game.checkpoint(&mut shared);
        let bytes = serde_json::to_vec(&(vec![before, after], shared)).unwrap();
        let (snapshots, shared): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&shared, game.content().clone());
        let mut restored: Vec<_> = snapshots
            .into_iter()
            .map(|s| context.restore(s).unwrap())
            .collect();
        assert_eq!(restored[0], original);
        assert_eq!(restored[1], game);
        assert!(restored[0].world.shares_storage(&restored[1].world));
        assert!(restored[0].navigation[&actor].shares_storage(&restored[1].navigation[&actor]));
        assert!(restored[0].items.shares_storage(&restored[1].items));
        assert!(restored[0].actors[&actor]
            .body
            .shares_storage(&restored[1].actors[&actor].body));
        drop(context);
        let mut encoded_shared = SharedState::default();
        let encoded: Vec<_> = restored
            .iter()
            .map(|g| g.checkpoint(&mut encoded_shared))
            .collect();
        assert_eq!(
            serde_json::to_vec(&(encoded, encoded_shared)).unwrap(),
            bytes
        );
        restored[0]
            .set_body(
                actor,
                crate::BodySpec {
                    mass: 99,
                    ..Default::default()
                },
            )
            .unwrap();
        restored[0]
            .items
            .edit(item, |item| item.spec.name = "edited".into())
            .unwrap();
        restored[0]
            .navigation
            .get_mut(&actor)
            .unwrap()
            .cells
            .remove(&at);
        restored[0]
            .world
            .add_region(tor_world::Region {
                id: RegionId(9),
                name: "new".into(),
                bounds: tor_world::Extent::new(3, 3, 1).unwrap(),
            })
            .unwrap();
        assert_eq!(restored[1], game);
        assert_eq!(restored[1].items[&item].spec.name, "token");
        assert_eq!(restored[1].actors[&actor].body.mass, 80);
    }

    #[test]
    fn body_pool_preserves_cell_order_eye_and_mass_and_rejects_invalid_snapshots() {
        let base = crate::BodySpec {
            cells: vec![[0, 0, 0], [1, 0, 0]],
            eye: [0, 0, 0],
            mass: 80,
        };
        let mut reordered = base.clone();
        reordered.cells.reverse();
        let mut eye = base.clone();
        eye.eye = [1, 0, 0];
        let mut mass = base.clone();
        mass.mass = 81;
        let pool = BTreeSet::from([
            Shared::new(base),
            Shared::new(reordered),
            Shared::new(eye),
            Shared::new(mass),
        ]);
        assert_eq!(pool.len(), 4);
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        let mut shared = SharedState::default();
        let broken = game.checkpoint(&mut shared);
        shared.actors[broken.actors]
            .get_mut(&actor)
            .unwrap()
            .body
            .eye = [1, 0, 0];
        let valid = game.checkpoint(&mut shared);
        let mut context = RestoreContext::new(&shared, game.content().clone());
        assert!(context.restore(broken).is_err());
        assert_eq!(context.restore(valid), Ok(game));
    }

    #[test]
    fn snapshots_preserve_complete_game_and_reject_invalid_shared_references() {
        let mut game = Game::two_room_in_stone(42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(
                Location {
                    region: RegionId(1),
                    position: Position { x: 1, y: 1, z: 0 },
                },
                NonZeroU64::new(100).unwrap(),
            )
            .unwrap();
        game.refresh_navigation();
        let mut shared = SharedState::default();
        let before = game.checkpoint(&mut shared);
        assert_eq!(
            Game::restore_checkpoint(before, &shared, game.content().clone()),
            Ok(game.clone())
        );
        game.act(actor, crate::Action::Wait).unwrap();
        let after = game.checkpoint(&mut shared);
        assert_eq!(
            Game::restore_checkpoint(after, &shared, game.content().clone()),
            Ok(game.clone())
        );
        let mut broken = game.checkpoint(&mut shared);
        broken.navigation.insert(actor, usize::MAX);
        assert!(Game::restore_checkpoint(broken, &shared, game.content().clone()).is_err());
        let mut broken = game.checkpoint(&mut shared);
        broken.world = usize::MAX;
        assert!(Game::restore_checkpoint(broken, &shared, game.content().clone()).is_err());
        let mut broken = game.checkpoint(&mut shared);
        broken.actors = usize::MAX;
        assert!(Game::restore_checkpoint(broken, &shared, game.content().clone()).is_err());
    }
}

/// Format-6 navigation pools retain sharing when decoded, including between
/// independently equal region maps. No historical-format fallback is accepted.
mod navigation_regions {
    use super::*;
    use crate::navigation_map::RegionMap;
    use crate::travel::KnownEdge;
    use serde::{Deserializer, Serializer};
    use tor_world::{Direction, Location, RegionId};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Instance {
        places: Vec<usize>,
        cells: Vec<usize>,
        edges: Vec<usize>,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Regions {
        places: Vec<RegionMap<Location, crate::PlaceName>>,
        cells: Vec<CellRegion>,
        edges: Vec<EdgeRegion>,
        instances: Vec<Instance>,
    }

    type Edges = RegionMap<(Location, Direction), KnownEdge>;
    type Cells = RegionMap<Location, bool>;
    /// Ordered x/y columns with explicit [z, value] entries. No implicit runs
    /// or unbounded expansion: each decoded position has an encoded entry.
    type Columns = Vec<(i32, i32, Vec<[i32; 2]>)>;

    fn columns(entries: impl IntoIterator<Item = (tor_world::Position, i32)>) -> Columns {
        let mut columns = Columns::new();
        for (p, value) in entries {
            match columns.last_mut() {
                Some((x, y, values)) if (*x, *y) == (p.x, p.y) => values.push([p.z, value]),
                _ => columns.push((p.x, p.y, vec![[p.z, value]])),
            }
        }
        columns
    }

    fn expand_columns(columns: Columns) -> Option<Vec<(tor_world::Position, i32)>> {
        let mut entries = Vec::new();
        let mut previous = None;
        for (x, y, values) in columns {
            if values.is_empty() || previous.is_some_and(|p| p >= (x, y)) {
                return None;
            }
            previous = Some((x, y));
            let mut last_z = None;
            for [z, value] in values {
                if last_z.is_some_and(|last| last >= z) {
                    return None;
                }
                last_z = Some(z);
                entries.push((tor_world::Position { x, y, z }, value));
            }
        }
        Some(entries)
    }

    /// Remembered opacity is independent of current terrain. Store the region
    /// once per shared pool entry and retain each cell's exact stale value.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct CellRegion {
        region: RegionId,
        cells: Columns,
    }

    impl CellRegion {
        fn encode(map: &Cells) -> Self {
            Self {
                region: map
                    .iter()
                    .next()
                    .expect("pooled regions are nonempty")
                    .0
                    .region,
                cells: columns(
                    map.iter()
                        .map(|(location, opaque)| (location.position, i32::from(*opaque))),
                ),
            }
        }

        fn decode(self) -> Option<Cells> {
            if self.cells.is_empty() {
                return None;
            }
            let mut map = Cells::default();
            for (position, opaque) in expand_columns(self.cells)? {
                if !(0..=1).contains(&opaque) {
                    return None;
                }
                map.insert(
                    Location {
                        region: self.region,
                        position,
                    },
                    opaque == 1,
                );
            }
            Some(map)
        }
    }

    /// Link directions, in mask bit order.
    const DIRECTIONS: [Direction; 6] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ];

    /// One pooled region of known links. Most links are plain: the ordinary
    /// neighbour in the same region with no rotation, fully determined by the
    /// start cell and direction. Those use coordinate columns and a mask with bit
    /// `i` for `DIRECTIONS[i]`; only the rest are stored in full. Decoding
    /// accepts exactly one encoding per map.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct EdgeRegion {
        region: RegionId,
        plain: Columns,
        other: Vec<((Location, Direction), KnownEdge)>,
    }

    fn plain_target(from: Location, direction: Direction) -> Option<Location> {
        let (dx, dy, dz) = direction.delta();
        let p = from.position;
        Some(Location {
            position: tor_world::Position {
                x: p.x.checked_add(dx)?,
                y: p.y.checked_add(dy)?,
                z: p.z.checked_add(dz)?,
            },
            ..from
        })
    }

    fn is_plain(from: Location, direction: Direction, to: Location, turns: u8) -> bool {
        turns == 0 && plain_target(from, direction) == Some(to)
    }

    impl EdgeRegion {
        /// `map` is a pooled single-region map.
        fn encode(map: &Edges) -> Self {
            let mut plain = BTreeMap::<tor_world::Position, i32>::new();
            let mut other = Vec::new();
            let mut region = None;
            for (&(from, direction), &edge) in map.iter() {
                let (to, turns) = (edge.destination, edge.rotation);
                region = Some(from.region);
                match DIRECTIONS.iter().position(|d| *d == direction) {
                    Some(bit) if is_plain(from, direction, to, turns) => {
                        *plain.entry(from.position).or_default() |= (1 << bit)
                            | if edge.requires_flight {
                                1 << (bit + 6)
                            } else {
                                0
                            };
                    }
                    _ => other.push(((from, direction), edge)),
                }
            }
            Self {
                region: region.expect("pooled regions are nonempty"),
                plain: columns(plain),
                other,
            }
        }

        fn decode(self) -> Option<Edges> {
            let mut map = Edges::default();
            let mut count = 0usize;
            for (position, mask) in expand_columns(self.plain)? {
                if !(1..4096).contains(&mask)
                    || ((mask >> 6) & !(mask & 63)) != 0
                    || ((mask >> 6) & 15) != 0
                {
                    return None;
                }
                let from = Location {
                    region: self.region,
                    position,
                };
                for (bit, direction) in DIRECTIONS.into_iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        map.insert(
                            (from, direction),
                            KnownEdge {
                                destination: plain_target(from, direction)?,
                                rotation: 0,
                                requires_flight: mask & (1 << (bit + 6)) != 0,
                            },
                        );
                        count += 1;
                    }
                }
            }
            let mut previous = None;
            for ((from, direction), edge) in self.other {
                let (to, turns) = (edge.destination, edge.rotation);
                if from.region != self.region
                    || turns >= 24
                    || !DIRECTIONS.contains(&direction)
                    || (edge.requires_flight
                        && !matches!(direction, Direction::Up | Direction::Down))
                    || previous.is_some_and(|key| key >= (from, direction))
                    || (DIRECTIONS.contains(&direction) && is_plain(from, direction, to, turns))
                    || map.contains_key(&(from, direction))
                {
                    return None;
                }
                previous = Some((from, direction));
                map.insert((from, direction), edge);
                count += 1;
            }
            (map.iter().count() == count).then_some(map)
        }
    }

    pub(super) fn serialize<S: Serializer>(
        navigation: &[Navigation],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut saved = Regions {
            places: Vec::new(),
            cells: Vec::new(),
            edges: Vec::new(),
            instances: Vec::new(),
        };
        let mut places = BTreeMap::new();
        let mut cells = BTreeMap::new();
        let mut edges = BTreeMap::new();
        let mut edge_pool = Vec::new();
        let mut cell_pool = Vec::new();
        for map in navigation {
            saved.instances.push(Instance {
                places: map
                    .places
                    .checkpoint_regions(&mut saved.places, &mut places),
                cells: map.cells.checkpoint_regions(&mut cell_pool, &mut cells),
                edges: map.edges.checkpoint_regions(&mut edge_pool, &mut edges),
            });
        }
        saved.edges = edge_pool.iter().map(EdgeRegion::encode).collect();
        saved.cells = cell_pool.iter().map(CellRegion::encode).collect();
        saved.serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Navigation>, D::Error> {
        let saved = Regions::deserialize(deserializer)?;
        let Some(cell_pool) = saved
            .cells
            .into_iter()
            .map(CellRegion::decode)
            .collect::<Option<Vec<_>>>()
        else {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation cells",
            ));
        };
        let Some(edge_pool) = saved
            .edges
            .into_iter()
            .map(EdgeRegion::decode)
            .collect::<Option<Vec<_>>>()
        else {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation links",
            ));
        };
        if saved.places.iter().any(|map| !map.is_checkpoint_region())
            || cell_pool.iter().any(|map| !map.is_checkpoint_region())
            || edge_pool.iter().any(|map| !map.is_checkpoint_region())
        {
            return Err(serde::de::Error::custom(
                "invalid checkpoint navigation region",
            ));
        }
        saved
            .instances
            .into_iter()
            .map(|instance| {
                let places = RegionMap::restore_regions(&instance.places, &saved.places);
                let cells = RegionMap::restore_regions(&instance.cells, &cell_pool);
                let edges = RegionMap::restore_regions(&instance.edges, &edge_pool);
                match (cells, edges, places) {
                    (Some(cells), Some(edges), Some(places)) => Ok(Navigation {
                        cells,
                        edges,
                        places,
                    }),
                    _ => Err(serde::de::Error::custom(
                        "invalid checkpoint navigation reference",
                    )),
                }
            })
            .collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use tor_world::Position;

        fn at(region: u64, x: i32, y: i32, z: i32) -> Location {
            Location {
                region: RegionId(region),
                position: Position { x, y, z },
            }
        }

        #[test]
        fn remembered_opacity_round_trips_in_compact_cells() {
            let mut map = Cells::default();
            map.insert(at(7, -1, 2, 3), false);
            map.insert(at(7, 4, 5, 6), true);
            let encoded = CellRegion::encode(&map);
            assert_eq!(encoded.region, RegionId(7));
            assert_eq!(
                encoded.cells,
                vec![(-1, 2, vec![[3, 0]]), (4, 5, vec![[6, 1]])]
            );
            assert_eq!(encoded.decode(), Some(map));
        }

        #[test]
        fn coordinate_columns_compact_dense_cells_without_implicit_expansion() {
            let entries: Vec<_> = (-4..=4)
                .flat_map(|x| {
                    (-2..=2).flat_map(move |y| (-1..=8).map(move |z| (Position { x, y, z }, z & 1)))
                })
                .collect();
            let encoded = columns(entries.clone());
            assert_eq!(encoded.len(), 45);
            let flat: Vec<_> = entries
                .iter()
                .map(|(p, value)| [p.x, p.y, p.z, *value])
                .collect();
            assert!(
                serde_json::to_vec(&encoded).unwrap().len()
                    < serde_json::to_vec(&flat).unwrap().len()
            );
            assert_eq!(expand_columns(encoded), Some(entries));
            for malformed in [
                vec![(0, 0, vec![])],
                vec![(0, 0, vec![[0, 0]]), (0, 0, vec![[1, 1]])],
                vec![(0, 1, vec![[0, 0]]), (0, 0, vec![[0, 0]])],
                vec![(0, 0, vec![[1, 0], [0, 0]])],
                vec![(0, 0, vec![[0, 0], [0, 1]])],
            ] {
                assert!(expand_columns(malformed).is_none());
            }
        }

        #[test]
        fn malformed_or_noncanonical_remembered_cells_are_rejected() {
            for rows in [
                vec![],
                vec![[0, 0, 0, -1]],
                vec![[0, 0, 0, 2]],
                vec![[0, 0, 0, 0], [0, 0, 0, 1]],
                vec![[1, 0, 0, 0], [0, 0, 0, 1]],
            ] {
                assert!(CellRegion {
                    region: RegionId(1),
                    cells: columns(
                        rows.into_iter()
                            .map(|[x, y, z, value]| (Position { x, y, z }, value))
                    )
                }
                .decode()
                .is_none());
            }
        }

        #[test]
        fn links_round_trip_with_plain_masks_and_unusual_links_in_full() {
            let mut map = Edges::default();
            map.insert(
                (at(1, 0, 0, 0), Direction::East),
                KnownEdge::ordinary(at(1, 1, 0, 0), 0),
            );
            map.insert(
                (at(1, 0, 0, 0), Direction::South),
                KnownEdge::ordinary(at(1, 0, 1, 0), 0),
            );
            map.insert(
                (at(1, 0, 0, 1), Direction::Down),
                KnownEdge::ordinary(at(1, 0, 0, 0), 0),
            );
            // A portal, a rotated join, and a stair are stored in full.
            map.insert(
                (at(1, 4, 0, 0), Direction::East),
                KnownEdge::ordinary(at(2, 0, 0, 0), 0),
            );
            map.insert(
                (at(1, 0, 3, 0), Direction::West),
                KnownEdge::ordinary(at(1, -1, 3, 0), 1),
            );
            map.insert(
                (at(1, 2, 2, 0), Direction::Up),
                KnownEdge::ordinary(at(3, 2, 2, 0), 0),
            );
            let encoded = EdgeRegion::encode(&map);
            assert_eq!(
                encoded.plain,
                vec![(0, 0, vec![[0, 0b10 | 0b100], [1, 0b10_0000]])]
            );
            assert_eq!(encoded.other.len(), 3);
            assert_eq!(encoded.decode(), Some(map));
        }

        #[test]
        fn flight_masks_round_trip_and_reject_orphan_horizontal_and_unused_bits() {
            let mut map = Edges::default();
            let from = at(1, 2, 2, 0);
            map.insert(
                (from, Direction::Up),
                KnownEdge {
                    destination: at(1, 2, 2, 1),
                    rotation: 0,
                    requires_flight: true,
                },
            );
            let saved = EdgeRegion::encode(&map);
            assert_eq!(saved.plain, vec![(2, 2, vec![[0, (1 << 4) | (1 << 10)]])]);
            let encoded = serde_json::to_vec(&saved).unwrap();
            assert_eq!(
                serde_json::from_slice::<EdgeRegion>(&encoded)
                    .unwrap()
                    .decode(),
                Some(map)
            );
            for mask in [-1, 0, 64, 128, 129, (1 << 10), 4096, 8191] {
                assert!(
                    EdgeRegion {
                        region: RegionId(1),
                        plain: vec![(2, 2, vec![[0, mask]])],
                        other: vec![]
                    }
                    .decode()
                    .is_none(),
                    "{mask}"
                );
            }
            let edge = KnownEdge {
                destination: at(2, 0, 0, 0),
                rotation: 0,
                requires_flight: true,
            };
            assert!(EdgeRegion {
                region: RegionId(1),
                plain: vec![],
                other: vec![((from, Direction::East), edge)]
            }
            .decode()
            .is_none());
        }

        #[test]
        fn non_canonical_or_malformed_links_are_rejected() {
            let plain = (
                (at(1, 0, 0, 0), Direction::East),
                KnownEdge::ordinary(at(1, 1, 0, 0), 0),
            );
            let decode = |plain: Vec<[i32; 4]>, other| {
                EdgeRegion {
                    region: RegionId(1),
                    plain: columns(
                        plain
                            .into_iter()
                            .map(|[x, y, z, value]| (Position { x, y, z }, value)),
                    ),
                    other,
                }
                .decode()
            };
            // A plain link written in full has a second encoding.
            assert_eq!(decode(vec![], vec![plain]), None);
            // Duplicates, empty or out-of-range masks, and unsorted cells.
            assert_eq!(decode(vec![[0, 0, 0, 2]], vec![plain]), None);
            assert_eq!(decode(vec![[0, 0, 0, 0]], vec![]), None);
            assert_eq!(decode(vec![[0, 0, 0, 64]], vec![]), None);
            assert_eq!(decode(vec![[1, 0, 0, 2], [0, 0, 0, 2]], vec![]), None);
            // Links belong to the pooled region.
            let foreign = (
                (at(2, 4, 0, 0), Direction::East),
                KnownEdge::ordinary(at(3, 0, 0, 0), 0),
            );
            assert_eq!(decode(vec![], vec![foreign]), None);
        }
    }
}

/// Region pools retain immutable appearances and placement buckets across actors
/// and rewind boundaries. Only the current, explicit four-table shape is read.
mod explored_map_regions {
    use super::*;
    use crate::explored_map::{ChildKey, MapAppearance, Neighbor, Witness};
    use crate::navigation_map::{RegionKey, RegionMap};
    use serde::{Deserializer, Serializer};
    use std::rc::Rc;
    use tor_world::{Direction, Position, RegionId};

    #[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
    #[serde(deny_unknown_fields)]
    struct Instance {
        cells: Vec<usize>,
        neighbors: Vec<usize>,
        witnesses: Vec<usize>,
        children: Vec<usize>,
    }

    impl<'de> Deserialize<'de> for Instance {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Fields {
                cells: Vec<usize>,
                neighbors: Vec<usize>,
                witnesses: Vec<usize>,
                children: Vec<usize>,
            }
            let fields: Fields = crate::bounded::named_map(deserializer)?;
            Ok(Self {
                cells: fields.cells,
                neighbors: fields.neighbors,
                witnesses: fields.witnesses,
                children: fields.children,
            })
        }
    }

    struct Regions {
        cells: Vec<RegionMap<Location, Shared<MapAppearance>>>,
        neighbors: Vec<RegionMap<(Location, Direction), Neighbor>>,
        witnesses: Vec<RegionMap<Location, Witness>>,
        children: Vec<RegionMap<ChildKey, u8>>,
        instances: Vec<Instance>,
    }

    type Cells = RegionMap<Location, Shared<MapAppearance>>;
    type Neighbors = RegionMap<(Location, Direction), Neighbor>;
    /// Each row is explicit: one cell or at most six ordinary links. There are
    /// no runs or products of dimensions controlled by checkpoint input.
    type Columns<T> = Vec<(i32, i32, Vec<(i32, T)>)>;

    fn columns<T>(entries: impl IntoIterator<Item = (Position, T)>) -> Columns<T> {
        let mut result = Columns::new();
        for (p, value) in entries {
            match result.last_mut() {
                Some((x, y, rows)) if (*x, *y) == (p.x, p.y) => rows.push((p.z, value)),
                _ => result.push((p.x, p.y, vec![(p.z, value)])),
            }
        }
        result
    }

    fn expand_columns<T>(
        columns: Columns<T>,
        mut visit: impl FnMut(Position, T) -> Option<()>,
    ) -> Option<()> {
        let mut previous = None;
        for (x, y, rows) in columns {
            if rows.is_empty() || previous.is_some_and(|p| p >= (x, y)) {
                return None;
            }
            previous = Some((x, y));
            let mut last_z = None;
            for (z, value) in rows {
                if last_z.is_some_and(|before| before >= z) {
                    return None;
                }
                last_z = Some(z);
                visit(Position { x, y, z }, value)?;
            }
        }
        Some(())
    }

    #[derive(Serialize)]
    struct CellRegion {
        region: RegionId,
        cells: Columns<usize>,
    }
    impl<'de> Deserialize<'de> for CellRegion {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Fields {
                region: RegionId,
                cells: Columns<usize>,
            }
            let fields: Fields = crate::bounded::named_map(d)?;
            Ok(Self {
                region: fields.region,
                cells: fields.cells,
            })
        }
    }

    const DIRECTIONS: [Direction; 6] = [
        Direction::North,
        Direction::East,
        Direction::South,
        Direction::West,
        Direction::Up,
        Direction::Down,
    ];
    fn plain_target(from: Location, direction: Direction) -> Option<Location> {
        let (dx, dy, dz) = direction.delta();
        let p = from.position;
        Some(Location {
            region: from.region,
            position: Position {
                x: p.x.checked_add(dx)?,
                y: p.y.checked_add(dy)?,
                z: p.z.checked_add(dz)?,
            },
        })
    }
    fn is_plain(from: Location, direction: Direction, edge: &Neighbor) -> bool {
        matches!(edge, Neighbor::Placed { target, rotation: 0 }
            if plain_target(from, direction) == Some(*target))
    }

    #[derive(Serialize)]
    struct NeighborRegion {
        region: RegionId,
        plain: Columns<u8>,
        other: Vec<((Location, Direction), Neighbor)>,
    }
    impl<'de> Deserialize<'de> for NeighborRegion {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Fields {
                region: RegionId,
                plain: Columns<u8>,
                other: Vec<((Location, Direction), Neighbor)>,
            }
            let fields: Fields = crate::bounded::named_map(d)?;
            Ok(Self {
                region: fields.region,
                plain: fields.plain,
                other: fields.other,
            })
        }
    }
    impl NeighborRegion {
        fn encode(map: &Neighbors) -> Self {
            let mut plain = BTreeMap::<Position, u8>::new();
            let mut other = Vec::new();
            for (&(from, direction), edge) in map.iter() {
                match DIRECTIONS.iter().position(|d| *d == direction) {
                    Some(bit) if is_plain(from, direction, edge) => {
                        *plain.entry(from.position).or_default() |= 1 << bit;
                    }
                    _ => other.push(((from, direction), edge.clone())),
                }
            }
            Self {
                region: map
                    .keys()
                    .next()
                    .expect("pooled regions are nonempty")
                    .0
                    .region,
                plain: columns(plain),
                other,
            }
        }
        fn decode(self) -> Option<Neighbors> {
            let mut map = Neighbors::default();
            expand_columns(self.plain, |position, mask| {
                if !(1..64).contains(&mask) {
                    return None;
                }
                let from = Location {
                    region: self.region,
                    position,
                };
                for (bit, direction) in DIRECTIONS.into_iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        map.insert(
                            (from, direction),
                            Neighbor::Placed {
                                target: plain_target(from, direction)?,
                                rotation: 0,
                            },
                        );
                    }
                }
                Some(())
            })?;
            let mut previous = None;
            for ((from, direction), edge) in self.other {
                let key = (from, direction);
                if from.region != self.region
                    || !DIRECTIONS.contains(&direction)
                    || previous.is_some_and(|before| before >= key)
                    || is_plain(from, direction, &edge)
                    || map.contains_key(&key)
                {
                    return None;
                }
                previous = Some(key);
                map.insert(key, edge);
            }
            map.is_checkpoint_region().then_some(map)
        }
    }

    /// Private wire shape. The runtime and validation tables below still use
    /// RegionMap; detached actors use this same codec, without cropping history.
    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct SavedRegions {
        palette: Vec<Shared<MapAppearance>>,
        cells: Vec<CellRegion>,
        neighbors: Vec<NeighborRegion>,
        witnesses: Vec<RegionMap<Location, Witness>>,
        children: Vec<RegionMap<ChildKey, u8>>,
        instances: Vec<Instance>,
    }
    impl SavedRegions {
        fn encode(saved: Regions) -> Self {
            let palette = AppearancePalette::new(
                saved
                    .cells
                    .iter()
                    .flat_map(|bucket| bucket.iter().map(|(_, appearance)| appearance)),
            );
            let cells = saved
                .cells
                .iter()
                .map(|bucket| CellRegion {
                    region: bucket
                        .keys()
                        .next()
                        .expect("pooled regions are nonempty")
                        .region,
                    cells: columns(bucket.iter().map(|(at, appearance)| {
                        (
                            at.position,
                            palette.ranks[&std::ptr::from_ref::<MapAppearance>(appearance)],
                        )
                    })),
                })
                .collect();
            Self {
                palette: palette.values,
                cells,
                neighbors: saved.neighbors.iter().map(NeighborRegion::encode).collect(),
                witnesses: saved.witnesses,
                children: saved.children,
                instances: saved.instances,
            }
        }
        fn decode(self) -> Option<Regions> {
            // Descriptor and region-reference checks precede all row expansion.
            checked_directories(
                &self.instances,
                &self.cells,
                &self.neighbors,
                &self.witnesses,
                &self.children,
            )?;
            if self.palette.windows(2).any(|pair| pair[0] >= pair[1]) {
                return None;
            }
            let mut used = vec![false; self.palette.len()];
            let mut cells = Vec::new();
            for bucket in self.cells {
                let mut map = Cells::default();
                expand_columns(bucket.cells, |position, index| {
                    let appearance = self.palette.get(index)?;
                    used[index] = true;
                    map.insert(
                        Location {
                            region: bucket.region,
                            position,
                        },
                        appearance.clone(),
                    );
                    Some(())
                })?;
                cells.push(map);
            }
            if used.iter().any(|used| !used) {
                return None;
            }
            Some(Regions {
                cells,
                neighbors: self
                    .neighbors
                    .into_iter()
                    .map(NeighborRegion::decode)
                    .collect::<Option<Vec<_>>>()?,
                witnesses: self.witnesses,
                children: self.children,
                instances: self.instances,
            })
        }
    }

    trait PooledRegion {
        fn pooled_region(&self) -> Option<RegionId>;
    }
    impl<K: RegionKey, V: PartialEq> PooledRegion for RegionMap<K, V> {
        fn pooled_region(&self) -> Option<RegionId> {
            self.is_checkpoint_region()
                .then(|| self.keys().next().unwrap().region())
        }
    }
    impl PooledRegion for CellRegion {
        fn pooled_region(&self) -> Option<RegionId> {
            (!self.cells.is_empty()).then_some(self.region)
        }
    }
    impl PooledRegion for NeighborRegion {
        fn pooled_region(&self) -> Option<RegionId> {
            (!self.plain.is_empty() || !self.other.is_empty()).then_some(self.region)
        }
    }

    type AppearanceRanks = BTreeMap<*const MapAppearance, usize>;
    struct AppearancePalette {
        values: Vec<Shared<MapAppearance>>,
        ranks: AppearanceRanks,
    }
    impl AppearancePalette {
        fn new<'a>(appearances: impl Iterator<Item = &'a Shared<MapAppearance>>) -> Self {
            // Input roots keep every allocation alive for this entire operation.
            // Deduplicate pointers before content comparisons: a palette value
            // can contain an arbitrarily long legal ground-item vector.
            let allocations: BTreeMap<_, _> = appearances
                .map(|appearance| {
                    (
                        std::ptr::from_ref::<MapAppearance>(appearance),
                        appearance.clone(),
                    )
                })
                .collect();
            let mut distinct = BTreeMap::<Shared<MapAppearance>, Vec<*const MapAppearance>>::new();
            for (allocation, appearance) in allocations {
                distinct.entry(appearance).or_default().push(allocation);
            }
            let mut values = Vec::new();
            let mut ranks = BTreeMap::new();
            for (appearance, allocations) in distinct {
                for allocation in allocations {
                    ranks.insert(allocation, values.len());
                }
                values.push(appearance);
            }
            Self { values, ranks }
        }
    }

    trait PoolValue: Ord {
        fn pool_cmp(&self, other: &Self, _: &AppearanceRanks) -> std::cmp::Ordering {
            self.cmp(other)
        }
    }
    impl PoolValue for Neighbor {}
    impl PoolValue for Witness {}
    impl PoolValue for u8 {}
    impl PoolValue for Shared<MapAppearance> {
        fn pool_cmp(&self, other: &Self, ranks: &AppearanceRanks) -> std::cmp::Ordering {
            ranks[&std::ptr::from_ref::<MapAppearance>(self)]
                .cmp(&ranks[&std::ptr::from_ref::<MapAppearance>(other)])
        }
    }

    /// Full content equality is used only once per distinct bucket allocation.
    /// Entry comparisons are observable so near-equal historical versions cannot
    /// hide a linear candidate search behind validation-only counters.
    struct BucketContent<K, V>(Shared<BTreeMap<K, V>>, Rc<AppearanceRanks>);
    impl<K: Ord, V: PoolValue> PartialEq for BucketContent<K, V> {
        fn eq(&self, other: &Self) -> bool {
            self.cmp(other).is_eq()
        }
    }
    impl<K: Ord, V: PoolValue> Eq for BucketContent<K, V> {}
    impl<K: Ord, V: PoolValue> PartialOrd for BucketContent<K, V> {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl<K: Ord, V: PoolValue> Ord for BucketContent<K, V> {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            let length = self.0.len().cmp(&other.0.len());
            if !length.is_eq() {
                return length;
            }
            for ((key, value), (other_key, other_value)) in self.0.iter().zip(other.0.iter()) {
                crate::explored_map::count(|n| n.pool_compared_entries += 1);
                let order = key
                    .cmp(other_key)
                    .then_with(|| value.pool_cmp(other_value, &self.1));
                if !order.is_eq() {
                    return order;
                }
            }
            std::cmp::Ordering::Equal
        }
    }

    struct Buckets<K, V> {
        saved: Vec<RegionMap<K, V>>,
        // This identity never escapes collect(). All input roots remain held
        // for that call, so even a noncanonical equal allocation stays alive.
        allocations: BTreeMap<*const BTreeMap<K, V>, usize>,
        contents: BTreeMap<BucketContent<K, V>, usize>,
        appearance_ranks: Rc<AppearanceRanks>,
    }
    impl<K, V> Default for Buckets<K, V> {
        fn default() -> Self {
            Self {
                saved: Vec::new(),
                allocations: BTreeMap::new(),
                contents: BTreeMap::new(),
                appearance_ranks: Rc::default(),
            }
        }
    }
    impl<K: RegionKey, V: PoolValue> Buckets<K, V> {
        fn indices(&mut self, map: &RegionMap<K, V>) -> Vec<usize> {
            map.shared_regions()
                .map(|(&region, bucket)| {
                    let allocation = std::ptr::from_ref::<BTreeMap<K, V>>(bucket);
                    if let Some(&index) = self.allocations.get(&allocation) {
                        crate::explored_map::count(|n| n.pool_identity_hits += 1);
                        return index;
                    }
                    crate::explored_map::count(|n| n.pool_bucket_allocations += 1);
                    let index = match self
                        .contents
                        .entry(BucketContent(bucket.clone(), self.appearance_ranks.clone()))
                    {
                        std::collections::btree_map::Entry::Occupied(entry) => *entry.get(),
                        std::collections::btree_map::Entry::Vacant(entry) => {
                            let index = self.saved.len();
                            self.saved
                                .push(RegionMap::from_shared_region(region, bucket.clone()));
                            entry.insert(index);
                            index
                        }
                    };
                    self.allocations.insert(allocation, index);
                    index
                })
                .collect()
        }
    }

    fn collect(maps: &[Shared<ExploredMap>]) -> Regions {
        // Shared historical buckets are visited once, before content pooling.
        let cell_buckets: BTreeMap<_, _> = maps
            .iter()
            .flat_map(|map| {
                map.cells.shared_regions().map(|(_, bucket)| {
                    (
                        std::ptr::from_ref::<BTreeMap<Location, Shared<MapAppearance>>>(bucket),
                        bucket,
                    )
                })
            })
            .collect();
        let palette =
            AppearancePalette::new(cell_buckets.values().flat_map(|bucket| bucket.values()));
        let mut cells = Buckets {
            appearance_ranks: Rc::new(palette.ranks),
            ..Buckets::default()
        };
        let mut neighbors = Buckets::default();
        let mut witnesses = Buckets::default();
        let mut children = Buckets::default();
        let instances = maps
            .iter()
            .map(|map| Instance {
                cells: cells.indices(&map.cells),
                neighbors: neighbors.indices(&map.neighbors),
                witnesses: witnesses.indices(&map.witnesses),
                children: children.indices(&map.children),
            })
            .collect();
        Regions {
            cells: cells.saved,
            neighbors: neighbors.saved,
            witnesses: witnesses.saved,
            children: children.saved,
            instances,
        }
    }

    pub(super) fn serialize<S: Serializer>(
        maps: &[Shared<ExploredMap>],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        SavedRegions::encode(collect(maps)).serialize(serializer)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Vec<Shared<ExploredMap>>, D::Error> {
        let encoded: SavedRegions = crate::bounded::named_map(deserializer)?;
        let saved = encoded.decode().ok_or_else(|| {
            <D::Error as serde::de::Error>::custom("invalid explored-map compact pool")
        })?;
        let maps = saved
            .instances
            .iter()
            .map(|instance| {
                Some(Shared::new(ExploredMap {
                    cells: RegionMap::restore_regions(&instance.cells, &saved.cells)?,
                    neighbors: RegionMap::restore_regions(&instance.neighbors, &saved.neighbors)?,
                    witnesses: RegionMap::restore_regions(&instance.witnesses, &saved.witnesses)?,
                    children: RegionMap::restore_regions(&instance.children, &saved.children)?,
                }))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| {
                <D::Error as serde::de::Error>::custom("invalid explored-map region reference")
            })?;
        // Re-pooling canonicalizes semantically equal buckets too, so aliases
        // using redundant bucket indices cannot evade descriptor uniqueness.
        let mut validation = RestoreValidation::new(&maps).ok_or_else(|| {
            <D::Error as serde::de::Error>::custom("invalid explored-map evidence")
        })?;
        for index in 0..maps.len() {
            validation.validate_knowledge(index).ok_or_else(|| {
                <D::Error as serde::de::Error>::custom("invalid explored-map evidence")
            })?;
        }
        Ok(maps)
    }

    #[cfg(test)]
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(super) struct ValidationWork {
        pub descriptor_checks: usize,
        pub appearance_cells: usize,
        pub appearance_payloads: usize,
        pub neighbor_rows: usize,
        pub witness_rows: usize,
        pub child_rows: usize,
        pub world_cells: usize,
    }
    #[cfg(test)]
    thread_local! { static WORK: std::cell::Cell<ValidationWork> = const {
        std::cell::Cell::new(ValidationWork { descriptor_checks: 0, appearance_cells: 0, appearance_payloads: 0,
            neighbor_rows: 0, witness_rows: 0, child_rows: 0, world_cells: 0 })
    }; }
    #[cfg(test)]
    pub(super) fn work_counts() -> ValidationWork {
        WORK.with(std::cell::Cell::get)
    }
    #[cfg(test)]
    pub(super) fn reset_work_counts() {
        WORK.with(|c| c.set(ValidationWork::default()));
    }
    macro_rules! visit {
        (@local $field:ident) => {
            #[cfg(test)]
            WORK.with(|c| { let mut n = c.get(); n.$field += 1; c.set(n); });
        };
        (descriptor_checks) => { visit!(@local descriptor_checks); };
        (appearance_cells) => {
            crate::explored_map::count_map_validation(1, 0, 0);
            visit!(@local appearance_cells);
        };
        (world_cells) => {
            crate::explored_map::count_map_validation(0, 0, 1);
            visit!(@local world_cells);
        };
        ($field:ident) => {
            crate::explored_map::count_map_validation(0, 1, 0);
            visit!(@local $field);
        };
    }

    struct Directories {
        cells: BTreeMap<RegionId, usize>,
        neighbors: BTreeMap<RegionId, usize>,
        witnesses: BTreeMap<RegionId, usize>,
        children: BTreeMap<RegionId, usize>,
    }

    fn directory<T: PooledRegion>(
        indices: &[usize],
        pool: &[T],
        used: &mut BTreeSet<usize>,
    ) -> Option<BTreeMap<RegionId, usize>> {
        let mut directory = BTreeMap::new();
        let mut previous = None;
        for &index in indices {
            let region = pool.get(index)?.pooled_region()?;
            if previous.is_some_and(|before| before >= region) {
                return None;
            }
            previous = Some(region);
            directory.insert(region, index);
            used.insert(index);
        }
        Some(directory)
    }

    fn directories(saved: &Regions) -> Option<Vec<Directories>> {
        checked_directories(
            &saved.instances,
            &saved.cells,
            &saved.neighbors,
            &saved.witnesses,
            &saved.children,
        )
    }

    fn checked_directories<C: PooledRegion, N: PooledRegion>(
        instances: &[Instance],
        cell_pool: &[C],
        neighbor_pool: &[N],
        witness_pool: &[RegionMap<Location, Witness>],
        child_pool: &[RegionMap<ChildKey, u8>],
    ) -> Option<Vec<Directories>> {
        let mut distinct = BTreeSet::new();
        // Keep this ahead of both reference expansion and payload validation.
        for instance in instances {
            visit!(descriptor_checks);
            if !distinct.insert(instance) {
                return None;
            }
        }
        let (mut cells, mut neighbors, mut witnesses, mut children) = (
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
            BTreeSet::new(),
        );
        let result = instances
            .iter()
            .map(|instance| {
                Some(Directories {
                    cells: directory(&instance.cells, cell_pool, &mut cells)?,
                    neighbors: directory(&instance.neighbors, neighbor_pool, &mut neighbors)?,
                    witnesses: directory(&instance.witnesses, witness_pool, &mut witnesses)?,
                    children: directory(&instance.children, child_pool, &mut children)?,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        (cells.len() == cell_pool.len()
            && neighbors.len() == neighbor_pool.len()
            && witnesses.len() == witness_pool.len()
            && children.len() == child_pool.len())
        .then_some(result)
    }

    #[derive(Default)]
    struct Dependencies {
        cells: BTreeSet<RegionId>,
        reverse: BTreeSet<RegionId>,
    }
    fn dependency_indices(
        regions: &BTreeSet<RegionId>,
        directory: &BTreeMap<RegionId, usize>,
    ) -> Option<Vec<usize>> {
        regions
            .iter()
            .map(|region| directory.get(region).copied())
            .collect()
    }
    fn has_cell(saved: &Regions, directory: &Directories, at: Location) -> bool {
        directory
            .cells
            .get(&at.region)
            .is_some_and(|&index| saved.cells[index].contains_key(&at))
    }

    /// Owns only shared immutable buckets and small validation identities. A
    /// restored root never repeats validation of unchanged dependent buckets.
    pub(super) struct RestoreValidation {
        saved: Regions,
        directories: Vec<Directories>,
        neighbor_dependencies: Vec<Dependencies>,
        witness_dependencies: Vec<Dependencies>,
        child_dependencies: Vec<Dependencies>,
        valid_neighbors: BTreeSet<(usize, Vec<usize>)>,
        valid_witnesses: BTreeSet<(usize, Vec<usize>, Vec<usize>)>,
        valid_children: BTreeSet<(usize, Vec<usize>)>,
        valid_instances: BTreeSet<usize>,
        // Region names, residency and other world state cannot affect bounds.
        valid_world_cells: BTreeSet<(usize, Position, (i32, i32, i32))>,
    }

    impl RestoreValidation {
        pub(super) fn new(maps: &[Shared<ExploredMap>]) -> Option<Self> {
            Self::from_saved(collect(maps))
        }
        fn from_saved(saved: Regions) -> Option<Self> {
            let directories = directories(&saved)?;
            let mut valid_appearances = BTreeSet::new();
            for bucket in &saved.cells {
                for (_, appearance) in bucket.iter() {
                    visit!(appearance_cells);
                    if valid_appearances.insert(std::ptr::from_ref::<MapAppearance>(appearance)) {
                        visit!(@local appearance_payloads);
                        if !appearance.valid() {
                            return None;
                        }
                    }
                }
            }
            let mut neighbor_dependencies = Vec::new();
            for bucket in &saved.neighbors {
                let mut depends = Dependencies::default();
                for ((source, direction), edge) in bucket.iter() {
                    visit!(neighbor_rows);
                    if !matches!(
                        direction,
                        Direction::North
                            | Direction::East
                            | Direction::South
                            | Direction::West
                            | Direction::Up
                            | Direction::Down
                    ) {
                        return None;
                    }
                    depends.cells.insert(source.region);
                    if let Neighbor::Placed { target, rotation } = edge {
                        if *rotation >= 24 {
                            return None;
                        }
                        depends.cells.insert(target.region);
                    }
                }
                neighbor_dependencies.push(depends);
            }
            let mut witness_dependencies = Vec::new();
            for bucket in &saved.witnesses {
                let mut depends = Dependencies::default();
                for (child, witness) in bucket.iter() {
                    visit!(witness_rows);
                    depends.cells.insert(child.region);
                    if let Witness::Placed {
                        anchor,
                        delta,
                        rotation,
                    } = witness
                    {
                        // A valid cube rotation is a signed permutation. The
                        // inverse must also negate delta, excluding MIN on every axis.
                        if *rotation >= 24 || delta.iter().any(|n| n.checked_neg().is_none()) {
                            return None;
                        }
                        depends.cells.insert(anchor.region);
                        depends.reverse.insert(anchor.region);
                    }
                }
                witness_dependencies.push(depends);
            }
            let mut child_dependencies = Vec::new();
            for bucket in &saved.children {
                let mut depends = Dependencies::default();
                for ((_, _, child), rotation) in bucket.iter() {
                    visit!(child_rows);
                    if *rotation >= 24 {
                        return None;
                    }
                    depends.reverse.insert(child.region);
                }
                child_dependencies.push(depends);
            }
            Some(Self {
                saved,
                directories,
                neighbor_dependencies,
                witness_dependencies,
                child_dependencies,
                valid_neighbors: BTreeSet::new(),
                valid_witnesses: BTreeSet::new(),
                valid_children: BTreeSet::new(),
                valid_instances: BTreeSet::new(),
                valid_world_cells: BTreeSet::new(),
            })
        }

        fn validate_knowledge(&mut self, index: usize) -> Option<()> {
            if self.valid_instances.contains(&index) {
                return Some(());
            }
            let directory = self.directories.get(index)?;
            for &bucket in directory.neighbors.values() {
                let dependencies = dependency_indices(
                    &self.neighbor_dependencies[bucket].cells,
                    &directory.cells,
                )?;
                let key = (bucket, dependencies);
                if !self.valid_neighbors.contains(&key) {
                    for ((source, _), edge) in self.saved.neighbors[bucket].iter() {
                        visit!(neighbor_rows);
                        if !has_cell(&self.saved, directory, *source)
                            || matches!(edge, Neighbor::Placed { target, .. } if !has_cell(&self.saved, directory, *target))
                        {
                            return None;
                        }
                    }
                    self.valid_neighbors.insert(key);
                }
            }
            for &bucket in directory.witnesses.values() {
                let depends = &self.witness_dependencies[bucket];
                let key = (
                    bucket,
                    dependency_indices(&depends.cells, &directory.cells)?,
                    dependency_indices(&depends.reverse, &directory.children)?,
                );
                if !self.valid_witnesses.contains(&key) {
                    for (child, witness) in self.saved.witnesses[bucket].iter() {
                        visit!(witness_rows);
                        if !has_cell(&self.saved, directory, *child) {
                            return None;
                        }
                        if let Witness::Placed {
                            anchor,
                            delta,
                            rotation,
                        } = witness
                        {
                            let reverse = *directory.children.get(&anchor.region)?;
                            if !has_cell(&self.saved, directory, *anchor)
                                || self.saved.children[reverse].get(&(*anchor, *delta, *child))
                                    != Some(rotation)
                            {
                                return None;
                            }
                        }
                    }
                    self.valid_witnesses.insert(key);
                }
            }
            for &bucket in directory.children.values() {
                let dependencies = dependency_indices(
                    &self.child_dependencies[bucket].reverse,
                    &directory.witnesses,
                )?;
                let key = (bucket, dependencies);
                if !self.valid_children.contains(&key) {
                    for ((anchor, delta, child), rotation) in self.saved.children[bucket].iter() {
                        visit!(child_rows);
                        let witness = *directory.witnesses.get(&child.region)?;
                        if self.saved.witnesses[witness].get(child)
                            != Some(&Witness::Placed {
                                anchor: *anchor,
                                delta: *delta,
                                rotation: *rotation,
                            })
                        {
                            return None;
                        }
                    }
                    self.valid_children.insert(key);
                }
            }
            self.valid_instances.insert(index);
            Some(())
        }

        pub(super) fn validate(&mut self, index: usize, world: &World) -> Option<()> {
            self.validate_knowledge(index)?;
            for &bucket in &self.saved.instances.get(index)?.cells {
                let cells = &self.saved.cells[bucket];
                let region = cells.keys().next()?.region;
                let bounds = world.known_region(region)?.bounds;
                let key = (bucket, bounds.origin(), bounds.dimensions());
                if !self.valid_world_cells.contains(&key) {
                    for at in cells.keys() {
                        visit!(world_cells);
                        // Use exactly the metadata in the cache key. A forged
                        // world with conflicting loaded/absent entries cannot
                        // seed a reusable proof from the other entry's bounds.
                        if !bounds.contains(at.position) {
                            return None;
                        }
                    }
                    self.valid_world_cells.insert(key);
                }
            }
            Some(())
        }
    }
    /// Temporary owner validation for one record/arrival operation. Equal roots
    /// and shared buckets are checked once; every caller still checks ownership.
    pub(crate) struct OwnerValidation {
        validation: RestoreValidation,
        indices: BTreeMap<*const ExploredMap, usize>,
        // Keep every indexed allocation alive, including equal noncanonical roots.
        _roots: Vec<Shared<ExploredMap>>,
    }
    impl OwnerValidation {
        pub(crate) fn new<'a>(
            maps: impl IntoIterator<Item = &'a Shared<ExploredMap>>,
        ) -> Option<Self> {
            let mut indices = BTreeMap::new();
            let mut roots = Vec::new();
            for map in maps {
                let allocation = std::ptr::from_ref::<ExploredMap>(map);
                if let std::collections::btree_map::Entry::Vacant(entry) = indices.entry(allocation)
                {
                    entry.insert(roots.len());
                    roots.push(map.clone());
                }
            }
            let mut saved = collect(&roots);
            let mut distinct = BTreeMap::new();
            let mut instances = Vec::new();
            let remap = saved
                .instances
                .into_iter()
                .map(|instance| {
                    if let Some(&index) = distinct.get(&instance) {
                        return index;
                    }
                    let index = instances.len();
                    distinct.insert(instance.clone(), index);
                    instances.push(instance);
                    index
                })
                .collect::<Vec<_>>();
            saved.instances = instances;
            for index in indices.values_mut() {
                *index = remap[*index];
            }
            Some(Self {
                validation: RestoreValidation::from_saved(saved)?,
                indices,
                _roots: roots,
            })
        }
        pub(crate) fn structure(&mut self, map: &Shared<ExploredMap>) -> bool {
            self.indices
                .get(&std::ptr::from_ref::<ExploredMap>(map))
                .copied()
                .is_some_and(|index| self.validation.validate_knowledge(index).is_some())
        }
        pub(crate) fn world(&mut self, map: &Shared<ExploredMap>, world: &World) -> bool {
            self.indices
                .get(&std::ptr::from_ref::<ExploredMap>(map))
                .copied()
                .is_some_and(|index| self.validation.validate(index, world).is_some())
        }
    }
}

pub(crate) use explored_map_regions::OwnerValidation as ExploredOwnerValidation;

/// A detached actor retains all its remembered regions, with the same pooled
/// encoding as loaded checkpoints. Detachment never crops memory to its region.
pub(crate) mod explored_actor_maps {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Table {
        #[serde(with = "tor_world::checkpoint_map")]
        owners: BTreeMap<ActorId, usize>,
        #[serde(with = "super::explored_map_regions")]
        maps: Vec<Shared<ExploredMap>>,
    }

    pub(crate) fn serialize<S: Serializer>(
        actors: &BTreeMap<ActorId, Shared<ExploredMap>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let mut maps = Vec::<Shared<ExploredMap>>::new();
        let owners = actors
            .iter()
            .map(|(actor, map)| {
                let index = maps
                    .iter()
                    .position(|existing| existing.shares_storage(map) || existing == map)
                    .unwrap_or_else(|| {
                        maps.push(map.clone());
                        maps.len() - 1
                    });
                (*actor, index)
            })
            .collect();
        Table { owners, maps }.serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<ActorId, Shared<ExploredMap>>, D::Error> {
        let saved: Table = crate::bounded::named_map(deserializer)?;
        let mut used = BTreeSet::new();
        let mut owners = BTreeMap::new();
        for (actor, index) in saved.owners {
            let map = saved
                .maps
                .get(index)
                .filter(|_| actor.0 != 0)
                .ok_or_else(|| serde::de::Error::custom("invalid explored-map owner reference"))?;
            used.insert(index);
            owners.insert(actor, map.clone());
        }
        if used.len() != saved.maps.len() {
            return Err(serde::de::Error::custom(
                "unreferenced explored-map instance",
            ));
        }
        Ok(owners)
    }
}

#[cfg(test)]
mod explored_map_tests {
    use super::*;
    use crate::explored_map::{
        MapAppearance, MapDoorAppearance, MapItemAppearance, Neighbor, Witness,
    };
    use std::num::NonZeroU64;
    use tor_world::{Direction, Extent, Position, Region, RegionId};

    #[derive(Serialize, Deserialize)]
    struct Maps {
        #[serde(with = "super::explored_map_regions")]
        maps: Vec<Shared<ExploredMap>>,
    }

    fn at(region: u64, x: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y: 1, z: 0 },
        }
    }

    fn appearance(material: &str) -> Shared<MapAppearance> {
        Shared::new(MapAppearance {
            wall: true,
            material: material.into(),
            asset: Some("wall/remembered".into()),
            place_hint: true,
            stairs_up: true,
            stairs_down: false,
            door: Some(MapDoorAppearance {
                name: "old gate".into(),
                description: "last seen closed".into(),
                open: false,
                asset: Some("door/closed".into()),
            }),
            ground_items: vec![MapItemAppearance {
                class: crate::ItemClass::Potion,
                description: "a blue bottle".into(),
                name: "blue potion".into(),
                appearance: "blue".into(),
                identified: false,
                quantity: 3,
                asset: Some("item/blue".into()),
            }],
        })
    }

    fn map() -> ExploredMap {
        let mut map = ExploredMap::default();
        for location in [at(1, 1), at(1, 2), at(2, 1)] {
            map.cells.insert(location, appearance("remembered stone"));
        }
        map.neighbors.insert(
            (at(1, 1), Direction::East),
            Neighbor::Placed {
                target: at(1, 2),
                rotation: 0,
            },
        );
        map.neighbors.insert(
            (at(1, 2), Direction::West),
            Neighbor::Placed {
                target: at(1, 1),
                rotation: 0,
            },
        );
        map.witnesses.insert(
            at(2, 1),
            Witness::Placed {
                anchor: at(1, 1),
                delta: [10, 0, 0],
                rotation: 0,
            },
        );
        map.children.insert((at(1, 1), [10, 0, 0], at(2, 1)), 0);
        map
    }

    fn large_map(cells: i32) -> ExploredMap {
        let mut map = ExploredMap::default();
        let visual = appearance("shared history");
        for x in 0..cells {
            map.cells.insert(at(1, x), visual.clone());
        }
        map.neighbors.insert(
            (at(1, 0), Direction::East),
            Neighbor::Placed {
                target: at(1, 1),
                rotation: 0,
            },
        );
        map.neighbors.insert(
            (at(1, 1), Direction::West),
            Neighbor::Placed {
                target: at(1, 0),
                rotation: 0,
            },
        );
        let delta = [i64::from(cells - 1), 0, 0];
        map.witnesses.insert(
            at(1, cells - 1),
            Witness::Placed {
                anchor: at(1, 0),
                delta,
                rotation: 0,
            },
        );
        map.children.insert((at(1, 0), delta, at(1, cells - 1)), 0);
        map
    }

    fn roots_with_unrelated_changes(cells: i32, roots: usize) -> Vec<Shared<ExploredMap>> {
        let base = large_map(cells);
        (0..roots)
            .map(|root| {
                let mut map = base.clone();
                map.cells
                    .insert(at(2, 1), appearance(&format!("unrelated change {root}")));
                Shared::new(map)
            })
            .collect()
    }

    #[test]
    fn duplicate_root_descriptors_fail_before_history_sized_validation() {
        use super::explored_map_regions::{reset_work_counts, work_counts};
        let map = Shared::new(large_map(4096));
        for roots in [2, 128] {
            let bytes = serde_json::to_vec(&Maps {
                maps: vec![map.clone(); roots],
            })
            .unwrap();
            reset_work_counts();
            crate::reset_map_work_counts();
            assert!(serde_json::from_slice::<Maps>(&bytes).is_err());
            let work = work_counts();
            assert_eq!(work.descriptor_checks, 2);
            assert_eq!(work.appearance_cells, 0);
            assert_eq!(work.neighbor_rows + work.witness_rows + work.child_rows, 0);
            assert_eq!(crate::map_work_counts().validation_cells, 0);
            assert_eq!(crate::map_work_counts().validation_relations, 0);
        }
    }

    #[test]
    fn redundant_bucket_indices_cannot_hide_duplicate_map_roots() {
        use super::explored_map_regions::{reset_work_counts, work_counts};
        let mut value = serde_json::to_value(Maps {
            maps: vec![Shared::new(large_map(1024))],
        })
        .unwrap();
        let cells = value["maps"]["cells"][0].clone();
        value["maps"]["cells"].as_array_mut().unwrap().push(cells);
        let mut alias = value["maps"]["instances"][0].clone();
        alias["cells"] = serde_json::json!([1]);
        value["maps"]["instances"]
            .as_array_mut()
            .unwrap()
            .push(alias);
        reset_work_counts();
        assert!(serde_json::from_value::<Maps>(value).is_err());
        assert_eq!(work_counts().appearance_cells, 0);
        assert_eq!(
            work_counts().neighbor_rows + work_counts().witness_rows + work_counts().child_rows,
            0
        );
    }

    #[test]
    fn distinct_roots_validate_shared_payloads_and_actual_relation_dependencies_once() {
        use super::explored_map_regions::{reset_work_counts, work_counts};
        for roots in [1, 32] {
            let maps = roots_with_unrelated_changes(4096, roots);
            let bytes = serde_json::to_vec(&Maps { maps: maps.clone() }).unwrap();
            reset_work_counts();
            crate::reset_map_work_counts();
            let restored: Maps = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(restored.maps, maps);
            let work = work_counts();
            assert_eq!(work.appearance_cells, 4096 + roots);
            assert_eq!(
                work.neighbor_rows, 4,
                "unrelated region changes cannot invalidate links"
            );
            assert_eq!(work.witness_rows, 2);
            assert_eq!(work.child_rows, 2);
            assert_eq!(crate::map_work_counts().validation_cells, 4096 + roots);
            assert_eq!(crate::map_work_counts().validation_relations, 8);
        }
    }

    #[test]
    fn later_shared_bucket_versions_are_canonicalized_once_before_content_comparison() {
        const CELLS: i32 = 4096;
        for roots in [1, 32, 64] {
            let original = Shared::new(large_map(CELLS));
            let mut later = (*original).clone();
            later
                .cells
                .insert(at(1, CELLS - 1), appearance("later version"));
            let mut maps = vec![original];
            for root in 0..roots {
                let mut map = later.clone();
                map.cells
                    .insert(at(2, 1), appearance(&format!("tiny change {root:03}")));
                maps.push(Shared::new(map));
            }
            crate::reset_map_work_counts();
            let _validation = super::explored_map_regions::RestoreValidation::new(&maps).unwrap();
            let work = crate::map_work_counts();
            assert_eq!(work.pool_bucket_allocations, roots + 5);
            assert_eq!(work.validation_cells, 2 * CELLS as usize + roots);
            assert!(work.pool_identity_hits >= roots * 3);
            // The second large bucket is compared once with the first; extra
            // roots compare only their one-cell unrelated buckets.
            assert!(work.pool_compared_entries >= CELLS as usize);
            assert!(work.pool_compared_entries <= CELLS as usize + roots * 32);
        }
    }

    #[test]
    fn near_equal_distinct_bucket_versions_use_ordered_not_linear_canonicalization() {
        const CELLS: i32 = 256;
        for versions in [8, 128] {
            let original = large_map(CELLS);
            let maps = (0..versions)
                .map(|version| {
                    let mut map = original.clone();
                    map.cells.insert(
                        at(1, CELLS - 1),
                        appearance(&format!("version {version:03}")),
                    );
                    Shared::new(map)
                })
                .collect::<Vec<_>>();
            crate::reset_map_work_counts();
            let _validation = super::explored_map_regions::RestoreValidation::new(&maps).unwrap();
            let work = crate::map_work_counts();
            assert_eq!(work.pool_bucket_allocations, versions + 3);
            assert_eq!(work.validation_cells, CELLS as usize * versions);
            assert!(work.pool_compared_entries > 0);
            // An ordered B-tree search is logarithmic in versions; this loose
            // ceiling allows its small-node comparisons but excludes V*(V-1)/2
            // full scans for the 128-version witness.
            assert!(work.pool_compared_entries <= CELLS as usize * versions * 36);
        }
    }

    #[test]
    fn owner_batches_share_bucket_validation_across_distinct_and_equal_roots() {
        let mut roots = roots_with_unrelated_changes(4096, 32);
        // Equal content in a distinct root is valid for a different owner, and
        // repeated ownership of that exact root must also stay constant-work.
        roots.push(Shared::new((*roots[0]).clone()));
        roots.extend(vec![roots[0].clone(); 32]);
        let world = validation_world(4096, 0);
        crate::reset_map_work_counts();
        let mut validation = super::ExploredOwnerValidation::new(roots.iter()).unwrap();
        for root in &roots {
            assert!(validation.structure(root));
            assert!(validation.world(root, &world));
        }
        let work = crate::map_work_counts();
        assert_eq!(work.validation_cells, 4096 + 32);
        assert_eq!(work.validation_relations, 8);
        assert_eq!(work.world_location_checks, 4096 + 32);
    }

    #[test]
    fn pooled_relation_cache_never_crosses_changed_endpoints_or_reverse_indices() {
        let original = Shared::new(large_map(64));
        for damage_reverse in [false, true] {
            let mut damaged = original.clone();
            if damage_reverse {
                damaged
                    .children
                    .insert((at(1, 0), [63, 0, 0], at(1, 63)), 1);
            } else {
                damaged.cells.remove(&at(1, 1));
            }
            let maps = Maps {
                maps: vec![original.clone(), damaged],
            };
            let bytes = serde_json::to_vec(&maps).unwrap();
            assert!(serde_json::from_slice::<Maps>(&bytes).is_err());
            // Also exercise forged in-memory state, which bypasses serde.
            let mut validator =
                super::explored_map_regions::RestoreValidation::new(&maps.maps).unwrap();
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_region(Region {
                    id: RegionId(1),
                    name: "memory".into(),
                    bounds: Extent::new(64, 3, 1).unwrap(),
                })
                .unwrap();
            assert!(validator.validate(0, &world).is_some());
            assert!(validator.validate(1, &world).is_none());
        }
    }

    fn validation_world(cells: i32, label: usize) -> World {
        let mut world = World::new(vec![], vec![]).unwrap();
        for (id, width) in [(1, cells), (2, 3)] {
            world
                .add_region(Region {
                    id: RegionId(id),
                    name: format!("world {label} region {id}"),
                    bounds: Extent::new(width, 3, 1).unwrap(),
                })
                .unwrap();
        }
        world
    }

    #[test]
    fn pooled_world_cache_tracks_bounds_and_reuses_loaded_or_detached_metadata() {
        use super::explored_map_regions::{reset_work_counts, work_counts, RestoreValidation};
        let mut validation = RestoreValidation::new(&[Shared::new(large_map(64))]).unwrap();
        let loaded = validation_world(64, 0);
        reset_work_counts();
        assert!(validation.validate(0, &loaded).is_some());
        assert_eq!(work_counts().world_cells, 64);
        let mut detached = World::new(vec![], vec![]).unwrap();
        detached
            .add_unbuilt_region(loaded.known_region(RegionId(1)).unwrap().clone(), false)
            .unwrap();
        assert!(validation.validate(0, &detached).is_some());
        assert_eq!(
            work_counts().world_cells,
            64,
            "residency does not change membership"
        );
        assert!(validation.validate(0, &validation_world(63, 1)).is_none());
        assert!(validation
            .validate(0, &World::new(vec![], vec![]).unwrap())
            .is_none());
        let mut shifted = World::new(vec![], vec![]).unwrap();
        let mut bounds = serde_json::to_value(Extent::new(64, 3, 1).unwrap()).unwrap();
        bounds["origin"]["x"] = serde_json::json!(1);
        shifted
            .add_region(Region {
                id: RegionId(1),
                name: "shifted origin".into(),
                bounds: serde_json::from_value(bounds).unwrap(),
            })
            .unwrap();
        assert!(validation.validate(0, &shifted).is_none());
        assert!(validation.validate(0, &loaded).is_some());
    }

    #[test]
    fn restore_context_reuses_shared_buckets_across_distinct_roots_and_worlds() {
        use super::explored_map_regions::{reset_work_counts, work_counts};
        let roots = roots_with_unrelated_changes(4096, 32);
        let mut game = Game::new(
            validation_world(4096, 0),
            42,
            crate::test_creatures::content(),
        );
        let actor = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut shared = SharedState::default();
        let mut snapshots = Vec::new();
        for (label, map) in roots.into_iter().enumerate() {
            game.explored_maps.insert(actor, map);
            let mut snapshot = game.checkpoint(&mut shared);
            // Distinct immutable worlds with equal relevant bounds are safe to
            // reuse. Names and residency are not knowledge membership facts.
            shared.worlds.push(validation_world(4096, label + 1));
            snapshot.world = shared.worlds.len() - 1;
            snapshots.push(snapshot);
        }
        assert!(shared.explored_map_references_valid(snapshots.iter()));
        let bytes = serde_json::to_vec(&(snapshots, shared)).unwrap();
        let (snapshots, decoded): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        reset_work_counts();
        crate::reset_map_work_counts();
        let mut context = RestoreContext::new(&decoded, game.content().clone());
        for snapshot in snapshots {
            let restored = context.restore(snapshot).unwrap();
            assert!(restored
                .map_validation
                .loaded_matches(actor, &restored.explored_maps[&actor]));
        }
        let work = work_counts();
        assert_eq!(work.appearance_cells, 4096 + 32);
        assert_eq!(work.neighbor_rows, 4);
        assert_eq!(work.witness_rows, 2);
        assert_eq!(work.child_rows, 2);
        assert_eq!(work.world_cells, 4096 + 32);
        assert_eq!(crate::map_work_counts().validation_cells, 4096 + 32);
        assert_eq!(crate::map_work_counts().validation_relations, 8);
        assert_eq!(crate::map_work_counts().world_location_checks, 4096 + 32);
    }

    #[test]
    fn complete_checkpoint_reference_validation_rejects_unused_and_dangling_roots() {
        let mut game = Game::new(
            validation_world(64, 0),
            42,
            crate::test_creatures::content(),
        );
        let actor = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let roots = roots_with_unrelated_changes(64, 2);
        let mut shared = SharedState::default();
        game.explored_maps.insert(actor, roots[0].clone());
        let before = game.checkpoint(&mut shared);
        game.explored_maps.insert(actor, roots[1].clone());
        let after = game.checkpoint(&mut shared);
        assert!(shared.explored_map_references_valid([&before, &after]));
        assert!(!shared.explored_map_references_valid([&before]));
        let mut dangling = game.checkpoint(&mut shared);
        dangling.explored_maps = usize::MAX;
        assert!(!shared.explored_map_references_valid([&before, &after, &dangling]));
        dangling.explored_maps = shared.explored_map_owners.len();
        shared
            .explored_map_owners
            .push(BTreeMap::from([(actor, usize::MAX)]));
        assert!(!shared.explored_map_references_valid([&before, &after, &dangling]));
        shared.explored_map_owners.pop();
        // Every directory must be used, even when it adds no new map roots.
        shared.explored_map_owners.push(BTreeMap::new());
        assert!(!shared.explored_map_references_valid([&before, &after]));
        shared.explored_map_owners.pop();
        // Duplicate equal roots forged directly also fail before payload scans.
        shared.explored_maps.push(shared.explored_maps[0].clone());
        assert!(!shared.explored_map_references_valid([&before, &after]));
        let mut forged = game.checkpoint(&mut shared);
        forged.explored_maps = shared.explored_map_owners.len();
        shared
            .explored_map_owners
            .push(BTreeMap::from([(actor, shared.explored_maps.len() - 1)]));
        super::explored_map_regions::reset_work_counts();
        let mut context = RestoreContext::new(&shared, game.content().clone());
        assert!(context.restore(forged).is_err());
        assert_eq!(
            super::explored_map_regions::work_counts().appearance_cells,
            0
        );
    }

    #[test]
    fn explored_map_region_pool_restores_exact_stale_values_and_sharing() {
        let before = Shared::new(map());
        let mut after = before.clone();
        after.cells.insert(at(1, 1), appearance("newly seen wood"));
        let original = Maps {
            maps: vec![before.clone(), after.clone()],
        };
        let bytes = serde_json::to_vec(&original).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["maps"]["cells"].as_array().unwrap().len(), 3);
        assert_eq!(value["maps"]["neighbors"].as_array().unwrap().len(), 1);
        assert_eq!(value["maps"]["witnesses"].as_array().unwrap().len(), 1);
        assert_eq!(value["maps"]["children"].as_array().unwrap().len(), 1);
        let mut decoded: Maps = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(decoded.maps, original.maps);
        assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
        assert!(std::ptr::eq(
            decoded.maps[0].cells.region(RegionId(2)).unwrap(),
            decoded.maps[1].cells.region(RegionId(2)).unwrap(),
        ));
        assert!(std::ptr::eq(
            decoded.maps[0].neighbors.region(RegionId(1)).unwrap(),
            decoded.maps[1].neighbors.region(RegionId(1)).unwrap(),
        ));
        assert!(std::ptr::eq(
            decoded.maps[0].witnesses.region(RegionId(2)).unwrap(),
            decoded.maps[1].witnesses.region(RegionId(2)).unwrap(),
        ));
        assert!(std::ptr::eq(
            decoded.maps[0].children.region(RegionId(1)).unwrap(),
            decoded.maps[1].children.region(RegionId(1)).unwrap(),
        ));
        decoded.maps[0]
            .cells
            .insert(at(2, 1), appearance("changed after restore"));
        assert_eq!(decoded.maps[1], after);
        assert_eq!(
            decoded.maps[1].cells.get(&at(2, 1)).unwrap().material,
            "remembered stone"
        );
    }

    #[test]
    fn compact_palette_aliases_validate_large_appearance_payloads_once() {
        use super::explored_map_regions::{reset_work_counts, work_counts};
        const CELLS: i32 = 2048;
        let mut visual = appearance("large stale item stack");
        visual.ground_items = vec![visual.ground_items[0].clone(); 4097];
        let mut before = ExploredMap::default();
        for x in 0..CELLS {
            before.cells.insert(at(1, x), visual.clone());
        }
        let mut after = before.clone();
        let mut changed = visual.clone();
        changed.material = "later stale item stack".into();
        after.cells.insert(at(1, CELLS - 1), changed);
        let expected = vec![Shared::new(before), Shared::new(after)];
        let bytes = serde_json::to_vec(&Maps {
            maps: expected.clone(),
        })
        .unwrap();
        reset_work_counts();
        let restored: Maps = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(work_counts().appearance_cells, 2 * CELLS as usize);
        assert_eq!(
            work_counts().appearance_payloads,
            2,
            "cell aliases must not multiply scans of unbounded legal item vectors"
        );
        for (restored, expected) in restored.maps.iter().zip(&expected) {
            assert_eq!(
                restored.cells.keys().collect::<Vec<_>>(),
                expected.cells.keys().collect::<Vec<_>>()
            );
            for at in [at(1, 0), at(1, CELLS - 1)] {
                assert_eq!(restored.cells.get(&at), expected.cells.get(&at));
            }
            let first = restored.cells.get(&at(1, 0)).unwrap();
            assert!(restored
                .cells
                .iter()
                .take((CELLS - 1) as usize)
                .all(|(_, appearance)| appearance.shares_storage(first)));
        }
        assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
    }

    #[test]
    fn compact_map_palette_columns_and_neighbor_masks_round_trip_exactly() {
        let mut original = map();
        let center = Location {
            region: RegionId(1),
            position: Position { x: 5, y: 5, z: 5 },
        };
        original
            .cells
            .insert(center, appearance("stale painted stone"));
        for direction in [
            Direction::North,
            Direction::East,
            Direction::South,
            Direction::West,
            Direction::Up,
            Direction::Down,
        ] {
            let (x, y, z) = direction.delta();
            let target = Location {
                region: center.region,
                position: Position {
                    x: center.position.x + x,
                    y: center.position.y + y,
                    z: center.position.z + z,
                },
            };
            original
                .cells
                .insert(target, appearance("stale painted stone"));
            original.neighbors.insert(
                (center, direction),
                Neighbor::Placed {
                    target,
                    rotation: 0,
                },
            );
        }
        original.neighbors.insert(
            (at(1, 2), Direction::North),
            Neighbor::Placed {
                target: at(2, 1),
                rotation: 0,
            },
        );
        original.neighbors.insert(
            (at(1, 2), Direction::East),
            Neighbor::Placed {
                target: at(1, 1),
                rotation: 7,
            },
        );
        original
            .neighbors
            .insert((at(1, 2), Direction::Up), Neighbor::Ambiguous);
        let expected = vec![Shared::new(original)];
        let bytes = serde_json::to_vec(&Maps {
            maps: expected.clone(),
        })
        .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["maps"]["palette"].as_array().unwrap().len(), 2);
        let plain = value["maps"]["neighbors"][0]["plain"].as_array().unwrap();
        assert!(plain.contains(&serde_json::json!([5, 5, [[5, 63]]])));
        assert_eq!(
            value["maps"]["neighbors"][0]["other"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        let restored: Maps = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.maps, expected);
        assert_eq!(serde_json::to_vec(&restored).unwrap(), bytes);
        assert!(restored.maps[0].cells.get(&center).unwrap().shares_storage(
            restored.maps[0]
                .cells
                .get(&Location {
                    region: center.region,
                    position: Position {
                        z: 6,
                        ..center.position
                    },
                })
                .unwrap()
        ));
        // Existing witnesses/children remain explicit, with their exact stale values.
        assert_eq!(restored.maps[0].witnesses, expected[0].witnesses);
        assert_eq!(restored.maps[0].children, expected[0].children);
    }

    #[test]
    fn compact_map_codec_rejects_noncanonical_palette_columns_and_masks() {
        let good = serde_json::to_value(Maps {
            maps: vec![Shared::new(map())],
        })
        .unwrap();
        let reject = |value| assert!(serde_json::from_value::<Maps>(value).is_err());
        let mut bad = good.clone();
        let duplicate = bad["maps"]["palette"][0].clone();
        bad["maps"]["palette"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        reject(bad);
        let mut bad = good.clone();
        bad["maps"]["palette"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::to_value(appearance("zz unused")).unwrap());
        reject(bad);
        let mut two = map();
        two.cells
            .insert(at(1, 1), appearance("another stale value"));
        let mut bad = serde_json::to_value(Maps {
            maps: vec![Shared::new(two)],
        })
        .unwrap();
        bad["maps"]["palette"].as_array_mut().unwrap().reverse();
        reject(bad);
        let mut bad = good.clone();
        bad["maps"]["cells"][0]["cells"][0][2][0][1] = serde_json::json!(999);
        reject(bad);
        for columns in [
            serde_json::json!([]),
            serde_json::json!([[1, 1, []]]),
            serde_json::json!([[1, 1, [[0, 0], [0, 0]]]]),
            serde_json::json!([[1, 1, [[1, 0], [0, 0]]]]),
            serde_json::json!([[1, 1, [[0, 0]]], [1, 1, [[1, 0]]]]),
            serde_json::json!([[2, 1, [[0, 0]]], [1, 1, [[0, 0]]]]),
        ] {
            let mut bad = good.clone();
            bad["maps"]["cells"][0]["cells"] = columns;
            reject(bad);
        }
        for columns in [
            serde_json::json!([[1, 1, []]]),
            serde_json::json!([[1, 1, [[0, 0]]]]),
            serde_json::json!([[1, 1, [[0, 64]]]]),
            serde_json::json!([[1, 1, [[0, -1]]]]),
            serde_json::json!([[1, 1, [[0, 2], [0, 2]]]]),
            serde_json::json!([[1, 1, [[1, 2], [0, 2]]]]),
            serde_json::json!([[2, 1, [[0, 2]]], [1, 1, [[0, 2]]]]),
            serde_json::json!([[i32::MAX, 1, [[0, 2]]]]),
            serde_json::json!([[i32::MIN, 1, [[0, 8]]]]),
            serde_json::json!([[1, i32::MAX, [[0, 4]]]]),
            serde_json::json!([[1, i32::MIN, [[0, 1]]]]),
            serde_json::json!([[1, 1, [[i32::MAX, 16]]]]),
            serde_json::json!([[1, 1, [[i32::MIN, 32]]]]),
        ] {
            let mut bad = good.clone();
            bad["maps"]["neighbors"][0]["plain"] = columns;
            reject(bad);
        }
    }

    #[test]
    fn compact_map_codec_rejects_noncanonical_exceptions_and_unnamed_records() {
        let good = serde_json::to_value(Maps {
            maps: vec![Shared::new(map())],
        })
        .unwrap();
        let reject = |value| assert!(serde_json::from_value::<Maps>(value).is_err());
        let placed = serde_json::json!([[at(1, 1), "East"],
            {"kind":"placed", "target":at(1, 2), "rotation":0}]);
        let ambiguous = serde_json::json!([[at(1, 1), "East"], {"kind":"ambiguous"}]);
        let later = serde_json::json!([[at(1, 2), "East"], {"kind":"ambiguous"}]);
        for (plain, other) in [
            (serde_json::json!([]), serde_json::json!([placed])),
            (
                good["maps"]["neighbors"][0]["plain"].clone(),
                serde_json::json!([ambiguous.clone()]),
            ),
            (
                serde_json::json!([]),
                serde_json::json!([ambiguous.clone(), ambiguous.clone()]),
            ),
            (serde_json::json!([]), serde_json::json!([later, ambiguous])),
            (
                serde_json::json!([]),
                serde_json::json!([[[at(2, 1), "East"], {"kind":"ambiguous"}]]),
            ),
            (
                serde_json::json!([]),
                serde_json::json!([[[at(1, 1), "NorthEast"], {"kind":"ambiguous"}]]),
            ),
            (serde_json::json!([]), serde_json::json!([])),
        ] {
            let mut bad = good.clone();
            bad["maps"]["neighbors"][0]["plain"] = plain;
            bad["maps"]["neighbors"][0]["other"] = other;
            reject(bad);
        }
        for (path, fields) in [
            (
                "/maps",
                vec![
                    "palette",
                    "cells",
                    "neighbors",
                    "witnesses",
                    "children",
                    "instances",
                ],
            ),
            ("/maps/cells/0", vec!["region", "cells"]),
            ("/maps/neighbors/0", vec!["region", "plain", "other"]),
        ] {
            let mut bad = good.clone();
            *bad.pointer_mut(path).unwrap() = serde_json::json!([]);
            reject(bad);
            let mut bad = good.clone();
            bad.pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), serde_json::json!(0));
            reject(bad);
            for field in fields {
                let mut bad = good.clone();
                bad.pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                reject(bad);
            }
        }
        let mut legacy = good;
        legacy["maps"]["cells"] = serde_json::to_value(vec![map().cells]).unwrap();
        reject(legacy);
    }

    #[test]
    fn explored_map_pool_rejects_invalid_order_regions_references_and_missing_fields() {
        let good = serde_json::to_value(Maps {
            maps: vec![Shared::new(map())],
        })
        .unwrap();
        let reject = |value: serde_json::Value| {
            assert!(serde_json::from_value::<Maps>(value).is_err());
        };
        let mut bad = good.clone();
        bad["maps"]["cells"][0]["cells"]
            .as_array_mut()
            .unwrap()
            .reverse();
        reject(bad);
        let mut bad = good.clone();
        let duplicate = bad["maps"]["cells"][0]["cells"][0].clone();
        bad["maps"]["cells"][0]["cells"]
            .as_array_mut()
            .unwrap()
            .insert(0, duplicate);
        reject(bad);
        let mut bad = good.clone();
        // A region-once bucket cannot smuggle foreign locations in its rows;
        // changing its region must still reject duplicate region ownership.
        bad["maps"]["cells"][0]["region"] = serde_json::json!(2);
        reject(bad);
        let mut bad = good.clone();
        bad["maps"]["instances"][0]["cells"] = serde_json::json!([1, 0]);
        reject(bad);
        let mut bad = good.clone();
        bad["maps"]["instances"][0]["cells"] = serde_json::json!([0, 0]);
        reject(bad);
        let mut bad = good.clone();
        bad["maps"]["instances"][0]["children"] = serde_json::json!([999]);
        reject(bad);
        let mut bad = good.clone();
        bad["maps"]["instances"][0] = serde_json::json!([[0, 1], [0], [0], [0]]);
        reject(bad);
        let mut bad = good.clone();
        bad["maps"]["instances"][0]
            .as_object_mut()
            .unwrap()
            .remove("witnesses");
        reject(bad);
        let mut bad = good.clone();
        bad["maps"].as_object_mut().unwrap().remove("children");
        reject(bad);
        let mut bad = good;
        let unused = bad["maps"]["cells"][0].clone();
        bad["maps"]["cells"].as_array_mut().unwrap().push(unused);
        reject(bad);
    }

    #[test]
    fn explored_map_pool_rejects_invalid_appearance_frames_and_reverse_evidence() {
        let reject = |map: ExploredMap| {
            let bytes = serde_json::to_vec(&Maps {
                maps: vec![Shared::new(map)],
            })
            .unwrap();
            assert!(serde_json::from_slice::<Maps>(&bytes).is_err());
        };
        let mut bad = map();
        bad.cells.get_mut(&at(1, 1)).unwrap().ground_items[0].quantity = 0;
        reject(bad);
        let mut bad = map();
        bad.cells.get_mut(&at(1, 1)).unwrap().material = "x".repeat(4097);
        reject(bad);
        let mut bad = map();
        bad.neighbors.insert(
            (at(1, 1), Direction::East),
            Neighbor::Placed {
                target: at(1, 2),
                rotation: 24,
            },
        );
        reject(bad);
        let mut bad = map();
        bad.neighbors.insert(
            (at(1, 1), Direction::East),
            Neighbor::Placed {
                target: at(9, 1),
                rotation: 0,
            },
        );
        reject(bad);
        let mut bad = map();
        bad.witnesses.insert(
            at(2, 1),
            Witness::Placed {
                anchor: at(1, 1),
                delta: [10, 0, 0],
                rotation: 24,
            },
        );
        bad.children.insert((at(1, 1), [10, 0, 0], at(2, 1)), 24);
        reject(bad);
        let mut bad = map();
        bad.children.remove(&(at(1, 1), [10, 0, 0], at(2, 1)));
        reject(bad);
        let mut bad = map();
        bad.children.insert((at(1, 1), [11, 0, 0], at(2, 1)), 0);
        reject(bad);
        let mut bad = map();
        bad.witnesses.insert(at(2, 1), Witness::Ambiguous);
        reject(bad);
        let mut bad = map();
        bad.children.remove(&(at(1, 1), [10, 0, 0], at(2, 1)));
        bad.witnesses.insert(
            at(2, 1),
            Witness::Placed {
                anchor: at(1, 1),
                delta: [i64::MIN, 0, 0],
                rotation: 0,
            },
        );
        bad.children
            .insert((at(1, 1), [i64::MIN, 0, 0], at(2, 1)), 0);
        reject(bad);
    }

    #[test]
    fn explored_appearance_decoding_requires_named_complete_bounded_values() {
        let good = serde_json::to_value(appearance("remembered")).unwrap();
        assert!(serde_json::from_value::<MapAppearance>(good.clone()).is_ok());
        let reject = |value: serde_json::Value| {
            assert!(serde_json::from_value::<MapAppearance>(value).is_err());
        };
        reject(serde_json::json!([
            true,
            "stone",
            null,
            true,
            true,
            false,
            null,
            []
        ]));
        for field in ["asset", "door", "ground_items"] {
            let mut bad = good.clone();
            bad.as_object_mut().unwrap().remove(field);
            reject(bad);
        }
        for path in ["door", "ground_items"] {
            let mut bad = good.clone();
            let nested = if path == "door" {
                &mut bad["door"]
            } else {
                &mut bad["ground_items"][0]
            };
            nested.as_object_mut().unwrap().remove("asset");
            reject(bad);
        }
        let mut bad = good.clone();
        bad["door"] = serde_json::json!(["gate", "closed", false, null]);
        reject(bad);
        let mut bad = good.clone();
        bad["ground_items"][0] =
            serde_json::json!(["potion", "blue", "potion", "blue", false, 3, null]);
        reject(bad);
        let mut bad = good.clone();
        bad["door"]["action_id"] = serde_json::json!(99);
        reject(bad);
        let mut bad = good.clone();
        bad["ground_items"][0]["item_id"] = serde_json::json!(99);
        reject(bad);
        let mut bad = good.clone();
        bad["material"] = serde_json::json!("tab\tcharacter");
        reject(bad);
        let mut bad = good.clone();
        bad["ground_items"] = serde_json::Value::Array(vec![good["ground_items"][0].clone(); 4097]);
        assert!(
            serde_json::from_value::<MapAppearance>(bad).is_ok(),
            "presentation limits cannot impose a new ground-stack gameplay limit"
        );
        let mut maximum = good;
        maximum["material"] = serde_json::json!("x".repeat(4096));
        assert!(serde_json::from_value::<MapAppearance>(maximum).is_ok());
    }

    #[test]
    fn direct_explored_map_decode_checks_named_fields_and_reverse_evidence() {
        let good = serde_json::to_value(map()).unwrap();
        assert_eq!(
            serde_json::from_value::<ExploredMap>(good.clone()).unwrap(),
            map()
        );
        assert!(
            serde_json::from_value::<ExploredMap>(serde_json::json!([[], [], [], []])).is_err()
        );
        let mut missing = good.clone();
        missing.as_object_mut().unwrap().remove("children");
        assert!(serde_json::from_value::<ExploredMap>(missing).is_err());
        let mut mismatch = good;
        mismatch["children"] = serde_json::json!([]);
        assert!(serde_json::from_value::<ExploredMap>(mismatch).is_err());
    }

    #[test]
    fn explored_map_restore_validation_is_scoped_to_world_and_actor_membership() {
        let world = |width| {
            let mut world = World::new(vec![], vec![]).unwrap();
            world
                .add_region(Region {
                    id: RegionId(1),
                    name: "memory".into(),
                    bounds: Extent::new(width, 3, 1).unwrap(),
                })
                .unwrap();
            world
        };
        let mut game = Game::new(world(12), 42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut map = ExploredMap::default();
        map.cells.insert(at(1, 9), appearance("stale wall"));
        game.explored_maps.insert(actor, Shared::new(map));
        let mut shared = SharedState::default();
        let valid = game.checkpoint(&mut shared);
        let mut wrong_membership = game.checkpoint(&mut shared);
        shared.actors.push(BTreeMap::new());
        wrong_membership.actors = shared.actors.len() - 1;
        assert!(!shared.explored_map_references_valid([&wrong_membership]));
        let mut zero_actor = game.checkpoint(&mut shared);
        let record = shared.actors[zero_actor.actors][&actor].clone();
        shared.actors.push(BTreeMap::from([(ActorId(0), record)]));
        zero_actor.actors = shared.actors.len() - 1;
        zero_actor.explored_maps = shared.explored_map_owners.len();
        shared
            .explored_map_owners
            .push(BTreeMap::from([(ActorId(0), 0)]));
        assert!(!shared.explored_map_references_valid([&zero_actor]));
        let mut wrong_world = game.checkpoint(&mut shared);
        shared.worlds.push(world(3));
        wrong_world.world = shared.worlds.len() - 1;
        let mut wrong_actor = game.checkpoint(&mut shared);
        let index = shared.explored_map_owners[wrong_actor.explored_maps][&actor];
        wrong_actor.explored_maps = shared.explored_map_owners.len();
        shared
            .explored_map_owners
            .push(BTreeMap::from([(ActorId(999), index)]));
        assert!(!shared.explored_map_references_valid([&valid, &wrong_actor]));
        let mut wrong_index = game.checkpoint(&mut shared);
        wrong_index.explored_maps = shared.explored_map_owners.len();
        shared
            .explored_map_owners
            .push(BTreeMap::from([(actor, usize::MAX)]));
        let mut missing_directory = game.checkpoint(&mut shared);
        missing_directory.explored_maps = usize::MAX;
        assert!(!shared.explored_map_references_valid([&wrong_actor]));
        assert!(!shared.explored_map_references_valid([&wrong_index]));
        assert!(!shared.explored_map_references_valid([&missing_directory]));
        let mut context = RestoreContext::new(&shared, game.content().clone());
        super::explored_map_regions::reset_work_counts();
        assert_eq!(context.restore(zero_actor), Err(PersistError::ExploredMap));
        assert_eq!(
            super::explored_map_regions::work_counts().appearance_cells,
            0
        );
        let restored = context.restore(valid).unwrap();
        assert_eq!(restored, game);
        assert!(restored.explored_maps[&actor].shares_storage(&game.explored_maps[&actor]));
        assert_eq!(
            context.restore(wrong_membership),
            Err(PersistError::ExploredMap)
        );
        assert_eq!(context.restore(wrong_world), Err(PersistError::ExploredMap));
        assert_eq!(context.restore(wrong_actor), Err(PersistError::ExploredMap));
        assert_eq!(context.restore(wrong_index), Err(PersistError::ExploredMap));
        assert_eq!(
            context.restore(missing_directory),
            Err(PersistError::ExploredMap)
        );
    }

    #[test]
    fn checkpoints_require_explored_maps_and_preserve_more_than_one_viewport() {
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(Region {
                id: RegionId(1),
                name: "large history".into(),
                bounds: Extent::new(5002, 3, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, crate::test_creatures::content());
        let actor = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut map = ExploredMap::default();
        for x in 0..5000 {
            map.cells.insert(at(1, x), appearance("last seen"));
        }
        game.explored_maps.insert(actor, Shared::new(map));
        let original = game.clone();
        assert!(game.explored_maps[&actor].shares_storage(&original.explored_maps[&actor]));
        let mut shared = SharedState::default();
        let before = game.checkpoint(&mut shared);
        let unchanged = game.checkpoint(&mut shared);
        assert_eq!(shared.explored_maps.len(), 1);
        assert_eq!(shared.explored_map_owners.len(), 1);
        assert_eq!(before.explored_maps, unchanged.explored_maps);
        let bytes = serde_json::to_vec(&(vec![before, unchanged], &shared)).unwrap();
        let (snapshots, decoded): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&decoded, game.content().clone());
        let restored: Vec<_> = snapshots
            .into_iter()
            .map(|s| context.restore(s).unwrap())
            .collect();
        assert_eq!(restored[0], original);
        assert_eq!(restored[0].explored_maps[&actor].cells.iter().count(), 5000);
        assert!(
            restored[0].explored_maps[&actor].shares_storage(&restored[1].explored_maps[&actor])
        );
        let snapshot = game.checkpoint(&mut shared);
        let mut value = serde_json::to_value(snapshot).unwrap();
        assert!(value["explored_maps"].is_u64());
        for invalid in [
            serde_json::json!([]),
            serde_json::json!([[actor, 0]]),
            serde_json::Value::Null,
        ] {
            let mut legacy = value.clone();
            legacy["explored_maps"] = invalid;
            assert!(serde_json::from_value::<Snapshot>(legacy).is_err());
        }
        value.as_object_mut().unwrap().remove("explored_maps");
        assert!(serde_json::from_value::<Snapshot>(value).is_err());
        let value = serde_json::to_value(shared).unwrap();
        for field in ["explored_maps", "explored_map_owners"] {
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove(field);
            assert!(serde_json::from_value::<SharedState>(missing).is_err());
        }
        for invalid in [
            serde_json::Value::Null,
            serde_json::json!([[[0, 0]]]),
            serde_json::json!([[[1, 0], [1, 0]]]),
            serde_json::json!([[[1, 0], [1, 1]]]),
            serde_json::json!([[[2, 0], [1, 0]]]),
            serde_json::json!([[[1, 0]], [[1, 0]]]),
            serde_json::json!([[], []]),
        ] {
            let mut malformed = value.clone();
            malformed["explored_map_owners"] = invalid;
            assert!(serde_json::from_value::<SharedState>(malformed).is_err());
        }
    }

    #[test]
    fn checkpoints_pool_complete_owner_directories_without_merging_actor_knowledge() {
        let mut game = Game::new(
            validation_world(64, 0),
            42,
            crate::test_creatures::content(),
        );
        let first = game
            .spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let second = game
            .spawn_actor(at(1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        let roots = roots_with_unrelated_changes(64, 2);
        game.explored_maps.insert(first, roots[0].clone());
        game.explored_maps.insert(second, roots[1].clone());
        let original = game.clone();
        let mut shared = SharedState::default();
        let before = game.checkpoint(&mut shared);
        let unchanged = game.checkpoint(&mut shared);
        assert_eq!(before.explored_maps, unchanged.explored_maps);
        assert_eq!(shared.explored_map_owners.len(), 1);
        assert_eq!(shared.explored_map_owners[0].len(), 2);
        game.explored_maps.insert(second, roots[0].clone());
        let changed = game.checkpoint(&mut shared);
        assert_ne!(before.explored_maps, changed.explored_maps);
        assert_eq!(shared.explored_map_owners.len(), 2);
        assert_eq!(shared.explored_maps.len(), 2);
        assert!(shared.explored_map_references_valid([&before, &unchanged, &changed]));
        // The changed state still uses root zero, but cannot hide either the
        // unreferenced earlier directory or its distinct second actor's root.
        assert!(!shared.explored_map_references_valid([&changed]));
        let bytes = serde_json::to_vec(&(vec![before, unchanged, changed], &shared)).unwrap();
        let (snapshots, decoded): (Vec<Snapshot>, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        let mut context = RestoreContext::new(&decoded, game.content().clone());
        let restored: Vec<_> = snapshots
            .into_iter()
            .map(|snapshot| context.restore(snapshot).unwrap())
            .collect();
        assert_eq!(restored[0], original);
        assert_eq!(restored[1], original);
        assert_eq!(restored[2], game);
        assert!(
            restored[0].explored_maps[&first].shares_storage(&restored[1].explored_maps[&first])
        );
        assert!(
            restored[0].explored_maps[&first].shares_storage(&restored[2].explored_maps[&second])
        );
        assert_ne!(
            restored[0].explored_maps[&first],
            restored[0].explored_maps[&second]
        );
    }

    #[test]
    fn checkpoints_require_unique_referenced_owner_directories_even_when_empty() {
        let mut game = Game::new(
            validation_world(64, 0),
            42,
            crate::test_creatures::content(),
        );
        game.spawn_actor(at(1, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let mut shared = SharedState::default();
        let snapshot = game.checkpoint(&mut shared);
        assert_eq!(snapshot.explored_maps, 0);
        assert_eq!(shared.explored_map_owners, vec![BTreeMap::new()]);
        assert!(shared.explored_maps.is_empty());
        assert!(shared.explored_map_references_valid([&snapshot]));
        let bytes = serde_json::to_vec(&(&snapshot, &shared)).unwrap();
        let (decoded_snapshot, decoded_shared): (Snapshot, SharedState) =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            Game::restore_checkpoint(decoded_snapshot, &decoded_shared, game.content().clone())
                .unwrap(),
            game
        );
        shared.explored_map_owners.push(BTreeMap::new());
        assert!(!shared.explored_map_references_valid([&snapshot]));
        assert_eq!(
            Game::restore_checkpoint(snapshot, &shared, game.content().clone()),
            Err(PersistError::ExploredMap)
        );
    }
}
