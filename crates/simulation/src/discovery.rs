//! Actor-owned physical discovery. Queries never create attempts or consume RNG.
//!
//! Presence and family knowledge are separate from physical geometry. First-eye
//! receipts survive reentry, blindness, control transfer and save restoration.
use crate::{ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use tor_world::{ConcealedFeatureId, Shared};

/// Allocation guard, not a forgetting policy: old failed attempts are never evicted.
const MAX_DISCOVERY_FACTS: usize = 262_144;

#[cfg(test)]
thread_local! {
    // Count the candidate scan even on a warm scene cache: LOS counts alone miss
    // the cloned sight vectors and per-cell feature lookup work.
    static DISCOVERY_WORK: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) };
}

/// Stable discovery identity. Actor IDs are never reused and a guise is one-lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum DiscoveryTarget {
    Door(u64),
    Trap(u64),
    Guise(ActorId),
}
impl<'de> Deserialize<'de> for DiscoveryTarget {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            tag = "kind",
            content = "value",
            rename_all = "snake_case",
            deny_unknown_fields
        )]
        enum Fields {
            Door(u64),
            Trap(u64),
            Guise(ActorId),
        }
        Ok(match crate::bounded::named_map(d)? {
            Fields::Door(id) => Self::Door(id),
            Fields::Trap(id) => Self::Trap(id),
            Fields::Guise(id) => Self::Guise(id),
        })
    }
}
impl From<ConcealedFeatureId> for DiscoveryTarget {
    fn from(value: ConcealedFeatureId) -> Self {
        match value {
            ConcealedFeatureId::Door(id) => Self::Door(id),
            ConcealedFeatureId::Trap(id) => Self::Trap(id),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct DiscoveryFact {
    feature: DiscoveryTarget,
    attempted: bool,
    adjacent_attempted: bool,
    presence: bool,
    family: bool,
}
impl<'de> Deserialize<'de> for DiscoveryFact {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            feature: DiscoveryTarget,
            attempted: bool,
            adjacent_attempted: bool,
            presence: bool,
            family: bool,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            feature: f.feature,
            attempted: f.attempted,
            adjacent_attempted: f.adjacent_attempted,
            presence: f.presence,
            family: f.family,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid discovery fact"));
        }
        Ok(value)
    }
}
impl DiscoveryFact {
    fn valid(self) -> bool {
        let id = match self.feature {
            DiscoveryTarget::Door(id) | DiscoveryTarget::Trap(id) => id,
            DiscoveryTarget::Guise(id) => id.0,
        };
        id != 0
            && id != u64::MAX
            && (self.attempted || self.presence)
            && (!self.adjacent_attempted || self.attempted)
            && (!self.family || self.presence)
            && (!self.family || matches!(self.feature, DiscoveryTarget::Trap(_)))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub(crate) struct DiscoveryKnowledge(Vec<DiscoveryFact>);
impl DiscoveryKnowledge {
    pub(crate) fn features(&self) -> impl Iterator<Item = ConcealedFeatureId> + '_ {
        self.0.iter().filter_map(|fact| match fact.feature {
            DiscoveryTarget::Door(id) => Some(ConcealedFeatureId::Door(id)),
            DiscoveryTarget::Trap(id) => Some(ConcealedFeatureId::Trap(id)),
            DiscoveryTarget::Guise(_) => None,
        })
    }
    pub(crate) fn guise_subjects(&self) -> impl Iterator<Item = ActorId> + '_ {
        self.0.iter().filter_map(|fact| match fact.feature {
            DiscoveryTarget::Guise(id) => Some(id),
            _ => None,
        })
    }
    pub(crate) fn recognized_guises(&self) -> Vec<ActorId> {
        let start = self
            .0
            .partition_point(|fact| !matches!(fact.feature, DiscoveryTarget::Guise(_)));
        self.0[start..]
            .iter()
            .filter_map(|fact| match fact.feature {
                DiscoveryTarget::Guise(id) if fact.presence => Some(id),
                _ => None,
            })
            .collect()
    }
    fn index(&self, feature: DiscoveryTarget) -> Result<usize, usize> {
        self.0.binary_search_by_key(&feature, |fact| fact.feature)
    }
    fn fact(&self, feature: impl Into<DiscoveryTarget>) -> Option<&DiscoveryFact> {
        self.index(feature.into()).ok().map(|index| &self.0[index])
    }
    fn update(
        &mut self,
        feature: impl Into<DiscoveryTarget>,
        attempted: bool,
        presence: bool,
        family: bool,
    ) -> Result<(), GameError> {
        let feature = feature.into();
        let index = match self.index(feature) {
            Ok(index) => index,
            Err(index) => {
                if self.0.len() >= MAX_DISCOVERY_FACTS {
                    return Err(GameError::Rule(crate::abilities::RuleError::NumericBudget));
                }
                self.0.insert(
                    index,
                    DiscoveryFact {
                        feature,
                        attempted: false,
                        adjacent_attempted: false,
                        presence: false,
                        family: false,
                    },
                );
                index
            }
        };
        let fact = &mut self.0[index];
        fact.attempted |= attempted;
        fact.presence |= presence;
        fact.family |= family;
        Ok(())
    }
    fn mark_attempt(
        &mut self,
        feature: DiscoveryTarget,
        adjacent: bool,
        success: bool,
    ) -> Result<(), GameError> {
        self.update(feature, true, success, false)?;
        let index = self.index(feature).expect("updated fact");
        self.0[index].adjacent_attempted |= adjacent;
        Ok(())
    }
    pub(crate) fn valid(&self) -> bool {
        self.0.len() <= MAX_DISCOVERY_FACTS
            && self.0.iter().all(|fact| fact.valid())
            && self
                .0
                .windows(2)
                .all(|pair| pair[0].feature < pair[1].feature)
    }
}
pub(crate) fn read_discovery<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Shared<DiscoveryKnowledge>, D::Error> {
    let value = DiscoveryKnowledge(
        crate::bounded::Bounded::<DiscoveryFact, MAX_DISCOVERY_FACTS>::deserialize(d)?.0,
    );
    if !value.valid() {
        return Err(serde::de::Error::custom("invalid discovery knowledge"));
    }
    Ok(Shared::new(value))
}

/// Actor operands captured when paid preparation starts. Candidate geometry and
/// DCs remain live at completion, and no undiscovered feature appears in a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenDiscoveryCheck {
    pub bonus: i64,
    pub edge: i64,
}
/// One active check sample applied at live distance. These are correlated totals,
/// never independent re-rolls or separately owned consumable checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiscoveryRoll {
    near: i64,
    middle: i64,
    far: i64,
}
impl<'de> Deserialize<'de> for DiscoveryRoll {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            near: i64,
            middle: i64,
            far: i64,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            near: fields.near,
            middle: fields.middle,
            far: fields.far,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid discovery sample"));
        }
        Ok(value)
    }
}
impl DiscoveryRoll {
    pub(crate) fn valid(self) -> bool {
        [self.near, self.middle, self.far]
            .iter()
            .all(|n| n.unsigned_abs() <= crate::limits::NUMERIC as u64 + 18)
            && self.near >= self.middle
            && self.middle >= self.far
    }
    pub(crate) fn at(self, distance: u32) -> i64 {
        match distance {
            0..=1 => self.near,
            2..=3 => self.middle,
            _ => self.far,
        }
    }
    #[cfg(test)]
    fn fixed(total: i64) -> Self {
        Self {
            near: total,
            middle: total,
            far: total,
        }
    }
}

