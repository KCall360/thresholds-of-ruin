//! Hand-carved props in the cave's style: chunky faceted forms in a small
//! shared palette of wood, leather, cloth, bone, iron, steel, bronze and gold.
//! Only magical or valuable things carry a glowing accent, in the same teal,
//! violet and amber as the cave's crystals, so every prop belongs to one world.
//!
//! Models are built from code at first use and cached. Each has up to three
//! parts: matte, metal, and its glowing accent.
use bevy::{
    math::Affine3A,
    prelude::*,
    render::{
        mesh::{Indices, PrimitiveTopology},
        render_asset::RenderAssetUsages,
    },
};
use std::{collections::HashMap, f32::consts::TAU, sync::Arc};

#[cfg(test)]
#[path = "props_art_tests.rs"]
mod art_contract_tests;

// The shared palette.
const WOOD: Vec3 = Vec3::new(0.45, 0.29, 0.16);
const WOOD_DARK: Vec3 = Vec3::new(0.27, 0.16, 0.09);
const LEATHER: Vec3 = Vec3::new(0.34, 0.2, 0.12);
const CLOTH: Vec3 = Vec3::new(0.86, 0.82, 0.72);
const LINEN: Vec3 = Vec3::new(0.7, 0.6, 0.44);
const TWINE: Vec3 = Vec3::new(0.5, 0.4, 0.26);
const BONE: Vec3 = Vec3::new(0.86, 0.82, 0.7);
const IRON: Vec3 = Vec3::new(0.42, 0.43, 0.47);
const STEEL: Vec3 = Vec3::new(0.72, 0.74, 0.78);
const BRONZE: Vec3 = Vec3::new(0.74, 0.5, 0.26);
const GOLD: Vec3 = Vec3::new(0.95, 0.74, 0.32);
const STONE: Vec3 = Vec3::new(0.47, 0.45, 0.5);
const STONE_DARK: Vec3 = Vec3::new(0.32, 0.3, 0.35);

/// The glowing accent colours, shared with the cave's crystals.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Accent {
    Teal,
    Violet,
    Amber,
    Ember,
    White,
    Abyss,
    Gold,
    Danger,
    Travel,
}

impl Accent {
    fn colors(self) -> (Color, LinearRgba) {
        match self {
            Accent::Teal => (
                Color::srgb(0.35, 0.95, 0.85),
                LinearRgba::rgb(0.4, 2.4, 2.0),
            ),
            Accent::Violet => (Color::srgb(0.75, 0.5, 1.0), LinearRgba::rgb(1.6, 0.7, 2.8)),
            Accent::Amber => (
                Color::srgb(1.0, 0.75, 0.35),
                LinearRgba::rgb(2.8, 1.6, 0.45),
            ),
            Accent::Ember => (Color::srgb(1.0, 0.5, 0.15), LinearRgba::rgb(3.0, 1.0, 0.18)),
            Accent::White => (Color::WHITE, LinearRgba::rgb(3.0, 2.8, 2.4)),
            Accent::Abyss => (Color::srgb(0.3, 0.5, 1.0), LinearRgba::rgb(0.4, 0.9, 2.6)),
            Accent::Gold => (Color::srgb(1.0, 0.8, 0.4), LinearRgba::rgb(2.2, 1.5, 0.5)),
            Accent::Danger => (
                Color::srgb(1.0, 0.3, 0.25),
                LinearRgba::rgb(2.6, 0.35, 0.25),
            ),
            Accent::Travel => (Color::srgb(0.4, 0.9, 1.0), LinearRgba::rgb(0.5, 1.8, 2.4)),
        }
    }

    pub fn light(self) -> Color {
        self.colors().0
    }
}

/// Surface families shared by props and the entire creature cast.
/// Keep broad diffuse colour on cloth, hide and feathers; reserve the tighter
/// highlights for leather and metal so material reads survive gameplay distance.
pub struct PropMaterials {
    matte: Handle<StandardMaterial>,
    /// Leather and other surfaces with a soft sheen.
    satin: Handle<StandardMaterial>,
    metal: Handle<StandardMaterial>,
    /// Translucent spectral surfaces, faintly lit from within.
    ghost: Handle<StandardMaterial>,
    glow: HashMap<Accent, Handle<StandardMaterial>>,
    /// The same accents nearly put out, for the glow on the dead.
    dim: HashMap<Accent, Handle<StandardMaterial>>,
}

impl PropMaterials {
    /// The polished metal every model's armour and fittings share.
    #[cfg(test)]
    pub fn metal(&self) -> &Handle<StandardMaterial> {
        &self.metal
    }

    pub fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        let glow = [
            Accent::Teal,
            Accent::Violet,
            Accent::Amber,
            Accent::Ember,
            Accent::White,
            Accent::Abyss,
            Accent::Gold,
            Accent::Danger,
            Accent::Travel,
        ]
        .into_iter()
        .map(|accent| {
            let (base, emissive) = accent.colors();
            let material = materials.add(StandardMaterial {
                base_color: base,
                emissive,
                perceptual_roughness: 0.25,
                ..default()
            });
            (accent, material)
        })
        .collect::<HashMap<_, _>>();
        let dim = glow
            .keys()
            .map(|&accent| {
                let (base, emissive) = accent.colors();
                let material = materials.add(StandardMaterial {
                    base_color: base.darker(0.45),
                    emissive: emissive * 0.05,
                    perceptual_roughness: 0.6,
                    ..default()
                });
                (accent, material)
            })
            .collect();
        Self {
            matte: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.92,
                reflectance: 0.28,
                ..default()
            }),
            satin: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.46,
                reflectance: 0.42,
                ..default()
            }),
            metal: materials.add(StandardMaterial {
                base_color: Color::WHITE,
                metallic: 0.78,
                perceptual_roughness: 0.27,
                reflectance: 0.5,
                ..default()
            }),
            ghost: materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 1.0, 1.0, 0.78),
                emissive: LinearRgba::rgb(0.035, 0.05, 0.09),
                perceptual_roughness: 0.6,
                alpha_mode: AlphaMode::Blend,
                ..default()
            }),
            glow,
            dim,
        }
    }

    /// The put-out version of a glowing material, if it is one.
    pub fn dim(&self, material: &Handle<StandardMaterial>) -> Option<Handle<StandardMaterial>> {
        let accent = self.glow.iter().find(|(_, h)| *h == material)?.0;
        Some(self.dim[accent].clone())
    }

    /// The lit version of a put-out material, if it is one.
    pub fn undim(&self, material: &Handle<StandardMaterial>) -> Option<Handle<StandardMaterial>> {
        let accent = self.dim.iter().find(|(_, h)| *h == material)?.0;
        Some(self.glow[accent].clone())
    }

    pub fn glow(&self, accent: Accent) -> Handle<StandardMaterial> {
        self.glow[&accent].clone()
    }

    /// The translucent material spectral bodies are drawn with.
    #[cfg(test)]
    pub fn ghost(&self) -> Handle<StandardMaterial> {
        self.ghost.clone()
    }
}

/// A finished model: meshes with their materials, positioned in model space.
#[derive(Clone, Default)]
pub struct Model(pub Vec<(Handle<Mesh>, Handle<StandardMaterial>)>);

impl Model {
    /// Spawn the model's meshes as children of an entity.
    pub fn attach(&self, parent: &mut ChildSpawnerCommands) {
        self.attach_scaled(parent, Vec3::ONE);
    }

    /// Art-directed mesh proportions without changing the rig's joint lengths.
    pub(crate) fn attach_scaled(&self, parent: &mut ChildSpawnerCommands, scale: Vec3) {
        for (mesh, material) in &self.0 {
            parent.spawn((
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_scale(scale),
            ));
        }
    }
}

/// A model under construction.
#[derive(Default)]
pub(crate) struct Parts {
    pub matte: Builder,
    pub satin: Builder,
    pub metal: Builder,
    pub glow: Builder,
    /// Seen through: drawn with the translucent spectral material.
    pub ghost: Builder,
    pub accent: Option<Accent>,
}

impl Parts {
    pub(crate) fn finish(self, meshes: &mut Assets<Mesh>, materials: &PropMaterials) -> Model {
        let mut model = Model::default();
        if let Some(mesh) = self.matte.finish() {
            model.0.push((meshes.add(mesh), materials.matte.clone()));
        }
        if let Some(mesh) = self.satin.finish() {
            model.0.push((meshes.add(mesh), materials.satin.clone()));
        }
        if let Some(mesh) = self.metal.finish() {
            model.0.push((meshes.add(mesh), materials.metal.clone()));
        }
        if let Some(mesh) = self.ghost.finish() {
            model.0.push((meshes.add(mesh), materials.ghost.clone()));
        }
        if let (Some(mesh), Some(accent)) = (self.glow.finish(), self.accent) {
            model.0.push((meshes.add(mesh), materials.glow(accent)));
        }
        model
    }

