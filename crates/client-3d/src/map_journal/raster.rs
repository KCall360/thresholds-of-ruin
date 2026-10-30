//! Bounded raster of disclosed current-level terrain; all coordinate arithmetic is widened.
use crate::minimap::terrain_kind;
use std::collections::BTreeMap;
use tor_client_3d_model::View;
pub(super) const SIZE: usize = 512;
#[derive(Clone, Copy)]
pub(super) struct Fit {
    left: i64,
    top: i64,
    span: f64,
}
impl Fit {
    pub(super) fn normalized(self, x: i32, y: i32) -> [f32; 2] {
        let (x, y) = self.point(x, y);
        [
            (x as f32 + 0.5) / SIZE as f32,
            (y as f32 + 0.5) / SIZE as f32,
        ]
    }
    pub(super) fn pick(self, uv: [f32; 2]) -> Option<(i32, i32)> {
        if self.span > (SIZE - 16) as f64
            || uv.iter().any(|v| !v.is_finite() || !(0.0..1.0).contains(v))
        {
            return None;
        }
        let at = |value: f32, start: i64| {
            let pixel = f64::from(value) * SIZE as f64;
            if !(8.0..(SIZE - 8) as f64).contains(&pixel) {
                return None;
            }
            i32::try_from(start + (((pixel - 8.) * self.span / (SIZE - 16) as f64).floor() as i64))
                .ok()
        };
        Some((at(uv[0], self.left)?, at(uv[1], self.top)?))
    }

