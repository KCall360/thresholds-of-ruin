//! A sufficient, private proof that canonical pictures are unchanged.
//!
//! This compares borrowed inputs, not a second wire projection. Failure to prove
//! equality MUST fall back to canonical picture comparison; raw differences may
//! contain facts omitted by normalization or account-role filtering.
use crate::chart_view::PreparedCharts;
use tor_simulation::{combat::CombatView, InteractionView, Observation};
use tor_world::SightCell;

pub(crate) struct PictureInputs<'a> {
    pub observation: &'a Observation,
    pub scene: &'a [SightCell],
    pub ready: bool,
    pub charts: &'a PreparedCharts,
}

/// Caller must separately establish equal actor target scopes, terrain policy,
/// remembered-place output, and chart token branch/generation. Target scope
/// includes the view salt and observer identity.
/// Those values are not all represented in the simulation's ordinary view.
///
/// The inputs belong to two immutable captures of one observer. Equality proves
/// unchanged pictures for every disclosure role; inequality proves nothing.
pub(crate) fn same_picture_inputs(before: PictureInputs<'_>, after: PictureInputs<'_>) -> bool {
    if before.ready != after.ready || before.scene != after.scene || before.charts != after.charts {
        return false;
    }
    let (before, after) = (before.observation, after.observation);
    let Observation {
        linked_holds,
        interactions,
        combat,
        motion,
        actor,
        tick: _,
        location,
        region,
        asset,
        worn,
        visible_cells,
        ground_items,
        apparent_objects,
        inventory,
        visible_actors,
        exits,
        known_places,
    } = before;
    actor == &after.actor
        && location == &after.location
        && region == &after.region
        && asset == &after.asset
        && worn == &after.worn
        && visible_cells == &after.visible_cells
        && ground_items == &after.ground_items
        && apparent_objects == &after.apparent_objects
        && inventory == &after.inventory
        && visible_actors == &after.visible_actors
        && exits == &after.exits
        && known_places == &after.known_places
        && linked_holds == &after.linked_holds
        && same_interactions(interactions.as_ref(), after.interactions.as_ref())
        && same_combat(combat.as_ref(), after.combat.as_ref())
        && motion_velocity(motion.as_ref().map(|view| view.velocity))
            == motion_velocity(after.motion.as_ref().map(|view| view.velocity))
}

fn same_combat(before: Option<&CombatView>, after: Option<&CombatView>) -> bool {
    match (before, after) {
        (None, None) => true,
        (Some(before), Some(after)) => {
            let CombatView {
                own_stats,
                public_plan: _,
                hp,
                max_hp,
                preparation_remaining,
                preparation_active,
                recovery_remaining,
                sequence,
                actors,
                events: _,
                objective,
                exit,
                victory,
                dead,
                terminal,
            } = before;
            own_stats == &after.own_stats
                && hp == &after.hp
                && max_hp == &after.max_hp
                && preparation_remaining == &after.preparation_remaining
                && preparation_active == &after.preparation_active
                && recovery_remaining == &after.recovery_remaining
                && sequence == &after.sequence
                && actors == &after.actors
                && objective == &after.objective
                && exit == &after.exit
                && victory == &after.victory
                && dead == &after.dead
                && terminal == &after.terminal
        }
        _ => false,
    }
}

fn picture_interactions(view: Option<&InteractionView>) -> Option<&InteractionView> {
    view.filter(|view| {
        view.identification.is_some()
            || !view.slots.is_empty()
            || view.preparation.is_some()
            || !view.inventory.is_empty()
    })
}

fn same_interactions(before: Option<&InteractionView>, after: Option<&InteractionView>) -> bool {
    match (picture_interactions(before), picture_interactions(after)) {
        (None, None) => true,
        (Some(before), Some(after)) => {
            let InteractionView {
                identification,
                completed: _,
                refused: _,
                curse_blocked: _,
                slots,
                preparation,
                inventory,
            } = before;
            identification == &after.identification
                && slots == &after.slots
                && preparation == &after.preparation
                && inventory == &after.inventory
        }
        _ => false,
    }
}

fn motion_velocity(velocity: Option<[i64; 3]>) -> Option<[i64; 3]> {
    velocity.filter(|velocity| *velocity != [0; 3])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_only_interactions_do_not_require_a_picture() {
        let mut view = InteractionView {
            identification: None,
            completed: vec![],
            refused: vec![],
            curse_blocked: vec![tor_simulation::ItemId(1)],
            slots: vec![],
            preparation: None,
            inventory: vec![],
        };
        assert!(same_interactions(None, Some(&view)));
        view.curse_blocked.clear();
        assert!(same_interactions(Some(&view), None));
    }

    #[test]
    fn zero_motion_is_not_a_picture_input_but_velocity_is() {
        assert_eq!(motion_velocity(Some([0; 3])), motion_velocity(None));
        assert_ne!(motion_velocity(Some([1, 0, 0])), motion_velocity(None));
        assert_eq!(motion_velocity(Some([1, 0, 0])), Some([1, 0, 0]));
    }
}