impl FrozenDiscoveryCheck {
    pub(crate) fn capture_for(
        creature: &crate::creatures::CreatureState,
        actor: ActorId,
    ) -> Result<Self, GameError> {
        Self::capture_skill_for(creature, actor, crate::attributes::Skill::Perception, 0)
    }
    pub(crate) fn capture_skill_for(
        creature: &crate::creatures::CreatureState,
        actor: ActorId,
        skill: crate::attributes::Skill,
        extra_edge: i64,
    ) -> Result<Self, GameError> {
        let derived = creature.derived();
        let value = Self {
            bonus: crate::attributes::SkillCheck {
                skill,
                binding: derived.binding(),
                modifier: 0,
                threshold: 0,
            }
            .bonus(derived.attributes, derived.skills),
            edge: derived
                .discovery_edge(
                    creature.build().ruleset(),
                    actor,
                    None,
                    &[],
                    crate::rules::OutgoingEdgePolicy::Ordinary,
                )
                .map_err(GameError::Build)?
                + extra_edge
                + derived.perception.skill_edge(skill),
        };
        if !value.valid() {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        Ok(value)
    }
    pub(crate) fn valid(self) -> bool {
        self.bonus.unsigned_abs() <= crate::limits::NUMERIC as u64
            && self.edge.unsigned_abs() <= crate::limits::NUMERIC as u64
    }
    pub(crate) fn sample(self, rng: &mut u64) -> DiscoveryRoll {
        let rolled = crate::dice::roll_check(rng, crate::dice::Edge::from_counts(3, 0));
        let total = |distance_edge: i64| {
            let balance = (self.edge + distance_edge).clamp(-3, 3);
            let count = 3 + balance.unsigned_abs() as usize;
            let mut values = rolled.rolled()[..count].to_vec();
            values.sort_unstable();
            if balance > 0 {
                values.reverse();
            }
            self.bonus + values[..3].iter().map(|n| i64::from(*n)).sum::<i64>()
        };
        DiscoveryRoll {
            near: total(1),
            middle: total(0),
            far: total(-1),
        }
    }
    pub(crate) fn roll_at(self, rng: &mut u64, passive: bool, distance: u32) -> i64 {
        let adjustment = match distance {
            0..=1 => 1,
            2..=3 => 0,
            _ => -1,
        };
        Self {
            edge: self.edge + adjustment,
            ..self
        }
        .roll(rng, passive)
    }
    pub(crate) fn roll(self, rng: &mut u64, passive: bool) -> i64 {
        let balance = self.edge - i64::from(passive);
        let edge = if balance >= 0 {
            crate::dice::Edge::from_counts(balance as u32, 0)
        } else {
            crate::dice::Edge::from_counts(0, balance.unsigned_abs() as u32)
        };
        i64::from(crate::dice::roll_check(rng, edge).kept) + self.bonus
    }
}
impl<'de> Deserialize<'de> for FrozenDiscoveryCheck {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            bonus: i64,
            edge: i64,
        }
        let f: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            bonus: f.bonus,
            edge: f.edge,
        };
        if !value.valid() {
            return Err(serde::de::Error::custom("invalid discovery operands"));
        }
        Ok(value)
    }
}

