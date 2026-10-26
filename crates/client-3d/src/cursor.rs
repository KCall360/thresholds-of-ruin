//! Contextual mouse cursors, painted at start-up from simple shapes, as the
//! models are built: a grasping hand over things to pick up, a walking figure
//! over ground to travel to, crossed swords over creatures to fight.

use crate::interact::{Hover, Intent};
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    window::PrimaryWindow,
    winit::cursor::{CursorIcon, CustomCursor, CustomCursorImage},
};

const SIZE: u32 = 40;
const INK: [f32; 3] = [0.07, 0.06, 0.08];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Pointer {
    Default,
    Grasp,
    Walk,
    Fight,
}

#[derive(Resource)]
pub(crate) struct Cursors {
    grasp: Handle<Image>,
    walk: Handle<Image>,
    fight: Handle<Image>,
    shown: Pointer,
}

/// One piece of an icon: a disc or a capsule, filled and outlined.
#[derive(Clone, Copy)]
enum Shape {
    Disc(Vec2, f32),
    Capsule(Vec2, Vec2, f32),
}

impl Shape {
    /// Signed distance from a pixel centre to the shape's edge.
    fn distance(self, p: Vec2) -> f32 {
        match self {
            Shape::Disc(c, r) => p.distance(c) - r,
            Shape::Capsule(a, b, r) => {
                let ab = b - a;
                let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
                p.distance(a + ab * t) - r
            }
        }
    }
}

/// Paints shapes back to front, each with a dark outline, so overlapping
/// pieces stay readable at cursor size. Colours light toward the top left.
struct Canvas {
    pixels: Vec<[f32; 4]>,
}

impl Canvas {
    fn new() -> Self {
        Self {
            pixels: vec![[0.0; 4]; (SIZE * SIZE) as usize],
        }
    }

    fn paint(&mut self, shape: Shape, color: [f32; 3]) {
        for y in 0..SIZE {
            for x in 0..SIZE {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let d = shape.distance(p);
                let outline = (1.9 - d).clamp(0.0, 1.0);
                if outline <= 0.0 {
                    continue;
                }
                let fill = (0.5 - d).clamp(0.0, 1.0);
                let light = 1.08 - (p.x + p.y) / (SIZE as f32 * 2.0) * 0.3
                    + if d > -1.6 { -0.12 } else { 0.0 };
                let lit = color.map(|c| (c * light).min(1.0));
                let mixed: [f32; 3] = std::array::from_fn(|i| {
                    INK[i] + (lit[i] - INK[i]) * (fill / outline.max(1e-3)).min(1.0)
                });
                let pixel = &mut self.pixels[(y * SIZE + x) as usize];
                for i in 0..3 {
                    pixel[i] = pixel[i] + (mixed[i] - pixel[i]) * outline;
                }
                pixel[3] = pixel[3] + (1.0 - pixel[3]) * outline;
            }
        }
    }

    fn image(self) -> Image {
        let data = self
            .pixels
            .iter()
            .flat_map(|p| p.map(|c| (c.clamp(0.0, 1.0) * 255.0) as u8))
            .collect();
        Image::new(
            Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        )
    }
}

fn v(x: f32, y: f32) -> Vec2 {
    Vec2::new(x, y)
}

/// A hand reaching to grasp: palm, four fingers curling in at the tips, a
/// thumb opposed, and a cuff at the wrist.
fn grasp() -> Image {
    const SKIN: [f32; 3] = [0.98, 0.84, 0.66];
    const CUFF: [f32; 3] = [0.35, 0.82, 0.74];
    let mut c = Canvas::new();
    c.paint(Shape::Capsule(v(25.0, 34.0), v(30.0, 38.0), 4.5), CUFF);
    c.paint(Shape::Capsule(v(21.0, 25.0), v(25.0, 30.0), 8.0), SKIN);
    // Each finger rises from its knuckle, then its tip hooks down and in.
    for (i, (knuckle, mid, tip)) in [
        (v(14.0, 22.0), v(9.0, 14.0), v(10.0, 19.0)),
        (v(17.0, 18.0), v(13.0, 9.0), v(15.0, 14.0)),
        (v(21.0, 17.0), v(19.0, 7.0), v(21.0, 12.0)),
        (v(25.0, 18.0), v(25.0, 9.0), v(26.5, 13.5)),
    ]
    .into_iter()
    .enumerate()
    {
        let r = 2.9 - i as f32 * 0.15;
        c.paint(Shape::Capsule(knuckle, mid, r), SKIN);
        c.paint(Shape::Capsule(mid, tip, r * 0.92), SKIN);
    }
    c.paint(Shape::Capsule(v(28.0, 26.0), v(33.0, 20.0), 3.0), SKIN);
    c.paint(Shape::Capsule(v(33.0, 20.0), v(31.0, 16.0), 2.7), SKIN);
    c.image()
}

