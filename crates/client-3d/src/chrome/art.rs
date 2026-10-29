//! Original, bounded UI artwork. The atlas and panel are generated once at startup.
use bevy::prelude::*;
use tor_client_3d_model::ItemModel;
use tor_protocol::ItemClass;

pub const TILE: usize = 96;
pub const ATLAS: usize = TILE * 4;
pub const ATLAS_HEIGHT: usize = TILE * 5;

pub fn icon_index(class: ItemClass, model: ItemModel) -> usize {
    match model {
        ItemModel::Mace => return 14,
        ItemModel::RingMail
        | ItemModel::ScaleMail
        | ItemModel::ChainMail
        | ItemModel::SplintMail
        | ItemModel::BandedMail
        | ItemModel::PlateMail => return 15,
        ItemModel::Pouch => return 16,
        ItemModel::Bandage => return 17,
        ItemModel::Seal => return 18,
        ItemModel::Tablet => return 19,
        ItemModel::ShortSword | ItemModel::Greatsword => return 0,
        ItemModel::LeatherArmor | ItemModel::StuddedLeather => return 1,
        ItemModel::Potion => return 2,
        ItemModel::Ration => return 3,
        ItemModel::Corpse => return 4,
        ItemModel::Pick => return 5,
        ItemModel::Amulet => return 6,
        ItemModel::Ring => return 7,
        ItemModel::Scroll => return 8,
        ItemModel::Spellbook => return 9,
        ItemModel::Wand => return 10,
        ItemModel::Coins => return 11,
        ItemModel::Gem => return 12,
        ItemModel::Stairs | ItemModel::Placeholder => {}
    }
    match class {
        ItemClass::Weapon => 0,
        ItemClass::Armor => 1,
        ItemClass::Potion => 2,
        ItemClass::Food => 3,
        ItemClass::Corpse => 4,
        ItemClass::Tool => 5,
        ItemClass::Amulet => 6,
        ItemClass::Ring => 7,
        ItemClass::Scroll => 8,
        ItemClass::Spellbook => 9,
        ItemClass::Wand => 10,
        ItemClass::Coin => 11,
        ItemClass::Gem => 12,
        ItemClass::Misc => 13,
    }
}

pub fn icon_rect(class: ItemClass, model: ItemModel) -> Rect {
    let index = icon_index(class, model);
    Rect::from_corners(
        Vec2::new((index % 4 * TILE) as f32, (index / 4 * TILE) as f32),
        Vec2::new(
            (index % 4 * TILE + TILE) as f32,
            (index / 4 * TILE + TILE) as f32,
        ),
    )
}

struct Paint {
    bytes: Vec<u8>,
    width: usize,
}
impl Paint {
    fn new(width: usize) -> Self {
        Self {
            bytes: vec![0; width * width * 4],
            width,
        }
    }
    fn pixel(&mut self, x: i32, y: i32, color: [u8; 4]) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.width as i32 {
            return;
        }
        let i = (y as usize * self.width + x as usize) * 4;
        let alpha = u32::from(color[3]);
        let old = u32::from(self.bytes[i + 3]);
        let combined = alpha + old * (255 - alpha) / 255;
        if combined == 0 {
            return;
        }
        for (channel, new) in color[..3].iter().enumerate() {
            self.bytes[i + channel] = ((u32::from(*new) * alpha
                + u32::from(self.bytes[i + channel]) * old * (255 - alpha) / 255)
                / combined) as u8;
        }
        self.bytes[i + 3] = combined as u8;
    }
    fn circle(&mut self, x: f32, y: f32, r: f32, color: [u8; 4]) {
        for py in (y - r - 1.0) as i32..=(y + r + 1.0) as i32 {
            for px in (x - r - 1.0) as i32..=(x + r + 1.0) as i32 {
                let coverage = (r + 0.5
                    - Vec2::new(px as f32 + 0.5 - x, py as f32 + 0.5 - y).length())
                .clamp(0.0, 1.0);
                let mut c = color;
                c[3] = (f32::from(c[3]) * coverage) as u8;
                self.pixel(px, py, c);
            }
        }
    }
    fn line(&mut self, a: Vec2, b: Vec2, width: f32, color: [u8; 4]) {
        let span = b - a;
        let count = (span.length() * 1.5).ceil() as usize;
        for i in 0..=count {
            let p = a + span * (i as f32 / count.max(1) as f32);
            self.circle(p.x, p.y, width * 0.5, color);
        }
    }
    fn poly(&mut self, points: &[[f32; 2]], color: [u8; 4]) {
        for y in 0..self.width {
            for x in 0..self.width {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let mut inside = false;
                for i in 0..points.len() {
                    let a = Vec2::from_array(points[i]);
                    let b = Vec2::from_array(points[(i + 1) % points.len()]);
                    if (a.y > p.y) != (b.y > p.y)
                        && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x
                    {
                        inside = !inside;
                    }
                }
                if inside {
                    self.pixel(x as i32, y as i32, color);
                }
            }
        }
    }
}

