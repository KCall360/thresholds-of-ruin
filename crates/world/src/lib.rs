//! Region-local geometry, independent of rendering and transport.

/// Integer position in a region's local coordinate system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

/// Positive dimensions of a region, with a zero-inclusive local origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Extent {
    width: i32,
    depth: i32,
    height: i32,
}

impl Extent {
    pub fn new(width: i32, depth: i32, height: i32) -> Option<Self> {
        (width > 0 && depth > 0 && height > 0).then_some(Self {
            width,
            depth,
            height,
        })
    }

    pub fn contains(self, position: Position) -> bool {
        (0..self.width).contains(&position.x)
            && (0..self.depth).contains(&position.y)
            && (0..self.height).contains(&position.z)
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