/// A figure in mid-stride, leaning into it: the far limbs darker, behind.
fn walk() -> Image {
    const BODY: [f32; 3] = [0.45, 0.9, 1.0];
    const FAR: [f32; 3] = [0.25, 0.55, 0.68];
    let mut c = Canvas::new();
    // Far arm swings forward, far leg reaches back.
    c.paint(Shape::Capsule(v(21.0, 14.0), v(26.0, 19.0), 1.9), FAR);
    c.paint(Shape::Capsule(v(26.0, 19.0), v(30.0, 17.0), 1.8), FAR);
    c.paint(Shape::Capsule(v(18.0, 25.0), v(14.0, 31.0), 2.2), FAR);
    c.paint(Shape::Capsule(v(14.0, 31.0), v(9.0, 35.0), 2.0), FAR);
    // Body leaning forward, head up.
    c.paint(Shape::Capsule(v(21.0, 14.0), v(18.0, 24.0), 3.6), BODY);
    // Near leg striding forward to a planted foot.
    c.paint(Shape::Capsule(v(18.0, 25.0), v(23.0, 31.0), 2.3), BODY);
    c.paint(Shape::Capsule(v(23.0, 31.0), v(23.0, 37.0), 2.1), BODY);
    c.paint(Shape::Capsule(v(23.0, 37.5), v(27.5, 37.5), 1.6), BODY);
    // Near arm swings back.
    c.paint(Shape::Capsule(v(20.0, 15.0), v(15.0, 20.0), 1.9), BODY);
    c.paint(Shape::Capsule(v(15.0, 20.0), v(13.0, 25.0), 1.8), BODY);
    c.paint(Shape::Disc(v(23.0, 7.0), 4.3), BODY);
    c.image()
}

/// Two swords crossed, points up, with gold guards.
fn fight() -> Image {
    const STEEL: [f32; 3] = [0.88, 0.9, 0.95];
    const GOLD: [f32; 3] = [0.98, 0.76, 0.3];
    const GRIP: [f32; 3] = [0.55, 0.32, 0.2];
    let mut c = Canvas::new();
    for flip in [false, true] {
        let m = |p: Vec2| if flip { v(SIZE as f32 - p.x, p.y) } else { p };
        c.paint(
            Shape::Capsule(m(v(29.0, 29.0)), m(v(34.0, 34.0)), 2.2),
            GRIP,
        );
        c.paint(Shape::Disc(m(v(35.5, 35.5)), 2.6), GOLD);
        c.paint(Shape::Capsule(m(v(28.0, 28.0)), m(v(6.0, 6.0)), 2.3), STEEL);
        c.paint(
            Shape::Capsule(m(v(23.0, 31.0)), m(v(31.0, 23.0)), 2.0),
            GOLD,
        );
    }
    c.image()
}

/// A blown-up copy of an icon, for inspection.
fn magnify(image: &Image, k: u32) -> Image {
    let data = image.data.clone().unwrap_or_default();
    let mut out = Vec::with_capacity(data.len() * (k * k) as usize);
    for y in 0..SIZE * k {
        for x in 0..SIZE * k {
            let i = (((y / k) * SIZE + x / k) * 4) as usize;
            out.extend_from_slice(&data[i..i + 4]);
        }
    }
    Image::new(
        Extent3d {
            width: SIZE * k,
            height: SIZE * k,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        out,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD,
    )
}

pub(crate) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    // TOR_3D_CURSORS=DIR saves the icons, for looking them over.
    if let Some(dir) = std::env::var_os("TOR_3D_CURSORS") {
        for (name, image) in [("grasp", grasp()), ("walk", walk()), ("fight", fight())] {
            if let Ok(picture) = magnify(&image, 6).try_into_dynamic() {
                let _ = picture.save(std::path::Path::new(&dir).join(format!("cursor-{name}.png")));
            }
        }
    }
    commands.insert_resource(Cursors {
        grasp: images.add(grasp()),
        walk: images.add(walk()),
        fight: images.add(fight()),
        shown: Pointer::Default,
    });
}

/// What the cursor should say about a click on the hovered cell.
pub(crate) fn pointer(intent: Option<&Intent>) -> Pointer {
    match intent {
        Some(Intent::Take(_) | Intent::Fetch(_)) => Pointer::Grasp,
        Some(Intent::Attack(..) | Intent::Charge(_)) => Pointer::Fight,
        Some(Intent::Travel(_) | Intent::Climb(_)) => Pointer::Walk,
        Some(Intent::Door(..) | Intent::ExamineTrap(_)) => Pointer::Grasp,
        None => Pointer::Default,
    }
}

pub(crate) fn update(
    mut commands: Commands,
    hover: Res<Hover>,
    mut cursors: ResMut<Cursors>,
    window: Query<Entity, With<PrimaryWindow>>,
) {
    let wanted = hover.pointer();
    if wanted == cursors.shown {
        return;
    }
    let Ok(window) = window.single() else {
        return;
    };
    cursors.shown = wanted;
    let custom = |handle: &Handle<Image>, hotspot: (u16, u16)| {
        CursorIcon::Custom(CustomCursor::Image(CustomCursorImage {
            handle: handle.clone(),
            hotspot,
            ..default()
        }))
    };
    let icon = match wanted {
        Pointer::Default => CursorIcon::default(),
        Pointer::Grasp => custom(&cursors.grasp, (18, 10)),
        Pointer::Walk => custom(&cursors.walk, (22, 38)),
        Pointer::Fight => custom(&cursors.fight, (20, 20)),
    };
    commands.entity(window).insert(icon);
}
