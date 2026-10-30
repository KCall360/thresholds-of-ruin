use super::*;

pub(in crate::creatures) fn limb_form(shape: LimbShape) -> crate::sdf::Sculpt {
    use crate::sdf::{Sculpt, Shape::*};
    if let Some(contour) = shape.contour {
        let mut form = Sculpt::default();
        for mass in contour.masses {
            form.add(Ellipsoid(mass.center, mass.radii), contour.blend);
        }
        return form;
    }
    let LimbShape {
        length: len,
        radius_top: r0,
        radius_bottom: r1,
        belly,
        belly_center,
        ..
    } = shape;
    let mut s = Sculpt::default();
    s.add(Capsule(Vec3::ZERO, Vec3::Y * -len, r0, r1), 0.0);
    if belly.x > 0.0 {
        s.add(Ellipsoid(belly_center, belly), LimbShape::BELLY_BLEND);
    }
    s
}

pub(super) fn sculpted_limb(b: &mut Builder, shape: LimbShape, color: Vec3) {
    let (low, high) = shape.mesh_bounds(0.02);
    limb_form(shape).mesh_projected(b, low, high, limb_step(shape), &|at, _| {
        color * crate::sdf::Sculpt::weave(at)
    });
}

fn limb_step(shape: LimbShape) -> f32 {
    let mut radius = shape.radius_top.min(shape.radius_bottom);
    if let Some(contour) = shape.contour {
        for mass in contour.masses {
            radius = radius.min(mass.radii.min_element());
        }
    }
    closed_surface_step(radius)
}

/// How far cloth stands off the body beneath it, by build.
pub(in crate::creatures) const SLEEVE_EASE: f32 = 0.007;
pub(in crate::creatures) const TROUSER_EASE: f32 = 0.008;

/// A cloth layer over a limb: the anatomy grown outward by `ease` so the
/// muscle hollows fill and it reads as fabric, gathered into soft rolls at
/// each fraction of its length in `rolls` (the crook of a joint).
pub(in crate::creatures) fn cloth_over(
    mut form: crate::sdf::Sculpt,
    ease: f32,
    rolls: &[(f32, f32)],
) -> crate::sdf::Sculpt {
    use crate::sdf::Shape::Torus;
    form.inflate(ease);
    for &(y, radius) in rolls {
        form.add(
            Torus(
                Vec3::new(0.0, y, 0.004),
                Vec2::new(radius - 0.013, 0.013),
                1.04,
            ),
            0.012,
        );
    }
    form
}

/// Trousers over a thigh or a shin, rolled at the knee.
pub(super) fn trouser_leg(b: &mut Builder, shape: LimbShape, build: f32, knee: bool, color: Vec3) {
    let ease = TROUSER_EASE * build;
    let rolls: &[(f32, f32)] = if knee {
        &[(-0.035, shape.radius_top + ease + 0.004)]
    } else {
        &[(-shape.length + 0.03, shape.radius_bottom + ease + 0.005)]
    };
    let (low, high) = shape.mesh_bounds(0.03);
    cloth_over(limb_form(shape), ease, rolls).mesh_projected(
        b,
        low,
        high,
        limb_step(shape),
        &|at, _| color * crate::sdf::Sculpt::weave(at),
    );
}

/// How hanging cloth is cut: fitted at the waist, rounding out over the
/// hips, then falling to its hem. Radii are across and front to back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(in crate::creatures) struct SkirtCut {
    pub waist: Vec2,
    pub hip: Vec2,
    /// How far down the cloth, 0 at the waist and 1 at the hem, the hip is.
    pub hip_at: f32,
    pub hem: Vec2,
    /// A cutaway coat: its fronts part from the belt down and sweep back
    /// around the legs instead of closing over them.
    pub cutaway: bool,
}

impl SkirtCut {
    /// A plain flare from the top edge to the hem, with no hip of its own.
    pub fn straight(top: Vec2, bottom: Vec2) -> Self {
        Self {
            waist: top,
            hip: top,
            hip_at: 0.0,
            hem: bottom,
            cutaway: false,
        }
    }

    /// The radii `t` of the way down: a rounded swell from the waist to the
    /// hip, then an eased fall to the hem.
    pub fn radius(&self, t: f32) -> Vec2 {
        if t < self.hip_at {
            let s = t / self.hip_at;
            self.waist.lerp(self.hip, 1.0 - (1.0 - s) * (1.0 - s))
        } else {
            let s = (t - self.hip_at) / (1.0 - self.hip_at).max(1e-4);
            self.hip.lerp(self.hem, ease(s))
        }
    }
}

/// One point of a draped garment at `t` down its length and angle `a`.
/// A free hem hangs loose: its folds deepen toward the edge, and the cloth
/// drops lower where each fold swings out, so the hem is never a level ring.
/// Short pieces such as cowls keep their neat edge.
#[allow(clippy::too_many_arguments)]
pub(in crate::creatures) fn drape_point(
    cut: SkirtCut,
    length: f32,
    folds: u32,
    split: f32,
    phase: f32,
    t: f32,
    a: f32,
    inner: bool,
) -> Vec3 {
    let r = cut.radius(t);
    let free = (length / 0.3).clamp(0.0, 1.0);
    let wave = (a * folds as f32 + phase).cos();
    let fold = wave * (0.007 * t.powf(1.2) + 0.01 * free * t.powi(3));
    let drop = 0.013 * free * (0.5 + 0.5 * wave) * ease((t - 0.6) / 0.4);
    let thickness = if inner { 0.007 } else { 0.0 };
    let hem_split =
        split * a.cos().max(0.0).powi(if split < 0.0 { 2 } else { 14 }) * ease((t - 0.55) / 0.45);
    // A slit parts only at the front; a cutaway's fronts part from the belt
    // and swing back across the whole front of the legs.
    let opening = if cut.cutaway {
        split.max(0.0) * a.cos().max(0.0).powi(2) * ease(t / 0.6)
    } else {
        split.max(0.0) * a.cos().max(0.0).powi(12) * ease((t - 0.3) / 0.7)
    };
    let side = if a < PI { 1.0 } else { -1.0 };
    Vec3::new(
        a.sin() * (r.x + fold - thickness) + side * opening,
        0.04 - (length + 0.04) * t + hem_split - drop,
        a.cos() * (r.y + fold - thickness),
    )
}

