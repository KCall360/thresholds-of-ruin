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

// Priority-ordered transition table: frightened, target known, resulting state.
const TRANSITIONS: [(bool, bool, State); 4] = [
    (true, true, State::Flee),
    (false, true, State::Attack),
    (true, false, State::Search),
    (false, false, State::Search),
];

impl Game {
    pub fn configure_ai(&mut self, actor: ActorId, profile: AiProfile) -> Result<(), GameError> {
        if !profile.valid() || self.health(actor).is_none() {
            return Err(GameError::InvalidLocation);
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
        self.choose_ai(id).map(|(action, _)| (id, action))
    }

    pub(crate) fn choose_ai(&self, id: ActorId) -> Option<(Action, Ai)> {
        let mut ai = self.combat.ai.get(&id)?.clone();
        let view = self.observe(id).ok()?;
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
                return Some((Action::Move(direction), ai));
            }
        }
        if let Some((target, at, _)) = ai.target {
            if self.attack_available(id, target) {
                return Some((Action::Attack { target }, ai));
            }
            if ai.state != State::Flee {
                if let Some(step) = route(at).and_then(|r| r.first().copied()) {
                    if self.actor_translation(id, step.direction).is_some() {
                        return Some((Action::Move(step.direction), ai));
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
                return Some((
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
                return Some((Action::Move(*direction), ai));
            }
        }
        Some((Action::Wait, ai))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::{CombatSpec, DamageType};
    use std::{collections::BTreeSet, num::NonZeroU64};
    use tor_world::{Position, RegionId};
    fn at(region: u64, x: i32, y: i32) -> Location {
        Location {
            region: RegionId(region),
            position: Position { x, y, z: 0 },
        }
    }
    fn fixture() -> Game {
        let mut game = Game::two_room_in_stone(42);
        for (x, faction) in [(1, "hero"), (2, "foe")] {
            let id = game
                .spawn_actor(at(1, x, 1), NonZeroU64::new(100).unwrap())
                .unwrap();
            game.configure_combat(
                id,
                CombatSpec {
                    faction: faction.into(),
                    ..Default::default()
                },
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
    #[test]
    fn one_decision_shares_its_search_across_visible_targets() {
        let mut game = fixture();
        let other = game
            .spawn_actor(at(1, 3, 1), NonZeroU64::new(100).unwrap())
            .unwrap();
        game.configure_combat(
            other,
            CombatSpec {
                faction: "hero".into(),
                ..Default::default()
            },
        )
        .unwrap();
        game.refresh_navigation();
        assert_eq!(game.observe(ActorId(2)).unwrap().visible_actors.len(), 2);
        let before = crate::diagnostics::work_counts().route_searches;
        let (action, _) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(action, Action::Attack { target: ActorId(1) });
        assert_eq!(crate::diagnostics::work_counts().route_searches - before, 1);
    }

    #[test]
    fn hidden_movement_does_not_update_target_memory_and_memory_expires() {
        let mut game = fixture();
        let (_, ai) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.target, Some((ActorId(1), at(1, 1, 1), 0)));
        game.combat.ai.insert(ActorId(2), ai);
        game.teleport(ActorId(1), at(2, 4, 2)).unwrap();
        let (_, remembered) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(remembered.target, Some((ActorId(1), at(1, 1, 1), 0)));
        game.tick = 50;
        let (_, expired) = game.choose_ai(ActorId(2)).unwrap();
        assert!(expired.target.is_none());
        assert_eq!(expired.state, State::Search);
    }
    #[test]
    fn wounded_ai_flees_and_a_cornered_ai_fights() {
        let mut game = fixture();
        game.apply_damage(ActorId(2), &BTreeMap::from([(DamageType::Vital, 25)]));
        let (action, ai) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Flee);
        assert!(matches!(action, Action::Move(_)));
        for (x, y) in [(1, 0), (1, 2), (2, 0), (2, 2), (3, 0), (3, 1), (3, 2)] {
            game.set_wall(at(1, x, y), true).unwrap();
        }
        let (action, ai) = game.choose_ai(ActorId(2)).unwrap();
        assert_eq!(ai.state, State::Flee);
        assert_eq!(action, Action::Attack { target: ActorId(1) });
    }
}
