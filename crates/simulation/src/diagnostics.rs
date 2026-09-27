//! Thread-local operation counters, deliberately outside persisted game state.
//! They read no clock and never participate in simulation decisions.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub struct WorkCounts {
    pub observations: usize,
    pub scenes: usize,
    pub item_candidates: usize,
    pub stack_candidates: usize,
    pub knowledge_checks: usize,
}
thread_local! { static COUNTS: Cell<WorkCounts> = const { Cell::new(WorkCounts {observations:0,scenes:0,item_candidates:0,stack_candidates:0,knowledge_checks:0}) }; }
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