/// Radial ease at the emitted cloth grid's vertices. Local fitting keeps a
/// lifted front split from forcing unrelated side and back panels outward.
pub(super) struct ClothFit {
    pub(super) lift: Vec<f32>,
    pub(super) rows: usize,
}

/// The surface used by both cloth emission and fitting another layer over it.
/// Keeping phase here prevents the fitter from silently using different folds.
#[derive(Clone)]
pub(super) struct ClothSurface {
    pub(super) cut: SkirtCut,
    pub(super) length: f32,
    pub(super) folds: u32,
    pub(super) split: f32,
    pub(super) phase: f32,
    /// How the hem is cut round the garment.
    pub(super) hem: Hem,
    pub(super) fit: Option<std::sync::Arc<ClothFit>>,
    pub(super) seam: Option<std::sync::Arc<Vec<Vec3>>>,
    pub(super) paired: bool,
}

impl ClothSurface {
    pub(super) fn new(cut: SkirtCut, length: f32, folds: u32, split: f32, seed: u64) -> Self {
        Self {
            cut,
            length,
            folds,
            split,
            phase: rand(seed + 73, 0) * 0.4,
            hem: Hem::Even,
            fit: None,
            seam: None,
            paired: false,
        }
    }

    pub(super) fn with_hem(self, hem: Hem) -> Self {
        Self { hem, ..self }
    }

    pub(super) fn rows(&self) -> usize {
        ((self.length / 0.025).ceil() as usize).max(12)
    }

    pub(super) fn point(&self, t: f32, angle: f32, inner: bool) -> Vec3 {
        let mut point = drape_point(
            self.cut,
            self.length * self.hem.reach(angle),
            self.folds,
            self.split,
            self.phase,
            t,
            angle,
            inner,
        );
        if self.paired {
            point.y -= self.split * angle.cos().max(0.0).powi(14) * ease((t - 0.55) / 0.45);
        }
        if let Some(seam) = &self.seam {
            let column = (angle / TAU).clamp(0.0, 1.0) * 64.0;
            let low = (column.floor() as usize).min(63);
            let target = seam[low].lerp(seam[low + 1], column - low as f32);
            let original = drape_point(
                self.cut,
                self.length,
                self.folds,
                self.split,
                self.phase,
                0.0,
                angle,
                false,
            );
            point += (target - original) * (1.0 - ease(t / 0.25));
        }
        if let Some(fit) = &self.fit {
            let row = t.clamp(0.0, 1.0) * fit.rows as f32;
            let low = (row.floor() as usize).min(fit.rows - 1);
            let column = (angle / TAU).clamp(0.0, 1.0) * 64.0;
            let left = (column.floor() as usize).min(63);
            let along = column - left as f32;
            let at = |r: usize| {
                let a = fit.lift[r * 65 + left];
                let b = fit.lift[r * 65 + left + 1];
                a + (b - a) * along
            };
            let lift = at(low) + (at(low + 1) - at(low)) * (row - low as f32);
            let direction = Vec3::new(point.x, 0.0, point.z).normalize_or_zero();
            point += direction * lift;
        }
        point
    }

    /// A raised waist must clear the full pelvis on the way down to the
    /// original hip cut, not only meet the body at its top seam.
    pub(super) fn fit_pelvis(mut self, body: HumanoidBody) -> Self {
        let support = pelvic_supports(body, Vec3::ZERO);
        let rows = self.rows();
        let mut fit = ClothFit {
            lift: vec![0.0; (rows + 1) * 65],
            rows,
        };
        for row in 0..=rows {
            for column in 0..=64 {
                let t = row as f32 / rows as f32;
                let angle = column as f32 * TAU / 64.0;
                let point = self.point(t, angle, true);
                if !(-0.10..=0.16).contains(&point.y) {
                    continue;
                }
                let out = Vec3::new(point.x, 0.0, point.z).normalize_or(Vec3::Z);
                let body_point = pelvis_surface(&support, point.y, out);
                if support.distance(body_point).abs() < 0.002 {
                    fit.lift[row * 65 + column] =
                        (body_point.dot(out) + 0.008 - point.dot(out)).max(0.0);
                }
            }
        }
        self.fit = Some(std::sync::Arc::new(fit));
        self
    }

    /// Sample the emitted triangles, not the analytic curve between vertices.
    /// Concave folds can put a triangle outside that curve.
    pub(super) fn mesh_point(&self, t: f32, angle: f32, inner: bool) -> Vec3 {
        let row = t.clamp(0.0, 1.0) * self.rows() as f32;
        let low = (row.floor() as usize).min(self.rows() - 1);
        let v = row - low as f32;
        let column = (angle / TAU).clamp(0.0, 1.0) * 64.0;
        let left = (column.floor() as usize).min(63);
        let u = column - left as f32;
        let at = |r, c| self.point(r as f32 / self.rows() as f32, c as f32 * TAU / 64.0, inner);
        let a = at(low, left);
        let b = at(low, left + 1);
        let c = at(low + 1, left + 1);
        let d = at(low + 1, left);
        if u >= v {
            a * (1.0 - u) + b * (u - v) + c * v
        } else {
            a * (1.0 - v) + c * u + d * (v - u)
        }
    }