#[derive(Clone, Debug)]
struct DiscoveryWitness {
    world: tor_world::PerceptionSnapshot,
    actors: u64,
    eligible: BTreeSet<ActorId>,
    observers: BTreeMap<ActorId, crate::observation::SceneWitness>,
}
#[derive(Clone, Debug, Default)]
pub(crate) struct DiscoveryContexts(Shared<Option<DiscoveryWitness>>);
impl PartialEq for DiscoveryContexts {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for DiscoveryContexts {}

impl Game {
    /// Recognition is observer-owned; it does not spend the subject's guise/ambush.
    pub(crate) fn guise_known(&self, observer: ActorId, subject: ActorId) -> bool {
        self.actors
            .get(&observer)
            .and_then(|actor| actor.discovery.fact(DiscoveryTarget::Guise(subject)))
            .is_some_and(|fact| fact.presence)
    }
    pub(crate) fn has_discovery_cues(&self) -> bool {
        self.world.has_concealed_features() || self.actors.has_guises()
    }

    pub fn feature_known(&self, actor: ActorId, feature: ConcealedFeatureId) -> bool {
        self.actors
            .get(&actor)
            .and_then(|actor| actor.discovery.fact(feature))
            .is_some_and(|fact| fact.presence)
    }
    pub fn feature_family_known(&self, actor: ActorId, feature: ConcealedFeatureId) -> bool {
        self.actors
            .get(&actor)
            .and_then(|actor| actor.discovery.fact(feature))
            .is_some_and(|fact| fact.family)
    }
    /// Trusted activation or an authorized adjacent examination grants the family.
    pub(crate) fn reveal_feature_family(
        &mut self,
        actor: ActorId,
        feature: ConcealedFeatureId,
    ) -> Result<(), GameError> {
        if !matches!(feature, ConcealedFeatureId::Trap(_)) || !self.knows_concealed_feature(feature)
        {
            return Err(GameError::InvalidLocation);
        }
        let owner = self.actors.get(&actor).ok_or(GameError::UnknownActor)?;
        let mut knowledge = owner.discovery.clone();
        knowledge.update(feature, false, true, true)?;
        self.actors
            .get_mut(&actor)
            .expect("validated observer")
            .discovery = knowledge;
        Ok(())
    }
    /// Automatic scans exclude one-use check tokens, including somebody else's
    /// reserved paid check. Timed, equipment and talent edges use the ordinary fold.
    pub(crate) fn capture_discovery_check(
        &self,
        actor: ActorId,
    ) -> Result<FrozenDiscoveryCheck, GameError> {
        let creature = self.creature(actor).ok_or(GameError::UnknownActor)?;
        FrozenDiscoveryCheck::capture_for(&creature, actor)
    }
    fn active_discovery_eligible(&self, actor: ActorId) -> bool {
        self.actors
            .get(&actor)
            .is_some_and(|owner| owner.alive() && owner.combat.is_some())
            && !self.actor_frozen(actor)
            && self.condition_allows_voluntary_input(actor)
    }
    fn discovery_eligible(&self, actor: ActorId) -> bool {
        self.active_discovery_eligible(actor) && self.actors[&actor].perception().sight
    }

