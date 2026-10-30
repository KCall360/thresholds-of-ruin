//! Decisions use disclosed observations and remembered navigation, never hidden targets.
use crate::{Action, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::collections::BTreeMap;
use tor_world::{Direction, Location};

fn low_health(current: u64, maximum: u64, percent: u32) -> bool {
    u128::from(current) * 100 <= u128::from(maximum) * u128::from(percent)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AiProfile {
    pub idle: crate::behavior::IdleBehavior,
    pub doors: bool,
    pub care: bool,
    pub equipment: bool,
    pub loot: bool,
    pub gold: bool,
    pub flee_percent: u32,
}
impl<'de> Deserialize<'de> for AiProfile {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            idle: crate::behavior::IdleBehavior,
            doors: bool,
            care: bool,
            equipment: bool,
            loot: bool,
            gold: bool,
            flee_percent: u32,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            idle: fields.idle,
            doors: fields.doors,
            care: fields.care,
            equipment: fields.equipment,
            loot: fields.loot,
            gold: fields.gold,
            flee_percent: fields.flee_percent,
        })
    }
}
impl Default for AiProfile {
    fn default() -> Self {
        Self {
            idle: crate::behavior::IdleBehavior::Explore,
            doors: false,
            care: false,
            equipment: false,
            loot: false,
            gold: false,
            flee_percent: 25,
        }
    }
}
impl AiProfile {
    /// Explicit generic adventurer policy; species still own acquisition.
    pub fn general() -> Self {
        Self {
            idle: crate::behavior::IdleBehavior::Explore,
            doors: true,
            care: true,
            equipment: true,
            loot: true,
            gold: false,
            flee_percent: 25,
        }
    }
    pub fn valid(&self) -> bool {
        self.flee_percent <= 100
    }
}

impl From<crate::behavior::BehaviorPolicy> for AiProfile {
    fn from(policy: crate::behavior::BehaviorPolicy) -> Self {
        Self {
            idle: policy.idle,
            doors: policy.doors,
            care: policy.care,
            equipment: policy.equipment,
            loot: policy.loot,
            gold: policy.gold,
            flee_percent: policy.flee_percent,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    #[default]
    Search,
    Attack,
    Flee,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ai {
    pub profile: AiProfile,
    pub state: State,
    pub visits: crate::navigation_map::RegionMap<Location, u64>,
}

impl<'de> Deserialize<'de> for Ai {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            profile: AiProfile,
            state: State,
            visits: crate::navigation_map::RegionMap<Location, u64>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            profile: fields.profile,
            state: fields.state,
            visits: fields.visits,
        })
    }
}

/// An uninterrupted decision owns evaluated ability operands through commit.
pub(crate) struct AiDecision {
    pub action: Action,
    pub ai: Ai,
    pub behavior: crate::behavior::Behavior,
    pub plan: Option<crate::abilities::Plan>,
    pub numeric_budget_refusal: bool,
}
impl AiDecision {
    fn action(action: Action, ai: Ai, behavior: crate::behavior::Behavior) -> Self {
        Self {
            action,
            ai,
            behavior,
            plan: None,
            numeric_budget_refusal: false,
        }
    }
    fn ability(plan: crate::abilities::Plan, ai: Ai, behavior: crate::behavior::Behavior) -> Self {
        Self {
            action: plan.work().action(),
            ai,
            behavior,
            plan: Some(plan),
            numeric_budget_refusal: false,
        }
    }
}

enum AiAbilityChoice {
    Frozen(crate::abilities::Plan),
    Request(Action),
}
impl From<crate::abilities::Plan> for AiAbilityChoice {
    fn from(plan: crate::abilities::Plan) -> Self {
        Self::Frozen(plan)
    }
}
impl AiAbilityChoice {
    fn decision(self, ai: Ai, behavior: crate::behavior::Behavior) -> AiDecision {
        match self {
            Self::Frozen(plan) => AiDecision::ability(plan, ai, behavior),
            Self::Request(action) => AiDecision::action(action, ai, behavior),
        }
    }
}
#[path = "ai_projectiles.rs"]
mod projectiles;

// Priority-ordered transition table: frightened, target known, resulting state.
const TRANSITIONS: [(bool, bool, State); 4] = [
    (true, true, State::Flee),
    (false, true, State::Attack),
    (true, false, State::Search),
    (false, false, State::Search),
];

/// Only positive healing, across all compiled active branches and riders.
pub(crate) fn safe_healing_plan(plan: &crate::abilities::Plan) -> bool {
    use crate::abilities::{Plan, PlannedAmount, PlannedEffect};
    let Plan::Ability(ability) = plan else {
        return false;
    };
    let mut positive = false;
    let harmless = ability
        .strikes
        .iter()
        .flat_map(|strike| {
            strike
                .on_success
                .iter()
                .chain(&strike.on_failure)
                .chain(strike.riders.iter().flat_map(|rider| &rider.effects))
        })
        .all(|effect| match effect {
            PlannedEffect::Heal { amount } => {
                positive |= match amount {
                    PlannedAmount::Fixed { amount } => *amount > 0,
                    PlannedAmount::Rolled { dice } => dice.maximum() > 0,
                };
                true
            }
            _ => false,
        });
    harmless && positive
}

/// Every frozen resource operand participates in a fresh AI affordability gate.
pub(crate) fn resource_budget_satisfies(
    ledger: &crate::costs::CostLedger,
    bundle: crate::costs::CostBundle,
    minimum_percent: u8,
) -> bool {
    bundle.entries().all(|cost| {
        let available = ledger.available(cost.resource);
        cost.start
            .checked_add(cost.resolution)
            .is_some_and(|total| available >= total)
            && u64::from(available) * 100
                >= u64::from(ledger.resources().maximum(cost.resource)) * u64::from(minimum_percent)
    })
}

impl Game {
    pub(crate) fn ai_plan_resources_available(&self, plan: &crate::abilities::Plan) -> bool {
        let Some(cost) = plan.cost() else {
            return true;
        };
        let ability = plan.ability_id();
        let minimum = ability
            .and_then(|id| self.content.rules().ability(id))
            .map_or(0, |definition| definition.ai.minimum_resource_percent);
        self.creature(plan.actor())
            .is_some_and(|creature| resource_budget_satisfies(creature.costs(), cost, minimum))
    }
}