    /// Fit an upright lower layer in the outer garment's coordinates. Each
    /// support sample expands only the cloth cells sharing its height and angle.
    /// This is resting fit; independently animated layers still need contact.
    pub(super) fn fit_over(
        mut self,
        under: Self,
        under_to_self: Affine3A,
        under_trim: bool,
    ) -> Self {
        let rows = self.rows();
        let mut fit = ClothFit {
            lift: vec![0.0; (rows + 1) * 65],
            rows,
        };
        // Bound each actual emitted quad, including its midpoints. Radius
        // changes preserve angle and height, so these bounds remain valid.
        let mut cells = Vec::with_capacity(rows * 64);
        let delta = |angle: f32, center: f32| (angle - center + PI).rem_euclid(TAU) - PI;
        for row in 0..rows {
            for side in 0..64 {
                let center = (side as f32 + 0.5) * TAU / 64.0;
                let mut low_y = f32::INFINITY;
                let mut high_y = f32::NEG_INFINITY;
                let mut low_angle = f32::INFINITY;
                let mut high_angle = f32::NEG_INFINITY;
                for vertical in 0..=2 {
                    for horizontal in 0..=2 {
                        let t = (row as f32 + vertical as f32 * 0.5) / rows as f32;
                        let a = (side as f32 + horizontal as f32 * 0.5) * TAU / 64.0;
                        let point = self.point(t, a, true);
                        low_y = low_y.min(point.y);
                        high_y = high_y.max(point.y);
                        let angle = delta(point.x.atan2(point.z), center);
                        low_angle = low_angle.min(angle);
                        high_angle = high_angle.max(angle);
                    }
                }
                let corners = [
                    (row, side),
                    (row, side + 1),
                    (row + 1, side),
                    (row + 1, side + 1),
                ]
                .map(|(r, c)| {
                    let point = self.point(r as f32 / rows as f32, c as f32 * TAU / 64.0, true);
                    (r * 65 + c, point)
                });
                cells.push((center, low_y, high_y, low_angle, high_angle, corners));
            }
        }
        // Sample between the lower mesh's emitted rows/columns too. The
        // margin covers triangle chords and the raised trim's radial extent.
        const GAP: f32 = 0.005;
        for row in 0..=under.rows() * 2 {
            let t = row as f32 / (under.rows() * 2) as f32;
            for side in 0..=128 {
                let angle = side as f32 * TAU / 128.0;
                let mut point = under.mesh_point(t, angle, false);
                if under_trim {
                    point += Vec3::new(angle.sin(), 0.0, angle.cos()) * 0.002;
                }
                let point = under_to_self.transform_point3(point);
                let radius = point.xz().length() + GAP;
                let direction = point.x.atan2(point.z);
                for &(center, low_y, high_y, low_angle, high_angle, corners) in &cells {
                    if point.y < low_y - GAP || point.y > high_y + GAP {
                        continue;
                    }
                    let angle = delta(direction, center);
                    if angle < low_angle - 0.015 || angle > high_angle + 0.015 {
                        continue;
                    }
                    // Compare at the support's actual height and direction.
                    // Using every corner's radius at once turns a sloping quad
                    // into its widest enclosing cylinder and creates shelves.
                    let y = point.y.clamp(low_y + 1e-6, high_y - 1e-6);
                    let origin = Vec3::Y * y;
                    let ray = Vec3::new(direction.sin(), 0.0, direction.cos());
                    let vertices = corners.map(|(_, p)| p);
                    let mut hit = None;
                    for indices in [[0, 1, 3], [0, 3, 2]] {
                        let [a, b, c] = indices.map(|i| vertices[i]);
                        let e1 = b - a;
                        let e2 = c - a;
                        let h = ray.cross(e2);
                        let determinant = e1.dot(h);
                        if determinant.abs() < 1e-9 {
                            continue;
                        }
                        let delta = origin - a;
                        let u = delta.dot(h) / determinant;
                        let q = delta.cross(e1);
                        let v = ray.dot(q) / determinant;
                        let reach = e2.dot(q) / determinant;
                        if u >= -1e-5 && v >= -1e-5 && u + v <= 1.00001 && reach > 0.0 {
                            hit = Some(reach);
                        }
                    }
                    // Neighbouring support inside the clearance margin can
                    // lie just outside the triangle. Extend its edge profile
                    // conservatively instead of dropping that support.
                    let inner_radius = hit.unwrap_or_else(|| {
                        let u = ((angle - low_angle) / (high_angle - low_angle).max(1e-6))
                            .clamp(0.0, 1.0);
                        let top = vertices[0].lerp(vertices[1], u);
                        let bottom = vertices[2].lerp(vertices[3], u);
                        let v = ((top.y - y) / (top.y - bottom.y).max(1e-6)).clamp(0.0, 1.0);
                        top.xz().length() + (bottom.xz().length() - top.xz().length()) * v
                    });
                    let lift = (radius - inner_radius).max(0.0);
                    for (index, _) in corners {
                        fit.lift[index] = fit.lift[index].max(lift);
                    }
                }
            }
        }
        // The front's duplicate grid columns are one sewn edge until a slit
        // actually opens. Asymmetric support must not separate that edge.
        for row in 0..=rows {
            let t = row as f32 / rows as f32;
            if self.point(t, 0.0, true).distance(self.point(t, TAU, true)) < 1e-6 {
                let lift = fit.lift[row * 65].max(fit.lift[row * 65 + 64]);
                fit.lift[row * 65] = lift;
                fit.lift[row * 65 + 64] = lift;
            }
        }
        // This pass measures additional clearance from the already fitted
        // surface. Keep earlier supports when fitting a stack of underlayers.
        if let Some(previous) = &self.fit {
            for (lift, previous) in fit.lift.iter_mut().zip(&previous.lift) {
                *lift += previous;
            }
        }
        self.fit = Some(std::sync::Arc::new(fit));
        self
    }
}

