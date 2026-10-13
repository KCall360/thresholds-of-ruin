use tor_client_ascii::{App, Effect, Input, Key};
use tor_client_common::{test_support, ClientState};
use tor_protocol::*;

fn app() -> App {
    let mut view = test_support::observation();
    let mut ability = test_support::ability("mend");
    ability.reach = AbilityReach::SelfOnly;
    ability.target_kind = AbilityTargetKind::SelfOnly;
    ability.cooldown_remaining = 25;
    ability.supply_costs = vec![AbilitySupplyCost {
        supply: "bandage".into(),
        name: "Bandage".into(),
        quantity: 1,
        access: SupplyAccess::CarriedOrGroundTouch,
    }];
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities = vec![ability];
    view.ground_items.push(GroundItemView {
        reachable: false,
        supply_reachable: true,
        position: Position { x: 1, y: 0, z: 0 },
        item: ItemView {
            id: super::item_target(4),
            class: ItemClass::Misc,
            quantity: 1,
            appearance: "bandage".into(),
            identified: true,
            description: String::new(),
            name: "clean bandage".into(),
            asset: None,
            known_supply: Some(SupplyView {
                id: "bandage".into(),
                name: "Bandage".into(),
            }),
        },
    });
    let snapshot: Snapshot = serde_json::from_value(serde_json::json!({
        "readiness":{"revision":"0","admission":true,"resume":[],"cancel":[]},
        "context":{"stream":"fixture-attachment","epoch":"0"},"actor":"1","branch":"test",
        "cursor":{"sequence":"0","tick":"0"},"has_control":true,"intentions":[],
        "history":{"entries":[],"older_before":null},"state":{"wizard_game":false,"revision":"1","observation":view}
    })).unwrap();
    let mut app = App::default();
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    app.role = AccessRole::Player;
    app.ready();
    app.messages.skip();
    app
}
fn open(app: &mut App) {
    assert_eq!(
        app.input(Input::Key {
            key: Key::Abilities
        }),
        Effect::None
    );
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.supply_selection.is_some());
}
#[test]
fn phase5_supply_menu_defaults_to_carried_and_never_pays_locally() {
    let mut app = app();
    let before = app.state.as_ref().unwrap().snapshot();
    open(&mut app);
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
    assert!(app.supply_rows().join(" ").contains("25 active ticks"));
    assert!(app.supply_rows().join(" ").contains("1 Bandage"));
    assert!(matches!(
        app.input(Input::Key { key: Key::Enter }),
        Effect::Request(Request::Command {
            command: Command::Act {
                action: Action::UseAbility {
                    supplies: SupplySelection::Carried,
                    ..
                },
                ..
            },
            ..
        })
    ));
    assert_eq!(app.state.as_ref().unwrap().snapshot(), before);
}
#[test]
fn phase5_supply_menu_selects_exact_adjacent_ground_and_cancels_cleanly() {
    let mut app = app();
    open(&mut app);
    app.input(Input::Key { key: Key::Down });
    app.input(Input::Key { key: Key::Wait });
    assert_eq!(app.supply_selection.as_ref().unwrap().checked.len(), 1);
    assert!(
        matches!(app.input(Input::Key { key: Key::Enter }), Effect::Request(Request::Command { command: Command::Act { action: Action::UseAbility { supplies: SupplySelection::Explicit { items }, .. }, .. }, .. }) if items == vec![SelectedSupply { cost_ordinal: 0, item: super::item_target(4) }])
    );
    let mut app = self::app();
    open(&mut app);
    assert_eq!(app.input(Input::Key { key: Key::Escape }), Effect::None);
    assert!(app.supply_selection.is_none());
    open(&mut app);
    app.disconnect("Disconnected".into());
    assert!(app.supply_selection.is_none());
}

#[test]
fn phase5_supply_picker_bounds_exact_checked_rows_and_clears_on_revision_change() {
    let mut app = app();
    let mut snapshot = app.state.as_ref().unwrap().snapshot();
    let view = &mut std::sync::Arc::make_mut(&mut snapshot.state).observation;
    let source = view.ground_items[0].clone();
    view.ground_items = (1..=33)
        .map(|index| {
            let mut row = source.clone();
            row.item.id = super::item_target(index);
            row
        })
        .collect();
    view.combat
        .as_mut()
        .unwrap()
        .own_stats
        .as_mut()
        .unwrap()
        .abilities[0]
        .supply_costs[0]
        .quantity = 33;
    app.set_state(ClientState::from_snapshot(snapshot.clone()).unwrap());
    open(&mut app);
    for index in 1..=33 {
        app.selected = index;
        assert_eq!(app.input(Input::Key { key: Key::Wait }), Effect::None);
    }
    assert_eq!(
        app.supply_selection.as_ref().unwrap().checked.len(),
        MAX_SUPPLY_FRAGMENTS
    );
    assert!(app.status.contains("32"));
    assert_eq!(app.input(Input::Key { key: Key::Enter }), Effect::None);
    assert!(app.supply_selection.is_some());
    std::sync::Arc::make_mut(&mut snapshot.state).revision += 1;
    app.set_state(ClientState::from_snapshot(snapshot).unwrap());
    assert!(app.supply_selection.is_none());
}
