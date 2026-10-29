//! Exact owner facts retain legacy projection equality without owned copies.
use super::*;
fn state() -> CreatureState {
    CreatureState::new(crate::test_creatures::build(
        crate::test_creatures::species(),
    ))
    .unwrap()
}
// Retained pre-refactor eager projection: independent from facts/materializer.
fn eager_reference(state: &CreatureState) -> OwnStats {
    let derived = state.derived();
    let costs = state.costs();
    OwnStats {
        rules: state.build().content().rules.clone(),
        kind: derived.kind.clone(),
        subtypes: derived.subtypes.clone(),
        hit_dice: derived.hit_dice.clone(),
        attributes: derived.attributes,
        restorable_losses: state.restorable_losses(),
        restorable_hit_dice: state.restorable_hit_dice(),
        skills: derived.skills,
        defenses: derived.defenses,
        binding: derived.binding(),
        health_recovery: state.health_recovery(),
        nutrition: state
            .nutrition()
            .view(state.build().ruleset().document().nutrition),
        conditions: state
            .conditions()
            .instances()
            .filter(|entry| {
                state
                    .build()
                    .ruleset()
                    .condition(&entry.condition)
                    .is_some_and(|definition| {
                        definition.lifecycle == crate::rules::ConditionLifecycle::Timed
                    })
            })
            .map(|entry| PersonalCondition {
                condition: entry.condition.clone(),
                phase: entry.phase,
                remaining_ticks: entry.remaining,
            })
            .collect(),
        health_recovery_reset_percent: derived.health_recovery_reset_percent,
        resources: Resource::ALL
            .into_iter()
            .map(|resource| {
                let balance = costs.resources().balance(resource);
                let available = costs.available(resource);
                ResourceView {
                    resource,
                    balance,
                    maximum: costs.resources().maximum(resource),
                    available,
                    reserved: balance - available,
                }
            })
            .collect(),
        active_talents: derived.active_talents.clone(),
        expertises: derived.expertises.clone(),
        dormant_talents: derived.dormant_talents.clone(),
        abilities: derived.abilities.clone(),
        weapon_abilities: derived.weapon_abilities.clone(),
        ability_cooldowns: derived
            .abilities
            .iter()
            .map(|id| (id.clone(), state.cooldowns().remaining(id)))
            .collect(),
        ability_reaches: derived.ability_reaches.clone(),
    }
}
#[test]
fn own_stats_facts_capture_and_equality_do_not_materialize_owned_collections() {
    let state = state();
    let expected = eager_reference(&state);
    let before = crate::diagnostics::work_counts();
    let facts = state.own_stats_facts();
    assert_eq!(facts.nutrition(), expected.nutrition);
    assert_eq!(facts.rules(), &expected.rules);
    assert_eq!(facts, facts.clone());
    assert_eq!(
        crate::diagnostics::work_counts().own_stats_materializations,
        before.own_stats_materializations
    );
    let owned = facts.materialize();
    assert_eq!(owned, expected);
    assert_eq!(
        crate::diagnostics::work_counts().own_stats_materializations,
        before.own_stats_materializations + 1
    );
    let after = crate::diagnostics::work_counts();
    let copied = after.own_stats_static_entries_copied - before.own_stats_static_entries_copied;
    let mut one_more = facts.clone();
    assert!(one_more
        .derived
        .subtypes
        .insert(SubtypeId::new("fixture_counted_subtype").unwrap()));
    one_more.materialize();
    assert_eq!(
        crate::diagnostics::work_counts().own_stats_static_entries_copied
            - after.own_stats_static_entries_copied,
        copied + 1
    );
}
#[test]
fn own_stats_facts_equality_ignores_unexposed_derived_fields_not_projected_values() {
    let facts = state().own_stats_facts();
    let mut same_output = facts.clone();
    same_output.derived.attack_damage_roll_bonus += 1;
    assert!(!same_output.derived.shares_storage(&facts.derived));
    assert_eq!(same_output, facts);
    assert_eq!(same_output.materialize(), facts.materialize());
    let mut changed = facts.clone();
    changed
        .derived
        .subtypes
        .insert(SubtypeId::new("fixture_projection_subtype").unwrap());
    assert_ne!(changed, facts);
    assert_ne!(changed.materialize(), facts.materialize());
}
#[test]
fn own_stats_materialization_and_later_state_changes_cannot_edit_retained_facts() {
    let mut state = state();
    let expected = eager_reference(&state);
    let retained = state.own_stats_facts();
    let mut owned = retained.materialize();
    owned
        .subtypes
        .insert(SubtypeId::new("fixture_caller_subtype").unwrap());
    owned.resources.clear();
    assert_eq!(retained.materialize(), expected);
    let condition = crate::content_ids::ConditionId::new("braced").unwrap();
    state
        .apply_condition(&condition, crate::ActorId(1), 25)
        .unwrap();
    let fresh = state.own_stats_facts();
    assert_ne!(fresh, retained);
    assert_eq!(fresh.materialize(), eager_reference(&state));
    assert!(fresh
        .materialize()
        .conditions
        .iter()
        .any(|entry| entry.condition == condition && entry.remaining_ticks == Some(25)));
    assert_eq!(retained.materialize(), expected);
    state.advance_active(7);
    assert_eq!(
        state.own_stats_facts().materialize(),
        eager_reference(&state)
    );
    assert_eq!(retained.materialize(), expected);
}
