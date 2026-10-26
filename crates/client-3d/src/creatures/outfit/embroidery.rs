//! Small repeatable stitched motifs, independent of body species or outfit.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Motif {
    Leaf,
    Sun,
    Lily,
    Knot,
    Bone,
    Scroll,
}

impl Motif {
    pub fn stitch(self, at: Vec3) -> bool {
        // A stable cloth-space repeat. No world position, animation time or RNG
        // enters the pattern, so it stays attached as the garment moves.
        let u = (at.x / 0.115 + at.z / 0.17 + 0.5).rem_euclid(1.0) - 0.5;
        let v = (at.y / 0.145 + 0.5).rem_euclid(1.0) - 0.5;
        let p = Vec2::new(u, v);
        let stroke = 0.035;
        match self {
            Self::Leaf => {
                (u.abs() < stroke && v.abs() < 0.38)
                    || ((u.abs() * 0.8 + (v.abs() - 0.16).abs() - 0.22).abs() < stroke
                        && u.abs() < 0.29)
            }
            Self::Sun => {
                let r = p.length();
                (r - 0.17).abs() < stroke
                    || ((0.23..0.38).contains(&r) && (v.atan2(u) * 8.0).sin().abs() < 0.2)
            }
            Self::Lily => {
                (u.abs() < stroke && v.abs() < 0.35)
                    || ((p - Vec2::new(u.signum() * 0.16, 0.08)).length() - 0.13).abs() < stroke
                    || (v.abs() < stroke && u.abs() < 0.25)
            }
            Self::Knot => (u.abs() + v.abs() - 0.32).abs() < stroke,
            Self::Bone => {
                (u.abs() < stroke && v.abs() < 0.28)
                    || (Vec2::new(u.abs() - 0.065, v.abs() - 0.27).length() - 0.065).abs() < stroke
            }
            Self::Scroll => {
                (u - (v * 10.0).sin() * 0.16).abs() < stroke
                    || ((p - Vec2::new(0.15, 0.18)).length() - 0.10).abs() < stroke
            }
        }
    }
}