const BRASS: [u8; 4] = [181, 130, 61, 255];
const LIGHT: [u8; 4] = [237, 204, 133, 255];
const STEEL: [u8; 4] = [176, 187, 189, 255];
const SHADE: [u8; 4] = [63, 73, 79, 255];

pub fn panel_pixels() -> Vec<u8> {
    let mut paint = Paint::new(128);
    for y in 0..128usize {
        for x in 0..128usize {
            let hash = (x as u32)
                .wrapping_mul(374761393u32)
                .wrapping_add((y as u32).wrapping_mul(668265263u32))
                .wrapping_mul(1274126177);
            let noise = ((hash >> 25) & 7) as u8;
            let vein = ((x * 3 + y * 7 + (hash >> 20) as usize % 9) % 71 < 2) as u8;
            paint.pixel(
                x as i32,
                y as i32,
                [
                    20 + noise - vein * 4,
                    19 + noise - vein * 4,
                    17 + noise - vein * 4,
                    255,
                ],
            );
        }
    }
    for (edge, color) in [
        (1, BRASS),
        (3, [49, 31, 16, 255]),
        (5, LIGHT),
        (6, [71, 46, 22, 255]),
    ] {
        let a = edge as f32;
        let b = 127.0 - a;
        for (p, q) in [
            (Vec2::new(a, a), Vec2::new(b, a)),
            (Vec2::new(a, a), Vec2::new(a, b)),
            (Vec2::new(a, b), Vec2::new(b, b)),
            (Vec2::new(b, a), Vec2::new(b, b)),
        ] {
            paint.line(p, q, 1.0, color);
        }
    }
    for (x, y) in [(9.0, 9.0), (118.0, 9.0), (9.0, 118.0), (118.0, 118.0)] {
        paint.poly(
            &[[x, y - 6.0], [x + 6.0, y], [x, y + 6.0], [x - 6.0, y]],
            BRASS,
        );
        paint.poly(
            &[[x, y - 3.0], [x + 3.0, y], [x, y + 3.0], [x - 3.0, y]],
            LIGHT,
        );
    }
    paint.bytes
}