    /// Soft, finely tessellated shading for organic forms.
    pub(crate) fn soften(&mut self) {
        for b in [
            &mut self.matte,
            &mut self.satin,
            &mut self.metal,
            &mut self.glow,
            &mut self.ghost,
        ] {
            b.res = (9, 16);
            b.sides_mul = 2;
            b.smooth = true;
        }
    }

    pub(crate) fn set_transform(&mut self, xf: Affine3A) {
        self.matte.xf = xf;
        self.satin.xf = xf;
        self.metal.xf = xf;
        self.glow.xf = xf;
        self.ghost.xf = xf;
    }

    /// Move every opaque surface into the translucent spectral layer; glowing
    /// eyes and accents stay solid so they still read through the body.
    pub(crate) fn ghostify(&mut self) {
        for b in [&mut self.matte, &mut self.satin, &mut self.metal] {
            let taken = std::mem::take(b);
            self.ghost.absorb(taken);
        }
    }
}

// Live models are retained regardless of size. These limits apply only to
// unused models kept warm for reuse; the count also bounds empty model keys.
const WARM_PAYLOAD_LIMIT: usize = 128 * 1024 * 1024;
const WARM_MODEL_LIMIT: usize = 1024;

struct CachedModel {
    model: Model,
    payload_bytes: usize,
    last_used: u64,
}

impl CachedModel {
    fn is_live(&self) -> bool {
        // Parts::finish creates a unique strong handle for each mesh. The
        // cache owns one clone; entities and queued spawns own the others.
        // Render extraction retains mesh IDs/data, not another handle clone.
        self.model.0.iter().any(|(mesh, _)| match mesh {
            Handle::Strong(owner) => Arc::strong_count(owner) > 1,
            Handle::Weak(_) => false,
        })
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct CacheUsage {
    live_models: usize,
    live_payload_bytes: usize,
    warm_models: usize,
    warm_payload_bytes: usize,
    evicted_models: usize,
}

/// Reuses live models and a bounded set of recently used idle models.
#[derive(Resource)]
pub struct PropLibrary {
    pub materials: PropMaterials,
    cache: HashMap<String, CachedModel>,
    payload_bytes: usize,
    clock: u64,
    evicted_models: usize,
    last_report: Option<CacheUsage>,
}

/// Optional local startup profiling; read the environment once for the process.
pub(crate) fn profile_builds() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("TOR_3D_PROFILE_BUILD").is_some_and(|value| value == "1"))
}

impl PropLibrary {
    pub fn new(materials: PropMaterials) -> Self {
        Self {
            materials,
            cache: HashMap::new(),
            payload_bytes: 0,
            clock: 0,
            evicted_models: 0,
            last_report: None,
        }
    }

    pub(crate) fn get(
        &mut self,
        key: String,
        meshes: &mut Assets<Mesh>,
        build: impl FnOnce() -> Parts,
    ) -> Model {
        self.clock += 1;
        if let Some(entry) = self.cache.get_mut(&key) {
            entry.last_used = self.clock;
            return entry.model.clone();
        }
        let started = profile_builds().then(std::time::Instant::now);
        let parts = build();
        let parts_ms = started.map(|time| time.elapsed().as_secs_f64() * 1000.0);
        let model = parts.finish(meshes, &self.materials);
        // Measure while the newly added meshes are still in main-world Assets.
        // RENDER_WORLD-only meshes leave that collection after extraction.
        let payload_bytes = model
            .0
            .iter()
            .map(|(handle, _)| {
                let mesh = meshes.get(handle).expect("newly built model mesh");
                mesh.get_vertex_buffer_size() + mesh.get_index_buffer_bytes().map_or(0, <[u8]>::len)
            })
            .sum();
        if let Some(started) = started {
            let total_ms = started.elapsed().as_secs_f64() * 1000.0;
            eprintln!(
                "3d_model_build key={key:?} ms={total_ms:.3} parts_ms={:.3} finish_ms={:.3} cache={}",
                parts_ms.unwrap_or(0.0),
                total_ms - parts_ms.unwrap_or(0.0),
                self.cache.len() + 1,
            );
        }
        self.cache.insert(
            key,
            CachedModel {
                model: model.clone(),
                payload_bytes,
                last_used: self.clock,
            },
        );
        self.payload_bytes += payload_bytes;
        // Below these total bounds the idle subset cannot exceed its budget.
        // Avoid scanning the growing cache on every ordinary startup insertion.
        if self.payload_bytes > WARM_PAYLOAD_LIMIT || self.cache.len() > WARM_MODEL_LIMIT {
            self.trim();
        }
        model
    }

    fn trim(&mut self) -> CacheUsage {
        self.clock += 1;
        let mut usage = CacheUsage {
            evicted_models: self.evicted_models,
            ..default()
        };
        for entry in self.cache.values_mut() {
            if entry.is_live() {
                // A long-lived actor enters the warm cache as recently used
                // when it disappears, rather than retaining its spawn age.
                entry.last_used = self.clock;
                usage.live_models += 1;
                usage.live_payload_bytes += entry.payload_bytes;
            } else {
                usage.warm_models += 1;
                usage.warm_payload_bytes += entry.payload_bytes;
            }
        }
        if usage.warm_payload_bytes > WARM_PAYLOAD_LIMIT || usage.warm_models > WARM_MODEL_LIMIT {
            let mut idle: Vec<_> = self
                .cache
                .iter()
                .filter(|(_, entry)| entry.last_used != self.clock)
                .map(|(key, entry)| (entry.last_used, key.clone()))
                .collect();
            idle.sort_unstable();
            for (_, key) in idle {
                if usage.warm_payload_bytes <= WARM_PAYLOAD_LIMIT
                    && usage.warm_models <= WARM_MODEL_LIMIT
                {
                    break;
                }
                if let Some(entry) = self.cache.remove(&key) {
                    usage.warm_models -= 1;
                    usage.warm_payload_bytes -= entry.payload_bytes;
                    self.payload_bytes -= entry.payload_bytes;
                    self.evicted_models += 1;
                }
                // Dropping handles lets Bevy release unused render assets.
                // Never remove an asset directly while an entity may own it.
            }
        }
        usage.evicted_models = self.evicted_models;
        usage
    }

    /// Build a resolved cosmetic recipe; names never select geometry.
    pub fn item(&mut self, kind: Kind, meshes: &mut Assets<Mesh>) -> Model {
        self.get(format!("item:{kind:?}"), meshes, move || {
            build_item(kind, "neutral")
        })
    }

    pub fn door_frame(&mut self, meshes: &mut Assets<Mesh>) -> Model {
        self.get("door-frame".into(), meshes, door_frame)
    }

    pub fn door_leaf(&mut self, meshes: &mut Assets<Mesh>) -> Model {
        self.get("door-leaf".into(), meshes, door_leaf)
    }

    pub fn stairs_up(&mut self, meshes: &mut Assets<Mesh>) -> Model {
        self.get("stairs-up".into(), meshes, stairs_up)
    }

    pub fn stairs_down(&mut self, meshes: &mut Assets<Mesh>) -> Model {
        self.get("stairs-down".into(), meshes, stairs_down)
    }

    pub fn glint(&mut self, meshes: &mut Assets<Mesh>) -> Model {
        self.get("glint".into(), meshes, || {
            let mut p = Parts {
                accent: Some(Accent::White),
                ..default()
            };
            p.glow.cuboid(
                Vec3::ZERO,
                Vec3::new(0.05, 0.004, 0.004),
                Quat::IDENTITY,
                Vec3::ONE,
            );
            p.glow.cuboid(
                Vec3::ZERO,
                Vec3::new(0.004, 0.05, 0.004),
                Quat::IDENTITY,
                Vec3::ONE,
            );
            p.glow.cuboid(
                Vec3::ZERO,
                Vec3::new(0.004, 0.004, 0.05),
                Quat::IDENTITY,
                Vec3::ONE,
            );
            p
        })
    }

    /// A frame of light around a floor cell, for the cursor.
    pub fn selection(&mut self, accent: Accent, meshes: &mut Assets<Mesh>) -> Model {
        self.get(format!("select:{accent:?}"), meshes, move || {
            let mut p = Parts {
                accent: Some(accent),
                ..default()
            };
            for (center, half) in [
                (Vec3::new(0.0, 0.0, 0.46), Vec3::new(0.5, 0.012, 0.035)),
                (Vec3::new(0.0, 0.0, -0.46), Vec3::new(0.5, 0.012, 0.035)),
                (Vec3::new(0.46, 0.0, 0.0), Vec3::new(0.035, 0.012, 0.43)),
                (Vec3::new(-0.46, 0.0, 0.0), Vec3::new(0.035, 0.012, 0.43)),
            ] {
                p.glow.cuboid(center, half, Quat::IDENTITY, Vec3::ONE);
            }
            p
        })
    }
}

