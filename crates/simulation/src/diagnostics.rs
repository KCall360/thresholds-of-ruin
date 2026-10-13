//! Thread-local operation counters, deliberately outside persisted game state.
//! They read no clock and never participate in simulation decisions.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub struct WorkCounts {
    pub health_derivations: usize,
    pub descriptor_lookups: usize,
    /// Full Game snapshots, separate from copy-on-write actor table copies.
    pub game_clones: usize,
    /// Actors enumerated by the preparation scheduler, including idle actors.
    pub preparation_candidates: usize,
    pub strike_resolutions: usize,
    pub rider_resolutions: usize,
    pub observations: usize,
    pub actor_table_copies: usize,
    pub physics_steps: usize,
    pub body_cells: usize,
    pub scenes: usize,
    /// Disclosed-location membership requests, independent of physical contact.
    pub scene_membership_lookups: usize,
    /// Sight cells visited by membership scans or index construction.
    pub scene_membership_cells: usize,
    /// Sight cells copied into or out of the derived scene cache.
    pub scene_cells_copied: usize,
    pub item_candidates: usize,
    pub actor_candidates: usize,
    pub route_searches: usize,
    pub stack_candidates: usize,
    pub knowledge_checks: usize,
    pub timer_candidates: usize,
    pub cooldown_deadline_candidates: usize,
    pub cooldown_entries_advanced: usize,
    pub cooldown_entries_copied: usize,
    pub ability_plans: usize,
    pub decision_scenes: usize,
    pub ordinary_scenes: usize,
    pub ai_precommit_ordinary_scenes: usize,
}
thread_local! { static COUNTS: Cell<WorkCounts> = const { Cell::new(WorkCounts {descriptor_lookups:0,game_clones:0,preparation_candidates:0,strike_resolutions:0,rider_resolutions:0,health_derivations:0,actor_table_copies:0,physics_steps:0,body_cells:0,observations:0,scenes:0,scene_membership_lookups:0,scene_membership_cells:0,scene_cells_copied:0,item_candidates:0,actor_candidates:0,route_searches:0,stack_candidates:0,knowledge_checks:0,timer_candidates:0,cooldown_deadline_candidates:0,cooldown_entries_advanced:0,cooldown_entries_copied:0,ability_plans:0,decision_scenes:0,ordinary_scenes:0,ai_precommit_ordinary_scenes:0}) }; }

pub(crate) fn cooldown_deadline_candidates(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.cooldown_deadline_candidates += count;
        c.set(n);
    });
}

pub(crate) fn cooldown_entries_advanced(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.cooldown_entries_advanced += count;
        c.set(n);
    });
}

pub(crate) fn cooldown_entries_copied(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.cooldown_entries_copied += count;
        c.set(n);
    });
}
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

pub(crate) fn health_derivation() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.health_derivations += 1;
        c.set(n);
    });
}

pub(crate) fn game_clone() {
    COUNTS.with(|counter| {
        let mut counts = counter.get();
        counts.game_clones += 1;
        counter.set(counts);
    });
}

pub(crate) fn preparation_candidates(count: usize) {
    COUNTS.with(|counter| {
        let mut counts = counter.get();
        counts.preparation_candidates += count;
        counter.set(counts);
    });
}

pub(crate) fn strike_resolution() {
    COUNTS.with(|counter| {
        let mut counts = counter.get();
        counts.strike_resolutions += 1;
        counter.set(counts);
    });
}

pub(crate) fn rider_resolution() {
    COUNTS.with(|counter| {
        let mut counts = counter.get();
        counts.rider_resolutions += 1;
        counter.set(counts);
    });
}

pub(crate) fn descriptor_lookup() {
    COUNTS.with(|counter| {
        let mut counts = counter.get();
        counts.descriptor_lookups += 1;
        counter.set(counts);
    });
}

/// Offline aggregate of currently loaded finite-item authority. Never disclosed
/// through a player observation and never consulted by simulation decisions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FiniteClaimProfile {
    pub item_holds: usize,
    pub preparation_refs: usize,
    pub claimant_actors: usize,
    pub max_holds_per_item: usize,
    pub held_units: u64,
}

pub(crate) fn scene_membership_lookup() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.scene_membership_lookups += 1;
        c.set(n);
    });
}
pub(crate) fn scene_membership_cells(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.scene_membership_cells += count;
        c.set(n);
    });
}
pub(crate) fn scene_cells_copied(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.scene_cells_copied += count;
        c.set(n);
    });
}