    /// Physical touch only. Every component ordering must agree and remain
    /// clear before the endpoint; a closed door may be the touched surface.
    /// Unlike local awareness, this never reaches around a blocked corner.
    fn tactile_discovery_cells(&self, actor: ActorId) -> BTreeSet<tor_world::Location> {
        let owner = &self.actors[&actor];
        let Some(body) = self.body_cells(owner.location, owner.orientation, &owner.body) else {
            return BTreeSet::new();
        };
        let occupied_by_other = |at| {
            self.actors
                .at(&self.world, at)
                .keys()
                .any(|id| *id != actor && self.actors[id].alive())
        };
        let mut cells: BTreeSet<_> = body.iter().map(|(at, _)| *at).collect();
        for (origin, _) in body {
            for x in -1_i64..=1 {
                for y in -1_i64..=1 {
                    for z in -1_i64..=1 {
                        let delta = [x, y, z];
                        if delta == [0; 3] {
                            continue;
                        }
                        let mut destination = None;
                        let mut consistent = true;
                        for order in [
                            [0, 1, 2],
                            [0, 2, 1],
                            [1, 0, 2],
                            [1, 2, 0],
                            [2, 0, 1],
                            [2, 1, 0],
                        ] {
                            let mut at = origin;
                            let mut frame = 0;
                            let mut remaining = delta.iter().filter(|v| **v != 0).count();
                            for axis in order {
                                if delta[axis] == 0 {
                                    continue;
                                }
                                let mut component = [0; 3];
                                component[axis] = delta[axis];
                                let direction = tor_world::Direction::from_delta(
                                    tor_world::rotate_vector(frame, component),
                                )
                                .expect("unit component");
                                let Some((next, rotation)) =
                                    self.world.physics_neighbor(at, direction)
                                else {
                                    consistent = false;
                                    break;
                                };
                                remaining -= 1;
                                if self.world.is_wall(next)
                                    || occupied_by_other(next)
                                    || (remaining != 0 && !self.world.walkable(next))
                                {
                                    consistent = false;
                                    break;
                                }
                                at = next;
                                frame = tor_world::compose_rotation(frame, rotation);
                            }
                            if !consistent || destination.is_some_and(|old| old != (at, frame)) {
                                consistent = false;
                                break;
                            }
                            destination = Some((at, frame));
                        }
                        if consistent {
                            if let Some((at, _)) = destination {
                                cells.insert(at);
                            }
                        }
                    }
                }
            }
        }
        cells
    }