/// Sweep after actor despawns even when no new model is requested.
pub(crate) fn maintain_cache(
    time: Res<Time<Real>>,
    mut library: ResMut<PropLibrary>,
    mut next_sweep: Local<f64>,
) {
    let now = time.elapsed_secs_f64();
    if now < *next_sweep {
        return;
    }
    *next_sweep = now + 1.0;
    let usage = library.trim();
    if profile_builds() && library.last_report != Some(usage) {
        eprintln!(
            "3d_model_cache live={} live_bytes={} warm={} warm_bytes={} evicted={} warm_limit_bytes={} warm_limit_models={}",
            usage.live_models,
            usage.live_payload_bytes,
            usage.warm_models,
            usage.warm_payload_bytes,
            usage.evicted_models,
            WARM_PAYLOAD_LIMIT,
            WARM_MODEL_LIMIT,
        );
        library.last_report = Some(usage);
    }
}

pub use tor_client_3d_model::ItemModel as Kind;

pub(crate) fn hash(text: &str) -> u64 {
    text.bytes().fold(1469598103934665603u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    })
}

/// A magical colour for things that vary by name.
fn accent_for(name: &str) -> Accent {
    [Accent::Teal, Accent::Violet, Accent::Amber, Accent::Ember][(hash(name) % 4) as usize]
}

fn build_item(kind: Kind, name: &str) -> Parts {
    let mut p = Parts::default();
    match kind {
        Kind::Stairs => return stairs_down(),
        Kind::Placeholder => p.matte.blob(
            Vec3::Y * 0.16,
            Vec3::new(0.22, 0.16, 0.22),
            0,
            0.0,
            Vec3::new(0.38, 0.6, 0.68),
        ),
        Kind::Mace => mace(&mut p),
        Kind::Greatsword => sword(&mut p, 1.0),
        Kind::ShortSword => sword(&mut p, 0.62),
        Kind::RingMail => ring_mail(&mut p),
        Kind::Bandage => bandage(&mut p),
        Kind::Ration => ration(&mut p),
        Kind::Corpse => corpse(&mut p, name),
        Kind::Seal => seal(&mut p),
        Kind::Coins => coins(&mut p, name),
        Kind::Tablet => tablet(&mut p),
        Kind::Potion => potion(&mut p, name),
        Kind::Ring => ring(&mut p),
        Kind::Amulet => amulet(&mut p),
        Kind::Scroll => scroll(&mut p),
        Kind::Spellbook => spellbook(&mut p),
        Kind::Wand => wand(&mut p),
        Kind::Gem => gem(&mut p, name),
        Kind::Pick => pick(&mut p),
        Kind::Pouch => pouch(&mut p),
    }
    p
}

/// A flanged mace lying on the floor, its head propping up the haft.
fn mace(p: &mut Parts) {
    let a = Vec3::new(-0.34, 0.028, 0.0);
    let b = Vec3::new(0.26, 0.072, 0.0);
    let at = |t: f32| a.lerp(b, t);
    let axis = (b - a).normalize();
    p.matte
        .prism(at(0.05), at(0.72), 0.024, 0.027, 6, WOOD, 0.0);
    p.matte
        .prism(at(0.04), at(0.27), 0.031, 0.031, 6, LEATHER, 0.5);
    for t in [0.08, 0.15, 0.22] {
        p.matte
            .prism(at(t), at(t + 0.025), 0.034, 0.034, 6, WOOD_DARK, 0.5);
    }
    p.metal.prism(at(0.0), at(0.05), 0.036, 0.03, 6, IRON, 0.0);
    p.metal.prism(at(0.7), at(0.93), 0.05, 0.046, 6, IRON, 0.0);
    p.metal.prism(at(0.93), at(1.0), 0.032, 0.0, 6, IRON, 0.0);
    let (e1, e2) = basis(axis);
    for k in 0..6 {
        let angle = (k as f32 + 0.5) * TAU / 6.0;
        let radial = e1 * angle.cos() + e2 * angle.sin();
        p.metal.extrude(
            &[
                Vec2::new(-0.075, 0.03),
                Vec2::new(0.075, 0.03),
                Vec2::new(0.05, 0.088),
                Vec2::new(-0.035, 0.092),
            ],
            at(0.815),
            axis,
            radial,
            0.011,
            STEEL,
        );
    }
}

/// A sword lying flat: fullered blade, bronze crossguard, wrapped grip.
fn sword(p: &mut Parts, scale: f32) {
    p.set_transform(Affine3A::from_scale(Vec3::splat(scale)));
    let y = 0.024;
    p.metal.extrude(
        &[
            Vec2::new(-0.2, -0.04),
            Vec2::new(0.36, -0.034),
            Vec2::new(0.48, 0.0),
            Vec2::new(0.36, 0.034),
            Vec2::new(-0.2, 0.04),
        ],
        Vec3::new(0.0, y, 0.0),
        Vec3::X,
        Vec3::NEG_Z,
        0.018,
        STEEL,
    );
    p.metal.cuboid(
        Vec3::new(0.06, y + 0.0095, 0.0),
        Vec3::new(0.25, 0.0015, 0.009),
        Quat::IDENTITY,
        IRON,
    );
    p.metal.cuboid(
        Vec3::new(-0.215, 0.032, 0.0),
        Vec3::new(0.02, 0.026, 0.12),
        Quat::IDENTITY,
        BRONZE,
    );
    p.metal.blob(
        Vec3::new(-0.215, 0.032, 0.13),
        Vec3::splat(0.026),
        0,
        0.0,
        BRONZE,
    );
    p.metal.blob(
        Vec3::new(-0.215, 0.032, -0.13),
        Vec3::splat(0.026),
        0,
        0.0,
        BRONZE,
    );
    p.matte.prism(
        Vec3::new(-0.235, 0.03, 0.0),
        Vec3::new(-0.4, 0.03, 0.0),
        0.022,
        0.022,
        6,
        LEATHER,
        0.0,
    );
    for x in [-0.27, -0.32, -0.37] {
        p.matte.prism(
            Vec3::new(x, 0.03, 0.0),
            Vec3::new(x - 0.012, 0.03, 0.0),
            0.026,
            0.026,
            6,
            WOOD_DARK,
            0.5,
        );
    }
    p.metal.blob(
        Vec3::new(-0.425, 0.034, 0.0),
        Vec3::splat(0.034),
        7,
        0.15,
        BRONZE,
    );
}

/// A ring-mail shirt laid flat, its rows of rings catching the light.
fn ring_mail(p: &mut Parts) {
    p.metal.extrude(
        &[
            Vec2::new(-0.17, -0.25),
            Vec2::new(0.17, -0.25),
            Vec2::new(0.18, 0.06),
            Vec2::new(0.3, 0.1),
            Vec2::new(0.28, 0.2),
            Vec2::new(0.1, 0.25),
            Vec2::new(0.0, 0.2),
            Vec2::new(-0.1, 0.25),
            Vec2::new(-0.28, 0.2),
            Vec2::new(-0.3, 0.1),
            Vec2::new(-0.18, 0.06),
        ],
        Vec3::new(0.0, 0.025, 0.0),
        Vec3::X,
        Vec3::NEG_Z,
        0.05,
        IRON * 0.8,
    );
    for row in 0..6 {
        for col in 0..5 {
            let x = -0.13 + col as f32 * 0.065 + if row % 2 == 1 { 0.032 } else { 0.0 };
            if x > 0.15 {
                continue;
            }
            let z = 0.18 - row as f32 * 0.062;
            p.metal
                .torus(Vec3::new(x, 0.052, z), Vec3::Y, 0.024, 0.0045, 10, 4, STEEL);
        }
    }
    p.matte.cuboid(
        Vec3::new(0.0, 0.054, 0.17),
        Vec3::new(0.172, 0.01, 0.022),
        Quat::IDENTITY,
        LEATHER,
    );
    p.metal.cuboid(
        Vec3::new(0.0, 0.066, 0.17),
        Vec3::new(0.03, 0.006, 0.026),
        Quat::IDENTITY,
        BRONZE,
    );
    p.matte.torus(
        Vec3::new(0.0, 0.05, -0.2),
        Vec3::Y,
        0.075,
        0.012,
        9,
        4,
        LEATHER,
    );
}