/// One articulated plate: a band from `top` down to `bottom` (heights along a
/// limb), with its radius at each edge.
#[derive(Clone, Copy, Debug)]
pub(in crate::creatures) struct Lame {
    pub top: f32,
    pub bottom: f32,
    pub radius_top: f32,
    pub radius_bottom: f32,
}

/// The cuisse over a thigh: four lames from the hip to the knee cop, cut
/// for this wearer's thigh.
pub(in crate::creatures) fn cuisse_lames(body: HumanoidBody) -> Vec<Lame> {
    fitted_lames(&limb_form(body.thigh), -0.04, 4, 0.085, PLATE_CLEARANCE)
}

/// The rerebrace below a pauldron: three lames down the upper arm, cut for
/// this wearer's arm.
pub(in crate::creatures) fn rerebrace_lames(body: HumanoidBody) -> Vec<Lame> {
    let arm = upper_arm_form(1.0, body.build, body.arm_length, body.profile.muscle, 0.94);
    let length = body.arm_length;
    fitted_lames(&arm, -0.08 * length, 3, 0.055 * length, PLATE_CLEARANCE)
}

/// How far plate stands off the limb it covers: room for the padding and
/// mail beneath it, and for the limb's mesh facets.
pub(super) const PLATE_CLEARANCE: f32 = 0.007;

/// How far a sculpted limb reaches from its axis at a height.
pub(super) fn limb_reach(form: &crate::sdf::Sculpt, y: f32) -> f32 {
    (0..24)
        .filter_map(|k| {
            let a = k as f32 / 24.0 * TAU;
            let out = Vec3::new(a.cos(), 0.0, a.sin());
            let hit = form.onto(Vec3::new(0.0, y, 0.0) + out * 0.3, -out);
            (form.distance(hit).abs() < 0.002).then(|| Vec2::new(hit.x, hit.z).length())
        })
        .fold(0.0, f32::max)
}

/// Overlapping lames fitted over a limb's sculpted form. Each band clears
/// the limb along its whole span, and its upper edge tucks inside the lower
/// edge of the band above, so the plates slide as the limb bends without
/// opening a gap; a swelling muscle widens the plates above it rather than
/// turning a shingle the wrong way.
pub(in crate::creatures) fn fitted_lames(
    form: &crate::sdf::Sculpt,
    top: f32,
    count: usize,
    length: f32,
    clearance: f32,
) -> Vec<Lame> {
    const TUCK: f32 = 0.004;
    const FLARE: f32 = 0.003;
    let overlap = length * 0.3;
    let mut bands: Vec<Lame> = (0..count)
        .map(|k| {
            let upper = top - k as f32 * (length - overlap);
            let lower = upper - length;
            let need = (0..=4)
                .map(|s| limb_reach(form, upper + (lower - upper) * s as f32 / 4.0))
                .fold(0.0, f32::max)
                + clearance;
            Lame {
                top: upper,
                bottom: lower,
                radius_top: need,
                radius_bottom: need + FLARE,
            }
        })
        .collect();
    for k in (0..count.saturating_sub(1)).rev() {
        let inside = bands[k + 1].radius_top + TUCK;
        if bands[k].radius_bottom < inside {
            bands[k].radius_bottom = inside;
            bands[k].radius_top = bands[k].radius_top.max(inside - FLARE);
        }
    }
    bands
}

/// Draw overlapping lames as shallow metal bands with darker lower edges.
pub(super) fn plate_lames(b: &mut Builder, bands: &[Lame]) {
    for (k, &lame) in bands.iter().enumerate() {
        let shade = 1.0 - 0.04 * k as f32;
        b.tube(
            &[Vec3::Y * lame.top, Vec3::Y * lame.bottom],
            &[
                Vec2::new(lame.radius_top, lame.radius_top * 0.96),
                Vec2::new(lame.radius_bottom, lame.radius_bottom * 0.96),
            ],
            16,
            &|_, _, along| STEEL * shade * if along > 0.85 { 0.82 } else { 1.0 },
        );
    }
}

/// Whether a kind's tunic skirt is a cutaway coat, as the leprechauns wear
/// on their sheet: open from the belt over trousered legs.
pub(in crate::creatures) fn cutaway_coat(style: Style) -> bool {
    style == Style::Leprechaun
}

/// Where the armhole's curve turns under the arm.
pub(super) const ARMPIT: f32 = 0.44;

/// Where a sleeveless bodice's armhole edge lies at a height, from the
/// midline: down from the shoulder strap and closing under the arm, so the
/// bodice wraps the whole torso below the armpit. In front (`front` 1, the
/// back 0) it curves out round a chest so the side of it stays covered.
pub(in crate::creatures) fn armhole_edge(body: HumanoidBody, y: f32, front: f32) -> f32 {
    let sh = body.shoulders * body.build;
    let side = 0.16 * sh;
    let under = ((ARMPIT - y) / 0.06).max(0.0);
    let edge = side + under * under * 0.2 * sh;
    let Some(bust) = body.bust else {
        return edge;
    };
    let (center, radii) = bust.mass(1.0, 0.0);
    let reach = (center.x + radii.x + 0.012).min(0.183 * sh * 0.97);
    let t = ((y - center.y) / (radii.y + 0.05)).clamp(-1.0, 1.0);
    edge.max(side + (reach - side).max(0.0) * (1.0 - t * t) * front)
}

