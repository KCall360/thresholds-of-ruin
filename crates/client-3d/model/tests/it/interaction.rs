use std::sync::Arc;
use tor_client_3d_model::{
    describe, request_from, view, CreatureModel, GroundItem, Intent, ItemModel, View,
};
use tor_client_common::{ClientState, Palette};
use tor_protocol::*;

fn empty() -> View {
    let mut view = view(&super::projection::state(), &Palette::default());
    view.actors.clear();
    view.items.clear();
    view.visible.clear();
    view.memory.clear();
    view
}

fn cell(position: Position, key: &str) -> CellView {
    CellView {
        object_presentation: None,
        lit: true,
        key: key.into(),
        position,
        wall: false,
        material: String::new(),
        trap: None,
        door: None,
        asset: None,
        stairs_up: false,
        stairs_down: false,
        place_hint: false,
    }
}

#[test]
fn stacked_cells_do_not_borrow_items_from_another_height() {
    let mut view = empty();
    let below = Position { x: 1, y: 0, z: 0 };
    let above = Position { z: 1, ..below };
    view.visible = vec![cell(below, "lower"), cell(above, "upper")];
    view.items.push(GroundItem {
        corpse_seed: None,
        model: ItemModel::Placeholder,
        corpse_model: CreatureModel::Placeholder,
        id: ItemTarget::from_digest([9; 32]),
        name: "lower relic".into(),
        class: ItemClass::Misc,
        quantity: 1,
        description: "Only below".into(),
        position: below,
        reachable: true,
    });
    assert_eq!(describe(&view, below).0, "Lower relic");
    assert_eq!(
        describe(&view, above).0,
        "Cave floor",
        "a 3D pick must keep its exact disclosed height"
    );
}

#[test]
fn selecting_one_model_preserves_its_target_when_items_share_a_cell() {
    use tor_client_3d_model::{describe_focus, Focus};
    let mut view = empty();
    let at = view.feet;
    let first = ItemTarget::from_digest([21; 32]);
    let second = ItemTarget::from_digest([22; 32]);
    for (id, name) in [(first, "first relic"), (second, "second relic")] {
        view.items.push(GroundItem {
            corpse_seed: None,
            model: ItemModel::Placeholder,
            corpse_model: CreatureModel::Placeholder,
            id,
            name: name.into(),
            class: ItemClass::Misc,
            quantity: 1,
            description: String::new(),
            position: at,
            reachable: true,
        });
    }
    assert!(
        matches!(describe_focus(&view, Focus::Item(second)).2, Some(Intent::Take(id)) if id == second)
    );
    assert_eq!(describe_focus(&view, Focus::Item(second)).0, "Second relic");
    view.items.retain(|item| item.id != second);
    assert!(
        Focus::Item(second).position(&view).is_none(),
        "stale model cannot acquire another target"
    );
    assert!(describe_focus(&view, Focus::Item(second)).2.is_none());
}

#[test]
fn local_routing_does_not_substitute_a_lower_floor_for_an_elevated_destination() {
    use std::collections::HashSet;
    use tor_client_3d_model::step_toward;
    let mut view = empty();
    view.feet = Position { x: 0, y: 0, z: 0 };
    let lower = Position { x: 1, y: 0, z: 0 };
    let upper = Position { z: 2, ..lower };
    view.visible = vec![
        cell(view.feet, "start"),
        cell(lower, "lower"),
        cell(upper, "upper"),
    ];
    assert!(matches!(
        step_toward(&view, &HashSet::from([lower])),
        Some(Direction::East)
    ));
    assert!(step_toward(&view, &HashSet::from([upper])).is_none());
}

#[test]
fn finishing_a_pursuit_attacks_the_disclosed_elevated_target() {
    use tor_client_3d_model::{attack_at_hand, Cmd};
    let mut view = view(&super::projection::state(), &Palette::default());
    let feet = view.feet;
    let foe = view.actors.iter_mut().find(|a| !a.is_self).unwrap();
    foe.hostile = true;
    foe.position = Position {
        x: feet.x + 1,
        y: feet.y,
        z: feet.z + 1,
    };
    let target = foe.id;
    assert!(matches!(attack_at_hand(&view, target), Some(Cmd::Attack(id)) if id == target));
}

