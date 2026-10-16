//! The cave motif: seamless sculpted rock built from the known solid cells,
//! with crystals, stalagmites and rubble grown from cell identities.
//!
//! Every exposed cell face is subdivided and displaced by a noise field
//! sampled in world space. Neighbouring faces share lattice points, so they
//! displace identically and the rock stays watertight across cells and
//! chunks. Meshes are flat shaded for a faceted, hand-cut look; vertex colour
//! carries ambient occlusion and the dimming of remembered cells.
use bevy::{
    asset::weak_handle,
    pbr::{ExtendedMaterial, MaterialExtension},
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
        render_resource::{AsBindGroup, ShaderRef, ShaderType},
    },
};
use std::collections::{HashMap, HashSet};

pub const SHADER: Handle<Shader> = weak_handle!("6b0f3c1e-5d7a-4c2b-9f4e-2a8d1c7e9b30");
pub const CHUNK: i32 = 8;
/// Face subdivisions per cell edge.
const N: i32 = 3;

pub type CaveMaterial = ExtendedMaterial<StandardMaterial, CaveExtension>;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct CaveExtension {
    #[uniform(100)]
    pub params: CaveParams,
}

/// Shader inputs: the cutaway and the motif's palette.
#[allow(dead_code)] // ShaderType derives unused size checks.
#[derive(ShaderType, Reflect, Debug, Clone)]
pub struct CaveParams {
    /// xyz: the player's chest; w: the floor surface under the player.
    pub player: Vec4,
    /// xyz: the camera; w: cutaway radius.
    pub camera: Vec4,
    pub wall_a: Vec4,
    pub wall_b: Vec4,
    pub floor_a: Vec4,
    pub floor_b: Vec4,
    /// rgb: moss; w: how brightly it glows.
    pub moss: Vec4,
    /// rgb: mineral flecks.
    pub accent: Vec4,
    /// x: the even light of a lit room on the rock the character sees.
    pub light: Vec4,
}

impl Default for CaveParams {
    fn default() -> Self {
        Self {
            player: Vec4::new(0.0, 1.0, 0.0, -0.5),
            camera: Vec4::new(0.0, 10.0, 8.0, 1.5),
            wall_a: Vec4::new(0.20, 0.19, 0.26, 1.0),
            wall_b: Vec4::new(0.34, 0.30, 0.36, 1.0),
            floor_a: Vec4::new(0.30, 0.23, 0.17, 1.0),
            floor_b: Vec4::new(0.42, 0.34, 0.25, 1.0),
            moss: Vec4::new(0.16, 0.34, 0.27, 0.22),
            accent: Vec4::new(0.45, 0.95, 0.9, 1.0),
            light: Vec4::new(0.42, 0.0, 0.0, 0.0),
        }
    }
}

impl MaterialExtension for CaveExtension {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(SHADER)
    }
}

/// One known solid cell, in Bevy space (y up).
#[derive(Clone, Copy, PartialEq)]
pub struct Solid {
    /// Visible now, or only remembered.
    pub seen: bool,
    /// Stable identity, so decoration is the same every visit.
    pub key: u64,
}

/// Which crystal colour a cluster grows in.
#[derive(Clone, Copy)]
pub enum Glow {
    Teal,
    Violet,
}

impl Glow {
    pub fn light(self) -> Color {
        match self {
            Glow::Teal => Color::srgb(0.3, 1.0, 0.85),
            Glow::Violet => Color::srgb(0.7, 0.4, 1.0),
        }
    }
}

/// A chunk's built geometry.
pub struct ChunkMeshes {
    pub rock: Option<Mesh>,
    pub crystals: [Option<Mesh>; 2],
    /// Light sources at crystal clusters.
    pub lights: Vec<(Vec3, Glow)>,
}

pub fn chunk_of(cell: IVec3) -> IVec3 {
    cell.div_euclid(IVec3::splat(CHUNK))
}

