//! A continuous, fitted caster robe driven by the final supported leg pose.
//!
//! One modest mesh belongs to each wearer; materials remain shared. Neutral
//! positions use the original lower elliptical surface and a fitted waist.
//! Local body clearance deforms connected rows, without rigid panel seams.
use super::anatomy::{BootShape, HumanoidBody, LimbShape, FOOT_SCALE, HIP_SCALE, LIMB_SCALE};
use super::*;
use bevy::render::{
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    primitives::Aabb,
    render_asset::RenderAssetUsages,
};

const COLUMNS: usize = 48;
const LOWER_ROWS: usize = 16;
const YOKE_ROWS: usize = 2;
const ROWS: usize = LOWER_ROWS + YOKE_ROWS;
const GRID: usize = (ROWS + 1) * COLUMNS;
const BODY_VERTICES: usize = GRID * 2 + COLUMNS * 2;
const SASH_ROWS: usize = 32;
const SASH_SIDES: usize = 8;
const SASH_VERTICES: usize = (SASH_ROWS + 1) * SASH_SIDES + 2;
const THICKNESS: f32 = 0.007;
const TOP: f32 = 0.04;
const WAIST_TOP: f32 = 0.092;
const HEM: f32 = -0.84;
const ROW_HEIGHT: f32 = (TOP - HEM) / LOWER_ROWS as f32;
const LOWER_ANCHOR: usize = YOKE_ROWS + LOWER_ROWS / 2;
// Contact may ease one knee row above the hem-shaping band.
const CONTACT_ANCHOR: usize = LOWER_ANCHOR - 1;
const MAX_HEM_RISE: f32 = ROW_HEIGHT * 2.0;
const MAX_CONTACT_SHIFT: f32 = ROW_HEIGHT * 2.0;
const CORE_PIECES: usize = 3;
const CUFF_PIECES: usize = 12;
const CONTACT_PROXIES: usize = 6 + 4 * (CORE_PIECES + 1) + 2 * (CORE_PIECES + CUFF_PIECES);
const POSE_JOINTS: [J; 8] = [
    J::Lift,
    J::Hips,
    J::LegL,
    J::ShinL,
    J::FootL,
    J::LegR,
    J::ShinR,
    J::FootR,
];

#[cfg(test)]
mod art_contract_tests {
    use super::*;