    fn point(self, x: i32, y: i32) -> (usize, usize) {
        let at = |p: i64, start: i64| {
            ((((p - start) as f64 + 0.5) / self.span * (SIZE - 16) as f64) + 8.)
                .clamp(0., (SIZE - 1) as f64) as usize
        };
        (at(i64::from(x), self.left), at(i64::from(y), self.top))
    }
}
pub(super) fn empty() -> Vec<u8> {
    [8, 10, 14, 255].repeat(SIZE * SIZE)
}
#[cfg(test)]
pub(super) fn draw(view: &View) -> Vec<u8> {
    render(view, None).0
}
pub(super) fn render(view: &View, selected: Option<tor_protocol::Position>) -> (Vec<u8>, Fit) {
    let mut cells = BTreeMap::new();
    for cell in view.terrain() {
        let dz = i64::from(cell.position.z) - i64::from(view.feet.z);
        if !(-1..=1).contains(&dz) {
            continue;
        }
        let Some(kind) = terrain_kind(&cell, dz as i32) else {
            continue;
        };
        let key = (cell.position.x, cell.position.y);
        let candidate = (cell.seen, kind);
        cells
            .entry(key)
            .and_modify(|old| {
                if candidate > *old {
                    *old = candidate;
                }
            })
            .or_insert(candidate);
    }
    let mut low = (i64::from(view.feet.x), i64::from(view.feet.y));
    let mut high = low;
    for &(x, y) in cells.keys() {
        low.0 = low.0.min(i64::from(x));
        low.1 = low.1.min(i64::from(y));
        high.0 = high.0.max(i64::from(x));
        high.1 = high.1.max(i64::from(y));
    }
    let span = (high.0 - low.0 + 3).max(high.1 - low.1 + 3) as f64;
    let fit = Fit {
        left: low.0 - 1 - ((span - (high.0 - low.0 + 3) as f64) * 0.5) as i64,
        top: low.1 - 1 - ((span - (high.1 - low.1 + 3) as f64) * 0.5) as i64,
        span,
    };
    let mut data = empty();
    let size = (((SIZE - 16) as f64 / span).floor() as usize).max(1);
    for (&(x, y), &(seen, kind)) in &cells {
        let (cx, cy) = fit.point(x, y);
        let color = kind
            .color()
            .map(|c| (c * if seen { 255. } else { 110. }) as u8);
        square(&mut data, cx, cy, size.saturating_sub(1).max(1), color);
    }
    for item in &view.items {
        if (i64::from(item.position.z) - i64::from(view.feet.z)).abs() <= 1 {
            let (x, y) = fit.point(item.position.x, item.position.y);
            if cells.contains_key(&(item.position.x, item.position.y)) {
                square(&mut data, x, y, 3, [242, 199, 76]);
            }
        }
    }
    let (x, y) = fit.point(view.feet.x, view.feet.y);
    square(&mut data, x, y, 7, [89, 242, 216]);
    if let Some(selected) = selected {
        let (x, y) = fit.point(selected.x, selected.y);
        for offset in 0..=6usize {
            let tail = 6 - offset;
            for (px, py) in [
                (x.saturating_sub(offset), y.saturating_sub(tail)),
                (x + offset, y.saturating_sub(tail)),
                (x.saturating_sub(offset), y + tail),
                (x + offset, y + tail),
            ] {
                square(
                    &mut data,
                    px.min(SIZE - 1),
                    py.min(SIZE - 1),
                    1,
                    [255, 215, 110],
                );
            }
        }
    }
    (data, fit)
}
fn square(data: &mut [u8], x: usize, y: usize, size: usize, color: [u8; 3]) {
    let half = size / 2;
    for py in y.saturating_sub(half)..=(y + half).min(SIZE - 1) {
        for px in x.saturating_sub(half)..=(x + half).min(SIZE - 1) {
            let at = (py * SIZE + px) * 4;
            data[at..at + 3].copy_from_slice(&color);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::*;
    fn cell(x: i32, y: i32, z: i32) -> CellView {
        CellView {
            key: format!("{x},{y},{z}"),
            position: Position { x, y, z },
            wall: false,
            stairs_up: false,
            stairs_down: false,
            door: None,
            trap: None,
            lit: true,
            object_presentation: None,
            material: String::new(),
            asset: None,
            place_hint: false,
        }
    }
    #[test]
    fn map_fits_extreme_coordinates_and_ignores_undisclosed_other_levels() {
        let mut view = crate::test_support::view(1);
        view.visible = vec![cell(i32::MIN, 0, 0), cell(i32::MAX, 0, 0)];
        let data = draw(&view);
        assert_eq!(data.len(), SIZE * SIZE * 4);
        assert!(data
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] == [158, 132, 96]));
        view.visible.push(cell(0, i32::MAX, i32::MAX));
        assert_eq!(draw(&view), data);
    }
    #[test]
    fn empty_map_marks_only_the_observer_without_fabricating_terrain() {
        let view = crate::test_support::view(1);
        let data = draw(&view);
        assert_eq!(
            data.as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[..3] == [89, 242, 216])
                .count(),
            49
        );
        assert!(data
            .as_chunks::<4>()
            .0
            .iter()
            .all(|p| p[..3] == [8, 10, 14] || p[..3] == [89, 242, 216]));
    }
    #[test]
    fn visible_and_remembered_cells_are_distinct_and_current_terrain_wins() {
        let mut view = crate::test_support::view(1);
        view.visible = vec![cell(1, 0, 0)];
        view.memory = vec![RememberedMapCell {
            object_presentation: None,
            key: "known".into(),
            position: Position { x: -1, y: 0, z: 0 },
            wall: false,
            material: "stone".into(),
            asset: None,
            place_hint: false,
            stairs_up: false,
            stairs_down: false,
            door: None,
            trap: None,
            ground_items: vec![],
        }];
        let pixels = draw(&view);
        assert!(pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] == [158, 132, 96]));
        assert!(pixels
            .as_chunks::<4>()
            .0
            .iter()
            .any(|p| p[..3] == [68, 57, 41]));
        let mut stale = view.memory[0].clone();
        stale.position.x = 1;
        stale.stairs_down = true;
        view.memory.push(stale);
        assert_eq!(
            draw(&view),
            pixels,
            "current disclosed floor wins over stale remembered stairs"
        );
    }
    #[test]
    fn picking_round_trips_exact_tiles_and_rejects_ambiguous_or_invalid_maps() {
        let mut view = crate::test_support::view(1);
        view.visible = vec![cell(-2, -3, 0), cell(4, 5, 0)];
        let (_, fit) = render(&view, None);
        for at in &view.visible {
            assert_eq!(
                fit.pick(fit.normalized(at.position.x, at.position.y)),
                Some((at.position.x, at.position.y))
            );
        }
        assert_eq!(fit.pick([0., 0.]), None);
        assert_eq!(fit.pick([f32::NAN, 0.5]), None);
        view.visible.push(cell(i32::MAX, 0, 0));
        let (_, wide) = render(&view, None);
        assert_eq!(wide.pick([0.5, 0.5]), None);
    }
}
