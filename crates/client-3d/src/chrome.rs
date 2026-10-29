//! Shared visual vocabulary and pointer boundary for the live interface.
use bevy::prelude::*;
use bevy::{
    asset::{weak_handle, RenderAssetUsages},
    input::mouse::{MouseScrollUnit, MouseWheel},
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    sprite::{BorderRect, SliceScaleMode, TextureSlicer},
    ui::FocusPolicy,
    window::PrimaryWindow,
};

mod art;
pub(crate) const FONT: Handle<Font> = weak_handle!("83c6b8d9-c118-4ea0-ab4f-d2b52d09aa71");
const PANEL: Handle<Image> = weak_handle!("83c6b8d9-c118-4ea0-ab4f-d2b52d09aa72");
const ICONS: Handle<Image> = weak_handle!("83c6b8d9-c118-4ea0-ab4f-d2b52d09aa73");

#[derive(Component)]
pub(crate) struct FrameSkin;
#[derive(Component)]
pub(crate) struct Decoration;
#[derive(Component)]
pub(crate) struct ScrollArea;

pub(crate) fn setup(mut fonts: ResMut<Assets<Font>>, mut images: ResMut<Assets<Image>>) {
    fonts.insert(
        FONT.id(),
        Font::try_from_bytes(include_bytes!("../assets/ui/CrimsonPro.ttf").to_vec())
            .expect("bundled Crimson Pro font"),
    );
    for (handle, width, height, pixels) in [
        (PANEL, 128, 128, art::panel_pixels()),
        (
            ICONS,
            art::ATLAS as u32,
            art::ATLAS_HEIGHT as u32,
            art::atlas_pixels(),
        ),
    ] {
        images.insert(
            handle.id(),
            Image::new(
                Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                pixels,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
            ),
        );
    }
}

pub(crate) fn icon(
    class: tor_protocol::ItemClass,
    model: tor_client_3d_model::ItemModel,
) -> ImageNode {
    ImageNode {
        image: ICONS,
        rect: Some(art::icon_rect(class, model)),
        ..default()
    }
}

