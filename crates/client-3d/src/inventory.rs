//! Interactive inventory presentation. Browsing stays local; item actions reuse the pack plan.
use crate::{
    chrome,
    interact::{self, Panel, PanelButton},
    pack::Line,
    Game,
};
use bevy::{
    input::keyboard::{Key, KeyboardInput},
    prelude::*,
};
use tor_client_3d_model::{InventoryFilter, View};
use tor_protocol::{ActorId, BranchId, ItemClass, ItemTarget, StreamContext};

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Framing<'w, 's> {
    panel: Res<'w, Panel>,
    window: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
    root:
        Query<'w, 's, (&'static ComputedNode, &'static GlobalTransform), With<interact::PanelRoot>>,
    character: Query<
        'w,
        's,
        (&'static ComputedNode, &'static GlobalTransform),
        With<crate::character::Root>,
    >,
}
impl Framing<'_, '_> {
    pub(crate) fn coverage(&self, game: &Game) -> f32 {
        let character = game.screen == Some(crate::screen::Screen::Advancement);
        if !character && (!self.panel.open || game.screen.is_some()) {
            return 0.;
        }
        let Ok(window) = self.window.single() else {
            return 0.;
        };
        if window.physical_width() == 0 {
            return 0.;
        }
        let pane = if character {
            self.character.single()
        } else {
            self.root.single()
        };
        let Ok((node, transform)) = pane else {
            return 0.;
        };
        if node.size().x == 0. {
            return 0.54;
        }
        let left = transform.translation().x - node.size().x * 0.5;
        (1. - left / window.physical_width() as f32).clamp(0., 0.95)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scope {
    actor: ActorId,
    branch: BranchId,
    stream: StreamContext,
}
impl Scope {
    pub(crate) fn matches(&self, view: &View) -> bool {
        self.actor == view.shown.actor && self.branch == view.branch && self.stream == view.context
    }
    pub(crate) fn of(view: &View) -> Self {
        Self {
            actor: view.shown.actor,
            branch: view.branch.clone(),
            stream: view.context.clone(),
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
pub(crate) enum Control {
    Page(crate::equipment::Page),
    Socket(u16),
    Select(ItemTarget),
    Filter(InventoryFilter),
    Search,
    Close,
}
#[derive(Component)]
pub(crate) struct Scroll;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct InputCapture<'w, 's> {
    panel: Res<'w, Panel>,
    pointer: chrome::UiPointer<'w, 's>,
}
impl InputCapture<'_, '_> {
    pub(crate) fn pointer(&self) -> bool {
        self.pointer.over()
    }
    pub(crate) fn keyboard(&self, game: &Game, keys: &ButtonInput<KeyCode>) -> bool {
        let arrows = [
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
        ];
        self.panel.keyboard_captured
            || (self.panel.open
                && (self.panel.search_focused
                    || (grid(game) && (keys.any_pressed(arrows) || keys.any_just_pressed(arrows)))))
    }
}

pub(crate) fn grid(game: &Game) -> bool {
    game.projectile.is_none()
        && game
            .view
            .as_ref()
            .is_some_and(|view| view.identify.is_none())
}

pub(crate) fn controls(
    game: Res<Game>,
    mut panel: ResMut<Panel>,
    mut buttons: Query<(&Interaction, &Control, &mut BackgroundColor), Changed<Interaction>>,
) {
    if !panel.open || game.screen.is_some() {
        return;
    }
    for (interaction, control, mut color) in &mut buttons {
        let active = match control {
            Control::Page(page) => panel.page == *page,
            Control::Socket(slot) => {
                game.view
                    .as_ref()
                    .and_then(|view| crate::equipment::comparison_slot(view, &panel))
                    == Some(*slot)
            }
            Control::Filter(filter) => panel.browser.filter() == *filter,
            Control::Search => panel.search_focused,
            _ => false,
        };
        color.0 = if *interaction == Interaction::None && !active {
            chrome::BUTTON
        } else {
            chrome::HOT
        };
        if *interaction != Interaction::Pressed {
            continue;
        }
        match control {
            Control::Page(page) => panel.set_page(*page),
            Control::Socket(slot) => {
                if let Some(view) = &game.view {
                    panel.equipment_slot = Some(*slot);
                    let keep_candidate = panel.browser.selected().is_some_and(|target| {
                        view.equipment
                            .item(target)
                            .is_some_and(|item| item.equipped_slot.is_none())
                            && view
                                .equipment
                                .compatible_slots(target)
                                .any(|candidate| candidate == *slot)
                    });
                    if !keep_candidate {
                        panel.browser.set_query("");
                        panel.browser.clear_selection();
                        if let Some(target) = view.equipment.occupant(*slot) {
                            panel.browser.select(view, target);
                        }
                    }
                }
                panel.search_focused = false;
            }
            Control::Select(target) => {
                if let Some(view) = &game.view {
                    panel.browser.select(view, *target);
                }
                panel.search_focused = false;
            }
            Control::Filter(filter) => {
                panel.browser.set_filter(*filter);
                panel.search_focused = false;
                panel.reset_scroll = true;
            }
            Control::Search => panel.search_focused = true,
            Control::Close => {
                panel.open = false;
                panel.search_focused = false;
            }
        }
        if let Some(view) = &game.view {
            panel.browser.reconcile(view);
        }
        panel.dirty = true;
    }
}

pub(crate) fn typing(
    mut events: EventReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    game: Res<Game>,
    mut panel: ResMut<Panel>,
) {
    if !panel.open || game.screen.is_some() || !grid(&game) {
        events.clear();
        return;
    }
    if keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && keys.just_pressed(KeyCode::KeyF)
    {
        panel.search_focused = true;
        panel.dirty = true;
        events.clear();
        return;
    }
    if panel.search_focused {
        panel.keyboard_captured = true;
        if events.is_empty() {
            return;
        }
        let mut query = panel.browser.query().to_owned();
        for event in events.read().filter(|event| event.state.is_pressed()) {
            match &event.logical_key {
                Key::Backspace => {
                    query.pop();
                }
                Key::Enter => {
                    panel.search_focused = false;
                    panel.dirty = true;
                    break;
                }
                Key::Space if query.chars().count() < tor_client_3d_model::MAX_INVENTORY_SEARCH => {
                    query.push(' ')
                }
                Key::Character(text)
                    if !keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) =>
                {
                    let remaining = tor_client_3d_model::MAX_INVENTORY_SEARCH
                        .saturating_sub(query.chars().count());
                    query.extend(text.chars().filter(|c| !c.is_control()).take(remaining));
                }
                _ => {}
            }
        }
        let before = panel.browser.query().to_owned();
        panel.browser.set_query(&query);
        if panel.browser.query() != before {
            panel.dirty = true;
            panel.reset_scroll = true;
        }
    } else {
        events.clear();
        let delta = [
            (KeyCode::ArrowLeft, -1),
            (KeyCode::ArrowRight, 1),
            (KeyCode::ArrowUp, -4),
            (KeyCode::ArrowDown, 4),
        ]
        .into_iter()
        .find(|(key, _)| keys.just_pressed(*key))
        .map(|(_, delta)| delta);
        if let (Some(delta), Some(view)) = (delta, &game.view) {
            panel.browser.shift(view, delta);
            panel.dirty = true;
            panel.keyboard_captured = true;
        }
    }
}

pub(crate) fn control(ui: &mut ChildSpawnerCommands, label: &str, action: Control, active: bool) {
    ui.spawn((
        Button,
        action,
        Node {
            padding: UiRect::axes(Val::Px(12.), Val::Px(6.)),
            border: UiRect::all(Val::Px(1.)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(if active { chrome::HOT } else { chrome::BUTTON }),
        BorderColor(chrome::GOLD),
    ))
    .with_child((Text::new(label), chrome::text(18., chrome::INK)));
}

pub(crate) fn item_target(action: PanelButton) -> Option<ItemTarget> {
    match action {
        PanelButton::Take(id)
        | PanelButton::Drop(id)
        | PanelButton::Use(id, _)
        | PanelButton::Aim(id)
        | PanelButton::ZapSelf(id)
        | PanelButton::Identify(Some(id)) => Some(id),
        _ => None,
    }
}

pub(crate) fn tile(
    ui: &mut ChildSpawnerCommands,
    id: ItemTarget,
    name: &str,
    icon: (ItemClass, tor_client_3d_model::ItemModel),
    quantity: u64,
    equipped: bool,
    selected: bool,
) {
    ui.spawn((
        Button,
        Control::Select(id),
        Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            min_width: Val::Px(0.),
            min_height: Val::Px(116.),
            padding: UiRect::all(Val::Px(6.)),
            border: UiRect::all(Val::Px(if selected { 2. } else { 1. })),
            row_gap: Val::Px(4.),
            ..default()
        },
        BackgroundColor(chrome::BUTTON),
        BorderColor(if selected {
            Color::srgb(0.93, 0.75, 0.35)
        } else {
            chrome::GOLD
        }),
    ))
    .with_children(|tile| {
        tile.spawn((
            chrome::icon(icon.0, icon.1),
            Node {
                width: Val::Px(52.),
                height: Val::Px(52.),
                ..default()
            },
        ));
        tile.spawn((
            Text::new(name),
            chrome::text(17., chrome::INK),
            TextLayout::new_with_justify(JustifyText::Center),
            Node {
                max_width: Val::Percent(100.),
                max_height: Val::Px(42.),
                overflow: Overflow::clip(),
                ..default()
            },
        ));
        if quantity > 1 {
            tile.spawn((
                Text::new(format!("x{quantity}")),
                chrome::text(15., chrome::INK),
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(5.),
                    top: Val::Px(4.),
                    ..default()
                },
            ));
        }
        if equipped {
            tile.spawn((
                Text::new("Equipped"),
                chrome::text(14., Color::srgb(0.45, 0.85, 0.74)),
            ));
        }
    });
}

pub(crate) fn grid_node() -> Node {
    Node {
        display: Display::Grid,
        grid_template_columns: RepeatedGridTrack::flex(4, 1.),
        column_gap: Val::Px(6.),
        row_gap: Val::Px(6.),
        width: Val::Percent(100.),
        flex_shrink: 0.,
        ..default()
    }
}

pub(crate) fn header(ui: &mut ChildSpawnerCommands, panel: &Panel, title: &str) {
    ui.spawn(Node {
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        flex_shrink: 0.,
        ..default()
    })
    .with_children(|header| {
        header.spawn((Text::new(title), chrome::text(29., chrome::INK)));
        control(header, "Close", Control::Close, false);
    });
    ui.spawn(Node {
        column_gap: Val::Px(6.),
        flex_wrap: FlexWrap::Wrap,
        flex_shrink: 0.,
        ..default()
    })
    .with_children(|tabs| {
        for (label, page) in [
            ("Inventory", crate::equipment::Page::Inventory),
            ("Equipment", crate::equipment::Page::Equipment),
        ] {
            control(tabs, label, Control::Page(page), panel.page == page);
        }
    });
}

pub(crate) fn search(ui: &mut ChildSpawnerCommands, panel: &Panel) {
    control(
        ui,
        &if panel.search_focused {
            format!("Search: {}_", panel.browser.query())
        } else if panel.browser.query().is_empty() {
            "Search items...  Ctrl+F".into()
        } else {
            format!("Search: {}", panel.browser.query())
        },
        Control::Search,
        panel.search_focused,
    );
}

pub(crate) fn item_actions(
    panel: &Panel,
    selected: Option<ItemTarget>,
) -> Vec<(String, PanelButton)> {
    panel
        .actions
        .iter()
        .enumerate()
        .filter(|(_, (_, action))| item_target(*action) == selected)
        .map(|(index, (label, action))| {
            let label = label
                .rsplit_once(": ")
                .map_or(label.as_str(), |(_, action)| action);
            (
                crate::pack::hotkey(index)
                    .map_or_else(|| label.to_owned(), |key| format!("{key} {label}")),
                *action,
            )
        })
        .collect()
}

pub(crate) fn draw(ui: &mut ChildSpawnerCommands, view: &View, panel: &Panel, lines: &[Line]) {
    header(ui, panel, "INVENTORY");
    ui.spawn(Node {
        column_gap: Val::Px(6.),
        flex_wrap: FlexWrap::Wrap,
        flex_shrink: 0.,
        ..default()
    })
    .with_children(|tabs| {
        for (label, filter) in [
            ("All", InventoryFilter::All),
            ("Gear", InventoryFilter::Gear),
            ("Usable", InventoryFilter::Usable),
        ] {
            control(
                tabs,
                label,
                Control::Filter(filter),
                panel.browser.filter() == filter,
            );
        }
    });
    search(ui, panel);
    ui.spawn((
        Scroll,
        chrome::ScrollArea,
        ScrollPosition {
            offset_y: panel.scroll,
            ..default()
        },
        Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.,
            flex_shrink: 1.,
            min_height: Val::Px(0.),
            overflow: Overflow::scroll_y(),
            row_gap: Val::Px(12.),
            padding: UiRect::right(Val::Px(6.)),
            ..default()
        },
    ))
    .with_children(|body| {
        let prefix = lines
            .iter()
            .position(|line| matches!(line,Line::Heading(title) if title=="Pack"))
            .unwrap_or(0);
        interact::draw_lines(body, &lines[..prefix]);
        let carried: Vec<_> = panel.browser.carried(view).collect();
        body.spawn((
            Text::new(format!(
                "CARRIED  {} shown / {} stacks",
                carried.len(),
                view.inventory.len()
            )),
            chrome::text(18., chrome::INK),
        ));
        if carried.is_empty() {
            body.spawn((
                Text::new(if view.inventory.is_empty() {
                    "You carry nothing."
                } else {
                    "No items match this filter."
                }),
                chrome::text(18., chrome::MUTED),
            ));
        }
        body.spawn(grid_node()).with_children(|grid| {
            for item in carried {
                tile(
                    grid,
                    item.id,
                    &item.name,
                    (item.class, item.model),
                    item.quantity,
                    item.equipped.is_some(),
                    panel.browser.selected() == Some(item.id),
                );
            }
        });
        let selected = panel.browser.selected();
        let detail = selected
            .and_then(|id| {
                view.inventory
                    .iter()
                    .find(|item| item.id == id)
                    .map(|item| {
                        (
                            item.name.as_str(),
                            item.description.as_str(),
                            item.quantity,
                            item.equipped.as_deref(),
                        )
                    })
            })
            .or_else(|| {
                selected.and_then(|id| {
                    view.items
                        .iter()
                        .find(|item| item.id == id && item.reachable)
                        .map(|item| {
                            (
                                item.name.as_str(),
                                item.description.as_str(),
                                item.quantity,
                                None,
                            )
                        })
                })
            });
        body.spawn((
            chrome::frame(),
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(14.)),
                border: UiRect::all(Val::Px(1.)),
                row_gap: Val::Px(7.),
                flex_shrink: 0.,
                ..default()
            },
        ))
        .with_children(|details| {
            if let Some((name, description, quantity, equipped)) = detail {
                details.spawn((Text::new(name), chrome::text(23., chrome::INK)));
                if !description.is_empty() {
                    details.spawn((Text::new(description), chrome::text(18., chrome::INK)));
                }
                details.spawn((
                    Text::new(format!(
                        "Quantity: {quantity}{}",
                        equipped.map_or_else(String::new, |slot| format!("  |  Equipped: {slot}"))
                    )),
                    chrome::text(17., chrome::MUTED),
                ));
                let buttons = item_actions(panel, selected);
                interact::row(details, "", None, &buttons);
            } else {
                details.spawn((
                    Text::new("Select an item to inspect it and see its actions."),
                    chrome::text(19., chrome::MUTED),
                ));
            }
        });
        let reachable: Vec<_> = view.items.iter().filter(|item| item.reachable).collect();
        body.spawn((Text::new("WITHIN REACH"), chrome::text(20., chrome::INK)));
        if reachable.is_empty() {
            body.spawn((
                Text::new("Nothing within reach."),
                chrome::text(18., chrome::MUTED),
            ));
        }
        body.spawn(grid_node()).with_children(|grid| {
            for item in reachable {
                tile(
                    grid,
                    item.id,
                    &item.name,
                    (item.class, item.model),
                    item.quantity,
                    false,
                    selected == Some(item.id),
                );
            }
        });
    });
    ui.spawn((
        Text::new("Tab: equipment  |  Arrows: select  |  1-0: pack actions  |  Ctrl+F: search  |  Esc: close"),
        chrome::text(15., chrome::MUTED),
        Node {
            flex_shrink: 0.,
            ..default()
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::input::ButtonState;

    #[test]
    fn character_and_inventory_use_their_own_actual_panes_for_framing() {
        #[derive(Resource, Default)]
        struct Measured(f32);
        fn measure(game: Res<Game>, framing: Framing, mut value: ResMut<Measured>) {
            value.0 = framing.coverage(&game);
        }
        let mut app = App::new();
        app.insert_resource(Game {
            screen: Some(crate::screen::Screen::Advancement),
            ..default()
        })
        .init_resource::<Panel>()
        .init_resource::<Measured>()
        .add_systems(Update, measure);
        app.world_mut().spawn((
            bevy::window::PrimaryWindow,
            Window {
                resolution: bevy::window::WindowResolution::new(1600., 1000.)
                    .with_scale_factor_override(1.),
                ..default()
            },
        ));
        app.world_mut().spawn((
            crate::character::Root,
            ComputedNode {
                size: Vec2::new(880., 800.),
                ..default()
            },
            GlobalTransform::from_translation(Vec3::new(1160., 400., 0.)),
        ));
        app.world_mut().spawn((
            interact::PanelRoot,
            ComputedNode {
                size: Vec2::new(700., 800.),
                ..default()
            },
            GlobalTransform::from_translation(Vec3::new(1250., 400., 0.)),
        ));
        app.update();
        assert!((app.world().resource::<Measured>().0 - 0.55).abs() < 1e-6);
        app.world_mut().resource_mut::<Panel>().open = true;
        app.update();
        assert!(
            (app.world().resource::<Measured>().0 - 0.55).abs() < 1e-6,
            "hidden equipment must not choose character framing"
        );
        app.world_mut().resource_mut::<Game>().screen = None;
        app.update();
        assert!((app.world().resource::<Measured>().0 - 0.4375).abs() < 1e-6);
        app.world_mut().resource_mut::<Game>().screen = Some(crate::screen::Screen::History);
        app.update();
        assert_eq!(app.world().resource::<Measured>().0, 0.);
    }

    #[test]
    fn equipment_tabs_capture_control_transfer_and_keep_item_actions_in_plan_order() {
        let (mut app, mut receiver, id) = fixture();
        app.world_mut().resource_mut::<Panel>().search_focused = false;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Tab);
        app.update();
        let panel = app.world().resource::<Panel>();
        assert_eq!(panel.page, crate::equipment::Page::Equipment);
        assert_eq!(panel.browser.filter(), InventoryFilter::Gear);
        assert!(
            receiver.try_recv().is_err(),
            "Tab in a panel must not transfer actor control"
        );
        assert_eq!(
            item_actions(panel, Some(id))[0].1,
            PanelButton::Use(id, tor_client_common::items::ItemOperation::Equip)
        );
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.release(KeyCode::Tab);
        keys.clear();
        keys.press(KeyCode::KeyI);
        app.update();
        assert!(app.world().resource::<Panel>().open);
        assert_eq!(
            app.world().resource::<Panel>().page,
            crate::equipment::Page::Inventory
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn equipment_shortcut_toggles_locally_and_search_holds_the_page() {
        let (mut app, mut receiver, _) = fixture();
        app.world_mut().resource_mut::<Panel>().search_focused = false;
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::F6);
        app.update();
        assert_eq!(
            app.world().resource::<Panel>().page,
            crate::equipment::Page::Equipment
        );
        assert!(app.world().resource::<Panel>().open);
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.search_focused = true;
        }
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::F6);
            keys.clear();
            keys.press(KeyCode::F6);
        }
        app.update();
        assert!(
            app.world().resource::<Panel>().open,
            "search owns function shortcuts too"
        );
        app.world_mut().resource_mut::<Panel>().search_focused = false;
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::F6);
            keys.clear();
            keys.press(KeyCode::F6);
        }
        app.update();
        assert!(!app.world().resource::<Panel>().open);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn active_tabs_and_search_keep_their_feedback_without_pointer_hover() {
        let mut app = App::new();
        app.insert_resource(Game {
            view: Some(crate::test_support::view(1)),
            ..default()
        })
        .init_resource::<Panel>()
        .add_systems(Update, controls);
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.open = true;
            panel.search_focused = true;
        }
        let all = app
            .world_mut()
            .spawn((
                Control::Filter(InventoryFilter::All),
                Interaction::None,
                BackgroundColor(chrome::BUTTON),
            ))
            .id();
        let search = app
            .world_mut()
            .spawn((
                Control::Search,
                Interaction::None,
                BackgroundColor(chrome::BUTTON),
            ))
            .id();
        let close = app
            .world_mut()
            .spawn((
                Control::Close,
                Interaction::None,
                BackgroundColor(chrome::BUTTON),
            ))
            .id();
        app.update();
        let idle = app.world().get::<BackgroundColor>(close).unwrap();
        assert_ne!(app.world().get::<BackgroundColor>(all).unwrap(), idle);
        assert_ne!(app.world().get::<BackgroundColor>(search).unwrap(), idle);
    }
    fn fixture() -> (
        App,
        tokio::sync::mpsc::Receiver<crate::net::Input>,
        ItemTarget,
    ) {
        let (net, receiver) = crate::net::test_channel();
        let mut view = crate::test_support::view(1);
        view.has_control = true;
        view.can_act = true;
        let id = ItemTarget::from_digest([7; 32]);
        view.inventory = vec![tor_client_3d_model::Carried {
            id,
            model: tor_client_3d_model::ItemModel::ShortSword,
            name: "Iron sword".into(),
            class: ItemClass::Weapon,
            quantity: 1,
            description: "An iron blade.".into(),
            equipped: None,
            can_equip: true,
            drinkable: false,
            edible: false,
            readable: false,
            device: None,
            portable_light: None,
        }];
        let actions = crate::pack::actions(&crate::pack::plan(&view, None));
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(view),
                ..default()
            })
            .init_resource::<Panel>()
            .init_resource::<Time>()
            .init_resource::<crate::CameraRig>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<interact::Hover>()
            .add_event::<KeyboardInput>()
            .add_event::<bevy::input::mouse::MouseWheel>()
            .add_event::<AppExit>()
            .add_systems(
                Update,
                (interact::keys, typing, crate::camera::input).chain(),
            );
        {
            let mut panel = app.world_mut().resource_mut::<Panel>();
            panel.open = true;
            panel.search_focused = true;
            panel.actions = actions;
        }
        (app, receiver, id)
    }
    fn event(key: Key) -> KeyboardInput {
        KeyboardInput {
            key_code: KeyCode::KeyW,
            logical_key: key,
            state: ButtonState::Pressed,
            text: None,
            repeat: false,
            window: Entity::PLACEHOLDER,
        }
    }
    #[test]
    fn search_captures_movement_camera_and_item_shortcuts() {
        let (mut app, mut receiver, _) = fixture();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            for key in [KeyCode::KeyW, KeyCode::KeyQ, KeyCode::Digit1] {
                keys.press(key);
            }
        }
        app.world_mut()
            .send_event(event(Key::Character("wq1".into())));
        app.update();
        assert_eq!(app.world().resource::<Panel>().browser.query(), "wq1");
        assert_eq!(app.world().resource::<crate::CameraRig>().facing, 0);
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn opening_search_captures_other_keys_delivered_in_the_same_frame() {
        let (mut app, mut receiver, _) = fixture();
        app.world_mut().resource_mut::<Panel>().search_focused = false;
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            for key in [
                KeyCode::ControlLeft,
                KeyCode::KeyF,
                KeyCode::KeyW,
                KeyCode::Digit1,
            ] {
                keys.press(key);
            }
        }
        app.world_mut()
            .send_event(event(Key::Character("f1w".into())));
        app.update();
        let panel = app.world().resource::<Panel>();
        assert!(panel.search_focused);
        assert_eq!(panel.browser.query(), "");
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn closing_search_does_not_release_same_frame_letters_into_the_world() {
        let (mut app, mut receiver, _) = fixture();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.world_mut()
            .send_event(event(Key::Character("w".into())));
        app.world_mut().send_event(event(Key::Enter));
        app.update();
        assert!(!app.world().resource::<Panel>().search_focused);
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn a_short_arrow_tap_selects_without_sending_a_move() {
        let (mut app, mut receiver, id) = fixture();
        app.world_mut().resource_mut::<Panel>().search_focused = false;
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ArrowRight);
            keys.release(KeyCode::ArrowRight);
        }
        app.update();
        assert_eq!(app.world().resource::<Panel>().browser.selected(), Some(id));
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn a_new_branch_clears_selection_search_and_scroll_even_if_the_item_remains() {
        let (mut app, _, id) = fixture();
        app.world_mut().remove_resource::<crate::Net>();
        // Use only the presentation owner; the interaction systems above require Net.
        let mut app2 = App::new();
        let mut view = app.world().resource::<Game>().view.clone().unwrap();
        let mut panel = app.world_mut().remove_resource::<Panel>().unwrap();
        panel.scope = Some(Scope::of(&view));
        panel.scroll = 75.;
        panel.browser.set_query("sword");
        assert!(panel.browser.select(&view, id));
        view.branch = BranchId("rewound".into());
        view.shown.branch = view.branch.clone();
        app2.insert_resource(Game {
            view: Some(view),
            view_serial: 2,
            ..default()
        })
        .insert_resource(panel)
        .add_systems(Update, interact::panel);
        app2.world_mut()
            .spawn((interact::PanelRoot, Visibility::Hidden, Node::default()));
        app2.update();
        let panel = app2.world().resource::<Panel>();
        assert_eq!(panel.browser.selected(), None);
        assert_eq!(panel.browser.query(), "");
        assert!(!panel.search_focused);
        assert_eq!(panel.scroll, 0.);
    }
}
