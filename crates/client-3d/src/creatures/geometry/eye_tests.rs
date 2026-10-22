//! Mesh contracts for the visible eye and its attachment to the sculpted head.
use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::{BTreeMap, VecDeque};

const IRIS: Vec3 = Vec3::new(0.12, 0.43, 0.69);
const WHITE: Vec3 = Vec3::new(0.82, 0.81, 0.76);
const KINDS: [CreatureModel; 4] = [
    CreatureModel::Human,
    CreatureModel::Orc,
    CreatureModel::Vampire,
    CreatureModel::Medusa,
];

#[derive(Clone, Copy)]
struct Triangle {
    p: [Vec3; 3],
    color: Vec3,
    glow: bool,
}

fn linear(color: Vec3) -> Vec3 {
    let c = Color::srgb(color.x, color.y, color.z).to_linear();
    Vec3::new(c.red, c.green, c.blue)
}

fn painted(tris: &[Triangle], color: Vec3) -> Vec<Triangle> {
    let color = linear(color);
    let result: Vec<_> = tris
        .iter()
        .copied()
        .filter(|tri| !tri.glow && tri.color.distance(color) < 1e-5)
        .collect();
    assert!(!result.is_empty(), "missing pigment witness {color:?}");
    result
}

fn emit(parts: Parts) -> Vec<Triangle> {
    let mut material_assets = Assets::<StandardMaterial>::default();
    let materials = crate::props::PropMaterials::new(&mut material_assets);
    let glow = materials.glow(Accent::Danger).id();
    let mut meshes = Assets::<Mesh>::default();
    let model = parts.finish(&mut meshes, &materials);
    let mut result = Vec::new();
    for (handle, material) in model.0 {
        let mesh = meshes.get(&handle).unwrap();
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("eye positions")
        };
        let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("eye pigments")
        };
        let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
        for tri in indices.as_chunks::<3>().0 {
            let p = tri.map(|i| Vec3::from_array(points[i]));
            assert!(p.iter().all(|p| p.is_finite()), "nonfinite eye mesh");
            assert!(
                (p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1e-24,
                "degenerate eye triangle {p:?}"
            );
            result.push(Triangle {
                p,
                color: Vec3::from_array(colors[tri[0]][..3].try_into().unwrap()),
                glow: material.id() == glow,
            });
        }
    }
    assert!(!result.is_empty(), "missing emitted geometry");
    result
}

fn fixture(
    kind: CreatureModel,
    build: u8,
    glowing: bool,
    with_head: bool,
) -> (Look, Vec<Triangle>, Vec<Triangle>) {
    let mut look = Look::with_choice(
        kind,
        42,
        Some(Variant {
            woman: true,
            build,
            ..default()
        }),
    );
    look.eyes = glowing.then_some(Accent::Danger);
    let head = face_shape(&look, true).form;
    let mut parts = Parts::default();
    parts.soften();
    soft_eyes(
        &mut parts,
        Vec3::new(0.0, 0.16, 0.088),
        0.038,
        0.0158,
        IRIS,
        &look,
        &head,
    );
    let eyes = emit(parts);
    let head = if with_head {
        let mut parts = Parts::default();
        parts.soften();
        // Match human_face's bounds and resolution. A unique pigment keeps
        // anatomy independent from all eye material/pigment classifications.
        head.mesh(
            &mut parts.matte,
            Vec3::new(-0.125, 0.0, -0.13),
            Vec3::new(0.125, 0.3, 0.14),
            0.0055,
            &|_, _| Vec3::new(0.61, 0.09, 0.47),
        );
        let (eye_low, eye_high) = bounds(&eyes);
        emit(parts)
            .into_iter()
            .filter(|tri| {
                // Ray pruning only: triangles outside the emitted eye assembly's
                // projected box cannot support or occlude any queried eye point.
                let low = tri.p[0].min(tri.p[1]).min(tri.p[2]);
                let high = tri.p[0].max(tri.p[1]).max(tri.p[2]);
                low.x <= eye_high.x
                    && high.x >= eye_low.x
                    && low.y <= eye_high.y
                    && high.y >= eye_low.y
            })
            .collect()
    } else {
        Vec::new()
    };
    (look, eyes, head)
}

fn key(p: Vec3) -> [i32; 3] {
    p.to_array().map(|x| (x * 1_000_000.0).round() as i32)
}