/// Recreate the decorative background only when a panel's children are rebuilt.
pub(crate) fn skin(
    mut commands: Commands,
    frames: Query<(Entity, Option<&Children>), With<FrameSkin>>,
    decorations: Query<(), With<Decoration>>,
) {
    for (entity, children) in &frames {
        if children.is_some_and(|children| children.iter().any(|child| decorations.contains(child)))
        {
            continue;
        }
        commands.entity(entity).with_child((
            Decoration,
            FocusPolicy::Pass,
            ZIndex(-1),
            ImageNode {
                image: PANEL,
                image_mode: NodeImageMode::Sliced(TextureSlicer {
                    border: BorderRect::all(15.0),
                    center_scale_mode: SliceScaleMode::Tile { stretch_value: 1.0 },
                    ..default()
                }),
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ));
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct UiPointer<'w, 's> {
    windows: Query<'w, 's, (Entity, &'static Window), With<PrimaryWindow>>,
    regions: Query<
        'w,
        's,
        (
            &'static ComputedNode,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
        With<BlockWorld>,
    >,
}
impl UiPointer<'_, '_> {
    pub(crate) fn over(&self) -> bool {
        let Ok((_, window)) = self.windows.single() else {
            return false;
        };
        let Some(cursor) = window.cursor_position() else {
            return false;
        };
        let point = cursor * window.scale_factor();
        self.regions
            .iter()
            .any(|(node, transform, visible)| visible.get() && hit(node, transform, point))
    }
}
fn hit(node: &ComputedNode, transform: &GlobalTransform, point: Vec2) -> bool {
    node.size().min_element() > 0.0
        && Rect::from_center_size(transform.translation().truncate(), node.size()).contains(point)
}

pub(crate) fn scroll(
    mut wheel: EventReader<MouseWheel>,
    pointer: UiPointer,
    mut areas: Query<
        (
            &ComputedNode,
            &GlobalTransform,
            &InheritedVisibility,
            &mut ScrollPosition,
        ),
        With<ScrollArea>,
    >,
) {
    let Ok((window_entity, window)) = pointer.windows.single() else {
        wheel.clear();
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        wheel.clear();
        return;
    };
    let delta: f32 = wheel
        .read()
        .filter(|event| event.window == window_entity)
        .map(|e| {
            -e.y * if e.unit == MouseScrollUnit::Line {
                28.0
            } else {
                1.0
            }
        })
        .sum();
    if delta == 0.0 {
        return;
    }
    for (node, transform, visible, mut position) in &mut areas {
        if visible.get() && hit(node, transform, cursor * window.scale_factor()) {
            let max =
                (node.content_size().y - node.size().y).max(0.0) * node.inverse_scale_factor();
            position.offset_y = (position.offset_y + delta).clamp(0.0, max);
        }
    }
}

pub(crate) const INK: Color = Color::srgb(0.93, 0.86, 0.69);
pub(crate) const MUTED: Color = Color::srgb(0.64, 0.59, 0.48);
pub(crate) const GOLD: Color = Color::srgb(0.65, 0.46, 0.23);
pub(crate) const STONE: Color = Color::srgba(0.035, 0.032, 0.028, 0.97);
pub(crate) const BUTTON: Color = Color::srgb(0.09, 0.075, 0.055);
pub(crate) const HOT: Color = Color::srgb(0.12, 0.24, 0.21);

/// Empty areas of framed UI consume pointer input too.
#[derive(Component)]
pub(crate) struct BlockWorld;

pub(crate) fn frame() -> impl Bundle {
    (
        FrameSkin,
        BackgroundColor(STONE),
        BorderColor(GOLD),
        BorderRadius::all(Val::Px(3.0)),
    )
}

pub(crate) fn text(size: f32, color: Color) -> impl Bundle {
    (
        TextFont {
            font: FONT,
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolling_is_clamped_and_does_not_zoom_the_world_camera() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut app = App::new();
        let mut window = Window::default();
        window.set_cursor_position(Some(Vec2::new(200., 150.)));
        let entity = app.world_mut().spawn((PrimaryWindow, window)).id();
        let area = app
            .world_mut()
            .spawn((
                BlockWorld,
                ScrollArea,
                ComputedNode {
                    size: Vec2::new(100., 100.),
                    content_size: Vec2::new(100., 300.),
                    inverse_scale_factor: 1.,
                    ..default()
                },
                GlobalTransform::from_translation(Vec3::new(200., 150., 0.)),
                InheritedVisibility::VISIBLE,
                ScrollPosition::default(),
            ))
            .id();
        app.insert_resource(net)
            .init_resource::<crate::Game>()
            .init_resource::<crate::CameraRig>()
            .init_resource::<crate::interact::Panel>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<Time>()
            .add_event::<MouseWheel>()
            .add_systems(Update, (scroll, crate::camera::input).chain());
        let distance = app.world().resource::<crate::CameraRig>().distance;
        for (y, expected) in [(-3., 84.), (-100., 200.), (100., 0.)] {
            app.world_mut().send_event(MouseWheel {
                unit: MouseScrollUnit::Line,
                x: 0.,
                y,
                window: entity,
            });
            app.update();
            assert_eq!(
                app.world().get::<ScrollPosition>(area).unwrap().offset_y,
                expected
            );
            assert_eq!(
                app.world().resource::<crate::CameraRig>().distance,
                distance
            );
        }
        app.world_mut()
            .entity_mut(area)
            .insert(InheritedVisibility::HIDDEN);
        app.world_mut().send_event(MouseWheel {
            unit: MouseScrollUnit::Line,
            x: 0.,
            y: -3.,
            window: entity,
        });
        app.update();
        assert_eq!(
            app.world().get::<ScrollPosition>(area).unwrap().offset_y,
            0.
        );
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn artwork_is_bounded_repeatable_and_contains_visible_symbols() {
        let panel = art::panel_pixels();
        assert_eq!(panel.len(), 128 * 128 * 4);
        assert_eq!(panel, art::panel_pixels());
        let atlas = art::atlas_pixels();
        assert_eq!(atlas.len(), art::ATLAS * art::ATLAS_HEIGHT * 4);
        for class in [
            tor_protocol::ItemClass::Weapon,
            tor_protocol::ItemClass::Armor,
            tor_protocol::ItemClass::Potion,
            tor_protocol::ItemClass::Misc,
        ] {
            let rect = art::icon_rect(class, tor_client_3d_model::ItemModel::Placeholder);
            assert!(rect
                .max
                .cmple(Vec2::new(art::ATLAS as f32, art::ATLAS_HEIGHT as f32))
                .all());
            let mut opaque = 0;
            for y in rect.min.y as usize..rect.max.y as usize {
                for x in rect.min.x as usize..rect.max.x as usize {
                    opaque += usize::from(atlas[(y * art::ATLAS + x) * 4 + 3] > 0);
                }
            }
            assert!(opaque > 300);
        }
        assert!(
            Font::try_from_bytes(include_bytes!("../assets/ui/CrimsonPro.ttf").to_vec()).is_ok()
        );
    }
    #[test]
    fn known_weapon_and_armor_models_keep_their_disclosed_visual_identity() {
        use tor_client_3d_model::ItemModel;
        use tor_protocol::ItemClass;
        assert_ne!(
            art::icon_rect(ItemClass::Weapon, ItemModel::Mace),
            art::icon_rect(ItemClass::Weapon, ItemModel::ShortSword)
        );
        assert_ne!(
            art::icon_rect(ItemClass::Armor, ItemModel::RingMail),
            art::icon_rect(ItemClass::Armor, ItemModel::LeatherArmor)
        );
        assert_ne!(
            art::icon_rect(ItemClass::Tool, ItemModel::Pouch),
            art::icon_rect(ItemClass::Tool, ItemModel::Pick)
        );
    }
    #[test]
    fn pointer_regions_reject_zero_size_and_points_outside_the_frame() {
        let mut node = ComputedNode::default();
        let transform = GlobalTransform::from_translation(Vec3::new(200., 150., 0.));
        assert!(!hit(&node, &transform, Vec2::new(200., 150.)));
        node.size = Vec2::new(100., 80.);
        assert!(hit(&node, &transform, Vec2::new(200., 150.)));
        assert!(!hit(&node, &transform, Vec2::new(251., 150.)));
    }
}
