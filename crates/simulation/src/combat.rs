//! Deterministic combat values. These are authoritative rules, never wire views.
use crate::{ActorId, Game, GameError, ItemId};
pub(crate) mod contact;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// Nullable context is explicit for trusted direct rule calls. Requiring the
// field prevents silently decoding progress from a different saved format.
fn deserialize_intention_context<'de, D>(
    deserializer: D,
) -> Result<Option<crate::IntentionId>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<crate::IntentionId>::deserialize(deserializer)
}

pub(crate) use crate::actor_creatures::CombatState;
pub use crate::sequence::PreparationProgress;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Preparation {
    #[serde(deserialize_with = "deserialize_intention_context")]
    pub intention: Option<crate::IntentionId>,
    #[serde(deserialize_with = "deserialize_intention_context")]
    pub hold: Option<crate::IntentionId>,
    #[serde(deserialize_with = "deserialize_intention_context")]
    pub origin: Option<crate::IntentionId>,
    pub plan: crate::abilities::Plan,
    pub progress: PreparationProgress,
    pub item_payment: crate::supplies::ItemPayment,
    pub threats: BTreeSet<ActorId>,
    pub remaining: u64,
    pub started: u64,
    pub active: bool,
}
impl<'de> serde::Deserialize<'de> for Preparation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "deserialize_intention_context")]
            intention: Option<crate::IntentionId>,
            #[serde(deserialize_with = "deserialize_intention_context")]
            hold: Option<crate::IntentionId>,
            #[serde(deserialize_with = "deserialize_intention_context")]
            origin: Option<crate::IntentionId>,
            plan: crate::abilities::Plan,
            progress: PreparationProgress,
            item_payment: crate::supplies::ItemPayment,
            threats: BTreeSet<ActorId>,
            remaining: u64,
            started: u64,
            active: bool,
        }
        let fields: Fields = crate::bounded::named_map(deserializer)?;
        Ok(Self {
            intention: fields.intention,
            hold: fields.hold,
            origin: fields.origin,
            plan: fields.plan,
            progress: fields.progress,
            item_payment: fields.item_payment,
            threats: fields.threats,
            remaining: fields.remaining,
            started: fields.started,
            active: fields.active,
        })
    }
}

