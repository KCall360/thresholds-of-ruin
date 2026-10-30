use super::*;

fn fixture() -> (PropLibrary, Assets<Mesh>) {
    let mut materials = Assets::<StandardMaterial>::default();
    (
        PropLibrary::new(PropMaterials::new(&mut materials)),
        Assets::default(),
    )
}

fn triangle() -> Parts {
    let mut parts = Parts::default();
    parts.matte.tri([Vec3::ZERO, Vec3::X, Vec3::Y], Vec3::ONE);
    parts
}

#[test]
fn art_contract_idle_cache_is_bounded_without_evicting_visible_geometry() {
    let (mut library, mut meshes) = fixture();
    let visible = library.get("visible".into(), &mut meshes, triangle);
    for i in 0..1100 {
        drop(library.get(format!("idle-{i}"), &mut meshes, triangle));
    }
    assert!(
        // The insertion being returned is live during trimming. After its caller
        // drops it, periodic pruning accounts for that last newly idle model.
        library.cache.len() <= 1026,
        "idle recipe keys must not grow without bound"
    );
    let reused = library.get("visible".into(), &mut meshes, || {
        panic!("live geometry was evicted")
    });
    assert_eq!(visible.0[0].0.id(), reused.0[0].0.id());
}

#[test]
fn art_contract_empty_recipe_keys_also_obey_the_idle_budget() {
    let (mut library, mut meshes) = fixture();
    for i in 0..1100 {
        drop(library.get(format!("empty-{i}"), &mut meshes, Parts::default));
    }
    assert!(library.cache.len() <= 1024);
}

#[test]
fn art_contract_payload_budget_does_not_count_live_geometry_as_idle() {
    let (mut library, mut meshes) = fixture();
    let visible = library.get("visible".into(), &mut meshes, triangle);
    drop(library.get("old".into(), &mut meshes, triangle));
    // Accounting fixture avoids allocating a hundred megabytes to exercise eviction.
    library.cache.get_mut("old").unwrap().payload_bytes = WARM_PAYLOAD_LIMIT + 1;
    library.payload_bytes = library
        .cache
        .values()
        .map(|entry| entry.payload_bytes)
        .sum();
    let usage = library.trim();
    assert_eq!(usage.live_models, 1);
    assert_eq!(usage.warm_models, 0);
    assert!(meshes.contains(visible.0[0].0.id()));
    assert!(!library.cache.contains_key("old"));
}

#[test]
fn outer_surface_finds_outermost_layer_in_recipe_coordinates() {
    for xf in [
        Affine3A::IDENTITY,
        Affine3A::from_scale_rotation_translation(
            Vec3::new(2.0, 0.5, 1.3),
            Quat::from_rotation_y(0.7),
            Vec3::new(3.0, -2.0, 4.0),
        ),
    ] {
        let mut builder = Builder {
            xf,
            ..Builder::default()
        };
        for z in [0.4, 0.2, -0.3] {
            builder.tri(
                [
                    Vec3::new(-1.0, -1.0, z),
                    Vec3::new(1.0, -1.0, z),
                    Vec3::new(0.0, 1.0, z),
                ],
                Vec3::ONE,
            );
        }
        let hit = builder.outer_surface(Vec3::ZERO, Vec3::Z).unwrap();
        assert!(hit.distance(Vec3::Z * 0.4) < 1e-5, "{hit:?}");
        assert!(builder.outer_surface(Vec3::X * 3.0, Vec3::Z).is_none());
        assert!(builder.outer_surface(Vec3::Z, Vec3::Z).is_none());
    }
}

