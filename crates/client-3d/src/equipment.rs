//! Equipment presentation shares the inventory's selection and item action path.
use crate::{
    chrome,
    interact::{self, Panel},
    inventory::{self, Control},
};
use bevy::prelude::*;
use tor_client_3d_model::{InventoryFilter, View};
use tor_protocol::ItemTarget;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Page {
    #[default]
    Inventory,
    Equipment,
}

#[derive(Component)]
struct EmptySocket;

impl Panel {
    pub(crate) fn set_page(&mut self, page: Page) {
        if self.page != page {
            self.page = page;
            self.browser.set_query("");
            self.browser.set_filter(if page == Page::Equipment {
                InventoryFilter::Gear
            } else {
                InventoryFilter::All
            });
            self.equipment_slot = None;
            self.reset_scroll = true;
        }
        self.search_focused = false;
        self.dirty = true;
    }
    pub(crate) fn toggle_page(&mut self, page: Page) {
        self.open = !self.open || self.page != page;
        self.set_page(page);
    }
}

pub(crate) fn comparison_slot(view: &View, panel: &Panel) -> Option<u16> {
    match panel.browser.selected() {
        Some(target) => view.equipment.comparison_slot(target, panel.equipment_slot),
        None => panel
            .equipment_slot
            .filter(|slot| usize::from(*slot) < view.equipment.slots.len()),
    }
}

fn card(
    ui: &mut ChildSpawnerCommands,
    view: &View,
    title: &str,
    target: Option<ItemTarget>,
    panel: &Panel,
    actions: bool,
) {
    ui.spawn((
        chrome::frame(),
        Node {
            flex_direction: FlexDirection::Column,
            flex_grow: 1.,
            flex_basis: Val::Px(0.),
            min_width: Val::Px(0.),
            padding: UiRect::all(Val::Px(12.)),
            border: UiRect::all(Val::Px(1.)),
            row_gap: Val::Px(8.),
            ..default()
        },
    ))
    .with_children(|card| {
        card.spawn((Text::new(title), chrome::text(17., chrome::GOLD)));
        let item = target.and_then(|target| view.inventory.iter().find(|item| item.id == target));
        if let Some(item) = item {
            card.spawn(Node {
                align_items: AlignItems::Center,
                column_gap: Val::Px(10.),
                flex_shrink: 0.,
                min_width: Val::Px(0.),
                ..default()
            })
            .with_children(|header| {
                header.spawn((
                    chrome::icon(item.class, item.model),
                    Node {
                        width: Val::Px(48.),
                        height: Val::Px(48.),
                        flex_shrink: 0.,
                        ..default()
                    },
                ));
                header
                    .spawn(Node {
                        flex_direction: FlexDirection::Column,
                        flex_grow: 1.,
                        min_width: Val::Px(0.),
                        row_gap: Val::Px(4.),
                        ..default()
                    })
                    .with_children(|names| {
                        names.spawn((Text::new(&item.name), chrome::text(23., chrome::INK)));
                        if let Some(slot) = &item.equipped {
                            names.spawn((Text::new(slot), chrome::text(17., chrome::MUTED)));
                        }
                    });
            });
            if actions {
                let mut buttons = inventory::item_actions(panel, Some(item.id));
                if item.equipped.is_none()
                    && item.can_equip
                    && !view.equipment.has_free_slot(item.id)
                {
                    buttons.retain(|(_, action)| {
                        !matches!(
                            action,
                            interact::PanelButton::Use(
                                _,
                                tor_client_common::items::ItemOperation::Equip
                            )
                        )
                    });
                    card.spawn((
                        Text::new("Remove an item from a matching slot before equipping this one."),
                        chrome::text(17., chrome::GOLD),
                    ));
                }
                interact::row(card, "", None, &buttons);
            }
            for fact in view.equipment.facts(item.id) {
                card.spawn((Text::new(fact), chrome::text(17., chrome::INK)));
            }
            if !item.description.is_empty() {
                card.spawn((
                    Text::new(&item.description),
                    chrome::text(17., chrome::MUTED),
                ));
            }
        } else {
            card.spawn((
                Text::new(if actions {
                    "Select a carried item to compare its remembered properties."
                } else if title == "NO COMPARISON" {
                    "Select an item or a compatible equipment slot."
                } else {
                    "Empty slot"
                }),
                chrome::text(18., chrome::MUTED),
            ));
        }
    });
}

