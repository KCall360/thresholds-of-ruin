use tor_client_3d_model::happenings;
use tor_client_common::test_support::{content, observation};
use tor_protocol::{AbilityOutcome, CombatEventView};

#[test]
fn harm_requires_a_worsening_disclosed_value_in_the_same_observer_scope() {
    use tor_client_3d_model::{observed_harm, view};
    use tor_client_common::Palette;
    use tor_protocol::{ActorId, Injury};
    let mut before = view(&super::projection::state(), &Palette::default());
    before.hp = Some((12, 12));
    let other = before.actors.iter().position(|a| !a.is_self).unwrap();
    let target = before.actors[other].id;
    before.actors[other].injury = Some(Injury::Healthy);
    let mut after = before.clone();
    after.actors[other].injury = Some(Injury::Wounded);
    // Multiple body cells still describe one injury.
    after.actors.push(after.actors[other].clone());
    assert_eq!(observed_harm(&before, &after), vec![target]);
    assert!(observed_harm(&after, &before).is_empty(), "healing");
    assert!(observed_harm(&after, &after).is_empty(), "stable");
    let mut unknown = before.clone();
    unknown.actors[other].injury = None;
    assert!(
        observed_harm(&unknown, &after).is_empty(),
        "newly learned injury"
    );
    after = before.clone();
    after.hp = Some((11, 12));
    let own = after.actors.iter().find(|a| a.is_self).unwrap().id;
    assert_eq!(observed_harm(&before, &after), vec![own]);
    for boundary in 0..3 {
        let mut reset = after.clone();
        match boundary {
            0 => reset.context.epoch += 1,
            1 => reset.branch.0 = "other".into(),
            _ => reset.shown.actor = ActorId(99),
        }
        assert!(
            observed_harm(&before, &reset).is_empty(),
            "boundary {boundary}"
        );
    }
}

#[test]
fn projected_techniques_keep_their_disclosed_id_separate_from_the_display_name() {
    let mut observed = observation();
    let mut ability = content("fixture_technique");
    ability.name = "A completely unrelated caption".into();
    observed.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
        caster: Some(observed.self_target),
        target: None,
        ability,
        outcome: AbilityOutcome::Applied,
        strike: None,
    }];
    assert!(
        matches!(&happenings(&observed)[0], tor_client_3d_model::Happening::Ability { id, name, .. }
        if id == "fixture_technique" && name == "A completely unrelated caption")
    );
}

#[test]
fn unaffected_ability_retains_its_non_applied_outcome() {
    let mut observed = observation();
    observed.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
        caster: Some(observed.self_target),
        target: Some(observed.visible_actors[0].id),
        ability: content("basic_melee"),
        outcome: AbilityOutcome::Unaffected,
        strike: None,
    }];
    let events = happenings(&observed);
    assert_eq!(
        events.len(),
        1,
        "a resisted effect still has a visible animation"
    );
    assert!(
        !events[0].is_applied(),
        "unaffected must not look like a landed hit"
    );
}

#[test]
fn applied_outcomes_remain_distinct_from_misses() {
    for outcome in [AbilityOutcome::Applied, AbilityOutcome::Miss] {
        let mut observed = observation();
        observed.combat.as_mut().unwrap().events = vec![CombatEventView::Ability {
            caster: Some(observed.self_target),
            target: Some(observed.visible_actors[0].id),
            ability: content("arbitrary_authored_ability"),
            outcome,
            strike: Some(2),
        }];
        assert_eq!(
            happenings(&observed)[0].is_applied(),
            outcome == AbilityOutcome::Applied
        );
    }
}
