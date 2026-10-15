//! One branch-local ordinary item, retrieved on real floor entry and escaped
//! through an explicit terminal action. Population remains independently pooled.
use crate::floor_activation::EntryCause;
use crate::{ActorId, Game, GameError, ItemId, ItemSpec};
use serde::{Deserialize, Serialize};
use tor_world::{Direction, Location, RegionId, Shared};

pub const RETRIEVAL_VERSION: u32 = 1;
pub const MAX_RETRIEVAL_ENTRIES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TerminalExit {
    #[serde(deserialize_with = "crate::bounded::location")]
    pub at: Location,
    pub direction: Direction,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UniqueRetrievalPolicy {
    pub version: u32,
    pub protagonist: ActorId,
    pub family: String,
    pub minimum_depth: u32,
    pub item: ItemSpec,
    pub exit: TerminalExit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalSource {
    Ground(#[serde(deserialize_with = "crate::bounded::location")] Location),
    Carried(ActorId),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RetrievalResult {
    Created {
        item: ItemId,
        #[serde(deserialize_with = "crate::bounded::location")]
        at: Location,
        placement: u64,
    },
    Relocated {
        item: ItemId,
        from: RetrievalSource,
        #[serde(deserialize_with = "crate::bounded::location")]
        at: Location,
        placement: u64,
    },
    AlreadyCarried {
        item: ItemId,
    },
}
impl RetrievalResult {
    pub fn item(&self) -> ItemId {
        match self {
            Self::Created { item, .. }
            | Self::Relocated { item, .. }
            | Self::AlreadyCarried { item } => *item,
        }
    }
    pub fn item_birth(&self) -> Option<ItemId> {
        match self {
            Self::Created { item, .. } => Some(*item),
            _ => None,
        }
    }
    pub fn placement(&self) -> Option<u64> {
        match self {
            Self::Created { placement, .. } | Self::Relocated { placement, .. } => Some(*placement),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetrievalEntry {
    pub scope: String,
    pub depth: u32,
    pub entrant: ActorId,
    pub tick: u64,
    pub ordinal: u64,
    pub cause: EntryCause,
    pub result: RetrievalResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UniqueRetrievalState {
    pub policy: Shared<UniqueRetrievalPolicy>,
    pub initialized: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")]
    pub item: Option<ItemId>,
    pub entry_count: u64,
    pub placement_count: u64,
    #[serde(deserialize_with = "read_entries")]
    pub entries: Vec<RetrievalEntry>,
}
pub(crate) fn read_entries<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Vec<RetrievalEntry>, D::Error> {
    crate::bounded::Bounded::<RetrievalEntry, MAX_RETRIEVAL_ENTRIES>::deserialize(d).map(|v| v.0)
}

macro_rules! named_record {
    ($name:ident { $( $(#[$attribute:meta])* $field:ident: $ty:ty ),* $(,)? }) => {
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Fields { $( $(#[$attribute])* $field: $ty, )* }
                let fields: Fields = crate::bounded::named_map(d)?;
                Ok(Self { $( $field: fields.$field, )* })
            }
        }
    };
}
named_record!(TerminalExit {
    #[serde(deserialize_with = "crate::bounded::location")]
    at: Location,
    direction: Direction,
});
named_record!(UniqueRetrievalPolicy {
    version: u32,
    protagonist: ActorId,
    family: String,
    minimum_depth: u32,
    item: ItemSpec,
    exit: TerminalExit,
});
named_record!(RetrievalEntry {
    scope: String,
    depth: u32,
    entrant: ActorId,
    tick: u64,
    ordinal: u64,
    cause: EntryCause,
    result: RetrievalResult,
});
named_record!(UniqueRetrievalState {
    policy: Shared<UniqueRetrievalPolicy>, initialized: bool,
    #[serde(deserialize_with = "crate::bounded::required_option")] item: Option<ItemId>,
    entry_count: u64, placement_count: u64,
    #[serde(deserialize_with = "read_entries")] entries: Vec<RetrievalEntry>,
});

impl UniqueRetrievalPolicy {
    fn valid(&self, game: &Game) -> bool {
        self.version == RETRIEVAL_VERSION
            && self.protagonist.0 > 0
            && !self.family.is_empty()
            && self.family.len() <= 128
            && !self.family.chars().any(|c| c.is_control() || c == '/')
            && self.minimum_depth > 0
            && self.exit.direction == Direction::Up
            && self.exit.at.region.0 > 0
            && self.exit.at.position.z == 0
            && self.item.valid()
            && self.item.validate_rules(game.content.rules()).is_ok()
            && self.item.class == crate::ItemClass::Amulet
            && !self.item.stackable
            && self.item.equipment.is_none()
            && self.item.consumable.is_none()
            && self.item.edible.is_none()
            && self.item.supply.is_none()
            && game.retrieval_population_compatible(self)
    }
    pub(crate) fn selects_scope(&self, scope: &crate::floor_activation::PreparedScope) -> bool {
        scope.depth >= self.minimum_depth
            && scope.scope == format!("{}/{}", self.family, scope.depth)
    }
    pub(crate) fn reserves(&self, spec: &ItemSpec) -> bool {
        spec.archetype == self.item.archetype || spec.identity == self.item.identity
    }
}
fn invalid() -> GameError {
    GameError::Rule(crate::abilities::RuleError::InvalidDefinition)
}
impl Game {
    pub fn configure_unique_retrieval(
        &mut self,
        policy: UniqueRetrievalPolicy,
    ) -> Result<(), GameError> {
        if self.tick != 0
            || self.unique_retrieval.is_some()
            || !policy.valid(self)
            || self.combat.run.objective().is_some()
            || self.combat.run.arena().is_some()
            || self.items.values().any(|item| policy.reserves(&item.spec))
        {
            return Err(invalid());
        }
        self.unique_retrieval = Some(UniqueRetrievalState {
            policy: Shared::new(policy),
            initialized: false,
            item: None,
            entry_count: 0,
            placement_count: 0,
            entries: vec![],
        });
        Ok(())
    }
    pub fn unique_retrieval_state(&self) -> Option<&UniqueRetrievalState> {
        self.unique_retrieval.as_ref()
    }
    pub fn unique_retrieval_policy(&self) -> Option<&UniqueRetrievalPolicy> {
        self.unique_retrieval.as_ref().map(|state| &*state.policy)
    }
    pub fn retrieval_entries(&self) -> &[RetrievalEntry] {
        self.unique_retrieval
            .as_ref()
            .map_or(&[], |state| &state.entries)
    }
    /// Clear only the bounded evidence of the preceding owning transaction.
    /// A host calls this once when capturing its command candidate, before any
    /// preload can continue physics. Its `*_for_host` calls preserve the batch.
    /// Never call this for an individual entry or an observation/preload.
    pub fn begin_retrieval_transaction(&mut self) {
        if let Some(state) = &mut self.unique_retrieval {
            state.entries.clear();
        }
    }
    pub fn retrieval_configuration_matches(&self, policy: Option<&UniqueRetrievalPolicy>) -> bool {
        self.unique_retrieval_policy() == policy
    }
    pub(crate) fn retrieval_source_region(&self) -> Option<RegionId> {
        self.unique_retrieval
            .as_ref()
            .and_then(|state| state.item)
            .and_then(|id| self.target_region(crate::ReferenceTarget::Item(id)))
    }

    pub(crate) fn retrieval_initial_entry(
        &mut self,
        id: ActorId,
        at: Location,
    ) -> Result<(), GameError> {
        if self
            .unique_retrieval
            .as_ref()
            .is_none_or(|state| state.initialized)
        {
            return Ok(());
        }
        self.retrieval_entry(id, at, EntryCause::Initial)?;
        if let Some(state) = &mut self.unique_retrieval {
            state.initialized = true;
        }
        Ok(())
    }
    pub(crate) fn retrieval_entry(
        &mut self,
        id: ActorId,
        at: Location,
        cause: EntryCause,
    ) -> Result<(), GameError> {
        let Some(state) = &self.unique_retrieval else {
            return Ok(());
        };
        if id != state.policy.protagonist {
            return Ok(());
        }
        let Some(scope) = self
            .activation_scope_at(at.region)
            .filter(|scope| state.policy.selects_scope(scope))
            .cloned()
        else {
            return Ok(());
        };
        let policy = state.policy.clone();
        let item = state.item;
        let ordinal = state
            .entry_count
            .checked_add(1)
            .ok_or(GameError::IdentityExhausted)?;
        if state.entries.len() >= MAX_RETRIEVAL_ENTRIES {
            return Err(GameError::IdentityExhausted);
        }
        // A bound identity is never recreated. Source availability and exact
        // ordinary ownership must have been admitted by residency settlement.
        let source = item
            .map(|id| self.retrieval_item_source(id, &policy))
            .transpose()?;
        let result = if source == Some(crate::ItemLocation::Carried(id)) {
            RetrievalResult::AlreadyCarried {
                item: item.ok_or_else(invalid)?,
            }
        } else {
            let placement = state
                .placement_count
                .checked_add(1)
                .ok_or(GameError::IdentityExhausted)?;
            let candidates = self.retrieval_ground_candidates(&scope, at)?;
            if candidates.is_empty() {
                return Err(GameError::InvalidLocation);
            }
            let mut stream = placement_stream(self.seed, &policy, &scope.scope, placement);
            let destination =
                candidates[crate::floor_activation::index(&mut stream, candidates.len())];
            match (item, source) {
                (None, None) => {
                    let item = self.spawn_item_stack(destination, None, 1, policy.item.clone())?;
                    RetrievalResult::Created {
                        item,
                        at: destination,
                        placement,
                    }
                }
                (Some(item), Some(from)) => {
                    self.relocate_exact_item_in_candidate(item, from, destination)?;
                    RetrievalResult::Relocated {
                        item,
                        from: from.into(),
                        at: destination,
                        placement,
                    }
                }
                _ => return Err(invalid()),
            }
        };
        let state = self.unique_retrieval.as_mut().ok_or_else(invalid)?;
        state.initialized = true;
        state.item = Some(result.item());
        state.entry_count = ordinal;
        if let Some(placement) = result.placement() {
            state.placement_count = placement;
        }
        state.entries.push(RetrievalEntry {
            scope: scope.scope,
            depth: scope.depth,
            entrant: id,
            tick: self.tick,
            ordinal,
            cause,
            result,
        });
        Ok(())
    }
    fn retrieval_item_source(
        &self,
        id: ItemId,
        policy: &UniqueRetrievalPolicy,
    ) -> Result<crate::ItemLocation, GameError> {
        let item = self.items.get(&id).ok_or(GameError::ItemUnavailable)?;
        if *item.spec != policy.item
            || item.quantity != 1
            || !item.holds.is_empty()
            || self
                .actors
                .values()
                .any(|actor| actor.equipment.values().any(|equipped| *equipped == id))
        {
            return Err(GameError::ItemUnavailable);
        }
        if let crate::ItemLocation::Carried(owner) = item.location {
            if !self.actors.contains_key(&owner) {
                return Err(GameError::ItemUnavailable);
            }
        }
        Ok(item.location)
    }
    /// Enumerate bounded *current* ground, independently of population's pinned
    /// originally carved cells. Actors, piles and protected cells are legal.
    fn retrieval_ground_candidates(
        &self,
        scope: &crate::floor_activation::PreparedScope,
        arrival: Location,
    ) -> Result<Vec<Location>, GameError> {
        use std::collections::{BTreeSet, VecDeque};
        let mut admitted = BTreeSet::new();
        let mut examined = 0usize;
        for member in &scope.members {
            let bounds = self
                .world
                .region(*member)
                .ok_or(GameError::InvalidLocation)?
                .bounds;
            if !self.world.loaded_regions().any(|loaded| loaded == *member) {
                return Err(GameError::InvalidLocation);
            }
            let (width, depth, _) = bounds.dimensions();
            let count = usize::try_from(width)
                .ok()
                .and_then(|w| usize::try_from(depth).ok().and_then(|d| w.checked_mul(d)))
                .ok_or(GameError::InvalidLocation)?;
            examined = examined
                .checked_add(count)
                .filter(|count| *count <= crate::floor_activation::MAX_SCOPE_CELLS)
                .ok_or(GameError::InvalidLocation)?;
            let origin = bounds.origin();
            for x in 0..width {
                for y in 0..depth {
                    let position = tor_world::Position {
                        x: origin.x.checked_add(x).ok_or(GameError::InvalidLocation)?,
                        y: origin.y.checked_add(y).ok_or(GameError::InvalidLocation)?,
                        z: 0,
                    };
                    let at = Location {
                        region: *member,
                        position,
                    };
                    if self.retrieval_supported_ground(at) {
                        admitted.insert(at);
                    }
                }
            }
        }
        if !admitted.contains(&arrival) {
            return Ok(vec![]);
        }
        let mut reached = BTreeSet::from([arrival]);
        let mut queue = VecDeque::from([arrival]);
        while let Some(at) = queue.pop_front() {
            for direction in Direction::HORIZONTAL {
                // Horizontal movement never follows an abstract stair. The
                // ordinary resolver preserves beveled corner walking.
                let next = if direction.components().is_some() {
                    self.world
                        .diagonal_reach(at, direction, |side| admitted.contains(&side))
                } else {
                    self.world.physics_neighbor(at, direction)
                };
                if let Some((to, _)) = next {
                    if admitted.contains(&to) && reached.insert(to) {
                        queue.push_back(to);
                    }
                }
            }
        }
        Ok(reached.into_iter().collect())
    }
    fn retrieval_supported_ground(&self, at: Location) -> bool {
        if !self.world.walkable(at) {
            return false;
        }
        let gravity = self.world.gravity(at).unwrap_or([0; 3]);
        // Rogue's no-gravity ground preserves ordinary walking/placement. For
        // resting items, every nonzero gravity axis must be physically blocked.
        gravity.iter().enumerate().all(|(axis, amount)| {
            if *amount == 0 {
                return true;
            }
            let mut delta = [0i64; 3];
            delta[axis] = i64::from(amount.signum());
            let direction = Direction::from_delta(delta).expect("one gravity axis");
            self.world
                .physics_neighbor(at, direction)
                .is_none_or(|(to, _)| !self.world.walkable(to))
        })
    }
    pub(crate) fn retrieval_placements_at(&self, tick: u64) -> impl Iterator<Item = ItemId> + '_ {
        self.retrieval_entries()
            .iter()
            .filter(move |entry| entry.tick == tick && entry.result.placement().is_some())
            .map(|entry| entry.result.item())
    }
    pub(crate) fn is_terminal_exit(
        &self,
        actor: ActorId,
        at: Location,
        direction: Direction,
    ) -> bool {
        self.unique_retrieval_policy().is_some_and(|policy| {
            policy.protagonist == actor
                && policy.exit.at == at
                && policy.exit.direction == direction
        })
    }
    pub(crate) fn prepare_terminal_escape(
        &self,
        actor: ActorId,
        direction: Direction,
    ) -> Result<ItemId, GameError> {
        let state = self.unique_retrieval.as_ref().ok_or(GameError::Blocked)?;
        let at = self
            .actors
            .get(&actor)
            .ok_or(GameError::UnknownActor)?
            .location;
        if !self.is_terminal_exit(actor, at, direction) {
            return Err(GameError::Blocked);
        }
        let item = state.item.ok_or(GameError::ItemUnavailable)?;
        if self.retrieval_item_source(item, &state.policy)? != crate::ItemLocation::Carried(actor) {
            return Err(GameError::ItemUnavailable);
        }
        Ok(item)
    }
    pub(crate) fn commit_terminal_escape(
        &mut self,
        actor: ActorId,
        item: ItemId,
    ) -> Result<(), GameError> {
        if self.prepare_terminal_escape(actor, Direction::Up)? != item {
            return Err(GameError::ItemUnavailable);
        }
        let crate::run_policy::RunMode::Adventure(run) = &mut self.combat.run else {
            return Err(invalid());
        };
        if run.outcome.terminal {
            return Err(GameError::Run(crate::run_policy::RunError::Finished));
        }
        run.outcome.victor = Some(actor);
        run.outcome.terminal = true;
        Ok(())
    }
    /// Historical shape/policy validation deliberately does not compare the
    /// present branch's counters, tick, ownership or allocation high-water.
    pub fn validate_retrieval_entry(&self, entry: &RetrievalEntry) -> Result<(), GameError> {
        let policy = self.unique_retrieval_policy().ok_or_else(invalid)?;
        if entry.scope != format!("{}/{}", policy.family, entry.depth)
            || entry.depth < policy.minimum_depth
            || entry.entrant != policy.protagonist
            || entry.ordinal == 0
            || entry.result.item().0 == 0
            || entry.result.item().0 == u64::MAX
            || (entry.cause == EntryCause::Initial && entry.ordinal != 1)
        {
            return Err(invalid());
        }
        match &entry.result {
            RetrievalResult::Created { at, placement, .. } => {
                if entry.ordinal != 1 || *placement != 1 || at.region.0 == 0 || at.position.z != 0 {
                    return Err(invalid());
                }
            }
            RetrievalResult::Relocated {
                from,
                at,
                placement,
                ..
            } => {
                if *placement < 2
                    || *placement > entry.ordinal
                    || at.region.0 == 0
                    || at.position.z != 0
                {
                    return Err(invalid());
                }
                match from {
                    RetrievalSource::Ground(at) if at.region.0 != 0 => {}
                    RetrievalSource::Carried(owner)
                        if owner.0 > 0 && owner.0 != u64::MAX && *owner != policy.protagonist => {}
                    _ => return Err(invalid()),
                }
            }
            RetrievalResult::AlreadyCarried { .. } if entry.ordinal >= 2 => {}
            RetrievalResult::AlreadyCarried { .. } => return Err(invalid()),
        }
        Ok(())
    }
    pub fn validate_retrieval_entry_identity_bounds(
        &self,
        entry: &RetrievalEntry,
    ) -> Result<(), GameError> {
        if entry.entrant.0 == 0
            || entry.entrant.0 >= self.next_actor_id
            || entry.result.item().0 == 0
            || entry.result.item().0 >= self.next_item_id
            || matches!(entry.result, RetrievalResult::Relocated { from: RetrievalSource::Carried(actor), .. } if actor.0 == 0 || actor.0 >= self.next_actor_id)
        {
            return Err(invalid());
        }
        Ok(())
    }
    // Retained boundary evidence has its own registered scopes and metadata.
    // Expired historical branches are validated against source-pinned bounds by
    // the host, rather than against a later Game's changing set of known floors.
    fn retrieval_entry_addresses_valid(&self, entry: &RetrievalEntry) -> bool {
        let Some(scope) = self
            .activation_scopes()
            .find(|scope| scope.scope == entry.scope)
        else {
            return false;
        };
        if scope.depth != entry.depth {
            return false;
        }
        match &entry.result {
            RetrievalResult::Created { at, .. } => {
                scope.members.contains(&at.region) && self.world.knows(*at)
            }
            RetrievalResult::Relocated { at, from, .. } => {
                scope.members.contains(&at.region)
                    && self.world.knows(*at)
                    && match from {
                        RetrievalSource::Ground(source) => self.world.knows(*source),
                        RetrievalSource::Carried(_) => true,
                    }
            }
            RetrievalResult::AlreadyCarried { .. } => true,
        }
    }
    pub(crate) fn retrieval_state_valid(&self) -> bool {
        let Some(state) = &self.unique_retrieval else {
            return true;
        };
        if !state.policy.valid(self)
            || self.combat.run.objective().is_some()
            || self.combat.run.arena().is_some()
            || state.placement_count > state.entry_count
            || state.item.is_some() != (state.entry_count > 0)
            || (state.entry_count > 0 && (state.placement_count == 0 || !state.initialized))
            || state.entries.len() > MAX_RETRIEVAL_ENTRIES
            || state.entries.len() as u64 > state.entry_count
        {
            return false;
        }
        if let Some(id) = state.item {
            if id.0 == 0
                || id.0 >= self.next_item_id
                || self.retrieval_source_region().is_none()
                || (self.items.contains_key(&id)
                    && self.retrieval_item_source(id, &state.policy).is_err())
            {
                return false;
            }
        }
        if self
            .items
            .iter()
            .any(|(id, item)| state.policy.reserves(&item.spec) && Some(*id) != state.item)
        {
            return false;
        }
        let prior_entries = state.entry_count - state.entries.len() as u64;
        let placements = state
            .entries
            .iter()
            .filter(|entry| entry.result.placement().is_some())
            .count() as u64;
        let Some(mut placement_count) = state.placement_count.checked_sub(placements) else {
            return false;
        };
        if placement_count > prior_entries || (placement_count == 0) != (prior_entries == 0) {
            return false;
        }
        let first = if state.entries.is_empty() {
            0
        } else {
            state.entry_count - state.entries.len() as u64 + 1
        };
        for (offset, entry) in state.entries.iter().enumerate() {
            if entry.ordinal != first + offset as u64
                || entry.tick > self.tick
                || Some(entry.result.item()) != state.item
                || self.validate_retrieval_entry(entry).is_err()
                || !self.retrieval_entry_addresses_valid(entry)
                || self
                    .validate_retrieval_entry_identity_bounds(entry)
                    .is_err()
                || offset > 0 && entry.tick < state.entries[offset - 1].tick
            {
                return false;
            }
            if let Some(placement) = entry.result.placement() {
                let Some(next) = placement_count.checked_add(1) else {
                    return false;
                };
                if placement != next {
                    return false;
                }
                placement_count = next;
            }
        }
        if placement_count != state.placement_count {
            return false;
        }
        let outcome = self.run_outcome();
        outcome.victor.is_none_or(|victor| {
            victor == state.policy.protagonist
                && outcome.terminal
                && self
                    .actors
                    .get(&victor)
                    .is_some_and(|actor| actor.location == state.policy.exit.at)
                && state.item.is_some_and(|item| {
                    self.items
                        .get(&item)
                        .is_some_and(|entry| entry.location == crate::ItemLocation::Carried(victor))
                })
        })
    }
}
impl From<crate::ItemLocation> for RetrievalSource {
    fn from(source: crate::ItemLocation) -> Self {
        match source {
            crate::ItemLocation::Ground(at) => Self::Ground(at),
            crate::ItemLocation::Carried(owner) => Self::Carried(owner),
        }
    }
}
fn placement_stream(seed: u64, policy: &UniqueRetrievalPolicy, scope: &str, placement: u64) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    let mut add = |bytes: &[u8]| {
        for byte in (bytes.len() as u64).to_le_bytes().iter().chain(bytes) {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3);
        }
    };
    add(b"tor-unique-retrieval-placement-v1");
    add(&seed.to_le_bytes());
    add(&policy.version.to_le_bytes());
    add(policy.family.as_bytes());
    add(scope.as_bytes());
    add(&placement.to_le_bytes());
    hash
}

#[cfg(test)]
mod tests;