#[test]
fn sword_handle_mesh_centers_share_the_hand_grasp_and_keep_item_size() {
    let mut spans = Vec::new();
    for kind in [Kind::Greatsword, Kind::ShortSword] {
        let parts = build_item(kind, "");
        // The matte sword geometry is its handle and wrapping. Measure the
        // actual emitted handle, independently of the attachment metadata.
        let mut low = Vec3::splat(f32::INFINITY);
        let mut high = Vec3::splat(f32::NEG_INFINITY);
        for point in &parts.matte.positions {
            low = low.min(Vec3::from_array(*point));
            high = high.max(Vec3::from_array(*point));
        }
        let center = (low + high) * 0.5;
        let held = held_transform(kind);
        let grasp = Vec3::new(-0.03, -0.02, 0.0);
        assert!(
            held.transform_point(center).distance(grasp) < 0.0001,
            "{kind:?}: emitted handle misses the shared hand grasp: {:?}",
            held.transform_point(center)
        );
        assert!((held.rotation * Vec3::X).distance(Vec3::Z) < 0.0001);
        assert_eq!(
            held.scale,
            Vec3::ONE,
            "attachment must not rescale the item"
        );
        spans.push(
            held.transform_point(high)
                .distance(held.transform_point(low)),
        );
        // A wrist rotation/translation moves both frames together.
        let wrist = Transform::from_xyz(0.3, 1.1, -0.2).with_rotation(Quat::from_euler(
            EulerRot::XYZ,
            0.4,
            -0.7,
            0.2,
        ));
        assert!(
            wrist
                .transform_point(held.transform_point(center))
                .distance(wrist.transform_point(grasp))
                < 0.0001
        );
    }
    assert!((spans[1] / spans[0] - 0.62).abs() < 0.0001);
}

#[test]
fn nonsword_items_do_not_claim_sword_handle_frames() {
    for kind in [Kind::Mace, Kind::Wand, Kind::RingMail, Kind::Placeholder] {
        assert!(sword_handle_frame(kind).is_none());
        assert_eq!(
            held_transform(kind).translation,
            Vec3::new(0.0, -0.02, 0.29)
        );
    }
}

#[test]
fn smooth_small_tubes_keep_closed_surfaces_without_blanket_subdivision() {
    let mut parts = Parts::default();
    parts.soften();
    parts.matte.tube(
        &[Vec3::ZERO, Vec3::Y * 0.03],
        &[Vec2::splat(0.01); 2],
        10,
        &|_, _, t| Vec3::new(0.2 + 0.6 * t, 0.3, 0.4),
    );
    let mesh = parts.matte.finish().unwrap();
    let VertexAttributeValues::Float32x3(positions) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
    else {
        panic!("tube positions")
    };
    assert!(positions.iter().flatten().all(|v| v.is_finite()));
    let indices: Vec<_> = mesh.indices().unwrap().iter().collect();
    let mut edges = std::collections::HashMap::new();
    let mut volume = 0.0;
    for triangle in indices.as_chunks::<3>().0 {
        let points = triangle.map(|i| Vec3::from(positions[i]));
        volume += points[0].dot(points[1].cross(points[2])) / 6.0;
        let keys = points.map(|p| p.to_array().map(|v| (v * 1e6).round() as i32));
        for i in 0..3 {
            let (a, b) = (keys[i], keys[(i + 1) % 3]);
            let (key, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
            let entry = edges.entry(key).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += sign;
        }
    }
    assert!(
        volume > 1e-7,
        "positive control: tube has outward enclosed volume"
    );
    assert!(
        edges
            .values()
            .all(|&(count, balance)| count == 2 && balance == 0),
        "end caps and seams remain closed"
    );
    let VertexAttributeValues::Float32x4(colors) = mesh.attribute(Mesh::ATTRIBUTE_COLOR).unwrap()
    else {
        panic!("tube colors")
    };
    let red_min = colors.iter().map(|c| c[0]).fold(f32::INFINITY, f32::min);
    let red_max = colors
        .iter()
        .map(|c| c[0])
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(
        red_max - red_min > 0.3,
        "paint still spans both authored ends"
    );
    assert!(
        indices.len() / 3 <= 100,
        "a straight 3cm tube must not need hundreds of triangles just for smooth shading: {}",
        indices.len() / 3
    );
}