/// The cave as the window currently knows it.
#[derive(Default)]
pub struct CaveWorld {
    pub solids: HashMap<IVec3, Solid>,
    /// Known open cells: decoration only grows where one stands above rock.
    pub open: HashSet<IVec3>,
    /// Open cells holding stairs, doors or items, kept clear of decoration.
    pub blocked: HashSet<IVec3>,
    displacement: HashMap<IVec3, Vec3>,
}

const DIRS: [IVec3; 6] = [
    IVec3::X,
    IVec3::NEG_X,
    IVec3::Y,
    IVec3::NEG_Y,
    IVec3::Z,
    IVec3::NEG_Z,
];
const SIDES: [IVec3; 4] = [IVec3::X, IVec3::NEG_X, IVec3::Z, IVec3::NEG_Z];

impl CaveWorld {
    /// Replace the known cave, returning the chunks whose meshes changed.
    pub fn update(
        &mut self,
        solids: HashMap<IVec3, Solid>,
        open: HashSet<IVec3>,
        blocked: HashSet<IVec3>,
    ) -> HashSet<IVec3> {
        let mut dirty = HashSet::new();
        let mut touch = |cell: IVec3| {
            // Faces, occlusion and decoration all look one cell around.
            for dx in -1..=1 {
                for dy in -1..=1 {
                    for dz in -1..=1 {
                        dirty.insert(chunk_of(cell + IVec3::new(dx, dy, dz)));
                    }
                }
            }
        };
        for (cell, solid) in &solids {
            if self.solids.get(cell) != Some(solid) {
                touch(*cell);
            }
        }
        for cell in self.solids.keys() {
            if !solids.contains_key(cell) {
                touch(*cell);
            }
        }
        for cell in open.symmetric_difference(&self.open) {
            touch(*cell);
        }
        for cell in blocked.symmetric_difference(&self.blocked) {
            touch(*cell);
        }
        self.solids = solids;
        self.open = open;
        self.blocked = blocked;
        dirty
    }

    fn solid(&self, cell: IVec3) -> bool {
        self.solids.contains_key(&cell)
    }

    /// Displacement at a lattice point, identical for every face sharing it.
    fn displaced(&mut self, lattice: IVec3) -> Vec3 {
        let p = lattice.as_vec3() / (2.0 * N as f32);
        let d = *self
            .displacement
            .entry(lattice)
            .or_insert_with(|| displacement(p));
        p + d
    }

    pub fn build(&mut self, chunk: IVec3) -> ChunkMeshes {

        let cells: Vec<(IVec3, Solid)> = self
            .solids
            .iter()
            .filter(|(c, _)| chunk_of(**c) == chunk)
            .map(|(c, s)| (*c, *s))
            .collect();
        let mut rock = Builder::default();
        let mut crystals = [Builder::default(), Builder::default()];
        let mut lights = Vec::new();
        for (cell, solid) in cells {
            let dim = if solid.seen {
                Vec3::ONE
            } else {
                Vec3::new(0.30, 0.32, 0.40)
            };
            for d in DIRS {
                if self.solid(cell + d) {
                    continue;
                }
                self.face(&mut rock, cell, d, dim);
            }
            let above = cell + IVec3::Y;
            if !self.solid(above) && self.open.contains(&above) && !self.blocked.contains(&above) {
                self.decorate(&mut rock, &mut crystals, &mut lights, cell, solid, dim);
            }
        }
        ChunkMeshes {
            rock: rock.finish(),
            crystals: crystals.map(Builder::finish),
            lights,
        }
    }

