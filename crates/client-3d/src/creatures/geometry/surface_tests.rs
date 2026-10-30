//! Shape-error and topology witnesses for shared closed limb surfaces.
use super::*;
use bevy::render::mesh::VertexAttributeValues;
use std::collections::BTreeMap;

fn mesh(emit: impl FnOnce(&mut Builder)) -> Mesh {
    let mut parts = Parts::default();
    emit(&mut parts.matte);
    let mut materials = Assets::default();
    let palette = crate::props::PropMaterials::new(&mut materials);
    let mut meshes = Assets::default();
    let model = parts.finish(&mut meshes, &palette);
    assert_eq!(model.0.len(), 1);
    meshes.remove(model.0[0].0.id()).expect("emitted limb mesh")
}

fn triangles(mesh: &Mesh) -> Vec<[Vec3; 3]> {
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        panic!("limb positions");
    };
    mesh.indices()
        .expect("indexed limb")
        .iter()
        .collect::<Vec<_>>()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|tri| std::array::from_fn(|i| Vec3::from_array(positions[tri[i]])))
        .collect()
}

fn assert_surface(triangles: &[[Vec3; 3]], distance: impl Fn(Vec3) -> f32) -> f32 {
    assert!(!triangles.is_empty());
    let mut edges: BTreeMap<_, (usize, i32)> = BTreeMap::new();
    let mut worst = 0.0f32;
    let mut worst_at = Vec3::ZERO;
    let mut volume = 0.0;
    let bits = |p: Vec3| p.to_array().map(|v| if v == 0.0 { 0 } else { v.to_bits() });
    for &[a, b, c] in triangles {
        assert!(a.is_finite() && b.is_finite() && c.is_finite());
        volume += a.dot(b.cross(c)) / 6.0;
        for (start, end) in [(a, b), (b, c), (c, a)] {
            let (x, y) = (bits(start), bits(end));
            assert_ne!(x, y, "collapsed surface edge");
            let (key, direction) = if x < y { ((x, y), 1) } else { ((y, x), -1) };
            let entry = edges.entry(key).or_default();
            entry.0 += 1;
            entry.1 += direction;
            for t in [0.0, 0.25, 0.5, 0.75] {
                let p = start.lerp(end, t);
                let error = distance(p).abs();
                if error > worst {
                    worst = error;
                    worst_at = p;
                }
            }
        }
        let p = (a + b + c) / 3.0;
        let error = distance(p).abs();
        if error > worst {
            worst = error;
            worst_at = p;
        }
    }
    assert!(
        edges
            .values()
            .all(|&(count, winding)| count == 2 && winding == 0),
        "surface must be closed and consistently wound: {:?}",
        edges
            .iter()
            .filter(|(_, &(count, winding))| count != 2 || winding != 0)
            .take(3)
            .collect::<Vec<_>>()
    );
    assert!(volume > 0.0, "surface must face outward");
    assert!(
        worst <= 0.001,
        "surface error {worst} at {worst_at:?} exceeds one millimeter"
    );
    volume
}

fn assert_resolved(mesh: &Mesh, field: &crate::sdf::Sculpt, length: f32, label: &str) {
    let triangles = triangles(mesh);
    let result = std::panic::catch_unwind(|| {
        assert_surface(&triangles, |p| geometric_error_bound(field, p));
        for &[a, b, c] in &triangles {
            let normal = (b - a).cross(c - a).normalize_or_zero();
            let center = (a + b + c) / 3.0;
            assert!(
                field.distance(center + normal * 0.002) > field.distance(center - normal * 0.002),
                "every face must remain outward against its source field"
            );
        }
        assert_radial_silhouette(&triangles, length, field);
    });
    if result.is_err() && std::env::var_os("TOR_3D_SURFACE_DIAGNOSTICS").is_some() {
        let directory =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/quality");
        std::fs::create_dir_all(&directory).expect("surface diagnostic directory");
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("surface diagnostic positions");
        };
        let output = serde_json::json!({ "case": label, "positions": positions,
            "indices": mesh.indices().unwrap().iter().collect::<Vec<_>>() });
        std::fs::write(
            directory.join("surface-failed-mesh.json"),
            output.to_string(),
        )
        .expect("surface diagnostic mesh");
    }
    assert!(result.is_ok(), "resolved surface case: {label}");
}