fn signature(tris: &[Triangle]) -> BTreeMap<[[i32; 3]; 4], usize> {
    let mut result = BTreeMap::new();
    for tri in tris {
        let mut vertices = tri.p.map(key);
        vertices.sort();
        let normal = (tri.p[1] - tri.p[0]).cross(tri.p[2] - tri.p[0]).normalize();
        *result
            .entry([vertices[0], vertices[1], vertices[2], key(normal)])
            .or_default() += 1;
    }
    result
}

// A front ray through the actual projected triangle; no analytic eye surface.
fn ray(tri: Triangle, at: Vec2) -> Option<f32> {
    let [a, b, c] = tri.p.map(Vec3::truncate);
    if at.cmplt(a.min(b).min(c) - Vec2::splat(1e-7)).any()
        || at.cmpgt(a.max(b).max(c) + Vec2::splat(1e-7)).any()
    {
        return None;
    }
    let cross = |a: Vec2, b: Vec2| a.x * b.y - a.y * b.x;
    let d = cross(b - a, c - a);
    if d.abs() < 1e-12 {
        return None;
    }
    let v = cross(at - a, c - a) / d;
    let w = cross(b - a, at - a) / d;
    (v >= -1e-5 && w >= -1e-5 && v + w <= 1.00001)
        .then_some(tri.p[0].z * (1.0 - v - w) + tri.p[1].z * v + tri.p[2].z * w)
}

fn front(tris: &[Triangle], at: Vec2) -> Option<f32> {
    tris.iter()
        .filter_map(|tri| ray(*tri, at))
        .max_by(f32::total_cmp)
}

fn bounds(tris: &[Triangle]) -> (Vec3, Vec3) {
    tris.iter().flat_map(|tri| tri.p).fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(low, high), p| (low.min(p), high.max(p)),
    )
}

fn side(tris: &[Triangle], sign: f32) -> Vec<Triangle> {
    tris.iter()
        .copied()
        .filter(|tri| tri.p.iter().all(|p| p.x * sign > 0.0))
        .collect()
}

fn samples([a, b, c]: [Vec3; 3]) -> [Vec3; 7] {
    [
        a,
        b,
        c,
        (a + b) * 0.5,
        (b + c) * 0.5,
        (c + a) * 0.5,
        (a + b + c) / 3.0,
    ]
}

// Reconstruct the front surface's perimeter. Closed backs and edge walls do
// not participate, so each returned edge belongs to the visible patch once.
fn boundary(tris: &[Triangle]) -> Vec<[Vec3; 2]> {
    let mut edges: BTreeMap<[[i32; 3]; 2], (usize, [Vec3; 2])> = BTreeMap::new();
    for tri in tris {
        if (tri.p[1] - tri.p[0]).cross(tri.p[2] - tri.p[0]).z <= 1e-12 {
            continue;
        }
        for edge in [
            [tri.p[0], tri.p[1]],
            [tri.p[1], tri.p[2]],
            [tri.p[2], tri.p[0]],
        ] {
            let mut k = edge.map(key);
            k.sort();
            let entry = edges.entry(k).or_insert((0, edge));
            entry.0 += 1;
        }
    }
    let edges: Vec<_> = edges
        .into_values()
        .filter_map(|(n, edge)| (n == 1).then_some(edge))
        .collect();
    assert!(edges.len() >= 8, "missing emitted patch perimeter");
    edges
}