/// Where a bikini top covers the torso: a cup over each side of her chest,
/// fitted to its own size and contour, and the band beneath it all round.
/// Returns how near the covered point lies to a cup's bound rim, 0 at its
/// heart to 1 at the edge; a man has neither cups nor band.
pub(in crate::creatures) fn bikini_cover(body: HumanoidBody, at: Vec3) -> Option<f32> {
    let bust = body.bust?;
    if (at.y - (bust.fold + 0.004)).abs() < 0.011 {
        return Some(0.0);
    }
    [-1.0f32, 1.0]
        .into_iter()
        .filter_map(|side| {
            let (center, radii) = bust.mass(side, 0.0);
            let dx = (at.x - center.x) / (radii.x * 1.12);
            let dy = (at.y - (center.y + 0.12 * radii.y)) / (radii.y * 1.28);
            let d = (dx * dx + dy * dy).sqrt();
            (d < 1.0 && at.z > center.z - radii.z * 0.8).then_some(d)
        })
        .reduce(f32::min)
}

/// Where a strapless bodice's top edge lies across the front at `x`: a
/// sweetheart rising over each side of her chest and dipping between; behind
/// (`front` 0) a level band at her underarm.
pub(in crate::creatures) fn strapless_top(body: HumanoidBody, x: f32, front: f32) -> f32 {
    sweetheart_top(body, x, front, 0.1)
}

pub(super) fn sweetheart_top(body: HumanoidBody, x: f32, front: f32, dip_share: f32) -> f32 {
    let Some(bust) = body.bust else {
        return 0.37 + 0.0 * front;
    };
    let (left, l) = bust.mass(-1.0, 0.0);
    let (right, r) = bust.mass(1.0, 0.0);
    let (center, radii) = if x < 0.0 { (left, l) } else { (right, r) };
    let peak = center.y + radii.y * 0.55;
    // Independent peaks share one center dip and level back, avoiding a
    // discontinuity where two differently fitted halves meet.
    let dip = (left.y - l.y * dip_share).min(right.y - r.y * dip_share);
    let d = x.abs() / center.x.abs();
    let sweetheart = if d < 1.0 {
        dip + (peak - dip) * (1.0 - (1.0 - d) * (1.0 - d))
    } else {
        peak - (d - 1.0) * 0.06
    };
    let back = (left.y + l.y * 0.3 + right.y + r.y * 0.3) * 0.5;
    back + (sweetheart - back) * front
}

/// Where a one-shoulder bodice's top edge lies at `x`: over her right
/// shoulder, falling diagonally beneath her left arm.
pub(in crate::creatures) fn one_shoulder_top(body: HumanoidBody, x: f32) -> f32 {
    let sh = body.shoulders * body.build;
    let low = body
        .bust
        .map_or(0.36, |bust| bust.apex(1.0).y + bust.radii.y * 0.45);
    let t = ((x + 0.1 * sh) / (0.27 * sh)).clamp(0.0, 1.0);
    0.56 + (low - 0.56) * t
}

/// Shared continuous blend from back coverage to front coverage.
pub(super) fn bodice_front(at: Vec3) -> f32 {
    ((at.z + 0.02) / 0.08).clamp(0.0, 1.0)
}

/// Whether a bodice covers a point of the torso, by what holds it up and
/// how far down it reaches; its neckline is cut separately.
pub(in crate::creatures) fn bodice_covers(
    body: HumanoidBody,
    bodice: Bodice,
    sleeveless: bool,
    at: Vec3,
) -> bool {
    if bodice.cover == Cover::Bare {
        return false;
    }
    let front = bodice_front(at);
    let fold = body.bust.map_or(0.27, |bust| bust.fold);
    if let Cover::Crop(hem) = bodice.cover {
        let angle = at.x.atan2(at.z);
        let edge = fold - 0.03 - 0.04 * (1.0 - hem.reach(angle));
        if at.y < edge {
            return false;
        }
    }
    let armhole = |front: f32| at.x.abs() > armhole_edge(body, at.y, front) || at.y > 0.51;
    match bodice.straps {
        Straps::Shoulders => !(sleeveless && armhole(front)),
        Straps::Strapless => at.y < strapless_top(body, at.x, front),
        Straps::Sweetheart => at.y < sweetheart_top(body, at.x, front, 0.55),
        Straps::Panels { width } => panel_parts::covers(body, at, width),
        // The strap itself runs over the top of her right shoulder; only
        // the arm's side of it is bare.
        Straps::OneShoulder => {
            at.y < one_shoulder_top(body, at.x)
                && !(at.x < 0.0 && sleeveless && at.x.abs() > armhole_edge(body, at.y, front))
        }
        Straps::Halter => {
            if front < 0.5 {
                // The back is bare above the band beneath her chest.
                return at.y < fold + 0.02;
            }
            let (from, wide) = body.bust.map_or((0.33, 0.12), |bust| {
                let (center, radii) = bust.mass(1.0, 0.0);
                (center.y + radii.y * 0.3, center.x + radii.x)
            });
            // Each panel narrows from the side of her chest to the neck.
            let t = ((at.y - from) / (0.55 - from)).clamp(0.0, 1.0);
            at.y < 0.55 && at.x.abs() < wide + (0.045 * body.build - wide) * t
        }
    }
}