    fn face(&mut self, out: &mut Builder, cell: IVec3, d: IVec3, dim: Vec3) {
        let u = if d.y != 0 { IVec3::X } else { IVec3::Y };
        let v = d.cross(u);
        // Lattice units are 1/(2N) of a cell, so cell edges land on integers.
        let s = 2 * N;
        let center = cell * s + d * N;
        let mut grid = [[Vec3::ZERO; (N + 1) as usize]; (N + 1) as usize];
        let mut shade = [[1.0f32; (N + 1) as usize]; (N + 1) as usize];
        let front = cell + d;
        for i in 0..=N {
            for j in 0..=N {
                let lattice = center + u * (2 * i - N) + v * (2 * j - N);
                grid[i as usize][j as usize] = self.displaced(lattice);
                // Corner occlusion from the cells beside the one in front.
                let lu = (2 * i - N) as f32 / N as f32;
                let lv = (2 * j - N) as f32 / N as f32;
                let su = if lu > 0.0 { u } else { -u };
                let sv = if lv > 0.0 { v } else { -v };
                let (wu, wv) = (lu.abs().powi(2), lv.abs().powi(2));
                let occ = f32::from(u8::from(self.solid(front + su))) * wu
                    + f32::from(u8::from(self.solid(front + sv))) * wv
                    + f32::from(u8::from(self.solid(front + su + sv))) * wu * wv * 0.6;
                shade[i as usize][j as usize] = (1.0 - 0.42 * occ).max(0.35);
            }
        }
        for i in 0..N as usize {
            for j in 0..N as usize {
                let (a, b, c, e) = (grid[i][j], grid[i + 1][j], grid[i + 1][j + 1], grid[i][j + 1]);
                let (sa, sb, sc, se) = (shade[i][j], shade[i + 1][j], shade[i + 1][j + 1], shade[i][j + 1]);
                out.tri_shaded([a, b, c], [sa, sb, sc], dim);
                out.tri_shaded([a, c, e], [sa, sc, se], dim);
            }
        }
    }

    /// Floor dressing: crystal clusters and stalagmites against walls,
    /// rubble anywhere. Chosen by the cell's identity, never by chance.
    fn decorate(
        &mut self,
        rock: &mut Builder,
        crystals: &mut [Builder; 2],
        lights: &mut Vec<(Vec3, Glow)>,
        cell: IVec3,
        solid: Solid,
        dim: Vec3,
    ) {
        let r = |i: u32| rand(solid.key, i);
        let top = cell.as_vec3() + Vec3::Y * 0.5;
        let surface = |p: Vec3| p + displacement(p);
        let walls: Vec<IVec3> = SIDES
            .into_iter()
            .filter(|s| self.solid(cell + IVec3::Y + *s))
            .collect();
        let roll = r(0);
        if let Some(&side) = walls.get((r(1) * walls.len() as f32) as usize) {
            let toward = side.as_vec3();
            let across = Vec3::new(toward.z, 0.0, -toward.x);
            if roll < 0.14 {
                let glow = if r(2) < 0.6 { Glow::Teal } else { Glow::Violet };
                let base = surface(top + toward * 0.33 + across * (r(3) - 0.5) * 0.5);
                let count = 3 + (r(4) * 4.0) as u32;
                for k in 0..count {
                    let spread = across * (rand(solid.key, 10 + k) - 0.5) * 0.36
                        + toward * rand(solid.key, 20 + k) * 0.08;
                    let lean = (Vec3::Y * 1.6 - toward * (0.4 + rand(solid.key, 30 + k))
                        + across * (rand(solid.key, 40 + k) - 0.5) * 0.9)
                        .normalize();
                    let length = 0.3 + rand(solid.key, 50 + k) * 0.6 * if k == 0 { 1.4 } else { 1.0 };
                    let radius = 0.045 + rand(solid.key, 60 + k) * 0.05;
                    crystal(&mut crystals[glow as usize], base + spread - Vec3::Y * 0.04, lean, length, radius);
                }
                lights.push((base + Vec3::Y * 0.35 - toward * 0.15, glow));
            } else if roll < 0.28 {
                let height = 0.3 + r(5) * 0.6;
                let base = surface(top + toward * 0.3 + across * (r(6) - 0.5) * 0.4);
                stalagmite(rock, base - Vec3::Y * 0.03, height, 0.13 + r(7) * 0.1, solid.key, dim);
            }
        }
        if r(8) < 0.4 {
            let count = 2 + (r(9) * 4.0) as u32;
            for k in 0..count {
                let at = top
                    + Vec3::new(
                        rand(solid.key, 70 + k) - 0.5,
                        0.0,
                        rand(solid.key, 80 + k) - 0.5,
                    ) * 0.85;
                let size = 0.025 + rand(solid.key, 90 + k).powi(2) * 0.07;
                pebble(rock, surface(at), size, solid.key.wrapping_add(u64::from(k)), dim);
            }
        }
    }
}

