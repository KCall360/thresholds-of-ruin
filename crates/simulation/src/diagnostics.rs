//! Thread-local operation counters, deliberately outside persisted game state.
//! They read no clock and never participate in simulation decisions.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub struct WorkCounts {
    pub observations: usize,
    pub scenes: usize,
}
thread_local! { static COUNTS: Cell<WorkCounts> = const { Cell::new(WorkCounts {observations:0,scenes:0}) }; }
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