/// A rolled bandage with its loose end trailing across the floor.
fn bandage(p: &mut Parts) {
    let r = 0.06;
    p.matte.prism(
        Vec3::new(0.0, r, -0.065),
        Vec3::new(0.0, r, 0.065),
        r,
        r,
        10,
        CLOTH,
        0.0,
    );
    p.matte.prism(
        Vec3::new(0.0, r, -0.067),
        Vec3::new(0.0, r, 0.067),
        0.022,
        0.022,
        8,
        LINEN,
        0.0,
    );
    p.matte.cuboid(
        Vec3::new(0.12, 0.009, 0.0),
        Vec3::new(0.08, 0.008, 0.056),
        Quat::from_rotation_y(0.12),
        CLOTH,
    );
    p.matte.cuboid(
        Vec3::new(0.27, 0.009, 0.035),
        Vec3::new(0.08, 0.008, 0.053),
        Quat::from_rotation_y(0.38),
        CLOTH * 0.95,
    );
}

/// A cloth-wrapped ration tied with twine, a hard biscuit propped beside it.
fn ration(p: &mut Parts) {
    let turn = Quat::from_rotation_y(0.2);
    p.matte.cuboid(
        Vec3::new(0.0, 0.06, 0.0),
        Vec3::new(0.14, 0.06, 0.09),
        turn,
        LINEN,
    );
    p.matte.cuboid(
        Vec3::new(0.0, 0.06, 0.0),
        Vec3::new(0.143, 0.062, 0.01),
        turn,
        TWINE,
    );
    p.matte.cuboid(
        Vec3::new(0.0, 0.06, 0.0),
        Vec3::new(0.01, 0.062, 0.093),
        turn,
        TWINE,
    );
    p.matte.blob(
        Vec3::new(0.0, 0.126, 0.0),
        Vec3::new(0.025, 0.016, 0.025),
        3,
        0.2,
        TWINE,
    );
    p.matte.cuboid(
        Vec3::new(0.05, 0.03, 0.16),
        Vec3::new(0.08, 0.016, 0.05),
        Quat::from_rotation_y(-0.4) * Quat::from_rotation_x(0.35),
        Vec3::new(0.78, 0.6, 0.36),
    );
}

/// What a fallen creature leaves: a slumped heap, a skull and a bone.
fn corpse(p: &mut Parts, name: &str) {
    let key = hash(name);
    let tint = crate::creature_tint(name).lerp(Vec3::new(0.12, 0.1, 0.1), 0.55);
    p.matte.blob(
        Vec3::new(0.0, 0.06, 0.0),
        Vec3::new(0.32, 0.1, 0.22),
        key,
        0.25,
        tint,
    );
    p.matte.blob(
        Vec3::new(0.14, 0.05, 0.1),
        Vec3::new(0.18, 0.08, 0.14),
        key ^ 7,
        0.3,
        tint * 0.8,
    );
    p.matte.blob(
        Vec3::new(-0.3, 0.07, 0.05),
        Vec3::new(0.07, 0.065, 0.075),
        key ^ 3,
        0.08,
        BONE,
    );
    p.matte.blob(
        Vec3::new(-0.36, 0.075, 0.03),
        Vec3::splat(0.016),
        0,
        0.0,
        Vec3::splat(0.05),
    );
    p.matte.blob(
        Vec3::new(-0.36, 0.075, 0.08),
        Vec3::splat(0.016),
        0,
        0.0,
        Vec3::splat(0.05),
    );
    p.matte.prism(
        Vec3::new(0.18, 0.018, -0.2),
        Vec3::new(0.42, 0.018, -0.06),
        0.014,
        0.014,
        5,
        BONE,
        0.0,
    );
    p.matte.blob(
        Vec3::new(0.18, 0.02, -0.2),
        Vec3::splat(0.024),
        1,
        0.1,
        BONE,
    );
    p.matte.blob(
        Vec3::new(0.42, 0.02, -0.06),
        Vec3::splat(0.024),
        2,
        0.1,
        BONE,
    );
}

/// A gold medallion stamped with a sunburst that glows like dawn.
fn seal(p: &mut Parts) {
    p.accent = Some(Accent::Amber);
    p.metal.prism(
        Vec3::ZERO,
        Vec3::new(0.0, 0.03, 0.0),
        0.13,
        0.13,
        16,
        GOLD,
        0.0,
    );
    p.metal
        .torus(Vec3::new(0.0, 0.03, 0.0), Vec3::Y, 0.12, 0.012, 16, 4, GOLD);
    for k in 0..8 {
        let a = k as f32 * TAU / 8.0;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        p.glow.cuboid(
            dir * 0.065 + Vec3::Y * 0.033,
            Vec3::new(0.025, 0.003, 0.006),
            Quat::from_rotation_y(-a),
            Vec3::ONE,
        );
    }
    p.glow.blob(
        Vec3::new(0.0, 0.04, 0.0),
        Vec3::new(0.035, 0.018, 0.035),
        5,
        0.0,
        Vec3::ONE,
    );
}

/// A small stack of struck tokens in the metal they are named for.
fn coins(p: &mut Parts, name: &str) {
    let n = name.to_ascii_lowercase();
    let metal = if n.contains("copper") {
        Vec3::new(0.78, 0.45, 0.27)
    } else if n.contains("silver") {
        Vec3::new(0.82, 0.84, 0.88)
    } else if n.contains("iron") {
        IRON
    } else {
        GOLD
    };
    let key = hash(name);
    for k in 0..4 {
        let jitter = Vec3::new(rand(key, k) - 0.5, 0.0, rand(key, k + 9) - 0.5) * 0.02;
        let base = Vec3::new(0.0, k as f32 * 0.013, 0.0) + jitter;
        p.metal
            .prism(base, base + Vec3::Y * 0.012, 0.055, 0.055, 12, metal, 0.0);
        p.metal.prism(
            base + Vec3::Y * 0.012,
            base + Vec3::Y * 0.014,
            0.022,
            0.022,
            8,
            metal * 0.75,
            0.0,
        );
    }
    let lean = Vec3::new(0.09, 0.0, 0.05);
    p.metal.prism(
        lean + Vec3::new(0.0, 0.05, 0.0),
        lean + Vec3::new(0.012, 0.05, 0.0),
        0.05,
        0.05,
        12,
        metal,
        0.0,
    );
}

/// A standing stone tablet with runes that glow in the cave's teal.
fn tablet(p: &mut Parts) {
    p.accent = Some(Accent::Teal);
    p.matte.cuboid(
        Vec3::new(0.0, 0.03, 0.0),
        Vec3::new(0.25, 0.03, 0.09),
        Quat::IDENTITY,
        STONE_DARK,
    );
    let lean =
        Affine3A::from_translation(Vec3::new(0.0, 0.06, 0.0)) * Affine3A::from_rotation_x(-0.08);
    p.set_transform(lean);
    let mut outline = vec![
        Vec2::new(-0.19, 0.0),
        Vec2::new(0.19, 0.0),
        Vec2::new(0.19, 0.4),
    ];
    for k in 1..8 {
        let a = k as f32 / 8.0 * std::f32::consts::PI;
        outline.push(Vec2::new(0.19 * a.cos(), 0.4 + 0.19 * a.sin()));
    }
    outline.push(Vec2::new(-0.19, 0.4));
    p.matte
        .extrude(&outline, Vec3::ZERO, Vec3::X, Vec3::Y, 0.08, STONE);
    let key = 41;
    for row in 0..4 {
        for col in 0..3 {
            let center = Vec3::new(-0.1 + col as f32 * 0.1, 0.1 + row as f32 * 0.1, 0.042);
            let r = |i: u32| rand(key, row * 10 + col * 3 + i);
            p.glow.cuboid(
                center,
                Vec3::new(0.03, 0.004, 0.003),
                Quat::from_rotation_z(r(0) * 1.2 - 0.6),
                Vec3::ONE,
            );
            p.glow.cuboid(
                center + Vec3::new(r(1) * 0.02 - 0.01, 0.015, 0.0),
                Vec3::new(0.004, 0.018, 0.003),
                Quat::from_rotation_z(r(2) * 0.8 - 0.4),
                Vec3::ONE,
            );
        }
    }
    p.glow.torus(
        Vec3::new(0.0, 0.52, 0.042),
        Vec3::Z,
        0.045,
        0.006,
        12,
        3,
        Vec3::ONE,
    );
}

/// A stoppered flask whose contents glow.
fn potion(p: &mut Parts, name: &str) {
    p.accent = Some(accent_for(name));
    p.glow.blob(
        Vec3::new(0.0, 0.085, 0.0),
        Vec3::new(0.08, 0.085, 0.08),
        0,
        0.0,
        Vec3::ONE,
    );
    p.matte.prism(
        Vec3::new(0.0, 0.15, 0.0),
        Vec3::new(0.0, 0.22, 0.0),
        0.028,
        0.024,
        8,
        Vec3::new(0.2, 0.24, 0.26),
        0.0,
    );
    p.matte.prism(
        Vec3::new(0.0, 0.215, 0.0),
        Vec3::new(0.0, 0.25, 0.0),
        0.03,
        0.032,
        8,
        WOOD,
        0.0,
    );
    p.metal.torus(
        Vec3::new(0.0, 0.16, 0.0),
        Vec3::Y,
        0.03,
        0.006,
        8,
        3,
        BRONZE,
    );
}

