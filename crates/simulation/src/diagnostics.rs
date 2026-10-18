//! Thread-local operation counters, deliberately outside persisted game state.
//! They read no clock and never participate in simulation decisions.
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default)]
pub struct WorkCounts {
    pub health_derivations: usize,
    /// Permanent record compilations, including standalone build validation.
    pub permanent_creature_compiles: usize,
    /// Exact permanent-input equality calls; permitted only at build replacement.
    pub permanent_input_comparisons: usize,
    /// Effective folds from permanent inputs; no HD traversal is needed.
    pub dynamic_creature_folds: usize,
    /// Raw HD records visited by compilation/counting/Health sampling.
    pub raw_hit_die_visits: usize,
    /// Raw species catalog reads, permitted at permanent compilation and inspection.
    pub raw_species_lookups: usize,
    pub advancement_choice_visits: usize,
    /// Individual exact Health entries copied, excluding shared Arc clones.
    pub health_provenance_copies: usize,
    pub creature_build_copies: usize,

    /// Actor snapshots captured only while contextual recovery has watched owners.
    pub perception_witness_captures: usize,
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
    /// Full observer-specific entity projection passes.
    pub perception_projections: usize,
    /// Individual body classifications, including bounded single-target checks.
    pub actor_disclosures: usize,
    /// Lazy observer contact neighborhoods, independent of concealed target count.
    pub contact_neighborhoods: usize,
    /// Bounded physical contact endpoint probes (at most 26 per source body cell).
    pub contact_route_probes: usize,
    /// Historical compact bindings actually inspected for committed disclosure.
    pub event_receipt_binding_inspections: usize,
    /// Compact binding entries copied because a shared receipt batch diverged.
    pub event_receipt_cow_bindings: usize,
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
    /// Supported defensive candidates visited in authored-priority order.
    pub ai_defense_candidates: usize,
    /// Whole own-source folds for one immutable defensive comparison baseline.
    pub ai_defense_source_scans: usize,
    /// Visit entries copied when an idle decision changes its shared region bucket.
    pub ai_visit_entries_copied: usize,
}
thread_local! { static COUNTS: Cell<WorkCounts> = const { Cell::new(WorkCounts {creature_build_copies:0,permanent_creature_compiles:0,permanent_input_comparisons:0,dynamic_creature_folds:0,raw_hit_die_visits:0,raw_species_lookups:0,advancement_choice_visits:0,health_provenance_copies:0,perception_witness_captures:0,descriptor_lookups:0,game_clones:0,preparation_candidates:0,strike_resolutions:0,rider_resolutions:0,health_derivations:0,actor_table_copies:0,physics_steps:0,body_cells:0,observations:0,scenes:0,scene_membership_lookups:0,scene_membership_cells:0,scene_cells_copied:0,item_candidates:0,actor_candidates:0,perception_projections:0,actor_disclosures:0,contact_neighborhoods:0,contact_route_probes:0,event_receipt_binding_inspections:0,event_receipt_cow_bindings:0,route_searches:0,stack_candidates:0,knowledge_checks:0,timer_candidates:0,cooldown_deadline_candidates:0,cooldown_entries_advanced:0,cooldown_entries_copied:0,ability_plans:0,decision_scenes:0,ordinary_scenes:0,ai_precommit_ordinary_scenes:0,ai_defense_candidates:0,ai_defense_source_scans:0,ai_visit_entries_copied:0}) }; }

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
pub(crate) fn perception_witness_capture() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.perception_witness_captures += 1;
        c.set(n);
    });
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

pub(crate) fn ai_defense_candidate() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.ai_defense_candidates += 1;
        counts.set(value);
    });
}
pub(crate) fn ai_defense_source_scan() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.ai_defense_source_scans += 1;
        counts.set(value);
    });
}

pub(crate) fn permanent_creature_compile() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.permanent_creature_compiles += 1;
        counts.set(value);
    });
}

pub(crate) fn permanent_input_comparison() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.permanent_input_comparisons += 1;
        counts.set(value);
    });
}

pub(crate) fn dynamic_creature_fold() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.dynamic_creature_folds += 1;
        counts.set(value);
    });
}

pub(crate) fn raw_hit_die_visit() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.raw_hit_die_visits += 1;
        counts.set(value);
    });
}

pub(crate) fn advancement_choice_visit() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.advancement_choice_visits += 1;
        counts.set(value);
    });
}

pub(crate) fn health_provenance_copy() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.health_provenance_copies += 1;
        counts.set(value);
    });
}

pub(crate) fn creature_build_copy() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.creature_build_copies += 1;
        counts.set(value);
    });
}

pub(crate) fn raw_species_lookup() {
    COUNTS.with(|counts| {
        let mut value = counts.get();
        value.raw_species_lookups += 1;
        counts.set(value);
    });
}

pub(crate) fn ai_visit_entries_copied(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.ai_visit_entries_copied += count;
        c.set(n);
    });
}

pub(crate) fn perception_projection() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.perception_projections += 1;
        c.set(n);
    });
}
pub(crate) fn actor_disclosure() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.actor_disclosures += 1;
        c.set(n);
    });
}

pub(crate) fn contact_neighborhood() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.contact_neighborhoods += 1;
        c.set(n);
    });
}
pub(crate) fn contact_route_probe() {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.contact_route_probes += 1;
        c.set(n);
    });
}

pub(crate) fn event_receipt_binding_inspections(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.event_receipt_binding_inspections += count;
        c.set(n);
    });
}
pub(crate) fn event_receipt_cow_bindings(count: usize) {
    COUNTS.with(|c| {
        let mut n = c.get();
        n.event_receipt_cow_bindings += count;
        c.set(n);
    });
}