pub(crate) fn draw(ui: &mut ChildSpawnerCommands, view: &View, panel: &Panel) {
    inventory::header(ui, panel, "EQUIPMENT");
    inventory::search(ui, panel);
    ui.spawn((
        inventory::Scroll,
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
        body.spawn((Text::new("EQUIPMENT SLOTS"), chrome::text(19., chrome::INK)));
        if view.equipment.slots.is_empty() {
            body.spawn((
                Text::new("No equipment slots disclosed."),
                chrome::text(18., chrome::MUTED),
            ));
        }
        let selected = panel.browser.selected();
        let comparison = comparison_slot(view, panel);
        body.spawn(Node {
            display: Display::Grid,
            grid_template_columns: RepeatedGridTrack::flex(3, 1.),
            column_gap: Val::Px(6.),
            row_gap: Val::Px(6.),
            flex_shrink: 0.,
            ..default()
        })
        .with_children(|slots| {
            for (index, slot) in view.equipment.slots.iter().enumerate() {
                let Ok(index) = u16::try_from(index) else {
                    continue;
                };
                let occupant = view
                    .equipment
                    .occupant(index)
                    .and_then(|id| view.inventory.iter().find(|item| item.id == id));
                slots
                    .spawn((
                        Button,
                        Control::Socket(index),
                        BackgroundColor(if comparison == Some(index) {
                            chrome::HOT
                        } else {
                            chrome::BUTTON
                        }),
                        BorderColor(chrome::GOLD),
                        Node {
                            flex_direction: FlexDirection::Row,
                            align_items: AlignItems::Center,
                            min_height: Val::Px(64.),
                            padding: UiRect::all(Val::Px(6.)),
                            border: UiRect::all(Val::Px(1.)),
                            column_gap: Val::Px(8.),
                            min_width: Val::Px(0.),
                            ..default()
                        },
                    ))
                    .with_children(|tile| {
                        if let Some(item) = occupant {
                            tile.spawn((
                                chrome::icon(item.class, item.model),
                                Node {
                                    width: Val::Px(32.),
                                    height: Val::Px(32.),
                                    flex_shrink: 0.,
                                    ..default()
                                },
                            ));
                        } else {
                            tile.spawn((
                                EmptySocket,
                                Node {
                                    width: Val::Px(26.),
                                    height: Val::Px(26.),
                                    border: UiRect::all(Val::Px(1.)),
                                    flex_shrink: 0.,
                                    ..default()
                                },
                                BorderColor(chrome::GOLD),
                                BorderRadius::all(Val::Px(13.)),
                            ));
                        }
                        tile.spawn(Node {
                            flex_direction: FlexDirection::Column,
                            min_width: Val::Px(0.),
                            ..default()
                        })
                        .with_children(|labels| {
                            labels.spawn((
                                Text::new(format!("{} · {}", index + 1, slot.name)),
                                chrome::text(17., chrome::INK),
                            ));
                            labels.spawn((
                                Text::new(occupant.map_or("Empty", |item| item.name.as_str())),
                                chrome::text(
                                    16.,
                                    if occupant.is_some() {
                                        chrome::INK
                                    } else {
                                        chrome::MUTED
                                    },
                                ),
                            ));
                        });
                    });
            }
        });
        body.spawn(Node {
            column_gap: Val::Px(8.),
            flex_shrink: 0.,
            align_items: AlignItems::Stretch,
            ..default()
        })
        .with_children(|comparison_ui| {
            let equipped = comparison.and_then(|slot| view.equipment.occupant(slot));
            card(
                comparison_ui,
                view,
                if comparison.is_some() {
                    "EQUIPPED"
                } else {
                    "NO COMPARISON"
                },
                equipped,
                panel,
                equipped.is_some() && equipped != selected,
            );
            card(comparison_ui, view, "SELECTED", selected, panel, true);
        });
        let carried: Vec<_> = panel.browser.carried(view).collect();
        body.spawn((
            Text::new(format!("CARRIED GEAR · {} shown", carried.len())),
            chrome::text(19., chrome::INK),
        ));
        if carried.is_empty() {
            body.spawn((
                Text::new("No carried gear matches this search."),
                chrome::text(18., chrome::MUTED),
            ));
        }
        body.spawn(inventory::grid_node()).with_children(|grid| {
            for item in carried {
                inventory::tile(
                    grid,
                    item.id,
                    &item.name,
                    (item.class, item.model),
                    item.quantity,
                    item.equipped.is_some(),
                    selected == Some(item.id),
                );
            }
        });
    });
    ui.spawn((
        Text::new("Tab: inventory  |  Arrows: select  |  Ctrl+F: search  |  Esc: close"),
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
    use tor_protocol::{EquipmentSlot, ItemClass, ItemInteractionView};

    fn fixture() -> (View, Panel, ItemTarget) {
        let mut view = crate::test_support::view(1);
        let id = ItemTarget::from_digest([71; 32]);
        view.inventory = vec![tor_client_3d_model::Carried {
            model: tor_client_3d_model::ItemModel::Mace,
            id,
            name: "Disclosed mace".into(),
            class: ItemClass::Weapon,
            quantity: 1,
            description: "A disclosed weapon.".into(),
            equipped: Some("in a grip".into()),
            can_equip: false,
            drinkable: false,
            portable_light: None,
            edible: false,
            readable: false,
            device: None,
        }];
        let slot = EquipmentSlot {
            id: "grip".into(),
            name: "Grip".into(),
            equipped_phrase: "in a grip".into(),
            requires_attack: true,
        };
        view.equipment = tor_client_3d_model::Equipment {
            slots: vec![slot.clone(), slot.clone()],
            items: vec![ItemInteractionView {
                item: id,
                slot: Some(slot),
                equipped_slot: Some(0),
                known_equipment: None,
                known_projectile_bonus: None,
                known_cursed: None,
                known_protected: None,
                drinkable: false,
                edible: false,
                readable: false,
                zappable: false,
                device_activation: None,
                charges: None,
                portable_light: None,
            }],
        };
        let mut panel = Panel::default();
        panel.toggle_page(Page::Equipment);
        assert!(panel.browser.select(&view, id));
        panel.actions = crate::pack::actions(&crate::pack::plan(&view, None));
        (view, panel, id)
    }

    #[test]
    fn equipment_renders_declared_sockets_unknown_properties_and_original_action_numbers() {
        let (view, panel, _) = fixture();
        let mut app = App::new();
        app.insert_resource(crate::Game {
            view: Some(view),
            ..default()
        })
        .insert_resource(panel)
        .add_systems(
            Startup,
            |mut commands: Commands, game: Res<crate::Game>, panel: Res<Panel>| {
                commands
                    .spawn_empty()
                    .with_children(|ui| draw(ui, game.view.as_ref().unwrap(), &panel));
            },
        );
        app.update();
        let text: Vec<_> = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect();
        for expected in [
            "EQUIPMENT",
            "1 · Grip",
            "2 · Grip",
            "Empty",
            "Combat properties not known.",
            "1 Remove",
        ] {
            assert!(
                text.iter().any(|text| text == expected),
                "missing {expected}: {text:?}"
            );
        }
        assert_eq!(
            app.world_mut()
                .query::<&Control>()
                .iter(app.world())
                .filter(|control| matches!(control, Control::Socket(_)))
                .count(),
            2
        );
        assert_eq!(
            app.world_mut()
                .query_filtered::<Entity, With<EmptySocket>>()
                .iter(app.world())
                .count(),
            1
        );
        assert!(
            !text.iter().any(|text| text == "◇"),
            "empty slots use drawn indicators, not a potentially missing font glyph"
        );
        let selected_title = app
            .world_mut()
            .query::<(Entity, &Text)>()
            .iter(app.world())
            .find(|(_, text)| text.0 == "SELECTED")
            .unwrap()
            .0;
        let selected_card = app.world().get::<ChildOf>(selected_title).unwrap().parent();
        let mut pending = vec![selected_card];
        let mut selected_text = Vec::new();
        while let Some(entity) = pending.pop() {
            if let Some(text) = app.world().get::<Text>(entity) {
                selected_text.push(text.0.clone());
            }
            if let Some(children) = app.world().get::<Children>(entity) {
                pending.extend(children.iter().rev());
            }
        }
        let action = selected_text
            .iter()
            .position(|text| text == "1 Remove")
            .unwrap();
        let properties = selected_text
            .iter()
            .position(|text| text == "Combat properties not known.")
            .unwrap();
        assert!(
            action < properties,
            "item actions must precede potentially long property text: {selected_text:?}"
        );
    }

    #[test]
    fn empty_socket_selection_clears_the_old_item_and_does_not_emit_actions() {
        let (view, panel, _) = fixture();
        let mut app = App::new();
        let (net, mut receiver) = crate::net::test_channel();
        app.insert_resource(net)
            .insert_resource(crate::Game {
                view: Some(view),
                ..default()
            })
            .insert_resource(panel)
            .add_systems(Update, inventory::controls);
        app.world_mut().spawn((
            Control::Socket(1),
            Interaction::Pressed,
            BackgroundColor(chrome::BUTTON),
        ));
        app.update();
        let panel = app.world().resource::<Panel>();
        assert_eq!(panel.browser.selected(), None);
        assert_eq!(
            comparison_slot(
                app.world().resource::<crate::Game>().view.as_ref().unwrap(),
                panel
            ),
            Some(1)
        );
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn a_full_matching_slot_shows_removal_guidance_and_keeps_remove_on_the_current_item() {
        let (mut view, mut panel, id) = fixture();
        view.equipment.slots.truncate(1);
        let mut candidate = view.inventory[0].clone();
        let target = ItemTarget::from_digest([72; 32]);
        candidate.id = target;
        candidate.name = "Candidate".into();
        candidate.equipped = None;
        candidate.can_equip = true;
        view.inventory.push(candidate);
        let mut offer = view.equipment.items[0].clone();
        offer.item = target;
        offer.equipped_slot = None;
        view.equipment.items.push(offer);
        assert!(panel.browser.select(&view, target));
        panel.actions = crate::pack::actions(&crate::pack::plan(&view, None));
        let mut app = App::new();
        app.insert_resource(crate::Game {
            view: Some(view),
            ..default()
        })
        .insert_resource(panel)
        .add_systems(
            Startup,
            |mut commands: Commands, game: Res<crate::Game>, panel: Res<Panel>| {
                commands
                    .spawn_empty()
                    .with_children(|ui| draw(ui, game.view.as_ref().unwrap(), &panel));
            },
        );
        app.update();
        let text: Vec<_> = app
            .world_mut()
            .query::<&Text>()
            .iter(app.world())
            .map(|text| text.0.clone())
            .collect();
        assert!(text
            .iter()
            .any(|text| text == "Remove an item from a matching slot before equipping this one."));
        let buttons: Vec<_> = app
            .world_mut()
            .query::<&interact::PanelButton>()
            .iter(app.world())
            .copied()
            .collect();
        assert!(buttons.contains(&interact::PanelButton::Use(
            id,
            tor_client_common::items::ItemOperation::Unequip
        )));
        assert!(!buttons.contains(&interact::PanelButton::Use(
            target,
            tor_client_common::items::ItemOperation::Equip
        )));
    }

    #[test]
    fn changing_pages_resets_search_and_scroll_and_unknown_slots_stay_absent() {
        let (view, mut panel, id) = fixture();
        panel.browser.set_query("missing");
        panel.search_focused = true;
        panel.equipment_slot = Some(1);
        panel.reset_scroll = false;
        panel.set_page(Page::Inventory);
        assert_eq!(panel.browser.query(), "");
        assert_eq!(panel.browser.selected(), Some(id));
        assert_eq!(panel.browser.filter(), InventoryFilter::All);
        assert!(panel.reset_scroll && !panel.search_focused);
        assert_eq!(panel.equipment_slot, None);
        let mut hidden = view;
        hidden.equipment = default();
        assert_eq!(comparison_slot(&hidden, &panel), None);
    }
}