/// A garment's V opening resolved on one wearer: its point on the sternum
/// and its edges rising to the collarbones, in torso coordinates.
#[derive(Clone, Copy, Debug)]
pub(in crate::creatures) struct NecklineCut {
    /// Height of the V's point and its half-width there.
    pub bottom: f32,
    pub point: f32,
    /// Height of the collarbones and the opening's half-width there.
    pub collar: f32,
    pub wide: f32,
    /// Whether a panel crosses over beneath the point.
    pub wrap: bool,
    /// A closed teardrop beneath the collar rather than a V to it.
    pub keyhole: bool,
}

impl NecklineCut {
    pub(super) const THROAT: f32 = 0.46;
    pub(super) const COLLAR: f32 = 0.53;

    /// The cut a neckline makes on this body, if it opens at all. The point
    /// follows the chest down the sternum; the edges always keep the fullest
    /// part of each side covered, however deep the plunge.
    pub fn resolve(neckline: Neckline, body: HumanoidBody) -> Option<Self> {
        let (depth, width, wrap, keyhole) = match neckline {
            Neckline::Closed => return None,
            Neckline::Plunge { depth, width, wrap } => (depth, width, wrap, false),
            Neckline::Keyhole { depth, width } => (depth, width, false, true),
        };
        let sh = body.shoulders * body.build;
        let fold = body.bust.map_or(0.3, |bust| bust.fold);
        let bottom = Self::THROAT + (fold - Self::THROAT) * depth;
        let mut cut = Self {
            bottom,
            point: 0.006,
            collar: Self::COLLAR,
            wide: width * sh,
            wrap,
            keyhole,
        };
        if let Some(bust) = body.bust {
            // At the height of the fullest point the edge stays well inside
            // it: the opening shows the upper slope and the sternum between,
            // never the fullness itself.
            // A centered opening is constrained by both independently fitted
            // sides. Tightening for the second side must never reopen the first.
            for side in [-1.0, 1.0] {
                let apex = bust.apex(side);
                let (_, radii) = bust.mass(side, 0.0);
                let limit = apex.x.abs() - 0.55 * radii.x;
                if apex.y > bottom && cut.edge(apex.y) > limit {
                    let t = (apex.y - bottom) / (Self::COLLAR - bottom);
                    cut.point = cut.point.min(limit * 0.5);
                    cut.wide = cut.point + (limit - cut.point) / t;
                }
            }
        }
        Some(cut)
    }

    /// The opening's half-width at a height, where it is open at all.
    pub fn edge(&self, y: f32) -> f32 {
        self.point + (self.wide - self.point) * (y - self.bottom) / (self.collar - self.bottom)
    }

    /// Whether a point on the front of the torso lies in the opening.
    pub fn open(&self, at: Vec3) -> bool {
        if self.keyhole {
            return at.z > 0.0 && self.keyhole_reach(at) < 1.0;
        }
        at.z > 0.0 && at.y > self.bottom && at.x.abs() < self.edge(at.y)
    }

    /// The keyhole's top, below the collar, and how far a point lies out
    /// across it: under 1 inside. Its edges follow the plunge's, narrowing
    /// to a round top.
    pub(super) fn keyhole_reach(&self, at: Vec3) -> f32 {
        let top = self.collar - 0.05;
        let mid = (self.bottom + top) * 0.5;
        let half = (top - self.bottom) * 0.5;
        let wide = self.edge(mid).max(0.012);
        let v = (at.y - mid) / half;
        // Fuller below, as a drop.
        let w = wide * (1.0 - 0.35 * v.max(0.0));
        (at.x / w).powi(2) + v * v
    }

    /// Points round the keyhole's rim, for its binding.
    pub fn keyhole_rim(&self) -> Vec<Vec2> {
        let top = self.collar - 0.05;
        let mid = (self.bottom + top) * 0.5;
        let half = (top - self.bottom) * 0.5;
        let wide = self.edge(mid).max(0.012);
        (0..=20)
            .map(|k| {
                let a = k as f32 / 20.0 * TAU;
                let v = a.sin();
                Vec2::new(a.cos() * wide * (1.0 - 0.35 * v.max(0.0)), mid + v * half)
            })
            .collect()
    }

    /// The same opening carved through a layer worn over the garment, up
    /// to that layer's top: a tapering cut along the chest's front, `margin`
    /// wider so the outer edge frames the inner one.
    pub fn opening(&self, body: HumanoidBody, top: f32, margin: f32) -> crate::sdf::Shape {
        let b = body.build;
        let sh = body.shoulders * b;
        let above = top + 0.06;
        let low = body.bust.map_or(
            super::anatomy::chest_wall(b, sh, 0.0, self.bottom) + 0.02,
            |bust| bust.apex(1.0).z + 0.01,
        );
        crate::sdf::Shape::Capsule(
            Vec3::new(0.0, self.bottom - margin * 0.5, low),
            Vec3::new(
                0.0,
                above,
                super::anatomy::chest_wall(b, sh, 0.0, above) + 0.03,
            ),
            self.point + margin,
            self.edge(above) + margin,
        )
    }
}

