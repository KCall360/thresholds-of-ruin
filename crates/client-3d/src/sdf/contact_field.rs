//! Exact regional field programs for repeated visual contact queries.
use super::{Sculpt, Step};
use bevy::prelude::*;
use std::collections::HashMap;
use std::sync::RwLock;

const CELL: f32 = 0.1;
const CAPACITY: usize = 256;

/// Immutable source geometry with a bounded cache of conservatively culled
/// programs. Each program preserves source operation order and blend arithmetic.
pub(crate) struct ContactField {
    source: Sculpt,
    cells: RwLock<HashMap<[i32; 3], Sculpt>>,
}
impl ContactField {
    pub(crate) fn new(source: Sculpt) -> Self {
        Self {
            source,
            cells: RwLock::new(HashMap::new()),
        }
    }
    pub(crate) fn distance(&self, p: Vec3) -> f32 {
        let grid = p / CELL;
        if !grid.is_finite() || grid.abs().max_element() > 1_000_000.0 {
            return self.source.distance(p);
        }
        let key = grid.floor().as_ivec3().to_array();
        let cells = self.cells.read().expect("contact cache");
        if let Some(local) = cells.get(&key) {
            return local.distance(p);
        }
        if cells.len() >= CAPACITY {
            return self.source.distance(p);
        }
        drop(cells);
        let center = (IVec3::from_array(key).as_vec3() + Vec3::splat(0.5)) * CELL;
        // Include a rounding margin at cell faces and around finite differences.
        let radius = CELL * 0.5 * 3.0f32.sqrt()
            + 0.0001
            + (center.abs().max_element() + CELL) * f32::EPSILON * 4.0;
        let local = localize(&self.source, center, radius);
        let value = local.distance(p);
        let mut cells = self.cells.write().expect("contact cache");
        if cells.len() < CAPACITY {
            cells.entry(key).or_insert(local);
        }
        value
    }
}
impl std::ops::Deref for ContactField {
    type Target = Sculpt;
    fn deref(&self) -> &Sculpt {
        &self.source
    }
}

fn localize(source: &Sculpt, center: Vec3, radius: f32) -> Sculpt {
    let mut indices = Vec::new();
    source.steps_near(center, radius, &mut indices);
    Sculpt {
        steps: indices
            .into_iter()
            .map(|i| match &source.steps[i] {
                Step::Union(child, blend) => Step::Union(localize(child, center, radius), *blend),
                step => step.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdf::Shape::*;
    fn layers() -> Sculpt {
        let mut body = Sculpt::default();
        body.add(Ellipsoid(Vec3::ZERO, Vec3::new(0.3, 0.5, 0.2)), 0.0);
        body.add(
            Capsule(
                Vec3::new(-0.2, 0.3, 0.0),
                Vec3::new(0.2, 0.3, 0.0),
                0.1,
                0.05,
            ),
            0.04,
        );
        body.carve(
            Ellipsoid(Vec3::new(0.0, 0.2, 0.2), Vec3::splat(0.12)),
            0.025,
        );
        body.inflate(0.007);
        let mut armor = Sculpt::default();
        armor.add(Ellipsoid(Vec3::ZERO, Vec3::new(0.32, 0.4, 0.23)), 0.0);
        armor.carve(Beyond(Vec3::Y, 0.28), 0.0);
        armor.carve(Beyond(Vec3::NEG_Y, 0.2), 0.015);
        body.union(armor, 0.008);
        for x in 1..12 {
            let mut distant = Sculpt::default();
            distant.add(Ellipsoid(Vec3::X * x as f32, Vec3::splat(0.1)), 0.0);
            distant.inflate(0.01);
            body.union(distant, 0.02);
        }
        body
    }
    fn count(s: &Sculpt) -> usize {
        s.steps
            .iter()
            .map(|step| {
                1 + match step {
                    Step::Union(child, _) => count(child),
                    _ => 0,
                }
            })
            .sum()
    }
    #[test]
    fn cached_program_matches_nested_blends_cuts_and_cell_boundaries() {
        let source = layers();
        for x in -8..=8 {
            for y in -6..=6 {
                for z in -5..=5 {
                    let base = Vec3::new(x as f32, y as f32, z as f32) * 0.1;
                    let field = ContactField::new(source.clone());
                    for offset in [-0.00001, 0.0, 0.00001, 0.025, 0.075, 0.08, 0.08001] {
                        let p = base + Vec3::splat(offset);
                        assert_eq!(
                            field.distance(p).to_bits(),
                            source.distance(p).to_bits(),
                            "field changed at {p:?}"
                        );
                    }
                }
            }
        }
        // A fresh near-body query must evaluate fewer operations, not merely
        // cache a copy of the full source program.
        let nearby = ContactField::new(source.clone());
        nearby.distance(Vec3::ZERO);
        let cells = nearby.cells.read().unwrap();
        assert!(count(cells.values().next().unwrap()) < count(&source) / 2);
    }
    #[test]
    fn cache_capacity_and_large_coordinates_fall_back_to_original_field() {
        let source = layers();
        let field = ContactField::new(source.clone());
        for i in 0..CAPACITY + 20 {
            let p = Vec3::X * i as f32 * 0.2;
            assert_eq!(field.distance(p).to_bits(), source.distance(p).to_bits());
        }
        assert_eq!(field.cells.read().unwrap().len(), CAPACITY);
        let p = Vec3::new(1e8, 0.0, 0.0);
        assert_eq!(field.distance(p).to_bits(), source.distance(p).to_bits());
    }
    #[test]
    fn regional_bounds_include_coordinate_rounding() {
        let center = Vec3::new(10_000.0, -10_000.0, 10_000.0);
        let mut source = Sculpt::default();
        source.add(Ellipsoid(center, Vec3::splat(0.1)), 0.0);
        source.add(Ellipsoid(center + Vec3::X * 0.15, Vec3::splat(0.1)), 0.02);
        let field = ContactField::new(source.clone());
        for x in -8..=8 {
            for y in -8..=8 {
                for z in -8..=8 {
                    let p = center + Vec3::new(x as f32, y as f32, z as f32) * 0.025;
                    assert_eq!(field.distance(p).to_bits(), source.distance(p).to_bits());
                }
            }
        }
    }
}