fn ring(p: &mut Parts) {
    p.accent = Some(Accent::Violet);
    p.metal.torus(
        Vec3::new(0.0, 0.014, 0.0),
        Vec3::Y,
        0.05,
        0.014,
        14,
        5,
        GOLD,
    );
    p.glow.blob(
        Vec3::new(0.055, 0.032, 0.0),
        Vec3::splat(0.022),
        0,
        0.0,
        Vec3::ONE,
    );
}

fn amulet(p: &mut Parts) {
    p.accent = Some(Accent::Teal);
    p.metal.torus(
        Vec3::new(-0.04, 0.006, 0.0),
        Vec3::Y,
        0.12,
        0.006,
        18,
        3,
        GOLD,
    );
    p.metal.prism(
        Vec3::new(0.1, 0.0, 0.0),
        Vec3::new(0.1, 0.016, 0.0),
        0.045,
        0.045,
        10,
        GOLD,
        0.0,
    );
    p.glow.blob(
        Vec3::new(0.1, 0.022, 0.0),
        Vec3::new(0.022, 0.012, 0.022),
        0,
        0.0,
        Vec3::ONE,
    );
}

fn scroll(p: &mut Parts) {
    p.accent = Some(Accent::Ember);
    p.matte.prism(
        Vec3::new(0.0, 0.035, -0.12),
        Vec3::new(0.0, 0.035, 0.12),
        0.035,
        0.035,
        9,
        Vec3::new(0.88, 0.8, 0.6),
        0.0,
    );
    p.matte.prism(
        Vec3::new(0.0, 0.035, -0.14),
        Vec3::new(0.0, 0.035, -0.12),
        0.02,
        0.02,
        6,
        WOOD_DARK,
        0.0,
    );
    p.matte.prism(
        Vec3::new(0.0, 0.035, 0.12),
        Vec3::new(0.0, 0.035, 0.14),
        0.02,
        0.02,
        6,
        WOOD_DARK,
        0.0,
    );
    p.glow.blob(
        Vec3::new(0.0, 0.07, 0.0),
        Vec3::new(0.022, 0.01, 0.022),
        2,
        0.1,
        Vec3::ONE,
    );
}

fn spellbook(p: &mut Parts) {
    p.accent = Some(Accent::Violet);
    p.matte.cuboid(
        Vec3::new(0.0, 0.045, 0.0),
        Vec3::new(0.12, 0.045, 0.16),
        Quat::from_rotation_y(0.15),
        Vec3::new(0.36, 0.13, 0.2),
    );
    p.matte.cuboid(
        Vec3::new(0.012, 0.045, 0.0),
        Vec3::new(0.115, 0.036, 0.152),
        Quat::from_rotation_y(0.15),
        Vec3::new(0.9, 0.85, 0.7),
    );
    p.glow.torus(
        Vec3::new(0.0, 0.092, 0.0),
        Vec3::Y,
        0.055,
        0.007,
        12,
        3,
        Vec3::ONE,
    );
    p.metal.cuboid(
        Vec3::new(0.125, 0.045, 0.02),
        Vec3::new(0.01, 0.02, 0.03),
        Quat::from_rotation_y(0.15),
        BRONZE,
    );
}

fn wand(p: &mut Parts) {
    p.accent = Some(Accent::Teal);
    p.matte.prism(
        Vec3::new(-0.2, 0.016, 0.0),
        Vec3::new(0.17, 0.02, 0.0),
        0.014,
        0.009,
        6,
        WOOD_DARK,
        0.0,
    );
    p.metal.prism(
        Vec3::new(0.14, 0.02, 0.0),
        Vec3::new(0.17, 0.02, 0.0),
        0.014,
        0.014,
        6,
        BRONZE,
        0.0,
    );
    p.glow.prism(
        Vec3::new(0.17, 0.02, 0.0),
        Vec3::new(0.22, 0.02, 0.0),
        0.016,
        0.0,
        6,
        Vec3::ONE,
        0.0,
    );
}

fn gem(p: &mut Parts, name: &str) {
    p.accent = Some(accent_for(name));
    p.glow.prism(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.05, 0.0),
        0.0,
        0.05,
        6,
        Vec3::ONE,
        0.0,
    );
    p.glow.prism(
        Vec3::new(0.0, 0.05, 0.0),
        Vec3::new(0.0, 0.08, 0.0),
        0.05,
        0.0,
        6,
        Vec3::ONE,
        0.0,
    );
}

fn pick(p: &mut Parts) {
    p.matte.prism(
        Vec3::new(-0.3, 0.025, 0.0),
        Vec3::new(0.2, 0.03, 0.0),
        0.022,
        0.024,
        6,
        WOOD,
        0.0,
    );
    p.metal.cuboid(
        Vec3::new(0.2, 0.04, 0.0),
        Vec3::new(0.035, 0.035, 0.035),
        Quat::IDENTITY,
        IRON,
    );
    p.metal.prism(
        Vec3::new(0.2, 0.04, 0.03),
        Vec3::new(0.2, 0.03, 0.2),
        0.03,
        0.0,
        5,
        IRON,
        0.0,
    );
    p.metal.prism(
        Vec3::new(0.2, 0.04, -0.03),
        Vec3::new(0.2, 0.03, -0.17),
        0.028,
        0.006,
        5,
        IRON,
        0.0,
    );
}

fn pouch(p: &mut Parts) {
    p.matte.blob(
        Vec3::new(0.0, 0.07, 0.0),
        Vec3::new(0.1, 0.075, 0.09),
        13,
        0.12,
        LEATHER,
    );
    p.matte.prism(
        Vec3::new(0.0, 0.13, 0.0),
        Vec3::new(0.0, 0.17, 0.0),
        0.04,
        0.05,
        7,
        LEATHER * 0.85,
        0.0,
    );
    p.matte.torus(
        Vec3::new(0.0, 0.135, 0.0),
        Vec3::Y,
        0.042,
        0.008,
        10,
        3,
        TWINE,
    );
}

/// A carved stone doorway two cells high, with an amber lamp on its lintel.
/// The opening spans the model's x axis.
fn door_frame() -> Parts {
    let mut p = Parts {
        accent: Some(Accent::Amber),
        ..default()
    };
    for side in [-1.0f32, 1.0] {
        for k in 0..5 {
            let y = 0.2 + k as f32 * 0.38;
            let jitter = rand(k as u64 + 7, side.to_bits()) * 0.03;
            let shade = if k % 2 == 0 { STONE } else { STONE_DARK };
            p.matte.cuboid(
                Vec3::new(side * (0.47 + jitter), y, 0.0),
                Vec3::new(0.07 + jitter, 0.18, 0.17),
                Quat::from_rotation_y(jitter),
                shade,
            );
        }
    }
    for k in -2..=2 {
        let x = k as f32 * 0.22;
        let (half, color) = if k == 0 {
            (Vec3::new(0.1, 0.13, 0.2), STONE * 1.1)
        } else {
            (
                Vec3::new(0.1, 0.09, 0.18),
                if k % 2 == 0 { STONE } else { STONE_DARK },
            )
        };
        p.matte
            .cuboid(Vec3::new(x, 2.0, 0.0), half, Quat::IDENTITY, color);
    }
    p.metal.cuboid(
        Vec3::new(0.0, 1.84, 0.2),
        Vec3::new(0.015, 0.05, 0.04),
        Quat::IDENTITY,
        IRON,
    );
    p.glow.blob(
        Vec3::new(0.0, 1.77, 0.24),
        Vec3::new(0.035, 0.05, 0.035),
        3,
        0.05,
        Vec3::ONE,
    );
    p
}

/// A plank door with iron bands and a ring pull, hinged at its origin.
fn door_leaf() -> Parts {
    let mut p = Parts::default();
    for k in 0..4 {
        let shade = if k % 2 == 0 { WOOD } else { WOOD * 0.85 };
        p.matte.cuboid(
            Vec3::new(0.11 + k as f32 * 0.205, 0.97, 0.0),
            Vec3::new(0.1, 0.94, 0.035),
            Quat::IDENTITY,
            shade,
        );
    }
    for y in [0.4, 1.5] {
        p.metal.cuboid(
            Vec3::new(0.42, y, 0.0),
            Vec3::new(0.41, 0.035, 0.045),
            Quat::IDENTITY,
            IRON,
        );
        for k in 0..4 {
            p.metal.blob(
                Vec3::new(0.11 + k as f32 * 0.205, y, 0.05),
                Vec3::splat(0.012),
                0,
                0.0,
                IRON,
            );
        }
    }
    p.metal.torus(
        Vec3::new(0.7, 0.95, 0.07),
        Vec3::Z,
        0.045,
        0.008,
        10,
        3,
        BRONZE,
    );
    p.metal.torus(
        Vec3::new(0.7, 0.95, -0.07),
        Vec3::Z,
        0.045,
        0.008,
        10,
        3,
        BRONZE,
    );
    p
}