#[test]
fn a_pursuit_does_not_attack_a_target_that_has_become_friendly() {
    use tor_client_3d_model::attack_at_hand;
    let mut view = view(&super::projection::state(), &Palette::default());
    let feet = view.feet;
    let foe = view.actors.iter_mut().find(|a| !a.is_self).unwrap();
    foe.hostile = false;
    foe.position = Position {
        x: feet.x + 1,
        ..feet
    };
    let target = foe.id;
    assert!(attack_at_hand(&view, target).is_none());
}

#[test]
fn pursuit_reach_uses_disclosed_body_cells_and_approach_goals_keep_elevation() {
    use tor_client_3d_model::{approach_cells, attack_at_hand, Cmd};
    let mut view = view(&super::projection::state(), &Palette::default());
    view.feet = Position { x: 0, y: 0, z: 0 };
    let own = view.actors.iter_mut().find(|a| a.is_self).unwrap();
    own.position = Position { x: 0, y: 0, z: 2 };
    let foe = view.actors.iter_mut().find(|a| !a.is_self).unwrap();
    foe.hostile = true;
    foe.position = Position { x: 1, y: 0, z: 2 };
    let target = foe.id;
    assert!(matches!(attack_at_hand(&view, target), Some(Cmd::Attack(id)) if id == target));
    let lower = Position { x: 2, y: 0, z: 0 };
    let upper = Position { z: 2, ..lower };
    view.visible = vec![cell(lower, "lower"), cell(upper, "upper")];
    let goals = approach_cells(&view, target);
    assert!(goals.contains(&upper));
    assert!(!goals.contains(&lower));
}

#[test]
fn a_stair_in_the_same_column_is_not_the_stair_underfoot() {
    let mut view = empty();
    let above = Position { x: 0, y: 0, z: 1 };
    let mut stair = cell(above, "upper-stair");
    stair.stairs_up = true;
    view.visible.push(stair);
    assert!(matches!(describe(&view, above).2, Some(Intent::Travel(key)) if key == "upper-stair"));
    view.feet = above;
    assert!(
        matches!(describe(&view, above).2, Some(Intent::Climb(Direction::Up))),
        "positive underfoot control"
    );
}

fn door(position: Position, reachable: bool) -> CellView {
    let mut cell = cell(position, "door");
    cell.door = Some(DoorView {
        id: DoorTarget::from_digest([8; 32]),
        name: "a door".into(),
        description: String::new(),
        open: false,
        reachable,
        approaches: vec![],
        asset: None,
    });
    cell
}

#[test]
fn a_door_action_uses_the_servers_reachability_not_grid_adjacency() {
    for (x, reachable) in [(1, false), (2, true)] {
        let mut view = empty();
        let at = Position { x, y: 0, z: 0 };
        view.visible.push(door(at, reachable));
        assert_eq!(
            matches!(describe(&view, at).2, Some(Intent::Door(_, true))),
            reachable
        );
    }
}

#[test]
fn remembered_door_reachability_never_grants_a_current_action() {
    let mut view = empty();
    let at = Position { x: 1, y: 0, z: 0 };
    let live = door(at, true);
    view.memory.push(tor_protocol::RememberedMapCell {
        object_presentation: None,
        key: live.key,
        position: at,
        wall: live.wall,
        material: live.material,
        asset: live.asset,
        place_hint: live.place_hint,
        stairs_up: false,
        stairs_down: false,
        trap: None,
        door: live.door.map(|d| tor_protocol::MapDoorAppearance {
            name: d.name,
            description: d.description,
            open: d.open,
            asset: d.asset,
        }),
        ground_items: vec![],
    });
    assert!(!matches!(describe(&view, at).2, Some(Intent::Door(..))));
}

