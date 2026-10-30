//! A live equipment portrait reuses the displayed actor's existing rig and meshes.
use crate::{chrome, equipment::Page, interact::Panel, ActorBody, Game, HostileRing};
use bevy::{
    prelude::*,
    render::{
        primitives::Aabb,
        render_asset::RenderAssetUsages,
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::RenderLayers,
    },
};
use std::collections::{HashMap, HashSet};

const LAYER: usize = 1;
const FOV: f32 = 0.65;
const MAX_SIZE: UVec2 = UVec2::new(768, 1024);

#[derive(Component)]
pub(crate) struct PortraitCamera;
/// Every single exploration-camera query must exclude the offscreen portrait.
pub(crate) type ExplorationCamera = (With<Camera3d>, Without<PortraitCamera>);
#[derive(Component)]
struct Root;
#[derive(Component)]
pub(crate) struct Picture;
#[derive(Component)]
enum Field {
    Name,
    Status,
    Hint,
}
#[derive(Component, Clone, Copy)]
pub(crate) enum Control {
    Left,
    Right,
    Front,
}

#[derive(Resource)]
pub(crate) struct Portrait {
    pub(crate) image: Handle<Image>,
    pub(crate) owner: Option<Entity>,
    pub(crate) active: bool,
    pub(crate) size: UVec2,
    pub(crate) turn: f32,
    layers: HashMap<Entity, Option<RenderLayers>>,
}
impl Portrait {
    fn new(image: Handle<Image>) -> Self {
        Self {
            image,
            owner: None,
            active: false,
            size: UVec2::new(512, 768),
            turn: 0.35,
            layers: default(),
        }
    }
    pub(crate) fn parts(&self) -> usize {
        self.layers.len()
    }
}

fn wanted(game: &Game, panel: &Panel) -> bool {
    match game.screen {
        Some(crate::screen::Screen::Advancement) => {
            game.view.as_ref().is_some_and(|view| view.has_control)
        }
        None => panel.open && panel.page == Page::Equipment && crate::inventory::grid(game),
        _ => false,
    }
}

pub(crate) fn setup(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    lights: Query<(Entity, Option<&RenderLayers>), With<DirectionalLight>>,
) {
    let mut image = Image::new_fill(
        Extent3d {
            width: 512,
            height: 768,
            ..default()
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING
        | TextureUsages::COPY_DST
        | TextureUsages::COPY_SRC
        | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    // Sharing the existing lights preserves the main view's illumination.
    for (entity, layers) in &lights {
        commands
            .entity(entity)
            .insert(layers.cloned().unwrap_or_default().with(LAYER));
    }
    commands.spawn((
        PortraitCamera,
        Camera3d::default(),
        crate::graphics::camera_antialiasing(),
        Camera {
            order: -1,
            is_active: false,
            hdr: true,
            target: image.clone().into(),
            clear_color: Color::srgb(0.018, 0.021, 0.025).into(),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: FOV,
            aspect_ratio: 2. / 3.,
            ..default()
        }),
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        RenderLayers::layer(LAYER),
        Transform::default(),
    ));
    commands
        .spawn((
            Root,
            chrome::BlockWorld,
            Interaction::default(),
            chrome::frame(),
            GlobalZIndex(4),
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.),
                top: Val::Px(16.),
                bottom: Val::Px(184.),
                width: Val::Percent(40.),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(14.)),
                border: UiRect::all(Val::Px(1.)),
                row_gap: Val::Px(8.),
                overflow: Overflow::clip(),
                ..default()
            },
        ))
        .with_children(|ui| {
            ui.spawn((
                Field::Name,
                Text::new("CHARACTER"),
                chrome::text(27., chrome::INK),
                Node {
                    flex_shrink: 0.,
                    ..default()
                },
            ));
            ui.spawn((
                Picture,
                ImageNode::new(image.clone()),
                Node {
                    width: Val::Percent(100.),
                    flex_grow: 1.,
                    flex_basis: Val::Px(0.),
                    flex_shrink: 1.,
                    min_height: Val::Px(0.),
                    ..default()
                },
            ));
            ui.spawn((
                Field::Status,
                Text::new(""),
                chrome::text(17., chrome::MUTED),
                Node {
                    flex_shrink: 0.,
                    ..default()
                },
            ));
            ui.spawn(Node {
                justify_content: JustifyContent::Center,
                column_gap: Val::Px(6.),
                flex_wrap: FlexWrap::Wrap,
                flex_shrink: 0.,
                ..default()
            })
            .with_children(|buttons| {
                for (control, label) in [
                    (Control::Left, "Turn left"),
                    (Control::Front, "Front"),
                    (Control::Right, "Turn right"),
                ] {
                    buttons
                        .spawn((
                            Button,
                            control,
                            BackgroundColor(chrome::BUTTON),
                            BorderColor(chrome::GOLD),
                            Node {
                                padding: UiRect::axes(Val::Px(10.), Val::Px(6.)),
                                border: UiRect::all(Val::Px(1.)),
                                ..default()
                            },
                        ))
                        .with_child((Text::new(label), chrome::text(17., chrome::INK)));
                }
            });
            ui.spawn((
                Field::Hint,
                Text::new("[ / ]: turn the preview"),
                chrome::text(15., chrome::MUTED),
                Node {
                    flex_shrink: 0.,
                    ..default()
                },
            ));
        });
    commands.insert_resource(Portrait::new(image));
}