impl Game {
    pub fn configure_ai(&mut self, actor: ActorId, profile: AiProfile) -> Result<(), GameError> {
        let mut candidate = self.clone();
        candidate.configure_ai_in_transaction(actor, profile)?;
        *self = candidate;
        Ok(())
    }
    fn configure_ai_in_transaction(
        &mut self,
        actor: ActorId,
        profile: AiProfile,
    ) -> Result<(), GameError> {
        if !profile.valid() {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        if self.health(actor).is_none() {
            return Err(GameError::UnknownActor);
        }
        if let Some(ai) = self.combat.ai.get_mut(&actor) {
            ai.profile = profile;
        } else {
            self.combat.ai.insert(
                actor,
                Ai {
                    profile,
                    state: State::Search,
                    visits: Default::default(),
                },
            );
        }
        self.reconcile_awareness()
    }

    pub fn is_ai(&self, actor: ActorId) -> bool {
        self.combat.ai.contains_key(&actor)
    }

    pub fn next_ai_action(&self) -> Option<(ActorId, Action)> {
        let id = self.next_actor()?;
        self.choose_ai(id).map(|decision| (id, decision.action))
    }

    pub(crate) fn choose_ai(&self, id: ActorId) -> Option<AiDecision> {
        let mut ai = self.combat.ai.get(&id)?.clone();
        let actor = self.actors.get(&id)?;
        let policy = actor.behavior_policy()?;
        let mut behavior = actor.behavior.clone();
        behavior.engagement = actor.engagement_at(self.actor_clock(id));
        let snapshot = self.decision_snapshot(id).ok()?;
        let view = &snapshot.view;
        let retained = self.preparation(id).filter(|preparation| {
            !preparation.active
                && self.preparation_still_available(preparation, Some(&snapshot.sight))
        });
        let committed = retained.filter(|preparation| preparation.has_prefix());
        // The private fact that another observer recognizes a guise is not
        // knowledge owned by its wearer. An intact ambush guise waits for actual
        // touch or incoming hostility; mere adjacency never selects an attack.
        if actor.perception().readied_touch_ambush
            && actor.guise.appearance().is_some()
            && retained.is_none()
        {
            if !self.actor_frozen(id) && self.condition_allows_voluntary_input(id) {
                if let Some(toucher) = actor.readied_ambush.toucher() {
                    if let Some(ability) = self
                        .creature(id)
                        .and_then(|creature| crate::readied_ambush::eligible_ability(&creature))
                    {
                        if let Ok(plan) = self.compile_touch_strike_plan(id, &ability, toucher) {
                            if self.ai_plan_resources_available(&plan) {
                                return Some(AiDecision::ability(plan, ai, behavior));
                            }
                        }
                    }
                }
            }
            return Some(AiDecision::action(Action::Wait, ai, behavior));
        }
        let mut search = self.route_search(id).ok();
        let mut route = |destination| search.as_mut()?.route(destination).ok();
        let candidate = view
            .visible_actors
            .iter()
            .filter(|other| {
                other.id != id
                    && self.awareness_allows_target(id, other.id)
                    && self.perceived_hostile(id, other.id, other.disclosure)
            })
            .min_by_key(|other| {
                (
                    route(other.location).map_or(usize::MAX, |r| r.len()),
                    other.id,
                )
            });
        // Retained work resumes its frozen operands; it does not acquire a new
        // enemy, refresh memory or reroll a sensory cue.
        if retained.is_none() && self.condition_allows_voluntary_input(id) {
            if let Some(engagement) = behavior.engagement.as_mut() {
                if let Some(visible) = view.visible_actors.iter().find(|other| {
                    other.id == engagement.target
                        && self.awareness_allows_target(id, other.id)
                        && other.disclosure.reference() == Some(engagement.reference)
                }) {
                    engagement.location = visible.location;
                    engagement.last_seen = self.actor_clock(id);
                } else if view.location == engagement.location {
                    behavior.engagement = None;
                }
            }
            // A greedy species pursues known Gold it can safely collect and
            // acquires a visible hostile only when there is none.
            let acquires = match policy.disposition {
                crate::behavior::Disposition::Passive => false,
                crate::behavior::Disposition::Mean => true,
                crate::behavior::Disposition::Greedy => {
                    self.exposed_to_visible_hostile(id, view.location, view)
                        || self
                            .choose_known_gold(id, view, &ai.profile, &mut route)
                            .is_none()
                }
            };
            if behavior.engagement.is_none() && policy.memory_ticks > 0 && acquires {
                if let Some(visible) = candidate {
                    {
                        behavior.engagement = Some(crate::behavior::Engagement {
                            target: visible.id,
                            reference: visible.disclosure.reference().expect("disclosed foe"),
                            location: visible.location,
                            last_seen: self.actor_clock(id),
                        });
                    }
                }
            }
        }
        let target = behavior.engagement.and_then(|entry| {
            view.visible_actors.iter().find(|other| {
                other.id == entry.target
                    && self.awareness_allows_target(id, other.id)
                    && other.disclosure.reference() == Some(entry.reference)
            })
        });
        let forced_flee = self.forced_flee_location(id);
        let alert = actor.awareness.alert_at(self.actor_clock(id));
        let threat = forced_flee
            .or_else(|| {
                behavior
                    .engagement
                    .map(|entry| entry.location)
                    .or_else(|| retained.and(candidate).map(|other| other.location))
            })
            .or(alert);
        let (hp, max) = self.health(id)?;
        let frightened = forced_flee.is_some() || low_health(hp, max, ai.profile.flee_percent);
        ai.state = TRANSITIONS
            .iter()
            .find(|(low, known, _)| *low == frightened && *known == threat.is_some())
            .unwrap()
            .2;
        // A suspended prefix cannot be replaced by new healing, gear or attacks.
        // Only a chosen ordinary retreat can abandon it, paying the whole tail.
        if ai.state != State::Flee {
            if let Some(preparation) = retained {
                return Some(AiDecision::ability(preparation.plan.clone(), ai, behavior));
            }
        }
        // Use a known restorative before fleeing or attacking. Unknown potions
        // and mixed sequences with harm never qualify as safe healing.
        if ai.profile.care && retained.is_none() && low_health(hp, max, 50) {
            if let Some(item) = view
                .inventory
                .iter()
                .find(|item| self.known_healing(id, item.id))
            {
                return Some(AiDecision::action(
                    Action::Drink { item: item.id },
                    ai,
                    behavior,
                ));
            }
            if let Some(plan) = self.choose_ai_healing(id, &snapshot.sight, view) {
                return Some(AiDecision::ability(plan, ai, behavior));
            }
        }
        if ai.profile.care
            && retained.is_none()
            && ai.state == State::Search
            && !self.exposed_to_visible_hostile(id, view.location, view)
            && crate::ai_items::required_nutrition(view)
                .is_some_and(|hunger| hunger != crate::nutrition::Hunger::Nourished)
        {
            if let Some(item) = view
                .inventory
                .iter()
                .find(|item| self.known_food(id, item.id))
            {
                return Some(AiDecision::action(
                    Action::Eat { item: item.id },
                    ai,
                    behavior,
                ));
            }
        }
        // Care and retained work take precedence. Random movement uses only
        // disclosed legal candidates and the copied actor-owned decision RNG.
        if retained.is_none() && !self.condition_blocks_action(id, &Action::Move(Direction::North))
        {
            if let Some(chance) = actor.locomotion().erratic.filter(|p| p.numerator > 0) {
                let candidates: Vec<_> = Direction::HORIZONTAL
                    .into_iter()
                    .chain([Direction::Up, Direction::Down])
                    .filter(|direction| {
                        self.actor_translation(id, *direction)
                            .is_some_and(|(at, _)| {
                                view.visible_cells
                                    .iter()
                                    .any(|cell| cell.location == at && !cell.wall)
                            })
                    })
                    .collect();
                if !candidates.is_empty() {
                    let draw = crate::dice::random_word(&mut behavior.rng);
                    if draw % u64::from(chance.denominator) < u64::from(chance.numerator) {
                        let choice =
                            crate::dice::random_word(&mut behavior.rng) as usize % candidates.len();
                        return Some(AiDecision::action(
                            Action::Move(candidates[choice]),
                            ai,
                            behavior,
                        ));
                    }
                }
            }
        }
        if ai.state == State::Flee {
            let threat = threat.expect("flee has a disclosed or remembered threat");
            let current = route(threat).map_or(0, |r| r.len());
            let mut choices = Vec::new();
            for direction in Direction::HORIZONTAL
                .into_iter()
                .chain([Direction::Up, Direction::Down])
            {
                if let Some((at, _)) = self.actor_translation(id, direction) {
                    if view
                        .visible_cells
                        .iter()
                        .any(|c| c.location == at && !c.wall)
                    {
                        // Distances in the actor's current region; portal retreat uses remembered route length.
                        let score = if at.region == threat.region {
                            (at.position.x - threat.position.x).unsigned_abs() as usize
                                + (at.position.y - threat.position.y).unsigned_abs() as usize
                                + (at.position.z - threat.position.z).unsigned_abs() as usize
                        } else {
                            current + 1
                        };
                        choices.push((score, direction));
                    }
                }
            }
            if let Some((_, direction)) = choices
                .into_iter()
                .filter(|(d, _)| *d > current)
                .max_by_key(|(d, _)| *d)
            {
                // A technique may use only this already disclosed legal retreat.
                // Paid prefixes still escape through the ordinary atomic Move.
                if committed.is_none() {
                    if let Some(plan) =
                        self.choose_ai_step_ability(id, direction, &snapshot.sight, view)
                    {
                        return Some(AiDecision::ability(plan, ai, behavior));
                    }
                }
                return Some(AiDecision::action(Action::Move(direction), ai, behavior));
            }
        }
        // No admissible retreat: finish the frozen commitment rather than
        // deadlocking on a fresh attack or silently replaying the opening hit.
        if let Some(preparation) = committed {
            return Some(AiDecision::ability(preparation.plan.clone(), ai, behavior));
        }
        // Only a presently sensed threat warrants proactive self-defense.
        // Healing, an admissible escape and paid strike prefixes retain priority.
        if let Some(threat) = target {
            if let Some(plan) = self.choose_ai_defense(id, threat.id, &snapshot.sight, view) {
                return Some(AiDecision::ability(plan, ai, behavior));
            }
        }
        if let Some(crate::behavior::Engagement {
            target,
            reference,
            location: at,
            ..
        }) = behavior.engagement
        {
            // Remembered positions support pursuit, but only a currently
            // disclosed target can prompt a paid technique.
            if ai.state != State::Flee
                && view.visible_actors.iter().any(|actor| {
                    actor.id == target
                        && self.awareness_allows_target(id, target)
                        && actor.disclosure.reference() == Some(reference)
                })
            {
                if let Some(ability) =
                    self.choose_ai_ability(id, target, &snapshot.sight, view, false)
                {
                    return Some(ability.decision(ai, behavior));
                }
            }
            if ai.state == State::Flee
                && view.visible_actors.iter().any(|actor| {
                    actor.id == target
                        && self.awareness_allows_target(id, target)
                        && actor.disclosure.reference() == Some(reference)
                })
            {
                if let Some(ability) =
                    self.choose_ai_ability(id, target, &snapshot.sight, view, true)
                {
                    return Some(ability.decision(ai, behavior));
                }
            }
            if ai.state != State::Flee {
                if let Some(step) = route(at).and_then(|r| r.first().copied()) {
                    if self.actor_translation(id, step.direction).is_some() {
                        return Some(AiDecision::action(
                            Action::Move(step.direction),
                            ai,
                            behavior,
                        ));
                    }
                }
                behavior.engagement = None;
                ai.state = State::Search;
            }
        }
        // An anonymous cue licenses investigation only, never a target attack.
        if ai.state != State::Flee && behavior.engagement.is_none() {
            if let Some(at) = alert.filter(|at| *at != view.location) {
                if let Some(step) = route(at).and_then(|r| r.first().copied()) {
                    if self.actor_translation(id, step.direction).is_some() {
                        return Some(AiDecision::action(
                            Action::Move(step.direction),
                            ai,
                            behavior,
                        ));
                    }
                }
            }
            ai.state = State::Search;
        }
        if ai.state == State::Search && !self.exposed_to_visible_hostile(id, view.location, view) {
            if let Some(action) = ai
                .profile
                .equipment
                .then(|| self.choose_gear(id, view))
                .flatten()
            {
                return Some(AiDecision::action(action, ai, behavior));
            }
            if let Some(action) = ai
                .profile
                .loot
                .then(|| {
                    self.choose_nearby_loot(id, view, &ai.profile, &snapshot.sight, &mut route)
                })
                .flatten()
            {
                return Some(AiDecision::action(action, ai, behavior));
            }
            if let Some(action) = self.choose_known_gold(id, view, &ai.profile, &mut route) {
                return Some(AiDecision::action(action, ai, behavior));
            }
        }
        if ai.state == State::Search
            && ai.profile.doors
            && self.condition_allows_voluntary_input(id)
        {
            if let Some(door) = view
                .visible_cells
                .iter()
                .filter(|c| c.door_reachable)
                .filter_map(|c| c.door)
                .find(|d| !d.open)
            {
                return Some(AiDecision::action(
                    Action::SetDoor {
                        door: door.id,
                        open: true,
                    },
                    ai,
                    behavior,
                ));
            }
        }
        if ai.state == State::Search
            && ai.profile.idle == crate::behavior::IdleBehavior::Explore
            && self.condition_allows_voluntary_input(id)
        {
            let can_move = !self.condition_blocks_action(id, &Action::Move(Direction::North));
            if retained.is_none() && can_move {
                let count = ai
                    .visits
                    .get(&view.location)
                    .copied()
                    .unwrap_or(0)
                    .saturating_add(1);
                crate::diagnostics::ai_visit_entries_copied(
                    ai.visits.copy_on_write_entries(view.location.region),
                );
                ai.visits.insert(view.location, count);
            }
            if !can_move {
                return Some(AiDecision::action(Action::Wait, ai, behavior));
            }
            let mut candidates = Vec::new();
            for direction in Direction::HORIZONTAL
                .into_iter()
                .chain([Direction::Up, Direction::Down])
            {
                if let Some((at, _)) = self.actor_translation(id, direction) {
                    if view
                        .visible_cells
                        .iter()
                        .any(|c| c.location == at && !c.wall)
                    {
                        candidates.push((*ai.visits.get(&at).unwrap_or(&0), at, direction));
                    }
                }
            }
            candidates.sort_by_key(|(visits, at, _)| (*visits, *at));
            if let Some((_, _, direction)) = candidates.first() {
                return Some(AiDecision::action(Action::Move(*direction), ai, behavior));
            }
        }
        Some(AiDecision::action(Action::Wait, ai, behavior))
    }

    /// Default combat policy uses personal capabilities and resources, never
    /// enemy defenses, immunity or condition state. Choices reserve nothing;
    /// ordinary execution rechecks funding and reach before preparation.
    fn choose_ai_ability(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
        basic_only: bool,
    ) -> Option<AiAbilityChoice> {
        use crate::rules::RangePreference;
        let creature = self.creature(actor)?;
        let costs = creature.costs();
        let rules = creature.build().ruleset();
        if let Some(preparation) = self
            .preparation(actor)
            .filter(|preparation| !preparation.active)
        {
            if let crate::Work::UseAbility { id, invocation, .. } = &preparation.plan.work() {
                if let Some(definition) = rules.ability(id).filter(|definition| {
                    definition.ai.hostile && (!basic_only || definition.basic_attack)
                }) {
                    let selected = match &invocation.aim {
                        crate::abilities::AbilityAim::ActorTarget {
                            target: prepared_target,
                        } => *prepared_target == target,
                        crate::abilities::AbilityAim::DirectionalRay { direction } => {
                            let crate::rules::Reach::Sight { range } = definition.reach else {
                                return None;
                            };
                            disclosed_ray_direction(
                                target,
                                sight,
                                view,
                                self.actors.get(&actor)?.body.eye,
                                range,
                            ) == Some(*direction)
                        }
                        crate::abilities::AbilityAim::DirectionalProjectile {
                            direction, ..
                        } => preparation.plan.as_projectile().is_some_and(|plan| {
                            u16::try_from(
                                plan.range.long_feet / crate::weapon_profiles::PROJECTILE_STEP_FEET,
                            )
                            .ok()
                            .and_then(|range| {
                                disclosed_ray_direction(
                                    target,
                                    sight,
                                    view,
                                    self.actors.get(&actor)?.body.eye,
                                    range,
                                )
                            }) == Some(*direction)
                        }),
                        _ => false,
                    };
                    if selected && self.preparation_still_available(preparation, Some(sight)) {
                        return Some(preparation.plan.clone().into());
                    }
                }
            }
        }
        let in_melee = self.attack_available_in_scene(actor, target, sight);
        let mut candidates: Vec<_> = creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                let definition = rules.ability(id)?;
                (definition.ai.hostile && (!basic_only || definition.basic_attack)).then_some((
                    definition.ai.priority,
                    id,
                    &definition.ai,
                    definition.cost.as_ref(),
                ))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, id, hints, cost) in candidates {
            if hints.range_preference == RangePreference::OutsideMelee && in_melee {
                continue;
            }
            if let Some(cost) = cost {
                let available = costs.available(cost.resource);
                let maximum = costs.resources().maximum(cost.resource);
                if !cost
                    .start
                    .checked_add(cost.resolution)
                    .is_some_and(|total| available >= total)
                    || u64::from(available) * 100
                        < u64::from(maximum) * u64::from(hints.minimum_resource_percent)
                {
                    continue;
                }
            }
            let definition = rules.ability(id)?;
            if definition.invocation == crate::abilities::InvocationKind::DirectionalProjectile {
                if let Some(action) =
                    self.choose_ai_projectile_request(actor, target, sight, view, id)
                {
                    return Some(AiAbilityChoice::Request(action));
                }
                continue;
            }
            let invocation = if definition.invocation.is_directional_ray() {
                let crate::rules::Reach::Sight { range } = definition.reach else {
                    continue;
                };
                let Some(direction) = disclosed_ray_direction(
                    target,
                    sight,
                    view,
                    self.actors.get(&actor)?.body.eye,
                    range,
                ) else {
                    continue;
                };
                crate::abilities::AbilityInvocation::directional_ray(direction)
            } else {
                crate::abilities::AbilityInvocation::actor_target(target)
            };
            let Ok(plan) = self.ability_invocation_plan_in_scene(actor, id, invocation, sight)
            else {
                continue;
            };
            if let Some(plan) = self.physical_ai_plan(plan, view) {
                return Some(plan.into());
            }
        }
        if !basic_only {
            return self
                .choose_ai_known_device(actor, target, sight, view)
                .map(Into::into);
        }
        None
    }