/// A smooth vector noise field, gentle vertically near floors and ceilings
/// so they stay walkable while walls bulge and twist.
fn displacement(p: Vec3) -> Vec3 {
    let low = Vec3::new(fbm(p * 0.85, 11), fbm(p * 0.85, 23), fbm(p * 0.85, 37)) * 0.26;
    let high = Vec3::new(perlin(p * 2.7, 41), perlin(p * 2.7, 53), perlin(p * 2.7, 67)) * 0.075;
    let mut d = low + high;
    let plane = ((p.y - p.y.floor()) - 0.5).abs();
    d.y *= 0.12 + 0.88 * (plane * 4.0).min(1.0);
    d
}

#[derive(Default)]
struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
}

impl Builder {
    fn tri_shaded(&mut self, p: [Vec3; 3], shade: [f32; 3], dim: Vec3) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n == Vec3::ZERO {
            return;
        }
        for k in 0..3 {
            self.positions.push(p[k].to_array());
            self.normals.push(n.to_array());
            let c = dim * shade[k];
            self.colors.push([c.x, c.y, c.z, 1.0]);
        }
    }

    /// A triangle wound to face away from a point inside its solid.
    fn tri_out(&mut self, mut p: [Vec3; 3], inside: Vec3, dim: Vec3) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        if n.dot(p[0] - inside) < 0.0 {
            p.swap(1, 2);
        }
        self.tri_shaded(p, [1.0; 3], dim);
    }

    fn finish(self) -> Option<Mesh> {
        if self.positions.is_empty() {
            return None;
        }
        let count = self.positions.len() as u32;
        Some(
            Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD)
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
                .with_inserted_indices(Indices::U32((0..count).collect())),
        )
    }
}

fn basis(axis: Vec3) -> (Vec3, Vec3) {
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let e1 = axis.cross(helper).normalize();
    (e1, axis.cross(e1))
}

/// A six-sided crystal with a faceted point, its root buried in the floor.
fn crystal(out: &mut Builder, base: Vec3, axis: Vec3, length: f32, radius: f32) {
    let (e1, e2) = basis(axis);
    let ring = |at: f32, r: f32| -> Vec<Vec3> {
        (0..6)
            .map(|k| {
                let a = k as f32 * std::f32::consts::TAU / 6.0;
                base + axis * at + (e1 * a.cos() + e2 * a.sin()) * r
            })
            .collect()
    };
    let low = ring(0.0, radius * 0.85);
    let high = ring(length * 0.72, radius);
    let tip = base + axis * length;
    let inside = base + axis * length * 0.4;
    for k in 0..6 {
        let n = (k + 1) % 6;
        out.tri_out([low[k], low[n], high[n]], inside, Vec3::ONE);
        out.tri_out([low[k], high[n], high[k]], inside, Vec3::ONE);
        out.tri_out([high[k], high[n], tip], inside, Vec3::ONE);
    }
}

/// A knobbly cone of rock rising from the floor.
fn stalagmite(out: &mut Builder, base: Vec3, height: f32, radius: f32, key: u64, dim: Vec3) {
    const SIDES: usize = 7;
    let levels = [(0.0, 1.0), (0.35, 0.68), (0.7, 0.36), (0.9, 0.14)];
    let rings: Vec<Vec<Vec3>> = levels
        .iter()
        .enumerate()
        .map(|(l, (h, r))| {
            (0..SIDES)
                .map(|k| {
                    let a = (k as f32 + rand(key, 200 + l as u32) * 0.5) * std::f32::consts::TAU / SIDES as f32;
                    let wobble = 0.75 + rand(key, 300 + (l * SIDES + k) as u32) * 0.5;
                    base + Vec3::new(a.cos(), 0.0, a.sin()) * radius * r * wobble + Vec3::Y * height * h
                })
                .collect()
        })
        .collect();
    let tip = base + Vec3::Y * height;
    for l in 0..rings.len() - 1 {
        let inside = base + Vec3::Y * height * (levels[l].0 + levels[l + 1].0) * 0.5;
        for k in 0..SIDES {
            let n = (k + 1) % SIDES;
            out.tri_out([rings[l][k], rings[l][n], rings[l + 1][n]], inside, dim);
            out.tri_out([rings[l][k], rings[l + 1][n], rings[l + 1][k]], inside, dim);
        }
    }
    let last = rings.last().unwrap();
    for k in 0..SIDES {
        out.tri_out([last[k], last[(k + 1) % SIDES], tip], base + Vec3::Y * height * 0.85, dim);
    }
}