pub(crate) fn controls(
    game: Res<Game>,
    mut panel: ResMut<Panel>,
    keys: Res<ButtonInput<KeyCode>>,
    mut portrait: ResMut<Portrait>,
    mut buttons: Query<(&Interaction, &Control, &mut BackgroundColor), Changed<Interaction>>,
) {
    if !wanted(&game, &panel) {
        return;
    }
    let mut action = None;
    let character = game.screen == Some(crate::screen::Screen::Advancement);
    if character && game.typed.is_empty() {
        if keys.just_pressed(KeyCode::Home) {
            action = Some(Control::Left);
        }
        if keys.just_pressed(KeyCode::End) {
            action = Some(Control::Right);
        }
    }
    if !character && !panel.search_focused && !panel.keyboard_captured {
        if keys.just_pressed(KeyCode::BracketLeft) {
            action = Some(Control::Left)
        }
        if keys.just_pressed(KeyCode::BracketRight) {
            action = Some(Control::Right)
        }
    }
    for (interaction, control, mut color) in &mut buttons {
        color.0 = if *interaction == Interaction::None {
            chrome::BUTTON
        } else {
            chrome::HOT
        };
        if *interaction == Interaction::Pressed {
            action = Some(*control);
            if !character {
                panel.search_focused = false;
                panel.dirty = true;
            }
        }
    }
    match action {
        Some(Control::Left) => portrait.turn -= std::f32::consts::FRAC_PI_4,
        Some(Control::Right) => portrait.turn += std::f32::consts::FRAC_PI_4,
        Some(Control::Front) => portrait.turn = 0.35,
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_upgrade_activates_the_shared_portrait_and_preserves_command_keys() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut view = crate::test_support::view(0);
        view.has_control = true;
        let game = Game {
            view: Some(view),
            screen: Some(crate::screen::Screen::Advancement),
            ..default()
        };
        let panel = Panel::default();
        assert!(
            wanted(&game, &panel),
            "Character must reuse the portrait even when Equipment is closed"
        );
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(game)
            .insert_resource(panel)
            .insert_resource(Portrait::new(default()))
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, controls);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Home);
        app.update();
        let turn = app.world().resource::<Portrait>().turn;
        assert!((turn - 0.35 + std::f32::consts::FRAC_PI_4).abs() < 1e-5);
        app.world_mut().resource_mut::<Game>().typed = "purchase [".into();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.clear();
            keys.press(KeyCode::End);
            keys.press(KeyCode::BracketRight);
        }
        app.update();
        assert_eq!(
            app.world().resource::<Portrait>().turn,
            turn,
            "a command draft keeps all editing keys"
        );
        assert_eq!(app.world().resource::<Game>().typed, "purchase [");
        app.world_mut().resource_mut::<Panel>().search_focused = true;
        app.world_mut().spawn((
            Control::Front,
            Interaction::Pressed,
            BackgroundColor(chrome::BUTTON),
        ));
        app.update();
        assert_eq!(app.world().resource::<Portrait>().turn, 0.35);
        assert!(
            app.world().resource::<Panel>().search_focused,
            "character controls preserve hidden inventory focus"
        );
        assert_eq!(app.world().resource::<Game>().typed, "purchase [");
        app.world_mut().resource_mut::<Game>().screen = None;
        assert!(!wanted(
            app.world().resource::<Game>(),
            app.world().resource::<Panel>()
        ));
        assert!(receiver.try_recv().is_err(), "preview rotation is local");
    }

    #[test]
    fn texture_resolution_is_bounded_and_keeps_the_widget_aspect() {
        assert_eq!(texture_size(Vec2::new(512., 768.)), UVec2::new(512, 768));
        assert_eq!(texture_size(Vec2::new(1920., 1080.)), UVec2::new(768, 432));
        assert_eq!(texture_size(Vec2::new(1024., 2048.)), UVec2::new(512, 1024));
        assert_eq!(texture_size(Vec2::ZERO), UVec2::ONE);
        assert_eq!(texture_size(Vec2::splat(f32::NAN)), UVec2::ONE);
    }

    #[test]
    fn portrait_pose_fits_wide_and_tall_bounds_without_modifying_the_actor() {
        let base = Vec3::new(123., 10., -91.);
        for size in [
            Vec3::new(1., 2., 1.),
            Vec3::new(5., 1., 3.),
            Vec3::new(2., 5., 2.),
        ] {
            let low = base - Vec3::new(size.x * 0.5, 0., size.z * 0.5);
            let high = low + size;
            for yaw in [0.35, 3.] {
                let camera = pose(base, size.y, yaw, 0.6, Some((low, high)));
                let matrix =
                    Mat4::perspective_rh(FOV, 0.6, 0.1, 1000.) * camera.compute_matrix().inverse();
                for x in [low.x, high.x] {
                    for y in [low.y, high.y] {
                        for z in [low.z, high.z] {
                            let clip = matrix * Vec3::new(x, y, z).extend(1.);
                            assert!(clip.w > 0.);
                            assert!(
                                clip.x.abs() / clip.w < 0.98 && clip.y.abs() / clip.w < 0.98,
                                "{size:?} at {yaw}"
                            );
                        }
                    }
                }
                assert_eq!(base, Vec3::new(123., 10., -91.));
            }
        }
    }

    #[derive(Resource)]
    struct Selection(Option<Entity>);
    fn membership(
        mut commands: Commands,
        cast: Cast,
        mut portrait: ResMut<Portrait>,
        selection: Res<Selection>,
    ) {
        if portrait.owner != selection.0 {
            cast.restore(&mut commands, &mut portrait);
        }
        portrait.owner = selection.0;
        if let Some(owner) = selection.0 {
            cast.include(&mut commands, &mut portrait, owner);
        } else {
            cast.restore(&mut commands, &mut portrait);
        }
    }

    #[test]
    fn live_mesh_membership_reuses_handles_and_restores_layers_on_replace_detach_and_close() {
        let mut app = App::new();
        let owner = app.world_mut().spawn_empty().id();
        let outside = app.world_mut().spawn_empty().id();
        let mesh: Handle<Mesh> = bevy::asset::weak_handle!("68a6b921-7843-4b25-a0df-5beaaeb1b399");
        let first = app
            .world_mut()
            .spawn((Mesh3d(mesh.clone()), ChildOf(owner)))
            .id();
        let custom = app
            .world_mut()
            .spawn((Mesh3d(mesh.clone()), RenderLayers::layer(7), ChildOf(owner)))
            .id();
        let ring = app
            .world_mut()
            .spawn((Mesh3d(mesh.clone()), HostileRing, ChildOf(owner)))
            .id();
        app.insert_resource(Portrait::new(default()))
            .insert_resource(Selection(Some(owner)))
            .add_systems(Update, membership);
        app.update();
        assert_eq!(app.world().resource::<Portrait>().parts(), 2);
        assert_eq!(
            app.world().get::<RenderLayers>(first),
            Some(&RenderLayers::layer(0).with(1))
        );
        assert_eq!(
            app.world().get::<RenderLayers>(custom),
            Some(&RenderLayers::layer(7).with(1))
        );
        assert!(app.world().get::<RenderLayers>(ring).is_none());
        assert_eq!(
            app.world().get::<Mesh3d>(first).unwrap().0,
            mesh,
            "the portrait shares the original mesh handle"
        );
        let ready = app
            .world_mut()
            .spawn((Mesh3d(mesh.clone()), ChildOf(owner)))
            .id();
        app.world_mut().entity_mut(first).insert(ChildOf(outside));
        app.update();
        assert!(
            app.world().get::<RenderLayers>(first).is_none(),
            "detached geometry cannot leak into the portrait"
        );
        assert!(
            app.world()
                .get::<RenderLayers>(ready)
                .unwrap()
                .intersects(&RenderLayers::layer(1)),
            "ready meshes join without another view"
        );
        app.world_mut().entity_mut(custom).despawn();
        app.world_mut().resource_mut::<Selection>().0 = None;
        app.update();
        assert_eq!(app.world().resource::<Portrait>().parts(), 0);
        assert!(app.world().get::<RenderLayers>(ready).is_none());
        let second = app
            .world_mut()
            .spawn((
                Mesh3d(mesh.clone()),
                RenderLayers::layer(4),
                ChildOf(outside),
            ))
            .id();
        app.world_mut().resource_mut::<Selection>().0 = Some(outside);
        app.update();
        app.world_mut().resource_mut::<Selection>().0 = Some(owner);
        app.update();
        assert_eq!(
            app.world().get::<RenderLayers>(second),
            Some(&RenderLayers::layer(4))
        );
        assert!(app.world().get::<RenderLayers>(first).is_none());
        assert_eq!(
            app.world_mut().query::<&Mesh3d>().iter(app.world()).count(),
            4,
            "membership creates no extra mesh entities"
        );
    }

    #[test]
    fn camera_fitting_ignores_deformation_envelopes_but_still_renders_their_meshes() {
        #[derive(Resource, Default)]
        struct Measured(Option<(Vec3, Vec3)>);
        fn measure(
            mut commands: Commands,
            cast: Cast,
            mut portrait: ResMut<Portrait>,
            selection: Res<Selection>,
            mut measured: ResMut<Measured>,
        ) {
            measured.0 = cast.include(&mut commands, &mut portrait, selection.0.unwrap());
        }
        let mut app = App::new();
        let owner = app.world_mut().spawn_empty().id();
        app.world_mut().spawn((
            Mesh3d::default(),
            GlobalTransform::from_translation(Vec3::new(3., 4., 5.)),
            Aabb::from_min_max(Vec3::new(-1., 0., -1.), Vec3::new(1., 2., 1.)),
            ChildOf(owner),
        ));
        let drape = app
            .world_mut()
            .spawn((
                Mesh3d::default(),
                crate::picking::LooseBounds,
                Aabb::from_min_max(Vec3::splat(-100.), Vec3::splat(100.)),
                ChildOf(owner),
            ))
            .id();
        app.insert_resource(Portrait::new(default()))
            .insert_resource(Selection(Some(owner)))
            .init_resource::<Measured>()
            .add_systems(Update, measure);
        app.update();
        assert_eq!(
            app.world().resource::<Measured>().0,
            Some((Vec3::new(2., 4., 4.), Vec3::new(4., 6., 6.)))
        );
        assert_eq!(app.world().resource::<Portrait>().parts(), 2);
        assert_eq!(
            app.world().get::<RenderLayers>(drape),
            Some(&RenderLayers::layer(0).with(1))
        );
    }

    #[test]
    fn portrait_turning_is_local_and_search_keeps_the_bracket_keys() {
        let (net, mut receiver) = crate::net::test_channel();
        let mut panel = Panel::default();
        panel.toggle_page(Page::Equipment);
        let mut app = App::new();
        app.insert_resource(net)
            .insert_resource(Game {
                view: Some(crate::test_support::view(1)),
                ..default()
            })
            .insert_resource(panel)
            .insert_resource(Portrait::new(default()))
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, controls);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::BracketRight);
        app.update();
        let turn = app.world().resource::<Portrait>().turn;
        assert!((turn - 0.35 - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
        app.world_mut().resource_mut::<Panel>().search_focused = true;
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::BracketRight);
            keys.clear();
            keys.press(KeyCode::BracketRight);
        }
        app.update();
        assert_eq!(app.world().resource::<Portrait>().turn, turn);
        app.world_mut().spawn((
            Control::Front,
            Interaction::Pressed,
            BackgroundColor(chrome::BUTTON),
        ));
        app.update();
        assert_eq!(app.world().resource::<Portrait>().turn, 0.35);
        assert!(!app.world().resource::<Panel>().search_focused);
        assert!(receiver.try_recv().is_err());
    }

    #[test]
    fn setup_allocates_one_inactive_target_and_preserves_shared_light_intensity() {
        let mut app = App::new();
        app.init_resource::<Assets<Image>>()
            .add_systems(Startup, setup);
        let sun = app
            .world_mut()
            .spawn(DirectionalLight {
                illuminance: 1234.,
                ..default()
            })
            .id();
        app.update();
        for _ in 0..8 {
            app.update();
        }
        assert_eq!(app.world().resource::<Assets<Image>>().len(), 1);
        let (camera, layers) = app
            .world_mut()
            .query_filtered::<(&Camera, &RenderLayers), With<PortraitCamera>>()
            .single(app.world())
            .unwrap();
        assert!(!camera.is_active);
        assert_eq!(layers, &RenderLayers::layer(1));
        assert_eq!(
            app.world()
                .get::<DirectionalLight>(sun)
                .unwrap()
                .illuminance,
            1234.
        );
        assert_eq!(
            app.world().get::<RenderLayers>(sun),
            Some(&RenderLayers::layer(0).with(1))
        );
    }
}