    /// A device is a carried capability, not a granted build ability. Restrict
    /// this conservative fallback to explicitly known positive instance charges.
    /// Unknown balances cannot become a free AI-only empty-device oracle.
    fn choose_ai_known_device(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<crate::abilities::Plan> {
        let rows = &view.interactions.as_ref()?.inventory;
        let rules = self.content.rules();
        let mut candidates = Vec::new();
        for disclosed in &view.inventory {
            if !disclosed.identified
                || !rows.iter().any(|row| {
                    row.item == disclosed.id
                        && row.zappable
                        && row.charges.is_some_and(|count| count > 0)
                })
            {
                continue;
            }
            let device = self.items.get(&disclosed.id)?.spec.device.as_ref()?;
            let definition = rules.ability(&device.ability)?;
            if !definition.ai.hostile {
                continue;
            }
            let crate::rules::Reach::Sight { range } = definition.reach else {
                continue;
            };
            let Some(direction) = disclosed_ray_direction(
                target,
                sight,
                view,
                self.actors.get(&actor)?.body.eye,
                range,
            ) else {
                continue;
            };
            candidates.push((definition.ai.priority, disclosed.id, direction));
        }
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        for (_, item, direction) in candidates {
            let work = crate::Work::Zap {
                item,
                aim: crate::DeviceAim::Direction { direction },
            };
            if let Some(pending) = self.preparation(actor).filter(|pending| {
                !pending.active
                    && pending.plan.work() == work
                    && self.preparation_still_available(pending, Some(sight))
            }) {
                return Some(pending.plan.clone());
            }
            if let Ok(plan) = self.plan_work(actor, work) {
                return self.physical_ai_plan(plan, view);
            }
        }
        None
    }

    fn choose_ai_healing(
        &self,
        actor: ActorId,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<crate::abilities::Plan> {
        let creature = self.creature(actor)?;
        let rules = creature.build().ruleset();
        let mut candidates: Vec<_> = creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                rules
                    .ability(id)
                    .filter(|definition| !definition.ai.hostile)
                    .map(|definition| (definition.ai.priority, id, definition))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, id, definition) in candidates {
            let plan = if let Some(pending) = self.preparation(actor).filter(|pending| {
                pending
                    .plan
                    .as_ability()
                    .is_some_and(|plan| &plan.ability == id && plan.target == actor)
                    && self.preparation_still_available(pending, Some(sight))
            }) {
                pending.plan.clone()
            } else {
                // These gates apply only to a fresh preparation. The retained
                // plan already owns its resolution cost and finite-item claims.
                if let Some(cost) = &definition.cost {
                    let available = creature.costs().available(cost.resource);
                    if cost
                        .start
                        .checked_add(cost.resolution)
                        .is_none_or(|total| total > available)
                        || u64::from(available) * 100
                            < u64::from(creature.costs().resources().maximum(cost.resource))
                                * u64::from(definition.ai.minimum_resource_percent)
                    {
                        continue;
                    }
                }
                let Ok(plan) = self.ability_plan_in_scene(actor, id, actor, sight) else {
                    continue;
                };
                plan
            };
            if self.effective_safe_healing_plan(&plan) {
                if let Some(plan) = self.physical_ai_plan(plan, view) {
                    return Some(plan);
                }
            }
        }
        None
    }

    pub(crate) fn known_healing(&self, actor: ActorId, item: crate::ItemId) -> bool {
        let Some(item) = self.items.get(&item) else {
            return false;
        };
        let Some(actor) = self.actors.get(&actor) else {
            return false;
        };
        (!item.spec.concealed || actor.knowledge.contains(&item.spec.identity))
            && item.spec.consumable.as_ref().is_some_and(|consumable| {
                !consumable.effects.is_empty()
                    && consumable
                        .effects
                        .iter()
                        .all(|effect| matches!(effect, crate::rules::Effect::Heal { amount } if match amount {
                            crate::rules::AmountExpr::Fixed { amount } => *amount > 0,
                            crate::rules::AmountExpr::Rolled { dice } => crate::dice::DicePool::new(dice.count, dice.sides, dice.bonus).is_ok_and(|pool| pool.maximum() > 0),
                        }))
            })
    }
}

impl Game {
    fn choose_ai_step_ability(
        &self,
        actor: ActorId,
        direction: Direction,
        sight: &[tor_world::SightCell],
        view: &crate::Observation,
    ) -> Option<crate::abilities::Plan> {
        use crate::abilities::{AbilityInvocation, InvocationKind};
        use crate::rules::RangePreference;
        let creature = self.creature(actor)?;
        let rules = creature.build().ruleset();
        let invocation = AbilityInvocation::self_step(direction.try_into().ok()?);
        let in_melee = view.visible_actors.iter().any(|other| {
            self.perceived_hostile(actor, other.id, other.disclosure)
                && self.attack_available_in_scene(actor, other.id, sight)
        });
        let eligible = |definition: &crate::rules::AbilityDef| {
            definition.invocation == InvocationKind::SelfStep
                && !definition.ai.hostile
                && (definition.ai.range_preference != RangePreference::OutsideMelee || !in_melee)
        };
        // An exact paused selection retains its captured clock, claims and payment.
        if let Some(preparation) = self
            .preparation(actor)
            .filter(|pending| !pending.active && !pending.has_prefix())
        {
            if let Some(plan) = preparation.plan.as_ability() {
                if plan.invocation == invocation
                    && rules.ability(&plan.ability).is_some_and(eligible)
                    && self.preparation_still_available(preparation, Some(sight))
                {
                    return Some(preparation.plan.clone());
                }
            }
        }
        let mut candidates: Vec<_> = creature
            .derived()
            .abilities
            .iter()
            .filter_map(|id| {
                let definition = rules.ability(id)?;
                eligible(definition).then_some((definition.ai.priority, id, definition))
            })
            .collect();
        candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
        for (_, id, definition) in candidates {
            if let Some(cost) = definition.cost.as_ref() {
                let available = creature.costs().available(cost.resource);
                let maximum = creature.costs().resources().maximum(cost.resource);
                if cost
                    .start
                    .checked_add(cost.resolution)
                    .is_none_or(|total| available < total)
                    || u64::from(available) * 100
                        < u64::from(maximum) * u64::from(definition.ai.minimum_resource_percent)
                {
                    continue;
                }
            }
            let Ok(plan) = self.ability_invocation_plan_in_scene(actor, id, invocation, sight)
            else {
                continue;
            };
            if let Some(plan) = self.physical_ai_plan(plan, view) {
                return Some(plan);
            }
        }
        None
    }
}

/// Choose solely from disclosed physical occurrences. Neither first-hit bodies,
/// terrain collision, hidden defenses nor a ray trace participates in this choice.
fn disclosed_ray_direction(
    target: ActorId,
    sight: &[tor_world::SightCell],
    view: &crate::Observation,
    eye: [i32; 3],
    range: u16,
) -> Option<crate::abilities::StepDirection> {
    let target = view.visible_actors.iter().find(|other| {
        other.id == target && other.disclosure == crate::perception::ActorDisclosure::Creature
    })?;
    sight
        .iter()
        .filter(|cell| cell.physical && !cell.wall && cell.location == target.location)
        .filter_map(|cell| {
            let offset = [
                cell.offset.x.checked_add(eye[0])?,
                cell.offset.y.checked_add(eye[1])?,
                cell.offset.z.checked_add(eye[2])?,
            ];
            let distance = offset.iter().map(|value| value.unsigned_abs()).max()?;
            if distance == 0
                || distance > u32::from(range)
                || offset
                    .iter()
                    .any(|value| *value != 0 && value.unsigned_abs() != distance)
            {
                return None;
            }
            let direction = Direction::from_delta(offset.map(|value| i64::from(value.signum())))?;
            let direction = crate::abilities::StepDirection::try_from(direction).ok()?;
            Some((distance, direction))
        })
        .min()
        .map(|(_, direction)| direction)
}

#[path = "ai_defense.rs"]
mod defense;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{combat::DamageType, test_creatures};
    use std::{collections::BTreeSet, num::NonZeroU64};
    use tor_world::{Position, RegionId};
    fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }
    #[test]
    fn invalid_ai_profile_and_missing_creature_have_distinct_errors() {
        let mut game = fixture();
        let before = game.clone();
        assert_eq!(
            game.configure_ai(
                ActorId(2),
                AiProfile {
                    flee_percent: 101,
                    ..AiProfile::general()
                }
            ),
            Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition
            ))
        );
        assert_eq!(
            game.configure_ai(ActorId(999), AiProfile::general()),
            Err(GameError::UnknownActor)
        );
        assert_eq!(game, before);
    }

    fn fixture() -> Game {
        fixture_with_anatomy(crate::AnatomySpec::from_rules(
            &crate::test_creatures::ruleset(),
        ))
    }
    fn fixture_with_anatomy(anatomy: crate::AnatomySpec) -> Game {
        let hero = test_creatures::species();
        let mut foe = test_creatures::species();
        foe.anatomy = anatomy;
        configured_game(hero, foe, paid_probe_rules())
    }
    pub(super) fn configured_game(
        mut hero: crate::creatures::Species,
        mut foe: crate::creatures::Species,
        rules: std::sync::Arc<crate::rules::Ruleset>,
    ) -> Game {
        use crate::{
            attributes::ManaBinding,
            content_ids::SpeciesId,
            creatures::{Content, CreatureBuild, ScenarioCatalog},
            progression::{HdLedger, HdSource, HitDie},
        };
        hero.id = SpeciesId::new("hero").unwrap();
        foe.id = SpeciesId::new("foe").unwrap();
        foe.behavior.memory_ticks = 50;
        let content = Content::new(
            rules,
            ScenarioCatalog {
                species: BTreeMap::from([(hero.id.clone(), hero), (foe.id.clone(), foe)]),
                templates: BTreeMap::new(),
            },
        )
        .unwrap();
        let mut game = Game::two_room_in_stone(42, content.clone());
        for (x, name, faction) in [(1, "hero", "hero"), (2, "foe", "foe")] {
            let actor = game
                .spawn_actor(at(1, x, 1), NonZeroU64::new(100).unwrap())
                .unwrap();
            let build = CreatureBuild::new(
                content.clone(),
                SpeciesId::new(name).unwrap(),
                HdLedger::new(vec![HitDie::new(HdSource::Racial, 42)]).unwrap(),
                ManaBinding::Intellect,
                [
                    tor_simulation::attributes::Skill::Deception,
                    tor_simulation::attributes::Skill::Persuasion,
                ],
            )
            .unwrap();
            game.configure_creature(
                actor,
                crate::CreatureIdentity {
                    name: name.into(),
                    faction: faction.into(),
                },
                build,
            )
            .unwrap();
        }
        game.configure_run(
            ActorId(1),
            BTreeSet::from([ActorId(1)]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        game.configure_ai(ActorId(2), AiProfile::general()).unwrap();
        game.refresh_navigation();
        game
    }
    fn paid_probe_rules() -> std::sync::Arc<crate::rules::Ruleset> {
        let mut document = test_creatures::ruleset().document().clone();
        document.abilities.extend([
            test_creatures::paid_melee(),
            test_creatures::mana_ray(),
            test_creatures::focus_fear(),
        ]);
        crate::rules::Ruleset::validate(document).unwrap()
    }
    fn paid_fixture(magical: bool, fear: bool) -> Game {
        use crate::{
            attributes::{Attributes, ByAttribute},
            content_ids::{AbilityId, DescriptorId, FlagId},
            rules::{Modifier, ModifierOp, Scope, Selector},
        };
        let mut hero = test_creatures::species();
        hero.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Immunity {
                selector: Selector::Descriptor {
                    descriptor: DescriptorId::new("fear").unwrap(),
                },
            },
        });
        let mut foe = test_creatures::species();
        foe.default_attributes = Attributes::new(ByAttribute {
            strength: 2,
            speed: 1,
            intellect: 2,
            willpower: 2,
            awareness: 1,
            presence: 1,
        })
        .unwrap();
        for name in ["fixture_paid_melee", "fixture_mana_ray"]
            .into_iter()
            .chain(fear.then_some("fixture_focus_fear"))
        {
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: AbilityId::new(name).unwrap(),
                },
            });
        }
        if magical {
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Flag {
                    flag: FlagId::new("magical").unwrap(),
                },
            });
        }
        configured_game(hero, foe, paid_probe_rules())
    }
    fn basic(target: ActorId) -> Action {
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            invocation: crate::abilities::AbilityInvocation::actor_target(target),
            supplies: Default::default(),
        }
    }
    fn heal(amount: u32) -> crate::rules::Effect {
        crate::rules::Effect::Heal {
            amount: crate::rules::AmountExpr::Fixed { amount },
        }
    }
    fn injury(game: &mut Game, actor: ActorId, amount: u32) {
        let damage = crate::damage::DamageSpec::new(
            vec![crate::damage::DamageComponent::fixed(
                DamageType::Vital,
                None,
                amount,
            )],
            None,
        )
        .unwrap();
        game.apply_damage(actor, &damage).unwrap();
    }

    #[test]
    fn paid_choices_do_not_read_hidden_fear_immunity() {
        let game = paid_fixture(false, true);
        assert!(game
            .creature(ActorId(1))
            .unwrap()
            .derived()
            .protection
            .has_immunity(&crate::rules::Selector::Descriptor {
                descriptor: crate::content_ids::DescriptorId::new("fear").unwrap()
            }));
        let before = game.clone();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("fixture_focus_fear").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            }
        );
        assert_eq!(game, before);
    }

    #[test]
    fn paid_choices_use_granted_power_strike_without_mutating_state() {
        let game = paid_fixture(false, false);
        let before = game.clone();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("fixture_paid_melee").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            }
        );
        assert_eq!(game, before);
    }

    #[test]
    fn paid_choices_preserve_mana_for_ranged_combat() {
        let mut unfunded = paid_fixture(false, false);
        unfunded.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        assert_eq!(
            unfunded.choose_ai(ActorId(2)).unwrap().action,
            Action::Move(Direction::East)
        );
        let mut game = paid_fixture(true, false);
        game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::UseAbility {
                id: crate::content_ids::AbilityId::new("fixture_mana_ray").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            }
        );
    }
    #[test]
    fn one_decision_shares_its_search_across_visible_targets() {
        let mut game = fixture();
        for actor in [ActorId(1), ActorId(2)] {
            assert!(
                game.creature(actor).is_some(),
                "AI subjects own their builds"
            );
        }
        let other = game
            .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let build = game.creature(ActorId(1)).unwrap().build().clone();
        game.configure_creature(
            other,
            crate::CreatureIdentity {
                name: "other".into(),
                faction: "hero".into(),
            },
            build,
        )
        .unwrap();
        game.refresh_navigation();
        assert_eq!(game.observe(ActorId(2)).unwrap().visible_actors.len(), 2);
        let before = crate::diagnostics::work_counts();
        let AiDecision { action, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(action, basic(ActorId(1)));
        let after = crate::diagnostics::work_counts();
        assert_eq!(after.route_searches - before.route_searches, 1);
        assert_eq!(after.scenes - before.scenes, 1);
    }

    #[test]
    fn hidden_movement_does_not_update_target_memory_and_memory_expires() {
        let mut game = fixture();
        let AiDecision { ai, behavior, .. } = game.choose_ai(ActorId(2)).unwrap();
        let expected = Some(crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1, 1, 1),
            last_seen: 0,
        });
        assert_eq!(behavior.engagement, expected);
        game.actors.get_mut(&ActorId(2)).unwrap().behavior = behavior;
        game.combat.ai.insert(ActorId(2), ai);
        game.set_region_light(RegionId(2), false).unwrap();
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        let AiDecision {
            behavior: remembered,
            ..
        } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(remembered.engagement, expected);
        game.tick = 50;
        let AiDecision {
            ai,
            behavior: expired,
            ..
        } = game.choose_ai(ActorId(2)).unwrap();
        assert!(expired.engagement.is_none());
        assert_eq!(ai.state, State::Search);
    }
    #[test]
    fn wounded_ai_flees_and_a_cornered_ai_fights() {
        let mut game = fixture();
        injury(&mut game, ActorId(2), 25);
        let AiDecision { action, ai, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Flee);
        assert!(matches!(action, Action::Move(_)));
        for (x, y) in [(1, 0), (1, 2), (2, 0), (2, 2), (3, 0), (3, 1), (3, 2)] {
            game.set_wall(at(1, x, y), true).unwrap();
        }
        let AiDecision { action, ai, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Flee);
        assert_eq!(action, basic(ActorId(1)));
    }

    fn ring(game: &mut Game, item: u64, defense: i32, concealed: bool) {
        let mut spec = crate::ItemSpec::ordinary(format!("ring-{item}"));
        spec.class = crate::ItemClass::Ring;
        spec.concealed = concealed;
        spec.appearance = "silver ring".into();
        spec.equipment = Some(crate::EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            armor: None,
            slot: crate::EquipmentSlot::new("ring").unwrap(),
            attack: None,
            modifiers: vec![crate::rules::Modifier {
                cancellable: false,
                scope: crate::rules::Scope::Always,
                op: crate::rules::ModifierOp::Add {
                    stat: crate::rules::Stat::Defense {
                        defense: crate::rules::DefenseKind::Physical,
                    },
                    value: i64::from(defense),
                },
            }],
        });
        game.place_item_stack(item, at(1, 2, 1), Some(ActorId(2)), 1, spec)
            .unwrap();
    }

    #[test]
    fn loot_range_and_safe_first_step_are_checked_on_remembered_routes() {
        use tor_world::{Extent, Region, World};
        let mut world = World::new(vec![], vec![]).unwrap();
        world
            .add_region(Region {
                id: RegionId(1),
                name: "arena".into(),
                bounds: Extent::new(10, 5, 1).unwrap(),
            })
            .unwrap();
        let mut game = Game::new(world, 42, test_creatures::content());
        let human = game
            .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        let actor = game
            .spawn_actor(at(1, 1, 2), NonZeroU64::new(100).unwrap())
            .unwrap();
        for (id, faction) in [(human, "hero"), (actor, "foe")] {
            test_creatures::configure(&mut game, id, faction, test_creatures::species());
        }
        game.configure_run(
            human,
            BTreeSet::from([human]),
            None,
            BTreeMap::from([("foe".into(), BTreeSet::from(["hero".into()]))]),
        )
        .unwrap();
        let mut potion = crate::ItemSpec::ordinary("healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(10)],
        });
        game.place_item_stack(20, at(1, 4, 2), None, 1, potion.clone())
            .unwrap();
        game.place_item_stack(21, at(1, 1, 0), None, 1, potion.clone())
            .unwrap();
        game.refresh_navigation();
        let view = game.observe(actor).unwrap();
        let mut search = game.route_search(actor).unwrap();
        let mut route = |at| search.route(at).ok();
        // The eastward first step is adjacent to the hostile; the northward
        // alternative remains safe and must still be considered.
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &AiProfile::general(), &[], &mut route),
            Some(Action::Move(Direction::North))
        );
        game.teleport(human, at(1, 8, 2)).unwrap();
        game.place_item_stack(22, at(1, 5, 2), None, 1, potion)
            .unwrap();
        game.refresh_navigation();
        let mut view = game.observe(actor).unwrap();
        view.ground_items
            .retain(|item| item.id == crate::ItemId(22));
        let mut search = game.route_search(actor).unwrap();
        assert_eq!(
            game.choose_nearby_loot(actor, &view, &AiProfile::general(), &[], &mut |at| search
                .route(at)
                .ok()),
            None
        );
    }

    #[test]
    fn nearby_loot_uses_own_knowledge_and_takes_only_one_unit() {
        let mut game = fixture();
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        let mut potion = crate::ItemSpec::ordinary("healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.stackable = true;
        potion.concealed = true;
        potion.appearance = "red potion".into();
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(10)],
        });
        game.place_item_stack(20, at(1, 2, 1), None, 3, potion)
            .unwrap();
        game.identify_item(ActorId(1), crate::ItemId(20)).unwrap();
        assert!(!matches!(
            game.choose_ai(actor).unwrap().action,
            Action::Take { .. }
        ));
        game.identify_item(actor, crate::ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Take {
                item: crate::ItemId(20),
                quantity: Some(1)
            }
        );
    }

    #[test]
    fn nearby_loot_routes_share_one_search_and_do_not_step_beside_hostiles() {
        let mut game = fixture();
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(1, 5, 1)).unwrap();
        for (id, x) in [(20, 3), (21, 4)] {
            let mut potion = crate::ItemSpec::ordinary(format!("healing {id}"));
            potion.class = crate::ItemClass::Potion;
            potion.consumable = Some(crate::ConsumableSpec {
                descriptors: vec![],
                discovery: crate::ItemDiscovery::SelfEffect,
                effects: vec![heal(10)],
            });
            game.place_item_stack(id, at(1, x, 1), None, 1, potion)
                .unwrap();
        }
        game.refresh_navigation();
        let before = crate::diagnostics::work_counts().route_searches;
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Move(Direction::East)
        );
        assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
        game.teleport(actor, at(1, 3, 1)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Move(Direction::East),
            "engagement takes priority over even an underfoot healing reserve"
        );
        let mut unengaged = game.clone();
        unengaged.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        unengaged.set_region_light(RegionId(2), false).unwrap();
        assert_eq!(
            unengaged.choose_ai(actor).unwrap().action,
            Action::Take {
                item: crate::ItemId(20),
                quantity: Some(1)
            }
        );
        game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
    }

    #[test]
    fn equipment_uses_duplicate_anatomy_sockets_and_only_the_ais_known_stats() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 2],
        });
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        ring(&mut game, 10, 1, false);
        ring(&mut game, 11, 2, false);
        ring(&mut game, 12, 3, true);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        let expected = Action::Equip {
            item: crate::ItemId(11),
            slot: crate::EquipmentSlotId(1),
        };
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, expected);
        game.identify_item(ActorId(1), crate::ItemId(12)).unwrap();
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, expected);
        game.identify_item(ActorId(2), crate::ItemId(12)).unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Equip {
                item: crate::ItemId(12),
                slot: crate::EquipmentSlotId(1)
            }
        );
    }

    #[test]
    fn gear_decisions_do_not_read_hidden_curses_and_stop_after_observed_failure() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("ring").unwrap()],
        });
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        ring(&mut game, 10, 1, false);
        ring(&mut game, 11, 2, false);
        game.equip_authored(actor, crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        let action = game.choose_ai(actor).unwrap().action;
        assert_eq!(
            action,
            Action::Unequip {
                item: crate::ItemId(10)
            }
        );
        game.items
            .edit(crate::ItemId(10), |item| item.condition.cursed = true)
            .unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            action,
            "Hidden curse changes cannot alter the decision"
        );
        assert!(game
            .known_equipment_view(actor, crate::ItemId(10))
            .is_none());
        assert_eq!(
            game.actors[&actor].known_equipment.curse(crate::ItemId(10)),
            None
        );
        game.act(ActorId(1), Action::Wait).unwrap();
        game.act(actor, action).unwrap();
        while game.preparation(actor).is_some() {
            let next = game.next_actor().unwrap();
            game.act(next, Action::Wait).unwrap();
        }
        assert_eq!(
            game.actors[&actor].known_equipment.curse(crate::ItemId(10)),
            Some(true)
        );
        assert_eq!(
            game.equipment(actor)
                .unwrap()
                .get(&crate::EquipmentSlotId(0)),
            Some(&crate::ItemId(10))
        );
        assert!(game
            .known_equipment_view(actor, crate::ItemId(10))
            .is_none());
        assert!(!matches!(
            game.choose_ai(actor).unwrap().action,
            Action::Unequip { .. }
        ));
    }

    #[test]
    fn ordinary_gear_estimates_ignore_hidden_bonuses_without_disclosing_numbers() {
        let slot = crate::EquipmentSlot::new("body_armor").unwrap();
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![slot.clone()],
        });
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        for (id, reduction) in [(10, 1), (11, 2)] {
            let mut spec = crate::ItemSpec::ordinary(format!("armor-{id}"));
            spec.class = crate::ItemClass::Armor;
            spec.equipment = Some(crate::EquipmentSpec {
                periodic_relocation: None,
                potency_modifiers: Vec::new(),
                weapon_profile: None,
                slot: slot.clone(),
                armor: Some(crate::ArmorSpec {
                    physical_reduction: reduction,
                    material: crate::ArmorMaterial::Metal,
                    rustproof: false,
                }),
                attack: None,
                modifiers: vec![],
            });
            game.place_item_stack(id, at(1, 2, 1), Some(actor), 1, spec)
                .unwrap();
        }
        game.equip_authored(actor, crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        // Authored placement supplies no inspection witness for either instance.
        assert!(game
            .known_equipment_view(actor, crate::ItemId(10))
            .is_none());
        assert!(game
            .known_equipment_view(actor, crate::ItemId(11))
            .is_none());
        let expected = Action::Unequip {
            item: crate::ItemId(10),
        };
        assert_eq!(game.choose_ai(actor).unwrap().action, expected);
        let mut hidden = game.clone();
        hidden
            .items
            .edit(crate::ItemId(10), |item| {
                item.condition.cursed = true;
                item.condition.armor_enchantment = 10;
            })
            .unwrap();
        hidden
            .items
            .edit(crate::ItemId(11), |item| {
                item.condition.armor_penalty = 2;
            })
            .unwrap();
        assert_eq!(hidden.choose_ai(actor).unwrap().action, expected);
        assert!(hidden
            .known_equipment_view(actor, crate::ItemId(10))
            .is_none());
        assert!(hidden
            .known_equipment_view(actor, crate::ItemId(11))
            .is_none());
        // Once observed, actual instance facts replace the ordinary estimate.
        hidden.identify_item(actor, crate::ItemId(11)).unwrap();
        assert!(!matches!(
            hidden.choose_ai(actor).unwrap().action,
            Action::Unequip { .. }
        ));
    }

    #[test]
    fn gear_removal_chooses_an_upgrade_and_does_not_reequip_the_dominated_old_item() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 1],
        });
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        game.set_region_light(RegionId(2), false).unwrap();
        ring(&mut game, 10, 1, false);
        ring(&mut game, 11, 2, false);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Unequip {
                item: crate::ItemId(10)
            }
        );
        game.act(ActorId(1), Action::Wait).unwrap();
        game.act(
            ActorId(2),
            Action::Unequip {
                item: crate::ItemId(10),
            },
        )
        .unwrap();
        while game.preparation(ActorId(2)).is_some() {
            let next = game.next_actor().unwrap();
            game.act(next, Action::Wait).unwrap();
        }
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Equip {
                item: crate::ItemId(11),
                slot: crate::EquipmentSlotId(0)
            }
        );
    }

    #[test]
    fn gear_work_does_not_replace_unknown_equipment_or_start_beside_a_visible_hostile() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            capacities: Default::default(),
            primary_attack_socket: crate::equipment_capacity::PrimaryAttackSocket::None,
            slots: vec![crate::EquipmentSlot::new("ring").unwrap(); 1],
        });
        ring(&mut game, 10, 1, true);
        ring(&mut game, 11, 2, false);
        game.equip_authored(ActorId(2), crate::ItemId(10), crate::EquipmentSlotId(0))
            .unwrap();
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            basic(ActorId(1))
        );
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        assert!(!matches!(
            game.choose_ai(ActorId(2)).unwrap().action,
            Action::Unequip { .. } | Action::Equip { .. }
        ));
    }

    #[test]
    fn healing_decisions_require_the_ai_actors_own_identity_knowledge() {
        let mut game = fixture();
        let actor = ActorId(2);
        let mut potion = crate::ItemSpec::ordinary("healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.concealed = true;
        potion.appearance = "red potion".into();
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(10)],
        });
        game.place_item_stack(20, at(1, 2, 1), Some(actor), 1, potion)
            .unwrap();
        injury(&mut game, actor, 15);
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
        game.identify_item(ActorId(1), crate::ItemId(20)).unwrap();
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
        game.identify_item(actor, crate::ItemId(20)).unwrap();
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Drink {
                item: crate::ItemId(20)
            }
        );
    }

    #[test]
    fn healing_priority_does_not_drink_known_mixed_harmful_effects_or_waste_full_health() {
        let mut game = fixture();
        let actor = ActorId(2);
        let mut potion = crate::ItemSpec::ordinary("mixed".into());
        potion.class = crate::ItemClass::Potion;
        potion.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![
                heal(10),
                crate::rules::Effect::Damage {
                    damage: crate::rules::DamageExpr::Components {
                        components: vec![crate::rules::DamageComponentExpr {
                            category: DamageType::Vital,
                            descriptor: None,
                            amount: crate::rules::AmountExpr::Fixed { amount: 1 },
                        }],
                    },
                },
            ],
        });
        game.place_item_stack(20, at(1, 2, 1), Some(actor), 1, potion)
            .unwrap();
        injury(&mut game, actor, 15);
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
        let mut healing = crate::ItemSpec::ordinary("healing".into());
        healing.class = crate::ItemClass::Potion;
        healing.consumable = Some(crate::ConsumableSpec {
            descriptors: vec![],
            discovery: crate::ItemDiscovery::SelfEffect,
            effects: vec![heal(20)],
        });
        game.place_item_stack(21, at(1, 2, 1), Some(actor), 1, healing)
            .unwrap();
        game.apply_effects(actor, &[heal(30)]).unwrap();
        assert_eq!(game.choose_ai(actor).unwrap().action, basic(ActorId(1)));
        injury(&mut game, actor, 25);
        assert_eq!(
            game.choose_ai(actor).unwrap().action,
            Action::Drink {
                item: crate::ItemId(21)
            }
        );
    }
    fn data_ability_game(reverse: bool) -> Game {
        use crate::{
            content_ids::{AbilityId, FlagId},
            rules::{Modifier, ModifierOp, RangePreference, Ruleset, Scope},
        };
        let rules = paid_probe_rules();
        let mut document = rules.document().clone();
        let base = rules
            .ability(&AbilityId::new("fixture_mana_ray").unwrap())
            .unwrap();
        let mut foe = test_creatures::species();
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Flag {
                flag: FlagId::new("magical").unwrap(),
            },
        });
        for name in ["custom_arc", "custom_zap"] {
            let mut definition = base.clone();
            definition.id = AbilityId::new(name).unwrap();
            definition.name = name.into();
            definition.ai.priority = 100;
            definition.ai.range_preference = RangePreference::Any;
            definition.cost = None;
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: definition.id.clone(),
                },
            });
            document.abilities.push(definition);
        }
        for index in 0..8 {
            let mut definition = base.clone();
            definition.id = AbilityId::new(format!("unfunded_{index}")).unwrap();
            definition.name = format!("Unfunded {index}");
            definition.ai.priority = 200;
            definition.ai.range_preference = RangePreference::Any;
            definition.cost = Some(crate::rules::Cost {
                resource: crate::resources::Resource::Mana,
                start: 100,
                resolution: 100,
            });
            foe.modifiers.push(Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: definition.id.clone(),
                },
            });
            document.abilities.push(definition);
        }
        if reverse {
            document.abilities.reverse();
        }
        configured_game(
            test_creatures::species(),
            foe,
            Ruleset::validate(document).unwrap(),
        )
    }

    #[test]
    fn authored_abilities_use_priority_and_stable_ids_with_one_sight_scene_per_decision() {
        for reverse in [false, true] {
            let mut game = data_ability_game(reverse);
            game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
            let before = crate::diagnostics::work_counts();
            let AiDecision { action, .. } = game.choose_ai(ActorId(2)).unwrap();
            let after = crate::diagnostics::work_counts();
            assert_eq!(
                action,
                Action::UseAbility {
                    id: crate::content_ids::AbilityId::new("custom_arc").unwrap(),
                    invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                    supplies: Default::default(),
                }
            );
            assert_eq!(after.scenes - before.scenes, 1);
            assert_eq!(after.route_searches - before.route_searches, 1);
        }
    }

    #[test]
    fn admitted_ai_executes_one_captured_plan_from_one_decision_scene() {
        let mut game = data_ability_game(false);
        game.teleport(ActorId(1), at(1, 4, 1)).unwrap();
        game.act(ActorId(1), Action::Wait).unwrap();
        assert_eq!(game.next_actor(), Some(ActorId(2)));
        let before_admission = crate::diagnostics::work_counts();
        game.admit_ai_intention(ActorId(2)).unwrap();
        let before = crate::diagnostics::work_counts();
        assert_eq!(before.scenes, before_admission.scenes);
        assert_eq!(before.ability_plans, before_admission.ability_plans);
        let execution = game.execute_next_intention().unwrap();
        let after = crate::diagnostics::work_counts();
        assert_eq!(
            execution.action,
            Some(Action::UseAbility {
                id: crate::content_ids::AbilityId::new("custom_arc").unwrap(),
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default(),
            })
        );
        execution.outcome.unwrap();
        assert_eq!(
            after.ability_plans - before.ability_plans,
            1,
            "unfunded candidates are filtered before planning and selected operands are reused"
        );
        assert_eq!(after.route_searches - before.route_searches, 1);
        assert_eq!(after.decision_scenes - before.decision_scenes, 1);
        assert_eq!(
            after.ai_precommit_ordinary_scenes - before.ai_precommit_ordinary_scenes,
            0,
            "no ordinary scene may be built before consuming the chosen plan"
        );
        // Stationary sight/reach and source-origin queries reuse the caster's
        // decision scene. Only the teleported target's participant disclosure
        // builds an ordinary scene, after commitment. Count actual builds,
        // including cold snapshot misses, rather than cached scene requests.
        assert_eq!(after.ordinary_scenes - before.ordinary_scenes, 1);
        assert_eq!(after.scenes - before.scenes, 2);
        assert_eq!(
            after.scenes - before.scenes,
            (after.decision_scenes - before.decision_scenes)
                + (after.ordinary_scenes - before.ordinary_scenes)
        );
        assert!(
            game.disclosed_event_history(ActorId(1))
                .iter()
                .any(|event| {
                    matches!(event, crate::combat::DisclosedCombatEvent::Ability {
                caster: Some(ActorId(2)),
                caster_reference: Some(crate::perception::ActorReferenceKind::Creature),
                target: Some(ActorId(1)),
                id,
                ..
            } if id.as_str() == "custom_arc")
                }),
            "the shared scenes retain the target's identified participant feedback"
        );
    }

    #[test]
    fn tactile_awareness_does_not_count_as_illuminated_sight_for_ranged_abilities() {
        let mut game = data_ability_game(false);
        game.set_region_light(RegionId(1), false).unwrap();
        let before = crate::diagnostics::work_counts();
        let snapshot = game.decision_snapshot(ActorId(2)).unwrap();
        assert!(snapshot
            .view
            .visible_actors
            .iter()
            .any(|actor| actor.id == ActorId(1)));
        assert!(!snapshot
            .sight
            .iter()
            .any(|cell| cell.location == at(1, 1, 1)));
        assert_eq!(crate::diagnostics::work_counts().scenes - before.scenes, 1);
        assert_eq!(
            game.choose_ai(ActorId(2)).unwrap().action,
            basic(ActorId(1))
        );
    }
    /// Exercise the shipped paid W05 definition rather than a colliding clone.
    fn public_plan_ai_rules() -> std::sync::Arc<crate::rules::Ruleset> {
        test_creatures::ruleset()
    }
    #[test]
    fn public_plan_ai_uses_authored_policy_then_spends_readied_on_a_real_attack() {
        use crate::{
            content_ids::AbilityId,
            resources::Resource,
            rules::{Modifier, ModifierOp, Scope, Stat},
        };
        let rules = public_plan_ai_rules();
        assert_eq!(
            rules
                .document()
                .resources
                .iter()
                .find(|definition| definition.resource == Resource::Focus)
                .unwrap()
                .recovery_period,
            300
        );
        let observation = AbilityId::new("read_the_rhythm").unwrap();
        let definition = rules
            .ability(&observation)
            .expect("ordinary authored W05 definition");
        assert_eq!(definition.cost.as_ref().unwrap().resource, Resource::Focus);
        assert_eq!(
            definition.cost.as_ref().unwrap().start + definition.cost.as_ref().unwrap().resolution,
            2
        );
        assert!(definition.ai.hostile);
        assert_eq!(definition.ai.minimum_resource_percent, 100);
        let mut foe = test_creatures::species();
        foe.modifiers.extend([
            Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: observation.clone(),
                },
            },
            Modifier {
                cancellable: false,
                scope: Scope::Always,
                op: ModifierOp::Set {
                    stat: Stat::ResourceMax {
                        resource: Resource::Focus,
                    },
                    value: 2,
                },
            },
        ]);
        let mut game = configured_game(test_creatures::species(), foe, rules);
        game.act(ActorId(1), Action::Wait).unwrap();
        let decision = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(
            decision.action,
            Action::UseAbility {
                id: observation,
                invocation: crate::abilities::AbilityInvocation::actor_target(ActorId(1)),
                supplies: Default::default()
            }
        );
        let plan = decision.plan.unwrap();
        assert_eq!(
            (plan.preparation(), plan.as_ability().unwrap().recovery),
            (100, 100)
        );
        game.admit_ai_intention(ActorId(2)).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(game.tick(), 100);
        let remembered = game.public_plan_observation(ActorId(2)).unwrap().clone();
        assert_eq!(remembered.observed_at, 100);
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .costs()
                .resources()
                .balance(Resource::Focus),
            0
        );
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .conditions()
                .instances()
                .filter(|entry| entry.condition.as_str() == "readied")
                .count(),
            1
        );
        for _ in 0..3 {
            if game.next_actor() == Some(ActorId(2)) {
                break;
            }
            game.act(ActorId(1), Action::Wait).unwrap();
        }
        assert_eq!(game.tick(), 200);
        assert_eq!(game.next_actor(), Some(ActorId(2)));
        let attack = game.choose_ai(ActorId(2)).unwrap();
        let plan = attack.plan.as_ref().unwrap().as_ability().unwrap();
        assert!(game
            .content()
            .rules()
            .ability(&plan.ability)
            .unwrap()
            .tags
            .iter()
            .any(|tag| tag.as_str() == "attack"));
        assert_eq!(plan.tokens.len(), 1);
        assert_eq!(
            plan.tokens[0]
                .reservation
                .qualification
                .as_ref()
                .unwrap()
                .opponent,
            ActorId(1)
        );
        game.admit_ai_intention(ActorId(2)).unwrap();
        game.execute_next_intention().unwrap().outcome.unwrap();
        for _ in 0..4 {
            if game.preparation(ActorId(2)).is_none() {
                break;
            }
            game.act(ActorId(1), Action::Wait).unwrap();
        }
        assert!(game.preparation(ActorId(2)).is_none());
        assert!(game
            .creature(ActorId(2))
            .unwrap()
            .conditions()
            .instances()
            .all(|entry| entry.condition.as_str() != "readied"));
        assert_eq!(game.tick(), 300);
        assert_eq!(
            game.creature(ActorId(2))
                .unwrap()
                .costs()
                .resources()
                .balance(Resource::Focus),
            1
        );
        assert_eq!(game.public_plan_observation(ActorId(2)), Some(&remembered));
    }
    #[test]
    fn public_plan_ai_choice_is_equal_for_publicly_equal_idle_and_private_work() {
        use crate::{
            content_ids::AbilityId,
            rules::{Modifier, ModifierOp, Scope},
        };
        let mut foe = test_creatures::species();
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::GrantAbility {
                ability: AbilityId::new("read_the_rhythm").unwrap(),
            },
        });
        let mut idle = configured_game(test_creatures::species(), foe, public_plan_ai_rules());
        let hero = ActorId(1);
        let observer = ActorId(2);
        let slot = idle
            .actors
            .get(&hero)
            .unwrap()
            .anatomy
            .slots
            .iter()
            .position(|slot| slot.as_str() == "body_armor")
            .unwrap();
        let mut item = crate::ItemSpec::ordinary("private armor".into());
        item.class = crate::ItemClass::Armor;
        item.equipment = Some(crate::EquipmentSpec {
            periodic_relocation: None,
            potency_modifiers: Vec::new(),
            weapon_profile: None,
            armor: None,
            slot: crate::EquipmentSlot::new("body_armor").unwrap(),
            attack: None,
            modifiers: vec![],
        });
        let item = idle
            .place_item_stack(20, at(1, 1, 1), Some(hero), 1, item)
            .unwrap();
        let mut private = idle.clone();
        idle.act(hero, Action::Wait).unwrap();
        private
            .admit_intention(
                hero,
                Action::Equip {
                    item,
                    slot: crate::EquipmentSlotId(slot as u16),
                },
                crate::IntentionOrigin::Human,
            )
            .unwrap();
        private.execute_next_intention().unwrap().outcome.unwrap();
        assert_eq!(idle.tick(), private.tick());
        assert_eq!(
            idle.observe(observer).unwrap(),
            private.observe(observer).unwrap(),
            "same authorized facts precede the decision"
        );
        let idle_choice = idle.choose_ai(observer).unwrap();
        let private_choice = private.choose_ai(observer).unwrap();
        assert_eq!(idle_choice.action, private_choice.action);
        assert_eq!(idle_choice.plan, private_choice.plan);
        assert!(
            matches!(idle_choice.action, Action::UseAbility { ref id, invocation: crate::abilities::AbilityInvocation { aim: crate::abilities::AbilityAim::ActorTarget { target }, .. }, .. } if id.as_str() == "read_the_rhythm" && target == hero)
        );
    }
    #[test]
    fn erratic_move_can_replace_a_fresh_adjacent_attack_but_preview_does_not_draw() {
        use crate::locomotion::{LocomotionValue, MovementChance};
        use crate::rules::{Modifier, ModifierOp, Scope};
        let mut foe = test_creatures::species();
        let ordinary = foe.clone();
        foe.modifiers.push(Modifier {
            cancellable: false,
            scope: Scope::Always,
            op: ModifierOp::Locomotion {
                value: LocomotionValue::Erratic {
                    chance: Some(MovementChance {
                        numerator: 1,
                        denominator: 1,
                    }),
                },
            },
        });
        let mut game = configured_game(test_creatures::species(), foe, paid_probe_rules());
        game.configure_ai(
            ActorId(2),
            AiProfile {
                idle: crate::behavior::IdleBehavior::Wait,
                flee_percent: 0,
                ..Default::default()
            },
        )
        .unwrap();
        let engagement = Some(crate::behavior::Engagement {
            reference: crate::perception::ActorReferenceKind::Creature,
            target: ActorId(1),
            location: at(1, 1, 1),
            last_seen: game.actor_clock(ActorId(2)),
        });
        game.actors
            .get_mut(&ActorId(2))
            .unwrap()
            .behavior
            .engagement = engagement;
        let before = game.clone();
        let chosen = game.choose_ai(ActorId(2)).unwrap();
        assert!(matches!(chosen.action, Action::Move(_)));
        assert!(chosen.plan.is_none());
        assert_ne!(chosen.behavior.rng, before.actors[&ActorId(2)].behavior.rng);
        assert_eq!(game.choose_ai(ActorId(2)).unwrap().action, chosen.action);
        assert_eq!(game, before);
        // The same fixture without the modifier chooses its ordinary adjacent attack.
        let mut ordinary = configured_game(test_creatures::species(), ordinary, paid_probe_rules());
        ordinary
            .configure_ai(
                ActorId(2),
                AiProfile {
                    idle: crate::behavior::IdleBehavior::Wait,
                    flee_percent: 0,
                    ..Default::default()
                },
            )
            .unwrap();
        ordinary
            .actors
            .get_mut(&ActorId(2))
            .unwrap()
            .behavior
            .engagement = engagement;
        assert!(matches!(
            ordinary.choose_ai(ActorId(2)).unwrap().action,
            Action::UseAbility { .. }
        ));
    }
}

#[cfg(test)]
#[path = "ai_defense_tests.rs"]
mod defense_tests;

#[cfg(test)]
mod wide_scalar_ratio_tests {
    use super::low_health;
    #[test]
    fn wide_scalar_ai_health_thresholds_are_exact_near_u64_maximum() {
        for maximum in [u64::from(u32::MAX) + 1, (1_u64 << 63), u64::MAX] {
            for percent in [0, 25, 50, 100] {
                let boundary =
                    u64::try_from(u128::from(maximum) * u128::from(percent) / 100).unwrap();
                assert!(low_health(boundary, maximum, percent));
                if let Some(next) = boundary.checked_add(1) {
                    assert!(!low_health(next, maximum, percent));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "ai_idle_tests.rs"]
mod idle_tests;

#[cfg(test)]
#[path = "ai_ray_tests.rs"]
mod ray_tests;