    /// Eye sight only: awareness, contact and abstract stair landing projection do
    /// not license a concealed exterior cue. Alias/body cells deduplicate by ID.
    fn discovery_candidates(
        &self,
        actor: ActorId,
    ) -> Result<BTreeMap<DiscoveryTarget, (Option<i64>, u32)>, GameError> {
        let mut candidates = BTreeMap::new();
        if !self.discovery_eligible(actor) {
            return Ok(candidates);
        }
        let (_, sight) = self.scene_snapshot(actor)?;
        #[cfg(test)]
        DISCOVERY_WORK.with(|count| {
            let (scans, cells) = count.get();
            count.set((scans + 1, cells + sight.len()));
        });
        let cells: BTreeSet<_> = sight
            .iter()
            .filter(|cell| cell.physical)
            .map(|cell| cell.location)
            .collect();
        let owner = &self.actors[&actor];
        let body: BTreeSet<_> = self
            .body_cells(owner.location, owner.orientation, &owner.body)
            .into_iter()
            .flatten()
            .map(|(at, _)| at)
            .collect();
        let mut origins: Vec<_> = sight
            .iter()
            .filter(|cell| cell.physical && body.contains(&cell.location))
            .map(|cell| cell.offset)
            .collect();
        // A dark occupied cell is still a physical distance origin. Retain
        // visible physical aliases, and add the resolved body's canonical frame
        // offsets independently of illumination (never additional target cues).
        if !body.is_empty() {
            origins.extend(
                owner
                    .body
                    .cells
                    .iter()
                    .map(|[x, y, z]| tor_world::Position {
                        x: *x,
                        y: *y,
                        z: *z,
                    }),
            );
        }
        let mut distances = BTreeMap::new();
        for cell in sight.iter().filter(|cell| cell.physical) {
            let range = origins
                .iter()
                .map(|origin| {
                    (i64::from(cell.offset.x) - i64::from(origin.x))
                        .unsigned_abs()
                        .max((i64::from(cell.offset.y) - i64::from(origin.y)).unsigned_abs())
                        .max((i64::from(cell.offset.z) - i64::from(origin.z)).unsigned_abs())
                        as u32
                })
                .min()
                .unwrap_or(u32::MAX);
            distances
                .entry(cell.location)
                .and_modify(|old: &mut u32| *old = (*old).min(range))
                .or_insert(range);
        }
        let mut insert = |target, dc, at| {
            let range = distances.get(&at).copied().unwrap_or(u32::MAX);
            candidates
                .entry(target)
                .and_modify(|old: &mut (Option<i64>, u32)| old.1 = old.1.min(range))
                .or_insert((dc, range));
        };
        for at in &cells {
            if let (Some(concealment), Some(door)) =
                (self.world.secret_door(*at), self.world.door(*at))
            {
                insert(
                    DiscoveryTarget::Door(door.id),
                    (!door.open).then_some(i64::from(concealment.dc)),
                    *at,
                );
            }
            if let Some(trap) = self.world.trap(*at) {
                insert(
                    DiscoveryTarget::Trap(trap.id),
                    Some(i64::from(trap.dc)),
                    *at,
                );
            }
        }
        let see_invisible = self.actors[&actor].perception().see_invisible;
        for (subject, body) in self.actors.perceived(&self.world, &cells) {
            let state = &self.actors[&subject];
            let capabilities = state.perception();
            if subject != actor
                && state.alive()
                && state.guise.appearance().is_some()
                && (!capabilities.invisible || see_invisible)
            {
                if let Some(total) = state.guise.concealment_total() {
                    for (at, _) in body.iter().filter(|(at, _)| cells.contains(at)) {
                        insert(DiscoveryTarget::Guise(subject), Some(total), *at);
                    }
                }
            }
        }
        Ok(candidates)
    }
    /// One authoritative actor boundary. Failed first observations persist too;
    /// none of the observation/snapshot/render paths call this mutating method.
    pub(crate) fn reconcile_passive_discovery_for(
        &mut self,
        actor: ActorId,
    ) -> Result<(), GameError> {
        let candidates = self.discovery_candidates(actor)?;
        if candidates.is_empty() {
            return Ok(());
        }
        let check = self.capture_discovery_check(actor)?;
        let mut knowledge = self.actors[&actor].discovery.clone();
        let mut rng = self.rules_rng;
        for (feature, (dc, distance)) in candidates {
            let Some(dc) = dc else {
                // An exposed open entrance is no longer concealed. Its identity
                // remains learned when somebody closes the shared door later.
                knowledge.update(feature, false, true, false)?;
                continue;
            };
            if knowledge.fact(feature).is_some_and(|fact| {
                fact.presence || (fact.attempted && (distance > 1 || fact.adjacent_attempted))
            }) {
                continue;
            }
            let success = check.roll_at(&mut rng, true, distance) >= dc;
            knowledge.mark_attempt(feature, distance <= 1, success)?;
        }
        self.actors
            .get_mut(&actor)
            .expect("validated observer")
            .discovery = knowledge;
        self.rules_rng = rng;
        Ok(())
    }
    /// Authoritative environment changes may affect every loaded active observer.
    pub(crate) fn reconcile_passive_discovery(&mut self) -> Result<(), GameError> {
        if !self.lifecycle.discovery_started || !self.has_discovery_cues() {
            return Ok(());
        }
        let eligible: BTreeSet<_> = self
            .actors
            .keys()
            .copied()
            .filter(|actor| self.discovery_eligible(*actor))
            .collect();
        let world = self.world.perception_snapshot();
        let actors = self.actors.visibility_epoch();
        if self
            .lifecycle
            .discovery_contexts
            .0
            .as_ref()
            .is_some_and(|old| {
                old.world == world && old.actors == actors && old.eligible == eligible
            })
        {
            return Ok(());
        }
        // Actor movement can introduce a guise into unchanged eye sight. World cues
        // depend on this observer's chart/body/sight and shared world geometry
        // and illumination. Reuse observation's complete witness so another
        // actor's visibility change cannot force warm sight-vector clones and
        // feature lookups for every unchanged observer.
        let old = self.lifecycle.discovery_contexts.0.as_ref();
        let mut observers = BTreeMap::new();
        let mut changed = Vec::new();
        for actor in &eligible {
            let witness = self.scene_witness(*actor)?;
            if old.is_none_or(|old| {
                old.observers.get(actor) != Some(&witness)
                    || (old.actors != actors && self.actors.has_guises())
            }) {
                changed.push(*actor);
            }
            observers.insert(*actor, witness);
        }
        // Rebuilding from eligible loaded actors also drops sleep/freeze/death
        // and detached entries: waking/thawing must reconcile deferred cues.
        let witness = DiscoveryWitness {
            world,
            actors,
            eligible,
            observers,
        };
        if changed.is_empty() {
            self.lifecycle.discovery_contexts.0 = Shared::new(Some(witness));
            return Ok(());
        }
        let mut candidate = self.clone();
        for actor in changed {
            candidate.reconcile_passive_discovery_for(actor)?;
        }
        candidate.lifecycle.discovery_contexts.0 = Shared::new(Some(witness));
        *self = candidate;
        Ok(())
    }
    /// Defer encounter receipts while a host constructs an authored game.
    /// Only an empty, fresh game may enter this mode. Complete construction
    /// (including starting equipment) with `initialize_discovery` before use.
    pub fn defer_initial_awareness(&mut self) -> Result<(), GameError> {
        if self.tick != 0 || !self.actors.is_empty() || self.lifecycle.discovery_started {
            return Err(GameError::InvalidLocation);
        }
        self.lifecycle.initial_awareness_deferred = true;
        Ok(())
    }