/// A stone stair spiralling up around a pillar, ringed by a teal rune.
fn stairs_up() -> Parts {
    let mut p = Parts {
        accent: Some(Accent::Teal),
        ..default()
    };
    p.matte.prism(
        Vec3::ZERO,
        Vec3::new(0.0, 2.2, 0.0),
        0.09,
        0.08,
        8,
        STONE_DARK,
        0.0,
    );
    for i in 0..15 {
        let a = i as f32 * 0.46;
        let y = 0.06 + i as f32 * 0.14;
        let shade = if i % 2 == 0 { STONE } else { STONE * 0.9 };
        p.matte.cuboid(
            Vec3::new(a.cos() * 0.27, y, a.sin() * 0.27),
            Vec3::new(0.2, 0.035, 0.085),
            Quat::from_rotation_y(-a),
            shade,
        );
    }
    p.glow.torus(
        Vec3::new(0.0, 0.012, 0.0),
        Vec3::Y,
        0.45,
        0.012,
        24,
        3,
        Vec3::ONE,
    );
    p
}

/// A dark shaft ringed by worked stone, with steps winding down into it.
fn stairs_down() -> Parts {
    let mut p = Parts {
        accent: Some(Accent::Abyss),
        ..default()
    };
    p.matte.prism(
        Vec3::new(0.0, 0.002, 0.0),
        Vec3::new(0.0, 0.01, 0.0),
        0.39,
        0.39,
        20,
        Vec3::splat(0.01),
        0.0,
    );
    p.glow.torus(
        Vec3::new(0.0, 0.014, 0.0),
        Vec3::Y,
        0.36,
        0.012,
        24,
        3,
        Vec3::ONE,
    );
    for k in 0..10 {
        let a = k as f32 * TAU / 10.0;
        p.matte.cuboid(
            Vec3::new(a.cos() * 0.43, 0.04, a.sin() * 0.43),
            Vec3::new(0.06, 0.045, 0.13),
            Quat::from_rotation_y(-a),
            if k % 2 == 0 { STONE } else { STONE_DARK },
        );
    }
    for i in 0..10 {
        let a = i as f32 * 0.55;
        let y = -0.1 - i as f32 * 0.14;
        p.matte.cuboid(
            Vec3::new(a.cos() * 0.2, y, a.sin() * 0.2),
            Vec3::new(0.15, 0.03, 0.07),
            Quat::from_rotation_y(-a),
            STONE_DARK,
        );
    }
    p
}

pub(crate) fn basis(axis: Vec3) -> (Vec3, Vec3) {
    let helper = if axis.y.abs() < 0.9 { Vec3::Y } else { Vec3::X };
    let e1 = axis.cross(helper).normalize();
    (e1, axis.cross(e1))
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

/// Triangles with artist-authored sRGB per-vertex colour.
/// Convert to Bevy's linear PBR colour space only when uploading the mesh.
pub(crate) struct Builder {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    /// Baked sculpt occlusion, applied after decoding the sRGB pigment.
    shades: Vec<f32>,
    xf: Affine3A,
    /// Latitude and longitude steps of a blob.
    pub res: (usize, usize),
    /// Multiplies the sides of every prism, for rounder limbs.
    pub sides_mul: usize,
    /// Soft shading: normals averaged across gentle angles, creases kept.
    pub smooth: bool,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            colors: Vec::new(),
            shades: Vec::new(),
            xf: Affine3A::IDENTITY,
            res: (5, 8),
            sides_mul: 1,
            smooth: false,
        }
    }
}

impl Builder {
    /// Whether nothing has been built yet.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }

    /// The box enclosing everything built so far.
    #[cfg(test)]
    pub(crate) fn bounds(&self) -> (Vec3, Vec3) {
        self.positions.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(low, high), p| {
                let p = Vec3::from_array(*p);
                (low.min(p), high.max(p))
            },
        )
    }

    /// Take over another builder's finished triangles.
    pub(crate) fn absorb(&mut self, other: Builder) {
        self.positions.extend(other.positions);
        self.normals.extend(other.normals);
        self.colors.extend(other.colors);
        self.shades.extend(other.shades);
        self.smooth |= other.smooth;
    }

    /// Average each vertex's normal with those of coincident vertices whose
    /// faces meet at a gentle angle, so curves shade softly and creases stay.
    fn soften(&mut self) {
        let key = |p: [f32; 3]| {
            (
                (p[0] * 2000.0).round() as i32,
                (p[1] * 2000.0).round() as i32,
                (p[2] * 2000.0).round() as i32,
            )
        };
        let mut groups: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
        for (i, p) in self.positions.iter().enumerate() {
            groups.entry(key(*p)).or_default().push(i);
        }
        let faces: Vec<Vec3> = self.normals.iter().map(|n| Vec3::from_array(*n)).collect();
        let crease = 0.55;
        let mut softened: Vec<([u32; 3], [f32; 3])> = Vec::with_capacity(8);
        for members in groups.values() {
            softened.clear();
            for &i in members {
                let own = faces[i];
                let bits = own.to_array().map(f32::to_bits);
                if let Some((_, normal)) = softened.iter().find(|(input, _)| *input == bits) {
                    self.normals[i] = *normal;
                    continue;
                }
                // Repeated triangle corners have identical input normals.
                // Retain the original ordered sum for each distinct input;
                // weighting duplicates would change float accumulation.
                let sum: Vec3 = members
                    .iter()
                    .map(|&j| faces[j])
                    .filter(|n| n.dot(own) > crease)
                    .sum();
                let normal = sum.normalize_or(own).to_array();
                softened.push((bits, normal));
                self.normals[i] = normal;
            }
        }
    }

    /// A triangle with its own vertex normals, for sculpted surfaces.
    pub(crate) fn tri_smooth(&mut self, p: [Vec3; 3], n: [Vec3; 3], colors: [Vec3; 3]) {
        self.tri_smooth_shaded(p, n, colors, [1.0; 3]);
    }

    /// Keep artist-authored pigment separate from a linear occlusion factor.
    pub(crate) fn tri_smooth_shaded(
        &mut self,
        p: [Vec3; 3],
        n: [Vec3; 3],
        colors: [Vec3; 3],
        shades: [f32; 3],
    ) {
        let normal = self.xf.matrix3.inverse().transpose();
        for k in 0..3 {
            let v = self.xf.transform_point3(p[k]);
            let m = (normal * n[k]).normalize_or(Vec3::Y);
            self.positions.push(v.to_array());
            self.normals.push(m.to_array());
            self.colors
                .push([colors[k].x, colors[k].y, colors[k].z, 1.0]);
            self.shades.push(shades[k]);
        }
    }

    pub(crate) fn tri(&mut self, p: [Vec3; 3], color: Vec3) {
        self.tri_colors(p, [color; 3]);
    }

    fn tri_colors(&mut self, p: [Vec3; 3], colors: [Vec3; 3]) {
        let p = p.map(|v| self.xf.transform_point3(v));
        let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
        if n == Vec3::ZERO {
            return;
        }
        for (v, c) in p.into_iter().zip(colors) {
            self.positions.push(v.to_array());
            self.normals.push(n.to_array());
            self.colors.push([c.x, c.y, c.z, 1.0]);
            self.shades.push(1.0);
        }
    }

    /// A triangle wound to face away from a point inside its solid.
    pub(crate) fn tri_out(&mut self, mut p: [Vec3; 3], inside: Vec3, color: Vec3) {
        let n = (p[1] - p[0]).cross(p[2] - p[0]);
        let centroid = (p[0] + p[1] + p[2]) / 3.0;
        if n.dot(centroid - inside) < 0.0 {
            p.swap(1, 2);
        }
        self.tri(p, color);
    }

    pub(crate) fn quad_out(&mut self, q: [Vec3; 4], inside: Vec3, color: Vec3) {
        self.tri_out([q[0], q[1], q[2]], inside, color);
        self.tri_out([q[0], q[2], q[3]], inside, color);
    }

    /// A many-sided prism (or cone, at a zero radius) from a to b.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prism(
        &mut self,
        a: Vec3,
        b: Vec3,
        r0: f32,
        r1: f32,
        sides: usize,
        color: Vec3,
        phase: f32,
    ) {
        let sides = (sides * self.sides_mul).min(32);
        let axis = (b - a).normalize_or_zero();
        if axis == Vec3::ZERO {
            return;
        }
        let (e1, e2) = basis(axis);
        let ring = |c: Vec3, r: f32| -> Vec<Vec3> {
            (0..sides)
                .map(|k| {
                    let t = (k as f32 + phase) * TAU / sides as f32;
                    c + (e1 * t.cos() + e2 * t.sin()) * r
                })
                .collect()
        };
        let (ra, rb) = (ring(a, r0), ring(b, r1));
        let mid = (a + b) * 0.5;
        for k in 0..sides {
            let n = (k + 1) % sides;
            if r0 > 0.0 || r1 > 0.0 {
                if r0 == 0.0 {
                    self.tri_out([a, rb[k], rb[n]], mid, color);
                } else if r1 == 0.0 {
                    self.tri_out([ra[k], ra[n], b], mid, color);
                } else {
                    self.quad_out([ra[k], ra[n], rb[n], rb[k]], mid, color);
                }
            }
            if r0 > 0.0 {
                self.tri_out([a, ra[k], ra[n]], mid, color);
            }
            if r1 > 0.0 {
                self.tri_out([b, rb[k], rb[n]], mid, color);
            }
        }
    }

    pub(crate) fn cuboid(&mut self, center: Vec3, half: Vec3, rot: Quat, color: Vec3) {
        let c = |x: f32, y: f32, z: f32| center + rot * (half * Vec3::new(x, y, z));
        let faces = [
            [
                c(1., -1., -1.),
                c(1., 1., -1.),
                c(1., 1., 1.),
                c(1., -1., 1.),
            ],
            [
                c(-1., -1., -1.),
                c(-1., -1., 1.),
                c(-1., 1., 1.),
                c(-1., 1., -1.),
            ],
            [
                c(-1., 1., -1.),
                c(-1., 1., 1.),
                c(1., 1., 1.),
                c(1., 1., -1.),
            ],
            [
                c(-1., -1., -1.),
                c(1., -1., -1.),
                c(1., -1., 1.),
                c(-1., -1., 1.),
            ],
            [
                c(-1., -1., 1.),
                c(1., -1., 1.),
                c(1., 1., 1.),
                c(-1., 1., 1.),
            ],
            [
                c(-1., -1., -1.),
                c(-1., 1., -1.),
                c(1., 1., -1.),
                c(1., -1., -1.),
            ],
        ];
        for q in faces {
            self.quad_out(q, center, color);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn torus(
        &mut self,
        center: Vec3,
        axis: Vec3,
        major: f32,
        minor: f32,
        segs: usize,
        sides: usize,
        color: Vec3,
    ) {
        let (e1, e2) = basis(axis);
        let point = |i: usize, j: usize| {
            let u = i as f32 * TAU / segs as f32;
            let v = j as f32 * TAU / sides as f32;
            let radial = e1 * u.cos() + e2 * u.sin();
            center + radial * major + (radial * v.cos() + axis * v.sin()) * minor
        };
        for i in 0..segs {
            let u = (i as f32 + 0.5) * TAU / segs as f32;
            let inside = center + (e1 * u.cos() + e2 * u.sin()) * major;
            for j in 0..sides {
                self.quad_out(
                    [
                        point(i, j),
                        point(i + 1, j),
                        point(i + 1, j + 1),
                        point(i, j + 1),
                    ],
                    inside,
                    color,
                );
            }
        }
    }

    /// A slab of stone or plate: a box with softly rounded edges and corners
    /// (a superellipsoid), flat across its faces. `chip` roughens the
    /// surface for weathered stone; `paint` gets the outward direction and
    /// the point, as for a painted blob.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn slab(
        &mut self,
        center: Vec3,
        half: Vec3,
        rot: Quat,
        key: u64,
        chip: f32,
        paint: &dyn Fn(Vec3, Vec3) -> Vec3,
    ) {
        let (lat, lon) = (12usize, 16usize);
        // Smaller exponents square the shape off; 0.22 leaves a soft bevel.
        let squareness = 0.22;
        let shape = |v: f32| v.signum() * v.abs().powf(squareness);
        let dir = |i: usize, j: usize| {
            let t = i as f32 / lat as f32 * std::f32::consts::PI;
            let p = (j % lon) as f32 / lon as f32 * TAU + TAU / (lon as f32 * 2.0);
            Vec3::new(t.sin() * p.cos(), t.cos(), t.sin() * p.sin())
        };
        let point = |i: usize, j: usize| {
            let d = dir(i, j);
            let wear = if i == 0 || i == lat {
                1.0
            } else {
                1.0 - rand(key, (i * lon + j % lon) as u32) * chip
            };
            center + rot * (Vec3::new(shape(d.x), shape(d.y), shape(d.z)) * half * wear)
        };
        let face = |b: &mut Self, idx: [(usize, usize); 3]| {
            let mut ps = idx.map(|(i, j)| point(i, j));
            let mut cs = idx.map(|(i, j)| paint(rot * dir(i, j), point(i, j)));
            let n = (ps[1] - ps[0]).cross(ps[2] - ps[0]);
            let centroid = (ps[0] + ps[1] + ps[2]) / 3.0;
            if n.dot(centroid - center) < 0.0 {
                ps.swap(1, 2);
                cs.swap(1, 2);
            }
            b.tri_colors(ps, cs);
        };
        for i in 0..lat {
            for j in 0..lon {
                let q = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                if i == 0 {
                    face(self, [q[0], q[1], q[2]]);
                } else if i == lat - 1 {
                    face(self, [q[0], q[1], q[3]]);
                } else {
                    face(self, [q[0], q[1], q[2]]);
                    face(self, [q[0], q[2], q[3]]);
                }
            }
        }
    }

    /// A lumpy low-poly ellipsoid.
    pub(crate) fn blob(&mut self, center: Vec3, radii: Vec3, key: u64, jitter: f32, color: Vec3) {
        self.blob_painted(center, radii, key, jitter, &|_, _| color);
    }

    /// A blob whose colour is painted per vertex: `paint` gets the unit
    /// direction from the centre and the point, and returns the colour.
    pub(crate) fn blob_painted(
        &mut self,
        center: Vec3,
        radii: Vec3,
        key: u64,
        jitter: f32,
        paint: &dyn Fn(Vec3, Vec3) -> Vec3,
    ) {
        let (lat, lon) = self.res;
        let dir = |i: usize, j: usize| {
            let t = i as f32 / lat as f32 * std::f32::consts::PI;
            let p = (j % lon) as f32 / lon as f32 * TAU;
            Vec3::new(t.sin() * p.cos(), t.cos(), t.sin() * p.sin())
        };
        let point = |i: usize, j: usize| {
            let wobble = if i == 0 || i == lat {
                1.0
            } else {
                1.0 + (rand(key, (i * lon + j % lon) as u32) - 0.5) * jitter
            };
            center + dir(i, j) * radii * wobble
        };
        let face = |b: &mut Self, idx: [(usize, usize); 3]| {
            let mut ps = idx.map(|(i, j)| point(i, j));
            let mut cs = idx.map(|(i, j)| paint(dir(i, j), point(i, j)));
            let n = (ps[1] - ps[0]).cross(ps[2] - ps[0]);
            let centroid = (ps[0] + ps[1] + ps[2]) / 3.0;
            if n.dot(centroid - center) < 0.0 {
                ps.swap(1, 2);
                cs.swap(1, 2);
            }
            b.tri_colors(ps, cs);
        };
        for i in 0..lat {
            for j in 0..lon {
                let q = [(i, j), (i + 1, j), (i + 1, j + 1), (i, j + 1)];
                if i == 0 {
                    face(self, [q[0], q[1], q[2]]);
                } else if i == lat - 1 {
                    face(self, [q[0], q[1], q[3]]);
                } else {
                    face(self, [q[0], q[1], q[2]]);
                    face(self, [q[0], q[2], q[3]]);
                }
            }
        }
    }

    /// A continuous tube swept along a path: each ring has its own
    /// elliptical radii (across x, across z), so necks, limbs and tails
    /// taper smoothly. `paint` gets the ring's outward direction, the point
    /// and how far along the path it lies (0 to 1).
    pub(crate) fn tube(
        &mut self,
        path: &[Vec3],
        radii: &[Vec2],
        sides: usize,
        paint: &dyn Fn(Vec3, Vec3, f32) -> Vec3,
    ) {
        self.tube_framed(path, radii, sides, paint, None);
    }

    /// A tube whose first radius (across) starts along `across` rather than
    /// the world's x axis, for flat shapes that must lie a particular way.
    pub(crate) fn tube_framed(
        &mut self,
        path: &[Vec3],
        radii: &[Vec2],
        sides: usize,
        paint: &dyn Fn(Vec3, Vec3, f32) -> Vec3,
        across: Option<Vec3>,
    ) {
        if path.len() < 2 {
            return;
        }
        // Resample as a Catmull-Rom spline so curves are smooth and painted
        // patterns have rings enough to show.
        let sub = if self.smooth { 8 } else { 1 };
        let at = |i: isize| path[i.clamp(0, path.len() as isize - 1) as usize];
        let mut fine_path = Vec::new();
        let mut fine_radii = Vec::new();
        for i in 0..path.len() - 1 {
            for k in 0..sub {
                let t = k as f32 / sub as f32;
                let (p0, p1, p2, p3) = (
                    at(i as isize - 1),
                    at(i as isize),
                    at(i as isize + 1),
                    at(i as isize + 2),
                );
                let t2 = t * t;
                let t3 = t2 * t;
                fine_path.push(
                    0.5 * ((2.0 * p1)
                        + (p2 - p0) * t
                        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                        + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3),
                );
                fine_radii.push(radii[i].lerp(radii[i + 1], t));
            }
        }
        fine_path.push(*path.last().unwrap());
        fine_radii.push(*radii.last().unwrap());
        let (path, radii) = (&fine_path[..], &fine_radii[..]);
        let n = path.len();
        let sides = (sides * self.sides_mul).min(32);
        let tangent =
            |i: usize| (path[(i + 1).min(n - 1)] - path[i.saturating_sub(1)]).normalize_or(Vec3::Y);
        // Frames carried along the path so the rings never twist.
        let mut frames = Vec::with_capacity(n);
        let t0 = tangent(0);
        let start = across.unwrap_or(Vec3::X);
        let mut e1 = (start - t0 * start.dot(t0)).normalize_or_zero();
        if e1 == Vec3::ZERO {
            e1 = basis(t0).0;
        }
        for i in 0..n {
            let t = tangent(i);
            e1 = (e1 - t * e1.dot(t)).normalize_or(basis(t).0);
            frames.push((e1, t.cross(e1)));
        }
        let length: Vec<f32> = path
            .windows(2)
            .scan(0.0, |acc, w| {
                *acc += (w[1] - w[0]).length();
                Some(*acc)
            })
            .collect();
        let total = length.last().copied().unwrap_or(1.0).max(1e-4);
        let along = |i: usize| if i == 0 { 0.0 } else { length[i - 1] / total };
        let ring: Vec<Vec<(Vec3, Vec3)>> = (0..n)
            .map(|i| {
                let (a, b) = frames[i];
                (0..sides)
                    .map(|k| {
                        let ang = k as f32 * TAU / sides as f32;
                        let dir = a * ang.cos() + b * ang.sin();
                        let at = path[i] + a * ang.cos() * radii[i].x + b * ang.sin() * radii[i].y;
                        (at, dir)
                    })
                    .collect()
            })
            .collect();
        for i in 0..n - 1 {
            let inside = (path[i] + path[i + 1]) * 0.5;
            for k in 0..sides {
                let m = (k + 1) % sides;
                let quad = [(i, k), (i + 1, k), (i + 1, m), (i, m)];
                for tri in [[quad[0], quad[1], quad[2]], [quad[0], quad[2], quad[3]]] {
                    let mut ps = tri.map(|(r, c)| ring[r][c].0);
                    let mut cs = tri.map(|(r, c)| paint(ring[r][c].1, ring[r][c].0, along(r)));
                    let normal = (ps[1] - ps[0]).cross(ps[2] - ps[0]);
                    if normal.dot((ps[0] + ps[1] + ps[2]) / 3.0 - inside) < 0.0 {
                        ps.swap(1, 2);
                        cs.swap(1, 2);
                    }
                    self.tri_colors(ps, cs);
                }
            }
        }
        // Close the ends.
        for (i, outward) in [(0, -tangent(0)), (n - 1, tangent(n - 1))] {
            if radii[i].x <= 0.0 {
                continue;
            }
            let center = path[i];
            let color = paint(outward, center, along(i));
            for k in 0..sides {
                let m = (k + 1) % sides;
                let mut ps = [center, ring[i][k].0, ring[i][m].0];
                if (ps[1] - ps[0]).cross(ps[2] - ps[0]).dot(outward) < 0.0 {
                    ps.swap(1, 2);
                }
                self.tri_colors(ps, [color; 3]);
            }
        }
    }

    /// A flat outline given thickness, in the plane of ex and ey.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn extrude(
        &mut self,
        outline: &[Vec2],
        origin: Vec3,
        ex: Vec3,
        ey: Vec3,
        thickness: f32,
        color: Vec3,
    ) {
        let mut outline = outline.to_vec();
        let area: f32 = outline
            .iter()
            .zip(outline.iter().cycle().skip(1))
            .map(|(a, b)| a.x * b.y - b.x * a.y)
            .sum();
        if area < 0.0 {
            outline.reverse();
        }
        let n = ex.cross(ey).normalize();
        let center2 = outline.iter().copied().sum::<Vec2>() / outline.len() as f32;
        let at = |v: Vec2, z: f32| origin + ex * v.x + ey * v.y + n * z;
        let h = thickness * 0.5;
        let count = outline.len();
        for k in 0..count {
            let (a, b) = (outline[k], outline[(k + 1) % count]);
            self.tri([at(center2, h), at(a, h), at(b, h)], color);
            self.tri([at(center2, -h), at(b, -h), at(a, -h)], color);
            let edge = b - a;
            let outward = ex * edge.y - ey * edge.x;
            let q = [at(a, h), at(a, -h), at(b, -h), at(b, h)];
            let inside = (q[0] + q[1] + q[2] + q[3]) * 0.25 - outward;
            self.quad_out(q, inside, color);
        }
    }

    fn finish(mut self) -> Option<Mesh> {
        if self.positions.is_empty() {
            return None;
        }
        let started = profile_builds().then(std::time::Instant::now);
        if self.smooth {
            self.soften();
        }
        let soften_ms = started.map(|time| time.elapsed().as_secs_f64() * 1000.0);
        // Palette constants and their painted variations are sRGB, just like
        // Color::srgb on StandardMaterial. Vertex colours bypass that material
        // conversion, so decode them here before linear lighting/interpolation.
        // Eye glints use overbright white (1.5/1.6): preserve the linear excess
        // above display white. Apply sculpt occlusion in linear space, rather
        // than darkening pigment before gamma decoding. Alpha and material
        // emissive HDR stay unchanged.
        for (color, shade) in self.colors.iter_mut().zip(&self.shades) {
            let rgb = Vec3::new(color[0], color[1], color[2]);
            let display = rgb.min(Vec3::ONE);
            let excess = rgb - display;
            let linear = Color::srgb(display.x, display.y, display.z).to_linear();
            *color = [
                (linear.red + excess.x) * shade,
                (linear.green + excess.y) * shade,
                (linear.blue + excess.z) * shade,
                color[3],
            ];
        }
        let color_done_ms = started.map(|time| time.elapsed().as_secs_f64() * 1000.0);
        // Share only complete, bit-identical final vertices. Position alone
        // cannot identify a vertex: creases and painted edges need distinct
        // normals or colours even when they occupy the same point.
        let count = self.positions.len();
        let mut unique: HashMap<[u32; 10], u32> = HashMap::with_capacity(count / 3);
        let mut indices = Vec::with_capacity(count);
        let mut unique_count = 0usize;
        for i in 0..count {
            let position = self.positions[i];
            let normal = self.normals[i];
            let color = self.colors[i];
            let bits = [
                position[0].to_bits(),
                position[1].to_bits(),
                position[2].to_bits(),
                normal[0].to_bits(),
                normal[1].to_bits(),
                normal[2].to_bits(),
                color[0].to_bits(),
                color[1].to_bits(),
                color[2].to_bits(),
                color[3].to_bits(),
            ];
            let index = *unique.entry(bits).or_insert_with(|| {
                let index = unique_count as u32;
                // First occurrences retain their original order. Compact in
                // place; this slot never reaches ahead of the read cursor.
                self.positions[unique_count] = position;
                self.normals[unique_count] = normal;
                self.colors[unique_count] = color;
                unique_count += 1;
                index
            });
            indices.push(index);
        }
        drop(unique);
        self.positions.truncate(unique_count);
        self.normals.truncate(unique_count);
        self.colors.truncate(unique_count);
        self.positions.shrink_to_fit();
        self.normals.shrink_to_fit();
        self.colors.shrink_to_fit();
        let reuse_ms = started
            .map(|time| time.elapsed().as_secs_f64() * 1000.0 - color_done_ms.unwrap_or(0.0));
        let mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::RENDER_WORLD,
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, self.normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(indices));
        if let Some(started) = started {
            let total_ms = started.elapsed().as_secs_f64() * 1000.0;
            eprintln!(
                "3d_mesh_finish vertices={count} unique_vertices={unique_count} smooth={} ms={total_ms:.3} soften_ms={:.3} reuse_ms={:.3} color_mesh_ms={:.3}",
                self.smooth,
                soften_ms.unwrap_or(0.0),
                reuse_ms.unwrap_or(0.0),
                total_ms - soften_ms.unwrap_or(0.0) - reuse_ms.unwrap_or(0.0),
            );
        }
        Some(mesh)
    }
}
