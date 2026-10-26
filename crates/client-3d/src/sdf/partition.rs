//! Split painted surface triangles at a binary coverage boundary. Positions
//! remain on the original triangle; the two sides share geometric cut points
//! but sample pigments just inside their own region.
use bevy::prelude::*;

#[derive(Clone, Copy)]
pub(super) struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub shade: f32,
    pub sample: Vec3,
}
impl Vertex {
    fn between(self, other: Self, t: f32) -> Self {
        let position = self.position.lerp(other.position, t);
        Self {
            position,
            sample: position,
            normal: self.normal.lerp(other.normal, t).normalize_or(self.normal),
            shade: self.shade + (other.shade - self.shade) * t,
        }
    }
}

pub(super) fn split(tri: [Vertex; 3], region: &dyn Fn(Vec3) -> bool) -> Option<Vec<[Vertex; 3]>> {
    let mid = std::array::from_fn::<_, 3, _>(|i| tri[i].between(tri[(i + 1) % 3], 0.5));
    let center = mid[0].between(tri[2], 1.0 / 3.0);
    let first = region(tri[0].position);
    if tri
        .iter()
        .chain(mid.iter())
        .chain(std::iter::once(&center))
        .all(|v| region(v.position) == first)
    {
        return None;
    }
    // Sample edge midpoints and center as well as corners, so a cut turning
    // inside a face is not discarded solely because its corners agree.
    let mut result = Vec::new();
    for i in 0..3 {
        clip([tri[i], mid[i], center], region, &mut result);
        clip([mid[i], tri[(i + 1) % 3], center], region, &mut result);
    }
    Some(result)
}

fn clip(tri: [Vertex; 3], region: &dyn Fn(Vec3) -> bool, output: &mut Vec<[Vertex; 3]>) {
    let labels = tri.map(|v| region(v.position));
    if labels.iter().all(|label| *label == labels[0]) {
        output.push(tri);
        return;
    }
    for keep in [false, true] {
        let mut polygon = Vec::with_capacity(4);
        for i in 0..3 {
            let j = (i + 1) % 3;
            if labels[i] == keep {
                polygon.push(tri[i]);
            }
            if labels[i] != labels[j] {
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..20 {
                    let t = (lo + hi) * 0.5;
                    if region(tri[i].position.lerp(tri[j].position, t)) == labels[i] {
                        lo = t;
                    } else {
                        hi = t;
                    }
                }
                let mut cut = tri[i].between(tri[j], (lo + hi) * 0.5);
                cut.sample = tri[i]
                    .position
                    .lerp(tri[j].position, if labels[i] == keep { lo } else { hi });
                polygon.push(cut);
            }
        }
        for i in 1..polygon.len() - 1 {
            let face = [polygon[0], polygon[i], polygon[i + 1]];
            if (face[1].position - face[0].position)
                .cross(face[2].position - face[0].position)
                .length_squared()
                > 1e-20
            {
                output.push(face);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn triangle() -> [Vertex; 3] {
        [Vec3::ZERO, Vec3::X, Vec3::Y].map(|position| Vertex {
            position,
            sample: position,
            normal: Vec3::Z,
            shade: 0.8,
        })
    }
    #[test]
    fn coverage_split_preserves_area_winding_normals_and_region_pigments() {
        let region = |p: Vec3| p.x + p.y * 0.3 < 0.43;
        let faces = split(triangle(), &region).unwrap();
        let mut area = 0.0;
        for face in faces {
            let normal =
                (face[1].position - face[0].position).cross(face[2].position - face[0].position);
            assert!(normal.z > 0.0);
            area += normal.length() * 0.5;
            let label = region(face[0].sample);
            for v in face {
                assert_eq!(region(v.sample), label);
                assert!((v.normal.length() - 1.0).abs() < 1e-6);
                assert!((v.shade - 0.8).abs() < 1e-6);
                assert!((v.position - v.sample).length() < 2e-6);
            }
        }
        assert!(
            (area - 0.5).abs() < 1e-6,
            "no missing or overlapping surface"
        );
        assert!(split(triangle(), &|_| true).is_none());
    }
    #[test]
    fn coverage_split_detects_a_region_inside_matching_corners() {
        let region = |p: Vec3| p.distance(Vec3::new(1.0 / 3.0, 1.0 / 3.0, 0.0)) < 0.2;
        let faces = split(triangle(), &region).unwrap();
        assert!(faces.iter().any(|f| region(f[0].sample)));
        assert!(faces.iter().any(|f| !region(f[0].sample)));
    }
}