/// A squat, irregular stone.
fn pebble(out: &mut Builder, at: Vec3, size: f32, key: u64, dim: Vec3) {
    let turn = Quat::from_rotation_y(rand(key, 400) * std::f32::consts::TAU);
    let j = |i: u32| 0.7 + rand(key, 410 + i) * 0.6;
    let center = at + Vec3::Y * size * 0.25;
    let points = [
        Vec3::X * j(0),
        Vec3::NEG_X * j(1),
        Vec3::Z * j(2),
        Vec3::NEG_Z * j(3),
        Vec3::Y * 0.6 * j(4),
        Vec3::NEG_Y * 0.3,
    ]
    .map(|p| center + turn * (p * size));
    let faces = [
        (0, 2, 4),
        (2, 1, 4),
        (1, 3, 4),
        (3, 0, 4),
        (2, 0, 5),
        (1, 2, 5),
        (3, 1, 5),
        (0, 3, 5),
    ];
    for (a, b, c) in faces {
        out.tri_out([points[a], points[b], points[c]], center, dim);
    }
}

pub fn rand(key: u64, i: u32) -> f32 {
    let mut h = key ^ u64::from(i).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^= h >> 31;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

fn hash3(x: i32, y: i32, z: i32, seed: u32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x8da6_b343)
        ^ (y as u32).wrapping_mul(0xd816_3841)
        ^ (z as u32).wrapping_mul(0xcb1a_b31f)
        ^ seed.wrapping_mul(0x9e37_79b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    h = h.wrapping_mul(0x297a_2d39);
    h ^ (h >> 15)
}

fn gradient(h: u32) -> Vec3 {
    const G: [Vec3; 12] = [
        Vec3::new(1.0, 1.0, 0.0),
        Vec3::new(-1.0, 1.0, 0.0),
        Vec3::new(1.0, -1.0, 0.0),
        Vec3::new(-1.0, -1.0, 0.0),
        Vec3::new(1.0, 0.0, 1.0),
        Vec3::new(-1.0, 0.0, 1.0),
        Vec3::new(1.0, 0.0, -1.0),
        Vec3::new(-1.0, 0.0, -1.0),
        Vec3::new(0.0, 1.0, 1.0),
        Vec3::new(0.0, -1.0, 1.0),
        Vec3::new(0.0, 1.0, -1.0),
        Vec3::new(0.0, -1.0, -1.0),
    ];
    G[(h % 12) as usize]
}

/// Classic gradient noise, roughly in -1..1.
fn perlin(p: Vec3, seed: u32) -> f32 {
    let i = p.floor();
    let f = p - i;
    let (x, y, z) = (i.x as i32, i.y as i32, i.z as i32);
    let fade = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
    let corner = |dx: i32, dy: i32, dz: i32| {
        gradient(hash3(x + dx, y + dy, z + dz, seed)).dot(f - Vec3::new(dx as f32, dy as f32, dz as f32))
    };
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let x00 = lerp(corner(0, 0, 0), corner(1, 0, 0), fade.x);
    let x10 = lerp(corner(0, 1, 0), corner(1, 1, 0), fade.x);
    let x01 = lerp(corner(0, 0, 1), corner(1, 0, 1), fade.x);
    let x11 = lerp(corner(0, 1, 1), corner(1, 1, 1), fade.x);
    lerp(lerp(x00, x10, fade.y), lerp(x01, x11, fade.y), fade.z)
}

fn fbm(p: Vec3, seed: u32) -> f32 {
    perlin(p, seed) + 0.5 * perlin(p * 2.03 + Vec3::splat(17.1), seed + 1)
}