    #[test]
    fn art_contract_motion_changes_only_the_wearers_owned_cloth_mesh() {
        let look = Look::with_choice(
            CreatureModel::Mage,
            42,
            Some(Variant {
                woman: true,
                build: 2,
                ..default()
            }),
        );
        let body = HumanoidBody::resolve(&look);
        let boot = BootShape::fitted(body.build, -0.24);
        let mut materials = Assets::<StandardMaterial>::default();
        let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
        let mut meshes = Assets::<Mesh>::default();
        let mut world = World::new();
        let hips = world.spawn_empty().id();
        let other_hips = world.spawn_empty().id();
        let mut queue = bevy::ecs::world::CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut first = spawn(
            &mut commands,
            &mut library,
            &mut meshes,
            hips,
            &look,
            body,
            boot,
            &[],
        );
        let second = spawn(
            &mut commands,
            &mut library,
            &mut meshes,
            other_hips,
            &look,
            body,
            boot,
            &[],
        );
        queue.apply(&mut world);
        assert_ne!(
            first.mesh.id(),
            second.mesh.id(),
            "cached appearances must not share mutable cloth"
        );
        let positions = |mesh: &Mesh| match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
            VertexAttributeValues::Float32x3(values) => values.clone(),
            _ => panic!("cloth must expose positions"),
        };
        let untouched = positions(meshes.get(&second.mesh).unwrap());
        let mut rest = [Vec3::ZERO; JOINTS];
        for (joint, offset) in [
            (J::Hips, body.hips),
            (J::LegL, body.leg_root(1.0)),
            (J::LegR, body.leg_root(-1.0)),
            (J::ShinL, body.knee()),
            (J::ShinR, body.knee()),
            (J::FootL, body.ankle()),
            (J::FootR, body.ankle()),
        ] {
            rest[joint as usize] = offset;
        }
        let mut rotations = [Quat::IDENTITY; JOINTS];
        let offsets = [Vec3::ZERO; JOINTS];
        place(&mut first, &rest, &rotations, &offsets, &mut meshes);
        let neutral = positions(meshes.get(&first.mesh).unwrap());
        rotations[J::LegL as usize] = Quat::from_rotation_x(0.8);
        place(&mut first, &rest, &rotations, &offsets, &mut meshes);
        let moved = positions(meshes.get(&first.mesh).unwrap());
        assert_ne!(
            neutral, moved,
            "the final limb pose must actually deform the owned cloth"
        );
        assert_eq!(
            neutral.len(),
            moved.len(),
            "motion must preserve mesh topology"
        );
        assert!(moved
            .iter()
            .all(|point| Vec3::from_array(*point).is_finite()));
        assert_eq!(untouched, positions(meshes.get(&second.mesh).unwrap()));
    }

    #[test]
    fn art_contract_neutral_cloth_stays_finite_closed_and_fitted_across_profiles() {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Mage,
                    42,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                for row in 0..=ROWS {
                    assert!(
                        neutral(&look, body, row, 0).distance(neutral(&look, body, row, COLUMNS))
                            < 0.000001
                    );
                    for column in 0..COLUMNS {
                        let point = neutral(&look, body, row, column);
                        assert!(point.is_finite());
                        assert!(point.with_y(0.0).length() > 0.05);
                    }
                }
                let waist = neutral(&look, body, 0, COLUMNS / 4).x.abs();
                let hem = neutral(&look, body, ROWS, COLUMNS / 4).x.abs();
                assert!(
                    hem > waist,
                    "free hem must not collapse into a body coating"
                );
                let heights: Vec<f32> = (0..COLUMNS)
                    .map(|column| neutral(&look, body, ROWS, column).y)
                    .collect();
                let range = heights.iter().copied().fold(f32::NEG_INFINITY, f32::max)
                    - heights.iter().copied().fold(f32::INFINITY, f32::min);
                assert!(range > 0.01, "the robe's hem hangs free, not level");
                assert!(
                    range < MAX_HEM_RISE,
                    "its unevenness stays within the hem band"
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Limb {
    origin: Vec3,
    inverse: Quat,
    shape: LimbShape,
    boot: Option<BootShape>,
}

#[derive(Clone, Copy)]
struct Foot {
    origin: Vec3,
    inverse: Quat,
    shapes: [FootShape; 3],
    min_y: f32,
    max_y: f32,
}

#[derive(Clone, Copy)]
struct FootShape {
    center: Vec3,
    radii: Vec3,
    sweep_rotation: Quat,
    sweep_half: f32,
}

/// A sphere or constant-radius capsule in normalized coordinates. Its affine
/// image supplies both an exact support function and a cheap cached world box.
/// These are garment-local proxies, not a general collision representation.
#[derive(Clone, Copy)]
struct BodyProxy {
    origin: Vec3,
    inverse: Quat,
    scale: Vec3,
    end: Vec3,
    min: Vec3,
    max: Vec3,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct ContactReport {
    boundary: usize,
    limited: usize,
    remaining: usize,
}

/// A garment trim recipe, in the hip frame before mesh-only proportions.
/// The supporting garment supplies depth; the recipe retains its cut and color.
pub(super) struct SashTail {
    pub path: [Vec2; 3],
    pub radii: [Vec2; 3],
    pub color: Vec3,
}

struct SurfaceBinding {
    triangle: [usize; 3],
    weights: Vec3,
    lift: f32,
    clearance: f32,
}

struct SurfaceTrim {
    bindings: Box<[SurfaceBinding]>,
    points: Box<[Vec3]>,
    normals: Box<[Vec3]>,
}

pub(super) struct RobeRig {
    mesh: Handle<Mesh>,
    shapes: [LimbShape; 2],
    boot: BootShape,
    feet: [(Vec3, Vec3); 3],
    rest: Box<[Vec3; GRID]>,
    directions: Box<[Vec3; GRID]>,
    points: Box<[Vec3; GRID]>,
    candidate: Box<[Vec3; GRID]>,
    normals: Box<[Vec3; GRID]>,
    displacement: Box<[f32; GRID]>,
    smooth: Box<[f32; GRID]>,
    neutral_lengths: Option<[f32; COLUMNS]>,
    contacts: ContactReport,
    trim: SurfaceTrim,
    last_rot: [Quat; 8],
    last_off: [Vec3; 8],
    dirty: bool,
}

fn index(row: usize, column: usize) -> usize {
    row * COLUMNS + column % COLUMNS
}
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(super) fn belt_radii(build: f32, waist: f32) -> Vec2 {
    Vec2::new(0.152 * waist + 0.018, 0.121) * build
}

fn neutral(look: &Look, body: HumanoidBody, row: usize, column: usize) -> Vec3 {
    let (build, hips, waist) = (body.build, body.hips_width, body.waist);
    let t = row.saturating_sub(YOKE_ROWS) as f32 / LOWER_ROWS as f32;
    let angle = column as f32 / COLUMNS as f32 * TAU;
    let top = Vec2::new(0.18 * build * hips, 0.135 * build);
    let bottom = Vec2::new(0.2458 * build * hips, 0.2458 * 0.74 * build);
    let r = if row < YOKE_ROWS {
        (belt_radii(build, waist) - Vec2::splat(0.003)).lerp(top, row as f32 / YOKE_ROWS as f32)
    } else {
        top.lerp(bottom, ease(t))
    };
    let phase = rand(u64::from(look.v.shade) + 73, 0) * 0.4;
    // A free hem: folds deepen toward the edge and the cloth hangs lower
    // where each fold swings out, as on the draped skirts.
    let wave = (angle * 12.0 + phase).cos();
    let fold = wave * (0.007 * t.powf(1.2) + 0.01 * t.powi(3));
    let drop = 0.013 * (0.5 + 0.5 * wave) * ease((t - 0.6) / 0.4);
    let y = if row < YOKE_ROWS {
        WAIST_TOP + (TOP - WAIST_TOP) * row as f32 / YOKE_ROWS as f32
    } else {
        TOP + (HEM - TOP) * t - drop
    };
    let mut p = Vec3::new(angle.sin() * (r.x + fold), y, angle.cos() * (r.y + fold));
    if row > 0 && row <= YOKE_ROWS {
        // The upper pelvis is fuller than the waistband. Its authored blob
        // has at most 2.5% radial jitter; clearance also covers chord sag
        // between these short yoke rows. Only the belt ring stays fixed.
        let direction = p.with_y(0.0).normalize_or(Vec3::Z);
        let body = ellipsoid_far(
            Vec3::Y * y,
            direction,
            Vec3::ZERO,
            body.pelvis_radii() * 1.025,
        );
        p += direction * (body + 0.008 - p.with_y(0.0).length()).max(0.0);
    }
    // Hips meshes receive these factors separately. The owned robe is a direct
    // child of the unscaled hip joint, so apply them once to its neutral shape.
    p * HIP_SCALE
}

fn shell_offset(column: usize) -> Vec3 {
    let angle = column as f32 / COLUMNS as f32 * TAU;
    Vec3::new(
        angle.sin() * THICKNESS * HIP_SCALE.x,
        0.0,
        angle.cos() * THICKNESS * HIP_SCALE.z,
    )
}

/// Farthest ray crossing of the same tapered capsule used by limb sculpting.
/// The ray need not be normalized after conversion into mesh-local space.
fn capsule_far(origin: Vec3, direction: Vec3, length: f32, top: f32, bottom: f32) -> f32 {
    let slope = (top - bottom) / length;
    let radius = top + slope * origin.y;
    let change = slope * direction.y;
    let qa = direction.x * direction.x + direction.z * direction.z - change * change;
    let qb = origin.x * direction.x + origin.z * direction.z - radius * change;
    let qc = origin.x * origin.x + origin.z * origin.z - radius * radius;
    let mut far = f32::NEG_INFINITY;
    let disc = qb * qb - qa * qc;
    if qa.abs() > 0.000001 && disc >= 0.0 {
        let root = disc.sqrt();
        for t in [(-qb - root) / qa, (-qb + root) / qa] {
            let y = origin.y + direction.y * t;
            if t >= 0.0 && y <= 0.0 && y >= -length {
                far = far.max(t);
            }
        }
    } else if qa.abs() <= 0.000001 && qb.abs() > 0.000001 {
        let t = -qc / (2.0 * qb);
        let y = origin.y + direction.y * t;
        if t >= 0.0 && y <= 0.0 && y >= -length {
            far = far.max(t);
        }
    }
    let dd = direction.length_squared();
    for (center, r, upper) in [(Vec3::ZERO, top, true), (Vec3::Y * -length, bottom, false)] {
        let offset = origin - center;
        let b = offset.dot(direction);
        let h = b * b - dd * (offset.length_squared() - r * r);
        if h >= 0.0 {
            for t in [(-b - h.sqrt()) / dd, (-b + h.sqrt()) / dd] {
                let y = origin.y + direction.y * t;
                if t >= 0.0 && if upper { y >= 0.0 } else { y <= -length } {
                    far = far.max(t);
                }
            }
        }
    }
    far
}

fn ellipsoid_far(origin: Vec3, direction: Vec3, center: Vec3, radii: Vec3) -> f32 {
    let o = (origin - center) / radii;
    let d = direction / radii;
    let a = d.length_squared();
    let b = o.dot(d);
    let h = b * b - a * (o.length_squared() - 1.0);
    if h >= 0.0 {
        (-b + h.sqrt()) / a
    } else {
        f32::NEG_INFINITY
    }
}

/// A thin cylinder bounds the actual torus cuff without extending its radius
/// along the entire calf or boot. All coordinates remain mesh-local here.
fn cuff_far(origin: Vec3, direction: Vec3, boot: BootShape, half_cell: Vec3) -> f32 {
    let o = origin - Vec3::Y * boot.top;
    let (ring, tube) = (boot.cuff_radius, boot.cuff_thickness);
    // A cuff may fall between cloth rows. Sweep its bound over the row's
    // half-cell in shin-local space, including the radial projection when
    // the shin tilts. This remains local to the thin cuff feature.
    let radius = ring + tube + half_cell.with_y(0.0).length();
    let half_height = tube + half_cell.y.abs();
    let a = direction.x * direction.x + direction.z * direction.z;
    let b = o.x * direction.x + o.z * direction.z;
    let c = o.x * o.x + o.z * o.z - radius * radius;
    let mut far = f32::NEG_INFINITY;
    let h = b * b - a * c;
    if a > 0.000001 && h >= 0.0 {
        let root = h.sqrt();
        for t in [(-b - root) / a, (-b + root) / a] {
            if t >= 0.0 && (o.y + direction.y * t).abs() <= half_height {
                far = far.max(t);
            }
        }
    }
    if direction.y.abs() > 0.000001 {
        for y in [-half_height, half_height] {
            let t = (y - o.y) / direction.y;
            let p = o + direction * t;
            if t >= 0.0 && p.x * p.x + p.z * p.z <= radius * radius {
                far = far.max(t);
            }
        }
    }
    far
}

fn limb_far(limb: Limb, origin: Vec3, direction: Vec3) -> f32 {
    // Mesh-only Storybook proportions do not move the authored bone centers.
    let scale = LIMB_SCALE;
    let o = (limb.inverse * (origin - limb.origin)) / scale;
    let d = (limb.inverse * direction) / scale;
    let s = limb.shape;
    // The local smooth union can add at most .0075 at its capsule/belly join.
    // The clearance acts locally; it never expands an entire ring or garment.
    let margin = 0.0075;
    let mut far = capsule_far(
        o,
        d,
        s.length,
        s.radius_top + margin,
        s.radius_bottom + margin,
    )
    .max(ellipsoid_far(
        o,
        d,
        s.belly_center,
        s.belly + Vec3::splat(margin),
    ));
    if let Some(boot) = limb.boot {
        let half_cell = (limb.inverse * (Vec3::Y * ROW_HEIGHT * 0.5)) / scale;
        far = far.max(cuff_far(o, d, boot, half_cell));
        far = far.max(capsule_far(
            o - Vec3::Y * boot.top,
            d,
            s.length + boot.top,
            boot.sample_radius_top,
            boot.sample_radius_bottom,
        ));
    }
    far
}

fn envelope(limbs: &[Limb; 4], origin: Vec3, direction: Vec3) -> f32 {
    limbs
        .iter()
        .map(|limb| limb_far(*limb, origin, direction))
        .fold(f32::NEG_INFINITY, f32::max)
}

fn foot_far(foot: Foot, origin: Vec3, direction: Vec3) -> f32 {
    if direction.y.abs() < 0.000001 && (origin.y < foot.min_y || origin.y > foot.max_y) {
        return f32::NEG_INFINITY;
    }
    let scale = FOOT_SCALE;
    let o = (foot.inverse * (origin - foot.origin)) / scale;
    let d = (foot.inverse * direction) / scale;
    foot.shapes.iter().fold(f32::NEG_INFINITY, |far, shape| {
        // In ellipsoid space, sweeping over a vertical cloth half-cell is
        // exactly a unit capsule. Even the thin sole cannot slip between rows.
        let o =
            shape.sweep_rotation * ((o - shape.center) / shape.radii) - Vec3::Y * shape.sweep_half;
        let d = shape.sweep_rotation * (d / shape.radii);
        far.max(capsule_far(o, d, shape.sweep_half * 2.0, 1.0, 1.0))
    })
}

impl BodyProxy {
    fn new(origin: Vec3, rotation: Quat, scale: Vec3, end: Vec3) -> Self {
        let inverse = rotation.inverse();
        let other = origin + rotation * (end * scale);
        let extent = Vec3::new(
            ((inverse * Vec3::X) * scale).length(),
            ((inverse * Vec3::Y) * scale).length(),
            ((inverse * Vec3::Z) * scale).length(),
        );
        Self {
            origin,
            inverse,
            scale,
            end,
            min: origin.min(other) - extent,
            max: origin.max(other) + extent,
        }
    }

    fn ellipsoid(limb: Limb, center: Vec3, radii: Vec3) -> Self {
        let rotation = limb.inverse.inverse();
        let proportions = LIMB_SCALE;
        Self::new(
            limb.origin + rotation * (center * proportions),
            rotation,
            radii * proportions,
            Vec3::ZERO,
        )
    }

    fn capsule(limb: Limb, a: Vec3, b: Vec3, radius: f32) -> Self {
        let rotation = limb.inverse.inverse();
        let proportions = LIMB_SCALE;
        Self::new(
            limb.origin + rotation * (a * proportions),
            rotation,
            proportions * radius,
            (b - a) / radius,
        )
    }

    fn overlaps(self, min: Vec3, max: Vec3) -> bool {
        min.cmple(self.max + Vec3::splat(0.003)).all()
            && max.cmpge(self.min - Vec3::splat(0.003)).all()
    }

    fn support(self, normal: Vec3) -> f32 {
        let local = self.inverse * normal;
        self.origin.dot(normal)
            + (local * self.scale).length()
            + (local * self.scale).dot(self.end).max(0.0)
    }

    fn contact_normal(self, triangle: [Vec3; 3]) -> Option<Vec3> {
        let [a, b, c] = triangle.map(|p| (self.inverse * (p - self.origin)) / self.scale);
        let separation = segment_triangle_separation(self.end, [a, b, c]);
        if separation.length_squared() > 1.0002 {
            return None;
        }
        // Inverse transpose of the affine map from normalized sphere space.
        Some((self.inverse.inverse() * (separation / self.scale)).normalize_or_zero())
    }
}

fn body_proxies(limbs: &[Limb; 4], feet: &[Foot; 2]) -> [BodyProxy; CONTACT_PROXIES] {
    std::array::from_fn(|i| {
        if i < 6 {
            let foot = feet[i / 3];
            let shape = foot.shapes[i % 3];
            let rotation = foot.inverse.inverse();
            let proportions = FOOT_SCALE;
            // Raw authored ellipsoids: half-row sweeps belong only to point
            // sampling and would overinflate an exact triangle contact query.
            return BodyProxy::new(
                foot.origin + rotation * (shape.center * proportions),
                rotation,
                shape.radii * proportions,
                Vec3::ZERO,
            );
        }
        let i = i - 6;
        let cores = 4 * (CORE_PIECES + 1);
        if i < cores {
            let limb = limbs[i / (CORE_PIECES + 1)];
            let piece = i % (CORE_PIECES + 1);
            let shape = limb.shape;
            let margin = 0.0075;
            if piece == CORE_PIECES {
                // This bounds the positive margin level of the approximate
                // ellipsoid field. The extracted SDF mesh remains a visual
                // proxy, not a proven exact bound on every rendered triangle.
                let radii = shape.belly * (1.0 + margin / shape.belly.min_element());
                return BodyProxy::ellipsoid(limb, shape.belly_center, radii);
            }
            let t = piece as f32 / CORE_PIECES as f32;
            let radius = shape.radius_top + (shape.radius_bottom - shape.radius_top) * t + margin;
            return BodyProxy::capsule(
                limb,
                Vec3::Y * (-shape.length * t),
                Vec3::Y * (-shape.length * (piece + 1) as f32 / CORE_PIECES as f32),
                radius,
            );
        }
        let i = i - cores;
        if i < 2 * CORE_PIECES {
            let limb = limbs[(i / CORE_PIECES) * 2 + 1];
            let piece = i % CORE_PIECES;
            let t = piece as f32 / CORE_PIECES as f32;
            let boot = limb.boot.expect("lower robe limb has fitted footwear");
            // Short max-radius pieces bound the authored shaft taper.
            let length = boot.top - boot.bottom;
            return BodyProxy::capsule(
                limb,
                Vec3::Y * (boot.top - length * t),
                Vec3::Y * (boot.top - length * (piece + 1) as f32 / CORE_PIECES as f32),
                boot.radius_top + (boot.radius_bottom - boot.radius_top) * t,
            );
        }
        let i = i - 2 * CORE_PIECES;
        let limb = limbs[(i / CUFF_PIECES) * 2 + 1];
        let piece = i % CUFF_PIECES;
        let boot = limb.boot.expect("lower robe limb has fitted footwear");
        let (ring, tube) = (boot.cuff_radius, boot.cuff_thickness);
        let center = |i: usize| {
            let angle = i as f32 / CUFF_PIECES as f32 * TAU;
            // Matches props::basis(Y)=(-Z,-X) and the authored 12-segment torus.
            Vec3::new(-angle.sin() * ring, boot.top, -angle.cos() * ring)
        };
        BodyProxy::capsule(limb, center(piece), center(piece + 1), tube)
    })
}

fn closest_segment(p: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let edge = b - a;
    let length = edge.length_squared();
    if length <= 1e-12 {
        a
    } else {
        a + edge * ((p - a).dot(edge) / length).clamp(0.0, 1.0)
    }
}

fn closest_triangle(p: Vec3, [a, b, c]: [Vec3; 3]) -> Vec3 {
    let ab = b - a;
    let ac = c - a;
    if ab.cross(ac).length_squared() <= 1e-12 {
        let candidates = [
            closest_segment(p, a, b),
            closest_segment(p, b, c),
            closest_segment(p, c, a),
        ];
        return candidates
            .into_iter()
            .min_by(|a, b| a.distance_squared(p).total_cmp(&b.distance_squared(p)))
            .unwrap();
    }
    let ap = p - a;
    let d1 = ab.dot(ap);
    let d2 = ac.dot(ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = p - b;
    let d3 = ab.dot(bp);
    let d4 = ac.dot(bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        return a + ab * (d1 / (d1 - d3));
    }
    let cp = p - c;
    let d5 = ab.dot(cp);
    let d6 = ac.dot(cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        return a + ac * (d2 / (d2 - d6));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && d4 >= d3 && d5 >= d6 {
        return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
    }
    let denominator = 1.0 / (va + vb + vc);
    a + ab * (vb * denominator) + ac * (vc * denominator)
}

fn segment_pair_separation(end: Vec3, a: Vec3, b: Vec3) -> Vec3 {
    let edge = b - a;
    let aa = end.length_squared();
    let ee = edge.length_squared();
    if aa <= 1e-12 {
        return closest_segment(Vec3::ZERO, a, b);
    }
    if ee <= 1e-12 {
        return a - closest_segment(a, Vec3::ZERO, end);
    }
    let cross = end.dot(edge);
    let d = end.dot(-a);
    let f = edge.dot(-a);
    let denominator = aa * ee - cross * cross;
    let mut s = if denominator > 1e-12 {
        ((cross * f - d * ee) / denominator).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut t = (cross * s + f) / ee;
    if t < 0.0 {
        t = 0.0;
        s = (-d / aa).clamp(0.0, 1.0);
    } else if t > 1.0 {
        t = 1.0;
        s = ((cross - d) / aa).clamp(0.0, 1.0);
    }
    a + edge * t - end * s
}

fn segment_triangle_separation(end: Vec3, triangle: [Vec3; 3]) -> Vec3 {
    let [a, b, c] = triangle;
    let normal = (b - a).cross(c - a);
    let denominator = normal.dot(end);
    if denominator.abs() > 1e-10 {
        let t = normal.dot(a) / denominator;
        if (0.0..=1.0).contains(&t) {
            let point = end * t;
            // Axis/face intersection is a distinct feature case; endpoints
            // and edge pairs alone miss a capsule piercing the face interior.
            if closest_triangle(point, triangle).distance_squared(point) < 1e-10 {
                return Vec3::ZERO;
            }
        }
    }
    let mut separation = closest_triangle(Vec3::ZERO, triangle);
    let other = closest_triangle(end, triangle) - end;
    if other.length_squared() < separation.length_squared() {
        separation = other;
    }
    for (a, b) in [(a, b), (b, c), (c, a)] {
        let other = segment_pair_separation(end, a, b);
        if other.length_squared() < separation.length_squared() {
            separation = other;
        }
    }
    separation
}

fn posed_limbs(
    shapes: [LimbShape; 2],
    boot: BootShape,
    rest: &[Vec3; JOINTS],
    rot: &[Quat; JOINTS],
    off: &[Vec3; JOINTS],
) -> [Limb; 4] {
    std::array::from_fn(|i| {
        let (leg, shin) = if i < 2 {
            (J::LegL, J::ShinL)
        } else {
            (J::LegR, J::ShinR)
        };
        let h = rest[leg as usize] + off[leg as usize];
        let q = rot[leg as usize];
        let lower = i % 2;
        let origin = if lower == 0 {
            h
        } else {
            h + q * (rest[shin as usize] + off[shin as usize])
        };
        let rotation = if lower == 0 {
            q
        } else {
            q * rot[shin as usize]
        };
        Limb {
            origin,
            inverse: rotation.inverse(),
            shape: shapes[lower],
            boot: (lower == 1).then_some(boot),
        }
    })
}

fn posed_feet(
    shapes: [(Vec3, Vec3); 3],
    limbs: &[Limb; 4],
    rest: &[Vec3; JOINTS],
    rot: &[Quat; JOINTS],
    off: &[Vec3; JOINTS],
) -> [Foot; 2] {
    std::array::from_fn(|i| {
        let joint = if i == 0 { J::FootL } else { J::FootR };
        let lower = limbs[i * 2 + 1];
        let lower_rotation = lower.inverse.inverse();
        let inverse = (lower_rotation * rot[joint as usize]).inverse();
        let scale = FOOT_SCALE;
        let half_cell = (inverse * (Vec3::Y * ROW_HEIGHT * 0.5)) / scale;
        let origin = lower.origin + lower_rotation * (rest[joint as usize] + off[joint as usize]);
        let mut min_y = f32::INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        let shapes = shapes.map(|(center, radii)| {
            let y = (origin + inverse.inverse() * (center * scale)).y;
            let height = ((inverse * Vec3::Y) * scale * radii).length() + ROW_HEIGHT * 0.5;
            min_y = min_y.min(y - height);
            max_y = max_y.max(y + height);
            let half = half_cell / radii;
            FootShape {
                center,
                radii,
                sweep_rotation: Quat::from_rotation_arc(half.normalize(), Vec3::Y),
                sweep_half: half.length(),
            }
        });
        Foot {
            origin,
            inverse,
            shapes,
            min_y,
            max_y,
        }
    })
}

fn lift_weight(row: usize) -> f32 {
    ease(row.saturating_sub(LOWER_ANCHOR) as f32 / (ROWS - LOWER_ANCHOR) as f32)
}

fn fit_rows(
    robe: &mut RobeRig,
    limbs: &[Limb; 4],
    feet: &[Foot; 2],
    rise: &[f32; COLUMNS],
    lower_only: bool,
) {
    let start = if lower_only {
        (LOWER_ANCHOR + 1) * COLUMNS
    } else {
        0
    };
    for i in start..GRID {
        let base = robe.rest[i];
        let radius = base.with_y(0.0).length();
        let origin = Vec3::Y * (base.y + rise[i % COLUMNS] * lift_weight(i / COLUMNS));
        let direction = robe.directions[i];
        let current = envelope(limbs, origin, direction)
            .max(foot_far(feet[0], origin, direction))
            .max(foot_far(feet[1], origin, direction));
        let fit = (current + 0.010 - radius).max(0.0);
        let waist = ease((WAIST_TOP - base.y) / (WAIST_TOP - TOP));
        // Fit anew at the lifted height; retaining the previous radial max
        // would retain a toe-shaped sleeve after its row had risen above it.
        robe.displacement[i] = fit * waist;
    }
}

fn spread_rows(robe: &mut RobeRig, lower_only: bool) {
    for _ in 0..2 {
        for row in 0..=ROWS {
            for column in 0..COLUMNS {
                let i = index(row, column);
                if lower_only && row <= LOWER_ANCHOR {
                    robe.smooth[i] = robe.displacement[i];
                    continue;
                }
                let above = row.saturating_sub(1);
                let below = (row + 1).min(ROWS);
                let average = (robe.displacement[i] * 4.0
                    + robe.displacement[index(row, column + 1)] * 2.0
                    + robe.displacement[index(row, column + COLUMNS - 1)] * 2.0
                    + robe.displacement[index(above, column)] * 2.0
                    + robe.displacement[index(below, column)] * 2.0
                    + robe.displacement[index(above, column + 1)]
                    + robe.displacement[index(above, column + COLUMNS - 1)]
                    + robe.displacement[index(below, column + 1)]
                    + robe.displacement[index(below, column + COLUMNS - 1)])
                    / 16.0;
                robe.smooth[i] = if row == 0 {
                    0.0
                } else {
                    robe.displacement[i].max(average)
                };
            }
        }
        std::mem::swap(&mut robe.displacement, &mut robe.smooth);
    }
    // A local outward majorant: at most four neighboring cells receive each
    // peak, with one unit of radial falloff per unit of neutral edge length.
    // Synchronous passes and wrapped columns avoid directional/seam bias.
    for _ in 0..4 {
        robe.smooth.copy_from_slice(robe.displacement.as_ref());
        for row in LOWER_ANCHOR + 1..=ROWS {
            for column in 0..COLUMNS {
                let i = index(row, column);
                for j in [
                    index(row - 1, column),
                    index((row + 1).min(ROWS), column),
                    index(row, column + 1),
                    index(row, column + COLUMNS - 1),
                ] {
                    let allowance = robe.rest[i].distance(robe.rest[j]);
                    robe.smooth[i] = robe.smooth[i].max(robe.displacement[j] - allowance);
                }
            }
        }
        std::mem::swap(&mut robe.displacement, &mut robe.smooth);
    }
}

fn candidate_rows(robe: &mut RobeRig, rise: &[f32; COLUMNS]) {
    for i in 0..GRID {
        robe.candidate[i] = robe.rest[i]
            + robe.directions[i] * robe.displacement[i]
            + Vec3::Y * (rise[i % COLUMNS] * lift_weight(i / COLUMNS));
    }
}

fn column_lengths(points: &[Vec3; GRID]) -> [f32; COLUMNS] {
    std::array::from_fn(|column| {
        (LOWER_ANCHOR..ROWS)
            .map(|row| points[index(row, column)].distance(points[index(row + 1, column)]))
            .sum()
    })
}

fn contact_direction(radial: Vec3, up: Vec3) -> Vec3 {
    if up.y > 0.3 {
        (radial - up * radial.dot(up)).normalize()
    } else {
        radial
    }
}

fn cell_bounds(points: &[Vec3; GRID], cell: [usize; 4]) -> (Vec3, Vec3) {
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for i in cell {
        let outer = points[i];
        let inner = outer - shell_offset(i % COLUMNS);
        min = min.min(outer).min(inner);
        max = max.max(outer).max(inner);
    }
    (min, max)
}

fn separation_steps(
    robe: &RobeRig,
    cell: [usize; 4],
    directions: [Vec3; 4],
    proxy: BodyProxy,
    normal: Vec3,
) -> Result<[f32; 4], bool> {
    if !normal.is_finite() || normal.length_squared() < 0.5 {
        return Err(false);
    }
    let support = proxy.support(normal);
    let mut steps = [0.0; 4];
    for (k, i) in cell.into_iter().enumerate() {
        let denominator = normal.dot(directions[k]);
        // Never turn a nearly tangent direction into an enormous divided push.
        if denominator <= 0.1 {
            return Err(false);
        }
        let shell = normal.dot(shell_offset(i % COLUMNS)).max(0.0);
        // The last cell also supports a .0015-offset, interpolated hem ribbon.
        let trim = if i / COLUMNS >= ROWS - 1 { 0.0015 } else { 0.0 };
        let deficit = (support + 0.003 + shell + trim - normal.dot(robe.candidate[i])).max(0.0);
        let row = i / COLUMNS;
        if row <= CONTACT_ANCHOR && deficit > 1e-6 {
            return Err(true);
        }
        let step = if deficit > 1e-6 {
            deficit / denominator
        } else {
            0.0
        };
        let max_shift = if row == LOWER_ANCHOR {
            MAX_CONTACT_SHIFT * 0.5
        } else {
            MAX_CONTACT_SHIFT
        };
        if !step.is_finite() || robe.smooth[i] + step > max_shift {
            return Err(false);
        }
        steps[k] = step;
    }
    Ok(steps)
}

#[allow(clippy::too_many_arguments)]
fn contact_triangle(
    robe: &mut RobeRig,
    cell: [usize; 4],
    triangle: [usize; 3],
    inner: [bool; 3],
    proxy: BodyProxy,
    up: Vec3,
    solve: bool,
    report: &mut ContactReport,
) {
    let points = std::array::from_fn(|k| {
        let i = triangle[k];
        robe.candidate[i]
            - if inner[k] {
                shell_offset(i % COLUMNS)
            } else {
                Vec3::ZERO
            }
    });
    let min = points[0].min(points[1]).min(points[2]);
    let max = points[0].max(points[1]).max(points[2]);
    if !proxy.overlaps(min, max) {
        return;
    }
    let Some(normal) = proxy.contact_normal(points) else {
        return;
    };
    if !solve {
        report.remaining += 1;
        return;
    }
    let directions = cell.map(|i| contact_direction(robe.directions[i], up));
    let outward = directions.into_iter().sum::<Vec3>().normalize_or_zero();
    let steps = separation_steps(robe, cell, directions, proxy, normal).or_else(|first| {
        separation_steps(robe, cell, directions, proxy, outward).map_err(|second| first || second)
    });
    match steps {
        Ok(steps) => {
            // All four cell controls share this supporting half-space. Inner
            // skin, hem cap and interpolated trim remain on its exterior side.
            for (k, i) in cell.into_iter().enumerate() {
                robe.candidate[i] += directions[k] * steps[k];
                robe.smooth[i] += steps[k];
            }
        }
        Err(true) => report.boundary += 1,
        Err(false) => report.limited += 1,
    }
}

fn contact_sweep(
    robe: &mut RobeRig,
    proxies: &[BodyProxy; CONTACT_PROXIES],
    up: Vec3,
    solve: bool,
    report: &mut ContactReport,
) {
    for row in CONTACT_ANCHOR..ROWS {
        for column in 0..COLUMNS {
            let a = index(row, column);
            let b = index(row, column + 1);
            let c = index(row + 1, column);
            let d = index(row + 1, column + 1);
            let cell = [a, b, c, d];
            for &proxy in proxies {
                // Recompute this cheap box after prior proxy pushes. No affine
                // transforms or closest-feature work for disjoint whole cells.
                let (min, max) = cell_bounds(&robe.candidate, cell);
                if !proxy.overlaps(min, max) {
                    continue;
                }
                for inner in [false, true] {
                    for triangle in [[a, c, b], [b, c, d]] {
                        contact_triangle(
                            robe, cell, triangle, [inner; 3], proxy, up, solve, report,
                        );
                    }
                }
                if row == ROWS - 1 {
                    contact_triangle(
                        robe,
                        cell,
                        [c, c, d],
                        [false, true, false],
                        proxy,
                        up,
                        solve,
                        report,
                    );
                    contact_triangle(
                        robe,
                        cell,
                        [d, c, d],
                        [false, true, true],
                        proxy,
                        up,
                        solve,
                        report,
                    );
                }
            }
        }
    }
}

fn surface_normals(points: &[Vec3; GRID], normals: &mut [Vec3; GRID]) {
    for row in 0..=ROWS {
        let above = row.saturating_sub(1);
        let below = (row + 1).min(ROWS);
        for column in 0..COLUMNS {
            let across = points[index(row, column + 1)] - points[index(row, column + COLUMNS - 1)];
            let down = points[index(below, column)] - points[index(above, column)];
            normals[index(row, column)] = down.cross(across).normalize_or(Vec3::Z);
        }
    }
}

/// The same clamped Catmull-Rom profile used by the authored three-point tubes.
fn sash_sample(path: [Vec2; 3], t: f32) -> Vec2 {
    let segment = ((t * 2.0) as usize).min(1);
    let u = t * 2.0 - segment as f32;
    let p0 = path[segment.saturating_sub(1)];
    let p1 = path[segment];
    let p2 = path[segment + 1];
    let p3 = path[(segment + 2).min(2)];
    ((p1 * 2.0)
        + (p2 - p0) * u
        + (p0 * 2.0 - p1 * 5.0 + p2 * 4.0 - p3) * u * u
        + (-p0 + p1 * 3.0 - p2 * 3.0 + p3) * u * u * u)
        * 0.5
}

fn surface_binding(points: &[Vec3; GRID], target: Vec2, clearance: f32) -> SurfaceBinding {
    let y = target.y.clamp(HEM, WAIST_TOP);
    let at = Vec2::new(target.x, y);
    let row = (0..ROWS)
        .find(|&row| y >= points[index(row + 1, 0)].y)
        .unwrap_or(ROWS - 1);
    let mut binding = SurfaceBinding {
        triangle: [index(row, 0); 3],
        weights: Vec3::X,
        lift: target.y - y,
        clearance,
    };
    let mut best_distance = f32::INFINITY;
    let mut best_z = f32::NEG_INFINITY;
    for column in 0..COLUMNS {
        let a = index(row, column);
        let b = index(row, column + 1);
        let c = index(row + 1, column);
        let d = index(row + 1, column + 1);
        for triangle in [[a, c, b], [b, c, d]] {
            let [a, b, c] = triangle.map(|i| points[i]);
            if a.z + b.z + c.z <= 0.0 {
                continue;
            }
            let ab = (b - a).truncate();
            let ac = (c - a).truncate();
            let area = ab.perp_dot(ac);
            if area.abs() < 0.0000001 {
                continue;
            }
            let delta = at - a.truncate();
            let wb = delta.perp_dot(ac) / area;
            let wc = ab.perp_dot(delta) / area;
            // The recipe lies on the front. Clamping also gives a bounded
            // nearest surface binding if a later recipe reaches an edge.
            let weights = Vec3::new(1.0 - wb - wc, wb, wc).max(Vec3::ZERO);
            let weights = weights / weights.element_sum();
            let projected = a * weights.x + b * weights.y + c * weights.z;
            let distance = projected.truncate().distance_squared(at);
            if distance < best_distance - 0.00000001
                || ((distance - best_distance).abs() <= 0.00000001 && projected.z > best_z)
            {
                binding.triangle = triangle;
                binding.weights = weights;
                best_distance = distance;
                best_z = projected.z;
            }
        }
    }
    binding
}

fn make_trim(points: &[Vec3; GRID], recipes: &[SashTail]) -> SurfaceTrim {
    let mut bindings = Vec::with_capacity(recipes.len() * SASH_VERTICES);
    let mut clearance = 0.003;
    for recipe in recipes {
        // Distinct cloth layers do not occupy a coplanar overlap. The inner
        // side of each elliptical strip still clears its supporting surface.
        let mut max_depth: f32 = 0.0;
        for row in 0..=SASH_ROWS {
            let t = row as f32 / SASH_ROWS as f32;
            let center = sash_sample(recipe.path, t);
            let radii = sash_sample(recipe.radii, t);
            max_depth = max_depth.max(radii.y * HIP_SCALE.z);
            for side in 0..SASH_SIDES {
                let angle = side as f32 / SASH_SIDES as f32 * TAU;
                let at = Vec2::new((center.x + radii.x * angle.cos()) * HIP_SCALE.x, center.y);
                bindings.push(surface_binding(
                    points,
                    at,
                    clearance + radii.y * HIP_SCALE.z * (1.0 + angle.sin()),
                ));
            }
        }
        for end in [0, 2] {
            bindings.push(surface_binding(
                points,
                Vec2::new(recipe.path[end].x * HIP_SCALE.x, recipe.path[end].y),
                clearance + recipe.radii[end].y * HIP_SCALE.z,
            ));
        }
        // Preserve the full thickness of overlapping tails, not merely a
        // polygon offset: each new layer clears the preceding closed tube.
        clearance += max_depth * 2.0 + 0.0015;
    }
    SurfaceTrim {
        points: vec![Vec3::ZERO; bindings.len()].into_boxed_slice(),
        normals: vec![Vec3::Z; bindings.len()].into_boxed_slice(),
        bindings: bindings.into_boxed_slice(),
    }
}

fn place_trim(trim: &mut SurfaceTrim, points: &[Vec3; GRID], normals: &[Vec3; GRID]) {
    for (i, binding) in trim.bindings.iter().enumerate() {
        let [a, b, c] = binding.triangle;
        let w = binding.weights;
        let n = (normals[a] * w.x + normals[b] * w.y + normals[c] * w.z).normalize_or(Vec3::Z);
        trim.points[i] = points[a] * w.x
            + points[b] * w.y
            + points[c] * w.z
            + n * binding.clearance
            + Vec3::Y * binding.lift;
        trim.normals[i] = n;
    }
    for start in (0..trim.points.len()).step_by(SASH_VERTICES) {
        for row in 0..=SASH_ROWS {
            let above = row.saturating_sub(1);
            let below = (row + 1).min(SASH_ROWS);
            for side in 0..SASH_SIDES {
                let i = start + row * SASH_SIDES + side;
                let across = trim.points[start + row * SASH_SIDES + (side + 1) % SASH_SIDES]
                    - trim.points[start + row * SASH_SIDES + (side + SASH_SIDES - 1) % SASH_SIDES];
                let down = trim.points[start + below * SASH_SIDES + side]
                    - trim.points[start + above * SASH_SIDES + side];
                trim.normals[i] = across.cross(down).normalize_or(trim.normals[i]);
            }
        }
        let caps = start + (SASH_ROWS + 1) * SASH_SIDES;
        let ring_center = |row: usize| {
            (0..SASH_SIDES)
                .map(|side| trim.points[start + row * SASH_SIDES + side])
                .sum::<Vec3>()
                / SASH_SIDES as f32
        };
        trim.normals[caps] = (trim.points[caps] - ring_center(1)).normalize_or(Vec3::Y);
        trim.normals[caps + 1] =
            (trim.points[caps + 1] - ring_center(SASH_ROWS - 1)).normalize_or(-Vec3::Y);
    }
}

fn write_attributes(
    mesh: &mut Mesh,
    points: &[Vec3; GRID],
    normals: &[Vec3; GRID],
    trim: &SurfaceTrim,
) {
    if let Some(VertexAttributeValues::Float32x3(values)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        for i in 0..GRID {
            values[i] = points[i].to_array();
            values[GRID + i] = (points[i] - shell_offset(i % COLUMNS)).to_array();
        }
        for row in 0..2 {
            let blend = if row == 0 {
                1.0 - 0.018 / ROW_HEIGHT
            } else {
                1.0
            };
            for column in 0..COLUMNS {
                let p = points[index(ROWS - 1, column)].lerp(points[index(ROWS, column)], blend);
                let n = normals[index(ROWS - 1, column)]
                    .lerp(normals[index(ROWS, column)], blend)
                    .normalize_or(Vec3::Z);
                values[GRID * 2 + row * COLUMNS + column] = (p + n * 0.0015).to_array();
            }
        }
        for (i, p) in trim.points.iter().enumerate() {
            values[BODY_VERTICES + i] = p.to_array();
        }
    }
    if let Some(VertexAttributeValues::Float32x3(values)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for i in 0..GRID {
            values[i] = normals[i].to_array();
            values[GRID + i] = (-normals[i]).to_array();
        }
        for row in 0..2 {
            let blend = if row == 0 {
                1.0 - 0.018 / ROW_HEIGHT
            } else {
                1.0
            };
            for column in 0..COLUMNS {
                values[GRID * 2 + row * COLUMNS + column] = normals[index(ROWS - 1, column)]
                    .lerp(normals[index(ROWS, column)], blend)
                    .normalize_or(Vec3::Z)
                    .to_array();
            }
        }
        for (i, n) in trim.normals.iter().enumerate() {
            values[BODY_VERTICES + i] = n.to_array();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn spawn(
    commands: &mut Commands,
    library: &mut PropLibrary,
    meshes: &mut Assets<Mesh>,
    hips: Entity,
    look: &Look,
    body: HumanoidBody,
    boot: BootShape,
    sashes: &[SashTail],
) -> RobeRig {
    // One tiny shared recipe supplies the existing matte material. Dynamic
    // garment meshes never enter the appearance cache or mutate another wearer.
    let material = library
        .get("creature/robe-material".to_owned(), meshes, || {
            let mut p = Parts::default();
            p.matte.tri([Vec3::ZERO, Vec3::X, Vec3::Y], Vec3::ONE);
            p
        })
        .0[0]
        .1
        .clone();
    let shapes = [body.thigh, body.shin];
    let rest: Box<[Vec3; GRID]> = Box::new(std::array::from_fn(|i| {
        neutral(look, body, i / COLUMNS, i % COLUMNS)
    }));
    let directions: Box<[Vec3; GRID]> = Box::new(std::array::from_fn(|i| {
        rest[i].with_y(0.0).normalize_or(Vec3::Z)
    }));
    let points = rest.clone();
    let candidate = points.clone();
    let mut normals = Box::new([Vec3::ZERO; GRID]);
    surface_normals(&points, &mut normals);
    let mut trim = make_trim(&points, sashes);
    place_trim(&mut trim, &points, &normals);
    let vertices = BODY_VERTICES + trim.points.len();
    let mut colors = vec![[0.0; 4]; vertices];
    for i in 0..GRID {
        let angle = (i % COLUMNS) as f32 / COLUMNS as f32 * TAU;
        let phase = rand(u64::from(look.v.shade) + 73, 0) * 0.4;
        let shade = 0.97 + 0.03 * (angle * 12.0 + phase).cos();
        colors[i] = (look.cloth * shade).extend(1.0).to_array();
        colors[GRID + i] = (look.cloth * shade * 0.70).extend(1.0).to_array();
    }
    for color in &mut colors[GRID * 2..BODY_VERTICES] {
        *color = look.trim.extend(1.0).to_array();
    }
    for (tail, recipe) in sashes.iter().enumerate() {
        let start = BODY_VERTICES + tail * SASH_VERTICES;
        colors[start..start + SASH_VERTICES].fill(recipe.color.extend(1.0).to_array());
    }
    // Match Builder::finish: palette pigments are sRGB and vertex colors
    // enter Bevy's lighting path in linear space.
    for color in &mut colors {
        let pigment = Vec3::new(color[0], color[1], color[2]);
        let display = pigment.min(Vec3::ONE);
        let excess = pigment - display;
        let linear = Color::srgb(display.x, display.y, display.z).to_linear();
        *color = [
            linear.red + excess.x,
            linear.green + excess.y,
            linear.blue + excess.z,
            color[3],
        ];
    }
    let mut indices = Vec::with_capacity(
        ROWS * COLUMNS * 12 + COLUMNS * 18 + sashes.len() * (SASH_ROWS + 1) * SASH_SIDES * 6,
    );
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let a = index(row, column) as u32;
            let b = index(row, column + 1) as u32;
            let c = index(row + 1, column) as u32;
            let d = index(row + 1, column + 1) as u32;
            indices.extend_from_slice(&[a, c, b, b, c, d]);
            let inner = GRID as u32;
            indices.extend_from_slice(&[
                inner + a,
                inner + b,
                inner + c,
                inner + b,
                inner + d,
                inner + c,
            ]);
        }
    }
    for column in 0..COLUMNS {
        for row in [0, ROWS] {
            let a = index(row, column) as u32;
            let b = index(row, column + 1) as u32;
            let c = a + GRID as u32;
            let d = b + GRID as u32;
            if row == 0 {
                indices.extend_from_slice(&[a, b, c, b, d, c]);
            } else {
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        let a = (GRID * 2 + column) as u32;
        let b = (GRID * 2 + (column + 1) % COLUMNS) as u32;
        let c = a + COLUMNS as u32;
        let d = b + COLUMNS as u32;
        indices.extend_from_slice(&[a, c, b, b, c, d]);
    }
    for tail in 0..sashes.len() {
        let start = (BODY_VERTICES + tail * SASH_VERTICES) as u32;
        let sides = SASH_SIDES as u32;
        for row in 0..SASH_ROWS {
            for side in 0..SASH_SIDES {
                let a = start + (row * SASH_SIDES + side) as u32;
                let b = start + (row * SASH_SIDES + (side + 1) % SASH_SIDES) as u32;
                let c = a + sides;
                let d = b + sides;
                indices.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }
        let caps = start + ((SASH_ROWS + 1) * SASH_SIDES) as u32;
        for side in 0..SASH_SIDES {
            let a = start + side as u32;
            let b = start + ((side + 1) % SASH_SIDES) as u32;
            indices.extend_from_slice(&[caps, b, a]);
            let end = (SASH_ROWS * SASH_SIDES) as u32;
            indices.extend_from_slice(&[caps + 1, a + end, b + end]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; vertices])
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0; 3]; vertices])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    write_attributes(&mut mesh, &points, &normals, &trim);
    let mesh = meshes.add(mesh);
    let entity = commands
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            Visibility::default(),
            Aabb {
                center: bevy::math::Vec3A::new(0.0, -0.4, 0.0),
                half_extents: bevy::math::Vec3A::new(1.2, 0.7, 1.2),
            },
        ))
        .id();
    commands.entity(hips).add_child(entity);
    RobeRig {
        mesh,
        shapes,
        boot,
        feet: body.feet,
        rest,
        directions,
        points,
        candidate,
        normals,
        displacement: Box::new([0.0; GRID]),
        smooth: Box::new([0.0; GRID]),
        neutral_lengths: None,
        contacts: ContactReport::default(),
        trim,
        last_rot: [Quat::IDENTITY; 8],
        last_off: [Vec3::ZERO; 8],
        dirty: true,
    }
}

/// Fixed buffers and fixed topology; only positions/normals change. Unchanged
/// poses leave the mesh untouched, including quiet catalog and settled corpses.
pub(super) fn place(
    robe: &mut RobeRig,
    rest: &[Vec3; JOINTS],
    rot: &[Quat; JOINTS],
    off: &[Vec3; JOINTS],
    meshes: &mut Assets<Mesh>,
) {
    let rotations = POSE_JOINTS.map(|j| rot[j as usize]);
    let offsets = POSE_JOINTS.map(|j| off[j as usize]);
    if !robe.dirty && rotations == robe.last_rot && offsets == robe.last_off {
        return;
    }
    let zero_rise = [0.0; COLUMNS];
    if robe.neutral_lengths.is_none() {
        // spawn() precedes authored leg joints. Initialize once from their
        // immutable rest offsets and an explicit neutral pose, never from the
        // first actual pose (which may already be a fallen corpse).
        let neutral_rot = [Quat::IDENTITY; JOINTS];
        let neutral_off = [Vec3::ZERO; JOINTS];
        let limbs = posed_limbs(robe.shapes, robe.boot, rest, &neutral_rot, &neutral_off);
        let feet = posed_feet(robe.feet, &limbs, rest, &neutral_rot, &neutral_off);
        fit_rows(robe, &limbs, &feet, &zero_rise, false);
        spread_rows(robe, false);
        candidate_rows(robe, &zero_rise);
        robe.neutral_lengths = Some(column_lengths(&robe.candidate));
    }
    let limbs = posed_limbs(robe.shapes, robe.boot, rest, rot, off);
    let feet = posed_feet(robe.feet, &limbs, rest, rot, off);
    fit_rows(robe, &limbs, &feet, &zero_rise, false);
    spread_rows(robe, false);
    candidate_rows(robe, &zero_rise);
    let budget = robe.neutral_lengths.unwrap();
    let mut rise = zero_rise;
    for _ in 0..2 {
        let lengths = column_lengths(&robe.candidate);
        let mut next = std::array::from_fn(|c| {
            (rise[c] + (lengths[c] - budget[c]).max(0.0)).min(MAX_HEM_RISE)
        });
        // Spread lift gently between neighboring columns, never lower a
        // column whose bend already consumed that much of its length budget.
        for _ in 0..2 {
            next = std::array::from_fn(|c| {
                next[c].max(
                    (next[c] * 2.0 + next[(c + 1) % COLUMNS] + next[(c + COLUMNS - 1) % COLUMNS])
                        * 0.25,
                )
            });
        }
        if next == rise {
            break;
        }
        rise = next;
        fit_rows(robe, &limbs, &feet, &rise, true);
        spread_rows(robe, true);
        candidate_rows(robe, &rise);
    }
    let rotation = rot[J::Lift as usize] * rot[J::Hips as usize];
    let origin = off[J::Lift as usize]
        + rot[J::Lift as usize] * (rest[J::Hips as usize] + off[J::Hips as usize]);
    let up = rotation.inverse() * Vec3::Y;
    for i in (LOWER_ANCHOR + 1) * COLUMNS..GRID {
        if up.y <= 0.3 {
            continue;
        }
        let mut p = robe.candidate[i];
        let lift = (0.032 - (origin + rotation * p).y).max(0.0);
        if lift > 0.0 {
            p += up * lift;
            let tangent = contact_direction(robe.directions[i], up);
            let contact = envelope(&limbs, p, tangent)
                .max(foot_far(feet[0], p, tangent))
                .max(foot_far(feet[1], p, tangent));
            p += tangent * (contact + 0.010).max(0.0);
        }
        robe.candidate[i] = p;
    }
    let proxies = body_proxies(&limbs, &feet);
    // Reuse the no-longer-needed smoothing buffer as cumulative contact travel.
    // Every accepted contact uses these same per-vertex directions. Later
    // nonnegative pushes preserve earlier supporting half-spaces and floor
    // height; no smoothing or vertical adjustment is allowed afterward.
    robe.smooth.fill(0.0);
    let mut report = ContactReport::default();
    for _ in 0..2 {
        contact_sweep(robe, &proxies, up, true, &mut report);
    }
    contact_sweep(robe, &proxies, up, false, &mut report);
    if report != robe.contacts {
        bevy::log::debug!(
            boundary = report.boundary,
            limited = report.limited,
            remaining = report.remaining,
            "caster robe constrained contacts"
        );
        robe.contacts = report;
    }
    let changed = robe.points.as_ref() != robe.candidate.as_ref();
    if !changed && !robe.dirty {
        robe.last_rot = rotations;
        robe.last_off = offsets;
        return;
    }
    robe.points.copy_from_slice(robe.candidate.as_ref());
    robe.dirty = true;
    surface_normals(&robe.points, &mut robe.normals);
    place_trim(&mut robe.trim, &robe.points, &robe.normals);
    if let Some(mesh) = meshes.get_mut(&robe.mesh) {
        write_attributes(mesh, &robe.points, &robe.normals, &robe.trim);
        robe.last_rot = rotations;
        robe.last_off = offsets;
        robe.dirty = false;
    }
}