fn edge_distance(p: Vec3, [a, b]: [Vec3; 2], projected: bool) -> f32 {
    let (p, a, b) = if projected {
        (p.with_z(0.0), a.with_z(0.0), b.with_z(0.0))
    } else {
        (p, a, b)
    };
    let ab = b - a;
    let t = if ab.length_squared() > 1e-16 {
        ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    p.distance(a + ab * t)
}

fn distance_to_edges(p: Vec3, edges: &[[Vec3; 2]], projected: bool) -> f32 {
    edges
        .iter()
        .map(|edge| edge_distance(p, *edge, projected))
        .fold(f32::INFINITY, f32::min)
}

fn upper_boundary_at(edges: &[[Vec3; 2]], x: f32) -> f32 {
    edges
        .iter()
        .filter_map(|[a, b]| {
            if x < a.x.min(b.x) - 1e-7 || x > a.x.max(b.x) + 1e-7 || (b.x - a.x).abs() < 1e-8 {
                return None;
            }
            let t = ((x - a.x) / (b.x - a.x)).clamp(0.0, 1.0);
            Some(a.y + (b.y - a.y) * t)
        })
        .max_by(f32::total_cmp)
        .expect("upper emitted opening boundary")
}

// A corner cap transitions from the white tip to the seated outer lid tip.
// Recover that whole subdivided bridge from the emitted perimeters, rather
// than treating its interior as another outer attachment edge.
fn corner_bridges(white: &[Triangle], lid: &[Triangle]) -> Vec<[Vec3; 2]> {
    let mut bridges = Vec::new();
    for sign in [-1.0, 1.0] {
        let tips = |tris: &[Triangle]| {
            let mut points: Vec<_> = boundary(&side(tris, sign)).into_iter().flatten().collect();
            points.sort_by(|a, b| a.x.total_cmp(&b.x));
            [*points.first().unwrap(), *points.last().unwrap()]
        };
        let inner = tips(white);
        let outer = tips(lid);
        bridges.extend([[inner[0], outer[0]], [inner[1], outer[1]]]);
    }
    bridges
}

// Dark lashes share the pupil pigment. Select the connected component hit at
// the opening center instead of assuming a coordinate box excludes lashes.
fn pupil(eyes: &[Triangle], white: &[Triangle]) -> Vec<Triangle> {
    let dark = painted(eyes, DARK);
    let (low, high) = bounds(white);
    let at = (low + high).truncate() * 0.5;
    let seed = dark
        .iter()
        .enumerate()
        .filter_map(|(i, tri)| ray(*tri, at).map(|z| (i, z)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .expect("visible central pupil witness")
        .0;
    let mut vertices: BTreeMap<[i32; 3], Vec<usize>> = BTreeMap::new();
    for (i, tri) in dark.iter().enumerate() {
        for p in tri.p {
            vertices.entry(key(p)).or_default().push(i);
        }
    }
    let mut seen = vec![false; dark.len()];
    let mut pending = VecDeque::from([seed]);
    seen[seed] = true;
    while let Some(i) = pending.pop_front() {
        for p in dark[i].p {
            for &j in &vertices[&key(p)] {
                if !seen[j] {
                    seen[j] = true;
                    pending.push_back(j);
                }
            }
        }
    }
    dark.into_iter()
        .zip(seen)
        .filter_map(|(tri, seen)| seen.then_some(tri))
        .collect()
}

fn pupil_center_radius(tris: &[Triangle]) -> (Vec2, f32) {
    let (low, high) = bounds(tris);
    let tips: Vec<_> = tris
        .iter()
        .flat_map(|tri| tri.p)
        .filter(|p| (p.x - low.x).abs() < 1e-6 || (p.x - high.x).abs() < 1e-6)
        .collect();
    let y = tips.iter().map(|p| p.y).sum::<f32>() / tips.len() as f32;
    (Vec2::new((low.x + high.x) * 0.5, y), (high.x - low.x) * 0.5)
}

#[test]
fn art_contract_eye_glow_owns_exactly_the_emitted_iris() {
    for kind in KINDS {
        for build in 0..3 {
            let (_, plain, _) = fixture(kind, build, false, false);
            let (_, glowing, _) = fixture(kind, build, true, false);
            assert!(plain.iter().all(|tri| !tri.glow));
            let iris = painted(&plain, IRIS);
            let light: Vec<_> = glowing.iter().copied().filter(|tri| tri.glow).collect();
            assert!(
                !light.is_empty(),
                "{kind:?} build {build}: missing glowing iris"
            );
            assert!(
                signature(&iris) == signature(&light),
                "{kind:?} build {build}: glow geometry differs from normal iris"
            );
            let rest: Vec<_> = plain
                .iter()
                .copied()
                .filter(|tri| tri.color.distance(linear(IRIS)) >= 1e-5)
                .collect();
            let matte: Vec<_> = glowing.iter().copied().filter(|tri| !tri.glow).collect();
            assert!(
                signature(&rest) == signature(&matte),
                "{kind:?} build {build}: glow changed or duplicated non-iris geometry"
            );
            for sign in [-1.0, 1.0] {
                let area: f32 = side(&light, sign)
                    .iter()
                    .map(|tri| (tri.p[1] - tri.p[0]).cross(tri.p[2] - tri.p[0]).z.max(0.0) * 0.5)
                    .sum();
                assert!(
                    area > 1e-5,
                    "{kind:?} build {build}: iris lacks visible area on side {sign}: {area}"
                );
            }
        }
    }
}

#[test]
fn art_contract_adult_pupils_keep_circular_unclipped_arcs_except_medusa() {
    let mut failures = Vec::new();
    for kind in KINDS {
        for build in 0..3 {
            let (_, eyes, _) = fixture(kind, build, false, false);
            let white = painted(&eyes, WHITE);
            for sign in [-1.0, 1.0] {
                let component = pupil(&eyes, &side(&white, sign));
                let (center, radius) = pupil_center_radius(&component);
                let (_, high) = bounds(&component);
                assert!(
                    radius > 0.002 && high.y - center.y > 0.002,
                    "{kind:?} build {build}: missing pupil extent"
                );
                let ratio = (high.y - center.y) / radius;
                // The horizontal endpoints and upper arc are unclipped;
                // a clipped lower edge must not make a circle appear elliptical.
                let allowed = if kind == CreatureModel::Medusa {
                    1.3..=1.85
                } else {
                    0.9..=1.1
                };
                if !allowed.contains(&ratio) {
                    failures.push(format!("{kind:?} build {build} side {sign}: pupil upper radius / transverse radius={ratio:.4}, radius={radius:.6}, upper={:.6}", high.y - center.y));
                }
                if kind != CreatureModel::Medusa {
                    let mut arc_samples = 0;
                    let mut arc_error = 0.0_f32;
                    for edge in boundary(&component) {
                        for p in [edge[0], (edge[0] + edge[1]) * 0.5, edge[1]] {
                            if p.y >= center.y && (p.x - center.x).abs() <= radius * 0.8 {
                                let relative = p.truncate().distance(center) / radius;
                                arc_error = arc_error.max((relative - 1.0).abs());
                                arc_samples += 1;
                            }
                        }
                    }
                    assert!(
                        arc_samples >= 4,
                        "{kind:?} build {build} side {sign}: missing unclipped pupil arc"
                    );
                    if arc_error > 0.12 {
                        failures.push(format!("{kind:?} build {build} side {sign}: noncircular upper pupil arc, maximum relative radial error={arc_error:.4}"));
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn art_contract_emitted_eye_lids_join_the_opening_and_head_without_hiding_pigments() {
    for kind in KINDS {
        for build in 0..3 {
            for glowing in [false, true] {
                let (look, eyes, head) = fixture(kind, build, glowing, true);
                let white = painted(&eyes, if glowing { look.skin * 0.3 } else { WHITE });
                let iris = if glowing {
                    eyes.iter().copied().filter(|tri| tri.glow).collect()
                } else {
                    painted(&eyes, IRIS)
                };
                let upper = painted(&eyes, look.skin.lerp(Vec3::new(0.55, 0.36, 0.42), 0.12));
                let lower = painted(&eyes, look.skin * 0.98);
                let white_boundary = boundary(&white);
                let lid_boundary: Vec<_> = boundary(&upper)
                    .into_iter()
                    .chain(boundary(&lower))
                    .collect();
                for edge in &white_boundary {
                    for p in [edge[0], (edge[0] + edge[1]) * 0.5, edge[1]] {
                        let gap = distance_to_edges(p, &lid_boundary, false);
                        assert!(gap <= 0.0012, "{kind:?} build {build} glow {glowing}: open eye/lid seam gap={gap:.6} at {p:?}");
                    }
                }
                for (name, lid) in [("upper", &upper), ("lower", &lower)] {
                    let mut outer_hits = 0;
                    let bridges = corner_bridges(&white, lid);
                    for [_, tip] in &bridges {
                        let skin = front(&head, tip.truncate()).expect("outer lid corner support");
                        let gap = tip.z - skin;
                        assert!((-0.00025..=0.0035).contains(&gap),
                            "{kind:?} build {build} glow {glowing} {name}: outer corner gap={gap:.6} at {tip:?}");
                    }
                    for edge in boundary(lid) {
                        let middle = (edge[0] + edge[1]) * 0.5;
                        if distance_to_edges(middle, &white_boundary, true) < 1e-5 {
                            continue;
                        }
                        if bridges.iter().any(|bridge| {
                            edge.iter().all(|p| edge_distance(*p, *bridge, true) < 1e-6)
                        }) {
                            for p in [edge[0], middle, edge[1]] {
                                let skin =
                                    front(&head, p.truncate()).expect("corner bridge skin support");
                                assert!(p.z >= skin - 0.00025,
                                    "{kind:?} build {build} {name}: corner bridge sinks into head at {p:?}");
                            }
                            continue;
                        }
                        for p in [edge[0], middle, edge[1]] {
                            let skin = front(&head, p.truncate())
                                .expect("lid outer edge has emitted skin support");
                            let gap = p.z - skin;
                            assert!((-0.00025..=0.0035).contains(&gap), "{kind:?} build {build} glow {glowing} {name}: outer lid gap={gap:.6} at {p:?}");
                            outer_hits += 1;
                        }
                    }
                    assert!(
                        outer_hits >= 24,
                        "{kind:?} build {build} {name}: missing outer attachment coverage"
                    );
                    let mut surface_hits = 0;
                    for tri in lid
                        .iter()
                        .filter(|tri| (tri.p[1] - tri.p[0]).cross(tri.p[2] - tri.p[0]).z > 1e-12)
                    {
                        for p in samples(tri.p) {
                            let skin =
                                front(&head, p.truncate()).expect("lid has emitted head support");
                            assert!(p.z >= skin - 0.00025, "{kind:?} build {build} glow {glowing} {name}: lid triangle sinks {:.6} into head at {p:?}", skin - p.z);
                            surface_hits += 1;
                        }
                    }
                    assert!(
                        surface_hits >= 32,
                        "missing across-width lid surface witnesses"
                    );
                }
                for sign in [-1.0, 1.0] {
                    let white_side = side(&white, sign);
                    let component = pupil(&eyes, &white_side);
                    let (center, _) = pupil_center_radius(&component);
                    // Both disc layers stay inside the actual emitted opening,
                    // including triangle interiors and their shallow backs.
                    for pigment in [&component, &iris] {
                        for tri in side(pigment, sign) {
                            for p in samples(tri.p) {
                                let white_depth = front(&white, p.truncate())
                                    .unwrap_or_else(|| panic!("{kind:?} build {build}: disc escapes emitted white at {p:?}"));
                                assert!(
                                    p.z >= white_depth - 0.00005,
                                    "{kind:?} build {build}: disc falls behind white at {p:?}"
                                );
                            }
                        }
                    }
                    let (low, high) = bounds(&white_side);
                    let half_width = (high.x - low.x) * 0.5;
                    for (fraction, name, pigment) in [
                        (-0.78, "white", &white),
                        (-0.4, "iris", &iris),
                        (0.0, "pupil", &component),
                        (0.4, "iris", &iris),
                        (0.78, "white", &white),
                    ] {
                        let at = center + Vec2::X * (half_width * fraction);
                        let target = front(pigment, at).unwrap_or_else(|| panic!("{kind:?} build {build} side {sign}: missing {name} probe at {at:?}"));
                        let visible = front(&eyes, at).unwrap().max(front(&head, at).unwrap());
                        assert!(target >= visible - 0.00005, "{kind:?} build {build} glow {glowing} side {sign}: {name} hidden by {:.6} at {at:?}", visible - target);
                    }
                }
            }
        }
    }
}

#[test]
fn art_contract_upper_lash_line_has_visible_width_above_the_opening() {
    for kind in KINDS {
        for build in 0..3 {
            for glowing in [false, true] {
                let (look, eyes, head) = fixture(kind, build, glowing, true);
                let white = painted(&eyes, if glowing { look.skin * 0.3 } else { WHITE });
                let dark = painted(&eyes, DARK);
                for sign in [-1.0, 1.0] {
                    let white_side = side(&white, sign);
                    let component = pupil(&eyes, &white_side);
                    let (center, _) = pupil_center_radius(&component);
                    let (low, high) = bounds(&white_side);
                    let half_width = (high.x - low.x) * 0.5;
                    let opening_edges = boundary(&white_side);
                    for fraction in [-0.4, 0.4] {
                        let x = center.x + half_width * fraction;
                        let y = upper_boundary_at(&opening_edges, x);
                        // These interior positions exclude the isolated outer
                        // lash ticks. Brows are absent from this eye fixture;
                        // also prove that no pupil supplies the dark witness.
                        // A visible band must survive on both sides of the
                        // opening, rather than only at one centerline pixel.
                        for offset in [-0.0014, 0.0, 0.0014] {
                            let at = Vec2::new(x, y + offset);
                            assert!(
                                front(&component, at).is_none(),
                                "eyeliner probe overlaps pupil"
                            );
                            let line = front(&dark, at).unwrap_or_else(|| panic!(
                                "{kind:?} build {build} glow {glowing} side {sign}: no upper dark-line coverage at opening fraction {fraction}, offset={offset:.6}"));
                            let visible = front(&eyes, at).unwrap().max(front(&head, at).unwrap());
                            assert!(line >= visible - 0.00005,
                                "{kind:?} build {build} glow {glowing} side {sign}: upper dark line hidden by {:.6} at opening fraction {fraction}, offset={offset:.6}", visible - line);
                        }
                    }
                }
            }
        }
    }
}