#[test]
fn an_elevated_hostiles_click_attacks_its_exact_target_instead_of_moving() {
    let mut snapshot = super::projection::state().reset_snapshot().unwrap();
    snapshot.has_control = true;
    snapshot.readiness.admission = true;
    let observation = &mut Arc::make_mut(&mut snapshot.state).observation;
    let foe = observation.visible_actors[0].id;
    observation.visible_actors[0].position.z = 1;
    observation
        .combat
        .as_mut()
        .unwrap()
        .actors
        .push(CombatActorView {
            actor: foe,
            hostile: true,
            allied: false,
            injury: Injury::Healthy,
        });
    let state = ClientState::from_snapshot(snapshot).unwrap();
    let projected = view(&state, &Palette::default());
    let target = projected
        .actors
        .iter()
        .find(|actor| actor.id == foe)
        .unwrap();
    let intent = describe(&projected, target.position).2.unwrap();
    let request = request_from(&state, &state.reply_context(), intent.command().unwrap()).unwrap();
    assert!(
        matches!(request, Request::Command { command: Command::Act {
        action: Action::UseAbility { invocation: AbilityInvocation { aim: tor_protocol::AbilityAim::ActorTarget { target }, weapon: tor_protocol::WeaponSelection::Default }, .. }, .. }, .. }
        if target == foe),
        "the actor target must survive click interpretation: {request:?}"
    );
}

#[test]
fn stair_cosmetics_do_not_create_or_remove_real_navigation() {
    let mut view = empty();
    let at = view.feet;
    let mut stair = cell(at, "actual-stairs");
    stair.stairs_down = true;
    view.visible = vec![stair];
    assert_eq!(describe(&view, at).0, "Stairs leading down");
    assert!(matches!(
        describe(&view, at).2,
        Some(Intent::Climb(Direction::Down))
    ));
    view.visible[0].object_presentation = Some(ObjectPresentation {
        name: "silver flask".into(),
        asset: None,
        kind: ObjectPresentationKind::Item {
            class: ItemClass::Potion,
        },
    });
    assert_eq!(describe(&view, at).0, "Silver flask");
    assert!(matches!(
        describe(&view, at).2,
        Some(Intent::Climb(Direction::Down))
    ));
    view.visible[0].stairs_down = false;
    view.visible[0].object_presentation.as_mut().unwrap().kind = ObjectPresentationKind::Stairs;
    assert!(describe(&view, at).2.is_none());
}

#[test]
fn a_found_trap_is_named_by_family_and_only_an_unknown_reachable_one_is_studied() {
    let mut view = empty();
    let at = Position { x: 1, y: 0, z: 0 };
    let id = TrapTarget::from_digest([31; 32]);
    let mut floor = cell(at, "trap-cell");
    floor.trap = Some(TrapView {
        id,
        family: None,
        reachable: true,
    });
    view.visible = vec![floor];
    let (name, _, intent) = describe(&view, at);
    assert_eq!(name, "Trap");
    assert!(matches!(intent, Some(Intent::ExamineTrap(found)) if found == id));
    view.visible[0].trap.as_mut().unwrap().reachable = false;
    assert!(
        describe(&view, at).2.is_none(),
        "out of reach: no click, no travel onto it"
    );
    view.visible[0].trap.as_mut().unwrap().family = Some(TrapFamily::BearTrap);
    view.visible[0].trap.as_mut().unwrap().reachable = true;
    let (name, _, intent) = describe(&view, at);
    assert_eq!(name, "Bear trap");
    assert!(intent.is_none(), "a known trap needs no study");
}

#[test]
fn hovering_a_creature_names_its_visible_gear() {
    let mut view = view(&super::projection::state(), &Palette::default());
    let creature = view.actors.iter_mut().find(|a| !a.is_self).unwrap();
    creature.description = "A wary guard.".into();
    creature.attire = tor_client_common::narration::Attire {
        wears: vec!["ring mail".into()],
        holds: vec!["mace".into()],
    };
    let at = creature.position;
    let (_, description, _) = describe(&view, at);
    assert_eq!(
        description,
        "A wary guard. Wearing ring mail, holding mace."
    );
    view.actors.iter_mut().find(|a| !a.is_self).unwrap().attire = Default::default();
    assert_eq!(
        describe(&view, at).1,
        "A wary guard.",
        "nothing disclosed, nothing added"
    );
}