/// The sculpt's ellipsoid field is an approximation, not a Euclidean distance.
/// Establish an actual zero crossing within one millimeter along a separately
/// estimated normal, then bisect that segment. No mesher grid or Newton step
/// participates in this oracle. A crossing bounds distance conservatively.
fn geometric_error_bound(field: &crate::sdf::Sculpt, p: Vec3) -> f32 {
    let value = field.distance(p);
    if value.abs() < 1e-7 {
        return 0.0;
    }
    let epsilon = 0.0002;
    let gradient = Vec3::new(
        field.distance(p + Vec3::X * epsilon) - field.distance(p - Vec3::X * epsilon),
        field.distance(p + Vec3::Y * epsilon) - field.distance(p - Vec3::Y * epsilon),
        field.distance(p + Vec3::Z * epsilon) - field.distance(p - Vec3::Z * epsilon),
    );
    let direction = gradient.normalize_or_zero() * -value.signum();
    let (mut low, mut high) = (0.0, 0.001);
    if field.distance(p + direction * high).signum() == value.signum() {
        return f32::INFINITY;
    }
    for _ in 0..4 {
        let middle = (low + high) * 0.5;
        if field.distance(p + direction * middle).signum() == value.signum() {
            low = middle;
        } else {
            high = middle;
        }
    }
    high
}

#[test]
fn world_space_oracle_accepts_actual_crossing_and_rejects_bad_cuff_point() {
    let look = Look::with_choice(
        CreatureModel::Human,
        42,
        Some(Variant {
            woman: true,
            build: 1,
            ..default()
        }),
    );
    let body = HumanoidBody::resolve(&look);
    let shape = body.thigh;
    let ease = 0.008 * body.build;
    // Retain the old flattened-ellipsoid counterexample even when the garment
    // recipe moves to a rounder field with the same outer dimensions.
    let mut field = limb_form(shape);
    field.inflate(ease).add(
        crate::sdf::Shape::Ellipsoid(
            Vec3::new(0.0, -0.035, 0.004),
            Vec3::new(
                shape.radius_top + ease + 0.004,
                0.013,
                (shape.radius_top + ease + 0.004) * 1.04,
            ),
        ),
        0.012,
    );
    let p = Vec3::new(0.06992158, -0.03489414, 0.07931442);
    // First validate a true crossing independently along a horizontal ray.
    let (mut low, mut high) = (0.0, 0.25);
    assert!(field.distance(Vec3::new(low, p.y, p.z)) < 0.0);
    assert!(field.distance(Vec3::new(high, p.y, p.z)) > 0.0);
    for _ in 0..24 {
        let middle = (low + high) * 0.5;
        if field.distance(Vec3::new(middle, p.y, p.z)) < 0.0 {
            low = middle;
        } else {
            high = middle;
        }
    }
    assert!(geometric_error_bound(&field, Vec3::new(high, p.y, p.z)) <= 0.001);
    assert!(
        field.distance(p).abs() > 0.001,
        "retain the misleading scalar witness"
    );
    let bound = geometric_error_bound(&field, p);
    assert!(
        !bound.is_finite(),
        "retain rejection of the physically bad cuff point"
    );
}