impl Preparation {
    pub fn origin_intention(&self) -> Option<crate::IntentionId> {
        self.origin
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CombatWorld {
    pub run: crate::run_policy::RunMode,
    pub selected: Option<ActorId>,
    pub characters: BTreeSet<ActorId>,
    pub hostility: BTreeMap<String, BTreeSet<String>>,
    #[serde(deserialize_with = "read_input_boundaries")]
    pub input_boundaries: BTreeMap<ActorId, InputEligibility>,
    pub ai: BTreeMap<ActorId, crate::ai::Ai>,
}

/// Membership retains Require/Human/Preserve history; this value only gates
/// its input-only pin. A configured detached actor has not been inspected yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum InputEligibility {
    Permitted,
    Denied,
    Pending,
}
impl<'de> Deserialize<'de> for InputEligibility {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "permitted" => Ok(Self::Permitted),
            "denied" => Ok(Self::Denied),
            "pending" => Ok(Self::Pending),
            _ => Err(serde::de::Error::custom("Unknown input eligibility")),
        }
    }
}
fn read_input_boundaries<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<ActorId, InputEligibility>, D::Error> {
    struct Boundaries;
    impl<'de> serde::de::Visitor<'de> for Boundaries {
        type Value = BTreeMap<ActorId, InputEligibility>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("unique actor input eligibility entries")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some((actor, eligibility)) = map.next_entry()? {
                if entries.insert(actor, eligibility).is_some() {
                    return Err(serde::de::Error::custom("Duplicate input boundary"));
                }
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_map(Boundaries)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Objective {
    pub anchor: tor_world::Location,
    pub item: Option<ItemId>,
    pub disclosed: bool,
    pub continue_play: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunOutcome {
    pub victor: Option<ActorId>,
    pub deceased: Option<ActorId>,
    pub terminal: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequenceView {
    pub completed: u16,
    pub total: u16,
    pub next_strike_remaining: u64,
    pub cancel_recovery_remaining: u64,
}
/// Read-only actor-active clock projection. Hosts still authorize disclosure;
/// this value cannot alter scheduling or replace a preparation's frozen plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreparationTiming {
    pub preparation_remaining: Option<u64>,
    pub preparation_active: bool,
    pub recovery_remaining: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CombatView {
    /// Exact creature information belongs only to the observer's own actor.
    pub own_stats: Option<crate::creatures::OwnStats>,
    /// The observer's immutable paid record, never a current subject lookup.
    pub public_plan: Option<tor_world::Shared<crate::public_plan::PublicPlanObservation>>,
    pub hp: u32,
    pub max_hp: u32,
    pub preparation_remaining: Option<u64>,
    pub preparation_active: bool,
    pub recovery_remaining: u64,
    pub sequence: Option<SequenceView>,
    pub actors: Vec<(ActorId, bool, bool, Injury)>,
    /// What the action this view follows did, as far as the observer knows.
    pub events: Vec<DisclosedCombatEvent>,
    pub objective: Option<ObjectiveKind>,
    /// Where the objective is met, when the objective is disclosed.
    pub exit: Option<tor_world::Location>,
    pub victory: bool,
    pub dead: bool,
    pub terminal: bool,
}

/// How hurt a visible actor looks; never its numbers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Injury {
    Healthy,
    Wounded,
    BadlyWounded,
    NearDeath,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveKind {
    /// Bring the objective item back to the exit.
    RetrieveAndReturn,
    /// Retrieve the bound item, then deliberately use the upward surface exit.
    RetrieveAndExit,
    ReachExit,
}

/// One disclosed event for every data-defined ability, including the basic attack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisclosedCombatEvent {
    Ability {
        caster: Option<ActorId>,
        target: Option<ActorId>,
        id: crate::content_ids::AbilityId,
        name: String,
        outcome: crate::abilities::OutcomeStatus,
        strike: Option<u16>,
    },
    /// A personally experienced standalone condition, without hidden provenance.
    Condition {
        id: crate::content_ids::ConditionId,
        name: String,
    },
    Interrupted {
        actor: ActorId,
    },
    Died {
        actor: ActorId,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SequenceEnd {
    Completed,
    Abandoned,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatEvent {
    #[serde(deserialize_with = "deserialize_respec_completion")]
    RespecCompleted {
        actor: ActorId,
        basis: crate::player_advancement::AdvancementBasis,
        replacement: crate::player_respec::ReplacementOperation,
        #[serde(deserialize_with = "deserialize_intention_context")]
        intention: Option<crate::IntentionId>,
    },
    Resolved {
        outcome: crate::abilities::Outcome,
        #[serde(deserialize_with = "crate::bounded::required_option")]
        strike: Option<u16>,
    },
    SequenceEnded {
        actor: ActorId,
        #[serde(deserialize_with = "deserialize_intention_context")]
        intention: Option<crate::IntentionId>,
        reason: SequenceEnd,
    },
    ItemCompleted {
        actor: ActorId,
        work: crate::Work,
        intention: Option<crate::IntentionId>,
    },
    Interrupted {
        #[serde(deserialize_with = "deserialize_intention_context")]
        intention: Option<crate::IntentionId>,
        actor: ActorId,
    },
    Died {
        actor: ActorId,
    },
}

// A derived externally tagged struct variant accepts positional JSON bytes.
// Keep the public event shape and all historical variants intact, while the new
// private completion payload requires named fields at the actual byte boundary.
fn deserialize_respec_completion<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<
    (
        ActorId,
        crate::player_advancement::AdvancementBasis,
        crate::player_respec::ReplacementOperation,
        Option<crate::IntentionId>,
    ),
    D::Error,
> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        actor: ActorId,
        basis: crate::player_advancement::AdvancementBasis,
        replacement: crate::player_respec::ReplacementOperation,
        #[serde(deserialize_with = "deserialize_intention_context")]
        intention: Option<crate::IntentionId>,
    }
    let fields: Fields = crate::bounded::named_map(deserializer)?;
    Ok((
        fields.actor,
        fields.basis,
        fields.replacement,
        fields.intention,
    ))
}

impl CombatWorld {
    pub fn new() -> Self {
        Self {
            run: crate::run_policy::RunMode::default(),
            selected: None,
            characters: BTreeSet::new(),
            hostility: BTreeMap::new(),
            input_boundaries: BTreeMap::new(),
            ai: BTreeMap::new(),
        }
    }
}

impl Game {
    pub(crate) fn combat_valid(&self) -> bool {
        self.combat_valid_with_detached_tokens(false)
    }

    /// Arrival validation precedes streaming pins, which may load the other
    /// token partner on the next attempt. Completed boundaries are strict.
    pub(crate) fn combat_valid_with_detached_tokens(&self, allow_detached: bool) -> bool {
        // Characters may be detached, or not built yet, like anyone else;
        // reference points and pins, not these checks, decide what's loaded.
        // An input pin keeps eligible actors active after a transition. An
        // authored voluntary denial suppresses that input-only pin until thaw.
        let known = |id: &ActorId| self.actors.contains_key(id) || self.detached_actor(*id);
        self.combat
            .input_boundaries
            .iter()
            .all(|(id, eligibility)| {
                known(id)
                    && (!self.actors.contains_key(id)
                        || *eligibility == self.actor_input_eligibility(*id))
            })
            && self
                .combat
                .selected
                .is_none_or(|id| known(&id) && self.combat.characters.contains(&id))
            && self.combat.characters.iter().all(known)
            && self.run_policy_valid()
            && self.actors.iter().all(|(id, actor)| {
                self.work_state_valid(*id)
                    && self.token_reservations_valid(*id, allow_detached)
                    && actor.combat.as_ref().is_none_or(|c| c.valid())
                    && self.behavior_valid(*id, actor)
            })
            && self.combat.ai.iter().all(|(id, ai)| {
                self.health(*id).is_some()
                    && ai.profile.valid()
                    && ai.visits.keys().all(|at| self.world.knows(*at))
            })
    }
    pub(crate) fn combat_view(
        &self,
        id: ActorId,
        visible: &impl Fn(tor_world::Location) -> bool,
        perceived: &[(ActorId, crate::actor_store::BodyCells)],
    ) -> Option<CombatView> {
        let a = self.actors.get(&id)?;
        let creature = self.creature(id)?;
        let timing = self.preparation_timing(id)?;
        let actors = perceived
            .iter()
            .filter(|(other, body)| {
                *other != id && self.actors[other].alive() && body.iter().any(|(p, _)| visible(*p))
            })
            .filter_map(|(other, _)| {
                let a = &self.actors[other];
                let c = a.combat.as_ref()?;
                let injury = if c.hp() == c.creature().health().maximum() {
                    Injury::Healthy
                } else if u64::from(c.hp()) * 4 <= u64::from(c.creature().health().maximum()) {
                    Injury::NearDeath
                } else if u64::from(c.hp()) * 2 <= u64::from(c.creature().health().maximum()) {
                    Injury::BadlyWounded
                } else {
                    Injury::Wounded
                };
                Some((
                    *other,
                    self.hostile(id, *other),
                    self.allied(id, *other),
                    injury,
                ))
            })
            .collect();
        // Personal sensations are always safe; third-party events require both participants visible.
        let disclosed =
            |actor| actor == id || self.actors.get(&actor).is_some_and(|a| visible(a.location));
        let seen = |actor| disclosed(actor).then_some(actor);
        let events = self
            .event_log
            .iter()
            .filter_map(|event| match event {
                CombatEvent::Resolved { outcome, .. }
                    if outcome.ability.is_none()
                        && outcome.actor == id
                        && outcome.target == id
                        && outcome.result == crate::abilities::OutcomeStatus::Applied =>
                {
                    let crate::abilities::ResolvedEffect::ApplyCondition { id, .. } =
                        outcome.effects.first()?
                    else {
                        return None;
                    };
                    let definition = self.content.rules.condition(id)?;
                    Some(DisclosedCombatEvent::Condition {
                        id: id.clone(),
                        name: definition.name.clone(),
                    })
                }
                CombatEvent::Resolved { outcome, strike }
                    if (outcome.actor == id
                        || (outcome.target == id && outcome.target_participated()))
                        || (disclosed(outcome.actor) && disclosed(outcome.target)) =>
                {
                    let ability = outcome.ability.as_ref()?;
                    let definition = self.content.rules.ability(ability)?;
                    Some(DisclosedCombatEvent::Ability {
                        caster: seen(outcome.actor),
                        target: seen(outcome.target),
                        id: ability.clone(),
                        name: definition.name.clone(),
                        outcome: outcome.result,
                        strike: *strike,
                    })
                }
                CombatEvent::Interrupted { actor, .. } if *actor == id => {
                    Some(DisclosedCombatEvent::Interrupted { actor: *actor })
                }
                CombatEvent::Died { actor } if disclosed(*actor) => {
                    Some(DisclosedCombatEvent::Died { actor: *actor })
                }
                _ => None,
            })
            .collect();
        Some(CombatView {
            own_stats: Some(creature.own_stats()),
            public_plan: a.public_plan.clone(),
            hp: creature.health().current(),
            max_hp: creature.health().maximum(),
            preparation_remaining: timing.preparation_remaining,
            preparation_active: timing.preparation_active,
            recovery_remaining: timing.recovery_remaining,
            sequence: a
                .pending
                .as_ref()
                .and_then(|p| p.sequence_view(self.actor_clock(id))),
            actors,
            events,
            objective: self
                .unique_retrieval_policy()
                .filter(|policy| policy.protagonist == id)
                .map(|_| ObjectiveKind::RetrieveAndExit)
                .or_else(|| {
                    self.combat
                        .run
                        .objective()
                        .filter(|o| o.disclosed)
                        .map(|o| {
                            if o.item.is_some() {
                                ObjectiveKind::RetrieveAndReturn
                            } else {
                                ObjectiveKind::ReachExit
                            }
                        })
                }),
            exit: self
                .unique_retrieval_policy()
                .filter(|policy| policy.protagonist == id)
                .map(|policy| policy.exit.at)
                .or_else(|| {
                    self.combat
                        .run
                        .objective()
                        .filter(|o| o.disclosed)
                        .map(|o| o.anchor)
                }),
            victory: self.run_outcome().victor.is_some(),
            dead: creature.health().dead(),
            terminal: self.run_outcome().terminal,
        })
    }
    pub fn configure_run(
        &mut self,
        selected: ActorId,
        characters: BTreeSet<ActorId>,
        objective: Option<Objective>,
        hostility: BTreeMap<String, BTreeSet<String>>,
    ) -> Result<(), GameError> {
        let run = crate::run_policy::RunMode::Adventure(crate::run_policy::AdventureRun {
            objective,
            outcome: RunOutcome::default(),
        });
        self.configure_run_policy(selected, characters, hostility, run)
    }

    pub fn configure_arena_run(
        &mut self,
        selected: ActorId,
        participants: BTreeSet<ActorId>,
        hostility: BTreeMap<String, BTreeSet<String>>,
    ) -> Result<(), GameError> {
        self.configure_arena_run_with_limits(
            selected,
            participants,
            hostility,
            crate::arena::ArenaLimits::default(),
        )
    }
    pub fn configure_arena_run_with_limits(
        &mut self,
        selected: ActorId,
        participants: BTreeSet<ActorId>,
        hostility: BTreeMap<String, BTreeSet<String>>,
        limits: crate::arena::ArenaLimits,
    ) -> Result<(), GameError> {
        if self.combat.selected.is_some() {
            return Err(GameError::Run(
                crate::run_policy::RunError::AlreadyConfigured,
            ));
        }
        if participants
            .iter()
            .any(|id| self.creature(*id).is_none() || !self.alive(*id))
        {
            return Err(GameError::Run(
                crate::run_policy::RunError::InvalidParticipants,
            ));
        }
        let run = crate::arena::ArenaRun::new(limits, self.tick)
            .ok_or(GameError::Run(crate::run_policy::RunError::InvalidLimits))?;
        self.configure_run_policy(
            selected,
            participants,
            hostility,
            crate::run_policy::RunMode::Arena(run),
        )
    }
    fn configure_run_policy(
        &mut self,
        selected: ActorId,
        characters: BTreeSet<ActorId>,
        hostility: BTreeMap<String, BTreeSet<String>>,
        run: crate::run_policy::RunMode,
    ) -> Result<(), GameError> {
        if self.combat.run.arena().is_some() && self.combat.selected.is_some() {
            return Err(GameError::Run(
                crate::run_policy::RunError::AlreadyConfigured,
            ));
        }
        let known = |id: &ActorId| self.actors.contains_key(id) || self.detached_actor(*id);
        if !known(&selected) || !characters.contains(&selected) || !characters.iter().all(known) {
            return Err(GameError::UnknownActor);
        }
        let mut candidate = self.clone();
        candidate.combat.run = run;
        candidate.combat.selected = Some(selected);
        candidate.combat.input_boundaries.clear();
        candidate
            .combat
            .input_boundaries
            .insert(selected, candidate.actor_input_eligibility(selected));
        candidate.combat.characters = characters;
        candidate.combat.hostility = hostility;
        candidate.reconcile_recovery_contexts()?;
        candidate.check_run_policy()?;
        *self = candidate;
        Ok(())
    }

    /// A narrow relation for supportive targeting; nonhostility alone is not alliance.
    pub fn allied(&self, actor: ActorId, target: ActorId) -> bool {
        let faction = |id| {
            self.actors
                .get(&id)
                .and_then(|actor| actor.combat.as_ref())
                .map(|combat| combat.identity.faction.as_str())
        };
        matches!((faction(actor), faction(target)), (Some(a), Some(b)) if !a.is_empty() && a == b)
            && !self.hostile(actor, target)
            && !self.faction_hostile(target, actor)
    }

    pub fn hostile(&self, actor: ActorId, target: ActorId) -> bool {
        actor != target
            && (self.faction_hostile(actor, target)
                || self
                    .actors
                    .get(&actor)
                    .and_then(|owner| owner.engagement_at(self.actor_clock(actor)))
                    .is_some_and(|engagement| engagement.target == target))
    }

    pub(crate) fn faction_hostile(&self, actor: ActorId, target: ActorId) -> bool {
        let faction = |id| {
            self.actors
                .get(&id)
                .and_then(|a| a.combat.as_ref())
                .map(|c| &c.identity.faction)
        };
        match (faction(actor), faction(target)) {
            (Some(a), Some(b)) => self
                .combat
                .hostility
                .get(a)
                .is_some_and(|enemies| enemies.contains(b)),
            _ => false,
        }
    }

    pub fn health(&self, actor: ActorId) -> Option<(u32, u32)> {
        let state = self.actors.get(&actor)?.combat.as_ref()?;
        Some((state.hp(), state.creature().health().maximum()))
    }

    /// Internal single-effect application inside the caller's transaction.
    pub(crate) fn apply_damage(
        &mut self,
        actor: ActorId,
        damage: &crate::damage::DamageSpec,
    ) -> Result<u32, GameError> {
        self.apply_damage_with_cause(actor, damage, None)
    }

    /// Future forced-motion episodes pass their bounded original owner here.
    /// Ordinary gravity remains unattributed; a later voluntary move must clear
    /// its causal episode before reaching this shared damage boundary.
    pub(crate) fn apply_damage_with_cause(
        &mut self,
        actor: ActorId,
        damage: &crate::damage::DamageSpec,
        cause: Option<&crate::earned_growth::FatalCause>,
    ) -> Result<u32, GameError> {
        self.settle_actor_time(actor)?;
        let Some(creature) = self.creature(actor) else {
            // Noncombat bodies participate in physics but have no injury pool.
            return self
                .actors
                .get(&actor)
                .map(|_| 0)
                .ok_or(GameError::UnknownActor);
        };
        let before = creature.health().current();
        let snapshot = crate::abilities::Snapshot::capture(&creature);
        let effects = [crate::abilities::PlannedEffect::Damage {
            damage: damage.clone(),
        }];
        let plan = crate::abilities::ResolutionPlan::Effects {
            actor: cause.map_or(actor, |cause| cause.owner),
            target: actor,
            effects: &effects,
        };
        let mut rng = self.rules_rng;
        let outcome = crate::abilities::resolve(plan, &snapshot, &mut rng, &mut ())?;
        self.apply_resolved_outcome_with_origin(
            outcome,
            crate::lifecycle::ResolutionPublication::Standalone,
            &mut crate::lifecycle::DamageActivityGuard::default(),
            cause.map(|cause| cause.origin.clone()),
        )?;
        self.rules_rng = rng;
        Ok(before.saturating_sub(
            self.creature(actor)
                .ok_or(GameError::UnknownActor)?
                .health()
                .current(),
        ))
    }

    pub fn alive(&self, id: ActorId) -> bool {
        self.actors.get(&id).is_some_and(|a| a.alive())
    }

    pub fn combat_events(&self) -> &[CombatEvent] {
        &self.event_log
    }

    pub fn preparation(&self, actor: ActorId) -> Option<&Preparation> {
        self.actors.get(&actor)?.pending.as_ref()
    }

    pub fn preparation_timing(&self, actor: ActorId) -> Option<PreparationTiming> {
        let state = self.actors.get(&actor)?;
        let clock = self.actor_clock(actor);
        Some(PreparationTiming {
            preparation_remaining: state
                .pending
                .as_ref()
                .map(|pending| pending.remaining_at(clock)),
            preparation_active: state.pending.as_ref().is_some_and(|pending| pending.active),
            recovery_remaining: if state.pending.is_none() {
                state.ready_at.saturating_sub(clock)
            } else {
                0
            },
        })
    }

    pub fn pause_preparation(&mut self, actor: ActorId) -> Option<crate::Work> {
        let mut outcome = self.actor_outcome(actor).ok()?;
        let work = outcome.pause(self.actor_clock(actor))?;
        self.apply_outcome(outcome).ok()?;
        Some(work)
    }

    pub fn attack_available(&self, actor: ActorId, target: ActorId) -> bool {
        let Ok(scene) = self.scene(actor) else {
            return false;
        };
        self.attack_available_in_scene(actor, target, &scene)
    }
    pub(crate) fn attack_available_in_scene(
        &self,
        actor: ActorId,
        target: ActorId,
        scene: &[tor_world::SightCell],
    ) -> bool {
        self.melee_reachable_in_scene(actor, target, scene)
    }

    pub(crate) fn melee_neighbor(
        &self,
        from: tor_world::Location,
        delta: [i64; 3],
    ) -> Option<tor_world::Location> {
        self.melee_neighbor_frame(from, delta)
            .map(|(location, _)| location)
    }

    pub(crate) fn next_attack_tick(&self) -> Option<u64> {
        self.actors
            .iter()
            .filter(|(id, _)| !self.actor_frozen(**id))
            .filter_map(|(_, a)| a.pending.as_ref())
            .filter(|p| p.active)
            .map(|p| p.started + p.remaining)
            .min()
    }
}

impl crate::Actor {
    pub(crate) fn alive(&self) -> bool {
        self.combat.as_ref().is_none_or(|c| c.hp() > 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DamageType {
    Energy,
    Impact,
    Keen,
    Spirit,
    Vital,
}

/// Collision injury saturates at the same bound as Health and DamageSpec.
pub fn impact_damage(incoming_velocity: i64) -> u32 {
    incoming_velocity
        .unsigned_abs()
        .saturating_sub(2048)
        .div_ceil(512)
        .min(crate::limits::NUMERIC as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Action, ActorId, Game};

    #[test]
    fn arena_setup_requires_loaded_creature_backed_participants() {
        let mut game = fixture();
        let before = game.clone();
        assert_eq!(
            game.configure_arena_run(ActorId(1), BTreeSet::from([ActorId(1)]), BTreeMap::new()),
            Err(GameError::Run(
                crate::run_policy::RunError::InvalidParticipants
            ))
        );
        assert_eq!(game, before);
    }

    #[test]
    fn injury_and_death_reject_invalid_creature_state_with_rule_errors() {
        let mut game = fixture();
        let before = game.clone();
        assert_eq!(
            crate::test_creatures::injure(&mut game, ActorId(1), 1),
            Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition
            ))
        );
        assert_eq!(game, before);
        configure_subject(&mut game, ActorId(1), 0, 60);
        crate::test_creatures::injure(&mut game, ActorId(1), 0).unwrap();
        assert!(game.alive(ActorId(1)));
        assert!(!game
            .combat_events()
            .iter()
            .any(|event| matches!(event, CombatEvent::Died { .. })));
    }

    #[test]
    fn damage_preserves_shared_snapshots_and_unrelated_creatures() {
        for count in [16, 256, 4096] {
            let mut world = tor_world::World::new(vec![], vec![]).unwrap();
            world
                .add_region(tor_world::Region {
                    id: tor_world::RegionId(1),
                    name: "combat".into(),
                    bounds: tor_world::Extent::new(count + 2, 3, 1).unwrap(),
                })
                .unwrap();
            let mut game = Game::new(world, 42, test_content());
            for x in 0..count {
                let actor = game
                    .spawn_actor(
                        tor_world::Location {
                            region: tor_world::RegionId(1),
                            position: tor_world::Position { x, y: 1, z: 0 },
                        },
                        std::num::NonZeroU64::new(100).unwrap(),
                    )
                    .unwrap();
                configure_subject(&mut game, actor, 0, 60);
            }
            let old = game.clone();
            assert_eq!(
                game.apply_damage(
                    ActorId(1),
                    &crate::damage::DamageSpec::new(
                        vec![crate::damage::DamageComponent::fixed(
                            DamageType::Impact,
                            None,
                            1
                        )],
                        None
                    )
                    .unwrap()
                ),
                Ok(1)
            );
            let copied = crate::diagnostics::work_counts().actor_table_copies;
            game.apply_damage(
                ActorId(2),
                &crate::damage::DamageSpec::new(
                    vec![crate::damage::DamageComponent::fixed(
                        DamageType::Impact,
                        None,
                        1,
                    )],
                    None,
                )
                .unwrap(),
            )
            .unwrap();
            assert_eq!(
                crate::diagnostics::work_counts().actor_table_copies,
                copied,
                "a second injury in the same transaction must not clone the actor table again"
            );
            assert_eq!(old.health(ActorId(1)), Some((30, 30)));
            assert_eq!(game.health(ActorId(1)), Some((29, 30)));
            assert_eq!(
                old.health(ActorId(count as u64)),
                game.health(ActorId(count as u64))
            );
        }
    }

    fn fixture() -> Game {
        let mut game = Game::two_room_in_stone(42, test_content());
        game.spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 1, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        game
    }

    fn subject(bonus: i32, wind_up: u64) -> crate::creatures::Species {
        let mut species = crate::test_creatures::species();
        species.id =
            crate::content_ids::SpeciesId::new(format!("combat_{bonus}_{wind_up}")).unwrap();
        species.melee = crate::attacks::NaturalAttack::from_melee(crate::test_creatures::melee(
            bonus,
            wind_up,
            DamageType::Impact,
            4,
        ))
        .unwrap();
        species
    }
    fn configure_subject(game: &mut Game, actor: ActorId, bonus: i32, wind_up: u64) {
        let build = crate::creatures::CreatureBuild::new(
            game.content().clone(),
            subject(bonus, wind_up).id,
            crate::progression::HdLedger::seeded(vec![crate::progression::HdSource::Racial], 42)
                .unwrap(),
            crate::attributes::ManaBinding::Intellect,
            [
                tor_simulation::attributes::Skill::Deception,
                tor_simulation::attributes::Skill::Persuasion,
            ],
        )
        .unwrap();
        game.configure_creature(
            actor,
            crate::CreatureIdentity {
                name: "subject".into(),
                faction: "neutral".into(),
            },
            build,
        )
        .unwrap();
    }
    fn test_content() -> std::sync::Arc<crate::creatures::Content> {
        let mut catalog = crate::creatures::ScenarioCatalog::default();
        for species in [subject(0, 60), subject(100, 60), subject(100, 30)] {
            catalog.species.insert(species.id.clone(), species);
        }
        crate::creatures::Content::new(crate::test_creatures::ruleset(), catalog).unwrap()
    }

    fn opponents() -> Game {
        opponents_with_windups([60, 60])
    }

    fn opponents_with_windups(wind_ups: [u64; 2]) -> Game {
        let mut game = fixture();
        game.spawn_actor(
            tor_world::Location {
                region: tor_world::RegionId(1),
                position: tor_world::Position { x: 2, y: 1, z: 0 },
            },
            std::num::NonZeroU64::new(100).unwrap(),
        )
        .unwrap();
        for (id, wind_up) in [ActorId(1), ActorId(2)].into_iter().zip(wind_ups) {
            configure_subject(&mut game, id, 100, wind_up);
        }
        game
    }

    #[test]
    fn attacks_resolve_after_windup_and_charge_recovery() {
        let mut game = opponents();
        game.act(
            ActorId(1),
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::ActorTarget { target: ActorId(2) },
                supplies: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(game.tick(), 0);
        assert_eq!(game.health(ActorId(2)), Some((30, 30)));
        game.act(ActorId(2), Action::Wait).unwrap();
        assert_eq!(game.tick(), 100);
        assert_eq!(game.health(ActorId(2)), Some((26, 30)));
        assert!(game.preparation(ActorId(1)).is_none());
    }

    #[test]
    fn earlier_hit_interrupts_without_erasing_spent_preparation() {
        let mut game = opponents_with_windups([60, 30]);
        game.act(
            ActorId(1),
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::ActorTarget { target: ActorId(2) },
                supplies: Default::default(),
            },
        )
        .unwrap();
        game.act(
            ActorId(2),
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::ActorTarget { target: ActorId(1) },
                supplies: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(game.tick(), 30);
        let progress = game.preparation(ActorId(1)).unwrap();
        assert!(!progress.active);
        assert_eq!(progress.remaining, 30);
        assert_eq!(game.health(ActorId(1)), Some((26, 30)));
        game.act(
            ActorId(1),
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::ActorTarget { target: ActorId(2) },
                supplies: Default::default(),
            },
        )
        .unwrap();
        assert_eq!(game.health(ActorId(2)), Some((26, 30)));
        assert_eq!(game.tick(), 70);
    }

    #[test]
    fn death_drops_once_and_removes_living_occupancy() {
        let mut game = opponents();
        let damage = crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                DamageType::Vital,
                None,
                100,
            )],
            None,
        )
        .unwrap();
        assert_eq!(game.apply_damage(ActorId(2), &damage), Ok(30));
        let count = game.items.len();
        assert_eq!(game.apply_damage(ActorId(2), &damage), Ok(0));
        assert_eq!(game.items.len(), count);
        assert!(!game.alive(ActorId(2)));
        assert!(!game.occupied(game.actors[&ActorId(2)].location));
    }

    #[test]
    fn authored_combat_survives_checkpoints_without_changing_rng() {
        let mut game = fixture();
        configure_subject(&mut game, ActorId(1), 0, 60);
        let source = game
            .inspect_creature(ActorId(1))
            .expect("owned combat source");
        assert_eq!(source.creature.build().ledger().total_hd(), 1);
        let mut shared = crate::checkpoint::SharedState::default();
        let restored = Game::restore_checkpoint(
            game.checkpoint(&mut shared),
            &shared,
            game.content().clone(),
        )
        .unwrap();
        assert_eq!(game, restored);
        assert_eq!(game.health(ActorId(1)), Some((30, 30)));
    }

    #[test]
    fn damage_persists_without_wait_healing() {
        let mut game = fixture();
        configure_subject(&mut game, ActorId(1), 0, 60);
        game.apply_damage(
            ActorId(1),
            &crate::damage::DamageSpec::new(
                vec![crate::damage::DamageComponent::fixed(
                    DamageType::Impact,
                    None,
                    4,
                )],
                None,
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(game.health(ActorId(1)), Some((26, 30)));
        game.act(ActorId(1), Action::Wait).unwrap();
        assert_eq!(game.health(ActorId(1)), Some((26, 30)));
    }

    #[test]
    fn impacts_have_a_safe_threshold_and_ignore_sign() {
        assert_eq!(impact_damage(2048), 0);
        assert_eq!(impact_damage(2049), 1);
        assert_eq!(impact_damage(-2560), 1);
        assert_eq!(impact_damage(8192), 12);
    }
}
