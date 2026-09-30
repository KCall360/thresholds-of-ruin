//! Chart translation copied from `MapMemory::observe`'s anchor rule.
//! Does not read `CellChanges.shift`.
use std::collections::BTreeMap;
use tor_protocol::{Observation, Position};

/// Delta applied to remembered cells, or `None` when the old chart was cleared.
pub fn chart_shift(before: &Observation, after: &Observation) -> Option<(i64, i64, i64)> {
    let mut previous: BTreeMap<&str, Vec<Position>> = BTreeMap::new();
    for cell in &before.visible_cells {
        previous
            .entry(cell.key.as_str())
            .or_default()
            .push(cell.position);
    }
    let mut current: BTreeMap<&str, Vec<Position>> = BTreeMap::new();
    for cell in &after.visible_cells {
        current
            .entry(cell.key.as_str())
            .or_default()
            .push(cell.position);
    }
    let mut shift = None;
    let mut conflict = false;
    for (key, positions) in &current {
        if let Some(old) = previous.get(key) {
            if let ([before], [after]) = (old.as_slice(), positions.as_slice()) {
                let delta = (
                    i64::from(after.x) - i64::from(before.x),
                    i64::from(after.y) - i64::from(before.y),
                    i64::from(after.z) - i64::from(before.z),
                );
                if shift.is_some_and(|known| known != delta) {
                    conflict = true;
                }
                shift = Some(delta);
            }
        }
    }
    shift.filter(|_| !conflict)
}

/// Move both origins, or move neither. `None` means the caller leaves both.
pub fn shift_origin(origin_x: i32, origin_y: i32, dx: i64, dy: i64) -> Option<(i32, i32)> {
    let x = i32::try_from(i64::from(origin_x).checked_add(dx)?).ok()?;
    let y = i32::try_from(i64::from(origin_y).checked_add(dy)?).ok()?;
    Some((x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::*;

    fn cell(key: &str, x: i32, y: i32, z: i32) -> CellView {
        CellView {
            asset: None,
            door: None,
            material: String::new(),
            key: key.into(),
            stairs_up: false,
            stairs_down: false,
            position: Position { x, y, z },
            wall: false,
            place_hint: false,
        }
    }

    fn view(cells: Vec<CellView>) -> Observation {
        Observation {
            combat: None,
            motion: None,
            places: Vec::new(),
            actor: ActorId(1),
            tick: 0,
            position: Position { x: 0, y: 0, z: 0 },
            visible_cells: cells,
            ground_items: Vec::new(),
            inventory: Vec::new(),
            visible_actors: Vec::new(),
            ready: true,
        }
    }

    #[test]
    fn unique_anchor_returns_the_delta_and_a_still_key_is_zero() {
        let before = view(vec![cell("a", 0, 1, 0), cell("only-old", 4, 4, 0)]);
        let after = view(vec![cell("a", -1, 1, 2), cell("only-new", 9, 9, 0)]);
        assert_eq!(chart_shift(&before, &after), Some((-1, 0, 2)));
        let still = view(vec![cell("a", 3, 3, 3)]);
        assert_eq!(chart_shift(&still, &still), Some((0, 0, 0)));
    }

    #[test]
    fn disagreeing_anchors_and_no_shared_unique_key_return_none() {
        let before = view(vec![cell("a", 0, 0, 0), cell("b", 0, 0, 0)]);
        let after = view(vec![cell("a", 1, 0, 0), cell("b", 2, 0, 0)]);
        assert_eq!(chart_shift(&before, &after), None);
        let disjoint_before = view(vec![cell("a", 0, 0, 0)]);
        let disjoint_after = view(vec![cell("b", 1, 0, 0)]);
        assert_eq!(chart_shift(&disjoint_before, &disjoint_after), None);
        assert_eq!(chart_shift(&view(vec![]), &view(vec![])), None);
    }

    #[test]
    fn a_key_seen_more_than_once_is_not_an_anchor() {
        let before = view(vec![
            cell("dup", 0, 0, 0),
            cell("dup", 5, 0, 0),
            cell("one", 1, 0, 0),
        ]);
        let after = view(vec![cell("dup", 0, 0, 0), cell("one", 4, 2, 0)]);
        assert_eq!(chart_shift(&before, &after), Some((3, 2, 0)));
        let only_dup = view(vec![cell("dup", 0, 0, 0), cell("dup", 1, 0, 0)]);
        assert_eq!(
            chart_shift(&only_dup, &view(vec![cell("dup", 2, 0, 0)])),
            None
        );
    }

    #[test]
    fn origin_moves_when_the_sum_fits_even_if_the_delta_does_not() {
        assert_eq!(
            shift_origin(-1_000_000_000, 5, 3_000_000_000, 0),
            Some((2_000_000_000, 5))
        );
    }

    #[test]
    fn a_sum_that_does_not_fit_on_either_axis_shifts_neither() {
        assert_eq!(shift_origin(2_000_000_000, 0, 2_000_000_000, 0), None);
        assert_eq!(shift_origin(0, i32::MAX, 0, 1), None);
        assert_eq!(shift_origin(i32::MAX, 0, i64::MAX, 0), None);
    }
}
