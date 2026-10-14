//! Backend-only deterministic checkpoint state. Equal world and navigation
//! regions, actor tables, intentions and population states are encoded once
//! across rewind boundaries.
//! This module does not perform storage or I/O.
use crate::actor_record::ActorRecord;
use crate::creatures::Content;
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
    events: Vec<crate::combat::CombatEvent>,
    physics: crate::physics::Physics,
    world: usize,
    navigation: BTreeMap<ActorId, usize>,
    seed: u64,
    tick: u64,
    actors: usize,
    items: usize,
    next_actor_id: u64,
    next_item_id: u64,
    next_door_id: u64,
    intentions: crate::intention::checkpoint::Snapshot,
    /// Region streaming state in [`SharedState`], omitted while empty so
    /// games that never stream save exactly as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lifecycle: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    scenario_policy: Option<crate::floor_activation::ScenarioPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    floor_activation: Option<usize>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SharedState {
    #[serde(with = "tor_world::checkpoint_worlds")]
    worlds: Vec<World>,
    #[serde(with = "navigation_regions")]
    navigation: Vec<Navigation>,
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
}

impl SharedState {
    pub fn valid_world_count(&self, maximum: usize) -> bool {
        !self.worlds.is_empty() && self.worlds.len() <= maximum
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
            !self.population_entry_pending() && !self.host_region_continuation,
            "a private host continuation must settle before checkpoint capture"
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
        Snapshot {
            combat: self.combat.clone(),
            rules_rng: self.rules_rng,
            events: self.event_log.clone(),
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
            combat_diagnostics: None,
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
            seed: snapshot.seed,
            tick: snapshot.tick,
            actors: context.actors(snapshot.actors, snapshot.items)?,
            items,
            next_actor_id: snapshot.next_actor_id,
            next_item_id: snapshot.next_item_id,
            next_door_id: snapshot.next_door_id,
            intentions: context
                .intentions
                .restore(snapshot.intentions, &context.shared.intentions)
                .ok_or(PersistError::Intentions)?,
            scenario_policy: snapshot.scenario_policy.unwrap_or_default(),
            floor_activation: snapshot
                .floor_activation
                .map(|index| context.floor_activation(index))
                .transpose()?,
            population_entry_boundary: None,
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
        if !game.combat_valid() {
            return Err(PersistError::RunState);
        }
        if (game.actors.is_empty() && !game.has_detached_actors())
            || game.next_actor_id == 0
            || game.next_item_id == 0
            || game.next_door_id == 0
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
        if !game.world.checkpoint_valid(game.next_door_id) {
            return Err(PersistError::World);
        }
        if !game.recovery_contexts_valid() {
            return Err(PersistError::Schedule);
        }
        game.remember_recovery_contexts();
        Ok(game)
    }

    /// Checks restoring gives a loaded actor, apart from its body's room;
    /// attaching a stored region gives its actors the same.
    pub(crate) fn actor_state_valid(&self, id: ActorId, actor: &Actor) -> bool {
        id.0 != 0
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
                    && p.schedule_valid(self.actor_clock(id), actor.ready_at)
                    && (!p.active
                        || actor.combat.as_ref().is_none_or(|combat| {
                            !combat
                                .creature()
                                .condition_pauses_preparation(&p.plan.work().action())
                        }))
                    && p.plan.target().0 < self.next_actor_id
                    && (!(p.plan.as_ability().is_some()
                        || matches!(p.plan, crate::abilities::Plan::Respec(_)))
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
        id.0 != 0
            && id.0 < self.next_item_id
            && item.quantity != 0
            && item.holds_valid()
            && (item.spec.stackable || item.quantity == 1)
            && item.spec.valid()
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
            effects: vec![Effect::ApplyCondition {
                condition: ConditionId::new("missing_condition").unwrap(),
                duration: 100,
            }],
        });
        let mut attack = crate::ItemSpec::ordinary("weapon".into());
        attack.class = crate::ItemClass::Weapon;
        attack.equipment = Some(crate::EquipmentSpec {
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

    type Edges = RegionMap<(Location, Direction), (Location, u8)>;
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
        other: Vec<((Location, Direction), (Location, u8))>,
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
            for (&(from, direction), &(to, turns)) in map.iter() {
                region = Some(from.region);
                match DIRECTIONS.iter().position(|d| *d == direction) {
                    Some(bit) if is_plain(from, direction, to, turns) => {
                        *plain.entry(from.position).or_default() |= 1 << bit;
                    }
                    _ => other.push(((from, direction), (to, turns))),
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
                if !(1..64).contains(&mask) {
                    return None;
                }
                let from = Location {
                    region: self.region,
                    position,
                };
                for (bit, direction) in DIRECTIONS.into_iter().enumerate() {
                    if mask & (1 << bit) != 0 {
                        map.insert((from, direction), (plain_target(from, direction)?, 0));
                        count += 1;
                    }
                }
            }
            for ((from, direction), (to, turns)) in self.other {
                if from.region != self.region
                    || (DIRECTIONS.contains(&direction) && is_plain(from, direction, to, turns))
                    || map.contains_key(&(from, direction))
                {
                    return None;
                }
                map.insert((from, direction), (to, turns));
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
            map.insert((at(1, 0, 0, 0), Direction::East), (at(1, 1, 0, 0), 0));
            map.insert((at(1, 0, 0, 0), Direction::South), (at(1, 0, 1, 0), 0));
            map.insert((at(1, 0, 0, 1), Direction::Down), (at(1, 0, 0, 0), 0));
            // A portal, a rotated join, and a stair are stored in full.
            map.insert((at(1, 4, 0, 0), Direction::East), (at(2, 0, 0, 0), 0));
            map.insert((at(1, 0, 3, 0), Direction::West), (at(1, -1, 3, 0), 1));
            map.insert((at(1, 2, 2, 0), Direction::Up), (at(3, 2, 2, 0), 0));
            let encoded = EdgeRegion::encode(&map);
            assert_eq!(
                encoded.plain,
                vec![(0, 0, vec![[0, 0b10 | 0b100], [1, 0b10_0000]])]
            );
            assert_eq!(encoded.other.len(), 3);
            assert_eq!(encoded.decode(), Some(map));
        }

        #[test]
        fn non_canonical_or_malformed_links_are_rejected() {
            let plain = ((at(1, 0, 0, 0), Direction::East), (at(1, 1, 0, 0), 0));
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
            let foreign = ((at(2, 4, 0, 0), Direction::East), (at(3, 0, 0, 0), 0));
            assert_eq!(decode(vec![], vec![foreign]), None);
        }
    }
}
