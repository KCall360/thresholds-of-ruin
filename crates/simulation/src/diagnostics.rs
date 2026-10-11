//! Thread-local operation counters, deliberately outside persisted game state.
//! They read no clock and never participate in simulation decisions.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub struct WorkCounts {
    pub observations: usize,
    pub actor_table_copies: usize,
    pub physics_steps: usize,
    pub body_cells: usize,
    pub scenes: usize,
    pub item_candidates: usize,
    pub actor_candidates: usize,
    pub route_searches: usize,
    pub stack_candidates: usize,
    pub knowledge_checks: usize,
    pub timer_candidates: usize,
    pub ability_plans: usize,
    pub decision_scenes: usize,
    pub ordinary_scenes: usize,
    pub ai_precommit_ordinary_scenes: usize,
}
thread_local! { static COUNTS: Cell<WorkCounts> = const { Cell::new(WorkCounts {actor_table_copies:0,physics_steps:0,body_cells:0,observations:0,scenes:0,item_candidates:0,actor_candidates:0,route_searches:0,stack_candidates:0,knowledge_checks:0,timer_candidates:0,ability_plans:0,decision_scenes:0,ordinary_scenes:0,ai_precommit_ordinary_scenes:0}) }; }
pub(crate) fn route_search() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.route_searches += 1;
        c.set(n);
    });
}
pub(crate) fn actor_candidates(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.actor_candidates += count;
        c.set(n);
    });
}
pub(crate) fn item_view(concealed: bool) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.item_candidates += 1;
        n.knowledge_checks += usize::from(concealed);
        c.set(n);
    });
}
pub(crate) fn stack_candidate() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.stack_candidates += 1;
        c.set(n);
    });
}
pub fn work_counts() -> WorkCounts {
    COUNTS.with(Cell::get)
}
pub(crate) fn observation() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.observations += 1;
        c.set(n)
    });
}
pub(crate) fn scene() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.scenes += 1;
        c.set(n)
    });
}

pub(crate) fn physics_step() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.physics_steps += 1;
        c.set(n);
    });
}
pub(crate) fn body_cells(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.body_cells += count;
        c.set(n);
    });
}

pub(crate) fn timer_candidate() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.timer_candidates += 1;
        c.set(n);
    });
}

pub(crate) fn actor_table_copy(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.actor_table_copies += count;
        c.set(n);
    });
}

pub(crate) fn ability_plan() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.ability_plans += 1;
        c.set(n);
    });
}

pub(crate) fn decision_scene() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.decision_scenes += 1;
        c.set(n);
    });
}
pub(crate) fn ordinary_scene() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.ordinary_scenes += 1;
        c.set(n);
    });
}

pub(crate) fn ai_precommit_ordinary_scenes(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.ai_precommit_ordinary_scenes += count;
        c.set(n);
    });
}