    /// Initial scenario publication owns its first observation attempts. Hosts
    /// call once after complete construction, never during a query or restoration.
    pub fn initialize_discovery(&mut self) -> Result<(), GameError> {
        if self.lifecycle.discovery_started {
            return self.reconcile_passive_discovery();
        }
        let mut candidate = self.clone();
        candidate.lifecycle.discovery_started = true;
        candidate.lifecycle.initial_awareness_deferred = false;
        candidate.reconcile_passive_discovery()?;
        candidate.reconcile_awareness()?;
        candidate.reconcile_control_contacts()?;
        *self = candidate;
        Ok(())
    }
    /// Apply an already resolved active Search total against live physical cues.
    /// The caller owns the ordinary action's RNG transaction and token lifecycle.
    pub(crate) fn apply_discovery_roll(
        &mut self,
        actor: ActorId,
        roll: DiscoveryRoll,
    ) -> Result<(), GameError> {
        if !self.active_discovery_eligible(actor) {
            return Ok(());
        }
        let mut candidates: BTreeMap<_, _> = self
            .discovery_candidates(actor)?
            .into_iter()
            .map(|(feature, (dc, distance))| (feature, (dc, roll.at(distance))))
            .collect();
        for at in self.tactile_discovery_cells(actor) {
            // A usable visual cue wins. Otherwise adjacent +1 and tactile -1
            // cancel, using the same sample's middle total (no extra roll).
            if let (Some(concealment), Some(door)) =
                (self.world.secret_door(at), self.world.door(at))
            {
                candidates
                    .entry(DiscoveryTarget::Door(door.id))
                    .or_insert((Some(i64::from(concealment.dc)), roll.middle));
            }
            if let Some(trap) = self.world.trap(at) {
                candidates
                    .entry(DiscoveryTarget::Trap(trap.id))
                    .or_insert((Some(i64::from(trap.dc)), roll.middle));
            }
            // Creature guises retain their visual/ambush contract, never touch.
        }
        let mut knowledge = self
            .actors
            .get(&actor)
            .ok_or(GameError::UnknownActor)?
            .discovery
            .clone();
        for (feature, (dc, total)) in candidates {
            if dc.is_none_or(|dc| total >= dc) {
                knowledge.update(feature, false, true, false)?;
            }
        }
        self.actors
            .get_mut(&actor)
            .expect("validated observer")
            .discovery = knowledge;
        Ok(())
    }
    #[cfg(test)]
    pub(crate) fn apply_discovery_total(
        &mut self,
        actor: ActorId,
        total: i64,
    ) -> Result<(), GameError> {
        self.apply_discovery_roll(actor, DiscoveryRoll::fixed(total))
    }
    /// The periodic ring runs the active algorithm without an action or token.
    pub(crate) fn run_active_discovery(
        &mut self,
        actor: ActorId,
        check: &FrozenDiscoveryCheck,
    ) -> Result<(), GameError> {
        if !self.discovery_eligible(actor) {
            return Ok(());
        }
        let mut candidate = self.clone();
        let mut rng = candidate.rules_rng;
        let roll = check.sample(&mut rng);
        candidate.apply_discovery_roll(actor, roll)?;
        candidate.rules_rng = rng;
        *self = candidate;
        Ok(())
    }
}

#[cfg(test)]
#[path = "discovery_tests.rs"]
mod tests;
