//! Decisions use disclosed observations and remembered navigation, never hidden targets.
use crate::{Action, ActorId, Game, GameError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tor_world::{Direction, Location};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AiProfile {
    pub memory_ticks: u64,
    pub flee_percent: u32,
}
impl Default for AiProfile {
    fn default() -> Self {
        Self {
            memory_ticks: 1000,
            flee_percent: 25,
        }
    }
}
impl AiProfile {
    pub fn valid(&self) -> bool {
        self.memory_ticks <= 1_000_000 && self.flee_percent <= 100
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum State {
    #[default]
    Search,
    Attack,
    Flee,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Ai {
    pub profile: AiProfile,
    pub state: State,
    pub target: Option<(ActorId, Location, u64)>,
    #[serde(with = "visit_counts")]
    pub visits: BTreeMap<Location, u64>,
}

// JSON object keys cannot represent a region-local location. Ordered entries also
// let the reader reject duplicate locations instead of silently overwriting them.
mod visit_counts {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(
        visits: &BTreeMap<Location, u64>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        visits.iter().collect::<Vec<_>>().serialize(serializer)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<Location, u64>, D::Error> {
        let entries = Vec::<(Location, u64)>::deserialize(deserializer)?;
        let mut visits = BTreeMap::new();
        for (location, count) in entries {
            if visits.insert(location, count).is_some() {
                return Err(serde::de::Error::custom("Duplicate AI visit location"));
            }
        }
        Ok(visits)
    }
}

/// An uninterrupted decision owns evaluated ability operands through commit.
pub(crate) struct AiDecision {
    pub action: Action,
    pub ai: Ai,
    pub plan: Option<crate::abilities::Plan>,
}
impl AiDecision {
    fn action(action: Action, ai: Ai) -> Self {
        Self {
            action,
            ai,
            plan: None,
        }
    }
    fn ability(plan: crate::abilities::Plan, ai: Ai) -> Self {
        Self {
            action: plan.work.action(),
            ai,
            plan: Some(plan),
        }
    }
}

// Priority-ordered transition table: frightened, target known, resulting state.
const TRANSITIONS: [(bool, bool, State); 4] = [
    (true, true, State::Flee),
    (false, true, State::Attack),
    (true, false, State::Search),
    (false, false, State::Search),
];

impl Game {
    pub fn configure_ai(&mut self, actor: ActorId, profile: AiProfile) -> Result<(), GameError> {
        if !profile.valid() {
            return Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition,
            ));
        }
        if self.health(actor).is_none() {
            return Err(GameError::UnknownActor);
        }
        self.combat.ai.insert(
            actor,
            Ai {
                profile,
                state: State::Search,
                target: None,
                visits: BTreeMap::new(),
            },
        );
        Ok(())
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
        let snapshot = self.decision_snapshot(id).ok()?;
        let view = &snapshot.view;
        let visits = ai.visits.entry(view.location).or_default();
        *visits = visits.saturating_add(1);
        let mut search = self.route_search(id).ok();
        let mut route = |destination| search.as_mut()?.route(destination).ok();
        let target = view
            .visible_actors
            .iter()
            .filter(|a| self.hostile(id, a.id))
            .min_by_key(|a| (route(a.location).map_or(usize::MAX, |r| r.len()), a.id));
        if let Some(target) = target {
            ai.target = Some((target.id, target.location, self.tick));
        } else if ai.target.is_some_and(|(_, at, seen)| {
            self.tick.saturating_sub(seen) >= ai.profile.memory_ticks || view.location == at
        }) {
            ai.target = None;
        }
        let (hp, max) = self.health(id)?;
        let frightened = u64::from(hp) * 100 <= u64::from(max) * u64::from(ai.profile.flee_percent);
        ai.state = TRANSITIONS
            .iter()
            .find(|(low, known, _)| *low == frightened && *known == ai.target.is_some())
            .unwrap()
            .2;
        // Use a known restorative before fleeing or attacking. Unknown potions
        // and mixed sequences with harm never qualify as safe healing.
        if u64::from(hp) * 2 <= u64::from(max) {
            if let Some(item) = view
                .inventory
                .iter()
                .find(|item| self.known_healing(id, item.id))
            {
                return Some(AiDecision::action(Action::Drink { item: item.id }, ai));
            }
        }
        if ai.state != State::Flee && !self.exposed_to_visible_hostile(id, view.location, view) {
            if let Some(action) = self.choose_gear(id, view) {
                return Some(AiDecision::action(action, ai));
            }
            if let Some(action) = self.choose_nearby_loot(id, view, &mut route) {
                return Some(AiDecision::action(action, ai));
            }
        }
        if ai.state == State::Flee {
            let (_, threat, _) = ai.target.unwrap();
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
                return Some(AiDecision::action(Action::Move(direction), ai));
            }
        }
        if let Some((target, at, _)) = ai.target {
            // Remembered positions support pursuit, but only a currently
            // disclosed target can prompt a paid technique.
            if ai.state != State::Flee && view.visible_actors.iter().any(|actor| actor.id == target)
            {
                if let Some(ability) = self.choose_ai_ability(id, target, &snapshot.sight, false) {
                    return Some(AiDecision::ability(ability, ai));
                }
            }
            if ai.state == State::Flee {
                if let Some(ability) = self.choose_ai_ability(id, target, &snapshot.sight, true) {
                    return Some(AiDecision::ability(ability, ai));
                }
            }
            if ai.state != State::Flee {
                if let Some(step) = route(at).and_then(|r| r.first().copied()) {
                    if self.actor_translation(id, step.direction).is_some() {
                        return Some(AiDecision::action(Action::Move(step.direction), ai));
                    }
                }
                ai.target = None;
                ai.state = State::Search;
            }
        }
        if ai.state == State::Search {
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
                ));
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
                return Some(AiDecision::action(Action::Move(*direction), ai));
            }
        }
        Some(AiDecision::action(Action::Wait, ai))
    }

    /// Default combat policy uses personal capabilities and resources, never
    /// enemy defenses, immunity or condition state. Choices reserve nothing;
    /// ordinary execution rechecks funding and reach before preparation.
    fn choose_ai_ability(
        &self,
        actor: ActorId,
        target: ActorId,
        sight: &[tor_world::SightCell],
        basic_only: bool,
    ) -> Option<crate::abilities::Plan> {
        use crate::rules::RangePreference;
        let creature = self.creature(actor)?;
        let costs = creature.costs();
        let rules = creature.build().ruleset();
        if let Some(preparation) = self
            .preparation(actor)
            .filter(|preparation| !preparation.active)
        {
            if let crate::Work::UseAbility {
                id,
                target: prepared_target,
            } = &preparation.plan.work
            {
                let eligible = rules.ability(id).is_some_and(|definition| {
                    definition.ai.hostile && (!basic_only || definition.basic_attack)
                });
                let funded = preparation.plan.cost.is_none_or(|cost| {
                    preparation
                        .hold
                        .is_some_and(|owner| creature.validate_cost(owner, cost).is_ok())
                });
                if *prepared_target == target
                    && eligible
                    && funded
                    && self.plan_still_available_in_scene(&preparation.plan, sight)
                {
                    return Some(preparation.plan.clone());
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
            let Ok(plan) = self.ability_plan_in_scene(actor, id, target, sight) else {
                continue;
            };
            return Some(plan);
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
                    ..AiProfile::default()
                }
            ),
            Err(GameError::Rule(
                crate::abilities::RuleError::InvalidDefinition
            ))
        );
        assert_eq!(
            game.configure_ai(ActorId(999), AiProfile::default()),
            Err(GameError::UnknownActor)
        );
        assert_eq!(game, before);
    }

    fn fixture() -> Game {
        fixture_with_anatomy(crate::AnatomySpec::humanoid())
    }
    fn fixture_with_anatomy(anatomy: crate::AnatomySpec) -> Game {
        let hero = test_creatures::species();
        let mut foe = test_creatures::species();
        foe.anatomy = anatomy;
        configured_game(hero, foe, test_creatures::ruleset())
    }
    fn configured_game(
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
        game.configure_ai(
            ActorId(2),
            AiProfile {
                memory_ticks: 50,
                ..Default::default()
            },
        )
        .unwrap();
        game.refresh_navigation();
        game
    }
    fn paid_fixture(magical: bool, fear: bool) -> Game {
        use crate::{
            attributes::{Attributes, ByAttribute},
            content_ids::{AbilityId, DescriptorId, FlagId},
            rules::{Modifier, ModifierOp, Scope, Selector},
        };
        let mut hero = test_creatures::species();
        hero.modifiers.push(Modifier {
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
        for name in ["power_strike", "magic_bolt"]
            .into_iter()
            .chain(fear.then_some("fear"))
        {
            foe.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::GrantAbility {
                    ability: AbilityId::new(name).unwrap(),
                },
            });
        }
        if magical {
            foe.modifiers.push(Modifier {
                scope: Scope::Always,
                op: ModifierOp::Flag {
                    flag: FlagId::new("magical").unwrap(),
                },
            });
        }
        configured_game(hero, foe, test_creatures::ruleset())
    }
    fn basic(target: ActorId) -> Action {
        Action::UseAbility {
            id: crate::content_ids::AbilityId::new("basic_melee").unwrap(),
            target,
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
                id: crate::content_ids::AbilityId::new("fear").unwrap(),
                target: ActorId(1)
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
                id: crate::content_ids::AbilityId::new("power_strike").unwrap(),
                target: ActorId(1),
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
                id: crate::content_ids::AbilityId::new("magic_bolt").unwrap(),
                target: ActorId(1)
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
        let AiDecision { ai, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.target, Some((ActorId(1), at(1, 1, 1), 0)));
        game.combat.ai.insert(ActorId(2), ai);
        game.set_region_light(RegionId(2), false).unwrap();
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        let AiDecision { ai: remembered, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(remembered.target, Some((ActorId(1), at(1, 1, 1), 0)));
        game.tick = 50;
        let AiDecision { ai: expired, .. } = game.choose_ai(ActorId(2)).unwrap();
        assert!(expired.target.is_none());
        assert_eq!(expired.state, State::Search);
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
            slot: crate::EquipmentSlot::Ring,
            attack: None,
            modifiers: vec![crate::rules::Modifier {
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
            game.choose_nearby_loot(actor, &view, &mut route),
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
            game.choose_nearby_loot(actor, &view, &mut |at| search.route(at).ok()),
            None
        );
    }

    #[test]
    fn nearby_loot_uses_own_knowledge_and_takes_only_one_unit() {
        let mut game = fixture();
        let actor = ActorId(2);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        let mut potion = crate::ItemSpec::ordinary("healing".into());
        potion.class = crate::ItemClass::Potion;
        potion.stackable = true;
        potion.concealed = true;
        potion.appearance = "red potion".into();
        potion.consumable = Some(crate::ConsumableSpec {
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
            slots: vec![crate::EquipmentSlot::Ring; 2],
        });
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
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
    fn gear_removal_chooses_an_upgrade_and_does_not_reequip_the_dominated_old_item() {
        let mut game = fixture_with_anatomy(crate::AnatomySpec {
            slots: vec![crate::EquipmentSlot::Ring; 1],
        });
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
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
            slots: vec![crate::EquipmentSlot::Ring; 1],
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
        let rules = test_creatures::ruleset();
        let mut document = rules.document().clone();
        let base = rules
            .ability(&AbilityId::new("magic_bolt").unwrap())
            .unwrap();
        let mut foe = test_creatures::species();
        foe.modifiers.push(Modifier {
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
                    target: ActorId(1)
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
                target: ActorId(1),
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
            "no ordinary scene request may occur before consuming the chosen plan"
        );
        // Commit/time advancement and completion availability checks make three
        // ordinary reach scenes. Due completion settles expiring conditions
        // before its availability check, without another decision or scene.
        assert_eq!(after.ordinary_scenes - before.ordinary_scenes, 3);
        assert_eq!(after.scenes - before.scenes, 4);
        assert_eq!(
            after.scenes - before.scenes,
            (after.decision_scenes - before.decision_scenes)
                + (after.ordinary_scenes - before.ordinary_scenes)
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
}