fn assert_radial_silhouette(triangles: &[[Vec3; 3]], length: f32, field: &crate::sdf::Sculpt) {
    for fraction in [0.1, 0.5, 0.9] {
        let origin = Vec3::new(0.0, -length * fraction, 0.0);
        assert!(field.distance(origin) < 0.0);
        for angle in [0.0, 0.7, 1.6, 2.5, 3.3, 4.4, 5.7] {
            let direction = Vec3::new(f32::cos(angle), 0.0, f32::sin(angle));
            // Bracket the exact composed field independently of its voxel grid.
            let (mut inside, mut outside) = (0.0, 0.4);
            assert!(field.distance(origin + direction * outside) > 0.0);
            for _ in 0..24 {
                let middle = (inside + outside) * 0.5;
                if field.distance(origin + direction * middle) < 0.0 {
                    inside = middle;
                } else {
                    outside = middle;
                }
            }
            let mut hit = f32::INFINITY;
            for &[a, b, c] in triangles {
                let (ab, ac) = (b - a, c - a);
                let cross = direction.cross(ac);
                let determinant = ab.dot(cross);
                if determinant.abs() < 1e-10 {
                    continue;
                }
                let to = origin - a;
                let u = to.dot(cross) / determinant;
                let q = to.cross(ab);
                let v = direction.dot(q) / determinant;
                let t = ac.dot(q) / determinant;
                if u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t >= 0.0 {
                    hit = hit.min(t);
                }
            }
            assert!(
                (hit - outside).abs() <= 0.001,
                "ray silhouette error {} at fraction {fraction}, angle {angle}",
                (hit - outside).abs()
            );
        }
    }
}

#[test]
fn closed_limb_surface_preserves_capsule_shape_without_excessive_triangles() {
    let length = 0.30;
    let radius = 0.07;
    let shape = LimbShape::new(length, radius, radius, 0.0, 0.0);
    let mesh = mesh(|b| sculpted_limb(b, shape, Vec3::ONE));
    let triangles = triangles(&mesh);
    // Independent analytic capsule: closest point on its straight central axis.
    let volume = assert_surface(&triangles, |p| {
        p.distance(Vec3::new(0.0, p.y.clamp(-length, 0.0), 0.0)) - radius
    });
    let exact_volume = std::f32::consts::PI * radius * radius * length
        + 4.0 / 3.0 * std::f32::consts::PI * radius.powi(3);
    assert!((0.96..1.01).contains(&(volume / exact_volume)));
    // The shape, volume and winding controls must pass before the cost guard.
    assert!(
        triangles.len() <= 6_000,
        "closed limb used {} triangles",
        triangles.len()
    );
}

#[test]
fn closed_limb_surfaces_preserve_small_and_large_capsules() {
    for (length, radius) in [(0.12, 0.014), (0.24, 0.035), (0.5, 0.12)] {
        let shape = LimbShape::new(length, radius, radius, 0.0, 0.0);
        let mesh = mesh(|b| sculpted_limb(b, shape, Vec3::ONE));
        assert_surface(&triangles(&mesh), |p| {
            p.distance(Vec3::new(0.0, p.y.clamp(-length, 0.0), 0.0)) - radius
        });
    }
}