/// How a tunic skirt is cut for a body, before the hips' Storybook
/// proportion is applied. The waistband hugs the pelvis it hangs from; the
/// cloth rounds out over the hips where the thighs join them, then falls to
/// a hem flared just enough to contain both thighs, diagonals included, at
/// every height down its `length`, so fuller hips or heavier builds never
/// push a thigh through.
pub(in crate::creatures) fn skirt_cut(
    body: HumanoidBody,
    flare: f32,
    length: f32,
    trousered: bool,
    cutaway: bool,
) -> SkirtCut {
    use super::anatomy::{HIP_SCALE, LIMB_SCALE};
    let b = body.build;
    let thigh = body.thigh;
    let ease_room = if trousered { TROUSER_EASE * b } else { 0.0 };
    let proportion = Vec2::new(HIP_SCALE.x, HIP_SCALE.z);
    let root = body.leg_root(1.0);
    // The thigh as sculpted, in the hips' space and as worn: its surface
    // found round each height, grown by any trouser ease and a margin.
    let form = limb_form(thigh);
    let surface = |y: f32| -> Vec<Vec2> {
        let along = y - root.y;
        (0..24)
            .filter_map(|k| {
                let a = k as f32 / 24.0 * TAU;
                let out = Vec3::new(a.cos(), 0.0, a.sin());
                let hit = form.onto(Vec3::new(0.0, along, 0.0) + out * 0.25, -out);
                (form.distance(hit).abs() < 0.002).then(|| {
                    let worn = hit * LIMB_SCALE + out * (ease_room + CLEARANCE);
                    Vec2::new(root.x + worn.x, worn.z)
                })
            })
            .collect()
    };
    // Room for the thigh mesh's surface-net step and the cloth's own
    // thickness, so neither surface's facets meet the other's.
    const CLEARANCE: f32 = 0.014;
    // The fold troughs that `drape_point` pulls in at this far down.
    let free = (length / 0.3).clamp(0.0, 1.0);
    let trough = |t: f32| 0.007 * t.powf(1.2) + 0.01 * free * t.powi(3);
    let contains = |radii: Vec2, t: f32, points: &[Vec2]| {
        let worn = radii * proportion - Vec2::splat(trough(t));
        points
            .iter()
            .all(|&point| (point / worn).length_squared() <= 1.0)
    };
    // The waistband sits on the pelvis where it hangs from, just below the
    // belt: the sculpted pelvis's own section at that height, measured
    // across and front to back, its seat included.
    let pelvis = pelvic_supports(body, Vec3::ZERO);
    let waist_y = 0.04;
    // It hangs from beneath the belt, so it is never shallower than the
    // belt above it, front to back.
    let reach = |dir: Vec3| pelvis_surface(&pelvis, waist_y, dir).dot(dir);
    let belt = super::robe::belt_radii(body, BELT_BOTTOM_Y);
    let depth = reach(Vec3::Z).max(reach(Vec3::NEG_Z)).max(belt.y);
    let top = Vec2::new(reach(Vec3::X), depth) * 1.01 + Vec2::splat(0.004);
    let bottom = Vec2::new(flare * b * body.hips_width, flare * 0.74 * b);
    // Below the crotch, where the thighs part from the pelvis, down to the hem.
    // The hip falls just below where the thighs join the pelvis.
    let hip_y = root.y - 0.015;
    let mut cut = SkirtCut {
        waist: top,
        hip: top,
        hip_at: ((waist_y - hip_y) / (length + waist_y)).min(0.6),
        hem: bottom,
        cutaway,
    };
    let rings: Vec<(f32, Vec<Vec2>)> = (0..)
        .map(|k| -0.02 * k as f32)
        .take_while(|&y| y > -length)
        .map(|y| {
            let mut points = surface(y);
            // A cutaway's fronts part around the legs.
            points.retain(|p| !cutaway || p.y < 0.02);
            (y, points)
        })
        .collect();
    // Which way the cloth is short at this ring: across, or front to back.
    let short = |radii: Vec2, t: f32, points: &[Vec2]| {
        let worn = radii * proportion - Vec2::splat(trough(t));
        points
            .iter()
            .map(|&point| point / worn)
            .filter(|q| q.length_squared() > 1.0)
            .fold(Vec2::ZERO, |most, q| most.max(q.abs()))
    };
    for _ in 0..120 {
        let (mut hip_short, mut hem_short) = (Vec2::ZERO, false);
        for (y, points) in &rings {
            let t = (waist_y - y) / (length + waist_y);
            if !contains(cut.radius(t), t, points) {
                // Widen whichever shapes the cloth here: the hip until the
                // fall towards the hem is well under way, then the hem.
                let fall = (t - cut.hip_at) / (1.0 - cut.hip_at).max(1e-4);
                if ease(fall) < 0.5 {
                    hip_short = hip_short.max(short(cut.radius(t), t, points));
                } else {
                    hem_short = true;
                }
            }
        }
        if hip_short == Vec2::ZERO && !hem_short {
            break;
        }
        // The hip widens only the way the thighs press on it, so a skirt
        // clearing the thighs at the sides still hugs the seat and belly.
        if hip_short.x >= hip_short.y && hip_short != Vec2::ZERO {
            cut.hip.x *= 1.015;
        } else if hip_short != Vec2::ZERO {
            cut.hip.y *= 1.015;
        }
        if hem_short {
            cut.hem *= 1.015;
        }
        // Cloth hangs from the hip; it never draws in beneath it.
        cut.hem = cut.hem.max(cut.hip);
    }
    cut
}

/// Whether her boots have heels: the kinds whose women wear them on their
/// concept sheets.
pub(in crate::creatures) fn heeled_boots(style: Style, woman: bool) -> bool {
    woman
        && matches!(
            style,
            Style::Vampire | Style::Medusa | Style::Phantom | Style::Wraith | Style::Leprechaun
        )
}

