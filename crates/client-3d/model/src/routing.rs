//! Local fallback routing over disclosed floor; the server owns 3D travel and reach.
use crate::{direction, known_cells, near_actor, Cmd, View};
use std::collections::{HashMap, HashSet, VecDeque};
use tor_protocol::{ActorTarget, Direction, Position};

pub fn step_toward(view: &View, goals: &HashSet<Position>) -> Option<Direction> {
    let goals = goals
        .iter()
        .filter(|at| at.z == view.feet.z)
        .map(|at| (at.x, at.y))
        .collect();
    projected_step(view, &goals)
}

pub fn attack_at_hand(view: &View, id: ActorTarget) -> Option<Cmd> {
    (view.actors.iter().any(|a| a.id == id && a.hostile) && near_actor(view, id))
        .then_some(Cmd::Attack(id))
}

/// Candidate approach cells keep all three coordinates and disclosed body cells.
pub fn approach_cells(view: &View, id: ActorTarget) -> HashSet<Position> {
    known_cells(view)
        .filter(|cell| !cell.wall && !view.actors.iter().any(|a| a.position == cell.position))
        .filter(|cell| {
            view.actors
                .iter()
                .filter(|actor| actor.id == id && actor.hostile)
                .any(|actor| {
                    [
                        (cell.position.x, actor.position.x),
                        (cell.position.y, actor.position.y),
                        (cell.position.z, actor.position.z),
                    ]
                    .into_iter()
                    .all(|(a, b)| (i64::from(a) - i64::from(b)).abs() <= 1)
                })
        })
        .map(|cell| cell.position)
        .collect()
}

/// The first step along the shortest known way to any of `goals`, over open
/// floor on the character's level, around closed doors and other creatures.
/// Diagonal steps never cut a corner.
fn projected_step(view: &View, goals: &HashSet<(i32, i32)>) -> Option<Direction> {
    let feet = view.feet;
    let start = (feet.x, feet.y);
    let creatures: HashSet<(i32, i32)> = view
        .actors
        .iter()
        .filter(|a| !a.is_self && (a.position.z - feet.z).abs() <= 1)
        .map(|a| (a.position.x, a.position.y))
        .collect();
    let open: HashSet<(i32, i32)> = known_cells(view)
        .filter(|c| c.position.z == feet.z && !c.wall && c.door != Some(false))
        .map(|c| (c.position.x, c.position.y))
        .filter(|cell| !creatures.contains(cell) || goals.contains(cell))
        .collect();
    let mut came_from: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    let mut queue = VecDeque::from([start]);
    came_from.insert(start, start);
    while let Some(at) = queue.pop_front() {
        if goals.contains(&at) && at != start {
            let mut step = at;
            while came_from[&step] != start {
                step = came_from[&step];
            }
            return direction(step.0 - start.0, step.1 - start.1);
        }
        for dx in -1..=1 {
            for dy in -1..=1 {
                let next = (at.0 + dx, at.1 + dy);
                if (dx, dy) == (0, 0) || came_from.contains_key(&next) || !open.contains(&next) {
                    continue;
                }
                if dx != 0
                    && dy != 0
                    && !(open.contains(&(at.0 + dx, at.1)) && open.contains(&(at.0, at.1 + dy)))
                {
                    continue;
                }
                came_from.insert(next, at);
                queue.push_back(next);
            }
        }
    }
    None
}