#[test]
fn resolved_thigh_and_shin_surfaces_follow_their_composed_fields() {
    for model in [
        CreatureModel::Human,
        CreatureModel::Hobgoblin,
        CreatureModel::Vampire,
    ] {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    model,
                    42,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let body = HumanoidBody::resolve(&look);
                for (part, shape) in [("thigh", body.thigh), ("shin", body.shin)] {
                    let label = format!("{model:?}/woman={woman}/build={build}/{part}");
                    let field = limb_form(shape);
                    let skin = mesh(|b| sculpted_limb(b, shape, Vec3::ONE));
                    assert_resolved(&skin, &field, shape.length, &format!("{label}/skin"));
                    for knee in [false, true] {
                        let ease = 0.008 * body.build;
                        let rolls = if knee {
                            [(-0.035, shape.radius_top + ease + 0.004)]
                        } else {
                            [(-shape.length + 0.03, shape.radius_bottom + ease + 0.005)]
                        };
                        let field = cloth_over(limb_form(shape), ease, &rolls);
                        let cloth = mesh(|b| trouser_leg(b, shape, body.build, knee, Vec3::ONE));
                        assert_resolved(
                            &cloth,
                            &field,
                            shape.length,
                            &format!("{label}/cloth/knee={knee}"),
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn closed_surface_sampling_rejects_nonpositive_and_nonfinite_features() {
    for radius in [0.0, -0.1, f32::NAN, f32::INFINITY] {
        assert!(std::panic::catch_unwind(|| closed_surface_step(radius)).is_err());
    }
    assert!(closed_surface_step(0.014) < 0.005);
    assert_eq!(closed_surface_step(0.2), 0.01);
}

fn offset_shin() -> LimbShape {
    let look = Look::with_choice(
        CreatureModel::Hobgoblin,
        42,
        Some(Variant {
            woman: true,
            build: 2,
            ..default()
        }),
    );
    let shape = HumanoidBody::resolve(&look).shin;
    assert!(shape.contour.is_none() && shape.belly.x > 0.0);
    shape
}

#[test]
fn offset_belly_bounds_include_primitive_extents_and_requested_padding() {
    let shape = offset_shin();
    let apex = shape.belly_center - Vec3::Z * shape.belly.z;
    assert!(
        limb_form(shape).distance(apex) <= 1e-6,
        "the primitive apex belongs to the composed skin"
    );
    let (low, _) = shape.mesh_bounds(0.02);
    assert!(
        low.z <= apex.z - 0.02,
        "mesh bounds {low:?} must include offset belly apex {apex:?} plus padding"
    );
}

#[test]
fn offset_belly_bounds_keep_legacy_skin_meshing_closed() {
    let shape = offset_shin();
    let field = limb_form(shape);
    let (low, high) = shape.mesh_bounds(0.02);
    let mesh = mesh(|b| field.mesh(b, low, high, 0.007, &|_, _| Vec3::ONE));
    assert_resolved(&mesh, &field, shape.length, "legacy 7mm offset shin");
}

#[test]
fn gathered_crests_survive_different_grid_phases() {
    use crate::sdf::{Sculpt, Shape::*};
    for offset in [0.001, 0.004, 0.007] {
        let mut field = Sculpt::default();
        field
            .add(Capsule(Vec3::ZERO, Vec3::NEG_Y * 0.3, 0.07, 0.07), 0.0)
            .inflate(0.008)
            .add(
                Ellipsoid(
                    Vec3::new(0.0, -0.035 + offset, 0.004),
                    Vec3::new(0.11, 0.013, 0.1144),
                ),
                0.012,
            );
        let mesh = mesh(|b| {
            field.mesh_projected(
                b,
                Vec3::new(-0.15, -0.43, -0.15),
                Vec3::new(0.15, 0.15, 0.15),
                0.01,
                &|_, _| Vec3::ONE,
            )
        });
        assert_resolved(&mesh, &field, 0.3, &format!("gather offset={offset}"));
    }
}

#[test]
fn resolved_mage_forearm_keeps_its_shape_without_excessive_triangles() {
    let look = Look::preview_adjusted(CreatureModel::Mage, 7, |body| body.woman = true);
    let body = HumanoidBody::resolve(&look);
    let field = forearm_sleeve_form(body.build, body.arm_length, body.profile.muscle);
    let surface = mesh(|b| {
        organic_forearm(
            b,
            body.build,
            body.arm_length,
            body.profile.muscle,
            false,
            Vec3::ONE,
        )
    });
    let faces = triangles(&surface);
    for &[a, b, c] in &faces {
        for (start, end) in [(a, b), (b, c), (c, a)] {
            for t in [0.0, 0.25, 0.5, 0.75] {
                assert!(geometric_error_bound(&field, start.lerp(end, t)) <= 0.001);
            }
        }
    }
    assert_radial_silhouette(&faces, 0.3 * body.arm_length, &field);
    assert!(
        faces.len() <= 12_000,
        "resolved Mage forearm triangle budget: {}",
        faces.len()
    );
}

#[test]
fn resolved_troll_upper_arms_keep_fidelity_and_bounded_density() {
    let look = Look::with_choice(
        CreatureModel::Troll,
        17,
        Some(Variant {
            woman: false,
            build: 1,
            hair: 2,
            ..default()
        }),
    );
    let body = HumanoidBody::resolve(&look);
    for side in [-1.0, 1.0] {
        let field = upper_arm_form(side, body.build, body.arm_length, body.profile.muscle, 0.94);
        let surface = mesh(|b| {
            organic_upper_arm(
                b,
                side,
                body.build,
                body.arm_length,
                body.profile.muscle,
                true,
                Vec3::ONE,
            )
        });
        let faces = triangles(&surface);
        for &[a, b, c] in &faces {
            for (start, end) in [(a, b), (b, c), (c, a)] {
                for t in [0.0, 0.25, 0.5, 0.75] {
                    assert!(geometric_error_bound(&field, start.lerp(end, t)) <= 0.001);
                }
            }
        }
        assert_radial_silhouette(&faces, 0.3 * body.arm_length, &field);
        assert!(
            faces.len() <= 12_000,
            "resolved Troll upper arm side={side} triangles={}",
            faces.len()
        );
    }
}

#[test]
fn projected_arm_recipes_follow_their_shared_source_fields() {
    let mut density_failures = Vec::new();
    for side in [-1.0, 1.0] {
        for build in [0.8, 1.0, 1.25] {
            for length in [0.8, 1.2] {
                for muscle in [0.8, 1.3] {
                    let label =
                        format!("arm side={side} build={build} length={length} muscle={muscle}");
                    let upper = mesh(|b| {
                        organic_upper_arm(b, side, build, length, muscle, true, Vec3::ONE)
                    });
                    let sleeve =
                        mesh(|b| organic_upper_sleeve(b, side, build, length, muscle, Vec3::ONE));
                    let fields = [
                        (
                            upper,
                            upper_arm_form(side, build, length, muscle, 0.94),
                            "upper bare",
                        ),
                        (
                            sleeve,
                            upper_sleeve_form(side, build, length, muscle),
                            "upper sleeve",
                        ),
                    ];
                    let mut samples = Vec::from(fields);
                    for bare in [false, true] {
                        samples.push((
                            mesh(|b| organic_forearm(b, build, length, muscle, bare, Vec3::ONE)),
                            if bare {
                                forearm_form(build, length, muscle)
                            } else {
                                forearm_sleeve_form(build, length, muscle)
                            },
                            if bare {
                                "forearm bare"
                            } else {
                                "forearm sleeve"
                            },
                        ));
                    }
                    for (surface, field, part) in samples {
                        let faces = triangles(&surface);
                        // Wrist ends may be intentionally buried in the next
                        // joint; the existing joint owners cover that overlap.
                        for &[a, b, c] in &faces {
                            for (start, end) in [(a, b), (b, c), (c, a)] {
                                for t in [0.0, 0.2, 0.4, 0.6, 0.8] {
                                    assert!(
                                        geometric_error_bound(&field, start.lerp(end, t)) <= 0.001,
                                        "shared source field mismatch: {label}/{part} at {:?}",
                                        start.lerp(end, t)
                                    );
                                }
                            }
                        }
                        assert_radial_silhouette(&faces, 0.3 * length, &field);
                        if faces.len() > 20_000 {
                            density_failures.push(format!("{label}/{part}: {}", faces.len()));
                        }
                    }
                }
            }
        }
    }
    assert!(
        density_failures.is_empty(),
        "shared arm triangle budget violations:\n{}",
        density_failures.join("\n")
    );
}