/// Uniform downsampling retains the widget's aspect and bounds memory use.
fn texture_size(size: Vec2) -> UVec2 {
    if !size.is_finite() || size.min_element() <= 0. {
        return UVec2::ONE;
    }
    let scale = (MAX_SIZE.as_vec2() / size).min_element().min(1.);
    (size * scale)
        .round()
        .as_uvec2()
        .max(UVec2::ONE)
        .min(MAX_SIZE)
}

fn pose(base: Vec3, height: f32, yaw: f32, aspect: f32, bounds: Option<(Vec3, Vec3)>) -> Transform {
    let height = height.max(0.1);
    let (center, half) = bounds
        .filter(|(low, high)| {
            low.is_finite() && high.is_finite() && (*high - *low).min_element() >= 0.
        })
        .map(|(low, high)| ((low + high) * 0.5, (high - low) * 0.5))
        .unwrap_or((
            base + Vec3::Y * height * 0.5,
            Vec3::new(height * 0.35, height * 0.5, height * 0.35),
        ));
    let radius = Vec2::new(half.x, half.z).length().max(height * 0.15);
    let distance = (half.y / (FOV * 0.5).tan()).max(radius / ((FOV * 0.5).tan() * aspect.max(0.1)))
        * 1.15
        + radius;
    let forward = Vec3::new(yaw.sin(), 0., yaw.cos());
    Transform::from_translation(center + forward * distance + Vec3::Y * (height * 0.08))
        .looking_at(center, Vec3::Y)
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Cast<'w, 's> {
    bodies: Query<
        'w,
        's,
        (
            &'static ActorBody,
            &'static crate::creatures::Anim,
            &'static Transform,
        ),
        Without<PortraitCamera>,
    >,
    children: Query<'w, 's, &'static Children>,
    parts: Query<'w, 's, Option<&'static RenderLayers>, PortraitMeshes>,
    bounds: Query<'w, 's, (&'static GlobalTransform, &'static Aabb), ShapedBounds>,
    pending: Query<'w, 's, (), With<crate::creatures::loading::PendingModel>>,
}
type PortraitMeshes = (With<Mesh3d>, Without<HostileRing>);
type ShapedBounds = (
    With<Mesh3d>,
    Without<HostileRing>,
    Without<crate::picking::LooseBounds>,
);
type PortraitView = (With<PortraitCamera>, Without<ActorBody>);

impl Cast<'_, '_> {
    fn restore(&self, commands: &mut Commands, portrait: &mut Portrait) {
        for (entity, layers) in portrait.layers.drain() {
            if self.parts.contains(entity) {
                if let Some(layers) = layers {
                    commands.entity(entity).try_insert(layers);
                } else {
                    commands.entity(entity).try_remove::<RenderLayers>();
                }
            }
        }
    }
    fn include(
        &self,
        commands: &mut Commands,
        portrait: &mut Portrait,
        owner: Entity,
    ) -> Option<(Vec3, Vec3)> {
        let mut current = HashSet::new();
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for entity in std::iter::once(owner).chain(self.children.iter_descendants(owner)) {
            let Ok(layers) = self.parts.get(entity) else {
                continue;
            };
            current.insert(entity);
            let original = portrait
                .layers
                .entry(entity)
                .or_insert_with(|| layers.cloned());
            let desired = original.clone().unwrap_or_default().with(LAYER);
            if layers != Some(&desired) {
                commands.entity(entity).try_insert(desired);
            }
            // Drapes and hair deliberately carry broad deformation-culling
            // envelopes. Render them, but fit to shaped geometry, not envelopes.
            if let Ok((transform, aabb)) = self.bounds.get(entity) {
                for x in [-1., 1.] {
                    for y in [-1., 1.] {
                        for z in [-1., 1.] {
                            let point = transform.affine().transform_point3(
                                Vec3::from(aabb.center)
                                    + Vec3::from(aabb.half_extents) * Vec3::new(x, y, z),
                            );
                            low = low.min(point);
                            high = high.max(point);
                        }
                    }
                }
            }
        }
        let stale: Vec<_> = portrait
            .layers
            .keys()
            .filter(|entity| !current.contains(entity))
            .copied()
            .collect();
        for entity in stale {
            let original = portrait.layers.remove(&entity).expect("tracked part");
            if self.parts.contains(entity) {
                if let Some(layers) = original {
                    commands.entity(entity).try_insert(layers);
                } else {
                    commands.entity(entity).try_remove::<RenderLayers>();
                }
            }
        }
        (low.is_finite() && high.is_finite()).then_some((low, high))
    }
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct Pane<'w, 's> {
    roots: Query<'w, 's, (&'static mut Node, &'static mut Visibility), With<Root>>,
    pictures: Query<'w, 's, &'static ComputedNode, With<Picture>>,
    fields: Query<'w, 's, (&'static Field, &'static mut Text)>,
    windows: Query<'w, 's, &'static Window, With<bevy::window::PrimaryWindow>>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn update(
    mut commands: Commands,
    game: Res<Game>,
    panel: Res<Panel>,
    time: Res<Time>,
    mut portrait: ResMut<Portrait>,
    mut images: ResMut<Assets<Image>>,
    cast: Cast,
    mut pane: Pane,
    mut cameras: Query<(&mut Camera, &mut Transform, &mut Projection), PortraitView>,
    framing: crate::inventory::Framing,
) {
    let available = pane.windows.single().map_or(0., |window| {
        window.width() * (1. - framing.coverage(&game)) - 32.
    });
    let show = wanted(&game, &panel) && available >= 180.;
    if let Ok((mut node, mut visibility)) = pane.roots.single_mut() {
        if show && node.width != Val::Px(available) {
            node.width = Val::Px(available);
        }
        visibility.set_if_neq(if show {
            Visibility::Visible
        } else {
            Visibility::Hidden
        });
    }
    let actor = game
        .view
        .as_ref()
        .and_then(|view| view.actors.iter().find(|actor| actor.is_self));
    let owner = show
        .then(|| actor.and_then(|actor| game.actors.get(&actor.id).copied()))
        .flatten()
        .filter(|entity| cast.bodies.contains(*entity));
    let changed = portrait.owner != owner;
    if changed || !show {
        cast.restore(&mut commands, &mut portrait);
    }
    if changed {
        portrait.turn = 0.35;
    }
    portrait.owner = owner;
    let size = pane
        .pictures
        .single()
        .map_or(Vec2::ZERO, ComputedNode::size);
    portrait.active = owner.is_some() && size.min_element() > 0.;
    if portrait.active {
        let size = texture_size(size);
        if size != portrait.size {
            if let Some(image) = images.get_mut(&portrait.image) {
                image.resize(Extent3d {
                    width: size.x,
                    height: size.y,
                    ..default()
                });
            }
            portrait.size = size;
        }
    }
    if let Ok((mut camera, mut transform, mut projection)) = cameras.single_mut() {
        if camera.is_active != portrait.active {
            camera.is_active = portrait.active;
        }
        if let Some(owner) = owner {
            let (body, anim, root) = cast.bodies.get(owner).expect("checked live owner");
            let bounds = cast
                .include(&mut commands, &mut portrait, owner)
                .filter(|(low, high)| {
                    ((*low + *high) * 0.5 - root.translation).length() < body.height.max(1.) * 8.
                });
            let aspect = portrait.size.x as f32 / portrait.size.y as f32;
            if let Projection::Perspective(p) = projection.as_mut() {
                p.aspect_ratio = aspect;
            }
            let target = pose(
                root.translation,
                body.height,
                anim.yaw + portrait.turn,
                aspect,
                bounds,
            );
            if changed {
                *transform = target;
            } else {
                let smooth = 1. - (-time.delta_secs() * 12.).exp();
                transform.translation = transform.translation.lerp(target.translation, smooth);
                transform.rotation = transform.rotation.slerp(target.rotation, smooth);
            }
        }
    }
    for (field, mut text) in &mut pane.fields {
        let value = match field {
            Field::Name => actor
                .filter(|actor| !actor.name.trim().is_empty())
                .map_or("CHARACTER", |actor| actor.name.as_str()),
            Field::Hint => {
                if game.screen == Some(crate::screen::Screen::Advancement) {
                    "Home / End: turn the preview"
                } else {
                    "[ / ]: turn the preview"
                }
            }
            Field::Status => {
                if owner.is_some_and(|owner| cast.pending.contains(owner)) {
                    "Preparing appearance..."
                } else if owner.is_some() {
                    "Live character appearance"
                } else {
                    "Appearance not available."
                }
            }
        };
        if text.0 != value {
            text.0 = value.into();
        }
    }
}
