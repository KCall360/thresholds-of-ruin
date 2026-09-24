//! Region-local geometry, independent of rendering and transport.

mod scene;
mod shadowcasting;
mod topology;
pub use scene::SightCell;
mod material;
pub use material::{Material, Terrain};

/// Physical scale shared by horizontal and vertical world cells.
pub const CELL_SIZE_FEET: u32 = 5;

pub use topology::{Direction, Door, Location, Passage, Region, RegionId, World, WorldError};

/// Integer position in a region's local coordinate system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Positive dimensions and an origin; authored interiors start at zero, while
/// their material shells include negative coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    origin: Position,
    width: i32,
    depth: i32,
    height: i32,
}

impl Extent {
    /// Returns width (x), depth (y), and height (z).
    pub fn dimensions(self) -> (i32, i32, i32) {
        (self.width, self.depth, self.height)
    }

    pub fn new(width: i32, depth: i32, height: i32) -> Option<Self> {
        (width > 0 && depth > 0 && height > 0).then_some(Self {
            origin: Position { x: 0, y: 0, z: 0 },
            width,
            depth,
            height,
        })
    }

    pub fn contains(self, position: Position) -> bool {
        let inside = |value: i32, start: i32, size: i32| {
            (0..i64::from(size)).contains(&(i64::from(value) - i64::from(start)))
        };
        inside(position.x, self.origin.x, self.width)
            && inside(position.y, self.origin.y, self.depth)
            && inside(position.z, self.origin.z, self.height)
    }

    /// Add one cell of finite storage on every face, without shifting coordinates.
    pub fn with_shell(self) -> Option<Self> {
        Some(Self {
            origin: Position {
                x: self.origin.x.checked_sub(1)?,
                y: self.origin.y.checked_sub(1)?,
                z: self.origin.z.checked_sub(1)?,
            },
            width: self.width.checked_add(2)?,
            depth: self.depth.checked_add(2)?,
            height: self.height.checked_add(2)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Extent;

    #[test]
    fn regions_require_positive_dimensions_on_every_axis() {
        for invalid in [i32::MIN, -1, 0] {
            assert!(Extent::new(invalid, 1, 1).is_none());
            assert!(Extent::new(1, invalid, 1).is_none());
            assert!(Extent::new(1, 1, invalid).is_none());
        }
        assert!(Extent::new(1, 1, 1).is_some());
    }
}