pub fn atlas_pixels() -> Vec<u8> {
    let mut atlas = vec![0; ATLAS * ATLAS_HEIGHT * 4];
    for index in 0..20 {
        let mut p = Paint::new(TILE);
        match index {
            0 => {
                p.poly(&[[69., 12.], [77., 18.], [44., 65.], [33., 56.]], SHADE);
                p.poly(&[[69., 12.], [72., 19.], [39., 61.], [33., 56.]], STEEL);
                p.line(Vec2::new(29., 53.), Vec2::new(50., 68.), 5., BRASS);
                p.line(
                    Vec2::new(38., 64.),
                    Vec2::new(26., 81.),
                    7.,
                    [100, 55, 26, 255],
                );
                p.circle(24., 83., 5., BRASS);
            }
            1 => {
                p.poly(
                    &[
                        [31., 18.],
                        [42., 24.],
                        [54., 24.],
                        [65., 18.],
                        [83., 35.],
                        [72., 47.],
                        [63., 40.],
                        [65., 80.],
                        [31., 80.],
                        [33., 40.],
                        [24., 47.],
                        [13., 35.],
                    ],
                    [111, 66, 36, 255],
                );
                p.poly(
                    &[[42., 27.], [54., 27.], [60., 73.], [37., 73.]],
                    [158, 99, 50, 255],
                );
                for y in [37., 48., 59., 70.] {
                    p.circle(48., y, 2., BRASS);
                }
                p.line(Vec2::new(32., 75.), Vec2::new(64., 75.), 4., BRASS);
            }
            2 => {
                p.circle(48., 60., 26., [75, 32, 35, 255]);
                p.circle(47., 60., 22., [173, 39, 49, 255]);
                p.poly(
                    &[[37., 23.], [59., 23.], [59., 47.], [37., 47.]],
                    [196, 171, 128, 255],
                );
                p.line(Vec2::new(36., 23.), Vec2::new(60., 23.), 6., BRASS);
                p.line(
                    Vec2::new(35., 47.),
                    Vec2::new(30., 62.),
                    4.,
                    [243, 204, 174, 255],
                );
            }
            3 => {
                p.circle(48., 52., 29., [128, 77, 34, 255]);
                p.circle(46., 47., 25., [207, 155, 82, 255]);
                for x in [34., 47., 60.] {
                    p.line(
                        Vec2::new(x, 32.),
                        Vec2::new(x - 6., 56.),
                        4.,
                        [246, 205, 135, 255],
                    );
                }
            }
            4 => {
                p.poly(
                    &[
                        [24., 65.],
                        [35., 32.],
                        [65., 29.],
                        [78., 60.],
                        [62., 77.],
                        [34., 79.],
                    ],
                    [112, 87, 66, 255],
                );
                p.line(
                    Vec2::new(31., 35.),
                    Vec2::new(70., 73.),
                    7.,
                    [209, 196, 158, 255],
                );
                p.circle(29., 32., 8., [209, 196, 158, 255]);
                p.circle(72., 76., 8., [209, 196, 158, 255]);
            }
            5 => {
                p.line(
                    Vec2::new(25., 80.),
                    Vec2::new(68., 26.),
                    8.,
                    [123, 77, 39, 255],
                );
                p.poly(
                    &[
                        [41., 23.],
                        [56., 13.],
                        [80., 25.],
                        [83., 49.],
                        [72., 35.],
                        [53., 30.],
                    ],
                    STEEL,
                );
            }
            6 | 7 => {
                p.circle(48., 50., 29., BRASS);
                p.circle(48., 50., 22., [32, 26, 20, 255]);
                if index == 6 {
                    p.poly(&[[48., 39.], [65., 58.], [48., 81.], [31., 58.]], BRASS);
                    p.poly(
                        &[[48., 47.], [57., 58.], [48., 72.], [39., 58.]],
                        [54, 157, 151, 255],
                    );
                } else {
                    p.poly(
                        &[[39., 21.], [57., 21.], [62., 33.], [48., 42.], [34., 33.]],
                        [64, 164, 173, 255],
                    );
                }
            }
            8 | 9 => {
                p.poly(
                    &[[23., 18.], [73., 18.], [73., 79.], [23., 79.]],
                    if index == 9 {
                        [112, 50, 34, 255]
                    } else {
                        [220, 194, 142, 255]
                    },
                );
                if index == 9 {
                    p.line(Vec2::new(30., 20.), Vec2::new(30., 77.), 5., BRASS);
                }
                for y in [34., 45., 56., 67.] {
                    p.line(
                        Vec2::new(37., y),
                        Vec2::new(63., y - 2.),
                        2.,
                        if index == 9 {
                            BRASS
                        } else {
                            [101, 73, 40, 255]
                        },
                    );
                }
            }
            10 => {
                p.line(
                    Vec2::new(24., 80.),
                    Vec2::new(72., 18.),
                    7.,
                    [149, 100, 54, 255],
                );
                p.circle(72., 18., 9., [158, 183, 216, 255]);
                p.circle(70., 16., 4., [237, 239, 240, 255]);
            }
            11 => {
                for (x, y) in [(31., 65.), (61., 65.), (48., 44.)] {
                    p.circle(x, y, 18., [116, 73, 21, 255]);
                    p.circle(x, y - 3., 17., [226, 172, 65, 255]);
                    p.circle(x, y - 3., 12., [168, 115, 38, 255]);
                    p.line(Vec2::new(x, y - 12.), Vec2::new(x, y + 6.), 3., LIGHT);
                }
            }
            12 => {
                p.poly(
                    &[[32., 20.], [64., 20.], [79., 45.], [48., 81.], [17., 45.]],
                    [126, 88, 174, 255],
                );
                p.poly(&[[32., 20.], [48., 44.], [17., 45.]], [220, 191, 245, 255]);
                p.poly(&[[64., 20.], [79., 45.], [48., 44.]], [171, 130, 218, 255]);
                p.poly(&[[48., 44.], [79., 45.], [48., 81.]], [83, 59, 137, 255]);
            }
            14 => {
                p.line(
                    Vec2::new(25., 83.),
                    Vec2::new(59., 37.),
                    8.,
                    [117, 73, 38, 255],
                );
                p.poly(
                    &[
                        [57., 9.],
                        [65., 18.],
                        [78., 16.],
                        [78., 29.],
                        [87., 36.],
                        [75., 43.],
                        [74., 56.],
                        [61., 51.],
                        [51., 57.],
                        [44., 44.],
                        [33., 39.],
                        [43., 28.],
                        [43., 16.],
                    ],
                    SHADE,
                );
                p.circle(61., 34., 18., STEEL);
                p.circle(56., 28., 9., [221, 224, 217, 255]);
                p.line(Vec2::new(24., 79.), Vec2::new(33., 85.), 4., BRASS);
            }
            15 => {
                p.poly(
                    &[
                        [31., 18.],
                        [42., 24.],
                        [54., 24.],
                        [65., 18.],
                        [83., 35.],
                        [72., 47.],
                        [63., 40.],
                        [65., 80.],
                        [31., 80.],
                        [33., 40.],
                        [24., 47.],
                        [13., 35.],
                    ],
                    SHADE,
                );
                p.poly(
                    &[[35., 27.], [61., 27.], [61., 75.], [35., 75.]],
                    [124, 140, 148, 255],
                );
                for y in [34., 43., 52., 61., 70.] {
                    for x in [39., 48., 57.] {
                        p.circle(x, y, 3.7, STEEL);
                        p.circle(x, y, 1.8, SHADE);
                    }
                }
                p.line(Vec2::new(32., 78.), Vec2::new(64., 78.), 4., BRASS);
            }
            16 => {
                p.circle(48., 59., 28., [99, 60, 35, 255]);
                p.poly(
                    &[[29., 29.], [67., 29.], [59., 47.], [37., 47.]],
                    [154, 104, 57, 255],
                );
                p.line(Vec2::new(32., 38.), Vec2::new(64., 38.), 5., BRASS);
                p.line(Vec2::new(45., 40.), Vec2::new(34., 65.), 3., LIGHT);
            }
            17 => {
                p.poly(
                    &[[22., 29.], [65., 20.], [79., 70.], [36., 81.]],
                    [212, 202, 170, 255],
                );
                for y in [35., 47., 59.] {
                    p.line(
                        Vec2::new(30., y),
                        Vec2::new(68., y - 8.),
                        5.,
                        [159, 151, 125, 255],
                    );
                }
            }
            18 => {
                for angle in 0..8 {
                    let d = Vec2::from_angle(angle as f32 * std::f32::consts::FRAC_PI_4);
                    p.line(
                        Vec2::splat(48.) + d * 18.,
                        Vec2::splat(48.) + d * 34.,
                        5.,
                        BRASS,
                    );
                }
                p.circle(48., 48., 22., BRASS);
                p.circle(48., 48., 15., LIGHT);
                p.circle(48., 48., 10., [174, 117, 40, 255]);
            }
            19 => {
                p.poly(
                    &[[26., 17.], [67., 17.], [75., 28.], [71., 79.], [23., 79.]],
                    SHADE,
                );
                p.poly(
                    &[[29., 21.], [63., 21.], [69., 31.], [66., 73.], [28., 73.]],
                    [143, 148, 145, 255],
                );
                for y in [35., 47., 59.] {
                    p.line(Vec2::new(36., y), Vec2::new(59., y), 3., SHADE);
                }
            }
            _ => {
                p.poly(&[[48., 17.], [78., 48.], [48., 79.], [18., 48.]], BRASS);
                p.poly(
                    &[[48., 25.], [69., 48.], [48., 70.], [27., 48.]],
                    [64, 76, 76, 255],
                );
                p.circle(48., 48., 7., LIGHT);
            }
        }
        for y in 0..TILE {
            let start = ((index / 4 * TILE + y) * ATLAS + index % 4 * TILE) * 4;
            let source = y * TILE * 4;
            atlas[start..start + TILE * 4].copy_from_slice(&p.bytes[source..source + TILE * 4]);
        }
    }
    atlas
}