/// A hollow garment with vertical folds, an elliptical silhouette and a
/// narrow turned hem. Its length is the hem position, not a capsule's centre.
pub(super) fn draped_cloth(
    b: &mut Builder,
    surface: ClothSurface,
    color: Vec3,
    hem_color: Option<Vec3>,
) {
    draped_cloth_painted(b, surface, &|_, _| color, hem_color);
}

/// A skirt's cloth as its recipe describes it: its colour, a front panel of
/// another and any pattern, painted onto the draped surface.
pub(super) fn skirt_cloth(b: &mut Builder, surface: ClothSurface, skirt: Skirt) {
    let paint = move |at: Vec3, angle: f32| {
        let angle = (angle + PI).rem_euclid(TAU) - PI;
        let base = match skirt.panel {
            Some((color, share)) if angle.abs() < share * PI => color,
            _ => skirt.color,
        };
        match skirt.pattern {
            Pattern::Brocade { .. } => base,
            _ => skirt.pattern.paint(base, at),
        }
    };
    // Embroidery has its own narrow strokes; it must not inherit the much
    // larger quad size used to shade the underlying cloth.
    let thread_surface = surface.clone();
    if let Some(width) = skirt.center_panel {
        royal_parts::center_panel(b, &surface, width, skirt.color, skirt.trim);
    }
    if let Some(trim) = skirt.trim.filter(|_| skirt.split > 0.0) {
        let rows = surface.rows();
        for angle in [0.0, TAU] {
            let path: Vec<_> = (1..=rows)
                .map(|row| surface.point(row as f32 / rows as f32, angle, false) + Vec3::Z * 0.002)
                .collect();
            tube(b, &path, &vec![(0.003, 0.003); path.len()], trim);
        }
    }
    draped_cloth_painted(b, surface, &paint, skirt.trim);
    if let Pattern::Brocade { thread, motif } = skirt.pattern {
        embroidery_parts::embroider(b, &thread_surface, motif, thread);
    }
}

/// Hanging cloth whose colour is painted per quad: `paint` gets a point on
/// the cloth and its angle round, 0 at the front.
pub(super) fn draped_cloth_painted(
    b: &mut Builder,
    surface: ClothSurface,
    paint: &dyn Fn(Vec3, f32) -> Vec3,
    hem_color: Option<Vec3>,
) {
    let ClothSurface {
        length,
        folds,
        split,
        phase,
        ..
    } = surface;
    let sides = 64;
    // Rows no more than a couple of centimetres apart, so a long skirt
    // still follows the round of the hip instead of cutting across it.
    let rows = surface.rows();
    let point = |t: f32, a: f32, inner: bool| surface.point(t, a, inner);
    for row in 0..rows {
        let t0 = row as f32 / rows as f32;
        let t1 = (row + 1) as f32 / rows as f32;
        for side in 0..sides {
            let a0 = side as f32 / sides as f32 * TAU;
            let a1 = (side + 1) as f32 / sides as f32 * TAU;
            let mid = (a0 + a1) * 0.5;
            let y = 0.04 - (length + 0.04) * (t0 + t1) * 0.5;
            let shade = 0.97 + 0.03 * (mid * folds as f32 + phase).cos();
            for inner in [false, true] {
                let inside = if inner {
                    Vec3::new(mid.sin() * 2.0, y, mid.cos() * 2.0)
                } else {
                    Vec3::Y * y
                };
                let color = paint(point((t0 + t1) * 0.5, mid, false), mid);
                b.quad_out(
                    [
                        point(t0, a0, inner),
                        point(t0, a1, inner),
                        point(t1, a1, inner),
                        point(t1, a0, inner),
                    ],
                    inside,
                    color * shade * if inner { 0.7 } else { 1.0 },
                );
            }
        }
    }
    for t in [0.0, 1.0] {
        for side in 0..sides {
            let a0 = side as f32 / sides as f32 * TAU;
            let a1 = (side + 1) as f32 / sides as f32 * TAU;
            b.quad_out(
                [
                    point(t, a0, false),
                    point(t, a1, false),
                    point(t, a1, true),
                    point(t, a0, true),
                ],
                Vec3::Y * (-length * 0.5),
                paint(point(t, a0, false), a0) * 0.83,
            );
        }
    }
    if split > 0.0 {
        // Close the turned front edges of the two panels, leaving an actual
        // opening rather than lifting a continuous bell-shaped hem.
        for a in [0.0f32, TAU] {
            for row in 0..rows {
                let t0 = row as f32 / rows as f32;
                let t1 = (row + 1) as f32 / rows as f32;
                let inside = Vec3::new(if a == 0.0 { 2.0 } else { -2.0 }, -length * 0.5, 0.0);
                b.quad_out(
                    [
                        point(t0, a, false),
                        point(t1, a, false),
                        point(t1, a, true),
                        point(t0, a, true),
                    ],
                    inside,
                    paint(point(t0, a, false), a) * 0.83,
                );
            }
        }
    }
    if let Some(hem_color) = hem_color {
        // The edging shares the actual elliptical hem and its folds; a
        // circular torus floats away from the front of a fitted long robe.
        let top_t = (1.0 - 0.018 / (length + 0.04)).max(0.0);
        for column in 0..sides {
            let a0 = column as f32 * TAU / sides as f32;
            let a1 = (column + 1) as f32 * TAU / sides as f32;
            let raised = |t, angle: f32| {
                point(t, angle, false) + Vec3::new(angle.sin(), 0.0, angle.cos()) * 0.002
            };
            b.quad_out(
                [
                    raised(top_t, a0),
                    raised(top_t, a1),
                    raised(1.0, a1),
                    raised(1.0, a0),
                ],
                Vec3::Y * -length,
                hem_color,
            );
        }
    }
}
